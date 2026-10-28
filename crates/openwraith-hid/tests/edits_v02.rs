//! Colour, dead-band and preset edits against the simulator.

mod common;

use std::ops::ControlFlow;

use common::{c06_backup, consent, pos, profile, selects, temp_dir, write_bytes};
use openwraith_hid::protocol::{Distance, KeyPos, Rgb, TriggerWriteMode};
use openwraith_hid::{
    ColorEdit, DeadBandEdit, DeviceSim, EditError, EditOptions, EditWarning, SavedBackup, Session,
    SimTable, TRIGGER_PRESETS, WriteOptions, apply_color_edit, apply_dead_band_edit,
    apply_trigger_edit, plan_color_edit, plan_dead_band_edit, plan_trigger_edit, preset_edit,
    read_all, trigger_preset,
};

fn sim() -> DeviceSim {
    DeviceSim::from_backup(&c06_backup())
}

fn safety(sim: &mut DeviceSim, dir: &tempfile::TempDir) -> SavedBackup {
    let backup = read_all(&mut Session::new(sim)).expect("backup");
    backup.save_safety(dir.path(), "set").expect("saved")
}

fn key(name: &str) -> KeyPos {
    openwraith_hid::protocol::W75
        .key_by_name(name)
        .and_then(|k| k.pos())
        .unwrap_or_else(|| panic!("no key {name}"))
}

fn slot(table: &[u8], index: usize) -> [u8; 3] {
    [table[index * 3], table[index * 3 + 1], table[index * 3 + 2]]
}

const BLUE: Rgb = Rgb {
    r: 0,
    g: 0,
    b: 0xff,
};

