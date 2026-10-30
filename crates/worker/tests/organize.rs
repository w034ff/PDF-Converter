//! Organizing with the standalone worker binary (design organize §1, §3, §8): the pages of the
//! output look like the pages they were copied from, and nothing of the sources' documents
//! information remains.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[path = "../../core/tests/support/fixture_passwords.rs"]
mod fixture_passwords;
#[path = "../../core/tests/support/organize_fixtures.rs"]
mod organize_fixtures;
#[path = "support/worker_paths.rs"]
mod worker_paths;

use image::RgbImage;
use pdfconv_worker::client::{ORGANIZE_STEP_TIMEOUT, StreamPart, WorkerError, WorkerProcess};
use pdfconv_worker::protocol::{
    OUTPUT_CHUNK_BYTES, OrganizePage, OrganizeSource, Password, RenderColor, RenderFormat, Request,
    Response,
};
use worker_paths::start;

const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

/// How fast a cancellation must take effect (NFR-02).
const CANCEL_TIME_LIMIT: Duration = Duration::from_secs(1);

/// The creator that the tests pass in; the output's document information holds only this.
const CREATOR: &str = "PDF Converter test";

/// Resolution of the renders that are compared.
const COMPARE_DPI: u32 = 72;

/// Tolerance for pixel color differences, as in the PDF → image render tests (design §11.2).
const COLOR_TOLERANCE_RENDER: i16 = 32;

/// The ratio of differing pixels allowed. The render of the output and the render of the source
/// come from the same pdfium and match to the pixel, so this is much stricter than the 0.5% of
/// the PDF → image render tests (design §11.2): a missing sticky note of 20 × 20 pt on a
/// 300 × 400 pt page is 0.28% of the pixels and must fail.
const MAX_MISMATCH_RATIO_ORGANIZE: f64 = 0.0005;

/// The ratio allowed between a page that pdfium turned and the render of the source turned as a
/// picture: the two put the edges on the pixel grid differently (design §11.2).
const MAX_MISMATCH_RATIO_TURNED: f64 = 0.005;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures")
}

