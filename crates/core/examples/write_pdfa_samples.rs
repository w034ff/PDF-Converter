//! Writes PDF/A-2b samples of the fixture images into a folder (design §11.2).
//!
//! Run with `cargo run -p pdfconv-core --example write_pdfa_samples -- <folder>`.
//! Each image becomes a PDF the size of the image and a PDF on A4, and all of
//! them together become one merged PDF. CI validates the folder with veraPDF
//! because krilla reports its own PDF/A conformance.
//!
//! An image that cannot be a PDF/A page is skipped and listed in the output:
//! a broken one fails with `DecodeFailed`, and a CMYK JPEG with
//! `CmykNotAllowedInPdfA`. Any other failure ends the run with a non-zero exit code.

use std::error::Error;
use std::path::{Path, PathBuf};

use pdfconv_core::{
    IMAGE_EXTENSIONS, PageOrientation, PageSize, PaperSize, PdfConformance, PdfWriter, ProbeError,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Relative to the crate root.
const FIXTURES_DIR: &str = "tests/fixtures";
const CREATOR: &str = "PDF Converter PDF/A samples";
const MERGED_NAME: &str = "merged.pdf";
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
                Some(probe @ (ProbeError::DecodeFailed | ProbeError::CmykNotAllowedInPdfA)) => {
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
    println!(
        "wrote {} images as PDF/A-2b, one each at image size and on A4, and {MERGED_NAME}",
        usable.len()
    );
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
