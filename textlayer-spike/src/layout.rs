//! Item 5: from OCR quads to runs of invisible text.
//!
//! Everything here is in page points as shown (origin top-left, y down), like
//! the OCR result and krilla's surface; the lopdf writer flips y.
//!
//! A run is either horizontal text on a baseline, or a column in vertical
//! writing mode (`Identity-V`, lopdf only). Vertical OCR lines can also be
//! turned into horizontal runs: every character upright on its own
//! (`UprightChars`, what krilla can do), or the whole column as one line
//! turned 90° clockwise (`Rot90`, glyphs on their side).

use crate::types::{OcrLine, OcrPage, Orientation, QuadPt};

/// Fraction of the em below the baseline (Noto Sans JP's ideographic
/// descent; the glyphless font uses the same 880 / −120 split).
pub const DESCENT: f64 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// One run per line, stretched to the line's length (`Tz` horizontally,
    /// `Tc` in vertical writing mode).
    Line,
    /// As `Line`, and each character moved to the start of its OCR cell
    /// (`TJ` adjustments; horizontal runs only).
    LineTj,
    /// One run per character, placed in the character's quad.
    Char,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VMode {
    IdentityV,
    UprightChars,
    Rot90,
}

#[derive(Clone, Debug)]
pub struct Run {
    pub text: Vec<char>,
    pub vertical: bool,
    /// Horizontal: start of the baseline. Vertical: top centre of the first em.
    pub origin: [f64; 2],
    /// Angle of the text's x axis (y down, so positive turns clockwise on screen).
    pub angle: f64,
    pub size: f64,
    /// Wanted length along the writing direction.
    pub length: f64,
    /// Horizontal runs in `Fit::Char` for krilla: where each character starts
    /// along the baseline, relative to `origin`.
    pub starts: Option<Vec<f64>>,
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn len(v: [f64; 2]) -> f64 {
    v[0].hypot(v[1])
}

fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// A horizontal run from a quad in horizontal order (tl, tr, br, bl).
fn horizontal(text: Vec<char>, q: &QuadPt) -> Run {
    let (tl, tr, br, bl) = (q[0], q[1], q[2], q[3]);
    let left_h = len(sub(bl, tl));
    let right_h = len(sub(br, tr));
    let h = (left_h + right_h) / 2.0;
    let top = sub(tr, tl);
    let bottom = sub(br, bl);
    let length = (len(top) + len(bottom)) / 2.0;
    let angle = (top[1] + bottom[1]).atan2(top[0] + bottom[0]);
    // The baseline sits DESCENT of the height above the bottom edge.
    let origin = lerp(bl, tl, DESCENT);
    Run { text, vertical: false, origin, angle, size: h, length, starts: None }
}

/// A vertical-writing run from a quad in vertical order (tr, br, bl, tl).
fn vertical(text: Vec<char>, q: &QuadPt) -> Run {
    let (tr, br, bl, tl) = (q[0], q[1], q[2], q[3]);
    let across_top = sub(tr, tl);
    let across_bottom = sub(br, bl);
    let w = (len(across_top) + len(across_bottom)) / 2.0;
    let length = (len(sub(br, tr)) + len(sub(bl, tl))) / 2.0;
    let angle = (across_top[1] + across_bottom[1]).atan2(across_top[0] + across_bottom[0]);
    Run { text, vertical: true, origin: lerp(tl, tr, 0.5), angle, size: w, length, starts: None }
}

/// A vertical quad (tr, br, bl, tl) seen as an upright horizontal box.
fn upright(q: &QuadPt) -> QuadPt {
    [q[3], q[0], q[1], q[2]]
}

/// A vertical quad as a horizontal line turned 90° clockwise.
fn turned(q: &QuadPt) -> QuadPt {
    *q
}

fn line_chars(l: &OcrLine) -> impl Iterator<Item = (char, &QuadPt)> {
    l.words.iter().flat_map(|w| &w.chars).flat_map(|c| c.text.chars().map(move |ch| (ch, &c.quad)))
}

/// The line's characters, words joined by one space (the OCR characters, so
/// that the layer holds exactly what the checker compares with).
fn line_text(l: &OcrLine) -> Vec<char> {
    let mut t = Vec::new();
    for (k, w) in l.words.iter().enumerate() {
        if k > 0 {
            t.push(' ');
        }
        t.extend(w.chars.iter().flat_map(|c| c.text.chars()));
    }
    t
}

/// Projection of `p` on the run's baseline direction, relative to its origin.
fn along(run: &Run, p: [f64; 2]) -> f64 {
    let d = sub(p, run.origin);
    d[0] * run.angle.cos() + d[1] * run.angle.sin()
}

pub fn runs(page: &OcrPage, fit: Fit, vmode: VMode, min_confidence: f32) -> Vec<Run> {
    let mut out = Vec::new();
    for l in page.lines() {
        if l.confidence < min_confidence {
            continue;
        }
        let is_v = l.orientation == Orientation::Vertical;
        match (is_v, vmode, fit) {
            (false, _, Fit::LineTj) => {
                let mut r = horizontal(line_text(l), &l.quad);
                // Where each character starts, for krilla's per-glyph advances;
                // a space between words starts where the previous word ends.
                let mut starts = Vec::new();
                for (k, w) in l.words.iter().enumerate() {
                    if k > 0 {
                        let prev = &l.words[k - 1].quad;
                        starts.push(along(&r, prev[1]).max(0.0));
                    }
                    for c in &w.chars {
                        for _ in c.text.chars() {
                            starts.push(along(&r, c.quad[0]).max(0.0));
                        }
                    }
                }
                r.starts = Some(starts);
                out.push(r);
            }
            (false, _, Fit::Line) => out.push(horizontal(line_text(l), &l.quad)),
            (false, _, Fit::Char) => {
                for (ch, q) in line_chars(l) {
                    out.push(horizontal(vec![ch], q));
                }
            }
            (true, VMode::IdentityV, Fit::Line | Fit::LineTj) => out.push(vertical(line_text(l), &l.quad)),
            (true, VMode::IdentityV, Fit::Char) => {
                for (ch, q) in line_chars(l) {
                    out.push(vertical(vec![ch], q));
                }
            }
            (true, VMode::UprightChars, _) => {
                for (ch, q) in line_chars(l) {
                    out.push(horizontal(vec![ch], &upright(q)));
                }
            }
            (true, VMode::Rot90, Fit::Line | Fit::LineTj) => out.push(horizontal(line_text(l), &turned(&l.quad))),
            (true, VMode::Rot90, Fit::Char) => {
                for (ch, q) in line_chars(l) {
                    out.push(horizontal(vec![ch], &turned(q)));
                }
            }
        }
    }
    out
}
