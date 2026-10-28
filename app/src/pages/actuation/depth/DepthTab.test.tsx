// The Key depth tab on the mocked IPC. `start_depth_view` replays the frames
// the core's tests recorded from the simulator (`fixture.ui.depth_frames`)
// on the real `Channel` and resolves with the recorded report when the view
// stops (on `stop_depth_view`, or at once for a stop the keyboard or the
// backend decides); every other command goes to the preview's mock backend.
//
// Covered: the vendor-site tick before Start, the live board (fill, value,
// held peak, the followed key's gauge and history, the hidden table), "Show
// where keys trigger" (estimated), Announce, Stop, the automatic stops
// (tab left, window hidden, 5 minutes without movement with Resume, a reset
// with "Stop it"), a marker left by an earlier run, blocked states.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ToastRegion } from "../../../components/Toasts";
import { DICTIONARIES, setLang } from "../../../i18n";
import type { AppError, DepthFrame, DeviceStatus } from "../../../ipc/bindings";
import { createMockBackend } from "../../../preview/mockBackend";
import { FIXTURES } from "../../../preview/registry";
import type { Fixture } from "../../../preview/types";
import { appInfoStore, DEFAULT_APP_INFO } from "../../../state/appInfo";
import { acknowledgeVendor, resetSession } from "../../../state/session";
import { snapshotStore } from "../../../state/snapshot";
import { INITIAL_STATUS, statusStore } from "../../../state/status";
import { clearToasts } from "../../../state/toasts";
import DepthTab from "./index";

const en = DICTIONARIES.en;
const t = en.actuation.depth;
const W = 46;
const A = 68;

interface Call {
  command: string;
  args: Record<string, unknown>;
}

interface TauriInternals {
  runCallback(id: number, data: unknown): void;
}

let calls: Call[] = [];

function fixture(id: string): Fixture {
  const found = FIXTURES[id];
  if (found === undefined) throw new Error(`no fixture ${id}`);
  return structuredClone(found);
}

/** How the recorded view ends: when the page stops it, at once (the backend stopped it), or refused. */
type Ending = "on_stop" | "at_once" | { error: AppError };

interface InstallOptions {
  status?: Partial<DeviceStatus>;
  ending?: Ending;
  /** Only the first frames. */
  frames?: number;
  /** Answer unrecorded commands from other fixtures. */
  fallback?: boolean;
  /**
   * The backend registers the view only this long after Start (the start runs on a worker thread):
   * a stop sent before finds no view and answers `false`.
   */
  registerAfterMs?: number;
}

/** What each `stop_depth_view` answered. */
let stopAnswers: boolean[] = [];
/** Whether the recorded view ended (its start resolved). */
let viewEnded = false;

function install(id: string, options: InstallOptions = {}): Fixture {
  const source = fixture(id);
  const backend = createMockBackend(source, {
    speed: 0,
    replayEvents: false,
    fallback: options.fallback ?? false,
    library: FIXTURES,
  });
  const recorded = source.calls.find((call) => call.command === "start_depth_view");
  const allFrames = (source.ui?.["depth_frames"] as DepthFrame[] | undefined) ?? [];
  const frames = options.frames === undefined ? allFrames : allFrames.slice(0, options.frames);
  const ending = options.ending ?? "on_stop";
  let finish: (() => void) | null = null;
  let registered = options.registerAfterMs === undefined;
  // A stop the backend took before the view could run: it ends without waiting.
  let stopKept = false;
  mockIPC(async (command, args) => {
    const record = (args ?? {}) as Record<string, unknown>;
    calls.push({ command, args: record });
    if (command === "start_depth_view") {
      if (typeof ending === "object") throw ending.error;
      if (options.registerAfterMs !== undefined) {
        await new Promise<void>((resolve) => setTimeout(resolve, options.registerAfterMs));
        registered = true;
      }
      const channel = record["channel"] as { id: number };
      const internals = (window as unknown as { __TAURI_INTERNALS__: TauriInternals }).__TAURI_INTERNALS__;
      frames.forEach((frame, index) => {
        internals.runCallback(channel.id, { index, message: frame });
      });
      if (ending === "on_stop" && !stopKept) {
        await new Promise<void>((resolve) => {
          finish = resolve;
        });
      }
      viewEnded = true;
      return recorded?.ok;
    }
    if (command === "stop_depth_view") {
      stopAnswers.push(registered);
      if (!registered) return false;
      if (finish === null) stopKept = true;
      finish?.();
      return true;
    }
    return backend.handle(command, args);
  });
  statusStore.set({ ...source.initial_status, ...options.status });
  snapshotStore.set({ snapshot: structuredClone(source.initial_snapshot), loading: false, error: null });
  appInfoStore.set(source.app_info);
  return source;
}

