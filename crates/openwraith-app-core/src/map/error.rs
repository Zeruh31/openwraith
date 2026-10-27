//! Library errors as [`AppError`]s, with structured detail. Every match is
//! exhaustive: a new library error or kind breaks the build here.

use openwraith_hid::{
    BackupError, ErrorInfo, ErrorKind, ListenError, ProfilesError, ProfilesFailure, RestoreError,
    SessionError, TransportError, WriteError,
};

use super::{foreign, sat8, sat16, table_ref};
use crate::dto::{AppError, AppErrorKind, ErrorDetail};

/// The app's kind of a library kind (one to one).
pub(crate) fn app_kind(kind: ErrorKind) -> AppErrorKind {
    match kind {
        ErrorKind::NotFound => AppErrorKind::NotFound,
        ErrorKind::Busy => AppErrorKind::Busy,
        ErrorKind::OtherProgramActive => AppErrorKind::OtherProgramActive,
        ErrorKind::Timeout => AppErrorKind::Timeout,
        ErrorKind::KeyboardReset => AppErrorKind::KeyboardReset,
        ErrorKind::Disconnected => AppErrorKind::Disconnected,
        ErrorKind::ProfileChanged => AppErrorKind::ProfileChanged,
        ErrorKind::ChangedSincePlan => AppErrorKind::ChangedSincePlan,
        ErrorKind::VerifyFailed => AppErrorKind::VerifyFailed,
        ErrorKind::NotPersisted => AppErrorKind::NotPersisted,
        ErrorKind::ModelMismatch => AppErrorKind::ModelMismatch,
        ErrorKind::FirmwareMismatch => AppErrorKind::FirmwareMismatch,
        ErrorKind::InvalidBackup => AppErrorKind::InvalidBackup,
        ErrorKind::Cancelled => AppErrorKind::Cancelled,
        ErrorKind::Io => AppErrorKind::Io,
        ErrorKind::UnexpectedReply => AppErrorKind::UnexpectedReply,
        ErrorKind::InvalidRequest => AppErrorKind::InvalidRequest,
        ErrorKind::TravelTestOpen => AppErrorKind::TravelTestOpen,
    }
}

fn make(kind: ErrorKind, detail: Option<ErrorDetail>, error: &dyn std::fmt::Display) -> AppError {
    AppError {
        kind: app_kind(kind),
        detail,
        message: error.to_string(),
    }
}

