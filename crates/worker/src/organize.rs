//! The worker's side of organizing (design organize §1, §3): copies pages of several PDFs into
//! a new document with pdfium, noting the bookmarks, links and view settings of the sources on
//! the way, then hands the bytes and the notes to `pdfconv-core` to attach again, encrypt and
//! send.

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

use pdfconv_core::{
    DestinationView, LinkStyleSource, LinkTarget, OutlineItem, OutputLink, OutputPassword,
    Relinking, SourceDetails, SourceLinkKey, finish_organized, read_source_details,
};
use pdfium_render::prelude::{
    PdfAction, PdfBookmark, PdfDestination, PdfDestinationViewSettings, PdfDocument, PdfPage,
    PdfPageAnnotationCommon, PdfPageAnnotationType, PdfPageRenderRotation, PdfRenderConfig, Pdfium,
};

use crate::protocol::{
    OUTPUT_CHUNK_BYTES, OrganizePage, OrganizeSource, Password, Request, Response, write_message,
};
use crate::server::{
    CODE_INVALID_PARAMS, CODE_PDF_OPEN_FAILED, CODE_PDFIUM_UNAVAILABLE, WORKER_MEMORY_LIMIT,
    check_page_count, map_pdfium_open_error,
};
use crate::system_memory::current_usage;

/// Width in pixels of the render that makes pdfium build the appearance of annotations that have
/// none, before the page is flattened (design organize §1, step 4). The picture is thrown away.
pub const FLATTEN_PRERENDER_WIDTH: i32 = 50;

/// How much larger than the estimate the buffer for the saved document is made, so that saving
/// does not grow it by doubling (design organize §1, step 6).
const SAVE_BUFFER_FACTOR_PERCENT: u64 = 110;

/// How many levels of bookmarks are followed. A deeper one is left out, because a malformed or
/// hostile PDF can nest bookmarks as deep as it likes and the walk is recursive.
const MAX_OUTLINE_DEPTH: usize = 64;

/// How many bookmarks are read from one source. It also ends a chain of siblings that loops back
/// on itself, which pdfium does not notice.
const MAX_OUTLINE_ITEMS: usize = 100_000;

/// The largest memory lopdf needs to read a file, as a multiple of the file's size. lopdf reads
/// the whole file whatever pages are used, and does not select objects in an encrypted PDF. The
/// most measured was 3.7 times for an encrypted PDF on Windows and 3.6 times on Linux (2.6 times
/// without encryption); this is the round number above that.
const RAW_MEMORY_FACTOR: u64 = 4;

/// What is kept free when deciding whether lopdf may read a file: the allocations that the
/// worker's usage does not show (pdfium's and the allocator's).
const RAW_MARGIN: u64 = 256 * 1024 * 1024;

/// Error code when the rewritten document cannot be written.
const CODE_WRITE_FAILED: &str = "WriteFailed";

/// The arguments of `Request::Organize`.
pub(crate) struct OrganizeArgs {
    pub sources: Vec<OrganizeSource>,
    pub pages: Vec<OrganizePage>,
    pub creator: String,
    pub output_password: Option<Password>,
}

impl OrganizeArgs {
    /// Takes the arguments out of `request`, or gives it back if it is another request.
    pub(crate) fn from_request(request: Request) -> Result<Self, Request> {
        match request {
            Request::Organize {
                sources,
                pages,
                creator,
                output_password,
            } => Ok(Self {
                sources,
                pages,
                creator,
                output_password,
            }),
            other => Err(other),
        }
    }
}

/// Why an organize request ended without an output.
enum Failure {
    /// A source PDF could not be used.
    Source {
        code: String,
        detail: Option<String>,
        source: u32,
    },
    /// Anything else.
    General {
        code: String,
        detail: Option<String>,
    },
    /// The pipe to the main process failed; nobody is left to tell.
    Io(io::Error),
}

impl Failure {
    fn general(code: &str) -> Self {
        Self::General {
            code: code.into(),
            detail: None,
        }
    }

    /// The failure of source `source`, with the code and detail of the error response `response`.
    fn from_response(response: Response, source: u32) -> Self {
        match response {
            Response::Error { code, detail } => Self::Source {
                code,
                detail,
                source,
            },
            _ => Self::Source {
                code: CODE_PDF_OPEN_FAILED.into(),
                detail: None,
                source,
            },
        }
    }

