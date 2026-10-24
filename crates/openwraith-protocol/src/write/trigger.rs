//! Trigger writes: actuation point and Rapid Trigger per key (`21`, byte 5 =
//! mode; spec section 4.11.3).
//!
//! The mode byte of a trigger write shares byte 5 with every other `0x21`
//! sub-function, including reset all triggers (`06`), calibration (`07`,
//! `08`, `0F`, `10`), set poll rate (`09`) and select profile (`12`). The
//! write path therefore uses its own closed [`TriggerWriteMode`], never the
//! decoder's [`TriggerMode`](crate::TriggerMode), whose `Unknown(b)` could
//! carry any of those values.

use crate::decode::TriggerGlobal;
use crate::error::WriteBuildError;
use crate::layout::{KeyMask, KeyPos};
use crate::model::{Model, known_model};
use crate::request::ReadRequest;
use crate::response::RawResponse;
use crate::units::Distance;

/// Declared length of a `21/05` reply.
const KEY_TRIGGER_RECORD_LEN: usize = 12;

/// The mode byte of a trigger write (byte 5). A closed set: there is no
/// conversion from an arbitrary byte.
///
/// ```
/// use openwraith_protocol::write::TriggerWriteMode;
/// assert_eq!(TriggerWriteMode::RapidTrigger.byte(), 0x0C);
/// ```
///
/// ```compile_fail
/// // 0x06 would be "reset all triggers"; no such conversion exists.
/// use openwraith_protocol::write::TriggerWriteMode;
/// let mode = TriggerWriteMode::from(0x06u8);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TriggerWriteMode {
    /// `0x00`: Rapid Trigger off (CONFIRMED, c03 / c10).
    Off,
    /// `0x0C`: Rapid Trigger, one sensitivity for press and release
    /// (CONFIRMED, c03).
    RapidTrigger,
    /// `0x0D`: Rapid Trigger with separate press and release sensitivity
    /// (CONFIRMED, c10).
    RapidTriggerSeparate,
}

impl TriggerWriteMode {
    /// The three modes.
    pub const ALL: [TriggerWriteMode; 3] = [
        TriggerWriteMode::Off,
        TriggerWriteMode::RapidTrigger,
        TriggerWriteMode::RapidTriggerSeparate,
    ];

    /// The mode byte on the wire.
    pub const fn byte(self) -> u8 {
        match self {
            TriggerWriteMode::Off => 0x00,
            TriggerWriteMode::RapidTrigger => 0x0C,
            TriggerWriteMode::RapidTriggerSeparate => 0x0D,
        }
    }

    /// Whether Rapid Trigger is on.
    pub const fn rapid_trigger(self) -> bool {
        !matches!(self, TriggerWriteMode::Off)
    }

    /// The mode for a byte read from a `21/05` reply, if it is one of the
    /// three. Private: values only ever come from a validated reply.
    const fn from_read(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(TriggerWriteMode::Off),
            0x0C => Some(TriggerWriteMode::RapidTrigger),
            0x0D => Some(TriggerWriteMode::RapidTriggerSeparate),
            _ => None,
        }
    }
}

/// Trigger settings of one key, as written (spec section 4.11.3).
///
/// The second travel value is always written equal to the actuation point
/// (write byte 29 = byte 28, byte 33 = byte 32), as the vendor site does.
/// Values are checked by [`TriggerWrite::new`] against [`TriggerLimits`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TriggerSetting {
    /// Mode (byte 5).
    pub mode: TriggerWriteMode,
    /// Actuation point (bytes 28/32, copied to 29/33).
    pub actuation: Distance,
    /// Rapid Trigger press sensitivity (bytes 30/34). Kept as read in mode
    /// [`Off`](TriggerWriteMode::Off).
    pub rt_press: Distance,
    /// Rapid Trigger release sensitivity (bytes 31/35). Equal to `rt_press`
    /// in mode [`RapidTrigger`](TriggerWriteMode::RapidTrigger).
    pub rt_release: Distance,
}

