//! The known-good store (automatic backups), reset detection and recovery
//! on the simulator (virtual time): promotion (first read, changes made
//! elsewhere, OpenWraith's own writes), holding after a reset (R1, R2, R3),
//! false positives, "It's fine, keep them", the recovery restore (one
//! profile and all profiles composed), rotation, pins, the settings that
//! steer it and the export options.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use openwraith_hid::known_good::FACTORY_TRIGGER_RECORD;
use openwraith_hid::protocol::{Layer, ReadRequest, W75};
use openwraith_hid::{Backup, SimTable};

use super::harness::{Harness, pos, profile};
use super::seeds;
use crate::autobackup::{self, AutoReason};
use crate::dto::{
    AppErrorKind, AppEvent, AutoBackupReason, AutoBackupSettings, BackgroundSettingsIntent,
    ExportIntent, ExportSource, FileToken, Headline, KnownGoodTableView, Notice,
    NotificationSettings, OpKind, PresetEditIntent, PresetRef, ProfileNamesIntent, ProgressStep,
    RecoveryIntent, RecoveryProfileState, RecoveryStatusView, ResetEvidenceView,
    ShareSelectionView, ShellChanges, SimAction, WindowsStartState,
};
use crate::handle::WorkerHandle;
use crate::sink::ShellHook;
use crate::worker::{Conn, Request};

const W: u8 = 46;

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

/// A reset of the keyboard (Fn + Space held): the factory forms, its
/// notification, then the user's reconnect and the read.
fn reset_and_reconnect(h: &mut Harness) {
    h.with_sim(|s| s.factory_reset_all());
    h.sim.apply(SimAction::KeyboardReset).unwrap();
    h.tick();
    assert_eq!(h.worker.conn, Conn::ResetSeen);
    h.op(|reply| Request::Reconnect { reply }).unwrap();
    h.until_ready();
}

/// The settings file of a harness.
fn settings_path(h: &Harness) -> std::path::PathBuf {
    h.worker.config.settings_path.clone().unwrap()
}

/// Background settings with automatic backups `enabled`, keeping `keep`.
fn auto_backup_settings(enabled: bool, keep: u8) -> BackgroundSettingsIntent {
    BackgroundSettingsIntent {
        tray_enabled: false,
        start_with_windows: false,
        notifications: NotificationSettings::default(),
        auto_backup: AutoBackupSettings { enabled, keep },
    }
}

/// Every table the restore writes, of every profile, and the triggers.
fn sim_tables(h: &Harness) -> Vec<Option<Vec<u8>>> {
    h.with_sim(|s| {
        let mut out = Vec::new();
        for p in 1..=4 {
            for table in [
                SimTable::MainLight,
                SimTable::CustomColors1,
                SimTable::Keymap(Layer::Base),
                SimTable::Keymap(Layer::Fn),
                SimTable::AdvancedKeys,
                SimTable::DeadBand,
                SimTable::SocdPairs,
            ] {
                out.push(s.table(profile(p), table).map(<[u8]>::to_vec));
            }
            for key in W75.positions() {
                out.push(s.trigger(profile(p), key).map(|r| r.to_vec()));
            }
        }
        out
    })
}

