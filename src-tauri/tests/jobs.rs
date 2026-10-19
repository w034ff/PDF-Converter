//! Integration tests for conversion execution and jobs (work-plan T08, design §6.2-§6.5).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "integration-test helpers: a failure here should fail the test"
)]

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pdf_converter_lib::AppState;
use pdf_converter_lib::error::ErrorCode;
use pdf_converter_lib::items::{add_dropped, add_images, add_pdfs, remove_items, unlock_pdf};
use pdf_converter_lib::jobs::{
    JobCallbacks, JobFinishedPayload, JobItemPayload, JobItemStatus, JobProgressPayload,
    PageOrientationChoice, PageSizeChoice, PdfOutput, RenderColorChoice, RenderFormatChoice,
    RunningGuard, check_page_range_internal, list_existing_files, output_password, pdf_conformance,
    run_images_to_pdfs, run_pdfs_to_images, run_save_merged_pdf, save_atomic,
    start_images_to_pdfs_internal, start_pdfs_to_images_internal,
};

#[path = "../../crates/core/tests/support/fixture_passwords.rs"]
mod fixture_passwords;
use fixture_passwords::ENCRYPTED_USER_PASSWORD;
use pdf_converter_lib::settings::{
    OutputKind, apply_picked_dir, get_settings_internal, load_settings, restore_settings,
};
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig, default_worker_limit};
use pdfconv_core::{OutputPassword, PdfConformance, parse_page_range};
use pdfconv_worker::WORKER_FLAG;
#[cfg(feature = "test-hooks")]
use pdfconv_worker::server::CRASH_ON_OPEN_FILE_NAME;
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

fn state() -> AppState {
    let config = WorkerPoolConfig::new(
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
    .with_render_timeout(Duration::from_secs(60));
    AppState::new(WorkerPool::new(config))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/core/tests/fixtures")
        .join(name)
}

fn copy_fixture(dir: &Path, name: &str, as_name: &str) -> PathBuf {
    let path = dir.join(as_name);
    fs::copy(fixture(name), &path).expect("copying fixture");
    path
}

/// Collects callback notifications into thread-safe vectors for inspection.
struct TestRecorder {
    progress: Arc<Mutex<Vec<JobProgressPayload>>>,
    items: Arc<Mutex<Vec<JobItemPayload>>>,
    finished: Arc<Mutex<Option<JobFinishedPayload>>>,
    is_running_at_finished: Arc<Mutex<Option<bool>>>,
}

impl TestRecorder {
    fn new() -> Self {
        Self {
            progress: Arc::new(Mutex::new(Vec::new())),
            items: Arc::new(Mutex::new(Vec::new())),
            finished: Arc::new(Mutex::new(None)),
            is_running_at_finished: Arc::new(Mutex::new(None)),
        }
    }

    fn callbacks(
        &self,
    ) -> JobCallbacks<
        impl Fn(JobProgressPayload) + Send + Sync + 'static,
        impl Fn(JobItemPayload) + Send + Sync + 'static,
        impl FnOnce(JobFinishedPayload) + Send + Sync + 'static,
    > {
        let p = Arc::clone(&self.progress);
        let i = Arc::clone(&self.items);
        let f = Arc::clone(&self.finished);
        JobCallbacks {
            on_progress: move |prog| p.lock().unwrap().push(prog),
            on_item: move |item| i.lock().unwrap().push(item),
            on_finished: move |fin| *f.lock().unwrap() = Some(fin),
        }
    }

    fn callbacks_recording_running(
        &self,
        is_running: Arc<AtomicBool>,
    ) -> JobCallbacks<
        impl Fn(JobProgressPayload) + Send + Sync + 'static,
        impl Fn(JobItemPayload) + Send + Sync + 'static,
        impl FnOnce(JobFinishedPayload) + Send + Sync + 'static,
    > {
        let p = Arc::clone(&self.progress);
        let i = Arc::clone(&self.items);
        let f = Arc::clone(&self.finished);
        let r = Arc::clone(&self.is_running_at_finished);
        JobCallbacks {
            on_progress: move |prog| p.lock().unwrap().push(prog),
            on_item: move |item| i.lock().unwrap().push(item),
            on_finished: move |fin| {
                *r.lock().unwrap() = Some(is_running.load(Ordering::SeqCst));
                *f.lock().unwrap() = Some(fin);
            },
        }
    }

    fn is_running_at_finished(&self) -> Option<bool> {
        *self.is_running_at_finished.lock().unwrap()
    }

    fn items(&self) -> Vec<JobItemPayload> {
        self.items.lock().unwrap().clone()
    }

    fn progress(&self) -> Vec<JobProgressPayload> {
        self.progress.lock().unwrap().clone()
    }

    fn finished(&self) -> JobFinishedPayload {
        self.finished
            .lock()
            .unwrap()
            .clone()
            .expect("finished event received")
    }

    fn wait_finished(&self, timeout: Duration) -> JobFinishedPayload {
        let start = std::time::Instant::now();
        loop {
            if let Some(f) = self.finished.lock().unwrap().clone() {
                return f;
            }
            if start.elapsed() > timeout {
                panic!("timed out waiting for finished payload");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Asserts that no temporary files starting with '.' remain in `dir`.
fn assert_no_temp_files(dir: &Path) {
    for entry in fs::read_dir(dir).expect("reading dir").flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        assert!(
            !name_str.starts_with('.'),
            "unexpected temporary file found: {name_str}"
        );
    }
}

#[test]
fn images_to_pdfs_each_mixed_success_and_failure() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let valid_path1 = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let corrupt_path = copy_fixture(temp_in.path(), "corrupt.png", "corrupt.png");
    let valid_path2 = copy_fixture(temp_in.path(), "logo_alpha.png", "logo.png");

    let add_res = add_images(&app_state, &[valid_path1, corrupt_path, valid_path2]);
    assert_eq!(add_res.added.len(), 3);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    let recorder = TestRecorder::new();
    run_images_to_pdfs(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("run_images_to_pdfs should succeed");

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 2);
    assert_eq!(finished.failed, 1);
    assert_eq!(finished.unprocessed, 0);
    assert!(!finished.cancelled);

    let items = recorder.items();
    assert_eq!(items.len(), 3);

    let ok_items: Vec<_> = items
        .iter()
        .filter(|i| i.status == JobItemStatus::Ok)
        .collect();
    let failed_items: Vec<_> = items
        .iter()
        .filter(|i| i.status == JobItemStatus::Failed)
        .collect();
    assert_eq!(ok_items.len(), 2);
    assert_eq!(failed_items.len(), 1);

    // Corrupt image must report DecodeFailed
    assert_eq!(
        failed_items[0].error.as_ref().map(|e| e.code),
        Some(ErrorCode::DecodeFailed)
    );

    // Successful outputs exist on disk
    for item in ok_items {
        assert_eq!(item.outputs.len(), 1);
        let out_path = temp_out.path().join(&item.outputs[0]);
        assert!(out_path.exists());
    }

    assert_no_temp_files(temp_out.path());
}

#[test]
fn pdfs_to_images_mixed_success_and_failure() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let shapes_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let corrupt_path = copy_fixture(temp_in.path(), "corrupt.pdf", "corrupt.pdf");
    let enc_path = copy_fixture(temp_in.path(), "encrypted.pdf", "encrypted.pdf");

    let add_res = add_pdfs(&app_state, &[shapes_path, corrupt_path, enc_path]);
    assert_eq!(add_res.added.len(), 3);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    let page_set = parse_page_range("1-2").unwrap();
    let recorder = TestRecorder::new();

    run_pdfs_to_images(
        &app_state,
        &ids,
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("run_pdfs_to_images should succeed");

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 2);
    assert_eq!(finished.no_pages, 0);
    assert_eq!(finished.unprocessed, 0);

    let items = recorder.items();
    let ok_items: Vec<_> = items
        .iter()
        .filter(|i| i.status == JobItemStatus::Ok)
        .collect();
    assert_eq!(ok_items.len(), 1);
    assert_eq!(ok_items[0].outputs.len(), 2);
    for out in &ok_items[0].outputs {
        assert!(temp_out.path().join(out).exists());
    }

    let failed_items: Vec<_> = items
        .iter()
        .filter(|i| i.status == JobItemStatus::Failed)
        .collect();
    assert_eq!(failed_items.len(), 2);

    assert_no_temp_files(temp_out.path());
}

#[test]
fn unlocked_encrypted_pdf_converts_to_image() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let enc_path = copy_fixture(temp_in.path(), "encrypted.pdf", "encrypted.pdf");
    let add_res = add_pdfs(&app_state, &[enc_path]);
    let id = add_res.added[0].id;

    let unlock_res = unlock_pdf(&app_state, id, ENCRYPTED_USER_PASSWORD).expect("unlock succeeds");
    assert_eq!(unlock_res.items.len(), 1);
    assert_eq!(unlock_res.items[0].page_count, 1);

    let page_set = parse_page_range("1").unwrap();
    let recorder = TestRecorder::new();

    run_pdfs_to_images(
        &app_state,
        &[id],
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("run_pdfs_to_images should succeed");

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 0);

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, JobItemStatus::Ok);
    assert_eq!(items[0].outputs.len(), 1);
    assert!(temp_out.path().join(&items[0].outputs[0]).exists());
    assert_no_temp_files(temp_out.path());
}

#[test]
fn pdfs_to_images_no_pages_and_out_of_bounds() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let shapes_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let add_res = add_pdfs(&app_state, &[shapes_path]);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    // shapes.pdf has 3 pages; range 10-20 has no matching pages
    let page_set = parse_page_range("10-20").unwrap();
    let recorder = TestRecorder::new();

    run_pdfs_to_images(
        &app_state,
        &ids,
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("should succeed with no pages");

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 0);
    assert_eq!(finished.failed, 0);
    assert_eq!(finished.no_pages, 1);

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, JobItemStatus::NoPages);
    assert!(items[0].outputs.is_empty());
}

#[test]
fn pdfs_to_images_partial_failure_with_mixed_sizes() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let mixed_path = copy_fixture(temp_in.path(), "mixed_sizes.pdf", "mixed.pdf");
    let add_res = add_pdfs(&app_state, &[mixed_path]);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    // At 300 dpi, page 2 of mixed_sizes.pdf (3000x3000pt) exceeds MAX_RENDER_PIXELS and fails with RenderTooLarge.
    // Pages 1 and 3 (A4) succeed.
    let page_set = parse_page_range("1-3").unwrap();
    let recorder = TestRecorder::new();

    run_pdfs_to_images(
        &app_state,
        &ids,
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        300,
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("run_pdfs_to_images should succeed");

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 0);
    assert_eq!(finished.failed, 1);

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, JobItemStatus::Partial);
    assert_eq!(items[0].outputs.len(), 2); // pages 1 and 3 succeeded
    assert_eq!(items[0].failed_pages, Some(vec![2]));
    assert_eq!(
        items[0].error.as_ref().map(|e| e.code),
        Some(ErrorCode::RenderTooLarge)
    );

    for out in &items[0].outputs {
        assert!(temp_out.path().join(out).exists());
    }

    assert_no_temp_files(temp_out.path());
}

