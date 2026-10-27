//! Admission, close deferral and the journal, with the worker on its own
//! thread.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::harness::Harness;
use crate::dto::{AppErrorKind, CloseChoice, OpKind, RtIntent};
use crate::handle::WorkerHandle;
use crate::ops::journal::{self, JournalDevice, JournalEntry};
use crate::sink::NoProgress;

const W: u8 = 46;

/// Moves a ready harness's worker onto its own thread (requests only, no
/// ticks: virtual time does not race ahead).
fn threaded(h: Harness) -> (WorkerHandle, Harness2) {
    let Harness {
        mut worker,
        shared,
        sim,
        events,
        exits,
        dir,
        tx,
    } = h;
    let config = worker.config.clone();
    worker.requests_only = true;
    let join = std::thread::spawn(move || worker.run());
    let handle = WorkerHandle::from_parts(tx, Arc::clone(&shared), config, Some(join), Some(sim));
    (
        handle,
        Harness2 {
            _events: events,
            exits,
            _dir: dir,
        },
    )
}

/// What a test keeps of the harness.
struct Harness2 {
    _events: Arc<crate::sink::recorder::Recorder<crate::dto::AppEvent>>,
    exits: Arc<crate::sink::recorder::ExitCounter>,
    _dir: tempfile::TempDir,
}

fn wait_until(f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(10), "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_second_operation_is_refused_at_once_while_one_runs() {
    let h = Harness::ready();
    let (handle, _keep) = threaded(h);
    let snapshot = handle.snapshot().unwrap();
    let preview = handle
        .prepare_trigger_edit(crate::dto::TriggerEditIntent {
            keys: vec![W, 68],
            actuation: Some(60),
            rapid_trigger: Some(RtIntent::Off),
            based_on: snapshot.revision,
        })
        .unwrap();
    let token = preview.token.unwrap();
    let sim = handle.sim().unwrap().clone();
    sim.set_paused(true);
    let handle = Arc::new(handle);
    let apply = {
        let handle = Arc::clone(&handle);
        let token = token.clone();
        std::thread::spawn(move || handle.apply_trigger_edit(token, Arc::new(NoProgress)))
    };
    // The worker is now stuck at the first write packet.
    wait_until(|| sim.is_blocked());
    assert!(
        handle
            .status()
            .busy
            .as_ref()
            .is_some_and(|b| b.kind == OpKind::TriggerEdit)
    );
    let started = Instant::now();
    let error = handle.refresh(Arc::new(NoProgress)).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::OperationRunning);
    let error = handle
        .apply_trigger_edit(token.clone(), Arc::new(NoProgress))
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::OperationRunning);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "refused at once"
    );
    // Direct commands still answer.
    assert!(handle.status().busy.is_some());
    assert!(handle.snapshot().is_some());
    // The close is deferred while it runs.
    assert!(handle.close_requested());
    assert!(handle.status().close_requested);
    sim.set_paused(false);
    let report = apply.join().unwrap().unwrap();
    assert_eq!(report.items.len(), 2);
    assert_eq!(sim.with(0, |s| s.writes_sent()), 2, "as planned");
    // The same token again: stale.
    let error = handle
        .apply_trigger_edit(token, Arc::new(NoProgress))
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::StalePlan);
    wait_until(|| !handle.shared().busy.load(Ordering::SeqCst));
    assert!(!handle.close_requested(), "idle: the window may close");
    handle.answer_close(CloseChoice::KeepOpen);
    handle.shutdown();
    sim.with(0, |s| s.assert_clean());
}

#[test]
fn stop_and_close_finishes_safely_then_exits() {
    let h = Harness::ready();
    let (handle, keep) = threaded(h);
    let snapshot = handle.snapshot().unwrap();
    let preview = handle
        .prepare_trigger_edit(crate::dto::TriggerEditIntent {
            keys: vec![W, 68, 69],
            actuation: Some(60),
            rapid_trigger: None,
            based_on: snapshot.revision,
        })
        .unwrap();
    let sim = handle.sim().unwrap().clone();
    sim.set_paused(true);
    let handle = Arc::new(handle);
    let apply = {
        let handle = Arc::clone(&handle);
        let token = preview.token.unwrap();
        std::thread::spawn(move || handle.apply_trigger_edit(token, Arc::new(NoProgress)))
    };
    wait_until(|| sim.is_blocked());
    assert!(handle.close_requested());
    handle.answer_close(CloseChoice::StopAndClose);
    assert_eq!(keep.exits.0.load(Ordering::SeqCst), 0, "not while writing");
    sim.set_paused(false);
    let report = apply.join().unwrap().unwrap();
    assert_eq!(report.headline, crate::dto::Headline::Cancelled);
    // The key being written when Stop came was finished; the save was
    // waited for.
    assert!(report.waited_for_commit_ms.is_some());
    wait_until(|| keep.exits.0.load(Ordering::SeqCst) == 1);
    let packets = sim.with(0, |s| s.written().len());
    handle.shutdown();
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(
        sim.with(0, |s| s.written().len()),
        packets,
        "nothing after the shutdown"
    );
}

