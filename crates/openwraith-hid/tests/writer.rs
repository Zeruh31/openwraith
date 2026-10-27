//! The write gate and the `Writer` against the device simulator, with
//! failure injection. No HID device is opened.
//!
//! Table writes go through `Writer::write_guarded`, the only public way to
//! send one: it needs a safety backup of the active profile, confirms the
//! active profile and re-reads what it would replace.

mod common;

use std::time::Duration;

use common::{
    C06, captured_reset_report, captured_status_report, consent, fixed_options, pos, profile,
    report,
};
use openwraith_hid::protocol::FromResponse;
use openwraith_hid::protocol::{
    AdvancedSlotId, ColorTableData, CustomTable, DeadBandData, DeadBandWrite, Distance, KeyMask,
    KeymapData, Layer, LightLimits, LightMode, LightModeList, LightRecord, MacroData, MacroSlotId,
    MainLightWrite, ReadRequest, Rgb, TableId, TriggerGlobal, TriggerLimits, TriggerSetting,
    TriggerWrite, TriggerWriteMode, W75, WriteRequest,
};
use openwraith_hid::{
    CommitSettle, CommitTrigger, CommitWait, DeviceSim, ErrorKind, GuardedWrite, SavedBackup,
    ScopeConfig, Session, SimTable, WriteError, WriteOptions, WriteOutcome, WriteWarning, Writer,
    read_all_with,
};

fn sim() -> DeviceSim {
    DeviceSim::from_backup_with(&common::c06_backup(), ScopeConfig::default())
}

/// A verified safety backup of profile 1 of the (unchanged) c06 keyboard,
/// read from a separate simulator so that the packet counts of the tested
/// one are not affected.
fn safety(dir: &tempfile::TempDir) -> SavedBackup {
    let mut copy = sim();
    let mut session = Session::new(&mut copy);
    let backup = read_all_with(&mut session, &fixed_options(), |_| {}).expect("backup");
    backup.save_safety(dir.path(), "test").expect("saved")
}

/// The outcome of a write that was sent.
fn written(result: Result<GuardedWrite, WriteError>) -> WriteOutcome {
    match result {
        Ok(GuardedWrite::Written(outcome)) => outcome,
        other => panic!("not written: {other:?}"),
    }
}

/// Custom colour table 1 as the keyboard holds it, with the slot of W set
/// to `color`.
fn colours_with<T: openwraith_hid::WriteTransport>(
    writer: &mut Writer<'_, T>,
    color: Rgb,
) -> WriteRequest {
    colours_at(writer, pos(2, 2), color)
}

/// Custom colour table 1 as the keyboard holds it, with one slot changed.
fn colours_at<T: openwraith_hid::WriteTransport>(
    writer: &mut Writer<'_, T>,
    at: openwraith_hid::protocol::KeyPos,
    color: Rgb,
) -> WriteRequest {
    let raw = writer
        .read(ReadRequest::CustomColors(CustomTable::Custom1))
        .expect("colours");
    let mut data = ColorTableData::from_response(&raw).expect("table");
    data.set_color(at, color);
    WriteRequest::CustomColors1(data)
}

/// A trigger write of `key` at `units` actuation, RT off.
fn trigger_at<T: openwraith_hid::WriteTransport>(
    writer: &mut Writer<'_, T>,
    key: openwraith_hid::protocol::KeyPos,
    units: u16,
) -> WriteRequest {
    let raw = writer.read(ReadRequest::TriggerGlobal).expect("21/04");
    let limits = TriggerLimits::new(&W75, &TriggerGlobal::from_response(&raw).expect("decodes"));
    let setting = TriggerSetting {
        mode: TriggerWriteMode::Off,
        actuation: Distance(units),
        rt_press: Distance(1),
        rt_release: Distance(1),
    };
    let write =
        TriggerWrite::new(&W75, KeyMask::from_positions([key]), setting, &limits).expect("valid");
    WriteRequest::Trigger(write)
}

/// A trigger write of Q (row 2, col 1) at `units` actuation, RT off.
fn q_trigger<T: openwraith_hid::WriteTransport>(
    writer: &mut Writer<'_, T>,
    units: u16,
) -> WriteRequest {
    trigger_at(writer, pos(2, 1), units)
}

/// The W75 light limits from the keyboard's mode list.
fn light_limits<T: openwraith_hid::WriteTransport>(writer: &mut Writer<'_, T>) -> LightLimits {
    let modes =
        LightModeList::from_response(&writer.read(ReadRequest::MainLightModes).expect("0A"))
            .expect("decodes");
    LightLimits::new(&W75, &modes)
}

/// A main-light write to `target`, built from the keyboard's state.
fn light_to<T: openwraith_hid::WriteTransport>(
    writer: &mut Writer<'_, T>,
    edit: impl FnOnce(LightRecord) -> LightRecord,
) -> WriteRequest {
    let limits = light_limits(writer);
    let current = LightRecord::from_response(&writer.read(ReadRequest::MainLight).expect("07"))
        .expect("record");
    let write = MainLightWrite::new(&current, edit(current), &limits).expect("valid");
    WriteRequest::MainLight(write)
}

/// c13 macro slot content ("A down, A up, B down, B up").
fn c13_macro(slot: MacroSlotId) -> WriteRequest {
    let mut bytes = vec![0u8; 256];
    bytes[..24].copy_from_slice(&[
        0x00, 0x00, 0x00, 0x10, 0x00, 0x01, 0x00, 0x00, 0x04, 0x10, 0x00, 0x64, 0x04, 0x00, 0x00,
        0x64, 0x05, 0x10, 0x00, 0x64, 0x05, 0x00, 0x00, 0x00,
    ]);
    let mut reports = Vec::new();
    for chunk in 0..5u8 {
        let start = usize::from(chunk) * 58;
        let len = if chunk == 4 { 24 } else { 58 };
        let mut prefix = vec![
            0x19,
            0x80 | slot.get(),
            0,
            chunk,
            u8::try_from(len).expect("len"),
        ];
        prefix.extend_from_slice(&bytes[start..start + len]);
        reports.push(report(&prefix));
    }
    let raw =
        openwraith_hid::protocol::RawResponse::from_reports(ReadRequest::Macro(slot), reports)
            .expect("reply");
    let (slot, data) = MacroData::from_response(&raw).expect("canonical");
    WriteRequest::Macro(slot, data)
}

#[test]
fn gate_refuses_when_another_program_is_active() {
    let foreign = [
        // The vendor site's heartbeat reply.
        captured_status_report(C06),
        // The ack of the site's empty-mask trigger write.
        report(&[0x21, 0, 0, 0, 0x01, 0x00]),
        // The site's 21/11 query reply.
        report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x01]),
        // A keymap-write ack.
        report(&[0x18, 0, 0, 0, 0]),
    ];
    for input in foreign {
        let mut sim = sim();
        sim.push_input(input.clone());
        let mut session = Session::new(&mut sim);
        let err = session.writer(consent()).err().expect("refused");
        assert!(
            matches!(err, WriteError::OtherProgramActive { .. }),
            "{input:?}: {err}"
        );
        assert_eq!(err.kind(), ErrorKind::OtherProgramActive);
        drop(session);
        assert_eq!(sim.writes_sent(), 0);
        sim.assert_clean();
    }
}

