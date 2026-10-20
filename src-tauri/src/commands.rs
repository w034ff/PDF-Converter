//! IPC commands (design §7.1).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use pdfconv_core::IMAGE_EXTENSIONS;
use pdfconv_worker::client::WorkerError;
use pdfconv_worker::protocol::Request;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder};
use ts_rs::TS;

use crate::AppState;
use crate::crop::{
    self, Candidate, CropPdf, DetectAllCallbacks, DropTarget, LAYOUT_PAGE_EVENT, PageImage,
};
use crate::crop_export::{self, CropRegionInput};
use crate::error::{ErrorCode, IpcError};
use crate::items::{self, AddResult, ImageItem, PDF_EXTENSION, PdfItem};
use crate::jobs::{
    self, CheckPageRangeResult, PageOrientationChoice, PageSizeChoice, RenderColorChoice,
    RenderFormatChoice, SaveMergedPdfResult, pdf_conformance,
};
use crate::settings::{self, CropOptions, OutputDirLabel, OutputKind, Settings, SettingsInput};
use crate::worker_pool::WorkerPool;

/// Name of the image filter of the file dialog.
const IMAGE_FILTER_NAME: &str = "Images";

/// Name of the PDF filter of the file dialog.
const PDF_FILTER_NAME: &str = "PDF";

/// Answer of [`get_about`].
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    pub version: String,
    /// Whether a worker started and could use pdfium. The about screen
    /// shows `pdfium_error` when it could not, so an installed build tells
    /// whether its bundled pdfium works.
    pub pdfium_ready: bool,
    pub pdfium_error: Option<String>,
}

/// Returns the app version after checking that a worker can start and load
/// pdfium.
#[tauri::command]
pub async fn get_about(app: AppHandle) -> AboutInfo {
    let version = app.package_info().version.to_string();
    let pool = app.state::<AppState>().pool.clone();
    let check = tauri::async_runtime::spawn_blocking(move || check_worker(&pool)).await;
    let pdfium_error = match check {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e),
        Err(e) => Some(e.to_string()),
    };
    AboutInfo {
        version,
        pdfium_ready: pdfium_error.is_none(),
        pdfium_error,
    }
}

/// Starts a worker with the pool's arguments, outside the pool, and asks it to
/// load pdfium. The arguments are the pool's so that the check cannot fall
/// behind what a worker needs to start (design §5.4).
///
/// # Errors
///
/// Returns why the worker could not start or load pdfium.
pub fn check_worker(pool: &WorkerPool) -> Result<(), String> {
    let mut worker = pool.spawn_dedicated().map_err(|e| e.to_string())?;
    let hello = Request::Hello;
    match worker.request(&hello, pool.timeout_for(&hello)) {
        Ok(_) => Ok(()),
        Err(WorkerError::Remote { code, detail }) => {
            Err(format!("{code}: {}", detail.unwrap_or_default()))
        }
        Err(e) => Err(format!("{e:?}")),
    }
}

/// Returns the settings, with the output folders as labels (design §6.7).
#[tauri::command]
pub fn get_settings(state: tauri::State<'_, AppState>) -> Settings {
    settings::get_settings_internal(&state)
}

/// Saves the settings except the output folders (design §6.7). Nothing is
/// saved if a value is invalid, and a `revision` not higher than an earlier
/// one from the page is ignored.
///
/// Not `async`, so that the calls run on the main thread one after another
/// and a change has reached memory before `RunEvent::Exit` writes it. The file
/// is written on another thread; a failed write is not reported, because the
/// settings still apply to this session and the next write tries again.
#[tauri::command]
pub fn save_settings(
    state: tauri::State<'_, AppState>,
    revision: u64,
    settings: SettingsInput,
) -> Result<(), IpcError> {
    state.settings.update(revision, &settings)?;
    let store = Arc::clone(&state.settings);
    tauri::async_runtime::spawn_blocking(move || {
        let flushed = store.flush();
        #[cfg(debug_assertions)]
        if let Err(error) = &flushed {
            eprintln!("[debug] save_settings: {error:?}");
        }
        let _ = flushed;
    });
    Ok(())
}

