//! What the worker cannot read with pdfium from a source PDF: the view settings of its catalog
//! and the frames of its links (design organize §1, "元の PDF を `lopdf` で読む").

use std::collections::HashMap;
use std::path::Path;

use lopdf::{Document, LoadOptions, ObjectId};

use super::annots::{LinkInfo, page_annotations, rects_match};
use super::relink::view_settings;
use super::types::{LinkStyle, SourceDetails, SourceLinkKey};

/// The largest size one stream may inflate to while a source is loaded. lopdf inflates object
/// streams and cross-reference streams when it loads, and by default nothing bounds one of them.
/// The largest in 38 real PDFs was 0.23 MB, so this leaves room, and one inflation still fits in
/// the margin the worker keeps (`RAW_MARGIN` there). An object in a stream over the limit is not
/// read; lopdf does not fail, so the page count or the missing value shows it.
pub const RAW_MAX_DECOMPRESSED: usize = 64 * 1024 * 1024;

/// Reads the source PDF at `path` with lopdf, which pdfium-render's view of a PDF lacks.
///
/// `password` is the one pdfium opened the file with, and `page_count` the number of pages pdfium
/// found. `want_view` asks for the view settings of the catalog; `links` are the links whose
/// frames are wanted. Nothing of the document is kept.
///
/// Returns `None` if the file cannot be read, or if lopdf finds a number of pages other than
/// `page_count`: lopdf reads a PDF that needs a password, and was not given it, as a document
/// without pages. A frame that cannot be found is `None` in [`SourceDetails::styles`].
pub fn read_source_details(
    path: &Path,
    password: Option<&str>,
    page_count: u32,
    want_view: bool,
    links: &[SourceLinkKey],
) -> Option<SourceDetails> {
    let options = LoadOptions {
        password: password.map(str::to_owned),
        max_decompressed_size: Some(RAW_MAX_DECOMPRESSED),
        ..LoadOptions::default()
    };
    let doc = Document::load_with_options(path, options).ok()?;
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    if pages.len() != page_count as usize {
        return None;
    }

    let view = if want_view { view_settings(&doc) } else { None };
    let mut on_page: HashMap<u32, Vec<LinkInfo>> = HashMap::new();
    let styles = links
        .iter()
        .map(|key| {
            let found = match on_page.get(&key.page) {
                Some(found) => found,
                None => {
                    let id = *pages.get(key.page as usize)?;
                    on_page
                        .entry(key.page)
                        .or_insert(page_annotations(&doc, id).links)
                }
            };
            let link = found.get(key.nth as usize)?;
            link.rect
                .is_some_and(|rect| rects_match(rect, key.rect))
                .then(|| LinkStyle(link.style.clone()))
        })
        .collect();
    Some(SourceDetails { view, styles })
}
