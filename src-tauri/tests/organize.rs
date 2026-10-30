//! Integration tests for the organize tab (design organize §2–§6, design §6.5, §7): the list of
//! PDFs, what is refused before anything is made, saving one PDF, and saving in parts. They use
//! the real worker, like `jobs.rs`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pdf_converter_lib::AppState;
use pdf_converter_lib::crop::{
    DropTarget, Dropped, activate_crop_pdf, handle_drop, list_page_images, open_pdf,
    unlock_crop_pdf,
};
use pdf_converter_lib::error::{ErrorCode, IpcError};
use pdf_converter_lib::items::{self, make_thumbnail};
use pdf_converter_lib::jobs::{
    self, JobCallbacks, JobFinishedPayload, JobItemPayload, JobItemStatus, JobProgressPayload,
    RenderColorChoice, RenderFormatChoice,
};
use pdf_converter_lib::organize::{
    self, MAX_ORGANIZE_OUTPUT_BYTES, OrganizePageInput, OrganizePdf,
};
use pdf_converter_lib::organize_export::{
    OutputStop, SaveOrganizedResult, begin_single, run_save_organized_pdf,
    start_organize_split_internal,
};
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig};
use pdfconv_worker::WORKER_FLAG;
use pdfconv_worker::protocol::{Password, Request, Response};
#[cfg(feature = "test-hooks")]
use pdfconv_worker::server::{CRASH_ON_ORGANIZE_FILE_NAME, HANG_ON_ORGANIZE_FILE_NAME};
use tempfile::TempDir;

#[path = "../../crates/core/tests/support/fixture_passwords.rs"]
mod fixture_passwords;
use fixture_passwords::ENCRYPTED_USER_PASSWORD;

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

fn state() -> AppState {
    state_with(config())
}

fn state_with(config: WorkerPoolConfig) -> AppState {
    AppState::new(WorkerPool::new(config))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/core/tests/fixtures")
        .join(name)
}

/// Copies the fixture `name` into `dir` as `as_name`.
fn put(dir: &Path, name: &str, as_name: &str) -> PathBuf {
    let path = dir.join(as_name);
    fs::copy(fixture(name), &path).expect("copying a fixture");
    path
}

fn error_code(error: &Option<IpcError>) -> Option<ErrorCode> {
    error.as_ref().map(|error| error.code)
}

fn code_of<T: std::fmt::Debug>(result: Result<T, IpcError>) -> ErrorCode {
    result.expect_err("the call should be refused").code
}

/// Adds the PDFs at `paths` to the organize list and returns them.
fn add(state: &AppState, paths: &[&Path]) -> Vec<OrganizePdf> {
    let paths: Vec<PathBuf> = paths.iter().map(|path| path.to_path_buf()).collect();
    organize::add_pdfs(state, &paths)
        .expect("adding PDFs")
        .added
}

fn add_one(state: &AppState, path: &Path) -> OrganizePdf {
    add(state, &[path]).remove(0)
}

fn page(pdf: &OrganizePdf, page: u32, rotation: u16) -> OrganizePageInput {
    OrganizePageInput {
        id: pdf.id,
        page,
        rotation,
    }
}

/// The size of every page of the PDF at `path`, in the orientation shown, from a pooled worker.
fn open_sizes(
    state: &AppState,
    path: &Path,
    password: Option<&str>,
) -> Result<Vec<(f32, f32)>, ErrorCode> {
    let mut worker = state.pool.acquire().expect("borrowing a worker");
    let request = Request::Open {
        path: path.to_path_buf(),
        password: password.map(|password| Password::new(password).expect("a valid password")),
    };
    let answer = worker.send(&request);
    let _ = worker.send(&Request::Close);
    match answer {
        Ok((Response::Open { pages, .. }, _)) => Ok(pages
            .iter()
            .map(|page| (page.width_pt, page.height_pt))
            .collect()),
        Ok(other) => unreachable!("Open answered {other:?}"),
        Err(error) => Err(IpcError::from(error).code),
    }
}

fn close(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
}

fn assert_sizes(actual: &[(f32, f32)], expected: &[(f32, f32)]) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{actual:?} against {expected:?}"
    );
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!(close(*a, *e), "page {}: {a:?} against {e:?}", index + 1);
    }
}

fn sizes(pdf: &OrganizePdf) -> Vec<(f32, f32)> {
    pdf.pages
        .iter()
        .map(|page| (page.width_pt, page.height_pt))
        .collect()
}

fn turned(size: (f32, f32)) -> (f32, f32) {
    (size.1, size.0)
}

/// Asserts that no temporary files starting with '.' remain in `dir`.
fn assert_no_temp_files(dir: &Path) {
    for entry in fs::read_dir(dir).expect("reading a folder").flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(!name.starts_with('.'), "unexpected temporary file: {name}");
    }
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("reading a folder")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// What a job reported.
#[derive(Clone, Default)]
struct Recorder {
    progress: Arc<Mutex<Vec<JobProgressPayload>>>,
    items: Arc<Mutex<Vec<JobItemPayload>>>,
    finished: Arc<Mutex<Option<(JobFinishedPayload, Instant)>>>,
    running_at_finished: Arc<Mutex<Option<bool>>>,
}

impl Recorder {
    fn callbacks(
        &self,
        state: &AppState,
        on_progress: impl Fn(&JobProgressPayload) + Send + Sync + 'static,
    ) -> JobCallbacks<
        impl Fn(JobProgressPayload) + Send + Sync + 'static,
        impl Fn(JobItemPayload) + Send + Sync + 'static,
        impl FnOnce(JobFinishedPayload) + Send + Sync + 'static,
    > {
        let progress = Arc::clone(&self.progress);
        let items = Arc::clone(&self.items);
        let finished = Arc::clone(&self.finished);
        let running = Arc::clone(&self.running_at_finished);
        let is_running = Arc::clone(&state.is_running);
        JobCallbacks {
            on_progress: move |payload: JobProgressPayload| {
                on_progress(&payload);
                progress
                    .lock()
                    .expect("the recorder is not poisoned")
                    .push(payload);
            },
            on_item: move |item| {
                items
                    .lock()
                    .expect("the recorder is not poisoned")
                    .push(item)
            },
            on_finished: move |payload| {
                *running.lock().expect("the recorder is not poisoned") =
                    Some(is_running.load(Ordering::SeqCst));
                *finished.lock().expect("the recorder is not poisoned") =
                    Some((payload, Instant::now()));
            },
        }
    }

    fn quiet_callbacks(
        &self,
        state: &AppState,
    ) -> JobCallbacks<
        impl Fn(JobProgressPayload) + Send + Sync + 'static,
        impl Fn(JobItemPayload) + Send + Sync + 'static,
        impl FnOnce(JobFinishedPayload) + Send + Sync + 'static,
    > {
        self.callbacks(state, |_| {})
    }

    fn items(&self) -> Vec<JobItemPayload> {
        self.items
            .lock()
            .expect("the recorder is not poisoned")
            .clone()
    }

    fn progress(&self) -> Vec<JobProgressPayload> {
        self.progress
            .lock()
            .expect("the recorder is not poisoned")
            .clone()
    }

    fn finished(&self) -> JobFinishedPayload {
        self.finished
            .lock()
            .expect("the recorder is not poisoned")
            .clone()
            .expect("job-finished was sent")
            .0
    }

    fn finished_at(&self) -> Instant {
        self.finished
            .lock()
            .expect("the recorder is not poisoned")
            .clone()
            .expect("job-finished was sent")
            .1
    }

