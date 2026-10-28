//! The command handlers' side of the worker.
//!
//! Operation commands pass admission first (one operation at a time: a
//! second one fails with `operation_running` at once and is never queued),
//! send a request and wait for the reply. Direct commands (status, snapshot,
//! cancel, files) never wait for the worker.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::backend::DeviceBackend;
use crate::config::{CoreConfig, TESTED_FIRMWARE};
use crate::dto::{
    AllProfilesPreview, AppError, AppErrorKind, AppInfo, BackupFileSummary, BackupKind,
    BackupSaved, CloseChoice, ColorEditIntent, ColorEditPreview, ColorsState, DeadBandEditIntent,
    DeadBandEditPreview, DeadBandState, DepthStopReason, DepthStopRequest, DepthViewReport,
    DeviceId, DeviceSnapshot, DeviceStatus, EditReportView, ExportIntent, ExportedProfile, FileRef,
    FileToken, ImportIntent, ImportPreview, ImportSummaryView, InterruptedOp, KeymapEditIntent,
    KeymapEditPreview, KeysState, LastReport, LightEditIntent, LightEditPreview, MacroLibraryView,
    MacroProgramView, OpId, PlanInvalidation, PlanToken, PlatformView, PresetEditIntent,
    PresetsView, ProfileSwitchPreview, ProfileSwitchReport, RestoreIntent, RestorePreview,
    RestoreReportView, RevealTarget, SafetyBackupEntry, SocdEditIntent, SocdEditPreview,
    SocdStateView, TravelCloseReport, TriggerEditIntent, TriggerEditPreview, TriggerLimitsView,
    UserPresetIntent,
};
use crate::files::{FileKind, list_safety, safety_entry};
use crate::ops::journal;
use crate::shared::{Shared, lock};
use crate::sink::{DepthSink, EventSink, ExitHook, ProgressSink};
use crate::worker::request::Reply;
use crate::worker::{Envelope, Request, Worker};

/// How long a shutdown waits for the worker.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

/// The handle of the device worker. Operation commands pass admission (one
/// operation at a time; a second fails at once with `operation_running`),
/// send a request and wait for the reply; direct commands never wait for the
/// worker.
pub struct WorkerHandle {
    tx: Sender<Envelope>,
    shared: Arc<Shared>,
    config: CoreConfig,
    join: Mutex<Option<JoinHandle<()>>>,
    #[cfg(any(test, feature = "sim"))]
    sim: Option<crate::backend::sim::SimControl>,
}

/// The facts about this build.
pub(crate) fn app_info(config: &CoreConfig, sim: bool) -> AppInfo {
    AppInfo {
        version: config.version.clone(),
        sim,
        platform: PlatformView::current(),
        os_locale: config.os_locale.clone(),
        safety_dir: config.safety_dir_view(),
        tested_firmware: TESTED_FIRMWARE.iter().map(ToString::to_string).collect(),
        repository: config.repository.clone(),
    }
}

/// The operation a journal left behind (at start). The journal is first
/// moved to its own file ([`CoreConfig::interrupted_path`]), so that the
/// next write cannot overwrite or delete it before the user dismissed it.
pub(crate) fn interrupted(shared: &Shared, config: &CoreConfig) -> Option<InterruptedOp> {
    let path = journal::adopt_left_behind(&config.journal_path(), &config.interrupted_path());
    let entry = journal::read(&path)?;
    Some(match entry {
        Ok(entry) => {
            let path = PathBuf::from(&entry.safety_backup_path);
            let safety_backup = path
                .is_file()
                .then(|| shared.register_file(path, FileKind::Safety));
            InterruptedOp {
                kind: entry.op,
                started_at: crate::map::clean(&entry.started_at),
                profiles: entry.profiles,
                safety_backup,
            }
        }
        Err(_) => InterruptedOp {
            kind: crate::dto::OpKind::Restore,
            started_at: String::new(),
            profiles: Vec::new(),
            safety_backup: None,
        },
    })
}

