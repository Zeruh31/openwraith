//! Keymap-group edits (keys, macros, advanced keys) and the Keys page's view
//! of the group, end to end on the simulator.

use openwraith_hid::DeviceSim;
use openwraith_hid::SimTable;
use openwraith_hid::protocol::write::{MacroPlay, MacroProgram, MacroStep};
use openwraith_hid::protocol::{HidUsage, Layer, MacroSlotId};

use super::harness::{Harness, profile};
use crate::dto::{
    ActionValue, AdvancedDefView, AdvancedSlotContentView, AppErrorKind, BrokenKindView,
    BuiltinAckIntent, CommitTriggerReasonView, DksRowView, DksStepView, EntryOriginView,
    ErrorDetail, EvidenceView, Headline, ItemResultView, KeyKindView, KeyMotionView,
    KeymapChangeIntent, KeymapProblemView, KeymapWarningView, LayerView, MacroPlayView,
    MacroProgramView, MacroSlotContentView, MacroStepView, MouseActionView, OpKind, OutcomeView,
    PersistenceView, PlanInvalidation, PreservedKindView, ProgressStep, ResendReasonView,
    SlotContentView, TableRef,
};

pub(crate) const F1: u8 = 2;
pub(crate) const F2: u8 = 3;
pub(crate) const GRAVE: u8 = 22;
pub(crate) const TWO: u8 = 24;
pub(crate) const HOME: u8 = 37;
pub(crate) const Q: u8 = 45;
pub(crate) const W: u8 = 46;
pub(crate) const E: u8 = 47;
pub(crate) const R: u8 = 48;
pub(crate) const CAPS: u8 = 66;
pub(crate) const A: u8 = 68;
pub(crate) const G: u8 = 72;
pub(crate) const H: u8 = 73;
pub(crate) const J: u8 = 74;
pub(crate) const SPACE: u8 = 116;
pub(crate) const FN: u8 = 121;
pub(crate) const KNOB: u8 = 15;

fn usage(u: u8) -> HidUsage {
    HidUsage::new(u).unwrap()
}

/// A macro that types `key` (press, `delay` ms, release).
pub(crate) fn tap_program(key: u8, delay: u16, play: MacroPlay) -> MacroProgram {
    MacroProgram::new(
        play,
        vec![
            MacroStep::press(usage(key), delay),
            MacroStep::release(usage(key), 0),
        ],
    )
    .unwrap()
}

/// The same as a view.
pub(crate) fn tap_view(key: u8, delay: u16) -> MacroProgramView {
    MacroProgramView {
        play: MacroPlayView::Once,
        steps: vec![
            MacroStepView {
                usage: key,
                motion: KeyMotionView::Press,
                delay_after_ms: delay,
            },
            MacroStepView {
                usage: key,
                motion: KeyMotionView::Release,
                delay_after_ms: 0,
            },
        ],
    }
}

/// Sets one keymap entry of profile 1.
pub(crate) fn set_entry(s: &mut DeviceSim, layer: Layer, index: u8, entry: [u8; 4]) {
    let mut table = s
        .table(profile(1), SimTable::Keymap(layer))
        .unwrap()
        .to_vec();
    let at = usize::from(index) * 4;
    table[at..at + 4].copy_from_slice(&entry);
    s.set_table(profile(1), SimTable::Keymap(layer), table);
}

/// Sets macro slot `slot` of profile 1.
pub(crate) fn set_macro(s: &mut DeviceSim, slot: u8, program: &MacroProgram) {
    let id = MacroSlotId::new(slot).unwrap();
    s.set_table(
        profile(1),
        SimTable::Macro(id),
        program.encode(id).to_bytes().to_vec(),
    );
}

/// Sets advanced-key slot `slot` of profile 1 (its first bytes).
pub(crate) fn set_advanced(s: &mut DeviceSim, slot: u8, bytes: &[u8]) {
    let mut table = s
        .table(profile(1), SimTable::AdvancedKeys)
        .unwrap()
        .to_vec();
    let at = usize::from(slot) * 40;
    table[at..at + 40].fill(0);
    table[at..at + bytes.len()].copy_from_slice(bytes);
    s.set_table(profile(1), SimTable::AdvancedKeys, table);
}

/// c16's DKS on E: E tap at stage 1, R tap at stage 2, 1.5 mm.
pub(crate) const DKS_E: [u8; 21] = [
    0x01, 0x0f, 0x1e, 0x1e, 0x0f, 0x00, 0x08, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x15, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x00,
];
/// Mod-Tap on G: tap G, hold L-Ctrl after 300 ms.
pub(crate) const MT_G: [u8; 10] = [0x02, 0x00, 0xe0, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x1e];
/// Toggle of H.
pub(crate) const TGL_H: [u8; 5] = [0x03, 0x00, 0x0b, 0x00, 0x00];

/// Profile 1 with a keymap group of every kind: remaps on both layers, a
/// built-in replaced on the Fn layer, a macro bound on the Fn layer, an
/// orphan macro, an entry kept as read, DKS / Mod-Tap / Toggle slots, a
/// slot of another type and a key bound to an empty macro slot.
pub(crate) fn keys_seed(s: &mut DeviceSim) {
    set_entry(s, Layer::Base, CAPS, [0x00, 0xE0, 0x00, 0x00]);
    set_entry(s, Layer::Base, GRAVE, [0x07, 0x12, 0x00, 0x00]);
    set_entry(s, Layer::Fn, W, [0x00, 0x52, 0x00, 0x00]);
    set_entry(s, Layer::Fn, Q, [0x10, 0x14, 0x02, 0x00]);
    set_entry(s, Layer::Fn, F1, [0x13, 0xE0, 0x06, 0x00]);
    set_entry(s, Layer::Base, E, [0x17, 0x00, 0x00, 0x00]);
    set_entry(s, Layer::Base, G, [0x17, 0x01, 0x01, 0x00]);
    set_entry(s, Layer::Base, H, [0x17, 0x02, 0x02, 0x00]);
    set_entry(s, Layer::Base, HOME, [0x02, 0x02, 0x00, 0x00]);
    set_entry(s, Layer::Base, J, [0x10, 0x0D, 0x05, 0x00]);
    set_macro(s, 2, &tap_program(0x04, 30, MacroPlay::Once));
    set_macro(s, 3, &tap_program(0x06, 0, MacroPlay::Times(3)));
    set_advanced(s, 0, &DKS_E);
    set_advanced(s, 1, &MT_G);
    set_advanced(s, 2, &TGL_H);
    set_advanced(s, 5, &[0x04, 0x00, 0x04, 0x00, 0x00, 0x05]);
}

