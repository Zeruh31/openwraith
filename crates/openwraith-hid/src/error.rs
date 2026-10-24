//! Error types of the HID crate, and [`ErrorKind`], their serialisable
//! classification for a GUI.

use std::fmt;
use std::path::PathBuf;

use openwraith_protocol::{
    DecodeError, FrameError, Mismatch, ProfileId, ReadRequest, ResponseError, TableId,
    WriteBuildError,
};
use serde::Serialize;

use crate::session::ForeignActivity;

/// Machine-readable class of an error, for command results of a GUI
/// (serialised in snake_case).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// No supported keyboard is connected.
    NotFound,
    /// The keyboard could not be opened (held by another program?).
    Busy,
    /// Another program (such as the vendor web configurator) is talking to
    /// the keyboard.
    OtherProgramActive,
    /// The keyboard did not answer in time.
    Timeout,
    /// The keyboard reported a reset (Fn + Space held).
    KeyboardReset,
    /// The connection was lost (keyboard unplugged?).
    Disconnected,
    /// The active onboard profile changed unexpectedly.
    ProfileChanged,
    /// The keyboard's settings changed between planning and writing.
    ChangedSincePlan,
    /// A write could not be verified by reading it back.
    VerifyFailed,
    /// The keyboard is another model than expected.
    ModelMismatch,
    /// The keyboard runs other firmware than the backup was made with.
    FirmwareMismatch,
    /// A backup file is invalid or cannot be used for this operation.
    InvalidBackup,
    /// The user cancelled the operation.
    Cancelled,
    /// Reading or writing a file failed.
    Io,
    /// The keyboard answered in an unexpected way.
    UnexpectedReply,
    /// The requested change is not valid (out of range, unknown key, ...).
    InvalidRequest,
}

/// An error flattened to its [`ErrorKind`] and message (serialisable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorInfo {
    /// Machine-readable class.
    pub kind: ErrorKind,
    /// Human-readable message (the error's `Display`).
    pub message: String,
}

impl ErrorInfo {
    /// The kind and message of an error.
    pub fn new(kind: ErrorKind, error: &dyn fmt::Display) -> Self {
        ErrorInfo {
            kind,
            message: error.to_string(),
        }
    }
}

/// A transport could not exchange reports with the device.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The HID subsystem could not be initialised or could not list devices.
    #[error("cannot access the HID subsystem: {0}")]
    Init(String),
    /// A device was found but could not be opened.
    #[error("cannot open the keyboard at {path}: {message}")]
    Open {
        /// Platform path of the HID interface.
        path: String,
        /// Message from the HID library.
        message: String,
    },
    /// Sending an output report failed (for example because the keyboard was
    /// unplugged).
    #[error("writing to the keyboard failed: {0}")]
    Write(String),
    /// Reading an input report failed (for example because the keyboard was
    /// unplugged).
    #[error("reading from the keyboard failed: {0}")]
    Read(String),
    /// The device sent something that is not a 63-byte report with ID 1.
    /// Sessions skip such reports.
    #[error("malformed input report: {0}")]
    Malformed(#[from] FrameError),
    /// A read was interrupted by a signal (for example Ctrl+C on Linux,
    /// where `poll` is never restarted). Sessions retry it with the remaining
    /// time; it does not mean the connection was lost.
    #[error("reading from the keyboard was interrupted by a signal")]
    Interrupted,
}

impl TransportError {
    /// The machine-readable class of this error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            TransportError::Init(_) => ErrorKind::Io,
            TransportError::Open { .. } => ErrorKind::Busy,
            TransportError::Write(_) | TransportError::Read(_) => ErrorKind::Disconnected,
            TransportError::Malformed(_) => ErrorKind::UnexpectedReply,
            TransportError::Interrupted => ErrorKind::Timeout,
        }
    }
}