#[test]
fn the_first_complete_read_is_the_first_automatic_backup() {
    let mut h = Harness::ready();
    let files = h.auto_files(1);
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(
        files[0].ends_with("-auto-first-read.owbackup.json"),
        "{files:?}"
    );
    // Unchanged reads add nothing.
    h.refresh().unwrap();
    h.refresh().unwrap();
    assert_eq!(h.auto_files(1).len(), 1);
    let list = h.auto_backups();
    assert!(list.enabled && list.connected_keyboard);
    assert_eq!(list.keep, 10);
    assert_eq!(list.entries.len(), 1);
    let entry = &list.entries[0];
    assert_eq!(
        (entry.profile, entry.reason, entry.pinned),
        (1, AutoBackupReason::FirstRead, false)
    );
    assert_eq!(entry.created_at, "2026-10-24T07:30:00Z");
    assert_eq!(entry.product.as_deref(), Some("W75HEARGB"));
    let recovery = h.recovery();
    assert!(!recovery.attention && !recovery.reset_reported);
    assert_eq!(recovery.profiles.len(), 4);
    assert_eq!(recovery.profiles[0].state, RecoveryProfileState::Fine);
    assert_eq!(
        recovery.profiles[1].state,
        RecoveryProfileState::NoKnownGood
    );
    assert!(!recovery.can_restore_all);
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    // The file is a complete single-profile read of profile 1.
    let path = h
        .worker
        .config
        .known_good_dir()
        .join(autobackup::device_key(Some("W75HEARGB"), Some("SIM-1")))
        .join("profile-1")
        .join(&files[0]);
    let backup = Backup::load(&path).unwrap();
    assert!(!backup.is_all_profiles());
    assert_eq!(backup.active_profile, Some(1));
    // Reading only: nothing was written.
    assert_eq!(h.writes(), 0);
    h.assert_clean();
}

#[test]
fn changes_made_elsewhere_are_backed_up_at_once_and_never_alarm() {
    let mut h = Harness::ready();
    // The light back to its factory form, another dead band (the vendor
    // site): neither alone ever looks like a reset.
    h.with_sim(|s| {
        s.set_table(
            profile(1),
            SimTable::MainLight,
            openwraith_hid::sim::FACTORY_LIGHT.to_vec(),
        );
        s.set_table(profile(1), SimTable::DeadBand, vec![0x05; 264]);
    });
    h.refresh().unwrap();
    let files = h.auto_files(1);
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(files[0].contains("-auto-changed-elsewhere"), "{files:?}");
    // SOCD switched on the keyboard (one setting of the whole keyboard).
    h.with_sim(|s| s.press_socd_hotkey_at(Duration::from_millis(10)));
    h.ticks(5);
    h.refresh().unwrap();
    assert_eq!(h.auto_files(1).len(), 3);
    assert!(reset_notices(&h).is_empty(), "{:?}", h.notices());
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    assert_eq!(h.writes(), 0);
}

#[test]
fn the_factory_preset_written_by_openwraith_never_looks_like_a_reset() {
    let mut h = Harness::ready();
    let intent = PresetEditIntent {
        preset: PresetRef::Builtin {
            id: "factory".to_string(),
        },
        keys: vec![W],
        based_on: h.snapshot().revision,
    };
    let preview = h.prepare_preset(intent).unwrap();
    h.apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(
        h.with_sim(|s| s.trigger(profile(1), pos(W))),
        Some(FACTORY_TRIGGER_RECORD)
    );
    // The edit re-reads only W; the next complete read is backed up as the
    // write's result.
    h.refresh().unwrap();
    let files = h.auto_files(1);
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(files[0].contains("-auto-after-write"), "{files:?}");
    h.refresh().unwrap();
    assert_eq!(h.auto_files(1).len(), 2);
    assert!(reset_notices(&h).is_empty(), "{:?}", h.notices());
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
}

#[test]
fn factory_triggers_without_a_reset_report_still_look_reset() {
    let mut h = Harness::ready();
    let writes = h.writes();
    h.with_sim(|s| {
        for key in W75
            .positions()
            .filter(|p| Some(p.index()) != W75.knob_index)
        {
            s.set_trigger(profile(1), key, FACTORY_TRIGGER_RECORD);
        }
    });
    h.refresh().unwrap();
    assert_eq!(
        reset_notices(&h),
        vec![(1, vec![ResetEvidenceView::TriggersToFactory])]
    );
    assert_eq!(h.auto_files(1).len(), 1, "held, not backed up");
    let status = h.status().recovery;
    assert_eq!(status.reset_suspected, vec![1]);
    assert!(!status.reset_reported);
    assert_eq!(h.writes(), writes);
}

