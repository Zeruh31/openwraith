//! Reading the settings (connect, refresh, after operations) and the
//! connection requests. Reads only: the device-facing function is generic
//! over `Transport`, so it cannot reach a writer.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use openwraith_hid::{
    Backup, BackupError, BackupOptions, ForeignActivity, Progress, Session, Transport,
    read_all_observed,
};

use super::progress::{CancelPolicy, OpProgress};
use crate::backend::DeviceBackend;
use crate::dto::{
    AppError, AppErrorKind, DeviceId, DeviceSnapshot, ErrorDetail, ForeignActivityView, Notice,
    OpInfo, OpKind, PlanInvalidation, ReadPurpose, WriteBlockReason,
};
use crate::map::{foreign, sat16, snapshot_from_backup};
use crate::shared::Shared;
use crate::sink::ProgressSink;
use crate::worker::{Conn, Worker};

/// Why a full read failed.
#[derive(Debug)]
pub(crate) enum ReadFail {
    /// The keyboard is not a supported model.
    Unsupported(Option<String>),
    /// The keyboard reset itself.
    Reset,
    /// The connection was lost.
    Lost(AppError),
    /// The user stopped it.
    Cancelled,
    /// Another program was heard during the read (status reports or
    /// replies to its commands): nothing more may be sent.
    OtherProgram(ForeignActivityView),
    /// Anything else.
    Other(AppError),
}

impl ReadFail {
    /// As an error for a reply.
    pub(crate) fn into_error(self) -> AppError {
        match self {
            ReadFail::Unsupported(product) => AppError::with(
                AppErrorKind::ModelMismatch,
                ErrorDetail::UnsupportedModel { product },
                "the keyboard is not a supported model",
            ),
            ReadFail::Reset => AppError::new(
                AppErrorKind::KeyboardReset,
                "the keyboard reported a reset; nothing more was sent",
            ),
            ReadFail::Lost(error) | ReadFail::Other(error) => error,
            ReadFail::Cancelled => AppError::new(AppErrorKind::Cancelled, "cancelled"),
            ReadFail::OtherProgram(evidence) => AppError::with(
                AppErrorKind::OtherProgramActive,
                ErrorDetail::OtherProgram { evidence },
                "another program is using the keyboard; OpenWraith stopped reading and only \
                 listens until it is closed",
            ),
        }
    }
}

/// Whether `activity` shows another program: status reports or replies to
/// its commands (profile reports alone are a switch on the keyboard).
pub(crate) fn is_other_program(activity: &ForeignActivity) -> bool {
    activity.status_reports > 0 || activity.foreign_acks > 0
}

/// Before one more request of the worker's own (the view refresh after a
/// write): classifies what arrived meanwhile, sending nothing, and tells
/// whether the request may go out: not after a reset or a lost connection,
/// and not once another program was heard since the gate mark (D5).
pub(crate) fn may_send<T: Transport>(session: &mut Session<T>) -> bool {
    session.listen(Duration::ZERO).is_ok()
        && !session.keyboard_reset_seen()
        && !session.connection_lost()
        && !is_other_program(&session.activity_since_gate())
}

