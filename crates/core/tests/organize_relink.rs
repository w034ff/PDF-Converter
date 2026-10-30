//! `finish_organized` attaching bookmarks, links and view settings, and `read_source_details`
//! (design organize §1, steps 7–8, and "元の PDF を `lopdf` で読む"). The input of
//! `finish_organized` is a fixture instead of pdfium's output, so no pdfium is needed: the links
//! of `organize_links.pdf` stand for the link annotations that pdfium copies into the output.

use std::path::PathBuf;

use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId};
use pdfconv_core::{
    DestinationView, LINK_RECT_TOLERANCE_PT, LinkStyle, LinkStyleSource, LinkTarget, OutlineItem,
    OutputLink, OutputPassword, Relinking, SourceLinkKey, ViewSettings, finish_organized,
    read_source_details,
};

#[path = "support/fixture_passwords.rs"]
mod fixture_passwords;
#[path = "support/organize_fixtures.rs"]
mod organize_fixtures;

const CREATOR: &str = "PDF Converter 0.1.0";
const PASSWORD: &str = "relink pass-1";
/// Pages of `organize_meta.pdf`.
const META_PAGES: u32 = 3;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).expect("reading a fixture")
}

/// Three pages, a bare catalog and no annotations.
fn meta() -> Vec<u8> {
    fixture("organize_meta.pdf")
}

/// Four pages with the links of the fixture.
fn links() -> Vec<u8> {
    fixture("organize_links.pdf")
}

fn finish(pdf: Vec<u8>, relinking: Relinking) -> Result<Vec<u8>, pdfconv_core::PdfWriteError> {
    let mut out = Vec::new();
    finish_organized(pdf, CREATOR, relinking, None, &mut out)?;
    Ok(out)
}

fn finished(pdf: Vec<u8>, relinking: Relinking) -> Document {
    Document::load_mem(&finish(pdf, relinking).expect("finishing")).expect("reading the output")
}

fn pages(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().values().copied().collect()
}

fn dict<'a>(doc: &'a Document, object: &'a Object) -> &'a Dictionary {
    doc.dereference(object)
        .and_then(|(_, object)| object.as_dict())
        .expect("a dictionary")
}

fn catalog(doc: &Document) -> &Dictionary {
    doc.catalog().expect("the catalog")
}

fn number(object: &Object) -> String {
    match object {
        Object::Null => "null".to_owned(),
        Object::Integer(value) => value.to_string(),
        Object::Real(value) => value.to_string(),
        other => format!("{other:?}"),
    }
}

fn numbers(doc: &Document, object: &Object) -> Vec<String> {
    doc.dereference(object)
        .and_then(|(_, object)| object.as_array())
        .expect("an array")
        .iter()
        .map(number)
        .collect()
}

/// A destination array as text: the page's number from 1, the type and its values.
fn describe_destination(doc: &Document, destination: &Object) -> String {
    let array = doc
        .dereference(destination)
        .and_then(|(_, object)| object.as_array())
        .expect("a destination array");
    let page = array[0].as_reference().expect("a reference to a page");
    let number_of_page = pages(doc)
        .iter()
        .position(|&candidate| candidate == page)
        .expect("a page of the output")
        + 1;
    let mut parts = vec![number_of_page.to_string()];
    parts.push(String::from_utf8_lossy(array[1].as_name().expect("a type")).into_owned());
    parts.extend(array[2..].iter().map(number));
    parts.join(" ")
}

/// Where an outline item or a link leads, as text.
fn describe_target(doc: &Document, dict: &Dictionary) -> Option<String> {
    if let Ok(destination) = dict.get(b"Dest") {
        return Some(describe_destination(doc, destination));
    }
    let action = self::dict(doc, dict.get(b"A").ok()?);
    assert_eq!(
        action
            .get(b"S")
            .and_then(Object::as_name)
            .expect("an action type"),
        b"URI"
    );
    let uri = action.get(b"URI").and_then(Object::as_str).expect("a URI");
    Some(format!("uri {}", String::from_utf8_lossy(uri)))
}

