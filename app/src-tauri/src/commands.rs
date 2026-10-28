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
    CloseChoice, ColorEditIntent, ColorEditPreview, ColorsState, DeadBandEditIntent,
    DeadBandEditPreview, DeadBandState, DepthFrame, DepthStopRequest, DepthViewReport, DeviceId,
    DeviceSnapshot, DeviceStatus, DialogText, EditReportView, ExportIntent, ExportedProfile,
    FileRef, FileToken, ImportIntent, ImportPreview, ImportSummaryView, KeymapEditIntent,
    KeymapEditPreview, KeysState, LastReport, LightEditIntent, LightEditPreview, MacroLibraryView,
    MacroProgramView, OpId, PlanToken, PresetEditIntent, PresetsView, ProfileSwitchPreview,
    ProfileSwitchReport, ProgressEvent, RestoreIntent, RestorePreview, RestoreReportView,
    RevealTarget, SafetyBackupEntry, Settings, SocdEditIntent, SocdEditPreview, SocdStateView,
    ThemeSetting, TravelCloseReport, TriggerEditIntent, TriggerEditPreview, UserPresetIntent,
};
use tauri::ipc::Channel;
use tauri::{AppHandle, Runtime, State, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

use crate::dialogs;
use crate::sinks::{ChannelDepth, ChannelEvents, ChannelProgress};

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

// ----- the Keys page -----

/// `get_keys_state`: the keymap group of the last full read (no traffic);
/// `null` without a snapshot.
#[tauri::command]
pub fn get_keys_state(worker: Worker<'_>) -> Option<KeysState> {
    worker.keys_state()
}

/// `prepare_keymap_edit`.
#[tauri::command]
pub async fn prepare_keymap_edit(
    intent: KeymapEditIntent,
    worker: Worker<'_>,
) -> Result<KeymapEditPreview, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.prepare_keymap_edit(intent)).await
}

/// `apply_keymap_edit`.
#[tauri::command]
pub async fn apply_keymap_edit(
    token: PlanToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<EditReportView, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.apply_keymap_edit(token, sink)).await
}

/// `list_macro_library`: the macros saved on this PC.
#[tauri::command]
pub async fn list_macro_library(worker: Worker<'_>) -> Result<MacroLibraryView, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.list_macro_library()).await
}

/// `save_library_macro`: adds a macro to the library (or renames the entry
/// with the same program).
#[tauri::command]
pub async fn save_library_macro(
    name: String,
    program: MacroProgramView,
    worker: Worker<'_>,
) -> Result<MacroLibraryView, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.save_library_macro(&name, &program)).await
}

/// `rename_library_macro`.
#[tauri::command]
pub async fn rename_library_macro(
    hash: String,
    name: String,
    worker: Worker<'_>,
) -> Result<MacroLibraryView, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.rename_library_macro(&hash, &name)).await
}

/// `delete_library_macro`.
#[tauri::command]
pub async fn delete_library_macro(
    hash: String,
    worker: Worker<'_>,
) -> Result<MacroLibraryView, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.delete_library_macro(&hash)).await
}

// ----- key colours, dead band, SOCD, trigger presets -----

/// `get_colors_state`: custom table 1 of the last full read (no traffic);
/// `null` without a snapshot.
#[tauri::command]
pub fn get_colors_state(worker: Worker<'_>) -> Option<ColorsState> {
    worker.colors_state()
}

/// `prepare_color_edit`.
#[tauri::command]
pub async fn prepare_color_edit(
    intent: ColorEditIntent,
    worker: Worker<'_>,
) -> Result<ColorEditPreview, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.prepare_color_edit(intent)).await
}

/// `apply_color_edit`.
#[tauri::command]
pub async fn apply_color_edit(
    token: PlanToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<EditReportView, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.apply_color_edit(token, sink)).await
}

/// `get_dead_band_state`: the dead band of the last full read (no
/// traffic); `null` without a snapshot.
#[tauri::command]
pub fn get_dead_band_state(worker: Worker<'_>) -> Option<DeadBandState> {
    worker.dead_band_state()
}

/// `prepare_dead_band_edit`.
#[tauri::command]
pub async fn prepare_dead_band_edit(
    intent: DeadBandEditIntent,
    worker: Worker<'_>,
) -> Result<DeadBandEditPreview, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.prepare_dead_band_edit(intent)).await
}

/// `apply_dead_band_edit`.
#[tauri::command]
pub async fn apply_dead_band_edit(
    token: PlanToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<EditReportView, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.apply_dead_band_edit(token, sink)).await
}

/// `get_socd_state`: SOCD of the last full read with the keyboard's pushes
/// applied (no traffic); `null` without a snapshot.
#[tauri::command]
pub fn get_socd_state(worker: Worker<'_>) -> Option<SocdStateView> {
    worker.socd_state()
}