fn fixture(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

fn source(name: &str) -> OrganizeSource {
    OrganizeSource {
        path: fixture(name),
        password: None,
    }
}

fn page(source: u32, page: u32, rotation: u16) -> OrganizePage {
    OrganizePage {
        source,
        page,
        rotation,
    }
}

/// All the pages of the first `count` pages of source 0, unturned.
fn pages_in_order(count: u32) -> Vec<OrganizePage> {
    (0..count).map(|index| page(0, index, 0)).collect()
}

/// What `request_stream` handed over for one `Organize` request.
struct Output {
    bytes: Vec<u8>,
    chunk_lengths: Vec<usize>,
    progress: Vec<u32>,
    reported_bytes: u64,
}

fn organize_request(
    sources: Vec<OrganizeSource>,
    pages: Vec<OrganizePage>,
    output_password: Option<&str>,
) -> Request {
    Request::Organize {
        sources,
        pages,
        creator: CREATOR.into(),
        output_password: output_password.map(|text| Password::new(text).expect("a password")),
    }
}

fn run_organize(worker: &mut WorkerProcess, request: &Request) -> Result<Output, WorkerError> {
    let mut output = Output {
        bytes: Vec::new(),
        chunk_lengths: Vec::new(),
        progress: Vec::new(),
        reported_bytes: 0,
    };
    let last =
        worker.request_stream(
            request,
            ORGANIZE_STEP_TIMEOUT,
            &|| false,
            &mut |part| match part {
                StreamPart::Progress { done } => output.progress.push(done),
                StreamPart::Chunk(chunk) => {
                    output.chunk_lengths.push(chunk.len());
                    output.bytes.extend_from_slice(&chunk);
                }
            },
        )?;
    output.reported_bytes = match last {
        Response::Organized { bytes } => Some(bytes),
        _ => None,
    }
    .expect("the last answer is Organized");
    Ok(output)
}

fn organize(
    worker: &mut WorkerProcess,
    sources: Vec<OrganizeSource>,
    pages: Vec<OrganizePage>,
) -> Output {
    run_organize(worker, &organize_request(sources, pages, None)).expect("organizing succeeds")
}

fn expect_invalid_params(worker: &mut WorkerProcess, request: &Request) {
    let result = run_organize(worker, request).map(|_| ());
    assert!(
        matches!(&result, Err(WorkerError::Remote { code, .. }) if code == "InvalidParams"),
        "expected InvalidParams, got {result:?}"
    );
}

/// A PDF written to the temporary directory; the file goes when the value does.
struct TemporaryFile(PathBuf);

impl TemporaryFile {
    fn new(name: &str, bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pdfconv-organize-{name}-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&path, bytes).expect("the temporary PDF should be written");
        Self(path)
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Opens the PDF at `path` in `worker`, returning its page count.
fn open(worker: &mut WorkerProcess, path: &std::path::Path, password: Option<&str>) -> u32 {
    let (response, _) = worker
        .request(
            &Request::Open {
                path: path.to_path_buf(),
                password: password.map(|text| Password::new(text).expect("a password")),
            },
            ANSWER_TIMEOUT,
        )
        .expect("opening the PDF");
    match response {
        Response::Open { page_count, .. } => Some(page_count),
        _ => None,
    }
    .expect("the answer to Open is Open")
}

/// Every page of the PDF at `path`, rendered at [`COMPARE_DPI`].
fn render_all(path: &std::path::Path, password: Option<&str>) -> Vec<RgbImage> {
    let mut worker = start();
    let count = open(&mut worker, path, password);
    (1..=count)
        .map(|number| {
            let (response, body) = worker
                .request(
                    &Request::Render {
                        page: number,
                        dpi: COMPARE_DPI,
                        format: RenderFormat::Png,
                        color: RenderColor::Color,
                    },
                    ANSWER_TIMEOUT,
                )
                .expect("rendering a page");
            assert_eq!(response, Response::Render);
            image::load_from_memory(&body)
                .expect("a rendered page decodes")
                .to_rgb8()
        })
        .collect()
}

/// Every page of `bytes`, rendered.
fn render_bytes(name: &str, bytes: &[u8], password: Option<&str>) -> Vec<RgbImage> {
    let file = TemporaryFile::new(name, bytes);
    render_all(&file.0, password)
}

fn mismatch_ratio(actual: &RgbImage, expected: &RgbImage) -> f64 {
    assert_eq!(actual.dimensions(), expected.dimensions(), "page sizes");
    let differing = actual
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, b)| {
            a.0.iter()
                .zip(b.0)
                .any(|(&x, y)| (i16::from(x) - i16::from(y)).abs() > COLOR_TOLERANCE_RENDER)
        })
        .count();
    differing as f64 / (u64::from(actual.width()) * u64::from(actual.height())) as f64
}

fn assert_same_look(actual: &RgbImage, expected: &RgbImage, what: &str) {
    assert_within(actual, expected, MAX_MISMATCH_RATIO_ORGANIZE, what);
}

fn assert_within(actual: &RgbImage, expected: &RgbImage, limit: f64, what: &str) {
    let ratio = mismatch_ratio(actual, expected);
    println!("{what}: {:.4}% differ", ratio * 100.0);
    assert!(
        ratio <= limit,
        "{what}: {:.3}% of the pixels differ (limit {:.3}%)",
        ratio * 100.0,
        limit * 100.0
    );
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    count(haystack, needle) > 0
}

fn count(haystack: &[u8], needle: &str) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle.as_bytes())
        .count()
}

#[test]
fn every_page_of_the_fixtures_looks_like_the_source_page() {
    let mut worker = start();
    for (name, pages) in [
        ("organize_annots.pdf", 2),
        ("organize_meta.pdf", 3),
        ("mixed_sizes.pdf", 3),
        ("crop_align.pdf", 3),
    ] {
        let output = organize(&mut worker, vec![source(name)], pages_in_order(pages));
        let expected = render_all(&fixture(name), None);
        let actual = render_bytes("same-look", &output.bytes, None);
        assert_eq!(actual.len(), expected.len(), "{name}: page count");
        for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            assert_same_look(actual, expected, &format!("{name} page {}", index + 1));
        }
    }
}