#[test]
fn key_colours_change_only_the_keys_and_keep_every_other_slot() {
    let mut sim = sim();
    let p1 = profile(1);
    // Distinct colours outside the layout and at the knob.
    let mut table = sim
        .table(p1, SimTable::CustomColors1)
        .expect("colours")
        .to_vec();
    for (index, colour) in [(15usize, [1, 2, 3]), (103, [4, 5, 6]), (126, [7, 8, 9])] {
        table[index * 3..index * 3 + 3].copy_from_slice(&colour);
    }
    sim.set_table(p1, SimTable::CustomColors1, table.clone());
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let mut session = Session::new(&mut sim);
    let plan = plan_color_edit(
        &mut session,
        &ColorEdit {
            colors: vec![(key("Up"), BLUE), (key("Right"), BLUE)],
            switch_to_custom: false,
        },
    )
    .expect("plan");
    // c06 lights in the Custom mode: the table is shown.
    assert!(plan.shown_now());
    assert_eq!(plan.changes().len(), 2);
    let report = apply_color_edit(
        &mut session,
        &plan,
        &saved,
        consent(),
        &EditOptions::default(),
        |_| ControlFlow::Continue(()),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    drop(session);
    // Only the colour table (8 chunks), no light write, no profile switch
    // (custom colours 1 are known to be kept).
    let writes = write_bytes(&sim);
    assert_eq!(writes.len(), 8);
    assert!(writes.iter().all(|b| b[..2] == [0x09, 0x00]));
    assert!(selects(&sim).is_empty());
    let after = sim.table(p1, SimTable::CustomColors1).expect("colours");
    assert_eq!(slot(after, 102), [0, 0, 0xff]);
    assert_eq!(slot(after, 125), [0, 0, 0xff]);
    for index in [15usize, 103, 126] {
        assert_eq!(slot(after, index), slot(&table, index), "slot {index}");
    }
    // Every other slot as before.
    let differing: Vec<usize> = (0..132)
        .filter(|i| slot(after, *i) != slot(&table, *i))
        .collect();
    assert_eq!(differing, [102, 125]);
    // Custom colours 1 survive a power cycle (H22, 2026-10-27): no note.
    assert_eq!(report.warnings, []);
    sim.assert_clean();
}

#[test]
fn colours_in_another_light_mode_are_shown_only_after_switching_to_custom() {
    let mut sim = sim();
    sim.set_table(
        profile(1),
        SimTable::MainLight,
        openwraith_hid::sim::FACTORY_LIGHT.to_vec(),
    );
    let edit = |switch_to_custom| ColorEdit {
        colors: vec![(key("Up"), BLUE)],
        switch_to_custom,
    };
    let plan = plan_color_edit(&mut Session::new(&mut sim), &edit(false)).expect("plan");
    assert!(!plan.shown_now());
    assert!(
        plan.warnings()
            .contains(&EditWarning::NotShownUntilCustomMode)
    );
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let mut session = Session::new(&mut sim);
    let plan = plan_color_edit(&mut session, &edit(true)).expect("plan");
    let report = apply_color_edit(
        &mut session,
        &plan,
        &saved,
        consent(),
        &EditOptions::default(),
        |_| ControlFlow::Continue(()),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    drop(session);
    // The table, then the music-rhythm leave and the light record with the
    // Custom mode; the foreground colour is kept.
    let heads: Vec<(u8, u8)> = write_bytes(&sim).iter().map(|b| (b[0], b[1])).collect();
    let mut expected = vec![(0x09, 0x00); 8];
    expected.extend([(0x17, 0x00), (0x07, 0x00)]);
    assert_eq!(heads, expected);
    let light = sim.table(profile(1), SimTable::MainLight).expect("light");
    assert_eq!(light[0], 0x0a);
    assert_eq!(light[1..4], openwraith_hid::sim::FACTORY_LIGHT[1..4]);
    sim.assert_clean();
}

#[test]
fn the_knob_has_no_colour_that_can_be_changed() {
    let mut sim = sim();
    let knob = KeyPos::from_index(15).expect("knob");
    let result = plan_color_edit(
        &mut Session::new(&mut sim),
        &ColorEdit {
            colors: vec![(knob, BLUE)],
            switch_to_custom: false,
        },
    );
    assert!(matches!(result, Err(EditError::Build(_))));
    assert!(write_bytes(&sim).is_empty());
}

#[test]
fn a_dead_band_edit_writes_the_whole_table_with_the_keys_changed() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let mut session = Session::new(&mut sim);
    let plan = plan_dead_band_edit(
        &mut session,
        &DeadBandEdit {
            keys: vec![pos(2, 2)],
            top: Some(Distance(5)),
            bottom: Some(Distance(10)),
        },
    )
    .expect("plan");
    assert_eq!(plan.changes().len(), 1);
    let report = apply_dead_band_edit(
        &mut session,
        &plan,
        &saved,
        consent(),
        &EditOptions::default(),
        |_| ControlFlow::Continue(()),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    drop(session);
    // c12's layout: W top 0.10 mm at offset 92, bottom 0.20 mm at 93.
    let table = sim
        .table(profile(1), SimTable::DeadBand)
        .expect("dead band");
    assert_eq!(table[92..94], [0x05, 0x0a]);
    assert_eq!(table.iter().filter(|b| **b != 0).count(), 2);
    // Each profile has its own dead band (H16, 2026-10-27).
    assert!(
        sim.persisted_table(profile(2), SimTable::DeadBand)
            .expect("dead band")
            .iter()
            .all(|b| *b == 0)
    );
    let writes = write_bytes(&sim);
    assert_eq!(writes.len(), 5);
    assert!(writes.iter().all(|b| b[..2] == [0x26, 0x01]));
    assert!(selects(&sim).is_empty());
    sim.assert_clean();
}

#[test]
fn dead_band_values_outside_the_range_and_the_knob_are_refused() {
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let too_deep = plan_dead_band_edit(
        &mut session,
        &DeadBandEdit {
            keys: vec![pos(2, 2)],
            top: Some(Distance(51)),
            bottom: None,
        },
    );
    assert!(matches!(too_deep, Err(EditError::Build(_))));
    let knob = plan_dead_band_edit(
        &mut session,
        &DeadBandEdit {
            keys: vec![KeyPos::from_index(15).expect("knob")],
            top: Some(Distance(5)),
            bottom: None,
        },
    );
    assert!(matches!(knob, Err(EditError::Invalid { .. })));
    drop(session);
    assert!(write_bytes(&sim).is_empty());
}

#[test]
fn a_preset_is_a_trigger_edit_one_key_per_packet() {
    let mut sim = sim();
    let dir = temp_dir();
    let saved = safety(&mut sim, &dir);
    let fast = trigger_preset("fast").expect("preset");
    assert_eq!(TRIGGER_PRESETS.len(), 5);
    let mut session = Session::new(&mut sim);
    let plan = plan_trigger_edit(&mut session, &preset_edit(fast, vec![pos(2, 2), pos(2, 1)]))
        .expect("plan");
    let report = apply_trigger_edit(
        &mut session,
        &plan,
        &saved,
        consent(),
        &WriteOptions::default(),
        |_| ControlFlow::Continue(()),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    drop(session);
    for key in [pos(2, 2), pos(2, 1)] {
        let record = sim.trigger(profile(1), key).expect("trigger");
        assert_eq!(record[0], TriggerWriteMode::RapidTrigger.byte());
        assert_eq!(record[1..5], [40, 40, 8, 8]);
    }
    assert_eq!(write_bytes(&sim).len(), 2);
    sim.assert_clean();
}
