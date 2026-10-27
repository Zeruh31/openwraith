//! Editing settings of the active profile (behind `openwraith set` and a
//! GUI's editors): per-key triggers and presets, the main light, the keymap
//! group (key assignments on both layers, macros, advanced keys), the
//! colours of custom table 1, the dead band and SOCD.
//!
//! An edit has two steps, like a restore:
//!
//! 1. `plan_*_edit` reads the current values (read-only, generic over
//!    [`Transport`]) and computes the new ones from the user's intent,
//!    validated against the keyboard's limits and the protocol's rules. The
//!    plan shows "current -> new", every write in order, and what is not
//!    verified on hardware ([`EditWarning`]) before anything is written.
//! 2. `apply_*_edit` writes the plan. It needs a [`SavedBackup`] of the
//!    active profile (made before, see
//!    [`Backup::save_safety`](crate::Backup::save_safety)) and consent. Each
//!    table is re-read first: if it no longer holds the planned bytes the
//!    edit stops with `changed_since_plan`; if it already holds the new ones
//!    nothing is written. Every write is verified by reading it back
//!    (stop-and-wait, one more attempt with the whole table).
//!
//! Edits never switch profiles, except the optional persistence check. A
//! GUI commits on release (one edit per gesture), never a stream of writes.
//!
//! # Saving
//!
//! The keyboard saves a write about 1 s after it (spec 4.5.3, "Delayed
//! commit"): a profile switch or a power loss before that loses it, and the
//! read-back cannot tell. An edit therefore waits for the writer's
//! [`CommitSettle`](crate::CommitSettle) (2 s) after its last write before
//! it returns ([`EditReport::waited_for_commit_ms`]). When another profile
//! became active (a switch on the keyboard need not be reported) or the
//! connection was lost before the save, the edit fails and its writes are
//! reported as [`ItemResult::MaybeLost`].
//!
//! Whether the keyboard keeps a table after the settle is CONFIRMED for
//! triggers, the main light, the dead band, custom colours 1 and the keymap
//! layers, and for advanced-key slots written after a complete Fn keymap;
//! it is not tested for macro slots and SOCD. The **persistence check**
//! ([`PersistenceCheckMode`]) proves it: it switches to the lowest other
//! profile and back (the lighting changes briefly) and reads the written
//! tables again; a table the keyboard did not keep is reported
//! [`ItemResult::Lost`]. [`EditOptions::default`] checks exactly the tables
//! without that evidence ([`PersistenceCheckMode::Auto`]); trigger and light
//! edits ([`apply_trigger_edit`], [`apply_light_edit`]) never check.
//!
//! # The keymap group
//!
//! Key assignments, macros and advanced keys are one group (see
//! [`keymap`]): it is written in the vendor site's order (changed macro
//! slots, the base keymap, the Fn keymap, changed advanced-key slots), the
//! Fn keymap is written after any base keymap (the keyboard keeps a base
//! keymap only then, spec 4.8.2) and, when only slots change, between the
//! macro and the advanced-key slots, and a cancellation never separates a
//! binding from what it needs.

mod colors;
mod dead_band;
pub mod keymap;
mod light;
mod socd;
mod trigger;

use std::ops::ControlFlow;
use std::path::PathBuf;

use openwraith_protocol::{
    ActiveProfile, CustomTable, KeyPos, Layer, PlacementError, ProfileId, ReadRequest, TableId,
    UnverifiedNote, WriteBuildError, WriteRequest, WriteStep,
};
use serde::Serialize;

use crate::error::{BackupError, ErrorInfo, ErrorKind, RestoreError, SessionError};
use crate::group::{
    DanglingBinding, GroupCursor, GroupView, GroupWrite, SlotRef, Step, dangling_after,
};
use crate::persist::{CheckOutcome, persistence_confirmed, recheck, reload_profile};
use crate::restore::{
    Checked, CommitTriggerReason, ItemReport, ItemResult, Outcome, PersistenceCheck, TableFailure,
    TableState, WriteProgress, apply_to_view, confirm_profile, profile_after_gate_failure,
    write_checked,
};
use crate::safety::SavedBackup;
use crate::session::{ForeignActivity, Session};
use crate::transport::{Transport, WriteTransport};
use crate::writer::{CommitTrigger, PacketProgress, WriteConsent, WriteOptions};

