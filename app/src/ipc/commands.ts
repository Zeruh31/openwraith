// The typed IPC layer: the only importer of `@tauri-apps/api/core`.
//
// Command names, argument names and types mirror the Rust commands in
// `app/src-tauri/src/commands.rs`; the names must equal
// `app/src-tauri/src/command_names.rs` (checked by the core's tests). Every
// argument name is a single word, so Tauri's case conversion changes
// nothing. Rejections are `AppError`s (see `errors.ts`).

import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  AllProfilesPreview,
  AppEvent,
  AppInfo,
  AutoBackupsView,
  AutoProfileRulesIntent,
  AutoProfileStateView,
  BackgroundSettingsApplied,
  BackgroundSettingsIntent,
  BackgroundSettingsView,
  BackupFileSummary,
  BackupKind,
  BackupSaved,
  CloseChoice,
  ColorEditIntent,
  ColorEditPreview,
  ColorsState,
  DeadBandEditIntent,
  DeadBandEditPreview,
  DeadBandState,
  DepthFrame,
  DepthStopRequest,
  DepthViewReport,
  DeviceId,
  DeviceSnapshot,
  DeviceStatus,
  DialogText,
  EditReportView,
  ExportIntent,
  ExportedProfile,
  FileRef,
  FileToken,
  ImportIntent,
  ImportPreview,
  ImportSummaryView,
  KeymapEditIntent,
  KeymapEditPreview,
  KeysState,
  LastReport,
  LightEditIntent,
  LightEditPreview,
  MacroLibraryView,
  MacroProgramView,
  OpId,
  PlanToken,
  PresetEditIntent,
  PresetsView,
  ProfileNamesIntent,
  ProfileNamesView,
  ProfileSwitchPreview,
  ProfileSwitchReport,
  ProgramView,
  ProgressEvent,
  RecoveryIntent,
  RecoveryView,
  RestoreIntent,
  RestorePreview,
  RestoreReportView,
  RevealTarget,
  SafetyBackupEntry,
  Settings,
  SimAction,
  SimInfo,
  SocdEditIntent,
  SocdEditPreview,
  SocdStateView,
  TravelCloseReport,
  TriggerEditIntent,
  TriggerEditPreview,
  UiStateReport,
  UserPresetIntent,
} from "./bindings";
import { toAppError } from "./errors";

/** The commands of every build (`command_names.rs`: `COMMANDS`). */
export const COMMAND_NAMES = [
  "app_info",
  "get_settings",
  "set_settings",
  "subscribe",
  "get_status",
  "get_snapshot",
  "get_last_report",
  "cancel_operation",
  "answer_close",
  "dismiss_interrupted",
  "list_safety_backups",
  "open_safety_backup",
  "open_backup_file",
  "choose_backup_destination",
  "reveal",
  "refresh",
  "select_device",
  "reconnect",
  "disconnect",
  "recheck_other_program",
  "prepare_profile_switch",
  "apply_profile_switch",
  "prepare_trigger_edit",
  "apply_trigger_edit",
  "prepare_light_edit",
  "apply_light_edit",
  "backup_active_profile",
  "prepare_all_profile_backup",
  "apply_all_profile_backup",
  "prepare_restore",
  "apply_restore",
  "discard_plan",
  "get_keys_state",
  "prepare_keymap_edit",
  "apply_keymap_edit",
  "list_macro_library",
  "save_library_macro",
  "rename_library_macro",
  "delete_library_macro",
  "get_colors_state",
  "prepare_color_edit",
  "apply_color_edit",
  "get_dead_band_state",
  "prepare_dead_band_edit",
  "apply_dead_band_edit",
  "get_socd_state",
  "prepare_socd_edit",
  "apply_socd_edit",
  "list_presets",
  "save_user_preset",
  "delete_user_preset",
  "prepare_preset_edit",
  "start_depth_view",
  "stop_depth_view",
  "close_stray_travel_test",
  "choose_export_destination",
  "export_profile",
  "open_import_file",
  "prepare_import",
  "apply_import",
  "get_hints_shown",
  "mark_hint_shown",
  "reset_hints_shown",
  "list_auto_backups",
  "pin_auto_backup",
  "get_recovery",
  "keep_current_settings",
  "prepare_recovery",
  "get_background_settings",
  "set_background_settings",
  "get_profile_names",
  "set_profile_names",
  "get_auto_profile_state",
  "set_auto_profile_rules",
  "pause_auto_profile",
  "list_running_programs",
  "pick_game_program",
  "detect_next_program",
  "report_ui_state",
] as const;

