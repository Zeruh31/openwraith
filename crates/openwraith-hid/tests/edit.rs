//! Trigger and main-light edits against the device simulator.

mod common;

use std::ops::ControlFlow;
use std::time::Duration;

use common::{c06_backup, consent, pos, profile, temp_dir, write_bytes};
use openwraith_hid::protocol::{Distance, KeyPos, LightMode, Rgb, TriggerWriteMode};
use openwraith_hid::{
    CommitSettle, DeviceSim, ErrorKind, ItemResult, LightEdit, Outcome, Persistence, RestoreError,
    RtEdit, SavedBackup, Session, SimTable, TriggerEdit, WriteOptions, WriteProgress,
    apply_light_edit, apply_trigger_edit, plan_light_edit, plan_trigger_edit, read_all,
};

const W: (u8, u8) = (2, 2);
const Q: (u8, u8) = (2, 1);

fn sim() -> DeviceSim {
    DeviceSim::from_backup(&c06_backup())
}

/// A safety backup of the simulator's active profile.
fn safety(sim: &mut DeviceSim, dir: &tempfile::TempDir) -> SavedBackup {
    let backup = {
        let mut session = Session::new(sim);
        read_all(&mut session).expect("backup")
    };
    backup.save_safety(dir.path(), "set").expect("saved")
}

fn trigger_edit(keys: &[(u8, u8)], actuation: Option<u16>, rt: Option<RtEdit>) -> TriggerEdit {
    TriggerEdit {
        keys: keys.iter().map(|(r, c)| pos(*r, *c)).collect(),
        actuation: actuation.map(Distance),
        rapid_trigger: rt,
    }
}

#[test]
fn a_trigger_edit_writes_one_key_per_packet_and_verifies() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let edit = trigger_edit(&[W, Q], Some(60), None);
    let plan = {
        let mut session = Session::new(&mut sim);
        plan_trigger_edit(&mut session, &edit).expect("plan")
    };
    assert_eq!(sim.writes_sent(), 0);
    assert_eq!(plan.profile(), profile(1));
    assert_eq!(plan.keys().len(), 2);
    let w = &plan.keys()[0];
    assert_eq!(w.name, "W");
    assert_eq!(w.current.mode, TriggerWriteMode::RapidTrigger);
    assert_eq!(w.target.actuation, Distance(60));
    assert_eq!(w.target.rt_press, Distance(0x0f));
    assert!(plan.has_changes());
    serde_json::to_value(&plan).expect("plan serializes");

    let report = {
        let mut session = Session::new(&mut sim);
        apply_trigger_edit(
            &mut session,
            &plan,
            &saved,
            consent(),
            &WriteOptions::default(),
            |_| ControlFlow::Continue(()),
        )
    };
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(report.written(), 2);
    assert_eq!(report.active_profile_now, Some(1));
    assert_eq!(
        sim.trigger(profile(1), pos(W.0, W.1)),
        Some([0x0c, 60, 60, 0x0f, 0x0f, 0, 0, 0, 0])
    );
    assert_eq!(
        sim.trigger(profile(1), pos(Q.0, Q.1)),
        Some([0x00, 60, 60, 0x01, 0x01, 0, 0, 0, 0])
    );
    // One key per packet: exactly one mask bit in bytes 6-27.
    let writes = write_bytes(&sim);
    assert_eq!(writes.len(), 2);
    for packet in &writes {
        let bits: u32 = packet[6..28].iter().map(|b| b.count_ones()).sum();
        assert_eq!(bits, 1);
    }
    let json = serde_json::to_value(&report).expect("report serializes");
    assert_eq!(json["outcome"], "completed");
    sim.assert_clean();
}