/// A read request could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The transport failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The keyboard did not send a complete reply in time, even after the
    /// retries allowed by the session options.
    #[error(
        "the keyboard did not answer {request} in time \
         ({received} of {expected} chunk(s) after {attempts} attempt(s))"
    )]
    Timeout {
        /// The request that went unanswered.
        request: ReadRequest,
        /// How many times the request was sent.
        attempts: u32,
        /// Chunks received.
        received: usize,
        /// Chunks expected.
        expected: u16,
    },
    /// The keyboard sent its reset notification (`14 .. 01 01`, spec section
    /// 4.6.3), for example because Fn + Space was held. The session did not
    /// answer it and stopped; values read before it may be stale.
    #[error(
        "the keyboard reported a reset (input report {report}) while reading {request}; \
         nothing was sent in response"
    )]
    KeyboardReset {
        /// The request that was being processed when the report arrived.
        request: ReadRequest,
        /// Hex dump of the reset report.
        report: String,
    },
    /// Two copies of the same reply chunk with different contents arrived,
    /// on the last attempt: the keyboard's settings changed while they were
    /// being read (for example because another program wrote them). The
    /// session returns nothing rather than a reply stitched together from two
    /// different states.
    #[error(
        "the keyboard's data for {request} changed while it was being read \
         (two different copies of the same chunk arrived)"
    )]
    Inconsistent {
        /// The request whose reply was inconsistent.
        request: ReadRequest,
    },
    /// The collected reports do not form a valid reply.
    #[error(transparent)]
    Response(#[from] ResponseError),
    /// A complete reply could not be decoded into the requested model.
    #[error("cannot decode the reply to {request}: {source}")]
    Decode {
        /// The request whose reply failed to decode.
        request: ReadRequest,
        /// The decoder error.
        #[source]
        source: DecodeError,
    },
}

impl SessionError {
    /// The machine-readable class of this error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            SessionError::Transport(e) => e.kind(),
            SessionError::Timeout { .. } => ErrorKind::Timeout,
            SessionError::KeyboardReset { .. } => ErrorKind::KeyboardReset,
            SessionError::Inconsistent { .. } => ErrorKind::OtherProgramActive,
            SessionError::Response(_) | SessionError::Decode { .. } => ErrorKind::UnexpectedReply,
        }
    }
}

/// A backup could not be made, saved or loaded.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    /// Reading from the keyboard failed.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// The device-info string names a product this crate does not know.
    #[error(
        "unsupported keyboard: the device reports product {product:?} (device info {info:?}); \
         only the Wraith W75 (W75HEARGB) is supported"
    )]
    UnsupportedModel {
        /// Product code from field 4 of the device-info string, if any.
        product: Option<String>,
        /// The whole device-info string.
        info: String,
    },
    /// The active onboard profile changed while the backup was being read,
    /// so the tables may come from two different profiles.
    #[error(
        "the active onboard profile changed during the backup (profile {before} -> {after}); \
         run the backup again"
    )]
    ProfileChanged {
        /// Profile at the start of the backup.
        before: u8,
        /// Profile seen later.
        after: u8,
    },
    /// The file is not valid JSON or does not have the backup structure.
    #[error("invalid backup file: {0}")]
    Json(#[from] serde_json::Error),
    /// The backup file stores more than one reply to the same request, so it
    /// is not clear which one is the keyboard's state.
    #[error("invalid backup file: the reply to {request} is stored more than once")]
    DuplicateEntry {
        /// The request that appears twice.
        request: ReadRequest,
    },
    /// The JSON is not an OpenWraith backup.
    #[error("not an OpenWraith backup (format {found:?}, expected {expected:?})")]
    NotABackup {
        /// The `format` field found, if any.
        found: Option<String>,
        /// The expected `format` value.
        expected: &'static str,
    },
    /// The backup was written by a newer version with an unknown schema.
    #[error(
        "backup schema version {found} is newer than this version of OpenWraith supports \
         ({supported}); please update"
    )]
    UnsupportedSchema {
        /// Schema version in the file.
        found: u32,
        /// Highest version this crate reads.
        supported: u32,
    },
    /// The target file exists and overwriting was not allowed.
    #[error("{} already exists (choose another name or allow overwriting)", path.display())]
    AlreadyExists {
        /// The existing file.
        path: PathBuf,
    },
    /// Reading or writing a file failed.
    #[error("cannot {action} {}: {source}", path.display())]
    Io {
        /// `"read"` or `"write"`.
        action: &'static str,
        /// The file.
        path: PathBuf,
        /// The I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The progress callback asked to stop. Nothing was saved.
    #[error("the backup was cancelled")]
    Cancelled,
    /// A schema-2 (all profiles) backup has an invalid profile snapshot, or a
    /// schema-1 backup carries profile snapshots.
    #[error("invalid backup file: {reason}")]
    InvalidSnapshot {
        /// The snapshot's profile, if known.
        profile: Option<u8>,
        /// What is wrong.
        reason: String,
    },
    /// No directory for safety backups could be determined. Nothing was
    /// written to the keyboard.
    #[error("cannot determine a directory for safety backups: {reason}")]
    SafetyDirUnavailable {
        /// Why.
        reason: String,
    },
    /// A safety backup was saved, but reading it back did not give the same
    /// backup. Nothing was written to the keyboard.
    #[error("the safety backup {} could not be verified after saving", path.display())]
    SafetyCheckFailed {
        /// The saved file.
        path: PathBuf,
    },
}

