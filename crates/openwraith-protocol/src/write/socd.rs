//! SOCD writes (spec section 4.15): the power switch (`24 03`) and the pair
//! table (`24 00`, two 40-byte chunks).
//!
//! The site sends both on every SOCD action: power, then chunk 0, then
//! chunk 1 (CONFIRMED, c15, 12 bursts; `setPrcsPower`/`setPrcsData`,
//! `agreement.js:2029-2087`). [`SocdWrite`] always produces that whole
//! burst; a power-only change re-sends the unchanged pairs, as the site's
//! switch does. The pair table is packed from entry 0 with entries
//! `01 <mode> <key1 usage> <key2 usage>`; unused entries are `00 00 00 00`.
//! The non-US `0x31 -> 0x32` substitution of the site is never applied.

use super::evidence::UnverifiedNote;
use super::keymap::HidUsage;
use crate::error::WriteBuildError;
use crate::model::{Model, known_model};
use crate::request::ReadRequest;
use crate::response::RawResponse;

/// Size of the SOCD pair table in bytes (20 entries x 4).
pub const SOCD_PAIRS_LEN: usize = 80;

/// How a SOCD pair resolves both keys being held (pair byte 1). The vendor
/// UI numbers the options 1-4, the wire 0-3 (spec section 4.15 table).
/// Modes 1 and 4 behave as named (CONFIRMED, hardware test H21,
/// 2026-10-27); modes 2 and 3 are untested (Q35).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SocdMode {
    /// Wire 0, UI "Model1" (default): the last input wins.
    M1 = 0,
    /// Wire 1, UI "Model2": key 1 wins.
    M2 = 1,
    /// Wire 2, UI "Model3": key 2 wins.
    M3 = 2,
    /// Wire 3, UI "Model4": the later key only interrupts the first.
    M4 = 3,
}

impl SocdMode {
    /// The four modes.
    pub const ALL: [SocdMode; 4] = [SocdMode::M1, SocdMode::M2, SocdMode::M3, SocdMode::M4];

    /// The wire value 0..=3.
    pub const fn wire(self) -> u8 {
        self as u8
    }

    /// The UI number 1..=4.
    pub const fn number(self) -> u8 {
        self as u8 + 1
    }

    /// The mode for wire value 0..=3.
    pub const fn from_wire(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(SocdMode::M1),
            1 => Some(SocdMode::M2),
            2 => Some(SocdMode::M3),
            3 => Some(SocdMode::M4),
            _ => None,
        }
    }
}

/// One SOCD pair: two keys (by HID usage, CONFIRMED: A/D = `04 07`, c15)
/// and how they resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SocdPair {
    /// Resolution mode.
    pub mode: SocdMode,
    /// Key 1.
    pub key1: HidUsage,
    /// Key 2.
    pub key2: HidUsage,
}

impl SocdPair {
    /// The 4 entry bytes `01 mode key1 key2`.
    const fn entry(&self) -> [u8; 4] {
        [0x01, self.mode.wire(), self.key1.get(), self.key2.get()]
    }
}

/// The 80-byte SOCD pair table in the site's form: entries
/// `01 <mode> <key1 usage> <key2 usage>` packed from entry 0, unused
/// entries `00 00 00 00`, at most 20 pairs, no key in two pairs.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SocdPairsData(Box<[u8; SOCD_PAIRS_LEN]>);

hex_table!(SocdPairsData);

impl SocdPairsData {
    /// Largest number of pairs (`wmIndex.js:2404, 2418`).
    pub const MAX_PAIRS: usize = 20;