#[test]
fn rapid_trigger_modes() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let cases = [
        (
            RtEdit::Separate {
                press: Distance(0x0f),
                release: Distance(0x19),
            },
            [0x0d, 0x32, 0x32, 0x0f, 0x19, 0, 0, 0, 0],
        ),
        (
            RtEdit::Same {
                sensitivity: Distance(5),
            },
            [0x0c, 0x32, 0x32, 0x05, 0x05, 0, 0, 0, 0],
        ),
        // Off keeps the press and release values.
        (RtEdit::Off, [0x00, 0x32, 0x32, 0x05, 0x05, 0, 0, 0, 0]),
    ];
    for (rt, expected) in cases {
        let edit = trigger_edit(&[W], None, Some(rt));
        let mut session = Session::new(&mut sim);
        let plan = plan_trigger_edit(&mut session, &edit).expect("plan");
        let report = apply_trigger_edit(
            &mut session,
            &plan,
            &saved,
            consent(),
            &WriteOptions::default(),
            |_| ControlFlow::Continue(()),
        );
        assert!(report.is_success(), "{rt:?}: {:#?}", report.error);
        drop(session);
        assert_eq!(
            sim.trigger(profile(1), pos(W.0, W.1)),
            Some(expected),
            "{rt:?}"
        );
    }
    sim.assert_clean();
}

#[test]
fn invalid_trigger_edits_are_refused_before_anything_is_written() {
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let knob = KeyPos::from_index(15).expect("knob");
    let cases = [
        trigger_edit(&[W], None, None),
        trigger_edit(&[], Some(50), None),
        TriggerEdit {
            keys: vec![knob],
            actuation: Some(Distance(50)),
            rapid_trigger: None,
        },
        // r0c1 is not a key of the W75.
        trigger_edit(&[(0, 1)], Some(50), None),
        // 3.42 mm is beyond the maximum travel.
        trigger_edit(&[W], Some(171), None),
        trigger_edit(
            &[W],
            None,
            Some(RtEdit::Same {
                sensitivity: Distance(0),
            }),
        ),
    ];
    for edit in cases {
        let err = plan_trigger_edit(&mut session, &edit).unwrap_err();
        assert!(
            matches!(err, RestoreError::InvalidRequest { .. }),
            "{edit:?}: {err}"
        );
        assert_eq!(err.kind(), ErrorKind::InvalidRequest);
    }
    drop(session);
    assert_eq!(sim.writes_sent(), 0);
    sim.assert_clean();
}

#[test]
fn an_edit_returns_only_after_the_keyboard_had_time_to_save() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let q = pos(Q.0, Q.1);
    let plan_for = |sim: &mut DeviceSim, units: u16| {
        let mut session = Session::new(sim);
        plan_trigger_edit(&mut session, &trigger_edit(&[Q], Some(units), None)).expect("plan")
    };
    let plan = plan_for(&mut sim, 100);
    let mut events = Vec::new();
    let report = {
        let mut session = Session::new(&mut sim);
        apply_trigger_edit(
            &mut session,
            &plan,
            &saved,
            consent(),
            &WriteOptions::default(),
            |p| {
                events.push(p);
                ControlFlow::Continue(())
            },
        )
    };
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(report.waited_for_commit_ms, Some(2000));
    assert!(
        events.contains(&WriteProgress::WaitingForCommit {
            profile: Some(1),
            wait_ms: 2000
        }),
        "{events:?}"
    );
    // An edit does not switch profiles to prove the save (the lighting
    // stays); it relies on the wait.
    assert!(matches!(
        report.items[0].result,
        ItemResult::Written {
            persistence: Persistence::NotChecked,
            ..
        }
    ));
    assert!(common::selects(&sim).is_empty());
    let last_write = sim
        .written()
        .iter()
        .zip(sim.sent_times())
        .filter(|(p, _)| p.is_write())
        .map(|(_, t)| *t)
        .next_back()
        .expect("a write");
    assert!(sim.elapsed() - last_write >= std::time::Duration::from_secs(2));
    // The wait ends by checking that the edited profile is still active.
    assert_eq!(
        sim.written().last().map(|p| p.as_bytes()[..6].to_vec()),
        Some(vec![0x21, 0, 0, 0, 0x00, 0x11])
    );
    assert!(!sim.has_unsaved_edits());
    // Unplugging the keyboard right after the command keeps the change.
    sim.power_cycle();
    assert_eq!(sim.trigger(profile(1), q).map(|r| r[1]), Some(100));
    assert_eq!(sim.discarded_edits(), 0);

    // Without the wait (test option only), the same replug loses the edit.
    let plan = plan_for(&mut sim, 60);
    let options = WriteOptions {
        commit_settle: CommitSettle::disabled_for_tests(),
        ..WriteOptions::default()
    };
    let report = {
        let mut session = Session::new(&mut sim);
        apply_trigger_edit(&mut session, &plan, &saved, consent(), &options, |_| {
            ControlFlow::Continue(())
        })
    };
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(report.waited_for_commit_ms, None);
    assert!(sim.has_unsaved_edits());
    sim.power_cycle();
    assert_eq!(sim.trigger(profile(1), q).map(|r| r[1]), Some(100));
    assert_eq!(sim.discarded_edits(), 1);
    sim.assert_clean();
}

