//! The worker side: answers requests on stdin/stdout until stdin closes
//! (design §5). This is the only code that loads pdfium.

use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use image::ExtendedColorType;
use image::ImageEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use pdfium_render::prelude::{
    PdfColor, PdfDocument, PdfPage, PdfRenderConfig, Pdfium, PdfiumError, PdfiumInternalError,
};

use crate::protocol::{
    PageDimensions, RenderFormat, Request, Response, read_message, write_message,
};
use crate::{JPEG_QUALITY, MAX_PDF_PAGES, MAX_RENDER_PIXELS};

/// How much memory one worker may use (design §5.3). Allocations beyond it
/// fail and end the worker, which the main process reports as a crash.
pub const WORKER_MEMORY_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// Error code for a pdfium library that cannot be loaded or used.
const CODE_PDFIUM_UNAVAILABLE: &str = "PdfiumUnavailable";

/// Runs the worker loop. `library_dir` is the folder holding the bundled
/// pdfium library; pdfium is loaded on the first request that needs it.
///
/// # Errors
///
/// Returns an I/O error when stdin or stdout fails. A clean end of stdin
/// (the main process closed the pipe or exited) returns `Ok(())`.
pub fn run(library_dir: &Path) -> io::Result<()> {
    apply_memory_limit()?;
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = BufWriter::new(io::stdout().lock());
    let mut state = State {
        library_dir: library_dir.to_path_buf(),
        pdfium: None,
        document: None,
    };
    while let Some((request, _body)) = read_message::<_, Request>(&mut input)? {
        let (response, body) = state.handle(request);
        write_message(&mut output, &response, &body)?;
    }
    Ok(())
}