/** The developer simulator's commands (`command_names.rs`: `DEV_COMMANDS`). */
export const DEV_COMMAND_NAMES = ["dev_sim_action", "dev_sim_info"] as const;

/** Every command name. */
export type CommandName = (typeof COMMAND_NAMES)[number] | (typeof DEV_COMMAND_NAMES)[number];

/**
 * The arguments of each command, without its channel (`subscribe` takes a
 * `channel`, the operations below `ProgressCommand` a `progress` channel).
 */
export type Args = {
  app_info: Record<string, never>;
  get_settings: Record<string, never>;
  set_settings: { settings: Settings };
  subscribe: Record<string, never>;
  get_status: Record<string, never>;
  get_snapshot: Record<string, never>;
  get_last_report: Record<string, never>;
  cancel_operation: { op: OpId };
  answer_close: { choice: CloseChoice };
  dismiss_interrupted: Record<string, never>;
  list_safety_backups: Record<string, never>;
  open_safety_backup: { file: FileToken };
  open_backup_file: { dialog: DialogText };
  choose_backup_destination: { kind: BackupKind; dialog: DialogText };
  reveal: { target: RevealTarget };
  refresh: Record<string, never>;
  select_device: { id: DeviceId };
  reconnect: Record<string, never>;
  disconnect: Record<string, never>;
  recheck_other_program: Record<string, never>;
  prepare_profile_switch: { profile: number };
  apply_profile_switch: { token: PlanToken };
  prepare_trigger_edit: { intent: TriggerEditIntent };
  apply_trigger_edit: { token: PlanToken };
  prepare_light_edit: { intent: LightEditIntent };
  apply_light_edit: { token: PlanToken };
  backup_active_profile: { destination: FileToken };
  prepare_all_profile_backup: { destination: FileToken };
  apply_all_profile_backup: { token: PlanToken };
  prepare_restore: { intent: RestoreIntent };
  apply_restore: { token: PlanToken };
  discard_plan: { token: PlanToken };
  get_keys_state: Record<string, never>;
  prepare_keymap_edit: { intent: KeymapEditIntent };
  apply_keymap_edit: { token: PlanToken };
  list_macro_library: Record<string, never>;
  save_library_macro: { name: string; program: MacroProgramView };
  rename_library_macro: { hash: string; name: string };
  delete_library_macro: { hash: string };
  get_colors_state: Record<string, never>;
  prepare_color_edit: { intent: ColorEditIntent };
  apply_color_edit: { token: PlanToken };
  get_dead_band_state: Record<string, never>;
  prepare_dead_band_edit: { intent: DeadBandEditIntent };
  apply_dead_band_edit: { token: PlanToken };
  get_socd_state: Record<string, never>;
  prepare_socd_edit: { intent: SocdEditIntent };
  apply_socd_edit: { token: PlanToken };
  list_presets: Record<string, never>;
  save_user_preset: { preset: UserPresetIntent };
  delete_user_preset: { id: string };
  prepare_preset_edit: { intent: PresetEditIntent };
  start_depth_view: Record<string, never>;
  stop_depth_view: { reason: DepthStopRequest };
  close_stray_travel_test: Record<string, never>;
  choose_export_destination: { profile: number; dialog: DialogText };
  export_profile: { intent: ExportIntent };
  open_import_file: { dialog: DialogText };
  prepare_import: { intent: ImportIntent };
  apply_import: { token: PlanToken };
  get_hints_shown: Record<string, never>;
  mark_hint_shown: { hint: string };
  reset_hints_shown: Record<string, never>;
  list_auto_backups: Record<string, never>;
  pin_auto_backup: { file: FileToken; pinned: boolean };
  get_recovery: Record<string, never>;
  keep_current_settings: { profiles: number[] };
  prepare_recovery: { intent: RecoveryIntent };
  get_background_settings: Record<string, never>;
  set_background_settings: { settings: BackgroundSettingsIntent };
  get_profile_names: Record<string, never>;
  set_profile_names: { names: ProfileNamesIntent };
  get_auto_profile_state: Record<string, never>;
  set_auto_profile_rules: { rules: AutoProfileRulesIntent };
  pause_auto_profile: { paused: boolean };
  list_running_programs: Record<string, never>;
  pick_game_program: { dialog: DialogText };
  detect_next_program: Record<string, never>;
  report_ui_state: { state: UiStateReport };
  dev_sim_action: { action: SimAction };
  dev_sim_info: Record<string, never>;
};