#[test]
fn naming_collision_resolution_and_noclobber() {
    let temp_dir = TempDir::new().unwrap();
    let out_dir = temp_dir.path();

    // Pre-create file in output directory
    let existing_file = out_dir.join("photo.pdf");
    fs::write(&existing_file, b"existing content").unwrap();

    let used_names = Mutex::new(HashSet::new());

    // First save: photo.pdf exists on disk -> should save as photo (1).pdf
    let saved1 = save_atomic(out_dir, "photo.pdf", b"pdf 1", &used_names).unwrap();
    assert_eq!(saved1, "photo (1).pdf");
    assert_eq!(fs::read(out_dir.join("photo (1).pdf")).unwrap(), b"pdf 1");
    // Existing file was not clobbered
    assert_eq!(fs::read(&existing_file).unwrap(), b"existing content");

    // Second save: both photo.pdf and photo (1).pdf are taken -> should save as photo (2).pdf
    let saved2 = save_atomic(out_dir, "photo.pdf", b"pdf 2", &used_names).unwrap();
    assert_eq!(saved2, "photo (2).pdf");
    assert_eq!(fs::read(out_dir.join("photo (2).pdf")).unwrap(), b"pdf 2");

    // Collision at the moment of persist: pre-create photo (3).pdf behind our back
    fs::write(out_dir.join("photo (3).pdf"), b"third party").unwrap();
    let saved3 = save_atomic(out_dir, "photo (3).pdf", b"pdf 3", &used_names).unwrap();
    assert_eq!(saved3, "photo (3) (1).pdf");
    assert_eq!(
        fs::read(out_dir.join("photo (3).pdf")).unwrap(),
        b"third party"
    );
    assert_eq!(
        fs::read(out_dir.join("photo (3) (1).pdf")).unwrap(),
        b"pdf 3"
    );

    assert_no_temp_files(out_dir);
}

#[test]
fn cancellation_images_to_pdfs_leaves_no_temp_files() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path1 = copy_fixture(temp_in.path(), "photo.jpg", "img1.jpg");
    let path2 = copy_fixture(temp_in.path(), "logo_alpha.png", "img2.png");
    let path3 = copy_fixture(temp_in.path(), "deep16.png", "img3.png");

    let add_res = add_images(&app_state, &[path1, path2, path3]);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    // Signal cancel before starting
    app_state.cancel_flag.store(true, Ordering::SeqCst);

    let recorder = TestRecorder::new();
    run_images_to_pdfs(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        temp_out.path(),
        recorder.callbacks(),
    )
    .unwrap();

    let finished = recorder.finished();
    assert!(finished.cancelled);
    assert_eq!(finished.succeeded, 0);
    assert_eq!(finished.unprocessed, 3);

    // No output or temp files created
    assert_no_temp_files(temp_out.path());
    let remaining_files = list_existing_files(temp_out.path()).unwrap();
    assert!(remaining_files.is_empty());
}

#[test]
fn cancellation_merged_pdf_writes_no_file() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path1 = copy_fixture(temp_in.path(), "photo.jpg", "img1.jpg");
    let path2 = copy_fixture(temp_in.path(), "logo_alpha.png", "img2.png");

    let add_res = add_images(&app_state, &[path1, path2]);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    // Signal cancellation before running
    app_state.cancel_flag.store(true, Ordering::SeqCst);

    let dest_path = temp_out.path().join("merged.pdf");
    let recorder = TestRecorder::new();

    let res = run_save_merged_pdf(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest_path,
        recorder.callbacks(),
    )
    .unwrap();

    assert_eq!(res.saved_name, None);
    assert!(!dest_path.exists());
    assert_no_temp_files(temp_out.path());

    let finished = recorder.finished();
    assert!(finished.cancelled);
    assert_eq!(finished.succeeded, 0);
}

#[test]
fn check_page_range_sums_valid_items_and_rejects_invalid_syntax() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();

    let shapes_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let corrupt_path = copy_fixture(temp_in.path(), "corrupt.pdf", "corrupt.pdf");
    let add_res = add_pdfs(&app_state, &[shapes_path, corrupt_path]);

    let shapes_id = add_res.added[0].id;
    let corrupt_id = add_res.added[1].id;
    let nonexistent_id = 99999u64;

    // shapes.pdf has 3 pages. "1-2, 5" matches pages 1 and 2 (count = 2).
    // Corrupt PDF has error and is skipped. Nonexistent ID is skipped.
    let res = check_page_range_internal(
        &app_state,
        "1-2, 5",
        &[shapes_id, corrupt_id, nonexistent_id],
    )
    .unwrap();
    assert_eq!(res.total_pages, 2);
    // The intervals are normalized and not clipped to any PDF's pages.
    assert_eq!(res.intervals, vec![(1, 2), (5, 5)]);

    let res = check_page_range_internal(&app_state, "5, 1-3, 2", &[shapes_id]).unwrap();
    assert_eq!(res.intervals, vec![(1, 3), (5, 5)]);

    // Invalid syntax returns InvalidPageRange
    let err = check_page_range_internal(&app_state, "abc", &[shapes_id]).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidPageRange);
    assert_eq!(err.detail, Some("abc".to_string()));
}

#[test]
fn check_page_range_resolves_end_per_pdf() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();

    let shapes_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let enc_path = copy_fixture(temp_in.path(), "encrypted.pdf", "encrypted.pdf");
    let add_res = add_pdfs(&app_state, &[shapes_path, enc_path]);
    let shapes_id = add_res.added[0].id;
    let enc_id = add_res.added[1].id;
    unlock_pdf(&app_state, enc_id, ENCRYPTED_USER_PASSWORD).expect("unlock succeeds");
    let ids = [shapes_id, enc_id];

    // shapes.pdf has 3 pages and encrypted.pdf 1: `end` is page 3 of one and page 1 of the other.
    let res = check_page_range_internal(&app_state, "end", &ids).unwrap();
    assert_eq!(res.total_pages, 2);
    assert!(res.intervals.is_empty());
    assert!(res.last_page);

    // `3-end` has no page in encrypted.pdf.
    let res = check_page_range_internal(&app_state, "3-end", &ids).unwrap();
    assert_eq!(res.total_pages, 1);
    assert_eq!(res.intervals, vec![(3, u32::MAX)]);
    assert!(!res.last_page);

    let err = check_page_range_internal(&app_state, "end-2", &ids).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidPageRange);
    assert_eq!(err.detail, Some("end-2".to_string()));
}

#[test]
fn pdfs_to_images_end_converts_the_last_page() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let shapes_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let add_res = add_pdfs(&app_state, &[shapes_path]);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    let page_set = parse_page_range("end").unwrap();
    let recorder = TestRecorder::new();

    run_pdfs_to_images(
        &app_state,
        &ids,
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("run_pdfs_to_images should succeed");

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, JobItemStatus::Ok);
    assert_eq!(items[0].outputs.len(), 1);
    assert!(
        items[0].outputs[0].ends_with("p3.png"),
        "{:?}",
        items[0].outputs
    );
}

#[cfg(feature = "test-hooks")]
#[test]
fn crash_on_open_file_name_recovers_and_continues_batch() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    // Copy shapes.pdf as crash-on-open-for-test.pdf
    let crash_path = copy_fixture(temp_in.path(), "shapes.pdf", CRASH_ON_OPEN_FILE_NAME);
    let shapes_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");

    let add_res = add_pdfs(&app_state, &[crash_path, shapes_path]);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    let page_set = parse_page_range("1").unwrap();
    let recorder = TestRecorder::new();

    run_pdfs_to_images(
        &app_state,
        &ids,
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("batch execution should complete");

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 1);

    let items = recorder.items();
    assert_eq!(items.len(), 2);

    let crash_item = items.iter().find(|i| i.id == ids[0]).unwrap();
    assert_eq!(crash_item.status, JobItemStatus::Failed);
    assert_eq!(
        crash_item.error.as_ref().map(|e| e.code),
        Some(ErrorCode::WorkerCrashed)
    );

    let ok_item = items.iter().find(|i| i.id == ids[1]).unwrap();
    assert_eq!(ok_item.status, JobItemStatus::Ok);
    assert_eq!(ok_item.outputs.len(), 1);
    assert!(temp_out.path().join(&ok_item.outputs[0]).exists());

    assert_no_temp_files(temp_out.path());
}

