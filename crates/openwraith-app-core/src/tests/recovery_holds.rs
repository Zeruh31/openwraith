//! What keeps a profile held after a suspected reset, and what releases it
//! (virtual time, the simulator): a recovery restore that may not have been
//! saved or was cancelled, a partial import, R1 for the profiles not read
//! yet, the R1-only hold, the "not compared yet" profiles after a report,
//! another keyboard, keyboards without a USB serial number, the command
//! line tool's own writes, OpenWraith's own writes checked against what
//! they left in each table (also across runs), the report's banner and the
//! store's remaining paths.

use openwraith_hid::coordination::TOOL_CLI;
use openwraith_hid::known_good::FACTORY_TRIGGER_RECORD;
use openwraith_hid::protocol::{Layer, W75};
use openwraith_hid::{DeviceLock, SimTable, UsbInfo};

use super::harness::{Harness, is_write_start, profile};
use crate::autobackup;
use crate::dto::{
    ConnectionState, ExportIntent, ExportSource, Headline, ImportIntent, Notice, ProgressStep,
    RecoveryIntent, RecoveryProfileState, RecoveryStatusView, ResetEvidenceView,
    ShareSelectionView, SimAction,
};
use crate::worker::{Conn, Request};

/// The `ResetSuspected` notices sent so far.
fn reset_notices(h: &Harness) -> Vec<(u8, Vec<ResetEvidenceView>)> {
    h.notices()
        .into_iter()
        .filter_map(|n| match n {
            Notice::ResetSuspected { profile, evidence } => Some((profile, evidence)),
            _ => None,
        })
        .collect()
}

/// Fn + Space held: the factory forms, the report, the reconnect, the read.
fn reset_and_reconnect(h: &mut Harness) {
    h.with_sim(|s| s.factory_reset_all());
    h.sim.apply(SimAction::KeyboardReset).unwrap();
    h.tick();
    assert_eq!(h.worker.conn, Conn::ResetSeen);
    h.op(|reply| Request::Reconnect { reply }).unwrap();
    h.until_ready();
}

/// The automatic backup of profile 1 as a recovery of it.
fn recovery_of_profile_1(h: &mut Harness) -> crate::dto::RestorePreview {
    let backup = h.recovery().profiles[0].backups[0].file.token.clone();
    h.prepare_recovery(
        RecoveryIntent {
            files: vec![backup],
            allow_firmware_mismatch: false,
            confirmed_switches: Vec::new(),
        },
        &h.watch(),
    )
    .unwrap()
}

fn switch_to(h: &mut Harness, p: u8) {
    let preview = h.prepare_switch(p).unwrap();
    h.apply_switch(&Harness::token(&preview.token), &h.watch())
        .unwrap();
}

fn no_serial() -> Option<UsbInfo> {
    Some(UsbInfo {
        vendor_id: 0x2E3C,
        product_id: 0xC368,
        product: Some("Wraith W75".to_string()),
        manufacturer: None,
        serial: None,
    })
}

#[test]
fn a_recovery_restore_that_may_be_lost_keeps_the_profile_held() {
    for reported in [false, true] {
        for cut in ["save", "packets"] {
            let mut h = Harness::ready();
            if reported {
                reset_and_reconnect(&mut h);
            } else {
                // A reset while the app did not look (R3 only).
                h.with_sim(|s| s.factory_reset_all());
                h.refresh().unwrap();
            }
            assert_eq!(h.status().recovery.reset_suspected, vec![1]);
            assert_eq!(reset_notices(&h).len(), 1);
            let preview = recovery_of_profile_1(&mut h);
            let watch = h.watch();
            if cut == "save" {
                let sim = h.sim.clone();
                let mut done = false;
                watch.on_event(move |event, _| {
                    if !done && matches!(event.step, ProgressStep::WaitingForKeyboardSave { .. }) {
                        done = true;
                        sim.apply(SimAction::Unplug).unwrap();
                    }
                });
            } else {
                h.sim
                    .apply(SimAction::UnplugAfterPackets { packets: 5 })
                    .unwrap();
            }
            let report = h.apply_restore(&Harness::token(&preview.token), &watch);
            if let Ok(report) = &report {
                assert_ne!(report.headline, Headline::Done, "{reported} {cut}");
            }
            h.tick();
            h.sim.apply(SimAction::Replug).unwrap();
            h.until_ready();
            let status = h.status().recovery;
            assert_eq!(status.reset_suspected, vec![1], "{reported} {cut}");
            assert_eq!(h.auto_files(1).len(), 1, "{reported} {cut}: no backup");
            assert_eq!(
                h.recovery().profiles[0].state,
                RecoveryProfileState::ResetSuspected
            );
            assert_eq!(reset_notices(&h).len(), 1, "{reported} {cut}");
        }
    }
}

