//! Restore against the device simulator: planning, safety backup,
//! execution, verification and failure handling. No HID device is opened.

mod common;

use std::ops::ControlFlow;

use common::{
    C06, C10, c06_backup, captured_status_report, consent, fixed_options, load, pos, profile,
    profiles_options, report, selects, sim_with_profiles, temp_dir, write_bytes,
};
use openwraith_hid::protocol::{
    CustomTable, Layer, MacroSlotId, ProfileId, RawResponse, ReadRequest, Report, TableId, W75,
};
use openwraith_hid::sim::FACTORY_TRIGGER;
use openwraith_hid::{
    Backup, BackupEntry, BlockCause, CommitSettle, CommitTriggerReason, DeviceSim, ErrorKind,
    ItemResult, KeymapCommit, Outcome, Persistence, PersistenceCheck, PlanAction, PlanWarning,
    PreparedRestore, ProfileSet, RestoreError, RestoreOptions, RestorePlan, RestoreReport,
    SavedBackup, Scope, ScopeConfig, Session, SimTable, SkipReason, WriteOptions, WriteProgress,
    WriteWarning, dry_run_restore, execute_restore, plan_restore, prepare_restore, read_all,
    read_all_profiles,
};

/// c13 macro slot 0: "A down 100 ms, A up 100 ms, B down 100 ms, B up".
fn c13_macro() -> Vec<u8> {
    let mut bytes = vec![0u8; 256];
    bytes[..24].copy_from_slice(&[
        0x00, 0x00, 0x00, 0x10, 0x00, 0x01, 0x00, 0x00, 0x04, 0x10, 0x00, 0x64, 0x04, 0x00, 0x00,
        0x64, 0x05, 0x10, 0x00, 0x64, 0x05, 0x00, 0x00, 0x00,
    ]);
    bytes
}

fn slot(n: u8) -> MacroSlotId {
    MacroSlotId::new(n).expect("slot")
}

/// Overwrites bytes of a simulator table.
fn patch(sim: &mut DeviceSim, p: ProfileId, table: SimTable, offset: usize, bytes: &[u8]) {
    let mut data = sim.table(p, table).expect("table").to_vec();
    data[offset..offset + bytes.len()].copy_from_slice(bytes);
    sim.set_table(p, table, data);
}

/// Profile 1 gets a keymap remap (W -> A), the c13 macro bound to Caps, a
/// dead band on W and a blue colour slot, on top of c06.
fn customise(sim: &mut DeviceSim) {
    let p1 = profile(1);
    patch(
        sim,
        p1,
        SimTable::Keymap(Layer::Base),
        46 * 4,
        &[0x00, 0x04, 0x00, 0x00],
    );
    patch(
        sim,
        p1,
        SimTable::Keymap(Layer::Base),
        66 * 4,
        &[0x10, 0x39, 0x00, 0x00],
    );
    sim.set_table(p1, SimTable::Macro(slot(0)), c13_macro());
    patch(sim, p1, SimTable::DeadBand, 92, &[0x05, 0x0a]);
    patch(sim, p1, SimTable::CustomColors1, 30 * 3, &[0, 0, 0xff]);
}

/// An all-profile backup of the simulator.
fn snapshot(sim: &mut DeviceSim) -> Backup {
    let mut session = Session::new(sim);
    read_all_profiles(&mut session, consent(), &profiles_options(), |_| {
        ControlFlow::Continue(())
    })
    .expect("all-profile backup")
}

fn options() -> RestoreOptions {
    RestoreOptions {
        backup: fixed_options(),
        ..RestoreOptions::default()
    }
}

fn prepare(sim: &mut DeviceSim, target: &Backup, options: &RestoreOptions) -> PreparedRestore {
    let mut session = Session::new(sim);
    prepare_restore(&mut session, target, options, consent(), |_| {
        ControlFlow::Continue(())
    })
    .expect("prepared")
}

fn execute(sim: &mut DeviceSim, prepared: &PreparedRestore, saved: &SavedBackup) -> RestoreReport {
    let mut session = Session::new(sim);
    let report = execute_restore(&mut session, prepared, saved, consent(), |_| {
        ControlFlow::Continue(())
    });
    assert!(
        session.foreign_activity().is_quiet() || !report.is_success(),
        "{:?}",
        session.foreign_activity()
    );
    report
}

/// Prepare, save the safety backup, execute.
fn restore(sim: &mut DeviceSim, target: &Backup, options: &RestoreOptions) -> RestoreReport {
    let prepared = prepare(sim, target, options);
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety backup");
    execute(sim, &prepared, &saved)
}

/// The declared bytes of a reply (table bytes, or the payload).
fn content(raw: &RawResponse) -> Vec<u8> {
    raw.table_bytes("test").unwrap_or_else(|_| raw.payload())
}

fn empty(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0) || bytes.iter().all(|b| *b == 0xFF)
}

/// Every restorable table of every profile in `target` equals the
/// simulator's state (macro slots: empty equals empty).
fn assert_matches(sim: &DeviceSim, target: &Backup) {
    for n in target.profile_numbers() {
        let p = profile(n);
        let view = target.snapshot(p).expect("snapshot");
        let mut tables = vec![
            SimTable::MainLight,
            SimTable::CustomColors1,
            SimTable::Keymap(Layer::Base),
            SimTable::Keymap(Layer::Fn),
            SimTable::AdvancedKeys,
            SimTable::DeadBand,
            SimTable::SocdPairs,
            SimTable::SocdSettings,
        ];
        tables.extend(MacroSlotId::all().map(SimTable::Macro));
        for table in tables {
            let request = match table {
                SimTable::MainLight => ReadRequest::MainLight,
                SimTable::CustomColors1 => ReadRequest::CustomColors(CustomTable::Custom1),
                SimTable::Keymap(layer) => ReadRequest::Keymap(layer),
                SimTable::Macro(s) => ReadRequest::Macro(s),
                SimTable::AdvancedKeys => ReadRequest::AdvancedKeys,
                SimTable::DeadBand => ReadRequest::DeadBand,
                SimTable::SocdPairs => ReadRequest::SocdPairs,
                SimTable::SocdSettings => ReadRequest::SocdSettings,
            };
            let expected = content(view.response(request).expect("reply"));
            let got = sim.table(p, table).expect("sim table");
            if matches!(table, SimTable::Macro(_)) && empty(&expected) {
                assert!(empty(got), "profile {n} {table:?}");
            } else {
                assert_eq!(got, expected.as_slice(), "profile {n} {table:?}");
            }
        }
        let knob = W75.knob_index;
        for key in W75.positions().filter(|k| Some(k.index()) != knob) {
            let expected = view
                .response(ReadRequest::KeyTrigger(key))
                .map(|r| r.payload()[1..10].to_vec());
            let got = sim.trigger(p, key).map(|r| r.to_vec());
            assert_eq!(got, expected, "profile {n} key {key}");
        }
    }
}

fn written_tables(report: &RestoreReport, profile_number: u8) -> Vec<TableId> {
    report
        .items()
        .filter(|(p, i)| *p == profile_number && matches!(i.result, ItemResult::Written { .. }))
        .map(|(_, i)| i.table)
        .collect()
}

#[test]
fn full_round_trip_of_all_profiles() {
    let mut sim = sim_with_profiles(ScopeConfig::uniform(Scope::PerProfile));
    customise(&mut sim);
    let target = snapshot(&mut sim);
    let before_writes = sim.writes_sent();

    // Wipe every profile to factory state, then restore.
    sim.factory_reset_all();
    let options = options();
    let prepared = prepare(&mut sim, &target, &options);
    assert_eq!(prepared.original_profile(), profile(1));
    assert!(prepared.safety().is_all_profiles());
    let plan = prepared.plan();
    assert!(plan.has_writes());
    assert!(plan.not_compared().is_empty());
    // Plans are serializable for display.
    let json = serde_json::to_value(plan).expect("plan serializes");
    assert_eq!(json["profiles"][0]["profile"], 1);

    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety backup");
    assert!(saved.path().starts_with(dir.path()));
    assert!(saved.describes(prepared.safety()));
    assert_eq!(saved.profiles(), &[1, 2, 3, 4]);
    assert!(saved.undo_command().starts_with("openwraith restore "));
    assert_eq!(
        &Backup::load(saved.path()).expect("loads"),
        prepared.safety()
    );

    let report = execute(&mut sim, &prepared, &saved);
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(report.outcome, Outcome::Completed);
    assert_eq!(report.active_profile_now, Some(1));
    assert_eq!(sim.active_profile(), 1);
    assert!(report.anything_written());
    assert!(report.items().all(|(_, i)| matches!(
        i.result,
        ItemResult::Written { .. } | ItemResult::AlreadyMatched
    )));
    assert!(report.foreign_activity.is_none());
    let json = serde_json::to_value(&report).expect("report serializes");
    assert_eq!(json["outcome"], "completed");

    assert_matches(&sim, &target);
    // The final check reads the SOCD power again after it was written in
    // another profile; the power did not change here.
    assert_eq!(report.final_check, None);

    // Profile 1, in execution order: triggers, dead band, the macro (before
    // the keymap that binds it), the base keymap, colours, light.
    let p1 = written_tables(&report, 1);
    let pos_of = |t: TableId| p1.iter().position(|x| *x == t).expect("written");
    assert!(pos_of(TableId::Triggers) < pos_of(TableId::DeadBand));
    assert!(pos_of(TableId::DeadBand) < pos_of(TableId::Macro { slot: slot(0) }));
    assert!(
        pos_of(TableId::Macro { slot: slot(0) }) < pos_of(TableId::Keymap { layer: Layer::Base })
    );
    assert!(
        pos_of(TableId::Keymap { layer: Layer::Base })
            < pos_of(TableId::CustomColors {
                table: CustomTable::Custom1
            })
    );
    assert_eq!(p1.last(), Some(&TableId::MainLight));

    // The light changed mode (factory 02 -> custom 0a): leave, then light.
    let writes = write_bytes(&sim);
    let restored: Vec<_> = writes[before_writes..].to_vec();
    let leave = restored.iter().position(|b| b[0] == 0x17).expect("leave");
    assert_eq!(restored[leave + 1][0], 0x07);

    // Running the restore again finds nothing to do.
    let again = restore(&mut sim, &target, &options);
    assert_eq!(again.outcome, Outcome::NothingToDo);
    sim.assert_clean();
}

#[test]
fn restoring_one_profile_from_another_one() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    // A single-profile backup of profile 1.
    let target = {
        let mut session = Session::new(&mut sim);
        openwraith_hid::read_all_with(&mut session, &fixed_options(), |_| {}).expect("backup")
    };
    assert_eq!(target.active_profile, Some(1));
    sim.factory_reset_all();
    let profile2_light = sim
        .table(profile(2), SimTable::MainLight)
        .map(<[u8]>::to_vec);
    // The keyboard is on profile 2.
    sim.set_active_profile(profile(2));
    let selects_before = selects(&sim).len();

    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(report.original_profile, 2);
    assert_eq!(report.profiles.len(), 1);
    assert_eq!(report.profiles[0].profile, 1);
    // Profile 1 restored, profile 2 untouched, back on profile 2.
    assert_matches(&sim, &target);
    assert_eq!(
        sim.table(profile(2), SimTable::MainLight)
            .map(<[u8]>::to_vec),
        profile2_light
    );
    assert_eq!(sim.active_profile(), 2);
    // Safety backup: 1 then back to 2; restore: 1; persistence check: away
    // to 2 and 1 again (read back); then back to 2.
    assert_eq!(selects(&sim)[selects_before..], [1, 2, 1, 2, 1, 2]);
    sim.assert_clean();
}

/// Q at 2.00 mm, Rapid Trigger off.
const Q_2_00: [u8; 9] = [0x00, 100, 100, 0x01, 0x01, 0, 0, 0, 0];