/// The page's annotations.
fn annotations(doc: &Document, page: ObjectId) -> Vec<&Dictionary> {
    let page = doc.get_dictionary(page).expect("a page");
    match page.get(b"Annots") {
        Ok(annots) => doc
            .dereference(annots)
            .and_then(|(_, annots)| annots.as_array())
            .expect("an array of annotations")
            .iter()
            .map(|entry| dict(doc, entry))
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn subtype(annotation: &Dictionary) -> &[u8] {
    annotation
        .get(b"Subtype")
        .and_then(Object::as_name)
        .expect("a subtype")
}

fn rect(doc: &Document, annotation: &Dictionary) -> Vec<String> {
    numbers(doc, annotation.get(b"Rect").expect("a rectangle"))
}

fn xyz(left: f32, top: f32, zoom: f32) -> DestinationView {
    DestinationView::Xyz {
        left: Some(left),
        top: Some(top),
        zoom: Some(zoom),
    }
}

fn to_page(page: u32, view: DestinationView) -> Option<LinkTarget> {
    Some(LinkTarget::Page { page, view })
}

fn item(title: &str, target: Option<LinkTarget>, children: Vec<OutlineItem>) -> OutlineItem {
    OutlineItem {
        title: title.to_owned(),
        target,
        children,
    }
}

/// An outline item of the output, flattened: its depth, its title as the raw bytes of the PDF
/// string and its target as text. Walking it checks the links between the items.
struct Walked {
    depth: usize,
    title: Vec<u8>,
    target: Option<String>,
    count: Option<i64>,
}

/// An item of [`Walked`] as the test compares it: depth, raw title, target and `/Count`.
type Summary<'a> = (usize, &'a [u8], Option<&'a str>, Option<i64>);

fn walk_outline(doc: &Document) -> Vec<Walked> {
    let root_id = catalog(doc)
        .get(b"Outlines")
        .and_then(Object::as_reference)
        .expect("the catalog names the outline");
    let root = doc.get_dictionary(root_id).expect("the outline root");
    assert_eq!(
        root.get(b"Type").and_then(Object::as_name).expect("a type"),
        b"Outlines"
    );
    let mut walked = Vec::new();
    let top = walk_level(doc, root, root_id, 0, &mut walked);
    assert_eq!(
        root.get(b"Count")
            .and_then(Object::as_i64)
            .expect("a count"),
        top as i64,
        "the root counts the items of the top level"
    );
    walked
}

/// Walks the children of `parent` and returns how many there are, checking `/First`, `/Last`,
/// `/Prev`, `/Next` and `/Parent` on the way.
fn walk_level(
    doc: &Document,
    parent: &Dictionary,
    parent_id: ObjectId,
    depth: usize,
    walked: &mut Vec<Walked>,
) -> usize {
    let Ok(first) = parent.get(b"First").and_then(Object::as_reference) else {
        assert!(parent.get(b"Last").is_err(), "a last child without a first");
        return 0;
    };
    let mut previous: Option<ObjectId> = None;
    let mut current = Some(first);
    let mut count = 0;
    while let Some(id) = current {
        let item = doc.get_dictionary(id).expect("an outline item");
        assert_eq!(
            item.get(b"Parent")
                .and_then(Object::as_reference)
                .expect("a parent"),
            parent_id
        );
        assert_eq!(
            item.get(b"Prev").and_then(Object::as_reference).ok(),
            previous
        );
        let next = item.get(b"Next").and_then(Object::as_reference).ok();
        walked.push(Walked {
            depth,
            title: item
                .get(b"Title")
                .and_then(Object::as_str)
                .expect("a title")
                .to_vec(),
            target: describe_target(doc, item),
            count: item.get(b"Count").and_then(Object::as_i64).ok(),
        });
        walk_level(doc, item, id, depth + 1, walked);
        count += 1;
        previous = Some(id);
        current = next;
    }
    assert_eq!(
        parent
            .get(b"Last")
            .and_then(Object::as_reference)
            .expect("a last child"),
        previous.expect("a child"),
        "the last child"
    );
    count
}

fn utf16(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFE, 0xFF];
    bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
    bytes
}

fn relinking_with_outline(outline: Vec<OutlineItem>) -> Relinking {
    Relinking {
        outline,
        ..Relinking::default()
    }
}

#[test]
fn bookmarks_are_written_in_three_levels_and_closed() {
    use organize_fixtures::LINKS_NON_ASCII_TITLE;
    let outline = vec![
        item(
            "Part",
            to_page(0, DestinationView::Fit),
            vec![
                item(
                    "Chapter",
                    to_page(1, xyz(20.0, 300.0, 1.5)),
                    vec![item(
                        "Site",
                        Some(LinkTarget::Uri("https://example.com/x".into())),
                        vec![],
                    )],
                ),
                item(
                    "Heading",
                    None,
                    vec![item("Deep", to_page(2, DestinationView::FitB), vec![])],
                ),
            ],
        ),
        item(
            LINKS_NON_ASCII_TITLE,
            to_page(2, DestinationView::FitB),
            vec![],
        ),
        item(
            "Last",
            None,
            vec![item("Child", to_page(0, DestinationView::Fit), vec![])],
        ),
    ];
    let doc = finished(meta(), relinking_with_outline(outline));
    let walked = walk_outline(&doc);

    let summary: Vec<Summary<'_>> = walked
        .iter()
        .map(|w| (w.depth, w.title.as_slice(), w.target.as_deref(), w.count))
        .collect();
    assert_eq!(
        summary,
        [
            (0, &b"Part"[..], Some("1 Fit"), Some(-2)),
            (1, &b"Chapter"[..], Some("2 XYZ 20 300 1.5"), Some(-1)),
            (2, &b"Site"[..], Some("uri https://example.com/x"), None),
            (1, &b"Heading"[..], None, Some(-1)),
            (2, &b"Deep"[..], Some("3 FitB"), None),
            (0, &utf16(LINKS_NON_ASCII_TITLE)[..], Some("3 FitB"), None),
            (0, &b"Last"[..], None, Some(-1)),
            (1, &b"Child"[..], Some("1 Fit"), None),
        ]
    );
}

#[test]
fn every_kind_of_destination_is_written_and_a_missing_value_is_null() {
    let views = [
        (xyz(1.0, 2.0, 3.0), "XYZ 1 2 3"),
        (
            DestinationView::Xyz {
                left: None,
                top: None,
                zoom: None,
            },
            "XYZ null null null",
        ),
        (
            DestinationView::Xyz {
                left: Some(5.0),
                top: None,
                zoom: Some(0.5),
            },
            "XYZ 5 null 0.5",
        ),
        (DestinationView::Fit, "Fit"),
        (DestinationView::FitH { top: Some(7.5) }, "FitH 7.5"),
        (DestinationView::FitH { top: None }, "FitH null"),
        (DestinationView::FitV { left: Some(8.0) }, "FitV 8"),
        (DestinationView::FitV { left: None }, "FitV null"),
        (
            DestinationView::FitR {
                left: 1.0,
                bottom: 2.0,
                right: 3.0,
                top: 4.0,
            },
            "FitR 1 2 3 4",
        ),
        (DestinationView::FitB, "FitB"),
        (DestinationView::FitBH { top: Some(9.0) }, "FitBH 9"),
        (DestinationView::FitBH { top: None }, "FitBH null"),
        (DestinationView::FitBV { left: Some(10.0) }, "FitBV 10"),
        (DestinationView::FitBV { left: None }, "FitBV null"),
    ];
    let outline = views
        .iter()
        .map(|(view, _)| item("v", to_page(1, *view), vec![]))
        .collect();
    let doc = finished(meta(), relinking_with_outline(outline));
    let written: Vec<String> = walk_outline(&doc)
        .into_iter()
        .map(|walked| walked.target.expect("a target"))
        .collect();
    let expected: Vec<String> = views.iter().map(|(_, text)| format!("2 {text}")).collect();
    assert_eq!(written, expected);
}

#[test]
fn no_bookmarks_means_no_outline() {
    let doc = finished(meta(), Relinking::default());
    assert!(catalog(&doc).get(b"Outlines").is_err());
}

#[test]
fn a_page_the_output_lacks_is_a_write_error() {
    // The meta fixture has three pages.
    let outline = vec![item("Gone", to_page(3, DestinationView::Fit), vec![])];
    assert!(finish(meta(), relinking_with_outline(outline)).is_err());

    let link = OutputLink {
        page: 0,
        rect: [0.0, 0.0, 10.0, 10.0],
        target: LinkTarget::Page {
            page: 3,
            view: DestinationView::Fit,
        },
        style: LinkStyleSource::Source(None),
    };
    let relinking = Relinking {
        links: vec![link.clone()],
        ..Relinking::default()
    };
    assert!(finish(meta(), relinking).is_err());

    let relinking = Relinking {
        links: vec![OutputLink {
            page: 3,
            target: LinkTarget::Uri("https://example.com/".into()),
            ..link
        }],
        ..Relinking::default()
    };
    assert!(finish(meta(), relinking).is_err());
}

fn link(page: u32, rect: [f32; 4], target: &str, style: LinkStyleSource) -> OutputLink {
    OutputLink {
        page,
        rect,
        target: LinkTarget::Uri(target.to_owned()),
        style,
    }
}

/// The frame keys of page 2's link in `organize_links.pdf`, read from the file.
fn source_style() -> LinkStyle {
    let details = read_source_details(
        &fixture_path("organize_links.pdf"),
        None,
        organize_fixtures::LINKS_PAGES,
        false,
        &[SourceLinkKey {
            page: 1,
            nth: 0,
            rect: [20.0, 100.0, 120.0, 130.0],
        }],
    )
    .expect("reading organize_links.pdf");
    details.styles[0].clone().expect("the frame of the link")
}

fn border(doc: &Document, annotation: &Dictionary) -> Vec<String> {
    numbers(doc, annotation.get(b"Border").expect("a /Border"))
}

fn keys(annotation: &Dictionary) -> Vec<String> {
    annotation
        .iter()
        .map(|(key, _)| String::from_utf8_lossy(key).into_owned())
        .collect()
}

#[test]
fn copied_links_are_replaced_and_a_frame_is_taken_only_from_the_link_that_matches() {
    use organize_fixtures::LINKS_PAGE_URL;
    // The four links of page 1 in the fixture, in order:
    //   0 [20 270 120 300] /C [1 0 0] /Border [0 0 2] /H /O
    //   1 [20 230 120 260] /Border [0 0 1] /BS << .. >>
    //   2 [20 190 120 220] /C [0 0 1]
    //   3 [20 150 120 180] /Border [0 0 1]  (JavaScript)
    let new = "https://example.com/new";
    let copied = |nth, rect| link(0, rect, new, LinkStyleSource::Copied { nth });
    let relinking = Relinking {
        links: vec![
            copied(0, [20.0, 270.0, 120.0, 300.0]),
            // Within the tolerance on every side.
            copied(
                1,
                [
                    20.0 + LINK_RECT_TOLERANCE_PT,
                    230.0 - LINK_RECT_TOLERANCE_PT,
                    120.0 + LINK_RECT_TOLERANCE_PT,
                    260.0 - LINK_RECT_TOLERANCE_PT,
                ],
            ),
            copied(2, [20.0, 190.0, 120.0, 220.0 + LINK_RECT_TOLERANCE_PT]),
            // A little past the tolerance on one side.
            copied(
                2,
                [20.0, 190.0, 120.0, 220.0 + LINK_RECT_TOLERANCE_PT + 0.1],
            ),
            // The rectangle of the third link under the number of the first.
            copied(0, [20.0, 190.0, 120.0, 220.0]),
            // A number the page does not have.
            copied(9, [20.0, 150.0, 120.0, 180.0]),
            link(
                1,
                [20.0, 100.0, 120.0, 130.0],
                new,
                LinkStyleSource::Source(Some(source_style())),
            ),
            link(
                1,
                [20.0, 10.0, 60.0, 30.0],
                new,
                LinkStyleSource::Source(None),
            ),
        ],
        ..Relinking::default()
    };
    let out = finish(links(), relinking).expect("finishing");
    let doc = Document::load_mem(&out).expect("reading the output");
    let ids = pages(&doc);

    // Every link that pdfium would have copied is gone and the new ones are in their place.
    assert!(
        !out.windows(LINKS_PAGE_URL.len())
            .any(|window| window == LINKS_PAGE_URL.as_bytes())
    );
    let page_one = annotations(&doc, ids[0]);
    assert_eq!(page_one.len(), 6);
    for annotation in &page_one {
        assert_eq!(subtype(annotation), b"Link");
        assert_eq!(
            annotation
                .get(b"Type")
                .and_then(Object::as_name)
                .expect("present"),
            b"Annot"
        );
        assert_eq!(
            annotation
                .get(b"F")
                .and_then(Object::as_i64)
                .expect("present"),
            4
        );
        assert_eq!(
            annotation
                .get(b"P")
                .and_then(Object::as_reference)
                .expect("present"),
            ids[0]
        );
        assert_eq!(
            describe_target(&doc, annotation).as_deref(),
            Some("uri https://example.com/new")
        );
    }

    assert_eq!(rect(&doc, page_one[0]), ["20", "270", "120", "300"]);
    assert_eq!(border(&doc, page_one[0]), ["0", "0", "2"]);
    assert_eq!(
        numbers(&doc, page_one[0].get(b"C").expect("present")),
        ["1", "0", "0"]
    );
    assert_eq!(
        page_one[0]
            .get(b"H")
            .and_then(Object::as_name)
            .expect("present"),
        b"O"
    );

    assert_eq!(border(&doc, page_one[1]), ["0", "0", "1"]);
    let styled = dict(&doc, page_one[1].get(b"BS").expect("/BS"));
    assert_eq!(
        styled.get(b"S").and_then(Object::as_name).expect("present"),
        b"D"
    );
    assert_eq!(numbers(&doc, styled.get(b"D").expect("present")), ["3"]);

    // The third link has /C and no /Border; a link that has none of the keys gets none either.
    assert_eq!(
        numbers(&doc, page_one[2].get(b"C").expect("present")),
        ["0", "0", "1"]
    );
    assert!(page_one[2].get(b"Border").is_err());

    // What does not match has no frame, and takes nothing from another link.
    for annotation in &page_one[3..] {
        assert_eq!(border(&doc, annotation), ["0", "0", "0"]);
        for key in ["C", "H", "BS"] {
            assert!(annotation.get(key.as_bytes()).is_err(), "{key}");
        }
    }
}

#[test]
fn a_link_that_does_not_match_is_written_without_a_frame() {
    let relinking = Relinking {
        links: vec![
            // The rectangle of the second link under the number of the first.
            link(
                0,
                [20.0, 230.0, 120.0, 260.0],
                "https://example.com/a",
                LinkStyleSource::Copied { nth: 0 },
            ),
            link(
                0,
                [20.0, 270.0, 120.0, 300.0],
                "https://example.com/b",
                LinkStyleSource::Copied { nth: 20 },
            ),
        ],
        ..Relinking::default()
    };
    let doc = finished(links(), relinking);
    let page_one = annotations(&doc, pages(&doc)[0]);
    assert_eq!(page_one.len(), 2);
    for annotation in page_one {
        assert_eq!(border(&doc, annotation), ["0", "0", "0"]);
        for key in ["C", "H", "BS"] {
            assert!(annotation.get(key.as_bytes()).is_err(), "{key}");
        }
    }
}

#[test]
fn a_frame_read_from_the_source_is_written_and_none_means_no_frame() {
    let relinking = Relinking {
        links: vec![
            link(
                1,
                [20.0, 100.0, 120.0, 130.0],
                "https://example.com/a",
                LinkStyleSource::Source(Some(source_style())),
            ),
            link(
                1,
                [20.0, 10.0, 60.0, 30.0],
                "https://example.com/b",
                LinkStyleSource::Source(None),
            ),
        ],
        ..Relinking::default()
    };
    let doc = finished(links(), relinking);
    let page_two = annotations(&doc, pages(&doc)[1]);

    // The FreeText stays, ahead of the new links.
    assert_eq!(page_two.len(), 3);
    assert_eq!(subtype(page_two[0]), b"FreeText");
    assert_eq!(subtype(page_two[1]), b"Link");
    assert_eq!(border(&doc, page_two[1]), ["0", "0", "3"]);
    assert_eq!(
        numbers(&doc, page_two[1].get(b"C").expect("present")),
        ["0", "1", "0"]
    );
    assert_eq!(border(&doc, page_two[2]), ["0", "0", "0"]);
    assert!(page_two[2].get(b"C").is_err());
    // Only the frame keys come from the source.
    assert_eq!(
        keys(page_two[1]),
        ["Type", "Subtype", "Rect", "Border", "C", "F", "P", "A"]
    );
}

#[test]
fn a_page_without_annotations_has_no_annots_key() {
    let doc = finished(links(), Relinking::default());
    let ids = pages(&doc);
    // Page 3 had an empty array and page 4 had only a link; page 2 keeps its FreeText.
    for id in [ids[0], ids[2], ids[3]] {
        assert!(
            doc.get_dictionary(id)
                .expect("present")
                .get(b"Annots")
                .is_err()
        );
    }
    let page_two = annotations(&doc, ids[1]);
    assert_eq!(page_two.len(), 1);
    assert_eq!(subtype(page_two[0]), b"FreeText");
}

#[test]
fn links_go_on_the_page_they_name() {
    let relinking = Relinking {
        links: vec![
            link(
                2,
                [1.0, 2.0, 3.0, 4.0],
                "https://example.com/c",
                LinkStyleSource::Source(None),
            ),
            link(
                0,
                [5.0, 6.0, 7.0, 8.0],
                "https://example.com/a",
                LinkStyleSource::Source(None),
            ),
            link(
                2,
                [9.0, 10.0, 11.0, 12.0],
                "https://example.com/d",
                LinkStyleSource::Source(None),
            ),
            OutputLink {
                page: 1,
                rect: [13.0, 14.0, 15.0, 16.0],
                target: LinkTarget::Page {
                    page: 2,
                    view: DestinationView::FitH { top: Some(40.0) },
                },
                style: LinkStyleSource::Source(None),
            },
        ],
        ..Relinking::default()
    };
    let doc = finished(meta(), relinking);
    let ids = pages(&doc);
    let on = |index: usize| -> Vec<(Vec<String>, Option<String>)> {
        annotations(&doc, ids[index])
            .into_iter()
            .map(|annotation| (rect(&doc, annotation), describe_target(&doc, annotation)))
            .collect()
    };
    let text = |values: &[&str]| {
        values
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        on(0),
        [(
            text(&["5", "6", "7", "8"]),
            Some("uri https://example.com/a".into())
        )]
    );
    assert_eq!(
        on(1),
        [(text(&["13", "14", "15", "16"]), Some("3 FitH 40".into()))]
    );
    assert_eq!(
        on(2),
        [
            (
                text(&["1", "2", "3", "4"]),
                Some("uri https://example.com/c".into())
            ),
            (
                text(&["9", "10", "11", "12"]),
                Some("uri https://example.com/d".into())
            ),
        ]
    );
}

fn view_of_links_pdf() -> ViewSettings {
    read_source_details(
        &fixture_path("organize_links.pdf"),
        None,
        organize_fixtures::LINKS_PAGES,
        true,
        &[],
    )
    .expect("reading organize_links.pdf")
    .view
    .expect("the view settings")
}

fn name_of<'a>(catalog: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    catalog.get(key).and_then(Object::as_name).ok()
}

