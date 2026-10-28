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
// - after the fixture's last recorded call, its recorded events are replayed
//   so the app ends in the recorded final state;
// - the live key-depth view replays the frames the fixture recorded
//   (`ui.depth_frames`) on its channel and resolves with the recorded report
//   when the page stops it (or at once for a stop the backend decided);
// - the macro library of "this PC" is kept in memory, starting from the
//   recorded one.

import type {
  AppError,
  AppEvent,
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
  ProgressEvent,
  ProgressStep,
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
};

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
];

function error(kind: AppError["kind"], message: string): AppError {
  return { kind, detail: null, message };
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
  const lastCall = fixture.calls[fixture.calls.length - 1];

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
    if (tailPlayed || lastCall === undefined) return;
    tailPlayed = true;
    let previous = lastCall.at_ms + afterMs;
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
        if (rec.ok !== null) lastReport = { kind: "restore", report: rec.ok };
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
    if (rec.progress.length > 0) {
      await replayProgress(rec.progress, channel);
    } else {
      await wait(kind === undefined ? 0 : 250);
    }
    if (kind !== undefined) clearBusy();
    if (rec.err === null) absorb(rec);
    if (found.own && rec === lastCall) {
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
        // Without calls, every event; with calls, those recorded before the
        // first one (e.g. a connection that hears a test left open); the
        // rest follow the last call.
        const firstCall = fixture.calls[0];
        const early =
          firstCall === undefined ? fixture.events : fixture.events.filter((item) => item.at_ms < firstCall.at_ms);
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
        const to = Number(args["profile"]);
        const from = status.active_profile ?? 1;
        setBusy("profile_switch", `o-${opCounter++}`);
        await wait(250);
        clearBusy();
        return { token: from === to ? null : `p-preview-${to}`, from, to, nothing_to_do: from === to, save_wait_ms: 0 };
      }
      case "apply_profile_switch": {
        requireReady();
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