    fn wait_finished(&self, timeout: Duration) -> JobFinishedPayload {
        let start = Instant::now();
        loop {
            if let Some((payload, _)) = self
                .finished
                .lock()
                .expect("the recorder is not poisoned")
                .clone()
            {
                return payload;
            }
            assert!(
                start.elapsed() < timeout,
                "timed out waiting for job-finished"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn running_at_finished(&self) -> Option<bool> {
        *self
            .running_at_finished
            .lock()
            .expect("the recorder is not poisoned")
    }
}

const WAIT: Duration = Duration::from_secs(60);

/// Saves `pages` as one PDF at `dest` and reports what the command returned and the job said.
fn save_single_result(
    state: &AppState,
    pages: &[OrganizePageInput],
    password: Option<&str>,
    dest: &Path,
) -> (SaveOrganizedResult, Recorder) {
    let (_guard, output) = begin_single(state, pages, password.map(str::to_owned))
        .expect("the pages should be accepted");
    let recorder = Recorder::default();
    let result = run_save_organized_pdf(state, &output, dest, recorder.quiet_callbacks(state))
        .expect("saving");
    (result, recorder)
}

/// Saves `pages` as one PDF at `dest` and reports what the job said.
fn save_single(
    state: &AppState,
    pages: &[OrganizePageInput],
    password: Option<&str>,
    dest: &Path,
) -> (Option<String>, Recorder) {
    let (result, recorder) = save_single_result(state, pages, password, dest);
    (result.saved_name, recorder)
}

/// Starts saving `parts` in parts into the output folder `out` and waits for the end.
fn save_split(
    state: &AppState,
    parts: &[Vec<OrganizePageInput>],
    out: &Path,
) -> (Recorder, JobFinishedPayload) {
    save_split_with(state, parts, out, |_| {})
}

fn save_split_with(
    state: &AppState,
    parts: &[Vec<OrganizePageInput>],
    out: &Path,
    on_progress: impl Fn(&JobProgressPayload) + Send + Sync + 'static,
) -> (Recorder, JobFinishedPayload) {
    *state
        .organize_output_dir
        .lock()
        .expect("the folder is not poisoned") = Some(out.to_path_buf());
    let recorder = Recorder::default();
    start_organize_split_internal(state, parts, None, recorder.callbacks(state, on_progress))
        .expect("the parts should be accepted");
    let finished = recorder.wait_finished(WAIT);
    (recorder, finished)
}

// ---- The list ----

#[test]
fn a_pdf_is_added_with_its_pages_and_the_size_of_its_file() {
    let dir = TempDir::new().unwrap();
    let path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();

    let result = organize::add_pdfs(&state, std::slice::from_ref(&path)).expect("adding");

    assert_eq!(result.skipped, Default::default());
    let [pdf] = result.added.as_slice() else {
        panic!("one PDF should be added: {:?}", result.added);
    };
    assert_eq!(pdf.name, "shapes.pdf");
    assert_eq!(pdf.page_count, 3);
    assert_eq!(pdf.pages.len(), 3);
    assert!(
        pdf.pages
            .iter()
            .all(|p| p.width_pt > 0.0 && p.height_pt > 0.0)
    );
    assert_eq!(pdf.file_bytes, fs::metadata(&path).unwrap().len());
    assert!(!pdf.encrypted);
    assert_eq!(pdf.error, None);
    // The list of images and PDFs is not touched.
    assert!(state.items.is_empty());
}

#[test]
fn pdfs_are_kept_in_the_order_they_were_added_with_ids_that_do_not_repeat() {
    let dir = TempDir::new().unwrap();
    let b = put(dir.path(), "shapes.pdf", "b.pdf");
    let a = put(dir.path(), "mixed_sizes.pdf", "a.pdf");
    let state = state();

    let first = add_one(&state, &b);
    let listed = items::add_pdfs(&state, std::slice::from_ref(&a));
    let second = add_one(&state, &a);

    assert_eq!(first.name, "b.pdf");
    assert_eq!(second.name, "a.pdf");
    let list_id = listed.added[0].id;
    let mut ids = [first.id, list_id, second.id];
    ids.sort_unstable();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "{ids:?}");
}

#[test]
fn a_pdf_that_needs_a_password_or_is_broken_is_added_with_its_error() {
    let dir = TempDir::new().unwrap();
    let encrypted = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let corrupt = put(dir.path(), "corrupt.pdf", "corrupt.pdf");
    let state = state();

    let added = add(&state, &[&encrypted, &corrupt]);

    let [encrypted, corrupt] = added.as_slice() else {
        panic!("two PDFs should be added");
    };
    assert_eq!(
        error_code(&encrypted.error),
        Some(ErrorCode::PasswordProtected)
    );
    assert!(encrypted.encrypted);
    assert_eq!(encrypted.page_count, 0);
    assert!(encrypted.pages.is_empty());
    assert!(encrypted.file_bytes > 0);
    assert_eq!(error_code(&corrupt.error), Some(ErrorCode::PdfOpenFailed));
    assert!(!corrupt.encrypted);
    assert_eq!(corrupt.page_count, 0);
    assert!(corrupt.pages.is_empty());
}

#[test]
fn a_file_whose_size_cannot_be_read_is_added_with_read_failed() {
    let dir = TempDir::new().unwrap();
    let state = state();

    let added = add(&state, &[&dir.path().join("gone.pdf")]);

    let [pdf] = added.as_slice() else {
        panic!("the PDF should be added with its error");
    };
    assert_eq!(error_code(&pdf.error), Some(ErrorCode::ReadFailed));
    assert_eq!(pdf.file_bytes, 0);
    assert_eq!(pdf.page_count, 0);
    assert!(!pdf.encrypted);
}

#[test]
fn the_same_file_twice_in_the_organize_list_is_a_duplicate() {
    let dir = TempDir::new().unwrap();
    let path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    // The same file by another way of writing its path.
    let roundabout = dir.path().join(".").join("shapes.pdf");
    let state = state();

    add_one(&state, &path);
    let again = organize::add_pdfs(&state, &[path.clone(), roundabout]).expect("adding");

    assert!(again.added.is_empty());
    assert_eq!(again.skipped.duplicates, 2);
}

#[test]
fn the_same_file_in_the_other_lists_does_not_count_as_a_duplicate() {
    let dir = TempDir::new().unwrap();
    let path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    items::add_pdfs(&state, std::slice::from_ref(&path));
    open_pdf(&state, &path);

    let result = organize::add_pdfs(&state, &[path]).expect("adding");

    assert_eq!(result.added.len(), 1);
    assert_eq!(result.skipped.duplicates, 0);
    // The PDF of the list can still be added to the list of images and PDFs once.
    assert_eq!(state.items.len(), 1);
}

#[test]
fn a_folder_adds_its_pdfs_by_name_and_counts_what_it_leaves_out() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "shapes.pdf", "b.pdf");
    put(dir.path(), "mixed_sizes.pdf", "A.PDF");
    put(dir.path(), "shapes.pdf", ".hidden.pdf");
    put(dir.path(), "photo.jpg", "photo.jpg");
    fs::write(dir.path().join("notes.txt"), "text").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    put(&dir.path().join("sub"), "shapes.pdf", "nested.pdf");
    let state = state();

    let result = organize::add_pdfs_from_folder(&state, dir.path()).expect("reading the folder");

    let names: Vec<&str> = result.added.iter().map(|pdf| pdf.name.as_str()).collect();
    assert_eq!(names, ["A.PDF", "b.pdf"]);
    assert_eq!(result.skipped.unsupported, 2);
    assert_eq!(result.skipped.folders, 1);
    assert_eq!(result.skipped.duplicates, 0);
    // A second time, everything is a duplicate.
    let again = organize::add_pdfs_from_folder(&state, dir.path()).expect("reading the folder");
    assert!(again.added.is_empty());
    assert_eq!(again.skipped.duplicates, 2);
}

#[test]
fn removing_forgets_the_pdf_and_ignores_ids_it_does_not_know() {
    let dir = TempDir::new().unwrap();
    let path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let other = put(dir.path(), "mixed_sizes.pdf", "mixed_sizes.pdf");
    let state = state();
    let [first, second] = add(&state, &[&path, &other]).try_into().unwrap();
    assert!(make_thumbnail(&state, first.id, None, None).is_ok());

    organize::remove_pdfs(&state, &[first.id, 9999, first.id]).expect("removing");

    assert_eq!(
        code_of(make_thumbnail(&state, first.id, None, None)),
        ErrorCode::UnknownHandle
    );
    assert!(make_thumbnail(&state, second.id, None, None).is_ok());
    // The file can be added again, as a new PDF.
    let again = add_one(&state, &path);
    assert_ne!(again.id, first.id);
}

#[test]
fn a_thumbnail_is_a_png_of_the_page() {
    let dir = TempDir::new().unwrap();
    let path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let pdf = add_one(&state, &path);

    let first = make_thumbnail(&state, pdf.id, None, None).expect("the first page");
    let third = make_thumbnail(&state, pdf.id, Some(3), Some(64)).expect("the third page");

    assert!(first.starts_with(b"\x89PNG"));
    assert!(third.starts_with(b"\x89PNG"));
    assert_ne!(first, third);
    for bad_page in [0, 4] {
        assert_eq!(
            code_of(make_thumbnail(&state, pdf.id, Some(bad_page), None)),
            ErrorCode::InvalidParams
        );
    }
    assert_eq!(
        code_of(make_thumbnail(&state, pdf.id, None, Some(0))),
        ErrorCode::InvalidParams
    );
}

#[test]
fn a_pdf_with_an_error_has_no_thumbnail_and_says_why() {
    let dir = TempDir::new().unwrap();
    let encrypted = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let corrupt = put(dir.path(), "corrupt.pdf", "corrupt.pdf");
    let state = state();
    let [encrypted, corrupt] = add(&state, &[&encrypted, &corrupt]).try_into().unwrap();

    assert_eq!(
        code_of(make_thumbnail(&state, encrypted.id, None, None)),
        ErrorCode::PasswordProtected
    );
    assert_eq!(
        code_of(make_thumbnail(&state, corrupt.id, None, None)),
        ErrorCode::PdfOpenFailed
    );
}

#[test]
fn the_right_password_opens_a_pdf_and_it_stays_marked_as_encrypted() {
    let dir = TempDir::new().unwrap();
    let path = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let state = state();
    let pdf = add_one(&state, &path);

    let unlocked =
        organize::unlock_pdf(&state, pdf.id, ENCRYPTED_USER_PASSWORD).expect("unlocking");

    let [opened] = unlocked.items.as_slice() else {
        panic!(
            "only the PDF itself should be answered: {:?}",
            unlocked.items
        );
    };
    assert_eq!(opened.id, pdf.id);
    assert_eq!(opened.error, None);
    assert!(opened.page_count > 0);
    assert_eq!(opened.pages.len() as u32, opened.page_count);
    assert!(opened.encrypted);
    assert_eq!(opened.file_bytes, pdf.file_bytes);
    assert!(make_thumbnail(&state, pdf.id, None, None).is_ok());
}

#[test]
fn a_wrong_password_changes_nothing() {
    let dir = TempDir::new().unwrap();
    let path = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let state = state();
    let pdf = add_one(&state, &path);

    assert_eq!(
        code_of(organize::unlock_pdf(&state, pdf.id, "wrong-password")),
        ErrorCode::PasswordProtected
    );
    assert_eq!(
        code_of(make_thumbnail(&state, pdf.id, None, None)),
        ErrorCode::PasswordProtected
    );
    // The right one still works.
    organize::unlock_pdf(&state, pdf.id, ENCRYPTED_USER_PASSWORD).expect("unlocking");
}

