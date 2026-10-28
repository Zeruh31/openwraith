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
    AllProfilesPreview, AppError, AppErrorKind, AppInfo, AutoBackupEntry, AutoBackupsView,
    AutoProfileRulesIntent, AutoProfileSettings, AutoProfileStateView, BackgroundSettingsApplied,
    BackgroundSettingsIntent, BackgroundSettingsView, BackupFileSummary, BackupKind, BackupSaved,
    CloseChoice, ColorEditIntent, ColorEditPreview, ColorsState, DeadBandEditIntent,
    DeadBandEditPreview, DeadBandState, DepthStopReason, DepthStopRequest, DepthViewReport,
    DeviceId, DeviceSnapshot, DeviceStatus, EditReportView, ExportIntent, ExportedProfile, FileRef,
    FileToken, ImportIntent, ImportPreview, ImportSummaryView, InterruptedOp, KeymapEditIntent,
    KeymapEditPreview, KeysState, LastReport, LightEditIntent, LightEditPreview, MacroLibraryView,
    MacroProgramView, Notice, OpId, PlanInvalidation, PlanToken, PlatformView, PresetEditIntent,
    PresetsView, ProductProfileNamesView, ProfileNamesIntent, ProfileNamesView,
    ProfileSwitchPreview, ProfileSwitchReport, ProgramView, RecoveryIntent, RecoveryView,
    RestoreIntent, RestorePreview, RestoreReportView, RevealTarget, SafetyBackupEntry,
    SocdEditIntent, SocdEditPreview, SocdStateView, TravelCloseReport, TriggerEditIntent,
    TriggerEditPreview, TriggerLimitsView, UiStateReport, UserPresetIntent,
};
use crate::files::{FileKind, list_safety, safety_entry};
use crate::ops::journal;
use crate::shared::{Shared, lock};
use crate::shell::{ForegroundProgram, OpenReason, TrayAction, TrayModel, TrayOutcome};
use crate::sink::{
    DepthSink, EventSink, ExitHook, NotificationSink, ProgramSource, ProgressSink, ShellHook,
    TraySink,
};
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

/// At start: an unreadable settings file is kept aside (the defaults are
/// used) and the first subscriber is told (`Notice::SettingsFileInvalid`).
pub(crate) fn check_settings_file(shared: &Shared, config: &CoreConfig) {
    if let Some(kept_as) = config
        .settings_path
        .as_deref()
        .and_then(crate::settings::quarantine_invalid)
    {
        shared.set_start_notice(Some(Notice::SettingsFileInvalid { kept_as }));
    }
}

/// Loads the per-game rules and the background settings into the automatic
/// profiles' state (at start; tests call it after changing the file).
pub(crate) fn load_auto(shared: &Shared, config: &CoreConfig) {
    let path = config.settings_path.as_deref();
    let settings = path.map(crate::settings::auto_profile).unwrap_or_default();
    let background = path.map(crate::settings::background).unwrap_or_default();
    let now = shared.now();
    let mut auto = shared.auto();
    auto.settings_path = config.settings_path.clone();
    auto.forget_names();
    auto.load(now, settings, background);
}

