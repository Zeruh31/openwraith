// The SOCD tab on the mocked IPC: the preview's mock backend answers with the
// fixtures the core's tests recorded from the real worker on the simulator
// (socd_*), through the real `api`, `invoke` and `Channel`.
//
// Covered: the keyboard-wide power switch and its note, the game-rules
// warning, the pairs with the library's evidence per mode, quick pairs, the
// site's rules, draft → review → write → report (with the state read back),
// "Not yet verified" in the plan, the keyboard's own toggle (view updated,
// pending plan invalidated), cancel, pairs kept as read (replacing them
// needs a confirmation), blocked states.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ToastRegion } from "../../../components/Toasts";
import { DICTIONARIES, setLang } from "../../../i18n";
import type { AppError, DeviceStatus, SocdEditIntent, SocdStateView } from "../../../ipc/bindings";
import { createMockBackend } from "../../../preview/mockBackend";
import { FIXTURES } from "../../../preview/registry";
import type { Fixture } from "../../../preview/types";
import { appInfoStore, DEFAULT_APP_INFO } from "../../../state/appInfo";
import { acknowledgeVendor, resetSession } from "../../../state/session";
import { snapshotStore } from "../../../state/snapshot";
import { emitNotice, INITIAL_STATUS, statusStore } from "../../../state/status";
import { clearToasts } from "../../../state/toasts";
import SocdTab from "./index";
import { socdDrafts, socdPart } from "./store";

const en = DICTIONARIES.en;
const t = en.actuation.socd;

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

function install(
  id: string,
  patch: {
    status?: Partial<DeviceStatus>;
    state?: (state: SocdStateView) => SocdStateView;
    /** Answers that replace the backend's for a command (called with the call's number, from 1). */
    answers?: Record<string, (n: number) => unknown>;
  } = {},
): Fixture {
  const source = fixture(id);
  if (patch.state !== undefined) {
    for (const call of source.calls) {
      if (call.command === "get_socd_state" && call.ok !== null) call.ok = patch.state(call.ok);
    }
  }
  const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
  mockIPC((command, args) => {
    calls.push({ command, args: (args ?? {}) as Record<string, unknown> });
    const answer = patch.answers?.[command];
    if (answer !== undefined) return answer(calls.filter((call) => call.command === command).length);
    return backend.handle(command, args);
  });
  statusStore.set({ ...source.initial_status, ...patch.status });
  snapshotStore.set({ snapshot: structuredClone(source.initial_snapshot), loading: false, error: null });
  appInfoStore.set(source.app_info);
  return source;
}

function renderTab() {
  return render(
    <>
      <SocdTab />
      <ToastRegion />
    </>,
  );
}

function commands(): string[] {
  return calls.map((call) => call.command);
}

function button(name: string): HTMLElement {
  return screen.getByRole("button", { name });
}

function applyButton(): HTMLElement {
  return button(en.common.actions.reviewAndWrite);
}

function powerSwitch(): HTMLElement {
  return screen.getByRole("switch", { name: t.power.label });
}

