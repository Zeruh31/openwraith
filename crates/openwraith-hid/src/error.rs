//! Error types of the HID crate.

use std::path::PathBuf;

use openwraith_protocol::{DecodeError, FrameError, ReadRequest, ResponseError};

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
}