impl TriggerSetting {
    /// The key and settings of a `21/05` reply (12 bytes
    /// `05 MM A A2 P R Ah A2h Ph Rh row col`).
    ///
    /// Fails if the mode is not `00`/`0C`/`0D`, if the second travel value
    /// differs from the actuation point, or if mode `0C` has different press
    /// and release values: such a key cannot be written back exactly.
    pub fn from_response(
        response: &RawResponse,
    ) -> Result<(KeyPos, TriggerSetting), WriteBuildError> {
        let ReadRequest::KeyTrigger(pos) = response.request() else {
            return Err(WriteBuildError::WrongRequest {
                expected: "a key trigger setting",
                got: response.request(),
            });
        };
        let p = response.payload();
        let Ok(p) = <[u8; KEY_TRIGGER_RECORD_LEN]>::try_from(p.as_slice()) else {
            return Err(WriteBuildError::TableLength {
                what: "key trigger record",
                expected: KEY_TRIGGER_RECORD_LEN,
                got: p.len(),
            });
        };
        let mode = TriggerWriteMode::from_read(p[1])
            .ok_or(WriteBuildError::UnknownTriggerMode { mode: p[1] })?;
        let actuation = Distance::from_lo_hi(p[2], p[6]);
        let second = Distance::from_lo_hi(p[3], p[7]);
        if second != actuation {
            return Err(WriteBuildError::SecondTravelDiffers { actuation, second });
        }
        let setting = TriggerSetting {
            mode,
            actuation,
            rt_press: Distance::from_lo_hi(p[4], p[8]),
            rt_release: Distance::from_lo_hi(p[5], p[9]),
        };
        setting.check_same_sensitivity()?;
        Ok((pos, setting))
    }

    /// The nine bytes a `21/05` reply carries for this setting (payload bytes
    /// 1..10: `MM A A P R Ah Ah Ph Rh`). Read-back verification and restore
    /// planning compare these.
    pub fn read_bytes(&self) -> [u8; 9] {
        let [a, a2, p, r, ah, a2h, ph, rh] = self.write_values();
        [self.mode.byte(), a, a2, p, r, ah, a2h, ph, rh]
    }

    /// Write bytes 28..36: `A A P R Ah Ah Ph Rh` (low bytes, then high bytes).
    pub(super) fn write_values(&self) -> [u8; 8] {
        let [a_lo, a_hi] = self.actuation.units().to_le_bytes();
        let [p_lo, p_hi] = self.rt_press.units().to_le_bytes();
        let [r_lo, r_hi] = self.rt_release.units().to_le_bytes();
        [a_lo, a_lo, p_lo, r_lo, a_hi, a_hi, p_hi, r_hi]
    }

    /// Mode `0C` needs equal press and release sensitivities.
    fn check_same_sensitivity(&self) -> Result<(), WriteBuildError> {
        if self.mode == TriggerWriteMode::RapidTrigger && self.rt_press != self.rt_release {
            return Err(WriteBuildError::PressReleaseDiffer {
                press: self.rt_press,
                release: self.rt_release,
            });
        }
        Ok(())
    }
}

/// Value ranges for trigger writes, from the `21/04` parameters (spec
/// section 4.11.5): actuation `min_travel..=max_travel`, Rapid Trigger
/// sensitivity `1..=ceil(max_travel / 2)` (W75: actuation 5..=170 units =
/// 0.10..3.40 mm, sensitivity 1..=85). In mode `00` press and release are
/// carried over and may be `0..=ceil(max_travel / 2)`.
///
/// Built only by [`TriggerLimits::new`], which intersects the keyboard's
/// parameters with the model's compiled-in [`WriteCeilings`](crate::WriteCeilings): the decoded
/// [`TriggerGlobal`] has public fields (and a reply can be forged), so the
/// keyboard's values alone are never trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TriggerLimits {
    min_actuation: Distance,
    max_actuation: Distance,
    max_sensitivity: Distance,
}