#[test]
fn conversion_running_rejects_operations() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let add_res = add_images(&app_state, &[path]);
    let id = add_res.added[0].id;

    *app_state.images_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());
    *app_state.pdfs_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());

    // Flag conversion running
    app_state.is_running.store(true, Ordering::SeqCst);

    let recorder = TestRecorder::new();
    let err_img = start_images_to_pdfs_internal(
        &app_state,
        &[id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        PdfConformance::Standard,
        None,
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err_img.code, ErrorCode::ConversionRunning);

    let recorder = TestRecorder::new();
    let err_pdf = start_pdfs_to_images_internal(
        &app_state,
        &[id],
        "1",
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err_pdf.code, ErrorCode::ConversionRunning);

    let err_remove = remove_items(&app_state, &[id]).unwrap_err();
    assert_eq!(err_remove.code, ErrorCode::ConversionRunning);

    let dropped_paths = [temp_in.path().join("photo.jpg")];
    let dropped = add_dropped(&app_state, &dropped_paths);
    assert!(dropped.images.is_empty());
    assert!(dropped.pdfs.is_empty());
    assert_eq!(
        dropped.error.as_ref().map(|e| e.code),
        Some(ErrorCode::ConversionRunning)
    );

    // ConversionRunning takes precedence even if output directories are unset (Item 7)
    *app_state.images_output_dir.lock().unwrap() = None;
    *app_state.pdfs_output_dir.lock().unwrap() = None;

    let recorder = TestRecorder::new();
    let err_img_unset = start_images_to_pdfs_internal(
        &app_state,
        &[id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        PdfConformance::Standard,
        None,
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err_img_unset.code, ErrorCode::ConversionRunning);

    let recorder = TestRecorder::new();
    let err_pdf_unset = start_pdfs_to_images_internal(
        &app_state,
        &[id],
        "1",
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err_pdf_unset.code, ErrorCode::ConversionRunning);
}

#[test]
fn unset_output_dir_rejects_with_invalid_params() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();

    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let add_res = add_images(&app_state, &[path]);
    let id = add_res.added[0].id;

    // Both output dirs are None by default
    let recorder = TestRecorder::new();
    let err_img = start_images_to_pdfs_internal(
        &app_state,
        &[id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        PdfConformance::Standard,
        None,
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err_img.code, ErrorCode::InvalidParams);
    assert!(!app_state.is_running.load(Ordering::SeqCst));

    let recorder = TestRecorder::new();
    let err_pdf = start_pdfs_to_images_internal(
        &app_state,
        &[id],
        "1",
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err_pdf.code, ErrorCode::InvalidParams);
    assert!(!app_state.is_running.load(Ordering::SeqCst));
}

#[test]
fn unknown_handle_rejects() {
    let app_state = state();
    let temp_out = TempDir::new().unwrap();

    *app_state.images_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());
    *app_state.pdfs_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());

    let nonexistent_id = 999_999u64;

    let recorder = TestRecorder::new();
    let err = start_images_to_pdfs_internal(
        &app_state,
        &[nonexistent_id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        PdfConformance::Standard,
        None,
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownHandle);
    assert!(!app_state.is_running.load(Ordering::SeqCst));

    let recorder = TestRecorder::new();
    let err = start_pdfs_to_images_internal(
        &app_state,
        &[nonexistent_id],
        "1",
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownHandle);
    assert!(!app_state.is_running.load(Ordering::SeqCst));

    let recorder = TestRecorder::new();
    let err = run_images_to_pdfs(
        &app_state,
        &[nonexistent_id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        temp_out.path(),
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownHandle);

    let recorder = TestRecorder::new();
    let page_set = parse_page_range("1").unwrap();
    let err = run_pdfs_to_images(
        &app_state,
        &[nonexistent_id],
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        temp_out.path(),
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownHandle);

    let recorder = TestRecorder::new();
    let err = run_save_merged_pdf(
        &app_state,
        &[nonexistent_id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &temp_out.path().join("merged.pdf"),
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownHandle);
}

#[cfg(unix)]
#[test]
fn output_dir_that_cannot_be_listed_rejects_and_resets_is_running() {
    use std::os::unix::fs::PermissionsExt;

    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let add_res = add_images(&app_state, &[path]);
    let id = add_res.added[0].id;

    // A file can be created in the folder, so the pre-start check passes, but
    // the names in it cannot be listed (design §6.4).
    let unreadable_dir = temp_out.path().join("unreadable");
    fs::create_dir(&unreadable_dir).unwrap();
    fs::set_permissions(&unreadable_dir, fs::Permissions::from_mode(0o300)).unwrap();

    *app_state.images_output_dir.lock().unwrap() = Some(unreadable_dir.clone());
    *app_state.pdfs_output_dir.lock().unwrap() = Some(unreadable_dir.clone());

    let recorder = TestRecorder::new();
    let err_img = start_images_to_pdfs_internal(
        &app_state,
        &[id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        PdfConformance::Standard,
        None,
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err_img.code, ErrorCode::WriteFailed);
    assert!(!app_state.is_running.load(Ordering::SeqCst));

    let recorder = TestRecorder::new();
    let err_pdf = start_pdfs_to_images_internal(
        &app_state,
        &[id],
        "1",
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err_pdf.code, ErrorCode::WriteFailed);
    assert!(!app_state.is_running.load(Ordering::SeqCst));

    fs::set_permissions(&unreadable_dir, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn start_internal_ok_delivers_job_finished() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let img_path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let pdf_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let add_img = add_images(&app_state, &[img_path]);
    let add_pdf = add_pdfs(&app_state, &[pdf_path]);
    let img_id = add_img.added[0].id;
    let pdf_id = add_pdf.added[0].id;

    *app_state.images_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());
    *app_state.pdfs_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());

    let recorder = TestRecorder::new();
    start_images_to_pdfs_internal(
        &app_state,
        &[img_id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        PdfConformance::Standard,
        None,
        recorder.callbacks_recording_running(Arc::clone(&app_state.is_running)),
    )
    .expect("start_images_to_pdfs_internal should succeed");

    let finished = recorder.wait_finished(Duration::from_secs(5));
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 0);
    assert!(!finished.cancelled);
    assert_eq!(recorder.is_running_at_finished(), Some(false));
    assert!(!app_state.is_running.load(Ordering::SeqCst));

    let recorder = TestRecorder::new();
    start_pdfs_to_images_internal(
        &app_state,
        &[pdf_id],
        "1",
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        recorder.callbacks_recording_running(Arc::clone(&app_state.is_running)),
    )
    .expect("start_pdfs_to_images_internal should succeed");

    let finished = recorder.wait_finished(Duration::from_secs(5));
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 0);
    assert!(!finished.cancelled);
    assert_eq!(recorder.is_running_at_finished(), Some(false));
    assert!(!app_state.is_running.load(Ordering::SeqCst));
}

#[test]
fn starting_images_to_pdfs_passes_the_conformance_on() {
    for (conformance, expect_pdf_a) in [
        (PdfConformance::PdfA2b, true),
        (PdfConformance::Standard, false),
    ] {
        let app_state = state();
        let temp_in = TempDir::new().unwrap();
        let temp_out = TempDir::new().unwrap();
        let img_path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
        let id = add_images(&app_state, &[img_path]).added[0].id;
        *app_state.images_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());

        let recorder = TestRecorder::new();
        start_images_to_pdfs_internal(
            &app_state,
            &[id],
            PageSizeChoice::Fit,
            PageOrientationChoice::Auto,
            conformance,
            None,
            recorder.callbacks(),
        )
        .expect("start_images_to_pdfs_internal should succeed");

        let finished = recorder.wait_finished(Duration::from_secs(5));
        assert_eq!(finished.succeeded, 1);
        let pdf = fs::read(temp_out.path().join("photo.pdf")).expect("reading the PDF");
        assert_eq!(is_pdf_a(&pdf), expect_pdf_a, "{conformance:?}");
    }
}

#[test]
fn cancellation_during_pdf_to_images_saves_early_pages_and_cleans_temp() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let pdf_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let add_pdf = add_pdfs(&app_state, &[pdf_path]);
    let pdf_id = add_pdf.added[0].id;

    let page_set = parse_page_range("1-3").unwrap();

    let cancel_flag = Arc::clone(&app_state.cancel_flag);
    let recorder = TestRecorder::new();
    let default_cb = recorder.callbacks();
    let callbacks = JobCallbacks {
        on_progress: {
            let on_prog = default_cb.on_progress;
            move |prog: JobProgressPayload| {
                if prog.done == 1 {
                    cancel_flag.store(true, Ordering::SeqCst);
                }
                on_prog(prog);
            }
        },
        on_item: default_cb.on_item,
        on_finished: default_cb.on_finished,
    };

    run_pdfs_to_images(
        &app_state,
        &[pdf_id],
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        temp_out.path(),
        callbacks,
    )
    .expect("run_pdfs_to_images should complete");

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.status, JobItemStatus::Cancelled);
    assert!(!item.outputs.is_empty(), "at least one output was saved");
    assert!(
        item.outputs.len() < 3,
        "fewer outputs than 3 pages in range"
    );

    for output in &item.outputs {
        assert!(temp_out.path().join(output).exists());
    }

    let finished = recorder.finished();
    assert!(finished.cancelled);

    assert_no_temp_files(temp_out.path());
}

#[test]
fn cancellation_during_images_to_pdfs_leaves_unprocessed() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let mut paths = Vec::new();
    for i in 1..=6 {
        paths.push(copy_fixture(
            temp_in.path(),
            "photo.jpg",
            &format!("photo_{i}.jpg"),
        ));
    }
    let add_res = add_images(&app_state, &paths);
    assert_eq!(add_res.added.len(), 6);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    let cancel_flag = Arc::clone(&app_state.cancel_flag);
    let recorder = TestRecorder::new();
    let default_cb = recorder.callbacks();
    let item_counter = Arc::new(AtomicU32::new(0));
    let callbacks = JobCallbacks {
        on_progress: default_cb.on_progress,
        on_item: {
            let on_item = default_cb.on_item;
            let counter = Arc::clone(&item_counter);
            move |item: JobItemPayload| {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    cancel_flag.store(true, Ordering::SeqCst);
                }
                on_item(item);
            }
        },
        on_finished: default_cb.on_finished,
    };

    run_images_to_pdfs(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        temp_out.path(),
        callbacks,
    )
    .expect("run_images_to_pdfs should complete");

    let finished = recorder.finished();
    assert!(finished.cancelled);
    assert!(
        finished.unprocessed >= 1,
        "expected at least 1 unprocessed, got {}",
        finished.unprocessed
    );
    assert_eq!(
        finished.succeeded + finished.failed + finished.unprocessed,
        6
    );

    assert_no_temp_files(temp_out.path());
}

#[test]
fn cancellation_during_merged_pdf_on_item_writes_no_file() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path1 = copy_fixture(temp_in.path(), "photo.jpg", "img1.jpg");
    let path2 = copy_fixture(temp_in.path(), "logo_alpha.png", "img2.png");

    let add_res = add_images(&app_state, &[path1, path2]);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    let cancel_flag = Arc::clone(&app_state.cancel_flag);
    let recorder = TestRecorder::new();
    let default_cb = recorder.callbacks();
    let item_count = Arc::new(AtomicU32::new(0));
    let callbacks = JobCallbacks {
        on_progress: default_cb.on_progress,
        on_item: {
            let on_item = default_cb.on_item;
            let count = Arc::clone(&item_count);
            move |item: JobItemPayload| {
                on_item(item);
                if count.fetch_add(1, Ordering::SeqCst) + 1 == 2 {
                    cancel_flag.store(true, Ordering::SeqCst);
                }
            }
        },
        on_finished: default_cb.on_finished,
    };

    let dest_path = temp_out.path().join("merged.pdf");
    let res = run_save_merged_pdf(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest_path,
        callbacks,
    )
    .unwrap();

    assert_eq!(res.saved_name, None);
    assert!(!dest_path.exists());
    assert_no_temp_files(temp_out.path());

    let finished = recorder.finished();
    assert!(finished.cancelled);
    assert_eq!(finished.succeeded, 0);
}

#[test]
fn running_guard_drop_does_not_clear_subsequent_running_flag() {
    let flag = Arc::new(AtomicBool::new(true));
    let guard = RunningGuard::new(Arc::clone(&flag));

    let running_during_callback = Arc::new(AtomicBool::new(true));
    let r_clone = Arc::clone(&running_during_callback);
    let flag_clone = Arc::clone(&flag);
    let wrapped = guard.wrap_on_finished(move |_| {
        r_clone.store(flag_clone.load(Ordering::SeqCst), Ordering::SeqCst);
        // Simulate next conversion starting immediately and acquiring running flag
        flag_clone.store(true, Ordering::SeqCst);
    });

    wrapped(JobFinishedPayload {
        succeeded: 1,
        failed: 0,
        no_pages: 0,
        unprocessed: 0,
        cancelled: false,
    });

    assert!(
        !running_during_callback.load(Ordering::SeqCst),
        "flag must be false when on_finished is called"
    );
    assert!(
        flag.load(Ordering::SeqCst),
        "flag should be true after new conversion started"
    );

    // Old guard is dropped now
    drop(guard);

    // Old guard must not clear the new conversion's running flag
    assert!(
        flag.load(Ordering::SeqCst),
        "flag must remain true after old guard is dropped"
    );
}

