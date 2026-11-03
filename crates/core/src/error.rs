//! The error codes of design §6.6 and the error types of image probing and operations.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::probe::MAX_IMAGE_PIXELS;

/// Maximum pages allowed in a multi-page TIFF image (design §4.1.1).
pub const MAX_TIFF_PAGES: u32 = 1_000;

/// The error codes of design §6.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ErrorCode {
    UnsupportedFormat,
    DecodeFailed,
    PdfOpenFailed,
    PasswordProtected,
    TooLarge,
    TooManyPages,
    RenderTooLarge,
    WorkerCrashed,
    WorkerTimeout,
    ReadFailed,
    WriteFailed,
    InvalidPageRange,
    ConversionRunning,
    UnknownHandle,
    InvalidParams,
    OutputDirMissing,
    OutputDirNotWritable,
    CmykNotAllowedInPdfA,
    LayoutUnavailable,
    OutputTooLarge,
    MergeTooLarge,
}

impl fmt::Display for ErrorCode {
    /// Writes the name of the code in design §6.6, which is the variant name
    /// and so the string serde writes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Error returned when probing an image header fails (design §4.1, §6.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeError {
    /// Format is not supported or cannot be recognized from the header magic bytes.
    UnsupportedFormat,
    /// Header is corrupt, truncated, or failed to decode.
    DecodeFailed,
    /// Total image pixels exceed [`MAX_IMAGE_PIXELS`].
    TooLarge,
    /// Failed to open or read the image file.
    ReadFailed,
    /// The image is CMYK and the document is PDF/A (design §4.3).
    CmykNotAllowedInPdfA,
    /// Multi-page TIFF page count exceeds [`MAX_TIFF_PAGES`] (design §4.1.1).
    TooManyPages,
}

impl ProbeError {
    /// Returns the IPC error code of design §6.6.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::UnsupportedFormat => ErrorCode::UnsupportedFormat,
            Self::DecodeFailed => ErrorCode::DecodeFailed,
            Self::TooLarge => ErrorCode::TooLarge,
            Self::ReadFailed => ErrorCode::ReadFailed,
            Self::CmykNotAllowedInPdfA => ErrorCode::CmykNotAllowedInPdfA,
            Self::TooManyPages => ErrorCode::TooManyPages,
        }
    }

    /// Returns the error detail string if applicable (design §6.6).
    ///
    /// For [`ProbeError::TooLarge`], returns the pixel limit formatted with commas
    /// (e.g. `"80,000,000"`). For [`ProbeError::TooManyPages`], returns the page limit
    /// formatted with commas (e.g. `"1,000"`). For other variants, returns `None`.
    #[must_use]
    pub fn detail(&self) -> Option<String> {
        match self {
            Self::TooLarge => Some(format_with_commas(MAX_IMAGE_PIXELS)),
            Self::TooManyPages => Some(format_with_commas(u64::from(MAX_TIFF_PAGES))),
            Self::UnsupportedFormat
            | Self::DecodeFailed
            | Self::ReadFailed
            | Self::CmykNotAllowedInPdfA => None,
        }
    }
}

/// Formats an integer with comma thousands separators without external dependencies.
pub(crate) fn format_with_commas(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut s = String::new();
    let mut count = 0;
    while n > 0 {
        if count > 0 && count % 3 == 0 {
            s.push(',');
        }
        s.push(char::from(b'0' + (n % 10) as u8));
        n /= 10;
        count += 1;
    }
    s.chars().rev().collect()
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat => write!(f, "unsupported image format"),
            Self::DecodeFailed => write!(f, "failed to decode image header"),
            Self::TooLarge => write!(
                f,
                "image is too large (limit: {} pixels)",
                format_with_commas(MAX_IMAGE_PIXELS)
            ),
            Self::ReadFailed => write!(f, "failed to read image file"),
            Self::CmykNotAllowedInPdfA => write!(f, "CMYK images can't be used in PDF/A"),
            Self::TooManyPages => write!(
                f,
                "too many pages in image (limit: {} pages)",
                format_with_commas(u64::from(MAX_TIFF_PAGES))
            ),
        }
    }
}

impl std::error::Error for ProbeError {}

/// Error returned when writing/finishing a PDF document fails (design §4.3, §6.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfWriteError(pub(crate) String);

impl PdfWriteError {
    /// Returns the IPC error code of design §6.6.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        ErrorCode::WriteFailed
    }

    /// Returns the error detail string if applicable (design §6.6).
    #[must_use]
    pub fn detail(&self) -> Option<String> {
        None
    }
}

impl fmt::Display for PdfWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to write PDF: {}", self.0)
    }
}

impl std::error::Error for PdfWriteError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_with_commas_cases() {
        assert_eq!(format_with_commas(0), "0");
        assert_eq!(format_with_commas(7), "7");
        assert_eq!(format_with_commas(999), "999");
        assert_eq!(format_with_commas(1000), "1,000");
        assert_eq!(format_with_commas(1234567), "1,234,567");
        assert_eq!(format_with_commas(80_000_000), "80,000,000");
    }

    #[test]
    fn probe_error_code_and_detail() {
        assert_eq!(
            ProbeError::UnsupportedFormat.code(),
            ErrorCode::UnsupportedFormat
        );
        assert_eq!(ProbeError::UnsupportedFormat.detail(), None);

        assert_eq!(ProbeError::DecodeFailed.code(), ErrorCode::DecodeFailed);
        assert_eq!(ProbeError::DecodeFailed.detail(), None);

        assert_eq!(ProbeError::TooLarge.code(), ErrorCode::TooLarge);
        assert_eq!(ProbeError::TooLarge.detail().as_deref(), Some("80,000,000"));

        assert_eq!(ProbeError::ReadFailed.code(), ErrorCode::ReadFailed);
        assert_eq!(ProbeError::ReadFailed.detail(), None);

        assert_eq!(
            ProbeError::CmykNotAllowedInPdfA.code(),
            ErrorCode::CmykNotAllowedInPdfA
        );
        assert_eq!(ProbeError::CmykNotAllowedInPdfA.detail(), None);

        assert_eq!(ProbeError::TooManyPages.code(), ErrorCode::TooManyPages);
        assert_eq!(ProbeError::TooManyPages.detail().as_deref(), Some("1,000"));
    }

    #[test]
    fn display_is_the_string_serde_writes() {
        for code in [
            ErrorCode::UnsupportedFormat,
            ErrorCode::PdfOpenFailed,
            ErrorCode::CmykNotAllowedInPdfA,
            ErrorCode::MergeTooLarge,
        ] {
            let json = serde_json::to_string(&code).expect("serializing a code");
            assert_eq!(json, format!("\"{code}\""));
            let back: ErrorCode = serde_json::from_str(&json).expect("reading a code");
            assert_eq!(back, code);
        }
    }

    #[test]
    fn pdf_write_error_code_and_detail() {
        let err = PdfWriteError("unexpected error".to_string());
        assert_eq!(err.code(), ErrorCode::WriteFailed);
        assert_eq!(err.detail(), None);
        assert_eq!(err.to_string(), "failed to write PDF: unexpected error");
    }
}
