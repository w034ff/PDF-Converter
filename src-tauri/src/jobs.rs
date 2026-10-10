//! Batch conversion execution, state management, and atomic file saving
//! per design §6.2, §6.3, §6.4, §6.5.

use std::collections::{HashSet, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use pdfconv_core::naming::{
    PdfToImageInput, resolve_image_to_pdf_names, resolve_pdf_to_image_names, split_stem_and_ext,
};
use pdfconv_core::{PageSet, PdfWriter, parse_page_range};
use pdfconv_worker::DPI_CHOICES;
use pdfconv_worker::protocol::{RenderFormat, Request, Response};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::AppState;
use crate::error::{ErrorCode, IpcError};
use crate::items::ItemKind;
use crate::settings::{self, OutputKind};
use crate::worker_pool::{WorkerPoolError, default_worker_limit};

/// Name of the event emitted for job progress updates (design §7.2).
pub const JOB_PROGRESS_EVENT: &str = "job-progress";

/// Name of the event emitted when an item completes (design §7.2).
pub const JOB_ITEM_EVENT: &str = "job-item";

/// Name of the event emitted when a job finishes (design §7.2).
pub const JOB_FINISHED_EVENT: &str = "job-finished";

/// Page size choice for image-to-PDF conversion (design §6.2, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PageSizeChoice {
    Fit,
    A4,
}

impl PageSizeChoice {
    /// Converts this choice and an A4 orientation into a [`pdfconv_core::PageSize`] (design §7.1).
    ///
    /// When `self` is [`PageSizeChoice::Fit`], `a4_orientation` is ignored.
    pub fn to_core(self, a4_orientation: A4OrientationChoice) -> pdfconv_core::PageSize {
        match self {
            Self::Fit => pdfconv_core::PageSize::Fit,
            Self::A4 => pdfconv_core::PageSize::A4(a4_orientation.into()),
        }
    }
}

/// A4 orientation choice for image-to-PDF conversion (design §6.7, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum A4OrientationChoice {
    Auto,
    Portrait,
    Landscape,
}

impl From<A4OrientationChoice> for pdfconv_core::layout::A4Orientation {
    fn from(choice: A4OrientationChoice) -> Self {
        match choice {
            A4OrientationChoice::Auto => Self::Auto,
            A4OrientationChoice::Portrait => Self::Portrait,
            A4OrientationChoice::Landscape => Self::Landscape,
        }
    }
}

/// Image format choice for PDF-to-image conversion (design §6.3, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RenderFormatChoice {
    Png,
    Jpeg,
}

impl RenderFormatChoice {
    /// File extension for output files without the dot (design §6.3).
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

impl From<RenderFormatChoice> for RenderFormat {
    fn from(choice: RenderFormatChoice) -> Self {
        match choice {
            RenderFormatChoice::Png => Self::Png,
            RenderFormatChoice::Jpeg => Self::Jpeg,
        }
    }
}

/// Status of an individual item in a job (design §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum JobItemStatus {
    Ok,
    Failed,
    Partial,
    NoPages,
    Cancelled,
}

/// Progress notification emitted via `job-progress` event (design §7.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobProgressPayload {
    /// Total completed items or pages so far.
    #[ts(type = "number")]
    pub done: u32,
    /// Total items (image-to-pdf) or total pages (pdf-to-image).
    #[ts(type = "number")]
    pub total: u32,
    /// Name of the current file being processed, or None.
    pub current: Option<String>,
}

/// Item completion notification emitted via `job-item` event (design §7.2).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobItemPayload {
    #[ts(type = "number")]
    pub id: u64,
    pub status: JobItemStatus,
    pub outputs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<IpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub failed_pages: Option<Vec<u32>>,
}

/// Final summary emitted via `job-finished` event (design §7.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobFinishedPayload {
    #[ts(type = "number")]
    pub succeeded: u32,
    #[ts(type = "number")]
    pub failed: u32,
    #[ts(type = "number")]
    pub no_pages: u32,
    #[ts(type = "number")]
    pub unprocessed: u32,
    pub cancelled: bool,
}

/// Result of checking a page range across candidate PDFs (design §7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CheckPageRangeResult {
    #[ts(type = "number")]
    pub total_pages: u32,
    /// The range as sorted, non-overlapping `[start, end]` page intervals, so
    /// the screen can mark pages without parsing the text itself (design
    /// §4.5, §6.3). Its length follows the text, not the pages it covers.
    #[ts(type = "Array<[number, number]>")]
    pub intervals: Vec<(u32, u32)>,
}

/// Result returned once a merge has run (design §7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SaveMergedPdfResult {
    /// The saved file's name, or `None` when no PDF was written (the job was
    /// cancelled, or no image could be added).
    pub saved_name: Option<String>,
}

/// Callbacks for receiving job progress, item results, and completion events.
pub struct JobCallbacks<FProg, FItem, FFin> {
    pub on_progress: FProg,
    pub on_item: FItem,
    pub on_finished: FFin,
}