/// Asks for a folder and makes it the output folder of `kind`. Returns `None`
/// if the dialog was cancelled (design §6.7).
#[tauri::command]
pub async fn pick_output_dir(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    kind: OutputKind,
) -> Result<Option<OutputDirLabel>, IpcError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(dir) = pick_folder(&window)? else {
            return Ok(None);
        };
        let (label, persisted) = settings::apply_picked_dir(&state, kind, dir);
        #[cfg(debug_assertions)]
        if let Err(error) = &persisted {
            eprintln!("[debug] settings were not saved: {error}");
        }
        // The folder is in use even if the file could not be written.
        let _ = persisted;
        Ok(Some(label))
    })
    .await
    .map_err(task_failed)?
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
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    source: AddSource,
) -> Result<Option<AddResult<ImageItem>>, IpcError> {
    if state.is_running.load(Ordering::SeqCst) {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || match source {
        AddSource::Files => Ok(pick_files(&window, IMAGE_FILTER_NAME, IMAGE_EXTENSIONS)?
            .map(|paths| items::add_images(&state, &paths))),
        AddSource::Folder => pick_folder(&window)?
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
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    source: AddSource,
) -> Result<Option<AddResult<PdfItem>>, IpcError> {
    if state.is_running.load(Ordering::SeqCst) {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || match source {
        AddSource::Files => Ok(pick_files(&window, PDF_FILTER_NAME, &[PDF_EXTENSION])?
            .map(|paths| items::add_pdfs(&state, &paths))),
        AddSource::Folder => pick_folder(&window)?
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

/// Unlocks a password-protected PDF and auto-unlocks matching entries (design §6.8, §7.1).
#[tauri::command]
pub async fn unlock_pdf(
    state: tauri::State<'_, AppState>,
    id: u64,
    password: String,
) -> Result<items::UnlockResult, IpcError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || items::unlock_pdf(&state, id, &password))
        .await
        .map_err(task_failed)?
}

/// Asks for one PDF and holds it as the pending crop PDF (design §6.10, §7.1).
/// Returns `None` if the dialog was cancelled. The open crop PDF is not
/// touched until [`activate_crop_pdf`].
#[tauri::command]
pub async fn open_crop_pdf(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
) -> Result<Option<CropPdf>, IpcError> {
    if state.is_running.load(Ordering::SeqCst) {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(pick_file(&window, PDF_FILTER_NAME, &[PDF_EXTENSION])?
            .map(|path| crop::open_pdf(&state, &path)))
    })
    .await
    .map_err(task_failed)?
}

/// Makes the pending crop PDF `id` the open one (design §6.10, §7.1).
#[tauri::command]
pub fn activate_crop_pdf(state: tauri::State<'_, AppState>, id: u64) -> Result<(), IpcError> {
    crop::activate_crop_pdf(&state, id)
}

/// Drops the pending crop PDF if it is `id` (design §6.10, §7.1).
#[tauri::command]
pub fn discard_crop_pdf(state: tauri::State<'_, AppState>, id: u64) {
    crop::discard_crop_pdf(&state, id);
}

/// Drops the open and the pending crop PDF (design §6.10, §7.1).
#[tauri::command]
pub fn close_crop_pdf(state: tauri::State<'_, AppState>) -> Result<(), IpcError> {
    crop::close_crop_pdf(&state)
}

/// Opens the open crop PDF again with a password (design §6.8, §6.10, §7.1).
#[tauri::command]
pub async fn unlock_crop_pdf(
    state: tauri::State<'_, AppState>,
    id: u64,
    password: String,
) -> Result<CropPdf, IpcError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crop::unlock_crop_pdf(&state, id, &password))
        .await
        .map_err(task_failed)?
}

/// Tells Rust where a drop goes: the lists or the crop tab (design §6.10, §7.1).
#[tauri::command]
pub fn set_drop_target(state: tauri::State<'_, AppState>, target: DropTarget) {
    state.crop.set_drop_target(target);
}

