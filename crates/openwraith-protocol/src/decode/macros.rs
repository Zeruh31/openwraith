//! Macro slots `19 80`..`19 89` (spec section 4.9).
//!
//! Framing is CONFIRMED (5 chunks, 256 bytes per slot). Every captured slot is
//! all `FF` (empty), so the slot layout below is CODE-ONLY. The parser is
//! tolerant: it never fails on content, and the raw 256 bytes are kept.

use super::{FromResponse, at, need, wrong_request};
use crate::error::DecodeError;
use crate::hex::HexBytes;
use crate::request::ReadRequest;
use crate::response::RawResponse;

/// Size of one macro slot in bytes.
const SLOT_LEN: usize = 256;
/// Offset of the first step.
const STEPS_OFFSET: usize = 8;
/// Largest step-data length the vendor reader accepts (62 steps).
const MAX_STEP_BYTES: u8 = 248;

/// How a macro plays (slot byte 1; CODE-ONLY).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MacroPlayMode {
    /// 0: play once.
    Once,
    /// 1: play `repeat_count` times.
    Times,
    /// 2: press to start, press again to stop.
    Toggle,
    /// 3: repeat while held.
    WhileHeld,
    /// Any other value.
    Unknown(u8),
}

impl MacroPlayMode {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => MacroPlayMode::Once,
            1 => MacroPlayMode::Times,
            2 => MacroPlayMode::Toggle,
            3 => MacroPlayMode::WhileHeld,
            other => MacroPlayMode::Unknown(other),
        }
    }
}

/// One macro step (4 bytes; CODE-ONLY).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MacroStep {
    /// +0: HID usage, or a mouse/other code.
    pub code: u8,
    /// +1: `(state << 4) | type`; state 1 press / 0 release, type 0 keyboard /
    /// 1 anything else.
    pub flags: u8,
    /// +2..+3: delay in ms to wait **after** this step (BE16).
    pub delay_ms: u16,
}

impl MacroStep {
    /// Whether this step presses (rather than releases) its code.
    pub fn is_press(&self) -> bool {
        self.flags >> 4 == 1
    }

    /// Step type: 0 keyboard, 1 anything else (mouse, media...).
    pub fn kind(&self) -> u8 {
        self.flags & 0x0F
    }
}

/// Decoded header and steps of a non-empty slot (CODE-ONLY).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MacroContent {
    /// Byte 0 bits 0-6: macro ID (the vendor site's library index).
    pub macro_id: u8,
    /// Byte 0 bit 7: copy flag (same macro already written to a lower slot).
    pub copy: bool,
    /// Byte 1: play mode.
    pub play_mode: MacroPlayMode,
    /// Byte 3: step data length in bytes (4 per step).
    pub step_bytes: u8,
    /// Bytes 4-5: repeat count (BE16).
    pub repeat_count: u16,
    /// The steps (as many complete 4-byte steps as `step_bytes` announces).
    pub steps: Vec<MacroStep>,
}

/// One macro slot.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MacroSlot {
    /// Slot number 0..=9.
    pub slot: u8,
    /// `None` when the slot is empty (header byte 3 < 4 or > 248, like the
    /// vendor reader; covers the factory all-`FF` and the all-`00` slots).
    pub content: Option<MacroContent>,
    /// The 256 slot bytes.
    pub raw: HexBytes,
}

impl MacroSlot {
    /// Whether the slot holds no macro.
    pub fn is_empty(&self) -> bool {
        self.content.is_none()
    }

    /// Decodes 256 slot bytes (flat layout: chunk c holds bytes `c*58 ..`).
    pub fn parse(slot: u8, bytes: &[u8]) -> MacroSlot {
        let step_bytes = at(bytes, 3);
        let content = (4..=MAX_STEP_BYTES).contains(&step_bytes).then(|| {
            let steps_end = (STEPS_OFFSET + usize::from(step_bytes)).min(bytes.len());
            let steps = bytes
                .get(STEPS_OFFSET..steps_end)
                .unwrap_or_default()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|s| MacroStep {
                    code: at(s, 0),
                    flags: at(s, 1),
                    delay_ms: u16::from_be_bytes([at(s, 2), at(s, 3)]),
                })
                .collect();
            MacroContent {
                macro_id: at(bytes, 0) & 0x7F,
                copy: at(bytes, 0) & 0x80 != 0,
                play_mode: MacroPlayMode::from_byte(at(bytes, 1)),
                step_bytes,
                repeat_count: u16::from_be_bytes([at(bytes, 4), at(bytes, 5)]),
                steps,
            }
        });
        MacroSlot {
            slot,
            content,
            raw: HexBytes(bytes.to_vec()),
        }
    }
}

impl FromResponse for MacroSlot {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        let ReadRequest::Macro(slot) = response.request() else {
            return Err(wrong_request(response, "MacroSlot"));
        };
        let bytes = response.table_bytes("macro slot")?;
        need(&bytes, "macro slot", SLOT_LEN)?;
        Ok(MacroSlot::parse(slot.get(), &bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::table;
    use crate::request::MacroSlotId;

    #[test]
    fn factory_slot_is_empty() {
        let raw = table(
            ReadRequest::Macro(MacroSlotId::new(4).unwrap()),
            &[0xFF; 256],
        );
        let slot = MacroSlot::from_response(&raw).unwrap();
        assert_eq!(slot.slot, 4);
        assert!(slot.is_empty());
        assert_eq!(slot.raw.len(), 256);
        assert!(MacroSlot::parse(0, &[0u8; 256]).is_empty());
    }

    #[test]
    fn spec_example_slot() {
        // Spec 4.9.3 example: macro 0, play once, "A down, 30 ms, A up".
        let mut bytes = vec![0u8; 256];
        bytes[..16].copy_from_slice(&[
            0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00, 0x00, 0x04, 0x10, 0x00, 0x1e, 0x04, 0x00,
            0x00, 0x00,
        ]);
        let raw = table(ReadRequest::Macro(MacroSlotId::new(0).unwrap()), &bytes);
        let slot = MacroSlot::from_response(&raw).unwrap();
        let content = slot.content.unwrap();
        assert_eq!(content.macro_id, 0);
        assert!(!content.copy);
        assert_eq!(content.play_mode, MacroPlayMode::Once);
        assert_eq!(content.repeat_count, 1);
        assert_eq!(content.steps.len(), 2);
        assert!(content.steps[0].is_press());
        assert_eq!(content.steps[0].kind(), 0);
        assert_eq!(content.steps[0].code, 0x04);
        assert_eq!(content.steps[0].delay_ms, 30);
        assert!(!content.steps[1].is_press());
        assert_eq!(content.steps[1].delay_ms, 0);
    }

    #[test]
    fn tolerates_odd_step_length() {
        let mut bytes = vec![0u8; 256];
        bytes[0] = 0x83;
        bytes[1] = 0x07;
        bytes[3] = 10; // not a multiple of 4
        let content = MacroSlot::parse(1, &bytes).content.unwrap();
        assert_eq!(content.steps.len(), 2);
        assert!(content.copy);
        assert_eq!(content.macro_id, 3);
        assert_eq!(content.play_mode, MacroPlayMode::Unknown(7));
        // Short buffers never panic.
        assert!(
            MacroSlot::parse(1, &[0, 0, 0, 200])
                .content
                .unwrap()
                .steps
                .is_empty()
        );
    }
}
