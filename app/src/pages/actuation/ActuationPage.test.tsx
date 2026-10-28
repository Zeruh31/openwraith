// The actuation page on the mocked IPC: the preview's mock backend answers
// with the fixtures the core's tests recorded from the real worker on the
// simulator, through the real `api`, `invoke` and `Channel`.
//
// Covered: the keys and their marks, selection tools, no IPC while editing,
// plan → confirm → progress → report, one prepare per double click, keys
// that can't be changed left out, cancel, refused and stale applies,
// nothing to do, blocked states, and the draft discarded on a profile change.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ToastRegion } from "../../components/Toasts";
import { DICTIONARIES, setLang } from "../../i18n";
import type { DeviceSnapshot, DeviceStatus, TriggerEditIntent } from "../../ipc/bindings";
import { createMockBackend } from "../../preview/mockBackend";
import { FIXTURES } from "../../preview/registry";
import type { Fixture } from "../../preview/types";
import { appInfoStore, DEFAULT_APP_INFO } from "../../state/appInfo";
import { draftEntries, draftPages } from "../../state/drafts";
import { acknowledgeVendor, resetSession } from "../../state/session";
import { snapshotStore } from "../../state/snapshot";
import { INITIAL_STATUS, statusStore } from "../../state/status";
import { clearToasts } from "../../state/toasts";
import { resetActuationStore } from "./draftStore";
import ActuationPage from "./index";

const en = DICTIONARIES.en;
const tr = DICTIONARIES.tr;
const W = 46;
const Q = 45;
const E = 47;

interface Call {
  command: string;
  args: Record<string, unknown>;
}

let calls: Call[] = [];

function fixture(id: string): Fixture {
  const found = FIXTURES[id];
  if (found === undefined) throw new Error(`no fixture ${id}`);
  return found;
}

function snapshotOf(source: Fixture): DeviceSnapshot {
  if (source.initial_snapshot === null) throw new Error(`no snapshot in ${source.id}`);
  return structuredClone(source.initial_snapshot);
}