#[test]
fn gate_passes_on_a_quiet_keyboard_after_two_seconds() {
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let writer = session.writer(consent()).expect("quiet");
    assert!(!writer.is_poisoned());
    assert!(writer.foreign_activity_seen().is_none());
    drop(writer);
    assert!(session.foreign_activity().is_quiet());
    // The gate observed the keyboard for the whole period (virtual time).
    assert!(session.observed_for() >= Duration::from_secs(2));
    drop(session);
    assert!(sim.elapsed() >= Duration::from_secs(2));
}

#[test]
fn a_heartbeat_during_the_observation_period_refuses_the_writer() {
    let mut sim = sim();
    // The vendor site's heartbeat reply 1.5 s after the session opened.
    sim.push_input_at(Duration::from_millis(1500), captured_status_report(C06));
    let mut session = Session::new(&mut sim);
    let err = session.writer(consent()).err().expect("refused");
    assert!(
        matches!(err, WriteError::OtherProgramActive { .. }),
        "{err}"
    );
    drop(session);
    assert!(sim.elapsed() >= Duration::from_millis(1500));
    assert_eq!(sim.writes_sent(), 0);

    // The same heartbeat 2.5 s later is not seen by the gate.
    let mut sim = self::sim();
    sim.push_input_at(Duration::from_millis(2500), captured_status_report(C06));
    let mut session = Session::new(&mut sim);
    assert!(session.writer(consent()).is_ok());
}

#[test]
fn a_switch_to_another_profile_is_a_profile_change_until_the_gate_is_rearmed() {
    let mut sim = sim();
    // 0.5 s after we read profile 1, the keyboard pushes profile 3 (a
    // switch on the keyboard, or by another program).
    sim.push_input_at(
        Duration::from_millis(500),
        report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x03, 0x00]),
    );
    let mut session = Session::new(&mut sim);
    let now: openwraith_hid::protocol::ActiveProfile =
        session.read(ReadRequest::ActiveProfile).expect("21/11");
    assert_eq!(now.profile, 1);
    let err = session.writer(consent()).err().expect("refused");
    assert!(
        matches!(
            err,
            WriteError::ProfileChanged {
                expected: Some(1),
                reported: 3
            }
        ),
        "{err}"
    );
    // It sticks for the session...
    assert!(session.writer(consent()).is_err());
    // ...until the gate is started afresh (a GUI, after the user confirmed
    // the change): another full observation period, then a writer.
    session.rearm_gate();
    assert!(session.activity_since_gate().is_quiet());
    assert!(!session.foreign_activity().is_quiet());
    let writer = session.writer(consent()).expect("allowed after re-arming");
    drop(writer);
    assert!(session.observed_since_gate() >= Duration::from_secs(2));
    drop(session);
    assert_eq!(sim.writes_sent(), 0);
}

#[test]
fn another_programs_profile_query_sticks_until_the_gate_is_rearmed() {
    let mut sim = sim();
    // The vendor site's 21/11 query reply (byte 7 = 01) for the same
    // profile.
    sim.push_input_at(
        Duration::from_millis(500),
        report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x01]),
    );
    let mut session = Session::new(&mut sim);
    session
        .request(ReadRequest::ActiveProfile)
        .expect("profile 1");
    for _ in 0..2 {
        let err = session.writer(consent()).err().expect("refused");
        assert!(
            matches!(err, WriteError::OtherProgramActive { .. }),
            "{err}"
        );
    }
    session.rearm_gate();
    assert!(session.writer(consent()).is_ok());
}

#[test]
fn no_writer_after_a_reset_notification() {
    let mut sim = sim();
    sim.push_input(captured_reset_report());
    let mut session = Session::new(&mut sim);
    assert!(session.request(ReadRequest::Status).is_err());
    assert!(session.keyboard_reset_seen());
    let err = session.writer(consent()).err().expect("refused");
    assert!(matches!(err, WriteError::KeyboardReset { .. }), "{err}");
    drop(session);
    assert_eq!(sim.writes_sent(), 0);
    sim.assert_clean();
}

#[test]
fn select_profile_switches_and_confirms() {
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let outcome = writer.select_profile(profile(2)).expect("selects");
    assert!(outcome.switched);
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    assert_eq!(writer.known_profile(), Some(profile(2)));
    // Already active: nothing is sent.
    let again = writer.select_profile(profile(2)).expect("no-op");
    assert!(!again.switched);
    drop(writer);
    assert_eq!(session.stats().writes, 1);
    assert!(session.foreign_activity().is_quiet());
    drop(session);
    assert_eq!(sim.active_profile(), 2);
    assert_eq!(common::selects(&sim), [2]);
    sim.assert_clean();
}

#[test]
fn a_missing_push_is_only_a_warning() {
    let mut sim = sim();
    sim.omit_next_pushes(1);
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let outcome = writer.select_profile(profile(3)).expect("selects");
    assert!(
        outcome
            .warnings
            .contains(&WriteWarning::ProfilePushMissing { profile: 3 })
    );
    drop(writer);
    drop(session);
    assert_eq!(sim.active_profile(), 3);
    sim.assert_clean();
}

#[test]
fn a_select_that_does_not_switch_is_retried_once_then_fails() {
    let mut sim = sim();
    sim.ignore_next_selects(2);
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let err = writer.select_profile(profile(4)).unwrap_err();
    assert!(
        matches!(err, WriteError::ProfileNotSelected { active: 1, .. }),
        "{err}"
    );
    drop(writer);
    drop(session);
    assert_eq!(common::selects(&sim), [4, 4]);
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn a_table_is_written_stop_and_wait_and_verified() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = colours_with(&mut writer, Rgb::new(0, 0, 0xff));
    let mut packets = Vec::new();
    let outcome = written(
        writer.write_guarded_observed(&saved, &request, &mut |p| packets.push((p.packet, p.of))),
    );
    assert_eq!(outcome.sends, 1);
    assert_eq!(outcome.packets, 8);
    assert_eq!(outcome.profile, Some(1));
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    assert_eq!(packets.first(), Some(&(1, 8)));
    assert_eq!(packets.last(), Some(&(8, 8)));
    drop(writer);
    assert_eq!(session.stats().acks, 8);
    assert!(session.foreign_activity().is_quiet());
    drop(session);
    let table = sim
        .table(profile(1), SimTable::CustomColors1)
        .expect("table");
    assert_eq!(&table[46 * 3..46 * 3 + 3], &[0, 0, 0xff]);
    assert_eq!(sim.writes_sent(), 8);
    sim.assert_clean();
}

