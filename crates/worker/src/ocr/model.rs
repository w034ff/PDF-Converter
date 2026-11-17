//! The detector's and the recogniser's input and output, written after
//! PaddleOCR 3.x's defaults (design ocr §3, §4): the detector's resize and
//! normalisation, the DB post-processing, the recogniser's resize to height
//! 48, and greedy CTC decoding.

use super::bgr::{Bgr, resize_linear};
use super::contours::region_borders;
use super::geom::{self, PxQuad};

/// ImageNet means and deviations, applied to B, G, R in that order as
/// PaddleOCR does.
const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// The detector's side lengths are multiples of this.
const DET_SIDE_MULTIPLE: f64 = 32.0;

/// PaddleOCR's `DetResizeForTest` limit on the longer side.
const DET_MAX_SIDE_LIMIT: usize = 4000;

/// A pixel of the probability map above this is text (PaddleOCR 3.x's
/// `text_det_thresh`).
const DET_THRESH: f32 = 0.3;

/// A box whose mean probability is below this is dropped
/// (`text_det_box_thresh`).
const DET_BOX_THRESH: f64 = 0.6;

/// How far a box is grown (`text_det_unclip_ratio`).
const DET_UNCLIP_RATIO: f64 = 1.5;

/// At most this many regions are looked at (`max_candidates`).
const DET_MAX_CANDIDATES: usize = 3000;

/// A region whose rectangle is thinner than this many pixels is dropped,
/// before growing and after.
const DET_MIN_SIDE: f64 = 3.0;
const DET_MIN_SIDE_UNCLIPPED: f64 = 5.0;

/// Boxes whose tops are within this many pixels count as one row when the
/// detector's boxes are sorted (`SortQuadBoxes`).
const SORT_ROW_PX: f64 = 10.0;

/// The recogniser's input height.
pub(crate) const REC_HEIGHT: usize = 48;

/// The narrowest and the widest input of the recogniser.
const REC_MIN_WIDTH: usize = 320;
const REC_MAX_WIDTH: usize = 3200;

/// Input columns of the recogniser per CTC time step.
pub(crate) const REC_STEP_PX: usize = 8;

/// The detector's input size for an image `w` × `h` whose longer side is at
/// most `long_side` (`DetResizeForTest.resize_image_type0` with `limit_type`
/// `max`): both sides rounded to multiples of 32.
pub(crate) fn det_size(w: usize, h: usize, long_side: usize) -> (usize, usize) {
    let ratio = if w.max(h) > long_side {
        long_side as f64 / w.max(h) as f64
    } else {
        1.0
    };
    let (mut rh, mut rw) = ((h as f64 * ratio) as usize, (w as f64 * ratio) as usize);
    if rh.max(rw) > DET_MAX_SIDE_LIMIT {
        let r = DET_MAX_SIDE_LIMIT as f64 / rh.max(rw) as f64;
        rh = (rh as f64 * r) as usize;
        rw = (rw as f64 * r) as usize;
    }
    // Python's round() takes halves to the even neighbour.
    let round = |v: usize| {
        (((v as f64 / DET_SIDE_MULTIPLE).round_ties_even() * DET_SIDE_MULTIPLE) as usize)
            .max(DET_SIDE_MULTIPLE as usize)
    };
    (round(rw), round(rh))
}

/// The detector's input for `img`, channel by channel, and its width and height.
pub(crate) fn det_input(img: &Bgr, long_side: usize) -> (Vec<f32>, usize, usize) {
    let (rw, rh) = det_size(img.w, img.h, long_side);
    let resized = resize_linear(img, rw, rh);
    let plane = rw * rh;
    let mut x = vec![0_f32; 3 * plane];
    let alpha: [f32; 3] = std::array::from_fn(|c| 1.0 / 255.0 / DET_STD[c]);
    let beta: [f32; 3] = std::array::from_fn(|c| -DET_MEAN[c] / DET_STD[c]);
    for (i, px) in resized.data.as_chunks::<3>().0.iter().enumerate() {
        for c in 0..3 {
            x[c * plane + i] = f32::from(px[c]) * alpha[c] + beta[c];
        }
    }
    (x, rw, rh)
}

