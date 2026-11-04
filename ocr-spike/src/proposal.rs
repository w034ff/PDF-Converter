//! Item 6: how OCR would enter the worker protocol
//! (`crates/worker/src/protocol.rs`), written here only as a proposal. The
//! variants are what would be added to the existing `Request`, `Response`
//! and `ErrorCode`; the rest of those enums is left out.

use serde::{Deserialize, Serialize};

use crate::types::OcrPage;

/// New `Request` variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RequestAddition {
    /// Recognises the text of page `page` (1-based) of the open PDF.
    /// The worker renders the page itself so that its long side is
    /// `OCR_RENDER_LONG_SIDE` px (the detector then works at 1600 px without
    /// another resize), like `Detect` does for layout.
    Ocr {
        page: u32,
        #[serde(default)]
        model: OcrModel,
    },
    /// The same for an image file being turned into a PDF (`CheckImage`'s
    /// sibling): one TIFF page at a time.
    OcrImage { path: std::path::PathBuf, page: u32, #[serde(default)] model: OcrModel },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OcrModel {
    /// PP-OCRv6 small: CER 3.9–4.3 % on XFUND ja, ~0.9 s a page on the survey PC.
    #[default]
    Small,
    /// PP-OCRv6 medium: CER 3.4–3.6 %, about twice as slow and 7× the download.
    Medium,
}

/// New `Response` variant. The page travels as JSON in the header; a dense
/// page (XFUND, ~200 lines and ~1000 characters with quads) is about 0.5 MB,
/// far below `MAX_HEADER_BYTES`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ResponseAddition {
    Ocr { page: OcrPage },
}

/// New `ErrorCode` variant (one line in `crates/core/src/error.rs`): the
/// models or onnxruntime could not be loaded or run. Kept apart from
/// `LayoutUnavailable` because the OCR models are a separate, optional
/// download and the screen should say which one is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCodeAddition {
    OcrUnavailable,
}

/// Long side of the rendering given to the detector, in pixels.
pub const OCR_RENDER_LONG_SIDE: u32 = 1600;

/// Upper bound on one `Ocr` request, first model load included, in seconds.
/// Measured: 1.4–2.5 s a dense A4 page on a 4-vCPU cloud VM with 3 threads
/// (small); about 3× that for medium. A slow 2-core laptop may be 3× slower
/// again, so 60 s (the same as `DETECT_TIMEOUT`) leaves room.
pub const OCR_TIMEOUT_SECS: u64 = 60;

/// Recognition batches are cut by total width rather than by count, so that
/// a page of wide lines cannot push the worker past `WORKER_MEMORY_LIMIT`
/// (one batch of six 3200-px lines is ~180 MB of output alone).
pub const OCR_REC_BATCH_MAX_LINES: usize = 6;
pub const OCR_REC_BATCH_MAX_WIDTH_SUM: usize = 6 * 1280;
