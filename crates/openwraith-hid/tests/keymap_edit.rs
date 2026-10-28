//! Keymap-group edits (key assignments, macros, advanced keys) against the
//! simulator: the site's order with the Fn keymap as commit trigger, the
//! captured slot and binding bytes (c13, c16), placement rules, slots,
//! cancellations, failures part-way and undo.

mod common;

use std::ops::ControlFlow;

use common::{c06_backup, consent, load, profile, selects, temp_dir, write_bytes};
use openwraith_hid::protocol::write::{Dks, DksRow, DksStep, MacroStep, ModTap, Toggle};
use openwraith_hid::protocol::{
    AdvancedKeyDef, AdvancedSlotId, BuiltinAck, Combo, EntryKind, HidUsage, KeyAssignment, KeyPos,
    Layer, MacroPlay, MacroProgram, MacroSlotId, MediaKey, MouseAction, PlacementError,
    ReadRequest, TableId,
};
use openwraith_hid::{
    AdvancedSlotView, Backup, BackupEntry, CommitTriggerMode, CommitTriggerReason, DeviceSim,
    EditError, EditOptions, EditReport, EditWarning, ErrorKind, ItemResult, KeymapChange,
    KeymapCommit, KeymapEdit, KeymapEditPlan, KeymapGroupState, MacroSlotView, Outcome,
    PersistenceCheckMode, RestoreOptions, SavedBackup, Session, SimTable, SlotRef, WriteProgress,
    apply_keymap_edit, execute_restore, plan_keymap_edit, prepare_restore, read_all,
};

const C13: &str = "capture_13_macro_ab_on_caps";
const C16: &str = "capture_16_advanced_keys";

fn sim() -> DeviceSim {
    DeviceSim::from_backup(&c06_backup())
}

fn usage(u: u8) -> HidUsage {
    HidUsage::new(u).expect("offered usage")
}

fn key(name: &str) -> KeyPos {
    openwraith_hid::protocol::W75
        .key_by_name(name)
        .and_then(|k| k.pos())
        .unwrap_or_else(|| panic!("no key {name}"))
}

fn captured(name: &str, indices: &[usize]) -> Vec<[u8; 63]> {
    let capture = load(name);
    indices
        .iter()
        .map(|index| {
            capture
                .out_packets()
                .find(|(i, _)| i == index)
                .map(|(_, bytes)| *bytes)
                .unwrap_or_else(|| panic!("no OUT packet #{index} in {name}"))
        })
        .collect()
}

fn safety(sim: &mut DeviceSim, dir: &tempfile::TempDir) -> SavedBackup {
    let backup = read_all(&mut Session::new(sim)).expect("backup");
    backup.save_safety(dir.path(), "set").expect("saved")
}

fn plan(sim: &mut DeviceSim, edit: &KeymapEdit) -> Result<KeymapEditPlan, EditError> {
    plan_keymap_edit(&mut Session::new(sim), edit)
}

fn apply(
    sim: &mut DeviceSim,
    plan: &KeymapEditPlan,
    saved: &SavedBackup,
    options: &EditOptions,
) -> EditReport {
    apply_keymap_edit(
        &mut Session::new(sim),
        plan,
        saved,
        consent(),
        options,
        |_| ControlFlow::Continue(()),
    )
}

/// Plans and applies `edit` with a fresh safety backup.
fn run(sim: &mut DeviceSim, edit: &KeymapEdit) -> EditReport {
    let dir = temp_dir();
    let saved = safety(sim, &dir);
    let plan = plan(sim, edit).expect("plan");
    apply(sim, &plan, &saved, &EditOptions::default())
}

/// The c16 DKS: press point 1.5 mm, E at stage 1, R at stage 2.
fn c16_dks() -> AdvancedKeyDef {
    AdvancedKeyDef::Dks(Dks {
        first_point: 15,
        rows: [
            Some(DksRow {
                key: usage(0x08),
                steps: [DksStep::Tap, DksStep::None, DksStep::None, DksStep::None],
            }),
            Some(DksRow {
                key: usage(0x15),
                steps: [DksStep::None, DksStep::Tap, DksStep::None, DksStep::None],
            }),
            None,
            None,
        ],
    })
}

fn c16_mt() -> AdvancedKeyDef {
    AdvancedKeyDef::ModTap(ModTap {
        hold: usage(0xe0),
        tap: usage(0x0a),
        threshold_10ms: 30,
    })
}

fn c16_tgl() -> AdvancedKeyDef {
    AdvancedKeyDef::Toggle(Toggle { key: usage(0x0b) })
}

fn bind_advanced(name: &str, def: AdvancedKeyDef) -> KeymapChange {
    KeymapChange::BindAdvanced {
        layer: Layer::Base,
        pos: key(name),
        def,
        slot: None,
    }
}

/// The c13 macro: A down, 100 ms, A up, 100 ms, B down, 100 ms, B up.
fn c13_program() -> MacroProgram {
    let (a, b) = (usage(0x04), usage(0x05));
    MacroProgram::new(
        MacroPlay::Once,
        vec![
            MacroStep::press(a, 100),
            MacroStep::release(a, 100),
            MacroStep::press(b, 100),
            MacroStep::release(b, 0),
        ],
    )
    .expect("valid")
}

fn edit(changes: Vec<KeymapChange>) -> KeymapEdit {
    KeymapEdit {
        changes,
        ..KeymapEdit::default()
    }
}

/// `(command, sub)` of every table write packet from write `from` on
/// (profile selects left out).
fn heads(sim: &DeviceSim, from: usize) -> Vec<(u8, u8)> {
    write_bytes(sim)[from..]
        .iter()
        .filter(|b| b[..6] != [0x21, 0, 0, 0, 0, 0x12])
        .map(|b| (b[0], b[1]))
        .collect()
}

fn entry(sim: &DeviceSim, layer: Layer, pos: KeyPos) -> Vec<u8> {
    let start = usize::from(pos.index()) * 4;
    sim.table(profile(1), SimTable::Keymap(layer))
        .expect("keymap")[start..start + 4]
        .to_vec()
}

