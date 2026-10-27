//! SOCD edits and the keyboard's own SOCD switch (Fn + Right Shift), the
//! automatic profile switcher, against the simulator. Packets are compared
//! with capture c15.

mod common;

use std::ops::ControlFlow;
use std::time::Duration;

use common::{c06_backup, consent, load, profile, selects, temp_dir, write_bytes};
use openwraith_hid::protocol::{
    DeviceEvent, HidUsage, KeymapData, Layer, ReadRequest, SocdMode, SocdPair, TableId,
    WriteRequest,
};
use openwraith_hid::{
    ConsentOrigin, DeviceSim, EditOptions, EditWarning, ErrorKind, ItemResult,
    PersistenceCheckMode, SavedBackup, Session, SimTable, SocdEdit, SocdState, WriteError,
    WriteWarning, apply_socd_edit, plan_socd_edit, read_all,
};

const C15: &str = "capture_15_socd";

fn sim() -> DeviceSim {
    DeviceSim::from_backup(&c06_backup())
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn usage(u: u8) -> HidUsage {
    HidUsage::new(u).expect("offered usage")
}

/// A/D with `mode`.
fn a_d(mode: SocdMode) -> SocdPair {
    SocdPair {
        mode,
        key1: usage(0x04),
        key2: usage(0x07),
    }
}

/// The OUT packets of c15 at these entries.
fn captured(indices: &[usize]) -> Vec<[u8; 63]> {
    let capture = load(C15);
    indices
        .iter()
        .map(|index| {
            capture
                .out_packets()
                .find(|(i, _)| i == index)
                .map(|(_, bytes)| *bytes)
                .unwrap_or_else(|| panic!("no OUT packet #{index} in c15"))
        })
        .collect()
}

fn safety(sim: &mut DeviceSim, dir: &tempfile::TempDir) -> SavedBackup {
    let backup = read_all(&mut Session::new(sim)).expect("backup");
    backup.save_safety(dir.path(), "set").expect("saved")
}

fn edit(sim: &mut DeviceSim, edit: &SocdEdit, options: &EditOptions) -> openwraith_hid::EditReport {
    let dir = temp_dir();
    let saved = safety(sim, &dir);
    let mut session = Session::new(&mut *sim);
    let plan = plan_socd_edit(&mut session, edit).expect("plan");
    apply_socd_edit(&mut session, &plan, &saved, consent(), options, |_| {
        ControlFlow::Continue(())
    })
}

#[test]
fn switching_socd_on_with_a_pair_sends_the_captured_burst_and_checks_the_save() {
    let mut sim = sim();
    let report = edit(
        &mut sim,
        &SocdEdit {
            allow_not_undoable: false,
            power: Some(true),
            pairs: Some(vec![a_d(SocdMode::M1)]),
        },
        &EditOptions::default(),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    // c15 #145-#147: power on, A/D Model1 (wire 0), the empty chunk 1.
    let writes: Vec<[u8; 63]> = write_bytes(&sim)
        .into_iter()
        .filter(|b| b[0] == 0x24)
        .collect();
    assert_eq!(writes, captured(&[145, 146, 147]));
    // SOCD is not known to be kept after the settle: the edit switched to
    // profile 2 and back and read it again.
    assert_eq!(selects(&sim), [2, 1]);
    let check = report.persistence.as_ref().expect("checked");
    assert_eq!((check.persisted, check.lost), (1, 0));
    assert!(matches!(
        report.items[0].result,
        ItemResult::Written {
            persistence: openwraith_hid::Persistence::Persisted,
            ..
        }
    ));
    assert!(report.warnings.contains(&EditWarning::ScopeUnknown {
        table: TableId::Socd
    }));
    assert!(report.warnings.contains(&EditWarning::SocdHotkey));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, EditWarning::Unverified(note) if note.question == "Q35"))
    );
    let p1 = profile(1);
    assert_eq!(
        sim.persisted_table(p1, SimTable::SocdSettings)
            .map(|b| b[0]),
        Some(1)
    );
    assert_eq!(
        sim.persisted_table(p1, SimTable::SocdPairs)
            .map(|b| b[..4].to_vec()),
        Some(vec![0x01, 0x00, 0x04, 0x07])
    );
    sim.assert_clean();
}