pub use colors::{ColorChange, ColorEdit, ColorEditPlan, apply_color_edit, plan_color_edit};
pub use dead_band::{
    DeadBandChange, DeadBandEdit, DeadBandEditPlan, apply_dead_band_edit, plan_dead_band_edit,
};
pub use keymap::{
    AdvancedSlotView, CommitTriggerMode, EntryChangeView, KeymapChange, KeymapEdit, KeymapEditPlan,
    KeymapGroupState, MacroSlotView, SlotChangeView, apply_keymap_edit, plan_keymap_edit,
    plan_keymap_edit_on,
};
pub use light::{LightEdit, LightEditPlan, apply_light_edit, plan_light_edit};
pub use socd::{SocdEdit, SocdEditPlan, SocdState, apply_socd_edit, plan_socd_edit};
pub use trigger::{
    KeyEdit, RtEdit, TriggerEdit, TriggerEditPlan, apply_trigger_edit, plan_trigger_edit,
};

/// When an edit proves the save by switching profiles away and back (see
/// the [module documentation](self)).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceCheckMode {
    /// Never (the commit settle is relied on).
    Never,
    /// Always, after a complete edit that wrote anything.
    Always,
    /// When a written table is not known to be kept after the settle
    /// (macro slots, SOCD, advanced-key slots without an Fn keymap before
    /// them).
    #[default]
    Auto,
}

/// Options of the `apply_*_edit` operations of the v0.2 editors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditOptions {
    /// Writer timeouts and the commit settle.
    pub write: WriteOptions,
    /// The persistence check.
    pub persistence: PersistenceCheckMode,
}

/// Which kind of slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    /// Macro slots (10).
    Macro,
    /// Advanced-key slots (40).
    Advanced,
}

impl std::fmt::Display for SlotKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SlotKind::Macro => "macro",
            SlotKind::Advanced => "advanced-key",
        })
    }
}

/// The name of a layer in messages.
fn layer_name(layer: Layer) -> &'static str {
    match layer {
        Layer::Base => "base",
        Layer::Fn => "Fn",
    }
}

/// An edit could not be planned. Nothing was written.
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// The requested edit is not valid.
    #[error("{reason}")]
    Invalid {
        /// Why.
        reason: String,
    },
    /// An assignment cannot go to that key and layer.
    #[error(transparent)]
    Placement(#[from] PlacementError),
    /// Every slot of the kind is in use.
    #[error("no free {kind} slot: every slot holds content or is bound to a key")]
    NoFreeSlot {
        /// Which kind.
        kind: SlotKind,
    },
    /// The slot is already bound to another key (one slot per key, as the
    /// vendor site does).
    #[error(
        "{slot} is already bound to key #{} on the {} layer",
        by.1,
        layer_name(by.0)
    )]
    SlotAlreadyBound {
        /// The slot.
        slot: SlotRef,
        /// The layer and key index of the other binding.
        by: (Layer, u8),
    },
    /// A key would be bound to a slot that is empty after the edit.
    #[error("key #{index} on the {} layer would refer to the empty {slot}", layer_name(*layer))]
    BindingToEmptySlot {
        /// The layer.
        layer: Layer,
        /// The key index.
        index: u8,
        /// The slot.
        slot: SlotRef,
    },
    /// A slot would be cleared while keys still refer to it.
    #[error("{slot} cannot be cleared: {} key(s) still refer to it", by.len())]
    SlotStillReferenced {
        /// The slot.
        slot: SlotRef,
        /// The layers and key indices that refer to it.
        by: Vec<(Layer, u8)>,
    },
    /// The keyboard's current table cannot be edited (unreadable, or in a
    /// form OpenWraith cannot write back).
    #[error("{table} cannot be edited: {reason}")]
    UnsupportedCurrentTable {
        /// The table.
        table: TableId,
        /// Why.
        reason: String,
    },
    /// The keyboard's distance unit is not the 0.02 mm OpenWraith writes.
    #[error(
        "the keyboard's distance unit is {}, not the 0.02 mm OpenWraith writes",
        unit_um.map_or_else(|| "unknown".to_string(), |u| format!("{u} um"))
    )]
    DistanceUnit {
        /// The unit in micrometres, if known.
        unit_um: Option<u32>,
    },
    /// A write could not be built.
    #[error(transparent)]
    Build(#[from] WriteBuildError),
    /// A read failed.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// Identifying the keyboard failed.
    #[error(transparent)]
    Backup(#[from] BackupError),
    /// Planning failed like a restore plan does.
    #[error(transparent)]
    Restore(#[from] RestoreError),
}

impl EditError {
    /// The machine-readable class of this error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            EditError::Invalid { .. }
            | EditError::Placement(_)
            | EditError::NoFreeSlot { .. }
            | EditError::SlotAlreadyBound { .. }
            | EditError::BindingToEmptySlot { .. }
            | EditError::SlotStillReferenced { .. }
            | EditError::DistanceUnit { .. }
            | EditError::Build(_) => ErrorKind::InvalidRequest,
            EditError::UnsupportedCurrentTable { .. } => ErrorKind::UnexpectedReply,
            EditError::Session(e) => e.kind(),
            EditError::Backup(e) => e.kind(),
            EditError::Restore(e) => e.kind(),
        }
    }
}

