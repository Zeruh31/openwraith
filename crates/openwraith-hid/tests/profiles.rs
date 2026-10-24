//! All-profile backups (schema 2) against the device simulator, and the
//! schema-1 / schema-2 file format.

mod common;

use std::ops::ControlFlow;
use std::time::Duration;

use common::{
    C06, assert_only_reads, c06_backup, consent, mock, pos, profile, profiles_options, selects,
    sim_with_profiles,
};
use openwraith_hid::protocol::{
    Distance, FromResponse, KeyMask, LightMode, ReadRequest, TriggerGlobal, TriggerLimits,
    TriggerMode, TriggerSetting, TriggerWrite, TriggerWriteMode, W75, WriteRequest,
};
use openwraith_hid::{
    BACKUP_SCHEMA_VERSION, BACKUP_SCHEMA_VERSION_PROFILES, BACKUP_SCOPE_PROFILES, Backup,
    BackupError, CommitSettle, DeviceSim, ErrorKind, ProfileSet, ProfilesFailure, ProfilesOptions,
    ProfilesProgress, ScopeConfig, Session, WriteError, WriteOptions, read_all, read_all_profiles,
    read_profiles,
};

/// W (row 2, col 2).
const W: (u8, u8) = (2, 2);

fn all_profiles(sim: &mut DeviceSim) -> Backup {
    let mut session = Session::new(sim);
    read_all_profiles(&mut session, consent(), &profiles_options(), |_| {
        ControlFlow::Continue(())
    })
    .expect("all-profile backup")
}

#[test]
fn plain_backup_of_the_simulator_sends_only_reads() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let backup = {
        let mut session = Session::new(&mut sim);
        read_all(&mut session).expect("backup")
    };
    assert_eq!(sim.writes_sent(), 0);
    assert!(sim.written().iter().all(|p| !p.is_write()));
    assert_eq!(backup.schema_version, BACKUP_SCHEMA_VERSION);
    assert!(backup.profiles.is_empty());
    // The simulator answers like the keyboard: the same tables as c06.
    let c06 = c06_backup();
    for entry in &c06.entries {
        let request = entry.request();
        let got = backup.response(request).expect("reply");
        assert_eq!(
            got.payload(),
            entry.raw.payload(),
            "{request} differs between the simulator and c06"
        );
    }
    sim.assert_clean();
}

#[test]
fn all_profile_backup_reads_every_profile_and_returns() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let backup = all_profiles(&mut sim);

    assert_eq!(backup.schema_version, BACKUP_SCHEMA_VERSION_PROFILES);
    assert_eq!(backup.scope, BACKUP_SCOPE_PROFILES);
    assert!(backup.note.contains("profile 1"));
    assert_eq!(backup.active_profile, Some(1));
    assert_eq!(backup.profile_numbers(), vec![1, 2, 3, 4]);
    assert_eq!(
        backup
            .profiles
            .iter()
            .map(|s| s.profile)
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    // Profile 1 is c06; the others are factory.
    let w = |n: u8| {
        backup
            .snapshot(profile(n))
            .and_then(|s| s.key_trigger(pos(W.0, W.1).index()).cloned())
            .expect("W")
    };
    assert_eq!(w(1).mode, TriggerMode::RapidTrigger);
    assert_eq!(w(2).mode, TriggerMode::Off);
    assert_eq!(w(4).actuation.to_string(), "1.70 mm");
    let light = |n: u8| {
        backup
            .snapshot(profile(n))
            .and_then(|s| s.main_light().cloned())
    };
    assert_eq!(light(1).map(|l| l.mode), Some(LightMode::Custom));
    assert_eq!(light(3).map(|l| l.mode), Some(LightMode::from_id(0x02)));
    // Every snapshot is a complete read (the same plan).
    let plan_len = backup.entries.len();
    assert!(backup.profiles.iter().all(|s| s.entries.len() == plan_len));

    // Only reads and profile selects were sent; the last select is the
    // original profile, and the keyboard is back on it.
    assert_eq!(selects(&sim), [2, 3, 4, 1]);
    for packet in sim.written() {
        if packet.is_write() {
            assert_eq!(packet.as_bytes()[..6], [0x21, 0, 0, 0, 0, 0x12]);
        } else {
            assert!(ReadRequest::from_packet(packet.as_bytes()).is_some());
        }
    }
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn starting_on_another_profile_returns_to_it() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    sim.set_active_profile(profile(3));
    let backup = all_profiles(&mut sim);
    assert_eq!(backup.active_profile, Some(3));
    assert_eq!(selects(&sim), [1, 2, 4, 3]);
    assert_eq!(sim.active_profile(), 3);
    sim.assert_clean();
}

