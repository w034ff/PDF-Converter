//! The worker side: answers requests on stdin/stdout until stdin closes
//! (design §5). This is the only code that loads pdfium.

use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use image::ExtendedColorType;
use image::ImageEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use pdfconv_core::{ErrorCode, POINTS_PER_INCH};
use pdfium_render::prelude::{
    PdfColor, PdfDocument, PdfPage, PdfRenderConfig, Pdfium, PdfiumError, PdfiumInternalError,
};

use crate::crop::{self, RegionRequest};
use crate::layout::Detector;
use crate::organize::{self, OrganizeArgs};
use crate::protocol::{
    PageDimensions, RenderColor, RenderFormat, Request, Response, WorkerCode, read_message,
    write_message,
};
use crate::{
    JPEG_QUALITY, MAX_PDF_PAGES, MAX_RENDER_PIXELS, PAGE_VIEW_MAX_SIDE, is_render_dpi_in_range,
};

/// How much memory one worker may use (design §5.3). Allocations beyond it
/// fail and end the worker, which the main process reports as a crash.
pub const WORKER_MEMORY_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// Name of a PDF file that causes the worker to abort on Open for testing (design §11.3).
#[cfg(feature = "test-hooks")]
pub const CRASH_ON_OPEN_FILE_NAME: &str = "crash-on-open-for-test.pdf";

/// Name of a source PDF that makes the worker abort on `Organize` for testing (design §11.3).
/// `Open` takes a file of this name like any other, so a test can add it to a list first.
#[cfg(feature = "test-hooks")]
pub const CRASH_ON_ORGANIZE_FILE_NAME: &str = "crash-on-organize-for-test.pdf";

/// Name of a source PDF that makes the worker stop answering `Organize` for testing, as
/// `HangForTest` does (design §11.3).
#[cfg(feature = "test-hooks")]
pub const HANG_ON_ORGANIZE_FILE_NAME: &str = "hang-on-organize-for-test.pdf";

/// Name of an image file that makes the worker abort on `ImageThumbnail` and `CheckImage` for
/// testing (design §11.3).
#[cfg(feature = "test-hooks")]
pub const CRASH_ON_IMAGE_FILE_NAME: &str = "crash-on-image-for-test.png";

/// Name of an image file that makes the worker stop answering `ImageThumbnail` and `CheckImage`
/// for testing, as `HangForTest` does (design §11.3).
#[cfg(feature = "test-hooks")]
pub const HANG_ON_IMAGE_FILE_NAME: &str = "hang-on-image-for-test.png";

/// Runs the worker loop (design §5.4 for the arguments). `library_dir` is
/// the folder holding the bundled pdfium library, loaded on the first request
/// that needs it. `onnxruntime` is the onnxruntime library file and `model`
/// the layout model file, both loaded on the first `Detect`.
///
/// # Errors
///
/// Returns an I/O error when stdin or stdout fails. A clean end of stdin
/// (the main process closed the pipe or exited) returns `Ok(())`.
pub fn run(library_dir: &Path, onnxruntime: &Path, model: &Path) -> io::Result<()> {
    apply_memory_limit()?;
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = BufWriter::new(io::stdout().lock());
    let mut state = State {
        library_dir: library_dir.to_path_buf(),
        pdfium: None,
        document: None,
        detector: Detector::new(onnxruntime, model),
    };
    while let Some((request, _body)) = read_message::<_, Request>(&mut input)? {
        // Only `Organize` and `CheckImage` answer with several messages, so they bypass `handle`.
        if let Request::CheckImage { path } = &request {
            #[cfg(feature = "test-hooks")]
            fail_image_for_test(path);
            crate::image::check(path, &mut output)?;
            continue;
        }
        match OrganizeArgs::from_request(request) {
            Ok(args) => {
                #[cfg(feature = "test-hooks")]
                fail_organize_for_test(&args);
                state.document = None;
                match state.pdfium() {
                    Ok(pdfium) => organize::run(pdfium, args, &mut output)?,
                    Err(response) => write_message(&mut output, &response, &[])?,
                }
            }
            Err(request) => {
                let (response, body) = state.handle(request);
                write_message(&mut output, &response, &body)?;
            }
        }
    }
    Ok(())
}

