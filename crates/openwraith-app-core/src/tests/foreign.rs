//! Another program heard during a read or an operation: the gate is never
//! re-armed over its evidence, and nothing more is sent after it (no
//! refresh of the view either).

use std::sync::{Arc, Mutex};

use openwraith_hid::SimTable;
use openwraith_hid::protocol::ReadRequest;

use super::harness::{Harness, Watch, is_write_start, pos, profile};
use crate::dto::{
    AppErrorKind, ErrorDetail, ProgressStep, ReadPurpose, RestoreIntent, TriggerEditIntent,
    WriteBlockReason,
};
use crate::worker::Conn;

const W: u8 = 46;
const FACTORY: [u8; 9] = [0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0];

/// The vendor site's heartbeat early in the next read, then a profile
/// switch on the keyboard (with its report) later in the same read.
fn heartbeat_then_switch(h: &Harness) {
    let base = h.packets();
    let heartbeat = h.sim.heartbeat();
    h.with_sim(|s| {
        s.inject_after_packets(base + 2, heartbeat);
        s.switch_profile_after_packets(base + 50, profile(2));
    });
}

/// Everything is blocked and nothing is sent: prepare is refused, the
/// worker only listens, and nothing was ever written.
fn assert_blocked_and_silent(h: &mut Harness) {
    assert_blocked_from_now(h);
    assert_eq!(h.writes(), 0, "nothing was written");
}

/// Everything is blocked and nothing more is sent: prepare is refused,
/// the worker only listens.
fn assert_blocked_from_now(h: &mut Harness) {
    assert!(
        matches!(h.worker.conn, Conn::OtherProgram { .. }),
        "{:?}",
        h.worker.conn
    );
    let status = h.status();
    assert!(!status.write_gate.allowed);
    assert!(
        status
            .write_gate
            .reasons
            .contains(&WriteBlockReason::OtherProgram)
    );
    let activity = h
        .worker
        .session
        .as_ref()
        .map(|s| s.activity_since_gate())
        .unwrap();
    assert!(activity.status_reports > 0, "the evidence was kept");
    let before = h.packets();
    let intent = TriggerEditIntent {
        keys: vec![W],
        actuation: Some(60),
        rapid_trigger: None,
        based_on: h.shared.snapshot().map_or(0, |s| s.revision),
    };
    let error = h.prepare_trigger(intent).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::WritesBlocked);
    h.ticks(30);
    assert_eq!(h.packets(), before, "only listening");
}

#[test]
fn a_refresh_keeps_the_evidence_of_another_program_over_a_profile_change() {
    let mut h = Harness::ready();
    heartbeat_then_switch(&h);
    let error = h.refresh().unwrap_err();
    assert_eq!(error.kind, AppErrorKind::OtherProgramActive);
    assert!(matches!(
        error.detail,
        Some(ErrorDetail::OtherProgram { .. })
    ));
    assert_blocked_and_silent(&mut h);
    h.assert_clean();
}

#[test]
fn the_first_read_keeps_the_evidence_of_another_program_over_a_profile_change() {
    let mut h = Harness::new(1);
    assert!(h.until(|w| w.conn == Conn::Observing, 50));
    heartbeat_then_switch(&h);
    assert!(h.until(
        |w| !matches!(w.conn, Conn::Observing | Conn::Reading { .. }),
        100
    ));
    assert_blocked_and_silent(&mut h);
    h.assert_clean();
}

/// The vendor site's heartbeat right after the reply to the `n`-th packet
/// from now.
fn heartbeat_after(h: &Harness, n: usize) -> usize {
    let base = h.packets();
    let heartbeat = h.sim.heartbeat();
    h.with_sim(|s| s.inject_after_packets(base + n, heartbeat));
    base
}

/// After the heartbeat (behind the reply to packet `base + n`), the read
/// stopped: at most the request in flight when it arrived was completed.
fn assert_stopped_after(h: &Harness, base: usize, n: usize) {
    let sent = h.packets() - base;
    assert!(
        sent <= n + 1,
        "{sent} requests, {} after the heartbeat",
        sent.saturating_sub(n)
    );
    let active = ReadRequest::ActiveProfile.packet();
    assert!(
        h.packets_since(base + n)
            .iter()
            .all(|p| p.as_bytes() != active.as_bytes()),
        "no 21/11 after the heartbeat"
    );
}

