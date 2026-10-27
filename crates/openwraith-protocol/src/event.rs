//! Classification of input reports that are not (or not only) replies to our
//! requests (spec sections 2.5, 4.5, 4.6.3, 4.11.7 and 4.15).
//!
//! The most important one is the keyboard-initiated reset: when Fn + Space is
//! held for a few seconds the keyboard resets and sends `14 00 00 00 01 01`
//! (CONFIRMED, c07). The vendor site answers it with its own reset command.
//! **We never answer it**; a session should stop and tell the user.
//!
//! Two kinds of reports come from the keyboard itself, never from another
//! program: the SOCD push after Fn + R-Shift ([`DeviceEvent::SocdPower`])
//! and the live travel stream ([`DeviceEvent::TravelStream`]).

use crate::decode::TravelReport;
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
    /// `24 02 00 00 06 PP ..` that answered none of our reads: the user
    /// pressed Fn + R-Shift on the keyboard (CONFIRMED, c15 #218 and #231).
    /// A user action on the keyboard, never another program. The report has
    /// the shape of the `24 02` read reply; a session that has a `24 02`
    /// read pending takes it as that reply instead.
    SocdPower {
        /// Payload byte 0: the SOCD power after the toggle (1 = on).
        power: u8,
        /// Payload bytes 1-5 (`00 00 00 fa e5` on the W75).
        rest: [u8; 5],
    },
    /// `21 00 00 00 (05|03) 01 ..`: a live travel-stream report (spec
    /// section 4.11.7). Sent only while a travel test is open; never another
    /// program's write.
    TravelStream(TravelReport),
}

impl DeviceEvent {
    /// Classifies `report`, or returns `None` for ordinary reports.
    ///
    /// Order: reset notification, profile report (`21`, sub 0, byte 5
    /// `11`), travel stream, SOCD push (`24 02 00 00 06`), status (`01`).
    /// A stream report cannot be mistaken for the `21 .. 01 00` write ack
    /// (byte 4 `01`, byte 5 `00`) or a `21/05` reply (byte 5 `05`).
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
            0x21 => TravelReport::parse(report).map(DeviceEvent::TravelStream),
            0x24 if report.as_bytes()[..5] == [0x24, 0x02, 0x00, 0x00, 0x06] => {
                Some(DeviceEvent::SocdPower {
                    power: report.byte(5),
                    rest: [6, 7, 8, 9, 10].map(|i| report.byte(i)),
                })
            }
            0x01 => Some(DeviceEvent::Status),
            _ => None,
        }
    }

    /// Whether the event comes from the keyboard itself (a key press or the
    /// travel stream), so that it never shows another program's activity.
    pub fn is_keyboard_initiated(&self) -> bool {
        matches!(
            self,
            DeviceEvent::SocdPower { .. } | DeviceEvent::TravelStream(_)
        )
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
    use crate::decode::TravelVariant;
    use crate::frame::REPORT_LEN;
    use crate::layout::KeyPos;

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

    #[test]
    fn keyboard_initiated_reports() {
        // c15 #218: Fn + R-Shift turned SOCD off.
        let push = report(&[0x24, 0x02, 0, 0, 0x06, 0x00, 0, 0, 0, 0xfa, 0xe5]);
        let event = DeviceEvent::classify(&push).unwrap();
        assert_eq!(
            event,
            DeviceEvent::SocdPower {
                power: 0,
                rest: [0, 0, 0, 0xfa, 0xe5]
            }
        );
        assert!(event.is_keyboard_initiated());
        // The pair reply and the chunk ack are not pushes.
        assert_eq!(
            DeviceEvent::classify(&report(&[0x24, 0x01, 0, 0, 0x28])),
            None
        );
        assert_eq!(DeviceEvent::classify(&report(&[0x24, 0, 0, 0, 0])), None);
        // c14 #1157: Esc at 169.
        let stream = report(&[0x21, 0, 0, 0, 0x05, 0x01, 0, 0, 0xa9, 0]);
        match DeviceEvent::classify(&stream) {
            Some(DeviceEvent::TravelStream(r)) => {
                assert_eq!(r.pos, KeyPos::new(0, 0).unwrap());
                assert_eq!(r.depth, 169);
                assert_eq!(r.variant, TravelVariant::Short);
            }
            other => panic!("expected a stream report, got {other:?}"),
        }
        assert!(!DeviceEvent::Status.is_keyboard_initiated());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serializes() {
        let json = serde_json::to_string(&DeviceEvent::SocdPower {
            power: 1,
            rest: [0, 0, 0, 0xfa, 0xe5],
        })
        .unwrap();
        assert_eq!(json, r#"{"socd_power":{"power":1,"rest":[0,0,0,250,229]}}"#);
        let back: DeviceEvent = serde_json::from_str(&json).unwrap();
        assert!(back.is_keyboard_initiated());
    }
}
