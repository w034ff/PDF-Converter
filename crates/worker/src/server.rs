//! The worker side: answers requests on stdin/stdout until stdin closes
//! (design §5). This is the only code that loads pdfium.

use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use image::ExtendedColorType;
use image::ImageEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use pdfium_render::prelude::{
    PdfColor, PdfDocument, PdfPage, PdfRenderConfig, Pdfium, PdfiumError, PdfiumInternalError,
};

use crate::crop::{self, RegionRequest};
use crate::layout::Detector;
use crate::organize::{self, OrganizeArgs};
use crate::protocol::{
    PageDimensions, RenderColor, RenderFormat, Request, Response, read_message, write_message,
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

/// Error code for a pdfium library that cannot be loaded or used.
pub(crate) const CODE_PDFIUM_UNAVAILABLE: &str = "PdfiumUnavailable";

/// Error code when opening or decoding a PDF fails.
pub(crate) const CODE_PDF_OPEN_FAILED: &str = "PdfOpenFailed";

/// Error code when a PDF requires a password to open.
const CODE_PASSWORD_PROTECTED: &str = "PasswordProtected";

/// Error code when reading a file fails at the I/O level.
const CODE_READ_FAILED: &str = "ReadFailed";

/// Error code when a PDF exceeds the maximum supported page count.
const CODE_TOO_MANY_PAGES: &str = "TooManyPages";

/// Error code when rendered page dimensions exceed the maximum pixel limit.
pub(crate) const CODE_RENDER_TOO_LARGE: &str = "RenderTooLarge";

/// Error code when arguments are out of range or no document is open.
pub(crate) const CODE_INVALID_PARAMS: &str = "InvalidParams";

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
        match OrganizeArgs::from_request(request) {
            Ok(args) => {
                // Only `Organize` answers with several messages, so it bypasses `handle`.
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

pub(crate) fn check_page_count(count: u32) -> Result<(), Response> {
    // Every screen assumes an open PDF has a first page to show.
    if count == 0 {
        Err(Response::Error {
            code: CODE_PDF_OPEN_FAILED.into(),
            detail: None,
        })
    } else if count > MAX_PDF_PAGES {
        Err(Response::Error {
            code: CODE_TOO_MANY_PAGES.into(),
            detail: Some(format_number_with_commas(MAX_PDF_PAGES)),
        })
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
        match request {
            Request::Hello => match self.pdfium() {
                Ok(pdfium) => match pdfium.create_new_pdf() {
                    Ok(_) => (Response::Hello, Vec::new()),
                    Err(e) => (unavailable(&e), Vec::new()),
                },
                Err(response) => (response, Vec::new()),
            },
            Request::Open { path, password } => {
                #[cfg(feature = "test-hooks")]
                if path.file_name().and_then(|n| n.to_str()) == Some(CRASH_ON_OPEN_FILE_NAME) {
                    std::process::abort();
                }
                self.document = None;
                let pdfium = match self.pdfium() {
                    Ok(p) => p,
                    Err(resp) => return (resp, Vec::new()),
                };
                let doc = match pdfium.load_pdf_from_file(&path, password.as_deref()) {
                    Ok(doc) => doc,
                    Err(e) => return (map_pdfium_open_error(&e), Vec::new()),
                };
                let page_count = doc.pages().len() as u32;
                if let Err(resp) = check_page_count(page_count) {
                    return (resp, Vec::new());
                }
                let page_sizes = match doc.pages().page_sizes() {
                    Ok(sizes) => sizes,
                    Err(_) => {
                        return (
                            Response::Error {
                                code: CODE_PDF_OPEN_FAILED.into(),
                                detail: None,
                            },
                            Vec::new(),
                        );
                    }
                };
                let pages = page_sizes
                    .into_iter()
                    .map(|rect| PageDimensions {
                        width_pt: rect.width().value,
                        height_pt: rect.height().value,
                    })
                    .collect();
                self.document = Some(doc);
                (Response::Open { page_count, pages }, Vec::new())
            }
            Request::Render {
                page,
                dpi,
                format,
                color,
            } => {
                if !is_render_dpi_in_range(dpi) {
                    return (
                        Response::Error {
                            code: CODE_INVALID_PARAMS.into(),
                            detail: None,
                        },
                        Vec::new(),
                    );
                }
                let pdf_page = match self.page_for_rendering(page) {
                    Ok(p) => p,
                    Err(resp) => return (resp, Vec::new()),
                };
                let width_pt = f64::from(pdf_page.width().value);
                let height_pt = f64::from(pdf_page.height().value);
                let width_px = (width_pt / 72.0 * (dpi as f64)).round() as u64;
                let height_px = (height_pt / 72.0 * (dpi as f64)).round() as u64;

                let total_pixels = width_px.saturating_mul(height_px);
                if total_pixels > MAX_RENDER_PIXELS {
                    return (
                        Response::Error {
                            code: CODE_RENDER_TOO_LARGE.into(),
                            detail: None,
                        },
                        Vec::new(),
                    );
                }

                let target_width = (width_px as u32).max(1);
                let target_height = (height_px as u32).max(1);

                let (pixels, color_type) =
                    match render_page_pixels(&pdf_page, target_width, target_height, color) {
                        Ok(p) => p,
                        Err(resp) => return (resp, Vec::new()),
                    };

                match encode_pixels(&pixels, target_width, target_height, color_type, format) {
                    Ok(encoded) => (Response::Render, encoded),
                    Err(response) => (response, Vec::new()),
                }
            }
            Request::Thumbnail { page, max_side } => {
                if max_side == 0 || max_side > PAGE_VIEW_MAX_SIDE {
                    return (
                        Response::Error {
                            code: CODE_INVALID_PARAMS.into(),
                            detail: None,
                        },
                        Vec::new(),
                    );
                }
                let pdf_page = match self.page_for_rendering(page) {
                    Ok(p) => p,
                    Err(resp) => return (resp, Vec::new()),
                };
                let (target_w, target_h) = size_for_long_side(
                    f64::from(pdf_page.width().value),
                    f64::from(pdf_page.height().value),
                    max_side,
                );

                let total_pixels = (target_w as u64).saturating_mul(target_h as u64);
                if total_pixels > MAX_RENDER_PIXELS {
                    return (
                        Response::Error {
                            code: CODE_RENDER_TOO_LARGE.into(),
                            detail: None,
                        },
                        Vec::new(),
                    );
                }

                let (pixels, color_type) =
                    match render_page_pixels(&pdf_page, target_w, target_h, RenderColor::Color) {
                        Ok(p) => p,
                        Err(resp) => return (resp, Vec::new()),
                    };

                let mut encoded = Vec::new();
                match PngEncoder::new(&mut encoded)
                    .write_image(&pixels, target_w, target_h, color_type)
                {
                    Ok(()) => (Response::Thumbnail, encoded),
                    Err(e) => (
                        Response::Error {
                            code: CODE_PDF_OPEN_FAILED.into(),
                            detail: Some(format!("{e:?}")),
                        },
                        Vec::new(),
                    ),
                }
            }
            Request::PageImages { page } => {
                let result = self
                    .document()
                    .and_then(|doc| crop::page_images(doc, page, MAX_RENDER_PIXELS));
                match result {
                    Ok(images) => (Response::PageImages { images }, Vec::new()),
                    Err(response) => (response, Vec::new()),
                }
            }
            Request::ImageOnTop { page, image_index } => {
                let result = self.document().and_then(|doc| {
                    crop::image_on_top(doc, page, image_index, crop::ON_TOP_CHECK_PIXELS)
                });
                match result {
                    Ok(on_top) => (Response::ImageOnTop { on_top }, Vec::new()),
                    Err(response) => (response, Vec::new()),
                }
            }
            Request::Detect { page } => {
                let result = match &self.document {
                    Some(doc) => self.detector.detect(doc, page),
                    None => Err(error_response(CODE_INVALID_PARAMS, None)),
                };
                match result {
                    Ok(candidates) => (Response::Detect { candidates }, Vec::new()),
                    Err(response) => (response, Vec::new()),
                }
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
                let result = self
                    .document()
                    .and_then(|doc| crop::render_region(doc, &request, MAX_RENDER_PIXELS));
                match result {
                    Ok((response, image)) => (response, image),
                    Err(response) => (response, Vec::new()),
                }
            }
            // `run` sends `Organize` to `organize::run`, which answers with several messages.
            Request::Organize { .. } => (error_response(CODE_INVALID_PARAMS, None), Vec::new()),
            Request::Close => {
                self.document = None;
                (Response::Close, Vec::new())
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
                (Response::Allocated, Vec::new())
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
            .ok_or_else(|| error_response(CODE_INVALID_PARAMS, None))
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
pub(crate) fn error_response(code: &str, detail: Option<String>) -> Response {
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
        return Err(error_response(CODE_INVALID_PARAMS, None));
    }
    doc.pages()
        .get((page - 1) as i32)
        .map_err(|_| error_response(CODE_INVALID_PARAMS, None))
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
        .map_err(|e| error_response(CODE_PDF_OPEN_FAILED, Some(format!("{e:?}"))))
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
            Response::Error {
                code: CODE_PASSWORD_PROTECTED.into(),
                detail: None,
            }
        }
        PdfiumError::IoError(_) => Response::Error {
            code: CODE_READ_FAILED.into(),
            detail: None,
        },
        _ => Response::Error {
            code: CODE_PDF_OPEN_FAILED.into(),
            detail: None,
        },
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
        .map_err(|e| Response::Error {
            code: CODE_PDF_OPEN_FAILED.into(),
            detail: Some(format!("{e:?}")),
        })?;

    Ok(pixels_from_rgba(bitmap.as_rgba_bytes(), color, false))
}

fn unavailable(error: &dyn std::fmt::Debug) -> Response {
    Response::Error {
        code: CODE_PDFIUM_UNAVAILABLE.into(),
        detail: Some(format!("{error:?}")),
    }
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
                code: CODE_PDF_OPEN_FAILED.into(),
                detail: None,
            }
        );
        assert!(check_page_count(1).is_ok());
        assert!(check_page_count(10_000).is_ok());
        let err = check_page_count(10_001).unwrap_err();
        assert_eq!(
            err,
            Response::Error {
                code: CODE_TOO_MANY_PAGES.into(),
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
}
