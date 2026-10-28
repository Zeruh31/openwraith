// The browser preview's backend: answers the app's commands from a recorded
// fixture (recorded by the core's tests from the real worker on the
// simulator), replays progress on the real `Channel` through
// `__TAURI_INTERNALS__.runCallback` (the same ordering path as the real IPC)
// and keeps a small status model so that busy states, profile switches and
// banners behave like the app's.
//
// Rules:
// - a command answers with the fixture's next recorded call of that command
//   (the last one again when they are used up);
// - commands the fixture didn't record are answered from its state
//   (status, snapshot, settings, safety backups), synthesized (profile
//   switch, reconnect, simulator actions) or taken from another fixture
//   (`fallback`);
// - a recording may start with calls that set the scene (saved game rules,
//   background settings, profile names): their results are the starting
//   state, the UI never replays them;
// - the views of local settings and of the automatic-backup store
//   (`VIEW_GETTERS`) are kept in memory: a getter answers with the recorded
//   answer that is due (by the fixture's virtual time, and only after the
//   recorded user actions before it were replayed), and once the user changed
//   the view (synthesized setters: background settings, profile names, game
//   rules, pause, pins, "keep them") with the changed view;
// - events recorded before the first user action are replayed at
//   `subscribe` on their recorded timeline (things that happen on their own,
//   such as an automatic switch); the rest are replayed after the last
//   recorded user action (a recording of views only: once its last view is
//   due), so the app ends in the recorded final state;
// - the live key-depth view replays the frames the fixture recorded
//   (`ui.depth_frames`) on its channel and resolves with the recorded report
//   when the page stops it (or at once for a stop the backend decided);
// - the macro library of "this PC" is kept in memory, starting from the
//   recorded one.

import type {
  AppError,
  AppEvent,
  AutoActivityView,
  AutoBackupsView,
  AutoEventView,
  AutoLineView,
  AutoPauseReason,
  AutoProfileRulesIntent,
  AutoProfileRuleView,
  AutoProfileStateView,
  BackgroundSettingsIntent,
  BackgroundSettingsView,
  DepthFrame,
  DepthStopRequest,
  DepthViewReport,
  DeviceSnapshot,
  DeviceStatus,
  KeysState,
  LastReport,
  MacroLibraryView,
  MacroProgramView,
  OpKind,
  ProfileNamesIntent,
  ProfileNamesView,
  ProgramView,
  ProgressEvent,
  ProgressStep,
  RecoveryIntent,
  RecoveryView,
  SimAction,
  SimInfo,
  WriteBlockReason,
} from "../ipc/bindings";
import type { CommandName } from "../ipc/commands";
import type { Fixture, Recorded } from "./types";

/** How the mock behaves. */
export interface MockOptions {
  /** Time factor: 1 = recorded time, 4 = four times faster, 0 = at once. */
  speed: number;
  /** Replay the fixture's events after `subscribe` (fixtures without calls). */
  replayEvents: boolean;
  /** Answer unrecorded commands from other fixtures. */
  fallback: boolean;
  /** The fixtures to fall back on. */
  library: Readonly<Record<string, Fixture>>;
  /** Delivers a channel message (default: `__TAURI_INTERNALS__.runCallback`). */
  deliver?: ((channelId: number, data: unknown) => void) | undefined;
}

/** The mock's command handler for `mockIPC`. */
export interface MockBackend {
  handle(command: string, args: unknown): Promise<unknown>;
  /** The current status (tests). */
  status(): DeviceStatus;
}

interface ChannelRef {
  id: number;
}

interface TauriInternals {
  runCallback(id: number, data: unknown): void;
}

function defaultDeliver(channelId: number, data: unknown): void {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: TauriInternals }).__TAURI_INTERNALS__;
  internals?.runCallback(channelId, data);
}

function isChannel(value: unknown): value is ChannelRef {
  return typeof value === "object" && value !== null && typeof (value as { id?: unknown }).id === "number";
}

function argsRecord(args: unknown): Record<string, unknown> {
  return typeof args === "object" && args !== null && !Array.isArray(args) ? (args as Record<string, unknown>) : {};
}

function clone<T>(value: T): T {
  return structuredClone(value);
}

const OP_KINDS: Partial<Record<CommandName, OpKind>> = {
  refresh: "refresh",
  prepare_profile_switch: "profile_switch",
  apply_profile_switch: "profile_switch",
  prepare_trigger_edit: "trigger_edit",
  apply_trigger_edit: "trigger_edit",
  prepare_light_edit: "light_edit",
  apply_light_edit: "light_edit",
  backup_active_profile: "backup_active",
  prepare_all_profile_backup: "backup_all",
  apply_all_profile_backup: "backup_all",
  prepare_restore: "restore_prepare",
  apply_restore: "restore",
  prepare_keymap_edit: "keymap_edit",
  apply_keymap_edit: "keymap_edit",
  prepare_color_edit: "color_edit",
  apply_color_edit: "color_edit",
  prepare_dead_band_edit: "dead_band_edit",
  apply_dead_band_edit: "dead_band_edit",
  prepare_socd_edit: "socd_edit",
  apply_socd_edit: "socd_edit",
  prepare_preset_edit: "trigger_edit",
  close_stray_travel_test: "travel_close",
  export_profile: "export",
  prepare_import: "import_prepare",
  apply_import: "import",
  prepare_recovery: "recovery_check",
};

/**
 * Views of local settings and of the automatic-backup store: kept in memory,
 * answered by virtual time and changed by the (synthesized) setters.
 */
const VIEW_GETTERS = [
  "get_recovery",
  "list_auto_backups",
  "get_background_settings",
  "get_profile_names",
  "get_auto_profile_state",
] as const;

/** A view kept in memory. */
type ViewCommand = (typeof VIEW_GETTERS)[number];

/** Calls a recording may start with to set the scene: their results are the starting state. */
const SEED_COMMANDS: readonly string[] = [
  "set_auto_profile_rules",
  "pause_auto_profile",
  "set_background_settings",
  "set_profile_names",
  "report_ui_state",
];