/// Validates that a document page count does not exceed [`MAX_PDF_PAGES`] (design §5.1, §6.6).
pub fn check_page_count(count: u32) -> Result<(), Response> {
    if count > MAX_PDF_PAGES {
        Err(Response::Error {
            code: "TooManyPages".into(),
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
            Request::Open { path } => {
                self.document = None;
                let pdfium = match self.pdfium() {
                    Ok(p) => p,
                    Err(resp) => return (resp, Vec::new()),
                };
                let doc = match pdfium.load_pdf_from_file(&path, None) {
                    Ok(doc) => doc,
                    Err(e) => return (map_pdfium_open_error(&e), Vec::new()),
                };
                let page_count = doc.pages().len() as u32;
                if let Err(resp) = check_page_count(page_count) {
                    return (resp, Vec::new());
                }
                let mut pages = Vec::with_capacity(page_count as usize);
                for i in 0..(page_count as i32) {
                    match doc.pages().get(i) {
                        Ok(page) => {
                            pages.push(PageDimensions {
                                width_pt: page.width().value,
                                height_pt: page.height().value,
                            });
                        }
                        Err(_) => {
                            return (
                                Response::Error {
                                    code: "PdfOpenFailed".into(),
                                    detail: None,
                                },
                                Vec::new(),
                            );
                        }
                    }
                }
                self.document = Some(doc);
                (Response::Open { page_count, pages }, Vec::new())
            }
            Request::Render { page, dpi, format } => {
                let Some(doc) = &self.document else {
                    return (
                        Response::Error {
                            code: "InvalidParams".into(),
                            detail: None,
                        },
                        Vec::new(),
                    );
                };
                let page_count = doc.pages().len() as u32;
                if page == 0 || page > page_count || dpi == 0 {
                    return (
                        Response::Error {
                            code: "InvalidParams".into(),
                            detail: None,
                        },
                        Vec::new(),
                    );
                }
                let pdf_page = match doc.pages().get((page - 1) as i32) {
                    Ok(p) => p,
                    Err(_) => {
                        return (
                            Response::Error {
                                code: "InvalidParams".into(),
                                detail: None,
                            },
                            Vec::new(),
                        );
                    }
                };
                let width_pt = f64::from(pdf_page.width().value);
                let height_pt = f64::from(pdf_page.height().value);
                let width_px = (width_pt / 72.0 * (dpi as f64)).round() as u64;
                let height_px = (height_pt / 72.0 * (dpi as f64)).round() as u64;

                let total_pixels = width_px.saturating_mul(height_px);
                if total_pixels > MAX_RENDER_PIXELS {
                    return (
                        Response::Error {
                            code: "RenderTooLarge".into(),
                            detail: None,
                        },
                        Vec::new(),
                    );
                }

                let target_width = (width_px as u32).max(1);
                let target_height = (height_px as u32).max(1);

                let rgb = match render_page_to_rgb(&pdf_page, target_width, target_height) {
                    Ok(bytes) => bytes,
                    Err(resp) => return (resp, Vec::new()),
                };

                let mut encoded = Vec::new();
                let encode_result = match format {
                    RenderFormat::Png => PngEncoder::new(&mut encoded).write_image(
                        &rgb,
                        target_width,
                        target_height,
                        ExtendedColorType::Rgb8,
                    ),
                    RenderFormat::Jpeg => JpegEncoder::new_with_quality(&mut encoded, JPEG_QUALITY)
                        .write_image(&rgb, target_width, target_height, ExtendedColorType::Rgb8),
                };

                match encode_result {
                    Ok(()) => (Response::Render, encoded),
                    Err(e) => (
                        Response::Error {
                            code: "PdfOpenFailed".into(),
                            detail: Some(format!("{e:?}")),
                        },
                        Vec::new(),
                    ),
                }
            }
            Request::Thumbnail { page, max_side } => {
                let Some(doc) = &self.document else {
                    return (
                        Response::Error {
                            code: "InvalidParams".into(),
                            detail: None,
                        },
                        Vec::new(),
                    );
                };
                let page_count = doc.pages().len() as u32;
                if page == 0 || page > page_count || max_side == 0 {
                    return (
                        Response::Error {
                            code: "InvalidParams".into(),
                            detail: None,
                        },
                        Vec::new(),
                    );
                }
                let pdf_page = match doc.pages().get((page - 1) as i32) {
                    Ok(p) => p,
                    Err(_) => {
                        return (
                            Response::Error {
                                code: "InvalidParams".into(),
                                detail: None,
                            },
                            Vec::new(),
                        );
                    }
                };
                let width_pt = f64::from(pdf_page.width().value);
                let height_pt = f64::from(pdf_page.height().value);

                let (target_w, target_h) = if width_pt >= height_pt {
                    let w = max_side;
                    let h = if width_pt > 0.0 {
                        (f64::from(max_side) * height_pt / width_pt)
                            .round()
                            .max(1.0) as u32
                    } else {
                        1
                    };
                    (w, h)
                } else {
                    let h = max_side;
                    let w = if height_pt > 0.0 {
                        (f64::from(max_side) * width_pt / height_pt)
                            .round()
                            .max(1.0) as u32
                    } else {
                        1
                    };
                    (w, h)
                };

                let rgb = match render_page_to_rgb(&pdf_page, target_w, target_h) {
                    Ok(bytes) => bytes,
                    Err(resp) => return (resp, Vec::new()),
                };

                let mut encoded = Vec::new();
                match PngEncoder::new(&mut encoded).write_image(
                    &rgb,
                    target_w,
                    target_h,
                    ExtendedColorType::Rgb8,
                ) {
                    Ok(()) => (Response::Thumbnail, encoded),
                    Err(e) => (
                        Response::Error {
                            code: "PdfOpenFailed".into(),
                            detail: Some(format!("{e:?}")),
                        },
                        Vec::new(),
                    ),
                }
            }
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

fn map_pdfium_open_error(error: &PdfiumError) -> Response {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            Response::Error {
                code: "PasswordProtected".into(),
                detail: None,
            }
        }
        PdfiumError::IoError(_) => Response::Error {
            code: "ReadFailed".into(),
            detail: None,
        },
        _ => Response::Error {
            code: "PdfOpenFailed".into(),
            detail: None,
        },
    }
}

fn render_page_to_rgb(
    page: &PdfPage<'_>,
    target_width: u32,
    target_height: u32,
) -> Result<Vec<u8>, Response> {
    let config = PdfRenderConfig::new()
        .set_target_size(target_width as i32, target_height as i32)
        .set_clear_color(PdfColor::WHITE);

    let bitmap = page
        .render_with_config(&config)
        .map_err(|e| Response::Error {
            code: "PdfOpenFailed".into(),
            detail: Some(format!("{e:?}")),
        })?;

    let rgba = bitmap.as_rgba_bytes();
    let mut rgb = Vec::with_capacity((target_width as usize) * (target_height as usize) * 3);
    for chunk in rgba.as_chunks::<4>().0 {
        rgb.extend_from_slice(&chunk[0..3]);
    }
    Ok(rgb)
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
        assert!(check_page_count(0).is_ok());
        assert!(check_page_count(10_000).is_ok());
        let err = check_page_count(10_001).unwrap_err();
        assert_eq!(
            err,
            Response::Error {
                code: "TooManyPages".into(),
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
}