#[test]
fn a_reported_reset_holds_the_factory_tables_and_writes_nothing() {
    let mut h = Harness::ready();
    let writes = h.writes();
    reset_and_reconnect(&mut h);
    let notices = reset_notices(&h);
    assert_eq!(notices.len(), 1, "{notices:?}");
    let (p, evidence) = &notices[0];
    assert_eq!(*p, 1);
    assert_eq!(evidence[0], ResetEvidenceView::ResetSeen);
    assert!(evidence.contains(&ResetEvidenceView::TriggersToFactory));
    let status = h.status().recovery;
    assert_eq!(status.reset_suspected, vec![1]);
    assert!(status.reset_reported && !status.firmware_changed);
    // Nothing backed up, nothing written.
    assert_eq!(h.auto_files(1).len(), 1);
    assert_eq!(h.writes(), writes);
    let recovery = h.recovery();
    assert!(recovery.attention && recovery.reset_reported);
    assert!(recovery.reset_reported_at.is_some());
    let p1 = &recovery.profiles[0];
    assert_eq!(p1.state, RecoveryProfileState::ResetSuspected);
    assert!(p1.factory_now.contains(&KnownGoodTableView::Triggers));
    assert!(p1.factory_now.contains(&KnownGoodTableView::MainLight));
    assert_eq!(p1.backups.len(), 1);
    assert_eq!(p1.known_good_age_s, Some(0));
    assert!(!p1.firmware_differs);
    // Later reads keep holding it: no other notice, no backup, no write.
    h.refresh().unwrap();
    h.ticks(50);
    assert_eq!(reset_notices(&h).len(), 1);
    assert_eq!(h.auto_files(1).len(), 1);
    assert_eq!(h.writes(), writes);
    assert_eq!(h.status().recovery.reset_suspected, vec![1]);
    h.assert_clean();
}

#[test]
fn its_fine_keep_them_rebaselines() {
    let mut h = Harness::ready();
    reset_and_reconnect(&mut h);
    assert!(h.keep_current(&[5]).is_err(), "profiles are 1-4");
    let writes = h.writes();
    let view = h.keep_current(&[]).unwrap();
    assert!(!view.attention && !view.reset_reported);
    assert_eq!(view.profiles[0].state, RecoveryProfileState::Fine);
    let files = h.auto_files(1);
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(files[0].contains("-auto-kept-current"), "{files:?}");
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    // The factory forms are the known-good ones now.
    h.refresh().unwrap();
    assert_eq!(h.auto_files(1).len(), 2);
    assert_eq!(reset_notices(&h).len(), 1);
    assert_eq!(h.writes(), writes, "keeping sends nothing");
}

