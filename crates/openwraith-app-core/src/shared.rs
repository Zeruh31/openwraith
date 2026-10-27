//! State shared by the worker thread and the command handlers: the published
//! status and snapshot, the subscriber, the running operation's cancel flag,
//! the file registry, the close flags and the admission flag.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::dto::{
    AppEvent, AppInfo, ConnectionState, DeviceSnapshot, DeviceStatus, FileRef, InterruptedOp,
    LastReport, Notice, OpId, ProgressEvent, WriteGate,
};
use crate::files::{FileKind, FileRegistry};
use crate::sink::{EventSink, ExitHook};
use crate::tokens::TokenMint;

/// Status updates while reading are sent at most this often.
const READING_PUBLISH_INTERVAL: Duration = Duration::from_millis(100);

/// The running operation's cancellation.
struct CurrentOp {
    op: OpId,
    cancel: Arc<AtomicBool>,
}

struct StatusCell {
    status: DeviceStatus,
    last_reading_publish: Option<Instant>,
}

/// Locks a mutex, also after a panic elsewhere (the data stays usable: every
/// update is a plain replacement).
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// See the [module documentation](self).
pub(crate) struct Shared {
    /// Admission: set while an operation is queued or running.
    pub(crate) busy: AtomicBool,
    /// The user asked to close while an operation runs.
    pub(crate) close_requested: AtomicBool,
    /// Exit once the running operation is over.
    pub(crate) exit_after_op: AtomicBool,
    /// The exit hook ran (it runs once).
    exited: AtomicBool,
    status: Mutex<StatusCell>,
    snapshot: Mutex<Option<DeviceSnapshot>>,
    last_report: Mutex<Option<LastReport>>,
    subscriber: Mutex<Option<Arc<dyn EventSink>>>,
    current: Mutex<Option<CurrentOp>>,
    files: Mutex<FileRegistry>,
    mint: Mutex<TokenMint>,
    interrupted: Mutex<Option<InterruptedOp>>,
    exit: Arc<dyn ExitHook>,
    app_info: AppInfo,
}

impl Shared {
    /// The shared state at start.
    pub(crate) fn new(app_info: AppInfo, exit: Arc<dyn ExitHook>, seed: Option<u64>) -> Self {
        let sim = app_info.sim;
        Shared {
            busy: AtomicBool::new(false),
            close_requested: AtomicBool::new(false),
            exit_after_op: AtomicBool::new(false),
            exited: AtomicBool::new(false),
            status: Mutex::new(StatusCell {
                status: DeviceStatus {
                    revision: 0,
                    generation: 0,
                    connection: ConnectionState::Starting,
                    device: None,
                    active_profile: None,
                    firmware_tested: None,
                    write_gate: WriteGate {
                        allowed: false,
                        reasons: vec![crate::dto::WriteBlockReason::NotConnected],
                    },
                    busy: None,
                    snapshot_revision: None,
                    snapshot_stale: false,
                    close_requested: false,
                    interrupted: None,
                    sim,
                },
                last_reading_publish: None,
            }),
            snapshot: Mutex::new(None),
            last_report: Mutex::new(None),
            subscriber: Mutex::new(None),
            current: Mutex::new(None),
            files: Mutex::new(FileRegistry::default()),
            mint: Mutex::new(TokenMint::new(seed.map(|s| s.wrapping_add(1)))),
            interrupted: Mutex::new(None),
            exit,
            app_info,
        }
    }

    /// Facts about this build.
    pub(crate) fn app_info(&self) -> &AppInfo {
        &self.app_info
    }

    // ----- status and events -----

    /// The current status.
    pub(crate) fn status(&self) -> DeviceStatus {
        let mut status = lock(&self.status).status.clone();
        status.close_requested = self.close_requested.load(Ordering::SeqCst);
        status.interrupted = lock(&self.interrupted).clone();
        status
    }

