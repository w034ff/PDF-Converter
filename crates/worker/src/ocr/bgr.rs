//! Image operations that reproduce the OpenCV calls PaddleOCR makes (design
//! ocr §3, 前処理): resizing, the perspective crop of a line and turning it.
//!
//! Images are 8-bit, three channels interleaved, in **BGR** order as
//! `cv2.imread` gives them: PaddleOCR normalises the channels in that order,
//! so keeping BGR avoids a silent swap.

/// An 8-bit BGR image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Bgr {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

impl Bgr {
    pub(crate) fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            data: vec![0; w * h * 3],
        }
    }

    /// The image of RGB pixels `rgb` (`w` × `h`), or `None` when the length
    /// does not match.
    pub(crate) fn from_rgb(rgb: &[u8], w: usize, h: usize) -> Option<Self> {
        if rgb.len() != w * h * 3 {
            return None;
        }
        let mut data = Vec::with_capacity(rgb.len());
        for [r, g, b] in rgb.as_chunks::<3>().0 {
            data.extend_from_slice(&[*b, *g, *r]);
        }
        Some(Self { w, h, data })
    }

    fn at(&self, x: usize, y: usize) -> &[u8] {
        let i = (y * self.w + x) * 3;
        &self.data[i..i + 3]
    }

    /// `np.rot90(img)`: turned 90° counter-clockwise.
    pub(crate) fn rot90(&self) -> Self {
        let mut out = Self::new(self.h, self.w);
        for i in 0..out.h {
            for j in 0..out.w {
                // out[i, j] = in[j, W - 1 - i]
                let o = (i * out.w + j) * 3;
                out.data[o..o + 3].copy_from_slice(self.at(self.w - 1 - i, j));
            }
        }
        out
    }
}

const COEF_BITS: i32 = 11;
const COEF_SCALE: i32 = 1 << COEF_BITS;

/// Per-axis table of `cv2.resize(INTER_LINEAR)`: the two source indices and
/// the fixed-point weight of the second.
fn linear_table(src: usize, dst: usize) -> Vec<(usize, usize, i32)> {
    let scale = src as f64 / dst as f64;
    (0..dst)
        .map(|d| {
            let f = (d as f64 + 0.5) * scale - 0.5;
            let mut x0 = f.floor() as isize;
            let mut a = (f - x0 as f64) as f32;
            if x0 < 0 {
                x0 = 0;
                a = 0.0;
            }
            if x0 as usize >= src - 1 {
                x0 = src as isize - 1;
                a = 0.0;
            }
            let x0 = x0 as usize;
            let x1 = (x0 + 1).min(src - 1);
            let w1 = (a * COEF_SCALE as f32).round() as i32;
            (x0, x1, w1)
        })
        .collect()
}

/// `cv2.resize(src, (dw, dh))` with the default `INTER_LINEAR`, including
/// OpenCV's fixed-point rounding for 8-bit images. Downscaling samples only
/// the two nearest pixels per axis, as OpenCV does. Both sizes are at least 1.
pub(crate) fn resize_linear(src: &Bgr, dw: usize, dh: usize) -> Bgr {
    if src.w == dw && src.h == dh {
        return src.clone();
    }
    let xt = linear_table(src.w, dw);
    let yt = linear_table(src.h, dh);
    // The horizontal pass of each source row that is needed, as value × 2048.
    let mut rows: Vec<Option<Vec<i32>>> = vec![None; src.h];
    let horizontal = |y: usize| {
        let line = &src.data[y * src.w * 3..(y + 1) * src.w * 3];
        let mut row = vec![0_i32; dw * 3];
        for (d, &(x0, x1, w1)) in xt.iter().enumerate() {
            let w0 = COEF_SCALE - w1;
            for c in 0..3 {
                row[d * 3 + c] =
                    i32::from(line[x0 * 3 + c]) * w0 + i32::from(line[x1 * 3 + c]) * w1;
            }
        }
        row
    };
    let mut out = Bgr::new(dw, dh);
    for (dy, &(y0, y1, b1)) in yt.iter().enumerate() {
        for y in [y0, y1] {
            if rows[y].is_none() {
                rows[y] = Some(horizontal(y));
            }
        }
        let b0 = COEF_SCALE - b1;
        let (Some(r0), Some(r1)) = (&rows[y0], &rows[y1]) else {
            continue;
        };
        let o = &mut out.data[dy * dw * 3..(dy + 1) * dw * 3];
        for ((value, s0), s1) in o.iter_mut().zip(r0).zip(r1) {
            // OpenCV's VResizeLinear<uchar>.
            let v = (((b0 * (s0 >> 4)) >> 16) + ((b1 * (s1 >> 4)) >> 16) + 2) >> 2;
            *value = v.clamp(0, 255) as u8;
        }
    }
    out
}

