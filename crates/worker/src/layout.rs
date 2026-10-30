//! Layout detection for crop candidates (design §4.7, §5.1): the page is
//! rendered, passed through PP-DocLayout_plus-L on onnxruntime, and the
//! model's boxes are turned into photos, figures, charts and tables.
//!
//! onnxruntime and the model are loaded on the first `Detect` only, so that a
//! worker that never detects does not pay for them (design §5.1).

use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;
use pdfconv_core::{PHOTO_MATCH_IOU, PageRect, iou};
use pdfium_render::prelude::PdfDocument;

use crate::protocol::{
    CandidateKind, EmbeddedImage, LayoutCandidate, RectPt, RenderColor, Response,
};
use crate::server::{
    CODE_PDF_OPEN_FAILED, error_response, load_page, render_page_pixels, size_for_long_side,
};
use crate::{MAX_RENDER_PIXELS, crop};

/// The longer side of the page as rendered for the model, in pixels; the
/// rendering the spike matched against PaddleX with (design §4.7, 前処理).
pub const LAYOUT_RENDER_LONG_SIDE: u32 = 1600;

/// The model's fixed input size; the rendering is squeezed to this square
/// without keeping its aspect ratio (design §4.7, 前処理).
pub const LAYOUT_INPUT_SIDE: u32 = 800;

/// Boxes below this score are dropped. The int8 model agrees best with the
/// original model at its 0.5 threshold at this value (design §4.7, 後処理).
pub const LAYOUT_SCORE_MIN: f32 = 0.48;

/// Of two boxes of the same class overlapping by at least this IoU, the
/// lower-scoring one is dropped, as PaddleX's `layout_nms` does (design §4.7,
/// 後処理).
pub const LAYOUT_NMS_SAME_CLASS_IOU: f64 = 0.6;

/// Of two boxes of different classes overlapping by at least this IoU, the
/// lower-scoring one is dropped, as PaddleX's `layout_nms` does (design §4.7,
/// 後処理).
pub const LAYOUT_NMS_ANY_CLASS_IOU: f64 = 0.98;

/// Candidates whose shorter side is below this many points are dropped: in
/// the survey they were almost all icons, numbers and logos (design §4.7,
/// 後処理).
pub const LAYOUT_MIN_SIDE_PT: f64 = 24.0;

/// The most threads one inference uses; the session also leaves one logical
/// CPU free (design §4.7, セッション).
pub const LAYOUT_THREADS: usize = 6;

/// The model's classes in the order of its outputs (`label_list` of the
/// model's `inference.yml`). Only `image`, `table` and `chart` become
/// candidates; the others still take part in removing overlaps.
const LAYOUT_LABELS: [&str; 20] = [
    "paragraph_title",
    "image",
    "text",
    "number",
    "abstract",
    "content",
    "figure_title",
    "formula",
    "table",
    "reference",
    "doc_title",
    "footnote",
    "header",
    "algorithm",
    "footer",
    "seal",
    "chart",
    "formula_number",
    "aside_text",
    "reference_content",
];

/// Values per row of the model's first output: class, score, then the box's
/// corners `x0, y0, x1, y1`. Rows may carry more values after these.
const ROW_MIN_VALUES: usize = 6;

/// Error code when onnxruntime or the model cannot be loaded or run (design §6.6).
const CODE_LAYOUT_UNAVAILABLE: &str = "LayoutUnavailable";

/// One box of the model's output, in pixels of the rendered page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ModelBox {
    /// Index into [`LAYOUT_LABELS`].
    pub class: usize,
    pub score: f32,
    pub rect: PageRect,
}

/// Runs layout detection for one worker, loading onnxruntime and the model
/// on first use.
pub(crate) struct Detector {
    runtime: PathBuf,
    model: PathBuf,
    session: SessionState,
}

enum SessionState {
    NotLoaded,
    Loaded(Box<Session>),
    /// Loading failed once; every later `Detect` gets the same error rather
    /// than trying again (design §5.1). Trying again would also end the
    /// worker: a second `ort::init_from` after one that failed to load the
    /// library brings the process down.
    Unavailable(String),
}

