//! Advanced-key slot content (spec section 4.14): DKS (type 1), Mod-Tap
//! (type 2) and Toggle (type 3), the three types the W75 offers.
//!
//! A slot is 40 bytes: the payload of its `22 80` read chunk, and write bytes
//! 5-44 (the write carries 50 payload bytes; the last 10 are zero). Read and
//! write offsets are identical (CONFIRMED for TGL by c16 #1320 and for DKS,
//! MT and TGL by OpenWraith's snapshot after c17). The encoding follows the
//! site's writer (`agreement.js:1945-1984`) and is CONFIRMED byte for byte
//! by c16 (DKS #2302, MT #3072, TGL #3563).
//!
//! Actions are always keyboard usages `00 uu 00 00`: the W75 does not offer
//! mouse actions in DKS (status flag), and MT/TGL store a mouse action in
//! the site's unswapped order, which OpenWraith never writes.

use std::fmt;

use super::evidence::UnverifiedNote;
use super::keymap::HidUsage;
use crate::decode::TriggerGlobal;
use crate::error::WriteBuildError;
use crate::model::{MODELS, Model, known_model};

/// Bytes per advanced-key slot (the `22 80` read chunk; write bytes 5-44).
pub const ADVANCED_SLOT_LEN: usize = 40;

/// Type byte of a DKS slot.
const TYPE_DKS: u8 = 1;
/// Type byte of a Mod-Tap slot.
const TYPE_MT: u8 = 2;
/// Type byte of a Toggle slot.
const TYPE_TGL: u8 = 3;
/// The Mod-Tap hold threshold tested on hardware (300 ms, H20; the CLI's
/// default).
const MT_TESTED_THRESHOLD: u8 = 30;

/// The 40 bytes of a non-empty advanced-key slot that OpenWraith may write
/// (write `22 SS 00 00 32` + these bytes + 10 zero bytes).
///
/// Built only from a validated [`AdvancedKeyDef`]
/// ([`from_def`](Self::from_def)) or from a slot read from the keyboard or a
/// backup that re-encodes exactly from such a definition
/// ([`from_read_slot`](Self::from_read_slot)). An empty slot is cleared
/// with [`WriteRequest::AdvancedKeyClear`](super::WriteRequest::AdvancedKeyClear)
/// instead.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct AdvancedSlotData(Box<[u8; ADVANCED_SLOT_LEN]>);

hex_table!(AdvancedSlotData);

impl AdvancedSlotData {
    /// The content of one slot as read (the 40 payload bytes of its `22 80`
    /// chunk). `Ok(None)` for an empty (all-zero) slot.
    ///
    /// Anything else must be a DKS, MT or TGL slot that a valid
    /// [`AdvancedKeyDef`] encodes to exactly: keyboard-usage actions the
    /// site offers, the DKS step grammar, `1 <= r1 <= r2 == r3`, `r4 == r1`,
    /// `r2` at most the model ceiling, an MT threshold of 15..=100 and zero
    /// bytes after the type's layout. Otherwise it fails with
    /// [`WriteBuildError::UnwritableAdvancedKey`] (or the validation error).
    pub fn from_read_slot(bytes: &[u8]) -> Result<Option<Self>, WriteBuildError> {
        let Ok(bytes) = <[u8; ADVANCED_SLOT_LEN]>::try_from(bytes) else {
            return Err(WriteBuildError::TableLength {
                what: "advanced-key slot",
                expected: ADVANCED_SLOT_LEN,
                got: bytes.len(),
            });
        };
        if bytes.iter().all(|b| *b == 0) {
            return Ok(None);
        }
        let def = parse(&bytes)?;
        // The deep press point is fixed by the keyboard's travel; a slot
        // read back carries it, bounded by the largest ceiling we know.
        let ceilings = widest_ceilings();
        let dks_bottom = match def {
            AdvancedKeyDef::Dks(_) => bytes[2],
            _ => ceilings.dks_bottom,
        };
        if dks_bottom > ceilings.dks_bottom {
            return Err(WriteBuildError::DksPointOutOfRange {
                value: dks_bottom,
                max: ceilings.dks_bottom,
            });
        }
        let limits = AdvancedLimits {
            dks_bottom,
            ..ceilings
        };
        def.validate(&limits)?;
        if encode(&def, dks_bottom) != bytes {
            return Err(WriteBuildError::UnwritableAdvancedKey {
                reason: "bytes outside the DKS, MT or TGL layout",
            });
        }
        Ok(Some(AdvancedSlotData(Box::new(bytes))))
    }

