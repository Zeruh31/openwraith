//! The keymap group as the Keys page sees it, keymap plans and refusals.
//! Everything here reads the library's views (`KeymapGroupState`,
//! `EntryKind`, `MacroSlotView`, `AdvancedSlotView`, `KeymapEditPlan`); no
//! entry or slot bytes are built for writing.

use std::collections::BTreeMap;

use openwraith_hid::protocol::write::{DksStep, KeyMotion, MacroStep};
use openwraith_hid::protocol::{
    AdvancedKeyDef, AdvancedSlotId, EntryKind, Evidence, KeyAction, KeyAssignment, KeyPos,
    KeymapReference, Layer, MATRIX_COLS, MacroPlay, MacroProgram, MacroSlotId, Model, MouseAction,
    MouseButton, PlacementError, ProfileId, TableId, UnverifiedNote, WriteBuildError,
};
use openwraith_hid::{
    AdvancedSlotView, EditError, EditWarning, KeymapEditPlan, KeymapGroupState, MacroSlotView,
    PersistenceCheckMode, PlannedEditWrite, SlotKind, SlotRef, persistence_check_tables,
};

use super::error::app_kind;
use super::{
    clean, commit_reason, hex, key_ref, key_ref_index, layer_view, resend_reason, sat8, sat16,
    table_ref,
};
use crate::dto::{
    ActionValue, AdvancedDefView, AdvancedSlotContentView, AdvancedSlotStateView, AppError,
    AppErrorKind, BindingView, BrokenKindView, BrokenReferenceView, DksRowView, DksStepView,
    EntryOriginView, ErrorDetail, EvidenceView, FnBuiltinView, KeyAssignmentsView,
    KeyEntryChangeView, KeyEntryView, KeyKindView, KeyMotionView, KeymapEditPreview,
    KeymapProblemView, KeymapSlotChangeView, KeymapWarningView, KeymapWriteView, KeysLimitsView,
    KeysState, KnobEntryView, LayerView, MacroPlayView, MacroProgramView, MacroSlotContentView,
    MacroSlotStateView, MacroStepView, MouseActionView, PersistencePlanView, PreservedEntryView,
    PreservedKindView, SlotContentView, SlotKindView, TableRef, UnverifiedNoteView,
};
use crate::ops::keymap::KeysCache;

// ----- evidence -----

/// One unverified note.
pub(crate) fn note_view(note: &UnverifiedNote) -> UnverifiedNoteView {
    UnverifiedNoteView {
        what: note.what.to_string(),
        question: note.question.to_string(),
    }
}

/// The library's evidence of one value.
pub(crate) fn evidence_view(evidence: Evidence) -> EvidenceView {
    match evidence.note() {
        None => EvidenceView::Confirmed,
        Some(note) => EvidenceView::Unverified {
            notes: vec![note_view(&note)],
        },
    }
}

/// The library's notes of a value that may have several (empty: confirmed).
pub(crate) fn notes_evidence(notes: &[UnverifiedNote]) -> EvidenceView {
    if notes.is_empty() {
        EvidenceView::Confirmed
    } else {
        EvidenceView::Unverified {
            notes: notes.iter().map(note_view).collect(),
        }
    }
}

// ----- actions -----

/// A mouse action as a view.
pub(crate) fn mouse_view(action: MouseAction) -> MouseActionView {
    match action {
        MouseAction::Button(MouseButton::Left) => MouseActionView::Left,
        MouseAction::Button(MouseButton::Right) => MouseActionView::Right,
        MouseAction::Button(MouseButton::Middle) => MouseActionView::Middle,
        MouseAction::Button(MouseButton::Back) => MouseActionView::Back,
        MouseAction::Button(MouseButton::Forward) => MouseActionView::Forward,
        MouseAction::WheelUp => MouseActionView::WheelUp,
        MouseAction::WheelDown => MouseActionView::WheelDown,
    }
}

/// A mouse view as the library's action.
pub(crate) fn mouse_action(view: MouseActionView) -> MouseAction {
    match view {
        MouseActionView::Left => MouseAction::Button(MouseButton::Left),
        MouseActionView::Right => MouseAction::Button(MouseButton::Right),
        MouseActionView::Middle => MouseAction::Button(MouseButton::Middle),
        MouseActionView::Back => MouseAction::Button(MouseButton::Back),
        MouseActionView::Forward => MouseAction::Button(MouseButton::Forward),
        MouseActionView::WheelUp => MouseAction::WheelUp,
        MouseActionView::WheelDown => MouseAction::WheelDown,
    }
}

/// The id word of a mouse action (`"mouse:<word>"`).
fn mouse_word(view: MouseActionView) -> &'static str {
    match view {
        MouseActionView::Left => "left",
        MouseActionView::Right => "right",
        MouseActionView::Middle => "middle",
        MouseActionView::Back => "back",
        MouseActionView::Forward => "forward",
        MouseActionView::WheelUp => "wheel_up",
        MouseActionView::WheelDown => "wheel_down",
    }
}