#[test]
fn a_password_is_tried_on_the_other_pdfs_that_wait_for_one_and_the_target_comes_first() {
    let dir = TempDir::new().unwrap();
    let first = put(dir.path(), "encrypted.pdf", "one.pdf");
    let second = put(dir.path(), "encrypted.pdf", "two.pdf");
    let third = put(dir.path(), "encrypted.pdf", "three.pdf");
    let plain = put(dir.path(), "shapes.pdf", "plain.pdf");
    let state = state();
    let [one, two, three, plain] = add(&state, &[&first, &second, &third, &plain])
        .try_into()
        .unwrap();

    let unlocked =
        organize::unlock_pdf(&state, two.id, ENCRYPTED_USER_PASSWORD).expect("unlocking");

    let ids: Vec<u64> = unlocked.items.iter().map(|pdf| pdf.id).collect();
    assert_eq!(ids, [two.id, one.id, three.id]);
    assert!(
        unlocked
            .items
            .iter()
            .all(|pdf| pdf.error.is_none() && pdf.encrypted)
    );
    // A PDF that never needed a password is not in the answer.
    assert!(!ids.contains(&plain.id));
}

#[test]
fn the_other_lists_do_not_share_passwords_with_the_organize_list() {
    let dir = TempDir::new().unwrap();
    let path = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let listed_path = put(dir.path(), "encrypted.pdf", "listed.pdf");
    let cropped_path = put(dir.path(), "encrypted.pdf", "cropped.pdf");
    let state = state();
    let listed = items::add_pdfs(&state, std::slice::from_ref(&listed_path))
        .added
        .remove(0);
    let cropped = open_pdf(&state, &cropped_path);
    activate_crop_pdf(&state, cropped.id).expect("activating");
    let pdf = add_one(&state, &path);

    // The organize PDF is unlocked; the other two are not tried.
    organize::unlock_pdf(&state, pdf.id, ENCRYPTED_USER_PASSWORD).expect("unlocking");
    assert!(state.items.get(listed.id).unwrap().password.is_none());
    assert_eq!(
        code_of(make_thumbnail(&state, cropped.id, None, None)),
        ErrorCode::PasswordProtected
    );

    // And the other way round, with another organize PDF waiting.
    let waiting_path = put(dir.path(), "encrypted.pdf", "waiting.pdf");
    let waiting = add_one(&state, &waiting_path);
    items::unlock_pdf(&state, listed.id, ENCRYPTED_USER_PASSWORD).expect("unlocking");
    unlock_crop_pdf(&state, cropped.id, ENCRYPTED_USER_PASSWORD).expect("unlocking");
    assert_eq!(
        code_of(make_thumbnail(&state, waiting.id, None, None)),
        ErrorCode::PasswordProtected
    );
}

#[test]
fn unlocking_refuses_what_is_not_waiting_for_a_password() {
    let dir = TempDir::new().unwrap();
    let plain = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let encrypted = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let state = state();
    let [plain, encrypted] = add(&state, &[&plain, &encrypted]).try_into().unwrap();

    assert_eq!(
        code_of(organize::unlock_pdf(&state, 9999, "pw")),
        ErrorCode::UnknownHandle
    );
    assert_eq!(
        code_of(organize::unlock_pdf(&state, plain.id, "pw")),
        ErrorCode::InvalidParams
    );
    assert_eq!(
        code_of(organize::unlock_pdf(&state, encrypted.id, "")),
        ErrorCode::InvalidParams
    );
    // A NUL cannot be given to pdfium; it is a wrong password.
    assert_eq!(
        code_of(organize::unlock_pdf(&state, encrypted.id, "a\0b")),
        ErrorCode::PasswordProtected
    );
}

#[test]
fn adding_removing_and_unlocking_are_refused_during_a_conversion() {
    let dir = TempDir::new().unwrap();
    let plain = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let encrypted = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let state = state();
    let [plain_pdf, encrypted_pdf] = add(&state, &[&plain, &encrypted]).try_into().unwrap();
    state.is_running.store(true, Ordering::SeqCst);

    assert_eq!(
        code_of(organize::add_pdfs(&state, std::slice::from_ref(&plain))),
        ErrorCode::ConversionRunning
    );
    assert_eq!(
        code_of(organize::add_pdfs_from_folder(&state, dir.path())),
        ErrorCode::ConversionRunning
    );
    assert_eq!(
        code_of(organize::remove_pdfs(&state, &[plain_pdf.id])),
        ErrorCode::ConversionRunning
    );
    assert_eq!(
        code_of(organize::unlock_pdf(
            &state,
            encrypted_pdf.id,
            ENCRYPTED_USER_PASSWORD
        )),
        ErrorCode::ConversionRunning
    );
    state.is_running.store(false, Ordering::SeqCst);
    // Nothing changed.
    assert!(make_thumbnail(&state, plain_pdf.id, None, None).is_ok());
    assert_eq!(
        code_of(make_thumbnail(&state, encrypted_pdf.id, None, None)),
        ErrorCode::PasswordProtected
    );
}

// ---- Drops ----

#[test]
fn a_drop_in_the_organize_tab_goes_to_the_organize_list_only() {
    let dir = TempDir::new().unwrap();
    let pdf = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let image = put(dir.path(), "photo.jpg", "photo.jpg");
    let notes = dir.path().join("notes.txt");
    fs::write(&notes, "text").unwrap();
    let folder = dir.path().join("folder");
    fs::create_dir(&folder).unwrap();
    put(&folder, "mixed_sizes.pdf", "inside.pdf");
    put(&folder, "photo.jpg", "inside.png");
    fs::create_dir(folder.join("sub")).unwrap();
    let state = state();
    state.crop.set_drop_target(DropTarget::Organize);

    let Dropped::Organize(dropped) = handle_drop(&state, &[pdf.clone(), image, notes, folder])
    else {
        panic!("the organize tab takes the drop");
    };

    let names: Vec<&str> = dropped.added.iter().map(|pdf| pdf.name.as_str()).collect();
    assert_eq!(names, ["shapes.pdf", "inside.pdf"]);
    // The image and the text file, and the image in the folder; and the subfolder.
    assert_eq!(dropped.skipped.unsupported, 3);
    assert_eq!(dropped.skipped.folders, 1);
    assert_eq!(dropped.error, None);
    assert!(state.items.is_empty());

    // The same drop again adds nothing.
    let Dropped::Organize(again) = handle_drop(&state, &[pdf]) else {
        panic!("the organize tab takes the drop");
    };
    assert!(again.added.is_empty());
    assert_eq!(again.skipped.duplicates, 1);
}

#[test]
fn a_drop_in_the_organize_tab_during_a_conversion_adds_nothing() {
    let dir = TempDir::new().unwrap();
    let pdf = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    state.crop.set_drop_target(DropTarget::Organize);
    state.is_running.store(true, Ordering::SeqCst);

    let Dropped::Organize(dropped) = handle_drop(&state, std::slice::from_ref(&pdf)) else {
        panic!("the organize tab takes the drop");
    };

    assert!(dropped.added.is_empty());
    assert_eq!(
        error_code(&dropped.error),
        Some(ErrorCode::ConversionRunning)
    );
    state.is_running.store(false, Ordering::SeqCst);
    let again = organize::add_pdfs(&state, &[pdf]).expect("adding");
    assert_eq!(again.added.len(), 1, "the drop added nothing");
}

#[test]
fn a_new_page_sends_drops_back_to_the_lists() {
    let state = state();
    state.crop.set_drop_target(DropTarget::Organize);

    state.crop.start_page();

    assert_eq!(state.crop.drop_target(), DropTarget::Lists);
}

// ---- The other commands do not know organize PDFs, and the reverse ----

#[test]
fn the_commands_of_the_lists_and_of_crop_treat_an_organize_id_as_unknown() {
    let dir = TempDir::new().unwrap();
    let path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let pdf = add_one(&state, &path);
    let id = pdf.id;

    // The commands of the lists.
    items::remove_items(&state, &[id]).expect("removing");
    assert!(
        make_thumbnail(&state, id, None, None).is_ok(),
        "it is still here"
    );
    assert_eq!(
        code_of(items::unlock_pdf(&state, id, "pw")),
        ErrorCode::UnknownHandle
    );
    let range = jobs::check_page_range_internal(&state, "1-3", &[id]).expect("checking");
    assert_eq!(range.total_pages, 0);
    // The output folder is looked at before the IDs, so one has to be chosen.
    *state.pdfs_output_dir.lock().unwrap() = Some(dir.path().to_path_buf());
    let callbacks = Recorder::default();
    assert_eq!(
        code_of(jobs::start_pdfs_to_images_internal(
            &state,
            &[id],
            "1",
            RenderFormatChoice::Png,
            RenderColorChoice::Color,
            150,
            callbacks.quiet_callbacks(&state),
        )),
        ErrorCode::UnknownHandle
    );
    assert!(!state.is_running.load(Ordering::SeqCst));

    // The commands of crop.
    assert_eq!(
        code_of(activate_crop_pdf(&state, id).map(drop)),
        ErrorCode::UnknownHandle
    );
    assert_eq!(
        code_of(list_page_images(&state, id, 1)),
        ErrorCode::UnknownHandle
    );
    assert_eq!(
        code_of(unlock_crop_pdf(&state, id, "pw")),
        ErrorCode::UnknownHandle
    );
}