/// Aborts or hangs when `path` has one of the image names the tests use.
#[cfg(feature = "test-hooks")]
fn fail_image_for_test(path: &Path) {
    match path.file_name().and_then(|name| name.to_str()) {
        Some(CRASH_ON_IMAGE_FILE_NAME) => std::process::abort(),
        Some(HANG_ON_IMAGE_FILE_NAME) => loop {
            std::thread::park();
        },
        _ => {}
    }
}

/// Aborts or hangs when a source of `args` has one of the names the tests use.
#[cfg(feature = "test-hooks")]
fn fail_organize_for_test(args: &OrganizeArgs) {
    for source in &args.sources {
        match source.path.file_name().and_then(|name| name.to_str()) {
            Some(CRASH_ON_ORGANIZE_FILE_NAME) => std::process::abort(),
            Some(HANG_ON_ORGANIZE_FILE_NAME) => loop {
                std::thread::park();
            },
            _ => {}
        }
    }
}

pub(crate) fn check_page_count(count: u32) -> Result<(), Response> {
    // Every screen assumes an open PDF has a first page to show.
    if count == 0 {
        Err(error_response(ErrorCode::PdfOpenFailed, None))
    } else if count > MAX_PDF_PAGES {
        Err(error_response(
            ErrorCode::TooManyPages,
            Some(format_number_with_commas(MAX_PDF_PAGES)),
        ))
    } else {
        Ok(())
    }
}