/// A ready keyboard with [`keys_seed`].
pub(crate) fn seeded() -> Harness {
    let mut h = Harness::new(1);
    h.with_sim(keys_seed);
    h.until_ready();
    h
}

/// Profile 1 with macros in slots 1 and 2, bound to Q (base) and W (Fn).
pub(crate) fn two_macros(s: &mut DeviceSim) {
    set_macro(s, 1, &tap_program(0x04, 20, MacroPlay::Once));
    set_macro(s, 2, &tap_program(0x05, 20, MacroPlay::Toggle));
    set_entry(s, Layer::Base, Q, [0x10, 0x14, 0x01, 0x00]);
    set_entry(s, Layer::Fn, W, [0x10, 0x1A, 0x02, 0x00]);
}

fn assign(layer: LayerView, key: u8, action: ActionValue) -> KeymapChangeIntent {
    KeymapChangeIntent::Assign { layer, key, action }
}

fn key(u: u8) -> ActionValue {
    ActionValue::Key { usage: u }
}

fn problem(error: &crate::dto::AppError) -> &KeymapProblemView {
    match &error.detail {
        Some(ErrorDetail::Keymap { problem }) => problem,
        other => panic!("not a keymap refusal: {other:?} ({})", error.message),
    }
}

#[test]
fn the_keys_state_shows_every_kind_of_entry_and_slot() {
    let h = seeded();
    let keys = h.keys();
    assert!(keys.editable && keys.advanced_editable);
    assert_eq!(keys.profile, 1);
    assert_eq!(keys.revision, h.snapshot().revision);
    assert_eq!(keys.keys.len(), 82);
    let find = |index: u8| keys.keys.iter().find(|k| k.key.index == index).unwrap();
    // Remaps on both layers.
    let caps = find(CAPS);
    assert_eq!(caps.base.origin, EntryOriginView::Remapped);
    assert_eq!(caps.base.id, "key:e0");
    assert_eq!(caps.base.hex, "00e00000");
    assert_eq!(caps.base.evidence, Some(EvidenceView::Confirmed));
    assert_eq!(caps.fn_layer.origin, EntryOriginView::Default);
    assert_eq!(find(W).fn_layer.id, "key:52");
    assert_eq!(find(GRAVE).base.label, "Mute");
    // A shortcut replacing the built-in of Fn + F1.
    let f1 = find(F1);
    assert_eq!(f1.fn_layer.id, "combo:e0+06");
    let builtin = f1.builtin.as_ref().unwrap();
    assert_eq!(builtin.function, "media player");
    assert!(!builtin.critical && builtin.label_confirmed);
    assert!(find(TWO).builtin.as_ref().unwrap().critical);
    assert!(find(SPACE).builtin.as_ref().unwrap().critical);
    // The knob and the Fn key are never assignable.
    assert_eq!(find(KNOB).kind, KeyKindView::Knob);
    assert!(!find(KNOB).assignable_base && !find(KNOB).assignable_fn);
    assert_eq!(find(FN).kind, KeyKindView::FnKey);
    assert!(!find(FN).assignable_base && !find(FN).assignable_fn);
    assert!(find(A).assignable_fn && find(A).macro_capable);
    // The knob's entries 15-21 on both layers, kept as read.
    let knob: Vec<u8> = keys.knob.iter().map(|k| k.index).collect();
    assert_eq!(
        knob,
        [15, 15, 16, 16, 17, 17, 18, 18, 19, 19, 20, 20, 21, 21]
    );
    // A macro bound on the Fn layer.
    let q = find(Q);
    assert_eq!(q.fn_layer.origin, EntryOriginView::MacroSlot { slot: 2 });
    assert_eq!(q.fn_layer.evidence, Some(EvidenceView::Confirmed));
    let slot2 = &keys.macros[2];
    assert_eq!(slot2.bindings.len(), 1);
    assert_eq!(slot2.bindings[0].layer, LayerView::Fn);
    assert!(!slot2.orphan);
    let MacroSlotContentView::Program {
        program,
        content_hash,
        ..
    } = &slot2.content
    else {
        panic!("a program in slot 2");
    };
    assert_eq!(program, &tap_view(0x04, 30));
    assert_eq!(content_hash.len(), 16);
    // An orphan macro.
    assert!(keys.macros[3].orphan);
    assert!(matches!(
        keys.macros[3].content,
        MacroSlotContentView::Program {
            program: MacroProgramView {
                play: MacroPlayView::Times { count: 3 },
                ..
            },
            ..
        }
    ));
    // An entry kept as read.
    let home = find(HOME);
    assert_eq!(home.base.origin, EntryOriginView::Preserved);
    assert_eq!(home.base.id, "preserved:02020000");
    assert_eq!(home.base.evidence, None);
    assert_eq!(
        home.base.preserved.as_ref().unwrap().kind,
        PreservedKindView::Lighting
    );
    // DKS, Mod-Tap and Toggle in UI units, and a slot of another type.
    assert_eq!(keys.advanced_used, 4);
    let AdvancedSlotContentView::Def { def, evidence } = &keys.advanced[0].content else {
        panic!("a DKS in slot 0");
    };
    assert_eq!(*evidence, EvidenceView::Confirmed);
    assert_eq!(
        def,
        &AdvancedDefView::Dks {
            first_point: 15,
            rows: vec![
                Some(DksRowView {
                    key: 0x08,
                    steps: vec![
                        DksStepView::Tap,
                        DksStepView::None,
                        DksStepView::None,
                        DksStepView::None
                    ],
                }),
                Some(DksRowView {
                    key: 0x15,
                    steps: vec![
                        DksStepView::None,
                        DksStepView::Tap,
                        DksStepView::None,
                        DksStepView::None
                    ],
                }),
                None,
                None,
            ],
        }
    );
    assert!(matches!(
        &keys.advanced[1].content,
        AdvancedSlotContentView::Def {
            def: AdvancedDefView::ModTap {
                tap: 0x0A,
                hold: 0xE0,
                hold_ms: 300
            },
            ..
        }
    ));
    assert!(matches!(
        &keys.advanced[2].content,
        AdvancedSlotContentView::Def {
            def: AdvancedDefView::Toggle { key: 0x0B },
            ..
        }
    ));
    assert!(matches!(
        keys.advanced[5].content,
        AdvancedSlotContentView::KeptAsRead { type_byte: 4, .. }
    ));
    assert!(keys.advanced[5].orphan);
    assert_eq!(keys.advanced[0].bindings[0].key.name, "E");
    // A key bound to an empty macro slot.
    assert_eq!(keys.broken.len(), 1);
    let broken = &keys.broken[0];
    assert_eq!(broken.key.index, J);
    assert_eq!(
        broken.problem,
        BrokenKindView::EmptySlot {
            slot: TableRef::Macro { slot: 5 }
        }
    );
    assert!(broken.resettable);
    assert_eq!(keys.limits.dks_first_point_max, Some(30));
    assert_eq!(
        (keys.limits.mt_hold_ms_min, keys.limits.mt_hold_ms_max),
        (150, 1000)
    );
    assert_eq!(keys.limits.macro_max_steps, 40);
    // Reading the view sent nothing.
    assert_eq!(h.writes(), 0);
}

