//! Reset detection against known-good backups (`known_good::assess`,
//! `reset_verdict`) and `compose` with the restore path, against the
//! captures and the device simulator. No HID device is opened.

mod common;

use std::ops::ControlFlow;
use std::time::Duration;

use common::{
    C07, c06_backup, consent, fixed_options, fixed_time, load, mock, pos, profile, put_macro,
    sim_with_profiles, tap, temp_dir, write_bytes,
};
use openwraith_hid::known_good::{
    FACTORY_LIGHT_PREFIX, FACTORY_TRIGGER_RECORD, Side, factory_custom1,
};
use openwraith_hid::protocol::capture::CaptureEvent;
use openwraith_hid::protocol::{
    CustomTable, Layer, MacroSlotId, ProfileId, RawResponse, ReadRequest, Report, W75,
    is_keyboard_reset,
};
use openwraith_hid::{
    BACKUP_SCHEMA_VERSION, BACKUP_SCHEMA_VERSION_PROFILES, BACKUP_SCOPE_PROFILES, Backup,
    BackupEntry, BackupOptions, DeviceSim, KnownGoodError, KnownGoodTable, PlanWarning,
    ProfileAssessment, ResetEvidence, RestoreOptions, ScopeConfig, Session, SimTable, TableVerdict,
    Transport, UsbInfo, assess, compose, execute_restore, prepare_restore, read_all_with,
    reset_verdict, sim::FACTORY_LIGHT,
};

const ALL_TABLES: [KnownGoodTable; 9] = KnownGoodTable::ALL;

/// A one-profile backup of `p` (made active without a write), read at
/// `secs` seconds after the fixed test time.
fn read_profile(sim: &mut DeviceSim, p: ProfileId, secs: u64) -> Backup {
    sim.set_active_profile(p);
    let options = BackupOptions {
        created_at: Some(fixed_time() + Duration::from_secs(secs)),
        ..fixed_options()
    };
    read_all_with(&mut Session::new(sim), &options, |_| {}).expect("backup")
}

/// Overwrites bytes of a simulator table.
fn patch(sim: &mut DeviceSim, p: ProfileId, table: SimTable, offset: usize, bytes: &[u8]) {
    let mut data = sim.table(p, table).expect("table").to_vec();
    data[offset..offset + bytes.len()].copy_from_slice(bytes);
    sim.set_table(p, table, data);
}

/// c13 macro slot 0: "A down 100 ms, A up 100 ms, B down 100 ms, B up".
fn c13_macro() -> Vec<u8> {
    let mut bytes = vec![0u8; 256];
    bytes[..24].copy_from_slice(&[
        0x00, 0x00, 0x00, 0x10, 0x00, 0x01, 0x00, 0x00, 0x04, 0x10, 0x00, 0x64, 0x04, 0x00, 0x00,
        0x64, 0x05, 0x10, 0x00, 0x64, 0x05, 0x00, 0x00, 0x00,
    ]);
    bytes
}

fn slot(n: u8) -> MacroSlotId {
    MacroSlotId::new(n).expect("slot")
}

/// Profile `p` gets a keymap remap (W -> A), the c13 macro in slot 0 bound
/// to Caps, a one-key macro in slot 3, a dead band on W and a blue colour
/// slot.
fn customise(sim: &mut DeviceSim, p: ProfileId) {
    let base = SimTable::Keymap(Layer::Base);
    patch(sim, p, base, 46 * 4, &[0x00, 0x04, 0x00, 0x00]);
    patch(sim, p, base, 66 * 4, &[0x10, 0x39, 0x00, 0x00]);
    sim.set_table(p, SimTable::Macro(slot(0)), c13_macro());
    put_macro(sim, p, 3, &tap(0x05));
    patch(sim, p, SimTable::DeadBand, 92, &[0x05, 0x0a]);
    patch(sim, p, SimTable::CustomColors1, 30 * 3, &[0, 0, 0xff]);
}

/// SOCD power (one setting of the whole keyboard).
fn set_power(sim: &mut DeviceSim, on: bool) {
    sim.set_table(
        profile(1),
        SimTable::SocdSettings,
        vec![u8::from(on), 0, 0, 0, 0xfa, 0xe5],
    );
}