/// The 3×3 homography that maps `from[i]` to `to[i]`
/// (`cv2.getPerspectiveTransform`), row-major with the last element 1.
pub(crate) fn perspective_transform(from: &[[f64; 2]; 4], to: &[[f64; 2]; 4]) -> [f64; 9] {
    let mut a = [[0.0_f64; 9]; 8];
    for (i, ([x, y], [u, v])) in from.iter().zip(to).enumerate() {
        a[i] = [*x, *y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u, *u];
        a[i + 4] = [0.0, 0.0, 0.0, *x, *y, 1.0, -x * v, -y * v, *v];
    }
    // Gauss-Jordan elimination with partial pivoting.
    for col in 0..8 {
        let pivot = (col..8)
            .max_by(|&p, &q| a[p][col].abs().total_cmp(&a[q][col].abs()))
            .unwrap_or(col);
        a.swap(col, pivot);
        let d = a[col][col];
        if d == 0.0 {
            continue;
        }
        for value in &mut a[col][col..] {
            *value /= d;
        }
        let pivot_row = a[col];
        for (r, row) in a.iter_mut().enumerate() {
            let f = row[col];
            if r != col && f != 0.0 {
                for (value, pivot) in row[col..].iter_mut().zip(&pivot_row[col..]) {
                    *value -= f * pivot;
                }
            }
        }
    }
    let mut m = [0.0; 9];
    for (value, row) in m.iter_mut().zip(&a) {
        *value = row[8];
    }
    m[8] = 1.0;
    m
}

/// OpenCV's `interpolateCubic` weights, A = -0.75.
fn cubic_coeffs(x: f32) -> [f32; 4] {
    const A: f32 = -0.75;
    let c0 = ((A * (x + 1.0) - 5.0 * A) * (x + 1.0) + 8.0 * A) * (x + 1.0) - 4.0 * A;
    let c1 = ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0;
    let c2 = ((A + 2.0) * (1.0 - x) - (A + 3.0)) * (1.0 - x) * (1.0 - x) + 1.0;
    [c0, c1, c2, 1.0 - c0 - c1 - c2]
}

/// Sub-pixel steps of OpenCV's `INTER_TAB_SIZE`.
const TAB_SIZE: usize = 32;

