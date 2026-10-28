//! Coexistence with the CLI and the vendor site (design B9).
//!
//! * **The device lock** (`openwraith_hid::coordination::DeviceLock`, in
//!   [`CoreConfig::lock_dir`](crate::CoreConfig)): the app takes it
//!   exclusively right before every write (in `ensure_quiet`, which every
//!   write passes) and every automatic switch, and holds it until the
//!   operation has ended and the keyboard had its commit settle. A CLI
//!   command that wants it meanwhile fails at once ("the OpenWraith app is
//!   writing; try again"). The app never holds it while idle.
//! * **`PausedForCli`**: while idle the worker probes the lock about once a
//!   second, and at once when it hears traffic of another program;
//!   held by someone else (the CLI), the app pauses: it listens,
//!   sends nothing (no read, no write, no automatic switch), invalidates the
//!   plan and attributes what it hears to the CLI (no `OtherProgram`). An
//!   `OtherProgram` state entered since the last probe that found the lock
//!   free is the CLI's doing too, and so is what was heard since that
//!   probe (it is not remembered as another program's activity). When the
//!   lock is free again: the gate is re-armed, 2 s of observation, a full
//!   read and with it the known-good comparison (`Observing` ->
//!   `connect_read` -> `full_read`). Left over: a CLI command so short
//!   that it released the lock before the app heard its traffic (one
//!   query) cannot be told from another program: `OtherProgram`, "check
//!   again".
//! * **Automation after another program** (open decision 4): with the tray
//!   on and the automatic profiles on, an `OtherProgram` episode followed by
//!   180 s of complete silence (no report of any kind) re-arms the session's
//!   gate for the automatic switches only; the state stays `OtherProgram`,
//!   so manual writes still need "check again" and the vendor-site
//!   acknowledgement. Anything heard ends it, and so does turning the tray
//!   or the automatic profiles off, or pausing them.

use std::time::{Duration, Instant};

use openwraith_hid::coordination::{LockState, TOOL_APP};
use openwraith_hid::{DeviceLock, LockInfo};

use super::{Conn, Worker};
use crate::backend::DeviceBackend;
use crate::dto::{AppError, Notice, OpKind, PlanInvalidation, WriteBlockReason};

/// How often the lock is probed while idle.
pub(crate) const PROBE_INTERVAL: Duration = Duration::from_secs(1);

/// The silence after which an `OtherProgram` episode re-arms the gate for
/// the automatic switches (tray mode).
pub(crate) const AUTO_REARM_SILENCE: Duration = Duration::from_secs(180);

/// The keyboard's commit settle after a write (the lock is held this long
/// after the last write).
pub(crate) const LOCK_SETTLE: Duration = Duration::from_secs(2);

/// The app is paused because the CLI holds the lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliPause {
    /// Since when.
    pub(crate) since: Instant,
    /// Since when what was heard is the CLI's (the last probe that found
    /// the lock free, when the CLI's traffic may have come before the
    /// pause).
    pub(crate) from: Instant,
    /// What to do when it ends: observe and read again (the state was
    /// connected, or `OtherProgram` caused by the CLI), or nothing (the
    /// state stays, such as `OtherProgram` from before or `ResetSeen`).
    pub(crate) observe_after: bool,
    /// The lock was seen held exclusively (the CLI wrote).
    pub(crate) exclusive: bool,
}

/// The lock's purpose for an operation (`device.lock.json`).
pub(crate) fn purpose(kind: OpKind) -> &'static str {
    match kind {
        OpKind::Refresh | OpKind::Connect | OpKind::Recheck => "read",
        OpKind::ProfileSwitch => "profile switch",
        OpKind::TriggerEdit => "trigger edit",
        OpKind::LightEdit => "light edit",
        OpKind::BackupActive => "backup",
        OpKind::BackupAll => "backup of all profiles",
        OpKind::RestorePrepare => "restore comparison",
        OpKind::Restore => "restore",
        OpKind::KeymapEdit => "keymap edit",
        OpKind::ColorEdit => "colour edit",
        OpKind::DeadBandEdit => "dead-band edit",
        OpKind::SocdEdit => "SOCD edit",
        OpKind::DepthView => "key-depth view",
        OpKind::TravelClose => "key-depth test close",
        OpKind::Export => "profile export",
        OpKind::ImportPrepare => "import comparison",
        OpKind::Import => "import",
        OpKind::RecoveryCheck => "recovery comparison",
        OpKind::AutoSwitch => "automatic profile switch",
    }
}

impl<B: DeviceBackend> Worker<B> {
    /// Whether the app is paused for the CLI.
    pub(crate) fn paused_for_cli(&self) -> bool {
        self.cli_pause.is_some()
    }

    /// The `writes_blocked` error of the pause.
    pub(crate) fn paused_error() -> AppError {
        crate::ops::writes_blocked(vec![WriteBlockReason::PausedForCli])
    }

