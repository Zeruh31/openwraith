//! Full backups replayed from the USB captures.

mod common;

use common::{
    C06, C07, W, assert_only_reads, captured_reset_report, captured_status_report, fixed_options,
    mock, report, unknown_model_mock,
};
use openwraith_hid::protocol::{
    CustomTable, Decoded, KeyPos, LightMode, REPORT_LEN, ReadRequest, Rgb, TriggerMode, W75,
};
use openwraith_hid::{
    BACKUP_FORMAT, BACKUP_SCHEMA_VERSION, Backup, BackupError, MockTransport, Session,
    SessionError, read_all, read_all_with,
};

fn c06_backup() -> (Backup, MockTransport) {
    let mut session = Session::new(mock(C06));
    let backup = read_all_with(&mut session, &fixed_options(), |_| {}).expect("backup of c06");
    (backup, session.into_transport())
}

#[test]
fn backup_of_capture_06_has_the_persisted_settings() {
    let (backup, mock) = c06_backup();

    assert_eq!(backup.format, BACKUP_FORMAT);
    assert_eq!(backup.schema_version, BACKUP_SCHEMA_VERSION);
    assert_eq!(backup.created_at, "2026-10-24T07:30:00Z");
    assert_eq!(backup.device.model.as_deref(), Some("Wraith W75"));
    assert_eq!(backup.device.product.as_deref(), Some("W75HEARGB"));
    assert_eq!(backup.device.firmware.as_deref(), Some("3.22.07"));
    assert_eq!(backup.device.firmware_raw.as_deref(), Some("V3.22.07"));
    let usb = backup.device.usb.as_ref().expect("usb identity from OPEN");
    assert_eq!((usb.vendor_id, usb.product_id), (0x2E3C, 0xC368));
    assert_eq!(backup.active_profile, Some(1));
    assert_eq!(backup.model(), Some(&W75));

    // Every request of the plan, in order, each decoded.
    let status = backup.status().expect("status");
    let plan = ReadRequest::full_read_plan(&W75, Some(status));
    let requests: Vec<ReadRequest> = backup.entries.iter().map(|e| e.request()).collect();
    assert_eq!(requests, plan);
    assert_eq!(backup.undecoded().count(), 0);

    // W: 1.00 mm, Rapid Trigger 0.30 / 0.30 mm (c06: after a USB power cycle).
    let triggers = backup.key_triggers();
    assert_eq!(triggers.len(), 82);
    let w = backup.key_trigger(W).expect("W");
    assert_eq!(w.mode, TriggerMode::RapidTrigger);
    assert_eq!(w.actuation.to_string(), "1.00 mm");
    assert_eq!(w.rt_press.to_string(), "0.30 mm");
    assert_eq!(w.rt_release.to_string(), "0.30 mm");
    for t in triggers.iter().filter(|t| t.index() != W) {
        assert_eq!(t.mode, TriggerMode::Off, "key {}", t.index());
        assert_eq!(t.actuation.to_string(), "1.70 mm", "key {}", t.index());
    }

    // Main light: Custom (per-key) with red, Custom1 = 81 red slots.
    let light = backup.main_light().expect("main light");
    assert_eq!(light.mode, LightMode::Custom);
    assert_eq!(light.foreground, Rgb::new(0xff, 0, 0));
    let custom1 = backup
        .custom_colors()
        .into_iter()
        .find(|t| t.table == CustomTable::Custom1)
        .expect("custom 1");
    assert_eq!(custom1.lit_indices().len(), 81);
    assert_eq!(backup.keymaps().len(), 2);
    assert_eq!(backup.macros().len(), 10);
    assert!(backup.macros().iter().all(|m| m.is_empty()));

    // Only read requests were sent: the plan plus the final profile check.
    assert_eq!(mock.written().len(), plan.len() + 1);
    assert_only_reads(&mock);
}