/// Something about an edit plan the user should know before writing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "warning", rename_all = "snake_case")]
pub enum EditWarning {
    /// The Fn-layer entry replaces a built-in Fn function (acknowledged).
    FnBuiltinOverridden {
        /// The key index.
        index: u8,
        /// The built-in function.
        function: &'static str,
        /// Whether it is a critical one (Fn + 1..4, Fn + Space).
        critical: bool,
    },
    /// The vendor site sends the same bytes, but OpenWraith has not yet
    /// tested on a W75 that it does what it says.
    Unverified(UnverifiedNote),
    /// The table's profile scope is unknown: it may change the other
    /// profiles too (dead band, SOCD).
    ScopeUnknown {
        /// The table.
        table: TableId,
    },
    /// A malformed macro or advanced-key entry of a written layer is kept
    /// byte for byte.
    MalformedReferencePreserved {
        /// The layer.
        layer: Layer,
        /// The key index.
        index: u8,
    },
    /// An entry of a written layer already referred to an empty slot; it
    /// is kept byte for byte.
    DanglingReferencePreserved {
        /// The layer.
        layer: Layer,
        /// The key index.
        index: u8,
        /// The empty slot.
        slot: SlotRef,
    },
    /// In the site's order the binding (a layer) is written before the
    /// advanced-key slot it refers to: for a moment the key refers to the
    /// slot's old content. The slot is written even after a cancellation.
    TransientBinding {
        /// The slot.
        slot: SlotRef,
    },
    /// A slot whose last binding this edit removes keeps its content (the
    /// keyboard keeps unbound slots).
    OrphanSlotKept {
        /// The slot.
        slot: SlotRef,
    },
    /// The colours are written to custom table 1, which the keyboard shows
    /// only in the Custom light mode, and the light is in another mode.
    NotShownUntilCustomMode,
    /// Whether the keyboard keeps these tables across a power loss is not
    /// tested.
    PowerLossUntested {
        /// The tables.
        tables: Vec<TableId>,
    },
    /// SOCD can also be switched on the keyboard with Fn + Right Shift.
    SocdHotkey,
    /// The edit replaces content the undo (a restore of the safety backup)
    /// cannot write back, because it is not in a form OpenWraith writes;
    /// allowed explicitly ([`KeymapEdit::allow_not_undoable`],
    /// [`SocdEdit::allow_not_undoable`]).
    NotUndoable {
        /// The table.
        table: TableId,
    },
}

