//! The live travel test (key-depth view) against the simulator: the open
//! and close packets of c14, the guard that always closes, stops on profile
//! changes, other programs and resets, stray streams and the travel state
//! that refuses table writes.

mod common;

use std::ops::ControlFlow;
use std::panic::AssertUnwindSafe;
use std::time::Duration;

use common::{
    C06, c06_backup, captured_status_report, consent, load, pos, profile, report, temp_dir,
    write_bytes,
};
use openwraith_hid::protocol::{Distance, TravelVariant};
use openwraith_hid::{
    CloseResult, DeviceSim, ErrorKind, Session, TravelOptions, TravelState, TravelStop,
    TriggerEdit, WriteError, WriteOptions, apply_trigger_edit, live_travel, plan_trigger_edit,
    read_all,
};

fn sim() -> DeviceSim {
    DeviceSim::from_backup(&c06_backup())
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// The OUT packet at entry `index` of a capture.
fn captured(name: &str, index: usize) -> [u8; 63] {
    load(name)
        .out_packets()
        .find(|(i, _)| *i == index)
        .map(|(_, bytes)| *bytes)
        .unwrap_or_else(|| panic!("no OUT packet #{index} in {name}"))
}

const C14: &str = "capture_14_travel_test";

/// A slow press of W (long reports on the 72 ms grid) and a fast press of
/// Esc (short reports), starting `start` from now.
fn script(sim: &mut DeviceSim, start: Duration) {
    let w = pos(2, 2);
    let slow: Vec<(Duration, u16)> = (0..5u16)
        .map(|k| (start + ms(72 * u64::from(k)), 10 * (k + 1)))
        .collect();
    sim.travel_script(w, &slow);
    let esc = pos(0, 0);
    let at = start + ms(500);
    sim.travel_script(
        esc,
        &[
            (at, 40),
            (at + ms(1), 90),
            (at + ms(2), 169),
            (at + ms(80), 0),
        ],
    );
}

fn options(max_open: Duration) -> TravelOptions {
    TravelOptions {
        max_open,
        ..TravelOptions::default()
    }
}

#[test]
fn the_view_opens_streams_and_closes_with_the_captured_packets() {
    let mut sim = sim();
    // The gate observes for 2 s before the open; the keys move after it.
    script(&mut sim, Duration::from_secs(3));
    let mut seen = Vec::new();
    let mut frames = 0;
    let outcome = live_travel(
        &mut Session::new(&mut sim),
        consent(),
        &options(Duration::from_secs(3)),
        |frame| {
            frames += 1;
            seen.extend(frame.reports.iter().copied());
            ControlFlow::Continue(())
        },
    )
    .expect("opened");
    assert_eq!(outcome.stop, TravelStop::MaxOpen);
    assert_eq!(outcome.close, CloseResult::Acked);
    assert!(frames > 100, "{frames}");
    // Exactly the open and the close of c14 (#981, #1312), nothing else.
    assert_eq!(write_bytes(&sim), [captured(C14, 981), captured(C14, 1312)]);
    assert!(!sim.travel_open());
    // W slow (long reports, depth up to 50, then 0), Esc fast (short
    // reports up to 169, then 0).
    let w: Vec<(u16, bool)> = seen
        .iter()
        .filter(|r| r.pos == pos(2, 2))
        .map(|r| (r.depth, matches!(r.variant, TravelVariant::Long(_))))
        .collect();
    assert_eq!(
        w,
        [
            (10, true),
            (20, true),
            (30, true),
            (40, true),
            (50, true),
            (0, true)
        ]
    );
    let esc: Vec<u16> = seen
        .iter()
        .filter(|r| r.pos == pos(0, 0))
        .map(|r| r.depth)
        .collect();
    assert_eq!(esc, [40, 90, 169, 0]);
    assert!(
        seen.iter()
            .filter(|r| r.pos == pos(0, 0) && r.depth == 169)
            .all(|r| r.variant == TravelVariant::Short)
    );
    assert_eq!(outcome.reports, 10);
    sim.assert_clean();
}

#[test]
fn stream_reports_are_never_another_program_or_an_event() {
    let mut sim = sim();
    script(&mut sim, Duration::from_secs(3));
    let mut session = Session::new(&mut sim);
    let outcome = live_travel(
        &mut session,
        consent(),
        &options(Duration::from_secs(2)),
        |_| ControlFlow::Continue(()),
    )
    .expect("opened");
    assert_eq!(outcome.stop, TravelStop::MaxOpen);
    assert!(session.foreign_activity().is_quiet());
    assert!(session.events().is_empty());
    assert_eq!(session.stats().travel_reports, 10);
    assert_eq!(session.stats().stray_travel_reports, 0);
    assert_eq!(session.travel_state(), TravelState::Closed);
    // A write right after works: nothing is stray.
    drop(session);
    let dir = temp_dir();
    trigger_edit(&mut sim, &dir).expect("written");
    sim.assert_clean();
}

/// W to 1.00 mm through the trigger edit on a new session.
fn trigger_edit(sim: &mut DeviceSim, dir: &tempfile::TempDir) -> Result<(), String> {
    let backup = read_all(&mut Session::new(&mut *sim)).expect("backup");
    let saved = backup.save_safety(dir.path(), "set").expect("saved");
    let mut session = Session::new(&mut *sim);
    let plan = plan_trigger_edit(
        &mut session,
        &TriggerEdit {
            keys: vec![pos(2, 2)],
            actuation: Some(Distance(52)),
            rapid_trigger: None,
        },
    )
    .expect("plan");
    let report = apply_trigger_edit(
        &mut session,
        &plan,
        &saved,
        consent(),
        &WriteOptions::default(),
        |_| ControlFlow::Continue(()),
    );
    if report.is_success() {
        Ok(())
    } else {
        Err(format!("{:?}", report.error))
    }
}

#[test]
fn the_sink_stops_the_view_and_it_is_closed() {
    let mut sim = sim();
    script(&mut sim, Duration::from_secs(3));
    let outcome = live_travel(
        &mut Session::new(&mut sim),
        consent(),
        &options(Duration::from_secs(60)),
        |frame| {
            if frame.reports.iter().any(|r| r.depth >= 30) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        },
    )
    .expect("opened");
    assert_eq!(outcome.stop, TravelStop::Requested);
    assert_eq!(outcome.close, CloseResult::Acked);
    assert_eq!((sim.travel_opens(), sim.travel_closes()), (1, 1));
    assert!(!sim.travel_open());
    sim.assert_clean();
}

#[test]
fn a_profile_change_on_the_keyboard_stops_and_closes() {
    let mut sim = sim();
    // Fn + 2 three seconds in: the keyboard pushes the new profile.
    sim.switch_profile_at(Duration::from_secs(3), profile(2), true);
    let outcome = live_travel(
        &mut Session::new(&mut sim),
        consent(),
        &options(Duration::from_secs(60)),
        |_| ControlFlow::Continue(()),
    )
    .expect("opened");
    assert_eq!(outcome.stop, TravelStop::ProfileChanged { profile: 2 });
    assert_eq!(outcome.close, CloseResult::Acked);
    assert!(!sim.travel_open());
    sim.assert_clean();
}

#[test]
fn another_program_stops_and_closes() {
    let mut sim = sim();
    sim.push_input_at(Duration::from_secs(3), captured_status_report(C06));
    let outcome = live_travel(
        &mut Session::new(&mut sim),
        consent(),
        &options(Duration::from_secs(60)),
        |_| ControlFlow::Continue(()),
    )
    .expect("opened");
    assert!(
        matches!(outcome.stop, TravelStop::OtherProgram { evidence } if evidence.status_reports == 1),
        "{:?}",
        outcome.stop
    );
    assert_eq!(outcome.close, CloseResult::Acked);
    assert!(!sim.travel_open());
    sim.assert_clean();
}

#[test]
fn a_reset_stops_without_sending_anything_not_even_the_close() {
    let mut sim = sim();
    sim.push_input_at(Duration::from_secs(3), report(&[0x14, 0, 0, 0, 0x01, 0x01]));
    let outcome = live_travel(
        &mut Session::new(&mut sim),
        consent(),
        &options(Duration::from_secs(60)),
        |_| ControlFlow::Continue(()),
    )
    .expect("opened");
    assert_eq!(outcome.stop, TravelStop::KeyboardReset);
    assert!(matches!(outcome.close, CloseResult::NotSent { .. }));
    // Only the open went out; the test stays open on the keyboard.
    assert_eq!(write_bytes(&sim), [captured(C14, 981)]);
    assert!(sim.travel_open());
    sim.allow_travel_left_open();
    // Nothing after the reset (assert_clean refuses any packet after it).
    sim.assert_clean();
}

#[test]
fn the_guard_closes_on_drop_and_on_panic() {
    // Dropped without close().
    let mut sim = sim();
    {
        let mut session = Session::new(&mut sim);
        let mut writer = session.writer(consent()).expect("writer");
        let mut test = writer.open_travel_test().expect("opened");
        test.poll(ms(100)).expect("polled");
    }
    assert_eq!((sim.travel_opens(), sim.travel_closes()), (1, 1));
    assert!(!sim.travel_open());
    sim.assert_clean();

    // A panic while the test is open: unwinding drops the guard.
    let mut sim = self::sim();
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let mut session = Session::new(&mut sim);
        let mut writer = session.writer(consent()).expect("writer");
        let _test = writer.open_travel_test().expect("opened");
        panic!("a bug while the test is open");
    }));
    assert!(result.is_err());
    assert_eq!((sim.travel_opens(), sim.travel_closes()), (1, 1));
    assert!(!sim.travel_open());
    sim.assert_clean();
}