#[test]
fn the_view_settings_go_into_the_catalog() {
    let relinking = Relinking {
        outline: vec![item("Only", to_page(0, DestinationView::Fit), vec![])],
        view: Some(view_of_links_pdf()),
        ..Relinking::default()
    };
    let doc = finished(meta(), relinking);
    let catalog = catalog(&doc);
    assert_eq!(name_of(catalog, b"PageLayout"), Some(&b"TwoPageRight"[..]));
    assert_eq!(name_of(catalog, b"PageMode"), Some(&b"UseOutlines"[..]));
    let preferences = dict(
        &doc,
        catalog.get(b"ViewerPreferences").expect("preferences"),
    );
    assert!(
        preferences
            .get(b"DisplayDocTitle")
            .and_then(Object::as_bool)
            .expect("present")
    );
    assert!(
        preferences
            .get(b"FitWindow")
            .and_then(Object::as_bool)
            .expect("present")
    );
}

#[test]
fn use_outlines_stays_out_when_there_are_no_bookmarks_and_the_other_keys_go_in() {
    let relinking = Relinking {
        view: Some(view_of_links_pdf()),
        ..Relinking::default()
    };
    let doc = finished(meta(), relinking);
    let catalog = catalog(&doc);
    assert!(catalog.get(b"PageMode").is_err());
    assert_eq!(name_of(catalog, b"PageLayout"), Some(&b"TwoPageRight"[..]));
    assert!(catalog.get(b"ViewerPreferences").is_ok());
}