/// The scenario of hardware test H7 that failed on the real keyboard
/// (2026-10-24, PROTOCOL.md 4.5.3 "Delayed commit"): on profile 1,
/// `restore all.owbackup.json --profile 2` wrote Q in profile 2, verified it
/// by reading it back, and selected profile 1 again within milliseconds. The
/// keyboard had not saved the write yet and dropped it.
#[test]
fn restoring_another_profile_survives_the_switch_back() {
    let q = pos(2, 1);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let target = snapshot(&mut sim);
    // Later, Q of profile 2 was changed (and saved long ago).
    sim.set_trigger(profile(2), q, Q_2_00);
    assert_eq!(sim.active_profile(), 1);
    let options = RestoreOptions {
        profiles: Some(ProfileSet::only(profile(2))),
        ..options()
    };
    let first_restore_packet = sim.written().len();
    let selects_before = selects(&sim).len();
    let report = restore(&mut sim, &target, &options);
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(report.active_profile_now, Some(1));
    assert_eq!(sim.active_profile(), 1);
    // The keyboard kept the write: no unsaved edit was dropped, and profile
    // 2 holds the backup's Q after the switch back.
    assert_eq!(sim.discarded_edits(), 0);
    assert_eq!(sim.persisted_trigger(profile(2), q), Some(FACTORY_TRIGGER));

    // The report says so: written and verified, then read again after a
    // profile switch.
    let (_, item) = report.items().next().expect("Q");
    assert_eq!(item.key, Some(q));
    assert!(
        matches!(
            item.result,
            ItemResult::Written {
                persistence: Persistence::Persisted,
                ..
            }
        ),
        "{:?}",
        item.result
    );
    assert_eq!(
        report.persistence,
        Some(PersistenceCheck {
            profiles: vec![2],
            persisted: 1,
            lost: 0,
            not_checked: 0
        })
    );
    // Safety backup 2, 1; restore 2; persistence check: away to 1 (after
    // the settle) and back to 2; return to 1.
    assert_eq!(selects(&sim)[selects_before..], [2, 1, 2, 1, 2, 1]);
    // No select left the written profile before the settle had passed.
    let packets = &sim.written()[first_restore_packet..];
    let times = &sim.sent_times()[first_restore_packet..];
    let write = packets
        .iter()
        .position(|p| p.is_write() && p.as_bytes()[..5] == [0x21, 0, 0, 0, 0x18])
        .expect("the trigger write");
    let select = write
        + packets[write..]
            .iter()
            .position(|p| p.is_write() && p.as_bytes()[..6] == [0x21, 0, 0, 0, 0, 0x12])
            .expect("a select after the write");
    assert!(times[select] - times[write] >= std::time::Duration::from_secs(2));
    sim.assert_clean();
}

#[test]
fn without_the_settle_the_persistence_check_reports_the_lost_write() {
    // Exactly the hardware failure, reproduced with the test-only option
    // that turns the commit settle off.
    let q = pos(2, 1);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let target = snapshot(&mut sim);
    sim.set_trigger(profile(2), q, Q_2_00);
    let options = RestoreOptions {
        profiles: Some(ProfileSet::only(profile(2))),
        write: WriteOptions {
            commit_settle: CommitSettle::disabled_for_tests(),
            ..WriteOptions::default()
        },
        ..options()
    };
    let report = restore(&mut sim, &target, &options);
    // Written and verified (the read-back reads the working copy), but gone
    // after the switch away and back.
    assert_eq!(report.outcome, Outcome::Failed);
    let error = report.error.as_ref().expect("error");
    assert_eq!(error.kind, ErrorKind::NotPersisted);
    assert!(error.message.contains("profile 2: 1 table(s)"), "{error:?}");
    assert!(
        error.message.contains("Run the same restore again"),
        "{error:?}"
    );
    let (_, item) = report.items().next().expect("Q");
    match &item.result {
        ItemResult::Lost {
            sends, mismatch, ..
        } => {
            assert_eq!(*sends, 1);
            assert_eq!(mismatch.table, TableId::Triggers);
        }
        other => panic!("not lost: {other:?}"),
    }
    assert_eq!(report.lost(), 1);
    assert_eq!(report.written(), 0);
    assert_eq!(
        report.persistence,
        Some(PersistenceCheck {
            profiles: vec![2],
            persisted: 0,
            lost: 1,
            not_checked: 0
        })
    );
    assert!(sim.discarded_edits() >= 1);
    assert_eq!(sim.persisted_trigger(profile(2), q), Some(Q_2_00));
    // Still back on the original profile.
    assert_eq!(report.active_profile_now, Some(1));
    assert_eq!(sim.active_profile(), 1);
    let json = serde_json::to_value(&report).expect("serializes");
    assert_eq!(json["profiles"][0]["items"][0]["result"]["result"], "lost");
    assert_eq!(json["persistence"]["lost"], 1);
    sim.assert_clean();
}

#[test]
fn every_written_profile_is_proven_persisted_the_original_last() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    let target = snapshot(&mut sim);
    // One trigger differs in each of profiles 1, 2 and 3.
    sim.set_trigger(profile(2), pos(2, 1), Q_2_00);
    sim.set_trigger(profile(3), pos(2, 1), Q_2_00);
    sim.set_trigger(profile(1), pos(2, 2), FACTORY_TRIGGER);
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    // Written 1, 2, 3; checked 2 (left when 3 was selected), then 3 (after
    // switching away from it), then the original 1.
    let check = report.persistence.as_ref().expect("persistence check");
    assert_eq!(check.profiles, [2, 3, 1]);
    assert_eq!((check.persisted, check.lost), (3, 0));
    assert!(report.items().all(|(_, i)| matches!(
        i.result,
        ItemResult::Written {
            persistence: Persistence::Persisted,
            ..
        }
    )));
    assert_eq!(sim.discarded_edits(), 0);
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn a_reset_during_the_settle_wait_of_a_restore_sends_nothing_more() {
    let q = pos(2, 1);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let target = snapshot(&mut sim);
    sim.set_trigger(profile(2), q, Q_2_00);
    let options = RestoreOptions {
        profiles: Some(ProfileSet::only(profile(2))),
        ..options()
    };
    let prepared = prepare(&mut sim, &target, &options);
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    // Find when the trigger write goes out (on a copy: the run is
    // deterministic), and let the keyboard reset 500 ms after it, during
    // the wait for the keyboard to save.
    let write_at = {
        let mut copy = sim.clone();
        let start = copy.written().len();
        execute(&mut copy, &prepared, &saved);
        copy.written()[start..]
            .iter()
            .zip(&copy.sent_times()[start..])
            .find(|(p, _)| p.is_write() && p.as_bytes()[..5] == [0x21, 0, 0, 0, 0x18])
            .map(|(_, t)| *t)
            .expect("trigger write")
    };
    let after = write_at + std::time::Duration::from_millis(500) - sim.elapsed();
    sim.push_input_at(after, common::captured_reset_report());
    let selects_before = selects(&sim).len();
    let report = execute(&mut sim, &prepared, &saved);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::KeyboardReset)
    );
    assert!(sim.reset_delivered());
    // Only the select to profile 2 before the write; no switch after the
    // notification, not even back to profile 1.
    assert_eq!(selects(&sim)[selects_before..], [2]);
    assert_eq!(report.active_profile_now, Some(2));
    assert!(report.persistence.is_none());
    sim.assert_clean();
}

/// The scenario of hardware test H7 with a profile switch on the keyboard
/// 300 ms after the write in profile 2, while the restore waits for the
/// keyboard to save before its persistence check: with a profile report the
/// wait ends at once, without one the switch is found when the wait is
/// over. The keyboard dropped the write; the restore must not call it done.
#[test]
fn a_switch_on_the_keyboard_during_a_restore_wait_is_not_reported_as_done() {
    let q = pos(2, 1);
    for push in [true, false] {
        let mut sim = sim_with_profiles(ScopeConfig::default());
        let target = snapshot(&mut sim);
        sim.set_trigger(profile(2), q, Q_2_00);
        let options = RestoreOptions {
            profiles: Some(ProfileSet::only(profile(2))),
            ..options()
        };
        let prepared = prepare(&mut sim, &target, &options);
        let dir = temp_dir();
        let saved = prepared
            .safety()
            .save_safety(dir.path(), "restore")
            .expect("safety");
        let write_at = {
            let mut copy = sim.clone();
            assert!(execute(&mut copy, &prepared, &saved).is_success());
            common::last_trigger_write(&copy).1
        };
        let after = write_at + std::time::Duration::from_millis(300) - sim.elapsed();
        sim.switch_profile_at(after, profile(3), push);
        let selects_before = selects(&sim).len();
        let report = execute(&mut sim, &prepared, &saved);

        assert_eq!(report.outcome, Outcome::Failed, "push: {push}");
        let error = report.error.as_ref().expect("error");
        assert_eq!(error.kind, ErrorKind::ProfileChanged);
        assert!(
            error
                .message
                .contains("changed to 3 before the keyboard had saved the last write to profile 2"),
            "{error:?}"
        );
        let (_, item) = report.items().next().expect("Q");
        assert!(
            matches!(item.result, ItemResult::MaybeLost { sends: 1, .. }),
            "{:?}",
            item.result
        );
        assert_eq!(
            (report.written(), report.maybe_lost(), report.lost()),
            (0, 1, 0)
        );
        assert!(report.anything_written());
        // The persistence check never got to read it again.
        assert_eq!(report.persistence, None);
        // Profile 2 for the write, then straight back to profile 1 (nothing
        // left to wait for).
        assert_eq!(selects(&sim)[selects_before..], [2, 1]);
        assert_eq!(report.active_profile_now, Some(1));
        assert_eq!(sim.active_profile(), 1);
        // What the keyboard did: dropped the write.
        assert_eq!(sim.discarded_edits(), 1);
        assert_eq!(sim.persisted_trigger(profile(2), q), Some(Q_2_00));
        let json = serde_json::to_value(&report).expect("serializes");
        assert_eq!(
            json["profiles"][0]["items"][0]["result"]["result"],
            "maybe_lost"
        );
        sim.assert_clean();
    }
}

#[test]
fn restoring_only_selected_profiles() {
    let mut sim = sim_with_profiles(ScopeConfig::uniform(Scope::PerProfile));
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let options = RestoreOptions {
        profiles: Some(ProfileSet::only(profile(1))),
        ..options()
    };
    let report = restore(&mut sim, &target, &options);
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(report.profiles.len(), 1);
    // Profile 1 has the c06 + custom state again; profile 2 stays factory.
    assert_eq!(
        sim.table(profile(1), SimTable::Macro(slot(0))),
        Some(c13_macro().as_slice())
    );
    assert_eq!(
        sim.table(profile(2), SimTable::Macro(slot(0))),
        Some(&[0xFF; 256][..])
    );
    // A profile the backup does not hold is refused before anything is read.
    let v1 = c06_backup();
    let err = plan_restore(
        &v1,
        &target,
        &RestoreOptions {
            profiles: Some(ProfileSet::only(profile(3))),
            ..options
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, RestoreError::ProfileNotInBackup { profile: 3 }),
        "{err}"
    );
    sim.assert_clean();
}

#[test]
fn a_dry_run_writes_nothing() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let writes = sim.writes_sent();
    let plan = {
        let mut session = Session::new(&mut sim);
        dry_run_restore(&mut session, &target, &options(), |_| {
            ControlFlow::Continue(())
        })
        .expect("plan")
    };
    assert_eq!(sim.writes_sent(), writes);
    assert!(plan.has_writes());
    // Only the active profile was compared.
    assert_eq!(
        plan.not_compared(),
        vec![profile(2), profile(3), profile(4)]
    );
    assert!(plan.writes().all(|(p, _, _)| p == profile(1)));
    // The write packets of the plan can be listed (for --packets).
    let packets: usize = plan.writes().map(|(_, _, w)| w.request.steps().len()).sum();
    assert_eq!(packets, plan.packet_count());
    sim.assert_clean();
}

#[test]
fn offline_planning_against_another_backup() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let before = snapshot(&mut sim);
    customise(&mut sim);
    let after = snapshot(&mut sim);
    let plan = plan_restore(&before, &after, &options()).expect("plan");
    let tables: Vec<TableId> = plan
        .writes()
        .filter(|(p, _, _)| *p == profile(1))
        .map(|(_, item, _)| item.table)
        .collect();
    // The keymap group in the vendor site's order: macro slots, the base
    // keymap, then the (unchanged) Fn keymap so that the keyboard saves the
    // base keymap (spec 4.8.2).
    assert_eq!(
        tables,
        [
            TableId::DeadBand,
            TableId::Macro { slot: slot(0) },
            TableId::Keymap { layer: Layer::Base },
            TableId::Keymap { layer: Layer::Fn },
            TableId::CustomColors {
                table: CustomTable::Custom1
            },
        ]
    );
    let fn_write = plan
        .writes()
        .find(|(_, item, _)| item.table == TableId::Keymap { layer: Layer::Fn })
        .map(|(_, _, w)| w)
        .expect("Fn keymap write");
    assert!(fn_write.is_resent_unchanged());
    assert_eq!(
        fn_write.commit_trigger,
        Some(CommitTriggerReason::BaseKeymap)
    );
    assert_eq!(
        fn_write.summary,
        "Fn keymap (18 02) re-sent unchanged to make the keyboard save the base keymap"
    );
    assert_eq!(fn_write.packets, 10);
    // The keymap entries that change: W (46) and Caps (66).
    let keymap = plan
        .writes()
        .find(|(_, item, _)| item.table == TableId::Keymap { layer: Layer::Base })
        .map(|(_, _, w)| w)
        .expect("keymap write");
    let indices: Vec<usize> = keymap.changes.iter().map(|c| c.index).collect();
    assert_eq!(indices, [46, 66]);
    assert_eq!(keymap.packets, 10);
    // The macro is cleared (written as zeros), before the keymaps.
    let macro_write = plan
        .writes()
        .find(|(_, item, _)| item.table == TableId::Macro { slot: slot(0) })
        .map(|(_, _, w)| w)
        .expect("macro write");
    assert!(macro_write.changes[0].new.is_empty());
    let steps = macro_write.request.steps();
    assert!(
        steps
            .iter()
            .all(|s| s.packet.as_bytes()[5..].iter().all(|b| *b == 0))
    );
}

