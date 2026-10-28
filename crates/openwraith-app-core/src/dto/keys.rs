//! The Keys page: the keymap group of the active profile (both layers, the
//! ten macro slots, the forty advanced-key slots), keymap-group edits and
//! their plans, and the local macro library.
//!
//! Key usages are HID usages of the keyboard page (`0x07`) as numbers;
//! the action catalogue (`app/src/generated/actionCatalog.ts`) names them.
//! Distances of advanced keys are in the library's units: the DKS press
//! point in **0.1 mm**, the Mod-Tap hold time in milliseconds.

use serde::{Deserialize, Serialize};

use super::common::{
    CommitTriggerReasonView, KeyRef, LayerView, PlanToken, ResendReasonView, TableRef,
};

// ----- evidence -----

/// How well the effect of a value is verified on a W75 (the library's
/// `Evidence`). The library knows two levels: confirmed on hardware, or
/// sent by the vendor site but not yet verified. Values OpenWraith keeps as
/// read carry no evidence at all (they are never written).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EvidenceView {
    /// Captured from the vendor site and tested on a W75.
    Confirmed,
    /// The vendor site sends the same bytes; what the keyboard does is not
    /// verified ("Not yet verified" badge).
    Unverified {
        /// What is not verified, one note per open question.
        notes: Vec<UnverifiedNoteView>,
    },
}

/// One unverified aspect (technical text from the library).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct UnverifiedNoteView {
    /// What is not verified, e.g. `"DKS tap at stage 4"`.
    pub what: String,
    /// The open question of the protocol spec that tracks it, e.g. `"Q19"`.
    pub question: String,
}

// ----- actions -----

/// What a keymap entry does, in the closed set OpenWraith can write. Used
/// both in views (decoded entries) and in intents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionValue {
    /// No override: the key's default (on the Fn layer: its built-in Fn
    /// function, if any).
    Default,
    /// A keyboard key (HID usage the vendor site offers).
    Key {
        /// The HID usage.
        usage: u8,
    },
    /// A multimedia key (the vendor's index).
    Media {
        /// The multimedia index.
        index: u8,
    },
    /// A mouse button or the wheel.
    Mouse {
        /// The mouse action.
        action: MouseActionView,
    },
    /// A shortcut: one or two distinct modifiers (`E0`-`E7`), then one key
    /// that is not a modifier.
    Combo {
        /// The modifiers in pick order.
        modifiers: Vec<u8>,
        /// The key.
        key: u8,
    },
    /// Plays macro slot `slot` (0-based; the UI shows slot + 1).
    Macro {
        /// The slot.
        slot: u8,
    },
    /// Uses advanced-key slot `slot` (0-based; the UI shows slot + 1).
    Advanced {
        /// The slot.
        slot: u8,
    },
}

/// A mouse action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum MouseActionView {
    /// Left button (`12 00 01`).
    Left,
    /// Right button (`12 00 02`).
    Right,
    /// Middle button (`12 00 04`).
    Middle,
    /// Back button X1 (`12 00 08`).
    Back,
    /// Forward button X2 (`12 00 10`).
    Forward,
    /// Wheel up (`12 01 01`).
    WheelUp,
    /// Wheel down (`12 01 ff`).
    WheelDown,
}

// ----- the keys state -----

/// The keymap group of the active profile as the Keys page shows it
/// (`get_keys_state`). Built from the last full read, without traffic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeysState {
    /// The snapshot revision of the read this view comes from; keymap
    /// intents carry it as `based_on`. A later change of the snapshot that
    /// does not touch the keymap group (a trigger or light edit) keeps it.
    pub revision: u32,
    /// The connection it was read in.
    pub generation: u32,
    /// The profile it describes (the active one).
    pub profile: u8,
    /// When it was read (RFC 3339).
    pub read_at: String,
    /// Whether the keymap group can be edited.
    pub editable: bool,
    /// Why not.
    pub disabled: Option<KeysDisabled>,
    /// Whether advanced keys can be written (the keyboard's distance unit
    /// is 0.02 mm, so the DKS limits are known).
    pub advanced_editable: bool,
    /// Every key of the layout in index order (the knob and the Fn key
    /// included, never assignable).
    pub keys: Vec<KeyAssignmentsView>,
    /// The knob's keymap entries (the knob and the positions next to it,
    /// 15-21 on the W75), on both layers: kept as read, never changed.
    pub knob: Vec<KnobEntryView>,
    /// Macro slots 0-9.
    pub macros: Vec<MacroSlotStateView>,
    /// Advanced-key slots 0-39.
    pub advanced: Vec<AdvancedSlotStateView>,
    /// Advanced-key slots that hold content.
    pub advanced_used: u8,
    /// Keymap entries that refer to an empty slot or are malformed.
    pub broken: Vec<BrokenReferenceView>,
    /// Value ranges of the editors.
    pub limits: KeysLimitsView,
}

