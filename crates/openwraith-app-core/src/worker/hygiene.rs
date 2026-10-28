//! Session hygiene. With the tray agent the app keeps one session open for
//! days; the session keeps every profile report and SOCD push it saw
//! (`Session::events`; travel-stream samples never enter it). An idle
//! session is therefore replaced by a new one on the same open keyboard
//! after 24 hours or 1000 events: nothing is sent, the new session's
//! "other program" gate starts afresh (a write observes 2 s again, as
//! `ensure_quiet` does anyway), and the snapshot, the plans (none) and the
//! connection's generation stay.
//!
//! Idle means: `Ready`, no operation, no request waiting, no plan, no
//! re-read due, no profile report being settled, no key-depth test open or
//! streaming, no base keymap waiting for its Fn keymap, no setting write in
//! the last 10 s (its commit settle) and no other program heard in the last
//! 120 s (the automatic profiles' gate keeps that memory).

use std::time::Duration;

use openwraith_hid::{Session, TravelState};

use super::{Conn, Worker};
use crate::backend::DeviceBackend;

/// A session older than this is recycled when idle.
pub(crate) const RECYCLE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// A session with this many events is recycled when idle.
pub(crate) const RECYCLE_EVENTS: usize = 1000;

/// No recycling this soon after a setting write (the commit settle and the
/// session's memory of it).
const WRITE_QUIET: Duration = Duration::from_secs(10);

/// No recycling this soon after another program was heard.
const FOREIGN_QUIET: Duration = Duration::from_secs(120);

impl<B: DeviceBackend> Worker<B> {
    /// Replaces an idle session that is due (see the [module
    /// documentation](self)); true when it did. Sends nothing.
    pub(crate) fn recycle_session_if_due(&mut self) -> bool {
        if self.conn != Conn::Ready
            || self.current.is_some()
            || self.pending.is_some()
            || self.reread
            || self.profile_seen_at.is_some()
            || self.owns_busy
            || self.shared.busy.load(std::sync::atomic::Ordering::SeqCst)
        {
            return false;
        }
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        let due =
            session.observed_for() >= RECYCLE_AFTER || session.events().len() >= RECYCLE_EVENTS;
        if !due {
            return false;
        }
        let now = openwraith_hid::Transport::now(session.transport());
        let quiet_since = |at: Option<std::time::Instant>, quiet: Duration| {
            at.is_none_or(|t| now.saturating_duration_since(t) >= quiet)
        };
        let idle = session.travel_state() == TravelState::Closed
            && session.stray_travel_since_gate() == 0
            && !session.base_keymap_unsaved()
            && !session.keyboard_reset_seen()
            && !session.connection_lost()
            && session.activity_since_gate().is_quiet()
            && quiet_since(session.last_write_at(), WRITE_QUIET)
            && quiet_since(session.last_foreign_at(), FOREIGN_QUIET);
        if !idle {
            return false;
        }
        let Some(old) = self.session.take() else {
            return false;
        };
        self.session = Some(Session::new(old.into_transport()));
        self.socd_pushes_seen = 0;
        self.session_recycles = self.session_recycles.saturating_add(1);
        true
    }
}
