//! Automatic backups (the known-good store), reset detection and recovery.
//!
//! OpenWraith never restores anything without asking: a recovery is the
//! standard restore (`prepare_recovery` compares, `apply_restore` writes
//! after the review).

use serde::{Deserialize, Serialize};

use super::common::{FileRef, FileToken};

/// Why an automatic backup was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AutoBackupReason {
    /// The first complete read of the profile on this PC.
    FirstRead,
    /// The read after a change OpenWraith wrote.
    AfterWrite,
    /// The profile was changed elsewhere (on the keyboard or by another
    /// program) since the last automatic backup.
    ChangedElsewhere,
    /// "It's fine, keep them" after a suspected reset.
    KeptCurrent,
    /// "Back up now" in the tray.
    Manual,
}

/// One automatic backup (from its file name; not loaded).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AutoBackupEntry {
    /// The file (`reveal` shows it in its folder; restore and export take
    /// its token like any backup file).
    pub file: FileRef,
    /// The profile it holds (1-4).
    pub profile: u8,
    /// The product code in the name.
    pub product: Option<String>,
    /// When it was read (RFC 3339, UTC).
    pub created_at: String,
    /// Why it was made.
    pub reason: AutoBackupReason,
    /// The user keeps it: never deleted, not counted in the newest N.
    pub pinned: bool,
}

/// The automatic backups (`list_auto_backups`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AutoBackupsView {
    /// Whether automatic backups are made (Settings > Background).
    pub enabled: bool,
    /// How many are kept per profile (pinned ones aside).
    pub keep: u8,
    /// Whether the list is the connected keyboard's (else every keyboard's
    /// in the store, before one was read).
    pub connected_keyboard: bool,
    /// The keyboard reports no USB serial number (the W75): the store
    /// cannot tell it from another keyboard of its model on this computer.
    pub unidentified: bool,
    /// Newest first, every profile.
    pub entries: Vec<AutoBackupEntry>,
}

/// A table compared with its known-good copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum KnownGoodTableView {
    /// Actuation and Rapid Trigger.
    Triggers,
    /// The main light.
    MainLight,
    /// The key colours (custom colour table 1).
    CustomColors1,
    /// The base keymap.
    BaseKeymap,
    /// The Fn keymap.
    FnKeymap,
    /// The macros.
    Macros,
    /// The advanced keys.
    AdvancedKeys,
    /// The dead band.
    DeadBand,
    /// The SOCD pairs of the profile.
    SocdPairs,
}

/// A firmware as the keyboard reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FirmwareView {
    /// The version (`3.22.07`).
    pub version: Option<String>,
    /// The build date as the keyboard reports it.
    pub build_date: Option<String>,
}

/// Why a profile looks reset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "evidence", rename_all = "snake_case")]
pub enum ResetEvidenceView {
    /// The keyboard reported a reset (Fn + Space held) before this read.
    ResetSeen,
    /// The keyboard runs another firmware than when the automatic backup
    /// was read (an update very likely resets the settings).
    FirmwareChanged {
        /// The automatic backup's firmware.
        known: FirmwareView,
        /// The keyboard's firmware now.
        current: FirmwareView,
    },
    /// Every key's actuation is the factory one now, and it was not.
    TriggersToFactory,
    /// At least two of keys, Fn layer, macros and advanced keys are empty
    /// (factory) now, and they were not.
    KeymapGroupToFactory {
        /// Those tables.
        tables: Vec<KnownGoodTableView>,
    },
}

/// How a profile compares with its last automatic backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum RecoveryProfileState {
    /// No automatic backup of this profile yet.
    NoKnownGood,
    /// Not read since OpenWraith started (or since its backup).
    NotChecked,
    /// The same as its last automatic backup (or changed, not reset-like:
    /// the changes were backed up).
    Fine,
    /// It looks reset: the tables that changed to the factory form are held
    /// (no automatic backup replaces them) until "Restore" or "It's fine,
    /// keep them".
    ResetSuspected,
}

/// One profile on the Recovery card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RecoveryProfileView {
    /// The profile (1-4).
    pub profile: u8,
    /// Its local name (Settings > Profile names), if any.
    pub name: Option<String>,
    /// How it compares.
    pub state: RecoveryProfileState,
    /// Why it looks reset (empty unless `reset_suspected`).
    pub evidence: Vec<ResetEvidenceView>,
    /// The tables that changed to their factory form (the only ones a
    /// restore is offered for), when it looks reset.
    pub factory_now: Vec<KnownGoodTableView>,
    /// Tables that changed otherwise (informational).
    pub changed: Vec<KnownGoodTableView>,
    /// When the automatic backup it was compared with was read.
    pub known_good_at: Option<String>,
    /// How old that backup was at the comparison, in seconds.
    pub known_good_age_s: Option<u32>,
    /// When the profile was read for the comparison.
    pub checked_at: Option<String>,
    /// The automatic backups of the profile to restore from, newest first.
    pub backups: Vec<AutoBackupEntry>,
    /// The newest backup's firmware differs from the keyboard's (a restore
    /// needs `allow_firmware_mismatch`).
    pub firmware_differs: bool,
}

/// The Recovery card (`get_recovery`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RecoveryView {
    /// Something looks reset: a profile is held, the keyboard reported a
    /// reset or its firmware changed (not yet resolved).
    pub attention: bool,
    /// The keyboard reported a reset (Fn + Space) during this run and the
    /// user has not restored or kept the settings since.
    pub reset_reported: bool,
    /// When it reported it (RFC 3339).
    pub reset_reported_at: Option<String>,
    /// The keyboard's firmware changed since its automatic backups.
    pub firmware_changed: Option<FirmwareChangeView>,
    /// Every profile, 1-4.
    pub profiles: Vec<RecoveryProfileView>,
    /// "Restore all profiles" is possible: at least two profiles have
    /// automatic backups, and their newest ones share one firmware (pass the
    /// newest backup of every profile that has one to `prepare_recovery`).
    pub can_restore_all: bool,
    /// Whether automatic backups (and with them the comparison) are on.
    pub auto_backup_enabled: bool,
    /// The keyboard reports no USB serial number (the W75): its automatic
    /// backups cannot be told from another keyboard of its model's.
    pub unidentified: bool,
}

/// A firmware change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FirmwareChangeView {
    /// The firmware of the automatic backups.
    pub known: FirmwareView,
    /// The keyboard's firmware now.
    pub current: FirmwareView,
}

/// Compare automatic backups with the keyboard, for the standard restore
/// review (`apply_restore` writes the plan).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RecoveryIntent {
    /// Automatic backups (tokens of `list_auto_backups` or `get_recovery`),
    /// at most one per profile. One restores that profile; several are
    /// composed into one backup ("Restore all profiles").
    pub files: Vec<FileToken>,
    /// Restore although the firmware differs (after a firmware update).
    pub allow_firmware_mismatch: bool,
    /// The profiles the user agreed to switch to for the comparison (every
    /// profile restored except the active one).
    pub confirmed_switches: Vec<u8>,
}

/// What the header and the tray show about a suspected reset.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RecoveryStatusView {
    /// Profiles that look reset (their factory tables are held).
    pub reset_suspected: Vec<u8>,
    /// The keyboard reported a reset during this run, not yet resolved.
    pub reset_reported: bool,
    /// The keyboard's firmware changed since its automatic backups.
    pub firmware_changed: bool,
}