#[test]
fn binding_dks_mod_tap_and_toggle_sends_the_captured_slots_after_the_keymaps() {
    let mut sim = sim();
    let report = run(
        &mut sim,
        &edit(vec![
            bind_advanced("E", c16_dks()),
            bind_advanced("G", c16_mt()),
            bind_advanced("H", c16_tgl()),
        ]),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    // The site's order: base keymap, Fn keymap (re-sent as the commit
    // trigger), then the slots, lowest free first.
    let mut expected: Vec<(u8, u8)> = vec![(0x18, 0x00); 10];
    expected.extend(vec![(0x18, 0x02); 10]);
    expected.extend([(0x22, 0), (0x22, 1), (0x22, 2)]);
    assert_eq!(heads(&sim, 0), expected);
    // The slot packets the vendor site sent in c16 (DKS #2302, MT #3072,
    // TGL in slot 2 #3563), byte for byte.
    assert_eq!(
        write_bytes(&sim)[20..],
        captured(C16, &[2302, 3072, 3563])[..]
    );
    // The bindings `17 SS SS 00` (c16 B10).
    assert_eq!(entry(&sim, Layer::Base, key("E")), [0x17, 0, 0, 0]);
    assert_eq!(entry(&sim, Layer::Base, key("G")), [0x17, 1, 1, 0]);
    assert_eq!(entry(&sim, Layer::Base, key("H")), [0x17, 2, 2, 0]);
    let items: Vec<(TableId, Option<CommitTriggerReason>, bool)> = report
        .items
        .iter()
        .map(|i| (i.table, i.commit_trigger, i.resent_unchanged))
        .collect();
    assert_eq!(items[1].1, Some(CommitTriggerReason::BaseKeymap));
    assert!(items[1].2);
    // Advanced keys after a complete Fn keymap are known to be kept: no
    // profile switch for a persistence check.
    assert!(report.persistence.is_none());
    assert!(selects(&sim).is_empty());
    // H20 (2026-10-27) confirmed these three on hardware: no "not yet
    // verified" note.
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| matches!(w, EditWarning::Unverified(_))),
        "{:?}",
        report.warnings
    );
    assert!(report.warnings.contains(&EditWarning::TransientBinding {
        slot: SlotRef::Advanced(AdvancedSlotId::new(0).expect("slot"))
    }));
    assert!(sim.dangling_references(profile(1), false).is_empty());
    // A new plan reads the keys back as what was written.
    let state = KeymapGroupState::read(&mut Session::new(&mut sim)).expect("state");
    assert_eq!(
        state.entry(Layer::Base, key("G")),
        EntryKind::Assignable(KeyAssignment::Advanced(
            AdvancedSlotId::new(1).expect("slot")
        ))
    );
    assert_eq!(
        state.advanced(AdvancedSlotId::new(1).expect("slot")),
        AdvancedSlotView::Def { def: c16_mt() }
    );
    assert!(state.orphans().is_empty());
    sim.assert_clean();
}

#[test]
fn binding_a_macro_sends_the_captured_slot_and_needs_no_persistence_check() {
    let mut sim = sim();
    let report = run(
        &mut sim,
        &edit(vec![KeymapChange::BindMacro {
            layer: Layer::Base,
            pos: key("Caps Lock"),
            program: c13_program(),
            // The capture's slot (a new macro takes slot 1 by itself);
            // every other slot is empty, so nothing is re-sent after it.
            slot: Some(MacroSlotId::new(0).expect("slot")),
        }]),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    // c13 slot 0: #316, #320, #322, #324, #326; then the base keymap with
    // Caps = `10 39 00 00` and the Fn keymap after it.
    let writes = write_bytes(&sim);
    assert_eq!(writes[..5], captured(C13, &[316, 320, 322, 324, 326])[..]);
    let mut expected = vec![(0x19, 0x00); 5];
    expected.extend(vec![(0x18, 0x00); 10]);
    expected.extend(vec![(0x18, 0x02); 10]);
    assert_eq!(heads(&sim, 0), expected);
    assert_eq!(
        entry(&sim, Layer::Base, key("Caps Lock")),
        [0x10, 0x39, 0, 0]
    );
    // Macro slots are known to be kept (H19, 2026-10-27), and so are the
    // keymap layers: no profile switch to check them.
    assert!(selects(&sim).is_empty());
    assert_eq!(report.persistence, None);
    assert_eq!(
        sim.persisted_table(
            profile(1),
            SimTable::Macro(MacroSlotId::new(0).expect("slot"))
        )
        .map(|b| b[..24].to_vec()),
        sim.table(
            profile(1),
            SimTable::Macro(MacroSlotId::new(0).expect("slot"))
        )
        .map(|b| b[..24].to_vec())
    );
    // Macro slots survive a power cycle too (H22, 2026-10-27): no note.
    assert_eq!(report.warnings, []);
    let state = KeymapGroupState::read(&mut Session::new(&mut sim)).expect("state");
    assert_eq!(
        state.macro_program(MacroSlotId::new(0).expect("slot")),
        MacroSlotView::Program {
            program: c13_program()
        }
    );
    sim.assert_clean();
}

#[test]
fn fn_layer_codes_need_an_acknowledgement_for_built_in_functions() {
    let mut sim = sim();
    let assign = |name: &str, assignment| KeymapChange::Assign {
        layer: Layer::Fn,
        pos: key(name),
        assignment,
    };
    let changes = vec![
        assign(
            "F1",
            KeyAssignment::Media(MediaKey::new(0x12).expect("mute")),
        ),
        assign("F2", KeyAssignment::Mouse(MouseAction::WheelDown)),
        assign(
            "W",
            KeyAssignment::Media(MediaKey::new(0x32).expect("calculator")),
        ),
        assign(
            "Caps Lock",
            KeyAssignment::Combo(
                Combo::new(&[usage(0xe0), usage(0xe1)], usage(0x04)).expect("combo"),
            ),
        ),
    ];
    // F1 and F2 are built-in Fn functions: refused without the user's
    // acknowledgement.
    assert!(matches!(
        plan(&mut sim, &edit(changes.clone())),
        Err(EditError::Placement(
            PlacementError::BuiltinNotAcknowledged {
                index: 2,
                critical: false,
                ..
            }
        ))
    ));
    let acked = KeymapEdit {
        changes,
        builtin_acks: ["F1", "F2"]
            .into_iter()
            .map(|name| BuiltinAck {
                pos: key(name),
                critical: false,
            })
            .collect(),
        ..KeymapEdit::default()
    };
    let report = run(&mut sim, &acked);
    assert!(report.is_success(), "{:#?}", report.error);
    // Only the Fn keymap changes: one plain Fn write, no trigger.
    assert_eq!(heads(&sim, 0), vec![(0x18, 0x02); 10]);
    assert_eq!(report.items.len(), 1);
    assert_eq!(report.items[0].commit_trigger, None);
    // The entries of c17.
    assert_eq!(entry(&sim, Layer::Fn, key("F1")), [0x07, 0x12, 0, 0]);
    assert_eq!(entry(&sim, Layer::Fn, key("F2")), [0x12, 0x01, 0xff, 0]);
    assert_eq!(entry(&sim, Layer::Fn, key("W")), [0x07, 0x32, 0, 0]);
    assert_eq!(
        entry(&sim, Layer::Fn, key("Caps Lock")),
        [0x13, 0xe0, 0xe1, 0x04]
    );
    // The Fn key keeps its entry; the base layer did not change.
    assert_eq!(heads(&sim, 0).iter().filter(|h| h.1 == 0).count(), 0);
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        EditWarning::FnBuiltinOverridden {
            index: 2,
            critical: false,
            ..
        }
    )));
    sim.assert_clean();
}

