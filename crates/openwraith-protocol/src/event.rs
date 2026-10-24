//! Classification of input reports that are not (or not only) replies to our
//! requests (spec sections 2.5, 4.5 and 4.6.3).
//!
//! The most important one is the keyboard-initiated reset: when Fn + Space is
//! held for a few seconds the keyboard resets and sends `14 00 00 00 01 01`
//! (CONFIRMED, c07). The vendor site answers it with its own reset command.
//! **We never answer it**; a session should stop and tell the user.

use crate::frame::Report;

/// A noteworthy input report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum DeviceEvent {
    /// `14 xx .. byte 5 = 01`: the keyboard reset itself (Fn + Space held).
    /// Every value read before this report may be stale. Never respond to it.
    KeyboardReset,
    /// `21 .. 11 PP XX`: active-profile report. It answers our `21/11` query
    /// (byte 7 = `01`) and also follows a profile change (byte 7 = `00`, c04).
    ProfileReport {
        /// Active profile (1-4).
        profile: u8,
        /// Byte 7 (meaning unknown).
        extra: u8,
    },
    /// `01 ..`: status report (heartbeat reply of another application, or an
    /// unsolicited push; OBSERVED once in c02).
    Status,
}

impl DeviceEvent {
    /// Classifies `report`, or returns `None` for ordinary reports.
    pub fn classify(report: &Report) -> Option<DeviceEvent> {
        if is_keyboard_reset(report) {
            return Some(DeviceEvent::KeyboardReset);
        }
        match report.cmd() {
            0x21 if report.sub() == 0 && report.byte(5) == 0x11 => {
                Some(DeviceEvent::ProfileReport {
                    profile: report.byte(6),
                    extra: report.byte(7),
                })
            }
            0x01 => Some(DeviceEvent::Status),
            _ => None,
        }
    }
}

/// Whether `report` is the keyboard's reset notification: byte 0 = `0x14`
/// and byte 5 = `0x01`, any byte 1 (spec section 4.6.3). The bare `14 .. 00`
/// acknowledgement that follows a reset is not a notification.
pub fn is_keyboard_reset(report: &Report) -> bool {
    report.cmd() == 0x14 && report.byte(5) == 0x01
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::REPORT_LEN;

    fn report(prefix: &[u8]) -> Report {
        let mut bytes = [0u8; REPORT_LEN];
        bytes[..prefix.len()].copy_from_slice(prefix);
        Report::new(bytes)
    }

    #[test]
    fn reset_notification() {
        let r = report(&[0x14, 0x00, 0x00, 0x00, 0x01, 0x01]);
        assert!(is_keyboard_reset(&r));
        assert_eq!(DeviceEvent::classify(&r), Some(DeviceEvent::KeyboardReset));
        // Any byte 1.
        assert!(is_keyboard_reset(&report(&[0x14, 0x03, 0, 0, 0x01, 0x01])));
        // The final ack after a reset is not a notification.
        let ack = report(&[0x14, 0x00, 0x00, 0x00, 0x00]);
        assert!(!is_keyboard_reset(&ack));
        assert_eq!(DeviceEvent::classify(&ack), None);
    }

    #[test]
    fn profile_and_status() {
        assert_eq!(
            DeviceEvent::classify(&report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x02, 0x00])),
            Some(DeviceEvent::ProfileReport {
                profile: 2,
                extra: 0
            })
        );
        assert_eq!(
            DeviceEvent::classify(&report(&[0x01, 0, 0, 0, 0x12])),
            Some(DeviceEvent::Status)
        );
        assert_eq!(
            DeviceEvent::classify(&report(&[0x21, 0, 0, 0, 0x01, 0x00])),
            None
        );
    }
}