function pair(n: number): HTMLElement {
  return screen.getByRole("group", { name: t.pairs.pairN({ n }) });
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

function recordedIntent(source: Fixture): SocdEditIntent {
  const call = source.calls.find((c) => c.command === "prepare_socd_edit");
  if (call?.command !== "prepare_socd_edit") throw new Error(`no prepare in ${source.id}`);
  return call.args.intent;
}

function preparedIntents(): SocdEditIntent[] {
  return calls.filter((c) => c.command === "prepare_socd_edit").map((c) => c.args["intent"] as SocdEditIntent);
}

/** Picks a key in a pair's key picker by typing and clicking the matching option. */
function pickKey(row: HTMLElement, label: string, name: string): void {
  const input = within(row).getByRole("combobox", { name: label });
  fireEvent.focus(input);
  fireEvent.change(input, { target: { value: name } });
  fireEvent.click(within(row).getByRole("option", { name }));
}

function setMode(row: HTMLElement, mode: number): void {
  fireEvent.change(within(row).getByRole("combobox", { name: t.pairs.mode }), { target: { value: String(mode) } });
}

async function ready(): Promise<void> {
  await screen.findByRole("switch", { name: t.power.label });
}

beforeEach(() => {
  setLang("en");
  socdPart.reset();
  socdDrafts.reset();
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

describe("the SOCD tab", () => {
  it("labels the power switch as a setting of the whole keyboard, with the hotkey and the game-rules warning", async () => {
    install("socd_ready");
    renderTab();
    await ready();
    expect(powerSwitch().getAttribute("aria-checked")).toBe("false");
    expect(screen.getByText(en.common.badge.keyboardWide)).toBeTruthy();
    expect(document.querySelector('[data-badge="keyboard_wide"]')).not.toBeNull();
    expect(screen.getByText(t.power.hotkey)).toBeTruthy();
    expect(screen.getByText(t.games.title)).toBeTruthy();
    expect(screen.getByText(t.games.body)).toBeTruthy();
    expect(screen.getByText(t.pairs.none)).toBeTruthy();
    expect(screen.getByText(t.pairs.count({ n: 0, max: 20 }))).toBeTruthy();
    expect(commands()).toEqual(["get_socd_state"]);
  });

  it("shows the pairs, marking only the modes the library marks as not yet verified", async () => {
    install("socd_ready_pairs");
    renderTab();
    await ready();
    expect(powerSwitch().getAttribute("aria-checked")).toBe("true");
    expect(within(pair(1)).getByRole("combobox", { name: t.pairs.key1 })).toHaveProperty("value", "A");
    expect(within(pair(1)).getByRole("combobox", { name: t.pairs.key2 })).toHaveProperty("value", "D");
    expect(within(pair(1)).queryByText(en.common.badge.unverified)).toBeNull();
    expect(within(pair(2)).getByText(en.common.badge.unverified)).toBeTruthy();
    expect(within(pair(3)).queryByText(en.common.badge.unverified)).toBeNull();
    // The mode menu names the modes only (the badge beside it marks the chosen one, so a
    // Turkish name isn't cut off and the marker isn't shown twice).
    const options = within(within(pair(1)).getByRole("combobox", { name: t.pairs.mode })).getAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual([
      "1 · Last input wins",
      "2 · Key 1 wins",
      "3 · Key 2 wins",
      "4 · Later key only interrupts the first",
    ]);
    expect(within(pair(2)).getAllByText(en.common.badge.unverified)).toHaveLength(1);
  });

  it("enforces the site's rules before anything is planned", async () => {
    install("socd_ready");
    renderTab();
    await ready();
    fireEvent.click(button(t.pairs.quick.ad));
    fireEvent.click(button(t.pairs.add));
    // An incomplete pair can't be reviewed.
    expect(within(pair(2)).getByText(t.problems.incomplete)).toBeTruthy();
    expect(applyButton()).toHaveProperty("disabled", true);
    expect(screen.getByText(t.apply.fixPairs)).toBeTruthy();
    // A key in another pair is offered with the reason and can't be picked.
    const input = within(pair(2)).getByRole("combobox", { name: t.pairs.key1 });
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "A" } });
    const taken = within(pair(2))
      .getAllByRole("option")
      .find((option) => option.textContent === `A · ${t.pairs.usedIn({ n: 1 })}`);
    if (taken === undefined) throw new Error("A is not offered with its reason");
    expect(taken.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(taken);
    expect(input).toHaveProperty("value", "A");
    fireEvent.blur(input);
    expect(input).toHaveProperty("value", "");
    // The quick pair whose keys are used is not offered again.
    expect(button(t.pairs.quick.ad)).toHaveProperty("disabled", true);
    fireEvent.click(within(pair(2)).getByRole("button", { name: t.pairs.remove }));
    expect(applyButton()).toHaveProperty("disabled", false);
    expect(commands()).toEqual(["get_socd_state"]);
  });
});

describe("the write flow", () => {
  it("plans, confirms, writes, reports with Undo and shows the values read back", async () => {
    const source = install("socd_apply_ok");
    renderTab();
    await ready();
    fireEvent.click(powerSwitch());
    expect(screen.getByText(t.power.pending({ on: true }))).toBeTruthy();
    fireEvent.click(button(t.pairs.quick.ad));
    expect(commands()).toEqual(["get_socd_state"]);
    fireEvent.click(applyButton());
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText(t.review.title)).toBeTruthy();
    expect(preparedIntents()).toEqual([recordedIntent(source)]);
    expect(within(dialog).getByText(t.review.power({ from: t.power.off, to: t.power.on }))).toBeTruthy();
    expect(within(dialog).getByText(en.common.badge.keyboardWide)).toBeTruthy();
    expect(within(dialog).getByText(en.actuation.edit.warnings.socd_power_all_profiles)).toBeTruthy();
    expect(within(dialog).getByText(en.actuation.edit.warnings.socd_hotkey)).toBeTruthy();
    expect(within(dialog).getByText("A + D")).toBeTruthy();
    await pressWrite();
    expect(await screen.findByText(en.flow.results.written)).toBeTruthy();
    expect(commands().filter((c) => c === "apply_socd_edit")).toHaveLength(1);
    expect(within(screen.getByRole("dialog")).getByRole("button", { name: en.common.actions.undo })).toBeTruthy();
    await waitFor(() => {
      expect(commands().filter((c) => c === "get_socd_state")).toHaveLength(2);
    });
    fireEvent.click(dialogAction(en.common.actions.close));
    expect(screen.getByText(t.apply.idle)).toBeTruthy();
    expect(powerSwitch().getAttribute("aria-checked")).toBe("true");
    expect(within(pair(1)).getByRole("combobox", { name: t.pairs.key1 })).toHaveProperty("value", "A");
  });

  it("marks modes 2 and 3 as not yet verified in the plan", async () => {
    const source = install("socd_preview");
    renderTab();
    await ready();
    fireEvent.click(powerSwitch());
    fireEvent.click(button(t.pairs.quick.ad));
    fireEvent.click(button(t.pairs.quick.ws));
    setMode(pair(2), 2);
    fireEvent.click(button(t.pairs.add));
    pickKey(pair(3), t.pairs.key1, "Q");
    pickKey(pair(3), t.pairs.key2, "E");
    setMode(pair(3), 3);
    expect(within(pair(3)).getByText(en.common.badge.unverified)).toBeTruthy();
    fireEvent.click(applyButton());
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText(t.review.title);
    expect(preparedIntents()).toEqual([recordedIntent(source)]);
    expect(within(dialog).getByText(en.actuation.edit.warnings.unverified({ what: en.common.badge.notes["SOCD modes 2 and 3"] ?? "" }))).toBeTruthy();
    expect(within(dialog).getAllByText(en.common.badge.unverified).length).toBeGreaterThan(0);
  });

  it("follows the keyboard's own toggle: the view updates and the pending plan asks to be reviewed again", async () => {
    install("socd_toggled");
    renderTab();
    await ready();
    fireEvent.click(button(t.pairs.quick.ad));
    setMode(pair(1), 4);
    fireEvent.click(applyButton());
    await screen.findByText(t.review.title);
    act(() => {
      emitNotice({ notice: "plan_invalidated", reason: "values_changed" });
      emitNotice({ notice: "socd_toggled_on_keyboard", power: true });
    });
    expect(await screen.findByText(en.flow.review.invalidated)).toBeTruthy();
    expect(screen.getByText(en.errors.planInvalidation.values_changed)).toBeTruthy();
    expect(screen.getByRole("button", { name: en.common.actions.writeToKeyboard })).toHaveProperty("disabled", true);
    await waitFor(() => {
      expect(commands().filter((c) => c === "get_socd_state")).toHaveLength(2);
    });
    expect(screen.getByText(t.power.toggled({ on: true }))).toBeTruthy();
    await waitFor(() => {
      expect(powerSwitch().getAttribute("aria-checked")).toBe("true");
    });
    expect(commands()).not.toContain("apply_socd_edit");
  });

  it("discards the plan on Cancel and keeps the draft", async () => {
    install("socd_preview");
    renderTab();
    await ready();
    fireEvent.click(powerSwitch());
    fireEvent.click(applyButton());
    await screen.findByText(t.review.title);
    fireEvent.click(dialogAction(en.common.actions.cancel));
    await waitFor(() => {
      expect(commands()).toContain("discard_plan");
    });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText(t.power.pending({ on: true }))).toBeTruthy();
  });

  it("replaces pairs kept as read only after an explicit confirmation", async () => {
    install("socd_apply_ok", { state: (state) => ({ ...state, pairs_writable: false, unwritable_reason: "odd pairs" }) });
    renderTab();
    await ready();
    expect(screen.getByText(t.replace.title)).toBeTruthy();
    expect(screen.getByText(en.common.badge.kept)).toBeTruthy();
    expect(screen.getByText(t.pairs.notShown)).toBeTruthy();
    fireEvent.click(button(t.replace.start));
    fireEvent.click(button(t.pairs.quick.ad));
    expect(applyButton()).toHaveProperty("disabled", true);
    expect(screen.getByText(t.apply.confirmReplace)).toBeTruthy();
    fireEvent.click(screen.getByRole("checkbox", { name: t.replace.confirm }));
    fireEvent.click(applyButton());
    await screen.findByText(t.review.title);
    expect(preparedIntents()[0]).toMatchObject({ pairs: [{ key1: 68, key2: 70, mode: 1 }], allow_not_undoable: true });
  });
});

