//! Session hygiene on the virtual clock: an idle session is replaced after
//! 24 hours or 1000 events, sends nothing, starts a new gate and keeps the
//! connection; never while something depends on the old session.

use std::time::Duration;

use super::harness::Harness;
use crate::dto::{ProgressStep, SimAction};
use crate::worker::Conn;

const W: u8 = 46;

#[test]
fn an_idle_session_is_replaced_after_a_day() {
    let mut h = Harness::ready();
    let generation = h.worker.generation;
    let snapshot = h.snapshot();
    let packets = h.packets();
    h.advance(Duration::from_secs(23 * 60 * 60));
    h.tick();
    assert_eq!(h.worker.session_recycles, 0, "not yet");
    h.advance(Duration::from_secs(60 * 60));
    h.tick();
    assert_eq!(h.worker.session_recycles, 1);
    assert_eq!(h.worker.conn, Conn::Ready);
    assert_eq!(h.worker.generation, generation, "the same connection");
    assert_eq!(h.snapshot(), snapshot, "the snapshot stays");
    let session = h.worker.session.as_ref().unwrap();
    assert!(session.observed_for() < Duration::from_secs(1));
    assert!(session.events().is_empty());
    assert_eq!(h.packets(), packets, "nothing was sent");
    // A write observes the new gate first, then works.
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    let watch = h.watch();
    h.apply_trigger(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert!(
        watch
            .steps()
            .iter()
            .any(|s| matches!(s, ProgressStep::CheckingOtherPrograms { .. }))
    );
    // Another program heard on the new session is still caught.
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 0,
        })
        .unwrap();
    h.ticks(3);
    assert!(matches!(h.worker.conn, Conn::OtherProgram { .. }));
    h.assert_clean();
}

#[test]
fn a_session_with_a_thousand_events_is_replaced_but_not_right_after_a_write() {
    let mut h = Harness::ready();
    // A write, then 1000 SOCD pushes (Fn + Right Shift) within a second.
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    h.apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    h.with_sim(|s| {
        for i in 0..1000u64 {
            s.press_socd_hotkey_at(Duration::from_millis(i));
        }
    });
    h.ticks(15);
    let events = h.worker.session.as_ref().unwrap().events().len();
    assert!(events >= 1000, "{events}");
    assert_eq!(
        h.worker.session_recycles, 0,
        "not within 10 s of a setting write"
    );
    let packets = h.packets();
    h.ticks(100);
    assert_eq!(h.worker.session_recycles, 1);
    assert!(h.worker.session.as_ref().unwrap().events().len() < 1000);
    assert_eq!(h.packets(), packets, "nothing was sent");
    assert_eq!(h.worker.conn, Conn::Ready);
    // The pushes of the new session are still taken (the view follows).
    let power = h.socd().power;
    h.with_sim(|s| s.press_socd_hotkey_at(Duration::from_millis(10)));
    h.ticks(3);
    assert_ne!(h.socd().power, power);
    h.assert_clean();
}

#[test]
fn a_session_in_use_is_never_replaced() {
    // A plan waiting for the user.
    let mut h = Harness::ready();
    h.advance(Duration::from_secs(24 * 60 * 60));
    h.prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    assert!(!h.worker.recycle_session_if_due());
    // Another program heard within two minutes.
    let mut h = Harness::ready();
    h.advance(Duration::from_secs(24 * 60 * 60));
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 0,
        })
        .unwrap();
    h.ticks(2);
    assert!(matches!(h.worker.conn, Conn::OtherProgram { .. }));
    assert!(!h.worker.recycle_session_if_due());
    // A key-depth test left open on the keyboard (stray stream).
    let mut h = Harness::ready();
    h.with_sim(|s| {
        s.leave_travel_open();
        s.travel_script(super::harness::pos(W), &[(Duration::from_millis(50), 30)]);
    });
    h.ticks(3);
    h.advance(Duration::from_secs(24 * 60 * 60));
    assert!(!h.worker.recycle_session_if_due());
    h.with_sim(|s| s.allow_travel_left_open());
}
