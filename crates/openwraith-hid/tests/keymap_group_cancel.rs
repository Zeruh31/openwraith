//! A cancellation (Ctrl+C) inside the keymap group never leaves a key bound
//! to a macro slot the cancellation skipped: only a sent base keymap makes
//! the Fn keymap (the commit trigger) due, since the keyboard keeps a macro
//! slot without it (H19 of 2026-10-27), and a macro slot that a layer newly
//! refers to goes out whenever that layer still does.

mod common;

use std::ops::ControlFlow;

use common::{
    bind_macro, c06_backup, consent, kept_entry, kept_macro, key_named, macro_bytes, macro_slot,
    profile, put_macro, tap, temp_dir,
};
use openwraith_hid::protocol::{HidUsage, KeyAssignment, Layer, TableId};
use openwraith_hid::{
    Backup, DeviceSim, EditOptions, EditReport, ItemResult, KeymapChange, KeymapEdit, Outcome,
    RestoreOptions, RestoreReport, Session, WriteProgress, apply_keymap_edit, execute_restore,
    plan_keymap_edit, prepare_restore, read_all,
};

fn options() -> RestoreOptions {
    RestoreOptions {
        backup: common::fixed_options(),
        ..RestoreOptions::default()
    }
}

/// Restores `target` onto `sim`, with Ctrl+C at the start of macro slot
/// `cancel_at`.
fn restore_cancelled_at(sim: &mut DeviceSim, target: &Backup, cancel_at: u8) -> RestoreReport {
    let prepared = prepare_restore(
        &mut Session::new(&mut *sim),
        target,
        &options(),
        consent(),
        |_| ControlFlow::Continue(()),
    )
    .expect("prepared");
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("safety backup");
    execute_restore(
        &mut Session::new(&mut *sim),
        &prepared,
        &saved,
        consent(),
        cancel_at_slot(cancel_at),
    )
}

/// Applies `edit` to `sim`, with Ctrl+C at the start of macro slot
/// `cancel_at`.
fn edit_cancelled_at(sim: &mut DeviceSim, edit: &KeymapEdit, cancel_at: u8) -> EditReport {
    let dir = temp_dir();
    let saved = read_all(&mut Session::new(&mut *sim))
        .expect("backup")
        .save_safety(dir.path(), "set")
        .expect("saved");
    let planned = plan_keymap_edit(&mut Session::new(&mut *sim), edit).expect("plan");
    apply_keymap_edit(
        &mut Session::new(&mut *sim),
        &planned,
        &saved,
        consent(),
        &EditOptions::default(),
        cancel_at_slot(cancel_at),
    )
}

/// Ctrl+C when macro slot `n` is about to be written.
fn cancel_at_slot(n: u8) -> impl FnMut(WriteProgress) -> ControlFlow<()> {
    move |progress| match progress {
        WriteProgress::Writing {
            table: TableId::Macro { slot },
            packet: 0,
            ..
        } if slot.get() == n => ControlFlow::Break(()),
        _ => ControlFlow::Continue(()),
    }
}

/// The tables of `items` and whether each was sent.
fn sent<'a>(items: impl Iterator<Item = (TableId, &'a ItemResult)>) -> Vec<(TableId, bool)> {
    items
        .map(|(table, result)| (table, result.was_sent()))
        .collect()
}

/// `10 <own usage of Q> 05 00`: Fn + Q plays macro slot 5.
fn q_on_slot_5() -> [u8; 4] {
    let own = openwraith_hid::protocol::W75
        .key(key_named("Q").index())
        .and_then(|k| k.hid_usage)
        .expect("usage");
    [0x10, own, 5, 0]
}

fn macro_table(n: u8) -> TableId {
    TableId::Macro {
        slot: macro_slot(n),
    }
}

const BASE: TableId = TableId::Keymap { layer: Layer::Base };
const FN: TableId = TableId::Keymap { layer: Layer::Fn };

/// Profile 1: macro 2 = B, macro 5 = F, no key bound to either.
fn keyboard() -> DeviceSim {
    let mut sim = DeviceSim::from_backup(&c06_backup());
    put_macro(&mut sim, profile(1), 2, &tap(0x05));
    put_macro(&mut sim, profile(1), 5, &tap(0x09));
    sim
}