/** The result of each command (`null` for Rust's `()`). */
export type Results = {
  app_info: AppInfo;
  get_settings: Settings;
  set_settings: Settings;
  subscribe: DeviceStatus;
  get_status: DeviceStatus;
  get_snapshot: DeviceSnapshot | null;
  get_last_report: LastReport | null;
  cancel_operation: boolean;
  answer_close: null;
  dismiss_interrupted: null;
  list_safety_backups: SafetyBackupEntry[];
  open_safety_backup: BackupFileSummary;
  open_backup_file: BackupFileSummary | null;
  choose_backup_destination: FileRef | null;
  reveal: null;
  refresh: DeviceSnapshot;
  select_device: null;
  reconnect: null;
  disconnect: null;
  recheck_other_program: null;
  prepare_profile_switch: ProfileSwitchPreview;
  apply_profile_switch: ProfileSwitchReport;
  prepare_trigger_edit: TriggerEditPreview;
  apply_trigger_edit: EditReportView;
  prepare_light_edit: LightEditPreview;
  apply_light_edit: EditReportView;
  backup_active_profile: BackupSaved;
  prepare_all_profile_backup: AllProfilesPreview;
  apply_all_profile_backup: BackupSaved;
  prepare_restore: RestorePreview;
  apply_restore: RestoreReportView;
  discard_plan: null;
  get_keys_state: KeysState | null;
  prepare_keymap_edit: KeymapEditPreview;
  apply_keymap_edit: EditReportView;
  list_macro_library: MacroLibraryView;
  save_library_macro: MacroLibraryView;
  rename_library_macro: MacroLibraryView;
  delete_library_macro: MacroLibraryView;
  get_colors_state: ColorsState | null;
  prepare_color_edit: ColorEditPreview;
  apply_color_edit: EditReportView;
  get_dead_band_state: DeadBandState | null;
  prepare_dead_band_edit: DeadBandEditPreview;
  apply_dead_band_edit: EditReportView;
  get_socd_state: SocdStateView | null;
  prepare_socd_edit: SocdEditPreview;
  apply_socd_edit: EditReportView;
  list_presets: PresetsView;
  save_user_preset: PresetsView;
  delete_user_preset: PresetsView;
  prepare_preset_edit: TriggerEditPreview;
  start_depth_view: DepthViewReport;
  stop_depth_view: boolean;
  close_stray_travel_test: TravelCloseReport;
  choose_export_destination: FileRef | null;
  export_profile: ExportedProfile;
  open_import_file: ImportSummaryView | null;
  prepare_import: ImportPreview;
  apply_import: RestoreReportView;
  get_hints_shown: string[];
  mark_hint_shown: string[];
  reset_hints_shown: string[];
  list_auto_backups: AutoBackupsView;
  pin_auto_backup: AutoBackupsView;
  get_recovery: RecoveryView;
  keep_current_settings: RecoveryView;
  prepare_recovery: RestorePreview;
  get_background_settings: BackgroundSettingsView;
  set_background_settings: BackgroundSettingsApplied;
  get_profile_names: ProfileNamesView;
  set_profile_names: ProfileNamesView;
  get_auto_profile_state: AutoProfileStateView;
  set_auto_profile_rules: AutoProfileStateView;
  pause_auto_profile: AutoProfileStateView;
  list_running_programs: ProgramView[];
  pick_game_program: ProgramView | null;
  detect_next_program: ProgramView | null;
  report_ui_state: null;
  dev_sim_action: SimInfo;
  dev_sim_info: SimInfo;
};

