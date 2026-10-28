//! Coexistence with the CLI (design B9) on the simulator: the device lock
//! in a temporary folder (never the user's), the app's exclusive hold
//! around its writes, `PausedForCli` while the CLI holds the lock, and what
//! happens when it is released.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use openwraith_hid::DeviceLock;
use openwraith_hid::coordination::{LockState, TOOL_APP, TOOL_CLI};

use super::harness::Harness;
use crate::dto::{AppErrorKind, ErrorDetail, Notice, RtIntent, SimAction, WriteBlockReason};
use crate::shell::ForegroundProgram;
use crate::worker::{Conn, Request};

/// A ready keyboard whose app coordinates through a lock in a temporary
/// folder.
fn ready_with_lock() -> (Harness, PathBuf, tempfile::TempDir) {
    let folder = tempfile::tempdir().unwrap();
    let lock = folder.path().to_path_buf();
    let dir = lock.clone();
    let mut h = Harness::with_config(1, move |c| c.lock_dir = Some(dir));
    h.until_ready();
    (h, lock, folder)
}

fn paused_notices(h: &Harness) -> Vec<Option<String>> {
    h.notices()
        .into_iter()
        .filter_map(|n| match n {
            Notice::PausedForCli { purpose } => Some(purpose),
            _ => None,
        })
        .collect()
}

#[test]
fn the_app_pauses_while_the_cli_holds_the_lock_and_reads_again_after() {
    let (mut h, lock, _folder) = ready_with_lock();
    let cli = DeviceLock::try_exclusive(&lock, TOOL_CLI, "set trigger").unwrap();
    h.ticks(12);
    assert!(h.worker.paused_for_cli());
    let status = h.status();
    assert!(status.paused_for_cli);
    assert!(
        status
            .write_gate
            .reasons
            .contains(&WriteBlockReason::PausedForCli)
    );
    assert_eq!(paused_notices(&h), [Some("set trigger".to_string())]);
    // Listening only: the CLI's traffic is the CLI's, nothing is sent.
    let packets = h.packets();
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 3,
            interval_ms: 300,
        })
        .unwrap();
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 2,
            after_ms: 200,
            report: true,
        })
        .unwrap();
    h.ticks(30);
    h.post(Request::WindowFocused);
    let refused = h.refresh().unwrap_err();
    assert_eq!(refused.kind, AppErrorKind::WritesBlocked);
    assert!(matches!(
        refused.detail,
        Some(ErrorDetail::WritesBlocked { ref reasons }) if reasons == &[WriteBlockReason::PausedForCli]
    ));
    assert_eq!(h.packets(), packets, "nothing sent while paused");
    assert_eq!(h.worker.conn, Conn::Ready, "attributed to the CLI");
    // Released: re-armed, observed for 2 s, read again (profile 2 now).
    drop(cli);
    h.ticks(12);
    assert!(!h.worker.paused_for_cli());
    assert!(matches!(
        h.worker.conn,
        Conn::Observing | Conn::Reading { .. } | Conn::Ready
    ));
    h.until_ready();
    assert!(h.packets() > packets, "read again");
    assert_eq!(h.snapshot().profile, 2);
    assert!(!h.status().paused_for_cli);
    // The CLI's traffic does not count as another program for the
    // automatic profiles.
    assert_eq!(h.worker.auto_block(), None);
    h.assert_clean();
}

#[test]
fn every_write_holds_the_lock_exclusively_until_the_commit_settle() {
    let (mut h, lock, _folder) = ready_with_lock();
    assert_eq!(DeviceLock::probe(&lock).unwrap(), LockState::Free);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let watch = h.watch();
    {
        let seen = Arc::clone(&seen);
        let lock = lock.clone();
        watch.on_event(move |event, _| {
            if super::harness::is_write_start(event) {
                let state = DeviceLock::probe(&lock).unwrap();
                let holder = DeviceLock::holder(&lock).map(|i| (i.tool, i.purpose));
                // A CLI command now fails at once with the app's message.
                let cli = DeviceLock::try_exclusive(&lock, TOOL_CLI, "set trigger")
                    .err()
                    .map(|e| e.to_string());
                seen.lock().unwrap().push((state, holder, cli));
            }
        });
    }
    let preview = h
        .prepare_trigger(h.trigger_intent(&[46], Some(60), Some(RtIntent::Off)))
        .unwrap();
    h.apply_trigger(&Harness::token(&preview.token), &watch)
        .unwrap();
    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        [(
            LockState::Exclusive,
            Some((TOOL_APP.to_string(), "trigger edit".to_string())),
            Some("the OpenWraith app is writing; try again".to_string())
        )]
    );
    // Kept at most for the commit settle, then released (never held while
    // idle).
    h.ticks(25);
    assert_eq!(DeviceLock::probe(&lock).unwrap(), LockState::Free);
    assert!(!h.worker.paused_for_cli());
}

