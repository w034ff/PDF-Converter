//! Geometry of PaddleOCR's DB post-processing (design ocr §4, 検出):
//! `cv2.minAreaRect`, `get_mini_boxes`, `unclip` (pyclipper, round joins) and
//! the mean of the probability map inside a quad (`box_score_fast`).
//! Coordinates are image pixels, y downwards.

pub(crate) type Pt = [f64; 2];
pub(crate) type PxQuad = [Pt; 4];

fn cross(o: Pt, a: Pt, b: Pt) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Andrew's monotone chain, without collinear points.
fn convex_hull(points: &[Pt]) -> Vec<Pt> {
    let mut p: Vec<Pt> = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let half = |points: &mut dyn Iterator<Item = Pt>| {
        let mut chain: Vec<Pt> = Vec::new();
        for q in points {
            while chain.len() >= 2
                && cross(chain[chain.len() - 2], chain[chain.len() - 1], q) <= 0.0
            {
                chain.pop();
            }
            chain.push(q);
        }
        chain.pop();
        chain
    };
    let mut lower = half(&mut p.iter().copied());
    let upper = half(&mut p.iter().rev().copied());
    lower.extend(upper);
    lower
}

/// Minimum-area enclosing rectangle: its corners and the lengths of its sides
/// from the first corner to the second and to the fourth. Like OpenCV, a set
/// of one point or of points on a line gives a rectangle of zero width.
pub(crate) fn min_area_rect(points: &[Pt]) -> (PxQuad, f64, f64) {
    let hull = convex_hull(points);
    match hull.as_slice() {
        [] => return ([[0.0; 2]; 4], 0.0, 0.0),
        [p] => return ([*p; 4], 0.0, 0.0),
        _ => {}
    }
    let mut best: Option<(f64, PxQuad, f64, f64)> = None;
    let n = hull.len();
    for i in 0..n {
        let (a, b) = (hull[i], hull[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy);
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
            let q = [
                corner(umin, vmin),
                corner(umax, vmin),
                corner(umax, vmax),
                corner(umin, vmax),
            ];
            best = Some((area, q, umax - umin, vmax - vmin));
        }
    }
    best.map_or(([hull[0]; 4], 0.0, 0.0), |(_, q, w, h)| (q, w, h))
}

/// `get_mini_boxes`: the minimum-area rectangle with its corners ordered
/// top-left, top-right, bottom-right, bottom-left as PaddleOCR picks them,
/// and its shorter side.
pub(crate) fn mini_box(points: &[Pt]) -> (PxQuad, f64) {
    let (q, w, h) = min_area_rect(points);
    (order_like_paddle(q), w.min(h))
}

fn order_like_paddle(q: PxQuad) -> PxQuad {
    let mut p = q;
    p.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let (i1, i4) = if p[1][1] > p[0][1] { (0, 1) } else { (1, 0) };
    let (i2, i3) = if p[3][1] > p[2][1] { (2, 3) } else { (3, 2) };
    [p[i1], p[i2], p[i3], p[i4]]
}

fn polygon_area(q: &[Pt]) -> f64 {
    let n = q.len();
    (0..n)
        .map(|i| q[i][0] * q[(i + 1) % n][1] - q[(i + 1) % n][0] * q[i][1])
        .sum::<f64>()
        / 2.0
}

/// pyclipper's arc tolerance for round joins.
const ARC_TOLERANCE: f64 = 0.25;

