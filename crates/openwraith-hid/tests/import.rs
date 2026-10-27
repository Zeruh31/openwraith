//! v0.2 restores against the simulator: non-empty advanced-key slots and
//! SOCD are restorable (the undo of every editor), exporting one profile
//! and importing it into another, and the backup size limit.

mod common;

use std::ops::ControlFlow;

use common::{c06_backup, consent, pos, profile, sim_with_profiles, temp_dir, write_bytes};
use openwraith_hid::protocol::write::{ModTap, Toggle};
use openwraith_hid::protocol::{
    AdvancedKeyDef, AdvancedSlotId, HidUsage, Layer, ProfileId, ReadRequest, TableId,
};
use openwraith_hid::{
    BACKUP_MAX_FILE_BYTES, BACKUP_SCOPE_EXPORT, Backup, BackupError, BlockCause, DeviceSim,
    EditOptions, ImportOptions, ImportSelection, ItemResult, KeymapChange, KeymapEdit, PlanAction,
    PlanWarning, PreparedRestore, ProfileSet, ProfilesOptions, RestoreOptions, RestoreReport,
    ScopeConfig, Session, SimTable, SkipReason, UsbInfo, apply_keymap_edit, execute_restore,
    plan_keymap_edit, prepare_restore, read_all, read_profiles,
};

fn usage(u: u8) -> HidUsage {
    HidUsage::new(u).expect("offered usage")
}

fn key(name: &str) -> openwraith_hid::protocol::KeyPos {
    openwraith_hid::protocol::W75
        .key_by_name(name)
        .and_then(|k| k.pos())
        .unwrap_or_else(|| panic!("no key {name}"))
}

fn options() -> RestoreOptions {
    RestoreOptions {
        backup: common::fixed_options(),
        ..RestoreOptions::default()
    }
}

fn prepare(sim: &mut DeviceSim, target: &Backup, options: &RestoreOptions) -> PreparedRestore {
    prepare_restore(&mut Session::new(sim), target, options, consent(), |_| {
        ControlFlow::Continue(())
    })
    .expect("prepared")
}

fn restore_with(sim: &mut DeviceSim, target: &Backup, options: &RestoreOptions) -> RestoreReport {
    let prepared = prepare(sim, target, options);
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("saved");
    execute_restore(&mut Session::new(sim), &prepared, &saved, consent(), |_| {
        ControlFlow::Continue(())
    })
}

/// Binds Mod-Tap to G and Toggle to H on profile 1 of `sim`.
fn bind_advanced_keys(sim: &mut DeviceSim) {
    let edit = KeymapEdit {
        changes: vec![
            KeymapChange::BindAdvanced {
                layer: Layer::Base,
                pos: key("G"),
                def: AdvancedKeyDef::ModTap(ModTap {
                    hold: usage(0xe0),
                    tap: usage(0x0a),
                    threshold_10ms: 30,
                }),
                slot: None,
            },
            KeymapChange::BindAdvanced {
                layer: Layer::Base,
                pos: key("H"),
                def: AdvancedKeyDef::Toggle(Toggle { key: usage(0x0b) }),
                slot: None,
            },
        ],
        ..KeymapEdit::default()
    };
    let dir = temp_dir();
    let saved = read_all(&mut Session::new(&mut *sim))
        .expect("backup")
        .save_safety(dir.path(), "set")
        .expect("saved");
    let mut session = Session::new(&mut *sim);
    let plan = plan_keymap_edit(&mut session, &edit).expect("plan");
    let report = apply_keymap_edit(
        &mut session,
        &plan,
        &saved,
        consent(),
        &EditOptions::default(),
        |_| ControlFlow::Continue(()),
    );
    assert!(report.is_success(), "{:#?}", report.error);
}