/// One planned write of an edit. Made by the `plan_*_edit` functions; for
/// display (`request` is not serialized, see `kind` and `summary`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedEditWrite {
    /// The table.
    pub table: TableId,
    /// The key, for trigger writes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<KeyPos>,
    /// Request kind, e.g. `"keymap"`.
    pub kind: &'static str,
    /// Human-readable description.
    pub summary: String,
    /// Packets the write sends (its first attempt).
    pub packets: usize,
    /// Set when the write is the keymap group's commit trigger (the Fn
    /// keymap, sent even when the keyboard holds it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_trigger: Option<CommitTriggerReason>,
    /// Whether it re-sends an unchanged Fn keymap as the commit trigger.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub resent_unchanged: bool,
    /// Whether it completes a binding of a layer written in the same group
    /// (it is written even after a cancellation).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub completes_binding: bool,
    /// The tables whose binding it completes.
    #[serde(skip)]
    pub(crate) binding_tables: Vec<TableId>,
    #[serde(skip)]
    request: WriteRequest,
    /// The bytes the write was planned against (`changed_since_plan`).
    #[serde(skip)]
    planned: TableState,
}

impl PlannedEditWrite {
    /// A plain write of `request`, planned against `planned`.
    pub(crate) fn new(request: WriteRequest, key: Option<KeyPos>, planned: TableState) -> Self {
        PlannedEditWrite {
            table: request.table(),
            key,
            kind: request.kind(),
            summary: request.to_string(),
            packets: request.steps().len(),
            commit_trigger: None,
            resent_unchanged: false,
            completes_binding: false,
            binding_tables: Vec::new(),
            request,
            planned,
        }
    }

    /// The request (for a packet preview).
    pub fn request(&self) -> &WriteRequest {
        &self.request
    }

    /// The packets of the first attempt, in order (for a packet preview).
    pub fn steps(&self) -> Vec<WriteStep> {
        self.request.steps()
    }
}

impl GroupWrite for PlannedEditWrite {
    fn table(&self) -> TableId {
        self.table
    }

    fn is_trigger(&self) -> bool {
        self.commit_trigger.is_some()
    }

    fn binding_tables(&self) -> &[TableId] {
        &self.binding_tables
    }
}

/// What an edit did. Serializable for a GUI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditReport {
    /// How it ended.
    pub outcome: Outcome,
    /// The error, if it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    /// How the user agreed.
    pub consent: WriteConsent,
    /// The profile that was edited.
    pub profile: u8,
    /// The profile that is active now, as far as known (as last read, or as
    /// a profile report of a switch named it).
    pub active_profile_now: Option<u8>,
    /// One entry per planned write.
    pub items: Vec<ItemReport>,
    /// The safety backup made before the edit.
    pub safety_backup: PathBuf,
    /// The command that undoes the edit.
    pub undo_command: String,
    /// Reports of another program seen while writing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreign_activity: Option<ForeignActivity>,
    /// How long the edit waited at the end for the keyboard to save its last
    /// write, in milliseconds; `None` when nothing was pending (or the
    /// writer could not wait after a reset or a lost connection).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waited_for_commit_ms: Option<u64>,
    /// The persistence check, if one was made.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persistence: Option<PersistenceCheck>,
    /// The plan's warnings.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<EditWarning>,
    /// Keys left referring to an empty slot because the edit stopped
    /// part-way (the undo command restores them).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dangling: Vec<DanglingBinding>,
    /// A cancellation was asked for (Ctrl+C), but every write still made
    /// after it was needed to keep the keymap group consistent (the Fn
    /// keymap that makes the keyboard save a base keymap, a slot a written
    /// key refers to): with [`Outcome::Completed`] everything planned was
    /// written.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cancel_requested: bool,
}

impl EditReport {
    /// Whether every planned write was made and verified.
    pub fn is_success(&self) -> bool {
        matches!(self.outcome, Outcome::Completed | Outcome::NothingToDo)
    }

