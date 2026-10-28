//! What the colour, dead-band and SOCD editors show: built from the last
//! full read (no traffic), patched after their own edits (the written table
//! is read again) and, for SOCD, by the keyboard's push after Fn + Right
//! Shift. Each part has its own revision (`based_on` of its intents): an
//! edit of one part never invalidates a draft of another.
//!
//! Also the helpers the editors' operations share: key lists, the
//! persistence check option, the refresh of a written table.

use std::collections::HashSet;

use openwraith_hid::protocol::{
    ActiveCustomTable, ColorTable, CustomTable, DeadBandTable, DeviceEvent, KeyPos, Model,
    ReadRequest, TableId,
};
use openwraith_hid::{Backup, PersistenceCheckMode, PlanAction, SocdState};

use super::snapshot::may_send;
use crate::backend::DeviceBackend;
use crate::dto::{AppError, DeviceSnapshot, EditProblemView, Notice, PlanInvalidation};
use crate::map::editors::refused;
use crate::map::{key_ref_index, sat16};
use crate::worker::{PendingPlan, Worker};

/// The most keys of one edit (every key but the knob).
pub(crate) const MAX_KEYS: usize = 81;

/// What was read of one table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TableRead<T> {
    /// The keyboard did not answer the read.
    NotRead,
    /// The reply cannot be used.
    Unreadable(String),
    /// The table.
    Ok(T),
}

impl<T> TableRead<T> {
    fn of(result: Option<Result<T, String>>) -> Self {
        match result {
            None => TableRead::NotRead,
            Some(Ok(value)) => TableRead::Ok(value),
            Some(Err(message)) => TableRead::Unreadable(crate::map::clean(&message)),
        }
    }
}

/// One part of the cache with its revision.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Versioned<T> {
    /// Changes whenever the part changes.
    pub(crate) revision: u32,
    /// The part.
    pub(crate) data: T,
}

/// The colours of custom table 1 and the active custom table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ColorsData {
    /// Custom table 1.
    pub(crate) custom1: TableRead<ColorTable>,
    /// The active custom table (1 or 2), if known.
    pub(crate) active: Option<u8>,
}

/// See the [module documentation](self).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditorsCache {
    /// The connection it was read in.
    pub(crate) generation: u32,
    /// The active profile.
    pub(crate) profile: u8,
    /// When it was read.
    pub(crate) read_at: String,
    /// The keyboard's model (`None`: not supported).
    pub(crate) model: Option<&'static Model>,
    /// The distance unit in micrometres.
    pub(crate) unit_um: Option<u32>,
    /// Key colours.
    pub(crate) colors: Versioned<ColorsData>,
    /// The dead band.
    pub(crate) dead_band: Versioned<TableRead<DeadBandTable>>,
    /// SOCD.
    pub(crate) socd: Versioned<TableRead<SocdState>>,
}

impl EditorsCache {
    /// The parts of a fresh full read and the snapshot made from it.
    pub(crate) fn new(backup: &Backup, snapshot: &DeviceSnapshot) -> Self {
        let view = backup.primary();
        let custom1 = view
            .get::<ColorTable>(ReadRequest::CustomColors(CustomTable::Custom1))
            .map(|r| r.map_err(|e| e.to_string()));
        let active = view
            .get::<ActiveCustomTable>(ReadRequest::ActiveCustomTable)
            .and_then(Result::ok)
            .and_then(|a| a.table)
            .map(|t| match t {
                CustomTable::Custom1 => 1,
                CustomTable::Custom2 => 2,
            });
        let dead_band = view
            .get::<DeadBandTable>(ReadRequest::DeadBand)
            .map(|r| r.map_err(|e| e.to_string()));
        let socd = match (
            view.response(ReadRequest::SocdSettings),
            view.response(ReadRequest::SocdPairs),
        ) {
            (Some(settings), Some(pairs)) => {
                Some(SocdState::from_replies(settings, pairs).map_err(|e| e.to_string()))
            }
            _ => None,
        };
        let revision = snapshot.revision;
        EditorsCache {
            generation: snapshot.generation,
            profile: snapshot.profile,
            read_at: snapshot.read_at.clone(),
            model: view.model(),
            unit_um: snapshot.triggers.unit_um,
            colors: Versioned {
                revision,
                data: ColorsData {
                    custom1: TableRead::of(custom1),
                    active,
                },
            },
            dead_band: Versioned {
                revision,
                data: TableRead::of(dead_band),
            },
            socd: Versioned {
                revision,
                data: TableRead::of(socd),
            },
        }
    }
}

/// The persistence check of an editor's apply ("Check that the keyboard
/// kept it" is not pre-ticked; `Auto` checks only tables without hardware
/// evidence, which these editors never write).
pub(crate) fn persistence_mode(check: bool) -> PersistenceCheckMode {
    if check {
        PersistenceCheckMode::Always
    } else {
        PersistenceCheckMode::Auto
    }
}

/// A key of the layout that has the setting (not the knob).
pub(crate) fn layout_key(model: &Model, index: u8) -> Result<KeyPos, AppError> {
    if model.knob_index == Some(index) {
        return Err(refused(
            EditProblemView::Knob,
            "the knob has no such setting",
        ));
    }
    KeyPos::from_index(index)
        .filter(|_| model.key(index).is_some())
        .ok_or_else(|| {
            refused(
                EditProblemView::NotAKey { index },
                format!("#{index} is not a key of this keyboard"),
            )
        })
}