#[test]
fn a_table_the_keyboard_already_holds_is_not_sent() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let current = writer
        .read(ReadRequest::CustomColors(CustomTable::Custom1))
        .expect("colours");
    let request =
        WriteRequest::CustomColors1(ColorTableData::from_response(&current).expect("table"));
    assert_eq!(
        writer.write_guarded(&saved, &request).expect("guarded"),
        GuardedWrite::AlreadyMatched { profile: 1 }
    );
    drop(writer);
    drop(session);
    assert_eq!(sim.writes_sent(), 0);
    sim.assert_clean();
}

#[test]
fn a_table_write_needs_a_safety_backup_of_the_active_profile() {
    let dir = common::temp_dir();
    // A backup of profile 1 only; the keyboard is on profile 2.
    let saved = safety(&dir);
    let mut sim = sim();
    sim.set_active_profile(profile(2));
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = colours_with(&mut writer, Rgb::new(1, 1, 1));
    let err = writer.write_guarded(&saved, &request).unwrap_err();
    assert!(
        matches!(err, WriteError::NotBackedUp { profile: 2, .. }),
        "{err}"
    );
    assert_eq!(err.kind(), ErrorKind::InvalidBackup);
    drop(writer);
    drop(session);
    assert_eq!(sim.writes_sent(), 0);
    sim.assert_clean();
}

#[test]
fn a_stale_main_light_write_is_rebuilt_from_the_keyboard() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    // Built while the keyboard held Custom (0a): brightness only, no leave.
    let request = light_to(&mut writer, |r| r.with_brightness(3));
    if let WriteRequest::MainLight(write) = &request {
        assert!(!write.changes_mode());
    }
    drop(writer);
    drop(session);
    // Meanwhile the mode changed on the keyboard (Fn shortcut, another
    // program): Wave (02).
    let mut light = sim
        .table(profile(1), SimTable::MainLight)
        .expect("light")
        .to_vec();
    light[0] = 0x02;
    sim.set_table(profile(1), SimTable::MainLight, light);
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let outcome = written(writer.write_guarded(&saved, &request));
    // Rebuilt: the mode changes now (02 -> 0a), so the leave is sent first.
    assert_eq!(outcome.packets, 2);
    drop(writer);
    drop(session);
    let cmds: Vec<u8> = common::write_bytes(&sim).iter().map(|b| b[0]).collect();
    assert_eq!(cmds, [0x17, 0x07]);
    sim.assert_clean();

    // The reverse: a mode-changing write whose target mode the keyboard
    // already has sends no leave.
    let mut sim = self::sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let limits = light_limits(&mut writer);
    let current = LightRecord::from_response(&writer.read(ReadRequest::MainLight).expect("07"))
        .expect("record");
    let stale = current.with_mode(LightMode::from_id(0x02));
    let target = current.with_brightness(2);
    let write = MainLightWrite::new(&stale, target, &limits).expect("valid");
    assert!(write.changes_mode());
    let outcome = written(writer.write_guarded(&saved, &WriteRequest::MainLight(write)));
    assert_eq!(outcome.packets, 1);
    drop(writer);
    drop(session);
    let cmds: Vec<u8> = common::write_bytes(&sim).iter().map(|b| b[0]).collect();
    assert_eq!(cmds, [0x07]);
    sim.assert_clean();
}

#[test]
fn a_lost_ack_stops_the_table_and_the_whole_table_is_sent_again() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    // The third colour chunk is applied but not acknowledged; the changed
    // slot (index 125, offset 375) is in chunk 6, which is then not sent.
    sim.drop_ack(3);
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = colours_at(&mut writer, pos(5, 15), Rgb::new(0x12, 0x34, 0x56));
    let outcome = written(writer.write_guarded(&saved, &request));
    assert_eq!(outcome.sends, 2);
    assert!(outcome.warnings.contains(&WriteWarning::AckTimeout {
        packet: 3,
        of: 8,
        send: 1
    }));
    assert!(outcome.warnings.contains(&WriteWarning::Retried));
    // The lost ack might still have come: its window ended before the
    // table was sent again.
    assert!(outcome.warnings.contains(&WriteWarning::WaitedForLateAck));
    // Three packets, then the whole table again; never a single chunk.
    assert_eq!(outcome.packets, 3 + 8);
    drop(writer);
    drop(session);
    let chunks: Vec<u8> = common::write_bytes(&sim).iter().map(|b| b[3]).collect();
    assert_eq!(chunks, [0, 1, 2, 0, 1, 2, 3, 4, 5, 6, 7]);
    let table = sim
        .table(profile(1), SimTable::CustomColors1)
        .expect("table");
    assert_eq!(&table[125 * 3..125 * 3 + 3], &[0x12, 0x34, 0x56]);
    sim.assert_clean();
}

#[test]
fn a_late_ack_is_ours_not_another_programs() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    // The ack of the (only) trigger packet arrives after the next packet.
    sim.delay_ack(1);
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = q_trigger(&mut writer, 0x4b);
    let outcome = written(writer.write_guarded(&saved, &request));
    // The write took effect; the read-back confirms it without a retry.
    assert_eq!(outcome.sends, 1);
    assert!(matches!(
        outcome.warnings.as_slice(),
        [WriteWarning::AckTimeout {
            packet: 1,
            of: 1,
            send: 1
        }]
    ));
    assert!(writer.foreign_activity_seen().is_none());
    // Another write is still allowed.
    let again = q_trigger(&mut writer, 0x4c);
    written(writer.write_guarded(&saved, &again));
    drop(writer);
    assert_eq!(session.stats().foreign_acks, 0);
    assert_eq!(session.stats().expected_reports_consumed, 1);
    drop(session);
    assert_eq!(
        sim.trigger(profile(1), pos(2, 1)),
        Some([0x00, 0x4c, 0x4c, 0x01, 0x01, 0, 0, 0, 0])
    );
    sim.assert_clean();
}

#[test]
fn a_late_ack_of_the_previous_table_is_not_taken_for_the_next_ones_first_ack() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let (slot3, slot4) = (
        MacroSlotId::new(3).expect("slot"),
        MacroSlotId::new(4).expect("slot"),
    );
    // Slot 3's last chunk (write 5) is acknowledged 700 ms late; slot 4's
    // first chunk (write 6) 400 ms late. Without waiting for the late ack,
    // it would arrive while slot 4's first chunk waits and be taken for
    // its ack (both are `19 00 00 00 00`).
    sim.delay_ack_by(5, Duration::from_millis(700));
    sim.delay_ack_by(6, Duration::from_millis(400));
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let first = written(writer.write_guarded(&saved, &c13_macro(slot3)));
    assert_eq!(
        first.warnings,
        [WriteWarning::AckTimeout {
            packet: 5,
            of: 5,
            send: 1
        }]
    );
    let second = written(writer.write_guarded(&saved, &c13_macro(slot4)));
    assert_eq!(second.warnings, [WriteWarning::WaitedForLateAck]);
    assert_eq!(second.sends, 1);
    drop(writer);
    // Four acks of slot 3 plus five of slot 4; the late one was absorbed.
    assert_eq!(session.stats().acks, 9);
    assert_eq!(session.stats().expected_reports_consumed, 1);
    assert_eq!(session.stats().foreign_acks, 0);
    drop(session);
    sim.assert_clean();
}

