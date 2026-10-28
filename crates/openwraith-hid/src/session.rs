//! A request/response session over a [`Transport`] (spec sections 2.5 and
//! 2.8).
//!
//! The policy follows the spec's advice for our application:
//!
//! 1. One outstanding request at a time. Stale input is drained before each
//!    request.
//! 2. Replies are recognised by their header (and the row/col echo for
//!    `21/05`), never by arrival order. Reports that do not match are
//!    ignored: another program, such as the vendor's web configurator, may be
//!    talking to the keyboard at the same time and its replies reach us too.
//! 3. Each request has its own timeout ([`ReadRequest::timeout`]); the wait
//!    is extended while chunks keep arriving. A request that times out is
//!    sent once more, keeping the chunks already received.
//! 4. Two different copies of the same chunk mean that the keyboard's data
//!    changed while it was being read. The partial reply is discarded and
//!    the request sent again; on the last attempt the request fails with
//!    [`SessionError::Inconsistent`]. A reply is never stitched together
//!    from two states.
//! 5. The keyboard's reset notification (`14 .. 01 01`) ends the request
//!    with [`SessionError::KeyboardReset`]. **The session never answers it**
//!    (the vendor site answers with a reset of its own), and after one the
//!    session refuses to create a [`Writer`](crate::Writer).
//!
//! A session only reads. Writing needs a transport that implements
//! [`WriteTransport`] and goes through [`Session::writer`], which checks first
//! that no other program is using the keyboard (see [`Writer`](crate::Writer)).
//!
//! Every timeout and observation period is measured on the transport's clock
//! ([`Transport::now`]). A read interrupted by a signal
//! ([`TransportError::Interrupted`], Ctrl+C on Linux) is retried with the
//! remaining time; it never ends the session.
//!
//! # Other programs on the same keyboard
//!
//! Filtering is not perfect. Some replies cannot be told apart from another
//! program's traffic: the `09 20` reply is byte-identical to the
//! acknowledgement of every colour-table write (spec sections 2.7 and 4.7.9),
//! and the status reply is the same report as the vendor site's heartbeat
//! reply (section 4.2). A colour-write ack that arrives while our `09 20`
//! is pending is taken as its answer.
//!
//! The reverse direction matters too: on Windows every program that has the
//! configuration interface open receives **every** input report, so our
//! replies reach the vendor site as well, and the site reacts to them. On
//! our `21/11` profile reply it deletes and rewrites its stored copy of the
//! settings, re-reads the keyboard and sends a (no-op) trigger write. It
//! assigns `21/05` replies by arrival order, so our extra replies can be
//! filed under the wrong keys in its model (sections 4.5.1, 4.11.2 and 6.5).
//! Users should close the vendor site before using OpenWraith;
//! [`Session::foreign_activity`] (status reports we did not request,
//! acknowledgements of writes we did not send, profile reports we did not
//! cause) shows whether another program was active.
//!
//! # Reports the keyboard sends on its own
//!
//! Two kinds of unsolicited reports come from the keyboard itself and are
//! **never** counted as another program's activity (spec sections 4.11.7
//! and 4.15):
//!
//! * the SOCD push `24 02 00 00 06 PP` after Fn + Right Shift: recorded as a
//!   [`DeviceEvent::SocdPower`] in [`Session::events`] and counted in
//!   [`SessionStats::socd_pushes`] (a `24 02` read pending at that moment
//!   takes it as its reply);
//! * the live travel stream `21 .. (05|03) 01` while a travel test is open
//!   ([`TravelState`]): queued for the open test, never listed in
//!   [`Session::events`]. Stream reports that arrive while no test of ours
//!   is open (a test left open by a crashed run) are counted as stray
//!   ([`Session::stray_travel_since_gate`]); table writes are refused until
//!   the stray test is closed.
//!
//! # Egress allowlist
//!
//! Every write packet passes [`is_permitted_packet`] right before it is
//! sent, a coarse header check independent of the builders. A packet that
//! fails it is not sent, and the session refuses every later write (a
//! programming error, never a keyboard condition).

use std::collections::VecDeque;
use std::fmt;
use std::time::{Duration, Instant};

use openwraith_protocol::{
    AckOffer, AckSpec, DeviceEvent, FromResponse, Offer, OutPacket, ProfileId, RawResponse,
    ReadRequest, Report, ResponseCollector, TravelReport, WritePacket, is_keyboard_reset,
    is_permitted_packet, is_write_ack,
};
use serde::Serialize;

use crate::error::{ListenError, SessionError, TransportError};
use crate::transport::{Transport, WriteTransport};

/// Tunables of a [`Session`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOptions {
    /// How many times a request is re-sent after a timeout (spec section 2.8:
    /// once).
    pub retries: u32,
    /// Overrides the per-request timeout of [`ReadRequest::timeout`].
    pub timeout: Option<Duration>,
    /// At most this many stale reports are discarded before a request; after
    /// that the request is sent anyway (reply matching still protects it).
    pub max_drain: usize,
}

