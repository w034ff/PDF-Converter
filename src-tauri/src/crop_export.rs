//! Writing the ranges of the open crop PDF out as images (design §6.4, §6.5,
//! §6.10, §7.1).
//!
//! Everything that can be refused is refused before the export starts, and
//! every name is decided then, so a request is taken or refused as a whole.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use pdfconv_core::naming::{CropRegion, resolve_crop_names};
use pdfconv_core::{PageRect, position_order};
use pdfconv_worker::is_render_dpi_in_range;
use pdfconv_worker::protocol::{RectPt, Request, Response};
use serde::Deserialize;
use ts_rs::TS;

use crate::AppState;
use crate::crop::ExportTarget;
use crate::error::{ErrorCode, IpcError};
use crate::items::{PageSizePt, close_pdf, send_open, unexpected_response};
use crate::jobs::{
    self, FinishJob, JobCallbacks, JobFinishedPayload, JobItemPayload, JobItemStatus,
    JobProgressPayload,
};
use crate::settings::{CropOptions, OutputKind};
use crate::worker_pool::PooledWorker;

/// One range to write, as the screen sends it (design §7.1).
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CropRegionInput {
    /// What the screen calls the range; the `id` of its `job-item`.
    #[ts(type = "number")]
    pub key: u64,
    /// The page the range lies on, from 1.
    pub page: u32,
    /// `[x0, y0, x1, y1]` in points, with the origin at the top left of the
    /// page as shown (design §4.6). For a photo it is the frame of the image.
    pub rect_pt: [f64; 4],
    /// The embedded image behind a photo range; `null` for the other kinds.
    pub image_index: Option<u32>,
}

/// A range ready to be written: where it is, and the name it is saved as.
struct PlannedRange {
    key: u64,
    page: u32,
    rect_pt: RectPt,
    image_index: Option<u32>,
    name: String,
}

/// Everything an export does, decided when it starts.
struct ExportPlan {
    pdf_name: String,
    open: Request,
    output_dir: PathBuf,
    options: CropOptions,
    /// In the order they are written: by page, then by position on the page,
    /// which is the order of the numbers in the names (design §6.4).
    ranges: Vec<PlannedRange>,
    /// The names in the output folder and the planned ones, in lower case, for
    /// `save_atomic` to pick numbers from when a name is taken meanwhile.
    taken_lower: HashSet<String>,
}

fn page_rect(rect_pt: &[f64; 4]) -> PageRect {
    PageRect {
        x0: rect_pt[0],
        y0: rect_pt[1],
        x1: rect_pt[2],
        y1: rect_pt[3],
    }
}

/// Whether `rect_pt` is a non-empty rectangle inside `page`.
///
/// The page's size is an `f32` (`PageSizePt`), and the screen gets it as the
/// shortest decimal text of that `f32`. A rectangle as wide as the page, taken
/// from that text as an `f64`, can be larger than the `f32` widened to `f64`.
/// So the edges are rounded to `f32` before they are compared with the page,
/// which makes a rectangle limited to the size the screen was told fit.
fn rect_in_page(rect_pt: &[f64; 4], page: &PageSizePt) -> bool {
    let [x0, y0, x1, y1] = rect_pt.map(|edge| edge as f32);
    // `NaN` fails every comparison.
    rect_pt[0] < rect_pt[2]
        && rect_pt[1] < rect_pt[3]
        && x0 >= 0.0
        && y0 >= 0.0
        && x1 <= page.width_pt
        && y1 <= page.height_pt
}

