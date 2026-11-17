//! OCR of one page (design ocr §4, §5): the page is rendered, PP-OCRv6
//! small's detector finds the lines and its recogniser reads them on
//! onnxruntime, and the result is the page's blocks, lines, words and
//! characters in points.
//!
//! The models are loaded on the first `Ocr` only and kept until the worker
//! ends; the onnxruntime environment is shared with layout detection (design
//! ocr §6).

mod bgr;
mod contours;
mod geom;
mod model;
mod order;
mod page;

use std::path::{Path, PathBuf};

use ort::session::Session;
use ort::value::Tensor;
use pdfconv_core::ErrorCode;
use pdfconv_core::ocr::OcrPage;
use pdfium_render::prelude::PdfDocument;

use crate::onnx::{Runtime, load_session};
use crate::protocol::{RenderColor, Response, WorkerCode};
use crate::server::{error_response, load_page, render_page_pixels, size_for_long_side};

use bgr::Bgr;
use model::{REC_HEIGHT, batches, ctc_decode, db_boxes, det_input, rec_fill, rec_width};
use page::{ReadLine, build, crop, place};

/// The longer side of the page as rendered for OCR, in pixels (design ocr §4,
/// 描く). Reading XFUND shrunk to this was better than reading it at 300 dpi;
/// the recogniser shrinks each line to height 48 anyway.
pub const OCR_RENDER_LONG_SIDE: u32 = 1600;

/// A line whose rectangle is at least this many times as tall as wide is
/// vertical, and one at most its inverse horizontal; the lines in between
/// take the page's majority (design ocr §4, 向き). PaddleOCR turns a crop for
/// reading at the same ratio.
pub const OCR_VERTICAL_ASPECT: f64 = 1.5;

/// The most lines the recogniser reads in one run, as PaddleOCR batches them
/// (design ocr §4, 認識).
pub const OCR_REC_BATCH_LINES: usize = 6;

/// The most input columns of one recogniser run: its line count times its
/// widest line at height 48 (design ocr §4, 認識). The output grows with it,
/// 18,710 values per 8 columns, so a page of wide lines does not get one
/// large run. A line wider than this on its own is still read, alone.
pub const OCR_REC_BATCH_MAX_WIDTH_SUM: usize = 9600;

/// The most threads one inference uses, counted as layout detection counts
/// them (design ocr §6).
pub const OCR_THREADS: usize = 6;

/// The models' name in [`OcrPage::engine`].
pub const OCR_ENGINE: &str = "PP-OCRv6_small";

/// The files in the OCR model folder (design ocr §8).
pub const OCR_DET_MODEL_FILE: &str = "PP-OCRv6_small_det.onnx";
pub const OCR_REC_MODEL_FILE: &str = "PP-OCRv6_small_rec.onnx";
pub const OCR_REC_DICT_FILE: &str = "PP-OCRv6_small_rec_dict.txt";

/// Runs OCR for one worker, loading the models on first use.
pub(crate) struct Recognizer {
    model_dir: PathBuf,
    state: State,
}

enum State {
    NotLoaded,
    Loaded(Box<Models>),
    /// Loading failed once; every later `Ocr` gets the same error rather than
    /// trying again (design ocr §5).
    Unavailable(String),
}

struct Models {
    det: Session,
    rec: Session,
    /// The recogniser's classes: the CTC blank, the dictionary, and the space.
    dict: Vec<String>,
}

fn unavailable(detail: String) -> Response {
    error_response(WorkerCode::OCR_UNAVAILABLE, Some(detail))
}

impl Recognizer {
    /// `model_dir` is the OCR model folder; it is not touched until
    /// [`Recognizer::read`].
    pub(crate) fn new(model_dir: &Path) -> Self {
        Self {
            model_dir: model_dir.to_path_buf(),
            state: State::NotLoaded,
        }
    }

    /// The OCR result of page `page` (1-based) of `doc`.
    ///
    /// # Errors
    ///
    /// `InvalidParams` when there is no such page, checked before anything is
    /// loaded; `OcrUnavailable` when onnxruntime or the models cannot be
    /// loaded or run; `PdfOpenFailed` when the page cannot be rendered.
    pub(crate) fn read(
        &mut self,
        runtime: &mut Runtime,
        doc: &PdfDocument<'_>,
        page: u32,
    ) -> Result<OcrPage, Response> {
        let pdf_page = load_page(doc, page)?;
        let models = self.models(runtime)?;
        let page_pt = (
            f64::from(pdf_page.width().value),
            f64::from(pdf_page.height().value),
        );
        let (width, height) = size_for_long_side(page_pt.0, page_pt.1, OCR_RENDER_LONG_SIDE);
        let (pixels, _) = render_page_pixels(&pdf_page, width, height, RenderColor::Color)?;
        drop(pdf_page);
        let (w, h) = (width as usize, height as usize);
        let img = Bgr::from_rgb(&pixels, w, h).ok_or_else(|| {
            error_response(
                ErrorCode::PdfOpenFailed,
                Some(format!("the rendering is not {width}×{height} RGB pixels")),
            )
        })?;
        drop(pixels);
        let lines = models.read_lines(&img).map_err(unavailable)?;
        Ok(build(&lines, (w, h), page_pt, OCR_ENGINE))
    }