/// Why the keymap group cannot be edited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum KeysDisabled {
    /// A table of the group is missing or unreadable (`message` is
    /// technical text).
    UnreadableTable {
        /// The table.
        table: TableRef,
        /// Why.
        message: String,
    },
    /// The keyboard is not a supported model, or anything else (technical
    /// text).
    Unsupported {
        /// Why.
        message: String,
    },
}

/// What kind of key a layout position is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum KeyKindView {
    /// An ordinary key.
    Normal,
    /// The rotary knob (its entries are kept as read).
    Knob,
    /// The Fn key (never reassigned: without it the Fn layer and the
    /// keyboard's own shortcuts could not be reached).
    FnKey,
}

/// One key on both layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeyAssignmentsView {
    /// The key.
    pub key: KeyRef,
    /// Knob, Fn key or an ordinary key.
    pub kind: KeyKindView,
    /// The base-layer entry.
    pub base: KeyEntryView,
    /// The Fn-layer entry.
    pub fn_layer: KeyEntryView,
    /// The keyboard locks this key on the Fn layer (`0E`).
    pub fn_locked: bool,
    /// The built-in Fn function at this key, if any (shown on the Fn layer
    /// while the entry is the default).
    pub builtin: Option<FnBuiltinView>,
    /// Whether an assignment may go to the base layer.
    pub assignable_base: bool,
    /// Whether an assignment may go to the Fn layer (a built-in still needs
    /// its acknowledgement).
    pub assignable_fn: bool,
    /// Whether a macro can be bound to it (a macro entry carries the key's
    /// own HID usage).
    pub macro_capable: bool,
}

/// A built-in Fn function (what Fn + key does while the Fn-layer entry is
/// the default).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FnBuiltinView {
    /// The key index (the frontend translates the function by index).
    pub index: u8,
    /// English description from the library, e.g. `"profile 2"`.
    pub function: String,
    /// Fn + 1..4 (profiles) and Fn + Space (reset): replacing it needs the
    /// critical acknowledgement (danger callout + checkbox).
    pub critical: bool,
    /// Whether a hardware test confirmed the label (F1-F4, R-Shift);
    /// otherwise it is the vendor site's label.
    pub label_confirmed: bool,
}

/// Where an entry's action comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum EntryOriginView {
    /// `00 00 00 00`: the key's default.
    Default,
    /// Another key, media, mouse action or shortcut.
    Remapped,
    /// Bound to a macro slot.
    MacroSlot {
        /// The slot (0-based).
        slot: u8,
    },
    /// Bound to an advanced-key slot.
    AdvancedSlot {
        /// The slot (0-based).
        slot: u8,
    },
    /// An entry OpenWraith does not write (lighting, function, gamepad,
    /// unknown bytes...): kept as read unless replaced.
    Preserved,
}

/// One keymap entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeyEntryView {
    /// The layer.
    pub layer: LayerView,
    /// The stable action id (see the action catalogue; `"preserved:<hex>"`
    /// for entries kept as read).
    pub id: String,
    /// The action, for entries OpenWraith can write.
    pub value: Option<ActionValue>,
    /// English label (technical fallback), e.g. `"A"`, `"Mute"`,
    /// `"L-Ctrl+C"`, `"Macro 2"`.
    pub label: String,
    /// Where it comes from.
    pub origin: EntryOriginView,
    /// The 4 entry bytes (hex).
    pub hex: String,
    /// The library's evidence of this action on this layer; `null` for
    /// entries kept as read.
    pub evidence: Option<EvidenceView>,
    /// What a preserved entry is.
    pub preserved: Option<PreservedEntryView>,
}

/// An entry kept as read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PreservedEntryView {
    /// Its category.
    pub kind: PreservedKindView,
    /// The library's description (technical text).
    pub text: String,
}

