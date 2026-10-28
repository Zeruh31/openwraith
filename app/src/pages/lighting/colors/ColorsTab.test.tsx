// Lighting > Key colours on mocked IPC: the real `api` talks to the browser
// preview's mock backend, which answers with the fixtures the core recorded
// from the real worker on the simulator. Covers: the board and its spoken
// colours, the tools (select + fill, brush, eyedropper, fill all, gradient,
// the keyboard path), undo/redo, no IPC while painting, draft → review →
// write → report (and the colours read again), the opt-in switch to Custom,
// cancel, refusals, errors, the disconnected / disabled / blocked states,
// slots kept as read, the per-profile draft and Turkish.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ToastRegion } from "../../../components/Toasts";
import { DICTIONARIES, setLang } from "../../../i18n";
import type { AppError, ColorEditPreview, ColorsState, DeviceSnapshot, EditReportView } from "../../../ipc/bindings";
import { createMockBackend, type MockBackend } from "../../../preview/mockBackend";
import { FIXTURES } from "../../../preview/registry";
import type { Fixture } from "../../../preview/types";
import { appInfoStore, DEFAULT_APP_INFO } from "../../../state/appInfo";
import { draftEntries, draftPages } from "../../../state/drafts";
import { acknowledgeVendor, resetSession } from "../../../state/session";
import { snapshotStore } from "../../../state/snapshot";
import { INITIAL_STATUS, statusStore } from "../../../state/status";
import { clearToasts } from "../../../state/toasts";
import { resetDraftStore } from "../draftStore";
import ColorsTab from "./index";
import { resetColorsStores } from "./store";

const en = DICTIONARIES.en;
const t = en.lighting.colors;

const W = 46;
const E = 47;
const A = 68;
const S = 69;
const D = 70;

interface Call {
  command: string;
  args: unknown;
}

let calls: Call[] = [];

beforeEach(() => {
  setLang("en");
  resetColorsStores();
  resetDraftStore();
  resetSession();
  acknowledgeVendor();
  clearToasts();
  calls = [];
  window.location.hash = "#/lighting?tab=colors";
});

afterEach(() => {
  clearMocks();
  statusStore.set(INITIAL_STATUS);
  snapshotStore.set({ snapshot: null, loading: false, error: null });
  appInfoStore.set(DEFAULT_APP_INFO);
  resetColorsStores();
  clearToasts();
  setLang("en");
});

function fixture(id: string): Fixture {
  const found = FIXTURES[id];
  if (found === undefined) throw new Error(`no fixture ${id}`);
  return structuredClone(found);
}

/** The first recorded `get_colors_state` result. */
function recordedState(source: Fixture): ColorsState {
  const found = source.calls.find((call) => call.command === "get_colors_state");
  if (found?.command !== "get_colors_state" || found.ok === null) throw new Error("no recorded colours");
  return structuredClone(found.ok);
}

/** The first recorded `prepare_color_edit` result. */
function recordedPreview(source: Fixture): ColorEditPreview {
  const found = source.calls.find((call) => call.command === "prepare_color_edit");
  if (found?.command !== "prepare_color_edit" || found.ok === null) throw new Error("no recorded preview");
  return structuredClone(found.ok);
}

/** The first recorded `apply_color_edit` result. */
function recordedReport(source: Fixture): EditReportView {
  const found = source.calls.find((call) => call.command === "apply_color_edit");
  if (found?.command !== "apply_color_edit" || found.ok === null) throw new Error("no recorded report");
  return structuredClone(found.ok);
}

type Handler = (command: string, args: unknown, backend: MockBackend) => unknown;

