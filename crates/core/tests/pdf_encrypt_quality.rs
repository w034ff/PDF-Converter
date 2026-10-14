//! Quality tests for the password protection of written PDFs (design §11.2).
//!
//! pdfium does not support multithreaded use in one process, so everything that renders runs
//! in a single `#[test]` function, as in `pdf_quality.rs`.

use std::path::{Path, PathBuf};

use lopdf::encryption::EncryptionState;
use lopdf::{Dictionary, Document, LoadOptions, Object};
use pdfconv_core::{
    IMAGE_EXTENSIONS, OutputPassword, PageOrientation, PageSize, PaperSize, PdfConformance,
    PdfWriter, encrypt_pdf,
};
use pdfium_render::prelude::*;

const A4_AUTO: PageSize = PageSize::Paper(PaperSize::A4, PageOrientation::Auto);

/// The creator string written by [`PdfWriter`] in these tests: plain text in an ordinary PDF,
/// so it must be gone from the encrypted one.
const CREATOR: &str = "PDF Converter 0.1.0";

const PASSWORD: &str = "U15 pass-word!";
const WRONG_PASSWORD: &str = "U15 pass-word?";

/// Scale for the rendered pages; the comparison is exact, so a small page is enough.
const RENDER_SCALE: f32 = 0.5;

fn library_dir() -> PathBuf {
    let os_dir = if cfg!(windows) {
        "windows/bin"
    } else {
        "linux/lib"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri/pdfium")
        .join(os_dir)
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Every image of the fixtures folder that becomes a page, merged into one PDF.
fn merged_pdf(page_size: PageSize) -> (Vec<u8>, u32) {
    let mut names: Vec<String> = std::fs::read_dir(fixtures_dir())
        .expect("reading the fixtures folder")
        .map(|entry| {
            entry
                .expect("reading a fixtures entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| {
            Path::new(name)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext))
        })
        .collect();
    names.sort();

    let mut writer = PdfWriter::new(CREATOR, PdfConformance::Standard);
    let mut pages = 0;
    for name in names {
        let bytes = std::fs::read(fixtures_dir().join(&name)).expect("reading a fixture");
        // The unreadable fixtures (corrupt, too large, unsupported) add no page.
        if let Ok(pdfconv_core::AddImageResult::Ok { pages_added }) =
            writer.add_image(&bytes, page_size, |_, _| {}, || false)
        {
            pages += pages_added;
        }
    }
    (writer.finish().expect("finishing the PDF"), pages)
}

fn password(text: &str) -> OutputPassword {
    OutputPassword::new(text.to_string()).expect("a valid password")
}

/// Whether pdfium refuses to open `pdf` with `password` because the password is wrong or missing.
fn is_refused_for_password(pdfium: &Pdfium, pdf: &[u8], password: Option<&str>) -> bool {
    matches!(
        pdfium.load_pdf_from_byte_slice(pdf, password),
        Err(PdfiumError::PdfiumLibraryInternalError(
            PdfiumInternalError::PasswordError
        ))
    )
}

/// The RGBA bytes of every page, rendered at [`RENDER_SCALE`].
fn render_pages(pdfium: &Pdfium, pdf: &[u8], password: Option<&str>) -> Vec<(i32, i32, Vec<u8>)> {
    let document = pdfium
        .load_pdf_from_byte_slice(pdf, password)
        .expect("opening the PDF");
    let config = PdfRenderConfig::new().scale_page_by_factor(RENDER_SCALE);
    document
        .pages()
        .iter()
        .map(|page| {
            let bitmap = page.render_with_config(&config).expect("rendering a page");
            (bitmap.width(), bitmap.height(), bitmap.as_rgba_bytes())
        })
        .collect()
}

fn dict<'a>(parent: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
    parent.get(key).and_then(Object::as_dict).ok()
}

fn integer(parent: &Dictionary, key: &[u8]) -> Option<i64> {
    parent.get(key).and_then(Object::as_i64).ok()
}

fn name<'a>(parent: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    parent.get(key).and_then(Object::as_name).ok()
}

