//! Keymap tables `18 80` / `18 82` (spec section 4.8).
//!
//! Framing is CONFIRMED (10 chunks, 528 bytes = 132 x 4). Every captured
//! table is all zero, so the entry encoding below is CODE-ONLY except for
//! `00 00 00 00` ("no override"). Each entry keeps its raw 4 bytes.

use std::fmt;

use super::{FromResponse, need, quad, wrong_request};
use crate::error::DecodeError;
use crate::hex::HexBytes;
use crate::layout::MATRIX_SIZE;
use crate::request::{Layer, ReadRequest};
use crate::response::RawResponse;
use crate::usage::{hid_usage_name, media_name};

/// Meaning of a 4-byte keymap entry `b0 b1 b2 b3` (CODE-ONLY).
///
/// Entries that do not fit a documented pattern decode as
/// [`KeyAction::Unknown`]; nothing is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum KeyAction {
    /// `00 00 00 00`: no override, the firmware default applies (CONFIRMED).
    Default,
    /// `00 uu 00 00`: keyboard key with HID usage `uu`.
    Key {
        /// HID usage (page 0x07).
        usage: u8,
    },
    /// `01 xx 00 00`: Fn.
    Fn {
        /// Layout code sent with it (`0xFA` in other layouts).
        code: u8,
    },
    /// `02 ss 00 00`: lighting (`02` = cycle effects).
    Lighting {
        /// Sub-function.
        sub: u8,
    },
    /// `03 ss 00 00`: brightness (`00` up, `01` down).
    Brightness {
        /// Sub-function.
        sub: u8,
    },
    /// `04 ss 00 00`: effect speed (`00` up, `01` down).
    EffectSpeed {
        /// Sub-function.
        sub: u8,
    },
    /// `05 ss 00 00`: effect direction (`02` = cycle).
    Direction {
        /// Sub-function.
        sub: u8,
    },
    /// `06 ss 00 00`: colour (`00` full colour, `01` foreground, `02` background cycle).
    Color {
        /// Sub-function.
        sub: u8,
    },
    /// `07 ii 00 00`: multimedia key with vendor index `ii`.
    Media {
        /// Vendor multimedia index.
        index: u8,
    },
    /// `10 uu ss 00`: macro in slot `ss & 0x1F`.
    Macro {
        /// Macro slot (0..=9).
        slot: u8,
        /// The physical key's own usage (ignored on read by the vendor site).
        key_usage: u8,
    },
    /// `11 ss 00 00`: function (`00` lock keyboard, `02` lock Win, `04` lights off).
    Function {
        /// Sub-function.
        sub: u8,
    },
    /// `12 kk vv 00`: mouse; kind `00` button (mask), `01` wheel (`01` up, `FF` down).
    Mouse {
        /// 0 = button, 1 = wheel.
        kind: u8,
        /// Button mask or wheel direction.
        value: u8,
    },
    /// `13 u1 u2 u3`: key combination (modifiers + key).
    Combo {
        /// Up to three HID usages (0 = unused).
        usages: [u8; 3],
    },
    /// `17 ss ss 00`: advanced key in slot `ss`.
    Advanced {
        /// Advanced-key slot (0..40).
        slot: u8,
    },
    /// `18 cc 00 00`: gamepad "Classical" control.
    GamepadClassical {
        /// Control number.
        control: u8,
    },
    /// `19 cc 00 00`: gamepad Xbox 360 control.
    GamepadXbox {
        /// Control number.
        control: u8,
    },
    /// Anything else.
    Unknown {
        /// The raw entry.
        bytes: [u8; 4],
    },
}