#[test]
fn raw_reports_are_stored_losslessly() {
    let (backup, _) = c06_backup();
    let json = backup.to_json().unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let entries = value["entries"].as_array().unwrap();
    assert_eq!(entries.len(), backup.entries.len());
    for entry in entries {
        for report in entry["raw"]["reports"].as_array().unwrap() {
            // All 63 bytes, including stale bytes after the declared length.
            assert_eq!(report.as_str().unwrap().len(), REPORT_LEN * 2);
        }
        assert!(entry.get("decoded").is_some());
    }
    // Chunk 7 of the colour table declares 18 bytes but carries stale buffer
    // data after them (spec section 2.3); the raw report keeps it.
    let colors = backup
        .response(ReadRequest::CustomColors(CustomTable::Custom1))
        .unwrap();
    let last = colors.reports().last().unwrap();
    assert_eq!(last.declared_len(), 0x12);
    assert!(last.as_bytes()[5 + 0x12..].iter().any(|b| *b != 0));
    let reloaded = Backup::from_json(&json).unwrap();
    let reloaded_last = reloaded
        .response(ReadRequest::CustomColors(CustomTable::Custom1))
        .and_then(|r| r.reports().last().cloned())
        .unwrap();
    assert_eq!(&reloaded_last, last);
}

#[test]
fn json_round_trip_rebuilds_decoded_views_from_raw() {
    let (backup, _) = c06_backup();
    let json = backup.to_json().unwrap();
    let back = Backup::from_json(&json).unwrap();
    assert_eq!(back, backup);

    // A hand-edited decoded view is ignored: raw is authoritative.
    let edited = json.replacen("\"custom\"", "\"static\"", 1);
    assert_ne!(edited, json);
    let back = Backup::from_json(&edited).unwrap();
    assert_eq!(back.main_light().unwrap().mode, LightMode::Custom);
    assert_eq!(back, backup);
}

#[test]
fn plain_serde_gives_the_same_backup_as_from_json() {
    let (backup, _) = c06_backup();
    let via_json = Backup::from_json(&backup.to_json().unwrap()).unwrap();
    let value = serde_json::to_value(&backup).unwrap();
    let via_serde: Backup = serde_json::from_value(value).unwrap();
    assert_eq!(via_serde, via_json);
    assert_eq!(via_serde, backup);
    // The decoded views are rebuilt, not left empty.
    assert_eq!(via_serde.key_triggers().len(), 82);
    assert_eq!(
        via_serde.main_light().map(|l| l.mode),
        Some(LightMode::Custom)
    );
    assert_eq!(via_serde.undecoded().count(), 0);
}

#[test]
fn header_fields_are_derived_from_the_raw_replies() {
    let (backup, _) = c06_backup();
    let mut value = serde_json::to_value(&backup).unwrap();
    value["device"]["product"] = "SOMETHING".into();
    value["device"]["model"] = "Other".into();
    value["device"]["firmware"] = "9.99.99\u{1b}[5m".into();
    value["device"]["info"] = "\u{1b}]0;pwned\u{7}".into();
    value["active_profile"] = 3.into();
    let text = serde_json::to_string(&value).unwrap();
    let loaded = [
        Backup::from_json(&text).unwrap(),
        serde_json::from_value::<Backup>(value).unwrap(),
    ];
    for loaded in loaded {
        // Raw wins: the 0D reply says W75HEARGB / V3.22.07, 21/11 says 1.
        assert_eq!(loaded.device, backup.device);
        assert_eq!(loaded.device.firmware.as_deref(), Some("3.22.07"));
        assert_eq!(loaded.active_profile, Some(1));
        assert_eq!(loaded.model(), Some(&W75));
        assert_eq!(loaded, backup);
    }

    // Without a device-info reply nothing is known about the keyboard,
    // whatever the header says.
    let mut value = serde_json::to_value(&backup).unwrap();
    value["entries"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["raw"]["request"] != "device_info");
    let loaded: Backup = serde_json::from_value(value).unwrap();
    assert_eq!(loaded.device.product, None);
    assert_eq!(loaded.device.model, None);
    assert_eq!(loaded.device.firmware, None);
    assert!(loaded.device.info.is_empty());
    assert_eq!(loaded.device.usb, backup.device.usb);
    assert_eq!(loaded.model(), None);
    assert_eq!(loaded.active_profile, Some(1));
}

#[test]
fn a_request_stored_twice_is_rejected() {
    let (backup, _) = c06_backup();
    let mut value = serde_json::to_value(&backup).unwrap();
    let entries = value["entries"].as_array_mut().unwrap();
    let light = entries
        .iter()
        .find(|e| e["raw"]["request"] == "main_light")
        .cloned()
        .unwrap();
    entries.push(light);
    let text = serde_json::to_string(&value).unwrap();
    assert!(matches!(
        Backup::from_json(&text),
        Err(BackupError::DuplicateEntry {
            request: ReadRequest::MainLight
        })
    ));
    assert!(serde_json::from_value::<Backup>(value).is_err());
}