/// Plans and applies W to 0.80 mm on `session` (a safety backup of the
/// active profile in `saved`).
fn edit_on<T: openwraith_hid::WriteTransport>(
    session: &mut Session<T>,
    saved: &openwraith_hid::SavedBackup,
) -> openwraith_hid::EditReport {
    let plan = plan_trigger_edit(
        session,
        &TriggerEdit {
            keys: vec![pos(2, 2)],
            actuation: Some(Distance(40)),
            rapid_trigger: None,
        },
    )
    .expect("plan");
    apply_trigger_edit(
        session,
        &plan,
        saved,
        consent(),
        &WriteOptions::default(),
        |_| ControlFlow::Continue(()),
    )
}

#[test]
fn an_unacknowledged_close_blocks_table_writes_until_a_close_is_acknowledged() {
    let mut sim = sim();
    let dir = temp_dir();
    let backup = read_all(&mut Session::new(&mut sim)).expect("backup");
    let saved = backup.save_safety(dir.path(), "set").expect("saved");
    // Write packets: 1 = open, 2 and 3 = close and its retry.
    sim.drop_ack(2);
    sim.drop_ack(3);
    let mut session = Session::new(&mut sim);
    {
        let mut writer = session.writer(consent()).expect("writer");
        let test = writer.open_travel_test().expect("opened");
        assert_eq!(test.close(), CloseResult::NotAcked);
    }
    assert_eq!(session.travel_state(), TravelState::MaybeOpen);
    // Table writes are refused (nothing is sent), selects are not.
    let before = session.stats().writes;
    let refused = edit_on(&mut session, &saved);
    assert_eq!(
        refused.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::TravelTestOpen)
    );
    assert_eq!(session.stats().writes, before);
    {
        let mut writer = session.writer(consent()).expect("writer");
        writer.select_profile(profile(2)).expect("select");
        writer.select_profile(profile(1)).expect("back");
        assert!(matches!(
            writer.open_travel_test().err(),
            Some(WriteError::TravelTestMaybeOpen)
        ));
        // An acknowledged close lifts the block.
        assert_eq!(
            writer.close_stray_travel_test().expect("sent"),
            CloseResult::Acked
        );
    }
    assert_eq!(session.travel_state(), TravelState::Closed);
    let written = edit_on(&mut session, &saved);
    assert!(written.is_success(), "{:#?}", written.error);
    drop(session);
    sim.assert_clean();
}

