//! Main-light edits: effect, colour, brightness, speed.

use std::sync::Arc;

use openwraith_hid::protocol::{LightMode, ReadRequest, Rgb};
use openwraith_hid::{
    LightEdit, LightEditPlan, Session, Transport, WriteOptions, apply_light_edit, plan_light_edit,
};

use super::progress::CancelPolicy;
use super::snapshot::{ReadFail, ReadVia, may_send};
use super::{consent, invalid};
use crate::backend::DeviceBackend;
use crate::dto::{
    AppError, AppErrorKind, EditReportView, ErrorDetail, IntentField, LastReport, LightEditIntent,
    LightEditPreview, OpKind, PlanInvalidation, PlanToken, ProgressStep, RangeField, ReadPurpose,
};
use crate::map::{edit_report_view, light_state, light_value};
use crate::sink::ProgressSink;
use crate::worker::{PendingPlan, PlanKind, Worker, stale};

/// Plans the edit (read-only).
pub(crate) fn plan<T: Transport>(
    session: &mut Session<T>,
    edit: &LightEdit,
) -> Result<LightEditPlan, AppError> {
    plan_light_edit(session, edit).map_err(|e| AppError::from(&e))
}

fn out_of_range(field: RangeField, value: u8, max: u8) -> AppError {
    invalid(
        ErrorDetail::ValueOutOfRange {
            field,
            value: u16::from(value),
            min: 0,
            max: u16::from(max),
        },
        format!("{field:?} {value} is above {max}"),
    )
}

impl<B: DeviceBackend> Worker<B> {
    /// Validates an intent against the snapshot the user saw.
    fn light_edit(&self, intent: &LightEditIntent) -> Result<LightEdit, AppError> {
        let snapshot = self.snapshot.as_ref().ok_or_else(Self::not_connected)?;
        if intent.based_on != snapshot.revision {
            return Err(stale(PlanInvalidation::SnapshotChanged));
        }
        let light = &snapshot.light;
        if !light.editable {
            return Err(AppError::new(
                AppErrorKind::InvalidRequest,
                format!("the main light cannot be changed ({:?})", light.disabled),
            ));
        }
        let mode = match intent.mode {
            None => None,
            Some(id) => {
                let mode = LightMode::from_id(id);
                if mode == LightMode::MusicRhythm || !light.modes.iter().any(|m| m.id == id) {
                    return Err(invalid(
                        ErrorDetail::InvalidIntent {
                            field: IntentField::Mode,
                        },
                        format!("the keyboard does not offer light mode {id:#04x}"),
                    ));
                }
                Some(mode)
            }
        };
        let foreground = match &intent.color {
            None => None,
            Some(text) => {
                let valid = text.len() == 7 && text.starts_with('#');
                Some(Rgb::from_hex(text).filter(|_| valid).ok_or_else(|| {
                    invalid(
                        ErrorDetail::InvalidIntent {
                            field: IntentField::Color,
                        },
                        "colours are written as #rrggbb",
                    )
                })?)
            }
        };
        if let Some(brightness) = intent.brightness {
            if brightness > light.max_brightness {
                return Err(out_of_range(
                    RangeField::Brightness,
                    brightness,
                    light.max_brightness,
                ));
            }
        }
        if let Some(speed) = intent.speed {
            if speed > light.max_speed {
                return Err(out_of_range(RangeField::Speed, speed, light.max_speed));
            }
        }
        if mode.is_none()
            && foreground.is_none()
            && intent.brightness.is_none()
            && intent.speed.is_none()
        {
            return Err(invalid(
                ErrorDetail::InvalidIntent {
                    field: IntentField::NothingToChange,
                },
                "nothing to change",
            ));
        }
        Ok(LightEdit {
            mode,
            foreground,
            brightness: intent.brightness,
            speed: intent.speed,
        })
    }

    /// `prepare_light_edit`: reads the light (read-only) and stores the
    /// plan.
    pub(crate) fn prepare_light_edit(
        &mut self,
        intent: &LightEditIntent,
    ) -> Result<LightEditPreview, AppError> {
        self.check_writable()?;
        let edit = self.light_edit(intent)?;
        let shown = self.snapshot.as_ref().map(|s| s.profile);
        self.run_op(
            OpKind::LightEdit,
            Arc::new(crate::sink::NoProgress),
            CancelPolicy::Never,
            |w, _| {
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let plan = plan(session, &edit)?;
                if Some(plan.profile().get()) != shown {
                    w.reread = true;
                    w.invalidate(PlanInvalidation::ProfileChanged);
                    return Err(stale(PlanInvalidation::ProfileChanged));
                }
                let preview = LightEditPreview {
                    token: None,
                    profile: plan.profile().get(),
                    current: light_value(plan.current()),
                    target: light_value(plan.target()),
                    changes_mode: plan.changes_mode(),
                    packets: 1 + u8::from(plan.changes_mode()),
                    nothing_to_do: !plan.has_changes(),
                };
                let token = plan.has_changes().then(|| {
                    w.store_plan(PlanKind::LightEdit, PendingPlan::LightEdit(Box::new(plan)))
                });
                Ok(LightEditPreview { token, ..preview })
            },
        )
    }

    /// `apply_light_edit`: the confirmed plan, after a verified safety
    /// backup. Not cancellable once the safety backup is read.
    pub(crate) fn apply_light_edit(
        &mut self,
        token: &PlanToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        let PendingPlan::LightEdit(plan) = self.take_plan(token, PlanKind::LightEdit)? else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        self.check_writable()?;
        self.run_op(
            OpKind::LightEdit,
            sink,
            CancelPolicy::Boundaries,
            |w, progress| {
                w.ensure_quiet(progress)?;
                let fresh = w
                    .full_read(ReadVia::Op(progress, ReadPurpose::SafetyBackup))
                    .map_err(ReadFail::into_error)?;
                let profile = plan.profile().get();
                if fresh.active_profile != Some(profile) {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ProfileChanged));
                }
                let current = fresh
                    .primary()
                    .response(ReadRequest::MainLight)
                    .map(|raw| raw.payload());
                if current.as_deref() != Some(&plan.current().as_bytes()[..]) {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ValuesChanged));
                }
                progress.policy = CancelPolicy::Never;
                let (saved, file) = w.save_safety(&fresh, "set-light", progress)?;
                w.write_journal(OpKind::LightEdit, vec![profile], saved.path())?;
                progress.items = 1;
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let report = apply_light_edit(
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
                    progress.step(ProgressStep::RefreshingView);
                    w.refresh_light();
                }
                let view = edit_report_view(&report, file);
                w.shared.set_last_report(LastReport::Edit {
                    report: view.clone(),
                });
                Ok(view)
            },
        )
    }

    /// Reads the main light again and patches the snapshot. Stops before
    /// the next request once another program is heard (the snapshot stays
    /// as it was, marked stale; the state becomes `OtherProgram`).
    pub(crate) fn refresh_light(&mut self) {
        let Some(mut snapshot) = self.snapshot.clone() else {
            return;
        };
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !may_send(session) {
            self.stale = true;
            return;
        }
        let modes = session.request(ReadRequest::MainLightModes);
        if !may_send(session) {
            self.stale = true;
            return;
        }
        let light = session.request(ReadRequest::MainLight);
        match (modes, light) {
            (Ok(modes), Ok(light)) => {
                let model = snapshot
                    .device
                    .product
                    .as_deref()
                    .and_then(openwraith_hid::protocol::model_for_product);
                snapshot.light = light_state(model, Some(&modes), Some(&light));
                self.update_snapshot(snapshot);
            }
            _ => self.reread = true,
        }
    }
}