#[test]
fn the_knob_the_fn_key_critical_and_locked_keys_are_protected() {
    let mut sim = sim();
    let assign = |layer, pos, assignment| KeymapChange::Assign {
        layer,
        pos,
        assignment,
    };
    let a = KeyAssignment::Key(usage(0x04));
    for layer in [Layer::Base, Layer::Fn] {
        // The knob (15) and the Fn key (121), on either layer.
        assert!(matches!(
            plan(
                &mut sim,
                &edit(vec![assign(
                    layer,
                    KeyPos::from_index(15).expect("knob"),
                    a
                )])
            ),
            Err(EditError::Placement(PlacementError::Knob))
        ));
        assert!(matches!(
            plan(
                &mut sim,
                &edit(vec![assign(layer, KeyPos::from_index(121).expect("Fn"), a)])
            ),
            Err(EditError::Placement(PlacementError::FnKey))
        ));
    }
    // Fn + 2 switches to profile 2: a plain acknowledgement is not enough.
    let two = key("2");
    let fn_two = || edit(vec![assign(Layer::Fn, two, a)]);
    let plain = KeymapEdit {
        builtin_acks: vec![BuiltinAck {
            pos: two,
            critical: false,
        }],
        ..fn_two()
    };
    assert!(matches!(
        plan(&mut sim, &plain),
        Err(EditError::Placement(
            PlacementError::BuiltinNotAcknowledged { critical: true, .. }
        ))
    ));
    let critical = KeymapEdit {
        builtin_acks: vec![BuiltinAck {
            pos: two,
            critical: true,
        }],
        ..fn_two()
    };
    let planned = plan(&mut sim, &critical).expect("acknowledged");
    assert!(
        planned
            .warnings()
            .iter()
            .any(|w| matches!(w, EditWarning::FnBuiltinOverridden { critical: true, .. }))
    );
    // The base layer has no built-in functions.
    plan(&mut sim, &edit(vec![assign(Layer::Base, two, a)])).expect("base layer");
    // A key the keyboard locks on the Fn layer (`0E`): here Esc.
    let mut locked = c06_backup();
    lock_esc(&mut locked);
    let mut locked_sim = DeviceSim::from_backup(&locked);
    assert!(matches!(
        plan(
            &mut locked_sim,
            &edit(vec![assign(Layer::Fn, key("Esc"), a)])
        ),
        Err(EditError::Placement(PlacementError::FnLocked { index: 0 }))
    ));
    plan(
        &mut locked_sim,
        &edit(vec![assign(Layer::Base, key("Esc"), a)]),
    )
    .expect("base layer");
    assert!(write_bytes(&sim).is_empty());
    sim.assert_clean();
}

/// Sets the Esc bit (row 0, column 0) in the backup's `0E` mask.
fn lock_esc(backup: &mut Backup) {
    let index = backup
        .entries
        .iter()
        .position(|e| e.request() == ReadRequest::FnLockedKeys)
        .expect("0E reply");
    let mut reports = backup.entries[index].raw.reports().to_vec();
    let mut bytes = *reports[0].as_bytes();
    bytes[5] |= 0x01;
    reports[0] = openwraith_hid::protocol::Report::new(bytes);
    let raw =
        openwraith_hid::protocol::RawResponse::from_reports(ReadRequest::FnLockedKeys, reports)
            .expect("valid reply");
    backup.entries[index] = BackupEntry::new(raw);
}

