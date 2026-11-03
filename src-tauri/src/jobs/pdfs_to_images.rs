//! Converting PDFs to images and checking a page range per design §6.3, §6.5, §7.1.

use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use pdfconv_core::naming::{PdfToImageInput, resolve_pdf_to_image_names};
use pdfconv_core::{ErrorCode, PageSet, parse_page_range};
use pdfconv_worker::is_render_dpi_in_range;
use pdfconv_worker::protocol::{Request, Response};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::options::{RenderColorChoice, RenderFormatChoice};
use super::output::{
    chosen_output_dir, list_existing_files, save_atomic, stop_after_write_failure,
};
use super::queue::{TaskOutcome, run_queue};
use super::{
    JobCallbacks, JobFinishedPayload, JobItemPayload, JobItemStatus, JobProgressPayload,
    begin_conversion, spawn_conversion,
};
use crate::AppState;
use crate::error::IpcError;
use crate::items::{ItemKind, file_name};
use crate::settings::OutputKind;
use crate::worker_pool::WorkerPoolError;

/// Result of checking a page range across candidate PDFs (design §7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CheckPageRangeResult {
    #[ts(type = "number")]
    pub total_pages: u32,
    /// The range as sorted, non-overlapping `[start, end]` page intervals, so
    /// the screen can mark pages without parsing the text itself (design
    /// §4.5, §6.3). Its length follows the text, not the pages it covers.
    /// `n-end` reaches 4294967295, which each PDF clamps to its page count.
    #[ts(type = "Array<[number, number]>")]
    pub intervals: Vec<(u32, u32)>,
    /// Whether `end` stood alone, adding each PDF's last page to `intervals`.
    pub last_page: bool,
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
        last_page: page_set.last_page(),
    })
}

/// Prepared input and output metadata for a single PDF document in PDF-to-image conversion.
struct PdfConversionTask {
    id: u64,
    filename: String,
    open_request: Request,
    initial_error: Option<IpcError>,
    pages: Vec<u32>,
    planned_names: Vec<String>,
}

