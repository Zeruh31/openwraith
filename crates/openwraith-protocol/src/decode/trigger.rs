//! Actuation / Rapid Trigger `21/04`, `21/05` and dead band `26 00`
//! (spec sections 4.11 and 4.12).

use super::{FromResponse, at, expect_request, need, wrong_request};
use crate::error::DecodeError;
use crate::hex::HexBytes;
use crate::layout::{KeyPos, MATRIX_SIZE};
use crate::request::ReadRequest;
use crate::response::RawResponse;
use crate::units::Distance;

/// Trigger mode of a key (reply byte 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TriggerMode {
    /// `0x00`: Rapid Trigger off (CONFIRMED).
    Off,
    /// `0x0C`: Rapid Trigger, same press/release sensitivity (CONFIRMED by c05/c06).
    RapidTrigger,
    /// `0x0D`: Rapid Trigger with separate press/release sensitivity (CODE-ONLY).
    RapidTriggerSeparate,
    /// Any other value.
    Unknown(u8),
}

impl TriggerMode {
    /// Decodes the mode byte.
    pub const fn from_byte(b: u8) -> Self {
        match b {
            0x00 => TriggerMode::Off,
            0x0C => TriggerMode::RapidTrigger,
            0x0D => TriggerMode::RapidTriggerSeparate,
            other => TriggerMode::Unknown(other),
        }
    }

    /// The mode byte on the wire.
    pub const fn byte(self) -> u8 {
        match self {
            TriggerMode::Off => 0x00,
            TriggerMode::RapidTrigger => 0x0C,
            TriggerMode::RapidTriggerSeparate => 0x0D,
            TriggerMode::Unknown(b) => b,
        }
    }

    /// Whether Rapid Trigger is enabled.
    pub const fn rapid_trigger(self) -> bool {
        matches!(
            self,
            TriggerMode::RapidTrigger | TriggerMode::RapidTriggerSeparate
        )
    }
}

/// Trigger settings of one key (`21/05` reply, 12 bytes; CONFIRMED layout).
///
/// Reply: `05 MM tt TT pp rr th TH ph rh row col` where lower-case = low byte,
/// `th`/`TH`/`ph`/`rh` = high bytes (CODE-ONLY; always 0 on the W75).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct KeyTrigger {
    /// Key position (verified against the reply's row/col echo).
    pub pos: KeyPos,
    /// Byte 6: mode.
    pub mode: TriggerMode,
    /// Bytes 7 + 11: actuation point (CONFIRMED).
    pub actuation: Distance,
    /// Bytes 8 + 12: second travel value, equal to the actuation point in every
    /// capture; ignored by the vendor site (GUESS: reset/release point).
    pub second_travel: Distance,
    /// Bytes 9 + 13: Rapid Trigger press sensitivity (CONFIRMED).
    pub rt_press: Distance,
    /// Bytes 10 + 14: Rapid Trigger release sensitivity (CONFIRMED).
    pub rt_release: Distance,
    /// The 12-byte declared payload (bytes 5..16).
    pub raw: HexBytes,
}

impl KeyTrigger {
    /// Key index (row x 22 + col).
    pub fn index(&self) -> u8 {
        self.pos.index()
    }

    /// Whether Rapid Trigger is enabled.
    pub fn rapid_trigger(&self) -> bool {
        self.mode.rapid_trigger()
    }
}

impl FromResponse for KeyTrigger {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        let ReadRequest::KeyTrigger(pos) = response.request() else {
            return Err(wrong_request(response, "KeyTrigger"));
        };
        let p = response.payload();
        need(&p, "key trigger record", 12)?;
        Ok(KeyTrigger {
            pos,
            mode: TriggerMode::from_byte(at(&p, 1)),
            actuation: Distance::from_lo_hi(at(&p, 2), at(&p, 6)),
            second_travel: Distance::from_lo_hi(at(&p, 3), at(&p, 7)),
            rt_press: Distance::from_lo_hi(at(&p, 4), at(&p, 8)),
            rt_release: Distance::from_lo_hi(at(&p, 5), at(&p, 9)),
            raw: HexBytes(p),
        })
    }
}

