//! Writing macro slot 0 makes the keyboard empty macro slots 1-9 of the
//! profile (CONFIRMED on hardware, H24 and `hw/v02/macro-exp.txt`,
//! 2026-10-27): every edit, restore and import that writes slot 0 writes
//! every other slot that keeps content again right after it, also after a
//! cancellation, and reports what a stop part-way left empty.

mod common;

use std::ops::ControlFlow;

use common::{
    bind_macro as bind, c06_backup, consent, is_empty_slot as is_empty, kept_macro as kept,
    key_named as key, macro_bytes as bytes, macro_slot as slot, profile, profiles_options,
    put_macro, sim_with_profiles, tap, temp_dir,
};
use openwraith_hid::protocol::{Layer, MacroProgram, ProfileId, TableId};
use openwraith_hid::{
    Backup, ClearedMacro, DeviceSim, EditError, EditOptions, EditReport, ErrorKind, ImportOptions,
    ImportSelection, ItemResult, KeymapChange, KeymapEdit, KeymapEditPlan, Outcome, PlanAction,
    PlanWarning, PreparedRestore, ResendReason, RestoreOptions, RestoreReport, SavedBackup,
    ScopeConfig, Session, SimTable, SlotRef, WriteProgress, apply_keymap_edit, execute_restore,
    plan_keymap_edit, prepare_restore, read_all, read_all_profiles,
};

// ----- edits -----

/// Profile 1 (active): slot 0 = A, slot 2 = C bound to W, slot 5 = E
/// (unbound).
fn edit_sim() -> DeviceSim {
    let mut sim = DeviceSim::from_backup(&c06_backup());
    let p1 = profile(1);
    put_macro(&mut sim, p1, 0, &tap(0x04));
    put_macro(&mut sim, p1, 2, &tap(0x06));
    bind(&mut sim, p1, Layer::Base, "W", 2);
    put_macro(&mut sim, p1, 5, &tap(0x08));
    sim
}

fn safety(sim: &mut DeviceSim, dir: &tempfile::TempDir) -> SavedBackup {
    let backup = read_all(&mut Session::new(sim)).expect("backup");
    backup.save_safety(dir.path(), "set").expect("saved")
}

fn plan(sim: &mut DeviceSim, edit: &KeymapEdit) -> Result<KeymapEditPlan, EditError> {
    plan_keymap_edit(&mut Session::new(sim), edit)
}

