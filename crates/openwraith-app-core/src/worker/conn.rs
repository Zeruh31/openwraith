//! The connection state machine.
//!
//! ```text
//! Starting -> Searching --1--> Opening -> Observing (2 s, listen only) -quiet-> Reading -> Ready
//!               |  >1 -> SeveralKeyboards -select-^      | foreign status/ack -> OtherProgram -recheck-> Observing
//!               |                                        | profile reports only -> rearm, keep observing
//!               +-- Lost (auto: same keyboard back, or exactly one) <- transport error
//!               +-- OpenFailed (backoff 2/5/10/30 s)      Reading fails -> ReadFailed (manual retry)
//!               +-- Unsupported (after 0D)  ResetSeen (manual reconnect only)  Released  Fault
//! ```

use std::time::{Duration, Instant};

use openwraith_hid::Session;

use super::Worker;
use crate::backend::{DeviceBackend, DeviceEntry};
use crate::clock::millis;
use crate::dto::{
    AppError, AppErrorKind, ConnectionState, DeviceId, ForeignActivityView, Notice,
    PlanInvalidation,
};

/// How often the keyboards are listed while none is open.
pub(crate) const ENUM_INTERVAL: Duration = Duration::from_millis(1500);

/// How often the keyboards are listed while one is open (unplug check).
pub(crate) const ENUM_INTERVAL_CONNECTED: Duration = Duration::from_secs(3);

/// The observation before the first request and before every write.
pub(crate) const GATE: Duration = Duration::from_secs(2);

/// Delays between open attempts.
const BACKOFF: [Duration; 4] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

/// The connection state (the worker's own form of
/// [`ConnectionState`]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Conn {
    Starting,
    Searching,
    Several,
    OpenFailed {
        error: AppError,
        retry_at: Instant,
    },
    Observing,
    OtherProgram {
        evidence: ForeignActivityView,
    },
    Reading {
        done: u16,
        total: u16,
    },
    ReadFailed {
        error: AppError,
    },
    Ready,
    Lost {
        error: AppError,
    },
    ResetSeen,
    Unsupported {
        product: Option<String>,
        id: DeviceId,
    },
    Released,
    Fault {
        message: String,
    },
}

impl<B: DeviceBackend> Worker<B> {
    /// The connection state for the frontend.
    pub(crate) fn connection_state(&self, now: Instant) -> ConnectionState {
        match &self.conn {
            Conn::Starting => ConnectionState::Starting,
            Conn::Searching => ConnectionState::Searching,
            Conn::Several => ConnectionState::SeveralKeyboards {
                devices: self.listed.iter().map(DeviceEntry::view).collect(),
            },
            Conn::OpenFailed { error, retry_at } => ConnectionState::OpenFailed {
                error: error.clone(),
                // Whole seconds: the status changes once a second, not at
                // every tick.
                retry_in_ms: millis(retry_at.saturating_duration_since(now)).div_ceil(1000) * 1000,
            },
            Conn::Observing => ConnectionState::Observing {
                required_ms: millis(GATE),
            },
            Conn::OtherProgram { evidence } => ConnectionState::OtherProgram {
                evidence: *evidence,
            },
            Conn::Reading { done, total } => ConnectionState::Reading {
                done: *done,
                total: *total,
            },
            Conn::ReadFailed { error } => ConnectionState::ReadFailed {
                error: error.clone(),
            },
            Conn::Ready => ConnectionState::Ready,
            Conn::Lost { error } => ConnectionState::Lost {
                error: error.clone(),
            },
            Conn::ResetSeen => ConnectionState::ResetSeen,
            Conn::Unsupported { product, .. } => ConnectionState::Unsupported {
                product: product.clone(),
            },
            Conn::Released => ConnectionState::Released,
            Conn::Fault { message } => ConnectionState::Fault {
                message: message.clone(),
            },
        }
    }

