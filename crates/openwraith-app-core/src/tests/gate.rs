//! Other programs, profile changes on the keyboard, stale plans and the
//! drift check.

use std::time::Duration;

use super::harness::Harness;
use crate::dto::{
    AppErrorKind, ConnectionState, ErrorDetail, Notice, PlanInvalidation, RtIntent, SimAction,
    WriteBlockReason,
};
use crate::worker::{Conn, Request};

const W: u8 = 46;

fn stale_reason(error: &crate::dto::AppError) -> Option<PlanInvalidation> {
    match error.detail {
        Some(ErrorDetail::PlanInvalidated { reason }) if error.kind == AppErrorKind::StalePlan => {
            Some(reason)
        }
        _ => None,
    }
}

#[test]
fn a_heartbeat_while_ready_blocks_everything_until_check_again() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 0,
        })
        .unwrap();
    h.tick();
    assert!(matches!(h.worker.conn, Conn::OtherProgram { .. }));
    assert!(h.notices().contains(&Notice::PlanInvalidated {
        reason: PlanInvalidation::OtherProgram
    }));
    let status = h.status();
    assert!(status.snapshot_stale);
    assert!(
        status
            .write_gate
            .reasons
            .contains(&WriteBlockReason::OtherProgram)
    );
    assert!(
        status
            .write_gate
            .reasons
            .contains(&WriteBlockReason::SnapshotStale)
    );
    let before = h.packets();
    // Nothing is sent: not the apply, not a new prepare, not a refresh.
    let error = h
        .apply_trigger(&preview.token.unwrap(), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::StalePlan);
    let error = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::WritesBlocked);
    assert_eq!(
        h.refresh().unwrap_err().kind,
        AppErrorKind::OtherProgramActive
    );
    h.ticks(50);
    assert_eq!(h.packets(), before, "only listening");
    // The user closed the site: observe again, then read again.
    h.op(|reply| Request::RecheckOtherProgram { reply })
        .unwrap();
    assert_eq!(h.worker.conn, Conn::Observing);
    h.ticks(5);
    assert_eq!(h.packets(), before, "the observation sends nothing");
    h.until_ready();
    assert!(!h.status().snapshot_stale);
    assert!(h.status().write_gate.allowed);
    h.assert_clean();
}

#[test]
fn a_profile_switch_on_the_keyboard_with_a_report_is_read_again() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 2,
            after_ms: 0,
            report: true,
        })
        .unwrap();
    h.ticks(10);
    assert_eq!(h.worker.conn, Conn::Ready);
    assert_eq!(h.snapshot().profile, 2);
    let notices = h.notices();
    assert!(notices.contains(&Notice::ProfileChangedOnKeyboard {
        from: Some(1),
        to: 2
    }));
    assert!(notices.contains(&Notice::PlanInvalidated {
        reason: PlanInvalidation::ProfileChanged
    }));
    let error = h
        .apply_trigger(&preview.token.unwrap(), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::StalePlan);
    // Writes work again (after the rest of the new observation).
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    assert_eq!(preview.profile, 2);
    let report = h
        .apply_trigger(&preview.token.unwrap(), &h.watch())
        .unwrap();
    assert_eq!(report.profile, 2);
    assert_eq!(report.headline, crate::dto::Headline::Done);
    h.assert_clean();
}

#[test]
fn a_silent_profile_switch_is_found_by_the_focus_check() {
    let mut h = Harness::ready();
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 3,
            after_ms: 0,
            report: false,
        })
        .unwrap();
    h.ticks(5);
    assert_eq!(h.snapshot().profile, 1, "nothing tells the app yet");
    // Within 30 s of the last traffic, focus does nothing.
    let before = h.packets();
    h.post(Request::WindowFocused);
    assert_eq!(h.packets(), before);
    h.advance(Duration::from_secs(31));
    h.post(Request::WindowFocused);
    h.ticks(2);
    assert_eq!(h.snapshot().profile, 3);
    assert!(h.notices().contains(&Notice::ProfileChangedOnKeyboard {
        from: Some(1),
        to: 3
    }));
    h.assert_clean();
}

