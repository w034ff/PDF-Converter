//! IPC commands (design §7.1).

use std::path::PathBuf;

use pdfconv_core::IMAGE_EXTENSIONS;
use pdfconv_worker::WORKER_FLAG;
use pdfconv_worker::client::{OPEN_TIMEOUT, WorkerError, WorkerProcess};
use pdfconv_worker::protocol::Request;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use ts_rs::TS;

use crate::AppState;
use crate::error::{ErrorCode, IpcError};
use crate::items::{self, AddResult, ImageItem, PDF_EXTENSION, PdfItem};

/// Name of the image filter of the file dialog.
const IMAGE_FILTER_NAME: &str = "Images";

/// Name of the PDF filter of the file dialog.
const PDF_FILTER_NAME: &str = "PDF";

/// Answer of [`get_about`].
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    pub version: String,
    pub pdfium_version: String,
    /// Whether a worker started and could use pdfium. Until the about
    /// screen (T13) exists this is how an installed build shows that the
    /// bundled pdfium works.
    pub pdfium_ready: bool,
    pub pdfium_error: Option<String>,
}

/// Returns the app and pdfium versions after checking that a worker can
/// start and load pdfium.
#[tauri::command]
pub async fn get_about(app: AppHandle) -> AboutInfo {
    let version = app.package_info().version.to_string();
    let check = tauri::async_runtime::spawn_blocking(move || check_worker(&app)).await;
    let pdfium_error = match check {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e),
        Err(e) => Some(e.to_string()),
    };
    AboutInfo {
        version,
        pdfium_version: crate::pdfium::release(),
        pdfium_ready: pdfium_error.is_none(),
        pdfium_error,
    }
}

fn check_worker(app: &AppHandle) -> Result<(), String> {
    let library_dir = crate::pdfium::library_dir(app).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut worker = WorkerProcess::spawn(
        exe.as_os_str(),
        [WORKER_FLAG.as_ref(), library_dir.as_os_str()],
    )
    .map_err(|e| format!("{e:?}"))?;
    match worker.request(&Request::Hello, OPEN_TIMEOUT) {
        Ok(_) => Ok(()),
        Err(WorkerError::Remote { code, detail }) => {
            Err(format!("{code}: {}", detail.unwrap_or_default()))
        }
        Err(e) => Err(format!("{e:?}")),
    }
}

/// Where `add_images` and `add_pdfs` take their files from (design §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum AddSource {
    Files,
    Folder,
}

/// Asks for image files or a folder and adds the images in them to the list
/// of images. Returns `None` if the dialog was cancelled.
#[tauri::command]
pub async fn add_images(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    source: AddSource,
) -> Result<Option<AddResult<ImageItem>>, IpcError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || match source {
        AddSource::Files => Ok(pick_files(&app, IMAGE_FILTER_NAME, IMAGE_EXTENSIONS)?
            .map(|paths| items::add_images(&state, &paths))),
        AddSource::Folder => pick_folder(&app)?
            .map(|dir| items::add_images_from_folder(&state, &dir))
            .transpose(),
    })
    .await
    .map_err(task_failed)?
}

/// Asks for PDF files or a folder and adds the PDFs in them to the list of
/// PDFs. Returns `None` if the dialog was cancelled.
#[tauri::command]
pub async fn add_pdfs(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    source: AddSource,
) -> Result<Option<AddResult<PdfItem>>, IpcError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || match source {
        AddSource::Files => Ok(pick_files(&app, PDF_FILTER_NAME, &[PDF_EXTENSION])?
            .map(|paths| items::add_pdfs(&state, &paths))),
        AddSource::Folder => pick_folder(&app)?
            .map(|dir| items::add_pdfs_from_folder(&state, &dir))
            .transpose(),
    })
    .await
    .map_err(task_failed)?
}

/// Removes items from the lists. An ID that is not in the table is ignored.
#[tauri::command]
pub fn remove_items(state: tauri::State<'_, AppState>, ids: Vec<u64>) {
    state.items.remove(&ids);
}

/// Returns the PNG thumbnail of an item. `page` is used for a PDF only and
/// defaults to 1.
#[tauri::command]
pub async fn get_thumbnail(
    state: tauri::State<'_, AppState>,
    id: u64,
    page: Option<u32>,
) -> Result<tauri::ipc::Response, IpcError> {
    let state = state.inner().clone();
    let png = tauri::async_runtime::spawn_blocking(move || items::make_thumbnail(&state, id, page))
        .await
        .map_err(task_failed)??;
    Ok(tauri::ipc::Response::new(png))
}

/// Opens a dialog to pick several files with one of `extensions`. Blocks
/// until it is closed, so it must not run on the main thread.
fn pick_files(
    app: &AppHandle,
    filter_name: &str,
    extensions: &[&str],
) -> Result<Option<Vec<PathBuf>>, IpcError> {
    let patterns = filter_extensions(extensions);
    let patterns: Vec<&str> = patterns.iter().map(String::as_str).collect();
    app.dialog()
        .file()
        .add_filter(filter_name, &patterns)
        .blocking_pick_files()
        .map(|files| files.into_iter().map(dialog_path).collect())
        .transpose()
}

/// The extensions to give a file dialog's filter: each one in lower and upper
/// case. On Linux the dialog is GTK3's, whose patterns are case sensitive, so
/// `*.png` alone would hide `IMG_0001.PNG`; the Windows dialog ignores case and
/// is not hurt by the extra entries. Mixed case such as `Jpg` is not covered.
fn filter_extensions(extensions: &[&str]) -> Vec<String> {
    extensions
        .iter()
        .flat_map(|extension| [extension.to_lowercase(), extension.to_uppercase()])
        .collect()
}

/// Opens a dialog to pick a folder. Blocks until it is closed, so it must not
/// run on the main thread.
fn pick_folder(app: &AppHandle) -> Result<Option<PathBuf>, IpcError> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(dialog_path)
        .transpose()
}

fn dialog_path(picked: tauri_plugin_dialog::FilePath) -> Result<PathBuf, IpcError> {
    picked
        .into_path()
        .map_err(|_| IpcError::from_code(ErrorCode::ReadFailed))
}

/// The error of a blocking task that panicked or was cancelled.
fn task_failed(error: tauri::Error) -> IpcError {
    IpcError::new(ErrorCode::ReadFailed, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_has_each_extension_in_both_cases() {
        assert_eq!(filter_extensions(&["pdf"]), ["pdf", "PDF"]);
        assert_eq!(
            filter_extensions(IMAGE_EXTENSIONS),
            [
                "png", "PNG", "jpg", "JPG", "jpeg", "JPEG", "webp", "WEBP", "bmp", "BMP"
            ]
        );
    }

    #[test]
    fn the_extension_is_normalized_before_both_cases_are_made() {
        assert_eq!(filter_extensions(&["Jpg"]), ["jpg", "JPG"]);
    }
}