#[test]
fn a_refresh_stops_sending_as_soon_as_another_program_is_heard() {
    let mut h = Harness::ready();
    let base = heartbeat_after(&h, 2);
    let error = h.refresh().unwrap_err();
    assert_eq!(error.kind, AppErrorKind::OtherProgramActive);
    assert_stopped_after(&h, base, 2);
    assert_blocked_and_silent(&mut h);
    h.assert_clean();
}

#[test]
fn the_first_read_stops_sending_as_soon_as_another_program_is_heard() {
    let mut h = Harness::new(1);
    assert!(h.until(|w| w.conn == Conn::Observing, 50));
    let base = heartbeat_after(&h, 2);
    assert!(h.until(
        |w| !matches!(w.conn, Conn::Observing | Conn::Reading { .. }),
        100
    ));
    assert_stopped_after(&h, base, 2);
    assert!(h.shared.snapshot().is_none());
    assert_blocked_and_silent(&mut h);
    h.assert_clean();
}

#[test]
fn a_safety_read_stops_sending_as_soon_as_another_program_is_heard() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    let base = heartbeat_after(&h, 2);
    let watch = h.watch();
    let error = h
        .apply_trigger(&Harness::token(&preview.token), &watch)
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::OtherProgramActive);
    assert_stopped_after(&h, base, 2);
    assert_eq!(h.writes(), 0);
    assert!(h.safety_files().is_empty(), "no safety backup was saved");
    assert!(!h.worker.config.journal_path().exists());
    assert!(!watch.steps().iter().any(|s| matches!(
        s,
        ProgressStep::SavingSafetyBackup | ProgressStep::Writing { .. }
    )));
    assert_blocked_and_silent(&mut h);
    h.assert_clean();
}

#[test]
fn a_read_sends_nothing_when_another_program_was_heard_before_it() {
    let mut h = Harness::ready();
    // Queued before the refresh starts.
    let heartbeat = h.sim.heartbeat();
    h.with_sim(|s| s.push_input(heartbeat));
    let base = h.packets();
    let error = h.refresh().unwrap_err();
    assert_eq!(error.kind, AppErrorKind::OtherProgramActive);
    assert_eq!(h.packets(), base, "not even the first request");
    assert_blocked_and_silent(&mut h);
}

/// A switch on the keyboard without a profile report (PROTOCOL Q2), then
/// 31 s without traffic: the focus check is due.
fn silent_switch_then_quiet(h: &mut Harness) {
    h.sim
        .apply(crate::dto::SimAction::KeyboardSwitch {
            profile: 3,
            after_ms: 0,
            report: false,
        })
        .unwrap();
    h.ticks(2);
    h.advance(std::time::Duration::from_secs(31));
}

#[test]
fn the_focus_check_sends_nothing_over_a_heartbeat_that_arrived_before_it() {
    let mut h = Harness::ready();
    silent_switch_then_quiet(&mut h);
    let heartbeat = h.sim.heartbeat();
    h.with_sim(|s| s.push_input(heartbeat));
    let base = h.packets();
    h.post(crate::worker::Request::WindowFocused);
    assert_eq!(h.packets(), base);
    assert_blocked_and_silent(&mut h);
}

#[test]
fn the_focus_check_reads_nothing_more_over_a_heartbeat_heard_with_its_reply() {
    let mut h = Harness::ready();
    silent_switch_then_quiet(&mut h);
    let base = heartbeat_after(&h, 1);
    h.post(crate::worker::Request::WindowFocused);
    assert_eq!(h.packets() - base, 1, "the 21/11 only, no full read");
    // The snapshot described profile 1: it is dropped.
    assert!(h.shared.snapshot().is_none());
    assert_eq!(h.status().active_profile, Some(3));
    assert_blocked_and_silent(&mut h);
}

/// Injects the heartbeat behind the reply to the view refresh's first
/// read; returns the packet count when the refresh started.
fn heartbeat_in_view_refresh(h: &Harness, watch: &Arc<Watch>) -> Arc<Mutex<Option<usize>>> {
    let at = Arc::new(Mutex::new(None));
    let sim = h.sim.clone();
    let heartbeat = sim.heartbeat();
    let mark = Arc::clone(&at);
    watch.on_event(move |event, _| {
        if matches!(event.step, ProgressStep::RefreshingView) {
            let packets = sim.with(0, |s| s.written().len());
            *mark.lock().unwrap() = Some(packets);
            sim.with(0, |s| {
                s.inject_after_packets(packets + 1, heartbeat.clone())
            });
        }
    });
    at
}

