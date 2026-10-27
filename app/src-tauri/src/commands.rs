//! The IPC commands (names in `command_names.rs`). Every argument name is a
//! single word, so Tauri's case conversion changes nothing.
//!
//! Direct commands answer from shared state. Operation commands run the
//! worker call on a blocking thread (`spawn_blocking`); admission happens
//! there before anything is queued, so a second operation fails at once with
//! `operation_running`.

use std::sync::{Arc, Mutex};

use openwraith_app_core::WorkerHandle;
use openwraith_app_core::dto::{
    AllProfilesPreview, AppError, AppEvent, AppInfo, BackupFileSummary, BackupKind, BackupSaved,
    CloseChoice, DeviceId, DeviceSnapshot, DeviceStatus, DialogText, EditReportView, FileRef,
    FileToken, LastReport, LightEditIntent, LightEditPreview, OpId, PlanToken,
    ProfileSwitchPreview, ProfileSwitchReport, ProgressEvent, RestoreIntent, RestorePreview,
    RestoreReportView, RevealTarget, SafetyBackupEntry, Settings, ThemeSetting, TriggerEditIntent,
    TriggerEditPreview,
};
use tauri::ipc::Channel;
use tauri::{AppHandle, Runtime, State, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

use crate::dialogs;
use crate::sinks::{ChannelEvents, ChannelProgress};

/// The worker, managed by Tauri.
pub type Worker<'a> = State<'a, Arc<WorkerHandle>>;

/// The settings and their file.
pub struct SettingsState {
    /// The settings file.
    pub path: std::path::PathBuf,
    /// The current settings.
    pub value: Mutex<Settings>,
}

/// Runs `f` on a blocking thread.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::internal(format!("the command did not finish: {e}")))?
}

fn progress(channel: Channel<ProgressEvent>) -> Arc<ChannelProgress> {
    Arc::new(ChannelProgress(channel))
}

/// The native window theme for a setting.
pub fn native_theme(theme: ThemeSetting) -> Option<tauri::Theme> {
    match theme {
        ThemeSetting::System => None,
        ThemeSetting::Light => Some(tauri::Theme::Light),
        ThemeSetting::Dark => Some(tauri::Theme::Dark),
    }
}

// ----- direct commands -----

/// `app_info`.
#[tauri::command]
pub fn app_info(worker: Worker<'_>) -> AppInfo {
    worker.app_info()
}

/// `get_settings`.
#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> Settings {
    state.value.lock().map(|s| *s).unwrap_or_default()
}

/// `set_settings`: saved atomically; the native title bar follows the
/// theme.
#[tauri::command]
pub async fn set_settings<R: Runtime>(
    settings: Settings,
    state: State<'_, SettingsState>,
    window: WebviewWindow<R>,
) -> Result<Settings, AppError> {
    let path = state.path.clone();
    blocking(move || openwraith_app_core::save_settings(&path, &settings)).await?;
    if let Ok(mut value) = state.value.lock() {
        *value = settings;
    }
    let _ = window.set_theme(native_theme(settings.theme));
    Ok(settings)
}

/// `subscribe`: status changes and notices on `channel`.
#[tauri::command]
pub fn subscribe(channel: Channel<AppEvent>, worker: Worker<'_>) -> DeviceStatus {
    worker.subscribe(Arc::new(ChannelEvents(channel)))
}

/// `get_status`.
#[tauri::command]
pub fn get_status(worker: Worker<'_>) -> DeviceStatus {
    worker.status()
}

/// `get_snapshot`.
#[tauri::command]
pub fn get_snapshot(worker: Worker<'_>) -> Option<DeviceSnapshot> {
    worker.snapshot()
}

/// `get_last_report`.
#[tauri::command]
pub fn get_last_report(worker: Worker<'_>) -> Option<LastReport> {
    worker.last_report()
}

/// `cancel_operation`.
#[tauri::command]
pub fn cancel_operation(op: OpId, worker: Worker<'_>) -> bool {
    worker.cancel_operation(&op)
}

/// `answer_close`.
#[tauri::command]
pub fn answer_close(choice: CloseChoice, worker: Worker<'_>) {
    worker.answer_close(choice);
}

/// `dismiss_interrupted`.
#[tauri::command]
pub fn dismiss_interrupted(worker: Worker<'_>) -> Result<(), AppError> {
    worker.dismiss_interrupted()
}

/// `list_safety_backups`.
#[tauri::command]
pub async fn list_safety_backups(worker: Worker<'_>) -> Result<Vec<SafetyBackupEntry>, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.list_safety_backups()).await
}

/// `open_safety_backup`: loads a safety backup, or summarizes again any
/// backup file the app gave a token for (the comparison with the keyboard).
#[tauri::command]
pub async fn open_safety_backup(
    file: FileToken,
    worker: Worker<'_>,
) -> Result<BackupFileSummary, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.open_safety_backup(&file)).await
}

/// `open_backup_file`: the open dialog; `null` when cancelled.
#[tauri::command]
pub async fn open_backup_file<R: Runtime>(
    dialog: DialogText,
    app: AppHandle<R>,
    worker: Worker<'_>,
) -> Result<Option<BackupFileSummary>, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || match dialogs::pick_backup(&app, &dialog)? {
        Some(path) => worker.open_backup_file(&path).map(Some),
        None => Ok(None),
    })
    .await
}