#[test]
fn running_guard_resets_flag_on_drop_when_unfinished() {
    let flag = Arc::new(AtomicBool::new(true));
    let guard = RunningGuard::new(Arc::clone(&flag));
    assert!(flag.load(Ordering::SeqCst));
    drop(guard);
    assert!(!flag.load(Ordering::SeqCst));
}

#[test]
fn images_to_pdfs_each_skips_initial_error_and_does_not_claim_output_name() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    // b.bmp comes first in the list and has another stem: if error items
    // took resolved names, a.png would be saved as b.pdf.
    let other_bmp_path = temp_in.path().join("b.bmp");
    fs::write(&other_bmp_path, b"not an image").unwrap();
    let other_res = add_images(&app_state, &[other_bmp_path]);
    assert!(other_res.added[0].error.is_some());

    let bmp_path = temp_in.path().join("a.bmp");
    fs::write(&bmp_path, b"not an image").unwrap();
    let png_path = copy_fixture(temp_in.path(), "logo_alpha.png", "a.png");

    let add_res = add_images(&app_state, &[bmp_path.clone(), png_path]);
    assert_eq!(add_res.added.len(), 2);
    assert_eq!(add_res.added[0].name, "a.bmp");
    let bmp_err = add_res.added[0]
        .error
        .clone()
        .expect("a.bmp must have error upon addition");
    assert_eq!(add_res.added[1].name, "a.png");
    assert!(add_res.added[1].error.is_none());

    // Overwrite a.bmp with valid image bytes after adding to item table
    // to verify that run_images_to_pdfs does not re-read files that failed probe.
    fs::write(&bmp_path, fs::read(fixture("logo_alpha.png")).unwrap()).unwrap();

    let ids: Vec<u64> = std::iter::once(other_res.added[0].id)
        .chain(add_res.added.iter().map(|item| item.id))
        .collect();

    let recorder = TestRecorder::new();
    run_images_to_pdfs(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("run_images_to_pdfs should succeed");

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 2);
    assert_eq!(finished.unprocessed, 0);
    assert!(!finished.cancelled);

    let items = recorder.items();
    assert_eq!(items.len(), 3);

    let bmp_item = items.iter().find(|i| i.id == ids[1]).unwrap();
    assert_eq!(bmp_item.status, JobItemStatus::Failed);
    assert_eq!(bmp_item.error.as_ref().map(|e| e.code), Some(bmp_err.code));
    assert!(bmp_item.outputs.is_empty());

    let png_item = items.iter().find(|i| i.id == ids[2]).unwrap();
    assert_eq!(png_item.status, JobItemStatus::Ok);
    assert_eq!(png_item.outputs, vec!["a.pdf".to_string()]);
    assert!(temp_out.path().join("a.pdf").exists());

    assert_no_temp_files(temp_out.path());
}

#[test]
fn merged_pdf_skips_initial_error_without_rereading_file() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let bmp_path = temp_in.path().join("a.bmp");
    fs::write(&bmp_path, b"not an image").unwrap();

    let add_res = add_images(&app_state, std::slice::from_ref(&bmp_path));
    assert_eq!(add_res.added.len(), 1);
    let bmp_err = add_res.added[0]
        .error
        .clone()
        .expect("a.bmp must have error upon addition");
    let id = add_res.added[0].id;

    // Overwrite a.bmp with valid image bytes after adding to item table
    fs::write(&bmp_path, fs::read(fixture("logo_alpha.png")).unwrap()).unwrap();

    let dest_path = temp_out.path().join("merged.pdf");
    let recorder = TestRecorder::new();
    let res = run_save_merged_pdf(
        &app_state,
        &[id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest_path,
        recorder.callbacks(),
    )
    .expect("run_save_merged_pdf should succeed");

    assert_eq!(res.saved_name, None);
    assert!(!dest_path.exists());

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 0);
    assert_eq!(finished.failed, 1);
    assert!(!finished.cancelled);

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, JobItemStatus::Failed);
    assert_eq!(items[0].error.as_ref().map(|e| e.code), Some(bmp_err.code));

    assert_no_temp_files(temp_out.path());
}

#[test]
fn a_merged_pdf_that_was_written_reports_its_name() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path1 = copy_fixture(temp_in.path(), "photo.jpg", "img1.jpg");
    let path2 = copy_fixture(temp_in.path(), "logo_alpha.png", "img2.png");
    let ids: Vec<u64> = add_images(&app_state, &[path1, path2])
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    let dest_path = temp_out.path().join("merged.pdf");
    let recorder = TestRecorder::new();
    let res = run_save_merged_pdf(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest_path,
        recorder.callbacks(),
    )
    .expect("run_save_merged_pdf should succeed");

    assert_eq!(res.saved_name, Some("merged.pdf".to_string()));
    assert!(dest_path.exists());

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 2);
    assert!(!finished.cancelled);
}

/// Whether `pdf` identifies itself as PDF/A in its XMP metadata.
fn is_pdf_a(pdf: &[u8]) -> bool {
    const MARKER: &[u8] = b"<pdfaid:part>2</pdfaid:part>";
    pdf.windows(MARKER.len()).any(|window| window == MARKER)
}

#[test]
fn the_pdf_a_choice_selects_the_conformance() {
    assert_eq!(pdf_conformance(true), PdfConformance::PdfA2b);
    assert_eq!(pdf_conformance(false), PdfConformance::Standard);
}

#[test]
fn a_merged_pdf_is_pdf_a_only_when_asked() {
    for (conformance, expect_pdf_a) in [
        (PdfConformance::PdfA2b, true),
        (PdfConformance::Standard, false),
    ] {
        let app_state = state();
        let temp_in = TempDir::new().unwrap();
        let temp_out = TempDir::new().unwrap();
        let path1 = copy_fixture(temp_in.path(), "photo.jpg", "img1.jpg");
        let path2 = copy_fixture(temp_in.path(), "logo_alpha.png", "img2.png");
        let ids: Vec<u64> = add_images(&app_state, &[path1, path2])
            .added
            .iter()
            .map(|item| item.id)
            .collect();

        let dest_path = temp_out.path().join("merged.pdf");
        let recorder = TestRecorder::new();
        run_save_merged_pdf(
            &app_state,
            &ids,
            PageSizeChoice::Fit,
            PageOrientationChoice::Auto,
            &PdfOutput::new(conformance, None),
            &dest_path,
            recorder.callbacks(),
        )
        .expect("run_save_merged_pdf should succeed");

        assert_eq!(recorder.finished().succeeded, 2);
        let pdf = fs::read(&dest_path).expect("reading the merged PDF");
        assert_eq!(is_pdf_a(&pdf), expect_pdf_a, "{conformance:?}");
    }
}

#[test]
fn each_pdf_is_pdf_a_only_when_asked() {
    for (conformance, expect_pdf_a) in [
        (PdfConformance::PdfA2b, true),
        (PdfConformance::Standard, false),
    ] {
        let app_state = state();
        let temp_in = TempDir::new().unwrap();
        let temp_out = TempDir::new().unwrap();
        let path1 = copy_fixture(temp_in.path(), "photo.jpg", "img1.jpg");
        let path2 = copy_fixture(temp_in.path(), "logo_alpha.png", "img2.png");
        let ids: Vec<u64> = add_images(&app_state, &[path1, path2])
            .added
            .iter()
            .map(|item| item.id)
            .collect();

        let recorder = TestRecorder::new();
        run_images_to_pdfs(
            &app_state,
            &ids,
            PageSizeChoice::Fit,
            PageOrientationChoice::Auto,
            &PdfOutput::new(conformance, None),
            temp_out.path(),
            recorder.callbacks(),
        )
        .expect("run_images_to_pdfs should succeed");

        let items = recorder.items();
        assert_eq!(items.len(), 2);
        for item in items {
            assert_eq!(item.status, JobItemStatus::Ok);
            let pdf = fs::read(temp_out.path().join(&item.outputs[0])).expect("reading a PDF");
            assert_eq!(is_pdf_a(&pdf), expect_pdf_a, "{conformance:?}");
        }
    }
}

#[test]
fn a_cmyk_image_fails_alone_in_pdf_a_and_the_merged_pdf_has_the_rest() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let rgb1 = copy_fixture(temp_in.path(), "photo.jpg", "a.jpg");
    let cmyk = copy_fixture(temp_in.path(), "photo_cmyk.jpg", "b_cmyk.jpg");
    let rgb2 = copy_fixture(temp_in.path(), "logo_alpha.png", "c.png");
    let ids: Vec<u64> = add_images(&app_state, &[rgb1, cmyk, rgb2])
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    let dest_path = temp_out.path().join("merged.pdf");
    let recorder = TestRecorder::new();
    let res = run_save_merged_pdf(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::PdfA2b, None),
        &dest_path,
        recorder.callbacks(),
    )
    .expect("run_save_merged_pdf should succeed");

    assert_eq!(res.saved_name, Some("merged.pdf".to_string()));
    let finished = recorder.finished();
    assert_eq!((finished.succeeded, finished.failed), (2, 1));
    let items = recorder.items();
    let failed: Vec<_> = items
        .iter()
        .filter(|item| item.status == JobItemStatus::Failed)
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].id, ids[1]);
    assert_eq!(
        failed[0].error.as_ref().map(|e| e.code),
        Some(ErrorCode::CmykNotAllowedInPdfA)
    );
    let pdf = fs::read(&dest_path).expect("reading the merged PDF");
    assert!(is_pdf_a(&pdf));
    assert!(pdf.windows(8).any(|w| w == b"/Count 2"));
}

#[test]
fn a_cmyk_image_fails_alone_in_each_mode_in_pdf_a_and_not_otherwise() {
    for (conformance, expected_code) in [
        (
            PdfConformance::PdfA2b,
            Some(ErrorCode::CmykNotAllowedInPdfA),
        ),
        (PdfConformance::Standard, None),
    ] {
        let app_state = state();
        let temp_in = TempDir::new().unwrap();
        let temp_out = TempDir::new().unwrap();
        let rgb = copy_fixture(temp_in.path(), "photo.jpg", "a.jpg");
        let cmyk = copy_fixture(temp_in.path(), "photo_cmyk.jpg", "b_cmyk.jpg");
        let ids: Vec<u64> = add_images(&app_state, &[rgb, cmyk])
            .added
            .iter()
            .map(|item| item.id)
            .collect();

        let recorder = TestRecorder::new();
        run_images_to_pdfs(
            &app_state,
            &ids,
            PageSizeChoice::Fit,
            PageOrientationChoice::Auto,
            &PdfOutput::new(conformance, None),
            temp_out.path(),
            recorder.callbacks(),
        )
        .expect("run_images_to_pdfs should succeed");

        let items = recorder.items();
        let rgb_item = items.iter().find(|item| item.id == ids[0]).unwrap();
        let cmyk_item = items.iter().find(|item| item.id == ids[1]).unwrap();
        assert_eq!(rgb_item.status, JobItemStatus::Ok);
        assert_eq!(cmyk_item.error.as_ref().map(|e| e.code), expected_code);
        assert_eq!(
            cmyk_item.status,
            if expected_code.is_some() {
                JobItemStatus::Failed
            } else {
                JobItemStatus::Ok
            }
        );
        assert_no_temp_files(temp_out.path());
    }
}