    /// The slot content of `def`, after checking it against `limits`.
    pub fn from_def(
        def: &AdvancedKeyDef,
        limits: &AdvancedLimits,
    ) -> Result<Self, WriteBuildError> {
        def.validate(limits)?;
        Ok(AdvancedSlotData(Box::new(encode(def, limits.dks_bottom))))
    }

    /// The 40 slot bytes.
    pub fn as_bytes(&self) -> &[u8; ADVANCED_SLOT_LEN] {
        &self.0
    }

    /// The definition this slot holds (always `Some` for a slot built by
    /// this type).
    pub fn def(&self) -> Option<AdvancedKeyDef> {
        parse(&self.0).ok()
    }

    /// The type byte (1 DKS, 2 MT, 3 TGL).
    pub fn type_byte(&self) -> u8 {
        self.0[0]
    }
}

/// Limits for advanced-key definitions: the DKS deep press point (0.1 mm)
/// and the Mod-Tap hold threshold range (10 ms).
///
/// Built by [`AdvancedLimits::new`] from the keyboard's `21/04` travel and
/// capped by the model's compiled-in [`WriteCeilings`](crate::WriteCeilings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AdvancedLimits {
    dks_bottom: u8,
    mt_threshold_min: u8,
    mt_threshold_max: u8,
}

impl AdvancedLimits {
    /// The deep press point is `max travel x unit / 100 - 4` in 0.1 mm
    /// (`wmIndex.js:4161-4170`; 170 x 20 um -> 34 - 4 = 30 = 3.0 mm on the
    /// W75), capped by the model's ceiling. Only a trigger unit of 20 um is
    /// supported (`unit_um`, from
    /// [`TriggerGlobal::unit_micrometres`](crate::TriggerGlobal::unit_micrometres)).
    pub fn new(
        model: &Model,
        global: &TriggerGlobal,
        unit_um: u32,
    ) -> Result<AdvancedLimits, WriteBuildError> {
        let known = known_model(model).ok_or(WriteBuildError::UnknownModel {
            product: model.product_code,
        })?;
        if unit_um != 20 {
            return Err(WriteBuildError::UnsupportedUnit { unit_um });
        }
        let ceilings = known.write_ceilings;
        let tenths = u32::from(global.max_travel.units()) * unit_um / 100;
        let bottom = tenths
            .checked_sub(4)
            .filter(|b| *b >= 1)
            .ok_or(WriteBuildError::TriggerLimitsUnsupported)?
            .min(u32::from(ceilings.dks_bottom));
        Ok(AdvancedLimits {
            dks_bottom: u8::try_from(bottom).unwrap_or(ceilings.dks_bottom),
            mt_threshold_min: ceilings.mt_threshold_min,
            mt_threshold_max: ceilings.mt_threshold_max,
        })
    }

    /// The DKS deep press point in 0.1 mm (30 on the W75): the largest first
    /// press point, and the value written to route bytes 2 and 3.
    pub fn dks_bottom(&self) -> u8 {
        self.dks_bottom
    }

    /// Smallest and largest Mod-Tap hold threshold in 10 ms (15 and 100).
    pub fn mt_threshold_range(&self) -> (u8, u8) {
        (self.mt_threshold_min, self.mt_threshold_max)
    }
}