#[test]
fn a_cancelled_recovery_restore_keeps_the_profile_held() {
    let mut h = Harness::ready();
    h.with_sim(|s| s.factory_reset_all());
    h.refresh().unwrap();
    let preview = recovery_of_profile_1(&mut h);
    let watch = h.watch();
    watch.cancel_when(is_write_start);
    let _ = h.apply_restore(&Harness::token(&preview.token), &watch);
    h.ticks(10);
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    h.op(|reply| Request::Reconnect { reply }).unwrap();
    h.until_ready();
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    assert_eq!(h.auto_files(1).len(), 1, "nothing saved");
}

/// Exports profile 1, resets the keyboard and imports `include` of the file
/// into profile 1.
fn import_after_reset(include: ShareSelectionView) -> Harness {
    let mut h = Harness::ready();
    let destination = h.destination("p1.owprofile.json");
    h.export(
        ExportIntent {
            source: ExportSource::Keyboard,
            profile: 1,
            label: None,
            destination: destination.token.clone(),
            confirmed_switch: false,
            include: None,
        },
        &h.watch(),
    )
    .unwrap();
    reset_and_reconnect(&mut h);
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    let path = h.dir.path().join("p1.owprofile.json");
    let summary = crate::ops::share::import_summary(&h.shared, &path).unwrap();
    let preview = h
        .prepare_import(
            ImportIntent {
                source: summary.file.file.token.clone(),
                from: None,
                into: 1,
                include,
                allowed_critical_keys: Vec::new(),
                allow_firmware_mismatch: false,
                confirmed_switches: Vec::new(),
            },
            &h.watch(),
        )
        .unwrap();
    let report = h
        .apply_import(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    h
}

#[test]
fn a_partial_import_leaves_a_held_profile_held() {
    let mut h = import_after_reset(ShareSelectionView {
        triggers: false,
        lighting: true,
        keys: false,
        dead_band: false,
        socd: false,
    });
    // The triggers are still at the factory form.
    assert_eq!(
        h.with_sim(|s| s.trigger(profile(1), super::harness::pos(46))),
        Some(FACTORY_TRIGGER_RECORD)
    );
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    assert!(h.recovery().attention);
    h.refresh().unwrap();
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    assert_eq!(h.auto_files(1).len(), 1, "nothing saved");
    // Every part: the profile is the file's again.
    let mut h = import_after_reset(ShareSelectionView {
        triggers: true,
        lighting: true,
        keys: true,
        dead_band: true,
        socd: true,
    });
    assert_eq!(h.status().recovery.reset_suspected, Vec::<u8>::new());
    h.refresh().unwrap();
    assert_eq!(h.status().recovery.reset_suspected, Vec::<u8>::new());
}

#[test]
fn keeping_or_restoring_one_profile_keeps_r1_for_the_profiles_not_read_yet() {
    for resolve in ["keep", "restore"] {
        let mut h = Harness::ready();
        // Profile 2 differs from the factory forms only in tables that
        // never alarm alone.
        h.with_sim(|s| {
            let mut light = s.table(profile(2), SimTable::MainLight).unwrap().to_vec();
            light[0] = 0x0a;
            s.set_table(profile(2), SimTable::MainLight, light);
            s.set_table(profile(2), SimTable::DeadBand, vec![0x05; 264]);
        });
        switch_to(&mut h, 2);
        switch_to(&mut h, 1);
        assert_eq!(h.auto_files(2).len(), 1);
        reset_and_reconnect(&mut h);
        if resolve == "keep" {
            h.keep_current(&[]).unwrap();
        } else {
            let preview = recovery_of_profile_1(&mut h);
            let report = h
                .apply_restore(&Harness::token(&preview.token), &h.watch())
                .unwrap();
            assert_eq!(report.headline, Headline::Done);
        }
        // Profile 2 was not compared yet: the report stays.
        assert!(h.status().recovery.reset_reported, "{resolve}");
        let recovery = h.recovery();
        assert_eq!(
            recovery.profiles[1].state,
            RecoveryProfileState::NotChecked,
            "{resolve}"
        );
        switch_to(&mut h, 2);
        assert_eq!(h.status().recovery.reset_suspected, vec![2], "{resolve}");
        assert!(
            reset_notices(&h)
                .iter()
                .any(|(p, e)| *p == 2 && e.first() == Some(&ResetEvidenceView::ResetSeen)),
            "{resolve}: {:?}",
            reset_notices(&h)
        );
        assert_eq!(h.auto_files(2).len(), 1, "{resolve}: nothing saved");
    }
}

#[test]
fn after_a_reset_report_profiles_not_read_since_are_not_compared_yet() {
    let mut h = Harness::ready();
    for p in [2u8, 3, 4, 1] {
        switch_to(&mut h, p);
    }
    let before = h.recovery();
    assert!(
        before
            .profiles
            .iter()
            .all(|p| p.state == RecoveryProfileState::Fine)
    );
    reset_and_reconnect(&mut h);
    let after = h.recovery();
    assert_eq!(
        after.profiles[0].state,
        RecoveryProfileState::ResetSuspected
    );
    for p in &after.profiles[1..] {
        assert_eq!(p.state, RecoveryProfileState::NotChecked, "{}", p.profile);
    }
    // "It's fine, keep them" for profile 1 leaves the report standing.
    h.keep_current(&[]).unwrap();
    assert!(h.recovery().reset_reported);
}

#[test]
fn a_reset_report_alone_holds_a_profile_that_differs_only_in_its_light() {
    let mut h = Harness::ready();
    // A profile at the factory forms but for one byte of its light, kept
    // as its known-good settings.
    h.with_sim(|s| {
        s.factory_reset_all();
        let mut light = s.table(profile(1), SimTable::MainLight).unwrap().to_vec();
        light[0] = 0x0a;
        s.set_table(profile(1), SimTable::MainLight, light);
    });
    h.refresh().unwrap();
    h.keep_current(&[]).unwrap();
    h.refresh().unwrap();
    let files = h.auto_files(1).len();
    let notices = reset_notices(&h).len();
    reset_and_reconnect(&mut h);
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    // Later reads have no evidence of their own: still held.
    h.refresh().unwrap();
    h.refresh().unwrap();
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    assert_eq!(h.auto_files(1).len(), files, "nothing saved");
    let new: Vec<_> = reset_notices(&h).into_iter().skip(notices).collect();
    assert_eq!(new, vec![(1, vec![ResetEvidenceView::ResetSeen])]);
}

#[test]
fn a_reset_report_never_holds_a_profile_already_at_the_factory_forms() {
    let mut h = Harness::ready();
    h.with_sim(|s| s.factory_reset_all());
    h.refresh().unwrap();
    h.keep_current(&[]).unwrap();
    h.refresh().unwrap();
    let notices = reset_notices(&h).len();
    reset_and_reconnect(&mut h);
    assert_eq!(h.recovery().profiles[0].state, RecoveryProfileState::Fine);
    assert_eq!(reset_notices(&h).len(), notices, "no false alarm");
    // The only profile with automatic backups was compared and is fine:
    // the report's banner goes.
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    assert!(!h.recovery().attention);
}

#[test]
fn another_keyboard_starts_afresh() {
    let mut h = Harness::ready();
    reset_and_reconnect(&mut h);
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    // Keyboard B (another serial number) instead.
    h.sim.apply(SimAction::Unplug).unwrap();
    h.tick();
    h.with_sim(|s| {
        s.set_usb_info(Some(UsbInfo {
            vendor_id: 0x2E3C,
            product_id: 0xC368,
            product: Some("Wraith W75".to_string()),
            manufacturer: None,
            serial: Some("B-2".to_string()),
        }));
    });
    h.sim.apply(SimAction::Replug).unwrap();
    h.until_ready();
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    assert!(
        h.recovery()
            .profiles
            .iter()
            .all(|p| p.state != RecoveryProfileState::ResetSuspected)
    );
    let key_b = autobackup::device_key(Some("W75HEARGB"), Some("B-2"));
    let dir_b = autobackup::profile_dir(&h.worker.config.known_good_dir(), &key_b, 1);
    let before = autobackup::list_profile(&dir_b).unwrap().len();
    assert_eq!(before, 1, "B's first read");
    h.keep_current(&[]).unwrap();
    assert_eq!(autobackup::list_profile(&dir_b).unwrap().len(), before);
}

#[test]
fn keyboards_without_a_serial_number_share_the_store_and_say_so() {
    let mut h = Harness::new(2);
    h.with_sim(|s| s.set_usb_info(no_serial()));
    h.sim.with(1, |s| s.set_usb_info(no_serial()));
    h.ticks(5);
    let ConnectionState::SeveralKeyboards { devices } = h.status().connection else {
        panic!("{:?}", h.status().connection);
    };
    let (a, b) = (devices[0].id.clone(), devices[1].id.clone());
    h.op(|reply| Request::SelectDevice { id: a, reply })
        .unwrap();
    h.until_ready();
    let list = h.auto_backups();
    assert!(list.unidentified && list.connected_keyboard);
    assert!(h.recovery().unidentified);
    // Keyboard A looks reset (R3) and is held ...
    h.with_sim(|s| s.factory_reset_all());
    h.refresh().unwrap();
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    // ... then keyboard B, on another connection, whose light alone is at
    // its factory form (never an alarm on its own): A's hold is not B's.
    h.sim.with(1, |s| {
        s.set_table(
            profile(1),
            SimTable::MainLight,
            openwraith_hid::sim::FACTORY_LIGHT.to_vec(),
        );
    });
    h.op(|reply| Request::SelectDevice { id: b, reply })
        .unwrap();
    h.until_ready();
    assert!(h.recovery().unidentified);
    assert_eq!(h.status().recovery.reset_suspected, Vec::<u8>::new());
    let st = h.shared.known_good();
    assert!(st.own.is_empty());
    assert!(st.profiles.values().all(|c| c.held.is_none()));
}

/// A harness whose device lock lives in a temporary folder.
fn ready_with_lock() -> (Harness, std::path::PathBuf, tempfile::TempDir) {
    let folder = tempfile::tempdir().unwrap();
    let lock = folder.path().to_path_buf();
    let dir = lock.clone();
    let mut h = Harness::with_config(1, move |c| c.lock_dir = Some(dir));
    h.until_ready();
    (h, lock, folder)
}

/// The command line tool writes the factory record to every key under its
/// exclusive lock (`reset`: the keyboard also reports a reset meanwhile).
fn cli_writes_the_factory_preset(reset: bool) -> Harness {
    let (mut h, lock, _folder) = ready_with_lock();
    let cli = DeviceLock::try_exclusive(&lock, TOOL_CLI, "set trigger").unwrap();
    h.ticks(12);
    assert!(h.worker.paused_for_cli());
    h.with_sim(|s| {
        for key in W75
            .positions()
            .filter(|p| Some(p.index()) != W75.knob_index)
        {
            s.set_trigger(profile(1), key, FACTORY_TRIGGER_RECORD);
        }
    });
    if reset {
        h.sim.apply(SimAction::KeyboardReset).unwrap();
        h.ticks(2);
        assert_eq!(h.worker.conn, Conn::ResetSeen);
    }
    drop(cli);
    h.ticks(12);
    if reset {
        h.op(|reply| Request::Reconnect { reply }).unwrap();
    }
    h.until_ready();
    h
}

#[test]
fn the_command_line_tools_own_factory_writes_are_no_reset() {
    let h = cli_writes_the_factory_preset(false);
    assert!(reset_notices(&h).is_empty(), "{:?}", h.notices());
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    let files = h.auto_files(1);
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(files[0].contains("-auto-changed-elsewhere"), "{files:?}");
    // A reset heard during the pause still counts.
    let h = cli_writes_the_factory_preset(true);
    let notices = reset_notices(&h);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0].1[0], ResetEvidenceView::ResetSeen);
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
}

