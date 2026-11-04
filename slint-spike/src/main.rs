//! The Slint crop spike. Usage:
//!   slint-crop-spike [file.pdf] [--dpi N] [--screenshot DIR | --bench]

use slint::ComponentHandle;
use slint_crop_spike::{AppWindow, app};

mod shots;

fn main() {
    if let Some(code) = slint_crop_spike::pdf::run_worker_if_asked() {
        std::process::exit(code);
    }
    let mut pdf = None;
    let mut dpi = 150u32;
    let mut shots_dir = None;
    let mut bench = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dpi" => dpi = args.next().and_then(|s| s.parse().ok()).unwrap_or(dpi),
            "--screenshot" => shots_dir = args.next(),
            "--bench" => bench = true,
            _ => pdf = Some(std::path::PathBuf::from(a)),
        }
    }
    let app = AppWindow::new().unwrap();
    app::install(&app, dpi);
    install_file_drop(&app);
    if let Some(path) = pdf {
        let weak = app.as_weak();
        // Opening needs the event loop for its answers, so it waits for it.
        slint::Timer::single_shot(std::time::Duration::from_millis(50), move || {
            if let Some(app) = weak.upgrade() {
                app.invoke_fit_width();
            }
            app::with(|c| c.open(&path));
        });
    }
    if bench {
        shots::bench(&app);
    }
    if let Some(dir) = shots_dir {
        shots::start(&app, std::path::PathBuf::from(dir));
    }
    app.run().unwrap();
}

/// OS file drops reach Slint's `DropArea` only from inside the app, so the
/// winit event is taken directly (needs the `unstable-winit-030` feature).
fn install_file_drop(app: &AppWindow) {
    use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
    app.window().on_winit_window_event(|_, event| {
        if let winit::event::WindowEvent::DroppedFile(path) = event {
            let path = path.clone();
            app::with(|c| c.open(&path));
            return EventResult::PreventDefault;
        }
        EventResult::Propagate
    });
}
