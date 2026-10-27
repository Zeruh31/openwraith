//! `plan_restore` against edited backups: every skip reason, block cause and
//! plan warning. Pure: no keyboard, no simulator.

mod common;

use common::{c06_backup, fixed_options, pos};
use openwraith_hid::protocol::{
    CustomTable, KeyPos, Layer, MacroSlotId, RawResponse, ReadRequest, Report, TableId,
};
use openwraith_hid::{
    Backup, BackupEntry, BlockCause, CommitTriggerReason, PlanAction, PlanWarning, RestoreOptions,
    RestorePlan, SkipReason, plan_restore,
};

fn options() -> RestoreOptions {
    RestoreOptions {
        backup: fixed_options(),
        ..RestoreOptions::default()
    }
}

/// `backup` with chunk `chunk` of the reply to `request` changed by `edit`
/// (on the 63 report bytes).
fn edited(
    backup: &Backup,
    request: ReadRequest,
    chunk: usize,
    edit: impl FnOnce(&mut [u8; 63]),
) -> Backup {
    let mut backup = backup.clone();
    let entry = backup
        .entries
        .iter_mut()
        .find(|e| e.request() == request)
        .expect("request in the backup");
    let mut reports: Vec<Report> = entry.raw.reports().to_vec();
    let mut bytes = *reports[chunk].as_bytes();
    edit(&mut bytes);
    reports[chunk] = Report::new(bytes);
    *entry = BackupEntry::new(RawResponse::from_reports(request, reports).expect("valid reply"));
    backup
}

/// `backup` without the reply to `request`.
fn without(backup: &Backup, request: ReadRequest) -> Backup {
    let mut backup = backup.clone();
    backup.entries.retain(|e| e.request() != request);
    backup
}

/// The trigger record of `key` in `backup` changed by `edit` (record bytes
/// `MM A A2 P R Ah A2h Ph Rh` at report bytes 6..15).
fn trigger(backup: &Backup, key: KeyPos, edit: impl FnOnce(&mut [u8])) -> Backup {
    edited(backup, ReadRequest::KeyTrigger(key), 0, |b| {
        edit(&mut b[6..15])
    })
}

fn plan(target: &Backup, current: &Backup) -> RestorePlan {
    plan_restore(target, current, &options()).expect("plan")
}

/// The action planned for `table` (and `key`) in profile 1.
fn action(plan: &RestorePlan, table: TableId, key: Option<KeyPos>) -> PlanAction {
    plan.profiles[0]
        .items
        .iter()
        .find(|i| i.table == table && i.key == key)
        .map(|i| i.action.clone())
        .unwrap_or_else(|| panic!("{table} {key:?} not in the plan"))
}

fn reason(plan: &RestorePlan, table: TableId, key: Option<KeyPos>) -> SkipReason {
    match action(plan, table, key) {
        PlanAction::Skip { reason, .. } => reason,
        other => panic!("{table} {key:?}: not skipped but {other:?}"),
    }
}

fn is_write(plan: &RestorePlan, table: TableId, key: Option<KeyPos>) -> bool {
    matches!(action(plan, table, key), PlanAction::Write(_))
}

const Q: (u8, u8) = (2, 1);
const W: (u8, u8) = (2, 2);
const E: (u8, u8) = (2, 3);
const R: (u8, u8) = (2, 4);
const T: (u8, u8) = (2, 5);
const Y: (u8, u8) = (2, 6);

fn key((row, col): (u8, u8)) -> KeyPos {
    pos(row, col)
}

/// The c06 keyboard with W top dead band `value` (offset 92 = chunk 1,
/// report byte 39).
fn dead_band(backup: &Backup, value: u8) -> Backup {
    edited(backup, ReadRequest::DeadBand, 1, |b| b[39] = value)
}

