//! OCR against the standalone worker and the models fetched by
//! `npm run layout:fetch` (design ocr §11). The pages are drawn by
//! `crates/core/examples/gen_ocr_fixtures.rs`; the parity pages and their
//! expected lines come from `crates/core/tests/ocr-fixtures/make_parity.py`.

#[path = "support/worker_paths.rs"]
mod worker_paths;

use std::ffi::OsString;
use std::path::PathBuf;

use pdfconv_core::ErrorCode;
use pdfconv_core::ocr::{OcrLine, OcrPage, Orientation, Quad};
use pdfconv_worker::client::{
    DETECT_TIMEOUT, OCR_TIMEOUT, RENDER_TIMEOUT, WorkerError, WorkerProcess,
};
use pdfconv_worker::protocol::{Request, Response, WorkerCode};

/// How far a corner of a line's box may be from PaddleOCR's, in points (one
/// pixel on the parity pages). The survey's port matched PaddleOCR within 2
/// pixels: OpenCV computes boxes in float32 and pyclipper truncates them.
const PARITY_MAX_CORNER_PT: f64 = 2.0;

/// The most edits per expected character of a block's text. The pages are
/// clean, and on Linux they read without one; another platform's onnxruntime
/// may read a character differently.
const MAX_EDIT_RATE: f64 = 0.05;

fn ocr_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/ocr-fixtures")
        .join(name)
}

fn open(worker: &mut WorkerProcess, path: PathBuf) {
    let opened = worker.request(
        &Request::Open {
            path: path.clone(),
            password: None,
        },
        RENDER_TIMEOUT,
    );
    assert!(opened.is_ok(), "opening {}: {opened:?}", path.display());
}

fn ocr(worker: &mut WorkerProcess, page: u32) -> OcrPage {
    match worker.request(&Request::Ocr { page }, OCR_TIMEOUT) {
        Ok((Response::Ocr, body)) => serde_json::from_slice(&body).expect("an OcrPage"),
        other => unreachable!("expected an OCR result, got {other:?}"),
    }
}

fn expect_error(worker: &mut WorkerProcess, request: &Request, expected: WorkerCode) {
    match worker.request(request, OCR_TIMEOUT) {
        Err(WorkerError::Remote { code, detail }) => {
            assert_eq!(code, expected, "{detail:?}");
        }
        other => unreachable!("expected {expected}, got {other:?}"),
    }
}

fn pages_worker() -> WorkerProcess {
    let mut worker = worker_paths::start();
    open(&mut worker, ocr_fixture("ocr_pages.pdf"));
    worker
}

