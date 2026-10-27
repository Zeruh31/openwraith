//! A worker on the simulator with virtual time, driven synchronously.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use openwraith_hid::protocol::{KeyPos, ProfileId, REPORT_LEN};
use openwraith_hid::{DeviceSim, SimPacket};
use tempfile::TempDir;

use crate::backend::sim::{SimControl, SimDeviceBackend, SimOptions, make_devices};
use crate::config::CoreConfig;
use crate::dto::{
    AllProfilesPreview, AppError, AppEvent, BackupSaved, DeviceSnapshot, DeviceStatus,
    EditReportView, FileRef, FileToken, LightEditIntent, LightEditPreview, Notice, PlanToken,
    ProfileSwitchPreview, ProfileSwitchReport, ProgressEvent, ProgressStep, RestoreIntent,
    RestorePreview, RestoreReportView, RtIntent, TriggerEditIntent, TriggerEditPreview,
};
use crate::files::FileKind;
use crate::handle::app_info;
use crate::shared::Shared;
use crate::sink::ProgressSink;
use crate::sink::recorder::{ExitCounter, Recorder};
use crate::worker::request::Reply;
use crate::worker::{Conn, Envelope, Request, Worker};

/// 2026-10-24T07:30:00Z.
pub(crate) fn fixed_time() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_792_827_000)
}

/// The configuration of a test in `dir`.
pub(crate) fn test_config(dir: &std::path::Path) -> CoreConfig {
    CoreConfig {
        safety_dir: Ok(dir.join("safety")),
        state_dir: dir.join("state"),
        version: "0.1.0".to_string(),
        os_locale: Some("en-US".to_string()),
        repository: "https://github.com/Zeruh31/openwraith".to_string(),
        fixed_time: Some(fixed_time()),
        token_seed: Some(42),
        legal_dir: Some(dir.join("legal")),
    }
}

/// The key at index `i`.
pub(crate) fn pos(i: u8) -> KeyPos {
    KeyPos::from_index(i).unwrap()
}

/// Profile `n`.
pub(crate) fn profile(n: u8) -> ProfileId {
    ProfileId::new(n).unwrap()
}

/// A hook on progress events.
type OnEvent = Box<dyn FnMut(&ProgressEvent, &Shared) + Send>;

/// A progress sink that records every event and, at the first write
/// packet, whether the journal file existed.
pub(crate) struct Watch {
    pub(crate) events: Mutex<Vec<ProgressEvent>>,
    journal: PathBuf,
    pub(crate) journal_seen: Mutex<Option<bool>>,
    /// Runs on every event (cancel the operation, unplug, ...).
    on_event: Mutex<Option<OnEvent>>,
    shared: Arc<Shared>,
}

impl Watch {
    pub(crate) fn steps(&self) -> Vec<ProgressStep> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.step.clone())
            .collect()
    }

    pub(crate) fn all(&self) -> Vec<ProgressEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Cancels the operation at the first event matching `f`.
    pub(crate) fn cancel_when(&self, f: impl Fn(&ProgressEvent) -> bool + Send + 'static) {
        self.on_event(move |event, shared| {
            if f(event) {
                shared.cancel(&event.op);
            }
        });
    }

    /// Runs `f` on every event.
    pub(crate) fn on_event(&self, f: impl FnMut(&ProgressEvent, &Shared) + Send + 'static) {
        *self.on_event.lock().unwrap() = Some(Box::new(f));
    }
}

impl ProgressSink for Watch {
    fn send(&self, event: ProgressEvent) {
        if matches!(event.step, ProgressStep::Writing { .. }) {
            let mut seen = self.journal_seen.lock().unwrap();
            if seen.is_none() {
                *seen = Some(self.journal.exists());
            }
        }
        if let Some(f) = &mut *self.on_event.lock().unwrap() {
            f(&event, &self.shared);
        }
        self.events.lock().unwrap().push(event);
    }
}