    /// Number of writes made and verified, not counting the ones the
    /// keyboard may not have saved ([`maybe_lost`](Self::maybe_lost)) or did
    /// not keep ([`lost`](Self::lost)).
    pub fn written(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.result, ItemResult::Written { .. }))
            .count()
    }

    /// Number of writes made and verified that the keyboard may not have
    /// saved ([`ItemResult::MaybeLost`]: the active profile changed or the
    /// connection was lost before the save).
    pub fn maybe_lost(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.result, ItemResult::MaybeLost { .. }))
            .count()
    }

    /// Number of writes the persistence check found gone
    /// ([`ItemResult::Lost`]).
    pub fn lost(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.result, ItemResult::Lost { .. }))
            .count()
    }

    /// Whether anything was written, also when the keyboard may not have
    /// saved it.
    pub fn anything_written(&self) -> bool {
        self.items.iter().any(|i| i.result.was_sent())
    }

    fn fail(&mut self, error: ErrorInfo) {
        self.outcome = Outcome::Failed;
        if self.error.is_none() {
            self.error = Some(error);
        }
    }
}

pub(crate) fn invalid(reason: impl Into<String>) -> RestoreError {
    RestoreError::InvalidRequest {
        reason: reason.into(),
    }
}

pub(crate) fn decode_error(
    request: ReadRequest,
    source: openwraith_protocol::DecodeError,
) -> RestoreError {
    RestoreError::from(SessionError::Decode { request, source })
}

/// The active profile, which must be 1-4.
pub(crate) fn active_profile<T: Transport>(
    session: &mut Session<T>,
) -> Result<ProfileId, RestoreError> {
    let active: ActiveProfile = session.read(ReadRequest::ActiveProfile)?;
    ProfileId::new(active.profile).ok_or_else(|| {
        invalid(format!(
            "the keyboard reports active profile {}; only profiles 1-4 are supported",
            active.profile
        ))
    })
}

/// Custom colour table 1 as a [`TableId`].
pub(crate) const CUSTOM1: TableId = TableId::CustomColors {
    table: CustomTable::Custom1,
};

/// What [`run_edit`] needs besides the writes.
pub(crate) struct EditRun<'a> {
    /// The active profile the edit was planned for.
    pub(crate) profile: ProfileId,
    /// The keyboard's product code at planning.
    pub(crate) product: Option<&'a str>,
    /// The keymap group before the edit (for dangling references), if the
    /// edit writes it.
    pub(crate) group_before: Option<&'a GroupView>,
    /// The plan's warnings, copied into the report.
    pub(crate) warnings: &'a [EditWarning],
    /// The persistence check.
    pub(crate) persistence: PersistenceCheckMode,
}