/** Installs the mock backend for a fixture and puts its state into the stores. */
function install(
  id: string,
  patch: { status?: Partial<DeviceStatus>; snapshot?: DeviceSnapshot | null; speed?: number } = {},
) {
  const source = fixture(id);
  const backend = createMockBackend(source, {
    speed: patch.speed ?? 0,
    replayEvents: false,
    fallback: false,
    library: FIXTURES,
  });
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

function renderPage() {
  return render(
    <>
      <ActuationPage />
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

/** What a screen reader says for a key: its name and its description. */
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

function setActuation(value: number): void {
  const toggle = screen.getByRole("switch", { name: en.actuation.actuationControl.change });
  if (toggle.getAttribute("aria-checked") !== "true") fireEvent.click(toggle);
  fireEvent.change(screen.getByRole("slider", { name: en.actuation.actuationControl.label }), {
    target: { value: String(value) },
  });
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

/** Waits until Write is armed (500 ms) and presses it. */
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

function preparedIntent(): TriggerEditIntent {
  const call = calls.find((c) => c.command === "prepare_trigger_edit");
  if (call === undefined) throw new Error("prepare_trigger_edit was not called");
  return call.args["intent"] as TriggerEditIntent;
}

beforeEach(() => {
  setLang("en");
  resetActuationStore();
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

describe("the keyboard", () => {
  it("shows each key's value, Rapid Trigger, changed keys and keys that can't be changed", () => {
    install("trigger_ready_mixed");
    renderPage();
    const board = screen.getByRole("listbox", { name: en.actuation.board.label });
    expect(within(board).getAllByRole("option")).toHaveLength(82);

    const w = keyElement(W);
    expect(w.getAttribute("aria-label")).toContain("1.20");
    expect(spoken(w)).toContain("Rapid Trigger 0.30 mm");
    expect(w.textContent).toContain(en.actuation.chip);
    expect(w.hasAttribute("data-flag")).toBe(true);
    expect(keyElement(0).hasAttribute("data-flag")).toBe(false);

    const e = keyElement(E);
    expect(e.textContent).toContain("⚠");
    expect(e.getAttribute("title")).toBe(en.errors.notEditable.press_release_differ);

    const knob = keyElement(15);
    expect(knob.getAttribute("aria-disabled")).toBe("true");
    expect(knob.hasAttribute("data-flag")).toBe(false);
  });

  it("switches the keys to Rapid Trigger values", () => {
    install("trigger_ready_mixed");
    renderPage();
    fireEvent.click(screen.getByRole("radio", { name: en.actuation.show.rapidTrigger }));
    expect(keyElement(Q).textContent).toContain("↓0.30 ↑0.50");
    expect(keyElement(0).textContent).toContain(en.actuation.values.off);
  });

  it("selects with the tools and summarizes the selection", () => {
    install("trigger_ready_mixed");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    expect(screen.getByText(en.actuation.inspector.count({ n: 4 }))).toBeTruthy();
    expect(screen.getByText("Actuation 1.20 mm")).toBeTruthy();
    expect(screen.getByText("RT 0.30 mm")).toBeTruthy();

    fireEvent.change(screen.getByLabelText(en.actuation.select.rowLabel), { target: { value: "top" } });
    expect(screen.getByText(en.actuation.inspector.count({ n: 15 }))).toBeTruthy();
    expect(screen.getByText(en.actuation.inspector.leftOut({ key: "E" }))).toBeTruthy();

    fireEvent.click(button(en.actuation.select.none));
    expect(screen.getByText(en.actuation.inspector.none)).toBeTruthy();

    fireEvent.click(button(en.actuation.select.changed));
    expect(screen.getByText(en.actuation.inspector.count({ n: 6 }))).toBeTruthy();
  });

  it("selects keys by clicking them; the knob never", () => {
    install("trigger_ready_mixed");
    renderPage();
    clickKey(W);
    clickKey(15);
    expect(keyElement(W).getAttribute("aria-selected")).toBe("true");
    expect(keyElement(15).getAttribute("aria-selected")).toBe("false");
    expect(screen.getByText(en.actuation.inspector.count({ n: 1 }))).toBeTruthy();
  });
});

describe("editing", () => {
  it("sends nothing to the keyboard while values are changed", () => {
    install("trigger_ready_mixed");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(50);
    const field = screen.getByRole("textbox", { name: en.actuation.actuationControl.label });
    fireEvent.focus(field);
    fireEvent.change(field, { target: { value: "1,1" } });
    fireEvent.blur(field);
    fireEvent.click(screen.getByRole("radio", { name: en.actuation.rt.separate }));
    fireEvent.change(screen.getByRole("slider", { name: en.actuation.rt.press }), { target: { value: "10" } });

    expect(commands()).toEqual([]);
    expect(spoken(keyElement(W))).toContain("new setting 1.10 mm · RT 0.20 / 0.30 mm, not written yet");
    expect(keyElement(W).textContent).toContain("1.10");
    expect(keyElement(W).textContent).toContain("●");
    expect(screen.getByText(/4 keys will change/)).toBeTruthy();
    expect(draftPages()).toContain("actuation");
  });

  it("registers the draft under the trigger tab", () => {
    install("trigger_ready_mixed");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(50);
    expect(draftEntries()).toContainEqual({ page: "actuation", tab: "trigger", count: null });
  });

  it("says the live depth view holds the keyboard while it runs", () => {
    install("trigger_ready_mixed", {
      status: { busy: { op: "o-9", kind: "depth_view", last: null }, write_gate: { allowed: false, reasons: ["busy"] } },
    });
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(50);
    expect(applyButton()).toHaveProperty("disabled", true);
    expect(screen.getByText(en.actuation.edit.depthRunning)).toBeTruthy();
  });

  it("discards the draft", () => {
    install("trigger_ready_mixed");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(50);
    fireEvent.click(button(en.common.actions.discard));
    expect(screen.getByText(en.actuation.apply.idle)).toBeTruthy();
    expect(draftPages()).not.toContain("actuation");
  });

  it("says why it can't write when the selected keys already have the settings", () => {
    install("trigger_ready_mixed");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(60);
    expect(applyButton()).toHaveProperty("disabled", true);
    expect(screen.getByText(en.actuation.apply.reasons.nothing)).toBeTruthy();
  });

  it("leaves keys that can't be changed out of the intent", async () => {
    install("trigger_ready_mixed");
    renderPage();
    clickKey(Q);
    clickKey(W);
    clickKey(E);
    setActuation(70);
    expect(screen.getByText(/1 left out/)).toBeTruthy();
    fireEvent.click(applyButton());
    await waitFor(() => {
      expect(commands()).toContain("prepare_trigger_edit");
    });
    expect(preparedIntent()).toEqual({ keys: [Q, W], actuation: 70, rapid_trigger: null, based_on: 1 });
  });
});

describe("the write flow", () => {
  it("plans, confirms, writes with progress and reports with Undo", async () => {
    const source = install("trigger_apply_ok");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(60);
    fireEvent.click(screen.getByRole("radio", { name: en.actuation.rt.same }));
    expect(screen.getByRole("slider", { name: en.actuation.rt.sensitivity })).toHaveProperty("value", "15");
    expect(commands()).toEqual([]);

    fireEvent.click(applyButton());
    expect(await screen.findByText(en.actuation.review.title({ n: 4, profile: 1 }))).toBeTruthy();
    const recorded = source.calls.find((call) => call.command === "prepare_trigger_edit");
    expect(preparedIntent()).toEqual(recorded?.args.intent);
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText(en.actuation.review.packets({ n: 4 }))).toBeTruthy();
    expect(within(dialog).getAllByText("1.20 mm").length).toBeGreaterThan(0);
    expect(commands()).not.toContain("apply_trigger_edit");

    await pressWrite();
    expect(await screen.findByText(en.flow.headline.doneKeys({ n: 4 }))).toBeTruthy();
    expect(commands().filter((c) => c === "apply_trigger_edit")).toHaveLength(1);
    expect(screen.getByText(en.flow.footer.safetyBackup)).toBeTruthy();
    expect(screen.getByRole("button", { name: en.common.actions.undo })).toBeTruthy();
    expect(screen.getAllByText(en.flow.results.written)).toHaveLength(4);

    fireEvent.click(dialogAction(en.common.actions.close));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText(en.actuation.apply.idle)).toBeTruthy();
  });

  it("shows the wait for the keyboard to save while writing", async () => {
    // Recorded time at 4x speed: the 2 s save wait takes 0.5 s.
    install("trigger_apply_ok", { speed: 4 });
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(60);
    fireEvent.click(applyButton());
    await screen.findByText(en.actuation.review.title({ n: 4, profile: 1 }));
    await pressWrite();
    expect(await screen.findByText(en.flow.progress.waitProfile({ n: 1 }))).toBeTruthy();
    expect(screen.getByText(en.flow.progress.keepPlugged)).toBeTruthy();
    expect(await screen.findByText(en.flow.headline.doneKeys({ n: 4 }), {}, { timeout: 3000 })).toBeTruthy();
  });

  it("prepares once for a double click", async () => {
    install("trigger_apply_ok");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(60);
    const apply = applyButton();
    fireEvent.click(apply);
    fireEvent.click(apply);
    await screen.findByText(en.actuation.review.title({ n: 4, profile: 1 }));
    await sleep(20);
    expect(commands().filter((c) => c === "prepare_trigger_edit")).toHaveLength(1);
  });

  it("discards the plan on Cancel and writes nothing", async () => {
    install("trigger_preview");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(60);
    fireEvent.click(applyButton());
    await screen.findByText(en.actuation.review.title({ n: 4, profile: 1 }));
    fireEvent.click(dialogAction(en.common.actions.cancel));
    await waitFor(() => {
      expect(commands()).toContain("discard_plan");
    });
    expect(commands()).not.toContain("apply_trigger_edit");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText(/1 key will change|4 keys will change/)).toBeTruthy();
  });

  it.each([
    ["trigger_refused_other_program", "other_program_active"],
    ["trigger_stale_plan", "stale_plan"],
  ] as const)("shows a refused apply (%s) as nothing written and keeps the draft", async (id, kind) => {
    install(id);
    renderPage();
    clickKey(W);
    setActuation(60);
    fireEvent.click(applyButton());
    await screen.findByText(en.actuation.review.title({ n: 1, profile: 1 }));
    await pressWrite();
    expect(await screen.findByText(en.errors.kind[kind].title)).toBeTruthy();
    expect(screen.getByText(en.flow.nothingWritten)).toBeTruthy();
    fireEvent.click(dialogAction(en.common.actions.close));
    expect(screen.getByText(en.actuation.apply.willChange({ n: 1 }), { exact: false })).toBeTruthy();
  });

  it("ends without writing when the keyboard already has the settings", async () => {
    install("trigger_nothing_to_do");
    renderPage();
    clickKey(W);
    setActuation(60);
    fireEvent.click(applyButton());
    expect(await screen.findByText(en.actuation.review.titleNothing)).toBeTruthy();
    expect(screen.getByText(en.actuation.review.nothing)).toBeTruthy();
    expect(screen.queryByRole("button", { name: en.common.actions.writeToKeyboard })).toBeNull();
  });

  it("shows a prepare error (distance unit) as nothing written", async () => {
    // Pretend the page allowed it: the backend still refuses (CLI parity: unit check before any write).
    const snapshot = snapshotOf(fixture("trigger_unit_mismatch"));
    install("trigger_unit_mismatch", {
      snapshot: { ...snapshot, triggers: { ...snapshot.triggers, editable: true, disabled: null } },
    });
    renderPage();
    clickKey(W);
    setActuation(70);
    fireEvent.click(applyButton());
    expect(await screen.findByText(en.errors.kind.invalid_request.title)).toBeTruthy();
    expect(screen.getByText(en.errors.detail.distance_unit({ um: 30 }))).toBeTruthy();
    expect(screen.getByText(en.flow.nothingWritten)).toBeTruthy();
  });
});

describe("blocked states", () => {
  it("shows an empty state while no keyboard is read", () => {
    install("trigger_ready_mixed", {
      status: { connection: { state: "searching" }, write_gate: { allowed: false, reasons: ["not_connected"] } },
      snapshot: null,
    });
    renderPage();
    expect(screen.getByText(en.actuation.empty.title)).toBeTruthy();
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("names the write-gate reason under the button", () => {
    install("trigger_ready_mixed", {
      status: {
        connection: { state: "other_program", evidence: { status_reports: 1, foreign_acks: 0, profile_reports: 0 } },
        write_gate: { allowed: false, reasons: ["other_program"] },
      },
    });
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(50);
    expect(applyButton()).toHaveProperty("disabled", true);
    expect(screen.getByText(en.errors.writeBlocked.other_program)).toBeTruthy();
  });

  it("is read-only when the keyboard reports another distance unit", () => {
    install("trigger_unit_mismatch");
    renderPage();
    expect(screen.getByText(en.actuation.disabled.distance_unit)).toBeTruthy();
    expect(screen.getByRole("switch", { name: en.actuation.actuationControl.change })).toHaveProperty("disabled", true);
    // Values are shown in the keyboard's own unit (0.03 mm): 85 units = 2.55 mm.
    expect(keyElement(0).textContent).toContain("2.55");
  });

  it("is read-only while the values may be out of date", () => {
    install("trigger_ready_mixed", {
      status: { snapshot_stale: true, write_gate: { allowed: false, reasons: ["snapshot_stale"] } },
    });
    renderPage();
    expect(screen.getByText(en.actuation.stale)).toBeTruthy();
    expect(screen.getByRole("switch", { name: en.actuation.actuationControl.change })).toHaveProperty("disabled", true);
  });

  it("jumps from the apply bar to the inspector", () => {
    install("trigger_ready_mixed");
    renderPage();
    expect(screen.queryByRole("button", { name: en.actuation.apply.setValues })).toBeNull();
    fireEvent.click(button(en.actuation.select.wasd));
    fireEvent.click(screen.getByRole("button", { name: en.actuation.apply.setValues }));
    const anchor = document.querySelector(".act-inspector-anchor");
    expect(anchor).not.toBeNull();
    expect(anchor?.contains(document.activeElement)).toBe(true);
  });

  it("discards the draft with a toast when the profile changes", () => {
    const source = install("trigger_ready_mixed");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(50);
    act(() => {
      snapshotStore.set({ snapshot: { ...snapshotOf(source), profile: 2, revision: 2 }, loading: false, error: null });
    });
    expect(screen.getByText(en.actuation.draftDiscarded({ n: 2 }))).toBeTruthy();
    expect(screen.getByText(en.actuation.apply.idle)).toBeTruthy();
    // The selection is kept.
    expect(screen.getByText(en.actuation.inspector.count({ n: 4 }))).toBeTruthy();
    // Back on profile 1 (same connection): the discarded draft stays discarded.
    act(() => {
      snapshotStore.set({ snapshot: { ...snapshotOf(source), profile: 1, revision: 3 }, loading: false, error: null });
    });
    expect(screen.getByText(en.actuation.apply.idle)).toBeTruthy();
    expect(draftPages()).not.toContain("actuation");
  });

  it("discards the draft when the keyboard is disconnected and connected again", () => {
    const source = install("trigger_ready_mixed");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(50);
    act(() => {
      statusStore.set({
        ...source.initial_status,
        revision: 50,
        connection: { state: "lost", error: { kind: "disconnected", detail: null, message: "unplugged" } },
        write_gate: { allowed: false, reasons: ["not_connected"] },
        snapshot_revision: null,
      });
      snapshotStore.set({ snapshot: null, loading: false, error: null });
    });
    expect(screen.getByText(en.actuation.empty.title)).toBeTruthy();
    // The draft belonged to the lost connection: it no longer counts as unwritten.
    expect(draftPages()).not.toContain("actuation");
    act(() => {
      statusStore.set({ ...source.initial_status, revision: 60, generation: 2 });
      snapshotStore.set({ snapshot: { ...snapshotOf(source), generation: 2, revision: 3 }, loading: false, error: null });
    });
    expect(screen.getByText(en.actuation.draftDiscardedReconnect)).toBeTruthy();
    expect(screen.getByText(en.actuation.apply.idle)).toBeTruthy();
    expect(draftPages()).not.toContain("actuation");
  });

  it("keeps the draft and the selection when the page is left and shown again", () => {
    install("trigger_ready_mixed");
    const page = renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(50);
    expect(screen.getByText(/4 keys will change/)).toBeTruthy();
    // Another page is shown (the route's lazy page unmounts).
    page.unmount();
    expect(draftPages()).toContain("actuation");
    renderPage();
    expect(screen.getByText(/4 keys will change/)).toBeTruthy();
    expect(screen.getByText(en.actuation.inspector.count({ n: 4 }))).toBeTruthy();
    expect(calls).toEqual([]);
  });

  it("drops the draft on a profile change made while the page is not shown", () => {
    const source = install("trigger_ready_mixed");
    const page = renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(50);
    page.unmount();
    act(() => {
      snapshotStore.set({ snapshot: { ...snapshotOf(source), profile: 2, revision: 2 }, loading: false, error: null });
    });
    expect(draftPages()).not.toContain("actuation");
    // Back on profile 1: the discarded draft does not come back.
    act(() => {
      snapshotStore.set({ snapshot: { ...snapshotOf(source), profile: 1, revision: 3 }, loading: false, error: null });
    });
    renderPage();
    expect(screen.getByText(en.actuation.apply.idle)).toBeTruthy();
    expect(screen.getByText(en.actuation.draftDiscarded({ n: 2 }))).toBeTruthy();
  });

  it("keeps the draft when the same profile is read again", () => {
    const source = install("trigger_ready_mixed");
    renderPage();
    fireEvent.click(button(en.actuation.select.wasd));
    setActuation(50);
    act(() => {
      snapshotStore.set({ snapshot: { ...snapshotOf(source), revision: 5 }, loading: false, error: null });
    });
    expect(screen.getByText(/4 keys will change/)).toBeTruthy();
  });
});

describe("Turkish", () => {
  it("shows values with a decimal comma and Turkish texts", () => {
    install("trigger_ready_mixed");
    setLang("tr");
    renderPage();
    fireEvent.click(screen.getByRole("button", { name: tr.actuation.select.wasd }));
    expect(screen.getByText(tr.actuation.inspector.count({ n: 4 }))).toBeTruthy();
    expect(screen.getByText("Aktivasyon 1,20 mm")).toBeTruthy();
    expect(keyElement(W).textContent).toContain("1,20");
  });
});
