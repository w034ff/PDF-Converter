//! The pixel check of input images and the writer that relies on it (design §4.3, §6.2).

use std::path::PathBuf;

use pdfconv_core::{AddImageResult, PageSize, PdfConformance, PdfWriter, ProbeError, verify_image};

const CREATOR: &str = "PDF Converter 0.1.0";

/// One fixture for each format the app reads, with the number of pages it has.
const READABLE: &[(&str, u32)] = &[
    ("photo.jpg", 1),
    ("dpi300.png", 1),
    ("deep16.png", 1),
    ("logo_alpha.webp", 1),
    ("opaque.bmp", 1),
    ("fax_standard.tif", 1),
    ("scan_g4.tif", 3),
];

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(path).expect("reading the fixture")
}

fn page_count(pdf: &[u8]) -> usize {
    lopdf::Document::load_mem(pdf)
        .expect("loading the written PDF")
        .get_pages()
        .len()
}

#[test]
fn verify_image_accepts_every_format() {
    for (name, _) in READABLE {
        verify_image(&fixture(name), |_| {}).unwrap_or_else(|err| panic!("{name}: {err:?}"));
    }
}

#[test]
fn verify_image_reports_each_tiff_page_and_no_page_of_other_formats() {
    for (name, pages) in READABLE {
        let mut reported = Vec::new();
        verify_image(&fixture(name), |done| reported.push(done)).expect("verifying the image");
        if name.ends_with(".tif") {
            assert_eq!(reported, (1..=*pages).collect::<Vec<_>>(), "{name}");
        } else {
            assert!(reported.is_empty(), "{name}: {reported:?}");
        }
    }
}

#[test]
fn verify_image_fails_on_a_corrupt_png() {
    assert_eq!(
        verify_image(&fixture("corrupt.png"), |_| {}),
        Err(ProbeError::DecodeFailed)
    );
}

#[test]
fn verify_image_fails_on_a_truncated_png() {
    let bytes = fixture("dpi300.png");
    let truncated = &bytes[..bytes.len() * 2 / 3];
    assert_eq!(
        verify_image(truncated, |_| {}),
        Err(ProbeError::DecodeFailed)
    );
}

#[test]
fn verify_image_fails_in_a_later_page_of_a_tiff() {
    let mut reported = Vec::new();
    let result = verify_image(&fixture("corrupt_page2.tif"), |done| reported.push(done));
    assert_eq!(result, Err(ProbeError::DecodeFailed));
    assert!(reported.len() < 2, "{reported:?}");
}

#[test]
fn verify_image_keeps_the_probe_errors() {
    assert_eq!(
        verify_image(b"not an image", |_| {}),
        Err(ProbeError::UnsupportedFormat)
    );
    assert_eq!(
        verify_image(&fixture("too_large_page2.tif"), |_| {}),
        Err(ProbeError::TooLarge)
    );
    assert_eq!(
        verify_image(&fixture("too_many_pages.tif"), |_| {}),
        Err(ProbeError::TooManyPages)
    );
}

#[test]
fn add_checked_image_writes_the_pages_that_add_image_writes() {
    for conformance in [PdfConformance::Standard, PdfConformance::PdfA2b] {
        for (name, pages) in READABLE {
            let bytes = fixture(name);

            let mut checked = PdfWriter::new(CREATOR, conformance);
            let result = checked
                .add_checked_image(bytes.clone(), PageSize::Fit, |_, _| {}, || false)
                .unwrap_or_else(|err| panic!("{name}: {err:?}"));
            assert_eq!(
                result,
                AddImageResult::Ok {
                    pages_added: *pages
                },
                "{name}"
            );
            let checked_pdf = checked.finish().expect("finishing the PDF");

            let mut plain = PdfWriter::new(CREATOR, conformance);
            let result = plain
                .add_image(&bytes, PageSize::Fit, |_, _| {}, || false)
                .unwrap_or_else(|err| panic!("{name}: {err:?}"));
            assert_eq!(
                result,
                AddImageResult::Ok {
                    pages_added: *pages
                },
                "{name}"
            );
            let plain_pdf = plain.finish().expect("finishing the PDF");

            assert_eq!(page_count(&checked_pdf), *pages as usize, "{name}");
            assert_eq!(page_count(&plain_pdf), *pages as usize, "{name}");
        }
    }
}

#[test]
fn add_checked_image_writes_the_same_bytes_as_add_image() {
    for (name, _) in READABLE {
        let bytes = fixture(name);

        let mut checked = PdfWriter::new(CREATOR, PdfConformance::Standard);
        checked
            .add_checked_image(bytes.clone(), PageSize::Fit, |_, _| {}, || false)
            .expect("adding the checked image");

        let mut plain = PdfWriter::new(CREATOR, PdfConformance::Standard);
        plain
            .add_image(&bytes, PageSize::Fit, |_, _| {}, || false)
            .expect("adding the image");

        assert_eq!(
            checked.finish().expect("finishing the PDF"),
            plain.finish().expect("finishing the PDF"),
            "{name}"
        );
    }
}

#[test]
fn add_checked_image_still_reads_bmp_and_tiff_pixels() {
    let mut writer = PdfWriter::new(CREATOR, PdfConformance::Standard);
    let mut truncated_bmp = fixture("opaque.bmp");
    truncated_bmp.truncate(truncated_bmp.len() * 2 / 3);
    assert_eq!(
        writer.add_checked_image(truncated_bmp, PageSize::Fit, |_, _| {}, || false),
        Err(ProbeError::DecodeFailed)
    );
    assert_eq!(
        writer.add_checked_image(
            fixture("corrupt_page2.tif"),
            PageSize::Fit,
            |_, _| {},
            || false
        ),
        Err(ProbeError::DecodeFailed)
    );
    writer
        .add_checked_image(fixture("photo.jpg"), PageSize::Fit, |_, _| {}, || false)
        .expect("the writer continues after a failure");
    assert_eq!(page_count(&writer.finish().expect("finishing the PDF")), 1);
}

#[test]
fn add_image_still_fails_on_a_corrupt_png() {
    let mut writer = PdfWriter::new(CREATOR, PdfConformance::Standard);
    assert_eq!(
        writer.add_image(&fixture("corrupt.png"), PageSize::Fit, |_, _| {}, || false),
        Err(ProbeError::DecodeFailed)
    );
}

#[test]
fn add_checked_image_cancels_without_adding_pages() {
    let mut writer = PdfWriter::new(CREATOR, PdfConformance::Standard);
    let result = writer
        .add_checked_image(fixture("scan_g4.tif"), PageSize::Fit, |_, _| {}, || true)
        .expect("cancelling");
    assert_eq!(result, AddImageResult::Cancelled);
}

#[test]
fn add_checked_image_refuses_cmyk_in_pdfa() {
    let mut writer = PdfWriter::new(CREATOR, PdfConformance::PdfA2b);
    assert_eq!(
        writer.add_checked_image(
            fixture("photo_cmyk.jpg"),
            PageSize::Fit,
            |_, _| {},
            || false
        ),
        Err(ProbeError::CmykNotAllowedInPdfA)
    );
}
