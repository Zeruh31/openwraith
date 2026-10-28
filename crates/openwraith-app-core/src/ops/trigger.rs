//! Trigger edits: actuation point and Rapid Trigger, one packet per key.

use std::collections::HashSet;
use std::sync::Arc;

use openwraith_hid::protocol::{Distance, KeyPos, ReadRequest, W75};
use openwraith_hid::{
    Backup, RtEdit, Session, Transport, TriggerEdit, TriggerEditPlan, WriteOptions,
    apply_trigger_edit, plan_trigger_edit, read_summary,
};

use super::progress::CancelPolicy;
use super::snapshot::{ReadVia, may_send};
use super::{consent, invalid};
use crate::backend::DeviceBackend;
use crate::dto::{
    AppError, EditReportView, ErrorDetail, IntentField, KeyEditView, LastReport, OpKind,
    PlanInvalidation, PlanToken, RangeField, ReadPurpose, RtIntent, TriggerDisabled,
    TriggerEditIntent, TriggerEditPreview, TriggerLimitsView,
};
use crate::map::{edit_report_view, key_ref, key_trigger_view, sat16, trigger_value_of_setting};
use crate::sink::ProgressSink;
use crate::worker::{PendingPlan, PlanKind, Worker, stale};

/// The most keys of one edit (every key but the knob).
const MAX_KEYS: usize = 81;

/// Checks the unit and plans the edit (read-only).
pub(crate) fn plan<T: Transport>(
    session: &mut Session<T>,
    edit: &TriggerEdit,
) -> Result<TriggerEditPlan, AppError> {
    let summary = read_summary(session).map_err(|e| AppError::from(&e))?;
    let unit = summary
        .trigger_global
        .unit_micrometres(summary.status.flags_b.high_precision);
    if unit != Distance::MICROMETRES_PER_UNIT {
        return Err(invalid(
            ErrorDetail::DistanceUnit {
                unit_um: Some(unit),
            },
            format!(
                "the keyboard reports a distance unit of {unit} um; OpenWraith only changes \
                 values in 0.02 mm units. Nothing was written."
            ),
        ));
    }
    plan_trigger_edit(session, edit).map_err(|e| AppError::from(&e))
}

/// Refuses a value outside `min..=max` (never clamped).
pub(crate) fn range(field: RangeField, value: u16, min: u16, max: u16) -> Result<(), AppError> {
    if value < min || value > max {
        return Err(invalid(
            ErrorDetail::ValueOutOfRange {
                field,
                value,
                min,
                max,
            },
            format!("{field:?} {value} is outside {min}..={max}"),
        ));
    }
    Ok(())
}

/// The bytes a key's `21/05` reply in `backup` carries for its settings.
fn key_bytes(backup: &Backup, pos: KeyPos) -> Option<Vec<u8>> {
    backup
        .primary()
        .response(ReadRequest::KeyTrigger(pos))
        .and_then(|raw| raw.payload().get(1..10).map(<[u8]>::to_vec))
}

