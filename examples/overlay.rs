//! This example demonstrates how to create an overlay window with dioxus.
//!
//! Basically, we just create a new window with a transparent background and no decorations, size it to the screen, and
//! then we can draw whatever we want on it. In this case, we're drawing a simple overlay with a draggable header.
//!
//! We also add a global shortcut to toggle the overlay on and off, so you could build a raycast-type app with this.

use dioxus::desktop::{use_global_shortcut, winit::dpi::PhysicalPosition, LogicalSize, Window};
use dioxus::mobile::winit::window::WindowLevel;
use dioxus::mobile::WindowAttributes;
use dioxus::prelude::*;

fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(make_config())
        .launch(app);
}

fn app() -> Element {
    let mut show_overlay = use_signal(|| true);

    _ = use_global_shortcut("cmd+g", move || show_overlay.toggle());

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/examples/assets/overlay.css"),
        }
        if show_overlay() {
            div {
                width: "100%",
                height: "100%",
                background_color: "red",
                border: "1px solid black",

                div {
                    width: "100%",
                    height: "10px",
                    background_color: "black",
                    onmousedown: move |_| dioxus::desktop::window().drag(),
                }

                "This is an overlay!"
            }
        }
    }
}

fn make_config() -> dioxus::desktop::Config {
    dioxus::desktop::Config::default().with_window_attributes(make_window())
}

fn make_window() -> WindowAttributes {
    Window::default_attributes()
        .with_transparent(true)
        .with_decorations(false)
        .with_resizable(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_position(PhysicalPosition::new(0, 0))
        .with_max_inner_size(LogicalSize::new(100000, 50))
}