    /// Lists the keyboards (when due) and opens one when the state allows
    /// it automatically.
    pub(crate) fn search(&mut self, force: bool) {
        let now = self.now();
        if !force && self.next_enum.is_some_and(|due| now < due) {
            return;
        }
        self.next_enum = Some(now + ENUM_INTERVAL);
        let Ok(listed) = self.backend.enumerate() else {
            return;
        };
        self.listed = listed;
        match &self.conn {
            Conn::Unsupported { id, .. } => {
                if !self.listed.iter().any(|e| e.id == *id) {
                    self.conn = Conn::Searching;
                }
                return;
            }
            Conn::OpenFailed { retry_at, .. } if !force && now < *retry_at => return,
            Conn::Released | Conn::ResetSeen | Conn::Fault { .. } => return,
            _ => {}
        }
        if self.listed.is_empty() {
            // A lost keyboard stays "lost" until it is back; a failed open
            // whose keyboard is gone goes back to searching.
            if !matches!(self.conn, Conn::Lost { .. }) {
                self.conn = Conn::Searching;
                self.open_attempts = 0;
            }
            return;
        }
        let preferred = self
            .preferred
            .as_ref()
            .and_then(|id| self.listed.iter().find(|e| e.id == *id).cloned());
        let choice = preferred.or_else(|| match self.listed.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        });
        match choice {
            Some(entry) => self.open(entry),
            None => self.conn = Conn::Several,
        }
    }

    /// Opens `entry`: a new session (a new generation) that starts with
    /// the listen-only observation.
    pub(crate) fn open(&mut self, entry: DeviceEntry) {
        if matches!(self.conn, Conn::Fault { .. }) {
            // Writes stay blocked until the app restarts (D13).
            return;
        }
        self.drop_session();
        match self.backend.open(&entry) {
            Ok(transport) => {
                self.session = Some(Session::new(transport));
                self.generation = self.generation.wrapping_add(1);
                self.preferred = Some(entry.id.clone());
                self.device = Some(entry);
                self.open_attempts = 0;
                self.read_retries = 0;
                self.stale = false;
                self.profile_seen_at = None;
                self.reread = false;
                self.notify_connected = true;
                self.socd_pushes_seen = 0;
                self.stray_notified = false;
                self.clear_snapshot();
                self.invalidate(PlanInvalidation::Disconnected);
                self.conn = Conn::Observing;
                self.next_enum = Some(self.now() + ENUM_INTERVAL_CONNECTED);
            }
            Err(error) => {
                let delay = BACKOFF
                    .get(usize::from(self.open_attempts))
                    .or(BACKOFF.last())
                    .copied()
                    .unwrap_or(Duration::from_secs(30));
                self.open_attempts = self.open_attempts.saturating_add(1);
                let retry_at = self.now() + delay;
                self.next_enum = Some(retry_at);
                self.conn = Conn::OpenFailed {
                    error: AppError::from(&error),
                    retry_at,
                };
            }
        }
    }

    /// Closes the keyboard.
    pub(crate) fn drop_session(&mut self) {
        self.session = None;
    }

    /// Forgets the snapshot.
    pub(crate) fn clear_snapshot(&mut self) {
        self.snapshot = None;
        self.active_profile = None;
        self.shared.set_snapshot(None);
        self.shared.set_keys(None);
        self.shared.set_editors(None);
    }

    /// The connection was lost: the session is dropped; the worker looks for
    /// the keyboard again.
    pub(crate) fn enter_lost(&mut self, error: AppError) {
        let was_connected = self.session.is_some();
        self.drop_session();
        self.clear_snapshot();
        self.invalidate(PlanInvalidation::Disconnected);
        self.conn = Conn::Lost { error };
        self.next_enum = Some(self.now() + ENUM_INTERVAL);
        if was_connected {
            self.shared.notice(Notice::Disconnected);
        }
    }

    /// The keyboard sent its reset notification: the session is dropped at
    /// once and nothing more is sent until the user reconnects.
    pub(crate) fn enter_reset_seen(&mut self) {
        self.drop_session();
        self.clear_snapshot();
        self.invalidate(PlanInvalidation::ResetSeen);
        self.conn = Conn::ResetSeen;
    }

    /// Another program uses the keyboard: listen only, send nothing.
    pub(crate) fn enter_other_program(&mut self, evidence: ForeignActivityView) {
        self.stale = true;
        self.invalidate(PlanInvalidation::OtherProgram);
        self.conn = Conn::OtherProgram { evidence };
    }

    /// The keyboard is not a supported model.
    pub(crate) fn enter_unsupported(&mut self, product: Option<String>) {
        let id = self
            .device
            .as_ref()
            .map(|d| d.id.clone())
            .unwrap_or_else(|| DeviceId(String::new()));
        self.drop_session();
        self.clear_snapshot();
        self.conn = Conn::Unsupported { product, id };
    }

    /// Whether the opened keyboard is still listed (checked every 3 s while
    /// connected, besides the transport errors).
    pub(crate) fn check_still_listed(&mut self) {
        let now = self.now();
        if self.next_enum.is_some_and(|due| now < due) {
            return;
        }
        self.next_enum = Some(now + ENUM_INTERVAL_CONNECTED);
        let Ok(listed) = self.backend.enumerate() else {
            return;
        };
        let present = self
            .device
            .as_ref()
            .is_some_and(|d| listed.iter().any(|e| e.id == d.id));
        self.listed = listed;
        if !present {
            self.enter_lost(AppError::new(
                AppErrorKind::Disconnected,
                "the keyboard is no longer connected",
            ));
        }
    }
}