/// 1 to [`MAX_KEYS`] distinct keys of the layout (not the knob).
pub(crate) fn distinct_keys(
    model: &Model,
    keys: impl IntoIterator<Item = u8>,
) -> Result<Vec<KeyPos>, AppError> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for index in keys {
        if !seen.insert(index) {
            return Err(refused(
                EditProblemView::DuplicateKey {
                    key: key_ref_index(index),
                },
                format!("key #{index} is listed twice"),
            ));
        }
        out.push(layout_key(model, index)?);
    }
    if out.is_empty() {
        return Err(refused(EditProblemView::NoChanges, "no key given"));
    }
    if out.len() > MAX_KEYS {
        return Err(refused(
            EditProblemView::TooManyKeys {
                max: sat16(MAX_KEYS),
            },
            format!("at most {MAX_KEYS} keys per edit"),
        ));
    }
    Ok(out)
}

/// The cache, after checking that an intent was made against what it
/// shows (`revision` of the part, the same connection and profile).
pub(crate) fn checked_cache<B: DeviceBackend>(
    worker: &Worker<B>,
    based_on: u32,
    revision: impl Fn(&EditorsCache) -> u32,
) -> Result<std::sync::Arc<EditorsCache>, AppError> {
    let cache = worker
        .shared
        .editors()
        .ok_or_else(Worker::<B>::not_connected)?;
    let shown = worker.snapshot.as_ref().map(|s| (s.generation, s.profile));
    if based_on != revision(&cache) || shown != Some((cache.generation, cache.profile)) {
        return Err(crate::worker::stale(PlanInvalidation::SnapshotChanged));
    }
    Ok(cache)
}

impl<B: DeviceBackend> Worker<B> {
    /// A fresh revision for a changed part of the cache (never one used
    /// before in this run).
    pub(crate) fn next_revision(&mut self) -> u32 {
        self.snapshot_revision = self.snapshot_revision.wrapping_add(1);
        self.snapshot_revision
    }

    /// Reads custom table 1 again after a colour edit and patches the
    /// cache. Stops before the request once another program was heard.
    pub(crate) fn refresh_colors(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !may_send(session) {
            self.stale = true;
            return;
        }
        let read = session.read::<ColorTable>(ReadRequest::CustomColors(CustomTable::Custom1));
        let data = match read {
            Ok(table) => TableRead::Ok(table),
            Err(_) => {
                self.reread = true;
                return;
            }
        };
        let revision = self.next_revision();
        self.shared.update_editors(|cache| {
            cache.colors.revision = revision;
            cache.colors.data.custom1 = data;
        });
    }

    /// Reads the dead band again after its edit and patches the cache.
    pub(crate) fn refresh_dead_band(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !may_send(session) {
            self.stale = true;
            return;
        }
        let Ok(table) = session.read::<DeadBandTable>(ReadRequest::DeadBand) else {
            self.reread = true;
            return;
        };
        let revision = self.next_revision();
        self.shared.update_editors(|cache| {
            cache.dead_band = Versioned {
                revision,
                data: TableRead::Ok(table),
            };
        });
    }

    /// Reads SOCD again after its edit and patches the cache.
    pub(crate) fn refresh_socd(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !may_send(session) {
            self.stale = true;
            return;
        }
        let Ok(state) = SocdState::read(session) else {
            self.reread = true;
            return;
        };
        let revision = self.next_revision();
        self.shared.update_editors(|cache| {
            cache.socd = Versioned {
                revision,
                data: TableRead::Ok(state),
            };
        });
    }

    /// The keyboard's SOCD pushes (Fn + Right Shift) since the last look:
    /// the SOCD view takes the new power, a pending plan made against the
    /// old power (a SOCD edit, or a restore or import that compared the
    /// SOCD table) is invalidated, and the frontend is told. Sends nothing.
    pub(crate) fn check_socd_pushes(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let pushes = session.socd_pushes();
        if pushes <= self.socd_pushes_seen {
            return;
        }
        self.socd_pushes_seen = pushes;
        let Some(power) = session.events().iter().rev().find_map(|e| match e {
            DeviceEvent::SocdPower { power, .. } => Some(*power),
            _ => None,
        }) else {
            return;
        };
        let revision = self.next_revision();
        self.shared.update_editors(|cache| {
            cache.socd.revision = revision;
            if let TableRead::Ok(state) = &mut cache.socd.data {
                state.power = power;
            }
        });
        if self
            .pending
            .as_ref()
            .is_some_and(|p| made_against_socd_power(&p.plan))
        {
            self.invalidate(PlanInvalidation::ValuesChanged);
        }
        self.shared
            .notice(Notice::SocdToggledOnKeyboard { power: power == 1 });
    }
}

/// Whether a plan depends on the keyboard-wide SOCD power: a SOCD edit, a
/// restore whose plan compared the SOCD table (writes it or found it
/// unchanged), or an import that writes it (an import keeps the keyboard's
/// power, so it depends on it only when it writes the table). After a push
/// such a plan would stop half-way ("changed after it was planned") or
/// leave the new power in place.
fn made_against_socd_power(plan: &PendingPlan) -> bool {
    match plan {
        PendingPlan::SocdEdit(_) => true,
        PendingPlan::Restore { prepared, .. } => {
            let import = prepared.options().import.is_some();
            prepared
                .plan()
                .profiles
                .iter()
                .flat_map(|p| &p.items)
                .any(|item| {
                    item.table == TableId::Socd
                        && match item.action {
                            PlanAction::Write(_) => true,
                            PlanAction::Unchanged => !import,
                            _ => false,
                        }
                })
        }
        _ => false,
    }
}