/// An assignment as an action value.
pub(crate) fn action_value(assignment: &KeyAssignment) -> ActionValue {
    match assignment {
        KeyAssignment::Default => ActionValue::Default,
        KeyAssignment::Key(usage) => ActionValue::Key { usage: usage.get() },
        KeyAssignment::Media(key) => ActionValue::Media { index: key.get() },
        KeyAssignment::Mouse(action) => ActionValue::Mouse {
            action: mouse_view(*action),
        },
        KeyAssignment::Combo(combo) => ActionValue::Combo {
            modifiers: combo.modifiers().iter().map(|m| m.get()).collect(),
            key: combo.key().get(),
        },
        KeyAssignment::Macro(slot) => ActionValue::Macro { slot: slot.get() },
        KeyAssignment::Advanced(slot) => ActionValue::Advanced { slot: slot.get() },
    }
}

/// The stable id of an action: `default`, `key:04`, `media:12`,
/// `mouse:back`, `combo:e0+06`, `combo:e0+e1+04`, `macro:3`,
/// `advanced:12` (hex lowercase, slots 0-based).
pub(crate) fn action_id(value: &ActionValue) -> String {
    match value {
        ActionValue::Default => "default".to_string(),
        ActionValue::Key { usage } => format!("key:{usage:02x}"),
        ActionValue::Media { index } => format!("media:{index:02x}"),
        ActionValue::Mouse { action } => format!("mouse:{}", mouse_word(*action)),
        ActionValue::Combo { modifiers, key } => {
            let mut parts: Vec<String> = modifiers.iter().map(|m| format!("{m:02x}")).collect();
            parts.push(format!("{key:02x}"));
            format!("combo:{}", parts.join("+"))
        }
        ActionValue::Macro { slot } => format!("macro:{slot}"),
        ActionValue::Advanced { slot } => format!("advanced:{slot}"),
    }
}

/// An English label of an assignment (technical fallback text).
pub(crate) fn action_label(assignment: &KeyAssignment) -> String {
    match assignment {
        KeyAssignment::Default => "Default".to_string(),
        KeyAssignment::Key(usage) => usage.to_string(),
        KeyAssignment::Media(key) => key
            .name()
            .map_or_else(|| format!("Media {:#04x}", key.get()), str::to_string),
        KeyAssignment::Mouse(action) => mouse_label(*action).to_string(),
        KeyAssignment::Combo(combo) => combo.to_string(),
        KeyAssignment::Macro(slot) => format!("Macro {}", u16::from(slot.get()) + 1),
        KeyAssignment::Advanced(slot) => format!("Advanced key {}", u16::from(slot.get()) + 1),
    }
}

/// The English name of a mouse action.
pub(crate) fn mouse_label(action: MouseAction) -> &'static str {
    match action {
        MouseAction::Button(MouseButton::Left) => "Left click",
        MouseAction::Button(MouseButton::Right) => "Right click",
        MouseAction::Button(MouseButton::Middle) => "Middle click",
        MouseAction::Button(MouseButton::Back) => "Back button (X1)",
        MouseAction::Button(MouseButton::Forward) => "Forward button (X2)",
        MouseAction::WheelUp => "Wheel up",
        MouseAction::WheelDown => "Wheel down",
    }
}

/// The category of an entry kept as read.
fn preserved_kind(action: &KeyAction) -> PreservedKindView {
    match action {
        KeyAction::Default | KeyAction::Unknown { .. } => PreservedKindView::Unknown,
        KeyAction::Key { .. } => PreservedKindView::Key,
        KeyAction::Fn { .. } => PreservedKindView::FnKey,
        KeyAction::Lighting { .. }
        | KeyAction::Brightness { .. }
        | KeyAction::EffectSpeed { .. }
        | KeyAction::Direction { .. }
        | KeyAction::Color { .. } => PreservedKindView::Lighting,
        KeyAction::Media { .. } => PreservedKindView::Media,
        KeyAction::Macro { .. } => PreservedKindView::Macro,
        KeyAction::Function { .. } => PreservedKindView::Function,
        KeyAction::Mouse { .. } => PreservedKindView::Mouse,
        KeyAction::Combo { .. } => PreservedKindView::Combo,
        KeyAction::Advanced { .. } => PreservedKindView::Advanced,
        KeyAction::GamepadClassical { .. } | KeyAction::GamepadXbox { .. } => {
            PreservedKindView::Gamepad
        }
    }
}

