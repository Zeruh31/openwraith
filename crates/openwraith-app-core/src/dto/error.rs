//! Errors of commands.

use std::fmt;

use serde::Serialize;

use super::common::{ForeignActivityView, KeyRef, TableRef};
use super::editors::EditProblemView;
use super::keys::KeymapProblemView;
use super::snapshot::NotEditable;
use super::status::{PlanInvalidation, WriteBlockReason};

/// The error of a command. Invariant: a command that fails with an
/// `AppError` wrote no setting to the keyboard (profile switches of a
/// restore comparison or an all-profile backup aside; see
/// [`ErrorDetail::ActiveProfileNow`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AppError {
    /// What kind of error.
    pub kind: AppErrorKind,
    /// Structured details for the message.
    pub detail: Option<ErrorDetail>,
    /// English technical text (shown under "Technical details").
    pub message: String,
}

impl AppError {
    /// An error without detail.
    pub fn new(kind: AppErrorKind, message: impl Into<String>) -> Self {
        AppError {
            kind,
            detail: None,
            message: message.into(),
        }
    }

    /// An error with detail.
    pub fn with(kind: AppErrorKind, detail: ErrorDetail, message: impl Into<String>) -> Self {
        AppError {
            kind,
            detail: Some(detail),
            message: message.into(),
        }
    }

    /// An internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        AppError::new(AppErrorKind::Internal, message)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

/// Kinds of errors: the library's kinds one to one, then the app's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum AppErrorKind {
    NotFound,
    /// The keyboard could not be opened (library `busy`).
    Busy,
    OtherProgramActive,
    Timeout,
    KeyboardReset,
    Disconnected,
    ProfileChanged,
    ChangedSincePlan,
    VerifyFailed,
    NotPersisted,
    ModelMismatch,
    FirmwareMismatch,
    InvalidBackup,
    Cancelled,
    Io,
    UnexpectedReply,
    InvalidRequest,
    NotConnected,
    /// Another operation is running (app admission).
    OperationRunning,
    StalePlan,
    NeedsSwitchConsent,
    WritesBlocked,
    SafetyDirUnavailable,
    FileUnknown,
    Internal,
    /// A live key-depth test is (or may be) open on the keyboard.
    TravelTestOpen,
}

/// Structured details of an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "detail", rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum ErrorDetail {
    OtherProgram {
        evidence: ForeignActivityView,
    },
    ProfileChanged {
        expected: Option<u8>,
        reported: u8,
    },
    ProfileChangedBeforeSave {
        profile: Option<u8>,
        reported: u8,
    },
    FirmwareMismatch {
        backup: Option<String>,
        keyboard: Option<String>,
    },
    ModelMismatch {
        backup: Option<String>,
        keyboard: Option<String>,
    },
    UnsupportedModel {
        product: Option<String>,
    },
    ProfileNotInBackup {
        profile: u8,
    },
    /// Where the keyboard is now (after profile switches).
    ActiveProfileNow {
        profile: Option<u8>,
    },
    KeyNotEditable {
        key: KeyRef,
        reason: NotEditable,
    },
    ValueOutOfRange {
        field: RangeField,
        value: u16,
        min: u16,
        max: u16,
    },
    InvalidIntent {
        field: IntentField,
    },
    DistanceUnit {
        unit_um: Option<u32>,
    },
    NeedsSwitchConsent {
        profiles: Vec<u8>,
    },
    WritesBlocked {
        reasons: Vec<WriteBlockReason>,
    },
    PlanInvalidated {
        reason: PlanInvalidation,
    },
    VerifyFailed {
        table: TableRef,
        sends: u8,
    },
    CommitTriggerIncomplete {
        packet: u16,
        of: u16,
    },
    Io {
        file_name: Option<String>,
    },
    /// The backup file was made by a newer OpenWraith (schema `found`; this
    /// version reads up to `supported`).
    UnsupportedSchema {
        found: u32,
        supported: u32,
    },
    /// A keymap-group edit (keys, macros, advanced keys) was refused.
    Keymap {
        problem: KeymapProblemView,
    },
    /// An edit of colours, dead band or SOCD, a preset, an export or an
    /// import was refused.
    Edit {
        problem: EditProblemView,
    },
}

/// A field whose value is out of range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum RangeField {
    Actuation,
    Sensitivity,
    Press,
    Release,
    Brightness,
    Speed,
}

/// A field of an intent that is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum IntentField {
    Keys,
    Knob,
    DuplicateKey,
    NothingToChange,
    Mode,
    Color,
    Profiles,
    Source,
}