/// `DBPostProcess.boxes_from_bitmap` on the probability map `pred`
/// (`pw` × `ph`): the line boxes in pixels of the image `dw` × `dh`, sorted
/// as `SortQuadBoxes` sorts them.
pub(crate) fn db_boxes(pred: &[f32], pw: usize, ph: usize, dw: usize, dh: usize) -> Vec<PxQuad> {
    let mask: Vec<bool> = pred.iter().map(|p| *p > DET_THRESH).collect();
    let (sx, sy) = (dw as f64 / pw as f64, dh as f64 / ph as f64);
    let mut out = Vec::new();
    for border in region_borders(&mask, pw, ph)
        .iter()
        .take(DET_MAX_CANDIDATES)
    {
        let (q, side) = geom::mini_box(border);
        if side < DET_MIN_SIDE {
            continue;
        }
        if geom::quad_mean(pred, pw, ph, &q) < DET_BOX_THRESH {
            continue;
        }
        let (grown, side) = geom::mini_box(&geom::unclip(&q, DET_UNCLIP_RATIO));
        if side < DET_MIN_SIDE_UNCLIPPED {
            continue;
        }
        out.push(grown.map(|v| {
            [
                (v[0] * sx).round_ties_even().clamp(0.0, dw as f64),
                (v[1] * sy).round_ties_even().clamp(0.0, dh as f64),
            ]
        }));
    }
    sort_quads(out)
}

/// `SortQuadBoxes`: top to bottom, then left to right within one row.
fn sort_quads(mut boxes: Vec<PxQuad>) -> Vec<PxQuad> {
    boxes.sort_by(|a, b| {
        a[0][1]
            .total_cmp(&b[0][1])
            .then(a[0][0].total_cmp(&b[0][0]))
    });
    for i in 0..boxes.len().saturating_sub(1) {
        for j in (0..=i).rev() {
            if (boxes[j + 1][0][1] - boxes[j][0][1]).abs() < SORT_ROW_PX
                && boxes[j + 1][0][0] < boxes[j][0][0]
            {
                boxes.swap(j, j + 1);
            } else {
                break;
            }
        }
    }
    boxes
}

/// The width of a crop `w` × `h` resized to height 48 before it is cut to the
/// batch's input width, and the input width it needs on its own
/// (`OCRReisizeNormImg.resize_norm_img`).
pub(crate) fn rec_width(w: usize, h: usize) -> (usize, usize) {
    let ratio = w as f64 / h.max(1) as f64;
    let max_ratio = (REC_MIN_WIDTH as f64 / REC_HEIGHT as f64).max(ratio);
    let canvas = (REC_HEIGHT as f64 * max_ratio) as usize;
    if canvas > REC_MAX_WIDTH {
        return (REC_MAX_WIDTH, REC_MAX_WIDTH);
    }
    ((REC_HEIGHT as f64 * ratio).ceil().max(1.0) as usize, canvas)
}

/// Writes `crop` resized to height 48 into `out` (3 × 48 × `tw`, zero
/// elsewhere), `tw` being the widest input of its batch, and returns its
/// resized width.
pub(crate) fn rec_fill(crop: &Bgr, out: &mut [f32], tw: usize) -> usize {
    let rw = rec_width(crop.w, crop.h).0.min(tw);
    let resized = resize_linear(crop, rw, REC_HEIGHT);
    let plane = REC_HEIGHT * tw;
    for y in 0..REC_HEIGHT {
        for x in 0..rw {
            let px = &resized.data[(y * rw + x) * 3..(y * rw + x) * 3 + 3];
            for (c, value) in px.iter().enumerate() {
                out[c * plane + y * tw + x] = f32::from(*value) / 255.0 * 2.0 - 1.0;
            }
        }
    }
    rw
}

/// One character of a decoded line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DecodedChar {
    pub text: String,
    /// The first and last time step of its run.
    pub first_step: usize,
    pub last_step: usize,
    pub probability: f32,
}

/// The text of one crop: its characters and their mean probability.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Decoded {
    pub chars: Vec<DecodedChar>,
    pub score: f32,
}

impl Decoded {
    pub(crate) fn text(&self) -> String {
        self.chars.iter().map(|c| c.text.as_str()).collect()
    }
}

/// Index and value of the largest element, the first one on ties as numpy's
/// `argmax`. The maximum is found in 16 lanes so that it vectorises.
fn argmax(row: &[f32]) -> (usize, f32) {
    let mut lanes = [f32::NEG_INFINITY; 16];
    let (chunks, rest) = row.as_chunks::<16>();
    for chunk in chunks {
        for (lane, v) in lanes.iter_mut().zip(chunk) {
            if *v > *lane {
                *lane = *v;
            }
        }
    }
    let m = lanes
        .iter()
        .chain(rest)
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    (row.iter().position(|&v| v == m).unwrap_or(0), m)
}

