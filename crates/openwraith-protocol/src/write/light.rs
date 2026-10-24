//! The main light effect record (`07`), its limits (`0A`) and the main-light
//! write (spec section 4.7).

use std::fmt;

use crate::decode::{LightMode, LightModeList, Rgb};
use crate::error::WriteBuildError;
use crate::hex;
use crate::model::{Model, known_model};
use crate::request::ReadRequest;
use crate::response::RawResponse;

/// Length of the light effect record.
pub const LIGHT_RECORD_LEN: usize = 12;

/// Largest direction value (spec section 4.7.4: 0..=5).
const MAX_DIRECTION: u8 = 5;

/// The 12-byte main light effect record: mode, brightness, speed,
/// foreground R G B, background R G B, direction, fullColor, power
/// (spec section 4.7.2).
///
/// The record is identical in the `07 01` reply payload and the `07 00`
/// write (CONFIRMED: the c02 write `0a 04 03 ff 00 00 00 00 00 00 01 00`
/// reads back unchanged in c03 and c05). Obtained from a reply with
/// [`from_response`](Self::from_response) and edited with the `with_*`
/// methods (read-modify-write).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct LightRecord([u8; LIGHT_RECORD_LEN]);

impl LightRecord {
    /// The record of a `07 01` (main light) reply. The reply must declare
    /// exactly 12 bytes.
    pub fn from_response(response: &RawResponse) -> Result<LightRecord, WriteBuildError> {
        if response.request() != ReadRequest::MainLight {
            return Err(WriteBuildError::WrongRequest {
                expected: "the main light record",
                got: response.request(),
            });
        }
        let payload = response.payload();
        let got = payload.len();
        <[u8; LIGHT_RECORD_LEN]>::try_from(payload.as_slice())
            .map(LightRecord)
            .map_err(|_| WriteBuildError::TableLength {
                what: "light record",
                expected: LIGHT_RECORD_LEN,
                got,
            })
    }

    /// The 12 record bytes.
    pub fn as_bytes(&self) -> &[u8; LIGHT_RECORD_LEN] {
        &self.0
    }

    /// Byte 0: effect mode (CONFIRMED).
    pub fn mode(&self) -> LightMode {
        LightMode::from_id(self.0[0])
    }

    /// Byte 1: brightness (CONFIRMED).
    pub fn brightness(&self) -> u8 {
        self.0[1]
    }

    /// Byte 2: speed (CODE-ONLY).
    pub fn speed(&self) -> u8 {
        self.0[2]
    }

    /// Bytes 3-5: foreground colour (CONFIRMED).
    pub fn foreground(&self) -> Rgb {
        Rgb::new(self.0[3], self.0[4], self.0[5])
    }

    /// Bytes 6-8: background colour (CODE-ONLY).
    pub fn background(&self) -> Rgb {
        Rgb::new(self.0[6], self.0[7], self.0[8])
    }

    /// Byte 9: direction (CODE-ONLY, 0..=5).
    pub fn direction(&self) -> u8 {
        self.0[9]
    }

    /// Byte 10: "Full Color" (CODE-ONLY, 0 or 1).
    pub fn full_color(&self) -> u8 {
        self.0[10]
    }

    /// Byte 11: power, 0 = lighting on, 1 = off (CODE-ONLY).
    pub fn power(&self) -> u8 {
        self.0[11]
    }

    /// The record with another mode; nothing else changes (use
    /// [`with_direction`](Self::with_direction) and
    /// [`default_direction`](Self::default_direction) to reset the direction
    /// as the vendor site does on a mode change).
    #[must_use]
    pub fn with_mode(mut self, mode: LightMode) -> Self {
        self.0[0] = mode.id();
        self
    }

    /// The record with another brightness.
    #[must_use]
    pub fn with_brightness(mut self, brightness: u8) -> Self {
        self.0[1] = brightness;
        self
    }

    /// The record with another speed.
    #[must_use]
    pub fn with_speed(mut self, speed: u8) -> Self {
        self.0[2] = speed;
        self
    }

    /// The record with another foreground colour.
    #[must_use]
    pub fn with_foreground(mut self, color: Rgb) -> Self {
        self.0[3..6].copy_from_slice(&[color.r, color.g, color.b]);
        self
    }

    /// The record with another direction.
    #[must_use]
    pub fn with_direction(mut self, direction: u8) -> Self {
        self.0[9] = direction;
        self
    }

