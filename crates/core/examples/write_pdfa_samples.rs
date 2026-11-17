//! Writes PDF/A-2b samples of the fixture images into a folder (design §11.2).
//!
//! Run with `cargo run -p pdfconv-core --example write_pdfa_samples -- <folder>`.
//! Each image becomes a PDF the size of the image and a PDF on A4, and all of
//! them together become one merged PDF. One more, `text_layer.pdf`, is the
//! first usable image with an OCR text layer added by lopdf (design ocr §9):
//! horizontal and vertical text and a character outside the BMP. CI validates
//! the folder with veraPDF because krilla reports its own PDF/A conformance.
//!
//! An image that cannot be a PDF/A page is skipped and listed in the output:
//! a broken one fails with `DecodeFailed`, and a CMYK JPEG with
//! `CmykNotAllowedInPdfA`. Any other failure ends the run with a non-zero exit code.

use std::error::Error;
use std::path::{Path, PathBuf};

use pdfconv_core::ocr::{
    OcrBlock, OcrChar, OcrLine, OcrPage, OcrWord, Orientation, Quad, quad_bbox,
};
use pdfconv_core::text_layer::add_text_layer;
use pdfconv_core::{
    IMAGE_EXTENSIONS, PageOrientation, PageSize, PaperSize, PdfConformance, PdfWriter, ProbeError,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Relative to the crate root.
const FIXTURES_DIR: &str = "tests/fixtures";
const CREATOR: &str = "PDF Converter PDF/A samples";
const MERGED_NAME: &str = "merged.pdf";
const TEXT_LAYER_NAME: &str = "text_layer.pdf";
const A4_AUTO: PageSize = PageSize::Paper(PaperSize::A4, PageOrientation::Auto);

fn main() -> Result<()> {
    let out_dir = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("usage: write_pdfa_samples <output folder>")?,
    );
    std::fs::create_dir_all(&out_dir)?;

    let mut usable: Vec<(String, Vec<u8>)> = Vec::new();
    for path in image_fixtures()? {
        let name = path
            .file_name()
            .ok_or("fixture without a file name")?
            .to_string_lossy()
            .into_owned();
        let bytes = std::fs::read(&path)?;

        match write_pdf(
            &out_dir,
            &format!("{name}.fit.pdf"),
            &[&bytes],
            PageSize::Fit,
        ) {
            Ok(()) => {}
            Err(err) => match err.downcast_ref::<ProbeError>() {
                Some(
                    probe @ (ProbeError::DecodeFailed
                    | ProbeError::CmykNotAllowedInPdfA
                    | ProbeError::UnsupportedFormat
                    | ProbeError::TooLarge
                    | ProbeError::TooManyPages),
                ) => {
                    println!("skipped {name}: {probe}");
                    continue;
                }
                _ => return Err(format!("{name}: {err}").into()),
            },
        }
        write_pdf(&out_dir, &format!("{name}.a4.pdf"), &[&bytes], A4_AUTO)
            .map_err(|err| format!("{name}: {err}"))?;
        usable.push((name, bytes));
    }

    let all: Vec<&[u8]> = usable.iter().map(|(_, bytes)| bytes.as_slice()).collect();
    write_pdf(&out_dir, MERGED_NAME, &all, A4_AUTO)?;
    let (_, first) = usable.first().ok_or("no usable image")?;
    write_text_layer_pdf(&out_dir, first)?;
    println!(
        "wrote {} images as PDF/A-2b, one each at image size and on A4, {MERGED_NAME} and \
         {TEXT_LAYER_NAME}",
        usable.len()
    );
    Ok(())
}

/// One line of OCR text, its characters side by side (or one under another)
/// from `start`, each `size` points large.
fn line(text: &str, start: [f64; 2], size: f64, orientation: Orientation) -> OcrLine {
    let chars: Vec<OcrChar> = text
        .chars()
        .enumerate()
        .map(|(i, c)| {
            let offset = i as f64 * size;
            let [x, y] = start;
            let quad: Quad = match orientation {
                Orientation::Horizontal => [
                    [x + offset, y],
                    [x + offset + size, y],
                    [x + offset + size, y + size],
                    [x + offset, y + size],
                ],
                // Top-right first.
                Orientation::Vertical => [
                    [x + size, y + offset],
                    [x + size, y + offset + size],
                    [x, y + offset + size],
                    [x, y + offset],
                ],
            };
            OcrChar {
                quad,
                text: c.to_string(),
                confidence: 1.0,
            }
        })
        .collect();
    let quad = [
        chars[0].quad[0],
        chars[chars.len() - 1].quad[1],
        chars[chars.len() - 1].quad[2],
        chars[0].quad[3],
    ];
    OcrLine {
        quad,
        bbox: quad_bbox(&quad),
        orientation,
        text: text.to_owned(),
        confidence: 1.0,
        words: vec![OcrWord {
            quad,
            text: text.to_owned(),
            confidence: 1.0,
            chars,
        }],
    }
}

/// `image` at its own size, as PDF/A-2b, with a text layer over it.
fn write_text_layer_pdf(out_dir: &Path, image: &[u8]) -> Result<()> {
    let mut writer = PdfWriter::new(CREATOR, PdfConformance::PdfA2b);
    writer.add_page(image, PageSize::Fit)?;
    let mut doc = lopdf::Document::load_mem(&writer.finish()?)?;
    let page = *doc.get_pages().values().next().ok_or("no page")?;
    let media = doc
        .get_dictionary(page)?
        .get(b"MediaBox")?
        .as_array()?
        .clone();
    let size: Vec<f64> = media
        .iter()
        .map(|v| {
            v.as_float()
                .map(f64::from)
                .or_else(|_| v.as_i64().map(|i| i as f64))
        })
        .collect::<std::result::Result<_, _>>()?;
    let (width, height) = (size[2] - size[0], size[3] - size[1]);
    let unit = width.min(height) / 20.0;
    let lines = vec![
        line(
            "OCR テキスト 𠮷",
            [unit, unit],
            unit,
            Orientation::Horizontal,
        ),
        line(
            "縦書きの列",
            [width - 2.0 * unit, 3.0 * unit],
            unit,
            Orientation::Vertical,
        ),
    ];
    let result = OcrPage {
        width_pt: width,
        height_pt: height,
        engine: "write_pdfa_samples".into(),
        blocks: lines
            .into_iter()
            .map(|line| OcrBlock {
                bbox: line.bbox,
                orientation: line.orientation,
                lines: vec![line],
            })
            .collect(),
    };
    add_text_layer(&mut doc, &[(0, &result)])?;
    doc.save(out_dir.join(TEXT_LAYER_NAME))?;
    Ok(())
}

/// The images of the fixtures folder, in name order.
fn image_fixtures() -> Result<Vec<PathBuf>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES_DIR);
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let is_image = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()));
        if is_image {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Writes one PDF/A-2b with a page per image, all on `page_size`.
fn write_pdf(out_dir: &Path, name: &str, images: &[&[u8]], page_size: PageSize) -> Result<()> {
    let mut writer = PdfWriter::new(CREATOR, PdfConformance::PdfA2b);
    for image in images {
        writer.add_page(image, page_size)?;
    }
    std::fs::write(out_dir.join(name), writer.finish()?)?;
    Ok(())
}
