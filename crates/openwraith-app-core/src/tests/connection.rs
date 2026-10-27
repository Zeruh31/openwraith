//! The connection state machine on the simulator.

use std::time::Duration;

use openwraith_hid::protocol::ReadRequest;

use super::harness::Harness;
use crate::dto::{AppEvent, ConnectionState, Notice, SimAction, WriteBlockReason};
use crate::worker::Conn;

#[test]
fn a_keyboard_is_observed_then_read() {
    let mut h = Harness::new(1);
    h.tick();
    assert_eq!(h.worker.conn, Conn::Observing);
    // Listen only for 2 s: nothing is sent.
    h.ticks(15);
    assert_eq!(h.worker.conn, Conn::Observing);
    assert_eq!(h.packets(), 0, "nothing is sent while observing");
    h.until_ready();
    let status = h.status();
    assert_eq!(status.connection, ConnectionState::Ready);
    assert!(status.write_gate.allowed, "{:?}", status.write_gate);
    assert_eq!(status.active_profile, Some(1));
    assert_eq!(status.firmware_tested, Some(true));
    assert_eq!(status.generation, 1);
    let snapshot = h.snapshot();
    assert_eq!(snapshot.profile, 1);
    assert_eq!(snapshot.triggers.keys.len(), 82);
    assert!(snapshot.triggers.editable);
    // W: 1.00 mm, Rapid Trigger 0.30 mm (c06).
    let w = &snapshot
        .triggers
        .keys
        .iter()
        .find(|k| k.key.name == "W")
        .unwrap();
    let value = w.value.unwrap();
    assert_eq!((value.actuation, value.rt_press), (50, 15));
    let knob = snapshot
        .triggers
        .keys
        .iter()
        .find(|k| k.key.index == 15)
        .unwrap();
    assert!(!knob.selectable);
    assert!(snapshot.light.editable);
    assert!(h.notices().contains(&Notice::Connected));
    // Only reads were sent.
    assert_eq!(h.writes(), 0);
    h.assert_clean();
}

#[test]
fn a_heartbeat_while_observing_means_another_program_and_nothing_is_sent() {
    let mut h = Harness::new(1);
    h.tick();
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 3,
            interval_ms: 400,
        })
        .unwrap();
    h.ticks(40);
    assert!(
        matches!(h.worker.conn, Conn::OtherProgram { evidence } if evidence.status_reports >= 1),
        "{:?}",
        h.worker.conn
    );
    assert_eq!(h.packets(), 0, "not even reads after another program");
    let status = h.status();
    assert!(!status.write_gate.allowed);
    assert!(
        status
            .write_gate
            .reasons
            .contains(&WriteBlockReason::OtherProgram)
    );
    h.assert_clean();
}

#[test]
fn profile_reports_while_observing_rearm_the_gate() {
    let mut h = Harness::new(1);
    h.tick();
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 2,
            after_ms: 500,
            report: true,
        })
        .unwrap();
    h.until_ready();
    assert_eq!(h.snapshot().profile, 2);
    h.assert_clean();
}

#[test]
fn an_unplug_while_idle_is_a_lost_connection_and_a_replug_reconnects() {
    let mut h = Harness::ready();
    h.sim.apply(SimAction::Unplug).unwrap();
    h.tick();
    assert!(
        matches!(h.worker.conn, Conn::Lost { .. }),
        "{:?}",
        h.worker.conn
    );
    assert!(h.shared.snapshot().is_none());
    assert!(h.notices().contains(&Notice::Disconnected));
    h.ticks(30);
    assert!(matches!(h.worker.conn, Conn::Lost { .. }));
    h.sim.apply(SimAction::Replug).unwrap();
    h.until_ready();
    assert_eq!(h.status().generation, 2);
    h.assert_clean();
}