    fn source(code: &str, source: u32) -> Self {
        Self::Source {
            code: code.into(),
            detail: None,
            source,
        }
    }
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Builds the output of `args` and writes the answers to `out`: progress, chunks, then
/// `Organized`, or one failure.
///
/// # Errors
///
/// Returns the I/O error of `out`. A failure of the request itself is answered, not returned.
pub(crate) fn run<W: Write>(
    pdfium: &'static Pdfium,
    args: OrganizeArgs,
    out: &mut W,
) -> io::Result<()> {
    match organize(pdfium, args, out) {
        Ok(()) => Ok(()),
        Err(Failure::Io(error)) => Err(error),
        Err(Failure::Source {
            code,
            detail,
            source,
        }) => write_message(
            out,
            &Response::OrganizeFailed {
                code,
                detail,
                source,
            },
            &[],
        ),
        Err(Failure::General { code, detail }) => {
            write_message(out, &Response::Error { code, detail }, &[])
        }
    }
}

fn organize<W: Write>(
    pdfium: &'static Pdfium,
    args: OrganizeArgs,
    out: &mut W,
) -> Result<(), Failure> {
    let password = validate(&args)?;
    let OrganizeArgs {
        sources,
        pages,
        creator,
        ..
    } = args;

    let mut output = pdfium.create_new_pdf().map_err(|error| Failure::General {
        code: CODE_PDFIUM_UNAVAILABLE.into(),
        detail: Some(format!("{error:?}")),
    })?;
    // Output positions already filled, in order. A page of a source goes in front of the later
    // sources' pages that come after it in the output, so its index is the number of filled
    // positions before it.
    let mut filled: Vec<u32> = Vec::new();
    let mut done: u32 = 0;
    let mut estimate: u64 = 0;

    // The pages of each source with their output positions, in output order.
    let mut by_source: Vec<Vec<(u32, &OrganizePage)>> = vec![Vec::new(); sources.len()];
    for (position, page) in (0u32..).zip(&pages) {
        by_source[page.source as usize].push((position, page));
    }
    // `validate` has checked that there is a first page.
    let first_source = pages[0].source as usize;

    let mut relinking = Relinking::default();

    for (index, source) in sources.iter().enumerate() {
        let source_index = index as u32;
        let used = &by_source[index];

        let doc = pdfium
            .load_pdf_from_file(&source.path, source.password.as_deref())
            .map_err(|error| Failure::from_response(map_pdfium_open_error(&error), source_index))?;
        let page_count = doc.pages().len() as u32;
        check_page_count(page_count)
            .map_err(|response| Failure::from_response(response, source_index))?;
        if used.iter().any(|(_, page)| page.page >= page_count) {
            return Err(Failure::source(CODE_INVALID_PARAMS, source_index));
        }

        // Where each used page of this source goes in the output. A page that is not in it is
        // gone, and so is whatever leads to it.
        let kept: HashMap<u32, u32> = used
            .iter()
            .map(|(position, page)| (page.page, *position))
            .collect();
        // The bookmarks are noted before any page is touched (design organize §1, step 2).
        relinking.outline.extend(outline_of(&doc, &kept));

        let mut flattened_links = FlattenedLinks::default();
        for (position, page) in used {
            prepare_page(
                &doc,
                page.page,
                *position,
                &kept,
                &mut relinking.links,
                &mut flattened_links,
            )
            .map_err(|code| Failure::source(code, source_index))?;
            done += 1;
            write_message(out, &Response::OrganizeProgress { done }, &[])?;
        }

        for run in runs(used) {
            let start = run[0].0;
            let destination = filled.partition_point(|&position| position < start);
            let list = run
                .iter()
                .map(|(_, page)| (page.page + 1).to_string())
                .collect::<Vec<_>>()
                .join(",");
            output
                .pages_mut()
                .copy_pages_from_document(&doc, &list, destination as i32)
                .map_err(|_| Failure::source(CODE_PDF_OPEN_FAILED, source_index))?;
            for (offset, (_, page)) in run.iter().enumerate() {
                turn(&output, destination + offset, page.rotation)
                    .map_err(|code| Failure::source(code, source_index))?;
            }
            filled.splice(
                destination..destination,
                run.iter().map(|(position, _)| *position),
            );
        }

        if page_count > 0 {
            let file_len = std::fs::metadata(&source.path).map_or(0, |meta| meta.len());
            estimate += file_len * used.len() as u64 / u64::from(page_count);
        }
        // The next source is opened only after this one is closed.
        drop(doc);

        // What pdfium does not show is read from the file, after pdfium is done with it. The
        // view settings are those of the PDF that the first page of the output comes from.
        let want_view = index == first_source;
        if want_view || !flattened_links.keys.is_empty() {
            let details = read_raw(source, page_count, want_view, &flattened_links.keys);
            if let Some(details) = details {
                if want_view {
                    relinking.view = details.view;
                }
                for (slot, style) in flattened_links.slots.iter().zip(details.styles) {
                    relinking.links[*slot].style = LinkStyleSource::Source(style);
                }
            }
        }
    }

    let mut saved: Vec<u8> = Vec::new();
    // A failed reservation only costs the doubling that the reservation avoids.
    let _ = saved.try_reserve_exact((estimate / 100 * SAVE_BUFFER_FACTOR_PERCENT) as usize);
    output
        .save_to_writer(&mut saved)
        .map_err(|_| Failure::general(CODE_WRITE_FAILED))?;
    drop(output);

    let mut chunks = ChunkWriter::new(out);
    let finished = finish_organized(saved, &creator, relinking, password.as_ref(), &mut chunks);
    if let Some(error) = chunks.error.take() {
        return Err(Failure::Io(error));
    }
    finished.map_err(|error| Failure::General {
        code: CODE_WRITE_FAILED.into(),
        detail: Some(error.to_string()),
    })?;
    let bytes = chunks.finish()?;
    write_message(out, &Response::Organized { bytes }, &[])?;
    Ok(())
}

/// Checks the arguments of the request (design organize §3) and returns the output password.
fn validate(args: &OrganizeArgs) -> Result<Option<OutputPassword>, Failure> {
    let invalid = || Failure::general(CODE_INVALID_PARAMS);
    if args.pages.is_empty() || i32::try_from(args.pages.len()).is_err() {
        return Err(invalid());
    }
    let mut seen = HashSet::new();
    for page in &args.pages {
        if page.source as usize >= args.sources.len()
            || !matches!(page.rotation, 0 | 90 | 180 | 270)
            || !seen.insert((page.source, page.page))
        {
            return Err(invalid());
        }
    }
    args.output_password
        .as_ref()
        .map(|password| OutputPassword::new(password.as_str().to_owned()).map_err(|_| invalid()))
        .transpose()
}

/// Splits the pages of one source, which come in output order, into the stretches whose output
/// positions follow each other; each is copied with one call (design organize §3).
fn runs<'a, 'b>(used: &'a [(u32, &'b OrganizePage)]) -> Vec<&'a [(u32, &'b OrganizePage)]> {
    let mut runs = Vec::new();
    let mut start = 0;
    for index in 1..=used.len() {
        if index == used.len() || used[index].0 != used[index - 1].0 + 1 {
            runs.push(&used[start..index]);
            start = index;
        }
    }
    runs.retain(|run| !run.is_empty());
    runs
}

