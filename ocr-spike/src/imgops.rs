//! Image operations that reproduce the OpenCV calls PaddleOCR makes.
//!
//! Images are 8-bit, 3 channels interleaved, in **BGR** order, as
//! `cv2.imread` returns them: PaddleOCR normalises the channels in that order
//! (ImageNet means applied to B, G, R), so keeping BGR avoids a silent swap.

use std::path::Path;

#[derive(Clone, Debug)]
pub struct Bgr {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

impl Bgr {
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, data: vec![0; w * h * 3] }
    }

    /// Loads like `cv2.imread(path, IMREAD_COLOR)`: grey images become three equal channels.
    pub fn open(path: &Path) -> image::ImageResult<Self> {
        let rgb = image::open(path)?.to_rgb8();
        Ok(Self::from_rgb(&rgb))
    }

    pub fn from_rgb(rgb: &image::RgbImage) -> Self {
        let (w, h) = (rgb.width() as usize, rgb.height() as usize);
        let mut data = Vec::with_capacity(w * h * 3);
        for p in rgb.pixels() {
            data.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        Self { w, h, data }
    }

    pub fn to_rgb(&self) -> image::RgbImage {
        let mut out = image::RgbImage::new(self.w as u32, self.h as u32);
        for (i, p) in out.pixels_mut().enumerate() {
            let s = &self.data[i * 3..i * 3 + 3];
            *p = image::Rgb([s[2], s[1], s[0]]);
        }
        out
    }

    #[inline]
    pub fn at(&self, x: usize, y: usize, c: usize) -> u8 {
        self.data[(y * self.w + x) * 3 + c]
    }

    /// `np.rot90(img)`: 90° counter-clockwise.
    pub fn rot90(&self) -> Self {
        let mut out = Self::new(self.h, self.w);
        for i in 0..out.h {
            for j in 0..out.w {
                // out[i, j] = in[j, W-1-i]
                for c in 0..3 {
                    out.data[(i * out.w + j) * 3 + c] = self.at(self.w - 1 - i, j, c);
                }
            }
        }
        out
    }
}

const COEF_BITS: i32 = 11;
const COEF_SCALE: i32 = 1 << COEF_BITS;

/// Per-axis tables of `cv2.resize(INTER_LINEAR)`: source index and the
/// fixed-point weight of the next pixel.
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
/// the two nearest pixels per axis (no anti-aliasing), as OpenCV does.
pub fn resize_linear(src: &Bgr, dw: usize, dh: usize) -> Bgr {
    if src.w == dw && src.h == dh {
        return src.clone();
    }
    let xt = linear_table(src.w, dw);
    let yt = linear_table(src.h, dh);
    // Horizontal pass on every source row that is needed, as i32 (value × 2048).
    let mut rows: Vec<Option<Vec<i32>>> = vec![None; src.h];
    let horiz = |y: usize, rows: &mut Vec<Option<Vec<i32>>>| {
        if rows[y].is_none() {
            let mut r = vec![0i32; dw * 3];
            let line = &src.data[y * src.w * 3..(y + 1) * src.w * 3];
            for (d, &(x0, x1, w1)) in xt.iter().enumerate() {
                let w0 = COEF_SCALE - w1;
                for c in 0..3 {
                    r[d * 3 + c] =
                        i32::from(line[x0 * 3 + c]) * w0 + i32::from(line[x1 * 3 + c]) * w1;
                }
            }
            rows[y] = Some(r);
        }
    };
    let mut out = Bgr::new(dw, dh);
    for (dy, &(y0, y1, b1)) in yt.iter().enumerate() {
        horiz(y0, &mut rows);
        horiz(y1, &mut rows);
        let b0 = COEF_SCALE - b1;
        let (r0, r1) = (rows[y0].as_ref().unwrap(), rows[y1].as_ref().unwrap());
        let o = &mut out.data[dy * dw * 3..(dy + 1) * dw * 3];
        for i in 0..dw * 3 {
            // OpenCV's VResizeLinear<uchar>: ((b0*(S0>>4))>>16) + ((b1*(S1>>4))>>16) + 2) >> 2
            let v = (((b0 * (r0[i] >> 4)) >> 16) + ((b1 * (r1[i] >> 4)) >> 16) + 2) >> 2;
            o[i] = v.clamp(0, 255) as u8;
        }
    }
    out
}