impl WorkerHandle {
    /// Starts the worker thread; `make_backend` runs inside it.
    fn spawn<B, F>(
        config: CoreConfig,
        exit: Arc<dyn ExitHook>,
        sim: bool,
        requests_only: bool,
        make_backend: F,
    ) -> Result<(Self, Arc<Shared>), AppError>
    where
        B: DeviceBackend + 'static,
        F: FnOnce() -> Result<B, AppError> + Send + 'static,
    {
        let shared = Arc::new(Shared::new(app_info(&config, sim), exit, config.token_seed));
        shared.set_interrupted(interrupted(&shared, &config));
        // A live key-depth view of an earlier run whose close was never
        // acknowledged (or that crashed): the keyboard may still stream.
        shared.set_travel_marker_left(crate::ops::travel::marker_left(&config));
        let (tx, rx) = mpsc::channel();
        let thread_shared = Arc::clone(&shared);
        let thread_config = config.clone();
        let join = std::thread::Builder::new()
            .name("openwraith-device".to_string())
            .spawn(move || match make_backend() {
                Ok(backend) => {
                    let mut worker = Worker::new(backend, thread_shared, thread_config, rx);
                    worker.requests_only = requests_only;
                    worker.run();
                }
                Err(error) => {
                    let mut status = thread_shared.status();
                    status.connection = crate::dto::ConnectionState::Fault {
                        message: error.message,
                    };
                    thread_shared.publish(status, std::time::Instant::now());
                }
            })
            .map_err(|e| AppError::internal(format!("cannot start the device worker: {e}")))?;
        Ok((
            WorkerHandle {
                tx,
                shared: Arc::clone(&shared),
                config,
                join: Mutex::new(Some(join)),
                #[cfg(any(test, feature = "sim"))]
                sim: None,
            },
            shared,
        ))
    }

    /// Starts the worker on real keyboards (hidapi). It opens a connected
    /// keyboard at once (listening first).
    #[cfg(not(feature = "sim"))]
    pub fn spawn_hid(config: CoreConfig, exit: Arc<dyn ExitHook>) -> Result<Self, AppError> {
        let seed = config.token_seed;
        Self::spawn(config, exit, false, false, move || {
            crate::backend::hid::HidDeviceBackend::new(seed).map_err(|e| AppError::from(&e))
        })
        .map(|(handle, _)| handle)
    }

    /// Starts the worker on the developer simulator (no keyboard is used).
    #[cfg(any(test, feature = "sim"))]
    pub fn spawn_sim(
        config: CoreConfig,
        exit: Arc<dyn ExitHook>,
        options: crate::backend::sim::SimOptions,
    ) -> Result<Self, AppError> {
        use crate::backend::sim::{SimControl, SimDeviceBackend, make_devices};
        let devices = make_devices(options.devices)?;
        let control = SimControl::new(Arc::clone(&devices));
        let seed = config.token_seed;
        let (mut handle, _) = Self::spawn(config, exit, true, false, move || {
            Ok(SimDeviceBackend::new(devices, &options, seed))
        })?;
        handle.sim = Some(control);
        Ok(handle)
    }

    /// A handle for tests that drive a worker built elsewhere.
    #[cfg(test)]
    pub(crate) fn from_parts(
        tx: Sender<Envelope>,
        shared: Arc<Shared>,
        config: CoreConfig,
        join: Option<JoinHandle<()>>,
        sim: Option<crate::backend::sim::SimControl>,
    ) -> Self {
        WorkerHandle {
            tx,
            shared,
            config,
            join: Mutex::new(join),
            sim,
        }
    }

    // ----- plumbing -----

    /// Sends an operation request after admission and waits for the reply.
    fn call<T>(&self, make: impl FnOnce(Reply<T>) -> Request) -> Result<T, AppError> {
        self.admit()?;
        self.send_admitted(make, || {})
    }

