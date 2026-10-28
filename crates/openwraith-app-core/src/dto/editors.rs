//! The editors of the Actuation and Lighting pages besides triggers and the
//! main light: key colours (custom table 1), dead band, SOCD and trigger
//! presets. Every state is built from the last full read (no traffic);
//! intents carry its `revision` as `based_on`.

use serde::{Deserialize, Serialize};

use super::common::{KeyRef, LightModeView, PlanToken, TableRef};
use super::intent::RtIntent;
use super::keys::{EvidenceView, PersistencePlanView, UnverifiedNoteView};
use super::snapshot::TriggerLimitsView;

// ----- shared by the editors -----

/// One planned write (technical details of a review).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct EditWriteView {
    /// The table.
    pub table: TableRef,
    /// Packets of its first attempt.
    pub packets: u16,
    /// The library's description (technical English).
    pub summary: String,
}

/// Something the user should know about a plan before writing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum EditWarningView {
    /// Colours go to custom table 1, which the keyboard shows only in the
    /// Custom light mode, and the light is in another mode.
    NotShownUntilCustomMode,
    /// The SOCD power is one setting of the whole keyboard: switching it
    /// changes every profile (confirmed on hardware).
    SocdPowerAllProfiles,
    /// SOCD can also be switched on the keyboard with Fn + Right Shift.
    SocdHotkey,
    /// A written value is not yet verified on a W75 ("Not yet verified").
    Unverified {
        /// The library's note.
        note: UnverifiedNoteView,
    },
    /// The edit replaces content the Undo cannot write back (allowed
    /// explicitly).
    NotUndoable {
        /// The table.
        table: TableRef,
    },
    /// Any other library warning (technical text; not expected here).
    Other {
        /// The warning.
        message: String,
    },
}

/// A value field of an editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum EditFieldView {
    Top,
    Bottom,
    SocdMode,
    Actuation,
    Sensitivity,
    Press,
    Release,
}

/// Why an edit of these editors, a preset, an export or an import was
/// refused (`ErrorDetail::Edit`). Nothing was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "problem", rename_all = "snake_case")]
pub enum EditProblemView {
    /// The intent changes nothing.
    NoChanges,
    /// More keys (or pairs) than allowed.
    TooManyKeys {
        /// The most.
        max: u16,
    },
    /// A key is listed twice.
    DuplicateKey {
        /// The key.
        key: KeyRef,
    },
    /// The index is not a key of this keyboard.
    NotAKey {
        /// The index.
        index: u8,
    },
    /// The knob has no such setting.
    Knob,
    /// A colour is not `#rrggbb`.
    InvalidColor,
    /// A value is out of the keyboard's range (refused, never clamped).
    OutOfRange {
        /// Which value.
        field: EditFieldView,
        /// The value given.
        value: u16,
        /// The smallest allowed.
        min: u16,
        /// The largest allowed.
        max: u16,
    },
    /// The key has no HID usage (it cannot be in a SOCD pair).
    NoHidUsage {
        /// The key.
        key: KeyRef,
    },
    /// A SOCD pair uses the same key twice.
    SocdSameKey {
        /// The key.
        key: KeyRef,
    },
    /// A key is in two SOCD pairs.
    SocdDuplicateKey {
        /// The key.
        key: KeyRef,
    },
    /// No preset with this id.
    UnknownPreset {
        /// The id given.
        id: String,
    },
    /// A preset name must have 1 to 40 characters.
    PresetName,
    /// At most this many user presets.
    TooManyPresets {
        /// The most.
        max: u16,
    },
    /// The keyboard's table cannot be edited (unreadable, or in a form
    /// OpenWraith cannot write back).
    UnsupportedCurrentTable {
        /// The table.
        table: TableRef,
        /// Why (technical English).
        reason: String,
    },
    /// The editor is not available (the table was not read or cannot be
    /// decoded).
    NotAvailable,
    /// Any other refusal (technical English).
    Other {
        /// Why.
        message: String,
    },
}

// ----- key colours (custom table 1) -----