/// `prepare_socd_edit`.
#[tauri::command]
pub async fn prepare_socd_edit(
    intent: SocdEditIntent,
    worker: Worker<'_>,
) -> Result<SocdEditPreview, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.prepare_socd_edit(intent)).await
}

/// `apply_socd_edit`.
#[tauri::command]
pub async fn apply_socd_edit(
    token: PlanToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<EditReportView, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.apply_socd_edit(token, sink)).await
}

/// `list_presets`: the built-in trigger presets and the user's.
#[tauri::command]
pub async fn list_presets(worker: Worker<'_>) -> Result<PresetsView, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.list_presets()).await
}

/// `save_user_preset`.
#[tauri::command]
pub async fn save_user_preset(
    preset: UserPresetIntent,
    worker: Worker<'_>,
) -> Result<PresetsView, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.save_user_preset(&preset)).await
}

/// `delete_user_preset`.
#[tauri::command]
pub async fn delete_user_preset(id: String, worker: Worker<'_>) -> Result<PresetsView, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.delete_user_preset(&id)).await
}

/// `prepare_preset_edit`: a preset as a trigger plan (`apply_trigger_edit`
/// writes it).
#[tauri::command]
pub async fn prepare_preset_edit(
    intent: PresetEditIntent,
    worker: Worker<'_>,
) -> Result<TriggerEditPreview, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.prepare_preset_edit(intent)).await
}

// ----- the live key-depth view -----

/// `start_depth_view`: frames on `channel` until the view stops; then its
/// report.
#[tauri::command]
pub async fn start_depth_view(
    channel: Channel<DepthFrame>,
    worker: Worker<'_>,
) -> Result<DepthViewReport, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = Arc::new(ChannelDepth(channel));
    blocking(move || worker.start_depth_view(sink)).await
}

/// `stop_depth_view`: false when no view runs. This command answers at once
/// on the main thread, while `start_depth_view` registers its view on a
/// blocking thread a moment later: a stop sent in that gap answers false,
/// and the Key depth tab sends it again while its start is on its way.
#[tauri::command]
pub fn stop_depth_view(reason: DepthStopRequest, worker: Worker<'_>) -> bool {
    worker.stop_depth_view(reason)
}

/// `close_stray_travel_test` (a user click only).
#[tauri::command]
pub async fn close_stray_travel_test(worker: Worker<'_>) -> Result<TravelCloseReport, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.close_stray_travel_test()).await
}

// ----- sharing a profile -----

/// `choose_export_destination`: the save dialog (`.owprofile.json`);
/// `null` when cancelled.
#[tauri::command]
pub async fn choose_export_destination<R: Runtime>(
    profile: u8,
    dialog: DialogText,
    app: AppHandle<R>,
    worker: Worker<'_>,
) -> Result<Option<FileRef>, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || {
        let name = worker.default_export_name(profile);
        Ok(dialogs::choose_destination(&app, &dialog, &name)?
            .map(|path| worker.register_destination(&path)))
    })
    .await
}

/// `export_profile`.
#[tauri::command]
pub async fn export_profile(
    intent: ExportIntent,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<ExportedProfile, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.export_profile(intent, sink)).await
}

/// `open_import_file`: the open dialog; `null` when cancelled.
#[tauri::command]
pub async fn open_import_file<R: Runtime>(
    dialog: DialogText,
    app: AppHandle<R>,
    worker: Worker<'_>,
) -> Result<Option<ImportSummaryView>, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || match dialogs::pick_backup(&app, &dialog)? {
        Some(path) => worker.open_import_file(&path).map(Some),
        None => Ok(None),
    })
    .await
}

/// `prepare_import`.
#[tauri::command]
pub async fn prepare_import(
    intent: ImportIntent,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<ImportPreview, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.prepare_import(intent, sink)).await
}

/// `apply_import`.
#[tauri::command]
pub async fn apply_import(
    token: PlanToken,
    progress: Channel<ProgressEvent>,
    worker: Worker<'_>,
) -> Result<RestoreReportView, AppError> {
    let worker = Arc::clone(worker.inner());
    let sink = self::progress(progress);
    blocking(move || worker.apply_import(token, sink)).await
}

// ----- one-time hints -----

/// `get_hints_shown`.
#[tauri::command]
pub async fn get_hints_shown(worker: Worker<'_>) -> Result<Vec<String>, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || Ok(worker.hints_shown())).await
}

/// `mark_hint_shown`.
#[tauri::command]
pub async fn mark_hint_shown(hint: String, worker: Worker<'_>) -> Result<Vec<String>, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.mark_hint_shown(&hint)).await
}

/// `reset_hints_shown`.
#[tauri::command]
pub async fn reset_hints_shown(worker: Worker<'_>) -> Result<Vec<String>, AppError> {
    let worker = Arc::clone(worker.inner());
    blocking(move || worker.reset_hints_shown()).await
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