    fn models(&mut self, runtime: &mut Runtime) -> Result<&mut Models, Response> {
        if matches!(self.state, State::NotLoaded) {
            self.state = match runtime
                .environment()
                .and_then(|environment| Models::load(environment, &self.model_dir))
            {
                Ok(models) => State::Loaded(Box::new(models)),
                Err(detail) => State::Unavailable(detail),
            };
        }
        match &mut self.state {
            State::Loaded(models) => Ok(models),
            State::Unavailable(detail) => Err(unavailable(detail.clone())),
            State::NotLoaded => Err(unavailable("the models were not stored".into())),
        }
    }
}

impl Models {
    fn load(environment: &ort::environment::Environment, dir: &Path) -> Result<Self, String> {
        let dict_path = dir.join(OCR_REC_DICT_FILE);
        let text = std::fs::read_to_string(&dict_path)
            .map_err(|e| format!("{}: {e}", dict_path.display()))?;
        let mut dict = vec![String::new()];
        dict.extend(text.lines().map(str::to_owned));
        dict.push(" ".to_owned());
        Ok(Self {
            det: load_session(environment, &dir.join(OCR_DET_MODEL_FILE), OCR_THREADS)?,
            rec: load_session(environment, &dir.join(OCR_REC_MODEL_FILE), OCR_THREADS)?,
            dict,
        })
    }

    /// Finds and reads the lines of `img`.
    fn read_lines(&mut self, img: &Bgr) -> Result<Vec<ReadLine>, String> {
        let quads = self.detect(img)?;
        let mut placed = Vec::new();
        let mut crops = Vec::new();
        for line in place(&quads) {
            if let Some(c) = crop(img, &line) {
                placed.push(line);
                crops.push(c);
            }
        }
        let read = self.recognize(&crops)?;
        Ok(placed
            .into_iter()
            .zip(read)
            .map(|(placed, (decoded, resized_w))| ReadLine {
                placed,
                decoded,
                resized_w,
            })
            .collect())
    }

    fn detect(&mut self, img: &Bgr) -> Result<Vec<geom::PxQuad>, String> {
        let (x, rw, rh) = det_input(img, OCR_RENDER_LONG_SIDE as usize);
        let input =
            Tensor::from_array(([1_i64, 3, rh as i64, rw as i64], x)).map_err(|e| e.to_string())?;
        let outputs = self
            .det
            .run(ort::inputs![input])
            .map_err(|e| e.to_string())?;
        let (shape, pred) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        let [_, _, ph, pw] = shape[..] else {
            return Err(format!("unexpected detector output shape {shape:?}"));
        };
        let (ph, pw) = (ph as usize, pw as usize);
        if pred.len() < ph * pw {
            return Err(format!("unexpected detector output shape {shape:?}"));
        }
        Ok(db_boxes(&pred[..ph * pw], pw, ph, img.w, img.h))
    }

    /// Reads `crops` in batches (design ocr §4, 認識): each crop's text and its
    /// width at height 48.
    fn recognize(&mut self, crops: &[Bgr]) -> Result<Vec<(model::Decoded, usize)>, String> {
        let sizes: Vec<(usize, usize)> = crops.iter().map(|c| (c.w, c.h)).collect();
        let mut results: Vec<Option<(model::Decoded, usize)>> = vec![None; crops.len()];
        for batch in batches(&sizes, OCR_REC_BATCH_LINES, OCR_REC_BATCH_MAX_WIDTH_SUM) {
            let tw = batch
                .iter()
                .map(|&i| rec_width(crops[i].w, crops[i].h).1)
                .max()
                .unwrap_or(0);
            let per = 3 * REC_HEIGHT * tw;
            let mut x = vec![0_f32; batch.len() * per];
            let widths: Vec<usize> = batch
                .iter()
                .zip(x.chunks_exact_mut(per.max(1)))
                .map(|(&i, out)| rec_fill(&crops[i], out, tw))
                .collect();
            let input =
                Tensor::from_array(([batch.len() as i64, 3, REC_HEIGHT as i64, tw as i64], x))
                    .map_err(|e| e.to_string())?;
            let outputs = self
                .rec
                .run(ort::inputs![input])
                .map_err(|e| e.to_string())?;
            let (shape, probs) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?;
            let [n, steps, classes] = shape[..] else {
                return Err(format!("unexpected recogniser output shape {shape:?}"));
            };
            let (n, steps, classes) = (n as usize, steps as usize, classes as usize);
            if n != batch.len()
                || steps == 0
                || classes != self.dict.len()
                || probs.len() < n * steps * classes
            {
                return Err(format!(
                    "the recogniser's output {shape:?} does not match {} lines and {} classes",
                    batch.len(),
                    self.dict.len()
                ));
            }
            for ((&i, rows), width) in batch
                .iter()
                .zip(probs.chunks_exact(steps * classes))
                .zip(widths)
            {
                results[i] = Some((ctc_decode(rows, steps, classes, &self.dict), width));
            }
        }
        Ok(results.into_iter().flatten().collect())
    }
}
