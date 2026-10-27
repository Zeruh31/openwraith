//! The live key-depth view: the keyboard's travel test (spec section 4.11.7)
//! behind a guard that always closes it.
//!
//! While the test is open the keyboard sends one `21 .. (05|03) 01` report
//! per depth change of one key ([`TravelReport`]): about every 72 ms while a
//! key moves slowly, in 0-3 ms bursts while it moves fast. Nothing is stored
//! on the keyboard, so there is no backup, no read-back and no commit; but a
//! test left open keeps streaming, so it must always be closed again (T2,
//! `21 .. 18 03`).
//!
//! * [`Writer::open_travel_test`] passes the usual gate (no other program,
//!   no reset, no test left open), waits for the keyboard to save the last
//!   write ([`CommitSettle`](crate::CommitSettle)), confirms the active
//!   profile and sends the open (T1) with the model's full layout mask, the
//!   only form captured. It returns a [`TravelTest`] guard that **borrows
//!   the writer mutably**: no profile select, table write or read can run
//!   while the test is open.
//! * [`TravelTest::poll`] drains the stream. A profile report (Fn + 1..4 on
//!   the keyboard) or another program's report closes the test and stops;
//!   the keyboard's reset notification stops without sending anything (it is
//!   never answered).
//! * [`TravelTest::close`] sends T2 and waits for its acknowledgement (one
//!   retry). The guard's `Drop` does the same unless the test was closed
//!   already or the session may not send any more, so every exit path,
//!   errors and panics included, closes the test. When no close is
//!   acknowledged the session records [`TravelState::MaybeOpen`] and refuses
//!   table writes until [`Writer::close_stray_travel_test`] succeeds.
//! * Stream reports are never another program's activity. Reports that
//!   arrive while no test of ours is open (a test a crashed run left open)
//!   are counted as stray and also refuse table writes until a close is
//!   acknowledged.
//!
//! [`live_travel`] runs the whole loop for a user interface: one frame per
//! slice to a sink, stop on request, after a maximum time, or on any event
//! above.

use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use openwraith_protocol::{AckOffer, TravelReport, TravelTestMask, TravelTestRequest, WriteStep};
use serde::Serialize;

use openwraith_protocol::write::TRAVEL_ACK_TIMEOUT;

use crate::error::WriteError;
use crate::session::{Expect, ForeignActivity, Session, TravelState, Unrelated};
use crate::transport::WriteTransport;
use crate::writer::{WriteConsent, WriteOptions, Writer};

/// At most this many reports are drained by one [`TravelTest::poll`].
const POLL_MAX_REPORTS: usize = 256;

/// Options of [`live_travel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TravelOptions {
    /// The test is closed after this long at the latest (default 10 min).
    pub max_open: Duration,
    /// One frame is given to the sink per slice (default 16 ms).
    pub slice: Duration,
    /// Writer timeouts and the gate.
    pub write: WriteOptions,
}

impl Default for TravelOptions {
    fn default() -> Self {
        TravelOptions {
            max_open: Duration::from_secs(600),
            slice: Duration::from_millis(16),
            write: WriteOptions::default(),
        }
    }
}

/// Why a travel test stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "stop", rename_all = "snake_case")]
pub enum TravelStop {
    /// The caller asked to stop.
    Requested,
    /// [`TravelOptions::max_open`] passed.
    MaxOpen,
    /// The keyboard reported another active profile (Fn + 1..4).
    ProfileChanged {
        /// The profile it reported.
        profile: u8,
    },
    /// Another program's report arrived (a status report we did not
    /// request, an acknowledgement of a write we did not send).
    OtherProgram {
        /// What was seen.
        evidence: ForeignActivity,
    },
    /// The keyboard sent its reset notification. Nothing was sent after it,
    /// not even the close.
    KeyboardReset,
    /// The connection was lost.
    Disconnected,
    /// The sink can no longer take frames (for a caller whose sink fails
    /// rather than asking to stop).
    SinkStopped,
}

/// What a close of the travel test achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "close", rename_all = "snake_case")]
pub enum CloseResult {
    /// The keyboard acknowledged the close.
    Acked,
    /// The close was sent (twice) without an acknowledgement: the keyboard
    /// may still stream ([`TravelState::MaybeOpen`]).
    NotAcked,
    /// Nothing was sent.
    NotSent {
        /// Why (a reset notification, a lost connection).
        reason: &'static str,
    },
}

