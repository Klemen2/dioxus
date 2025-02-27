use crate::{
    config::{Config, CustomEventHandler, DefaultWindowCloseBehaviour, WindowCloseBehaviour},
    event_handlers::WindowEventHandlers,
    file_upload::{DesktopFileUploadForm, FileDialogRequest, NativeFileEngine},
    ipc::{IpcMessage, IpcMethod, UserWindowEvent},
    query::QueryResult,
    shortcut::ShortcutRegistry,
    webview::WebviewInstance,
    DesktopService, WeakDesktopContext,
};
use dioxus_core::{ElementId, ScopeId, VirtualDom};
use dioxus_document::eval;
use dioxus_html::PlatformEventData;
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};
use tokio::sync::oneshot::Sender;
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId},
};

/// The single top-level object that manages all the running windows, assets, shortcuts, etc
pub(crate) struct App {
    // move the props into a cell so we can pop it out later to create the first window
    // iOS panics if we create a window before the event loop is started, so we toss them into a cell
    pub(crate) unmounted_dom: Cell<Option<VirtualDom>>,
    pub(crate) cfg: Cell<Option<Config>>,

    // Stuff we need mutable access to
    pub(crate) control_flow: AppControlFlow,
    pub(crate) is_visible_before_start: bool,
    pub(crate) default_window_close_behavior: DefaultWindowCloseBehaviour,
    pub(crate) webviews: HashMap<WindowId, WebviewInstance>,
    pub(crate) float_all: bool,
    pub(crate) show_devtools: bool,
    pub(crate) custom_event_handler: Option<CustomEventHandler>,

    /// This single blob of state is shared between all the windows so they have access to the runtime state
    ///
    /// This includes stuff like the event handlers, shortcuts, etc as well as ways to modify *other* windows
    pub(crate) shared: Rc<SharedContext>,
}

/// A bundle of state shared between all the windows, providing a way for us to communicate with running webview.
pub(crate) struct SharedContext {
    pub(crate) event_handlers: WindowEventHandlers,
    pub(crate) pending_windows: RefCell<Vec<(VirtualDom, Config, Sender<WeakDesktopContext>)>>,
    pub(crate) shortcut_manager: ShortcutRegistry,
    pub(crate) proxy: EventLoopProxy<UserWindowEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AppControlFlow {
    Wait,
    Exit,
}

impl App {
    pub fn new(
        proxy: EventLoopProxy<UserWindowEvent>,
        mut cfg: Config,
        virtual_dom: VirtualDom,
    ) -> Self {
        let app = Self {
            default_window_close_behavior: cfg.default_window_close_behaviour,
            is_visible_before_start: true,
            webviews: HashMap::new(),
            control_flow: AppControlFlow::Wait,
            unmounted_dom: Cell::new(Some(virtual_dom)),
            float_all: false,
            show_devtools: false,
            custom_event_handler: cfg.custom_event_handler.take(),
            cfg: Cell::new(Some(cfg)),
            shared: Rc::new(SharedContext {
                event_handlers: WindowEventHandlers::default(),
                pending_windows: Default::default(),
                shortcut_manager: ShortcutRegistry::new(),
                proxy,
            }),
        };

        // Set the event converter
        dioxus_html::set_event_converter(Box::new(crate::events::SerializedHtmlEventConverter));

        // Wire up the global hotkey handler
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
        app.set_global_hotkey_handler();

        // Wire up the menubar receiver - this way any component can key into the menubar actions
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
        app.set_menubar_receiver();

        // Wire up the tray icon receiver - this way any component can key into the menubar actions
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
        app.set_tray_icon_receiver();

        // Allow hotreloading to work - but only in debug mode
        #[cfg(all(feature = "devtools", debug_assertions))]
        app.connect_hotreload();

        #[cfg(debug_assertions)]
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
        app.connect_preserve_window_state_handler();

        app
    }