    /// Publishes `status` (its revision, close flag and interrupted
    /// operation are filled in here) when it differs from the last one.
    /// While reading, at most every 100 ms of `now`.
    pub(crate) fn publish(&self, mut status: DeviceStatus, now: Instant) {
        status.close_requested = self.close_requested.load(Ordering::SeqCst);
        status.interrupted = lock(&self.interrupted).clone();
        let event = {
            let mut cell = lock(&self.status);
            // The last progress event is kept from the previous status.
            if let (Some(new), Some(old)) = (&mut status.busy, &cell.status.busy) {
                if new.op == old.op {
                    new.last = old.last.clone();
                }
            }
            status.revision = cell.status.revision;
            if status == cell.status {
                return;
            }
            if matches!(status.connection, ConnectionState::Reading { .. })
                && matches!(cell.status.connection, ConnectionState::Reading { .. })
                && cell
                    .last_reading_publish
                    .is_some_and(|t| now.saturating_duration_since(t) < READING_PUBLISH_INTERVAL)
            {
                return;
            }
            if matches!(status.connection, ConnectionState::Reading { .. }) {
                cell.last_reading_publish = Some(now);
            }
            status.revision = cell.status.revision.wrapping_add(1);
            cell.status = status.clone();
            AppEvent::Status {
                status: Box::new(status),
            }
        };
        self.send(event);
    }

    /// Publishes the stored status again (after the close flag changed).
    pub(crate) fn republish(&self) {
        let event = {
            let mut cell = lock(&self.status);
            cell.status.close_requested = self.close_requested.load(Ordering::SeqCst);
            cell.status.interrupted = lock(&self.interrupted).clone();
            cell.status.revision = cell.status.revision.wrapping_add(1);
            AppEvent::Status {
                status: Box::new(cell.status.clone()),
            }
        };
        self.send(event);
    }

    /// Keeps the last progress event in the status (without publishing), so
    /// that a reloaded window can show the progress panel.
    pub(crate) fn set_last_progress(&self, event: &ProgressEvent) {
        let mut cell = lock(&self.status);
        if let Some(busy) = &mut cell.status.busy {
            if busy.op == event.op {
                busy.last = Some(event.clone());
            }
        }
    }

    /// Sends a notice.
    pub(crate) fn notice(&self, notice: Notice) {
        self.send(AppEvent::Notice { notice });
    }

    fn send(&self, event: AppEvent) {
        let sink = lock(&self.subscriber).clone();
        if let Some(sink) = sink {
            sink.send(event);
        }
    }

    /// Replaces the subscriber.
    pub(crate) fn set_subscriber(&self, sink: Arc<dyn EventSink>) {
        *lock(&self.subscriber) = Some(sink);
    }

    // ----- snapshot and report -----

    /// The current snapshot.
    pub(crate) fn snapshot(&self) -> Option<DeviceSnapshot> {
        lock(&self.snapshot).clone()
    }

    /// Replaces the snapshot.
    pub(crate) fn set_snapshot(&self, snapshot: Option<DeviceSnapshot>) {
        *lock(&self.snapshot) = snapshot;
    }

    /// The last report.
    pub(crate) fn last_report(&self) -> Option<LastReport> {
        lock(&self.last_report).clone()
    }

    /// Replaces the last report.
    pub(crate) fn set_last_report(&self, report: LastReport) {
        *lock(&self.last_report) = Some(report);
    }

    // ----- operations -----

    /// Registers the running operation's cancel flag.
    pub(crate) fn start_op(&self, op: OpId) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        *lock(&self.current) = Some(CurrentOp {
            op,
            cancel: Arc::clone(&cancel),
        });
        cancel
    }

    /// Forgets the running operation.
    pub(crate) fn end_op(&self) {
        *lock(&self.current) = None;
    }

    /// Asks the running operation `op` to stop; false when it is not the
    /// running one.
    pub(crate) fn cancel(&self, op: &OpId) -> bool {
        match &*lock(&self.current) {
            Some(current) if current.op == *op => {
                current.cancel.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }

    /// Asks whatever runs to stop.
    pub(crate) fn cancel_any(&self) {
        if let Some(current) = &*lock(&self.current) {
            current.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Exits the app (once: the worker and `answer_close` may both see
    /// that nothing runs any more).
    pub(crate) fn exit(&self) {
        if !self.exited.swap(true, Ordering::SeqCst) {
            self.exit.exit();
        }
    }

    // ----- files and tokens -----

    /// Registers a file.
    pub(crate) fn register_file(&self, path: PathBuf, kind: FileKind) -> FileRef {
        let mut mint = lock(&self.mint);
        lock(&self.files).register(&mut mint, path, kind)
    }

    /// The file registry.
    pub(crate) fn files(&self) -> MutexGuard<'_, FileRegistry> {
        lock(&self.files)
    }

    /// The token mint.
    pub(crate) fn mint(&self) -> MutexGuard<'_, TokenMint> {
        lock(&self.mint)
    }

    // ----- the interrupted operation -----

    /// Sets or clears the interrupted operation.
    pub(crate) fn set_interrupted(&self, op: Option<InterruptedOp>) {
        *lock(&self.interrupted) = op;
    }
}
