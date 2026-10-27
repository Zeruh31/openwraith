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
  BackupFileSummary,
  BackupKind,
  BackupSaved,
  CloseChoice,
  DeviceId,
  DeviceSnapshot,
  DeviceStatus,
  DialogText,
  EditReportView,
  FileRef,
  FileToken,
  LastReport,
  LightEditIntent,
  LightEditPreview,
  OpId,
  PlanToken,
  ProfileSwitchPreview,
  ProfileSwitchReport,
  ProgressEvent,
  RestoreIntent,
  RestorePreview,
  RestoreReportView,
  RevealTarget,
  SafetyBackupEntry,
  Settings,
  SimAction,
  SimInfo,
  TriggerEditIntent,
  TriggerEditPreview,
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
  | "apply_restore";

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