/// Adds `rotation` degrees to the rotation of output page `index`.
fn turn(output: &PdfDocument<'_>, index: usize, rotation: u16) -> Result<(), &'static str> {
    if rotation == 0 {
        return Ok(());
    }
    let mut page = output
        .pages()
        .get(index as i32)
        .map_err(|_| CODE_PDF_OPEN_FAILED)?;
    let own = page
        .rotation()
        .map_err(|_| CODE_PDF_OPEN_FAILED)?
        .as_degrees() as u16;
    page.set_rotation(match (own + rotation) % 360 {
        90 => PdfPageRenderRotation::Degrees90,
        180 => PdfPageRenderRotation::Degrees180,
        270 => PdfPageRenderRotation::Degrees270,
        _ => PdfPageRenderRotation::None,
    });
    Ok(())
}

/// Prepares page `index` of `doc` for copying to output position `position`: notes its links
/// (design organize §1, step 3), then flattens it if it has an annotation other than a link
/// (step 4). Every annotation kind but links, form fields included, is drawn into the page and
/// removed, and so are the links with them, which is why they are noted first.
fn prepare_page(
    doc: &PdfDocument<'_>,
    index: u32,
    position: u32,
    kept: &HashMap<u32, u32>,
    links: &mut Vec<OutputLink>,
    flattened_links: &mut FlattenedLinks,
) -> Result<(), &'static str> {
    let mut page = doc
        .pages()
        .get(index as i32)
        .map_err(|_| CODE_PDF_OPEN_FAILED)?;
    let flattens = has_annotation_besides_links(&page);
    note_links(
        &page,
        index,
        position,
        flattens,
        kept,
        links,
        flattened_links,
    );
    if !flattens {
        return Ok(());
    }
    // pdfium builds the appearance of an annotation that has none only when it draws the page.
    page.render_with_config(&PdfRenderConfig::new().set_target_width(FLATTEN_PRERENDER_WIDTH))
        .map_err(|_| CODE_PDF_OPEN_FAILED)?;
    // Flattening draws only the annotations that print.
    let annotations = page.annotations();
    for position in annotations.as_range() {
        let mut annotation = annotations
            .get(position)
            .map_err(|_| CODE_PDF_OPEN_FAILED)?;
        if !annotation.is_hidden() && !annotation.is_printed() {
            annotation
                .set_is_printed(true)
                .map_err(|_| CODE_PDF_OPEN_FAILED)?;
        }
    }
    page.flatten().map_err(|_| CODE_PDF_OPEN_FAILED)
}

