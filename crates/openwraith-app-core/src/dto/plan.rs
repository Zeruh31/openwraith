//! Restore plans.

use serde::Serialize;

use super::common::{
    CommitTriggerReasonView, KeyRef, LayerView, LightValue, PlanToken, ResendReasonView, TableRef,
    TriggerValue,
};
use super::files::BackupFileSummary;

/// A prepared restore, for review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RestorePreview {
    /// The token to apply it; `null` when there is nothing to write.
    pub token: Option<PlanToken>,
    /// The backup.
    pub source: BackupFileSummary,
    /// The plan.
    pub plan: RestorePlanView,
    /// Everything that can be restored already matches.
    pub nothing_to_write: bool,
    /// The profiles the comparison switched to (and back).
    pub switches: Vec<u8>,
}

/// What a restore would write, per profile and table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RestorePlanView {
    /// Product code of the keyboard.
    pub product: Option<String>,
    /// When the backup was made.
    pub backup_created_at: String,
    /// When the keyboard was read.
    pub keyboard_read_at: String,
    /// Firmware in the backup.
    pub backup_firmware: Option<String>,
    /// Firmware of the keyboard.
    pub keyboard_firmware: Option<String>,
    /// The profile that was active when the keyboard was read.
    pub original_profile: Option<u8>,
    /// One entry per profile in scope, ascending.
    pub profiles: Vec<ProfilePlanView>,
    /// Differences that do not block the restore.
    pub warnings: Vec<PlanWarningView>,
    /// Counts.
    pub totals: PlanTotals,
}

/// The plan of one profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProfilePlanView {
    /// The profile.
    pub profile: u8,
    /// Whether the keyboard's state of this profile was compared.
    pub compared: bool,
    /// Every table, writes first in execution order.
    pub items: Vec<PlanItemView>,
}

/// One table (or, for triggers, one key) of a profile plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PlanItemView {
    /// The table.
    pub table: TableRef,
    /// The key, for per-key trigger items.
    pub key: Option<KeyRef>,
    /// What happens to it.
    pub action: PlanActionView,
}

/// What happens to a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PlanActionView {
    /// The keyboard already holds the backup's bytes.
    Unchanged,
    /// The backup has no reply for this table.
    NotInBackup,
    /// The table is written.
    Write {
        /// Packets of the first send.
        packets: u16,
        /// The entries that change (at most 200).
        changes: Vec<ChangeView>,
        /// Changes left out of `changes`.
        more_changes: u16,
        /// Set when this is the keymap group's commit trigger.
        commit_trigger: Option<CommitTriggerReasonView>,
        /// An unchanged Fn keymap re-sent as the commit trigger.
        resent_unchanged: bool,
        /// Set when it re-sends a macro slot whose content does not change
        /// (written again after macro slot 0; `changes` is empty).
        resent: Option<ResendReasonView>,
    },
    /// The table differs but is not written.
    Skip {
        /// Why.
        reason: SkipReasonView,
        /// English, technical details only.
        message: String,
    },
}

/// One changed entry of a planned write, decoded where possible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeView {
    /// A key's trigger settings.
    Trigger {
        /// Now.
        old: TriggerBytesView,
        /// In the backup.
        new: TriggerBytesView,
    },
    /// The main light.
    Light {
        /// Now.
        old: LightBytesView,
        /// In the backup.
        new: LightBytesView,
    },
    /// A keymap entry.
    Keymap {
        /// The key.
        key: KeyRef,
        /// Now (hex).
        old_hex: String,
        /// In the backup (hex).
        new_hex: String,
    },
    /// A key's dead band.
    DeadBand {
        /// The key.
        key: KeyRef,
        /// Now.
        old: Option<DeadBandPair>,
        /// In the backup.
        new: Option<DeadBandPair>,
    },
    /// A key's custom colour.
    Color {
        /// The key.
        key: KeyRef,
        /// Now, `"#rrggbb"`.
        old: Option<String>,
        /// In the backup.
        new: Option<String>,
    },
    /// A macro slot.
    Macro {
        /// Now.
        old: MacroSummary,
        /// In the backup.
        new: MacroSummary,
    },
    /// An advanced-key slot.
    AdvancedKey {
        /// Now.
        old: AdvancedSummary,
        /// In the backup.
        new: AdvancedSummary,
    },
    /// Anything else, as hex.
    Raw {
        /// Entry index.
        index: u16,
        /// Now (hex).
        old_hex: String,
        /// In the backup (hex).
        new_hex: String,
    },
}

/// A trigger record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TriggerBytesView {
    /// Decoded.
    Decoded {
        /// The settings.
        value: TriggerValue,
        /// The second travel value (normally the actuation point).
        second_travel: u16,
    },
    /// Not decodable.
    Raw {
        /// The bytes.
        hex: String,
    },
}

