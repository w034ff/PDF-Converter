//! The text layer of design ocr §9, end to end: a scan-like PDF is read by
//! the worker, `add_text_layer` adds the result, and pdfium takes the text
//! back out (design ocr §11, 文字の層). The scans are pages of
//! `ocr_pages.pdf` rendered by the worker and written back as images by the
//! app's own image → PDF, so they hold no text of their own.
//!
//! The character boxes are checked through pdfium's own page-to-device
//! conversion, not through the layer's: a box that pdfium shows where OCR
//! found the character is where a viewer selects it.

#[path = "support/worker_paths.rs"]
mod worker_paths;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use pdfconv_core::ocr::{OcrPage, Quad};
use pdfconv_core::text_layer::add_text_layer;
use pdfconv_core::{OutputPassword, PageSize, PdfConformance, PdfWriter, encrypt_pdf};
use pdfconv_worker::client::{OCR_TIMEOUT, RENDER_TIMEOUT, WorkerProcess};
use pdfconv_worker::protocol::{RenderColor, RenderFormat, Request, Response};
use pdfium_render::prelude::{
    PdfDocument, PdfPage, PdfPoints, PdfRenderConfig, PdfSearchOptions, Pdfium, PdfiumError,
};

/// The resolution the scans are rendered at.
const SCAN_DPI: u32 = 150;

/// How far outside its OCR quad a character's box centre may be, in points.
const BOX_TOLERANCE_PT: f64 = 0.5;

/// The page-to-device conversion of pdfium gives whole pixels; the page is
/// converted at this many pixels per point.
const DEVICE_PX_PER_PT: f64 = 10.0;

const PASSWORD: &str = "ocr-layer";

fn ocr_pages() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/ocr-fixtures/ocr_pages.pdf")
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("pdfconv-text-layer-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("creating the scratch folder");
        Self(dir)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).expect("writing a scratch file");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn open(worker: &mut WorkerProcess, path: &Path) -> u32 {
    match worker.request(
        &Request::Open {
            path: path.to_path_buf(),
            password: None,
        },
        RENDER_TIMEOUT,
    ) {
        Ok((Response::Open { page_count, .. }, _)) => page_count,
        other => unreachable!("opening {}: {other:?}", path.display()),
    }
}

/// Pages of `ocr_pages.pdf` as a PDF of images, written as `conformance`.
fn scan(worker: &mut WorkerProcess, pages: &[u32], conformance: PdfConformance) -> Vec<u8> {
    open(worker, &ocr_pages());
    let mut writer = PdfWriter::new("text layer test", conformance);
    for &page in pages {
        let png = match worker.request(
            &Request::Render {
                page,
                dpi: SCAN_DPI,
                format: RenderFormat::Png,
                color: RenderColor::Gray,
            },
            RENDER_TIMEOUT,
        ) {
            Ok((Response::Render, body)) => body,
            other => unreachable!("rendering page {page}: {other:?}"),
        };
        writer
            .add_page(&png, PageSize::Fit)
            .expect("adding the page");
    }
    writer.finish().expect("writing the scan")
}

/// Every page of the PDF at `path` read by the worker.
fn read_all(worker: &mut WorkerProcess, path: &Path) -> Vec<OcrPage> {
    let count = open(worker, path);
    (1..=count)
        .map(
            |page| match worker.request(&Request::Ocr { page }, OCR_TIMEOUT) {
                Ok((Response::Ocr, body)) => serde_json::from_slice(&body).expect("an OcrPage"),
                other => unreachable!("reading page {page}: {other:?}"),
            },
        )
        .collect()
}

/// `pdf` with the text layer of `results`.
fn layered(pdf: &[u8], results: &[OcrPage]) -> Vec<u8> {
    let mut doc = Document::load_mem(pdf).expect("lopdf reads the PDF");
    let pages: Vec<(usize, &OcrPage)> = results.iter().enumerate().collect();
    add_text_layer(&mut doc, &pages).expect("adding the text layer");
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("saving");
    out
}

/// pdfium binds once per process, and this build of pdfium-render does not
/// lock it for threads, so the tests take turns.
static PDFIUM_TURN: Mutex<()> = Mutex::new(());

fn with_pdfium<R>(f: impl FnOnce(&Pdfium) -> R) -> R {
    let _turn = PDFIUM_TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let path = Pdfium::pdfium_platform_library_name_at_path(&worker_paths::library_dir());
    let pdfium = match Pdfium::bind_to_library(&path) {
        Ok(bindings) => Pdfium::new(bindings),
        // Another test bound it first; the default instance takes those bindings.
        Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Pdfium::default(),
        Err(e) => unreachable!("binding pdfium at {}: {e:?}", path.display()),
    };
    f(&pdfium)
}

