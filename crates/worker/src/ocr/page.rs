//! From the detector's boxes to the page's result (design ocr §4): each line's
//! orientation and crop, the place of each character, and the blocks in
//! reading order, in points on the page.

use pdfconv_core::ocr::{
    OcrBlock, OcrChar, OcrLine, OcrPage, OcrWord, Orientation, Quad, quad_bbox,
};

use super::OCR_VERTICAL_ASPECT;
use super::bgr::{Bgr, perspective_transform, warp_perspective_cubic};
use super::geom::{Pt, PxQuad, mini_box};
use super::model::{Decoded, REC_STEP_PX};
use super::order::{Item, reading_order};

/// A detected line before it is read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Placed {
    /// The line's rectangle with its corners top-left, top-right,
    /// bottom-right, bottom-left as the image shows it, in pixels.
    pub upright: PxQuad,
    /// Whether the line is read top to bottom.
    pub vertical: bool,
}

impl Placed {
    /// The corners from the start of the reading direction (design ocr §2):
    /// for vertical text top-right, bottom-right, bottom-left, top-left.
    fn reading_quad(&self) -> PxQuad {
        let [tl, tr, br, bl] = self.upright;
        if self.vertical {
            [tr, br, bl, tl]
        } else {
            [tl, tr, br, bl]
        }
    }
}

fn sub(a: Pt, b: Pt) -> Pt {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: Pt, b: Pt) -> Pt {
    [a[0] + b[0], a[1] + b[1]]
}

fn length(v: Pt) -> f64 {
    v[0].hypot(v[1])
}

/// The rectangle of a detector box with its corners named as the image shows
/// it, and its height divided by its width.
///
/// The rectangle is the minimum-area one of the box's corners truncated to
/// whole pixels, as PaddleOCR's crop takes it. Its width is the side that runs
/// closer to horizontal, whichever corner comes first: naming the corners by
/// their order instead took most columns of a vertical page turned by 2° for
/// horizontal lines (design ocr §4, 向き).
fn upright(quad: &PxQuad) -> (PxQuad, f64) {
    let points: Vec<Pt> = quad.iter().map(|p| [p[0].trunc(), p[1].trunc()]).collect();
    let (r, _) = mini_box(&points);
    let (e1, e2) = (sub(r[1], r[0]), sub(r[3], r[0]));
    let leans_across =
        |e: Pt| e[0].abs() * length(e2).max(1e-12) >= e2[0].abs() * length(e).max(1e-12);
    let (mut ex, mut ey) = if leans_across(e1) { (e1, e2) } else { (e2, e1) };
    if ex[0] < 0.0 || (ex[0] == 0.0 && ex[1] < 0.0) {
        ex = [-ex[0], -ex[1]];
    }
    if ey[1] < 0.0 || (ey[1] == 0.0 && ey[0] < 0.0) {
        ey = [-ey[0], -ey[1]];
    }
    // The corner from which both sides run forwards: the lowest along both.
    let along = |p: &Pt| p[0] * (ex[0] + ey[0]) + p[1] * (ex[1] + ey[1]);
    let start = r
        .iter()
        .copied()
        .min_by(|a, b| along(a).total_cmp(&along(b)))
        .unwrap_or(r[0]);
    let quad = [
        start,
        add(start, ex),
        add(add(start, ex), ey),
        add(start, ey),
    ];
    (quad, length(ey) / length(ex).max(1e-12))
}