fn format_number_with_commas(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    for (i, &ch) in chars.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

struct State {
    library_dir: PathBuf,
    pdfium: Option<&'static Pdfium>,
    document: Option<PdfDocument<'static>>,
    /// Outlives `Open` and `Close`: the model stays loaded until the worker ends.
    detector: Detector,
}

impl State {
    fn handle(&mut self, request: Request) -> (Response, Vec<u8>) {
        self.answer(request)
            .unwrap_or_else(|response| (response, Vec::new()))
    }

    /// The answer to `request` and its body, or the error response, which has no body.
    fn answer(&mut self, request: Request) -> Result<(Response, Vec<u8>), Response> {
        match request {
            Request::Hello => {
                self.pdfium()?
                    .create_new_pdf()
                    .map_err(|e| unavailable(&e))?;
                Ok((Response::Hello, Vec::new()))
            }
            Request::Open { path, password } => {
                #[cfg(feature = "test-hooks")]
                if path.file_name().and_then(|n| n.to_str()) == Some(CRASH_ON_OPEN_FILE_NAME) {
                    std::process::abort();
                }
                self.document = None;
                let doc = self
                    .pdfium()?
                    .load_pdf_from_file(&path, password.as_deref())
                    .map_err(|e| map_pdfium_open_error(&e))?;
                let page_count = doc.pages().len() as u32;
                check_page_count(page_count)?;
                let pages = doc
                    .pages()
                    .page_sizes()
                    .map_err(|_| error_response(ErrorCode::PdfOpenFailed, None))?
                    .into_iter()
                    .map(|rect| PageDimensions {
                        width_pt: rect.width().value,
                        height_pt: rect.height().value,
                    })
                    .collect();
                self.document = Some(doc);
                Ok((Response::Open { page_count, pages }, Vec::new()))
            }
            Request::Render {
                page,
                dpi,
                format,
                color,
            } => {
                if !is_render_dpi_in_range(dpi) {
                    return Err(error_response(ErrorCode::InvalidParams, None));
                }
                let pdf_page = self.page_for_rendering(page)?;
                let width_pt = f64::from(pdf_page.width().value);
                let height_pt = f64::from(pdf_page.height().value);
                let points_per_inch = f64::from(POINTS_PER_INCH);
                let width_px = (width_pt / points_per_inch * (dpi as f64)).round() as u64;
                let height_px = (height_pt / points_per_inch * (dpi as f64)).round() as u64;

                let total_pixels = width_px.saturating_mul(height_px);
                if total_pixels > MAX_RENDER_PIXELS {
                    return Err(error_response(ErrorCode::RenderTooLarge, None));
                }

                let target_width = (width_px as u32).max(1);
                let target_height = (height_px as u32).max(1);

                let (pixels, color_type) =
                    render_page_pixels(&pdf_page, target_width, target_height, color)?;
                let encoded =
                    encode_pixels(&pixels, target_width, target_height, color_type, format)?;
                Ok((Response::Render, encoded))
            }
            Request::Thumbnail { page, max_side } => {
                if max_side == 0 || max_side > PAGE_VIEW_MAX_SIDE {
                    return Err(error_response(ErrorCode::InvalidParams, None));
                }
                let pdf_page = self.page_for_rendering(page)?;
                let (target_w, target_h) = size_for_long_side(
                    f64::from(pdf_page.width().value),
                    f64::from(pdf_page.height().value),
                    max_side,
                );

                let total_pixels = (target_w as u64).saturating_mul(target_h as u64);
                if total_pixels > MAX_RENDER_PIXELS {
                    return Err(error_response(ErrorCode::RenderTooLarge, None));
                }

                let (pixels, color_type) =
                    render_page_pixels(&pdf_page, target_w, target_h, RenderColor::Color)?;
                let encoded =
                    encode_pixels(&pixels, target_w, target_h, color_type, RenderFormat::Png)?;
                Ok((Response::Thumbnail, encoded))
            }
            Request::PageImages { page } => {
                let images = crop::page_images(self.document()?, page, MAX_RENDER_PIXELS)?;
                Ok((Response::PageImages { images }, Vec::new()))
            }
            Request::ImageOnTop { page, image_index } => {
                let on_top = crop::image_on_top(
                    self.document()?,
                    page,
                    image_index,
                    crop::ON_TOP_CHECK_PIXELS,
                )?;
                Ok((Response::ImageOnTop { on_top }, Vec::new()))
            }
            Request::Detect { page } => {
                let doc = self
                    .document
                    .as_ref()
                    .ok_or_else(|| error_response(ErrorCode::InvalidParams, None))?;
                let candidates = self.detector.detect(doc, page)?;
                Ok((Response::Detect { candidates }, Vec::new()))
            }
            Request::RenderRegion {
                page,
                rect_pt,
                image_index,
                dpi,
                format,
                color,
                photo_native,
                photo_only,
            } => {
                let request = RegionRequest {
                    page,
                    rect: rect_pt,
                    image_index,
                    dpi,
                    format,
                    color,
                    photo_native,
                    photo_only,
                };
                crop::render_region(self.document()?, &request, MAX_RENDER_PIXELS)
            }
            Request::ImageThumbnail { path } => {
                #[cfg(feature = "test-hooks")]
                fail_image_for_test(&path);
                Ok(crate::image::thumbnail(&path))
            }
            // `run` sends `Organize` to `organize::run` and `CheckImage` to `image::check`,
            // which answer with several messages.
            Request::Organize { .. } | Request::CheckImage { .. } => {
                Err(error_response(ErrorCode::InvalidParams, None))
            }
            Request::Close => {
                self.document = None;
                Ok((Response::Close, Vec::new()))
            }
            #[cfg(feature = "test-hooks")]
            Request::AllocateForTest { mebibytes } => {
                const MIB: usize = 1024 * 1024;
                let size = usize::try_from(mebibytes)
                    .unwrap_or(usize::MAX)
                    .saturating_mul(MIB);
                // Writing every page makes the operating system commit the
                // memory, so the limit is hit here rather than lazily later.
                let mut block = vec![0u8; size];
                for byte in block.iter_mut().step_by(4096) {
                    *byte = 1;
                }
                std::hint::black_box(&block);
                Ok((Response::Allocated, Vec::new()))
            }
            #[cfg(feature = "test-hooks")]
            Request::CrashForTest => std::process::abort(),
            #[cfg(feature = "test-hooks")]
            Request::HangForTest => loop {
                std::thread::park();
            },
        }
    }

    fn document(&self) -> Result<&PdfDocument<'static>, Response> {
        self.document
            .as_ref()
            .ok_or_else(|| error_response(ErrorCode::InvalidParams, None))
    }

    fn page_for_rendering(&self, page: u32) -> Result<PdfPage<'_>, Response> {
        load_page(self.document()?, page)
    }

    fn pdfium(&mut self) -> Result<&'static Pdfium, Response> {
        if self.pdfium.is_none() {
            let path = Pdfium::pdfium_platform_library_name_at_path(&self.library_dir);
            let bindings = Pdfium::bind_to_library(&path).map_err(|e| unavailable(&e))?;
            let instance = Pdfium::new(bindings);
            let static_ref: &'static Pdfium = Box::leak(Box::new(instance));
            self.pdfium = Some(static_ref);
        }
        self.pdfium
            .ok_or_else(|| unavailable(&"pdfium was not stored"))
    }
}