/// Replaces the reply to `request` in a list of entries.
fn replace(entries: &mut [BackupEntry], request: ReadRequest, reports: Vec<Report>) {
    let raw = RawResponse::from_reports(request, reports).expect("valid reply");
    let entry = entries
        .iter_mut()
        .find(|e| e.request() == request)
        .expect("request present");
    *entry = BackupEntry::new(raw);
}

#[test]
fn tables_without_a_confirmed_write_are_skipped_and_reported() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let mut target = {
        let mut session = Session::new(&mut sim);
        read_all(&mut session).expect("backup")
    };
    // The backup has 1 kHz polling, another side light, Win-Lock off.
    replace(
        &mut target.entries,
        ReadRequest::PollRate,
        vec![report(&[0x21, 0, 0, 0, 0x02, 0x09, 0x01])],
    );
    replace(
        &mut target.entries,
        ReadRequest::SideLight,
        vec![report(&[0x08, 0, 0, 0, 0x0c, 0x03, 0x04, 0x02, 0xff])],
    );
    let mut status = *captured_status_report(C06).as_bytes();
    status[20] = 0x00;
    replace(
        &mut target.entries,
        ReadRequest::Status,
        vec![Report::new(status)],
    );
    let target = Backup::from_json(&target.to_json().expect("json")).expect("reloads");

    let report = restore(&mut sim, &target, &options());
    assert_eq!(report.outcome, Outcome::NothingToDo);
    // Nothing was written: not even a profile switch.
    assert_eq!(sim.writes_sent(), 0);

    let plan = {
        let mut session = Session::new(&mut sim);
        dry_run_restore(&mut session, &target, &options(), |_| {
            ControlFlow::Continue(())
        })
        .expect("plan")
    };
    let skipped: Vec<(TableId, SkipReason)> = plan
        .skips()
        .map(|(_, item)| match &item.action {
            PlanAction::Skip { reason, .. } => (item.table, reason.clone()),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        skipped,
        [
            (
                TableId::SideLight,
                SkipReason::UnconfirmedCommand { command: "08 00" }
            ),
            (
                TableId::PollRate,
                SkipReason::UnconfirmedCommand {
                    command: "21/09 set"
                }
            ),
            (
                TableId::WinLockAndRollover,
                SkipReason::UnconfirmedCommand { command: "14 00" }
            ),
        ]
    );
    sim.assert_clean();
}

/// The base keymap of profile 1 after the c09 Applies, read in c10: it holds
/// the vendor site's knob defaults at 16-21 and `01` at Fn (121).
fn c10_base_keymap() -> Vec<u8> {
    let capture = load(C10);
    let exchange = capture
        .pair_reads()
        .exchanges
        .into_iter()
        .rfind(|ex| ex.complete && ex.request == ReadRequest::Keymap(Layer::Base))
        .expect("base keymap read in c10");
    exchange
        .response()
        .expect("complete")
        .table_bytes("keymap")
        .expect("table")
}

#[test]
fn knob_defaults_are_never_introduced() {
    let knob_table = c10_base_keymap();
    assert_ne!(&knob_table[16 * 4..17 * 4], &[0, 0, 0, 0]);
    assert_eq!(&knob_table[121 * 4..122 * 4], &[0x01, 0, 0, 0]);

    // The keyboard has the site's defaults; the backup (c06) has zeros.
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    sim.set_table(profile(1), SimTable::Keymap(Layer::Base), knob_table);
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(
        sim.table(profile(1), SimTable::Keymap(Layer::Base)),
        Some(&[0u8; 528][..])
    );
    // The ten base-keymap packets carry exactly the backup's (zero) bytes;
    // the ten Fn-keymap packets after them re-send its (zero) Fn keymap.
    let keymap_packets: Vec<_> = write_bytes(&sim)
        .into_iter()
        .filter(|b| b[0] == 0x18)
        .collect();
    assert_eq!(heads(&keymap_packets), base_then_fn());
    assert!(
        keymap_packets
            .iter()
            .all(|b| b[5..].iter().all(|x| *x == 0))
    );
    sim.assert_clean();
}

/// The write packets sent since `from` (an index into the write packets),
/// without profile selects.
fn table_packets(sim: &DeviceSim, from: usize) -> Vec<[u8; 63]> {
    write_bytes(sim)[from..]
        .iter()
        .filter(|b| b[..6] != [0x21, 0, 0, 0, 0, 0x12])
        .copied()
        .collect()
}

/// `(command, sub, chunk)` of each packet.
fn heads(packets: &[[u8; 63]]) -> Vec<(u8, u8, u8)> {
    packets.iter().map(|b| (b[0], b[1], b[3])).collect()
}

/// `18 00` chunks 0-9, then `18 02` chunks 0-9.
fn base_then_fn() -> Vec<(u8, u8, u8)> {
    (0..10)
        .map(|c| (0x18, 0x00, c))
        .chain((0..10).map(|c| (0x18, 0x02, c)))
        .collect()
}

/// Hardware test H9 of 2026-10-24 (PROTOCOL.md 4.8.2): restoring the pre-c09
/// backup wrote only the base keymap (`18 00`, ten packets); it was verified
/// and given the commit settle, but the persistence check found it gone. The
/// keyboard saves the base keymap only when the Fn keymap (`18 02`) is
/// written after it, so the restore re-sends the (unchanged) Fn keymap right
/// after the base keymap.
#[test]
fn a_base_keymap_restore_re_sends_the_fn_keymap_so_that_the_keyboard_saves_it() {
    let target = c06_backup();
    // Both models of the macro commit (macro slots are kept alone, H19; the
    // simulator's switch).
    for macros_wait in [false, true] {
        let mut sim = DeviceSim::from_backup(&target);
        assert!(sim.keymap_commit().base_needs_fn_write);
        sim.set_keymap_commit(KeymapCommit {
            macros_need_fn_write: macros_wait,
            ..KeymapCommit::default()
        });
        // The keyboard holds the vendor site's knob defaults (c10); the
        // backup has zeros. Nothing else differs.
        sim.set_table(profile(1), SimTable::Keymap(Layer::Base), c10_base_keymap());
        let fn_before = sim
            .table(profile(1), SimTable::Keymap(Layer::Fn))
            .map(<[u8]>::to_vec)
            .expect("Fn keymap");
        let first = sim.writes_sent();
        let report = restore(&mut sim, &target, &options());
        assert!(report.is_success(), "{macros_wait}: {:#?}", report.error);

        // 18 00 x10, then 18 02 x10 with the backup's Fn table, nothing
        // else.
        let packets = table_packets(&sim, first);
        assert_eq!(heads(&packets), base_then_fn(), "{macros_wait}");
        let fn_table = content(
            target
                .response(ReadRequest::Keymap(Layer::Fn))
                .expect("Fn keymap in the backup"),
        );
        for (chunk, packet) in packets[10..].iter().enumerate() {
            let start = chunk * 56;
            let len = usize::from(packet[4]);
            assert_eq!(&packet[5..5 + len], &fn_table[start..start + len]);
        }
        // Both written; the base keymap, read again after a profile switch,
        // was saved. The unchanged Fn keymap reads the same either way, so
        // the persistence check does not count it. It says why it was
        // written.
        assert_eq!(
            written_tables(&report, 1),
            [
                TableId::Keymap { layer: Layer::Base },
                TableId::Keymap { layer: Layer::Fn }
            ]
        );
        let results: Vec<&ItemResult> = report.items().map(|(_, i)| &i.result).collect();
        assert!(matches!(
            results[..],
            [
                ItemResult::Written {
                    persistence: Persistence::Persisted,
                    packets: 10,
                    ..
                },
                ItemResult::Written {
                    persistence: Persistence::NotChecked,
                    packets: 10,
                    sends: 1,
                    ..
                }
            ]
        ));
        let (_, fn_item) = report.items().nth(1).expect("Fn keymap item");
        assert_eq!(
            fn_item.summary,
            "Fn keymap (18 02) re-sent unchanged to make the keyboard save the base keymap"
        );
        assert_eq!(
            fn_item.commit_trigger,
            Some(CommitTriggerReason::BaseKeymap)
        );
        assert!(fn_item.resent_unchanged);
        assert_eq!((report.written(), report.resent_unchanged()), (2, 1));
        assert_eq!(
            report.persistence,
            Some(PersistenceCheck {
                profiles: vec![1],
                persisted: 1,
                lost: 0,
                not_checked: 0
            })
        );
        let json = serde_json::to_value(&report).expect("serializes");
        let item = &json["profiles"][0]["items"][1];
        assert_eq!(item["commit_trigger"], "base_keymap");
        assert_eq!(item["resent_unchanged"], true);
        assert!(
            json["profiles"][0]["items"][0]
                .get("commit_trigger")
                .is_none()
        );
        assert_eq!(
            sim.persisted_table(profile(1), SimTable::Keymap(Layer::Base)),
            Some(&[0u8; 528][..])
        );
        assert_eq!(
            sim.persisted_table(profile(1), SimTable::Keymap(Layer::Fn)),
            Some(fn_before.as_slice())
        );
        assert_eq!(sim.discarded_edits(), 0);
        sim.assert_clean();
    }
}

/// A backup of c06 whose profile 1 has the c13 macro in slot 0, bound to
/// Caps, and W -> A (the state of capture session 2's c13).
fn macro_backup() -> Backup {
    let mut source = DeviceSim::from_backup(&c06_backup());
    let p1 = profile(1);
    source.set_table(p1, SimTable::Macro(slot(0)), c13_macro());
    patch(
        &mut source,
        p1,
        SimTable::Keymap(Layer::Base),
        46 * 4,
        &[0x00, 0x04, 0x00, 0x00],
    );
    patch(
        &mut source,
        p1,
        SimTable::Keymap(Layer::Base),
        66 * 4,
        &[0x10, 0x39, 0x00, 0x00],
    );
    let mut session = Session::new(&mut source);
    read_all(&mut session).expect("backup")
}

/// The keymap group is written in the vendor site's order: macro slots,
/// base keymap, Fn keymap, advanced-key slots (spec 4.8.2). The Fn keymap
/// comes before the advanced-key slots in the same save, so the keyboard
/// keeps them (H20); everything persists, also if macro slots needed a
/// following `18 02` (they do not, H19; the simulator's switch).
#[test]
fn the_keymap_group_is_written_in_the_vendor_sites_order() {
    let target = macro_backup();
    for macros_wait in [false, true] {
        // The keyboard: c06 (no macro, zero keymaps) with advanced-key slot
        // 5 in use.
        let mut sim = DeviceSim::from_backup(&c06_backup());
        assert!(sim.keymap_commit().advanced_need_fn_write);
        sim.set_keymap_commit(KeymapCommit {
            macros_need_fn_write: macros_wait,
            ..KeymapCommit::default()
        });
        patch(
            &mut sim,
            profile(1),
            SimTable::AdvancedKeys,
            5 * 40,
            &[0x01, 0x02, 0x03],
        );
        let first = sim.writes_sent();
        let report = restore(&mut sim, &target, &options());
        assert!(report.is_success(), "{macros_wait}: {:#?}", report.error);
        let mut expected: Vec<(u8, u8, u8)> = (0..5).map(|c| (0x19, 0x00, c)).collect();
        expected.extend(base_then_fn());
        expected.push((0x22, 0x05, 0));
        assert_eq!(
            heads(&table_packets(&sim, first)),
            expected,
            "{macros_wait}"
        );
        let advanced5 = TableId::AdvancedKey {
            slot: openwraith_hid::protocol::AdvancedSlotId::new(5).expect("slot"),
        };
        assert_eq!(
            report.items().map(|(_, i)| i.table).collect::<Vec<_>>(),
            [
                TableId::Macro { slot: slot(0) },
                TableId::Keymap { layer: Layer::Base },
                TableId::Keymap { layer: Layer::Fn },
                advanced5,
            ]
        );
        let persisted = |sim: &DeviceSim, table: SimTable| {
            sim.persisted_table(profile(1), table).map(<[u8]>::to_vec)
        };
        assert_eq!(persisted(&sim, SimTable::Macro(slot(0))), Some(c13_macro()));
        let base = content(
            target
                .response(ReadRequest::Keymap(Layer::Base))
                .expect("base keymap"),
        );
        assert_eq!(persisted(&sim, SimTable::Keymap(Layer::Base)), Some(base));
        // Macro, base keymap and advanced-key clear; the unchanged Fn keymap
        // is not counted.
        assert_eq!(report.persistence.as_ref().map(|c| c.persisted), Some(3));
        assert_eq!(persisted(&sim, SimTable::AdvancedKeys), Some(vec![0; 1600]));
        sim.assert_clean();
    }
}

