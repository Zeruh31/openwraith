//! Session behaviour (spec section 2.8) against mock transports.

mod common;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use common::{
    C06, C07, W, assert_only_reads, captured_reset_report, captured_status_report, load, mock,
    report,
};
use openwraith_hid::protocol::capture::{Capture, CaptureEvent};
use openwraith_hid::protocol::{
    ColorTable, CustomTable, DeviceEvent, DeviceInfo, FrameError, FromResponse, KeyPos, KeyTrigger,
    KeymapTable, Layer, LightEffect, LightMode, OutPacket, ReadRequest, Report, Rgb, TriggerMode,
    is_keyboard_reset,
};
use openwraith_hid::{
    MockTransport, Session, SessionError, SessionOptions, Transport, TransportError,
};

fn w() -> ReadRequest {
    ReadRequest::KeyTrigger(KeyPos::new(2, 2).unwrap())
}

/// A `21/05` reply for row/col with the given actuation and mode.
fn trigger_reply(row: u8, col: u8, mode: u8, travel: u8) -> Report {
    report(&[
        0x21, 0, 0, 0, 0x0c, 0x05, mode, travel, travel, 0x0f, 0x0f, 0, 0, 0, 0, row, col,
    ])
}

#[test]
fn timeout_is_retried_once_then_reported() {
    let mut session = Session::new(MockTransport::new());
    let err = session.request(ReadRequest::Status).unwrap_err();
    assert!(
        matches!(
            err,
            SessionError::Timeout {
                request: ReadRequest::Status,
                attempts: 2,
                received: 0,
                expected: 1
            }
        ),
        "{err}"
    );
    assert_eq!(session.transport().written().len(), 2);
    assert_eq!(session.stats().retries, 1);

    // Without retries the request is sent once.
    let options = SessionOptions {
        retries: 0,
        ..SessionOptions::default()
    };
    let mut session = Session::with_options(MockTransport::new(), options);
    assert!(session.request(ReadRequest::Status).is_err());
    assert_eq!(session.transport().written().len(), 1);
}

#[test]
fn incomplete_table_reports_missing_chunks() {
    let mut mock = MockTransport::new();
    // Chunk 9 of the base keymap never arrives.
    let chunks = (0..9u8)
        .map(|i| report(&[0x18, 0x80, 0, i, 0x38]))
        .collect();
    mock.add_reply(ReadRequest::Keymap(Layer::Base), chunks);
    let mut session = Session::new(mock);
    let err = session
        .request(ReadRequest::Keymap(Layer::Base))
        .unwrap_err();
    assert!(
        matches!(
            err,
            SessionError::Timeout {
                received: 9,
                expected: 10,
                ..
            }
        ),
        "{err}"
    );
    // The retry got the same nine chunks again: duplicates, kept once.
    assert_eq!(session.stats().duplicates, 9);
}

#[test]
fn a_retry_completes_a_partial_table() {
    let base = ReadRequest::Keymap(Layer::Base);
    let mut mock = MockTransport::new();
    // The first attempt delivers chunks 0-8, the retry only the missing 9.
    mock.add_reply(
        base,
        (0..9u8)
            .map(|i| report(&[0x18, 0x80, 0, i, 0x38]))
            .collect(),
    );
    mock.add_reply(base, vec![report(&[0x18, 0x80, 0, 9, 0x18])]);
    let mut session = Session::new(mock);
    let raw = session
        .request(base)
        .expect("the retry delivers the missing chunk");
    assert_eq!(raw.reports().len(), 10);
    assert_eq!(session.stats().retries, 1);
    // Chunk lengths are valid: the table decodes.
    let table = KeymapTable::from_response(&raw).expect("decodes");
    assert_eq!(table.entries.len(), 132);
}

/// Chunk `i` of a `09 80` colour table whose bytes are all `fill`.
fn colour_chunk(i: u8, fill: u8) -> Report {
    let len: u8 = if i == 7 { 0x12 } else { 0x36 };
    let mut bytes = vec![0x09, 0x80, 0, i, len];
    bytes.extend(std::iter::repeat_n(fill, usize::from(len)));
    report(&bytes)
}