/// Reads every setting of the active profile (read-only). It stops before
/// its next request as soon as another program is heard (D5): what arrived
/// is classified before every request, and nothing more is sent then.
pub(crate) fn read_profile<T: Transport>(
    session: &mut Session<T>,
    options: &BackupOptions,
    progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<Backup, BackupError> {
    read_all_observed(session, options, progress, |activity| {
        if is_other_program(activity) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })
}

/// Where the progress of a read goes.
pub(crate) enum ReadVia<'a> {
    /// The connection state (`Reading { done, total }`).
    Status,
    /// An operation's progress.
    Op(&'a mut OpProgress, ReadPurpose),
}

/// Publishes the reading progress in the status.
fn status_progress(
    shared: &Shared,
    worker_status: &mut crate::dto::DeviceStatus,
    p: Progress,
    now: std::time::Instant,
) {
    worker_status.connection = crate::dto::ConnectionState::Reading {
        done: sat16(p.done),
        total: sat16(p.total),
    };
    shared.publish(worker_status.clone(), now);
}

impl<B: DeviceBackend> Worker<B> {
    /// Reads every setting of the active profile; a profile change during
    /// the read is retried twice (after re-arming the gate), but only when
    /// profile reports were the only activity heard since the gate mark.
    /// Status reports or replies to another program's commands end the read
    /// with [`ReadFail::OtherProgram`] before its next request (nothing is
    /// sent at all when they were heard before it started): the gate is
    /// never re-armed over evidence of another program (D5/D6).
    pub(crate) fn full_read(&mut self, mut via: ReadVia<'_>) -> Result<Backup, ReadFail> {
        let options = self.config.backup_options(self.sim);
        let shared = Arc::clone(&self.shared);
        let clock = Arc::clone(&self.clock);
        let mut status = self.status();
        for attempt in 0..3 {
            let session = self
                .session
                .as_mut()
                .ok_or_else(|| ReadFail::Lost(Self::not_connected()))?;
            let result = match &mut via {
                ReadVia::Status => read_profile(session, &options, |p| {
                    status_progress(&shared, &mut status, p, clock.now());
                    ControlFlow::Continue(())
                }),
                ReadVia::Op(progress, purpose) => {
                    let purpose = *purpose;
                    read_profile(session, &options, |p| progress.read(purpose, p))
                }
            };
            let error = match result {
                Ok(backup) => return Ok(backup),
                Err(error) => error,
            };
            let activity = session.activity_since_gate();
            // A read stopped by `read_profile` because another program was
            // heard ends here too (`Cancelled` with the evidence).
            let foreign_heard = is_other_program(&activity);
            match error {
                BackupError::ProfileChanged { .. }
                    if attempt < 2
                        && !foreign_heard
                        && !session.keyboard_reset_seen()
                        && !session.connection_lost() =>
                {
                    // Profile reports only (Fn + 1..4 on the keyboard).
                    session.rearm_gate();
                }
                error => {
                    return Err(if session.keyboard_reset_seen() {
                        ReadFail::Reset
                    } else if session.connection_lost() {
                        ReadFail::Lost(AppError::from(&error))
                    } else if foreign_heard {
                        ReadFail::OtherProgram(foreign(activity))
                    } else {
                        match error {
                            BackupError::UnsupportedModel { product, .. } => {
                                ReadFail::Unsupported(product.as_deref().map(crate::map::clean))
                            }
                            BackupError::Cancelled => ReadFail::Cancelled,
                            other => ReadFail::Other(AppError::from(&other)),
                        }
                    });
                }
            }
        }
        Err(ReadFail::Other(AppError::new(
            AppErrorKind::ProfileChanged,
            "the active profile kept changing while the settings were read",
        )))
    }

    /// Makes `backup` (a fresh read of the active profile) the snapshot.
    pub(crate) fn set_snapshot(&mut self, backup: &Backup) -> DeviceSnapshot {
        self.snapshot_revision = self.snapshot_revision.wrapping_add(1);
        let snapshot = snapshot_from_backup(backup, self.snapshot_revision, self.generation);
        self.active_profile = backup.active_profile;
        self.stale = false;
        self.snapshot = Some(snapshot.clone());
        self.shared.set_snapshot(Some(snapshot.clone()));
        snapshot
    }

    /// Replaces the snapshot after changing it in place.
    pub(crate) fn update_snapshot(&mut self, mut snapshot: DeviceSnapshot) {
        self.snapshot_revision = self.snapshot_revision.wrapping_add(1);
        snapshot.revision = self.snapshot_revision;
        snapshot.read_at = openwraith_hid::rfc3339_utc(self.config.now());
        self.active_profile = Some(snapshot.profile);
        self.snapshot = Some(snapshot.clone());
        self.shared.set_snapshot(Some(snapshot));
    }

    /// Acts on a failed read: the connection state follows the cause.
    pub(crate) fn read_failed(&mut self, fail: &ReadFail) {
        match fail {
            ReadFail::Unsupported(product) => self.enter_unsupported(product.clone()),
            ReadFail::Reset => self.enter_reset_seen(),
            ReadFail::Lost(error) => self.enter_lost(error.clone()),
            ReadFail::Cancelled => {}
            ReadFail::OtherProgram(evidence) => self.enter_other_program(*evidence),
            ReadFail::Other(error) => {
                self.clear_snapshot();
                self.conn = Conn::ReadFailed {
                    error: error.clone(),
                };
            }
        }
    }

    /// Reads the settings after the observation (or to refresh them after a
    /// profile change): the state shows `Reading`, then `Ready`.
    pub(crate) fn connect_read(&mut self, kind: OpKind) {
        if self.foreign_heard() {
            // Another program was heard since the gate mark: nothing is
            // sent (D5); the state becomes `OtherProgram`.
            self.after_op_state();
            self.publish();
            return;
        }
        if self
            .shared
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            // A request is waiting; it goes first, the read follows.
            self.reread = self.conn == Conn::Ready;
            return;
        }
        self.owns_busy = true;
        let op = self.shared.mint().op();
        self.current = Some(OpInfo {
            op,
            kind,
            last: None,
        });
        // Published with the first progress (`done` of `total`).
        self.conn = Conn::Reading { done: 0, total: 0 };
        match self.full_read(ReadVia::Status) {
            Ok(backup) => {
                self.set_snapshot(&backup);
                self.conn = Conn::Ready;
                if std::mem::take(&mut self.notify_connected) {
                    self.shared.notice(Notice::Connected);
                }
                self.after_op_state();
            }
            Err(fail) => self.read_failed(&fail),
        }
        self.end_op();
        self.release_own_busy();
        self.publish();
        // "Stop safely and close" may have arrived during the read.
        self.exit_if_requested();
    }

    /// `refresh`: reads the settings again.
    pub(crate) fn refresh(
        &mut self,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<DeviceSnapshot, AppError> {
        self.check_readable()?;
        self.invalidate(PlanInvalidation::Operation);
        self.run_op(
            OpKind::Refresh,
            sink,
            CancelPolicy::Boundaries,
            |w, progress| match w.full_read(ReadVia::Op(progress, ReadPurpose::Snapshot)) {
                Ok(backup) => {
                    let snapshot = w.set_snapshot(&backup);
                    w.conn = Conn::Ready;
                    Ok(snapshot)
                }
                Err(fail) => {
                    w.read_failed(&fail);
                    Err(fail.into_error())
                }
            },
        )
    }

    /// Refuses a connection request after a panic: the worker's state is
    /// not trusted any more, so writes stay blocked until the app restarts
    /// (D13).
    fn refuse_in_fault(&self) -> Result<(), AppError> {
        if matches!(self.conn, Conn::Fault { .. }) {
            Err(super::writes_blocked(vec![WriteBlockReason::Fault]))
        } else {
            Ok(())
        }
    }

    /// `select_device`: opens the chosen keyboard.
    pub(crate) fn select_device(&mut self, id: &DeviceId) -> Result<(), AppError> {
        self.refuse_in_fault()?;
        if !self.listed.iter().any(|e| e.id == *id) {
            self.search(true);
        }
        let entry = self
            .listed
            .iter()
            .find(|e| e.id == *id)
            .cloned()
            .ok_or_else(|| {
                AppError::new(AppErrorKind::NotFound, "that keyboard is not connected")
            })?;
        self.open(entry);
        Ok(())
    }

    /// `reconnect`: closes the keyboard and connects again (a new session,
    /// observed for 2 s first). The only way out of `ResetSeen`.
    pub(crate) fn reconnect(&mut self) -> Result<(), AppError> {
        self.refuse_in_fault()?;
        self.drop_session();
        self.clear_snapshot();
        self.invalidate(PlanInvalidation::Disconnected);
        self.conn = Conn::Searching;
        self.search(true);
        Ok(())
    }

    /// `disconnect`: releases the keyboard until `reconnect`.
    pub(crate) fn disconnect(&mut self) -> Result<(), AppError> {
        self.refuse_in_fault()?;
        self.drop_session();
        self.clear_snapshot();
        self.invalidate(PlanInvalidation::Disconnected);
        self.conn = Conn::Released;
        Ok(())
    }

    /// `recheck_other_program`: the user closed the other program; observe
    /// again (2 s, listen only), then read everything again.
    pub(crate) fn recheck_other_program(&mut self) -> Result<(), AppError> {
        if !matches!(self.conn, Conn::OtherProgram { .. }) {
            return Err(AppError::new(
                AppErrorKind::InvalidRequest,
                "no other program was detected",
            ));
        }
        let session = self.session.as_mut().ok_or_else(Self::not_connected)?;
        session.rearm_gate();
        self.conn = Conn::Observing;
        Ok(())
    }
}
