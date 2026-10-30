//! The last steps of an organized PDF: attach the bookmarks, links and view settings again, clean
//! the document information, optionally encrypt, and write (design organize §1, steps 7–9). The
//! worker runs pdfium and hands over its output; this module holds the parts that need lopdf,
//! which only this crate has.

mod annots;
mod relink;
mod source;
mod types;

use std::io::Write;

use lopdf::{Dictionary, Document, Object};

use crate::error::PdfWriteError;
use crate::pdf_encrypt::{OutputPassword, encrypt_document};

pub use source::{RAW_MAX_DECOMPRESSED, read_source_details};
pub use types::{
    DestinationView, LinkStyle, LinkStyleSource, LinkTarget, OutlineItem, OutputLink, Relinking,
    SourceDetails, SourceLinkKey, ViewSettings,
};

/// How far apart two rectangles of the same link may be, on any side, and still be taken for the
/// same link. pdfium reports a rectangle as it reads it from the file; this absorbs the rounding
/// of a rewritten file.
pub const LINK_RECT_TOLERANCE_PT: f32 = 0.5;

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
/// The link annotations that pdfium copied are removed, because they still lead into the source
/// documents, and `relinking` puts the links, the bookmarks and the view settings in their
/// place. Objects nothing refers to any more are dropped. With `password`, the document is
/// encrypted in memory after that, so the bookmarks and links are encrypted too (design §4.3.1).
///
/// `pdf` is taken by value so that it is freed once lopdf has read it.
///
/// # Errors
///
/// Returns [`PdfWriteError`] if lopdf cannot read the PDF, `relinking` names a page the PDF does
/// not have, the OS cannot give random bytes, or `out` fails. `pdf` is the output of pdfium in the
/// worker (design §9), so a failure to read it means a bug, not a bad input.
pub fn finish_organized<W: Write>(
    pdf: Vec<u8>,
    creator: &str,
    relinking: Relinking,
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
    // Drops the objects nothing refers to, and so the copied links, before the new ones are added.
    relink::relink(&mut doc, relinking)?;

    if let Some(password) = password {
        encrypt_document(&mut doc, password)?;
    }
    doc.save_to(out).map_err(write_failed)
}
