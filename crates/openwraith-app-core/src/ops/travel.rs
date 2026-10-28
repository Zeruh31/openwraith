//! The live key-depth view (the keyboard's travel test, spec 4.11.7) and
//! the close of a test left open: the only callers of `live_travel` and
//! `close_stray_travel_test` (a source rule).
//!
//! * `start_depth_view` runs `live_travel` (which always closes the test)
//!   as an operation of its own: nothing else can run while it is open.
//!   Frames are coalesced to at most 60 per second and sent only when a key
//!   moved. It stops on Stop, a hidden window or a page left (the frontend
//!   calls `stop_depth_view`), at app exit, after 5 minutes without
//!   movement, after 60 minutes, when the frames' channel is gone, and on
//!   every stop of the library (lost connection, reset, another program, a
//!   profile switch on the keyboard). Travel samples never enter the
//!   session's events (the library keeps them out).
//! * A marker file (`state/travel-test-open`) is written before the test is
//!   opened and removed only after a close this call sent was acknowledged;
//!   a marker found at the next start means a crashed run may have left the
//!   test open. A start that fails before anything was opened leaves the
//!   marker as it was (an earlier run's marker stays; the one it wrote
//!   itself is removed).
//! * `close_stray_travel_test` sends the close alone (a user click only).

use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use openwraith_hid::{
    CloseResult, TravelFrame, TravelOptions, TravelState, TravelStop, live_travel,
};

use super::consent;
use super::progress::CancelPolicy;
use crate::backend::DeviceBackend;
use crate::dto::{
    AppError, AppErrorKind, DepthFrame, DepthSample, DepthStopReason, DepthViewReport, ErrorDetail,
    Notice, OpKind, PlanInvalidation, TravelCloseReport, TravelCloseView, TravelStateView,
};
use crate::map::{clean, sat32};
use crate::sink::{DepthSink, NoProgress};
use crate::worker::Worker;

/// One frame per slice of the library (at most 60 per second).
const SLICE: Duration = Duration::from_millis(17);

/// Frames are sent at most this often.
const FRAME_MS: u64 = 17;

/// The view stops after this long without movement ("Paused").
pub(crate) const NO_MOVEMENT_MS: u64 = 5 * 60 * 1000;

/// The view stops after this long at the latest.
const MAX_OPEN: Duration = Duration::from_secs(60 * 60);

/// Whether the marker of a live view is there (at start: a view of an
/// earlier run may still be open on the keyboard).
pub(crate) fn marker_left(config: &crate::config::CoreConfig) -> bool {
    config.travel_marker_path().is_file()
}

/// A close as a view.
pub(crate) fn close_view(close: CloseResult) -> TravelCloseView {
    match close {
        CloseResult::Acked => TravelCloseView::Acked,
        CloseResult::NotAcked => TravelCloseView::NotAcked,
        CloseResult::NotSent { reason } => TravelCloseView::NotSent {
            reason: clean(reason),
        },
    }
}

/// Why the library stopped, unless the view asked to stop itself.
fn stop_reason(stop: TravelStop) -> DepthStopReason {
    match stop {
        TravelStop::Requested => DepthStopReason::Requested,
        TravelStop::MaxOpen => DepthStopReason::TimeLimit,
        TravelStop::ProfileChanged { .. } => DepthStopReason::ProfileChanged,
        TravelStop::OtherProgram { .. } => DepthStopReason::OtherProgram,
        TravelStop::KeyboardReset => DepthStopReason::KeyboardReset,
        TravelStop::Disconnected => DepthStopReason::Disconnected,
        TravelStop::SinkStopped => DepthStopReason::ChannelClosed,
    }
}

/// The report of a view stopped before the test was opened.
fn not_opened(reason: DepthStopReason) -> DepthViewReport {
    DepthViewReport {
        reason,
        close: TravelCloseView::NotSent {
            reason: "the test was not opened".to_string(),
        },
        reports: 0,
        dropped: 0,
        open_ms: 0,
        may_be_open: false,
    }
}