function renderTab() {
  return render(
    <>
      <DepthTab />
      <ToastRegion />
    </>,
  );
}

function commands(): string[] {
  return calls.map((call) => call.command);
}

function stopRequests(): unknown[] {
  return calls.filter((call) => call.command === "stop_depth_view").map((call) => call.args["reason"]);
}

function keyElement(index: number): HTMLElement {
  const element = document.querySelector<HTMLElement>(`[data-key-index="${index}"]`);
  if (element === null) throw new Error(`no key ${index}`);
  return element;
}

function spoken(element: HTMLElement): string {
  const describedBy = element.getAttribute("aria-describedby");
  const description = describedBy === null ? "" : (document.getElementById(describedBy)?.textContent ?? "");
  return `${element.getAttribute("aria-label") ?? ""}, ${description}`;
}

function button(name: string): HTMLElement {
  return screen.getByRole("button", { name });
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

async function startView(): Promise<void> {
  fireEvent.click(button(t.controls.start));
  await screen.findByRole("button", { name: t.controls.stop });
}

function setVisibility(state: DocumentVisibilityState): void {
  Object.defineProperty(document, "visibilityState", { configurable: true, get: () => state });
  document.dispatchEvent(new Event("visibilitychange"));
}

beforeEach(() => {
  setLang("en");
  resetSession();
  acknowledgeVendor();
  calls = [];
  stopAnswers = [];
  viewEnded = false;
});

afterEach(async () => {
  // Leaving the tab stops a running view; let it end (and release the operation lock) before the mocks go.
  cleanup();
  await act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));
  clearMocks();
  clearToasts();
  setVisibility("visible");
  statusStore.set(INITIAL_STATUS);
  snapshotStore.set({ snapshot: null, loading: false, error: null });
  appInfoStore.set(DEFAULT_APP_INFO);
});

describe("starting", () => {
  it("needs the wraith.software tick, without a plan dialog", () => {
    install("depth_live");
    resetSession();
    renderTab();
    expect(screen.getByText(t.note)).toBeTruthy();
    expect(button(t.controls.start)).toHaveProperty("disabled", true);
    expect(screen.getByText(en.flow.review.vendorTickNeeded)).toBeTruthy();
    fireEvent.click(screen.getByRole("checkbox", { name: en.flow.review.vendorTick }));
    expect(button(t.controls.start)).toHaveProperty("disabled", false);
    fireEvent.click(button(t.controls.start));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(commands()).toEqual(["start_depth_view"]);
  });

  it("is not offered while writes are blocked", () => {
    install("depth_live", {
      status: {
        connection: { state: "other_program", evidence: { status_reports: 1, foreign_acks: 0, profile_reports: 0 } },
        write_gate: { allowed: false, reasons: ["other_program"] },
      },
    });
    renderTab();
    expect(button(t.controls.start)).toHaveProperty("disabled", true);
    expect(screen.getByText(en.errors.writeBlocked.other_program)).toBeTruthy();
  });

  it("is not offered while no keyboard is connected", () => {
    install("depth_live", {
      status: { connection: { state: "searching" }, write_gate: { allowed: false, reasons: ["not_connected"] } },
    });
    renderTab();
    expect(button(t.controls.start)).toHaveProperty("disabled", true);
    expect(screen.getByText(en.errors.writeBlocked.not_connected)).toBeTruthy();
  });

  it("shows a refused start as an error", async () => {
    const error: AppError = { kind: "operation_running", detail: null, message: "another operation runs" };
    install("depth_live", { ending: { error } });
    renderTab();
    fireEvent.click(button(t.controls.start));
    expect(await screen.findByText(en.errors.kind.operation_running.title)).toBeTruthy();
    expect(button(t.controls.start)).toBeTruthy();
  });
});