impl BackupError {
    /// The machine-readable class of this error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            BackupError::Session(e) => e.kind(),
            BackupError::UnsupportedModel { .. } => ErrorKind::ModelMismatch,
            BackupError::ProfileChanged { .. } => ErrorKind::ProfileChanged,
            BackupError::Json(_)
            | BackupError::DuplicateEntry { .. }
            | BackupError::NotABackup { .. }
            | BackupError::UnsupportedSchema { .. }
            | BackupError::InvalidSnapshot { .. } => ErrorKind::InvalidBackup,
            BackupError::AlreadyExists { .. }
            | BackupError::Io { .. }
            | BackupError::SafetyDirUnavailable { .. }
            | BackupError::SafetyCheckFailed { .. } => ErrorKind::Io,
            BackupError::Cancelled => ErrorKind::Cancelled,
        }
    }
}

/// [`Session::listen`](crate::Session::listen) stopped.
#[derive(Debug, thiserror::Error)]
pub enum ListenError {
    /// The transport failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The keyboard sent its reset notification. Nothing was sent in
    /// response.
    #[error("the keyboard reported a reset (input report {report}); nothing was sent in response")]
    KeyboardReset {
        /// Hex dump of the reset report.
        report: String,
    },
}

/// A write operation failed. See [`Writer`](crate::Writer).
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// Another program is talking to the keyboard (heartbeats, write
    /// acknowledgements or profile reports we did not cause). Nothing more
    /// was written.
    #[error(
        "another program seems to be using the keyboard ({evidence}); nothing more was written. \
         Close the vendor web configurator (the whole browser tab: a background tab may not be \
         detected) and other keyboard software, then try again"
    )]
    OtherProgramActive {
        /// What was seen.
        evidence: ForeignActivity,
    },
    /// The keyboard sent its reset notification (Fn + Space held). OpenWraith
    /// did not answer it and sends nothing more in this session, not even a
    /// switch back to the original profile.
    #[error(
        "the keyboard reported a reset (input report {report}); OpenWraith did not answer it \
         and sent nothing more"
    )]
    KeyboardReset {
        /// Hex dump of the reset report.
        report: String,
    },
    /// The connection was lost. Nothing more is sent in this session.
    #[error("the connection to the keyboard was lost: {message}")]
    Disconnected {
        /// The transport's message.
        message: String,
    },
    /// A read failed (timeout, inconsistent or undecodable reply).
    #[error(transparent)]
    Session(SessionError),
    /// The keyboard does not hold the written bytes, even after one more
    /// attempt with the whole table.
    #[error("{table}: the write could not be verified after {sends} attempt(s): {mismatch}")]
    VerifyFailed {
        /// The table.
        table: TableId,
        /// How often the whole table was sent.
        sends: u32,
        /// The last read-back difference.
        mismatch: Box<Mismatch>,
    },
    /// The same table of the same profile was already written twice by this
    /// writer (protection against loops and floods).
    #[error("{table} was already written twice in this operation; not writing it again")]
    RepeatedWrite {
        /// The table.
        table: TableId,
        /// The profile (if known).
        profile: Option<u8>,
    },
    /// The keyboard did not switch to the requested profile.
    #[error("the keyboard did not switch to profile {requested} (it reports profile {active})")]
    ProfileNotSelected {
        /// The requested profile.
        requested: ProfileId,
        /// The profile the keyboard reports.
        active: u8,
    },
    /// The active onboard profile changed without us asking (keyboard-side
    /// switch or another program).
    #[error(
        "the active onboard profile changed unexpectedly (expected {}, the keyboard reports \
         {reported})",
        expected.map_or_else(|| "unknown".to_string(), |p| p.to_string())
    )]
    ProfileChanged {
        /// The profile we expected, if known.
        expected: Option<u8>,
        /// The profile the keyboard reported.
        reported: u8,
    },
    /// The keyboard reports an active profile outside 1..=4.
    #[error("the keyboard reports active profile {profile}; only profiles 1-4 are supported")]
    InvalidProfile {
        /// The reported value.
        profile: u8,
    },
    /// A write could not be (re)built from the keyboard's current state.
    #[error("cannot build the write: {0}")]
    Build(#[from] WriteBuildError),
    /// The safety backup passed to
    /// [`Writer::write_guarded`](crate::Writer::write_guarded) does not hold
    /// the active profile of this keyboard. Nothing was written.
    #[error(
        "the safety backup {} does not hold profile {profile} of this keyboard; save a safety \
         backup of it first. Nothing was written",
        path.display()
    )]
    NotBackedUp {
        /// The safety backup.
        path: PathBuf,
        /// The active profile.
        profile: u8,
    },
    /// The keyboard reports a product this crate does not know. Nothing was
    /// written.
    #[error(
        "unsupported keyboard (product {}); nothing was written",
        product.as_deref().unwrap_or("unknown")
    )]
    UnsupportedModel {
        /// The product code the keyboard reports, if any.
        product: Option<String>,
    },
}

