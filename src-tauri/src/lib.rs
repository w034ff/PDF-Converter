//! Application entry point.

pub mod commands;
pub mod pdfium;

/// Starts the Tauri application.
///
/// # Panics
///
/// Panics if Tauri fails to start, which leaves no window to report the error in.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::get_about])
        .run(tauri::generate_context!())
        .expect("Tauri application should start");
}