#[test]
fn an_own_ack_after_its_window_counts_as_another_programs() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    for (delay, foreign) in [(1900u64, 0u64), (2600, 1)] {
        let mut sim = sim();
        // The trigger write's ack is 1.9 s / 2.6 s late (the window is 2 s
        // after the 500 ms ack timeout).
        sim.delay_ack_by(1, Duration::from_millis(500 + delay));
        let mut session = Session::new(&mut sim);
        let mut writer = session.writer(consent()).expect("writer");
        let request = q_trigger(&mut writer, 0x4b);
        written(writer.write_guarded(&saved, &request));
        drop(writer);
        session.listen(Duration::from_secs(4)).expect("listen");
        assert_eq!(session.stats().foreign_acks, foreign, "delay {delay} ms");
        assert_eq!(
            session.stats().expected_reports_consumed,
            1 - foreign,
            "delay {delay} ms"
        );
        drop(session);
        sim.assert_clean();
    }
}

#[test]
fn a_status_push_outside_its_window_counts_as_another_programs() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    // The leave's ack and the status push come 200 ms / 300 ms late: the
    // optional ack is not awaited that long (150 ms), and the push is
    // expected only within 250 ms after the leave was sent.
    for (delay, foreign) in [(200u64, 0u64), (300, 1)] {
        let mut sim = sim();
        sim.delay_ack_by(1, Duration::from_millis(delay));
        let mut session = Session::new(&mut sim);
        let mut writer = session.writer(consent()).expect("writer");
        let request = light_to(&mut writer, |r| {
            r.with_mode(LightMode::from_id(0x02))
                .with_direction(LightRecord::default_direction(LightMode::from_id(0x02)))
        });
        let outcome = written(writer.write_guarded(&saved, &request));
        assert!(
            outcome
                .warnings
                .contains(&WriteWarning::OptionalAckMissing { packet: 1 })
        );
        drop(writer);
        session.listen(Duration::from_secs(1)).expect("listen");
        assert_eq!(session.stats().status_reports, foreign, "delay {delay} ms");
        // The late leave ack is ours in both cases.
        assert_eq!(session.stats().foreign_acks, 0);
        assert_eq!(
            session.stats().expected_reports_consumed,
            2 - foreign,
            "delay {delay} ms"
        );
        drop(session);
        sim.assert_clean();
    }
}

#[test]
fn a_write_that_never_takes_effect_fails_after_two_sends() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    sim.freeze(SimTable::CustomColors1);
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = colours_with(&mut writer, Rgb::new(1, 2, 3));
    let err = writer.write_guarded(&saved, &request).unwrap_err();
    match &err {
        WriteError::VerifyFailed {
            sends, mismatch, ..
        } => {
            assert_eq!(*sends, 2);
            // Slot 46 (W): offsets 138-140.
            assert_eq!(mismatch.differing_offsets(), vec![138, 139, 140]);
        }
        other => panic!("unexpected {other}"),
    }
    assert_eq!(err.kind(), ErrorKind::VerifyFailed);
    // The table was sent twice; a third write of it is refused unsent.
    let before = writer.session().stats().writes;
    let err = writer.write_guarded(&saved, &request).unwrap_err();
    assert!(matches!(err, WriteError::RepeatedWrite { .. }), "{err}");
    assert_eq!(writer.session().stats().writes, before);
    drop(writer);
    drop(session);
    assert_eq!(sim.writes_sent(), 16);
    sim.assert_clean();
}

#[test]
fn a_table_is_written_at_most_twice_per_writer() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let first = q_trigger(&mut writer, 0x40);
    let second = q_trigger(&mut writer, 0x41);
    written(writer.write_guarded(&saved, &first));
    written(writer.write_guarded(&saved, &second));
    let err = writer.write_guarded(&saved, &first).unwrap_err();
    assert!(
        matches!(
            err,
            WriteError::RepeatedWrite {
                profile: Some(1),
                ..
            }
        ),
        "{err}"
    );
    // Another key is another table entry.
    let other = trigger_at(&mut writer, pos(2, 3), 0x32);
    written(writer.write_guarded(&saved, &other));
    drop(writer);
    drop(session);
    assert_eq!(sim.writes_sent(), 3);
    sim.assert_clean();
}

#[test]
fn a_reset_mid_table_stops_everything() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    // Packets: 09 80 read, then the guard (0D, 21/11, 09 80 re-read), then
    // colour chunks; reset after the 6th packet (the second chunk).
    sim.reset_after_packets(6);
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = colours_with(&mut writer, Rgb::new(9, 9, 9));
    let err = writer.write_guarded(&saved, &request).unwrap_err();
    assert!(matches!(err, WriteError::KeyboardReset { .. }), "{err}");
    assert!(err.is_fatal());
    assert!(writer.is_poisoned());
    // Every later call fails without sending anything.
    let err = writer.select_profile(profile(2)).unwrap_err();
    assert!(matches!(err, WriteError::KeyboardReset { .. }), "{err}");
    assert!(writer.read(ReadRequest::Status).is_err());
    assert!(writer.write_guarded(&saved, &request).is_err());
    drop(writer);
    drop(session);
    assert!(sim.reset_delivered());
    assert_eq!(sim.writes_sent(), 2);
    assert_eq!(sim.written().len(), 6);
    sim.assert_clean();
}

#[test]
fn a_disconnect_poisons_the_writer() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    // 09 80, 0D, 21/11, 09 80, then chunks: lost after the third chunk.
    sim.disconnect_after_packets(7);
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = colours_with(&mut writer, Rgb::new(7, 7, 7));
    let err = writer.write_guarded(&saved, &request).unwrap_err();
    assert!(matches!(err, WriteError::Disconnected { .. }), "{err}");
    assert_eq!(err.kind(), ErrorKind::Disconnected);
    assert!(writer.is_poisoned());
    assert!(matches!(
        writer.select_profile(profile(1)).unwrap_err(),
        WriteError::Disconnected { .. }
    ));
    drop(writer);
    assert!(session.connection_lost());
    drop(session);
    assert_eq!(sim.written().len(), 7);
    sim.assert_clean();
}