#[test]
fn a_retry_never_stitches_a_table_from_two_states() {
    let custom1 = ReadRequest::CustomColors(CustomTable::Custom1);
    let mut mock = MockTransport::new();
    // Attempt 1: chunks 0-3 of an all-0xAA table, then the stream stalls.
    mock.add_reply(custom1, (0..4).map(|i| colour_chunk(i, 0xAA)).collect());
    // Meanwhile another program rewrote the table: the retry streams 0xBB.
    mock.add_reply(custom1, (0..8).map(|i| colour_chunk(i, 0xBB)).collect());

    let mut session = Session::new(mock.clone());
    let err = session.request(custom1).unwrap_err();
    assert!(
        matches!(err, SessionError::Inconsistent { request } if request == custom1),
        "{err}"
    );
    assert_eq!(session.stats().conflicts, 1);
    assert_eq!(session.stats().retries, 1);

    // With one more attempt the request succeeds, with one consistent table.
    let options = SessionOptions {
        retries: 2,
        ..SessionOptions::default()
    };
    let mut session = Session::with_options(mock, options);
    let raw = session
        .request(custom1)
        .expect("the third attempt succeeds");
    let table = ColorTable::from_response(&raw).expect("decodes");
    assert!(
        table
            .colors
            .iter()
            .all(|c| *c == Rgb::new(0xBB, 0xBB, 0xBB)),
        "{:?}",
        table.colors
    );
    assert_only_reads(session.transport());
}

#[test]
fn a_lost_reply_is_recovered_by_the_retry() {
    let mut mock = MockTransport::new();
    // The first request gets no reply (like the lost heartbeat in c02), the
    // second one is answered.
    mock.add_reply(ReadRequest::Status, vec![]);
    mock.add_reply(ReadRequest::Status, vec![captured_status_report(C06)]);
    let mut session = Session::new(mock);
    let raw = session
        .request(ReadRequest::Status)
        .expect("retry succeeds");
    assert_eq!(raw.reports().len(), 1);
    assert_eq!(session.stats().retries, 1);
}

#[test]
fn replies_are_matched_by_header_and_echo_not_by_order() {
    let mut mock = MockTransport::new();
    mock.add_reply(
        w(),
        vec![
            // Another program's read of Q (row 2, col 1) arrives first.
            trigger_reply(2, 1, 0x00, 0x55),
            // A heartbeat reply and a trigger-write ack of another program.
            captured_status_report(C06),
            report(&[0x21, 0, 0, 0, 0x01, 0x00]),
            // The bare ack after a reset is not a reset notification.
            report(&[0x14, 0, 0, 0, 0x00]),
            trigger_reply(2, 2, 0x0c, 0x32),
        ],
    );
    let mut session = Session::new(mock);
    let trigger: KeyTrigger = session.read(w()).unwrap();
    assert_eq!(trigger.index(), W);
    assert_eq!(trigger.mode, TriggerMode::RapidTrigger);
    assert_eq!(trigger.actuation.to_string(), "1.00 mm");
    let stats = session.stats();
    assert_eq!(stats.chunks, 1);
    assert_eq!(stats.status_reports, 1);
    assert_eq!(stats.ignored, 3);
}

#[test]
fn stale_input_is_drained_before_a_request() {
    let mut mock = MockTransport::new();
    // A stale W reply (e.g. answer to another program's read) is waiting.
    mock.push_input(trigger_reply(2, 2, 0x00, 0x55));
    mock.add_reply(w(), vec![trigger_reply(2, 2, 0x0c, 0x32)]);
    let mut session = Session::new(mock);
    let trigger: KeyTrigger = session.read(w()).unwrap();
    assert_eq!(trigger.actuation.to_string(), "1.00 mm");
    assert_eq!(session.stats().drained, 1);
}

#[test]
fn pending_reset_notification_stops_before_sending() {
    let mut mock = mock(C06);
    mock.push_input(captured_reset_report());
    let mut session = Session::new(mock);
    let err = session.request(ReadRequest::DeviceInfo).unwrap_err();
    assert!(matches!(err, SessionError::KeyboardReset { .. }), "{err}");
    // Found while draining: nothing was sent at all.
    assert!(session.transport().written().is_empty());
}