#[test]
fn the_view_refresh_after_a_trigger_edit_stops_once_another_program_is_heard() {
    let mut h = Harness::ready();
    let keys: Vec<u8> = (45..57).collect();
    let preview = h
        .prepare_trigger(h.trigger_intent(&keys, Some(60), None))
        .unwrap();
    let watch = h.watch();
    let at = heartbeat_in_view_refresh(&h, &watch);
    let report = h
        .apply_trigger(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.items.len(), 12);
    let start = at.lock().unwrap().expect("the view was refreshed");
    assert_eq!(h.packets() - start, 1, "one key read, then nothing");
    assert!(h.status().snapshot_stale);
    assert_blocked_from_now(&mut h);
    h.assert_clean();
}

#[test]
fn the_view_refresh_after_a_light_edit_stops_once_another_program_is_heard() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_light(h.light_intent(None, None, Some(1), None))
        .unwrap();
    let watch = h.watch();
    let at = heartbeat_in_view_refresh(&h, &watch);
    h.apply_light(&Harness::token(&preview.token), &watch)
        .unwrap();
    let start = at.lock().unwrap().expect("the view was refreshed");
    assert_eq!(h.packets() - start, 1, "the light modes, then nothing");
    assert!(h.status().snapshot_stale);
    assert_blocked_from_now(&mut h);
    h.assert_clean();
}

#[test]
fn profile_reports_alone_during_a_refresh_are_retried() {
    let mut h = Harness::ready();
    let base = h.packets();
    h.with_sim(|s| s.switch_profile_after_packets(base + 50, profile(2)));
    let snapshot = h.refresh().unwrap();
    assert_eq!(snapshot.profile, 2);
    assert_eq!(h.worker.conn, Conn::Ready);
    h.assert_clean();
}

/// Packet counts seen at the library's progress events.
#[derive(Debug, Default)]
struct Marks {
    /// When the library announced its first write.
    first_write: Option<usize>,
    /// At the library's last progress event.
    last_library: usize,
}

/// Injects the vendor site's heartbeat at the library's first write packet
/// and records the packet counts at the library's progress events.
fn heartbeat_at_first_write(h: &Harness, watch: &Arc<Watch>) -> Arc<Mutex<Marks>> {
    let marks = Arc::new(Mutex::new(Marks::default()));
    let sim = h.sim.clone();
    let heartbeat = sim.heartbeat();
    let recorded = Arc::clone(&marks);
    watch.on_event(move |event, _| {
        let library = matches!(
            event.step,
            ProgressStep::Writing { .. }
                | ProgressStep::WaitingForKeyboardSave { .. }
                | ProgressStep::CheckingPersistence { .. }
                | ProgressStep::FinalCheck
                | ProgressStep::ReturningToProfile { .. }
                | ProgressStep::SelectingProfile { .. }
        );
        if !library {
            return;
        }
        let packets = sim.with(0, |s| s.written().len());
        let mut marks = recorded.lock().unwrap();
        marks.last_library = packets;
        // Announced before the first write packet: the heartbeat arrives
        // with that packet's acknowledgement.
        if marks.first_write.is_none() && is_write_start(event) {
            marks.first_write = Some(packets);
            sim.with(0, |s| s.push_input(heartbeat.clone()));
        }
    });
    marks
}

/// No refresh after the library returned: no refresh step, no full read
/// (`0D` first), and after the library's last progress event at most its
/// own closing `21/11`.
fn assert_no_refresh(h: &Harness, watch: &Watch, from: usize, last_library: usize) {
    let steps = watch.steps();
    assert!(
        !steps.iter().any(|s| matches!(
            s,
            ProgressStep::RefreshingView
                | ProgressStep::Reading {
                    purpose: ReadPurpose::Snapshot,
                    ..
                }
        )),
        "{steps:?}"
    );
    let device_info = ReadRequest::DeviceInfo.packet();
    assert!(
        h.packets_since(from)
            .iter()
            .all(|p| p.as_bytes() != device_info.as_bytes()),
        "no full read after the other program was heard"
    );
    let active = ReadRequest::ActiveProfile.packet();
    let trailing = h.packets_since(last_library);
    assert!(
        trailing.len() <= 1 && trailing.iter().all(|p| p.as_bytes() == active.as_bytes()),
        "requests after the library returned: {trailing:?}"
    );
    assert!(
        matches!(h.worker.conn, Conn::OtherProgram { .. }),
        "{:?}",
        h.worker.conn
    );
    assert!(h.status().snapshot_stale);
}

