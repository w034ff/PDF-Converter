//! The image requests of the standalone worker binary (design §5.1, §6.1, §6.2): the thumbnail of
//! a list item and the check of the pixels before an image is written.

#[cfg(feature = "test-hooks")]
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

#[path = "support/worker_paths.rs"]
mod worker_paths;

use pdfconv_worker::client::{RENDER_TIMEOUT, StreamPart, WorkerError, WorkerProcess};
use pdfconv_worker::protocol::{Request, Response};
#[cfg(feature = "test-hooks")]
use pdfconv_worker::server::{CRASH_ON_IMAGE_FILE_NAME, HANG_ON_IMAGE_FILE_NAME};
use worker_paths::start;

const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

/// The longer side of a thumbnail (design §6.1).
const THUMBNAIL_SIDE: u32 = pdfconv_core::THUMBNAIL_SIDE;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures")
        .join(name)
}

fn thumbnail(
    worker: &mut WorkerProcess,
    path: PathBuf,
) -> Result<(Response, Vec<u8>), WorkerError> {
    worker.request(&Request::ImageThumbnail { path }, ANSWER_TIMEOUT)
}

/// What a `CheckImage` request answered: the pages reported and the last answer.
fn check(worker: &mut WorkerProcess, path: PathBuf) -> (Vec<u32>, Result<Response, WorkerError>) {
    let mut pages = Vec::new();
    let mut chunks = 0;
    let result = worker.request_stream(
        &Request::CheckImage { path },
        RENDER_TIMEOUT,
        &|| false,
        &mut |part| match part {
            StreamPart::Progress { done } => pages.push(done),
            StreamPart::Chunk(_) => chunks += 1,
        },
    );
    assert_eq!(chunks, 0, "a check sends no chunk");
    (pages, result)
}

/// The code and detail of a remote error; anything else fails the test.
fn remote_code(result: Result<impl std::fmt::Debug, WorkerError>) -> (String, Option<String>) {
    assert!(
        matches!(result, Err(WorkerError::Remote { .. })),
        "expected a remote error, got {result:?}"
    );
    match result {
        Err(WorkerError::Remote { code, detail }) => (code, detail),
        _ => (String::new(), None),
    }
}

#[test]
fn a_thumbnail_is_a_png_with_the_longer_side_at_the_thumbnail_side() {
    let mut worker = start();
    for name in [
        "photo.jpg",
        "dpi300.png",
        "logo_alpha.webp",
        "opaque.bmp",
        "scan_g4.tif",
    ] {
        let (response, body) = thumbnail(&mut worker, fixture(name)).expect(name);
        assert_eq!(response, Response::ImageThumbnail, "{name}");
        let png = image::load_from_memory_with_format(&body, image::ImageFormat::Png)
            .unwrap_or_else(|err| panic!("{name}: {err}"));
        assert_eq!(png.width().max(png.height()), THUMBNAIL_SIDE, "{name}");
    }
}

#[test]
fn a_thumbnail_is_the_same_as_the_one_the_core_makes() {
    let mut worker = start();
    let expected = pdfconv_core::thumbnail_png(&std::fs::read(fixture("photo.jpg")).unwrap())
        .expect("making the thumbnail");
    let (_, body) = thumbnail(&mut worker, fixture("photo.jpg")).unwrap();
    assert_eq!(body, expected);
}

#[test]
fn a_thumbnail_that_fails_answers_the_code_of_the_probe_error() {
    let mut worker = start();
    assert_eq!(
        remote_code(thumbnail(&mut worker, fixture("corrupt.png"))).0,
        "DecodeFailed"
    );
    assert_eq!(
        remote_code(thumbnail(&mut worker, fixture("corrupt.pdf"))).0,
        "UnsupportedFormat"
    );
    assert_eq!(
        remote_code(thumbnail(&mut worker, fixture("no-such-file.png"))).0,
        "ReadFailed"
    );
    // The worker still answers.
    thumbnail(&mut worker, fixture("photo.jpg")).expect("the next thumbnail");
}

#[test]
fn a_check_of_one_page_images_ends_with_image_checked_and_no_progress() {
    let mut worker = start();
    for name in [
        "photo.jpg",
        "dpi300.png",
        "deep16.png",
        "logo_alpha.webp",
        "opaque.bmp",
    ] {
        let (pages, result) = check(&mut worker, fixture(name));
        assert_eq!(result.expect(name), Response::ImageChecked, "{name}");
        assert!(pages.is_empty(), "{name}: {pages:?}");
    }
}

