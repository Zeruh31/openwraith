//! Dead-band edits (W9): top and bottom dead zones of keys of the active
//! profile (each profile has its own), in 0.02 mm units, through the
//! library's `plan_dead_band_edit` / `apply_dead_band_edit`.

use std::sync::Arc;

use openwraith_hid::protocol::{DeadBandTable, Distance, ReadRequest};
use openwraith_hid::{
    DeadBandEdit, DeadBandEditPlan, EditOptions, PersistenceCheckMode, WriteOptions,
    apply_dead_band_edit, plan_dead_band_edit,
};

use super::consent;
use super::editors::{TableRead, checked_cache, distinct_keys, persistence_mode};
use super::progress::CancelPolicy;
use super::snapshot::{ReadFail, ReadVia};
use crate::backend::DeviceBackend;
use crate::dto::{
    AppError, DeadBandChangeView, DeadBandEditIntent, DeadBandEditPreview, DeadBandHintKind,
    DeadBandHintView, DeadBandValue, EditFieldView, EditProblemView, EditReportView, ErrorDetail,
    LastReport, OpKind, PlanInvalidation, PlanToken, ProgressStep, ReadPurpose,
};
use crate::map::editors::{
    dead_band_max, edit_error, persistence_plan, raw_entry, refused, warning_view, write_view,
};
use crate::map::{edit_report_view, key_ref, sat16};
use crate::sink::ProgressSink;
use crate::worker::{PendingPlan, PlanKind, Worker, stale};

/// A confirmed dead-band plan waiting for its apply step.
pub(crate) struct DeadBandPending {
    /// The library's plan.
    pub(crate) plan: DeadBandEditPlan,
    /// The persistence check.
    pub(crate) mode: PersistenceCheckMode,
}

fn value(pair: [u8; 2]) -> DeadBandValue {
    DeadBandValue {
        top: u16::from(pair[0]),
        bottom: u16::from(pair[1]),
    }
}

fn in_range(field: EditFieldView, value: Option<u16>, max: u16) -> Result<(), AppError> {
    match value {
        Some(v) if v > max => Err(refused(
            EditProblemView::OutOfRange {
                field,
                value: v,
                min: 0,
                max,
            },
            format!("{field:?} {v} is outside 0..={max}"),
        )),
        _ => Ok(()),
    }
}

impl<B: DeviceBackend> Worker<B> {
    /// `prepare_dead_band_edit`: reads the dead band (read-only) and stores
    /// the plan.
    pub(crate) fn prepare_dead_band_edit(
        &mut self,
        intent: &DeadBandEditIntent,
    ) -> Result<DeadBandEditPreview, AppError> {
        self.check_writable()?;
        let cache = checked_cache(self, intent.based_on, |c| c.dead_band.revision)?;
        let model = cache.model.ok_or_else(|| {
            refused(
                EditProblemView::NotAvailable,
                "the keyboard is not a supported model",
            )
        })?;
        let unit = cache.unit_um;
        if unit != Some(Distance::MICROMETRES_PER_UNIT) {
            return Err(AppError::with(
                crate::dto::AppErrorKind::InvalidRequest,
                ErrorDetail::DistanceUnit { unit_um: unit },
                "the keyboard reports another distance unit than 0.02 mm; nothing was written",
            ));
        }
        if !matches!(cache.dead_band.data, TableRead::Ok(_)) {
            return Err(refused(
                EditProblemView::NotAvailable,
                "the dead band was not read or cannot be decoded",
            ));
        }
        let keys = distinct_keys(model, intent.keys.iter().copied())?;
        if intent.top.is_none() && intent.bottom.is_none() {
            return Err(refused(EditProblemView::NoChanges, "nothing to change"));
        }
        let max = dead_band_max(model, Distance::MICROMETRES_PER_UNIT);
        in_range(EditFieldView::Top, intent.top, max)?;
        in_range(EditFieldView::Bottom, intent.bottom, max)?;
        let edit = DeadBandEdit {
            keys,
            top: intent.top.map(Distance::from_units),
            bottom: intent.bottom.map(Distance::from_units),
        };
        let mode = persistence_mode(intent.check_persistence);
        let shown = self.snapshot.as_ref().map(|s| s.profile);
        let triggers = self.snapshot.as_ref().map(|s| s.triggers.clone());
        self.run_op(
            OpKind::DeadBandEdit,
            Arc::new(crate::sink::NoProgress),
            CancelPolicy::Never,
            |w, _| {
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let plan =
                    plan_dead_band_edit(session, &edit).map_err(|e| edit_error(&e, model))?;
                if Some(plan.profile().get()) != shown {
                    w.reread = true;
                    w.invalidate(PlanInvalidation::ProfileChanged);
                    return Err(stale(PlanInvalidation::ProfileChanged));
                }
                let max_travel = triggers
                    .as_ref()
                    .and_then(|t| t.limits)
                    .map(|l| l.max_actuation);
                let mut hints = Vec::new();
                for change in plan.changes() {
                    let actuation = triggers
                        .as_ref()
                        .and_then(|t| t.keys.iter().find(|k| k.key.index == change.pos.index()))
                        .and_then(|k| k.value)
                        .map(|v| v.actuation);
                    let Some(actuation) = actuation else {
                        continue;
                    };
                    let after = value(change.after);
                    if after.top > 0 && after.top >= actuation {
                        hints.push(DeadBandHintView {
                            key: key_ref(change.pos),
                            kind: DeadBandHintKind::TopReachesActuation,
                            actuation,
                        });
                    }
                    if let Some(max_travel) = max_travel
                        && after.bottom > 0
                        && after.bottom > max_travel.saturating_sub(actuation)
                    {
                        hints.push(DeadBandHintView {
                            key: key_ref(change.pos),
                            kind: DeadBandHintKind::BottomReachesActuation,
                            actuation,
                        });
                    }
                }
                let preview = DeadBandEditPreview {
                    token: None,
                    nothing_to_do: !plan.has_changes(),
                    profile: plan.profile().get(),
                    changes: plan
                        .changes()
                        .iter()
                        .map(|c| DeadBandChangeView {
                            key: key_ref(c.pos),
                            before: value(c.before),
                            after: value(c.after),
                        })
                        .collect(),
                    hints,
                    writes: plan.writes().iter().map(write_view).collect(),
                    packets: sat16(plan.writes().iter().map(|w| w.packets).sum()),
                    warnings: plan.warnings().iter().map(warning_view).collect(),
                    persistence: persistence_plan(
                        plan.writes(),
                        mode,
                        intent.check_persistence,
                        plan.profile(),
                    ),
                };
                let token = plan.has_changes().then(|| {
                    w.store_plan(
                        PlanKind::DeadBandEdit,
                        PendingPlan::DeadBandEdit(Box::new(DeadBandPending { plan, mode })),
                    )
                });
                Ok(DeadBandEditPreview { token, ..preview })
            },
        )
    }

