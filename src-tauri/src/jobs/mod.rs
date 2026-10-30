//! Batch conversion execution and state management per design §6.2, §6.3, §6.4, §6.5.
//! The conversions and the output writing are in the submodules.

mod images_to_pdfs;
mod merged;
mod options;
mod output;
mod pdfs_to_images;

pub use images_to_pdfs::{run_images_to_pdfs, start_images_to_pdfs_internal};
pub use merged::{SaveMergedPdfResult, run_save_merged_pdf};
pub use options::{
    PageOrientationChoice, PageSizeChoice, PdfOutput, RenderColorChoice, RenderFormatChoice,
    output_password, pdf_conformance,
};
pub(crate) use output::create_output_temp;
pub use output::{check_output_dir, list_existing_files, persist_unique, save_atomic};
pub use pdfs_to_images::{
    CheckPageRangeResult, check_page_range_internal, run_pdfs_to_images,
    start_pdfs_to_images_internal,
};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::AppState;
use crate::error::{ErrorCode, IpcError};

/// Name of the event emitted for job progress updates (design §7.2).
pub const JOB_PROGRESS_EVENT: &str = "job-progress";

/// Name of the event emitted when an item completes (design §7.2).
pub const JOB_ITEM_EVENT: &str = "job-item";

/// Name of the event emitted when a job finishes (design §7.2).
pub const JOB_FINISHED_EVENT: &str = "job-finished";

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
    /// The organize PDF an output failed on (design organize §4); `None` in the other jobs.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub source_id: Option<u64>,
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

/// Marks a conversion as running and clears the cancel flag (design §6.5,
/// §7.1). The guard puts `is_running` back when it is dropped, so a start that
/// is refused afterwards needs no cleanup of its own.
///
/// # Errors
///
/// `ConversionRunning` if a conversion is running already.
pub(crate) fn begin_conversion(state: &AppState) -> Result<RunningGuard, IpcError> {
    if state
        .is_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    state.cancel_flag.store(false, Ordering::SeqCst);
    Ok(RunningGuard::new(Arc::clone(&state.is_running)))
}

/// Sends the `job-finished` of a conversion, once.
pub(crate) type FinishJob = Box<dyn FnOnce(JobFinishedPayload) + Send + Sync>;

/// Runs `run` on a thread of its own as the conversion that `guard` marks.
/// `run` must send the end of the job through the [`FinishJob`] it is given;
/// `is_running` is put back before the callback is called. If `run` panics,
/// or returns an error without having sent the end, a `job-finished` with
/// `failed_if_it_panics` failures is sent instead, so the screen never waits
/// for a job that is gone.
pub(crate) fn spawn_conversion<FFin, Run>(
    state: &AppState,
    guard: RunningGuard,
    on_finished: FFin,
    failed_if_it_panics: u32,
    run: Run,
) where
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
    Run: FnOnce(&AppState, FinishJob) -> Result<(), IpcError> + Send + 'static,
{
    let state = state.clone();
    std::thread::spawn(move || {
        let pending = Arc::new(Mutex::new(Some(guard.wrap_on_finished(on_finished))));
        let finish: FinishJob = {
            let pending = Arc::clone(&pending);
            Box::new(move |payload| {
                if let Ok(mut slot) = pending.lock()
                    && let Some(finish) = slot.take()
                {
                    finish(payload);
                }
            })
        };
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&state, finish)));
        if !matches!(outcome, Ok(Ok(())))
            && let Ok(mut slot) = pending.lock()
            && let Some(finish) = slot.take()
        {
            finish(JobFinishedPayload {
                succeeded: 0,
                failed: failed_if_it_panics,
                no_pages: 0,
                unprocessed: 0,
                cancelled: state.cancel_flag.load(Ordering::SeqCst),
            });
        }
        // Dropping the guard puts `is_running` back if `run` never finished.
        drop(guard);
    });
}
