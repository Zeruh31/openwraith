//! Keymap entries for writes (spec section 4.8.3).
//!
//! [`KeyAction`] stays the read model: it decodes every entry, including the
//! categories OpenWraith never writes (Fn `01`, lighting `02`-`06`,
//! function `11`, gamepad `18`/`19`, unknown bytes). Writes use the closed
//! [`KeyAssignment`], which can only express the categories confirmed on
//! hardware: keyboard key `00`, multimedia `07`, mouse `12` (kind first),
//! combination `13`, macro `10` and advanced key `17`. Each payload is a
//! validated atom ([`HidUsage`], [`MediaKey`], [`MouseAction`], [`Combo`])
//! restricted to the values the vendor configurator offers. An entry of any
//! other form is kept as read ([`EntryKind::Preserved`]).
//!
//! [`check_placement`] decides whether an assignment may go to a key and
//! layer: never the knob or the Fn key, never a key the keyboard locks on
//! the Fn layer (`0E`), and a built-in Fn function only with the user's
//! explicit [`BuiltinAck`].

use std::fmt;

use super::evidence::Evidence;
use super::ids::AdvancedSlotId;
use crate::decode::KeyAction;
use crate::error::WriteBuildError;
use crate::layout::{KeyMask, KeyPos};
use crate::model::{FnBuiltin, Model, known_model};
use crate::request::{Layer, MacroSlotId};
use crate::usage::{hid_usage_name, media_name};

/// A keyboard usage (page 0x07) from the set the vendor configurator offers
/// (spec section 4.8.3): `04`-`31`, `33`-`38` (`32` is not offered),
/// `39`-`45`, `46`-`4E`, `4F`-`52`, `53`-`63`, `64`, `65`, `68`-`73`,
/// `87`-`8B` and `E0`-`E7`.
///
/// ```
/// use openwraith_protocol::write::HidUsage;
/// assert_eq!(HidUsage::new(0x04).map(HidUsage::get), Some(0x04));
/// assert!(HidUsage::new(0x32).is_none());
/// assert!(HidUsage::new(0xE1).is_some_and(HidUsage::is_modifier));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
pub struct HidUsage(u8);

/// Whether the vendor configurator offers `usage` (spec section 4.8.3).
const fn is_offered_usage(usage: u8) -> bool {
    matches!(
        usage,
        0x04..=0x31 | 0x33..=0x65 | 0x68..=0x73 | 0x87..=0x8B | 0xE0..=0xE7
    )
}

impl HidUsage {
    /// The usage, or `None` if the vendor configurator does not offer it.
    pub const fn new(usage: u8) -> Option<Self> {
        if is_offered_usage(usage) {
            Some(HidUsage(usage))
        } else {
            None
        }
    }

    /// The usage, or [`WriteBuildError::NotOffered`].
    pub fn try_new(usage: u8) -> Result<Self, WriteBuildError> {
        HidUsage::new(usage).ok_or(WriteBuildError::NotOffered {
            what: "HID usage",
            value: usage,
        })
    }

    /// The usage byte.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether this is a modifier (`E0`-`E7`).
    pub const fn is_modifier(self) -> bool {
        matches!(self.0, 0xE0..=0xE7)
    }

    /// The usage's name, e.g. `"A"` or `"L-Ctrl"`.
    pub fn name(self) -> Option<&'static str> {
        hid_usage_name(self.0)
    }

    /// Every offered usage in ascending order.
    pub fn all() -> impl Iterator<Item = HidUsage> {
        (0..=u8::MAX).filter_map(HidUsage::new)
    }
}

impl TryFrom<u8> for HidUsage {
    type Error = String;

    fn try_from(usage: u8) -> Result<Self, Self::Error> {
        HidUsage::new(usage).ok_or_else(|| format!("HID usage {usage:#04x} is not offered"))
    }
}

impl From<HidUsage> for u8 {
    fn from(usage: HidUsage) -> u8 {
        usage.0
    }
}

impl fmt::Display for HidUsage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => write!(f, "HID {:#04x}", self.0),
        }
    }
}

/// A multimedia index (`07 ii 00 00`) the vendor configurator offers:
/// `10`-`16`, `20`-`27`, `30`, `32`, `34` (spec section 4.8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
pub struct MediaKey(u8);

impl MediaKey {
    /// Every offered index in ascending order.
    pub const ALL: [u8; 18] = [
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
        0x30, 0x32, 0x34,
    ];

    /// The index, or `None` if it is not offered.
    pub const fn new(index: u8) -> Option<Self> {
        if matches!(index, 0x10..=0x16 | 0x20..=0x27 | 0x30 | 0x32 | 0x34) {
            Some(MediaKey(index))
        } else {
            None
        }
    }