describe("the live view", () => {
  it("fills the caps from the top with the depth, follows the key that moved last and says writes are held", async () => {
    install("depth_live", { frames: 4 });
    renderTab();
    await startView();
    // W at 80 units of 170 (1.60 mm).
    const w = keyElement(W);
    await waitFor(() => {
      expect(w.querySelector<HTMLElement>(".ow-key__fill")?.style.height).toBe("47%");
    });
    expect(w.textContent).toContain("1.60");
    expect(spoken(w)).toContain("pressed 1.60 mm");
    expect(keyElement(A).textContent).toContain("1.40");
    expect(screen.getByText(t.detail.title({ key: "W" }))).toBeTruthy();
    expect(screen.getByText(t.detail.peak({ value: "1.60 mm" }))).toBeTruthy();
    expect(screen.getByText(t.controls.blocksWrites)).toBeTruthy();
    // The hidden table alternative lists the keys that moved.
    const table = screen.getByRole("table", { name: t.table.caption });
    expect(within(table).getAllByRole("row")).toHaveLength(3);
  });

  it("estimates where keys trigger from their settings, labelled as an estimate", async () => {
    install("depth_live", { frames: 4 });
    renderTab();
    fireEvent.click(screen.getByRole("switch", { name: t.options.showTrigger }));
    expect(screen.getByText(t.options.estimatedNote)).toBeTruthy();
    await startView();
    // W triggers at 1.00 mm (Rapid Trigger on) and is at 1.60 mm.
    await waitFor(() => {
      expect(keyElement(W).textContent).toContain(t.onBadge);
    });
    expect(spoken(keyElement(W))).toContain(t.keyInfo.on);
    const detail = document.querySelector<HTMLElement>(".act-depth-detail");
    if (detail === null) throw new Error("no detail");
    expect(within(detail).getByText(t.detail.estimatedOn)).toBeTruthy();
    expect(within(screen.getByRole("table", { name: t.table.caption })).getByText(t.table.state)).toBeTruthy();
  });

  it("follows a selected key instead", async () => {
    install("depth_live");
    renderTab();
    await startView();
    const a = keyElement(A);
    fireEvent.pointerDown(a, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
    fireEvent.pointerUp(a, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
    expect(screen.getByText(t.detail.title({ key: "A" }))).toBeTruthy();
    expect(screen.getByText(t.detail.peak({ value: "3.00 mm" }))).toBeTruthy();
    // A was tapped faster than the reports: it is at rest, its deepest point held on the cap.
    expect(a.textContent).toContain("3.00");
    expect(a.textContent).toContain(t.peakChip);
    expect(spoken(a)).toContain("deepest 3.00 mm");
  });

  it("announces the followed key's depth only when asked, at most four times a second", async () => {
    install("depth_live", { frames: 4 });
    renderTab();
    await startView();
    const live = document.querySelector('[aria-live="polite"].sr-only');
    await sleep(300);
    expect(live?.textContent).toBe("");
    fireEvent.click(screen.getByRole("checkbox", { name: t.options.announce }));
    await sleep(300);
    expect(live?.textContent).toBe("W 1.60 mm");
  });

  it("stops on Stop and is ready to start again", async () => {
    install("depth_live");
    renderTab();
    await startView();
    fireEvent.click(button(t.controls.stop));
    expect(await screen.findByRole("button", { name: t.controls.start })).toBeTruthy();
    expect(stopRequests()).toEqual(["requested"]);
    // A requested stop needs no explanation.
    expect(screen.queryByText(t.stopped.requested)).toBeNull();
  });
});

describe("the keys being tested", () => {
  /** Dispatches a key event at the focused element (or the body); true when its default was prevented. */
  function press(type: "keydown" | "keyup", key: string, init: KeyboardEventInit = {}): boolean {
    const target = document.activeElement ?? document.body;
    const event = new KeyboardEvent(type, { key, bubbles: true, cancelable: true, ...init });
    target.dispatchEvent(event);
    return event.defaultPrevented;
  }

  it("don't scroll the page or press the focused control while the view runs, and pass when idle", async () => {
    install("depth_live");
    renderTab();
    // Idle: the page's own keys work.
    expect(press("keydown", " ")).toBe(false);
    expect(press("keydown", "ArrowDown")).toBe(false);
    await startView();
    expect(screen.getByText(t.controls.keysHeld)).toBeTruthy();
    // The focus left the Start/Stop button for the board.
    expect(document.activeElement?.tagName).not.toBe("BUTTON");
    for (const key of [" ", "ArrowDown", "PageDown", "End", "Tab", "Enter", "w"]) {
      expect(press("keydown", key), key).toBe(true);
      expect(press("keyup", key), key).toBe(true);
    }
    // A switch clicked before keeps its state when Space is tested.
    const trigger = screen.getByRole("switch", { name: t.options.showTrigger });
    fireEvent.click(trigger);
    const checked = trigger.getAttribute("aria-checked") ?? String((trigger as HTMLInputElement).checked);
    trigger.focus();
    expect(press("keydown", " ")).toBe(true);
    expect(press("keyup", " ")).toBe(true);
    expect(trigger.getAttribute("aria-checked") ?? String((trigger as HTMLInputElement).checked)).toBe(checked);
    // Shortcuts with Ctrl, Alt or Win pass.
    expect(press("keydown", "r", { ctrlKey: true })).toBe(false);
    expect(stopRequests()).toEqual([]);
  });

  it("Esc stops the view, and the keys pass again", async () => {
    install("depth_live");
    renderTab();
    await startView();
    act(() => {
      press("keydown", "Escape");
    });
    expect(await screen.findByRole("button", { name: t.controls.start })).toBeTruthy();
    expect(stopRequests()).toEqual(["requested"]);
    expect(press("keydown", " ")).toBe(false);
  });

  it("a view the tab didn't start still gets Stop", async () => {
    const source = install("depth_live");
    statusStore.set({
      ...source.initial_status,
      busy: { op: "o-9", kind: "depth_view", last: null },
    });
    renderTab();
    expect(screen.queryByRole("button", { name: t.controls.start })).toBeNull();
    expect(screen.getByText(t.controls.otherView)).toBeTruthy();
    fireEvent.click(button(t.controls.stop));
    await waitFor(() => {
      expect(stopRequests()).toEqual(["requested"]);
    });
    expect(screen.getByRole("button", { name: t.controls.stopping })).toHaveProperty("disabled", true);
  });
});

describe("automatic stops", () => {
  it("stops when the tab or the page is left", async () => {
    install("depth_stopped_page_left");
    const view = renderTab();
    await startView();
    view.unmount();
    expect(stopRequests()).toEqual(["page_left"]);
  });

  it("sends a stop again until the backend took it when the tab is left right after Start", async () => {
    // The backend registers the view a moment after Start; the stop of the unmount comes first.
    install("depth_stopped_page_left", { registerAfterMs: 150 });
    const view = renderTab();
    fireEvent.click(button(t.controls.start));
    view.unmount();
    await waitFor(
      () => {
        expect(stopAnswers).toContain(true);
      },
      { timeout: 3000 },
    );
    expect(stopAnswers[0]).toBe(false);
    expect(stopAnswers.at(-1)).toBe(true);
    expect(new Set(stopRequests())).toEqual(new Set(["page_left"]));
    await waitFor(() => {
      expect(viewEnded).toBe(true);
    });
    // Taken once: no more stops after that.
    const sent = stopRequests().length;
    await sleep(150);
    expect(stopRequests()).toHaveLength(sent);
  });

  it("sends Stop again until the backend took it when it is pressed right after Start", async () => {
    install("depth_live", { registerAfterMs: 150 });
    renderTab();
    fireEvent.click(button(t.controls.start));
    fireEvent.click(button(t.controls.stop));
    expect(await screen.findByRole("button", { name: t.controls.start }, { timeout: 3000 })).toBeTruthy();
    expect(stopAnswers[0]).toBe(false);
    expect(stopAnswers.at(-1)).toBe(true);
    expect(new Set(stopRequests())).toEqual(new Set(["requested"]));
  });

  it("stops when the window is hidden, and says why", async () => {
    install("depth_stopped_window_hidden");
    renderTab();
    await startView();
    act(() => {
      setVisibility("hidden");
    });
    expect(await screen.findByText(t.stopped.window_hidden)).toBeTruthy();
    expect(stopRequests()).toEqual(["window_hidden"]);
  });

  it("pauses after 5 minutes without movement and resumes", async () => {
    install("depth_stopped_no_movement", { ending: "at_once" });
    renderTab();
    fireEvent.click(button(t.controls.start));
    expect(await screen.findByText(t.stopped.no_movement)).toBeTruthy();
    const resume = screen.getAllByRole("button", { name: t.controls.resume });
    expect(resume.length).toBeGreaterThan(0);
    fireEvent.click(resume[0] as HTMLElement);
    await waitFor(() => {
      expect(commands().filter((c) => c === "start_depth_view")).toHaveLength(2);
    });
  });

  it.each([
    ["depth_stopped_other_program", t.stopped.other_program],
    ["depth_stopped_profile_changed", t.stopped.profile_changed],
    ["depth_stopped_channel_closed", t.stopped.channel_closed],
  ] as const)("explains a stop the backend decided (%s)", async (id, text) => {
    install(id, { ending: "at_once" });
    renderTab();
    fireEvent.click(button(t.controls.start));
    expect(await screen.findByText(text)).toBeTruthy();
  });

  it("offers Stop it when a reset left the test maybe open", async () => {
    install("depth_stopped_reset", { ending: "at_once", fallback: true });
    renderTab();
    fireEvent.click(button(t.controls.start));
    expect(await screen.findByText(t.stopped.keyboard_reset)).toBeTruthy();
    expect(screen.getByText(t.close.notSent)).toBeTruthy();
    expect(screen.getByText(t.stray.title)).toBeTruthy();
    fireEvent.click(button(t.stray.stop));
    expect(await screen.findByText(t.stray.closed)).toBeTruthy();
    expect(commands()).toContain("close_stray_travel_test");
    expect(screen.queryByText(t.stray.title)).toBeNull();
  });
});

describe("a test left open", () => {
  it("offers Stop it for a marker an earlier run left, on a click only", async () => {
    install("depth_stray_marker", {
      status: {
        connection: { state: "ready" },
        active_profile: 1,
        write_gate: { allowed: true, reasons: [] },
        snapshot_revision: 1,
        travel: { state: "closed", stray_reports: 0, marker_left: true },
      },
    });
    renderTab();
    expect(screen.getByText(t.stray.title)).toBeTruthy();
    expect(commands()).toEqual([]);
    fireEvent.click(button(t.stray.stop));
    expect(await screen.findByText(t.stray.closed)).toBeTruthy();
    expect(commands()).toEqual(["close_stray_travel_test"]);
  });

  it("asks for the wraith.software tick before Stop it too", () => {
    install("depth_stray_marker", {
      status: {
        connection: { state: "ready" },
        write_gate: { allowed: true, reasons: [] },
        travel: { state: "maybe_open", stray_reports: 3, marker_left: false },
      },
    });
    resetSession();
    renderTab();
    expect(button(t.stray.stop)).toHaveProperty("disabled", true);
    expect(screen.getAllByText(en.flow.review.vendorTickNeeded).length).toBeGreaterThan(0);
  });
});
