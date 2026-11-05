//! Slint version of the OCR text selection spike.
//!   select-spike-slint [--script OUT_DIR [--zoom 2]]

slint::include_modules!();

mod app;
mod drive;
mod model;
mod panel;
mod script;
mod vtext;

use slint::ComponentHandle;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let app = AppWindow::new().unwrap();
    let state = app::install(&app);
    app.invoke_focus_keys();
    if let Some(dir) = arg("--script") {
        let zoom: f32 = arg("--zoom").and_then(|z| z.parse().ok()).unwrap_or(1.0);
        script::start(&app, state, std::path::PathBuf::from(dir), zoom);
    }
    app.run().unwrap();
}