    /// The index byte.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The name, e.g. `"Mute"` (the vendor's label; `24`/`25` by what they
    /// do on a W75, spec section 4.8.3).
    pub fn name(self) -> Option<&'static str> {
        media_name(self.0)
    }

    /// Mute (`12`) and Calculator (`32`) are CONFIRMED (c17), Browser Back
    /// (`24`) and Browser Forward (`25`) by hardware test H18 (2026-10-27,
    /// Q14); the others rely on the vendor's label only (Q37).
    pub const fn evidence(self) -> Evidence {
        match self.0 {
            0x12 | 0x24 | 0x25 | 0x32 => Evidence::Confirmed,
            _ => Evidence::unverified("multimedia function named by the vendor site", "Q37"),
        }
    }
}

impl TryFrom<u8> for MediaKey {
    type Error = String;

    fn try_from(index: u8) -> Result<Self, Self::Error> {
        MediaKey::new(index).ok_or_else(|| format!("multimedia index {index:#04x} is not offered"))
    }
}

impl From<MediaKey> for u8 {
    fn from(key: MediaKey) -> u8 {
        key.0
    }
}

/// A mouse button mask of a `12 00 <mask> 00` entry.
///
/// Mask `08` is the back button (X1) and `10` the forward button (X2), as
/// in standard HID numbering: CONFIRMED by hardware test H18 (2026-10-27,
/// Q37). The vendor site labels them the other way round (spec section
/// 4.8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MouseButton {
    /// `01`: left button.
    Left,
    /// `02`: right button.
    Right,
    /// `04`: middle button.
    Middle,
    /// `08`: back (X1; the site's "Forward").
    Back,
    /// `10`: forward (X2; the site's "Back").
    Forward,
}

impl MouseButton {
    /// The five buttons.
    pub const ALL: [MouseButton; 5] = [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
    ];

    /// The button mask byte.
    pub const fn mask(self) -> u8 {
        match self {
            MouseButton::Left => 0x01,
            MouseButton::Right => 0x02,
            MouseButton::Middle => 0x04,
            MouseButton::Back => 0x08,
            MouseButton::Forward => 0x10,
        }
    }

    /// The button with mask `mask`, if it is one of the five.
    pub const fn from_mask(mask: u8) -> Option<Self> {
        match mask {
            0x01 => Some(MouseButton::Left),
            0x02 => Some(MouseButton::Right),
            0x04 => Some(MouseButton::Middle),
            0x08 => Some(MouseButton::Back),
            0x10 => Some(MouseButton::Forward),
            _ => None,
        }
    }
}

/// A mouse entry, written kind first: `12 00 <mask> 00` for a button,
/// `12 01 01 00` / `12 01 ff 00` for the wheel (CONFIRMED order, c17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MouseAction {
    /// A button.
    Button(MouseButton),
    /// Wheel up (`12 01 01 00`).
    WheelUp,
    /// Wheel down (`12 01 ff 00`).
    WheelDown,
}

impl MouseAction {
    /// The 4 entry bytes.
    pub const fn bytes(self) -> [u8; 4] {
        match self {
            MouseAction::Button(button) => [0x12, 0x00, button.mask(), 0x00],
            MouseAction::WheelUp => [0x12, 0x01, 0x01, 0x00],
            MouseAction::WheelDown => [0x12, 0x01, 0xFF, 0x00],
        }
    }

    /// The action whose [`bytes`](Self::bytes) are `bytes`, if any.
    pub const fn from_bytes(bytes: [u8; 4]) -> Option<Self> {
        match bytes {
            [0x12, 0x00, mask, 0x00] => match MouseButton::from_mask(mask) {
                Some(button) => Some(MouseAction::Button(button)),
                None => None,
            },
            [0x12, 0x01, 0x01, 0x00] => Some(MouseAction::WheelUp),
            [0x12, 0x01, 0xFF, 0x00] => Some(MouseAction::WheelDown),
            _ => None,
        }
    }

    /// Every action is CONFIRMED: wheel down and forward were written and
    /// reported working in c17; the left, right, middle, back and forward
    /// buttons and wheel up were tested one by one in hardware test H18
    /// (2026-10-27, Q37).
    pub const fn evidence(self) -> Evidence {
        Evidence::Confirmed
    }
}

/// A key combination: one or two distinct modifiers, then exactly one key
/// that is not a modifier (the vendor UI's rule, `wmIndex.js:3563-3577`).
///
/// Written as `13 m1 m2 key` with two modifiers (CONFIRMED, c17
/// `13 e0 e1 04`) and `13 m key 00` with one (CONFIRMED, hardware test H18:
/// `13 e0 06` = L-Ctrl+C copies).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Combo {
    modifiers: (HidUsage, Option<HidUsage>),
    key: HidUsage,
}