impl KeyAction {
    /// Decodes a 4-byte entry.
    pub fn decode(bytes: [u8; 4]) -> Self {
        let [b0, b1, b2, b3] = bytes;
        let tail_zero = b2 == 0 && b3 == 0;
        match b0 {
            0x00 if b1 == 0 && tail_zero => KeyAction::Default,
            0x00 if tail_zero => KeyAction::Key { usage: b1 },
            0x01 if tail_zero => KeyAction::Fn { code: b1 },
            0x02 if tail_zero => KeyAction::Lighting { sub: b1 },
            0x03 if tail_zero => KeyAction::Brightness { sub: b1 },
            0x04 if tail_zero => KeyAction::EffectSpeed { sub: b1 },
            0x05 if tail_zero => KeyAction::Direction { sub: b1 },
            0x06 if tail_zero => KeyAction::Color { sub: b1 },
            0x07 if tail_zero => KeyAction::Media { index: b1 },
            0x10 if b3 == 0 => KeyAction::Macro {
                slot: b2 & 0x1F,
                key_usage: b1,
            },
            0x11 if tail_zero => KeyAction::Function { sub: b1 },
            0x12 if b3 == 0 => KeyAction::Mouse {
                kind: b1,
                value: b2,
            },
            0x13 => KeyAction::Combo {
                usages: [b1, b2, b3],
            },
            0x17 if b1 == b2 && b3 == 0 => KeyAction::Advanced { slot: b1 },
            0x18 if tail_zero => KeyAction::GamepadClassical { control: b1 },
            0x19 if tail_zero => KeyAction::GamepadXbox { control: b1 },
            _ => KeyAction::Unknown { bytes },
        }
    }

    /// Decodes an action stored in the vendor's *unswapped* order.
    ///
    /// Keymap entries and DKS actions write a mouse action kind first
    /// (`12 <kind> <value> 00`, see [`decode`](Self::decode)). The MT, TGL
    /// and RKRT advanced-key types write it the other way round,
    /// `12 <value> <kind> 00` (spec section 4.14, CODE-ONLY). This swaps
    /// bytes 1 and 2 back for mouse actions; every other action decodes
    /// exactly like [`decode`](Self::decode). An entry that still fits no
    /// pattern is returned as [`KeyAction::Unknown`] with its bytes as stored.
    pub fn decode_unswapped(bytes: [u8; 4]) -> Self {
        let [b0, b1, b2, b3] = bytes;
        if b0 != 0x12 {
            return Self::decode(bytes);
        }
        match Self::decode([b0, b2, b1, b3]) {
            KeyAction::Unknown { .. } => KeyAction::Unknown { bytes },
            action => action,
        }
    }

    /// Whether this is the "no override" entry.
    pub fn is_default(&self) -> bool {
        *self == KeyAction::Default
    }
}

fn usage_label(usage: u8) -> String {
    match hid_usage_name(usage) {
        Some(name) => name.to_string(),
        None => format!("HID {usage:#04x}"),
    }
}

impl fmt::Display for KeyAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            KeyAction::Default => f.write_str("default"),
            KeyAction::Key { usage } => f.write_str(&usage_label(usage)),
            KeyAction::Fn { .. } => f.write_str("Fn"),
            KeyAction::Lighting { sub: 0x02 } => f.write_str("Lighting: cycle effects"),
            KeyAction::Lighting { sub } => write!(f, "Lighting {sub:#04x}"),
            KeyAction::Brightness { sub: 0x00 } => f.write_str("Brightness +"),
            KeyAction::Brightness { sub: 0x01 } => f.write_str("Brightness -"),
            KeyAction::Brightness { sub } => write!(f, "Brightness {sub:#04x}"),
            KeyAction::EffectSpeed { sub: 0x00 } => f.write_str("Effect speed +"),
            KeyAction::EffectSpeed { sub: 0x01 } => f.write_str("Effect speed -"),
            KeyAction::EffectSpeed { sub } => write!(f, "Effect speed {sub:#04x}"),
            KeyAction::Direction { sub: 0x02 } => f.write_str("Direction: cycle"),
            KeyAction::Direction { sub } => write!(f, "Direction {sub:#04x}"),
            KeyAction::Color { sub: 0x00 } => f.write_str("Colour: full colour"),
            KeyAction::Color { sub: 0x01 } => f.write_str("Colour: foreground cycle"),
            KeyAction::Color { sub: 0x02 } => f.write_str("Colour: background cycle"),
            KeyAction::Color { sub } => write!(f, "Colour {sub:#04x}"),
            KeyAction::Media { index } => match media_name(index) {
                Some(name) => f.write_str(name),
                None => write!(f, "Media {index:#04x}"),
            },
            KeyAction::Macro { slot, .. } => write!(f, "Macro slot {slot}"),
            KeyAction::Function { sub: 0x00 } => f.write_str("Lock keyboard"),
            KeyAction::Function { sub: 0x02 } => f.write_str("Lock Win key"),
            KeyAction::Function { sub: 0x04 } => f.write_str("All lights off"),
            KeyAction::Function { sub } => write!(f, "Function {sub:#04x}"),
            KeyAction::Mouse { kind: 0x00, value } => {
                let name = match value {
                    0x01 => "left",
                    0x02 => "right",
                    0x04 => "middle",
                    // X1 and X2 (hardware test H18; the vendor site labels
                    // them the other way round, spec 4.8.3).
                    0x08 => "back",
                    0x10 => "forward",
                    _ => return write!(f, "Mouse buttons {value:#04x}"),
                };
                write!(f, "Mouse {name} button")
            }
            KeyAction::Mouse { kind: 0x01, value } => match value {
                0x01 => f.write_str("Mouse wheel up"),
                0xFF => f.write_str("Mouse wheel down"),
                _ => write!(f, "Mouse wheel {value:#04x}"),
            },
            KeyAction::Mouse { kind, value } => write!(f, "Mouse {kind:#04x} {value:#04x}"),
            KeyAction::Combo { usages } => {
                let parts: Vec<String> = usages
                    .iter()
                    .filter(|u| **u != 0)
                    .map(|u| usage_label(*u))
                    .collect();
                write!(f, "Combo {}", parts.join("+"))
            }
            KeyAction::Advanced { slot } => write!(f, "Advanced key slot {slot}"),
            KeyAction::GamepadClassical { control } => {
                write!(f, "Gamepad (Classical) control {control}")
            }
            KeyAction::GamepadXbox { control } => write!(f, "Gamepad (Xbox) control {control}"),
            KeyAction::Unknown { bytes } => {
                write!(f, "unknown {}", crate::hex::to_spaced_hex(&bytes))
            }
        }
    }
}