/// The colours of custom table 1 (`get_colors_state`). LED slot = key index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ColorsState {
    /// The revision the colours come from (`based_on` of colour intents).
    pub revision: u32,
    /// The connection it was read in.
    pub generation: u32,
    /// The active profile.
    pub profile: u8,
    /// When it was read (RFC 3339).
    pub read_at: String,
    /// Whether the colours can be changed.
    pub editable: bool,
    /// Why not.
    pub disabled: Option<ColorsDisabled>,
    /// Every key of the layout but the knob, in index order.
    pub keys: Vec<KeyColorView>,
    /// Slots outside those keys that hold a colour (the knob, 103, 126 on
    /// the W75): never written ("Kept as read").
    pub kept: Vec<KeptColorView>,
    /// The custom table the keyboard reports active (1 or 2), if known.
    pub active_table: Option<u8>,
    /// The main light's mode now.
    pub light_mode: Option<LightModeView>,
    /// Whether the keyboard shows table 1 now (the Custom mode).
    pub shown_now: bool,
    /// Whether "Also switch the effect to Custom" can be offered (the light
    /// can be changed and offers the Custom mode).
    pub can_switch_to_custom: bool,
}

/// Why the colours cannot be changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ColorsDisabled {
    /// The table was not read.
    NotRead,
    /// The table cannot be decoded.
    Unreadable {
        /// Why (technical English).
        message: String,
    },
    /// The keyboard is not a supported model.
    UnsupportedModel,
}

/// The colour of one key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeyColorView {
    /// The key.
    pub key: KeyRef,
    /// `"#rrggbb"` lowercase.
    pub color: String,
}

/// A slot kept as read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeptColorView {
    /// The slot (key index).
    pub slot: u8,
    /// `"#rrggbb"` lowercase.
    pub color: String,
}

/// Change key colours of custom table 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ColorEditIntent {
    /// The keys and their new colours (1 to 81 distinct keys, not the knob).
    pub colors: Vec<KeyColorIntent>,
    /// Also switch the main light to the Custom mode when it is in another
    /// one (opt-in; otherwise the keyboard does not show the table).
    pub switch_to_custom: bool,
    /// Check that the keyboard kept it (switches profiles; not pre-ticked).
    pub check_persistence: bool,
    /// `ColorsState.revision`.
    pub based_on: u32,
}

/// One key's new colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct KeyColorIntent {
    /// The key index.
    pub key: u8,
    /// `"#rrggbb"`.
    pub color: String,
}

/// A planned colour edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ColorEditPreview {
    /// The token to apply it; `null` when there is nothing to do.
    pub token: Option<PlanToken>,
    /// Nothing would be written.
    pub nothing_to_do: bool,
    /// The profile it writes to.
    pub profile: u8,
    /// The keys whose colour changes.
    pub changes: Vec<ColorChangeView>,
    /// Whether the keyboard shows table 1 now.
    pub shown_now: bool,
    /// Whether the plan also switches the light to the Custom mode.
    pub switches_to_custom: bool,
    /// The writes, in order.
    pub writes: Vec<EditWriteView>,
    /// Write packets in total.
    pub packets: u16,
    /// What the user should know.
    pub warnings: Vec<EditWarningView>,
    /// The persistence check.
    pub persistence: PersistencePlanView,
}

/// One key's colour change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ColorChangeView {
    /// The key.
    pub key: KeyRef,
    /// `"#rrggbb"` now.
    pub before: String,
    /// `"#rrggbb"` after the edit.
    pub after: String,
}

// ----- dead band -----

/// The dead band of the active profile (`get_dead_band_state`). Each
/// profile has its own (confirmed on hardware).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeadBandState {
    /// The revision it comes from (`based_on` of dead-band intents).
    pub revision: u32,
    /// The connection it was read in.
    pub generation: u32,
    /// The active profile.
    pub profile: u8,
    /// When it was read (RFC 3339).
    pub read_at: String,
    /// Whether it can be changed.
    pub editable: bool,
    /// Why not.
    pub disabled: Option<DeadBandDisabled>,
    /// The keyboard's distance unit in micrometres (20 on the W75).
    pub unit_um: Option<u32>,
    /// The largest top or bottom value, in trigger units (50 = 1.00 mm on
    /// the W75); the step is one unit.
    pub max: u16,
    /// Every key of the layout but the knob, in index order.
    pub keys: Vec<KeyDeadBandView>,
}

/// Why the dead band cannot be changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum DeadBandDisabled {
    /// The table was not read.
    NotRead,
    /// The keyboard's distance unit is not 0.02 mm (or unknown).
    DistanceUnit {
        /// The unit, if known.
        unit_um: Option<u32>,
    },
    /// The table cannot be decoded.
    Unreadable {
        /// Why (technical English).
        message: String,
    },
    /// The keyboard is not a supported model.
    UnsupportedModel,
}

/// Top and bottom dead zone of a key, in trigger units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeadBandValue {
    /// Ignored travel from rest.
    pub top: u16,
    /// Ignored travel before bottom-out.
    pub bottom: u16,
}

/// The dead band of one key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeyDeadBandView {
    /// The key.
    pub key: KeyRef,
    /// Its values.
    pub value: DeadBandValue,
}