fn apply(
    sim: &mut DeviceSim,
    plan: &KeymapEditPlan,
    saved: &SavedBackup,
    progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> EditReport {
    apply_keymap_edit(
        &mut Session::new(sim),
        plan,
        saved,
        consent(),
        &EditOptions::default(),
        progress,
    )
}

fn set_slot0(program: Option<MacroProgram>) -> KeymapEdit {
    KeymapEdit {
        changes: vec![KeymapChange::SetMacroSlot {
            slot: slot(0),
            program,
        }],
        ..KeymapEdit::default()
    }
}

/// (slot, re-send reason, written even after a cancellation) of every
/// macro write of a plan, in order.
fn macro_writes(plan: &KeymapEditPlan) -> Vec<(u8, Option<ResendReason>, bool)> {
    plan.writes()
        .iter()
        .filter_map(|w| match w.table {
            TableId::Macro { slot } => Some((slot.get(), w.resent, w.completes_binding)),
            _ => None,
        })
        .collect()
}

/// Step 4 of `hw/v02/macro-exp.txt`: slot 0 rewritten with content (B)
/// while slots 2 and 5 hold macros. On the keyboard slot 2 was gone; the
/// edit now writes slots 2 and 5 again right after slot 0, and every macro
/// is kept. Step 3 (clearing slot 0) keeps them too.
#[test]
fn rewriting_or_clearing_macro_slot_0_writes_the_other_macros_again() {
    let resend = Some(ResendReason::MacroSlot0Clears);
    let p1 = profile(1);
    for program in [Some(tap(0x05)), None] {
        let mut sim = edit_sim();
        let dir = temp_dir();
        let saved = safety(&mut sim, &dir);
        let planned = plan(&mut sim, &set_slot0(program.clone())).expect("plan");
        assert_eq!(
            macro_writes(&planned),
            [(0, None, false), (2, resend, true), (5, resend, true)],
            "{program:?}"
        );
        // Only slot 0 changes content; the re-sends name why.
        assert_eq!(planned.slot_changes().len(), 1);
        assert!(
            planned.writes()[1]
                .summary
                .contains("writing macro slot 0 makes the keyboard clear slots 1-9")
        );
        let report = apply(&mut sim, &planned, &saved, |_| ControlFlow::Continue(()));
        assert!(report.is_success(), "{program:?}: {:#?}", report.error);
        // The re-sends found the slots emptied by the slot-0 write (not
        // "changed after planning") and wrote them again.
        for item in report.items.iter().filter(|i| i.resent.is_some()) {
            assert!(
                matches!(item.result, ItemResult::Written { .. }),
                "{:?}",
                item.result
            );
        }
        match &program {
            Some(program) => assert_eq!(kept(&sim, p1, 0), bytes(0, program)),
            None => assert!(is_empty(&kept(&sim, p1, 0))),
        }
        assert_eq!(kept(&sim, p1, 2), bytes(2, &tap(0x06)));
        assert_eq!(kept(&sim, p1, 5), bytes(5, &tap(0x08)));
        assert!(sim.dangling_references(p1, true).is_empty());
        sim.assert_clean();
    }
}

/// Writing slot 1-9 touches only that slot: nothing else is re-sent.
#[test]
fn writing_another_macro_slot_re_sends_nothing() {
    let mut sim = edit_sim();
    let planned = plan(
        &mut sim,
        &KeymapEdit {
            changes: vec![KeymapChange::SetMacroSlot {
                slot: slot(5),
                program: Some(tap(0x09)),
            }],
            ..KeymapEdit::default()
        },
    )
    .expect("plan");
    assert_eq!(macro_writes(&planned), [(5, None, false)]);
}

/// Unbinding the key on slot 0 clears the orphan slot 0 in the same
/// group: the other macros are written again after it, and the base keymap
/// (Del back to default) still goes out after a cancellation.
#[test]
fn unbinding_the_key_on_slot_0_keeps_the_other_macros() {
    let p1 = profile(1);
    let mut sim = edit_sim();
    bind(&mut sim, p1, Layer::Base, "Del", 0);
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let unbind = KeymapEdit {
        changes: vec![KeymapChange::Unbind {
            layer: Layer::Base,
            pos: key("Del"),
        }],
        ..KeymapEdit::default()
    };
    let planned = plan(&mut sim, &unbind).expect("plan");
    let resend = Some(ResendReason::MacroSlot0Clears);
    assert_eq!(
        macro_writes(&planned),
        [(0, None, false), (2, resend, true), (5, resend, true)]
    );
    let base = planned
        .writes()
        .iter()
        .find(|w| w.table == (TableId::Keymap { layer: Layer::Base }))
        .expect("base keymap");
    assert!(base.completes_binding);
    let report = apply(&mut sim, &planned, &saved, |progress| match progress {
        WriteProgress::Writing {
            table: TableId::Macro { slot },
            packet: 0,
            ..
        } if slot.get() == 2 => ControlFlow::Break(()),
        _ => ControlFlow::Continue(()),
    });
    assert!(report.cancel_requested);
    assert!(report.error.is_none(), "{:#?}", report.error);
    assert!(is_empty(&kept(&sim, p1, 0)));
    assert_eq!(kept(&sim, p1, 2), bytes(2, &tap(0x06)));
    assert_eq!(kept(&sim, p1, 5), bytes(5, &tap(0x08)));
    assert!(sim.dangling_references(p1, true).is_empty());
    sim.assert_clean();
}

/// A new macro takes the lowest free slot in 1-9; slot 0 only when 1-9
/// are taken, and then every other macro is written again after it.
#[test]
fn a_new_macro_takes_slot_0_only_when_slots_1_to_9_are_taken() {
    let p1 = profile(1);
    let bind_caps = KeymapEdit {
        changes: vec![KeymapChange::BindMacro {
            layer: Layer::Base,
            pos: key("Caps Lock"),
            program: tap(0x05),
            slot: None,
        }],
        ..KeymapEdit::default()
    };
    // Slot 0 is free (and so is slot 1): slot 1.
    let mut sim = DeviceSim::from_backup(&c06_backup());
    let planned = plan(&mut sim, &bind_caps).expect("plan");
    assert_eq!(macro_writes(&planned), [(1, None, false)]);

    // Slots 1-9 hold macros: slot 0, then 1-9 again.
    let mut sim = DeviceSim::from_backup(&c06_backup());
    for n in 1..10 {
        put_macro(&mut sim, p1, n, &tap(0x04 + n));
    }
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &bind_caps).expect("plan");
    let mut expected = vec![(0, None, false)];
    expected.extend((1..10).map(|n| (n, Some(ResendReason::MacroSlot0Clears), true)));
    assert_eq!(macro_writes(&planned), expected);
    let report = apply(&mut sim, &planned, &saved, |_| ControlFlow::Continue(()));
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(kept(&sim, p1, 0), bytes(0, &tap(0x05)));
    for n in 1..10 {
        assert_eq!(kept(&sim, p1, n), bytes(n, &tap(0x04 + n)), "slot {n}");
    }
    sim.assert_clean();
}