#[test]
fn plain_serde_checks_format_and_schema() {
    let (backup, _) = c06_backup();
    let mut value = serde_json::to_value(&backup).unwrap();
    value["format"] = "something-else".into();
    let err = serde_json::from_value::<Backup>(value).unwrap_err();
    assert!(
        err.to_string().contains("not an OpenWraith backup"),
        "{err}"
    );
    let mut value = serde_json::to_value(&backup).unwrap();
    value["schema_version"] = 99.into();
    let err = serde_json::from_value::<Backup>(value).unwrap_err();
    assert!(err.to_string().contains("schema version 99"), "{err}");
}

#[test]
fn an_undecodable_reply_is_kept_raw() {
    let esc = ReadRequest::KeyTrigger(KeyPos::new(0, 0).unwrap());
    // Header and row/col echo intact, but only 3 bytes declared.
    let mut short = [0u8; 17];
    short[..9].copy_from_slice(&[0x21, 0, 0, 0, 0x03, 0x05, 0x00, 0x55, 0x55]);
    let short = report(&short);
    let mut mock = mock(C06);
    mock.set_reply(esc, vec![short.clone()]);
    let mut session = Session::new(mock);
    let backup =
        read_all_with(&mut session, &fixed_options(), |_| {}).expect("the backup still succeeds");

    let undecoded: Vec<_> = backup.undecoded().collect();
    assert_eq!(undecoded.len(), 1);
    assert_eq!(undecoded[0].request(), esc);
    let error = undecoded[0].decode_error.as_deref().unwrap_or_default();
    assert!(error.contains("need 12 byte(s), got 3"), "{error}");
    assert_eq!(backup.key_triggers().len(), 81);

    // Lossless: the raw report survives a round trip, still undecoded.
    let reloaded = Backup::from_json(&backup.to_json().unwrap()).unwrap();
    assert_eq!(reloaded, backup);
    assert_eq!(reloaded.undecoded().count(), 1);
    assert_eq!(
        reloaded.response(esc).map(|r| r.reports().to_vec()),
        Some(vec![short])
    );
}

#[test]
fn foreign_or_tampered_files_are_rejected() {
    assert!(matches!(
        Backup::from_json("{}"),
        Err(BackupError::NotABackup { found: None, .. })
    ));
    assert!(matches!(
        Backup::from_json("[1, 2]"),
        Err(BackupError::Json(_))
    ));
    let (backup, _) = c06_backup();
    let json = backup.to_json().unwrap();
    let newer = json.replacen("\"schema_version\": 1", "\"schema_version\": 99", 1);
    assert!(matches!(
        Backup::from_json(&newer),
        Err(BackupError::UnsupportedSchema { found: 99, .. })
    ));
    // A reply filed under the wrong request fails validation.
    let wrong = json.replacen("\"request\": \"status\"", "\"request\": \"poll_rate\"", 1);
    assert!(matches!(
        Backup::from_json(&wrong),
        Err(BackupError::Json(_))
    ));
}

/// Names of the temporary files `Backup::save` leaves in `dir` (none expected).
fn leftover_temp_files(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".openwraith-") && name.ends_with(".partial"))
        .collect()
}

#[test]
fn save_never_overwrites_by_accident() {
    let (backup, _) = c06_backup();
    assert_eq!(
        backup.default_file_name(),
        "openwraith-W75HEARGB-20261024-073000.owbackup.json"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(backup.default_file_name());
    backup.save(&path, false).unwrap();
    assert!(matches!(
        backup.save(&path, false),
        Err(BackupError::AlreadyExists { .. })
    ));
    backup.save(&path, true).unwrap();
    let loaded = Backup::load(&path).unwrap();
    assert_eq!(loaded, backup);
    assert!(leftover_temp_files(dir.path()).is_empty());
}

#[test]
fn save_leaves_other_files_alone_and_cleans_up_after_errors() {
    let (backup, _) = c06_backup();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("w75.owbackup.json");

    // An existing file is untouched without overwrite.
    std::fs::write(&target, "original").unwrap();
    assert!(matches!(
        backup.save(&target, false),
        Err(BackupError::AlreadyExists { .. })
    ));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "original");

    // An unrelated file named like an old-style temporary file is not used.
    let unrelated = dir.path().join("w75.owbackup.json.partial");
    std::fs::write(&unrelated, "keep me").unwrap();
    backup.save(&target, true).unwrap();
    assert_eq!(Backup::load(&target).unwrap(), backup);
    assert_eq!(std::fs::read_to_string(&unrelated).unwrap(), "keep me");

    // A failed save (the target is a directory) leaves no temporary file.
    let blocked = dir.path().join("blocked.owbackup.json");
    std::fs::create_dir(&blocked).unwrap();
    assert!(matches!(
        backup.save(&blocked, true),
        Err(BackupError::Io { .. })
    ));
    assert!(matches!(
        backup.save(&blocked, false),
        Err(BackupError::AlreadyExists { .. })
    ));
    // A missing directory is an I/O error too.
    assert!(matches!(
        backup.save(&dir.path().join("missing").join("b.owbackup.json"), false),
        Err(BackupError::Io { .. })
    ));
    assert!(
        leftover_temp_files(dir.path()).is_empty(),
        "{:?}",
        leftover_temp_files(dir.path())
    );
}