impl Combo {
    /// Checks the site's rule: 1-2 distinct modifiers and a non-modifier key.
    pub fn new(modifiers: &[HidUsage], key: HidUsage) -> Result<Combo, WriteBuildError> {
        let invalid = |reason| WriteBuildError::InvalidCombo { reason };
        if key.is_modifier() {
            return Err(invalid("the last key must not be a modifier"));
        }
        if modifiers.iter().any(|m| !m.is_modifier()) {
            return Err(invalid("only modifiers may come before the key"));
        }
        match *modifiers {
            [first] => Ok(Combo {
                modifiers: (first, None),
                key,
            }),
            [first, second] if first != second => Ok(Combo {
                modifiers: (first, Some(second)),
                key,
            }),
            [_, _] => Err(invalid("the two modifiers are the same")),
            [] => Err(invalid("at least one modifier is needed")),
            _ => Err(invalid("at most two modifiers are allowed")),
        }
    }

    /// The modifiers in pick order.
    pub fn modifiers(&self) -> Vec<HidUsage> {
        let (first, second) = self.modifiers;
        std::iter::once(first).chain(second).collect()
    }

    /// The key that is not a modifier.
    pub const fn key(&self) -> HidUsage {
        self.key
    }

    /// The 4 entry bytes.
    pub const fn bytes(&self) -> [u8; 4] {
        match self.modifiers {
            (first, Some(second)) => [0x13, first.get(), second.get(), self.key.get()],
            (first, None) => [0x13, first.get(), self.key.get(), 0x00],
        }
    }

    /// The combination whose [`bytes`](Self::bytes) are `bytes`, if any.
    pub fn from_bytes(bytes: [u8; 4]) -> Option<Combo> {
        let usage = |b| HidUsage::new(b);
        let combo = match bytes {
            [0x13, m, k, 0x00] => Combo::new(&[usage(m)?], usage(k)?),
            [0x13, m1, m2, k] => Combo::new(&[usage(m1)?, usage(m2)?], usage(k)?),
            _ => return None,
        };
        combo.ok().filter(|c| c.bytes() == bytes)
    }

    /// Left-side modifiers (`E0`-`E3`): CONFIRMED, two by c17 (`13 e0 e1
    /// 04`) and one by hardware test H18 (`13 e0 06`); a right-side modifier
    /// (`E4`-`E7`) was never tested on a W75 (Q37).
    pub const fn evidence(&self) -> Evidence {
        let (first, second) = self.modifiers;
        let right_side = match second {
            Some(second) => is_right_side(first) || is_right_side(second),
            None => is_right_side(first),
        };
        if right_side {
            Evidence::unverified("key combination with a right-side modifier", "Q37")
        } else {
            Evidence::Confirmed
        }
    }
}

/// Whether `modifier` is a right-side modifier (`E4`-`E7`: R-Ctrl,
/// R-Shift, R-Alt, R-GUI).
const fn is_right_side(modifier: HidUsage) -> bool {
    modifier.get() >= 0xE4
}

impl fmt::Display for Combo {
    /// E.g. `L-Ctrl+L-Shift+A`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for modifier in self.modifiers() {
            write!(f, "{modifier}+")?;
        }
        write!(f, "{}", self.key)
    }
}

/// What a keymap entry can be set to (the CONFIRMED categories of spec
/// section 4.8.3).
///
/// | Variant | Bytes |
/// |---|---|
/// | `Default` | `00 00 00 00` |
/// | `Key` | `00 uu 00 00` |
/// | `Media` | `07 ii 00 00` |
/// | `Mouse` | `12 kk vv 00` (kind first) |
/// | `Combo` | `13 m1 m2 key` / `13 m key 00` |
/// | `Macro` | `10 <the key's own usage> SS 00` |
/// | `Advanced` | `17 SS SS 00` |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum KeyAssignment {
    /// No override: the firmware default (and, on the Fn layer, the
    /// built-in Fn function) applies.
    Default,
    /// A keyboard key.
    Key(HidUsage),
    /// A multimedia key.
    Media(MediaKey),
    /// A mouse button or wheel.
    Mouse(MouseAction),
    /// A key combination.
    Combo(Combo),
    /// Plays a macro slot.
    Macro(MacroSlotId),
    /// Uses an advanced-key slot.
    Advanced(AdvancedSlotId),
}