/// The A/D pair in profile `p`.
fn set_pair(sim: &mut DeviceSim, p: ProfileId) {
    let mut pairs = vec![0u8; 80];
    pairs[..4].copy_from_slice(&[0x01, 0x00, 0x04, 0x07]);
    sim.set_table(p, SimTable::SocdPairs, pairs);
}

/// A change made to a simulator.
type Change = fn(&mut DeviceSim);

/// Q at 2.00 mm, Rapid Trigger off.
const Q_2_00: [u8; 9] = [0x00, 100, 100, 0x01, 0x01, 0, 0, 0, 0];

fn verdicts(a: &ProfileAssessment) -> Vec<(KnownGoodTable, TableVerdict)> {
    a.tables.iter().map(|t| (t.table, t.verdict)).collect()
}

fn assessed(known: &Backup, current: &Backup, p: u8) -> ProfileAssessment {
    assess(known, current, profile(p)).expect("assessed")
}

/// `backup` with its device-info reply naming firmware `version` instead
/// of 3.22.07 (reloaded, so the header is derived again).
fn with_firmware(backup: &Backup, version: &[u8; 7]) -> Backup {
    let mut backup = backup.clone();
    let raw = backup
        .response(ReadRequest::DeviceInfo)
        .expect("0D")
        .clone();
    let reports: Vec<Report> = raw
        .reports()
        .iter()
        .map(|r| {
            let mut bytes = *r.as_bytes();
            if let Some(at) = bytes.windows(7).position(|w| w == b"3.22.07") {
                bytes[at..at + 7].copy_from_slice(version);
            }
            Report::new(bytes)
        })
        .collect();
    let entry = backup
        .entries
        .iter_mut()
        .find(|e| e.request() == ReadRequest::DeviceInfo)
        .expect("0D entry");
    *entry = BackupEntry::new(
        RawResponse::from_reports(ReadRequest::DeviceInfo, reports).expect("valid reply"),
    );
    Backup::from_json(&backup.to_json().expect("json")).expect("reloads")
}

fn usb(serial: &str) -> UsbInfo {
    UsbInfo {
        vendor_id: 0x2e3c,
        product_id: 0xc368,
        product: Some("Wraith W75".into()),
        manufacturer: Some("Wraith".into()),
        serial: Some(serial.into()),
    }
}

// ----- factory forms against the captures -----

/// c01 is the keyboard before any change; c06 after the user's edits.
#[test]
fn the_factory_forms_match_the_first_connect_and_not_a_changed_keyboard() {
    let c01 = read_all_with(
        &mut Session::new(mock("capture_01_connect")),
        &fixed_options(),
        |_| {},
    )
    .expect("c01 backup");
    for table in ALL_TABLES {
        assert_eq!(
            table.is_factory(c01.primary()),
            Some(true),
            "{table} in c01"
        );
    }
    let c06 = c06_backup();
    for (table, factory) in [
        (KnownGoodTable::Triggers, false),
        (KnownGoodTable::MainLight, false),
        (KnownGoodTable::CustomColors1, false),
        (KnownGoodTable::BaseKeymap, true),
        (KnownGoodTable::DeadBand, true),
    ] {
        assert_eq!(table.is_factory(c06.primary()), Some(factory), "{table}");
    }
    assert_eq!(
        c01.response(ReadRequest::CustomColors(CustomTable::Custom1))
            .and_then(|r| r.table_bytes("test").ok()),
        Some(factory_custom1())
    );
}

/// c07: after the Fn + Space reset the site re-read every trigger and the
/// main light (PROTOCOL.md 6.1 row d).
#[test]
fn the_trigger_and_light_forms_are_what_the_keyboard_read_after_its_reset() {
    let capture = load(C07);
    let reset = capture
        .entries
        .iter()
        .find_map(|e| match &e.event {
            CaptureEvent::In(r) if is_keyboard_reset(r) => Some(e.index),
            _ => None,
        })
        .expect("reset notification");
    let after: Vec<RawResponse> = capture
        .pair_reads()
        .exchanges
        .into_iter()
        .filter(|x| x.out_index > reset)
        .filter_map(|x| x.response().ok())
        .collect();
    let triggers: Vec<&RawResponse> = after
        .iter()
        .filter(|r| matches!(r.request(), ReadRequest::KeyTrigger(_)))
        .collect();
    assert!(triggers.len() >= 80, "{} trigger replies", triggers.len());
    for raw in triggers {
        assert_eq!(
            raw.payload().get(1..10),
            Some(&FACTORY_TRIGGER_RECORD[..]),
            "{:?}",
            raw.request()
        );
    }
    let light = after
        .iter()
        .find(|r| r.request() == ReadRequest::MainLight)
        .expect("light re-read");
    assert_eq!(light.payload().get(..3), Some(&FACTORY_LIGHT_PREFIX[..]));
}