/** Answers IPC from the fixture's recording (or `handler`), then loads its state into the stores. */
function load(source: Fixture, handler?: Handler) {
  const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
  mockIPC((command, args) => {
    calls.push({ command, args });
    return handler === undefined ? backend.handle(command, args) : handler(command, args, backend);
  });
  appInfoStore.set(source.app_info);
  snapshotStore.set({ snapshot: source.initial_snapshot, loading: false, error: null });
  statusStore.set(source.initial_status);
  return render(
    <>
      <ColorsTab />
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

async function board(): Promise<HTMLElement> {
  return screen.findByRole("listbox", { name: t.board.label });
}

function keyElement(index: number): HTMLElement {
  const element = document.querySelector<HTMLElement>(`.ow-keyboard[role="listbox"] [data-key-index="${index}"]`);
  if (element === null) throw new Error(`no key ${index}`);
  return element;
}

function clickKey(index: number): void {
  const element = keyElement(index);
  fireEvent.pointerDown(element, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
  fireEvent.pointerUp(element, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
}

/** What a screen reader says after a key's name: its description. */
function described(index: number): string {
  const id = keyElement(index).getAttribute("aria-describedby");
  return id === null ? "" : (document.getElementById(id)?.textContent ?? "");
}

function summaryText(): string {
  return document.querySelector(".ow-applybar__summary")?.textContent ?? "";
}

function liveText(): string {
  return document.querySelector(".li-colors > [role='status']")?.textContent ?? "";
}

function button(name: string): HTMLElement {
  return screen.getByRole("button", { name });
}

function reviewButton(): HTMLElement {
  return button(en.common.actions.reviewAndWrite);
}

function tool(name: string): HTMLElement {
  return within(screen.getByRole("radiogroup", { name: t.tools.label })).getByRole("radio", { name });
}

function swatch(name: string): HTMLElement {
  return screen.getByRole("button", { name: `${name}, ${swatchHex(name)}` });
}

function swatchHex(name: string): string {
  const names = en.lighting.color.names;
  const table: Record<string, string> = {
    [names.red]: "#ff0000",
    [names.green]: "#00ff00",
    [names.blue]: "#0000ff",
    [names.white]: "#ffffff",
  };
  return table[name] ?? "";
}

function dialogAction(name: string): HTMLElement {
  const found = screen
    .getAllByRole("button", { name })
    .find((element) => element.closest(".ow-dialog__actions") !== null);
  if (found === undefined) throw new Error(`no dialog action ${name}`);
  return found;
}

/** W, A, S, D in blue: the intent the `colors_apply_ok` fixture recorded. */
async function draftWasdBlue(): Promise<void> {
  await board();
  for (const key of [W, A, S, D]) clickKey(key);
  fireEvent.click(swatch(en.lighting.color.names.blue));
  fireEvent.click(button(t.paint.fillSelection));
}

/** W and A in green: the intent the `colors_preview_*` fixtures recorded. */
async function draftWaGreen(): Promise<void> {
  await board();
  clickKey(W);
  clickKey(A);
  fireEvent.click(swatch(en.lighting.color.names.green));
  fireEvent.click(button(t.paint.fillSelection));
}

describe("Key colours: the board", () => {
  it("shows each key's colour and says it for screen readers", async () => {
    load(fixture("colors_ready"));
    await board();
    expect(described(W)).toBe("red, #FF0000");
    expect(keyElement(W).querySelector<HTMLElement>(".ow-key__swatch")?.style.backgroundColor).toBe("rgb(255, 0, 0)");
    expect(keyElement(15).getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText(t.perProfile.body({ n: 1 }))).toBeTruthy();
    // The keyboard shows the table (Custom): no "not shown" callout.
    expect(screen.queryByText(t.notShown.title)).toBeNull();
    expect(summaryText()).toBe(en.lighting.applyBar.noChanges);
    expect(commands()).toEqual(["get_colors_state"]);
  });

  it("fills the selection, undoes and redoes it, all without IPC", async () => {
    load(fixture("colors_ready"));
    await board();
    for (const key of [W, A, S, D]) clickKey(key);
    expect(screen.getByText(`${t.board.selected({ n: 4 })} · ${t.board.selectedColor({ color: "red, #FF0000" })}`)).toBeTruthy();
    fireEvent.click(swatch(en.lighting.color.names.blue));
    fireEvent.click(button(t.paint.fillSelection));

    expect(summaryText()).toContain(en.lighting.applyBar.notWritten);
    expect(summaryText()).toContain(t.applyBar.changes({ n: 4 }));
    expect(described(W)).toBe(t.board.changed({ now: "red, #FF0000", next: "blue, #0000FF" }));
    expect(liveText()).toBe(t.announce.filled({ n: 4, color: "blue, #0000FF" }));
    expect(draftEntries()).toContainEqual({ page: "lighting", tab: "colors", count: 4 });

    fireEvent.click(button(t.tools.undo));
    expect(summaryText()).toBe(en.lighting.applyBar.noChanges);
    expect(liveText()).toBe(t.announce.undone({ n: 0 }));
    expect(draftPages()).not.toContain("lighting");
    fireEvent.click(button(t.tools.redo));
    expect(summaryText()).toContain(t.applyBar.changes({ n: 4 }));

    // Ctrl + Z / Ctrl + Y on the board.
    fireEvent.keyDown(keyElement(W), { key: "z", ctrlKey: true });
    expect(summaryText()).toBe(en.lighting.applyBar.noChanges);
    fireEvent.keyDown(keyElement(W), { key: "y", ctrlKey: true });
    expect(summaryText()).toContain(t.applyBar.changes({ n: 4 }));

    // Discard is one more undoable step.
    fireEvent.click(button(en.common.actions.discard));
    expect(summaryText()).toBe(en.lighting.applyBar.noChanges);
    fireEvent.click(button(t.tools.undo));
    expect(summaryText()).toContain(t.applyBar.changes({ n: 4 }));

    expect(commands()).toEqual(["get_colors_state"]);
  });

  it("paints with the brush and takes a colour with the eyedropper", async () => {
    load(fixture("colors_ready"));
    await board();
    fireEvent.click(swatch(en.lighting.color.names.green));
    fireEvent.click(tool(t.tools.names.brush));
    clickKey(W);
    expect(described(W)).toBe(t.board.changed({ now: "red, #FF0000", next: "green, #00FF00" }));
    expect(liveText()).toBe(t.announce.painted({ key: "W", color: "green, #00FF00" }));
    clickKey(W);
    expect(liveText()).toBe(t.announce.unchanged({ key: "W" }));

    fireEvent.click(tool(t.tools.names.eyedropper));
    clickKey(A);
    expect(liveText()).toBe(t.announce.picked({ key: "A", color: "red, #FF0000" }));
    // Back to the brush, now painting red: W goes back to the keyboard's colour.
    expect(tool(t.tools.names.brush).getAttribute("aria-checked")).toBe("true");
    clickKey(W);
    expect(described(W)).toBe("red, #FF0000");
    expect(summaryText()).toBe(en.lighting.applyBar.noChanges);
    expect(commands()).toEqual(["get_colors_state"]);
  });

  it("has a keyboard path: arrows and Space select, Fill paints; Space paints with the brush", async () => {
    load(fixture("colors_ready"));
    await board();
    fireEvent.focus(keyElement(W));
    fireEvent.keyDown(keyElement(W), { key: " " });
    fireEvent.keyDown(keyElement(W), { key: "ArrowRight" });
    fireEvent.keyDown(keyElement(E), { key: " " });
    expect(keyElement(E).getAttribute("aria-selected")).toBe("true");
    fireEvent.click(swatch(en.lighting.color.names.white));
    fireEvent.click(button(t.paint.fillSelection));
    expect(summaryText()).toContain(t.applyBar.changes({ n: 2 }));

    fireEvent.click(tool(t.tools.names.brush));
    fireEvent.focus(keyElement(A));
    fireEvent.keyDown(keyElement(A), { key: " " });
    expect(summaryText()).toContain(t.applyBar.changes({ n: 3 }));
  });

  it("fills every key, and paints a gradient over the selection", async () => {
    load(fixture("colors_ready"));
    await board();
    expect(button(t.paint.fillSelection).hasAttribute("disabled")).toBe(true);
    expect(screen.getByText(t.paint.needSelection)).toBeTruthy();
    fireEvent.click(button(t.paint.fillAll));
    // White is the start colour: every key changes.
    expect(summaryText()).toContain(t.applyBar.changes({ n: 81 }));

    fireEvent.click(button(t.tools.undo));
    fireEvent.click(swatch(en.lighting.color.names.red));
    fireEvent.click(button(t.paint.fillAll));
    expect(liveText()).toBe(t.announce.nothingChanged);

    clickKey(W);
    expect(screen.getByText(t.gradient.needTwo)).toBeTruthy();
    fireEvent.click(button(t.board.selectAll));
    expect(screen.getByText(new RegExp(t.board.selected({ n: 81 })))).toBeTruthy();
    fireEvent.click(swatch(en.lighting.color.names.white));
    fireEvent.click(button(t.gradient.fill));
    expect(summaryText()).toContain(t.applyBar.changes({ n: 81 }));
    // Esc is at the left (the start colour), Right at the right (the gradient colour, blue).
    expect(described(0)).toBe(t.board.changed({ now: "red, #FF0000", next: "white, #FFFFFF" }));
    expect(described(125)).toBe(t.board.changed({ now: "red, #FF0000", next: "blue, #0000FF" }));
    fireEvent.click(button(t.board.clearSelection));
    expect(screen.getByText(t.board.selected({ n: 0 }))).toBeTruthy();
    expect(commands()).toEqual(["get_colors_state"]);
  });
});

describe("Key colours: the write flow", () => {
  it("plans, confirms, writes and reports with Undo, then reads the colours again", async () => {
    const source = fixture("colors_apply_ok");
    load(source);
    await draftWasdBlue();
    fireEvent.click(reviewButton());

    await screen.findByText(t.review.title({ n: 1 }));
    const prepare = calls.find((call) => call.command === "prepare_color_edit");
    expect(prepare?.args).toEqual({
      intent: {
        based_on: 1,
        check_persistence: false,
        colors: [
          { color: "#0000ff", key: W },
          { color: "#0000ff", key: A },
          { color: "#0000ff", key: S },
          { color: "#0000ff", key: D },
        ],
        switch_to_custom: false,
      },
    });
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText(t.review.intro({ n: 1 }))).toBeTruthy();
    expect(within(dialog).getByRole("img", { name: `${t.review.before}: red, #FF0000 on 81 keys` })).toBeTruthy();
    expect(
      within(dialog).getByRole("img", {
        name: `${t.review.after}: red, #FF0000 on 77 keys; blue, #0000FF on 4 keys`,
      }),
    ).toBeTruthy();
    expect(within(dialog).getByRole("rowheader", { name: "W, A, S, D" })).toBeTruthy();
    // "Check that the keyboard kept it" is offered, not pre-ticked.
    const persistence = within(dialog).getByRole("checkbox", { name: t.review.persistence.check({ m: 2 }) });
    expect(persistence).toHaveProperty("checked", false);
    expect(commands()).not.toContain("apply_color_edit");

    await sleep(550);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.writeToKeyboard }));
    await screen.findByText("openwraith-W75HEARGB-20261024-073000-set-colors.owbackup.json", {}, { timeout: 3000 });
    expect(commands().filter((c) => c === "apply_color_edit")).toHaveLength(1);
    expect(screen.getByRole("button", { name: en.common.actions.undo })).toBeTruthy();

    fireEvent.click(dialogAction(en.common.actions.close));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    // The draft is spent and the colours were read again (W is blue now).
    await waitFor(() => {
      expect(described(W)).toBe("blue, #0000FF");
    });
    expect(commands().filter((c) => c === "get_colors_state")).toHaveLength(2);
    expect(summaryText()).toBe(en.lighting.applyBar.noChanges);
    expect(draftPages()).not.toContain("lighting");
  });

  it("prepares once for a double click", async () => {
    load(fixture("colors_apply_ok"));
    await draftWasdBlue();
    const review = reviewButton();
    fireEvent.click(review);
    fireEvent.click(review);
    await screen.findByText(t.review.title({ n: 1 }));
    await sleep(20);
    expect(commands().filter((c) => c === "prepare_color_edit")).toHaveLength(1);
  });

  it("asks a new plan for the persistence check", async () => {
    load(fixture("colors_apply_ok"));
    await draftWasdBlue();
    fireEvent.click(reviewButton());
    await screen.findByText(t.review.title({ n: 1 }));
    fireEvent.click(screen.getByRole("checkbox", { name: t.review.persistence.check({ m: 2 }) }));
    await waitFor(() => {
      expect(commands().filter((c) => c === "prepare_color_edit")).toHaveLength(2);
    });
    const second = calls.filter((call) => call.command === "prepare_color_edit")[1];
    expect(second?.args).toMatchObject({ intent: { check_persistence: true } });
    expect(commands()).toContain("discard_plan");
  });

  it("discards the plan on Cancel and keeps the draft", async () => {
    load(fixture("colors_preview_not_shown"));
    await draftWaGreen();
    fireEvent.click(reviewButton());
    await screen.findByText(t.review.title({ n: 1 }));
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.cancel }));
    await waitFor(() => {
      expect(commands()).toContain("discard_plan");
    });
    expect(commands()).not.toContain("apply_color_edit");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(summaryText()).toContain(t.applyBar.changes({ n: 2 }));
  });

  it("explains a refusal, says nothing was written and keeps the draft", async () => {
    const refusal: AppError = {
      kind: "invalid_request",
      detail: { detail: "edit", problem: { problem: "knob" } },
      message: "the knob has no colour of its own",
    };
    load(fixture("colors_ready"), async (command, args, backend) => {
      if (command === "prepare_color_edit") {
        await Promise.resolve();
        throw refusal;
      }
      return backend.handle(command, args);
    });
    await draftWasdBlue();
    fireEvent.click(reviewButton());
    await screen.findByText(t.problems.knob);
    expect(screen.getByText(en.flow.nothingWritten)).toBeTruthy();
    expect(screen.getByText(en.errors.kind.invalid_request.title)).toBeTruthy();
    fireEvent.click(dialogAction(en.common.actions.close));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(summaryText()).toContain(t.applyBar.changes({ n: 4 }));
    expect(commands()).not.toContain("apply_color_edit");
  });

  it("reads the colours again when the plan was out of date", async () => {
    const stale: AppError = {
      kind: "stale_plan",
      detail: { detail: "plan_invalidated", reason: "snapshot_changed" },
      message: "the colours changed",
    };
    load(fixture("colors_ready"), async (command, args, backend) => {
      if (command === "prepare_color_edit") {
        await Promise.resolve();
        throw stale;
      }
      return backend.handle(command, args);
    });
    await draftWasdBlue();
    fireEvent.click(reviewButton());
    await screen.findByText(en.errors.kind.stale_plan.title);
    await waitFor(() => {
      expect(commands().filter((c) => c === "get_colors_state")).toHaveLength(2);
    });
  });

  it("keeps the draft when the write fails", async () => {
    const source = fixture("colors_apply_ok");
    const failure: AppError = { kind: "disconnected", detail: null, message: "unplugged" };
    load(source, async (command, args, backend) => {
      if (command === "apply_color_edit") {
        await Promise.resolve();
        throw failure;
      }
      return backend.handle(command, args);
    });
    await draftWasdBlue();
    fireEvent.click(reviewButton());
    await screen.findByText(t.review.title({ n: 1 }));
    await sleep(550);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.writeToKeyboard }));
    await screen.findByText(en.errors.kind.disconnected.title);
    fireEvent.click(dialogAction(en.common.actions.close));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(summaryText()).toContain(t.applyBar.changes({ n: 4 }));
  });

  it("spends the draft when its write ends after the tab went away", async () => {
    const source = fixture("colors_apply_ok");
    const report = recordedReport(source);
    let finish: (value: EditReportView) => void = () => undefined;
    const page = load(source, (command, args, backend) =>
      command === "apply_color_edit"
        ? new Promise<EditReportView>((resolve) => {
            finish = resolve;
          })
        : backend.handle(command, args),
    );
    await draftWasdBlue();
    fireEvent.click(reviewButton());
    await screen.findByText(t.review.title({ n: 1 }));
    await sleep(550);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.writeToKeyboard }));
    await waitFor(() => {
      expect(commands()).toContain("apply_color_edit");
    });
    page.unmount();
    expect(draftPages()).toContain("lighting");
    await act(async () => {
      finish(report);
      await Promise.resolve();
    });
    await waitFor(() => {
      expect(draftPages()).not.toContain("lighting");
    });
  });
});