#[test]
fn a_mode_change_sends_the_leave_and_its_status_push_is_expected() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = light_to(&mut writer, |r| {
        r.with_mode(LightMode::from_id(0x02))
            .with_direction(LightRecord::default_direction(LightMode::from_id(0x02)))
    });
    let outcome = written(writer.write_guarded(&saved, &request));
    assert_eq!(outcome.packets, 2);
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    assert!(writer.foreign_activity_seen().is_none());
    // Brightness only: no leave packet.
    let request = light_to(&mut writer, |r| r.with_brightness(2));
    let outcome = written(writer.write_guarded(&saved, &request));
    assert_eq!(outcome.packets, 1);
    drop(writer);
    assert!(session.foreign_activity().is_quiet());
    assert_eq!(session.stats().expected_reports_consumed, 1);
    drop(session);
    let cmds: Vec<u8> = common::write_bytes(&sim).iter().map(|b| b[0]).collect();
    assert_eq!(cmds, [0x17, 0x07, 0x07]);
    let light = sim.table(profile(1), SimTable::MainLight).expect("light");
    assert_eq!(light[0], 0x02);
    assert_eq!(light[1], 2);
    sim.assert_clean();
}

#[test]
fn a_foreign_ack_stops_after_the_current_table() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    // During the colour write (packet 7 = third chunk, after the 09 80 read
    // and the guard's three reads), another program's trigger-write ack
    // arrives.
    sim.inject_after_packets(7, report(&[0x21, 0, 0, 0, 0x01, 0x00]));
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = colours_with(&mut writer, Rgb::new(5, 5, 5));
    // The current table is finished and verified.
    written(writer.write_guarded(&saved, &request));
    assert!(writer.foreign_activity_seen().is_some());
    // The next one is refused, unsent.
    let next = q_trigger(&mut writer, 0x30);
    let err = writer.write_guarded(&saved, &next).unwrap_err();
    assert!(
        matches!(err, WriteError::OtherProgramActive { .. }),
        "{err}"
    );
    // Returning to a profile is still possible.
    writer.select_profile(profile(1)).expect("select allowed");
    drop(writer);
    assert_eq!(session.stats().foreign_acks, 1);
    drop(session);
    assert_eq!(sim.writes_sent(), 8);
    sim.assert_clean();
}

#[test]
fn a_profile_change_on_the_keyboard_aborts() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    // The user presses Fn + 3 during the colour write (packet 6 = second
    // chunk).
    sim.switch_profile_after_packets(6, profile(3));
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = colours_with(&mut writer, Rgb::new(4, 4, 4));
    let err = writer.write_guarded(&saved, &request).unwrap_err();
    // Right after a write to profile 1: the keyboard reloaded profile 3 and
    // dropped what was sent so far, and the error says so.
    assert!(
        matches!(
            err,
            WriteError::ProfileChangedBeforeSave {
                profile: Some(1),
                reported: 3
            }
        ),
        "{err}"
    );
    assert!(err.to_string().contains("may not have been saved"), "{err}");
    assert_eq!(err.kind(), ErrorKind::ProfileChanged);
    assert!(writer.commit_interrupted());
    assert_eq!(writer.known_profile(), Some(profile(3)));
    // Nothing is left to wait for.
    assert_eq!(writer.commit_pending(), Duration::ZERO);
    drop(writer);
    drop(session);
    assert_eq!(sim.discarded_edits(), 1);
    sim.assert_clean();
}

// ----- commit settle (PROTOCOL.md 4.5.3, "Delayed commit") -----

/// Whether a packet is a profile select (`21 .. 00 12 PP`).
fn is_select(bytes: &[u8; 63]) -> bool {
    bytes[..6] == [0x21, 0, 0, 0, 0, 0x12]
}

/// Indices (into `DeviceSim::written`) of the trigger writes and of the
/// selects.
fn trigger_and_select_indices(sim: &DeviceSim) -> (Vec<usize>, Vec<usize>) {
    let mut triggers = Vec::new();
    let mut selects = Vec::new();
    for (i, packet) in sim.written().iter().enumerate() {
        let bytes = packet.as_bytes();
        if !packet.is_write() {
            continue;
        }
        if is_select(bytes) {
            selects.push(i);
        } else if bytes[..5] == [0x21, 0, 0, 0, 0x18] {
            triggers.push(i);
        }
    }
    (triggers, selects)
}

#[test]
fn a_select_after_a_write_waits_for_the_keyboard_to_save() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    assert_eq!(writer.commit_pending(), Duration::ZERO);
    assert_eq!(writer.options().commit_settle, CommitSettle::DEFAULT);
    let request = q_trigger(&mut writer, 100);
    written(writer.write_guarded(&saved, &request));
    // The read-back matched, but the keyboard has not saved the write yet.
    assert_eq!(writer.commit_pending(), Duration::from_secs(2));
    assert!(writer.session().transport().has_unsaved_edits());

    let mut waits: Vec<CommitWait> = Vec::new();
    let outcome = writer
        .select_profile_observed(profile(2), &mut |w| waits.push(w))
        .expect("selects");
    assert!(outcome.switched);
    assert_eq!(
        waits,
        [CommitWait {
            profile: Some(1),
            wait_ms: 2000,
            settle_ms: 2000
        }]
    );
    assert_eq!(
        outcome.warnings,
        [WriteWarning::WaitedForCommit { waited_ms: 2000 }]
    );
    assert_eq!(writer.commit_pending(), Duration::ZERO);
    // Nothing was written since: switching back does not wait again.
    let back = writer
        .select_profile_observed(profile(1), &mut |w| waits.push(w))
        .expect("back");
    assert!(back.switched);
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    assert_eq!(waits.len(), 1);
    drop(writer);
    assert!(session.foreign_activity().is_quiet());
    drop(session);

    // On the virtual clock the select went out the settle after the write,
    // so the keyboard kept it.
    let (triggers, selects) = trigger_and_select_indices(&sim);
    let times = sim.sent_times();
    assert!(times[selects[0]] - times[triggers[0]] >= Duration::from_secs(2));
    assert_eq!(sim.discarded_edits(), 0);
    assert_eq!(
        sim.persisted_trigger(profile(1), pos(2, 1)).map(|r| r[1]),
        Some(100)
    );
    sim.assert_clean();
}

#[test]
fn a_select_without_a_write_does_not_wait() {
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let before = writer.session().transport().elapsed();
    let mut waits = 0;
    let outcome = writer
        .select_profile_observed(profile(3), &mut |_| waits += 1)
        .expect("selects");
    assert!(outcome.switched);
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    assert_eq!(waits, 0);
    // The select and its confirmation took no virtual time at all.
    assert_eq!(writer.session().transport().elapsed(), before);
    assert_eq!(writer.wait_for_commit().expect("nothing pending"), None);
    drop(writer);
    assert_eq!(session.last_write_at(), None);
    drop(session);
    sim.assert_clean();
}

