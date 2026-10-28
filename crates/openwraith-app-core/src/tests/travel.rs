//! The live key-depth view and the close of a test left open, against the
//! simulator: the test is always closed (also when the frames' receiver is
//! gone), every stop reason is reported, the marker file's lifecycle, the
//! stray-test notice, and that travel samples never enter the session's
//! events.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use openwraith_hid::protocol::DeviceEvent;

use super::harness::{Harness, pos};
use crate::dto::{
    AppErrorKind, DepthFrame, DepthStopReason, Notice, OpKind, SimAction, TravelCloseView,
    TravelStateView,
};
use crate::shared::Shared;
use crate::sink::DepthSink;
use crate::worker::Conn;

const W: u8 = 46;

/// A hook on frames: `false` = the receiver is gone.
type OnFrame = Box<dyn FnMut(&DepthFrame, &Shared) -> bool + Send>;

/// Records frames and runs a hook on each.
struct Frames {
    frames: Mutex<Vec<DepthFrame>>,
    on_frame: Mutex<Option<OnFrame>>,
    shared: Arc<Shared>,
}

impl Frames {
    fn new(
        h: &Harness,
        on_frame: impl FnMut(&DepthFrame, &Shared) -> bool + Send + 'static,
    ) -> Arc<Self> {
        Arc::new(Frames {
            frames: Mutex::new(Vec::new()),
            on_frame: Mutex::new(Some(Box::new(on_frame))),
            shared: Arc::clone(&h.shared),
        })
    }

    fn all(&self) -> Vec<DepthFrame> {
        self.frames.lock().unwrap().clone()
    }
}

impl DepthSink for Frames {
    fn send(&self, frame: DepthFrame) -> bool {
        let keep = match &mut *self.on_frame.lock().unwrap() {
            Some(f) => f(&frame, &self.shared),
            None => true,
        };
        self.frames.lock().unwrap().push(frame);
        keep
    }
}

/// W moves down and up a little after now.
fn script_w(h: &Harness) {
    h.with_sim(|s| {
        s.travel_script(
            pos(W),
            &[
                (Duration::from_millis(100), 20),
                (Duration::from_millis(180), 60),
                (Duration::from_millis(260), 100),
                (Duration::from_millis(340), 40),
            ],
        );
    });
}

fn marker(h: &Harness) -> std::path::PathBuf {
    h.worker.config.travel_marker_path()
}

#[test]
fn the_live_view_streams_coalesced_frames_and_always_closes() {
    let mut h = Harness::ready();
    script_w(&h);
    let marker_path = marker(&h);
    let seen = Arc::new(Mutex::new((false, None)));
    let seen_in = Arc::clone(&seen);
    let sink = Frames::new(&h, move |frame, shared| {
        let mut seen = seen_in.lock().unwrap();
        seen.0 |= marker_path.is_file();
        seen.1 = shared.status().busy.map(|op| op.kind);
        if frame.keys.iter().any(|k| k.peak >= 100) {
            shared.stop_depth(DepthStopReason::Requested);
        }
        true
    });
    let report = h.depth_view(sink.clone()).unwrap();
    assert_eq!(report.reason, DepthStopReason::Requested, "{report:?}");
    assert_eq!(report.close, TravelCloseView::Acked);
    assert!(!report.may_be_open);
    assert!(report.reports >= 3);
    let (marker_while_open, busy) = *seen.lock().unwrap();
    assert!(marker_while_open, "the marker is written before the open");
    assert_eq!(busy, Some(OpKind::DepthView));
    assert!(!marker(&h).exists(), "removed after the acknowledged close");
    let frames = sink.all();
    assert!(!frames.is_empty());
    assert!(frames.iter().all(|f| f.keys.iter().all(|k| k.index == W)));
    assert!(frames.windows(2).all(|w| w[1].seq == w[0].seq + 1));
    assert!(
        frames
            .windows(2)
            .all(|w| w[1].open_ms.saturating_sub(w[0].open_ms) >= 17),
        "at most 60 frames per second"
    );
    h.with_sim(|s| {
        assert!(!s.travel_open());
        assert_eq!((s.travel_opens(), s.travel_closes()), (1, 1));
    });
    // Travel samples never enter the session's events.
    let session = h.worker.session.as_ref().unwrap();
    assert!(
        !session
            .events()
            .iter()
            .any(|e| matches!(e, DeviceEvent::TravelStream(_)))
    );
    assert!(h.notices().contains(&Notice::DepthViewStopped {
        reason: DepthStopReason::Requested,
        close: TravelCloseView::Acked
    }));
    assert_eq!(h.status().travel.state, TravelStateView::Closed);
    assert!(h.status().busy.is_none());
    h.assert_clean();
}