/// Advanced-key slots written while neither keymap layer differs: the
/// unchanged Fn keymap goes first, in the same save, so that the keyboard
/// keeps them (H20, 2026-10-27: a `22` without an `18 02` was lost).
#[test]
fn advanced_key_slots_alone_follow_the_unchanged_fn_keymap() {
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    patch(
        &mut sim,
        profile(1),
        SimTable::AdvancedKeys,
        5 * 40,
        &[0x01, 0x02, 0x03],
    );
    let first = sim.writes_sent();
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    let mut expected: Vec<(u8, u8, u8)> = (0..10).map(|c| (0x18, 0x02, c)).collect();
    expected.push((0x22, 0x05, 0));
    assert_eq!(heads(&table_packets(&sim, first)), expected);
    let (_, trigger) = report.items().next().expect("Fn keymap");
    assert_eq!(
        trigger.commit_trigger,
        Some(CommitTriggerReason::AdvancedKeys)
    );
    assert!(trigger.resent_unchanged);
    assert_eq!(
        trigger.summary,
        "Fn keymap (18 02) re-sent unchanged before the advanced-key slots to make the \
         keyboard save them"
    );
    assert_eq!(
        report.persistence.as_ref().map(|c| (c.persisted, c.lost)),
        Some((1, 0))
    );
    assert_eq!(
        sim.persisted_table(profile(1), SimTable::AdvancedKeys),
        Some(&[0u8; 1600][..])
    );
    sim.assert_clean();
}

/// Macro slots written while neither keymap layer differs: no Fn keymap
/// (the keyboard keeps a macro slot written alone, H19 of 2026-10-27), and
/// the slot persists. If macro slots needed a following `18 02` (the
/// simulator's switch), the persistence check would report the slot lost.
#[test]
fn macro_slots_alone_are_written_without_the_fn_keymap() {
    let target = c06_backup();
    for macros_wait in [false, true] {
        let mut sim = DeviceSim::from_backup(&target);
        sim.set_keymap_commit(KeymapCommit {
            macros_need_fn_write: macros_wait,
            ..KeymapCommit::default()
        });
        sim.set_table(profile(1), SimTable::Macro(slot(4)), c13_macro());
        let first = sim.writes_sent();
        let report = restore(&mut sim, &target, &options());
        let expected: Vec<(u8, u8, u8)> = (0..5).map(|c| (0x19, 0x04, c)).collect();
        assert_eq!(
            heads(&table_packets(&sim, first)),
            expected,
            "{macros_wait}"
        );
        let items: Vec<(TableId, Option<CommitTriggerReason>)> = report
            .items()
            .map(|(_, i)| (i.table, i.commit_trigger))
            .collect();
        assert_eq!(items, [(TableId::Macro { slot: slot(4) }, None)]);
        if macros_wait {
            assert_eq!(
                report.error.as_ref().map(|e| e.kind),
                Some(ErrorKind::NotPersisted)
            );
            assert_eq!(report.lost(), 1);
        } else {
            assert!(report.is_success(), "{:#?}", report.error);
            assert_eq!(report.persistence.as_ref().map(|c| c.persisted), Some(1));
            assert_eq!(
                sim.persisted_table(profile(1), SimTable::Macro(slot(4))),
                Some(&[0u8; 256][..])
            );
        }
        sim.assert_clean();
    }
}

/// A cancellation asked for between the base keymap and the Fn keymap after
/// it takes effect after the Fn keymap: the keyboard would not keep the
/// base keymap otherwise.
#[test]
fn a_cancellation_never_separates_the_base_keymap_from_the_fn_keymap() {
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    patch(
        &mut sim,
        profile(1),
        SimTable::Keymap(Layer::Base),
        46 * 4,
        &[0x00, 0x04, 0x00, 0x00],
    );
    // A colour slot differs too: a table after the keymap group.
    patch(&mut sim, profile(1), SimTable::CustomColors1, 0, &[1, 2, 3]);
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    let report = {
        let mut session = Session::new(&mut sim);
        execute_restore(&mut session, &prepared, &saved, consent(), |p| match p {
            // Ctrl+C when the Fn keymap is about to start.
            WriteProgress::Writing {
                table: TableId::Keymap { layer: Layer::Fn },
                packet: 0,
                ..
            } => ControlFlow::Break(()),
            _ => ControlFlow::Continue(()),
        })
    };
    assert_eq!(report.outcome, Outcome::Cancelled);
    assert_eq!(
        written_tables(&report, 1),
        [
            TableId::Keymap { layer: Layer::Base },
            TableId::Keymap { layer: Layer::Fn }
        ]
    );
    let colours = TableId::CustomColors {
        table: CustomTable::Custom1,
    };
    assert!(
        report
            .items()
            .any(|(_, i)| i.table == colours && matches!(i.result, ItemResult::NotAttempted))
    );
    // The keyboard saved the base keymap.
    assert!(!sim.has_edits_waiting_for_fn_write());
    assert_eq!(
        sim.persisted_table(profile(1), SimTable::Keymap(Layer::Base)),
        Some(&[0u8; 528][..])
    );
    sim.assert_clean();
}

