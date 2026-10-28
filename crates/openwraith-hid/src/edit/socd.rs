//! SOCD edits (W11): the power switch and the pair table, always written as
//! the vendor site's whole burst (`24 03`, then both `24 00` chunks; c15).
//! A power-only change re-sends the unchanged pairs, as the site's switch
//! does. Only `24 02` and `24 01` are read: the status report does not hold
//! the SOCD power (spec 7.4). The power is one setting of the whole
//! keyboard (switching it changes every profile), the pairs belong to the
//! active profile (OBSERVED on hardware, H21, 2026-10-27). SOCD can also be
//! switched on the keyboard with Fn + Right Shift; such a switch during the
//! write is detected
//! ([`WriteError::ChangedOnKeyboard`](crate::WriteError::ChangedOnKeyboard)).

use std::ops::ControlFlow;

use openwraith_protocol::{
    HexBytes, ProfileId, RawResponse, ReadRequest, SocdPair, SocdPairsData, SocdWrite, TableId,
    WriteBuildError, WriteRequest,
};
use serde::Serialize;

use super::{
    EditError, EditOptions, EditReport, EditRun, EditWarning, PlannedEditWrite, active_profile,
    run_edit,
};
use crate::restore::{WriteProgress, state_of};
use crate::safety::SavedBackup;
use crate::session::Session;
use crate::summary::identify;
use crate::transport::{Transport, WriteTransport};
use crate::writer::WriteConsent;

/// The keyboard's SOCD state as read (`24 02`, `24 01`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SocdState {
    /// Payload byte 0 of `24 02`: 1 = on (one setting of the whole
    /// keyboard: every profile reads the same power).
    pub power: u8,
    /// Payload bytes 1-5 of `24 02` (the hotkey; never written).
    pub rest: HexBytes,
    /// The pairs of the active profile, when the table is in the vendor
    /// site's form.
    pub pairs: Option<Vec<SocdPair>>,
    /// Why the pairs cannot be edited, when they are not in that form.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unwritable: Option<&'static str>,
    /// The 80 table bytes of `24 01`.
    pub raw_pairs: HexBytes,
}

impl SocdState {
    /// Reads `24 02` and `24 01` (read-only).
    pub fn read<T: Transport>(session: &mut Session<T>) -> Result<SocdState, EditError> {
        let settings = session.request(ReadRequest::SocdSettings)?;
        let pairs = session.request(ReadRequest::SocdPairs)?;
        SocdState::from_replies(&settings, &pairs)
    }

    /// The state of a `24 02` and a `24 01` reply.
    pub fn from_replies(settings: &RawResponse, pairs: &RawResponse) -> Result<Self, EditError> {
        let payload = settings.payload();
        let Some(power) = payload.first().copied() else {
            return Err(EditError::UnsupportedCurrentTable {
                table: TableId::Socd,
                reason: "the 24 02 reply is empty".to_string(),
            });
        };
        let raw_pairs =
            pairs
                .table_bytes("SOCD pairs")
                .map_err(|e| EditError::UnsupportedCurrentTable {
                    table: TableId::Socd,
                    reason: e.to_string(),
                })?;
        let (parsed, unwritable) = match SocdPairsData::from_response(pairs) {
            Ok(data) => (Some(data.pairs()), None),
            Err(WriteBuildError::UnwritableSocdPairs { reason }) => (None, Some(reason)),
            Err(_) => (None, Some("the table cannot be read")),
        };
        Ok(SocdState {
            power,
            rest: HexBytes(payload.get(1..).unwrap_or_default().to_vec()),
            pairs: parsed,
            unwritable,
            raw_pairs: HexBytes(raw_pairs),
        })
    }
}

/// A SOCD edit: the new power and/or the new list of pairs (replacing the
/// table; `None` keeps what the keyboard holds).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SocdEdit {
    /// Switch SOCD on or off (in every profile: the power is one setting
    /// of the whole keyboard).
    pub power: Option<bool>,
    /// The pairs of the active profile after the edit, in order.
    pub pairs: Option<Vec<SocdPair>>,
    /// Replace pairs that are not in the vendor site's form although the
    /// undo (a restore of the safety backup) cannot write them back
    /// ([`EditWarning::NotUndoable`]); without it such a replacement is
    /// refused.
    pub allow_not_undoable: bool,
}

