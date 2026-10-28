// The Dead band tab on the mocked IPC: the preview's mock backend answers
// with the fixtures the core's tests recorded from the real worker on the
// simulator (dead_band_*), through the real `api`, `invoke` and `Channel`.
//
// Covered: the keys and the editor, no IPC while editing, draft → review →
// write → report (with the state read back), the backend's soft warnings,
// cancel, a refused value, the persistence option, blocked states
// (disconnected, another program, the live depth view), no scope badge, the
// draft per tab and its discard on a profile change, Turkish.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ToastRegion } from "../../../components/Toasts";
import { DICTIONARIES, setLang } from "../../../i18n";
import type { DeadBandEditIntent, DeadBandState, DeviceSnapshot, DeviceStatus } from "../../../ipc/bindings";
import { createMockBackend } from "../../../preview/mockBackend";
import { FIXTURES } from "../../../preview/registry";
import type { Fixture } from "../../../preview/types";
import { appInfoStore, DEFAULT_APP_INFO } from "../../../state/appInfo";
import { draftEntries } from "../../../state/drafts";
import { acknowledgeVendor, resetSession } from "../../../state/session";
import { snapshotStore } from "../../../state/snapshot";
import { INITIAL_STATUS, statusStore } from "../../../state/status";
import { clearToasts } from "../../../state/toasts";
import DeadbandTab from "./index";
import { deadBandDrafts, deadBandPart } from "./store";

const en = DICTIONARIES.en;
const tr = DICTIONARIES.tr;
const t = en.actuation.deadband;
const W = 46;

interface Call {
  command: string;
  args: Record<string, unknown>;
}

let calls: Call[] = [];

function fixture(id: string): Fixture {
  const found = FIXTURES[id];
  if (found === undefined) throw new Error(`no fixture ${id}`);
  return structuredClone(found);
}

function snapshotOf(source: Fixture): DeviceSnapshot {
  if (source.initial_snapshot === null) throw new Error(`no snapshot in ${source.id}`);
  return structuredClone(source.initial_snapshot);
}

/** Installs the mock backend for a fixture (optionally changing its recorded state) and puts its state into the stores. */
function install(
  id: string,
  patch: { status?: Partial<DeviceStatus>; snapshot?: DeviceSnapshot | null; state?: (state: DeadBandState) => DeadBandState } = {},
): Fixture {
  const source = fixture(id);
  if (patch.state !== undefined) {
    for (const call of source.calls) {
      if (call.command === "get_dead_band_state" && call.ok !== null) call.ok = patch.state(call.ok);
    }
  }
  const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
  mockIPC((command, args) => {
    calls.push({ command, args: (args ?? {}) as Record<string, unknown> });
    return backend.handle(command, args);
  });
  statusStore.set({ ...source.initial_status, ...patch.status });
  snapshotStore.set({
    snapshot: patch.snapshot === undefined ? snapshotOf(source) : patch.snapshot,
    loading: false,
    error: null,
  });
  appInfoStore.set(source.app_info);
  return source;
}

function renderTab() {
  return render(
    <>
      <DeadbandTab />
      <ToastRegion />
    </>,
  );
}

