//! SOCD edits (W11): the power (one setting of the whole keyboard) and the
//! pairs of the active profile, always written as the vendor site's whole
//! burst by the library (`plan_socd_edit` / `apply_socd_edit`). Fn + Right
//! Shift switches the power on the keyboard: its push updates the view and
//! invalidates a pending plan made against the old power (`ops::editors`).

use std::collections::HashSet;
use std::sync::Arc;

use openwraith_hid::protocol::{HidUsage, Model, ReadRequest, SocdMode, SocdPair};
use openwraith_hid::{
    EditOptions, PersistenceCheckMode, SocdEdit, SocdEditPlan, SocdState, WriteOptions,
    apply_socd_edit, plan_socd_edit,
};

use super::consent;
use super::editors::{TableRead, checked_cache, layout_key, persistence_mode};
use super::progress::CancelPolicy;
use super::snapshot::{ReadFail, ReadVia};
use crate::backend::DeviceBackend;
use crate::dto::{
    AppError, EditFieldView, EditProblemView, EditReportView, LastReport, OpKind, PlanInvalidation,
    PlanToken, ProgressStep, ReadPurpose, SocdEditIntent, SocdEditPreview, SocdPairIntent,
};
use crate::map::editors::{
    edit_error, persistence_plan, refused, socd_pair_view, warning_view, write_view,
};
use crate::map::{edit_report_view, key_ref_index, sat16};
use crate::sink::ProgressSink;
use crate::worker::{PendingPlan, PlanKind, Worker, stale};

/// A confirmed SOCD plan waiting for its apply step.
pub(crate) struct SocdPending {
    /// The library's plan.
    pub(crate) plan: SocdEditPlan,
    /// The persistence check.
    pub(crate) mode: PersistenceCheckMode,
}

/// The usage of a key of a pair.
fn pair_key(model: &Model, index: u8) -> Result<HidUsage, AppError> {
    layout_key(model, index)?;
    model
        .key(index)
        .and_then(|k| k.hid_usage)
        .and_then(HidUsage::new)
        .ok_or_else(|| {
            refused(
                EditProblemView::NoHidUsage {
                    key: key_ref_index(index),
                },
                format!("{} cannot be in a SOCD pair", model.key_name(index)),
            )
        })
}