/// When another program shows up during the base-keymap write, the restore
/// stops before the Fn keymap (nothing is written after foreign activity):
/// the base keymap is reported as may-be-lost, with a note that the keyboard
/// will not keep it, and not as written.
#[test]
fn a_restore_that_stops_before_the_fn_keymap_says_the_base_keymap_is_not_saved() {
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    patch(
        &mut sim,
        profile(1),
        SimTable::Keymap(Layer::Base),
        46 * 4,
        &[0x00, 0x04, 0x00, 0x00],
    );
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    // A foreign trigger ack right after the first base-keymap packet.
    let at = {
        let mut copy = sim.clone();
        let start = copy.written().len();
        execute(&mut copy, &prepared, &saved);
        start
            + copy.written()[start..]
                .iter()
                .position(|p| p.is_write() && p.as_bytes()[..2] == [0x18, 0x00])
                .expect("base keymap write")
            + 1
    };
    sim.inject_after_packets(at, report(&[0x21, 0, 0, 0, 0x01, 0x00]));
    let first = sim.writes_sent();
    let result = execute(&mut sim, &prepared, &saved);
    assert_eq!(result.outcome, Outcome::Failed);
    assert_eq!(
        result.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::OtherProgramActive)
    );
    // The base keymap went out, the Fn keymap did not.
    let packets = table_packets(&sim, first);
    assert_eq!(heads(&packets), base_then_fn()[..10].to_vec());
    let (_, base) = result.items().next().expect("base keymap");
    match &base.result {
        ItemResult::MaybeLost {
            warnings, packets, ..
        } => {
            assert_eq!(*packets, 10);
            assert!(
                warnings.contains(&WriteWarning::BaseKeymapNotSaved),
                "{warnings:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    let (_, fn_item) = result.items().nth(1).expect("Fn keymap");
    assert!(matches!(fn_item.result, ItemResult::Failed { .. }));
    // Not counted as written, but as sent (the undo hint stays).
    assert_eq!((result.written(), result.maybe_lost()), (0, 1));
    assert!(result.anything_written());
    // The keyboard indeed holds it unsaved.
    assert!(sim.has_edits_waiting_for_fn_write());
    sim.assert_clean();
}

/// Ctrl+C while the first macro slot of the keymap group is written: the
/// group's other tables (here the base keymap, a new change, and the Fn
/// keymap that would follow it) are not written, and the restore stops
/// after the macro slot. The keyboard keeps a macro slot without an Fn
/// keymap (H19 of 2026-10-27), and only a sent base keymap needs one.
#[test]
fn a_cancellation_after_a_macro_slot_skips_the_rest_of_the_keymap_group() {
    let target = macro_backup();
    let mut sim = DeviceSim::from_backup(&c06_backup());
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    let first = sim.writes_sent();
    let mut starts = Vec::new();
    let report = {
        let mut session = Session::new(&mut sim);
        execute_restore(&mut session, &prepared, &saved, consent(), |p| match p {
            WriteProgress::Writing {
                table, packet: 0, ..
            } => {
                starts.push(table);
                ControlFlow::Continue(())
            }
            // Ctrl+C during the first macro packet.
            WriteProgress::Writing {
                table: TableId::Macro { .. },
                packet: 1,
                ..
            } => ControlFlow::Break(()),
            _ => ControlFlow::Continue(()),
        })
    };
    assert_eq!(report.outcome, Outcome::Cancelled);
    // The skipped base and Fn keymaps are not even started.
    assert_eq!(starts, [TableId::Macro { slot: slot(0) }]);
    let expected: Vec<(u8, u8, u8)> = (0..5).map(|c| (0x19, 0x00, c)).collect();
    assert_eq!(heads(&table_packets(&sim, first)), expected);
    let results: Vec<(TableId, &ItemResult)> =
        report.items().map(|(_, i)| (i.table, &i.result)).collect();
    assert!(matches!(
        results[..],
        [
            (TableId::Macro { .. }, ItemResult::Written { .. }),
            (
                TableId::Keymap { layer: Layer::Base },
                ItemResult::NotAttempted
            ),
            (
                TableId::Keymap { layer: Layer::Fn },
                ItemResult::NotAttempted
            ),
        ]
    ));
    assert!(report.dangling.is_empty(), "{:?}", report.dangling);
    // No persistence check after a cancellation; the keyboard saved the
    // macro slot (the restore waited for the commit before it returned).
    assert_eq!(report.persistence, None);
    assert!(!sim.has_unsaved_edits());
    assert_eq!(
        sim.persisted_table(profile(1), SimTable::Macro(slot(0))),
        Some(c13_macro().as_slice())
    );
    assert_eq!(
        sim.persisted_table(profile(1), SimTable::Keymap(Layer::Base)),
        Some(&[0u8; 528][..])
    );
    sim.assert_clean();
}

/// A Ctrl+C during the Fn keymap, when it is the last planned write: the
/// restore did complete, so it is not reported as cancelled and the
/// persistence check runs.
#[test]
fn a_cancellation_during_the_last_write_still_completes_the_restore() {
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    sim.set_table(profile(1), SimTable::Keymap(Layer::Base), c10_base_keymap());
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    let report = {
        let mut session = Session::new(&mut sim);
        execute_restore(&mut session, &prepared, &saved, consent(), |p| match p {
            WriteProgress::Writing {
                table: TableId::Keymap { layer: Layer::Base },
                packet: 3,
                ..
            } => ControlFlow::Break(()),
            _ => ControlFlow::Continue(()),
        })
    };
    assert_eq!(report.outcome, Outcome::Completed, "{:#?}", report.error);
    assert_eq!(report.written(), 2);
    assert_eq!(report.persistence.as_ref().map(|c| c.persisted), Some(1));
    sim.assert_clean();
}

/// The write index (1-based, as `DeviceSim::drop_ack` counts) of the first
/// `18 02` packet a restore of `target` sends to a copy of `sim`.
fn first_fn_keymap_write(sim: &DeviceSim, target: &Backup) -> usize {
    let mut copy = sim.clone();
    let before = copy.writes_sent();
    restore(&mut copy, target, &options());
    copy.write_packets()
        .skip(before)
        .position(|p| p.as_bytes()[..2] == [0x18, 0x02])
        .expect("an 18 02 write")
        + before
        + 1
}

/// The ack of the first `18 02` packet of the commit trigger times out: the
/// read-back of an unchanged table cannot tell what arrived, so the whole
/// table is sent once more (not counted as written after one packet).
#[test]
fn a_commit_trigger_cut_short_by_an_ack_timeout_is_sent_again_in_full() {
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    sim.set_table(profile(1), SimTable::Keymap(Layer::Base), c10_base_keymap());
    let fn_write = first_fn_keymap_write(&sim, &target);
    sim.drop_ack(fn_write);
    let first = sim.writes_sent();
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    let mut expected: Vec<(u8, u8, u8)> = (0..10).map(|c| (0x18, 0x00, c)).collect();
    expected.push((0x18, 0x02, 0));
    expected.extend((0..10).map(|c| (0x18, 0x02, c)));
    assert_eq!(heads(&table_packets(&sim, first)), expected);
    let (_, fn_item) = report.items().nth(1).expect("Fn keymap");
    match &fn_item.result {
        ItemResult::Written {
            sends,
            packets,
            warnings,
            ..
        } => {
            assert_eq!((*sends, *packets), (2, 11));
            assert!(warnings.contains(&WriteWarning::AckTimeout {
                packet: 1,
                of: 10,
                send: 1
            }));
            assert!(warnings.contains(&WriteWarning::Retried));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        sim.persisted_table(profile(1), SimTable::Keymap(Layer::Base)),
        Some(&[0u8; 528][..])
    );
    sim.assert_clean();
}

/// Both sends of the commit trigger are cut short: the restore fails, and
/// the base keymap before it is reported as not saved (the simulator, like
/// the keyboard as far as known, did not save it).
#[test]
fn a_commit_trigger_that_never_arrives_complete_fails_the_restore() {
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    sim.set_table(profile(1), SimTable::Keymap(Layer::Base), c10_base_keymap());
    let fn_write = first_fn_keymap_write(&sim, &target);
    // The first packet of each send.
    sim.drop_ack(fn_write);
    sim.drop_ack(fn_write + 1);
    let report = restore(&mut sim, &target, &options());
    assert_eq!(report.outcome, Outcome::Failed);
    let error = report.error.as_ref().expect("error");
    assert_eq!(error.kind, ErrorKind::Timeout);
    assert!(
        error.message.contains("did not reach it complete"),
        "{error:?}"
    );
    let results: Vec<&ItemResult> = report.items().map(|(_, i)| &i.result).collect();
    match results[..] {
        [
            ItemResult::MaybeLost { warnings, .. },
            ItemResult::Failed { .. },
        ] => assert!(warnings.contains(&WriteWarning::BaseKeymapNotSaved)),
        _ => panic!("{results:?}"),
    }
    assert_eq!(report.persistence, None);
    assert!(sim.has_edits_waiting_for_fn_write());
    sim.assert_clean();
}

/// Without the commit settle (test-only option), a profile switch right
/// after the base keymap and its Fn keymap loses both: the persistence
/// check reports the base keymap lost. (The simulator does not save the
/// keymap at the moment the `18 02` arrives.)
#[test]
fn without_the_settle_a_base_keymap_restore_is_reported_lost() {
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    sim.set_table(profile(1), SimTable::Keymap(Layer::Base), c10_base_keymap());
    let options = RestoreOptions {
        write: WriteOptions {
            commit_settle: CommitSettle::disabled_for_tests(),
            ..WriteOptions::default()
        },
        ..options()
    };
    let report = restore(&mut sim, &target, &options);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::NotPersisted)
    );
    assert!(matches!(
        report.items().next().map(|(_, i)| &i.result),
        Some(ItemResult::Lost { .. })
    ));
    assert_eq!(
        report.persistence,
        Some(PersistenceCheck {
            profiles: vec![1],
            persisted: 0,
            lost: 1,
            not_checked: 0
        })
    );
    assert!(sim.discarded_edits() >= 1);
    assert_eq!(
        sim.persisted_table(profile(1), SimTable::Keymap(Layer::Base)),
        Some(c10_base_keymap().as_slice())
    );
    sim.assert_clean();
}

#[test]
fn empty_macro_slots_are_equivalent_and_written_as_zeros() {
    // The backup has empty (FF) macros; the keyboard has zeros: no change.
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    for s in MacroSlotId::all() {
        sim.set_table(profile(1), SimTable::Macro(s), vec![0; 256]);
    }
    let report = restore(&mut sim, &target, &options());
    assert_eq!(report.outcome, Outcome::NothingToDo);
    assert_eq!(sim.writes_sent(), 0);

    // The keyboard has a macro that the backup does not have: the slot is
    // cleared with zeros, never with FF.
    sim.set_table(profile(1), SimTable::Macro(slot(4)), c13_macro());
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(
        sim.table(profile(1), SimTable::Macro(slot(4))),
        Some(&[0u8; 256][..])
    );
    let macro_packets: Vec<_> = write_bytes(&sim)
        .into_iter()
        .filter(|b| b[0] == 0x19)
        .collect();
    assert_eq!(macro_packets.len(), 5);
    assert!(
        macro_packets
            .iter()
            .all(|b| b[1] == 4 && b[5..].iter().all(|x| *x == 0))
    );
    sim.assert_clean();
}

#[test]
fn a_non_canonical_macro_blocks_the_keymap_group() {
    let mut sim = DeviceSim::from_backup(&c06_backup());
    let mut odd = c13_macro();
    odd[1] = 7; // play mode above 3
    sim.set_table(profile(1), SimTable::Macro(slot(1)), odd);
    patch(
        &mut sim,
        profile(1),
        SimTable::Keymap(Layer::Base),
        46 * 4,
        &[0, 4, 0, 0],
    );
    let target = {
        let mut session = Session::new(&mut sim);
        read_all(&mut session).expect("backup")
    };
    sim.factory_reset_all();
    let prepared = prepare(&mut sim, &target, &options());
    let plan = prepared.plan();
    let group: Vec<_> = plan.profiles[0]
        .items
        .iter()
        .filter(|i| matches!(i.table, TableId::Keymap { .. } | TableId::Macro { .. }))
        .collect();
    for item in &group {
        match (&item.table, &item.action) {
            (
                TableId::Keymap { layer: Layer::Base } | TableId::Macro { .. },
                PlanAction::Skip {
                    reason:
                        SkipReason::KeymapGroupBlocked {
                            cause: BlockCause::NonCanonicalMacro { slot: 1, .. },
                        },
                    ..
                },
            ) => {}
            (_, PlanAction::Unchanged) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(plan.writes().all(|(_, item, _)| !matches!(
        item.table,
        TableId::Keymap { .. } | TableId::Macro { .. } | TableId::AdvancedKey { .. }
    )));
    sim.assert_clean();
}

#[test]
fn advanced_keys_are_only_cleared_and_after_the_keymaps() {
    // The keyboard has a non-empty advanced slot 5 and a keymap entry
    // using it; the backup (c06) has neither.
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    patch(
        &mut sim,
        profile(1),
        SimTable::AdvancedKeys,
        5 * 40,
        &[0x01, 0x02, 0x03],
    );
    patch(
        &mut sim,
        profile(1),
        SimTable::Keymap(Layer::Base),
        66 * 4,
        &[0x17, 5, 5, 0],
    );
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    let tables = written_tables(&report, 1);
    assert_eq!(
        tables,
        [
            TableId::Keymap { layer: Layer::Base },
            TableId::Keymap { layer: Layer::Fn },
            TableId::AdvancedKey {
                slot: openwraith_hid::protocol::AdvancedSlotId::new(5).expect("slot")
            },
        ]
    );
    // The last table write clears the slot; then the persistence check
    // switches away and back (2, then 1).
    let writes = write_bytes(&sim);
    let tables: Vec<_> = writes
        .iter()
        .filter(|b| b[..6] != [0x21, 0, 0, 0, 0, 0x12])
        .collect();
    assert_eq!(
        tables.last().map(|b| b[..5].to_vec()),
        Some(vec![0x22, 5, 0, 0, 0x32])
    );
    assert_eq!(selects(&sim), [2, 1]);
    assert_eq!(
        sim.table(profile(1), SimTable::AdvancedKeys),
        Some(&[0u8; 1600][..])
    );

    // The other way round, a slot in the backup that is not a DKS, Mod-Tap
    // or Toggle OpenWraith can write (type 9) blocks the group.
    let mut source = DeviceSim::from_backup(&target);
    patch(
        &mut source,
        profile(1),
        SimTable::AdvancedKeys,
        3 * 40,
        &[0x09],
    );
    let with_advanced = {
        let mut session = Session::new(&mut source);
        read_all(&mut session).expect("backup")
    };
    let plan = plan_restore(&with_advanced, &target, &options()).expect("plan");
    assert!(plan.skips().any(|(_, item)| matches!(
        &item.action,
        PlanAction::Skip {
            reason: SkipReason::KeymapGroupBlocked {
                cause: BlockCause::UnwritableAdvancedKey { slot: 3, .. }
            },
            ..
        }
    )));
    assert!(!plan.has_writes());
    sim.assert_clean();
}

#[test]
fn per_profile_and_global_tables_both_end_right() {
    for scope in [Scope::PerProfile, Scope::Global] {
        let mut sim = sim_with_profiles(ScopeConfig::uniform(scope));
        customise(&mut sim);
        // Profile 2 differs in its light, so two profiles are written.
        patch(&mut sim, profile(2), SimTable::MainLight, 1, &[0x03]);
        let target = snapshot(&mut sim);
        sim.factory_reset_all();
        let report = restore(&mut sim, &target, &options());
        assert!(report.is_success(), "{scope:?}: {:#?}", report.error);
        assert_matches(&sim, &target);
        let colours = TableId::CustomColors {
            table: CustomTable::Custom1,
        };
        let colour_results: Vec<&ItemResult> = report
            .items()
            .filter(|(_, i)| i.table == colours)
            .map(|(_, i)| &i.result)
            .collect();
        match scope {
            // Written in every profile.
            Scope::PerProfile => {
                assert_eq!(colour_results.len(), 4);
                assert!(
                    colour_results
                        .iter()
                        .all(|r| matches!(r, ItemResult::Written { .. }))
                );
            }
            // Written in profile 1; the other profiles already match.
            Scope::Global => {
                assert!(matches!(colour_results[0], ItemResult::Written { .. }));
                assert!(
                    colour_results[1..]
                        .iter()
                        .all(|r| matches!(r, ItemResult::AlreadyMatched))
                );
            }
        }
        assert_eq!(sim.active_profile(), 1);
        sim.assert_clean();
    }
}

#[test]
fn a_write_that_cannot_be_verified_fails_and_returns_to_the_original_profile() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    sim.set_active_profile(profile(2));
    sim.freeze(SimTable::CustomColors1);
    let report = restore(&mut sim, &target, &options());
    assert_eq!(report.outcome, Outcome::Failed);
    let error = report.error.as_ref().expect("error");
    assert_eq!(error.kind, ErrorKind::VerifyFailed);
    // The original profile (2) is written first; its colour table failed
    // after two sends, with the read-back difference.
    assert_eq!(report.profiles[0].profile, 2);
    let failed = report
        .items()
        .find(|(_, i)| matches!(i.result, ItemResult::Failed { .. }))
        .expect("failed item");
    assert_eq!(failed.0, 2);
    match &failed.1.result {
        ItemResult::Failed { mismatch, .. } => assert!(mismatch.is_some()),
        _ => unreachable!(),
    }
    // Later tables and profiles were not attempted.
    assert!(
        report
            .items()
            .any(|(p, i)| p == 3 && matches!(i.result, ItemResult::NotAttempted))
    );
    let colour_packets = write_bytes(&sim).iter().filter(|b| b[0] == 0x09).count();
    assert_eq!(colour_packets, 16);
    assert_eq!(report.active_profile_now, Some(2));
    assert_eq!(sim.active_profile(), 2);
    assert!(report.undo_command.contains("restore"));
    sim.assert_clean();
}

#[test]
fn a_reset_mid_restore_stops_without_answering_or_switching_back() {
    // Per-profile colours, so that every profile has a table to write.
    let mut sim = sim_with_profiles(ScopeConfig::uniform(Scope::PerProfile));
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    // Run once on a copy to find the packet number of the third select
    // (the switch to profile 3), then reset two packets after it.
    let reset_at = {
        let mut copy = sim.clone();
        let start = copy.written().len();
        execute(&mut copy, &prepared, &saved);
        let selects: Vec<usize> = copy.written()[start..]
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_write() && p.as_bytes()[..6] == [0x21, 0, 0, 0, 0, 0x12])
            .map(|(i, _)| i)
            .collect();
        start + selects[1] + 1 + 6
    };
    sim.reset_after_packets(reset_at);
    let report = execute(&mut sim, &prepared, &saved);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::KeyboardReset)
    );
    assert!(sim.reset_delivered());
    // No packet after the notification (assert_clean), and no switch back:
    // the keyboard stays on profile 3.
    assert_eq!(report.active_profile_now, Some(3));
    assert_eq!(sim.active_profile(), 3);
    assert!(report.anything_written());
    sim.assert_clean();
}