#[test]
fn an_unacknowledged_open_is_closed_at_once() {
    let mut sim = sim();
    sim.drop_ack(1);
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let error = writer.open_travel_test().err().expect("not acknowledged");
    assert!(matches!(error, WriteError::TravelOpenNotAcked));
    drop(writer);
    assert_eq!(session.travel_state(), TravelState::Closed);
    drop(session);
    assert_eq!((sim.travel_opens(), sim.travel_closes()), (1, 1));
    sim.assert_clean();
}

#[test]
fn a_stray_stream_refuses_table_writes_but_not_selects_until_it_is_closed() {
    let mut sim = sim();
    // A test a crashed run left open: the keyboard streams W.
    sim.leave_travel_open();
    sim.travel_script(pos(2, 2), &[(ms(500), 20), (ms(572), 40)]);
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("not another program");
    assert!(writer.session().foreign_activity().is_quiet());
    assert!(writer.session().stray_travel_since_gate() >= 2);
    // A select is allowed.
    let selected = writer.select_profile(profile(2)).expect("select");
    assert!(selected.switched);
    writer.select_profile(profile(1)).expect("back");
    // A travel open is refused while it streams.
    assert!(matches!(
        writer.open_travel_test().err(),
        Some(WriteError::TravelTestOpen { stray_reports }) if stray_reports >= 2
    ));
    assert_eq!(
        writer.close_stray_travel_test().expect("sent"),
        CloseResult::Acked
    );
    assert_eq!(writer.session().stray_travel_since_gate(), 0);
    drop(writer);
    drop(session);
    assert!(!sim.travel_open());
    let dir = temp_dir();
    trigger_edit(&mut sim, &dir).expect("written after the close");
    sim.assert_clean();
}