/// The permission bits of `path`.
#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .expect("reading metadata")
        .permissions()
        .mode()
        & 0o777
}

/// What `0666` becomes under this process's umask, found by creating an
/// ordinary file next to the outputs rather than assuming a umask.
#[cfg(unix)]
fn mode_of_a_new_file_in(dir: &Path) -> u32 {
    let probe = dir.join("umask-probe");
    fs::File::create(&probe).expect("creating the probe file");
    let mode = mode_of(&probe);
    fs::remove_file(&probe).expect("removing the probe file");
    mode
}

#[cfg(unix)]
#[test]
fn saved_files_get_the_permissions_of_a_new_file() {
    let temp_out = TempDir::new().unwrap();
    let expected = mode_of_a_new_file_in(temp_out.path());

    let used = Mutex::new(HashSet::new());
    let name = save_atomic(temp_out.path(), "out.bin", b"data", &used).unwrap();

    assert_eq!(mode_of(&temp_out.path().join(name)), expected);
}

#[cfg(unix)]
#[test]
fn each_mode_pdfs_get_the_permissions_of_a_new_file() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let expected = mode_of_a_new_file_in(temp_out.path());

    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let ids: Vec<u64> = add_images(&app_state, &[path])
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    let recorder = TestRecorder::new();
    run_images_to_pdfs(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        temp_out.path(),
        recorder.callbacks(),
    )
    .unwrap();

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, JobItemStatus::Ok);
    assert_eq!(
        mode_of(&temp_out.path().join(&items[0].outputs[0])),
        expected
    );
}

#[cfg(unix)]
#[test]
fn the_merged_pdf_gets_the_permissions_of_a_new_file() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let expected = mode_of_a_new_file_in(temp_out.path());

    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let ids: Vec<u64> = add_images(&app_state, &[path])
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    let dest_path = temp_out.path().join("merged.pdf");
    let recorder = TestRecorder::new();
    run_save_merged_pdf(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest_path,
        recorder.callbacks(),
    )
    .unwrap();

    assert_eq!(mode_of(&dest_path), expected);
}

#[cfg(unix)]
#[test]
fn page_images_get_the_permissions_of_a_new_file() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let expected = mode_of_a_new_file_in(temp_out.path());

    let path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let ids: Vec<u64> = add_pdfs(&app_state, &[path])
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    let recorder = TestRecorder::new();
    run_pdfs_to_images(
        &app_state,
        &ids,
        &parse_page_range("1").unwrap(),
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        72,
        temp_out.path(),
        recorder.callbacks(),
    )
    .unwrap();

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, JobItemStatus::Ok);
    assert_eq!(
        mode_of(&temp_out.path().join(&items[0].outputs[0])),
        expected
    );
}

/// A state whose settings are kept in `config_dir`, with `dir` chosen as the
/// output folder of both conversions, as `pick_output_dir` would leave it.
fn state_with_chosen_output_dir(config_dir: &Path, dir: &Path) -> AppState {
    let app_state = state();
    restore_settings(&app_state, config_dir);
    for kind in [OutputKind::ImagesToPdf, OutputKind::PdfToImages] {
        let (_, persisted) = apply_picked_dir(&app_state, kind, dir.to_path_buf());
        persisted.expect("writing the settings");
    }
    app_state
}

/// Asserts that both output folders are "not chosen" in memory, in what
/// `get_settings` returns, and in the settings file.
fn assert_output_dirs_not_chosen(app_state: &AppState, config_dir: &Path) {
    assert!(app_state.images_output_dir.lock().unwrap().is_none());
    assert!(app_state.pdfs_output_dir.lock().unwrap().is_none());
    let settings = get_settings_internal(app_state);
    assert!(settings.images_to_pdf.output_dir.is_none());
    assert!(settings.pdf_to_images.output_dir.is_none());
    let file = load_settings(config_dir);
    assert!(file.images_to_pdf.output_dir.is_none());
    assert!(file.pdf_to_images.output_dir.is_none());
}

/// Starts both conversions and returns their errors.
fn start_both(app_state: &AppState, image_id: u64, pdf_id: u64) -> (ErrorCode, ErrorCode) {
    let recorder = TestRecorder::new();
    let images = start_images_to_pdfs_internal(
        app_state,
        &[image_id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        PdfConformance::Standard,
        None,
        recorder.callbacks(),
    )
    .unwrap_err();
    let recorder = TestRecorder::new();
    let pdfs = start_pdfs_to_images_internal(
        app_state,
        &[pdf_id],
        "1",
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        recorder.callbacks(),
    )
    .unwrap_err();
    (images.code, pdfs.code)
}

fn add_one_image_and_one_pdf(app_state: &AppState, dir: &Path) -> (u64, u64) {
    let image = copy_fixture(dir, "photo.jpg", "photo.jpg");
    let pdf = copy_fixture(dir, "shapes.pdf", "shapes.pdf");
    (
        add_images(app_state, &[image]).added[0].id,
        add_pdfs(app_state, &[pdf]).added[0].id,
    )
}

#[test]
fn a_missing_output_dir_is_refused_and_forgotten() {
    let temp_in = TempDir::new().unwrap();
    let temp_config = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let out_dir = temp_out.path().join("gone");
    fs::create_dir(&out_dir).unwrap();

    let app_state = state_with_chosen_output_dir(temp_config.path(), &out_dir);
    let (image_id, pdf_id) = add_one_image_and_one_pdf(&app_state, temp_in.path());
    fs::remove_dir(&out_dir).unwrap();

    let (images, pdfs) = start_both(&app_state, image_id, pdf_id);

    assert_eq!(images, ErrorCode::OutputDirMissing);
    assert_eq!(pdfs, ErrorCode::OutputDirMissing);
    assert!(!app_state.is_running.load(Ordering::SeqCst));
    assert_output_dirs_not_chosen(&app_state, temp_config.path());
}

#[test]
fn an_output_dir_that_is_now_a_file_is_refused_and_forgotten() {
    let temp_in = TempDir::new().unwrap();
    let temp_config = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let out_dir = temp_out.path().join("was-a-folder");
    fs::create_dir(&out_dir).unwrap();

    let app_state = state_with_chosen_output_dir(temp_config.path(), &out_dir);
    let (image_id, pdf_id) = add_one_image_and_one_pdf(&app_state, temp_in.path());
    fs::remove_dir(&out_dir).unwrap();
    fs::write(&out_dir, b"not a folder").unwrap();

    let (images, pdfs) = start_both(&app_state, image_id, pdf_id);

    assert_eq!(images, ErrorCode::OutputDirMissing);
    assert_eq!(pdfs, ErrorCode::OutputDirMissing);
    assert_output_dirs_not_chosen(&app_state, temp_config.path());
}

#[test]
fn only_the_missing_output_dir_is_forgotten() {
    let temp_in = TempDir::new().unwrap();
    let temp_config = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let gone = temp_out.path().join("gone");
    let kept = temp_out.path().join("kept");
    fs::create_dir(&gone).unwrap();
    fs::create_dir(&kept).unwrap();

    let app_state = state_with_chosen_output_dir(temp_config.path(), &kept);
    apply_picked_dir(&app_state, OutputKind::ImagesToPdf, gone.clone())
        .1
        .unwrap();
    let (image_id, _) = add_one_image_and_one_pdf(&app_state, temp_in.path());
    fs::remove_dir(&gone).unwrap();

    let recorder = TestRecorder::new();
    let err = start_images_to_pdfs_internal(
        &app_state,
        &[image_id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        PdfConformance::Standard,
        None,
        recorder.callbacks(),
    )
    .unwrap_err();

    assert_eq!(err.code, ErrorCode::OutputDirMissing);
    assert!(app_state.images_output_dir.lock().unwrap().is_none());
    assert_eq!(
        app_state.pdfs_output_dir.lock().unwrap().as_deref(),
        Some(kept.as_path())
    );
    let file = load_settings(temp_config.path());
    assert!(file.images_to_pdf.output_dir.is_none());
    assert_eq!(
        file.pdf_to_images.output_dir.as_deref(),
        Some(kept.as_path())
    );
}

#[cfg(unix)]
#[test]
fn an_output_dir_that_cannot_be_written_is_refused_and_kept() {
    use std::os::unix::fs::PermissionsExt;

    let temp_in = TempDir::new().unwrap();
    let temp_config = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let out_dir = temp_out.path().join("read-only");
    fs::create_dir(&out_dir).unwrap();

    let app_state = state_with_chosen_output_dir(temp_config.path(), &out_dir);
    let (image_id, pdf_id) = add_one_image_and_one_pdf(&app_state, temp_in.path());
    fs::set_permissions(&out_dir, fs::Permissions::from_mode(0o555)).unwrap();

    // A process that may write anyway (root) cannot show the refusal.
    if fs::File::create(out_dir.join("probe")).is_ok() {
        fs::set_permissions(&out_dir, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipped: this process can write to a 0555 folder");
        return;
    }

    let (images, pdfs) = start_both(&app_state, image_id, pdf_id);
    fs::set_permissions(&out_dir, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(images, ErrorCode::OutputDirNotWritable);
    assert_eq!(pdfs, ErrorCode::OutputDirNotWritable);
    assert!(!app_state.is_running.load(Ordering::SeqCst));
    assert!(list_existing_files(&out_dir).unwrap().is_empty());
    assert_eq!(
        app_state.images_output_dir.lock().unwrap().as_deref(),
        Some(out_dir.as_path())
    );
    assert_eq!(
        app_state.pdfs_output_dir.lock().unwrap().as_deref(),
        Some(out_dir.as_path())
    );
    let file = load_settings(temp_config.path());
    assert_eq!(
        file.images_to_pdf.output_dir.as_deref(),
        Some(out_dir.as_path())
    );
    assert_eq!(
        file.pdf_to_images.output_dir.as_deref(),
        Some(out_dir.as_path())
    );
}

/// A folder that is not there: creating the first output in it fails with
/// `WriteFailed`, which is what a conversion meets when its folder goes away
/// or fills up after it has started. `run_*` do not check the folder first.
fn missing_dir(parent: &Path) -> PathBuf {
    parent.join("never-created")
}

#[test]
fn a_write_failure_stops_images_to_pdfs_and_leaves_the_rest_unprocessed() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let total = 12;
    let paths: Vec<PathBuf> = (0..total)
        .map(|n| copy_fixture(temp_in.path(), "photo.jpg", &format!("photo{n}.jpg")))
        .collect();
    let ids: Vec<u64> = add_images(&app_state, &paths)
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    let recorder = TestRecorder::new();
    run_images_to_pdfs(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &missing_dir(temp_out.path()),
        recorder.callbacks(),
    )
    .unwrap();

    let finished = recorder.finished();
    let items = recorder.items();
    // Each thread fails once and then sees the flag, so no more items than
    // threads are tried.
    assert!(!items.is_empty());
    assert!(items.len() <= default_worker_limit());
    for item in &items {
        assert_eq!(item.status, JobItemStatus::Failed);
        assert_eq!(
            item.error.as_ref().map(|e| e.code),
            Some(ErrorCode::WriteFailed)
        );
    }
    assert_eq!(finished.succeeded, 0);
    assert_eq!(finished.failed as usize, items.len());
    assert_eq!(finished.unprocessed as usize, total - items.len());
    assert!(!finished.cancelled);
}

#[test]
fn a_write_failure_stops_pdfs_to_images_and_leaves_the_rest_unprocessed() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let total = 8;
    let paths: Vec<PathBuf> = (0..total)
        .map(|n| copy_fixture(temp_in.path(), "shapes.pdf", &format!("shapes{n}.pdf")))
        .collect();
    let ids: Vec<u64> = add_pdfs(&app_state, &paths)
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    let recorder = TestRecorder::new();
    run_pdfs_to_images(
        &app_state,
        &ids,
        &parse_page_range("1-3").unwrap(),
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        72,
        &missing_dir(temp_out.path()),
        recorder.callbacks(),
    )
    .unwrap();

    let finished = recorder.finished();
    let items = recorder.items();
    assert!(!items.is_empty());
    assert!(items.len() <= default_worker_limit());
    for item in &items {
        // Page 1 failed to save; pages 2 and 3 were not tried.
        assert_eq!(item.status, JobItemStatus::Failed);
        assert!(item.outputs.is_empty());
        assert_eq!(item.failed_pages, Some(vec![1]));
        assert_eq!(
            item.error.as_ref().map(|e| e.code),
            Some(ErrorCode::WriteFailed)
        );
    }
    assert_eq!(finished.succeeded, 0);
    assert_eq!(finished.failed as usize, items.len());
    assert_eq!(finished.unprocessed as usize, total - items.len());
    assert!(!finished.cancelled);
}

#[test]
fn a_write_failure_after_some_pages_makes_the_pdf_partial() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let out_dir = temp_out.path().join("vanishing");
    fs::create_dir(&out_dir).unwrap();

    let path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let ids: Vec<u64> = add_pdfs(&app_state, &[path])
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    // Remove the folder once page 1 is saved, so page 2 cannot be written.
    let recorder = TestRecorder::new();
    let items = Arc::clone(&recorder.items);
    let finished = Arc::clone(&recorder.finished);
    let vanishing = out_dir.clone();
    let callbacks = JobCallbacks {
        on_progress: move |progress: JobProgressPayload| {
            if progress.done == 1 {
                let _ = fs::remove_dir_all(&vanishing);
            }
        },
        on_item: move |item| items.lock().unwrap().push(item),
        on_finished: move |fin| *finished.lock().unwrap() = Some(fin),
    };
    run_pdfs_to_images(
        &app_state,
        &ids,
        &parse_page_range("1-3").unwrap(),
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        72,
        &out_dir,
        callbacks,
    )
    .unwrap();

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, JobItemStatus::Partial);
    assert_eq!(items[0].outputs.len(), 1);
    assert_eq!(items[0].failed_pages, Some(vec![2]));
    assert_eq!(
        items[0].error.as_ref().map(|e| e.code),
        Some(ErrorCode::WriteFailed)
    );
}

