//! Keymap-group edits: key assignments on the base and Fn layers, macros and
//! advanced keys (DKS, Mod-Tap, Toggle), as one keymap-group write.
//!
//! # What can be written
//!
//! Entries are built from the closed [`KeyAssignment`] (keyboard usages,
//! multimedia, mouse, combinations, macro and advanced-key bindings: the
//! CONFIRMED categories only). Every other entry the keyboard holds (the Fn
//! key's `01`, lighting, function, gamepad, unknown bytes) is kept byte for
//! byte unless the user replaces it. The knob (15-21) and the Fn key (121)
//! are never changed, on either layer; Fn-layer keys the keyboard locks
//! (`0E`) are refused; an Fn-layer entry that hides a built-in Fn function
//! needs the user's acknowledgement ([`BuiltinAck`]), a critical one (Fn +
//! 1..4, Fn + Space) a separate one ([`check_placement`]).
//!
//! # Slots
//!
//! One slot per key, as the vendor site does: binding a macro or an
//! advanced key reuses the slot only this key refers to, or takes the lowest
//! slot that is empty and unreferenced in both layers
//! (`getAdvIndex`, `wmIndex.js:3790-3803`). A bound slot whose content
//! changes is rewritten in place, as the site does on every Apply. A slot
//! whose last binding the edit removes is cleared in the same group unless
//! [`KeymapEdit::keep_orphans`] (the keyboard keeps unbound slots, c16).
//! Existing malformed or dangling entries are kept byte for byte (a
//! warning says so).
//!
//! Every edit must be undoable by a restore of its safety backup, which
//! writes only slots in a form OpenWraith can write
//! ([`AdvancedSlotData::from_read_slot`], [`MacroData::from_response`]). A
//! slot whose content is in another form (made by the vendor site, say a
//! Toggle of a media key) is therefore never cleared as an orphan and never
//! reused for a new binding of its key (a free slot is taken and the old
//! one kept, [`EditWarning::OrphanSlotKept`]); overwriting or clearing it
//! explicitly needs [`KeymapEdit::allow_not_undoable`].
//!
//! # Order
//!
//! Changed macro slots ascending, the base keymap, the Fn keymap, changed
//! advanced-key slots ascending (the site's order, spec 4.8.2). The Fn
//! keymap follows every base keymap (re-sent unchanged if need be, the
//! keyboard's commit trigger) and sits between the macro and the
//! advanced-key slots when only slots change. With
//! [`CommitTriggerMode::OmitForSlotsOnly`] (hardware tests only) a
//! slot-only group is written without it, and the persistence check is
//! forced on.

use std::ops::ControlFlow;

use openwraith_protocol::write::ADVANCED_SLOT_LEN;
use openwraith_protocol::{
    ActiveProfile, AdvancedKeyDef, AdvancedLimits, AdvancedSlotData, AdvancedSlotId, BuiltinAck,
    DeviceInfo, EntryKind, FnLockedKeys, FromResponse, HexBytes, KeyAssignment, KeyMask, KeyPos,
    KeymapData, KeymapReference, Layer, MacroData, MacroProgram, MacroSlotId, Model, ProfileId,
    RawResponse, ReadRequest, Status, TableId, TriggerGlobal, WriteRequest, check_placement,
    model_for_product,
};
use serde::Serialize;

use super::{
    EditError, EditOptions, EditReport, EditRun, EditWarning, PersistenceCheckMode,
    PlannedEditWrite, run_edit,
};
use crate::backup::SnapshotView;
use crate::group::{GroupView, SlotRef, layer_index};
use crate::restore::{CommitTriggerReason, WriteProgress, state_of};
use crate::safety::SavedBackup;
use crate::session::Session;
use crate::transport::{Transport, WriteTransport};
use crate::writer::WriteConsent;

/// One change of a keymap-group edit, applied in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum KeymapChange {
    /// An entry: default, key, media, mouse, combination, or a binding to
    /// an existing non-empty slot no other key refers to.
    Assign {
        /// The layer.
        layer: Layer,
        /// The key.
        pos: KeyPos,
        /// What it does.
        assignment: KeyAssignment,
    },
    /// A macro and its binding. `slot` `None`: the slot only this key
    /// refers to, else the lowest free one.
    BindMacro {
        /// The layer.
        layer: Layer,
        /// The key.
        pos: KeyPos,
        /// The macro.
        program: MacroProgram,
        /// The slot, if chosen.
        slot: Option<MacroSlotId>,
    },
    /// An advanced key and its binding. `slot` `None`: the slot only this
    /// key refers to, else the lowest free one.
    BindAdvanced {
        /// The layer.
        layer: Layer,
        /// The key.
        pos: KeyPos,
        /// The advanced key.
        def: AdvancedKeyDef,
        /// The slot, if chosen.
        slot: Option<AdvancedSlotId>,
    },
    /// A macro slot's content only (`None` = clear; refused while a key
    /// refers to it after the edit).
    SetMacroSlot {
        /// The slot.
        slot: MacroSlotId,
        /// The macro, or `None` to clear it.
        program: Option<MacroProgram>,
    },
    /// An advanced-key slot's content only (`None` = clear; refused while a
    /// key refers to it after the edit).
    SetAdvancedSlot {
        /// The slot.
        slot: AdvancedSlotId,
        /// The advanced key, or `None` to clear it.
        def: Option<AdvancedKeyDef>,
    },
    /// The entry back to default (`00 00 00 00`).
    Unbind {
        /// The layer.
        layer: Layer,
        /// The key.
        pos: KeyPos,
    },
}

