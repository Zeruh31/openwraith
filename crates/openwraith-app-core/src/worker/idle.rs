//! What the worker does between requests: listen (sending nothing), watch
//! for other programs, profile changes on the keyboard, resets and unplugs,
//! and look for keyboards while none is open.

use std::time::Duration;

use openwraith_hid::protocol::DeviceEvent;
use openwraith_hid::{ListenError, Transport};

use super::conn::GATE;
use super::{Conn, IDLE_SLICE, Worker};
use crate::backend::DeviceBackend;
use crate::dto::{AppError, Notice, OpKind, PlanInvalidation};
use crate::map::foreign;

/// Profile reports (and nothing else) are acted on after this much more
/// listening (D6).
const PROFILE_REPORT_SETTLE: Duration = Duration::from_millis(500);

/// The focus check reads the active profile only this long after the last
/// traffic (D7).
const FOCUS_QUIET: Duration = Duration::from_secs(30);

/// What a listening tick watches for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Observing,
    Ready,
    OtherProgram,
    ReadFailed,
}

impl<B: DeviceBackend> Worker<B> {
    /// One tick (about 100 ms).
    pub(crate) fn tick(&mut self) {
        // What the session saw (also outside a listen) decides first.
        self.after_op_state();
        self.expire_plan();
        match &self.conn {
            Conn::Starting
            | Conn::Searching
            | Conn::Several
            | Conn::Lost { .. }
            | Conn::OpenFailed { .. }
            | Conn::Unsupported { .. } => {
                self.search(false);
                if self.session.is_none() {
                    self.backend.idle(IDLE_SLICE);
                }
            }
            Conn::Observing => self.listen_tick(Phase::Observing),
            Conn::Ready => {
                if self.reread {
                    self.reread = false;
                    self.connect_read(OpKind::Refresh);
                } else {
                    self.listen_tick(Phase::Ready);
                }
            }
            Conn::OtherProgram { .. } => self.listen_tick(Phase::OtherProgram),
            Conn::ReadFailed { .. } => self.listen_tick(Phase::ReadFailed),
            Conn::Reading { .. } => {}
            Conn::ResetSeen | Conn::Released | Conn::Fault { .. } => {
                self.backend.idle(IDLE_SLICE);
            }
        }
        self.publish();
    }

    /// Listens for one slice and acts on what was heard.
    fn listen_tick(&mut self, phase: Phase) {
        let Some(session) = self.session.as_mut() else {
            self.conn = Conn::Searching;
            return;
        };
        match session.listen(IDLE_SLICE) {
            Ok(_) => {}
            Err(ListenError::KeyboardReset { .. }) => {
                self.enter_reset_seen();
                return;
            }
            Err(error @ ListenError::Transport(_)) => {
                self.enter_lost(AppError::from(&error));
                return;
            }
        }
        let activity = session.activity_since_gate();
        let foreign_traffic = activity.status_reports > 0 || activity.foreign_acks > 0;
        match phase {
            Phase::Observing => {
                if foreign_traffic {
                    self.enter_other_program(foreign(activity));
                } else if activity.profile_reports > 0 {
                    // A profile change on the keyboard: start observing anew.
                    session.rearm_gate();
                } else if session.observed_since_gate() >= GATE {
                    self.connect_read(OpKind::Connect);
                    return;
                }
            }
            Phase::Ready => {
                if foreign_traffic {
                    self.enter_other_program(foreign(activity));
                } else if activity.profile_reports > 0 {
                    self.profile_reports_heard();
                    return;
                } else {
                    self.profile_seen_at = None;
                }
            }
            Phase::OtherProgram => {
                let evidence = foreign(activity);
                self.conn = Conn::OtherProgram { evidence };
            }
            Phase::ReadFailed => {
                if foreign_traffic {
                    self.enter_other_program(foreign(activity));
                }
            }
        }
        if self.session.is_some() {
            self.check_still_listed();
        }
    }

    /// Profile reports and nothing else while idle (Fn + 1..4 on the
    /// keyboard, D6): after 500 ms more, the gate is re-armed (the reports
    /// are no sign of another program), the plan invalidated and the
    /// settings read again.
    fn profile_reports_heard(&mut self) {
        let now = self.now();
        let since = *self.profile_seen_at.get_or_insert(now);
        if now.saturating_duration_since(since) < PROFILE_REPORT_SETTLE {
            return;
        }
        self.profile_changed_on_keyboard();
    }

    /// Acts on a profile change seen on the keyboard: re-arm the gate,
    /// invalidate the plan, tell the frontend, read again.
    pub(crate) fn profile_changed_on_keyboard(&mut self) {
        self.profile_seen_at = None;
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let to = session.events().iter().rev().find_map(|e| match e {
            DeviceEvent::ProfileReport { profile, .. } => Some(*profile),
            _ => None,
        });
        session.rearm_gate();
        self.invalidate(PlanInvalidation::ProfileChanged);
        let from = self.snapshot.as_ref().map(|s| s.profile);
        if let Some(to) = to {
            if Some(to) != from {
                self.shared
                    .notice(Notice::ProfileChangedOnKeyboard { from, to });
            }
        }
        self.connect_read(OpKind::Refresh);
    }

    /// The window got the focus (D7): when idle and quiet for 30 s, read the
    /// active profile once; another profile than shown means a silent switch
    /// on the keyboard: read everything again.
    pub(crate) fn window_focused(&mut self) {
        if self.conn != Conn::Ready || self.current.is_some() {
            return;
        }
        let now = self.now();
        if self
            .last_traffic
            .is_some_and(|t| now.saturating_duration_since(t) < FOCUS_QUIET)
        {
            return;
        }
        let shown = self.snapshot.as_ref().map(|s| s.profile);
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !session.activity_since_gate().is_quiet() {
            return;
        }
        let read = session.read::<openwraith_hid::protocol::ActiveProfile>(
            openwraith_hid::protocol::ReadRequest::ActiveProfile,
        );
        self.last_traffic = Some(Transport::now(session.transport()));
        match read {
            Ok(active) if Some(active.profile) != shown => {
                self.invalidate(PlanInvalidation::ProfileChanged);
                self.shared.notice(Notice::ProfileChangedOnKeyboard {
                    from: shown,
                    to: active.profile,
                });
                self.connect_read(OpKind::Refresh);
            }
            Ok(_) => {}
            Err(_) => self.after_op_state(),
        }
    }
}
