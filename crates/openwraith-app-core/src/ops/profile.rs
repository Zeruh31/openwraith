//! Profile switches (W1). The only place of the app that creates a writer
//! itself; every other write goes through the library's operations.

use std::sync::Arc;
use std::time::Duration;

use openwraith_hid::protocol::{ActiveProfile, ProfileId, ReadRequest};
use openwraith_hid::{Session, Transport};

use super::consent;
use super::progress::CancelPolicy;
use super::snapshot::{ReadFail, ReadVia};
use crate::backend::DeviceBackend;
use crate::clock::millis;
use crate::dto::{
    AppError, ErrorDetail, IntentField, LastReport, OpKind, PlanInvalidation, PlanToken,
    ProfileSwitchPreview, ProfileSwitchReport, ProgressStep, ReadPurpose,
};
use crate::map::{profile_switch_report, sat32};
use crate::sink::ProgressSink;
use crate::worker::{PendingPlan, PlanKind, Worker, stale};

/// The commit settle the library's writer waits after a write.
const SETTLE: Duration = Duration::from_secs(2);

/// The active profile, read fresh (`21/11`).
pub(crate) fn read_active<T: Transport>(session: &mut Session<T>) -> Result<u8, AppError> {
    session
        .read::<ActiveProfile>(ReadRequest::ActiveProfile)
        .map(|p| p.profile)
        .map_err(|e| AppError::from(&e))
}

/// How long a switch would first wait for the keyboard to save.
pub(crate) fn save_wait<T: Transport>(session: &Session<T>) -> u32 {
    session.last_write_at().map_or(0, |at| {
        let due = at + SETTLE;
        millis(due.saturating_duration_since(session.transport().now()))
    })
}

impl<B: DeviceBackend> Worker<B> {
    /// `prepare_profile_switch`.
    pub(crate) fn prepare_profile_switch(
        &mut self,
        profile: u8,
    ) -> Result<ProfileSwitchPreview, AppError> {
        self.check_writable()?;
        if ProfileId::new(profile).is_none() {
            return Err(super::invalid(
                ErrorDetail::InvalidIntent {
                    field: IntentField::Profiles,
                },
                format!("profile {profile} does not exist (1-4)"),
            ));
        }
        self.run_op(
            OpKind::ProfileSwitch,
            Arc::new(crate::sink::NoProgress),
            CancelPolicy::Never,
            |w, _| {
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let from = read_active(session)?;
                let save_wait_ms = save_wait(session);
                if Some(from) != w.snapshot.as_ref().map(|s| s.profile) {
                    w.reread = true;
                }
                let token = (from != profile).then(|| {
                    w.store_plan(
                        PlanKind::ProfileSwitch,
                        PendingPlan::ProfileSwitch { from, to: profile },
                    )
                });
                Ok(ProfileSwitchPreview {
                    nothing_to_do: token.is_none(),
                    token,
                    from,
                    to: profile,
                    save_wait_ms,
                })
            },
        )
    }

    /// `apply_profile_switch`: W1 behind the commit settle, then a full
    /// read of the new profile. Not cancellable.
    pub(crate) fn apply_profile_switch(
        &mut self,
        token: &PlanToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<ProfileSwitchReport, AppError> {
        let PendingPlan::ProfileSwitch { from, to } =
            self.take_plan(token, PlanKind::ProfileSwitch)?
        else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        let target = ProfileId::new(to).ok_or_else(|| stale(PlanInvalidation::Unknown))?;
        self.check_writable()?;
        self.run_op(
            OpKind::ProfileSwitch,
            sink,
            CancelPolicy::Never,
            |w, progress| {
                w.ensure_quiet(progress)?;
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let (outcome, now) = {
                    let mut writer = session
                        .writer(consent::application())
                        .map_err(|e| AppError::from(&e))?;
                    let outcome = writer.select_profile_observed(target, &mut |wait| {
                        progress.step(ProgressStep::WaitingForKeyboardSave {
                            profile: wait.profile,
                            wait_ms: sat32(wait.wait_ms),
                        });
                    });
                    let now = writer.known_profile().map(ProfileId::get);
                    (outcome, now)
                };
                let outcome = match outcome {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        w.active_profile = now;
                        w.reread = now.is_some();
                        return Err(AppError::from(&error));
                    }
                };
                if w.foreign_heard() {
                    // Another program talks to the keyboard: no read of the
                    // new profile (the state becomes `OtherProgram`). The
                    // snapshot describes the old profile: it is dropped.
                    w.clear_snapshot();
                    w.active_profile = now;
                    w.stale = true;
                } else {
                    match w.full_read(ReadVia::Op(progress, ReadPurpose::Snapshot)) {
                        Ok(backup) => {
                            w.set_snapshot(&backup);
                        }
                        Err(ReadFail::Cancelled) => w.reread = true,
                        Err(fail) => w.read_failed(&fail),
                    }
                }
                let report = profile_switch_report(from, to, &outcome, w.active_profile.or(now));
                w.shared.set_last_report(LastReport::ProfileSwitch {
                    report: report.clone(),
                });
                Ok(report)
            },
        )
    }
}