#[test]
fn a_check_of_a_tiff_reports_each_page() {
    let mut worker = start();
    let (pages, result) = check(&mut worker, fixture("scan_g4.tif"));
    assert_eq!(result.unwrap(), Response::ImageChecked);
    assert_eq!(pages, [1, 2, 3]);
}

#[test]
fn a_check_that_fails_answers_the_code_and_detail_of_the_probe_error() {
    let mut worker = start();
    assert_eq!(
        remote_code(check(&mut worker, fixture("corrupt.png")).1),
        ("DecodeFailed".to_owned(), None)
    );

    // The first page is read and reported before the second fails.
    let (pages, result) = check(&mut worker, fixture("corrupt_page2.tif"));
    assert_eq!(remote_code(result), ("DecodeFailed".to_owned(), None));
    assert!(pages.len() < 2, "{pages:?}");

    assert_eq!(
        remote_code(check(&mut worker, fixture("too_many_pages.tif")).1),
        ("TooManyPages".to_owned(), Some("1,000".to_owned()))
    );
    assert_eq!(
        remote_code(check(&mut worker, fixture("no-such-file.png")).1),
        ("ReadFailed".to_owned(), None)
    );
    assert_eq!(
        remote_code(check(&mut worker, fixture("corrupt.pdf")).1).0,
        "UnsupportedFormat"
    );

    // The worker still answers.
    let (_, result) = check(&mut worker, fixture("photo.jpg"));
    assert_eq!(result.unwrap(), Response::ImageChecked);
}

#[test]
fn the_image_requests_leave_the_open_pdf_open() {
    let mut worker = start();
    let (response, _) = worker
        .request(
            &Request::Open {
                path: fixture("shapes.pdf"),
                password: None,
            },
            ANSWER_TIMEOUT,
        )
        .expect("opening the PDF");
    assert!(matches!(response, Response::Open { .. }));

    thumbnail(&mut worker, fixture("photo.jpg")).expect("the image thumbnail");
    let (_, result) = check(&mut worker, fixture("scan_g4.tif"));
    assert_eq!(result.unwrap(), Response::ImageChecked);
    let _ = remote_code(check(&mut worker, fixture("corrupt.png")).1);
    let _ = remote_code(thumbnail(&mut worker, fixture("corrupt.png")));

    let (response, body) = worker
        .request(
            &Request::Thumbnail {
                page: 1,
                max_side: 160,
            },
            ANSWER_TIMEOUT,
        )
        .expect("the PDF is still open");
    assert_eq!(response, Response::Thumbnail);
    assert!(!body.is_empty());
}

#[cfg(feature = "test-hooks")]
/// Copies `fixture_name` to `name` in a folder of its own: the hooks go by the file name.
fn put(fixture_name: &str, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfconv-worker-image-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("creating the folder");
    let path = dir.join(name);
    std::fs::copy(fixture(fixture_name), &path).expect("copying the fixture");
    path
}

#[cfg(feature = "test-hooks")]
fn remove_folder_of(path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_worker_that_crashes_on_an_image_is_a_crash() {
    let path = put("dpi300.png", CRASH_ON_IMAGE_FILE_NAME);

    let mut worker = start();
    assert!(matches!(
        thumbnail(&mut worker, path.clone()),
        Err(WorkerError::Crashed)
    ));

    let mut worker = start();
    let (_, result) = check(&mut worker, path.clone());
    assert!(matches!(result, Err(WorkerError::Crashed)), "{result:?}");
    remove_folder_of(&path);
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_worker_that_stops_on_an_image_times_out() {
    let path = put("dpi300.png", HANG_ON_IMAGE_FILE_NAME);

    let mut worker = start();
    let result = worker.request(
        &Request::ImageThumbnail { path: path.clone() },
        Duration::from_millis(500),
    );
    assert!(matches!(result, Err(WorkerError::Timeout)), "{result:?}");

    let mut worker = start();
    let result = worker.request_stream(
        &Request::CheckImage { path: path.clone() },
        Duration::from_millis(500),
        &|| false,
        &mut |_| {},
    );
    assert!(matches!(result, Err(WorkerError::Timeout)), "{result:?}");
    remove_folder_of(&path);
}
