//! Advanced keys `22 80` (spec section 4.14).
//!
//! Framing is CONFIRMED: 40 chunks of 40 bytes, chunk index = slot, payload
//! byte 0 (report byte 5) = type. All captured slots are zero, so the per-type
//! layouts below are CODE-ONLY (taken from the write format, which carries
//! 50 bytes; the read carries 40). Every slot keeps its raw bytes.
//!
//! Mouse actions are stored kind first (`12 <kind> <value> 00`) in DKS
//! actions, but value first in MT, TGL and RKRT slots (spec section 4.14);
//! the latter are decoded with [`KeyAction::decode_unswapped`].

use super::{FromResponse, at, expect_request, quad};
use crate::decode::keymap::KeyAction;
use crate::error::DecodeError;
use crate::hex::HexBytes;
use crate::request::ReadRequest;
use crate::response::RawResponse;

/// Number of advanced-key slots.
const SLOTS: usize = 40;
/// Bytes per slot in the read reply.
const SLOT_LEN: usize = 40;

/// Decoded content of an advanced-key slot (CODE-ONLY).
///
/// Offsets are given as report bytes of the vendor's write format; payload
/// byte `i` = report byte `5 + i`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AdvancedKey {
    /// Type 0: empty slot.
    Empty,
    /// Type 1: DKS (dynamic keystroke), offered on the W75.
    Dks {
        /// Bytes 6-9: press points route1..route4 in **0.1 mm** units.
        press_points: [u8; 4],
        /// Bytes 10+8k..13+8k: action for key slot k = 0..3.
        actions: [KeyAction; 4],
        /// Bytes 14+8k..17+8k: step byte of key slot k at stages 0..3
        /// (0 nothing, 1 tap, 2 hold, 3 re-press; firmware meaning GUESS).
        steps: [[u8; 4]; 4],
    },
    /// Type 2: MT (tap / hold), offered on the W75.
    ModTap {
        /// Bytes 6-9: hold action (mouse actions stored unswapped).
        hold: KeyAction,
        /// Bytes 10-13: tap action (mouse actions stored unswapped).
        tap: KeyAction,
        /// Byte 14: hold threshold in 10 ms units.
        hold_threshold_10ms: u8,
    },
    /// Type 3: TGL (toggle), offered on the W75.
    Toggle {
        /// Bytes 6-9: action (mouse actions stored unswapped).
        action: KeyAction,
    },
    /// Type 4: RS (deeper of two keys wins); not offered on the W75.
    Rs {
        /// Bytes 6-9: key A code.
        key_a: KeyAction,
        /// Bytes 10-11: key A row, col.
        pos_a: [u8; 2],
        /// Bytes 12-15: key B code.
        key_b: KeyAction,
        /// Bytes 16-17: key B row, col.
        pos_b: [u8; 2],
    },
    /// Type 5: RKRT (send another key on release); not offered on the W75.
    Rkrt {
        /// Bytes 6-9: the bound key (mouse actions stored unswapped).
        bound: KeyAction,
        /// Bytes 12-15: the key sent on release (mouse actions stored unswapped).
        on_release: KeyAction,
    },
    /// Type 6: MDS (delay, then up to 6 keys); not offered on the W75.
    Mds {
        /// Bytes 6-7: delay in ms (BE16).
        delay_ms: u16,
        /// Byte 8: 0 = after the delay, 1 = after all keys.
        mode: u8,
        /// Bytes 10..: HID usages (byte 9 = count, at most 6).
        keys: Vec<u8>,
    },
    /// Any other type byte.
    Unknown {
        /// The type byte.
        kind: u8,
    },
}

