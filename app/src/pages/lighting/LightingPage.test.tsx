// The Lighting page on mocked IPC: the real `api` talks to the browser
// preview's mock backend, which answers with the fixtures the core recorded
// from the real worker on the simulator. Covers: effects and controls per
// effect, no IPC while editing, plan → confirm → progress → report, double
// clicks, cancel, errors, a failed verify, the disconnected and blocked
// states, the unknown effect, power off, the per-profile draft and Turkish.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ToastRegion } from "../../components/Toasts";
import { DICTIONARIES, setLang } from "../../i18n";
import type { AppError, DeviceSnapshot, DeviceStatus, EditReportView, LightEditPreview } from "../../ipc/bindings";
import { createMockBackend } from "../../preview/mockBackend";
import { FIXTURES } from "../../preview/registry";
import type { Fixture } from "../../preview/types";
import { appInfoStore, DEFAULT_APP_INFO } from "../../state/appInfo";
import { acknowledgeVendor, resetSession } from "../../state/session";
import { draftPages } from "../../state/drafts";
import { snapshotStore, startSnapshotSync } from "../../state/snapshot";
import { INITIAL_STATUS, statusStore } from "../../state/status";
import { clearToasts } from "../../state/toasts";
import { resetDraftStore } from "./draftStore";
import LightingPage from "./index";

const en = DICTIONARIES.en;
const t = en.lighting;

interface Call {
  command: string;
  args: unknown;
}

let calls: Call[] = [];

beforeEach(() => {
  setLang("en");
  resetDraftStore();
  resetSession();
  acknowledgeVendor();
  clearToasts();
  calls = [];
  window.location.hash = "#/lighting";
});

afterEach(() => {
  clearMocks();
  statusStore.set(INITIAL_STATUS);
  snapshotStore.set({ snapshot: null, loading: false, error: null });
  appInfoStore.set(DEFAULT_APP_INFO);
  clearToasts();
  setLang("en");
});

function fixture(id: string): Fixture {
  const found = FIXTURES[id];
  if (found === undefined) throw new Error(`no fixture ${id}`);
  return structuredClone(found);
}

/** Loads a fixture's state into the stores and answers IPC from its recording. */
function load(source: Fixture, handler?: (command: string, args: unknown) => unknown) {
  statusStore.set(source.initial_status);
  snapshotStore.set({ snapshot: source.initial_snapshot, loading: false, error: null });
  appInfoStore.set(source.app_info);
  const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
  mockIPC((command, args) => {
    calls.push({ command, args });
    return handler === undefined ? backend.handle(command, args) : handler(command, args);
  });
  return render(
    <>
      <LightingPage />
      <ToastRegion />
    </>,
  );
}