#[test]
fn a_recovery_is_the_standard_restore_after_the_review() {
    let mut h = Harness::ready();
    let before = sim_tables(&h);
    reset_and_reconnect(&mut h);
    let writes = h.writes();
    let backup = h.recovery().profiles[0].backups[0].file.token.clone();
    h.events.clear();
    let preview = h
        .prepare_recovery(
            RecoveryIntent {
                files: vec![backup],
                allow_firmware_mismatch: false,
                confirmed_switches: Vec::new(),
            },
            &h.watch(),
        )
        .unwrap();
    // The comparison only reads, as a recovery check.
    assert_eq!(h.writes(), writes);
    assert!(h.events.all().iter().any(|e| matches!(
        e,
        AppEvent::Status { status }
            if status.busy.as_ref().is_some_and(|b| b.kind == OpKind::RecoveryCheck)
    )));
    assert_eq!(preview.source.profiles, vec![1]);
    // Nothing is written until the user applies the plan.
    h.ticks(20);
    assert_eq!(h.writes(), writes);
    let report = h
        .apply_restore(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(sim_tables(&h)[..7 + 82], before[..7 + 82]);
    // The restore's safety backup (Undo) and a resolved reset.
    assert_eq!(h.safety_files().len(), 1);
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    h.refresh().unwrap();
    assert!(
        reset_notices(&h).is_empty(),
        "no new notice after the restore"
    );
    h.assert_clean();
}

#[test]
fn restore_all_profiles_composes_the_automatic_backups() {
    let mut h = Harness::ready();
    // Every profile read once (a switch reads the new profile).
    for p in [2u8, 3, 4, 1] {
        let preview = h.prepare_switch(p).unwrap();
        h.apply_switch(&Harness::token(&preview.token), &h.watch())
            .unwrap();
    }
    for p in 1..=4 {
        assert_eq!(h.auto_files(p).len(), 1, "profile {p}");
    }
    let before = sim_tables(&h);
    reset_and_reconnect(&mut h);
    assert_ne!(sim_tables(&h), before);
    let recovery = h.recovery();
    assert!(recovery.can_restore_all);
    let files: Vec<FileToken> = recovery
        .profiles
        .iter()
        .map(|p| p.backups[0].file.token.clone())
        .collect();
    // The comparison switches profiles only after the user agreed.
    let refused = h
        .prepare_recovery(
            RecoveryIntent {
                files: files.clone(),
                allow_firmware_mismatch: false,
                confirmed_switches: Vec::new(),
            },
            &h.watch(),
        )
        .unwrap_err();
    assert_eq!(refused.kind, AppErrorKind::NeedsSwitchConsent);
    let preview = h
        .prepare_recovery(
            RecoveryIntent {
                files,
                allow_firmware_mismatch: false,
                confirmed_switches: vec![2, 3, 4],
            },
            &h.watch(),
        )
        .unwrap();
    assert_eq!(preview.source.profiles, vec![1, 2, 3, 4]);
    let report = h
        .apply_restore(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(sim_tables(&h), before);
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    h.assert_clean();
}

#[test]
fn another_firmware_is_reported_and_held_until_kept() {
    let mut h = Harness::ready();
    h.sim.apply(SimAction::Unplug).unwrap();
    h.tick();
    h.with_sim(|s| *s = seeds::firmware_sim("3.23.01"));
    h.sim.apply(SimAction::Replug).unwrap();
    h.until_ready();
    let firmware: Vec<_> = h
        .notices()
        .into_iter()
        .filter_map(|n| match n {
            Notice::FirmwareChanged { known, current } => Some((known, current)),
            _ => None,
        })
        .collect();
    assert_eq!(firmware.len(), 1, "{:?}", h.notices());
    assert_eq!(firmware[0].0.version.as_deref(), Some("3.22.07"));
    assert_eq!(firmware[0].1.version.as_deref(), Some("3.23.01"));
    let notices = reset_notices(&h);
    assert_eq!(notices.len(), 1);
    assert!(matches!(
        notices[0].1.as_slice(),
        [ResetEvidenceView::FirmwareChanged { .. }]
    ));
    let status = h.status().recovery;
    assert!(status.firmware_changed);
    assert_eq!(status.reset_suspected, vec![1]);
    let recovery = h.recovery();
    assert!(recovery.firmware_changed.is_some());
    assert!(recovery.profiles[0].firmware_differs);
    h.keep_current(&[1]).unwrap();
    assert_eq!(h.status().recovery, RecoveryStatusView::default());
    h.refresh().unwrap();
    assert_eq!(reset_notices(&h).len(), 1);
}

#[test]
fn a_write_that_may_be_lost_is_never_backed_up_on_its_connection() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    // Fn + 2 on the keyboard while it waits to save: the write may be lost.
    let watch = h.watch();
    let sim = h.sim.clone();
    let mut done = false;
    watch.on_event(move |event, _| {
        if !done && matches!(event.step, ProgressStep::WaitingForKeyboardSave { .. }) {
            done = true;
            sim.apply(SimAction::KeyboardSwitch {
                profile: 2,
                after_ms: 300,
                report: true,
            })
            .unwrap();
        }
    });
    let report = h
        .apply_trigger(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert!(
        matches!(report.headline, Headline::MaybeLost | Headline::Lost),
        "{report:?}"
    );
    h.ticks(20);
    // Back to profile 1 (Fn + 1): read on the same connection, the write's
    // table is not taken as known-good, and never as evidence.
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 1,
            after_ms: 0,
            report: true,
        })
        .unwrap();
    assert!(h.until(|w| w.snapshot.as_ref().is_some_and(|s| s.profile == 1), 50));
    h.ticks(10);
    assert_eq!(h.auto_files(1).len(), 1, "not backed up on this connection");
    assert!(h.shared.known_good().own.contains_key(&1));
    // A later connection shows what the keyboard kept (here: nothing, the
    // switch discarded the write), and the write no longer counts.
    h.op(|reply| Request::Reconnect { reply }).unwrap();
    h.until_ready();
    assert!(h.shared.known_good().own.is_empty());
    assert!(reset_notices(&h).is_empty(), "{:?}", h.notices());
    h.assert_clean();
}

