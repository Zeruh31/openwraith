//! Status `0x01`, Fn-locked keys `0x0E`, active profile `21/11` and polling
//! rate `21/09` (spec sections 4.2, 4.3, 4.5.2 and 4.6.1).

use super::{FromResponse, at, expect_request, need};
use crate::error::DecodeError;
use crate::hex::HexBytes;
use crate::layout::{KEY_MASK_LEN, KeyMask};
use crate::request::ReadRequest;
use crate::response::RawResponse;

/// Link type reported in status byte 10 (CODE-ONLY).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LinkType {
    /// 0: USB cable.
    Usb,
    /// 1: 2.4 GHz dongle.
    Wireless,
    /// Any other value.
    Other(u8),
}

impl LinkType {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => LinkType::Usb,
            1 => LinkType::Wireless,
            other => LinkType::Other(other),
        }
    }
}

/// Capability flags A (status byte 17; W75 `0xDF`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CapabilityFlagsA {
    /// The raw byte.
    pub raw: u8,
    /// Bit 0: calibration uses `21/0F` + `21/10` (CODE-ONLY).
    pub any_key_calibration: bool,
    /// Bit 1: per-key custom colour tables (`09 20/80/81`) (CONFIRMED).
    pub custom_light: bool,
    /// Bit 2: switch types (`25` reads) (CONFIRMED).
    pub more_switch: bool,
    /// Bit 3: polling rate (`21/09`) (CONFIRMED).
    pub poll_rate: bool,
    /// Bit 4: dead band (`26 00`) (CONFIRMED).
    pub dead_band: bool,
    /// Bit 5: music-rhythm light mode (CODE-ONLY).
    pub music_rhythm: bool,
    /// Bit 6: unknown (not read by the vendor site).
    pub unknown_bit6: bool,
    /// Bit 7: N-key / 6-key rollover selector (CODE-ONLY).
    pub rollover_switch: bool,
}

impl CapabilityFlagsA {
    fn from_byte(b: u8) -> Self {
        let bit = |n: u8| b & (1 << n) != 0;
        CapabilityFlagsA {
            raw: b,
            any_key_calibration: bit(0),
            custom_light: bit(1),
            more_switch: bit(2),
            poll_rate: bit(3),
            dead_band: bit(4),
            music_rhythm: bit(5),
            unknown_bit6: bit(6),
            rollover_switch: bit(7),
        }
    }
}

/// Capability flags B (status byte 18; W75 `0x04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CapabilityFlagsB {
    /// The raw byte.
    pub raw: u8,
    /// Bit 0: distance unit denominator 1000 instead of 100 (CONFIRMED).
    pub high_precision: bool,
    /// Bit 2: gamepad modes (CODE-ONLY).
    pub gamepad: bool,
    /// Bit 3: advanced-key type RS (CODE-ONLY).
    pub rs: bool,
    /// Bit 4: advanced-key type RKRT (CODE-ONLY).
    pub rkrt: bool,
    /// Bit 5: mouse keys allowed in DKS (CODE-ONLY).
    pub dks_mouse: bool,
    /// Bit 6: advanced-key type MDS (CODE-ONLY).
    pub mds: bool,
}

impl CapabilityFlagsB {
    fn from_byte(b: u8) -> Self {
        let bit = |n: u8| b & (1 << n) != 0;
        CapabilityFlagsB {
            raw: b,
            high_precision: bit(0),
            gamepad: bit(2),
            rs: bit(3),
            rkrt: bit(4),
            dks_mouse: bit(5),
            mds: bit(6),
        }
    }
}

/// Win-Lock options (status byte 20, CODE-ONLY).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WinLockOptions {
    /// The raw byte.
    pub raw: u8,
    /// Bit 0: block the Windows key while Win-Lock is on.
    pub block_win: bool,
    /// Bit 1: block Shift+Tab.
    pub block_shift_tab: bool,
    /// Bit 2: block Alt+F4.
    pub block_alt_f4: bool,
    /// Bit 3: block Alt+Tab.
    pub block_alt_tab: bool,
}

