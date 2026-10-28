//! Automatic profile switches for games (design B7): the only place of the
//! app that uses the library's `ProfileSwitcher` (consent
//! `StandingRule`, which covers profile selects only). The automatic path
//! sends nothing but the profile select and reads (the read of the new
//! profile): no table, no journal, no safety backup (a profile select
//! changes no setting, as with the manual switch).
//!
//! At every tick the worker computes the gates ([`Worker::auto_gates`]),
//! the engine (`auto/engine.rs`) decides, and [`Worker::auto_switch`] makes
//! the switch as an operation of its own (`auto_switch`): the busy flag is
//! taken like the worker's own reads (compare-exchange), the 2 s gate is
//! observed (`ensure_quiet`, which also takes the device lock), and the
//! switcher refuses on its own while a base keymap waits for its Fn keymap,
//! a key-depth test may be open, or another program was heard.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use openwraith_hid::protocol::ProfileId;
use openwraith_hid::{TravelState, WriteError};

use super::consent;
use super::progress::{CancelPolicy, OpProgress};
use super::snapshot::{ReadFail, ReadVia};
use crate::auto::engine::{Gates, Target};
use crate::backend::DeviceBackend;
use crate::dto::{AppError, AppErrorKind, AutoWaitReason, OpKind, ReadPurpose};
use crate::worker::{Conn, Worker};

/// The keyboard's commit settle: no switch earlier after a write.
const SETTLE: Duration = Duration::from_secs(2);

/// No switch within this time after another program was heard.
pub(crate) const FOREIGN_QUIET: Duration = Duration::from_secs(120);

/// How a switch ended.
enum Switch {
    /// Made: the profile active now.
    Done(u8),
    /// Not sent: a gate closed in between.
    NotStarted(AutoWaitReason),
}

/// A library refusal before anything was sent, as a wait reason.
fn not_started(error: &WriteError) -> Option<AutoWaitReason> {
    match error {
        WriteError::OtherProgramActive { .. } => Some(AutoWaitReason::OtherProgram),
        WriteError::ProfileChanged { .. } => Some(AutoWaitReason::ProfileChanging),
        WriteError::BaseKeymapUnsaved => Some(AutoWaitReason::KeymapUnsaved),
        WriteError::TravelTestMaybeOpen | WriteError::TravelTestOpen { .. } => {
            Some(AutoWaitReason::TravelTest)
        }
        WriteError::KeyboardReset { .. } => Some(AutoWaitReason::ResetSeen),
        WriteError::Disconnected { .. } => Some(AutoWaitReason::NotConnected),
        _ => None,
    }
}