fn without_spaces(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
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

/// Whether `point` is inside the convex quad grown by `margin`.
fn inside(quad: &Quad, point: [f64; 2], margin: f64) -> bool {
    let sides: Vec<f64> = (0..4)
        .map(|i| {
            let (a, b) = (quad[i], quad[(i + 1) % 4]);
            let edge = [b[0] - a[0], b[1] - a[1]];
            let len = edge[0].hypot(edge[1]).max(1e-9);
            (edge[0] * (point[1] - a[1]) - edge[1] * (point[0] - a[0])) / len
        })
        .collect();
    sides.iter().all(|s| *s >= -margin) || sides.iter().all(|s| *s <= margin)
}

/// A point of the page's user space on the page as shown, in points.
fn shown(page: &PdfPage<'_>, config: &PdfRenderConfig, x: f32, y: f32) -> [f64; 2] {
    let (dx, dy) = page
        .points_to_pixels(PdfPoints::new(x), PdfPoints::new(y), config)
        .expect("converting a point");
    [
        f64::from(dx) / DEVICE_PX_PER_PT,
        f64::from(dy) / DEVICE_PX_PER_PT,
    ]
}

/// Checks one page of the layered PDF against its OCR result: the text, a
/// search, and the place of every character.
fn check_page(page: &PdfPage<'_>, result: &OcrPage, label: &str) {
    let wanted = without_spaces(&result.text());
    assert!(
        wanted.chars().count() > 20,
        "{label}: OCR read too little: {wanted}"
    );
    let text = page.text().expect("the page's text");
    // Every character in pdfium's order. `PdfPageText::all` would keep only
    // those inside a rectangle from (0, 0) the size of the page, which leaves
    // out characters on a page whose box does not start there.
    let got: String = text
        .chars()
        .iter()
        .filter(|c| !c.is_generated().unwrap_or(false))
        .filter_map(|c| c.unicode_string())
        .collect();
    let got = without_spaces(&got);
    // pdfium may join the lines in another order than the blocks; the
    // characters must all be there.
    let mut sorted_got: Vec<char> = got.chars().collect();
    let mut sorted_wanted: Vec<char> = wanted.chars().collect();
    sorted_got.sort_unstable();
    sorted_wanted.sort_unstable();
    assert_eq!(sorted_got, sorted_wanted, "{label}: {got}");
    let rate = edits(&got, &wanted) as f64 / wanted.chars().count() as f64;
    assert!(rate <= 0.02, "{label}: {got} for {wanted}");

    // The longest line is found by a search for its first half.
    let line = result
        .blocks
        .iter()
        .flat_map(|b| &b.lines)
        .max_by_key(|l| l.text.chars().count())
        .expect("a line");
    let needle: String = line
        .text
        .chars()
        .take(line.text.chars().count() / 2)
        .collect();
    let search = text
        .search(&needle, &PdfSearchOptions::new())
        .expect("searching");
    assert!(
        search.find_next().is_some(),
        "{label}: no match for {needle}"
    );

    let config = PdfRenderConfig::new().set_target_size(
        (result.width_pt * DEVICE_PX_PER_PT).round() as i32,
        (result.height_pt * DEVICE_PX_PER_PT).round() as i32,
    );
    let ocr_chars: Vec<(String, Quad)> = result
        .blocks
        .iter()
        .flat_map(|b| &b.lines)
        .flat_map(|l| &l.words)
        .flat_map(|w| &w.chars)
        .map(|c| (c.text.clone(), c.quad))
        .collect();
    let mut checked = 0;
    for c in text.chars().iter() {
        let Some(s) = c.unicode_string() else {
            continue;
        };
        if s.trim().is_empty() || c.is_generated().unwrap_or(false) {
            continue;
        }
        let bounds = c.loose_bounds().expect("the character's box");
        let corners = [
            shown(page, &config, bounds.left().value, bounds.bottom().value),
            shown(page, &config, bounds.right().value, bounds.top().value),
        ];
        let centre = [
            (corners[0][0] + corners[1][0]) / 2.0,
            (corners[0][1] + corners[1][1]) / 2.0,
        ];
        let found = ocr_chars
            .iter()
            .any(|(text, quad)| *text == s && inside(quad, centre, BOX_TOLERANCE_PT));
        assert!(found, "{label}: {s} at {centre:?} is in no OCR box of it");
        checked += 1;
    }
    assert_eq!(checked, wanted.chars().count(), "{label}");
}

fn check(document: &PdfDocument<'_>, results: &[OcrPage], label: &str) {
    assert_eq!(document.pages().len() as usize, results.len(), "{label}");
    for (index, (page, result)) in document.pages().iter().zip(results).enumerate() {
        check_page(&page, result, &format!("{label}, page {}", index + 1));
    }
}

/// Reads `pdf` with the worker, adds the layer, and checks every page.
fn read_layer_and_check(
    worker: &mut WorkerProcess,
    scratch: &Scratch,
    pdf: &[u8],
    label: &str,
) -> Vec<u8> {
    let input = scratch.write(&format!("{label}.pdf"), pdf);
    let results = read_all(worker, &input);
    let out = layered(pdf, &results);
    with_pdfium(|pdfium| {
        let document = pdfium
            .load_pdf_from_byte_slice(&out, None)
            .expect("pdfium opens the layered PDF");
        check(&document, &results, label);
    });
    out
}

#[test]
fn horizontal_vertical_and_turned_pages_get_their_text() {
    let scratch = Scratch::new("plain");
    let mut worker = worker_paths::start();
    let pdf = scan(&mut worker, &[1, 2, 5], PdfConformance::Standard);
    read_layer_and_check(&mut worker, &scratch, &pdf, "plain");
}

/// The single page of `doc`.
fn only_page(doc: &Document) -> ObjectId {
    *doc.get_pages().values().next().expect("a page")
}

fn page_box(doc: &Document, page: ObjectId) -> [f64; 4] {
    let media = doc
        .get_dictionary(page)
        .and_then(|d| d.get(b"MediaBox"))
        .and_then(Object::as_array)
        .expect("a MediaBox");
    let values: Vec<f64> = media
        .iter()
        .map(|v| {
            v.as_float()
                .map(f64::from)
                .or_else(|_| v.as_i64().map(|i| i as f64))
                .expect("a number")
        })
        .collect();
    [values[0], values[1], values[2], values[3]]
}

/// Draws the page's contents through `matrix` (`a b c d e f`).
fn transform_contents(doc: &mut Document, page: ObjectId, matrix: [f64; 6]) {
    let contents = doc
        .get_dictionary(page)
        .and_then(|d| d.get(b"Contents"))
        .expect("contents")
        .clone();
    let operator = format!(
        "q {} cm\n",
        matrix
            .iter()
            .map(|v| format!("{v:.4}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let open = doc.add_object(Stream::new(Dictionary::new(), operator.into_bytes()));
    let close = doc.add_object(Stream::new(Dictionary::new(), b"\nQ\n".to_vec()));
    doc.get_dictionary_mut(page)
        .expect("the page")
        .set("Contents", vec![open.into(), contents, close.into()]);
}

fn save(doc: &mut Document) -> Vec<u8> {
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("saving");
    out
}

fn real_box(values: [f64; 4]) -> Vec<Object> {
    values.iter().map(|v| Object::Real(*v as f32)).collect()
}

#[test]
fn pages_shown_turned_by_their_rotate_entry_get_their_text() {
    let scratch = Scratch::new("rotate");
    let mut worker = worker_paths::start();
    let pdf = scan(&mut worker, &[1], PdfConformance::Standard);
    for rotate in [90, 180, 270] {
        // The page is drawn turned the other way and shown upright by
        // /Rotate, as a scanner that sets /Rotate writes it.
        let mut doc = Document::load_mem(&pdf).expect("lopdf reads the scan");
        let page = only_page(&doc);
        let [_, _, w, h] = page_box(&doc, page);
        let (matrix, size) = match rotate {
            90 => ([0.0, 1.0, -1.0, 0.0, h, 0.0], [h, w]),
            180 => ([-1.0, 0.0, 0.0, -1.0, w, h], [w, h]),
            _ => ([0.0, -1.0, 1.0, 0.0, 0.0, w], [h, w]),
        };
        transform_contents(&mut doc, page, matrix);
        let dict = doc.get_dictionary_mut(page).expect("the page");
        dict.set("MediaBox", real_box([0.0, 0.0, size[0], size[1]]));
        dict.set("Rotate", i64::from(rotate));
        let rotated = save(&mut doc);
        read_layer_and_check(&mut worker, &scratch, &rotated, &format!("rotate {rotate}"));
    }
}

#[test]
fn a_page_whose_boxes_do_not_start_at_the_origin_gets_its_text() {
    let scratch = Scratch::new("boxes");
    let mut worker = worker_paths::start();
    let pdf = scan(&mut worker, &[2], PdfConformance::Standard);
    let mut doc = Document::load_mem(&pdf).expect("lopdf reads the scan");
    let page = only_page(&doc);
    let [_, _, w, h] = page_box(&doc, page);
    let (dx, dy) = (100.0, 200.0);
    transform_contents(&mut doc, page, [1.0, 0.0, 0.0, 1.0, dx, dy]);
    let dict = doc.get_dictionary_mut(page).expect("the page");
    dict.set("MediaBox", real_box([dx, dy, dx + w, dy + h]));
    // The crop box leaves out a margin on every side, more on the left and bottom.
    dict.set(
        "CropBox",
        real_box([dx + 30.0, dy + 40.0, dx + w - 10.0, dy + h - 20.0]),
    );
    let moved = save(&mut doc);
    read_layer_and_check(&mut worker, &scratch, &moved, "boxes");
}

#[test]
fn a_page_with_user_unit_gets_its_text() {
    let scratch = Scratch::new("user-unit");
    let mut worker = worker_paths::start();
    let pdf = scan(&mut worker, &[1], PdfConformance::Standard);
    let mut doc = Document::load_mem(&pdf).expect("lopdf reads the scan");
    let page = only_page(&doc);
    doc.get_dictionary_mut(page)
        .expect("the page")
        .set("UserUnit", 2.0);
    let scaled = save(&mut doc);
    read_layer_and_check(&mut worker, &scratch, &scaled, "user unit");
}

#[test]
fn inherited_resources_a_contents_array_and_a_font_reference_are_kept() {
    let scratch = Scratch::new("resources");
    let mut worker = worker_paths::start();
    let pdf = scan(&mut worker, &[1], PdfConformance::Standard);
    let mut doc = Document::load_mem(&pdf).expect("lopdf reads the scan");
    let page = only_page(&doc);
    // The resources move to the page tree, with a font dictionary of their
    // own behind a reference; the contents become an array.
    let helvetica = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let fonts = doc.add_object(dictionary! { "F1" => helvetica, "OcrH" => helvetica });
    let mut resources = match doc.get_dictionary(page).and_then(|d| d.get(b"Resources")) {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(id)) => doc.get_dictionary(*id).expect("the resources").clone(),
        other => unreachable!("the page has resources: {other:?}"),
    };
    let page_dict = doc.get_dictionary_mut(page).expect("the page");
    page_dict.remove(b"Resources");
    resources.set("Font", fonts);
    let contents = page_dict.get(b"Contents").expect("contents").clone();
    page_dict.set("Contents", vec![contents]);
    let parent = page_dict
        .get(b"Parent")
        .and_then(Object::as_reference)
        .expect("a parent");
    doc.get_dictionary_mut(parent)
        .expect("the page tree")
        .set("Resources", resources);
    let inherited = save(&mut doc);

    let out = read_layer_and_check(&mut worker, &scratch, &inherited, "resources");
    let doc = Document::load_mem(&out).expect("lopdf reads the layered PDF");
    let page = only_page(&doc);
    let resources = doc
        .get_dictionary(page)
        .and_then(|d| d.get(b"Resources"))
        .and_then(Object::as_dict)
        .expect("the page's own resources");
    let fonts = resources
        .get(b"Font")
        .and_then(Object::as_dict)
        .expect("fonts");
    // The page's font keeps its name; the layer's take free ones.
    assert_eq!(
        fonts.get(b"F1").and_then(Object::as_reference).ok(),
        Some(helvetica)
    );
    assert_eq!(
        fonts.get(b"OcrH").and_then(Object::as_reference).ok(),
        Some(helvetica)
    );
    assert!(fonts.has(b"OcrH1") && fonts.has(b"OcrV"), "{fonts:?}");
    assert!(
        resources.has(b"XObject"),
        "the image is still there: {resources:?}"
    );
}

#[test]
fn an_encrypted_layered_pdf_keeps_its_text() {
    let scratch = Scratch::new("encrypted");
    let mut worker = worker_paths::start();
    let pdf = scan(&mut worker, &[1], PdfConformance::Standard);
    let input = scratch.write("plain.pdf", &pdf);
    let results = read_all(&mut worker, &input);
    // The layer is added before the encryption, as an output password asks.
    let encrypted = encrypt_pdf(
        layered(&pdf, &results),
        &OutputPassword::new(PASSWORD.to_owned()).expect("a valid password"),
    )
    .expect("encrypting");
    with_pdfium(|pdfium| {
        assert!(pdfium.load_pdf_from_byte_slice(&encrypted, None).is_err());
        let document = pdfium
            .load_pdf_from_byte_slice(&encrypted, Some(PASSWORD))
            .expect("pdfium opens it with the password");
        check(&document, &results, "encrypted");
    });
}

#[test]
fn a_pdf_a_scan_keeps_its_text() {
    let scratch = Scratch::new("pdfa");
    let mut worker = worker_paths::start();
    let pdf = scan(&mut worker, &[2], PdfConformance::PdfA2b);
    let out = read_layer_and_check(&mut worker, &scratch, &pdf, "pdfa");
    // The layer leaves the PDF/A identification as it was.
    let doc = Document::load_mem(&out).expect("lopdf reads the layered PDF");
    assert!(doc.catalog().expect("a catalog").has(b"Metadata"));
}