#[test]
fn a_turned_page_looks_like_the_source_page_turned() {
    let mut worker = start();
    // crop_align.pdf has /Rotate 0, 90 and 270 on its pages, so each turn is added to a
    // rotation the page already has.
    let expected = render_all(&fixture("crop_align.pdf"), None);
    for rotation in [90u16, 180, 270] {
        let pages = (0..3).map(|index| page(0, index, rotation)).collect();
        let output = organize(&mut worker, vec![source("crop_align.pdf")], pages);
        let actual = render_bytes("turned", &output.bytes, None);
        for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            let turned = match rotation {
                90 => image::imageops::rotate90(expected),
                180 => image::imageops::rotate180(expected),
                _ => image::imageops::rotate270(expected),
            };
            assert_within(
                actual,
                &turned,
                MAX_MISMATCH_RATIO_TURNED,
                &format!("crop_align page {} turned {rotation}", index + 1),
            );
        }
    }
}

#[test]
fn pages_of_several_sources_come_out_in_the_requested_order() {
    let mut worker = start();
    let meta = render_all(&fixture("organize_meta.pdf"), None);
    let mixed = render_all(&fixture("mixed_sizes.pdf"), None);
    let sources = || vec![source("organize_meta.pdf"), source("mixed_sizes.pdf")];
    let a = |index: usize| (0u32, index, &meta[index]);
    let b = |index: usize| (1u32, index, &mixed[index]);

    // The sources alternate (A1, B1, A2, A3, B2), then the pages of one source come in an order
    // other than the source's own, and one is left out.
    for (name, order) in [
        ("alternating", vec![a(0), b(0), a(1), a(2), b(1)]),
        ("shuffled", vec![b(2), b(0), a(1), a(0), b(1), a(2)]),
        ("second first", vec![b(1), b(2), a(2), a(0)]),
    ] {
        let pages = order
            .iter()
            .map(|(source, index, _)| page(*source, *index as u32, 0))
            .collect();
        let output = organize(&mut worker, sources(), pages);
        let actual = render_bytes("order", &output.bytes, None);
        assert_eq!(actual.len(), order.len(), "{name}: page count");
        for (position, (actual, (_, _, expected))) in actual.iter().zip(&order).enumerate() {
            assert_same_look(actual, expected, &format!("{name}: output page {position}"));
        }
    }
}

#[test]
fn annotations_are_flattened_but_links_stay_on_a_page_of_links_only() {
    let mut worker = start();
    let flattened = organize(
        &mut worker,
        vec![source("organize_annots.pdf")],
        vec![page(0, 0, 0)],
    );
    for kind in ["/Link", "/Widget", "/Highlight", "/FreeText", "/Annots"] {
        assert!(!contains(&flattened.bytes, kind), "page 1 still has {kind}");
    }

    let linked = organize(
        &mut worker,
        vec![source("organize_annots.pdf")],
        vec![page(0, 1, 0)],
    );
    assert_eq!(count(&linked.bytes, "/Link"), 1, "the link of page 2");
    assert!(contains(&linked.bytes, "/Annots"));
    assert!(!contains(&linked.bytes, "/Widget"));
}

#[test]
fn nothing_of_the_sources_document_information_remains() {
    use organize_fixtures::*;
    let mut worker = start();
    let output = organize(
        &mut worker,
        vec![source("organize_meta.pdf")],
        pages_in_order(3),
    );
    for secret in [
        META_AUTHOR,
        META_TITLE,
        META_CREATOR,
        META_PRODUCER,
        META_CREATION_DIGITS,
        META_XMP,
    ] {
        assert!(
            !contains(&output.bytes, secret),
            "{secret} is in the output"
        );
    }
    // The whole information dictionary, as lopdf writes it.
    assert!(contains(&output.bytes, &format!("<</Creator({CREATOR})>>")));
    for key in [
        "/Author",
        "/Title",
        "/Producer",
        "/CreationDate",
        "/ModDate",
        "/Metadata",
    ] {
        assert!(!contains(&output.bytes, key), "{key} is in the output");
    }
}

#[test]
fn an_output_password_opens_the_output_and_only_that() {
    let mut worker = start();
    let sources = || vec![source("organize_meta.pdf")];
    let plain = organize(&mut worker, sources(), pages_in_order(3));
    let locked = run_organize(
        &mut worker,
        &organize_request(sources(), pages_in_order(3), Some("out-pass 1")),
    )
    .expect("organizing with a password succeeds");
    assert!(contains(&locked.bytes, "/Encrypt"));

    let file = TemporaryFile::new("locked", &locked.bytes);
    let mut checker = start();
    for wrong in [None, Some("out-pass 2")] {
        let refused = checker.request(
            &Request::Open {
                path: file.0.clone(),
                password: wrong.map(|text| Password::new(text).expect("a password")),
            },
            ANSWER_TIMEOUT,
        );
        match refused {
            Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "PasswordProtected"),
            other => panic!("expected PasswordProtected for {wrong:?}, got {other:?}"),
        }
    }
    let expected = render_bytes("plain", &plain.bytes, None);
    let actual = render_all(&file.0, Some("out-pass 1"));
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        assert_same_look(actual, expected, &format!("locked page {}", index + 1));
    }
}