/// Reads the encryption dictionary and the catalog back with lopdf (design §4.3.1).
fn assert_encryption_dictionary(encrypted: &[u8], pages: u32) {
    // The password opens the PDF again, and the document has all its pages.
    let opened = Document::load_mem_with_options(encrypted, LoadOptions::with_password(PASSWORD))
        .expect("reading the encrypted PDF with the password");
    assert_eq!(opened.get_pages().len(), pages as usize);

    // The encryption dictionary itself is not encrypted, so it can be read as written.
    let raw = Document::load_mem(encrypted).expect("reading the encrypted PDF as it is");
    let encrypt = raw.get_encrypted().expect("the /Encrypt dictionary");
    assert_eq!(name(encrypt, b"Filter"), Some(&b"Standard"[..]));
    assert_eq!(integer(encrypt, b"V"), Some(5));
    assert_eq!(integer(encrypt, b"R"), Some(6));
    assert_eq!(integer(encrypt, b"Length"), Some(256));
    assert_eq!(name(encrypt, b"StmF"), Some(&b"StdCF"[..]));
    assert_eq!(name(encrypt, b"StrF"), Some(&b"StdCF"[..]));
    let std_cf = dict(encrypt, b"CF")
        .and_then(|filters| dict(filters, b"StdCF"))
        .expect("the StdCF crypt filter");
    assert_eq!(name(std_cf, b"CFM"), Some(&b"AESV3"[..]));
    assert_eq!(integer(std_cf, b"Length"), Some(32));

    // The catalog may sit in an object stream, which only the password can unpack.
    let catalog = opened.catalog().expect("the catalog");
    let adbe = dict(catalog, b"Extensions")
        .and_then(|extensions| dict(extensions, b"ADBE"))
        .expect("the ADBE extensions");
    assert_eq!(name(adbe, b"BaseVersion"), Some(&b"1.7"[..]));
    assert_eq!(integer(adbe, b"ExtensionLevel"), Some(8));
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[test]
fn encrypted_pdf_needs_the_password_and_draws_like_the_plain_one() {
    let lib_path = Pdfium::pdfium_platform_library_name_at_path(&library_dir());
    let bindings = Pdfium::bind_to_library(&lib_path)
        .unwrap_or_else(|e| panic!("failed to bind to pdfium at {lib_path:?}: {e:?}"));
    let pdfium = Pdfium::new(bindings);

    for (label, page_size) in [("image size", PageSize::Fit), ("A4", A4_AUTO)] {
        let (plain, pages) = merged_pdf(page_size);
        assert!(pages > 1, "{label}: the merged PDF has several pages");
        let encrypted = encrypt_pdf(plain.clone(), &password(PASSWORD)).expect("encrypting");

        // Without the password, and with a wrong one, pdfium refuses to open it. The first
        // case also fails when the owner password is empty: pdfium tries the empty password.
        assert!(
            is_refused_for_password(&pdfium, &encrypted, None),
            "{label}: no password"
        );
        assert!(
            is_refused_for_password(&pdfium, &encrypted, Some(WRONG_PASSWORD)),
            "{label}: wrong password"
        );

        // With the password, every page is drawn exactly as in the plain PDF.
        let expected = render_pages(&pdfium, &plain, None);
        let actual = render_pages(&pdfium, &encrypted, Some(PASSWORD));
        assert_eq!(actual.len(), pages as usize, "{label}: page count");
        assert_eq!(actual.len(), expected.len(), "{label}: page count");
        for (index, (act, exp)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!((act.0, act.1), (exp.0, exp.1), "{label}: page {index} size");
            assert!(act.2 == exp.2, "{label}: page {index} differs from plain");
        }

        assert_encryption_dictionary(&encrypted, pages);

        // The creator is in the plain PDF and cannot be read from the encrypted one.
        assert!(
            contains(&plain, CREATOR),
            "{label}: the plain PDF names the creator"
        );
        assert!(
            !contains(&encrypted, CREATOR),
            "{label}: the creator is readable"
        );
    }
}

/// The file encryption key, which only the password unlocks.
fn file_key(encrypted: &[u8]) -> Vec<u8> {
    let raw = Document::load_mem(encrypted).expect("reading the encrypted PDF as it is");
    EncryptionState::decode(&raw, PASSWORD)
        .expect("the password unlocks the key")
        .file_encryption_key()
        .to_vec()
}

#[test]
fn encrypting_twice_gives_different_bytes_and_a_different_key() {
    let (plain, _) = merged_pdf(PageSize::Fit);
    let password = password(PASSWORD);
    let first = encrypt_pdf(plain.clone(), &password).expect("encrypting");
    let second = encrypt_pdf(plain, &password).expect("encrypting");
    assert!(first != second, "the key and salts are made for every call");

    // The initialization vectors differ on their own, so the bytes alone do not show that the
    // file key is new each time.
    let (first_key, second_key) = (file_key(&first), file_key(&second));
    assert_eq!(first_key.len(), 32);
    assert!(
        first_key != second_key,
        "the file key is new for every call"
    );
    assert!(
        first_key.iter().any(|byte| *byte != 0),
        "the key is not zero"
    );
}