    /// `apply_dead_band_edit`: the confirmed plan, after the drift check and
    /// a verified safety backup.
    pub(crate) fn apply_dead_band_edit(
        &mut self,
        token: &PlanToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        let PendingPlan::DeadBandEdit(pending) = self.take_plan(token, PlanKind::DeadBandEdit)?
        else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        self.check_writable()?;
        self.run_op(
            OpKind::DeadBandEdit,
            sink,
            CancelPolicy::Boundaries,
            |w, progress| {
                w.ensure_quiet(progress)?;
                let fresh = w
                    .full_read(ReadVia::Op(progress, ReadPurpose::SafetyBackup))
                    .map_err(ReadFail::into_error)?;
                let plan = &pending.plan;
                let profile = plan.profile().get();
                if fresh.active_profile != Some(profile) {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ProfileChanged));
                }
                let table = fresh
                    .primary()
                    .get::<DeadBandTable>(ReadRequest::DeadBand)
                    .and_then(Result::ok);
                let kept = plan.changes().iter().all(|c| {
                    table
                        .as_ref()
                        .is_some_and(|t| raw_entry(t, c.pos.index()) == c.before)
                });
                if !kept {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ValuesChanged));
                }
                let (saved, file) = w.save_safety(&fresh, "set-deadband", progress)?;
                w.write_journal(OpKind::DeadBandEdit, vec![profile], saved.path())?;
                progress.items = sat16(plan.writes().len());
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let options = EditOptions {
                    write: WriteOptions::default(),
                    persistence: pending.mode,
                };
                let report = apply_dead_band_edit(
                    session,
                    plan,
                    &saved,
                    consent::application(),
                    &options,
                    |p| progress.write(p),
                );
                w.remove_journal();
                let clean = super::recovery::completed_cleanly(
                    report.outcome,
                    report.lost(),
                    report.maybe_lost(),
                );
                w.own_write_done(clean);
                if w.foreign_heard() {
                    w.stale = true;
                } else if report.active_profile_now != Some(profile) {
                    w.reread = true;
                } else {
                    progress.step(ProgressStep::RefreshingView);
                    let reread = w.refresh_dead_band();
                    if clean && let Some(reread) = reread {
                        // What the edit left in the dead band.
                        w.own_write_reread(OpKind::DeadBandEdit, &fresh, reread);
                    }
                }
                let view = edit_report_view(&report, file);
                w.shared.set_last_report(LastReport::Edit {
                    report: view.clone(),
                });
                Ok(view)
            },
        )
    }
}
