//! After a panic (Fault): nothing reopens the keyboard, the busy flag is
//! not left set, and "Stop safely and close" always exits once.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::harness::Harness;
use crate::dto::{
    AppErrorKind, AppEvent, CloseChoice, ConnectionState, DeviceId, SimAction, WriteBlockReason,
};
use crate::handle::WorkerHandle;
use crate::sink::EventSink;
use crate::worker::{Conn, Request};

fn exits(h: &Harness) -> usize {
    h.exits.0.load(Ordering::SeqCst)
}

fn handle_of(h: &Harness) -> WorkerHandle {
    WorkerHandle::from_parts(
        h.tx.clone(),
        Arc::clone(&h.shared),
        h.worker.config.clone(),
        None,
        None,
    )
}

#[test]
fn a_fault_is_not_left_by_reconnect_select_or_disconnect() {
    let mut h = Harness::ready();
    let id = h.worker.device.as_ref().map(|d| d.id.clone()).unwrap();
    h.post(Request::Panic);
    assert!(matches!(h.worker.conn, Conn::Fault { .. }));
    let before = h.packets();
    let refused = |error: crate::dto::AppError| {
        assert_eq!(error.kind, AppErrorKind::WritesBlocked);
    };
    refused(h.op(|reply| Request::Reconnect { reply }).unwrap_err());
    refused(
        h.op(|reply| Request::SelectDevice {
            id: id.clone(),
            reply,
        })
        .unwrap_err(),
    );
    refused(
        h.op(|reply| Request::SelectDevice {
            id: DeviceId("d-unknown".to_string()),
            reply,
        })
        .unwrap_err(),
    );
    refused(h.op(|reply| Request::Disconnect { reply }).unwrap_err());
    h.ticks(30);
    assert!(matches!(h.worker.conn, Conn::Fault { .. }));
    assert!(h.worker.session.is_none());
    let status = h.status();
    assert!(matches!(status.connection, ConnectionState::Fault { .. }));
    assert!(status.write_gate.reasons.contains(&WriteBlockReason::Fault));
    assert!(!status.write_gate.allowed);
    assert_eq!(h.packets(), before, "nothing was sent");
    assert!(!h.shared.busy.load(Ordering::SeqCst));
}

/// Panics once, at the first status that shows a read in progress.
struct PanicWhileReading {
    armed: AtomicBool,
}

impl EventSink for PanicWhileReading {
    fn send(&self, event: AppEvent) {
        if let AppEvent::Status { status } = &event
            && matches!(status.connection, ConnectionState::Reading { .. })
            && self.armed.swap(false, Ordering::SeqCst)
        {
            #[allow(clippy::panic)]
            {
                panic!("injected subscriber panic");
            }
        }
    }
}

#[test]
fn a_panic_in_the_workers_own_read_does_not_leave_the_busy_flag_set() {
    let mut h = Harness::ready();
    // A silent switch, found by the focus check, whose re-read panics.
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 3,
            after_ms: 0,
            report: false,
        })
        .unwrap();
    h.ticks(2);
    h.advance(Duration::from_secs(31));
    h.shared.set_subscriber(Arc::new(PanicWhileReading {
        armed: AtomicBool::new(true),
    }));
    h.post(Request::WindowFocused);
    assert!(
        matches!(h.worker.conn, Conn::Fault { .. }),
        "{:?}",
        h.worker.conn
    );
    assert!(!h.shared.busy.load(Ordering::SeqCst), "busy was released");
    assert!(!h.worker.owns_busy);
    // A new operation is admitted (and refused: writes stay blocked).
    let error = h.op(|reply| Request::Reconnect { reply }).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::WritesBlocked);
    // The window may close; "Stop safely and close" exits once.
    let handle = handle_of(&h);
    assert!(!handle.close_requested());
    handle.answer_close(CloseChoice::StopAndClose);
    assert_eq!(exits(&h), 1);
    h.ticks(10);
    assert_eq!(exits(&h), 1, "exactly once");
}

/// Panics once, at the next status.
struct PanicOnce {
    armed: AtomicBool,
}

impl EventSink for PanicOnce {
    fn send(&self, event: AppEvent) {
        if matches!(event, AppEvent::Status { .. }) && self.armed.swap(false, Ordering::SeqCst) {
            #[allow(clippy::panic)]
            {
                panic!("injected subscriber panic");
            }
        }
    }
}

#[test]
fn a_panic_in_a_tick_keeps_a_queued_requests_admission() {
    let mut h = Harness::ready();
    // A request was admitted and is queued (the command set the flag).
    h.shared.busy.store(true, Ordering::SeqCst);
    h.shared.set_subscriber(Arc::new(PanicOnce {
        armed: AtomicBool::new(true),
    }));
    // The next status differs: the tick's publish panics.
    h.shared.close_requested.store(true, Ordering::SeqCst);
    h.tick();
    assert!(matches!(h.worker.conn, Conn::Fault { .. }));
    assert!(
        h.shared.busy.load(Ordering::SeqCst),
        "the queued request keeps its admission"
    );
}

/// Answers the close guard with "Stop safely and close" at the first status
/// that shows a read in progress.
struct StopDuringReading {
    handle: Mutex<Option<WorkerHandle>>,
    busy_at_answer: Mutex<Option<bool>>,
}

impl EventSink for StopDuringReading {
    fn send(&self, event: AppEvent) {
        if let AppEvent::Status { status } = &event
            && matches!(status.connection, ConnectionState::Reading { .. })
            && let Some(handle) = self.handle.lock().unwrap().take()
        {
            handle.answer_close(CloseChoice::StopAndClose);
            *self.busy_at_answer.lock().unwrap() = Some(true);
        }
    }
}

#[test]
fn stop_and_close_during_the_workers_own_read_exits_when_it_ends() {
    let mut h = Harness::ready();
    // The close guard is shown (a deferred close).
    h.shared.close_requested.store(true, Ordering::SeqCst);
    let sink = Arc::new(StopDuringReading {
        handle: Mutex::new(Some(handle_of(&h))),
        busy_at_answer: Mutex::new(None),
    });
    h.shared.set_subscriber(sink.clone());
    // A profile change on the keyboard (D6): the worker reads again.
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 2,
            after_ms: 0,
            report: true,
        })
        .unwrap();
    let mut answered = false;
    for _ in 0..40 {
        h.tick();
        if sink.busy_at_answer.lock().unwrap().is_some() && !answered {
            answered = true;
            // The read that was running when the user answered is over: the
            // app exited exactly once.
            assert_eq!(exits(&h), 1);
            assert!(!h.shared.busy.load(Ordering::SeqCst));
        }
    }
    assert!(answered, "the read ran");
    assert_eq!(exits(&h), 1, "exactly once");
    assert_eq!(h.snapshot().profile, 2);
}
