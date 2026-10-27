//! Converting each image to a PDF of its own per design §6.2, §6.5.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use pdfconv_core::naming::resolve_image_to_pdf_names;
use pdfconv_core::{AddImageResult, PdfConformance, PdfWriter};

use super::options::{
    PageOrientationChoice, PageSizeChoice, PdfOutput, output_password, protect_pdf,
};
use super::output::{check_output_dir, list_existing_files, save_atomic, stop_after_write_failure};
use super::{
    JobCallbacks, JobFinishedPayload, JobItemPayload, JobItemStatus, JobProgressPayload,
    begin_conversion, spawn_conversion,
};
use crate::AppState;
use crate::decode_budget::DECODE_MEMORY_FACTOR_PDF;
use crate::error::{ErrorCode, IpcError};
use crate::items::ItemKind;
use crate::settings::OutputKind;
use crate::worker_pool::default_worker_limit;

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
    let on_progress = Arc::new(callbacks.on_progress);
    let on_item = Arc::new(callbacks.on_item);
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
        let filename = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
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
    let used_names_lower = Arc::new(Mutex::new(initial_used));

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

    let total_items = tasks.len() as u32;
    let total = total_pages;
    on_progress(JobProgressPayload {
        done: 0,
        total,
        current: None,
    });

    if total_items == 0 {
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

    let num_threads = default_worker_limit().min(total_items as usize).max(1);
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
        let decode_budget = Arc::clone(&state.decode_budget);
        let write_failed = Arc::clone(&write_failed);
        let used_names = Arc::clone(&used_names_lower);
        let output_dir = output_dir_buf.clone();
        let creator = creator.clone();
        let output = output.clone();
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
                    on_item(JobItemPayload {
                        id: task.id,
                        status: JobItemStatus::Failed,
                        outputs: Vec::new(),
                        error: Some(err),
                        failed_pages: None,
                    });
                    continue;
                }

                let output_name = task.initial_output_name.as_deref().unwrap_or_default();

                on_progress(JobProgressPayload {
                    done: done.load(Ordering::SeqCst),
                    total,
                    current: Some(task.filename.clone()),
                });

                // Borrowed before the file is read and held until this image's PDF is written
                // and its bytes are let go (design §5.3). A wait that is cancelled leaves the
                // image unstarted, like the ones still in the queue.
                let Some(_permit) = decode_budget.acquire(
                    task.decoded_bytes.saturating_mul(DECODE_MEMORY_FACTOR_PDF),
                    &|| cancel_flag.load(Ordering::SeqCst) || write_failed.load(Ordering::SeqCst),
                ) else {
                    let mut q = match queue.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    unprocessed.fetch_add(1 + q.len() as u32, Ordering::SeqCst);
                    q.clear();
                    break;
                };

                let bytes = match std::fs::read(&task.path) {
                    Ok(b) => b,
                    Err(_) => {
                        let cur_done =
                            done.fetch_add(task.page_count, Ordering::SeqCst) + task.page_count;
                        failed.fetch_add(1, Ordering::SeqCst);
                        on_item(JobItemPayload {
                            id: task.id,
                            status: JobItemStatus::Failed,
                            outputs: Vec::new(),
                            error: Some(IpcError::from_code(ErrorCode::ReadFailed)),
                            failed_pages: None,
                        });
                        on_progress(JobProgressPayload {
                            done: cur_done,
                            total,
                            current: None,
                        });
                        continue;
                    }
                };

                let mut writer = PdfWriter::new(&creator, output.conformance);
                let mut item_pages_done = 0u32;
                let add_res = writer.add_image(
                    &bytes,
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
                    || cancel_flag.load(Ordering::SeqCst) || write_failed.load(Ordering::SeqCst),
                );

                match add_res {
                    Ok(AddImageResult::Cancelled) => {
                        unprocessed.fetch_add(1, Ordering::SeqCst);
                        on_item(JobItemPayload {
                            id: task.id,
                            status: JobItemStatus::Cancelled,
                            outputs: Vec::new(),
                            error: None,
                            failed_pages: None,
                        });
                        let mut q = match queue.lock() {
                            Ok(g) => g,
                            Err(p) => p.into_inner(),
                        };
                        unprocessed.fetch_add(q.len() as u32, Ordering::SeqCst);
                        q.clear();
                        break;
                    }
                    Err(probe_err) => {
                        let remaining = task.page_count.saturating_sub(item_pages_done);
                        let cur_done = done.fetch_add(remaining, Ordering::SeqCst) + remaining;
                        failed.fetch_add(1, Ordering::SeqCst);
                        on_item(JobItemPayload {
                            id: task.id,
                            status: JobItemStatus::Failed,
                            outputs: Vec::new(),
                            error: Some(IpcError::from(probe_err)),
                            failed_pages: None,
                        });
                        on_progress(JobProgressPayload {
                            done: cur_done,
                            total,
                            current: None,
                        });
                        continue;
                    }
                    Ok(AddImageResult::Ok { .. }) => {}
                }

                let pdf_bytes = writer
                    .finish()
                    .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))
                    .and_then(|bytes| protect_pdf(bytes, output.password.as_ref()));
                let process_result = pdf_bytes.and_then(|bytes| {
                    save_atomic(&output_dir, output_name, &bytes, &used_names).inspect_err(|err| {
                        stop_after_write_failure(err, &write_failed);
                    })
                });

                match process_result {
                    Ok(saved_name) => {
                        succeeded.fetch_add(1, Ordering::SeqCst);
                        on_item(JobItemPayload {
                            id: task.id,
                            status: JobItemStatus::Ok,
                            outputs: vec![saved_name],
                            error: None,
                            failed_pages: None,
                        });
                        on_progress(JobProgressPayload {
                            done: done.load(Ordering::SeqCst),
                            total,
                            current: None,
                        });
                    }
                    Err(err) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                        on_item(JobItemPayload {
                            id: task.id,
                            status: JobItemStatus::Failed,
                            outputs: Vec::new(),
                            error: Some(err),
                            failed_pages: None,
                        });
                        on_progress(JobProgressPayload {
                            done: done.load(Ordering::SeqCst),
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
    if accounted < total_items {
        failed.fetch_add(total_items - accounted, Ordering::SeqCst);
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

    let output_dir = state
        .images_output_dir
        .lock()
        .expect("output_dir lock")
        .clone()
        .ok_or_else(|| IpcError::from_code(ErrorCode::InvalidParams))?;
    check_output_dir(state, OutputKind::ImagesToPdf, &output_dir)?;
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
