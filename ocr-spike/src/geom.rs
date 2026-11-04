//! Geometry of PaddleOCR's DB post-processing and crop:
//! `cv2.minAreaRect`, `get_mini_boxes`, `unclip` (pyclipper, round joins),
//! and the mean of the probability map inside a quad (`box_score_fast`).

pub type Pt = [f64; 2];
pub type Quad = [Pt; 4];

pub const OPENCV_F32_BIAS: f64 = 0.0;

fn cross(o: Pt, a: Pt, b: Pt) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Andrew's monotone chain; counter-clockwise, without collinear points.
pub fn convex_hull(points: &[Pt]) -> Vec<Pt> {
    let mut p: Vec<Pt> = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let mut lower: Vec<Pt> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<Pt> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Minimum-area enclosing rectangle: its four corners and its side lengths.
/// Like OpenCV, a degenerate set (one point or a line) gives a zero-width rect.
pub fn min_area_rect(points: &[Pt]) -> (Quad, f64, f64) {
    let hull = convex_hull(points);
    if hull.is_empty() {
        return ([[0.0; 2]; 4], 0.0, 0.0);
    }
    if hull.len() == 1 {
        return ([hull[0]; 4], 0.0, 0.0);
    }
    let mut best: Option<(f64, Quad, f64, f64)> = None;
    let n = hull.len();
    for i in 0..n {
        let (a, b) = (hull[i], hull[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len == 0.0 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (vx, vy) = (-uy, ux);
        let (mut umin, mut umax, mut vmin, mut vmax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in &hull {
            let (px, py) = (p[0] - a[0], p[1] - a[1]);
            let u = px * ux + py * uy;
            let v = px * vx + py * vy;
            umin = umin.min(u);
            umax = umax.max(u);
            vmin = vmin.min(v);
            vmax = vmax.max(v);
        }
        let area = (umax - umin) * (vmax - vmin);
        if best.as_ref().is_none_or(|b| area < b.0 - 1e-9) {
            let corner = |u: f64, v: f64| [a[0] + u * ux + v * vx, a[1] + u * uy + v * vy];
            let q = [corner(umin, vmin), corner(umax, vmin), corner(umax, vmax), corner(umin, vmax)];
            best = Some((area, q, umax - umin, vmax - vmin));
        }
    }
    let (_, q, w, h) = best.unwrap();
    (q, w, h)
}

/// `get_mini_boxes`: corners ordered top-left, top-right, bottom-right,
/// bottom-left as PaddleOCR picks them, and the shorter side.
pub fn mini_box(points: &[Pt]) -> (Quad, f64) {
    let (q, w, h) = min_area_rect(points);
    (order_like_paddle(q), w.min(h))
}

pub fn order_like_paddle(q: Quad) -> Quad {
    let mut p = q.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let (i1, i4) = if p[1][1] > p[0][1] { (0, 1) } else { (1, 0) };
    let (i2, i3) = if p[3][1] > p[2][1] { (2, 3) } else { (3, 2) };
    [p[i1], p[i2], p[i3], p[i4]]
}

fn polygon_area(q: &[Pt]) -> f64 {
    let n = q.len();
    (0..n).map(|i| q[i][0] * q[(i + 1) % n][1] - q[(i + 1) % n][0] * q[i][1]).sum::<f64>() / 2.0
}

/// `unclip`: offsets the quad outwards by `area * ratio / perimeter` with
/// round joins, as pyclipper does (integer coordinates, arc tolerance 0.25),
/// and returns the offset polygon's points.
pub fn unclip(q: &Quad, ratio: f64) -> Vec<Pt> {
    let area = polygon_area(q).abs();
    let perim: f64 = (0..4)
        .map(|i| {
            let (a, b) = (q[i], q[(i + 1) % 4]);
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
        })
        .sum();
    if perim == 0.0 {
        return q.to_vec();
    }
    let delta = area * ratio / perim;
    // pyclipper turns float input into integers by truncation, and OpenCV's
    // float32 minAreaRect/boxPoints put integer corners slightly below the
    // integer (334.99994 for 335), so PaddleOCR's quads lose one pixel there.
    // A fixed bias does not reproduce it (the error's sign depends on the
    // box: 1e-4 made 16 of 56 quads identical instead of 47), so it is 0.
    let p: Vec<Pt> = q.iter().map(|v| [(v[0] - OPENCV_F32_BIAS).trunc(), (v[1] - OPENCV_F32_BIAS).trunc()]).collect();
    let sign = if polygon_area(&p) >= 0.0 { 1.0 } else { -1.0 };
    let steps_per_rad = {
        let arc_tol = 0.25f64;
        let steps = std::f64::consts::PI / (1.0 - arc_tol / delta.abs()).acos();
        steps / (2.0 * std::f64::consts::PI)
    };
    let n = p.len();
    let normal = |a: Pt, b: Pt| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt().max(1e-12);
        // Outward normal for a polygon of orientation `sign` (y down, like images).
        [sign * dy / l, -sign * dx / l]
    };
    let mut out = Vec::new();
    for i in 0..n {
        let prev = p[(i + n - 1) % n];
        let cur = p[i];
        let next = p[(i + 1) % n];
        let n1 = normal(prev, cur);
        let n2 = normal(cur, next);
        let a1 = n1[1].atan2(n1[0]);
        let mut a2 = n2[1].atan2(n2[0]);
        // Sweep the short way round from n1 to n2.
        let mut d = a2 - a1;
        while d > std::f64::consts::PI {
            d -= 2.0 * std::f64::consts::PI;
        }
        while d < -std::f64::consts::PI {
            d += 2.0 * std::f64::consts::PI;
        }
        a2 = a1 + d;
        let steps = ((d.abs() * steps_per_rad).ceil() as usize).max(1);
        for s in 0..=steps {
            let a = a1 + (a2 - a1) * s as f64 / steps as f64;
            out.push([(cur[0] + delta * a.cos()).round(), (cur[1] + delta * a.sin()).round()]);
        }
    }
    out
}

/// `box_score_fast`: mean of `pred` (w × h) inside the quad, with the quad's
/// corners truncated to integers like `fillPoly(box.astype(int32))`.
pub fn quad_mean(pred: &[f32], w: usize, h: usize, q: &Quad) -> f64 {
    let xs = q.iter().map(|p| p[0]);
    let ys = q.iter().map(|p| p[1]);
    let clampi = |v: f64, hi: usize| (v as isize).clamp(0, hi as isize - 1) as usize;
    let xmin = clampi(xs.clone().fold(f64::MAX, f64::min).floor(), w);
    let xmax = clampi(xs.fold(f64::MIN, f64::max).ceil(), w);
    let ymin = clampi(ys.clone().fold(f64::MAX, f64::min).floor(), h);
    let ymax = clampi(ys.fold(f64::MIN, f64::max).ceil(), h);
    let poly: Vec<Pt> = q
        .iter()
        .map(|p| [(p[0] - xmin as f64).trunc(), (p[1] - ymin as f64).trunc()])
        .collect();
    let (mut sum, mut cnt) = (0.0f64, 0usize);
    for y in 0..=(ymax - ymin) {
        // Span of the polygon on this row, boundary included.
        let yf = y as f64;
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for i in 0..4 {
            let (a, b) = (poly[i], poly[(i + 1) % 4]);
            if (a[1] - yf) * (b[1] - yf) <= 0.0 {
                if a[1] == b[1] {
                    lo = lo.min(a[0].min(b[0]));
                    hi = hi.max(a[0].max(b[0]));
                } else {
                    let x = a[0] + (yf - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                    lo = lo.min(x);
                    hi = hi.max(x);
                }
            }
        }
        if lo > hi {
            continue;
        }
        let x0 = (lo.round().max(0.0)) as usize;
        let x1 = (hi.round() as usize).min(xmax - xmin);
        for x in x0..=x1 {
            sum += f64::from(pred[(ymin + y) * w + xmin + x]);
            cnt += 1;
        }
    }
    if cnt == 0 { 0.0 } else { sum / cnt as f64 }
}

pub fn quad_bounds(q: &Quad) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for p in q {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    b
}

pub fn iou(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let iw = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let ih = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let inter = iw * ih;
    let ua = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter;
    if ua <= 0.0 { 0.0 } else { inter / ua }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_aligned_rect() {
        let pts = [[1.0, 1.0], [5.0, 1.0], [5.0, 3.0], [1.0, 3.0], [3.0, 2.0]];
        let (q, s) = mini_box(&pts);
        assert_eq!(s, 2.0);
        assert_eq!(q, [[1.0, 1.0], [5.0, 1.0], [5.0, 3.0], [1.0, 3.0]]);
    }

    #[test]
    fn unclip_expands_each_side() {
        let q = [[10.0, 10.0], [50.0, 10.0], [50.0, 20.0], [10.0, 20.0]];
        // area 400, perimeter 100, ratio 1.5 -> 6 px.
        let (r, w, h) = min_area_rect(&unclip(&q, 1.5));
        let b = quad_bounds(&r);
        assert_eq!((w.round(), h.round()), (52.0, 22.0));
        assert_eq!(b, [4.0, 4.0, 56.0, 26.0]);
    }
}