// ----- assess -----

#[test]
fn a_factory_reset_of_the_simulator_is_detected() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim, profile(1));
    sim.set_trigger(profile(3), pos(2, 1), Q_2_00);
    // Profile 4: one keymap-group table only.
    patch(
        &mut sim,
        profile(4),
        SimTable::Keymap(Layer::Fn),
        2 * 4,
        &[0x07, 0x12, 0, 0],
    );
    let known: Vec<Backup> = (1..=4)
        .map(|n| read_profile(&mut sim, profile(n), u64::from(n)))
        .collect();

    sim.factory_reset_all();
    let current: Vec<Backup> = (1..=4)
        .map(|n| read_profile(&mut sim, profile(n), 100 + u64::from(n)))
        .collect();
    let a: Vec<ProfileAssessment> = (1..=4u8)
        .map(|n| {
            let i = usize::from(n - 1);
            assessed(&known[i], &current[i], n)
        })
        .collect();

    use KnownGoodTable as T;
    use TableVerdict as V;
    assert_eq!(
        verdicts(&a[0]),
        [
            (T::Triggers, V::ChangedToFactory),
            (T::MainLight, V::ChangedToFactory),
            // The simulator zeroes the table; the factory table has 8 red
            // slots (c01), so this is another change.
            (T::CustomColors1, V::ChangedOther),
            (T::BaseKeymap, V::ChangedToFactory),
            (T::FnKeymap, V::Unchanged),
            (T::Macros, V::ChangedToFactory),
            (T::AdvancedKeys, V::Unchanged),
            (T::DeadBand, V::ChangedToFactory),
            (T::SocdPairs, V::Unchanged),
        ]
    );
    assert_eq!(
        a[0].evidence,
        [
            ResetEvidence::TriggersToFactory,
            ResetEvidence::KeymapGroupToFactory {
                tables: vec![T::BaseKeymap, T::Macros]
            }
        ]
    );
    assert!(a[0].reset_like());
    // Profile 2 had factory triggers and light: only the colours changed.
    assert!(!a[1].reset_like(), "{:?}", a[1]);
    assert_eq!(a[1].tables_with(V::ChangedOther), [T::CustomColors1]);
    assert_eq!(a[1].tables_with(V::ChangedToFactory), []);
    // Profile 3: its triggers.
    assert_eq!(a[2].evidence, [ResetEvidence::TriggersToFactory]);
    // Profile 4: one keymap-group table is not enough.
    assert_eq!(a[3].verdict(T::FnKeymap), V::ChangedToFactory);
    assert!(!a[3].reset_like());

    let verdict = reset_verdict(&a, false);
    assert!(verdict.reset_like);
    assert!(!verdict.reset_seen);
    assert_eq!(
        verdict
            .profiles
            .iter()
            .map(|p| p.profile)
            .collect::<Vec<_>>(),
        [1, 3]
    );
    assert_eq!(verdict.socd_power, V::Unchanged);
    let json = serde_json::to_value(&verdict).expect("serialises");
    assert_eq!(
        json["profiles"][0]["evidence"][0]["evidence"],
        "triggers_to_factory"
    );
    sim.assert_clean();
}

#[test]
fn untouched_profiles_and_a_new_baseline_raise_no_alarm() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim, profile(1));
    let known = read_profile(&mut sim, profile(1), 1);
    let again = read_profile(&mut sim, profile(1), 50);
    let a = assessed(&known, &again, 1);
    assert!(a.is_unchanged(), "{a:?}");
    assert!(!a.reset_like());
    assert!(!reset_verdict(&[a], false).reset_like);

    // "It's fine, keep them": the factory state becomes the known-good
    // baseline and compares unchanged from then on.
    sim.factory_reset_all();
    let current = read_profile(&mut sim, profile(1), 60);
    assert!(assessed(&known, &current, 1).reset_like());
    let later = read_profile(&mut sim, profile(1), 70);
    let a = assessed(&current, &later, 1);
    assert!(a.is_unchanged());
    assert!(!a.reset_like());
    sim.assert_clean();
}