#[test]
fn a_disconnect_mid_restore_is_reported() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    let at = sim.written().len() + 30;
    sim.disconnect_after_packets(at);
    let report = execute(&mut sim, &prepared, &saved);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::Disconnected)
    );
    assert_eq!(report.active_profile_now, Some(1));
    assert_eq!(sim.written().len(), at);
    sim.assert_clean();
}

#[test]
fn a_profile_switch_on_the_keyboard_aborts_and_the_original_is_selected() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    // Fn + 4 on the keyboard a few packets into the restore.
    sim.switch_profile_after_packets(sim.written().len() + 8, profile(4));
    let report = execute(&mut sim, &prepared, &saved);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::ProfileChanged)
    );
    assert_eq!(report.active_profile_now, Some(1));
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn another_program_refuses_or_stops_the_restore() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");

    // A heartbeat arrived while the user read the plan: refused, nothing
    // written.
    let mut refused = sim.clone();
    let writes = refused.writes_sent();
    refused.push_input(captured_status_report(C06));
    let report = execute(&mut refused, &prepared, &saved);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::OtherProgramActive)
    );
    assert!(!report.anything_written());
    assert_eq!(refused.writes_sent(), writes);
    refused.assert_clean();

    // A foreign ack during the first table: it finishes, then the restore
    // stops and returns to the original profile.
    let at = sim.written().len() + 4;
    sim.inject_after_packets(at, report_bytes(&[0x21, 0, 0, 0, 0x01, 0x00]));
    let report = execute(&mut sim, &prepared, &saved);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::OtherProgramActive)
    );
    assert_eq!(report.written(), 1);
    assert_eq!(report.foreign_activity.map(|a| a.foreign_acks), Some(1));
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

fn report_bytes(prefix: &[u8]) -> Report {
    report(prefix)
}

#[test]
fn changes_after_planning_are_detected() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    // Someone changes W on profile 1 after the plan was made.
    sim.set_trigger(profile(1), pos(2, 2), [0x00, 0x20, 0x20, 1, 1, 0, 0, 0, 0]);
    let writes = sim.writes_sent();
    let report = execute(&mut sim, &prepared, &saved);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::ChangedSincePlan)
    );
    // W was the first table: nothing was written, the change is kept.
    assert_eq!(sim.writes_sent(), writes);
    assert_eq!(
        sim.trigger(profile(1), pos(2, 2)),
        Some([0x00, 0x20, 0x20, 1, 1, 0, 0, 0, 0])
    );
    sim.assert_clean();
}

#[test]
fn cancelling_stops_at_a_table_boundary() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    let mut tables_started = 0;
    let report = {
        let mut session = Session::new(&mut sim);
        execute_restore(&mut session, &prepared, &saved, consent(), |p| {
            if let WriteProgress::Writing { packet: 0, .. } = p {
                tables_started += 1;
                if tables_started == 3 {
                    return ControlFlow::Break(());
                }
            }
            ControlFlow::Continue(())
        })
    };
    assert_eq!(report.outcome, Outcome::Cancelled);
    assert_eq!(report.written(), 2);
    assert!(
        report
            .items()
            .skip(2)
            .all(|(_, i)| matches!(i.result, ItemResult::NotAttempted))
    );
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn a_safety_backup_of_another_restore_is_refused() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    // The saved copy of a different backup (the target itself).
    let other = target.save_safety(dir.path(), "other").expect("saved");
    assert!(!other.describes(prepared.safety()));
    let writes = sim.writes_sent();
    let report = execute(&mut sim, &prepared, &other);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::InvalidBackup)
    );
    assert_eq!(sim.writes_sent(), writes);
    sim.assert_clean();
}

#[test]
fn model_and_firmware_are_checked() {
    let target = c06_backup();
    // Firmware differs: refused unless allowed, then a warning.
    let mut other = target.clone();
    let mut info = other
        .response(ReadRequest::DeviceInfo)
        .expect("0D")
        .reports()
        .to_vec();
    let mut chunk = *info[0].as_bytes();
    let text = b"W669,34,KB,ML,W75HEARGB,V3.23.01";
    chunk[5..5 + text.len()].copy_from_slice(text);
    info[0] = Report::new(chunk);
    replace(&mut other.entries, ReadRequest::DeviceInfo, info.clone());
    let other = Backup::from_json(&other.to_json().expect("json")).expect("reloads");
    assert_eq!(other.device.firmware.as_deref(), Some("3.23.01"));
    let err = plan_restore(&target, &other, &options()).unwrap_err();
    assert!(
        matches!(err, RestoreError::FirmwareMismatch { .. }),
        "{err}"
    );
    assert_eq!(err.kind(), ErrorKind::FirmwareMismatch);
    let plan = plan_restore(
        &target,
        &other,
        &RestoreOptions {
            allow_firmware_mismatch: true,
            ..options()
        },
    )
    .expect("allowed");
    assert!(
        plan.warnings
            .iter()
            .any(|w| matches!(w, openwraith_hid::PlanWarning::FirmwareDiffers { .. }))
    );

    // Another model: always refused.
    let mut alien = target.clone();
    let text = b"W669,34,KB,ML,AP61HEARGB,V3.22.07";
    chunk[5..5 + text.len()].copy_from_slice(text);
    info[0] = Report::new(chunk);
    replace(&mut alien.entries, ReadRequest::DeviceInfo, info);
    let alien = Backup::from_json(&alien.to_json().expect("json")).expect("reloads");
    let err = plan_restore(&target, &alien, &options()).unwrap_err();
    assert!(matches!(err, RestoreError::ModelMismatch { .. }), "{err}");
    assert_eq!(err.kind(), ErrorKind::ModelMismatch);

    // A backup without an active-profile reply cannot be restored.
    let mut anonymous = target.clone();
    anonymous
        .entries
        .retain(|e| e.request() != ReadRequest::ActiveProfile);
    let anonymous = Backup::from_json(&anonymous.to_json().expect("json")).expect("reloads");
    let err = plan_restore(&anonymous, &target, &options()).unwrap_err();
    assert!(matches!(err, RestoreError::InvalidBackup { .. }), "{err}");
}

#[test]
fn the_knob_is_never_written() {
    let target = c06_backup();
    let mut source = DeviceSim::from_backup(&target);
    let knob = openwraith_hid::protocol::KeyPos::from_index(15).expect("knob");
    source.set_trigger(profile(1), knob, [0x0c, 0x20, 0x20, 2, 2, 0, 0, 0, 0]);
    let with_knob = {
        let mut session = Session::new(&mut source);
        read_all(&mut session).expect("backup")
    };
    let plan = plan_restore(&with_knob, &target, &options()).expect("plan");
    assert!(!plan.has_writes());
    let knob_item = plan
        .skips()
        .find(|(_, i)| i.key == Some(knob))
        .expect("knob");
    assert!(matches!(
        knob_item.1.action,
        PlanAction::Skip {
            reason: SkipReason::Knob,
            ..
        }
    ));
}

#[test]
fn the_capture_backup_round_trips_through_the_simulator() {
    // The simulator answers the read plan exactly like the capture.
    let target = c06_backup();
    let mut sim = DeviceSim::from_backup(&target);
    let read = {
        let mut session = Session::new(&mut sim);
        openwraith_hid::read_all_with(&mut session, &fixed_options(), |_| {}).expect("backup")
    };
    let plan = plan_restore(&target, &read, &options()).expect("plan");
    assert!(!plan.has_writes());
    assert_eq!(plan.skips().count(), 0);
    sim.assert_clean();
}

#[test]
fn a_per_profile_table_must_hold_exactly_the_planned_state() {
    let mut sim = sim_with_profiles(ScopeConfig::uniform(Scope::PerProfile));
    customise(&mut sim);
    // Profile 2: the factory light with another brightness (same mode).
    patch(&mut sim, profile(2), SimTable::MainLight, 1, &[0x03]);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    // After planning, profile 2 gets the light that is about to be written to
    // profile 1 (another mode). It must not be taken for a global table
    // written earlier: the planned write would change the mode without the
    // music-rhythm leave.
    let p1_light = content(
        target
            .snapshot(profile(1))
            .and_then(|s| s.response(ReadRequest::MainLight))
            .expect("light"),
    );
    sim.set_table(profile(2), SimTable::MainLight, p1_light.clone());
    let report = execute(&mut sim, &prepared, &saved);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::ChangedSincePlan)
    );
    assert_eq!(
        sim.table(profile(2), SimTable::MainLight),
        Some(p1_light.as_slice())
    );
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

/// `backup` with chunk 0 of the reply to `request` changed by `edit`.
fn edited_reply(backup: &Backup, request: ReadRequest, edit: impl FnOnce(&mut [u8; 63])) -> Backup {
    let mut backup = backup.clone();
    let mut reports = backup
        .response(request)
        .expect("reply in the backup")
        .reports()
        .to_vec();
    let mut bytes = *reports[0].as_bytes();
    edit(&mut bytes);
    reports[0] = Report::new(bytes);
    replace(&mut backup.entries, request, reports);
    backup
}

#[test]
fn another_distance_unit_on_the_keyboard_writes_no_trigger_or_dead_band() {
    // The backup: c06 (0.02 mm units), W at 1.00 mm and a dead band on W.
    let mut source = DeviceSim::from_backup(&c06_backup());
    patch(
        &mut source,
        profile(1),
        SimTable::DeadBand,
        92,
        &[0x05, 0x0a],
    );
    let target = {
        let mut session = Session::new(&mut source);
        openwraith_hid::read_all_with(&mut session, &fixed_options(), |_| {}).expect("backup")
    };
    // Two keyboards: 0.01 mm units (21/04 raw unit 1, max 340, min 10), and
    // the high-precision flag (0.002 mm). W at its factory value.
    let ten_um = edited_reply(&c06_backup(), ReadRequest::TriggerGlobal, |b| {
        b[6] = 0x54;
        b[7] = 0x01;
        b[9] = 0x01;
        b[10] = 0x0a;
    });
    let two_um = edited_reply(&c06_backup(), ReadRequest::Status, |b| b[18] |= 0x01);
    for (seed, unit) in [(ten_um, 10u32), (two_um, 2)] {
        let mut sim = DeviceSim::from_backup(&seed);
        sim.set_trigger(profile(1), pos(2, 2), FACTORY_TRIGGER);
        let report = restore(&mut sim, &target, &options());
        assert!(report.is_success(), "{unit} um: {:#?}", report.error);
        // Nothing to write at all: the trigger and the dead band are skipped.
        assert_eq!(report.outcome, Outcome::NothingToDo, "{unit} um");
        let plan = {
            let mut session = Session::new(&mut sim);
            dry_run_restore(&mut session, &target, &options(), |_| {
                ControlFlow::Continue(())
            })
            .expect("plan")
        };
        let expected = SkipReason::DistanceUnitChanged {
            backup_um: Some(20),
            keyboard_um: Some(unit),
        };
        let skipped = |table: TableId| {
            plan.skips().any(|(_, i)| {
                i.table == table
                    && matches!(&i.action, PlanAction::Skip { reason, .. } if *reason == expected)
            })
        };
        assert!(skipped(TableId::Triggers), "{unit} um");
        assert!(skipped(TableId::DeadBand), "{unit} um");
        assert!(
            write_bytes(&sim)
                .iter()
                .all(|b| b[0] != 0x21 && b[0] != 0x26),
            "{unit} um"
        );
        assert_eq!(sim.trigger(profile(1), pos(2, 2)), Some(FACTORY_TRIGGER));
        sim.assert_clean();
    }
}