describe("refusals", () => {
  it("asks to confirm replacing pairs Undo can't bring back, then plans again with that answer", async () => {
    const plan = fixture("socd_apply_ok").calls.find((c) => c.command === "prepare_socd_edit")?.ok;
    const refusal: AppError = {
      kind: "invalid_request",
      detail: {
        detail: "edit",
        problem: { problem: "unsupported_current_table", table: { table: "socd_pairs" }, reason: "pairs not in the site's form" },
      },
      message: "the current SOCD pairs can't be restored by Undo; nothing was written",
    };
    install("socd_apply_ok", {
      answers: {
        prepare_socd_edit: (n) => {
          if (n === 1) throw refusal;
          return plan;
        },
      },
    });
    renderTab();
    await ready();
    fireEvent.click(button(t.pairs.quick.ad));
    fireEvent.click(applyButton());
    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText(en.actuation.edit.problems.unsupported_current_table({ table: en.flow.tables.socd_pairs })),
    ).toBeTruthy();
    expect(within(dialog).getByText(en.flow.nothingWritten)).toBeTruthy();
    const again = within(dialog).getByRole("button", { name: t.replace.reviewAgain });
    expect(again).toHaveProperty("disabled", true);
    fireEvent.click(within(dialog).getByRole("checkbox", { name: t.replace.confirm }));
    fireEvent.click(within(dialog).getByRole("button", { name: t.replace.reviewAgain }));
    expect(await within(dialog).findByText(t.review.title)).toBeTruthy();
    expect(preparedIntents()).toHaveLength(2);
    expect(preparedIntents()[0]?.allow_not_undoable).toBe(false);
    expect(preparedIntents()[1]?.allow_not_undoable).toBe(true);
  });
});