#[test]
fn the_light_the_dead_band_or_socd_alone_never_alarm() {
    let p1 = profile(1);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim, p1);
    set_pair(&mut sim, p1);
    set_power(&mut sim, true);
    let known = read_profile(&mut sim, p1, 1);

    // Each one alone, then all of them together.
    let changes: [(&str, Change); 5] = [
        ("light", |s| {
            s.set_table(profile(1), SimTable::MainLight, FACTORY_LIGHT.to_vec());
        }),
        ("colours", |s| {
            s.set_table(profile(1), SimTable::CustomColors1, factory_custom1());
        }),
        ("dead band", |s| {
            s.set_table(profile(1), SimTable::DeadBand, vec![0; 264]);
        }),
        ("SOCD pairs", |s| {
            s.set_table(profile(1), SimTable::SocdPairs, vec![0; 80]);
        }),
        ("SOCD power", |s| set_power(s, false)),
    ];
    let mut together = sim.clone();
    for (name, change) in changes {
        let mut changed = sim.clone();
        change(&mut changed);
        change(&mut together);
        let a = assessed(&known, &read_profile(&mut changed, p1, 2), 1);
        assert!(!a.reset_like(), "{name}: {a:?}");
        assert!(!a.is_unchanged(), "{name}");
    }
    let a = assessed(&known, &read_profile(&mut together, p1, 3), 1);
    use KnownGoodTable as T;
    assert_eq!(
        a.tables_with(TableVerdict::ChangedToFactory),
        [T::MainLight, T::CustomColors1, T::DeadBand, T::SocdPairs]
    );
    assert_eq!(a.socd_power, TableVerdict::ChangedToFactory);
    assert!(a.evidence.is_empty(), "{:?}", a.evidence);
    assert!(!reset_verdict(&[a], false).reset_like);
}

#[test]
fn the_socd_power_is_never_blamed_on_a_profile() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    set_pair(&mut sim, profile(2));
    set_power(&mut sim, true);
    let known2 = read_profile(&mut sim, profile(2), 10);
    let known1 = read_profile(&mut sim, profile(1), 5);
    // Fn + Right Shift on the keyboard: off in every profile.
    sim.press_socd_hotkey_at(Duration::from_millis(10));
    let push = Transport::read(&mut sim, Duration::from_millis(50)).expect("read");
    assert_eq!(push.map(|r| r.cmd()), Some(0x24));
    let current2 = read_profile(&mut sim, profile(2), 20);
    let current1 = read_profile(&mut sim, profile(1), 21);
    let a2 = assessed(&known2, &current2, 2);
    let a1 = assessed(&known1, &current1, 1);
    for a in [&a1, &a2] {
        assert_eq!(
            a.verdict(KnownGoodTable::SocdPairs),
            TableVerdict::Unchanged
        );
        assert_eq!(a.socd_power, TableVerdict::ChangedToFactory);
        assert!(a.evidence.is_empty());
        assert!(
            a.tables
                .iter()
                .all(|t| t.verdict == TableVerdict::Unchanged)
        );
    }
    // Reported once, from the newest known-good backup (profile 2's).
    let mut a2_older = a2.clone();
    a2_older.socd_power = TableVerdict::ChangedOther;
    let verdict = reset_verdict(&[a1.clone(), a2_older], false);
    assert_eq!(verdict.socd_power, TableVerdict::ChangedOther);
    assert!(!verdict.reset_like);
    assert_eq!(reset_verdict(&[], false).socd_power, TableVerdict::Missing);
}

