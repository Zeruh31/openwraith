//! Keymap-group edits: key assignments on both layers, macros and advanced
//! keys, written as one group in the vendor site's order by the library
//! (`plan_keymap_edit_on`, `apply_keymap_edit`).
//!
//! The Keys page shows the group of the last full read (`KeysCache`, no
//! traffic); a prepare plans the user's intent against exactly that view
//! (`based_on`), and the apply step reads the profile again for the safety
//! backup and refuses when any table the edit writes (both layers, the ten
//! macro slots, the advanced-key table) differs from what the plan was made
//! against (the drift check).

use std::sync::Arc;

use openwraith_hid::protocol::write::{Dks, DksRow, DksStep, KeyMotion, MacroStep, ModTap, Toggle};
use openwraith_hid::protocol::{
    AdvancedKeyDef, AdvancedSlotId, BuiltinAck, Combo, EntryKind, HidUsage, KeyAssignment, KeyPos,
    KeymapReference, Layer, MacroPlay, MacroProgram, MacroSlotId, MediaKey, Model, ReadRequest,
    W75,
};
use openwraith_hid::{
    Backup, CommitTriggerMode, EditError, EditOptions, KeymapChange, KeymapEdit, KeymapEditPlan,
    KeymapGroupState, PersistenceCheckMode, WriteOptions, apply_keymap_edit, plan_keymap_edit_on,
};

use super::consent;
use super::progress::CancelPolicy;
use super::snapshot::{ReadFail, ReadVia};
use crate::backend::DeviceBackend;
use crate::dto::{
    ActionValue, AdvancedDefView, AppError, AppErrorKind, DeviceSnapshot, DksStepView,
    EditReportView, KeyMotionView, KeymapChangeIntent, KeymapEditIntent, KeymapEditPreview,
    KeymapProblemView, KeysDisabled, LastReport, MacroPlayView, MacroProgramView, OpKind,
    PlanInvalidation, PlanToken, ProgressStep, ReadPurpose, SlotKindView,
};
use crate::map::keys::{
    SlotTargets, build_problem, keymap_error, keymap_preview, layer_of, mouse_action, refused,
};
use crate::map::{edit_report_view, key_ref_index, sat16, table_ref};
use crate::sink::ProgressSink;
use crate::worker::{PendingPlan, PlanKind, Worker, stale};

/// The most changes of one edit.
pub(crate) const MAX_CHANGES: usize = 200;

/// The reads of the tables a keymap-group edit writes (the drift check
/// compares exactly these).
fn group_reads() -> Vec<ReadRequest> {
    let mut reads: Vec<ReadRequest> = Layer::ALL.into_iter().map(ReadRequest::Keymap).collect();
    reads.extend(MacroSlotId::all().map(ReadRequest::Macro));
    reads.push(ReadRequest::AdvancedKeys);
    reads
}

/// The group tables' bytes in `backup` (its active profile).
pub(crate) fn group_tables(backup: &Backup) -> Vec<(ReadRequest, Option<Vec<u8>>)> {
    group_reads()
        .into_iter()
        .map(|read| {
            let bytes = backup.primary().response(read).map(|raw| {
                raw.table_bytes("keymap group")
                    .unwrap_or_else(|_| raw.payload())
            });
            (read, bytes)
        })
        .collect()
}

/// The keymap group of the last full read: what `get_keys_state` shows and
/// what keymap edits are planned against.
#[derive(Debug)]
pub(crate) struct KeysCache {
    /// The snapshot revision of that read (`based_on` of keymap intents).
    pub(crate) revision: u32,
    /// The connection it was read in.
    pub(crate) generation: u32,
    /// The active profile.
    pub(crate) profile: u8,
    /// When it was read.
    pub(crate) read_at: String,
    /// The keyboard's model (the W75 when unknown).
    pub(crate) model: &'static Model,
    /// The group, or why it cannot be edited.
    pub(crate) group: Result<KeymapGroupState, KeysDisabled>,
    /// The group tables' bytes (the drift check).
    pub(crate) tables: Vec<(ReadRequest, Option<Vec<u8>>)>,
}