/// One keymap entry (`bytes`: the 4 entry bytes, for display).
pub(crate) fn entry_view(layer: Layer, kind: &EntryKind, bytes: [u8; 4]) -> KeyEntryView {
    match kind {
        EntryKind::Assignable(assignment) => {
            let value = action_value(assignment);
            KeyEntryView {
                layer: layer_view(layer),
                id: action_id(&value),
                origin: match assignment {
                    KeyAssignment::Default => EntryOriginView::Default,
                    KeyAssignment::Macro(slot) => EntryOriginView::MacroSlot { slot: slot.get() },
                    KeyAssignment::Advanced(slot) => {
                        EntryOriginView::AdvancedSlot { slot: slot.get() }
                    }
                    _ => EntryOriginView::Remapped,
                },
                value: Some(value),
                label: action_label(assignment),
                hex: hex(&bytes),
                evidence: Some(evidence_view(assignment.evidence(layer))),
                preserved: None,
            }
        }
        EntryKind::Preserved(action) => {
            let text = clean(&action.to_string());
            KeyEntryView {
                layer: layer_view(layer),
                id: format!("preserved:{}", hex(&bytes)),
                value: None,
                label: text.clone(),
                origin: EntryOriginView::Preserved,
                hex: hex(&bytes),
                evidence: None,
                preserved: Some(PreservedEntryView {
                    kind: preserved_kind(action),
                    text,
                }),
            }
        }
    }
}

/// The current entry of the key at `pos` on `layer`.
fn current_entry(group: &KeymapGroupState, layer: Layer, pos: KeyPos) -> KeyEntryView {
    entry_view(
        layer,
        &group.entry(layer, pos),
        group.layer(layer).entry(pos),
    )
}

// ----- macros and advanced keys -----

/// A macro program as a view.
pub(crate) fn program_view(program: &MacroProgram) -> MacroProgramView {
    MacroProgramView {
        play: play_view(program.play()),
        steps: program.steps().iter().map(step_view).collect(),
    }
}

/// A play mode as a view.
pub(crate) fn play_view(play: MacroPlay) -> MacroPlayView {
    match play {
        MacroPlay::Once => MacroPlayView::Once,
        MacroPlay::Times(count) => MacroPlayView::Times { count },
        MacroPlay::Toggle => MacroPlayView::Toggle,
        MacroPlay::WhileHeld => MacroPlayView::WhileHeld,
    }
}

fn step_view(step: &MacroStep) -> MacroStepView {
    MacroStepView {
        usage: step.usage.get(),
        motion: match step.motion {
            KeyMotion::Press => KeyMotionView::Press,
            KeyMotion::Release => KeyMotionView::Release,
        },
        delay_after_ms: step.delay_after_ms,
    }
}