function commands(): string[] {
  return calls.map((call) => call.command);
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

function clickKey(index: number): void {
  const element = keyElement(index);
  fireEvent.pointerDown(element, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
  fireEvent.pointerUp(element, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
}

function button(name: string): HTMLElement {
  return screen.getByRole("button", { name });
}

function applyButton(): HTMLElement {
  return button(en.common.actions.reviewAndWrite);
}

function setZone(which: "top" | "bottom", value: number): void {
  const words = t[which];
  const toggle = screen.getByRole("switch", { name: words.change });
  if (toggle.getAttribute("aria-checked") !== "true") fireEvent.click(toggle);
  fireEvent.change(screen.getByRole("slider", { name: words.label }), { target: { value: String(value) } });
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

async function pressWrite(): Promise<void> {
  const write = await screen.findByRole("button", { name: en.common.actions.writeToKeyboard });
  await sleep(560);
  fireEvent.click(write);
}

function dialogAction(name: string): HTMLElement {
  const dialog = screen.getByRole("dialog");
  const actions = dialog.querySelector<HTMLElement>(".ow-dialog__actions");
  if (actions === null) throw new Error("no dialog actions");
  return within(actions).getByRole("button", { name });
}

function preparedIntents(): DeadBandEditIntent[] {
  return calls.filter((c) => c.command === "prepare_dead_band_edit").map((c) => c.args["intent"] as DeadBandEditIntent);
}

function recordedIntent(source: Fixture): DeadBandEditIntent {
  const call = source.calls.find((c) => c.command === "prepare_dead_band_edit");
  if (call?.command !== "prepare_dead_band_edit") throw new Error(`no prepare in ${source.id}`);
  return call.args.intent;
}

/** Waits until the recorded state has been fetched and the board is shown. */
async function ready(): Promise<void> {
  await screen.findByRole("listbox", { name: en.actuation.board.label });
}

beforeEach(() => {
  setLang("en");
  deadBandPart.reset();
  deadBandDrafts.reset();
  resetSession();
  acknowledgeVendor();
  calls = [];
});

afterEach(() => {
  clearMocks();
  clearToasts();
  statusStore.set(INITIAL_STATUS);
  snapshotStore.set({ snapshot: null, loading: false, error: null });
  appInfoStore.set(DEFAULT_APP_INFO);
});

describe("the dead band tab", () => {
  it("shows the keys of the active profile, with no scope badge", async () => {
    install("dead_band_ready");
    renderTab();
    await ready();
    expect(screen.getByRole("heading", { level: 1, name: en.shell.nav.actuation })).toBeTruthy();
    expect(screen.getByText(en.actuation.board.title({ n: 1 }))).toBeTruthy();
    expect(within(screen.getByRole("listbox")).getAllByRole("option")).toHaveLength(82);
    expect(spoken(keyElement(W))).toContain("top dead zone 0.00 mm, bottom dead zone 0.00 mm");
    expect(keyElement(15).getAttribute("aria-disabled")).toBe("true");
    // The dead band is per profile (confirmed on hardware): no "Applies to all profiles".
    expect(document.querySelector("[data-badge]")).toBeNull();
    expect(commands()).toEqual(["get_dead_band_state"]);
  });

  it("edits a local draft only, registered under its tab", async () => {
    install("dead_band_ready");
    renderTab();
    await ready();
    fireEvent.click(button(en.actuation.select.wasd));
    setZone("top", 5);
    setZone("bottom", 10);
    expect(commands()).toEqual(["get_dead_band_state"]);
    expect(spoken(keyElement(W))).toContain("new: top 0.10 mm, bottom 0.20 mm, not written yet");
    expect(keyElement(W).textContent).toContain("0.10");
    expect(screen.getByText(/4 keys will change/)).toBeTruthy();
    expect(draftEntries()).toContainEqual({ page: "actuation", tab: "deadband", count: null });
    fireEvent.click(button(en.common.actions.discard));
    expect(screen.getByText(t.apply.idle)).toBeTruthy();
    expect(draftEntries()).not.toContainEqual({ page: "actuation", tab: "deadband", count: null });
  });

  it("shows soft warnings against the keys' actuation points before and in the review", async () => {
    const source = install("dead_band_preview");
    renderTab();
    await ready();
    fireEvent.click(button(en.actuation.select.wasd));
    setZone("top", 50);
    setZone("bottom", 10);
    const warning = t.hints.top_reaches_actuation({ key: "W", actuation: "1.00 mm" });
    expect(screen.getByText(warning)).toBeTruthy();
    fireEvent.click(applyButton());
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText(t.review.title({ n: 4, profile: 1 }))).toBeTruthy();
    expect(preparedIntents()).toEqual([recordedIntent(source)]);
    expect(within(dialog).getByText(warning)).toBeTruthy();
    expect(within(dialog).getByText(/dead band \(26 01\)/)).toBeTruthy();
    // The persistence check is offered, not ticked.
    const check = within(dialog).getByRole("checkbox", { name: en.actuation.edit.persistence.check({ m: 2 }) });
    expect(check).toHaveProperty("checked", false);
  });
});

describe("the write flow", () => {
  it("plans, confirms, writes, reports with Undo and shows the values read back", async () => {
    const source = install("dead_band_apply_ok");
    renderTab();
    await ready();
    fireEvent.click(button(en.actuation.select.wasd));
    setZone("top", 5);
    setZone("bottom", 5);
    fireEvent.click(applyButton());
    expect(await screen.findByText(t.review.title({ n: 4, profile: 1 }))).toBeTruthy();
    expect(preparedIntents()).toEqual([recordedIntent(source)]);
    expect(commands()).not.toContain("apply_dead_band_edit");
    await pressWrite();
    expect(await screen.findByText(en.flow.results.written)).toBeTruthy();
    expect(commands().filter((c) => c === "apply_dead_band_edit")).toHaveLength(1);
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText(en.flow.tables.dead_band)).toBeTruthy();
    expect(within(dialog).getByText(en.flow.footer.safetyBackup)).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: en.common.actions.undo })).toBeTruthy();
    // The tab reads the dead band again (its own revision changed; the snapshot's didn't).
    await waitFor(() => {
      expect(commands().filter((c) => c === "get_dead_band_state")).toHaveLength(2);
    });
    fireEvent.click(dialogAction(en.common.actions.close));
    expect(screen.getByText(t.apply.idle)).toBeTruthy();
    await waitFor(() => {
      expect(spoken(keyElement(W))).toContain("top dead zone 0.10 mm, bottom dead zone 0.10 mm");
    });
  });

  it("prepares again with the persistence check when it is ticked", async () => {
    install("dead_band_preview");
    renderTab();
    await ready();
    fireEvent.click(button(en.actuation.select.wasd));
    setZone("top", 50);
    fireEvent.click(applyButton());
    const check = await screen.findByRole("checkbox", { name: en.actuation.edit.persistence.check({ m: 2 }) });
    fireEvent.click(check);
    await waitFor(() => {
      expect(preparedIntents()).toHaveLength(2);
    });
    expect(preparedIntents()[1]?.check_persistence).toBe(true);
    expect(commands()).toContain("discard_plan");
  });

  it("never carries the persistence tick into the next flow", async () => {
    install("dead_band_preview");
    renderTab();
    await ready();
    fireEvent.click(button(en.actuation.select.wasd));
    setZone("top", 50);
    fireEvent.click(applyButton());
    fireEvent.click(await screen.findByRole("checkbox", { name: en.actuation.edit.persistence.check({ m: 2 }) }));
    await waitFor(() => {
      expect(preparedIntents()).toHaveLength(2);
    });
    await screen.findByText(t.review.title({ n: 4, profile: 1 }));
    fireEvent.click(dialogAction(en.common.actions.cancel));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    fireEvent.click(applyButton());
    await screen.findByText(t.review.title({ n: 4, profile: 1 }));
    expect(preparedIntents().map((intent) => intent.check_persistence)).toEqual([false, true, false]);
  });

  it("stays open when the keyboard goes away, and doesn't prepare by itself when it is back", async () => {
    const source = install("dead_band_preview");
    renderTab();
    await ready();
    fireEvent.click(button(en.actuation.select.wasd));
    setZone("top", 50);
    fireEvent.click(applyButton());
    await screen.findByText(t.review.title({ n: 4, profile: 1 }));
    act(() => {
      statusStore.set({
        ...source.initial_status,
        connection: { state: "searching" },
        write_gate: { allowed: false, reasons: ["not_connected"] },
        snapshot_revision: null,
      });
    });
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByText(t.review.title({ n: 4, profile: 1 }))).toBeTruthy();
    act(() => {
      statusStore.set({ ...source.initial_status, snapshot_revision: (source.initial_status.snapshot_revision ?? 0) + 5 });
    });
    await waitFor(() => {
      expect(commands().filter((c) => c === "get_dead_band_state").length).toBeGreaterThan(1);
    });
    await sleep(20);
    expect(preparedIntents()).toHaveLength(1);
    expect(screen.queryByText(en.errors.kind.not_connected.title)).toBeNull();
  });

  it("discards the plan on Cancel and keeps the draft", async () => {
    install("dead_band_preview");
    renderTab();
    await ready();
    fireEvent.click(button(en.actuation.select.wasd));
    setZone("top", 50);
    fireEvent.click(applyButton());
    await screen.findByText(t.review.title({ n: 4, profile: 1 }));
    fireEvent.click(dialogAction(en.common.actions.cancel));
    await waitFor(() => {
      expect(commands()).toContain("discard_plan");
    });
    expect(commands()).not.toContain("apply_dead_band_edit");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText(/4 keys will change/)).toBeTruthy();
  });

  it("explains a value the backend refuses (never clamped) and writes nothing", async () => {
    // Pretend the tab allowed 1.02 mm: the backend still refuses it.
    install("dead_band_refused_range", { state: (state) => ({ ...state, max: 60 }) });
    renderTab();
    await ready();
    clickKey(W);
    setZone("top", 51);
    fireEvent.click(applyButton());
    expect(await screen.findByText(en.errors.kind.invalid_request.title)).toBeTruthy();
    expect(
      screen.getByText(
        en.actuation.edit.problems.out_of_range({
          field: en.actuation.edit.fields.top,
          value: "1.02 mm",
          min: "0.00 mm",
          max: "1.00 mm",
        }),
      ),
    ).toBeTruthy();
    expect(screen.getByText(en.flow.nothingWritten)).toBeTruthy();
  });
});