#[test]
fn one_slot_per_key_and_no_binding_to_an_empty_slot() {
    let mut sim = sim();
    let report = run(
        &mut sim,
        &edit(vec![KeymapChange::BindMacro {
            layer: Layer::Base,
            pos: key("Caps Lock"),
            program: c13_program(),
            slot: None,
        }]),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    // The first free slot in 1-9 (slot 0 would make the keyboard empty the
    // others).
    let slot1 = MacroSlotId::new(1).expect("slot");
    assert_eq!(
        entry(&sim, Layer::Base, key("Caps Lock")),
        [0x10, 0x39, 0x01, 0x00]
    );
    // Another key on slot 1: refused (the site keeps one slot per key).
    let shared = plan(
        &mut sim,
        &edit(vec![KeymapChange::Assign {
            layer: Layer::Fn,
            pos: key("J"),
            assignment: KeyAssignment::Macro(slot1),
        }]),
    );
    assert!(
        matches!(
            shared,
            Err(EditError::SlotAlreadyBound {
                slot: SlotRef::Macro(_),
                by: (Layer::Base, 66),
            })
        ),
        "{shared:?}"
    );
    // A binding to an empty slot.
    assert!(matches!(
        plan(
            &mut sim,
            &edit(vec![KeymapChange::Assign {
                layer: Layer::Base,
                pos: key("A"),
                assignment: KeyAssignment::Advanced(AdvancedSlotId::new(5).expect("slot")),
            }])
        ),
        Err(EditError::BindingToEmptySlot { index: 68, .. })
    ));
    // Clearing a slot a key still refers to.
    assert!(matches!(
        plan(
            &mut sim,
            &edit(vec![KeymapChange::SetMacroSlot {
                slot: slot1,
                program: None,
            }])
        ),
        Err(EditError::SlotStillReferenced { .. })
    ));
    // A second macro takes the lowest free slot; re-binding Caps reuses its
    // own slot.
    let second = plan(
        &mut sim,
        &edit(vec![
            KeymapChange::BindMacro {
                layer: Layer::Base,
                pos: key("A"),
                program: c13_program(),
                slot: None,
            },
            KeymapChange::BindMacro {
                layer: Layer::Base,
                pos: key("Caps Lock"),
                program: c13_program(),
                slot: None,
            },
        ]),
    )
    .expect("plan");
    let macro_writes: Vec<TableId> = second
        .writes()
        .iter()
        .filter(|w| matches!(w.table, TableId::Macro { .. }))
        .map(|w| w.table)
        .collect();
    assert_eq!(
        macro_writes,
        [TableId::Macro {
            slot: MacroSlotId::new(2).expect("slot")
        }]
    );
    sim.assert_clean();
}

#[test]
fn unbinding_clears_the_orphan_slot_first_and_never_leaves_a_dangling_key() {
    let mut sim = sim();
    let bound = run(
        &mut sim,
        &edit(vec![
            KeymapChange::BindMacro {
                layer: Layer::Base,
                pos: key("Caps Lock"),
                program: c13_program(),
                slot: None,
            },
            bind_advanced("H", c16_tgl()),
        ]),
    );
    assert!(bound.is_success(), "{:#?}", bound.error);
    let first = write_bytes(&sim).len();
    let unbind = edit(vec![
        KeymapChange::Unbind {
            layer: Layer::Base,
            pos: key("Caps Lock"),
        },
        KeymapChange::Unbind {
            layer: Layer::Base,
            pos: key("H"),
        },
    ]);
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &unbind).expect("plan");
    // The macro clear goes first (site order); the base keymap stops
    // referring to it, so it completes the binding change and is written
    // even after a cancellation.
    let flags: Vec<(TableId, bool)> = planned
        .writes()
        .iter()
        .map(|w| (w.table, w.completes_binding))
        .collect();
    assert_eq!(
        flags,
        [
            (
                TableId::Macro {
                    slot: MacroSlotId::new(1).expect("slot")
                },
                false
            ),
            (TableId::Keymap { layer: Layer::Base }, true),
            (TableId::Keymap { layer: Layer::Fn }, false),
            (
                TableId::AdvancedKey {
                    slot: AdvancedSlotId::new(0).expect("slot")
                },
                false
            ),
        ]
    );
    let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
    assert!(report.is_success(), "{:#?}", report.error);
    let mut expected = vec![(0x19, 0x01); 5];
    expected.extend(vec![(0x18, 0x00); 10]);
    expected.extend(vec![(0x18, 0x02); 10]);
    expected.push((0x22, 0x00));
    assert_eq!(heads(&sim, first), expected);
    assert_eq!(
        sim.table(
            profile(1),
            SimTable::Macro(MacroSlotId::new(1).expect("slot"))
        ),
        Some(&[0u8; 256][..])
    );
    // With keep_orphans the slots keep their content.
    let mut sim = self::sim();
    run(
        &mut sim,
        &edit(vec![KeymapChange::BindMacro {
            layer: Layer::Base,
            pos: key("Caps Lock"),
            program: c13_program(),
            slot: None,
        }]),
    );
    let keep = KeymapEdit {
        keep_orphans: true,
        ..edit(vec![KeymapChange::Unbind {
            layer: Layer::Base,
            pos: key("Caps Lock"),
        }])
    };
    let planned = plan(&mut sim, &keep).expect("plan");
    assert!(
        planned
            .writes()
            .iter()
            .all(|w| !matches!(w.table, TableId::Macro { .. }))
    );
    assert!(planned.warnings().contains(&EditWarning::OrphanSlotKept {
        slot: SlotRef::Macro(MacroSlotId::new(1).expect("slot"))
    }));
    sim.assert_clean();
}

#[test]
fn slot_only_writes_get_the_fn_keymap_before_the_advanced_keys() {
    let mut sim = sim();
    let report = run(
        &mut sim,
        &edit(vec![
            KeymapChange::SetAdvancedSlot {
                slot: AdvancedSlotId::new(4).expect("slot"),
                def: Some(c16_tgl()),
            },
            KeymapChange::SetMacroSlot {
                slot: MacroSlotId::new(3).expect("slot"),
                program: Some(c13_program()),
            },
        ]),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    let mut expected = vec![(0x19, 0x03); 5];
    expected.extend(vec![(0x18, 0x02); 10]);
    expected.push((0x22, 0x04));
    assert_eq!(heads(&sim, 0), expected);
    assert_eq!(
        report.items[1].commit_trigger,
        Some(CommitTriggerReason::AdvancedKeys)
    );
    assert_eq!(
        report.items[1].summary,
        "Fn keymap (18 02) re-sent unchanged before the advanced-key slots to make the \
         keyboard save them"
    );
    // Unbound slots are orphans (kept by the keyboard).
    let state = KeymapGroupState::read(&mut Session::new(&mut sim)).expect("state");
    assert_eq!(state.orphans().len(), 2);
    sim.assert_clean();
}

/// A macro slot is kept without an Fn keymap (H19 step 2, 2026-10-27,
/// `hw/v02/h19-2.txt`): a macro-only edit writes no `18 02`, and macro
/// slots are known to be kept, so the default edit does not switch
/// profiles to check.
#[test]
fn a_macro_only_edit_writes_no_fn_keymap() {
    let mut sim = sim();
    let report = run(
        &mut sim,
        &edit(vec![KeymapChange::SetMacroSlot {
            slot: MacroSlotId::new(3).expect("slot"),
            program: Some(c13_program()),
        }]),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(heads(&sim, 0), vec![(0x19, 0x03); 5]);
    assert_eq!(report.items.len(), 1);
    assert_eq!(report.items[0].commit_trigger, None);
    assert_eq!(report.persistence, None);
    assert!(selects(&sim).is_empty());
    let kept = sim.persisted_table(
        profile(1),
        SimTable::Macro(MacroSlotId::new(3).expect("slot")),
    );
    assert!(kept.is_some_and(|b| b.iter().any(|x| *x != 0 && *x != 0xff)));
    sim.assert_clean();
}

/// H20 step 2 (2026-10-27, `hw/v02/h20-2.txt`, `h20-2b.txt`): an
/// advanced-key slot written alone (`--commit-trigger skip`, one `22`
/// packet) read back fine but was gone after the forced profile switch;
/// the same edit with the Fn keymap before it (`18 02`, then `22`) was
/// kept. Macro slots written alone are kept (H19 step 2).
#[test]
fn without_the_fn_keymap_a_lone_advanced_key_slot_is_reported_lost() {
    let lone = |change: KeymapChange| KeymapEdit {
        commit_trigger: CommitTriggerMode::OmitForSlotsOnly,
        ..edit(vec![change])
    };
    let dks = KeymapChange::SetAdvancedSlot {
        slot: AdvancedSlotId::new(0).expect("slot"),
        def: Some(c16_dks()),
    };
    let never = EditOptions {
        persistence: PersistenceCheckMode::Never,
        ..EditOptions::default()
    };

    // The lone 22: the check, forced by the hardware-test mode, says lost.
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &lone(dks.clone())).expect("plan");
    assert!(planned.forces_persistence_check());
    assert_eq!(planned.writes().len(), 1);
    let report = apply(&mut sim, &planned, &saved, &never);
    assert_eq!(heads(&sim, 0), vec![(0x22, 0x00)]);
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::NotPersisted)
    );
    assert_eq!(report.lost(), 1);
    assert_eq!(
        sim.persisted_table(profile(1), SimTable::AdvancedKeys)
            .map(|b| b[..40].iter().all(|x| *x == 0)),
        Some(true)
    );
    sim.assert_clean();

    // The default mode: 18 02, then 22, kept; the check (an advanced-key
    // slot after a complete Fn keymap is known to be kept) does not run.
    let mut sim = self::sim();
    let report = run(&mut sim, &edit(vec![dks]));
    assert!(report.is_success(), "{:#?}", report.error);
    let mut expected = vec![(0x18, 0x02); 10];
    expected.push((0x22, 0x00));
    assert_eq!(heads(&sim, 0), expected);
    assert_eq!(report.persistence, None);
    assert_eq!(
        sim.persisted_table(profile(1), SimTable::AdvancedKeys)
            .map(|b| b[0]),
        Some(0x01)
    );
    sim.assert_clean();

    // A lone macro slot is kept, and the forced check says so.
    let macro3 = KeymapChange::SetMacroSlot {
        slot: MacroSlotId::new(3).expect("slot"),
        program: Some(c13_program()),
    };
    let mut sim = self::sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &lone(macro3.clone())).expect("plan");
    let report = apply(&mut sim, &planned, &saved, &never);
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(report.persistence.as_ref().map(|c| c.persisted), Some(1));

    // If macros needed the Fn keymap after them (the simulator's switch),
    // the same lone slot would be reported lost.
    let mut sim = self::sim();
    sim.set_keymap_commit(KeymapCommit {
        macros_need_fn_write: true,
        ..KeymapCommit::default()
    });
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &lone(macro3)).expect("plan");
    let report = apply(&mut sim, &planned, &saved, &never);
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::NotPersisted)
    );
    assert_eq!(report.lost(), 1);
    assert_eq!(heads(&sim, 0), vec![(0x19, 0x03); 5]);

    // The mode is refused when a layer changes.
    let with_layer = KeymapEdit {
        commit_trigger: CommitTriggerMode::OmitForSlotsOnly,
        ..edit(vec![KeymapChange::BindMacro {
            layer: Layer::Base,
            pos: key("Caps Lock"),
            program: c13_program(),
            slot: None,
        }])
    };
    assert!(matches!(
        plan(&mut sim, &with_layer),
        Err(EditError::Invalid { .. })
    ));
    sim.assert_clean();
}