/// A planned SOCD edit; made by [`plan_socd_edit`], not deserializable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SocdEditPlan {
    profile: ProfileId,
    product: Option<String>,
    current: SocdState,
    power: bool,
    pairs: Vec<SocdPair>,
    writes: Vec<PlannedEditWrite>,
    warnings: Vec<EditWarning>,
}

impl SocdEditPlan {
    /// The active profile the plan was made for.
    pub fn profile(&self) -> ProfileId {
        self.profile
    }

    /// The state now.
    pub fn current(&self) -> &SocdState {
        &self.current
    }

    /// The power after the edit.
    pub fn power(&self) -> bool {
        self.power
    }

    /// The pairs after the edit.
    pub fn pairs(&self) -> &[SocdPair] {
        &self.pairs
    }

    /// The writes (at most one burst).
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

/// Reads `24 02` and `24 01` (read-only) and plans the edit. Keeping the
/// pairs needs a table in the vendor site's form
/// ([`EditError::UnsupportedCurrentTable`] otherwise). Replacing pairs in
/// another form cannot be undone (a restore writes only pairs in the site's
/// form), so it needs [`SocdEdit::allow_not_undoable`].
pub fn plan_socd_edit<T: Transport>(
    session: &mut Session<T>,
    edit: &SocdEdit,
) -> Result<SocdEditPlan, EditError> {
    if edit.power.is_none() && edit.pairs.is_none() {
        return Err(EditError::Invalid {
            reason: "nothing to change: give the power or the pairs".to_string(),
        });
    }
    let (_, info, model) = identify(session)?;
    let profile = active_profile(session)?;
    let settings_raw = session.request(ReadRequest::SocdSettings)?;
    let pairs_raw = session.request(ReadRequest::SocdPairs)?;
    let current = SocdState::from_replies(&settings_raw, &pairs_raw)?;
    let current_power = match current.power {
        0 => false,
        1 => true,
        other => {
            return Err(EditError::UnsupportedCurrentTable {
                table: TableId::Socd,
                reason: format!("the SOCD power byte is {other:02x}, not 00 or 01"),
            });
        }
    };
    let mut warnings = Vec::new();
    if edit.power.is_some() {
        warnings.push(EditWarning::SocdPowerAllProfiles);
    }
    warnings.push(EditWarning::SocdHotkey);
    if let (Some(_), Some(reason)) = (&edit.pairs, current.unwritable) {
        if !edit.allow_not_undoable {
            return Err(EditError::UnsupportedCurrentTable {
                table: TableId::Socd,
                reason: format!(
                    "the pairs on the keyboard are not in the vendor site's form ({reason}), so \
                     a restore of the safety backup could not write them back: replacing them \
                     could not be undone"
                ),
            });
        }
        warnings.push(EditWarning::NotUndoable {
            table: TableId::Socd,
        });
    }
    let pairs = match &edit.pairs {
        Some(pairs) => SocdPairsData::from_pairs(model, pairs)?,
        None => SocdPairsData::from_response(&pairs_raw).map_err(|e| {
            EditError::UnsupportedCurrentTable {
                table: TableId::Socd,
                reason: format!("{e}; give the pairs to replace them"),
            }
        })?,
    };
    let power = edit.power.unwrap_or(current_power);
    let write = SocdWrite::new(power, pairs);
    warnings.extend(write.evidence().into_iter().map(EditWarning::Unverified));
    let unchanged = power == current_power && write.pairs().as_bytes()[..] == current.raw_pairs[..];
    let pairs_after = write.pairs().pairs();
    let mut writes = Vec::new();
    if !unchanged {
        let request = WriteRequest::Socd(write);
        let planned = state_of(&request, &[settings_raw, pairs_raw]);
        writes.push(PlannedEditWrite::new(request, None, planned));
    }
    Ok(SocdEditPlan {
        profile,
        product: info.product,
        current,
        power,
        pairs: pairs_after,
        writes,
        warnings,
    })
}

/// Writes a planned SOCD edit (see the [module documentation](super)).
pub fn apply_socd_edit<T: WriteTransport>(
    session: &mut Session<T>,
    plan: &SocdEditPlan,
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