/// Refuses requests that cannot be exported (design §7.1).
fn check_regions(regions: &[CropRegionInput], pages: &[PageSizePt]) -> Result<(), IpcError> {
    let invalid = || IpcError::from_code(ErrorCode::InvalidParams);
    if regions.is_empty() {
        return Err(invalid());
    }
    let mut keys = HashSet::new();
    for region in regions {
        let page = usize::try_from(region.page)
            .ok()
            .and_then(|page| page.checked_sub(1))
            .and_then(|index| pages.get(index))
            .ok_or_else(invalid)?;
        if !keys.insert(region.key) || !rect_in_page(&region.rect_pt, page) {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Asks the pool's workers for the images of every page that has a photo
/// range, and refuses an `imageIndex` that is not among them.
///
/// The check is the cost of one `PageImages` per page: a bad request is
/// refused as a whole, instead of failing range by range after some files are
/// written (design §6.10).
fn check_image_indices(
    state: &AppState,
    open: &Request,
    regions: &[CropRegionInput],
) -> Result<(), IpcError> {
    let mut wanted: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for region in regions {
        if let Some(index) = region.image_index {
            wanted.entry(region.page).or_default().push(index);
        }
    }
    if wanted.is_empty() {
        return Ok(());
    }
    let mut worker = state.pool.acquire()?;
    send_open(&mut worker, open)?;
    let checked = wanted.iter().try_for_each(|(&page, indices)| {
        match worker.send(&Request::PageImages { page })? {
            (Response::PageImages { images }, _) => {
                let listed = |index: &u32| images.iter().any(|image| image.image_index == *index);
                if indices.iter().all(listed) {
                    Ok(())
                } else {
                    Err(IpcError::from_code(ErrorCode::InvalidParams))
                }
            }
            _ => Err(unexpected_response()),
        }
    });
    close_pdf(&mut worker);
    checked
}

fn plan_ranges(
    pdf_name: &str,
    page_count: u32,
    regions: &[CropRegionInput],
    extension: &str,
    existing: &HashSet<String>,
) -> Vec<PlannedRange> {
    let named: Vec<CropRegion> = regions
        .iter()
        .map(|region| CropRegion {
            page: region.page,
            rect: page_rect(&region.rect_pt),
        })
        .collect();
    let names = resolve_crop_names(pdf_name, page_count, &named, extension, existing);

    // The same grouping and ordering `resolve_crop_names` numbers the names by.
    let mut by_page: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (index, region) in named.iter().enumerate() {
        by_page.entry(region.page).or_default().push(index);
    }
    let mut order = Vec::with_capacity(regions.len());
    for indices in by_page.values() {
        let rects: Vec<PageRect> = indices.iter().map(|&index| named[index].rect).collect();
        order.extend(
            position_order(&rects)
                .into_iter()
                .map(|local| indices[local]),
        );
    }

    order
        .into_iter()
        .map(|index| {
            let region = &regions[index];
            let [x0, y0, x1, y1] = region.rect_pt;
            PlannedRange {
                key: region.key,
                page: region.page,
                rect_pt: RectPt { x0, y0, x1, y1 },
                image_index: region.image_index,
                name: names[index].clone(),
            }
        })
        .collect()
}

/// Starts writing `regions` of the open crop PDF `id` in the background, as a
/// conversion (design §6.10, §7.1). The ranges are written one at a time, by
/// page and by position on the page; each gets a `job-item`.
///
/// # Errors
///
/// Nothing starts, and `is_running` is as it was, with:
///
/// - `ConversionRunning` during a conversion;
/// - `UnknownHandle` unless `id` is the open PDF, and its own error if it
///   could not be opened;
/// - `InvalidParams` for a `dpi` outside design §4.4, no ranges, a `key` used
///   twice, a page outside the PDF, a range that is empty or reaches outside
///   its page, an `imageIndex` the page has no image for, and when the crop
///   output folder is not chosen;
/// - `OutputDirMissing` and `OutputDirNotWritable` as [`jobs::check_output_dir`];
/// - the error of the worker that lists the images (`WorkerCrashed` and the
///   like), and `WriteFailed` if the folder cannot be read.
pub fn start_crop_export_internal<FProg, FItem, FFin>(
    state: &AppState,
    id: u64,
    regions: &[CropRegionInput],
    options: CropOptions,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    let guard = jobs::begin_conversion(state)?;
    let ExportTarget {
        open,
        name,
        page_count,
        pages,
    } = state.crop.export_target(id)?;

    if !is_render_dpi_in_range(options.dpi) {
        return Err(IpcError::from_code(ErrorCode::InvalidParams));
    }
    check_regions(regions, &pages)?;

    let output_dir = state
        .crop_output_dir
        .lock()
        .expect("output_dir lock")
        .clone()
        .ok_or_else(|| IpcError::from_code(ErrorCode::InvalidParams))?;
    jobs::check_output_dir(state, OutputKind::Crop, &output_dir)?;

    check_image_indices(state, &open, regions)?;
    let existing = jobs::list_existing_files(&output_dir)?;

    let ranges = plan_ranges(
        &name,
        page_count,
        regions,
        options.format.extension(),
        &existing,
    );
    let taken_lower = existing
        .iter()
        .chain(ranges.iter().map(|range| &range.name))
        .map(|name| name.to_lowercase())
        .collect();
    let plan = ExportPlan {
        pdf_name: name,
        open,
        output_dir,
        options,
        ranges,
        taken_lower,
    };

    let JobCallbacks {
        on_progress,
        on_item,
        on_finished,
    } = callbacks;
    let failed_if_it_panics = u32::try_from(regions.len()).unwrap_or(u32::MAX);
    jobs::spawn_conversion(
        state,
        guard,
        on_finished,
        failed_if_it_panics,
        move |state, finish| {
            run_crop_export(state, &plan, &on_progress, &on_item, finish);
            Ok(())
        },
    );
    Ok(())
}

#[derive(Default)]
struct Tally {
    succeeded: u32,
    failed: u32,
    /// The ranges that got a `job-item`.
    reached: u32,
}

/// Sends the `job-item` of `range` and counts it.
fn report(
    range: &PlannedRange,
    result: Result<String, IpcError>,
    on_item: &dyn Fn(JobItemPayload),
    tally: &mut Tally,
) {
    tally.reached += 1;
    on_item(match result {
        Ok(saved_name) => {
            tally.succeeded += 1;
            JobItemPayload {
                id: range.key,
                status: JobItemStatus::Ok,
                outputs: vec![saved_name],
                error: None,
                failed_pages: None,
                source_id: None,
            }
        }
        Err(error) => {
            tally.failed += 1;
            JobItemPayload {
                id: range.key,
                status: JobItemStatus::Failed,
                outputs: Vec::new(),
                error: Some(error),
                failed_pages: None,
                source_id: None,
            }
        }
    });
}

/// Whether the ranges after one that failed with `error` are left unwritten.
///
/// A folder that cannot be written fails every range alike (design §6.5). A
/// worker that crashed or timed out on a range would meet the same PDF again
/// (design §5.2, §6.10). Any other error belongs to its range alone.
fn ends_the_export(error: &IpcError) -> bool {
    matches!(
        error.code,
        ErrorCode::WriteFailed | ErrorCode::WorkerCrashed | ErrorCode::WorkerTimeout
    )
}

/// Borrows a worker and opens the PDF of the plan in it.
fn open_in_worker(state: &AppState, plan: &ExportPlan) -> Result<PooledWorker, IpcError> {
    let mut worker = state.pool.acquire()?;
    send_open(&mut worker, &plan.open)?;
    Ok(worker)
}

/// Renders one range and saves it.
fn write_range(
    worker: &mut PooledWorker,
    plan: &ExportPlan,
    range: &PlannedRange,
    taken_lower: &Mutex<HashSet<String>>,
) -> Result<String, IpcError> {
    let options = &plan.options;
    let request = Request::RenderRegion {
        page: range.page,
        rect_pt: range.rect_pt,
        image_index: range.image_index,
        dpi: options.dpi,
        format: options.format.into(),
        color: options.color_mode.into(),
        photo_native: options.photo_native,
        photo_only: options.photo_only,
    };
    match worker.send(&request)? {
        (Response::RenderRegion { .. }, image) => {
            jobs::save_atomic(&plan.output_dir, &range.name, &image, taken_lower)
        }
        _ => Err(unexpected_response()),
    }
}

fn run_crop_export(
    state: &AppState,
    plan: &ExportPlan,
    on_progress: &dyn Fn(JobProgressPayload),
    on_item: &dyn Fn(JobItemPayload),
    finish: FinishJob,
) {
    let total = u32::try_from(plan.ranges.len()).unwrap_or(u32::MAX);
    let progress = |done: u32| {
        on_progress(JobProgressPayload {
            done,
            total,
            current: Some(plan.pdf_name.clone()),
        });
    };
    let taken_lower = Mutex::new(plan.taken_lower.clone());
    let mut tally = Tally::default();
    let mut cancelled = false;

    progress(0);
    match open_in_worker(state, plan) {
        // The ranges share the one worker, so there is nothing to try the
        // others on: the first range carries the error.
        Err(error) => {
            if let Some(first) = plan.ranges.first() {
                report(first, Err(error), on_item, &mut tally);
                progress(tally.reached);
            }
        }
        Ok(mut worker) => {
            for range in &plan.ranges {
                if state.cancel_flag.load(Ordering::SeqCst) {
                    cancelled = true;
                    break;
                }
                let result = write_range(&mut worker, plan, range, &taken_lower);
                let ends = result.as_ref().err().is_some_and(ends_the_export);
                report(range, result, on_item, &mut tally);
                progress(tally.reached);
                if ends {
                    break;
                }
            }
            close_pdf(&mut worker);
        }
    }

    finish(JobFinishedPayload {
        succeeded: tally.succeeded,
        failed: tally.failed,
        no_pages: 0,
        unprocessed: total - tally.reached,
        cancelled,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const A4: PageSizePt = PageSizePt {
        width_pt: 595.28,
        height_pt: 841.89,
    };

    #[test]
    fn a_rectangle_inside_the_page_fits_up_to_its_edges() {
        assert!(rect_in_page(&[10.0, 20.0, 100.0, 200.0], &A4));
        assert!(rect_in_page(&[0.0, 0.0, 595.28, 841.89], &A4));
    }

    #[test]
    fn a_rectangle_with_an_edge_outside_the_page_does_not_fit() {
        assert!(!rect_in_page(&[-0.1, 0.0, 100.0, 100.0], &A4));
        assert!(!rect_in_page(&[0.0, -0.1, 100.0, 100.0], &A4));
        assert!(!rect_in_page(&[0.0, 0.0, 595.4, 100.0], &A4));
        assert!(!rect_in_page(&[0.0, 0.0, 100.0, 842.0], &A4));
    }

    #[test]
    fn an_empty_reversed_or_not_a_number_rectangle_does_not_fit() {
        assert!(!rect_in_page(&[50.0, 0.0, 50.0, 100.0], &A4));
        assert!(!rect_in_page(&[0.0, 50.0, 100.0, 50.0], &A4));
        assert!(!rect_in_page(&[60.0, 0.0, 50.0, 100.0], &A4));
        assert!(!rect_in_page(&[f64::NAN, 0.0, 100.0, 100.0], &A4));
        assert!(!rect_in_page(&[0.0, 0.0, f64::NAN, 100.0], &A4));
        assert!(!rect_in_page(&[0.0, 0.0, f64::INFINITY, 100.0], &A4));
    }

    #[test]
    fn a_rectangle_as_large_as_the_page_text_fits_even_when_the_f32_rounded_down() {
        // These f32 values are below the decimal text they print as, so the
        // text read back as an `f64` is larger than the f32 widened to `f64`.
        let page = PageSizePt {
            width_pt: 595.2759,
            height_pt: 841.8899,
        };
        assert!(f64::from(page.width_pt) < 595.2759);
        assert!(f64::from(page.height_pt) < 841.8899);

        assert!(rect_in_page(&[0.0, 0.0, 595.2759, 841.8899], &page));
        assert!(!rect_in_page(&[0.0, 0.0, 595.28, 841.8899], &page));
    }

    #[test]
    fn only_a_folder_a_crash_or_a_timeout_ends_the_export() {
        for code in [
            ErrorCode::WriteFailed,
            ErrorCode::WorkerCrashed,
            ErrorCode::WorkerTimeout,
        ] {
            assert!(ends_the_export(&IpcError::from_code(code)), "{code:?}");
        }
        for code in [ErrorCode::RenderTooLarge, ErrorCode::InvalidParams] {
            assert!(!ends_the_export(&IpcError::from_code(code)), "{code:?}");
        }
    }
}