/// The review case: macro 2 -> C, macro 5 -> E, W -> A on the base layer and
/// Fn + Q bound to macro 5 (written in that order: 19 02, 19 05, 18 00,
/// 18 02). Ctrl+C at the start of macro slot 5: macro 2 went out, nothing
/// else needs to. Before, the sent macro slot made the Fn keymap due, and it
/// went out with Fn + Q on macro slot 5, which still held the old macro F:
/// Fn + Q played the wrong macro and the report said nothing.
#[test]
fn a_cancellation_after_a_macro_slot_does_not_send_the_fn_keymap() {
    let p1 = profile(1);
    let mut source = keyboard();
    put_macro(&mut source, p1, 2, &tap(0x06));
    put_macro(&mut source, p1, 5, &tap(0x08));
    let mut base = source
        .table(p1, openwraith_hid::SimTable::Keymap(Layer::Base))
        .expect("keymap")
        .to_vec();
    let w = usize::from(key_named("W").index()) * 4;
    base[w..w + 4].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
    source.set_table(p1, openwraith_hid::SimTable::Keymap(Layer::Base), base);
    bind_macro(&mut source, p1, Layer::Fn, "Q", 5);
    let target = read_all(&mut Session::new(&mut source)).expect("backup");

    let mut sim = keyboard();
    let report = restore_cancelled_at(&mut sim, &target, 5);
    assert_eq!(report.outcome, Outcome::Cancelled, "{:#?}", report.error);
    assert_eq!(
        sent(report.items().map(|(_, i)| (i.table, &i.result))),
        [
            (macro_table(2), true),
            (macro_table(5), false),
            (BASE, false),
            (FN, false)
        ]
    );
    assert_eq!(kept_entry(&sim, p1, Layer::Fn, "Q"), [0; 4]);
    assert_eq!(kept_macro(&sim, p1, 5), macro_bytes(5, &tap(0x09)));
    assert_eq!(kept_macro(&sim, p1, 2), macro_bytes(2, &tap(0x06)));
    assert!(!sim.has_edits_waiting_for_fn_write());
    sim.assert_clean();

    // The same as an edit.
    let mut sim = keyboard();
    let edit = KeymapEdit {
        changes: vec![
            KeymapChange::SetMacroSlot {
                slot: macro_slot(2),
                program: Some(tap(0x06)),
            },
            KeymapChange::SetMacroSlot {
                slot: macro_slot(5),
                program: Some(tap(0x08)),
            },
            KeymapChange::Assign {
                layer: Layer::Base,
                pos: key_named("W"),
                assignment: KeyAssignment::Key(HidUsage::new(0x04).expect("usage")),
            },
            KeymapChange::Assign {
                layer: Layer::Fn,
                pos: key_named("Q"),
                assignment: KeyAssignment::Macro(macro_slot(5)),
            },
        ],
        ..KeymapEdit::default()
    };
    let report = edit_cancelled_at(&mut sim, &edit, 5);
    assert_eq!(report.outcome, Outcome::Cancelled, "{:#?}", report.error);
    assert_eq!(
        sent(report.items.iter().map(|i| (i.table, &i.result))),
        [
            (macro_table(2), true),
            (macro_table(5), false),
            (BASE, false),
            (FN, false)
        ]
    );
    assert_eq!(kept_entry(&sim, p1, Layer::Fn, "Q"), [0; 4]);
    assert_eq!(kept_macro(&sim, p1, 5), macro_bytes(5, &tap(0x09)));
    sim.assert_clean();
}

/// Macro 1 (Home's) is cleared, Home goes back to default, macro 5 -> E and
/// Fn + Q is bound to it. Ctrl+C at the start of macro slot 5: the clear of
/// slot 1 went out, so the base keymap must (Home would play an empty
/// slot), so the Fn keymap must (it saves the base keymap); and since the
/// Fn keymap binds Fn + Q to slot 5, slot 5 goes out too, or Fn + Q would
/// play the old macro F.
#[test]
fn a_macro_slot_a_layer_newly_refers_to_goes_out_with_that_layer() {
    let p1 = profile(1);
    let keyboard = || {
        let mut sim = DeviceSim::from_backup(&c06_backup());
        put_macro(&mut sim, p1, 1, &tap(0x05));
        bind_macro(&mut sim, p1, Layer::Base, "Home", 1);
        put_macro(&mut sim, p1, 5, &tap(0x09));
        sim
    };
    let mut source = DeviceSim::from_backup(&c06_backup());
    put_macro(&mut source, p1, 5, &tap(0x08));
    bind_macro(&mut source, p1, Layer::Fn, "Q", 5);
    let target = read_all(&mut Session::new(&mut source)).expect("backup");

    let mut sim = keyboard();
    let report = restore_cancelled_at(&mut sim, &target, 5);
    assert!(report.cancel_requested);
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(
        sent(report.items().map(|(_, i)| (i.table, &i.result))),
        [
            (macro_table(1), true),
            (macro_table(5), true),
            (BASE, true),
            (FN, true)
        ]
    );
    assert_eq!(kept_entry(&sim, p1, Layer::Fn, "Q"), q_on_slot_5());
    assert_eq!(kept_macro(&sim, p1, 5), macro_bytes(5, &tap(0x08)));
    assert_eq!(kept_entry(&sim, p1, Layer::Base, "Home"), [0; 4]);
    assert!(sim.dangling_references(p1, true).is_empty());
    sim.assert_clean();

    // The same as an edit.
    let mut sim = keyboard();
    let edit = KeymapEdit {
        changes: vec![
            KeymapChange::Unbind {
                layer: Layer::Base,
                pos: key_named("Home"),
            },
            KeymapChange::SetMacroSlot {
                slot: macro_slot(5),
                program: Some(tap(0x08)),
            },
            KeymapChange::Assign {
                layer: Layer::Fn,
                pos: key_named("Q"),
                assignment: KeyAssignment::Macro(macro_slot(5)),
            },
        ],
        ..KeymapEdit::default()
    };
    let report = edit_cancelled_at(&mut sim, &edit, 5);
    assert!(report.cancel_requested);
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(
        sent(report.items.iter().map(|i| (i.table, &i.result))),
        [
            (macro_table(1), true),
            (macro_table(5), true),
            (BASE, true),
            (FN, true)
        ]
    );
    assert_eq!(kept_entry(&sim, p1, Layer::Fn, "Q"), q_on_slot_5());
    assert_eq!(kept_macro(&sim, p1, 5), macro_bytes(5, &tap(0x08)));
    sim.assert_clean();
}