#[test]
fn a_reset_notification_or_a_firmware_change_is_evidence_on_its_own() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let known = read_profile(&mut sim, profile(1), 1);
    let current = read_profile(&mut sim, profile(1), 2);
    let a = assessed(&known, &current, 1);
    assert!(a.is_unchanged());
    // R1.
    let verdict = reset_verdict(std::slice::from_ref(&a), true);
    assert!(verdict.reset_like);
    assert!(verdict.reset_seen);
    assert!(verdict.profiles.is_empty());

    // R2: same settings, other firmware.
    let updated = with_firmware(&current, b"3.22.08");
    assert_eq!(updated.device.firmware.as_deref(), Some("3.22.08"));
    let a = assessed(&known, &updated, 1);
    assert!(
        a.tables
            .iter()
            .all(|t| t.verdict == TableVerdict::Unchanged)
    );
    match a.evidence.as_slice() {
        [ResetEvidence::FirmwareChanged { known, current }] => {
            assert_eq!(known.version.as_deref(), Some("3.22.07"));
            assert_eq!(current.version.as_deref(), Some("3.22.08"));
            assert_eq!(current.to_string(), "3.22.08 (Aug  6 2026,13:46:54)");
        }
        other => panic!("{other:?}"),
    }
    assert!(reset_verdict(&[a], false).reset_like);
}

#[test]
fn assess_compares_one_profile_of_one_keyboard() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    sim.set_usb_info(Some(usb("A-1")));
    let known1 = read_profile(&mut sim, profile(1), 1);
    let current2 = read_profile(&mut sim, profile(2), 2);
    assert!(matches!(
        assess(&known1, &current2, profile(1)),
        Err(KnownGoodError::ProfileNotInBackup {
            side: Side::Current,
            profile: 1
        })
    ));
    assert!(matches!(
        assess(&known1, &current2, profile(2)),
        Err(KnownGoodError::ProfileNotInBackup {
            side: Side::Known,
            ..
        })
    ));
    // Another keyboard of the same model (another USB serial number).
    sim.set_usb_info(Some(usb("B-2")));
    let other = read_profile(&mut sim, profile(1), 3);
    let err = assess(&known1, &other, profile(1)).expect_err("other keyboard");
    assert!(matches!(err, KnownGoodError::OtherKeyboard { .. }), "{err}");
    assert!(err.to_string().contains("serial B-2"), "{err}");
    // An all-profile read is compared profile by profile.
    sim.set_usb_info(Some(usb("A-1")));
    sim.set_active_profile(profile(1));
    let all = openwraith_hid::read_all_profiles(
        &mut Session::new(&mut sim),
        consent(),
        &common::profiles_options(),
        |_| ControlFlow::Continue(()),
    )
    .expect("all profiles");
    assert!(assessed(&known1, &all, 1).is_unchanged());
    let known2 = read_profile(&mut sim, profile(2), 4);
    assert!(assessed(&known2, &all, 2).is_unchanged());
    sim.assert_clean();
}

/// `KnownGoodTable::canonical` tells whether a table still holds what it
/// held: equal forms exactly where `assess` finds the table unchanged.
#[test]
fn canonical_forms_follow_the_comparison_on_the_simulator() {
    let p1 = profile(1);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    customise(&mut sim, p1);
    let known = read_profile(&mut sim, p1, 1);
    // Unchanged, read again later: every form is the same.
    let again = read_profile(&mut sim, p1, 50);
    for table in ALL_TABLES {
        let form = table.canonical(known.primary());
        assert!(form.is_some(), "{table}");
        assert_eq!(form, table.canonical(again.primary()), "{table}");
    }
    // One key's trigger and the factory reset: the forms follow the verdicts.
    sim.set_trigger(p1, pos(2, 1), Q_2_00);
    let edited = read_profile(&mut sim, p1, 60);
    sim.factory_reset_all();
    let reset = read_profile(&mut sim, p1, 70);
    for current in [&edited, &reset] {
        let a = assessed(&known, current, 1);
        for table in ALL_TABLES {
            assert_eq!(
                table.canonical(known.primary()) == table.canonical(current.primary()),
                a.verdict(table) == TableVerdict::Unchanged,
                "{table}"
            );
        }
    }
    // An empty macro slot equals an empty one whatever its filler.
    let ff = vec![0xFFu8; 256];
    let zero = vec![0u8; 256];
    sim.set_table(p1, SimTable::Macro(slot(5)), ff);
    let with_ff = read_profile(&mut sim, p1, 80);
    sim.set_table(p1, SimTable::Macro(slot(5)), zero);
    let with_zero = read_profile(&mut sim, p1, 90);
    assert_eq!(
        KnownGoodTable::Macros.canonical(with_ff.primary()),
        KnownGoodTable::Macros.canonical(with_zero.primary())
    );
    // A snapshot without the table has no form.
    let mut partial = known.clone();
    partial
        .entries
        .retain(|e| e.request() != ReadRequest::DeadBand);
    assert_eq!(KnownGoodTable::DeadBand.canonical(partial.primary()), None);
    sim.assert_clean();
}