#[test]
fn a_power_only_change_re_sends_the_unchanged_pairs() {
    // c15 #265-#267: on, no pairs; #272-#274: off, no pairs.
    let mut sim = sim();
    let never = EditOptions {
        persistence: PersistenceCheckMode::Never,
        ..EditOptions::default()
    };
    let on = edit(
        &mut sim,
        &SocdEdit {
            allow_not_undoable: false,
            power: Some(true),
            pairs: None,
        },
        &never,
    );
    assert!(on.is_success(), "{:#?}", on.error);
    let off = edit(
        &mut sim,
        &SocdEdit {
            allow_not_undoable: false,
            power: Some(false),
            pairs: None,
        },
        &never,
    );
    assert!(off.is_success(), "{:#?}", off.error);
    assert_eq!(write_bytes(&sim), captured(&[265, 266, 267, 272, 273, 274]));
    // No profile switch when the check is turned off.
    assert!(selects(&sim).is_empty());
    sim.assert_clean();
}

#[test]
fn mode_2_is_wire_value_1() {
    let mut sim = sim();
    let report = edit(
        &mut sim,
        &SocdEdit {
            allow_not_undoable: false,
            power: Some(true),
            pairs: Some(vec![a_d(SocdMode::M2)]),
        },
        &EditOptions {
            persistence: PersistenceCheckMode::Never,
            ..EditOptions::default()
        },
    );
    assert!(report.is_success(), "{:#?}", report.error);
    // c15 #246-#248.
    assert_eq!(write_bytes(&sim), captured(&[246, 247, 248]));
    sim.assert_clean();
}

#[test]
fn nothing_to_write_when_the_keyboard_already_holds_it() {
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let plan = plan_socd_edit(
        &mut session,
        &SocdEdit {
            allow_not_undoable: false,
            power: Some(false),
            pairs: Some(Vec::new()),
        },
    )
    .expect("plan");
    assert!(!plan.has_changes());
    // Only 24 02 and 24 01 are read for SOCD (the status bytes are not the
    // power, spec 7.4).
    drop(session);
    assert!(write_bytes(&sim).is_empty());
    sim.assert_clean();
}

