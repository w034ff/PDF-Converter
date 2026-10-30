//! The worker's side of organizing (design organize §1, §3): copies pages of several PDFs into
//! a new document with pdfium, then hands the bytes to `pdfconv-core` to rewrite, encrypt and
//! send. Bookmarks, links and the view settings are not carried over here.

use std::collections::HashSet;
use std::io::{self, Write};

use pdfconv_core::{OutputPassword, finish_organized};
use pdfium_render::prelude::{
    PdfDocument, PdfPage, PdfPageAnnotationCommon, PdfPageAnnotationType, PdfPageRenderRotation,
    PdfRenderConfig, Pdfium,
};

use crate::protocol::{
    OUTPUT_CHUNK_BYTES, OrganizePage, OrganizeSource, Password, Request, Response, write_message,
};
use crate::server::{
    CODE_INVALID_PARAMS, CODE_PDF_OPEN_FAILED, CODE_PDFIUM_UNAVAILABLE, check_page_count,
    map_pdfium_open_error,
};

/// Width in pixels of the render that makes pdfium build the appearance of annotations that have
/// none, before the page is flattened (design organize §1, step 4). The picture is thrown away.
pub const FLATTEN_PRERENDER_WIDTH: i32 = 50;

/// How much larger than the estimate the buffer for the saved document is made, so that saving
/// does not grow it by doubling (design organize §1, step 6).
const SAVE_BUFFER_FACTOR_PERCENT: u64 = 110;

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

        for (_, page) in used {
            flatten_if_annotated(&doc, page.page)
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
    }

    let mut saved: Vec<u8> = Vec::new();
    // A failed reservation only costs the doubling that the reservation avoids.
    let _ = saved.try_reserve_exact((estimate / 100 * SAVE_BUFFER_FACTOR_PERCENT) as usize);
    output
        .save_to_writer(&mut saved)
        .map_err(|_| Failure::general(CODE_WRITE_FAILED))?;
    drop(output);

    let mut chunks = ChunkWriter::new(out);
    let finished = finish_organized(saved, &creator, password.as_ref(), &mut chunks);
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

/// Flattens page `index` of `doc` into its content if it has an annotation other than a link
/// (design organize §1, step 4). Links are kept as annotations so that they can be attached to
/// the output; every other kind, form fields included, is drawn into the page and removed.
fn flatten_if_annotated(doc: &PdfDocument<'_>, index: u32) -> Result<(), &'static str> {
    let mut page = doc
        .pages()
        .get(index as i32)
        .map_err(|_| CODE_PDF_OPEN_FAILED)?;
    if !has_annotation_besides_links(&page) {
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
