//! The IPC form of the error codes of design §6.6, and how core and worker errors become it.

use pdfconv_core::{ErrorCode, ProbeError};
use pdfconv_worker::protocol::{WorkerCode, WorkerOnlyCode};
use serde::Serialize;
use ts_rs::TS;

use crate::worker_pool::WorkerPoolError;

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

    /// A `WriteFailed` with `error` as its detail.
    pub fn write_failed(error: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::WriteFailed, error.to_string())
    }

    /// Turns the `{ code, detail }` a worker answered with into an IPC error.
    ///
    /// The worker's `PdfiumUnavailable` becomes `WorkerCrashed`, keeping the
    /// detail (design §6.6).
    pub fn from_worker(code: WorkerCode, detail: Option<&str>) -> Self {
        let code = match code {
            WorkerCode::Ipc(code) => code,
            WorkerCode::Worker(WorkerOnlyCode::PdfiumUnavailable) => ErrorCode::WorkerCrashed,
        };
        Self {
            code,
            detail: detail.map(str::to_owned),
        }
    }
}

impl From<ProbeError> for IpcError {
    fn from(err: ProbeError) -> Self {
        Self {
            code: err.code(),
            detail: err.detail(),
        }
    }
}

impl From<&WorkerPoolError> for IpcError {
    fn from(err: &WorkerPoolError) -> Self {
        match err {
            WorkerPoolError::WorkerCrashed => Self::from_code(ErrorCode::WorkerCrashed),
            WorkerPoolError::WorkerTimeout => Self::from_code(ErrorCode::WorkerTimeout),
            WorkerPoolError::Remote { code, detail } => Self::from_worker(*code, detail.as_deref()),
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
            Some(detail) => write!(f, "{}: {detail}", self.code),
            None => write!(f, "{}", self.code),
        }
    }
}

impl std::error::Error for IpcError {}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            IpcError::from(ProbeError::TooManyPages),
            IpcError::new(ErrorCode::TooManyPages, "1,000")
        );
        assert_eq!(
            IpcError::from(ProbeError::CmykNotAllowedInPdfA),
            IpcError::from_code(ErrorCode::CmykNotAllowedInPdfA)
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
                code: ErrorCode::TooManyPages.into(),
                detail: Some("10,000".into()),
            }),
            IpcError::new(ErrorCode::TooManyPages, "10,000")
        );
        assert_eq!(
            IpcError::from(WorkerPoolError::Remote {
                code: ErrorCode::PasswordProtected.into(),
                detail: None,
            }),
            IpcError::from_code(ErrorCode::PasswordProtected)
        );
    }

    #[test]
    fn pdfium_unavailable_is_a_crash() {
        let err = IpcError::from_worker(WorkerCode::PDFIUM_UNAVAILABLE, Some("library not found"));
        assert_eq!(err.code, ErrorCode::WorkerCrashed);
        assert_eq!(err.detail.as_deref(), Some("library not found"));
    }

    #[test]
    fn the_workers_layout_unavailable_keeps_its_code_and_detail() {
        assert_eq!(
            IpcError::from_worker(
                ErrorCode::LayoutUnavailable.into(),
                Some("the model is missing")
            ),
            IpcError::new(ErrorCode::LayoutUnavailable, "the model is missing")
        );
    }

    #[test]
    fn displays_the_code_and_the_detail() {
        assert_eq!(
            IpcError::new(ErrorCode::TooLarge, "80,000,000").to_string(),
            "TooLarge: 80,000,000"
        );
        assert_eq!(
            IpcError::from_code(ErrorCode::UnknownHandle).to_string(),
            "UnknownHandle"
        );
    }
}