impl Default for SessionOptions {
    fn default() -> Self {
        SessionOptions {
            retries: 1,
            timeout: None,
            max_drain: 512,
        }
    }
}

/// Counters describing what a session saw. Useful for diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SessionStats {
    /// Requests completed or attempted.
    pub requests: u64,
    /// Requests re-sent after a timeout.
    pub retries: u64,
    /// Reply chunks accepted.
    pub chunks: u64,
    /// Reply chunks received twice with the same data (kept once).
    pub duplicates: u64,
    /// Reply chunks received twice with different data; each one discarded
    /// the partial reply (see [`SessionError::Inconsistent`]).
    pub conflicts: u64,
    /// Stale reports discarded before a request.
    pub drained: u64,
    /// Status reports that were not replies to our requests (another
    /// program's heartbeat, or a push from the keyboard).
    pub status_reports: u64,
    /// Other reports that did not belong to the pending request (including
    /// the [`foreign_acks`](Self::foreign_acks)).
    pub ignored: u64,
    /// Reports with a wrong size or report ID.
    pub malformed: u64,
    /// Write packets sent (Phase 2).
    pub writes: u64,
    /// Acknowledgements of our write packets received (Phase 2).
    pub acks: u64,
    /// Reports shaped like the acknowledgement of a write that answered none
    /// of our writes: another program is writing to the keyboard (for
    /// example the vendor site's empty-mask trigger write at connect). Also
    /// counted in [`ignored`](Self::ignored).
    pub foreign_acks: u64,
    /// Unsolicited reports that our own writes cause and that were expected
    /// (the profile push after a select, the status push after a
    /// music-rhythm leave, a late acknowledgement).
    pub expected_reports_consumed: u64,
    /// Reads interrupted by a signal and retried.
    pub interrupted: u64,
    /// SOCD pushes (`24 02 00 00 06 PP`, Fn + Right Shift on the keyboard)
    /// that answered none of our reads. A user action, never another
    /// program.
    pub socd_pushes: u64,
    /// Travel-stream reports (`21 .. (05|03) 01`) received, whether a test
    /// of ours was open or not.
    pub travel_reports: u64,
    /// Travel-stream reports received while no travel test of ours was open
    /// (outside a short grace period after our close): a test left open by
    /// an earlier run.
    pub stray_travel_reports: u64,
    /// Travel-stream reports dropped because the queue of the open test was
    /// full.
    pub travel_dropped: u64,
}

/// Whether a live travel test (spec section 4.11.7) may be open on the
/// keyboard, as far as this session knows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelState {
    /// No test of this session is open (stray stream reports are counted
    /// separately, see [`Session::stray_travel_since_gate`]).
    #[default]
    Closed,
    /// A test of this session is open; stream reports are queued for it.
    Open,
    /// The close of a test was not acknowledged: the keyboard may still be
    /// streaming. Table writes are refused until a close is acknowledged.
    MaybeOpen,
}

/// At most this many travel-stream reports wait for the open test; older
/// ones are kept, newer ones dropped ([`SessionStats::travel_dropped`]).
const TRAVEL_QUEUE_CAP: usize = 1024;

/// Stream reports that arrive this soon after our acknowledged close are
/// still in flight, not stray.
pub(crate) const TRAVEL_CLOSE_GRACE: Duration = Duration::from_millis(100);

/// What a session saw that suggests another program is using the keyboard.
///
/// Counted since the session was opened: status reports that answered none
/// of our requests (the vendor site's heartbeat replies), acknowledgements of
/// writes we did not send, and profile reports we did not cause.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ForeignActivity {
    /// Status reports we did not request.
    pub status_reports: u64,
    /// Write acknowledgements we did not cause.
    pub foreign_acks: u64,
    /// Profile reports we did not cause.
    pub profile_reports: u64,
}

impl ForeignActivity {
    /// Whether nothing suspicious was seen.
    pub fn is_quiet(&self) -> bool {
        self.status_reports == 0 && self.foreign_acks == 0 && self.profile_reports == 0
    }

    /// What was seen after `earlier` (a value taken from the same session).
    pub fn since(&self, earlier: &ForeignActivity) -> ForeignActivity {
        ForeignActivity {
            status_reports: self.status_reports.saturating_sub(earlier.status_reports),
            foreign_acks: self.foreign_acks.saturating_sub(earlier.foreign_acks),
            profile_reports: self.profile_reports.saturating_sub(earlier.profile_reports),
        }
    }
}

impl fmt::Display for ForeignActivity {
    /// E.g. `2 status report(s) we did not request, 1 write acknowledgement(s) we did not cause`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.status_reports > 0 {
            parts.push(format!(
                "{} status report(s) we did not request",
                self.status_reports
            ));
        }
        if self.foreign_acks > 0 {
            parts.push(format!(
                "{} write acknowledgement(s) we did not cause",
                self.foreign_acks
            ));
        }
        if self.profile_reports > 0 {
            parts.push(format!(
                "{} profile report(s) we did not cause",
                self.profile_reports
            ));
        }
        if parts.is_empty() {
            f.write_str("no foreign activity")
        } else {
            f.write_str(&parts.join(", "))
        }
    }
}