/// Ctrl+C while slot 0 is being sent: the other macros still go out (the
/// slot-0 write already emptied them), and nothing is lost.
#[test]
fn a_cancellation_after_slot_0_still_writes_the_other_macros_again() {
    let p1 = profile(1);
    let mut sim = edit_sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &set_slot0(Some(tap(0x05)))).expect("plan");
    let report = apply(&mut sim, &planned, &saved, |progress| match progress {
        WriteProgress::Writing {
            table: TableId::Macro { slot },
            packet,
            ..
        } if slot.get() == 0 && packet >= 1 => ControlFlow::Break(()),
        _ => ControlFlow::Continue(()),
    });
    assert!(report.cancel_requested);
    assert!(report.error.is_none(), "{:#?}", report.error);
    let resent: Vec<bool> = report
        .items
        .iter()
        .filter(|i| i.resent.is_some())
        .map(|i| i.result.was_sent())
        .collect();
    assert_eq!(resent, [true, true]);
    assert!(report.cleared_macros.is_empty());
    assert_eq!(kept(&sim, p1, 2), bytes(2, &tap(0x06)));
    assert_eq!(kept(&sim, p1, 5), bytes(5, &tap(0x08)));
    assert!(sim.dangling_references(p1, true).is_empty());
    sim.assert_clean();
}

/// A re-send that fails (the slot never takes the write): the report names
/// the macros left empty and the key that now refers to an empty slot.
#[test]
fn a_failed_re_send_reports_the_emptied_macros_and_the_keys_bound_to_them() {
    let p1 = profile(1);
    let mut sim = edit_sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &set_slot0(Some(tap(0x05)))).expect("plan");
    sim.freeze(SimTable::Macro(slot(2)));
    let report = apply(&mut sim, &planned, &saved, |_| ControlFlow::Continue(()));
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::VerifyFailed)
    );
    assert_eq!(
        report.cleared_macros,
        [
            ClearedMacro {
                profile: 1,
                slot: slot(2)
            },
            ClearedMacro {
                profile: 1,
                slot: slot(5)
            }
        ]
    );
    let dangling: Vec<(Layer, u8, SlotRef)> = report
        .dangling
        .iter()
        .map(|d| (d.layer, d.index, d.slot))
        .collect();
    assert_eq!(
        dangling,
        [(Layer::Base, key("W").index(), SlotRef::Macro(slot(2)))]
    );
    assert_eq!(dangling, sim.dangling_references(p1, false));
    sim.assert_clean();
}

/// A slot whose content OpenWraith cannot write again would be lost by a
/// slot-0 write: refused unless allowed explicitly.
#[test]
fn a_macro_that_cannot_be_written_again_blocks_a_slot_0_write() {
    let mut sim = edit_sim();
    // Play mode 5: not the canonical layout.
    let mut odd = vec![0u8; 256];
    odd[..12].copy_from_slice(&[3, 5, 0, 4, 0, 1, 0, 0, 0x04, 0x10, 0, 0]);
    sim.set_table(profile(1), SimTable::Macro(slot(3)), odd);
    let refused = plan(&mut sim, &set_slot0(Some(tap(0x05))));
    assert!(
        matches!(
            refused,
            Err(EditError::UnsupportedCurrentTable {
                table: TableId::Macro { slot },
                ..
            }) if slot.get() == 3
        ),
        "{refused:?}"
    );
}

// ----- restores and imports -----

fn options() -> RestoreOptions {
    RestoreOptions {
        backup: common::fixed_options(),
        ..RestoreOptions::default()
    }
}

/// An all-profile backup of the simulator.
fn snapshot(sim: &mut DeviceSim) -> Backup {
    read_all_profiles(
        &mut Session::new(sim),
        consent(),
        &profiles_options(),
        |_| ControlFlow::Continue(()),
    )
    .expect("all-profile backup")
}

fn prepare(sim: &mut DeviceSim, target: &Backup, options: &RestoreOptions) -> PreparedRestore {
    prepare_restore(&mut Session::new(sim), target, options, consent(), |_| {
        ControlFlow::Continue(())
    })
    .expect("prepared")
}