#[test]
fn pins_survive_the_rotation_and_safety_backups_are_never_touched() {
    let mut h = Harness::new(1);
    crate::settings::set_background(&settings_path(&h), &auto_backup_settings(true, 2)).unwrap();
    h.until_ready();
    let first = h.auto_backups().entries[0].file.token.clone();
    let list =
        crate::ops::recovery::pin_auto_backup(&h.shared, &h.worker.config, &first, true).unwrap();
    assert!(list.entries[0].pinned);
    // Three changes made elsewhere: the newest two and the pinned one stay.
    let safety_before = {
        let preview = h
            .prepare_trigger(h.trigger_intent(&[W], Some(70), None))
            .unwrap();
        h.apply_trigger(&Harness::token(&preview.token), &h.watch())
            .unwrap();
        h.safety_files()
    };
    for brightness in [1u8, 2, 3] {
        h.with_sim(|s| {
            let mut light = s.table(profile(1), SimTable::MainLight).unwrap().to_vec();
            light[1] = brightness;
            s.set_table(profile(1), SimTable::MainLight, light);
        });
        h.refresh().unwrap();
    }
    let list = h.auto_backups();
    assert_eq!(list.entries.len(), 3, "{:?}", list.entries);
    assert_eq!(list.entries.iter().filter(|e| e.pinned).count(), 1);
    assert_eq!(list.entries[2].file.token, first);
    assert_eq!(h.safety_files(), safety_before, "safety backups untouched");
    // Unpinned, it is an ordinary automatic backup again.
    crate::ops::recovery::pin_auto_backup(&h.shared, &h.worker.config, &first, false).unwrap();
    assert!(h.auto_backups().entries.iter().all(|e| !e.pinned));
    // Only automatic backups can be pinned.
    let other = h.user_file(h.safety_files()[0].clone());
    assert_eq!(
        crate::ops::recovery::pin_auto_backup(&h.shared, &h.worker.config, &other, true)
            .unwrap_err()
            .kind,
        AppErrorKind::FileUnknown
    );
}

#[test]
fn rotation_keeps_the_newest_and_only_touches_its_own_files() {
    let dir = tempfile::tempdir().unwrap();
    let profile_dir = dir.path().join("known-good").join("k").join("profile-1");
    let safety = dir.path().join("safety");
    std::fs::create_dir_all(&safety).unwrap();
    let safety_file = safety.join("openwraith-W75HEARGB-20261024-073000-set-trigger.owbackup.json");
    std::fs::write(&safety_file, "{}").unwrap();
    std::fs::create_dir_all(&profile_dir).unwrap();
    let foreign = [
        "notes.txt",
        "openwraith-W75HEARGB-20261020-073000-set-trigger.owbackup.json",
    ];
    for name in foreign {
        std::fs::write(profile_dir.join(name), "{}").unwrap();
    }
    let mut backup = crate::backend::sim::seed_backup();
    let mut save = |day: u8, reason: AutoReason| {
        backup.created_at = format!("2026-10-{day:02}T07:30:00Z");
        autobackup::save(&profile_dir, &backup, reason, 2)
            .unwrap()
            .0
    };
    save(10, AutoReason::FirstRead);
    save(11, AutoReason::ChangedElsewhere);
    let oldest_kept = save(12, AutoReason::AfterWrite);
    let names = |dir: &std::path::Path| -> Vec<String> {
        autobackup::list_profile(dir)
            .unwrap()
            .into_iter()
            .map(|f| f.file_name)
            .collect()
    };
    assert_eq!(names(&profile_dir).len(), 2);
    let pinned = oldest_kept
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    autobackup::set_pinned(&profile_dir, &pinned, true).unwrap();
    save(13, AutoReason::ChangedElsewhere);
    save(14, AutoReason::KeptCurrent);
    save(15, AutoReason::ChangedElsewhere);
    let left = names(&profile_dir);
    assert_eq!(left.len(), 3, "{left:?}");
    assert!(left[0].contains("20261015") && left[1].contains("20261014"));
    assert_eq!(left[2], pinned);
    // Same time stamps keep their order by number.
    backup.created_at = "2026-10-16T07:30:00Z".to_string();
    let (a, _) = autobackup::save(&profile_dir, &backup, AutoReason::AfterWrite, 5).unwrap();
    let (b, _) = autobackup::save(&profile_dir, &backup, AutoReason::ChangedElsewhere, 5).unwrap();
    let listed = names(&profile_dir);
    assert_eq!(listed[0], b.file_name().unwrap().to_string_lossy());
    assert_eq!(listed[1], a.file_name().unwrap().to_string_lossy());
    for name in foreign {
        assert!(profile_dir.join(name).is_file(), "{name} untouched");
    }
    assert!(safety_file.is_file());
}