/// Binding DKS on E and a macro on Caps: macro slot, base keymap, Fn
/// keymap, advanced slot (34 packets).
fn mixed_edit() -> KeymapEdit {
    edit(vec![
        KeymapChange::BindMacro {
            layer: Layer::Base,
            pos: key("Caps Lock"),
            program: c13_program(),
            slot: None,
        },
        bind_advanced("E", c16_dks()),
    ])
}

#[test]
fn a_cancellation_at_any_table_never_leaves_a_dangling_key_or_an_unsaved_base_keymap() {
    let tables = [
        TableId::Macro {
            slot: MacroSlotId::new(1).expect("slot"),
        },
        TableId::Keymap { layer: Layer::Base },
        TableId::Keymap { layer: Layer::Fn },
        TableId::AdvancedKey {
            slot: AdvancedSlotId::new(0).expect("slot"),
        },
    ];
    for (k, cancel_at) in tables.iter().enumerate() {
        let mut sim = sim();
        let dir = temp_dir();
        let saved = safety(&mut sim, &dir);
        let planned = plan(&mut sim, &mixed_edit()).expect("plan");
        let report = apply_keymap_edit(
            &mut Session::new(&mut sim),
            &planned,
            &saved,
            consent(),
            &EditOptions::default(),
            |p| match p {
                WriteProgress::Writing {
                    table, packet: 0, ..
                } if table == *cancel_at => ControlFlow::Break(()),
                _ => ControlFlow::Continue(()),
            },
        );
        // Cancelled before anything: nothing written; otherwise the group
        // is completed up to the commit trigger and every binding fill.
        let sent: Vec<TableId> = report
            .items
            .iter()
            .filter(|i| i.result.was_sent())
            .map(|i| i.table)
            .collect();
        match k {
            0 => assert!(sent.is_empty(), "{sent:?}"),
            // After the macro: the base keymap is skipped (it is not needed
            // by what was sent), and so is the Fn keymap (a macro slot is
            // kept without it, only a sent base keymap needs it); the DKS
            // fill is not needed either (its binding was not written).
            1 => assert_eq!(sent, [tables[0]], "{k}"),
            // From the Fn keymap on, the DKS fill completes the binding.
            _ => assert_eq!(sent, tables, "{k}"),
        }
        assert!(
            sim.dangling_references(profile(1), false).is_empty(),
            "{k}: {:?}",
            sim.dangling_references(profile(1), false)
        );
        assert!(!sim.has_edits_waiting_for_fn_write(), "{k}");
        assert!(report.dangling.is_empty(), "{k}");
        sim.assert_clean();
    }
}

#[test]
fn a_disconnect_after_any_packet_reports_every_dangling_key() {
    let total = {
        let mut copy = sim();
        let report = run(&mut copy, &mixed_edit());
        assert!(report.is_success(), "{:#?}", report.error);
        copy.written().len()
    };
    let mut reported = 0;
    for after in 1..=total {
        let mut sim = sim();
        let dir = temp_dir();
        let saved = safety(&mut sim, &dir);
        let planned = plan(&mut sim, &mixed_edit()).expect("plan");
        let start = sim.written().len();
        if after <= start {
            continue;
        }
        sim.disconnect_after_packets(after);
        let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
        let left: Vec<(Layer, u8, SlotRef)> = sim.dangling_references(profile(1), false);
        let said: Vec<(Layer, u8, SlotRef)> = report
            .dangling
            .iter()
            .map(|d| (d.layer, d.index, d.slot))
            .collect();
        // Every key the keyboard was left with that refers to an empty slot
        // is in the report (which may name more: a table cut off part-way
        // counts as written).
        assert!(
            left.iter().all(|d| said.contains(d)),
            "disconnect after packet {after}: left {left:?}, said {said:?}"
        );
        if !said.is_empty() {
            reported += 1;
        }
    }
    // The window between the base keymap and the DKS slot exists.
    assert!(reported > 0);
}