#[test]
fn back_up_now_saves_a_manual_backup_when_the_read_was_not_saved() {
    let mut h = Harness::ready();
    // A write that may be lost waits for a later connection: the read
    // after it is not saved by the comparison, "Back up now" saves it.
    let preview = h
        .prepare_trigger(h.trigger_intent(&[46], Some(60), None))
        .unwrap();
    let watch = h.watch();
    let sim = h.sim.clone();
    let mut done = false;
    watch.on_event(move |event, _| {
        if !done && matches!(event.step, ProgressStep::WaitingForKeyboardSave { .. }) {
            done = true;
            sim.apply(SimAction::KeyboardSwitch {
                profile: 1,
                after_ms: 300,
                report: true,
            })
            .unwrap();
        }
    });
    let _ = h.apply_trigger(&Harness::token(&preview.token), &watch);
    h.ticks(30);
    h.until_ready();
    // Whatever the keyboard kept, W differs from the known-good now.
    h.with_sim(|s| {
        let mut record = s.trigger(profile(1), super::harness::pos(46)).unwrap();
        record[1] = record[1].wrapping_add(7);
        s.set_trigger(profile(1), super::harness::pos(46), record);
    });
    h.refresh().unwrap();
    assert_eq!(h.auto_files(1).len(), 1, "not saved on this connection");
    let entry = h
        .op(|reply| Request::BackUpNow { reply })
        .unwrap()
        .expect("saved");
    assert_eq!(entry.reason, crate::dto::AutoBackupReason::Manual);
    assert!(h.auto_files(1)[0].contains("-auto-manual"));
}