fn execute(
    sim: &mut DeviceSim,
    prepared: &PreparedRestore,
    progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> RestoreReport {
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety backup");
    execute_restore(
        &mut Session::new(sim),
        prepared,
        &saved,
        consent(),
        progress,
    )
}

/// Profile `p` as in H24 (`hw/v02/h24.txt`): macros in slots 0-4, Del on
/// slot 0, Home on slot 3, Fn + Q on slot 4.
fn h24_profile(sim: &mut DeviceSim, p: ProfileId) {
    for n in 0..5 {
        put_macro(sim, p, n, &tap(0x04 + n));
    }
    bind(sim, p, Layer::Base, "Del", 0);
    bind(sim, p, Layer::Base, "Home", 3);
    bind(sim, p, Layer::Fn, "Q", 4);
}

/// (profile, slot, re-send reason) of every planned macro write.
fn planned_macros(prepared: &PreparedRestore) -> Vec<(u8, u8, Option<ResendReason>)> {
    prepared
        .plan()
        .writes()
        .filter_map(|(p, item, write)| match item.table {
            TableId::Macro { slot } => Some((p.get(), slot.get(), write.resent)),
            _ => None,
        })
        .collect()
}

/// H24 (2026-10-27): a restore of all four profiles whose profiles 3 and 4
/// go from macros in slots 0-4 to none, on profile 4. After slot 0 the
/// keyboard has emptied slots 1-4 already: each later macro write finds
/// them empty and is not needed (not "changed after planning"), keys and
/// slots end as in the backup, and the undo (the safety backup) brings
/// every macro back.
#[test]
fn the_h24_restore_of_profiles_with_macros_in_slots_0_to_4() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let target = snapshot(&mut sim);
    for p in [profile(3), profile(4)] {
        h24_profile(&mut sim, p);
    }
    sim.set_active_profile(profile(4));
    let prepared = prepare(&mut sim, &target, &options());
    let mut expected = Vec::new();
    for p in [3, 4] {
        expected.extend((0..5).map(|n| (p, n, None)));
    }
    assert_eq!(planned_macros(&prepared), expected);
    let undo = prepared.safety().clone();
    let report = execute(&mut sim, &prepared, |_| ControlFlow::Continue(()));
    assert!(report.is_success(), "{:#?}", report.error);
    for p in [3, 4] {
        let results: Vec<bool> = report
            .items()
            .filter(|(profile, i)| *profile == p && matches!(i.table, TableId::Macro { .. }))
            .map(|(_, i)| matches!(i.result, ItemResult::AlreadyMatched))
            .collect();
        assert_eq!(results, [false, true, true, true, true], "profile {p}");
        for n in 0..10 {
            assert!(is_empty(&kept(&sim, profile(p), n)), "profile {p} slot {n}");
        }
        assert!(sim.dangling_references(profile(p), true).is_empty());
    }
    sim.assert_clean();

    // The undo: slot 0 first, then slots 1-4 (each one written: it is
    // empty, as planned).
    let prepared = prepare(&mut sim, &undo, &options());
    let report = execute(&mut sim, &prepared, |_| ControlFlow::Continue(()));
    assert!(report.is_success(), "{:#?}", report.error);
    for p in [profile(3), profile(4)] {
        for n in 0..5 {
            assert_eq!(kept(&sim, p, n), bytes(n, &tap(0x04 + n)), "{p} slot {n}");
        }
    }
    sim.assert_clean();
}

/// A restore whose slots 0 and 2 change, slot 4 is cleared and slots 1 and
/// 3 keep their macros: slot 0, the re-send of 1, the new slot 2, the
/// re-send of 3, the clear of 4. Without the re-sends slots 1 and 3 would
/// be gone, and the slot-2 write would stop with "changed after planning".
#[test]
fn a_restore_that_writes_slot_0_writes_the_unchanged_macros_again() {
    let p2 = profile(2);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    for n in 0..5 {
        put_macro(&mut sim, p2, n, &tap(0x04 + n));
    }
    bind(&mut sim, p2, Layer::Base, "Home", 3);
    // The backup: slot 0 = Z, slot 2 = Y, slot 4 empty.
    let mut source = sim.clone();
    put_macro(&mut source, p2, 0, &tap(0x1d));
    put_macro(&mut source, p2, 2, &tap(0x1c));
    source.set_table(p2, SimTable::Macro(slot(4)), vec![0u8; 256]);
    let target = snapshot(&mut source);
    let prepared = prepare(&mut sim, &target, &options());
    let resend = Some(ResendReason::MacroSlot0Clears);
    assert_eq!(
        planned_macros(&prepared),
        [
            (2, 0, None),
            (2, 1, resend),
            (2, 2, None),
            (2, 3, resend),
            (2, 4, None)
        ]
    );
    // The re-sends change nothing in the plan's eyes; slot 0 is not the
    // binding of anything, the slots after it complete it.
    for (_, item, write) in prepared.plan().writes() {
        if write.resent.is_some() {
            assert!(
                write.changes.is_empty() && write.completes_binding,
                "{item:?}"
            );
            assert!(write.summary.contains("re-sent"));
        }
    }
    let report = execute(&mut sim, &prepared, |_| ControlFlow::Continue(()));
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(kept(&sim, p2, 0), bytes(0, &tap(0x1d)));
    assert_eq!(kept(&sim, p2, 1), bytes(1, &tap(0x05)));
    assert_eq!(kept(&sim, p2, 2), bytes(2, &tap(0x1c)));
    assert_eq!(kept(&sim, p2, 3), bytes(3, &tap(0x07)));
    assert!(is_empty(&kept(&sim, p2, 4)));
    assert!(sim.dangling_references(p2, true).is_empty());
    // Every re-sent slot was read again after a profile switch.
    assert_eq!(
        report
            .items()
            .filter(|(_, i)| i.resent.is_some())
            .filter(|(_, i)| matches!(
                i.result,
                ItemResult::Written {
                    persistence: openwraith_hid::Persistence::Persisted,
                    ..
                }
            ))
            .count(),
        2
    );
    sim.assert_clean();
}

