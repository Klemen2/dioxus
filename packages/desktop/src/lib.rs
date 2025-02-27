#![doc = include_str!("readme.md")]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/79236386")]
#![doc(html_favicon_url = "https://avatars.githubusercontent.com/u/79236386")]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod android_sync_lock;
mod app;
mod assets;
mod config;
mod default_icon;
mod desktop_context;
mod document;
mod edits;
mod element;
mod event_handlers;
mod events;
mod file_upload;
mod hooks;
mod ipc;
mod menubar;
mod protocol;
mod query;
mod shortcut;
mod trayicon;
mod waker;
mod webview;

pub use default_icon::default_icon;

// mobile shortcut is only supported on mobile platforms
#[cfg(any(target_os = "ios", target_os = "android"))]
mod mobile_shortcut;

/// The main entrypoint for this crate
pub mod launch;

// Reexport tao and wry, might want to re-export other important things
pub use winit;
pub use winit::dpi::{LogicalPosition, LogicalSize};
pub use winit::event::WindowEvent;
pub use winit::window::{Window, WindowAttributes};
pub use wry;
// Reexport muda only if we are on desktop platforms that support menus
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use muda;

// Tray icon
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub use trayicon::{default_tray_icon, init_tray_icon, use_tray_icon, DioxusTray, DioxusTrayIcon};

// Public exports
pub use assets::AssetRequest;
pub use config::{Config, DefaultWindowCloseBehaviour, WindowCloseBehaviour};
pub use desktop_context::{
    window, DesktopContext, DesktopService, MaybeDesktopContext, WeakDesktopContext,
};
pub use event_handlers::WryEventHandler;
pub use hooks::*;
pub use shortcut::{ShortcutHandle, ShortcutRegistryError};
pub use wry::RequestAsyncResponder;