impl<B: DeviceBackend> Worker<B> {
    /// Checks keys against the snapshot the user saw (`based_on`): 1 to 81
    /// distinct keys that can be changed, and the keyboard's limits.
    pub(crate) fn trigger_keys(
        &self,
        indices: &[u8],
        based_on: u32,
    ) -> Result<(Vec<KeyPos>, TriggerLimitsView), AppError> {
        let snapshot = self.snapshot.as_ref().ok_or_else(Self::not_connected)?;
        if based_on != snapshot.revision {
            return Err(stale(PlanInvalidation::SnapshotChanged));
        }
        let triggers = &snapshot.triggers;
        match triggers.disabled {
            None => {}
            Some(TriggerDisabled::DistanceUnit) => {
                return Err(invalid(
                    ErrorDetail::DistanceUnit {
                        unit_um: triggers.unit_um,
                    },
                    "the keyboard reports another distance unit than 0.02 mm",
                ));
            }
            Some(reason) => {
                return Err(AppError::new(
                    crate::dto::AppErrorKind::InvalidRequest,
                    format!("trigger settings cannot be changed ({reason:?})"),
                ));
            }
        }
        let limits = triggers.limits.ok_or_else(|| {
            AppError::new(
                crate::dto::AppErrorKind::InvalidRequest,
                "the trigger limits are unknown",
            )
        })?;
        if indices.is_empty() || indices.len() > MAX_KEYS {
            return Err(invalid(
                ErrorDetail::InvalidIntent {
                    field: IntentField::Keys,
                },
                "choose between 1 and 81 keys",
            ));
        }
        let mut seen = HashSet::new();
        let mut keys = Vec::with_capacity(indices.len());
        for &index in indices {
            if !seen.insert(index) {
                return Err(invalid(
                    ErrorDetail::InvalidIntent {
                        field: IntentField::DuplicateKey,
                    },
                    format!("key {index} is listed twice"),
                ));
            }
            let Some(view) = triggers.keys.iter().find(|k| k.key.index == index) else {
                return Err(invalid(
                    ErrorDetail::InvalidIntent {
                        field: IntentField::Keys,
                    },
                    format!("there is no key {index}"),
                ));
            };
            if W75.knob_index == Some(index) || !view.selectable {
                return Err(invalid(
                    ErrorDetail::InvalidIntent {
                        field: IntentField::Knob,
                    },
                    "the knob has no trigger settings",
                ));
            }
            if let Some(reason) = view.not_editable {
                return Err(invalid(
                    ErrorDetail::KeyNotEditable {
                        key: view.key.clone(),
                        reason,
                    },
                    format!("{} cannot be changed safely ({reason:?})", view.key.name),
                ));
            }
            let pos = KeyPos::from_index(index).ok_or_else(|| {
                invalid(
                    ErrorDetail::InvalidIntent {
                        field: IntentField::Keys,
                    },
                    format!("there is no key {index}"),
                )
            })?;
            keys.push(pos);
        }
        Ok((keys, limits))
    }

    /// Validates an intent against the snapshot the user saw.
    fn trigger_edit(&self, intent: &TriggerEditIntent) -> Result<TriggerEdit, AppError> {
        let (keys, limits) = self.trigger_keys(&intent.keys, intent.based_on)?;
        if intent.actuation.is_none() && intent.rapid_trigger.is_none() {
            return Err(invalid(
                ErrorDetail::InvalidIntent {
                    field: IntentField::NothingToChange,
                },
                "nothing to change",
            ));
        }
        if let Some(actuation) = intent.actuation {
            range(
                RangeField::Actuation,
                actuation,
                limits.min_actuation,
                limits.max_actuation,
            )?;
        }
        let max = limits.max_sensitivity;
        let rapid_trigger = match intent.rapid_trigger {
            None => None,
            Some(RtIntent::Off) => Some(RtEdit::Off),
            Some(RtIntent::Same { sensitivity }) => {
                range(RangeField::Sensitivity, sensitivity, 1, max)?;
                Some(RtEdit::Same {
                    sensitivity: Distance::from_units(sensitivity),
                })
            }
            Some(RtIntent::Separate { press, release }) => {
                range(RangeField::Press, press, 1, max)?;
                range(RangeField::Release, release, 1, max)?;
                Some(RtEdit::Separate {
                    press: Distance::from_units(press),
                    release: Distance::from_units(release),
                })
            }
        };
        Ok(TriggerEdit {
            keys,
            actuation: intent.actuation.map(Distance::from_units),
            rapid_trigger,
        })
    }

    /// `prepare_trigger_edit`: reads the keys (read-only) and stores the
    /// plan.
    pub(crate) fn prepare_trigger_edit(
        &mut self,
        intent: &TriggerEditIntent,
    ) -> Result<TriggerEditPreview, AppError> {
        self.check_writable()?;
        let edit = self.trigger_edit(intent)?;
        self.prepare_trigger_plan(&edit)
    }