function commands(): string[] {
  return calls.map((call) => call.command);
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

function tile(name: string): HTMLElement {
  const group = screen.getByRole("radiogroup", { name: t.effects.label });
  const found = within(group)
    .getAllByRole("radio")
    .find((element) => element.querySelector(".li-tile__name")?.textContent === name);
  if (found === undefined) throw new Error(`no tile ${name}`);
  return found;
}

function reviewButton(): HTMLElement {
  return screen.getByRole("button", { name: en.common.actions.reviewAndWrite });
}

function summaryText(): string {
  return document.querySelector(".ow-applybar__summary")?.textContent ?? "";
}

function dialogAction(name: string): HTMLElement {
  const found = screen
    .getAllByRole("button", { name })
    .find((button) => button.closest(".ow-dialog__actions") !== null);
  if (found === undefined) throw new Error(`no dialog action ${name}`);
  return found;
}

/** Static + blue: the intent recorded by the `light_*_mode_change` and `light_apply_ok` fixtures. */
function draftStaticBlue(): void {
  fireEvent.click(tile(t.modes.static));
  fireEvent.click(screen.getByRole("button", { name: t.color.names.blue }));
}

describe("Lighting page: effects and controls", () => {
  it("shows the keyboard's effects in device order and marks the one on the keyboard", () => {
    load(fixture("light_ready"));
    const group = screen.getByRole("radiogroup", { name: t.effects.label });
    const names = within(group)
      .getAllByRole("radio")
      .map((element) => element.querySelector(".li-tile__name")?.textContent);
    expect(names).toEqual([
      t.modes.wave,
      t.modes.breath,
      t.modes.neon,
      t.modes.twinkle,
      t.modes.radar,
      t.modes.static,
      t.modes.auto_ripple,
      t.modes.striation,
      t.modes.reactive,
      t.modes.aurora,
      t.modes.cross,
      t.modes.speed_respond,
      t.modes.fireworks,
      t.modes.ripple,
      t.modes.custom,
    ]);
    expect(names).not.toContain(t.modes.music_rhythm);
    const custom = tile(t.modes.custom);
    expect(custom.getAttribute("aria-checked")).toBe("true");
    expect(custom.textContent).toContain(t.effects.onKeyboard);
    expect(screen.getByText(t.perProfile.body({ n: 1 }))).toBeTruthy();
    expect(screen.getByText(t.applyBar.noChanges)).toBeTruthy();
  });

  it("shows the colour and the speed only for effects that use them", () => {
    load(fixture("light_ready"));
    // Custom (on the keyboard): stored per-key colours, no speed.
    expect(screen.getByText(t.color.customNote)).toBeTruthy();
    expect(screen.queryByRole("slider", { name: t.speed.label })).toBeNull();
    expect(screen.getByText(t.speed.notUsed)).toBeTruthy();

    fireEvent.click(tile(t.modes.wave));
    expect(screen.getByLabelText(en.common.fields.hexValue({ label: t.color.label }))).toBeTruthy();
    expect(screen.getByRole("slider", { name: t.speed.label })).toBeTruthy();
    expect(screen.getByText(t.color.fullColorNote)).toBeTruthy();

    fireEvent.click(tile(t.modes.static));
    expect(screen.getByLabelText(en.common.fields.hexValue({ label: t.color.label }))).toBeTruthy();
    expect(screen.queryByRole("slider", { name: t.speed.label })).toBeNull();
    expect(screen.queryByText(t.color.fullColorNote)).toBeNull();

    fireEvent.click(tile(t.modes.speed_respond));
    expect(screen.queryByRole("slider", { name: t.speed.label })).toBeNull();
    expect(screen.getByRole("slider", { name: t.brightness.label })).toBeTruthy();
  });

  it("changes only the local draft while editing: no IPC until Review and write", () => {
    load(fixture("light_ready"));
    fireEvent.click(tile(t.modes.wave));
    fireEvent.click(screen.getByRole("button", { name: t.color.names.green }));
    const brightness = screen.getByRole("slider", { name: t.brightness.label });
    fireEvent.change(brightness, { target: { value: "1" } });
    fireEvent.change(brightness, { target: { value: "2" } });
    const speed = screen.getByRole("slider", { name: t.speed.label });
    fireEvent.change(speed, { target: { value: "0" } });
    const hex = screen.getByLabelText(en.common.fields.hexValue({ label: t.color.label }));
    fireEvent.focus(hex);
    fireEvent.change(hex, { target: { value: "#0080FF" } });
    fireEvent.blur(hex);
    expect(calls).toEqual([]);

    const summary = summaryText();
    expect(summary).toContain(t.applyBar.notWritten);
    expect(summary).toContain(t.applyBar.effect({ from: t.modes.custom, to: t.modes.wave }));
    expect(summary).toContain("#0080ff");
    // Custom shows per-key colours, so the summary doesn't pretend the record's colour is visible.
    expect(summary).toContain(`${t.applyBar.color}: ${t.review.perKey}`);
    expect(summary).not.toContain("#ff0000");
    expect(summary).toContain(t.applyBar.brightness({ from: 4, to: 2 }));
    expect(summary).toContain(t.applyBar.speed({ from: 3, to: 0 }));
    expect(brightness.getAttribute("aria-valuetext")).toBe(t.level({ value: 2, max: 4 }));
    expect(screen.getByText(t.preview.after)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: en.common.actions.discard }));
    expect(summaryText()).toBe(t.applyBar.noChanges);
    expect(tile(t.modes.custom).getAttribute("aria-checked")).toBe("true");
    expect(calls).toEqual([]);
  });

  it("selects effects with the arrow keys (draft only)", () => {
    load(fixture("light_ready"));
    const custom = tile(t.modes.custom);
    expect(custom.tabIndex).toBe(0);
    fireEvent.keyDown(custom, { key: "ArrowRight" });
    expect(tile(t.modes.wave).getAttribute("aria-checked")).toBe("true");
    expect(document.activeElement).toBe(tile(t.modes.wave));
    fireEvent.keyDown(tile(t.modes.wave), { key: "ArrowLeft" });
    expect(tile(t.modes.custom).getAttribute("aria-checked")).toBe("true");
    expect(summaryText()).toBe(t.applyBar.noChanges);
    expect(calls).toEqual([]);
  });
});