/// An error response with `code` (design §6.6).
pub(crate) fn error_response(code: impl Into<WorkerCode>, detail: Option<String>) -> Response {
    Response::Error {
        code: code.into(),
        detail,
    }
}

/// Loads page `page` (1-based) of `doc`, or `InvalidParams` when there is no
/// such page.
pub(crate) fn load_page<'a>(doc: &'a PdfDocument<'_>, page: u32) -> Result<PdfPage<'a>, Response> {
    let page_count = doc.pages().len() as u32;
    if page == 0 || page > page_count {
        return Err(error_response(ErrorCode::InvalidParams, None));
    }
    doc.pages()
        .get((page - 1) as i32)
        .map_err(|_| error_response(ErrorCode::InvalidParams, None))
}

/// Encodes `pixels` (`width` × `height`, laid out as `color_type`) in `format`
/// (design §4.4).
pub(crate) fn encode_pixels(
    pixels: &[u8],
    width: u32,
    height: u32,
    color_type: ExtendedColorType,
    format: RenderFormat,
) -> Result<Vec<u8>, Response> {
    let mut encoded = Vec::new();
    let result = match format {
        RenderFormat::Png => {
            PngEncoder::new(&mut encoded).write_image(pixels, width, height, color_type)
        }
        RenderFormat::Jpeg => JpegEncoder::new_with_quality(&mut encoded, JPEG_QUALITY)
            .write_image(pixels, width, height, color_type),
        RenderFormat::Webp => {
            WebPEncoder::new_lossless(&mut encoded).write_image(pixels, width, height, color_type)
        }
    };
    result
        .map(|()| encoded)
        .map_err(|e| error_response(ErrorCode::PdfOpenFailed, Some(format!("{e:?}"))))
}

/// Turns pdfium's RGBA bytes into the pixels to encode: RGB, or one gray
/// channel (design §4.4: `FPDF_GRAYSCALE` makes R = G = B). With `keep_alpha`
/// the alpha channel stays: RGBA, or gray and alpha.
pub(crate) fn pixels_from_rgba(
    rgba: Vec<u8>,
    color: RenderColor,
    keep_alpha: bool,
) -> (Vec<u8>, ExtendedColorType) {
    let pixels = rgba.as_chunks::<4>().0;
    match (color, keep_alpha) {
        (RenderColor::Color, true) => (rgba, ExtendedColorType::Rgba8),
        (RenderColor::Color, false) => (
            pixels.iter().flat_map(|p| [p[0], p[1], p[2]]).collect(),
            ExtendedColorType::Rgb8,
        ),
        (RenderColor::Gray, true) => (
            pixels.iter().flat_map(|p| [p[0], p[3]]).collect(),
            ExtendedColorType::La8,
        ),
        (RenderColor::Gray, false) => {
            (pixels.iter().map(|p| p[0]).collect(), ExtendedColorType::L8)
        }
    }
}

pub(crate) fn map_pdfium_open_error(error: &PdfiumError) -> Response {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            error_response(ErrorCode::PasswordProtected, None)
        }
        PdfiumError::IoError(_) => error_response(ErrorCode::ReadFailed, None),
        _ => error_response(ErrorCode::PdfOpenFailed, None),
    }
}