/// RAII guard to reset the running flag when dropped if not already reset.
pub struct RunningGuard {
    flag: Arc<AtomicBool>,
    active: Arc<AtomicBool>,
}

impl RunningGuard {
    /// Creates a new guard for the given running flag.
    pub fn new(flag: Arc<AtomicBool>) -> Self {
        Self {
            flag,
            active: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Wraps an `on_finished` callback so that `is_running` is reset to `false`
    /// before `on_finished` is invoked, and disarms this guard so that
    /// dropping it later does not reset the flag again.
    pub fn wrap_on_finished<FFin>(
        &self,
        on_finished: FFin,
    ) -> impl FnOnce(JobFinishedPayload) + Send + Sync + 'static + use<FFin>
    where
        FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
    {
        let flag = Arc::clone(&self.flag);
        let active = Arc::clone(&self.active);
        move |payload| {
            if active.swap(false, Ordering::SeqCst) {
                flag.store(false, Ordering::SeqCst);
            }
            on_finished(payload);
        }
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        if self.active.swap(false, Ordering::SeqCst) {
            self.flag.store(false, Ordering::SeqCst);
        }
    }
}

/// Lists existing file and directory names directly under `dir` (design §5.2, §6.4).
pub fn list_existing_files(dir: &Path) -> Result<HashSet<String>, IpcError> {
    let mut set = HashSet::new();
    if !dir.exists() {
        return Ok(set);
    }
    let entries = std::fs::read_dir(dir).map_err(|e| {
        IpcError::new(
            ErrorCode::WriteFailed,
            format!("could not read output directory: {e}"),
        )
    })?;
    for entry in entries.flatten() {
        set.insert(entry.file_name().to_string_lossy().to_string());
    }
    Ok(set)
}

/// Mode a converted file is created with before the umask is applied, so a
/// saved file gets the permissions any other new file in the folder gets.
#[cfg(unix)]
const OUTPUT_FILE_MODE: u32 = 0o666;

/// Creates the dot-prefixed temporary file a conversion writes into before
/// renaming it to its final name (design §6.5).
///
/// `tempfile` creates it readable by its owner only, and the rename keeps that.
/// Unix therefore asks for [`OUTPUT_FILE_MODE`] instead; Windows files have no
/// such mode.
fn create_output_temp(output_dir: &Path) -> std::io::Result<tempfile::NamedTempFile> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(".");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(OUTPUT_FILE_MODE));
    }
    builder.tempfile_in(output_dir)
}

/// Checks, before a conversion starts, that the output folder of `kind` is
/// still there and that a file can be created in it (design §6.5).
///
/// A folder that is gone or is not a folder is put back to "not chosen", in
/// the state and in the settings file, since the screen has to ask for another.
/// A folder that cannot be written to stays chosen: the owner may fix its
/// permissions.
///
/// # Errors
///
/// `OutputDirMissing` if `dir` does not exist or is not a folder,
/// `OutputDirNotWritable` if a temporary file cannot be created in it.
pub fn check_output_dir(state: &AppState, kind: OutputKind, dir: &Path) -> Result<(), IpcError> {
    if !dir.is_dir() {
        // A settings file that cannot be written must not hide the real problem.
        let _ = settings::clear_output_dir(state, kind);
        return Err(IpcError::from_code(ErrorCode::OutputDirMissing));
    }
    // The error is not kept: its text names the folder, and the frontend
    // never sees paths (design §1).
    create_output_temp(dir)
        .map(drop)
        .map_err(|_| IpcError::from_code(ErrorCode::OutputDirNotWritable))
}

/// Raises `write_failed`, which stops a conversion from taking up its next
/// item or page, if `err` is a failure to write the output (design §6.5).
/// Returns whether it was one.
fn stop_after_write_failure(err: &IpcError, write_failed: &AtomicBool) -> bool {
    let is_write_failure = err.code == ErrorCode::WriteFailed;
    if is_write_failure {
        write_failed.store(true, Ordering::SeqCst);
    }
    is_write_failure
}

/// Saves file bytes atomically into `output_dir` using a dot-prefixed temporary file
/// and `persist_noclobber` (design §6.4, §6.5).
///
/// If a collision occurs (`AlreadyExists`), increments the candidate numeric suffix
/// (`stem (1).ext`, `stem (2).ext`, ...) until an unoccupied name is found.
pub fn save_atomic(
    output_dir: &Path,
    initial_output_name: &str,
    bytes: &[u8],
    used_names_lower: &Mutex<HashSet<String>>,
) -> Result<String, IpcError> {
    let mut temp = create_output_temp(output_dir)
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;

    temp.write_all(bytes)
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;
    temp.flush()
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;

    let (stem, ext) = split_stem_and_ext(initial_output_name);
    let mut candidate = initial_output_name.to_string();
    let mut suffix_num = 0usize;

    loop {
        let dest_path = output_dir.join(&candidate);
        match temp.persist_noclobber(&dest_path) {
            Ok(_) => {
                let mut used = used_names_lower
                    .lock()
                    .expect("used_names_lower mutex poisoned");
                used.insert(candidate.to_lowercase());
                return Ok(candidate);
            }
            Err(persist_err) => {
                if persist_err.error.kind() == std::io::ErrorKind::AlreadyExists {
                    temp = persist_err.file;
                    let mut used = used_names_lower
                        .lock()
                        .expect("used_names_lower mutex poisoned");
                    loop {
                        suffix_num += 1;
                        let next_candidate = if ext.is_empty() {
                            format!("{stem} ({suffix_num})")
                        } else {
                            format!("{stem} ({suffix_num}).{ext}")
                        };
                        let next_lower = next_candidate.to_lowercase();
                        if !used.contains(&next_lower) && !output_dir.join(&next_candidate).exists()
                        {
                            used.insert(next_lower);
                            candidate = next_candidate;
                            break;
                        }
                    }
                } else {
                    return Err(IpcError::new(
                        ErrorCode::WriteFailed,
                        persist_err.error.to_string(),
                    ));
                }
            }
        }
    }
}

