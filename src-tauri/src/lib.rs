//! Application entry point.

/// Starts the Tauri application.
///
/// # Panics
///
/// Panics if Tauri fails to start, which leaves no window to report the error in.
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("Tauri application should start");
}
