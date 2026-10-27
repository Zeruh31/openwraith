//! Trigger edits.

use openwraith_hid::protocol::ReadRequest;
use openwraith_hid::{Backup, DeviceSim, Session};

use super::harness::{Harness, pos, profile};
use super::seeds::other_unit_backup;
use crate::dto::{
    AppErrorKind, ErrorDetail, Headline, IntentField, ItemResultView, NotEditable, PersistenceView,
    ProgressStep, RangeField, ReadPurpose, RtIntent, TriggerModeView,
};

const W: u8 = 46;
const A: u8 = 68;
const S: u8 = 69;
const D: u8 = 70;
const Q: u8 = 45;

#[test]
fn five_keys_are_written_one_packet_each_after_a_verified_safety_backup() {
    let mut h = Harness::ready();
    let before = h.snapshot();
    let intent = h.trigger_intent(
        &[W, A, S, D, Q],
        Some(60),
        Some(RtIntent::Same { sensitivity: 15 }),
    );
    let preview = h.prepare_trigger(intent).unwrap();
    assert_eq!(h.writes(), 0, "prepare is read-only");
    assert_eq!((preview.changing, preview.packets), (5, 5));
    assert!(!preview.nothing_to_do);
    let w = &preview.keys[0];
    assert_eq!((w.current.actuation, w.target.actuation), (50, 60));
    assert_eq!(w.target.mode, TriggerModeView::RapidTrigger);
    let token = Harness::token(&preview.token);

    let watch = h.watch();
    let report = h.apply_trigger(&token, &watch).unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(report.items.len(), 5);
    assert!(report.items.iter().all(|i| matches!(
        i.result,
        ItemResultView::Written {
            persistence: PersistenceView::NotChecked,
            ..
        }
    )));
    assert!(report.waited_for_commit_ms.is_some());
    assert!(report.anything_written);
    assert_eq!(report.active_profile_now, Some(1));

    // Exactly five trigger packets, each with a single-key mask.
    let packets = h.trigger_packets();
    assert_eq!(packets.len(), 5);
    for packet in &packets {
        let bits: u32 = packet[6..28].iter().map(|b| b.count_ones()).sum();
        assert_eq!(bits, 1, "one key per packet");
    }
    assert_eq!(h.writes(), 5, "nothing but the trigger packets");

    // The progress showed the wait for the keyboard to save.
    let steps = watch.steps();
    assert!(steps.iter().any(|s| matches!(
        s,
        ProgressStep::Reading {
            purpose: ReadPurpose::SafetyBackup,
            ..
        }
    )));
    assert!(
        steps
            .iter()
            .any(|s| matches!(s, ProgressStep::SavingSafetyBackup))
    );
    assert!(
        steps
            .iter()
            .any(|s| matches!(s, ProgressStep::WaitingForKeyboardSave { .. }))
    );
    let writing: Vec<_> = steps
        .iter()
        .filter_map(|s| match s {
            ProgressStep::Writing {
                item,
                items,
                packet: 0,
                ..
            } => Some((*item, *items)),
            _ => None,
        })
        .collect();
    assert_eq!(writing, vec![(1, 5), (2, 5), (3, 5), (4, 5), (5, 5)]);

    // The keyboard saved the new settings.
    for key in [W, A, S, D, Q] {
        assert_eq!(
            h.with_sim(|s| s.persisted_trigger(profile(1), pos(key))),
            Some([0x0C, 60, 60, 15, 15, 0, 0, 0, 0]),
            "key {key}"
        );
    }

    // The safety backup holds the state before the edit.
    let files = h.safety_files();
    assert_eq!(files.len(), 1);
    assert!(
        files[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-set-trigger.owbackup.json")
    );
    let safety = Backup::load(&files[0]).unwrap();
    assert_eq!(safety.key_trigger(W).unwrap().actuation.units(), 50);
    assert_eq!(safety.tool.name, "openwraith-app-sim");
    assert_eq!(
        report.safety_backup.display_path,
        files[0].display().to_string()
    );

    // The journal existed while writing and is gone now.
    assert_eq!(*watch.journal_seen.lock().unwrap(), Some(true));
    assert!(!h.worker.config.journal_path().exists());

    // The view was refreshed.
    let after = h.snapshot();
    assert!(after.revision > before.revision);
    let w_now = after
        .triggers
        .keys
        .iter()
        .find(|k| k.key.index == W)
        .unwrap();
    assert_eq!(w_now.value.unwrap().actuation, 60);
    h.assert_clean();
}

#[test]
fn separate_press_and_release_is_mode_0d() {
    let mut h = Harness::ready();
    let intent = h.trigger_intent(
        &[Q],
        None,
        Some(RtIntent::Separate {
            press: 15,
            release: 25,
        }),
    );
    let preview = h.prepare_trigger(intent).unwrap();
    let report = h
        .apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done);
    assert_eq!(
        h.with_sim(|s| s.persisted_trigger(profile(1), pos(Q))),
        Some([0x0D, 85, 85, 15, 25, 0, 0, 0, 0])
    );
    h.assert_clean();
}

#[test]
fn nothing_to_change_ends_without_a_token() {
    let mut h = Harness::ready();
    let intent = h.trigger_intent(&[W], Some(50), Some(RtIntent::Same { sensitivity: 15 }));
    let preview = h.prepare_trigger(intent).unwrap();
    assert!(preview.nothing_to_do);
    assert!(preview.token.is_none());
    assert_eq!(h.writes(), 0);
    assert!(h.safety_files().is_empty());
}

