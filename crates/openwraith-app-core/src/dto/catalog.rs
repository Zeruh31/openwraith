//! The action catalogue of the ActionPicker: every action OpenWraith can
//! put on a key, generated from the protocol crate by the tests into
//! `app/src/generated/actionCatalog.ts` (no hand-written list in the
//! frontend). Macros and advanced keys come from the keys state (they are
//! the keyboard's slots); this catalogue gives their binding rules.

use serde::Serialize;

use super::common::LayerView;
use super::keys::{ActionValue, DksStepView, EvidenceView, SlotKindView};

/// Everything the ActionPicker offers, with the rules of each kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ActionCatalog {
    /// The keyboard model's product code (`"W75HEARGB"`).
    pub product: String,
    /// The fixed actions: default, keys, media, mouse.
    pub entries: Vec<CatalogEntry>,
    /// The keys of the layout with a HID usage and the action id of that
    /// usage (the "pick on the board" view of the Keys tab).
    pub layout_keys: Vec<LayoutKeyActionView>,
    /// The modifiers of the shortcut builder (`E0`-`E7`).
    pub modifiers: Vec<CatalogModifier>,
    /// The shortcut rule.
    pub combo: ComboRuleView,
    /// Where macro and advanced-key bindings may go, and their evidence.
    pub bindings: Vec<BindingRuleView>,
    /// Every DKS row the step grammar allows (stages 1-4), with the
    /// library's evidence.
    pub dks_rows: Vec<DksRowRuleView>,
    /// The Mod-Tap hold time.
    pub mod_tap: ModTapRuleView,
    /// Macro limits and play modes.
    pub macros: MacroRulesView,
}

/// A key of the layout and its own action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LayoutKeyActionView {
    /// The key index.
    pub index: u8,
    /// The action id of the key's own HID usage (`"key:04"`).
    pub id: String,
}

/// A tab of the ActionPicker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum CatalogTab {
    Keys,
    Media,
    Mouse,
    Shortcuts,
    Macros,
    Advanced,
    Default,
}

/// A group inside a tab (by HID usage range or function).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum CatalogGroup {
    Default,
    Letters,
    Digits,
    Basic,
    Symbols,
    Function,
    System,
    Navigation,
    Arrows,
    Keypad,
    International,
    Modifiers,
    Playback,
    Browser,
    Apps,
    MouseButtons,
    MouseWheel,
}

/// One fixed action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CatalogEntry {
    /// Stable id (`"default"`, `"key:04"`, `"media:12"`, `"mouse:back"`).
    pub id: String,
    /// The tab.
    pub tab: CatalogTab,
    /// The group.
    pub group: CatalogGroup,
    /// The action (sent in intents as is).
    pub value: ActionValue,
    /// English name (the library's), e.g. `"Caps Lock"`, `"Mute"`.
    pub name: String,
    /// Search aliases: `KeyboardEvent.code` names and the usage or index
    /// as hex (`"0x04"`).
    pub aliases: Vec<String>,
    /// The 4 entry bytes it writes (hex; a macro entry differs per key).
    pub hex: String,
    /// The layers it may go on, each with the library's evidence.
    pub layers: Vec<CatalogLayerView>,
    /// A modifier (`E0`-`E7`).
    pub modifier: bool,
    /// May be the key of a shortcut (a usage that is not a modifier).
    pub combo_key: bool,
}

/// A layer an action may go on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CatalogLayerView {
    /// The layer.
    pub layer: LayerView,
    /// The library's evidence of the action there.
    pub evidence: EvidenceView,
}

/// A modifier of the shortcut builder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CatalogModifier {
    /// The HID usage (`E0`-`E7`).
    pub usage: u8,
    /// English name (`"L-Ctrl"`).
    pub name: String,
    /// The evidence of a shortcut with this modifier: a shortcut is
    /// unverified when any of its modifiers is (the library's rule; the plan
    /// says so again).
    pub combo_evidence: EvidenceView,
}

/// The shortcut rule (the vendor UI's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ComboRuleView {
    /// Fewest modifiers (1).
    pub min_modifiers: u8,
    /// Most modifiers (2, distinct).
    pub max_modifiers: u8,
}

/// Where a slot binding may go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BindingRuleView {
    /// Macro or advanced key.
    pub kind: SlotKindView,
    /// The layers, each with the library's evidence.
    pub layers: Vec<CatalogLayerView>,
}

/// A grammar-valid DKS row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DksRowRuleView {
    /// Stages 1-4.
    pub steps: Vec<DksStepView>,
    /// The library's evidence of a DKS with this row.
    pub evidence: EvidenceView,
}

/// The Mod-Tap hold time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ModTapRuleView {
    /// Shortest (ms).
    pub min_ms: u16,
    /// Longest (ms).
    pub max_ms: u16,
    /// Hold times are multiples of this (ms).
    pub step_ms: u16,
    /// The default of a new Mod-Tap (ms).
    pub default_ms: u16,
    /// The hold times confirmed on hardware.
    pub confirmed_ms: Vec<u16>,
    /// The evidence of every other hold time.
    pub other_evidence: EvidenceView,
}

/// Macro limits and play modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MacroRulesView {
    /// Most steps (40).
    pub max_steps: u8,
    /// Largest delay after a step (ms).
    pub max_delay_ms: u16,
    /// Largest repeat count.
    pub max_times: u8,
    /// The play modes with the library's evidence.
    pub play_modes: Vec<PlayModeRuleView>,
}

/// A macro play mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PlayModeRuleView {
    /// The mode (the `mode` tag of `MacroPlayView`).
    pub mode: MacroPlayKindView,
    /// The library's evidence.
    pub evidence: EvidenceView,
}

/// A macro play mode without its count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum MacroPlayKindView {
    Once,
    Times,
    Toggle,
    WhileHeld,
}