#[test]
fn the_view_stops_and_closes_when_its_receiver_is_gone() {
    let mut h = Harness::ready();
    script_w(&h);
    let sink = Frames::new(&h, |_, _| false);
    let report = h.depth_view(sink).unwrap();
    assert_eq!(report.reason, DepthStopReason::ChannelClosed);
    assert_eq!(report.close, TravelCloseView::Acked);
    h.with_sim(|s| assert!(!s.travel_open()));
    assert!(!marker(&h).exists());
    h.assert_clean();
}

#[test]
fn the_view_pauses_after_five_minutes_without_movement() {
    let mut h = Harness::ready();
    let sink = Frames::new(&h, |_, _| true);
    let report = h.depth_view(sink.clone()).unwrap();
    assert_eq!(report.reason, DepthStopReason::NoMovement);
    assert_eq!(report.close, TravelCloseView::Acked);
    assert!(u64::from(report.open_ms) >= crate::ops::travel::NO_MOVEMENT_MS);
    assert!(sink.all().is_empty(), "no frame without movement");
    h.with_sim(|s| assert!(!s.travel_open()));
}

#[test]
fn the_frontends_reasons_are_reported() {
    for reason in [DepthStopReason::PageLeft, DepthStopReason::WindowHidden] {
        let mut h = Harness::ready();
        script_w(&h);
        let sink = Frames::new(&h, move |_, shared| {
            shared.stop_depth(reason);
            true
        });
        let report = h.depth_view(sink).unwrap();
        assert_eq!(report.reason, reason);
        assert_eq!(report.close, TravelCloseView::Acked);
    }
}