impl WriteError {
    /// The machine-readable class of this error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            WriteError::OtherProgramActive { .. } => ErrorKind::OtherProgramActive,
            WriteError::KeyboardReset { .. } => ErrorKind::KeyboardReset,
            WriteError::Disconnected { .. } => ErrorKind::Disconnected,
            WriteError::Session(e) => e.kind(),
            WriteError::VerifyFailed { .. } | WriteError::RepeatedWrite { .. } => {
                ErrorKind::VerifyFailed
            }
            WriteError::ProfileNotSelected { .. }
            | WriteError::ProfileChanged { .. }
            | WriteError::InvalidProfile { .. } => ErrorKind::ProfileChanged,
            WriteError::Build(_) => ErrorKind::InvalidRequest,
            WriteError::NotBackedUp { .. } => ErrorKind::InvalidBackup,
            WriteError::UnsupportedModel { .. } => ErrorKind::ModelMismatch,
        }
    }

    /// Whether nothing at all may be sent any more in this session (keyboard
    /// reset or lost connection).
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            WriteError::KeyboardReset { .. } | WriteError::Disconnected { .. }
        )
    }
}

impl From<SessionError> for WriteError {
    fn from(error: SessionError) -> Self {
        match error {
            SessionError::KeyboardReset { report, .. } => WriteError::KeyboardReset { report },
            SessionError::Transport(e) => e.into(),
            other => WriteError::Session(other),
        }
    }
}

impl From<TransportError> for WriteError {
    fn from(error: TransportError) -> Self {
        WriteError::Disconnected {
            message: error.to_string(),
        }
    }
}

impl From<ListenError> for WriteError {
    fn from(error: ListenError) -> Self {
        match error {
            ListenError::Transport(e) => e.into(),
            ListenError::KeyboardReset { report } => WriteError::KeyboardReset { report },
        }
    }
}

/// Reading several onboard profiles failed
/// ([`read_profiles`](crate::read_profiles)).
#[derive(Debug, thiserror::Error)]
#[error("{failure}")]
pub struct ProfilesError {
    /// What went wrong.
    #[source]
    pub failure: ProfilesFailure,
    /// The profile that is active now, as far as known. After a failure the
    /// original profile is selected again, unless the keyboard reset or the
    /// connection was lost.
    pub active_profile_now: Option<u8>,
}

impl ProfilesError {
    /// The machine-readable class of this error.
    pub fn kind(&self) -> ErrorKind {
        self.failure.kind()
    }
}

/// The cause of a [`ProfilesError`].
#[derive(Debug, thiserror::Error)]
pub enum ProfilesFailure {
    /// Reading a profile failed.
    #[error(transparent)]
    Backup(#[from] BackupError),
    /// Switching profiles failed (or another program is active).
    #[error(transparent)]
    Write(#[from] WriteError),
    /// The keyboard did not report an active profile in 1..=4.
    #[error(
        "the keyboard reports active profile {}; only profiles 1-4 are supported",
        profile.map_or_else(|| "none".to_string(), |p| p.to_string())
    )]
    UnknownActiveProfile {
        /// The reported profile, if any.
        profile: Option<u8>,
    },
    /// Another profile reports another product than the first one read.
    #[error(
        "profile {profile} reports another keyboard ({})",
        product.as_deref().unwrap_or("unknown product")
    )]
    ProductChanged {
        /// The profile.
        profile: u8,
        /// Its product code.
        product: Option<String>,
    },
    /// The progress callback asked to stop.
    #[error("cancelled")]
    Cancelled,
}