#[test]
fn no_view_settings_leave_the_catalog_alone() {
    let doc = finished(meta(), Relinking::default());
    let catalog = catalog(&doc);
    for key in ["PageLayout", "PageMode", "ViewerPreferences"] {
        assert!(catalog.get(key.as_bytes()).is_err(), "{key}");
    }
}

#[test]
fn what_is_attached_is_encrypted_with_the_rest() {
    let relinking = Relinking {
        outline: vec![item(
            "Encrypted title",
            to_page(1, DestinationView::Fit),
            vec![],
        )],
        links: vec![link(
            0,
            [1.0, 2.0, 3.0, 4.0],
            "https://example.com/secret-url",
            LinkStyleSource::Source(None),
        )],
        view: Some(view_of_links_pdf()),
    };
    let password = OutputPassword::new(PASSWORD.to_owned()).expect("a password");
    let mut out = Vec::new();
    finish_organized(meta(), CREATOR, relinking, Some(&password), &mut out).expect("finishing");
    for secret in ["Encrypted title", "secret-url"] {
        assert!(
            !out.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "{secret}"
        );
    }

    let doc = Document::load_mem_with_options(&out, LoadOptions::with_password(PASSWORD))
        .expect("reading the output with the password");
    let walked = walk_outline(&doc);
    assert_eq!(walked.len(), 1);
    assert_eq!(walked[0].title, b"Encrypted title");
    assert_eq!(walked[0].target.as_deref(), Some("2 Fit"));
    let annotation = annotations(&doc, pages(&doc)[0])[0];
    assert_eq!(
        describe_target(&doc, annotation).as_deref(),
        Some("uri https://example.com/secret-url")
    );
    assert_eq!(
        name_of(catalog(&doc), b"PageLayout"),
        Some(&b"TwoPageRight"[..])
    );
}