/// The category of an entry kept as read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum PreservedKindView {
    /// The Fn function (`01`).
    FnKey,
    /// Lighting, brightness, effect speed, direction or colour (`02`-`06`).
    Lighting,
    /// Keyboard function (`11`: lock, Win lock, lights off).
    Function,
    /// Gamepad (`18`, `19`).
    Gamepad,
    /// A key usage the vendor site does not offer.
    Key,
    /// A multimedia index the vendor site does not offer.
    Media,
    /// A mouse entry in another form.
    Mouse,
    /// A combination in another form.
    Combo,
    /// A macro entry in another form.
    Macro,
    /// An advanced-key entry in another form.
    Advanced,
    /// Anything else.
    Unknown,
}

/// One keymap entry of the knob (kept as read).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KnobEntryView {
    /// The entry's index (15-21 on the W75).
    pub index: u8,
    /// The layer.
    pub layer: LayerView,
    /// The 4 entry bytes (hex).
    pub hex: String,
    /// The library's description (technical text).
    pub text: String,
    /// The decoded action, when the entry has one of the site's forms (for a
    /// translated text; the entry is kept as read either way).
    pub value: Option<ActionValue>,
    /// What the entry is when it has no such form.
    pub preserved: Option<PreservedEntryView>,
}

/// A key bound to a slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BindingView {
    /// The layer.
    pub layer: LayerView,
    /// The key (a position outside the layout shows as `#N`).
    pub key: KeyRef,
}

/// A macro slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MacroSlotStateView {
    /// The slot (0-based; the UI shows slot + 1).
    pub slot: u8,
    /// Its content.
    pub content: MacroSlotContentView,
    /// The keys bound to it.
    pub bindings: Vec<BindingView>,
    /// Content that no key refers to (the keyboard keeps it).
    pub orphan: bool,
}

/// What a macro slot holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MacroSlotContentView {
    /// Nothing (all `00` or all `FF`).
    Empty,
    /// A macro OpenWraith can show and edit.
    Program {
        /// The macro.
        program: MacroProgramView,
        /// Hash of the program (the library matches entries by it).
        content_hash: String,
        /// The library's evidence of the program.
        evidence: EvidenceView,
    },
    /// Content in another form (made by other software, mouse or media
    /// steps): kept as read ("non-canonical").
    KeptAsRead {
        /// The 256 slot bytes (hex).
        hex: String,
    },
}

/// A macro of keyboard steps (views and intents).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MacroProgramView {
    /// How it plays.
    pub play: MacroPlayView,
    /// 1-40 steps; every pressed key released by the end; the last step's
    /// delay is 0.
    pub steps: Vec<MacroStepView>,
}

/// How a macro plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum MacroPlayView {
    /// Once per press.
    Once,
    /// `count` times per press (1-255).
    Times {
        /// How often.
        count: u8,
    },
    /// One press starts, the next stops.
    Toggle,
    /// Repeats while held.
    WhileHeld,
}

/// One macro step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MacroStepView {
    /// The key (HID usage).
    pub usage: u8,
    /// Press or release.
    pub motion: KeyMotionView,
    /// Delay after the step, 0-65535 ms.
    pub delay_after_ms: u16,
}

/// Press or release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum KeyMotionView {
    /// Key down.
    Press,
    /// Key up.
    Release,
}

/// An advanced-key slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AdvancedSlotStateView {
    /// The slot (0-based; the UI shows slot + 1).
    pub slot: u8,
    /// Its content.
    pub content: AdvancedSlotContentView,
    /// The keys bound to it.
    pub bindings: Vec<BindingView>,
    /// Content that no key refers to (the keyboard keeps it).
    pub orphan: bool,
}

/// What an advanced-key slot holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AdvancedSlotContentView {
    /// Nothing (all zero).
    Empty,
    /// A DKS, Mod-Tap or Toggle OpenWraith can show and edit.
    Def {
        /// The advanced key.
        def: AdvancedDefView,
        /// The library's evidence of this definition.
        evidence: EvidenceView,
    },
    /// Another type or form (RS, RKRT, MDS, a mouse action...): kept as
    /// read.
    KeptAsRead {
        /// The type byte.
        type_byte: u8,
        /// The 40 slot bytes (hex).
        hex: String,
    },
}