/// Lists the embedded images of a page of the open crop PDF (design §7.1).
#[tauri::command]
pub async fn list_page_images(
    state: tauri::State<'_, AppState>,
    id: u64,
    page: u32,
) -> Result<Vec<PageImage>, IpcError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crop::list_page_images(&state, id, page))
        .await
        .map_err(task_failed)?
}

/// Whether anything is drawn over an embedded image of the open crop PDF
/// (design §7.1).
#[tauri::command]
pub async fn check_on_top(
    state: tauri::State<'_, AppState>,
    id: u64,
    page: u32,
    image_index: u32,
) -> Result<bool, IpcError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crop::check_on_top(&state, id, page, image_index))
        .await
        .map_err(task_failed)?
}

/// The candidates for cropping on a page of the open crop PDF. Allowed during
/// a conversion (design §7.1).
#[tauri::command]
pub async fn detect_layout(
    state: tauri::State<'_, AppState>,
    id: u64,
    page: u32,
) -> Result<Vec<Candidate>, IpcError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crop::detect_layout(&state, id, page))
        .await
        .map_err(task_failed)?
}

/// Starts the layout analysis of every page of the open crop PDF that has not
/// been analyzed, as a conversion (design §6.10, §7.1). Each page is reported
/// by `layout-page`.
#[tauri::command]
pub async fn start_detect_all(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    id: u64,
) -> Result<(), IpcError> {
    let w1 = window.clone();
    let w2 = window.clone();
    let w3 = window.clone();

    let callbacks = DetectAllCallbacks {
        on_progress: move |p| {
            let _ = w1.emit(jobs::JOB_PROGRESS_EVENT, &p);
        },
        on_page: move |page| {
            let _ = w2.emit(LAYOUT_PAGE_EVENT, &page);
        },
        on_finished: move |fin| {
            let _ = w3.emit(jobs::JOB_FINISHED_EVENT, &fin);
        },
    };

    crop::start_detect_all_internal(&state, id, callbacks)
}

/// Starts writing ranges of the open crop PDF as image files in the background
/// (design §6.10, §7.1).
// The parameters are the IPC arguments of design §7.1, so they cannot be grouped.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn start_crop_export(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    id: u64,
    regions: Vec<CropRegionInput>,
    format: RenderFormatChoice,
    color_mode: RenderColorChoice,
    dpi: u32,
    photo_native: bool,
    photo_only: bool,
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
    let options = CropOptions {
        format,
        color_mode,
        dpi,
        photo_native,
        photo_only,
    };

    // Listing the images of the pages takes workers, which do not belong on
    // the async runtime's threads.
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crop_export::start_crop_export_internal(&state, id, &regions, options, callbacks)
    })
    .await
    .map_err(task_failed)?
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
///
/// Returns `None` only when the dialog was cancelled, so nothing ran. Once the
/// conversion has run, the result says whether a PDF was written.
#[tauri::command]
pub async fn save_merged_pdf(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    ids: Vec<u64>,
    page_size: PageSizeChoice,
    orientation: PageOrientationChoice,
    pdf_a: bool,
    password: Option<String>,
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

    // Refused before the save dialog opens, so that nothing is asked for a refused call.
    let conformance = pdf_conformance(pdf_a);
    let output = jobs::PdfOutput::new(conformance, jobs::output_password(conformance, password)?);

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

    let dialog_window = window.clone();
    let picked_path = tauri::async_runtime::spawn_blocking(move || {
        let patterns = filter_extensions(&[PDF_EXTENSION]);
        let patterns: Vec<&str> = patterns.iter().map(String::as_str).collect();
        let dialog = dialog_window
            .dialog()
            .file()
            .set_parent(&dialog_window)
            .add_filter(PDF_FILTER_NAME, &patterns)
            .set_file_name(&default_name);
        save_dialog_start_dir(&dialog_window, dialog)
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
        jobs::run_save_merged_pdf(
            &state_inner,
            &ids,
            page_size,
            orientation,
            &output,
            &dest_path,
            callbacks,
        )
    })
    .await
    .map_err(task_failed)?
    .map(Some)
}