#[test]
fn a_stray_stream_seen_by_a_later_session_refuses_its_table_writes() {
    let mut sim = sim();
    sim.leave_travel_open();
    sim.travel_script(pos(2, 2), &[(ms(100), 20)]);
    let dir = temp_dir();
    let refused = trigger_edit(&mut sim, &dir).expect_err("refused");
    assert!(refused.contains("TravelTestOpen"), "{refused}");
    // Nothing but reads was sent.
    assert!(write_bytes(&sim).is_empty());
    sim.assert_clean();
}

/// The 1-based number (reads and writes, as `DeviceSim::inject_after_packets`
/// counts) of the first write packet `run` sends to a copy of `sim` that
/// satisfies `want`.
fn packet_number(
    sim: &DeviceSim,
    run: impl FnOnce(&mut DeviceSim),
    want: impl Fn(&[u8; 63]) -> bool,
) -> usize {
    let mut copy = sim.clone();
    let start = copy.written().len();
    run(&mut copy);
    start
        + copy.written()[start..]
            .iter()
            .position(|p| p.is_write() && want(p.as_bytes()))
            .expect("the packet")
        + 1
}

#[test]
fn stream_reports_mid_write_do_not_disturb_the_acks_and_count_as_stray() {
    let mut sim = sim();
    let dir = temp_dir();
    let backup = read_all(&mut Session::new(&mut sim)).expect("backup");
    let saved = backup.save_safety(dir.path(), "set").expect("saved");
    let edit = TriggerEdit {
        keys: vec![pos(2, 2), pos(2, 1)],
        actuation: Some(Distance(40)),
        rapid_trigger: None,
    };
    let apply = |sim: &mut DeviceSim| {
        let mut session = Session::new(sim);
        let plan = plan_trigger_edit(&mut session, &edit).expect("plan");
        let report = apply_trigger_edit(
            &mut session,
            &plan,
            &saved,
            consent(),
            &WriteOptions::default(),
            |_| ControlFlow::Continue(()),
        );
        let stats = session.stats().clone();
        let quiet = session.foreign_activity().is_quiet();
        (report, stats, quiet)
    };
    // 300 stream reports right after the first trigger packet, before its
    // acknowledgement.
    let at = packet_number(
        &sim,
        |copy| {
            apply(copy);
        },
        |b| b[..5] == [0x21, 0, 0, 0, 0x18] && b[5] != 0x02 && b[5] != 0x03,
    );
    for _ in 0..300 {
        sim.inject_after_packets(at, report(&[0x21, 0, 0, 0, 0x05, 0x01, 2, 2, 0x10, 0]));
    }
    let (report, stats, quiet) = apply(&mut sim);
    // The first key's acknowledgement was matched as usual ...
    assert!(
        matches!(
            &report.items[0].result,
            openwraith_hid::ItemResult::Written { sends: 1, warnings, .. } if warnings.is_empty()
        ),
        "{:#?}",
        report.items
    );
    // ... and the stream, which no test of ours opened, stops the edit
    // before the next table: nothing is written while it streams.
    assert_eq!(
        report.error.as_ref().map(|e| e.kind),
        Some(ErrorKind::TravelTestOpen)
    );
    assert!(matches!(
        report.items[1].result,
        openwraith_hid::ItemResult::Failed { .. }
    ));
    assert!(quiet);
    assert_eq!(
        (stats.travel_reports, stats.stray_travel_reports),
        (300, 300)
    );
    sim.assert_clean();
}

