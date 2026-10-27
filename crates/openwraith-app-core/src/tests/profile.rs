//! Profile switches.

use super::harness::Harness;
use crate::dto::{CancelState, LastReport, ProgressStep, RtIntent};

#[test]
fn a_switch_selects_the_profile_and_reads_it() {
    let mut h = Harness::ready();
    let preview = h.prepare_switch(2).unwrap();
    assert_eq!((preview.from, preview.to), (1, 2));
    assert_eq!(preview.save_wait_ms, 0);
    assert_eq!(h.writes(), 0, "prepare is read-only");
    let watch = h.watch();
    let report = h
        .apply_switch(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.active_profile_now, Some(2));
    assert_eq!(h.selects(), vec![2]);
    assert_eq!(h.writes(), 1, "only the select");
    assert_eq!(h.snapshot().profile, 2);
    assert_eq!(h.status().active_profile, Some(2));
    assert!(
        watch
            .all()
            .iter()
            .all(|e| e.cancel == CancelState::NotCancellable)
    );
    assert!(matches!(
        h.shared.last_report(),
        Some(LastReport::ProfileSwitch { .. })
    ));
    h.assert_clean();
}

#[test]
fn the_active_profile_needs_no_switch() {
    let mut h = Harness::ready();
    let preview = h.prepare_switch(1).unwrap();
    assert!(preview.nothing_to_do);
    assert!(preview.token.is_none());
    assert!(h.prepare_switch(5).is_err());
}

#[test]
fn a_switch_right_after_a_write_waits_for_the_keyboard_to_save() {
    let mut h = Harness::ready();
    // A light write without its own wait at the end is not possible through
    // the app: an edit waits before it returns. A write made just before is
    // simulated by a short virtual time after the edit's own wait.
    let preview = h
        .prepare_trigger(h.trigger_intent(&[46], Some(60), Some(RtIntent::Off)))
        .unwrap();
    h.apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    let preview = h.prepare_switch(3).unwrap();
    // The edit already waited; nothing is left to wait for.
    assert_eq!(preview.save_wait_ms, 0);
    let watch = h.watch();
    let report = h
        .apply_switch(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.active_profile_now, Some(3));
    assert!(
        !watch
            .steps()
            .iter()
            .any(|s| matches!(s, ProgressStep::WaitingForKeyboardSave { .. }))
    );
    // The edit was kept by the keyboard.
    assert_eq!(
        h.with_sim(|s| s.persisted_trigger(super::harness::profile(1), super::harness::pos(46))),
        Some([0x00, 60, 60, 15, 15, 0, 0, 0, 0])
    );
    h.assert_clean();
}