describe("Lighting page: the write flow", () => {
  it("plans, confirms, writes and reports with Undo, then clears the draft", async () => {
    load(fixture("light_apply_ok"));
    draftStaticBlue();
    fireEvent.click(reviewButton());

    await screen.findByText(t.review.title({ n: 1 }));
    const prepare = calls.find((call) => call.command === "prepare_light_edit");
    expect(prepare?.args).toEqual({
      intent: { mode: 0, color: "#0000ff", brightness: null, speed: null, based_on: 1 },
    });
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText(t.review.modeChange)).toBeTruthy();
    expect(within(dialog).getByText(t.review.intro({ n: 1 }))).toBeTruthy();
    expect(within(dialog).getByText(t.modes.static)).toBeTruthy();
    expect(commands()).not.toContain("apply_light_edit");

    await sleep(550);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.writeToKeyboard }));
    await screen.findByText(en.flow.headline.doneLight, {}, { timeout: 3000 });
    expect(commands().filter((c) => c === "apply_light_edit")).toHaveLength(1);
    expect(screen.getByText(en.flow.headline.waited({ seconds: "2 s" }))).toBeTruthy();
    expect(screen.getByText("openwraith-W75HEARGB-20261024-073000-set-light.owbackup.json")).toBeTruthy();
    expect(screen.getByRole("button", { name: en.common.actions.undo })).toBeTruthy();

    fireEvent.click(dialogAction(en.common.actions.close));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(summaryText()).toBe(t.applyBar.noChanges);
  });

  it("prepares once for a double click", async () => {
    load(fixture("light_preview_mode_change"));
    draftStaticBlue();
    const button = reviewButton();
    fireEvent.click(button);
    fireEvent.click(button);
    await screen.findByText(t.review.title({ n: 1 }));
    await sleep(20);
    expect(commands().filter((c) => c === "prepare_light_edit")).toHaveLength(1);
  });

  it("discards the plan on Cancel and keeps the draft", async () => {
    load(fixture("light_preview_mode_change"));
    draftStaticBlue();
    fireEvent.click(reviewButton());
    await screen.findByText(t.review.title({ n: 1 }));
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.cancel }));
    await waitFor(() => {
      expect(commands()).toContain("discard_plan");
    });
    expect(commands()).not.toContain("apply_light_edit");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(summaryText()).toContain(t.applyBar.effect({ from: t.modes.custom, to: t.modes.static }));
  });

  it("shows a prepare error, says nothing was written and keeps the draft", async () => {
    const stale: AppError = {
      kind: "stale_plan",
      detail: { detail: "plan_invalidated", reason: "snapshot_changed" },
      message: "the plan is out of date",
    };
    load(fixture("light_ready"), async (command) => {
      await Promise.resolve();
      if (command === "prepare_light_edit") throw stale;
      return null;
    });
    fireEvent.change(screen.getByRole("slider", { name: t.brightness.label }), { target: { value: "2" } });
    fireEvent.click(reviewButton());
    await screen.findByText(en.errors.kind.stale_plan.title);
    expect(screen.getByText(en.flow.nothingWritten)).toBeTruthy();
    expect(commands()).not.toContain("apply_light_edit");
    fireEvent.click(dialogAction(en.common.actions.close));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(summaryText()).toContain(t.applyBar.brightness({ from: 4, to: 2 }));
  });

  it("ends with one Close when the keyboard already has the settings", async () => {
    load(fixture("light_ready"), (command) => {
      if (command !== "prepare_light_edit") return null;
      const light = FIXTURES["light_ready"]?.initial_snapshot?.light.current;
      return { token: null, profile: 1, current: light, target: light, changes_mode: false, packets: 1, nothing_to_do: true };
    });
    fireEvent.change(screen.getByRole("slider", { name: t.brightness.label }), { target: { value: "3" } });
    fireEvent.click(reviewButton());
    await screen.findByText(en.flow.headline.nothingToDo);
    expect(screen.queryByRole("button", { name: en.common.actions.writeToKeyboard })).toBeNull();
    expect(commands()).not.toContain("apply_light_edit");
  });

  it("reports a write the keyboard didn't accept and keeps the draft", async () => {
    load(fixture("light_apply_verify_failed"));
    fireEvent.change(screen.getByRole("slider", { name: t.brightness.label }), { target: { value: "1" } });
    fireEvent.click(reviewButton());
    await screen.findByText(t.review.title({ n: 1 }));
    await sleep(550);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.writeToKeyboard }));
    await screen.findByText(en.flow.results.failed({ reason: en.errors.kind.verify_failed.title }), {}, { timeout: 3000 });
    expect(screen.getAllByText(en.flow.headline.notWritten).length).toBeGreaterThan(0);
    fireEvent.click(dialogAction(en.common.actions.close));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(summaryText()).toContain(t.applyBar.brightness({ from: 4, to: 1 }));
  });
});