#[test]
fn the_commands_of_organize_treat_the_ids_of_the_lists_and_of_crop_as_unknown() {
    let dir = TempDir::new().unwrap();
    let listed_path = put(dir.path(), "encrypted.pdf", "listed.pdf");
    let cropped_path = put(dir.path(), "shapes.pdf", "cropped.pdf");
    let state = state();
    let listed = items::add_pdfs(&state, std::slice::from_ref(&listed_path))
        .added
        .remove(0);
    let cropped = open_pdf(&state, &cropped_path);
    activate_crop_pdf(&state, cropped.id).expect("activating");

    for id in [listed.id, cropped.id] {
        assert_eq!(
            code_of(organize::unlock_pdf(&state, id, ENCRYPTED_USER_PASSWORD)),
            ErrorCode::UnknownHandle
        );
        let pages = [OrganizePageInput {
            id,
            page: 1,
            rotation: 0,
        }];
        assert_eq!(
            code_of(begin_single(&state, &pages, None).map(drop)),
            ErrorCode::UnknownHandle
        );
        organize::remove_pdfs(&state, &[id]).expect("removing");
    }
    // Removing them from the organize list did nothing to them.
    assert_eq!(state.items.len(), 1);
    assert!(make_thumbnail(&state, cropped.id, None, None).is_ok());
}

// ---- What is refused before anything is made ----

/// Two PDFs in the organize list: `shapes.pdf` (3 pages) and `mixed_sizes.pdf` (3 pages, the
/// second 3000 × 3000 pt), and a broken one.
struct Fixtures {
    _dir: TempDir,
    shapes: OrganizePdf,
    mixed: OrganizePdf,
    corrupt: OrganizePdf,
}

fn fixtures(state: &AppState) -> Fixtures {
    let dir = TempDir::new().expect("creating a temporary folder");
    let shapes = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let mixed = put(dir.path(), "mixed_sizes.pdf", "mixed_sizes.pdf");
    let corrupt = put(dir.path(), "corrupt.pdf", "corrupt.pdf");
    let [shapes, mixed, corrupt] = add(state, &[&shapes, &mixed, &corrupt])
        .try_into()
        .expect("three PDFs were added");
    Fixtures {
        _dir: dir,
        shapes,
        mixed,
        corrupt,
    }
}

/// Every set of pages that is refused, with the code it is refused with.
fn refused_parts(f: &Fixtures) -> Vec<(&'static str, Vec<Vec<OrganizePageInput>>, ErrorCode)> {
    let bad = ErrorCode::InvalidParams;
    vec![
        ("no output", vec![], bad),
        ("an empty output", vec![vec![]], bad),
        (
            "an empty output after another",
            vec![vec![page(&f.shapes, 1, 0)], vec![]],
            bad,
        ),
        ("a rotation of 45", vec![vec![page(&f.shapes, 1, 45)]], bad),
        (
            "a rotation of 360",
            vec![vec![page(&f.shapes, 1, 360)]],
            bad,
        ),
        ("page 0", vec![vec![page(&f.shapes, 0, 0)]], bad),
        (
            "a page past the end",
            vec![vec![page(&f.shapes, 4, 0)]],
            bad,
        ),
        (
            "a page twice",
            vec![vec![page(&f.shapes, 2, 0), page(&f.shapes, 2, 90)]],
            bad,
        ),
        (
            "a page twice across outputs",
            vec![vec![page(&f.shapes, 2, 0)], vec![page(&f.shapes, 2, 0)]],
            bad,
        ),
        (
            "an unknown PDF",
            vec![vec![OrganizePageInput {
                id: 9999,
                page: 1,
                rotation: 0,
            }]],
            ErrorCode::UnknownHandle,
        ),
        (
            "a broken PDF",
            vec![vec![page(&f.shapes, 1, 0), page(&f.corrupt, 1, 0)]],
            ErrorCode::PdfOpenFailed,
        ),
    ]
}

#[test]
fn what_cannot_be_made_is_refused_for_one_pdf_before_the_dialog_and_leaves_nothing_running() {
    let state = state();
    let f = fixtures(&state);

    for (what, parts, expected) in refused_parts(&f) {
        // A set of several outputs has no meaning for one PDF.
        if parts.len() > 1 {
            continue;
        }
        let pages = parts.into_iter().next().unwrap_or_default();
        let refused = begin_single(&state, &pages, None).map(drop);
        assert_eq!(code_of(refused), expected, "{what}");
        assert!(!state.is_running.load(Ordering::SeqCst), "{what}");
    }
    // A password that breaks the rule of design §4.3.1.
    for password in ["", "日本語"] {
        let refused = begin_single(&state, &[page(&f.shapes, 1, 0)], Some(password.into()));
        assert_eq!(code_of(refused.map(drop)), ErrorCode::InvalidParams);
        assert!(!state.is_running.load(Ordering::SeqCst));
    }
}

#[test]
fn what_cannot_be_made_is_refused_in_parts_with_nothing_written_and_nothing_left_running() {
    let state = state();
    let f = fixtures(&state);
    let out = TempDir::new().unwrap();
    *state.organize_output_dir.lock().unwrap() = Some(out.path().to_path_buf());

    for (what, parts, expected) in refused_parts(&f) {
        let recorder = Recorder::default();
        let refused =
            start_organize_split_internal(&state, &parts, None, recorder.quiet_callbacks(&state));
        assert_eq!(code_of(refused), expected, "{what}");
        assert!(!state.is_running.load(Ordering::SeqCst), "{what}");
        assert!(recorder.progress().is_empty(), "{what}");
        assert!(recorder.items().is_empty(), "{what}");
        assert!(names_in(out.path()).is_empty(), "{what}");
    }
    let recorder = Recorder::default();
    let bad_password = start_organize_split_internal(
        &state,
        &[vec![page(&f.shapes, 1, 0)]],
        Some("日本語".into()),
        recorder.quiet_callbacks(&state),
    );
    assert_eq!(code_of(bad_password), ErrorCode::InvalidParams);
    assert!(!state.is_running.load(Ordering::SeqCst));
    assert!(names_in(out.path()).is_empty());
}

#[test]
fn a_conversion_that_is_running_refuses_to_start_another_and_stays_running() {
    let state = state();
    let f = fixtures(&state);
    *state.organize_output_dir.lock().unwrap() = Some(f._dir.path().to_path_buf());
    state.is_running.store(true, Ordering::SeqCst);
    let pages = [page(&f.shapes, 1, 0)];

    assert_eq!(
        code_of(begin_single(&state, &pages, None).map(drop)),
        ErrorCode::ConversionRunning
    );
    let recorder = Recorder::default();
    assert_eq!(
        code_of(start_organize_split_internal(
            &state,
            &[pages.to_vec()],
            None,
            recorder.quiet_callbacks(&state),
        )),
        ErrorCode::ConversionRunning
    );
    assert!(
        state.is_running.load(Ordering::SeqCst),
        "it is the other job's"
    );
}

#[test]
fn the_folder_to_save_in_is_checked_after_the_pages_and_before_any_name() {
    let state = state();
    let f = fixtures(&state);
    let pages = vec![vec![page(&f.shapes, 1, 0)]];
    let recorder = Recorder::default();

    // Not chosen.
    assert_eq!(
        code_of(start_organize_split_internal(
            &state,
            &pages,
            None,
            recorder.quiet_callbacks(&state)
        )),
        ErrorCode::InvalidParams
    );
    assert!(!state.is_running.load(Ordering::SeqCst));

    // Chosen but gone: it goes back to "not chosen".
    let gone = f._dir.path().join("gone");
    *state.organize_output_dir.lock().unwrap() = Some(gone);
    assert_eq!(
        code_of(start_organize_split_internal(
            &state,
            &pages,
            None,
            recorder.quiet_callbacks(&state)
        )),
        ErrorCode::OutputDirMissing
    );
    assert_eq!(*state.organize_output_dir.lock().unwrap(), None);
    assert!(!state.is_running.load(Ordering::SeqCst));

    // A file in place of a folder.
    *state.organize_output_dir.lock().unwrap() = Some(f._dir.path().join("shapes.pdf"));
    assert_eq!(
        code_of(start_organize_split_internal(
            &state,
            &pages,
            None,
            recorder.quiet_callbacks(&state)
        )),
        ErrorCode::OutputDirMissing
    );
    assert_eq!(*state.organize_output_dir.lock().unwrap(), None);
}