    /// The direction the vendor site selects when `mode` is chosen: 4
    /// (spread) for Aurora, 2 (up) for Speed Respond, 0 otherwise (spec
    /// section 4.7.3, CODE-ONLY).
    pub fn default_direction(mode: LightMode) -> u8 {
        match mode {
            LightMode::Aurora => 4,
            LightMode::SpeedRespond => 2,
            _ => 0,
        }
    }

    /// Checks the record against the keyboard's limits: the mode is in the
    /// `0x0A` list or Custom (`0x0A`) and never music rhythm (`0x64`);
    /// brightness and speed are at most the reported maxima; direction is
    /// below 6; fullColor and power are 0 or 1.
    pub fn validate(&self, limits: &LightLimits) -> Result<(), WriteBuildError> {
        let mode = self.0[0];
        if !limits.supports_mode(mode) {
            return Err(WriteBuildError::UnsupportedLightMode { mode });
        }
        let checks = [
            ("brightness", self.brightness(), limits.max_brightness),
            ("speed", self.speed(), limits.max_speed),
            ("direction", self.direction(), MAX_DIRECTION),
            ("full colour", self.full_color(), 1),
            ("power", self.power(), 1),
        ];
        for (field, value, max) in checks {
            if value > max {
                return Err(WriteBuildError::LightValueOutOfRange { field, value, max });
            }
        }
        Ok(())
    }
}

impl fmt::Debug for LightRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LightRecord({})", hex::to_spaced_hex(&self.0))
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for LightRecord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::to_hex(&self.0))
    }
}

/// What the keyboard accepts in a main light record, from its `0x0A` mode
/// list (spec section 4.7.1).
///
/// Built only by [`LightLimits::new`], which intersects the keyboard's list
/// with the model's compiled-in [`WriteCeilings`](crate::WriteCeilings): the decoded
/// [`LightModeList`] has public fields (and a reply can be forged), so the
/// keyboard's values alone are never trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LightLimits {
    modes: Vec<u8>,
    custom: bool,
    max_speed: u8,
    max_brightness: u8,
}

impl LightLimits {
    /// The limits for `model` from its `0x0A` mode list, capped by the
    /// [`WriteCeilings`](crate::WriteCeilings) of the model in [`MODELS`](crate::MODELS) with the
    /// same product code: only modes in both the list and the ceilings (plus
    /// Custom when the ceilings allow it), and the smaller maxima. For an
    /// unknown model nothing may be written.
    pub fn new(model: &Model, list: &LightModeList) -> LightLimits {
        let Some(ceilings) = known_model(model).map(|m| m.write_ceilings) else {
            return LightLimits {
                modes: Vec::new(),
                custom: false,
                max_speed: 0,
                max_brightness: 0,
            };
        };
        let allowed =
            |id: u8| id != LightMode::MusicRhythm.id() && ceilings.light_modes.contains(&id);
        LightLimits {
            modes: list
                .modes
                .iter()
                .map(|m| m.id())
                .filter(|id| allowed(*id))
                .collect(),
            custom: allowed(LightMode::Custom.id()),
            max_speed: list.max_speed.min(ceilings.max_light_speed),
            max_brightness: list.max_brightness.min(ceilings.max_light_brightness),
        }
    }

    /// Mode IDs of the device list that the model's ceilings allow, in device
    /// order (Custom `0x0A` is not in the W75 list but is accepted, see
    /// [`supports_mode`](Self::supports_mode)).
    pub fn modes(&self) -> &[u8] {
        &self.modes
    }

    /// Largest speed value (CODE-ONLY: order of the two maxima).
    pub fn max_speed(&self) -> u8 {
        self.max_speed
    }

    /// Largest brightness value (CODE-ONLY: order of the two maxima).
    pub fn max_brightness(&self) -> u8 {
        self.max_brightness
    }

    /// Whether `mode` may be written: in the device list or Custom (`0x0A`,
    /// CONFIRMED accepted by the W75), allowed by the model's ceilings, and
    /// never music rhythm (`0x64`).
    pub fn supports_mode(&self, mode: u8) -> bool {
        mode != LightMode::MusicRhythm.id()
            && ((mode == LightMode::Custom.id() && self.custom) || self.modes.contains(&mode))
    }