/// `unclip`: the quad offset outwards by `area × ratio / perimeter` with round
/// joins, as pyclipper does it (integer coordinates, arc tolerance 0.25).
pub(crate) fn unclip(q: &PxQuad, ratio: f64) -> Vec<Pt> {
    let area = polygon_area(q).abs();
    let perimeter: f64 = (0..4)
        .map(|i| {
            let (a, b) = (q[i], q[(i + 1) % 4]);
            (a[0] - b[0]).hypot(a[1] - b[1])
        })
        .sum();
    if perimeter == 0.0 {
        return q.to_vec();
    }
    let delta = area * ratio / perimeter;
    // pyclipper turns float input into integers by truncation.
    let p: Vec<Pt> = q.iter().map(|v| [v[0].trunc(), v[1].trunc()]).collect();
    let sign = if polygon_area(&p) >= 0.0 { 1.0 } else { -1.0 };
    let steps_per_rad = {
        let steps = std::f64::consts::PI / (1.0 - ARC_TOLERANCE / delta.abs()).acos();
        steps / std::f64::consts::TAU
    };
    let n = p.len();
    let normal = |a: Pt, b: Pt| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx.hypot(dy).max(1e-12);
        // Outward for a polygon of orientation `sign`, y downwards.
        [sign * dy / l, -sign * dx / l]
    };
    let mut out = Vec::new();
    for i in 0..n {
        let (prev, cur, next) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
        let (n1, n2) = (normal(prev, cur), normal(cur, next));
        let a1 = n1[1].atan2(n1[0]);
        // Sweep the short way round from n1 to n2.
        let mut d = n2[1].atan2(n2[0]) - a1;
        while d > std::f64::consts::PI {
            d -= std::f64::consts::TAU;
        }
        while d < -std::f64::consts::PI {
            d += std::f64::consts::TAU;
        }
        let steps = ((d.abs() * steps_per_rad).ceil() as usize).max(1);
        for s in 0..=steps {
            let a = a1 + d * s as f64 / steps as f64;
            out.push([
                (cur[0] + delta * a.cos()).round(),
                (cur[1] + delta * a.sin()).round(),
            ]);
        }
    }
    out
}

/// `box_score_fast`: the mean of `pred` (`w` × `h`) inside the quad, with the
/// corners truncated to integers as `fillPoly(box.astype(int32))` does.
pub(crate) fn quad_mean(pred: &[f32], w: usize, h: usize, q: &PxQuad) -> f64 {
    if w == 0 || h == 0 {
        return 0.0;
    }
    let clamp = |v: f64, hi: usize| (v as isize).clamp(0, hi as isize - 1) as usize;
    let b = quad_bounds(q);
    let (xmin, xmax) = (clamp(b[0].floor(), w), clamp(b[2].ceil(), w));
    let (ymin, ymax) = (clamp(b[1].floor(), h), clamp(b[3].ceil(), h));
    let poly: Vec<Pt> = q
        .iter()
        .map(|p| [(p[0] - xmin as f64).trunc(), (p[1] - ymin as f64).trunc()])
        .collect();
    let (mut sum, mut count) = (0.0_f64, 0_usize);
    for y in 0..=(ymax - ymin) {
        // The polygon's span on this row, boundary included.
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
        let x0 = lo.round().max(0.0) as usize;
        let x1 = (hi.round().max(0.0) as usize).min(xmax - xmin);
        let row = (ymin + y) * w + xmin;
        if x0 <= x1 {
            sum += pred[row + x0..=row + x1]
                .iter()
                .map(|v| f64::from(*v))
                .sum::<f64>();
            count += x1 - x0 + 1;
        }
    }
    if count == 0 { 0.0 } else { sum / count as f64 }
}

/// `[x0, y0, x1, y1]` of the quad.
pub(crate) fn quad_bounds(q: &[Pt]) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for p in q {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_axis_aligned_rectangle_is_its_own_box() {
        let pts = [[1.0, 1.0], [5.0, 1.0], [5.0, 3.0], [1.0, 3.0], [3.0, 2.0]];
        let (q, side) = mini_box(&pts);
        assert_eq!(side, 2.0);
        assert_eq!(q, [[1.0, 1.0], [5.0, 1.0], [5.0, 3.0], [1.0, 3.0]]);
    }

    #[test]
    fn unclip_grows_every_side() {
        let q = [[10.0, 10.0], [50.0, 10.0], [50.0, 20.0], [10.0, 20.0]];
        // Area 400, perimeter 100, ratio 1.5: 6 px.
        let (r, w, h) = min_area_rect(&unclip(&q, 1.5));
        assert_eq!((w.round(), h.round()), (52.0, 22.0));
        assert_eq!(quad_bounds(&r), [4.0, 4.0, 56.0, 26.0]);
    }

    #[test]
    fn the_mean_covers_the_quad_with_its_border() {
        let (w, h) = (6, 4);
        let mut pred = vec![0.0_f32; w * h];
        for y in 1..=2 {
            for x in 1..=3 {
                pred[y * w + x] = 1.0;
            }
        }
        let q = [[1.0, 1.0], [3.0, 1.0], [3.0, 2.0], [1.0, 2.0]];
        assert!((quad_mean(&pred, w, h, &q) - 1.0).abs() < 1e-9);
    }
}