/// Whether the Fn keymap is written as the commit trigger of a slot-only
/// group.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitTriggerMode {
    /// The site's order: the unchanged Fn keymap between the macro and the
    /// advanced-key slots when only slots change.
    #[default]
    Auto,
    /// Hardware tests only: a slot-only group without the Fn keymap (does
    /// the keyboard keep macros or a lone advanced-key slot without it?).
    /// Refused when a layer changes; forces the persistence check.
    OmitForSlotsOnly,
}

/// A keymap-group edit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct KeymapEdit {
    /// The changes, applied in order.
    pub changes: Vec<KeymapChange>,
    /// The user's acknowledgements for Fn-layer entries that hide a
    /// built-in Fn function.
    pub builtin_acks: Vec<BuiltinAck>,
    /// Keep the content of a slot whose last binding the edit removes
    /// (default: clear it in the same group).
    pub keep_orphans: bool,
    /// Whether a slot-only group gets the Fn keymap as commit trigger.
    pub commit_trigger: CommitTriggerMode,
    /// Overwrite or clear a slot whose current content the undo (a restore
    /// of the safety backup) cannot write back ([`EditWarning::NotUndoable`]);
    /// without it such a write is refused. Slots that are only left unbound
    /// are never cleared and never reused for another key (see the
    /// [module documentation](self)).
    pub allow_not_undoable: bool,
}

/// A macro slot as an editor sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "slot", rename_all = "snake_case")]
pub enum MacroSlotView {
    /// No macro (all `00` or all `FF`).
    Empty,
    /// A macro OpenWraith can show and edit.
    Program {
        /// The macro.
        program: MacroProgram,
    },
    /// Content in another form (kept as read unless replaced).
    Raw {
        /// The 256 slot bytes.
        bytes: HexBytes,
    },
}

/// An advanced-key slot as an editor sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "slot", rename_all = "snake_case")]
pub enum AdvancedSlotView {
    /// No advanced key (all zero).
    Empty,
    /// A DKS, Mod-Tap or Toggle OpenWraith can show and edit.
    Def {
        /// The advanced key.
        def: AdvancedKeyDef,
    },
    /// Another type or form (kept as read unless replaced).
    Raw {
        /// The 40 slot bytes.
        bytes: HexBytes,
    },
}

/// The keymap group of the active profile, read-only: both layers, the ten
/// macro slots, the forty advanced-key slots, the Fn-locked keys and the
/// DKS limits.
#[derive(Debug, Clone)]
pub struct KeymapGroupState {
    profile: ProfileId,
    product: Option<String>,
    model: &'static Model,
    fn_locked: KeyMask,
    limits: Option<AdvancedLimits>,
    unit_um: Option<u32>,
    layers: [KeymapData; 2],
    macros: Vec<Vec<u8>>,
    advanced: Vec<u8>,
    raws: Vec<RawResponse>,
}

/// The reads of [`KeymapGroupState::read`], in order.
fn group_reads() -> Vec<ReadRequest> {
    let mut reads = vec![
        ReadRequest::DeviceInfo,
        ReadRequest::ActiveProfile,
        ReadRequest::FnLockedKeys,
        ReadRequest::Status,
        ReadRequest::TriggerGlobal,
        ReadRequest::Keymap(Layer::Base),
        ReadRequest::Keymap(Layer::Fn),
    ];
    reads.extend(MacroSlotId::all().map(ReadRequest::Macro));
    reads.push(ReadRequest::AdvancedKeys);
    reads
}

/// The table name of a read, for errors.
fn table_of(read: ReadRequest) -> TableId {
    match read {
        ReadRequest::Keymap(layer) => TableId::Keymap { layer },
        ReadRequest::Macro(slot) => TableId::Macro { slot },
        ReadRequest::AdvancedKeys => match AdvancedSlotId::new(0) {
            Some(slot) => TableId::AdvancedKey { slot },
            None => TableId::Profile,
        },
        _ => TableId::Profile,
    }
}

impl KeymapGroupState {
    /// Reads the group of the active profile (read-only): `0D`, `21/11`,
    /// `0E`, `01`, `21/04`, `18 80`, `18 82`, `19 80`-`89`, `22 80`.
    pub fn read<T: Transport>(session: &mut Session<T>) -> Result<Self, EditError> {
        let mut raws = Vec::new();
        for read in group_reads() {
            raws.push(session.request(read)?);
        }
        Self::from_raws(raws)
    }