// ----- compose -----

/// Per-profile known-good backups of a customised keyboard, read at
/// different times: profile 4 first with SOCD off, then SOCD on and
/// profiles 1, 2 and (newest) 3.
fn known_goods(sim: &mut DeviceSim) -> Vec<Backup> {
    customise(sim, profile(1));
    set_pair(sim, profile(1));
    sim.set_trigger(profile(2), pos(2, 1), Q_2_00);
    patch(sim, profile(3), SimTable::DeadBand, 92, &[0x05, 0x0a]);
    set_power(sim, false);
    let p4 = read_profile(sim, profile(4), 5);
    set_power(sim, true);
    let p1 = read_profile(sim, profile(1), 10);
    let p2 = read_profile(sim, profile(2), 20);
    let p3 = read_profile(sim, profile(3), 30);
    vec![p2, p4, p1, p3]
}

fn restore_options() -> RestoreOptions {
    RestoreOptions {
        backup: fixed_options(),
        ..RestoreOptions::default()
    }
}

#[test]
fn composed_known_goods_restore_every_profile_after_a_factory_reset() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let parts = known_goods(&mut sim);
    let composed = compose(&parts).expect("composed");
    assert_eq!(composed.schema_version, BACKUP_SCHEMA_VERSION_PROFILES);
    assert_eq!(composed.scope, BACKUP_SCOPE_PROFILES);
    // The newest part is the primary snapshot; the others ascend.
    assert_eq!(composed.active_profile, Some(3));
    assert_eq!(composed.created_at, parts[3].created_at);
    assert_eq!(
        composed
            .profiles
            .iter()
            .map(|s| s.profile)
            .collect::<Vec<_>>(),
        [1, 2, 4]
    );
    assert_eq!(composed.profile_numbers(), [1, 2, 3, 4]);
    assert!(composed.note.contains("profile 3"), "{}", composed.note);
    // Every snapshot holds its part's replies, unchanged.
    for part in &parts {
        let p = profile(part.active_profile.expect("profile"));
        assert_eq!(
            composed.snapshot(p).expect("snapshot").entries(),
            part.entries.as_slice()
        );
    }
    assert_eq!(Backup::compose(&parts).expect("same"), composed);

    sim.factory_reset_all();
    sim.set_active_profile(profile(1));
    let first_write = write_bytes(&sim).len();
    let prepared = prepare_restore(
        &mut Session::new(&mut sim),
        &composed,
        &restore_options(),
        consent(),
        |_| ControlFlow::Continue(()),
    )
    .expect("prepared");
    // Profile 4's part was read with SOCD off: the primary's power wins.
    assert!(
        prepared
            .plan()
            .warnings
            .contains(&PlanWarning::SocdPowerDiffersInBackup {
                profile: 3,
                power: true
            })
    );
    let dir = temp_dir();
    let saved = prepared
        .safety()
        .save_safety(dir.path(), "recovery")
        .expect("saved");
    let report = execute_restore(
        &mut Session::new(&mut sim),
        &prepared,
        &saved,
        consent(),
        |_| ControlFlow::Continue(()),
    );
    assert!(report.is_success(), "{:#?}", report.error);
    assert_eq!(sim.active_profile(), 1);
    // The power is written once.
    let power_writes = write_bytes(&sim)[first_write..]
        .iter()
        .filter(|b| b[..2] == [0x24, 0x03])
        .count();
    assert_eq!(power_writes, 1);

    // Every profile compares unchanged with its known-good part again (the
    // power of profile 4's part was off; the keyboard's is on).
    for part in &parts {
        let n = part.active_profile.expect("profile");
        let now = read_profile(&mut sim, profile(n), 200);
        let a = assessed(part, &now, n);
        assert!(
            a.tables
                .iter()
                .all(|t| t.verdict == TableVerdict::Unchanged),
            "profile {n}: {a:?}"
        );
        let power = if n == 4 {
            TableVerdict::ChangedOther
        } else {
            TableVerdict::Unchanged
        };
        assert_eq!(a.socd_power, power, "profile {n}");
    }
    sim.assert_clean();
}