/// An advanced key (views and intents).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdvancedDefView {
    /// Dynamic keystroke: up to four keys, each with an action at four
    /// stages of the key's travel.
    Dks {
        /// First press point in **0.1 mm** (1 to the deep press point,
        /// `limits.dks_first_point_max`).
        first_point: u8,
        /// Exactly four rows; `null` = unused.
        rows: Vec<Option<DksRowView>>,
    },
    /// Tap or hold.
    ModTap {
        /// The key sent when tapped (HID usage).
        tap: u8,
        /// The key sent when held (HID usage).
        hold: u8,
        /// Hold time in ms (150-1000, a multiple of 10).
        hold_ms: u16,
    },
    /// One press holds the key down, the next releases it.
    Toggle {
        /// The key (HID usage).
        key: u8,
    },
}

/// One DKS row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct DksRowView {
    /// The key the row sends (HID usage).
    pub key: u8,
    /// Exactly four stages: press past the first point, bottom-out, lift
    /// from the bottom, release past the first point.
    pub steps: Vec<DksStepView>,
}

/// What a DKS row does at one stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum DksStepView {
    /// Nothing.
    None,
    /// One press, no auto-repeat.
    Tap,
    /// Held while the key stays in this stage.
    Hold,
    /// Let go and press again (after a held stage).
    HoldAgain,
}

/// A keymap entry that refers to an empty slot, or is malformed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BrokenReferenceView {
    /// The layer.
    pub layer: LayerView,
    /// The key (a position outside the layout shows as `#N`).
    pub key: KeyRef,
    /// What is wrong.
    pub problem: BrokenKindView,
    /// The 4 entry bytes (hex).
    pub hex: String,
    /// Whether "reset key" can fix it (a layout key the app may change on
    /// this layer).
    pub resettable: bool,
}

/// What is wrong with a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrokenKindView {
    /// It refers to an empty macro or advanced-key slot: the key does
    /// nothing.
    EmptySlot {
        /// The slot.
        slot: TableRef,
    },
    /// A macro or advanced-key entry in neither form (technical text).
    Malformed {
        /// Which rule it breaks.
        reason: String,
    },
}

/// Value ranges of the Keys editors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeysLimitsView {
    /// Macro slots (10).
    pub macro_slots: u8,
    /// Advanced-key slots (40).
    pub advanced_slots: u8,
    /// Most steps of a macro (40).
    pub macro_max_steps: u8,
    /// Largest delay after a macro step (65535 ms).
    pub macro_max_delay_ms: u16,
    /// Largest repeat count (255).
    pub macro_max_times: u8,
    /// Largest DKS first press point in 0.1 mm (the deep press point, 30 on
    /// the W75); `null` when advanced keys cannot be written.
    pub dks_first_point_max: Option<u8>,
    /// Shortest Mod-Tap hold time (ms).
    pub mt_hold_ms_min: u16,
    /// Longest Mod-Tap hold time (ms).
    pub mt_hold_ms_max: u16,
    /// Mod-Tap hold times are multiples of this (ms).
    pub mt_hold_ms_step: u16,
}

// ----- intents -----

/// A keymap-group edit (`prepare_keymap_edit`): key assignments on both
/// layers, macros and advanced keys, written as one group in the vendor
/// site's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct KeymapEditIntent {
    /// The changes, applied in order (1-200).
    pub changes: Vec<KeymapChangeIntent>,
    /// The user's acknowledgements of replaced built-in Fn functions; a
    /// critical one (Fn + 1..4, Fn + Space) needs `critical`.
    pub builtin_acks: Vec<BuiltinAckIntent>,
    /// Keep the content of a slot whose last binding the edit removes
    /// (default: it is cleared in the same write).
    pub keep_orphans: bool,
    /// Allow overwriting or clearing slot content that the Undo (a restore
    /// of the safety backup) could not write back.
    pub allow_not_undoable: bool,
    /// Check that the keyboard kept it: switch to another profile and back
    /// and read the written tables again (not pre-ticked).
    pub check_persistence: bool,
    /// `KeysState::revision` of the view the user edited.
    pub based_on: u32,
}

/// The acknowledgement that an Fn-layer entry replaces a built-in function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct BuiltinAckIntent {
    /// The key index.
    pub key: u8,
    /// The user also ticked the danger checkbox of a critical built-in.
    pub critical: bool,
}