/// The dead band is per profile (H16, 2026-10-27): a backup with another
/// dead band in profiles 1 and 2 is restored profile by profile, without a
/// final check.
#[test]
fn per_profile_dead_bands_are_restored_without_a_final_check() {
    let mut source = sim_with_profiles(ScopeConfig::default());
    patch(
        &mut source,
        profile(1),
        SimTable::DeadBand,
        92,
        &[0x05, 0x0a],
    );
    patch(
        &mut source,
        profile(2),
        SimTable::DeadBand,
        92,
        &[0x07, 0x0b],
    );
    let target = snapshot(&mut source);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let prepared = prepare(&mut sim, &target, &options());
    assert!(
        prepared.plan().warnings.is_empty(),
        "{:?}",
        prepared.plan().warnings
    );
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(written_tables(&report, 1), [TableId::DeadBand]);
    assert_eq!(written_tables(&report, 2), [TableId::DeadBand]);
    assert_eq!(report.final_check, None);
    let w = |p: u8| {
        sim.persisted_table(profile(p), SimTable::DeadBand)
            .map(|b| b[92..94].to_vec())
            .expect("dead band")
    };
    assert_eq!((w(1), w(2), w(3)), (vec![5, 10], vec![7, 11], vec![0, 0]));
    sim.assert_clean();
}

#[test]
fn ctrl_c_during_the_final_check_does_not_fail_a_complete_restore() {
    // SOCD off in the backup; on (the whole keyboard) with the A/D pair in
    // profile 2 on the keyboard: written in profile 2, read again in 1.
    let target = snapshot(&mut sim_with_profiles(ScopeConfig::default()));
    let mut sim = sim_with_profiles(ScopeConfig::default());
    socd_in(&mut sim, 2, 1, Some((0x04, 0x07)));
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    // The user presses Ctrl+C when the final check starts (the flag stays
    // set, like the one of the CLI).
    let mut cancelled = false;
    let report = {
        let mut session = Session::new(&mut sim);
        execute_restore(&mut session, &prepared, &saved, consent(), |p| {
            if p == WriteProgress::Verifying {
                cancelled = true;
            }
            if cancelled {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
    };
    assert!(cancelled);
    assert_eq!(report.outcome, Outcome::Completed, "{:#?}", report.error);
    assert!(report.is_success());
    let check = report.final_check.as_ref().expect("final check completed");
    assert_eq!(check.profiles, [1]);
    assert!(check.mismatches.is_empty());
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn the_active_profile_is_only_reported_as_read() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim);
    let target = snapshot(&mut sim);
    sim.factory_reset_all();
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");

    // Another program selects profile 2 while the user reads the plan: its
    // ack and the push of the keyboard arrive (c04 / c08).
    let mut switched = sim.clone();
    switched.set_active_profile(profile(2));
    switched.push_input(report(&[0x21, 0, 0, 0, 0x02, 0x12, 0x01]));
    switched.push_input(report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x02, 0x00]));
    let writes = switched.writes_sent();
    let result = execute(&mut switched, &prepared, &saved);
    assert_eq!(
        result.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::OtherProgramActive)
    );
    assert_eq!(switched.writes_sent(), writes);
    assert_eq!(result.active_profile_now, Some(2));
    switched.assert_clean();

    // A reset notification: the profile afterwards is unknown.
    let mut reset = sim.clone();
    reset.push_input(common::captured_reset_report());
    let result = execute(&mut reset, &prepared, &saved);
    assert_eq!(
        result.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::KeyboardReset)
    );
    assert_eq!(result.active_profile_now, None);
    reset.assert_clean();

    // A safety backup that does not belong to the restore: nothing is read.
    let other = target.save_safety(dir.path(), "other").expect("saved");
    let result = execute(&mut sim, &prepared, &other);
    assert_eq!(
        result.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::InvalidBackup)
    );
    assert_eq!(result.active_profile_now, None);
}

#[test]
fn an_interrupted_read_mid_table_is_retried() {
    // The keyboard is on profile 2; the base keymap of profile 1 differs
    // from the backup (W -> A), so the restore selects 1, writes ten
    // base-keymap and ten Fn-keymap chunks and selects 2 again.
    let target = c06_backup();
    let mut sim = sim_with_profiles(ScopeConfig::default());
    patch(
        &mut sim,
        profile(1),
        SimTable::Keymap(Layer::Base),
        46 * 4,
        &[0x00, 0x04, 0x00, 0x00],
    );
    sim.set_active_profile(profile(2));
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    // Ctrl+C on Linux: the read of the ack of the third chunk (after the
    // select and three chunks) fails with EINTR.
    let report = {
        let mut tamper = common::Tamper::new(&mut sim);
        tamper.interrupt_after_writes = Some(4);
        let mut session = Session::new(&mut tamper);
        let report = execute_restore(&mut session, &prepared, &saved, consent(), |_| {
            ControlFlow::Continue(())
        });
        assert_eq!(session.stats().interrupted, 1);
        assert!(!session.connection_lost());
        report
    };
    assert_eq!(report.outcome, Outcome::Completed, "{:#?}", report.error);
    assert_eq!(
        written_tables(&report, 1),
        [
            TableId::Keymap { layer: Layer::Base },
            TableId::Keymap { layer: Layer::Fn }
        ]
    );
    assert_eq!(
        sim.table(profile(1), SimTable::Keymap(Layer::Base)),
        Some(&[0u8; 528][..])
    );
    // Back on the original profile.
    assert_eq!(report.active_profile_now, Some(2));
    assert_eq!(sim.active_profile(), 2);
    sim.assert_clean();
}

/// SOCD of profile `p` in `sim`: the power (one setting of the whole
/// keyboard unless the scope says otherwise) and the profile's pairs: one
/// pair `key1`/`key2` (mode 1), or none.
fn socd_in(sim: &mut DeviceSim, p: u8, power: u8, pair: Option<(u8, u8)>) {
    sim.set_table(
        profile(p),
        SimTable::SocdSettings,
        vec![power, 0, 0, 0, 0xfa, 0xe5],
    );
    let mut pairs = vec![0u8; 80];
    if let Some((key1, key2)) = pair {
        pairs[..4].copy_from_slice(&[0x01, 0x00, key1, key2]);
    }
    sim.set_table(profile(p), SimTable::SocdPairs, pairs);
}

/// The results of the SOCD items of a report, per profile.
fn socd_results(report: &RestoreReport) -> Vec<(u8, ItemResult)> {
    report
        .items()
        .filter(|(_, i)| i.table == TableId::Socd)
        .map(|(p, i)| (p, i.result.clone()))
        .collect()
}

/// The SOCD writes of a plan: profile and changed entries (0 = the power).
fn socd_writes(plan: &RestorePlan) -> Vec<(u8, Vec<usize>)> {
    plan.writes()
        .filter(|(_, item, _)| item.table == TableId::Socd)
        .map(|(p, _, w)| (p.get(), w.changes.iter().map(|c| c.index).collect()))
        .collect()
}

/// How many SOCD bursts (`24 03`) the simulator received.
fn socd_bursts(sim: &DeviceSim) -> usize {
    write_bytes(sim)
        .iter()
        .filter(|b| b[..2] == [0x24, 0x03])
        .count()
}

/// The SOCD power and the first pair entry of every profile, as saved.
fn socd_state(sim: &DeviceSim) -> Vec<(u8, Vec<u8>)> {
    ProfileId::ALL
        .into_iter()
        .map(|p| {
            let power = sim
                .persisted_table(p, SimTable::SocdSettings)
                .map_or(0xff, |b| b[0]);
            let pair = sim
                .persisted_table(p, SimTable::SocdPairs)
                .map(|b| b[..4].to_vec())
                .unwrap_or_default();
            (power, pair)
        })
        .collect()
}

const NO_PAIR: [u8; 4] = [0; 4];
const A_D: [u8; 4] = [0x01, 0x00, 0x04, 0x07];

/// H24 (2026-10-27, `hw/v02/h24.txt`): a multi-profile restore of a backup
/// with SOCD off and no pairs, while the keyboard has SOCD on (one setting
/// of the whole keyboard) and the A/D pair in profile 2 only. The old plan
/// wrote SOCD in every profile: the first write switched the power off for
/// all of them, and the re-read before profile 2's write took that for a
/// change by someone else and stopped. Now SOCD is written once, in profile
/// 2 (whose pairs differ), and carries the power for every profile.
#[test]
fn a_multi_profile_restore_writes_the_socd_power_once() {
    // The backup: c06 in every profile (SOCD off, no pairs), profile 4
    // active, as in H24.
    let mut source = sim_with_profiles(ScopeConfig::default());
    source.set_active_profile(profile(4));
    let target = snapshot(&mut source);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    sim.set_active_profile(profile(4));
    socd_in(&mut sim, 2, 1, Some((0x04, 0x07)));
    let prepared = prepare(&mut sim, &target, &options());
    // One SOCD write, in profile 2: the power (shown once) and pair 1.
    assert_eq!(socd_writes(prepared.plan()), [(2, vec![0, 1])]);
    for plan in prepared
        .plan()
        .profiles
        .iter()
        .filter(|p| p.profile.get() != 2)
    {
        let socd = plan.items.iter().find(|i| i.table == TableId::Socd);
        assert_eq!(
            socd.map(|i| &i.action),
            Some(&PlanAction::Unchanged),
            "profile {}",
            plan.profile
        );
    }
    assert!(
        prepared.plan().warnings.is_empty(),
        "{:?}",
        prepared.plan().warnings
    );
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    let report = execute(&mut sim, &prepared, &saved);
    assert!(report.is_success(), "{:#?}", report.error);
    assert!(matches!(
        socd_results(&report)[..],
        [(
            2,
            ItemResult::Written {
                persistence: Persistence::Persisted,
                ..
            }
        )]
    ));
    assert_eq!(socd_bursts(&sim), 1);
    // Written in profile 2; the original profile 4 reads the new power.
    let check = report.final_check.as_ref().expect("final check");
    assert_eq!(check.profiles, [4]);
    assert!(check.mismatches.is_empty(), "{:#?}", check.mismatches);
    assert_eq!(socd_state(&sim), vec![(0, NO_PAIR.to_vec()); 4]);
    assert_eq!(sim.active_profile(), 4);
    sim.assert_clean();
}

/// Only the SOCD power differs: it is written once, re-sending the unchanged
/// pairs, in the original profile when the restore covers it, else in the
/// first profile written; the other profiles read the new power too.
#[test]
fn a_socd_power_change_alone_is_written_once() {
    // The backup: SOCD off, no pairs, profile 1 active.
    let target = snapshot(&mut sim_with_profiles(ScopeConfig::default()));
    let keyboard = || {
        let mut sim = sim_with_profiles(ScopeConfig::default());
        socd_in(&mut sim, 1, 1, None);
        sim.set_active_profile(profile(3));
        sim
    };

    // Every profile, profile 3 active: written in profile 3.
    let mut sim = keyboard();
    let prepared = prepare(&mut sim, &target, &options());
    assert_eq!(socd_writes(prepared.plan()), [(3, vec![0])]);
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(socd_bursts(&sim), 1);
    // Written in the original profile itself: nothing to read again.
    assert_eq!(report.final_check, None);
    assert_eq!(socd_state(&sim), vec![(0, NO_PAIR.to_vec()); 4]);
    sim.assert_clean();

    // Profiles 1 and 2 only, profile 2 with another light: written in
    // profile 2 (written anyway), read again in profile 3.
    let mut sim = keyboard();
    patch(&mut sim, profile(2), SimTable::MainLight, 1, &[0x03]);
    let mut profiles = ProfileSet::only(profile(1));
    profiles.insert(profile(2));
    let options = RestoreOptions {
        profiles: Some(profiles),
        ..options()
    };
    let prepared = prepare(&mut sim, &target, &options);
    assert_eq!(socd_writes(prepared.plan()), [(2, vec![0])]);
    let report = restore(&mut sim, &target, &options);
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(socd_bursts(&sim), 1);
    let check = report.final_check.as_ref().expect("final check");
    assert_eq!(
        (check.profiles.as_slice(), check.mismatches.len()),
        (&[3][..], 0)
    );
    assert_eq!(socd_state(&sim), vec![(0, NO_PAIR.to_vec()); 4]);
    sim.assert_clean();
}