#[test]
fn the_list_without_a_keyboard_read_shows_every_keyboard() {
    let h = Harness::ready();
    let root = h.worker.config.known_good_dir();
    let other = autobackup::profile_dir(&root, "unidentified", 2);
    let mut backup = crate::backend::sim::seed_backup();
    backup.created_at = "2026-10-20T07:30:00Z".to_string();
    autobackup::save(&other, &backup, autobackup::AutoReason::FirstRead, 10).unwrap();
    // Before any keyboard was read (a start with the keyboard unplugged).
    h.shared.known_good().device = None;
    let list = h.auto_backups();
    assert!(!list.connected_keyboard);
    assert!(list.unidentified);
    assert_eq!(list.entries.len(), 2);
    assert!(list.entries.iter().any(|e| e.profile == 2));
}

#[test]
fn composed_recoveries_keep_the_newest_five() {
    let dir = tempfile::tempdir().unwrap();
    let mut backup = crate::backend::sim::seed_backup();
    for day in 10..17 {
        backup.created_at = format!("2026-10-{day:02}T07:30:00Z");
        autobackup::save_recovery(dir.path(), "k", &backup).unwrap();
    }
    let left = std::fs::read_dir(dir.path().join("k").join("recovery"))
        .unwrap()
        .count();
    assert_eq!(left, 5);
}