/// What [`Session::listen`] heard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ListenSummary {
    /// Input reports received.
    pub reports: u64,
    /// The foreign activity among them.
    pub activity: ForeignActivity,
}

/// How a report that is not part of a pending reply was classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unrelated {
    /// The keyboard's reset notification. Never answered.
    Reset,
    /// A report our own writes cause (consumed).
    Expected,
    /// A status report we did not request.
    ForeignStatus,
    /// An acknowledgement of a write we did not send.
    ForeignAck,
    /// A profile report we did not cause.
    ProfileReport {
        /// The profile it names.
        profile: u8,
        /// Byte 7: `00` in the push after a profile switch, `01` in the
        /// reply to a `21/11` query.
        extra: u8,
    },
    /// A report the keyboard sends on its own because of a key press (the
    /// SOCD push after Fn + Right Shift). Never another program.
    KeyboardEvent,
    /// A travel-stream report (queued for an open test, or counted as
    /// stray). Never another program.
    Travel,
    /// Anything else (another program's read replies, ...).
    Other,
}

/// An unsolicited report one of our writes may still cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expect {
    /// The late acknowledgement (or profile push) of a write whose wait
    /// timed out.
    Ack(AckSpec),
    /// The profile push of a select whose push did not arrive in time.
    ProfilePush(ProfileId),
    /// The status report the keyboard pushes after a music-rhythm leave.
    StatusPush,
}

impl Expect {
    fn matches(&self, report: &Report) -> bool {
        match self {
            Expect::Ack(spec) => spec.tracker().offer(report) != AckOffer::Ignored,
            Expect::ProfilePush(profile) => {
                let b = report.as_bytes();
                b[..4] == [0x21, 0, 0, 0] && b[5] == 0x11 && b[6] == profile.get()
            }
            Expect::StatusPush => report.cmd() == 0x01,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Expectation {
    what: Expect,
    until: Instant,
}

/// At most this many expected reports are remembered.
const MAX_EXPECTATIONS: usize = 8;

/// A read is retried at most this many times after being interrupted by a
/// signal; after that it counts as a timeout (never as a lost connection).
const MAX_INTERRUPTIONS: u32 = 64;

/// Why a session must not be written to any more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Fatal {
    /// The keyboard sent its reset notification (hex dump).
    Reset(String),
    /// The transport failed (message).
    Disconnected(String),
    /// A write packet failed the egress allowlist and was not sent (hex);
    /// a programming error, so nothing more is written.
    Refused(String),
}

/// A session with one keyboard.
///
/// It reads; writing needs a [`WriteTransport`] and
/// [`Session::writer`](Session::writer).
///
/// ```
/// # #[cfg(feature = "mock")] {
/// use openwraith_hid::{MockTransport, Session};
/// use openwraith_hid::protocol::{ReadRequest, Report};
///
/// let mut mock = MockTransport::new();
/// let mut reply = [0u8; 63];
/// reply[..8].copy_from_slice(&[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x01]);
/// mock.add_reply(ReadRequest::ActiveProfile, vec![Report::new(reply)]);
///
/// let mut session = Session::new(mock);
/// let profile: openwraith_hid::protocol::ActiveProfile =
///     session.read(ReadRequest::ActiveProfile)?;
/// assert_eq!(profile.profile, 1);
/// # }
/// # Ok::<(), openwraith_hid::SessionError>(())
/// ```
///
/// A session over a read-only transport has no `writer`:
///
/// ```compile_fail
/// use openwraith_hid::{ConsentOrigin, HidTransport, Session, WriteConsent};
/// fn try_to_write(session: &mut Session<HidTransport>) {
///     let _ = session.writer(WriteConsent::new(ConsentOrigin::CommandLineFlag));
/// }
/// ```
pub struct Session<T: Transport> {
    transport: T,
    options: SessionOptions,
    stats: SessionStats,
    events: Vec<DeviceEvent>,
    /// The reports the [`events`](Self::events) were decoded from, in the
    /// same order (what arrived, byte for byte).
    event_reports: Vec<Report>,
    opened: Instant,
    expected: Vec<Expectation>,
    fatal: Option<Fatal>,
    /// The last active profile the keyboard reported to one of our requests
    /// (`21/11`) or selects.
    last_profile: Option<u8>,
    /// Where the "other program" gate starts looking (see
    /// [`Session::rearm_gate`]).
    gate: GateMark,
    /// The last write packet that changes a setting; see
    /// [`Session::last_write_at`].
    last_write: Option<LastWrite>,
    /// When [`fatal`](Self::fatal) was first set (on the transport clock).
    fatal_at: Option<Instant>,
    /// Whether a travel test of this session is open (see [`TravelState`]).
    travel: TravelState,
    /// When our last close of a travel test was acknowledged.
    travel_closed_at: Option<Instant>,
    /// Stream reports for the open travel test, oldest first.
    travel_queue: VecDeque<TravelReport>,
    /// Stream reports dropped since the queue was last taken.
    travel_queue_dropped: u64,
    /// [`SessionStats::stray_travel_reports`] at the gate mark (or at the
    /// last acknowledged close of a stray test).
    stray_mark: u64,
    /// A base keymap was sent and no complete Fn-keymap write followed yet
    /// (see [`Session::base_keymap_unsaved`]).
    base_keymap_unsaved: bool,
    /// When the last report of another program (status or foreign ack) was
    /// seen.
    last_foreign_at: Option<Instant>,
}

/// The last write packet of a session that changes a setting (see
/// [`Session::last_write_at`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LastWrite {
    /// When it was sent, on the transport clock.
    pub(crate) at: Instant,
    /// The profile it went to, as the writer had last confirmed it.
    pub(crate) profile: Option<u8>,
    /// The keyboard switched to another profile before the commit settle
    /// had passed: it reloaded the profile and probably discarded the write
    /// (spec 4.5.3, "Delayed commit"). Nothing is pending any more.
    pub(crate) discarded: bool,
}

/// The point from which the "other program" gate looks at the session.
#[derive(Debug, Clone, Copy)]
struct GateMark {
    at: Instant,
    activity: ForeignActivity,
    events: usize,
}

impl<T: Transport> Session<T> {
    /// A session with the default options.
    pub fn new(transport: T) -> Self {
        Self::with_options(transport, SessionOptions::default())
    }

