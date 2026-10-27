//! Seeds for simulated keyboards that differ from the default one.

use openwraith_hid::protocol::{REPORT_LEN, RawResponse, ReadRequest, Report};
use openwraith_hid::{Backup, BackupEntry, DeviceSim};

use crate::backend::sim::{seed_backup, sim_from};

/// A report from its leading bytes.
pub(crate) fn report(prefix: &[u8]) -> Report {
    let mut bytes = [0u8; REPORT_LEN];
    bytes[..prefix.len()].copy_from_slice(prefix);
    Report::new(bytes)
}

/// Replaces the reply to `request` in the primary snapshot.
pub(crate) fn replace(backup: &mut Backup, request: ReadRequest, reports: Vec<Report>) {
    let raw = RawResponse::from_reports(request, reports).expect("valid reply");
    let entry = backup
        .entries
        .iter_mut()
        .find(|e| e.request() == request)
        .expect("request in the backup");
    *entry = BackupEntry::new(raw);
}

/// The seed with another device-info string.
pub(crate) fn with_device_info(product: &str, firmware: &str) -> Backup {
    let mut backup = seed_backup();
    let text = format!("W669,34,KB,ML,{product},V{firmware}");
    let mut chunk0 = vec![0x0D, 0, 0, 0, 0x32];
    chunk0.extend_from_slice(text.as_bytes());
    let mut chunk1 = vec![0x0D, 0, 0, 1, 0x32];
    chunk1.extend_from_slice(b"Aug  6 2026,13:46:54");
    replace(
        &mut backup,
        ReadRequest::DeviceInfo,
        vec![report(&chunk0), report(&chunk1)],
    );
    backup
}

/// A keyboard that reports another product (`AP61HEARGB`).
pub(crate) fn unsupported_sim() -> DeviceSim {
    sim_from(&with_device_info("AP61HEARGB", "3.22.07"), "SIM-1")
}

/// A W75 with other firmware.
pub(crate) fn firmware_sim(firmware: &str) -> DeviceSim {
    sim_from(&with_device_info("W75HEARGB", firmware), "SIM-1")
}

/// A W75 whose trigger parameters (`21/04`) report another distance unit
/// (raw unit 3: 0.03 mm).
pub(crate) fn other_unit_backup() -> Backup {
    let mut backup = seed_backup();
    let raw = backup
        .primary()
        .response(ReadRequest::TriggerGlobal)
        .expect("21/04 in the seed")
        .clone();
    let mut bytes = *raw.reports()[0].as_bytes();
    // Payload `04 aa 02 02 00 05` starts at byte 5; byte 7 is the raw unit.
    bytes[7] = 3;
    replace(
        &mut backup,
        ReadRequest::TriggerGlobal,
        vec![Report::new(bytes)],
    );
    backup
}
