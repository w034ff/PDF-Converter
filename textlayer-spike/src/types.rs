//! The OCR result, copied from `ocr-spike/src/types.rs` on `spike/ocr-vertical`
//! (only the data types, with `Deserialize` added).
//!
//! Coordinates are points on the page as shown: origin top-left, y down.
//! Quads are in reading order of the text: horizontal text top-left,
//! top-right, bottom-right, bottom-left; vertical text top-right,
//! bottom-right, bottom-left, top-left.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RectPt {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

pub type QuadPt = [[f64; 2]; 4];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrPage {
    pub width_pt: f64,
    pub height_pt: f64,
    pub engine: String,
    pub blocks: Vec<OcrBlock>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrBlock {
    pub bbox: RectPt,
    pub orientation: Orientation,
    pub lines: Vec<OcrLine>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub quad: QuadPt,
    pub bbox: RectPt,
    pub orientation: Orientation,
    pub text: String,
    pub confidence: f32,
    pub words: Vec<OcrWord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrWord {
    pub quad: QuadPt,
    pub text: String,
    pub confidence: f32,
    pub chars: Vec<OcrChar>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrChar {
    pub quad: QuadPt,
    pub text: String,
    pub confidence: f32,
}

impl OcrPage {
    pub fn load(path: &std::path::Path) -> Self {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    pub fn lines(&self) -> impl Iterator<Item = &OcrLine> {
        self.blocks.iter().flat_map(|b| &b.lines)
    }

    /// Every character in reading order (spaces between words are not characters).
    pub fn chars(&self) -> impl Iterator<Item = &OcrChar> {
        self.lines().flat_map(|l| &l.words).flat_map(|w| &w.chars)
    }
}