/// Switches SOCD on with A/D (wire mode 0) in profile 1 of `sim`.
fn socd_on(sim: &mut DeviceSim) {
    let p1 = profile(1);
    sim.set_table(p1, SimTable::SocdSettings, vec![1, 0, 0, 0, 0xfa, 0xe5]);
    let mut pairs = vec![0u8; 80];
    pairs[..4].copy_from_slice(&[0x01, 0x00, 0x04, 0x07]);
    sim.set_table(p1, SimTable::SocdPairs, pairs);
}

#[test]
fn advanced_keys_and_socd_are_restored_after_a_factory_reset() {
    let mut source = DeviceSim::from_backup(&c06_backup());
    bind_advanced_keys(&mut source);
    socd_on(&mut source);
    let target = read_all(&mut Session::new(&mut source)).expect("backup");

    let mut sim = DeviceSim::from_backup(&c06_backup());
    sim.factory_reset_all();
    let first = write_bytes(&sim).len();
    let report = restore_with(&mut sim, &target, &options());
    assert!(report.is_success(), "{:#?}", report.error);
    // Site order within the keymap group; SOCD before the group.
    let order: Vec<TableId> = report
        .items()
        .map(|(_, i)| i.table)
        .filter(|t| {
            matches!(
                t,
                TableId::Socd | TableId::Keymap { .. } | TableId::AdvancedKey { .. }
            )
        })
        .collect();
    let slot = |n| TableId::AdvancedKey {
        slot: AdvancedSlotId::new(n).expect("slot"),
    };
    assert_eq!(
        order,
        [
            TableId::Socd,
            TableId::Keymap { layer: Layer::Base },
            TableId::Keymap { layer: Layer::Fn },
            slot(0),
            slot(1),
        ]
    );
    let p1 = profile(1);
    for table in [
        SimTable::AdvancedKeys,
        SimTable::Keymap(Layer::Base),
        SimTable::SocdSettings,
        SimTable::SocdPairs,
    ] {
        let request = match table {
            SimTable::AdvancedKeys => ReadRequest::AdvancedKeys,
            SimTable::Keymap(layer) => ReadRequest::Keymap(layer),
            SimTable::SocdSettings => ReadRequest::SocdSettings,
            _ => ReadRequest::SocdPairs,
        };
        let raw = target.response(request).expect("reply");
        let expected = raw.table_bytes("t").unwrap_or_else(|_| raw.payload());
        assert_eq!(
            sim.persisted_table(p1, table),
            Some(expected.as_slice()),
            "{table:?}"
        );
    }
    // The advanced-key packets went out after the Fn keymap.
    let writes = &write_bytes(&sim)[first..];
    let fn_last = writes
        .iter()
        .rposition(|b| b[..2] == [0x18, 0x02])
        .expect("Fn keymap");
    let first_slot = writes.iter().position(|b| b[0] == 0x22).expect("slot");
    assert!(first_slot > fn_last);
    // The persistence check read them all again.
    let check = report.persistence.as_ref().expect("checked");
    assert_eq!(check.lost, 0);
    assert!(sim.dangling_references(p1, true).is_empty());
    sim.assert_clean();
}

#[test]
fn socd_differing_only_in_the_hotkey_bytes_is_not_written_and_odd_pairs_are_skipped() {
    let mut source = DeviceSim::from_backup(&c06_backup());
    source.set_table(
        profile(1),
        SimTable::SocdSettings,
        vec![0, 0, 0, 0, 0xfa, 0xe6],
    );
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    let mut sim = DeviceSim::from_backup(&c06_backup());
    let prepared = prepare(&mut sim, &target, &options());
    let socd = prepared
        .plan()
        .profiles
        .iter()
        .flat_map(|p| p.items.iter())
        .find(|i| i.table == TableId::Socd)
        .expect("SOCD item");
    assert!(matches!(
        socd.action,
        PlanAction::Skip {
            reason: SkipReason::ReadOnlyBytes,
            ..
        }
    ));

    // Pairs with a gap: not in the site's form.
    let mut source = DeviceSim::from_backup(&c06_backup());
    let mut pairs = vec![0u8; 80];
    pairs[4..8].copy_from_slice(&[0x01, 0x00, 0x04, 0x07]);
    source.set_table(profile(1), SimTable::SocdPairs, pairs);
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    let prepared = prepare(&mut sim, &target, &options());
    let socd = prepared
        .plan()
        .profiles
        .iter()
        .flat_map(|p| p.items.iter())
        .find(|i| i.table == TableId::Socd)
        .expect("SOCD item");
    assert!(matches!(
        socd.action,
        PlanAction::Skip {
            reason: SkipReason::UnwritableSocdPairs { .. },
            ..
        }
    ));
    assert!(!prepared.plan().has_writes());
    sim.assert_clean();
}