/// Ctrl+C during the slot-0 write of a restore: the re-sends still go out,
/// the clear of slot 4 (already empty) does not, and no macro is lost.
#[test]
fn a_cancelled_restore_still_writes_the_macros_slot_0_emptied() {
    let p1 = profile(1);
    let mut sim = DeviceSim::from_backup(&c06_backup());
    for n in 0..5 {
        put_macro(&mut sim, p1, n, &tap(0x04 + n));
    }
    let mut source = sim.clone();
    put_macro(&mut source, p1, 0, &tap(0x1d));
    source.set_table(p1, SimTable::Macro(slot(4)), vec![0u8; 256]);
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    let prepared = prepare(&mut sim, &target, &options());
    let report = execute(&mut sim, &prepared, |progress| match progress {
        WriteProgress::Writing {
            table: TableId::Macro { slot },
            packet,
            ..
        } if slot.get() == 0 && packet >= 1 => ControlFlow::Break(()),
        _ => ControlFlow::Continue(()),
    });
    assert!(report.cancel_requested);
    assert!(report.cleared_macros.is_empty());
    for n in 1..4 {
        assert_eq!(kept(&sim, p1, n), bytes(n, &tap(0x04 + n)), "slot {n}");
    }
    sim.assert_clean();
}

/// A restore whose re-send fails reports the macros left empty.
#[test]
fn a_restore_whose_re_send_fails_reports_the_emptied_macros() {
    let p1 = profile(1);
    let mut sim = DeviceSim::from_backup(&c06_backup());
    for n in [0, 3] {
        put_macro(&mut sim, p1, n, &tap(0x04 + n));
    }
    bind(&mut sim, p1, Layer::Base, "Home", 3);
    let mut source = sim.clone();
    put_macro(&mut source, p1, 0, &tap(0x1d));
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    let prepared = prepare(&mut sim, &target, &options());
    sim.freeze(SimTable::Macro(slot(3)));
    let report = execute(&mut sim, &prepared, |_| ControlFlow::Continue(()));
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.cleared_macros,
        [ClearedMacro {
            profile: 1,
            slot: slot(3)
        }]
    );
    assert!(
        report
            .dangling
            .iter()
            .any(|d| d.index == key("Home").index() && d.slot == SlotRef::Macro(slot(3)))
    );
    sim.assert_clean();
}

/// An import whose file writes slot 0 into a profile whose other macros
/// equal the file's: they are written again after it, and only the file's
/// changed macro is listed as imported.
#[test]
fn an_import_into_slot_0_keeps_the_profiles_other_macros() {
    let p1 = profile(1);
    let p3 = profile(3);
    let mut source = sim_with_profiles(ScopeConfig::default());
    put_macro(&mut source, p1, 0, &tap(0x1d));
    bind(&mut source, p1, Layer::Base, "Caps Lock", 0);
    for n in [1, 6] {
        put_macro(&mut source, p1, n, &tap(0x04 + n));
    }
    let file = read_all(&mut Session::new(&mut source))
        .expect("backup")
        .export_profile(p1, None)
        .expect("export");
    let mut sim = sim_with_profiles(ScopeConfig::default());
    for n in [1, 6] {
        put_macro(&mut sim, p3, n, &tap(0x04 + n));
    }
    let import = |include: ImportSelection| RestoreOptions {
        import: Some(ImportOptions {
            from: p1,
            into: p3,
            include,
            allow_critical_fn_overrides: false,
        }),
        ..options()
    };
    let keys = ImportSelection {
        triggers: false,
        lighting: false,
        keymap_group: true,
        ..ImportSelection::default()
    };
    let prepared = prepare(&mut sim, &file, &import(keys));
    let resend = Some(ResendReason::MacroSlot0Clears);
    assert_eq!(
        planned_macros(&prepared),
        [(3, 0, None), (3, 1, resend), (3, 6, resend)]
    );
    assert!(
        prepared
            .plan()
            .warnings
            .contains(&PlanWarning::ImportedMacros { slots: vec![0] })
    );
    let report = execute(&mut sim, &prepared, |_| ControlFlow::Continue(()));
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(kept(&sim, p3, 0), bytes(0, &tap(0x1d)));
    assert_eq!(kept(&sim, p3, 1), bytes(1, &tap(0x05)));
    assert_eq!(kept(&sim, p3, 6), bytes(6, &tap(0x0a)));

    // Keys not imported: nothing of the keymap group is written, and no
    // re-send is left over as "not imported".
    let mut sim = sim_with_profiles(ScopeConfig::default());
    put_macro(&mut sim, p3, 1, &tap(0x05));
    let prepared = prepare(&mut sim, &file, &import(ImportSelection::default()));
    assert!(planned_macros(&prepared).is_empty());
    let plan3 = prepared
        .plan()
        .profiles
        .iter()
        .find(|p| p.profile == p3)
        .expect("profile 3");
    let slot1 = plan3
        .items
        .iter()
        .find(|i| i.table == TableId::Macro { slot: slot(1) })
        .expect("slot 1");
    assert_eq!(slot1.action, PlanAction::Unchanged);
}