/// `cv2.warpPerspective(src, M, (w, h), flags=INTER_CUBIC,
/// borderMode=BORDER_REPLICATE)` where `inv` maps destination pixels to
/// source pixels. Sub-pixel positions are quantised to 1/32 as OpenCV does;
/// the weights stay float.
pub(crate) fn warp_perspective_cubic(src: &Bgr, inv: &[f64; 9], w: usize, h: usize) -> Bgr {
    let mut out = Bgr::new(w, h);
    if src.w == 0 || src.h == 0 {
        return out;
    }
    if let Some((dx, dy)) = integer_translation(inv) {
        // Every sample falls on a pixel centre, where the cubic weights are
        // [0, 1, 0, 0]: the warp is a copy with replicated borders.
        for y in 0..h {
            let sy = (y as isize + dy).clamp(0, src.h as isize - 1) as usize;
            for x in 0..w {
                let sx = (x as isize + dx).clamp(0, src.w as isize - 1) as usize;
                let o = (y * w + x) * 3;
                out.data[o..o + 3].copy_from_slice(src.at(sx, sy));
            }
        }
        return out;
    }
    let tab: Vec<[f32; 4]> = (0..TAB_SIZE)
        .map(|i| cubic_coeffs(i as f32 / TAB_SIZE as f32))
        .collect();
    let (sw, sh) = (src.w as isize, src.h as isize);
    let scale = TAB_SIZE as f64;
    for y in 0..h {
        for x in 0..w {
            let (xf, yf) = (x as f64, y as f64);
            let ww = inv[6] * xf + inv[7] * yf + inv[8];
            let ww = if ww == 0.0 { 0.0 } else { scale / ww };
            let sx = ((inv[0] * xf + inv[1] * yf + inv[2]) * ww).round() as isize;
            let sy = ((inv[3] * xf + inv[4] * yf + inv[5]) * ww).round() as isize;
            let (ix, iy) = (sx >> 5, sy >> 5);
            let (cx, cy) = (&tab[(sx & 31) as usize], &tab[(sy & 31) as usize]);
            let mut acc = [0_f32; 3];
            for (j, wy) in cy.iter().enumerate() {
                let yy = (iy - 1 + j as isize).clamp(0, sh - 1) as usize;
                for (i, wx) in cx.iter().enumerate() {
                    let xx = (ix - 1 + i as isize).clamp(0, sw - 1) as usize;
                    let weight = wy * wx;
                    for (sum, value) in acc.iter_mut().zip(src.at(xx, yy)) {
                        *sum += weight * f32::from(*value);
                    }
                }
            }
            let o = (y * w + x) * 3;
            for (value, sum) in out.data[o..o + 3].iter_mut().zip(acc) {
                *value = sum.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// `(dx, dy)` when `inv` only moves pixels by whole numbers (the usual
/// axis-aligned line).
fn integer_translation(inv: &[f64; 9]) -> Option<(isize, isize)> {
    const EPS: f64 = 1e-9;
    let near = |a: f64, b: f64| (a - b).abs() < EPS;
    let is_int = |v: f64| (v - v.round()).abs() < 1.0 / 256.0;
    (near(inv[0], 1.0)
        && near(inv[1], 0.0)
        && near(inv[3], 0.0)
        && near(inv[4], 1.0)
        && near(inv[6], 0.0)
        && near(inv[7], 0.0)
        && near(inv[8], 1.0)
        && is_int(inv[2])
        && is_int(inv[5]))
    .then(|| (inv[2].round() as isize, inv[5].round() as isize))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: usize, h: usize) -> Bgr {
        let mut img = Bgr::new(w, h);
        for (i, v) in img.data.iter_mut().enumerate() {
            *v = (i * 7 % 251) as u8;
        }
        img
    }

    #[test]
    fn rot90_turns_counter_clockwise() {
        let img = image(3, 2);
        let turned = img.rot90();
        assert_eq!((turned.w, turned.h), (2, 3));
        // The top-right pixel goes to the top-left.
        assert_eq!(turned.at(0, 0), img.at(2, 0));
        assert_eq!(turned.at(1, 2), img.at(0, 1));
    }

    #[test]
    fn resizing_to_the_same_size_keeps_the_image() {
        let img = image(5, 4);
        assert_eq!(resize_linear(&img, 5, 4), img);
        let half = resize_linear(&img, 2, 2);
        assert_eq!((half.w, half.h, half.data.len()), (2, 2, 12));
    }

    #[test]
    fn a_whole_pixel_shift_is_a_copy() {
        let img = image(6, 5);
        let from = [[0.0, 0.0], [3.0, 0.0], [3.0, 2.0], [0.0, 2.0]];
        let to = [[2.0, 1.0], [5.0, 1.0], [5.0, 3.0], [2.0, 3.0]];
        let crop = warp_perspective_cubic(&img, &perspective_transform(&from, &to), 3, 2);
        assert_eq!(crop.at(0, 0), img.at(2, 1));
        assert_eq!(crop.at(2, 1), img.at(4, 2));
    }

    #[test]
    fn the_homography_maps_the_corners() {
        let from = [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]];
        let to = [[3.0, 2.0], [13.0, 3.0], [12.0, 8.0], [2.0, 7.0]];
        let m = perspective_transform(&from, &to);
        for ([x, y], [u, v]) in from.iter().zip(&to) {
            let w = m[6] * x + m[7] * y + m[8];
            assert!(((m[0] * x + m[1] * y + m[2]) / w - u).abs() < 1e-9);
            assert!(((m[3] * x + m[4] * y + m[5]) / w - v).abs() < 1e-9);
        }
    }
}
