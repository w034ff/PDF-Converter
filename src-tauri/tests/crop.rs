//! Integration tests for the crop tab's PDF and its questions (design §5.2,
//! §6.10, §7.1): the pending and the open PDF, the separation from the lists,
//! what a drop opens, the questions about a page, the analysis worker and
//! passwords. They use the real worker, like `items.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pdf_converter_lib::AppState;
use pdf_converter_lib::crop::{
    Candidate, CandidateKind, CropDropped, CropPdf, DetectAllCallbacks, DropTarget, Dropped,
    LayoutPagePayload, activate_crop_pdf, check_on_top, close_crop_pdf, crop_dropped,
    detect_layout, discard_crop_pdf, handle_drop, list_page_images, open_pdf,
    start_detect_all_internal, unlock_crop_pdf,
};
use pdf_converter_lib::error::{ErrorCode, IpcError};
use pdf_converter_lib::items::{add_images, add_pdfs, make_thumbnail, remove_items, unlock_pdf};
use pdf_converter_lib::jobs::{
    JobCallbacks, JobFinishedPayload, JobProgressPayload, RenderColorChoice, RenderFormatChoice,
    check_page_range_internal, start_pdfs_to_images_internal,
};
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig};
use pdfconv_core::THUMBNAIL_SIDE;
use pdfconv_worker::{PAGE_VIEW_MAX_SIDE, WORKER_FLAG};

#[path = "../../crates/core/tests/support/fixture_passwords.rs"]
mod fixture_passwords;
use fixture_passwords::{ENCRYPTED_USER_PASSWORD, OWNER_PASSWORD};

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