/// Starts batch conversion of images to individual PDF files in the background (design §6.2, §7.1).
#[tauri::command]
pub async fn start_images_to_pdfs(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    ids: Vec<u64>,
    page_size: PageSizeChoice,
    orientation: PageOrientationChoice,
    pdf_a: bool,
    password: Option<String>,
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

    jobs::start_images_to_pdfs_internal(
        &state,
        &ids,
        page_size,
        orientation,
        pdf_conformance(pdf_a),
        password,
        callbacks,
    )
}

/// Starts batch conversion of PDF pages to image files in the background (design §6.3, §7.1).
#[tauri::command]
pub async fn start_pdfs_to_images(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    ids: Vec<u64>,
    range: String,
    format: RenderFormatChoice,
    color_mode: RenderColorChoice,
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

    jobs::start_pdfs_to_images_internal(&state, &ids, &range, format, color_mode, dpi, callbacks)
}

/// Returns the PNG thumbnail of an item of a list or of the open crop PDF.
/// `page` is used for a PDF only and defaults to 1; `max_side` sets the longer
/// side of a PDF page (design §7.1).
#[tauri::command]
pub async fn get_thumbnail(
    state: tauri::State<'_, AppState>,
    id: u64,
    page: Option<u32>,
    max_side: Option<u32>,
) -> Result<tauri::ipc::Response, IpcError> {
    let state = state.inner().clone();
    let png = tauri::async_runtime::spawn_blocking(move || {
        items::make_thumbnail(&state, id, page, max_side)
    })
    .await
    .map_err(task_failed)??;
    Ok(tauri::ipc::Response::new(png))
}

/// Opens a dialog, a child of `window`, to pick one file with one of
/// `extensions`. Blocks until it is closed, so it must not run on the main
/// thread.
fn pick_file(
    window: &tauri::Window,
    filter_name: &str,
    extensions: &[&str],
) -> Result<Option<PathBuf>, IpcError> {
    let patterns = filter_extensions(extensions);
    let patterns: Vec<&str> = patterns.iter().map(String::as_str).collect();
    window
        .dialog()
        .file()
        .set_parent(window)
        .add_filter(filter_name, &patterns)
        .blocking_pick_file()
        .map(dialog_path)
        .transpose()
}

/// Opens a dialog, a child of `window`, to pick several files with one of
/// `extensions`. Blocks until it is closed, so it must not run on the main
/// thread.
fn pick_files(
    window: &tauri::Window,
    filter_name: &str,
    extensions: &[&str],
) -> Result<Option<Vec<PathBuf>>, IpcError> {
    let patterns = filter_extensions(extensions);
    let patterns: Vec<&str> = patterns.iter().map(String::as_str).collect();
    window
        .dialog()
        .file()
        .set_parent(window)
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

/// Makes the save dialog start in the home folder.
///
/// Without a folder, GTK opens the dialog in the working directory. In an
/// AppImage that is the read-only mount of the image, where nothing can be
/// saved. Windows is left alone: its dialog remembers the last folder.
#[cfg(target_os = "linux")]
fn save_dialog_start_dir(
    window: &tauri::Window,
    dialog: FileDialogBuilder<Wry>,
) -> FileDialogBuilder<Wry> {
    use tauri::Manager;

    match window.path().home_dir() {
        Ok(home) => dialog.set_directory(home),
        Err(_) => dialog,
    }
}

#[cfg(not(target_os = "linux"))]
fn save_dialog_start_dir(
    _window: &tauri::Window,
    dialog: FileDialogBuilder<Wry>,
) -> FileDialogBuilder<Wry> {
    dialog
}

/// Opens a dialog, a child of `window`, to pick a folder. Blocks until it is
/// closed, so it must not run on the main thread.
fn pick_folder(window: &tauri::Window) -> Result<Option<PathBuf>, IpcError> {
    window
        .dialog()
        .file()
        .set_parent(window)
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
                "png", "PNG", "jpg", "JPG", "jpeg", "JPEG", "webp", "WEBP", "bmp", "BMP", "tif",
                "TIF", "tiff", "TIFF"
            ]
        );
    }

    #[test]
    fn the_extension_is_normalized_before_both_cases_are_made() {
        assert_eq!(filter_extensions(&["Jpg"]), ["jpg", "JPG"]);
    }
}
