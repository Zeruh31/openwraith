// The browser preview's IPC: the pinned @tauri-apps/api delivers Channel
// messages in index order through mockIPC, and the mock backend replays a
// recorded fixture (result, progress, final state) through the real `api`.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { AppEvent, DepthFrame, MacroProgramView, ProgressEvent } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { createMockBackend } from "../preview/mockBackend";
import { FIXTURES, findScenario, routeForFixture } from "../preview/registry";
import { readPreviewParams } from "../preview/installMocks";
import type { Fixture } from "../preview/types";

afterEach(() => {
  clearMocks();
});

function fixture(id: string): Fixture {
  const found = FIXTURES[id];
  if (found === undefined) throw new Error(`no fixture ${id}`);
  return found;
}

function runCallback(id: number, data: unknown): void {
  (window as unknown as { __TAURI_INTERNALS__: { runCallback(id: number, data: unknown): void } }).__TAURI_INTERNALS__.runCallback(
    id,
    data,
  );
}

function settle(ms = 20): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

const EVENT = (seq: number): ProgressEvent => ({
  op: "o-1",
  seq,
  elapsed_ms: 0,
  step: { step: "comparing" },
  cancel: "available",
});

describe("Channel delivery with the pinned @tauri-apps/api", () => {
  it("orders messages by index and resolves after them", async () => {
    mockIPC((_command, args) => {
      const channel = (args as { progress: { id: number } }).progress;
      runCallback(channel.id, { index: 1, message: EVENT(2) });
      runCallback(channel.id, { index: 0, message: EVENT(1) });
      return { file: { token: "f-1", file_name: "x", display_path: "x" }, profiles: [1], created_at: "", firmware: null, undecoded: 0, foreign_activity: null };
    });
    const seen: number[] = [];
    const saved = await api.backupActiveProfile("f-1", (event) => {
      seen.push(event.seq);
    });
    expect(seen).toEqual([1, 2]);
    expect(saved.profiles).toEqual([1]);
  });
});

