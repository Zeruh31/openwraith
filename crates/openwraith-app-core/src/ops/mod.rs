//! The operations. Each is a prepare step (reads only; generic over
//! `Transport` where it talks to the keyboard) that stores a plan behind a
//! token, and an apply step that consumes the token and writes through one
//! of the library's entry points (table in `docs/ARCHITECTURE.md`):
//!
//! | UI action | Library calls | Writes |
//! |---|---|---|
//! | connect / refresh / focus check | `read_all_observed`, `Session::read` | none |
//! | trigger edit | `read_summary`, `plan_trigger_edit`; `apply_trigger_edit` | W8, one key per packet |
//! | light edit | `plan_light_edit`; `apply_light_edit` | W3 (+ W2) |
//! | profile switch | `Session::writer` + `Writer::select_profile_observed` | W1 |
//! | backup of the active profile | `read_all_observed`, `Backup::save` | none |
//! | backup of all profiles | `read_all_profiles` | W1 only |
//! | restore | `prepare_restore`; `save_safety`, `execute_restore` | as planned |
//! | keymap / macro / advanced-key edit | `KeymapGroupState::from_snapshot`, `plan_keymap_edit_on`; `apply_keymap_edit` | the keymap group, in the site's order |
//! | key colours | `plan_color_edit`; `apply_color_edit` | W4 (+ W2/W3 to Custom, opt-in) |
//! | dead band | `plan_dead_band_edit`; `apply_dead_band_edit` | W9 |
//! | SOCD | `plan_socd_edit`; `apply_socd_edit` | W11 |
//! | trigger preset | `preset_edit` -> the trigger edit above | W8 |
//! | live key depth | `live_travel` | T1, T2 |
//! | stray close | `Writer::close_stray_travel_test` | T2 |
//! | export | `read_all_observed` or `read_profiles` (switch consent), `Backup::export_profile` | W1 only |
//! | import | `prepare_restore` with `ImportOptions`; `execute_restore` | as planned |
//! | recovery | `assess`, `Backup::compose`, `prepare_restore` (`recovery_check`); the write is `apply_restore` | as planned |
//! | automatic backup | the reads above, reused (`observe_read`) | none |
//! | automatic switch (a game rule) | `Session::profile_switcher` + `ProfileSwitcher::switch_to` (`ops/auto_profile.rs` only), then the read of the new profile | W1 |
//! | "Back up now" (tray) | `read_all_observed` of the active profile, saved as an automatic backup | none |

pub(crate) mod auto_profile;
pub(crate) mod backup;
pub(crate) mod colors;
pub(crate) mod consent;
pub(crate) mod dead_band;
pub(crate) mod editors;
pub(crate) mod journal;
pub(crate) mod keymap;
pub(crate) mod light;
pub(crate) mod presets;
pub(crate) mod profile;
pub(crate) mod progress;
pub(crate) mod recovery;
pub(crate) mod restore;
pub(crate) mod share;
pub(crate) mod snapshot;
pub(crate) mod socd;
pub(crate) mod travel;
pub(crate) mod trigger;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use openwraith_hid::{Backup, SavedBackup, Transport};

use self::journal::{JournalDevice, JournalEntry};
use self::progress::{CancelPolicy, OpProgress};
use crate::backend::DeviceBackend;
use crate::clock::millis;
use crate::dto::{
    AppError, AppErrorKind, ErrorDetail, FileRef, OpInfo, OpKind, PlanInvalidation, ProgressStep,
};
use crate::files::FileKind;
use crate::map::foreign;
use crate::sink::ProgressSink;
use crate::worker::{Conn, Worker};

/// The `writes_blocked` error.
pub(crate) fn writes_blocked(reasons: Vec<crate::dto::WriteBlockReason>) -> AppError {
    AppError::with(
        AppErrorKind::WritesBlocked,
        ErrorDetail::WritesBlocked { reasons },
        "writing is not possible now; nothing was written",
    )
}

/// An `invalid_request` error with detail.
pub(crate) fn invalid(detail: ErrorDetail, message: impl Into<String>) -> AppError {
    AppError::with(AppErrorKind::InvalidRequest, detail, message)
}