/// The pixel size of a page of `width_pt` × `height_pt` drawn with its longer
/// side at `long_side`; the shorter side is rounded and at least 1.
pub(crate) fn size_for_long_side(width_pt: f64, height_pt: f64, long_side: u32) -> (u32, u32) {
    let shorter = |short_pt: f64, long_pt: f64| {
        if long_pt > 0.0 {
            (f64::from(long_side) * short_pt / long_pt).round().max(1.0) as u32
        } else {
            1
        }
    };
    if width_pt >= height_pt {
        (long_side, shorter(height_pt, width_pt))
    } else {
        (shorter(width_pt, height_pt), long_side)
    }
}

/// Renders `page` at exactly `target_width` × `target_height` on white, as
/// RGB or gray pixels (design §4.4).
pub(crate) fn render_page_pixels(
    page: &PdfPage<'_>,
    target_width: u32,
    target_height: u32,
    color: RenderColor,
) -> Result<(Vec<u8>, ExtendedColorType), Response> {
    let mut config = PdfRenderConfig::new()
        .set_target_size(target_width as i32, target_height as i32)
        .set_clear_color(PdfColor::WHITE);
    if color == RenderColor::Gray {
        config = config.use_grayscale_rendering(true);
    }

    let bitmap = page
        .render_with_config(&config)
        .map_err(|e| error_response(ErrorCode::PdfOpenFailed, Some(format!("{e:?}"))))?;

    Ok(pixels_from_rgba(bitmap.as_rgba_bytes(), color, false))
}

fn unavailable(error: &dyn std::fmt::Debug) -> Response {
    error_response(WorkerCode::PDFIUM_UNAVAILABLE, Some(format!("{error:?}")))
}

#[cfg(target_os = "linux")]
fn apply_memory_limit() -> io::Result<()> {
    // RLIMIT_AS caps the address space; it must be set before pdfium is
    // loaded so that pdfium's own allocations are covered too.
    rlimit::setrlimit(
        rlimit::Resource::AS,
        WORKER_MEMORY_LIMIT,
        WORKER_MEMORY_LIMIT,
    )
}

#[cfg(windows)]
fn apply_memory_limit() -> io::Result<()> {
    // On Windows the main process puts the worker in a job object with the
    // limit before sending the first request (crate::windows_job).
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_page_count_bounds() {
        assert_eq!(
            check_page_count(0).unwrap_err(),
            Response::Error {
                code: ErrorCode::PdfOpenFailed.into(),
                detail: None,
            }
        );
        assert!(check_page_count(1).is_ok());
        assert!(check_page_count(10_000).is_ok());
        let err = check_page_count(10_001).unwrap_err();
        assert_eq!(
            err,
            Response::Error {
                code: ErrorCode::TooManyPages.into(),
                detail: Some("10,000".into()),
            }
        );
    }

    #[test]
    fn formats_numbers_with_commas() {
        assert_eq!(format_number_with_commas(0), "0");
        assert_eq!(format_number_with_commas(999), "999");
        assert_eq!(format_number_with_commas(1000), "1,000");
        assert_eq!(format_number_with_commas(10000), "10,000");
        assert_eq!(format_number_with_commas(1000000), "1,000,000");
    }

    #[cfg(feature = "test-hooks")]
    #[test]
    fn crash_on_open_file_name_constant() {
        assert_eq!(CRASH_ON_OPEN_FILE_NAME, "crash-on-open-for-test.pdf");
    }

    #[cfg(feature = "test-hooks")]
    #[test]
    fn image_test_file_name_constants() {
        assert_eq!(CRASH_ON_IMAGE_FILE_NAME, "crash-on-image-for-test.png");
        assert_eq!(HANG_ON_IMAGE_FILE_NAME, "hang-on-image-for-test.png");
    }

    #[cfg(feature = "test-hooks")]
    #[test]
    fn organize_test_file_name_constants() {
        assert_eq!(
            CRASH_ON_ORGANIZE_FILE_NAME,
            "crash-on-organize-for-test.pdf"
        );
        assert_eq!(HANG_ON_ORGANIZE_FILE_NAME, "hang-on-organize-for-test.pdf");
    }
}