#[test]
fn only_the_active_profile_is_a_plain_read() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let backup = {
        let mut session = Session::new(&mut sim);
        read_profiles(
            &mut session,
            ProfileSet::only(profile(1)),
            consent(),
            &profiles_options(),
            |_| ControlFlow::Continue(()),
        )
        .expect("backup")
    };
    assert_eq!(backup.schema_version, BACKUP_SCHEMA_VERSION);
    assert_eq!(sim.writes_sent(), 0);
    sim.assert_clean();
}

#[test]
fn schema_2_round_trips_and_schema_1_stays_byte_identical() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let backup = all_profiles(&mut sim);
    let json = backup.to_json().expect("json");
    let loaded = Backup::from_json(&json).expect("loads");
    assert_eq!(loaded, backup);
    let value: serde_json::Value = serde_json::to_value(&backup).expect("serializes");
    assert_eq!(value["profiles"][0]["profile"], 2);
    let via_serde: Backup = serde_json::from_value(value).expect("serde");
    assert_eq!(via_serde, backup);

    // A single-profile backup has no `profiles` field at all.
    let plain = c06_backup();
    let json = plain.to_json().expect("json");
    assert!(!json.contains("\"profiles\""));
    assert_eq!(Backup::from_json(&json).expect("loads"), plain);
}

#[test]
fn schema_2_snapshots_are_validated() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let backup = all_profiles(&mut sim);
    let value = serde_json::to_value(&backup).expect("serializes");

    let invalid = |value: serde_json::Value| {
        let text = serde_json::to_string(&value).expect("text");
        Backup::from_json(&text)
    };
    let is_invalid_snapshot =
        |r: Result<Backup, BackupError>| matches!(r, Err(BackupError::InvalidSnapshot { .. }));

    // The stored profile number is ignored: it is derived from 21/11.
    let mut edited = value.clone();
    edited["profiles"][0]["profile"] = 4.into();
    let loaded = invalid(edited).expect("loads");
    assert_eq!(loaded.profiles[0].profile, 2);

    // Schema 1 with profiles.
    let mut edited = value.clone();
    edited["schema_version"] = 1.into();
    assert!(is_invalid_snapshot(invalid(edited)));

    // The same profile twice.
    let mut edited = value.clone();
    let first = edited["profiles"][0].clone();
    edited["profiles"]
        .as_array_mut()
        .expect("array")
        .push(first);
    assert!(is_invalid_snapshot(invalid(edited)));

    // A snapshot of the primary profile.
    let mut edited = value.clone();
    let primary = edited["entries"].clone();
    edited["profiles"][0]["entries"] = primary;
    assert!(is_invalid_snapshot(invalid(edited)));

    // A snapshot without an active-profile reply.
    let mut edited = value.clone();
    edited["profiles"][1]["entries"]
        .as_array_mut()
        .expect("array")
        .retain(|e| e["raw"]["request"] != "active_profile");
    assert!(is_invalid_snapshot(invalid(edited)));

    // A snapshot without a device-info reply (other keyboard?).
    let mut edited = value.clone();
    edited["profiles"][2]["entries"]
        .as_array_mut()
        .expect("array")
        .retain(|e| e["raw"]["request"] != "device_info");
    assert!(is_invalid_snapshot(invalid(edited)));

    // A duplicate request inside a snapshot.
    let mut edited = value.clone();
    let entries = edited["profiles"][0]["entries"]
        .as_array_mut()
        .expect("array");
    let light = entries
        .iter()
        .find(|e| e["raw"]["request"] == "main_light")
        .cloned()
        .expect("light");
    entries.push(light);
    assert!(matches!(
        invalid(edited),
        Err(BackupError::DuplicateEntry { .. })
    ));

    // Newer schemas are refused.
    let mut edited = value;
    edited["schema_version"] = 3.into();
    assert!(matches!(
        invalid(edited),
        Err(BackupError::UnsupportedSchema { found: 3, .. })
    ));
}