#[test]
fn images_that_cannot_be_decoded_do_not_stop_the_rest() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    // More images than threads, the broken ones first, so that a conversion
    // that stopped at the first failure would leave valid images unprocessed.
    let broken = 6;
    let valid = 6;
    let mut paths: Vec<PathBuf> = (0..broken)
        .map(|n| copy_fixture(temp_in.path(), "corrupt.png", &format!("broken{n}.png")))
        .collect();
    paths.extend(
        (0..valid).map(|n| copy_fixture(temp_in.path(), "photo.jpg", &format!("photo{n}.jpg"))),
    );
    let added = add_images(&app_state, &paths).added;
    assert!(added.iter().all(|item| item.error.is_none()));
    let ids: Vec<u64> = added.iter().map(|item| item.id).collect();

    let recorder = TestRecorder::new();
    run_images_to_pdfs(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        temp_out.path(),
        recorder.callbacks(),
    )
    .unwrap();

    let finished = recorder.finished();
    assert_eq!(finished.succeeded as usize, valid);
    assert_eq!(finished.failed as usize, broken);
    assert_eq!(finished.unprocessed, 0);
}

#[test]
fn pages_that_cannot_be_rendered_do_not_stop_the_other_pdfs() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let total = 8;
    let paths: Vec<PathBuf> = (0..total)
        .map(|n| copy_fixture(temp_in.path(), "mixed_sizes.pdf", &format!("mixed{n}.pdf")))
        .collect();
    let ids: Vec<u64> = add_pdfs(&app_state, &paths)
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    // Page 2 of each is too large at 300 dpi; pages 1 and 3 are saved.
    let recorder = TestRecorder::new();
    run_pdfs_to_images(
        &app_state,
        &ids,
        &parse_page_range("1-3").unwrap(),
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        300,
        temp_out.path(),
        recorder.callbacks(),
    )
    .unwrap();

    let items = recorder.items();
    assert_eq!(items.len(), total);
    for item in &items {
        assert_eq!(item.status, JobItemStatus::Partial);
        assert_eq!(item.outputs.len(), 2);
        assert_eq!(item.failed_pages, Some(vec![2]));
    }
    assert_eq!(recorder.finished().unprocessed, 0);
}

#[test]
fn a_cancel_after_a_failed_page_still_reports_that_page() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path = copy_fixture(temp_in.path(), "mixed_sizes.pdf", "mixed.pdf");
    let ids: Vec<u64> = add_pdfs(&app_state, &[path])
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    // At 300 dpi page 2 is too large. Cancel once it has been tried, so
    // page 3 is never reached: page 1 saved, page 2 failed, page 3 left.
    let cancel_flag = Arc::clone(&app_state.cancel_flag);
    let recorder = TestRecorder::new();
    let default_cb = recorder.callbacks();
    let callbacks = JobCallbacks {
        on_progress: {
            let on_prog = default_cb.on_progress;
            move |prog: JobProgressPayload| {
                if prog.done == 2 {
                    cancel_flag.store(true, Ordering::SeqCst);
                }
                on_prog(prog);
            }
        },
        on_item: default_cb.on_item,
        on_finished: default_cb.on_finished,
    };
    run_pdfs_to_images(
        &app_state,
        &ids,
        &parse_page_range("1-3").unwrap(),
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        300,
        temp_out.path(),
        callbacks,
    )
    .unwrap();

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, JobItemStatus::Cancelled);
    assert_eq!(items[0].outputs.len(), 1);
    assert_eq!(items[0].failed_pages, Some(vec![2]));
}

#[test]
fn a_cancel_without_a_failed_page_sends_no_failed_pages() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let ids: Vec<u64> = add_pdfs(&app_state, &[path])
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    let cancel_flag = Arc::clone(&app_state.cancel_flag);
    let recorder = TestRecorder::new();
    let default_cb = recorder.callbacks();
    let callbacks = JobCallbacks {
        on_progress: {
            let on_prog = default_cb.on_progress;
            move |prog: JobProgressPayload| {
                if prog.done == 1 {
                    cancel_flag.store(true, Ordering::SeqCst);
                }
                on_prog(prog);
            }
        },
        on_item: default_cb.on_item,
        on_finished: default_cb.on_finished,
    };
    run_pdfs_to_images(
        &app_state,
        &ids,
        &parse_page_range("1-3").unwrap(),
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        72,
        temp_out.path(),
        callbacks,
    )
    .unwrap();

    let items = recorder.items();
    assert_eq!(items[0].status, JobItemStatus::Cancelled);
    assert_eq!(items[0].failed_pages, None);
}

#[test]
fn a4_portrait_produces_portrait_pdf_from_landscape_image() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let add_img = add_images(&app_state, &[path]);
    let id = add_img.added[0].id;

    let dest = temp_out.path().join("portrait.pdf");
    let recorder = TestRecorder::new();
    run_save_merged_pdf(
        &app_state,
        &[id],
        PageSizeChoice::A4,
        PageOrientationChoice::Portrait,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest,
        recorder.callbacks(),
    )
    .expect("run_save_merged_pdf should succeed");

    let add_pdf = add_pdfs(&app_state, &[dest]);
    assert_eq!(add_pdf.added.len(), 1);
    let size = add_pdf.added[0]
        .first_page_size_pt
        .expect("page size should be present");
    assert!(
        size.height_pt > size.width_pt,
        "expected portrait: height ({}) > width ({})",
        size.height_pt,
        size.width_pt
    );
    assert!((size.width_pt - 595.28).abs() < 1.0);
    assert!((size.height_pt - 841.89).abs() < 1.0);
}

#[test]
fn a4_landscape_produces_landscape_pdf_from_portrait_image() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path = copy_fixture(temp_in.path(), "shapes_150dpi_p1.png", "shapes.png");
    let add_img = add_images(&app_state, &[path]);
    let id = add_img.added[0].id;

    *app_state.images_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());

    let recorder = TestRecorder::new();
    start_images_to_pdfs_internal(
        &app_state,
        &[id],
        PageSizeChoice::A4,
        PageOrientationChoice::Landscape,
        PdfConformance::Standard,
        None,
        recorder.callbacks(),
    )
    .expect("start_images_to_pdfs_internal should succeed");

    let finished = recorder.wait_finished(Duration::from_secs(5));
    assert_eq!(finished.succeeded, 1);
    let out_path = temp_out.path().join("shapes.pdf");
    assert!(out_path.exists());

    let add_pdf = add_pdfs(&app_state, &[out_path]);
    assert_eq!(add_pdf.added.len(), 1);
    let size = add_pdf.added[0]
        .first_page_size_pt
        .expect("page size should be present");
    assert!(
        size.width_pt > size.height_pt,
        "expected landscape: width ({}) > height ({})",
        size.width_pt,
        size.height_pt
    );
    assert!((size.width_pt - 841.89).abs() < 1.0);
    assert!((size.height_pt - 595.28).abs() < 1.0);
}