#[test]
fn undo_restores_the_state_before_the_edit() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let before = saved.backup().clone();
    let planned = plan(&mut sim, &mixed_edit()).expect("plan");
    let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
    assert!(report.is_success(), "{:#?}", report.error);
    // The undo: restore the safety backup.
    let options = RestoreOptions::default();
    let mut session = Session::new(&mut sim);
    let prepared = prepare_restore(&mut session, &before, &options, consent(), |_| {
        ControlFlow::Continue(())
    })
    .expect("prepared");
    let undo_dir = temp_dir();
    let undo_saved = prepared
        .safety()
        .save_safety(undo_dir.path(), "restore")
        .expect("saved");
    let undone = execute_restore(&mut session, &prepared, &undo_saved, consent(), |_| {
        ControlFlow::Continue(())
    });
    assert!(undone.is_success(), "{:#?}", undone.error);
    drop(session);
    let after = read_all(&mut Session::new(&mut sim)).expect("read");
    for request in [
        ReadRequest::Keymap(Layer::Base),
        ReadRequest::Keymap(Layer::Fn),
        ReadRequest::Macro(MacroSlotId::new(0).expect("slot")),
        ReadRequest::AdvancedKeys,
    ] {
        let content = |b: &Backup| {
            b.response(request)
                .map(|r| r.table_bytes("t").unwrap_or_default())
        };
        let (old, new) = (content(&before), content(&after));
        if let ReadRequest::Macro(_) = request {
            // Empty equals empty (00 or FF).
            let empty = |b: &Option<Vec<u8>>| {
                b.as_ref()
                    .is_some_and(|b| b.iter().all(|x| *x == 0) || b.iter().all(|x| *x == 0xFF))
            };
            assert!(empty(&old) && empty(&new), "{request}");
        } else {
            assert_eq!(old, new, "{request}");
        }
    }
    sim.assert_clean();
}

#[test]
fn a_change_after_planning_stops_before_writing() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &mixed_edit()).expect("plan");
    // Someone changes the base keymap meanwhile.
    let mut base = sim
        .table(profile(1), SimTable::Keymap(Layer::Base))
        .expect("keymap")
        .to_vec();
    base[46 * 4 + 1] = 0x04;
    sim.set_table(profile(1), SimTable::Keymap(Layer::Base), base);
    let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::ChangedSincePlan)
    );
    // The macro slot went out first; the base keymap was refused, and the
    // keyboard holds no new binding.
    assert!(sim.dangling_references(profile(1), false).is_empty());
    assert_eq!(heads(&sim, 0), vec![(0x19, 0x01); 5]);
    sim.assert_clean();
}

#[test]
fn a_reset_mid_group_stops_without_another_packet() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &mixed_edit()).expect("plan");
    // Right after the first base-keymap packet.
    let at = {
        let mut copy = sim.clone();
        let start = copy.written().len();
        apply(&mut copy, &planned, &saved, &EditOptions::default());
        start
            + copy.written()[start..]
                .iter()
                .position(|p| p.is_write() && p.as_bytes()[..2] == [0x18, 0x00])
                .expect("base keymap")
            + 1
    };
    sim.reset_after_packets(at);
    let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::KeyboardReset)
    );
    // assert_clean refuses any packet after the reset notification.
    sim.assert_clean();
    assert!(
        report
            .items
            .iter()
            .any(|i| matches!(i.result, ItemResult::Failed { .. }))
    );
}

#[test]
fn the_group_state_reads_the_same_from_a_snapshot() {
    let mut sim = sim();
    run(&mut sim, &mixed_edit());
    let state = KeymapGroupState::read(&mut Session::new(&mut sim)).expect("state");
    let backup = read_all(&mut Session::new(&mut sim)).expect("backup");
    let snapshot = KeymapGroupState::from_snapshot(backup.primary()).expect("state");
    for layer in [Layer::Base, Layer::Fn] {
        assert_eq!(state.layer(layer), snapshot.layer(layer));
    }
    assert_eq!(state.references(), snapshot.references());
    assert_eq!(state.references().len(), 2);
    sim.assert_clean();
}

/// The 1-based number of the first write packet `want` accepts that
/// `run_it` sends to a copy of `sim`.
fn first_write(
    sim: &DeviceSim,
    run_it: impl FnOnce(&mut DeviceSim),
    want: impl Fn(&[u8; 63]) -> bool,
) -> usize {
    let mut copy = sim.clone();
    let start = copy.written().len();
    run_it(&mut copy);
    start
        + copy.written()[start..]
            .iter()
            .position(|p| p.is_write() && want(p.as_bytes()))
            .expect("the packet")
        + 1
}

#[test]
fn a_dropped_acknowledgement_resends_the_whole_slot_once() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &mixed_edit()).expect("plan");
    // Write packets: 1-5 = the macro slot; the second one is not acked.
    sim.drop_ack(2);
    let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
    assert!(report.is_success(), "{:#?}", report.error);
    assert!(matches!(
        &report.items[0].result,
        ItemResult::Written { sends: 2, warnings, .. }
            if warnings.contains(&openwraith_hid::WriteWarning::AckTimeout { packet: 2, of: 5, send: 1 })
    ));
    // The rest of the first send was not sent: 2 + 5 macro packets.
    assert_eq!(heads(&sim, 0).iter().filter(|h| h.0 == 0x19).count(), 7);
    sim.assert_clean();
}

#[test]
fn another_program_after_the_base_keymap_leaves_it_unsaved_and_names_the_dangling_key() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &mixed_edit()).expect("plan");
    let at = first_write(
        &sim,
        |copy| {
            apply(copy, &planned, &saved, &EditOptions::default());
        },
        |b| b[..2] == [0x18, 0x00],
    );
    // A foreign trigger acknowledgement right after the first base-keymap
    // packet: the base keymap is finished, then nothing more is written.
    sim.inject_after_packets(at, common::report(&[0x21, 0, 0, 0, 0x01, 0x00]));
    let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::OtherProgramActive)
    );
    let base = report
        .items
        .iter()
        .find(|i| i.table == TableId::Keymap { layer: Layer::Base })
        .expect("base keymap");
    assert!(
        matches!(&base.result, ItemResult::MaybeLost { warnings, .. }
            if warnings.contains(&openwraith_hid::WriteWarning::BaseKeymapNotSaved)),
        "{:?}",
        base.result
    );
    // E refers to the DKS slot that was never written: the report says so.
    let dangling: Vec<(Layer, u8, SlotRef)> = report
        .dangling
        .iter()
        .map(|d| (d.layer, d.index, d.slot))
        .collect();
    assert_eq!(
        dangling,
        [(
            Layer::Base,
            key("E").index(),
            SlotRef::Advanced(AdvancedSlotId::new(0).expect("slot"))
        )]
    );
    assert_eq!(dangling, sim.dangling_references(profile(1), false));
    assert!(sim.has_edits_waiting_for_fn_write());
    sim.assert_clean();
}