// ---------------------------------------------------------------------------
// read_source_details

fn read(
    name: &str,
    password: Option<&str>,
    page_count: u32,
    want_view: bool,
    links: &[SourceLinkKey],
) -> Option<pdfconv_core::SourceDetails> {
    read_source_details(&fixture_path(name), password, page_count, want_view, links)
}

fn key(page: u32, nth: u32, rect: [f32; 4]) -> SourceLinkKey {
    SourceLinkKey { page, nth, rect }
}

#[test]
fn the_view_settings_and_the_frame_of_a_link_are_read_from_a_source() {
    let page_count = organize_fixtures::LINKS_PAGES;
    let details = read(
        "organize_links.pdf",
        None,
        page_count,
        true,
        &[
            key(1, 0, [20.0, 100.0, 120.0, 130.0]),
            key(0, 0, [20.0, 270.0, 120.0, 300.0]),
            key(3, 0, [20.0, 100.0, 120.0, 130.0]),
        ],
    )
    .expect("reading the fixture");

    // Written into an output, the view settings are the fixture's.
    let relinking = Relinking {
        outline: vec![item("o", to_page(0, DestinationView::Fit), vec![])],
        view: details.view,
        ..Relinking::default()
    };
    let doc = finished(meta(), relinking);
    assert_eq!(
        name_of(catalog(&doc), b"PageLayout"),
        Some(&b"TwoPageRight"[..])
    );

    // And the frames are the ones of the links they were asked for.
    let styles: Vec<LinkStyle> = details
        .styles
        .into_iter()
        .map(|s| s.expect("a frame"))
        .collect();
    let links = styles
        .into_iter()
        .zip([
            [20.0, 100.0, 120.0, 130.0],
            [20.0, 270.0, 120.0, 300.0],
            [20.0, 100.0, 120.0, 130.0],
        ])
        .map(|(style, rect)| {
            link(
                0,
                rect,
                "https://example.com/",
                LinkStyleSource::Source(Some(style)),
            )
        })
        .collect();
    let doc = finished(
        meta(),
        Relinking {
            links,
            ..Relinking::default()
        },
    );
    let written = annotations(&doc, pages(&doc)[0]);
    assert_eq!(border(&doc, written[0]), ["0", "0", "3"]);
    assert_eq!(border(&doc, written[1]), ["0", "0", "2"]);
    assert_eq!(border(&doc, written[2]), ["0", "0", "1"]);
    assert_eq!(
        numbers(&doc, written[2].get(b"C").expect("present")),
        ["0.5", "0", "0.5"]
    );
}