/// Validates a page range string and counts pages across specified PDF items (design §4.5, §7.1).
pub fn check_page_range_internal(
    state: &AppState,
    text: &str,
    ids: &[u64],
) -> Result<CheckPageRangeResult, IpcError> {
    let page_set =
        parse_page_range(text).map_err(|e| IpcError::new(ErrorCode::InvalidPageRange, e.detail))?;
    let mut total_pages = 0u32;
    for &id in ids {
        if let Some(entry) = state.items.get(id)
            && entry.kind == ItemKind::Pdf
            && entry.error.is_none()
        {
            total_pages = total_pages.saturating_add(page_set.count_within(entry.page_count));
        }
    }
    Ok(CheckPageRangeResult {
        total_pages,
        intervals: page_set.intervals().to_vec(),
    })
}

/// Executes merging multiple images into a single PDF document (design §6.2, §6.5).
pub fn run_save_merged_pdf<FProg, FItem, FFin>(
    state: &AppState,
    ids: &[u64],
    page_size: PageSizeChoice,
    a4_orientation: A4OrientationChoice,
    dest_path: &Path,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<SaveMergedPdfResult, IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    let on_progress = callbacks.on_progress;
    let on_item = callbacks.on_item;
    let on_finished = callbacks.on_finished;

    let output_dir = dest_path
        .parent()
        .ok_or_else(|| IpcError::from_code(ErrorCode::WriteFailed))?;

    let mut valid_tasks = Vec::new();
    for &id in ids {
        let entry = state
            .items
            .get(id)
            .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
        if entry.kind == ItemKind::Image {
            valid_tasks.push((id, entry));
        }
    }

    let total = valid_tasks.len() as u32;
    on_progress(JobProgressPayload {
        done: 0,
        total,
        current: None,
    });

    let creator = format!("PDF Converter {}", env!("CARGO_PKG_VERSION"));
    let mut writer = PdfWriter::new(&creator);

    let mut temp = create_output_temp(output_dir)
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;

    let mut succeeded = 0u32;
    let mut failed = 0u32;
    let mut unprocessed = 0u32;
    let mut was_cancelled = false;

    for (idx, (id, entry)) in valid_tasks.into_iter().enumerate() {
        if state.cancel_flag.load(Ordering::SeqCst) {
            was_cancelled = true;
            unprocessed = total.saturating_sub(idx as u32);
            break;
        }

        let file_name = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        on_progress(JobProgressPayload {
            done: succeeded + failed,
            total,
            current: Some(file_name),
        });

        if let Some(err) = entry.error {
            failed += 1;
            on_item(JobItemPayload {
                id,
                status: JobItemStatus::Failed,
                outputs: Vec::new(),
                error: Some(err),
                failed_pages: None,
            });
            on_progress(JobProgressPayload {
                done: succeeded + failed,
                total,
                current: None,
            });
            continue;
        }

        let read_result = std::fs::read(&entry.path)
            .map_err(|_| IpcError::from_code(ErrorCode::ReadFailed))
            .and_then(|bytes| {
                writer
                    .add_page(&bytes, page_size.to_core(a4_orientation))
                    .map_err(IpcError::from)
            });

        match read_result {
            Ok(()) => {
                succeeded += 1;
                on_item(JobItemPayload {
                    id,
                    status: JobItemStatus::Ok,
                    outputs: Vec::new(),
                    error: None,
                    failed_pages: None,
                });
            }
            Err(err) => {
                failed += 1;
                on_item(JobItemPayload {
                    id,
                    status: JobItemStatus::Failed,
                    outputs: Vec::new(),
                    error: Some(err),
                    failed_pages: None,
                });
            }
        }

        on_progress(JobProgressPayload {
            done: succeeded + failed,
            total,
            current: None,
        });
    }

    if was_cancelled || state.cancel_flag.load(Ordering::SeqCst) {
        // Drop temp file without writing or persisting per design §6.5
        drop(temp);
        on_finished(JobFinishedPayload {
            succeeded: 0,
            failed: 0,
            no_pages: 0,
            unprocessed,
            cancelled: true,
        });
        return Ok(SaveMergedPdfResult { saved_name: None });
    }

    if succeeded == 0 {
        drop(temp);
        on_finished(JobFinishedPayload {
            succeeded: 0,
            failed,
            no_pages: 0,
            unprocessed: 0,
            cancelled: false,
        });
        return Ok(SaveMergedPdfResult { saved_name: None });
    }

    let pdf_bytes = writer
        .finish()
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;

    temp.write_all(&pdf_bytes)
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;
    temp.flush()
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;

    // Merged PDF save dialog allows overwriting (design §6.2)
    temp.persist(dest_path)
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.error.to_string()))?;

    let saved_name = dest_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    on_finished(JobFinishedPayload {
        succeeded,
        failed,
        no_pages: 0,
        unprocessed: 0,
        cancelled: false,
    });

    Ok(SaveMergedPdfResult {
        saved_name: Some(saved_name),
    })
}