impl AdvancedKey {
    /// Decodes one 40-byte slot payload.
    pub fn parse(p: &[u8]) -> AdvancedKey {
        // DKS (and RS) actions use the keymap order; MT, TGL and RKRT store
        // mouse actions unswapped (spec section 4.14, CODE-ONLY).
        let action = |i: usize| KeyAction::decode(quad(p, i));
        let unswapped = |i: usize| KeyAction::decode_unswapped(quad(p, i));
        match at(p, 0) {
            0 => AdvancedKey::Empty,
            1 => AdvancedKey::Dks {
                press_points: quad(p, 1),
                actions: [action(5), action(13), action(21), action(29)],
                steps: [quad(p, 9), quad(p, 17), quad(p, 25), quad(p, 33)],
            },
            2 => AdvancedKey::ModTap {
                hold: unswapped(1),
                tap: unswapped(5),
                hold_threshold_10ms: at(p, 9),
            },
            3 => AdvancedKey::Toggle {
                action: unswapped(1),
            },
            4 => AdvancedKey::Rs {
                key_a: action(1),
                pos_a: [at(p, 5), at(p, 6)],
                key_b: action(7),
                pos_b: [at(p, 11), at(p, 12)],
            },
            5 => AdvancedKey::Rkrt {
                bound: unswapped(1),
                on_release: unswapped(7),
            },
            6 => {
                let count = usize::from(at(p, 4)).min(6);
                AdvancedKey::Mds {
                    delay_ms: u16::from_be_bytes([at(p, 1), at(p, 2)]),
                    mode: at(p, 3),
                    keys: (0..count).map(|i| at(p, 5 + i)).collect(),
                }
            }
            kind => AdvancedKey::Unknown { kind },
        }
    }
}

/// One advanced-key slot.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdvancedKeySlot {
    /// Slot number (0..40) = chunk index.
    pub slot: u8,
    /// Type byte (report byte 5).
    pub kind: u8,
    /// Decoded content (CODE-ONLY).
    pub key: AdvancedKey,
    /// The slot's declared payload (40 bytes).
    pub raw: HexBytes,
}

impl AdvancedKeySlot {
    /// Whether the slot is empty (type 0).
    pub fn is_empty(&self) -> bool {
        self.kind == 0
    }
}

/// All 40 advanced-key slots.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdvancedKeys {
    /// Slots 0..40 in order.
    pub slots: Vec<AdvancedKeySlot>,
}

impl AdvancedKeys {
    /// Slots that are not empty.
    pub fn used(&self) -> impl Iterator<Item = &AdvancedKeySlot> {
        self.slots.iter().filter(|s| !s.is_empty())
    }
}