#[test]
fn an_acknowledged_power_write_is_taken_as_ours() {
    let mut sim = sim();
    sim.set_socd_power_acked(true);
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let mut session = Session::new(&mut sim);
    let plan = plan_socd_edit(
        &mut session,
        &SocdEdit {
            allow_not_undoable: false,
            power: Some(true),
            pairs: None,
        },
    )
    .expect("plan");
    let report = apply_socd_edit(
        &mut session,
        &plan,
        &saved,
        consent(),
        &EditOptions {
            persistence: PersistenceCheckMode::Never,
            ..EditOptions::default()
        },
        |_| ControlFlow::Continue(()),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    match &report.items[0].result {
        ItemResult::Written { warnings, .. } => {
            assert!(
                warnings.contains(&WriteWarning::UnexpectedAck { packet: 1 }),
                "{warnings:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(session.foreign_activity().is_quiet());
    assert_eq!(session.stats().foreign_acks, 0);
    drop(session);
    sim.assert_clean();
}

#[test]
fn fn_right_shift_during_the_write_stops_without_writing_again() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let socd = SocdEdit {
        allow_not_undoable: false,
        power: Some(true),
        pairs: Some(vec![a_d(SocdMode::M1)]),
    };
    let run = |sim: &mut DeviceSim| {
        let mut session = Session::new(sim);
        let plan = plan_socd_edit(&mut session, &socd).expect("plan");
        apply_socd_edit(
            &mut session,
            &plan,
            &saved,
            consent(),
            &EditOptions::default(),
            |_| ControlFlow::Continue(()),
        )
    };
    // When the power packet goes out (from a copy), and the press 10 ms
    // later, inside the listening window after it.
    let mut copy = sim.clone();
    let start = copy.written().len();
    run(&mut copy);
    let power_at = copy.written()[start..]
        .iter()
        .zip(&copy.sent_times()[start..])
        .find(|(p, _)| p.is_write() && p.as_bytes()[..2] == [0x24, 0x03])
        .map(|(_, t)| *t)
        .expect("the power packet");
    sim.press_socd_hotkey_at(power_at - sim.elapsed() + ms(10));
    let report = run(&mut sim);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::ChangedSincePlan),
        "{:#?}",
        report.error
    );
    // One burst, never a second one over the user's switch.
    let bursts = write_bytes(&sim)
        .iter()
        .filter(|b| b[..2] == [0x24, 0x03])
        .count();
    assert_eq!(bursts, 1);
    // The keyboard keeps the user's switch (off again).
    assert_eq!(
        sim.table(profile(1), SimTable::SocdSettings).map(|b| b[0]),
        Some(0)
    );
    sim.assert_clean();
}

#[test]
fn the_socd_push_is_a_keyboard_event_not_another_program() {
    let mut sim = sim();
    sim.press_socd_hotkey_at(ms(500));
    let mut session = Session::new(&mut sim);
    let heard = session.listen(Duration::from_secs(1)).expect("listen");
    assert!(heard.activity.is_quiet());
    assert_eq!(session.socd_pushes(), 1);
    assert!(matches!(
        session.events(),
        [DeviceEvent::SocdPower { power: 1, .. }]
    ));
    // The gate passes: no other program.
    let writer = session.writer(consent()).expect("writer");
    drop(writer);
    // The state read afterwards shows the switch.
    let state = SocdState::read(&mut session).expect("state");
    assert_eq!(state.power, 1);
    assert_eq!(state.pairs, Some(Vec::new()));
    drop(session);
    sim.assert_clean();
}

#[test]
fn a_push_during_our_settings_read_is_taken_as_the_reply() {
    let mut sim = sim();
    // The keyboard's current settings, pushed right after our 24 02 read.
    let push = common::report(&[0x24, 0x02, 0, 0, 0x06, 0, 0, 0, 0, 0xfa, 0xe5]);
    let at = sim.written().len() + 1;
    sim.inject_after_packets(at, push);
    let mut session = Session::new(&mut sim);
    let state = SocdState::read(&mut session).expect("state");
    assert_eq!(state.power, 0);
    assert!(session.foreign_activity().is_quiet());
    drop(session);
    sim.assert_clean();
}

#[test]
fn pairs_in_another_form_can_only_be_replaced() {
    let mut sim = sim();
    // A gap between pairs (entry 1 empty, entry 2 used).
    let mut pairs = vec![0u8; 80];
    pairs[..4].copy_from_slice(&[0x01, 0x00, 0x04, 0x07]);
    pairs[8..12].copy_from_slice(&[0x01, 0x00, 0x1a, 0x16]);
    sim.set_table(profile(1), SimTable::SocdPairs, pairs);
    let mut session = Session::new(&mut sim);
    let keep = plan_socd_edit(
        &mut session,
        &SocdEdit {
            allow_not_undoable: false,
            power: Some(true),
            pairs: None,
        },
    );
    assert!(matches!(
        keep,
        Err(openwraith_hid::EditError::UnsupportedCurrentTable {
            table: TableId::Socd,
            ..
        })
    ));
    let state = SocdState::read(&mut session).expect("state");
    assert_eq!(state.pairs, None);
    assert!(state.unwritable.is_some());
    // Replacing them cannot be undone (a restore writes only the site's
    // form): refused unless acknowledged.
    let refused = plan_socd_edit(
        &mut session,
        &SocdEdit {
            allow_not_undoable: false,
            power: None,
            pairs: Some(vec![a_d(SocdMode::M4)]),
        },
    );
    assert!(matches!(
        refused,
        Err(openwraith_hid::EditError::UnsupportedCurrentTable {
            table: TableId::Socd,
            ..
        })
    ));
    let replace = plan_socd_edit(
        &mut session,
        &SocdEdit {
            allow_not_undoable: true,
            power: None,
            pairs: Some(vec![a_d(SocdMode::M4)]),
        },
    )
    .expect("plan");
    assert!(replace.has_changes());
    assert!(replace.warnings().contains(&EditWarning::NotUndoable {
        table: TableId::Socd
    }));
    assert!(
        replace
            .warnings()
            .iter()
            .any(|w| matches!(w, EditWarning::Unverified(n) if n.what.contains("modes 3 and 4")))
    );
    // Site rules: the same key twice, a key in two pairs.
    let same = plan_socd_edit(
        &mut session,
        &SocdEdit {
            allow_not_undoable: true,
            power: None,
            pairs: Some(vec![SocdPair {
                mode: SocdMode::M1,
                key1: usage(0x04),
                key2: usage(0x04),
            }]),
        },
    );
    assert!(matches!(same, Err(openwraith_hid::EditError::Build(_))));
    drop(session);
    sim.assert_clean();
}

// ---------------------------------------------------------------------------
// The automatic profile switcher (standing rules).
// ---------------------------------------------------------------------------

#[test]
fn the_switcher_selects_with_the_standing_rule_consent_after_the_settle() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let mut session = Session::new(&mut sim);
    // A trigger write without the wait (a GUI's edit that just returned
    // would have waited; this proves the switcher waits on its own).
    let backup = saved.backup().clone();
    let w = common::pos(2, 2);
    let global = backup.trigger_global().expect("21/04").clone();
    let limits =
        openwraith_hid::protocol::TriggerLimits::new(&openwraith_hid::protocol::W75, &global);
    let setting = openwraith_hid::protocol::TriggerSetting {
        mode: openwraith_hid::protocol::TriggerWriteMode::Off,
        actuation: openwraith_hid::protocol::Distance(40),
        rt_press: openwraith_hid::protocol::Distance(1),
        rt_release: openwraith_hid::protocol::Distance(1),
    };
    let write = openwraith_hid::protocol::TriggerWrite::new(
        &openwraith_hid::protocol::W75,
        openwraith_hid::protocol::KeyMask::from_positions([w]),
        setting,
        &limits,
    )
    .expect("valid");
    {
        let mut writer = session.writer(consent()).expect("writer");
        writer
            .write_guarded(&saved, &WriteRequest::Trigger(write))
            .expect("written");
    }
    let mut waits = Vec::new();
    {
        let mut switcher = session.profile_switcher().expect("switcher");
        assert_eq!(switcher.consent().origin(), ConsentOrigin::StandingRule);
        let outcome = switcher
            .switch_to(profile(3), &mut |wait| waits.push(wait))
            .expect("switched");
        assert!(outcome.switched);
    }
    assert_eq!(waits.len(), 1);
    drop(session);
    // The keyboard kept the write: the switch came after the settle.
    assert_eq!(sim.persisted_trigger(profile(1), w).map(|r| r[1]), Some(40));
    assert_eq!(sim.active_profile(), 3);
    assert_eq!(sim.discarded_edits(), 0);
    sim.assert_clean();
}