describe("mock backend", () => {
  it("replays a recorded trigger edit with its progress and final state", async () => {
    const source = fixture("trigger_apply_ok");
    const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
    mockIPC((command, args) => backend.handle(command, args));

    const events: AppEvent[] = [];
    await api.subscribe((event) => events.push(event));

    const prepare = source.calls.find((c) => c.command === "prepare_trigger_edit");
    const apply = source.calls.find((c) => c.command === "apply_trigger_edit");
    if (prepare?.command !== "prepare_trigger_edit" || apply?.command !== "apply_trigger_edit") {
      throw new Error("fixture lacks the trigger calls");
    }
    const preview = await api.prepareTriggerEdit(prepare.args.intent);
    expect(preview).toEqual(prepare.ok);

    const progress: ProgressEvent[] = [];
    const report = await api.applyTriggerEdit(apply.args.token, (event) => progress.push(event));
    expect(report).toEqual(apply.ok);
    expect(progress.map((e) => e.seq)).toEqual(apply.progress.map((e) => e.seq));
    expect(progress.some((e) => e.step.step === "waiting_for_keyboard_save")).toBe(true);

    await settle();
    const statuses = events.flatMap((e) => (e.event === "status" ? [e.status] : []));
    expect(statuses.some((s) => s.busy?.kind === "trigger_edit")).toBe(true);
    expect(backend.status().busy).toBeNull();
    const revisions = statuses.map((s) => s.revision);
    expect(revisions).toEqual([...revisions].sort((a, b) => a - b));

    // The snapshot now shows the written values.
    const snapshot = await api.getSnapshot();
    const target = preview.keys.find((k) => k.changes);
    if (target !== undefined) {
      expect(snapshot?.triggers.keys.find((k) => k.key.index === target.key.index)?.value).toEqual(target.target);
    }
  });

  it("rejects with the recorded AppError", async () => {
    const source = fixture("trigger_unit_mismatch");
    const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
    mockIPC((command, args) => backend.handle(command, args));
    const call = source.calls[0];
    if (call?.command !== "prepare_trigger_edit") throw new Error("unexpected fixture");
    await expect(api.prepareTriggerEdit(call.args.intent)).rejects.toEqual(call.err);
  });

  it("fails unrecorded commands without fallback and borrows them with it", async () => {
    const strict = createMockBackend(fixture("ready"), { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
    mockIPC((command, args) => strict.handle(command, args));
    await expect(api.prepareLightEdit({ mode: 0, color: null, brightness: null, speed: null, based_on: 1 })).rejects.toMatchObject({
      kind: "internal",
    });

    clearMocks();
    const lenient = createMockBackend(fixture("ready"), { speed: 0, replayEvents: false, fallback: true, library: FIXTURES });
    mockIPC((command, args) => lenient.handle(command, args));
    const preview = await api.prepareLightEdit({ mode: 0, color: null, brightness: null, speed: null, based_on: 1 });
    expect(preview.profile).toBe(1);
  });

  it("switches profiles in any scenario", async () => {
    const backend = createMockBackend(fixture("ready"), { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
    mockIPC((command, args) => backend.handle(command, args));
    await api.subscribe(() => undefined);
    const preview = await api.prepareProfileSwitch(3);
    expect(preview).toMatchObject({ from: 1, to: 3, nothing_to_do: false });
    if (preview.token === null) throw new Error("no token");
    const report = await api.applyProfileSwitch(preview.token, () => undefined);
    expect(report.active_profile_now).toBe(3);
    expect((await api.getStatus()).active_profile).toBe(3);
    expect((await api.getSnapshot())?.profile).toBe(3);
  });

  it("replays the recorded live view and resolves when the page stops it", async () => {
    const source = fixture("depth_live");
    const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
    mockIPC((command, args) => backend.handle(command, args));
    const events: AppEvent[] = [];
    await api.subscribe((event) => events.push(event));
    const frames: DepthFrame[] = [];
    const done = api.startDepthView((frame) => frames.push(frame));
    await settle();
    expect(backend.status().busy?.kind).toBe("depth_view");
    expect(backend.status().travel.state).toBe("open");
    const recorded = (source.ui?.["depth_frames"] as DepthFrame[] | undefined) ?? [];
    expect(recorded.length).toBeGreaterThan(0);
    await vi.waitFor(() => {
      expect(frames).toEqual(recorded);
    });
    expect(await api.stopDepthView("page_left")).toBe(true);
    const report = await done;
    expect(report.reason).toBe("page_left");
    expect(backend.status().busy).toBeNull();
    expect(backend.status().travel.state).toBe("closed");
    expect(events.some((e) => e.event === "notice" && e.notice.notice === "depth_view_stopped")).toBe(true);
    expect(await api.stopDepthView("requested")).toBe(false);
  });

  it("ends a view the backend stopped at once, and keeps a test that may be open", async () => {
    const backend = createMockBackend(fixture("depth_stopped_reset"), {
      speed: 0,
      replayEvents: false,
      fallback: false,
      library: FIXTURES,
    });
    mockIPC((command, args) => backend.handle(command, args));
    await api.subscribe(() => undefined);
    const report = await api.startDepthView(() => undefined);
    expect(report.reason).toBe("keyboard_reset");
    expect(report.may_be_open).toBe(true);
    expect(backend.status().travel.state).toBe("maybe_open");
  });

  it("answers the new editors' views from other fixtures, and none while no keyboard was read", async () => {
    const lenient = createMockBackend(fixture("ready"), { speed: 0, replayEvents: false, fallback: true, library: FIXTURES });
    mockIPC((command, args) => lenient.handle(command, args));
    await api.subscribe(() => undefined);
    expect((await api.getKeysState())?.keys.length).toBeGreaterThan(0);
    expect(await api.getColorsState()).not.toBeNull();
    expect(await api.getDeadBandState()).not.toBeNull();
    expect(await api.getSocdState()).not.toBeNull();
    expect((await api.listPresets()).builtin.length).toBe(5);
    clearMocks();
    const searching = createMockBackend(fixture("searching"), { speed: 0, replayEvents: false, fallback: true, library: FIXTURES });
    mockIPC((command, args) => searching.handle(command, args));
    expect(await api.getKeysState()).toBeNull();
    expect(await api.getSocdState()).toBeNull();
  });

  it("keeps the macro library and the tips in memory", async () => {
    const backend = createMockBackend(fixture("keys_ready"), { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
    mockIPC((command, args) => backend.handle(command, args));
    const recorded = await api.listMacroLibrary();
    const program: MacroProgramView = {
      play: { mode: "once" },
      steps: [
        { usage: 4, motion: "press", delay_after_ms: 10 },
        { usage: 4, motion: "release", delay_after_ms: 0 },
      ],
    };
    const saved = await api.saveLibraryMacro("Tap A", program);
    expect(saved.entries).toHaveLength(recorded.entries.length + 1);
    const hash = saved.entries.find((e) => e.name === "Tap A")?.content_hash ?? "";
    expect(hash).toMatch(/^[0-9a-f]{16}$/);
    expect((await api.renameLibraryMacro(hash, "Tap A twice")).entries.some((e) => e.name === "Tap A twice")).toBe(true);
    expect((await api.deleteLibraryMacro(hash)).entries).toHaveLength(recorded.entries.length);
    expect(await api.getHintsShown()).toEqual([]);
    expect(await api.markHintShown("keys.intro")).toEqual(["keys.intro"]);
    expect(await api.resetHintsShown()).toEqual([]);
  });

  it("replays the events of a fixture without calls", async () => {
    const backend = createMockBackend(fixture("observing"), { speed: 0, replayEvents: true, fallback: false, library: FIXTURES });
    mockIPC((command, args) => backend.handle(command, args));
    const initial = await api.subscribe(() => undefined);
    expect(initial.connection.state).toBe("observing");
    await settle(50);
    expect(backend.status().connection.state).toBe("ready");
  });
});

describe("preview parameters", () => {
  it("read the URL", () => {
    expect(readPreviewParams("?scenario=lost&lang=tr&theme=dark&speed=0&freeze=1&fallback=0")).toEqual({
      scenario: "lost",
      lang: "tr",
      theme: "dark",
      speed: 0,
      freeze: true,
      fallback: false,
    });
    expect(readPreviewParams("")).toMatchObject({ scenario: "ready", lang: null, speed: 1, freeze: false, fallback: true });
  });

  it("find scenarios and bare fixtures", () => {
    expect(findScenario("ready")?.route).toBe("#/device");
    expect(findScenario("trigger_preview")?.route).toBe("#/actuation");
    expect(findScenario("light_ready")?.fixture).toBe("light_ready");
    expect(findScenario("fixture:restore_lost")?.route).toBe("#/backup");
    expect(findScenario("nope")).toBeUndefined();
  });

  it("open the new fixtures on their page and tab", () => {
    expect(routeForFixture("keys_ready")).toBe("#/keys");
    expect(routeForFixture("keys_preview_advanced")).toBe("#/keys?tab=advanced");
    expect(routeForFixture("keys_apply_failed_cleared")).toBe("#/keys?tab=macros");
    expect(routeForFixture("colors_ready")).toBe("#/lighting?tab=colors");
    expect(routeForFixture("dead_band_ready")).toBe("#/actuation?tab=deadband");
    expect(routeForFixture("socd_toggled")).toBe("#/actuation?tab=socd");
    expect(routeForFixture("depth_live")).toBe("#/actuation?tab=depth");
    expect(routeForFixture("presets_ready")).toBe("#/actuation");
    expect(routeForFixture("import_preview")).toBe("#/backup");
    expect(findScenario("keys_apply_ok")?.route).toBe("#/keys?tab=assign");
  });

  it("cover every recorded fixture", () => {
    for (const id of Object.keys(FIXTURES)) {
      expect(findScenario(`fixture:${id}`)?.fixture).toBe(id);
    }
  });
});