impl WinLockOptions {
    fn from_byte(b: u8) -> Self {
        WinLockOptions {
            raw: b,
            block_win: b & 0x01 != 0,
            block_shift_tab: b & 0x02 != 0,
            block_alt_f4: b & 0x04 != 0,
            block_alt_tab: b & 0x08 != 0,
        }
    }
}

/// Status and capabilities (`0x01`, 18-byte payload; CONFIRMED framing).
///
/// The W75 always answered `01 00 00 00 12 | 00 00 00 00 00 00 00 00 00 01 00
/// 00 df 04 01 01 00 00` in the captures.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Status {
    /// Byte 1: link-mode token; the site reloads when it changes (CODE-ONLY).
    pub link_token: u8,
    /// Byte 8: charging (battery models only, CODE-ONLY).
    pub charging: bool,
    /// Byte 9: battery percent (battery models only, CODE-ONLY).
    pub battery_percent: u8,
    /// Byte 10: link type (CODE-ONLY).
    pub link_type: LinkType,
    /// Byte 14 = 1: SOCD supported (CONFIRMED).
    pub socd_supported: bool,
    /// Byte 17: capability flags A.
    pub flags_a: CapabilityFlagsA,
    /// Byte 18: capability flags B.
    pub flags_b: CapabilityFlagsB,
    /// Byte 19: not parsed by the vendor site; `01` on the W75 (unknown).
    pub unknown_byte19: u8,
    /// Byte 20: Win-Lock option bits (CODE-ONLY).
    pub win_lock: WinLockOptions,
    /// Byte 21 = 1: music-rhythm lighting active (CODE-ONLY).
    pub music_rhythm_active: bool,
    /// Byte 22 bit 0: 6-key rollover instead of N-key (CODE-ONLY).
    pub six_key_rollover: bool,
    /// The 18-byte declared payload (bytes 5..22).
    pub raw: HexBytes,
}

impl FromResponse for Status {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "Status", |r| r == ReadRequest::Status)?;
        let link_token = response.first().map_or(0, |r| r.sub());
        let p = response.payload();
        need(&p, "status", 18)?;
        Ok(Status {
            link_token,
            charging: at(&p, 3) == 1,
            battery_percent: at(&p, 4),
            link_type: LinkType::from_byte(at(&p, 5)),
            socd_supported: at(&p, 9) == 1,
            flags_a: CapabilityFlagsA::from_byte(at(&p, 12)),
            flags_b: CapabilityFlagsB::from_byte(at(&p, 13)),
            unknown_byte19: at(&p, 14),
            win_lock: WinLockOptions::from_byte(at(&p, 15)),
            music_rhythm_active: at(&p, 16) == 1,
            six_key_rollover: at(&p, 17) & 0x01 != 0,
            raw: HexBytes(p),
        })
    }
}

/// Keys that cannot be remapped on the Fn layer (`0x0E`; bytes CONFIRMED,
/// meaning CODE-ONLY). The W75 reports only index 121 (Fn).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FnLockedKeys {
    /// The 22-byte key mask.
    pub mask: KeyMask,
    /// The declared payload.
    pub raw: HexBytes,
}

impl FnLockedKeys {
    /// Key indices whose bit is set.
    pub fn indices(&self) -> Vec<u8> {
        self.mask
            .positions()
            .into_iter()
            .map(|p| p.index())
            .collect()
    }
}

impl FromResponse for FnLockedKeys {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "FnLockedKeys", |r| r == ReadRequest::FnLockedKeys)?;
        let p = response.payload();
        need(&p, "Fn-locked key mask", KEY_MASK_LEN)?;
        Ok(FnLockedKeys {
            mask: KeyMask::from_slice(&p),
            raw: HexBytes(p),
        })
    }
}

/// Active onboard profile (`21/11` reply `11 PP XX`; CONFIRMED).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActiveProfile {
    /// Active onboard profile, 1-4 (range CODE-ONLY).
    pub profile: u8,
    /// Byte 7, not parsed by the vendor site. `01` in replies to our query,
    /// `00` in the reports that follow a profile change (unknown meaning).
    pub extra: u8,
    /// The declared payload (`11 PP XX`).
    pub raw: HexBytes,
}