#[test]
fn a_profile_switch_on_the_keyboard_before_the_save_makes_the_writes_may_be_lost() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let planned = plan(&mut sim, &mixed_edit()).expect("plan");
    let last_write = {
        let mut copy = sim.clone();
        let start = copy.written().len();
        apply(&mut copy, &planned, &saved, &EditOptions::default());
        copy.written()[start..]
            .iter()
            .zip(&copy.sent_times()[start..])
            .filter(|(p, _)| p.is_write() && p.as_bytes()[0] == 0x22)
            .map(|(_, t)| *t)
            .next_back()
            .expect("the slot write")
    };
    // Fn + 2, 300 ms after the last write, without a report.
    sim.switch_profile_at(
        last_write + std::time::Duration::from_millis(300) - sim.elapsed(),
        profile(2),
        false,
    );
    let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::ProfileChanged)
    );
    assert!(report.maybe_lost() >= 1);
    assert_eq!(report.written(), 0);
    // No persistence check after a failure.
    assert!(report.persistence.is_none());
    assert_eq!(report.active_profile_now, Some(2));
    sim.assert_clean();
}

/// Restores `before` (the undo of an edit) and returns whether the keymap
/// group then reads as in `before`.
fn undo_and_compare(sim: &mut DeviceSim, before: &Backup) {
    let mut session = Session::new(&mut *sim);
    let prepared = prepare_restore(
        &mut session,
        before,
        &RestoreOptions::default(),
        consent(),
        |_| ControlFlow::Continue(()),
    )
    .expect("prepared");
    assert!(
        prepared.plan().skips().all(|(_, item)| !matches!(
            item.action,
            openwraith_hid::PlanAction::Skip {
                reason: openwraith_hid::SkipReason::KeymapGroupBlocked { .. },
                ..
            }
        )),
        "{:#?}",
        prepared.plan()
    );
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("saved");
    let undone = execute_restore(&mut session, &prepared, &saved, consent(), |_| {
        ControlFlow::Continue(())
    });
    assert!(undone.is_success(), "{:#?}", undone.error);
    drop(session);
    let after = read_all(&mut Session::new(&mut *sim)).expect("read");
    let mut requests = vec![
        ReadRequest::Keymap(Layer::Base),
        ReadRequest::Keymap(Layer::Fn),
        ReadRequest::AdvancedKeys,
    ];
    requests.extend(MacroSlotId::all().map(ReadRequest::Macro));
    for request in requests {
        let content = |b: &Backup| {
            b.response(request)
                .map(|r| r.table_bytes("t").unwrap_or_default())
        };
        assert_eq!(content(before), content(&after), "{request}");
    }
}

/// A sim whose slot 0 holds a Toggle of a media key (`07 10 00 00`, a key
/// the vendor site offers for Toggle but OpenWraith does not write), bound
/// to E on the base layer.
fn sim_with_vendor_toggle_on_e() -> DeviceSim {
    let mut sim = sim();
    let p1 = profile(1);
    let mut advanced = vec![0u8; 1600];
    advanced[..5].copy_from_slice(&[0x03, 0x07, 0x10, 0x00, 0x00]);
    sim.set_table(p1, SimTable::AdvancedKeys, advanced);
    let mut base = sim
        .table(p1, SimTable::Keymap(Layer::Base))
        .expect("keymap")
        .to_vec();
    let e = usize::from(key("E").index()) * 4;
    base[e..e + 4].copy_from_slice(&[0x17, 0x00, 0x00, 0x00]);
    sim.set_table(p1, SimTable::Keymap(Layer::Base), base);
    sim
}

#[test]
fn a_slot_the_undo_cannot_write_back_is_never_cleared_or_reused() {
    let slot0 = AdvancedSlotId::new(0).expect("slot");
    // Unbinding E keeps the vendor's Toggle in slot 0 (no 22 00 clear).
    let mut sim = sim_with_vendor_toggle_on_e();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let before = saved.backup().clone();
    let unbind = edit(vec![KeymapChange::Unbind {
        layer: Layer::Base,
        pos: key("E"),
    }]);
    let planned = plan(&mut sim, &unbind).expect("plan");
    assert!(
        planned
            .writes()
            .iter()
            .all(|w| !matches!(w.table, TableId::AdvancedKey { .. })),
        "{:#?}",
        planned.writes()
    );
    assert!(planned.warnings().contains(&EditWarning::OrphanSlotKept {
        slot: SlotRef::Advanced(slot0),
    }));
    let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
    assert!(report.is_success(), "{:#?}", report.error);
    undo_and_compare(&mut sim, &before);

    // A new advanced key on E takes a free slot; slot 0 stays as it is.
    let mut sim = sim_with_vendor_toggle_on_e();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let before = saved.backup().clone();
    let rebind = edit(vec![bind_advanced("E", c16_tgl())]);
    let planned = plan(&mut sim, &rebind).expect("plan");
    let slots: Vec<TableId> = planned
        .writes()
        .iter()
        .map(|w| w.table)
        .filter(|t| matches!(t, TableId::AdvancedKey { .. }))
        .collect();
    assert_eq!(
        slots,
        [TableId::AdvancedKey {
            slot: AdvancedSlotId::new(1).expect("slot"),
        }]
    );
    let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
    assert!(report.is_success(), "{:#?}", report.error);
    undo_and_compare(&mut sim, &before);
}

#[test]
fn overwriting_a_slot_the_undo_cannot_write_back_needs_an_acknowledgement() {
    let slot0 = AdvancedSlotId::new(0).expect("slot");
    let mut sim = sim_with_vendor_toggle_on_e();
    for change in [
        KeymapChange::SetAdvancedSlot {
            slot: slot0,
            def: None,
        },
        KeymapChange::SetAdvancedSlot {
            slot: slot0,
            def: Some(c16_tgl()),
        },
        KeymapChange::BindAdvanced {
            layer: Layer::Base,
            pos: key("E"),
            def: c16_tgl(),
            slot: Some(slot0),
        },
    ] {
        let refused = plan(&mut sim, &edit(vec![change.clone()]));
        assert!(
            matches!(
                refused,
                Err(EditError::UnsupportedCurrentTable {
                    table: TableId::AdvancedKey { .. },
                    ..
                }) | Err(EditError::SlotStillReferenced { .. })
            ),
            "{change:?}: {refused:?}"
        );
    }
    let allowed = KeymapEdit {
        changes: vec![KeymapChange::BindAdvanced {
            layer: Layer::Base,
            pos: key("E"),
            def: c16_tgl(),
            slot: Some(slot0),
        }],
        allow_not_undoable: true,
        ..KeymapEdit::default()
    };
    let planned = plan(&mut sim, &allowed).expect("plan");
    assert!(planned.warnings().contains(&EditWarning::NotUndoable {
        table: TableId::AdvancedKey { slot: slot0 },
    }));
}