describe("the power alone", () => {
  it("can't be written from a profile whose pairs aren't in the site's form, and says how it can", async () => {
    install("socd_apply_ok", { state: (state) => ({ ...state, pairs_writable: false, unwritable_reason: "a gap" }) });
    renderTab();
    await ready();
    fireEvent.click(powerSwitch());
    expect(screen.getByText(t.power.pending({ on: true }))).toBeTruthy();
    // The reason is on the switch's card and on the disabled Review.
    expect(screen.getAllByText(t.apply.powerNeedsPairs).length).toBeGreaterThan(0);
    expect(applyButton()).toHaveProperty("disabled", true);
    fireEvent.click(applyButton());
    expect(commands()).not.toContain("prepare_socd_edit");
    // Replacing the pairs as well lets it through, after the explicit confirmation.
    fireEvent.click(button(t.replace.start));
    expect(screen.queryByText(t.apply.powerNeedsPairs)).toBeNull();
    fireEvent.click(screen.getByRole("checkbox", { name: t.replace.confirm }));
    expect(applyButton()).toHaveProperty("disabled", false);
  });

  it("explains a refused power-only plan without the confirmation that can't help it", async () => {
    const refusal: AppError = {
      kind: "invalid_request",
      detail: {
        detail: "edit",
        problem: { problem: "unsupported_current_table", table: { table: "socd_pairs" }, reason: "give the pairs to replace them" },
      },
      message: "nothing was written",
    };
    install("socd_apply_ok", {
      answers: {
        prepare_socd_edit: () => {
          throw refusal;
        },
      },
    });
    renderTab();
    await ready();
    fireEvent.click(powerSwitch());
    fireEvent.click(applyButton());
    const dialog = await screen.findByRole("dialog");
    expect((await within(dialog).findAllByText(t.apply.powerNeedsPairs)).length).toBeGreaterThan(0);
    expect(within(dialog).queryByRole("checkbox", { name: t.replace.confirm })).toBeNull();
    expect(within(dialog).getByRole("button", { name: t.replace.reviewAgain })).toHaveProperty("disabled", true);
  });
});