/// The widest limits over every known model (used for slots read back,
/// which carry their own deep press point).
fn widest_ceilings() -> AdvancedLimits {
    let ceilings = MODELS.iter().map(|m| m.write_ceilings);
    AdvancedLimits {
        dks_bottom: ceilings.clone().map(|c| c.dks_bottom).max().unwrap_or(0),
        mt_threshold_min: ceilings
            .clone()
            .map(|c| c.mt_threshold_min)
            .min()
            .unwrap_or(u8::MAX),
        mt_threshold_max: ceilings.map(|c| c.mt_threshold_max).max().unwrap_or(0),
    }
}

/// What an advanced-key slot does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AdvancedKeyDef {
    /// Type 1: dynamic keystroke.
    Dks(Dks),
    /// Type 2: tap / hold.
    ModTap(ModTap),
    /// Type 3: toggle.
    Toggle(Toggle),
}

/// A DKS: up to four keys, each with an action at four stages of the key's
/// travel (spec section 4.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Dks {
    /// First press point in **0.1 mm** (1..=deep press point; UI default
    /// 15 = 1.5 mm). Written to route bytes 1 and 4.
    pub first_point: u8,
    /// The four rows; `None` = unused (8 zero bytes).
    pub rows: [Option<DksRow>; 4],
}

/// One DKS row: a key and what it does at the four stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DksRow {
    /// The key the row sends.
    pub key: HidUsage,
    /// Stages: press past the first point, bottom-out, lift from the bottom,
    /// release past the first point.
    pub steps: [DksStep; 4],
}

/// What a DKS row does at one stage (step byte values of the site's UI,
/// `wmIndex.js:3725-3779`). `Tap` at stages 1 and 2 is CONFIRMED by c16,
/// `Tap` at stage 3 and `Hold` at stages 1-3 by hardware test H20
/// (2026-10-27); `Tap` at stage 4 and `HoldAgain` are untested (Q19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum DksStep {
    /// 0: nothing.
    None = 0,
    /// 1: a single short press, no auto-repeat while the key stays in the
    /// stage (CONFIRMED, H20).
    Tap = 1,
    /// 2: held, with the host's auto-repeat, while the key stays in this
    /// stage; reaching the next stage releases it (CONFIRMED, H20: a hold at
    /// stage 1 ends at the bottom-out, holds at stages 1-3 repeat while the
    /// key rests at the bottom).
    Hold = 2,
    /// 3: released, then pressed and held again right after a held stage
    /// (GUESS).
    HoldAgain = 3,
}

impl DksStep {
    /// The step for a byte value 0..=3.
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(DksStep::None),
            1 => Some(DksStep::Tap),
            2 => Some(DksStep::Hold),
            3 => Some(DksStep::HoldAgain),
            _ => None,
        }
    }

    /// The byte value.
    pub const fn byte(self) -> u8 {
        self as u8
    }
}

/// A Mod-Tap: one key when tapped, another when held longer than the
/// threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ModTap {
    /// The key sent when held (write bytes 6-9).
    pub hold: HidUsage,
    /// The key sent when tapped (write bytes 10-13).
    pub tap: HidUsage,
    /// Hold threshold in 10 ms units, 15..=100 (150-1000 ms; byte 14).
    pub threshold_10ms: u8,
}

/// A Toggle: one press holds the key down, the next releases it
/// (CONFIRMED, hardware test H20).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Toggle {
    /// The key.
    pub key: HidUsage,
}

impl AdvancedKeyDef {
    /// The type byte (1 DKS, 2 MT, 3 TGL).
    pub const fn type_byte(&self) -> u8 {
        match self {
            AdvancedKeyDef::Dks(_) => TYPE_DKS,
            AdvancedKeyDef::ModTap(_) => TYPE_MT,
            AdvancedKeyDef::Toggle(_) => TYPE_TGL,
        }
    }

