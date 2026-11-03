//! Merging images into one PDF per design §6.2, §6.5.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::Ordering;

use pdfconv_core::{AddImageResult, ErrorCode, PdfWriter};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::image_check::{ImageCheckStop, check_image};
use super::options::{PageOrientationChoice, PageSizeChoice, PdfOutput, creator, protect_pdf};
use super::output::create_output_temp;
use super::{JobCallbacks, JobFinishedPayload, JobItemPayload, JobProgressPayload};
use crate::AppState;
use crate::decode_budget::pdf_write_estimate;
use crate::error::IpcError;
use crate::items::{ItemKind, file_name};

/// How many times the output a merge holds at its peak: the whole output in memory, and the same
/// again while `finish` serializes it (design §6.2).
const MERGE_PEAK_FACTOR: u64 = 2;

/// The part of the installed memory a merge may use, as a fraction `NUMERATOR / DENOMINATOR`. The
/// rest is left to the OS and the other apps (design §6.2).
const MERGE_MEMORY_NUMERATOR: u64 = 3;
const MERGE_MEMORY_DENOMINATOR: u64 = 5;

/// The bytes of a megabyte in the detail of `MergeTooLarge`, as in `OutputTooLarge`.
const BYTES_PER_MB: u64 = 1024 * 1024;

/// The largest estimate a merge is let through with on a machine with `installed` bytes of
/// memory: 3/5 of it, halved because the peak is twice the output. See design §6.2 for the
/// numbers.
pub fn merge_output_limit(installed: u64) -> u64 {
    installed.saturating_mul(MERGE_MEMORY_NUMERATOR) / MERGE_MEMORY_DENOMINATOR / MERGE_PEAK_FACTOR
}

/// Refuses a merge whose estimate is over [`merge_output_limit`] of the installed memory, before
/// anything starts (design §6.2). The estimate is the sum of the sizes of the files of the images
/// that have no error; a file that cannot be read counts as 0. Exactly the limit is let through,
/// and so is everything when the installed memory is not known.
///
/// # Errors
///
/// `MergeTooLarge`, with the estimate and the limit in MB (1,048,576 bytes) as `detail`: the
/// estimate rounded up, so that one just over the limit does not read as the limit.
pub fn check_merge_size(state: &AppState, ids: &[u64]) -> Result<(), IpcError> {
    let Some(installed) = state.installed_memory else {
        return Ok(());
    };
    let estimate = ids
        .iter()
        .filter_map(|&id| state.items.get(id))
        .filter(|entry| entry.kind == ItemKind::Image && entry.error.is_none())
        .map(|entry| std::fs::metadata(&entry.path).map_or(0, |metadata| metadata.len()))
        .fold(0, u64::saturating_add);
    let limit = merge_output_limit(installed);
    if estimate <= limit {
        return Ok(());
    }
    Err(IpcError::new(
        ErrorCode::MergeTooLarge,
        format!(
            "{} MB / {} MB",
            estimate.div_ceil(BYTES_PER_MB),
            limit / BYTES_PER_MB
        ),
    ))
}

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

    let mut writer = PdfWriter::new(&creator(), output.conformance);

    let mut temp = create_output_temp(output_dir).map_err(IpcError::write_failed)?;

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

        let file_name = file_name(&entry.path);

        on_progress(JobProgressPayload {
            done,
            total,
            current: Some(file_name.clone()),
        });

        if let Some(err) = entry.error.clone() {
            failed += 1;
            on_item(JobItemPayload::failed(*id, err));
            on_progress(JobProgressPayload {
                done,
                total,
                current: None,
            });
            continue;
        }

        // The pixels are read in a worker first, so that a decoder that stops or crashes is
        // contained there (design §6.2). A failure is this image's, as an unreadable file is.
        match check_image(&state.pool, &entry.path, &|| {
            state.cancel_flag.load(Ordering::SeqCst)
        }) {
            Ok(()) => {}
            Err(ImageCheckStop::Failed(error)) => {
                failed += 1;
                done += entry.page_count;
                on_item(JobItemPayload::failed(*id, error));
                on_progress(JobProgressPayload {
                    done,
                    total,
                    current: None,
                });
                continue;
            }
            Err(ImageCheckStop::Cancelled) => {
                was_cancelled = true;
                unprocessed = (valid_tasks.len() - idx) as u32;
                break;
            }
        }

        // Borrowed before the file is read and held until `add_checked_image` is done and the
        // bytes are let go (design §5.3). A wait that is cancelled is a cancelled
        // `add_checked_image`.
        let Some(_permit) = state.decode_budget.acquire(
            pdf_write_estimate(&entry.path, entry.decoded_bytes),
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
                on_item(JobItemPayload::failed(
                    *id,
                    IpcError::from_code(ErrorCode::ReadFailed),
                ));
                on_progress(JobProgressPayload {
                    done,
                    total,
                    current: None,
                });
                continue;
            }
        };

        let mut item_pages_done = 0u32;
        let add_result = writer.add_checked_image(
            bytes,
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
                on_item(JobItemPayload::ok(*id, Vec::new()));
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
                on_item(JobItemPayload::failed(*id, IpcError::from(probe_err)));
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

    let pdf_bytes = writer.finish().map_err(IpcError::write_failed)?;
    let pdf_bytes = protect_pdf(pdf_bytes, output.password.as_ref())?;

    temp.write_all(&pdf_bytes).map_err(IpcError::write_failed)?;
    temp.flush().map_err(IpcError::write_failed)?;

    // Merged PDF save dialog allows overwriting (design §6.2)
    temp.persist(dest_path)
        .map_err(|e| IpcError::write_failed(e.error))?;

    let saved_name = file_name(dest_path);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_is_three_fifths_of_the_installed_memory_halved() {
        const GIB: u64 = 1024 * 1024 * 1024;
        assert_eq!(merge_output_limit(0), 0);
        assert_eq!(merge_output_limit(10 * GIB), 3 * GIB);
        assert_eq!(merge_output_limit(16 * GIB), 16 * GIB * 3 / 10);
        // The two divisions round down in turn, as `MERGE_PEAK_FACTOR` and the fraction say.
        assert_eq!(merge_output_limit(9), 2);
    }

    #[test]
    fn the_limit_saturates_instead_of_overflowing() {
        // No machine has this much; the product saturates and the limit stays finite.
        assert_eq!(merge_output_limit(u64::MAX), u64::MAX / 5 / 2);
    }
}