#[test]
fn automatic_backups_off_means_no_backup_and_no_comparison() {
    let mut h = Harness::new(1);
    crate::settings::set_background(&settings_path(&h), &auto_backup_settings(false, 10)).unwrap();
    h.until_ready();
    assert!(h.auto_files(1).is_empty());
    reset_and_reconnect(&mut h);
    assert!(reset_notices(&h).is_empty());
    assert!(h.auto_files(1).is_empty());
    let list = h.auto_backups();
    assert!(!list.enabled && list.entries.is_empty());
    assert!(!h.recovery().auto_backup_enabled);
}

#[test]
fn exports_take_the_chosen_parts_and_automatic_backups_as_a_source() {
    let mut h = Harness::ready();
    let packets = h.packets();
    let auto = h.auto_backups().entries[0].file.token.clone();
    let destination = h.destination("p1.owprofile.json");
    let include = ShareSelectionView {
        triggers: true,
        lighting: true,
        keys: true,
        dead_band: false,
        socd: false,
    };
    let intent = |include| ExportIntent {
        source: ExportSource::File { file: auto.clone() },
        profile: 1,
        label: Some("From an automatic backup".to_string()),
        destination: destination.token.clone(),
        confirmed_switch: false,
        include,
    };
    let exported = h.export(intent(Some(include)), &h.watch()).unwrap();
    assert_eq!(h.packets(), packets, "a file source sends nothing");
    let present = exported.contents.present;
    assert!(present.triggers && present.lighting && present.keys);
    assert!(!present.dead_band && !present.socd);
    let file = Backup::load(&h.dir.path().join("p1.owprofile.json")).unwrap();
    assert!(file.response(ReadRequest::DeadBand).is_none());
    assert!(file.response(ReadRequest::MainLight).is_some());
    assert!(file.note.contains("Parts:"), "{}", file.note);
    // Everything when nothing is chosen (as before), nothing refused.
    let all = h.export(intent(None), &h.watch()).unwrap();
    assert!(all.contents.present.dead_band);
    let none = ShareSelectionView {
        triggers: false,
        lighting: false,
        keys: false,
        dead_band: false,
        socd: false,
    };
    let error = h.export(intent(Some(none)), &h.watch()).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::InvalidRequest);
    assert_eq!(h.packets(), packets);
}

/// Records what the shell was asked to change; a failing one has no entry.
#[derive(Default)]
struct Shell(Mutex<Vec<ShellChanges>>, bool);

impl ShellHook for Shell {
    fn apply(&self, changes: &ShellChanges) -> Result<(), crate::dto::AppError> {
        self.0.lock().unwrap().push(*changes);
        if self.1 {
            Err(crate::dto::AppError::new(AppErrorKind::Io, "no Run key"))
        } else {
            Ok(())
        }
    }

    fn windows_start(&self, wanted: bool) -> WindowsStartState {
        match (wanted, self.1) {
            (false, _) => WindowsStartState::Off,
            (true, false) => WindowsStartState::On,
            (true, true) => WindowsStartState::EntryMissing,
        }
    }
}