#[test]
fn a_macro_in_another_layout_stays_when_its_key_is_unbound() {
    let mut sim = sim();
    let p1 = profile(1);
    let slot3 = MacroSlotId::new(3).expect("slot");
    // Play mode 5: not the canonical layout, so a restore cannot write it.
    let mut content = vec![0u8; 256];
    content[..12].copy_from_slice(&[3, 5, 0, 4, 0, 1, 0, 0, 0x04, 0x10, 0, 0]);
    sim.set_table(p1, SimTable::Macro(slot3), content);
    let mut base = sim
        .table(p1, SimTable::Keymap(Layer::Base))
        .expect("keymap")
        .to_vec();
    let caps = usize::from(key("Caps Lock").index()) * 4;
    base[caps..caps + 4].copy_from_slice(&[0x10, 0x39, 0x03, 0x00]);
    sim.set_table(p1, SimTable::Keymap(Layer::Base), base);
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let before = saved.backup().clone();
    let planned = plan(
        &mut sim,
        &edit(vec![KeymapChange::Unbind {
            layer: Layer::Base,
            pos: key("Caps Lock"),
        }]),
    )
    .expect("plan");
    assert!(
        planned
            .writes()
            .iter()
            .all(|w| !matches!(w.table, TableId::Macro { .. }))
    );
    assert!(planned.warnings().contains(&EditWarning::OrphanSlotKept {
        slot: SlotRef::Macro(slot3),
    }));
    let report = apply(&mut sim, &planned, &saved, &EditOptions::default());
    assert!(report.is_success(), "{:#?}", report.error);
    undo_and_compare(&mut sim, &before);
    // Clearing it explicitly is refused.
    let refused = plan(
        &mut sim,
        &edit(vec![
            KeymapChange::Unbind {
                layer: Layer::Base,
                pos: key("Caps Lock"),
            },
            KeymapChange::SetMacroSlot {
                slot: slot3,
                program: None,
            },
        ]),
    );
    assert!(matches!(
        refused,
        Err(EditError::UnsupportedCurrentTable {
            table: TableId::Macro { .. },
            ..
        })
    ));
}

/// A one-key macro: `usage` pressed and released.
fn tap_program(u: u8) -> MacroProgram {
    MacroProgram::new(
        MacroPlay::Once,
        vec![
            MacroStep::press(usage(u), 30),
            MacroStep::release(usage(u), 0),
        ],
    )
    .expect("valid")
}

/// Sets the base entry of the key `name` and macro slot `slot` of profile 1.
fn bind_macro_in_sim(sim: &mut DeviceSim, name: &str, slot: u8, program: &MacroProgram) {
    let p1 = profile(1);
    let slot_id = MacroSlotId::new(slot).expect("slot");
    sim.set_table(
        p1,
        SimTable::Macro(slot_id),
        program.encode(slot_id).to_bytes().to_vec(),
    );
    let mut base = sim
        .table(p1, SimTable::Keymap(Layer::Base))
        .expect("keymap")
        .to_vec();
    let k = key(name);
    let own = openwraith_hid::protocol::W75
        .key(k.index())
        .and_then(|k| k.hid_usage)
        .expect("usage");
    let at = usize::from(k.index()) * 4;
    base[at..at + 4].copy_from_slice(&[0x10, own, slot, 0x00]);
    sim.set_table(p1, SimTable::Keymap(Layer::Base), base);
}

fn base_entry(sim: &DeviceSim, name: &str) -> Vec<u8> {
    let at = usize::from(key(name).index()) * 4;
    sim.persisted_table(profile(1), SimTable::Keymap(Layer::Base))
        .map(|b| b[at..at + 4].to_vec())
        .unwrap_or_default()
}

/// A progress callback that asks to stop while macro slot `slot` is being
/// sent (after its first packet).
fn cancel_at_macro(slot: u8) -> impl FnMut(WriteProgress) -> ControlFlow<()> {
    move |progress| match progress {
        WriteProgress::Writing {
            table: TableId::Macro { slot: s },
            packet,
            ..
        } if s.get() == slot && packet >= 1 => ControlFlow::Break(()),
        _ => ControlFlow::Continue(()),
    }
}

#[test]
fn a_cancel_during_a_rewritten_macro_slot_still_moves_the_old_binding_away() {
    // Restore: the keyboard has W on macro slot 2; the backup has B on slot
    // 2 with other content and W unbound.
    let mut source = sim();
    bind_macro_in_sim(&mut source, "B", 2, &tap_program(0x05));
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    let mut sim = self::sim();
    bind_macro_in_sim(&mut sim, "W", 2, &tap_program(0x04));
    let mut session = Session::new(&mut sim);
    let prepared = prepare_restore(
        &mut session,
        &target,
        &RestoreOptions::default(),
        consent(),
        |_| ControlFlow::Continue(()),
    )
    .expect("prepared");
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("saved");
    let report = execute_restore(
        &mut session,
        &prepared,
        &saved,
        consent(),
        cancel_at_macro(2),
    );
    drop(session);
    // Everything left was needed to keep the keyboard consistent.
    assert!(report.error.is_none(), "{:#?}", report.error);
    assert!(report.cancel_requested);
    // The macro went out, so the base keymap (W unbound, B bound) did too.
    assert_eq!(base_entry(&sim, "W"), [0, 0, 0, 0]);
    assert_eq!(base_entry(&sim, "B"), [0x10, 0x05, 0x02, 0x00]);
    assert!(sim.dangling_references(profile(1), true).is_empty());

    // Edit: unbind W and give slot 2 new content in one group.
    let mut sim = self::sim();
    bind_macro_in_sim(&mut sim, "W", 2, &tap_program(0x04));
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let slot2 = MacroSlotId::new(2).expect("slot");
    let planned = plan(
        &mut sim,
        &KeymapEdit {
            changes: vec![
                KeymapChange::Unbind {
                    layer: Layer::Base,
                    pos: key("W"),
                },
                KeymapChange::SetMacroSlot {
                    slot: slot2,
                    program: Some(tap_program(0x05)),
                },
            ],
            ..KeymapEdit::default()
        },
    )
    .expect("plan");
    let report = apply_keymap_edit(
        &mut Session::new(&mut sim),
        &planned,
        &saved,
        consent(),
        &EditOptions::default(),
        cancel_at_macro(2),
    );
    // Everything left was needed to keep the keyboard consistent.
    assert!(report.error.is_none(), "{:#?}", report.error);
    assert_eq!(base_entry(&sim, "W"), [0, 0, 0, 0]);
}