impl Detector {
    /// `runtime` is the onnxruntime library file and `model` the model file;
    /// neither is touched until [`Detector::detect`].
    pub(crate) fn new(runtime: &Path, model: &Path) -> Self {
        Self {
            runtime: runtime.to_path_buf(),
            model: model.to_path_buf(),
            session: SessionState::NotLoaded,
        }
    }

    /// The candidates of page `page` (1-based) of `doc`, highest score first.
    ///
    /// # Errors
    ///
    /// `InvalidParams` when there is no such page, checked before anything is
    /// loaded; `LayoutUnavailable` when onnxruntime or the model cannot be
    /// loaded or run.
    pub(crate) fn detect(
        &mut self,
        doc: &PdfDocument<'_>,
        page: u32,
    ) -> Result<Vec<LayoutCandidate>, Response> {
        let pdf_page = load_page(doc, page)?;
        let session = self.session()?;

        let page_pt = (
            f64::from(pdf_page.width().value),
            f64::from(pdf_page.height().value),
        );
        let (width, height) = size_for_long_side(page_pt.0, page_pt.1, LAYOUT_RENDER_LONG_SIDE);
        let (pixels, _) = render_page_pixels(&pdf_page, width, height, RenderColor::Color)?;
        let input = model_input(pixels, width, height)?;
        let boxes = run_model(session, input, width, height)?;

        // Only an image box looks for an embedded image, so a page without
        // one keeps its charts and tables even where listing images fails.
        let images = if has_image_box(&boxes) {
            crop::page_images(doc, page, MAX_RENDER_PIXELS)?
        } else {
            Vec::new()
        };
        Ok(candidates(&boxes, (width, height), page_pt, &images))
    }

    fn session(&mut self) -> Result<&mut Session, Response> {
        if matches!(self.session, SessionState::NotLoaded) {
            self.session = match load_session(&self.runtime, &self.model) {
                Ok(session) => SessionState::Loaded(Box::new(session)),
                Err(detail) => SessionState::Unavailable(detail),
            };
        }
        match &mut self.session {
            SessionState::Loaded(session) => Ok(session),
            SessionState::Unavailable(detail) => Err(unavailable(detail.clone())),
            SessionState::NotLoaded => Err(unavailable("the session was not stored".into())),
        }
    }
}

fn unavailable(detail: String) -> Response {
    error_response(CODE_LAYOUT_UNAVAILABLE, Some(detail))
}

/// Loads onnxruntime from its full path, which keeps another copy on the
/// system's search path from being used (design §5.4), then the model.
fn load_session(runtime: &Path, model: &Path) -> Result<Session, String> {
    let environment = ort::init_from(runtime)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    let logical_cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    let build = || -> ort::Result<Session> {
        Session::builder(&environment)?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(intra_threads(logical_cpus))?
            .with_inter_threads(1)?
            // Idle threads that spin slow inference down (design §4.7, セッション).
            .with_intra_op_spinning(false)?
            .commit_from_file(model)
    };
    build().map_err(|e| e.to_string())
}

/// Threads for one inference on a machine with `logical_cpus` logical CPUs.
fn intra_threads(logical_cpus: usize) -> usize {
    LAYOUT_THREADS.min(logical_cpus.saturating_sub(1)).max(1)
}

/// The model's `image` input from the rendered page's RGB `pixels`: squeezed
/// to the input square, scaled to 0–1 and laid out channel by channel.
fn model_input(pixels: Vec<u8>, width: u32, height: u32) -> Result<Vec<f32>, Response> {
    let rendered = image::RgbImage::from_raw(width, height, pixels).ok_or_else(|| {
        error_response(
            CODE_PDF_OPEN_FAILED,
            Some(format!("the rendering is not {width}×{height} RGB pixels")),
        )
    })?;
    let squeezed = image::imageops::resize(
        &rendered,
        LAYOUT_INPUT_SIDE,
        LAYOUT_INPUT_SIDE,
        FilterType::CatmullRom,
    );
    let plane = (LAYOUT_INPUT_SIDE * LAYOUT_INPUT_SIDE) as usize;
    let mut input = vec![0.0_f32; 3 * plane];
    for (index, pixel) in squeezed.pixels().enumerate() {
        for (channel, value) in pixel.0.iter().enumerate() {
            input[channel * plane + index] = f32::from(*value) / 255.0;
        }
    }
    Ok(input)
}