#[test]
fn an_output_password_that_breaks_the_rules_is_invalid() {
    let mut worker = start();
    for password in ["日本語", &"a".repeat(128)] {
        let request = organize_request(
            vec![source("organize_meta.pdf")],
            pages_in_order(1),
            Some(password),
        );
        expect_invalid_params(&mut worker, &request);
    }
}

#[test]
fn a_source_with_a_password_needs_it_and_one_with_restrictions_does_not() {
    let mut worker = start();
    let with_password = OrganizeSource {
        path: fixture("encrypted.pdf"),
        password: Some(Password::new(fixture_passwords::ENCRYPTED_USER_PASSWORD).unwrap()),
    };
    let output = organize(
        &mut worker,
        vec![
            source("restricted.pdf"),
            with_password,
            source("organize_meta.pdf"),
        ],
        vec![page(0, 0, 0), page(1, 0, 0), page(2, 1, 0)],
    );
    let actual = render_bytes("sources-with-passwords", &output.bytes, None);
    let restricted = render_all(&fixture("restricted.pdf"), None);
    let encrypted = render_all(
        &fixture("encrypted.pdf"),
        Some(fixture_passwords::ENCRYPTED_USER_PASSWORD),
    );
    let meta = render_all(&fixture("organize_meta.pdf"), None);
    assert_eq!(actual.len(), 3);
    assert_same_look(&actual[0], &restricted[0], "restricted.pdf");
    assert_same_look(&actual[1], &encrypted[0], "encrypted.pdf");
    assert_same_look(&actual[2], &meta[1], "organize_meta.pdf page 2");

    let missing = run_organize(
        &mut worker,
        &organize_request(
            vec![source("organize_meta.pdf"), source("encrypted.pdf")],
            vec![page(0, 0, 0), page(1, 0, 0)],
            None,
        ),
    );
    match missing {
        Err(WorkerError::SourceFailed { code, source, .. }) => {
            assert_eq!((code.as_str(), source), ("PasswordProtected", 1));
        }
        other => panic!("expected PasswordProtected, got {:?}", other.map(|_| ())),
    }
    let wrong = OrganizeSource {
        path: fixture("encrypted.pdf"),
        password: Some(Password::new("not-the-password").unwrap()),
    };
    let wrong = run_organize(
        &mut worker,
        &organize_request(vec![wrong], vec![page(0, 0, 0)], None),
    );
    assert!(matches!(
        wrong,
        Err(WorkerError::SourceFailed { ref code, source: 0, .. }) if code == "PasswordProtected"
    ));
}

#[test]
fn a_corrupt_source_fails_with_its_number_and_the_worker_goes_on() {
    let mut worker = start();
    let result = run_organize(
        &mut worker,
        &organize_request(
            vec![source("organize_meta.pdf"), source("corrupt.pdf")],
            vec![page(0, 0, 0), page(1, 0, 0)],
            None,
        ),
    );
    match result {
        Err(WorkerError::SourceFailed { code, source, .. }) => {
            assert_eq!((code.as_str(), source), ("PdfOpenFailed", 1));
        }
        other => panic!("expected PdfOpenFailed, got {:?}", other.map(|_| ())),
    }
    let (response, _) = worker
        .request(&Request::Hello, ANSWER_TIMEOUT)
        .expect("the worker answers after a failed source");
    assert_eq!(response, Response::Hello);
    // And it organizes again.
    organize(
        &mut worker,
        vec![source("organize_meta.pdf")],
        pages_in_order(1),
    );
}