describe("Lighting page: states", () => {
  it("asks to connect the keyboard when it is disconnected", () => {
    load(fixture("lost"));
    expect(screen.getByText(t.empty.title)).toBeTruthy();
    expect(screen.getByText(en.shell.status.lost)).toBeTruthy();
    expect(screen.queryByRole("radiogroup")).toBeNull();
    expect(screen.queryByRole("button", { name: en.common.actions.reviewAndWrite })).toBeNull();
  });

  it("names why writing is blocked and doesn't open the flow", () => {
    const source = fixture("light_ready");
    const blocked: DeviceStatus = {
      ...source.initial_status,
      write_gate: { allowed: false, reasons: ["safety_dir_unavailable"] },
    };
    source.initial_status = blocked;
    load(source);
    fireEvent.change(screen.getByRole("slider", { name: t.brightness.label }), { target: { value: "2" } });
    expect(screen.getByText(en.errors.writeBlocked.safety_dir_unavailable)).toBeTruthy();
    const button = reviewButton();
    expect(button).toHaveProperty("disabled", true);
    fireEvent.click(button);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls).toEqual([]);
  });

  it("is read-only while the values are stale", () => {
    const source = fixture("light_ready");
    source.initial_status = {
      ...source.initial_status,
      snapshot_stale: true,
      write_gate: { allowed: false, reasons: ["snapshot_stale"] },
    };
    load(source);
    expect(tile(t.modes.wave)).toHaveProperty("disabled", true);
    expect(screen.getByRole("slider", { name: t.brightness.label })).toHaveProperty("disabled", true);
  });

  it("asks for an effect when the keyboard's effect is unknown", () => {
    load(fixture("light_unknown_mode"));
    const group = screen.getByRole("radiogroup", { name: t.effects.label });
    const unknown = within(group).getAllByRole("radio")[0];
    expect(unknown?.textContent).toContain(t.effects.unknown({ id: "0x05" }));
    expect(unknown).toHaveProperty("disabled", true);
    expect(screen.getByText(t.effects.unknownHint)).toBeTruthy();
    expect(screen.getByRole("slider", { name: t.brightness.label })).toHaveProperty("disabled", true);

    fireEvent.click(tile(t.modes.static));
    expect(screen.queryByText(t.effects.unknownHint)).toBeNull();
    expect(screen.getByRole("slider", { name: t.brightness.label })).toHaveProperty("disabled", false);
    expect(summaryText()).toContain(t.applyBar.effect({ from: t.effects.unknown({ id: "0x05" }), to: t.modes.static }));
    expect(reviewButton()).toHaveProperty("disabled", false);
  });

  it("warns when the keyboard reports its lighting as off", () => {
    load(fixture("light_power_off"));
    expect(screen.getByText(t.powerOff.title)).toBeTruthy();
  });

  it("explains why the lighting can't be changed", () => {
    const source = fixture("light_ready");
    const snapshot = source.initial_snapshot;
    if (snapshot === null) throw new Error("no snapshot");
    snapshot.light = { ...snapshot.light, editable: false, disabled: "limits_unknown" };
    load(source);
    expect(screen.getByText(t.disabled.reasons.limits_unknown)).toBeTruthy();
    expect(tile(t.modes.wave)).toHaveProperty("disabled", true);
  });

  it("discards the draft with a notice when the keyboard is on another profile", async () => {
    const source = fixture("light_ready");
    load(source);
    fireEvent.change(screen.getByRole("slider", { name: t.brightness.label }), { target: { value: "2" } });
    expect(summaryText()).toContain(t.applyBar.brightness({ from: 4, to: 2 }));
    const snapshot = source.initial_snapshot;
    if (snapshot === null) throw new Error("no snapshot");
    const next: DeviceSnapshot = { ...snapshot, revision: snapshot.revision + 1, profile: 2 };
    act(() => {
      snapshotStore.set({ snapshot: next, loading: false, error: null });
    });
    await screen.findByText(t.discarded.title);
    expect(summaryText()).toBe(t.applyBar.noChanges);
    expect(screen.getByText(t.perProfile.body({ n: 2 }))).toBeTruthy();
  });

  it("keeps the draft when the same profile is read again", () => {
    const source = fixture("light_ready");
    load(source);
    fireEvent.change(screen.getByRole("slider", { name: t.brightness.label }), { target: { value: "2" } });
    const snapshot = source.initial_snapshot;
    if (snapshot === null) throw new Error("no snapshot");
    act(() => {
      snapshotStore.set({ snapshot: { ...snapshot, revision: snapshot.revision + 1 }, loading: false, error: null });
    });
    expect(summaryText()).toContain(t.applyBar.brightness({ from: 4, to: 2 }));
    expect(screen.queryByText(t.discarded.title)).toBeNull();
  });

  it("speaks Turkish", () => {
    setLang("tr");
    load(fixture("light_ready"));
    const tr = DICTIONARIES.tr.lighting;
    expect(screen.getByText(tr.perProfile.body({ n: 1 })).textContent).toContain("1 numaralı profili");
    expect(screen.getByRole("radiogroup", { name: tr.effects.label })).toBeTruthy();
    expect(screen.getByText(tr.modes.static)).toBeTruthy();
    expect(screen.getByRole("slider", { name: tr.brightness.label }).getAttribute("aria-valuetext")).toBe("4 üzerinden 4");
  });
});