impl KeyAssignment {
    /// The entry bytes for the key at `pos` of `model`. A macro entry
    /// carries the key's own usage in b1 (`10 39 00 00` for Caps, c13), so
    /// it fails for a key without one or a model this crate does not know.
    pub fn entry(&self, model: &Model, pos: KeyPos) -> Result<[u8; 4], WriteBuildError> {
        Ok(match *self {
            KeyAssignment::Default => [0x00; 4],
            KeyAssignment::Key(usage) => [0x00, usage.get(), 0x00, 0x00],
            KeyAssignment::Media(key) => [0x07, key.get(), 0x00, 0x00],
            KeyAssignment::Mouse(action) => action.bytes(),
            KeyAssignment::Combo(combo) => combo.bytes(),
            KeyAssignment::Macro(slot) => [0x10, own_usage(model, pos)?, slot.get(), 0x00],
            KeyAssignment::Advanced(slot) => [0x17, slot.get(), slot.get(), 0x00],
        })
    }

    /// Classifies the entry `bytes` of the key at `pos`:
    /// [`EntryKind::Assignable`] only when an assignment produces exactly
    /// these bytes, anything else (Fn `01`, lighting, function, gamepad,
    /// unknown bytes, a macro entry whose b1 is not the key's usage, a
    /// value the site does not offer) is [`EntryKind::Preserved`].
    pub fn classify(model: &Model, pos: KeyPos, bytes: [u8; 4]) -> EntryKind {
        let candidate = match bytes {
            [0x00, 0x00, 0x00, 0x00] => Some(KeyAssignment::Default),
            [0x00, usage, 0x00, 0x00] => HidUsage::new(usage).map(KeyAssignment::Key),
            [0x07, index, 0x00, 0x00] => MediaKey::new(index).map(KeyAssignment::Media),
            [0x12, ..] => MouseAction::from_bytes(bytes).map(KeyAssignment::Mouse),
            [0x13, ..] => Combo::from_bytes(bytes).map(KeyAssignment::Combo),
            [0x10, _, slot, 0x00] => MacroSlotId::new(slot).map(KeyAssignment::Macro),
            [0x17, slot, again, 0x00] if slot == again => {
                AdvancedSlotId::new(slot).map(KeyAssignment::Advanced)
            }
            _ => None,
        };
        match candidate {
            Some(assignment) if assignment.entry(model, pos).ok() == Some(bytes) => {
                EntryKind::Assignable(assignment)
            }
            _ => EntryKind::Preserved(KeyAction::decode(bytes)),
        }
    }

    /// How well the effect of this assignment on `layer` is verified. A
    /// macro binding on the Fn layer is CONFIRMED by hardware test H19
    /// (2026-10-27: Fn + Q, `10 14 04 00`, plays the macro); an advanced-key
    /// binding there is not tested (Q36).
    pub const fn evidence(&self, layer: Layer) -> Evidence {
        match (self, layer) {
            (KeyAssignment::Default | KeyAssignment::Key(_), _) => Evidence::Confirmed,
            (KeyAssignment::Media(key), _) => key.evidence(),
            (KeyAssignment::Mouse(action), _) => action.evidence(),
            (KeyAssignment::Combo(combo), _) => combo.evidence(),
            (KeyAssignment::Macro(_), _) | (KeyAssignment::Advanced(_), Layer::Base) => {
                Evidence::Confirmed
            }
            (KeyAssignment::Advanced(_), Layer::Fn) => {
                Evidence::unverified("advanced-key binding on the Fn layer", "Q36")
            }
        }
    }
}

/// The usage of the key at `pos` of the known model with `model`'s product
/// code (b1 of a macro entry).
fn own_usage(model: &Model, pos: KeyPos) -> Result<u8, WriteBuildError> {
    let known = known_model(model).ok_or(WriteBuildError::UnknownModel {
        product: model.product_code,
    })?;
    known
        .key_at(pos)
        .and_then(|key| key.hid_usage)
        .ok_or(WriteBuildError::Placement(PlacementError::NoUsage {
            index: pos.index(),
        }))
}

/// A keymap entry as an editor sees it ([`KeyAssignment::classify`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EntryKind {
    /// An entry an assignment produces exactly; it may be changed.
    Assignable(KeyAssignment),
    /// Any other entry: kept byte for byte unless the user replaces it.
    Preserved(KeyAction),
}

/// The user's explicit acknowledgement that an Fn-layer entry at `pos`
/// replaces a built-in Fn function. `critical` must be set for the critical
/// ones (Fn + 1..4, Fn + Space).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BuiltinAck {
    /// The key whose built-in function is replaced.
    pub pos: KeyPos,
    /// Whether the user also acknowledged that it is a critical one.
    pub critical: bool,
}

