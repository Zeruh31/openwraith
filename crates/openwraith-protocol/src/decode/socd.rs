//! SOCD ("PRCS") `24 02` settings and `24 01` pairs (spec section 4.15).

use super::{FromResponse, at, expect_request, need};
use crate::error::DecodeError;
use crate::hex::HexBytes;
use crate::request::ReadRequest;
use crate::response::RawResponse;

/// SOCD power and hotkey (`24 02` reply `00 00 00 00 fa e5` on the W75; CONFIRMED bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SocdSettings {
    /// Payload byte 0 = 1: SOCD on (W75: off).
    pub enabled: bool,
    /// Payload bytes 1-3: unknown (0 on the W75).
    pub unknown: HexBytes,
    /// Payload bytes 4-5: hotkey shown as "Fn + R-Shift" (`FA E5`; GUESS:
    /// on-keyboard SOCD toggle).
    pub hotkey: [u8; 2],
    /// The 6-byte declared payload.
    pub raw: HexBytes,
}

impl FromResponse for SocdSettings {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "SocdSettings", |r| r == ReadRequest::SocdSettings)?;
        let p = response.payload();
        need(&p, "SOCD settings", 6)?;
        Ok(SocdSettings {
            enabled: at(&p, 0) == 1,
            unknown: HexBytes(p.get(1..4).unwrap_or_default().to_vec()),
            hotkey: [at(&p, 4), at(&p, 5)],
            raw: HexBytes(p),
        })
    }
}

/// One SOCD pair entry `[active, model, key1, key2]` (CODE-ONLY).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SocdEntry {
    /// Entry number (0..20).
    pub index: u8,
    /// Byte 0 = 1: entry in use.
    pub active: bool,
    /// Byte 1: resolution model (0 last input wins, 1 key1 wins, 2 key2 wins,
    /// 3 the later key only interrupts the first).
    pub model: u8,
    /// Byte 2: HID usage of key 1.
    pub key1: u8,
    /// Byte 3: HID usage of key 2.
    pub key2: u8,
    /// The raw 4 bytes.
    pub raw: HexBytes,
}

impl SocdEntry {
    /// Whether the entry is unused (byte 0 = 0, or the entry is `07 FF FF FF`).
    pub fn is_empty(&self) -> bool {
        !self.active || self.raw.0 == [0x07, 0xFF, 0xFF, 0xFF]
    }
}

/// SOCD pairs (`24 01`, 2 chunks x 40 bytes = 20 entries; CONFIRMED framing).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SocdPairs {
    /// 20 entries in order.
    pub entries: Vec<SocdEntry>,
    /// The 80 table bytes.
    pub raw: HexBytes,
}

impl SocdPairs {
    /// Entries in use.
    pub fn used(&self) -> impl Iterator<Item = &SocdEntry> {
        self.entries.iter().filter(|e| !e.is_empty())
    }
}

impl FromResponse for SocdPairs {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "SocdPairs", |r| r == ReadRequest::SocdPairs)?;
        let bytes = response.table_bytes("SOCD pairs")?;
        need(&bytes, "SOCD pairs", 80)?;
        let entries = (0u8..)
            .zip(bytes.as_chunks::<4>().0.iter())
            .map(|(index, e)| SocdEntry {
                index,
                active: at(e, 0) == 1,
                model: at(e, 1),
                key1: at(e, 2),
                key2: at(e, 3),
                raw: HexBytes(e.to_vec()),
            })
            .collect();
        Ok(SocdPairs {
            entries,
            raw: HexBytes(bytes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::{single, table};

    #[test]
    fn w75_settings() {
        let s = SocdSettings::from_response(&single(
            ReadRequest::SocdSettings,
            0x24,
            0x02,
            &[0, 0, 0, 0, 0xfa, 0xe5],
        ))
        .unwrap();
        assert!(!s.enabled);
        assert_eq!(s.hotkey, [0xfa, 0xe5]);
        assert_eq!(s.unknown.len(), 3);
    }

    #[test]
    fn pairs() {
        let mut bytes = vec![0u8; 80];
        bytes[..4].copy_from_slice(&[1, 1, 0x04, 0x07]); // A/D, key1 wins
        bytes[4..8].copy_from_slice(&[0x07, 0xFF, 0xFF, 0xFF]);
        let p = SocdPairs::from_response(&table(ReadRequest::SocdPairs, &bytes)).unwrap();
        assert_eq!(p.entries.len(), 20);
        let used: Vec<_> = p.used().collect();
        assert_eq!(used.len(), 1);
        assert_eq!((used[0].key1, used[0].key2, used[0].model), (0x04, 0x07, 1));
    }
}
