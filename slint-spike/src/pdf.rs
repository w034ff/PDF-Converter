//! Talks to `crates/worker` the way `src-tauri` does: the spike starts its
//! own executable with `WORKER_FLAG` and sends requests over the pipe.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pdfconv_worker::client::{DETECT_TIMEOUT, OPEN_TIMEOUT, RENDER_TIMEOUT, WorkerError, WorkerProcess};
use pdfconv_worker::protocol::{CandidateKind, PageDimensions, RectPt, RenderColor, RenderFormat, Request, Response};

use crate::edit::{Kind, Rect};

/// Where the libraries the worker loads are. `npm run pdfium:fetch` and
/// `npm run layout:fetch` put them under `src-tauri/`; the environment
/// variables override that.
pub struct Libraries {
    pub pdfium_dir: PathBuf,
    pub onnxruntime: PathBuf,
    pub model: PathBuf,
}

impl Libraries {
    pub fn locate() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src-tauri");
        let (os, pdfium_sub, ort) = if cfg!(windows) {
            ("windows", "bin", "onnxruntime.dll")
        } else {
            ("linux", "lib", "libonnxruntime.so")
        };
        let env = |name: &str, default: PathBuf| std::env::var_os(name).map(PathBuf::from).unwrap_or(default);
        Libraries {
            pdfium_dir: env("SPIKE_PDFIUM_DIR", root.join("pdfium").join(os).join(pdfium_sub)),
            onnxruntime: env("SPIKE_ONNXRUNTIME", root.join("onnxruntime").join(os).join(ort)),
            model: env("SPIKE_LAYOUT_MODEL", root.join("layout-model").join("PP-DocLayout_plus-L-int8.onnx")),
        }
    }
}

/// Runs the worker loop if this process was started as a worker.
pub fn run_worker_if_asked() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next().is_none_or(|flag| flag != pdfconv_worker::WORKER_FLAG) {
        return None;
    }
    let mut paths = args.map(PathBuf::from);
    let (Some(lib), Some(ort), Some(model)) = (paths.next(), paths.next(), paths.next()) else {
        return Some(2);
    };
    Some(match pdfconv_worker::server::run(&lib, &ort, &model) {
        Ok(()) => 0,
        Err(_) => 1,
    })
}

pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub struct Candidate {
    pub rect: Rect,
    pub kind: Kind,
    pub score: f32,
}

pub struct Pdf {
    worker: WorkerProcess,
    pub pages: Vec<PageDimensions>,
}

fn rect(r: RectPt) -> Rect {
    Rect::from_points(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32)
}

pub fn err(e: WorkerError) -> String {
    format!("{e:?}")
}

impl Pdf {
    pub fn open(path: &Path) -> Result<Self, String> {
        let libs = Libraries::locate();
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let args: [OsString; 4] = [
            pdfconv_worker::WORKER_FLAG.into(),
            libs.pdfium_dir.into(),
            libs.onnxruntime.into(),
            libs.model.into(),
        ];
        let mut worker = WorkerProcess::spawn(exe.as_os_str(), args).map_err(err)?;
        let open = Request::Open { path: path.to_path_buf(), password: None };
        match worker.request(&open, OPEN_TIMEOUT).map_err(err)? {
            (Response::Open { pages, .. }, _) => Ok(Pdf { worker, pages }),
            (other, _) => Err(format!("unexpected answer {other:?}")),
        }
    }

    /// Renders page `page` (1-based) at `dpi` and decodes it. Returns the image
    /// and the milliseconds the worker took and the decode took.
    pub fn render(&mut self, page: u32, dpi: u32) -> Result<(Rgba, u128, u128), String> {
        let t = Instant::now();
        let req = Request::Render { page, dpi, format: RenderFormat::Png, color: RenderColor::Color };
        let (_, png) = self.worker.request(&req, RENDER_TIMEOUT).map_err(err)?;
        let render_ms = t.elapsed().as_millis();
        let t = Instant::now();
        let img = decode_png(&png)?;
        Ok((img, render_ms, t.elapsed().as_millis()))
    }

    pub fn thumbnail(&mut self, page: u32, max_side: u32) -> Result<Rgba, String> {
        let (_, png) = self.worker.request(&Request::Thumbnail { page, max_side }, RENDER_TIMEOUT).map_err(err)?;
        decode_png(&png)
    }

    /// The photo ranges and the layout candidates of `page`.
    pub fn candidates(&mut self, page: u32) -> Result<Vec<Candidate>, String> {
        let mut out = Vec::new();
        if let (Response::PageImages { images }, _) =
            self.worker.request(&Request::PageImages { page }, RENDER_TIMEOUT).map_err(err)?
        {
            out.extend(images.iter().map(|i| Candidate { rect: rect(i.rect_pt), kind: Kind::Photo, score: 1.0 }));
        }
        // Detection needs onnxruntime and the model; without them the photo
        // ranges alone are shown.
        if let Ok((Response::Detect { candidates }, _)) = self.worker.request(&Request::Detect { page }, DETECT_TIMEOUT) {
            for c in candidates {
                let kind = match c.kind {
                    CandidateKind::Photo => continue, // already listed as a photo range
                    CandidateKind::Figure => Kind::Figure,
                    CandidateKind::Chart => Kind::Chart,
                    CandidateKind::Table => Kind::Table,
                };
                out.push(Candidate { rect: rect(c.rect_pt), kind, score: c.score });
            }
        }
        Ok(out)
    }
}

pub fn decode_png(png: &[u8]) -> Result<Rgba, String> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).map_err(|e| e.to_string())?.to_rgba8();
    Ok(Rgba { width: img.width(), height: img.height(), pixels: img.into_raw() })
}

/// Cuts `r` (page points) out of `page`, a rendering of a page `page_w_pt` wide.
pub fn crop(page: &Rgba, page_w_pt: f32, r: Rect) -> Option<Rgba> {
    let s = page.width as f32 / page_w_pt;
    let x0 = ((r.x * s).floor().max(0.0) as u32).min(page.width);
    let y0 = ((r.y * s).floor().max(0.0) as u32).min(page.height);
    let x1 = (((r.x + r.w) * s).ceil() as u32).min(page.width);
    let y1 = (((r.y + r.h) * s).ceil() as u32).min(page.height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y1 {
        let start = ((y * page.width + x0) * 4) as usize;
        pixels.extend_from_slice(&page.pixels[start..start + (w * 4) as usize]);
    }
    Some(Rgba { width: w, height: h, pixels })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_cuts_the_scaled_rectangle() {
        // A 4 × 2 image of a 2 pt × 1 pt page: 2 pixels per point.
        let pixels: Vec<u8> = (0..8u8).flat_map(|i| [i, 0, 0, 255]).collect();
        let page = Rgba { width: 4, height: 2, pixels };
        let out = crop(&page, 2.0, Rect { x: 1.0, y: 0.5, w: 1.0, h: 0.5 }).unwrap();
        assert_eq!((out.width, out.height), (2, 1));
        assert_eq!(out.pixels.iter().step_by(4).copied().collect::<Vec<_>>(), vec![6, 7]);
    }
}
