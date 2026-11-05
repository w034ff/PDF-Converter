//! Vertical item 3 (method 2): a vertical line is cut into characters, which
//! are set upright side by side and read as an ordinary horizontal line.
//!
//! 1. The column is unrolled without turning it (`crop_upright`).
//! 2. Ink is the crop's dark pixels (Otsu's threshold). The em is estimated
//!    from the column's ink width (the contiguous run of columns around the
//!    peak); the pitch from the autocorrelation of the
//!    ink per row, between [`PITCH_MIN`] and [`PITCH_MAX`] ems.
//! 3. Cuts between characters are placed by dynamic programming: each cut
//!    costs the ink on its row, and each step from one cut to the next costs
//!    [`LAMBDA`] × (relative deviation from the pitch)², steps limited to
//!    [`STEP_MIN`]–[`STEP_MAX`] pitches. Cells without ink at the ends drop.
//! 4. Each cell's em box: centred on the column; along it, on a straight line
//!    fitted through the ink centres of the full-size characters (a cell
//!    that holds a small mark has no reliable centre of its own).
//! 5. Vertical forms are put back (rules chosen on the development pages,
//!    see `VERTICAL.md`): a thin tall glyph (ー, ｜) or a flat glyph in the
//!    top or bottom third (﹁﹂︵︶) is turned 90° counter-clockwise; a glyph
//!    small in both directions (、。 and small kana, set to the upper right
//!    in vertical text) has its position mirrored through the em's centre.
//! 6. The em squares are set left to right with `gap` between them and
//!    `margin` around, and read by the rec model. Each emitted character is
//!    given to the cell under the centre of its CTC steps; the cell's em box
//!    is the character's position.

use crate::engine::{CtcBias, RecText};
use crate::imgops::Bgr;

pub const PITCH_MIN: f64 = 0.8;
pub const PITCH_MAX: f64 = 1.8;
pub const STEP_MIN: f64 = 0.75;
pub const STEP_MAX: f64 = 1.3;
pub const LAMBDA: f64 = 1.0;
/// The column's ink width (rows that hold at least 10 % of the peak) in ems.
pub const INK_WIDTH_EM: f64 = 0.9;

#[derive(Clone, Debug)]
pub struct CutParams {
    /// Space between em squares, in ems.
    pub gap: f64,
    /// Space above, below and at the ends of the line, in ems.
    pub margin: f64,
    /// Put the vertical forms back (step 5).
    pub restore: bool,
    /// Offsets for the CTC decoding of the cut lines (method 1 on top).
    pub bias: Option<CtcBias>,
    /// Item 4: keep PP-OCR's turned reading for lines it read as at most
    /// this many characters, and when the cut reading is longer than twice
    /// the turned one plus `runaway_slack`. `None`: no guard.
    pub guard: Option<(usize, usize)>,
}

impl Default for CutParams {
    fn default() -> Self {
        Self { gap: 0.0, margin: 0.15, restore: true, bias: None, guard: None }
    }
}

/// A cut column: the em, the column's centre across, and each cell's cut
/// range and em centre along the crop (crop pixels).
#[derive(Clone, Debug)]
pub struct Segmentation {
    pub em: f64,
    pub pitch: f64,
    pub cx: f64,
    pub cuts: Vec<(f64, f64)>,
    pub centres: Vec<f64>,
}

fn gray(img: &Bgr) -> Vec<u8> {
    img.data.chunks_exact(3).map(|p| ((u32::from(p[0]) * 29 + u32::from(p[1]) * 150 + u32::from(p[2]) * 77) >> 8) as u8).collect()
}

fn otsu(g: &[u8]) -> u8 {
    let mut hist = [0usize; 256];
    for &v in g {
        hist[v as usize] += 1;
    }
    let total = g.len() as f64;
    let sum: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut w0, mut sum0, mut best, mut thr) = (0.0, 0.0, -1.0, 128u8);
    for (t, &c) in hist.iter().enumerate() {
        w0 += c as f64;
        sum0 += t as f64 * c as f64;
        let w1 = total - w0;
        if w0 == 0.0 || w1 == 0.0 {
            continue;
        }
        let (m0, m1) = (sum0 / w0, (sum - sum0) / w1);
        let v = w0 * w1 * (m0 - m1).powi(2);
        if v > best {
            best = v;
            thr = t as u8;
        }
    }
    thr
}

/// Ink mask (true = ink) and the background level of a crop.
pub fn ink_mask(img: &Bgr) -> (Vec<bool>, u8) {
    let g = gray(img);
    let t = otsu(&g);
    let mut light: Vec<u8> = g.iter().copied().filter(|&v| v > t).collect();
    light.sort_unstable();
    let bg = light.get(light.len() / 2).copied().unwrap_or(255);
    (g.iter().map(|&v| v <= t).collect(), bg)
}