/// Runs the model on `input` for a page rendered at `width` × `height`.
fn run_model(
    session: &mut Session,
    input: Vec<f32>,
    width: u32,
    height: u32,
) -> Result<Vec<ModelBox>, Response> {
    let side = LAYOUT_INPUT_SIDE as f32;
    let tensors = || -> ort::Result<_> {
        let side_i = i64::from(LAYOUT_INPUT_SIDE);
        Ok((
            Tensor::from_array(([1_i64, 3, side_i, side_i], input))?,
            Tensor::from_array(([1_i64, 2], vec![side, side]))?,
            // Height first, as PaddleX orders it.
            Tensor::from_array(([1_i64, 2], vec![side / height as f32, side / width as f32]))?,
        ))
    };
    let (image, im_shape, scale_factor) = tensors().map_err(|e| unavailable(e.to_string()))?;
    let outputs = session
        .run(ort::inputs![
            "image" => image,
            "im_shape" => im_shape,
            "scale_factor" => scale_factor,
        ])
        .map_err(|e| unavailable(e.to_string()))?;
    let boxes = outputs
        .values()
        .next()
        .ok_or_else(|| unavailable("the model has no output".into()))?;
    let (shape, values) = boxes
        .try_extract_tensor::<f32>()
        .map_err(|e| unavailable(e.to_string()))?;
    let columns = match shape.get(1).map(|&c| usize::try_from(c)) {
        Some(Ok(columns)) if columns >= ROW_MIN_VALUES => columns,
        _ => return Err(unavailable(format!("unexpected output shape {shape:?}"))),
    };
    Ok(model_boxes(values, columns))
}

/// The boxes in the model's output rows of `columns` values each, leaving out
/// the rows with a negative class (padding).
fn model_boxes(values: &[f32], columns: usize) -> Vec<ModelBox> {
    values
        .chunks_exact(columns)
        .filter(|row| row[0] >= 0.0)
        .map(|row| ModelBox {
            class: row[0] as usize,
            score: row[1],
            rect: PageRect {
                x0: f64::from(row[2]),
                y0: f64::from(row[3]),
                x1: f64::from(row[4]),
                y1: f64::from(row[5]),
            },
        })
        .collect()
}

/// Turns the model's `boxes` on a page rendered at `rendered` pixels into
/// candidates (design §4.7, 後処理). `page_pt` is the page's shown width and
/// height in points, and `images` the page's embedded images as `PageImages`
/// lists them.
///
/// The steps run in the design's order: drop low scores, clip to the
/// rendering, remove overlaps among all classes, keep pictures, tables and
/// charts, convert to points, drop small boxes, and match photos. The result
/// keeps the order of the overlap removal, which is highest score first.
pub(crate) fn candidates(
    boxes: &[ModelBox],
    rendered: (u32, u32),
    page_pt: (f64, f64),
    images: &[EmbeddedImage],
) -> Vec<LayoutCandidate> {
    let (width_px, height_px) = (f64::from(rendered.0), f64::from(rendered.1));
    let confident: Vec<ModelBox> = boxes
        .iter()
        .filter(|b| b.score >= LAYOUT_SCORE_MIN)
        .map(|b| ModelBox {
            rect: PageRect {
                x0: b.rect.x0.clamp(0.0, width_px),
                y0: b.rect.y0.clamp(0.0, height_px),
                x1: b.rect.x1.clamp(0.0, width_px),
                y1: b.rect.y1.clamp(0.0, height_px),
            },
            ..*b
        })
        .collect();

    let scale_x = page_pt.0 / width_px;
    let scale_y = page_pt.1 / height_px;
    let pictures = without_overlaps(confident)
        .into_iter()
        .filter_map(|detected| {
            let kind = picture_kind(detected.class)?;
            let rect = PageRect {
                x0: detected.rect.x0 * scale_x,
                y0: detected.rect.y0 * scale_y,
                x1: detected.rect.x1 * scale_x,
                y1: detected.rect.y1 * scale_y,
            };
            (rect.width().min(rect.height()) >= LAYOUT_MIN_SIDE_PT).then_some(Picture {
                kind,
                score: detected.score,
                rect,
            })
        });
    classify(pictures, images)
}