#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_written_to_is_refused_and_stays_chosen() {
    use std::os::unix::fs::PermissionsExt;

    let state = state();
    let f = fixtures(&state);
    let out = TempDir::new().unwrap();
    fs::set_permissions(out.path(), fs::Permissions::from_mode(0o555)).unwrap();
    if fs::File::create(out.path().join("probe")).is_ok() {
        // Running as a user who can write anywhere, such as root.
        fs::set_permissions(out.path(), fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipped: the folder is writable although its mode forbids it");
        return;
    }
    *state.organize_output_dir.lock().unwrap() = Some(out.path().to_path_buf());
    let recorder = Recorder::default();

    let refused = start_organize_split_internal(
        &state,
        &[vec![page(&f.shapes, 1, 0)]],
        None,
        recorder.quiet_callbacks(&state),
    );

    fs::set_permissions(out.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(code_of(refused), ErrorCode::OutputDirNotWritable);
    assert!(state.organize_output_dir.lock().unwrap().is_some());
    assert!(!state.is_running.load(Ordering::SeqCst));
}

// ---- The size of an output ----

#[cfg(feature = "test-hooks")]
#[test]
fn an_output_over_the_limit_is_refused_before_anything_else_is_asked() {
    let state = state();
    let f = fixtures(&state);
    let all = [
        page(&f.shapes, 1, 0),
        page(&f.shapes, 2, 0),
        page(&f.shapes, 3, 0),
    ];

    // Exactly the limit is let through; one byte more is not.
    assert!(
        state
            .organize
            .set_file_bytes_for_test(f.shapes.id, MAX_ORGANIZE_OUTPUT_BYTES)
    );
    let (guard, _) = begin_single(&state, &all, None).expect("exactly the limit");
    drop(guard);
    assert!(
        state
            .organize
            .set_file_bytes_for_test(f.shapes.id, MAX_ORGANIZE_OUTPUT_BYTES + 1)
    );
    let refused = begin_single(&state, &all, None).map(drop).unwrap_err();
    assert_eq!(refused.code, ErrorCode::OutputTooLarge);
    assert_eq!(refused.detail.as_deref(), Some("513 MB / 512 MB"));
    assert!(!state.is_running.load(Ordering::SeqCst));

    // A part of the pages is estimated by its share: a third of 1536 MB is the limit.
    assert!(
        state
            .organize
            .set_file_bytes_for_test(f.shapes.id, 3 * MAX_ORGANIZE_OUTPUT_BYTES)
    );
    assert!(begin_single(&state, &all[..1], None).is_ok());
    let refused = begin_single(&state, &all[..2], None).map(drop).unwrap_err();
    assert_eq!(refused.code, ErrorCode::OutputTooLarge);
    assert_eq!(refused.detail.as_deref(), Some("1024 MB / 512 MB"));
}

#[cfg(feature = "test-hooks")]
#[test]
fn in_parts_the_first_output_over_the_limit_refuses_all_of_them_before_the_folder_is_looked_at() {
    let state = state();
    let f = fixtures(&state);
    assert!(
        state
            .organize
            .set_file_bytes_for_test(f.shapes.id, 3 * MAX_ORGANIZE_OUTPUT_BYTES)
    );
    assert!(
        state
            .organize
            .set_file_bytes_for_test(f.mixed.id, 5 * MAX_ORGANIZE_OUTPUT_BYTES)
    );
    // No folder is chosen, which would be an InvalidParams if it were looked at first.
    let parts = vec![
        vec![page(&f.shapes, 1, 0)],
        vec![
            page(&f.mixed, 1, 0),
            page(&f.mixed, 2, 0),
            page(&f.mixed, 3, 0),
        ],
        vec![page(&f.shapes, 2, 0), page(&f.shapes, 3, 0)],
    ];
    let recorder = Recorder::default();

    let refused =
        start_organize_split_internal(&state, &parts, None, recorder.quiet_callbacks(&state))
            .unwrap_err();

    assert_eq!(refused.code, ErrorCode::OutputTooLarge);
    // The second output is the first over the limit: 5 × 512 MB, and the third is 1024 MB.
    assert_eq!(refused.detail.as_deref(), Some("2560 MB / 512 MB"));
    assert!(!state.is_running.load(Ordering::SeqCst));
}

// ---- Saving one PDF ----

#[test]
fn pages_of_two_pdfs_are_mixed_and_turned_into_one_pdf() {
    let dir = TempDir::new().unwrap();
    let links = put(dir.path(), "organize_links.pdf", "organize_links.pdf");
    let meta = put(dir.path(), "organize_meta.pdf", "organize_meta.pdf");
    let state = state();
    let [links, meta] = add(&state, &[&links, &meta]).try_into().unwrap();
    let out = TempDir::new().unwrap();
    let dest = out.path().join("mixed.pdf");

    let (saved, recorder) = save_single(
        &state,
        &[page(&links, 2, 90), page(&meta, 1, 0), page(&links, 1, 0)],
        None,
        &dest,
    );

    assert_eq!(saved.as_deref(), Some("mixed.pdf"));
    let expected = [turned(sizes(&links)[1]), sizes(&meta)[0], sizes(&links)[0]];
    assert_sizes(
        &open_sizes(&state, &dest, None).expect("opening the output"),
        &expected,
    );
    assert!(
        !close(expected[0], sizes(&links)[1]),
        "the fixture's pages are not square, so a turn shows"
    );

    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, 1);
    assert_eq!(items[0].status, JobItemStatus::Ok);
    assert_eq!(items[0].outputs, ["mixed.pdf"]);
    assert_eq!(items[0].error, None);
    assert_eq!(items[0].source_id, None);
    assert_eq!(
        recorder.finished(),
        JobFinishedPayload {
            succeeded: 1,
            failed: 0,
            no_pages: 0,
            unprocessed: 0,
            cancelled: false,
        }
    );
    let progress = recorder.progress();
    assert_eq!(
        progress
            .first()
            .map(|p| (p.done, p.total, p.current.clone())),
        Some((0, 3, Some("mixed.pdf".into())))
    );
    assert_eq!(progress.last().map(|p| (p.done, p.total)), Some((3, 3)));
    assert!(progress.windows(2).all(|pair| pair[0].done <= pair[1].done));
    assert_no_temp_files(out.path());
}

#[test]
fn the_order_of_the_pages_given_is_the_order_of_the_pages_made() {
    let dir = TempDir::new().unwrap();
    let mixed = put(dir.path(), "mixed_sizes.pdf", "mixed_sizes.pdf");
    let shapes = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let [mixed, shapes] = add(&state, &[&mixed, &shapes]).try_into().unwrap();
    let out = TempDir::new().unwrap();
    let dest = out.path().join("order.pdf");
    let big = sizes(&mixed)[1];
    let a4 = sizes(&mixed)[0];
    assert!(!close(big, a4), "the fixture has a page of another size");

    let (saved, _) = save_single(
        &state,
        &[
            page(&mixed, 3, 0),
            page(&mixed, 2, 0),
            page(&shapes, 1, 270),
            page(&mixed, 1, 90),
            page(&shapes, 3, 180),
        ],
        None,
        &dest,
    );

    assert!(saved.is_some());
    assert_sizes(
        &open_sizes(&state, &dest, None).unwrap(),
        &[
            sizes(&mixed)[2],
            big,
            turned(sizes(&shapes)[0]),
            turned(a4),
            sizes(&shapes)[2],
        ],
    );
}

#[test]
fn saving_over_a_source_replaces_it() {
    let dir = TempDir::new().unwrap();
    let source = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let pdf = add_one(&state, &source);
    assert_eq!(open_sizes(&state, &source, None).unwrap().len(), 3);

    let (saved, recorder) = save_single(&state, &[page(&pdf, 2, 0)], None, &source);

    assert_eq!(saved.as_deref(), Some("shapes.pdf"));
    assert_eq!(open_sizes(&state, &source, None).unwrap().len(), 1);
    assert_eq!(recorder.finished().succeeded, 1);
    assert_no_temp_files(dir.path());
    assert_eq!(names_in(dir.path()), ["shapes.pdf"]);
}

#[test]
fn saving_over_a_source_drops_it_from_the_list_and_says_which() {
    let dir = TempDir::new().unwrap();
    let source = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let other = put(dir.path(), "mixed_sizes.pdf", "mixed_sizes.pdf");
    let state = state();
    let [pdf, kept] = add(&state, &[&source, &other]).try_into().unwrap();

    let (result, _) = save_single_result(
        &state,
        &[page(&pdf, 2, 0), page(&kept, 1, 0)],
        None,
        &source,
    );

    assert_eq!(result.saved_name.as_deref(), Some("shapes.pdf"));
    assert_eq!(result.replaced_id, Some(pdf.id));
    assert_eq!(
        code_of(make_thumbnail(&state, pdf.id, None, None)),
        ErrorCode::UnknownHandle
    );
    assert!(make_thumbnail(&state, kept.id, None, None).is_ok());
    // The file can be added again, as the new PDF it now is.
    let again = add_one(&state, &source);
    assert_eq!(again.page_count, 2);
}

#[test]
fn saving_to_another_file_leaves_the_list_as_it_was() {
    let dir = TempDir::new().unwrap();
    let source = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let pdf = add_one(&state, &source);
    let out = TempDir::new().unwrap();
    let dest = out.path().join("shapes.pdf");

    let (result, _) = save_single_result(&state, &[page(&pdf, 1, 0)], None, &dest);

    assert_eq!(result.saved_name.as_deref(), Some("shapes.pdf"));
    assert_eq!(result.replaced_id, None);
    assert!(make_thumbnail(&state, pdf.id, None, None).is_ok());
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_save_over_a_source_that_fails_leaves_the_list_as_it_was() {
    let state = state();
    let dir = TempDir::new().unwrap();
    let crash_path = put(dir.path(), "shapes.pdf", CRASH_ON_ORGANIZE_FILE_NAME);
    let crash = add_one(&state, &crash_path);
    let before = fs::read(&crash_path).unwrap();

    let (result, recorder) = save_single_result(&state, &[page(&crash, 1, 0)], None, &crash_path);

    assert_eq!(result.saved_name, None);
    assert_eq!(result.replaced_id, None);
    assert_eq!(recorder.items()[0].status, JobItemStatus::Failed);
    assert_eq!(fs::read(&crash_path).unwrap(), before);
    assert!(make_thumbnail(&state, crash.id, None, None).is_ok());
}

#[cfg(feature = "test-hooks")]
#[test]
fn cancelling_a_save_over_a_source_leaves_the_list_as_it_was() {
    let state = state();
    let dir = TempDir::new().unwrap();
    let hang_path = put(dir.path(), "shapes.pdf", HANG_ON_ORGANIZE_FILE_NAME);
    let hang = add_one(&state, &hang_path);
    let (_guard, output) = begin_single(&state, &[page(&hang, 1, 0)], None).unwrap();
    let cancel = Arc::clone(&state.cancel_flag);
    let recorder = Recorder::default();
    let callbacks = recorder.callbacks(&state, move |progress| {
        if progress.done == 0 {
            let cancel = Arc::clone(&cancel);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(500));
                cancel.store(true, Ordering::SeqCst);
            });
        }
    });

    let result = run_save_organized_pdf(&state, &output, &hang_path, callbacks).expect("saving");

    assert_eq!(result.saved_name, None);
    assert_eq!(result.replaced_id, None);
    assert!(make_thumbnail(&state, hang.id, None, None).is_ok());
}