    /// A session with custom options.
    pub fn with_options(transport: T, options: SessionOptions) -> Self {
        let opened = transport.now();
        Session {
            transport,
            options,
            stats: SessionStats::default(),
            events: Vec::new(),
            event_reports: Vec::new(),
            opened,
            expected: Vec::new(),
            fatal: None,
            last_profile: None,
            gate: GateMark {
                at: opened,
                activity: ForeignActivity::default(),
                events: 0,
            },
            last_write: None,
            fatal_at: None,
            travel: TravelState::Closed,
            travel_closed_at: None,
            travel_queue: VecDeque::new(),
            travel_queue_dropped: 0,
            stray_mark: 0,
            base_keymap_unsaved: false,
            last_foreign_at: None,
        }
    }

    /// SOCD pushes the keyboard sent since the session was opened (Fn +
    /// Right Shift; [`SessionStats::socd_pushes`]).
    pub fn socd_pushes(&self) -> u64 {
        self.stats.socd_pushes
    }

    /// Whether a travel test of this session is open, or may still be open
    /// after an unacknowledged close.
    pub fn travel_state(&self) -> TravelState {
        self.travel
    }

    /// Travel-stream reports that arrived while no test of ours was open,
    /// since the gate mark ([`rearm_gate`](Self::rearm_gate)) or the last
    /// acknowledged close of such a stray test. Table writes are refused
    /// while this is not zero.
    pub fn stray_travel_since_gate(&self) -> u64 {
        self.stats
            .stray_travel_reports
            .saturating_sub(self.stray_mark)
    }

    /// Whether a base keymap (`18 00`) was sent in this session and no
    /// complete Fn-keymap write (`18 02`, every packet acknowledged)
    /// followed it yet: the keyboard keeps the base keymap only then, and a
    /// profile switch or a power loss now would drop it (spec 4.8.2). Kept
    /// by the session, so it also covers later writers.
    pub fn base_keymap_unsaved(&self) -> bool {
        self.base_keymap_unsaved
    }

    /// When the last report of another program (a status report we did not
    /// request or an acknowledgement of a write we did not send) arrived,
    /// on the transport clock.
    pub fn last_foreign_at(&self) -> Option<Instant> {
        self.last_foreign_at
    }

    /// Records whether a base keymap waits for its Fn-keymap write.
    pub(crate) fn set_base_keymap_unsaved(&mut self, unsaved: bool) {
        self.base_keymap_unsaved = unsaved;
    }

    /// Sets the travel state; an acknowledged close also starts the grace
    /// period for reports still in flight.
    pub(crate) fn set_travel(&mut self, state: TravelState) {
        if self.travel == TravelState::Open && state == TravelState::Closed {
            self.travel_closed_at = Some(self.now());
        }
        self.travel = state;
    }

    /// After an acknowledged close of a stray test: earlier stray reports no
    /// longer refuse writes.
    pub(crate) fn rearm_stray_mark(&mut self) {
        self.stray_mark = self.stats.stray_travel_reports;
        self.travel_closed_at = Some(self.now());
    }

    /// The queued stream reports of the open test and how many were dropped
    /// since the last call.
    pub(crate) fn take_travel_reports(&mut self) -> (Vec<TravelReport>, u64) {
        let reports = self.travel_queue.drain(..).collect();
        (reports, std::mem::take(&mut self.travel_queue_dropped))
    }

    /// When this session last sent a write packet that changes a setting
    /// (anything but a profile select), on the transport's clock
    /// ([`Transport::now`]); `None` if it never did.
    ///
    /// The keyboard saves such a write about 1 s later; a profile switch or a
    /// power loss before that loses it (spec 4.5.3, "Delayed commit"). A
    /// [`Writer`](crate::Writer) therefore waits its
    /// [`commit_settle`](crate::WriteOptions::commit_settle) after this time
    /// before it selects another profile. The time is kept by the session,
    /// so it also covers later writers of a long-lived session.
    pub fn last_write_at(&self) -> Option<Instant> {
        self.last_write.map(|w| w.at)
    }

