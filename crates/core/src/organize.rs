//! The last steps of an organized PDF: clean the document information, optionally encrypt, and
//! write (design organize §1, steps 7–9). The worker runs pdfium and hands over its output; this
//! module holds the parts that need lopdf, which only this crate has.

use std::io::Write;

use lopdf::{Dictionary, Document, Object};

use crate::error::PdfWriteError;
use crate::pdf_encrypt::{OutputPassword, encrypt_document};

fn write_failed(err: impl std::fmt::Display) -> PdfWriteError {
    PdfWriteError(err.to_string())
}

/// A PDF text string: PDFDocEncoding-compatible ASCII as it is, anything else as UTF-16BE with a
/// byte order mark.
fn text_string(text: &str) -> Object {
    if text.is_ascii() {
        return Object::string_literal(text);
    }
    let mut bytes = vec![0xFE, 0xFF];
    bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
    Object::string_literal(bytes)
}

/// Rewrites the PDF that pdfium wrote for an organized output and writes it to `out`.
///
/// The document information becomes a new dictionary holding only `/Creator` (`creator`), and the
/// catalog loses its `/Metadata`, so nothing of the source files' authors, dates or XMP survives.
/// Objects nothing refers to any more are dropped. With `password`, the document is encrypted in
/// memory before it is written (design §4.3.1). The links that were copied are left as they are.
///
/// `pdf` is taken by value so that it is freed once lopdf has read it.
///
/// # Errors
///
/// Returns [`PdfWriteError`] if lopdf cannot read the PDF, the OS cannot give random bytes, or
/// `out` fails. `pdf` is the output of pdfium in the worker (design §9), so a failure to read it
/// means a bug, not a bad input.
pub fn finish_organized<W: Write>(
    pdf: Vec<u8>,
    creator: &str,
    password: Option<&OutputPassword>,
    out: &mut W,
) -> Result<(), PdfWriteError> {
    let mut doc = Document::load_mem(&pdf).map_err(write_failed)?;
    drop(pdf);

    let mut info = Dictionary::new();
    info.set(b"Creator", text_string(creator));
    let info_id = doc.add_object(Object::Dictionary(info));
    doc.trailer.set(b"Info", Object::Reference(info_id));
    doc.catalog_mut().map_err(write_failed)?.remove(b"Metadata");
    doc.prune_objects();

    if let Some(password) = password {
        encrypt_document(&mut doc, password)?;
    }
    doc.save_to(out).map_err(write_failed)
}