#[test]
fn an_exported_profile_has_no_usb_serial_and_loads_again() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    sim.set_usb_info(Some(UsbInfo {
        vendor_id: 0x2e3c,
        product_id: 0xc368,
        product: Some("Wraith W75".into()),
        manufacturer: Some("Wraith".into()),
        serial: Some("SECRET-SERIAL".into()),
    }));
    let all = read_profiles(
        &mut Session::new(&mut sim),
        ProfileSet::ALL,
        consent(),
        &ProfilesOptions {
            backup: common::fixed_options(),
            ..ProfilesOptions::default()
        },
        |_| ControlFlow::Continue(()),
    )
    .expect("all profiles");
    assert!(all.device.usb.is_some());
    let export = all
        .export_profile(profile(2), Some("FPS\u{7} setup"))
        .expect("export");
    assert_eq!(export.scope, BACKUP_SCOPE_EXPORT);
    assert_eq!(export.schema_version, 1);
    assert_eq!(export.active_profile, Some(2));
    assert!(export.device.usb.is_none());
    assert!(export.note.contains("Label: FPS setup"), "{}", export.note);
    assert!(!export.to_json().expect("json").contains("SECRET-SERIAL"));
    let dir = temp_dir();
    let path = dir.path().join("fps.owprofile.json");
    export.save(&path, false).expect("saved");
    let loaded = Backup::load(&path).expect("loaded");
    assert_eq!(loaded, export);
    assert!(
        all.export_profile(ProfileId::new(4).expect("4"), None)
            .is_ok()
    );
    // A profile the backup does not hold.
    let single = read_all(&mut Session::new(&mut sim)).expect("backup");
    assert!(single.export_profile(profile(3), None).is_err());
}