/// One change of a keymap-group edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "change", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeymapChangeIntent {
    /// An action for a key (an existing non-empty slot no other key uses
    /// for `macro` / `advanced`).
    Assign {
        /// The layer.
        layer: LayerView,
        /// The key index.
        key: u8,
        /// The action.
        action: ActionValue,
    },
    /// The key back to its default.
    Reset {
        /// The layer.
        layer: LayerView,
        /// The key index.
        key: u8,
    },
    /// Swaps the entries of two keys on a layer (both must be entries
    /// OpenWraith can write).
    Swap {
        /// The layer.
        layer: LayerView,
        /// The first key index.
        first: u8,
        /// The second key index.
        second: u8,
    },
    /// A macro and its binding; `slot` `null`: the slot only this key uses,
    /// else the lowest free one (1-9 first).
    BindMacro {
        /// The layer.
        layer: LayerView,
        /// The key index.
        key: u8,
        /// The macro.
        program: MacroProgramView,
        /// The slot, if chosen.
        slot: Option<u8>,
    },
    /// A macro slot's content only.
    SetMacro {
        /// The slot.
        slot: u8,
        /// The macro.
        program: MacroProgramView,
    },
    /// Empties a macro slot (refused while a key is bound to it).
    ClearMacro {
        /// The slot.
        slot: u8,
    },
    /// Unbinds every key bound to a macro slot and empties it, in one
    /// write.
    DeleteMacro {
        /// The slot.
        slot: u8,
    },
    /// An advanced key and its binding; `slot` `null`: the slot only this
    /// key uses, else the lowest free one.
    BindAdvanced {
        /// The layer.
        layer: LayerView,
        /// The key index.
        key: u8,
        /// The advanced key.
        def: AdvancedDefView,
        /// The slot, if chosen.
        slot: Option<u8>,
    },
    /// An advanced-key slot's content only.
    SetAdvanced {
        /// The slot.
        slot: u8,
        /// The advanced key.
        def: AdvancedDefView,
    },
    /// Empties an advanced-key slot (refused while a key is bound to it).
    ClearAdvanced {
        /// The slot.
        slot: u8,
    },
    /// Unbinds every key bound to an advanced-key slot and empties it, in
    /// one write.
    DeleteAdvanced {
        /// The slot.
        slot: u8,
    },
}

// ----- the plan -----

/// A planned keymap-group edit, for review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeymapEditPreview {
    /// The token to apply it; `null` when there is nothing to do.
    pub token: Option<PlanToken>,
    /// The keyboard already holds all of it.
    pub nothing_to_do: bool,
    /// The profile it writes to (the active one).
    pub profile: u8,
    /// The entries that change, in layer and index order.
    pub entry_changes: Vec<KeyEntryChangeView>,
    /// The slots whose content changes (re-sends not included).
    pub slot_changes: Vec<KeymapSlotChangeView>,
    /// Every write, in the order it is made, with why.
    pub writes: Vec<KeymapWriteView>,
    /// Write packets of the first send of every write.
    pub packets: u16,
    /// What the user should know (unverified values, replaced built-ins,
    /// kept orphans, ...).
    pub warnings: Vec<KeymapWarningView>,
    /// The persistence check.
    pub persistence: PersistencePlanView,
}

/// One entry that changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeyEntryChangeView {
    /// The layer.
    pub layer: LayerView,
    /// The key.
    pub key: KeyRef,
    /// The entry now.
    pub before: KeyEntryView,
    /// The entry after the edit.
    pub after: KeyEntryView,
}

/// One slot whose content changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeymapSlotChangeView {
    /// The slot (`macro` or `advanced_key`).
    pub slot: TableRef,
    /// The content now.
    pub before: SlotContentView,
    /// The content after the edit.
    pub after: SlotContentView,
    /// The library's description (technical text).
    pub summary: String,
}

/// A slot's content in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SlotContentView {
    /// Empty.
    Empty,
    /// A macro.
    Macro {
        /// The macro.
        program: MacroProgramView,
    },
    /// An advanced key.
    Advanced {
        /// The advanced key.
        def: AdvancedDefView,
    },
    /// Content kept as read (hex).
    KeptAsRead {
        /// The slot bytes.
        hex: String,
    },
}