/// Levenshtein distance in characters.
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != *cb))
                .min(row[j] + 1)
                .min(above + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// Checks that the page has one block per entry of `expected`, in that order,
/// each with that text (its lines joined) and orientation.
fn assert_blocks(page: &OcrPage, expected: &[(Orientation, String)]) {
    let got: Vec<(Orientation, String)> = page
        .blocks
        .iter()
        .map(|block| {
            let text: String = block.lines.iter().map(|l| l.text.as_str()).collect();
            (block.orientation, text)
        })
        .collect();
    assert_eq!(got.len(), expected.len(), "{got:#?}");
    for ((orientation, text), (want_orientation, want)) in got.iter().zip(expected) {
        assert_eq!(orientation, want_orientation, "{text}");
        let rate = edits(text, want) as f64 / want.chars().count() as f64;
        assert!(rate <= MAX_EDIT_RATE, "{text} for {want}");
    }
}

fn joined(lines: &[&str]) -> String {
    lines.concat()
}

fn horizontal(text: &str) -> (Orientation, String) {
    (Orientation::Horizontal, text.to_owned())
}

fn vertical(text: &str) -> (Orientation, String) {
    (Orientation::Vertical, text.to_owned())
}

// The pages' text, as gen_ocr_fixtures.rs draws it.
const HORIZONTAL_BODY: [&str; 6] = [
    "この頁は横書きの文章を読む試験のために作りました。",
    "見出しの下に本文が続き、行は上から下へ並びます。",
    "読み取った文字は、頁の上の位置と一緒に返されます。",
    "検索や選択ができるように、見えない文字の層を足します。",
    "画面の上で範囲を選ぶと、その中の文字を写せます。",
    "最後の行は短く終わります。",
];
const VERTICAL_UPPER: [&str; 6] = [
    "縦書きの本文は右の列から左の列へ読みます",
    "列の中では上から下へ字が並んでいます",
    "上の段を読み終えたら下の段へ移ります",
    "見出しは本文より大きな字で書かれています",
    "列と列の間は字の幅の半分ほどあいています",
    "この段はここで終わります",
];
const VERTICAL_LOWER: [&str; 5] = [
    "下の段も右の列から読み始めます",
    "段と段の間は広くあけてあります",
    "字の大きさは上の段と同じです",
    "古い本や新聞には縦書きが多くあります",
    "頁の終わりの列です",
];
const COLUMNS_LEFT: [&str; 7] = [
    "左の段の文章は上から下へ読みます。",
    "段の幅は頁の半分より少し狭く、行",
    "の長さはどれもほぼ同じになります。",
    "左の段を読み終えてから右の段へ移",
    "ります。行ごとに左右を行き来して",
    "読むと、文章がつながりません。",
    "左の段の終わり。",
];
const COLUMNS_RIGHT: [&str; 7] = [
    "右の段は左の段の続きです。段の間",
    "には字の幅の三つ分ほどのすき間を",
    "あけてあります。雑誌や論文では、",
    "このような組み方がよく使われます。",
    "読む順番を正しく決められれば、検",
    "索した語もつながったまま見つかり",
    "ます。",
];

fn vertical_blocks() -> Vec<(Orientation, String)> {
    vec![
        vertical("縦書きの見出し"),
        vertical(&joined(&VERTICAL_UPPER)),
        vertical(&joined(&VERTICAL_LOWER)),
    ]
}

#[test]
fn horizontal_text_is_read_heading_body_then_the_latin_line() {
    let mut worker = pages_worker();
    let page = ocr(&mut worker, 1);
    assert_eq!(page.engine, "PP-OCRv6_small");
    assert!((page.width_pt - 595.28).abs() < 0.01 && (page.height_pt - 841.89).abs() < 0.01);
    assert_blocks(
        &page,
        &[
            horizontal("文字を読む仕組みの試験"),
            horizontal(&joined(&HORIZONTAL_BODY)),
            horizontal("PDF Converter 0.1 OCR test 2026-11-17"),
        ],
    );
    // The Latin line has spaces, so it has several words.
    let latin = &page.blocks[2].lines[0];
    let words: Vec<&str> = latin.words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(
        words,
        ["PDF", "Converter", "0.1", "OCR", "test", "2026-11-17"]
    );
}

#[test]
fn vertical_tiers_are_read_heading_upper_tier_then_lower_tier() {
    let mut worker = pages_worker();
    assert_blocks(&ocr(&mut worker, 2), &vertical_blocks());
}

#[test]
fn two_columns_are_read_one_after_the_other() {
    let mut worker = pages_worker();
    assert_blocks(
        &ocr(&mut worker, 3),
        &[
            horizontal("二段組みの頁"),
            horizontal(&joined(&COLUMNS_LEFT)),
            horizontal(&joined(&COLUMNS_RIGHT)),
        ],
    );
}

#[test]
fn a_form_is_read_key_then_value() {
    let mut worker = pages_worker();
    assert_blocks(
        &ocr(&mut worker, 4),
        &[
            horizontal("申込書"),
            horizontal("氏名山田花子"),
            horizontal("住所東京都千代田区丸の内一丁目"),
            horizontal("電話番号03-1234-5678"),
            horizontal("生年月日1990年4月1日"),
            horizontal("職業会社員"),
        ],
    );
}

#[test]
fn a_vertical_page_turned_by_2_degrees_keeps_its_columns_vertical() {
    let mut worker = pages_worker();
    let page = ocr(&mut worker, 5);
    for line in page.blocks.iter().flat_map(|b| &b.lines) {
        assert_eq!(line.orientation, Orientation::Vertical, "{}", line.text);
    }
    assert_blocks(&page, &vertical_blocks());
}

fn centre(quad: &Quad) -> [f64; 2] {
    let sum = quad
        .iter()
        .fold([0.0, 0.0], |s, p| [s[0] + p[0], s[1] + p[1]]);
    [sum[0] / 4.0, sum[1] / 4.0]
}

/// Whether `point` is inside the convex quad, edges included.
fn inside(quad: &Quad, point: [f64; 2]) -> bool {
    let side = |a: [f64; 2], b: [f64; 2]| {
        (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0])
    };
    let sides: Vec<f64> = (0..4).map(|i| side(quad[i], quad[(i + 1) % 4])).collect();
    sides.iter().all(|s| *s >= -1e-6) || sides.iter().all(|s| *s <= 1e-6)
}

