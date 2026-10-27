//! The device worker: one thread that owns the keyboard's session and runs
//! every operation, one at a time.
//!
//! ```text
//! loop {
//!     wait for a request (or, in real time, at most the next tick)
//!     handle it                         (catch_unwind: a panic -> Fault)
//!     tick                              (listen 100 ms, timers, hot-plug)
//! }
//! ```
//!
//! While idle the worker sends nothing: it only listens (`Session::listen`)
//! or, without a keyboard, enumerates every 1.5 s.

mod conn;
mod idle;
mod pending;
mod publish;
pub(crate) mod request;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use openwraith_hid::Session;

pub(crate) use conn::{Conn, GATE};
pub(crate) use pending::{PendingPlan, PlanKind, stale};
pub(crate) use request::{Envelope, Request};

use crate::backend::{DeviceBackend, DeviceEntry};
use crate::clock::Clock;
use crate::config::CoreConfig;
use crate::dto::{AppError, DeviceId, DeviceSnapshot, OpInfo};
use crate::shared::Shared;

/// How long the worker listens per idle tick.
pub(crate) const IDLE_SLICE: Duration = Duration::from_millis(100);

/// The worker. See the [module documentation](self).
pub(crate) struct Worker<B: DeviceBackend> {
    pub(crate) backend: B,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) shared: Arc<Shared>,
    pub(crate) config: CoreConfig,
    pub(crate) sim: bool,
    /// Tests: the worker thread handles requests only (no idle ticks), so
    /// virtual time does not race ahead.
    pub(crate) requests_only: bool,
    rx: Receiver<Envelope>,
    pub(crate) conn: Conn,
    pub(crate) session: Option<Session<B::Transport>>,
    /// The opened keyboard.
    pub(crate) device: Option<DeviceEntry>,
    /// The keyboard used last (chosen again after a reconnect).
    pub(crate) preferred: Option<DeviceId>,
    /// The last enumeration.
    pub(crate) listed: Vec<DeviceEntry>,
    pub(crate) generation: u32,
    pub(crate) snapshot: Option<DeviceSnapshot>,
    pub(crate) snapshot_revision: u32,
    pub(crate) stale: bool,
    pub(crate) active_profile: Option<u8>,
    pub(crate) pending: Option<pending::Pending>,
    pub(crate) current: Option<OpInfo>,
    /// When the next enumeration is due.
    pub(crate) next_enum: Option<Instant>,
    /// When profile reports (and nothing else) were first seen while idle.
    pub(crate) profile_seen_at: Option<Instant>,
    /// The last time the worker talked to the keyboard (focus rule).
    pub(crate) last_traffic: Option<Instant>,
    /// Open attempts since the last success (backoff).
    pub(crate) open_attempts: u8,
    /// Read the settings again at the next tick.
    pub(crate) reread: bool,
    /// Send `Notice::Connected` after the next successful read.
    pub(crate) notify_connected: bool,
    /// Connect-read attempts that ended with a profile change.
    pub(crate) read_retries: u8,
    /// The request being handled passed admission and has not released it.
    pending_release: bool,
    /// The worker took the busy flag itself (a read of its own, such as
    /// `connect_read`) and has not cleared it yet. A panic clears the flag
    /// only when the worker owns it: a request admitted meanwhile keeps its
    /// admission.
    pub(crate) owns_busy: bool,
}

impl<B: DeviceBackend> Worker<B> {
    /// A worker over `backend`.
    pub(crate) fn new(
        backend: B,
        shared: Arc<Shared>,
        config: CoreConfig,
        rx: Receiver<Envelope>,
    ) -> Self {
        let clock = backend.clock();
        let sim = backend.is_simulated();
        Worker {
            backend,
            clock,
            shared,
            config,
            sim,
            requests_only: false,
            rx,
            conn: Conn::Starting,
            session: None,
            device: None,
            preferred: None,
            listed: Vec::new(),
            generation: 0,
            snapshot: None,
            snapshot_revision: 0,
            stale: false,
            active_profile: None,
            pending: None,
            current: None,
            next_enum: None,
            profile_seen_at: None,
            last_traffic: None,
            open_attempts: 0,
            reread: false,
            notify_connected: true,
            read_retries: 0,
            pending_release: false,
            owns_busy: false,
        }
    }