fn has_annotation_besides_links(page: &PdfPage<'_>) -> bool {
    page.annotations()
        .iter()
        .any(|annotation| annotation.annotation_type() != PdfPageAnnotationType::Link)
}

/// The links of one source that sit on pages that were flattened. Flattening drops the
/// annotations pdfium would otherwise copy, so their frames come from the file (`keys`), and
/// `slots` are the places of these links in the links of the output.
#[derive(Default)]
struct FlattenedLinks {
    keys: Vec<SourceLinkKey>,
    slots: Vec<usize>,
}

/// Adds the links of `page` that lead somewhere the output has to `links`.
///
/// The links are taken from `annotations()` one by one: `links()` returns a link several times
/// on a page that mixes links with other annotations. A link that is left out still counts in
/// `nth`, because the output's copy of the page keeps all of them in the order of the source.
fn note_links(
    page: &PdfPage<'_>,
    index: u32,
    position: u32,
    flattens: bool,
    kept: &HashMap<u32, u32>,
    links: &mut Vec<OutputLink>,
    flattened_links: &mut FlattenedLinks,
) {
    let mut nth: u32 = 0;
    for annotation in page.annotations().iter() {
        let Some(annotation) = annotation.as_link_annotation() else {
            continue;
        };
        let this = nth;
        nth += 1;
        let Ok(link) = annotation.link() else {
            continue;
        };
        let Ok(rect) = link.rect() else {
            continue;
        };
        let Reach::Kept(target) = reach(link.destination(), link.action(), kept) else {
            continue;
        };
        let (left, right) = (rect.left().value, rect.right().value);
        let (bottom, top) = (rect.bottom().value, rect.top().value);
        let rect = [
            left.min(right),
            bottom.min(top),
            left.max(right),
            bottom.max(top),
        ];
        let style = if flattens {
            flattened_links.keys.push(SourceLinkKey {
                page: index,
                nth: this,
                rect,
            });
            flattened_links.slots.push(links.len());
            LinkStyleSource::Source(None)
        } else {
            LinkStyleSource::Copied { nth: this }
        };
        links.push(OutputLink {
            page: position,
            rect,
            target,
            style,
        });
    }
}

/// Where a destination or an action leads, as far as the output is concerned.
enum Reach {
    Kept(LinkTarget),
    /// A page of the source that is not in the output.
    Gone,
    /// Nothing the output can use: no destination, or an action other than a URI or a page.
    Nothing,
}

/// Where a bookmark or a link leads: its destination, or else its action. Of the actions only a
/// URI and a destination in the same file count; pdfium resolves named destinations.
fn reach(
    destination: Option<PdfDestination<'_>>,
    action: Option<PdfAction<'_>>,
    kept: &HashMap<u32, u32>,
) -> Reach {
    if let Some(destination) = destination {
        return reach_page(&destination, kept);
    }
    let Some(action) = action else {
        return Reach::Nothing;
    };
    if let Some(uri) = action.as_uri_action() {
        return uri
            .uri()
            .map_or(Reach::Nothing, |uri| Reach::Kept(LinkTarget::Uri(uri)));
    }
    if let Some(local) = action.as_local_destination_action() {
        return local
            .destination()
            .map_or(Reach::Nothing, |destination| reach_page(&destination, kept));
    }
    Reach::Nothing
}