/// Writes the marker before the test is opened.
fn write_marker(path: &Path, version: &str, now: std::time::SystemTime) -> Result<(), AppError> {
    let io = |e: std::io::Error| {
        AppError::with(
            AppErrorKind::Io,
            ErrorDetail::Io {
                file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
            },
            format!(
                "cannot write {}: {e}; the key-depth view was not started",
                path.display()
            ),
        )
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let text = serde_json::json!({
        "started_at": openwraith_hid::rfc3339_utc(now),
        "app_version": version,
    })
    .to_string();
    std::fs::write(path, text).map_err(io)
}

/// Reports since the last frame, per key: the latest and the deepest depth.
#[derive(Debug, Default)]
struct Coalescer {
    keys: BTreeMap<u8, (u16, u16)>,
    dropped: u64,
    seq: u32,
    last_sent_ms: Option<u64>,
    last_movement_ms: u64,
}

impl Coalescer {
    fn add(&mut self, frame: &TravelFrame) {
        for report in &frame.reports {
            let entry = self
                .keys
                .entry(report.pos.index())
                .or_insert((report.depth, report.depth));
            entry.0 = report.depth;
            entry.1 = entry.1.max(report.depth);
        }
        self.dropped = self.dropped.saturating_add(frame.dropped);
        if !frame.reports.is_empty() {
            self.last_movement_ms = frame.open_ms;
        }
    }

    /// The next frame, when one is due (at most one per [`FRAME_MS`]).
    fn take(&mut self, now_ms: u64) -> Option<DepthFrame> {
        if self.keys.is_empty() && self.dropped == 0 {
            return None;
        }
        if self
            .last_sent_ms
            .is_some_and(|at| now_ms.saturating_sub(at) < FRAME_MS)
        {
            return None;
        }
        self.last_sent_ms = Some(now_ms);
        self.seq = self.seq.wrapping_add(1);
        let keys = std::mem::take(&mut self.keys)
            .into_iter()
            .map(|(index, (depth, peak))| DepthSample { index, depth, peak })
            .collect();
        Some(DepthFrame {
            seq: self.seq,
            open_ms: sat32(now_ms),
            keys,
            dropped: sat32(std::mem::take(&mut self.dropped)),
        })
    }
}

impl<B: DeviceBackend> Worker<B> {
    /// Removes the marker after an acknowledged close.
    fn clear_marker(&self) {
        let _ = std::fs::remove_file(self.config.travel_marker_path());
        self.shared.set_travel_marker_left(false);
    }

    /// The `start_depth_view` request: the view ([`Self::start_depth_view`]),
    /// then the view the handle registered at its admission is forgotten on
    /// every path, and a view that ran is announced.
    pub(crate) fn depth_view_request(
        &mut self,
        sink: Arc<dyn DepthSink>,
    ) -> Result<DepthViewReport, AppError> {
        let result = self.start_depth_view(sink);
        // On every path: the view the handle registered at its admission.
        self.shared.end_depth();
        if let Ok(report) = &result {
            self.shared.notice(Notice::DepthViewStopped {
                reason: report.reason,
                close: report.close.clone(),
            });
        }
        result
    }

    /// `start_depth_view`: runs the live view until it stops; frames go to
    /// `sink`. An `Err` means the test was not opened (or its open was not
    /// acknowledged; the library closed it at once).
    fn start_depth_view(&mut self, sink: Arc<dyn DepthSink>) -> Result<DepthViewReport, AppError> {
        self.check_writable()?;
        self.invalidate(PlanInvalidation::Operation);
        let shared = Arc::clone(&self.shared);
        let clock = Arc::clone(&self.clock);
        self.run_op(
            OpKind::DepthView,
            Arc::new(NoProgress),
            CancelPolicy::Always,
            |w, progress| {
                // A stop that arrived before the operation started (or
                // while the other programs' check waits) opens nothing.
                shared.start_depth(progress.op().clone());
                if let Some(reason) = shared.depth_stop_reason() {
                    return Ok(not_opened(reason));
                }
                w.ensure_quiet(progress)?;
                if let Some(reason) = shared.depth_stop_reason() {
                    return Ok(not_opened(reason));
                }
                let marker = w.config.travel_marker_path();
                // A marker of an earlier run (a test it may have left open).
                let marker_before = marker.is_file();
                write_marker(&marker, &w.config.version, w.config.now())?;
                let mut live = w.status();
                live.travel.state = TravelStateView::Open;
                let options = TravelOptions {
                    max_open: MAX_OPEN,
                    slice: SLICE,
                    ..TravelOptions::default()
                };
                let mut coalescer = Coalescer::default();
                let mut own: Option<DepthStopReason> = None;
                let mut first = true;
                let outcome = match w.session.as_mut() {
                    None => Err(Self::not_connected()),
                    Some(session) => {
                        live_travel(session, consent::application(), &options, |frame| {
                            if first {
                                first = false;
                                shared.publish(live.clone(), clock.now());
                            }
                            if progress.cancel_requested() {
                                return ControlFlow::Break(());
                            }
                            coalescer.add(frame);
                            if frame.open_ms.saturating_sub(coalescer.last_movement_ms)
                                >= NO_MOVEMENT_MS
                            {
                                own = Some(DepthStopReason::NoMovement);
                                return ControlFlow::Break(());
                            }
                            if let Some(out) = coalescer.take(frame.open_ms)
                                && !sink.send(out)
                            {
                                own = Some(DepthStopReason::ChannelClosed);
                                return ControlFlow::Break(());
                            }
                            ControlFlow::Continue(())
                        })
                        .map_err(|e| AppError::from(&e))
                    }
                };
                let may_be_open = w
                    .session
                    .as_ref()
                    .is_none_or(|s| s.travel_state() != TravelState::Closed);
                match &outcome {
                    // Only a close this call sent and the keyboard
                    // acknowledged settles the marker.
                    Ok(done) if done.close == CloseResult::Acked => w.clear_marker(),
                    // The marker stays; "Stop it" is offered (also after a
                    // reset or a lost connection, when the session is gone).
                    Ok(_) => shared.set_travel_marker_left(true),
                    Err(_) if may_be_open => shared.set_travel_marker_left(true),
                    // Nothing was opened: the state before the call. Its own
                    // marker goes; an earlier run's marker (and its "may
                    // still be on") stays.
                    Err(_) if !marker_before => {
                        let _ = std::fs::remove_file(&marker);
                    }
                    Err(_) => {}
                }
                let outcome = outcome?;
                let reason = match outcome.stop {
                    TravelStop::Requested => own
                        .or(shared.depth_stop_reason())
                        .unwrap_or(DepthStopReason::Requested),
                    other => stop_reason(other),
                };
                Ok(DepthViewReport {
                    reason,
                    close: close_view(outcome.close),
                    reports: sat32(outcome.reports),
                    dropped: sat32(outcome.dropped),
                    open_ms: sat32(outcome.open_ms),
                    may_be_open,
                })
            },
        )
    }

    /// `close_stray_travel_test`: sends the travel test's close alone (a
    /// test a crashed run left open, or one whose close was not
    /// acknowledged). A user click only; harmless when no test is open.
    pub(crate) fn close_stray_travel_test(&mut self) -> Result<TravelCloseReport, AppError> {
        self.check_readable()?;
        self.invalidate(PlanInvalidation::Operation);
        self.run_op(
            OpKind::TravelClose,
            Arc::new(NoProgress),
            CancelPolicy::Never,
            |w, progress| {
                w.ensure_quiet(progress)?;
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let stray = session.stray_travel_since_gate();
                let close = {
                    let mut writer = session
                        .writer(consent::application())
                        .map_err(|e| AppError::from(&e))?;
                    writer
                        .close_stray_travel_test()
                        .map_err(|e| AppError::from(&e))?
                };
                if close == CloseResult::Acked {
                    w.clear_marker();
                    // A later stray stream is told again.
                    w.stray_notified = false;
                }
                Ok(TravelCloseReport {
                    close: close_view(close),
                    stray_reports: sat32(stray),
                })
            },
        )
    }

    /// While idle: a marker of an earlier run or stray stream reports are
    /// told once (per connection, and again after an acknowledged close)
    /// with `StrayTravelTest`. Sends nothing.
    pub(crate) fn check_travel_signals(&mut self) {
        if self.stray_notified {
            return;
        }
        let stray = self
            .session
            .as_ref()
            .is_some_and(|s| s.stray_travel_since_gate() > 0);
        if stray || self.shared.travel_marker_left() {
            self.stray_notified = true;
            self.shared.notice(Notice::StrayTravelTest);
        }
    }
}

#[cfg(test)]
mod tests {
    use openwraith_hid::protocol::{KeyPos, TravelReport, TravelVariant};

    use super::*;

    fn frame(open_ms: u64, reports: &[(u8, u16)]) -> TravelFrame {
        TravelFrame {
            reports: reports
                .iter()
                .map(|&(index, depth)| TravelReport {
                    pos: KeyPos::from_index(index).unwrap(),
                    depth,
                    variant: TravelVariant::Short,
                })
                .collect(),
            dropped: 0,
            open_ms,
        }
    }

    #[test]
    fn frames_are_coalesced_to_at_most_60_per_second() {
        let mut c = Coalescer::default();
        c.add(&frame(0, &[(46, 10), (46, 40), (46, 20)]));
        let first = c.take(0).unwrap();
        assert_eq!(
            first.keys,
            [DepthSample {
                index: 46,
                depth: 20,
                peak: 40
            }]
        );
        c.add(&frame(5, &[(47, 5)]));
        assert!(c.take(5).is_none(), "too soon after the last frame");
        c.add(&frame(17, &[(47, 7)]));
        let second = c.take(17).unwrap();
        assert_eq!(
            second.keys,
            [DepthSample {
                index: 47,
                depth: 7,
                peak: 7
            }]
        );
        assert_eq!(second.seq, 2);
        assert!(c.take(40).is_none(), "nothing moved");
        assert_eq!(c.last_movement_ms, 17);
    }
}