fn file_name(path: &std::path::Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

impl From<&ErrorInfo> for AppError {
    fn from(info: &ErrorInfo) -> Self {
        AppError {
            kind: app_kind(info.kind),
            detail: None,
            message: info.message.clone(),
        }
    }
}

impl From<&TransportError> for AppError {
    fn from(error: &TransportError) -> Self {
        let detail = match error {
            TransportError::Init(_)
            | TransportError::Open { .. }
            | TransportError::Write(_)
            | TransportError::Read(_)
            | TransportError::Malformed(_)
            | TransportError::Interrupted
            | TransportError::Refused(_) => None,
        };
        make(error.kind(), detail, error)
    }
}

impl From<&SessionError> for AppError {
    fn from(error: &SessionError) -> Self {
        match error {
            SessionError::Transport(e) => e.into(),
            SessionError::Timeout { .. }
            | SessionError::KeyboardReset { .. }
            | SessionError::Inconsistent { .. }
            | SessionError::Response(_)
            | SessionError::Decode { .. } => make(error.kind(), None, error),
        }
    }
}

impl From<&ListenError> for AppError {
    fn from(error: &ListenError) -> Self {
        match error {
            ListenError::Transport(e) => e.into(),
            ListenError::KeyboardReset { .. } => make(ErrorKind::KeyboardReset, None, error),
        }
    }
}

impl From<&BackupError> for AppError {
    fn from(error: &BackupError) -> Self {
        let detail = match error {
            BackupError::Session(e) => return e.into(),
            BackupError::UnsupportedModel { product, .. } => Some(ErrorDetail::UnsupportedModel {
                product: product.as_deref().map(super::clean),
            }),
            BackupError::ProfileChanged { before, after } => Some(ErrorDetail::ProfileChanged {
                expected: Some(*before),
                reported: *after,
            }),
            BackupError::AlreadyExists { path }
            | BackupError::Io { path, .. }
            | BackupError::SafetyCheckFailed { path } => Some(ErrorDetail::Io {
                file_name: file_name(path),
            }),
            BackupError::SafetyDirUnavailable { .. } => {
                return AppError::new(AppErrorKind::SafetyDirUnavailable, error.to_string());
            }
            BackupError::UnsupportedSchema { found, supported } => {
                Some(ErrorDetail::UnsupportedSchema {
                    found: *found,
                    supported: *supported,
                })
            }
            BackupError::Json(_)
            | BackupError::DuplicateEntry { .. }
            | BackupError::NotABackup { .. }
            | BackupError::Cancelled
            | BackupError::InvalidSnapshot { .. } => None,
            BackupError::TooLarge { path, .. } => Some(ErrorDetail::Io {
                file_name: file_name(path),
            }),
        };
        make(error.kind(), detail, error)
    }
}

impl From<&WriteError> for AppError {
    fn from(error: &WriteError) -> Self {
        let detail = match error {
            WriteError::Session(e) => return e.into(),
            WriteError::OtherProgramActive { evidence } => Some(ErrorDetail::OtherProgram {
                evidence: foreign(*evidence),
            }),
            WriteError::VerifyFailed { table, sends, .. } => Some(ErrorDetail::VerifyFailed {
                table: table_ref(*table),
                sends: sat8(*sends),
            }),
            WriteError::CommitTriggerIncomplete { packet, of, .. } => {
                Some(ErrorDetail::CommitTriggerIncomplete {
                    packet: sat16(*packet),
                    of: sat16(*of),
                })
            }
            WriteError::ProfileNotSelected { requested, active } => {
                Some(ErrorDetail::ProfileChanged {
                    expected: Some(requested.get()),
                    reported: *active,
                })
            }
            WriteError::ProfileChanged { expected, reported } => {
                Some(ErrorDetail::ProfileChanged {
                    expected: *expected,
                    reported: *reported,
                })
            }
            WriteError::ProfileChangedBeforeSave { profile, reported } => {
                Some(ErrorDetail::ProfileChangedBeforeSave {
                    profile: *profile,
                    reported: *reported,
                })
            }
            WriteError::UnsupportedModel { product } => Some(ErrorDetail::UnsupportedModel {
                product: product.as_deref().map(super::clean),
            }),
            WriteError::KeyboardReset { .. }
            | WriteError::Disconnected { .. }
            | WriteError::RepeatedWrite { .. }
            | WriteError::InvalidProfile { .. }
            | WriteError::Build(_)
            | WriteError::NotBackedUp { .. }
            | WriteError::EgressRefused { .. }
            | WriteError::TravelTestMaybeOpen
            | WriteError::TravelTestOpen { .. }
            | WriteError::TravelOpenNotAcked
            | WriteError::BaseKeymapUnsaved
            | WriteError::ConsentOnlyForSelects => None,
            WriteError::ChangedOnKeyboard { table } => Some(ErrorDetail::VerifyFailed {
                table: table_ref(*table),
                sends: 1,
            }),
        };
        make(error.kind(), detail, error)
    }
}

impl From<&ProfilesError> for AppError {
    fn from(error: &ProfilesError) -> Self {
        let inner: AppError = match &error.failure {
            ProfilesFailure::Backup(e) => e.into(),
            ProfilesFailure::Write(e) => e.into(),
            ProfilesFailure::UnknownActiveProfile { .. }
            | ProfilesFailure::ProductChanged { .. }
            | ProfilesFailure::Cancelled => make(error.kind(), None, error),
        };
        // Where the keyboard is now matters most after profile switches.
        AppError {
            detail: Some(ErrorDetail::ActiveProfileNow {
                profile: error.active_profile_now,
            }),
            ..inner
        }
    }
}

impl From<&RestoreError> for AppError {
    fn from(error: &RestoreError) -> Self {
        let detail = match error {
            RestoreError::Profiles(e) => return e.into(),
            RestoreError::Backup(e) => return e.into(),
            RestoreError::ModelMismatch { backup, keyboard } => Some(ErrorDetail::ModelMismatch {
                backup: backup.as_deref().map(super::clean),
                keyboard: keyboard.as_deref().map(super::clean),
            }),
            RestoreError::FirmwareMismatch { backup, keyboard } => {
                Some(ErrorDetail::FirmwareMismatch {
                    backup: backup.as_deref().map(super::clean),
                    keyboard: keyboard.as_deref().map(super::clean),
                })
            }
            RestoreError::ProfileNotInBackup { profile } => {
                Some(ErrorDetail::ProfileNotInBackup { profile: *profile })
            }
            RestoreError::InvalidBackup { .. }
            | RestoreError::SafetyBackupMismatch { .. }
            | RestoreError::InvalidRequest { .. }
            | RestoreError::Cancelled => None,
        };
        make(error.kind(), detail, error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwraith_hid::ForeignActivity;

    #[test]
    fn write_errors_carry_detail() {
        let error = WriteError::OtherProgramActive {
            evidence: ForeignActivity {
                status_reports: 2,
                foreign_acks: 0,
                profile_reports: 0,
            },
        };
        let app = AppError::from(&error);
        assert_eq!(app.kind, AppErrorKind::OtherProgramActive);
        assert!(matches!(
            app.detail,
            Some(ErrorDetail::OtherProgram { evidence }) if evidence.status_reports == 2
        ));
        let busy = AppError::from(&TransportError::Open {
            path: "x".into(),
            message: "denied".into(),
        });
        assert_eq!(busy.kind, AppErrorKind::Busy);
    }

    #[test]
    fn profiles_errors_say_where_the_keyboard_is() {
        let error = ProfilesError {
            failure: ProfilesFailure::Cancelled,
            active_profile_now: Some(2),
        };
        let app = AppError::from(&error);
        assert_eq!(app.kind, AppErrorKind::Cancelled);
        assert_eq!(
            app.detail,
            Some(ErrorDetail::ActiveProfileNow { profile: Some(2) })
        );
    }
}