/// Greedy CTC decoding (`CTCLabelDecode`) of `probs`, `steps` rows of
/// `classes` values; class 0 is the blank and class `i` is `dict[i]`.
pub(crate) fn ctc_decode(probs: &[f32], steps: usize, classes: usize, dict: &[String]) -> Decoded {
    let mut out = Decoded::default();
    let mut prev = usize::MAX;
    for step in 0..steps {
        let (best, p) = argmax(&probs[step * classes..(step + 1) * classes]);
        if best != 0 && best != prev {
            out.chars.push(DecodedChar {
                text: dict.get(best).cloned().unwrap_or_default(),
                first_step: step,
                last_step: step,
                probability: p,
            });
        } else if best != 0
            && let Some(last) = out.chars.last_mut()
        {
            last.last_step = step;
        }
        prev = best;
    }
    out.score = if out.chars.is_empty() {
        0.0
    } else {
        out.chars.iter().map(|c| c.probability).sum::<f32>() / out.chars.len() as f32
    };
    out
}

/// The crops `(w, h)` of each recogniser run: crops sorted by aspect ratio as
/// PaddleOCR sorts them, cut after `max_lines` crops or before the padded
/// width of the batch, its crop count times its widest input, would pass
/// `max_width_sum`. A crop wider than that on its own still gets a batch.
pub(crate) fn batches(
    sizes: &[(usize, usize)],
    max_lines: usize,
    max_width_sum: usize,
) -> Vec<Vec<usize>> {
    let ratio = |i: usize| sizes[i].0 as f64 / sizes[i].1.max(1) as f64;
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| ratio(a).total_cmp(&ratio(b)));
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for i in order {
        // Sorted, so the new crop needs the widest input of the batch.
        let padded = (current.len() + 1) * rec_width(sizes[i].0, sizes[i].1).1;
        if !current.is_empty() && (current.len() == max_lines.max(1) || padded > max_width_sum) {
            out.push(std::mem::take(&mut current));
        }
        current.push(i);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_sides_round_to_multiples_of_32() {
        assert_eq!(det_size(1600, 1131, 1600), (1600, 1120));
        assert_eq!(det_size(3200, 2262, 1600), (1600, 1120));
        assert_eq!(det_size(10, 10, 1600), (32, 32));
        // 48 / 32 = 1.5 and 80 / 32 = 2.5 both go to the even neighbour, 2.
        assert_eq!(det_size(48, 80, 1600), (64, 64));
    }

    #[test]
    fn recogniser_widths_follow_paddleocr() {
        // Short crops are padded to 320.
        assert_eq!(rec_width(100, 48), (100, 320));
        assert_eq!(rec_width(1000, 48), (1000, 1000));
        assert_eq!(rec_width(400, 24), (800, 800));
        // 48 × 1001 / 47 = 1022.3: the input is 1022 wide, the resize 1023.
        assert_eq!(rec_width(1001, 47), (1023, 1022));
        assert_eq!(rec_width(100_000, 48), (3200, 3200));
    }

    #[test]
    fn ctc_merges_runs_and_skips_blanks() {
        let dict: Vec<String> = ["", "a", "b"].iter().map(|s| (*s).to_owned()).collect();
        // a a _ a b
        let rows = [
            [0.1, 0.8, 0.1],
            [0.1, 0.6, 0.3],
            [0.9, 0.05, 0.05],
            [0.2, 0.7, 0.1],
            [0.1, 0.1, 0.8],
        ];
        let probs: Vec<f32> = rows.iter().flatten().copied().collect();
        let decoded = ctc_decode(&probs, 5, 3, &dict);
        assert_eq!(decoded.text(), "aab");
        assert_eq!(
            decoded
                .chars
                .iter()
                .map(|c| (c.first_step, c.last_step))
                .collect::<Vec<_>>(),
            [(0, 1), (3, 3), (4, 4)]
        );
        assert!((decoded.score - (0.8 + 0.7 + 0.8) / 3.0).abs() < 1e-6);
    }

    #[test]
    fn batches_stop_at_the_line_count_or_the_padded_width() {
        assert_eq!(
            batches(&[(100, 48); 4], 3, 10_000),
            [vec![0, 1, 2], vec![3]]
        );
        // 2 × 600 fits in 1500, 3 × 600 does not; a crop over the limit runs alone.
        assert_eq!(
            batches(&[(2000, 48), (600, 48), (600, 48), (600, 48)], 6, 1500),
            [vec![1, 2], vec![3], vec![0]]
        );
    }
}