#[test]
fn a_full_queue_counts_dropped_reports() {
    let mut sim = sim();
    let open = |sim: &mut DeviceSim| {
        let mut session = Session::new(sim);
        let mut writer = session.writer(consent()).expect("writer");
        let test = writer.open_travel_test().expect("opened");
        test.close()
    };
    // 1500 stream reports right after the open, before its acknowledgement:
    // the open's wait reads them all into the queue of the test.
    let at = packet_number(
        &sim,
        |copy| {
            open(copy);
        },
        |b| b[..6] == [0x21, 0, 0, 0, 0x18, 0x02],
    );
    for _ in 0..1500 {
        sim.inject_after_packets(at, report(&[0x21, 0, 0, 0, 0x05, 0x01, 0, 0, 0x30, 0]));
    }
    let mut session = Session::new(&mut sim);
    let mut writer = session.writer(consent()).expect("writer");
    let mut test = writer.open_travel_test().expect("opened");
    let frame = test.poll(Duration::ZERO).expect("poll");
    assert_eq!((frame.reports.len(), frame.dropped), (1024, 476));
    assert_eq!(test.close(), CloseResult::Acked);
    drop(writer);
    assert_eq!(
        (
            session.stats().travel_reports,
            session.stats().travel_dropped,
            session.stats().stray_travel_reports
        ),
        (1500, 476, 0)
    );
    drop(session);
    sim.assert_clean();
}

#[test]
fn the_open_waits_for_the_keyboard_to_save_the_last_write() {
    let mut sim = sim();
    let dir = temp_dir();
    let backup = read_all(&mut Session::new(&mut sim)).expect("backup");
    let saved = backup.save_safety(dir.path(), "set").expect("saved");
    let mut session = Session::new(&mut sim);
    let plan = plan_trigger_edit(
        &mut session,
        &TriggerEdit {
            keys: vec![pos(2, 2)],
            actuation: Some(Distance(40)),
            rapid_trigger: None,
        },
    )
    .expect("plan");
    let request = plan.keys()[0].request().expect("a change").clone();
    {
        // A table write without the wait, then the open on the same writer.
        let mut writer = session.writer(consent()).expect("writer");
        writer.write_guarded(&saved, &request).expect("written");
        let test = writer.open_travel_test().expect("opened");
        assert_eq!(test.close(), CloseResult::Acked);
    }
    drop(session);
    let times: Vec<(Duration, [u8; 63])> = sim
        .written()
        .iter()
        .zip(sim.sent_times())
        .filter(|(p, _)| p.is_write())
        .map(|(p, t)| (*t, *p.as_bytes()))
        .collect();
    let write_at = times
        .iter()
        .find(|(_, b)| b[..5] == [0x21, 0, 0, 0, 0x18] && b[5] == 0x0c)
        .map(|(t, _)| *t)
        .expect("the trigger write");
    let open_at = times
        .iter()
        .find(|(_, b)| b[..6] == [0x21, 0, 0, 0, 0x18, 0x02])
        .map(|(t, _)| *t)
        .expect("the open");
    assert!(
        open_at - write_at >= Duration::from_secs(2),
        "{:?}",
        open_at - write_at
    );
    // The keyboard kept the write.
    assert_eq!(
        sim.persisted_trigger(profile(1), pos(2, 2)).map(|r| r[1]),
        Some(40)
    );
    sim.assert_clean();
}
