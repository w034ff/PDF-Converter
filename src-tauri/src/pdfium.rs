//! Where the bundled pdfium lives (design §8).

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// Folder under the resource directory that holds the pdfium library; it
/// matches the targets in `tauri.linux.conf.json` and `tauri.windows.conf.json`.
const PDFIUM_RESOURCE_DIR: &str = "pdfium";

/// The folder holding the bundled pdfium library, passed to each worker.
///
/// # Errors
///
/// Returns Tauri's error when the resource directory cannot be resolved.
pub fn library_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().resource_dir()?.join(PDFIUM_RESOURCE_DIR))
}