    pub fn handle_event(&mut self, event_loop: &ActiveEventLoop, event: Event<UserWindowEvent>) {
        self.control_flow = AppControlFlow::Wait;
        self.shared.event_handlers.apply_event(&event, event_loop);

        if let Some(ref mut f) = self.custom_event_handler {
            f(&event, event_loop)
        }

        match event {
            Event::LoopExiting => self.handle_loop_exiting(),
            Event::WindowEvent { window_id, event } => match event {
                WindowEvent::CloseRequested => self.handle_close_requested(window_id),
                WindowEvent::Destroyed { .. } => self.window_destroyed(window_id),
                WindowEvent::Resized(new_size) => self.resize_window(window_id, &new_size),
                _ => {}
            },
            Event::UserEvent(event) => match event {
                UserWindowEvent::Poll(id) => self.poll_vdom(id),
                UserWindowEvent::NewWindow => self.handle_new_windows(event_loop),
                UserWindowEvent::CloseWindow(id) => self.handle_close_msg(id),
                UserWindowEvent::Shutdown => self.control_flow = AppControlFlow::Exit,
                #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
                UserWindowEvent::GlobalHotKeyEvent(evnt) => self.handle_global_hotkey(evnt),
                #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
                UserWindowEvent::MudaMenuEvent(evnt) => self.handle_menu_event(evnt),
                #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
                UserWindowEvent::TrayIconEvent(evnt) => self.handle_tray_icon_event(evnt),
                #[cfg(all(feature = "devtools", debug_assertions))]
                UserWindowEvent::HotReloadEvent(msg) => self.handle_hot_reload_msg(msg),
                UserWindowEvent::WindowsDragDrop(id) => {
                    if let Some(webview) = self.webviews.get(&id) {
                        webview.dom.in_runtime(|| {
                            ScopeId::ROOT.in_runtime(|| {
                                eval("window.interpreter.handleWindowsDragDrop();");
                            });
                        });
                    }
                }
                UserWindowEvent::WindowsDragLeave(id) => {
                    if let Some(webview) = self.webviews.get(&id) {
                        webview.dom.in_runtime(|| {
                            ScopeId::ROOT.in_runtime(|| {
                                eval("window.interpreter.handleWindowsDragLeave();");
                            });
                        });
                    }
                }
                UserWindowEvent::WindowsDragOver(id, x_pos, y_pos) => {
                    if let Some(webview) = self.webviews.get(&id) {
                        webview.dom.in_runtime(|| {
                            ScopeId::ROOT.in_runtime(|| {
                                let e = eval(
                                    r#"
                                    const xPos = await dioxus.recv();
                                    const yPos = await dioxus.recv();
                                    window.interpreter.handleWindowsDragOver(xPos, yPos)
                                    "#,
                                );

                                _ = e.send(x_pos);
                                _ = e.send(y_pos);
                            });
                        });
                    }
                }
                UserWindowEvent::Ipc { id, msg } => match msg.method() {
                    IpcMethod::Initialize => self.handle_initialize_msg(id),
                    IpcMethod::FileDialog => self.handle_file_dialog_msg(msg, id),
                    IpcMethod::UserEvent => {}
                    IpcMethod::Query => self.handle_query_msg(msg, id),
                    IpcMethod::BrowserOpen => self.handle_browser_open(msg),
                    IpcMethod::Other(_) => {}
                },
                UserWindowEvent::CloseBehaviour(id, behaviour) => {
                    self.change_window_close_behaviour(id, behaviour)
                }
            },
            _ => {}
        }

        let winit_control_flow = match self.control_flow {
            AppControlFlow::Wait => ControlFlow::Wait,
            AppControlFlow::Exit => {
                event_loop.exit();
                return;
            }
        };
        event_loop.set_control_flow(winit_control_flow);
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    pub fn handle_global_hotkey(&self, event: global_hotkey::GlobalHotKeyEvent) {
        self.shared.shortcut_manager.call_handlers(event);
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    pub fn handle_menu_event(&mut self, event: muda::MenuEvent) {
        use winit::window::WindowLevel;

        match dbg!(event.id().0.as_str()) {
            "dioxus-float-top" => {
                for webview in self.webviews.values() {
                    let window_level = match self.float_all {
                        true => WindowLevel::AlwaysOnTop,
                        false => WindowLevel::Normal,
                    };

                    webview
                        .desktop_context
                        .window
                        .set_window_level(window_level);
                }
                self.float_all = !self.float_all;
            }
            "dioxus-toggle-dev-tools" => {
                self.show_devtools = !self.show_devtools;
                for webview in self.webviews.values() {
                    let wv = &webview.desktop_context.webview;
                    if self.show_devtools {
                        wv.open_devtools();
                    } else {
                        wv.close_devtools();
                    }
                }
            }
            _ => (),
        }
    }
    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    pub fn handle_tray_menu_event(&mut self, event: tray_icon::menu::MenuEvent) {
        _ = event;
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    pub fn handle_tray_icon_event(&mut self, event: tray_icon::TrayIconEvent) {
        if let tray_icon::TrayIconEvent::Click {
            id: _,
            position: _,
            rect: _,
            button,
            button_state: _,
        } = event
        {
            if button == tray_icon::MouseButton::Left {
                for webview in self.webviews.values() {
                    webview.desktop_context.window.set_visible(true);
                    webview.desktop_context.window.focus_window();
                }
            }
        }
    }

    #[cfg(all(feature = "devtools", debug_assertions))]
    pub fn connect_hotreload(&self) {
        if let Some(endpoint) = dioxus_cli_config::devserver_ws_endpoint() {
            let proxy = self.shared.proxy.clone();
            dioxus_devtools::connect(endpoint, move |msg| {
                _ = proxy.send_event(UserWindowEvent::HotReloadEvent(msg));
            })
        }
    }

    pub fn handle_new_windows(&mut self, event_loop: &ActiveEventLoop) {
        let mut pending_windows = self.shared.pending_windows.borrow_mut();

        for (dom, cfg, sender) in pending_windows.drain(..) {
            // Create window
            let window_attributes = cfg.window_attributes.clone();
            let window = Self::create_window(window_attributes, event_loop);

            // Create webview
            let webview = WebviewInstance::new(cfg, window, dom, self.shared.clone());

            // Send the desktop context to the MaybeDesktopService
            let cx = webview.dom.in_runtime(|| {
                ScopeId::ROOT
                    .consume_context::<Rc<DesktopService>>()
                    .unwrap()
            });
            let _ = sender.send(Rc::downgrade(&cx));

            // Send first poll event
            let id = webview.desktop_context.window.id();
            self.webviews.insert(id, webview);
            _ = self.shared.proxy.send_event(UserWindowEvent::Poll(id));
        }
    }

    fn create_window(mut attributes: WindowAttributes, event_loop: &ActiveEventLoop) -> Window {
        // Make the windows bigger on desktop
        //
        // on mobile, we want them to be `None` so winit makes them the size of the screen. Otherwise we
        // get a window that is not the size of the screen and weird black bars.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            if attributes.inner_size.is_none() {
                attributes = attributes.with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0));
            }
        }