    /// The group of a backup snapshot (for a user interface that already
    /// read the profile).
    pub fn from_snapshot(view: SnapshotView<'_>) -> Result<Self, EditError> {
        let raws = group_reads()
            .into_iter()
            .map(|read| {
                view.response(read)
                    .cloned()
                    .ok_or_else(|| EditError::UnsupportedCurrentTable {
                        table: table_of(read),
                        reason: format!("the snapshot has no reply to {read}"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_raws(raws)
    }

    fn from_raws(raws: Vec<RawResponse>) -> Result<Self, EditError> {
        let find = |read: ReadRequest| {
            raws.iter().find(|r| r.request() == read).ok_or_else(|| {
                EditError::UnsupportedCurrentTable {
                    table: table_of(read),
                    reason: format!("no reply to {read}"),
                }
            })
        };
        let decode = |read: ReadRequest, e: openwraith_protocol::DecodeError| {
            EditError::UnsupportedCurrentTable {
                table: table_of(read),
                reason: e.to_string(),
            }
        };
        let info = DeviceInfo::from_response(find(ReadRequest::DeviceInfo)?)
            .map_err(|e| decode(ReadRequest::DeviceInfo, e))?;
        let model = info
            .product
            .as_deref()
            .and_then(model_for_product)
            .ok_or_else(|| EditError::Invalid {
                reason: format!(
                    "unsupported keyboard (product {})",
                    info.product.as_deref().unwrap_or("unknown")
                ),
            })?;
        let active = ActiveProfile::from_response(find(ReadRequest::ActiveProfile)?)
            .map_err(|e| decode(ReadRequest::ActiveProfile, e))?;
        let profile = ProfileId::new(active.profile).ok_or_else(|| EditError::Invalid {
            reason: format!(
                "the keyboard reports active profile {}; only profiles 1-4 are supported",
                active.profile
            ),
        })?;
        let fn_locked = FnLockedKeys::from_response(find(ReadRequest::FnLockedKeys)?)
            .map_err(|e| decode(ReadRequest::FnLockedKeys, e))?
            .mask;
        let status = Status::from_response(find(ReadRequest::Status)?).ok();
        let global = TriggerGlobal::from_response(find(ReadRequest::TriggerGlobal)?).ok();
        let unit_um = match (&status, &global) {
            (Some(status), Some(global)) => {
                Some(global.unit_micrometres(status.flags_b.high_precision))
            }
            _ => None,
        };
        let limits = match (&global, unit_um) {
            (Some(global), Some(unit)) => AdvancedLimits::new(model, global, unit).ok(),
            _ => None,
        };
        let layer = |layer: Layer| -> Result<KeymapData, EditError> {
            let read = ReadRequest::Keymap(layer);
            KeymapData::from_response(find(read)?)
                .map(|(_, data)| data)
                .map_err(|e| EditError::UnsupportedCurrentTable {
                    table: table_of(read),
                    reason: e.to_string(),
                })
        };
        let layers = [layer(Layer::Base)?, layer(Layer::Fn)?];
        let mut macros = Vec::new();
        for slot in MacroSlotId::all() {
            let raw = find(ReadRequest::Macro(slot))?;
            macros.push(raw.table_bytes("macro slot").map_err(|e| {
                EditError::UnsupportedCurrentTable {
                    table: TableId::Macro { slot },
                    reason: e.to_string(),
                }
            })?);
        }
        let advanced = find(ReadRequest::AdvancedKeys)?
            .table_bytes("advanced keys")
            .map_err(|e| EditError::UnsupportedCurrentTable {
                table: table_of(ReadRequest::AdvancedKeys),
                reason: e.to_string(),
            })?;
        if advanced.len() != ADVANCED_SLOT_LEN * 40 {
            return Err(EditError::UnsupportedCurrentTable {
                table: table_of(ReadRequest::AdvancedKeys),
                reason: format!("{} bytes instead of 1600", advanced.len()),
            });
        }
        Ok(KeymapGroupState {
            profile,
            product: info.product,
            model,
            fn_locked,
            limits,
            unit_um,
            layers,
            macros,
            advanced,
            raws,
        })
    }

    /// The active profile the group was read from.
    pub fn profile(&self) -> ProfileId {
        self.profile
    }

    /// The keyboard's model.
    pub fn model(&self) -> &'static Model {
        self.model
    }

    /// The keys the keyboard locks on the Fn layer (`0E`).
    pub fn fn_locked(&self) -> &KeyMask {
        &self.fn_locked
    }

    /// The DKS and Mod-Tap limits, when the keyboard's distance unit is
    /// 0.02 mm.
    pub fn advanced_limits(&self) -> Option<&AdvancedLimits> {
        self.limits.as_ref()
    }

    /// The layer's table.
    pub fn layer(&self, layer: Layer) -> &KeymapData {
        &self.layers[layer_index(layer)]
    }

    /// The entry of the key at `pos` on `layer`, as an editor sees it.
    pub fn entry(&self, layer: Layer, pos: KeyPos) -> EntryKind {
        KeyAssignment::classify(self.model, pos, self.layer(layer).entry(pos))
    }

    /// The macro slot `slot`.
    pub fn macro_program(&self, slot: MacroSlotId) -> MacroSlotView {
        let bytes = self.macro_bytes(slot);
        if is_empty_macro(bytes) {
            return MacroSlotView::Empty;
        }
        let program = self
            .raws
            .iter()
            .find(|r| r.request() == ReadRequest::Macro(slot))
            .and_then(|raw| MacroData::from_response(raw).ok())
            .and_then(|(_, data)| match data {
                MacroData::Content(content) => MacroProgram::decode(&content),
                MacroData::Empty => None,
            });
        match program {
            Some(program) => MacroSlotView::Program { program },
            None => MacroSlotView::Raw {
                bytes: HexBytes(bytes.to_vec()),
            },
        }
    }

    /// The advanced-key slot `slot`.
    pub fn advanced(&self, slot: AdvancedSlotId) -> AdvancedSlotView {
        let bytes = self.advanced_bytes(slot);
        match AdvancedSlotData::from_read_slot(bytes) {
            Ok(None) => AdvancedSlotView::Empty,
            Ok(Some(data)) => match data.def() {
                Some(def) => AdvancedSlotView::Def { def },
                None => AdvancedSlotView::Raw {
                    bytes: HexBytes(bytes.to_vec()),
                },
            },
            Err(_) => AdvancedSlotView::Raw {
                bytes: HexBytes(bytes.to_vec()),
            },
        }
    }

    /// Every macro and advanced-key reference of both layers (malformed
    /// ones included).
    pub fn references(&self) -> Vec<(Layer, KeymapReference)> {
        Layer::ALL
            .into_iter()
            .flat_map(|layer| {
                self.layer(layer)
                    .references()
                    .into_iter()
                    .map(move |r| (layer, r))
            })
            .collect()
    }

    /// Non-empty slots no key refers to (the keyboard keeps them).
    pub fn orphans(&self) -> Vec<SlotRef> {
        let refs = slot_refs(&self.layers);
        let mut out: Vec<SlotRef> = MacroSlotId::all()
            .filter(|slot| !is_empty_macro(self.macro_bytes(*slot)))
            .map(SlotRef::Macro)
            .filter(|slot| !refs.iter().any(|(_, _, r)| r == slot))
            .collect();
        out.extend(
            AdvancedSlotId::all()
                .filter(|slot| self.advanced_bytes(*slot).iter().any(|b| *b != 0))
                .map(SlotRef::Advanced)
                .filter(|slot| !refs.iter().any(|(_, _, r)| r == slot)),
        );
        out
    }

    fn macro_bytes(&self, slot: MacroSlotId) -> &[u8] {
        self.macros
            .get(usize::from(slot.get()))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn advanced_bytes(&self, slot: AdvancedSlotId) -> &[u8] {
        let start = usize::from(slot.get()) * ADVANCED_SLOT_LEN;
        self.advanced
            .get(start..start + ADVANCED_SLOT_LEN)
            .unwrap_or_default()
    }

    /// Whether a restore of a backup of this group could write the current
    /// content of `slot` back (an empty slot always can).
    fn restorable(&self, slot: SlotRef) -> bool {
        match slot {
            SlotRef::Macro(slot) => self
                .raws
                .iter()
                .find(|r| r.request() == ReadRequest::Macro(slot))
                .is_some_and(|raw| MacroData::from_response(raw).is_ok()),
            SlotRef::Advanced(slot) => {
                AdvancedSlotData::from_read_slot(self.advanced_bytes(slot)).is_ok()
            }
        }
    }

    /// The group as a [`GroupView`] (for dangling references).
    fn view(&self) -> GroupView {
        let mut view = GroupView {
            layers: [Some(self.layers[0].clone()), Some(self.layers[1].clone())],
            ..GroupView::default()
        };
        for slot in MacroSlotId::all() {
            view.macros[usize::from(slot.get())] = !is_empty_macro(self.macro_bytes(slot));
        }
        for slot in AdvancedSlotId::all() {
            view.advanced[usize::from(slot.get())] =
                self.advanced_bytes(slot).iter().any(|b| *b != 0);
        }
        view
    }
}

/// Whether macro slot bytes are empty (all `00` or all `FF`).
fn is_empty_macro(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0) || bytes.iter().all(|b| *b == 0xFF)
}

/// Every macro and advanced-key reference of the layers: (layer, index,
/// slot). Malformed entries are not references.
fn slot_refs(layers: &[KeymapData; 2]) -> Vec<(Layer, u8, SlotRef)> {
    Layer::ALL
        .into_iter()
        .zip(layers)
        .flat_map(|(layer, data)| {
            data.references().into_iter().filter_map(move |r| match r {
                KeymapReference::Macro { index, slot } => {
                    Some((layer, index, SlotRef::Macro(slot)))
                }
                KeymapReference::Advanced { index, slot } => {
                    Some((layer, index, SlotRef::Advanced(slot)))
                }
                KeymapReference::Malformed { .. } => None,
            })
        })
        .collect()
}

/// The planned change of one keymap entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryChangeView {
    /// The layer.
    pub layer: Layer,
    /// The key index.
    pub index: u8,
    /// The key's name.
    pub name: String,
    /// The entry now.
    pub before: EntryKind,
    /// The entry after the edit.
    pub after: EntryKind,
}

/// The planned change of one slot's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlotChangeView {
    /// The slot.
    pub slot: SlotRef,
    /// The content now (`None` = empty).
    pub before: Option<String>,
    /// The content after the edit (`None` = empty).
    pub after: Option<String>,
}

/// A planned keymap-group edit; made by [`plan_keymap_edit`], not
/// deserializable.
#[derive(Debug, Clone, Serialize)]
pub struct KeymapEditPlan {
    profile: ProfileId,
    product: Option<String>,
    entry_changes: Vec<EntryChangeView>,
    slot_changes: Vec<SlotChangeView>,
    writes: Vec<PlannedEditWrite>,
    warnings: Vec<EditWarning>,
    /// The persistence check is forced on ([`CommitTriggerMode::OmitForSlotsOnly`]).
    force_persistence: bool,
    #[serde(skip)]
    before: GroupView,
}

impl KeymapEditPlan {
    /// The active profile the plan was made for.
    pub fn profile(&self) -> ProfileId {
        self.profile
    }

    /// The entries that change, in layer and index order.
    pub fn entry_changes(&self) -> &[EntryChangeView] {
        &self.entry_changes
    }

    /// The slots whose content changes.
    pub fn slot_changes(&self) -> &[SlotChangeView] {
        &self.slot_changes
    }

    /// The writes, in the order they are made.
    pub fn writes(&self) -> &[PlannedEditWrite] {
        &self.writes
    }

    /// What the user should know before writing.
    pub fn warnings(&self) -> &[EditWarning] {
        &self.warnings
    }

    /// Whether the persistence check is forced on.
    pub fn forces_persistence_check(&self) -> bool {
        self.force_persistence
    }

    /// Whether anything would be written.
    pub fn has_changes(&self) -> bool {
        !self.writes.is_empty()
    }
}

/// Reads the keymap group (read-only) and plans the edit
/// ([`plan_keymap_edit_on`]).
pub fn plan_keymap_edit<T: Transport>(
    session: &mut Session<T>,
    edit: &KeymapEdit,
) -> Result<KeymapEditPlan, EditError> {
    let state = KeymapGroupState::read(session)?;
    plan_keymap_edit_on(&state, edit)
}

/// The group after the edit, as far as planned.
struct Target {
    layers: [KeymapData; 2],
    /// `Some`: the slot is written with this content (or cleared).
    macros: [Option<MacroData>; 10],
    /// `Some(None)`: cleared; `Some(Some(_))`: written.
    advanced: Vec<Option<Option<AdvancedSlotData>>>,
}

impl Target {
    fn macro_full(&self, state: &KeymapGroupState, slot: MacroSlotId) -> bool {
        match &self.macros[usize::from(slot.get())] {
            Some(data) => !data.is_empty(),
            None => !is_empty_macro(state.macro_bytes(slot)),
        }
    }

    fn advanced_full(&self, state: &KeymapGroupState, slot: AdvancedSlotId) -> bool {
        match self.advanced.get(usize::from(slot.get())) {
            Some(Some(data)) => data.is_some(),
            _ => state.advanced_bytes(slot).iter().any(|b| *b != 0),
        }
    }

    fn full(&self, state: &KeymapGroupState, slot: SlotRef) -> bool {
        match slot {
            SlotRef::Macro(slot) => self.macro_full(state, slot),
            SlotRef::Advanced(slot) => self.advanced_full(state, slot),
        }
    }

    fn explicitly_set(&self, slot: SlotRef) -> bool {
        match slot {
            SlotRef::Macro(slot) => self.macros[usize::from(slot.get())].is_some(),
            SlotRef::Advanced(slot) => self
                .advanced
                .get(usize::from(slot.get()))
                .is_some_and(Option::is_some),
        }
    }
}

/// Plans `edit` against `state` (pure: nothing is read or written). See the
/// [module documentation](self) for the rules.
pub fn plan_keymap_edit_on(
    state: &KeymapGroupState,
    edit: &KeymapEdit,
) -> Result<KeymapEditPlan, EditError> {
    if edit.changes.is_empty() {
        return Err(EditError::Invalid {
            reason: "nothing to change".to_string(),
        });
    }
    let model = state.model;
    let mut target = Target {
        layers: state.layers.clone(),
        macros: Default::default(),
        advanced: vec![None; 40],
    };
    let mut warnings: Vec<EditWarning> = Vec::new();
    let push_warning = |warnings: &mut Vec<EditWarning>, warning: EditWarning| {
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    };
    let place = |layer: Layer,
                 pos: KeyPos,
                 assignment: &KeyAssignment,
                 warnings: &mut Vec<EditWarning>|
     -> Result<(), EditError> {
        let builtin = check_placement(
            model,
            layer,
            pos,
            assignment,
            &state.fn_locked,
            &edit.builtin_acks,
        )?;
        if let Some(builtin) = builtin {
            let warning = EditWarning::FnBuiltinOverridden {
                index: builtin.index,
                function: builtin.function,
                critical: builtin.critical,
            };
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
        if let Some(note) = assignment.evidence(layer).note() {
            let warning = EditWarning::Unverified(note);
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
        Ok(())
    };
    for change in &edit.changes {
        match change {
            KeymapChange::Assign {
                layer,
                pos,
                assignment,
            } => {
                place(*layer, *pos, assignment, &mut warnings)?;
                let bytes = assignment.entry(model, *pos)?;
                target.layers[layer_index(*layer)].set_entry(*pos, bytes);
            }
            KeymapChange::BindMacro {
                layer,
                pos,
                program,
                slot,
            } => {
                let slot = match slot {
                    Some(slot) => *slot,
                    None => choose_macro_slot(state, &target, *layer, *pos)?,
                };
                let assignment = KeyAssignment::Macro(slot);
                place(*layer, *pos, &assignment, &mut warnings)?;
                if let Some(note) = program.evidence().note() {
                    push_warning(&mut warnings, EditWarning::Unverified(note));
                }
                target.macros[usize::from(slot.get())] = Some(program.encode(slot));
                let bytes = assignment.entry(model, *pos)?;
                target.layers[layer_index(*layer)].set_entry(*pos, bytes);
            }
            KeymapChange::BindAdvanced {
                layer,
                pos,
                def,
                slot,
            } => {
                let limits = state.limits.ok_or(EditError::DistanceUnit {
                    unit_um: state.unit_um,
                })?;
                let data = AdvancedSlotData::from_def(def, &limits)?;
                let slot = match slot {
                    Some(slot) => *slot,
                    None => choose_advanced_slot(state, &target, *layer, *pos)?,
                };
                let assignment = KeyAssignment::Advanced(slot);
                place(*layer, *pos, &assignment, &mut warnings)?;
                for note in def.evidence() {
                    push_warning(&mut warnings, EditWarning::Unverified(note));
                }
                if let Some(entry) = target.advanced.get_mut(usize::from(slot.get())) {
                    *entry = Some(Some(data));
                }
                let bytes = assignment.entry(model, *pos)?;
                target.layers[layer_index(*layer)].set_entry(*pos, bytes);
            }
            KeymapChange::SetMacroSlot { slot, program } => {
                if let Some(note) = program.as_ref().and_then(|p| p.evidence().note()) {
                    push_warning(&mut warnings, EditWarning::Unverified(note));
                }
                target.macros[usize::from(slot.get())] = Some(match program {
                    Some(program) => program.encode(*slot),
                    None => MacroData::Empty,
                });
            }
            KeymapChange::SetAdvancedSlot { slot, def } => {
                let data = match def {
                    Some(def) => {
                        let limits = state.limits.ok_or(EditError::DistanceUnit {
                            unit_um: state.unit_um,
                        })?;
                        for note in def.evidence() {
                            push_warning(&mut warnings, EditWarning::Unverified(note));
                        }
                        Some(AdvancedSlotData::from_def(def, &limits)?)
                    }
                    None => None,
                };
                if let Some(entry) = target.advanced.get_mut(usize::from(slot.get())) {
                    *entry = Some(data);
                }
            }
            KeymapChange::Unbind { layer, pos } => {
                place(*layer, *pos, &KeyAssignment::Default, &mut warnings)?;
                target.layers[layer_index(*layer)].set_entry(*pos, [0; 4]);
            }
        }
    }

    let before_refs = slot_refs(&state.layers);
    let after_refs = slot_refs(&target.layers);
    let changed = |layer: Layer, index: u8| -> bool {
        KeyPos::from_index(index).is_some_and(|pos| {
            state.layer(layer).entry(pos) != target.layers[layer_index(layer)].entry(pos)
        })
    };

    // Slots whose last binding the edit removes.
    for (_, _, slot) in &before_refs {
        let still = after_refs.iter().any(|(_, _, r)| r == slot);
        if still || target.explicitly_set(*slot) || !target.full(state, *slot) {
            continue;
        }
        // Content the undo could not write back is never cleared.
        if edit.keep_orphans || !state.restorable(*slot) {
            push_warning(&mut warnings, EditWarning::OrphanSlotKept { slot: *slot });
        } else {
            match slot {
                SlotRef::Macro(slot) => {
                    target.macros[usize::from(slot.get())] = Some(MacroData::Empty)
                }
                SlotRef::Advanced(slot) => {
                    if let Some(entry) = target.advanced.get_mut(usize::from(slot.get())) {
                        *entry = Some(None);
                    }
                }
            }
        }
    }

    // Every reference after the edit.
    for (layer, index, slot) in &after_refs {
        let refs: Vec<&(Layer, u8, SlotRef)> =
            after_refs.iter().filter(|(_, _, r)| r == slot).collect();
        let entry_changed = changed(*layer, *index);
        if !target.full(state, *slot) {
            if entry_changed {
                return Err(EditError::BindingToEmptySlot {
                    layer: *layer,
                    index: *index,
                    slot: *slot,
                });
            }
            if target.explicitly_set(*slot) {
                return Err(EditError::SlotStillReferenced {
                    slot: *slot,
                    by: refs.iter().map(|(l, i, _)| (*l, *i)).collect(),
                });
            }
            push_warning(
                &mut warnings,
                EditWarning::DanglingReferencePreserved {
                    layer: *layer,
                    index: *index,
                    slot: *slot,
                },
            );
            continue;
        }
        if entry_changed && refs.len() > 1 {
            let other = refs
                .iter()
                .find(|(l, i, _)| (*l, *i) != (*layer, *index))
                .map(|(l, i, _)| (*l, *i))
                .unwrap_or((*layer, *index));
            return Err(EditError::SlotAlreadyBound {
                slot: *slot,
                by: other,
            });
        }
    }

    // The writes, in the site's order.
    let mut macro_writes = Vec::new();
    for slot in MacroSlotId::all() {
        let Some(data) = &target.macros[usize::from(slot.get())] else {
            continue;
        };
        if !data.is_equivalent(state.macro_bytes(slot)) {
            macro_writes.push(WriteRequest::Macro(slot, data.clone()));
        }
    }
    let mut advanced_writes = Vec::new();
    for slot in AdvancedSlotId::all() {
        let Some(Some(content)) = target.advanced.get(usize::from(slot.get())) else {
            continue;
        };
        let current = state.advanced_bytes(slot);
        match content {
            Some(data) if data.as_bytes()[..] != *current => {
                advanced_writes.push(WriteRequest::AdvancedKey(slot, data.clone()));
            }
            None if current.iter().any(|b| *b != 0) => {
                advanced_writes.push(WriteRequest::AdvancedKeyClear(slot));
            }
            _ => {}
        }
    }
    // Every slot written must be one the undo can write back.
    let written_slots = macro_writes
        .iter()
        .chain(&advanced_writes)
        .filter_map(|request| match request {
            WriteRequest::Macro(slot, _) => Some(SlotRef::Macro(*slot)),
            WriteRequest::AdvancedKey(slot, _) | WriteRequest::AdvancedKeyClear(slot) => {
                Some(SlotRef::Advanced(*slot))
            }
            _ => None,
        });
    for slot in written_slots.filter(|slot| !state.restorable(*slot)) {
        let table = match slot {
            SlotRef::Macro(slot) => TableId::Macro { slot },
            SlotRef::Advanced(slot) => TableId::AdvancedKey { slot },
        };
        if !edit.allow_not_undoable {
            return Err(EditError::UnsupportedCurrentTable {
                table,
                reason: "its content is not in a form OpenWraith can write, so a restore of the \
                         safety backup could not bring it back: overwriting or clearing it could \
                         not be undone"
                    .to_string(),
            });
        }
        push_warning(&mut warnings, EditWarning::NotUndoable { table });
    }
    let base_changed = target.layers[0] != state.layers[0];
    let fn_changed = target.layers[1] != state.layers[1];
    let slots_written = !macro_writes.is_empty() || !advanced_writes.is_empty();
    if edit.commit_trigger == CommitTriggerMode::OmitForSlotsOnly && (base_changed || fn_changed) {
        return Err(EditError::Invalid {
            reason: "the Fn keymap can only be left out when no keymap layer changes (a hardware \
                     test of slot-only writes)"
                .to_string(),
        });
    }
    let reason = if base_changed {
        Some(CommitTriggerReason::BaseKeymap)
    } else if slots_written && edit.commit_trigger == CommitTriggerMode::Auto {
        Some(CommitTriggerReason::MacrosOrAdvancedKeys)
    } else {
        None
    };
    let planned = |request: &WriteRequest| state_of(request, &state.raws);
    let mut writes: Vec<PlannedEditWrite> = Vec::new();
    for request in macro_writes {
        let p = planned(&request);
        writes.push(PlannedEditWrite::new(request, None, p));
    }
    if base_changed {
        let request = WriteRequest::Keymap(Layer::Base, target.layers[0].clone());
        let p = planned(&request);
        writes.push(PlannedEditWrite::new(request, None, p));
    }
    if reason.is_some() || fn_changed {
        let request = WriteRequest::Keymap(Layer::Fn, target.layers[1].clone());
        let p = planned(&request);
        let mut write = PlannedEditWrite::new(request, None, p);
        if let Some(reason) = reason {
            write.commit_trigger = Some(reason);
            write.resent_unchanged = !fn_changed;
            if !fn_changed {
                write.summary = match reason {
                    CommitTriggerReason::BaseKeymap => {
                        "Fn keymap (18 02) re-sent unchanged to make the keyboard save the base \
                         keymap"
                    }
                    CommitTriggerReason::MacrosOrAdvancedKeys => {
                        "Fn keymap (18 02) re-sent unchanged after the macro slots, as the vendor \
                         site does (precaution)"
                    }
                }
                .to_string();
            }
        }
        writes.push(write);
    }
    for request in advanced_writes {
        let p = planned(&request);
        writes.push(PlannedEditWrite::new(request, None, p));
    }
    mark_bindings(state, &target, &mut writes, &mut warnings);

    // What the user sees.
    for (layer, data) in Layer::ALL.into_iter().zip(&target.layers) {
        let written = writes
            .iter()
            .any(|w| w.table == (TableId::Keymap { layer }) && !w.resent_unchanged);
        if !written {
            continue;
        }
        for reference in data.references() {
            if let KeymapReference::Malformed { index, .. } = reference {
                push_warning(
                    &mut warnings,
                    EditWarning::MalformedReferencePreserved { layer, index },
                );
            }
        }
    }
    let untested: Vec<TableId> = {
        let mut tables: Vec<TableId> = Vec::new();
        for write in &writes {
            let table = match write.table {
                TableId::Macro { .. } => write.table,
                TableId::AdvancedKey { .. } => write.table,
                _ => continue,
            };
            if !tables.contains(&table) {
                tables.push(table);
            }
        }
        tables
    };
    if !untested.is_empty() {
        warnings.push(EditWarning::PowerLossUntested { tables: untested });
    }
    let mut entry_changes = Vec::new();
    for layer in Layer::ALL {
        for pos in KeyPos::all() {
            let (old, new) = (
                state.layer(layer).entry(pos),
                target.layers[layer_index(layer)].entry(pos),
            );
            if old != new {
                entry_changes.push(EntryChangeView {
                    layer,
                    index: pos.index(),
                    name: model.key_name(pos.index()),
                    before: KeyAssignment::classify(model, pos, old),
                    after: KeyAssignment::classify(model, pos, new),
                });
            }
        }
    }
    let slot_changes = slot_changes(state, &writes);
    Ok(KeymapEditPlan {
        profile: state.profile,
        product: state.product.clone(),
        entry_changes,
        slot_changes,
        writes,
        warnings,
        force_persistence: edit.commit_trigger == CommitTriggerMode::OmitForSlotsOnly,
        before: state.view(),
    })
}

/// The slot changes of the planned writes, for display.
fn slot_changes(state: &KeymapGroupState, writes: &[PlannedEditWrite]) -> Vec<SlotChangeView> {
    let macro_text = |view: MacroSlotView| match view {
        MacroSlotView::Empty => None,
        MacroSlotView::Program { program } => Some(format!("{program} ({:?})", program.play())),
        MacroSlotView::Raw { bytes } => Some(format!("raw {bytes}")),
    };
    let advanced_text = |view: AdvancedSlotView| match view {
        AdvancedSlotView::Empty => None,
        AdvancedSlotView::Def { def } => Some(def.to_string()),
        AdvancedSlotView::Raw { bytes } => Some(format!("raw {bytes}")),
    };
    writes
        .iter()
        .filter_map(|write| match write.request() {
            WriteRequest::Macro(slot, data) => Some(SlotChangeView {
                slot: SlotRef::Macro(*slot),
                before: macro_text(state.macro_program(*slot)),
                after: match data {
                    MacroData::Empty => None,
                    MacroData::Content(content) => Some(match MacroProgram::decode(content) {
                        Some(program) => format!("{program} ({:?})", program.play()),
                        None => format!("raw {}", HexBytes(content.as_bytes().to_vec())),
                    }),
                },
            }),
            WriteRequest::AdvancedKey(slot, data) => Some(SlotChangeView {
                slot: SlotRef::Advanced(*slot),
                before: advanced_text(state.advanced(*slot)),
                after: data.def().map(|def| def.to_string()),
            }),
            WriteRequest::AdvancedKeyClear(slot) => Some(SlotChangeView {
                slot: SlotRef::Advanced(*slot),
                before: advanced_text(state.advanced(*slot)),
                after: None,
            }),
            _ => None,
        })
        .collect()
}

/// Flags the writes that complete a binding: an advanced-key slot written
/// with content that a written layer refers to (a [`EditWarning::TransientBinding`]
/// in the site's order), and a layer write that changes an entry referring
/// to a macro slot written in this group (cleared or rewritten), so that a
/// cancellation never leaves the old key on the slot's new content.
fn mark_bindings(
    state: &KeymapGroupState,
    target: &Target,
    writes: &mut [PlannedEditWrite],
    warnings: &mut Vec<EditWarning>,
) {
    let written_layers: Vec<Layer> = writes
        .iter()
        .filter(|w| !w.resent_unchanged)
        .filter_map(|w| match w.table {
            TableId::Keymap { layer } => Some(layer),
            _ => None,
        })
        .collect();
    let bound: Vec<(SlotRef, Layer)> = slot_refs(&target.layers)
        .into_iter()
        .filter(|(layer, _, _)| written_layers.contains(layer))
        .map(|(layer, _, slot)| (slot, layer))
        .collect();
    let written_macros: Vec<SlotRef> = writes
        .iter()
        .filter_map(|w| match w.request() {
            WriteRequest::Macro(slot, _) => Some(SlotRef::Macro(*slot)),
            _ => None,
        })
        .collect();
    let before = slot_refs(&state.layers);
    let entry_changes = |layer: Layer, index: u8| {
        KeyPos::from_index(index).is_some_and(|pos| {
            state.layer(layer).entry(pos) != target.layers[layer_index(layer)].entry(pos)
        })
    };
    for write in writes.iter_mut() {
        let mut tables: Vec<TableId> = match write.request() {
            WriteRequest::AdvancedKey(slot, _) => bound
                .iter()
                .filter(|(s, _)| *s == SlotRef::Advanced(*slot))
                .map(|(_, layer)| TableId::Keymap { layer: *layer })
                .collect(),
            WriteRequest::Keymap(layer, _) if !write.resent_unchanged => before
                .iter()
                .filter(|(l, _, slot)| l == layer && written_macros.contains(slot))
                .filter(|(l, index, _)| entry_changes(*l, *index))
                .filter_map(|(_, _, slot)| match slot {
                    SlotRef::Macro(slot) => Some(TableId::Macro { slot: *slot }),
                    SlotRef::Advanced(_) => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        tables.dedup();
        write.completes_binding = !tables.is_empty();
        write.binding_tables = tables;
        if let (true, WriteRequest::AdvancedKey(slot, _)) =
            (write.completes_binding, write.request())
        {
            let warning = EditWarning::TransientBinding {
                slot: SlotRef::Advanced(*slot),
            };
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
    }
}

/// The macro slot for a binding at `(layer, pos)`: the slot only this key
/// refers to (when the undo can write its content back), else the lowest
/// slot that is empty and unreferenced.
fn choose_macro_slot(
    state: &KeymapGroupState,
    target: &Target,
    layer: Layer,
    pos: KeyPos,
) -> Result<MacroSlotId, EditError> {
    let refs = slot_refs(&target.layers);
    let own = refs.iter().find_map(|(l, i, slot)| match slot {
        SlotRef::Macro(slot) if *l == layer && *i == pos.index() => Some(*slot),
        _ => None,
    });
    if let Some(slot) = own {
        let others = refs
            .iter()
            .filter(|(_, _, r)| *r == SlotRef::Macro(slot))
            .count();
        // A slot the undo could not write back is left as it is.
        if others == 1 && state.restorable(SlotRef::Macro(slot)) {
            return Ok(slot);
        }
    }
    MacroSlotId::all()
        .find(|slot| {
            !target.macro_full(state, *slot)
                && !target.explicitly_set(SlotRef::Macro(*slot))
                && !refs.iter().any(|(_, _, r)| *r == SlotRef::Macro(*slot))
        })
        .ok_or(EditError::NoFreeSlot {
            kind: super::SlotKind::Macro,
        })
}

/// The advanced-key slot for a binding at `(layer, pos)`: the slot only
/// this key refers to (when the undo can write its content back), else the
/// lowest slot that is empty and unreferenced (`getAdvIndex`,
/// `wmIndex.js:3790-3803`).
fn choose_advanced_slot(
    state: &KeymapGroupState,
    target: &Target,
    layer: Layer,
    pos: KeyPos,
) -> Result<AdvancedSlotId, EditError> {
    let refs = slot_refs(&target.layers);
    let own = refs.iter().find_map(|(l, i, slot)| match slot {
        SlotRef::Advanced(slot) if *l == layer && *i == pos.index() => Some(*slot),
        _ => None,
    });
    if let Some(slot) = own {
        let others = refs
            .iter()
            .filter(|(_, _, r)| *r == SlotRef::Advanced(slot))
            .count();
        // A slot the undo could not write back is left as it is.
        if others == 1 && state.restorable(SlotRef::Advanced(slot)) {
            return Ok(slot);
        }
    }
    AdvancedSlotId::all()
        .find(|slot| {
            !target.advanced_full(state, *slot)
                && !target.explicitly_set(SlotRef::Advanced(*slot))
                && !refs.iter().any(|(_, _, r)| *r == SlotRef::Advanced(*slot))
        })
        .ok_or(EditError::NoFreeSlot {
            kind: super::SlotKind::Advanced,
        })
}

/// Writes a planned keymap-group edit (see the [module documentation](self)
/// and the [edit engine](super)). With
/// [`CommitTriggerMode::OmitForSlotsOnly`] the persistence check always
/// runs.
pub fn apply_keymap_edit<T: WriteTransport>(
    session: &mut Session<T>,
    plan: &KeymapEditPlan,
    saved: &SavedBackup,
    consent: WriteConsent,
    options: &EditOptions,
    progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> EditReport {
    let persistence = if plan.force_persistence {
        PersistenceCheckMode::Always
    } else {
        options.persistence
    };
    let run = EditRun {
        profile: plan.profile,
        product: plan.product.as_deref(),
        group_before: Some(&plan.before),
        warnings: &plan.warnings,
        persistence,
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