/// Where nothing is, for a layout file that is not there.
fn missing(name: &str) -> String {
    std::env::temp_dir()
        .join("pdfconv-no-layout-here")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn config_with(layout: [String; 2]) -> WorkerPoolConfig {
    WorkerPoolConfig::new(
        env!("CARGO_BIN_EXE_pdf-converter"),
        [
            WORKER_FLAG.to_string(),
            library_dir().to_string_lossy().into_owned(),
        ]
        .into_iter()
        .chain(layout),
    )
    .with_max_workers(2)
    .with_open_timeout(Duration::from_secs(30))
    .with_render_timeout(Duration::from_secs(60))
}

fn state_with(config: WorkerPoolConfig) -> AppState {
    AppState::new(WorkerPool::new(config))
}

fn state() -> AppState {
    state_with(config_with(layout_args()))
}

/// A state whose pool cannot start a worker, over the crop PDFs of `state`. A
/// question that is refused before it reaches a worker still gets its own
/// error from it, where one that reaches a worker gets `WorkerCrashed`.
fn without_workers(state: &AppState) -> AppState {
    let mut broken = state.clone();
    broken.pool = WorkerPool::new(WorkerPoolConfig::new(
        missing("no-such-worker"),
        Vec::<String>::new(),
    ));
    broken
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

fn open(state: &AppState, name: &str) -> CropPdf {
    open_pdf(state, &fixture(name))
}

/// Opens the fixture `name` and makes it the open PDF.
fn open_active(state: &AppState, name: &str) -> CropPdf {
    let pdf = open(state, name);
    activate_crop_pdf(state, pdf.id).expect("activating the pending PDF");
    pdf
}

fn code<T: std::fmt::Debug>(result: Result<T, IpcError>) -> ErrorCode {
    result.expect_err("the call should fail").code
}

fn error_code(error: &Option<IpcError>) -> Option<ErrorCode> {
    error.as_ref().map(|error| error.code)
}

/// The width and height in the header of a PNG.
fn png_size(png: &[u8]) -> (u32, u32) {
    let read = |range: std::ops::Range<usize>| {
        u32::from_be_bytes(png[range].try_into().expect("four bytes"))
    };
    (read(16..20), read(20..24))
}

fn longer_side(png: &[u8]) -> u32 {
    let (width, height) = png_size(png);
    width.max(height)
}

/// Candidates that are a photo, and those that are a chart or a figure.
fn photos(candidates: &[Candidate]) -> Vec<&Candidate> {
    candidates
        .iter()
        .filter(|c| c.kind == CandidateKind::Photo)
        .collect()
}

fn charts(candidates: &[Candidate]) -> Vec<&Candidate> {
    candidates
        .iter()
        .filter(|c| matches!(c.kind, CandidateKind::Chart | CandidateKind::Figure))
        .collect()
}

#[cfg(feature = "test-hooks")]
fn spawns(state: &AppState) -> usize {
    state.crop.analysis_spawn_count()
}

// ---- Pending and open ----

#[test]
fn opening_holds_the_pdf_and_leaves_the_open_one_alone() {
    let state = state();
    let first = open_active(&state, "crop_layout.pdf");
    assert_eq!(first.name, "crop_layout.pdf");
    assert_eq!(first.error, None);
    assert_eq!(first.page_count, 1);
    assert_eq!(first.pages.len(), 1);
    assert!(first.pages[0].width_pt > 0.0 && first.pages[0].height_pt > 0.0);

    let second = open(&state, "shapes.pdf");
    assert_eq!(second.error, None);
    assert!(second.page_count > 1);
    assert_eq!(
        second.pages.len(),
        usize::try_from(second.page_count).expect("a small count")
    );
    assert_ne!(second.id, first.id);

    // The first PDF is still the one that answers; the second only waits.
    assert_eq!(
        list_page_images(&state, first.id, 1)
            .expect("the open PDF answers")
            .len(),
        1
    );
    assert_eq!(
        code(list_page_images(&state, second.id, 1)),
        ErrorCode::UnknownHandle
    );
}

#[test]
fn activating_switches_to_the_pending_pdf() {
    let state = state();
    let first = open_active(&state, "crop_layout.pdf");
    let second = open(&state, "shapes.pdf");

    activate_crop_pdf(&state, second.id).expect("activating the pending PDF");

    assert!(list_page_images(&state, second.id, 1).is_ok());
    assert_eq!(
        code(list_page_images(&state, first.id, 1)),
        ErrorCode::UnknownHandle
    );
    // It is the open PDF now, not the pending one.
    assert_eq!(
        code(activate_crop_pdf(&state, second.id)),
        ErrorCode::UnknownHandle
    );
}

#[test]
fn only_the_pending_pdf_can_be_activated() {
    let state = state();
    let current = open_active(&state, "crop_layout.pdf");
    assert_eq!(
        code(activate_crop_pdf(&state, current.id)),
        ErrorCode::UnknownHandle
    );
    assert_eq!(
        code(activate_crop_pdf(&state, 99_999)),
        ErrorCode::UnknownHandle
    );
    assert!(list_page_images(&state, current.id, 1).is_ok());
}

#[test]
fn discarding_drops_the_pending_pdf_and_nothing_else() {
    let state = state();
    let current = open_active(&state, "crop_layout.pdf");
    let pending = open(&state, "shapes.pdf");

    // Not the pending PDF: nothing happens, and it is no error.
    discard_crop_pdf(&state, current.id);
    discard_crop_pdf(&state, 99_999);
    assert!(list_page_images(&state, current.id, 1).is_ok());

    discard_crop_pdf(&state, pending.id);
    assert_eq!(
        code(activate_crop_pdf(&state, pending.id)),
        ErrorCode::UnknownHandle
    );
    assert!(list_page_images(&state, current.id, 1).is_ok());
}

#[test]
fn a_new_pending_pdf_drops_the_one_before() {
    let state = state();
    let earlier = open(&state, "shapes.pdf");
    let later = open(&state, "crop_layout.pdf");

    assert_eq!(
        code(activate_crop_pdf(&state, earlier.id)),
        ErrorCode::UnknownHandle
    );
    activate_crop_pdf(&state, later.id).expect("activating the later one");
}

#[test]
fn closing_drops_the_open_and_the_pending_pdf() {
    let state = state();
    let current = open_active(&state, "crop_layout.pdf");
    let pending = open(&state, "shapes.pdf");

    close_crop_pdf(&state).expect("closing");

    assert_eq!(
        code(list_page_images(&state, current.id, 1)),
        ErrorCode::UnknownHandle
    );
    assert_eq!(
        code(activate_crop_pdf(&state, pending.id)),
        ErrorCode::UnknownHandle
    );
    // With nothing open, closing is no error.
    close_crop_pdf(&state).expect("closing again");
}

#[test]
fn a_conversion_blocks_switching_closing_and_unlocking() {
    let state = state();
    let current = open_active(&state, "crop_layout.pdf");
    let pending = open(&state, "shapes.pdf");

    state.is_running.store(true, Ordering::SeqCst);
    assert_eq!(
        code(activate_crop_pdf(&state, pending.id)),
        ErrorCode::ConversionRunning
    );
    assert_eq!(code(close_crop_pdf(&state)), ErrorCode::ConversionRunning);
    assert_eq!(
        code(unlock_crop_pdf(&state, current.id, "secret")),
        ErrorCode::ConversionRunning
    );
    state.is_running.store(false, Ordering::SeqCst);

    // The refusals changed nothing.
    assert!(list_page_images(&state, current.id, 1).is_ok());
    activate_crop_pdf(&state, pending.id).expect("the pending PDF is still there");
}

// ---- Apart from the lists ----

#[test]
fn a_file_in_a_list_can_be_opened_for_cropping_and_the_other_way() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "shapes.pdf", "doc.pdf");
    let state = state();

    let cropped = open_pdf(&state, &path);
    let listed = add_pdfs(&state, std::slice::from_ref(&path));
    assert_eq!(listed.skipped.duplicates, 0);
    assert_eq!(listed.added.len(), 1);
    assert_ne!(listed.added[0].id, cropped.id);

    let again = open_pdf(&state, &path);
    assert_eq!(again.error, None);
    // The crop PDFs are not in the table: only the list item is.
    assert_eq!(state.items.len(), 1);
    // The list still refuses a second copy.
    assert_eq!(
        add_pdfs(&state, std::slice::from_ref(&path))
            .skipped
            .duplicates,
        1
    );
}

#[test]
fn crop_and_list_ids_do_not_overlap() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let first = put(dir.path(), "shapes.pdf", "a.pdf");
    let second = put(dir.path(), "shapes.pdf", "b.pdf");
    let state = state();

    let listed_a = add_pdfs(&state, std::slice::from_ref(&first)).added[0].id;
    let cropped = open_pdf(&state, &first).id;
    let listed_b = add_pdfs(&state, std::slice::from_ref(&second)).added[0].id;

    let mut ids = [listed_a, cropped, listed_b];
    ids.sort_unstable();
    assert!(ids[0] < ids[1] && ids[1] < ids[2], "{ids:?}");
}

