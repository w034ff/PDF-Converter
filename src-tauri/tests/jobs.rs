//! Integration tests for conversion execution and jobs (work-plan T08, design §6.2-§6.5).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pdf_converter_lib::AppState;
use pdf_converter_lib::error::ErrorCode;
use pdf_converter_lib::items::{add_dropped, add_images, add_pdfs, remove_items};
use pdf_converter_lib::jobs::{
    JobCallbacks, JobFinishedPayload, JobItemPayload, JobItemStatus, JobProgressPayload,
    PageSizeChoice, RenderFormatChoice, check_page_range_internal, list_existing_files,
    run_images_to_pdfs, run_pdfs_to_images, run_save_merged_pdf, save_atomic,
    start_images_to_pdfs_internal, start_pdfs_to_images_internal,
};
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig};
use pdfconv_core::parse_page_range;
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

fn state() -> AppState {
    let config = WorkerPoolConfig::new(
        env!("CARGO_BIN_EXE_pdf-converter"),
        [
            WORKER_FLAG.to_string(),
            library_dir().to_string_lossy().into_owned(),
        ],
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
}

impl TestRecorder {
    fn new() -> Self {
        Self {
            progress: Arc::new(Mutex::new(Vec::new())),
            items: Arc::new(Mutex::new(Vec::new())),
            finished: Arc::new(Mutex::new(None)),
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

    fn items(&self) -> Vec<JobItemPayload> {
        self.items.lock().unwrap().clone()
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
        &dest_path,
        recorder.callbacks(),
    )
    .unwrap();

    assert!(res.is_none());
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

    // Invalid syntax returns InvalidPageRange
    let err = check_page_range_internal(&app_state, "abc", &[shapes_id]).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidPageRange);
    assert_eq!(err.detail, Some("abc".to_string()));
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
    let err_img =
        start_images_to_pdfs_internal(&app_state, &[id], PageSizeChoice::Fit, recorder.callbacks())
            .unwrap_err();
    assert_eq!(err_img.code, ErrorCode::ConversionRunning);

    let recorder = TestRecorder::new();
    let err_pdf = start_pdfs_to_images_internal(
        &app_state,
        &[id],
        "1",
        RenderFormatChoice::Png,
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
    let err_img_unset =
        start_images_to_pdfs_internal(&app_state, &[id], PageSizeChoice::Fit, recorder.callbacks())
            .unwrap_err();
    assert_eq!(err_img_unset.code, ErrorCode::ConversionRunning);

    let recorder = TestRecorder::new();
    let err_pdf_unset = start_pdfs_to_images_internal(
        &app_state,
        &[id],
        "1",
        RenderFormatChoice::Png,
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
    let err_img =
        start_images_to_pdfs_internal(&app_state, &[id], PageSizeChoice::Fit, recorder.callbacks())
            .unwrap_err();
    assert_eq!(err_img.code, ErrorCode::InvalidParams);
    assert!(!app_state.is_running.load(Ordering::SeqCst));

    let recorder = TestRecorder::new();
    let err_pdf = start_pdfs_to_images_internal(
        &app_state,
        &[id],
        "1",
        RenderFormatChoice::Png,
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
        &temp_out.path().join("merged.pdf"),
        recorder.callbacks(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownHandle);
}

#[cfg(unix)]
#[test]
fn unreadable_output_dir_rejects_and_resets_is_running() {
    use std::os::unix::fs::PermissionsExt;

    let app_state = state();
    let temp_in = TempDir::new().unwrap();
    let temp_out = TempDir::new().unwrap();

    let path = copy_fixture(temp_in.path(), "photo.jpg", "photo.jpg");
    let add_res = add_images(&app_state, &[path]);
    let id = add_res.added[0].id;

    let unreadable_dir = temp_out.path().join("unreadable");
    fs::create_dir(&unreadable_dir).unwrap();
    fs::set_permissions(&unreadable_dir, fs::Permissions::from_mode(0o000)).unwrap();

    *app_state.images_output_dir.lock().unwrap() = Some(unreadable_dir.clone());
    *app_state.pdfs_output_dir.lock().unwrap() = Some(unreadable_dir.clone());

    let recorder = TestRecorder::new();
    let err_img =
        start_images_to_pdfs_internal(&app_state, &[id], PageSizeChoice::Fit, recorder.callbacks())
            .unwrap_err();
    assert_eq!(err_img.code, ErrorCode::WriteFailed);
    assert!(!app_state.is_running.load(Ordering::SeqCst));

    let recorder = TestRecorder::new();
    let err_pdf = start_pdfs_to_images_internal(
        &app_state,
        &[id],
        "1",
        RenderFormatChoice::Png,
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
        recorder.callbacks(),
    )
    .expect("start_images_to_pdfs_internal should succeed");

    let finished = recorder.wait_finished(Duration::from_secs(5));
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 0);
    assert!(!finished.cancelled);
    assert!(!app_state.is_running.load(Ordering::SeqCst));

    let recorder = TestRecorder::new();
    start_pdfs_to_images_internal(
        &app_state,
        &[pdf_id],
        "1",
        RenderFormatChoice::Png,
        150,
        recorder.callbacks(),
    )
    .expect("start_pdfs_to_images_internal should succeed");

    let finished = recorder.wait_finished(Duration::from_secs(5));
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 0);
    assert!(!finished.cancelled);
    assert!(!app_state.is_running.load(Ordering::SeqCst));
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
    let res =
        run_save_merged_pdf(&app_state, &ids, PageSizeChoice::Fit, &dest_path, callbacks).unwrap();

    assert!(res.is_none());
    assert!(!dest_path.exists());
    assert_no_temp_files(temp_out.path());

    let finished = recorder.finished();
    assert!(finished.cancelled);
    assert_eq!(finished.succeeded, 0);
}
