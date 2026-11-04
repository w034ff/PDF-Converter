//! Drives the window with real pointer events, for the tests and the
//! screenshot runs. Positions are page points.

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

use crate::AppWindow;

/// Window coordinates of page point (`x`, `y`).
pub fn at(app: &AppWindow, x: f32, y: f32) -> LogicalPosition {
    LogicalPosition::new(app.get_page_abs_x() + x * app.get_zoom(), app.get_page_abs_y() + y * app.get_zoom())
}

/// Presses at `from`, moves to `to` in eight steps and releases.
pub fn drag(app: &AppWindow, from: (f32, f32), to: (f32, f32)) {
    let w = app.window();
    let start = at(app, from.0, from.1);
    w.dispatch_event(WindowEvent::PointerMoved { position: start });
    w.dispatch_event(WindowEvent::PointerPressed { position: start, button: PointerEventButton::Left });
    for i in 1..=8 {
        let f = i as f32 / 8.0;
        let p = at(app, from.0 + (to.0 - from.0) * f, from.1 + (to.1 - from.1) * f);
        w.dispatch_event(WindowEvent::PointerMoved { position: p });
    }
    let end = at(app, to.0, to.1);
    w.dispatch_event(WindowEvent::PointerReleased { position: end, button: PointerEventButton::Left });
}

pub fn click(app: &AppWindow, p: (f32, f32)) {
    drag(app, p, p);
}