/// Applies a planned trigger edit with the default options.
fn apply(
    sim: &mut DeviceSim,
    plan: &openwraith_hid::TriggerEditPlan,
    saved: &SavedBackup,
) -> openwraith_hid::EditReport {
    let mut session = Session::new(sim);
    apply_trigger_edit(
        &mut session,
        plan,
        saved,
        consent(),
        &WriteOptions::default(),
        |_| ControlFlow::Continue(()),
    )
}

/// The hardware failure inside an edit: the user switches profiles on the
/// keyboard 300 ms after the write, while the edit waits for the keyboard to
/// save. With a profile report the edit sees it at once; without one (spec
/// Q2: unknown whether the keyboard reports such a switch) it sees it when
/// it reads the active profile at the end of the wait. Either way the write
/// is not reported as done.
#[test]
fn a_switch_on_the_keyboard_during_the_wait_fails_the_edit() {
    let q = pos(Q.0, Q.1);
    for push in [true, false] {
        let mut sim = sim();
        let dir = temp_dir();
        let saved = safety(&mut sim, &dir);
        let before = sim.persisted_trigger(profile(1), q);
        let plan = {
            let mut session = Session::new(&mut sim);
            plan_trigger_edit(&mut session, &trigger_edit(&[Q], Some(60), None)).expect("plan")
        };
        // When the write goes out (on a copy: the run is deterministic).
        let write_at = {
            let mut copy = sim.clone();
            assert!(apply(&mut copy, &plan, &saved).is_success());
            common::last_trigger_write(&copy).1
        };
        let after = write_at + Duration::from_millis(300) - sim.elapsed();
        sim.switch_profile_at(after, profile(2), push);
        let report = apply(&mut sim, &plan, &saved);

        assert_eq!(report.outcome, Outcome::Failed, "push: {push}");
        let error = report.error.as_ref().expect("error");
        assert_eq!(error.kind, ErrorKind::ProfileChanged);
        assert!(
            error
                .message
                .contains("changed to 2 before the keyboard had saved the last write to profile 1"),
            "{error:?}"
        );
        assert!(
            matches!(
                report.items[0].result,
                ItemResult::MaybeLost {
                    sends: 1,
                    packets: 1,
                    ..
                }
            ),
            "{:?}",
            report.items[0].result
        );
        assert_eq!((report.written(), report.maybe_lost()), (0, 1));
        assert!(report.anything_written());
        assert_eq!(report.active_profile_now, Some(2));
        assert_eq!(report.waited_for_commit_ms, None);
        let json = serde_json::to_value(&report).expect("serializes");
        assert_eq!(json["items"][0]["result"]["result"], "maybe_lost");
        // The keyboard did drop it.
        assert_eq!(sim.discarded_edits(), 1);
        assert_eq!(sim.active_profile(), 2);
        assert_eq!(sim.persisted_trigger(profile(1), q), before);
        // A report ends the wait at the switch; a silent switch is found when
        // the wait is over. Nothing is selected or written afterwards.
        let (_, written_at) = common::last_trigger_write(&sim);
        let stopped = sim.elapsed() - written_at;
        if push {
            assert_eq!(stopped, Duration::from_millis(300));
        } else {
            assert_eq!(stopped, Duration::from_secs(2));
        }
        assert_eq!(sim.writes_sent(), 1);
        sim.assert_clean();
    }
}

