//! Layout detection against the standalone worker and the real model
//! fetched by `npm run layout:fetch` (design §4.7, §11.1, §11.3).

#[path = "support/worker_paths.rs"]
mod worker_paths;

use std::ffi::OsString;
use std::path::PathBuf;

use pdfconv_core::{PageRect, iou};
use pdfconv_worker::client::{DETECT_TIMEOUT, RENDER_TIMEOUT, WorkerError, WorkerProcess};
use pdfconv_worker::protocol::{
    CandidateKind, EmbeddedImage, LayoutCandidate, RectPt, RenderColor, RenderFormat, Request,
    Response,
};

/// The bar chart of `crop_layout.pdf` with its axes, as `crop_layout_pdf` in
/// `gen_fixtures` draws it: left, top, right, bottom in pt.
const CHART_FRAME_PT: [f64; 4] = [98.0, 520.0, 498.0, 810.0];

/// How closely the detected chart must cover [`CHART_FRAME_PT`]. Below 1 so
/// that a slightly different box from another platform's onnxruntime still
/// passes.
const CHART_MIN_IOU: f64 = 0.8;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures")
}

fn open(worker: &mut WorkerProcess, fixture: &str) {
    let opened = worker.request(
        &Request::Open {
            path: fixtures_dir().join(fixture),
            password: None,
        },
        RENDER_TIMEOUT,
    );
    assert!(opened.is_ok(), "opening {fixture}: {opened:?}");
}

fn crop_layout(args: [OsString; 3]) -> WorkerProcess {
    let mut worker = worker_paths::start_with(args);
    open(&mut worker, "crop_layout.pdf");
    worker
}

fn detect(worker: &mut WorkerProcess, page: u32) -> Vec<LayoutCandidate> {
    match worker.request(&Request::Detect { page }, DETECT_TIMEOUT) {
        Ok((Response::Detect { candidates }, body)) => {
            assert!(body.is_empty());
            candidates
        }
        other => unreachable!("expected candidates, got {other:?}"),
    }
}

fn expect_error(worker: &mut WorkerProcess, request: &Request, expected_code: &str) {
    match worker.request(request, DETECT_TIMEOUT) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, expected_code),
        other => unreachable!("expected {expected_code}, got {other:?}"),
    }
}

fn page_images(worker: &mut WorkerProcess) -> Vec<EmbeddedImage> {
    match worker.request(&Request::PageImages { page: 1 }, RENDER_TIMEOUT) {
        Ok((Response::PageImages { images }, _)) => images,
        other => unreachable!("expected the page's images, got {other:?}"),
    }
}

fn page_rect(rect: &RectPt) -> PageRect {
    PageRect {
        x0: rect.x0,
        y0: rect.y0,
        x1: rect.x1,
        y1: rect.y1,
    }
}

fn args_with(onnxruntime: OsString, model: OsString) -> [OsString; 3] {
    [worker_paths::library_dir().into(), onnxruntime, model]
}

fn missing(name: &str) -> OsString {
    std::env::temp_dir()
        .join("pdfconv-no-layout-here")
        .join(name)
        .into()
}

#[test]
fn crop_layout_has_its_photo_and_its_chart() {
    let mut worker = crop_layout(worker_paths::worker_args());
    let candidates = detect(&mut worker, 1);
    let images = page_images(&mut worker);
    assert_eq!(images.len(), 1, "{images:?}");

    let photos: Vec<_> = candidates
        .iter()
        .filter(|c| c.kind == CandidateKind::Photo)
        .collect();
    assert_eq!(photos.len(), 1, "{candidates:?}");
    assert_eq!(photos[0].image_index, Some(images[0].image_index));
    assert_eq!(photos[0].rect_pt, images[0].rect_pt);

    let charts: Vec<_> = candidates
        .iter()
        .filter(|c| matches!(c.kind, CandidateKind::Chart | CandidateKind::Figure))
        .collect();
    assert_eq!(charts.len(), 1, "{candidates:?}");
    assert_eq!(charts[0].image_index, None);
    let [x0, y0, x1, y1] = CHART_FRAME_PT;
    let overlap = iou(&page_rect(&charts[0].rect_pt), &PageRect { x0, y0, x1, y1 });
    assert!(overlap >= CHART_MIN_IOU, "{overlap}: {candidates:?}");

    assert_eq!(candidates.len(), 2, "{candidates:?}");
    assert!(candidates[0].score >= candidates[1].score, "{candidates:?}");
}

#[test]
fn the_model_stays_loaded_and_other_requests_still_work() {
    let mut worker = crop_layout(worker_paths::worker_args());
    let first = detect(&mut worker, 1);
    assert_eq!(detect(&mut worker, 1), first);

    let rendered = worker.request(
        &Request::Render {
            page: 1,
            dpi: 72,
            format: RenderFormat::Png,
            color: RenderColor::Color,
        },
        RENDER_TIMEOUT,
    );
    assert!(matches!(rendered, Ok((Response::Render, ref body)) if !body.is_empty()));
    assert_eq!(page_images(&mut worker).len(), 1);

    // Closing and opening again keeps the model: the answer is the same.
    assert!(worker.request(&Request::Close, RENDER_TIMEOUT).is_ok());
    open(&mut worker, "crop_layout.pdf");
    assert_eq!(detect(&mut worker, 1), first);
}

fn unavailable_stays_unavailable(args: [OsString; 3]) {
    let mut worker = crop_layout(args);
    expect_error(
        &mut worker,
        &Request::Detect { page: 1 },
        "LayoutUnavailable",
    );
    assert_eq!(page_images(&mut worker).len(), 1);
    expect_error(
        &mut worker,
        &Request::Detect { page: 1 },
        "LayoutUnavailable",
    );
    assert_eq!(page_images(&mut worker).len(), 1);
}

#[test]
fn a_missing_model_is_layout_unavailable_for_every_detect() {
    unavailable_stays_unavailable(args_with(
        worker_paths::onnxruntime_file().into(),
        missing("model.onnx"),
    ));
}

#[test]
fn a_missing_onnxruntime_is_layout_unavailable_for_every_detect() {
    unavailable_stays_unavailable(args_with(
        missing("onnxruntime-library"),
        worker_paths::model_file().into(),
    ));
}

#[test]
fn detect_needs_an_open_pdf_and_a_page_before_loading_anything() {
    // With nothing to load, a load would answer LayoutUnavailable; these
    // requests must be rejected before that.
    let mut worker = worker_paths::start_with(args_with(missing("onnxruntime"), missing("model")));
    expect_error(&mut worker, &Request::Detect { page: 1 }, "InvalidParams");
    open(&mut worker, "crop_layout.pdf");
    expect_error(&mut worker, &Request::Detect { page: 0 }, "InvalidParams");
    expect_error(&mut worker, &Request::Detect { page: 2 }, "InvalidParams");
    assert!(worker.request(&Request::Close, RENDER_TIMEOUT).is_ok());
    expect_error(&mut worker, &Request::Detect { page: 1 }, "InvalidParams");
}