/// Whether any box that [`candidates`] keeps by its score is of the image
/// class, the one class matched with the embedded images.
fn has_image_box(boxes: &[ModelBox]) -> bool {
    boxes.iter().any(|b| {
        b.score >= LAYOUT_SCORE_MIN && matches!(picture_kind(b.class), Some(PictureKind::Image))
    })
}

/// A box of a class that becomes a candidate, in points on the page.
struct Picture {
    kind: PictureKind,
    score: f32,
    rect: PageRect,
}

/// Names each of `pictures`, given highest score first, and keeps their
/// order (design §4.7, 種類). An `image` box that matches an embedded image
/// becomes that photo; when several boxes match one image, only the first,
/// the best scoring, does and the others are left out.
fn classify(
    pictures: impl IntoIterator<Item = Picture>,
    images: &[EmbeddedImage],
) -> Vec<LayoutCandidate> {
    let mut claimed_images = Vec::new();
    let mut out = Vec::new();
    for picture in pictures {
        let candidate = match picture.kind {
            PictureKind::Chart => box_candidate(CandidateKind::Chart, &picture),
            PictureKind::Table => box_candidate(CandidateKind::Table, &picture),
            PictureKind::Image => match matching_image(&picture.rect, images) {
                Some(image) if claimed_images.contains(&image.image_index) => continue,
                Some(image) => {
                    claimed_images.push(image.image_index);
                    LayoutCandidate {
                        kind: CandidateKind::Photo,
                        score: picture.score,
                        rect_pt: image.rect_pt,
                        image_index: Some(image.image_index),
                    }
                }
                None => box_candidate(CandidateKind::Figure, &picture),
            },
        };
        out.push(candidate);
    }
    out
}

/// The classes that become candidates.
enum PictureKind {
    Image,
    Chart,
    Table,
}

fn picture_kind(class: usize) -> Option<PictureKind> {
    match LAYOUT_LABELS.get(class).copied() {
        Some("image") => Some(PictureKind::Image),
        Some("chart") => Some(PictureKind::Chart),
        Some("table") => Some(PictureKind::Table),
        _ => None,
    }
}

/// Walks `boxes` from the highest score, equal scores in their given order,
/// and keeps each box that overlaps no kept box by
/// [`LAYOUT_NMS_SAME_CLASS_IOU`] (same class) or [`LAYOUT_NMS_ANY_CLASS_IOU`]
/// (another class).
fn without_overlaps(mut boxes: Vec<ModelBox>) -> Vec<ModelBox> {
    boxes.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<ModelBox> = Vec::new();
    for candidate in boxes {
        let overlaps = kept.iter().any(|k| {
            let limit = if k.class == candidate.class {
                LAYOUT_NMS_SAME_CLASS_IOU
            } else {
                LAYOUT_NMS_ANY_CLASS_IOU
            };
            iou(&k.rect, &candidate.rect) >= limit
        });
        if !overlaps {
            kept.push(candidate);
        }
    }
    kept
}

/// The embedded image whose visible frame coincides best with `rect`, if it
/// reaches [`PHOTO_MATCH_IOU`]; of equal matches the first listed.
fn matching_image<'a>(rect: &PageRect, images: &'a [EmbeddedImage]) -> Option<&'a EmbeddedImage> {
    let mut best: Option<(&EmbeddedImage, f64)> = None;
    for image in images {
        let overlap = iou(rect, &page_rect(&image.rect_pt));
        if overlap >= PHOTO_MATCH_IOU && best.is_none_or(|(_, b)| overlap > b) {
            best = Some((image, overlap));
        }
    }
    best.map(|(image, _)| image)
}