#[test]
fn the_commands_of_the_lists_do_not_know_a_crop_pdf() {
    let state = state();
    let cropped = open_active(&state, "crop_layout.pdf");

    assert_eq!(
        code(unlock_pdf(&state, cropped.id, "secret")),
        ErrorCode::UnknownHandle
    );

    *state.pdfs_output_dir.lock().expect("output folder lock") =
        Some(tempfile::tempdir().expect("a folder").keep());
    let started = start_pdfs_to_images_internal(
        &state,
        &[cropped.id],
        "1",
        RenderFormatChoice::Png,
        RenderColorChoice::Color,
        150,
        JobCallbacks {
            on_progress: |_| {},
            on_item: |_| {},
            on_finished: |_| {},
        },
    );
    assert_eq!(code(started), ErrorCode::UnknownHandle);
    assert!(!state.is_running.load(Ordering::SeqCst));

    // These two ignore every ID they do not know, so the crop PDF counts for
    // nothing and is left alone.
    let counted = check_page_range_internal(&state, "1", &[cropped.id]).expect("a valid range");
    assert_eq!(counted.total_pages, 0);
    remove_items(&state, &[cropped.id]).expect("removing nothing");
    assert!(list_page_images(&state, cropped.id, 1).is_ok());
}

#[test]
fn the_questions_of_the_crop_tab_do_not_know_a_list_item() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "crop_layout.pdf", "listed.pdf");
    let state = state();
    open_active(&state, "crop_layout.pdf");
    let listed = add_pdfs(&state, &[path]).added[0].id;

    assert_eq!(
        code(list_page_images(&state, listed, 1)),
        ErrorCode::UnknownHandle
    );
    assert_eq!(
        code(check_on_top(&state, listed, 1, 0)),
        ErrorCode::UnknownHandle
    );
    assert_eq!(
        code(detect_layout(&state, listed, 1)),
        ErrorCode::UnknownHandle
    );
    assert_eq!(
        code(unlock_crop_pdf(&state, listed, "secret")),
        ErrorCode::UnknownHandle
    );
    // `get_thumbnail` is the one question that takes both kinds of ID.
    assert!(make_thumbnail(&state, listed, None, None).is_ok());
}

// ---- Drops ----

fn nothing_opened(dropped: &CropDropped) {
    assert_eq!(dropped.crop_pdf, None);
    assert_eq!(dropped.error, None);
}

#[test]
fn one_dropped_pdf_is_held_as_the_pending_pdf() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "crop_layout.pdf", "dropped.PDF");
    let state = state();

    let dropped = crop_dropped(&state, &[path]);

    assert!(!dropped.not_one_pdf);
    assert_eq!(dropped.error, None);
    let pdf = dropped.crop_pdf.expect("the PDF that was dropped");
    assert_eq!(pdf.name, "dropped.PDF");
    assert_eq!(pdf.page_count, 1);
    activate_crop_pdf(&state, pdf.id).expect("it is the pending PDF");
    assert!(state.items.is_empty());
}

#[test]
fn a_dropped_pdf_that_cannot_be_opened_is_held_with_its_error() {
    let state = state();
    let dropped = crop_dropped(&state, &[fixture("corrupt.pdf")]);

    let pdf = dropped.crop_pdf.expect("the PDF that was dropped");
    assert_eq!(error_code(&pdf.error), Some(ErrorCode::PdfOpenFailed));
    assert_eq!(pdf.page_count, 0);
    assert!(pdf.pages.is_empty());
}

#[test]
fn a_drop_that_is_not_one_pdf_opens_nothing() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let second_pdf = put(dir.path(), "shapes.pdf", "second.pdf");
    let image = put(dir.path(), "photo.jpg", "photo.jpg");
    let folder = dir.path().join("folder");
    fs::create_dir(&folder).expect("creating a folder");
    put(&folder, "shapes.pdf", "inside.pdf");
    let folder_named_pdf = dir.path().join("folder.pdf");
    fs::create_dir(&folder_named_pdf).expect("creating a folder");
    let text = dir.path().join("notes.txt");
    fs::write(&text, "text").expect("writing a file");

    let state = state();
    // A pending PDF that a drop which opens nothing must leave alone.
    let pending = open(&state, "crop_layout.pdf");

    let cases: [(&str, Vec<PathBuf>); 7] = [
        (
            "two PDFs",
            vec![fixture("crop_layout.pdf"), second_pdf.clone()],
        ),
        ("a PDF and an image", vec![second_pdf, image.clone()]),
        ("one image", vec![image]),
        ("one folder", vec![folder]),
        ("a folder named like a PDF", vec![folder_named_pdf]),
        ("a text file", vec![text]),
        ("nothing", Vec::new()),
    ];
    for (what, paths) in cases {
        let dropped = crop_dropped(&state, &paths);
        assert!(dropped.not_one_pdf, "{what}");
        nothing_opened(&dropped);
    }

    activate_crop_pdf(&state, pending.id).expect("the pending PDF was not replaced");
    assert!(state.items.is_empty());
}

#[test]
fn a_drop_during_a_conversion_opens_nothing() {
    let state = state();
    let pending = open(&state, "crop_layout.pdf");
    state.is_running.store(true, Ordering::SeqCst);

    let dropped = crop_dropped(&state, &[fixture("shapes.pdf")]);

    assert_eq!(dropped.crop_pdf, None);
    assert!(!dropped.not_one_pdf);
    assert_eq!(
        error_code(&dropped.error),
        Some(ErrorCode::ConversionRunning)
    );
    state.is_running.store(false, Ordering::SeqCst);
    activate_crop_pdf(&state, pending.id).expect("the pending PDF was not replaced");
}

