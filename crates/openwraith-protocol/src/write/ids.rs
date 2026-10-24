//! Validated identifiers used by writes: onboard profiles and advanced-key
//! slots. Macro slots reuse [`MacroSlotId`](crate::MacroSlotId).

use std::fmt;

/// An onboard profile number, 1..=4 (spec section 4.5).
///
/// Profiles 1-4 are OBSERVED as valid (c04, c08, c10). Profile 0 is never
/// selected: whether it exists is open question Q11.
///
/// ```
/// use openwraith_protocol::write::ProfileId;
/// assert_eq!(ProfileId::new(2).map(ProfileId::get), Some(2));
/// assert!(ProfileId::new(0).is_none());
/// assert!(ProfileId::new(5).is_none());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
pub struct ProfileId(u8);

impl ProfileId {
    /// Number of onboard profiles.
    pub const COUNT: u8 = 4;

    /// All four profiles in ascending order.
    pub const ALL: [ProfileId; 4] = [ProfileId(1), ProfileId(2), ProfileId(3), ProfileId(4)];

    /// Creates a profile id, or `None` if `profile` is not in 1..=4.
    pub const fn new(profile: u8) -> Option<Self> {
        if profile >= 1 && profile <= Self::COUNT {
            Some(ProfileId(profile))
        } else {
            None
        }
    }

    /// The profile number (1..=4), as used on the wire.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for ProfileId {
    type Error = String;

    fn try_from(profile: u8) -> Result<Self, Self::Error> {
        ProfileId::new(profile).ok_or_else(|| format!("profile {profile} is not in 1..=4"))
    }
}

impl From<ProfileId> for u8 {
    fn from(profile: ProfileId) -> u8 {
        profile.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A validated advanced-key slot number, 0..=39 (spec section 4.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
pub struct AdvancedSlotId(u8);

impl AdvancedSlotId {
    /// Number of advanced-key slots.
    pub const COUNT: u8 = 40;

    /// Creates a slot id, or `None` if `slot` is not in 0..=39.
    pub const fn new(slot: u8) -> Option<Self> {
        if slot < Self::COUNT {
            Some(AdvancedSlotId(slot))
        } else {
            None
        }
    }

    /// The slot number (0..=39), as used in byte 1 of the write.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// All forty slots in order.
    pub fn all() -> impl Iterator<Item = AdvancedSlotId> {
        (0..Self::COUNT).map(AdvancedSlotId)
    }
}

impl TryFrom<u8> for AdvancedSlotId {
    type Error = String;

    fn try_from(slot: u8) -> Result<Self, Self::Error> {
        AdvancedSlotId::new(slot)
            .ok_or_else(|| format!("advanced-key slot {slot} is not in 0..=39"))
    }
}

impl From<AdvancedSlotId> for u8 {
    fn from(slot: AdvancedSlotId) -> u8 {
        slot.0
    }
}

impl fmt::Display for AdvancedSlotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_range() {
        assert_eq!(ProfileId::ALL.map(ProfileId::get), [1, 2, 3, 4]);
        for n in [0u8, 5, 0x80, 0xFF] {
            assert!(ProfileId::new(n).is_none(), "{n}");
            assert!(ProfileId::try_from(n).is_err(), "{n}");
        }
        assert_eq!(ProfileId::try_from(3).map(u8::from), Ok(3));
        assert_eq!(
            ProfileId::new(4).map(|p| p.to_string()).as_deref(),
            Some("4")
        );
    }

    #[test]
    fn advanced_slot_range() {
        assert_eq!(AdvancedSlotId::all().count(), 40);
        assert_eq!(
            AdvancedSlotId::all().last().map(AdvancedSlotId::get),
            Some(39)
        );
        assert!(AdvancedSlotId::new(40).is_none());
        assert!(AdvancedSlotId::try_from(0x80).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_validates() {
        assert_eq!(serde_json::to_string(&ProfileId::ALL[1]).unwrap(), "2");
        assert_eq!(
            serde_json::from_str::<ProfileId>("4").unwrap(),
            ProfileId::ALL[3]
        );
        assert!(serde_json::from_str::<ProfileId>("0").is_err());
        assert!(serde_json::from_str::<AdvancedSlotId>("40").is_err());
        assert_eq!(
            serde_json::from_str::<AdvancedSlotId>("39")
                .map(AdvancedSlotId::get)
                .ok(),
            Some(39)
        );
    }
}
