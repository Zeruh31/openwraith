//! Distance units (spec section 2.9).
//!
//! Travel, Rapid Trigger sensitivity and dead band are integers in *trigger
//! units*. One unit is `rawUnit / 100` mm (or `/ 1000` with the high-precision
//! flag). On the W75 `rawUnit` = 2 and high precision is off, so
//! **1 unit = 0.02 mm** (CONFIRMED). [`Distance`] assumes that unit; check
//! [`TriggerGlobal::unit_micrometres`](crate::TriggerGlobal::unit_micrometres)
//! before using it for another model.

use std::fmt;

/// A distance in W75 trigger units of 0.02 mm.
///
/// The wire format splits values into a low and a high byte, so the value is
/// stored as `u16`; every value the W75 reports fits in the low byte.
///
/// ```
/// use openwraith_protocol::Distance;
/// assert_eq!(Distance::from_units(0x55).to_string(), "1.70 mm");
/// assert_eq!(Distance::from_units(0x0F).to_string(), "0.30 mm");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Distance(pub u16);

impl Distance {
    /// Micrometres per unit on the W75 (0.02 mm).
    pub const MICROMETRES_PER_UNIT: u32 = 20;

    /// Creates a distance from a raw unit count.
    pub const fn from_units(units: u16) -> Self {
        Distance(units)
    }

    /// Creates a distance from separate low and high bytes, as the protocol sends them.
    pub const fn from_lo_hi(lo: u8, hi: u8) -> Self {
        Distance(u16::from_le_bytes([lo, hi]))
    }

    /// The raw unit count.
    pub const fn units(self) -> u16 {
        self.0
    }

    /// The distance in micrometres.
    pub const fn micrometres(self) -> u32 {
        self.0 as u32 * Self::MICROMETRES_PER_UNIT
    }

    /// The distance in millimetres.
    pub fn to_mm(self) -> f64 {
        f64::from(self.micrometres()) / 1000.0
    }
}

impl fmt::Display for Distance {
    /// Formats as millimetres with two decimals, e.g. `1.70 mm`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hundredths = self.micrometres() / 10;
        write!(f, "{}.{:02} mm", hundredths / 100, hundredths % 100)
    }
}

impl From<u8> for Distance {
    fn from(units: u8) -> Self {
        Distance(u16::from(units))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_examples() {
        assert_eq!(Distance::from(0x55).to_string(), "1.70 mm");
        assert_eq!(Distance::from(0x32).to_string(), "1.00 mm");
        assert_eq!(Distance::from(0x0F).to_string(), "0.30 mm");
        assert_eq!(Distance::from(0x01).to_string(), "0.02 mm");
        assert_eq!(Distance::from(0xAA).to_string(), "3.40 mm");
        assert_eq!(Distance::from(0).to_string(), "0.00 mm");
    }

    #[test]
    fn lo_hi_and_mm() {
        assert_eq!(Distance::from_lo_hi(0xAA, 0x00).units(), 170);
        assert_eq!(Distance::from_lo_hi(0x00, 0x01).units(), 256);
        assert!((Distance::from(0x32).to_mm() - 1.0).abs() < 1e-9);
        assert_eq!(Distance::from(0x0F).micrometres(), 300);
    }
}