/// The stream reports drained by one [`TravelTest::poll`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TravelFrame {
    /// The reports, oldest first.
    pub reports: Vec<TravelReport>,
    /// Reports dropped because the queue was full.
    pub dropped: u64,
    /// Milliseconds since the test was opened when the frame was taken
    /// (transport clock); the reports arrived within the poll before it.
    pub open_ms: u64,
}

/// An open travel test; closed on [`close`](Self::close) or when dropped.
/// Created by [`Writer::open_travel_test`]; it borrows the writer, so
/// nothing else is sent while it is open.
pub struct TravelTest<'w, 's, T: WriteTransport> {
    writer: &'w mut Writer<'s, T>,
    /// The result of the close, once one was attempted (or skipped after a
    /// reset).
    closed: Option<CloseResult>,
    /// When the open was acknowledged (transport clock).
    opened_at: Instant,
}

impl<'s, T: WriteTransport> Writer<'s, T> {
    /// Opens the live travel test (T1, `21 .. 18 02` + the model's full
    /// layout mask) and returns the guard that closes it again.
    ///
    /// Refuses (sending nothing) after a reset or a lost connection, after
    /// another program was seen ([`WriteError::OtherProgramActive`]), and
    /// while a test may be open or the keyboard streams from a test left
    /// open ([`WriteError::TravelTestMaybeOpen`], [`WriteError::TravelTestOpen`]).
    /// Waits for the keyboard to save the last write first, and confirms the
    /// active profile with `21/11`. When the open is not acknowledged within
    /// [`TRAVEL_ACK_TIMEOUT`], a close is sent at once (best effort) and the
    /// call fails with [`WriteError::TravelOpenNotAcked`].
    pub fn open_travel_test(&mut self) -> Result<TravelTest<'_, 's, T>, WriteError> {
        self.ensure_usable()?;
        self.consent().ensure_not_standing_rule()?;
        if let Some(evidence) = self.foreign_activity_seen() {
            return Err(WriteError::OtherProgramActive { evidence });
        }
        self.check_travel_closed()?;
        let mask = TravelTestMask::full_layout(self.model()?)?;
        // Nothing may be pending when the stream starts: the test has the
        // keyboard to itself until it is closed.
        self.wait_for_commit()?;
        self.active_profile()?;
        self.check_travel_closed()?;
        let step = TravelTestRequest::Open(mask).step();
        // Reports that arrive right after the open belong to the test.
        self.session.set_travel(TravelState::Open);
        let opened = self.send_control_acked(&step);
        match opened {
            Ok(true) => {
                let opened_at = self.session.now();
                Ok(TravelTest {
                    writer: self,
                    closed: None,
                    opened_at,
                })
            }
            Ok(false) => {
                close_now(self);
                Err(WriteError::TravelOpenNotAcked)
            }
            Err(error) => {
                if !self.is_poisoned() {
                    close_now(self);
                } else {
                    self.session.set_travel(TravelState::MaybeOpen);
                }
                Err(error)
            }
        }
    }

    /// Sends the close (T2) alone, for a test this session did not open: a
    /// test left open by a crashed run, or one whose close was not
    /// acknowledged. Acknowledged: the session counts the test as closed
    /// and earlier stray reports no longer refuse writes.
    ///
    /// Refuses only after a reset or a lost connection (nothing is sent
    /// then); another program's activity does not refuse it, since it only
    /// stops a stream.
    pub fn close_stray_travel_test(&mut self) -> Result<CloseResult, WriteError> {
        self.ensure_usable()?;
        self.consent().ensure_not_standing_rule()?;
        if self.session.travel_state() == TravelState::Open {
            return Err(WriteError::TravelTestOpen { stray_reports: 0 });
        }
        let result = close_now(self);
        if result == CloseResult::Acked {
            self.session.rearm_stray_mark();
        }
        Ok(result)
    }