    /// The table for `pairs`, with the site's rules (`isPrcsKeyDuplicate`,
    /// `wmIndex.js:3670-3675`): key 1 differs from key 2, no key is used in
    /// two pairs, at most 20 pairs, and both keys are selectable keys of the
    /// model of [`MODELS`](crate::MODELS) with `model`'s product code.
    pub fn from_pairs(model: &Model, pairs: &[SocdPair]) -> Result<Self, WriteBuildError> {
        let known = known_model(model).ok_or(WriteBuildError::UnknownModel {
            product: model.product_code,
        })?;
        let selectable = |usage: HidUsage| {
            known
                .keys
                .iter()
                .any(|k| k.hid_usage == Some(usage.get()) && Some(k.index) != known.knob_index)
        };
        for pair in pairs {
            for usage in [pair.key1, pair.key2] {
                if !selectable(usage) {
                    return Err(WriteBuildError::NotOffered {
                        what: "SOCD key",
                        value: usage.get(),
                    });
                }
            }
        }
        check_pairs(pairs.iter().map(|p| (p.key1.get(), p.key2.get())))?;
        let mut bytes = Box::new([0u8; SOCD_PAIRS_LEN]);
        for (dst, pair) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(pairs) {
            dst.copy_from_slice(&pair.entry());
        }
        Ok(SocdPairsData(bytes))
    }

    /// The table of a `24 01` reply, if it is in the site's form: used
    /// entries `01 mode key1 key2` (mode 0-3, offered usages, the rules of
    /// [`from_pairs`](Self::from_pairs)) packed from entry 0, then
    /// `00 00 00 00`. Gaps, `07 FF FF FF` and other byte-0 values fail with
    /// [`WriteBuildError::UnwritableSocdPairs`].
    pub fn from_response(raw: &RawResponse) -> Result<Self, WriteBuildError> {
        if raw.request() != ReadRequest::SocdPairs {
            return Err(WriteBuildError::WrongRequest {
                expected: "the SOCD pairs",
                got: raw.request(),
            });
        }
        let table = raw.table_bytes("SOCD pairs")?;
        let got = table.len();
        let bytes: Box<[u8; SOCD_PAIRS_LEN]> =
            table
                .into_boxed_slice()
                .try_into()
                .map_err(|_| WriteBuildError::TableLength {
                    what: "SOCD pairs",
                    expected: SOCD_PAIRS_LEN,
                    got,
                })?;
        let unwritable = |reason| WriteBuildError::UnwritableSocdPairs { reason };
        let mut pairs = Vec::new();
        let mut ended = false;
        for entry in bytes.as_chunks::<4>().0 {
            match *entry {
                [0x00, 0x00, 0x00, 0x00] => ended = true,
                [0x01, ..] if ended => return Err(unwritable("a gap between pairs")),
                [0x01, mode, key1, key2] => {
                    if SocdMode::from_wire(mode).is_none() {
                        return Err(unwritable("a mode above 3"));
                    }
                    if HidUsage::new(key1).is_none() || HidUsage::new(key2).is_none() {
                        return Err(unwritable("a key the site does not offer"));
                    }
                    pairs.push((key1, key2));
                }
                [0x07, 0xFF, 0xFF, 0xFF] => return Err(unwritable("an entry 07 FF FF FF")),
                _ => return Err(unwritable("an entry that is neither used nor zero")),
            }
        }
        check_pairs(pairs.into_iter())?;
        Ok(SocdPairsData(bytes))
    }

    /// The pairs in table order.
    pub fn pairs(&self) -> Vec<SocdPair> {
        self.0
            .as_chunks::<4>()
            .0
            .iter()
            .map_while(|entry| match *entry {
                [0x01, mode, key1, key2] => Some(SocdPair {
                    mode: SocdMode::from_wire(mode)?,
                    key1: HidUsage::new(key1)?,
                    key2: HidUsage::new(key2)?,
                }),
                _ => None,
            })
            .collect()
    }

    /// The 80 table bytes.
    pub fn as_bytes(&self) -> &[u8; SOCD_PAIRS_LEN] {
        &self.0
    }
}

/// The site's pair rules on `(key1, key2)` usages: at most 20 pairs, two
/// different keys per pair, no key in two pairs.
fn check_pairs(pairs: impl Iterator<Item = (u8, u8)>) -> Result<(), WriteBuildError> {
    let mut seen: Vec<u8> = Vec::new();
    let mut count = 0usize;
    for (key1, key2) in pairs {
        count += 1;
        if key1 == key2 {
            return Err(WriteBuildError::SocdSameKey { usage: key1 });
        }
        for usage in [key1, key2] {
            if seen.contains(&usage) {
                return Err(WriteBuildError::SocdDuplicateKey { usage });
            }
            seen.push(usage);
        }
    }
    if count > SocdPairsData::MAX_PAIRS {
        return Err(WriteBuildError::TooManySocdPairs { count });
    }
    Ok(())
}