    /// Takes the device lock exclusively for the running write (kept until
    /// the operation and the commit settle are over). The lock held by
    /// someone else pauses the app and refuses the write before anything is
    /// sent; a lock that cannot be used at all is ignored (as the CLI does:
    /// the "other program" gate still protects the write).
    pub(crate) fn take_device_lock(&mut self) -> Result<(), AppError> {
        if self.cli_pause.is_some() {
            return Err(Self::paused_error());
        }
        let Some(dir) = self.config.lock_dir.clone() else {
            return Ok(());
        };
        if self.device_lock.is_some() {
            return Ok(());
        }
        let purpose = self.current.as_ref().map_or("write", |op| purpose(op.kind));
        match DeviceLock::try_exclusive(&dir, TOOL_APP, purpose) {
            Ok(lock) => {
                self.device_lock = Some(lock);
                self.lock_until = None;
                Ok(())
            }
            Err(error) if error.is_busy() => {
                let holder = match &error {
                    openwraith_hid::LockError::Busy { holder, .. } => holder.clone(),
                    _ => None,
                };
                self.enter_paused_for_cli(holder, false);
                Err(Self::paused_error())
            }
            Err(_) => Ok(()),
        }
    }

    /// After an operation: the lock is released once the keyboard had its
    /// commit settle after the last write.
    pub(crate) fn end_device_lock(&mut self) {
        if self.device_lock.is_none() {
            return;
        }
        let now = self.now();
        let until = self
            .session
            .as_ref()
            .and_then(openwraith_hid::Session::last_write_at)
            .map(|t| t + LOCK_SETTLE)
            .filter(|t| *t > now);
        match until {
            Some(t) => self.lock_until = Some(t),
            None => {
                self.device_lock = None;
                self.lock_until = None;
            }
        }
    }

    /// Releases a lock kept for the commit settle when it is over (ticks).
    fn release_settled_lock(&mut self) {
        if self.current.is_some() {
            return;
        }
        let now = self.now();
        if self.device_lock.is_some() && self.lock_until.is_none_or(|t| now >= t) {
            self.device_lock = None;
            self.lock_until = None;
        }
    }

    /// The pause starts: listen only, plan invalidated, the frontend told.
    pub(crate) fn enter_paused_for_cli(&mut self, holder: Option<LockInfo>, exclusive: bool) {
        let exclusive = exclusive
            || holder
                .as_ref()
                .is_some_and(|h| h.is_cli() && h.mode == openwraith_hid::LockMode::Exclusive);
        if let Some(pause) = &mut self.cli_pause {
            pause.exclusive |= exclusive;
            return;
        }
        let now = self.now();
        let observe_after = match &self.conn {
            Conn::OtherProgram { .. } => self
                .other_since
                .is_some_and(|t| self.last_free_probe.is_none_or(|free| t >= free)),
            Conn::Ready | Conn::Observing | Conn::Reading { .. } | Conn::ReadFailed { .. } => true,
            _ => false,
        };
        // What the CLI made heard before this pause began (since the last
        // probe that found the lock free) is the CLI's, not another
        // program's.
        let from = if observe_after {
            self.foreign_seen_at = self.foreign_seen_at_free;
            self.last_free_probe.unwrap_or(now).min(now)
        } else {
            now
        };
        self.cli_pause = Some(CliPause {
            since: now,
            from,
            observe_after,
            exclusive,
        });
        self.invalidate(PlanInvalidation::OtherProgram);
        let purpose = holder
            .filter(LockInfo::is_cli)
            .map(|h| crate::map::clean(&h.purpose))
            .filter(|p| !p.is_empty());
        self.shared.notice(Notice::PausedForCli { purpose });
    }

    /// The pause ends: re-arm, observe 2 s, read everything again (and
    /// compare it with the automatic backups).
    pub(crate) fn leave_paused_for_cli(&mut self) {
        let Some(pause) = self.cli_pause.take() else {
            return;
        };
        self.cli_window = Some((pause.from, self.now()));
        if !pause.observe_after {
            return;
        }
        if pause.exclusive && self.session.is_some() {
            // The CLI wrote on this connection: the next comparison takes
            // its factory forms as the user's own change.
            self.cli_wrote_on = Some(self.generation);
        }
        if let Some(session) = self.session.as_mut() {
            session.rearm_gate();
            self.profile_seen_at = None;
            self.reread = false;
            self.conn = Conn::Observing;
        }
    }