#[test]
fn the_switcher_refuses_while_a_base_keymap_waits_for_its_fn_keymap() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let mut session = Session::new(&mut sim);
    let raw = saved
        .backup()
        .response(ReadRequest::Keymap(Layer::Base))
        .expect("base keymap")
        .clone();
    let (_, mut data) = KeymapData::from_response(&raw).expect("keymap");
    data.set_entry(common::pos(2, 2), [0x00, 0x04, 0x00, 0x00]);
    {
        let mut writer = session.writer(consent()).expect("writer");
        writer
            .write_guarded(&saved, &WriteRequest::Keymap(Layer::Base, data))
            .expect("written");
        assert!(writer.base_keymap_unsaved());
    }
    assert!(session.base_keymap_unsaved());
    let selects_before = selects(&sim_of(&session));
    {
        let mut switcher = session.profile_switcher().expect("switcher");
        let refused = switcher.switch_to(profile(2), &mut |_| {});
        assert!(matches!(refused, Err(WriteError::BaseKeymapUnsaved)));
    }
    assert_eq!(selects(&sim_of(&session)), selects_before);
    drop(session);
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

/// The simulator behind a session (for checks in the middle of a test).
fn sim_of(session: &Session<&mut DeviceSim>) -> DeviceSim {
    (**session.transport()).clone()
}

#[test]
fn the_switcher_refuses_after_another_program_and_while_a_stray_stream_runs() {
    // Another program's heartbeat: the gate refuses the switcher itself.
    let mut sim = sim();
    sim.push_input(common::captured_status_report(common::C06));
    let mut session = Session::new(&mut sim);
    assert!(matches!(
        session.profile_switcher().err(),
        Some(WriteError::OtherProgramActive { .. })
    ));
    drop(session);
    assert!(write_bytes(&sim).is_empty());

    // A stray key-depth stream.
    let mut sim = self::sim();
    sim.leave_travel_open();
    sim.travel_script(common::pos(2, 2), &[(ms(300), 30)]);
    let mut session = Session::new(&mut sim);
    let mut switcher = session.profile_switcher().expect("not another program");
    assert!(matches!(
        switcher.switch_to(profile(2), &mut |_| {}),
        Err(WriteError::TravelTestOpen { .. })
    ));
    drop(switcher);
    drop(session);
    assert!(write_bytes(&sim).is_empty());
    sim.assert_clean();
}