#[test]
fn a_switch_right_after_the_write_is_not_reported_as_done() {
    // The user presses Fn + 2 just as the edit's last packet went out: the
    // keyboard's push arrives with the reply to the edit's own 21/11 check.
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let plan = {
        let mut session = Session::new(&mut sim);
        plan_trigger_edit(&mut session, &trigger_edit(&[W, Q], Some(60), None)).expect("plan")
    };
    let check_packet = {
        let mut copy = sim.clone();
        assert!(apply(&mut copy, &plan, &saved).is_success());
        // The Q write, its read-back, then the 21/11 check (1-based).
        common::last_trigger_write(&copy).0 + 3
    };
    sim.switch_profile_after_packets(check_packet, profile(2));
    let mut waits = Vec::new();
    let report = {
        let mut session = Session::new(&mut sim);
        apply_trigger_edit(
            &mut session,
            &plan,
            &saved,
            consent(),
            &WriteOptions::default(),
            |p| {
                if let WriteProgress::WaitingForCommit { .. } = p {
                    waits.push(p);
                }
                ControlFlow::Continue(())
            },
        )
    };
    assert_eq!(report.outcome, Outcome::Failed);
    assert!(
        report
            .error
            .as_ref()
            .is_some_and(|e| e.message.contains("may not have been saved")),
        "{:?}",
        report.error
    );
    // Both writes went to the working copy the keyboard reloaded.
    assert!(
        report
            .items
            .iter()
            .all(|i| matches!(i.result, ItemResult::MaybeLost { .. })),
        "{:?}",
        report.items
    );
    assert_eq!(report.active_profile_now, Some(2));
    // No wait for a save that can no longer happen (and none announced for
    // the wrong profile).
    assert!(waits.is_empty(), "{waits:?}");
    assert_eq!(report.waited_for_commit_ms, None);
    assert_eq!(sim.discarded_edits(), 1);
    sim.assert_clean();
}

#[test]
fn unplugging_right_after_an_edit_write_reports_it_as_maybe_lost() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let before = sim.persisted_trigger(profile(1), pos(W.0, W.1));
    let plan = {
        let mut session = Session::new(&mut sim);
        plan_trigger_edit(&mut session, &trigger_edit(&[W, Q], Some(60), None)).expect("plan")
    };
    let check_packet = {
        let mut copy = sim.clone();
        assert!(apply(&mut copy, &plan, &saved).is_success());
        common::last_trigger_write(&copy).0 + 3
    };
    // The cable is pulled while the edit checks the active profile, right
    // after the last write.
    sim.disconnect_after_packets(check_packet);
    let report = apply(&mut sim, &plan, &saved);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::Disconnected)
    );
    assert!(
        report
            .items
            .iter()
            .all(|i| matches!(i.result, ItemResult::MaybeLost { .. })),
        "{:?}",
        report.items
    );
    assert_eq!(report.waited_for_commit_ms, None);
    // The unplug was a power loss within the commit window: the keyboard
    // comes back without the edits.
    sim.power_cycle();
    assert_eq!(sim.discarded_edits(), 1);
    assert_eq!(sim.persisted_trigger(profile(1), pos(W.0, W.1)), before);
    sim.assert_clean();
}

#[test]
fn an_edit_without_changes_writes_nothing() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    // W already has 1.00 mm.
    let edit = trigger_edit(&[W], Some(0x32), None);
    let mut session = Session::new(&mut sim);
    let plan = plan_trigger_edit(&mut session, &edit).expect("plan");
    assert!(!plan.has_changes());
    let report = apply_trigger_edit(
        &mut session,
        &plan,
        &saved,
        consent(),
        &WriteOptions::default(),
        |_| ControlFlow::Continue(()),
    );
    assert_eq!(report.outcome, Outcome::NothingToDo);
    drop(session);
    assert_eq!(sim.writes_sent(), 0);
}