/// Change the dead band of some keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct DeadBandEditIntent {
    /// Key indices (1 to 81 distinct keys, not the knob).
    pub keys: Vec<u8>,
    /// New top dead zone in trigger units, if it changes.
    pub top: Option<u16>,
    /// New bottom dead zone in trigger units, if it changes.
    pub bottom: Option<u16>,
    /// Check that the keyboard kept it (switches profiles; not pre-ticked).
    pub check_persistence: bool,
    /// `DeadBandState.revision`.
    pub based_on: u32,
}

/// A planned dead-band edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeadBandEditPreview {
    /// The token to apply it; `null` when there is nothing to do.
    pub token: Option<PlanToken>,
    /// Nothing would be written.
    pub nothing_to_do: bool,
    /// The profile it writes to.
    pub profile: u8,
    /// The keys whose dead band changes.
    pub changes: Vec<DeadBandChangeView>,
    /// Soft warnings against the keys' actuation points (never refused).
    pub hints: Vec<DeadBandHintView>,
    /// The writes (at most one).
    pub writes: Vec<EditWriteView>,
    /// Write packets in total.
    pub packets: u16,
    /// What the user should know.
    pub warnings: Vec<EditWarningView>,
    /// The persistence check.
    pub persistence: PersistencePlanView,
}

/// One key's dead-band change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeadBandChangeView {
    /// The key.
    pub key: KeyRef,
    /// Now.
    pub before: DeadBandValue,
    /// After the edit.
    pub after: DeadBandValue,
}

/// A dead zone that reaches the key's actuation point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeadBandHintView {
    /// The key.
    pub key: KeyRef,
    /// Which.
    pub kind: DeadBandHintKind,
    /// The key's actuation point, in trigger units.
    pub actuation: u16,
}

/// Which dead zone reaches the actuation point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum DeadBandHintKind {
    /// The top dead zone is at or below the actuation point.
    TopReachesActuation,
    /// The bottom dead zone starts above the actuation point.
    BottomReachesActuation,
}

// ----- SOCD -----

/// SOCD (`get_socd_state`): the power is one setting of the whole keyboard,
/// the pairs belong to the active profile (both confirmed on hardware).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SocdStateView {
    /// The revision it comes from (`based_on` of SOCD intents); changes
    /// when Fn + Right Shift switches SOCD on the keyboard.
    pub revision: u32,
    /// The connection it was read in.
    pub generation: u32,
    /// The active profile (the pairs' profile).
    pub profile: u8,
    /// When it was read (RFC 3339).
    pub read_at: String,
    /// Whether SOCD can be changed.
    pub editable: bool,
    /// Why not.
    pub disabled: Option<SocdDisabled>,
    /// SOCD on: one setting of the whole keyboard (every profile).
    pub power: bool,
    /// The pairs of the active profile (empty when not in the vendor
    /// site's form, see `pairs_writable`).
    pub pairs: Vec<SocdPairView>,
    /// Whether the pairs are in the vendor site's form (else they are kept
    /// as read; replacing them needs `allow_not_undoable`).
    pub pairs_writable: bool,
    /// Why not (technical English).
    pub unwritable_reason: Option<String>,
    /// The 80 table bytes as read (hex, technical details).
    pub raw_pairs_hex: String,
    /// The four resolution modes with their evidence (1 and 4 confirmed).
    pub modes: Vec<SocdModeView>,
    /// The most pairs (20).
    pub max_pairs: u8,
    /// The keys that can be in a pair (layout keys with a HID usage).
    pub keys: Vec<SocdKeyView>,
}

/// Why SOCD cannot be changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum SocdDisabled {
    /// `24 02` / `24 01` were not read.
    NotRead,
    /// The replies cannot be used.
    Unreadable {
        /// Why (technical English).
        message: String,
    },
    /// The keyboard is not a supported model.
    UnsupportedModel,
}

/// A resolution mode (the vendor site's numbers 1-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SocdModeView {
    /// 1 last input wins, 2 key 1 wins, 3 key 2 wins, 4 the later key only
    /// interrupts the first.
    pub mode: u8,
    /// Whether it is verified on a W75.
    pub evidence: EvidenceView,
}

/// A key of a SOCD pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SocdKeyView {
    /// Its HID usage (what the table stores).
    pub usage: u8,
    /// The layout key with that usage, if any.
    pub key: Option<KeyRef>,
}

/// One SOCD pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SocdPairView {
    /// Key 1.
    pub key1: SocdKeyView,
    /// Key 2.
    pub key2: SocdKeyView,
    /// The mode, 1-4.
    pub mode: u8,
}