fn reach_page(destination: &PdfDestination<'_>, kept: &HashMap<u32, u32>) -> Reach {
    let Ok(page) = destination.page_index() else {
        return Reach::Nothing;
    };
    let Some(&position) = u32::try_from(page).ok().and_then(|page| kept.get(&page)) else {
        return Reach::Gone;
    };
    // A view pdfium cannot tell is shown whole, the safest of the views.
    let view = destination
        .view_settings()
        .map_or(DestinationView::Fit, view_of);
    Reach::Kept(LinkTarget::Page {
        page: position,
        view,
    })
}

fn view_of(settings: PdfDestinationViewSettings) -> DestinationView {
    use PdfDestinationViewSettings::{
        FitBoundsHorizontallyToWindow, FitBoundsToWindow, FitBoundsVerticallyToWindow,
        FitPageHorizontallyToWindow, FitPageToRectangle, FitPageToWindow,
        FitPageVerticallyToWindow, SpecificCoordinatesAndZoom, Unknown,
    };
    match settings {
        SpecificCoordinatesAndZoom(left, top, zoom) => DestinationView::Xyz {
            left: left.map(|left| left.value),
            top: top.map(|top| top.value),
            zoom,
        },
        FitPageToWindow | Unknown => DestinationView::Fit,
        FitPageHorizontallyToWindow(top) => DestinationView::FitH {
            top: top.map(|top| top.value),
        },
        FitPageVerticallyToWindow(left) => DestinationView::FitV {
            left: left.map(|left| left.value),
        },
        FitPageToRectangle(rect) => DestinationView::FitR {
            left: rect.left().value,
            bottom: rect.bottom().value,
            right: rect.right().value,
            top: rect.top().value,
        },
        FitBoundsToWindow => DestinationView::FitB,
        FitBoundsHorizontallyToWindow(top) => DestinationView::FitBH {
            top: top.map(|top| top.value),
        },
        FitBoundsVerticallyToWindow(left) => DestinationView::FitBV {
            left: left.map(|left| left.value),
        },
    }
}

/// The bookmarks of `doc` that the output keeps (design organize §1, step 2): those that lead to
/// a page of the output or to a URL, and the headings that have something below them. A bookmark
/// that leads to a page the output lacks is left out and its kept children move up to its place.
fn outline_of(doc: &PdfDocument<'_>, kept: &HashMap<u32, u32>) -> Vec<OutlineItem> {
    let mut seen = 0;
    outline_level(doc.bookmarks().root(), kept, 0, &mut seen)
}

/// The bookmarks from `first` on, which are siblings, at nesting depth `depth`; `seen` counts the
/// bookmarks read from the source so far.
fn outline_level(
    first: Option<PdfBookmark<'_>>,
    kept: &HashMap<u32, u32>,
    depth: usize,
    seen: &mut usize,
) -> Vec<OutlineItem> {
    let mut items = Vec::new();
    let mut current = first;
    while let Some(bookmark) = current {
        if *seen >= MAX_OUTLINE_ITEMS {
            break;
        }
        *seen += 1;
        let children = if depth + 1 < MAX_OUTLINE_DEPTH {
            outline_level(bookmark.first_child(), kept, depth + 1, seen)
        } else {
            Vec::new()
        };
        match reach(bookmark.destination(), bookmark.action(), kept) {
            Reach::Gone => items.extend(children),
            Reach::Nothing if children.is_empty() => {}
            reached => items.push(OutlineItem {
                title: bookmark.title().unwrap_or_default(),
                target: match reached {
                    Reach::Kept(target) => Some(target),
                    Reach::Gone | Reach::Nothing => None,
                },
                children,
            }),
        }
        current = bookmark.next_sibling();
    }
    items
}

/// Whether lopdf may read a file of `file_len` bytes when the worker is using `usage` bytes: the
/// most it needs for the file has to fit in what is left under the worker's limit. Without a
/// usage the answer is no.
///
/// The budget is not a fixed amount because the usage differs: little for the first source, and
/// a lot for a later one, which comes after the pages of the earlier sources.
fn fits_in_budget(file_len: u64, usage: Option<u64>) -> bool {
    let Some(usage) = usage else {
        return false;
    };
    let Some(estimate) = file_len.checked_mul(RAW_MEMORY_FACTOR) else {
        return false;
    };
    let budget = WORKER_MEMORY_LIMIT
        .saturating_sub(usage)
        .saturating_sub(RAW_MARGIN);
    estimate <= budget
}