#[test]
fn interleaved_unrelated_traffic_does_not_change_the_backup() {
    let (clean, _) = c06_backup();
    let noise = vec![
        // Heartbeat reply to another program (same bytes the W75 always sends).
        captured_status_report(C06),
        // Trigger-write ack, music-rhythm ack, bare post-reset ack.
        report(&[0x21, 0, 0, 0, 0x01, 0x00]),
        report(&[0x17, 0, 0, 0, 0x00]),
        report(&[0x14, 0, 0, 0, 0x00]),
        // Live travel stream report (travel test of another program).
        report(&[0x21, 0, 0, 0, 0x03, 0x01, 0x02, 0x02, 0x30]),
        // Another program's read of a key that is not the one we ask for.
        report(&[
            0x21, 0, 0, 0, 0x0c, 0x05, 0, 0x10, 0x10, 1, 1, 0, 0, 0, 0, 5, 20,
        ]),
    ];
    let mut session = Session::new(mock(C06).with_noise(noise));
    let noisy = read_all_with(&mut session, &fixed_options(), |_| {}).expect("noisy backup");
    assert_eq!(noisy, clean);
    let stats = session.stats();
    assert!(stats.ignored > 100, "{stats:?}");
    assert!(stats.status_reports > 10, "{stats:?}");
    assert_eq!(stats.retries, 0);
    assert_only_reads(session.transport());
}

#[test]
fn keyboard_reset_aborts_the_backup_and_is_not_answered() {
    let mut mock = mock(C07);
    // The keyboard resets while the per-key triggers are being read.
    mock.schedule(40, captured_reset_report());
    let mut session = Session::new(mock);
    let err = read_all(&mut session).unwrap_err();
    assert!(
        matches!(
            err,
            BackupError::Session(SessionError::KeyboardReset {
                request: ReadRequest::KeyTrigger(_),
                ..
            })
        ),
        "{err}"
    );
    let mock = session.into_transport();
    assert_eq!(mock.written().len(), 40, "nothing sent after the reset");
    assert_only_reads(&mock);
}

#[test]
fn profile_change_during_the_backup_is_detected() {
    let mut mock = mock(C06);
    // Fn + 2 pressed half-way: the keyboard reports profile 2.
    mock.schedule(60, report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x02, 0x00]));
    let mut session = Session::new(mock);
    let err = read_all(&mut session).unwrap_err();
    assert!(
        matches!(
            err,
            BackupError::ProfileChanged {
                before: 1,
                after: 2
            }
        ),
        "{err}"
    );
}

#[test]
fn unknown_models_are_refused() {
    let mut session = Session::new(unknown_model_mock());
    let err = read_all(&mut session).unwrap_err();
    assert!(
        matches!(&err, BackupError::UnsupportedModel { product: Some(p), .. } if p == "AP61HEARGB"),
        "{err}"
    );
    // Only the device info was read.
    assert_eq!(session.transport().written().len(), 1);
}

#[test]
fn progress_is_reported_for_every_request() {
    let mut session = Session::new(mock(C06));
    let mut seen = Vec::new();
    let backup = read_all_with(&mut session, &fixed_options(), |p| seen.push(p)).unwrap();
    let total = backup.entries.len() + 1;
    assert_eq!(seen.len(), total);
    assert!(seen.iter().all(|p| p.total == total));
    assert_eq!(seen.last().map(|p| p.done), Some(total));
    assert!(seen.windows(2).all(|w| w[0].done + 1 == w[1].done));
    assert!(matches!(
        backup.decoded().next(),
        Some(Decoded::DeviceInfo(_))
    ));
}