#[test]
fn a_page_past_the_end_of_its_source_fails_that_source() {
    let mut worker = start();
    let result = run_organize(
        &mut worker,
        &organize_request(
            vec![source("organize_meta.pdf"), source("mixed_sizes.pdf")],
            vec![page(0, 0, 0), page(1, 3, 0)],
            None,
        ),
    );
    match result {
        Err(WorkerError::SourceFailed { code, source, .. }) => {
            assert_eq!((code.as_str(), source), ("InvalidParams", 1));
        }
        other => panic!("expected InvalidParams, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn arguments_that_do_not_make_sense_are_invalid() {
    let mut worker = start();
    let one = || vec![source("organize_meta.pdf")];
    for (what, request) in [
        ("no pages", organize_request(one(), vec![], None)),
        (
            "a source out of range",
            organize_request(one(), vec![page(1, 0, 0)], None),
        ),
        (
            "no sources",
            organize_request(vec![], vec![page(0, 0, 0)], None),
        ),
        (
            "a rotation of 45",
            organize_request(one(), vec![page(0, 0, 45)], None),
        ),
        (
            "a rotation of 360",
            organize_request(one(), vec![page(0, 0, 360)], None),
        ),
        (
            "the same page twice",
            organize_request(
                one(),
                vec![page(0, 1, 0), page(0, 0, 0), page(0, 1, 90)],
                None,
            ),
        ),
    ] {
        eprintln!("{what}");
        expect_invalid_params(&mut worker, &request);
    }
    // The same page of the same file is fine when it is two sources.
    organize(
        &mut worker,
        vec![source("organize_meta.pdf"), source("organize_meta.pdf")],
        vec![page(0, 1, 0), page(1, 1, 90)],
    );
}

#[test]
fn the_output_comes_in_chunks_that_add_up() {
    let mut worker = start();
    // Many copies of a page with its own images: each source is copied whole, so the output is
    // several chunks long.
    let copies = 120u32;
    let sources = (0..copies).map(|_| source("crop_align.pdf")).collect();
    let pages: Vec<OrganizePage> = (0..copies).map(|copy| page(copy, 0, 0)).collect();
    let output = organize(&mut worker, sources, pages.clone());

    assert!(
        output.chunk_lengths.len() > 1,
        "the output is {} bytes, one chunk",
        output.bytes.len()
    );
    assert!(
        output
            .chunk_lengths
            .iter()
            .all(|&length| length > 0 && length <= OUTPUT_CHUNK_BYTES)
    );
    assert_eq!(output.reported_bytes, output.bytes.len() as u64);
    assert_eq!(
        output.chunk_lengths.iter().sum::<usize>(),
        output.bytes.len()
    );
    assert!(output.progress.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(output.progress.last(), Some(&(pages.len() as u32)));

    let file = TemporaryFile::new("chunks", &output.bytes);
    let mut checker = start();
    assert_eq!(open(&mut checker, &file.0, None), copies);
}

#[test]
fn cancelling_kills_the_worker_within_a_second() {
    let mut worker = start();
    let id = worker.id();
    let copies = 50u32;
    let sources = (0..copies).map(|_| source("organize_annots.pdf")).collect();
    let pages = (0..copies).map(|copy| page(copy, 0, 0)).collect();
    let cancel = AtomicBool::new(false);
    let mut cancelled_at = None;
    let result = worker.request_stream(
        &organize_request(sources, pages, None),
        ORGANIZE_STEP_TIMEOUT,
        &|| cancel.load(Ordering::SeqCst),
        &mut |part| {
            if matches!(part, StreamPart::Progress { .. }) && cancelled_at.is_none() {
                cancelled_at = Some(Instant::now());
                cancel.store(true, Ordering::SeqCst);
            }
        },
    );
    let started = cancelled_at.expect("progress arrived before the end");
    assert!(matches!(result, Err(WorkerError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < CANCEL_TIME_LIMIT);
    assert!(worker.is_dead());
    #[cfg(target_os = "linux")]
    assert!(
        !std::path::Path::new(&format!("/proc/{id}")).exists(),
        "the worker process {id} is still there"
    );
    let _ = id;
    assert!(matches!(
        worker.request(&Request::Hello, ANSWER_TIMEOUT),
        Err(WorkerError::Crashed)
    ));
}

#[cfg(feature = "test-hooks")]
#[test]
fn a_worker_that_stops_answering_times_out() {
    let mut worker = start();
    let started = Instant::now();
    let result = worker.request_stream(
        &Request::HangForTest,
        Duration::from_millis(500),
        &|| false,
        &mut |_| {},
    );
    assert!(matches!(result, Err(WorkerError::Timeout)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(worker.is_dead());
}