describe("the keyboard's own toggle", () => {
  function states(powers: boolean[]): (n: number) => SocdStateView {
    const base = fixture("socd_ready").calls.find((c) => c.command === "get_socd_state")?.ok;
    if (base === undefined || base === null) throw new Error("no SOCD state");
    return (n) => ({ ...base, power: powers[Math.min(n, powers.length) - 1] ?? false, revision: base.revision + n });
  }

  it("drops a drafted power change that the toggle made a no-op", async () => {
    install("socd_ready", { answers: { get_socd_state: states([false, true]) } });
    renderTab();
    await ready();
    fireEvent.click(powerSwitch());
    expect(screen.getByText(t.power.pending({ on: true }))).toBeTruthy();
    act(() => {
      emitNotice({ notice: "socd_toggled_on_keyboard", power: true });
    });
    await waitFor(() => {
      expect(powerSwitch().getAttribute("aria-checked")).toBe("true");
    });
    await waitFor(() => {
      expect(screen.queryByText(t.power.pending({ on: true }))).toBeNull();
    });
    expect(socdDrafts.effective(socdDrafts.store.get(), null).power).toBeNull();
    expect(applyButton()).toHaveProperty("disabled", true);
    expect(screen.getByText(t.apply.idle)).toBeTruthy();
  });

  it("shows the toggle's note only while it is still true", async () => {
    install("socd_ready", { answers: { get_socd_state: states([false, true, false]) } });
    renderTab();
    await ready();
    act(() => {
      emitNotice({ notice: "socd_toggled_on_keyboard", power: true });
    });
    expect(await screen.findByText(t.power.toggled({ on: true }))).toBeTruthy();
    // The keyboard is read again and SOCD is off now (the user's own write, another toggle…).
    act(() => {
      socdPart.refetch();
    });
    await waitFor(() => {
      expect(powerSwitch().getAttribute("aria-checked")).toBe("false");
    });
    expect(screen.queryByText(t.power.toggled({ on: true }))).toBeNull();
  });
});