#[test]
fn fit_page_size_ignores_orientation() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path1 = copy_fixture(temp_in.path(), "photo.jpg", "photo1.jpg");
    let path2 = copy_fixture(temp_in.path(), "photo.jpg", "photo2.jpg");
    let add_img = add_images(&app_state, &[path1, path2]);
    let id1 = add_img.added[0].id;
    let id2 = add_img.added[1].id;

    let dest_portrait = temp_out.path().join("fit_portrait.pdf");
    let recorder = TestRecorder::new();
    run_save_merged_pdf(
        &app_state,
        &[id1],
        PageSizeChoice::Fit,
        PageOrientationChoice::Portrait,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest_portrait,
        recorder.callbacks(),
    )
    .expect("run_save_merged_pdf should succeed");

    let dest_landscape = temp_out.path().join("fit_landscape.pdf");
    let recorder = TestRecorder::new();
    run_save_merged_pdf(
        &app_state,
        &[id2],
        PageSizeChoice::Fit,
        PageOrientationChoice::Landscape,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest_landscape,
        recorder.callbacks(),
    )
    .expect("run_save_merged_pdf should succeed");

    let add_pdf = add_pdfs(&app_state, &[dest_portrait, dest_landscape]);
    assert_eq!(add_pdf.added.len(), 2);
    let size_p = add_pdf.added[0].first_page_size_pt.unwrap();
    let size_l = add_pdf.added[1].first_page_size_pt.unwrap();
    assert_eq!(size_p, size_l);
}

#[test]
fn jis_b5_landscape_produces_expected_dimensions_in_each_mode() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    // Portrait fixture: shapes_150dpi_p1.png
    let path = copy_fixture(temp_in.path(), "shapes_150dpi_p1.png", "shapes.png");
    let add_img = add_images(&app_state, &[path]);
    let id = add_img.added[0].id;

    *app_state.images_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());

    let recorder = TestRecorder::new();
    start_images_to_pdfs_internal(
        &app_state,
        &[id],
        PageSizeChoice::JisB5,
        PageOrientationChoice::Landscape,
        PdfConformance::Standard,
        None,
        recorder.callbacks(),
    )
    .expect("start_images_to_pdfs_internal should succeed");

    let finished = recorder.wait_finished(Duration::from_secs(5));
    assert_eq!(finished.succeeded, 1);
    let out_path = temp_out.path().join("shapes.pdf");
    assert!(out_path.exists());

    let add_pdf = add_pdfs(&app_state, &[out_path]);
    assert_eq!(add_pdf.added.len(), 1);
    let size = add_pdf.added[0]
        .first_page_size_pt
        .expect("page size should be present");
    // JIS B5 landscape: width = 728.50, height = 515.91
    assert!(
        (size.width_pt - 728.50).abs() <= 0.01,
        "width mismatch: got {}, exp 728.50",
        size.width_pt
    );
    assert!(
        (size.height_pt - 515.91).abs() <= 0.01,
        "height mismatch: got {}, exp 515.91",
        size.height_pt
    );
}

#[test]
fn letter_auto_produces_expected_dimensions_in_merged_mode() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    // Landscape fixture: photo.jpg -> auto orientation selects landscape (792 x 612 pt)
    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let add_img = add_images(&app_state, &[path]);
    let id = add_img.added[0].id;

    let dest = temp_out.path().join("merged_letter.pdf");
    let recorder = TestRecorder::new();
    run_save_merged_pdf(
        &app_state,
        &[id],
        PageSizeChoice::Letter,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest,
        recorder.callbacks(),
    )
    .expect("run_save_merged_pdf should succeed");

    let add_pdf = add_pdfs(&app_state, &[dest]);
    assert_eq!(add_pdf.added.len(), 1);
    let size = add_pdf.added[0]
        .first_page_size_pt
        .expect("page size should be present");
    assert!(
        (size.width_pt - 792.0).abs() <= 0.01,
        "width mismatch: got {}, exp 792.0",
        size.width_pt
    );
    assert!(
        (size.height_pt - 612.0).abs() <= 0.01,
        "height mismatch: got {}, exp 612.0",
        size.height_pt
    );
}

#[test]
fn page_size_choice_serde_roundtrip_all_eight_values() {
    let cases = [
        (PageSizeChoice::Fit, "\"fit\""),
        (PageSizeChoice::A3, "\"a3\""),
        (PageSizeChoice::A4, "\"a4\""),
        (PageSizeChoice::A5, "\"a5\""),
        (PageSizeChoice::JisB4, "\"jisB4\""),
        (PageSizeChoice::JisB5, "\"jisB5\""),
        (PageSizeChoice::Letter, "\"letter\""),
        (PageSizeChoice::Legal, "\"legal\""),
    ];

    for (choice, expected_json) in cases {
        let serialized = serde_json::to_string(&choice).expect("serializing PageSizeChoice");
        assert_eq!(serialized, expected_json);
        let deserialized: PageSizeChoice =
            serde_json::from_str(&serialized).expect("deserializing PageSizeChoice");
        assert_eq!(deserialized, choice);
    }
}

#[test]
fn pdfs_to_images_webp_extension_and_content() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let pdf_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let add_pdf = add_pdfs(&app_state, &[pdf_path]);
    let pdf_id = add_pdf.added[0].id;

    let page_set = parse_page_range("1").unwrap();
    let recorder = TestRecorder::new();

    run_pdfs_to_images(
        &app_state,
        &[pdf_id],
        &page_set,
        RenderFormatChoice::Webp,
        RenderColorChoice::Color,
        150,
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("run_pdfs_to_images should succeed");

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, JobItemStatus::Ok);
    assert_eq!(items[0].outputs.len(), 1);
    let output_name = &items[0].outputs[0];
    assert!(
        output_name.ends_with(".webp"),
        "extension must be .webp: {output_name}"
    );

    let output_path = temp_out.path().join(output_name);
    let bytes = fs::read(&output_path).expect("reading webp output");
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    let info = pdfconv_core::probe(&output_path).expect("probe webp");
    assert_eq!(info.format, pdfconv_core::ImageFormat::WebP);
    assert_eq!((info.width, info.height), (1240, 1754));
}

#[test]
fn pdfs_to_images_gray_and_color_modes() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let pdf_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let add_pdf = add_pdfs(&app_state, &[pdf_path]);
    let pdf_id = add_pdf.added[0].id;

    let page_set = parse_page_range("1").unwrap();

    // Render with Gray
    let recorder_gray = TestRecorder::new();
    run_pdfs_to_images(
        &app_state,
        &[pdf_id],
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Gray,
        150,
        temp_out.path(),
        recorder_gray.callbacks(),
    )
    .expect("render gray should succeed");
    let items_gray = recorder_gray.items();
    assert_eq!(items_gray[0].status, JobItemStatus::Ok);
    let gray_path = temp_out.path().join(&items_gray[0].outputs[0]);
    let gray_bytes = fs::read(&gray_path).unwrap();
    assert_eq!(&gray_bytes[0..8], b"\x89PNG\r\n\x1a\n");
    // PNG IHDR color type is at byte 25: 0 is Grayscale, 2 is Truecolor RGB
    assert_eq!(gray_bytes[25], 0, "PNG color type must be Grayscale (0)");
    let gray_info = pdfconv_core::probe(&gray_path).expect("probe gray");
    assert_eq!(gray_info.format, pdfconv_core::ImageFormat::Png);

    // Render with Color into a clean directory
    let temp_out_color = TempDir::new().unwrap();
    let recorder_color = TestRecorder::new();
    run_pdfs_to_images(
        &app_state,
        &[pdf_id],
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        temp_out_color.path(),
        recorder_color.callbacks(),
    )
    .expect("render color should succeed");
    let items_color = recorder_color.items();
    assert_eq!(items_color[0].status, JobItemStatus::Ok);
    let color_path = temp_out_color.path().join(&items_color[0].outputs[0]);
    let color_bytes = fs::read(&color_path).unwrap();
    assert_eq!(&color_bytes[0..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(
        color_bytes[25], 2,
        "PNG color type must be Truecolor RGB (2)"
    );
    let color_info = pdfconv_core::probe(&color_path).expect("probe color");
    assert_eq!(color_info.format, pdfconv_core::ImageFormat::Png);
}

#[test]
fn pdfs_to_images_custom_non_choice_dpi_dimensions() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let pdf_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let add_pdf = add_pdfs(&app_state, &[pdf_path]);
    let pdf_id = add_pdf.added[0].id;

    let page_set = parse_page_range("1").unwrap();
    let recorder = TestRecorder::new();

    let dpi = 96;
    run_pdfs_to_images(
        &app_state,
        &[pdf_id],
        &page_set,
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        dpi,
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("custom dpi should succeed");

    let items = recorder.items();
    assert_eq!(items[0].status, JobItemStatus::Ok);
    let output_path = temp_out.path().join(&items[0].outputs[0]);
    let info = pdfconv_core::probe(&output_path).expect("probe img");
    // shapes.pdf page 1: 595.28 x 841.89 pt
    let expected_w = (595.28 / 72.0 * (dpi as f64)).round() as u32;
    let expected_h = (841.89 / 72.0 * (dpi as f64)).round() as u32;
    assert_eq!((info.width, info.height), (expected_w, expected_h));
}

#[test]
fn start_pdfs_to_images_out_of_range_dpi_resets_is_running() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let pdf_path = copy_fixture(temp_in.path(), "shapes.pdf", "shapes.pdf");
    let add_pdf = add_pdfs(&app_state, &[pdf_path]);
    let pdf_id = add_pdf.added[0].id;

    *app_state.pdfs_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());

    for bad_dpi in [35, 1001, 0] {
        let recorder = TestRecorder::new();
        let err = start_pdfs_to_images_internal(
            &app_state,
            &[pdf_id],
            "1",
            RenderFormatChoice::Png,
            RenderColorChoice::Color,
            bad_dpi,
            recorder.callbacks(),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams, "bad dpi {bad_dpi}");
        assert!(
            !app_state.is_running.load(Ordering::SeqCst),
            "is_running must be reset after bad dpi {bad_dpi}"
        );
    }

    // Verify a subsequent valid conversion starts successfully
    let recorder = TestRecorder::new();
    start_pdfs_to_images_internal(
        &app_state,
        &[pdf_id],
        "1",
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        recorder.callbacks_recording_running(Arc::clone(&app_state.is_running)),
    )
    .expect("valid conversion must start after failed attempts");

    let finished = recorder.wait_finished(Duration::from_secs(5));
    assert_eq!(finished.succeeded, 1);
    assert!(!app_state.is_running.load(Ordering::SeqCst));
}

#[test]
fn merged_pdf_progress_counts_pages_and_corrupt_tiff_page2_reaches_total() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let valid_path = copy_fixture(temp_in.path(), "photo_jpeg.tif", "photo.tif");
    let corrupt_path = copy_fixture(temp_in.path(), "corrupt_page2.tif", "corrupt.tif");

    let add_res = add_images(&app_state, &[valid_path, corrupt_path]);
    assert_eq!(add_res.added.len(), 2);
    assert_eq!(add_res.added[0].page_count, 1);
    assert_eq!(add_res.added[1].page_count, 2);

    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();
    let dest_pdf = temp_out.path().join("merged.pdf");

    let recorder = TestRecorder::new();
    let res = run_save_merged_pdf(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest_pdf,
        recorder.callbacks(),
    )
    .expect("merge should complete");

    assert_eq!(res.saved_name, Some("merged.pdf".to_string()));
    assert!(dest_pdf.exists());

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 1);
    assert_eq!(finished.unprocessed, 0);
    assert!(!finished.cancelled);

    let items = recorder.items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].status, JobItemStatus::Ok);
    assert_eq!(items[1].status, JobItemStatus::Failed);
    assert_eq!(
        items[1].error.as_ref().map(|e| e.code),
        Some(ErrorCode::DecodeFailed)
    );

    let progress = recorder.progress();
    assert!(!progress.is_empty());
    let last = progress.last().unwrap();
    // Total pages is 1 + 2 = 3. Done must reach total 3 even though corrupt_page2 failed at page 2.
    assert_eq!(last.total, 3);
    assert_eq!(last.done, 3);
}