/// The pairs of an intent, with the vendor site's rules (the library
/// checks them again).
fn pairs(model: &Model, intents: &[SocdPairIntent]) -> Result<Vec<SocdPair>, AppError> {
    let max = usize::from(model.write_ceilings.socd_max_pairs);
    if intents.len() > max {
        return Err(refused(
            EditProblemView::TooManyKeys { max: sat16(max) },
            format!("at most {max} SOCD pairs"),
        ));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(intents.len());
    for pair in intents {
        let mode = pair
            .mode
            .checked_sub(1)
            .and_then(SocdMode::from_wire)
            .ok_or_else(|| {
                refused(
                    EditProblemView::OutOfRange {
                        field: EditFieldView::SocdMode,
                        value: u16::from(pair.mode),
                        min: 1,
                        max: 4,
                    },
                    format!("SOCD mode {} does not exist", pair.mode),
                )
            })?;
        if pair.key1 == pair.key2 {
            return Err(refused(
                EditProblemView::SocdSameKey {
                    key: key_ref_index(pair.key1),
                },
                "both keys of a SOCD pair are the same",
            ));
        }
        for index in [pair.key1, pair.key2] {
            if !seen.insert(index) {
                return Err(refused(
                    EditProblemView::SocdDuplicateKey {
                        key: key_ref_index(index),
                    },
                    format!("{} is in two SOCD pairs", model.key_name(index)),
                ));
            }
        }
        out.push(SocdPair {
            mode,
            key1: pair_key(model, pair.key1)?,
            key2: pair_key(model, pair.key2)?,
        });
    }
    Ok(out)
}

impl<B: DeviceBackend> Worker<B> {
    /// `prepare_socd_edit`: reads `24 02` and `24 01` (read-only) and stores
    /// the plan.
    pub(crate) fn prepare_socd_edit(
        &mut self,
        intent: &SocdEditIntent,
    ) -> Result<SocdEditPreview, AppError> {
        self.check_writable()?;
        let cache = checked_cache(self, intent.based_on, |c| c.socd.revision)?;
        let model = cache.model.ok_or_else(|| {
            refused(
                EditProblemView::NotAvailable,
                "the keyboard is not a supported model",
            )
        })?;
        if !matches!(cache.socd.data, TableRead::Ok(_)) {
            return Err(refused(
                EditProblemView::NotAvailable,
                "SOCD was not read or cannot be decoded",
            ));
        }
        if intent.power.is_none() && intent.pairs.is_none() {
            return Err(refused(EditProblemView::NoChanges, "nothing to change"));
        }
        let edit = SocdEdit {
            power: intent.power,
            pairs: intent
                .pairs
                .as_deref()
                .map(|p| pairs(model, p))
                .transpose()?,
            allow_not_undoable: intent.allow_not_undoable,
        };
        let mode = persistence_mode(intent.check_persistence);
        let shown = self.snapshot.as_ref().map(|s| s.profile);
        self.run_op(
            OpKind::SocdEdit,
            Arc::new(crate::sink::NoProgress),
            CancelPolicy::Never,
            |w, _| {
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let plan = plan_socd_edit(session, &edit).map_err(|e| edit_error(&e, model))?;
                if Some(plan.profile().get()) != shown {
                    w.reread = true;
                    w.invalidate(PlanInvalidation::ProfileChanged);
                    return Err(stale(PlanInvalidation::ProfileChanged));
                }
                let current = plan.current();
                let preview = SocdEditPreview {
                    token: None,
                    nothing_to_do: !plan.has_changes(),
                    profile: plan.profile().get(),
                    power_before: current.power == 1,
                    power_after: plan.power(),
                    pairs_before: current
                        .pairs
                        .iter()
                        .flatten()
                        .map(|p| socd_pair_view(model, p))
                        .collect(),
                    pairs_after: plan
                        .pairs()
                        .iter()
                        .map(|p| socd_pair_view(model, p))
                        .collect(),
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
                        PlanKind::SocdEdit,
                        PendingPlan::SocdEdit(Box::new(SocdPending { plan, mode })),
                    )
                });
                Ok(SocdEditPreview { token, ..preview })
            },
        )
    }

    /// `apply_socd_edit`: the confirmed plan, after the drift check and a
    /// verified safety backup.
    pub(crate) fn apply_socd_edit(
        &mut self,
        token: &PlanToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        let PendingPlan::SocdEdit(pending) = self.take_plan(token, PlanKind::SocdEdit)? else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        self.check_writable()?;
        self.run_op(
            OpKind::SocdEdit,
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
                // Drift check: the power and the pair bytes the plan was
                // made against (Fn + Right Shift may have switched SOCD).
                let now = match (
                    fresh.primary().response(ReadRequest::SocdSettings),
                    fresh.primary().response(ReadRequest::SocdPairs),
                ) {
                    (Some(settings), Some(pairs)) => SocdState::from_replies(settings, pairs).ok(),
                    _ => None,
                };
                let planned = plan.current();
                if now
                    .as_ref()
                    .is_none_or(|s| s.power != planned.power || s.raw_pairs != planned.raw_pairs)
                {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ValuesChanged));
                }
                let (saved, file) = w.save_safety(&fresh, "set-socd", progress)?;
                w.write_journal(OpKind::SocdEdit, vec![profile], saved.path())?;
                progress.items = sat16(plan.writes().len());
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let options = EditOptions {
                    write: WriteOptions::default(),
                    persistence: pending.mode,
                };
                let report = apply_socd_edit(
                    session,
                    plan,
                    &saved,
                    consent::application(),
                    &options,
                    |p| progress.write(p),
                );
                w.remove_journal();
                if w.foreign_heard() {
                    w.stale = true;
                } else if report.active_profile_now != Some(profile) {
                    w.reread = true;
                } else {
                    progress.step(ProgressStep::RefreshingView);
                    w.refresh_socd();
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