impl FromResponse for AdvancedKeys {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "AdvancedKeys", |r| r == ReadRequest::AdvancedKeys)?;
        // Validates the declared length of every chunk.
        response.table_bytes("advanced keys")?;
        let slots = response
            .reports()
            .iter()
            .take(SLOTS)
            .map(|report| {
                let p = report.payload();
                AdvancedKeySlot {
                    slot: u8::try_from(report.chunk_index()).unwrap_or(u8::MAX),
                    kind: at(p, 0),
                    key: AdvancedKey::parse(p),
                    raw: HexBytes(p.iter().copied().take(SLOT_LEN).collect()),
                }
            })
            .collect();
        Ok(AdvancedKeys { slots })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::table;

    #[test]
    fn factory_slots_are_empty() {
        let keys =
            AdvancedKeys::from_response(&table(ReadRequest::AdvancedKeys, &[0u8; 1600])).unwrap();
        assert_eq!(keys.slots.len(), 40);
        assert_eq!(keys.used().count(), 0);
        assert_eq!(keys.slots[39].slot, 39);
        assert_eq!(keys.slots[0].key, AdvancedKey::Empty);
    }

    #[test]
    fn decodes_types() {
        let mut bytes = vec![0u8; 1600];
        // Slot 1: MT, hold L-Ctrl, tap A, 300 ms.
        bytes[40..50].copy_from_slice(&[2, 0, 0xE0, 0, 0, 0, 0x04, 0, 0, 30]);
        // Slot 2: TGL on B.
        bytes[80..85].copy_from_slice(&[3, 0, 0x05, 0, 0]);
        // Slot 3: DKS, press points 1.5 / 3.0 mm, first action A.
        bytes[120..130].copy_from_slice(&[1, 15, 30, 30, 15, 0, 0x04, 0, 0, 1]);
        // Slot 4: unknown type.
        bytes[160] = 9;
        let keys = AdvancedKeys::from_response(&table(ReadRequest::AdvancedKeys, &bytes)).unwrap();
        assert_eq!(keys.used().count(), 4);
        assert_eq!(
            keys.slots[1].key,
            AdvancedKey::ModTap {
                hold: KeyAction::Key { usage: 0xE0 },
                tap: KeyAction::Key { usage: 0x04 },
                hold_threshold_10ms: 30,
            }
        );
        assert_eq!(
            keys.slots[2].key,
            AdvancedKey::Toggle {
                action: KeyAction::Key { usage: 0x05 }
            }
        );
        match &keys.slots[3].key {
            AdvancedKey::Dks {
                press_points,
                actions,
                steps,
            } => {
                assert_eq!(*press_points, [15, 30, 30, 15]);
                assert_eq!(actions[0], KeyAction::Key { usage: 0x04 });
                assert_eq!(steps[0], [1, 0, 0, 0]);
            }
            other => panic!("expected DKS, got {other:?}"),
        }
        assert_eq!(keys.slots[4].key, AdvancedKey::Unknown { kind: 9 });
        assert_eq!(keys.slots[4].raw.len(), 40);
    }

    #[test]
    fn mouse_actions_in_mt_and_tgl_are_unswapped() {
        // Spec 4.14: MT/TGL store a mouse action as `12 <value> <kind> 00`.
        // MT: hold = left mouse button, tap = A, 150 ms.
        let mt = [0x02, 0x12, 0x01, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x0f];
        assert_eq!(
            AdvancedKey::parse(&mt),
            AdvancedKey::ModTap {
                hold: KeyAction::Mouse { kind: 0, value: 1 },
                tap: KeyAction::Key { usage: 0x04 },
                hold_threshold_10ms: 0x0f,
            }
        );
        // TGL on the right mouse button.
        assert_eq!(
            AdvancedKey::parse(&[0x03, 0x12, 0x02, 0x00, 0x00]),
            AdvancedKey::Toggle {
                action: KeyAction::Mouse { kind: 0, value: 2 }
            }
        );
        // RKRT: same rule.
        let rkrt = [0x05, 0x12, 0xFF, 0x01, 0x00, 0, 0, 0x12, 0x01, 0x01, 0x00];
        assert_eq!(
            AdvancedKey::parse(&rkrt),
            AdvancedKey::Rkrt {
                bound: KeyAction::Mouse {
                    kind: 1,
                    value: 0xFF
                },
                on_release: KeyAction::Mouse { kind: 1, value: 1 },
            }
        );
        // DKS actions keep the keymap order `12 <kind> <value> 00`.
        let mut dks = [0u8; 40];
        dks[0] = 1;
        dks[5..9].copy_from_slice(&[0x12, 0x00, 0x01, 0x00]);
        match AdvancedKey::parse(&dks) {
            AdvancedKey::Dks { actions, .. } => {
                assert_eq!(actions[0], KeyAction::Mouse { kind: 0, value: 1 });
            }
            other => panic!("expected DKS, got {other:?}"),
        }
    }

    #[test]
    fn mds_is_bounded() {
        let mut p = [0u8; 40];
        p[0] = 6;
        p[4] = 200;
        match AdvancedKey::parse(&p) {
            AdvancedKey::Mds { keys, .. } => assert_eq!(keys.len(), 6),
            other => panic!("expected MDS, got {other:?}"),
        }
        assert_eq!(AdvancedKey::parse(&[]), AdvancedKey::Empty);
    }
}
