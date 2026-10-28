//! Lighting: mode lists `0x0A`/`0x02`, effect records `0x07`/`0x08`, the
//! active custom table `09 20` and the per-key colour tables `09 80/81`
//! (spec section 4.7).

use std::fmt;

use super::{FromResponse, at, expect_request, need, wrong_request};
use crate::error::DecodeError;
use crate::hex::HexBytes;
use crate::layout::MATRIX_SIZE;
use crate::request::{CustomTable, ReadRequest};
use crate::response::RawResponse;

/// An RGB colour. Serializes as `"#rrggbb"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// Creates a colour.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    /// Whether all channels are 0 (an unlit slot).
    pub const fn is_black(self) -> bool {
        self.r == 0 && self.g == 0 && self.b == 0
    }

    /// Parses `"#rrggbb"` or `"rrggbb"`.
    pub fn from_hex(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#').unwrap_or(text);
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let value = u32::from_str_radix(hex, 16).ok()?;
        let [_, r, g, b] = value.to_be_bytes();
        Some(Rgb { r, g, b })
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Rgb {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Rgb::from_hex(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid colour {text:?}")))
    }
}

/// Main-light effect mode (spec section 4.7.3; names CODE-ONLY).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LightMode {
    /// `0x00`
    Static,
    /// `0x01`
    Breath,
    /// `0x02`
    Wave,
    /// `0x03`
    Neon,
    /// `0x04`
    Radar,
    /// `0x06`
    Reactive,
    /// `0x07`
    Aurora,
    /// `0x08`
    Ripple,
    /// `0x09`
    Twinkle,
    /// `0x0A`: per-key custom colour table (not in the W75 list, but accepted
    /// and read back; CONFIRMED).
    Custom,
    /// `0x0B`
    Cross,
    /// `0x0C`
    SpeedRespond,
    /// `0x0E`
    AutoRipple,
    /// `0x0F`
    Striation,
    /// `0x10`
    Fireworks,
    /// `0x64`: music rhythm (not supported by the W75).
    MusicRhythm,
    /// Any other mode ID.
    Unknown(u8),
}

impl LightMode {
    /// Decodes a mode ID.
    pub const fn from_id(id: u8) -> Self {
        match id {
            0x00 => LightMode::Static,
            0x01 => LightMode::Breath,
            0x02 => LightMode::Wave,
            0x03 => LightMode::Neon,
            0x04 => LightMode::Radar,
            0x06 => LightMode::Reactive,
            0x07 => LightMode::Aurora,
            0x08 => LightMode::Ripple,
            0x09 => LightMode::Twinkle,
            0x0A => LightMode::Custom,
            0x0B => LightMode::Cross,
            0x0C => LightMode::SpeedRespond,
            0x0E => LightMode::AutoRipple,
            0x0F => LightMode::Striation,
            0x10 => LightMode::Fireworks,
            0x64 => LightMode::MusicRhythm,
            other => LightMode::Unknown(other),
        }
    }

    /// The mode ID on the wire.
    pub const fn id(self) -> u8 {
        match self {
            LightMode::Static => 0x00,
            LightMode::Breath => 0x01,
            LightMode::Wave => 0x02,
            LightMode::Neon => 0x03,
            LightMode::Radar => 0x04,
            LightMode::Reactive => 0x06,
            LightMode::Aurora => 0x07,
            LightMode::Ripple => 0x08,
            LightMode::Twinkle => 0x09,
            LightMode::Custom => 0x0A,
            LightMode::Cross => 0x0B,
            LightMode::SpeedRespond => 0x0C,
            LightMode::AutoRipple => 0x0E,
            LightMode::Striation => 0x0F,
            LightMode::Fireworks => 0x10,
            LightMode::MusicRhythm => 0x64,
            LightMode::Unknown(id) => id,
        }
    }

    /// English display name.
    pub fn name(self) -> String {
        let name = match self {
            LightMode::Static => "Static",
            LightMode::Breath => "Breath",
            LightMode::Wave => "Wave",
            LightMode::Neon => "Neon",
            LightMode::Radar => "Radar",
            LightMode::Reactive => "Reactive",
            LightMode::Aurora => "Aurora",
            LightMode::Ripple => "Ripple",
            LightMode::Twinkle => "Twinkle",
            LightMode::Custom => "Custom (per-key)",
            LightMode::Cross => "Cross",
            LightMode::SpeedRespond => "Speed Respond",
            LightMode::AutoRipple => "Auto Ripple",
            LightMode::Striation => "Striation",
            LightMode::Fireworks => "Fireworks",
            LightMode::MusicRhythm => "Musical Rhythm",
            LightMode::Unknown(id) => return format!("Unknown mode {id:#04x}"),
        };
        name.to_string()
    }
}

impl fmt::Display for LightMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

/// Effect direction (record byte 14; CODE-ONLY, only 0 observed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LightDirection {
    /// 0: right (Radar: clockwise).
    Right,
    /// 1: left (Radar: anticlockwise).
    Left,
    /// 2: up.
    Up,
    /// 3: down.
    Down,
    /// 4: spread.
    Spread,
    /// 5: gather.
    Gather,
    /// Any other value (the vendor site reads values >= 6 as 0).
    Unknown(u8),
}