#[test]
fn back_up_now_is_refused_while_automatic_backups_are_off() {
    let mut h = Harness::ready();
    let path = h.worker.config.settings_path.clone().unwrap();
    let off = crate::dto::BackgroundSettingsIntent {
        tray_enabled: false,
        start_with_windows: false,
        notifications: crate::dto::NotificationSettings::default(),
        auto_backup: crate::dto::AutoBackupSettings {
            enabled: false,
            keep: 10,
        },
    };
    crate::settings::set_background(&path, &off).unwrap();
    h.with_sim(|s| s.factory_reset_all());
    h.refresh().unwrap();
    assert!(h.op(|reply| Request::BackUpNow { reply }).is_err());
    assert_eq!(h.auto_files(1).len(), 1);
    // Back on: the reset is still found.
    let on = crate::dto::BackgroundSettingsIntent {
        auto_backup: crate::dto::AutoBackupSettings::default(),
        ..off
    };
    crate::settings::set_background(&path, &on).unwrap();
    h.refresh().unwrap();
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
}

#[test]
fn an_edit_saved_only_at_the_next_start_is_no_reset() {
    let mut h = Harness::ready();
    let keys: Vec<u8> = W75
        .positions()
        .map(|p| p.index())
        .filter(|i| Some(*i) != W75.knob_index)
        .collect();
    let intent = crate::dto::PresetEditIntent {
        preset: crate::dto::PresetRef::Builtin {
            id: "factory".to_string(),
        },
        keys,
        based_on: h.snapshot().revision,
    };
    let preview = h.prepare_preset(intent).unwrap();
    let report = h
        .apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done);
    assert_eq!(h.auto_files(1).len(), 1, "saved at the next complete read");
    // The app closes; the next start remembers nothing of this run.
    *h.shared.known_good() = Default::default();
    h.op(|reply| Request::Reconnect { reply }).unwrap();
    h.until_ready();
    assert!(reset_notices(&h).is_empty(), "{:?}", reset_notices(&h));
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    let files = h.auto_files(1);
    assert!(files[0].contains("-auto-after-write"), "{files:?}");
    // Saved: the record is gone.
    assert!(h.shared.known_good().own.is_empty());
    let key = autobackup::device_key(Some("W75HEARGB"), Some("SIM-1"));
    let own =
        autobackup::device_dir(&h.worker.config.known_good_dir(), &key).join("own-writes.json");
    assert!(!own.exists());
}