#[test]
fn the_drop_target_decides_what_a_drop_becomes() {
    let state = state();
    let paths = [fixture("shapes.pdf")];

    assert_eq!(state.crop.drop_target(), DropTarget::Lists);
    let Dropped::Items(items) = handle_drop(&state, &paths) else {
        unreachable!("the lists take drops by default");
    };
    assert_eq!(items.pdfs.len(), 1);

    state.crop.set_drop_target(DropTarget::Crop);
    let Dropped::Crop(crop) = handle_drop(&state, &paths) else {
        unreachable!("the crop tab was told to take drops");
    };
    assert!(crop.crop_pdf.is_some());
    // The crop drop added nothing to the lists.
    assert_eq!(state.items.len(), 1);

    // A new page starts on the lists again.
    state.crop.start_page();
    assert_eq!(state.crop.drop_target(), DropTarget::Lists);
    assert!(matches!(handle_drop(&state, &paths), Dropped::Items(_)));
}

// ---- Questions about a page ----

#[test]
fn the_images_of_a_page_are_listed_with_their_frames() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");

    let images = list_page_images(&state, pdf.id, 1).expect("listing the images");

    assert_eq!(images.len(), 1, "{images:?}");
    let [x0, y0, x1, y1] = images[0].rect_pt;
    assert!(x0 < x1 && y0 < y1, "{:?}", images[0].rect_pt);
    assert!(images[0].px_width > 0 && images[0].px_height > 0);
}

#[test]
fn nothing_is_drawn_over_the_photo_of_crop_layout() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    let image = list_page_images(&state, pdf.id, 1).expect("listing the images")[0].image_index;

    assert!(!check_on_top(&state, pdf.id, 1, image).expect("checking the photo"));
    assert_eq!(
        code(check_on_top(&state, pdf.id, 1, image + 100)),
        ErrorCode::InvalidParams
    );
}

#[test]
fn the_thumbnail_follows_max_side() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    let thumbnail = |max_side| make_thumbnail(&state, pdf.id, None, max_side);

    assert_eq!(
        longer_side(&thumbnail(None).expect("the default size")),
        THUMBNAIL_SIDE
    );
    assert_eq!(
        longer_side(&thumbnail(Some(PAGE_VIEW_MAX_SIDE)).expect("the largest size")),
        PAGE_VIEW_MAX_SIDE
    );
    assert_eq!(longer_side(&thumbnail(Some(300)).expect("300")), 300);
    assert_eq!(code(thumbnail(Some(0))), ErrorCode::InvalidParams);
    assert_eq!(
        code(thumbnail(Some(PAGE_VIEW_MAX_SIDE + 1))),
        ErrorCode::InvalidParams
    );
}

#[test]
fn a_bad_max_side_is_refused_before_a_worker_is_asked() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    let broken = without_workers(&state);

    for max_side in [0, PAGE_VIEW_MAX_SIDE + 1] {
        assert_eq!(
            code(make_thumbnail(&broken, pdf.id, None, Some(max_side))),
            ErrorCode::InvalidParams,
            "{max_side}"
        );
    }
    // A good one does reach a worker, which this pool cannot start.
    assert_eq!(
        code(make_thumbnail(&broken, pdf.id, None, Some(100))),
        ErrorCode::WorkerCrashed
    );
}

#[test]
fn max_side_applies_to_a_pdf_in_a_list_and_is_refused_for_an_image() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let pdf_path = put(dir.path(), "crop_layout.pdf", "listed.pdf");
    let image_path = put(dir.path(), "photo.jpg", "photo.jpg");
    let state = state();
    let pdf = add_pdfs(&state, &[pdf_path]).added[0].id;
    let image = add_images(&state, &[image_path]).added[0].id;

    let png = make_thumbnail(&state, pdf, None, Some(300)).expect("a listed PDF");
    assert_eq!(longer_side(&png), 300);
    assert_eq!(
        code(make_thumbnail(&state, pdf, None, Some(0))),
        ErrorCode::InvalidParams
    );

    assert!(make_thumbnail(&state, image, None, None).is_ok());
    assert_eq!(
        code(make_thumbnail(&state, image, None, Some(THUMBNAIL_SIDE))),
        ErrorCode::InvalidParams
    );
}

#[test]
fn a_page_outside_the_pdf_is_refused_before_a_worker_is_asked() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    let broken = without_workers(&state);

    for page in [0, 2] {
        assert_eq!(
            code(list_page_images(&broken, pdf.id, page)),
            ErrorCode::InvalidParams,
            "list_page_images {page}"
        );
        assert_eq!(
            code(check_on_top(&broken, pdf.id, page, 0)),
            ErrorCode::InvalidParams,
            "check_on_top {page}"
        );
        assert_eq!(
            code(make_thumbnail(&broken, pdf.id, Some(page), None)),
            ErrorCode::InvalidParams,
            "get_thumbnail {page}"
        );
        assert_eq!(
            code(detect_layout(&state, pdf.id, page)),
            ErrorCode::InvalidParams,
            "detect_layout {page}"
        );
    }
    // The analysis worker is not started for a page that is not there.
    #[cfg(feature = "test-hooks")]
    assert_eq!(spawns(&state), 0);
}

#[test]
fn a_pdf_that_could_not_be_opened_answers_with_its_own_error() {
    let state = state();
    let pdf = open_active(&state, "corrupt.pdf");
    let expected = pdf.error.expect("a corrupt PDF cannot be opened");
    assert_eq!(pdf.page_count, 0);
    assert!(pdf.pages.is_empty());

    assert_eq!(list_page_images(&state, pdf.id, 1), Err(expected.clone()));
    assert_eq!(check_on_top(&state, pdf.id, 1, 0), Err(expected.clone()));
    assert_eq!(detect_layout(&state, pdf.id, 1), Err(expected.clone()));
    assert_eq!(make_thumbnail(&state, pdf.id, None, None), Err(expected));
}

// ---- The analysis worker ----