/// Why an assignment cannot go to a key and layer ([`check_placement`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case", tag = "kind"))]
pub enum PlacementError {
    /// The model is not one this crate knows.
    #[error("unknown keyboard model {product:?}: nothing can be written to it")]
    UnknownModel {
        /// The product code of the model that was passed.
        product: &'static str,
    },
    /// The position is not a key of the model.
    #[error("position #{index} is not a key of this keyboard")]
    NotAKey {
        /// Key index.
        index: u8,
    },
    /// The knob's entries (15-21) are kept as read.
    #[error("the knob's keymap entries are kept as read")]
    Knob,
    /// The Fn key (121) is never reassigned, on either layer.
    #[error("the Fn key cannot be reassigned")]
    FnKey,
    /// The keyboard locks this key on the Fn layer (`0E` mask).
    #[error("key #{index} cannot be changed on the Fn layer (locked by the keyboard)")]
    FnLocked {
        /// Key index.
        index: u8,
    },
    /// The Fn-layer entry would hide a built-in Fn function the user did
    /// not acknowledge (or acknowledged without the critical flag).
    #[error(
        "Fn + {key} (#{index}) is the built-in function \"{function}\"; replacing it needs an explicit acknowledgement{}",
        critical_suffix(.critical)
    )]
    BuiltinNotAcknowledged {
        /// Key index.
        index: u8,
        /// The key's name in the model's layout, e.g. `"F1"`.
        key: &'static str,
        /// The built-in function.
        function: &'static str,
        /// Whether it is a critical one.
        critical: bool,
    },
    /// A macro binding needs the key's own usage, which this key lacks.
    #[error("key #{index} has no HID usage, so no macro can be bound to it")]
    NoUsage {
        /// Key index.
        index: u8,
    },
}

/// The end of the [`PlacementError::BuiltinNotAcknowledged`] message.
fn critical_suffix(critical: &bool) -> &'static str {
    if *critical {
        " for a critical function"
    } else {
        ""
    }
}