/// Executes PDF-to-image conversion across multiple PDFs (design §6.3, §6.5).
// Parameters mirror the conversion options and callbacks needed for the multi-step pipeline.
#[allow(clippy::too_many_arguments)]
pub fn run_pdfs_to_images<FProg, FItem, FFin>(
    state: &AppState,
    ids: &[u64],
    page_set: &PageSet,
    format: RenderFormatChoice,
    color_mode: RenderColorChoice,
    dpi: u32,
    output_dir: &Path,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    if !is_render_dpi_in_range(dpi) {
        return Err(IpcError::from_code(ErrorCode::InvalidParams));
    }

    let on_progress = callbacks.on_progress;
    let on_item = callbacks.on_item;
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

        let filename = file_name(&entry.path);

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

        initial_tasks.push((id, filename, entry.open_request(), entry.error, pages));
    }

    let resolved_names_nested =
        resolve_pdf_to_image_names(&naming_inputs, format.extension(), &existing);

    let mut initial_used: HashSet<String> = existing.iter().map(|s| s.to_lowercase()).collect();
    for names in &resolved_names_nested {
        for name in names {
            initial_used.insert(name.to_lowercase());
        }
    }
    let used_names_lower = Mutex::new(initial_used);

    let mut total_pages_all = 0u32;
    let mut tasks = VecDeque::new();

    for ((id, filename, open_request, initial_error, pages), planned_names) in
        initial_tasks.into_iter().zip(resolved_names_nested)
    {
        total_pages_all = total_pages_all.saturating_add(pages.len() as u32);
        tasks.push_back(PdfConversionTask {
            id,
            filename,
            open_request,
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

    let write_failed = AtomicBool::new(false);
    let done_pages = AtomicU32::new(0);
    let cancel_flag = &*state.cancel_flag;

    let tally = run_queue(cancel_flag, &write_failed, tasks, |task| {
        // If entry already failed probe / initial open
        if let Some(err) = task.initial_error {
            on_item(JobItemPayload::failed(task.id, err));
            return TaskOutcome::Failed;
        }

        // If no pages match the page range for this PDF (design §4.5, §6.6)
        if task.pages.is_empty() {
            on_item(JobItemPayload::no_pages(task.id));
            return TaskOutcome::NoPages;
        }

        let mut worker = match state.pool.acquire() {
            Ok(w) => w,
            Err(err) => {
                on_item(JobItemPayload::failed(task.id, IpcError::from(&err)));
                return TaskOutcome::Failed;
            }
        };

        let open_result = worker.send(&task.open_request);

        if let Err(err) = open_result {
            on_item(JobItemPayload::failed(task.id, IpcError::from(&err)));
            return TaskOutcome::Failed;
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
                color: color_mode.into(),
            });

            match render_result {
                Ok((Response::Render, image_bytes)) => {
                    let mut stop = false;
                    match save_atomic(output_dir, planned_name, &image_bytes, &used_names_lower) {
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
                    last_error = Some(IpcError::from_worker(code, detail.as_deref()));
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
            on_item(JobItemPayload {
                id: task.id,
                status: JobItemStatus::Cancelled,
                outputs,
                error: None,
                // Pages that failed before the cancel still count as
                // reached, so the screen must be told of them.
                failed_pages: (!failed_pages.is_empty()).then_some(failed_pages),
                source_id: None,
            });
            TaskOutcome::Unprocessed
        } else if failed_pages.is_empty() && outputs.len() == task.pages.len() {
            on_item(JobItemPayload::ok(task.id, outputs));
            TaskOutcome::Succeeded
        } else if !outputs.is_empty() {
            // Partial failure: at least one page succeeded, some failed
            on_item(JobItemPayload {
                id: task.id,
                status: JobItemStatus::Partial,
                outputs,
                error: last_error,
                failed_pages: Some(failed_pages),
                source_id: None,
            });
            TaskOutcome::Failed
        } else {
            on_item(JobItemPayload {
                id: task.id,
                status: JobItemStatus::Failed,
                outputs: Vec::new(),
                error: last_error,
                failed_pages: Some(failed_pages),
                source_id: None,
            });
            TaskOutcome::Failed
        }
    });

    on_finished(JobFinishedPayload {
        succeeded: tally.succeeded,
        failed: tally.failed,
        no_pages: tally.no_pages,
        unprocessed: tally.unprocessed,
        cancelled: cancel_flag.load(Ordering::SeqCst),
    });

    Ok(())
}

/// Prepares and starts PDF-to-image conversion in the background, validating state and parameters.
pub fn start_pdfs_to_images_internal<FProg, FItem, FFin>(
    state: &AppState,
    ids: &[u64],
    range: &str,
    format: RenderFormatChoice,
    color_mode: RenderColorChoice,
    dpi: u32,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    let guard = begin_conversion(state)?;

    if !is_render_dpi_in_range(dpi) {
        return Err(IpcError::from_code(ErrorCode::InvalidParams));
    }
    let page_set = parse_page_range(range)
        .map_err(|e| IpcError::new(ErrorCode::InvalidPageRange, e.detail))?;

    let output_dir = chosen_output_dir(state, OutputKind::PdfToImages)?;
    if ids.iter().any(|&id| state.items.get(id).is_none()) {
        return Err(IpcError::from_code(ErrorCode::UnknownHandle));
    }
    list_existing_files(&output_dir)?;

    let ids = ids.to_vec();
    let failed_if_it_panics = ids.len() as u32;
    spawn_conversion(
        state,
        guard,
        callbacks.on_finished,
        failed_if_it_panics,
        move |state, finish| {
            run_pdfs_to_images(
                state,
                &ids,
                &page_set,
                format,
                color_mode,
                dpi,
                &output_dir,
                JobCallbacks {
                    on_progress: callbacks.on_progress,
                    on_item: callbacks.on_item,
                    on_finished: finish,
                },
            )
        },
    );
    Ok(())
}