/// Names the corners of each detector box and decides its orientation
/// (design ocr §4, 向き): vertical at an aspect of [`OCR_VERTICAL_ASPECT`] or
/// more, horizontal at its inverse or less, and the page's majority in between
/// (vertical only when vertical lines are more).
pub(crate) fn place(quads: &[PxQuad]) -> Vec<Placed> {
    let shapes: Vec<(PxQuad, f64)> = quads.iter().map(upright).collect();
    let decided = |aspect: f64| {
        if aspect >= OCR_VERTICAL_ASPECT {
            Some(true)
        } else if aspect <= 1.0 / OCR_VERTICAL_ASPECT {
            Some(false)
        } else {
            None
        }
    };
    let vertical = shapes.iter().filter(|s| decided(s.1) == Some(true)).count();
    let horizontal = shapes
        .iter()
        .filter(|s| decided(s.1) == Some(false))
        .count();
    shapes
        .into_iter()
        .map(|(upright, aspect)| Placed {
            upright,
            vertical: decided(aspect).unwrap_or(vertical > horizontal),
        })
        .collect()
}

/// The line cut out of `img` for the recogniser (`get_rotate_crop_image`):
/// the rectangle unrolled, then turned 90° counter-clockwise for a vertical
/// line so that it reads left to right. `None` for a box under one pixel.
pub(crate) fn crop(img: &Bgr, line: &Placed) -> Option<Bgr> {
    let [tl, tr, _, bl] = line.upright;
    let (w, h) = (length(sub(tr, tl)) as usize, length(sub(bl, tl)) as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let (wf, hf) = (w as f64, h as f64);
    let target = [[0.0, 0.0], [wf, 0.0], [wf, hf], [0.0, hf]];
    let inverse = perspective_transform(&target, &line.upright);
    let crop = warp_perspective_cubic(img, &inverse, w, h);
    Some(if line.vertical { crop.rot90() } else { crop })
}

/// A line after it was read.
pub(crate) struct ReadLine {
    pub placed: Placed,
    pub decoded: Decoded,
    /// The crop's width at height 48, which the CTC time steps divide.
    pub resized_w: usize,
}

/// Each character's span along the line as parts (0 to 1) of the resized
/// crop (design ocr §4, 文字の位置). A character is centred on the time steps
/// where it was emitted; neighbouring characters meet halfway, and the first
/// and the last reach out by half the median distance between characters.
fn char_spans(line: &ReadLine) -> Vec<(f64, f64)> {
    let chars = &line.decoded.chars;
    let width = line.resized_w.max(1) as f64;
    let step = REC_STEP_PX as f64;
    let centre = |k: usize| ((chars[k].first_step + chars[k].last_step) as f64 / 2.0 + 0.5) * step;
    let n = chars.len();
    let mut pitches: Vec<f64> = (1..n).map(|k| centre(k) - centre(k - 1)).collect();
    pitches.sort_by(f64::total_cmp);
    let pitch = pitches.get(pitches.len() / 2).copied().unwrap_or(width);
    (0..n)
        .map(|k| {
            let start = if k == 0 {
                centre(0) - pitch / 2.0
            } else {
                (centre(k - 1) + centre(k)) / 2.0
            };
            let end = if k + 1 == n {
                centre(k) + pitch / 2.0
            } else {
                (centre(k) + centre(k + 1)) / 2.0
            };
            (
                (start / width).clamp(0.0, 1.0),
                (end / width).clamp(0.0, 1.0),
            )
        })
        .collect()
}

/// Builds the page `page_pt` points large from `lines` read on an image
/// `image_px` pixels large.
pub(crate) fn build(
    lines: &[ReadLine],
    image_px: (usize, usize),
    page_pt: (f64, f64),
    engine: &str,
) -> OcrPage {
    let (sx, sy) = (
        page_pt.0 / image_px.0.max(1) as f64,
        page_pt.1 / image_px.1.max(1) as f64,
    );
    let lines: Vec<OcrLine> = lines
        .iter()
        .filter(|line| !line.decoded.chars.is_empty())
        .map(|line| ocr_line(line, sx, sy))
        .collect();
    let items: Vec<Item> = lines
        .iter()
        .map(|line| Item {
            bbox: [line.bbox.x0, line.bbox.y0, line.bbox.x1, line.bbox.y1],
            vertical: line.orientation == Orientation::Vertical,
        })
        .collect();
    let blocks = reading_order(&items)
        .into_iter()
        .map(|block| OcrBlock {
            bbox: pdfconv_core::PageRect {
                x0: block.bbox[0],
                y0: block.bbox[1],
                x1: block.bbox[2],
                y1: block.bbox[3],
            },
            orientation: orientation(block.vertical),
            lines: block.lines.iter().map(|&i| lines[i].clone()).collect(),
        })
        .collect();
    OcrPage {
        width_pt: page_pt.0,
        height_pt: page_pt.1,
        engine: engine.to_owned(),
        blocks,
    }
}

fn orientation(vertical: bool) -> Orientation {
    if vertical {
        Orientation::Vertical
    } else {
        Orientation::Horizontal
    }
}

/// One read line in points, with its words and characters.
fn ocr_line(line: &ReadLine, sx: f64, sy: f64) -> OcrLine {
    let q = line.placed.reading_quad();
    let lerp = |a: Pt, b: Pt, t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    // A point `u` along the line and `v` across it, both 0 to 1, in points.
    let at = |u: f64, v: f64| {
        let p = lerp(lerp(q[0], q[1], u), lerp(q[3], q[2], u), v);
        [p[0] * sx, p[1] * sy]
    };
    let span = |u0: f64, u1: f64| -> Quad { [at(u0, 0.0), at(u1, 0.0), at(u1, 1.0), at(u0, 1.0)] };

    let spans = char_spans(line);
    let mut words = Vec::new();
    let mut word: Vec<(usize, OcrChar)> = Vec::new();
    let mut flush = |word: &mut Vec<(usize, OcrChar)>| {
        if let (Some(first), Some(last)) = (word.first(), word.last()) {
            let quad = span(spans[first.0].0, spans[last.0].1);
            let text = word.iter().map(|c| c.1.text.as_str()).collect();
            let confidence = word.iter().map(|c| c.1.confidence).sum::<f32>() / word.len() as f32;
            words.push(OcrWord {
                quad,
                text,
                confidence,
                chars: word.drain(..).map(|c| c.1).collect(),
            });
        }
    };
    for (k, (c, &(u0, u1))) in line.decoded.chars.iter().zip(&spans).enumerate() {
        if c.text.trim().is_empty() {
            flush(&mut word);
        } else {
            word.push((
                k,
                OcrChar {
                    quad: span(u0, u1),
                    text: c.text.clone(),
                    confidence: c.probability,
                },
            ));
        }
    }
    flush(&mut word);

    let quad = span(0.0, 1.0);
    OcrLine {
        quad,
        bbox: quad_bbox(&quad),
        orientation: orientation(line.placed.vertical),
        text: line.decoded.text(),
        confidence: line.decoded.score,
        words,
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::DecodedChar;
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> PxQuad {
        [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    /// A rectangle `w` × `h` centred on (cx, cy) and turned by `degrees`.
    fn turned(cx: f64, cy: f64, w: f64, h: f64, degrees: f64) -> PxQuad {
        let (s, c) = degrees.to_radians().sin_cos();
        [[-w, -h], [w, -h], [w, h], [-w, h]]
            .map(|[x, y]| [cx + (x * c - y * s) / 2.0, cy + (x * s + y * c) / 2.0])
    }

    #[test]
    fn aspect_decides_and_squares_follow_the_majority() {
        let placed = place(&[
            rect(0.0, 0.0, 20.0, 200.0),
            rect(30.0, 0.0, 50.0, 200.0),
            rect(60.0, 0.0, 200.0, 20.0),
            rect(60.0, 30.0, 80.0, 50.0),
        ]);
        let vertical: Vec<bool> = placed.iter().map(|p| p.vertical).collect();
        assert_eq!(vertical, [true, true, false, true]);
    }

    #[test]
    fn a_column_turned_by_2_degrees_stays_vertical() {
        for degrees in [-2.0, 2.0, 88.0 - 90.0] {
            let placed = place(&[turned(500.0, 500.0, 30.0, 600.0, degrees)]);
            assert!(placed[0].vertical, "{degrees}°");
            let [tl, tr, br, bl] = placed[0].upright;
            assert!(
                tl[0] < tr[0] && tl[1] < bl[1] && br[0] > bl[0] && br[1] > tr[1],
                "{degrees}°: {:?}",
                placed[0].upright
            );
        }
        let placed = place(&[turned(500.0, 500.0, 600.0, 30.0, 2.0)]);
        assert!(!placed[0].vertical);
    }

    #[test]
    fn a_vertical_line_is_cut_and_turned() {
        let mut img = Bgr::new(10, 30);
        // Mark the top-right pixel of the column.
        let i = (5 * 10 + 7) * 3;
        img.data[i..i + 3].copy_from_slice(&[255, 255, 255]);
        let line = Placed {
            upright: rect(2.0, 5.0, 8.0, 25.0),
            vertical: true,
        };
        let Some(crop) = crop(&img, &line) else {
            unreachable!("the crop is not empty")
        };
        assert_eq!((crop.w, crop.h), (20, 6));
        // The column's top-right pixel (7, 5) comes to the crop's top-left.
        let marked: Vec<usize> = (0..crop.w * crop.h)
            .filter(|i| crop.data[i * 3] == 255)
            .collect();
        assert_eq!(marked, [0]);
    }

    fn decoded(steps: &[(&str, usize)]) -> Decoded {
        Decoded {
            chars: steps
                .iter()
                .map(|(text, step)| DecodedChar {
                    text: (*text).to_owned(),
                    first_step: *step,
                    last_step: *step,
                    probability: 0.9,
                })
                .collect(),
            score: 0.9,
        }
    }

    #[test]
    fn characters_are_placed_along_the_reading_direction() {
        // A vertical column 100 px long and 20 px wide; the crop resized to
        // 48 high is 240 wide, 30 steps; characters on steps 4, 14 and 24.
        let line = ReadLine {
            placed: Placed {
                upright: rect(100.0, 0.0, 120.0, 100.0),
                vertical: true,
            },
            decoded: decoded(&[("一", 4), ("二", 14), ("三", 24)]),
            resized_w: 240,
        };
        let page = build(&[line], (200, 200), (100.0, 100.0), "test");
        let ocr = &page.blocks[0].lines[0];
        assert_eq!(ocr.orientation, Orientation::Vertical);
        assert_eq!(ocr.text, "一二三");
        // Top-right first, in points (half the pixels).
        assert_eq!(ocr.quad[0], [60.0, 0.0]);
        let chars = &ocr.words[0].chars;
        let tops: Vec<f64> = chars.iter().map(|c| c.quad[0][1]).collect();
        assert!(tops.windows(2).all(|t| t[0] < t[1]), "{tops:?}");
        // The middle character is centred on step 14: (14.5 × 8) / 240 of 50 pt.
        let middle = (chars[1].quad[0][1] + chars[1].quad[1][1]) / 2.0;
        assert!(
            (middle - 14.5 * 8.0 / 240.0 * 50.0).abs() < 1e-9,
            "{middle}"
        );
    }

    #[test]
    fn spaces_split_words_and_empty_lines_are_left_out() {
        let lines = [
            ReadLine {
                placed: Placed {
                    upright: rect(0.0, 0.0, 200.0, 20.0),
                    vertical: false,
                },
                decoded: decoded(&[("a", 2), ("b", 4), (" ", 6), ("c", 8)]),
                resized_w: 480,
            },
            ReadLine {
                placed: Placed {
                    upright: rect(0.0, 50.0, 200.0, 70.0),
                    vertical: false,
                },
                decoded: Decoded::default(),
                resized_w: 480,
            },
        ];
        let page = build(&lines, (200, 100), (200.0, 100.0), "test");
        assert_eq!(page.blocks.len(), 1);
        let words: Vec<&str> = page.blocks[0].lines[0]
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect();
        assert_eq!(words, ["ab", "c"]);
        assert_eq!(page.text(), "ab c\n");
    }
}