impl TriggerLimits {
    /// The limits for `model` from its `21/04` parameters, capped by the
    /// [`WriteCeilings`](crate::WriteCeilings) of the model in [`MODELS`](crate::MODELS) with the
    /// same product code. For an unknown model the limits are unusable
    /// ([`is_supported`](Self::is_supported) is false).
    pub fn new(model: &Model, global: &TriggerGlobal) -> TriggerLimits {
        let Some(ceilings) = known_model(model).map(|m| m.write_ceilings) else {
            return TriggerLimits {
                min_actuation: Distance::from_units(0),
                max_actuation: Distance::from_units(0),
                max_sensitivity: Distance::from_units(0),
            };
        };
        let (min, max) = (global.min_travel.units(), global.max_travel.units());
        // A minimum or maximum of 0 means "unsupported" and stays 0.
        let capped_min = if min == 0 {
            0
        } else {
            min.max(ceilings.min_actuation)
        };
        TriggerLimits {
            min_actuation: Distance::from_units(capped_min),
            max_actuation: Distance::from_units(max.min(ceilings.max_actuation)),
            max_sensitivity: Distance::from_units(max.div_ceil(2).min(ceilings.max_sensitivity)),
        }
    }

    /// Smallest actuation point (`21/04` minimum travel).
    pub fn min_actuation(&self) -> Distance {
        self.min_actuation
    }

    /// Largest actuation point (`21/04` maximum travel).
    pub fn max_actuation(&self) -> Distance {
        self.max_actuation
    }

    /// Largest Rapid Trigger sensitivity (`ceil(max_travel / 2)`).
    pub fn max_sensitivity(&self) -> Distance {
        self.max_sensitivity
    }

    /// Whether the parameters describe a usable range. A minimum or maximum
    /// travel of 0 means trigger writes are not supported (the vendor site
    /// would substitute a unit-dependent guess; we refuse instead).
    pub fn is_supported(&self) -> bool {
        self.min_actuation.units() > 0
            && self.max_actuation.units() > 0
            && self.min_actuation <= self.max_actuation
    }

    /// Checks `setting` against the ranges.
    pub fn check(&self, setting: &TriggerSetting) -> Result<(), WriteBuildError> {
        if !self.is_supported() {
            return Err(WriteBuildError::TriggerLimitsUnsupported);
        }
        let in_range = |field: &'static str, value: Distance, min: Distance, max: Distance| {
            if value < min || value > max {
                Err(WriteBuildError::TriggerValueOutOfRange {
                    field,
                    value,
                    min,
                    max,
                })
            } else {
                Ok(())
            }
        };
        in_range(
            "actuation",
            setting.actuation,
            self.min_actuation,
            self.max_actuation,
        )?;
        let min_sensitivity = Distance::from_units(u16::from(setting.mode.rapid_trigger()));
        in_range(
            "press sensitivity",
            setting.rt_press,
            min_sensitivity,
            self.max_sensitivity,
        )?;
        in_range(
            "release sensitivity",
            setting.rt_release,
            min_sensitivity,
            self.max_sensitivity,
        )?;
        setting.check_same_sensitivity()
    }
}

/// How the keys of one [`TriggerWrite`] are packed into packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TriggerGrouping {
    /// One packet per key, the only form captured (c03, c10). The default.
    #[default]
    SingleKey,
    /// One packet with the union mask of all keys. CODE-ONLY (spec 4.11.5,
    /// never captured): experimental until a hardware test confirms it
    /// (hardware test H10, `docs/HARDWARE_TESTS.md`).
    SameSetting,
}