/// Global trigger parameters (`21/04` reply `04 aa 02 02 00 05` on the W75).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TriggerGlobal {
    /// Bytes 6 + 9: maximum travel (W75: 3.40 mm; value CONFIRMED, hi/lo split CODE-ONLY).
    pub max_travel: Distance,
    /// Byte 7: raw unit; one unit = raw/100 mm (raw/1000 with high precision).
    /// 0 is treated as 10 (CONFIRMED: W75 = 2 -> 0.02 mm).
    pub raw_unit: u8,
    /// Byte 8: unknown, not read by the vendor site (W75: 2).
    pub unknown_byte8: u8,
    /// Byte 10: minimum travel (W75: 0.10 mm; CONFIRMED).
    pub min_travel: Distance,
    /// The 6-byte declared payload.
    pub raw: HexBytes,
}

impl TriggerGlobal {
    /// Size of one trigger unit in micrometres, given the high-precision flag
    /// of [`Status`](crate::Status) (flags B bit 0). [`Distance`] assumes 20.
    pub fn unit_micrometres(&self, high_precision: bool) -> u32 {
        let unit = if self.raw_unit == 0 {
            10
        } else {
            u32::from(self.raw_unit)
        };
        if high_precision { unit } else { unit * 10 }
    }
}

impl FromResponse for TriggerGlobal {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "TriggerGlobal", |r| {
            r == ReadRequest::TriggerGlobal
        })?;
        let p = response.payload();
        need(&p, "trigger parameters", 6)?;
        Ok(TriggerGlobal {
            max_travel: Distance::from_lo_hi(at(&p, 1), at(&p, 4)),
            raw_unit: at(&p, 2),
            unknown_byte8: at(&p, 3),
            min_travel: Distance::from(at(&p, 5)),
            raw: HexBytes(p),
        })
    }
}

/// Dead band of one key (CONFIRMED framing; top/bottom order CODE-ONLY).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeadBand {
    /// Ignored travel from rest (GUESS).
    pub top: Distance,
    /// Ignored travel before bottom-out (GUESS).
    pub bottom: Distance,
}

/// Dead-band table (`26 00`, 132 x 2 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeadBandTable {
    /// 132 entries indexed by key index.
    pub entries: Vec<DeadBand>,
    /// The 264 table bytes.
    pub raw: HexBytes,
}

impl DeadBandTable {
    /// Dead band of key `index`.
    pub fn get(&self, index: u8) -> Option<DeadBand> {
        self.entries.get(usize::from(index)).copied()
    }
}

