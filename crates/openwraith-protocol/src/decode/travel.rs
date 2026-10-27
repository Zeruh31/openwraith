//! Live travel-stream reports `21 00 00 00 (05|03) 01 RR CC DL DH ..`
//! (spec section 4.11.7, c14).
//!
//! The keyboard sends one report per depth change of one key while a travel
//! test is open. Two variants (OBSERVED): short (byte 4 = `05`) during fast
//! motion, and long (byte 4 = `03`) on a ~72 ms tick during slow motion.
//! The long variant declares length 3 but carries data up to byte 36, so it
//! is decoded past the declared length (the one exception to spec section
//! 2.3). Depth (bytes 8-9, trigger units) and the row/column are CONFIRMED;
//! the long-variant fields are OBSERVED with GUESSED meanings and kept raw.

use crate::frame::Report;
use crate::layout::KeyPos;

/// Extra fields of the long variant (OBSERVED; meanings GUESS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LongTravelFields {
    /// Byte 16: `01` deeper than the key's previous report, `02` shallower,
    /// `00` at depth 0.
    pub motion: u8,
    /// Bytes 19-20 and 21-22 (LE16): two sensor readings that fall as the
    /// key goes down.
    pub sensors: [u16; 2],
    /// Bytes 12-13, 14-15 and 31-32 (LE16): per-key constants (GUESS:
    /// calibrated top, a second top or noise bound, calibrated bottom).
    pub constants: [u16; 3],
    /// Bytes 35-36 (LE16): a word that changes over time, not tied to the
    /// key; meaning unknown.
    pub tail: u16,
}

/// Which variant a stream report is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TravelVariant {
    /// Byte 4 = `05`: depth only.
    Short,
    /// Byte 4 = `03`: depth plus the long-variant fields.
    Long(LongTravelFields),
}

/// One live travel-stream report: one key and its depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TravelReport {
    /// The key that moved (bytes 6-7 = row, column; OBSERVED).
    pub pos: KeyPos,
    /// Depth in trigger units, 0 at rest (bytes 8-9, LE; the high byte is
    /// CODE-ONLY, 0 on the W75 whose maximum is 170).
    pub depth: u16,
    /// Short or long report.
    pub variant: TravelVariant,
}

impl TravelReport {
    /// Parses a stream report: `21 00 00 00`, byte 4 `05` or `03`, byte 5
    /// `01`, row < 6, column < 22. Anything else is `None`.
    pub fn parse(report: &Report) -> Option<TravelReport> {
        let b = report.as_bytes();
        if b[..4] != [0x21, 0x00, 0x00, 0x00] || b[5] != 0x01 {
            return None;
        }
        let le = |at: usize| u16::from_le_bytes([report.byte(at), report.byte(at + 1)]);
        let variant = match b[4] {
            0x05 => TravelVariant::Short,
            0x03 => TravelVariant::Long(LongTravelFields {
                motion: report.byte(16),
                sensors: [le(19), le(21)],
                constants: [le(12), le(14), le(31)],
                tail: le(35),
            }),
            _ => return None,
        };
        Some(TravelReport {
            pos: KeyPos::new(report.byte(6), report.byte(7))?,
            depth: le(8),
            variant,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::report;

    #[test]
    fn short_and_long() {
        // c14 #1157: Esc at 169.
        let esc =
            TravelReport::parse(&report(&[0x21, 0, 0, 0, 0x05, 0x01, 0, 0, 0xa9, 0])).unwrap();
        assert_eq!(esc.pos, KeyPos::new(0, 0).unwrap());
        assert_eq!(esc.depth, 169);
        assert_eq!(esc.variant, TravelVariant::Short);
        // c14 #999: W at 17, long.
        let w = TravelReport::parse(&report(&[
            0x21, 0x00, 0x00, 0x00, 0x03, 0x01, 0x02, 0x02, 0x11, 0x00, 0xff, 0xff, 0x9b, 0x09,
            0xad, 0x09, 0x01, 0x11, 0x00, 0x82, 0x09, 0x82, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x87, 0x06, 0xff, 0xff,
        ]))
        .unwrap();
        assert_eq!(w.pos.index(), 46);
        assert_eq!(w.depth, 17);
        assert_eq!(
            w.variant,
            TravelVariant::Long(LongTravelFields {
                motion: 1,
                sensors: [2434, 2434],
                constants: [2459, 2477, 1671],
                tail: 0,
            })
        );
    }

    #[test]
    fn other_reports_are_not_stream_reports() {
        for prefix in [
            &[0x21, 0, 0, 0, 0x01, 0x00][..],            // trigger / travel ack
            &[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x00],    // profile push
            &[0x21, 0, 0, 0, 0x0c, 0x05, 0, 0x55, 0x55], // 21/05 reply
            &[0x21, 0, 0, 0, 0x06, 0x04, 0xaa, 0x02],    // 21/04 reply
            &[0x21, 0, 0, 0, 0x04, 0x01, 0, 0, 5, 0],    // other byte 4
            &[0x21, 0, 0, 1, 0x05, 0x01, 0, 0, 5, 0],    // chunk index
            &[0x21, 0, 0, 0, 0x05, 0x01, 6, 0, 5, 0],    // row 6
            &[0x21, 0, 0, 0, 0x05, 0x01, 0, 22, 5, 0],   // column 22
            &[0x24, 0x02, 0, 0, 0x06, 0x01, 0, 0, 0, 0xfa, 0xe5], // SOCD push
        ] {
            assert_eq!(TravelReport::parse(&report(prefix)), None, "{prefix:02x?}");
        }
    }
}