#[test]
fn closing_the_app_stops_the_view_and_exits_after_the_close() {
    let mut h = Harness::ready();
    script_w(&h);
    let handle = crate::WorkerHandle::from_parts(
        h.tx.clone(),
        Arc::clone(&h.shared),
        h.worker.config.clone(),
        None,
        None,
    );
    let asked = Arc::new(Mutex::new(None));
    let asked_in = Arc::clone(&asked);
    let sink = Frames::new(&h, move |_, _| {
        let mut asked = asked_in.lock().unwrap();
        if asked.is_none() {
            *asked = Some(handle.close_requested());
        }
        true
    });
    let report = h.depth_view(sink).unwrap();
    assert_eq!(*asked.lock().unwrap(), Some(true), "the close waits");
    assert!(!h.status().close_requested, "no close guard for a view");
    assert_eq!(report.reason, DepthStopReason::AppExit);
    assert_eq!(report.close, TravelCloseView::Acked);
    assert_eq!(h.exits.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    h.with_sim(|s| assert!(!s.travel_open()));
}

#[test]
fn a_reset_during_the_view_sends_nothing_more_and_keeps_the_marker() {
    let mut h = Harness::ready();
    script_w(&h);
    let sim = h.sim.clone();
    let sink = Frames::new(&h, move |_, _| {
        sim.apply(SimAction::KeyboardReset).unwrap();
        true
    });
    let report = h.depth_view(sink).unwrap();
    assert_eq!(report.reason, DepthStopReason::KeyboardReset);
    assert!(matches!(report.close, TravelCloseView::NotSent { .. }));
    assert!(report.may_be_open);
    assert!(marker(&h).exists(), "the test may still be open");
    assert_eq!(h.worker.conn, Conn::ResetSeen);
    assert!(h.status().travel.marker_left, "\"Stop it\" is offered");
    assert_eq!(h.sim.post_reset_packets(), 0);
    h.with_sim(|s| s.allow_travel_left_open());
}

#[test]
fn another_program_or_a_profile_switch_closes_the_view() {
    let mut h = Harness::ready();
    script_w(&h);
    let sim = h.sim.clone();
    let sink = Frames::new(&h, move |_, _| {
        sim.apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 0,
        })
        .unwrap();
        true
    });
    let report = h.depth_view(sink).unwrap();
    assert_eq!(report.reason, DepthStopReason::OtherProgram);
    assert_eq!(report.close, TravelCloseView::Acked);
    assert!(matches!(h.worker.conn, Conn::OtherProgram { .. }));
    h.with_sim(|s| assert!(!s.travel_open()));

    let mut h = Harness::ready();
    script_w(&h);
    let sim = h.sim.clone();
    let sink = Frames::new(&h, move |_, _| {
        sim.apply(SimAction::KeyboardSwitch {
            profile: 2,
            after_ms: 0,
            report: true,
        })
        .unwrap();
        true
    });
    let report = h.depth_view(sink).unwrap();
    assert_eq!(report.reason, DepthStopReason::ProfileChanged);
    assert_eq!(report.close, TravelCloseView::Acked);
    h.with_sim(|s| assert!(!s.travel_open()));
    // The settings are read again for the new profile.
    h.ticks(10);
    assert_eq!(h.snapshot().profile, 2);
}

#[test]
fn no_view_starts_while_writes_are_blocked() {
    let mut h = Harness::ready();
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 0,
        })
        .unwrap();
    h.ticks(2);
    let before = h.packets();
    let error = h.depth_view(Frames::new(&h, |_, _| true)).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::WritesBlocked);
    assert_eq!(h.packets(), before, "nothing sent");
    assert!(!marker(&h).exists());
}

#[test]
fn a_marker_left_by_an_earlier_run_is_told_and_closed_on_request() {
    let mut h = Harness::new(1);
    let path = marker(&h);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{}").unwrap();
    // What `WorkerHandle::spawn` does at start.
    h.shared
        .set_travel_marker_left(crate::ops::travel::marker_left(&h.worker.config));
    h.until_ready();
    h.ticks(2);
    assert!(h.status().travel.marker_left);
    assert_eq!(
        h.notices()
            .iter()
            .filter(|n| **n == Notice::StrayTravelTest)
            .count(),
        1,
        "told once"
    );
    let report = h.close_stray().unwrap();
    assert_eq!(report.close, TravelCloseView::Acked);
    assert_eq!(report.stray_reports, 0);
    assert!(!path.exists());
    assert!(!h.status().travel.marker_left);
    h.with_sim(|s| assert_eq!(s.travel_closes(), 1));
    h.assert_clean();
}

#[test]
fn stray_stream_reports_are_told_and_a_close_ends_them() {
    let mut h = Harness::ready();
    h.with_sim(|s| {
        s.leave_travel_open();
        s.travel_script(pos(W), &[(Duration::from_millis(50), 30)]);
    });
    h.ticks(5);
    assert!(
        h.status().travel.stray_reports > 0,
        "{:?}",
        h.status().travel
    );
    assert!(h.notices().contains(&Notice::StrayTravelTest));
    assert_eq!(h.worker.conn, Conn::Ready, "never another program");
    let report = h.close_stray().unwrap();
    assert_eq!(report.close, TravelCloseView::Acked);
    assert!(report.stray_reports > 0);
    assert_eq!(h.status().travel.stray_reports, 0);
    h.with_sim(|s| assert!(!s.travel_open()));
}