/// One SOCD write: the power switch and the whole pair table (W11).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SocdWrite {
    power: bool,
    pairs: SocdPairsData,
}

impl SocdWrite {
    /// Power `power` (`24 03 .. 01 PP`) and `pairs` (`24 00`).
    pub fn new(power: bool, pairs: SocdPairsData) -> Self {
        SocdWrite { power, pairs }
    }

    /// Whether SOCD is switched on.
    pub fn power(&self) -> bool {
        self.power
    }

    /// The pair table.
    pub fn pairs(&self) -> &SocdPairsData {
        &self.pairs
    }

    /// What is not verified about this write (Q35): modes 2 and 3. The
    /// power write and pairs in modes 1 and 4 take effect as named
    /// (CONFIRMED, hardware test H21, 2026-10-27).
    pub fn evidence(&self) -> Vec<UnverifiedNote> {
        let untested = self
            .pairs
            .pairs()
            .iter()
            .any(|p| matches!(p.mode, SocdMode::M2 | SocdMode::M3));
        if untested {
            vec![UnverifiedNote::new("SOCD modes 2 and 3", "Q35")]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::table;
    use crate::model::W75;

    fn usage(u: u8) -> HidUsage {
        HidUsage::new(u).unwrap()
    }

    fn pair(mode: SocdMode, key1: u8, key2: u8) -> SocdPair {
        SocdPair {
            mode,
            key1: usage(key1),
            key2: usage(key2),
        }
    }

    #[test]
    fn site_form() {
        // c15 #104: A/D with Model1, #247: Model2.
        let data = SocdPairsData::from_pairs(&W75, &[pair(SocdMode::M1, 0x04, 0x07)]).unwrap();
        assert_eq!(data.as_bytes()[..4], [0x01, 0x00, 0x04, 0x07]);
        assert!(data.as_bytes()[4..].iter().all(|b| *b == 0));
        let data = SocdPairsData::from_pairs(&W75, &[pair(SocdMode::M2, 0x04, 0x07)]).unwrap();
        assert_eq!(data.as_bytes()[..4], [0x01, 0x01, 0x04, 0x07]);
        assert_eq!(data.pairs(), vec![pair(SocdMode::M2, 0x04, 0x07)]);
        let empty = SocdPairsData::from_pairs(&W75, &[]).unwrap();
        assert!(empty.as_bytes().iter().all(|b| *b == 0));
        assert!(empty.pairs().is_empty());
        assert_eq!(SocdMode::M4.number(), 4);
        assert_eq!(SocdMode::from_wire(4), None);
    }

    #[test]
    fn pair_rules() {
        let keys: Vec<u8> = W75.keys.iter().filter_map(|k| k.hid_usage).collect();
        let twenty: Vec<SocdPair> = (0..20)
            .map(|i| pair(SocdMode::ALL[i % 4], keys[2 * i], keys[2 * i + 1]))
            .collect();
        let data = SocdPairsData::from_pairs(&W75, &twenty).unwrap();
        assert_eq!(data.pairs(), twenty);
        let mut more = twenty.clone();
        more.push(pair(SocdMode::M1, keys[40], keys[41]));
        assert_eq!(
            SocdPairsData::from_pairs(&W75, &more),
            Err(WriteBuildError::TooManySocdPairs { count: 21 })
        );
        assert_eq!(
            SocdPairsData::from_pairs(&W75, &[pair(SocdMode::M1, 0x04, 0x04)]),
            Err(WriteBuildError::SocdSameKey { usage: 0x04 })
        );
        assert_eq!(
            SocdPairsData::from_pairs(
                &W75,
                &[
                    pair(SocdMode::M1, 0x04, 0x07),
                    pair(SocdMode::M1, 0x1A, 0x07)
                ]
            ),
            Err(WriteBuildError::SocdDuplicateKey { usage: 0x07 })
        );
        // F13 is offered by the site but is not a W75 key.
        assert_eq!(
            SocdPairsData::from_pairs(&W75, &[pair(SocdMode::M1, 0x04, 0x68)]),
            Err(WriteBuildError::NotOffered {
                what: "SOCD key",
                value: 0x68
            })
        );
    }

    #[test]
    fn read_back_tables() {
        let read =
            |bytes: &[u8]| SocdPairsData::from_response(&table(ReadRequest::SocdPairs, bytes));
        assert_eq!(read(&[0u8; 80]).map(|d| d.pairs()), Ok(vec![]));
        let mut one = [0u8; 80];
        one[..4].copy_from_slice(&[0x01, 0x03, 0x1A, 0x16]);
        assert_eq!(
            read(&one).map(|d| d.pairs()),
            Ok(vec![pair(SocdMode::M4, 0x1A, 0x16)])
        );
        // Entries 10-19 live in chunk 1.
        let mut both = [0u8; 80];
        for i in 0..11 {
            let k = u8::try_from(4 + 2 * i).unwrap();
            both[4 * i..4 * i + 4].copy_from_slice(&[0x01, 0x00, k, k + 1]);
        }
        assert_eq!(read(&both).map(|d| d.pairs().len()), Ok(11));
        let unwritable = |reason| Err(WriteBuildError::UnwritableSocdPairs { reason });
        let cases: [(&[u8], Result<usize, WriteBuildError>); 6] = [
            (
                &[0, 0, 0, 0, 0x01, 0, 0x04, 0x07],
                unwritable("a gap between pairs"),
            ),
            (
                &[0x07, 0xFF, 0xFF, 0xFF],
                unwritable("an entry 07 FF FF FF"),
            ),
            (
                &[0x02, 0, 0x04, 0x07],
                unwritable("an entry that is neither used nor zero"),
            ),
            (
                &[0x00, 0, 0x04, 0x07],
                unwritable("an entry that is neither used nor zero"),
            ),
            (&[0x01, 4, 0x04, 0x07], unwritable("a mode above 3")),
            (
                &[0x01, 0, 0x04, 0x32],
                unwritable("a key the site does not offer"),
            ),
        ];
        for (prefix, expected) in cases {
            let mut bytes = [0u8; 80];
            bytes[..prefix.len()].copy_from_slice(prefix);
            assert_eq!(
                read(&bytes).map(|d| d.pairs().len()),
                expected,
                "{prefix:02x?}"
            );
        }
        let mut dup = [0u8; 80];
        dup[..8].copy_from_slice(&[0x01, 0, 0x04, 0x07, 0x01, 0, 0x07, 0x1A]);
        assert_eq!(
            read(&dup).map(|d| d.pairs().len()),
            Err(WriteBuildError::SocdDuplicateKey { usage: 0x07 })
        );
        assert!(matches!(
            SocdPairsData::from_response(&table(ReadRequest::DeadBand, &[0u8; 264])),
            Err(WriteBuildError::WrongRequest { .. })
        ));
    }

    #[test]
    fn evidence_notes() {
        // H21: the power write and modes 1 and 4 are confirmed.
        let empty = SocdWrite::new(true, SocdPairsData::from_pairs(&W75, &[]).unwrap());
        assert!(empty.evidence().is_empty());
        let m4 = SocdPairsData::from_pairs(
            &W75,
            &[
                pair(SocdMode::M4, 0x04, 0x07),
                pair(SocdMode::M1, 0x1A, 0x16),
            ],
        )
        .unwrap();
        let write = SocdWrite::new(false, m4);
        assert!(write.evidence().is_empty());
        assert!(!write.power());
        assert_eq!(write.pairs().pairs().len(), 2);
        for mode in [SocdMode::M2, SocdMode::M3] {
            let pairs = SocdPairsData::from_pairs(&W75, &[pair(mode, 0x04, 0x07)]).unwrap();
            let notes: Vec<String> = SocdWrite::new(true, pairs)
                .evidence()
                .iter()
                .map(ToString::to_string)
                .collect();
            assert_eq!(notes, ["SOCD modes 2 and 3 (Q35)"], "{mode:?}");
        }
    }
}
