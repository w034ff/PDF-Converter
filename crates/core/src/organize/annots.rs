//! Reading the annotations of a page and copying values out of a document, shared by the
//! rewrite of the output ([`super::relink`]) and the read of a source PDF ([`super::source`]).

use lopdf::{Dictionary, Document, Object, ObjectId};

/// The keys of a link annotation that decide how a viewer draws its frame.
pub(super) const LINK_STYLE_KEYS: [&[u8]; 4] = [b"Border", b"C", b"H", b"BS"];

/// The keys of the catalog that decide how a viewer first shows the document.
pub(super) const VIEW_KEYS: [&[u8]; 3] = [b"PageLayout", b"PageMode", b"ViewerPreferences"];

/// How many references a copied value is followed through. The values copied are a few numbers,
/// names and small dictionaries, so only a malformed or hostile PDF comes near this.
const RESOLVE_MAX_DEPTH: usize = 8;

/// How many objects one copied value may hold. References to a shared array can multiply at
/// every level, which the depth limit alone does not bound.
const RESOLVE_MAX_OBJECTS: usize = 4096;

/// A link annotation of a page: its rectangle (`None` if it has none that can be read) and the
/// frame keys it carries.
pub(super) struct LinkInfo {
    pub rect: Option<[f32; 4]>,
    pub style: Vec<(&'static [u8], Object)>,
}

/// The annotations of one page: the entries of `/Annots` that are not links, as they stand, and
/// the links in the order of `/Annots`.
pub(super) struct PageAnnotations {
    pub others: Vec<Object>,
    pub links: Vec<LinkInfo>,
}

pub(super) fn page_annotations(doc: &Document, page: ObjectId) -> PageAnnotations {
    let mut found = PageAnnotations {
        others: Vec::new(),
        links: Vec::new(),
    };
    let entries = doc
        .get_dictionary(page)
        .and_then(|dict| dict.get(b"Annots"))
        .and_then(|annots| doc.dereference(annots))
        .and_then(|(_, annots)| annots.as_array());
    let Ok(entries) = entries else {
        return found;
    };
    for entry in entries {
        match doc
            .dereference(entry)
            .and_then(|(_, object)| object.as_dict())
        {
            Ok(dict) if is_link(dict) => found.links.push(LinkInfo {
                rect: rect_of(doc, dict),
                style: link_style(doc, dict),
            }),
            _ => found.others.push(entry.clone()),
        }
    }
    found
}

fn is_link(dict: &Dictionary) -> bool {
    dict.get(b"Subtype")
        .and_then(Object::as_name)
        .is_ok_and(|name| name == b"Link")
}

/// The `/Rect` of `dict` as left, bottom, right and top, whatever order its corners are in.
fn rect_of(doc: &Document, dict: &Dictionary) -> Option<[f32; 4]> {
    let (_, rect) = doc.dereference(dict.get(b"Rect").ok()?).ok()?;
    let numbers: Vec<f32> = rect
        .as_array()
        .ok()?
        .iter()
        .map(|number| doc.dereference(number).ok()?.1.as_float().ok())
        .collect::<Option<_>>()?;
    let [x0, y0, x1, y1] = numbers[..] else {
        return None;
    };
    Some([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)])
}

/// The values of the frame keys that `dict` has.
fn link_style(doc: &Document, dict: &Dictionary) -> Vec<(&'static [u8], Object)> {
    copied_keys(doc, dict, &LINK_STYLE_KEYS)
}

/// The values of those of `keys` that `dict` has, with references followed.
pub(super) fn copied_keys(
    doc: &Document,
    dict: &Dictionary,
    keys: &[&'static [u8]],
) -> Vec<(&'static [u8], Object)> {
    let mut budget = RESOLVE_MAX_OBJECTS;
    keys.iter()
        .filter_map(|&key| {
            let value = dict.get(key).ok()?;
            Some((key, resolved(doc, value, 0, &mut budget)))
        })
        .collect()
}

/// A copy of `object` in which every reference is replaced by what it points to. A stream, a
/// value that is too deep and whatever comes after the budget runs out become null.
fn resolved(doc: &Document, object: &Object, depth: usize, budget: &mut usize) -> Object {
    if depth > RESOLVE_MAX_DEPTH || *budget == 0 {
        return Object::Null;
    }
    *budget -= 1;
    match object {
        Object::Reference(id) => doc.get_object(*id).map_or(Object::Null, |target| {
            resolved(doc, target, depth + 1, budget)
        }),
        Object::Array(items) => Object::Array(
            items
                .iter()
                .map(|item| resolved(doc, item, depth + 1, budget))
                .collect(),
        ),
        Object::Dictionary(dict) => {
            Object::Dictionary(resolved_dictionary(doc, dict, depth, budget))
        }
        Object::Stream(_) => Object::Null,
        other => other.clone(),
    }
}

fn resolved_dictionary(
    doc: &Document,
    dict: &Dictionary,
    depth: usize,
    budget: &mut usize,
) -> Dictionary {
    let mut copy = Dictionary::new();
    for (key, value) in dict.iter() {
        copy.set(key.clone(), resolved(doc, value, depth + 1, budget));
    }
    copy
}

/// Whether two rectangles agree on every side to within [`LINK_RECT_TOLERANCE_PT`](super::LINK_RECT_TOLERANCE_PT).
pub(super) fn rects_match(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter()
        .zip(b)
        .all(|(x, y)| (x - y).abs() <= super::LINK_RECT_TOLERANCE_PT)
}
