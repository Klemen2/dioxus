use crate::{menubar::DioxusMenuIcon, DioxusTrayIcon};

// TODO only implemented for windows, needs implementation for other platforms

pub trait DefaultIcon {
    fn get_icon() -> Self
    where
        Self: Sized;
}

#[cfg(not(target_os = "windows"))]
static ICON: &'static [u8; N] = include_bytes!("./assets/default_icon.bin");

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl DefaultIcon for DioxusTrayIcon {
    fn get_icon() -> Self
    where
        Self: Sized,
    {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let default = DioxusTrayIcon::from_rgba(ICON.to_vec(), 460, 460);
        #[cfg(target_os = "windows")]
        let default = DioxusTrayIcon::from_resource(32512, None);

        default.expect("image parse failed")
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl DefaultIcon for DioxusMenuIcon {
    fn get_icon() -> Self
    where
        Self: Sized,
    {
        #[cfg(not(any(target_os = "ios", target_os = "android", target_os = "windows")))]
        let default = DioxusMenuIcon::from_rgba(ICON.to_vec(), 460, 460);
        #[cfg(target_os = "windows")]
        let default = DioxusMenuIcon::from_resource(32512, None);

        default.expect("image parse failed")
    }
}

impl DefaultIcon for winit::window::Icon {
    fn get_icon() -> Self
    where
        Self: Sized,
    {
        #[cfg(not(target_os = "windows"))]
        let default = winit::window::Icon::from_rgba(
            include_bytes!("./assets/default_icon.bin").to_vec(),
            460,
            460,
        );
        #[cfg(target_os = "windows")]
        use winit::platform::windows::IconExtWindows;

        #[cfg(target_os = "windows")]
        let default = winit::window::Icon::from_resource(32512, None);

        default.expect("image parse failed")
    }
}

/// Provides the default icon of the app
pub fn default_icon<T: DefaultIcon>() -> T {
    T::get_icon()
}
