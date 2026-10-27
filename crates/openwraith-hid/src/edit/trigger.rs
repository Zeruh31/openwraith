//! Per-key trigger edits (actuation point and Rapid Trigger): single-key
//! trigger packets, the only captured form (W8).

use std::ops::ControlFlow;

use openwraith_protocol::{
    Distance, FromResponse, KeyMask, KeyPos, ProfileId, ReadRequest, TriggerGlobal, TriggerLimits,
    TriggerSetting, TriggerWrite, TriggerWriteMode, WriteRequest,
};
use serde::Serialize;

use super::{
    EditReport, EditRun, PersistenceCheckMode, PlannedEditWrite, active_profile, decode_error,
    invalid, run_edit,
};
use crate::error::RestoreError;
use crate::restore::WriteProgress;
use crate::safety::SavedBackup;
use crate::session::Session;
use crate::summary::identify;
use crate::transport::{Transport, WriteTransport};
use crate::writer::{WriteConsent, WriteOptions};

/// What to do with Rapid Trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "rapid_trigger", rename_all = "snake_case")]
pub enum RtEdit {
    /// Rapid Trigger off (mode `00`); the press and release values are kept.
    Off,
    /// Rapid Trigger off (mode `00`) with these press and release values
    /// stored (they apply when Rapid Trigger is switched on again): the
    /// factory record of a key is `00 55 55 01 01`, RT off with 0.02 mm.
    OffWith {
        /// Press sensitivity.
        press: Distance,
        /// Release sensitivity.
        release: Distance,
    },
    /// Rapid Trigger with one sensitivity for press and release (mode `0C`).
    Same {
        /// The sensitivity.
        sensitivity: Distance,
    },
    /// Rapid Trigger with separate press and release sensitivity (mode `0D`).
    Separate {
        /// Press sensitivity.
        press: Distance,
        /// Release sensitivity.
        release: Distance,
    },
}

/// A trigger edit: the keys and what to change (at least one of the two).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TriggerEdit {
    /// The keys (not the knob).
    pub keys: Vec<KeyPos>,
    /// New actuation point, if it changes.
    pub actuation: Option<Distance>,
    /// New Rapid Trigger setting, if it changes.
    pub rapid_trigger: Option<RtEdit>,
}

/// The planned change of one key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyEdit {
    /// The key.
    pub pos: KeyPos,
    /// The key's name.
    pub name: String,
    /// The settings now.
    pub current: TriggerSetting,
    /// The settings after the edit.
    pub target: TriggerSetting,
    /// The write (`None` when the key already has the target settings).
    #[serde(skip)]
    request: Option<WriteRequest>,
}

impl KeyEdit {
    /// Whether the key changes.
    pub fn changes(&self) -> bool {
        self.request.is_some()
    }

    /// The write, if the key changes (for a packet preview).
    pub fn request(&self) -> Option<&WriteRequest> {
        self.request.as_ref()
    }
}

/// A planned trigger edit; made by [`plan_trigger_edit`], not
/// deserializable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TriggerEditPlan {
    profile: ProfileId,
    product: Option<String>,
    keys: Vec<KeyEdit>,
}

impl TriggerEditPlan {
    /// The active profile the plan was made for.
    pub fn profile(&self) -> ProfileId {
        self.profile
    }

    /// Every key, in the requested order.
    pub fn keys(&self) -> &[KeyEdit] {
        &self.keys
    }

    /// Whether anything would be written.
    pub fn has_changes(&self) -> bool {
        self.keys.iter().any(KeyEdit::changes)
    }
}