#[test]
fn reset_notification_during_a_reply_is_never_answered() {
    let reset = captured_reset_report();
    assert!(is_keyboard_reset(&reset));
    let mut mock = mock(C06);
    mock.schedule(1, reset);
    let mut session = Session::new(mock);
    let err = session.request(ReadRequest::DeviceInfo).unwrap_err();
    match &err {
        SessionError::KeyboardReset { request, report } => {
            assert_eq!(*request, ReadRequest::DeviceInfo);
            assert!(report.starts_with("14 00 00 00 01 01"), "{report}");
        }
        other => panic!("unexpected error {other}"),
    }
    // Exactly the one read request was written; no reset was echoed.
    let mock = session.into_transport();
    assert_eq!(mock.written().len(), 1);
    assert_only_reads(&mock);
    // The reply that followed the reset is still unread: the session stopped.
    assert!(mock.pending_input() > 0);
}

#[test]
fn values_after_the_captured_reset() {
    // Replay only the traffic that followed the keyboard's reset in c07.
    let c07 = load(C07);
    let reset_at = c07
        .entries
        .iter()
        .position(|e| matches!(&e.event, CaptureEvent::In(r) if is_keyboard_reset(r)))
        .expect("reset in c07");
    let after = Capture {
        entries: c07.entries[reset_at + 1..].to_vec(),
    };
    let mut session = Session::new(MockTransport::from_capture(&after));
    let trigger: KeyTrigger = session.read(w()).unwrap();
    assert_eq!(trigger.mode, TriggerMode::Off);
    assert_eq!(trigger.actuation.to_string(), "1.70 mm");
    assert_eq!(trigger.rt_press.to_string(), "0.02 mm");
    let light: LightEffect = session.read(ReadRequest::MainLight).unwrap();
    assert_eq!(light.mode, LightMode::Wave);
    assert_only_reads(session.transport());
}

#[test]
fn unsolicited_profile_reports_are_recorded() {
    let mut mock = mock(C06);
    // Profile 2 became active (byte 7 = 00, as after a profile switch in c04).
    mock.push_input(report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x02, 0x00]));
    let mut session = Session::new(mock);
    let info: DeviceInfo = session.read(ReadRequest::DeviceInfo).unwrap();
    assert_eq!(info.product.as_deref(), Some("W75HEARGB"));
    assert_eq!(
        session.events(),
        &[DeviceEvent::ProfileReport {
            profile: 2,
            extra: 0
        }]
    );
}

/// A transport driven by a script, to test error handling.
struct Scripted {
    reads: VecDeque<Result<Option<Report>, TransportError>>,
    writes: usize,
}

impl Transport for Scripted {
    fn write(&mut self, _packet: &OutPacket) -> Result<(), TransportError> {
        self.writes += 1;
        Ok(())
    }

    fn read(&mut self, _timeout: Duration) -> Result<Option<Report>, TransportError> {
        self.reads.pop_front().unwrap_or(Ok(None))
    }
}

/// A transport that answers every read with another program's trigger-write
/// ack, forever.
struct Flood {
    writes: usize,
}

impl Transport for Flood {
    fn write(&mut self, _packet: &OutPacket) -> Result<(), TransportError> {
        self.writes += 1;
        Ok(())
    }

    fn read(&mut self, _timeout: Duration) -> Result<Option<Report>, TransportError> {
        Ok(Some(report(&[0x21, 0, 0, 0, 0x01, 0x00])))
    }
}

