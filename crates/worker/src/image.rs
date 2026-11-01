//! The requests about image files: the thumbnail of a list item and the check of the pixels
//! before an image is written into a PDF (design §5.1, §6.1, §6.2).
//!
//! Neither loads pdfium or touches the open document. They decode inside the worker so that a
//! decoder that stops or crashes is contained by the worker's time and memory limits (design
//! §5.3).

use std::fs::File;
use std::io::{self, BufReader, Write};
use std::path::Path;

use pdfconv_core::{ProbeError, thumbnail_png, verify_image};

use crate::protocol::{Response, write_message};

/// An error response with the code and detail of `error` (design §6.6).
fn probe_error(error: ProbeError) -> Response {
    Response::Error {
        code: error.code().into(),
        detail: error.detail(),
    }
}

/// Opens the image at `path`. It is read as it is decoded, not held whole: a multi-page TIFF can
/// be larger than `WORKER_MEMORY_LIMIT`.
fn open_file(path: &Path) -> Result<BufReader<File>, Response> {
    File::open(path)
        .map(BufReader::new)
        .map_err(|_| probe_error(ProbeError::ReadFailed))
}

/// Makes the thumbnail of the image at `path`: the PNG is the body of the answer.
pub(crate) fn thumbnail(path: &Path) -> (Response, Vec<u8>) {
    match open_file(path).and_then(|file| thumbnail_png(file).map_err(probe_error)) {
        Ok(png) => (Response::ImageThumbnail, png),
        Err(response) => (response, Vec::new()),
    }
}

/// Reads every pixel of the image at `path`. A TIFF answers `ImageCheckProgress` after each
/// page, and every request ends with `ImageChecked` or an error.
///
/// # Errors
///
/// Returns the I/O error of `out`; a failure of the check itself is an answer, not an error.
pub(crate) fn check<W: Write>(path: &Path, out: &mut W) -> io::Result<()> {
    let file = match open_file(path) {
        Ok(file) => file,
        Err(response) => return write_message(out, &response, &[]),
    };
    let mut write_error = None;
    let result = verify_image(file, |done| {
        if write_error.is_none() {
            write_error = write_message(out, &Response::ImageCheckProgress { done }, &[]).err();
        }
    });
    if let Some(error) = write_error {
        return Err(error);
    }
    let response = match result {
        Ok(()) => Response::ImageChecked,
        Err(error) => probe_error(error),
    };
    write_message(out, &response, &[])
}