/// One keymap entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct KeymapEntry {
    /// Key index (row x 22 + col).
    pub index: u8,
    /// Decoded action.
    pub action: KeyAction,
    /// The raw 4 bytes.
    pub raw: HexBytes,
}

/// A keymap layer: 132 entries of 4 bytes (CONFIRMED framing).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct KeymapTable {
    /// Which layer.
    pub layer: Layer,
    /// 132 entries in index order.
    pub entries: Vec<KeymapEntry>,
    /// The 528 table bytes.
    pub raw: HexBytes,
}

impl KeymapTable {
    /// Entry for key `index`.
    pub fn entry(&self, index: u8) -> Option<&KeymapEntry> {
        self.entries.get(usize::from(index))
    }

    /// Entries that override the firmware default.
    pub fn overrides(&self) -> impl Iterator<Item = &KeymapEntry> {
        self.entries.iter().filter(|e| !e.action.is_default())
    }
}

impl FromResponse for KeymapTable {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        let ReadRequest::Keymap(layer) = response.request() else {
            return Err(wrong_request(response, "KeymapTable"));
        };
        let bytes = response.table_bytes("keymap")?;
        need(&bytes, "keymap", MATRIX_SIZE * 4)?;
        let entries = (0u8..)
            .zip(bytes.as_chunks::<4>().0.iter().take(MATRIX_SIZE))
            .map(|(index, entry)| {
                let raw = quad(entry, 0);
                KeymapEntry {
                    index,
                    action: KeyAction::decode(raw),
                    raw: HexBytes(raw.to_vec()),
                }
            })
            .collect();
        Ok(KeymapTable {
            layer,
            entries,
            raw: HexBytes(bytes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::table;

    #[test]
    fn decode_entries() {
        assert_eq!(KeyAction::decode([0, 0, 0, 0]), KeyAction::Default);
        assert_eq!(
            KeyAction::decode([0, 0x04, 0, 0]),
            KeyAction::Key { usage: 0x04 }
        );
        assert_eq!(
            KeyAction::decode([0x01, 0xFA, 0, 0]),
            KeyAction::Fn { code: 0xFA }
        );
        assert_eq!(
            KeyAction::decode([0x07, 0x12, 0, 0]),
            KeyAction::Media { index: 0x12 }
        );
        assert_eq!(
            KeyAction::decode([0x10, 0x39, 0x23, 0]),
            KeyAction::Macro {
                slot: 3,
                key_usage: 0x39
            }
        );
        assert_eq!(
            KeyAction::decode([0x12, 0x01, 0xFF, 0]),
            KeyAction::Mouse {
                kind: 1,
                value: 0xFF
            }
        );
        assert_eq!(
            KeyAction::decode([0x13, 0xE0, 0x06, 0]),
            KeyAction::Combo {
                usages: [0xE0, 0x06, 0]
            }
        );
        assert_eq!(
            KeyAction::decode([0x17, 5, 5, 0]),
            KeyAction::Advanced { slot: 5 }
        );
        assert_eq!(
            KeyAction::decode([0x17, 5, 6, 0]),
            KeyAction::Unknown {
                bytes: [0x17, 5, 6, 0]
            }
        );
        assert_eq!(
            KeyAction::decode([0x00, 0x04, 0x01, 0]),
            KeyAction::Unknown {
                bytes: [0, 4, 1, 0]
            }
        );
        assert_eq!(
            KeyAction::decode([0x55, 1, 2, 3]),
            KeyAction::Unknown {
                bytes: [0x55, 1, 2, 3]
            }
        );
    }

    #[test]
    fn decode_unswapped_only_changes_mouse_actions() {
        // MT/TGL/RKRT store "left button" as `12 01 00 00` (spec 4.14).
        assert_eq!(
            KeyAction::decode_unswapped([0x12, 0x01, 0x00, 0]),
            KeyAction::Mouse { kind: 0, value: 1 }
        );
        assert_eq!(
            KeyAction::decode_unswapped([0x12, 0xFF, 0x01, 0]),
            KeyAction::Mouse {
                kind: 1,
                value: 0xFF
            }
        );
        assert_eq!(
            KeyAction::decode_unswapped([0x00, 0x04, 0, 0]),
            KeyAction::Key { usage: 0x04 }
        );
        // Unknown entries keep the bytes as stored, not the swapped ones.
        assert_eq!(
            KeyAction::decode_unswapped([0x12, 0x01, 0x00, 0x05]),
            KeyAction::Unknown {
                bytes: [0x12, 0x01, 0x00, 0x05]
            }
        );
    }

    #[test]
    fn display() {
        assert_eq!(KeyAction::decode([0, 0x04, 0, 0]).to_string(), "A");
        assert_eq!(KeyAction::decode([0x07, 0x12, 0, 0]).to_string(), "Mute");
        assert_eq!(
            KeyAction::decode([0x13, 0xE0, 0x06, 0]).to_string(),
            "Combo L-Ctrl+C"
        );
        assert_eq!(
            KeyAction::decode([0x12, 0x00, 0x04, 0]).to_string(),
            "Mouse middle button"
        );
        assert_eq!(
            KeyAction::decode([0x12, 0x00, 0x08, 0]).to_string(),
            "Mouse back button"
        );
        assert_eq!(
            KeyAction::decode([0x12, 0x00, 0x10, 0]).to_string(),
            "Mouse forward button"
        );
        assert_eq!(
            KeyAction::decode([0x03, 0x01, 0, 0]).to_string(),
            "Brightness -"
        );
        assert_eq!(
            KeyAction::decode([0x55, 1, 2, 3]).to_string(),
            "unknown 55 01 02 03"
        );
    }

    #[test]
    fn table_decoding() {
        let mut bytes = vec![0u8; 528];
        // W (index 46) -> A, at offset 184 (chunk 3, payload offset 16).
        bytes[184..188].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
        let t =
            KeymapTable::from_response(&table(ReadRequest::Keymap(Layer::Base), &bytes)).unwrap();
        assert_eq!(t.layer, Layer::Base);
        assert_eq!(t.entries.len(), 132);
        let overrides: Vec<_> = t.overrides().collect();
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].index, 46);
        assert_eq!(overrides[0].action, KeyAction::Key { usage: 0x04 });
        assert_eq!(t.entry(46).map(|e| e.raw.0.clone()), Some(vec![0, 4, 0, 0]));
    }

    #[test]
    fn wrong_request() {
        let raw = table(ReadRequest::DeadBand, &[0u8; 264]);
        assert!(matches!(
            KeymapTable::from_response(&raw),
            Err(DecodeError::WrongRequest { .. })
        ));
    }
}