#[test]
fn a_journal_left_behind_shows_the_interrupted_operation() {
    let dir = tempfile::tempdir().unwrap();
    let config = super::harness::test_config(dir.path());
    let safety = dir.path().join("safety-file.owbackup.json");
    std::fs::write(&safety, "{}").unwrap();
    journal::write(
        &config.journal_path(),
        &JournalEntry {
            op: OpKind::Restore,
            started_at: "2026-10-24T07:30:00Z".into(),
            profiles: vec![2],
            device: JournalDevice {
                product: Some("W75HEARGB".into()),
                firmware: Some("3.22.07".into()),
            },
            safety_backup_path: safety.display().to_string(),
            app_version: "0.1.0".into(),
        },
    )
    .unwrap();
    let handle = WorkerHandle::spawn_sim(
        config,
        Arc::new(crate::sink::recorder::ExitCounter::default()),
        crate::backend::sim::SimOptions::virtual_time(0),
    )
    .unwrap();
    let interrupted = handle.status().interrupted.unwrap();
    assert_eq!(interrupted.kind, OpKind::Restore);
    assert_eq!(interrupted.profiles, vec![2]);
    assert_eq!(
        interrupted.safety_backup.unwrap().file_name,
        "safety-file.owbackup.json"
    );
    handle.dismiss_interrupted().unwrap();
    assert!(handle.status().interrupted.is_none());
    assert!(journal::read(&dir.path().join("state").join("operation-journal.json")).is_none());
    handle.shutdown();
}

#[test]
fn the_realtime_simulator_runs_the_worker_loop_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let config = super::harness::test_config(dir.path());
    let handle = WorkerHandle::spawn_sim(
        config,
        Arc::new(crate::sink::recorder::ExitCounter::default()),
        crate::backend::sim::SimOptions {
            pacing: crate::backend::sim::Pacing::RealTime,
            devices: 1,
            speed: 0.05,
        },
    )
    .unwrap();
    let events = Arc::new(crate::sink::recorder::Recorder::<crate::dto::AppEvent>::default());
    handle.subscribe(events.clone());
    wait_until(|| handle.status().connection == crate::dto::ConnectionState::Ready);
    let snapshot = handle.snapshot().unwrap();
    // Back to back: the next command is admitted as soon as a reply arrives.
    let preview = handle
        .prepare_trigger_edit(crate::dto::TriggerEditIntent {
            keys: vec![W],
            actuation: Some(70),
            rapid_trigger: None,
            based_on: snapshot.revision,
        })
        .unwrap();
    let progress =
        Arc::new(crate::sink::recorder::Recorder::<crate::dto::ProgressEvent>::default());
    let report = handle
        .apply_trigger_edit(preview.token.unwrap(), progress.clone())
        .unwrap();
    assert_eq!(report.headline, crate::dto::Headline::Done);
    assert!(progress.all().iter().any(|e| matches!(
        e.step,
        crate::dto::ProgressStep::WaitingForKeyboardSave { .. }
    )));
    assert!(
        events
            .all()
            .iter()
            .any(|e| matches!(e, crate::dto::AppEvent::Status { .. }))
    );
    let sim = handle.sim().unwrap().clone();
    assert_eq!(sim.info().unwrap().writes_sent, 1);
    assert_eq!(sim.info().unwrap().violations, 0);
    handle.shutdown();
}

#[test]
fn a_close_is_deferred_only_while_something_may_be_written() {
    let h = Harness::ready();
    let shared = Arc::clone(&h.shared);
    let config = h.worker.config.clone();
    let (tx, _rx) = std::sync::mpsc::channel();
    let handle = WorkerHandle::from_parts(tx, Arc::clone(&shared), config, None, None);
    assert!(!handle.close_requested(), "idle");
    shared.busy.store(true, Ordering::SeqCst);
    for (kind, deferred) in [
        (OpKind::Refresh, false),
        (OpKind::BackupActive, false),
        (OpKind::Restore, true),
        (OpKind::TriggerEdit, true),
        (OpKind::BackupAll, true),
    ] {
        let mut status = shared.status();
        status.busy = Some(crate::dto::OpInfo {
            op: crate::dto::OpId(format!("o-{kind:?}")),
            kind,
            last: None,
        });
        shared.publish(status, Instant::now());
        assert_eq!(handle.close_requested(), deferred, "{kind:?}");
    }
    assert!(handle.status().close_requested);
    handle.answer_close(CloseChoice::KeepOpen);
    assert!(!handle.status().close_requested);
}