#[test]
fn cancelling_returns_to_the_original_profile() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let err = {
        let mut session = Session::new(&mut sim);
        read_all_profiles(&mut session, consent(), &profiles_options(), |p| match p {
            ProfilesProgress::Selecting { profile: 3 } => ControlFlow::Break(()),
            _ => ControlFlow::Continue(()),
        })
        .unwrap_err()
    };
    assert!(matches!(err.failure, ProfilesFailure::Cancelled), "{err}");
    assert_eq!(err.kind(), ErrorKind::Cancelled);
    assert_eq!(err.active_profile_now, Some(1));
    assert_eq!(selects(&sim), [2, 1]);
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn a_foreign_heartbeat_refuses_the_profile_switches() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    // The vendor site's heartbeat reply arrives during the first read.
    let status = common::captured_status_report(C06);
    sim.inject_after_packets(5, status);
    let err = {
        let mut session = Session::new(&mut sim);
        read_all_profiles(&mut session, consent(), &profiles_options(), |_| {
            ControlFlow::Continue(())
        })
        .unwrap_err()
    };
    assert!(
        matches!(
            err.failure,
            ProfilesFailure::Write(WriteError::OtherProgramActive { .. })
        ),
        "{err}"
    );
    assert_eq!(err.active_profile_now, Some(1));
    assert_eq!(sim.writes_sent(), 0);
    sim.assert_clean();
}

#[test]
fn a_reset_during_another_profile_sends_nothing_more() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    // The first snapshot plus a few packets of the second profile.
    let first = {
        let mut copy = sim.clone();
        let mut session = Session::new(&mut copy);
        read_all(&mut session).expect("backup");
        session.transport().written().len()
    };
    sim.reset_after_packets(first + 20);
    let err = {
        let mut session = Session::new(&mut sim);
        read_all_profiles(&mut session, consent(), &profiles_options(), |_| {
            ControlFlow::Continue(())
        })
        .unwrap_err()
    };
    assert_eq!(err.kind(), ErrorKind::KeyboardReset, "{err}");
    assert!(sim.reset_delivered());
    // Not even a switch back to profile 1.
    assert_eq!(selects(&sim), [2]);
    assert_eq!(err.active_profile_now, Some(2));
    sim.assert_clean();
}

#[test]
fn a_plain_backup_from_a_capture_is_still_read_only() {
    let mut session = Session::new(mock(C06));
    read_all(&mut session).expect("backup");
    assert_only_reads(session.transport());
}

#[test]
fn another_product_in_another_profile_is_refused_and_the_original_selected() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let err = {
        let mut tamper = common::Tamper::new(&mut sim);
        // After the first switch the device-info string names another
        // product (same length).
        tamper.rewrite = Box::new(|bytes, selects| {
            let product = b"W75HEARGB";
            if selects == 1 && bytes[0] == 0x0D {
                if let Some(at) = bytes.windows(product.len()).position(|w| w == product) {
                    bytes[at..at + product.len()].copy_from_slice(b"W75HEARGX");
                }
            }
        });
        let mut session = Session::new(&mut tamper);
        read_all_profiles(&mut session, consent(), &profiles_options(), |_| {
            ControlFlow::Continue(())
        })
        .unwrap_err()
    };
    // With one known model the other product is also an unknown one, so the
    // snapshot read refuses it before the product comparison
    // (`ProductChanged` needs a second known model); either way the kind is
    // a model mismatch and nothing more is read.
    assert!(
        matches!(
            &err.failure,
            ProfilesFailure::ProductChanged { profile: 2, .. }
                | ProfilesFailure::Backup(BackupError::UnsupportedModel { .. })
        ),
        "{err}"
    );
    assert_eq!(err.kind(), ErrorKind::ModelMismatch);
    assert_eq!(err.active_profile_now, Some(1));
    assert_eq!(sim.active_profile(), 1);
    assert_eq!(selects(&sim), [2, 1]);
    sim.assert_clean();
}