#[test]
fn the_settle_spans_the_writers_of_one_session() {
    // A long-lived session (a GUI): one writer writes, a later one selects.
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    {
        let mut writer = session.writer(consent()).expect("writer");
        let request = q_trigger(&mut writer, 100);
        written(writer.write_guarded(&saved, &request));
    }
    assert!(session.last_write_at().is_some());
    let mut writer = session.writer(consent()).expect("second writer");
    let outcome = writer.select_profile(profile(2)).expect("selects");
    assert_eq!(
        outcome.warnings,
        [WriteWarning::WaitedForCommit { waited_ms: 2000 }]
    );
    drop(writer);
    drop(session);
    assert_eq!(sim.discarded_edits(), 0);
    assert_eq!(
        sim.persisted_trigger(profile(1), pos(2, 1)).map(|r| r[1]),
        Some(100)
    );
    sim.assert_clean();
}

#[test]
fn without_the_settle_the_simulator_loses_the_write_like_the_keyboard() {
    // The behaviour before the fix, possible only with the test option:
    // select right after the write, as the restore of hardware test H7 did.
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let options = WriteOptions {
        commit_settle: CommitSettle::disabled_for_tests(),
        ..WriteOptions::default()
    };
    let mut writer = session.writer_with(consent(), options).expect("writer");
    let request = q_trigger(&mut writer, 100);
    written(writer.write_guarded(&saved, &request));
    let outcome = writer.select_profile(profile(2)).expect("selects");
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    writer.select_profile(profile(1)).expect("back");
    // Read after the switch: the old value (c06 Q: 1.70 mm = 0x55 units).
    let raw = writer.read(ReadRequest::KeyTrigger(pos(2, 1))).expect("Q");
    assert_eq!(raw.payload()[2], 0x55);
    drop(writer);
    drop(session);
    assert_eq!(sim.discarded_edits(), 1);
    sim.assert_clean();
}

#[test]
fn a_reset_during_the_settle_wait_poisons_the_writer() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    // The reset notification arrives 3 s in: after the 2 s gate and the
    // write, while the select waits for the keyboard to save.
    sim.push_input_at(Duration::from_secs(3), captured_reset_report());
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = q_trigger(&mut writer, 100);
    written(writer.write_guarded(&saved, &request));
    let err = writer.select_profile(profile(2)).unwrap_err();
    assert!(matches!(err, WriteError::KeyboardReset { .. }), "{err}");
    assert!(writer.is_poisoned());
    // Nothing more is sent, and nothing waits any more.
    assert!(writer.select_profile(profile(1)).is_err());
    assert!(writer.wait_for_commit().is_err());
    drop(writer);
    assert!(session.keyboard_reset_seen());
    drop(session);
    assert!(sim.reset_delivered());
    assert!(common::selects(&sim).is_empty());
    // assert_clean: no packet after the notification.
    sim.assert_clean();
}

/// What a writer did in [`switch_during_wait`].
struct WaitRun {
    /// The result of the wait (or of the select).
    result: Result<(), WriteError>,
    interrupted: bool,
    pending: Duration,
    known: Option<openwraith_hid::protocol::ProfileId>,
}

/// Writes Q, then waits for the keyboard to save, either directly (a GUI
/// before it says "done") or through a select.
fn switch_during_wait(sim: &mut DeviceSim, saved: &SavedBackup, select: bool) -> WaitRun {
    let mut session = Session::new(sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = q_trigger(&mut writer, 100);
    written(writer.write_guarded(saved, &request));
    let result = if select {
        writer.select_profile(profile(4)).map(|_| ())
    } else {
        writer.wait_for_commit().map(|_| ())
    };
    WaitRun {
        result,
        interrupted: writer.commit_interrupted(),
        pending: writer.commit_pending(),
        known: writer.known_profile(),
    }
}

#[test]
fn a_silent_switch_on_the_keyboard_during_the_wait_is_found_when_it_ends() {
    // The keyboard need not report a switch made on it (spec Q2): the user
    // presses Fn + 2 300 ms after the write and nothing arrives. The wait
    // ends by reading the active profile, finds profile 2 and says that the
    // write may not have been saved; a select does not go out.
    let dir = common::temp_dir();
    let saved = safety(&dir);
    for select in [false, true] {
        let mut copy = sim();
        let normal = switch_during_wait(&mut copy, &saved, select);
        assert!(normal.result.is_ok(), "{:?}", normal.result);
        assert!(!normal.interrupted);
        let (_, write_at) = common::last_trigger_write(&copy);

        let mut sim = sim();
        let before = sim.persisted_trigger(profile(1), pos(2, 1));
        sim.switch_profile_at(write_at + Duration::from_millis(300), profile(2), false);
        let run = switch_during_wait(&mut sim, &saved, select);
        let err = run.result.expect_err("the switch is found");
        assert!(
            matches!(
                err,
                WriteError::ProfileChangedBeforeSave {
                    profile: Some(1),
                    reported: 2
                }
            ),
            "{err}"
        );
        assert_eq!(err.kind(), ErrorKind::ProfileChanged);
        assert!(run.interrupted);
        assert_eq!(run.pending, Duration::ZERO);
        assert_eq!(run.known, Some(profile(2)));
        // Found when the wait was over, by a 21/11 read; no select was sent.
        assert_eq!(sim.elapsed() - write_at, Duration::from_secs(2));
        assert_eq!(
            sim.written().last().map(|p| p.as_bytes()[..6].to_vec()),
            Some(vec![0x21, 0, 0, 0, 0x00, 0x11])
        );
        assert!(common::selects(&sim).is_empty());
        // The keyboard dropped the write (without the switch it kept it).
        assert_eq!(sim.discarded_edits(), 1);
        assert_eq!(sim.persisted_trigger(profile(1), pos(2, 1)), before);
        assert_eq!(
            copy.persisted_trigger(profile(1), pos(2, 1)).map(|r| r[1]),
            Some(100)
        );
        sim.assert_clean();
    }
}

#[test]
fn another_programs_query_during_the_wait_does_not_cut_it_short() {
    // The vendor site's 21/11 query reply for the same profile, 300 ms after
    // the write: another program, not a switch. The wait still lasts the
    // whole settle (the keyboard has to save), then fails.
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut copy = sim();
    assert!(switch_during_wait(&mut copy, &saved, false).result.is_ok());
    let (_, write_at) = common::last_trigger_write(&copy);

    let mut sim = sim();
    sim.push_input_at(
        write_at + Duration::from_millis(300),
        report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x01]),
    );
    let run = switch_during_wait(&mut sim, &saved, false);
    let err = run.result.expect_err("another program");
    assert!(
        matches!(err, WriteError::OtherProgramActive { .. }),
        "{err}"
    );
    assert!(!run.interrupted);
    assert_eq!(run.known, Some(profile(1)));
    assert_eq!(sim.elapsed() - write_at, Duration::from_secs(2));
    assert_eq!(sim.discarded_edits(), 0);
    assert!(!sim.has_unsaved_edits());
    sim.assert_clean();
}

