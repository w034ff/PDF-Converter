//! Error types for image probing and operations (design §6.6).

use std::fmt;

use crate::probe::MAX_IMAGE_PIXELS;

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
}

impl ProbeError {
    /// Returns the IPC error code name of design §6.6.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedFormat => "UnsupportedFormat",
            Self::DecodeFailed => "DecodeFailed",
            Self::TooLarge => "TooLarge",
            Self::ReadFailed => "ReadFailed",
        }
    }

    /// Returns the error detail string if applicable (design §6.6).
    ///
    /// For [`ProbeError::TooLarge`], returns the pixel limit formatted with commas
    /// (e.g. `"80,000,000"`). For other variants, returns `None`.
    #[must_use]
    pub fn detail(&self) -> Option<String> {
        match self {
            Self::TooLarge => Some(format_with_commas(MAX_IMAGE_PIXELS)),
            Self::UnsupportedFormat | Self::DecodeFailed | Self::ReadFailed => None,
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
        }
    }
}

impl std::error::Error for ProbeError {}

/// Error returned when writing/finishing a PDF document fails (design §4.3, §6.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfWriteError(pub(crate) String);

impl PdfWriteError {
    /// Returns the IPC error code name of design §6.6.
    #[must_use]
    pub fn code(&self) -> &'static str {
        "WriteFailed"
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
        assert_eq!(ProbeError::UnsupportedFormat.code(), "UnsupportedFormat");
        assert_eq!(ProbeError::UnsupportedFormat.detail(), None);

        assert_eq!(ProbeError::DecodeFailed.code(), "DecodeFailed");
        assert_eq!(ProbeError::DecodeFailed.detail(), None);

        assert_eq!(ProbeError::TooLarge.code(), "TooLarge");
        assert_eq!(ProbeError::TooLarge.detail().as_deref(), Some("80,000,000"));

        assert_eq!(ProbeError::ReadFailed.code(), "ReadFailed");
        assert_eq!(ProbeError::ReadFailed.detail(), None);
    }

    #[test]
    fn pdf_write_error_code_and_detail() {
        let err = PdfWriteError("unexpected error".to_string());
        assert_eq!(err.code(), "WriteFailed");
        assert_eq!(err.detail(), None);
        assert_eq!(err.to_string(), "failed to write PDF: unexpected error");
    }
}
