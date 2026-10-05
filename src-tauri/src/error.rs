//! Error codes and their serialization across IPC (design §6.6).

use pdfconv_core::ProbeError;
use serde::Serialize;
use ts_rs::TS;

use crate::worker_pool::WorkerPoolError;

/// The error codes of design §6.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
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
}

impl ErrorCode {
    /// Every code, so that [`ErrorCode::from_name`] cannot miss one.
    const ALL: [Self; 15] = [
        Self::UnsupportedFormat,
        Self::DecodeFailed,
        Self::PdfOpenFailed,
        Self::PasswordProtected,
        Self::TooLarge,
        Self::TooManyPages,
        Self::RenderTooLarge,
        Self::WorkerCrashed,
        Self::WorkerTimeout,
        Self::ReadFailed,
        Self::WriteFailed,
        Self::InvalidPageRange,
        Self::ConversionRunning,
        Self::UnknownHandle,
        Self::InvalidParams,
    ];

    /// The name of the code as it appears in design §6.6.
    pub fn name(self) -> &'static str {
        match self {
            Self::UnsupportedFormat => "UnsupportedFormat",
            Self::DecodeFailed => "DecodeFailed",
            Self::PdfOpenFailed => "PdfOpenFailed",
            Self::PasswordProtected => "PasswordProtected",
            Self::TooLarge => "TooLarge",
            Self::TooManyPages => "TooManyPages",
            Self::RenderTooLarge => "RenderTooLarge",
            Self::WorkerCrashed => "WorkerCrashed",
            Self::WorkerTimeout => "WorkerTimeout",
            Self::ReadFailed => "ReadFailed",
            Self::WriteFailed => "WriteFailed",
            Self::InvalidPageRange => "InvalidPageRange",
            Self::ConversionRunning => "ConversionRunning",
            Self::UnknownHandle => "UnknownHandle",
            Self::InvalidParams => "InvalidParams",
        }
    }

    /// The code named `name` in design §6.6, if there is one.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|code| code.name() == name)
    }
}

/// The error payload of a command or of an item in a list (design §6.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: ErrorCode,
    pub detail: Option<String>,
}

impl IpcError {
    /// An error with a detail string.
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
        }
    }

    /// An error without a detail.
    pub fn from_code(code: ErrorCode) -> Self {
        Self { code, detail: None }
    }

    /// Turns the `{ code, detail }` a worker answered with into an IPC error.
    ///
    /// The worker's `PdfiumUnavailable` becomes `WorkerCrashed` (design §6.6).
    /// A code that design §6.6 does not list also becomes `WorkerCrashed`, and
    /// the worker's own code goes into `detail` so it is not lost.
    pub fn from_worker(code: &str, detail: Option<&str>) -> Self {
        if code == WORKER_CODE_PDFIUM_UNAVAILABLE {
            return Self {
                code: ErrorCode::WorkerCrashed,
                detail: detail.map(str::to_owned),
            };
        }
        match ErrorCode::from_name(code) {
            Some(known) => Self {
                code: known,
                detail: detail.map(str::to_owned),
            },
            None => Self::new(
                ErrorCode::WorkerCrashed,
                match detail {
                    Some(detail) => format!("{code}: {detail}"),
                    None => code.to_owned(),
                },
            ),
        }
    }
}

/// The worker's code for a pdfium library that cannot be loaded; it is not an
/// IPC code (design §6.6).
const WORKER_CODE_PDFIUM_UNAVAILABLE: &str = "PdfiumUnavailable";

impl From<ProbeError> for IpcError {
    fn from(err: ProbeError) -> Self {
        let code = match err {
            ProbeError::UnsupportedFormat => ErrorCode::UnsupportedFormat,
            ProbeError::DecodeFailed => ErrorCode::DecodeFailed,
            ProbeError::TooLarge => ErrorCode::TooLarge,
            ProbeError::ReadFailed => ErrorCode::ReadFailed,
        };
        Self {
            code,
            detail: err.detail(),
        }
    }
}