/// Home on slot 3; the plan writes slot 0 (unbound), clears slot 3 and sets
/// Home back to default. A cancellation after slot 0 skips the clear of
/// slot 3 (slot 0 emptied it already), but still writes the keymap that
/// moves Home off it: no key is left on the emptied slot.
#[test]
fn a_cancellation_after_slot_0_still_moves_keys_off_the_slots_it_emptied() {
    let p1 = profile(1);
    let keyboard = || {
        let mut sim = DeviceSim::from_backup(&c06_backup());
        put_macro(&mut sim, p1, 0, &tap(0x04));
        put_macro(&mut sim, p1, 3, &tap(0x07));
        bind(&mut sim, p1, Layer::Base, "Home", 3);
        sim
    };
    let cancel_at_slot3 = |progress: WriteProgress| match progress {
        WriteProgress::Writing {
            table: TableId::Macro { slot },
            packet: 0,
            ..
        } if slot.get() == 3 => ControlFlow::Break(()),
        _ => ControlFlow::Continue(()),
    };
    let check = |sim: &DeviceSim| {
        assert!(sim.dangling_references(p1, true).is_empty());
        let home = usize::from(key("Home").index()) * 4;
        let base = sim
            .persisted_table(p1, SimTable::Keymap(Layer::Base))
            .expect("keymap");
        assert_eq!(base[home..home + 4], [0, 0, 0, 0]);
        assert_eq!(kept(sim, p1, 0), bytes(0, &tap(0x1d)));
        sim.assert_clean();
    };

    // A restore.
    let mut source = DeviceSim::from_backup(&c06_backup());
    put_macro(&mut source, p1, 0, &tap(0x1d));
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    let mut sim = keyboard();
    let prepared = prepare(&mut sim, &target, &options());
    let report = execute(&mut sim, &prepared, cancel_at_slot3);
    assert!(report.cancel_requested);
    assert!(report.dangling.is_empty(), "{:?}", report.dangling);
    check(&sim);

    // An edit.
    let mut sim = keyboard();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let edit = KeymapEdit {
        changes: vec![
            KeymapChange::SetMacroSlot {
                slot: slot(0),
                program: Some(tap(0x1d)),
            },
            KeymapChange::Unbind {
                layer: Layer::Base,
                pos: key("Home"),
            },
        ],
        ..KeymapEdit::default()
    };
    let planned = plan(&mut sim, &edit).expect("plan");
    let report = apply(&mut sim, &planned, &saved, cancel_at_slot3);
    assert!(report.cancel_requested);
    assert!(report.dangling.is_empty(), "{:?}", report.dangling);
    check(&sim);
}

/// Keyboard: slot 0 = A, slot 3 = D with Home on it. Backup: slot 0 = Z,
/// slot 3 empty, Home back to default (and Fn + Q on slot 0 when
/// `fn_change`). The restore writes slot 0 (the keyboard empties slot 3),
/// finds slot 3 already empty, then the base keymap that moves Home off it.
fn home_on_slot_3(fn_change: bool) -> (DeviceSim, Backup) {
    let p1 = profile(1);
    let mut sim = DeviceSim::from_backup(&c06_backup());
    put_macro(&mut sim, p1, 0, &tap(0x04));
    put_macro(&mut sim, p1, 3, &tap(0x07));
    let mut source = sim.clone();
    bind(&mut sim, p1, Layer::Base, "Home", 3);
    put_macro(&mut source, p1, 0, &tap(0x1d));
    source.set_table(p1, SimTable::Macro(slot(3)), vec![0u8; 256]);
    if fn_change {
        bind(&mut source, p1, Layer::Fn, "Q", 0);
    }
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    (sim, target)
}

/// The keys of a report's dangling list.
fn dangling_keys(report: &RestoreReport) -> Vec<(Layer, u8, SlotRef)> {
    report
        .dangling
        .iter()
        .map(|d| (d.layer, d.index, d.slot))
        .collect()
}