impl FromResponse for ActiveProfile {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "ActiveProfile", |r| {
            r == ReadRequest::ActiveProfile
        })?;
        let p = response.payload();
        need(&p, "active profile", 3)?;
        Ok(ActiveProfile {
            profile: at(&p, 1),
            extra: at(&p, 2),
            raw: HexBytes(p),
        })
    }
}

/// USB polling rate (`21/09` reply `09 RR`; CONFIRMED, W75 `08` = 8 kHz).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PollRate {
    /// Raw rate code: 1, 2, 4 or 8 (the site ignores other values).
    pub code: u8,
    /// Rate in Hz for known codes (code x 1000).
    pub hz: Option<u32>,
    /// The declared payload (`09 RR`).
    pub raw: HexBytes,
}

impl FromResponse for PollRate {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "PollRate", |r| r == ReadRequest::PollRate)?;
        let p = response.payload();
        need(&p, "polling rate", 2)?;
        let code = at(&p, 1);
        let hz = matches!(code, 1 | 2 | 4 | 8).then(|| u32::from(code) * 1000);
        Ok(PollRate {
            code,
            hz,
            raw: HexBytes(p),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::single;

    const W75_STATUS: [u8; 18] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0xdf, 0x04, 0x01,
        0x01, 0x00, 0x00,
    ];

    #[test]
    fn w75_status() {
        let s = Status::from_response(&single(ReadRequest::Status, 0x01, 0, &W75_STATUS)).unwrap();
        assert_eq!(s.link_token, 0);
        assert_eq!(s.link_type, LinkType::Usb);
        assert!(s.socd_supported);
        assert_eq!(s.flags_a.raw, 0xdf);
        assert!(s.flags_a.any_key_calibration);
        assert!(s.flags_a.custom_light);
        assert!(s.flags_a.more_switch);
        assert!(s.flags_a.poll_rate);
        assert!(s.flags_a.dead_band);
        assert!(!s.flags_a.music_rhythm);
        assert!(s.flags_a.unknown_bit6);
        assert!(s.flags_a.rollover_switch);
        assert!(!s.flags_b.high_precision);
        assert!(s.flags_b.gamepad);
        assert!(!s.flags_b.rs && !s.flags_b.rkrt && !s.flags_b.dks_mouse && !s.flags_b.mds);
        assert_eq!(s.unknown_byte19, 1);
        assert!(s.win_lock.block_win);
        assert!(!s.win_lock.block_alt_tab);
        assert!(!s.music_rhythm_active);
        assert!(!s.six_key_rollover);
        assert_eq!(s.raw.len(), 18);
    }

    #[test]
    fn status_too_short() {
        let raw = single(ReadRequest::Status, 0x01, 0, &W75_STATUS[..10]);
        assert!(matches!(
            Status::from_response(&raw),
            Err(DecodeError::TooShort {
                need: 18,
                got: 10,
                ..
            })
        ));
    }

    #[test]
    fn fn_locked_keys() {
        let mut mask = [0u8; 22];
        mask[11] = 0x20;
        let k = FnLockedKeys::from_response(&single(ReadRequest::FnLockedKeys, 0x0E, 0, &mask))
            .unwrap();
        assert_eq!(k.indices(), vec![121]);
    }

    #[test]
    fn active_profile_and_poll_rate() {
        let p = ActiveProfile::from_response(&single(
            ReadRequest::ActiveProfile,
            0x21,
            0,
            &[0x11, 0x01, 0x01],
        ))
        .unwrap();
        assert_eq!((p.profile, p.extra), (1, 1));

        let r = PollRate::from_response(&single(ReadRequest::PollRate, 0x21, 0, &[0x09, 0x08]))
            .unwrap();
        assert_eq!(r.hz, Some(8000));
        let r = PollRate::from_response(&single(ReadRequest::PollRate, 0x21, 0, &[0x09, 0x03]))
            .unwrap();
        assert_eq!(r.hz, None);
    }
}