    /// Checks the definition against `limits` and the site's rules:
    ///
    /// * DKS: `1 <= first_point <= dks_bottom`; at least one row; every row
    ///   has at least one step; stage 1 is nothing, tap or hold; a repeated
    ///   hold at stage 2 needs a hold at stage 1, at stage 3 a hold (or
    ///   repeated hold) at stage 2; stage 4 is nothing or a tap.
    /// * MT: threshold within the limits' range.
    pub fn validate(&self, limits: &AdvancedLimits) -> Result<(), WriteBuildError> {
        match self {
            AdvancedKeyDef::Dks(dks) => validate_dks(dks, limits.dks_bottom),
            AdvancedKeyDef::ModTap(mt) => {
                let (min, max) = limits.mt_threshold_range();
                if (min..=max).contains(&mt.threshold_10ms) {
                    Ok(())
                } else {
                    Err(WriteBuildError::MtThresholdOutOfRange {
                        value: mt.threshold_10ms,
                        min,
                        max,
                    })
                }
            }
            AdvancedKeyDef::Toggle(_) => Ok(()),
        }
    }

    /// What about this definition is not verified on hardware (empty when
    /// everything is CONFIRMED). Open question Q19.
    ///
    /// Hardware test H20 (2026-10-27) confirmed the DKS press point unit
    /// (0.1 mm), taps at stages 1-3, holds at stages 1-3, the Mod-Tap tap and
    /// hold paths with a 300 ms hold time, and the Toggle release. Still
    /// untested: a DKS tap at stage 4, the "hold again" step and other
    /// Mod-Tap hold times.
    pub fn evidence(&self) -> Vec<UnverifiedNote> {
        match self {
            AdvancedKeyDef::Dks(dks) => {
                let rows = || dks.rows.iter().flatten();
                let mut notes = Vec::new();
                if rows().any(|row| row.steps[3] == DksStep::Tap) {
                    notes.push(UnverifiedNote::new("DKS tap at stage 4", "Q19"));
                }
                if rows().any(|row| row.steps.contains(&DksStep::HoldAgain)) {
                    notes.push(UnverifiedNote::new("DKS \"hold again\" step", "Q19"));
                }
                notes
            }
            AdvancedKeyDef::ModTap(mt) if mt.threshold_10ms != MT_TESTED_THRESHOLD => {
                vec![UnverifiedNote::new(
                    "Mod-Tap hold time other than 300 ms",
                    "Q19",
                )]
            }
            AdvancedKeyDef::ModTap(_) | AdvancedKeyDef::Toggle(_) => Vec::new(),
        }
    }
}

impl fmt::Display for AdvancedKeyDef {
    /// E.g. `DKS at 1.5 mm: E tap/-/-/-, R -/tap/-/-`, `Mod-Tap G / hold
    /// L-Ctrl after 300 ms`, `Toggle H`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdvancedKeyDef::Dks(dks) => {
                write!(
                    f,
                    "DKS at {}.{} mm:",
                    dks.first_point / 10,
                    dks.first_point % 10
                )?;
                for (i, row) in dks.rows.iter().flatten().enumerate() {
                    let steps: Vec<&str> = row
                        .steps
                        .iter()
                        .map(|s| match s {
                            DksStep::None => "-",
                            DksStep::Tap => "tap",
                            DksStep::Hold => "hold",
                            DksStep::HoldAgain => "hold again",
                        })
                        .collect();
                    let sep = if i == 0 { " " } else { ", " };
                    write!(f, "{sep}{} {}", row.key, steps.join("/"))?;
                }
                Ok(())
            }
            AdvancedKeyDef::ModTap(mt) => write!(
                f,
                "Mod-Tap {} / hold {} after {} ms",
                mt.tap,
                mt.hold,
                u16::from(mt.threshold_10ms) * 10
            ),
            AdvancedKeyDef::Toggle(t) => write!(f, "Toggle {}", t.key),
        }
    }
}

