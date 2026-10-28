//! The pending plan: at most one, behind an opaque single-use token that
//! expires after 10 minutes and is invalidated by every event that could make
//! it differ from what the user saw.

use std::time::{Duration, Instant};

use openwraith_hid::{LightEditPlan, PreparedRestore, TriggerEditPlan};

use super::{Conn, Worker};
use crate::backend::DeviceBackend;
use crate::dto::{
    AppError, AppErrorKind, ErrorDetail, FileToken, Notice, PlanInvalidation, PlanToken,
};
use crate::ops::colors::ColorPending;
use crate::ops::dead_band::DeadBandPending;
use crate::ops::keymap::KeymapPending;
use crate::ops::socd::SocdPending;

/// How long a plan may wait for the user.
pub(crate) const PLAN_TTL: Duration = Duration::from_secs(600);

/// What kind of plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanKind {
    ProfileSwitch,
    AllProfileBackup,
    TriggerEdit,
    LightEdit,
    Restore,
    KeymapEdit,
    ColorEdit,
    DeadBandEdit,
    SocdEdit,
    Import,
}

/// A plan waiting for confirmation.
pub(crate) enum PendingPlan {
    ProfileSwitch {
        from: u8,
        to: u8,
    },
    AllProfileBackup {
        destination: FileToken,
    },
    TriggerEdit(Box<TriggerEditPlan>),
    LightEdit(Box<LightEditPlan>),
    Restore {
        prepared: Box<PreparedRestore>,
        skipped: usize,
    },
    KeymapEdit(Box<KeymapPending>),
    ColorEdit(Box<ColorPending>),
    DeadBandEdit(Box<DeadBandPending>),
    SocdEdit(Box<SocdPending>),
}

/// The pending plan with its token and binding.
pub(crate) struct Pending {
    pub(crate) token: PlanToken,
    pub(crate) kind: PlanKind,
    pub(crate) generation: u32,
    pub(crate) created: Instant,
    pub(crate) plan: PendingPlan,
}

/// The `stale_plan` error.
pub(crate) fn stale(reason: PlanInvalidation) -> AppError {
    AppError::with(
        AppErrorKind::StalePlan,
        ErrorDetail::PlanInvalidated { reason },
        format!("the plan can no longer be applied ({reason:?}); nothing was written"),
    )
}

impl<B: DeviceBackend> Worker<B> {
    /// Stores a new plan (replacing any other) and returns its token.
    pub(crate) fn store_plan(&mut self, kind: PlanKind, plan: PendingPlan) -> PlanToken {
        self.invalidate(PlanInvalidation::Superseded);
        let token = self.shared.mint().plan();
        self.pending = Some(Pending {
            token: token.clone(),
            kind,
            generation: self.generation,
            created: self.now(),
            plan,
        });
        token
    }

    /// Drops the pending plan, telling the frontend why.
    pub(crate) fn invalidate(&mut self, reason: PlanInvalidation) {
        if self.pending.take().is_some() {
            self.shared.notice(Notice::PlanInvalidated { reason });
        }
    }

    /// Drops the plan of `token` (the user cancelled the dialog).
    pub(crate) fn discard_plan(&mut self, token: &PlanToken) {
        if self.pending.as_ref().is_some_and(|p| p.token == *token) {
            self.pending = None;
        }
    }

    /// Drops an expired plan.
    pub(crate) fn expire_plan(&mut self) {
        let now = self.now();
        if self
            .pending
            .as_ref()
            .is_some_and(|p| now.saturating_duration_since(p.created) >= PLAN_TTL)
        {
            self.invalidate(PlanInvalidation::Expired);
        }
    }

    /// Takes the plan of `token` (single use: it is gone afterwards, also
    /// when a check fails) after checking that it may be applied now.
    pub(crate) fn take_plan(
        &mut self,
        token: &PlanToken,
        kind: PlanKind,
    ) -> Result<PendingPlan, AppError> {
        let now = self.now();
        let Some(pending) = self.pending.take_if(|p| p.token == *token) else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        if pending.kind != kind {
            return Err(stale(PlanInvalidation::Unknown));
        }
        if pending.generation != self.generation {
            return Err(stale(PlanInvalidation::Disconnected));
        }
        if now.saturating_duration_since(pending.created) >= PLAN_TTL {
            return Err(stale(PlanInvalidation::Expired));
        }
        if self.conn != Conn::Ready || self.session.as_ref().is_none_or(|s| s.connection_lost()) {
            return Err(stale(PlanInvalidation::Disconnected));
        }
        Ok(pending.plan)
    }
}