fn handle_of(h: &Harness) -> WorkerHandle {
    WorkerHandle::from_parts(
        h.tx.clone(),
        Arc::clone(&h.shared),
        h.worker.config.clone(),
        None,
        Some(h.sim.clone()),
    )
}

#[test]
fn background_settings_are_validated_saved_and_handed_to_the_shell() {
    let h = Harness::ready();
    let handle = handle_of(&h);
    let defaults = handle.background_settings();
    assert!(!defaults.tray_enabled && !defaults.start_with_windows);
    assert!(defaults.notifications.reset && !defaults.notifications.auto_switch);
    assert!(defaults.auto_backup.enabled);
    assert_eq!(defaults.auto_backup.keep, 10);
    // Without a shell nobody can tell what Windows will do.
    assert_eq!(defaults.windows_start, WindowsStartState::Unavailable);
    let shell = Arc::new(Shell::default());
    handle.set_shell_hook(shell.clone());
    assert_eq!(
        handle.background_settings().windows_start,
        WindowsStartState::Off
    );
    let mut intent = auto_backup_settings(true, 5);
    intent.tray_enabled = true;
    intent.start_with_windows = true;
    let applied = handle.set_background_settings(&intent).unwrap();
    assert!(applied.settings.tray_enabled && applied.settings.start_with_windows);
    assert_eq!(applied.settings.auto_backup.keep, 5);
    // Read after the change, and again with every read of the settings.
    assert_eq!(applied.settings.windows_start, WindowsStartState::On);
    assert_eq!(
        handle.background_settings().windows_start,
        WindowsStartState::On
    );
    assert_eq!(
        applied.shell,
        ShellChanges {
            tray: Some(true),
            start_with_windows: Some(true),
        }
    );
    assert!(applied.shell_error.is_none());
    // Without the tray there is no start with Windows.
    intent.tray_enabled = false;
    let applied = handle.set_background_settings(&intent).unwrap();
    assert!(!applied.settings.start_with_windows);
    assert_eq!(applied.settings.windows_start, WindowsStartState::Off);
    assert_eq!(
        applied.shell,
        ShellChanges {
            tray: Some(false),
            start_with_windows: Some(false),
        }
    );
    // Nothing changed for the shell: not called.
    handle.set_background_settings(&intent).unwrap();
    assert_eq!(shell.0.lock().unwrap().len(), 2);
    // Out of range: refused, nothing saved.
    let error = handle
        .set_background_settings(&auto_backup_settings(true, 0))
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::InvalidRequest);
    assert_eq!(handle.background_settings().auto_backup.keep, 5);
    // A shell that fails: reported, the settings stay saved.
    let failing = Arc::new(Shell(Mutex::new(Vec::new()), true));
    handle.set_shell_hook(failing);
    intent.tray_enabled = true;
    let applied = handle.set_background_settings(&intent).unwrap();
    assert!(applied.shell_error.is_some());
    // The lasting state says so too.
    assert_eq!(
        applied.settings.windows_start,
        WindowsStartState::EntryMissing
    );
    assert!(handle.background_settings().tray_enabled);
    handle.mark_close_to_tray_explained().unwrap();
    assert!(handle.background_settings().close_to_tray_explained);
    // The other settings of the file are kept.
    crate::settings::save_settings(
        &settings_path(&h),
        &crate::dto::Settings {
            language: crate::dto::LanguageSetting::Tr,
            theme: crate::dto::ThemeSetting::Dark,
        },
    )
    .unwrap();
    assert!(handle.background_settings().tray_enabled);
}