describe("blocked states", () => {
  it.each([
    [{ reason: "unreadable", message: "x" }, t.disabled.unreadable],
    [{ reason: "distance_unit", unit_um: 10 }, t.disabled.distance_unit],
  ] as const)("says why the dead band can't be changed (%o) and keeps Review disabled", async (disabled, text) => {
    install("dead_band_ready", { state: (state) => ({ ...state, editable: false, disabled }) });
    renderTab();
    await ready();
    expect(screen.getAllByText(text).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByRole("status").textContent).toContain(text);
    fireEvent.click(button(en.actuation.select.wasd));
    expect(applyButton()).toHaveProperty("disabled", true);
  });

  it("says another program uses the keyboard instead of asking to connect it", () => {
    install("dead_band_ready", {
      status: {
        connection: { state: "other_program", evidence: { status_reports: 1, foreign_acks: 0, profile_reports: 0 } },
        write_gate: { allowed: false, reasons: ["other_program"] },
        snapshot_revision: null,
      },
      snapshot: null,
    });
    renderTab();
    expect(screen.getByText(t.empty.title)).toBeTruthy();
    expect(screen.queryByText(t.empty.body)).toBeNull();
    expect(screen.getByText(en.shell.status.otherProgram)).toBeTruthy();
  });

  it("shows an empty state while no keyboard is read", () => {
    install("dead_band_ready", {
      status: {
        connection: { state: "searching" },
        write_gate: { allowed: false, reasons: ["not_connected"] },
        snapshot_revision: null,
      },
      snapshot: null,
    });
    renderTab();
    expect(screen.getByText(t.empty.title)).toBeTruthy();
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(commands()).toEqual([]);
  });

  it("names the write gate's reason under the button", async () => {
    install("dead_band_ready", {
      status: {
        connection: { state: "other_program", evidence: { status_reports: 1, foreign_acks: 0, profile_reports: 0 } },
        write_gate: { allowed: false, reasons: ["other_program"] },
      },
    });
    renderTab();
    await ready();
    fireEvent.click(button(en.actuation.select.wasd));
    setZone("top", 5);
    expect(applyButton()).toHaveProperty("disabled", true);
    expect(screen.getByText(en.errors.writeBlocked.other_program)).toBeTruthy();
  });

  it("says the live depth view holds the keyboard while it runs", async () => {
    install("dead_band_ready", {
      status: { busy: { op: "o-9", kind: "depth_view", last: null }, write_gate: { allowed: false, reasons: ["busy"] } },
    });
    renderTab();
    await ready();
    fireEvent.click(button(en.actuation.select.wasd));
    setZone("top", 5);
    expect(applyButton()).toHaveProperty("disabled", true);
    expect(screen.getByText(en.actuation.edit.depthRunning)).toBeTruthy();
  });

  it("discards the draft with a toast when the profile changes", async () => {
    const source = install("dead_band_ready");
    renderTab();
    await ready();
    fireEvent.click(button(en.actuation.select.wasd));
    setZone("top", 5);
    act(() => {
      snapshotStore.set({ snapshot: { ...snapshotOf(source), profile: 2, revision: 2 }, loading: false, error: null });
    });
    expect(screen.getByText(t.discarded({ n: 2 }))).toBeTruthy();
    expect(screen.getByText(t.apply.idle)).toBeTruthy();
  });
});

describe("Turkish", () => {
  it("shows the tab in Turkish with decimal commas", async () => {
    install("dead_band_ready");
    setLang("tr");
    renderTab();
    await screen.findByRole("listbox", { name: tr.actuation.board.label });
    expect(screen.getByRole("heading", { level: 1, name: tr.shell.nav.actuation })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: tr.actuation.select.wasd }));
    fireEvent.click(screen.getByRole("switch", { name: tr.actuation.deadband.top.change }));
    fireEvent.change(screen.getByRole("slider", { name: tr.actuation.deadband.top.label }), { target: { value: "5" } });
    expect(keyElement(W).textContent).toContain("0,10");
  });
});
