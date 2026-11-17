//! The result of OCR on one page (design ocr §2): page → block → line → word
//! → character. The worker builds it and sends it as JSON; the text layer is
//! written from it (design ocr §9).
//!
//! Coordinates are points on the page as shown (after `/Rotate`, origin at the
//! top-left corner, y downwards), the space of [`PageRect`].

use serde::{Deserialize, Serialize};

use crate::PageRect;

/// Four corners of a piece of text in points on the page as shown, from the
/// start of the reading direction: for horizontal text top-left, top-right,
/// bottom-right, bottom-left; for vertical text top-right, bottom-right,
/// bottom-left, top-left (design ocr §2). A skewed scan gives a slanted quad.
pub type Quad = [[f64; 2]; 4];

/// Which way a block or a line is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    /// Left to right, lines top to bottom.
    Horizontal,
    /// Top to bottom, columns right to left.
    Vertical,
}

/// The OCR result of one page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrPage {
    /// The page's shown width in points.
    pub width_pt: f64,
    /// The page's shown height in points.
    pub height_pt: f64,
    /// The models that read the page, so that a stored result can be told to
    /// be read again after the models change.
    pub engine: String,
    /// The blocks in reading order.
    pub blocks: Vec<OcrBlock>,
}

/// Lines read together: a paragraph, a heading, a tier of columns or a row of
/// a form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrBlock {
    pub bbox: PageRect,
    pub orientation: Orientation,
    /// The lines in reading order.
    pub lines: Vec<OcrLine>,
}

/// One detected line (a column for vertical text).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub quad: Quad,
    pub bbox: PageRect,
    pub orientation: Orientation,
    pub text: String,
    /// Mean probability of the line's characters, 0 to 1.
    pub confidence: f32,
    /// The runs of characters between spaces. Japanese without spaces is one
    /// word.
    pub words: Vec<OcrWord>,
}

/// Characters between spaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrWord {
    pub quad: Quad,
    pub text: String,
    /// Mean probability of the word's characters, 0 to 1.
    pub confidence: f32,
    pub chars: Vec<OcrChar>,
}

/// One character and the part of its line it covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrChar {
    pub quad: Quad,
    pub text: String,
    /// The recogniser's probability of the character, 0 to 1.
    pub confidence: f32,
}

/// The smallest rectangle holding `quad`.
#[must_use]
pub fn quad_bbox(quad: &Quad) -> PageRect {
    let mut rect = PageRect {
        x0: f64::INFINITY,
        y0: f64::INFINITY,
        x1: f64::NEG_INFINITY,
        y1: f64::NEG_INFINITY,
    };
    for [x, y] in quad {
        rect.x0 = rect.x0.min(*x);
        rect.y0 = rect.y0.min(*y);
        rect.x1 = rect.x1.max(*x);
        rect.y1 = rect.y1.max(*y);
    }
    rect
}

impl OcrPage {
    /// The page's text in reading order: lines end with a newline.
    #[must_use]
    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .flat_map(|block| &block.lines)
            .fold(String::new(), |mut text, line| {
                text.push_str(&line.text);
                text.push('\n');
                text
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbox_holds_a_slanted_quad() {
        let quad = [[10.0, 12.0], [50.0, 10.0], [51.0, 20.0], [11.0, 22.0]];
        assert_eq!(
            quad_bbox(&quad),
            PageRect {
                x0: 10.0,
                y0: 10.0,
                x1: 51.0,
                y1: 22.0
            }
        );
    }

    #[test]
    fn page_is_written_in_camel_case() {
        let page = OcrPage {
            width_pt: 100.0,
            height_pt: 200.0,
            engine: "e".into(),
            blocks: vec![OcrBlock {
                bbox: quad_bbox(&[[0.0; 2]; 4]),
                orientation: Orientation::Vertical,
                lines: Vec::new(),
            }],
        };
        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains(r#""widthPt":100.0"#), "{json}");
        assert!(json.contains(r#""orientation":"vertical""#), "{json}");
        assert_eq!(serde_json::from_str::<OcrPage>(&json).unwrap(), page);
    }
}