#[test]
fn trigger_records_that_cannot_be_written_exactly_are_skipped() {
    let current = c06_backup();
    let mut target = current.clone();
    // Q: Rapid Trigger 0C with press 0.30 and release 0.50.
    target = trigger(&target, key(Q), |r| {
        r[0] = 0x0c;
        r[3] = 0x0f;
        r[4] = 0x19;
    });
    // E: second travel differs from the actuation point.
    target = trigger(&target, key(E), |r| r[2] = r[1] + 1);
    // R: 3.42 mm (171 units), above the maximum of 3.40 mm.
    target = trigger(&target, key(R), |r| {
        r[1] = 171;
        r[2] = 171;
    });
    // T: mode 06 (the "reset all triggers" sub-function).
    target = trigger(&target, key(T), |r| r[0] = 0x06);
    // Y: a valid change.
    target = trigger(&target, key(Y), |r| {
        r[1] = 0x40;
        r[2] = 0x40;
    });
    let plan = plan(&target, &current);
    let triggers = TableId::Triggers;
    assert_eq!(
        reason(&plan, triggers, Some(key(Q))),
        SkipReason::PressReleaseDiffer
    );
    assert_eq!(
        reason(&plan, triggers, Some(key(E))),
        SkipReason::SecondTravelDiffers
    );
    assert_eq!(
        reason(&plan, triggers, Some(key(R))),
        SkipReason::OutOfRange
    );
    assert_eq!(
        reason(&plan, triggers, Some(key(T))),
        SkipReason::UnknownTriggerMode { mode: 0x06 }
    );
    assert!(is_write(&plan, triggers, Some(key(Y))));
    // Unchanged keys are not listed.
    assert!(plan.profiles[0].items.iter().all(|i| i.key != Some(key(W))));
}

#[test]
fn a_firmware_update_that_shrinks_the_travel_range_skips_only_the_keys_out_of_it() {
    let target = trigger(&c06_backup(), key(R), |r| {
        r[1] = 150;
        r[2] = 150;
    });
    let target = trigger(&target, key(Y), |r| {
        r[1] = 0x32;
        r[2] = 0x32;
    });
    // The keyboard now reports a maximum travel of 2.00 mm (100 units).
    let current = edited(&c06_backup(), ReadRequest::TriggerGlobal, 0, |b| {
        b[6] = 100;
        b[9] = 0;
    });
    let plan = plan(&target, &current);
    assert_eq!(
        reason(&plan, TableId::Triggers, Some(key(R))),
        SkipReason::OutOfRange
    );
    assert!(is_write(&plan, TableId::Triggers, Some(key(Y))));
    assert!(plan.warnings.contains(&PlanWarning::ConstantDiffers {
        request: ReadRequest::TriggerGlobal
    }));
}

#[test]
fn unusable_trigger_parameters_skip_every_trigger() {
    let target = trigger(&c06_backup(), key(Y), |r| {
        r[1] = 0x40;
        r[2] = 0x40;
    });
    // A minimum travel of 0 means "unsupported".
    let current = edited(&c06_backup(), ReadRequest::TriggerGlobal, 0, |b| b[10] = 0);
    assert_eq!(
        reason(&plan(&target, &current), TableId::Triggers, Some(key(Y))),
        SkipReason::TriggerLimitsUnknown
    );
}

#[test]
fn another_distance_unit_skips_triggers_and_the_dead_band() {
    let target = trigger(&c06_backup(), key(Y), |r| {
        r[1] = 0x40;
        r[2] = 0x40;
    });
    let target = dead_band(&target, 5);
    // The keyboard reports 0.01 mm units (21/04 raw unit 1, max 3.40 mm =
    // 340 units, min 10).
    let ten_um = edited(&c06_backup(), ReadRequest::TriggerGlobal, 0, |b| {
        b[6] = 0x54;
        b[7] = 0x01;
        b[9] = 0x01;
        b[10] = 0x0a;
    });
    // The keyboard sets the high-precision flag (status byte 18 bit 0):
    // 0.002 mm units.
    let two_um = edited(&c06_backup(), ReadRequest::Status, 0, |b| b[18] |= 0x01);
    for (current, keyboard_um) in [(ten_um, 10), (two_um, 2)] {
        let plan = plan(&target, &current);
        let expected = SkipReason::DistanceUnitChanged {
            backup_um: Some(20),
            keyboard_um: Some(keyboard_um),
        };
        assert_eq!(
            reason(&plan, TableId::Triggers, Some(key(Y))),
            expected,
            "{keyboard_um} um"
        );
        assert_eq!(reason(&plan, TableId::DeadBand, None), expected);
        assert!(
            plan.writes()
                .all(|(_, i, _)| !matches!(i.table, TableId::Triggers | TableId::DeadBand)),
            "{keyboard_um} um"
        );
    }
    // A backup made with other units is not restored onto a 0.02 mm
    // keyboard either.
    let other_backup = edited(&target, ReadRequest::Status, 0, |b| b[18] |= 0x01);
    let plan = plan(&other_backup, &c06_backup());
    assert_eq!(
        reason(&plan, TableId::DeadBand, None),
        SkipReason::DistanceUnitChanged {
            backup_um: Some(2),
            keyboard_um: Some(20)
        }
    );
}