#[test]
fn invalid_intents_are_refused_before_any_traffic() {
    let mut h = Harness::ready();
    let before = h.packets();
    let cases = [
        (h.trigger_intent(&[15], Some(60), None), "knob"),
        (h.trigger_intent(&[W, W], Some(60), None), "duplicate"),
        (h.trigger_intent(&[], Some(60), None), "no keys"),
        (h.trigger_intent(&[16], Some(60), None), "no such key"),
        (h.trigger_intent(&[W], None, None), "nothing"),
        (h.trigger_intent(&[W], Some(4), None), "too low"),
        (h.trigger_intent(&[W], Some(171), None), "too high"),
        (
            h.trigger_intent(&[W], None, Some(RtIntent::Same { sensitivity: 0 })),
            "sensitivity 0",
        ),
        (
            h.trigger_intent(
                &[W],
                None,
                Some(RtIntent::Separate {
                    press: 10,
                    release: 86,
                }),
            ),
            "release too high",
        ),
    ];
    for (intent, what) in cases {
        let error = h.prepare_trigger(intent).unwrap_err();
        assert_eq!(
            error.kind,
            AppErrorKind::InvalidRequest,
            "{what}: {error:?}"
        );
    }
    let knob = h.trigger_intent(&[15], Some(60), None);
    assert_eq!(
        h.prepare_trigger(knob).unwrap_err().detail,
        Some(ErrorDetail::InvalidIntent {
            field: IntentField::Knob
        })
    );
    let high = h.trigger_intent(&[W], Some(171), None);
    assert_eq!(
        h.prepare_trigger(high).unwrap_err().detail,
        Some(ErrorDetail::ValueOutOfRange {
            field: RangeField::Actuation,
            value: 171,
            min: 5,
            max: 170
        })
    );
    assert_eq!(h.packets(), before, "nothing was sent");
}

#[test]
fn a_key_that_cannot_be_written_back_is_refused() {
    let mut h = Harness::new(1);
    // Mode 0C with different press and release (written by other software).
    h.with_sim(|s| s.set_trigger(profile(1), pos(Q), [0x0C, 85, 85, 15, 25, 0, 0, 0, 0]));
    h.until_ready();
    let q = h
        .snapshot()
        .triggers
        .keys
        .into_iter()
        .find(|k| k.key.index == Q)
        .unwrap();
    assert_eq!(q.not_editable, Some(NotEditable::PressReleaseDiffer));
    let error = h
        .prepare_trigger(h.trigger_intent(&[Q], Some(60), None))
        .unwrap_err();
    assert!(matches!(
        error.detail,
        Some(ErrorDetail::KeyNotEditable {
            reason: NotEditable::PressReleaseDiffer,
            ..
        })
    ));
}

#[test]
fn another_distance_unit_is_refused_before_planning() {
    let backup = other_unit_backup();
    let mut session = Session::new(DeviceSim::from_backup(&backup));
    let edit = openwraith_hid::TriggerEdit {
        keys: vec![pos(W)],
        actuation: Some(openwraith_hid::protocol::Distance::from_units(60)),
        rapid_trigger: None,
    };
    let error = crate::ops::trigger::plan(&mut session, &edit).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::InvalidRequest);
    assert_eq!(
        error.detail,
        Some(ErrorDetail::DistanceUnit { unit_um: Some(30) })
    );
    let sim = session.into_transport();
    assert_eq!(sim.writes_sent(), 0);
    // Only the summary reads; no key was read.
    assert!(!sim.written().iter().any(|p| matches!(
        ReadRequest::from_packet(p.as_bytes()),
        Some(ReadRequest::KeyTrigger(_))
    )));
    sim.assert_clean();
}

#[test]
fn the_snapshot_shows_another_unit_as_not_editable() {
    let mut h = Harness::new(1);
    h.with_sim(|s| *s = crate::backend::sim::sim_from(&other_unit_backup(), "SIM-1"));
    h.until_ready();
    let triggers = h.snapshot().triggers;
    assert!(!triggers.editable);
    assert_eq!(triggers.unit_um, Some(30));
    let error = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::DistanceUnit { unit_um: Some(30) })
    );
    assert_eq!(h.writes(), 0);
}

#[test]
fn stop_during_the_safety_read_writes_nothing() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    let watch = h.watch();
    watch.cancel_when(|e| {
        matches!(
            e.step,
            ProgressStep::Reading {
                purpose: ReadPurpose::SafetyBackup,
                done: 10..,
                ..
            }
        )
    });
    let error = h
        .apply_trigger(&Harness::token(&preview.token), &watch)
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::Cancelled);
    assert_eq!(h.writes(), 0);
    assert!(h.safety_files().is_empty());
    assert!(!h.worker.config.journal_path().exists());
    h.assert_clean();
}

#[test]
fn stop_between_keys_writes_only_the_keys_before() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[A, S, D], Some(60), None))
        .unwrap();
    let watch = h.watch();
    watch.cancel_when(super::harness::is_write_start);
    let report = h
        .apply_trigger(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.headline, Headline::Cancelled);
    assert_eq!(h.trigger_packets().len(), 1);
    assert!(matches!(
        report.items[1].result,
        ItemResultView::NotAttempted
    ));
    // The wait for the save still ran.
    assert!(report.waited_for_commit_ms.is_some());
    h.assert_clean();
}

#[test]
fn a_journal_that_cannot_be_written_stops_the_edit_before_any_write() {
    let mut h = Harness::with_config(1, |config| {
        // The state directory is below a file: the journal cannot be created.
        let blocker = config.state_dir.with_file_name("blocker");
        std::fs::create_dir_all(blocker.parent().unwrap()).unwrap();
        std::fs::write(&blocker, "x").unwrap();
        config.state_dir = blocker.join("state");
    });
    h.until_ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    let error = h
        .apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::Io);
    assert_eq!(h.writes(), 0, "zero write packets");
    h.assert_clean();
}
