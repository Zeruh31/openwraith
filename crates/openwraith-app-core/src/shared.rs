//! State shared by the worker thread and the command handlers: the published
//! status and snapshot, the keymap group of the last read, the subscriber, the running operation's cancel flag,
//! the file registry, the close flags and the admission flag.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::auto::AutoCore;
use crate::clock::Clock;
use crate::dto::{
    AppEvent, AppInfo, AutomationStatusView, ConnectionState, DepthStopReason, DeviceSnapshot,
    DeviceStatus, FileRef, InterruptedOp, LastReport, Notice, OpId, ProgressEvent,
    RecoveryStatusView, TravelStateView, TravelStatusView, WriteGate,
};
use crate::files::{FileKind, FileRegistry};
use crate::ops::editors::EditorsCache;
use crate::ops::keymap::KeysCache;
use crate::ops::recovery::KnownGoodState;
use crate::shell::OsNotification;
use crate::sink::{EventSink, ExitHook, NotificationSink, ProgramSource, ShellHook, TraySink};
use crate::tokens::TokenMint;

/// Status updates while reading are sent at most this often.
const READING_PUBLISH_INTERVAL: Duration = Duration::from_millis(100);

/// The live key-depth view, from its admission on: its operation (once
/// the worker started it) and why it was asked to stop (the first reason
/// wins). A stop that arrives before the operation started is kept, and the
/// view then never opens the test.
struct DepthRun {
    op: Option<OpId>,
    reason: Option<DepthStopReason>,
}

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
    /// The keymap group of the last full read (the Keys page).
    keys: Mutex<Option<Arc<KeysCache>>>,
    /// The colours, dead band and SOCD of the last full read (patched after
    /// their edits and by SOCD pushes).
    editors: Mutex<Option<Arc<EditorsCache>>>,
    /// The running live key-depth view.
    depth: Mutex<Option<DepthRun>>,
    /// The marker of a live view of an earlier run was there at the start
    /// (until a close is acknowledged).
    travel_marker_left: AtomicBool,
    last_report: Mutex<Option<LastReport>>,
    subscriber: Mutex<Option<Arc<dyn EventSink>>>,
    current: Mutex<Option<CurrentOp>>,
    files: Mutex<FileRegistry>,
    mint: Mutex<TokenMint>,
    interrupted: Mutex<Option<InterruptedOp>>,
    /// Automatic backups and reset detection (also serializes the store's
    /// files).
    known_good: Mutex<KnownGoodState>,
    /// A notice for the first subscriber (the settings file was unreadable
    /// at start).
    start_notice: Mutex<Option<Notice>>,
    /// Applies the background settings to the shell (the Tauri glue).
    shell: Mutex<Option<Arc<dyn ShellHook>>>,
    /// The per-game automatic profiles, the platform feed, the tray and
    /// notification state.
    auto: Mutex<AutoCore>,
    /// The worker's clock (set when the worker is made): the time of the
    /// platform feed and of the notification throttle.
    clock: Mutex<Option<Arc<dyn Clock>>>,
    /// Lists the running programs (the Tauri glue).
    programs: Mutex<Option<Arc<dyn ProgramSource>>>,
    /// Shows OS notifications (the Tauri glue).
    notifications: Mutex<Option<Arc<dyn NotificationSink>>>,
    /// Shows the tray (the Tauri glue).
    tray: Mutex<Option<Arc<dyn TraySink>>>,
    /// A plan waits for the user's review (mirrored by the worker at every
    /// publish).
    pub(crate) plan_pending: AtomicBool,
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
                    travel: TravelStatusView {
                        state: TravelStateView::Closed,
                        stray_reports: 0,
                        marker_left: false,
                    },
                    recovery: RecoveryStatusView::default(),
                    automation: AutomationStatusView::default(),
                    paused_for_cli: false,
                },
                last_reading_publish: None,
            }),
            snapshot: Mutex::new(None),
            keys: Mutex::new(None),
            editors: Mutex::new(None),
            depth: Mutex::new(None),
            travel_marker_left: AtomicBool::new(false),
            last_report: Mutex::new(None),
            subscriber: Mutex::new(None),
            current: Mutex::new(None),
            files: Mutex::new(FileRegistry::default()),
            mint: Mutex::new(TokenMint::new(seed.map(|s| s.wrapping_add(1)))),
            interrupted: Mutex::new(None),
            known_good: Mutex::new(KnownGoodState::default()),
            start_notice: Mutex::new(None),
            shell: Mutex::new(None),
            auto: Mutex::new(AutoCore::default()),
            clock: Mutex::new(None),
            programs: Mutex::new(None),
            notifications: Mutex::new(None),
            tray: Mutex::new(None),
            plan_pending: AtomicBool::new(false),
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
        let recovery = self.recovery_status();
        let automation = self.automation_status();
        let mut status = lock(&self.status).status.clone();
        status.close_requested = self.close_requested.load(Ordering::SeqCst);
        status.interrupted = lock(&self.interrupted).clone();
        status.recovery = recovery;
        status.automation = automation;
        status
    }

    /// What the status shows about the automatic profiles.
    pub(crate) fn automation_status(&self) -> AutomationStatusView {
        lock(&self.auto).engine.status()
    }

    /// What the status shows about a suspected reset.
    pub(crate) fn recovery_status(&self) -> RecoveryStatusView {
        lock(&self.known_good).status()
    }

    /// Publishes `status` (its revision, close flag and interrupted
    /// operation are filled in here) when it differs from the last one.
    /// While reading, at most every 100 ms of `now`.
    pub(crate) fn publish(&self, mut status: DeviceStatus, now: Instant) {
        status.close_requested = self.close_requested.load(Ordering::SeqCst);
        status.interrupted = lock(&self.interrupted).clone();
        status.recovery = self.recovery_status();
        status.automation = self.automation_status();
        let event = {
            let mut cell = lock(&self.status);
            // The last progress event is kept from the previous status.
            if let (Some(new), Some(old)) = (&mut status.busy, &cell.status.busy)
                && new.op == old.op
            {
                new.last = old.last.clone();
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
                status: Box::new(status.clone()),
            }
        };
        self.send(event);
        self.update_tray(&status);
    }

    /// Publishes the stored status again (after the close flag changed).
    pub(crate) fn republish(&self) {
        let recovery = self.recovery_status();
        let automation = self.automation_status();
        let (event, status) = {
            let mut cell = lock(&self.status);
            cell.status.recovery = recovery;
            cell.status.automation = automation;
            cell.status.close_requested = self.close_requested.load(Ordering::SeqCst);
            cell.status.interrupted = lock(&self.interrupted).clone();
            cell.status.revision = cell.status.revision.wrapping_add(1);
            (
                AppEvent::Status {
                    status: Box::new(cell.status.clone()),
                },
                cell.status.clone(),
            )
        };
        self.send(event);
        self.update_tray(&status);
    }

    /// Keeps the last progress event in the status (without publishing), so
    /// that a reloaded window can show the progress panel.
    pub(crate) fn set_last_progress(&self, event: &ProgressEvent) {
        let mut cell = lock(&self.status);
        if let Some(busy) = &mut cell.status.busy
            && busy.op == event.op
        {
            busy.last = Some(event.clone());
        }
    }

    /// Sends a notice (and the OS notification it deserves, if any).
    pub(crate) fn notice(&self, notice: Notice) {
        let os = {
            let auto = lock(&self.auto);
            crate::auto::notify::for_notice(&notice, &auto.background.notifications)
        };
        self.send(AppEvent::Notice { notice });
        if let Some(os) = os {
            self.os_notify(os);
        }
    }

    /// Shows an OS notification through the glue's sink (at most one per
    /// kind every 10 minutes).
    pub(crate) fn os_notify(&self, notification: OsNotification) {
        let Some(sink) = lock(&self.notifications).clone() else {
            return;
        };
        let now = self.now();
        let admitted = lock(&self.auto).notifier.admit(&notification, now);
        if admitted {
            sink.show(&notification);
        }
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

    /// The keymap group of the last full read.
    pub(crate) fn keys(&self) -> Option<Arc<KeysCache>> {
        lock(&self.keys).clone()
    }

    /// Replaces the keymap group (with every full read; `None` with the
    /// snapshot).
    pub(crate) fn set_keys(&self, keys: Option<Arc<KeysCache>>) {
        *lock(&self.keys) = keys;
    }

    /// The colours, dead band and SOCD of the last full read.
    pub(crate) fn editors(&self) -> Option<Arc<EditorsCache>> {
        lock(&self.editors).clone()
    }

    /// Replaces them (with every full read; `None` with the snapshot).
    pub(crate) fn set_editors(&self, editors: Option<Arc<EditorsCache>>) {
        *lock(&self.editors) = editors;
    }

    /// Changes them in place (a copy is changed and stored); nothing
    /// without a cache.
    pub(crate) fn update_editors(&self, f: impl FnOnce(&mut EditorsCache)) {
        let mut slot = lock(&self.editors);
        if let Some(current) = slot.as_ref() {
            let mut next = EditorsCache::clone(current);
            f(&mut next);
            *slot = Some(Arc::new(next));
        }
    }

    // ----- the live key-depth view -----

    /// Registers an admitted live view before the worker handles it (a stop
    /// sent meanwhile is kept).
    pub(crate) fn reserve_depth(&self) {
        *lock(&self.depth) = Some(DepthRun {
            op: None,
            reason: None,
        });
    }

    /// Registers the running live view's operation; a stop that arrived
    /// since the admission is kept and cancels it at once.
    pub(crate) fn start_depth(&self, op: OpId) {
        let stopped = {
            let mut depth = lock(&self.depth);
            let run = depth.get_or_insert(DepthRun {
                op: None,
                reason: None,
            });
            run.op = Some(op.clone());
            run.reason.is_some()
        };
        if stopped {
            self.cancel(&op);
        }
    }

    /// Forgets the live view; returns why it was asked to stop, if it was.
    pub(crate) fn end_depth(&self) -> Option<DepthStopReason> {
        lock(&self.depth).take().and_then(|run| run.reason)
    }

    /// Why the live view was asked to stop, if it was.
    pub(crate) fn depth_stop_reason(&self) -> Option<DepthStopReason> {
        lock(&self.depth).as_ref().and_then(|run| run.reason)
    }

    /// Asks the live view to stop (the first reason is kept), also before
    /// the worker started it; false when none was admitted.
    pub(crate) fn stop_depth(&self, reason: DepthStopReason) -> bool {
        let op = {
            let mut depth = lock(&self.depth);
            let Some(run) = depth.as_mut() else {
                return false;
            };
            run.reason.get_or_insert(reason);
            run.op.clone()
        };
        if let Some(op) = op {
            self.cancel(&op);
        }
        true
    }

    /// Whether the marker of an earlier run's live view was found.
    pub(crate) fn travel_marker_left(&self) -> bool {
        self.travel_marker_left.load(Ordering::SeqCst)
    }

    /// Records whether the marker of an earlier run's live view is there.
    pub(crate) fn set_travel_marker_left(&self, left: bool) {
        self.travel_marker_left.store(left, Ordering::SeqCst);
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

    // ----- automatic backups, start notice, shell -----

    /// The known-good state (the lock also serializes the store's files).
    pub(crate) fn known_good(&self) -> MutexGuard<'_, KnownGoodState> {
        lock(&self.known_good)
    }

    /// Keeps a notice for the first subscriber.
    pub(crate) fn set_start_notice(&self, notice: Option<Notice>) {
        *lock(&self.start_notice) = notice;
    }

    /// The notice kept for the first subscriber (once).
    pub(crate) fn take_start_notice(&self) -> Option<Notice> {
        lock(&self.start_notice).take()
    }

    /// Registers the shell hook.
    pub(crate) fn set_shell(&self, hook: Arc<dyn ShellHook>) {
        *lock(&self.shell) = Some(hook);
    }

    /// The shell hook, if registered.
    pub(crate) fn shell(&self) -> Option<Arc<dyn ShellHook>> {
        lock(&self.shell).clone()
    }

    // ----- automatic profiles, platform feed, tray, notifications -----

    /// The automatic profiles' state.
    pub(crate) fn auto(&self) -> MutexGuard<'_, AutoCore> {
        lock(&self.auto)
    }

    /// Registers the worker's clock.
    pub(crate) fn set_clock(&self, clock: Arc<dyn Clock>) {
        *lock(&self.clock) = Some(clock);
    }

    /// Now, on the worker's clock (the system clock before a worker was
    /// made).
    pub(crate) fn now(&self) -> Instant {
        let clock = lock(&self.clock).clone();
        clock.map_or_else(Instant::now, |c| c.now())
    }

    /// Registers the running-programs source.
    pub(crate) fn set_programs(&self, source: Arc<dyn ProgramSource>) {
        *lock(&self.programs) = Some(source);
    }

    /// The running-programs source, if registered.
    pub(crate) fn programs(&self) -> Option<Arc<dyn ProgramSource>> {
        lock(&self.programs).clone()
    }

    /// Registers the notification sink.
    pub(crate) fn set_notifications(&self, sink: Arc<dyn NotificationSink>) {
        *lock(&self.notifications) = Some(sink);
    }

    /// Registers the tray sink and shows the current model at once.
    pub(crate) fn set_tray(&self, sink: Arc<dyn TraySink>) {
        *lock(&self.tray) = Some(sink);
        lock(&self.auto).last_tray = None;
        let status = self.status();
        self.update_tray(&status);
    }

    /// The tray model of `status`.
    pub(crate) fn tray_model(&self, status: &DeviceStatus) -> crate::shell::TrayModel {
        let product = status.device.as_ref().and_then(|d| d.product.clone());
        let (names, auto_backup) = {
            let mut auto = lock(&self.auto);
            (
                auto.names_for(product.as_deref()),
                auto.background.auto_backup.enabled,
            )
        };
        crate::auto::tray::model(status, &names, auto_backup)
    }

    /// Shows the tray model of `status` when it changed.
    fn update_tray(&self, status: &DeviceStatus) {
        let Some(sink) = lock(&self.tray).clone() else {
            return;
        };
        let model = self.tray_model(status);
        {
            let mut auto = lock(&self.auto);
            if auto.last_tray.as_ref() == Some(&model) {
                return;
            }
            auto.last_tray = Some(model.clone());
        }
        sink.update(&model);
    }

    /// Whether a live key-depth view was admitted or runs.
    pub(crate) fn depth_active(&self) -> bool {
        lock(&self.depth).is_some()
    }
}