impl LightDirection {
    /// Decodes the direction byte.
    pub const fn from_byte(b: u8) -> Self {
        match b {
            0 => LightDirection::Right,
            1 => LightDirection::Left,
            2 => LightDirection::Up,
            3 => LightDirection::Down,
            4 => LightDirection::Spread,
            5 => LightDirection::Gather,
            other => LightDirection::Unknown(other),
        }
    }
}

/// Supported light modes (`0x0A` main, `0x02` side; CONFIRMED layout).
///
/// Payload: mode IDs, then `A5 5A`, then max speed and max brightness (the
/// order of the last two is CODE-ONLY; both are 4 on the W75).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LightModeList {
    /// Supported modes in device order. Empty for the W75 side light.
    pub modes: Vec<LightMode>,
    /// The two bytes before the maxima (`A5 5A` on the W75; GUESS: marker).
    pub marker: HexBytes,
    /// Maximum speed value (CODE-ONLY).
    pub max_speed: u8,
    /// Maximum brightness value (CODE-ONLY).
    pub max_brightness: u8,
    /// The declared payload.
    pub raw: HexBytes,
}

impl FromResponse for LightModeList {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "LightModeList", |r| {
            matches!(r, ReadRequest::MainLightModes | ReadRequest::SideLightModes)
        })?;
        let p = response.payload();
        need(&p, "light mode list", 4)?;
        let n = p.len();
        let modes = p
            .get(..n - 4)
            .unwrap_or_default()
            .iter()
            .map(|id| LightMode::from_id(*id))
            .collect();
        Ok(LightModeList {
            modes,
            marker: HexBytes(p.get(n - 4..n - 2).unwrap_or_default().to_vec()),
            max_speed: at(&p, n - 2),
            max_brightness: at(&p, n - 1),
            raw: HexBytes(p),
        })
    }
}

/// A light effect record (`07 01` main / `08 01` side reply, 12 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LightEffect {
    /// Byte 5: effect mode (CONFIRMED).
    pub mode: LightMode,
    /// Byte 6: brightness, 0..max (CONFIRMED).
    pub brightness: u8,
    /// Byte 7: speed, 0..max (CODE-ONLY).
    pub speed: u8,
    /// Bytes 8-10: foreground colour (CONFIRMED).
    pub foreground: Rgb,
    /// Bytes 11-13: background colour (CODE-ONLY).
    pub background: Rgb,
    /// Byte 14: direction (CODE-ONLY).
    pub direction: LightDirection,
    /// Byte 15: "Full Color" checkbox (CODE-ONLY; meaning GUESS).
    pub full_color: bool,
    /// Byte 16 = 0: lighting on; 1 = off (CODE-ONLY).
    pub power_on: bool,
    /// The 12-byte record.
    pub raw: HexBytes,
}

impl FromResponse for LightEffect {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "LightEffect", |r| {
            matches!(r, ReadRequest::MainLight | ReadRequest::SideLight)
        })?;
        let p = response.payload();
        need(&p, "light effect record", 12)?;
        Ok(LightEffect {
            mode: LightMode::from_id(at(&p, 0)),
            brightness: at(&p, 1),
            speed: at(&p, 2),
            foreground: Rgb::new(at(&p, 3), at(&p, 4), at(&p, 5)),
            background: Rgb::new(at(&p, 6), at(&p, 7), at(&p, 8)),
            direction: LightDirection::from_byte(at(&p, 9)),
            full_color: at(&p, 10) != 0,
            power_on: at(&p, 11) == 0,
            raw: HexBytes(p),
        })
    }
}

/// Active custom colour table (`09 20` reply `09 SS 00 00 00`; bytes
/// CONFIRMED, meaning CODE-ONLY). The reply is byte-identical to a colour
/// write ack.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActiveCustomTable {
    /// Byte 1 as sent (0 = Custom 1, 1 = Custom 2).
    pub slot: u8,
    /// The table, if `slot` is 0 or 1.
    pub table: Option<CustomTable>,
}

impl FromResponse for ActiveCustomTable {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "ActiveCustomTable", |r| {
            r == ReadRequest::ActiveCustomTable
        })?;
        let slot = response.first().map_or(0, |r| r.sub());
        Ok(ActiveCustomTable {
            slot,
            table: CustomTable::from_number(slot),
        })
    }
}

/// A per-key custom colour table (`09 80` / `09 81`; 132 x RGB, CONFIRMED).
///
/// Slot = key index. Slots outside the key layout can hold colours too (the
/// W75 factory table lit slots 103 and 126), so they must be preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ColorTable {
    /// Which table this is.
    pub table: CustomTable,
    /// 132 colours, indexed by key index.
    pub colors: Vec<Rgb>,
    /// The 396 table bytes.
    pub raw: HexBytes,
}