fn box_candidate(kind: CandidateKind, picture: &Picture) -> LayoutCandidate {
    LayoutCandidate {
        kind,
        score: picture.score,
        rect_pt: rect_pt(&picture.rect),
        image_index: None,
    }
}

fn page_rect(rect: &RectPt) -> PageRect {
    PageRect {
        x0: rect.x0,
        y0: rect.y0,
        x1: rect.x1,
        y1: rect.y1,
    }
}

fn rect_pt(rect: &PageRect) -> RectPt {
    RectPt {
        x0: rect.x0,
        y0: rect.y0,
        x1: rect.x1,
        y1: rect.y1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: usize = 2;
    const IMAGE: usize = 1;
    const TABLE: usize = 8;
    const CHART: usize = 16;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> PageRect {
        PageRect { x0, y0, x1, y1 }
    }

    fn model_box(class: usize, score: f32, rect: PageRect) -> ModelBox {
        ModelBox { class, score, rect }
    }

    /// Candidates on a page whose points are its rendered pixels.
    fn one_to_one(boxes: &[ModelBox], images: &[EmbeddedImage]) -> Vec<LayoutCandidate> {
        candidates(boxes, (1000, 1000), (1000.0, 1000.0), images)
    }

    fn image(image_index: u32, rect: PageRect) -> EmbeddedImage {
        EmbeddedImage {
            image_index,
            rect_pt: rect_pt(&rect),
            px_width: 10,
            px_height: 10,
            native_size: None,
        }
    }

    fn kinds_and_scores(candidates: &[LayoutCandidate]) -> Vec<(CandidateKind, f32)> {
        candidates.iter().map(|c| (c.kind, c.score)).collect()
    }

    #[test]
    fn the_labels_put_image_table_and_chart_where_the_model_does() {
        assert_eq!(LAYOUT_LABELS[IMAGE], "image");
        assert_eq!(LAYOUT_LABELS[TEXT], "text");
        assert_eq!(LAYOUT_LABELS[TABLE], "table");
        assert_eq!(LAYOUT_LABELS[CHART], "chart");
    }

    #[test]
    fn rows_with_a_negative_class_are_left_out_and_extra_values_ignored() {
        let values = [
            16.0, 0.9, 1.0, 2.0, 3.0, 4.0, 7.0, //
            -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, //
            1.0, 0.5, 5.0, 6.0, 7.0, 8.0, 9.0,
        ];
        assert_eq!(
            model_boxes(&values, 7),
            vec![
                model_box(CHART, 0.9, rect(1.0, 2.0, 3.0, 4.0)),
                model_box(IMAGE, 0.5, rect(5.0, 6.0, 7.0, 8.0)),
            ]
        );
    }

    #[test]
    fn a_score_of_exactly_the_minimum_is_kept() {
        let boxes = [
            model_box(CHART, LAYOUT_SCORE_MIN, rect(0.0, 0.0, 100.0, 100.0)),
            model_box(TABLE, 0.479, rect(200.0, 0.0, 300.0, 100.0)),
        ];
        assert_eq!(
            kinds_and_scores(&one_to_one(&boxes, &[])),
            vec![(CandidateKind::Chart, LAYOUT_SCORE_MIN)]
        );
    }

    #[test]
    fn boxes_are_clipped_to_the_rendering() {
        let boxes = [model_box(CHART, 0.9, rect(-20.0, -10.0, 1200.0, 1100.0))];
        let found = candidates(&boxes, (1000, 800), (500.0, 400.0), &[]);
        assert_eq!(
            found[0].rect_pt,
            RectPt {
                x0: 0.0,
                y0: 0.0,
                x1: 500.0,
                y1: 400.0
            }
        );
    }

    #[test]
    fn a_box_of_the_same_class_overlapping_by_the_limit_is_removed() {
        let first = model_box(CHART, 0.9, rect(0.0, 0.0, 100.0, 100.0));
        // IoU 6000 / 10000 = 0.6 exactly.
        let at_limit = model_box(CHART, 0.8, rect(0.0, 0.0, 100.0, 60.0));
        assert_eq!(one_to_one(&[first, at_limit], &[]).len(), 1);
        // IoU 0.59.
        let below = model_box(CHART, 0.8, rect(0.0, 0.0, 100.0, 59.0));
        assert_eq!(one_to_one(&[first, below], &[]).len(), 2);
    }

    #[test]
    fn a_box_of_another_class_overlapping_by_the_limit_is_removed() {
        let first = model_box(TABLE, 0.9, rect(0.0, 0.0, 100.0, 100.0));
        // IoU 9800 / 10000 = 0.98 exactly.
        let at_limit = model_box(CHART, 0.8, rect(0.0, 0.0, 100.0, 98.0));
        assert_eq!(
            kinds_and_scores(&one_to_one(&[first, at_limit], &[])),
            vec![(CandidateKind::Table, 0.9)]
        );
        // IoU 0.97.
        let below = model_box(CHART, 0.8, rect(0.0, 0.0, 100.0, 97.0));
        assert_eq!(
            kinds_and_scores(&one_to_one(&[first, below], &[])),
            vec![(CandidateKind::Table, 0.9), (CandidateKind::Chart, 0.8)]
        );
    }

    #[test]
    fn a_text_box_removes_an_image_box_it_covers_before_classes_are_kept() {
        let boxes = [
            model_box(IMAGE, 0.8, rect(0.0, 0.0, 100.0, 99.0)),
            model_box(TEXT, 0.9, rect(0.0, 0.0, 100.0, 100.0)),
        ];
        assert_eq!(one_to_one(&boxes, &[]), vec![]);
    }

    #[test]
    fn a_lower_score_box_does_not_remove_a_higher_one() {
        // Given lowest first: the overlap removal must still start from the top.
        let boxes = [
            model_box(CHART, 0.5, rect(0.0, 0.0, 100.0, 100.0)),
            model_box(CHART, 0.9, rect(0.0, 0.0, 100.0, 90.0)),
        ];
        assert_eq!(
            kinds_and_scores(&one_to_one(&boxes, &[])),
            vec![(CandidateKind::Chart, 0.9)]
        );
    }

    #[test]
    fn x_and_y_are_scaled_separately_on_a_portrait_and_a_landscape_page() {
        let boxes = [model_box(CHART, 0.9, rect(100.0, 200.0, 500.0, 1000.0))];
        let portrait = candidates(&boxes, (1000, 1600), (600.0, 800.0), &[]);
        assert_eq!(
            portrait[0].rect_pt,
            RectPt {
                x0: 60.0,
                y0: 100.0,
                x1: 300.0,
                y1: 500.0
            }
        );
        let landscape = candidates(&boxes, (1600, 1000), (800.0, 600.0), &[]);
        assert_eq!(
            landscape[0].rect_pt,
            RectPt {
                x0: 50.0,
                y0: 120.0,
                x1: 250.0,
                y1: 600.0
            }
        );
    }

    #[test]
    fn the_smallest_side_is_compared_in_points() {
        // Two pixels per point: 48 px is 24 pt, 47 px is 23.5 pt.
        let at_limit = model_box(CHART, 0.9, rect(0.0, 0.0, 48.0, 400.0));
        let below = model_box(TABLE, 0.8, rect(600.0, 0.0, 647.0, 400.0));
        let short = model_box(TABLE, 0.7, rect(0.0, 1000.0, 400.0, 1047.0));
        assert_eq!(
            kinds_and_scores(&candidates(
                &[at_limit, below, short],
                (1200, 1600),
                (600.0, 800.0),
                &[]
            )),
            vec![(CandidateKind::Chart, 0.9)]
        );
    }

    #[test]
    fn an_image_box_matching_an_embedded_image_by_the_limit_is_that_photo() {
        let frame = rect(100.0, 100.0, 200.0, 200.0);
        let images = [image(4, frame)];
        // IoU 8500 / 10000 = 0.85 exactly.
        let at_limit = [model_box(IMAGE, 0.7, rect(100.0, 100.0, 200.0, 185.0))];
        assert_eq!(
            one_to_one(&at_limit, &images),
            vec![LayoutCandidate {
                kind: CandidateKind::Photo,
                score: 0.7,
                rect_pt: rect_pt(&frame),
                image_index: Some(4),
            }]
        );
        // IoU 0.84.
        let below_rect = rect(100.0, 100.0, 200.0, 184.0);
        let below = [model_box(IMAGE, 0.7, below_rect)];
        assert_eq!(
            one_to_one(&below, &images),
            vec![LayoutCandidate {
                kind: CandidateKind::Figure,
                score: 0.7,
                rect_pt: rect_pt(&below_rect),
                image_index: None,
            }]
        );
    }

    #[test]
    fn an_image_box_takes_the_embedded_image_it_matches_best() {
        let images = [
            image(1, rect(0.0, 0.0, 100.0, 90.0)),
            image(2, rect(0.0, 0.0, 100.0, 100.0)),
            image(3, rect(0.0, 0.0, 100.0, 95.0)),
        ];
        let boxes = [model_box(IMAGE, 0.7, rect(0.0, 0.0, 100.0, 99.0))];
        assert_eq!(one_to_one(&boxes, &images)[0].image_index, Some(2));
    }

    #[test]
    fn of_two_boxes_on_one_image_only_the_first_becomes_the_photo() {
        let frame = rect(0.0, 0.0, 100.0, 100.0);
        let picture = |score: f32, rect: PageRect| Picture {
            kind: PictureKind::Image,
            score,
            rect,
        };
        let found = classify(
            [
                picture(0.9, rect(0.0, 0.0, 100.0, 95.0)),
                picture(0.8, rect(0.0, 5.0, 100.0, 100.0)),
                picture(0.7, rect(300.0, 0.0, 400.0, 100.0)),
            ],
            &[image(0, frame)],
        );
        assert_eq!(
            kinds_and_scores(&found),
            vec![(CandidateKind::Photo, 0.9), (CandidateKind::Figure, 0.7)]
        );
    }

    #[test]
    fn candidates_come_highest_score_first_and_ties_in_the_given_order() {
        let images = [image(0, rect(600.0, 0.0, 700.0, 100.0))];
        let boxes = [
            model_box(CHART, 0.5, rect(0.0, 0.0, 100.0, 100.0)),
            model_box(TABLE, 0.9, rect(200.0, 0.0, 300.0, 100.0)),
            model_box(IMAGE, 0.7, rect(400.0, 0.0, 500.0, 100.0)),
            model_box(CHART, 0.7, rect(0.0, 200.0, 100.0, 300.0)),
            model_box(IMAGE, 0.8, rect(600.0, 0.0, 700.0, 100.0)),
        ];
        assert_eq!(
            kinds_and_scores(&one_to_one(&boxes, &images)),
            vec![
                (CandidateKind::Table, 0.9),
                (CandidateKind::Photo, 0.8),
                (CandidateKind::Figure, 0.7),
                (CandidateKind::Chart, 0.7),
                (CandidateKind::Chart, 0.5),
            ]
        );
    }

    #[test]
    fn only_a_confident_image_box_asks_for_the_embedded_images() {
        let area = rect(0.0, 0.0, 100.0, 100.0);
        assert!(!has_image_box(&[
            model_box(CHART, 0.9, area),
            model_box(TABLE, 0.9, area),
        ]));
        assert!(!has_image_box(&[model_box(
            IMAGE,
            LAYOUT_SCORE_MIN - 0.01,
            area
        )]));
        assert!(has_image_box(&[
            model_box(CHART, 0.9, area),
            model_box(IMAGE, LAYOUT_SCORE_MIN, area),
        ]));
    }

    #[test]
    fn inference_leaves_one_logical_cpu_free_and_uses_at_least_one_thread() {
        assert_eq!(intra_threads(0), 1);
        assert_eq!(intra_threads(1), 1);
        assert_eq!(intra_threads(2), 1);
        assert_eq!(intra_threads(4), 3);
        assert_eq!(intra_threads(LAYOUT_THREADS + 1), LAYOUT_THREADS);
        assert_eq!(intra_threads(64), LAYOUT_THREADS);
    }
}