/// Reads the current trigger settings of the keys and plans the edit
/// (read-only).
pub fn plan_trigger_edit<T: Transport>(
    session: &mut Session<T>,
    edit: &TriggerEdit,
) -> Result<TriggerEditPlan, RestoreError> {
    if edit.actuation.is_none() && edit.rapid_trigger.is_none() {
        return Err(invalid(
            "nothing to change: give an actuation point or a Rapid Trigger setting",
        ));
    }
    if edit.keys.is_empty() {
        return Err(invalid("no key given"));
    }
    let (_, info, model) = identify(session)?;
    let profile = active_profile(session)?;
    let global_raw = session.request(ReadRequest::TriggerGlobal)?;
    let global = TriggerGlobal::from_response(&global_raw)
        .map_err(|e| decode_error(ReadRequest::TriggerGlobal, e))?;
    let limits = TriggerLimits::new(model, &global);
    if !limits.is_supported() {
        return Err(invalid(
            "the keyboard's trigger parameters (21/04) do not allow trigger edits",
        ));
    }
    let mut keys: Vec<KeyEdit> = Vec::with_capacity(edit.keys.len());
    for &pos in &edit.keys {
        if keys.iter().any(|k| k.pos == pos) {
            continue;
        }
        let name = model.key_name(pos.index());
        if model.key_at(pos).is_none() || Some(pos.index()) == model.knob_index {
            return Err(invalid(format!(
                "{name} ({pos}) has no trigger settings that can be changed"
            )));
        }
        let raw = session.request(ReadRequest::KeyTrigger(pos))?;
        let (_, current) = TriggerSetting::from_response(&raw).map_err(|e| {
            invalid(format!(
                "cannot edit {name}: its current settings cannot be written back ({e})"
            ))
        })?;
        let target = edited(current, edit);
        let write = TriggerWrite::new(model, KeyMask::from_positions([pos]), target, &limits)
            .map_err(|e| invalid(format!("{name}: {e}")))?;
        let request = (target != current).then_some(WriteRequest::Trigger(write));
        keys.push(KeyEdit {
            pos,
            name,
            current,
            target,
            request,
        });
    }
    Ok(TriggerEditPlan {
        profile,
        product: info.product,
        keys,
    })
}

/// The settings after applying `edit` to `current`.
fn edited(current: TriggerSetting, edit: &TriggerEdit) -> TriggerSetting {
    let mut target = current;
    if let Some(actuation) = edit.actuation {
        target.actuation = actuation;
    }
    match edit.rapid_trigger {
        None => {}
        Some(RtEdit::Off) => target.mode = TriggerWriteMode::Off,
        Some(RtEdit::OffWith { press, release }) => {
            target.mode = TriggerWriteMode::Off;
            target.rt_press = press;
            target.rt_release = release;
        }
        Some(RtEdit::Same { sensitivity }) => {
            target.mode = TriggerWriteMode::RapidTrigger;
            target.rt_press = sensitivity;
            target.rt_release = sensitivity;
        }
        Some(RtEdit::Separate { press, release }) => {
            target.mode = TriggerWriteMode::RapidTriggerSeparate;
            target.rt_press = press;
            target.rt_release = release;
        }
    }
    target
}

/// Writes a planned trigger edit (see the [module documentation](super)).
/// Triggers are known to be kept after the commit settle, so it never
/// switches profiles.
pub fn apply_trigger_edit<T: WriteTransport>(
    session: &mut Session<T>,
    plan: &TriggerEditPlan,
    saved: &SavedBackup,
    consent: WriteConsent,
    options: &WriteOptions,
    progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> EditReport {
    let writes: Vec<PlannedEditWrite> = plan
        .keys
        .iter()
        .filter_map(|k| {
            k.request.clone().map(|request| {
                PlannedEditWrite::new(
                    request,
                    Some(k.pos),
                    vec![Some(k.current.read_bytes().to_vec())],
                )
            })
        })
        .collect();
    let run = EditRun {
        profile: plan.profile,
        product: plan.product.as_deref(),
        group_before: None,
        warnings: &[],
        persistence: PersistenceCheckMode::Never,
    };
    run_edit(session, &run, &writes, saved, consent, options, progress)
}