#[test]
fn a_refused_start_keeps_an_earlier_runs_marker() {
    // An earlier run left its marker, and the keyboard still streams: the
    // test is open. A Start is refused before anything is sent; the marker
    // and its "may still be on" stay for the next start.
    let mut h = Harness::new(1);
    let path = marker(&h);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{}").unwrap();
    h.shared
        .set_travel_marker_left(crate::ops::travel::marker_left(&h.worker.config));
    h.until_ready();
    h.with_sim(|s| {
        s.leave_travel_open();
        s.travel_script(pos(W), &[(Duration::from_millis(50), 30)]);
    });
    h.ticks(5);
    assert!(h.status().travel.stray_reports > 0);
    let opens = h.with_sim(|s| s.travel_opens());
    let error = h.depth_view(Frames::new(&h, |_, _| true)).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::TravelTestOpen, "{error:?}");
    assert!(path.exists(), "the earlier run's marker stays");
    assert!(h.status().travel.marker_left);
    h.with_sim(|s| {
        assert!(s.travel_open());
        assert_eq!(s.travel_opens(), opens, "nothing opened");
    });
    // "Stop it" still settles it.
    let report = h.close_stray().unwrap();
    assert_eq!(report.close, TravelCloseView::Acked);
    assert!(!path.exists());
    assert!(!h.status().travel.marker_left);
}

#[test]
fn a_refused_start_removes_only_the_marker_it_wrote() {
    // No earlier marker; a stray stream refuses the start: the marker this
    // call wrote goes, so no false "may still be on" at the next start.
    let mut h = Harness::ready();
    h.with_sim(|s| {
        s.leave_travel_open();
        s.travel_script(pos(W), &[(Duration::from_millis(50), 30)]);
    });
    h.ticks(5);
    let error = h.depth_view(Frames::new(&h, |_, _| true)).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::TravelTestOpen, "{error:?}");
    assert!(!marker(&h).exists());
    assert!(!h.status().travel.marker_left);
    h.with_sim(|s| s.allow_travel_left_open());
}

/// A handle whose requests the test hands to the harness's worker itself.
fn handle_on_channel(
    h: &Harness,
) -> (
    Arc<crate::WorkerHandle>,
    std::sync::mpsc::Receiver<crate::worker::Envelope>,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = crate::WorkerHandle::from_parts(
        tx,
        Arc::clone(&h.shared),
        h.worker.config.clone(),
        None,
        None,
    );
    (Arc::new(handle), rx)
}

#[test]
fn a_stop_sent_before_the_worker_starts_the_view_is_kept() {
    for stop in [
        crate::dto::DepthStopRequest::PageLeft,
        crate::dto::DepthStopRequest::Requested,
    ] {
        let mut h = Harness::ready();
        script_w(&h);
        let (handle, rx) = handle_on_channel(&h);
        let sink = Frames::new(&h, |_, _| true);
        let starter = Arc::clone(&handle);
        let view = std::thread::spawn(move || starter.start_depth_view(sink));
        // Admitted and queued, not handled yet: Start, then at once a tab
        // switch (or Stop).
        let envelope = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(handle.stop_depth_view(stop), "the stop is kept");
        let opens = h.with_sim(|s| s.travel_opens());
        let packets = h.packets();
        h.worker.handle_guarded(envelope);
        let report = view.join().unwrap().unwrap();
        let expected = match stop {
            crate::dto::DepthStopRequest::PageLeft => DepthStopReason::PageLeft,
            _ => DepthStopReason::Requested,
        };
        assert_eq!(report.reason, expected);
        assert!(
            matches!(report.close, TravelCloseView::NotSent { .. }),
            "{report:?}"
        );
        assert!(!report.may_be_open);
        h.with_sim(|s| {
            assert_eq!(s.travel_opens(), opens, "the test was never opened");
            assert!(!s.travel_open());
        });
        assert_eq!(h.packets(), packets, "nothing sent");
        assert!(!marker(&h).exists());
        // Forgotten: a later stop finds no view, and nothing stays busy.
        assert!(!handle.stop_depth_view(stop));
        assert!(h.status().busy.is_none());
        assert!(!h.shared.busy.load(std::sync::atomic::Ordering::SeqCst));
    }
}