    /// Sends a travel-test packet and waits for its acknowledgement
    /// ([`TRAVEL_ACK_TIMEOUT`]). `Ok(false)` when it did not arrive (a late
    /// one is then consumed as ours). Other reports are handled as always:
    /// a profile report or a reset fails.
    fn send_control_acked(&mut self, step: &WriteStep) -> Result<bool, WriteError> {
        self.ensure_usable()?;
        self.session.send_control(&step.packet)?;
        let tracker = self.wait_for(step.ack, TRAVEL_ACK_TIMEOUT)?;
        if tracker.is_complete() {
            return Ok(true);
        }
        self.session
            .expect(Expect::Ack(step.ack), self.options().late_ack_window);
        Ok(false)
    }
}

/// Sends the close and waits for its acknowledgement, once more without
/// one; records the result in the session. Reports other than the ack are
/// classified as always but never stop the close (a reset does: nothing is
/// sent after it). Never panics.
fn close_now<T: WriteTransport>(writer: &mut Writer<'_, T>) -> CloseResult {
    let step = TravelTestRequest::Close.step();
    for _ in 0..2 {
        if writer.is_poisoned() {
            writer.session.set_travel(TravelState::MaybeOpen);
            return CloseResult::NotSent {
                reason: "the keyboard reset or the connection was lost",
            };
        }
        if writer.session.send_control(&step.packet).is_err() {
            writer.session.set_travel(TravelState::MaybeOpen);
            return CloseResult::NotSent {
                reason: "the connection was lost",
            };
        }
        if wait_close_ack(writer, &step) {
            writer.session.set_travel(TravelState::Closed);
            return CloseResult::Acked;
        }
        // A late ack is ours, not another program's.
        let window = writer.options().late_ack_window;
        writer.session.expect(Expect::Ack(step.ack), window);
    }
    writer.session.set_travel(TravelState::MaybeOpen);
    CloseResult::NotAcked
}

/// Waits for the close's acknowledgement; everything else is classified
/// (stream reports still queue) and otherwise ignored, except a reset.
fn wait_close_ack<T: WriteTransport>(writer: &mut Writer<'_, T>, step: &WriteStep) -> bool {
    let mut tracker = step.ack.tracker();
    let deadline = writer.session.now() + TRAVEL_ACK_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(writer.session.now());
        if remaining.is_zero() {
            return false;
        }
        let Ok(Some(report)) = writer.session.read_input(remaining) else {
            return false;
        };
        if tracker.offer(&report) == AckOffer::Ack {
            writer.session.count_ack();
            return true;
        }
        match writer.session.classify_unrelated(&report) {
            Unrelated::Reset => return false,
            Unrelated::ForeignStatus => writer.note_foreign(ForeignActivity {
                status_reports: 1,
                ..ForeignActivity::default()
            }),
            Unrelated::ForeignAck => writer.note_foreign(ForeignActivity {
                foreign_acks: 1,
                ..ForeignActivity::default()
            }),
            _ => {}
        }
    }
}

impl<T: WriteTransport> TravelTest<'_, '_, T> {
    /// Drains the stream for up to `wait` (or until 256 reports arrived)
    /// and returns the stream reports of the open test.
    ///
    /// Stops the test on the way:
    ///
    /// * the keyboard's reset notification: [`TravelStop::KeyboardReset`];
    ///   nothing is sent (not even the close), the session is poisoned;
    /// * a profile report (Fn + 1..4): closes, [`TravelStop::ProfileChanged`];
    /// * another program's status report or write acknowledgement: closes,
    ///   [`TravelStop::OtherProgram`];
    /// * a lost connection: [`TravelStop::Disconnected`].
    ///
    /// SOCD pushes are kept as events; other reports are ignored.
    pub fn poll(&mut self, wait: Duration) -> Result<TravelFrame, TravelStop> {
        if self.closed.is_some() {
            return Err(TravelStop::Requested);
        }
        let deadline = self.writer.session.now() + wait;
        for _ in 0..POLL_MAX_REPORTS {
            let remaining = deadline.saturating_duration_since(self.writer.session.now());
            // After the deadline only queued reports are drained.
            let report = match self.writer.session.read_input(remaining) {
                Ok(Some(report)) => report,
                Ok(None) => break,
                Err(_) => {
                    self.writer.session.set_travel(TravelState::MaybeOpen);
                    self.closed = Some(CloseResult::NotSent {
                        reason: "the connection was lost",
                    });
                    return Err(TravelStop::Disconnected);
                }
            };
            let other = |status_reports, foreign_acks| ForeignActivity {
                status_reports,
                foreign_acks,
                profile_reports: 0,
            };
            let evidence = match self.writer.session.classify_unrelated(&report) {
                Unrelated::Reset => {
                    // Never answered; the test may still be open.
                    self.writer.session.set_travel(TravelState::MaybeOpen);
                    self.closed = Some(CloseResult::NotSent {
                        reason: "the keyboard reported a reset",
                    });
                    return Err(TravelStop::KeyboardReset);
                }
                Unrelated::ProfileReport { profile, .. } => {
                    self.close_once();
                    return Err(TravelStop::ProfileChanged { profile });
                }
                Unrelated::ForeignStatus => other(1, 0),
                Unrelated::ForeignAck => other(0, 1),
                Unrelated::Travel
                | Unrelated::KeyboardEvent
                | Unrelated::Expected
                | Unrelated::Other => continue,
            };
            self.writer.note_foreign(evidence);
            self.close_once();
            return Err(TravelStop::OtherProgram { evidence });
        }
        let (reports, dropped) = self.writer.session.take_travel_reports();
        let open_ms = millis(
            self.writer
                .session
                .now()
                .saturating_duration_since(self.opened_at),
        );
        Ok(TravelFrame {
            reports,
            dropped,
            open_ms,
        })
    }