/// The base keymap that moves Home off the slot slot 0 emptied is not kept
/// (its Fn-keymap write fails, or the keyboard drops it): Home goes back to
/// the empty slot 3 at the next profile switch, and the report names it.
#[test]
fn a_base_keymap_the_keyboard_drops_leaves_its_old_keys_on_the_emptied_slots() {
    let p1 = profile(1);
    let home = (Layer::Base, key("Home").index(), SlotRef::Macro(slot(3)));

    // The Fn keymap (which binds Fn + Q) never takes the write: the base
    // keymap is not saved.
    let (mut sim, target) = home_on_slot_3(true);
    let prepared = prepare(&mut sim, &target, &options());
    sim.freeze(SimTable::Keymap(Layer::Fn));
    let report = execute(&mut sim, &prepared, |_| ControlFlow::Continue(()));
    assert_eq!(report.outcome, Outcome::Failed);
    let base = report
        .items()
        .find(|(_, i)| i.table == (TableId::Keymap { layer: Layer::Base }))
        .map(|(_, i)| i.result.clone())
        .expect("base keymap");
    assert!(
        matches!(&base, ItemResult::MaybeLost { warnings, .. }
            if warnings.contains(&openwraith_hid::WriteWarning::BaseKeymapNotSaved)),
        "{base:?}"
    );
    assert_eq!(dangling_keys(&report), [home]);
    assert_eq!(sim.dangling_references(p1, true), [home]);
    sim.assert_clean();

    // The Fn keymap is only re-sent (unchanged), and the keyboard does not
    // save the base keymap: the persistence check reports it lost.
    let (mut sim, target) = home_on_slot_3(false);
    let prepared = prepare(&mut sim, &target, &options());
    sim.freeze(SimTable::Keymap(Layer::Fn));
    let report = execute(&mut sim, &prepared, |_| ControlFlow::Continue(()));
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::NotPersisted)
    );
    assert_eq!(report.lost(), 1);
    assert_eq!(dangling_keys(&report), [home]);
    assert_eq!(sim.dangling_references(p1, true), [home]);
    sim.assert_clean();

    // The same as an edit: slot 0 rewritten, Home unbound (its slot 3 is
    // cleared with it), Fn + Q bound to slot 0; the Fn keymap fails.
    let (mut sim, _) = home_on_slot_3(false);
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let edit = KeymapEdit {
        changes: vec![
            KeymapChange::SetMacroSlot {
                slot: slot(0),
                program: Some(tap(0x1d)),
            },
            KeymapChange::Unbind {
                layer: Layer::Base,
                pos: key("Home"),
            },
            KeymapChange::Assign {
                layer: Layer::Fn,
                pos: key("Q"),
                assignment: openwraith_hid::protocol::KeyAssignment::Macro(slot(0)),
            },
        ],
        ..KeymapEdit::default()
    };
    let planned = plan(&mut sim, &edit).expect("plan");
    sim.freeze(SimTable::Keymap(Layer::Fn));
    let report = apply(&mut sim, &planned, &saved, |_| ControlFlow::Continue(()));
    assert_eq!(report.outcome, Outcome::Failed);
    let said: Vec<(Layer, u8, SlotRef)> = report
        .dangling
        .iter()
        .map(|d| (d.layer, d.index, d.slot))
        .collect();
    assert_eq!(said, [home]);
    assert_eq!(sim.dangling_references(p1, true), [home]);
    sim.assert_clean();
}

/// The base keymap that moves Home off the slot slot 0 emptied fails itself
/// (it never takes the write): its packets went out, but the group stops
/// before the Fn keymap, so the keyboard keeps the old base layer and Home
/// stays on the empty slot 3. The report names it (it used to count the
/// failed base keymap as kept).
#[test]
fn a_base_keymap_that_fails_leaves_its_old_keys_on_the_emptied_slots() {
    let p1 = profile(1);
    let home = (Layer::Base, key("Home").index(), SlotRef::Macro(slot(3)));
    let base_failed = |result: Option<ItemResult>| {
        assert!(
            matches!(
                &result,
                Some(ItemResult::Failed { error, .. }) if error.kind == ErrorKind::VerifyFailed
            ),
            "{result:?}"
        );
    };

    // A restore.
    let (mut sim, target) = home_on_slot_3(false);
    let prepared = prepare(&mut sim, &target, &options());
    sim.freeze(SimTable::Keymap(Layer::Base));
    let report = execute(&mut sim, &prepared, |_| ControlFlow::Continue(()));
    assert_eq!(report.outcome, Outcome::Failed);
    base_failed(
        report
            .items()
            .find(|(_, i)| i.table == (TableId::Keymap { layer: Layer::Base }))
            .map(|(_, i)| i.result.clone()),
    );
    assert_eq!(dangling_keys(&report), [home]);
    assert_eq!(sim.dangling_references(p1, false), [home]);
    assert_eq!(sim.dangling_references(p1, true), [home]);
    sim.assert_clean();

    // The same as an edit: slot 0 rewritten, Home unbound.
    let (mut sim, _) = home_on_slot_3(false);
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let edit = KeymapEdit {
        changes: vec![
            KeymapChange::SetMacroSlot {
                slot: slot(0),
                program: Some(tap(0x1d)),
            },
            KeymapChange::Unbind {
                layer: Layer::Base,
                pos: key("Home"),
            },
        ],
        ..KeymapEdit::default()
    };
    let planned = plan(&mut sim, &edit).expect("plan");
    sim.freeze(SimTable::Keymap(Layer::Base));
    let report = apply(&mut sim, &planned, &saved, |_| ControlFlow::Continue(()));
    assert_eq!(report.outcome, Outcome::Failed);
    base_failed(
        report
            .items
            .iter()
            .find(|i| i.table == (TableId::Keymap { layer: Layer::Base }))
            .map(|i| i.result.clone()),
    );
    let said: Vec<(Layer, u8, SlotRef)> = report
        .dangling
        .iter()
        .map(|d| (d.layer, d.index, d.slot))
        .collect();
    assert_eq!(said, [home]);
    assert_eq!(sim.dangling_references(p1, true), [home]);
    sim.assert_clean();
}