#[cfg(feature = "test-hooks")]
#[test]
fn the_first_detection_starts_one_worker_and_the_next_reuses_it() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    assert_eq!(spawns(&state), 0);

    let candidates = detect_layout(&state, pdf.id, 1).expect("detecting the layout");
    assert_eq!(spawns(&state), 1);
    let again = detect_layout(&state, pdf.id, 1).expect("detecting again");
    assert_eq!(spawns(&state), 1);
    assert_eq!(again, candidates);

    let images = list_page_images(&state, pdf.id, 1).expect("listing the images");
    let photos = photos(&candidates);
    assert_eq!(photos.len(), 1, "{candidates:?}");
    assert_eq!(photos[0].image_index, Some(images[0].image_index));
    assert_eq!(photos[0].rect_pt, images[0].rect_pt);
    let charts = charts(&candidates);
    assert_eq!(charts.len(), 1, "{candidates:?}");
    assert_eq!(charts[0].image_index, None);
    assert_eq!(candidates.len(), 2, "{candidates:?}");
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_crashed_analysis_worker_is_replaced_by_the_next_detection() {
    use pdfconv_worker::protocol::Request;

    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    let candidates = detect_layout(&state, pdf.id, 1).expect("detecting the layout");

    let crashed =
        state
            .crop
            .send_to_analysis_worker(pdf.id, &Request::CrashForTest, Duration::from_secs(10));
    assert_eq!(code(crashed), ErrorCode::WorkerCrashed);

    assert_eq!(detect_layout(&state, pdf.id, 1), Ok(candidates));
    assert_eq!(spawns(&state), 2);
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_hung_analysis_worker_times_out_and_is_replaced_by_the_next_detection() {
    use pdfconv_worker::protocol::Request;

    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    let candidates = detect_layout(&state, pdf.id, 1).expect("detecting the layout");

    let hung = state.crop.send_to_analysis_worker(
        pdf.id,
        &Request::HangForTest,
        Duration::from_millis(500),
    );
    assert_eq!(code(hung), ErrorCode::WorkerTimeout);

    assert_eq!(detect_layout(&state, pdf.id, 1), Ok(candidates));
    assert_eq!(spawns(&state), 2);
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_detection_that_takes_longer_than_the_detect_timeout_is_discarded() {
    let state =
        state_with(config_with(layout_args()).with_detect_timeout(Duration::from_millis(1)));
    let pdf = open_active(&state, "crop_layout.pdf");

    // Loading the model alone takes far longer than a millisecond.
    assert_eq!(
        code(detect_layout(&state, pdf.id, 1)),
        ErrorCode::WorkerTimeout
    );
    assert_eq!(
        code(detect_layout(&state, pdf.id, 1)),
        ErrorCode::WorkerTimeout
    );
    assert_eq!(spawns(&state), 2);
}

#[cfg(feature = "test-hooks")]
#[test]
fn layout_unavailable_is_remembered_until_the_pdf_is_opened_again() {
    let state = state_with(config_with([
        layout_args()[0].clone(),
        missing("model.onnx"),
    ]));
    let pdf = open_active(&state, "crop_layout.pdf");

    let first = detect_layout(&state, pdf.id, 1).expect_err("no model to load");
    assert_eq!(first.code, ErrorCode::LayoutUnavailable);
    assert!(first.detail.is_some());
    let second = detect_layout(&state, pdf.id, 1).expect_err("still no model");
    assert_eq!(second, first);
    assert_eq!(spawns(&state), 1);
    // The other questions do not depend on the model.
    assert_eq!(
        list_page_images(&state, pdf.id, 1)
            .expect("the pool answers")
            .len(),
        1
    );

    // Opening the PDF again starts afresh.
    let reopened = open_active(&state, "crop_layout.pdf");
    assert_eq!(
        code(detect_layout(&state, reopened.id, 1)),
        ErrorCode::LayoutUnavailable
    );
    assert_eq!(spawns(&state), 2);
}

#[cfg(feature = "test-hooks")]
#[test]
fn switching_pdfs_ends_the_analysis_of_the_one_before() {
    let state = state();
    let first = open_active(&state, "crop_layout.pdf");
    let before = detect_layout(&state, first.id, 1).expect("detecting the first PDF");

    let second = open_active(&state, "shapes.pdf");
    let after = detect_layout(&state, second.id, 1).expect("detecting the second PDF");

    assert_eq!(spawns(&state), 2);
    assert_ne!(after, before);
}

#[test]
fn a_detection_is_allowed_during_a_conversion() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    state.is_running.store(true, Ordering::SeqCst);

    let candidates = detect_layout(&state, pdf.id, 1).expect("detecting the layout");

    assert_eq!(photos(&candidates).len(), 1, "{candidates:?}");
    assert_eq!(charts(&candidates).len(), 1, "{candidates:?}");
}

/// Hangs the analysis worker of `id` for `timeout` on another thread, and
/// returns once it is waiting.
#[cfg(feature = "test-hooks")]
fn hang_in_background(
    state: &AppState,
    id: u64,
    timeout: Duration,
) -> std::thread::JoinHandle<Result<(), IpcError>> {
    use pdfconv_worker::protocol::Request;

    let state = state.clone();
    let handle = std::thread::spawn(move || {
        state
            .crop
            .send_to_analysis_worker(id, &Request::HangForTest, timeout)
    });
    std::thread::sleep(Duration::from_millis(500));
    handle
}

/// How long a switch or a close may take while an analysis is under way. The
/// analysis lasts much longer, so waiting for it would not fit.
#[cfg(feature = "test-hooks")]
const QUICK: Duration = Duration::from_secs(2);

#[cfg(feature = "test-hooks")]
const LONG_ANALYSIS: Duration = Duration::from_secs(5);

#[cfg(feature = "test-hooks")]
#[test]
fn switching_does_not_wait_for_an_analysis_under_way() {
    let state = state();
    let first = open_active(&state, "crop_layout.pdf");
    detect_layout(&state, first.id, 1).expect("starting the analysis worker");
    let analysis = hang_in_background(&state, first.id, LONG_ANALYSIS);

    let second = open(&state, "shapes.pdf");
    let started = std::time::Instant::now();
    activate_crop_pdf(&state, second.id).expect("switching");
    assert!(started.elapsed() < QUICK, "{:?}", started.elapsed());

    let ended = analysis.join().expect("the analysis thread");
    assert_eq!(code(ended), ErrorCode::WorkerTimeout);
}

#[cfg(feature = "test-hooks")]
#[test]
fn closing_does_not_wait_for_an_analysis_under_way() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    detect_layout(&state, pdf.id, 1).expect("starting the analysis worker");
    let analysis = hang_in_background(&state, pdf.id, LONG_ANALYSIS);

    let started = std::time::Instant::now();
    close_crop_pdf(&state).expect("closing");
    assert!(started.elapsed() < QUICK, "{:?}", started.elapsed());

    let ended = analysis.join().expect("the analysis thread");
    assert_eq!(code(ended), ErrorCode::WorkerTimeout);
}

