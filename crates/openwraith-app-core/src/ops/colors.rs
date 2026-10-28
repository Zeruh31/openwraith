//! Key colours: custom colour table 1 (W4), optionally with a switch of the
//! main light to the Custom mode (W2 + W3), through the library's
//! `plan_color_edit` / `apply_color_edit`. Only keys of the layout change;
//! every other slot keeps its bytes as read.

use std::sync::Arc;

use openwraith_hid::protocol::{ColorTable, CustomTable, KeyPos, ReadRequest, Rgb};
use openwraith_hid::{
    ColorEdit, ColorEditPlan, EditOptions, PersistenceCheckMode, WriteOptions, apply_color_edit,
    plan_color_edit,
};

use super::consent;
use super::editors::{checked_cache, distinct_keys, persistence_mode};
use super::progress::CancelPolicy;
use super::snapshot::{ReadFail, ReadVia};
use crate::backend::DeviceBackend;
use crate::dto::{
    AppError, ColorChangeView, ColorEditIntent, ColorEditPreview, EditProblemView, EditReportView,
    LastReport, OpKind, PlanInvalidation, PlanToken, ProgressStep, ReadPurpose,
};
use crate::map::editors::{
    CUSTOM_MODE, edit_error, persistence_plan, refused, rgb_text, warning_view, write_view,
};
use crate::map::{edit_report_view, key_ref, sat16};
use crate::sink::ProgressSink;
use crate::worker::{PendingPlan, PlanKind, Worker, stale};

/// A confirmed colour plan waiting for its apply step.
pub(crate) struct ColorPending {
    /// The library's plan.
    pub(crate) plan: ColorEditPlan,
    /// The persistence check.
    pub(crate) mode: PersistenceCheckMode,
}

/// `"#rrggbb"` exactly.
fn color(text: &str) -> Result<Rgb, AppError> {
    Rgb::from_hex(text)
        .filter(|_| text.len() == 7 && text.starts_with('#'))
        .ok_or_else(|| {
            refused(
                EditProblemView::InvalidColor,
                "colours are written as #rrggbb",
            )
        })
}

impl<B: DeviceBackend> Worker<B> {
    /// `prepare_color_edit`: reads table 1 and the light (read-only) and
    /// stores the plan.
    pub(crate) fn prepare_color_edit(
        &mut self,
        intent: &ColorEditIntent,
    ) -> Result<ColorEditPreview, AppError> {
        self.check_writable()?;
        let cache = checked_cache(self, intent.based_on, |c| c.colors.revision)?;
        let model = cache.model.ok_or_else(|| {
            refused(
                EditProblemView::NotAvailable,
                "the keyboard is not a supported model",
            )
        })?;
        if !matches!(cache.colors.data.custom1, super::editors::TableRead::Ok(_)) {
            return Err(refused(
                EditProblemView::NotAvailable,
                "custom colour table 1 was not read or cannot be decoded",
            ));
        }
        let keys = distinct_keys(model, intent.colors.iter().map(|c| c.key))?;
        let colors = keys
            .into_iter()
            .zip(&intent.colors)
            .map(|(pos, c)| Ok((pos, color(&c.color)?)))
            .collect::<Result<Vec<(KeyPos, Rgb)>, AppError>>()?;
        let edit = ColorEdit {
            colors,
            switch_to_custom: intent.switch_to_custom,
        };
        let mode = persistence_mode(intent.check_persistence);
        let shown = self.snapshot.as_ref().map(|s| s.profile);
        self.run_op(
            OpKind::ColorEdit,
            Arc::new(crate::sink::NoProgress),
            CancelPolicy::Never,
            |w, _| {
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let plan = plan_color_edit(session, &edit).map_err(|e| edit_error(&e, model))?;
                if Some(plan.profile().get()) != shown {
                    w.reread = true;
                    w.invalidate(PlanInvalidation::ProfileChanged);
                    return Err(stale(PlanInvalidation::ProfileChanged));
                }
                let changes = plan
                    .changes()
                    .iter()
                    .map(|c| ColorChangeView {
                        key: key_ref(c.pos),
                        before: rgb_text(c.before),
                        after: rgb_text(c.after),
                    })
                    .collect();
                let preview = ColorEditPreview {
                    token: None,
                    nothing_to_do: !plan.has_changes(),
                    profile: plan.profile().get(),
                    changes,
                    shown_now: plan.shown_now(),
                    switches_to_custom: plan.writes().len() > 1
                        || plan
                            .writes()
                            .iter()
                            .any(|w| w.table == openwraith_hid::protocol::TableId::MainLight),
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
                        PlanKind::ColorEdit,
                        PendingPlan::ColorEdit(Box::new(ColorPending { plan, mode })),
                    )
                });
                Ok(ColorEditPreview { token, ..preview })
            },
        )
    }

    /// `apply_color_edit`: the confirmed plan, after the drift check and a
    /// verified safety backup.
    pub(crate) fn apply_color_edit(
        &mut self,
        token: &PlanToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        let PendingPlan::ColorEdit(pending) = self.take_plan(token, PlanKind::ColorEdit)? else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        self.check_writable()?;
        self.run_op(
            OpKind::ColorEdit,
            sink,
            CancelPolicy::Boundaries,
            |w, progress| {
                w.ensure_quiet(progress)?;
                let fresh = w
                    .full_read(ReadVia::Op(progress, ReadPurpose::SafetyBackup))
                    .map_err(ReadFail::into_error)?;
                // Drift check: the colours the plan changes and whether the
                // keyboard shows the table must be as planned.
                let plan = &pending.plan;
                let profile = plan.profile().get();
                if fresh.active_profile != Some(profile) {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ProfileChanged));
                }
                let table = fresh
                    .primary()
                    .get::<ColorTable>(ReadRequest::CustomColors(CustomTable::Custom1))
                    .and_then(Result::ok);
                let colors_kept = plan.changes().iter().all(|c| {
                    table
                        .as_ref()
                        .and_then(|t| t.color(c.pos.index()))
                        .is_some_and(|now| now == c.before)
                });
                let mode_now = fresh
                    .primary()
                    .response(ReadRequest::MainLight)
                    .and_then(|raw| raw.payload().first().copied());
                if !colors_kept || (mode_now == Some(CUSTOM_MODE)) != plan.shown_now() {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ValuesChanged));
                }
                let (saved, file) = w.save_safety(&fresh, "set-colors", progress)?;
                w.write_journal(OpKind::ColorEdit, vec![profile], saved.path())?;
                progress.items = sat16(plan.writes().len());
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let options = EditOptions {
                    write: WriteOptions::default(),
                    persistence: pending.mode,
                };
                let report = apply_color_edit(
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
                    let mut reread = w.refresh_colors();
                    if plan
                        .writes()
                        .iter()
                        .any(|w| w.table == openwraith_hid::protocol::TableId::MainLight)
                    {
                        reread = reread.zip(w.refresh_light()).map(|(mut a, b)| {
                            a.extend(b);
                            a
                        });
                    }
                    if clean && let Some(reread) = reread {
                        // What the edit left in the colours (and the light).
                        w.own_write_reread(OpKind::ColorEdit, &fresh, reread);
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