        // We assume that if the icon is None in cfg, then the user just didnt set it
        if attributes.window_icon.is_none() {
            attributes = attributes.with_window_icon(Some(crate::default_icon()));
        }

        let window = event_loop.create_window(attributes).unwrap();

        // https://developer.apple.com/documentation/appkit/nswindowcollectionbehavior/nswindowcollectionbehaviormanaged
        #[cfg(target_os = "macos")]
        {
            // use cocoa::appkit::NSWindowCollectionBehavior;
            // use cocoa::base::id;
            // use objc::{msg_send, sel, sel_impl};
            // use tao::platform::macos::WindowExtMacOS;

            // unsafe {
            //     let window: id = window.ns_window() as id;
            //     #[allow(unexpected_cfgs)]
            //     let _: () = msg_send![window, setCollectionBehavior: NSWindowCollectionBehavior::NSWindowCollectionBehaviorManaged];
            // }
        }

        window
    }

    pub fn change_window_close_behaviour(
        &mut self,
        id: WindowId,
        behaviour: Option<WindowCloseBehaviour>,
    ) {
        if let Some(webview) = self.webviews.get_mut(&id) {
            webview.close_behaviour = behaviour
        }
    }

    pub fn handle_close_requested(&mut self, id: WindowId) {
        use DefaultWindowCloseBehaviour::*;
        use WindowCloseBehaviour::*;

        let mut remove = false;

        if let Some(webview) = self.webviews.get(&id) {
            if let Some(close_behaviour) = &webview.close_behaviour {
                match close_behaviour {
                    WindowExitsApp => {
                        self.control_flow = AppControlFlow::Exit;
                        return;
                    }
                    WindowHides => {
                        hide_window(&webview.desktop_context.window);
                        return;
                    }
                    WindowCloses => {
                        remove = true;
                    }
                }
            }
        }

        // needed in case of `default_window_close_behavior WindowsHides | LastWindowHides` since they may not remove a window on `WindowCloses`
        if remove {
            #[cfg(debug_assertions)]
            self.persist_window_state();

            self.webviews.remove(&id);
            if matches!(self.default_window_close_behavior, LastWindowExitsApp)
                && self.webviews.is_empty()
            {
                self.control_flow = AppControlFlow::Exit
            }
            return;
        }

        match self.default_window_close_behavior {
            LastWindowExitsApp => {
                #[cfg(debug_assertions)]
                self.persist_window_state();

                self.webviews.remove(&id);
                if self.webviews.is_empty() {
                    self.control_flow = AppControlFlow::Exit
                }
            }

            LastWindowHides if self.webviews.len() > 1 => {
                self.webviews.remove(&id);
            }

            WindowsHides | LastWindowHides => {
                if let Some(webview) = self.webviews.get(&id) {
                    hide_window(&webview.desktop_context.window);
                }
            }

            WindowsCloses => {
                self.webviews.remove(&id);
            }
        }
    }