impl From<&WorkerPoolError> for IpcError {
    fn from(err: &WorkerPoolError) -> Self {
        match err {
            WorkerPoolError::WorkerCrashed => Self::from_code(ErrorCode::WorkerCrashed),
            WorkerPoolError::WorkerTimeout => Self::from_code(ErrorCode::WorkerTimeout),
            WorkerPoolError::Remote { code, detail } => Self::from_worker(code, detail.as_deref()),
        }
    }
}

impl From<WorkerPoolError> for IpcError {
    fn from(err: WorkerPoolError) -> Self {
        Self::from(&err)
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.detail {
            Some(detail) => write!(f, "{}: {detail}", self.code.name()),
            None => f.write_str(self.code.name()),
        }
    }
}

impl std::error::Error for IpcError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_the_serialized_codes() {
        for code in ErrorCode::ALL {
            let json = serde_json::to_string(&code).expect("serializing a code");
            assert_eq!(json, format!("\"{}\"", code.name()));
            assert_eq!(ErrorCode::from_name(code.name()), Some(code));
        }
        assert_eq!(ErrorCode::from_name("PdfiumUnavailable"), None);
    }

    #[test]
    fn serializes_as_code_and_detail() {
        let json = serde_json::to_string(&IpcError::from_code(ErrorCode::UnknownHandle))
            .expect("serializing an error");
        assert_eq!(json, r#"{"code":"UnknownHandle","detail":null}"#);
        let json = serde_json::to_string(&IpcError::new(ErrorCode::TooLarge, "80,000,000"))
            .expect("serializing an error");
        assert_eq!(json, r#"{"code":"TooLarge","detail":"80,000,000"}"#);
    }

    #[test]
    fn maps_probe_errors() {
        assert_eq!(
            IpcError::from(ProbeError::DecodeFailed),
            IpcError::from_code(ErrorCode::DecodeFailed)
        );
        assert_eq!(
            IpcError::from(ProbeError::UnsupportedFormat),
            IpcError::from_code(ErrorCode::UnsupportedFormat)
        );
        assert_eq!(
            IpcError::from(ProbeError::ReadFailed),
            IpcError::from_code(ErrorCode::ReadFailed)
        );
        assert_eq!(
            IpcError::from(ProbeError::TooLarge),
            IpcError::new(ErrorCode::TooLarge, "80,000,000")
        );
    }

    #[test]
    fn maps_pool_errors() {
        assert_eq!(
            IpcError::from(WorkerPoolError::WorkerCrashed),
            IpcError::from_code(ErrorCode::WorkerCrashed)
        );
        assert_eq!(
            IpcError::from(WorkerPoolError::WorkerTimeout),
            IpcError::from_code(ErrorCode::WorkerTimeout)
        );
        assert_eq!(
            IpcError::from(WorkerPoolError::Remote {
                code: "TooManyPages".into(),
                detail: Some("10,000".into()),
            }),
            IpcError::new(ErrorCode::TooManyPages, "10,000")
        );
        assert_eq!(
            IpcError::from(WorkerPoolError::Remote {
                code: "PasswordProtected".into(),
                detail: None,
            }),
            IpcError::from_code(ErrorCode::PasswordProtected)
        );
    }

    #[test]
    fn pdfium_unavailable_is_a_crash() {
        let err = IpcError::from_worker("PdfiumUnavailable", Some("library not found"));
        assert_eq!(err.code, ErrorCode::WorkerCrashed);
        assert_eq!(err.detail.as_deref(), Some("library not found"));
    }

    #[test]
    fn an_unknown_worker_code_is_a_crash_that_keeps_the_code() {
        assert_eq!(
            IpcError::from_worker("Surprise", None),
            IpcError::new(ErrorCode::WorkerCrashed, "Surprise")
        );
        assert_eq!(
            IpcError::from_worker("Surprise", Some("more")),
            IpcError::new(ErrorCode::WorkerCrashed, "Surprise: more")
        );
    }
}