impl ColorTable {
    /// Colour of slot `index`.
    pub fn color(&self, index: u8) -> Option<Rgb> {
        self.colors.get(usize::from(index)).copied()
    }

    /// Indices of every slot that is not black.
    pub fn lit_indices(&self) -> Vec<u8> {
        (0u8..)
            .zip(&self.colors)
            .filter(|(_, c)| !c.is_black())
            .map(|(i, _)| i)
            .collect()
    }
}

impl FromResponse for ColorTable {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        let ReadRequest::CustomColors(table) = response.request() else {
            return Err(wrong_request(response, "ColorTable"));
        };
        let bytes = response.table_bytes("colour table")?;
        need(&bytes, "colour table", MATRIX_SIZE * 3)?;
        let colors = bytes
            .as_chunks::<3>()
            .0
            .iter()
            .take(MATRIX_SIZE)
            .map(|c| Rgb::new(at(c, 0), at(c, 1), at(c, 2)))
            .collect();
        Ok(ColorTable {
            table,
            colors,
            raw: HexBytes(bytes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::{single, table};

    #[test]
    fn w75_mode_list() {
        let payload = [
            0x02, 0x01, 0x03, 0x09, 0x04, 0x00, 0x0e, 0x0f, 0x06, 0x07, 0x0b, 0x0c, 0x10, 0x08,
            0xa5, 0x5a, 0x04, 0x04,
        ];
        let list =
            LightModeList::from_response(&single(ReadRequest::MainLightModes, 0x0A, 0, &payload))
                .unwrap();
        assert_eq!(list.modes.len(), 14);
        assert_eq!(list.modes[0], LightMode::Wave);
        assert!(!list.modes.contains(&LightMode::Custom));
        assert_eq!(list.marker.0, [0xa5, 0x5a]);
        assert_eq!((list.max_speed, list.max_brightness), (4, 4));

        let side = LightModeList::from_response(&single(
            ReadRequest::SideLightModes,
            0x02,
            0,
            &[0xa5, 0x5a, 0x04, 0x04],
        ))
        .unwrap();
        assert!(side.modes.is_empty());
    }

    #[test]
    fn effect_records() {
        // c01 main light: Wave, brightness 4, speed 3, white.
        let main = LightEffect::from_response(&single(
            ReadRequest::MainLight,
            0x07,
            0x00,
            &[0x02, 0x04, 0x03, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0x01, 0x00],
        ))
        .unwrap();
        assert_eq!(main.mode, LightMode::Wave);
        assert_eq!(main.brightness, 4);
        assert_eq!(main.speed, 3);
        assert_eq!(main.foreground, Rgb::new(255, 255, 255));
        assert!(main.background.is_black());
        assert_eq!(main.direction, LightDirection::Right);
        assert!(main.full_color);
        assert!(main.power_on);

        // c05 main light: Custom, red.
        let custom = LightEffect::from_response(&single(
            ReadRequest::MainLight,
            0x07,
            0x00,
            &[0x0a, 0x04, 0x03, 0xff, 0x00, 0x00, 0, 0, 0, 0, 0x01, 0x00],
        ))
        .unwrap();
        assert_eq!(custom.mode, LightMode::Custom);
        assert_eq!(custom.foreground.to_string(), "#ff0000");
    }

    #[test]
    fn effect_record_too_short() {
        let raw = single(ReadRequest::SideLight, 0x08, 0x00, &[0x02, 0x04]);
        assert!(LightEffect::from_response(&raw).is_err());
    }

    #[test]
    fn active_custom_table() {
        let t = ActiveCustomTable::from_response(&single(
            ReadRequest::ActiveCustomTable,
            0x09,
            0x00,
            &[],
        ))
        .unwrap();
        assert_eq!(t.table, Some(CustomTable::Custom1));
        let t = ActiveCustomTable::from_response(&single(
            ReadRequest::ActiveCustomTable,
            0x09,
            0x01,
            &[],
        ))
        .unwrap();
        assert_eq!(t.table, Some(CustomTable::Custom2));
    }

    #[test]
    fn color_table() {
        let mut bytes = vec![0u8; 396];
        for index in [46usize, 103, 126] {
            bytes[index * 3] = 0xff;
        }
        let t = ColorTable::from_response(&table(
            ReadRequest::CustomColors(CustomTable::Custom1),
            &bytes,
        ))
        .unwrap();
        assert_eq!(t.colors.len(), 132);
        assert_eq!(t.lit_indices(), vec![46, 103, 126]);
        assert_eq!(t.color(46), Some(Rgb::new(255, 0, 0)));
        assert_eq!(t.color(200), None);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn rgb_serde() {
        let json = serde_json::to_string(&Rgb::new(0xff, 0x10, 0x3b)).unwrap();
        assert_eq!(json, "\"#ff103b\"");
        assert_eq!(
            serde_json::from_str::<Rgb>("\"ff103b\"").unwrap(),
            Rgb::new(0xff, 0x10, 0x3b)
        );
        assert!(serde_json::from_str::<Rgb>("\"#ff10\"").is_err());
    }
}
