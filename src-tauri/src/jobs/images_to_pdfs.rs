//! Converting each image to a PDF of its own per design §6.2, §6.5.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use pdfconv_core::naming::resolve_image_to_pdf_names;
use pdfconv_core::{AddImageResult, PdfConformance, PdfWriter};

use super::image_check::{ImageCheckStop, check_image};
use super::options::{
    PageOrientationChoice, PageSizeChoice, PdfOutput, creator, output_password, protect_pdf,
};
use super::output::{
    chosen_output_dir, list_existing_files, save_atomic, stop_after_write_failure,
};
use super::queue::{TaskOutcome, run_queue};
use super::{
    JobCallbacks, JobFinishedPayload, JobItemPayload, JobProgressPayload, begin_conversion,
    spawn_conversion,
};
use crate::AppState;
use crate::decode_budget::pdf_write_estimate;
use crate::error::{ErrorCode, IpcError};
use crate::items::{ItemKind, file_name};
use crate::settings::OutputKind;

/// Executes individual conversion of multiple images into separate PDF documents (design §6.2).
pub fn run_images_to_pdfs<FProg, FItem, FFin>(
    state: &AppState,
    ids: &[u64],
    page_size: PageSizeChoice,
    orientation: PageOrientationChoice,
    output: &PdfOutput,
    output_dir: &Path,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    let on_progress = callbacks.on_progress;
    let on_item = callbacks.on_item;
    let on_finished = callbacks.on_finished;

    let existing = list_existing_files(output_dir)?;

    struct ImageTask {
        id: u64,
        filename: String,
        path: PathBuf,
        page_count: u32,
        decoded_bytes: u64,
        initial_error: Option<IpcError>,
        initial_output_name: Option<String>,
    }

    let mut target_names = Vec::new();
    let mut initial_tasks = Vec::new();
    let mut total_pages = 0u32;

    for &id in ids {
        let entry = state
            .items
            .get(id)
            .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
        if entry.kind != ItemKind::Image {
            continue;
        }
        let filename = file_name(&entry.path);
        if entry.error.is_none() {
            target_names.push(filename.clone());
            total_pages = total_pages.saturating_add(entry.page_count);
        }
        initial_tasks.push((
            id,
            filename,
            entry.path,
            entry.page_count,
            entry.decoded_bytes,
            entry.error,
        ));
    }

    let resolved_names = resolve_image_to_pdf_names(&target_names, &existing);

    let mut initial_used: HashSet<String> = existing.iter().map(|s| s.to_lowercase()).collect();
    for name in &resolved_names {
        initial_used.insert(name.to_lowercase());
    }
    let used_names_lower = Mutex::new(initial_used);

    let mut resolved_iter = resolved_names.into_iter();
    let tasks: VecDeque<ImageTask> = initial_tasks
        .into_iter()
        .map(
            |(id, filename, path, page_count, decoded_bytes, initial_error)| {
                let initial_output_name = if initial_error.is_none() {
                    resolved_iter.next()
                } else {
                    None
                };
                ImageTask {
                    id,
                    filename,
                    path,
                    page_count,
                    decoded_bytes,
                    initial_error,
                    initial_output_name,
                }
            },
        )
        .collect();

    let total = total_pages;
    on_progress(JobProgressPayload {
        done: 0,
        total,
        current: None,
    });

    let done = AtomicU32::new(0);
    let write_failed = AtomicBool::new(false);
    let cancel_flag = &*state.cancel_flag;
    let creator = creator();
    let should_stop = || cancel_flag.load(Ordering::SeqCst) || write_failed.load(Ordering::SeqCst);

    let tally = run_queue(cancel_flag, &write_failed, tasks, |task| {
        if let Some(err) = task.initial_error {
            on_item(JobItemPayload::failed(task.id, err));
            return TaskOutcome::Failed;
        }

        let output_name = task.initial_output_name.as_deref().unwrap_or_default();

        on_progress(JobProgressPayload {
            done: done.load(Ordering::SeqCst),
            total,
            current: Some(task.filename.clone()),
        });

        // The pixels are read in a worker first, so that a decoder that stops or
        // crashes is contained there (design §6.2). A failure is this image's, as an
        // unreadable file is; a cancel leaves the image unstarted, like the ones still
        // in the queue.
        match check_image(&state.pool, &task.path, &should_stop) {
            Ok(()) => {}
            Err(ImageCheckStop::Failed(error)) => {
                let cur_done = done.fetch_add(task.page_count, Ordering::SeqCst) + task.page_count;
                on_item(JobItemPayload::failed(task.id, error));
                on_progress(JobProgressPayload {
                    done: cur_done,
                    total,
                    current: None,
                });
                return TaskOutcome::Failed;
            }
            Err(ImageCheckStop::Cancelled) => return TaskOutcome::Unprocessed,
        }

        // Borrowed before the file is read and held until this image's PDF is written
        // and its bytes are let go (design §5.3). A wait that is cancelled leaves the
        // image unstarted, like the ones still in the queue.
        let Some(_permit) = state.decode_budget.acquire(
            pdf_write_estimate(&task.path, task.decoded_bytes),
            &should_stop,
        ) else {
            return TaskOutcome::Unprocessed;
        };

        let bytes = match std::fs::read(&task.path) {
            Ok(b) => b,
            Err(_) => {
                let cur_done = done.fetch_add(task.page_count, Ordering::SeqCst) + task.page_count;
                on_item(JobItemPayload::failed(
                    task.id,
                    IpcError::from_code(ErrorCode::ReadFailed),
                ));
                on_progress(JobProgressPayload {
                    done: cur_done,
                    total,
                    current: None,
                });
                return TaskOutcome::Failed;
            }
        };

        let mut writer = PdfWriter::new(&creator, output.conformance);
        let mut item_pages_done = 0u32;
        let add_res = writer.add_checked_image(
            bytes,
            page_size.to_core(orientation),
            |_page_idx, _total_in_item| {
                item_pages_done += 1;
                let cur = done.fetch_add(1, Ordering::SeqCst) + 1;
                on_progress(JobProgressPayload {
                    done: cur,
                    total,
                    current: Some(task.filename.clone()),
                });
            },
            should_stop,
        );

        match add_res {
            Ok(AddImageResult::Cancelled) => {
                // Only the user's cancel is the image's own. One stopped by another
                // thread's write failure is unprocessed, as the images still in the
                // queue are; which of the images in flight it catches depends on how
                // far each thread has got, so it must not show as an item.
                if cancel_flag.load(Ordering::SeqCst) {
                    on_item(JobItemPayload::cancelled(task.id));
                }
                return TaskOutcome::Unprocessed;
            }
            Err(probe_err) => {
                let remaining = task.page_count.saturating_sub(item_pages_done);
                let cur_done = done.fetch_add(remaining, Ordering::SeqCst) + remaining;
                on_item(JobItemPayload::failed(task.id, IpcError::from(probe_err)));
                on_progress(JobProgressPayload {
                    done: cur_done,
                    total,
                    current: None,
                });
                return TaskOutcome::Failed;
            }
            Ok(AddImageResult::Ok { .. }) => {}
        }

        let pdf_bytes = writer
            .finish()
            .map_err(IpcError::write_failed)
            .and_then(|bytes| protect_pdf(bytes, output.password.as_ref()));
        let process_result = pdf_bytes.and_then(|bytes| {
            save_atomic(output_dir, output_name, &bytes, &used_names_lower).inspect_err(|err| {
                stop_after_write_failure(err, &write_failed);
            })
        });

        let outcome = match process_result {
            Ok(saved_name) => {
                on_item(JobItemPayload::ok(task.id, vec![saved_name]));
                TaskOutcome::Succeeded
            }
            Err(err) => {
                on_item(JobItemPayload::failed(task.id, err));
                TaskOutcome::Failed
            }
        };
        on_progress(JobProgressPayload {
            done: done.load(Ordering::SeqCst),
            total,
            current: None,
        });
        outcome
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

/// Prepares and starts image-to-PDF conversion in the background, validating state and parameters.
pub fn start_images_to_pdfs_internal<FProg, FItem, FFin>(
    state: &AppState,
    ids: &[u64],
    page_size: PageSizeChoice,
    orientation: PageOrientationChoice,
    conformance: PdfConformance,
    password: Option<String>,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    // Nothing has started yet, so a refused password leaves no state to undo (design §7.1).
    let output = PdfOutput::new(conformance, output_password(conformance, password)?);

    let guard = begin_conversion(state)?;

    let output_dir = chosen_output_dir(state, OutputKind::ImagesToPdf)?;
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
            run_images_to_pdfs(
                state,
                &ids,
                page_size,
                orientation,
                &output,
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