/// A worker on the simulator, driven by the test.
pub(crate) struct Harness {
    pub(crate) worker: Worker<SimDeviceBackend>,
    pub(crate) shared: Arc<Shared>,
    pub(crate) sim: SimControl,
    pub(crate) events: Arc<Recorder<AppEvent>>,
    pub(crate) exits: Arc<ExitCounter>,
    pub(crate) dir: TempDir,
    pub(crate) tx: Sender<Envelope>,
}

impl Harness {
    /// `devices` simulated keyboards, nothing opened yet.
    pub(crate) fn new(devices: u8) -> Self {
        Self::with_config(devices, |_| {})
    }

    /// With a changed configuration.
    pub(crate) fn with_config(devices: u8, change: impl FnOnce(&mut CoreConfig)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = test_config(dir.path());
        change(&mut config);
        let list = make_devices(devices).unwrap();
        let sim = SimControl::new(Arc::clone(&list));
        let backend = SimDeviceBackend::new(list, &SimOptions::virtual_time(devices), Some(42));
        let exits = Arc::new(ExitCounter::default());
        let shared = Arc::new(Shared::new(
            app_info(&config, true),
            exits.clone(),
            Some(42),
        ));
        let events = Arc::new(Recorder::default());
        shared.set_subscriber(events.clone());
        let (tx, rx) = mpsc::channel();
        let worker = Worker::new(backend, Arc::clone(&shared), config, rx);
        Harness {
            worker,
            shared,
            sim,
            events,
            exits,
            dir,
            tx,
        }
    }

    /// One keyboard, observed and read: `Ready`.
    pub(crate) fn ready() -> Self {
        let mut h = Self::new(1);
        h.until_ready();
        h
    }

    /// Ticks until `Ready` (panics after 10 s of virtual time).
    pub(crate) fn until_ready(&mut self) {
        assert!(
            self.until(|w| w.conn == Conn::Ready, 100),
            "not ready: {:?}",
            self.worker.conn
        );
    }

    /// One tick.
    pub(crate) fn tick(&mut self) {
        self.worker.tick_guarded();
    }