    /// Closes the test (T2, acknowledged within [`TRAVEL_ACK_TIMEOUT`], one
    /// retry). Returns what an earlier stop already did, if the test was
    /// closed (or could not be) before.
    pub fn close(mut self) -> CloseResult {
        self.close_once()
    }

    /// The writer's session (statistics, events).
    pub fn session(&self) -> &Session<T> {
        self.writer.session
    }

    fn close_once(&mut self) -> CloseResult {
        if let Some(result) = self.closed {
            return result;
        }
        let result = close_now(self.writer);
        self.closed = Some(result);
        result
    }
}

impl<T: WriteTransport> Drop for TravelTest<'_, '_, T> {
    /// Closes the test unless it was closed already or nothing may be sent
    /// any more (a reset notification or a lost connection). Never panics.
    fn drop(&mut self) {
        if self.closed.is_none() {
            let _ = self.close_once();
        }
    }
}

/// What [`live_travel`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TravelOutcome {
    /// Why it stopped.
    pub stop: TravelStop,
    /// How the close went.
    pub close: CloseResult,
    /// Stream reports given to the sink.
    pub reports: u64,
    /// Stream reports dropped because the queue was full.
    pub dropped: u64,
    /// How long the test was open, in milliseconds (transport clock).
    pub open_ms: u64,
}

/// Runs a live key-depth view: creates a writer (the "other program" gate),
/// opens the travel test, gives `sink` one frame per
/// [`slice`](TravelOptions::slice) and closes the test when the sink asks to
/// stop ([`ControlFlow::Break`]), after [`max_open`](TravelOptions::max_open),
/// or on any stop of [`TravelTest::poll`]. The test is closed on every path
/// (by the guard's `Drop` if nothing else does).
///
/// An `Err` means the test was never opened (gate, travel state, the open's
/// acknowledgement).
pub fn live_travel<T: WriteTransport>(
    session: &mut Session<T>,
    consent: WriteConsent,
    options: &TravelOptions,
    mut sink: impl FnMut(&TravelFrame) -> ControlFlow<()>,
) -> Result<TravelOutcome, WriteError> {
    let mut writer = session.writer_with(consent, options.write.clone())?;
    let mut test = writer.open_travel_test()?;
    let started = test.session().now();
    let mut reports = 0u64;
    let mut dropped = 0u64;
    let stop = loop {
        let open_for = test.session().now().saturating_duration_since(started);
        if open_for >= options.max_open {
            break TravelStop::MaxOpen;
        }
        let slice = options.slice.min(options.max_open - open_for);
        match test.poll(slice) {
            Ok(frame) => {
                reports += u64::try_from(frame.reports.len()).unwrap_or(u64::MAX);
                dropped += frame.dropped;
                if sink(&frame).is_break() {
                    break TravelStop::Requested;
                }
            }
            Err(stop) => break stop,
        }
    };
    let open_ms = millis(test.session().now().saturating_duration_since(started));
    let close = test.close();
    Ok(TravelOutcome {
        stop,
        close,
        reports,
        dropped,
        open_ms,
    })
}

/// Whole milliseconds of `duration` (saturating).
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