#[test]
fn a_refused_start_forgets_the_admitted_view() {
    // Writes blocked: the worker refuses before the operation starts; the
    // view registered at the admission is forgotten all the same.
    let mut h = Harness::ready();
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 0,
        })
        .unwrap();
    h.ticks(2);
    let (handle, rx) = handle_on_channel(&h);
    let starter = Arc::clone(&handle);
    let sink = Frames::new(&h, |_, _| true);
    let view = std::thread::spawn(move || starter.start_depth_view(sink));
    let envelope = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    h.worker.handle_guarded(envelope);
    let error = view.join().unwrap().unwrap_err();
    assert_eq!(error.kind, AppErrorKind::WritesBlocked);
    assert!(!handle.stop_depth_view(crate::dto::DepthStopRequest::PageLeft));
}

#[test]
fn the_view_stops_after_sixty_minutes_at_the_latest() {
    // W moves every 4 minutes for 70 minutes: never "Paused", stopped at 60.
    let mut h = Harness::ready();
    let points: Vec<(Duration, u16)> = (1..=35u64)
        .map(|n| {
            (
                Duration::from_secs(n * 120),
                if n % 2 == 0 { 0 } else { 40 },
            )
        })
        .collect();
    h.with_sim(|s| s.travel_script(pos(W), &points));
    let report = h.depth_view(Frames::new(&h, |_, _| true)).unwrap();
    assert_eq!(report.reason, DepthStopReason::TimeLimit, "{report:?}");
    assert_eq!(report.close, TravelCloseView::Acked);
    assert!(u64::from(report.open_ms) >= 60 * 60 * 1000);
    h.with_sim(|s| assert!(!s.travel_open()));
    assert!(!marker(&h).exists());
}

#[test]
fn a_lost_connection_stops_the_view_and_keeps_the_marker() {
    let mut h = Harness::ready();
    script_w(&h);
    let sim = h.sim.clone();
    let sink = Frames::new(&h, move |_, _| {
        sim.apply(SimAction::Unplug).unwrap();
        true
    });
    let report = h.depth_view(sink).unwrap();
    assert_eq!(report.reason, DepthStopReason::Disconnected, "{report:?}");
    assert!(matches!(report.close, TravelCloseView::NotSent { .. }));
    assert!(report.may_be_open);
    assert!(marker(&h).exists(), "the test may still be open");
    assert!(h.status().travel.marker_left);
    h.with_sim(|s| s.allow_travel_left_open());
}

#[test]
fn a_reloaded_window_stops_the_view_and_releases_the_busy_flag() {
    let mut h = Harness::ready();
    script_w(&h);
    let handle = crate::WorkerHandle::from_parts(
        h.tx.clone(),
        Arc::clone(&h.shared),
        h.worker.config.clone(),
        None,
        None,
    );
    let subscribed = Arc::new(Mutex::new(false));
    let subscribed_in = Arc::clone(&subscribed);
    let sink = Frames::new(&h, move |_, _| {
        let mut done = subscribed_in.lock().unwrap();
        if !*done {
            *done = true;
            let events: Arc<dyn crate::sink::EventSink> =
                Arc::new(crate::sink::recorder::Recorder::<crate::dto::AppEvent>::default());
            handle.subscribe(events);
        }
        true
    });
    let report = h.depth_view(sink).unwrap();
    assert_eq!(report.reason, DepthStopReason::ChannelClosed, "{report:?}");
    assert_eq!(report.close, TravelCloseView::Acked);
    assert!(h.status().busy.is_none());
    assert!(!h.shared.busy.load(std::sync::atomic::Ordering::SeqCst));
    h.with_sim(|s| assert!(!s.travel_open()));
}