impl<B: DeviceBackend> Worker<B> {
    /// Starts an operation: an id, the busy status, the cancel flag.
    pub(crate) fn begin_op(
        &mut self,
        kind: OpKind,
        sink: Arc<dyn ProgressSink>,
        policy: CancelPolicy,
    ) -> OpProgress {
        let op = self.shared.mint().op();
        let cancel = self.shared.start_op(op.clone());
        self.current = Some(OpInfo {
            op: op.clone(),
            kind,
            last: None,
        });
        self.publish();
        OpProgress::new(
            op,
            Arc::clone(&self.clock),
            sink,
            Arc::clone(&self.shared),
            cancel,
            policy,
        )
    }

    /// Ends the running operation.
    pub(crate) fn end_op(&mut self) {
        self.current = None;
        self.shared.end_op();
        if let Some(session) = &self.session {
            self.last_traffic = Some(Transport::now(session.transport()));
        }
    }

    /// Runs `f` as an operation of `kind`: begin, run, update the state
    /// from what the session saw, end.
    pub(crate) fn run_op<T>(
        &mut self,
        kind: OpKind,
        sink: Arc<dyn ProgressSink>,
        policy: CancelPolicy,
        f: impl FnOnce(&mut Self, &mut OpProgress) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut progress = self.begin_op(kind, sink, policy);
        let result = f(self, &mut progress);
        // A write whose end was not reported (it failed on the way) may not
        // have been saved.
        self.own_write_done(false);
        self.after_op_state();
        self.check_socd_pushes();
        self.end_op();
        // The device lock goes once the keyboard had its commit settle.
        self.end_device_lock();
        result
    }

    /// Checks that settings may be written now (the published write gate;
    /// the running operation itself does not count).
    pub(crate) fn check_writable(&self) -> Result<(), AppError> {
        let reasons: Vec<_> = self
            .write_block_reasons()
            .into_iter()
            .filter(|r| *r != crate::dto::WriteBlockReason::Busy)
            .collect();
        if reasons.is_empty() {
            Ok(())
        } else {
            Err(writes_blocked(reasons))
        }
    }

    /// Checks that the keyboard may be read now (connected, no other
    /// program: nothing is sent while another program uses it).
    pub(crate) fn check_readable(&self) -> Result<(), AppError> {
        if self.paused_for_cli() {
            // Nothing is sent while the CLI uses the keyboard.
            return Err(Self::paused_error());
        }
        match &self.conn {
            Conn::Ready | Conn::ReadFailed { .. } if self.session.is_some() => Ok(()),
            Conn::OtherProgram { evidence } => Err(AppError::with(
                AppErrorKind::OtherProgramActive,
                ErrorDetail::OtherProgram {
                    evidence: *evidence,
                },
                "another program is using the keyboard; OpenWraith only listens until it is closed",
            )),
            _ => Err(Self::not_connected()),
        }
    }

    /// Updates the connection state from what the session saw during an
    /// operation: a reset, a lost connection, another program.
    pub(crate) fn after_op_state(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        if session.keyboard_reset_seen() {
            self.cli_pause = None;
            self.enter_reset_seen();
        } else if session.connection_lost() {
            self.cli_pause = None;
            self.enter_lost(AppError::new(
                AppErrorKind::Disconnected,
                "the connection to the keyboard was lost",
            ));
        } else if self.paused_for_cli() {
            // What is heard while the CLI holds the lock is the CLI's.
        } else {
            let activity = session.activity_since_gate();
            if activity.status_reports > 0 || activity.foreign_acks > 0 {
                self.enter_other_program(foreign(activity));
            } else if activity.profile_reports > 0 && self.conn == Conn::Ready {
                self.profile_seen_at
                    .get_or_insert(Transport::now(session.transport()));
            }
        }
    }

    /// Whether another program was heard since the gate mark: status
    /// reports or replies to its commands. After an operation this means no
    /// further request may be sent (no refresh of the view either): the
    /// state becomes `OtherProgram` and the snapshot stale.
    pub(crate) fn foreign_heard(&self) -> bool {
        self.session.as_ref().is_some_and(|session| {
            let activity = session.activity_since_gate();
            activity.status_reports > 0 || activity.foreign_acks > 0
        })
    }