describe("the write flow and the connection", () => {
  it("stays open when the keyboard goes away, and doesn't prepare by itself when it is back", async () => {
    const source = install("socd_preview");
    renderTab();
    await ready();
    fireEvent.click(powerSwitch());
    fireEvent.click(applyButton());
    await screen.findByText(t.review.title);
    expect(preparedIntents()).toHaveLength(1);
    act(() => {
      statusStore.set({
        ...source.initial_status,
        connection: { state: "searching" },
        write_gate: { allowed: false, reasons: ["not_connected"] },
        snapshot_revision: null,
      });
    });
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByText(t.review.title)).toBeTruthy();
    act(() => {
      statusStore.set({ ...source.initial_status, snapshot_revision: (source.initial_status.snapshot_revision ?? 0) + 5 });
    });
    await waitFor(() => {
      expect(commands().filter((c) => c === "get_socd_state").length).toBeGreaterThan(1);
    });
    await sleep(20);
    expect(preparedIntents()).toHaveLength(1);
    expect(screen.queryByText(en.errors.kind.not_connected.title)).toBeNull();
  });

  it("never carries the persistence tick into the next flow", async () => {
    install("socd_preview");
    renderTab();
    await ready();
    fireEvent.click(powerSwitch());
    fireEvent.click(applyButton());
    await screen.findByText(t.review.title);
    const check = screen.getByRole("checkbox", { name: /profile/u });
    fireEvent.click(check);
    await waitFor(() => {
      expect(preparedIntents()).toHaveLength(2);
    });
    await screen.findByText(t.review.title);
    fireEvent.click(dialogAction(en.common.actions.cancel));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    fireEvent.click(applyButton());
    await screen.findByText(t.review.title);
    expect(preparedIntents().map((intent) => intent.check_persistence)).toEqual([false, true, false]);
  });
});

describe("blocked states", () => {
  it("says why SOCD can't be changed and keeps Review disabled with that reason", async () => {
    install("socd_ready", {
      state: (state) => ({ ...state, editable: false, disabled: { reason: "unreadable", message: "x" } }),
    });
    renderTab();
    await ready();
    expect(screen.getAllByText(t.disabled.unreadable).length).toBeGreaterThanOrEqual(1);
    expect(applyButton()).toHaveProperty("disabled", true);
    expect(powerSwitch()).toHaveProperty("disabled", true);
  });

  it("says another program uses the keyboard instead of asking to connect it", () => {
    install("socd_ready", {
      status: {
        connection: { state: "other_program", evidence: { status_reports: 1, foreign_acks: 0, profile_reports: 0 } },
        write_gate: { allowed: false, reasons: ["other_program"] },
        snapshot_revision: null,
      },
    });
    renderTab();
    expect(screen.getByText(t.empty.title)).toBeTruthy();
    expect(screen.queryByText(t.empty.body)).toBeNull();
    expect(screen.getByText(en.shell.status.otherProgram)).toBeTruthy();
    expect(screen.getByRole("button", { name: en.common.notRead.goToKeyboard })).toBeTruthy();
  });

  it("shows an empty state while no keyboard is read", () => {
    install("socd_ready", {
      status: {
        connection: { state: "searching" },
        write_gate: { allowed: false, reasons: ["not_connected"] },
        snapshot_revision: null,
      },
    });
    renderTab();
    expect(screen.getByText(t.empty.title)).toBeTruthy();
    expect(screen.queryByRole("switch")).toBeNull();
  });

  it("is read-only while the values may be out of date", async () => {
    install("socd_ready", { status: { snapshot_stale: true, write_gate: { allowed: false, reasons: ["snapshot_stale"] } } });
    renderTab();
    await ready();
    expect(screen.getByText(en.actuation.stale)).toBeTruthy();
    expect(powerSwitch()).toHaveProperty("disabled", true);
    expect(button(t.pairs.quick.ad)).toHaveProperty("disabled", true);
  });
});