    /// Now, on the transport's clock while a keyboard is open (the
    /// simulator's virtual clock in tests), else the backend's.
    pub(crate) fn now(&self) -> Instant {
        match &self.session {
            Some(session) => openwraith_hid::Transport::now(session.transport()),
            None => self.clock.now(),
        }
    }

    /// The worker loop (see the [module documentation](self)).
    pub(crate) fn run(mut self) {
        self.publish();
        loop {
            let received = if self.requests_only {
                self.rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
            } else {
                self.rx.recv_timeout(self.next_wait())
            };
            match received {
                Ok(Envelope {
                    request: Request::Shutdown { reply },
                    ..
                }) => {
                    self.shutdown();
                    let _ = reply.send(());
                    return;
                }
                Ok(envelope) => self.handle_guarded(envelope),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.shutdown();
                    return;
                }
            }
            if !self.requests_only {
                self.tick_guarded();
            }
        }
    }

    /// How long the loop waits for a request before the next tick: not at
    /// all when the tick itself waits (listening, or the simulator's idle).
    fn next_wait(&self) -> Duration {
        if self.session.is_some() || self.backend.idle_waits() {
            Duration::ZERO
        } else {
            IDLE_SLICE
        }
    }

    /// Drops the session (closes the keyboard).
    fn shutdown(&mut self) {
        self.session = None;
    }

    /// Handles a request; a panic drops the session and stops all writes
    /// until the app restarts (`Fault`).
    pub(crate) fn handle_guarded(&mut self, envelope: Envelope) {
        self.pending_release = envelope.admitted;
        let result = catch_unwind(AssertUnwindSafe(|| self.handle(envelope.request)));
        if result.is_err() {
            self.fault("a request handler panicked");
            // A read the worker started itself (a focus check's re-read)
            // may have held the busy flag.
            self.release_own_busy();
        }
        // Requests without a reply (and a panic) release here.
        self.release();
        self.exit_if_requested();
    }

    /// Clears the busy flag when the worker took it itself.
    pub(crate) fn release_own_busy(&mut self) {
        if std::mem::take(&mut self.owns_busy) {
            self.shared.busy.store(false, Ordering::SeqCst);
        }
    }

    /// Exits the app when the user chose "Stop safely and close" and
    /// nothing runs any more. Called wherever an operation ends: after a
    /// request, after a read of the worker's own and after every tick.
    /// [`Shared::exit`] is idempotent.
    pub(crate) fn exit_if_requested(&self) {
        if self.shared.exit_after_op.load(Ordering::SeqCst)
            && !self.shared.busy.load(Ordering::SeqCst)
        {
            self.shared.exit();
        }
    }

    /// Ends an admitted request before its reply is sent: the busy flag is
    /// cleared and the status published, so that the next command, sent as
    /// soon as the reply arrives, is admitted. Only once per request (a
    /// request admitted meanwhile keeps its flag).
    fn release(&mut self) {
        if std::mem::take(&mut self.pending_release) {
            self.shared.busy.store(false, Ordering::SeqCst);
        }
        self.publish();
    }

    /// One tick, guarded like a request.
    pub(crate) fn tick_guarded(&mut self) {
        if catch_unwind(AssertUnwindSafe(|| self.tick())).is_err() {
            self.fault("the device worker panicked");
            // Only the flag the worker took itself: a request admitted
            // meanwhile (queued) keeps its admission.
            self.release_own_busy();
            self.publish();
        }
        self.exit_if_requested();
    }

    /// The fault state: the session is dropped (the keyboard closed), the
    /// running operation forgotten, writes blocked until a restart.
    fn fault(&mut self, message: &str) {
        self.session = None;
        self.current = None;
        self.pending = None;
        self.shared.end_op();
        self.conn = Conn::Fault {
            message: message.to_string(),
        };
    }

    /// Dispatches a request. First, what the session saw since the last
    /// tick decides the state (a reset, a lost connection or another
    /// program stops everything before anything is sent).
    fn handle(&mut self, request: Request) {
        self.after_op_state();
        match request {
            Request::Refresh { progress, reply } => {
                let result = self.refresh(progress);
                self.release();
                let _ = reply.send(result);
            }
            Request::SelectDevice { id, reply } => {
                let result = self.select_device(&id);
                self.release();
                let _ = reply.send(result);
            }
            Request::Reconnect { reply } => {
                let result = self.reconnect();
                self.release();
                let _ = reply.send(result);
            }
            Request::Disconnect { reply } => {
                let result = self.disconnect();
                self.release();
                let _ = reply.send(result);
            }
            Request::RecheckOtherProgram { reply } => {
                let result = self.recheck_other_program();
                self.release();
                let _ = reply.send(result);
            }
            Request::PrepareProfileSwitch { profile, reply } => {
                let result = self.prepare_profile_switch(profile);
                self.release();
                let _ = reply.send(result);
            }
            Request::ApplyProfileSwitch {
                token,
                progress,
                reply,
            } => {
                let result = self.apply_profile_switch(&token, progress);
                self.release();
                let _ = reply.send(result);
            }
            Request::PrepareTriggerEdit { intent, reply } => {
                let result = self.prepare_trigger_edit(&intent);
                self.release();
                let _ = reply.send(result);
            }
            Request::ApplyTriggerEdit {
                token,
                progress,
                reply,
            } => {
                let result = self.apply_trigger_edit(&token, progress);
                self.release();
                let _ = reply.send(result);
            }
            Request::PrepareLightEdit { intent, reply } => {
                let result = self.prepare_light_edit(&intent);
                self.release();
                let _ = reply.send(result);
            }
            Request::ApplyLightEdit {
                token,
                progress,
                reply,
            } => {
                let result = self.apply_light_edit(&token, progress);
                self.release();
                let _ = reply.send(result);
            }
            Request::BackupActiveProfile {
                destination,
                progress,
                reply,
            } => {
                let result = self.backup_active_profile(&destination, progress);
                self.release();
                let _ = reply.send(result);
            }
            Request::PrepareAllProfileBackup { destination, reply } => {
                let result = self.prepare_all_profile_backup(&destination);
                self.release();
                let _ = reply.send(result);
            }
            Request::ApplyAllProfileBackup {
                token,
                progress,
                reply,
            } => {
                let result = self.apply_all_profile_backup(&token, progress);
                self.release();
                let _ = reply.send(result);
            }
            Request::PrepareRestore {
                intent,
                progress,
                reply,
            } => {
                let result = self.prepare_restore(&intent, progress);
                self.release();
                let _ = reply.send(result);
            }
            Request::ApplyRestore {
                token,
                progress,
                reply,
            } => {
                let result = self.apply_restore(&token, progress);
                self.release();
                let _ = reply.send(result);
            }
            Request::DiscardPlan { token } => self.discard_plan(&token),
            Request::InvalidatePlan { reason } => self.invalidate(reason),
            Request::WindowFocused => self.window_focused(),
            Request::Shutdown { reply } => {
                self.shutdown();
                let _ = reply.send(());
            }
            #[cfg(test)]
            Request::Panic => {
                #[allow(clippy::panic)]
                {
                    panic!("injected panic");
                }
            }
        }
    }

    /// The error of a request that needs a connected keyboard.
    pub(crate) fn not_connected() -> AppError {
        AppError::new(
            crate::dto::AppErrorKind::NotConnected,
            "no keyboard is connected",
        )
    }
}