#[test]
fn restore_all_after_a_reset_keeps_the_socd_power_from_before_it() {
    let mut h = Harness::ready();
    let power =
        |h: &Harness| h.with_sim(|s| s.table(profile(1), SimTable::SocdSettings).map(|t| t[0]));
    // SOCD on with Fn + Right Shift; profiles 3 and 1 read with it on.
    h.with_sim(|s| s.press_socd_hotkey_at(std::time::Duration::from_millis(10)));
    h.ticks(5);
    h.refresh().unwrap();
    switch_to(&mut h, 3);
    switch_to(&mut h, 1);
    let on = power(&h);
    // A day later: Fn + Space (the power off), the user looks at profile 3.
    h.worker.config.fixed_time =
        Some(super::harness::fixed_time() + std::time::Duration::from_secs(86_400));
    reset_and_reconnect(&mut h);
    assert_ne!(power(&h), on);
    let files3 = h.auto_files(3);
    switch_to(&mut h, 3);
    // A second look at profile 3 (R1 no longer counts for it): its only
    // change is still the power at its reset form, which is not saved.
    h.refresh().unwrap();
    assert_eq!(h.auto_files(3), files3, "nothing saved for profile 3");
    switch_to(&mut h, 1);
    let view = h.recovery();
    let files: Vec<_> = view
        .profiles
        .iter()
        .filter_map(|p| p.backups.first().map(|b| b.file.token.clone()))
        .collect();
    let switches: Vec<u8> = view
        .profiles
        .iter()
        .filter(|p| !p.backups.is_empty() && p.profile != 1)
        .map(|p| p.profile)
        .collect();
    let preview = h
        .prepare_recovery(
            RecoveryIntent {
                files,
                allow_firmware_mismatch: false,
                confirmed_switches: switches,
            },
            &h.watch(),
        )
        .unwrap();
    let report = h
        .apply_restore(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(power(&h), on, "the power from before the reset");
}

// ----- own writes are checked against what they wrote -----

/// The `own-writes.json` of the simulator's keyboard.
fn own_writes_file(h: &Harness) -> std::path::PathBuf {
    let key = autobackup::device_key(Some("W75HEARGB"), Some("SIM-1"));
    autobackup::device_dir(&h.worker.config.known_good_dir(), &key).join("own-writes.json")
}

/// A typical user: own triggers, no remapped key (the keymap group at its
/// factory form), kept as the known-good settings. A reset then changes
/// only the triggers to their factory form.
fn custom_triggers_only() -> Harness {
    let mut h = Harness::ready();
    h.with_sim(|s| {
        for p in 1..=4 {
            for table in [
                SimTable::Keymap(Layer::Base),
                SimTable::Keymap(Layer::Fn),
                SimTable::AdvancedKeys,
            ] {
                let len = s.table(profile(p), table).map_or(0, <[u8]>::len);
                s.set_table(profile(p), table, vec![0; len]);
            }
        }
    });
    h.refresh().unwrap();
    h.keep_current(&[]).unwrap();
    h.refresh().unwrap();
    assert!(h.status().recovery.reset_suspected.is_empty());
    h
}

/// Key 46 at 0.60 mm: an edit that re-reads only that key.
fn edit_one_trigger(h: &mut Harness) {
    let preview = h
        .prepare_trigger(h.trigger_intent(&[46], Some(60), None))
        .unwrap();
    let report = h
        .apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done);
}