impl KeysCache {
    /// The group of a fresh full read and the snapshot made from it.
    pub(crate) fn new(backup: &Backup, snapshot: &DeviceSnapshot) -> Self {
        let group = KeymapGroupState::from_snapshot(backup.primary()).map_err(|e| match e {
            EditError::UnsupportedCurrentTable { table, reason } => KeysDisabled::UnreadableTable {
                table: table_ref(table),
                message: crate::map::clean(&reason),
            },
            other => KeysDisabled::Unsupported {
                message: crate::map::clean(&other.to_string()),
            },
        });
        KeysCache {
            revision: snapshot.revision,
            generation: snapshot.generation,
            profile: snapshot.profile,
            read_at: snapshot.read_at.clone(),
            model: backup.primary().model().unwrap_or(&W75),
            group,
            tables: group_tables(backup),
        }
    }
}

/// A confirmed keymap plan waiting for its apply step.
pub(crate) struct KeymapPending {
    /// The library's plan.
    pub(crate) plan: KeymapEditPlan,
    /// The persistence check.
    pub(crate) mode: PersistenceCheckMode,
    /// The group tables the plan was made against (the drift check).
    pub(crate) basis: Vec<(ReadRequest, Option<Vec<u8>>)>,
}

// ----- the intent -----

/// A key of the layout.
fn key_pos(model: &Model, index: u8) -> Result<KeyPos, AppError> {
    KeyPos::from_index(index)
        .filter(|_| model.key(index).is_some())
        .ok_or_else(|| {
            refused(
                KeymapProblemView::NotAKey { index },
                format!("#{index} is not a key of this keyboard"),
            )
        })
}

fn macro_slot(slot: u8) -> Result<MacroSlotId, AppError> {
    MacroSlotId::new(slot).ok_or_else(|| {
        refused(
            KeymapProblemView::SlotOutOfRange {
                kind: SlotKindView::Macro,
                slot,
            },
            format!("there is no macro slot {slot}"),
        )
    })
}

fn advanced_slot(slot: u8) -> Result<AdvancedSlotId, AppError> {
    AdvancedSlotId::new(slot).ok_or_else(|| {
        refused(
            KeymapProblemView::SlotOutOfRange {
                kind: SlotKindView::Advanced,
                slot,
            },
            format!("there is no advanced-key slot {slot}"),
        )
    })
}

fn usage(value: u8) -> Result<HidUsage, AppError> {
    HidUsage::new(value).ok_or_else(|| {
        refused(
            KeymapProblemView::NotOffered {
                what: "HID usage".to_string(),
                value,
            },
            format!("HID usage {value:#04x} is not offered"),
        )
    })
}

fn build_error(error: &openwraith_hid::protocol::WriteBuildError) -> AppError {
    refused(build_problem(error), error.to_string())
}

/// An action as the library's assignment (validated atoms only).
pub(crate) fn assignment(action: &ActionValue) -> Result<KeyAssignment, AppError> {
    Ok(match action {
        ActionValue::Default => KeyAssignment::Default,
        ActionValue::Key { usage: u } => KeyAssignment::Key(usage(*u)?),
        ActionValue::Media { index } => {
            KeyAssignment::Media(MediaKey::new(*index).ok_or_else(|| {
                refused(
                    KeymapProblemView::NotOffered {
                        what: "multimedia index".to_string(),
                        value: *index,
                    },
                    format!("multimedia index {index:#04x} is not offered"),
                )
            })?)
        }
        ActionValue::Mouse { action } => KeyAssignment::Mouse(mouse_action(*action)),
        ActionValue::Combo { modifiers, key } => {
            let modifiers = modifiers
                .iter()
                .map(|m| usage(*m))
                .collect::<Result<Vec<_>, _>>()?;
            KeyAssignment::Combo(Combo::new(&modifiers, usage(*key)?).map_err(|e| build_error(&e))?)
        }
        ActionValue::Macro { slot } => KeyAssignment::Macro(macro_slot(*slot)?),
        ActionValue::Advanced { slot } => KeyAssignment::Advanced(advanced_slot(*slot)?),
    })
}