/** Program names many programs share (a mirror of app-core's `auto/programs.rs`). */
const GENERIC_NAMES: readonly string[] = [
  "javaw.exe",
  "java.exe",
  "python.exe",
  "pythonw.exe",
  "py.exe",
  "node.exe",
  "electron.exe",
  "dotnet.exe",
  "steam.exe",
  "steamwebhelper.exe",
  "epicgameslauncher.exe",
  "battle.net.exe",
  "eadesktop.exe",
  "galaxyclient.exe",
  "launcher.exe",
  "game.exe",
  "start.exe",
  "explorer.exe",
  "cmd.exe",
  "powershell.exe",
  "pwsh.exe",
  "conhost.exe",
  "windowsterminal.exe",
  "rundll32.exe",
  "dllhost.exe",
  "svchost.exe",
  "applicationframehost.exe",
  "wscript.exe",
  "mshta.exe",
  "msedge.exe",
  "chrome.exe",
  "firefox.exe",
];

/** Most game rules (as app-core). */
const MAX_RULES = 64;

/** Longest profile name (as app-core). */
const MAX_PROFILE_NAME = 32;

/** JSON with sorted keys (recorded arguments come from serde in another key order). */
function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(",")}]`;
  if (typeof value === "object" && value !== null) {
    const entries = Object.entries(value as Record<string, unknown>)
      .filter(([, v]) => v !== undefined)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
    return `{${entries.map(([k, v]) => `${JSON.stringify(k)}:${stableJson(v)}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

/** Text without control characters. */
function withoutControls(text: string): string {
  return text.replace(/\p{Cc}/gu, "");
}

/** The line of the Games tab for a state (as app-core's engine, without a game in front). */
function lineOf(enabled: boolean, paused: AutoPauseReason | null, rules: readonly AutoProfileRuleView[]): AutoLineView {
  if (!enabled) return { line: "off" };
  if (paused !== null) return { line: "paused", reason: paused };
  return rules.some((rule) => rule.enabled) ? { line: "watching" } : { line: "no_rules" };
}

/** Commands that only read the app's state: unrecorded, they answer `null` while no keyboard was read. */
const STATE_VIEWS: readonly CommandName[] = ["get_keys_state", "get_colors_state", "get_dead_band_state", "get_socd_state"];

/** Stops the page asks for: the recorded view waits for one of them. */
const PAGE_STOPS: readonly string[] = ["requested", "page_left", "window_hidden"];

/** A stable 16-hex-digit hash for macros saved in the preview (FNV-1a 64 over the program's JSON). */
function previewHash(program: MacroProgramView): string {
  let hash = 0xcbf29ce484222325n;
  for (const char of JSON.stringify(program)) {
    hash ^= BigInt(char.charCodeAt(0));
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return hash.toString(16).padStart(16, "0");
}

/** Fixtures preferred as fallbacks (successful flows first). */
const FALLBACK_ORDER = [
  "trigger_apply_ok",
  "light_apply_ok",
  "backup_active_ok",
  "backup_all_ok",
  "restore_ok_persisted",
  "safety_list",
  "undo_preview",
  "profile_switch",
  "keys_ready",
  "keys_apply_ok",
  "colors_ready",
  "colors_apply_ok",
  "dead_band_ready",
  "dead_band_apply_ok",
  "socd_ready_pairs",
  "socd_apply_ok",
  "presets_ready",
  "depth_live",
  "depth_stray_marker",
  "export_done",
  "import_apply_ok",
  "auto_backups_list",
  "recovery_nothing_wrong",
  "settings_background",
  "games_settings",
  "export_options",
];

function error(kind: AppError["kind"], message: string): AppError {
  return { kind, detail: null, message };
}

/** A refused local change (as app-core's `refused`: `invalid_request`, nothing written). */
function refused(problem: string, what: string): AppError {
  return {
    kind: "invalid_request",
    detail: { detail: "edit", problem: { problem: "other", message: problem } },
    message: `${what}; nothing was written`,
  };
}

/** Writing is blocked while an OpenWraith command holds the keyboard. */
const PAUSED_FOR_CLI: AppError = {
  kind: "writes_blocked",
  detail: { detail: "writes_blocked", reasons: ["paused_for_cli"] },
  message: "writing is not possible now; nothing was written",
};

/** A view kept in memory and the fixture index of the call it came from (`Infinity`: the user changed it). */
interface Kept<T> {
  value: T | null;
  index: number;
}

/** Creates the mock backend for one fixture. */
export function createMockBackend(source: Fixture, options: MockOptions): MockBackend {
  const fixture = clone(source);
  const deliver = options.deliver ?? defaultDeliver;
  let status = clone(fixture.initial_status);
  let snapshot: DeviceSnapshot | null = clone(fixture.initial_snapshot);
  let settings = clone(fixture.settings);
  let lastReport: LastReport | null = null;
  let subscriber: { channel: ChannelRef; index: number } | null = null;
  let opCounter = 900;
  let writesSent = 0;
  let tailPlayed = false;
  /** Events already replayed (those recorded before the first call, at `subscribe`). */
  let eventsPlayed = 0;
  let lastPreview: Recorded | null = null;
  let hints: string[] = [];
  let macroLibrary: MacroLibraryView | null = null;
  let keysSeen: KeysState | null = null;
  let depth: { finish: (reason: DepthStopRequest) => void } | null = null;
  const cursors = new Map<string, number>();
  const calls = fixture.calls;
  // The calls that set the scene (leading saved settings and rules) and the
  // user actions (every other call but the views' getters and the UI report).
  let seedEnd = 0;
  while (seedEnd < calls.length && SEED_COMMANDS.includes(calls[seedEnd]?.command ?? "")) seedEnd += 1;
  const triggers = calls.flatMap((call, index) =>
    index >= seedEnd && call.command !== "report_ui_state" && !(VIEW_GETTERS as readonly string[]).includes(call.command)
      ? [index]
      : [],
  );
  const firstTrigger = triggers[0] === undefined ? undefined : calls[triggers[0]];
  const lastTrigger = triggers.length === 0 ? undefined : calls[triggers[triggers.length - 1] ?? 0];
  /**
   * The call after which the rest of the events follow: the last user
   * action, else (a recording of views only) the last call, once it is due.
   */
  const tailCall = lastTrigger ?? calls[calls.length - 1];
  /** Fixture indexes of the recorded calls replayed so far. */
  const replayed = new Set<number>();
  /** The start of the fixture's virtual time (reset by the first `subscribe`). */
  let startedAt = Date.now();
  let subscribed = false;
  const virtualNow = (): number => (options.speed <= 0 ? Number.POSITIVE_INFINITY : (Date.now() - startedAt) * options.speed);
  // Views kept in memory.
  const recovery: Kept<RecoveryView> = { value: null, index: -1 };
  const autoBackups: Kept<AutoBackupsView> = { value: null, index: -1 };
  const background: Kept<BackgroundSettingsView> = { value: null, index: -1 };
  const profileNames: Kept<ProfileNamesView> = { value: null, index: -1 };
  const automation: Kept<AutoProfileStateView> = { value: null, index: -1 };
  /** The recording shows OpenWraith turned off in Task Manager > Startup apps. */
  const taskManagerOff = calls.some(
    (call) => call.command === "get_background_settings" && call.ok?.windows_start === "off_in_task_manager",
  );
  /** Programs answered so far, by token (for game rules). */
  const programs = new Map<string, ProgramView>();
  /** The recovery the user compared (its `apply_restore` resolves the hold). */
  let recoveryIntent: RecoveryIntent | null = null;
  for (const [index, call] of calls.slice(0, seedEnd).entries()) {
    if (call.ok === null) continue;
    if (call.command === "set_auto_profile_rules" || call.command === "pause_auto_profile") {
      automation.value = clone(call.ok);
      automation.index = index;
    } else if (call.command === "set_background_settings") {
      background.value = clone(call.ok.settings);
      background.index = index;
    } else if (call.command === "set_profile_names") {
      profileNames.value = clone(call.ok);
      profileNames.index = index;
    }
  }

  const wait = (ms: number): Promise<void> =>
    new Promise((resolve) => {
      window.setTimeout(resolve, options.speed <= 0 ? 0 : Math.max(0, ms) / options.speed);
    });

  const send = (event: AppEvent) => {
    if (subscriber === null) return;
    const index = subscriber.index;
    subscriber.index += 1;
    deliver(subscriber.channel.id, { index, message: event });
  };

  const publish = (next: DeviceStatus) => {
    status = { ...next, revision: Math.max(status.revision + 1, next.revision) };
    send({ event: "status", status });
  };

  const gateWith = (reasons: readonly WriteBlockReason[]) => {
    const unique = [...new Set(reasons)];
    return { allowed: unique.length === 0, reasons: unique };
  };

  const setBusy = (kind: OpKind, op: string) => {
    publish({
      ...status,
      busy: { op, kind, last: null },
      write_gate: gateWith([...status.write_gate.reasons, "busy"]),
    });
  };

  const setBusyLast = (event: ProgressEvent) => {
    if (status.busy !== null) publish({ ...status, busy: { ...status.busy, last: event } });
  };

  const clearBusy = () => {
    publish({
      ...status,
      busy: null,
      write_gate: gateWith(status.write_gate.reasons.filter((r) => r !== "busy")),
    });
  };

  const ready = (): DeviceStatus => {
    const reasons = status.write_gate.reasons.filter(
      (r) => r !== "not_connected" && r !== "observing" && r !== "other_program" && r !== "reset_seen" && r !== "snapshot_stale",
    );
    return {
      ...status,
      connection: { state: "ready" },
      snapshot_stale: false,
      active_profile: snapshot?.profile ?? status.active_profile ?? 1,
      device: snapshot?.device ?? status.device,
      snapshot_revision: snapshot === null ? status.snapshot_revision : (status.snapshot_revision ?? 0) + 1,
      write_gate: gateWith(reasons),
    };
  };

  const notReady = (connection: DeviceStatus["connection"], reason: WriteBlockReason): DeviceStatus => ({
    ...status,
    connection,
    write_gate: gateWith([...status.write_gate.reasons, reason]),
  });

  /** Replays progress events on the operation's channel. */
  const replayProgress = async (events: readonly ProgressEvent[], channel: ChannelRef | null) => {
    let index = 0;
    let previous = 0;
    for (const event of events) {
      await wait(event.elapsed_ms - previous);
      previous = event.elapsed_ms;
      if (channel !== null) {
        deliver(channel.id, { index, message: event });
        index += 1;
      }
      setBusyLast(event);
    }
  };

  /** Synthesized progress (for unrecorded operations). */
  const progressOf = (op: string, steps: readonly ProgressStep[], stepMs: number): ProgressEvent[] =>
    steps.map((step, i) => ({ op, seq: i + 1, elapsed_ms: i * stepMs, step, cancel: "not_cancellable" }));

  /** Replays the fixture's recorded events after its last call (the recorded final state). */
  const replayTail = async (afterMs: number) => {
    if (tailPlayed || tailCall === undefined) return;
    tailPlayed = true;
    let previous = tailCall.at_ms + afterMs;
    for (const item of fixture.events.slice(eventsPlayed)) {
      if (item.at_ms > previous) {
        await wait(item.at_ms - previous);
        previous = item.at_ms;
      }
      if (item.event.event === "status") {
        publish({ ...item.event.status, sim: status.sim });
      } else {
        send(item.event);
      }
    }
  };

  const nextRecorded = (command: string): { rec: Recorded; own: boolean } | null => {
    const own = fixture.calls.filter((call) => call.command === command);
    if (own.length > 0) {
      const index = cursors.get(command) ?? 0;
      cursors.set(command, index + 1);
      const rec = own[Math.min(index, own.length - 1)];
      return rec === undefined ? null : { rec, own: true };
    }
    if (!options.fallback) return null;
    const ids = [...FALLBACK_ORDER, ...Object.keys(options.library)];
    let failed: Recorded | null = null;
    for (const id of ids) {
      for (const call of options.library[id]?.calls ?? []) {
        if (call.command !== command) continue;
        if (call.err === null) return { rec: clone(call), own: false };
        failed ??= clone(call);
      }
    }
    return failed === null ? null : { rec: failed, own: false };
  };

  /** Applies what a successful recorded result changes in the snapshot and the last report. */
  const absorb = (rec: Recorded) => {
    switch (rec.command) {
      case "prepare_trigger_edit":
      case "prepare_preset_edit":
      case "prepare_light_edit":
        lastPreview = rec;
        break;
      case "apply_trigger_edit":
        if (rec.ok !== null) {
          lastReport = { kind: "edit", report: rec.ok };
          writesSent += rec.ok.items.length;
          if (
            rec.ok.anything_written &&
            (lastPreview?.command === "prepare_trigger_edit" || lastPreview?.command === "prepare_preset_edit") &&
            snapshot !== null
          ) {
            const preview = lastPreview.ok;
            for (const edit of preview?.keys ?? []) {
              const key = snapshot.triggers.keys.find((k) => k.key.index === edit.key.index);
              if (key !== undefined) key.value = edit.target;
            }
          }
        }
        break;
      case "apply_light_edit":
        if (rec.ok !== null) {
          lastReport = { kind: "edit", report: rec.ok };
          writesSent += 1;
          if (rec.ok.anything_written && lastPreview?.command === "prepare_light_edit" && snapshot !== null) {
            const target = lastPreview.ok?.target;
            if (target !== undefined) snapshot.light.current = target;
          }
        }
        break;
      case "apply_keymap_edit":
      case "apply_color_edit":
      case "apply_dead_band_edit":
      case "apply_socd_edit":
        if (rec.ok !== null) {
          lastReport = { kind: "edit", report: rec.ok };
          writesSent += rec.ok.items.length;
        }
        break;
      case "apply_restore":
        if (rec.ok !== null) {
          lastReport = { kind: "restore", report: rec.ok };
          const intent = recoveryIntent;
          recoveryIntent = null;
          if (intent !== null && rec.ok.anything_written) afterRecoveryRestore(intent);
        }
        break;
      case "apply_import":
        if (rec.ok !== null) lastReport = { kind: "import", report: rec.ok };
        break;
      case "close_stray_travel_test":
        if (rec.ok?.close.close === "acked") {
          publish({ ...status, travel: { state: "closed", stray_reports: 0, marker_left: false } });
        }
        break;
      case "get_keys_state":
        keysSeen = rec.ok;
        break;
      case "apply_profile_switch":
        if (rec.ok !== null) {
          lastReport = { kind: "profile_switch", report: rec.ok };
          if (snapshot !== null && rec.ok.active_profile_now !== null) snapshot.profile = rec.ok.active_profile_now;
        }
        break;
      case "backup_active_profile":
      case "apply_all_profile_backup":
        if (rec.ok !== null) lastReport = { kind: "backup", saved: rec.ok };
        break;
      case "refresh":
        if (rec.ok !== null) snapshot = clone(rec.ok);
        break;
      default:
        break;
    }
  };

  /** Answers with a recorded call: busy, progress, result, and the tail after the last one. */
  const replay = async (command: CommandName, found: { rec: Recorded; own: boolean }, channel: ChannelRef | null) => {
    const { rec } = found;
    const kind = OP_KINDS[command];
    const op = rec.progress[0]?.op ?? `o-${opCounter++}`;
    if (kind !== undefined) setBusy(kind, op);
    if (found.own) replayed.add(calls.indexOf(rec));
    if (rec.progress.length > 0) {
      await replayProgress(rec.progress, channel);
    } else {
      // Detecting the next program waits for it to start (up to 30 s in the app).
      await wait(kind !== undefined ? 250 : command === "detect_next_program" ? 1500 : 0);
    }
    if (kind !== undefined) clearBusy();
    if (rec.err === null) absorb(rec);
    if (found.own && rec === tailCall) {
      const duration = rec.progress[rec.progress.length - 1]?.elapsed_ms ?? 0;
      void replayTail(duration);
    }
    if (rec.err !== null) throw rec.err;
    return rec.ok;
  };

  const simInfo = (): SimInfo => ({
    active_profile: status.active_profile ?? 1,
    unsaved_edits: false,
    discarded_edits: 0,
    writes_sent: writesSent,
    violations: 0,
    unplugged: status.connection.state === "searching" || status.connection.state === "lost",
  });

  const reconnectFlow = async () => {
    if (snapshot === null) {
      publish(notReady({ state: "searching" }, "not_connected"));
      return;
    }
    publish({ ...notReady({ state: "observing", required_ms: 2000 }, "observing"), busy: null });
    await wait(2000);
    publish(ready());
    send({ event: "notice", notice: { notice: "connected" } });
  };

  const simAction = async (action: SimAction): Promise<SimInfo> => {
    switch (action.action) {
      case "unplug":
        publish({
          ...notReady({ state: "lost", error: error("disconnected", "simulated unplug") }, "not_connected"),
          snapshot_revision: null,
        });
        send({ event: "notice", notice: { notice: "disconnected" } });
        break;
      case "replug":
        void reconnectFlow();
        break;
      case "vendor_heartbeat":
        publish({
          ...notReady(
            { state: "other_program", evidence: { status_reports: action.count, foreign_acks: 0, profile_reports: 0 } },
            "other_program",
          ),
          snapshot_stale: true,
        });
        send({ event: "notice", notice: { notice: "plan_invalidated", reason: "other_program" } });
        break;
      case "keyboard_switch":
        await wait(action.after_ms);
        if (action.report) {
          const from = status.active_profile;
          if (snapshot !== null) snapshot.profile = action.profile;
          publish({ ...ready(), active_profile: action.profile });
          send({ event: "notice", notice: { notice: "profile_changed_on_keyboard", from, to: action.profile } });
        }
        break;
      case "keyboard_reset":
        publish(notReady({ state: "reset_seen" }, "reset_seen"));
        send({ event: "notice", notice: { notice: "plan_invalidated", reason: "reset_seen" } });
        break;
      case "slow_save":
      case "drop_next_ack":
      case "unplug_after_packets":
        break;
    }
    return simInfo();
  };

  const snapshotView = (): DeviceSnapshot | null => {
    if (snapshot === null || status.snapshot_revision === null) return snapshot === null ? null : clone(snapshot);
    return {
      ...clone(snapshot),
      revision: status.snapshot_revision,
      generation: status.generation,
      profile: status.active_profile ?? snapshot.profile,
    };
  };

  const requireReady = () => {
    if (status.connection.state !== "ready" || snapshot === null) {
      throw error("not_connected", "no keyboard is connected and read (preview)");
    }
  };

  /** While an OpenWraith command holds the keyboard, the app sends nothing. */
  const requireNotPausedForCli = () => {
    if (status.paused_for_cli) throw clone(PAUSED_FOR_CLI);
  };

  /**
   * The recorded answer of a view that is due: among the own recorded calls
   * whose earlier user actions were replayed, the latest one whose virtual
   * time has come (else the first of them, else the first recorded).
   */
  const dueRecorded = (command: ViewCommand): { rec: Recorded; index: number } | null => {
    const own = calls.flatMap((rec, index) => (rec.command === command ? [{ rec, index }] : []));
    const usable = own.filter(({ index }) => triggers.every((t) => t >= index || replayed.has(t)));
    const now = virtualNow();
    const due = usable.filter(({ rec }) => rec.at_ms <= now);
    return due[due.length - 1] ?? usable[0] ?? own[0] ?? null;
  };

  /** A kept view: the due recorded answer when newer, the kept one, or another fixture's. */
  const keptView = <T>(command: ViewCommand, kept: Kept<T>, result: (rec: Recorded) => T | null): T => {
    const due = dueRecorded(command);
    if (due !== null && due.rec === tailCall && due.rec.at_ms <= virtualNow()) void replayTail(0);
    if (due !== null && (kept.value === null || due.index > kept.index)) {
      if (due.rec.err !== null) throw clone(due.rec.err);
      const value = result(due.rec);
      if (value !== null) {
        kept.value = clone(value);
        kept.index = due.index;
      }
    }
    if (kept.value === null) {
      const borrowed = nextRecorded(command);
      if (borrowed !== null && borrowed.rec.err !== null) throw clone(borrowed.rec.err);
      const value = borrowed === null ? null : result(borrowed.rec);
      if (value === null) throw error("internal", `the preview scenario "${fixture.id}" has no recording of ${command}`);
      kept.value = clone(value);
    }
    return clone(kept.value);
  };

  const recoveryView = (): RecoveryView =>
    keptView("get_recovery", recovery, (rec) => (rec.command === "get_recovery" ? rec.ok : null));
  const autoBackupsView = (): AutoBackupsView =>
    keptView("list_auto_backups", autoBackups, (rec) => (rec.command === "list_auto_backups" ? rec.ok : null));
  const backgroundView = (): BackgroundSettingsView =>
    keptView("get_background_settings", background, (rec) => (rec.command === "get_background_settings" ? rec.ok : null));
  const profileNamesView = (): ProfileNamesView =>
    keptView("get_profile_names", profileNames, (rec) => (rec.command === "get_profile_names" ? rec.ok : null));
  const automationView = (): AutoProfileStateView =>
    keptView("get_auto_profile_state", automation, (rec) => (rec.command === "get_auto_profile_state" ? rec.ok : null));

  /** Keeps a view the user changed (it wins over every recorded answer from now on). */
  const userChanged = <T>(kept: Kept<T>, value: T): T => {
    kept.value = clone(value);
    kept.index = Number.POSITIVE_INFINITY;
    return clone(value);
  };

  /** An own recorded call of `command` with exactly these arguments (replayed instead of synthesized). */
  const sameRecorded = (command: CommandName, args: Record<string, unknown>): { rec: Recorded; own: boolean } | null => {
    const wanted = stableJson(args);
    const rec = calls.find((call, index) => index >= seedEnd && call.command === command && stableJson(call.args) === wanted);
    return rec === undefined ? null : { rec, own: true };
  };

  /** Publishes the automation part of the status. */
  const publishAutomation = (state: AutoProfileStateView) => {
    publish({ ...status, automation: clone(state.status) });
  };

  /** The activity entry of an event, now. */
  const activity = (event: AutoEventView): AutoActivityView => ({ at: new Date().toISOString(), event });

  /** `set_background_settings`, as app-core: validated, saved, the shell changes reported. */
  const setBackground = (intent: BackgroundSettingsIntent) => {
    const keep = intent.auto_backup.keep;
    if (!Number.isInteger(keep) || keep < 1 || keep > 100) {
      throw refused(`keep 1 to 100 automatic backups per profile, not ${String(keep)}`, "the settings were not saved");
    }
    const before = backgroundView();
    // Starting with Windows needs the notification-area icon.
    const start = intent.tray_enabled && intent.start_with_windows;
    const next: BackgroundSettingsView = {
      tray_enabled: intent.tray_enabled,
      start_with_windows: start,
      // The mock's start entry follows the switch; a "turned off in Task Manager" the recording
      // shows stays, as in the app (OpenWraith never changes that choice).
      windows_start: !start ? "off" : taskManagerOff ? "off_in_task_manager" : "on",
      close_to_tray_explained: before.close_to_tray_explained,
      notifications: clone(intent.notifications),
      auto_backup: clone(intent.auto_backup),
    };
    userChanged(background, next);
    if (automation.value !== null) automation.value = { ...automation.value, tray_enabled: next.tray_enabled };
    if (autoBackups.value !== null) {
      autoBackups.value = { ...autoBackups.value, enabled: next.auto_backup.enabled, keep: next.auto_backup.keep };
    }
    if (recovery.value !== null) recovery.value = { ...recovery.value, auto_backup_enabled: next.auto_backup.enabled };
    return {
      settings: clone(next),
      shell: {
        tray: next.tray_enabled === before.tray_enabled ? null : next.tray_enabled,
        start_with_windows: next.start_with_windows === before.start_with_windows ? null : next.start_with_windows,
      },
      shell_error: null,
    };
  };

  /** `set_profile_names`, as app-core: cleaned names, a product without names removed. */
  const setProfileNames = (intent: ProfileNamesIntent): ProfileNamesView => {
    if (!/^[A-Za-z0-9_.-]{1,32}$/.test(intent.product) || intent.names.length !== 4) {
      throw refused("four names of a product (letters, digits, '_', '-', '.')", "the profile names were not saved");
    }
    const names = intent.names.map((name) => {
      if (name === null) return null;
      const text = Array.from(withoutControls(name).trim()).slice(0, MAX_PROFILE_NAME).join("").trim();
      return text === "" ? null : text;
    });
    const view = profileNamesView();
    const all = new Map(view.products.map((p) => [p.product, p.names]));
    if (names.every((n) => n === null)) all.delete(intent.product);
    else all.set(intent.product, names);
    if (view.current_product !== null && !all.has(view.current_product)) {
      all.set(view.current_product, [null, null, null, null]);
    }
    const products = [...all.entries()]
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([product, n]) => ({ product, names: n }));
    if (recovery.value !== null && view.current_product === intent.product) {
      recovery.value = {
        ...recovery.value,
        profiles: recovery.value.profiles.map((p) => ({ ...p, name: names[p.profile - 1] ?? null })),
      };
    }
    return userChanged(profileNames, { current_product: view.current_product, products });
  };

  /** `set_auto_profile_rules`, as app-core: programs by token, kept or typed; validated. */
  const setRules = (intent: AutoProfileRulesIntent): AutoProfileStateView => {
    const what = "the game rules were not saved";
    const before = automationView();
    if (intent.rules.length > MAX_RULES) throw refused(`at most ${String(MAX_RULES)} rules`, what);
    if (intent.default_profile !== null && (intent.default_profile < 1 || intent.default_profile > 4)) {
      throw refused("the default profile is 1 to 4", what);
    }
    const used = intent.rules.flatMap((rule) => (rule.id === null ? [] : [rule.id]));
    const rules: AutoProfileRuleView[] = [];
    for (const rule of intent.rules) {
      const existing = rule.id === null ? undefined : before.rules.find((r) => r.id === rule.id);
      let exe: string;
      let folder: string | null;
      if (rule.program.source === "keep") {
        if (existing === undefined) {
          throw refused(`rule ${JSON.stringify(rule.id)} keeps a program but no such rule exists`, what);
        }
        exe = existing.exe;
        folder = existing.folder;
      } else if (rule.program.source === "picked") {
        const program = programs.get(rule.program.token);
        if (program === undefined) throw refused("the program is not known any more", what);
        exe = program.exe;
        folder = program.folder;
      } else {
        exe = rule.program.exe.trim();
        folder = null;
      }
      let id = rule.id;
      if (id === null) {
        let n = 1;
        while (used.includes(`r-${String(n)}`)) n += 1;
        id = `r-${String(n)}`;
        used.push(id);
      }
      if (exe === "" || exe.length > 260 || /[/\\:]/.test(exe) || withoutControls(exe) !== exe) {
        throw refused(`${JSON.stringify(exe)} is not a program file name`, what);
      }
      if (rule.profile < 1 || rule.profile > 4) throw refused(`profile ${String(rule.profile)} does not exist`, what);
      rules.push({
        id,
        label: Array.from(withoutControls(rule.label)).slice(0, 64).join(""),
        exe,
        folder,
        profile: rule.profile,
        enabled: rule.enabled,
        generic_name: GENERIC_NAMES.includes(exe.trim().toLowerCase()),
      });
    }
    const holding = before.status.holding;
    const stillHolds = intent.enabled && holding !== null && rules.some((r) => r.enabled && r.id === holding.rule_id);
    const next: AutoProfileStateView = {
      ...before,
      enabled: intent.enabled,
      default_profile: intent.default_profile,
      rules,
      status: {
        enabled: intent.enabled,
        paused: before.status.paused,
        holding: stillHolds ? holding : null,
        waiting: intent.enabled ? before.status.waiting : null,
      },
      line: lineOf(intent.enabled, before.status.paused, rules),
      tray_enabled: background.value?.tray_enabled ?? before.tray_enabled,
    };
    publishAutomation(next);
    return userChanged(automation, next);
  };

  /** `pause_auto_profile`: paused or resumed by the user for this run. */
  const pauseAutomation = (paused: boolean): AutoProfileStateView => {
    const before = automationView();
    const reason: AutoPauseReason | null = paused ? { reason: "user" } : null;
    if ((before.status.paused !== null) === paused) return before;
    const next: AutoProfileStateView = {
      ...before,
      status: { ...before.status, paused: reason, waiting: paused ? null : before.status.waiting },
      line: lineOf(before.enabled, reason, before.rules),
      recent: [activity(reason === null ? { event: "resumed" } : { event: "paused", reason }), ...before.recent].slice(0, 20),
    };
    publishAutomation(next);
    if (reason !== null) send({ event: "notice", notice: { notice: "auto_profile_paused", reason } });
    return userChanged(automation, next);
  };

  /** `pin_auto_backup`: keeps (or no longer keeps) one automatic backup. */
  const pinAutoBackup = (token: string, pinned: boolean): AutoBackupsView => {
    const list = autoBackupsView();
    if (!list.entries.some((entry) => entry.file.token === token)) {
      throw error("file_unknown", "this is not an automatic backup OpenWraith listed (preview)");
    }
    return userChanged(autoBackups, {
      ...list,
      entries: list.entries.map((entry) => (entry.file.token === token ? { ...entry, pinned } : entry)),
    });
  };

  /**
   * Resolves the hold of `profiles` (every held profile when empty), as
   * "It's fine, keep them" or a completed restore does, and publishes it.
   */
  const resolveHold = (profiles: readonly number[]): RecoveryView => {
    const view = recoveryView();
    const resolved = (p: number) => profiles.length === 0 || profiles.includes(p);
    const changed: RecoveryView = {
      ...view,
      profiles: view.profiles.map((p) =>
        p.state === "reset_suspected" && resolved(p.profile)
          ? { ...p, state: "fine", evidence: [], factory_now: [], known_good_age_s: 0 }
          : p,
      ),
    };
    const held = changed.profiles.filter((p) => p.state === "reset_suspected");
    const firmware = held.flatMap((p) => p.evidence.flatMap((e) => (e.evidence === "firmware_changed" ? [e] : [])))[0];
    const reported = held.length > 0 && view.reset_reported;
    const next: RecoveryView = {
      ...changed,
      firmware_changed: firmware === undefined ? null : { known: firmware.known, current: firmware.current },
      reset_reported: reported,
      reset_reported_at: reported ? view.reset_reported_at : null,
      attention: held.length > 0 || reported || firmware !== undefined,
    };
    publish({
      ...status,
      recovery: {
        reset_suspected: held.map((p) => p.profile),
        reset_reported: reported,
        firmware_changed: firmware !== undefined,
      },
    });
    return userChanged(recovery, next);
  };

  /** After a completed restore of a recovery: the profiles it restored are fine (unless recorded). */
  const afterRecoveryRestore = (intent: RecoveryIntent) => {
    const recorded = calls.some(
      (call, index) => call.command === "get_recovery" && calls.slice(0, index).some((c) => c.command === "apply_restore"),
    );
    if (recorded || recovery.value === null) return;
    const tokens = new Set(intent.files);
    const profiles = recovery.value.profiles
      .filter((p) => p.backups.some((b) => tokens.has(b.file.token)))
      .map((p) => p.profile);
    resolveHold(profiles);
  };

  /** Keeps the view a replayed recorded setter answered with. */
  const adoptRecorded = (rec: Recorded) => {
    const index = calls.indexOf(rec);
    switch (rec.command) {
      case "set_background_settings":
        if (rec.ok !== null) Object.assign(background, { value: clone(rec.ok.settings), index });
        break;
      case "set_profile_names":
        if (rec.ok !== null) Object.assign(profileNames, { value: clone(rec.ok), index });
        break;
      case "set_auto_profile_rules":
      case "pause_auto_profile":
        if (rec.ok !== null) {
          Object.assign(automation, { value: clone(rec.ok), index });
          publishAutomation(rec.ok);
        }
        break;
      case "pin_auto_backup":
        if (rec.ok !== null) Object.assign(autoBackups, { value: clone(rec.ok), index });
        break;
      case "keep_current_settings":
        if (rec.ok !== null) {
          Object.assign(recovery, { value: clone(rec.ok), index });
          publish({
            ...status,
            recovery: {
              reset_suspected: rec.ok.profiles.filter((p) => p.state === "reset_suspected").map((p) => p.profile),
              reset_reported: rec.ok.reset_reported,
              firmware_changed: rec.ok.firmware_changed !== null,
            },
          });
        }
        break;
      default:
        break;
    }
  };

  /** Remembers the programs an answer named (their tokens are what rules send). */
  const rememberPrograms = (answer: unknown) => {
    const list = Array.isArray(answer) ? answer : answer === null || answer === undefined ? [] : [answer];
    for (const item of list as ProgramView[]) programs.set(item.token, clone(item));
  };

  /** The recorded live view: this fixture's, else (with fallbacks) another fixture's, with its frames. */
  const depthRecording = (): { report: DepthViewReport | null; err: AppError | null; frames: DepthFrame[] } | null => {
    const ids = options.fallback ? [...FALLBACK_ORDER, ...Object.keys(options.library)] : [];
    const sources = [fixture, ...ids.flatMap((id) => options.library[id] ?? [])];
    for (const source of sources) {
      for (const call of source.calls) {
        if (call.command !== "start_depth_view") continue;
        const frames = (source.ui?.["depth_frames"] as DepthFrame[] | undefined) ?? [];
        return { report: clone(call.ok), err: clone(call.err), frames: clone(frames) };
      }
    }
    return null;
  };

  /**
   * The live key-depth view: busy and "open" while it runs, the recorded
   * frames on the channel at their recorded times, then the recorded report
   * when the page stops it (or at once for a stop the backend decided), and
   * the `depth_view_stopped` notice.
   */
  const depthView = async (channel: ChannelRef | null): Promise<DepthViewReport> => {
    const recording = depthRecording();
    if (recording === null) {
      throw error("internal", `the preview scenario "${fixture.id}" has no recording of start_depth_view`);
    }
    if (recording.err !== null) throw recording.err;
    const report = recording.report;
    if (report === null) throw error("internal", "the recorded live view has no report");
    if (status.busy !== null || depth !== null) throw error("operation_running", "another operation is running (preview)");
    requireReady();
    const stop: { reason: DepthStopRequest | null } = { reason: null };
    const stopped = new Promise<DepthStopRequest>((resolve) => {
      depth = {
        finish: (reason) => {
          stop.reason ??= reason;
          resolve(stop.reason);
        },
      };
    });
    setBusy("depth_view", `o-${opCounter++}`);
    publish({ ...status, travel: { ...status.travel, state: "open" } });
    let previous = 0;
    for (const [index, frame] of recording.frames.entries()) {
      await wait(frame.open_ms - previous);
      previous = frame.open_ms;
      if (stop.reason !== null) break;
      if (channel !== null) deliver(channel.id, { index, message: frame });
    }
    const reason = PAGE_STOPS.includes(report.reason) ? await stopped : report.reason;
    depth = null;
    const final: DepthViewReport = { ...report, reason };
    publish({
      ...status,
      busy: null,
      write_gate: gateWith(status.write_gate.reasons.filter((r) => r !== "busy")),
      travel: { ...status.travel, state: final.may_be_open ? "maybe_open" : "closed" },
    });
    send({ event: "notice", notice: { notice: "depth_view_stopped", reason: final.reason, close: final.close } });
    return final;
  };

  /** The macro library of "this PC": the recorded one first (its notice once), then kept in memory. */
  const macros = (): MacroLibraryView => {
    if (macroLibrary !== null) return { ...clone(macroLibrary), notice: null };
    const found = nextRecorded("list_macro_library");
    if (found?.rec.command === "list_macro_library" && found.rec.err !== null) throw found.rec.err;
    const recorded = found?.rec.command === "list_macro_library" ? found.rec.ok : null;
    macroLibrary = recorded === null ? { entries: [], notice: null } : clone(recorded);
    return clone(macroLibrary);
  };

  /** A program's content hash: a keyboard slot's or a library entry's when equal, else a preview hash. */
  const hashOf = (program: MacroProgramView): string => {
    const json = JSON.stringify(program);
    for (const slot of keysSeen?.macros ?? []) {
      if (slot.content.state === "program" && JSON.stringify(slot.content.program) === json) return slot.content.content_hash;
    }
    return macroLibrary?.entries.find((entry) => JSON.stringify(entry.program) === json)?.content_hash ?? previewHash(program);
  };

  const editLibrary = (change: (entries: MacroLibraryView["entries"]) => MacroLibraryView["entries"]): MacroLibraryView => {
    macros();
    macroLibrary = { entries: change(macroLibrary?.entries ?? []), notice: null };
    return clone(macroLibrary);
  };

  const handle = async (command: string, rawArgs: unknown): Promise<unknown> => {
    const args = argsRecord(rawArgs);
    const name = command as CommandName;
    const channel = isChannel(args["progress"]) ? args["progress"] : null;

    // Commands answered from the state.
    switch (name) {
      case "app_info":
        return clone(fixture.app_info);
      case "get_settings":
        return clone(settings);
      case "set_settings":
        settings = clone(args["settings"] as typeof settings);
        return clone(settings);
      case "subscribe": {
        const subscription = args["channel"];
        if (isChannel(subscription)) subscriber = { channel: subscription, index: 0 };
        if (!subscribed) {
          subscribed = true;
          startedAt = Date.now();
        }
        // Without user actions, every event; with them, those recorded
        // before the first one (e.g. a connection that hears a test left
        // open, an automatic switch); the rest follow the last one.
        const early =
          firstTrigger === undefined
            ? fixture.events
            : fixture.events.filter((item) => item.at_ms < firstTrigger.at_ms);
        if (options.replayEvents && early.length > 0 && eventsPlayed === 0) {
          eventsPlayed = early.length;
          void (async () => {
            let previous = 0;
            for (const item of early) {
              await wait(item.at_ms - previous);
              previous = item.at_ms;
              if (item.event.event === "status") publish({ ...item.event.status, sim: status.sim });
              else send(item.event);
            }
          })();
        }
        return clone(status);
      }
      case "get_status":
        return clone(status);
      case "get_snapshot":
        return snapshotView();
      case "get_last_report":
        return clone(lastReport);
      case "list_safety_backups":
        return clone(fixture.safety_backups);
      case "cancel_operation":
        return true;
      case "answer_close":
        publish({ ...status, close_requested: false });
        return null;
      case "dismiss_interrupted":
        publish({ ...status, interrupted: null });
        return null;
      case "reveal":
      case "discard_plan":
        return null;
      case "dev_sim_info":
        return simInfo();
      case "dev_sim_action":
        return simAction(args["action"] as SimAction);
      case "get_hints_shown":
        return [...hints];
      case "mark_hint_shown": {
        const hint = String(args["hint"]);
        if (!hints.includes(hint)) hints = [...hints, hint];
        return [...hints];
      }
      case "reset_hints_shown":
        hints = [];
        return [];
      case "start_depth_view":
        return depthView(isChannel(args["channel"]) ? args["channel"] : null);
      case "stop_depth_view":
        if (depth === null) return false;
        depth.finish(args["reason"] as DepthStopRequest);
        return true;
      case "list_macro_library":
        return macros();
      case "save_library_macro": {
        const program = args["program"] as MacroProgramView;
        const hash = hashOf(program);
        const entry = { content_hash: hash, name: String(args["name"]).trim(), program: clone(program), saved_at: new Date().toISOString() };
        return editLibrary((entries) => [...entries.filter((e) => e.content_hash !== hash), entry]);
      }
      case "rename_library_macro":
        return editLibrary((entries) =>
          entries.map((e) => (e.content_hash === args["hash"] ? { ...e, name: String(args["name"]).trim() } : e)),
        );
      case "delete_library_macro":
        return editLibrary((entries) => entries.filter((e) => e.content_hash !== args["hash"]));
      // The frontend's drafts and vendor-site tick: nothing to answer.
      case "report_ui_state":
        return null;
      // Views kept in memory.
      case "get_recovery":
        return recoveryView();
      case "list_auto_backups":
        return autoBackupsView();
      case "get_background_settings":
        return backgroundView();
      case "get_profile_names":
        return profileNamesView();
      case "get_auto_profile_state":
        return automationView();
      // Their setters: the recorded call with the same arguments, else as app-core would answer.
      case "set_background_settings":
      case "set_profile_names":
      case "set_auto_profile_rules":
      case "pause_auto_profile":
      case "pin_auto_backup":
      case "keep_current_settings": {
        // Once the user changed the view here, the recording no longer describes it.
        const view = {
          set_background_settings: background,
          set_profile_names: profileNames,
          set_auto_profile_rules: automation,
          pause_auto_profile: automation,
          pin_auto_backup: autoBackups,
          keep_current_settings: recovery,
        }[name];
        const same = view.index === Number.POSITIVE_INFINITY ? null : sameRecorded(name, args);
        if (same !== null) {
          // The view changes first, as in the app (a status it publishes reads the new view).
          adoptRecorded(same.rec);
          return replay(name, same, channel);
        }
        switch (name) {
          case "set_background_settings":
            return setBackground(args["settings"] as BackgroundSettingsIntent);
          case "set_profile_names":
            return setProfileNames(args["names"] as ProfileNamesIntent);
          case "set_auto_profile_rules":
            return setRules(args["rules"] as AutoProfileRulesIntent);
          case "pause_auto_profile":
            return pauseAutomation(args["paused"] === true);
          case "pin_auto_backup":
            return pinAutoBackup(String(args["file"]), args["pinned"] === true);
          default:
            return resolveHold(Array.isArray(args["profiles"]) ? (args["profiles"] as number[]) : []);
        }
      }
      // Programs for game rules: recorded (or another fixture's), remembered by token.
      case "list_running_programs":
      case "pick_game_program":
      case "detect_next_program": {
        const found = nextRecorded(name);
        if (found === null) return name === "list_running_programs" ? [] : null;
        const answer = await replay(name, found, channel);
        rememberPrograms(answer);
        return answer;
      }
      case "prepare_recovery":
        recoveryIntent = clone(args["intent"] as RecoveryIntent);
        break;
      case "prepare_restore":
      case "prepare_import":
        recoveryIntent = null;
        break;
      default:
        break;
    }

    // Views of the last full read: none while no keyboard was read.
    if (STATE_VIEWS.includes(name) && !fixture.calls.some((call) => call.command === name)) {
      if (snapshot === null || status.snapshot_revision === null) return null;
    }

    // Recorded calls (own fixture first).
    const found = nextRecorded(name);
    if (found !== null && (found.own || !["disconnect", "reconnect", "select_device", "recheck_other_program", "prepare_profile_switch", "apply_profile_switch", "refresh"].includes(name))) {
      return replay(name, found, channel);
    }

    // Synthesized answers.
    switch (name) {
      case "disconnect":
        publish({ ...notReady({ state: "released" }, "not_connected"), snapshot_revision: null, active_profile: null });
        return null;
      case "reconnect":
      case "select_device":
      case "recheck_other_program":
        void reconnectFlow();
        return null;
      case "refresh": {
        requireReady();
        requireNotPausedForCli();
        const op = `o-${opCounter++}`;
        setBusy("refresh", op);
        const step = (done: number): ProgressStep => ({ step: "reading", purpose: "snapshot", profile: null, done, total: 114 });
        await replayProgress(progressOf(op, [step(1), step(40), step(80), step(114)], 150), channel);
        clearBusy();
        publish({ ...status, snapshot_revision: (status.snapshot_revision ?? 0) + 1, snapshot_stale: false });
        return snapshotView();
      }
      case "prepare_profile_switch": {
        requireReady();
        requireNotPausedForCli();
        const to = Number(args["profile"]);
        const from = status.active_profile ?? 1;
        setBusy("profile_switch", `o-${opCounter++}`);
        await wait(250);
        clearBusy();
        return { token: from === to ? null : `p-preview-${to}`, from, to, nothing_to_do: from === to, save_wait_ms: 0 };
      }
      case "apply_profile_switch": {
        requireReady();
        requireNotPausedForCli();
        const token = String(args["token"]);
        const to = Number(token.replace("p-preview-", ""));
        const from = status.active_profile ?? 1;
        const op = `o-${opCounter++}`;
        setBusy("profile_switch", op);
        const read = (done: number): ProgressStep => ({ step: "reading", purpose: "snapshot", profile: null, done, total: 114 });
        await replayProgress(progressOf(op, [{ step: "selecting_profile", profile: to }, read(1), read(114)], 300), channel);
        if (snapshot !== null) snapshot.profile = to;
        clearBusy();
        publish({ ...status, active_profile: to, snapshot_revision: (status.snapshot_revision ?? 0) + 1 });
        const report = { from, to, active_profile_now: to, waited_for_commit_ms: null, warnings: [] };
        lastReport = { kind: "profile_switch", report };
        return report;
      }
      case "open_backup_file":
      case "choose_backup_destination":
        return null;
      default:
        if (found !== null) return replay(name, found, channel);
        throw error("internal", `the preview scenario "${fixture.id}" has no recording of ${command}`);
    }
  };

  return { handle, status: () => status };
}