/// One planned write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeymapWriteView {
    /// The table.
    pub table: TableRef,
    /// Packets of its first send.
    pub packets: u16,
    /// Set when this write is the keymap group's commit trigger (the Fn
    /// keymap), with the plan's own reason.
    pub commit_trigger: Option<CommitTriggerReasonView>,
    /// An unchanged Fn keymap re-sent as the commit trigger.
    pub resent_unchanged: bool,
    /// Set when it re-sends a macro slot whose content does not change
    /// (written again right after macro slot 0, which empties slots 1-9).
    pub resent: Option<ResendReasonView>,
    /// It completes a binding (or follows macro slot 0): it is written even
    /// after Stop.
    pub completes_binding: bool,
    /// It writes a macro slot 1-9 again after this edit's macro slot-0
    /// write.
    pub follows_macro_slot_0: bool,
    /// It empties the slot.
    pub clears: bool,
    /// The library's description (technical text).
    pub summary: String,
}

/// Something about a keymap plan the user should know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum KeymapWarningView {
    /// The Fn-layer entry replaces a built-in Fn function (acknowledged).
    FnBuiltinOverridden {
        /// The key.
        key: KeyRef,
        /// English description of the function.
        function: String,
        /// Fn + 1..4 or Fn + Space.
        critical: bool,
    },
    /// A written value is not yet verified on a W75.
    Unverified {
        /// The note.
        note: UnverifiedNoteView,
    },
    /// A malformed entry of a written layer is kept byte for byte.
    MalformedReferencePreserved {
        /// The layer.
        layer: LayerView,
        /// The key.
        key: KeyRef,
    },
    /// An entry of a written layer already referred to an empty slot; it is
    /// kept byte for byte.
    DanglingReferencePreserved {
        /// The layer.
        layer: LayerView,
        /// The key.
        key: KeyRef,
        /// The empty slot.
        slot: TableRef,
    },
    /// For a moment the key refers to the slot's old content (the layer is
    /// written before the advanced-key slot); the slot is written even
    /// after Stop.
    TransientBinding {
        /// The slot.
        slot: TableRef,
    },
    /// A slot whose last binding the edit removes keeps its content.
    OrphanSlotKept {
        /// The slot.
        slot: TableRef,
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

/// The persistence check of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PersistencePlanView {
    /// The user asked for it (`check_persistence`).
    pub requested: bool,
    /// The library forces it (never for the app's edits).
    pub forced: bool,
    /// It will run (requested or needed): the edit switches to
    /// `away_profile` and back after the save.
    pub will_run: bool,
    /// The tables it will read again.
    pub tables: Vec<TableRef>,
    /// The profile it switches to and back from.
    pub away_profile: u8,
}

// ----- refusals -----

