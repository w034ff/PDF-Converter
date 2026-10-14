//! Writes a plain PDF and a password-protected copy of it into a folder (design §11.2).
//!
//! Run with `cargo run -p pdfconv-core --example write_encrypted_samples -- <folder>`.
//! Every readable fixture image becomes a page the size of the image, in one PDF
//! (`plain.pdf`), and `encrypted.pdf` is the same PDF protected with [`PASSWORD`]. CI opens
//! both with qpdf, poppler and mupdf, which are not the viewers the unit tests use.
//!
//! An image that cannot be a page (a broken one, or a kind the app does not read) is skipped
//! and listed in the output. Any other failure ends the run with a non-zero exit code.

use std::error::Error;
use std::path::{Path, PathBuf};

use pdfconv_core::{
    AddImageResult, IMAGE_EXTENSIONS, OutputPassword, PageSize, PdfConformance, PdfWriter,
    ProbeError, encrypt_pdf,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Relative to the crate root.
const FIXTURES_DIR: &str = "tests/fixtures";
const CREATOR: &str = "PDF Converter encrypted samples";
const PLAIN_NAME: &str = "plain.pdf";
const ENCRYPTED_NAME: &str = "encrypted.pdf";

/// The password of `encrypted.pdf`. It holds spaces and symbols so that the check in
/// `.github/workflows/ci.yml` covers what a shell and the viewers do with them. The same
/// string is written in that file: change them together.
const PASSWORD: &str = "U15 pass-word!";

fn main() -> Result<()> {
    let out_dir = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("usage: write_encrypted_samples <output folder>")?,
    );
    std::fs::create_dir_all(&out_dir)?;

    let mut writer = PdfWriter::new(CREATOR, PdfConformance::Standard);
    let mut pages = 0;
    for path in image_fixtures()? {
        let name = path
            .file_name()
            .ok_or("fixture without a file name")?
            .to_string_lossy()
            .into_owned();
        let bytes = std::fs::read(&path)?;
        match writer.add_image(&bytes, PageSize::Fit, |_, _| {}, || false) {
            Ok(AddImageResult::Ok { pages_added }) => pages += pages_added,
            Ok(AddImageResult::Cancelled) => return Err("an image was cancelled".into()),
            Err(
                probe @ (ProbeError::DecodeFailed
                | ProbeError::UnsupportedFormat
                | ProbeError::TooLarge
                | ProbeError::TooManyPages),
            ) => println!("skipped {name}: {probe}"),
            Err(err) => return Err(format!("{name}: {err}").into()),
        }
    }

    let plain = writer.finish()?;
    std::fs::write(out_dir.join(PLAIN_NAME), &plain)?;
    let password = OutputPassword::new(PASSWORD.to_string())?;
    std::fs::write(out_dir.join(ENCRYPTED_NAME), encrypt_pdf(plain, &password)?)?;
    println!("wrote {pages} pages as {PLAIN_NAME} and {ENCRYPTED_NAME}");
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