// ---- Passwords ----

#[test]
fn a_password_protected_pdf_answers_with_its_error_until_it_is_unlocked() {
    let state = state();
    let pdf = open_active(&state, "encrypted.pdf");
    assert_eq!(error_code(&pdf.error), Some(ErrorCode::PasswordProtected));
    assert_eq!(pdf.page_count, 0);

    assert_eq!(
        code(list_page_images(&state, pdf.id, 1)),
        ErrorCode::PasswordProtected
    );
    assert_eq!(
        code(detect_layout(&state, pdf.id, 1)),
        ErrorCode::PasswordProtected
    );
    assert_eq!(
        code(make_thumbnail(&state, pdf.id, None, None)),
        ErrorCode::PasswordProtected
    );
    #[cfg(feature = "test-hooks")]
    assert_eq!(spawns(&state), 0);
}

#[test]
fn a_wrong_password_changes_nothing() {
    let state = state();
    let pdf = open_active(&state, "encrypted.pdf");

    assert_eq!(
        code(unlock_crop_pdf(&state, pdf.id, "not the password")),
        ErrorCode::PasswordProtected
    );
    assert_eq!(
        code(unlock_crop_pdf(&state, pdf.id, "with a \0 in it")),
        ErrorCode::PasswordProtected
    );
    assert_eq!(
        code(unlock_crop_pdf(&state, pdf.id, "")),
        ErrorCode::InvalidParams
    );
    assert_eq!(
        code(list_page_images(&state, pdf.id, 1)),
        ErrorCode::PasswordProtected
    );
}

#[test]
fn only_the_open_password_protected_pdf_can_be_unlocked() {
    let state = state();
    let current = open_active(&state, "crop_layout.pdf");
    let pending = open(&state, "encrypted.pdf");

    // Open, but not waiting for a password.
    assert_eq!(
        code(unlock_crop_pdf(&state, current.id, ENCRYPTED_USER_PASSWORD)),
        ErrorCode::InvalidParams
    );
    // Waiting for a password, but pending.
    assert_eq!(
        code(unlock_crop_pdf(&state, pending.id, ENCRYPTED_USER_PASSWORD)),
        ErrorCode::UnknownHandle
    );
    assert_eq!(
        code(unlock_crop_pdf(&state, 99_999, ENCRYPTED_USER_PASSWORD)),
        ErrorCode::UnknownHandle
    );
}

#[test]
fn the_right_password_opens_the_pdf_for_every_question() {
    let state = state();
    let locked = open_active(&state, "encrypted.pdf");

    let unlocked = unlock_crop_pdf(&state, locked.id, ENCRYPTED_USER_PASSWORD)
        .expect("the user password opens it");

    assert_eq!(unlocked.id, locked.id);
    assert_eq!(unlocked.name, "encrypted.pdf");
    assert_eq!(unlocked.error, None);
    assert!(unlocked.page_count > 0);
    assert_eq!(
        unlocked.pages.len(),
        usize::try_from(unlocked.page_count).expect("a small count")
    );
    assert!(list_page_images(&state, locked.id, 1).is_ok());
    assert!(make_thumbnail(&state, locked.id, None, None).is_ok());
    assert!(detect_layout(&state, locked.id, 1).is_ok());
}

#[test]
fn the_password_of_a_list_item_does_not_open_a_crop_pdf() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let listed_path = put(dir.path(), "encrypted.pdf", "listed.pdf");
    let state = state();
    let listed = add_pdfs(&state, &[listed_path]).added[0].id;
    let cropped = open_active(&state, "encrypted_cafe.pdf");

    // The owner password opens both files, so a list unlock that tried the
    // crop PDF would open it.
    unlock_pdf(&state, listed, OWNER_PASSWORD).expect("unlocking the list item");

    assert_eq!(
        code(list_page_images(&state, cropped.id, 1)),
        ErrorCode::PasswordProtected
    );
}

#[test]
fn the_password_of_a_crop_pdf_does_not_open_a_list_item() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let listed_path = put(dir.path(), "encrypted.pdf", "listed.pdf");
    let state = state();
    let listed = add_pdfs(&state, &[listed_path]).added[0].id;
    let cropped = open_active(&state, "encrypted_cafe.pdf");

    unlock_crop_pdf(&state, cropped.id, OWNER_PASSWORD).expect("unlocking the crop PDF");

    let item = state.items.get(listed).expect("the list item");
    assert_eq!(error_code(&item.error), Some(ErrorCode::PasswordProtected));
    assert!(item.password.is_none());
    assert!(list_page_images(&state, cropped.id, 1).is_ok());
}