fn validate_dks(dks: &Dks, bottom: u8) -> Result<(), WriteBuildError> {
    if dks.first_point < 1 || dks.first_point > bottom {
        return Err(WriteBuildError::DksPointOutOfRange {
            value: dks.first_point,
            max: bottom,
        });
    }
    let steps_error = |row, reason| WriteBuildError::DksSteps { row, reason };
    let mut used = 0usize;
    for (row, slot) in dks.rows.iter().enumerate() {
        let Some(DksRow { steps, .. }) = slot else {
            continue;
        };
        used += 1;
        let [s0, s1, s2, s3] = *steps;
        if steps.iter().all(|s| *s == DksStep::None) {
            return Err(steps_error(row, "a row with a key needs at least one step"));
        }
        if s0 == DksStep::HoldAgain {
            return Err(steps_error(row, "stage 1 cannot be a repeated hold"));
        }
        if s1 == DksStep::HoldAgain && s0 != DksStep::Hold {
            return Err(steps_error(
                row,
                "a repeated hold at stage 2 needs a hold at stage 1",
            ));
        }
        if s2 == DksStep::HoldAgain && !matches!(s1, DksStep::Hold | DksStep::HoldAgain) {
            return Err(steps_error(
                row,
                "a repeated hold at stage 3 needs a hold at stage 2",
            ));
        }
        if !matches!(s3, DksStep::None | DksStep::Tap) {
            return Err(steps_error(row, "stage 4 can only be nothing or a tap"));
        }
    }
    if used == 0 {
        return Err(steps_error(0, "no row has a key"));
    }
    Ok(())
}

/// The 40 slot bytes of `def` with deep press point `bottom` (0.1 mm).
fn encode(def: &AdvancedKeyDef, bottom: u8) -> [u8; ADVANCED_SLOT_LEN] {
    let mut b = [0u8; ADVANCED_SLOT_LEN];
    b[0] = def.type_byte();
    let action = |key: HidUsage| [0x00, key.get(), 0x00, 0x00];
    match def {
        AdvancedKeyDef::Dks(dks) => {
            b[1..5].copy_from_slice(&[dks.first_point, bottom, bottom, dks.first_point]);
            for (k, row) in dks.rows.iter().enumerate() {
                if let Some(row) = row {
                    let at = 5 + 8 * k;
                    b[at..at + 4].copy_from_slice(&action(row.key));
                    b[at + 4..at + 8].copy_from_slice(&row.steps.map(DksStep::byte));
                }
            }
        }
        AdvancedKeyDef::ModTap(mt) => {
            b[1..5].copy_from_slice(&action(mt.hold));
            b[5..9].copy_from_slice(&action(mt.tap));
            b[9] = mt.threshold_10ms;
        }
        AdvancedKeyDef::Toggle(t) => b[1..5].copy_from_slice(&action(t.key)),
    }
    b
}