#[test]
fn dead_band_skips() {
    // 51 units (1.02 mm) is above the maximum of 50.
    let plan_51 = plan(&dead_band(&c06_backup(), 51), &c06_backup());
    assert_eq!(
        reason(&plan_51, TableId::DeadBand, None),
        SkipReason::DeadBandOutOfRange
    );
    // 50 is the maximum and is written.
    let plan_50 = plan(&dead_band(&c06_backup(), 50), &c06_backup());
    assert!(is_write(&plan_50, TableId::DeadBand, None));
    // Without the keyboard's 21/04 the unit is unknown.
    let current = without(&c06_backup(), ReadRequest::TriggerGlobal);
    let unknown_unit = plan(&dead_band(&c06_backup(), 5), &current);
    assert_eq!(
        reason(&unknown_unit, TableId::DeadBand, None),
        SkipReason::DeadBandUnitUnknown
    );
    // The keyboard did not report its dead band.
    let current = without(&c06_backup(), ReadRequest::DeadBand);
    let missing = plan(&dead_band(&c06_backup(), 5), &current);
    assert_eq!(
        reason(&missing, TableId::DeadBand, None),
        SkipReason::NotInKeyboard
    );
    // A backup without a dead band.
    let target = without(&c06_backup(), ReadRequest::DeadBand);
    assert_eq!(
        action(&plan_for(&target), TableId::DeadBand, None),
        PlanAction::NotInBackup
    );
}

fn plan_for(target: &Backup) -> RestorePlan {
    plan(target, &c06_backup())
}

#[test]
fn light_records_that_the_keyboard_does_not_accept_are_skipped() {
    // Mode 05 is not in the W75's list.
    let target = edited(&c06_backup(), ReadRequest::MainLight, 0, |b| b[5] = 0x05);
    assert_eq!(
        reason(&plan_for(&target), TableId::MainLight, None),
        SkipReason::UnsupportedLightMode { mode: 0x05 }
    );
    // Music rhythm is never written.
    let target = edited(&c06_backup(), ReadRequest::MainLight, 0, |b| b[5] = 0x64);
    assert_eq!(
        reason(&plan_for(&target), TableId::MainLight, None),
        SkipReason::UnsupportedLightMode { mode: 0x64 }
    );
    // Brightness 9 is above the maximum of 4.
    let target = edited(&c06_backup(), ReadRequest::MainLight, 0, |b| b[6] = 9);
    assert_eq!(
        reason(&plan_for(&target), TableId::MainLight, None),
        SkipReason::InvalidLightRecord
    );
    // A valid change is written.
    let target = edited(&c06_backup(), ReadRequest::MainLight, 0, |b| b[6] = 1);
    assert!(is_write(&plan_for(&target), TableId::MainLight, None));
}

#[test]
fn unusable_replies_are_skipped() {
    // Custom colour table 1 whose last chunk declares 54 bytes instead of
    // 18.
    let colours = ReadRequest::CustomColors(CustomTable::Custom1);
    let target = edited(&c06_backup(), colours, 7, |b| b[4] = 0x36);
    assert_eq!(
        reason(
            &plan_for(&target),
            TableId::CustomColors {
                table: CustomTable::Custom1
            },
            None
        ),
        SkipReason::Unusable
    );
}

#[test]
fn a_malformed_reference_blocks_the_keymap_group() {
    // Caps (index 66 = chunk 4, report byte 45) bound to macro slot 12.
    let target = edited(&c06_backup(), ReadRequest::Keymap(Layer::Base), 4, |b| {
        b[45..49].copy_from_slice(&[0x10, 0x39, 0x0c, 0x00]);
    });
    assert_eq!(
        reason(
            &plan_for(&target),
            TableId::Keymap { layer: Layer::Base },
            None
        ),
        SkipReason::KeymapGroupBlocked {
            cause: BlockCause::MalformedReference {
                layer: Layer::Base,
                index: 66
            }
        }
    );
}