#[test]
fn importing_a_profile_writes_only_the_chosen_categories_into_the_chosen_profile() {
    // The file: profile 1 of c06 with a remapped key and SOCD on.
    let mut source = DeviceSim::from_backup(&c06_backup());
    let mut base = source
        .table(profile(1), SimTable::Keymap(Layer::Base))
        .expect("keymap")
        .to_vec();
    base[46 * 4..47 * 4].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
    source.set_table(profile(1), SimTable::Keymap(Layer::Base), base);
    socd_on(&mut source);
    let file = read_all(&mut Session::new(&mut source))
        .expect("backup")
        .export_profile(profile(1), Some("work"))
        .expect("export");

    // The keyboard: profiles 2-4 at factory triggers and light.
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let p1_before = sim.trigger(profile(1), pos(2, 2));
    let import = |include| RestoreOptions {
        import: Some(ImportOptions {
            from: profile(1),
            into: profile(3),
            include,
            allow_critical_fn_overrides: false,
        }),
        ..options()
    };
    let report = restore_with(&mut sim, &file, &import(ImportSelection::default()));
    assert!(report.is_success(), "{:#?}", report.error);
    // Triggers and the light went to profile 3 only.
    assert_eq!(
        report
            .profiles
            .iter()
            .map(|p| p.profile)
            .collect::<Vec<_>>(),
        [3]
    );
    let written: Vec<TableId> = report
        .items()
        .filter(|(_, i)| matches!(i.result, ItemResult::Written { .. }))
        .map(|(_, i)| i.table)
        .collect();
    assert!(written.contains(&TableId::Triggers));
    assert!(written.contains(&TableId::MainLight));
    assert!(!written.contains(&TableId::Socd));
    assert!(!written.contains(&TableId::Keymap { layer: Layer::Base }));
    assert_eq!(sim.trigger(profile(3), pos(2, 2)), p1_before);
    assert_eq!(sim.trigger(profile(1), pos(2, 2)), p1_before);
    assert_eq!(
        sim.table(profile(3), SimTable::SocdSettings).map(|b| b[0]),
        Some(0)
    );
    assert_eq!(sim.active_profile(), 1);

    // With the key assignments and SOCD.
    let report = restore_with(
        &mut sim,
        &file,
        &import(ImportSelection {
            keymap_group: true,
            socd: true,
            ..ImportSelection::default()
        }),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    let entry = |sim: &DeviceSim, p| {
        sim.persisted_table(profile(p), SimTable::Keymap(Layer::Base))
            .map(|b| b[46 * 4..47 * 4].to_vec())
    };
    assert_eq!(entry(&sim, 3), Some(vec![0x00, 0x04, 0x00, 0x00]));
    assert_eq!(entry(&sim, 1), Some(vec![0, 0, 0, 0]));
    assert_eq!(
        sim.persisted_table(profile(3), SimTable::SocdSettings)
            .map(|b| b[0]),
        Some(1)
    );
    sim.assert_clean();
}

#[test]
fn an_imported_fn_layer_that_replaces_a_profile_switch_needs_its_own_permission() {
    // The file's Fn layer puts a key on Fn + 2 (index 24).
    let mut source = DeviceSim::from_backup(&c06_backup());
    let mut fn_layer = source
        .table(profile(1), SimTable::Keymap(Layer::Fn))
        .expect("keymap")
        .to_vec();
    fn_layer[24 * 4..25 * 4].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
    source.set_table(profile(1), SimTable::Keymap(Layer::Fn), fn_layer);
    let file = read_all(&mut Session::new(&mut source))
        .expect("backup")
        .export_profile(profile(1), None)
        .expect("export");
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let keys = ImportSelection {
        keymap_group: true,
        ..ImportSelection::default()
    };
    let import = |allow| RestoreOptions {
        import: Some(ImportOptions {
            from: profile(1),
            into: profile(3),
            include: keys,
            allow_critical_fn_overrides: allow,
        }),
        ..options()
    };
    let prepared = prepare(&mut sim, &file, &import(false));
    assert!(prepared.plan().skips().any(|(_, item)| matches!(
        &item.action,
        PlanAction::Skip {
            reason: SkipReason::KeymapGroupBlocked {
                cause: BlockCause::CriticalFnOverride { index: 24 }
            },
            ..
        }
    )));
    assert!(
        prepared
            .plan()
            .writes()
            .all(|(_, item, _)| !matches!(item.table, TableId::Keymap { .. }))
    );
    assert!(
        prepared
            .plan()
            .warnings
            .contains(&PlanWarning::Imported { from: 1, into: 3 })
    );
    // Allowed: written, and the plan says which built-in it replaces.
    let report = restore_with(&mut sim, &file, &import(true));
    assert!(report.is_success(), "{:#?}", report.error);
    let fn_entry = sim
        .persisted_table(profile(3), SimTable::Keymap(Layer::Fn))
        .map(|b| b[24 * 4..25 * 4].to_vec());
    assert_eq!(fn_entry, Some(vec![0x00, 0x04, 0x00, 0x00]));
    let prepared = prepare(&mut sim, &file, &import(true));
    assert!(!prepared.plan().has_writes());
    sim.assert_clean();
}

#[test]
fn an_import_checks_its_options() {
    let file = c06_backup();
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let mut only_two = ProfileSet::empty();
    only_two.insert(profile(2));
    let wrong = RestoreOptions {
        profiles: Some(only_two),
        import: Some(ImportOptions {
            from: profile(1),
            into: profile(3),
            include: ImportSelection::default(),
            allow_critical_fn_overrides: false,
        }),
        ..options()
    };
    let refused = prepare_restore(
        &mut Session::new(&mut sim),
        &file,
        &wrong,
        consent(),
        |_| ControlFlow::Continue(()),
    );
    assert!(refused.is_err());
    let missing = RestoreOptions {
        import: Some(ImportOptions {
            from: profile(4),
            into: profile(3),
            include: ImportSelection::default(),
            allow_critical_fn_overrides: false,
        }),
        ..options()
    };
    let refused = prepare_restore(
        &mut Session::new(&mut sim),
        &file,
        &missing,
        consent(),
        |_| ControlFlow::Continue(()),
    );
    assert!(refused.is_err());
    assert!(write_bytes(&sim).is_empty());
}

#[test]
fn files_larger_than_any_backup_are_not_read() {
    let dir = temp_dir();
    let path = dir.path().join("huge.owbackup.json");
    let size = usize::try_from(BACKUP_MAX_FILE_BYTES).expect("fits") + 1;
    std::fs::write(&path, vec![b' '; size]).expect("written");
    assert!(matches!(
        Backup::load(&path),
        Err(BackupError::TooLarge { bytes, max, .. }) if bytes == BACKUP_MAX_FILE_BYTES + 1 && max == BACKUP_MAX_FILE_BYTES
    ));
}

#[test]
fn a_restore_that_stops_after_a_binding_names_the_dangling_key() {
    let mut source = DeviceSim::from_backup(&c06_backup());
    bind_advanced_keys(&mut source);
    let target = read_all(&mut Session::new(&mut source)).expect("backup");
    let mut sim = DeviceSim::from_backup(&c06_backup());
    let prepared = prepare(&mut sim, &target, &options());
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "restore")
        .expect("saved");
    // Drop the connection right after the Fn keymap's last packet: the
    // bindings are in, the slots are not.
    let at = {
        let mut copy = sim.clone();
        let start = copy.written().len();
        execute_restore(
            &mut Session::new(&mut copy),
            &prepared,
            &saved,
            consent(),
            |_| ControlFlow::Continue(()),
        );
        start
            + copy.written()[start..]
                .iter()
                .rposition(|p| p.is_write() && p.as_bytes()[..2] == [0x18, 0x02])
                .expect("Fn keymap")
            + 1
    };
    sim.disconnect_after_packets(at);
    let report = execute_restore(
        &mut Session::new(&mut sim),
        &prepared,
        &saved,
        consent(),
        |_| ControlFlow::Continue(()),
    );
    assert!(!report.is_success());
    let left = sim.dangling_references(profile(1), false);
    assert_eq!(left.len(), 2);
    let said: Vec<_> = report
        .dangling
        .iter()
        .map(|d| (d.layer, d.index, d.slot))
        .collect();
    for d in &left {
        assert!(said.contains(d), "{d:?} not in {said:?}");
    }
    assert!(report.dangling.iter().all(|d| d.profile == 1));
    let json = serde_json::to_value(&report).expect("serializes");
    assert!(json["dangling"].is_array());
}