/// Splits the upright crop of a vertical line into character cells.
pub fn segment(img: &Bgr) -> Option<Segmentation> {
    let (w, h) = (img.w, img.h);
    if w < 4 || h < 4 {
        return None;
    }
    let (ink, _) = ink_mask(img);
    // Ink per column of the crop → the column's ink band and centre.
    let colp: Vec<usize> = (0..w).map(|x| (0..h).filter(|&y| ink[y * w + x]).count()).collect();
    let peak = *colp.iter().max()?;
    if peak == 0 {
        return None;
    }
    // The contiguous run around the peak column that holds at least 10 % of
    // the peak: ruby or a neighbour's edge beside a gap stays out.
    let px = colp.iter().position(|&c| c == peak)?;
    let mut a = px;
    while a > 0 && colp[a - 1] * 10 >= peak {
        a -= 1;
    }
    let mut b = px;
    while b + 1 < w && colp[b + 1] * 10 >= peak {
        b += 1;
    }
    let (bx0, bx1) = (a as f64, b as f64 + 1.0);
    let em0 = ((bx1 - bx0) / INK_WIDTH_EM).max(4.0);
    let total: f64 = colp[a..=b].iter().sum::<usize>() as f64;
    let cx0 = (a..=b).map(|x| (x as f64 + 0.5) * colp[x] as f64).sum::<f64>() / total;
    // Ink per row, inside the column's band (ruby and neighbours stay out).
    let (xa, xb) = (((cx0 - em0 * 0.55).max(0.0)) as usize, ((cx0 + em0 * 0.55).ceil() as usize).min(w));
    let row: Vec<f64> = (0..h).map(|y| (xa..xb).filter(|&x| ink[y * w + x]).count() as f64).collect();
    let first = row.iter().position(|&v| v > 0.0)?;
    let last = row.iter().rposition(|&v| v > 0.0)?;
    let smooth: Vec<f64> = (0..h).map(|y| (y.saturating_sub(1)..(y + 2).min(h)).map(|k| row[k]).sum::<f64>()).collect();
    // Pitch: autocorrelation of the mean-free profile.
    let len = (last - first + 1) as f64;
    let mean = row[first..=last].iter().sum::<f64>() / len;
    let prof: Vec<f64> = row[first..=last].iter().map(|v| v - mean).collect();
    let (lo, hi) = ((em0 * PITCH_MIN).floor() as usize, (em0 * PITCH_MAX).ceil() as usize);
    let mut pitch = em0;
    if len >= em0 * 2.2 && hi < prof.len() {
        let mut best = f64::MIN;
        for tau in lo.max(2)..=hi {
            let n = prof.len() - tau;
            let r = (0..n).map(|i| prof[i] * prof[i + tau]).sum::<f64>() / n as f64;
            if r > best {
                best = r;
                pitch = tau as f64;
            }
        }
    }
    // Dynamic programming over cut rows, with `pad` virtual white rows each side.
    let pad = pitch.ceil() as isize;
    let n = h as isize + 2 * pad;
    let maxv = smooth.iter().cloned().fold(1.0, f64::max);
    let cost = |y: isize| -> f64 {
        let r = y - pad;
        if r < 0 || r >= h as isize { 0.0 } else { smooth[r as usize] / maxv }
    };
    let (smin, smax) = ((pitch * STEP_MIN).floor().max(1.0) as isize, (pitch * STEP_MAX).ceil() as isize);
    let first_p = first as isize + pad;
    let last_p = last as isize + pad;
    let mut d = vec![f64::INFINITY; n as usize + 1];
    let mut from = vec![-1isize; n as usize + 1];
    for y in 0..=n {
        if y <= first_p {
            d[y as usize] = cost(y);
        }
        for s in smin..=smax {
            let p = y - s;
            if p < 0 || !d[p as usize].is_finite() {
                continue;
            }
            // A cut may not skip over the first ink row from above without a cut at or above it.
            let dev = (s as f64 - pitch) / pitch;
            let v = d[p as usize] + cost(y) + LAMBDA * dev * dev;
            if v < d[y as usize] {
                d[y as usize] = v;
                from[y as usize] = p;
            }
        }
    }
    let end = (last_p + 1..=n).min_by(|&a, &b| d[a as usize].total_cmp(&d[b as usize]))?;
    let mut ys = vec![end];
    let mut y = end;
    while from[y as usize] >= 0 {
        y = from[y as usize];
        ys.push(y);
    }
    ys.reverse();
    let mut cuts: Vec<(f64, f64)> = ys.windows(2).map(|p| ((p[0] - pad) as f64, (p[1] - pad) as f64)).collect();
    let has_ink = |c: &(f64, f64)| {
        let (a, b) = (c.0.max(0.0) as usize, (c.1.max(0.0) as usize).min(h));
        (a..b).any(|y| row[y] > 0.0)
    };
    while cuts.first().is_some_and(|c| !has_ink(c)) {
        cuts.remove(0);
    }
    while cuts.last().is_some_and(|c| !has_ink(c)) {
        cuts.pop();
    }
    if cuts.is_empty() {
        return None;
    }
    // Ink boxes per cell → em, the column centre and the em centres.
    let boxes: Vec<Option<[f64; 4]>> = cuts.iter().map(|c| ink_box(&ink, w, h, (xa, xb), *c)).collect();
    let mut widths: Vec<f64> = boxes.iter().flatten().map(|b| b[2] - b[0]).collect();
    widths.sort_by(f64::total_cmp);
    let em = if widths.len() >= 3 { (widths[widths.len() * 9 / 10] / 0.92).max(em0 * 0.8).min(em0 * 1.25) } else { em0 };
    let full: Vec<(usize, [f64; 4])> = boxes
        .iter()
        .enumerate()
        .filter_map(|(k, b)| b.filter(|b| b[2] - b[0] >= 0.6 * em && b[3] - b[1] >= 0.6 * em).map(|b| (k, b)))
        .collect();
    let cx = if full.is_empty() {
        cx0
    } else {
        let mut v: Vec<f64> = full.iter().map(|(_, b)| (b[0] + b[2]) / 2.0).collect();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let centres: Vec<f64> = if full.len() >= 2 {
        // Least squares: centre = a + p · k.
        let m = full.len() as f64;
        let (sk, sc) = full.iter().fold((0.0, 0.0), |s, (k, b)| (s.0 + *k as f64, s.1 + (b[1] + b[3]) / 2.0));
        let (mk, mc) = (sk / m, sc / m);
        let (num, den) = full.iter().fold((0.0, 0.0), |s, (k, b)| {
            let dk = *k as f64 - mk;
            (s.0 + dk * ((b[1] + b[3]) / 2.0 - mc), s.1 + dk * dk)
        });
        let p = if den > 0.0 { num / den } else { pitch };
        (0..cuts.len()).map(|k| mc + p * (k as f64 - mk)).collect()
    } else if full.len() == 1 {
        let (k0, b) = full[0];
        (0..cuts.len()).map(|k| (b[1] + b[3]) / 2.0 + pitch * (k as f64 - k0 as f64)).collect()
    } else {
        cuts.iter().map(|c| (c.0 + c.1) / 2.0).collect()
    };
    Some(Segmentation { em, pitch, cx, cuts, centres })
}

/// Ink box `[x0, y0, x1, y1]` of the rows `cut` within the columns `xr`.
fn ink_box(ink: &[bool], w: usize, h: usize, xr: (usize, usize), cut: (f64, f64)) -> Option<[f64; 4]> {
    let (a, b) = (cut.0.max(0.0) as usize, (cut.1.max(0.0) as usize).min(h));
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in a..b {
        for x in xr.0..xr.1 {
            if ink[y * w + x] {
                x0 = x0.min(x);
                x1 = x1.max(x + 1);
                y0 = y0.min(y);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x0 != usize::MAX).then(|| [x0 as f64, y0 as f64, x1 as f64, y1 as f64])
}

/// What step 5 did to a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fix {
    None,
    Turned,
    Mirrored,
}

/// The em square of cell `k` (side `s` pixels), upright, with step 5 applied.
fn em_square(img: &Bgr, ink: &[bool], bg: u8, seg: &Segmentation, k: usize, s: usize, restore: bool) -> (Bgr, Fix) {
    let mut sq = Bgr { w: s, h: s, data: vec![bg; s * s * 3] };
    let mut m = vec![false; s * s];
    let (x0, y0) = (seg.cx - s as f64 / 2.0, seg.centres[k] - s as f64 / 2.0);
    let (ca, cb) = seg.cuts[k];
    for i in 0..s {
        let y = (y0 + i as f64).round();
        // Only this cell's rows: neighbours that reach into the em square stay out.
        if y < ca.floor() || y >= cb.ceil() || y < 0.0 || y >= img.h as f64 {
            continue;
        }
        for j in 0..s {
            let x = (x0 + j as f64).round();
            if x < 0.0 || x >= img.w as f64 {
                continue;
            }
            let (xs, ysrc) = (x as usize, y as usize);
            let src = (ysrc * img.w + xs) * 3;
            sq.data[(i * s + j) * 3..(i * s + j) * 3 + 3].copy_from_slice(&img.data[src..src + 3]);
            m[i * s + j] = ink[ysrc * img.w + xs];
        }
    }
    if !restore {
        return (sq, Fix::None);
    }
    let (mut bx0, mut by0, mut bx1, mut by1) = (s, s, 0, 0);
    for i in 0..s {
        for j in 0..s {
            if m[i * s + j] {
                bx0 = bx0.min(j);
                bx1 = bx1.max(j + 1);
                by0 = by0.min(i);
                by1 = by1.max(i + 1);
            }
        }
    }
    if bx0 >= bx1 {
        return (sq, Fix::None);
    }
    let f = |v: usize| v as f64 / s as f64;
    let (bw, bh) = (f(bx1 - bx0), f(by1 - by0));
    let cy = f(by0 + by1) / 2.0;
    if (bw <= 0.25 && bh >= 0.5) || (bh <= 0.4 && bw >= 0.5 && !(0.35..=0.65).contains(&cy)) {
        return (sq.rot90(), Fix::Turned);
    }
    if bw <= 0.75 && bh <= 0.75 {
        // Mirror the ink box's position through the centre, keeping the glyph as it is.
        let (nx0, ny0) = (s - bx1, s - by1);
        let mut out = Bgr { w: s, h: s, data: vec![bg; s * s * 3] };
        for i in by0..by1 {
            for j in bx0..bx1 {
                let (ti, tj) = (ny0 + i - by0, nx0 + j - bx0);
                out.data[(ti * s + tj) * 3..(ti * s + tj) * 3 + 3].copy_from_slice(&sq.data[(i * s + j) * 3..(i * s + j) * 3 + 3]);
            }
        }
        return (out, Fix::Mirrored);
    }
    (sq, Fix::None)
}

/// The cut line: em squares left to right. Returns the image, each cell's
/// horizontal span in it, and what step 5 did to each cell.
pub fn layout(img: &Bgr, seg: &Segmentation, p: &CutParams) -> (Bgr, Vec<(f64, f64)>, Vec<Fix>) {
    let (ink, bg) = ink_mask(img);
    let s = seg.em.round().max(4.0) as usize;
    let gap = (p.gap * s as f64).round() as usize;
    let m = (p.margin * s as f64).round() as usize;
    let n = seg.cuts.len();
    let (w, h) = (2 * m + n * s + n.saturating_sub(1) * gap, s + 2 * m);
    let mut out = Bgr { w, h, data: vec![bg; w * h * 3] };
    let mut spans = Vec::with_capacity(n);
    let mut fixes = Vec::with_capacity(n);
    for k in 0..n {
        let (sq, fix) = em_square(img, &ink, bg, seg, k, s, p.restore);
        let x0 = m + k * (s + gap);
        for i in 0..s {
            let dst = ((m + i) * w + x0) * 3;
            out.data[dst..dst + s * 3].copy_from_slice(&sq.data[i * s * 3..(i + 1) * s * 3]);
        }
        spans.push((x0 as f64, (x0 + s) as f64));
        fixes.push(fix);
    }
    (out, spans, fixes)
}

/// The cell each emitted character falls in: the centre of its CTC steps
/// (8 columns of the image resized to height 48 and width `resized_w`).
pub fn chars_to_cells(rec: &RecText, layout_w: usize, resized_w: usize, spans: &[(f64, f64)]) -> Vec<usize> {
    let scale = layout_w as f64 / resized_w.max(1) as f64;
    rec.char_cols
        .iter()
        .map(|c| {
            let x = ((c.1 + c.2) as f64 / 2.0 + 0.5) * 8.0 * scale;
            spans
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    let da = if x < a.1.0 { a.1.0 - x } else if x > a.1.1 { x - a.1.1 } else { 0.0 };
                    let db = if x < b.1.0 { b.1.0 - x } else if x > b.1.1 { x - b.1.1 } else { 0.0 };
                    da.total_cmp(&db)
                })
                .map_or(0, |(k, _)| k)
        })
        .collect()
}

/// Each character's span along the crop (fractions 0–1 from the top): the
/// em box of its cell, split evenly when several characters share a cell.
pub fn cell_spans(seg: &Segmentation, crop_h: usize, cells: &[usize]) -> Vec<(f64, f64)> {
    let h = crop_h as f64;
    let mut out = Vec::with_capacity(cells.len());
    let mut i = 0;
    while i < cells.len() {
        let k = cells[i];
        let j = (i..cells.len()).take_while(|&j| cells[j] == k).count();
        let (a, b) = (seg.centres[k] - seg.em / 2.0, seg.centres[k] + seg.em / 2.0);
        for q in 0..j {
            let (u0, u1) = (a + (b - a) * q as f64 / j as f64, a + (b - a) * (q + 1) as f64 / j as f64);
            out.push(((u0 / h).clamp(0.0, 1.0), (u1 / h).clamp(0.0, 1.0)));
        }
        i += j;
    }
    out
}