    /// Every mode that may be written: the device list, plus Custom when the
    /// list does not contain it.
    pub fn writable_modes(&self) -> Vec<LightMode> {
        let mut modes: Vec<LightMode> = self
            .modes
            .iter()
            .copied()
            .filter(|m| self.supports_mode(*m))
            .map(LightMode::from_id)
            .collect();
        if self.custom && !modes.contains(&LightMode::Custom) {
            modes.push(LightMode::Custom);
        }
        modes
    }
}

/// A validated main-light write: `[17 00 00 00 02 00 00]` (music-rhythm
/// leave) when the mode changes, then `07 00 00 00 0E` + record + `00 00`.
///
/// Built only by [`MainLightWrite::new`]. The leave packet is never sent on
/// its own: it is emitted exactly when the target mode differs from the
/// current one, mirroring the vendor site (c02 #1087 / #1088; a brightness
/// change, #1119, sends `07 00` alone).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MainLightWrite {
    current_mode: u8,
    record: LightRecord,
}

impl MainLightWrite {
    /// A write of `target`, given the record the keyboard holds now
    /// (`current`, read just before). Fails if `target` does not pass
    /// [`LightRecord::validate`] against `limits`.
    pub fn new(
        current: &LightRecord,
        target: LightRecord,
        limits: &LightLimits,
    ) -> Result<MainLightWrite, WriteBuildError> {
        target.validate(limits)?;
        Ok(MainLightWrite {
            current_mode: current.0[0],
            record: target,
        })
    }

    /// The record that is written.
    pub fn record(&self) -> &LightRecord {
        &self.record
    }

    /// The mode the keyboard had when the write was built.
    pub fn current_mode(&self) -> u8 {
        self.current_mode
    }