/// A file exported from profile 1 of a c06 simulator whose keymap layers
/// hold `entries` (`(layer, index, entry)`).
fn file_with_entries(entries: &[(Layer, u8, [u8; 4])]) -> Backup {
    let mut source = DeviceSim::from_backup(&c06_backup());
    for layer in Layer::ALL {
        let mut table = source
            .table(profile(1), SimTable::Keymap(layer))
            .expect("keymap")
            .to_vec();
        for (_, index, entry) in entries.iter().filter(|(l, _, _)| *l == layer) {
            let at = usize::from(*index) * 4;
            table[at..at + 4].copy_from_slice(entry);
        }
        source.set_table(profile(1), SimTable::Keymap(layer), table);
    }
    read_all(&mut Session::new(&mut source))
        .expect("backup")
        .export_profile(profile(1), None)
        .expect("export")
}

/// Import of profile 1 of a file into profile 3 with the key assignments.
fn import_keys(allow_critical: bool) -> RestoreOptions {
    RestoreOptions {
        import: Some(ImportOptions {
            from: profile(1),
            into: profile(3),
            include: ImportSelection {
                keymap_group: true,
                ..ImportSelection::default()
            },
            allow_critical_fn_overrides: allow_critical,
        }),
        ..options()
    }
}

fn entry_of(sim: &DeviceSim, p: u8, layer: Layer, index: u8) -> Option<Vec<u8>> {
    let at = usize::from(index) * 4;
    sim.persisted_table(profile(p), SimTable::Keymap(layer))
        .map(|b| b[at..at + 4].to_vec())
}

