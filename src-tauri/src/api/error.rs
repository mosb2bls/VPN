//! Error types of the UI contract. The stable code catalogue lives in
//! `edge_core::error_code` (shared with the helper).

use serde::{Deserialize, Serialize};
use specta::Type;

pub use edge_core::ErrorCode;

use super::types::RestoreReport;

#[derive(Serialize, Deserialize, Type, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FieldError {
    /// Form field name exactly as in the form type (camelCase), e.g. `publicKey`.
    pub field: String,
    pub code: ErrorCode,
    pub message: String,
}

impl FieldError {
    pub fn new(field: &str, code: ErrorCode) -> Self {
        Self { field: field.into(), code, message: code.message().into() }
    }
}

/// Error returned by every fallible command and carried in events.
#[derive(Serialize, Deserialize, Type, Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{code:?}: {message}")]
pub struct AppError {
    pub code: ErrorCode,
    /// Display-ready English message.
    pub message: String,
    /// True if the app keeps working and the user can simply retry/continue.
    pub recoverable: bool,
    /// Field-level errors for `VALIDATION_FAILED`; empty otherwise.
    pub fields: Vec<FieldError>,
    /// Optional redacted technical detail for the logs/diagnostics view.
    pub detail: Option<String>,
    /// Per-item report, always present for `RESTORE_INCOMPLETE` (REQ-4).
    pub restore_report: Option<Box<RestoreReport>>,
}

impl AppError {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message: code.message().into(),
            recoverable: true,
            fields: Vec::new(),
            detail: None,
            restore_report: None,
        }
    }

    pub fn not_implemented(what: &str) -> Self {
        Self::new(ErrorCode::NotImplemented).with_detail(format!("{what}: not implemented (stub)"))
    }

    pub fn validation(fields: Vec<FieldError>) -> Self {
        Self { fields, ..Self::new(ErrorCode::ValidationFailed) }
    }

    /// `RESTORE_INCOMPLETE` with the report attached; needs user action.
    pub fn restore_incomplete(report: RestoreReport) -> Self {
        Self { restore_report: Some(Box::new(report)), ..Self::new(ErrorCode::RestoreIncomplete) }.fatal()
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn fatal(mut self) -> Self {
        self.recoverable = false;
        self
    }
}

pub type ApiResult<T> = Result<T, AppError>;