/// A keymap write of `layer` with the keyboard's table, `edit`ed.
fn keymap_with<T: openwraith_hid::WriteTransport>(
    writer: &mut Writer<'_, T>,
    layer: Layer,
    edit: impl FnOnce(&mut KeymapData),
) -> WriteRequest {
    let raw = writer.read(ReadRequest::Keymap(layer)).expect("keymap");
    let (layer, mut data) = KeymapData::from_response(&raw).expect("table");
    edit(&mut data);
    WriteRequest::Keymap(layer, data)
}

/// W -> A (`00 04 00 00` at index 46).
fn w_to_a(data: &mut KeymapData) {
    data.set_entry(pos(2, 2), [0x00, 0x04, 0x00, 0x00]);
}

#[test]
fn only_an_fn_keymap_write_can_be_a_commit_trigger() {
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let dead_band = {
        let raw = writer.read(ReadRequest::DeadBand).expect("26 00");
        let data = DeadBandData::from_response(&raw).expect("dead band");
        WriteRequest::DeadBand(DeadBandWrite::new(&W75, data, 50).expect("in range"))
    };
    // One request of every other kind: each is refused and given back.
    let others = vec![
        WriteRequest::SelectProfile(profile(2)),
        light_to(&mut writer, |r| r.with_brightness(1)),
        colours_with(&mut writer, Rgb::new(1, 2, 3)),
        keymap_with(&mut writer, Layer::Base, |_| {}),
        keymap_with(&mut writer, Layer::Base, w_to_a),
        c13_macro(MacroSlotId::new(3).expect("slot")),
        WriteRequest::AdvancedKeyClear(AdvancedSlotId::new(0).expect("slot")),
        q_trigger(&mut writer, 100),
        dead_band,
    ];
    for request in others {
        assert_eq!(
            CommitTrigger::try_from(request.clone()),
            Err(request.clone()),
            "{request}"
        );
    }
    // The Fn keymap, changed or not, is one.
    for edit in [|_: &mut KeymapData| {}, w_to_a] {
        let request = keymap_with(&mut writer, Layer::Fn, edit);
        let trigger = CommitTrigger::try_from(request.clone()).expect("Fn keymap");
        assert_eq!(trigger.request(), &request);
    }
    drop(writer);
    drop(session);
    // Building them sent nothing.
    assert_eq!(sim.writes_sent(), 0);
    sim.assert_clean();
}

#[test]
fn a_commit_trigger_is_sent_although_the_keyboard_holds_it_with_every_guard() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    // A GUI-style keymap edit: the base keymap, then the unchanged Fn
    // keymap, then the wait for the keyboard to save.
    let base = keymap_with(&mut writer, Layer::Base, w_to_a);
    let unchanged_fn = keymap_with(&mut writer, Layer::Fn, |_| {});
    written(writer.write_guarded(&saved, &base));
    // write_guarded skips the matching Fn keymap...
    assert_eq!(
        writer
            .write_guarded(&saved, &unchanged_fn)
            .expect("guarded"),
        GuardedWrite::AlreadyMatched { profile: 1 }
    );
    // ...the commit trigger sends it, stop-and-wait, and verifies it.
    let trigger = CommitTrigger::try_from(unchanged_fn).expect("Fn keymap");
    let mut packets = Vec::new();
    let outcome = writer
        .write_commit_trigger_observed(&saved, &trigger, &mut |p| packets.push(p.packet))
        .expect("sent");
    assert_eq!((outcome.sends, outcome.packets), (1, 10));
    assert_eq!(packets, (1..=10).collect::<Vec<_>>());
    assert_eq!(outcome.table, TableId::Keymap { layer: Layer::Fn });
    writer.wait_for_commit().expect("saved");
    // A second and a third send of the Fn keymap: RepeatedWrite, like any
    // table.
    writer
        .write_commit_trigger(&saved, &trigger)
        .expect("second send");
    let err = writer.write_commit_trigger(&saved, &trigger).unwrap_err();
    assert!(matches!(err, WriteError::RepeatedWrite { .. }), "{err}");
    drop(writer);
    assert_eq!(session.stats().acks, 30);
    drop(session);
    let cmds: Vec<(u8, u8)> = common::write_bytes(&sim)
        .iter()
        .map(|b| (b[0], b[1]))
        .collect();
    let mut expected = vec![(0x18, 0x00); 10];
    expected.extend(vec![(0x18, 0x02); 20]);
    assert_eq!(cmds, expected);
    // The keyboard kept the base keymap.
    assert!(!sim.has_edits_waiting_for_fn_write());
    let base_table = sim
        .persisted_table(profile(1), SimTable::Keymap(Layer::Base))
        .expect("base keymap");
    assert_eq!(&base_table[46 * 4..47 * 4], &[0x00, 0x04, 0x00, 0x00]);
    sim.assert_clean();

    // Without a safety backup of the active profile nothing is sent.
    let mut sim = self::sim();
    sim.set_active_profile(profile(2));
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let request = keymap_with(&mut writer, Layer::Fn, |_| {});
    let trigger = CommitTrigger::try_from(request).expect("Fn keymap");
    let err = writer.write_commit_trigger(&saved, &trigger).unwrap_err();
    assert!(
        matches!(err, WriteError::NotBackedUp { profile: 2, .. }),
        "{err}"
    );
    drop(writer);
    drop(session);
    assert_eq!(sim.writes_sent(), 0);
    sim.assert_clean();
}

#[test]
fn a_commit_trigger_is_refused_after_another_program() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    let mut sim = sim();
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let base = keymap_with(&mut writer, Layer::Base, w_to_a);
    let trigger =
        CommitTrigger::try_from(keymap_with(&mut writer, Layer::Fn, |_| {})).expect("Fn keymap");
    drop(writer);
    drop(session);
    // A foreign ack while the base keymap is written: after the reads of
    // 0D, 21/11 and 18 80, right after its first packet.
    let at = sim.written().len() + 4;
    sim.inject_after_packets(at, report(&[0x21, 0, 0, 0, 0x01, 0x00]));
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let writes_before = writer.session().stats().writes;
    written(writer.write_guarded(&saved, &base));
    assert!(writer.foreign_activity_seen().is_some());
    let err = writer.write_commit_trigger(&saved, &trigger).unwrap_err();
    assert!(
        matches!(err, WriteError::OtherProgramActive { .. }),
        "{err}"
    );
    assert_eq!(writer.session().stats().writes - writes_before, 10);
    drop(writer);
    drop(session);
    assert!(sim.has_edits_waiting_for_fn_write());
    sim.assert_clean();
}