/// The app closes (this run's memory is gone), `change` happens to the
/// keyboard while it is closed, and the app starts again.
fn closed_while(h: &mut Harness, change: impl FnOnce(&mut openwraith_hid::DeviceSim)) {
    *h.shared.known_good() = Default::default();
    h.sim.apply(SimAction::Unplug).unwrap();
    h.tick();
    h.with_sim(change);
    h.sim.apply(SimAction::Replug).unwrap();
    h.until_ready();
}

#[test]
fn an_edit_not_yet_saved_when_the_app_closed_never_hides_a_reset_while_it_was_closed() {
    let mut h = custom_triggers_only();
    edit_one_trigger(&mut h);
    // Not saved yet (the edit re-read only its key): kept on disk.
    assert!(own_writes_file(&h).exists());
    let files = h.auto_files(1);
    // Fn + Space held while the app is closed.
    closed_while(&mut h, |s| s.factory_reset_all());
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    assert_eq!(
        reset_notices(&h),
        vec![(1, vec![ResetEvidenceView::TriggersToFactory])]
    );
    assert_eq!(h.auto_files(1), files, "the factory read is not saved");
}

#[test]
fn a_reset_by_another_program_after_an_edit_is_found() {
    let mut h = custom_triggers_only();
    edit_one_trigger(&mut h);
    let files = h.auto_files(1);
    // The vendor site is opened and resets the keyboard; "check again".
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 100,
        })
        .unwrap();
    h.ticks(5);
    assert!(matches!(h.worker.conn, Conn::OtherProgram { .. }));
    h.with_sim(|s| s.factory_reset_all());
    h.op(|reply| Request::RecheckOtherProgram { reply })
        .unwrap();
    h.until_ready();
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    assert_eq!(h.auto_files(1), files, "the factory read is not saved");
}