/// A validated trigger write: one setting for a set of keys.
///
/// Built only by [`TriggerWrite::new`], which checks the key set (non-empty,
/// only keys a user can select: the knob, positions outside the layout and
/// mask bits 6-7 are refused; [`KeyMask`]'s field is public, so a mask is
/// never trusted) and the values against [`TriggerLimits`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TriggerWrite {
    keys: KeyMask,
    setting: TriggerSetting,
    grouping: TriggerGrouping,
}

impl TriggerWrite {
    /// A write of `setting` to every key in `keys`, packed one key per packet
    /// ([`TriggerGrouping::SingleKey`]).
    ///
    /// The selectable keys come from the model in [`MODELS`](crate::MODELS)
    /// with `model`'s product code, never from `model` itself (its fields are
    /// public, so a copy without a knob could be fabricated); an unknown
    /// model is refused with [`WriteBuildError::UnknownModel`].
    pub fn new(
        model: &Model,
        keys: KeyMask,
        setting: TriggerSetting,
        limits: &TriggerLimits,
    ) -> Result<TriggerWrite, WriteBuildError> {
        let model = known_model(model).ok_or(WriteBuildError::UnknownModel {
            product: model.product_code,
        })?;
        if keys.is_empty() {
            return Err(WriteBuildError::EmptyKeySet);
        }
        let selectable = model.selectable_mask();
        for (col, (byte, allowed)) in (0u8..).zip(keys.0.iter().zip(selectable.0.iter())) {
            let extra = byte & !allowed;
            if extra != 0 {
                let row = u8::try_from(extra.trailing_zeros()).unwrap_or(u8::MAX);
                return Err(WriteBuildError::KeyNotSelectable { row, col });
            }
        }
        limits.check(&setting)?;
        Ok(TriggerWrite {
            keys,
            setting,
            grouping: TriggerGrouping::SingleKey,
        })
    }

    /// The same write with another packet grouping.
    /// [`TriggerGrouping::SameSetting`] is experimental (CODE-ONLY).
    #[must_use]
    pub fn with_grouping(mut self, grouping: TriggerGrouping) -> Self {
        self.grouping = grouping;
        self
    }

    /// The keys written.
    pub fn keys(&self) -> KeyMask {
        self.keys
    }

    /// The keys written, in index order.
    pub fn positions(&self) -> Vec<KeyPos> {
        self.keys.positions()
    }

    /// The setting written to every key.
    pub fn setting(&self) -> &TriggerSetting {
        &self.setting
    }

    /// How keys are packed into packets.
    pub fn grouping(&self) -> TriggerGrouping {
        self.grouping
    }