    /// Takes the busy flag for an operation, or refuses at once.
    fn admit(&self) -> Result<(), AppError> {
        if self
            .shared
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AppError::new(
                AppErrorKind::OperationRunning,
                "another operation is running",
            ));
        }
        Ok(())
    }

    /// Sends an admitted request and waits for its reply; `unhandled` runs
    /// when the worker is gone without answering it.
    fn send_admitted<T>(
        &self,
        make: impl FnOnce(Reply<T>) -> Request,
        unhandled: impl FnOnce(),
    ) -> Result<T, AppError> {
        let (reply, answer) = mpsc::sync_channel(1);
        let envelope = Envelope {
            request: make(reply),
            admitted: true,
        };
        if self.tx.send(envelope).is_err() {
            self.shared.busy.store(false, Ordering::SeqCst);
            unhandled();
            return Err(AppError::internal("the device worker has stopped"));
        }
        answer.recv().unwrap_or_else(|_| {
            unhandled();
            Err(AppError::internal("the device worker did not answer"))
        })
    }

    /// Queues a request without admission and without waiting.
    fn post(&self, request: Request) {
        let _ = self.tx.send(Envelope {
            request,
            admitted: false,
        });
    }

    // ----- direct commands -----

    /// `app_info`.
    pub fn app_info(&self) -> AppInfo {
        self.shared.app_info().clone()
    }

    /// `subscribe`: replaces the subscriber (a reloaded window), stops a
    /// live key-depth view (its window is gone) and invalidates the pending
    /// plan.
    pub fn subscribe(&self, sink: Arc<dyn EventSink>) -> DeviceStatus {
        self.shared.set_subscriber(sink);
        self.shared.stop_depth(DepthStopReason::ChannelClosed);
        self.post(Request::InvalidatePlan {
            reason: PlanInvalidation::Resubscribed,
        });
        self.shared.status()
    }

    /// `get_status`.
    pub fn status(&self) -> DeviceStatus {
        self.shared.status()
    }

    /// `get_snapshot`.
    pub fn snapshot(&self) -> Option<DeviceSnapshot> {
        self.shared.snapshot()
    }

    /// `get_last_report`.
    pub fn last_report(&self) -> Option<LastReport> {
        self.shared.last_report()
    }

    /// `get_keys_state`: the keymap group of the last full read (no
    /// traffic); `None` without a snapshot.
    pub fn keys_state(&self) -> Option<KeysState> {
        self.shared
            .keys()
            .map(|cache| crate::map::keys::keys_state(&cache))
    }

    /// `list_macro_library`.
    pub fn list_macro_library(&self) -> Result<MacroLibraryView, AppError> {
        crate::library::list(&self.config)
    }

    /// `save_library_macro`: adds a macro to the library, or renames the
    /// entry with the same program.
    pub fn save_library_macro(
        &self,
        name: &str,
        program: &MacroProgramView,
    ) -> Result<MacroLibraryView, AppError> {
        crate::library::save(&self.config, name, program)
    }

    /// `rename_library_macro`.
    pub fn rename_library_macro(
        &self,
        hash: &str,
        name: &str,
    ) -> Result<MacroLibraryView, AppError> {
        crate::library::rename(&self.config, hash, name)
    }

    /// `delete_library_macro`.
    pub fn delete_library_macro(&self, hash: &str) -> Result<MacroLibraryView, AppError> {
        crate::library::delete(&self.config, hash)
    }

    /// `get_colors_state`: custom table 1 of the last full read (no
    /// traffic); `None` without a snapshot.
    pub fn colors_state(&self) -> Option<ColorsState> {
        let snapshot = self.shared.snapshot();
        self.shared
            .editors()
            .map(|cache| crate::map::editors::colors_state(&cache, snapshot.as_ref()))
    }

    /// `get_dead_band_state`: the dead band of the last full read (no
    /// traffic); `None` without a snapshot.
    pub fn dead_band_state(&self) -> Option<DeadBandState> {
        self.shared
            .editors()
            .map(|cache| crate::map::editors::dead_band_state(&cache))
    }

    /// `get_socd_state`: SOCD of the last full read, with the keyboard's
    /// pushes applied (no traffic); `None` without a snapshot.
    pub fn socd_state(&self) -> Option<SocdStateView> {
        self.shared
            .editors()
            .map(|cache| crate::map::editors::socd_state(&cache))
    }

    /// The keyboard's trigger limits (`21/04`), if read.
    fn trigger_limits(&self) -> Option<TriggerLimitsView> {
        self.shared.snapshot().and_then(|s| s.triggers.limits)
    }

    /// `list_presets`: the built-in presets and the user's, checked against
    /// the keyboard's limits.
    pub fn list_presets(&self) -> Result<PresetsView, AppError> {
        crate::ops::presets::list(&self.config, self.trigger_limits())
    }

    /// `save_user_preset`: a new preset or a replaced one (values outside
    /// the keyboard's limits are refused, never clamped).
    pub fn save_user_preset(&self, preset: &UserPresetIntent) -> Result<PresetsView, AppError> {
        crate::ops::presets::save(&self.config, preset, self.trigger_limits())
    }

    /// `delete_user_preset`.
    pub fn delete_user_preset(&self, id: &str) -> Result<PresetsView, AppError> {
        crate::ops::presets::delete(&self.config, id, self.trigger_limits())
    }

    /// `get_hints_shown`: the one-time hints already shown.
    pub fn hints_shown(&self) -> Vec<String> {
        self.config
            .settings_path
            .as_deref()
            .map(crate::settings::hints_shown)
            .unwrap_or_default()
    }

    /// `mark_hint_shown`: remembers a one-time hint (a short id).
    pub fn mark_hint_shown(&self, hint: &str) -> Result<Vec<String>, AppError> {
        let valid = !hint.is_empty()
            && hint.len() <= 64
            && hint
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
        if !valid {
            return Err(AppError::new(
                AppErrorKind::InvalidRequest,
                "a hint id has 1 to 64 letters, digits, '_', '-' or '.'",
            ));
        }
        let path =
            self.config.settings_path.as_deref().ok_or_else(|| {
                AppError::new(AppErrorKind::Io, "this build has no settings file")
            })?;
        crate::settings::mark_hint_shown(path, hint)
    }

    /// `reset_hints_shown`: forgets every hint shown ("Show tips again").
    pub fn reset_hints_shown(&self) -> Result<Vec<String>, AppError> {
        let path =
            self.config.settings_path.as_deref().ok_or_else(|| {
                AppError::new(AppErrorKind::Io, "this build has no settings file")
            })?;
        crate::settings::reset_hints_shown(path)
    }

    /// `stop_depth_view`: asks the live key-depth view to stop (the window
    /// was hidden, the page left, or Stop); false when none runs.
    pub fn stop_depth_view(&self, reason: DepthStopRequest) -> bool {
        self.shared.stop_depth(match reason {
            DepthStopRequest::Requested => DepthStopReason::Requested,
            DepthStopRequest::WindowHidden => DepthStopReason::WindowHidden,
            DepthStopRequest::PageLeft => DepthStopReason::PageLeft,
        })
    }

    /// `open_import_file`: a profile file the user chose in the (Rust) open
    /// dialog, summarized for an import.
    pub fn open_import_file(&self, path: &Path) -> Result<ImportSummaryView, AppError> {
        crate::ops::share::import_summary(&self.shared, path)
    }

    /// The default file name of a profile export.
    pub fn default_export_name(&self, profile: u8) -> String {
        let name = self.default_backup_name(BackupKind::Active);
        let stem = name
            .strip_suffix(&format!(".{}", openwraith_hid::BACKUP_EXTENSION))
            .unwrap_or(&name);
        format!(
            "{stem}-profile-{profile}.{}",
            crate::ops::share::PROFILE_EXTENSION
        )
    }

    /// `cancel_operation`: false when `op` is not running.
    pub fn cancel_operation(&self, op: &OpId) -> bool {
        self.shared.cancel(op)
    }

    /// The window is asked to close: true when it must stay open (an
    /// operation that may write runs; the frontend shows the close guard).
    /// A read (connecting, refreshing, a backup of the active profile) is
    /// safe to interrupt.
    pub fn close_requested(&self) -> bool {
        if !self.shared.busy.load(Ordering::SeqCst) {
            return false;
        }
        // A live key-depth view stops (the test is closed) and the app
        // exits after it, without the close guard.
        // (Also a view admitted but not started yet: it never opens the test.)
        if self.shared.stop_depth(DepthStopReason::AppExit) {
            self.shared.exit_after_op.store(true, Ordering::SeqCst);
            return true;
        }
        let reading_only = self.shared.status().busy.is_some_and(|op| {
            matches!(
                op.kind,
                crate::dto::OpKind::Connect
                    | crate::dto::OpKind::Refresh
                    | crate::dto::OpKind::BackupActive
                    | crate::dto::OpKind::Recheck
            )
        });
        if reading_only {
            return false;
        }
        self.shared.close_requested.store(true, Ordering::SeqCst);
        self.shared.republish();
        true
    }

    /// `answer_close`.
    pub fn answer_close(&self, choice: CloseChoice) {
        match choice {
            CloseChoice::KeepOpen => {
                self.shared.close_requested.store(false, Ordering::SeqCst);
                self.shared.exit_after_op.store(false, Ordering::SeqCst);
                self.shared.republish();
            }
            CloseChoice::StopAndClose => {
                self.shared.exit_after_op.store(true, Ordering::SeqCst);
                self.shared.cancel_any();
                if !self.shared.busy.load(Ordering::SeqCst) {
                    self.shared.exit();
                }
            }
        }
    }

    /// `dismiss_interrupted`: deletes the interrupted operation's journal
    /// (never the journal of a write that is running now).
    pub fn dismiss_interrupted(&self) -> Result<(), AppError> {
        journal::remove(&self.config.interrupted_path())?;
        self.shared.set_interrupted(None);
        self.shared.republish();
        Ok(())
    }

    /// `list_safety_backups`: newest first, at most 200.
    pub fn list_safety_backups(&self) -> Result<Vec<SafetyBackupEntry>, AppError> {
        let dir = self.config.safety_dir.clone()?;
        let listed = list_safety(&dir).map_err(|e| {
            AppError::new(
                AppErrorKind::Io,
                format!("cannot list {}: {e}", dir.display()),
            )
        })?;
        Ok(listed
            .into_iter()
            .map(|(path, name, modified)| {
                let file = self.shared.register_file(path, FileKind::Safety);
                safety_entry(file, &name, modified)
            })
            .collect())
    }

    /// `open_safety_backup`: loads a listed safety backup, or summarizes
    /// again any backup file the app gave a token for (a safety backup or a
    /// file the user opened), so that its comparison with the connected
    /// keyboard (`same_model`, `same_firmware`) is current. Backup
    /// destinations are refused (`file_unknown`).
    pub fn open_safety_backup(&self, file: &FileToken) -> Result<BackupFileSummary, AppError> {
        crate::ops::restore::summarize_file(&self.shared, file)
    }

    /// `open_backup_file`: loads a file the user chose in the (Rust) open
    /// dialog.
    pub fn open_backup_file(&self, path: &Path) -> Result<BackupFileSummary, AppError> {
        let file = self
            .shared
            .register_file(path.to_path_buf(), FileKind::User);
        self.open_safety_backup(&file.token)
    }

    /// A destination the user chose in the (Rust) save dialog.
    pub fn register_destination(&self, path: &Path) -> FileRef {
        self.shared
            .register_file(path.to_path_buf(), FileKind::Destination)
    }

    /// The default file name of a new backup.
    pub fn default_backup_name(&self, kind: BackupKind) -> String {
        let product = self
            .shared
            .snapshot()
            .and_then(|s| s.device.product)
            .map(|p| {
                p.chars()
                    .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                    .collect::<String>()
            })
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "keyboard".to_string());
        let stamp = openwraith_hid::rfc3339_utc(self.config.now())
            .replace(['-', ':'], "")
            .replace('T', "-")
            .trim_end_matches('Z')
            .to_string();
        let suffix = match kind {
            BackupKind::Active => "",
            BackupKind::AllProfiles => "-all-profiles",
        };
        format!(
            "openwraith-{product}-{stamp}{suffix}.{}",
            openwraith_hid::BACKUP_EXTENSION
        )
    }

    /// The path to show for `reveal`.
    pub fn reveal_path(&self, target: &RevealTarget) -> Result<PathBuf, AppError> {
        match target {
            RevealTarget::SafetyDir => self.config.safety_dir.clone(),
            RevealTarget::LegalNotices => self
                .config
                .legal_dir
                .as_ref()
                .map(|dir| dir.join(crate::config::NOTICES_FILE))
                .filter(|path| path.is_file())
                .ok_or_else(|| {
                    AppError::new(
                        AppErrorKind::Io,
                        "the license files are not installed with this build",
                    )
                }),
            RevealTarget::File { file } => self
                .shared
                .files()
                .get(file)
                .map(|f| f.path)
                .ok_or_else(|| AppError::new(AppErrorKind::FileUnknown, "the file is not known")),
        }
    }

    // ----- operations -----

    /// `refresh`.
    pub fn refresh(&self, progress: Arc<dyn ProgressSink>) -> Result<DeviceSnapshot, AppError> {
        self.call(|reply| Request::Refresh { progress, reply })
    }

    /// `select_device`.
    pub fn select_device(&self, id: DeviceId) -> Result<(), AppError> {
        self.call(|reply| Request::SelectDevice { id, reply })
    }

    /// `reconnect`.
    pub fn reconnect(&self) -> Result<(), AppError> {
        self.call(|reply| Request::Reconnect { reply })
    }

    /// `disconnect`.
    pub fn disconnect(&self) -> Result<(), AppError> {
        self.call(|reply| Request::Disconnect { reply })
    }

    /// `recheck_other_program`.
    pub fn recheck_other_program(&self) -> Result<(), AppError> {
        self.call(|reply| Request::RecheckOtherProgram { reply })
    }

    /// `prepare_profile_switch`.
    pub fn prepare_profile_switch(&self, profile: u8) -> Result<ProfileSwitchPreview, AppError> {
        self.call(|reply| Request::PrepareProfileSwitch { profile, reply })
    }

    /// `apply_profile_switch`.
    pub fn apply_profile_switch(
        &self,
        token: PlanToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<ProfileSwitchReport, AppError> {
        self.call(|reply| Request::ApplyProfileSwitch {
            token,
            progress,
            reply,
        })
    }

    /// `prepare_trigger_edit`.
    pub fn prepare_trigger_edit(
        &self,
        intent: TriggerEditIntent,
    ) -> Result<TriggerEditPreview, AppError> {
        self.call(|reply| Request::PrepareTriggerEdit { intent, reply })
    }

    /// `apply_trigger_edit`.
    pub fn apply_trigger_edit(
        &self,
        token: PlanToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        self.call(|reply| Request::ApplyTriggerEdit {
            token,
            progress,
            reply,
        })
    }

    /// `prepare_light_edit`.
    pub fn prepare_light_edit(
        &self,
        intent: LightEditIntent,
    ) -> Result<LightEditPreview, AppError> {
        self.call(|reply| Request::PrepareLightEdit { intent, reply })
    }

    /// `apply_light_edit`.
    pub fn apply_light_edit(
        &self,
        token: PlanToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        self.call(|reply| Request::ApplyLightEdit {
            token,
            progress,
            reply,
        })
    }

    /// `backup_active_profile`.
    pub fn backup_active_profile(
        &self,
        destination: FileToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<BackupSaved, AppError> {
        self.call(|reply| Request::BackupActiveProfile {
            destination,
            progress,
            reply,
        })
    }

    /// `prepare_all_profile_backup`.
    pub fn prepare_all_profile_backup(
        &self,
        destination: FileToken,
    ) -> Result<AllProfilesPreview, AppError> {
        self.call(|reply| Request::PrepareAllProfileBackup { destination, reply })
    }

    /// `apply_all_profile_backup`.
    pub fn apply_all_profile_backup(
        &self,
        token: PlanToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<BackupSaved, AppError> {
        self.call(|reply| Request::ApplyAllProfileBackup {
            token,
            progress,
            reply,
        })
    }

    /// `prepare_restore`.
    pub fn prepare_restore(
        &self,
        intent: RestoreIntent,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<RestorePreview, AppError> {
        self.call(|reply| Request::PrepareRestore {
            intent,
            progress,
            reply,
        })
    }

    /// `apply_restore`.
    pub fn apply_restore(
        &self,
        token: PlanToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<RestoreReportView, AppError> {
        self.call(|reply| Request::ApplyRestore {
            token,
            progress,
            reply,
        })
    }

    /// `prepare_keymap_edit`.
    pub fn prepare_keymap_edit(
        &self,
        intent: KeymapEditIntent,
    ) -> Result<KeymapEditPreview, AppError> {
        self.call(|reply| Request::PrepareKeymapEdit { intent, reply })
    }

    /// `apply_keymap_edit`.
    pub fn apply_keymap_edit(
        &self,
        token: PlanToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        self.call(|reply| Request::ApplyKeymapEdit {
            token,
            progress,
            reply,
        })
    }

    /// `prepare_color_edit`.
    pub fn prepare_color_edit(
        &self,
        intent: ColorEditIntent,
    ) -> Result<ColorEditPreview, AppError> {
        self.call(|reply| Request::PrepareColorEdit { intent, reply })
    }

    /// `apply_color_edit`.
    pub fn apply_color_edit(
        &self,
        token: PlanToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        self.call(|reply| Request::ApplyColorEdit {
            token,
            progress,
            reply,
        })
    }

    /// `prepare_dead_band_edit`.
    pub fn prepare_dead_band_edit(
        &self,
        intent: DeadBandEditIntent,
    ) -> Result<DeadBandEditPreview, AppError> {
        self.call(|reply| Request::PrepareDeadBandEdit { intent, reply })
    }

    /// `apply_dead_band_edit`.
    pub fn apply_dead_band_edit(
        &self,
        token: PlanToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        self.call(|reply| Request::ApplyDeadBandEdit {
            token,
            progress,
            reply,
        })
    }

    /// `prepare_socd_edit`.
    pub fn prepare_socd_edit(&self, intent: SocdEditIntent) -> Result<SocdEditPreview, AppError> {
        self.call(|reply| Request::PrepareSocdEdit { intent, reply })
    }

    /// `apply_socd_edit`.
    pub fn apply_socd_edit(
        &self,
        token: PlanToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<EditReportView, AppError> {
        self.call(|reply| Request::ApplySocdEdit {
            token,
            progress,
            reply,
        })
    }

    /// `prepare_preset_edit`: a preset as a trigger plan (`apply_trigger_edit`
    /// writes it).
    pub fn prepare_preset_edit(
        &self,
        intent: PresetEditIntent,
    ) -> Result<TriggerEditPreview, AppError> {
        self.call(|reply| Request::PreparePresetEdit { intent, reply })
    }

    /// `start_depth_view`: runs the live key-depth view until it stops;
    /// frames go to `sink`.
    pub fn start_depth_view(&self, sink: Arc<dyn DepthSink>) -> Result<DepthViewReport, AppError> {
        self.admit()?;
        // Registered at once, so that a stop sent before the worker handles
        // the request (Stop, a page left, a hidden window, a reloaded
        // window, app exit) is kept: the view then never opens the test.
        // The worker forgets it when the request ends.
        self.shared.reserve_depth();
        self.send_admitted(
            |reply| Request::StartDepthView { sink, reply },
            || {
                self.shared.end_depth();
            },
        )
    }

    /// `close_stray_travel_test`.
    pub fn close_stray_travel_test(&self) -> Result<TravelCloseReport, AppError> {
        self.call(|reply| Request::CloseStrayTravelTest { reply })
    }

    /// `export_profile`.
    pub fn export_profile(
        &self,
        intent: ExportIntent,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<ExportedProfile, AppError> {
        self.call(|reply| Request::ExportProfile {
            intent,
            progress,
            reply,
        })
    }

    /// `prepare_import`.
    pub fn prepare_import(
        &self,
        intent: ImportIntent,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<ImportPreview, AppError> {
        self.call(|reply| Request::PrepareImport {
            intent,
            progress,
            reply,
        })
    }

    /// `apply_import`.
    pub fn apply_import(
        &self,
        token: PlanToken,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<RestoreReportView, AppError> {
        self.call(|reply| Request::ApplyImport {
            token,
            progress,
            reply,
        })
    }

    /// `discard_plan` (queued, no admission).
    pub fn discard_plan(&self, token: PlanToken) {
        self.post(Request::DiscardPlan { token });
    }

    /// The window got the focus (queued, no admission).
    pub fn window_focused(&self) {
        self.post(Request::WindowFocused);
    }

    /// Stops the worker (the keyboard is closed); waits at most 3 s.
    pub fn shutdown(&self) {
        let (reply, done) = mpsc::sync_channel(1);
        let _ = self.tx.send(Envelope {
            request: Request::Shutdown { reply },
            admitted: false,
        });
        if done.recv_timeout(SHUTDOWN_WAIT).is_ok() {
            if let Some(join) = lock(&self.join).take() {
                let _ = join.join();
            }
        }
    }

    // ----- simulator -----

    /// `dev_sim_action` (simulator builds).
    #[cfg(any(test, feature = "sim"))]
    pub fn sim_action(
        &self,
        action: crate::dto::SimAction,
    ) -> Result<crate::dto::SimInfo, AppError> {
        self.sim_control()?.apply(action)
    }

    /// `dev_sim_info` (simulator builds).
    #[cfg(any(test, feature = "sim"))]
    pub fn sim_info(&self) -> Result<crate::dto::SimInfo, AppError> {
        self.sim_control()?.info()
    }

    #[cfg(any(test, feature = "sim"))]
    fn sim_control(&self) -> Result<&crate::backend::sim::SimControl, AppError> {
        self.sim
            .as_ref()
            .ok_or_else(|| AppError::new(AppErrorKind::InvalidRequest, "not a simulator build"))
    }

    /// The simulator controls (tests).
    #[cfg(test)]
    pub(crate) fn sim(&self) -> Option<&crate::backend::sim::SimControl> {
        self.sim.as_ref()
    }

    /// The shared state (tests).
    #[cfg(test)]
    pub(crate) fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }
}