#[test]
fn a_silent_switch_between_prepare_and_apply_fails_the_drift_check() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), Some(RtIntent::Off)))
        .unwrap();
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 2,
            after_ms: 0,
            report: false,
        })
        .unwrap();
    let error = h
        .apply_trigger(&preview.token.unwrap(), &h.watch())
        .unwrap_err();
    assert_eq!(stale_reason(&error), Some(PlanInvalidation::ProfileChanged));
    assert_eq!(h.writes(), 0, "zero write packets");
    assert!(h.safety_files().is_empty(), "no safety backup");
    // The view was updated from the fresh read.
    assert_eq!(h.snapshot().profile, 2);
    h.assert_clean();
}

#[test]
fn values_changed_by_another_program_fail_the_drift_check() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    h.with_sim(|s| {
        s.set_trigger(
            super::harness::profile(1),
            super::harness::pos(W),
            [0x00, 70, 70, 1, 1, 0, 0, 0, 0],
        );
    });
    let error = h
        .apply_trigger(&preview.token.unwrap(), &h.watch())
        .unwrap_err();
    assert_eq!(stale_reason(&error), Some(PlanInvalidation::ValuesChanged));
    assert_eq!(h.writes(), 0);
    assert!(h.safety_files().is_empty());
}

#[test]
fn plans_are_single_use_and_invalidated() {
    let mut h = Harness::ready();
    let prepare = |h: &mut Harness| {
        h.prepare_trigger(h.trigger_intent(&[W], Some(60), None))
            .unwrap()
            .token
            .unwrap()
    };
    // Another prepare supersedes.
    let first = prepare(&mut h);
    let second = prepare(&mut h);
    assert_eq!(
        stale_reason(&h.apply_trigger(&first, &h.watch()).unwrap_err()),
        Some(PlanInvalidation::Unknown)
    );
    // A discard.
    h.post(Request::DiscardPlan {
        token: second.clone(),
    });
    assert!(h.apply_trigger(&second, &h.watch()).is_err());
    // A resubscribe (reloaded window).
    let token = prepare(&mut h);
    h.post(Request::InvalidatePlan {
        reason: PlanInvalidation::Resubscribed,
    });
    assert!(h.apply_trigger(&token, &h.watch()).is_err());
    // A refresh.
    let token = prepare(&mut h);
    h.refresh().unwrap();
    assert!(h.apply_trigger(&token, &h.watch()).is_err());
    // The 10 minute limit.
    let token = prepare(&mut h);
    h.advance(Duration::from_secs(601));
    assert_eq!(
        stale_reason(&h.apply_trigger(&token, &h.watch()).unwrap_err()),
        Some(PlanInvalidation::Expired)
    );
    // A reconnect.
    let token = prepare(&mut h);
    h.op(|reply| Request::Reconnect { reply }).unwrap();
    h.until_ready();
    assert!(h.apply_trigger(&token, &h.watch()).is_err());
    assert_eq!(h.writes(), 0, "nothing was written");
    // A token of another kind.
    let token = prepare(&mut h);
    assert!(h.apply_switch(&token, &h.watch()).is_err());
    assert!(h.apply_trigger(&token, &h.watch()).is_err(), "single use");
    // Used once, then gone.
    let token = prepare(&mut h);
    h.apply_trigger(&token, &h.watch()).unwrap();
    assert!(h.apply_trigger(&token, &h.watch()).is_err());
    assert_eq!(h.trigger_packets().len(), 1);
    h.assert_clean();
}

#[test]
fn a_stale_snapshot_revision_is_refused() {
    let mut h = Harness::ready();
    let mut intent = h.trigger_intent(&[W], Some(60), None);
    intent.based_on += 1;
    let error = h.prepare_trigger(intent).unwrap_err();
    assert_eq!(
        stale_reason(&error),
        Some(PlanInvalidation::SnapshotChanged)
    );
}

#[test]
fn a_panic_is_a_fault_that_stops_everything() {
    let mut h = Harness::ready();
    h.post(Request::Panic);
    assert!(matches!(
        h.status().connection,
        ConnectionState::Fault { .. }
    ));
    assert!(h.worker.session.is_none());
    assert!(!h.shared.busy.load(std::sync::atomic::Ordering::SeqCst));
    let before = h.packets();
    h.ticks(30);
    assert_eq!(h.packets(), before);
    assert!(
        h.status()
            .write_gate
            .reasons
            .contains(&WriteBlockReason::Fault)
    );
    let error = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::WritesBlocked);
}