describe("Key colours: the effect isn't Custom", () => {
  it("says so, and the switch to Custom is opt-in and not pre-ticked", async () => {
    load(fixture("colors_preview_not_shown"));
    await board();
    expect(screen.getByText(t.notShown.title)).toBeTruthy();
    expect(screen.getByText(t.notShown.body({ effect: en.lighting.modes.wave }))).toBeTruthy();
    const option = screen.getByRole("checkbox", { name: t.notShown.switch });
    expect(option).toHaveProperty("checked", false);

    await draftWaGreen();
    fireEvent.click(reviewButton());
    await screen.findByText(t.review.title({ n: 1 }));
    const prepare = calls.find((call) => call.command === "prepare_color_edit");
    expect(prepare?.args).toEqual({
      intent: {
        based_on: 1,
        check_persistence: false,
        colors: [
          { color: "#00ff00", key: W },
          { color: "#00ff00", key: A },
        ],
        switch_to_custom: false,
      },
    });
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText(t.review.notShownBody)).toBeTruthy();
    expect(within(dialog).getByRole("checkbox", { name: t.review.switchOption })).toHaveProperty("checked", false);
  });

  it("asks a new plan with the switch when it is ticked in the review", async () => {
    const switched = recordedPreview(fixture("colors_preview_switch"));
    load(fixture("colors_preview_not_shown"), (command, args, backend) => {
      if (command === "prepare_color_edit") {
        const intent = (args as { intent: { switch_to_custom: boolean } }).intent;
        if (intent.switch_to_custom) return switched;
      }
      return backend.handle(command, args);
    });
    await draftWaGreen();
    fireEvent.click(reviewButton());
    await screen.findByText(t.review.title({ n: 1 }));
    fireEvent.click(screen.getByRole("checkbox", { name: t.review.switchOption }));
    await screen.findByText(t.review.switchesToCustom({ effect: en.lighting.modes.wave }));
    const prepares = calls.filter((call) => call.command === "prepare_color_edit");
    expect(prepares).toHaveLength(2);
    expect(prepares[1]?.args).toMatchObject({ intent: { switch_to_custom: true } });
    expect(commands()).toContain("discard_plan");
    expect(screen.queryByText(t.review.notShownBody)).toBeNull();
    // The tab remembers the choice.
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.cancel }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(screen.getByRole("checkbox", { name: t.notShown.switch })).toHaveProperty("checked", true);
    expect(summaryText()).toContain(t.applyBar.switches);
  });

  it("sends the switch when it was ticked on the tab", async () => {
    load(fixture("colors_preview_switch"));
    await board();
    fireEvent.click(screen.getByRole("checkbox", { name: t.notShown.switch }));
    await draftWaGreen();
    fireEvent.click(reviewButton());
    await screen.findByText(t.review.switchesToCustom({ effect: en.lighting.modes.wave }));
    expect(calls.find((call) => call.command === "prepare_color_edit")?.args).toMatchObject({
      intent: { switch_to_custom: true },
    });
  });
});