impl ProfilesFailure {
    /// The machine-readable class of this error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            ProfilesFailure::Backup(e) => e.kind(),
            ProfilesFailure::Write(e) => e.kind(),
            ProfilesFailure::UnknownActiveProfile { .. } => ErrorKind::UnexpectedReply,
            ProfilesFailure::ProductChanged { .. } => ErrorKind::ModelMismatch,
            ProfilesFailure::Cancelled => ErrorKind::Cancelled,
        }
    }
}

/// A restore or an edit could not be planned or prepared. Nothing was
/// written when this error is returned; failures while writing are reported
/// in a [`RestoreReport`](crate::RestoreReport) or an
/// [`EditReport`](crate::EditReport).
#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    /// The backup is from another keyboard model.
    #[error(
        "the backup was made on {}, but the keyboard is {}",
        backup.as_deref().unwrap_or("an unknown keyboard"),
        keyboard.as_deref().unwrap_or("an unknown keyboard")
    )]
    ModelMismatch {
        /// Product code in the backup.
        backup: Option<String>,
        /// Product code of the keyboard.
        keyboard: Option<String>,
    },
    /// The keyboard runs other firmware than the backup was made with, and
    /// the mismatch was not allowed.
    #[error(
        "the backup was made with firmware {}, the keyboard runs {}; table formats may differ \
         (allow a firmware mismatch to restore anyway)",
        backup.as_deref().unwrap_or("unknown"),
        keyboard.as_deref().unwrap_or("unknown")
    )]
    FirmwareMismatch {
        /// Firmware in the backup.
        backup: Option<String>,
        /// Firmware of the keyboard.
        keyboard: Option<String>,
    },
    /// The backup cannot be used (for example it has no active-profile
    /// reply).
    #[error("cannot restore from this backup: {reason}")]
    InvalidBackup {
        /// Why.
        reason: String,
    },
    /// A requested profile is not in the backup.
    #[error("profile {profile} is not in the backup")]
    ProfileNotInBackup {
        /// The profile.
        profile: u8,
    },
    /// The saved safety backup does not belong to this operation.
    #[error(
        "the safety backup {} does not belong to this operation (other time, keyboard or profiles)",
        path.display()
    )]
    SafetyBackupMismatch {
        /// The saved file.
        path: PathBuf,
    },
    /// The requested edit is not valid.
    #[error("{reason}")]
    InvalidRequest {
        /// Why.
        reason: String,
    },
    /// Reading several profiles failed.
    #[error(transparent)]
    Profiles(#[from] ProfilesError),
    /// Reading the keyboard failed.
    #[error(transparent)]
    Backup(#[from] BackupError),
    /// The progress callback asked to stop.
    #[error("cancelled")]
    Cancelled,
}

impl RestoreError {
    /// The machine-readable class of this error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            RestoreError::ModelMismatch { .. } => ErrorKind::ModelMismatch,
            RestoreError::FirmwareMismatch { .. } => ErrorKind::FirmwareMismatch,
            RestoreError::InvalidBackup { .. }
            | RestoreError::ProfileNotInBackup { .. }
            | RestoreError::SafetyBackupMismatch { .. } => ErrorKind::InvalidBackup,
            RestoreError::InvalidRequest { .. } => ErrorKind::InvalidRequest,
            RestoreError::Profiles(e) => e.kind(),
            RestoreError::Backup(e) => e.kind(),
            RestoreError::Cancelled => ErrorKind::Cancelled,
        }
    }

    /// The active profile now, if this error came from reading several
    /// profiles (which switches profiles and back).
    pub fn active_profile_now(&self) -> Option<u8> {
        match self {
            RestoreError::Profiles(e) => e.active_profile_now,
            _ => None,
        }
    }
}

impl From<SessionError> for RestoreError {
    fn from(error: SessionError) -> Self {
        RestoreError::Backup(error.into())
    }
}