#[test]
fn one_part_composes_to_a_single_profile_backup() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let part = read_profile(&mut sim, profile(2), 1);
    let composed = compose(std::slice::from_ref(&part)).expect("composed");
    assert_eq!(composed.schema_version, BACKUP_SCHEMA_VERSION);
    assert_eq!(composed.active_profile, Some(2));
    assert_eq!(composed.entries, part.entries);
    assert!(composed.profiles.is_empty());
}

#[test]
fn compose_refuses_parts_that_do_not_fit_together() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    sim.set_usb_info(Some(usb("A-1")));
    let p1 = read_profile(&mut sim, profile(1), 1);
    let p2 = read_profile(&mut sim, profile(2), 2);
    let kind = |parts: &[Backup]| compose(parts).expect_err("refused");

    assert!(matches!(kind(&[]), KnownGoodError::NoParts));
    assert!(matches!(
        kind(&[p1.clone(), p2.clone(), p1.clone()]),
        KnownGoodError::DuplicateProfile { profile: 1 }
    ));
    // Another firmware.
    let p2_new = with_firmware(&p2, b"3.22.08");
    let err = kind(&[p1.clone(), p2_new]);
    assert!(matches!(err, KnownGoodError::MixedFirmware { .. }), "{err}");
    assert_eq!(err.kind(), openwraith_hid::ErrorKind::FirmwareMismatch);
    // Another keyboard (another serial number).
    sim.set_usb_info(Some(usb("B-2")));
    let other = read_profile(&mut sim, profile(3), 3);
    assert!(matches!(
        kind(&[p1.clone(), other]),
        KnownGoodError::OtherKeyboard { .. }
    ));
    // An exported profile is only ever imported.
    let export = p2.export_profile(profile(2), None).expect("export");
    assert!(matches!(
        kind(&[p1.clone(), export]),
        KnownGoodError::ExportedProfile { index: 1 }
    ));
    // An all-profile backup is not a part.
    let both = compose(&[p1.clone(), p2.clone()]).expect("composed");
    assert!(matches!(
        kind(&[both]),
        KnownGoodError::NotSingleProfile { index: 0 }
    ));
    // A part of an unknown keyboard.
    let mut unknown = p1.clone();
    unknown
        .entries
        .retain(|e| e.request() != ReadRequest::DeviceInfo);
    let unknown = Backup::from_json(&unknown.to_json().expect("json")).expect("loads");
    assert!(matches!(
        kind(&[unknown, p2.clone()]),
        KnownGoodError::UnsupportedKeyboard { product: None }
    ));
    // A part without its profile.
    let mut nameless = p2.clone();
    nameless
        .entries
        .retain(|e| e.request() != ReadRequest::ActiveProfile);
    let nameless = Backup::from_json(&nameless.to_json().expect("json")).expect("loads");
    assert!(matches!(
        kind(&[p1, nameless]),
        KnownGoodError::NoProfile { index: 1 }
    ));
}

#[test]
fn the_trigger_form_skips_only_the_knob() {
    // Every key but the knob at the factory record: factory, whatever the
    // knob reads.
    let mut sim = DeviceSim::from_backup(&c06_backup());
    sim.factory_reset_all();
    let knob = W75.knob_index.expect("knob");
    let knob_pos = W75
        .positions()
        .find(|p| p.index() == knob)
        .expect("knob position");
    sim.set_trigger(profile(1), knob_pos, Q_2_00);
    let backup = read_profile(&mut sim, profile(1), 1);
    assert_eq!(
        KnownGoodTable::Triggers.is_factory(backup.primary()),
        Some(true)
    );
    // Any other key off the record: not factory.
    sim.set_trigger(profile(1), pos(2, 1), Q_2_00);
    let backup = read_profile(&mut sim, profile(1), 2);
    assert_eq!(
        KnownGoodTable::Triggers.is_factory(backup.primary()),
        Some(false)
    );
    // A missing reply: unknown.
    let mut partial = backup.clone();
    partial
        .entries
        .retain(|e| e.request() != ReadRequest::KeyTrigger(pos(2, 1)));
    assert_eq!(KnownGoodTable::Triggers.is_factory(partial.primary()), None);
    let a = assessed(&backup, &partial, 1);
    assert_eq!(a.verdict(KnownGoodTable::Triggers), TableVerdict::Missing);
    assert!(!a.reset_like());
}