    /// The profile the last write that changes a setting went to (see
    /// [`last_write_at`](Self::last_write_at)), as the writer had last
    /// confirmed it; `None` if this session never wrote one or the profile
    /// was not known.
    pub fn last_write_profile(&self) -> Option<u8> {
        self.last_write.and_then(|w| w.profile)
    }

    /// The last write that changes a setting, if any.
    pub(crate) fn last_write(&self) -> Option<LastWrite> {
        self.last_write
    }

    /// Records that the keyboard switched profiles before it had saved the
    /// last write (see [`LastWrite::discarded`]): nothing is pending any
    /// more.
    pub(crate) fn discard_pending_write(&mut self) {
        if let Some(last) = &mut self.last_write {
            last.discarded = true;
        }
    }

    /// When the session first became unusable (reset notification or lost
    /// connection), on the transport clock.
    pub(crate) fn fatal_at(&self) -> Option<Instant> {
        self.fatal_at
    }

    /// Marks the session as unusable; the time of the first such event is
    /// kept.
    fn set_fatal(&mut self, fatal: Fatal) {
        if self.fatal.is_none() {
            self.fatal_at = Some(self.now());
        }
        self.fatal = Some(fatal);
    }

    /// The current time on the transport's clock ([`Transport::now`]).
    pub(crate) fn now(&self) -> Instant {
        self.transport.now()
    }

    /// The transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Ends the session and returns the transport.
    pub fn into_transport(self) -> T {
        self.transport
    }

    /// Counters of what the session saw so far.
    pub fn stats(&self) -> &SessionStats {
        &self.stats
    }

    /// Noteworthy unsolicited reports seen so far, in arrival order: profile
    /// reports that did not answer one of our requests (for example after a
    /// profile change on the keyboard) and the keyboard's SOCD pushes
    /// ([`DeviceEvent::SocdPower`]). Status reports are only counted (see
    /// [`SessionStats::status_reports`]); a keyboard reset ends the request
    /// with an error instead; travel-stream reports are never listed.
    /// Profile pushes caused by our own profile selects are not listed.
    pub fn events(&self) -> &[DeviceEvent] {
        &self.events
    }

    /// The reports [`events`](Self::events) were decoded from, byte for
    /// byte, in the same order (`event_reports()[i]` is the report of
    /// `events()[i]`): what the keyboard really sent, for a tool that shows
    /// it.
    pub fn event_reports(&self) -> &[Report] {
        &self.event_reports
    }

    /// How long ago the session was opened: the time during which reports
    /// from another program would have been noticed.
    pub fn observed_for(&self) -> Duration {
        self.now().saturating_duration_since(self.opened)
    }

    /// Starts the "other program" gate of [`Session::writer`] afresh: only
    /// foreign activity seen from now on counts, and the gate needs a new
    /// observation period before the next writer.
    ///
    /// For a long-lived session (a GUI): call it after the user confirmed
    /// that the vendor site is closed, or after a profile change the user
    /// made on the keyboard. [`foreign_activity`](Self::foreign_activity)
    /// itself keeps counting everything since the session was opened.
    pub fn rearm_gate(&mut self) {
        self.gate = GateMark {
            at: self.now(),
            activity: self.foreign_activity(),
            events: self.events.len(),
        };
        self.stray_mark = self.stats.stray_travel_reports;
    }

    /// How long the gate has observed the keyboard: since the session was
    /// opened or since the last [`rearm_gate`](Self::rearm_gate).
    pub fn observed_since_gate(&self) -> Duration {
        self.now().saturating_duration_since(self.gate.at)
    }

    /// Foreign activity the gate counts: since the session was opened or
    /// since the last [`rearm_gate`](Self::rearm_gate).
    pub fn activity_since_gate(&self) -> ForeignActivity {
        self.foreign_activity().since(&self.gate.activity)
    }

    /// Every profile report we did not cause since the gate mark: `(profile,
    /// byte 7)`, oldest first.
    pub(crate) fn foreign_profile_reports(&self) -> Vec<(u8, u8)> {
        self.events
            .get(self.gate.events..)
            .unwrap_or_default()
            .iter()
            .filter_map(|event| match event {
                DeviceEvent::ProfileReport { profile, extra } => Some((*profile, *extra)),
                _ => None,
            })
            .collect()
    }

    /// The profile named by the last profile report we did not cause since
    /// the gate mark, if any.
    pub(crate) fn last_foreign_profile_report(&self) -> Option<u8> {
        self.events
            .get(self.gate.events..)
            .unwrap_or_default()
            .iter()
            .rev()
            .find_map(|event| match event {
                DeviceEvent::ProfileReport { profile, .. } => Some(*profile),
                _ => None,
            })
    }