#[test]
fn images_to_pdfs_progress_counts_pages_and_corrupt_tiff_leaves_no_file() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let valid_path = copy_fixture(temp_in.path(), "scan_g4.tif", "scan.tif");
    let corrupt_path = copy_fixture(temp_in.path(), "corrupt_page2.tif", "corrupt.tif");

    let add_res = add_images(&app_state, &[valid_path, corrupt_path]);
    assert_eq!(add_res.added.len(), 2);
    let ids: Vec<u64> = add_res.added.iter().map(|item| item.id).collect();

    let recorder = TestRecorder::new();
    run_images_to_pdfs(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("run_images_to_pdfs should complete");

    let finished = recorder.finished();
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 1);
    assert_eq!(finished.unprocessed, 0);

    let items = recorder.items();
    assert_eq!(items.len(), 2);

    // scan.pdf exists, but corrupt.pdf must not exist and no temporary files must remain
    assert!(temp_out.path().join("scan.pdf").exists());
    assert!(!temp_out.path().join("corrupt.pdf").exists());
    assert_no_temp_files(temp_out.path());

    let progress = recorder.progress();
    let last = progress.last().unwrap();
    assert_eq!(last.total, 5);
    assert_eq!(last.done, 5);
}

#[test]
fn cancellation_during_tiff_merged_pdf_writes_no_file() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path = copy_fixture(temp_in.path(), "scan_g4.tif", "scan.tif");
    let add_res = add_images(&app_state, &[path]);
    let ids = vec![add_res.added[0].id];
    let dest_pdf = temp_out.path().join("merged.pdf");

    let cancel_flag = Arc::clone(&app_state.cancel_flag);
    let p = Arc::new(Mutex::new(Vec::new()));
    let i = Arc::new(Mutex::new(Vec::new()));
    let f = Arc::new(Mutex::new(None));

    let p_clone = Arc::clone(&p);
    let i_clone = Arc::clone(&i);
    let f_clone = Arc::clone(&f);
    let flag_clone = Arc::clone(&cancel_flag);

    let callbacks = JobCallbacks {
        on_progress: move |prog: JobProgressPayload| {
            if prog.current.is_some() {
                flag_clone.store(true, Ordering::SeqCst);
            }
            p_clone.lock().unwrap().push(prog);
        },
        on_item: move |item| i_clone.lock().unwrap().push(item),
        on_finished: move |fin| *f_clone.lock().unwrap() = Some(fin),
    };

    let res = run_save_merged_pdf(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &dest_pdf,
        callbacks,
    )
    .expect("run_save_merged_pdf should handle cancellation");

    assert_eq!(res.saved_name, None);
    assert!(!dest_pdf.exists());
    assert_no_temp_files(temp_out.path());

    let finished = f.lock().unwrap().clone().expect("finished event");
    assert!(finished.cancelled);
    assert_eq!(finished.succeeded, 0);
}

const OUTPUT_PASSWORD: &str = "U15 pass-word!";
const WRONG_OUTPUT_PASSWORD: &str = "U15 pass-word?";

fn output_password_for_test(text: &str) -> OutputPassword {
    OutputPassword::new(text.to_string()).expect("a valid password")
}

/// Whether `pdf` is protected: it is refused as `PasswordProtected` when added, the wrong
/// password is refused, and `OUTPUT_PASSWORD` opens it with `page_count` pages.
fn assert_protected_by_output_password(pdf_path: &Path, page_count: u32) {
    let app_state = state();
    let added = add_pdfs(&app_state, &[pdf_path.to_path_buf()]);
    let item = &added.added[0];
    assert_eq!(
        item.error.as_ref().map(|e| e.code),
        Some(ErrorCode::PasswordProtected),
        "{pdf_path:?} is added without a password"
    );
    let wrong = unlock_pdf(&app_state, item.id, WRONG_OUTPUT_PASSWORD);
    assert_eq!(
        wrong.err().map(|e| e.code),
        Some(ErrorCode::PasswordProtected),
        "{pdf_path:?} with a wrong password"
    );
    let unlocked = unlock_pdf(&app_state, item.id, OUTPUT_PASSWORD).expect("the password opens it");
    assert_eq!(unlocked.items[0].error, None);
    assert_eq!(unlocked.items[0].page_count, page_count);
}

#[test]
fn the_merged_pdf_is_encrypted_with_the_password() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let paths = [
        copy_fixture(temp_in.path(), "photo.jpg", "a.jpg"),
        copy_fixture(temp_in.path(), "logo_alpha.png", "b.png"),
    ];
    let ids: Vec<u64> = add_images(&app_state, &paths)
        .added
        .iter()
        .map(|item| item.id)
        .collect();
    let dest_path = temp_out.path().join("merged.pdf");

    let recorder = TestRecorder::new();
    let res = run_save_merged_pdf(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(
            PdfConformance::Standard,
            Some(output_password_for_test(OUTPUT_PASSWORD)),
        ),
        &dest_path,
        recorder.callbacks(),
    )
    .expect("run_save_merged_pdf should succeed");

    assert_eq!(res.saved_name, Some("merged.pdf".to_string()));
    assert_eq!(recorder.finished().succeeded, 2);
    assert_protected_by_output_password(&dest_path, 2);
    assert_no_temp_files(temp_out.path());
}

#[test]
fn every_pdf_of_each_mode_is_encrypted_with_the_password() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let paths = [
        copy_fixture(temp_in.path(), "photo.jpg", "img1.jpg"),
        copy_fixture(temp_in.path(), "logo_alpha.png", "img2.png"),
        copy_fixture(temp_in.path(), "scan_g4.tif", "img3.tif"),
    ];
    let ids: Vec<u64> = add_images(&app_state, &paths)
        .added
        .iter()
        .map(|item| item.id)
        .collect();

    let recorder = TestRecorder::new();
    run_images_to_pdfs(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(
            PdfConformance::Standard,
            Some(output_password_for_test(OUTPUT_PASSWORD)),
        ),
        temp_out.path(),
        recorder.callbacks(),
    )
    .expect("run_images_to_pdfs should succeed");

    let items = recorder.items();
    assert_eq!(items.len(), 3);
    let page_counts = [1, 1, 3];
    for (id, page_count) in ids.iter().zip(page_counts) {
        let item = items.iter().find(|item| item.id == *id).expect("an item");
        assert_eq!(item.status, JobItemStatus::Ok);
        assert_protected_by_output_password(&temp_out.path().join(&item.outputs[0]), page_count);
    }
    assert_no_temp_files(temp_out.path());
}

#[test]
fn without_a_password_the_pdfs_are_not_encrypted() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let ids = [add_images(&app_state, &[path]).added[0].id];

    let merged = temp_out.path().join("merged.pdf");
    run_save_merged_pdf(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        &merged,
        TestRecorder::new().callbacks(),
    )
    .expect("run_save_merged_pdf should succeed");
    run_images_to_pdfs(
        &app_state,
        &ids,
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        &PdfOutput::new(PdfConformance::Standard, None),
        temp_out.path(),
        TestRecorder::new().callbacks(),
    )
    .expect("run_images_to_pdfs should succeed");

    for name in ["merged.pdf", "photo.pdf"] {
        let pdf = fs::read(temp_out.path().join(name)).expect("reading a PDF");
        assert!(!pdf.windows(8).any(|w| w == b"/Encrypt"), "{name}");
        let added = add_pdfs(&app_state, &[temp_out.path().join(name)]);
        assert_eq!(
            added.added[0].error, None,
            "{name} opens without a password"
        );
    }
}

#[test]
fn starting_images_to_pdfs_passes_the_password_on() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let id = add_images(&app_state, &[path]).added[0].id;
    *app_state.images_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());

    let recorder = TestRecorder::new();
    start_images_to_pdfs_internal(
        &app_state,
        &[id],
        PageSizeChoice::Fit,
        PageOrientationChoice::Auto,
        PdfConformance::Standard,
        Some(OUTPUT_PASSWORD.to_string()),
        recorder.callbacks(),
    )
    .expect("start_images_to_pdfs_internal should succeed");

    assert_eq!(recorder.wait_finished(Duration::from_secs(10)).succeeded, 1);
    assert_protected_by_output_password(&temp_out.path().join("photo.pdf"), 1);
}

#[test]
fn a_password_with_pdf_a_or_against_the_rule_is_refused_before_anything_starts() {
    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();
    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let id = add_images(&app_state, &[path]).added[0].id;
    *app_state.images_output_dir.lock().unwrap() = Some(temp_out.path().to_path_buf());

    let refused = [
        (PdfConformance::PdfA2b, OUTPUT_PASSWORD),
        (PdfConformance::Standard, ""),
        (PdfConformance::Standard, "パスワード"),
        (PdfConformance::Standard, "tab\there"),
        (PdfConformance::Standard, &"a".repeat(128)),
    ];
    for (conformance, password) in refused {
        let recorder = TestRecorder::new();
        let res = start_images_to_pdfs_internal(
            &app_state,
            &[id],
            PageSizeChoice::Fit,
            PageOrientationChoice::Auto,
            conformance,
            Some(password.to_string()),
            recorder.callbacks(),
        );
        assert_eq!(
            res.err().map(|e| e.code),
            Some(ErrorCode::InvalidParams),
            "{conformance:?} with {password:?}"
        );
        assert!(recorder.items().is_empty());
        assert!(
            !app_state.is_running.load(Ordering::SeqCst),
            "the refusal leaves the job idle"
        );
    }
    assert_eq!(fs::read_dir(temp_out.path()).unwrap().count(), 0);
}

#[test]
fn output_password_checks_the_rule_and_the_standard() {
    assert!(
        output_password(PdfConformance::Standard, None)
            .unwrap()
            .is_none()
    );
    assert!(
        output_password(PdfConformance::PdfA2b, None)
            .unwrap()
            .is_none()
    );
    assert!(
        output_password(PdfConformance::Standard, Some(OUTPUT_PASSWORD.to_string()))
            .unwrap()
            .is_some()
    );
    for (conformance, password) in [
        (PdfConformance::PdfA2b, OUTPUT_PASSWORD),
        (PdfConformance::Standard, ""),
        (PdfConformance::Standard, "\u{7f}"),
        (PdfConformance::Standard, "ＡＢＣ"),
    ] {
        assert_eq!(
            output_password(conformance, Some(password.to_string()))
                .err()
                .map(|e| e.code),
            Some(ErrorCode::InvalidParams),
            "{conformance:?} with {password:?}"
        );
    }
}
