//! Writes the links, the bookmarks and the view settings into the output of pdfium (design
//! organize §1, step 7).

use std::collections::{BTreeMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::annots::{LinkInfo, PageAnnotations, copied_keys, page_annotations, rects_match};
use super::text_string;
use super::types::{
    DestinationView, LinkStyleSource, LinkTarget, OutlineItem, OutputLink, Relinking, ViewSettings,
};
use crate::error::PdfWriteError;

/// The `/F` of a link: print. The other flags would hide it or change how it is drawn.
const LINK_FLAGS: i64 = 4;

/// A link written without a frame: PDF draws a black one when `/Border` is missing.
const NO_BORDER: [i64; 3] = [0, 0, 0];

fn invalid(what: &str) -> PdfWriteError {
    PdfWriteError(what.to_owned())
}

/// Takes the copied links off the output's pages and drops the objects that nothing refers to
/// any more, then puts `relinking.links` on the pages, adds the bookmarks and sets the view
/// settings.
pub(super) fn relink(doc: &mut Document, relinking: Relinking) -> Result<(), PdfWriteError> {
    let Relinking {
        outline,
        links,
        view,
    } = relinking;
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();

    let copied = take_copied_links(doc, &pages)?;
    prune_unreachable(doc);
    attach_links(doc, &pages, &links, copied)?;
    let has_outline = !outline.is_empty();
    if has_outline {
        write_outline(doc, &outline, &pages)?;
    }
    if let Some(view) = view {
        set_view(doc, view, has_outline)?;
    }
    Ok(())
}

/// Drops the objects that cannot be reached from the trailer, as lopdf's `prune_objects` does.
/// That one looks each reference up in a list of all those met before, so its time grows with the
/// square of the objects, and a large output would outlast the worker's limit on the time
/// between answers.
fn prune_unreachable(doc: &mut Document) {
    let mut reached: HashSet<ObjectId> = HashSet::new();
    let mut pending: Vec<&Object> = doc.trailer.iter().map(|(_, value)| value).collect();
    while let Some(object) = pending.pop() {
        match object {
            Object::Reference(id) => {
                if reached.insert(*id)
                    && let Some(target) = doc.objects.get(id)
                {
                    pending.push(target);
                }
            }
            Object::Array(items) => pending.extend(items),
            Object::Dictionary(dict) => pending.extend(dict.iter().map(|(_, value)| value)),
            Object::Stream(stream) => pending.extend(stream.dict.iter().map(|(_, value)| value)),
            _ => {}
        }
    }
    doc.objects.retain(|id, _| reached.contains(id));
}

/// Removes the link annotations from every page's `/Annots` and returns, for each page, the links
/// that were there. A page left with no annotation loses the key.
fn take_copied_links(
    doc: &mut Document,
    pages: &[ObjectId],
) -> Result<Vec<Vec<LinkInfo>>, PdfWriteError> {
    let mut copied = Vec::with_capacity(pages.len());
    for &page_id in pages {
        let PageAnnotations { others, links } = page_annotations(doc, page_id);
        set_annotations(doc, page_id, others)?;
        copied.push(links);
    }
    Ok(copied)
}

fn set_annotations(
    doc: &mut Document,
    page_id: ObjectId,
    annotations: Vec<Object>,
) -> Result<(), PdfWriteError> {
    let Ok(page) = doc.get_dictionary_mut(page_id) else {
        return Err(invalid("a page of the output is not a dictionary"));
    };
    if annotations.is_empty() {
        page.remove(b"Annots");
    } else {
        page.set("Annots", Object::Array(annotations));
    }
    Ok(())
}

/// Adds `links` to their pages. `copied[n]` are the links that page `n` had before they were
/// taken off, where the frames of [`LinkStyleSource::Copied`] come from.
fn attach_links(
    doc: &mut Document,
    pages: &[ObjectId],
    links: &[OutputLink],
    copied: Vec<Vec<LinkInfo>>,
) -> Result<(), PdfWriteError> {
    let mut by_page: BTreeMap<usize, Vec<&OutputLink>> = BTreeMap::new();
    for link in links {
        let page = link.page as usize;
        if page >= pages.len() {
            return Err(invalid("a link sits on a page the output does not have"));
        }
        by_page.entry(page).or_default().push(link);
    }

    for (index, link_list) in by_page {
        let copied_on_page = &copied[index];
        let page_id = pages[index];
        let mut annots = annotations_of_page(doc, page_id);
        for link in link_list {
            let style = match &link.style {
                LinkStyleSource::Copied { nth } => copied_on_page
                    .get(*nth as usize)
                    .filter(|found| found.rect.is_some_and(|rect| rects_match(rect, link.rect)))
                    .map(|found| found.style.clone()),
                LinkStyleSource::Source(style) => style.as_ref().map(|style| style.0.clone()),
            };
            let annotation = link_annotation(link, style, page_id, pages)?;
            annots.push(Object::Reference(doc.add_object(annotation)));
        }
        set_annotations(doc, page_id, annots)?;
    }
    Ok(())
}

/// The entries of the page's `/Annots` as they stand.
fn annotations_of_page(doc: &Document, page_id: ObjectId) -> Vec<Object> {
    doc.get_dictionary(page_id)
        .and_then(|page| page.get(b"Annots"))
        .and_then(Object::as_array)
        .cloned()
        .unwrap_or_default()
}

fn link_annotation(
    link: &OutputLink,
    style: Option<Vec<(&'static [u8], Object)>>,
    page: ObjectId,
    pages: &[ObjectId],
) -> Result<Dictionary, PdfWriteError> {
    let mut annotation = Dictionary::new();
    annotation.set("Type", Object::Name(b"Annot".to_vec()));
    annotation.set("Subtype", Object::Name(b"Link".to_vec()));
    annotation.set(
        "Rect",
        Object::Array(link.rect.iter().map(|&side| Object::Real(side)).collect()),
    );
    match style {
        Some(style) => {
            for (key, value) in style {
                annotation.set(key, value);
            }
        }
        None => annotation.set(
            "Border",
            Object::Array(
                NO_BORDER
                    .iter()
                    .map(|&width| Object::Integer(width))
                    .collect(),
            ),
        ),
    }
    annotation.set("F", Object::Integer(LINK_FLAGS));
    annotation.set("P", Object::Reference(page));
    set_target(&mut annotation, &link.target, pages)?;
    Ok(annotation)
}

/// Sets `/A` (a URI) or `/Dest` (a page) of `dict`.
fn set_target(
    dict: &mut Dictionary,
    target: &LinkTarget,
    pages: &[ObjectId],
) -> Result<(), PdfWriteError> {
    match target {
        LinkTarget::Uri(uri) => {
            let mut action = Dictionary::new();
            action.set("S", Object::Name(b"URI".to_vec()));
            action.set("URI", Object::string_literal(uri.as_bytes()));
            dict.set("A", Object::Dictionary(action));
        }
        LinkTarget::Page { page, view } => {
            let page = pages
                .get(*page as usize)
                .ok_or_else(|| invalid("a destination names a page the output does not have"))?;
            dict.set("Dest", destination(*page, *view));
        }
    }
    Ok(())
}

fn destination(page: ObjectId, view: DestinationView) -> Object {
    let number = |value: Option<f32>| value.map_or(Object::Null, Object::Real);
    let name = |name: &str| Object::Name(name.as_bytes().to_vec());
    let mut array = vec![Object::Reference(page)];
    match view {
        DestinationView::Xyz { left, top, zoom } => {
            array.extend([name("XYZ"), number(left), number(top), number(zoom)]);
        }
        DestinationView::Fit => array.push(name("Fit")),
        DestinationView::FitH { top } => array.extend([name("FitH"), number(top)]),
        DestinationView::FitV { left } => array.extend([name("FitV"), number(left)]),
        DestinationView::FitR {
            left,
            bottom,
            right,
            top,
        } => array.extend([
            name("FitR"),
            Object::Real(left),
            Object::Real(bottom),
            Object::Real(right),
            Object::Real(top),
        ]),
        DestinationView::FitB => array.push(name("FitB")),
        DestinationView::FitBH { top } => array.extend([name("FitBH"), number(top)]),
        DestinationView::FitBV { left } => array.extend([name("FitBV"), number(left)]),
    }
    Object::Array(array)
}

/// Adds `/Outlines` to the catalog. Every item that has children is closed.
fn write_outline(
    doc: &mut Document,
    outline: &[OutlineItem],
    pages: &[ObjectId],
) -> Result<(), PdfWriteError> {
    let root = doc.new_object_id();
    let (first, last) = write_level(doc, outline, root, pages)?;
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Outlines".to_vec()));
    dict.set("First", Object::Reference(first));
    dict.set("Last", Object::Reference(last));
    dict.set("Count", Object::Integer(outline.len() as i64));
    doc.objects.insert(root, Object::Dictionary(dict));
    doc.catalog_mut()
        .map_err(|err| invalid(&err.to_string()))?
        .set("Outlines", Object::Reference(root));
    Ok(())
}

/// Writes `items` as children of `parent` and returns the first and the last of them. `items`
/// is not empty. The nesting depth is the caller's to bound.
fn write_level(
    doc: &mut Document,
    items: &[OutlineItem],
    parent: ObjectId,
    pages: &[ObjectId],
) -> Result<(ObjectId, ObjectId), PdfWriteError> {
    let ids: Vec<ObjectId> = items.iter().map(|_| doc.new_object_id()).collect();
    for (position, item) in items.iter().enumerate() {
        let mut dict = Dictionary::new();
        dict.set("Title", text_string(&item.title));
        dict.set("Parent", Object::Reference(parent));
        if position > 0 {
            dict.set("Prev", Object::Reference(ids[position - 1]));
        }
        if let Some(next) = ids.get(position + 1) {
            dict.set("Next", Object::Reference(*next));
        }
        if let Some(target) = &item.target {
            set_target(&mut dict, target, pages)?;
        }
        if !item.children.is_empty() {
            let (first, last) = write_level(doc, &item.children, ids[position], pages)?;
            dict.set("First", Object::Reference(first));
            dict.set("Last", Object::Reference(last));
            dict.set("Count", Object::Integer(-(item.children.len() as i64)));
        }
        doc.objects.insert(ids[position], Object::Dictionary(dict));
    }
    Ok((ids[0], ids[ids.len() - 1]))
}

/// Puts the view settings in the catalog. `/PageMode /UseOutlines` stays out when the output has
/// no bookmarks, because the viewer would only open an empty pane.
fn set_view(
    doc: &mut Document,
    view: ViewSettings,
    has_outline: bool,
) -> Result<(), PdfWriteError> {
    let catalog = doc.catalog_mut().map_err(|err| invalid(&err.to_string()))?;
    for (key, value) in view.0 {
        let opens_empty_pane = key == b"PageMode"
            && value.as_name().is_ok_and(|mode| mode == b"UseOutlines")
            && !has_outline;
        if !opens_empty_pane {
            catalog.set(key, value);
        }
    }
    Ok(())
}

/// The view settings of a document: the catalog keys it has, or `None` if it has none.
pub(super) fn view_settings(doc: &Document) -> Option<ViewSettings> {
    let catalog = doc.catalog().ok()?;
    let keys = copied_keys(doc, catalog, &super::annots::VIEW_KEYS);
    (!keys.is_empty()).then_some(ViewSettings(keys))
}

#[cfg(test)]
mod tests {
    use lopdf::Stream;

    use super::*;

    fn number(value: i64) -> Object {
        let mut dict = Dictionary::new();
        dict.set("N", Object::Integer(value));
        Object::Dictionary(dict)
    }

    #[test]
    fn prune_keeps_what_the_trailer_reaches_and_drops_the_rest() {
        let mut doc = Document::with_version("1.7");
        let in_array = doc.add_object(number(1));
        let in_stream = doc.add_object(number(2));
        let unreached = doc.add_object(number(3));
        let pointed_from_unreached = doc.add_object(number(4));
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(unreached) {
            dict.set("Next", Object::Reference(pointed_from_unreached));
        }
        let mut stream_dict = Dictionary::new();
        stream_dict.set("Ref", Object::Reference(in_stream));
        let stream = doc.add_object(Object::Stream(Stream::new(stream_dict, Vec::new())));
        let mut catalog = Dictionary::new();
        catalog.set(
            "Kids",
            Object::Array(vec![Object::Reference(in_array), Object::Reference(stream)]),
        );
        let catalog = doc.add_object(Object::Dictionary(catalog));
        // A loop back to the catalog must not keep the walk going.
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(in_array) {
            dict.set("Up", Object::Reference(catalog));
        }
        doc.trailer.set("Root", Object::Reference(catalog));

        prune_unreachable(&mut doc);

        let mut kept: Vec<ObjectId> = doc.objects.keys().copied().collect();
        kept.sort_unstable();
        let mut expected = vec![in_array, in_stream, stream, catalog];
        expected.sort_unstable();
        assert_eq!(kept, expected);
    }
}