    pub fn window_destroyed(&mut self, id: WindowId) {
        self.webviews.remove(&id);

        if matches!(
            self.default_window_close_behavior,
            DefaultWindowCloseBehaviour::LastWindowExitsApp
        ) && self.webviews.is_empty()
        {
            self.control_flow = AppControlFlow::Exit;
        }
    }

    pub fn resize_window(&self, id: WindowId, size: &PhysicalSize<u32>) {
        // TODO: the app layer should avoid directly manipulating the webview webview instance internals.
        // Window creation and modification is the responsibility of the webview instance so it makes sense to
        // encapsulate that there.
        if let Some(webview) = self.webviews.get(&id) {
            use wry::Rect;

            _ = webview.desktop_context.webview.set_bounds(Rect {
                position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                    size.width,
                    size.height,
                )),
            });
        }
    }

    pub fn handle_app_resume(&mut self, event_loop: &ActiveEventLoop) {
        let virtual_dom = self
            .unmounted_dom
            .take()
            .expect("Virtualdom should be set before initialization");
        let mut cfg = self
            .cfg
            .take()
            .expect("Config should be set before initialization");

        self.is_visible_before_start = cfg.window_attributes.visible;
        cfg.window_attributes = cfg.window_attributes.with_visible(false);
        let explicit_window_size = cfg.window_attributes.inner_size;
        let explicit_window_position = cfg.window_attributes.position;

        let window = Self::create_window(cfg.window_attributes.clone(), event_loop);
        let webview = WebviewInstance::new(cfg, window, virtual_dom, self.shared.clone());

        // And then attempt to resume from state
        self.resume_from_state(&webview, explicit_window_size, explicit_window_position);

        let id = webview.desktop_context.window.id();
        self.webviews.insert(id, webview);
    }

    pub fn handle_browser_open(&mut self, msg: IpcMessage) {
        if let Some(temp) = msg.params().as_object() {
            if temp.contains_key("href") {
                if let Some(href) = temp.get("href").and_then(|v| v.as_str()) {
                    if let Err(e) = webbrowser::open(href) {
                        tracing::error!("Open Browser error: {:?}", e);
                    }
                }
            }
        }
    }

    /// The webview is finally loaded
    ///
    /// Let's rebuild it and then start polling it
    pub fn handle_initialize_msg(&mut self, id: WindowId) {
        let view = self.webviews.get_mut(&id).unwrap();

        view.edits
            .wry_queue
            .with_mutation_state_mut(|f| view.dom.rebuild(f));

        view.edits.wry_queue.send_edits();

        view.desktop_context
            .window
            .set_visible(self.is_visible_before_start);

        _ = self.shared.proxy.send_event(UserWindowEvent::Poll(id));
    }

    /// Todo: maybe we should poll the virtualdom asking if it has any final actions to apply before closing the webview
    ///
    /// Technically you can handle this with the use_window_event hook
    pub fn handle_close_msg(&mut self, id: WindowId) {
        self.webviews.remove(&id);
        if self.webviews.is_empty() {
            self.control_flow = AppControlFlow::Exit
        }
    }

    pub fn handle_query_msg(&mut self, msg: IpcMessage, id: WindowId) {
        let Ok(result) = serde_json::from_value::<QueryResult>(msg.params()) else {
            return;
        };

        let Some(view) = self.webviews.get(&id) else {
            return;
        };

        view.desktop_context.query.send(result);
    }

    #[cfg(all(feature = "devtools", debug_assertions))]
    pub fn handle_hot_reload_msg(&mut self, msg: dioxus_devtools::DevserverMsg) {
        use dioxus_devtools::DevserverMsg;

        match msg {
            DevserverMsg::HotReload(hr_msg) => {
                for webview in self.webviews.values_mut() {
                    dioxus_devtools::apply_changes(&webview.dom, &hr_msg);
                    webview.poll_vdom();
                }

                if !hr_msg.assets.is_empty() {
                    for webview in self.webviews.values_mut() {
                        webview.kick_stylsheets();
                    }
                }
            }
            DevserverMsg::FullReloadCommand
            | DevserverMsg::FullReloadStart
            | DevserverMsg::FullReloadFailed => {
                // usually only web gets this message - what are we supposed to do?
                // Maybe we could just binary patch ourselves in place without losing window state?
            }
            DevserverMsg::Shutdown => {
                self.control_flow = AppControlFlow::Exit;
            }
        }
    }

    pub fn handle_file_dialog_msg(&mut self, msg: IpcMessage, window: WindowId) {
        let Ok(file_dialog) = serde_json::from_value::<FileDialogRequest>(msg.params()) else {
            return;
        };

        let id = ElementId(file_dialog.target);
        let event_name = &file_dialog.event;
        let event_bubbles = file_dialog.bubbles;
        let files = file_dialog.get_file_event();

        let as_any = Box::new(DesktopFileUploadForm {
            files: Arc::new(NativeFileEngine::new(files)),
        });

        let data = Rc::new(PlatformEventData::new(as_any));

        let Some(view) = self.webviews.get_mut(&window) else {
            return;
        };

        let event = dioxus_core::Event::new(data as Rc<dyn Any>, event_bubbles);

        let runtime = view.dom.runtime();
        if event_name == "change&input" {
            runtime.handle_event("input", event.clone(), id);
            runtime.handle_event("change", event, id);
        } else {
            runtime.handle_event(event_name, event, id);
        }
    }

    /// Poll the virtualdom until it's pending
    ///
    /// The waker we give it is connected to the event loop, so it will wake up the event loop when it's ready to be polled again
    ///
    /// All IO is done on the tokio runtime we started earlier
    pub fn poll_vdom(&mut self, id: WindowId) {
        let Some(view) = self.webviews.get_mut(&id) else {
            return;
        };

        view.poll_vdom();
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    fn set_global_hotkey_handler(&self) {
        let receiver = self.shared.proxy.clone();

        // The event loop becomes the hotkey receiver
        // This means we don't need to poll the receiver on every tick - we just get the events as they come in
        // This is a bit more efficient than the previous implementation, but if someone else sets a handler, the
        // receiver will become inert.
        global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(move |t| {
            // todo: should we unset the event handler when the app shuts down?
            _ = receiver.send_event(UserWindowEvent::GlobalHotKeyEvent(t));
        }));
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    fn set_menubar_receiver(&self) {
        let receiver = self.shared.proxy.clone();

        // The event loop becomes the menu receiver
        // This means we don't need to poll the receiver on every tick - we just get the events as they come in
        // This is a bit more efficient than the previous implementation, but if someone else sets a handler, the
        // receiver will become inert.
        muda::MenuEvent::set_event_handler(Some(move |t| {
            // todo: should we unset the event handler when the app shuts down?
            _ = receiver.send_event(UserWindowEvent::MudaMenuEvent(t));
        }));
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    fn set_tray_icon_receiver(&self) {
        let receiver = self.shared.proxy.clone();

        // The event loop becomes the menu receiver
        // This means we don't need to poll the receiver on every tick - we just get the events as they come in
        // This is a bit more efficient than the previous implementation, but if someone else sets a handler, the
        // receiver will become inert.
        tray_icon::TrayIconEvent::set_event_handler(Some(move |t| {
            // todo: should we unset the event handler when the app shuts down?
            _ = receiver.send_event(UserWindowEvent::TrayIconEvent(t));
        }));

        // for whatever reason they had to make it separate
        let receiver = self.shared.proxy.clone();
        tray_icon::menu::MenuEvent::set_event_handler(Some(move |t| {
            // todo: should we unset the event handler when the app shuts down?
            _ = receiver.send_event(UserWindowEvent::MudaMenuEvent(t));
        }));
    }

    /// Do our best to preserve state about the window when the event loop is exiting
    ///
    /// This will attempt to save the window position, size, and monitor into the environment before
    /// closing. This way, when the app is restarted, it can attempt to restore the window to the same
    /// position and size it was in before, making a better DX.
    pub(crate) fn handle_loop_exiting(&self) {
        #[cfg(debug_assertions)]
        self.persist_window_state();
    }

    #[cfg(debug_assertions)]
    fn persist_window_state(&self) {
        if let Some(webview) = self.webviews.values().next() {
            let window = &webview.desktop_context.window;

            let Some(monitor) = window.current_monitor() else {
                return;
            };

            let Ok(position) = window.outer_position() else {
                return;
            };

            let size = window.outer_size();

            let x = position.x;
            let y = position.y;

            // This is to work around a bug in how tao handles inner_size on macOS
            // We *want* to use inner_size, but that's currently broken, so we use outer_size instead and then an adjustment
            //
            // https://github.com/tauri-apps/tao/issues/889
            let adjustment = match window.is_decorated() {
                true if cfg!(target_os = "macos") => 56,
                _ => 0,
            };

            let Some(monitor_name) = monitor.name() else {
                return;
            };

            let state = PreservedWindowState {
                x,
                y,
                width: size.width.max(200),
                height: size.height.saturating_sub(adjustment).max(200),
                monitor: monitor_name.to_string(),
            };

            // Yes... I know... we're loading a file that might not be ours... but it's a debug feature
            if let Ok(state) = serde_json::to_string(&state) {
                _ = std::fs::write(restore_file(), state);
            }
        }
    }

    // Write this to the target dir so we can pick back up
    fn resume_from_state(
        &mut self,
        webview: &WebviewInstance,
        explicit_inner_size: Option<winit::dpi::Size>,
        explicit_window_position: Option<winit::dpi::Position>,
    ) {
        // We only want to do this on desktop
        if cfg!(target_os = "android") || cfg!(target_os = "ios") {
            return;
        }

        // We only want to do this in debug mode
        if !cfg!(debug_assertions) {
            return;
        }

        if let Ok(state) = std::fs::read_to_string(restore_file()) {
            if let Ok(state) = serde_json::from_str::<PreservedWindowState>(&state) {
                let window = &webview.desktop_context.window;
                let position = (state.x, state.y);
                let size = (state.width, state.height);

                // Only set the outer position if it wasn't explicitly set
                if explicit_window_position.is_none() {
                    window.set_outer_position(winit::dpi::PhysicalPosition::new(
                        position.0, position.1,
                    ));
                }

                // Only set the inner size if it wasn't explicitly set
                if explicit_inner_size.is_none() {
                    let _ =
                        window.request_inner_size(winit::dpi::PhysicalSize::new(size.0, size.1));
                }
            }
        }
    }

    /// Wire up a receiver to sigkill that lets us preserve the window state
    /// Whenever sigkill is sent, we shut down the app and save the window state
    #[cfg(debug_assertions)]
    fn connect_preserve_window_state_handler(&self) {
        // TODO: make this work on windows
        #[cfg(unix)]
        {
            // Wire up the trap
            let target = self.shared.proxy.clone();
            std::thread::spawn(move || {
                use signal_hook::consts::{SIGINT, SIGTERM};
                let sigkill = signal_hook::iterator::Signals::new([SIGTERM, SIGINT]);
                if let Ok(mut sigkill) = sigkill {
                    for _ in sigkill.forever() {
                        if target.send_event(UserWindowEvent::Shutdown).is_err() {
                            std::process::exit(0);
                        }

                        // give it a moment for the event to be processed
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
            });
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PreservedWindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    monitor: String,
}

/// Hides a window.
///
/// On macOS, if we use `set_visibility(false)` on the window, it will hide the window but not show
/// it again when the user switches back to the app. `NSApplication::hide:` has the correct behaviour,
/// so we need to special case it.
#[allow(unused)]
fn hide_window(window: &Window) {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        window.set_visible(false);
    }

    #[cfg(target_os = "macos")]
    {
        // window.set_visible(false); has the wrong behaviour on macOS
        // It will hide the window but not show it again when the user switches
        // back to the app. `NSApplication::hide:` has the correct behaviour
        use objc::runtime::Object;
        use objc::{msg_send, sel, sel_impl};
        #[allow(unexpected_cfgs)]
        objc::rc::autoreleasepool(|| unsafe {
            let app: *mut Object = msg_send![objc::class!(NSApplication), sharedApplication];
            let nil = std::ptr::null_mut::<Object>();
            let _: () = msg_send![app, hide: nil];
        });
    }
}

/// Return the location of a tempfile with our window state in it such that we can restore it later
fn restore_file() -> std::path::PathBuf {
    let dir = dioxus_cli_config::session_cache_dir().unwrap_or_else(std::env::temp_dir);
    dir.join("window-state.json")
}