    /// The key mask of every packet, in order.
    pub(super) fn packet_masks(&self) -> Vec<KeyMask> {
        match self.grouping {
            TriggerGrouping::SingleKey => self
                .positions()
                .into_iter()
                .map(|pos| KeyMask::from_positions([pos]))
                .collect(),
            TriggerGrouping::SameSetting => vec![self.keys],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::FromResponse;
    use crate::decode::test_util::{report, single};
    use crate::model::W75;

    fn w75_limits() -> TriggerLimits {
        let global = TriggerGlobal::from_response(&single(
            ReadRequest::TriggerGlobal,
            0x21,
            0,
            &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
        ))
        .unwrap();
        TriggerLimits::new(&W75, &global)
    }

    fn key_reply(pos: KeyPos, record: [u8; 9]) -> RawResponse {
        let mut prefix = vec![0x21, 0, 0, 0, 0x0c, 0x05];
        prefix.extend_from_slice(&record);
        prefix.extend_from_slice(&[pos.row(), pos.col()]);
        RawResponse::from_reports(ReadRequest::KeyTrigger(pos), [report(&prefix)]).unwrap()
    }

    fn setting(mode: TriggerWriteMode, a: u16, p: u16, r: u16) -> TriggerSetting {
        TriggerSetting {
            mode,
            actuation: Distance(a),
            rt_press: Distance(p),
            rt_release: Distance(r),
        }
    }

    fn pos(row: u8, col: u8) -> KeyPos {
        KeyPos::new(row, col).unwrap()
    }

    #[test]
    fn w75_limits_from_21_04() {
        let limits = w75_limits();
        assert_eq!(limits.min_actuation(), Distance(5));
        assert_eq!(limits.max_actuation(), Distance(170));
        assert_eq!(limits.max_sensitivity(), Distance(85));
        assert!(limits.is_supported());
    }

    #[test]
    fn settings_from_reads() {
        // c10 after the edits: W in mode 0D, press 0.30, release 0.50.
        let (key, w) = TriggerSetting::from_response(&key_reply(
            pos(2, 2),
            [0x0d, 0x55, 0x55, 0x0f, 0x19, 0, 0, 0, 0],
        ))
        .unwrap();
        assert_eq!(key, pos(2, 2));
        assert_eq!(
            w,
            setting(TriggerWriteMode::RapidTriggerSeparate, 0x55, 0x0f, 0x19)
        );
        assert_eq!(w.read_bytes(), [0x0d, 0x55, 0x55, 0x0f, 0x19, 0, 0, 0, 0]);
        assert_eq!(w.write_values(), [0x55, 0x55, 0x0f, 0x19, 0, 0, 0, 0]);
        // Factory key: mode 00 with sensitivity 0.02 / 0.02 carried over.
        let (_, factory) = TriggerSetting::from_response(&key_reply(
            pos(0, 0),
            [0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0],
        ))
        .unwrap();
        assert_eq!(factory, setting(TriggerWriteMode::Off, 0x55, 1, 1));
        // High bytes are kept.
        let (_, high) = TriggerSetting::from_response(&key_reply(
            pos(1, 1),
            [0x00, 0x10, 0x10, 0x02, 0x03, 0x01, 0x01, 0x00, 0x00],
        ))
        .unwrap();
        assert_eq!(high.actuation, Distance(0x0110));
        assert_eq!(high.read_bytes(), [0, 0x10, 0x10, 2, 3, 1, 1, 0, 0]);
    }

    #[test]
    fn unwritable_reads_are_refused() {
        let w = pos(2, 2);
        let cases = [
            [0x06, 0x55, 0x55, 1, 1, 0, 0, 0, 0],
            [0x0f, 0x55, 0x55, 1, 1, 0, 0, 0, 0],
            [0x00, 0x55, 0x54, 1, 1, 0, 0, 0, 0],
            [0x00, 0x55, 0x55, 1, 1, 0, 1, 0, 0],
            [0x0c, 0x55, 0x55, 1, 2, 0, 0, 0, 0],
            [0x0c, 0x55, 0x55, 1, 1, 0, 0, 1, 0],
        ];
        let expected = [
            "UnknownTriggerMode",
            "UnknownTriggerMode",
            "SecondTravelDiffers",
            "SecondTravelDiffers",
            "PressReleaseDiffer",
            "PressReleaseDiffer",
        ];
        for (record, name) in cases.into_iter().zip(expected) {
            let err = TriggerSetting::from_response(&key_reply(w, record)).unwrap_err();
            assert!(
                format!("{err:?}").starts_with(name),
                "{record:02x?}: {err:?}"
            );
        }
        let global = single(
            ReadRequest::TriggerGlobal,
            0x21,
            0,
            &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
        );
        assert!(matches!(
            TriggerSetting::from_response(&global),
            Err(WriteBuildError::WrongRequest { .. })
        ));
    }

    #[test]
    fn key_sets_are_validated() {
        let limits = w75_limits();
        let ok = setting(TriggerWriteMode::Off, 0x4b, 1, 1);
        let write =
            TriggerWrite::new(&W75, KeyMask::from_positions([pos(2, 1)]), ok, &limits).unwrap();
        assert_eq!(write.positions(), vec![pos(2, 1)]);
        assert_eq!(write.grouping(), TriggerGrouping::SingleKey);
        assert_eq!(
            TriggerWrite::new(&W75, KeyMask::default(), ok, &limits),
            Err(WriteBuildError::EmptyKeySet)
        );
        // The knob (index 15 = r0c15).
        assert_eq!(
            TriggerWrite::new(&W75, KeyMask::from_positions([pos(0, 15)]), ok, &limits),
            Err(WriteBuildError::KeyNotSelectable { row: 0, col: 15 })
        );
        // A position outside the W75 layout (r0c1 has no key).
        assert_eq!(
            TriggerWrite::new(&W75, KeyMask::from_positions([pos(0, 1)]), ok, &limits),
            Err(WriteBuildError::KeyNotSelectable { row: 0, col: 1 })
        );
        // Bits 6-7 and columns 16-21.
        let mut mask = KeyMask::from_positions([pos(2, 2)]);
        mask.0[2] |= 0x40;
        assert_eq!(
            TriggerWrite::new(&W75, mask, ok, &limits),
            Err(WriteBuildError::KeyNotSelectable { row: 6, col: 2 })
        );
        let mut mask = KeyMask::default();
        mask.0[21] = 0x01;
        assert_eq!(
            TriggerWrite::new(&W75, mask, ok, &limits),
            Err(WriteBuildError::KeyNotSelectable { row: 0, col: 21 })
        );
        // Select All (81 keys) is accepted.
        let all = TriggerWrite::new(&W75, W75.selectable_mask(), ok, &limits).unwrap();
        assert_eq!(all.packet_masks().len(), 81);
        assert!(all.packet_masks().iter().all(|m| m.positions().len() == 1));
        let grouped = all.with_grouping(TriggerGrouping::SameSetting);
        assert_eq!(grouped.packet_masks(), vec![W75.selectable_mask()]);
    }

    #[test]
    fn values_are_validated() {
        let limits = w75_limits();
        let w = KeyMask::from_positions([pos(2, 2)]);
        let check = |s: TriggerSetting| TriggerWrite::new(&W75, w, s, &limits).map(|_| ());
        use TriggerWriteMode::{Off, RapidTrigger, RapidTriggerSeparate};
        // Boundaries.
        for s in [
            setting(Off, 5, 0, 0),
            setting(Off, 170, 85, 85),
            setting(RapidTrigger, 5, 1, 1),
            setting(RapidTrigger, 170, 85, 85),
            setting(RapidTriggerSeparate, 50, 1, 85),
        ] {
            assert_eq!(check(s), Ok(()), "{s:?}");
        }
        for (s, field) in [
            (setting(Off, 4, 1, 1), "actuation"),
            (setting(Off, 171, 1, 1), "actuation"),
            (setting(Off, 50, 86, 1), "press sensitivity"),
            (setting(Off, 50, 1, 86), "release sensitivity"),
            (setting(RapidTrigger, 50, 0, 0), "press sensitivity"),
            (
                setting(RapidTriggerSeparate, 50, 1, 0),
                "release sensitivity",
            ),
            (
                setting(RapidTriggerSeparate, 50, 86, 1),
                "press sensitivity",
            ),
        ] {
            assert!(
                matches!(
                    check(s),
                    Err(WriteBuildError::TriggerValueOutOfRange { field: f, .. }) if f == field
                ),
                "{s:?}"
            );
        }
        assert!(matches!(
            check(setting(RapidTrigger, 50, 10, 11)),
            Err(WriteBuildError::PressReleaseDiffer { .. })
        ));
        // Limits without a minimum travel are unusable.
        let mut global = TriggerGlobal::from_response(&single(
            ReadRequest::TriggerGlobal,
            0x21,
            0,
            &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x00],
        ))
        .unwrap();
        let no_min = TriggerLimits::new(&W75, &global);
        assert!(!no_min.is_supported());
        assert_eq!(
            TriggerWrite::new(&W75, w, setting(Off, 50, 1, 1), &no_min),
            Err(WriteBuildError::TriggerLimitsUnsupported)
        );
        // Limits are capped by the W75's ceilings (170 / 85), even when
        // the keyboard (or a fabricated reply) reports more.
        global.max_travel = Distance(171);
        global.min_travel = Distance(5);
        assert_eq!(
            TriggerLimits::new(&W75, &global).max_sensitivity(),
            Distance(85)
        );
        global.max_travel = Distance(0xFFFF);
        global.min_travel = Distance(1);
        let forged = TriggerLimits::new(&W75, &global);
        assert_eq!(forged.min_actuation(), Distance(5));
        assert_eq!(forged.max_actuation(), Distance(170));
        assert_eq!(forged.max_sensitivity(), Distance(85));
        assert!(matches!(
            TriggerWrite::new(
                &W75,
                w,
                setting(RapidTrigger, 0xFF00, 0x7000, 0x7000),
                &forged
            ),
            Err(WriteBuildError::TriggerValueOutOfRange { .. })
        ));
        // A smaller device range is kept.
        global.max_travel = Distance(100);
        global.min_travel = Distance(10);
        let smaller = TriggerLimits::new(&W75, &global);
        assert_eq!(
            (
                smaller.min_actuation(),
                smaller.max_actuation(),
                smaller.max_sensitivity()
            ),
            (Distance(10), Distance(100), Distance(50))
        );
        // An unknown model gives unusable limits.
        let unknown = Model {
            product_code: "AP61HEARGB",
            ..W75
        };
        assert!(!TriggerLimits::new(&unknown, &global).is_supported());
    }

    #[test]
    fn a_fabricated_model_cannot_select_the_knob() {
        let limits = w75_limits();
        let ok = setting(TriggerWriteMode::Off, 0x55, 1, 1);
        // A copy of the W75 without a knob, and one with an extra key at
        // r0c1 (no key there).
        let no_knob = Model {
            knob_index: None,
            ..W75
        };
        assert_eq!(
            TriggerWrite::new(&no_knob, KeyMask::from_positions([pos(0, 15)]), ok, &limits),
            Err(WriteBuildError::KeyNotSelectable { row: 0, col: 15 })
        );
        static EXTRA: [crate::layout::Key; 1] = [crate::layout::Key {
            index: 1,
            row: 0,
            col: 1,
            name: "Extra",
            hid_usage: None,
        }];
        let extra = Model {
            keys: &EXTRA,
            ..W75
        };
        assert_eq!(
            TriggerWrite::new(&extra, KeyMask::from_positions([pos(0, 1)]), ok, &limits),
            Err(WriteBuildError::KeyNotSelectable { row: 0, col: 1 })
        );
        let unknown = Model {
            product_code: "AP61HEARGB",
            knob_index: None,
            ..W75
        };
        assert_eq!(
            TriggerWrite::new(&unknown, KeyMask::from_positions([pos(2, 2)]), ok, &limits),
            Err(WriteBuildError::UnknownModel {
                product: "AP61HEARGB"
            })
        );
    }

    #[test]
    fn modes_are_closed() {
        assert_eq!(
            TriggerWriteMode::ALL.map(TriggerWriteMode::byte),
            [0x00, 0x0C, 0x0D]
        );
        assert!(!TriggerWriteMode::Off.rapid_trigger());
        assert!(TriggerWriteMode::RapidTriggerSeparate.rapid_trigger());
        for byte in 0..=u8::MAX {
            let mode = TriggerWriteMode::from_read(byte);
            assert_eq!(
                mode.is_some(),
                matches!(byte, 0x00 | 0x0C | 0x0D),
                "{byte:#04x}"
            );
            assert!(mode.is_none_or(|m| m.byte() == byte));
        }
    }
}