#[test]
fn an_existing_file_is_replaced_when_the_dialog_was_confirmed() {
    let dir = TempDir::new().unwrap();
    let source = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let pdf = add_one(&state, &source);
    let out = TempDir::new().unwrap();
    let dest = out.path().join("out.pdf");
    fs::write(&dest, b"old").unwrap();

    let (saved, _) = save_single(&state, &[page(&pdf, 1, 0)], None, &dest);

    assert_eq!(saved.as_deref(), Some("out.pdf"));
    assert_eq!(open_sizes(&state, &dest, None).unwrap().len(), 1);
}

#[test]
fn a_locked_source_and_a_password_for_the_output_work_together() {
    let dir = TempDir::new().unwrap();
    let encrypted = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let shapes = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let [encrypted, shapes] = add(&state, &[&encrypted, &shapes]).try_into().unwrap();
    let unlocked = organize::unlock_pdf(&state, encrypted.id, ENCRYPTED_USER_PASSWORD)
        .expect("unlocking")
        .items;
    let out = TempDir::new().unwrap();
    let dest = out.path().join("locked.pdf");

    let (saved, recorder) = save_single(
        &state,
        &[page(&unlocked[0], 1, 0), page(&shapes, 1, 0)],
        Some("output-secret"),
        &dest,
    );

    assert!(saved.is_some(), "{:?}", recorder.items());
    assert_eq!(
        open_sizes(&state, &dest, None).unwrap_err(),
        ErrorCode::PasswordProtected
    );
    assert_eq!(
        open_sizes(&state, &dest, Some("wrong")).unwrap_err(),
        ErrorCode::PasswordProtected
    );
    assert_eq!(
        open_sizes(&state, &dest, Some("output-secret"))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn a_source_that_failed_after_it_was_added_saves_nothing_and_names_the_source() {
    let dir = TempDir::new().unwrap();
    let source = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let other = put(dir.path(), "mixed_sizes.pdf", "mixed_sizes.pdf");
    let state = state();
    let [broken, other] = add(&state, &[&source, &other]).try_into().unwrap();
    // The file changed after it was opened for the list.
    fs::copy(fixture("corrupt.pdf"), &source).unwrap();
    let out = TempDir::new().unwrap();
    let dest = out.path().join("never.pdf");
    fs::write(&dest, b"kept").unwrap();

    let (saved, recorder) = save_single(
        &state,
        &[page(&other, 1, 0), page(&broken, 1, 0)],
        None,
        &dest,
    );

    assert_eq!(saved, None);
    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, 1);
    assert_eq!(items[0].status, JobItemStatus::Failed);
    assert!(items[0].outputs.is_empty());
    assert_eq!(error_code(&items[0].error), Some(ErrorCode::PdfOpenFailed));
    assert_eq!(items[0].source_id, Some(broken.id));
    assert_eq!(
        recorder.finished(),
        JobFinishedPayload {
            succeeded: 0,
            failed: 1,
            no_pages: 0,
            unprocessed: 0,
            cancelled: false,
        }
    );
    assert_eq!(
        fs::read(&dest).unwrap(),
        b"kept",
        "the file was not touched"
    );
    assert_no_temp_files(out.path());
}

#[test]
fn of_two_sources_that_fail_the_one_met_first_in_the_output_is_named() {
    let dir = TempDir::new().expect("creating a temporary folder");
    let first_path = put(dir.path(), "shapes.pdf", "added-first.pdf");
    let second_path = put(dir.path(), "shapes.pdf", "added-second.pdf");
    let state = state();
    let [added_first, added_second] = add(&state, &[&first_path, &second_path])
        .try_into()
        .expect("two PDFs were added");
    fs::copy(fixture("corrupt.pdf"), &first_path).expect("breaking a file");
    fs::copy(fixture("corrupt.pdf"), &second_path).expect("breaking a file");
    let out = TempDir::new().expect("creating a temporary folder");
    let dest = out.path().join("never.pdf");

    // The PDF added second comes first in the output, so the worker opens it first.
    let (saved, recorder) = save_single(
        &state,
        &[page(&added_second, 1, 0), page(&added_first, 1, 0)],
        None,
        &dest,
    );

    assert_eq!(saved, None);
    let items = recorder.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source_id, Some(added_second.id));
}