/// Play mode 5: a macro OpenWraith cannot write again (not the canonical
/// layout).
fn odd_macro() -> Vec<u8> {
    let mut odd = vec![0u8; 256];
    odd[..12].copy_from_slice(&[3, 5, 0, 4, 0, 1, 0, 0, 0x04, 0x10, 0, 0]);
    odd
}

/// A restore whose slot 0 differs while slot 3 holds a macro OpenWraith
/// cannot write again (the same on the keyboard and in the backup): writing
/// slot 0 would empty slot 3 for good, so the keymap group is blocked and
/// slot 3 is kept.
#[test]
fn a_macro_that_cannot_be_written_again_blocks_a_restore_of_slot_0() {
    let p1 = profile(1);
    let mut sim = DeviceSim::from_backup(&c06_backup());
    put_macro(&mut sim, p1, 0, &tap(0x04));
    sim.set_table(p1, SimTable::Macro(slot(3)), odd_macro());
    let mut source = sim.clone();
    put_macro(&mut source, p1, 0, &tap(0x1d));
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    let prepared = prepare(&mut sim, &target, &options());
    let slot0 = prepared
        .plan()
        .profiles
        .iter()
        .flat_map(|p| p.items.iter())
        .find(|i| i.table == TableId::Macro { slot: slot(0) })
        .expect("slot 0");
    assert!(
        matches!(
            &slot0.action,
            PlanAction::Skip {
                reason: openwraith_hid::SkipReason::KeymapGroupBlocked {
                    cause: openwraith_hid::BlockCause::NonCanonicalMacro { slot: 3, .. },
                },
                ..
            }
        ),
        "{:?}",
        slot0.action
    );
    assert!(planned_macros(&prepared).is_empty());
    let report = execute(&mut sim, &prepared, |_| ControlFlow::Continue(()));
    assert_eq!(report.outcome, Outcome::NothingToDo, "{:#?}", report.error);
    assert_eq!(kept(&sim, p1, 3), odd_macro());
    sim.assert_clean();
}

/// The connection is lost right after the first packet of the slot-0 write
/// (the keyboard empties slots 1-9 then, GUESS for the chunk): the macros
/// it emptied and that were never written again are listed.
#[test]
fn a_slot_0_write_cut_short_lists_the_macros_it_emptied() {
    let p1 = profile(1);
    let keyboard = || {
        let mut sim = DeviceSim::from_backup(&c06_backup());
        for n in [0, 2, 5] {
            put_macro(&mut sim, p1, n, &tap(0x04 + n));
        }
        sim
    };
    let mut source = keyboard();
    put_macro(&mut source, p1, 0, &tap(0x1d));
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    // The packet count up to the first `19 00` packet of the restore.
    let first = {
        let mut copy = keyboard();
        let prepared = prepare(&mut copy, &target, &options());
        let start = copy.written().len();
        execute(&mut copy, &prepared, |_| ControlFlow::Continue(()));
        start
            + copy.written()[start..]
                .iter()
                .position(|p| p.is_write() && p.as_bytes()[..2] == [0x19, 0x00])
                .expect("a slot-0 write")
            + 1
    };
    let mut sim = keyboard();
    let prepared = prepare(&mut sim, &target, &options());
    sim.disconnect_after_packets(first);
    let report = execute(&mut sim, &prepared, |_| ControlFlow::Continue(()));
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::Disconnected)
    );
    assert_eq!(
        report.cleared_macros,
        [
            ClearedMacro {
                profile: 1,
                slot: slot(2)
            },
            ClearedMacro {
                profile: 1,
                slot: slot(5)
            }
        ]
    );
}

/// `set macro --slot 0` while slot 3 holds a macro OpenWraith cannot write
/// again, with the user's permission to overwrite what the undo cannot
/// bring back: the plan says that slot 3 will be lost.
#[test]
fn an_allowed_slot_0_write_warns_that_the_unwritable_macro_is_lost() {
    let mut sim = edit_sim();
    sim.set_table(profile(1), SimTable::Macro(slot(3)), odd_macro());
    let edit = KeymapEdit {
        allow_not_undoable: true,
        ..set_slot0(Some(tap(0x05)))
    };
    let planned = plan(&mut sim, &edit).expect("plan");
    assert!(
        planned
            .warnings()
            .contains(&openwraith_hid::EditWarning::NotUndoable {
                table: TableId::Macro { slot: slot(3) }
            }),
        "{:?}",
        planned.warnings()
    );
    // Slot 3 is not written again (it cannot be).
    assert_eq!(
        macro_writes(&planned),
        [
            (0, None, false),
            (2, Some(ResendReason::MacroSlot0Clears), true),
            (5, Some(ResendReason::MacroSlot0Clears), true)
        ]
    );
}