#[test]
fn assignments_on_both_layers_are_written_in_the_group_order_after_a_safety_backup() {
    let mut h = Harness::ready();
    let before = h.keys();
    let intent = h.keymap_intent(vec![
        assign(LayerView::Base, W, key(0x04)),
        assign(
            LayerView::Fn,
            R,
            ActionValue::Mouse {
                action: MouseActionView::Back,
            },
        ),
    ]);
    let preview = h.prepare_keymap(intent).unwrap();
    assert_eq!(h.writes(), 0, "prepare sends nothing");
    assert_eq!(preview.entry_changes.len(), 2);
    let w = &preview.entry_changes[0];
    assert_eq!((w.layer, w.key.index), (LayerView::Base, W));
    assert_eq!(w.before.origin, EntryOriginView::Default);
    assert_eq!(w.after.id, "key:04");
    assert_eq!(w.after.hex, "00040000");
    let tables: Vec<TableRef> = preview.writes.iter().map(|w| w.table).collect();
    assert_eq!(
        tables,
        [
            TableRef::Keymap {
                layer: LayerView::Base
            },
            TableRef::Keymap {
                layer: LayerView::Fn
            }
        ]
    );
    // The Fn keymap is the commit trigger of the base keymap (it changes
    // too, so it is not a re-send).
    assert_eq!(
        preview.writes[1].commit_trigger,
        Some(CommitTriggerReasonView::BaseKeymap)
    );
    assert!(!preview.writes[1].resent_unchanged);
    assert!(!preview.persistence.will_run && !preview.persistence.requested);
    assert_eq!(preview.persistence.away_profile, 2);
    let watch = h.watch();
    let report = h
        .apply_keymap(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(report.items.len(), 2);
    assert!(report.items.iter().all(|i| matches!(
        i.result,
        ItemResultView::Written {
            persistence: PersistenceView::NotChecked,
            ..
        }
    )));
    assert!(report.persistence.is_none());
    assert!(report.cleared_macros.is_empty() && report.dangling.is_empty());
    assert!(!report.cancel_requested);
    // The keyboard saved both layers.
    let saved = |layer| {
        h.with_sim(|s| {
            s.persisted_table(profile(1), SimTable::Keymap(layer))
                .unwrap()
                .to_vec()
        })
    };
    assert_eq!(
        saved(Layer::Base)[usize::from(W) * 4..][..4],
        [0, 0x04, 0, 0]
    );
    assert_eq!(
        saved(Layer::Fn)[usize::from(R) * 4..][..4],
        [0x12, 0, 0x08, 0]
    );
    // The safety backup, the journal, no profile switch, a fresh view.
    let files = h.safety_files();
    assert_eq!(files.len(), 1);
    assert!(
        files[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-set-keys.owbackup.json")
    );
    assert_eq!(*watch.journal_seen.lock().unwrap(), Some(true));
    assert!(!h.worker.config.journal_path().exists());
    assert!(h.selects().is_empty());
    assert!(
        watch
            .steps()
            .iter()
            .any(|s| matches!(s, ProgressStep::RefreshingView))
    );
    let after = h.keys();
    assert!(after.revision > before.revision);
    let w_now = after.keys.iter().find(|k| k.key.index == W).unwrap();
    assert_eq!(w_now.base.id, "key:04");
    assert!(matches!(
        h.shared.last_report(),
        Some(crate::dto::LastReport::Edit { .. })
    ));
    h.assert_clean();
}

#[test]
fn nothing_is_written_without_a_confirmed_plan_and_a_token_is_single_use() {
    let mut h = Harness::ready();
    // An unknown token writes nothing.
    let error = h
        .apply_keymap(&crate::dto::PlanToken("p-0000".into()), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::StalePlan);
    let intent = h.keymap_intent(vec![assign(LayerView::Base, W, key(0x05))]);
    let preview = h.prepare_keymap(intent).unwrap();
    let token = Harness::token(&preview.token);
    assert_eq!(h.writes(), 0);
    h.apply_keymap(&token, &h.watch()).unwrap();
    let writes = h.writes();
    assert!(writes > 0);
    let again = h.apply_keymap(&token, &h.watch()).unwrap_err();
    assert_eq!(again.kind, AppErrorKind::StalePlan);
    assert_eq!(h.writes(), writes, "a used token writes nothing");
    // A trigger plan's token is not a keymap token.
    let trigger = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    let error = h
        .apply_keymap(&Harness::token(&trigger.token), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::StalePlan);
    assert_eq!(h.writes(), writes);
    // Nothing to change: no token.
    let same = h.keymap_intent(vec![assign(LayerView::Base, W, key(0x05))]);
    let preview = h.prepare_keymap(same).unwrap();
    assert!(preview.nothing_to_do && preview.token.is_none());
}

#[test]
fn a_view_from_another_read_is_refused() {
    let mut h = Harness::ready();
    let mut intent = h.keymap_intent(vec![assign(LayerView::Base, W, key(0x05))]);
    intent.based_on += 1;
    let error = h.prepare_keymap(intent).unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::PlanInvalidated {
            reason: PlanInvalidation::SnapshotChanged
        })
    );
    // A trigger edit changes the snapshot but not the keymap group: the
    // view the user edited stays valid.
    let keys = h.keys();
    let trigger = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    h.apply_trigger(&Harness::token(&trigger.token), &h.watch())
        .unwrap();
    assert!(h.snapshot().revision > keys.revision);
    assert_eq!(h.keys().revision, keys.revision);
    let mut intent = h.keymap_intent(vec![assign(LayerView::Base, W, key(0x05))]);
    intent.based_on = keys.revision;
    assert!(h.prepare_keymap(intent).unwrap().token.is_some());
}