/** The commands that stream progress on a `progress` channel. */
export type ProgressCommand =
  | "refresh"
  | "apply_profile_switch"
  | "apply_trigger_edit"
  | "apply_light_edit"
  | "backup_active_profile"
  | "apply_all_profile_backup"
  | "prepare_restore"
  | "apply_restore"
  | "apply_keymap_edit"
  | "apply_color_edit"
  | "apply_dead_band_edit"
  | "apply_socd_edit"
  | "export_profile"
  | "prepare_import"
  | "apply_import"
  | "prepare_recovery";

/** A progress listener. */
export type OnProgress = (event: ProgressEvent) => void;

/** Invokes `command`; rejections become `AppError`s. */
async function call<K extends CommandName>(command: K, args: Args[K]): Promise<Results[K]> {
  try {
    return await invoke<Results[K]>(command, args);
  } catch (error) {
    throw toAppError(error);
  }
}

/** Invokes `command` with a channel under `key`. */
// eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters -- E ties the channel's type to the listener
async function callWithChannel<K extends CommandName, E>(
  command: K,
  args: Args[K],
  key: "channel" | "progress",
  onEvent: (event: E) => void,
): Promise<Results[K]> {
  const channel = new Channel<E>();
  channel.onmessage = onEvent;
  try {
    return await invoke<Results[K]>(command, { ...(args as Record<string, unknown>), [key]: channel });
  } catch (error) {
    throw toAppError(error);
  }
}