/// A stable hash of a macro program (FNV-1a, 64 bit, 16 hex digits) over
/// its play mode, count and steps; the slot number is not part of it. The
/// local library matches its entries to the keyboard's slots by it.
pub(crate) fn content_hash(program: &MacroProgram) -> String {
    let mut bytes = b"owmacro1".to_vec();
    let play = program.play();
    bytes.push(play.code());
    bytes.extend_from_slice(&play.count().to_be_bytes());
    for step in program.steps() {
        bytes.push(step.usage.get());
        bytes.push(match step.motion {
            KeyMotion::Press => 0x10,
            KeyMotion::Release => 0x00,
        });
        bytes.extend_from_slice(&step.delay_after_ms.to_be_bytes());
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// A DKS step as a view.
pub(crate) fn dks_step_view(step: DksStep) -> DksStepView {
    match step {
        DksStep::None => DksStepView::None,
        DksStep::Tap => DksStepView::Tap,
        DksStep::Hold => DksStepView::Hold,
        DksStep::HoldAgain => DksStepView::HoldAgain,
    }
}

/// An advanced key as a view (the Mod-Tap hold time in ms).
pub(crate) fn def_view(def: &AdvancedKeyDef) -> AdvancedDefView {
    match def {
        AdvancedKeyDef::Dks(dks) => AdvancedDefView::Dks {
            first_point: dks.first_point,
            rows: dks
                .rows
                .iter()
                .map(|row| {
                    row.map(|row| DksRowView {
                        key: row.key.get(),
                        steps: row.steps.iter().map(|s| dks_step_view(*s)).collect(),
                    })
                })
                .collect(),
        },
        AdvancedKeyDef::ModTap(mt) => AdvancedDefView::ModTap {
            tap: mt.tap.get(),
            hold: mt.hold.get(),
            hold_ms: u16::from(mt.threshold_10ms) * 10,
        },
        AdvancedKeyDef::Toggle(t) => AdvancedDefView::Toggle { key: t.key.get() },
    }
}

/// A macro slot's content.
fn macro_content(view: MacroSlotView) -> MacroSlotContentView {
    match view {
        MacroSlotView::Empty => MacroSlotContentView::Empty,
        MacroSlotView::Program { program } => MacroSlotContentView::Program {
            content_hash: content_hash(&program),
            evidence: evidence_view(program.evidence()),
            program: program_view(&program),
        },
        MacroSlotView::Raw { bytes } => MacroSlotContentView::KeptAsRead { hex: hex(&bytes.0) },
    }
}

/// An advanced-key slot's content.
fn advanced_content(view: AdvancedSlotView) -> AdvancedSlotContentView {
    match view {
        AdvancedSlotView::Empty => AdvancedSlotContentView::Empty,
        AdvancedSlotView::Def { def } => AdvancedSlotContentView::Def {
            evidence: notes_evidence(&def.evidence()),
            def: def_view(&def),
        },
        AdvancedSlotView::Raw { bytes } => AdvancedSlotContentView::KeptAsRead {
            type_byte: bytes.0.first().copied().unwrap_or(0),
            hex: hex(&bytes.0),
        },
    }
}

/// A slot as a table.
pub(crate) fn slot_table(slot: SlotRef) -> TableRef {
    match slot {
        SlotRef::Macro(slot) => TableRef::Macro { slot: slot.get() },
        SlotRef::Advanced(slot) => TableRef::AdvancedKey { slot: slot.get() },
    }
}

/// A binding.
fn binding(layer: Layer, index: u8) -> BindingView {
    BindingView {
        layer: layer_view(layer),
        key: key_ref_index(index),
    }
}

// ----- the keys state -----

/// The value ranges of the editors.
fn limits(model: &Model, group: Option<&KeymapGroupState>) -> KeysLimitsView {
    let ceilings = model.write_ceilings;
    let advanced = group.and_then(KeymapGroupState::advanced_limits);
    let (mt_min, mt_max) = advanced.map_or(
        (ceilings.mt_threshold_min, ceilings.mt_threshold_max),
        |limits| limits.mt_threshold_range(),
    );
    KeysLimitsView {
        macro_slots: sat8(u32::try_from(MacroSlotId::all().count()).unwrap_or(u32::MAX)),
        advanced_slots: sat8(u32::try_from(AdvancedSlotId::all().count()).unwrap_or(u32::MAX)),
        macro_max_steps: sat8(u32::try_from(MacroProgram::MAX_STEPS).unwrap_or(u32::MAX))
            .min(ceilings.macro_max_steps),
        macro_max_delay_ms: u16::MAX,
        macro_max_times: u8::MAX,
        dks_first_point_max: advanced.map(|limits| limits.dks_bottom()),
        mt_hold_ms_min: u16::from(mt_min) * 10,
        mt_hold_ms_max: u16::from(mt_max) * 10,
        mt_hold_ms_step: 10,
    }
}

/// Whether "reset key" may change the entry at `index` on `layer`.
fn resettable(model: &Model, group: &KeymapGroupState, layer: Layer, index: u8) -> bool {
    let Some(pos) = KeyPos::from_index(index) else {
        return false;
    };
    model.key(index).is_some()
        && model.knob_index != Some(index)
        && model.fn_index != Some(index)
        && (layer == Layer::Base || !group.fn_locked().contains(pos))
}

/// The knob's entry indices: the knob and the positions without a key next
/// to it in its row (15-21 on the W75).
fn knob_indices(model: &Model) -> Vec<u8> {
    let Some(knob) = model.knob_index else {
        return Vec::new();
    };
    let row = knob / MATRIX_COLS;
    let mut out = vec![knob];
    let mut index = knob;
    while let Some(next) = index.checked_add(1) {
        if next / MATRIX_COLS != row || model.key(next).is_some() {
            break;
        }
        out.push(next);
        index = next;
    }
    out
}

/// The keys state of a cached group (`get_keys_state`).
pub(crate) fn keys_state(cache: &KeysCache) -> KeysState {
    let model = cache.model;
    let mut state = KeysState {
        revision: cache.revision,
        generation: cache.generation,
        profile: cache.profile,
        read_at: cache.read_at.clone(),
        editable: false,
        disabled: None,
        advanced_editable: false,
        keys: Vec::new(),
        knob: Vec::new(),
        macros: Vec::new(),
        advanced: Vec::new(),
        advanced_used: 0,
        broken: Vec::new(),
        limits: limits(model, None),
    };
    let group = match &cache.group {
        Ok(group) => group,
        Err(disabled) => {
            state.disabled = Some(disabled.clone());
            return state;
        }
    };
    let model = group.model();
    state.editable = true;
    state.advanced_editable = group.advanced_limits().is_some();
    state.limits = limits(model, Some(group));
    for key in model.keys {
        let Some(pos) = KeyPos::from_index(key.index) else {
            continue;
        };
        let knob = model.knob_index == Some(key.index);
        let fn_key = model.fn_index == Some(key.index);
        let fn_locked = group.fn_locked().contains(pos);
        let assignable_base = !knob && !fn_key;
        state.keys.push(KeyAssignmentsView {
            key: key_ref(pos),
            kind: if knob {
                KeyKindView::Knob
            } else if fn_key {
                KeyKindView::FnKey
            } else {
                KeyKindView::Normal
            },
            base: current_entry(group, Layer::Base, pos),
            fn_layer: current_entry(group, Layer::Fn, pos),
            fn_locked,
            builtin: model.fn_builtin(key.index).map(|b| FnBuiltinView {
                index: b.index,
                function: b.function.to_string(),
                critical: b.critical,
                label_confirmed: b.confirmed,
            }),
            assignable_base,
            assignable_fn: assignable_base && !fn_locked,
            macro_capable: assignable_base && key.hid_usage.is_some(),
        });
    }
    for index in knob_indices(model) {
        let Some(pos) = KeyPos::from_index(index) else {
            continue;
        };
        for layer in Layer::ALL {
            let entry = current_entry(group, layer, pos);
            state.knob.push(KnobEntryView {
                index,
                layer: layer_view(layer),
                hex: entry.hex,
                text: entry.label,
                value: entry.value,
                preserved: entry.preserved,
            });
        }
    }
    let references = group.references();
    let orphans = group.orphans();
    let bound = |slot: SlotRef| -> Vec<BindingView> {
        references
            .iter()
            .filter_map(|(layer, reference)| match (reference, slot) {
                (KeymapReference::Macro { index, slot: s }, SlotRef::Macro(m)) if *s == m => {
                    Some(binding(*layer, *index))
                }
                (KeymapReference::Advanced { index, slot: s }, SlotRef::Advanced(a)) if *s == a => {
                    Some(binding(*layer, *index))
                }
                _ => None,
            })
            .collect()
    };
    for slot in MacroSlotId::all() {
        state.macros.push(MacroSlotStateView {
            slot: slot.get(),
            content: macro_content(group.macro_program(slot)),
            bindings: bound(SlotRef::Macro(slot)),
            orphan: orphans.contains(&SlotRef::Macro(slot)),
        });
    }
    for slot in AdvancedSlotId::all() {
        let content = advanced_content(group.advanced(slot));
        if content != AdvancedSlotContentView::Empty {
            state.advanced_used = state.advanced_used.saturating_add(1);
        }
        state.advanced.push(AdvancedSlotStateView {
            slot: slot.get(),
            content,
            bindings: bound(SlotRef::Advanced(slot)),
            orphan: orphans.contains(&SlotRef::Advanced(slot)),
        });
    }
    let macro_empty = |slot: MacroSlotId| {
        state
            .macros
            .get(usize::from(slot.get()))
            .is_some_and(|m| m.content == MacroSlotContentView::Empty)
    };
    let advanced_empty = |slot: AdvancedSlotId| {
        state
            .advanced
            .get(usize::from(slot.get()))
            .is_some_and(|a| a.content == AdvancedSlotContentView::Empty)
    };
    let mut broken = Vec::new();
    for (layer, reference) in &references {
        let index = reference.index();
        let Some(pos) = KeyPos::from_index(index) else {
            continue;
        };
        let problem = match reference {
            KeymapReference::Malformed { reason, .. } => BrokenKindView::Malformed {
                reason: (*reason).to_string(),
            },
            KeymapReference::Macro { slot, .. } if macro_empty(*slot) => {
                BrokenKindView::EmptySlot {
                    slot: TableRef::Macro { slot: slot.get() },
                }
            }
            KeymapReference::Advanced { slot, .. } if advanced_empty(*slot) => {
                BrokenKindView::EmptySlot {
                    slot: TableRef::AdvancedKey { slot: slot.get() },
                }
            }
            _ => continue,
        };
        broken.push(BrokenReferenceView {
            layer: layer_view(*layer),
            key: key_ref_index(index),
            problem,
            hex: hex(&group.layer(*layer).entry(pos)),
            resettable: resettable(model, group, *layer, index),
        });
    }
    state.broken = broken;
    state
}

// ----- plans -----

/// The content every slot gets from an edit's explicit changes (`None`:
/// emptied). A slot the plan writes without an entry here is an orphan the
/// library clears.
#[derive(Debug, Default)]
pub(crate) struct SlotTargets {
    /// Macro slots by number.
    pub(crate) macros: BTreeMap<u8, Option<MacroProgram>>,
    /// Advanced-key slots by number.
    pub(crate) advanced: BTreeMap<u8, Option<AdvancedKeyDef>>,
}

/// A slot's content in a plan.
fn macro_slot_content(view: MacroSlotView) -> SlotContentView {
    match view {
        MacroSlotView::Empty => SlotContentView::Empty,
        MacroSlotView::Program { program } => SlotContentView::Macro {
            program: program_view(&program),
        },
        MacroSlotView::Raw { bytes } => SlotContentView::KeptAsRead { hex: hex(&bytes.0) },
    }
}

fn advanced_slot_content(view: AdvancedSlotView) -> SlotContentView {
    match view {
        AdvancedSlotView::Empty => SlotContentView::Empty,
        AdvancedSlotView::Def { def } => SlotContentView::Advanced {
            def: def_view(&def),
        },
        AdvancedSlotView::Raw { bytes } => SlotContentView::KeptAsRead { hex: hex(&bytes.0) },
    }
}

/// A plan warning.
fn warning_view(warning: &EditWarning) -> KeymapWarningView {
    match warning {
        EditWarning::FnBuiltinOverridden {
            index,
            function,
            critical,
        } => KeymapWarningView::FnBuiltinOverridden {
            key: key_ref_index(*index),
            function: (*function).to_string(),
            critical: *critical,
        },
        EditWarning::Unverified(note) => KeymapWarningView::Unverified {
            note: note_view(note),
        },
        EditWarning::MalformedReferencePreserved { layer, index } => {
            KeymapWarningView::MalformedReferencePreserved {
                layer: layer_view(*layer),
                key: key_ref_index(*index),
            }
        }
        EditWarning::DanglingReferencePreserved { layer, index, slot } => {
            KeymapWarningView::DanglingReferencePreserved {
                layer: layer_view(*layer),
                key: key_ref_index(*index),
                slot: slot_table(*slot),
            }
        }
        EditWarning::TransientBinding { slot } => KeymapWarningView::TransientBinding {
            slot: slot_table(*slot),
        },
        EditWarning::OrphanSlotKept { slot } => KeymapWarningView::OrphanSlotKept {
            slot: slot_table(*slot),
        },
        EditWarning::NotUndoable { table } => KeymapWarningView::NotUndoable {
            table: table_ref(*table),
        },
        EditWarning::SocdPowerAllProfiles
        | EditWarning::NotShownUntilCustomMode
        | EditWarning::SocdHotkey => KeymapWarningView::Other {
            message: format!("{warning:?}"),
        },
    }
}

/// One planned write.
fn write_view(write: &PlannedEditWrite, clears: bool) -> KeymapWriteView {
    KeymapWriteView {
        table: table_ref(write.table),
        packets: sat16(write.packets),
        commit_trigger: write.commit_trigger.map(commit_reason),
        resent_unchanged: write.resent_unchanged,
        resent: write.resent.map(resend_reason),
        completes_binding: write.completes_binding,
        follows_macro_slot_0: write.follows_macro_slot_0(),
        clears,
        summary: clean(&write.summary),
    }
}

/// The profile the persistence check switches to and back from.
pub(crate) fn away_profile(profile: ProfileId) -> u8 {
    ProfileId::ALL
        .into_iter()
        .find(|p| *p != profile)
        .unwrap_or(profile)
        .get()
}

/// The plan of a keymap-group edit, for review (`token` is set by the
/// caller).
pub(crate) fn keymap_preview(
    plan: &KeymapEditPlan,
    group: &KeymapGroupState,
    targets: &SlotTargets,
    mode: PersistenceCheckMode,
    requested: bool,
) -> KeymapEditPreview {
    let model = group.model();
    let entry_changes = plan
        .entry_changes()
        .iter()
        .filter_map(|change| {
            let pos = KeyPos::from_index(change.index)?;
            let before = group.layer(change.layer).entry(pos);
            let after = match &change.after {
                EntryKind::Assignable(assignment) => assignment.entry(model, pos).unwrap_or(before),
                EntryKind::Preserved(_) => before,
            };
            Some(KeyEntryChangeView {
                layer: layer_view(change.layer),
                key: key_ref(pos),
                before: entry_view(change.layer, &change.before, before),
                after: entry_view(change.layer, &change.after, after),
            })
        })
        .collect();
    let summary = |slot: SlotRef| {
        plan.slot_changes()
            .iter()
            .find(|c| c.slot == slot)
            .map(|c| {
                let text = |t: &Option<String>| t.clone().unwrap_or_else(|| "empty".to_string());
                clean(&format!(
                    "{}: {} -> {}",
                    c.slot,
                    text(&c.before),
                    text(&c.after)
                ))
            })
            .unwrap_or_default()
    };
    let mut slot_changes = Vec::new();
    let mut writes = Vec::new();
    for write in plan.writes() {
        let change = match (write.table, write.resent) {
            (TableId::Macro { slot }, None) => {
                let after = match targets.macros.get(&slot.get()) {
                    Some(Some(program)) => SlotContentView::Macro {
                        program: program_view(program),
                    },
                    _ => SlotContentView::Empty,
                };
                Some(KeymapSlotChangeView {
                    slot: TableRef::Macro { slot: slot.get() },
                    before: macro_slot_content(group.macro_program(slot)),
                    after,
                    summary: summary(SlotRef::Macro(slot)),
                })
            }
            (TableId::AdvancedKey { slot }, None) => {
                let after = match targets.advanced.get(&slot.get()) {
                    Some(Some(def)) => SlotContentView::Advanced { def: def_view(def) },
                    _ => SlotContentView::Empty,
                };
                Some(KeymapSlotChangeView {
                    slot: TableRef::AdvancedKey { slot: slot.get() },
                    before: advanced_slot_content(group.advanced(slot)),
                    after,
                    summary: summary(SlotRef::Advanced(slot)),
                })
            }
            _ => None,
        };
        let clears = change
            .as_ref()
            .is_some_and(|c| c.after == SlotContentView::Empty);
        writes.push(write_view(write, clears));
        slot_changes.extend(change);
    }
    let tables = persistence_check_tables(plan.writes(), mode);
    KeymapEditPreview {
        token: None,
        nothing_to_do: !plan.has_changes(),
        profile: plan.profile().get(),
        entry_changes,
        slot_changes,
        writes,
        packets: sat16(plan.writes().iter().map(|w| w.packets).sum()),
        warnings: plan.warnings().iter().map(warning_view).collect(),
        persistence: PersistencePlanView {
            requested,
            forced: plan.forces_persistence_check(),
            will_run: !tables.is_empty(),
            tables: tables.into_iter().map(table_ref).collect(),
            away_profile: away_profile(plan.profile()),
        },
    }
}

// ----- refusals -----

/// A refused keymap edit (nothing was written).
pub(crate) fn refused(problem: KeymapProblemView, message: impl Into<String>) -> AppError {
    let message = message.into();
    AppError::with(
        AppErrorKind::InvalidRequest,
        ErrorDetail::Keymap { problem },
        format!("{message}; nothing was written"),
    )
}

/// A slot kind.
pub(crate) fn slot_kind(kind: SlotKind) -> SlotKindView {
    match kind {
        SlotKind::Macro => SlotKindView::Macro,
        SlotKind::Advanced => SlotKindView::Advanced,
    }
}

/// A placement refusal.
pub(crate) fn placement_problem(error: &PlacementError) -> KeymapProblemView {
    match *error {
        PlacementError::UnknownModel { .. } => KeymapProblemView::UnknownModel,
        PlacementError::NotAKey { index } => KeymapProblemView::NotAKey { index },
        PlacementError::Knob => KeymapProblemView::Knob,
        PlacementError::FnKey => KeymapProblemView::FnKey,
        PlacementError::FnLocked { index } => KeymapProblemView::FnLocked {
            key: key_ref_index(index),
        },
        PlacementError::BuiltinNotAcknowledged {
            index,
            function,
            critical,
            ..
        } => KeymapProblemView::BuiltinNotAcknowledged {
            key: key_ref_index(index),
            function: function.to_string(),
            critical,
        },
        PlacementError::NoUsage { index } => KeymapProblemView::NoUsage {
            key: key_ref_index(index),
        },
    }
}

/// A value that cannot be built.
pub(crate) fn build_problem(error: &WriteBuildError) -> KeymapProblemView {
    match error {
        WriteBuildError::NotOffered { what, value } => KeymapProblemView::NotOffered {
            what: (*what).to_string(),
            value: *value,
        },
        WriteBuildError::InvalidCombo { reason } => KeymapProblemView::InvalidCombo {
            reason: (*reason).to_string(),
        },
        WriteBuildError::MacroTooLong { steps, max } => KeymapProblemView::MacroTooLong {
            steps: sat16(*steps),
            max: sat16(*max),
        },
        WriteBuildError::UnbalancedMacro { step, reason } => KeymapProblemView::MacroUnbalanced {
            step: sat16(*step),
            reason: (*reason).to_string(),
        },
        WriteBuildError::MacroLastDelay => KeymapProblemView::MacroLastDelay,
        WriteBuildError::InvalidMacroCount => KeymapProblemView::MacroTimesZero,
        WriteBuildError::DksPointOutOfRange { value, max } => KeymapProblemView::DksPoint {
            value: *value,
            max: *max,
        },
        WriteBuildError::DksSteps { row, reason } => KeymapProblemView::DksSteps {
            row: sat8(u32::try_from(*row).unwrap_or(u32::MAX)),
            reason: (*reason).to_string(),
        },
        WriteBuildError::MtThresholdOutOfRange { value, min, max } => {
            KeymapProblemView::MtHoldTime {
                value_ms: u16::from(*value) * 10,
                min_ms: u16::from(*min) * 10,
                max_ms: u16::from(*max) * 10,
            }
        }
        WriteBuildError::Placement(error) => placement_problem(error),
        WriteBuildError::UnknownModel { .. } => KeymapProblemView::UnknownModel,
        other => KeymapProblemView::Other {
            message: other.to_string(),
        },
    }
}

/// A library refusal of a keymap edit as an [`AppError`].
pub(crate) fn keymap_error(error: &EditError) -> AppError {
    let with = |problem: KeymapProblemView| AppError {
        kind: app_kind(error.kind()),
        detail: Some(ErrorDetail::Keymap { problem }),
        message: format!("{error}; nothing was written"),
    };
    match error {
        EditError::Invalid { reason } => with(KeymapProblemView::Other {
            message: reason.clone(),
        }),
        EditError::Placement(placement) => with(placement_problem(placement)),
        EditError::NoFreeSlot { kind } => with(KeymapProblemView::NoFreeSlot {
            kind: slot_kind(*kind),
        }),
        EditError::SlotAlreadyBound { slot, by } => with(KeymapProblemView::SlotAlreadyBound {
            slot: slot_table(*slot),
            by: binding(by.0, by.1),
        }),
        EditError::BindingToEmptySlot { layer, index, slot } => {
            with(KeymapProblemView::BindingToEmptySlot {
                binding: binding(*layer, *index),
                slot: slot_table(*slot),
            })
        }
        EditError::SlotStillReferenced { slot, by } => {
            with(KeymapProblemView::SlotStillReferenced {
                slot: slot_table(*slot),
                by: by.iter().map(|(l, i)| binding(*l, *i)).collect(),
            })
        }
        EditError::UnsupportedCurrentTable { table, reason } => {
            with(KeymapProblemView::UnsupportedCurrentTable {
                table: table_ref(*table),
                reason: clean(reason),
            })
        }
        EditError::DistanceUnit { unit_um } => AppError::with(
            app_kind(error.kind()),
            ErrorDetail::DistanceUnit { unit_um: *unit_um },
            format!("{error}; nothing was written"),
        ),
        EditError::Build(build) => with(build_problem(build)),
        EditError::Session(e) => AppError::from(e),
        EditError::Backup(e) => AppError::from(e),
        EditError::Restore(e) => AppError::from(e),
    }
}

/// A layer view as the library's layer.
pub(crate) fn layer_of(layer: LayerView) -> Layer {
    match layer {
        LayerView::Base => Layer::Base,
        LayerView::Fn => Layer::Fn,
    }
}

#[cfg(test)]
mod tests {
    use openwraith_hid::protocol::HidUsage;

    use super::*;

    fn usage(u: u8) -> HidUsage {
        HidUsage::new(u).unwrap()
    }

    #[test]
    fn action_ids_are_stable() {
        let combo =
            openwraith_hid::protocol::Combo::new(&[usage(0xE0), usage(0xE1)], usage(0x04)).unwrap();
        for (assignment, id) in [
            (KeyAssignment::Default, "default"),
            (KeyAssignment::Key(usage(0x04)), "key:04"),
            (
                KeyAssignment::Media(openwraith_hid::protocol::MediaKey::new(0x12).unwrap()),
                "media:12",
            ),
            (
                KeyAssignment::Mouse(MouseAction::Button(MouseButton::Back)),
                "mouse:back",
            ),
            (KeyAssignment::Mouse(MouseAction::WheelUp), "mouse:wheel_up"),
            (KeyAssignment::Combo(combo), "combo:e0+e1+04"),
            (
                KeyAssignment::Macro(MacroSlotId::new(3).unwrap()),
                "macro:3",
            ),
            (
                KeyAssignment::Advanced(AdvancedSlotId::new(12).unwrap()),
                "advanced:12",
            ),
        ] {
            assert_eq!(action_id(&action_value(&assignment)), id);
        }
        for view in [
            MouseActionView::Left,
            MouseActionView::Right,
            MouseActionView::Middle,
            MouseActionView::Back,
            MouseActionView::Forward,
            MouseActionView::WheelUp,
            MouseActionView::WheelDown,
        ] {
            assert_eq!(mouse_view(mouse_action(view)), view);
        }
    }

    #[test]
    fn content_hash_ignores_nothing_but_the_slot() {
        let a = usage(0x04);
        let program = |delay| {
            MacroProgram::new(
                MacroPlay::Once,
                vec![MacroStep::press(a, delay), MacroStep::release(a, 0)],
            )
            .unwrap()
        };
        assert_eq!(content_hash(&program(30)), content_hash(&program(30)));
        assert_ne!(content_hash(&program(30)), content_hash(&program(31)));
        assert_eq!(content_hash(&program(30)).len(), 16);
        let times = MacroProgram::new(
            MacroPlay::Times(2),
            vec![MacroStep::press(a, 30), MacroStep::release(a, 0)],
        )
        .unwrap();
        assert_ne!(content_hash(&program(30)), content_hash(&times));
    }
}
