//! Merging images into one PDF per design §6.2, §6.5.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::Ordering;

use pdfconv_core::{AddImageResult, PdfWriter};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::options::{PageOrientationChoice, PageSizeChoice, PdfOutput, protect_pdf};
use super::output::create_output_temp;
use super::{JobCallbacks, JobFinishedPayload, JobItemPayload, JobItemStatus, JobProgressPayload};
use crate::AppState;
use crate::decode_budget::DECODE_MEMORY_FACTOR_PDF;
use crate::error::{ErrorCode, IpcError};
use crate::items::ItemKind;

/// Result returned once a merge has run (design §7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SaveMergedPdfResult {
    /// The saved file's name, or `None` when no PDF was written (the job was
    /// cancelled, or no image could be added).
    pub saved_name: Option<String>,
}

/// Executes merging multiple images into a single PDF document (design §6.2, §6.5).
pub fn run_save_merged_pdf<FProg, FItem, FFin>(
    state: &AppState,
    ids: &[u64],
    page_size: PageSizeChoice,
    orientation: PageOrientationChoice,
    output: &PdfOutput,
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
    let mut total = 0u32;
    for &id in ids {
        let entry = state
            .items
            .get(id)
            .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
        if entry.kind == ItemKind::Image {
            if entry.error.is_none() {
                total = total.saturating_add(entry.page_count);
            }
            valid_tasks.push((id, entry));
        }
    }

    on_progress(JobProgressPayload {
        done: 0,
        total,
        current: None,
    });

    let creator = format!("PDF Converter {}", env!("CARGO_PKG_VERSION"));
    let mut writer = PdfWriter::new(&creator, output.conformance);

    let mut temp = create_output_temp(output_dir)
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;

    let mut done = 0u32;
    let mut succeeded = 0u32;
    let mut failed = 0u32;
    let mut unprocessed = 0u32;
    let mut was_cancelled = false;

    for (idx, (id, entry)) in valid_tasks.iter().enumerate() {
        if state.cancel_flag.load(Ordering::SeqCst) {
            was_cancelled = true;
            unprocessed = (valid_tasks.len() - idx) as u32;
            break;
        }

        let file_name = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        on_progress(JobProgressPayload {
            done,
            total,
            current: Some(file_name.clone()),
        });

        if let Some(err) = entry.error.clone() {
            failed += 1;
            on_item(JobItemPayload {
                id: *id,
                status: JobItemStatus::Failed,
                outputs: Vec::new(),
                error: Some(err),
                failed_pages: None,
            });
            on_progress(JobProgressPayload {
                done,
                total,
                current: None,
            });
            continue;
        }

        // Borrowed before the file is read and held until `add_image` is done and the bytes
        // are let go (design §5.3). A wait that is cancelled is a cancelled `add_image`.
        let Some(_permit) = state.decode_budget.acquire(
            entry.decoded_bytes.saturating_mul(DECODE_MEMORY_FACTOR_PDF),
            &|| state.cancel_flag.load(Ordering::SeqCst),
        ) else {
            was_cancelled = true;
            unprocessed = (valid_tasks.len() - idx) as u32;
            break;
        };

        let bytes = match std::fs::read(&entry.path) {
            Ok(b) => b,
            Err(_) => {
                failed += 1;
                done += entry.page_count;
                on_item(JobItemPayload {
                    id: *id,
                    status: JobItemStatus::Failed,
                    outputs: Vec::new(),
                    error: Some(IpcError::from_code(ErrorCode::ReadFailed)),
                    failed_pages: None,
                });
                on_progress(JobProgressPayload {
                    done,
                    total,
                    current: None,
                });
                continue;
            }
        };

        let mut item_pages_done = 0u32;
        let add_result = writer.add_image(
            &bytes,
            page_size.to_core(orientation),
            |_page_idx, _total_in_item| {
                item_pages_done += 1;
                done += 1;
                on_progress(JobProgressPayload {
                    done,
                    total,
                    current: Some(file_name.clone()),
                });
            },
            || state.cancel_flag.load(Ordering::SeqCst),
        );

        match add_result {
            Ok(AddImageResult::Ok { .. }) => {
                succeeded += 1;
                on_item(JobItemPayload {
                    id: *id,
                    status: JobItemStatus::Ok,
                    outputs: Vec::new(),
                    error: None,
                    failed_pages: None,
                });
            }
            Ok(AddImageResult::Cancelled) => {
                was_cancelled = true;
                unprocessed = (valid_tasks.len() - idx) as u32;
                break;
            }
            Err(probe_err) => {
                let remaining = entry.page_count.saturating_sub(item_pages_done);
                done += remaining;
                failed += 1;
                on_item(JobItemPayload {
                    id: *id,
                    status: JobItemStatus::Failed,
                    outputs: Vec::new(),
                    error: Some(IpcError::from(probe_err)),
                    failed_pages: None,
                });
            }
        }

        on_progress(JobProgressPayload {
            done,
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
    let pdf_bytes = protect_pdf(pdf_bytes, output.password.as_ref())?;

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