#[test]
fn a_write_while_the_cli_holds_the_lock_is_refused_before_anything_is_sent() {
    let (mut h, lock, _folder) = ready_with_lock();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[46], Some(60), Some(RtIntent::Off)))
        .unwrap();
    // The CLI took the lock after the plan was made, before any probe.
    let cli = DeviceLock::try_exclusive(&lock, TOOL_CLI, "restore").unwrap();
    let writes = h.writes();
    let safety = h.safety_files().len();
    let error = h
        .apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::WritesBlocked);
    assert_eq!(h.writes(), writes);
    assert_eq!(h.safety_files().len(), safety, "no safety backup either");
    assert!(h.worker.paused_for_cli());
    assert_eq!(paused_notices(&h), [Some("restore".to_string())]);
    drop(cli);
    h.ticks(12);
    h.until_ready();
    assert!(!h.worker.paused_for_cli());
}

#[test]
fn an_other_program_state_caused_by_the_cli_is_resolved_after_it() {
    let (mut h, lock, _folder) = ready_with_lock();
    // The CLI writes between two probes: hearing its traffic, the app
    // probes the lock at once and pauses (never `OtherProgram`), and what
    // it heard is not remembered as another program's activity.
    h.ticks(12);
    h.worker.next_probe = Some(h.worker.now() + std::time::Duration::from_secs(60));
    let cli = DeviceLock::try_exclusive(&lock, TOOL_CLI, "set trigger").unwrap();
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 10,
        })
        .unwrap();
    h.ticks(2);
    assert!(h.worker.paused_for_cli());
    assert!(!matches!(h.worker.conn, Conn::OtherProgram { .. }));
    drop(cli);
    h.ticks(12);
    h.until_ready();
    assert_eq!(h.worker.foreign_seen_at, None);
    assert_eq!(h.worker.auto_block(), None, "no 120 s wait after the CLI");
    // An `OtherProgram` from before the CLI stays.
    let (mut h, lock, _folder) = ready_with_lock();
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 10,
        })
        .unwrap();
    h.ticks(15);
    assert!(matches!(h.worker.conn, Conn::OtherProgram { .. }));
    let cli = DeviceLock::try_shared(&lock, TOOL_CLI, "info").unwrap();
    h.ticks(12);
    assert!(h.worker.paused_for_cli());
    drop(cli);
    h.ticks(12);
    assert!(!h.worker.paused_for_cli());
    assert!(
        matches!(h.worker.conn, Conn::OtherProgram { .. }),
        "the vendor site was heard before the CLI"
    );
}

#[test]
fn automatic_switches_wait_for_the_cli_and_take_the_lock() {
    let (mut h, lock, _folder) = ready_with_lock();
    let hd = crate::handle::WorkerHandle::from_parts(
        h.tx.clone(),
        Arc::clone(&h.shared),
        h.worker.config.clone(),
        None,
        None,
    );
    hd.set_auto_profile_rules(&crate::dto::AutoProfileRulesIntent {
        enabled: true,
        default_profile: None,
        rules: vec![crate::dto::AutoProfileRuleIntent {
            id: None,
            label: "Game".to_string(),
            program: crate::dto::RuleProgramIntent::Name {
                exe: "game.exe".to_string(),
            },
            profile: 2,
            enabled: true,
        }],
    })
    .unwrap();
    let cli = DeviceLock::try_exclusive(&lock, TOOL_CLI, "backup").unwrap();
    h.ticks(12);
    hd.foreground_changed(Some(ForegroundProgram {
        pid: 100,
        exe: Some("game.exe".to_string()),
        path: None,
        elevated: true,
    }));
    h.ticks(20);
    assert_eq!(
        h.status().automation.waiting.map(|w| w.reason),
        Some(crate::dto::AutoWaitReason::PausedForCli)
    );
    let writes = h.writes();
    drop(cli);
    assert!(h.until(|w| w.active_profile == Some(2), 80));
    assert_eq!(h.writes(), writes + 1);
    h.assert_clean();
}