/// A macro as the library's validated program.
pub(crate) fn macro_program(view: &MacroProgramView) -> Result<MacroProgram, AppError> {
    let play = match view.play {
        MacroPlayView::Once => MacroPlay::Once,
        MacroPlayView::Times { count } => MacroPlay::Times(count),
        MacroPlayView::Toggle => MacroPlay::Toggle,
        MacroPlayView::WhileHeld => MacroPlay::WhileHeld,
    };
    let steps = view
        .steps
        .iter()
        .map(|step| {
            Ok(MacroStep {
                usage: usage(step.usage)?,
                motion: match step.motion {
                    KeyMotionView::Press => KeyMotion::Press,
                    KeyMotionView::Release => KeyMotion::Release,
                },
                delay_after_ms: step.delay_after_ms,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    MacroProgram::new(play, steps).map_err(|e| build_error(&e))
}

fn dks_step(step: DksStepView) -> DksStep {
    match step {
        DksStepView::None => DksStep::None,
        DksStepView::Tap => DksStep::Tap,
        DksStepView::Hold => DksStep::Hold,
        DksStepView::HoldAgain => DksStep::HoldAgain,
    }
}

/// An advanced key as the library's definition (its ranges are checked by
/// the plan against the keyboard's limits).
fn advanced_def(
    group: &KeymapGroupState,
    view: &AdvancedDefView,
) -> Result<AdvancedKeyDef, AppError> {
    let shape = || {
        refused(
            KeymapProblemView::DksShape,
            "a DKS has four rows of four stages",
        )
    };
    Ok(match view {
        AdvancedDefView::Dks { first_point, rows } => {
            if rows.len() != 4 {
                return Err(shape());
            }
            let mut out: [Option<DksRow>; 4] = [None; 4];
            for (slot, row) in out.iter_mut().zip(rows) {
                let Some(row) = row else {
                    continue;
                };
                let steps: [DksStepView; 4] = row.steps.clone().try_into().map_err(|_| shape())?;
                *slot = Some(DksRow {
                    key: usage(row.key)?,
                    steps: steps.map(dks_step),
                });
            }
            AdvancedKeyDef::Dks(Dks {
                first_point: *first_point,
                rows: out,
            })
        }
        AdvancedDefView::ModTap { tap, hold, hold_ms } => {
            let (min, max) = group.advanced_limits().map_or(
                (
                    group.model().write_ceilings.mt_threshold_min,
                    group.model().write_ceilings.mt_threshold_max,
                ),
                |limits| limits.mt_threshold_range(),
            );
            let threshold = u8::try_from(hold_ms / 10)
                .ok()
                .filter(|_| hold_ms % 10 == 0)
                .ok_or_else(|| {
                    refused(
                        KeymapProblemView::MtHoldTime {
                            value_ms: *hold_ms,
                            min_ms: u16::from(min) * 10,
                            max_ms: u16::from(max) * 10,
                        },
                        format!("a Mod-Tap hold time of {hold_ms} ms is not possible"),
                    )
                })?;
            AdvancedKeyDef::ModTap(ModTap {
                hold: usage(*hold)?,
                tap: usage(*tap)?,
                threshold_10ms: threshold,
            })
        }
        AdvancedDefView::Toggle { key } => AdvancedKeyDef::Toggle(Toggle { key: usage(*key)? }),
    })
}

/// A macro or advanced key bound with the slot left to the library: its
/// slot is found in the plan afterwards.
enum PendingBind {
    Macro(Layer, KeyPos, MacroProgram),
    Advanced(Layer, KeyPos, AdvancedKeyDef),
}

/// The library's edit of an intent, checked against the group the user saw.
struct EditOfIntent {
    edit: KeymapEdit,
    targets: SlotTargets,
    binds: Vec<PendingBind>,
}

/// The keys bound to `slot` (unbound by a delete).
fn bindings_of(
    group: &KeymapGroupState,
    macro_: Option<MacroSlotId>,
    advanced: Option<AdvancedSlotId>,
) -> Vec<(Layer, u8)> {
    group
        .references()
        .into_iter()
        .filter_map(|(layer, reference)| match reference {
            KeymapReference::Macro { index, slot } if Some(slot) == macro_ => Some((layer, index)),
            KeymapReference::Advanced { index, slot } if Some(slot) == advanced => {
                Some((layer, index))
            }
            _ => None,
        })
        .collect()
}

fn unbind_all(bound: Vec<(Layer, u8)>, changes: &mut Vec<KeymapChange>) -> Result<(), AppError> {
    for (layer, index) in bound {
        let pos = KeyPos::from_index(index).ok_or_else(|| {
            refused(
                KeymapProblemView::NotAKey { index },
                format!("#{index} is not a key position"),
            )
        })?;
        changes.push(KeymapChange::Unbind { layer, pos });
    }
    Ok(())
}

/// Refuses a new macro or advanced key without a slot on a key whose
/// current slot the same edit changes: the library reuses the slot only
/// that key runs for the new one, so the slot's drafted change and the new
/// one would overwrite each other (a delete or a clear of the slot is
/// fine: it comes first and frees the key).
fn check_new_slots(
    group: &KeymapGroupState,
    binds: &[PendingBind],
    targets: &SlotTargets,
) -> Result<(), AppError> {
    for bind in binds {
        let (layer, pos) = match bind {
            PendingBind::Macro(layer, pos, _) | PendingBind::Advanced(layer, pos, _) => {
                (*layer, *pos)
            }
        };
        let EntryKind::Assignable(now) = group.entry(layer, pos) else {
            continue;
        };
        let (slot, edited, only) = match (bind, now) {
            (PendingBind::Macro(..), KeyAssignment::Macro(slot)) => (
                crate::dto::TableRef::Macro { slot: slot.get() },
                matches!(targets.macros.get(&slot.get()), Some(Some(_))),
                bindings_of(group, Some(slot), None).len() == 1,
            ),
            (PendingBind::Advanced(..), KeyAssignment::Advanced(slot)) => (
                crate::dto::TableRef::AdvancedKey { slot: slot.get() },
                matches!(targets.advanced.get(&slot.get()), Some(Some(_))),
                bindings_of(group, None, Some(slot)).len() == 1,
            ),
            _ => continue,
        };
        if edited && only {
            return Err(refused(
                KeymapProblemView::NewSlotReplacesEdit {
                    layer: crate::map::layer_view(layer),
                    key: key_ref_index(pos.index()),
                    slot,
                },
                format!(
                    "the new one on {} would go into the slot it runs now, which this edit also changes",
                    group.model().key_name(pos.index())
                ),
            ));
        }
    }
    Ok(())
}

/// Checks an intent and expands it into the library's edit.
fn edit_of(group: &KeymapGroupState, intent: &KeymapEditIntent) -> Result<EditOfIntent, AppError> {
    let model = group.model();
    if intent.changes.is_empty() {
        return Err(refused(KeymapProblemView::NoChanges, "nothing to change"));
    }
    if intent.changes.len() > MAX_CHANGES || intent.builtin_acks.len() > MAX_CHANGES {
        return Err(refused(
            KeymapProblemView::TooManyChanges {
                max: sat16(MAX_CHANGES),
            },
            format!("at most {MAX_CHANGES} changes per edit"),
        ));
    }
    let mut changes = Vec::new();
    let mut targets = SlotTargets::default();
    let mut binds = Vec::new();
    // Deletes and clears first: a delete unbinds the keys the slot has now
    // and empties it, so a new macro or advanced key drafted on one of those
    // keys (or an assignment to it) must come after it. In the order made,
    // the library would reuse the slot being deleted for the new one and the
    // delete would then wipe it.
    let removes = |change: &&KeymapChangeIntent| {
        matches!(
            change,
            KeymapChangeIntent::DeleteMacro { .. }
                | KeymapChangeIntent::ClearMacro { .. }
                | KeymapChangeIntent::DeleteAdvanced { .. }
                | KeymapChangeIntent::ClearAdvanced { .. }
        )
    };
    let ordered = intent
        .changes
        .iter()
        .filter(removes)
        .chain(intent.changes.iter().filter(|c| !removes(c)));
    for change in ordered {
        match change {
            KeymapChangeIntent::Assign { layer, key, action } => {
                changes.push(KeymapChange::Assign {
                    layer: layer_of(*layer),
                    pos: key_pos(model, *key)?,
                    assignment: assignment(action)?,
                });
            }
            KeymapChangeIntent::Reset { layer, key } => {
                changes.push(KeymapChange::Unbind {
                    layer: layer_of(*layer),
                    pos: key_pos(model, *key)?,
                });
            }
            KeymapChangeIntent::Swap {
                layer,
                first,
                second,
            } => {
                let layer = layer_of(*layer);
                let (a, b) = (key_pos(model, *first)?, key_pos(model, *second)?);
                let writable = |pos: KeyPos| match group.entry(layer, pos) {
                    EntryKind::Assignable(assignment) => Ok(assignment),
                    EntryKind::Preserved(_) => Err(refused(
                        KeymapProblemView::SwapNotWritable {
                            layer: crate::map::layer_view(layer),
                            key: key_ref_index(pos.index()),
                        },
                        format!(
                            "the entry of {} is kept as read and cannot be moved",
                            model.key_name(pos.index())
                        ),
                    )),
                };
                let (on_a, on_b) = (writable(a)?, writable(b)?);
                changes.push(KeymapChange::Assign {
                    layer,
                    pos: a,
                    assignment: on_b,
                });
                changes.push(KeymapChange::Assign {
                    layer,
                    pos: b,
                    assignment: on_a,
                });
            }
            KeymapChangeIntent::BindMacro {
                layer,
                key,
                program,
                slot,
            } => {
                let (layer, pos) = (layer_of(*layer), key_pos(model, *key)?);
                let program = macro_program(program)?;
                let slot = slot.map(macro_slot).transpose()?;
                match slot {
                    Some(slot) => {
                        targets.macros.insert(slot.get(), Some(program.clone()));
                    }
                    None => binds.push(PendingBind::Macro(layer, pos, program.clone())),
                }
                changes.push(KeymapChange::BindMacro {
                    layer,
                    pos,
                    program,
                    slot,
                });
            }
            KeymapChangeIntent::SetMacro { slot, program } => {
                let slot = macro_slot(*slot)?;
                let program = macro_program(program)?;
                targets.macros.insert(slot.get(), Some(program.clone()));
                changes.push(KeymapChange::SetMacroSlot {
                    slot,
                    program: Some(program),
                });
            }
            KeymapChangeIntent::ClearMacro { slot } => {
                let slot = macro_slot(*slot)?;
                targets.macros.insert(slot.get(), None);
                changes.push(KeymapChange::SetMacroSlot {
                    slot,
                    program: None,
                });
            }
            KeymapChangeIntent::DeleteMacro { slot } => {
                let slot = macro_slot(*slot)?;
                unbind_all(bindings_of(group, Some(slot), None), &mut changes)?;
                targets.macros.insert(slot.get(), None);
                changes.push(KeymapChange::SetMacroSlot {
                    slot,
                    program: None,
                });
            }
            KeymapChangeIntent::BindAdvanced {
                layer,
                key,
                def,
                slot,
            } => {
                let (layer, pos) = (layer_of(*layer), key_pos(model, *key)?);
                let def = advanced_def(group, def)?;
                let slot = slot.map(advanced_slot).transpose()?;
                match slot {
                    Some(slot) => {
                        targets.advanced.insert(slot.get(), Some(def));
                    }
                    None => binds.push(PendingBind::Advanced(layer, pos, def)),
                }
                changes.push(KeymapChange::BindAdvanced {
                    layer,
                    pos,
                    def,
                    slot,
                });
            }
            KeymapChangeIntent::SetAdvanced { slot, def } => {
                let slot = advanced_slot(*slot)?;
                let def = advanced_def(group, def)?;
                targets.advanced.insert(slot.get(), Some(def));
                changes.push(KeymapChange::SetAdvancedSlot {
                    slot,
                    def: Some(def),
                });
            }
            KeymapChangeIntent::ClearAdvanced { slot } => {
                let slot = advanced_slot(*slot)?;
                targets.advanced.insert(slot.get(), None);
                changes.push(KeymapChange::SetAdvancedSlot { slot, def: None });
            }
            KeymapChangeIntent::DeleteAdvanced { slot } => {
                let slot = advanced_slot(*slot)?;
                unbind_all(bindings_of(group, None, Some(slot)), &mut changes)?;
                targets.advanced.insert(slot.get(), None);
                changes.push(KeymapChange::SetAdvancedSlot { slot, def: None });
            }
        }
    }
    check_new_slots(group, &binds, &targets)?;
    let builtin_acks = intent
        .builtin_acks
        .iter()
        .map(|ack| {
            Ok(BuiltinAck {
                pos: key_pos(model, ack.key)?,
                critical: ack.critical,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(EditOfIntent {
        edit: KeymapEdit {
            changes,
            builtin_acks,
            keep_orphans: intent.keep_orphans,
            commit_trigger: CommitTriggerMode::Auto,
            allow_not_undoable: intent.allow_not_undoable,
        },
        targets,
        binds,
    })
}

/// The slots the library chose for bindings without a slot: the key's entry
/// after the edit.
fn resolve_binds(
    group: &KeymapGroupState,
    plan: &KeymapEditPlan,
    binds: Vec<PendingBind>,
    targets: &mut SlotTargets,
) {
    let after = |layer: Layer, pos: KeyPos| {
        plan.entry_changes()
            .iter()
            .rev()
            .find(|c| c.layer == layer && c.index == pos.index())
            .map_or_else(|| group.entry(layer, pos), |c| c.after)
    };
    for bind in binds {
        match bind {
            PendingBind::Macro(layer, pos, program) => {
                if let EntryKind::Assignable(KeyAssignment::Macro(slot)) = after(layer, pos) {
                    targets.macros.insert(slot.get(), Some(program));
                }
            }
            PendingBind::Advanced(layer, pos, def) => {
                if let EntryKind::Assignable(KeyAssignment::Advanced(slot)) = after(layer, pos) {
                    targets.advanced.insert(slot.get(), Some(def));
                }
            }
        }
    }
}

impl<B: DeviceBackend> Worker<B> {
    /// `prepare_keymap_edit`: plans the intent against the keymap group the
    /// user saw (no traffic) and stores the plan.
    pub(crate) fn prepare_keymap_edit(
        &mut self,
        intent: &KeymapEditIntent,
    ) -> Result<KeymapEditPreview, AppError> {
        self.check_writable()?;
        let cache = self.shared.keys().ok_or_else(Self::not_connected)?;
        let shown = self.snapshot.as_ref().map(|s| (s.generation, s.profile));
        if intent.based_on != cache.revision || shown != Some((cache.generation, cache.profile)) {
            return Err(stale(PlanInvalidation::SnapshotChanged));
        }
        let group = match &cache.group {
            Ok(group) => group,
            Err(disabled) => {
                return Err(AppError::new(
                    AppErrorKind::InvalidRequest,
                    format!(
                        "the keymap group cannot be edited ({disabled:?}); nothing was written"
                    ),
                ));
            }
        };
        let EditOfIntent {
            edit,
            mut targets,
            binds,
        } = edit_of(group, intent)?;
        let mode = if intent.check_persistence {
            PersistenceCheckMode::Always
        } else {
            PersistenceCheckMode::Auto
        };
        self.run_op(
            OpKind::KeymapEdit,
            Arc::new(crate::sink::NoProgress),
            CancelPolicy::Never,
            |w, _| {
                let plan = plan_keymap_edit_on(group, &edit).map_err(|e| keymap_error(&e))?;
                resolve_binds(group, &plan, binds, &mut targets);
                let preview =
                    keymap_preview(&plan, group, &targets, mode, intent.check_persistence);
                let token: Option<PlanToken> = plan.has_changes().then(|| {
                    w.store_plan(
                        PlanKind::KeymapEdit,
                        PendingPlan::KeymapEdit(Box::new(KeymapPending {
                            plan,
                            mode,
                            basis: cache.tables.clone(),
                        })),
                    )
                });
                Ok(KeymapEditPreview { token, ..preview })
            },
        )
    }

    /// `apply_keymap_edit`: the confirmed plan, after the drift check and a
    /// verified safety backup. Stop follows the keymap group's rules (the
    /// library's cursor): writes that keep the group consistent, such as
    /// the macro slots re-sent after slot 0, still go out.
    pub(crate) fn apply_keymap_edit(
        &mut self,
        token: &PlanToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        let PendingPlan::KeymapEdit(pending) = self.take_plan(token, PlanKind::KeymapEdit)? else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        self.check_writable()?;
        self.run_op(
            OpKind::KeymapEdit,
            sink,
            CancelPolicy::Restore,
            |w, progress| {
                w.ensure_quiet(progress)?;
                let fresh = w
                    .full_read(ReadVia::Op(progress, ReadPurpose::SafetyBackup))
                    .map_err(ReadFail::into_error)?;
                // Drift check: the plan must still describe the keyboard,
                // table by table.
                let profile = pending.plan.profile().get();
                if fresh.active_profile != Some(profile) {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ProfileChanged));
                }
                if group_tables(&fresh) != pending.basis {
                    w.set_snapshot(&fresh);
                    return Err(stale(PlanInvalidation::ValuesChanged));
                }
                let (saved, file) = w.save_safety(&fresh, "set-keys", progress)?;
                w.write_journal(OpKind::KeymapEdit, vec![profile], saved.path())?;
                progress.items = sat16(pending.plan.writes().len());
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let options = EditOptions {
                    write: WriteOptions::default(),
                    persistence: pending.mode,
                };
                let report = apply_keymap_edit(
                    session,
                    &pending.plan,
                    &saved,
                    consent::application(),
                    &options,
                    |p| progress.write(p),
                );
                w.remove_journal();
                w.own_write_done(super::recovery::completed_cleanly(
                    report.outcome,
                    report.lost(),
                    report.maybe_lost(),
                ));
                w.active_profile = report.active_profile_now;
                // No read of the result after a reset, a lost connection or
                // when another program was heard (it acts on our replies).
                let fatal = w.foreign_heard()
                    || w.session
                        .as_ref()
                        .is_none_or(|s| s.keyboard_reset_seen() || s.connection_lost());
                if w.foreign_heard() {
                    w.stale = true;
                } else if !fatal {
                    progress.step(ProgressStep::RefreshingView);
                    match w.full_read(ReadVia::Op(progress, ReadPurpose::Snapshot)) {
                        Ok(backup) => {
                            w.set_snapshot(&backup);
                        }
                        Err(ReadFail::Cancelled) => w.reread = true,
                        Err(fail) => w.read_failed(&fail),
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

#[cfg(test)]
mod tests {
    use openwraith_hid::protocol::TableId;

    use super::*;

    #[test]
    fn the_drift_check_covers_both_layers_every_macro_slot_and_the_advanced_table() {
        let reads = group_reads();
        assert_eq!(reads.len(), 13);
        assert!(reads.contains(&ReadRequest::Keymap(Layer::Base)));
        assert!(reads.contains(&ReadRequest::Keymap(Layer::Fn)));
        assert!(reads.contains(&ReadRequest::AdvancedKeys));
        assert_eq!(
            reads
                .iter()
                .filter(|r| matches!(r, ReadRequest::Macro(_)))
                .count(),
            10
        );
        // Every table the library writes for a keymap group is among them.
        for table in [
            TableId::Keymap { layer: Layer::Base },
            TableId::Keymap { layer: Layer::Fn },
        ] {
            assert!(reads.iter().any(|r| match (r, table) {
                (ReadRequest::Keymap(l), TableId::Keymap { layer }) => *l == layer,
                _ => false,
            }));
        }
    }
}