/// The virtual time of the last SOCD write packet the simulator received
/// from write packet `from` on.
fn last_socd_write_at(sim: &DeviceSim, from: usize) -> Duration {
    sim.written()[from..]
        .iter()
        .zip(&sim.sent_times()[from..])
        .filter(|(p, _)| p.is_write() && p.as_bytes()[0] == 0x24)
        .map(|(_, t)| *t)
        .next_back()
        .expect("a SOCD write")
}

#[test]
fn a_switch_on_the_keyboard_after_the_write_is_not_reported_lost() {
    // SOCD on with A/D; the user switches it on the keyboard at various
    // moments after the burst: during the settle wait, around the
    // persistence check's profile switches and before its read.
    let socd = SocdEdit {
        allow_not_undoable: false,
        power: None,
        pairs: Some(vec![a_d(SocdMode::M1)]),
    };
    let base = sim();
    let mut copy = base.clone();
    let start = copy.written().len();
    let report = edit(&mut copy, &socd, &EditOptions::default());
    assert!(report.is_success(), "{:#?}", report.error);
    let burst = last_socd_write_at(&copy, start);
    let end = copy.elapsed();
    let mut not_checked = 0;
    let mut offset = ms(50);
    while burst + offset < end + ms(200) {
        let mut sim = base.clone();
        sim.press_socd_hotkey_at(burst + offset - sim.elapsed());
        let report = edit(&mut sim, &socd, &EditOptions::default());
        let kind = report.error.as_ref().map(|e| e.kind);
        assert_ne!(
            kind,
            Some(ErrorKind::NotPersisted),
            "pressed {offset:?} after the burst: {:#?}",
            report.error
        );
        assert!(
            report.is_success() || kind == Some(ErrorKind::ChangedSincePlan),
            "pressed {offset:?} after the burst: {:#?}",
            report.error
        );
        if let ItemResult::Written {
            persistence: openwraith_hid::Persistence::NotChecked,
            warnings,
            ..
        } = &report.items[0].result
        {
            assert!(
                warnings
                    .iter()
                    .any(|w| matches!(w, WriteWarning::SocdToggledOnKeyboard { .. })),
                "{warnings:?}"
            );
            not_checked += 1;
        }
        assert_eq!(report.lost(), 0);
        offset += ms(250);
    }
    assert!(not_checked > 0, "no press landed between write and check");
}

#[test]
fn a_switch_on_the_keyboard_after_a_restored_socd_write_is_not_reported_lost() {
    let mut source = sim();
    source.set_table(
        profile(1),
        SimTable::SocdSettings,
        vec![1, 0, 0, 0, 0xfa, 0xe5],
    );
    let mut pairs = vec![0u8; 80];
    pairs[..4].copy_from_slice(&[0x01, 0x00, 0x04, 0x07]);
    source.set_table(profile(1), SimTable::SocdPairs, pairs);
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    let restore = |sim: &mut DeviceSim| {
        let mut session = Session::new(sim);
        let prepared = openwraith_hid::prepare_restore(
            &mut session,
            &target,
            &openwraith_hid::RestoreOptions::default(),
            consent(),
            |_| ControlFlow::Continue(()),
        )
        .expect("prepared");
        let dir = temp_dir();
        let saved = prepared
            .safety()
            .save_safety(dir.path(), "restore")
            .expect("saved");
        openwraith_hid::execute_restore(&mut session, &prepared, &saved, consent(), |_| {
            ControlFlow::Continue(())
        })
    };
    let base = sim();
    let mut copy = base.clone();
    let start = copy.written().len();
    let report = restore(&mut copy);
    assert!(report.is_success(), "{:#?}", report.error);
    let burst = last_socd_write_at(&copy, start);
    for (offset, before_the_check) in [
        (ms(500), true),
        (ms(1500), true),
        (ms(2500), false),
        (ms(3500), false),
    ] {
        let mut sim = base.clone();
        sim.press_socd_hotkey_at(burst + offset - sim.elapsed());
        let report = restore(&mut sim);
        assert!(
            report.is_success(),
            "pressed {offset:?} after the burst: {:#?}",
            report.error
        );
        assert_eq!(report.lost(), 0);
        let socd = report
            .items()
            .find(|(_, i)| i.table == TableId::Socd)
            .map(|(_, i)| i.result.clone())
            .expect("SOCD item");
        // Before the persistence check read it again: not checked, with
        // the warning; later: persisted (the read came first).
        let not_checked = matches!(
            &socd,
            ItemResult::Written {
                persistence: openwraith_hid::Persistence::NotChecked,
                warnings,
                ..
            } if warnings.iter().any(|w| matches!(w, WriteWarning::SocdToggledOnKeyboard { .. }))
        );
        if before_the_check {
            assert!(not_checked, "pressed {offset:?} after the burst: {socd:#?}");
        }
    }
}

