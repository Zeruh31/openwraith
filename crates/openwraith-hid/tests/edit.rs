//! Trigger and main-light edits against the device simulator.

mod common;

use std::ops::ControlFlow;

use common::{c06_backup, consent, pos, profile, temp_dir, write_bytes};
use openwraith_hid::protocol::{Distance, KeyPos, LightMode, Rgb, TriggerWriteMode};
use openwraith_hid::{
    DeviceSim, ErrorKind, ItemResult, LightEdit, Outcome, RestoreError, RtEdit, SavedBackup,
    Session, SimTable, TriggerEdit, WriteOptions, apply_light_edit, apply_trigger_edit,
    plan_light_edit, plan_trigger_edit, read_all,
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
