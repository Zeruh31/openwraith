//! Test support: the USB captures in `captures/` and small report builders.

#![allow(dead_code)]

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use openwraith_hid::protocol::capture::{Capture, CaptureEvent};
use openwraith_hid::protocol::{REPORT_LEN, ReadRequest, Report, is_keyboard_reset};
use openwraith_hid::{BackupOptions, MockTransport, ToolInfo};

pub const C06: &str = "capture_06_powercycle_W_persisted";
pub const C07: &str = "capture_07_fn_space_reset";

/// Index of the W key (row 2, col 2).
pub const W: u8 = 46;

/// Path of `captures/<name>.json` at the repository root.
pub fn capture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../captures")
        .join(format!("{name}.json"))
}

/// Loads and parses a capture.
pub fn load(name: &str) -> Capture {
    let path = capture_path(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    Capture::from_json(&text).unwrap_or_else(|e| panic!("cannot parse {name}: {e}"))
}

/// A mock replaying a capture.
pub fn mock(name: &str) -> MockTransport {
    MockTransport::from_capture_file(capture_path(name)).expect("capture loads")
}

/// A report from its leading bytes (rest zero).
pub fn report(prefix: &[u8]) -> Report {
    let mut bytes = [0u8; REPORT_LEN];
    bytes[..prefix.len()].copy_from_slice(prefix);
    Report::new(bytes)
}

/// The keyboard's unsolicited reset notification as captured in c07.
pub fn captured_reset_report() -> Report {
    load(C07)
        .entries
        .iter()
        .find_map(|e| match &e.event {
            CaptureEvent::In(r) if is_keyboard_reset(r) => Some(r.clone()),
            _ => None,
        })
        .expect("c07 contains the reset notification")
}

/// The first status report of a capture (identical in every W75 capture).
pub fn captured_status_report(name: &str) -> Report {
    load(name)
        .in_reports()
        .find(|(_, r)| r.cmd() == 0x01)
        .map(|(_, r)| r.clone())
        .expect("capture contains a status report")
}

/// Fixed options so that backups are reproducible.
pub fn fixed_options() -> BackupOptions {
    BackupOptions {
        tool: ToolInfo {
            name: "openwraith-tests".into(),
            version: "0".into(),
        },
        created_at: Some(fixed_time()),
    }
}

/// 2026-10-24T07:30:00Z.
pub fn fixed_time() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_792_827_000)
}

/// Asserts that every packet a test wrote is one of our read requests and
/// never a reset.
pub fn assert_only_reads(mock: &MockTransport) {
    for packet in mock.written() {
        let bytes = packet.as_bytes();
        assert!(
            ReadRequest::from_packet(bytes).is_some(),
            "wrote a non-read packet {packet:?}"
        );
        assert_ne!(bytes[0], 0x14, "wrote a 0x14 packet {packet:?}");
    }
}

/// A mock whose device-info reply names an unknown product (`AP61HEARGB`).
pub fn unknown_model_mock() -> MockTransport {
    let mut chunk0 = vec![0x0D, 0x00, 0x00, 0x00, 0x32];
    chunk0.extend_from_slice(b"W669,34,KB,ML,AP61HEARGB,V3.22.07");
    let mut chunk1 = vec![0x0D, 0x00, 0x00, 0x01, 0x32];
    chunk1.extend_from_slice(b"Aug  6 2026,13:46:54");
    let mut mock = MockTransport::new();
    mock.add_reply(
        ReadRequest::DeviceInfo,
        vec![report(&chunk0), report(&chunk1)],
    );
    mock
}

/// A unique scratch file path under the system temp directory.
pub fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("openwraith-hid-test-{}-{name}", std::process::id()))
}