#[test]
fn an_import_keeps_the_knob_the_fn_key_and_positions_outside_the_layout() {
    let w = key("W").index();
    // As a file from a keyboard the vendor site configured: the Fn key
    // remapped to A on both layers, a knob slot, a gamepad entry outside the
    // layout, and one ordinary remap (W -> B).
    let file = file_with_entries(&[
        (Layer::Base, 121, [0x00, 0x04, 0x00, 0x00]),
        (Layer::Base, 16, [0x02, 0x01, 0x00, 0x00]),
        (Layer::Base, 130, [0x18, 0x05, 0x00, 0x00]),
        (Layer::Base, w, [0x00, 0x05, 0x00, 0x00]),
        (Layer::Fn, 121, [0x11, 0x05, 0x00, 0x00]),
    ]);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let kept: Vec<(Layer, u8, Option<Vec<u8>>)> = [
        (Layer::Base, 121),
        (Layer::Base, 16),
        (Layer::Base, 130),
        (Layer::Fn, 121),
    ]
    .into_iter()
    .map(|(layer, index)| (layer, index, entry_of(&sim, 3, layer, index)))
    .collect();

    let prepared = prepare(&mut sim, &file, &import_keys(false));
    let warnings = &prepared.plan().warnings;
    assert!(
        warnings.contains(&PlanWarning::ImportKeptAsRead {
            layer: Layer::Base,
            indices: vec![16, 121, 130],
        }),
        "{warnings:?}"
    );
    assert!(warnings.contains(&PlanWarning::ImportKeptAsRead {
        layer: Layer::Fn,
        indices: vec![121],
    }));
    // Only W changes in the base layer the plan writes.
    let base = prepared
        .plan()
        .writes()
        .find(|(_, item, _)| item.table == (TableId::Keymap { layer: Layer::Base }))
        .map(|(_, _, write)| write.changes.clone())
        .expect("base keymap write");
    assert_eq!(
        base.iter().map(|c| c.index).collect::<Vec<_>>(),
        [usize::from(w)]
    );

    let report = restore_with(&mut sim, &file, &import_keys(false));
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(
        entry_of(&sim, 3, Layer::Base, w),
        Some(vec![0x00, 0x05, 0x00, 0x00])
    );
    for (layer, index, entry) in kept {
        assert_eq!(entry_of(&sim, 3, layer, index), entry, "{layer:?} #{index}");
    }
    sim.assert_clean();
}

