//! Integration tests for exporting the ranges of the crop PDF (design §6.4,
//! §6.5, §6.10, §7.1): what is checked before it starts, the names and the
//! order, what happens to a range that fails, and the end of the job. They use
//! the real worker, like `jobs.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pdf_converter_lib::AppState;
use pdf_converter_lib::crop::{
    CropPdf, activate_crop_pdf, detect_layout, list_page_images, open_pdf,
};
use pdf_converter_lib::crop_export::{CropRegionInput, start_crop_export_internal};
use pdf_converter_lib::error::{ErrorCode, IpcError};
use pdf_converter_lib::jobs::{
    JobCallbacks, JobFinishedPayload, JobItemPayload, JobItemStatus, JobProgressPayload,
    RenderColorChoice, RenderFormatChoice,
};
use pdf_converter_lib::settings::{
    CropOptions, OutputKind, apply_picked_dir, load_settings, restore_settings,
};
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig};
use pdfconv_worker::WORKER_FLAG;
use tempfile::TempDir;

fn library_dir() -> PathBuf {
    let os_dir = if cfg!(windows) {
        "windows/bin"
    } else {
        "linux/lib"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdfium")
        .join(os_dir)
}

/// The onnxruntime library and the layout model fetched by
/// `npm run layout:fetch`, the worker's last two arguments (design §5.4).
fn layout_args() -> [String; 2] {
    let onnxruntime = if cfg!(windows) {
        "onnxruntime/windows/onnxruntime.dll"
    } else {
        "onnxruntime/linux/libonnxruntime.so"
    };
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    [
        manifest_dir.join(onnxruntime),
        manifest_dir.join("layout-model/PP-DocLayout_plus-L-int8.onnx"),
    ]
    .map(|path| path.to_string_lossy().into_owned())
}

fn config() -> WorkerPoolConfig {
    WorkerPoolConfig::new(
        env!("CARGO_BIN_EXE_pdf-converter"),
        [
            WORKER_FLAG.to_string(),
            library_dir().to_string_lossy().into_owned(),
        ]
        .into_iter()
        .chain(layout_args()),
    )
    .with_max_workers(2)
    .with_open_timeout(Duration::from_secs(30))
    .with_render_timeout(Duration::from_secs(60))
}

fn state_with(config: WorkerPoolConfig) -> AppState {
    AppState::new(WorkerPool::new(config))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/core/tests/fixtures")
        .join(name)
}

/// Opens the fixture `name` as the open crop PDF.
fn open_active(state: &AppState, name: &str) -> CropPdf {
    let pdf = open_pdf(state, &fixture(name));
    activate_crop_pdf(state, pdf.id).expect("activating the pending PDF");
    pdf
}

/// A state with `fixture_name` open and a crop output folder that exists.
fn exporting(fixture_name: &str) -> (AppState, CropPdf, TempDir) {
    exporting_with(config(), fixture_name)
}

fn exporting_with(config: WorkerPoolConfig, fixture_name: &str) -> (AppState, CropPdf, TempDir) {
    let state = state_with(config);
    let pdf = open_active(&state, fixture_name);
    let out = TempDir::new().expect("creating the output folder");
    *state.crop_output_dir.lock().expect("output folder lock") = Some(out.path().to_path_buf());
    (state, pdf, out)
}

fn options() -> CropOptions {
    CropOptions {
        format: RenderFormatChoice::Png,
        color_mode: RenderColorChoice::Color,
        dpi: 150,
        photo_native: true,
        photo_only: false,
    }
}

fn manual(key: u64, page: u32, rect_pt: [f64; 4]) -> CropRegionInput {
    CropRegionInput {
        key,
        page,
        rect_pt,
        image_index: None,
    }
}

/// What a finished export reported.
struct Outcome {
    items: Vec<JobItemPayload>,
    progress: Vec<JobProgressPayload>,
    finished: JobFinishedPayload,
}

impl Outcome {
    fn keys(&self) -> Vec<u64> {
        self.items.iter().map(|item| item.id).collect()
    }

    fn statuses(&self) -> Vec<JobItemStatus> {
        self.items.iter().map(|item| item.status).collect()
    }

    fn saved(&self) -> Vec<String> {
        self.items
            .iter()
            .flat_map(|item| item.outputs.clone())
            .collect()
    }

    fn error_code(&self, index: usize) -> Option<ErrorCode> {
        self.items[index].error.as_ref().map(|error| error.code)
    }
}

/// Starts an export and waits for its `job-finished`. `after_item` runs on the
/// export's thread after each `job-item`, in the middle of the job.
fn export_with(
    state: &AppState,
    id: u64,
    regions: &[CropRegionInput],
    options: CropOptions,
    after_item: impl Fn(&JobItemPayload) + Send + Sync + 'static,
) -> Result<Outcome, IpcError> {
    let progress = Arc::new(Mutex::new(Vec::new()));
    let items = Arc::new(Mutex::new(Vec::new()));
    let finished = Arc::new(Mutex::new(None));
    let callbacks = JobCallbacks {
        on_progress: {
            let progress = Arc::clone(&progress);
            move |payload| progress.lock().expect("progress lock").push(payload)
        },
        on_item: {
            let items = Arc::clone(&items);
            move |payload: JobItemPayload| {
                after_item(&payload);
                items.lock().expect("items lock").push(payload);
            }
        },
        on_finished: {
            let finished = Arc::clone(&finished);
            move |payload| *finished.lock().expect("finished lock") = Some(payload)
        },
    };
    start_crop_export_internal(state, id, regions, options, callbacks)?;

    let started = Instant::now();
    let finished = loop {
        if let Some(payload) = finished.lock().expect("finished lock").clone() {
            break payload;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "the export did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        !state.is_running.load(Ordering::SeqCst),
        "is_running must be back when job-finished arrives"
    );
    let items = items.lock().expect("items lock").clone();
    let progress = progress.lock().expect("progress lock").clone();
    Ok(Outcome {
        items,
        progress,
        finished,
    })
}

fn export(
    state: &AppState,
    id: u64,
    regions: &[CropRegionInput],
    options: CropOptions,
) -> Result<Outcome, IpcError> {
    export_with(state, id, regions, options, |_| {})
}

/// Names of the files in `dir`, sorted, temporary files included.
fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("listing the output folder")
        .map(|entry| entry.expect("reading an entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn assert_no_temp_files(dir: &Path) {
    for name in files_in(dir) {
        assert!(
            !name.starts_with('.'),
            "unexpected temporary file found: {name}"
        );
    }
}

fn refused(result: Result<Outcome, IpcError>) -> ErrorCode {
    result.err().expect("the export should be refused").code
}

/// The width and height in the header of a PNG.
fn png_size(png: &[u8]) -> (u32, u32) {
    let read = |range: std::ops::Range<usize>| {
        u32::from_be_bytes(png[range].try_into().expect("four bytes"))
    };
    (read(16..20), read(20..24))
}

/// The photo range and the chart range of `crop_layout.pdf`: the photo as the
/// page lists it, the chart as the layout analysis found it.
fn photo_and_chart(state: &AppState, id: u64) -> (CropRegionInput, CropRegionInput) {
    let images = list_page_images(state, id, 1).expect("listing the images");
    let candidates = detect_layout(state, id, 1).expect("detecting the layout");
    let chart = candidates
        .iter()
        .find(|candidate| candidate.image_index.is_none())
        .expect("the chart");
    (
        CropRegionInput {
            key: 1,
            page: 1,
            rect_pt: images[0].rect_pt,
            image_index: Some(images[0].image_index),
        },
        manual(2, 1, chart.rect_pt),
    )
}

// ---- What is written ----

#[test]
fn a_photo_and_a_chart_are_written_in_position_order_with_the_photo_in_its_own_pixels() {
    let (state, pdf, out) = exporting("crop_layout.pdf");
    let (photo, chart) = photo_and_chart(&state, pdf.id);
    assert!(photo.rect_pt[1] < chart.rect_pt[1], "the photo is above");

    let outcome = export(&state, pdf.id, &[photo, chart], options()).expect("starting");

    assert_eq!(outcome.keys(), [1, 2]);
    assert_eq!(outcome.statuses(), [JobItemStatus::Ok, JobItemStatus::Ok]);
    assert_eq!(
        outcome.saved(),
        ["crop_layout_p1_1.png", "crop_layout_p1_2.png"]
    );
    assert_eq!(
        outcome.finished,
        JobFinishedPayload {
            succeeded: 2,
            failed: 0,
            no_pages: 0,
            unprocessed: 0,
            cancelled: false,
        }
    );
    assert_eq!(
        files_in(out.path()),
        ["crop_layout_p1_1.png", "crop_layout_p1_2.png"]
    );
    let photo_png = fs::read(out.path().join("crop_layout_p1_1.png")).expect("reading the photo");
    assert_eq!(png_size(&photo_png), (480, 320), "the pixels of photo.jpg");
    let chart_png = fs::read(out.path().join("crop_layout_p1_2.png")).expect("reading the chart");
    assert_ne!(png_size(&chart_png), (480, 320));
    assert_no_temp_files(out.path());
}

#[test]
fn the_ranges_in_the_reverse_order_get_the_same_names_and_are_written_in_position_order() {
    let (state, pdf, out) = exporting("crop_layout.pdf");
    let (photo, chart) = photo_and_chart(&state, pdf.id);

    let outcome = export(&state, pdf.id, &[chart, photo], options()).expect("starting");

    assert_eq!(outcome.keys(), [1, 2], "the photo is first");
    assert_eq!(
        outcome.saved(),
        ["crop_layout_p1_1.png", "crop_layout_p1_2.png"]
    );
    let photo_png = fs::read(out.path().join("crop_layout_p1_1.png")).expect("reading the photo");
    assert_eq!(png_size(&photo_png), (480, 320));
}

#[test]
fn the_pages_are_written_in_page_order_whatever_order_they_are_given_in() {
    let (state, pdf, out) = exporting("shapes.pdf");
    let regions = [
        manual(30, 3, [10.0, 10.0, 60.0, 60.0]),
        manual(10, 1, [10.0, 10.0, 60.0, 60.0]),
        manual(20, 2, [10.0, 10.0, 60.0, 60.0]),
    ];

    let outcome = export(
        &state,
        pdf.id,
        &regions,
        CropOptions {
            dpi: 72,
            ..options()
        },
    )
    .expect("starting");

    assert_eq!(outcome.keys(), [10, 20, 30]);
    assert_eq!(
        outcome.saved(),
        ["shapes_p1_1.png", "shapes_p2_1.png", "shapes_p3_1.png"]
    );
    assert_eq!(files_in(out.path()).len(), 3);
}

#[test]
fn a_name_that_is_taken_gets_a_number() {
    let (state, pdf, out) = exporting("crop_layout.pdf");
    let (photo, chart) = photo_and_chart(&state, pdf.id);
    fs::write(out.path().join("crop_layout_p1_1.png"), b"mine").expect("writing a file");
    fs::write(out.path().join("CROP_LAYOUT_P1_2.PNG"), b"mine too").expect("writing a file");

    let outcome = export(&state, pdf.id, &[photo, chart], options()).expect("starting");

    assert_eq!(
        outcome.saved(),
        ["crop_layout_p1_1 (1).png", "crop_layout_p1_2 (1).png"]
    );
    assert_eq!(
        fs::read(out.path().join("crop_layout_p1_1.png")).expect("reading"),
        b"mine"
    );
}

#[test]
fn the_format_decides_the_extension_and_the_bytes() {
    for (format, extension, magic) in [
        (RenderFormatChoice::Png, "png", &b"\x89PNG"[..]),
        (RenderFormatChoice::Jpeg, "jpg", &b"\xFF\xD8\xFF"[..]),
        (RenderFormatChoice::Webp, "webp", &b"RIFF"[..]),
    ] {
        let (state, pdf, out) = exporting("shapes.pdf");
        let regions = [manual(1, 1, [10.0, 10.0, 110.0, 90.0])];

        let outcome = export(
            &state,
            pdf.id,
            &regions,
            CropOptions {
                format,
                dpi: 72,
                ..options()
            },
        )
        .expect("starting");

        let name = format!("shapes_p1_1.{extension}");
        assert_eq!(outcome.saved(), std::slice::from_ref(&name));
        let bytes = fs::read(out.path().join(name)).expect("reading the image");
        assert!(bytes.starts_with(magic), "for {extension}");
    }
}

#[test]
fn progress_counts_the_ranges_and_names_the_pdf() {
    let (state, pdf, _out) = exporting("shapes.pdf");
    let regions = [
        manual(1, 1, [10.0, 10.0, 60.0, 60.0]),
        manual(2, 2, [10.0, 10.0, 60.0, 60.0]),
    ];

    let outcome = export(
        &state,
        pdf.id,
        &regions,
        CropOptions {
            dpi: 72,
            ..options()
        },
    )
    .expect("starting");

    let done: Vec<u32> = outcome.progress.iter().map(|p| p.done).collect();
    assert_eq!(done, [0, 1, 2]);
    for progress in &outcome.progress {
        assert_eq!(progress.total, 2);
        assert_eq!(progress.current.as_deref(), Some("shapes.pdf"));
    }
}

#[test]
fn a_region_as_large_as_the_page_text_is_taken() {
    let (state, pdf, out) = exporting("shapes.pdf");
    // The screen reads the sizes as text; a range limited to them must fit.
    let sizes: Vec<[f64; 4]> = pdf
        .pages
        .iter()
        .map(|page| {
            let text = serde_json::to_string(page).expect("serializing the size");
            let size: serde_json::Value = serde_json::from_str(&text).expect("reading it back");
            let edge = |key: &str| size[key].as_f64().expect("a number");
            [0.0, 0.0, edge("widthPt"), edge("heightPt")]
        })
        .collect();
    let regions: Vec<CropRegionInput> = (0u32..)
        .zip(&sizes)
        .map(|(index, rect)| manual(u64::from(index) + 1, index + 1, *rect))
        .collect();

    let outcome = export(
        &state,
        pdf.id,
        &regions,
        CropOptions {
            dpi: 36,
            ..options()
        },
    )
    .expect("starting");

    assert_eq!(outcome.finished.succeeded as usize, regions.len());
    assert_eq!(files_in(out.path()).len(), regions.len());
}

// ---- Refused before it starts ----

/// Starts `regions` and checks that it was refused with `expected`, that
/// nothing was written, and that a new export can start afterwards.
fn assert_refused(
    state: &AppState,
    id: u64,
    out: &Path,
    regions: &[CropRegionInput],
    options: CropOptions,
    expected: ErrorCode,
) {
    assert_eq!(refused(export(state, id, regions, options)), expected);
    assert!(
        !state.is_running.load(Ordering::SeqCst),
        "a refusal puts is_running back"
    );
    assert!(files_in(out).is_empty(), "nothing is written");
}

#[test]
fn bad_requests_are_refused_as_a_whole_and_write_nothing() {
    let (state, pdf, out) = exporting("shapes.pdf");
    let page = pdf.pages[0];
    let (width, height) = (f64::from(page.width_pt), f64::from(page.height_pt));
    let good = manual(1, 1, [10.0, 10.0, 60.0, 60.0]);
    let refuse = |regions: &[CropRegionInput], options: CropOptions| {
        assert_refused(
            &state,
            pdf.id,
            out.path(),
            regions,
            options,
            ErrorCode::InvalidParams,
        );
    };

    // The dpi is outside design §4.4.
    for dpi in [0, 35, 1001] {
        refuse(
            std::slice::from_ref(&good),
            CropOptions { dpi, ..options() },
        );
    }
    // No ranges, and a key used twice.
    refuse(&[], options());
    refuse(
        &[good.clone(), manual(1, 2, [10.0, 10.0, 60.0, 60.0])],
        options(),
    );
    // A page outside the PDF.
    refuse(&[manual(1, 0, [10.0, 10.0, 60.0, 60.0])], options());
    refuse(&[manual(1, 4, [10.0, 10.0, 60.0, 60.0])], options());
    // A rectangle outside the page, empty, reversed or not a number. One bad
    // range keeps the good one before it from being written.
    for rect in [
        [-1.0, 10.0, 60.0, 60.0],
        [10.0, -1.0, 60.0, 60.0],
        [10.0, 10.0, width + 1.0, 60.0],
        [10.0, 10.0, 60.0, height + 1.0],
        [20.0, 10.0, 20.0, 60.0],
        [10.0, 20.0, 60.0, 20.0],
        [60.0, 10.0, 10.0, 60.0],
        [f64::NAN, 10.0, 60.0, 60.0],
        [10.0, 10.0, 60.0, f64::NAN],
    ] {
        refuse(&[good.clone(), manual(2, 1, rect)], options());
    }
    // An image the page does not have.
    refuse(
        &[CropRegionInput {
            image_index: Some(7),
            ..good.clone()
        }],
        options(),
    );
}

#[test]
fn an_image_index_the_page_lacks_refuses_the_export_even_after_a_good_photo() {
    let (state, pdf, out) = exporting("crop_layout.pdf");
    let (photo, chart) = photo_and_chart(&state, pdf.id);
    let missing = CropRegionInput {
        key: 3,
        image_index: Some(photo.image_index.expect("a photo") + 1),
        ..chart
    };

    assert_refused(
        &state,
        pdf.id,
        out.path(),
        &[photo, missing],
        options(),
        ErrorCode::InvalidParams,
    );
}

#[test]
fn an_unknown_id_is_refused() {
    let (state, pdf, out) = exporting("shapes.pdf");
    let regions = [manual(1, 1, [10.0, 10.0, 60.0, 60.0])];

    assert_refused(
        &state,
        pdf.id + 1000,
        out.path(),
        &regions,
        options(),
        ErrorCode::UnknownHandle,
    );
}

#[test]
fn a_crop_pdf_that_could_not_be_opened_answers_with_its_own_error() {
    let state = state_with(config());
    let pdf = open_active(&state, "corrupt.pdf");
    assert!(pdf.error.is_some());
    let regions = [manual(1, 1, [10.0, 10.0, 60.0, 60.0])];

    let error = export(&state, pdf.id, &regions, options())
        .err()
        .expect("refused");

    assert_eq!(Some(error), pdf.error);
    assert!(!state.is_running.load(Ordering::SeqCst));
}

#[test]
fn an_export_during_a_conversion_is_refused_and_leaves_the_conversion_running() {
    let (state, pdf, out) = exporting("shapes.pdf");
    let regions = [manual(1, 1, [10.0, 10.0, 60.0, 60.0])];
    state.is_running.store(true, Ordering::SeqCst);

    let code = refused(export(&state, pdf.id, &regions, options()));

    assert_eq!(code, ErrorCode::ConversionRunning);
    assert!(
        state.is_running.load(Ordering::SeqCst),
        "the other conversion is still running"
    );
    assert!(files_in(out.path()).is_empty());
}

#[test]
fn a_folder_that_is_not_chosen_is_refused() {
    let (state, pdf, out) = exporting("shapes.pdf");
    *state.crop_output_dir.lock().expect("output folder lock") = None;
    let regions = [manual(1, 1, [10.0, 10.0, 60.0, 60.0])];

    assert_refused(
        &state,
        pdf.id,
        out.path(),
        &regions,
        options(),
        ErrorCode::InvalidParams,
    );
}

#[test]
fn a_folder_that_is_gone_is_refused_and_goes_back_to_not_chosen_only_for_crops() {
    let (state, pdf, _out) = exporting("shapes.pdf");
    let scratch = TempDir::new().expect("temp dir");
    let config_dir = scratch.path().join("config");
    let crop_dir = scratch.path().join("crops");
    let images_dir = scratch.path().join("images");
    fs::create_dir_all(&crop_dir).expect("creating a folder");
    fs::create_dir_all(&images_dir).expect("creating a folder");
    restore_settings(&state, &config_dir);
    let _ = apply_picked_dir(&state, OutputKind::ImagesToPdf, images_dir.clone());
    let _ = apply_picked_dir(&state, OutputKind::Crop, crop_dir.clone());
    fs::remove_dir_all(&crop_dir).expect("removing the folder");
    let regions = [manual(1, 1, [10.0, 10.0, 60.0, 60.0])];

    let code = refused(export(&state, pdf.id, &regions, options()));

    assert_eq!(code, ErrorCode::OutputDirMissing);
    assert!(!state.is_running.load(Ordering::SeqCst));
    assert_eq!(*state.crop_output_dir.lock().expect("lock"), None);
    assert_eq!(
        *state.images_output_dir.lock().expect("lock"),
        Some(images_dir.clone())
    );
    let file = load_settings(&config_dir);
    assert_eq!(file.crop.output_dir, None);
    assert_eq!(file.images_to_pdf.output_dir, Some(images_dir));
}

// ---- Failures of one range, the end of the job ----

#[test]
fn a_range_that_is_too_large_fails_alone_and_the_next_one_is_written() {
    let (state, pdf, out) = exporting("mixed_sizes.pdf");
    let big = pdf.pages[1];
    assert!(big.width_pt >= 3000.0 && big.height_pt >= 3000.0);
    let regions = [
        manual(1, 1, [10.0, 10.0, 60.0, 60.0]),
        // The whole 3000 pt page at 1000 dpi is far above MAX_RENDER_PIXELS.
        manual(
            2,
            2,
            [0.0, 0.0, f64::from(big.width_pt), f64::from(big.height_pt)],
        ),
        manual(3, 2, [100.0, 100.0, 200.0, 200.0]),
    ];

    let outcome = export(
        &state,
        pdf.id,
        &regions,
        CropOptions {
            dpi: 1000,
            ..options()
        },
    )
    .expect("starting");

    assert_eq!(outcome.keys(), [1, 2, 3]);
    assert_eq!(
        outcome.statuses(),
        [JobItemStatus::Ok, JobItemStatus::Failed, JobItemStatus::Ok]
    );
    assert_eq!(outcome.error_code(1), Some(ErrorCode::RenderTooLarge));
    assert!(outcome.items[1].outputs.is_empty());
    assert_eq!(
        outcome.finished,
        JobFinishedPayload {
            succeeded: 2,
            failed: 1,
            no_pages: 0,
            unprocessed: 0,
            cancelled: false,
        }
    );
    assert_eq!(
        files_in(out.path()),
        ["mixed_sizes_p1_1.png", "mixed_sizes_p2_2.png"],
        "the failed range still has its number"
    );
    assert_no_temp_files(out.path());
}

#[test]
fn cancelling_after_a_range_leaves_the_rest_unprocessed_without_items() {
    let (state, pdf, out) = exporting("shapes.pdf");
    let regions = [
        manual(1, 1, [10.0, 10.0, 60.0, 60.0]),
        manual(2, 2, [10.0, 10.0, 60.0, 60.0]),
        manual(3, 3, [10.0, 10.0, 60.0, 60.0]),
    ];
    let cancel = Arc::clone(&state.cancel_flag);

    let outcome = export_with(
        &state,
        pdf.id,
        &regions,
        CropOptions {
            dpi: 72,
            ..options()
        },
        move |_| cancel.store(true, Ordering::SeqCst),
    )
    .expect("starting");

    assert_eq!(outcome.keys(), [1]);
    assert_eq!(
        outcome.finished,
        JobFinishedPayload {
            succeeded: 1,
            failed: 0,
            no_pages: 0,
            unprocessed: 2,
            cancelled: true,
        }
    );
    assert_eq!(files_in(out.path()), ["shapes_p1_1.png"]);
    assert_no_temp_files(out.path());
}

#[test]
fn a_cancel_from_before_the_export_does_not_carry_over_to_it() {
    let (state, pdf, _out) = exporting("shapes.pdf");
    state.cancel_flag.store(true, Ordering::SeqCst);
    let regions = [manual(1, 1, [10.0, 10.0, 60.0, 60.0])];

    let outcome = export(
        &state,
        pdf.id,
        &regions,
        CropOptions {
            dpi: 72,
            ..options()
        },
    )
    .expect("starting");

    assert_eq!(outcome.finished.succeeded, 1);
    assert!(!outcome.finished.cancelled);
}

#[test]
fn a_write_failure_fails_that_range_and_leaves_the_rest_unprocessed() {
    let (state, pdf, out) = exporting("shapes.pdf");
    let regions = [
        manual(1, 1, [10.0, 10.0, 60.0, 60.0]),
        manual(2, 2, [10.0, 10.0, 60.0, 60.0]),
        manual(3, 3, [10.0, 10.0, 60.0, 60.0]),
    ];
    // The folder goes away once the first range is saved, as when a disk is
    // pulled out: the second range cannot be written.
    let folder = out.path().to_path_buf();

    let outcome = export_with(
        &state,
        pdf.id,
        &regions,
        CropOptions {
            dpi: 72,
            ..options()
        },
        move |item| {
            if item.id == 1 {
                fs::remove_dir_all(&folder).expect("removing the folder");
            }
        },
    )
    .expect("starting");

    assert_eq!(outcome.keys(), [1, 2], "the third range is not tried");
    assert_eq!(
        outcome.statuses(),
        [JobItemStatus::Ok, JobItemStatus::Failed]
    );
    assert_eq!(outcome.error_code(1), Some(ErrorCode::WriteFailed));
    assert_eq!(
        outcome.finished,
        JobFinishedPayload {
            succeeded: 1,
            failed: 1,
            no_pages: 0,
            unprocessed: 1,
            cancelled: false,
        }
    );
}

#[test]
fn a_worker_that_times_out_fails_that_range_and_the_rest_is_not_tried() {
    // The first request after Open gets a millisecond; a page at 300 dpi
    // takes far longer. Opening and closing keep their own, longer limit.
    let (state, pdf, out) = exporting_with(
        config().with_render_timeout(Duration::from_millis(1)),
        "shapes.pdf",
    );
    // Whole pages, so that rendering takes far longer than the limit.
    let regions: Vec<CropRegionInput> = (0u32..)
        .zip(&pdf.pages)
        .map(|(index, page)| {
            let rect = [
                0.0,
                0.0,
                f64::from(page.width_pt),
                f64::from(page.height_pt),
            ];
            manual(u64::from(index) + 1, index + 1, rect)
        })
        .collect();
    assert_eq!(regions.len(), 3);

    let outcome = export(
        &state,
        pdf.id,
        &regions,
        CropOptions {
            dpi: 600,
            ..options()
        },
    )
    .expect("starting");

    assert_eq!(outcome.keys(), [1]);
    assert_eq!(outcome.error_code(0), Some(ErrorCode::WorkerTimeout));
    assert_eq!(
        outcome.finished,
        JobFinishedPayload {
            succeeded: 0,
            failed: 1,
            no_pages: 0,
            unprocessed: 2,
            cancelled: false,
        }
    );
    assert!(files_in(out.path()).is_empty());
}

#[test]
fn a_worker_that_cannot_start_fails_the_first_range_and_the_rest_is_not_tried() {
    let (state, pdf, out) = exporting("shapes.pdf");
    // The PDF is open already; only the export's worker cannot start.
    let mut broken = state.clone();
    broken.pool = WorkerPool::new(WorkerPoolConfig::new(
        std::env::temp_dir().join("pdfconv-no-worker-here"),
        Vec::<String>::new(),
    ));
    let regions = [
        manual(1, 1, [10.0, 10.0, 60.0, 60.0]),
        manual(2, 2, [10.0, 10.0, 60.0, 60.0]),
    ];

    let outcome = export(&broken, pdf.id, &regions, options()).expect("starting");

    assert_eq!(outcome.keys(), [1]);
    assert_eq!(outcome.error_code(0), Some(ErrorCode::WorkerCrashed));
    assert_eq!(outcome.finished.failed, 1);
    assert_eq!(outcome.finished.unprocessed, 1);
    assert!(files_in(out.path()).is_empty());
}
