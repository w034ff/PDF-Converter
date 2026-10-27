//! Choices of the settings panel and what a PDF is written as, per design §6.2, §6.3, §6.9, §7.1.

use pdfconv_core::{OutputPassword, PdfConformance, encrypt_pdf};
use pdfconv_worker::protocol::RenderFormat;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ErrorCode, IpcError};

/// Page size choice for image-to-PDF conversion (design §6.2, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PageSizeChoice {
    Fit,
    A3,
    A4,
    A5,
    JisB4,
    JisB5,
    Letter,
    Legal,
}

/// The PDF standard `pdfA` of the commands and settings selects (design §6.2, §7.1).
pub fn pdf_conformance(pdf_a: bool) -> PdfConformance {
    if pdf_a {
        PdfConformance::PdfA2b
    } else {
        PdfConformance::Standard
    }
}

/// What a job writes: the PDF standard and the password of the PDFs, if any. A password is
/// never combined with PDF/A ([`output_password`]).
#[derive(Debug, Clone)]
pub struct PdfOutput {
    pub conformance: PdfConformance,
    pub password: Option<OutputPassword>,
}

impl PdfOutput {
    #[must_use]
    pub fn new(conformance: PdfConformance, password: Option<OutputPassword>) -> Self {
        Self {
            conformance,
            password,
        }
    }
}

/// Checks the password a command received against the PDF standard it selects (design §6.9,
/// §7.1). PDF/A forbids encryption, and a password outside the rule of design §4.3.1 would not
/// open the PDF in every viewer. The screen checks both first, so an error here is not shown.
pub fn output_password(
    conformance: PdfConformance,
    password: Option<String>,
) -> Result<Option<OutputPassword>, IpcError> {
    let Some(password) = password else {
        return Ok(None);
    };
    if conformance == PdfConformance::PdfA2b {
        return Err(IpcError::from_code(ErrorCode::InvalidParams));
    }
    OutputPassword::new(password)
        .map(Some)
        .map_err(|_| IpcError::from_code(ErrorCode::InvalidParams))
}

/// Encrypts a written PDF when the job has a password. Encryption is neither counted in the
/// progress nor interrupted by a cancel: a PDF takes about a second even when it is large
/// (design §6.2).
pub(super) fn protect_pdf(
    pdf: Vec<u8>,
    password: Option<&OutputPassword>,
) -> Result<Vec<u8>, IpcError> {
    match password {
        Some(password) => encrypt_pdf(pdf, password)
            .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string())),
        None => Ok(pdf),
    }
}

impl PageSizeChoice {
    /// Converts this choice and an orientation into a [`pdfconv_core::PageSize`] (design §7.1).
    ///
    /// When `self` is [`PageSizeChoice::Fit`], `orientation` is ignored.
    pub fn to_core(self, orientation: PageOrientationChoice) -> pdfconv_core::PageSize {
        let orientation = orientation.into();
        match self {
            Self::Fit => pdfconv_core::PageSize::Fit,
            Self::A3 => pdfconv_core::PageSize::Paper(pdfconv_core::PaperSize::A3, orientation),
            Self::A4 => pdfconv_core::PageSize::Paper(pdfconv_core::PaperSize::A4, orientation),
            Self::A5 => pdfconv_core::PageSize::Paper(pdfconv_core::PaperSize::A5, orientation),
            Self::JisB4 => {
                pdfconv_core::PageSize::Paper(pdfconv_core::PaperSize::JisB4, orientation)
            }
            Self::JisB5 => {
                pdfconv_core::PageSize::Paper(pdfconv_core::PaperSize::JisB5, orientation)
            }
            Self::Letter => {
                pdfconv_core::PageSize::Paper(pdfconv_core::PaperSize::Letter, orientation)
            }
            Self::Legal => {
                pdfconv_core::PageSize::Paper(pdfconv_core::PaperSize::Legal, orientation)
            }
        }
    }
}

/// Orientation choice for a paper page (design §6.7, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PageOrientationChoice {
    Auto,
    Portrait,
    Landscape,
}

impl From<PageOrientationChoice> for pdfconv_core::layout::PageOrientation {
    fn from(choice: PageOrientationChoice) -> Self {
        match choice {
            PageOrientationChoice::Auto => Self::Auto,
            PageOrientationChoice::Portrait => Self::Portrait,
            PageOrientationChoice::Landscape => Self::Landscape,
        }
    }
}

/// Image format choice for PDF-to-image conversion (design §6.3, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RenderFormatChoice {
    Png,
    Jpeg,
    Webp,
}

impl RenderFormatChoice {
    /// File extension for output files without the dot (design §6.3).
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }
}

impl From<RenderFormatChoice> for RenderFormat {
    fn from(choice: RenderFormatChoice) -> Self {
        match choice {
            RenderFormatChoice::Png => Self::Png,
            RenderFormatChoice::Jpeg => Self::Jpeg,
            RenderFormatChoice::Webp => Self::Webp,
        }
    }
}

/// Color mode choice for PDF-to-image conversion (design §6.3, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RenderColorChoice {
    Color,
    Gray,
}

impl From<RenderColorChoice> for pdfconv_worker::protocol::RenderColor {
    fn from(choice: RenderColorChoice) -> Self {
        match choice {
            RenderColorChoice::Color => Self::Color,
            RenderColorChoice::Gray => Self::Gray,
        }
    }
}