#[test]
fn the_view_is_read_only_when_asked_for() {
    let details = read(
        "organize_links.pdf",
        None,
        organize_fixtures::LINKS_PAGES,
        false,
        &[],
    )
    .expect("reading the fixture");
    assert!(details.view.is_none());
    assert!(details.styles.is_empty());
}

#[test]
fn a_source_without_view_settings_has_none() {
    let details =
        read("organize_meta.pdf", None, META_PAGES, true, &[]).expect("reading the fixture");
    assert!(details.view.is_none());
}

#[test]
fn a_frame_is_not_given_when_the_number_or_the_rectangle_is_wrong() {
    let page_count = organize_fixtures::LINKS_PAGES;
    let details = read(
        "organize_links.pdf",
        None,
        page_count,
        false,
        &[
            // The right link, the right rectangle.
            key(0, 1, [20.0, 230.0, 120.0, 260.0]),
            // The rectangle of the link before it.
            key(0, 1, [20.0, 270.0, 120.0, 300.0]),
            // Just past the tolerance on one side.
            key(
                0,
                1,
                [20.0, 230.0, 120.0, 260.0 + LINK_RECT_TOLERANCE_PT + 0.1],
            ),
            // Within the tolerance.
            key(0, 1, [20.0 - LINK_RECT_TOLERANCE_PT, 230.0, 120.0, 260.0]),
            // A link the page lacks, a page with no links, a page the file lacks.
            key(0, 4, [20.0, 150.0, 120.0, 180.0]),
            key(2, 0, [20.0, 100.0, 120.0, 130.0]),
            key(9, 0, [20.0, 100.0, 120.0, 130.0]),
        ],
    )
    .expect("reading the fixture");
    let found: Vec<bool> = details.styles.iter().map(Option::is_some).collect();
    assert_eq!(found, [true, false, false, true, false, false, false]);
}

#[test]
fn a_source_with_a_password_needs_it_and_a_wrong_page_count_gives_nothing() {
    use fixture_passwords::ENCRYPTED_USER_PASSWORD;
    // encrypted.pdf has one page.
    assert!(
        read(
            "encrypted.pdf",
            Some(ENCRYPTED_USER_PASSWORD),
            1,
            false,
            &[]
        )
        .is_some()
    );
    // Without the password lopdf reads a document without pages instead of failing.
    assert!(read("encrypted.pdf", None, 1, false, &[]).is_none());
    assert!(read("encrypted.pdf", Some("not-the-password"), 1, false, &[]).is_none());
    // The count that pdfium found has to be the count that lopdf finds.
    assert!(
        read(
            "organize_links.pdf",
            None,
            organize_fixtures::LINKS_PAGES + 1,
            false,
            &[]
        )
        .is_none()
    );
    assert!(read("organize_links.pdf", None, 0, false, &[]).is_none());
}

#[test]
fn a_file_that_cannot_be_read_gives_nothing() {
    assert!(read("corrupt.pdf", None, 1, true, &[]).is_none());
    assert!(read("does_not_exist.pdf", None, 1, true, &[]).is_none());
}
