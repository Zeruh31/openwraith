//! Dead-band edits (W9, `26 01`): top and bottom dead zones per key, in the
//! keyboard's 0.02 mm unit (spec 4.12). The whole 264-byte table is
//! written; keys not listed keep their bytes as read. Each profile has its
//! own dead band (OBSERVED on hardware, H16, 2026-10-27).

use std::ops::ControlFlow;

use openwraith_protocol::{
    DeadBandData, DeadBandWrite, Distance, KeyPos, ProfileId, ReadRequest, Status, TableId,
    TriggerGlobal, WriteRequest,
};
use serde::Serialize;

use super::{
    EditError, EditOptions, EditReport, EditRun, EditWarning, PlannedEditWrite, active_profile,
    run_edit,
};
use crate::restore::WriteProgress;
use crate::safety::SavedBackup;
use crate::session::Session;
use crate::summary::identify;
use crate::transport::{Transport, WriteTransport};
use crate::writer::WriteConsent;

/// A dead-band edit: the keys and what to change (at least one of the two).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeadBandEdit {
    /// The keys (not the knob).
    pub keys: Vec<KeyPos>,
    /// New top dead zone, if it changes.
    pub top: Option<Distance>,
    /// New bottom dead zone, if it changes.
    pub bottom: Option<Distance>,
}

/// The planned change of one key's dead band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DeadBandChange {
    /// The key.
    pub pos: KeyPos,
    /// `[top, bottom]` now, in trigger units.
    pub before: [u8; 2],
    /// `[top, bottom]` after the edit.
    pub after: [u8; 2],
}

/// A planned dead-band edit; made by [`plan_dead_band_edit`], not
/// deserializable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeadBandEditPlan {
    profile: ProfileId,
    product: Option<String>,
    changes: Vec<DeadBandChange>,
    writes: Vec<PlannedEditWrite>,
    warnings: Vec<EditWarning>,
}

impl DeadBandEditPlan {
    /// The active profile the plan was made for.
    pub fn profile(&self) -> ProfileId {
        self.profile
    }

    /// The keys whose dead band changes.
    pub fn changes(&self) -> &[DeadBandChange] {
        &self.changes
    }

    /// The writes (at most one).
    pub fn writes(&self) -> &[PlannedEditWrite] {
        &self.writes
    }

    /// What the user should know before writing.
    pub fn warnings(&self) -> &[EditWarning] {
        &self.warnings
    }

    /// Whether anything would be written.
    pub fn has_changes(&self) -> bool {
        !self.writes.is_empty()
    }
}

/// Reads the dead band (read-only) and plans the edit. Only the 0.02 mm
/// unit is supported ([`EditError::DistanceUnit`] otherwise).
pub fn plan_dead_band_edit<T: Transport>(
    session: &mut Session<T>,
    edit: &DeadBandEdit,
) -> Result<DeadBandEditPlan, EditError> {
    if edit.top.is_none() && edit.bottom.is_none() {
        return Err(EditError::Invalid {
            reason: "nothing to change: give a top or a bottom dead zone".to_string(),
        });
    }
    if edit.keys.is_empty() {
        return Err(EditError::Invalid {
            reason: "no key given".to_string(),
        });
    }
    let (_, info, model) = identify(session)?;
    let profile = active_profile(session)?;
    let status: Status = session.read(ReadRequest::Status)?;
    let global: TriggerGlobal = session.read(ReadRequest::TriggerGlobal)?;
    let unit = global.unit_micrometres(status.flags_b.high_precision);
    if unit != Distance::MICROMETRES_PER_UNIT {
        return Err(EditError::DistanceUnit {
            unit_um: Some(unit),
        });
    }
    let raw = session.request(ReadRequest::DeadBand)?;
    let current =
        DeadBandData::from_response(&raw).map_err(|e| EditError::UnsupportedCurrentTable {
            table: TableId::DeadBand,
            reason: e.to_string(),
        })?;
    let mut target = current.clone();
    let units = |d: Option<Distance>, which: &str| -> Result<Option<u8>, EditError> {
        d.map(|d| {
            u8::try_from(d.units()).map_err(|_| EditError::Invalid {
                reason: format!("the {which} dead zone {d} is too large"),
            })
        })
        .transpose()
    };
    let (top, bottom) = (units(edit.top, "top")?, units(edit.bottom, "bottom")?);
    let mut changes = Vec::new();
    for &pos in &edit.keys {
        let name = model.key_name(pos.index());
        if model.key_at(pos).is_none() || Some(pos.index()) == model.knob_index {
            return Err(EditError::Invalid {
                reason: format!("{name} ({pos}) has no dead band that can be changed"),
            });
        }
        let before = current.entry(pos);
        let after = [top.unwrap_or(before[0]), bottom.unwrap_or(before[1])];
        target.set_entry(pos, after);
        if before != after && !changes.iter().any(|c: &DeadBandChange| c.pos == pos) {
            changes.push(DeadBandChange { pos, before, after });
        }
    }
    let write = DeadBandWrite::new(model, target, DeadBandData::max_units(unit))?;
    let mut writes = Vec::new();
    if write.data() != &current {
        writes.push(PlannedEditWrite::new(
            WriteRequest::DeadBand(write),
            None,
            vec![Some(current.as_bytes().to_vec())],
        ));
    }
    Ok(DeadBandEditPlan {
        profile,
        product: info.product,
        changes,
        writes,
        warnings: Vec::new(),
    })
}

/// Writes a planned dead-band edit (see the [module documentation](super)).
pub fn apply_dead_band_edit<T: WriteTransport>(
    session: &mut Session<T>,
    plan: &DeadBandEditPlan,
    saved: &SavedBackup,
    consent: WriteConsent,
    options: &EditOptions,
    progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> EditReport {
    let run = EditRun {
        profile: plan.profile,
        product: plan.product.as_deref(),
        group_before: None,
        warnings: &plan.warnings,
        persistence: options.persistence,
    };
    run_edit(
        session,
        &run,
        &plan.writes,
        saved,
        consent,
        &options.write,
        progress,
    )
}