/// Executes individual conversion of multiple images into separate PDF documents (design §6.2).
pub fn run_images_to_pdfs<FProg, FItem, FFin>(
    state: &AppState,
    ids: &[u64],
    page_size: PageSizeChoice,
    a4_orientation: A4OrientationChoice,
    output_dir: &Path,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    let on_progress = Arc::new(callbacks.on_progress);
    let on_item = Arc::new(callbacks.on_item);
    let on_finished = callbacks.on_finished;

    let existing = list_existing_files(output_dir)?;

    struct ImageTask {
        id: u64,
        filename: String,
        path: PathBuf,
        initial_error: Option<IpcError>,
        initial_output_name: Option<String>,
    }

    let mut target_names = Vec::new();
    let mut initial_tasks = Vec::new();

    for &id in ids {
        let entry = state
            .items
            .get(id)
            .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
        if entry.kind != ItemKind::Image {
            continue;
        }
        let filename = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if entry.error.is_none() {
            target_names.push(filename.clone());
        }
        initial_tasks.push((id, filename, entry.path, entry.error));
    }

    let resolved_names = resolve_image_to_pdf_names(&target_names, &existing);

    let mut initial_used: HashSet<String> = existing.iter().map(|s| s.to_lowercase()).collect();
    for name in &resolved_names {
        initial_used.insert(name.to_lowercase());
    }
    let used_names_lower = Arc::new(Mutex::new(initial_used));

    let mut resolved_iter = resolved_names.into_iter();
    let tasks: VecDeque<ImageTask> = initial_tasks
        .into_iter()
        .map(|(id, filename, path, initial_error)| {
            let initial_output_name = if initial_error.is_none() {
                resolved_iter.next()
            } else {
                None
            };
            ImageTask {
                id,
                filename,
                path,
                initial_error,
                initial_output_name,
            }
        })
        .collect();

    let total = tasks.len() as u32;
    on_progress(JobProgressPayload {
        done: 0,
        total,
        current: None,
    });

    if total == 0 {
        on_finished(JobFinishedPayload {
            succeeded: 0,
            failed: 0,
            no_pages: 0,
            unprocessed: 0,
            cancelled: state.cancel_flag.load(Ordering::SeqCst),
        });
        return Ok(());
    }

    let queue = Arc::new(Mutex::new(tasks));
    let done = Arc::new(AtomicU32::new(0));
    let succeeded = Arc::new(AtomicU32::new(0));
    let failed = Arc::new(AtomicU32::new(0));
    let unprocessed = Arc::new(AtomicU32::new(0));
    let write_failed = Arc::new(AtomicBool::new(false));

    let num_threads = default_worker_limit().min(total as usize).max(1);
    let mut handles = Vec::with_capacity(num_threads);

    let creator = format!("PDF Converter {}", env!("CARGO_PKG_VERSION"));
    let output_dir_buf = output_dir.to_path_buf();

    for _ in 0..num_threads {
        let queue = Arc::clone(&queue);
        let done = Arc::clone(&done);
        let succeeded = Arc::clone(&succeeded);
        let failed = Arc::clone(&failed);
        let unprocessed = Arc::clone(&unprocessed);
        let cancel_flag = Arc::clone(&state.cancel_flag);
        let write_failed = Arc::clone(&write_failed);
        let used_names = Arc::clone(&used_names_lower);
        let output_dir = output_dir_buf.clone();
        let creator = creator.clone();
        let on_progress = Arc::clone(&on_progress);
        let on_item = Arc::clone(&on_item);

        handles.push(std::thread::spawn(move || {
            loop {
                if cancel_flag.load(Ordering::SeqCst) || write_failed.load(Ordering::SeqCst) {
                    let mut q = match queue.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    unprocessed.fetch_add(q.len() as u32, Ordering::SeqCst);
                    q.clear();
                    break;
                }

                let task = {
                    let mut q = match queue.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    q.pop_front()
                };

                let Some(task) = task else {
                    break;
                };

                if let Some(err) = task.initial_error {
                    failed.fetch_add(1, Ordering::SeqCst);
                    let current_done = done.fetch_add(1, Ordering::SeqCst) + 1;
                    on_item(JobItemPayload {
                        id: task.id,
                        status: JobItemStatus::Failed,
                        outputs: Vec::new(),
                        error: Some(err),
                        failed_pages: None,
                    });
                    on_progress(JobProgressPayload {
                        done: current_done,
                        total,
                        current: None,
                    });
                    continue;
                }

                let output_name = task.initial_output_name.as_deref().unwrap_or_default();

                on_progress(JobProgressPayload {
                    done: done.load(Ordering::SeqCst),
                    total,
                    current: Some(task.filename.clone()),
                });

                let pdf_bytes = (|| -> Result<Vec<u8>, IpcError> {
                    let bytes = std::fs::read(&task.path)
                        .map_err(|_| IpcError::from_code(ErrorCode::ReadFailed))?;
                    let mut writer = PdfWriter::new(&creator);
                    writer
                        .add_page(&bytes, page_size.to_core(a4_orientation))
                        .map_err(IpcError::from)?;
                    writer
                        .finish()
                        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))
                })();
                let process_result = pdf_bytes.and_then(|pdf_bytes| {
                    save_atomic(&output_dir, output_name, &pdf_bytes, &used_names).inspect_err(
                        |err| {
                            stop_after_write_failure(err, &write_failed);
                        },
                    )
                });

                match process_result {
                    Ok(saved_name) => {
                        succeeded.fetch_add(1, Ordering::SeqCst);
                        let current_done = done.fetch_add(1, Ordering::SeqCst) + 1;
                        on_item(JobItemPayload {
                            id: task.id,
                            status: JobItemStatus::Ok,
                            outputs: vec![saved_name],
                            error: None,
                            failed_pages: None,
                        });
                        on_progress(JobProgressPayload {
                            done: current_done,
                            total,
                            current: None,
                        });
                    }
                    Err(err) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                        let current_done = done.fetch_add(1, Ordering::SeqCst) + 1;
                        on_item(JobItemPayload {
                            id: task.id,
                            status: JobItemStatus::Failed,
                            outputs: Vec::new(),
                            error: Some(err),
                            failed_pages: None,
                        });
                        on_progress(JobProgressPayload {
                            done: current_done,
                            total,
                            current: None,
                        });
                    }
                }
            }
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    let cur_succeeded = succeeded.load(Ordering::SeqCst);
    let cur_failed = failed.load(Ordering::SeqCst);
    let cur_unprocessed = unprocessed.load(Ordering::SeqCst);
    let accounted = cur_succeeded + cur_failed + cur_unprocessed;
    if accounted < total {
        failed.fetch_add(total - accounted, Ordering::SeqCst);
    }

    on_finished(JobFinishedPayload {
        succeeded: succeeded.load(Ordering::SeqCst),
        failed: failed.load(Ordering::SeqCst),
        no_pages: 0,
        unprocessed: unprocessed.load(Ordering::SeqCst),
        cancelled: state.cancel_flag.load(Ordering::SeqCst),
    });

    Ok(())
}

/// Prepared input and output metadata for a single PDF document in PDF-to-image conversion.
struct PdfConversionTask {
    id: u64,
    filename: String,
    path: PathBuf,
    initial_error: Option<IpcError>,
    pages: Vec<u32>,
    planned_names: Vec<String>,
}

/// Executes PDF-to-image conversion across multiple PDFs (design §6.3, §6.5).
pub fn run_pdfs_to_images<FProg, FItem, FFin>(
    state: &AppState,
    ids: &[u64],
    page_set: &PageSet,
    format: RenderFormatChoice,
    dpi: u32,
    output_dir: &Path,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    if !DPI_CHOICES.contains(&dpi) {
        return Err(IpcError::from_code(ErrorCode::InvalidParams));
    }

    let on_progress = Arc::new(callbacks.on_progress);
    let on_item = Arc::new(callbacks.on_item);
    let on_finished = callbacks.on_finished;

    let existing = list_existing_files(output_dir)?;

    let mut naming_inputs = Vec::new();
    let mut initial_tasks = Vec::new();

    for &id in ids {
        let entry = state
            .items
            .get(id)
            .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
        if entry.kind != ItemKind::Pdf {
            continue;
        }

        let filename = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let pages = if entry.error.is_none() {
            page_set.pages_within(entry.page_count)
        } else {
            Vec::new()
        };

        naming_inputs.push(PdfToImageInput {
            filename: filename.clone(),
            total_pages: entry.page_count,
            pages: pages.clone(),
        });

        initial_tasks.push((id, filename, entry.path, entry.error, pages));
    }

    let resolved_names_nested =
        resolve_pdf_to_image_names(&naming_inputs, format.extension(), &existing);

    let mut initial_used: HashSet<String> = existing.iter().map(|s| s.to_lowercase()).collect();
    for names in &resolved_names_nested {
        for name in names {
            initial_used.insert(name.to_lowercase());
        }
    }
    let used_names_lower = Arc::new(Mutex::new(initial_used));

    let mut total_pages_all = 0u32;
    let mut tasks = VecDeque::new();

    for ((id, filename, path, initial_error, pages), planned_names) in
        initial_tasks.into_iter().zip(resolved_names_nested)
    {
        total_pages_all = total_pages_all.saturating_add(pages.len() as u32);
        tasks.push_back(PdfConversionTask {
            id,
            filename,
            path,
            initial_error,
            pages,
            planned_names,
        });
    }

    on_progress(JobProgressPayload {
        done: 0,
        total: total_pages_all,
        current: None,
    });

    if tasks.is_empty() {
        on_finished(JobFinishedPayload {
            succeeded: 0,
            failed: 0,
            no_pages: 0,
            unprocessed: 0,
            cancelled: state.cancel_flag.load(Ordering::SeqCst),
        });
        return Ok(());
    }

    let total_items = tasks.len() as u32;
    let queue = Arc::new(Mutex::new(tasks));
    let done_pages = Arc::new(AtomicU32::new(0));
    let succeeded_count = Arc::new(AtomicU32::new(0));
    let failed_count = Arc::new(AtomicU32::new(0));
    let no_pages_count = Arc::new(AtomicU32::new(0));
    let unprocessed_count = Arc::new(AtomicU32::new(0));
    let write_failed = Arc::new(AtomicBool::new(false));

    let num_threads = default_worker_limit().min(total_items as usize).max(1);
    let mut handles = Vec::with_capacity(num_threads);
    let output_dir_buf = output_dir.to_path_buf();

    for _ in 0..num_threads {
        let queue = Arc::clone(&queue);
        let done_pages = Arc::clone(&done_pages);
        let succeeded_count = Arc::clone(&succeeded_count);
        let failed_count = Arc::clone(&failed_count);
        let no_pages_count = Arc::clone(&no_pages_count);
        let unprocessed_count = Arc::clone(&unprocessed_count);
        let cancel_flag = Arc::clone(&state.cancel_flag);
        let write_failed = Arc::clone(&write_failed);
        let used_names = Arc::clone(&used_names_lower);
        let pool = state.pool.clone();
        let output_dir = output_dir_buf.clone();
        let on_progress = Arc::clone(&on_progress);
        let on_item = Arc::clone(&on_item);

        handles.push(std::thread::spawn(move || {
            loop {
                if cancel_flag.load(Ordering::SeqCst) || write_failed.load(Ordering::SeqCst) {
                    let mut q = match queue.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    unprocessed_count.fetch_add(q.len() as u32, Ordering::SeqCst);
                    q.clear();
                    break;
                }

                let task = {
                    let mut q = match queue.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    q.pop_front()
                };

                let Some(task) = task else {
                    break;
                };

                // If entry already failed probe / initial open
                if let Some(err) = task.initial_error {
                    failed_count.fetch_add(1, Ordering::SeqCst);
                    on_item(JobItemPayload {
                        id: task.id,
                        status: JobItemStatus::Failed,
                        outputs: Vec::new(),
                        error: Some(err),
                        failed_pages: None,
                    });
                    continue;
                }

                // If no pages match the page range for this PDF (design §4.5, §6.6)
                if task.pages.is_empty() {
                    no_pages_count.fetch_add(1, Ordering::SeqCst);
                    on_item(JobItemPayload {
                        id: task.id,
                        status: JobItemStatus::NoPages,
                        outputs: Vec::new(),
                        error: None,
                        failed_pages: None,
                    });
                    continue;
                }

                let mut worker = match pool.acquire() {
                    Ok(w) => w,
                    Err(err) => {
                        failed_count.fetch_add(1, Ordering::SeqCst);
                        on_item(JobItemPayload {
                            id: task.id,
                            status: JobItemStatus::Failed,
                            outputs: Vec::new(),
                            error: Some(IpcError::from(&err)),
                            failed_pages: None,
                        });
                        continue;
                    }
                };

                let open_result = worker.send(&Request::Open {
                    path: task.path.clone(),
                });

                if let Err(err) = open_result {
                    failed_count.fetch_add(1, Ordering::SeqCst);
                    on_item(JobItemPayload {
                        id: task.id,
                        status: JobItemStatus::Failed,
                        outputs: Vec::new(),
                        error: Some(IpcError::from(&err)),
                        failed_pages: None,
                    });
                    continue;
                }

                let mut outputs = Vec::new();
                let mut failed_pages = Vec::new();
                let mut last_error = None;
                let mut was_cancelled = false;

                for (&page, planned_name) in task.pages.iter().zip(&task.planned_names) {
                    // Check cancellation before starting next page per design §6.5
                    if cancel_flag.load(Ordering::SeqCst) {
                        was_cancelled = true;
                        break;
                    }

                    on_progress(JobProgressPayload {
                        done: done_pages.load(Ordering::SeqCst),
                        total: total_pages_all,
                        current: Some(task.filename.clone()),
                    });

                    let render_result = worker.send(&Request::Render {
                        page,
                        dpi,
                        format: format.into(),
                    });

                    match render_result {
                        Ok((Response::Render, image_bytes)) => {
                            let mut stop = false;
                            match save_atomic(&output_dir, planned_name, &image_bytes, &used_names)
                            {
                                Ok(saved_name) => {
                                    outputs.push(saved_name);
                                }
                                Err(err) => {
                                    stop = stop_after_write_failure(&err, &write_failed);
                                    failed_pages.push(page);
                                    last_error = Some(err);
                                }
                            }
                            let cur = done_pages.fetch_add(1, Ordering::SeqCst) + 1;
                            on_progress(JobProgressPayload {
                                done: cur,
                                total: total_pages_all,
                                current: None,
                            });
                            if stop {
                                // The folder is the same for every page, so
                                // the next ones would fail the same way.
                                break;
                            }
                        }
                        Err(WorkerPoolError::Remote { code, detail }) => {
                            failed_pages.push(page);
                            last_error = Some(IpcError::from_worker(&code, detail.as_deref()));
                            let cur = done_pages.fetch_add(1, Ordering::SeqCst) + 1;
                            on_progress(JobProgressPayload {
                                done: cur,
                                total: total_pages_all,
                                current: None,
                            });
                        }
                        Err(pool_err) => {
                            // Worker died or timed out: do not retry this PDF per design §5.2
                            failed_pages.push(page);
                            last_error = Some(IpcError::from(&pool_err));
                            let cur = done_pages.fetch_add(1, Ordering::SeqCst) + 1;
                            on_progress(JobProgressPayload {
                                done: cur,
                                total: total_pages_all,
                                current: None,
                            });
                            break;
                        }
                        _ => {
                            failed_pages.push(page);
                            last_error = Some(IpcError::new(
                                ErrorCode::WorkerCrashed,
                                "unexpected worker response",
                            ));
                            let cur = done_pages.fetch_add(1, Ordering::SeqCst) + 1;
                            on_progress(JobProgressPayload {
                                done: cur,
                                total: total_pages_all,
                                current: None,
                            });
                            break;
                        }
                    }
                }

                let _ = worker.send(&Request::Close);

                if was_cancelled {
                    unprocessed_count.fetch_add(1, Ordering::SeqCst);
                    on_item(JobItemPayload {
                        id: task.id,
                        status: JobItemStatus::Cancelled,
                        outputs,
                        error: None,
                        // Pages that failed before the cancel still count as
                        // reached, so the screen must be told of them.
                        failed_pages: (!failed_pages.is_empty()).then_some(failed_pages),
                    });
                } else if failed_pages.is_empty() && outputs.len() == task.pages.len() {
                    succeeded_count.fetch_add(1, Ordering::SeqCst);
                    on_item(JobItemPayload {
                        id: task.id,
                        status: JobItemStatus::Ok,
                        outputs,
                        error: None,
                        failed_pages: None,
                    });
                } else if !outputs.is_empty() {
                    // Partial failure: at least one page succeeded, some failed
                    failed_count.fetch_add(1, Ordering::SeqCst);
                    on_item(JobItemPayload {
                        id: task.id,
                        status: JobItemStatus::Partial,
                        outputs,
                        error: last_error,
                        failed_pages: Some(failed_pages),
                    });
                } else {
                    failed_count.fetch_add(1, Ordering::SeqCst);
                    on_item(JobItemPayload {
                        id: task.id,
                        status: JobItemStatus::Failed,
                        outputs: Vec::new(),
                        error: last_error,
                        failed_pages: Some(failed_pages),
                    });
                }
            }
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    let accounted = succeeded_count.load(Ordering::SeqCst)
        + failed_count.load(Ordering::SeqCst)
        + no_pages_count.load(Ordering::SeqCst)
        + unprocessed_count.load(Ordering::SeqCst);
    if accounted < total_items {
        failed_count.fetch_add(total_items - accounted, Ordering::SeqCst);
    }

    on_finished(JobFinishedPayload {
        succeeded: succeeded_count.load(Ordering::SeqCst),
        failed: failed_count.load(Ordering::SeqCst),
        no_pages: no_pages_count.load(Ordering::SeqCst),
        unprocessed: unprocessed_count.load(Ordering::SeqCst),
        cancelled: state.cancel_flag.load(Ordering::SeqCst),
    });

    Ok(())
}

/// Prepares and starts image-to-PDF conversion in the background, validating state and parameters.
pub fn start_images_to_pdfs_internal<FProg, FItem, FFin>(
    state: &AppState,
    ids: &[u64],
    page_size: PageSizeChoice,
    a4_orientation: A4OrientationChoice,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    // Check conversion running first per design §6.5, §7.1
    if state
        .is_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    state.cancel_flag.store(false, Ordering::SeqCst);

    let output_dir = {
        let lock = state.images_output_dir.lock().expect("output_dir lock");
        match lock.clone() {
            Some(dir) => dir,
            None => {
                state.is_running.store(false, Ordering::SeqCst);
                return Err(IpcError::from_code(ErrorCode::InvalidParams));
            }
        }
    };

    if let Err(err) = check_output_dir(state, OutputKind::ImagesToPdf, &output_dir) {
        state.is_running.store(false, Ordering::SeqCst);
        return Err(err);
    }

    for &id in ids {
        if state.items.get(id).is_none() {
            state.is_running.store(false, Ordering::SeqCst);
            return Err(IpcError::from_code(ErrorCode::UnknownHandle));
        }
    }

    if let Err(err) = list_existing_files(&output_dir) {
        state.is_running.store(false, Ordering::SeqCst);
        return Err(err);
    }

    let state_clone = state.clone();
    let ids_owned = ids.to_vec();

    std::thread::spawn(move || {
        let running_guard = RunningGuard::new(Arc::clone(&state_clone.is_running));
        let on_finished_cell = Arc::new(Mutex::new(Some(
            running_guard.wrap_on_finished(callbacks.on_finished),
        )));
        let on_finished_for_run = {
            let cell = Arc::clone(&on_finished_cell);
            move |finished: JobFinishedPayload| {
                if let Ok(mut lock) = cell.lock()
                    && let Some(cb) = lock.take()
                {
                    cb(finished);
                }
            }
        };
        let wrapped_callbacks = JobCallbacks {
            on_progress: callbacks.on_progress,
            on_item: callbacks.on_item,
            on_finished: on_finished_for_run,
        };
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_images_to_pdfs(
                &state_clone,
                &ids_owned,
                page_size,
                a4_orientation,
                &output_dir,
                wrapped_callbacks,
            )
        }));
        if (res.is_err() || matches!(res, Ok(Err(_))))
            && let Ok(mut lock) = on_finished_cell.lock()
            && let Some(cb) = lock.take()
        {
            cb(JobFinishedPayload {
                succeeded: 0,
                failed: ids_owned.len() as u32,
                no_pages: 0,
                unprocessed: 0,
                cancelled: state_clone.cancel_flag.load(Ordering::SeqCst),
            });
        }
    });

    Ok(())
}

/// Prepares and starts PDF-to-image conversion in the background, validating state and parameters.
pub fn start_pdfs_to_images_internal<FProg, FItem, FFin>(
    state: &AppState,
    ids: &[u64],
    range: &str,
    format: RenderFormatChoice,
    dpi: u32,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    // Check conversion running first per design §6.5, §7.1
    if state
        .is_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    state.cancel_flag.store(false, Ordering::SeqCst);

    if !DPI_CHOICES.contains(&dpi) {
        state.is_running.store(false, Ordering::SeqCst);
        return Err(IpcError::from_code(ErrorCode::InvalidParams));
    }

    let page_set = match parse_page_range(range) {
        Ok(ps) => ps,
        Err(e) => {
            state.is_running.store(false, Ordering::SeqCst);
            return Err(IpcError::new(ErrorCode::InvalidPageRange, e.detail));
        }
    };

    let output_dir = {
        let lock = state.pdfs_output_dir.lock().expect("output_dir lock");
        match lock.clone() {
            Some(dir) => dir,
            None => {
                state.is_running.store(false, Ordering::SeqCst);
                return Err(IpcError::from_code(ErrorCode::InvalidParams));
            }
        }
    };

    if let Err(err) = check_output_dir(state, OutputKind::PdfToImages, &output_dir) {
        state.is_running.store(false, Ordering::SeqCst);
        return Err(err);
    }

    for &id in ids {
        if state.items.get(id).is_none() {
            state.is_running.store(false, Ordering::SeqCst);
            return Err(IpcError::from_code(ErrorCode::UnknownHandle));
        }
    }

    if let Err(err) = list_existing_files(&output_dir) {
        state.is_running.store(false, Ordering::SeqCst);
        return Err(err);
    }

    let state_clone = state.clone();
    let ids_owned = ids.to_vec();

    std::thread::spawn(move || {
        let running_guard = RunningGuard::new(Arc::clone(&state_clone.is_running));
        let on_finished_cell = Arc::new(Mutex::new(Some(
            running_guard.wrap_on_finished(callbacks.on_finished),
        )));
        let on_finished_for_run = {
            let cell = Arc::clone(&on_finished_cell);
            move |finished: JobFinishedPayload| {
                if let Ok(mut lock) = cell.lock()
                    && let Some(cb) = lock.take()
                {
                    cb(finished);
                }
            }
        };
        let wrapped_callbacks = JobCallbacks {
            on_progress: callbacks.on_progress,
            on_item: callbacks.on_item,
            on_finished: on_finished_for_run,
        };
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_pdfs_to_images(
                &state_clone,
                &ids_owned,
                &page_set,
                format,
                dpi,
                &output_dir,
                wrapped_callbacks,
            )
        }));
        if (res.is_err() || matches!(res, Ok(Err(_))))
            && let Ok(mut lock) = on_finished_cell.lock()
            && let Some(cb) = lock.take()
        {
            cb(JobFinishedPayload {
                succeeded: 0,
                failed: ids_owned.len() as u32,
                no_pages: 0,
                unprocessed: 0,
                cancelled: state_clone.cancel_flag.load(Ordering::SeqCst),
            });
        }
    });

    Ok(())
}
