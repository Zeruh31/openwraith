//! Backup files.

use serde::Serialize;

use super::common::FileRef;

/// A backup file, as far as the restore wizard needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BackupFileSummary {
    /// The file.
    pub file: FileRef,
    /// A file the user chose, or a safety backup.
    pub kind: BackupFileKind,
    /// When the backup was made (RFC 3339).
    pub created_at: String,
    /// The program that made it.
    pub tool: ToolView,
    /// Made by the developer simulator.
    pub made_by_simulator: bool,
    /// Model name.
    pub model: Option<String>,
    /// Product code.
    pub product: Option<String>,
    /// Firmware.
    pub firmware: Option<String>,
    /// File schema version.
    pub schema_version: u32,
    /// The profiles it holds, ascending.
    pub profiles: Vec<u8>,
    /// The profile that was active when it was made.
    pub active_profile: Option<u8>,
    /// Same model as the connected keyboard (`null` when none is connected).
    pub same_model: Option<bool>,
    /// Same firmware as the connected keyboard (`null` when none is
    /// connected).
    pub same_firmware: Option<bool>,
    /// Why the safety backup was made (safety backups only).
    pub safety_reason: Option<SafetyReason>,
    /// Replies that could not be decoded.
    pub undecoded: u16,
}

/// Where a backup file came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum BackupFileKind {
    /// Chosen by the user.
    User,
    /// A safety backup.
    Safety,
}

/// The program that made a backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ToolView {
    /// Its name.
    pub name: String,
    /// Its version.
    pub version: String,
}

/// A safety backup in the safety folder (from its file name; not loaded).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SafetyBackupEntry {
    /// The file.
    pub file: FileRef,
    /// The product code in the name.
    pub product: Option<String>,
    /// The time stamp in the name (`YYYYMMDD-HHMMSS`, UTC).
    pub stamp: String,
    /// Why it was made.
    pub reason: SafetyReason,
    /// When the file was last modified (RFC 3339).
    pub modified_at: Option<String>,
}

/// Why a safety backup was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum SafetyReason {
    /// Before a trigger change.
    SetTrigger,
    /// Before a lighting change.
    SetLight,
    /// Before a restore.
    Restore,
    /// Before a key, macro or advanced-key change.
    SetKeys,
    /// Before a key-colour change.
    SetColors,
    /// Before a dead-band change.
    SetDeadband,
    /// Before a SOCD change.
    SetSocd,
    /// Before a profile import.
    Import,
    /// Anything else.
    Other,
}
