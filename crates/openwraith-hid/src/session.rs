//! A read-only request/response session over a [`Transport`] (spec sections
//! 2.5 and 2.8).
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
//!    (the vendor site answers with a reset of its own).
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
//! [`SessionStats::status_reports`] and [`Session::events`] show whether
//! another program was active.

use std::time::{Duration, Instant};

use openwraith_protocol::{
    DeviceEvent, FromResponse, Offer, RawResponse, ReadRequest, Report, ResponseCollector,
    is_keyboard_reset,
};

use crate::error::{SessionError, TransportError};
use crate::transport::Transport;

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
    /// Other reports that did not belong to the pending request.
    pub ignored: u64,
    /// Reports with a wrong size or report ID.
    pub malformed: u64,
}

/// A read-only session with one keyboard.
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
pub struct Session<T: Transport> {
    transport: T,
    options: SessionOptions,
    stats: SessionStats,
    events: Vec<DeviceEvent>,
}

impl<T: Transport> Session<T> {
    /// A session with the default options.
    pub fn new(transport: T) -> Self {
        Self::with_options(transport, SessionOptions::default())
    }

    /// A session with custom options.
    pub fn with_options(transport: T, options: SessionOptions) -> Self {
        Session {
            transport,
            options,
            stats: SessionStats::default(),
            events: Vec::new(),
        }
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
    /// profile change on the keyboard). Status reports are only counted (see
    /// [`SessionStats::status_reports`]); a keyboard reset ends the request
    /// with an error instead.
    pub fn events(&self) -> &[DeviceEvent] {
        &self.events
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
            self.transport.write(&request.packet())?;
            last = self.collect(&mut collector, timeout)?;
            match last {
                Collected::Complete => return Ok(collector.finish()?),
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
            match self.transport.read(Duration::ZERO) {
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
        let mut deadline = Instant::now() + timeout;
        while !collector.is_complete() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(Collected::TimedOut);
            }
            let report = match self.transport.read(remaining) {
                Ok(Some(report)) => report,
                Ok(None) => return Ok(Collected::TimedOut),
                Err(TransportError::Malformed(_)) => {
                    self.stats.malformed += 1;
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            if is_keyboard_reset(&report) {
                return Err(reset_error(request, &report));
            }
            match collector.offer(&report) {
                Offer::Accepted => {
                    self.stats.chunks += 1;
                    // Keep waiting as long as the reply is still streaming.
                    deadline = Instant::now() + timeout;
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
        match DeviceEvent::classify(report) {
            Some(DeviceEvent::KeyboardReset) => return Err(reset_error(request, report)),
            Some(DeviceEvent::Status) => self.stats.status_reports += 1,
            Some(event @ DeviceEvent::ProfileReport { .. }) => {
                self.stats.ignored += 1;
                self.events.push(event);
            }
            None => self.stats.ignored += 1,
        }
        Ok(())
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