#[test]
fn imported_entries_openwraith_does_not_write_block_the_keys() {
    let space = key("Space").index();
    for entry in [
        [0x11, 0x00, 0x00, 0x00], // lock keyboard
        [0x18, 0x01, 0x00, 0x00], // gamepad
        [0x02, 0x02, 0x00, 0x00], // lighting
        [0x19, 0x0a, 0x00, 0x00], // gamepad (Xbox)
        [0xde, 0xad, 0xbe, 0xef], // unknown
        [0x10, 0x04, 0x00, 0x00], // a macro entry carrying another key's usage
    ] {
        let file = file_with_entries(&[(Layer::Base, space, entry)]);
        let mut sim = sim_with_profiles(ScopeConfig::default());
        let prepared = prepare(&mut sim, &file, &import_keys(true));
        let blocked = prepared.plan().skips().any(|(_, item)| {
            matches!(
                &item.action,
                PlanAction::Skip {
                    reason: SkipReason::KeymapGroupBlocked {
                        cause: BlockCause::UnwritableImportedEntry {
                            layer: Layer::Base,
                            index,
                            entry: e,
                        }
                    },
                    ..
                } if *index == space && e.0 == entry
            )
        });
        assert!(blocked, "{entry:02x?}: {:#?}", prepared.plan());
        assert!(
            prepared
                .plan()
                .writes()
                .all(|(_, item, _)| !matches!(item.table, TableId::Keymap { .. })),
            "{entry:02x?}"
        );
        let before = write_bytes(&sim).len();
        let report = restore_with(&mut sim, &file, &import_keys(true));
        assert!(
            write_bytes(&sim)[before..]
                .iter()
                .all(|b| b[..2] != [0x18, 0x00] && b[..2] != [0x18, 0x02]),
            "{entry:02x?}"
        );
        assert!(report.is_success(), "{:#?}", report.error);
    }
}

#[test]
fn an_imported_fn_layer_keeps_the_keys_the_keyboard_locks() {
    // c06 locks only the Fn key; this keyboard also locks Q on the Fn layer.
    let q = key("Q");
    let mut keyboard = c06_backup();
    for entry in keyboard
        .entries
        .iter_mut()
        .filter(|e| e.raw.request() == ReadRequest::FnLockedKeys)
    {
        let mut bytes = *entry.raw.first().expect("0E reply").as_bytes();
        bytes[openwraith_hid::protocol::HEADER_LEN + q.mask_byte()] |= q.mask_bit();
        entry.raw = openwraith_hid::protocol::RawResponse::from_reports(
            ReadRequest::FnLockedKeys,
            [openwraith_hid::protocol::Report::new(bytes)],
        )
        .expect("0E reply");
    }
    let mut sim = DeviceSim::from_backup(&keyboard);
    let file = file_with_entries(&[(Layer::Fn, q.index(), [0x00, 0x04, 0x00, 0x00])]);
    let before = entry_of(&sim, 1, Layer::Fn, q.index());
    let into_one = RestoreOptions {
        import: Some(ImportOptions {
            into: profile(1),
            ..import_keys(true).import.expect("import")
        }),
        ..options()
    };
    let prepared = prepare(&mut sim, &file, &into_one);
    assert!(
        prepared
            .plan()
            .warnings
            .contains(&PlanWarning::ImportKeptAsRead {
                layer: Layer::Fn,
                indices: vec![q.index()],
            }),
        "{:?}",
        prepared.plan().warnings
    );
    assert!(!prepared.plan().has_writes(), "{:#?}", prepared.plan());
    let report = restore_with(&mut sim, &file, &into_one);
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(entry_of(&sim, 1, Layer::Fn, q.index()), before);
    sim.assert_clean();
}