#[test]
fn a_trigger_edit_sends_nothing_more_once_another_program_was_heard() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W, 47, 48], Some(60), None))
        .unwrap();
    let watch = h.watch();
    let marks = heartbeat_at_first_write(&h, &watch);
    let report = h
        .apply_trigger(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert!(report.foreign_activity.is_some(), "{report:#?}");
    let marks = marks.lock().unwrap();
    assert_no_refresh(&h, &watch, marks.first_write.unwrap(), marks.last_library);
    h.assert_clean();
}

#[test]
fn a_light_edit_sends_nothing_more_once_another_program_was_heard() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_light(h.light_intent(None, None, Some(1), None))
        .unwrap();
    let watch = h.watch();
    let marks = heartbeat_at_first_write(&h, &watch);
    let report = h
        .apply_light(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert!(report.foreign_activity.is_some(), "{report:#?}");
    let marks = marks.lock().unwrap();
    assert_no_refresh(&h, &watch, marks.first_write.unwrap(), marks.last_library);
    h.assert_clean();
}

#[test]
fn a_profile_switch_reads_nothing_once_another_program_was_heard() {
    let mut h = Harness::ready();
    let preview = h.prepare_switch(2).unwrap();
    let base = h.packets();
    let heartbeat = h.sim.heartbeat();
    // Right after the select (the switch's first packet).
    h.with_sim(|s| s.inject_after_packets(base + 1, heartbeat));
    let watch = h.watch();
    let report = h
        .apply_switch(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.to, 2);
    let sent = h.packets_since(base);
    assert!(sent.len() <= 3, "the select and its checks only: {sent:?}");
    let device_info = ReadRequest::DeviceInfo.packet();
    assert!(
        sent.iter().all(|p| p.as_bytes() != device_info.as_bytes()),
        "no full read of the new profile"
    );
    assert!(!watch.steps().iter().any(|s| matches!(
        s,
        ProgressStep::Reading {
            purpose: ReadPurpose::Snapshot,
            ..
        }
    )));
    assert!(matches!(h.worker.conn, Conn::OtherProgram { .. }));
    // The old profile's values are not shown as the new profile's.
    assert!(h.shared.snapshot().is_none());
    assert_eq!(h.status().active_profile, Some(2));
    h.assert_clean();
}

#[test]
fn a_restore_reads_nothing_more_once_another_program_was_heard() {
    let mut h = Harness::ready();
    let (file, _) = h.backup_all("all.owbackup.json");
    h.with_sim(|s| {
        s.set_trigger(profile(1), pos(W), FACTORY);
        s.set_table(
            profile(1),
            SimTable::MainLight,
            vec![0x00, 0x01, 0x02, 0x00, 0xff, 0x00, 0, 0, 0, 0, 0, 0],
        );
    });
    let intent = RestoreIntent {
        source: file,
        profiles: Some(vec![1]),
        allow_firmware_mismatch: false,
        confirmed_switches: vec![],
    };
    let preview = h.prepare_restore(intent, &h.watch()).unwrap();
    let watch = h.watch();
    let marks = heartbeat_at_first_write(&h, &watch);
    let report = h
        .apply_restore(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert!(report.foreign_activity.is_some(), "{report:#?}");
    let marks = marks.lock().unwrap();
    let steps = watch.steps();
    assert!(
        !steps.iter().any(|s| matches!(
            s,
            ProgressStep::Reading {
                purpose: ReadPurpose::Snapshot,
                ..
            }
        )),
        "{steps:?}"
    );
    let device_info = ReadRequest::DeviceInfo.packet();
    assert!(
        h.packets_since(marks.first_write.unwrap())
            .iter()
            .all(|p| p.as_bytes() != device_info.as_bytes()),
        "no full read after the other program was heard"
    );
    assert!(matches!(h.worker.conn, Conn::OtherProgram { .. }));
    let before = h.packets();
    h.ticks(30);
    assert_eq!(h.packets(), before, "only listening");
    h.assert_clean();
}