    /// The last active profile the keyboard reported to one of our `21/11`
    /// requests or profile selects in this session.
    pub fn last_known_profile(&self) -> Option<u8> {
        self.last_profile
    }

    /// Evidence of another program seen since the session was opened.
    pub fn foreign_activity(&self) -> ForeignActivity {
        let profile_reports = self
            .events
            .iter()
            .filter(|e| matches!(e, DeviceEvent::ProfileReport { .. }))
            .count();
        ForeignActivity {
            status_reports: self.stats.status_reports,
            foreign_acks: self.stats.foreign_acks,
            profile_reports: u64::try_from(profile_reports).unwrap_or(u64::MAX),
        }
    }

    /// Whether the keyboard sent its reset notification during this session.
    /// A [`Writer`](crate::Writer) sends nothing after that.
    pub fn keyboard_reset_seen(&self) -> bool {
        matches!(self.fatal, Some(Fatal::Reset(_)))
    }

    /// Whether the transport failed (keyboard unplugged?). A
    /// [`Writer`](crate::Writer) sends nothing after that.
    pub fn connection_lost(&self) -> bool {
        matches!(self.fatal, Some(Fatal::Disconnected(_)))
    }

    /// Reads input reports for `duration` without sending anything, and
    /// classifies them (status reports, acknowledgements and profile reports
    /// of another program are counted in [`foreign_activity`](Self::foreign_activity)).
    /// Queued reports are drained even when `duration` is zero.
    ///
    /// Stops with [`ListenError::KeyboardReset`] on the keyboard's reset
    /// notification (which is never answered).
    pub fn listen(&mut self, duration: Duration) -> Result<ListenSummary, ListenError> {
        self.listen_until(duration, false)
    }

    /// [`listen`](Self::listen); with `stop_at_profile_report`, it returns
    /// right after the first profile report we did not cause (a profile
    /// change must be acted on at once, not when the listen is over).
    pub(crate) fn listen_until(
        &mut self,
        duration: Duration,
        stop_at_profile_report: bool,
    ) -> Result<ListenSummary, ListenError> {
        let before = self.foreign_activity();
        let deadline = self.now() + duration;
        let mut reports = 0u64;
        let mut polls = 0usize;
        loop {
            let remaining = deadline.saturating_duration_since(self.now());
            if remaining.is_zero() {
                // After the deadline only queued reports are drained, at most
                // `max_drain` of them.
                if polls >= self.options.max_drain {
                    break;
                }
                polls += 1;
            }
            let Some(report) = self.read_input(remaining)? else {
                break;
            };
            reports += 1;
            match self.classify_unrelated(&report) {
                Unrelated::Reset => {
                    return Err(ListenError::KeyboardReset {
                        report: report.summary(),
                    });
                }
                Unrelated::ProfileReport { .. } if stop_at_profile_report => break,
                _ => {}
            }
        }
        Ok(ListenSummary {
            reports,
            activity: self.foreign_activity().since(&before),
        })
    }

    /// Sends `request` and returns its complete reply.
    ///
    /// The request is sent again (up to [`SessionOptions::retries`] times)
    /// when it times out, and when two different copies of a chunk arrive.
    pub fn request(&mut self, request: ReadRequest) -> Result<RawResponse, SessionError> {
        self.stats.requests += 1;
        let timeout = self.options.timeout.unwrap_or_else(|| request.timeout());
        let attempts = self.options.retries.saturating_add(1);
        let mut collector = request.collector();
        let mut last = Collected::TimedOut;

        // Only before the first attempt: on a retry, late chunks of the
        // first attempt are welcome.
        self.drain(request)?;
        for attempt in 1..=attempts {
            if attempt > 1 {
                self.stats.retries += 1;
            }
            self.transport_write(&request.packet())?;
            last = self.collect(&mut collector, timeout)?;
            match last {
                Collected::Complete => {
                    let response = collector.finish()?;
                    if request == ReadRequest::ActiveProfile
                        && let Some(profile) = response.payload().get(1)
                    {
                        self.last_profile = Some(*profile);
                    }
                    return Ok(response);
                }
                Collected::TimedOut => {}
                // The data changed between two copies of a chunk: start over
                // instead of completing the reply from two different states.
                Collected::Conflict => collector.reset(),
            }
        }
        match last {
            Collected::Conflict => Err(SessionError::Inconsistent { request }),
            Collected::Complete | Collected::TimedOut => Err(SessionError::Timeout {
                request,
                attempts,
                received: collector.received(),
                expected: collector.expected(),
            }),
        }
    }

    /// Sends `request` and decodes its reply into `M`.
    pub fn read<M: FromResponse>(&mut self, request: ReadRequest) -> Result<M, SessionError> {
        let response = self.request(request)?;
        M::from_response(&response).map_err(|source| SessionError::Decode { request, source })
    }

