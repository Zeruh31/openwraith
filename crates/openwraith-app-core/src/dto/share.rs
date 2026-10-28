//! Sharing one profile as a file: export and import. An import is a
//! restore with the library's import options; its plan is the restore's.

use serde::{Deserialize, Serialize};

use super::common::{FileRef, FileToken, KeyRef, PlanToken};
use super::files::BackupFileSummary;
use super::keys::MacroProgramView;
use super::plan::RestorePlanView;

/// Which categories of settings (an import writes only the chosen ones).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ShareSelectionView {
    /// Actuation and Rapid Trigger.
    pub triggers: bool,
    /// The main light and the key colours.
    pub lighting: bool,
    /// Key assignments, macros and advanced keys.
    pub keys: bool,
    /// Dead band.
    pub dead_band: bool,
    /// SOCD pairs (the SOCD power is a setting of the whole keyboard and
    /// keeps the keyboard's value).
    pub socd: bool,
}

/// Export one profile to a file (`.owprofile.json`; no USB serial number).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ExportIntent {
    /// Where the profile is read from.
    pub source: ExportSource,
    /// The profile (1-4).
    pub profile: u8,
    /// A label stored in the file's note (at most 120 characters).
    pub label: Option<String>,
    /// The destination (`choose_export_destination`).
    pub destination: FileToken,
    /// The user agreed to switch to `profile` and back (needed when it is
    /// not the active profile and the source is the keyboard).
    pub confirmed_switch: bool,
}

/// Where an export reads the profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExportSource {
    /// The keyboard now (a profile that is not active is read by switching
    /// to it and back).
    Keyboard,
    /// A backup file the app knows (no keyboard traffic).
    File {
        /// The file.
        file: FileToken,
    },
}

/// A saved export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ExportedProfile {
    /// The file.
    pub file: FileRef,
    /// The profile it holds.
    pub profile: u8,
    /// Product code.
    pub product: Option<String>,
    /// Firmware.
    pub firmware: Option<String>,
    /// What it holds.
    pub contents: ShareContentsView,
    /// Whether the export switched profiles to read it (and back).
    pub switched: bool,
}

/// What a profile file holds (the categories with readable tables).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ShareContentsView {
    /// Which categories it holds.
    pub present: ShareSelectionView,
    /// The macros with content, as steps where possible (they are shared
    /// with the file).
    pub macros: Vec<SharedMacroView>,
    /// Advanced-key slots with content.
    pub advanced_slots: u8,
}

/// One macro slot with content in a profile file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SharedMacroView {
    /// The library's slot 0..=9 (the UI shows slot + 1).
    pub slot: u8,
    /// The macro as steps; `null` when OpenWraith cannot show it as steps
    /// (an import of keys then stops the keys from being imported).
    pub program: Option<MacroProgramView>,
}

/// A profile file opened for an import (`open_import_file`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ImportSummaryView {
    /// The file.
    pub file: BackupFileSummary,
    /// The profile of the file that is imported by default (its own
    /// profile, for an all-profile backup the one that was active).
    pub profile: Option<u8>,
    /// What that profile holds.
    pub contents: ShareContentsView,
    /// The categories ticked by default (triggers and lighting).
    pub default_include: ShareSelectionView,
}

/// Import a profile of a file into a profile of the keyboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ImportIntent {
    /// The file (`open_import_file`).
    pub source: FileToken,
    /// The profile of the file; `null` = the file's own profile.
    pub from: Option<u8>,
    /// The profile of the keyboard that is written.
    pub into: u8,
    /// The categories written.
    pub include: ShareSelectionView,
    /// The keys (indices) whose critical built-in function (Fn + 1..4,
    /// Fn + Space) the user agreed the imported Fn layer may replace, each
    /// named in its own danger callout. The keys are imported only when
    /// every critical override of the plan is in this list; empty = none
    /// allowed.
    pub allowed_critical_keys: Vec<u8>,
    /// Import although the firmware differs.
    pub allow_firmware_mismatch: bool,
    /// The profiles the user agreed to switch to for the comparison (`into`
    /// when it is not the active profile).
    pub confirmed_switches: Vec<u8>,
}

/// A planned import (the restore's plan shapes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ImportPreview {
    /// The token to apply it; `null` when there is nothing to write.
    pub token: Option<PlanToken>,
    /// The file.
    pub source: BackupFileSummary,
    /// The profile of the file.
    pub from: u8,
    /// The profile of the keyboard.
    pub into: u8,
    /// The categories written.
    pub include: ShareSelectionView,
    /// The plan (the restore review's shapes; skips name why a category is
    /// not imported).
    pub plan: RestorePlanView,
    /// Everything chosen already matches.
    pub nothing_to_write: bool,
    /// The profiles the comparison switched to (and back).
    pub switches: Vec<u8>,
    /// Every key whose imported Fn-layer entry replaces a critical built-in
    /// function of the keyboard's profile (Fn + 1..4, Fn + Space), in the
    /// blocked and in the allowed plan alike, so each keeps its callout.
    /// Empty when the keys are not imported: not chosen, or left out for
    /// another cause (`other_cause_blocked`), which no permission lifts.
    pub critical_fn_overrides: Vec<KeyRef>,
    /// The keys were left out because these overrides were not allowed
    /// (list them all in `allowed_critical_keys` to import them).
    pub critical_blocked: bool,
    /// A chosen category is not imported as a whole (the keys blocked, or
    /// SOCD pairs not in the vendor site's form).
    pub category_blocked: bool,
    /// A chosen category is left out for a cause the critical permission
    /// does not lift: the keys blocked by something else in the file (an
    /// entry, macro or slot OpenWraith does not write), or SOCD pairs not in
    /// the vendor site's form. The plan's skips say why.
    pub other_cause_blocked: bool,
}