    /// Whether the write changes the mode (and so starts with the
    /// music-rhythm leave packet).
    pub fn changes_mode(&self) -> bool {
        self.record.0[0] != self.current_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::FromResponse;
    use crate::decode::test_util::single;

    fn w75_limits() -> LightLimits {
        let payload = [
            0x02, 0x01, 0x03, 0x09, 0x04, 0x00, 0x0e, 0x0f, 0x06, 0x07, 0x0b, 0x0c, 0x10, 0x08,
            0xa5, 0x5a, 0x04, 0x04,
        ];
        let list =
            LightModeList::from_response(&single(ReadRequest::MainLightModes, 0x0A, 0, &payload))
                .unwrap();
        LightLimits::new(&crate::model::W75, &list)
    }

    fn record(bytes: [u8; 12]) -> LightRecord {
        LightRecord::from_response(&single(ReadRequest::MainLight, 0x07, 0x00, &bytes)).unwrap()
    }

    /// c01 main light: Wave, brightness 4, speed 3, white.
    const C01: [u8; 12] = [0x02, 0x04, 0x03, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0x01, 0x00];

    #[test]
    fn record_accessors_and_edits() {
        let r = record(C01);
        assert_eq!(r.mode(), LightMode::Wave);
        assert_eq!((r.brightness(), r.speed()), (4, 3));
        assert_eq!(r.foreground(), Rgb::new(0xff, 0xff, 0xff));
        assert!(r.background().is_black());
        assert_eq!((r.direction(), r.full_color(), r.power()), (0, 1, 0));
        let edited = r
            .with_mode(LightMode::Custom)
            .with_foreground(Rgb::new(0xff, 0, 0));
        assert_eq!(
            edited.as_bytes(),
            &[0x0a, 0x04, 0x03, 0xff, 0x00, 0x00, 0, 0, 0, 0, 0x01, 0x00]
        );
        let edited = edited.with_brightness(0).with_speed(1).with_direction(4);
        assert_eq!(&edited.as_bytes()[..3], &[0x0a, 0x00, 0x01]);
        assert_eq!(edited.direction(), 4);
        assert_eq!(LightRecord::default_direction(LightMode::Aurora), 4);
        assert_eq!(LightRecord::default_direction(LightMode::SpeedRespond), 2);
        assert_eq!(LightRecord::default_direction(LightMode::Wave), 0);
    }

    #[test]
    fn record_needs_exactly_twelve_bytes_of_the_main_light() {
        let short = single(ReadRequest::MainLight, 0x07, 0x00, &C01[..11]);
        assert!(matches!(
            LightRecord::from_response(&short),
            Err(WriteBuildError::TableLength { got: 11, .. })
        ));
        let side = single(ReadRequest::SideLight, 0x08, 0x00, &C01);
        assert!(matches!(
            LightRecord::from_response(&side),
            Err(WriteBuildError::WrongRequest { .. })
        ));
    }

    #[test]
    fn validation() {
        let limits = w75_limits();
        let r = record(C01);
        assert_eq!(r.validate(&limits), Ok(()));
        // Custom is accepted although it is not in the W75 list.
        assert!(!limits.modes().contains(&0x0A));
        assert_eq!(r.with_mode(LightMode::Custom).validate(&limits), Ok(()));
        assert!(limits.writable_modes().contains(&LightMode::Custom));
        assert_eq!(limits.writable_modes().len(), 15);
        for mode in [0x05u8, 0x0D, 0x11, 0x64, 0xFF] {
            assert_eq!(
                r.with_mode(LightMode::from_id(mode)).validate(&limits),
                Err(WriteBuildError::UnsupportedLightMode { mode }),
                "{mode:#04x}"
            );
        }
        let bad = [
            (r.with_brightness(5), "brightness"),
            (r.with_speed(5), "speed"),
            (r.with_direction(6), "direction"),
        ];
        for (record, field) in bad {
            assert!(
                matches!(
                    record.validate(&limits),
                    Err(WriteBuildError::LightValueOutOfRange { field: f, .. }) if f == field
                ),
                "{field}"
            );
        }
        let mut bytes = C01;
        bytes[10] = 2;
        assert!(record(bytes).validate(&limits).is_err());
        bytes = C01;
        bytes[11] = 2;
        assert!(record(bytes).validate(&limits).is_err());
        // Boundaries are accepted.
        assert_eq!(
            r.with_brightness(4)
                .with_speed(4)
                .with_direction(5)
                .validate(&limits),
            Ok(())
        );
    }

    #[test]
    fn forged_lists_are_capped_by_the_model() {
        // A list with an unknown mode, music rhythm and huge maxima.
        let payload = [0x02, 0x63, 0x64, 0x05, 0xa5, 0x5a, 0xff, 0xff];
        let mut list =
            LightModeList::from_response(&single(ReadRequest::MainLightModes, 0x0A, 0, &payload))
                .unwrap();
        list.modes.push(LightMode::Unknown(0x77));
        let limits = LightLimits::new(&crate::model::W75, &list);
        assert_eq!(limits.modes(), &[0x02]);
        assert_eq!((limits.max_speed(), limits.max_brightness()), (4, 4));
        let r = record(C01);
        for mode in [0x63u8, 0x64, 0x05, 0x77] {
            assert_eq!(
                r.with_mode(LightMode::from_id(mode)).validate(&limits),
                Err(WriteBuildError::UnsupportedLightMode { mode })
            );
        }
        assert!(MainLightWrite::new(&r, r.with_brightness(0xff), &limits).is_err());
        assert_eq!(r.with_mode(LightMode::Custom).validate(&limits), Ok(()));
        // An unknown model: nothing may be written.
        let unknown = Model {
            product_code: "AP61HEARGB",
            ..crate::model::W75
        };
        let none = LightLimits::new(&unknown, &list);
        assert!(none.writable_modes().is_empty());
        assert!(r.validate(&none).is_err());
    }

    #[test]
    fn music_rhythm_is_refused_even_if_listed() {
        let mut limits = w75_limits();
        limits.modes.push(0x64);
        assert!(!limits.supports_mode(0x64));
        assert!(!limits.writable_modes().contains(&LightMode::MusicRhythm));
    }

    #[test]
    fn main_light_write_knows_whether_the_mode_changes() {
        let limits = w75_limits();
        let current = record(C01);
        let custom = current
            .with_mode(LightMode::Custom)
            .with_foreground(Rgb::new(0xff, 0, 0));
        let write = MainLightWrite::new(&current, custom, &limits).unwrap();
        assert!(write.changes_mode());
        assert_eq!(write.current_mode(), 0x02);
        assert_eq!(write.record(), &custom);
        let dimmer = MainLightWrite::new(&custom, custom.with_brightness(0), &limits).unwrap();
        assert!(!dimmer.changes_mode());
        assert!(MainLightWrite::new(&current, current.with_speed(9), &limits).is_err());
    }
}