/// A backup whose profiles hold different SOCD power settings (made where
/// the power was per profile, or edited): the keyboard has one power, so
/// the power of the profile that was active when the backup was made is
/// restored, and the plan says so.
#[test]
fn the_socd_power_of_the_backups_active_profile_is_restored() {
    let mut source = sim_with_profiles(ScopeConfig {
        socd_power: Scope::PerProfile,
        ..ScopeConfig::default()
    });
    socd_in(&mut source, 1, 1, None);
    socd_in(&mut source, 2, 0, Some((0x04, 0x07)));
    let target = snapshot(&mut source);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let prepared = prepare(&mut sim, &target, &options());
    assert!(
        prepared
            .plan()
            .warnings
            .contains(&PlanWarning::SocdPowerDiffersInBackup {
                profile: 1,
                power: true
            }),
        "{:?}",
        prepared.plan().warnings
    );
    // Profile 2's pair is written with profile 1's power, shown once.
    assert_eq!(socd_writes(prepared.plan()), [(2, vec![0, 1])]);
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    let check = report.final_check.as_ref().expect("final check");
    assert!(check.mismatches.is_empty(), "{:#?}", check.mismatches);
    assert_eq!(
        socd_state(&sim),
        [
            (1, NO_PAIR.to_vec()),
            (1, A_D.to_vec()),
            (1, NO_PAIR.to_vec()),
            (1, NO_PAIR.to_vec())
        ]
    );
    sim.assert_clean();
}

/// The final check reads the SOCD power once more in the original profile
/// after it was written in another one: a keyboard that kept the power per
/// profile (not the W75, H21) fails the restore there.
#[test]
fn a_keyboard_that_keeps_the_socd_power_per_profile_fails_the_final_check() {
    let target = snapshot(&mut sim_with_profiles(ScopeConfig::default()));
    let mut sim = sim_with_profiles(ScopeConfig {
        socd_power: Scope::PerProfile,
        ..ScopeConfig::default()
    });
    for p in 1..=4 {
        socd_in(&mut sim, p, 1, None);
    }
    socd_in(&mut sim, 2, 1, Some((0x04, 0x07)));
    let report = restore(&mut sim, &target, &options());
    assert_eq!(written_tables(&report, 2), [TableId::Socd]);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::VerifyFailed)
    );
    let check = report.final_check.as_ref().expect("final check");
    assert_eq!(check.profiles, [1]);
    let found: Vec<(u8, TableId, ReadRequest)> = check
        .mismatches
        .iter()
        .map(|m| (m.profile, m.mismatch.table, m.mismatch.read))
        .collect();
    assert_eq!(found, [(1, TableId::Socd, ReadRequest::SocdSettings)]);
    assert_eq!(report.active_profile_now, Some(1));
    sim.assert_clean();
}

/// An import writes the file's SOCD pairs only: the power is one setting of
/// the whole keyboard, not part of the profile, and keeps the keyboard's
/// value (a plan note says so).
#[test]
fn importing_socd_writes_the_pairs_and_keeps_the_keyboards_power() {
    let mut source = DeviceSim::from_backup(&c06_backup());
    socd_in(&mut source, 1, 1, Some((0x04, 0x07)));
    let file = read_all(&mut Session::new(&mut source))
        .expect("backup")
        .export_profile(profile(1), None)
        .expect("export");
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let import = RestoreOptions {
        import: Some(openwraith_hid::ImportOptions {
            from: profile(1),
            into: profile(3),
            include: openwraith_hid::ImportSelection {
                triggers: false,
                lighting: false,
                socd: true,
                ..openwraith_hid::ImportSelection::default()
            },
            allow_critical_fn_overrides: false,
        }),
        ..options()
    };
    let prepared = prepare(&mut sim, &file, &import);
    assert!(
        prepared
            .plan()
            .warnings
            .contains(&PlanWarning::SocdPowerNotImported { power: false }),
        "{:?}",
        prepared.plan().warnings
    );
    // The pair only, sent with the keyboard's power (off).
    assert_eq!(socd_writes(prepared.plan()), [(3, vec![1])]);
    let summary = prepared
        .plan()
        .writes()
        .find(|(_, item, _)| item.table == TableId::Socd)
        .map(|(_, _, w)| w.summary.clone());
    assert_eq!(
        summary.as_deref(),
        Some("SOCD power off and pairs (24 03, 24 00)")
    );
    let report = restore(&mut sim, &file, &import);
    assert!(report.is_success(), "{:#?}", report.error);
    assert!(matches!(
        socd_results(&report)[..],
        [(3, ItemResult::Written { .. })]
    ));
    assert_eq!(
        socd_state(&sim),
        [
            (0, NO_PAIR.to_vec()),
            (0, NO_PAIR.to_vec()),
            (0, A_D.to_vec()),
            (0, NO_PAIR.to_vec())
        ]
    );
    sim.assert_clean();
}

/// Pairs that differ in two profiles while the power changes: both are
/// written with the same power; the second write's re-read finds the power
/// the first one set (one setting of the whole keyboard) and takes it as
/// this restore's own, not as a change by someone else.
#[test]
fn socd_pairs_of_several_profiles_are_written_with_one_power() {
    let mut source = sim_with_profiles(ScopeConfig::default());
    socd_in(&mut source, 1, 0, Some((0x04, 0x07)));
    socd_in(&mut source, 2, 0, Some((0x1a, 0x16)));
    let target = snapshot(&mut source);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    socd_in(&mut sim, 1, 1, None);
    let prepared = prepare(&mut sim, &target, &options());
    // The power is shown once, on the first write (the original profile).
    assert_eq!(
        socd_writes(prepared.plan()),
        [(1, vec![0, 1]), (2, vec![1])]
    );
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(socd_bursts(&sim), 2);
    // Both written in their own profile; profile 1 is the original one, so
    // there is no final check.
    assert_eq!(report.final_check, None);
    assert_eq!(
        socd_state(&sim),
        [
            (0, A_D.to_vec()),
            (0, vec![0x01, 0x00, 0x1a, 0x16]),
            (0, NO_PAIR.to_vec()),
            (0, NO_PAIR.to_vec())
        ]
    );
    sim.assert_clean();
}

/// The virtual times of the write packets the simulator received from
/// write `from` on that start with `head`.
fn write_times(sim: &DeviceSim, from: usize, head: &[u8]) -> Vec<std::time::Duration> {
    sim.written()[from..]
        .iter()
        .zip(&sim.sent_times()[from..])
        .filter(|(p, _)| p.is_write() && p.as_bytes().starts_with(head))
        .map(|(_, t)| *t)
        .collect()
}

/// After this restore's first SOCD write (the power, one setting of the
/// whole keyboard), every profile is expected to read the new power: a
/// Fn + Right Shift press before the next profile's SOCD write switches it
/// back, and that write stops with "changed after planning" instead of
/// silently switching the user's choice back again (it was accepted as the
/// safety backup's power before). The same press stops a restore that does
/// not change the power.
#[test]
fn a_socd_switch_between_two_socd_writes_stops_the_restore() {
    for backup_power in [0, 1] {
        let mut source = sim_with_profiles(ScopeConfig::default());
        socd_in(&mut source, 1, backup_power, Some((0x04, 0x07)));
        socd_in(&mut source, 2, backup_power, Some((0x1d, 0x1b)));
        let target = snapshot(&mut source);
        let keyboard = || {
            let mut sim = sim_with_profiles(ScopeConfig::default());
            socd_in(&mut sim, 1, 1, None);
            sim
        };
        let mut copy = keyboard();
        let start = copy.written().len();
        let report = restore(&mut copy, &target, &options());
        assert!(report.is_success(), "{:#?}", report.error);
        assert_eq!(socd_bursts(&copy), 2);
        let first_burst = write_times(&copy, start, &[0x24, 0x03])[0];
        // The select of profile 2 for its SOCD write.
        let select2 = write_times(&copy, start, &[0x21, 0, 0, 0, 0, 0x12, 0x02])
            .into_iter()
            .find(|t| *t > first_burst)
            .expect("profile 2 selected after the first write");
        let mut offset = first_burst + std::time::Duration::from_millis(300);
        let mut presses = 0;
        while offset < select2 {
            let mut sim = keyboard();
            sim.press_socd_hotkey_at(offset - sim.elapsed());
            let report = restore(&mut sim, &target, &options());
            assert_eq!(
                report.error.as_ref().map(|e| e.kind),
                Some(ErrorKind::ChangedSincePlan),
                "power {backup_power}, pressed at {offset:?}: {:#?}",
                report.error
            );
            let results: Vec<(u8, bool)> = socd_results(&report)
                .into_iter()
                .map(|(p, r)| (p, r.was_sent()))
                .collect();
            assert_eq!(results, [(1, true), (2, false)], "power {backup_power}");
            // The press stands: the restore did not switch it back.
            let power = 1 - backup_power;
            assert!(
                socd_state(&sim).iter().all(|(p, _)| *p == power),
                "power {backup_power}: {:?}",
                socd_state(&sim)
            );
            sim.assert_clean();
            presses += 1;
            offset += std::time::Duration::from_millis(250);
        }
        assert!(presses > 0, "no press landed between the two writes");
    }
}

/// The final check reads the SOCD power once more in the original profile:
/// a Fn + Right Shift press after this restore's SOCD write (in another
/// profile) is the user's switch, not a mismatch.
#[test]
fn a_socd_switch_after_the_write_is_not_a_final_check_mismatch() {
    // H24: the backup made on profile 4 (SOCD off, no pairs); SOCD on with
    // the A/D pair in profile 2 on the keyboard, profile 4 active.
    let mut source = sim_with_profiles(ScopeConfig::default());
    source.set_active_profile(profile(4));
    let target = snapshot(&mut source);
    let keyboard = || {
        let mut sim = sim_with_profiles(ScopeConfig::default());
        sim.set_active_profile(profile(4));
        socd_in(&mut sim, 2, 1, Some((0x04, 0x07)));
        sim
    };
    let mut copy = keyboard();
    let start = copy.written().len();
    let report = restore(&mut copy, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    let burst = write_times(&copy, start, &[0x24, 0x03])[0];
    for after in [300, 1000, 1800] {
        let mut sim = keyboard();
        sim.press_socd_hotkey_at(burst + std::time::Duration::from_millis(after) - sim.elapsed());
        let report = restore(&mut sim, &target, &options());
        assert!(report.is_success(), "{after} ms: {:#?}", report.error);
        let check = report.final_check.as_ref().expect("final check");
        assert_eq!(check.profiles, [4]);
        assert!(
            check.mismatches.is_empty(),
            "{after} ms: {:#?}",
            check.mismatches
        );
        // The switch stands (on again); the write says it was switched.
        assert_eq!(socd_state(&sim)[3].0, 1, "{after} ms");
        assert!(
            matches!(
                &socd_results(&report)[..],
                [(2, ItemResult::Written { warnings, .. })]
                    if warnings.iter().any(|w| matches!(w, WriteWarning::SocdToggledOnKeyboard { .. }))
            ),
            "{after} ms: {:?}",
            socd_results(&report)
        );
        sim.assert_clean();
    }
}

/// The power restored is the one of the profile that was active when the
/// backup was made, also when that is not the first profile restored; and a
/// power-only write goes into the original profile before a profile that is
/// written anyway.
#[test]
fn the_socd_power_comes_from_the_backups_active_profile_and_goes_into_the_original_one() {
    // A backup made on profile 3, where the power read on (profiles 1, 2
    // and 4 off), with the A/D pair in profile 2.
    let mut source = sim_with_profiles(ScopeConfig {
        socd_power: Scope::PerProfile,
        ..ScopeConfig::default()
    });
    for p in [1, 4] {
        socd_in(&mut source, p, 0, None);
    }
    socd_in(&mut source, 2, 0, Some((0x04, 0x07)));
    socd_in(&mut source, 3, 1, None);
    source.set_active_profile(profile(3));
    let target = snapshot(&mut source);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let prepared = prepare(&mut sim, &target, &options());
    assert!(
        prepared
            .plan()
            .warnings
            .contains(&PlanWarning::SocdPowerDiffersInBackup {
                profile: 3,
                power: true
            }),
        "{:?}",
        prepared.plan().warnings
    );
    assert_eq!(socd_writes(prepared.plan()), [(2, vec![0, 1])]);
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    assert!(socd_state(&sim).iter().all(|(power, _)| *power == 1));
    sim.assert_clean();

    // Only the power differs, the keyboard is on profile 3, and profile 1
    // has another light: the power goes into profile 3, not into profile 1
    // (written anyway, and first in profile order).
    let target = snapshot(&mut sim_with_profiles(ScopeConfig::default()));
    let mut sim = sim_with_profiles(ScopeConfig::default());
    socd_in(&mut sim, 1, 1, None);
    patch(&mut sim, profile(1), SimTable::MainLight, 1, &[0x03]);
    sim.set_active_profile(profile(3));
    let prepared = prepare(&mut sim, &target, &options());
    assert_eq!(socd_writes(prepared.plan()), [(3, vec![0])]);
    let report = restore(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(report.final_check, None);
    assert_eq!(socd_state(&sim), vec![(0, NO_PAIR.to_vec()); 4]);
    sim.assert_clean();
}