#[test]
fn restore_all_never_hides_a_reset_or_a_firmware_change_of_another_profile_while_closed() {
    for change in ["reset", "firmware"] {
        let mut h = custom_triggers_only();
        // Profile 2 with an own trigger too (W at 2.00 mm).
        h.with_sim(|s| {
            s.set_trigger(
                profile(2),
                super::harness::pos(46),
                [0x00, 100, 100, 0x01, 0x01, 0, 0, 0, 0],
            );
        });
        switch_to(&mut h, 2);
        h.keep_current(&[]).unwrap();
        h.refresh().unwrap();
        switch_to(&mut h, 1);
        // Profile 2 changed elsewhere since (not read): the restore writes it.
        h.with_sim(|s| {
            let mut r = s.trigger(profile(2), super::harness::pos(46)).unwrap();
            r[1] = r[1].wrapping_add(9);
            s.set_trigger(profile(2), super::harness::pos(46), r);
        });
        let view = h.recovery();
        let files: Vec<_> = view
            .profiles
            .iter()
            .filter_map(|p| p.backups.first().map(|b| b.file.token.clone()))
            .collect();
        let switches: Vec<u8> = view
            .profiles
            .iter()
            .filter(|p| !p.backups.is_empty() && p.profile != 1)
            .map(|p| p.profile)
            .collect();
        let preview = h
            .prepare_recovery(
                RecoveryIntent {
                    files,
                    allow_firmware_mismatch: false,
                    confirmed_switches: switches,
                },
                &h.watch(),
            )
            .unwrap();
        let report = h
            .apply_restore(&Harness::token(&preview.token), &h.watch())
            .unwrap();
        assert_eq!(report.headline, Headline::Done, "{change}: {report:?}");
        // Profile 2 was written but not read since: its record stays.
        assert!(own_writes_file(&h).exists(), "{change}");
        let files2 = h.auto_files(2);
        closed_while(&mut h, |s| {
            if change == "reset" {
                s.factory_reset_all();
            } else {
                // Another firmware with the same settings.
                let tables: Vec<_> = (1..=4u8)
                    .map(|p| {
                        let triggers: Vec<_> = W75
                            .positions()
                            .map(|k| (k, s.trigger(profile(p), k)))
                            .collect();
                        (p, triggers)
                    })
                    .collect();
                *s = super::seeds::firmware_sim("3.23.01");
                for (p, triggers) in tables {
                    for (k, record) in triggers {
                        if let Some(record) = record {
                            s.set_trigger(profile(p), k, record);
                        }
                    }
                }
            }
        });
        switch_to(&mut h, 2);
        assert!(
            h.status().recovery.reset_suspected.contains(&2),
            "{change}: {:?}",
            h.status().recovery
        );
        assert_eq!(h.auto_files(2), files2, "{change}: nothing saved");
    }
}

#[test]
fn a_recovery_restore_on_new_firmware_releases_the_hold() {
    let mut h = Harness::ready();
    h.sim.apply(SimAction::Unplug).unwrap();
    h.tick();
    h.with_sim(|s| {
        *s = super::seeds::firmware_sim("3.23.01");
        s.factory_reset_all();
    });
    h.sim.apply(SimAction::Replug).unwrap();
    h.until_ready();
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    assert!(h.status().recovery.firmware_changed);
    let notices = reset_notices(&h).len();
    let backup = h.recovery().profiles[0].backups[0].file.token.clone();
    let preview = h
        .prepare_recovery(
            RecoveryIntent {
                files: vec![backup],
                allow_firmware_mismatch: true,
                confirmed_switches: Vec::new(),
            },
            &h.watch(),
        )
        .unwrap();
    let report = h
        .apply_restore(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    h.refresh().unwrap();
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    assert_eq!(reset_notices(&h).len(), notices, "no new notice");
}

// ----- the report's banner -----

#[test]
fn the_report_goes_once_every_profile_with_backups_was_compared_and_nothing_is_held() {
    let mut h = Harness::ready();
    // Profile 2 at the factory forms, with an automatic backup of it.
    h.with_sim(|s| s.factory_reset_all());
    switch_to(&mut h, 2);
    assert_eq!(h.auto_files(2).len(), 1);
    switch_to(&mut h, 1);
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    // Profile 1 back to its settings by the recovery restore.
    let preview = recovery_of_profile_1(&mut h);
    let report = h
        .apply_restore(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done);
    h.refresh().unwrap();
    assert!(h.status().recovery.reset_suspected.is_empty());
    // Fn + Space: the report; profile 1 held.
    reset_and_reconnect(&mut h);
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    h.keep_current(&[1]).unwrap();
    assert!(
        h.status().recovery.reset_reported,
        "profile 2 not compared yet"
    );
    // "It's fine" with nothing held while a profile is not compared yet:
    // the report stays.
    h.keep_current(&[]).unwrap();
    assert!(h.status().recovery.reset_reported);
    // The user looks at profile 2 (fine): every profile was compared.
    switch_to(&mut h, 2);
    let status = h.status().recovery;
    assert_eq!(status, RecoveryStatusView::default(), "{status:?}");
    assert!(!h.recovery().attention);
    switch_to(&mut h, 1);
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
}