/// Why a keymap-group edit was refused (`ErrorDetail::Keymap`). Nothing was
/// written. Key refusals name the key (`key.name`, index).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "problem", rename_all = "snake_case")]
pub enum KeymapProblemView {
    /// The intent has no changes.
    NoChanges,
    /// Too many changes in one edit.
    TooManyChanges {
        /// The most.
        max: u16,
    },
    /// The index is not a key of the layout.
    NotAKey {
        /// The index.
        index: u8,
    },
    /// No such slot.
    SlotOutOfRange {
        /// Macro or advanced.
        kind: SlotKindView,
        /// The slot given.
        slot: u8,
    },
    /// A value the vendor site does not offer (`what` is technical text:
    /// "HID usage", "multimedia index").
    NotOffered {
        /// What.
        what: String,
        /// The value.
        value: u8,
    },
    /// The shortcut breaks the rule (1-2 distinct modifiers, then one key
    /// that is not a modifier).
    InvalidCombo {
        /// Which rule (technical text).
        reason: String,
    },
    /// More macro steps than allowed.
    MacroTooLong {
        /// Steps given.
        steps: u16,
        /// The most.
        max: u16,
    },
    /// A key pressed twice, released without a press or still held at the
    /// end (`step` 0-based).
    MacroUnbalanced {
        /// The step.
        step: u16,
        /// Which rule (technical text).
        reason: String,
    },
    /// The last macro step has a delay after it.
    MacroLastDelay,
    /// A repeat count of 0.
    MacroTimesZero,
    /// A DKS needs exactly four rows of four stages.
    DksShape,
    /// The DKS first press point is out of range (0.1 mm).
    DksPoint {
        /// The value.
        value: u8,
        /// The largest.
        max: u8,
    },
    /// A DKS row breaks the step grammar (`row` 0-based; technical text).
    DksSteps {
        /// The row.
        row: u8,
        /// Which rule.
        reason: String,
    },
    /// The Mod-Tap hold time is out of range or not a multiple of 10 ms.
    MtHoldTime {
        /// The value (ms).
        value_ms: u16,
        /// The shortest (ms).
        min_ms: u16,
        /// The longest (ms).
        max_ms: u16,
    },
    /// A swapped entry is kept as read and cannot be moved.
    SwapNotWritable {
        /// The layer.
        layer: LayerView,
        /// The key.
        key: KeyRef,
    },
    /// The knob's entries are kept as read.
    Knob,
    /// The Fn key is never reassigned.
    FnKey,
    /// The keyboard locks this key on the Fn layer.
    FnLocked {
        /// The key.
        key: KeyRef,
    },
    /// The Fn-layer entry replaces a built-in function the user did not
    /// acknowledge (with `critical` for Fn + 1..4 and Fn + Space).
    BuiltinNotAcknowledged {
        /// The key.
        key: KeyRef,
        /// English description of the function.
        function: String,
        /// A critical built-in.
        critical: bool,
    },
    /// A macro needs a key with a HID usage.
    NoUsage {
        /// The key.
        key: KeyRef,
    },
    /// The keyboard is not a model OpenWraith writes to.
    UnknownModel,
    /// Every slot of the kind is in use.
    NoFreeSlot {
        /// Macro or advanced.
        kind: SlotKindView,
    },
    /// The slot is already bound to another key (one slot per key).
    SlotAlreadyBound {
        /// The slot.
        slot: TableRef,
        /// The other binding.
        by: BindingView,
    },
    /// A key would refer to a slot that is empty after the edit.
    BindingToEmptySlot {
        /// The binding.
        binding: BindingView,
        /// The slot.
        slot: TableRef,
    },
    /// A new macro or advanced key (no slot chosen) on a key whose slot is
    /// changed in the same edit: the library would put the new one into
    /// that slot, and one of the two changes would be lost.
    NewSlotReplacesEdit {
        /// The key's layer.
        layer: LayerView,
        /// The key.
        key: KeyRef,
        /// The slot the key runs now, whose change would be lost.
        slot: TableRef,
    },
    /// A slot would be emptied while keys still refer to it.
    SlotStillReferenced {
        /// The slot.
        slot: TableRef,
        /// The keys.
        by: Vec<BindingView>,
    },
    /// A table cannot be edited as it is: unreadable, or its content could
    /// not be written back by the Undo (then `allow_not_undoable` lets the
    /// user overwrite it; `reason` is technical text).
    UnsupportedCurrentTable {
        /// The table.
        table: TableRef,
        /// Why.
        reason: String,
    },
    /// Anything else (technical text).
    Other {
        /// Why.
        message: String,
    },
}

/// Which kind of slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum SlotKindView {
    /// Macro slots (10).
    Macro,
    /// Advanced-key slots (40).
    Advanced,
}

// ----- the local macro library -----

/// The macro library of this PC ("Library (this PC)"): named macros, kept
/// in a small JSON file in the app's data directory, matched to the
/// keyboard's slots by content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MacroLibraryView {
    /// The macros, in the order they were saved.
    pub entries: Vec<LibraryMacroView>,
    /// Something happened while loading the file.
    pub notice: Option<LibraryNotice>,
}

/// One macro of the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LibraryMacroView {
    /// Hash of the program (its id; equal to a keyboard slot's
    /// `content_hash` when the slot holds the same macro).
    pub content_hash: String,
    /// The user's name for it.
    pub name: String,
    /// The macro.
    pub program: MacroProgramView,
    /// When it was saved (RFC 3339).
    pub saved_at: String,
}

/// What happened while loading the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "notice", rename_all = "snake_case")]
pub enum LibraryNotice {
    /// The file could not be read; it was kept under another name and the
    /// library starts empty.
    InvalidFileKept {
        /// The name it was kept as (`macro-library.invalid-<stamp>.json`).
        kept_as: String,
    },
}