#[test]
fn an_imported_macro_must_be_steps_openwraith_can_show() {
    let mut source = DeviceSim::from_backup(&c06_backup());
    // Canonical, but A is still held at the end: not a program.
    let mut slot = vec![0u8; 256];
    slot[..12].copy_from_slice(&[0, 0, 0, 4, 0, 1, 0, 0, 0x04, 0x10, 0, 0]);
    let slot_id = openwraith_hid::protocol::MacroSlotId::new(0).expect("slot");
    source.set_table(profile(1), SimTable::Macro(slot_id), slot);
    let file = read_all(&mut Session::new(&mut source))
        .expect("backup")
        .export_profile(profile(1), None)
        .expect("export");
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let prepared = prepare(&mut sim, &file, &import_keys(false));
    assert!(prepared.plan().skips().any(|(_, item)| matches!(
        &item.action,
        PlanAction::Skip {
            reason: SkipReason::KeymapGroupBlocked {
                cause: BlockCause::NonCanonicalMacro { slot: 0, .. }
            },
            ..
        }
    )));
    assert!(prepared.plan().writes().all(|(_, item, _)| !matches!(
        item.table,
        TableId::Keymap { .. } | TableId::Macro { .. } | TableId::AdvancedKey { .. }
    )));
    // When the keys are not imported, nothing about them is blocked.
    let without_keys = RestoreOptions {
        import: Some(ImportOptions {
            from: profile(1),
            into: profile(3),
            include: ImportSelection::default(),
            allow_critical_fn_overrides: false,
        }),
        ..options()
    };
    let prepared = prepare(&mut sim, &file, &without_keys);
    assert!(prepared.plan().skips().all(|(_, item)| !matches!(
        &item.action,
        PlanAction::Skip {
            reason: SkipReason::KeymapGroupBlocked { .. },
            ..
        }
    )));
}

#[test]
fn imported_colours_keep_the_slots_outside_the_layout() {
    let w = usize::from(key("W").index());
    let mut source = DeviceSim::from_backup(&c06_backup());
    let mut colours = source
        .table(profile(1), SimTable::CustomColors1)
        .expect("colours")
        .to_vec();
    colours[w * 3..w * 3 + 3].copy_from_slice(&[0x12, 0x34, 0x56]);
    for slot in [15usize, 103, 126] {
        colours[slot * 3..slot * 3 + 3].copy_from_slice(&[0xab, 0xcd, 0xef]);
    }
    source.set_table(profile(1), SimTable::CustomColors1, colours);
    let file = read_all(&mut Session::new(&mut source))
        .expect("backup")
        .export_profile(profile(1), None)
        .expect("export");
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let before = sim
        .persisted_table(profile(3), SimTable::CustomColors1)
        .expect("colours")
        .to_vec();
    let lighting = RestoreOptions {
        import: Some(ImportOptions {
            from: profile(1),
            into: profile(3),
            include: ImportSelection::default(),
            allow_critical_fn_overrides: false,
        }),
        ..options()
    };
    let report = restore_with(&mut sim, &file, &lighting);
    assert!(report.is_success(), "{:#?}", report.error);
    let after = sim
        .persisted_table(profile(3), SimTable::CustomColors1)
        .expect("colours")
        .to_vec();
    assert_eq!(&after[w * 3..w * 3 + 3], &[0x12, 0x34, 0x56]);
    for slot in [15usize, 103, 126] {
        assert_eq!(
            after[slot * 3..slot * 3 + 3],
            before[slot * 3..slot * 3 + 3],
            "slot {slot}"
        );
    }
    sim.assert_clean();
}

#[test]
fn a_plain_restore_of_an_exported_profile_is_refused() {
    let space = key("Space").index();
    let file = file_with_entries(&[
        (Layer::Fn, 24, [0x00, 0x04, 0x00, 0x00]),
        (Layer::Base, space, [0x11, 0x00, 0x00, 0x00]),
    ]);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let refused = prepare_restore(
        &mut Session::new(&mut sim),
        &file,
        &options(),
        consent(),
        |_| ControlFlow::Continue(()),
    );
    assert!(
        matches!(
            &refused,
            Err(openwraith_hid::RestoreError::InvalidRequest { reason })
                if reason.contains("profile import")
        ),
        "{refused:?}"
    );
    let dry =
        openwraith_hid::dry_run_restore(&mut Session::new(&mut sim), &file, &options(), |_| {
            ControlFlow::Continue(())
        });
    assert!(dry.is_err());
    assert!(write_bytes(&sim).is_empty());
}