#[test]
fn a_group_table_changed_since_the_plan_refuses_the_write() {
    // Every table the edit writes or depends on is compared: a layer, a
    // macro slot it does not even write, the advanced-key table.
    let changes: [fn(&mut DeviceSim); 3] = [
        |s| set_entry(s, Layer::Fn, R, [0x00, 0x05, 0x00, 0x00]),
        |s| set_macro(s, 7, &tap_program(0x07, 0, MacroPlay::Once)),
        |s| set_advanced(s, 9, &TGL_H),
    ];
    for change in changes {
        let mut h = Harness::ready();
        let revision = h.keys().revision;
        let intent = h.keymap_intent(vec![assign(LayerView::Base, W, key(0x05))]);
        let preview = h.prepare_keymap(intent).unwrap();
        h.with_sim(change);
        let watch = h.watch();
        let error = h
            .apply_keymap(&Harness::token(&preview.token), &watch)
            .unwrap_err();
        assert_eq!(
            error.detail,
            Some(ErrorDetail::PlanInvalidated {
                reason: PlanInvalidation::ValuesChanged
            })
        );
        assert_eq!(h.writes(), 0, "nothing written");
        assert!(h.safety_files().is_empty(), "no safety backup either");
        assert!(!h.worker.config.journal_path().exists());
        // The view shows the keyboard's state now.
        assert!(h.keys().revision > revision);
    }
}