impl<B: DeviceBackend> Worker<B> {
    /// Why an automatic switch may not be sent now (`None`: it may).
    pub(crate) fn auto_block(&self) -> Option<AutoWaitReason> {
        if self.paused_for_cli() {
            return Some(AutoWaitReason::PausedForCli);
        }
        match &self.conn {
            Conn::Ready => {}
            Conn::OtherProgram { .. }
                if self.auto_rearmed && self.shared.auto().background.tray_enabled => {}
            Conn::OtherProgram { .. } => return Some(AutoWaitReason::OtherProgram),
            Conn::ResetSeen => return Some(AutoWaitReason::ResetSeen),
            Conn::Released => return Some(AutoWaitReason::Released),
            Conn::Fault { .. } => return Some(AutoWaitReason::Fault),
            Conn::Observing | Conn::Reading { .. } => return Some(AutoWaitReason::Connecting),
            Conn::Starting
            | Conn::Searching
            | Conn::Several
            | Conn::OpenFailed { .. }
            | Conn::ReadFailed { .. }
            | Conn::Lost { .. }
            | Conn::Unsupported { .. } => return Some(AutoWaitReason::NotConnected),
        }
        let Some(session) = self.session.as_ref() else {
            return Some(AutoWaitReason::NotConnected);
        };
        if session.keyboard_reset_seen() {
            return Some(AutoWaitReason::ResetSeen);
        }
        if session.connection_lost() {
            return Some(AutoWaitReason::NotConnected);
        }
        if self.shared.depth_active() {
            return Some(AutoWaitReason::DepthView);
        }
        if self.current.is_some() || self.shared.busy.load(Ordering::SeqCst) {
            return Some(AutoWaitReason::Busy);
        }
        if self.pending.is_some() {
            return Some(AutoWaitReason::PlanPending);
        }
        if self.shared.auto().ui.drafts {
            return Some(AutoWaitReason::Drafts);
        }
        if session.travel_state() != TravelState::Closed
            || session.stray_travel_since_gate() > 0
            || self.shared.travel_marker_left()
        {
            return Some(AutoWaitReason::TravelTest);
        }
        if session.base_keymap_unsaved() {
            return Some(AutoWaitReason::KeymapUnsaved);
        }
        let activity = session.activity_since_gate();
        if activity.status_reports > 0 || activity.foreign_acks > 0 {
            return Some(AutoWaitReason::OtherProgram);
        }
        if self.reread || self.profile_seen_at.is_some() || activity.profile_reports > 0 {
            return Some(AutoWaitReason::ProfileChanging);
        }
        let now = openwraith_hid::Transport::now(session.transport());
        // What was heard while the CLI held the lock was the CLI's.
        let heard = session
            .last_foreign_at()
            .into_iter()
            .chain(self.foreign_seen_at)
            .filter(|at| !self.in_cli_window(*at))
            .max();
        if heard.is_some_and(|t| now.saturating_duration_since(t) < FOREIGN_QUIET) {
            return Some(AutoWaitReason::RecentForeignActivity);
        }
        None
    }

    /// The gates of an automatic switch now.
    pub(crate) fn auto_gates(&self) -> Gates {
        Gates {
            block: self.auto_block(),
            not_before: self
                .session
                .as_ref()
                .and_then(openwraith_hid::Session::last_write_at)
                .map(|t| t + SETTLE),
            active: self.active_profile,
        }
    }

    /// One step of the automatic profiles (every tick): the engine decides
    /// with the gates; a switch it asks for is made at once.
    pub(crate) fn auto_tick(&mut self) {
        let now = self.now();
        let gates = self.auto_gates();
        let target = {
            let mut auto = self.shared.auto();
            auto.engine.observe_active(now, self.active_profile);
            auto.engine.poll(now, &gates)
        };
        if let Some(target) = target {
            self.auto_switch(&target);
        }
        self.emit_auto_events();
    }

    /// Records and sends what the engine reported.
    pub(crate) fn emit_auto_events(&self) {
        let events = self.shared.auto().engine.take_events();
        if events.is_empty() {
            return;
        }
        let at = openwraith_hid::rfc3339_utc(self.config.now());
        crate::auto::emit(&self.shared, &self.config.state_dir, events, &at);
    }

    /// The user switched profiles (the keyboard, the header, the tray): the
    /// engine's episodes are overridden.
    pub(crate) fn auto_manual_switch(&self, profile: u8) {
        let now = self.now();
        self.shared.auto().engine.manual_switch(now, profile);
        self.emit_auto_events();
    }