/// Solves the 3×3 homography that maps `from[i]` to `to[i]`
/// (`cv2.getPerspectiveTransform`).
pub fn perspective_transform(from: &[[f64; 2]; 4], to: &[[f64; 2]; 4]) -> [f64; 9] {
    let mut a = [[0.0f64; 9]; 8];
    for i in 0..4 {
        let ([x, y], [u, v]) = (from[i], to[i]);
        a[i] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u, u];
        a[i + 4] = [0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v, v];
    }
    // Gaussian elimination with partial pivoting.
    for col in 0..8 {
        let piv = (col..8).max_by(|&p, &q| a[p][col].abs().total_cmp(&a[q][col].abs())).unwrap();
        a.swap(col, piv);
        let d = a[col][col];
        for k in col..9 {
            a[col][k] /= d;
        }
        for r in 0..8 {
            if r != col {
                let f = a[r][col];
                if f != 0.0 {
                    for k in col..9 {
                        a[r][k] -= f * a[col][k];
                    }
                }
            }
        }
    }
    let mut m = [0.0; 9];
    for i in 0..8 {
        m[i] = a[i][8];
    }
    m[8] = 1.0;
    m
}

fn cubic_coeffs(x: f32) -> [f32; 4] {
    // OpenCV's interpolateCubic, A = -0.75.
    const A: f32 = -0.75;
    let c0 = ((A * (x + 1.0) - 5.0 * A) * (x + 1.0) + 8.0 * A) * (x + 1.0) - 4.0 * A;
    let c1 = ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0;
    let c2 = ((A + 2.0) * (1.0 - x) - (A + 3.0)) * (1.0 - x) * (1.0 - x) + 1.0;
    [c0, c1, c2, 1.0 - c0 - c1 - c2]
}

/// `cv2.warpPerspective(src, M, (w, h), flags=INTER_CUBIC, borderMode=BORDER_REPLICATE)`
/// where `inv` maps destination pixels to source pixels. Sub-pixel positions
/// are quantised to 1/32 like OpenCV's `INTER_TAB_SIZE`; the weights stay float.
pub fn warp_perspective_cubic(src: &Bgr, inv: &[f64; 9], w: usize, h: usize) -> Bgr {
    if let Some((dx, dy)) = integer_translation(inv) {
        // Every sample falls on a pixel centre, where the cubic weights are
        // [0, 1, 0, 0]: the warp is a copy with replicated borders.
        let mut out = Bgr::new(w, h);
        for y in 0..h {
            let sy = (y as isize + dy).clamp(0, src.h as isize - 1) as usize;
            for x in 0..w {
                let sx = (x as isize + dx).clamp(0, src.w as isize - 1) as usize;
                let (o, i) = ((y * w + x) * 3, (sy * src.w + sx) * 3);
                out.data[o..o + 3].copy_from_slice(&src.data[i..i + 3]);
            }
        }
        return out;
    }
    let tab: Vec<[f32; 4]> = (0..32).map(|i| cubic_coeffs(i as f32 / 32.0)).collect();
    let mut out = Bgr::new(w, h);
    let (sw, sh) = (src.w as isize, src.h as isize);
    for y in 0..h {
        for x in 0..w {
            let (xf, yf) = (x as f64, y as f64);
            let ww = inv[6] * xf + inv[7] * yf + inv[8];
            let ww = if ww != 0.0 { 32.0 / ww } else { 0.0 };
            let sx = ((inv[0] * xf + inv[1] * yf + inv[2]) * ww).round() as isize;
            let sy = ((inv[3] * xf + inv[4] * yf + inv[5]) * ww).round() as isize;
            let (ix, iy) = (sx >> 5, sy >> 5);
            let (cx, cy) = (&tab[(sx & 31) as usize], &tab[(sy & 31) as usize]);
            let mut acc = [0f32; 3];
            for (j, wy) in cy.iter().enumerate() {
                let yy = (iy - 1 + j as isize).clamp(0, sh - 1) as usize;
                for (i, wx) in cx.iter().enumerate() {
                    let xx = (ix - 1 + i as isize).clamp(0, sw - 1) as usize;
                    let base = (yy * src.w + xx) * 3;
                    let wgt = wy * wx;
                    for c in 0..3 {
                        acc[c] += wgt * f32::from(src.data[base + c]);
                    }
                }
            }
            for c in 0..3 {
                out.data[(y * w + x) * 3 + c] = acc[c].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// `(dx, dy)` when `inv` only moves pixels by whole numbers (the usual
/// axis-aligned text line).
fn integer_translation(inv: &[f64; 9]) -> Option<(isize, isize)> {
    const EPS: f64 = 1e-9;
    let near = |a: f64, b: f64| (a - b).abs() < EPS;
    let is_int = |v: f64| (v - v.round()).abs() < 1.0 / 64.0 / 4.0;
    (near(inv[0], 1.0) && near(inv[1], 0.0) && near(inv[3], 0.0) && near(inv[4], 1.0)
        && near(inv[6], 0.0) && near(inv[7], 0.0) && near(inv[8], 1.0)
        && is_int(inv[2]) && is_int(inv[5]))
    .then(|| (inv[2].round() as isize, inv[5].round() as isize))
}