/// A light record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LightBytesView {
    /// Decoded.
    Decoded {
        /// The record.
        value: LightValue,
    },
    /// Not decodable.
    Raw {
        /// The bytes.
        hex: String,
    },
}

/// Dead-band values of a key, in trigger units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeadBandPair {
    /// Top dead band.
    pub top: u16,
    /// Bottom dead band.
    pub bottom: u16,
}

/// A macro slot's contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MacroSummary {
    /// Empty.
    Empty,
    /// A number of steps.
    Steps {
        /// How many.
        count: u16,
    },
    /// In use, not decodable as steps.
    InUse,
}

/// An advanced-key slot's contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AdvancedSummary {
    /// Empty.
    Empty,
    /// In use.
    InUse {
        /// The type byte.
        kind: u8,
    },
}

/// Why a differing table is not restored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "code", rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum SkipReasonView {
    UnconfirmedCommand {
        command: String,
    },
    NoKnownCommand,
    UnsupportedLightMode {
        mode: u8,
    },
    InvalidLightRecord,
    Knob,
    UnknownTriggerMode {
        mode: u8,
    },
    SecondTravelDiffers,
    PressReleaseDiffer,
    OutOfRange,
    TriggerLimitsUnknown,
    DeadBandOutOfRange,
    DeadBandUnitUnknown,
    DistanceUnitChanged {
        backup_um: Option<u32>,
        keyboard_um: Option<u32>,
    },
    KeymapGroupBlocked {
        cause: BlockCauseView,
    },
    NotInKeyboard,
    Unusable,
    /// Only bytes that are never written differ (the SOCD hotkey).
    ReadOnlyBytes,
    /// The backup's SOCD pairs are not in the vendor site's form.
    UnwritableSocdPairs {
        rule: String,
    },
    /// An import does not write this category.
    NotImported,
}

/// Why the keymap group is not restored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "code", rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum BlockCauseView {
    /// An advanced-key slot that is not a DKS, Mod-Tap or Toggle OpenWraith
    /// can write (`reason` is technical text).
    UnwritableAdvancedKey {
        slot: u8,
        reason: String,
    },
    NonCanonicalMacro {
        slot: u8,
        rule: String,
    },
    MalformedReference {
        layer: LayerView,
        index: u8,
    },
    Incomplete {
        table: TableRef,
    },
    FnKeymapNotWritable {
        reason: CommitTriggerReasonView,
    },
    /// An imported Fn layer replaces a critical built-in function.
    CriticalFnOverride {
        index: u8,
    },
    /// An imported keymap entry OpenWraith does not write (`entry` is the
    /// hex of the file's 4 bytes).
    UnwritableImportedEntry {
        layer: LayerView,
        index: u8,
        entry: String,
    },
}

/// A difference that does not block the restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "code", rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum PlanWarningView {
    FirmwareDiffers {
        backup: Option<String>,
        keyboard: Option<String>,
    },
    /// A device constant differs (`request` is technical text).
    ConstantDiffers {
        request: String,
    },
    CapabilityFlagsDiffer,
    /// A restored profile of the backup holds another SOCD power than
    /// `profile` (active when the backup was made), whose power is restored
    /// for the whole keyboard.
    SocdPowerDiffersInBackup {
        profile: u8,
        power: bool,
    },
    /// An import writes the SOCD pairs only; the keyboard keeps its power
    /// (`power`), a setting of the whole keyboard.
    SocdPowerNotImported {
        power: bool,
    },
    /// Profile `from` of the file is imported into profile `into`.
    Imported {
        from: u8,
        into: u8,
    },
    /// An import writes these macro slots (0-based).
    ImportedMacros {
        slots: Vec<u8>,
    },
    /// The Fn layer replaces a built-in Fn function.
    FnBuiltinOverridden {
        index: u8,
        critical: bool,
    },
    /// An import keeps the keyboard's entries at these key indices (not
    /// keys, the knob, the Fn key, Fn-locked keys).
    ImportKeptAsRead {
        layer: LayerView,
        indices: Vec<u8>,
    },
    /// An imported entry whose effect is not yet verified on a W75
    /// (`what` and `question` are technical text).
    Unverified {
        layer: LayerView,
        index: u8,
        what: String,
        question: String,
    },
}

/// Counts of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PlanTotals {
    /// Table writes.
    pub writes: u16,
    /// Write packets.
    pub packets: u16,
    /// Per profile with writes.
    pub per_profile: Vec<ProfileTotals>,
}

/// Counts of one profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProfileTotals {
    /// The profile.
    pub profile: u8,
    /// Table writes.
    pub writes: u16,
    /// Write packets.
    pub packets: u16,
}
