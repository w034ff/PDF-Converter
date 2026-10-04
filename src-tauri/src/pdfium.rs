//! Where the bundled pdfium lives and which release it is (design §8).

use std::path::PathBuf;

use serde::Deserialize;
use tauri::{AppHandle, Manager};

/// Folder under the resource directory that holds the pdfium library; it
/// matches the targets in `tauri.linux.conf.json` and `tauri.windows.conf.json`.
const PDFIUM_RESOURCE_DIR: &str = "pdfium";

/// The pinned pdfium release, from the single place that records it.
const PDFIUM_VERSION_JSON: &str = include_str!("../../scripts/pdfium-version.json");

#[derive(Deserialize)]
struct PdfiumVersion {
    release: String,
}

/// The bundled pdfium release, such as `chromium/8076`.
///
/// # Panics
///
/// Panics if `scripts/pdfium-version.json` has no `release` string, which
/// the build embeds and `fetch-pdfium.ts` already validates.
pub fn release() -> String {
    serde_json::from_str::<PdfiumVersion>(PDFIUM_VERSION_JSON)
        .expect("scripts/pdfium-version.json should have a release")
        .release
}

/// The folder holding the bundled pdfium library, passed to each worker.
///
/// # Errors
///
/// Returns Tauri's error when the resource directory cannot be resolved.
pub fn library_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().resource_dir()?.join(PDFIUM_RESOURCE_DIR))
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_pinned_release() {
        assert!(super::release().starts_with("chromium/"));
    }
}
