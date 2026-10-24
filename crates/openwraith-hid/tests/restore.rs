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
    Backup, BackupEntry, BlockCause, DeviceSim, ErrorKind, ItemResult, Outcome, PlanAction,
    PreparedRestore, ProfileSet, RestoreError, RestoreOptions, RestoreReport, SavedBackup, Scope,
    ScopeConfig, Session, SimTable, SkipReason, WriteProgress, dry_run_restore, execute_restore,
    plan_restore, prepare_restore, read_all, read_all_profiles,
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
    // Every profile was written (at least its colour table, which is per
    // profile here), so the final check ran and found nothing.
    let check = report.final_check.as_ref().expect("final check");
    assert_eq!(check.profiles, vec![1, 2, 3, 4]);
    assert!(check.mismatches.is_empty());

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
    // Safety backup: 1 then back to 2; restore: 1 then back to 2.
    assert_eq!(selects(&sim)[selects_before..], [1, 2, 1, 2]);
    sim.assert_clean();
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
    assert_eq!(
        tables,
        [
            TableId::DeadBand,
            TableId::Keymap { layer: Layer::Base },
            TableId::Macro { slot: slot(0) },
            TableId::CustomColors {
                table: CustomTable::Custom1
            },
        ]
    );
    // The keymap entries that change: W (46) and Caps (66).
    let keymap = plan
        .writes()
        .find(|(_, item, _)| item.table == TableId::Keymap { layer: Layer::Base })
        .map(|(_, _, w)| w)
        .expect("keymap write");
    let indices: Vec<usize> = keymap.changes.iter().map(|c| c.index).collect();
    assert_eq!(indices, [46, 66]);
    assert_eq!(keymap.packets, 10);
    // The macro is cleared (written as zeros) after the keymap unbinds it.
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
    // The ten keymap packets carry exactly the backup's (zero) bytes.
    let keymap_packets: Vec<_> = write_bytes(&sim)
        .into_iter()
        .filter(|b| b[0] == 0x18)
        .collect();
    assert_eq!(keymap_packets.len(), 10);
    assert!(
        keymap_packets
            .iter()
            .all(|b| b[1] == 0x00 && b[5..].iter().all(|x| *x == 0))
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
            TableId::AdvancedKey {
                slot: openwraith_hid::protocol::AdvancedSlotId::new(5).expect("slot")
            },
        ]
    );
    let writes = write_bytes(&sim);
    assert_eq!(
        writes.last().map(|b| b[..5].to_vec()),
        Some(vec![0x22, 5, 0, 0, 0x32])
    );
    assert_eq!(
        sim.table(profile(1), SimTable::AdvancedKeys),
        Some(&[0u8; 1600][..])
    );

    // The other way round, a non-empty slot in the backup blocks the group.
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
                cause: BlockCause::NonEmptyAdvancedKey { slot: 3 }
            },
            ..
        }
    )));
    assert!(!plan.has_writes());
    sim.assert_clean();
}

#[test]
fn unknown_scope_tables_end_right_both_per_profile_and_global() {
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
        let check = report.final_check.as_ref().expect("final check");
        assert!(check.mismatches.is_empty(), "{scope:?}");
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

#[test]
fn a_shared_table_written_in_one_profile_is_checked_in_the_others() {
    // The backup was made where custom colour table 1 is per profile:
    // profile 1 has a blue slot, profiles 2-4 do not.
    let mut source = sim_with_profiles(ScopeConfig::uniform(Scope::PerProfile));
    customise(&mut source);
    let target = snapshot(&mut source);
    // The keyboard shares the table between all profiles (another firmware)
    // and holds the copy of profiles 2-4.
    let mut sim = sim_with_profiles(ScopeConfig {
        custom_colors1: Scope::Global,
        ..ScopeConfig::uniform(Scope::PerProfile)
    });
    let prepared = prepare(&mut sim, &target, &options());
    let colours = TableId::CustomColors {
        table: CustomTable::Custom1,
    };
    // The plan says so: the copies of the backup differ, the keyboard's do
    // not.
    assert!(
        prepared
            .plan()
            .warnings
            .contains(&openwraith_hid::PlanWarning::LikelySharedTableDiffers { table: colours }),
        "{:?}",
        prepared.plan().warnings
    );
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety");
    let report = execute(&mut sim, &prepared, &saved);
    // Only the colours of profile 1 were written; that changed the copy of
    // profile 2, which the plan left unchanged. The final check reads it
    // again.
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::VerifyFailed)
    );
    let check = report.final_check.as_ref().expect("final check ran");
    assert!(
        check
            .mismatches
            .iter()
            .any(|m| m.profile == 2 && m.mismatch.table == colours),
        "{check:#?}"
    );
    assert!(check.mismatches.iter().all(|m| m.profile != 1));
    assert_eq!(report.active_profile_now, Some(1));
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn a_final_check_mismatch_fails_the_restore() {
    // Per-profile dead bands in the backup: profile 1 `05 0a`, profile 2
    // `07 0b` on W.
    let mut source = sim_with_profiles(ScopeConfig::uniform(Scope::PerProfile));
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
    // The keyboard shares one (zero) dead band between all profiles.
    let mut sim = sim_with_profiles(ScopeConfig {
        dead_band: Scope::Global,
        ..ScopeConfig::uniform(Scope::PerProfile)
    });
    let report = restore(&mut sim, &target, &options());
    // Both were written (the write of profile 2 replaced that of profile 1),
    // and the final check found that profile 1 no longer holds its dead
    // band.
    assert_eq!(written_tables(&report, 1), [TableId::DeadBand]);
    assert_eq!(written_tables(&report, 2), [TableId::DeadBand]);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::VerifyFailed)
    );
    // Profiles 3 and 4 (zero dead band in the backup, planned unchanged)
    // now hold the shared table too.
    let check = report.final_check.as_ref().expect("final check");
    assert_eq!(check.profiles, [1, 2, 3, 4]);
    let differing: Vec<(u8, TableId)> = check
        .mismatches
        .iter()
        .map(|m| (m.profile, m.mismatch.table))
        .collect();
    assert_eq!(
        differing,
        [
            (1, TableId::DeadBand),
            (3, TableId::DeadBand),
            (4, TableId::DeadBand)
        ]
    );
    assert_eq!(report.active_profile_now, Some(1));
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn ctrl_c_during_the_final_check_does_not_fail_a_complete_restore() {
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
    assert_eq!(check.profiles, [1, 2, 3, 4]);
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
    // from the backup (W -> A), so the restore selects 1, writes ten keymap
    // chunks and selects 2 again.
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
        [TableId::Keymap { layer: Layer::Base }]
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