    /// Makes the switch the engine asked for and tells it how it went.
    pub(crate) fn auto_switch(&mut self, target: &Target) {
        let from = self.active_profile;
        if self
            .shared
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            let now = self.now();
            self.shared
                .auto()
                .engine
                .switch_not_started(now, AutoWaitReason::Busy);
            return;
        }
        self.owns_busy = true;
        let result = self.run_op(
            OpKind::AutoSwitch,
            Arc::new(crate::sink::NoProgress),
            CancelPolicy::Never,
            |w, progress| w.switch_for_rule(target, progress),
        );
        self.release_own_busy();
        let now = self.now();
        {
            let mut auto = self.shared.auto();
            match result {
                Ok(Switch::Done(profile)) => auto.engine.switch_done(now, Ok(profile), from),
                Ok(Switch::NotStarted(reason)) => auto.engine.switch_not_started(now, reason),
                Err(error) => auto.engine.switch_done(now, Err(error.message), from),
            }
        }
        self.publish();
        self.exit_if_requested();
    }

    /// The switch itself: the 2 s gate and the device lock, the library's
    /// profile switcher (standing rule), then a read of the new profile.
    fn switch_for_rule(
        &mut self,
        target: &Target,
        progress: &mut OpProgress,
    ) -> Result<Switch, AppError> {
        let Some(profile) = ProfileId::new(target.profile) else {
            return Err(AppError::new(
                AppErrorKind::InvalidRequest,
                format!("profile {} does not exist (1-4)", target.profile),
            ));
        };
        if let Some(reason) = self.auto_block_for_switch() {
            return Ok(Switch::NotStarted(reason));
        }
        if let Err(error) = self.ensure_quiet(progress) {
            // Nothing is sent before the gate is over: every refusal here
            // makes the switch wait (a reset or an unplug is no failed
            // switch).
            let reason = if self.paused_for_cli() {
                AutoWaitReason::PausedForCli
            } else {
                match error.kind {
                    AppErrorKind::OtherProgramActive => AutoWaitReason::OtherProgram,
                    AppErrorKind::StalePlan => AutoWaitReason::ProfileChanging,
                    AppErrorKind::KeyboardReset => AutoWaitReason::ResetSeen,
                    _ => AutoWaitReason::NotConnected,
                }
            };
            return Ok(Switch::NotStarted(reason));
        }
        let Some(session) = self.session.as_mut() else {
            return Ok(Switch::NotStarted(AutoWaitReason::NotConnected));
        };
        let (outcome, known) = {
            let mut switcher = match session.profile_switcher() {
                Ok(switcher) => switcher,
                Err(error) => {
                    return match not_started(&error) {
                        Some(reason) => Ok(Switch::NotStarted(reason)),
                        None => Err(AppError::from(&error)),
                    };
                }
            };
            if switcher.consent() != consent::standing_rule() {
                return Err(AppError::internal(
                    "the profile switcher does not switch with the standing rule's consent",
                ));
            }
            let outcome = switcher.switch_to(profile, &mut |_| {});
            let known = switcher.session().last_known_profile();
            (outcome, known)
        };
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                self.active_profile = known.or(self.active_profile);
                self.reread = known.is_some() && self.conn == Conn::Ready;
                return match not_started(&error) {
                    Some(reason) => Ok(Switch::NotStarted(reason)),
                    None => Err(AppError::from(&error)),
                };
            }
        };
        let now = outcome.profile.get();
        self.active_profile = Some(now);
        if self.conn != Conn::Ready || self.foreign_heard() {
            // Another program was heard (or the gate was re-armed for the
            // automatic switches only): no read; the snapshot describes the
            // old profile and is dropped.
            self.clear_snapshot();
            self.active_profile = Some(now);
            self.stale = true;
        } else {
            match self.full_read(ReadVia::Op(progress, ReadPurpose::Snapshot)) {
                Ok(backup) => {
                    self.set_snapshot(&backup);
                }
                Err(ReadFail::Cancelled) => self.reread = true,
                Err(fail) => self.read_failed(&fail),
            }
        }
        Ok(Switch::Done(now))
    }

    /// The gates again, right before the switch (the operation's own busy
    /// flag and op aside).
    fn auto_block_for_switch(&self) -> Option<AutoWaitReason> {
        match self.auto_block() {
            Some(AutoWaitReason::Busy) => {
                // The switch's own operation holds the busy flag; another
                // request admitted meanwhile cannot run before it ends.
                None
            }
            other => other,
        }
    }
}