/// The definition of a non-empty slot. Checks only the structure (type,
/// actions, step values); the caller validates and compares the re-encoding.
fn parse(b: &[u8; ADVANCED_SLOT_LEN]) -> Result<AdvancedKeyDef, WriteBuildError> {
    let unwritable = |reason| WriteBuildError::UnwritableAdvancedKey { reason };
    let action = |at: usize| match b.get(at..at + 4) {
        Some([0x00, usage, 0x00, 0x00]) => {
            HidUsage::new(*usage).ok_or(unwritable("an action is not an offered key"))
        }
        _ => Err(unwritable("an action is not a keyboard key")),
    };
    match b[0] {
        TYPE_DKS => {
            let mut rows = [None; 4];
            for (k, row) in rows.iter_mut().enumerate() {
                let at = 5 + 8 * k;
                let bytes = b.get(at..at + 8).unwrap_or_default();
                if bytes.iter().all(|x| *x == 0) {
                    continue;
                }
                let mut steps = [DksStep::None; 4];
                for (step, byte) in steps.iter_mut().zip(&b[at + 4..at + 8]) {
                    *step = DksStep::from_byte(*byte).ok_or(unwritable("a DKS step above 3"))?;
                }
                *row = Some(DksRow {
                    key: action(at)?,
                    steps,
                });
            }
            Ok(AdvancedKeyDef::Dks(Dks {
                first_point: b[1],
                rows,
            }))
        }
        TYPE_MT => Ok(AdvancedKeyDef::ModTap(ModTap {
            hold: action(1)?,
            tap: action(5)?,
            threshold_10ms: b[9],
        })),
        TYPE_TGL => Ok(AdvancedKeyDef::Toggle(Toggle { key: action(1)? })),
        _ => Err(unwritable("type other than DKS, MT or TGL")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::FromResponse;
    use crate::decode::test_util::single;
    use crate::model::W75;
    use crate::request::ReadRequest;

    fn usage(u: u8) -> HidUsage {
        HidUsage::new(u).unwrap()
    }

    fn w75_limits() -> AdvancedLimits {
        let global = TriggerGlobal::from_response(&single(
            ReadRequest::TriggerGlobal,
            0x21,
            0,
            &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
        ))
        .unwrap();
        AdvancedLimits::new(&W75, &global, 20).unwrap()
    }

    fn row(key: u8, steps: [u8; 4]) -> Option<DksRow> {
        Some(DksRow {
            key: usage(key),
            steps: steps.map(|s| DksStep::from_byte(s).unwrap()),
        })
    }

    /// c16: DKS on E, E tap at stage 1, R tap at stage 2, 1.5 mm.
    fn c16_dks() -> AdvancedKeyDef {
        AdvancedKeyDef::Dks(Dks {
            first_point: 15,
            rows: [row(0x08, [1, 0, 0, 0]), row(0x15, [0, 1, 0, 0]), None, None],
        })
    }

    #[test]
    fn limits_from_the_w75_travel() {
        let limits = w75_limits();
        assert_eq!(limits.dks_bottom(), 30);
        assert_eq!(limits.mt_threshold_range(), (15, 100));
        let global = TriggerGlobal::from_response(&single(
            ReadRequest::TriggerGlobal,
            0x21,
            0,
            &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
        ))
        .unwrap();
        assert_eq!(
            AdvancedLimits::new(&W75, &global, 10),
            Err(WriteBuildError::UnsupportedUnit { unit_um: 10 })
        );
        // A longer travel is capped by the ceiling; none at all is refused.
        let mut long = global.clone();
        long.max_travel = crate::Distance(250);
        assert_eq!(
            AdvancedLimits::new(&W75, &long, 20).map(|l| l.dks_bottom()),
            Ok(30)
        );
        let mut none = global.clone();
        none.max_travel = crate::Distance(10);
        assert_eq!(
            AdvancedLimits::new(&W75, &none, 20),
            Err(WriteBuildError::TriggerLimitsUnsupported)
        );
        let unknown = Model {
            product_code: "AP61HEARGB",
            ..W75
        };
        assert!(matches!(
            AdvancedLimits::new(&unknown, &global, 20),
            Err(WriteBuildError::UnknownModel { .. })
        ));
    }

    #[test]
    fn encodes_the_c16_slots() {
        let limits = w75_limits();
        let dks = AdvancedSlotData::from_def(&c16_dks(), &limits).unwrap();
        assert_eq!(
            dks.as_bytes()[..21],
            [
                0x01, 0x0f, 0x1e, 0x1e, 0x0f, 0x00, 0x08, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
                0x15, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00
            ]
        );
        assert!(dks.as_bytes()[21..].iter().all(|b| *b == 0));
        let mt = AdvancedKeyDef::ModTap(ModTap {
            hold: usage(0xE0),
            tap: usage(0x0A),
            threshold_10ms: 30,
        });
        let mt = AdvancedSlotData::from_def(&mt, &limits).unwrap();
        assert_eq!(
            mt.as_bytes()[..10],
            [0x02, 0x00, 0xe0, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x1e]
        );
        let tgl = AdvancedKeyDef::Toggle(Toggle { key: usage(0x0B) });
        let tgl = AdvancedSlotData::from_def(&tgl, &limits).unwrap();
        assert_eq!(tgl.as_bytes()[..5], [0x03, 0x00, 0x0b, 0x00, 0x00]);
        assert_eq!(tgl.type_byte(), 3);
        // Each re-reads as itself and decodes to its definition.
        for data in [&dks, &mt, &tgl] {
            assert_eq!(
                AdvancedSlotData::from_read_slot(data.as_bytes()).as_ref(),
                Ok(&Some(data.clone()))
            );
            let def = data.def().unwrap();
            assert_eq!(AdvancedSlotData::from_def(&def, &limits).as_ref(), Ok(data));
        }
        assert_eq!(
            c16_dks().to_string(),
            "DKS at 1.5 mm: E tap/-/-/-, R -/tap/-/-"
        );
        assert_eq!(
            mt.def().unwrap().to_string(),
            "Mod-Tap G / hold L-Ctrl after 300 ms"
        );
    }

    #[test]
    fn dks_step_grammar() {
        let limits = w75_limits();
        let dks = |first_point, rows| AdvancedKeyDef::Dks(Dks { first_point, rows });
        let ok = [
            [1, 0, 0, 0],
            [2, 3, 0, 0],
            [2, 2, 3, 0],
            [2, 3, 3, 1],
            [0, 2, 3, 0],
            [0, 0, 2, 1],
            [0, 0, 0, 1],
            [2, 2, 2, 0],
        ];
        for steps in ok {
            assert_eq!(
                dks(15, [row(4, steps), None, None, None]).validate(&limits),
                Ok(()),
                "{steps:?}"
            );
        }
        let bad: [([u8; 4], &str); 6] = [
            ([0, 0, 0, 0], "a row with a key needs at least one step"),
            ([3, 0, 0, 0], "stage 1 cannot be a repeated hold"),
            (
                [1, 3, 0, 0],
                "a repeated hold at stage 2 needs a hold at stage 1",
            ),
            (
                [0, 3, 0, 0],
                "a repeated hold at stage 2 needs a hold at stage 1",
            ),
            (
                [1, 1, 3, 0],
                "a repeated hold at stage 3 needs a hold at stage 2",
            ),
            ([1, 0, 0, 2], "stage 4 can only be nothing or a tap"),
        ];
        for (steps, reason) in bad {
            assert_eq!(
                dks(15, [None, row(4, steps), None, None]).validate(&limits),
                Err(WriteBuildError::DksSteps { row: 1, reason }),
                "{steps:?}"
            );
        }
        assert_eq!(
            dks(15, [None; 4]).validate(&limits),
            Err(WriteBuildError::DksSteps {
                row: 0,
                reason: "no row has a key"
            })
        );
        for point in [0, 31] {
            assert_eq!(
                dks(point, [row(4, [1, 0, 0, 0]), None, None, None]).validate(&limits),
                Err(WriteBuildError::DksPointOutOfRange {
                    value: point,
                    max: 30
                })
            );
        }
        for threshold in [14, 101] {
            let mt = AdvancedKeyDef::ModTap(ModTap {
                hold: usage(0xE0),
                tap: usage(0x04),
                threshold_10ms: threshold,
            });
            assert_eq!(
                AdvancedSlotData::from_def(&mt, &limits),
                Err(WriteBuildError::MtThresholdOutOfRange {
                    value: threshold,
                    min: 15,
                    max: 100
                })
            );
        }
    }

    #[test]
    fn read_slots_must_re_encode_exactly() {
        let limits = w75_limits();
        let base = *AdvancedSlotData::from_def(&c16_dks(), &limits)
            .unwrap()
            .as_bytes();
        assert_eq!(AdvancedSlotData::from_read_slot(&[0u8; 40]), Ok(None));
        assert!(matches!(
            AdvancedSlotData::from_read_slot(&[0u8; 39]),
            Err(WriteBuildError::TableLength { .. })
        ));
        // A shallower deep point read back is accepted as is.
        let mut shallow = base;
        shallow[2] = 20;
        shallow[3] = 20;
        assert!(matches!(
            AdvancedSlotData::from_read_slot(&shallow),
            Ok(Some(_))
        ));
        let unwritable = |reason| Err(WriteBuildError::UnwritableAdvancedKey { reason });
        let cases: [(usize, u8, Result<Option<AdvancedSlotData>, WriteBuildError>); 9] = [
            (0, 4, unwritable("type other than DKS, MT or TGL")),
            (0, 0, unwritable("type other than DKS, MT or TGL")),
            (
                3,
                0x1d,
                unwritable("bytes outside the DKS, MT or TGL layout"),
            ), // r3 != r2
            (
                4,
                0x0e,
                unwritable("bytes outside the DKS, MT or TGL layout"),
            ), // r4 != r1
            (
                39,
                0x01,
                unwritable("bytes outside the DKS, MT or TGL layout"),
            ),
            (5, 0x12, unwritable("an action is not a keyboard key")), // mouse action
            (6, 0x32, unwritable("an action is not an offered key")),
            (9, 0x04, unwritable("a DKS step above 3")),
            (
                2,
                31,
                Err(WriteBuildError::DksPointOutOfRange { value: 31, max: 30 }),
            ),
        ];
        for (offset, value, expected) in cases {
            let mut bytes = base;
            bytes[offset] = value;
            if offset == 2 {
                bytes[3] = value;
            }
            assert_eq!(
                AdvancedSlotData::from_read_slot(&bytes),
                expected,
                "offset {offset}"
            );
        }
        // r1 above the deep point.
        let mut deep = base;
        deep[1] = 0x1f;
        deep[4] = 0x1f;
        assert!(AdvancedSlotData::from_read_slot(&deep).is_err());
        // RS, RKRT, MDS are never written back.
        for kind in [4u8, 5, 6, 9] {
            let mut bytes = [0u8; 40];
            bytes[0] = kind;
            bytes[2] = 0x04;
            assert!(AdvancedSlotData::from_read_slot(&bytes).is_err(), "{kind}");
        }
        // TGL / MT with bytes after their layout.
        let mut tgl = [0u8; 40];
        tgl[..5].copy_from_slice(&[0x03, 0x00, 0x0b, 0x00, 0x00]);
        assert!(matches!(
            AdvancedSlotData::from_read_slot(&tgl),
            Ok(Some(_))
        ));
        tgl[5] = 0x01;
        assert!(AdvancedSlotData::from_read_slot(&tgl).is_err());
    }

    #[test]
    fn evidence_notes() {
        assert!(c16_dks().evidence().is_empty());
        // H20: taps at stages 1-3, holds at stages 1-3 and press points
        // other than 1.5 mm are confirmed.
        for (point, steps) in [(5, [1, 0, 0, 0]), (7, [2, 0, 0, 0]), (15, [2, 2, 2, 0])] {
            let dks = AdvancedKeyDef::Dks(Dks {
                first_point: point,
                rows: [row(4, steps), row(5, [0, 0, 1, 0]), None, None],
            });
            assert!(dks.evidence().is_empty(), "{dks}");
        }
        // Not tested: a tap at stage 4 and "hold again".
        let deep = AdvancedKeyDef::Dks(Dks {
            first_point: 5,
            rows: [row(4, [2, 3, 0, 1]), None, None, None],
        });
        let notes: Vec<&str> = deep.evidence().iter().map(|n| n.what).collect();
        assert_eq!(notes, ["DKS tap at stage 4", "DKS \"hold again\" step"]);
        // Mod-Tap: tested with a 300 ms hold time only.
        let mt = |threshold_10ms| {
            AdvancedKeyDef::ModTap(ModTap {
                hold: usage(0xE0),
                tap: usage(0x0A),
                threshold_10ms,
            })
        };
        assert!(mt(30).evidence().is_empty());
        let notes: Vec<&str> = mt(20).evidence().iter().map(|n| n.what).collect();
        assert_eq!(notes, ["Mod-Tap hold time other than 300 ms"]);
        let tgl = AdvancedKeyDef::Toggle(Toggle { key: usage(0x0B) });
        assert!(tgl.evidence().is_empty());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serializes() {
        let data = AdvancedSlotData::from_def(
            &AdvancedKeyDef::Toggle(Toggle { key: usage(0x0B) }),
            &w75_limits(),
        )
        .unwrap();
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.starts_with(r#""03000b0000"#), "{json}");
        assert_eq!(
            serde_json::to_string(&DksStep::HoldAgain).unwrap(),
            r#""hold_again""#
        );
    }
}