/// `choose_backup_destination`: the save dialog; `null` when cancelled.
#[tauri::command]
pub async fn choose_backup_destination<R: Runtime>(
    kind: BackupKind,
    dialog: DialogText,
    app: AppHandle<R>,
    worker: Worker<'_>,
) -> Result<Option<FileRef>, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || {
        let name = worker.default_backup_name(kind);
        Ok(dialogs::choose_destination(&app, &dialog, &name)?
            .map(|path| worker.register_destination(&path)))
    })
    .await
}

/// `reveal`: shows the safety folder or a known file in the file manager.
#[tauri::command]
pub async fn reveal<R: Runtime>(
    target: RevealTarget,
    app: AppHandle<R>,
    worker: Worker<'_>,
) -> Result<(), AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || {
        let path = worker.reveal_path(&target)?;
        if matches!(target, RevealTarget::SafetyDir) {
            std::fs::create_dir_all(&path).map_err(|e| {
                AppError::new(
                    openwraith_app_core::dto::AppErrorKind::Io,
                    format!("cannot create {}: {e}", path.display()),
                )
            })?;
        }
        app.opener().reveal_item_in_dir(&path).map_err(|e| {
            AppError::new(
                openwraith_app_core::dto::AppErrorKind::Io,
                format!("cannot show {}: {e}", path.display()),
            )
        })
    })
    .await
}

// ----- operations -----

/// `refresh`.
#[tauri::command]
pub async fn refresh(
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<DeviceSnapshot, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.refresh(sink)).await
}

/// `select_device`.
#[tauri::command]
pub async fn select_device(id: DeviceId, worker: Worker<'_>) -> Result<(), AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.select_device(id)).await
}

/// `reconnect`.
#[tauri::command]
pub async fn reconnect(worker: Worker<'_>) -> Result<(), AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.reconnect()).await
}

/// `disconnect`.
#[tauri::command]
pub async fn disconnect(worker: Worker<'_>) -> Result<(), AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.disconnect()).await
}

/// `recheck_other_program`.
#[tauri::command]
pub async fn recheck_other_program(worker: Worker<'_>) -> Result<(), AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.recheck_other_program()).await
}

/// `prepare_profile_switch`.
#[tauri::command]
pub async fn prepare_profile_switch(
    profile: u8,
    worker: Worker<'_>,
) -> Result<ProfileSwitchPreview, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.prepare_profile_switch(profile)).await
}

/// `apply_profile_switch`.
#[tauri::command]
pub async fn apply_profile_switch(
    token: PlanToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<ProfileSwitchReport, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.apply_profile_switch(token, sink)).await
}

/// `prepare_trigger_edit`.
#[tauri::command]
pub async fn prepare_trigger_edit(
    intent: TriggerEditIntent,
    worker: Worker<'_>,
) -> Result<TriggerEditPreview, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.prepare_trigger_edit(intent)).await
}

/// `apply_trigger_edit`.
#[tauri::command]
pub async fn apply_trigger_edit(
    token: PlanToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<EditReportView, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.apply_trigger_edit(token, sink)).await
}

/// `prepare_light_edit`.
#[tauri::command]
pub async fn prepare_light_edit(
    intent: LightEditIntent,
    worker: Worker<'_>,
) -> Result<LightEditPreview, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.prepare_light_edit(intent)).await
}

/// `apply_light_edit`.
#[tauri::command]
pub async fn apply_light_edit(
    token: PlanToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<EditReportView, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.apply_light_edit(token, sink)).await
}

/// `backup_active_profile`.
#[tauri::command]
pub async fn backup_active_profile(
    destination: FileToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<BackupSaved, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.backup_active_profile(destination, sink)).await
}

/// `prepare_all_profile_backup`.
#[tauri::command]
pub async fn prepare_all_profile_backup(
    destination: FileToken,
    worker: Worker<'_>,
) -> Result<AllProfilesPreview, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.prepare_all_profile_backup(destination)).await
}

/// `apply_all_profile_backup`.
#[tauri::command]
pub async fn apply_all_profile_backup(
    token: PlanToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<BackupSaved, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.apply_all_profile_backup(token, sink)).await
}

/// `prepare_restore`.
#[tauri::command]
pub async fn prepare_restore(
    intent: RestoreIntent,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<RestorePreview, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.prepare_restore(intent, sink)).await
}

/// `apply_restore`.
#[tauri::command]
pub async fn apply_restore(
    token: PlanToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<RestoreReportView, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.apply_restore(token, sink)).await
}

/// `discard_plan` (queued; no admission).
#[tauri::command]
pub fn discard_plan(token: PlanToken, worker: Worker<'_>) {
    worker.discard_plan(token);
}

// ----- developer simulator (simulator builds only) -----

/// `dev_sim_action`.
#[cfg(feature = "sim")]
#[tauri::command]
pub fn dev_sim_action(
    action: openwraith_app_core::dto::SimAction,
    worker: Worker<'_>,
) -> Result<openwraith_app_core::dto::SimInfo, AppError> {
    worker.sim_action(action)
}

/// `dev_sim_info`.
#[cfg(feature = "sim")]
#[tauri::command]
pub fn dev_sim_info(worker: Worker<'_>) -> Result<openwraith_app_core::dto::SimInfo, AppError> {
    worker.sim_info()
}