#[test]
fn a_sink_that_fails_stops_the_worker_and_is_reported_as_a_write_failure() {
    struct FailingSink;
    impl io::Write for FailingSink {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("the disk is full"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let dir = TempDir::new().unwrap();
    let source = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let pdf = add_one(&state, &source);
    let (_guard, output) =
        begin_single(&state, &[page(&pdf, 1, 0)], None).expect("accepting the pages");

    let stopped = output
        .write_to(&state, &mut FailingSink)
        .expect_err("the sink fails");

    match stopped {
        OutputStop::Failed {
            error,
            source_id,
            writing,
        } => {
            assert_eq!(error.code, ErrorCode::WriteFailed);
            assert_eq!(source_id, None);
            assert!(writing, "a failed write ends the job");
        }
        OutputStop::Cancelled => panic!("a failed write is not a cancel"),
    }
}

#[test]
fn a_sink_that_works_gets_the_whole_output() {
    let dir = TempDir::new().unwrap();
    let source = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let pdf = add_one(&state, &source);
    let (_guard, output) =
        begin_single(&state, &[page(&pdf, 1, 0), page(&pdf, 3, 90)], None).unwrap();
    let mut bytes: Vec<u8> = Vec::new();

    output.write_to(&state, &mut bytes).expect("writing");

    assert!(bytes.starts_with(b"%PDF-"));
    let copy = dir.path().join("copy.pdf");
    fs::write(&copy, &bytes).unwrap();
    assert_eq!(open_sizes(&state, &copy, None).unwrap().len(), 2);
}

// ---- A worker that says something it should not ----

/// A worker that is a shell script saying one canned message, to see how the main process takes
/// an answer a real worker does not give.
#[cfg(unix)]
fn fake_worker_config(answer: &Response) -> (WorkerPoolConfig, TempDir) {
    let dir = TempDir::new().expect("creating a temporary folder");
    let message = dir.path().join("answer.bin");
    let mut bytes = Vec::new();
    pdfconv_worker::protocol::write_message(&mut bytes, answer, &[]).expect("encoding");
    fs::write(&message, bytes).expect("writing the canned answer");
    let script = format!("cat '{}'; sleep 30", message.display());
    let config = WorkerPoolConfig::new("sh", ["-c".to_owned(), script]).with_max_workers(1);
    (config, dir)
}

#[cfg(unix)]
fn write_with_fake_worker(answer: &Response) -> Result<Vec<u8>, OutputStop> {
    let (config, _dir) = fake_worker_config(answer);
    let fake = state_with(config);
    // The pages come from a real list, so the request is a valid one.
    let real = state();
    let source_dir = TempDir::new().expect("creating a temporary folder");
    let pdf = add_one(&real, &put(source_dir.path(), "shapes.pdf", "shapes.pdf"));
    let (_guard, output) =
        begin_single(&real, &[page(&pdf, 1, 0)], None).expect("accepting the pages");
    let mut sink = Vec::new();
    output.write_to(&fake, &mut sink).map(|()| sink)
}

#[cfg(unix)]
#[test]
fn a_worker_whose_byte_count_does_not_match_what_arrived_is_a_failed_output() {
    // It says 5 bytes were sent, and none were.
    match write_with_fake_worker(&Response::Organized { bytes: 5 }) {
        Err(OutputStop::Failed { error, writing, .. }) => {
            assert_eq!(error.code, ErrorCode::WorkerCrashed);
            let detail = error.detail.expect("the two numbers are in the detail");
            assert!(detail.contains('5') && detail.contains('0'), "{detail}");
            assert!(!writing);
        }
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn a_worker_that_answers_with_the_wrong_kind_of_message_is_a_failed_output() {
    match write_with_fake_worker(&Response::Hello) {
        Err(OutputStop::Failed { error, writing, .. }) => {
            assert_eq!(error.code, ErrorCode::WorkerCrashed);
            assert!(!writing);
        }
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn a_worker_that_matches_its_count_is_taken() {
    assert_eq!(
        write_with_fake_worker(&Response::Organized { bytes: 0 })
            .expect("nothing announced and nothing written"),
        Vec::<u8>::new()
    );
}

// ---- Saving in parts ----

/// Three PDFs of the organize list in a folder of their own, named for the names of the outputs.
struct Parts {
    _dir: TempDir,
    shapes: OrganizePdf,
    mixed: OrganizePdf,
    links: OrganizePdf,
    meta: OrganizePdf,
}

fn parts_fixtures(state: &AppState) -> Parts {
    let dir = TempDir::new().expect("creating a temporary folder");
    let shapes = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let mixed = put(dir.path(), "mixed_sizes.pdf", "mixed_sizes.pdf");
    let links = put(dir.path(), "organize_links.pdf", "organize_links.pdf");
    let meta = put(dir.path(), "organize_meta.pdf", "organize_meta.pdf");
    let [shapes, mixed, links, meta] = add(state, &[&shapes, &mixed, &links, &meta])
        .try_into()
        .expect("four PDFs were added");
    Parts {
        _dir: dir,
        shapes,
        mixed,
        links,
        meta,
    }
}

#[test]
fn each_part_becomes_a_pdf_named_for_the_pdf_of_its_first_page() {
    let state = state();
    let f = parts_fixtures(&state);
    let out = TempDir::new().unwrap();

    let (recorder, finished) = save_split(
        &state,
        &[
            vec![page(&f.shapes, 1, 0), page(&f.mixed, 2, 0)],
            vec![page(&f.mixed, 1, 90)],
            vec![
                page(&f.mixed, 3, 0),
                page(&f.shapes, 3, 0),
                page(&f.shapes, 2, 180),
            ],
        ],
        out.path(),
    );

    assert_eq!(
        finished,
        JobFinishedPayload {
            succeeded: 3,
            failed: 0,
            no_pages: 0,
            unprocessed: 0,
            cancelled: false,
        }
    );
    assert_eq!(
        names_in(out.path()),
        ["mixed_sizes_2.pdf", "mixed_sizes_3.pdf", "shapes_1.pdf"]
    );
    let items = recorder.items();
    assert_eq!(items.iter().map(|i| i.id).collect::<Vec<_>>(), [1, 2, 3]);
    assert!(
        items
            .iter()
            .all(|i| i.status == JobItemStatus::Ok && i.error.is_none())
    );
    assert_eq!(items[0].outputs, ["shapes_1.pdf"]);
    assert_eq!(items[1].outputs, ["mixed_sizes_2.pdf"]);
    assert_eq!(items[2].outputs, ["mixed_sizes_3.pdf"]);
    assert_sizes(
        &open_sizes(&state, &out.path().join("shapes_1.pdf"), None).unwrap(),
        &[sizes(&f.shapes)[0], sizes(&f.mixed)[1]],
    );
    assert_sizes(
        &open_sizes(&state, &out.path().join("mixed_sizes_2.pdf"), None).unwrap(),
        &[turned(sizes(&f.mixed)[0])],
    );
    assert_sizes(
        &open_sizes(&state, &out.path().join("mixed_sizes_3.pdf"), None).unwrap(),
        &[
            sizes(&f.mixed)[2],
            sizes(&f.shapes)[2],
            // A half turn keeps the width and the height.
            sizes(&f.shapes)[1],
        ],
    );
    // The progress counts the pages of all the outputs.
    let progress = recorder.progress();
    assert_eq!(progress.first().map(|p| (p.done, p.total)), Some((0, 6)));
    assert_eq!(progress.last().map(|p| (p.done, p.total)), Some((6, 6)));
    assert!(progress.windows(2).all(|pair| pair[0].done <= pair[1].done));
    assert_eq!(recorder.running_at_finished(), Some(false));
    assert_no_temp_files(out.path());
}

#[test]
fn ten_parts_are_numbered_with_two_digits() {
    let state = state();
    let f = parts_fixtures(&state);
    let out = TempDir::new().unwrap();
    let mut parts = Vec::new();
    for number in 1..=4 {
        parts.push(vec![page(&f.links, number, 0)]);
    }
    for number in 1..=3 {
        parts.push(vec![page(&f.meta, number, 0)]);
    }
    for number in 1..=3 {
        parts.push(vec![page(&f.shapes, number, 0)]);
    }

    let (recorder, finished) = save_split(&state, &parts, out.path());

    assert_eq!(finished.succeeded, 10);
    let outputs: Vec<String> = recorder
        .items()
        .iter()
        .flat_map(|item| item.outputs.clone())
        .collect();
    assert_eq!(
        outputs,
        [
            "organize_links_01.pdf",
            "organize_links_02.pdf",
            "organize_links_03.pdf",
            "organize_links_04.pdf",
            "organize_meta_05.pdf",
            "organize_meta_06.pdf",
            "organize_meta_07.pdf",
            "shapes_08.pdf",
            "shapes_09.pdf",
            "shapes_10.pdf",
        ]
    );
    for name in &outputs {
        assert!(out.path().join(name).is_file(), "{name}");
    }
}

#[test]
fn a_name_that_is_taken_gets_a_number_whatever_its_case() {
    let state = state();
    let f = parts_fixtures(&state);
    let out = TempDir::new().unwrap();
    fs::write(out.path().join("shapes_1.pdf"), b"mine").unwrap();
    fs::write(out.path().join("SHAPES_2.PDF"), b"mine too").unwrap();

    let (recorder, finished) = save_split(
        &state,
        &[vec![page(&f.shapes, 1, 0)], vec![page(&f.shapes, 2, 0)]],
        out.path(),
    );

    assert_eq!(finished.succeeded, 2);
    let outputs: Vec<String> = recorder
        .items()
        .iter()
        .flat_map(|i| i.outputs.clone())
        .collect();
    assert_eq!(outputs, ["shapes_1 (1).pdf", "shapes_2 (1).pdf"]);
    assert_eq!(fs::read(out.path().join("shapes_1.pdf")).unwrap(), b"mine");
    assert_eq!(
        fs::read(out.path().join("SHAPES_2.PDF")).unwrap(),
        b"mine too"
    );
}

#[test]
fn a_name_taken_while_the_output_is_made_is_not_overwritten() {
    let state = state();
    let f = parts_fixtures(&state);
    let out = TempDir::new().unwrap();
    let taken = out.path().join("shapes_1.pdf");
    let hook_path = taken.clone();

    let (recorder, finished) = save_split_with(
        &state,
        &[vec![page(&f.shapes, 1, 0)]],
        out.path(),
        move |progress| {
            // The first notice comes before the output is made; the name is free until then.
            if progress.done == 0 {
                fs::write(&hook_path, b"someone else's").unwrap();
            }
        },
    );

    assert_eq!(finished.succeeded, 1);
    assert_eq!(recorder.items()[0].outputs, ["shapes_1 (1).pdf"]);
    assert_eq!(fs::read(&taken).unwrap(), b"someone else's");
    assert!(out.path().join("shapes_1 (1).pdf").is_file());
    assert_no_temp_files(out.path());
}

#[test]
fn a_source_that_fails_fails_its_outputs_and_the_others_go_on() {
    let state = state();
    let f = parts_fixtures(&state);
    let dir = TempDir::new().unwrap();
    let broken_path = put(dir.path(), "shapes.pdf", "broken.pdf");
    let broken = add_one(&state, &broken_path);
    fs::copy(fixture("corrupt.pdf"), &broken_path).unwrap();
    let out = TempDir::new().unwrap();

    let (recorder, finished) = save_split(
        &state,
        &[
            vec![page(&f.mixed, 1, 0)],
            vec![page(&f.mixed, 2, 0), page(&broken, 1, 0)],
            vec![page(&f.mixed, 3, 0)],
        ],
        out.path(),
    );

    let items = recorder.items();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].status, JobItemStatus::Ok);
    assert_eq!(items[1].status, JobItemStatus::Failed);
    assert_eq!(error_code(&items[1].error), Some(ErrorCode::PdfOpenFailed));
    assert_eq!(items[1].source_id, Some(broken.id));
    assert!(items[1].outputs.is_empty());
    assert_eq!(items[2].status, JobItemStatus::Ok);
    assert_eq!(items[2].source_id, None);
    assert_eq!(
        finished,
        JobFinishedPayload {
            succeeded: 2,
            failed: 1,
            no_pages: 0,
            unprocessed: 0,
            cancelled: false,
        }
    );
    assert_eq!(
        names_in(out.path()),
        ["mixed_sizes_1.pdf", "mixed_sizes_3.pdf"]
    );
    assert_no_temp_files(out.path());
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_source_that_failed_is_not_tried_again_by_a_later_output() {
    // The step timeout is long enough for a worker to start and short enough to end the wait for
    // a worker that is hung, if the output were sent to one.
    let state = state_with(config().with_organize_step_timeout(Duration::from_secs(3)));
    let dir = TempDir::new().unwrap();
    let broken_path = put(dir.path(), "shapes.pdf", "broken.pdf");
    let good_path = put(dir.path(), "shapes.pdf", "good.pdf");
    // A worker given this name stops answering: if the third output reaches a worker, the error
    // is a timeout, not the failure that was remembered.
    let hang_path = put(dir.path(), "shapes.pdf", HANG_ON_ORGANIZE_FILE_NAME);
    let [broken, good, hang] = add(&state, &[&broken_path, &good_path, &hang_path])
        .try_into()
        .unwrap();
    fs::copy(fixture("corrupt.pdf"), &broken_path).unwrap();
    let out = TempDir::new().unwrap();

    let (recorder, finished) = save_split(
        &state,
        &[
            vec![page(&broken, 1, 0)],
            vec![page(&good, 1, 0)],
            vec![page(&broken, 2, 0), page(&hang, 1, 0)],
        ],
        out.path(),
    );

    let items = recorder.items();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].status, JobItemStatus::Failed);
    assert_eq!(items[1].status, JobItemStatus::Ok);
    assert_eq!(items[2].status, JobItemStatus::Failed);
    assert_eq!(error_code(&items[2].error), Some(ErrorCode::PdfOpenFailed));
    assert_eq!(items[2].source_id, Some(broken.id));
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 2);
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_worker_that_crashes_fails_its_output_and_the_next_one_is_made() {
    let state = state();
    let dir = TempDir::new().unwrap();
    let crash_path = put(dir.path(), "shapes.pdf", CRASH_ON_ORGANIZE_FILE_NAME);
    let fine_path = put(dir.path(), "shapes.pdf", "fine.pdf");
    let [crash, fine] = add(&state, &[&crash_path, &fine_path]).try_into().unwrap();
    let out = TempDir::new().unwrap();

    let (recorder, finished) = save_split(
        &state,
        &[
            vec![page(&crash, 1, 0)],
            vec![page(&fine, 1, 0)],
            vec![page(&fine, 2, 0)],
        ],
        out.path(),
    );

    let items = recorder.items();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].status, JobItemStatus::Failed);
    assert_eq!(error_code(&items[0].error), Some(ErrorCode::WorkerCrashed));
    assert_eq!(items[0].source_id, None);
    assert_eq!(items[1].status, JobItemStatus::Ok);
    assert_eq!(items[2].status, JobItemStatus::Ok);
    assert_eq!(finished.succeeded, 2);
    assert_eq!(finished.failed, 1);
    assert_eq!(finished.unprocessed, 0);
    assert_no_temp_files(out.path());
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_worker_that_stops_answering_times_out_its_output_and_the_next_one_is_made() {
    let state = state_with(config().with_organize_step_timeout(Duration::from_secs(3)));
    let dir = TempDir::new().unwrap();
    let hang_path = put(dir.path(), "shapes.pdf", HANG_ON_ORGANIZE_FILE_NAME);
    let fine_path = put(dir.path(), "shapes.pdf", "fine.pdf");
    let [hang, fine] = add(&state, &[&hang_path, &fine_path]).try_into().unwrap();
    let out = TempDir::new().unwrap();

    let (recorder, finished) = save_split(
        &state,
        &[vec![page(&hang, 1, 0)], vec![page(&fine, 1, 0)]],
        out.path(),
    );

    let items = recorder.items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].status, JobItemStatus::Failed);
    assert_eq!(error_code(&items[0].error), Some(ErrorCode::WorkerTimeout));
    assert_eq!(items[1].status, JobItemStatus::Ok);
    assert_eq!(finished.succeeded, 1);
    assert_eq!(finished.failed, 1);
    assert_no_temp_files(out.path());
}