#[test]
fn writing_macro_slot_0_resends_the_other_slots_right_after_it() {
    let mut h = Harness::new(1);
    h.with_sim(two_macros);
    h.until_ready();
    let intent = h.keymap_intent(vec![KeymapChangeIntent::SetMacro {
        slot: 0,
        program: tap_view(0x06, 10),
    }]);
    let preview = h.prepare_keymap(intent).unwrap();
    let tables: Vec<TableRef> = preview.writes.iter().map(|w| w.table).collect();
    assert_eq!(
        tables,
        [
            TableRef::Macro { slot: 0 },
            TableRef::Macro { slot: 1 },
            TableRef::Macro { slot: 2 }
        ]
    );
    assert_eq!(preview.writes[0].resent, None);
    for write in &preview.writes[1..] {
        assert_eq!(write.resent, Some(ResendReasonView::MacroSlot0Clears));
        assert!(write.follows_macro_slot_0 && write.completes_binding);
    }
    // Macro slots are kept without the Fn keymap.
    assert!(preview.writes.iter().all(|w| w.commit_trigger.is_none()));
    // Only slot 0's content changes.
    assert_eq!(preview.slot_changes.len(), 1);
    assert_eq!(preview.slot_changes[0].slot, TableRef::Macro { slot: 0 });
    assert_eq!(preview.slot_changes[0].before, SlotContentView::Empty);
    assert_eq!(
        preview.slot_changes[0].after,
        SlotContentView::Macro {
            program: tap_view(0x06, 10)
        }
    );
    let report = h
        .apply_keymap(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(
        report.items[1].resent,
        Some(ResendReasonView::MacroSlot0Clears)
    );
    let keys = h.keys();
    for slot in [0, 1, 2] {
        assert!(
            matches!(
                keys.macros[slot].content,
                MacroSlotContentView::Program { .. }
            ),
            "slot {slot}"
        );
    }
    assert!(keys.broken.is_empty());
    h.assert_clean();
}

#[test]
fn a_stop_after_slot_0_still_resends_the_other_slots() {
    let mut h = Harness::new(1);
    h.with_sim(two_macros);
    h.until_ready();
    let intent = h.keymap_intent(vec![
        KeymapChangeIntent::SetMacro {
            slot: 0,
            program: tap_view(0x06, 10),
        },
        assign(LayerView::Base, GRAVE, key(0x29)),
    ]);
    let preview = h.prepare_keymap(intent).unwrap();
    let watch = h.watch();
    watch.cancel_when(|e| {
        matches!(
            e.step,
            ProgressStep::Writing {
                table: TableRef::Macro { slot: 0 },
                packet,
                packets,
                ..
            } if packet == packets && packet > 0
        )
    });
    let report = h
        .apply_keymap(&Harness::token(&preview.token), &watch)
        .unwrap();
    // The re-sends went out; the unrelated layer change did not.
    assert!(report.cancel_requested);
    assert_eq!(report.outcome, OutcomeView::Cancelled, "{report:?}");
    let results: Vec<(TableRef, bool)> = report
        .items
        .iter()
        .map(|i| (i.table, matches!(i.result, ItemResultView::Written { .. })))
        .collect();
    assert_eq!(
        results,
        [
            (TableRef::Macro { slot: 0 }, true),
            (TableRef::Macro { slot: 1 }, true),
            (TableRef::Macro { slot: 2 }, true),
            (
                TableRef::Keymap {
                    layer: LayerView::Base
                },
                false
            ),
            (
                TableRef::Keymap {
                    layer: LayerView::Fn
                },
                false
            ),
        ]
    );
    assert!(report.cleared_macros.is_empty() && report.dangling.is_empty());
    let saved = h.with_sim(|s| {
        s.persisted_table(profile(1), SimTable::Macro(MacroSlotId::new(2).unwrap()))
            .unwrap()
            .to_vec()
    });
    assert_eq!(
        saved,
        tap_program(0x05, 20, MacroPlay::Toggle)
            .encode(MacroSlotId::new(2).unwrap())
            .to_bytes()
    );
    // The progress said that Stop finishes the keymap group.
    assert!(
        watch
            .all()
            .iter()
            .any(|e| e.cancel == crate::dto::CancelState::FinishingKeymapGroup)
    );
    h.assert_clean();
}

#[test]
fn a_failure_after_slot_0_lists_the_emptied_macros_and_the_keys_left_on_them() {
    let mut h = Harness::new(1);
    h.with_sim(two_macros);
    h.until_ready();
    h.with_sim(|s| s.freeze(SimTable::Macro(MacroSlotId::new(1).unwrap())));
    let intent = h.keymap_intent(vec![KeymapChangeIntent::SetMacro {
        slot: 0,
        program: tap_view(0x06, 10),
    }]);
    let preview = h.prepare_keymap(intent).unwrap();
    let report = h
        .apply_keymap(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.outcome, OutcomeView::Failed, "{report:?}");
    assert_eq!(report.headline, Headline::Failed);
    let cleared: Vec<u8> = report.cleared_macros.iter().map(|c| c.slot).collect();
    assert_eq!(cleared, [1, 2]);
    let dangling: Vec<(LayerView, u8, TableRef)> = report
        .dangling
        .iter()
        .map(|d| (d.layer, d.key.index, d.slot))
        .collect();
    assert_eq!(
        dangling,
        [
            (LayerView::Base, Q, TableRef::Macro { slot: 1 }),
            (LayerView::Fn, W, TableRef::Macro { slot: 2 })
        ]
    );
    // Undo is the safety backup.
    assert_eq!(
        report.safety_backup.display_path,
        h.safety_files()[0].display().to_string()
    );
    // The view after the edit shows both keys broken.
    assert_eq!(h.keys().broken.len(), 2);
}

#[test]
fn critical_built_ins_need_the_critical_acknowledgement() {
    let mut h = Harness::ready();
    let change = || vec![assign(LayerView::Fn, TWO, key(0x04))];
    let error = h.prepare_keymap(h.keymap_intent(change())).unwrap_err();
    let KeymapProblemView::BuiltinNotAcknowledged {
        key: refused_key,
        function,
        critical,
    } = problem(&error)
    else {
        panic!("{error:?}");
    };
    assert_eq!(
        (refused_key.name.as_str(), function.as_str(), *critical),
        ("2", "profile 2", true)
    );
    assert!(error.message.contains("Fn + 2 (#24)"), "{}", error.message);
    // The plain acknowledgement is not enough.
    let mut intent = h.keymap_intent(change());
    intent.builtin_acks = vec![BuiltinAckIntent {
        key: TWO,
        critical: false,
    }];
    assert!(matches!(
        problem(&h.prepare_keymap(intent).unwrap_err()),
        KeymapProblemView::BuiltinNotAcknowledged { critical: true, .. }
    ));
    let mut intent = h.keymap_intent(change());
    intent.builtin_acks = vec![BuiltinAckIntent {
        key: TWO,
        critical: true,
    }];
    let preview = h.prepare_keymap(intent).unwrap();
    assert!(preview.warnings.iter().any(|w| matches!(
        w,
        KeymapWarningView::FnBuiltinOverridden { critical: true, key, .. } if key.index == TWO
    )));
    // A non-critical built-in needs an acknowledgement too; the default
    // brings it back without one.
    let error = h
        .prepare_keymap(h.keymap_intent(vec![assign(LayerView::Fn, F2, key(0x04))]))
        .unwrap_err();
    assert!(matches!(
        problem(&error),
        KeymapProblemView::BuiltinNotAcknowledged {
            critical: false,
            ..
        }
    ));
    assert_eq!(h.writes(), 0);
}

#[test]
fn the_persistence_check_runs_only_when_asked_for() {
    let mut h = Harness::ready();
    let mut intent = h.keymap_intent(vec![assign(LayerView::Base, W, key(0x05))]);
    intent.check_persistence = true;
    let preview = h.prepare_keymap(intent).unwrap();
    assert!(preview.persistence.requested && preview.persistence.will_run);
    assert!(!preview.persistence.forced);
    // The Fn keymap re-sent unchanged as the commit trigger is not checked.
    assert_eq!(
        preview.persistence.tables,
        [TableRef::Keymap {
            layer: LayerView::Base
        }]
    );
    let report = h
        .apply_keymap(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    let check = report.persistence.as_ref().unwrap();
    assert_eq!(check.profiles, [1]);
    assert_eq!((check.persisted, check.lost), (1, 0));
    assert!(matches!(
        report.items[0].result,
        ItemResultView::Written {
            persistence: PersistenceView::Persisted,
            ..
        }
    ));
    assert_eq!(h.selects(), [2, 1]);
}

#[test]
fn the_journal_names_the_keymap_edit_while_it_writes() {
    let mut h = Harness::ready();
    let intent = h.keymap_intent(vec![assign(LayerView::Base, W, key(0x05))]);
    let preview = h.prepare_keymap(intent).unwrap();
    let watch = h.watch();
    let journal = h.worker.config.journal_path();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let seen_in = std::sync::Arc::clone(&seen);
    watch.on_event(move |event, _| {
        if matches!(event.step, ProgressStep::Writing { .. }) && seen_in.lock().unwrap().is_none() {
            *seen_in.lock().unwrap() = Some(std::fs::read_to_string(&journal).unwrap());
        }
    });
    h.apply_keymap(&Harness::token(&preview.token), &watch)
        .unwrap();
    let text = seen.lock().unwrap().clone().unwrap();
    let entry: crate::ops::journal::JournalEntry = serde_json::from_str(&text).unwrap();
    assert_eq!(entry.op, OpKind::KeymapEdit);
    assert_eq!(entry.profiles, [1]);
    assert!(
        entry
            .safety_backup_path
            .ends_with("-set-keys.owbackup.json")
    );
    assert!(!h.worker.config.journal_path().exists());
}

#[test]
fn advanced_keys_are_written_after_the_fn_keymap_and_deleted_with_their_bindings() {
    let mut h = Harness::ready();
    let dks = AdvancedDefView::Dks {
        first_point: 15,
        rows: vec![
            Some(DksRowView {
                key: 0x08,
                steps: vec![
                    DksStepView::Tap,
                    DksStepView::None,
                    DksStepView::None,
                    DksStepView::None,
                ],
            }),
            None,
            None,
            None,
        ],
    };
    let intent = h.keymap_intent(vec![KeymapChangeIntent::BindAdvanced {
        layer: LayerView::Base,
        key: E,
        def: dks.clone(),
        slot: None,
    }]);
    let preview = h.prepare_keymap(intent).unwrap();
    let tables: Vec<TableRef> = preview.writes.iter().map(|w| w.table).collect();
    assert_eq!(
        tables,
        [
            TableRef::Keymap {
                layer: LayerView::Base
            },
            TableRef::Keymap {
                layer: LayerView::Fn
            },
            TableRef::AdvancedKey { slot: 0 }
        ]
    );
    assert!(preview.writes[1].resent_unchanged);
    assert!(preview.writes[2].completes_binding);
    assert_eq!(
        preview.slot_changes[0].after,
        SlotContentView::Advanced { def: dks.clone() }
    );
    assert!(
        preview
            .warnings
            .iter()
            .any(|w| matches!(w, KeymapWarningView::TransientBinding { .. }))
    );
    let report = h
        .apply_keymap(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    let keys = h.keys();
    assert_eq!(keys.advanced_used, 1);
    assert!(matches!(
        &keys.advanced[0].content,
        AdvancedSlotContentView::Def { def, .. } if *def == dks
    ));
    // A content-only change: the unchanged Fn keymap goes first.
    let mt = AdvancedDefView::ModTap {
        tap: 0x08,
        hold: 0xE0,
        hold_ms: 200,
    };
    let preview = h
        .prepare_keymap(h.keymap_intent(vec![KeymapChangeIntent::SetAdvanced { slot: 0, def: mt }]))
        .unwrap();
    assert_eq!(
        preview.writes[0].commit_trigger,
        Some(CommitTriggerReasonView::AdvancedKeys)
    );
    assert!(preview.writes[0].resent_unchanged);
    // 200 ms is not the tested hold time.
    assert!(preview.warnings.iter().any(|w| matches!(
        w,
        KeymapWarningView::Unverified { note } if note.question == "Q19"
    )));
    // Delete = unbind + clear in one write.
    let preview = h
        .prepare_keymap(h.keymap_intent(vec![KeymapChangeIntent::DeleteAdvanced { slot: 0 }]))
        .unwrap();
    assert_eq!(preview.entry_changes.len(), 1);
    assert_eq!(
        preview.entry_changes[0].after.origin,
        EntryOriginView::Default
    );
    let clear = preview.writes.last().unwrap();
    assert_eq!(clear.table, TableRef::AdvancedKey { slot: 0 });
    assert!(clear.clears);
    let report = h
        .apply_keymap(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(h.keys().advanced_used, 0);
    h.assert_clean();
}

#[test]
fn a_macro_bound_on_the_fn_layer_takes_the_lowest_free_slot_from_1() {
    let mut h = Harness::ready();
    let intent = h.keymap_intent(vec![KeymapChangeIntent::BindMacro {
        layer: LayerView::Fn,
        key: Q,
        program: tap_view(0x04, 30),
        slot: None,
    }]);
    let preview = h.prepare_keymap(intent).unwrap();
    assert_eq!(
        preview.entry_changes[0].after.origin,
        EntryOriginView::MacroSlot { slot: 1 }
    );
    assert_eq!(preview.slot_changes[0].slot, TableRef::Macro { slot: 1 });
    assert_eq!(
        preview.slot_changes[0].after,
        SlotContentView::Macro {
            program: tap_view(0x04, 30)
        }
    );
    // Macro slot first, then the Fn keymap (no base keymap).
    let tables: Vec<TableRef> = preview.writes.iter().map(|w| w.table).collect();
    assert_eq!(
        tables,
        [
            TableRef::Macro { slot: 1 },
            TableRef::Keymap {
                layer: LayerView::Fn
            }
        ]
    );
    let report = h
        .apply_keymap(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    let keys = h.keys();
    assert_eq!(keys.macros[1].bindings[0].layer, LayerView::Fn);
    // Reset of the key clears its macro (no orphan is left behind) ...
    let preview = h
        .prepare_keymap(h.keymap_intent(vec![KeymapChangeIntent::Reset {
            layer: LayerView::Fn,
            key: Q,
        }]))
        .unwrap();
    assert!(
        preview
            .slot_changes
            .iter()
            .any(|c| c.slot == TableRef::Macro { slot: 1 } && c.after == SlotContentView::Empty)
    );
    // ... unless the user keeps it.
    let mut intent = h.keymap_intent(vec![KeymapChangeIntent::Reset {
        layer: LayerView::Fn,
        key: Q,
    }]);
    intent.keep_orphans = true;
    let preview = h.prepare_keymap(intent).unwrap();
    assert!(preview.slot_changes.is_empty());
    assert!(preview.warnings.iter().any(|w| matches!(
        w,
        KeymapWarningView::OrphanSlotKept {
            slot: TableRef::Macro { slot: 1 }
        }
    )));
}

#[test]
fn swap_moves_entries_and_refuses_entries_kept_as_read() {
    let mut h = seeded();
    let preview = h
        .prepare_keymap(h.keymap_intent(vec![KeymapChangeIntent::Swap {
            layer: LayerView::Base,
            first: CAPS,
            second: GRAVE,
        }]))
        .unwrap();
    let after: Vec<(u8, String)> = preview
        .entry_changes
        .iter()
        .map(|c| (c.key.index, c.after.id.clone()))
        .collect();
    assert_eq!(
        after,
        [
            (GRAVE, "key:e0".to_string()),
            (CAPS, "media:12".to_string())
        ]
    );
    let error = h
        .prepare_keymap(h.keymap_intent(vec![KeymapChangeIntent::Swap {
            layer: LayerView::Base,
            first: HOME,
            second: A,
        }]))
        .unwrap_err();
    assert!(matches!(
        problem(&error),
        KeymapProblemView::SwapNotWritable { key, .. } if key.index == HOME
    ));
}

/// What a refusal must say.
type Expected = fn(&KeymapProblemView) -> bool;

#[test]
fn invalid_intents_are_refused_before_any_traffic() {
    let mut h = seeded();
    let before = h.packets();
    let cases: Vec<(Vec<KeymapChangeIntent>, Expected)> = vec![
        (vec![], |p| *p == KeymapProblemView::NoChanges),
        (vec![assign(LayerView::Base, 16, key(4))], |p| {
            *p == KeymapProblemView::NotAKey { index: 16 }
        }),
        (vec![assign(LayerView::Base, KNOB, key(4))], |p| {
            *p == KeymapProblemView::Knob
        }),
        (vec![assign(LayerView::Fn, FN, key(4))], |p| {
            *p == KeymapProblemView::FnKey
        }),
        (vec![assign(LayerView::Base, W, key(0x32))], |p| {
            matches!(p, KeymapProblemView::NotOffered { value: 0x32, .. })
        }),
        (
            vec![assign(
                LayerView::Base,
                W,
                ActionValue::Media { index: 0x17 },
            )],
            |p| matches!(p, KeymapProblemView::NotOffered { value: 0x17, .. }),
        ),
        (
            vec![assign(
                LayerView::Base,
                W,
                ActionValue::Combo {
                    modifiers: vec![0xE0, 0xE0],
                    key: 4,
                },
            )],
            |p| matches!(p, KeymapProblemView::InvalidCombo { .. }),
        ),
        (
            vec![assign(LayerView::Base, W, ActionValue::Macro { slot: 10 })],
            |p| matches!(p, KeymapProblemView::SlotOutOfRange { slot: 10, .. }),
        ),
        // Binding to an empty slot.
        (
            vec![assign(LayerView::Base, W, ActionValue::Macro { slot: 7 })],
            |p| matches!(p, KeymapProblemView::BindingToEmptySlot { .. }),
        ),
        // Slot 2 is already bound to Fn + Q.
        (
            vec![assign(LayerView::Base, W, ActionValue::Macro { slot: 2 })],
            |p| matches!(p, KeymapProblemView::SlotAlreadyBound { .. }),
        ),
        // Clearing a bound slot.
        (vec![KeymapChangeIntent::ClearMacro { slot: 2 }], |p| {
            matches!(p, KeymapProblemView::SlotStillReferenced { .. })
        }),
        (
            vec![KeymapChangeIntent::SetMacro {
                slot: 4,
                program: MacroProgramView {
                    play: MacroPlayView::Once,
                    steps: vec![MacroStepView {
                        usage: 4,
                        motion: KeyMotionView::Press,
                        delay_after_ms: 0,
                    }],
                },
            }],
            |p| matches!(p, KeymapProblemView::MacroUnbalanced { .. }),
        ),
        (
            vec![KeymapChangeIntent::SetMacro {
                slot: 4,
                program: MacroProgramView {
                    play: MacroPlayView::Times { count: 0 },
                    ..tap_view(4, 0)
                },
            }],
            |p| *p == KeymapProblemView::MacroTimesZero,
        ),
        (
            vec![KeymapChangeIntent::SetAdvanced {
                slot: 9,
                def: AdvancedDefView::Dks {
                    first_point: 15,
                    rows: vec![None],
                },
            }],
            |p| *p == KeymapProblemView::DksShape,
        ),
        (
            vec![KeymapChangeIntent::SetAdvanced {
                slot: 9,
                def: AdvancedDefView::Dks {
                    first_point: 31,
                    rows: vec![
                        Some(DksRowView {
                            key: 4,
                            steps: vec![
                                DksStepView::Tap,
                                DksStepView::None,
                                DksStepView::None,
                                DksStepView::None,
                            ],
                        }),
                        None,
                        None,
                        None,
                    ],
                },
            }],
            |p| *p == KeymapProblemView::DksPoint { value: 31, max: 30 },
        ),
        (
            vec![KeymapChangeIntent::SetAdvanced {
                slot: 9,
                def: AdvancedDefView::Dks {
                    first_point: 15,
                    rows: vec![
                        Some(DksRowView {
                            key: 4,
                            steps: vec![
                                DksStepView::Tap,
                                DksStepView::None,
                                DksStepView::None,
                                DksStepView::Hold,
                            ],
                        }),
                        None,
                        None,
                        None,
                    ],
                },
            }],
            |p| matches!(p, KeymapProblemView::DksSteps { row: 0, .. }),
        ),
        (
            vec![KeymapChangeIntent::SetAdvanced {
                slot: 9,
                def: AdvancedDefView::ModTap {
                    tap: 4,
                    hold: 0xE0,
                    hold_ms: 305,
                },
            }],
            |p| matches!(p, KeymapProblemView::MtHoldTime { value_ms: 305, .. }),
        ),
        (
            vec![KeymapChangeIntent::SetAdvanced {
                slot: 9,
                def: AdvancedDefView::ModTap {
                    tap: 4,
                    hold: 0xE0,
                    hold_ms: 1010,
                },
            }],
            |p| {
                *p == KeymapProblemView::MtHoldTime {
                    value_ms: 1010,
                    min_ms: 150,
                    max_ms: 1000,
                }
            },
        ),
        // A slot kept as read is not overwritten without the explicit
        // "not undoable" acknowledgement.
        (
            vec![KeymapChangeIntent::SetAdvanced {
                slot: 5,
                def: AdvancedDefView::Toggle { key: 4 },
            }],
            |p| {
                matches!(
                    p,
                    KeymapProblemView::UnsupportedCurrentTable {
                        table: TableRef::AdvancedKey { slot: 5 },
                        ..
                    }
                )
            },
        ),
    ];
    for (changes, expected) in cases {
        let error = h
            .prepare_keymap(h.keymap_intent(changes.clone()))
            .unwrap_err();
        assert!(expected(problem(&error)), "{changes:?}: {error:?}");
        assert!(
            matches!(
                error.kind,
                AppErrorKind::InvalidRequest | AppErrorKind::UnexpectedReply
            ),
            "{error:?}"
        );
    }
    let mut intent = h.keymap_intent(vec![KeymapChangeIntent::SetAdvanced {
        slot: 5,
        def: AdvancedDefView::Toggle { key: 4 },
    }]);
    intent.allow_not_undoable = true;
    let preview = h.prepare_keymap(intent).unwrap();
    assert!(preview.warnings.iter().any(|w| matches!(
        w,
        KeymapWarningView::NotUndoable {
            table: TableRef::AdvancedKey { slot: 5 }
        }
    )));
    assert_eq!(h.packets(), before, "nothing was sent");
    // Unknown fields are refused by the IPC layer.
    assert!(
        serde_json::from_value::<crate::dto::KeymapEditIntent>(serde_json::json!({
            "changes": [], "builtin_acks": [], "keep_orphans": false,
            "allow_not_undoable": false, "check_persistence": false, "based_on": 1,
            "extra": true
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<crate::dto::KeymapChangeIntent>(serde_json::json!({
            "change": "assign", "layer": "base", "key": 46,
            "action": {"kind": "key", "usage": 4, "bytes": "00040000"}
        }))
        .is_err()
    );
}

/// The base-layer origin of key `index` in the keys state.
fn base_origin(h: &Harness, index: u8) -> EntryOriginView {
    h.keys()
        .keys
        .iter()
        .find(|k| k.key.index == index)
        .map(|k| k.base.origin.clone())
        .expect("the key")
}

#[test]
fn deleting_a_keys_old_slot_after_drafting_a_new_advanced_key_on_it_keeps_the_new_one() {
    // G (base) uses advanced slot 1 (Mod-Tap). In the order the user made
    // them: a new DKS on G, then Delete on the slot-1 card.
    let mut h = seeded();
    let dks = AdvancedDefView::Dks {
        first_point: 15,
        rows: vec![
            Some(DksRowView {
                key: 0x0A,
                steps: vec![
                    DksStepView::Tap,
                    DksStepView::None,
                    DksStepView::None,
                    DksStepView::None,
                ],
            }),
            None,
            None,
            None,
        ],
    };
    let intent = h.keymap_intent(vec![
        KeymapChangeIntent::BindAdvanced {
            layer: LayerView::Base,
            key: G,
            def: dks.clone(),
            slot: None,
        },
        KeymapChangeIntent::DeleteAdvanced { slot: 1 },
    ]);
    let preview = h.prepare_keymap(intent).unwrap();
    let g = preview
        .entry_changes
        .iter()
        .find(|c| c.key.index == G)
        .expect("G changes");
    let EntryOriginView::AdvancedSlot { slot: new_slot } = g.after.origin else {
        panic!("G keeps an advanced key: {:?}", g.after.origin);
    };
    assert_ne!(new_slot, 1, "not the slot being deleted");
    assert!(
        preview
            .slot_changes
            .iter()
            .any(|c| c.slot == TableRef::AdvancedKey { slot: new_slot }
                && c.after == SlotContentView::Advanced { def: dks.clone() })
    );
    assert!(
        preview
            .slot_changes
            .iter()
            .any(|c| c.slot == TableRef::AdvancedKey { slot: 1 }
                && c.after == SlotContentView::Empty)
    );
    let report = h
        .apply_keymap(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(
        base_origin(&h, G),
        EntryOriginView::AdvancedSlot { slot: new_slot }
    );
    let keys = h.keys();
    assert!(matches!(
        &keys.advanced[usize::from(new_slot)].content,
        AdvancedSlotContentView::Def { def, .. } if *def == dks
    ));
    assert!(matches!(
        keys.advanced[1].content,
        AdvancedSlotContentView::Empty
    ));
    h.assert_clean();
}

#[test]
fn deleting_a_keys_old_slot_after_assigning_it_an_orphan_keeps_the_assignment() {
    // An orphan Toggle in slot 3; G (base) uses slot 1. Assign slot 3 to G,
    // then delete slot 1.
    let mut h = Harness::new(1);
    h.with_sim(|s| {
        keys_seed(s);
        set_advanced(s, 3, &TGL_H);
    });
    h.until_ready();
    let intent = h.keymap_intent(vec![
        assign(LayerView::Base, G, ActionValue::Advanced { slot: 3 }),
        KeymapChangeIntent::DeleteAdvanced { slot: 1 },
    ]);
    let preview = h.prepare_keymap(intent).unwrap();
    let report = h
        .apply_keymap(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(
        base_origin(&h, G),
        EntryOriginView::AdvancedSlot { slot: 3 }
    );
    assert!(matches!(
        h.keys().advanced[1].content,
        AdvancedSlotContentView::Empty
    ));
    h.assert_clean();
}

#[test]
fn a_new_macro_on_a_key_whose_macro_is_edited_in_the_same_edit_is_refused() {
    // Fn + Q runs macro slot 2 (only key). Slot 2 edited (C), and a new
    // macro (D) bound to Fn + Q: the library would reuse slot 2 for the new
    // one, and the edit of slot 2 would be lost.
    let mut h = seeded();
    let before = h.packets();
    let intent = h.keymap_intent(vec![
        KeymapChangeIntent::SetMacro {
            slot: 2,
            program: tap_view(0x06, 20),
        },
        KeymapChangeIntent::BindMacro {
            layer: LayerView::Fn,
            key: Q,
            program: tap_view(0x07, 20),
            slot: None,
        },
    ]);
    let error = h.prepare_keymap(intent).unwrap_err();
    assert_eq!(
        *problem(&error),
        KeymapProblemView::NewSlotReplacesEdit {
            layer: LayerView::Fn,
            key: crate::map::key_ref_index(Q),
            slot: TableRef::Macro { slot: 2 },
        }
    );
    assert_eq!(h.packets(), before, "nothing sent");
    // Deleting the slot instead frees the key: the new macro takes a free slot.
    let intent = h.keymap_intent(vec![
        KeymapChangeIntent::BindMacro {
            layer: LayerView::Fn,
            key: Q,
            program: tap_view(0x07, 20),
            slot: None,
        },
        KeymapChangeIntent::DeleteMacro { slot: 2 },
    ]);
    let preview = h.prepare_keymap(intent).unwrap();
    let q = preview
        .entry_changes
        .iter()
        .find(|c| c.key.index == Q && c.layer == LayerView::Fn)
        .expect("Fn + Q changes");
    assert!(matches!(
        q.after.origin,
        EntryOriginView::MacroSlot { slot } if slot != 2
    ));
}