    /// Plans a checked trigger edit (read-only) and stores the plan; a
    /// trigger intent and a preset both end here.
    pub(crate) fn prepare_trigger_plan(
        &mut self,
        edit: &TriggerEdit,
    ) -> Result<TriggerEditPreview, AppError> {
        let shown = self.snapshot.as_ref().map(|s| s.profile);
        self.run_op(
            OpKind::TriggerEdit,
            Arc::new(crate::sink::NoProgress),
            CancelPolicy::Never,
            |w, _| {
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let plan = plan(session, edit)?;
                if Some(plan.profile().get()) != shown {
                    w.reread = true;
                    w.invalidate(PlanInvalidation::ProfileChanged);
                    return Err(stale(PlanInvalidation::ProfileChanged));
                }
                let keys: Vec<KeyEditView> = plan
                    .keys()
                    .iter()
                    .map(|k| KeyEditView {
                        key: key_ref(k.pos),
                        current: trigger_value_of_setting(&k.current),
                        target: trigger_value_of_setting(&k.target),
                        changes: k.changes(),
                    })
                    .collect();
                let changing = sat16(keys.iter().filter(|k| k.changes).count());
                let profile = plan.profile().get();
                let token = plan.has_changes().then(|| {
                    w.store_plan(
                        PlanKind::TriggerEdit,
                        PendingPlan::TriggerEdit(Box::new(plan)),
                    )
                });
                Ok(TriggerEditPreview {
                    nothing_to_do: token.is_none(),
                    token,
                    profile,
                    keys,
                    changing,
                    packets: changing,
                })
            },
        )
    }

    /// `apply_trigger_edit`: the confirmed plan, written one key per packet
    /// after a verified safety backup.
    pub(crate) fn apply_trigger_edit(
        &mut self,
        token: &PlanToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        let PendingPlan::TriggerEdit(plan) = self.take_plan(token, PlanKind::TriggerEdit)? else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        self.check_writable()?;
        self.run_op(
            OpKind::TriggerEdit,
            sink,
            CancelPolicy::Boundaries,
            |w, progress| {
                w.ensure_quiet(progress)?;
                let fresh = w
                    .full_read(ReadVia::Op(progress, ReadPurpose::SafetyBackup))
                    .map_err(super::snapshot::ReadFail::into_error)?;
                // Drift check: the plan must still describe the keyboard.
                let profile = plan.profile().get();
                if fresh.active_profile != Some(profile) {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ProfileChanged));
                }
                if plan.keys().iter().any(|k| {
                    key_bytes(&fresh, k.pos).as_deref() != Some(&k.current.read_bytes()[..])
                }) {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ValuesChanged));
                }
                let (saved, file) = w.save_safety(&fresh, "set-trigger", progress)?;
                w.write_journal(OpKind::TriggerEdit, vec![profile], saved.path())?;
                progress.items = sat16(plan.keys().iter().filter(|k| k.changes()).count());
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let report = apply_trigger_edit(
                    session,
                    &plan,
                    &saved,
                    consent::application(),
                    &WriteOptions::default(),
                    |p| progress.write(p),
                );
                w.remove_journal();
                if w.foreign_heard() {
                    // Another program talks to the keyboard: send nothing
                    // more (the state becomes `OtherProgram`).
                    w.stale = true;
                } else if report.active_profile_now != Some(profile) {
                    w.reread = true;
                } else {
                    progress.step(crate::dto::ProgressStep::RefreshingView);
                    w.refresh_keys(plan.keys().iter().map(|k| k.pos));
                }
                let view = edit_report_view(&report, file);
                w.shared.set_last_report(LastReport::Edit {
                    report: view.clone(),
                });
                Ok(view)
            },
        )
    }

    /// Reads the trigger settings of `keys` again and patches the snapshot.
    /// Stops before the next request once another program is heard (the
    /// snapshot stays as it was, marked stale; the state becomes
    /// `OtherProgram`).
    fn refresh_keys(&mut self, keys: impl Iterator<Item = KeyPos>) {
        let Some(mut snapshot) = self.snapshot.clone() else {
            return;
        };
        let Some(session) = self.session.as_mut() else {
            return;
        };
        for pos in keys {
            if !may_send(session) {
                self.stale = true;
                return;
            }
            let Ok(raw) = session.request(ReadRequest::KeyTrigger(pos)) else {
                self.reread = true;
                return;
            };
            let view = key_trigger_view(&W75, pos, Some(&raw));
            if let Some(slot) = snapshot
                .triggers
                .keys
                .iter_mut()
                .find(|k| k.key.index == pos.index())
            {
                *slot = view;
            }
        }
        self.update_snapshot(snapshot);
    }
}
