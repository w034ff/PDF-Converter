//! IPC commands (design §7.1).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use pdfconv_core::IMAGE_EXTENSIONS;
use pdfconv_worker::WORKER_FLAG;
use pdfconv_worker::client::{OPEN_TIMEOUT, WorkerError, WorkerProcess};
use pdfconv_worker::protocol::Request;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;
use ts_rs::TS;

use crate::AppState;
use crate::error::{ErrorCode, IpcError};
use crate::items::{self, AddResult, ImageItem, PDF_EXTENSION, PdfItem};
use crate::jobs::{
    self, CheckPageRangeResult, PageSizeChoice, RenderFormatChoice, SaveMergedPdfResult,
};

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
    if state.is_running.load(Ordering::SeqCst) {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
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
    if state.is_running.load(Ordering::SeqCst) {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
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
pub fn remove_items(state: tauri::State<'_, AppState>, ids: Vec<u64>) -> Result<(), IpcError> {
    items::remove_items(&state, &ids)
}

/// Validates a page range string and counts matching pages across the specified PDF items (design §7.1).
#[tauri::command]
pub fn check_page_range(
    state: tauri::State<'_, AppState>,
    text: String,
    ids: Vec<u64>,
) -> Result<CheckPageRangeResult, IpcError> {
    jobs::check_page_range_internal(&state, &text, &ids)
}

/// Requests cancellation of the currently executing conversion job (design §6.5, §7.1).
#[tauri::command]
pub fn cancel_job(state: tauri::State<'_, AppState>) {
    state.cancel_flag.store(true, Ordering::SeqCst);
}

/// Opens a save dialog to save selected images into a single merged PDF (design §6.2, §7.1).
#[tauri::command]
pub async fn save_merged_pdf(
    app: AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    ids: Vec<u64>,
    page_size: PageSizeChoice,
) -> Result<Option<SaveMergedPdfResult>, IpcError> {
    if state
        .is_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    state.cancel_flag.store(false, Ordering::SeqCst);

    let state_inner = state.inner().clone();
    let running_guard = jobs::RunningGuard::new(Arc::clone(&state_inner.is_running));

    let default_name = {
        let first_id = ids
            .first()
            .ok_or_else(|| IpcError::from_code(ErrorCode::InvalidParams))?;
        let entry = state_inner
            .items
            .get(*first_id)
            .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
        let filename = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let (stem, _) = pdfconv_core::naming::split_stem_and_ext(&filename);
        format!("{stem}.pdf")
    };

    let picked_path = tauri::async_runtime::spawn_blocking(move || {
        let patterns = filter_extensions(&[PDF_EXTENSION]);
        let patterns: Vec<&str> = patterns.iter().map(String::as_str).collect();
        app.dialog()
            .file()
            .add_filter(PDF_FILTER_NAME, &patterns)
            .set_file_name(&default_name)
            .blocking_save_file()
            .map(dialog_path)
            .transpose()
    })
    .await
    .map_err(task_failed)??;

    let Some(dest_path) = picked_path else {
        return Ok(None);
    };

    let w1 = window.clone();
    let w2 = window.clone();
    let w3 = window.clone();

    let callbacks = jobs::JobCallbacks {
        on_progress: move |p| {
            let _ = w1.emit(jobs::JOB_PROGRESS_EVENT, &p);
        },
        on_item: move |item| {
            let _ = w2.emit(jobs::JOB_ITEM_EVENT, &item);
        },
        on_finished: running_guard.wrap_on_finished(move |fin| {
            let _ = w3.emit(jobs::JOB_FINISHED_EVENT, &fin);
        }),
    };

    tauri::async_runtime::spawn_blocking(move || {
        jobs::run_save_merged_pdf(&state_inner, &ids, page_size, &dest_path, callbacks)
    })
    .await
    .map_err(task_failed)?
}

/// Starts batch conversion of images to individual PDF files in the background (design §6.2, §7.1).
#[tauri::command]
pub async fn start_images_to_pdfs(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    ids: Vec<u64>,
    page_size: PageSizeChoice,
) -> Result<(), IpcError> {
    let w1 = window.clone();
    let w2 = window.clone();
    let w3 = window.clone();

    let callbacks = jobs::JobCallbacks {
        on_progress: move |p| {
            let _ = w1.emit(jobs::JOB_PROGRESS_EVENT, &p);
        },
        on_item: move |item| {
            let _ = w2.emit(jobs::JOB_ITEM_EVENT, &item);
        },
        on_finished: move |fin| {
            let _ = w3.emit(jobs::JOB_FINISHED_EVENT, &fin);
        },
    };

    jobs::start_images_to_pdfs_internal(&state, &ids, page_size, callbacks)
}

/// Starts batch conversion of PDF pages to image files in the background (design §6.3, §7.1).
#[tauri::command]
pub async fn start_pdfs_to_images(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    ids: Vec<u64>,
    range: String,
    format: RenderFormatChoice,
    dpi: u32,
) -> Result<(), IpcError> {
    let w1 = window.clone();
    let w2 = window.clone();
    let w3 = window.clone();

    let callbacks = jobs::JobCallbacks {
        on_progress: move |p| {
            let _ = w1.emit(jobs::JOB_PROGRESS_EVENT, &p);
        },
        on_item: move |item| {
            let _ = w2.emit(jobs::JOB_ITEM_EVENT, &item);
        },
        on_finished: move |fin| {
            let _ = w3.emit(jobs::JOB_FINISHED_EVENT, &fin);
        },
    };

    jobs::start_pdfs_to_images_internal(&state, &ids, &range, format, dpi, callbacks)
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