#[test]
fn a_standing_rule_consent_only_ever_selects_profiles() {
    let rule = openwraith_hid::WriteConsent::new(ConsentOrigin::StandingRule);
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let base = saved
        .backup()
        .response(ReadRequest::Keymap(Layer::Fn))
        .expect("Fn keymap")
        .clone();
    let (_, fn_data) = KeymapData::from_response(&base).expect("keymap");
    let trigger = openwraith_hid::CommitTrigger::try_from(WriteRequest::Keymap(Layer::Fn, fn_data))
        .expect("trigger");
    let mut session = Session::new(&mut sim);
    {
        let mut writer = session.writer(rule).expect("the gate itself is open");
        // A table write, a commit trigger, the travel test: refused.
        let socd = openwraith_hid::protocol::SocdWrite::new(
            true,
            openwraith_hid::protocol::SocdPairsData::from_pairs(
                &openwraith_hid::protocol::W75,
                &[a_d(SocdMode::M1)],
            )
            .expect("pairs"),
        );
        assert!(matches!(
            writer.write_guarded(&saved, &WriteRequest::Socd(socd)),
            Err(WriteError::ConsentOnlyForSelects)
        ));
        assert!(matches!(
            writer.write_commit_trigger(&saved, &trigger),
            Err(WriteError::ConsentOnlyForSelects)
        ));
        assert!(matches!(
            writer.open_travel_test().map(|_| ()),
            Err(WriteError::ConsentOnlyForSelects)
        ));
        assert!(matches!(
            writer.close_stray_travel_test(),
            Err(WriteError::ConsentOnlyForSelects)
        ));
        assert_eq!(
            WriteError::ConsentOnlyForSelects.kind(),
            ErrorKind::InvalidRequest
        );
        // Selects still work.
        let selected = writer.select_profile(profile(2)).expect("selected");
        assert!(selected.switched);
        writer.select_profile(profile(1)).expect("back");
    }
    // The edits and the restore refuse it before anything is sent.
    let writes_before = write_bytes(&sim_of(&session)).len();
    let plan = plan_socd_edit(
        &mut session,
        &SocdEdit {
            power: Some(true),
            ..SocdEdit::default()
        },
    )
    .expect("plan");
    let report = apply_socd_edit(
        &mut session,
        &plan,
        &saved,
        rule,
        &EditOptions::default(),
        |_| ControlFlow::Continue(()),
    );
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::InvalidRequest)
    );
    let prepared = openwraith_hid::prepare_restore(
        &mut session,
        saved.backup(),
        &openwraith_hid::RestoreOptions::default(),
        consent(),
        |_| ControlFlow::Continue(()),
    )
    .expect("prepared");
    let undo_dir = temp_dir();
    let undo_saved = prepared
        .safety()
        .save_safety(undo_dir.path(), "restore")
        .expect("saved");
    let report =
        openwraith_hid::execute_restore(&mut session, &prepared, &undo_saved, rule, |_| {
            ControlFlow::Continue(())
        });
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::InvalidRequest),
        "{report:#?}"
    );
    let travel = openwraith_hid::live_travel(
        &mut session,
        rule,
        &openwraith_hid::TravelOptions::default(),
        |_| ControlFlow::Break(()),
    );
    assert!(matches!(travel, Err(WriteError::ConsentOnlyForSelects)));
    let after: Vec<[u8; 63]> = write_bytes(&sim_of(&session));
    // Only the two selects above went out; nothing after them.
    assert!(
        after[..writes_before]
            .iter()
            .all(|b| b[..6] == [0x21, 0, 0, 0, 0, 0x12]),
        "{after:02x?}"
    );
    assert_eq!(after.len(), writes_before);
    drop(session);
    sim.assert_clean();
}
