//! Per-key colours: custom colour table 1 (W4, `09 00`), optionally with a
//! switch of the main light to the Custom mode (W2 + W3).
//!
//! Only keys of the model other than the knob change; every other slot of
//! the 132-slot table (the knob, 103, 126 and the other positions outside
//! the layout) keeps its bytes as read. The main light's foreground colour
//! is never touched by the colour write. The keyboard shows table 1 only in
//! the Custom mode (`0x0A`).

use std::ops::ControlFlow;

use openwraith_protocol::{
    ColorTableData, CustomTable, FromResponse, KeyPos, LightLimits, LightMode, LightModeList,
    LightRecord, MainLightWrite, ProfileId, ReadRequest, Rgb, TableId, WriteRequest,
};
use serde::Serialize;

use super::{
    CUSTOM1, EditError, EditOptions, EditReport, EditRun, EditWarning, PlannedEditWrite,
    active_profile, run_edit,
};
use crate::restore::WriteProgress;
use crate::safety::SavedBackup;
use crate::session::Session;
use crate::summary::identify;
use crate::transport::{Transport, WriteTransport};
use crate::writer::WriteConsent;

/// The Custom light mode, which shows custom colour table 1.
const CUSTOM_MODE: u8 = 0x0A;

/// A colour edit: keys and their new colours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColorEdit {
    /// The keys (not the knob) and their colours.
    pub colors: Vec<(KeyPos, Rgb)>,
    /// Also switch the main light to the Custom mode when it is in another
    /// one (otherwise the keyboard does not show the table).
    pub switch_to_custom: bool,
}

/// The planned change of one key's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ColorChange {
    /// The key.
    pub pos: KeyPos,
    /// The colour now.
    pub before: Rgb,
    /// The colour after the edit.
    pub after: Rgb,
}

/// A planned colour edit; made by [`plan_color_edit`], not deserializable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColorEditPlan {
    profile: ProfileId,
    product: Option<String>,
    changes: Vec<ColorChange>,
    shown_now: bool,
    writes: Vec<PlannedEditWrite>,
    warnings: Vec<EditWarning>,
}

impl ColorEditPlan {
    /// The active profile the plan was made for.
    pub fn profile(&self) -> ProfileId {
        self.profile
    }

    /// The keys whose colour changes.
    pub fn changes(&self) -> &[ColorChange] {
        &self.changes
    }

    /// Whether the main light is in the Custom mode now (the table is
    /// shown).
    pub fn shown_now(&self) -> bool {
        self.shown_now
    }

    /// The writes, in order.
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

/// Reads custom colour table 1 and the main light (read-only) and plans the
/// edit.
pub fn plan_color_edit<T: Transport>(
    session: &mut Session<T>,
    edit: &ColorEdit,
) -> Result<ColorEditPlan, EditError> {
    if edit.colors.is_empty() {
        return Err(EditError::Invalid {
            reason: "no key colour given".to_string(),
        });
    }
    let (_, info, model) = identify(session)?;
    let profile = active_profile(session)?;
    let colors_read = ReadRequest::CustomColors(CustomTable::Custom1);
    let colors_raw = session.request(colors_read)?;
    let current = ColorTableData::from_response(&colors_raw).map_err(|e| {
        EditError::UnsupportedCurrentTable {
            table: CUSTOM1,
            reason: e.to_string(),
        }
    })?;
    let mut target = current.clone();
    target.set_key_colors(model, &edit.colors)?;
    let mut changes: Vec<ColorChange> = Vec::new();
    for (pos, _) in &edit.colors {
        if changes.iter().any(|c| c.pos == *pos) {
            continue;
        }
        let (before, after) = (current.color(*pos), target.color(*pos));
        if before != after {
            changes.push(ColorChange {
                pos: *pos,
                before,
                after,
            });
        }
    }
    let light_raw = session.request(ReadRequest::MainLight)?;
    let light =
        LightRecord::from_response(&light_raw).map_err(|e| EditError::UnsupportedCurrentTable {
            table: TableId::MainLight,
            reason: e.to_string(),
        })?;
    let shown_now = light.as_bytes()[0] == CUSTOM_MODE;
    let mut writes = Vec::new();
    let mut warnings = Vec::new();
    if target != current {
        writes.push(PlannedEditWrite::new(
            WriteRequest::CustomColors1(target),
            None,
            vec![Some(current.as_bytes().to_vec())],
        ));
        warnings.push(EditWarning::PowerLossUntested {
            tables: vec![CUSTOM1],
        });
    }
    if !shown_now {
        if edit.switch_to_custom {
            let modes_raw = session.request(ReadRequest::MainLightModes)?;
            let modes = LightModeList::from_response(&modes_raw).map_err(|e| {
                EditError::UnsupportedCurrentTable {
                    table: TableId::MainLight,
                    reason: e.to_string(),
                }
            })?;
            let mode = LightMode::from_id(CUSTOM_MODE);
            let custom = light
                .with_mode(mode)
                .with_direction(LightRecord::default_direction(mode));
            let write = MainLightWrite::new(&light, custom, &LightLimits::new(model, &modes))?;
            writes.push(PlannedEditWrite::new(
                WriteRequest::MainLight(write),
                None,
                vec![Some(light.as_bytes().to_vec())],
            ));
        } else {
            warnings.push(EditWarning::NotShownUntilCustomMode);
        }
    }
    Ok(ColorEditPlan {
        profile,
        product: info.product,
        changes,
        shown_now,
        writes,
        warnings,
    })
}

/// Writes a planned colour edit (see the [module documentation](super)).
pub fn apply_color_edit<T: WriteTransport>(
    session: &mut Session<T>,
    plan: &ColorEditPlan,
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