// ---- Every page (`start_detect_all`) ----

/// What a finished `start_detect_all` reported.
struct Detected {
    pages: Vec<LayoutPagePayload>,
    progress: Vec<JobProgressPayload>,
    finished: JobFinishedPayload,
}

impl Detected {
    fn page_numbers(&self) -> Vec<u32> {
        self.pages.iter().map(|page| page.page).collect()
    }
}

const NOTHING_DONE: JobFinishedPayload = JobFinishedPayload {
    succeeded: 0,
    failed: 0,
    no_pages: 0,
    unprocessed: 0,
    cancelled: false,
};

/// Starts the analysis of every page and waits for its `job-finished`.
/// `after_page` runs on the job's thread after each `layout-page`.
fn detect_all_with(
    state: &AppState,
    id: u64,
    after_page: impl Fn(&LayoutPagePayload) + Send + 'static,
) -> Result<Detected, IpcError> {
    let pages = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(Mutex::new(Vec::new()));
    let finished = Arc::new(Mutex::new(None));
    let callbacks = DetectAllCallbacks {
        on_progress: {
            let progress = Arc::clone(&progress);
            move |payload| progress.lock().expect("progress lock").push(payload)
        },
        on_page: {
            let pages = Arc::clone(&pages);
            move |payload: LayoutPagePayload| {
                after_page(&payload);
                pages.lock().expect("pages lock").push(payload);
            }
        },
        on_finished: {
            let finished = Arc::clone(&finished);
            move |payload| *finished.lock().expect("finished lock") = Some(payload)
        },
    };
    start_detect_all_internal(state, id, callbacks)?;

    let started = Instant::now();
    let finished = loop {
        if let Some(payload) = finished.lock().expect("finished lock").clone() {
            break payload;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "the analysis did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        !state.is_running.load(Ordering::SeqCst),
        "is_running must be back when job-finished arrives"
    );
    let pages = pages.lock().expect("pages lock").clone();
    let progress = progress.lock().expect("progress lock").clone();
    Ok(Detected {
        pages,
        progress,
        finished,
    })
}

fn detect_all(state: &AppState, id: u64) -> Detected {
    detect_all_with(state, id, |_| {}).expect("starting")
}

#[test]
fn the_one_page_of_crop_layout_is_answered_with_its_photo_and_chart() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");

    let detected = detect_all(&state, pdf.id);

    assert_eq!(detected.pages.len(), 1);
    let page = &detected.pages[0];
    assert_eq!((page.id, page.page), (pdf.id, 1));
    assert_eq!(page.error, None);
    assert_eq!(photos(&page.candidates).len(), 1, "{:?}", page.candidates);
    assert_eq!(charts(&page.candidates).len(), 1, "{:?}", page.candidates);
    assert_eq!(
        detected.finished,
        JobFinishedPayload {
            succeeded: 1,
            ..NOTHING_DONE
        }
    );
    let done: Vec<u32> = detected.progress.iter().map(|p| p.done).collect();
    assert_eq!(done, [0, 1]);
    for progress in &detected.progress {
        assert_eq!(progress.total, 1);
        assert_eq!(progress.current.as_deref(), Some("crop_layout.pdf"));
    }
}

#[test]
fn the_pages_are_answered_in_page_order() {
    let state = state();
    let pdf = open_active(&state, "shapes.pdf");

    let detected = detect_all(&state, pdf.id);

    assert_eq!(detected.page_numbers(), [1, 2, 3]);
    assert!(detected.pages.iter().all(|page| page.error.is_none()));
    assert_eq!(
        detected.finished,
        JobFinishedPayload {
            succeeded: 3,
            ..NOTHING_DONE
        }
    );
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_page_that_detect_layout_answered_is_not_analyzed_again() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    detect_layout(&state, pdf.id, 1).expect("detecting the layout");
    assert_eq!(spawns(&state), 1);

    let detected = detect_all(&state, pdf.id);

    assert_eq!(detected.pages, []);
    assert_eq!(detected.progress, []);
    assert_eq!(detected.finished, NOTHING_DONE);
    assert_eq!(spawns(&state), 1, "no worker was asked");
}

#[cfg(feature = "test-hooks")]
#[test]
fn only_the_pages_that_were_not_answered_are_analyzed() {
    let state = state();
    let pdf = open_active(&state, "shapes.pdf");
    detect_layout(&state, pdf.id, 2).expect("detecting the layout");

    let first = detect_all(&state, pdf.id);

    assert_eq!(first.page_numbers(), [1, 3]);
    assert_eq!(
        first.progress.last().map(|p| (p.done, p.total)),
        Some((2, 2))
    );
    // Every page is answered now, by either.
    let second = detect_all(&state, pdf.id);
    assert_eq!(second.pages, []);
    assert_eq!(second.finished, NOTHING_DONE);
    assert_eq!(spawns(&state), 1);
}

#[test]
fn cancelling_after_a_page_leaves_the_rest_unprocessed() {
    let state = state();
    let pdf = open_active(&state, "shapes.pdf");
    let cancel = Arc::clone(&state.cancel_flag);

    let detected = detect_all_with(&state, pdf.id, move |_| {
        cancel.store(true, Ordering::SeqCst);
    })
    .expect("starting");

    assert_eq!(detected.page_numbers(), [1]);
    assert_eq!(
        detected.finished,
        JobFinishedPayload {
            succeeded: 1,
            unprocessed: 2,
            cancelled: true,
            ..NOTHING_DONE
        }
    );
}