    /// Discards stale input before a request.
    fn drain(&mut self, request: ReadRequest) -> Result<(), SessionError> {
        for _ in 0..self.options.max_drain {
            match self.transport_read(Duration::ZERO) {
                Ok(Some(report)) => {
                    self.stats.drained += 1;
                    self.note_unrelated(request, &report)?;
                }
                Ok(None) => return Ok(()),
                Err(TransportError::Malformed(_)) => self.stats.malformed += 1,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    /// Feeds input reports to `collector` until it is complete, nothing
    /// useful arrived for `timeout`, or a chunk arrived twice with different
    /// data.
    fn collect(
        &mut self,
        collector: &mut ResponseCollector,
        timeout: Duration,
    ) -> Result<Collected, SessionError> {
        let request = collector.request();
        let mut deadline = self.now() + timeout;
        while !collector.is_complete() {
            let remaining = deadline.saturating_duration_since(self.now());
            if remaining.is_zero() {
                return Ok(Collected::TimedOut);
            }
            let report = match self.transport_read(remaining) {
                Ok(Some(report)) => report,
                Ok(None) => return Ok(Collected::TimedOut),
                Err(TransportError::Malformed(_)) => {
                    self.stats.malformed += 1;
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            if is_keyboard_reset(&report) {
                self.set_fatal(Fatal::Reset(report.summary()));
                return Err(reset_error(request, &report));
            }
            match collector.offer(&report) {
                Offer::Accepted => {
                    self.stats.chunks += 1;
                    // Keep waiting as long as the reply is still streaming.
                    deadline = self.now() + timeout;
                }
                Offer::Duplicate => self.stats.duplicates += 1,
                Offer::Conflict => {
                    self.stats.conflicts += 1;
                    return Ok(Collected::Conflict);
                }
                Offer::Ignored => self.note_unrelated(request, &report)?,
            }
        }
        Ok(Collected::Complete)
    }

    /// Records a report that is not part of the pending reply.
    fn note_unrelated(
        &mut self,
        request: ReadRequest,
        report: &Report,
    ) -> Result<(), SessionError> {
        match self.classify_unrelated(report) {
            Unrelated::Reset => Err(reset_error(request, report)),
            _ => Ok(()),
        }
    }

    /// Classifies and counts a report that is not part of a pending reply.
    /// A reset notification marks the session so that nothing is ever
    /// written after it.
    pub(crate) fn classify_unrelated(&mut self, report: &Report) -> Unrelated {
        if is_keyboard_reset(report) {
            self.set_fatal(Fatal::Reset(report.summary()));
            return Unrelated::Reset;
        }
        let now = self.now();
        self.expected.retain(|e| e.until >= now);
        if let Some(pos) = self.expected.iter().position(|e| e.what.matches(report)) {
            self.expected.remove(pos);
            self.stats.expected_reports_consumed += 1;
            return Unrelated::Expected;
        }
        match DeviceEvent::classify(report) {
            Some(DeviceEvent::KeyboardReset) => Unrelated::Reset,
            Some(DeviceEvent::Status) => {
                self.stats.status_reports += 1;
                self.last_foreign_at = Some(now);
                Unrelated::ForeignStatus
            }
            Some(event @ DeviceEvent::ProfileReport { profile, extra }) => {
                self.stats.ignored += 1;
                self.events.push(event);
                self.event_reports.push(report.clone());
                Unrelated::ProfileReport { profile, extra }
            }
            // Fn + Right Shift on the keyboard: a user action (c15), never
            // another program.
            Some(event @ DeviceEvent::SocdPower { .. }) => {
                self.stats.socd_pushes += 1;
                self.events.push(event);
                self.event_reports.push(report.clone());
                Unrelated::KeyboardEvent
            }
            // The live travel stream: for the open test, or stray. Never
            // another program, never an event.
            Some(DeviceEvent::TravelStream(stream)) => {
                self.note_travel(stream, now);
                Unrelated::Travel
            }
            None if is_write_ack(report) => {
                self.stats.ignored += 1;
                self.stats.foreign_acks += 1;
                self.last_foreign_at = Some(now);
                Unrelated::ForeignAck
            }
            None => {
                self.stats.ignored += 1;
                Unrelated::Other
            }
        }
    }

    /// Queues a stream report for the open travel test, or counts it as
    /// stray when no test of ours is open (outside the grace period after
    /// our last acknowledged close).
    fn note_travel(&mut self, stream: TravelReport, now: Instant) {
        self.stats.travel_reports += 1;
        if self.travel == TravelState::Open {
            if self.travel_queue.len() >= TRAVEL_QUEUE_CAP {
                self.stats.travel_dropped += 1;
                self.travel_queue_dropped += 1;
            } else {
                self.travel_queue.push_back(stream);
            }
            return;
        }
        let in_grace = self.travel == TravelState::Closed
            && self
                .travel_closed_at
                .is_some_and(|at| now.saturating_duration_since(at) <= TRAVEL_CLOSE_GRACE);
        if !in_grace {
            self.stats.stray_travel_reports += 1;
        }
    }

    /// Remembers that one of our writes may still cause `what` within
    /// `window`; such a report is then consumed instead of being counted as
    /// another program's.
    pub(crate) fn expect(&mut self, what: Expect, window: Duration) {
        let now = self.now();
        self.expected.retain(|e| e.until >= now);
        if self.expected.len() >= MAX_EXPECTATIONS {
            self.expected.remove(0);
        }
        self.expected.push(Expectation {
            what,
            until: now + window,
        });
    }

    /// Until when an unexpired expectation equal to `what` is pending, if
    /// one is.
    pub(crate) fn pending_expect(&self, what: &Expect) -> Option<Instant> {
        let now = self.now();
        self.expected
            .iter()
            .filter(|e| e.what == *what && e.until >= now)
            .map(|e| e.until)
            .max()
    }

    /// Why nothing may be written any more, if so.
    pub(crate) fn fatal(&self) -> Option<&Fatal> {
        self.fatal.as_ref()
    }

    /// Reads one input report, waiting up to `timeout`; malformed reports
    /// are counted and skipped. `Ok(None)` when nothing arrived.
    pub(crate) fn read_input(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Report>, TransportError> {
        let deadline = self.now() + timeout;
        for _ in 0..=self.options.max_drain {
            let remaining = deadline.saturating_duration_since(self.now());
            match self.transport_read(remaining) {
                Ok(report) => return Ok(report),
                Err(TransportError::Malformed(_)) => self.stats.malformed += 1,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }

    /// Reads from the transport; a failure other than a malformed report
    /// marks the connection as lost. A read interrupted by a signal is
    /// retried with the remaining time (after too many interruptions it
    /// counts as a timeout), and never marks the connection as lost.
    fn transport_read(&mut self, timeout: Duration) -> Result<Option<Report>, TransportError> {
        let deadline = self.now() + timeout;
        let mut interruptions = 0u32;
        loop {
            let remaining = deadline.saturating_duration_since(self.now());
            let result = self.transport.read(remaining);
            match &result {
                Err(TransportError::Interrupted) => {
                    self.stats.interrupted += 1;
                    interruptions += 1;
                    if interruptions > MAX_INTERRUPTIONS {
                        return Ok(None);
                    }
                    continue;
                }
                Ok(_) | Err(TransportError::Malformed(_)) => {}
                Err(e) => self.set_fatal(Fatal::Disconnected(e.to_string())),
            }
            return result;
        }
    }

    /// Writes a read request; a failure marks the connection as lost.
    fn transport_write(&mut self, packet: &OutPacket) -> Result<(), TransportError> {
        let result = self.transport.write(packet);
        if let Err(e) = &result {
            self.set_fatal(Fatal::Disconnected(e.to_string()));
        }
        result
    }
}

impl<T: WriteTransport> Session<T> {
    /// Sends one write packet of a table write to `profile` (the active
    /// profile as the writer last confirmed it; used by
    /// [`Writer`](crate::Writer) only) and records it for the commit settle
    /// ([`last_write_at`](Self::last_write_at)); a failure marks the
    /// connection as lost. The write is recorded even then: the packet may
    /// have reached the keyboard.
    pub(crate) fn send_write(
        &mut self,
        packet: &WritePacket,
        profile: Option<u8>,
    ) -> Result<(), TransportError> {
        self.last_write = Some(LastWrite {
            at: self.now(),
            profile,
            discarded: false,
        });
        self.send(packet)
    }

    /// Sends a profile select (used by [`Writer`](crate::Writer) only): it
    /// changes no setting, so it does not restart the commit settle.
    pub(crate) fn send_select(&mut self, packet: &WritePacket) -> Result<(), TransportError> {
        self.send(packet)
    }

    /// Sends a travel-test open or close (used by the travel guard only): it
    /// stores nothing, so it does not restart the commit settle.
    pub(crate) fn send_control(&mut self, packet: &WritePacket) -> Result<(), TransportError> {
        self.send(packet)
    }

    /// Sends one write packet; a failure marks the connection as lost. A
    /// packet outside the egress allowlist ([`is_permitted_packet`]) is not
    /// sent, and the session refuses every later write.
    fn send(&mut self, packet: &WritePacket) -> Result<(), TransportError> {
        if !is_permitted_packet(packet) {
            self.set_fatal(Fatal::Refused(packet.summary()));
            return Err(TransportError::Refused(packet.summary()));
        }
        let result = self.transport.write_command(packet);
        match &result {
            Ok(()) => self.stats.writes += 1,
            Err(e) => self.set_fatal(Fatal::Disconnected(e.to_string())),
        }
        result
    }

    /// Counts an acknowledgement of one of our writes.
    pub(crate) fn count_ack(&mut self) {
        self.stats.acks += 1;
    }

    /// Counts an expected unsolicited report (a profile push after a select).
    pub(crate) fn count_expected(&mut self) {
        self.stats.expected_reports_consumed += 1;
    }
}

/// How one attempt of a request ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Collected {
    /// Every chunk arrived.
    Complete,
    /// Nothing useful arrived before the deadline.
    TimedOut,
    /// A chunk arrived twice with different data.
    Conflict,
}

fn reset_error(request: ReadRequest, report: &Report) -> SessionError {
    SessionError::KeyboardReset {
        request,
        report: report.summary(),
    }
}