    /// Probes the lock (about once a second, only while the app holds none
    /// and nothing runs): held by someone else -> paused; free again -> the
    /// pause ends.
    pub(crate) fn probe_cli_lock(&mut self) {
        self.release_settled_lock();
        let Some(dir) = self.config.lock_dir.clone() else {
            return;
        };
        if self.session.is_none() {
            // Without a keyboard there is nothing to pause; a pause ends.
            self.cli_pause = None;
            return;
        }
        if self.device_lock.is_some() || self.current.is_some() {
            return;
        }
        let now = self.now();
        if self.next_probe.is_some_and(|t| now < t) {
            return;
        }
        self.next_probe = Some(now + PROBE_INTERVAL);
        match DeviceLock::probe(&dir) {
            Ok(LockState::Free) => {
                self.last_free_probe = Some(now);
                if self.cli_pause.is_none() {
                    self.foreign_seen_at_free = self.foreign_seen_at;
                }
                self.leave_paused_for_cli();
            }
            Ok(state @ (LockState::Shared | LockState::Exclusive)) => {
                self.enter_paused_for_cli(DeviceLock::holder(&dir), state == LockState::Exclusive);
            }
            Err(_) => {}
        }
    }

    /// Traffic of another program was heard: probes the lock at once (not
    /// waiting for the next probe); held by someone else, the app pauses
    /// for the CLI (true) and the traffic is the CLI's.
    pub(crate) fn cli_holds_lock_now(&mut self) -> bool {
        let Some(dir) = self.config.lock_dir.clone() else {
            return false;
        };
        if self.cli_pause.is_some() {
            return true;
        }
        if self.device_lock.is_some() || self.current.is_some() {
            return false;
        }
        match DeviceLock::probe(&dir) {
            Ok(state @ (LockState::Shared | LockState::Exclusive)) => {
                let now = self.now();
                self.next_probe = Some(now + PROBE_INTERVAL);
                self.enter_paused_for_cli(DeviceLock::holder(&dir), state == LockState::Exclusive);
                true
            }
            Ok(LockState::Free) | Err(_) => false,
        }
    }

    /// Whether `at` lies in the last pause for the CLI.
    pub(crate) fn in_cli_window(&self, at: Instant) -> bool {
        self.cli_window
            .is_some_and(|(from, to)| at >= from && at <= to)
    }

    /// One listening tick while paused: resets and unplugs still count;
    /// everything else heard is the CLI's.
    pub(crate) fn paused_listen_tick(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        match session.listen(super::IDLE_SLICE) {
            Ok(_) => {}
            Err(openwraith_hid::ListenError::KeyboardReset { .. }) => {
                self.cli_pause = None;
                self.enter_reset_seen();
                return;
            }
            Err(error @ openwraith_hid::ListenError::Transport(_)) => {
                self.cli_pause = None;
                self.enter_lost(AppError::from(&error));
                return;
            }
        }
        self.check_still_listed();
    }

    /// Remembers when another program was heard last (over sessions; not
    /// what was heard while the CLI held the lock), and, in `OtherProgram`,
    /// applies the 180 s rule for the automatic switches.
    pub(crate) fn watch_foreign(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let cli_window = self.cli_window;
        let heard = session
            .last_foreign_at()
            .filter(|at| !cli_window.is_some_and(|(from, to)| *at >= from && *at <= to))
            .filter(|_| self.cli_pause.is_none());
        if let Some(at) = heard {
            self.foreign_seen_at = Some(self.foreign_seen_at.map_or(at, |t| t.max(at)));
        }
        if self
            .foreign_seen_at
            .is_some_and(|at| self.in_cli_window(at))
        {
            // The CLI's traffic: what was known before it stays.
            self.foreign_seen_at = self
                .foreign_seen_at_free
                .filter(|at| !self.in_cli_window(*at));
        }
        if !matches!(self.conn, Conn::OtherProgram { .. }) {
            self.auto_rearmed = false;
            self.other_quiet_since = None;
            return;
        }
        let allowed = {
            let auto = self.shared.auto();
            auto.background.tray_enabled
                && auto.engine.config().enabled
                && auto.engine.paused().is_none()
        };
        if self.auto_rearmed && !allowed {
            // The tray (or the automatic profiles) went off: the re-arm for
            // the automatic switches ends with it.
            self.auto_rearmed = false;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let now = openwraith_hid::Transport::now(session.transport());
        let total = session.foreign_activity();
        let signature = total
            .status_reports
            .saturating_add(total.foreign_acks)
            .saturating_add(total.profile_reports)
            .saturating_add(session.events().len() as u64)
            .saturating_add(session.stats().stray_travel_reports);
        if self.other_quiet_since.is_none() || signature != self.other_signature {
            self.other_signature = signature;
            self.other_quiet_since = Some(now);
            self.auto_rearmed = false;
            return;
        }
        let silent = self
            .other_quiet_since
            .is_some_and(|t| now.saturating_duration_since(t) >= AUTO_REARM_SILENCE);
        if silent && !self.auto_rearmed && allowed {
            // For the automatic switches only: the state stays
            // `OtherProgram` (manual writes need "check again").
            session.rearm_gate();
            self.auto_rearmed = true;
            self.other_reports_handled = 0;
        }
    }
}