/// Each character's centre lies in its line's quad, and the centres follow
/// the reading direction.
fn assert_characters_placed(line: &OcrLine) {
    let centres: Vec<[f64; 2]> = line
        .words
        .iter()
        .flat_map(|w| &w.chars)
        .map(|c| centre(&c.quad))
        .collect();
    assert_eq!(
        centres.len(),
        line.text.chars().filter(|c| !c.is_whitespace()).count(),
        "{}",
        line.text
    );
    for c in &centres {
        assert!(
            inside(&line.quad, *c),
            "{}: {c:?} outside {:?}",
            line.text,
            line.quad
        );
    }
    let along = |c: &[f64; 2]| match line.orientation {
        Orientation::Horizontal => c[0],
        Orientation::Vertical => c[1],
    };
    assert!(
        centres.windows(2).all(|w| along(&w[0]) < along(&w[1])),
        "{}: {centres:?}",
        line.text
    );
}

#[test]
fn characters_lie_in_their_line_in_reading_order() {
    let mut worker = pages_worker();
    for number in [1, 2, 5] {
        let page = ocr(&mut worker, number);
        for line in page.blocks.iter().flat_map(|b| &b.lines) {
            assert_characters_placed(line);
        }
    }
}

#[test]
fn a_page_full_of_text_is_read_within_the_memory_limit() {
    // The worker runs under WORKER_MEMORY_LIMIT (design §5.3); a page that
    // did not fit would end it, and the request would fail as a crash.
    let mut worker = pages_worker();
    let page = ocr(&mut worker, 6);
    let lines = page.blocks.iter().map(|b| b.lines.len()).sum::<usize>();
    assert_eq!(lines, 60);
}

/// The corners of a quad's bounding box.
fn bounds(quad: &[[f64; 2]]) -> [f64; 4] {
    quad.iter()
        .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| {
            [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ]
        })
}

#[test]
fn lines_match_paddleocr_on_the_same_pixels() {
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ocr_fixture("ocr_parity.paddle.json")).expect("reading"),
    )
    .expect("parsing");
    let mut worker = worker_paths::start();
    open(&mut worker, ocr_fixture("ocr_parity.pdf"));
    let pages = expected["pages"].as_array().expect("pages");
    for (number, page) in (1..).zip(pages) {
        let got = ocr(&mut worker, number);
        let lines: Vec<&OcrLine> = got.blocks.iter().flat_map(|b| &b.lines).collect();
        let regions = page["regions"].as_array().expect("regions");
        assert_eq!(lines.len(), regions.len(), "page {number}");
        // PaddleOCR lists the lines top to bottom, the worker in reading
        // order, so each line is found by its text.
        for region in regions {
            let text = region["text"].as_str().expect("text");
            let quad: Vec<[f64; 2]> = region["quad"]
                .as_array()
                .expect("quad")
                .iter()
                .map(|p| [p[0].as_f64().expect("x"), p[1].as_f64().expect("y")])
                .collect();
            let want = bounds(&quad);
            let found = lines.iter().any(|line| {
                line.text == text
                    && bounds(&line.quad)
                        .iter()
                        .zip(&want)
                        .all(|(a, b)| (a - b).abs() <= PARITY_MAX_CORNER_PT)
            });
            assert!(found, "page {number}: {text} at {want:?}");
        }
    }
}