describe("Lighting page: a lost connection during the write", () => {
  it("keeps the flow and its report, and never prepares again on reconnect", async () => {
    const source = fixture("light_apply_ok");
    const prepareCall = source.calls.find((c) => c.command === "prepare_light_edit");
    const applyCall = source.calls.find((c) => c.command === "apply_light_edit");
    if (prepareCall?.command !== "prepare_light_edit" || prepareCall.ok === null) throw new Error("no preview");
    if (applyCall?.command !== "apply_light_edit" || applyCall.ok === null) throw new Error("no report");
    const preview: LightEditPreview = prepareCall.ok;
    const report: EditReportView = { ...applyCall.ok, headline: "maybe_lost" };
    let finishApply: (value: EditReportView) => void = () => undefined;
    load(source, (command) => {
      switch (command) {
        case "prepare_light_edit":
          return preview;
        case "apply_light_edit":
          return new Promise<EditReportView>((resolve) => {
            finishApply = resolve;
          });
        case "get_snapshot":
          return source.initial_snapshot;
        default:
          return null;
      }
    });
    startSnapshotSync();
    draftStaticBlue();
    fireEvent.click(reviewButton());
    await screen.findByText(t.review.title({ n: 1 }));
    await sleep(550);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.writeToKeyboard }));
    await waitFor(() => {
      expect(commands()).toContain("apply_light_edit");
    });
    // The keyboard is unplugged during the save wait: the status clears the snapshot.
    act(() => {
      statusStore.set({
        ...source.initial_status,
        revision: source.initial_status.revision + 10,
        connection: { state: "lost", error: { kind: "disconnected", detail: null, message: "unplugged" } },
        snapshot_revision: null,
        write_gate: { allowed: false, reasons: ["not_connected"] },
      });
    });
    expect(snapshotStore.get().snapshot).toBeNull();
    expect(screen.getByRole("dialog")).toBeTruthy();
    await act(async () => {
      finishApply(report);
      await Promise.resolve();
    });
    await screen.findByRole("button", { name: en.common.actions.undo });
    expect(screen.getByRole("dialog")).toBeTruthy();
    // Connected again: a new snapshot arrives; no second plan is made.
    act(() => {
      statusStore.set({ ...source.initial_status, revision: source.initial_status.revision + 20 });
    });
    await sleep(20);
    expect(commands().filter((c) => c === "prepare_light_edit")).toHaveLength(1);
    expect(screen.getByRole("button", { name: en.common.actions.undo })).toBeTruthy();
  });
});

describe("Lighting page: a draft the chosen effect doesn't use", () => {
  it("can be discarded, and is no longer registered", () => {
    load(fixture("light_ready"));
    fireEvent.click(tile(t.modes.wave));
    fireEvent.click(screen.getByRole("button", { name: t.color.names.green }));
    fireEvent.click(tile(t.modes.custom));
    expect(summaryText()).toBe(t.applyBar.noChanges);
    expect(draftPages()).toContain("lighting");
    const discard = screen.getByRole("button", { name: en.common.actions.discard });
    expect(discard).toHaveProperty("disabled", false);
    fireEvent.click(discard);
    expect(draftPages()).not.toContain("lighting");
    expect(calls).toEqual([]);
  });
});
