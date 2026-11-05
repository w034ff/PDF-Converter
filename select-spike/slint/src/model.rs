//! The OCR page and the selection as a range of glyphs in reading order,
//! built from the glyphs' quads alone (the same design as web/src/core).

use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

pub type Pt = [f64; 2];
pub type Quad = [Pt; 4];

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub fn of_quad(q: &Quad) -> Rect {
        let f = |i: usize, pick: fn(f64, f64) -> f64, init: f64| q.iter().map(|p| p[i]).fold(init, pick);
        Rect { x0: f(0, f64::min, f64::MAX), y0: f(1, f64::min, f64::MAX), x1: f(0, f64::max, f64::MIN), y1: f(1, f64::max, f64::MIN) }
    }
    fn union(&self, o: &Rect) -> Rect {
        Rect { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }
    fn dist(&self, x: f64, y: f64) -> f64 {
        let dx = (self.x0 - x).max(0.0).max(x - self.x1);
        let dy = (self.y0 - y).max(0.0).max(y - self.y1);
        dx.hypot(dy)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrPage {
    pub width_pt: f64,
    pub height_pt: f64,
    pub blocks: Vec<OcrBlock>,
}
#[derive(Deserialize)]
pub struct OcrBlock {
    pub orientation: String,
    pub lines: Vec<OcrLine>,
}
#[derive(Deserialize)]
pub struct OcrLine {
    pub orientation: String,
    pub words: Vec<OcrWord>,
}
#[derive(Deserialize)]
pub struct OcrWord {
    pub text: String,
    pub chars: Vec<OcrChar>,
}
#[derive(Deserialize)]
pub struct OcrChar {
    pub quad: Quad,
    pub text: String,
}

impl OcrPage {
    /// The expected copy of the whole page.
    pub fn text(&self) -> String {
        let line = |l: &OcrLine| l.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
        let block = |b: &OcrBlock| b.lines.iter().map(line).collect::<Vec<_>>().join("\n");
        self.blocks.iter().map(block).collect::<Vec<_>>().join("\n\n")
    }
}

#[derive(Clone, Debug)]
pub struct Glyph {
    pub text: String,
    pub quad: Quad,
    pub bbox: Rect,
    /// Logical position, for the copied text.
    pub block: usize,
    pub line: usize,
    pub word: usize,
    /// Visual row (a line on the page, a column in the side panel), for hit tests and bands.
    pub vis: usize,
}

#[derive(Clone, Debug)]
pub struct VisLine {
    pub first: usize,
    pub end: usize,
    pub bbox: Rect,
    pub vertical: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Flat {
    pub glyphs: Vec<Glyph>,
    pub vis: Vec<VisLine>,
}

impl Flat {
    /// Groups glyphs (in reading order, `vis` set) into visual lines.
    pub fn new(glyphs: Vec<Glyph>, vertical: impl Fn(usize) -> bool) -> Flat {
        let mut vis: Vec<VisLine> = Vec::new();
        for (k, g) in glyphs.iter().enumerate() {
            match vis.last_mut() {
                Some(l) if glyphs[k - 1].vis == g.vis => {
                    l.end = k + 1;
                    l.bbox = l.bbox.union(&g.bbox);
                }
                _ => vis.push(VisLine { first: k, end: k + 1, bbox: g.bbox, vertical: vertical(g.vis) }),
            }
        }
        Flat { glyphs, vis }
    }

    pub fn from_page(page: &OcrPage) -> Flat {
        let mut glyphs = Vec::new();
        let mut vertical = Vec::new();
        let (mut line, mut word) = (0, 0);
        for (bi, b) in page.blocks.iter().enumerate() {
            for l in &b.lines {
                for w in &l.words {
                    for c in &w.chars {
                        glyphs.push(Glyph { text: c.text.clone(), quad: c.quad, bbox: Rect::of_quad(&c.quad), block: bi, line, word, vis: line });
                    }
                    word += 1;
                }
                vertical.push(l.orientation == "vertical");
                line += 1;
            }
        }
        Flat::new(glyphs, |v| vertical[v])
    }

    /// The text of glyphs [a, b): a space between words, a newline between lines, an empty line between blocks.
    pub fn text(&self, a: usize, b: usize) -> String {
        let mut s = String::new();
        for k in a..b {
            let g = &self.glyphs[k];
            if k > a {
                let p = &self.glyphs[k - 1];
                if p.block != g.block {
                    s.push_str("\n\n");
                } else if p.line != g.line {
                    s.push('\n');
                } else if p.word != g.word {
                    s.push(' ');
                }
            }
            s.push_str(&g.text);
        }
        s
    }

    /// The caret (0..=len, before glyph k) nearest to (x, y): the nearest visual
    /// line, then the glyph boundary along it. Points in gaps and margins go to
    /// the nearest line, so a drag never jumps to the start or end of the page.
    pub fn caret_at(&self, x: f64, y: f64) -> usize {
        let Some(l) = self.vis.iter().min_by(|a, b| a.bbox.dist(x, y).total_cmp(&b.bbox.dist(x, y))) else {
            return 0;
        };
        for k in l.first..l.end {
            let b = &self.glyphs[k].bbox;
            let (p, c) = if l.vertical { (y, (b.y0 + b.y1) / 2.0) } else { (x, (b.x0 + b.x1) / 2.0) };
            if p < c {
                return k;
            }
        }
        l.end
    }

    /// One rectangle per visual line covered by glyphs [a, b).
    pub fn bands(&self, a: usize, b: usize) -> Vec<Rect> {
        self.vis
            .iter()
            .filter_map(|l| {
                let (s, e) = (a.max(l.first), b.min(l.end));
                if s >= e {
                    return None;
                }
                let (f, t) = (&self.glyphs[s].quad, &self.glyphs[e - 1].quad);
                Some(Rect::of_quad(&[f[0], t[1], t[2], f[3]]))
            })
            .collect()
    }

    /// Finds `query`, ignoring spaces, width (NFKC) and case. Lines inside a
    /// block are joined without a separator, so a match may run from the end of
    /// one line into the next; blocks are never joined. Returns glyph ranges.
    pub fn search(&self, query: &str) -> Vec<(usize, usize)> {
        let norm = |s: &str| -> Vec<char> { s.nfkc().flat_map(char::to_lowercase).filter(|c| !c.is_whitespace()).collect() };
        let q = norm(query);
        if q.is_empty() {
            return Vec::new();
        }
        let (mut hay, mut at) = (Vec::new(), Vec::new());
        for (k, g) in self.glyphs.iter().enumerate() {
            if k > 0 && self.glyphs[k - 1].block != g.block {
                hay.push('\0');
                at.push(usize::MAX);
            }
            for c in norm(&g.text) {
                hay.push(c);
                at.push(k);
            }
        }
        let mut out = Vec::new();
        let mut i = 0;
        while i + q.len() <= hay.len() {
            if hay[i..i + q.len()] == q[..] {
                out.push((at[i], at[i + q.len() - 1] + 1));
                i += q.len();
            } else {
                i += 1;
            }
        }
        out
    }
}

/// Anchor and focus carets; the selection is [min, max).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sel {
    pub anchor: usize,
    pub focus: usize,
}

impl Sel {
    pub fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.focus), self.anchor.max(self.focus))
    }
}