#[test]
fn an_edit_detects_changes_after_planning_and_needs_a_matching_safety_backup() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let edit = trigger_edit(&[W], Some(70), None);
    let plan = {
        let mut session = Session::new(&mut sim);
        plan_trigger_edit(&mut session, &edit).expect("plan")
    };
    // Someone else changes W meanwhile.
    sim.set_trigger(profile(1), pos(W.0, W.1), [0, 0x40, 0x40, 1, 1, 0, 0, 0, 0]);
    let report = {
        let mut session = Session::new(&mut sim);
        apply_trigger_edit(
            &mut session,
            &plan,
            &saved,
            consent(),
            &WriteOptions::default(),
            |_| ControlFlow::Continue(()),
        )
    };
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::ChangedSincePlan)
    );
    assert!(matches!(report.items[0].result, ItemResult::Failed { .. }));
    assert_eq!(sim.writes_sent(), 0);

    // A safety backup of another profile does not cover the edit.
    let mut other = sim.clone();
    other.set_active_profile(profile(2));
    let other_saved = safety(&mut other, &dir);
    let plan = {
        let mut session = Session::new(&mut sim);
        plan_trigger_edit(&mut session, &edit).expect("plan")
    };
    let report = {
        let mut session = Session::new(&mut sim);
        apply_trigger_edit(
            &mut session,
            &plan,
            &other_saved,
            consent(),
            &WriteOptions::default(),
            |_| ControlFlow::Continue(()),
        )
    };
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::InvalidBackup)
    );
    assert_eq!(sim.writes_sent(), 0);
    sim.assert_clean();
}

#[test]
fn a_light_edit_sends_the_leave_only_on_a_mode_change() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let apply = |sim: &mut DeviceSim, edit: LightEdit| {
        let mut session = Session::new(sim);
        let plan = plan_light_edit(&mut session, &edit).expect("plan");
        let report = apply_light_edit(
            &mut session,
            &plan,
            &saved,
            consent(),
            &WriteOptions::default(),
            |_| ControlFlow::Continue(()),
        );
        assert!(report.is_success(), "{edit:?}: {:#?}", report.error);
        assert!(session.foreign_activity().is_quiet());
        plan
    };

    // Static blue: a mode change (Custom -> Static).
    let plan = apply(
        &mut sim,
        LightEdit {
            mode: Some(LightMode::from_id(0x01)),
            foreground: Some(Rgb::new(0, 0, 0xff)),
            ..LightEdit::default()
        },
    );
    assert!(plan.changes_mode());
    let light = sim
        .table(profile(1), SimTable::MainLight)
        .expect("light")
        .to_vec();
    assert_eq!(light[0], 0x01);
    assert_eq!(&light[3..6], &[0, 0, 0xff]);
    // Aurora sets its default direction (4).
    let plan = apply(
        &mut sim,
        LightEdit {
            mode: Some(LightMode::Aurora),
            ..LightEdit::default()
        },
    );
    assert_eq!(plan.target().direction(), 4);
    // Brightness only: no leave.
    let plan = apply(
        &mut sim,
        LightEdit {
            brightness: Some(1),
            ..LightEdit::default()
        },
    );
    assert!(!plan.changes_mode());
    let cmds: Vec<u8> = write_bytes(&sim).iter().map(|b| b[0]).collect();
    assert_eq!(cmds, [0x17, 0x07, 0x17, 0x07, 0x07]);
    sim.assert_clean();
}

#[test]
fn invalid_light_edits_are_refused() {
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    for edit in [
        LightEdit::default(),
        // Music rhythm is never written.
        LightEdit {
            mode: Some(LightMode::MusicRhythm),
            ..LightEdit::default()
        },
        // Not in the W75's mode list.
        LightEdit {
            mode: Some(LightMode::from_id(0x05)),
            ..LightEdit::default()
        },
        LightEdit {
            brightness: Some(5),
            ..LightEdit::default()
        },
    ] {
        let err = plan_light_edit(&mut session, &edit).unwrap_err();
        assert!(
            matches!(err, RestoreError::InvalidRequest { .. }),
            "{edit:?}: {err}"
        );
    }
    drop(session);
    assert_eq!(sim.writes_sent(), 0);
}