fn missing(name: &str) -> OsString {
    std::env::temp_dir()
        .join("pdfconv-no-ocr-here")
        .join(name)
        .into()
}

#[test]
fn missing_models_are_ocr_unavailable_for_every_ocr_and_other_requests_still_work() {
    let [library_dir, onnxruntime, model, _] = worker_paths::worker_args();
    let mut worker =
        worker_paths::start_with([library_dir, onnxruntime, model, missing("ocr-model")]);
    open(&mut worker, ocr_fixture("ocr_pages.pdf"));
    for _ in 0..2 {
        expect_error(
            &mut worker,
            &Request::Ocr { page: 1 },
            WorkerCode::OCR_UNAVAILABLE,
        );
        assert!(matches!(
            worker.request(
                &Request::Thumbnail {
                    page: 1,
                    max_side: 100
                },
                RENDER_TIMEOUT
            ),
            Ok((Response::Thumbnail, _))
        ));
    }
    // Layout detection shares onnxruntime and still works.
    assert!(matches!(
        worker.request(&Request::Detect { page: 1 }, DETECT_TIMEOUT),
        Ok((Response::Detect { .. }, _))
    ));
}

#[test]
fn a_missing_onnxruntime_fails_both_ocr_and_detect_every_time() {
    let [library_dir, _, model, ocr_model] = worker_paths::worker_args();
    let mut worker = worker_paths::start_with([
        library_dir,
        missing("onnxruntime-library"),
        model,
        ocr_model,
    ]);
    open(&mut worker, ocr_fixture("ocr_pages.pdf"));
    for _ in 0..2 {
        expect_error(
            &mut worker,
            &Request::Ocr { page: 1 },
            WorkerCode::OCR_UNAVAILABLE,
        );
        expect_error(
            &mut worker,
            &Request::Detect { page: 1 },
            ErrorCode::LayoutUnavailable.into(),
        );
    }
}

#[test]
fn ocr_needs_an_open_pdf_and_a_page_before_loading_anything() {
    // With nothing to load, a load would answer OcrUnavailable; these
    // requests must be rejected before that.
    let [library_dir, ..] = worker_paths::worker_args();
    let mut worker = worker_paths::start_with([
        library_dir,
        missing("onnxruntime"),
        missing("model"),
        missing("ocr-model"),
    ]);
    let invalid = ErrorCode::InvalidParams.into();
    expect_error(&mut worker, &Request::Ocr { page: 1 }, invalid);
    open(&mut worker, ocr_fixture("ocr_pages.pdf"));
    expect_error(&mut worker, &Request::Ocr { page: 0 }, invalid);
    expect_error(&mut worker, &Request::Ocr { page: 7 }, invalid);
}

#[test]
fn ocr_and_detect_share_onnxruntime_in_either_order() {
    for ocr_first in [true, false] {
        let mut worker = pages_worker();
        let detect = |worker: &mut WorkerProcess| {
            assert!(matches!(
                worker.request(&Request::Detect { page: 1 }, DETECT_TIMEOUT),
                Ok((Response::Detect { .. }, _))
            ));
        };
        if ocr_first {
            assert!(!ocr(&mut worker, 4).blocks.is_empty());
            detect(&mut worker);
        } else {
            detect(&mut worker);
            assert!(!ocr(&mut worker, 4).blocks.is_empty());
        }
        // Both models stay loaded.
        assert!(!ocr(&mut worker, 4).blocks.is_empty());
        detect(&mut worker);
    }
}