    /// `n` ticks.
    pub(crate) fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.tick();
        }
    }

    /// Ticks until `pred` holds, at most `max` ticks.
    pub(crate) fn until(
        &mut self,
        pred: impl Fn(&Worker<SimDeviceBackend>) -> bool,
        max: usize,
    ) -> bool {
        for _ in 0..max {
            if pred(&self.worker) {
                return true;
            }
            self.tick();
        }
        pred(&self.worker)
    }

    /// Runs a request as an admitted operation and returns its reply.
    pub(crate) fn op<T>(&mut self, make: impl FnOnce(Reply<T>) -> Request) -> Result<T, AppError> {
        assert!(
            self.shared
                .busy
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "busy"
        );
        let (reply, answer) = mpsc::sync_channel(1);
        self.worker.handle_guarded(Envelope {
            request: make(reply),
            admitted: true,
        });
        answer
            .try_recv()
            .unwrap_or_else(|_| Err(AppError::internal("no reply")))
    }

    /// Sends a queued request (no admission).
    pub(crate) fn post(&mut self, request: Request) {
        self.worker.handle_guarded(Envelope {
            request,
            admitted: false,
        });
    }

    /// A progress watcher.
    pub(crate) fn watch(&self) -> Arc<Watch> {
        Arc::new(Watch {
            events: Mutex::new(Vec::new()),
            journal: self.worker.config.journal_path(),
            journal_seen: Mutex::new(None),
            on_event: Mutex::new(None),
            shared: Arc::clone(&self.shared),
        })
    }

    /// Runs `f` on the first simulated keyboard.
    pub(crate) fn with_sim<R>(&self, f: impl FnOnce(&mut DeviceSim) -> R) -> R {
        self.sim.with(0, f)
    }

    /// Packets the first keyboard received.
    pub(crate) fn packets(&self) -> usize {
        self.with_sim(|s| s.written().len())
    }

    /// Write packets the first keyboard received.
    pub(crate) fn writes(&self) -> usize {
        self.with_sim(|s| s.writes_sent())
    }

    /// The write packets the first keyboard received.
    pub(crate) fn write_bytes(&self) -> Vec<[u8; REPORT_LEN]> {
        self.with_sim(|s| s.write_packets().map(|p| *p.as_bytes()).collect())
    }

    /// The packets the first keyboard received after the first `from`.
    pub(crate) fn packets_since(&self, from: usize) -> Vec<SimPacket> {
        self.with_sim(|s| s.written().get(from..).unwrap_or_default().to_vec())
    }

    /// The snapshot.
    pub(crate) fn snapshot(&self) -> DeviceSnapshot {
        self.shared.snapshot().expect("a snapshot")
    }

    /// The published status.
    pub(crate) fn status(&self) -> DeviceStatus {
        self.shared.status()
    }

    /// The notices sent so far.
    pub(crate) fn notices(&self) -> Vec<Notice> {
        self.events
            .all()
            .into_iter()
            .filter_map(|e| match e {
                AppEvent::Notice { notice } => Some(notice),
                AppEvent::Status { .. } => None,
            })
            .collect()
    }

    /// The safety backups saved so far.
    pub(crate) fn safety_files(&self) -> Vec<PathBuf> {
        match std::fs::read_dir(self.dir.path().join("safety")) {
            Ok(entries) => entries.flatten().map(|e| e.path()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// A destination file in the test directory, registered.
    pub(crate) fn destination(&self, name: &str) -> FileRef {
        self.shared
            .register_file(self.dir.path().join(name), FileKind::Destination)
    }

    /// A user file, registered.
    pub(crate) fn user_file(&self, path: PathBuf) -> FileToken {
        self.shared.register_file(path, FileKind::User).token
    }

    /// Virtual time passes while the worker listens (one big slice).
    pub(crate) fn advance(&mut self, duration: Duration) {
        if let Some(session) = self.worker.session.as_mut() {
            let _ = session.listen(duration);
        }
    }

    /// No refused packet, nothing sent after a reset.
    pub(crate) fn assert_clean(&self) {
        self.with_sim(|s| s.assert_clean());
        assert_eq!(self.sim.post_reset_packets(), 0, "packets after a reset");
    }

    /// A cloned plan token.
    pub(crate) fn token(token: &Option<PlanToken>) -> PlanToken {
        token.clone().expect("a token")
    }
}

/// The operations, as the commands call them.
impl Harness {
    /// A trigger intent against the current snapshot.
    pub(crate) fn trigger_intent(
        &self,
        keys: &[u8],
        actuation: Option<u16>,
        rapid_trigger: Option<RtIntent>,
    ) -> TriggerEditIntent {
        TriggerEditIntent {
            keys: keys.to_vec(),
            actuation,
            rapid_trigger,
            based_on: self.snapshot().revision,
        }
    }

    /// A light intent against the current snapshot.
    pub(crate) fn light_intent(
        &self,
        mode: Option<u8>,
        color: Option<&str>,
        brightness: Option<u8>,
        speed: Option<u8>,
    ) -> LightEditIntent {
        LightEditIntent {
            mode,
            color: color.map(str::to_string),
            brightness,
            speed,
            based_on: self.snapshot().revision,
        }
    }

    pub(crate) fn refresh(&mut self) -> Result<DeviceSnapshot, AppError> {
        let progress = self.watch();
        self.op(|reply| Request::Refresh { progress, reply })
    }

    pub(crate) fn prepare_trigger(
        &mut self,
        intent: TriggerEditIntent,
    ) -> Result<TriggerEditPreview, AppError> {
        self.op(|reply| Request::PrepareTriggerEdit { intent, reply })
    }

    pub(crate) fn apply_trigger(
        &mut self,
        token: &PlanToken,
        watch: &Arc<Watch>,
    ) -> Result<EditReportView, AppError> {
        let token = token.clone();
        let progress: Arc<dyn ProgressSink> = watch.clone();
        self.op(|reply| Request::ApplyTriggerEdit {
            token,
            progress,
            reply,
        })
    }

    pub(crate) fn prepare_light(
        &mut self,
        intent: LightEditIntent,
    ) -> Result<LightEditPreview, AppError> {
        self.op(|reply| Request::PrepareLightEdit { intent, reply })
    }

    pub(crate) fn apply_light(
        &mut self,
        token: &PlanToken,
        watch: &Arc<Watch>,
    ) -> Result<EditReportView, AppError> {
        let token = token.clone();
        let progress: Arc<dyn ProgressSink> = watch.clone();
        self.op(|reply| Request::ApplyLightEdit {
            token,
            progress,
            reply,
        })
    }

    pub(crate) fn prepare_switch(&mut self, profile: u8) -> Result<ProfileSwitchPreview, AppError> {
        self.op(|reply| Request::PrepareProfileSwitch { profile, reply })
    }

    pub(crate) fn apply_switch(
        &mut self,
        token: &PlanToken,
        watch: &Arc<Watch>,
    ) -> Result<ProfileSwitchReport, AppError> {
        let token = token.clone();
        let progress: Arc<dyn ProgressSink> = watch.clone();
        self.op(|reply| Request::ApplyProfileSwitch {
            token,
            progress,
            reply,
        })
    }

    pub(crate) fn backup_active(
        &mut self,
        destination: &FileRef,
        watch: &Arc<Watch>,
    ) -> Result<BackupSaved, AppError> {
        let destination = destination.token.clone();
        let progress: Arc<dyn ProgressSink> = watch.clone();
        self.op(|reply| Request::BackupActiveProfile {
            destination,
            progress,
            reply,
        })
    }

    pub(crate) fn prepare_all(
        &mut self,
        destination: &FileRef,
    ) -> Result<AllProfilesPreview, AppError> {
        let destination = destination.token.clone();
        self.op(|reply| Request::PrepareAllProfileBackup { destination, reply })
    }

    pub(crate) fn apply_all(
        &mut self,
        token: &PlanToken,
        watch: &Arc<Watch>,
    ) -> Result<BackupSaved, AppError> {
        let token = token.clone();
        let progress: Arc<dyn ProgressSink> = watch.clone();
        self.op(|reply| Request::ApplyAllProfileBackup {
            token,
            progress,
            reply,
        })
    }

    /// An all-profile backup to `name` in the test directory.
    pub(crate) fn backup_all(&mut self, name: &str) -> (FileToken, PathBuf) {
        let destination = self.destination(name);
        let preview = self.prepare_all(&destination).unwrap();
        let watch = self.watch();
        self.apply_all(&preview.token, &watch).unwrap();
        let path = self.dir.path().join(name);
        (self.user_file(path.clone()), path)
    }

    pub(crate) fn prepare_restore(
        &mut self,
        intent: RestoreIntent,
        watch: &Arc<Watch>,
    ) -> Result<RestorePreview, AppError> {
        let progress: Arc<dyn ProgressSink> = watch.clone();
        self.op(|reply| Request::PrepareRestore {
            intent,
            progress,
            reply,
        })
    }

    pub(crate) fn apply_restore(
        &mut self,
        token: &PlanToken,
        watch: &Arc<Watch>,
    ) -> Result<RestoreReportView, AppError> {
        let token = token.clone();
        let progress: Arc<dyn ProgressSink> = watch.clone();
        self.op(|reply| Request::ApplyRestore {
            token,
            progress,
            reply,
        })
    }

    /// The trigger write packets (`21 .. 18 MM`) sent so far.
    pub(crate) fn trigger_packets(&self) -> Vec<[u8; REPORT_LEN]> {
        self.write_bytes()
            .into_iter()
            .filter(|b| b[..5] == [0x21, 0, 0, 0, 0x18])
            .collect()
    }

    /// The profile selects (`21 .. 12 PP`) sent so far.
    pub(crate) fn selects(&self) -> Vec<u8> {
        self.write_bytes()
            .into_iter()
            .filter(|b| b[..6] == [0x21, 0, 0, 0, 0, 0x12])
            .map(|b| b[6])
            .collect()
    }
}

/// Whether an event is the first packet of a write.
pub(crate) fn is_write_start(event: &ProgressEvent) -> bool {
    matches!(event.step, ProgressStep::Writing { packet: 1, .. })
}