/// Checks that `target` may be written to the key at `pos` on `layer`.
///
/// * `pos` must be a key of `model` (the model of [`MODELS`](crate::MODELS)
///   with the same product code), never the knob or the Fn key, on either
///   layer;
/// * on the Fn layer, never a key of the keyboard's Fn-locked mask
///   `fn_locked` (`0E`);
/// * on the Fn layer, anything but [`KeyAssignment::Default`] at a key with
///   a built-in function needs a matching [`BuiltinAck`] in `acks`, with
///   `critical` set for a critical built-in. `Default` never needs one: it
///   brings the built-in back;
/// * a macro binding needs a key with a HID usage.
///
/// Returns the built-in function the entry hides (for plan warnings).
pub fn check_placement(
    model: &Model,
    layer: Layer,
    pos: KeyPos,
    target: &KeyAssignment,
    fn_locked: &KeyMask,
    acks: &[BuiltinAck],
) -> Result<Option<&'static FnBuiltin>, PlacementError> {
    let known = known_model(model).ok_or(PlacementError::UnknownModel {
        product: model.product_code,
    })?;
    let index = pos.index();
    let key = known.key(index).ok_or(PlacementError::NotAKey { index })?;
    if known.knob_index == Some(index) {
        return Err(PlacementError::Knob);
    }
    if known.fn_index == Some(index) {
        return Err(PlacementError::FnKey);
    }
    if matches!(target, KeyAssignment::Macro(_)) && key.hid_usage.is_none() {
        return Err(PlacementError::NoUsage { index });
    }
    if layer == Layer::Base {
        return Ok(None);
    }
    if fn_locked.contains(pos) {
        return Err(PlacementError::FnLocked { index });
    }
    if *target == KeyAssignment::Default {
        return Ok(None);
    }
    let Some(builtin) = known.fn_builtin(index) else {
        return Ok(None);
    };
    let acknowledged = acks
        .iter()
        .any(|ack| ack.pos == pos && (ack.critical || !builtin.critical));
    if acknowledged {
        Ok(Some(builtin))
    } else {
        Err(PlacementError::BuiltinNotAcknowledged {
            index,
            key: key.name,
            function: builtin.function,
            critical: builtin.critical,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::W75;

    fn usage(u: u8) -> HidUsage {
        HidUsage::new(u).unwrap()
    }

    fn key(name: &str) -> KeyPos {
        W75.key_by_name(name).and_then(|k| k.pos()).unwrap()
    }

    #[test]
    fn offered_usages() {
        let all: Vec<u8> = HidUsage::all().map(HidUsage::get).collect();
        // 04-31 (46), 33-65 (51), 68-73 (12), 87-8B (5), E0-E7 (8).
        assert_eq!(all.len(), 46 + 51 + 12 + 5 + 8);
        for rejected in [
            0x00, 0x01, 0x03, 0x32, 0x66, 0x67, 0x74, 0x86, 0x8C, 0xDF, 0xE8, 0xFF,
        ] {
            assert!(HidUsage::new(rejected).is_none(), "{rejected:#04x}");
            assert_eq!(
                HidUsage::try_new(rejected),
                Err(WriteBuildError::NotOffered {
                    what: "HID usage",
                    value: rejected
                })
            );
        }
        // Every offered usage has a name, and every W75 key's usage is offered.
        assert!(HidUsage::all().all(|u| u.name().is_some()));
        for k in W75.keys {
            if let Some(u) = k.hid_usage {
                assert!(HidUsage::new(u).is_some(), "{}", k.name);
            }
        }
        assert_eq!(usage(0x04).to_string(), "A");
        assert!(!usage(0x39).is_modifier());
    }

    #[test]
    fn media_keys() {
        assert!(MediaKey::ALL.iter().all(|i| MediaKey::new(*i).is_some()));
        let count = (0..=u8::MAX)
            .filter(|i| MediaKey::new(*i).is_some())
            .count();
        assert_eq!(count, MediaKey::ALL.len());
        assert!(MediaKey::new(0x17).is_none());
        assert!(MediaKey::ALL.iter().all(|i| media_name(*i).is_some()));
        assert!(MediaKey::new(0x32).unwrap().evidence().is_confirmed());
        // H18: 24 = Browser Back, 25 = Browser Forward.
        for (index, name) in [(0x24, "Browser Back"), (0x25, "Browser Forward")] {
            let key = MediaKey::new(index).unwrap();
            assert!(key.evidence().is_confirmed(), "{index:#04x}");
            assert_eq!(key.name(), Some(name));
        }
        assert_eq!(
            MediaKey::new(0x26)
                .unwrap()
                .evidence()
                .note()
                .map(|n| n.question),
            Some("Q37")
        );
    }

    #[test]
    fn mouse_actions_round_trip() {
        for button in MouseButton::ALL {
            let action = MouseAction::Button(button);
            assert_eq!(MouseAction::from_bytes(action.bytes()), Some(action));
        }
        // c17: Fn+F2 wheel down, Fn+F3 mask 10; H18: mask 08 = back (X1),
        // mask 10 = forward (X2).
        assert_eq!(MouseAction::WheelDown.bytes(), [0x12, 0x01, 0xff, 0x00]);
        assert_eq!(
            MouseAction::Button(MouseButton::Forward).bytes(),
            [0x12, 0x00, 0x10, 0x00]
        );
        assert_eq!(
            MouseAction::Button(MouseButton::Back).bytes(),
            [0x12, 0x00, 0x08, 0x00]
        );
        assert_eq!(
            MouseAction::from_bytes([0x12, 0x01, 0x01, 0x00]),
            Some(MouseAction::WheelUp)
        );
        // Value-first (unswapped) and other masks are not mouse entries we write.
        assert_eq!(MouseAction::from_bytes([0x12, 0x01, 0x00, 0x00]), None);
        assert_eq!(MouseAction::from_bytes([0x12, 0x00, 0x03, 0x00]), None);
        // H18 tested every button and wheel up; c17 wheel down.
        assert!(MouseAction::WheelDown.evidence().is_confirmed());
        assert!(MouseAction::WheelUp.evidence().is_confirmed());
        assert!(
            MouseButton::ALL
                .iter()
                .all(|b| MouseAction::Button(*b).evidence().is_confirmed())
        );
    }

    #[test]
    fn combos_follow_the_site_rule() {
        let (ctrl, shift, a) = (usage(0xE0), usage(0xE1), usage(0x04));
        let two = Combo::new(&[ctrl, shift], a).unwrap();
        assert_eq!(two.bytes(), [0x13, 0xe0, 0xe1, 0x04]); // c17 Caps
        assert_eq!(two.to_string(), "L-Ctrl+L-Shift+A");
        assert!(two.evidence().is_confirmed());
        let one = Combo::new(&[ctrl], a).unwrap();
        assert_eq!(one.bytes(), [0x13, 0xe0, 0x04, 0x00]);
        // H18: L-Ctrl+C (`13 e0 06`) copies.
        assert!(one.evidence().is_confirmed());
        // A right-side modifier (R-Ctrl E4 .. R-GUI E7) was never tested.
        for right in [0xE4, 0xE7] {
            let alone = Combo::new(&[usage(right)], usage(0x06)).unwrap();
            let paired = Combo::new(&[ctrl, usage(right)], a).unwrap();
            for combo in [alone, paired] {
                let evidence = combo.evidence();
                assert!(!evidence.is_confirmed(), "{combo}");
                assert_eq!(evidence.note().map(|n| n.question), Some("Q37"), "{combo}");
            }
        }
        assert!(
            Combo::new(&[usage(0xE3)], a)
                .unwrap()
                .evidence()
                .is_confirmed()
        );
        for (mods, key) in [
            (&[][..], a),
            (&[ctrl, ctrl][..], a),
            (&[ctrl, shift, usage(0xE2)][..], a),
            (&[a][..], usage(0x05)),
            (&[ctrl][..], shift),
        ] {
            assert!(
                matches!(
                    Combo::new(mods, key),
                    Err(WriteBuildError::InvalidCombo { .. })
                ),
                "{mods:?} {key:?}"
            );
        }
        assert_eq!(Combo::from_bytes(two.bytes()), Some(two));
        assert_eq!(Combo::from_bytes(one.bytes()), Some(one));
        // Predefined site combinations keep b3 = 00 (spec 4.8.3).
        assert!(Combo::from_bytes([0x13, 0xE3, 0x07, 0x00]).is_some());
        for bad in [
            [0x13, 0x04, 0xE0, 0x00],
            [0x13, 0xE0, 0xE0, 0x04],
            [0x13, 0xE0, 0xE1, 0xE2],
            [0x13, 0xE0, 0x00, 0x00],
            [0x13, 0x00, 0x00, 0x00],
        ] {
            assert_eq!(Combo::from_bytes(bad), None, "{bad:02x?}");
        }
    }

    #[test]
    fn entries_and_classification() {
        let caps = key("Caps Lock");
        let slot0 = MacroSlotId::new(0).unwrap();
        let cases: [(KeyAssignment, [u8; 4]); 7] = [
            (KeyAssignment::Default, [0, 0, 0, 0]),
            (KeyAssignment::Key(usage(0x04)), [0x00, 0x04, 0, 0]),
            (
                KeyAssignment::Media(MediaKey::new(0x32).unwrap()),
                [0x07, 0x32, 0, 0],
            ),
            (
                KeyAssignment::Mouse(MouseAction::WheelDown),
                [0x12, 0x01, 0xff, 0],
            ),
            (
                KeyAssignment::Combo(Combo::new(&[usage(0xE0)], usage(0x06)).unwrap()),
                [0x13, 0xe0, 0x06, 0],
            ),
            (KeyAssignment::Macro(slot0), [0x10, 0x39, 0x00, 0x00]), // c13 Caps
            (
                KeyAssignment::Advanced(AdvancedSlotId::new(2).unwrap()),
                [0x17, 0x02, 0x02, 0x00],
            ),
        ];
        for (assignment, bytes) in cases {
            assert_eq!(assignment.entry(&W75, caps), Ok(bytes), "{assignment:?}");
            assert_eq!(
                KeyAssignment::classify(&W75, caps, bytes),
                EntryKind::Assignable(assignment)
            );
        }
        // Kept as read: Fn, lighting, function, gamepad, unknown, a macro
        // entry with another key's usage, a value the site does not offer.
        for bytes in [
            [0x01, 0x00, 0x00, 0x00],
            [0x02, 0x02, 0x00, 0x00],
            [0x11, 0x04, 0x00, 0x00],
            [0x18, 0x03, 0x00, 0x00],
            [0x10, 0x04, 0x00, 0x00],
            [0x00, 0x32, 0x00, 0x00],
            [0x07, 0x17, 0x00, 0x00],
            [0x17, 0x01, 0x02, 0x00],
            [0x17, 0x28, 0x28, 0x00],
            [0x10, 0x39, 0x23, 0x00],
            [0x55, 1, 2, 3],
        ] {
            assert_eq!(
                KeyAssignment::classify(&W75, caps, bytes),
                EntryKind::Preserved(KeyAction::decode(bytes)),
                "{bytes:02x?}"
            );
        }
        // A macro entry needs the key's own usage.
        let knob = KeyPos::from_index(15).unwrap();
        assert_eq!(
            KeyAssignment::Macro(slot0).entry(&W75, knob),
            Err(WriteBuildError::Placement(PlacementError::NoUsage {
                index: 15
            }))
        );
        let unknown = Model {
            product_code: "AP61HEARGB",
            ..W75
        };
        assert!(matches!(
            KeyAssignment::Macro(slot0).entry(&unknown, caps),
            Err(WriteBuildError::UnknownModel { .. })
        ));
    }

    #[test]
    fn evidence_per_layer() {
        let slot = KeyAssignment::Advanced(AdvancedSlotId::new(0).unwrap());
        assert!(slot.evidence(Layer::Base).is_confirmed());
        assert_eq!(
            slot.evidence(Layer::Fn).note().map(|n| n.question),
            Some("Q36")
        );
        let macro_ = KeyAssignment::Macro(MacroSlotId::new(0).unwrap());
        assert!(macro_.evidence(Layer::Base).is_confirmed());
        // H19: a macro bound on the Fn layer plays.
        assert!(macro_.evidence(Layer::Fn).is_confirmed());
        assert!(
            KeyAssignment::Key(usage(4))
                .evidence(Layer::Fn)
                .is_confirmed()
        );
    }

    #[test]
    fn placement_rules() {
        let none = KeyMask::default();
        let fn_locked = KeyMask::from_positions([KeyPos::from_index(121).unwrap()]);
        let a = KeyAssignment::Key(usage(0x04));
        let check = |layer, pos, target: &KeyAssignment, acks: &[BuiltinAck]| {
            check_placement(&W75, layer, pos, target, &fn_locked, acks)
        };
        // Plain keys on both layers.
        assert_eq!(check(Layer::Base, key("W"), &a, &[]), Ok(None));
        assert_eq!(check(Layer::Fn, key("W"), &a, &[]), Ok(None));
        // Not a key, knob, Fn key: refused on both layers.
        let gap = KeyPos::from_index(16).unwrap();
        for layer in Layer::ALL {
            assert_eq!(
                check(layer, gap, &a, &[]),
                Err(PlacementError::NotAKey { index: 16 })
            );
            assert_eq!(
                check(layer, KeyPos::from_index(15).unwrap(), &a, &[]),
                Err(PlacementError::Knob)
            );
            assert_eq!(
                check(layer, key("Fn"), &KeyAssignment::Default, &[]),
                Err(PlacementError::FnKey)
            );
        }
        // A key locked by the keyboard on the Fn layer only.
        let locked = KeyMask::from_positions([key("Q")]);
        assert_eq!(
            check_placement(&W75, Layer::Fn, key("Q"), &a, &locked, &[]),
            Err(PlacementError::FnLocked { index: 45 })
        );
        assert_eq!(
            check_placement(&W75, Layer::Base, key("Q"), &a, &locked, &[]),
            Ok(None)
        );
        // Built-ins: base layer free, Fn layer needs an acknowledgement.
        let f1 = key("F1");
        assert_eq!(check(Layer::Base, f1, &a, &[]), Ok(None));
        assert_eq!(
            check(Layer::Fn, f1, &a, &[]),
            Err(PlacementError::BuiltinNotAcknowledged {
                index: 2,
                key: "F1",
                function: "media player",
                critical: false
            })
        );
        // The refusal names the key, not only its index.
        assert_eq!(
            check(Layer::Fn, f1, &a, &[]).unwrap_err().to_string(),
            "Fn + F1 (#2) is the built-in function \"media player\"; replacing it needs an \
             explicit acknowledgement"
        );
        let ack = BuiltinAck {
            pos: f1,
            critical: false,
        };
        assert_eq!(
            check(Layer::Fn, f1, &a, &[ack]).map(|b| b.map(|b| b.index)),
            Ok(Some(2))
        );
        // An acknowledgement for another key does not count.
        let other = BuiltinAck {
            pos: key("F2"),
            critical: true,
        };
        assert!(check(Layer::Fn, f1, &a, &[other]).is_err());
        // Default brings the built-in back without an acknowledgement.
        assert_eq!(check(Layer::Fn, f1, &KeyAssignment::Default, &[]), Ok(None));
        // Critical: Fn + 2 and Fn + Space need the critical flag.
        for name in ["2", "Space"] {
            let pos = key(name);
            let plain = BuiltinAck {
                pos,
                critical: false,
            };
            assert!(
                matches!(
                    check(Layer::Fn, pos, &a, &[plain]),
                    Err(PlacementError::BuiltinNotAcknowledged { critical: true, .. })
                ),
                "{name}"
            );
            let critical = BuiltinAck {
                pos,
                critical: true,
            };
            assert!(check(Layer::Fn, pos, &a, &[critical]).is_ok(), "{name}");
        }
        // The Fn-locked mask of an empty reply locks nothing.
        assert_eq!(
            check_placement(&W75, Layer::Fn, key("W"), &a, &none, &[]),
            Ok(None)
        );
        let unknown = Model {
            product_code: "AP61HEARGB",
            ..W75
        };
        assert!(matches!(
            check_placement(&unknown, Layer::Base, key("W"), &a, &none, &[]),
            Err(PlacementError::UnknownModel { .. })
        ));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serializes() {
        assert_eq!(serde_json::to_string(&usage(0x04)).unwrap(), "4");
        assert!(serde_json::from_str::<HidUsage>("50").is_err());
        assert_eq!(
            serde_json::to_string(&KeyAssignment::Mouse(MouseAction::Button(
                MouseButton::Forward
            )))
            .unwrap(),
            r#"{"mouse":{"button":"forward"}}"#
        );
        assert_eq!(
            serde_json::to_string(&PlacementError::Knob).unwrap(),
            r#"{"kind":"knob"}"#
        );
    }
}
