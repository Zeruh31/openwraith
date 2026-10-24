//! Backups for unit tests, replayed from the USB captures (no keyboard).

use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use openwraith_hid::{Backup, BackupEntry, BackupOptions, MockTransport, Session, ToolInfo};
use openwraith_protocol::{REPORT_LEN, RawResponse, ReadRequest, Report};

/// Backup replayed from capture c06 (W = 1.00 mm + RT 0.30, Custom red).
pub fn c06_backup() -> Backup {
    backup_from("capture_06_powercycle_W_persisted")
}

/// Backup replayed from `captures/<name>.json`.
pub fn backup_from(name: &str) -> Backup {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../captures")
        .join(format!("{name}.json"));
    let mock = MockTransport::from_capture_file(path).expect("capture loads");
    let options = BackupOptions {
        tool: ToolInfo {
            name: "openwraith-tests".into(),
            version: "0".into(),
        },
        created_at: Some(UNIX_EPOCH + Duration::from_secs(1_792_827_000)),
    };
    openwraith_hid::read_all_with(&mut Session::new(mock), &options, |_| {}).expect("backup")
}

/// Replaces the reply to `request` in `backup` with a table built from
/// `table` (chunked as the keyboard sends it, with read sub-command `sub`).
pub fn replace_table(backup: &mut Backup, request: ReadRequest, cmd: u8, sub: u8, table: &[u8]) {
    let layout = request.table_layout().expect("table request");
    let reports = (0..layout.chunks).map(|chunk| {
        let start = usize::from(chunk) * layout.chunk_len;
        let len = layout.expected_len(chunk);
        let mut bytes = [0u8; REPORT_LEN];
        let [hi, lo] = chunk.to_be_bytes();
        bytes[..5].copy_from_slice(&[cmd, sub, hi, lo, u8::try_from(len).expect("len")]);
        bytes[5..5 + len].copy_from_slice(&table[start..start + len]);
        Report::new(bytes)
    });
    replace_reply(backup, request, reports.collect());
}

/// Replaces the reply to `request` in `backup` with `reports` (which must be
/// a valid reply; it may still fail to decode).
pub fn replace_reply(backup: &mut Backup, request: ReadRequest, reports: Vec<Report>) {
    let raw = RawResponse::from_reports(request, reports).expect("valid reply");
    let entry = backup
        .entries
        .iter_mut()
        .find(|e| e.request() == request)
        .expect("request in backup");
    *entry = BackupEntry::new(raw);
}
