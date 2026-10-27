//! Main-light edits (W3, with the music-rhythm leave W2 only when the mode
//! changes).

use std::ops::ControlFlow;

use openwraith_protocol::{
    FromResponse, LightLimits, LightMode, LightModeList, LightRecord, MainLightWrite, ProfileId,
    ReadRequest, Rgb, WriteRequest,
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

/// A main-light edit: what to change (at least one field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct LightEdit {
    /// New effect mode. On a mode change the direction is set to the mode's
    /// default (spec section 4.7.3).
    pub mode: Option<LightMode>,
    /// New foreground colour.
    pub foreground: Option<Rgb>,
    /// New brightness.
    pub brightness: Option<u8>,
    /// New speed.
    pub speed: Option<u8>,
}

/// A planned main-light edit; made by [`plan_light_edit`], not
/// deserializable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LightEditPlan {
    profile: ProfileId,
    product: Option<String>,
    current: LightRecord,
    target: LightRecord,
    #[serde(skip)]
    request: Option<WriteRequest>,
}

impl LightEditPlan {
    /// The active profile the plan was made for.
    pub fn profile(&self) -> ProfileId {
        self.profile
    }

    /// The record now.
    pub fn current(&self) -> &LightRecord {
        &self.current
    }

    /// The record after the edit.
    pub fn target(&self) -> &LightRecord {
        &self.target
    }

    /// Whether anything would be written.
    pub fn has_changes(&self) -> bool {
        self.request.is_some()
    }

    /// Whether the edit changes the mode (and so sends the music-rhythm
    /// leave first).
    pub fn changes_mode(&self) -> bool {
        self.current.as_bytes()[0] != self.target.as_bytes()[0]
    }

    /// The write, if anything changes (for a packet preview).
    pub fn request(&self) -> Option<&WriteRequest> {
        self.request.as_ref()
    }
}

/// Reads the current main light record and plans the edit (read-only).
pub fn plan_light_edit<T: Transport>(
    session: &mut Session<T>,
    edit: &LightEdit,
) -> Result<LightEditPlan, RestoreError> {
    if edit.mode.is_none()
        && edit.foreground.is_none()
        && edit.brightness.is_none()
        && edit.speed.is_none()
    {
        return Err(invalid(
            "nothing to change: give a mode, colour, brightness or speed",
        ));
    }
    let (_, info, model) = identify(session)?;
    let profile = active_profile(session)?;
    let modes_raw = session.request(ReadRequest::MainLightModes)?;
    let modes = LightModeList::from_response(&modes_raw)
        .map_err(|e| decode_error(ReadRequest::MainLightModes, e))?;
    let limits = LightLimits::new(model, &modes);
    let current_raw = session.request(ReadRequest::MainLight)?;
    let current = LightRecord::from_response(&current_raw).map_err(|e| invalid(e.to_string()))?;
    let mut target = current;
    if let Some(mode) = edit.mode {
        if mode != current.mode() {
            target = target
                .with_mode(mode)
                .with_direction(LightRecord::default_direction(mode));
        }
    }
    if let Some(color) = edit.foreground {
        target = target.with_foreground(color);
    }
    if let Some(brightness) = edit.brightness {
        target = target.with_brightness(brightness);
    }
    if let Some(speed) = edit.speed {
        target = target.with_speed(speed);
    }
    let write =
        MainLightWrite::new(&current, target, &limits).map_err(|e| invalid(e.to_string()))?;
    let request = (target != current).then_some(WriteRequest::MainLight(write));
    Ok(LightEditPlan {
        profile,
        product: info.product,
        current,
        target,
        request,
    })
}

/// Writes a planned main-light edit (see the [module documentation](super)).
/// The main light is known to be kept after the commit settle, so it never
/// switches profiles.
pub fn apply_light_edit<T: WriteTransport>(
    session: &mut Session<T>,
    plan: &LightEditPlan,
    saved: &SavedBackup,
    consent: WriteConsent,
    options: &WriteOptions,
    progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> EditReport {
    let writes: Vec<PlannedEditWrite> = plan
        .request
        .clone()
        .map(|request| {
            PlannedEditWrite::new(request, None, vec![Some(plan.current.as_bytes().to_vec())])
        })
        .into_iter()
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
