//! Test support: the USB captures in `captures/` and small report builders.

#![allow(dead_code)]

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use openwraith_hid::protocol::capture::{Capture, CaptureEvent};
use openwraith_hid::protocol::{
    KeyPos, ProfileId, REPORT_LEN, ReadRequest, Report, is_keyboard_reset,
};
use openwraith_hid::{
    Backup, BackupOptions, ConsentOrigin, DeviceSim, MockTransport, ProfilesOptions, ScopeConfig,
    Session, SimPacket, ToolInfo, WriteConsent, read_all_with,
};

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

pub const C08: &str = "capture_08_profile_switch";
pub const C10: &str = "capture_10_triggers_Q_W_profile_reset";

/// Profile ids by number.
pub fn profile(n: u8) -> ProfileId {
    ProfileId::new(n).expect("profile 1-4")
}

/// A key position.
pub fn pos(row: u8, col: u8) -> KeyPos {
    KeyPos::new(row, col).expect("matrix position")
}

/// Consent as given by a command-line flag.
pub fn consent() -> WriteConsent {
    WriteConsent::new(ConsentOrigin::CommandLineFlag)
}

/// The c06 backup (profile 1: W 1.00 mm RT 0.30, Custom red light, 81 red
/// colour slots, empty macros, zero keymaps).
pub fn c06_backup() -> Backup {
    let mut session = Session::new(mock(C06));
    read_all_with(&mut session, &fixed_options(), |_| {}).expect("backup of c06")
}

/// Options for all-profile reads with a fixed time.
pub fn profiles_options() -> ProfilesOptions {
    ProfilesOptions {
        backup: fixed_options(),
        ..ProfilesOptions::default()
    }
}

/// A simulator seeded from c06 in which profiles 2-4 have factory triggers
/// and the factory light of c08 (profile 1 keeps c06), so that the profiles
/// differ like on the real keyboard.
pub fn sim_with_profiles(scope: ScopeConfig) -> DeviceSim {
    let mut sim = DeviceSim::from_backup_with(&c06_backup(), scope);
    let factory = DeviceSim::from_backup_with(&c06_backup(), scope);
    let mut factory = factory;
    factory.factory_reset_all();
    for n in 2..=4 {
        let p = profile(n);
        for key in openwraith_hid::protocol::W75.positions() {
            let record = factory.trigger(p, key).expect("factory trigger");
            sim.set_trigger(p, key, record);
        }
        sim.set_table(
            p,
            openwraith_hid::SimTable::MainLight,
            openwraith_hid::sim::FACTORY_LIGHT.to_vec(),
        );
    }
    sim
}

/// The write packets a simulator received, as byte arrays.
pub fn write_bytes(sim: &DeviceSim) -> Vec<[u8; REPORT_LEN]> {
    sim.write_packets().map(|p| *p.as_bytes()).collect()
}

/// Profile selects (`21 00 00 00 00 12 PP`) the simulator received, in order.
pub fn selects(sim: &DeviceSim) -> Vec<u8> {
    sim.written()
        .iter()
        .filter(|p| p.is_write())
        .map(SimPacket::as_bytes)
        .filter(|b| b[..6] == [0x21, 0, 0, 0, 0, 0x12])
        .map(|b| b[6])
        .collect()
}

/// The index (into `DeviceSim::written`) and virtual time of the last
/// trigger write (`21 00 00 00 18 ..`) the simulator received.
pub fn last_trigger_write(sim: &DeviceSim) -> (usize, Duration) {
    sim.written()
        .iter()
        .zip(sim.sent_times())
        .enumerate()
        .filter(|(_, (p, _))| p.is_write() && p.as_bytes()[..5] == [0x21, 0, 0, 0, 0x18])
        .map(|(i, (_, t))| (i, *t))
        .next_back()
        .expect("a trigger write")
}

/// A scratch directory for safety backups.
pub fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("temporary directory")
}

/// Rewrites a report the host reads (bytes, profile selects sent so far).
pub type Rewrite<'a> = Box<dyn FnMut(&mut [u8; REPORT_LEN], usize) + 'a>;

/// A transport over a simulator that tampers with what the host reads:
/// reports can be rewritten (knowing how many profile selects were sent),
/// and one read can be interrupted by a "signal" (EINTR) once the host sent
/// a number of write packets.
pub struct Tamper<'a> {
    /// The simulated keyboard.
    pub sim: &'a mut DeviceSim,
    /// Rewrites each report read (bytes, profile selects sent so far).
    pub rewrite: Rewrite<'a>,
    /// The first read after this many write packets fails once with
    /// `TransportError::Interrupted`.
    pub interrupt_after_writes: Option<usize>,
    writes: usize,
    selects: usize,
    interrupted: bool,
}

impl<'a> Tamper<'a> {
    /// No tampering yet.
    pub fn new(sim: &'a mut DeviceSim) -> Self {
        Tamper {
            sim,
            rewrite: Box::new(|_, _| {}),
            interrupt_after_writes: None,
            writes: 0,
            selects: 0,
            interrupted: false,
        }
    }
}

impl openwraith_hid::Transport for Tamper<'_> {
    fn write(
        &mut self,
        packet: &openwraith_hid::protocol::OutPacket,
    ) -> Result<(), openwraith_hid::TransportError> {
        openwraith_hid::Transport::write(&mut *self.sim, packet)
    }

    fn read(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Report>, openwraith_hid::TransportError> {
        if !self.interrupted && self.interrupt_after_writes == Some(self.writes) {
            self.interrupted = true;
            return Err(openwraith_hid::TransportError::Interrupted);
        }
        let report = openwraith_hid::Transport::read(&mut *self.sim, timeout)?;
        Ok(report.map(|r| {
            let mut bytes = *r.as_bytes();
            (self.rewrite)(&mut bytes, self.selects);
            Report::new(bytes)
        }))
    }

    fn usb_info(&self) -> Option<openwraith_hid::UsbInfo> {
        openwraith_hid::Transport::usb_info(&*self.sim)
    }

    fn now(&self) -> std::time::Instant {
        openwraith_hid::Transport::now(&*self.sim)
    }
}

impl openwraith_hid::WriteTransport for Tamper<'_> {
    fn write_command(
        &mut self,
        packet: &openwraith_hid::protocol::WritePacket,
    ) -> Result<(), openwraith_hid::TransportError> {
        self.writes += 1;
        if packet.as_bytes()[..6] == [0x21, 0, 0, 0, 0, 0x12] {
            self.selects += 1;
        }
        openwraith_hid::WriteTransport::write_command(&mut *self.sim, packet)
    }
}
