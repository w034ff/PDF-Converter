//! Drives the window with real pointer and key events (from slint-spike/src/drive.rs).

use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

use crate::AppWindow;

/// Window coordinates of page point (`x`, `y`).
pub fn at(app: &AppWindow, x: f32, y: f32) -> LogicalPosition {
    LogicalPosition::new(app.get_page_abs_x() + x * app.get_scale(), app.get_page_abs_y() + y * app.get_scale())
}

pub fn press(app: &AppWindow, p: LogicalPosition) {
    let w = app.window();
    w.dispatch_event(WindowEvent::PointerMoved { position: p });
    w.dispatch_event(WindowEvent::PointerPressed { position: p, button: PointerEventButton::Left });
}
pub fn move_to(app: &AppWindow, p: LogicalPosition) {
    app.window().dispatch_event(WindowEvent::PointerMoved { position: p });
}
pub fn release(app: &AppWindow, p: LogicalPosition) {
    app.window().dispatch_event(WindowEvent::PointerReleased { position: p, button: PointerEventButton::Left });
}

pub fn key_down(app: &AppWindow, text: slint::SharedString) {
    app.window().dispatch_event(WindowEvent::KeyPressed { text });
}
pub fn key_up(app: &AppWindow, text: slint::SharedString) {
    app.window().dispatch_event(WindowEvent::KeyReleased { text });
}

/// Ctrl + `c`.
pub fn ctrl(app: &AppWindow, c: char) {
    key_down(app, Key::Control.into());
    key_down(app, c.to_string().into());
    key_up(app, c.to_string().into());
    key_up(app, Key::Control.into());
}