/// Change SOCD: the power and/or the pairs (replacing the table).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SocdEditIntent {
    /// Switch SOCD on or off (every profile), if it changes.
    pub power: Option<bool>,
    /// The pairs of the active profile after the edit, if they change.
    pub pairs: Option<Vec<SocdPairIntent>>,
    /// Replace pairs that are not in the vendor site's form although the
    /// Undo cannot write them back.
    pub allow_not_undoable: bool,
    /// Check that the keyboard kept it (switches profiles; not pre-ticked).
    pub check_persistence: bool,
    /// `SocdStateView.revision`.
    pub based_on: u32,
}

/// One SOCD pair of an intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SocdPairIntent {
    /// Key 1 (key index).
    pub key1: u8,
    /// Key 2 (key index).
    pub key2: u8,
    /// The mode, 1-4.
    pub mode: u8,
}

/// A planned SOCD edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SocdEditPreview {
    /// The token to apply it; `null` when there is nothing to do.
    pub token: Option<PlanToken>,
    /// Nothing would be written.
    pub nothing_to_do: bool,
    /// The profile whose pairs are written.
    pub profile: u8,
    /// The power now.
    pub power_before: bool,
    /// The power after the edit (every profile).
    pub power_after: bool,
    /// The pairs now (empty when not in the vendor site's form).
    pub pairs_before: Vec<SocdPairView>,
    /// The pairs after the edit.
    pub pairs_after: Vec<SocdPairView>,
    /// The writes (one burst at most).
    pub writes: Vec<EditWriteView>,
    /// Write packets in total.
    pub packets: u16,
    /// What the user should know.
    pub warnings: Vec<EditWarningView>,
    /// The persistence check.
    pub persistence: PersistencePlanView,
}

// ----- trigger presets -----

/// The trigger presets (`list_presets`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PresetsView {
    /// The library's built-in presets.
    pub builtin: Vec<TriggerPresetView>,
    /// The user's presets (settings of this PC).
    pub user: Vec<UserPresetView>,
    /// The keyboard's limits they are checked against (`null` without a
    /// keyboard: the model's limits are used then).
    pub limits: Option<TriggerLimitsView>,
    /// The most user presets.
    pub max_user: u16,
}

/// A built-in preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TriggerPresetView {
    /// `typing`, `balanced`, `fast`, `competitive` or `factory`.
    pub id: String,
    /// Actuation point in trigger units.
    pub actuation: u16,
    /// Rapid Trigger.
    pub rapid_trigger: PresetRtView,
}

/// The Rapid Trigger setting of a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PresetRtView {
    /// Off.
    Off,
    /// Off, with both sensitivities set (the factory record).
    OffWith {
        /// Press sensitivity in trigger units.
        press: u16,
        /// Release sensitivity in trigger units.
        release: u16,
    },
    /// One sensitivity.
    Same {
        /// Sensitivity in trigger units.
        sensitivity: u16,
    },
    /// Separate press and release sensitivities.
    Separate {
        /// Press sensitivity in trigger units.
        press: u16,
        /// Release sensitivity in trigger units.
        release: u16,
    },
}

/// A preset of the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct UserPresetView {
    /// Its id (`u-<n>`).
    pub id: String,
    /// Its name.
    pub name: String,
    /// Actuation point in trigger units.
    pub actuation: u16,
    /// Rapid Trigger.
    pub rapid_trigger: RtIntent,
    /// Why it cannot be applied to the connected keyboard (a value out of
    /// its limits; refused, never clamped), if so.
    pub refused: Option<EditProblemView>,
}

/// Save a user preset (a new one, or replace the one with `id`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct UserPresetIntent {
    /// The preset to replace; `null` for a new one.
    pub id: Option<String>,
    /// Its name (1 to 40 characters).
    pub name: String,
    /// Actuation point in trigger units.
    pub actuation: u16,
    /// Rapid Trigger.
    pub rapid_trigger: RtIntent,
}

/// Apply a preset to keys: a trigger edit (`apply_trigger_edit` applies
/// the plan).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct PresetEditIntent {
    /// The preset.
    pub preset: PresetRef,
    /// Key indices (1 to 81 distinct keys, not the knob).
    pub keys: Vec<u8>,
    /// The snapshot revision the user saw.
    pub based_on: u32,
}

/// Which preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PresetRef {
    /// A built-in preset.
    Builtin {
        /// Its id.
        id: String,
    },
    /// A preset of the user.
    User {
        /// Its id.
        id: String,
    },
}