/** One function per command. */
export const api = {
  appInfo: () => call("app_info", {}),
  getSettings: () => call("get_settings", {}),
  setSettings: (settings: Settings) => call("set_settings", { settings }),
  subscribe: (onEvent: (event: AppEvent) => void) =>
    callWithChannel("subscribe", {}, "channel", onEvent),
  getStatus: () => call("get_status", {}),
  getSnapshot: () => call("get_snapshot", {}),
  getLastReport: () => call("get_last_report", {}),
  cancelOperation: (op: OpId) => call("cancel_operation", { op }),
  answerClose: (choice: CloseChoice) => call("answer_close", { choice }),
  dismissInterrupted: () => call("dismiss_interrupted", {}),
  listSafetyBackups: () => call("list_safety_backups", {}),
  /** Loads a safety backup, or summarizes again any backup file token (compares it with the keyboard read last). */
  openSafetyBackup: (file: FileToken) => call("open_safety_backup", { file }),
  openBackupFile: (dialog: DialogText) => call("open_backup_file", { dialog }),
  chooseBackupDestination: (kind: BackupKind, dialog: DialogText) =>
    call("choose_backup_destination", { kind, dialog }),
  reveal: (target: RevealTarget) => call("reveal", { target }),
  refresh: (onProgress: OnProgress) => callWithChannel("refresh", {}, "progress", onProgress),
  selectDevice: (id: DeviceId) => call("select_device", { id }),
  reconnect: () => call("reconnect", {}),
  disconnect: () => call("disconnect", {}),
  recheckOtherProgram: () => call("recheck_other_program", {}),
  prepareProfileSwitch: (profile: number) => call("prepare_profile_switch", { profile }),
  applyProfileSwitch: (token: PlanToken, onProgress: OnProgress) =>
    callWithChannel("apply_profile_switch", { token }, "progress", onProgress),
  prepareTriggerEdit: (intent: TriggerEditIntent) => call("prepare_trigger_edit", { intent }),
  applyTriggerEdit: (token: PlanToken, onProgress: OnProgress) =>
    callWithChannel("apply_trigger_edit", { token }, "progress", onProgress),
  prepareLightEdit: (intent: LightEditIntent) => call("prepare_light_edit", { intent }),
  applyLightEdit: (token: PlanToken, onProgress: OnProgress) =>
    callWithChannel("apply_light_edit", { token }, "progress", onProgress),
  backupActiveProfile: (destination: FileToken, onProgress: OnProgress) =>
    callWithChannel("backup_active_profile", { destination }, "progress", onProgress),
  prepareAllProfileBackup: (destination: FileToken) =>
    call("prepare_all_profile_backup", { destination }),
  applyAllProfileBackup: (token: PlanToken, onProgress: OnProgress) =>
    callWithChannel("apply_all_profile_backup", { token }, "progress", onProgress),
  prepareRestore: (intent: RestoreIntent, onProgress: OnProgress) =>
    callWithChannel("prepare_restore", { intent }, "progress", onProgress),
  applyRestore: (token: PlanToken, onProgress: OnProgress) =>
    callWithChannel("apply_restore", { token }, "progress", onProgress),
  discardPlan: (token: PlanToken) => call("discard_plan", { token }),
  /** The keymap group of the last full read (no traffic); refetch when `snapshot_revision` changes. */
  getKeysState: () => call("get_keys_state", {}),
  prepareKeymapEdit: (intent: KeymapEditIntent) => call("prepare_keymap_edit", { intent }),
  applyKeymapEdit: (token: PlanToken, onProgress: OnProgress) =>
    callWithChannel("apply_keymap_edit", { token }, "progress", onProgress),
  listMacroLibrary: () => call("list_macro_library", {}),
  /** Adds a macro to the library of this PC, or renames the entry with the same program. */
  saveLibraryMacro: (name: string, program: MacroProgramView) => call("save_library_macro", { name, program }),
  renameLibraryMacro: (hash: string, name: string) => call("rename_library_macro", { hash, name }),
  deleteLibraryMacro: (hash: string) => call("delete_library_macro", { hash }),
  /** Custom colour table 1 of the last full read (no traffic); refetch after a colour edit and when `snapshot_revision` changes. */
  getColorsState: () => call("get_colors_state", {}),
  prepareColorEdit: (intent: ColorEditIntent) => call("prepare_color_edit", { intent }),
  applyColorEdit: (token: PlanToken, onProgress: OnProgress) =>
    callWithChannel("apply_color_edit", { token }, "progress", onProgress),
  /** The dead band of the last full read (no traffic); refetch after a dead-band edit and when `snapshot_revision` changes. */
  getDeadBandState: () => call("get_dead_band_state", {}),
  prepareDeadBandEdit: (intent: DeadBandEditIntent) => call("prepare_dead_band_edit", { intent }),
  applyDeadBandEdit: (token: PlanToken, onProgress: OnProgress) =>
    callWithChannel("apply_dead_band_edit", { token }, "progress", onProgress),
  /** SOCD of the last full read (no traffic); refetch after a SOCD edit, on `socd_toggled_on_keyboard` and when `snapshot_revision` changes. */
  getSocdState: () => call("get_socd_state", {}),
  prepareSocdEdit: (intent: SocdEditIntent) => call("prepare_socd_edit", { intent }),
  applySocdEdit: (token: PlanToken, onProgress: OnProgress) =>
    callWithChannel("apply_socd_edit", { token }, "progress", onProgress),
  listPresets: () => call("list_presets", {}),
  /** Saves a user preset (values outside the keyboard's limits are refused, never clamped). */
  saveUserPreset: (preset: UserPresetIntent) => call("save_user_preset", { preset }),
  deleteUserPreset: (id: string) => call("delete_user_preset", { id }),
  /** A preset as a trigger plan; `applyTriggerEdit` writes it. */
  preparePresetEdit: (intent: PresetEditIntent) => call("prepare_preset_edit", { intent }),
  /** Runs the live key-depth view; resolves with its report when it stopped. */
  startDepthView: (onFrame: (frame: DepthFrame) => void) =>
    callWithChannel("start_depth_view", {}, "channel", onFrame),
  stopDepthView: (reason: DepthStopRequest) => call("stop_depth_view", { reason }),
  /** Sends the close of a key-depth test left open (a user click only). */
  closeStrayTravelTest: () => call("close_stray_travel_test", {}),
  chooseExportDestination: (profile: number, dialog: DialogText) =>
    call("choose_export_destination", { profile, dialog }),
  exportProfile: (intent: ExportIntent, onProgress: OnProgress) =>
    callWithChannel("export_profile", { intent }, "progress", onProgress),
  openImportFile: (dialog: DialogText) => call("open_import_file", { dialog }),
  prepareImport: (intent: ImportIntent, onProgress: OnProgress) =>
    callWithChannel("prepare_import", { intent }, "progress", onProgress),
  applyImport: (token: PlanToken, onProgress: OnProgress) =>
    callWithChannel("apply_import", { token }, "progress", onProgress),
  getHintsShown: () => call("get_hints_shown", {}),
  markHintShown: (hint: string) => call("mark_hint_shown", { hint }),
  resetHintsShown: () => call("reset_hints_shown", {}),
  /** The automatic backups of the keyboard read last (else every keyboard's), newest first. */
  listAutoBackups: () => call("list_auto_backups", {}),
  /** Keeps (`pinned`) or no longer keeps an automatic backup; a pinned one is never deleted. */
  pinAutoBackup: (file: FileToken, pinned: boolean) => call("pin_auto_backup", { file, pinned }),
  /** The Recovery card (no keyboard traffic); refetch on `reset_suspected`, `firmware_changed` and status changes. */
  getRecovery: () => call("get_recovery", {}),
  /** "It's fine, keep them": the held reads become the automatic backups (every held profile when empty). */
  keepCurrentSettings: (profiles: number[]) => call("keep_current_settings", { profiles }),
  /** Compares automatic backups with the keyboard (one per profile; several are composed); `applyRestore` writes the plan. */
  prepareRecovery: (intent: RecoveryIntent, onProgress: OnProgress) =>
    callWithChannel("prepare_recovery", { intent }, "progress", onProgress),
  getBackgroundSettings: () => call("get_background_settings", {}),
  /** Saves the background settings; the result says what the shell changed (tray, start with Windows). */
  setBackgroundSettings: (settings: BackgroundSettingsIntent) => call("set_background_settings", { settings }),
  getProfileNames: () => call("get_profile_names", {}),
  setProfileNames: (names: ProfileNamesIntent) => call("set_profile_names", { names }),
  /** Settings > Games: the rules, the live status line and the recent activity (no keyboard traffic). */
  getAutoProfileState: () => call("get_auto_profile_state", {}),
  /** Replaces the per-game rules (programs by token, by typed file name, or kept from the rule with the same id). */
  setAutoProfileRules: (rules: AutoProfileRulesIntent) => call("set_auto_profile_rules", { rules }),
  /** Pauses (`true`) or resumes the automatic profiles for this run. */
  pauseAutoProfile: (paused: boolean) => call("pause_auto_profile", { paused }),
  /** The running programs, each with a token for a rule. */
  listRunningPrograms: () => call("list_running_programs", {}),
  /** The open dialog for a program (`*.exe`); `null` when cancelled. */
  pickGameProgram: (dialog: DialogText) => call("pick_game_program", { dialog }),
  /** Resolves with the next program that comes to the front within 30 s, else `null`. */
  detectNextProgram: () => call("detect_next_program", {}),
  /** Tells the backend about drafts and this run's vendor-site acknowledgement (send on every change; reset by `subscribe`). */
  reportUiState: (state: UiStateReport) => call("report_ui_state", { state }),
};

/**
 * The developer simulator's commands (simulator builds only). Kept apart
 * from `api`, so that a production bundle, which never renders the
 * simulator panel, contains none of them.
 */
export const devApi = {
  devSimAction: (action: SimAction) => call("dev_sim_action", { action }),
  devSimInfo: () => call("dev_sim_info", {}),
};