#[test]
fn unrelated_traffic_neither_blocks_the_drain_nor_extends_the_deadline() {
    let options = SessionOptions {
        timeout: Some(Duration::from_millis(20)),
        ..SessionOptions::default()
    };
    let max_drain = SessionOptions::default().max_drain;
    let mut session = Session::with_options(Flood { writes: 0 }, options);
    let started = Instant::now();
    let err = session.request(ReadRequest::Status).unwrap_err();
    assert!(
        matches!(
            err,
            SessionError::Timeout {
                attempts: 2,
                received: 0,
                ..
            }
        ),
        "{err}"
    );
    assert_eq!(session.transport().writes, 2);
    // The drain stops at its cap and the request is sent anyway.
    assert_eq!(session.stats().drained, u64::try_from(max_drain).unwrap());
    // Terminating at all shows that ignored reports do not extend the
    // deadline; the bound is generous for slow CI machines.
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn malformed_reports_are_skipped_and_io_errors_surface() {
    let profile = report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x01]);
    let transport = Scripted {
        reads: VecDeque::from([
            Ok(None), // drain: nothing stale
            Err(TransportError::Malformed(FrameError::BadLength { len: 7 })),
            Ok(Some(profile)),
        ]),
        writes: 0,
    };
    let mut session = Session::new(transport);
    assert!(session.request(ReadRequest::ActiveProfile).is_ok());
    assert_eq!(session.stats().malformed, 1);

    let transport = Scripted {
        reads: VecDeque::from([
            Ok(None),
            Err(TransportError::Read("device unplugged".into())),
        ]),
        writes: 0,
    };
    let mut session = Session::new(transport);
    let err = session.request(ReadRequest::ActiveProfile).unwrap_err();
    assert!(
        matches!(err, SessionError::Transport(TransportError::Read(_))),
        "{err}"
    );
    assert_eq!(session.transport().writes, 1);
}

#[test]
fn every_full_plan_request_is_served_by_the_c06_mock() {
    let mut session = Session::new(mock(C06));
    for request in ReadRequest::full_read_plan(&openwraith_hid::protocol::W75, None) {
        let raw = session
            .request(request)
            .unwrap_or_else(|e| panic!("{request}: {e}"));
        raw.decode().unwrap_or_else(|e| panic!("{request}: {e}"));
    }
    assert_eq!(session.stats().retries, 0);
    assert_only_reads(session.transport());
}

/// `Session::last_foreign_at` (the gate of automatic profile switches):
/// when another program's status report or write acknowledgement last
/// arrived, on the transport clock. The keyboard's own pushes, our replies
/// and our acknowledgements never set it, and re-arming the gate does not
/// clear it.
#[test]
fn last_foreign_at_is_when_another_program_was_last_heard() {
    use openwraith_hid::{ConsentOrigin, DeviceSim, WriteConsent};
    let ms = Duration::from_millis;
    let mut sim = DeviceSim::from_backup(&common::c06_backup());
    // Fn + Right Shift and a profile report from the keyboard, then another
    // program's heartbeat (a status report) and its write acknowledgement.
    sim.press_socd_hotkey_at(ms(100));
    sim.push_input_at(ms(200), report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x00]));
    sim.push_input_at(ms(1000), captured_status_report(C06));
    sim.push_input_at(ms(3000), report(&[0x19, 0, 0, 0, 0]));
    let start = Transport::now(&sim);
    let mut session = Session::new(&mut sim);
    assert_eq!(session.last_foreign_at(), None);

    // Our own read and its reply.
    let _: DeviceInfo = session.read(ReadRequest::DeviceInfo).unwrap();
    assert_eq!(session.last_foreign_at(), None);
    // The keyboard's own pushes.
    session.listen(ms(500)).unwrap();
    assert_eq!(session.socd_pushes(), 1);
    assert_eq!(session.events().len(), 2);
    assert_eq!(session.last_foreign_at(), None);
    // The heartbeat.
    session.listen(ms(1000)).unwrap();
    assert_eq!(session.stats().status_reports, 1);
    assert_eq!(session.last_foreign_at(), Some(start + ms(1000)));
    // The acknowledgement of a write we did not send.
    session.listen(ms(2500)).unwrap();
    assert_eq!(session.stats().foreign_acks, 1);
    let heard = start + ms(3000);
    assert_eq!(session.last_foreign_at(), Some(heard));

    // Re-arming the gate keeps the time; our own write and its
    // acknowledgement (a profile switch, after a quiet observation) do not
    // change it.
    session.rearm_gate();
    assert_eq!(session.last_foreign_at(), Some(heard));
    let mut writer = session
        .writer(WriteConsent::new(ConsentOrigin::CommandLineFlag))
        .expect("quiet since the gate");
    let profile2 = openwraith_hid::protocol::ProfileId::new(2).expect("2");
    writer.select_profile(profile2).expect("switched");
    drop(writer);
    session.listen(ms(500)).unwrap();
    assert_eq!(session.stats().foreign_acks, 1);
    assert_eq!(session.last_foreign_at(), Some(heard));
    drop(session);
    sim.assert_clean();
}
