//! The side panel: the chosen vertical block laid out with vtext (one Text
//! per glyph), and the same glyph-range selection over the panel's cells.

use crate::model::{Flat, Glyph, Rect};
use crate::vtext;
use crate::PanelGlyph;

pub const WIDTH: f64 = 300.0;
pub const HEIGHT: f64 = 600.0;
pub const FONT: f64 = 18.0;
const GAP: f64 = 0.6;
const ROW: f64 = FONT * 1.05;

/// Lays out the glyphs of `block` (one OCR line per column, wrapped at the panel's height).
pub fn build(page: &Flat, block: usize) -> (Flat, Vec<PanelGlyph>) {
    let src: Vec<&Glyph> = page.glyphs.iter().filter(|g| g.block == block).collect();
    let mut text = String::new();
    for (k, g) in src.iter().enumerate() {
        if k > 0 && src[k - 1].line != g.line {
            text.push('\n');
        }
        text.push_str(&g.text);
    }
    let rows = (HEIGHT / ROW) as usize - 1;
    let laid = vtext::layout(&text, rows);
    let mut glyphs = Vec::new();
    let mut shown = Vec::new();
    for (v, g) in laid.iter().zip(&src) {
        let x = WIDTH - (v.col + 1) as f64 * FONT * (1.0 + GAP);
        let y = v.row as f64 * ROW;
        // A vertical cell: top-right, bottom-right, bottom-left, top-left.
        let quad = [[x + FONT, y], [x + FONT, y + ROW], [x, y + ROW], [x, y]];
        glyphs.push(Glyph { text: g.text.clone(), quad, bbox: Rect::of_quad(&quad), block: 0, line: g.line, word: g.word, vis: v.col });
        shown.push(PanelGlyph { ch: v.ch.clone().into(), x: x as f32, y: y as f32, rotate: v.rotate, shift: v.shift });
    }
    (Flat::new(glyphs, |_| true), shown)
}