/// Reads the view settings and the link frames of `source` with lopdf, if the memory allows it
/// (design organize §1, "元の PDF を `lopdf` で読む"). `None` means that they are not known; the
/// output is then made without them, not failed.
fn read_raw(
    source: &OrganizeSource,
    page_count: u32,
    want_view: bool,
    links: &[SourceLinkKey],
) -> Option<SourceDetails> {
    let file_len = std::fs::metadata(&source.path).ok()?.len();
    // Read just before lopdf starts, because the usage grows with every source.
    if !fits_in_budget(file_len, current_usage()) {
        return None;
    }
    read_source_details(
        &source.path,
        source.password.as_deref(),
        page_count,
        want_view,
        links,
    )
}

/// Sends what it is given as `OrganizeChunk` messages of `OUTPUT_CHUNK_BYTES`.
struct ChunkWriter<'a, W: Write> {
    out: &'a mut W,
    buffer: Vec<u8>,
    sent: u64,
    /// The first error of `out`; `Write` can only carry it through `finish_organized` as text.
    error: Option<io::Error>,
}

impl<'a, W: Write> ChunkWriter<'a, W> {
    fn new(out: &'a mut W) -> Self {
        Self {
            out,
            buffer: Vec::with_capacity(OUTPUT_CHUNK_BYTES),
            sent: 0,
            error: None,
        }
    }

    fn send(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        write_message(self.out, &Response::OrganizeChunk, &self.buffer)?;
        self.sent += self.buffer.len() as u64;
        self.buffer.clear();
        Ok(())
    }

    /// Sends the rest and returns the total length sent.
    fn finish(mut self) -> io::Result<u64> {
        self.send()?;
        Ok(self.sent)
    }
}

impl<W: Write> Write for ChunkWriter<'_, W> {
    fn write(&mut self, mut data: &[u8]) -> io::Result<usize> {
        let total = data.len();
        while !data.is_empty() {
            let room = OUTPUT_CHUNK_BYTES - self.buffer.len();
            let (now, later) = data.split_at(room.min(data.len()));
            self.buffer.extend_from_slice(now);
            data = later;
            if self.buffer.len() == OUTPUT_CHUNK_BYTES
                && let Err(error) = self.send()
            {
                self.error = Some(io::Error::new(error.kind(), error.to_string()));
                return Err(error);
            }
        }
        Ok(total)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What is left of the worker's limit for lopdf when the worker uses `usage`.
    fn budget(usage: u64) -> u64 {
        WORKER_MEMORY_LIMIT - usage - RAW_MARGIN
    }

    #[test]
    fn a_file_whose_estimate_is_exactly_the_budget_is_read() {
        let usage = 600 * 1024 * 1024;
        let file_len = budget(usage) / RAW_MEMORY_FACTOR;
        assert_eq!(file_len * RAW_MEMORY_FACTOR, budget(usage), "an exact fit");
        assert!(fits_in_budget(file_len, Some(usage)));
    }

    #[test]
    fn a_file_one_byte_over_is_not_read() {
        let usage = 600 * 1024 * 1024;
        let file_len = budget(usage) / RAW_MEMORY_FACTOR;
        assert!(!fits_in_budget(file_len + 1, Some(usage)));
        // One byte more of usage takes one byte off the budget.
        assert!(!fits_in_budget(file_len, Some(usage + 1)));
    }

    #[test]
    fn nothing_is_read_when_the_worker_is_at_or_over_its_limit() {
        for usage in [
            WORKER_MEMORY_LIMIT - RAW_MARGIN + 1,
            WORKER_MEMORY_LIMIT,
            WORKER_MEMORY_LIMIT + 1,
            u64::MAX,
        ] {
            assert!(!fits_in_budget(1, Some(usage)), "usage {usage}");
        }
        // With no room left, only an empty file has an estimate that fits.
        assert!(fits_in_budget(0, Some(WORKER_MEMORY_LIMIT - RAW_MARGIN)));
        assert!(!fits_in_budget(0, None));
    }

    #[test]
    fn an_estimate_that_overflows_is_not_read() {
        assert!(!fits_in_budget(u64::MAX / RAW_MEMORY_FACTOR + 1, Some(0)));
        assert!(!fits_in_budget(u64::MAX, Some(0)));
    }

    #[test]
    fn no_usage_means_no_reading() {
        assert!(!fits_in_budget(1, None));
    }
}
