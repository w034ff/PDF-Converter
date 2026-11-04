//! Item 6: the OCR result as the app would keep it, page → block → line →
//! word → character, and how it is built from the engine's lines.
//!
//! Coordinates are **points on the page as shown** (origin top-left, y down),
//! the same space as `RectPt` in `crates/worker/src/protocol.rs`:
//! - the rendering resolution used for OCR is an internal detail (it changes
//!   with the detector's 1600 px limit and the page size); points do not;
//! - a transparent text layer in a PDF is written in points;
//! - the screen already converts crop candidates from points, so selection and
//!   search highlights use the same code whether the screen stays Tauri or
//!   moves to Slint.
//!
//! Positions are quads, not rectangles: a skewed scan gives slanted lines, and
//! a text layer that follows the slant selects better. `bbox` is kept beside
//! each quad for hit tests.

use serde::Serialize;

use crate::engine::Line;
use crate::geom::Quad;
use crate::order;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct RectPt {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// Corners in reading order of the text: for horizontal text top-left,
/// top-right, bottom-right, bottom-left; for vertical text top-right,
/// bottom-right, bottom-left, top-left (the start of the column first).
pub type QuadPt = [[f64; 2]; 4];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrPage {
    /// Page size in points as shown.
    pub width_pt: f64,
    pub height_pt: f64,
    /// Which models produced it, so that a stored result can be redone after an update.
    pub engine: String,
    /// Blocks in reading order.
    pub blocks: Vec<OcrBlock>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrBlock {
    pub bbox: RectPt,
    pub orientation: Orientation,
    pub lines: Vec<OcrLine>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub quad: QuadPt,
    pub bbox: RectPt,
    pub orientation: Orientation,
    pub text: String,
    /// Mean probability of the line's characters (0–1).
    pub confidence: f32,
    /// Runs of characters between spaces. Japanese without spaces is one word.
    pub words: Vec<OcrWord>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrWord {
    pub quad: QuadPt,
    pub text: String,
    pub confidence: f32,
    pub chars: Vec<OcrChar>,
}

/// One character with its estimated cell (see [`char_cells`]).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrChar {
    pub quad: QuadPt,
    pub text: String,
    pub confidence: f32,
}

/// Point of the crop (`u` along the text, `v` across it, both 0–1) on the page.
fn crop_to_page(q: &Quad, vertical: bool, u: f64, v: f64) -> [f64; 2] {
    // The crop is the quad unrolled: tl, tr, br, bl. A vertical crop was then
    // turned 90° counter-clockwise, so its x runs down the original left edge
    // and its y runs from the original right edge to the left one.
    let lerp = |a: [f64; 2], b: [f64; 2], t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    if vertical {
        let right = lerp(q[1], q[2], u);
        let left = lerp(q[0], q[3], u);
        lerp(right, left, v)
    } else {
        let top = lerp(q[0], q[1], u);
        let bottom = lerp(q[3], q[2], u);
        lerp(top, bottom, v)
    }
}

/// Each character's span along the line, as fractions (0–1) of the crop's
/// length, from the CTC time steps where it was emitted.
///
/// One time step covers 8 columns of the 48-px-high resized crop. A character
/// usually fires on one or two steps near its centre, so the cell boundaries
/// are put halfway between neighbouring characters' steps; the first and last
/// cells extend outwards by half the median character pitch.
pub fn char_cells(line: &Line) -> Vec<(f64, f64)> {
    let cols = &line.rec.char_cols;
    if cols.is_empty() {
        return Vec::new();
    }
    let resized_w = line.crop_px.0 as f64;
    let step_px = 8.0;
    let centre = |k: usize| ((cols[k].1 + cols[k].2) as f64 / 2.0 + 0.5) * step_px;
    let n = cols.len();
    let mut pitches: Vec<f64> = (1..n).map(|k| centre(k) - centre(k - 1)).collect();
    pitches.sort_by(f64::total_cmp);
    let pitch = if pitches.is_empty() { resized_w } else { pitches[pitches.len() / 2] };
    (0..n)
        .map(|k| {
            let a = if k == 0 { centre(0) - pitch / 2.0 } else { (centre(k - 1) + centre(k)) / 2.0 };
            let b = if k + 1 == n { centre(k) + pitch / 2.0 } else { (centre(k) + centre(k + 1)) / 2.0 };
            ((a / resized_w).clamp(0.0, 1.0), (b / resized_w).clamp(0.0, 1.0))
        })
        .collect()
}

fn bbox_of(q: &QuadPt) -> RectPt {
    let xs = q.iter().map(|p| p[0]);
    let ys = q.iter().map(|p| p[1]);
    RectPt {
        x0: xs.clone().fold(f64::MAX, f64::min),
        y0: ys.clone().fold(f64::MAX, f64::min),
        x1: xs.fold(f64::MIN, f64::max),
        y1: ys.fold(f64::MIN, f64::max),
    }
}

/// Builds the page from the engine's lines on an image `img_w` × `img_h`
/// pixels rendered from a page of `page_pt` points.
pub fn build_page(lines: &[Line], img_w: usize, img_h: usize, page_pt: (f64, f64), engine: &str) -> OcrPage {
    let (sx, sy) = (page_pt.0 / img_w as f64, page_pt.1 / img_h as f64);
    let to_pt = |p: [f64; 2]| [p[0] * sx, p[1] * sy];
    let items: Vec<order::Item> = lines.iter().map(|l| order::Item { bbox: crate::geom::quad_bounds(&l.quad) }).collect();
    let blocks = order::reading_order(&items);
    let blocks = blocks
        .into_iter()
        .map(|b| {
            let lines: Vec<OcrLine> = b
                .lines
                .iter()
                .map(|&i| {
                    let l = &lines[i];
                    let vertical = l.vertical;
                    let span = |u0: f64, u1: f64| -> QuadPt {
                        [
                            to_pt(crop_to_page(&l.quad, vertical, u0, 0.0)),
                            to_pt(crop_to_page(&l.quad, vertical, u1, 0.0)),
                            to_pt(crop_to_page(&l.quad, vertical, u1, 1.0)),
                            to_pt(crop_to_page(&l.quad, vertical, u0, 1.0)),
                        ]
                    };
                    let cells = char_cells(l);
                    let chars: Vec<(usize, OcrChar)> = l
                        .rec
                        .char_cols
                        .iter()
                        .zip(&cells)
                        .enumerate()
                        .map(|(k, (c, &(u0, u1)))| (k, OcrChar { quad: span(u0, u1), text: c.0.to_string(), confidence: c.3 }))
                        .collect();
                    let mut words = Vec::new();
                    let mut cur: Vec<(usize, OcrChar)> = Vec::new();
                    let flush = |cur: &mut Vec<(usize, OcrChar)>, words: &mut Vec<OcrWord>| {
                        if cur.is_empty() {
                            return;
                        }
                        let (first, last) = (cur[0].0, cur[cur.len() - 1].0);
                        let text: String = cur.iter().map(|c| c.1.text.as_str()).collect();
                        let conf = cur.iter().map(|c| c.1.confidence).sum::<f32>() / cur.len() as f32;
                        words.push(OcrWord {
                            quad: span(cells[first].0, cells[last].1),
                            text,
                            confidence: conf,
                            chars: cur.drain(..).map(|c| c.1).collect(),
                        });
                    };
                    for c in chars {
                        if c.1.text.trim().is_empty() {
                            flush(&mut cur, &mut words);
                        } else {
                            cur.push(c);
                        }
                    }
                    flush(&mut cur, &mut words);
                    let quad = span(0.0, 1.0);
                    OcrLine {
                        bbox: bbox_of(&quad),
                        quad,
                        orientation: if vertical { Orientation::Vertical } else { Orientation::Horizontal },
                        text: l.text.clone(),
                        confidence: l.score,
                        words,
                    }
                })
                .collect();
            OcrBlock {
                bbox: RectPt { x0: b.bbox[0] * sx, y0: b.bbox[1] * sy, x1: b.bbox[2] * sx, y1: b.bbox[3] * sy },
                orientation: if b.vertical { Orientation::Vertical } else { Orientation::Horizontal },
                lines,
            }
        })
        .collect();
    OcrPage { width_pt: page_pt.0, height_pt: page_pt.1, engine: engine.into(), blocks }
}