#[test]
fn the_pages_a_cancelled_run_did_not_reach_are_analyzed_by_the_next() {
    let state = state();
    let pdf = open_active(&state, "shapes.pdf");
    let cancel = Arc::clone(&state.cancel_flag);
    detect_all_with(&state, pdf.id, move |_| {
        cancel.store(true, Ordering::SeqCst);
    })
    .expect("starting");

    let detected = detect_all(&state, pdf.id);

    assert_eq!(detected.page_numbers(), [2, 3]);
    assert!(!detected.finished.cancelled);
}

#[cfg(feature = "test-hooks")]
#[test]
fn without_the_model_the_first_page_says_so_and_the_run_ends() {
    let state = state_with(config_with([
        layout_args()[0].clone(),
        missing("model.onnx"),
    ]));
    let pdf = open_active(&state, "shapes.pdf");

    let detected = detect_all(&state, pdf.id);

    assert_eq!(detected.page_numbers(), [1]);
    let page = &detected.pages[0];
    assert_eq!(error_code(&page.error), Some(ErrorCode::LayoutUnavailable));
    assert_eq!(page.candidates, []);
    assert_eq!(
        detected.finished,
        JobFinishedPayload {
            failed: 1,
            unprocessed: 2,
            ..NOTHING_DONE
        }
    );
    assert_eq!(spawns(&state), 1);
    // A page that failed is not remembered as answered.
    assert_eq!(
        code(detect_layout(&state, pdf.id, 2)),
        ErrorCode::LayoutUnavailable
    );
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_page_that_times_out_has_its_error_and_the_run_goes_on_with_a_new_worker() {
    let state =
        state_with(config_with(layout_args()).with_detect_timeout(Duration::from_millis(1)));
    let pdf = open_active(&state, "shapes.pdf");

    let detected = detect_all(&state, pdf.id);

    // Loading the model alone takes far longer than a millisecond.
    assert_eq!(detected.page_numbers(), [1, 2, 3]);
    for page in &detected.pages {
        assert_eq!(error_code(&page.error), Some(ErrorCode::WorkerTimeout));
        assert_eq!(page.candidates, []);
    }
    assert_eq!(
        detected.finished,
        JobFinishedPayload {
            failed: 3,
            ..NOTHING_DONE
        }
    );
    assert_eq!(spawns(&state), 3, "each page started a worker again");
}

#[cfg(feature = "test-hooks")]
#[test]
fn opening_another_pdf_forgets_the_pages_that_were_answered() {
    let state = state();
    let first = open_active(&state, "crop_layout.pdf");
    detect_layout(&state, first.id, 1).expect("detecting the layout");
    assert_eq!(detect_all(&state, first.id).pages, []);

    let second = open_active(&state, "crop_layout.pdf");
    let detected = detect_all(&state, second.id);

    assert_eq!(detected.page_numbers(), [1]);
    assert_eq!(detected.pages[0].id, second.id);
    assert_eq!(
        detected.finished,
        JobFinishedPayload {
            succeeded: 1,
            ..NOTHING_DONE
        }
    );
    assert_eq!(spawns(&state), 2);
}

#[test]
fn the_analysis_of_every_page_is_refused_during_a_conversion() {
    let state = state();
    let pdf = open_active(&state, "crop_layout.pdf");
    state.is_running.store(true, Ordering::SeqCst);

    let refused = detect_all_with(&state, pdf.id, |_| {})
        .err()
        .expect("refused");

    assert_eq!(refused.code, ErrorCode::ConversionRunning);
    assert!(
        state.is_running.load(Ordering::SeqCst),
        "the other conversion is still running"
    );
}

#[test]
fn the_analysis_of_every_page_needs_the_open_pdf() {
    let state = state();
    let open = open_active(&state, "crop_layout.pdf");
    let pending = open_pdf(&state, &fixture("shapes.pdf"));
    let broken = without_workers(&state);

    for id in [pending.id, open.id + 1000] {
        let refused = detect_all_with(&broken, id, |_| {}).err().expect("refused");
        assert_eq!(refused.code, ErrorCode::UnknownHandle);
        assert!(!broken.is_running.load(Ordering::SeqCst));
    }
}

#[test]
fn the_analysis_of_every_page_answers_a_pdf_that_could_not_be_opened_with_its_error() {
    let state = state();
    let pdf = open_active(&state, "corrupt.pdf");
    assert!(pdf.error.is_some());

    let refused = detect_all_with(&state, pdf.id, |_| {})
        .err()
        .expect("refused");

    assert_eq!(Some(refused), pdf.error);
    assert!(!state.is_running.load(Ordering::SeqCst));
}

#[test]
fn a_detection_by_hand_is_allowed_while_every_page_is_being_analyzed() {
    let state = state();
    let pdf = open_active(&state, "shapes.pdf");
    let asked = Arc::new(Mutex::new(None));
    let during = {
        let state = state.clone();
        let asked = Arc::clone(&asked);
        let id = pdf.id;
        // Runs on the job's thread, after the first page: it takes its turn
        // for the analysis worker like the pages do.
        move |_: &LayoutPagePayload| {
            let answer = detect_layout(&state, id, 3);
            *asked.lock().expect("asked lock") = Some(answer.is_ok());
        }
    };

    let detected = detect_all_with(&state, pdf.id, during).expect("starting");

    assert_eq!(*asked.lock().expect("asked lock"), Some(true));
    assert_eq!(detected.page_numbers(), [1, 2, 3]);
}