describe("Key colours: states", () => {
  it("asks to connect the keyboard when it is disconnected", () => {
    load(fixture("lost"));
    expect(screen.getByText(t.empty.title)).toBeTruthy();
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(commands()).not.toContain("get_colors_state");
  });

  it("explains why the colours can't be changed and keeps the board read-only", async () => {
    const source = fixture("colors_ready");
    const state = recordedState(source);
    load(source, (command, args, backend) =>
      command === "get_colors_state"
        ? { ...state, editable: false, disabled: { reason: "unreadable", message: "table 1: bad length" } }
        : backend.handle(command, args),
    );
    await board();
    expect(screen.getByText(t.disabled.unreadable)).toBeTruthy();
    clickKey(W);
    expect(keyElement(W).getAttribute("aria-selected")).toBe("false");
    expect(button(t.paint.fillAll).hasAttribute("disabled")).toBe(true);
  });

  it("names why writing is blocked and doesn't open the flow", async () => {
    const source = fixture("colors_ready");
    source.initial_status = {
      ...source.initial_status,
      write_gate: { allowed: false, reasons: ["safety_dir_unavailable"] },
    };
    load(source);
    await draftWasdBlue();
    expect(screen.getByText(en.errors.writeBlocked.safety_dir_unavailable)).toBeTruthy();
    const review = reviewButton();
    expect(review).toHaveProperty("disabled", true);
    fireEvent.click(review);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(commands()).toEqual(["get_colors_state"]);
  });

  it("is read-only while the values are stale", async () => {
    const source = fixture("colors_ready");
    source.initial_status = {
      ...source.initial_status,
      snapshot_stale: true,
      write_gate: { allowed: false, reasons: ["snapshot_stale"] },
    };
    load(source);
    await board();
    expect(tool(t.tools.names.brush)).toHaveProperty("disabled", true);
    expect(button(t.paint.fillAll)).toHaveProperty("disabled", true);
  });

  it("lists slots outside the layout as kept as read", async () => {
    const source = fixture("colors_ready");
    const state = recordedState(source);
    load(source, (command, args, backend) =>
      command === "get_colors_state"
        ? {
            ...state,
            kept: [
              { slot: 15, color: "#00ff00" },
              { slot: 103, color: "#ffffff" },
            ],
          }
        : backend.handle(command, args),
    );
    await board();
    expect(screen.getByText(t.kept.title)).toBeTruthy();
    expect(screen.getByText(en.common.badge.kept)).toBeTruthy();
    expect(screen.getByText(t.kept.knob({ color: "green, #00FF00" }))).toBeTruthy();
    expect(screen.getByText(t.kept.slot({ slot: 103, color: "white, #FFFFFF" }))).toBeTruthy();
  });

  it("discards the draft with a notice when the keyboard is on another profile", async () => {
    const source = fixture("colors_ready");
    const page = load(source);
    await draftWasdBlue();
    page.unmount();
    expect(draftEntries()).toContainEqual({ page: "lighting", tab: "colors", count: 4 });
    const snapshot = source.initial_snapshot;
    if (snapshot === null) throw new Error("no snapshot");
    const next: DeviceSnapshot = { ...snapshot, revision: snapshot.revision + 1, profile: 2 };
    act(() => {
      snapshotStore.set({ snapshot: next, loading: false, error: null });
    });
    expect(draftPages()).not.toContain("lighting");
    render(<ToastRegion />);
    expect(screen.getByText(t.discarded.title)).toBeTruthy();
  });

  it("speaks Turkish", async () => {
    setLang("tr");
    load(fixture("colors_preview_not_shown"));
    const tr = DICTIONARIES.tr.lighting;
    await screen.findByRole("listbox", { name: tr.colors.board.label });
    expect(described(W)).toBe("kırmızı, #FF0000");
    expect(screen.getByText(tr.colors.notShown.body({ effect: tr.modes.wave }))).toBeTruthy();
    expect(screen.getByRole("radio", { name: tr.colors.tools.names.eyedropper })).toBeTruthy();
  });
});