#[test]
fn a_reset_while_idle_stops_everything_until_reconnect() {
    let mut h = Harness::ready();
    let before = h.packets();
    h.sim.apply(SimAction::KeyboardReset).unwrap();
    h.tick();
    assert_eq!(h.worker.conn, Conn::ResetSeen);
    assert!(h.worker.session.is_none());
    // 30 s: nothing is sent, no automatic reconnect.
    h.ticks(300);
    assert_eq!(h.worker.conn, Conn::ResetSeen);
    assert_eq!(h.packets(), before, "nothing after the reset");
    // The user reconnects: a new session, observed first.
    h.op(|reply| crate::worker::Request::Reconnect { reply })
        .unwrap();
    assert_eq!(h.worker.conn, Conn::Observing);
    assert_eq!(h.packets(), before, "the observation sends nothing");
    h.until_ready();
    assert_eq!(h.status().generation, 2);
    h.assert_clean();
}

#[test]
fn two_keyboards_need_a_choice_and_nothing_is_opened() {
    let mut h = Harness::new(2);
    h.ticks(5);
    let ConnectionState::SeveralKeyboards { devices } = h.status().connection else {
        panic!("{:?}", h.status().connection);
    };
    assert_eq!(devices.len(), 2);
    assert!(h.worker.session.is_none());
    assert_eq!(h.packets(), 0);
    assert_eq!(h.sim.with(1, |s| s.written().len()), 0);
    let id = devices[1].id.clone();
    h.op(|reply| crate::worker::Request::SelectDevice { id, reply })
        .unwrap();
    h.until_ready();
    assert_eq!(h.packets(), 0, "the first keyboard was not used");
    assert!(h.sim.with(1, |s| !s.written().is_empty()));
}

#[test]
fn open_failures_back_off() {
    let mut h = Harness::new(1);
    h.sim.fail_next_opens(0, 2);
    h.tick();
    let Conn::OpenFailed { .. } = h.worker.conn else {
        panic!("{:?}", h.worker.conn);
    };
    let ConnectionState::OpenFailed { retry_in_ms, .. } = h.status().connection else {
        panic!();
    };
    assert_eq!(retry_in_ms, 2000);
    // The second attempt after 2 s fails too; the third after 5 s more.
    h.ticks(21);
    assert!(matches!(h.worker.conn, Conn::OpenFailed { .. }));
    let ConnectionState::OpenFailed { retry_in_ms, .. } = h.status().connection else {
        panic!();
    };
    assert!(retry_in_ms > 4000, "{retry_in_ms}");
    h.until_ready();
}

#[test]
fn an_unsupported_keyboard_gets_only_the_device_info_request() {
    let mut h = Harness::new(1);
    h.with_sim(|sim| *sim = super::seeds::unsupported_sim());
    h.ticks(40);
    assert!(
        matches!(&h.worker.conn, Conn::Unsupported { product, .. } if product.as_deref() == Some("AP61HEARGB")),
        "{:?}",
        h.worker.conn
    );
    let sent: Vec<_> = h.packets_since(0);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        openwraith_hid::protocol::ReadRequest::from_packet(sent[0].as_bytes()),
        Some(ReadRequest::DeviceInfo)
    );
    // It stays unsupported while connected; nothing more is sent.
    h.ticks(50);
    assert_eq!(h.packets(), 1);
}

#[test]
fn status_events_are_published_on_change() {
    let mut h = Harness::ready();
    let statuses: Vec<_> = h
        .events
        .all()
        .into_iter()
        .filter_map(|e| match e {
            AppEvent::Status { status } => Some(*status),
            AppEvent::Notice { .. } => None,
        })
        .collect();
    assert!(statuses.len() >= 3);
    assert!(statuses.windows(2).all(|w| w[0].revision < w[1].revision));
    assert!(statuses.windows(2).all(|w| w[0] != w[1]));
    // Idle ticks change nothing and publish nothing.
    h.events.clear();
    h.ticks(10);
    assert!(h.events.all().is_empty());
    let _ = Duration::ZERO;
}

#[test]
fn untested_firmware_is_shown_but_not_blocking() {
    let mut h = Harness::new(1);
    h.with_sim(|s| *s = super::seeds::firmware_sim("3.23.01"));
    h.until_ready();
    let status = h.status();
    assert_eq!(status.firmware_tested, Some(false));
    assert!(status.write_gate.allowed);
    assert_eq!(status.device.unwrap().firmware.as_deref(), Some("3.23.01"));
}