/// The profile names as a view: every product with names, and the current
/// one (four entries each).
fn profile_names_view(
    current: Option<String>,
    mut all: std::collections::BTreeMap<String, Vec<Option<String>>>,
) -> ProfileNamesView {
    if let Some(product) = &current {
        all.entry(product.clone()).or_insert_with(|| vec![None; 4]);
    }
    ProfileNamesView {
        current_product: current,
        products: all
            .into_iter()
            .map(|(product, names)| ProductProfileNamesView { product, names })
            .collect(),
    }
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
        check_settings_file(&shared, &config);
        load_auto(&shared, &config);
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
        // A reloaded window has no drafts and no vendor-site tick any more.
        self.shared.auto().ui = UiStateReport::default();
        self.shared.stop_depth(DepthStopReason::ChannelClosed);
        self.post(Request::InvalidatePlan {
            reason: PlanInvalidation::Resubscribed,
        });
        if let Some(notice) = self.shared.take_start_notice() {
            self.shared.notice(notice);
        }
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

    // ----- automatic backups and recovery -----

    /// `list_auto_backups`: the automatic backups of the keyboard read last
    /// (else of every keyboard in the store), newest first.
    pub fn list_auto_backups(&self) -> AutoBackupsView {
        crate::ops::recovery::list_auto_backups(&self.shared, &self.config)
    }

    /// `pin_auto_backup`: keeps (`pinned`) or no longer keeps an automatic
    /// backup; a pinned one is never deleted.
    pub fn pin_auto_backup(
        &self,
        file: &FileToken,
        pinned: bool,
    ) -> Result<AutoBackupsView, AppError> {
        crate::ops::recovery::pin_auto_backup(&self.shared, &self.config, file, pinned)
    }

    /// `get_recovery`: the Recovery card (no traffic).
    pub fn get_recovery(&self) -> RecoveryView {
        crate::ops::recovery::get_recovery(&self.shared, &self.config)
    }

    /// `keep_current_settings` ("It's fine, keep them"): the reads held
    /// after a suspected reset become the automatic backups of `profiles`
    /// (every held profile when empty). Nothing is sent to the keyboard.
    pub fn keep_current_settings(&self, profiles: &[u8]) -> Result<RecoveryView, AppError> {
        crate::ops::recovery::keep_current_settings(&self.shared, &self.config, profiles)
    }

    /// `prepare_recovery`: compares automatic backups with the keyboard
    /// (the standard restore's comparison; `apply_restore` writes the plan
    /// after the review).
    pub fn prepare_recovery(
        &self,
        intent: RecoveryIntent,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<RestorePreview, AppError> {
        self.call(|reply| Request::PrepareRecovery {
            intent,
            progress,
            reply,
        })
    }

    // ----- background settings and profile names -----

    fn settings_file(&self) -> Result<&Path, AppError> {
        self.config
            .settings_path
            .as_deref()
            .ok_or_else(|| AppError::new(AppErrorKind::Io, "this build has no settings file"))
    }

    /// Registers how the shell applies changed background settings (the
    /// tray icon, the Run entry). Called once by the Tauri glue.
    pub fn set_shell_hook(&self, hook: Arc<dyn ShellHook>) {
        self.shared.set_shell(hook);
    }

    /// `get_background_settings`.
    pub fn background_settings(&self) -> BackgroundSettingsView {
        self.config
            .settings_path
            .as_deref()
            .map(crate::settings::background)
            .unwrap_or_default()
    }

    /// `set_background_settings`: validates and saves them, then has the
    /// shell apply what changed (the tray, start with Windows; through the
    /// registered [`ShellHook`]). Without the tray there is no start with
    /// Windows.
    pub fn set_background_settings(
        &self,
        intent: &BackgroundSettingsIntent,
    ) -> Result<BackgroundSettingsApplied, AppError> {
        let (settings, shell) = crate::settings::set_background(self.settings_file()?, intent)?;
        // The tray mode (the 180 s rule) and the notifications follow.
        self.shared.auto().background = settings;
        let shell_error = match self.shared.shell() {
            Some(hook) if !shell.is_empty() => hook.apply(&shell).err(),
            _ => None,
        };
        Ok(BackgroundSettingsApplied {
            settings,
            shell,
            shell_error,
        })
    }

    /// Remembers that the first close to the tray was explained (the glue
    /// shows that notification once).
    pub fn mark_close_to_tray_explained(&self) -> Result<(), AppError> {
        crate::settings::mark_close_to_tray_explained(self.settings_file()?)
    }

    /// The per-game rules as stored (Settings > Games; off by default).
    /// The commands of the automatic profiles use it.
    pub fn auto_profile_settings(&self) -> AutoProfileSettings {
        self.config
            .settings_path
            .as_deref()
            .map(crate::settings::auto_profile)
            .unwrap_or_default()
    }

    /// Validates and saves the per-game rules (at most 64; unique ids;
    /// program file names; profiles 1-4); returns them as saved.
    pub fn save_auto_profile_settings(
        &self,
        settings: &AutoProfileSettings,
    ) -> Result<AutoProfileSettings, AppError> {
        crate::settings::set_auto_profile(self.settings_file()?, settings)
    }

    /// The product of the keyboard read last, if any.
    fn current_product(&self) -> Option<String> {
        self.shared
            .snapshot()
            .and_then(|s| s.device.product)
            .or_else(|| {
                self.shared
                    .known_good()
                    .device
                    .as_ref()
                    .and_then(|d| d.product.clone())
            })
    }

    /// `get_profile_names`: the local names of every product's profiles.
    pub fn profile_names(&self) -> ProfileNamesView {
        let all = self
            .config
            .settings_path
            .as_deref()
            .map(crate::settings::profile_names)
            .unwrap_or_default();
        profile_names_view(self.current_product(), all)
    }

    /// `set_profile_names`: names the four profiles of a product (local to
    /// this PC; nothing is sent to the keyboard).
    pub fn set_profile_names(
        &self,
        names: &ProfileNamesIntent,
    ) -> Result<ProfileNamesView, AppError> {
        let all = crate::settings::set_profile_names(
            self.settings_file()?,
            &names.product,
            &names.names,
        )?;
        // The tray shows the names.
        self.shared.auto().forget_names();
        self.shared.republish();
        Ok(profile_names_view(self.current_product(), all))
    }

    // ----- per-game automatic profiles (Settings > Games) -----

    /// `get_auto_profile_state`: the rules, the live status line and the
    /// recent activity (no traffic).
    pub fn auto_profile_state(&self) -> AutoProfileStateView {
        let available = self.shared.programs().is_some();
        self.shared.auto().state_view(available)
    }

    /// `set_auto_profile_rules`: replaces the per-game rules (programs by
    /// token, by typed file name or kept from the rule with the same id;
    /// validated: at most 64 rules, profiles 1-4). Creating a rule is the
    /// user's consent for it; the engine uses the new rules at once.
    pub fn set_auto_profile_rules(
        &self,
        intent: &AutoProfileRulesIntent,
    ) -> Result<AutoProfileStateView, AppError> {
        let path = self.settings_file()?;
        let stored = crate::settings::auto_profile(path);
        let rules = {
            let auto = self.shared.auto();
            crate::auto::programs::rules_from_intent(intent, &stored, &auto.programs)?
        };
        let saved = crate::settings::set_auto_profile(path, &rules)?;
        let now = self.shared.now();
        {
            let mut auto = self.shared.auto();
            let background = auto.background;
            auto.load(now, saved, background);
        }
        self.emit_auto();
        self.shared.republish();
        Ok(self.auto_profile_state())
    }

    /// `pause_auto_profile`: pauses (`true`) or resumes the automatic
    /// profiles for this run (also after the automation paused itself:
    /// too many switches, a failed switch).
    pub fn pause_auto_profile(&self, paused: bool) -> AutoProfileStateView {
        self.shared.auto().engine.set_paused(paused);
        self.emit_auto();
        self.shared.republish();
        self.auto_profile_state()
    }

    /// Records and sends what the engine reported after a direct command.
    fn emit_auto(&self) {
        let events = self.shared.auto().engine.take_events();
        let at = openwraith_hid::rfc3339_utc(self.config.now());
        crate::auto::emit(&self.shared, &self.config.state_dir, events, &at);
    }

    /// `list_running_programs`: the running programs (from the glue's
    /// [`ProgramSource`]; empty without one), each with a token.
    pub fn list_running_programs(&self) -> Vec<ProgramView> {
        let Some(source) = self.shared.programs() else {
            return Vec::new();
        };
        let running = crate::auto::programs::running(source.running_programs(), std::process::id());
        let mut auto = self.shared.auto();
        let mut mint = self.shared.mint();
        running
            .into_iter()
            .map(|program| auto.programs.view(&mut mint, program))
            .collect()
    }

    /// `pick_game_program`: the program the user chose in the (Rust) open
    /// dialog, with a token.
    pub fn register_program(&self, path: &Path) -> Result<ProgramView, AppError> {
        // The path the platform will report for the running program: links,
        // junctions and SUBST drives resolved (the given path when that
        // fails).
        let path = crate::files::canonical_program_path(path);
        let text = path.to_string_lossy();
        let program = crate::auto::programs::program_of(None, Some(&text)).ok_or_else(|| {
            AppError::new(
                AppErrorKind::InvalidRequest,
                "the chosen file is not a program",
            )
        })?;
        let mut auto = self.shared.auto();
        let mut mint = self.shared.mint();
        Ok(auto.programs.view(&mut mint, program))
    }

    /// `detect_next_program`: waits (at most `timeout`; the command passes
    /// 30 s) for the next program that comes to the front (not the one in
    /// front now, not OpenWraith); `None` when none did.
    pub fn detect_next_program(&self, timeout: Duration) -> Option<ProgramView> {
        let (reply, answer) = mpsc::sync_channel(1);
        let id = self.shared.auto().start_detect(reply);
        let found = answer.recv_timeout(timeout).ok();
        if found.is_none() {
            self.shared.auto().stop_detect(id);
        }
        let found = found?;
        let program =
            crate::auto::programs::program_of(found.exe.as_deref(), found.path.as_deref())?;
        let mut auto = self.shared.auto();
        let mut mint = self.shared.mint();
        Some(auto.programs.view(&mut mint, program))
    }

    /// `report_ui_state`: the frontend has drafts / the vendor-site
    /// acknowledgement of this run (reset by every `subscribe`).
    pub fn report_ui_state(&self, state: UiStateReport) {
        self.shared.auto().ui = state;
    }

    // ----- the platform feed and the shell (the Tauri glue) -----

    /// The program in front changed (`None`: nothing is known about it).
    /// The glue's foreground thread calls it about once a second (repeating
    /// the same program changes nothing).
    pub fn foreground_changed(&self, program: Option<ForegroundProgram>) {
        let now = self.shared.now();
        self.shared.auto().foreground(now, program);
    }

    /// The processes whose exit the automatic profiles wait for; the glue
    /// checks them (about once a second) and reports
    /// [`process_exited`](Self::process_exited).
    pub fn auto_watched_pids(&self) -> Vec<u32> {
        self.shared.auto().engine.watched_pids()
    }

    /// A watched process is gone.
    pub fn process_exited(&self, pid: u32) {
        let now = self.shared.now();
        self.shared.auto().engine.process_exited(now, pid);
    }

    /// Registers the running-programs source (once, by the glue).
    pub fn set_program_source(&self, source: Arc<dyn ProgramSource>) {
        self.shared.set_programs(source);
    }

    /// Registers the OS notification sink (once, by the glue).
    pub fn set_notification_sink(&self, sink: Arc<dyn NotificationSink>) {
        self.shared.set_notifications(sink);
    }

    /// Registers the tray sink (once, by the glue); it gets the current
    /// model at once and every change after.
    pub fn set_tray_sink(&self, sink: Arc<dyn TraySink>) {
        self.shared.set_tray(sink);
    }

    /// The tray model now.
    pub fn tray_model(&self) -> TrayModel {
        let status = self.shared.status();
        self.shared.tray_model(&status)
    }

    /// A click in the tray menu (called on a blocking thread: a profile
    /// switch and "Back up now" are operations). A profile item is a manual
    /// switch through the profile switch's prepare and apply, unless drafts
    /// exist, a plan is pending, writes are blocked or this run has no
    /// vendor-site acknowledgement yet: then the window opens instead.
    pub fn tray_action(&self, action: TrayAction) -> TrayOutcome {
        use crate::auto::notify::{ROUTE_BACKUPS, ROUTE_DEVICE, ROUTE_RECOVERY};
        let open = |route: &str, reason| TrayOutcome::OpenWindow {
            route: route.to_string(),
            reason,
        };
        match action {
            TrayAction::SwitchProfile(profile) => {
                let status = self.shared.status();
                let ui = self.shared.auto().ui;
                let plan = self.shared.plan_pending.load(Ordering::SeqCst);
                if let Some(outcome) = crate::auto::tray::profile_click(profile, &status, &ui, plan)
                {
                    return outcome;
                }
                let preview = match self.prepare_profile_switch(profile) {
                    Ok(preview) => preview,
                    Err(error) if error.kind == AppErrorKind::OperationRunning => {
                        return open(ROUTE_DEVICE, OpenReason::WritesBlocked);
                    }
                    Err(_) => return open(ROUTE_DEVICE, OpenReason::Failed),
                };
                let Some(token) = preview.token else {
                    return TrayOutcome::NothingToDo;
                };
                match self.apply_profile_switch(token, Arc::new(crate::sink::NoProgress)) {
                    Ok(_) => TrayOutcome::Done,
                    Err(_) => open(ROUTE_DEVICE, OpenReason::Failed),
                }
            }
            TrayAction::SetAutoPaused(paused) => {
                self.pause_auto_profile(paused);
                TrayOutcome::Done
            }
            TrayAction::BackUpNow => match self.back_up_now() {
                Ok(Some(_)) => TrayOutcome::Done,
                Ok(None) => open(ROUTE_RECOVERY, OpenReason::NeedsWindow),
                Err(error) if error.kind == AppErrorKind::OperationRunning => {
                    open(ROUTE_BACKUPS, OpenReason::WritesBlocked)
                }
                Err(_) => open(ROUTE_BACKUPS, OpenReason::Failed),
            },
            TrayAction::Attention(item) => match crate::auto::tray::attention_route(item) {
                Some(route) => open(route, OpenReason::NeedsWindow),
                None => match self.recheck_other_program() {
                    Ok(()) => TrayOutcome::Done,
                    Err(_) => open(ROUTE_DEVICE, OpenReason::Failed),
                },
            },
        }
    }

    /// "Back up now" (the tray): an automatic backup of the active profile
    /// (no switching); `None` when the profile looks reset (the Recovery
    /// card first).
    pub fn back_up_now(&self) -> Result<Option<AutoBackupEntry>, AppError> {
        self.call(|reply| Request::BackUpNow { reply })
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
            RevealTarget::AutoBackupDir => {
                let root = self.config.known_good_dir();
                let key = self
                    .shared
                    .known_good()
                    .device
                    .as_ref()
                    .map(|d| d.key.clone());
                Ok(match key {
                    Some(key) => crate::autobackup::device_dir(&root, &key),
                    None => root,
                })
            }
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
        if done.recv_timeout(SHUTDOWN_WAIT).is_ok()
            && let Some(join) = lock(&self.join).take()
        {
            let _ = join.join();
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