#[test]
fn a_keymap_table_missing_on_the_keyboard_blocks_the_group() {
    // W -> A in the backup; the keyboard did not report its Fn layer.
    let target = edited(&c06_backup(), ReadRequest::Keymap(Layer::Base), 3, |b| {
        // Index 46 = offset 184 = chunk 3 (168..224), report byte 21.
        b[21..25].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
    });
    let current = without(&c06_backup(), ReadRequest::Keymap(Layer::Fn));
    let plan = plan(&target, &current);
    assert_eq!(
        reason(&plan, TableId::Keymap { layer: Layer::Base }, None),
        SkipReason::KeymapGroupBlocked {
            cause: BlockCause::Incomplete {
                table: TableId::Keymap { layer: Layer::Fn }
            }
        }
    );
    assert_eq!(
        reason(&plan, TableId::Keymap { layer: Layer::Fn }, None),
        SkipReason::NotInKeyboard
    );
}

#[test]
fn device_constants_that_differ_are_warnings() {
    // Another first mode in the 0A list, and another capability flag.
    let current = edited(&c06_backup(), ReadRequest::MainLightModes, 0, |b| {
        b[5] ^= 0x01;
    });
    let current = edited(&current, ReadRequest::Status, 0, |b| b[17] ^= 0x80);
    let plan = plan(&c06_backup(), &current);
    assert!(plan.warnings.contains(&PlanWarning::ConstantDiffers {
        request: ReadRequest::MainLightModes
    }));
    assert!(plan.warnings.contains(&PlanWarning::CapabilityFlagsDiffer));
    assert!(!plan.has_writes());
}

#[test]
fn writing_a_table_of_unknown_scope_warns_about_the_other_profiles() {
    // A single-profile backup that changes the dead band, whose profile
    // scope is unknown (key index 0 top = report byte 5 of chunk 0).
    let target = edited(&c06_backup(), ReadRequest::DeadBand, 0, |b| b[5] = 0x05);
    let plan = plan_for(&target);
    assert!(
        plan.warnings
            .contains(&PlanWarning::SharedTablesMayChangeOtherProfiles {
                tables: vec![TableId::DeadBand],
                profiles: vec![2, 3, 4],
            }),
        "{:?}",
        plan.warnings
    );
    // Custom colour table 1 is per profile (OBSERVED, spec 7.3): no warning.
    let target = edited(
        &c06_backup(),
        ReadRequest::CustomColors(CustomTable::Custom1),
        0,
        |b| b[5] ^= 0xff,
    );
    assert!(
        plan_for(&target)
            .warnings
            .iter()
            .all(|w| !matches!(w, PlanWarning::SharedTablesMayChangeOtherProfiles { .. }))
    );
    // A change of a per-profile table only (the main light) does not warn.
    let target = edited(&c06_backup(), ReadRequest::MainLight, 0, |b| b[6] = 1);
    assert!(
        plan_for(&target)
            .warnings
            .iter()
            .all(|w| !matches!(w, PlanWarning::SharedTablesMayChangeOtherProfiles { .. }))
    );
}

/// The backup's Fn keymap reply cannot be written (chunk 9 declares 56
/// bytes instead of 24); the keyboard holds the same reply.
fn unusable_fn(backup: &Backup) -> Backup {
    edited(backup, ReadRequest::Keymap(Layer::Fn), 9, |b| b[4] = 0x38)
}

/// W -> A in the base keymap (index 46 = chunk 3, report byte 21).
fn w_to_a(backup: &Backup) -> Backup {
    edited(backup, ReadRequest::Keymap(Layer::Base), 3, |b| {
        b[21..25].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
    })
}