#[test]
fn cancelling_before_the_first_output_makes_none_and_counts_them_all_as_unprocessed() {
    let state = state();
    let f = parts_fixtures(&state);
    let out = TempDir::new().unwrap();
    let cancel = Arc::clone(&state.cancel_flag);
    let set_at: Arc<Mutex<Option<Instant>>> = Arc::default();
    let set_at_hook = Arc::clone(&set_at);

    let (recorder, finished) = save_split_with(
        &state,
        &[
            vec![page(&f.shapes, 1, 0)],
            vec![page(&f.shapes, 2, 0)],
            vec![page(&f.shapes, 3, 0)],
        ],
        out.path(),
        move |progress| {
            if progress.done == 0 {
                *set_at_hook.lock().unwrap() = Some(Instant::now());
                cancel.store(true, Ordering::SeqCst);
            }
        },
    );

    assert!(recorder.items().is_empty());
    assert_eq!(
        finished,
        JobFinishedPayload {
            succeeded: 0,
            failed: 0,
            no_pages: 0,
            unprocessed: 3,
            cancelled: true,
        }
    );
    let waited = recorder.finished_at() - set_at.lock().unwrap().expect("the flag was set");
    assert!(waited < Duration::from_secs(1), "{waited:?}");
    assert!(names_in(out.path()).is_empty());
    assert_eq!(recorder.running_at_finished(), Some(false));
}

#[test]
fn a_cancel_is_taken_before_waiting_for_a_worker_for_the_next_output() {
    // One worker, and the test holds it: an output that went to the pool would wait for ever.
    let state = state_with(config().with_max_workers(1));
    let f = parts_fixtures(&state);
    let out = TempDir::new().expect("creating a temporary folder");
    let held = state.pool.acquire().expect("borrowing the only worker");
    *state
        .organize_output_dir
        .lock()
        .expect("the folder is not poisoned") = Some(out.path().to_path_buf());
    let cancel = Arc::clone(&state.cancel_flag);
    let recorder = Recorder::default();

    start_organize_split_internal(
        &state,
        &[vec![page(&f.shapes, 1, 0)], vec![page(&f.shapes, 2, 0)]],
        None,
        recorder.callbacks(&state, move |progress| {
            if progress.done == 0 {
                cancel.store(true, Ordering::SeqCst);
            }
        }),
    )
    .expect("the parts should be accepted");
    let finished = recorder.wait_finished(Duration::from_secs(10));
    drop(held);

    assert!(finished.cancelled);
    assert_eq!(finished.unprocessed, 2);
    assert!(recorder.items().is_empty());
}

#[cfg(feature = "test-hooks")]
#[test]
fn cancelling_an_output_that_is_being_made_stops_the_worker_within_a_second() {
    let state = state();
    let dir = TempDir::new().unwrap();
    let hang_path = put(dir.path(), "shapes.pdf", HANG_ON_ORGANIZE_FILE_NAME);
    let fine_path = put(dir.path(), "shapes.pdf", "fine.pdf");
    let [hang, fine] = add(&state, &[&hang_path, &fine_path]).try_into().unwrap();
    let out = TempDir::new().unwrap();
    let cancel = Arc::clone(&state.cancel_flag);
    let set_at: Arc<Mutex<Option<Instant>>> = Arc::default();
    let set_at_hook = Arc::clone(&set_at);

    let (recorder, finished) = save_split_with(
        &state,
        &[vec![page(&hang, 1, 0)], vec![page(&fine, 1, 0)]],
        out.path(),
        move |progress| {
            if progress.done == 0 {
                // The first output is under way, and hung, when this fires.
                let cancel = Arc::clone(&cancel);
                let set_at = Arc::clone(&set_at_hook);
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(500));
                    *set_at.lock().unwrap() = Some(Instant::now());
                    cancel.store(true, Ordering::SeqCst);
                });
            }
        },
    );

    assert!(recorder.items().is_empty(), "{:?}", recorder.items());
    assert_eq!(
        finished,
        JobFinishedPayload {
            succeeded: 0,
            failed: 0,
            no_pages: 0,
            unprocessed: 2,
            cancelled: true,
        }
    );
    let waited = recorder.finished_at() - set_at.lock().unwrap().expect("the flag was set");
    assert!(waited < Duration::from_secs(1), "{waited:?}");
    assert!(
        names_in(out.path()).is_empty(),
        "{:?}",
        names_in(out.path())
    );
}

#[cfg(feature = "test-hooks")]
#[test]
fn cancelling_the_one_pdf_leaves_the_destination_alone() {
    let state = state();
    let dir = TempDir::new().unwrap();
    let hang_path = put(dir.path(), "shapes.pdf", HANG_ON_ORGANIZE_FILE_NAME);
    let hang = add_one(&state, &hang_path);
    let out = TempDir::new().unwrap();
    let dest = out.path().join("kept.pdf");
    fs::write(&dest, b"kept").unwrap();
    let (_guard, output) = begin_single(&state, &[page(&hang, 1, 0)], None).unwrap();
    let cancel = Arc::clone(&state.cancel_flag);
    let recorder = Recorder::default();
    let callbacks = recorder.callbacks(&state, move |progress| {
        if progress.done == 0 {
            let cancel = Arc::clone(&cancel);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(500));
                cancel.store(true, Ordering::SeqCst);
            });
        }
    });

    let result = run_save_organized_pdf(&state, &output, &dest, callbacks).expect("saving");

    assert_eq!(result.saved_name, None);
    assert!(recorder.items().is_empty());
    assert_eq!(
        recorder.finished(),
        JobFinishedPayload {
            succeeded: 0,
            failed: 0,
            no_pages: 0,
            unprocessed: 1,
            cancelled: true,
        }
    );
    assert_eq!(fs::read(&dest).unwrap(), b"kept");
    assert_no_temp_files(out.path());
}

#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_written_ends_the_job_and_leaves_the_rest_unprocessed() {
    use std::os::unix::fs::PermissionsExt;

    let state = state();
    let f = parts_fixtures(&state);
    let out = TempDir::new().unwrap();
    let out_path = out.path().to_path_buf();

    let (recorder, finished) = save_split_with(
        &state,
        &[
            vec![page(&f.shapes, 1, 0)],
            vec![page(&f.shapes, 2, 0)],
            vec![page(&f.shapes, 3, 0)],
        ],
        out.path(),
        move |progress| {
            // The folder passed the check when the job started; it is taken away now.
            if progress.done == 0 {
                fs::set_permissions(&out_path, fs::Permissions::from_mode(0o555)).unwrap();
            }
        },
    );

    fs::set_permissions(out.path(), fs::Permissions::from_mode(0o755)).unwrap();
    if recorder
        .items()
        .iter()
        .any(|i| i.status == JobItemStatus::Ok)
    {
        eprintln!("skipped: writing worked in a folder whose mode forbids it");
        return;
    }
    let items = recorder.items();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0].id, 1);
    assert_eq!(items[0].status, JobItemStatus::Failed);
    assert_eq!(error_code(&items[0].error), Some(ErrorCode::WriteFailed));
    assert_eq!(items[0].source_id, None);
    assert_eq!(
        finished,
        JobFinishedPayload {
            succeeded: 0,
            failed: 1,
            no_pages: 0,
            unprocessed: 2,
            cancelled: false,
        }
    );
    assert_eq!(recorder.running_at_finished(), Some(false));
    assert!(names_in(out.path()).is_empty());
}

#[test]
fn a_job_whose_outputs_all_fail_still_reports_its_end() {
    let state = state();
    let dir = TempDir::new().unwrap();
    let path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let pdf = add_one(&state, &path);
    fs::copy(fixture("corrupt.pdf"), &path).unwrap();
    let out = TempDir::new().unwrap();

    let (recorder, finished) = save_split(
        &state,
        &[vec![page(&pdf, 1, 0)], vec![page(&pdf, 2, 0)]],
        out.path(),
    );

    assert_eq!(finished.failed, 2);
    assert_eq!(finished.succeeded, 0);
    assert_eq!(recorder.items().len(), 2);
    assert!(names_in(out.path()).is_empty());
}