#[test]
fn profile_names_are_local_per_product() {
    let h = Harness::ready();
    let handle = handle_of(&h);
    let names = handle.profile_names();
    assert_eq!(names.current_product.as_deref(), Some("W75HEARGB"));
    assert_eq!(names.products.len(), 1);
    assert_eq!(names.products[0].names, vec![None; 4]);
    let view = handle
        .set_profile_names(&ProfileNamesIntent {
            product: "W75HEARGB".to_string(),
            names: vec![
                None,
                Some("  FPS\u{7}  ".to_string()),
                Some(String::new()),
                Some("x".repeat(40)),
            ],
        })
        .unwrap();
    assert_eq!(
        view.products[0].names,
        vec![None, Some("FPS".to_string()), None, Some("x".repeat(32))]
    );
    assert_eq!(h.recovery().profiles[1].name.as_deref(), Some("FPS"));
    for bad in [
        ProfileNamesIntent {
            product: "W75 HEARGB".to_string(),
            names: vec![None; 4],
        },
        ProfileNamesIntent {
            product: "W75HEARGB".to_string(),
            names: vec![None; 3],
        },
    ] {
        assert_eq!(
            handle.set_profile_names(&bad).unwrap_err().kind,
            AppErrorKind::InvalidRequest
        );
    }
    // No names: the product is forgotten (the current one is still listed).
    let view = handle
        .set_profile_names(&ProfileNamesIntent {
            product: "W75HEARGB".to_string(),
            names: vec![None; 4],
        })
        .unwrap();
    assert_eq!(view.products[0].names, vec![None; 4]);
}

#[test]
fn per_game_rules_are_validated_and_stored() {
    use crate::dto::{AutoProfileRule, AutoProfileSettings};
    let h = Harness::ready();
    let handle = handle_of(&h);
    assert_eq!(
        handle.auto_profile_settings(),
        AutoProfileSettings::default()
    );
    let rule = AutoProfileRule {
        id: "r-1".to_string(),
        label: " Game\u{1b} ".to_string(),
        exe: "game.exe".to_string(),
        path_hint: Some(r"C:\Games".to_string()),
        profile: 2,
        enabled: true,
    };
    let settings = AutoProfileSettings {
        enabled: true,
        default_profile: Some(1),
        rules: vec![rule.clone()],
    };
    let saved = handle.save_auto_profile_settings(&settings).unwrap();
    assert_eq!(saved.rules[0].label, "Game");
    assert_eq!(handle.auto_profile_settings(), saved);
    for bad in [
        AutoProfileRule {
            exe: r"C:\Games\game.exe".to_string(),
            ..rule.clone()
        },
        AutoProfileRule {
            profile: 5,
            ..rule.clone()
        },
        AutoProfileRule {
            id: String::new(),
            ..rule.clone()
        },
    ] {
        let settings = AutoProfileSettings {
            rules: vec![bad],
            ..settings.clone()
        };
        assert!(handle.save_auto_profile_settings(&settings).is_err());
    }
    let twice = AutoProfileSettings {
        rules: vec![rule.clone(), rule],
        ..settings
    };
    assert!(handle.save_auto_profile_settings(&twice).is_err());
}

#[test]
fn an_unreadable_settings_file_is_kept_aside_with_a_notice() {
    let dir = tempfile::tempdir().unwrap();
    let config = super::harness::test_config(dir.path());
    let path = config.settings_path.clone().unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ not json").unwrap();
    let handle = WorkerHandle::spawn_sim(
        config,
        Arc::new(crate::sink::recorder::ExitCounter::default()),
        crate::backend::sim::SimOptions::virtual_time(0),
    )
    .unwrap();
    let events = Arc::new(crate::sink::recorder::Recorder::<AppEvent>::default());
    handle.subscribe(events.clone());
    let kept: Vec<_> = events
        .all()
        .into_iter()
        .filter_map(|e| match e {
            AppEvent::Notice {
                notice: Notice::SettingsFileInvalid { kept_as },
            } => Some(kept_as),
            _ => None,
        })
        .collect();
    assert_eq!(kept.len(), 1);
    let name = kept[0].clone().unwrap();
    assert!(name.starts_with("settings.invalid-"), "{name}");
    assert!(path.parent().unwrap().join(&name).is_file());
    assert!(!path.exists(), "the defaults are used");
    // Told once.
    let again = Arc::new(crate::sink::recorder::Recorder::<AppEvent>::default());
    handle.subscribe(again.clone());
    assert!(!again.all().iter().any(|e| matches!(
        e,
        AppEvent::Notice {
            notice: Notice::SettingsFileInvalid { .. }
        }
    )));
    handle.shutdown();
}