/// The ack of the first packet of a commit trigger times out. An unchanged
/// table reads back the same whether it arrived or not, so the whole table
/// is sent once more; when that send is cut short too, the write fails with
/// `CommitTriggerIncomplete` and the base keymap stays unsaved.
#[test]
fn a_commit_trigger_needs_every_packet_acknowledged() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    for both_sends in [false, true] {
        let mut sim = sim();
        // Writes 1-10: the base keymap; 11: the trigger's first packet; 12:
        // the first packet of its second send.
        sim.drop_ack(11);
        if both_sends {
            sim.drop_ack(12);
        }
        let mut session = Session::new(&mut sim);
        let mut writer = session.writer(consent()).expect("writer");
        let base = keymap_with(&mut writer, Layer::Base, w_to_a);
        let trigger = CommitTrigger::try_from(keymap_with(&mut writer, Layer::Fn, |_| {}))
            .expect("Fn keymap");
        written(writer.write_guarded(&saved, &base));
        assert!(writer.base_keymap_unsaved());
        let result = writer.write_commit_trigger(&saved, &trigger);
        if both_sends {
            let err = result.unwrap_err();
            assert!(
                matches!(
                    err,
                    WriteError::CommitTriggerIncomplete {
                        sends: 2,
                        packet: 1,
                        of: 10
                    }
                ),
                "{err}"
            );
            assert_eq!(err.kind(), ErrorKind::Timeout);
            assert!(writer.base_keymap_unsaved());
        } else {
            let outcome = result.expect("sent again in full");
            assert_eq!((outcome.sends, outcome.packets), (2, 11));
            assert!(outcome.warnings.contains(&WriteWarning::AckTimeout {
                packet: 1,
                of: 10,
                send: 1
            }));
            assert!(outcome.warnings.contains(&WriteWarning::Retried));
            assert!(!writer.base_keymap_unsaved());
        }
        writer.wait_for_commit().expect("waited");
        drop(writer);
        drop(session);
        let fn_packets = common::write_bytes(&sim)
            .iter()
            .filter(|b| b[..2] == [0x18, 0x02])
            .count();
        assert_eq!(fn_packets, if both_sends { 2 } else { 11 });
        assert_eq!(sim.has_edits_waiting_for_fn_write(), both_sends);
        sim.assert_clean();
    }
}

/// A base keymap written without the Fn keymap after it: the writer knows
/// that the keyboard has not saved it, and a profile switch reports that
/// it dropped it (the switch is made all the same). With the commit trigger
/// after it, or a changed Fn keymap, neither.
#[test]
fn a_switch_after_a_base_keymap_without_its_trigger_is_reported() {
    let dir = common::temp_dir();
    let saved = safety(&dir);
    for then in ["nothing", "trigger", "changed Fn keymap"] {
        let mut sim = sim();
        let mut session = Session::new(&mut sim);
        let mut writer = session.writer(consent()).expect("writer");
        assert!(!writer.base_keymap_unsaved());
        let base = keymap_with(&mut writer, Layer::Base, w_to_a);
        written(writer.write_guarded(&saved, &base));
        assert!(writer.base_keymap_unsaved(), "{then}");
        match then {
            "trigger" => {
                let trigger = CommitTrigger::try_from(keymap_with(&mut writer, Layer::Fn, |_| {}))
                    .expect("Fn keymap");
                writer.write_commit_trigger(&saved, &trigger).expect("sent");
            }
            "changed Fn keymap" => {
                let changed = keymap_with(&mut writer, Layer::Fn, w_to_a);
                written(writer.write_guarded(&saved, &changed));
            }
            _ => {}
        }
        let saves = then != "nothing";
        assert_eq!(writer.base_keymap_unsaved(), !saves, "{then}");
        let selected = writer.select_profile(profile(2)).expect("selected");
        assert!(selected.switched);
        assert_eq!(
            selected
                .warnings
                .contains(&WriteWarning::BaseKeymapNotSaved),
            !saves,
            "{then}: {:?}",
            selected.warnings
        );
        assert!(!writer.base_keymap_unsaved());
        let back = writer.select_profile(profile(1)).expect("back");
        assert!(!back.warnings.contains(&WriteWarning::BaseKeymapNotSaved));
        drop(writer);
        drop(session);
        let base_now = sim
            .table(profile(1), SimTable::Keymap(Layer::Base))
            .expect("base keymap");
        assert_eq!(
            base_now[46 * 4..47 * 4] == [0x00, 0x04, 0, 0],
            saves,
            "{then}"
        );
        sim.assert_clean();
    }
}

#[test]
fn a_round_trip_to_another_profile_on_the_keyboard_is_a_profile_change() {
    // Fn + 2, then Fn + 1, after we read profile 1: the keyboard pushes
    // both (byte 7 = 00). The profile is 1 again, but it was reloaded.
    let mut sim = sim();
    sim.switch_profile_at(Duration::from_millis(100), profile(2), true);
    sim.switch_profile_at(Duration::from_millis(400), profile(1), true);
    let mut session = Session::new(&mut sim);
    let now: openwraith_hid::protocol::ActiveProfile =
        session.read(ReadRequest::ActiveProfile).expect("21/11");
    assert_eq!(now.profile, 1);
    let err = session.writer(consent()).err().expect("refused");
    assert!(
        matches!(
            err,
            WriteError::ProfileChanged {
                expected: Some(1),
                reported: 1
            }
        ),
        "{err}"
    );
    assert_eq!(err.kind(), ErrorKind::ProfileChanged);
    assert!(
        err.to_string().contains("switched on the keyboard"),
        "{err}"
    );
    drop(session);
    assert_eq!(sim.writes_sent(), 0);

    // A push of the active profile alone (the push form, byte 7 = 00): a
    // switch on the keyboard too, not another program.
    let mut sim = self::sim();
    sim.push_input_at(
        Duration::from_millis(500),
        report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x00]),
    );
    let mut session = Session::new(&mut sim);
    session.read_raw_active_profile();
    let err = session.writer(consent()).err().expect("refused");
    assert!(matches!(err, WriteError::ProfileChanged { .. }), "{err}");

    // Another program's select: its acknowledgement and push. Its ack is
    // a foreign write acknowledgement: another program.
    let mut sim = self::sim();
    sim.push_input_at(
        Duration::from_millis(500),
        report(&[0x21, 0, 0, 0, 0x02, 0x12, 0x01]),
    );
    sim.push_input_at(
        Duration::from_millis(510),
        report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x02, 0x00]),
    );
    let mut session = Session::new(&mut sim);
    session.read_raw_active_profile();
    let err = session.writer(consent()).err().expect("refused");
    assert!(
        matches!(err, WriteError::OtherProgramActive { .. }),
        "{err}"
    );
}

/// Reads `21/11` (the profile the gate compares with).
trait ReadActive {
    fn read_raw_active_profile(&mut self);
}

impl<T: openwraith_hid::Transport> ReadActive for Session<T> {
    fn read_raw_active_profile(&mut self) {
        let now: openwraith_hid::protocol::ActiveProfile =
            self.read(ReadRequest::ActiveProfile).expect("21/11");
        assert_eq!(now.profile, 1);
    }
}