/// The edit engine: safety-backup check, the "other program" gate, every
/// write through [`write_checked`] with the keymap group's cursor rules,
/// the active-profile confirmation, the commit wait, `MaybeLost` on an
/// interrupted save, the persistence check, dangling references.
pub(crate) fn run_edit<T: WriteTransport>(
    session: &mut Session<T>,
    run: &EditRun<'_>,
    writes: &[PlannedEditWrite],
    saved: &SavedBackup,
    consent: WriteConsent,
    write_options: &WriteOptions,
    mut progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> EditReport {
    let profile = run.profile;
    let mut report = EditReport {
        outcome: Outcome::Completed,
        error: None,
        consent,
        profile: profile.get(),
        active_profile_now: Some(profile.get()),
        items: writes
            .iter()
            .map(|w| ItemReport {
                table: w.table,
                key: w.key,
                summary: w.summary.clone(),
                commit_trigger: w.commit_trigger,
                resent_unchanged: w.resent_unchanged,
                result: ItemResult::NotAttempted,
            })
            .collect(),
        safety_backup: saved.path().to_path_buf(),
        undo_command: saved.undo_command(),
        foreign_activity: None,
        waited_for_commit_ms: None,
        persistence: None,
        warnings: run.warnings.to_vec(),
        dangling: Vec::new(),
        cancel_requested: false,
    };
    if !saved.covers(profile) || saved.product() != run.product {
        let error = RestoreError::SafetyBackupMismatch {
            path: saved.path().to_path_buf(),
        };
        report.fail(ErrorInfo::new(error.kind(), &error));
        return report;
    }
    if writes.is_empty() {
        report.outcome = Outcome::NothingToDo;
        return report;
    }
    if let Err(e) = consent.ensure_not_standing_rule() {
        report.fail(ErrorInfo::new(e.kind(), &e));
        return report;
    }
    let mut writer = match session.writer_with(consent, write_options.clone()) {
        Ok(writer) => writer,
        Err(e) => {
            report.fail(ErrorInfo::new(e.kind(), &e));
            report.active_profile_now =
                profile_after_gate_failure(session, &e, Some(profile.get()));
            return report;
        }
    };
    let mut cursor = GroupCursor::new(writes);
    let mut cancel = false;
    // The SOCD push count right after a SOCD write (a later switch on the
    // keyboard is not a lost write).
    let mut socd_mark: Option<u64> = None;
    // Whether any packet of each write went out (a write that failed after
    // its first packet may have changed the keyboard's working copy).
    let mut started = vec![false; writes.len()];
    for (index, write) in writes.iter().enumerate() {
        let start = WriteProgress::Writing {
            profile: profile.get(),
            table: write.table,
            key: write.key,
            packet: 0,
            of: write.packets,
            send: 1,
        };
        // A request to stop is recorded even when the cursor still sends the
        // write (the trigger, a slot that completes a binding).
        let mut asked = false;
        let step = cursor.before(index, write, cancel, &mut || {
            asked = progress(start).is_break();
            asked
        });
        cancel |= asked;
        match step {
            Step::Stop => {
                report.outcome = Outcome::Cancelled;
                break;
            }
            Step::Skip => {
                cancel = true;
                continue;
            }
            Step::Send => {}
        }
        let mut any_packet = false;
        let mut on_packet = |p: PacketProgress| {
            any_packet = true;
            let step = WriteProgress::Writing {
                profile: profile.get(),
                table: write.table,
                key: write.key,
                packet: p.packet,
                of: p.of,
                send: p.send,
            };
            if progress(step).is_break() {
                cancel = true;
            }
        };
        let trigger = match write.commit_trigger {
            Some(_) => match CommitTrigger::try_from(write.request.clone()) {
                Ok(trigger) => Some(trigger),
                Err(other) => {
                    let error = ErrorInfo {
                        kind: ErrorKind::InvalidRequest,
                        message: format!("{other} cannot be a keymap commit trigger"),
                    };
                    if let Some(item) = report.items.get_mut(index) {
                        item.result = ItemResult::Failed {
                            error: error.clone(),
                            mismatch: None,
                        };
                    }
                    report.fail(error);
                    break;
                }
            },
            None => None,
        };
        let checked = match &trigger {
            Some(trigger) => Checked::CommitTrigger(trigger),
            None => Checked::Table(&write.request),
        };
        let result = write_checked(
            &mut writer,
            profile,
            checked,
            &write.planned,
            &[],
            &mut on_packet,
        );
        if let Some(flag) = started.get_mut(index) {
            *flag = any_packet;
        }
        let sent = match result {
            Ok(result) => {
                let sent = result.was_sent();
                if sent && write.table == TableId::Socd {
                    socd_mark = Some(writer.session().socd_pushes());
                }
                if let Some(item) = report.items.get_mut(index) {
                    item.result = result;
                }
                sent
            }
            Err(TableFailure { error, mismatch }) => {
                if let Some(item) = report.items.get_mut(index) {
                    item.result = ItemResult::Failed {
                        error: error.clone(),
                        mismatch,
                    };
                }
                report.fail(error);
                if cursor.base_unsaved_on_stop() {
                    note_base_keymap_unsaved(&mut report);
                }
                break;
            }
        };
        cursor.after(index, write, sent);
    }
    if report.outcome == Outcome::Completed && cursor.cancelled_with_skips() {
        report.outcome = Outcome::Cancelled;
    }
    report.cancel_requested = cancel;
    if !writer.is_poisoned() && report.outcome != Outcome::Failed {
        if let Err(stop) = confirm_profile(&mut writer, profile) {
            let failure = TableFailure::from(stop);
            report.fail(failure.error);
        }
    }
    // Done only when the keyboard had time to save the last write (also
    // after a failure or a cancellation: something may have been written).
    // The wait ends by reading the active profile again: a switch made on
    // the keyboard during it (reported or not) may have discarded the
    // writes. After a switch already seen, nothing is left to wait for.
    if !writer.is_poisoned() {
        let waited = writer.wait_for_commit_observed(&mut |wait| {
            let _ = progress(WriteProgress::waiting(wait));
        });
        match waited {
            Ok(wait) => report.waited_for_commit_ms = wait.map(|w| w.wait_ms),
            Err(e) => report.fail(ErrorInfo::new(e.kind(), &e)),
        }
    }
    // Verified in the keyboard's working copy only: after a profile change
    // or a lost connection before the save they may be gone.
    if writer.commit_interrupted() && writer.session().last_write_profile() == Some(profile.get()) {
        for item in &mut report.items {
            let result = std::mem::replace(&mut item.result, ItemResult::NotAttempted);
            item.result = result.into_maybe_lost();
        }
    }
    if report.outcome == Outcome::Completed
        && !writer.is_poisoned()
        && needs_check(run.persistence, writes, &report.items)
    {
        if let Err(error) = persistence_check(
            &mut writer,
            profile,
            writes,
            &mut report,
            socd_mark,
            &mut progress,
        ) {
            report.fail(error);
        }
    }
    // As last read or reported (a profile report of a switch included).
    report.active_profile_now = writer.known_profile().map(ProfileId::get);
    report.foreign_activity = writer.foreign_activity_seen();
    if let (false, Some(before)) = (report.is_success(), run.group_before) {
        // Every write whose packets (some or all) went out counts as
        // applied: a key that may refer to an empty slot is reported.
        let mut after = before.clone();
        for ((write, item), started) in writes.iter().zip(&report.items).zip(&started) {
            if item.result.was_sent() || *started {
                apply_to_view(&mut after, &write.request);
            }
        }
        report.dangling = dangling_after(profile.get(), before, &after);
    }
    report
}

/// A stop between the base keymap and its Fn-keymap write: the base keymap
/// becomes may-be-lost with the warning that the keyboard will not keep it.
fn note_base_keymap_unsaved(report: &mut EditReport) {
    let base = TableId::Keymap { layer: Layer::Base };
    for item in report.items.iter_mut().filter(|i| i.table == base) {
        if let ItemResult::Written { warnings, .. } = &mut item.result {
            warnings.push(crate::writer::WriteWarning::BaseKeymapNotSaved);
        }
        let result = std::mem::replace(&mut item.result, ItemResult::NotAttempted);
        item.result = result.into_maybe_lost();
    }
}

/// The tables the persistence check of an edit will read again when every
/// planned write is made (`writes`: a plan's writes, in order; `mode`: the
/// check mode, `Always` when the plan forces the check): with
/// [`PersistenceCheckMode::Auto`] the written tables that hardware tests
/// have not shown to be kept after the settle, with
/// [`PersistenceCheckMode::Always`] every written table, with
/// [`PersistenceCheckMode::Never`] none. Empty: the edit will not switch
/// profiles (for a plan text that says so before the user answers). An
/// unchanged Fn keymap re-sent as the commit trigger is never checked.
pub fn persistence_check_tables(
    writes: &[PlannedEditWrite],
    mode: PersistenceCheckMode,
) -> Vec<TableId> {
    let fn_table = TableId::Keymap { layer: Layer::Fn };
    let mut tables: Vec<TableId> = Vec::new();
    for (index, write) in writes.iter().enumerate() {
        if write.resent_unchanged {
            continue;
        }
        let checked = match mode {
            PersistenceCheckMode::Never => false,
            PersistenceCheckMode::Always => true,
            PersistenceCheckMode::Auto => {
                let fn_before = writes.iter().take(index).any(|w| w.table == fn_table);
                !persistence_confirmed(write.table, fn_before)
            }
        };
        if checked && !tables.contains(&write.table) {
            tables.push(write.table);
        }
    }
    tables
}

/// Whether the persistence check runs: `Always`, or `Auto` and a written
/// table is not known to be kept after the settle.
fn needs_check(
    mode: PersistenceCheckMode,
    writes: &[PlannedEditWrite],
    items: &[ItemReport],
) -> bool {
    let written = || {
        writes.iter().zip(items).enumerate().filter(|(_, (w, i))| {
            !w.resent_unchanged && matches!(i.result, ItemResult::Written { .. })
        })
    };
    match mode {
        PersistenceCheckMode::Never => false,
        PersistenceCheckMode::Always => written().next().is_some(),
        PersistenceCheckMode::Auto => written().any(|(index, (write, _))| {
            !persistence_confirmed(write.table, fn_written_before(writes, items, index))
        }),
    }
}

/// Whether a complete Fn keymap was written before write `index`.
fn fn_written_before(writes: &[PlannedEditWrite], items: &[ItemReport], index: usize) -> bool {
    writes.iter().zip(items).take(index).any(|(w, i)| {
        w.table == (TableId::Keymap { layer: Layer::Fn })
            && matches!(i.result, ItemResult::Written { .. })
    })
}

/// The persistence check of an edit: switch to the lowest other profile and
/// back, then read every written table again (an unchanged Fn keymap
/// re-sent as the commit trigger is not counted). A table that no longer
/// holds what was written fails the edit, except a SOCD power switched on
/// the keyboard since the write (`socd_mark`), which is not checked.
fn persistence_check<T: WriteTransport>(
    writer: &mut crate::writer::Writer<'_, T>,
    profile: ProfileId,
    writes: &[PlannedEditWrite],
    report: &mut EditReport,
    socd_mark: Option<u64>,
    progress: &mut impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> Result<(), ErrorInfo> {
    let failure = |e: crate::error::WriteError| ErrorInfo::new(e.kind(), &e);
    let away = ProfileId::ALL
        .into_iter()
        .find(|p| *p != profile)
        .unwrap_or(profile);
    let _ = progress(WriteProgress::CheckingPersistence {
        profile: profile.get(),
    });
    let mut check = PersistenceCheck {
        profiles: Vec::new(),
        persisted: 0,
        lost: 0,
        not_checked: 0,
    };
    let reloaded = reload_profile(writer, profile, away, &mut |wait| {
        let _ = progress(WriteProgress::waiting(wait));
    });
    if let Err(e) = reloaded {
        // Back to the edited profile if the switch away went through.
        if !writer.is_poisoned() && writer.known_profile() != Some(profile) {
            let _ = writer.select_profile(profile);
        }
        return Err(failure(e));
    }
    check.profiles.push(profile.get());
    for (write, item) in writes.iter().zip(report.items.iter_mut()) {
        if write.resent_unchanged {
            continue;
        }
        match recheck(
            writer,
            &write.request,
            &mut item.result,
            &|_| false,
            socd_mark,
        ) {
            Ok(CheckOutcome::Persisted) => check.persisted += 1,
            Ok(CheckOutcome::Lost) => check.lost += 1,
            Ok(CheckOutcome::NotChecked) => {
                if matches!(item.result, ItemResult::Written { .. }) {
                    check.not_checked += 1;
                }
            }
            Err(e) => {
                report.persistence = Some(check);
                return Err(failure(e));
            }
        }
    }
    let lost = check.lost;
    report.persistence = Some(check);
    if lost > 0 {
        return Err(ErrorInfo {
            kind: ErrorKind::NotPersisted,
            message: format!(
                "{lost} table(s) written and verified, but read again after switching to profile \
                 {away} and back they no longer held what was written: the keyboard did not keep \
                 them"
            ),
        });
    }
    Ok(())
}