impl FromResponse for DeadBandTable {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "DeadBandTable", |r| r == ReadRequest::DeadBand)?;
        let bytes = response.table_bytes("dead band")?;
        need(&bytes, "dead band", MATRIX_SIZE * 2)?;
        let entries = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .take(MATRIX_SIZE)
            .map(|pair| DeadBand {
                top: Distance::from(at(pair, 0)),
                bottom: Distance::from(at(pair, 1)),
            })
            .collect();
        Ok(DeadBandTable {
            entries,
            raw: HexBytes(bytes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::{report, single, table};

    fn key_reply(pos: KeyPos, record: [u8; 9]) -> RawResponse {
        let mut prefix = vec![0x21, 0, 0, 0, 0x0c, 0x05];
        prefix.extend_from_slice(&record);
        prefix.extend_from_slice(&[pos.row(), pos.col()]);
        RawResponse::from_reports(ReadRequest::KeyTrigger(pos), [report(&prefix)]).unwrap()
    }

    #[test]
    fn factory_key() {
        let pos = KeyPos::new(0, 0).unwrap();
        let t =
            KeyTrigger::from_response(&key_reply(pos, [0, 0x55, 0x55, 1, 1, 0, 0, 0, 0])).unwrap();
        assert_eq!(t.mode, TriggerMode::Off);
        assert!(!t.rapid_trigger());
        assert_eq!(t.actuation.to_string(), "1.70 mm");
        assert_eq!(t.second_travel, t.actuation);
        assert_eq!(t.rt_press.to_string(), "0.02 mm");
        assert_eq!(t.rt_release.to_string(), "0.02 mm");
        assert_eq!(t.raw.len(), 12);
    }

    #[test]
    fn w_key_after_edit() {
        // c05/c06: `05 0c 32 32 0f 0f 00 00 00 00 02 02`.
        let pos = KeyPos::new(2, 2).unwrap();
        let t =
            KeyTrigger::from_response(&key_reply(pos, [0x0c, 0x32, 0x32, 0x0f, 0x0f, 0, 0, 0, 0]))
                .unwrap();
        assert_eq!(t.index(), 46);
        assert_eq!(t.mode, TriggerMode::RapidTrigger);
        assert!(t.rapid_trigger());
        assert_eq!(t.actuation.to_string(), "1.00 mm");
        assert_eq!(t.rt_press.to_string(), "0.30 mm");
        assert_eq!(t.rt_release.to_string(), "0.30 mm");
    }

    #[test]
    fn high_bytes() {
        let pos = KeyPos::new(1, 1).unwrap();
        let t = KeyTrigger::from_response(&key_reply(pos, [0x0d, 0x10, 0x10, 2, 3, 1, 1, 0, 0]))
            .unwrap();
        assert_eq!(t.mode, TriggerMode::RapidTriggerSeparate);
        assert_eq!(t.actuation.units(), 0x0110);
        assert_eq!(TriggerMode::from_byte(0x42), TriggerMode::Unknown(0x42));
        assert_eq!(TriggerMode::Unknown(0x42).byte(), 0x42);
    }

    #[test]
    fn global_parameters() {
        let g = TriggerGlobal::from_response(&single(
            ReadRequest::TriggerGlobal,
            0x21,
            0,
            &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
        ))
        .unwrap();
        assert_eq!(g.max_travel.to_string(), "3.40 mm");
        assert_eq!(g.min_travel.to_string(), "0.10 mm");
        assert_eq!(g.raw_unit, 2);
        assert_eq!(g.unknown_byte8, 2);
        assert_eq!(g.unit_micrometres(false), Distance::MICROMETRES_PER_UNIT);
        assert_eq!(g.unit_micrometres(true), 2);
    }

    #[test]
    fn short_records_are_errors() {
        // A key reply that declares 3 bytes but still echoes row/col 0/0.
        let pos = KeyPos::new(0, 0).unwrap();
        let mut bytes = [0u8; 17];
        bytes[..9].copy_from_slice(&[0x21, 0, 0, 0, 0x03, 0x05, 0, 0x55, 0x55]);
        let short = RawResponse::from_reports(ReadRequest::KeyTrigger(pos), [report(&bytes)])
            .expect("header and echo match");
        assert!(matches!(
            KeyTrigger::from_response(&short),
            Err(DecodeError::TooShort {
                need: 12,
                got: 3,
                ..
            })
        ));
        let short = single(ReadRequest::TriggerGlobal, 0x21, 0, &[0x04, 0xaa]);
        assert!(matches!(
            TriggerGlobal::from_response(&short),
            Err(DecodeError::TooShort {
                need: 6,
                got: 2,
                ..
            })
        ));
    }

    #[test]
    fn dead_band_table() {
        let mut bytes = vec![0u8; 264];
        // W (index 46): flat offsets 92/93.
        bytes[92] = 5;
        bytes[93] = 10;
        let t = DeadBandTable::from_response(&table(ReadRequest::DeadBand, &bytes)).unwrap();
        assert_eq!(t.entries.len(), 132);
        let w = t.get(46).unwrap();
        assert_eq!(w.top.to_string(), "0.10 mm");
        assert_eq!(w.bottom.to_string(), "0.20 mm");
        assert_eq!(t.get(0), Some(DeadBand::default()));
    }
}