#[test]
fn an_active_profile_outside_1_to_4_is_refused_before_any_switch() {
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let err = {
        let mut tamper = common::Tamper::new(&mut sim);
        // The keyboard reports profile 0 (never selected, Q11).
        tamper.rewrite = Box::new(|bytes, _| {
            if bytes[..6] == [0x21, 0, 0, 0, 0x03, 0x11] {
                bytes[6] = 0;
            }
        });
        let mut session = Session::new(&mut tamper);
        read_all_profiles(&mut session, consent(), &profiles_options(), |_| {
            ControlFlow::Continue(())
        })
        .unwrap_err()
    };
    assert!(
        matches!(
            err.failure,
            ProfilesFailure::UnknownActiveProfile { profile: Some(0) }
        ),
        "{err}"
    );
    assert_eq!(err.kind(), ErrorKind::UnexpectedReply);
    assert_eq!(sim.writes_sent(), 0);
    sim.assert_clean();
}

#[test]
fn an_all_profile_backup_without_writes_adds_no_delay() {
    // The same backup with the default commit settle and without any: the
    // same virtual time (only the 2 s gate), no wait reported.
    let mut with_settle = sim_with_profiles(ScopeConfig::default());
    let mut without = with_settle.clone();
    let mut events = Vec::new();
    {
        let mut session = Session::new(&mut with_settle);
        read_all_profiles(&mut session, consent(), &profiles_options(), |p| {
            events.push(p);
            ControlFlow::Continue(())
        })
        .expect("all-profile backup");
    }
    {
        let options = ProfilesOptions {
            write: WriteOptions {
                commit_settle: CommitSettle::disabled_for_tests(),
                ..WriteOptions::default()
            },
            ..profiles_options()
        };
        let mut session = Session::new(&mut without);
        read_all_profiles(&mut session, consent(), &options, |_| {
            ControlFlow::Continue(())
        })
        .expect("all-profile backup");
    }
    assert_eq!(with_settle.elapsed(), without.elapsed());
    assert_eq!(with_settle.elapsed(), Duration::from_secs(2));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ProfilesProgress::WaitingForCommit { .. })),
        "{events:?}"
    );
    assert_eq!(selects(&with_settle), [2, 3, 4, 1]);
    with_settle.assert_clean();
}

#[test]
fn an_all_profile_backup_after_a_write_in_the_same_session_waits_first() {
    // A long-lived session (a GUI) writes Q, then makes an all-profile
    // backup: the first switch waits until the keyboard saved the write.
    let q = pos(2, 1);
    let mut sim = sim_with_profiles(ScopeConfig::default());
    let dir = common::temp_dir();
    let saved = {
        let mut copy = sim.clone();
        let mut session = Session::new(&mut copy);
        read_all(&mut session)
            .expect("backup")
            .save_safety(dir.path(), "test")
            .expect("saved")
    };
    let mut events = Vec::new();
    {
        let mut session = Session::new(&mut sim);
        {
            let mut writer = session.writer(consent()).expect("writer");
            let raw = writer.read(ReadRequest::TriggerGlobal).expect("21/04");
            let limits =
                TriggerLimits::new(&W75, &TriggerGlobal::from_response(&raw).expect("decodes"));
            let setting = TriggerSetting {
                mode: TriggerWriteMode::Off,
                actuation: Distance(100),
                rt_press: Distance(1),
                rt_release: Distance(1),
            };
            let write = TriggerWrite::new(&W75, KeyMask::from_positions([q]), setting, &limits)
                .expect("valid");
            writer
                .write_guarded(&saved, &WriteRequest::Trigger(write))
                .expect("written");
        }
        read_all_profiles(&mut session, consent(), &profiles_options(), |p| {
            events.push(p);
            ControlFlow::Continue(())
        })
        .expect("all-profile backup");
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, ProfilesProgress::WaitingForCommit { .. }))
            .collect::<Vec<_>>(),
        [&ProfilesProgress::WaitingForCommit { wait_ms: 2000 }]
    );
    // The write survived the switches.
    assert_eq!(sim.discarded_edits(), 0);
    assert_eq!(
        sim.persisted_trigger(profile(1), q).map(|r| r[1]),
        Some(100)
    );
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}
