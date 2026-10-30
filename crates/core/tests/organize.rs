//! `finish_organized` on a PDF that carries document information and XMP (design organize §1,
//! steps 7–9). The input is a fixture instead of pdfium's output, so no pdfium is needed.

use std::path::PathBuf;

use lopdf::{Dictionary, Document, LoadOptions, Object};
use pdfconv_core::{OutputPassword, finish_organized};

#[path = "support/organize_fixtures.rs"]
mod organize_fixtures;

const CREATOR: &str = "PDF Converter 0.1.0";
const PASSWORD: &str = "organize pass-1";
const WRONG_PASSWORD: &str = "organize pass-2";
const PAGES: usize = 3;

fn meta_pdf() -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/organize_meta.pdf"),
    )
    .expect("reading organize_meta.pdf")
}

fn finish(creator: &str, password: Option<&str>) -> Vec<u8> {
    let password = password.map(|text| OutputPassword::new(text.to_string()).expect("a password"));
    let mut out = Vec::new();
    finish_organized(meta_pdf(), creator, password.as_ref(), &mut out).expect("finishing");
    out
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn dict<'a>(parent: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
    parent.get(key).and_then(Object::as_dict).ok()
}

fn integer(parent: &Dictionary, key: &[u8]) -> Option<i64> {
    parent.get(key).and_then(Object::as_i64).ok()
}

fn information(doc: &Document) -> &Dictionary {
    let id = doc
        .trailer
        .get(b"Info")
        .and_then(Object::as_reference)
        .expect("the trailer names an information dictionary");
    doc.get_dictionary(id).expect("the information dictionary")
}

#[test]
fn the_fixture_carries_what_the_output_must_not() {
    // Without this the next test would pass on an input that has nothing to remove.
    let input = meta_pdf();
    for text in [
        organize_fixtures::META_AUTHOR,
        organize_fixtures::META_PRODUCER,
        organize_fixtures::META_XMP,
        organize_fixtures::META_CREATION_DIGITS,
    ] {
        assert!(contains(&input, text), "{text}");
    }
}

#[test]
fn the_information_holds_only_the_creator_and_the_xmp_is_gone() {
    use organize_fixtures::*;
    let out = finish(CREATOR, None);

    let doc = Document::load_mem(&out).expect("reading the output");
    assert_eq!(doc.get_pages().len(), PAGES);
    let info = information(&doc);
    let keys: Vec<&[u8]> = info.iter().map(|(key, _)| key.as_slice()).collect();
    assert_eq!(keys, [&b"Creator"[..]]);
    assert_eq!(
        info.get(b"Creator")
            .and_then(Object::as_str)
            .expect("the creator"),
        CREATOR.as_bytes()
    );
    assert!(
        doc.catalog()
            .expect("the catalog")
            .get(b"Metadata")
            .is_err()
    );

    for text in [
        META_AUTHOR,
        META_TITLE,
        META_CREATOR,
        META_PRODUCER,
        META_CREATION_DIGITS,
        META_XMP,
    ] {
        assert!(!contains(&out, text), "{text} is still in the output");
    }
}

#[test]
fn a_creator_outside_ascii_is_written_as_utf16() {
    let creator = "PDF変換 1.0";
    let out = finish(creator, None);
    let doc = Document::load_mem(&out).expect("reading the output");
    let written = information(&doc)
        .get(b"Creator")
        .and_then(Object::as_str)
        .expect("the creator");
    let mut expected = vec![0xFE, 0xFF];
    expected.extend(creator.encode_utf16().flat_map(u16::to_be_bytes));
    assert_eq!(written, expected);
}

#[test]
fn a_password_encrypts_the_output_in_place() {
    let out = finish(CREATOR, Some(PASSWORD));

    let opened = Document::load_mem_with_options(&out, LoadOptions::with_password(PASSWORD))
        .expect("reading the output with the password");
    assert_eq!(opened.get_pages().len(), PAGES);
    assert!(
        Document::load_mem_with_options(&out, LoadOptions::with_password(WRONG_PASSWORD)).is_err(),
        "a wrong password reads it"
    );

    let raw = Document::load_mem(&out).expect("reading the output as it is");
    let encrypt = raw.get_encrypted().expect("the /Encrypt dictionary");
    assert_eq!(integer(encrypt, b"Length"), Some(256));
    let std_cf = dict(encrypt, b"CF")
        .and_then(|filters| dict(filters, b"StdCF"))
        .expect("the StdCF crypt filter");
    assert_eq!(integer(std_cf, b"Length"), Some(32));
    let adbe = dict(opened.catalog().expect("the catalog"), b"Extensions")
        .and_then(|extensions| dict(extensions, b"ADBE"))
        .expect("the ADBE extensions");
    assert_eq!(integer(adbe, b"ExtensionLevel"), Some(8));

    // The information is the one without the sources' strings, and now it is unreadable.
    assert!(!contains(&out, CREATOR));
    assert!(!contains(&out, organize_fixtures::META_AUTHOR));
    assert!(
        information(&opened).get(b"Author").is_err(),
        "no author after decrypting"
    );
}

#[test]
fn bytes_that_are_not_a_pdf_are_a_write_error() {
    let mut out = Vec::new();
    assert!(finish_organized(b"not a pdf".to_vec(), CREATOR, None, &mut out).is_err());
}