    /// Before a write: listens for the rest of the 2 s observation and
    /// refuses when another program was heard since the gate mark (nothing
    /// saved or written then).
    pub(crate) fn ensure_quiet(&mut self, progress: &mut OpProgress) -> Result<(), AppError> {
        // Every write passes here: the device lock first (the CLI holding
        // it refuses the write before anything is sent).
        self.take_device_lock()?;
        let session = self.session.as_mut().ok_or_else(Self::not_connected)?;
        for _ in 0..64 {
            let observed = session.observed_since_gate();
            if observed >= crate::worker::GATE {
                break;
            }
            let remaining = crate::worker::GATE - observed;
            progress.step(ProgressStep::CheckingOtherPrograms {
                remaining_ms: millis(remaining),
            });
            session.listen(remaining).map_err(|e| AppError::from(&e))?;
        }
        session
            .listen(Duration::ZERO)
            .map_err(|e| AppError::from(&e))?;
        let activity = session.activity_since_gate();
        if activity.status_reports > 0 || activity.foreign_acks > 0 {
            let evidence = foreign(activity);
            self.enter_other_program(evidence);
            return Err(AppError::with(
                AppErrorKind::OtherProgramActive,
                ErrorDetail::OtherProgram { evidence },
                "another program is using the keyboard; nothing was written",
            ));
        }
        if activity.profile_reports > 0 {
            // A profile change on the keyboard: read again, then the user
            // reviews the change against the new profile. Fn + 1..4 is a
            // manual switch for the automatic profiles (an automatic switch
            // waiting for this gate is dropped).
            let to = crate::worker::idle::last_profile_report(session);
            self.reread = true;
            self.profile_changed_on_keyboard_later();
            self.keyboard_switch_heard(to);
            return Err(crate::worker::stale(PlanInvalidation::ProfileChanged));
        }
        Ok(())
    }

    /// Re-arms the gate after a profile change seen during an operation
    /// (the settings are read again at the next tick).
    fn profile_changed_on_keyboard_later(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.rearm_gate();
        }
        self.invalidate(PlanInvalidation::ProfileChanged);
    }

    /// Saves a verified safety backup and registers it.
    pub(crate) fn save_safety(
        &mut self,
        backup: &Backup,
        reason: &str,
        progress: &mut OpProgress,
    ) -> Result<(SavedBackup, FileRef), AppError> {
        progress.step(ProgressStep::SavingSafetyBackup);
        let dir = self.config.safety_dir.clone()?;
        let saved = backup
            .save_safety(&dir, reason)
            .map_err(|e| AppError::from(&e))?;
        let file = self
            .shared
            .register_file(saved.path().to_path_buf(), FileKind::Safety);
        Ok((saved, file))
    }

    /// Writes the journal of a setting write (right before its first write
    /// packet); the tables it writes are OpenWraith's own changes for the
    /// known-good comparison from now on.
    pub(crate) fn write_journal(
        &self,
        kind: OpKind,
        profiles: Vec<u8>,
        safety: &Path,
    ) -> Result<(), AppError> {
        let (tables, restore) = recovery::tables_of(kind);
        self.write_journal_of(kind, profiles, safety, &tables, restore)
    }

    /// [`write_journal`](Self::write_journal) with the tables the write
    /// changes (an import: its chosen parts).
    pub(crate) fn write_journal_of(
        &self,
        kind: OpKind,
        profiles: Vec<u8>,
        safety: &Path,
        tables: &std::collections::BTreeSet<openwraith_hid::KnownGoodTable>,
        restore: bool,
    ) -> Result<(), AppError> {
        let device = self.snapshot.as_ref().map(|s| &s.device);
        self.own_write_started(tables, restore, &profiles);
        journal::write(
            &self.config.journal_path(),
            &JournalEntry {
                op: kind,
                started_at: openwraith_hid::rfc3339_utc(self.config.now()),
                profiles,
                device: JournalDevice {
                    product: device.and_then(|d| d.product.clone()),
                    firmware: device.and_then(|d| d.firmware.clone()),
                },
                safety_backup_path: safety.display().to_string(),
                app_version: self.config.version.clone(),
            },
        )
    }

    /// Removes the journal after a setting write (its report is built).
    pub(crate) fn remove_journal(&self) {
        // A journal that cannot be removed shows as interrupted at the next
        // start; the write itself is reported normally.
        let _ = journal::remove(&self.config.journal_path());
    }
}
