//! tray icon

use dioxus_core::{
    prelude::{provide_context, try_consume_context},
    use_hook,
};

use crate::menubar::DioxusMenu;

/// tray icon icon type trait
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub type DioxusTrayIcon = tray_icon::Icon;
#[cfg(any(target_os = "ios", target_os = "android"))]
pub type DioxusTrayIcon = ();

/// tray icon type trait
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub type DioxusTray = tray_icon::TrayIcon;
#[cfg(any(target_os = "ios", target_os = "android"))]
pub type DioxusTray = ();

/// initializes a tray icon
#[allow(unused)]
pub fn init_tray_icon(menu: DioxusMenu, icon: Option<DioxusTrayIcon>) -> DioxusTray {
    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    {
        let builder = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_icon(match icon {
                Some(value) => value,
                None => crate::default_icon(),
            });

        provide_context(builder.build().expect("tray icon builder failed"))
    }
}

/// Returns a default tray icon menu
pub fn default_tray_icon() -> DioxusMenu {
    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    {
        use muda::PredefinedMenuItem;
        let tray_menu = DioxusMenu::new();
        tray_menu
            .append_items(&[&PredefinedMenuItem::quit(None)])
            .unwrap();
        tray_menu
    }
}

/// Provides a hook to the tray icon
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub fn use_tray_icon() -> Option<tray_icon::TrayIcon> {
    use_hook(try_consume_context)
}