#[test]
fn the_base_keymap_is_never_planned_without_the_fn_keymap_after_it() {
    let base = TableId::Keymap { layer: Layer::Base };
    let fn_layer = TableId::Keymap { layer: Layer::Fn };
    // The base keymap differs, the Fn keymap cannot be written: the group
    // is blocked, nothing is written.
    let plan = plan(
        &unusable_fn(&w_to_a(&c06_backup())),
        &unusable_fn(&c06_backup()),
    );
    let cause = BlockCause::FnKeymapNotWritable {
        reason: CommitTriggerReason::BaseKeymap,
    };
    assert_eq!(
        reason(&plan, base, None),
        SkipReason::KeymapGroupBlocked {
            cause: cause.clone()
        }
    );
    match action(&plan, base, None) {
        PlanAction::Skip { message, .. } => {
            assert!(message.contains("not written alone"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(action(&plan, fn_layer, None), PlanAction::Unchanged);
    assert!(!plan.has_writes());
    let json = serde_json::to_value(&plan).expect("serializes");
    assert!(
        json.to_string().contains("fn_keymap_not_writable"),
        "{json}"
    );

    // Macro slots alone: blocked the same way (the precaution).
    let with_macro = edited(&c06_backup(), ReadRequest::Macro(slot0()), 0, |b| b[5] = 0);
    let plan = plan_restore(
        &unusable_fn(&c06_backup()),
        &unusable_fn(&with_macro),
        &options(),
    )
    .expect("plan");
    assert_eq!(
        reason(&plan, TableId::Macro { slot: slot0() }, None),
        SkipReason::KeymapGroupBlocked {
            cause: BlockCause::FnKeymapNotWritable {
                reason: CommitTriggerReason::MacrosOrAdvancedKeys
            }
        }
    );
    assert!(!plan.has_writes());

    // The keyboard did not report its Fn keymap: blocked too.
    let missing = plan_restore(
        &w_to_a(&c06_backup()),
        &without(&c06_backup(), ReadRequest::Keymap(Layer::Fn)),
        &options(),
    )
    .expect("plan");
    assert!(!missing.has_writes());
}

fn slot0() -> MacroSlotId {
    MacroSlotId::new(0).expect("slot 0")
}

#[test]
fn the_fn_keymap_follows_the_base_keymap_changed_or_not() {
    let base = TableId::Keymap { layer: Layer::Base };
    let fn_layer = TableId::Keymap { layer: Layer::Fn };
    let order = |plan: &RestorePlan| -> Vec<(TableId, Option<CommitTriggerReason>, bool)> {
        plan.writes()
            .map(|(_, item, w)| (item.table, w.commit_trigger, w.is_resent_unchanged()))
            .collect()
    };
    // Only the base keymap differs: the unchanged Fn keymap is re-sent.
    let plan = plan_restore(&w_to_a(&c06_backup()), &c06_backup(), &options()).expect("plan");
    assert_eq!(
        order(&plan),
        [
            (base, None, false),
            (fn_layer, Some(CommitTriggerReason::BaseKeymap), true)
        ]
    );
    assert_eq!(plan.packet_count(), 20);
    // Re-sending unchanged bytes changes no other profile: no warning.
    assert!(
        plan.warnings
            .iter()
            .all(|w| !matches!(w, PlanWarning::SharedTablesMayChangeOtherProfiles { .. })),
        "{:?}",
        plan.warnings
    );

    // Both differ (Fn index 130 = chunk 9, report byte 21): the Fn write
    // is a real change and still the trigger.
    let target = edited(
        &w_to_a(&c06_backup()),
        ReadRequest::Keymap(Layer::Fn),
        9,
        |b| {
            b[21..25].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
        },
    );
    let plan = plan_restore(&target, &c06_backup(), &options()).expect("plan");
    assert_eq!(
        order(&plan),
        [
            (base, None, false),
            (fn_layer, Some(CommitTriggerReason::BaseKeymap), false)
        ]
    );
    let fn_write = plan
        .writes()
        .find(|(_, item, _)| item.table == fn_layer)
        .map(|(_, _, w)| w)
        .expect("Fn write");
    assert_eq!(fn_write.summary, "Fn keymap (18 02)");
    assert_eq!(fn_write.changes.len(), 1);
    // The Fn keymap is per profile (OBSERVED, spec 7.4): no warning about
    // the other profiles.
    assert!(plan.warnings.iter().all(|w| !matches!(
        w,
        PlanWarning::SharedTablesMayChangeOtherProfiles { tables, .. } if tables.contains(&fn_layer)
    )));

    // Only the Fn keymap differs: a plain write, no trigger.
    let target = edited(&c06_backup(), ReadRequest::Keymap(Layer::Fn), 9, |b| {
        b[21..25].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
    });
    assert_eq!(order(&plan_for(&target)), [(fn_layer, None, false)]);
}
