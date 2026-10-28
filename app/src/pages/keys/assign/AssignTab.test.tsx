// Keys > Assignments on the mocked IPC: the preview's mock backend answers
// with the fixtures the core's tests recorded from the real worker on the
// simulator (keys_*), through the real `api`, `invoke` and `Channel`.
//
// Covered: the board (layers, L, caps, the Fn key, the knob's popover), the
// selected key's summary, the draft (Change…, Swap with…, Reset key for a
// broken reference), the review → Write → report with the recorded plan,
// Cancel, a critical built-in (confirmation on the page, the refusal answered
// in the flow), a failed write, an out-of-date plan, a closed write gate and
// a profile change that discards the draft.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ToastRegion } from "../../../components/Toasts";
import { DICTIONARIES, setLang, useDict } from "../../../i18n";
import type { AppError, KeymapEditIntent, KeysState } from "../../../ipc/bindings";
import { createMockBackend } from "../../../preview/mockBackend";
import { FIXTURES } from "../../../preview/registry";
import type { Fixture } from "../../../preview/types";
import { appInfoStore, DEFAULT_APP_INFO } from "../../../state/appInfo";
import { draftEntries } from "../../../state/drafts";
import { acknowledgeVendor, resetSession } from "../../../state/session";
import { INITIAL_STATUS, statusStore, useStatus } from "../../../state/status";
import { clearToasts } from "../../../state/toasts";
import { keysBlockReason, keysStore, resetKeysStore, useKeysState } from "../shared";
import { assignState, resetAssignStore, updateDraft } from "./draftStore";
import KeysPage from "../index";
import AssignTab from "./index";
import { setTarget } from "./model";

const en = DICTIONARIES.en;
/** The key-assignment review's title on profile 1 (its words follow what the plan changes). */
const REVIEW_TITLE = /^Change .+ on profile 1\?$/u;
const t = en.keysAssign;

const R = 48;
const F2 = 3;
const CAPS = 66;
const BACKTICK = 22;
const J = 74;
const Q = 45;
const TWO = 24;
const FN = 121;
const KNOB = 15;

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

function recordedIntent(source: Fixture): KeymapEditIntent {
  const call = source.calls.find((c) => c.command === "prepare_keymap_edit");
  if (call?.command !== "prepare_keymap_edit") throw new Error(`no prepare in ${source.id}`);
  return call.args.intent;
}

/** Installs the mock backend for a fixture (optionally overriding commands) and puts its status into the store. */
function install(id: string, override?: (command: string) => AppError | null): Fixture {
  const source = fixture(id);
  const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
  mockIPC(async (command, args) => {
    calls.push({ command, args: (args ?? {}) as Record<string, unknown> });
    const refusal = override?.(command) ?? null;
    if (refusal !== null) throw refusal;
    const result = await backend.handle(command, args);
    return result;
  });
  statusStore.set({ ...source.initial_status, write_gate: { allowed: true, reasons: [] }, busy: null });
  appInfoStore.set(source.app_info);
  return source;
}

function count(command: string): number {
  return calls.filter((c) => c.command === command).length;
}

function preparedIntents(): KeymapEditIntent[] {
  return calls.filter((c) => c.command === "prepare_keymap_edit").map((c) => c.args["intent"] as KeymapEditIntent);
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

/** The tab as the Keys page renders it (the page itself is covered by the shared tests). */
function Harness() {
  const dict = useDict();
  const status = useStatus();
  const view = useKeysState();
  if (view.state === null) return null;
  return <AssignTab state={view.state} view={view} blockReason={keysBlockReason(view, status, dict)} />;
}

async function renderPage(): Promise<HTMLElement> {
  render(
    <>
      <Harness />
      <ToastRegion />
    </>,
  );
  return screen.findByRole("listbox", { name: t.board.label.base });
}

function board(): HTMLElement {
  const found = document.querySelector<HTMLElement>(".keys-assign-board [role='listbox']");
  if (found === null) throw new Error("no board");
  return found;
}

function keyOption(index: number): HTMLElement {
  const option = board().querySelector<HTMLElement>(`[data-key-index="${index}"]`);
  if (option === null) throw new Error(`no key ${index}`);
  return option;
}

function selectKey(index: number): void {
  const option = keyOption(index);
  act(() => {
    option.focus();
  });
  fireEvent.keyDown(option, { key: " " });
}

function summary(): HTMLElement {
  const heading = screen.getByRole("heading", { name: t.summary.title });
  return heading.closest("section") as HTMLElement;
}

function applyBar(): HTMLElement {
  return screen.getByRole("region", { name: en.common.actions.reviewAndWrite });
}

async function pick(query: string, option?: string): Promise<void> {
  const search = await screen.findByRole("combobox", { name: en.keys.picker.search });
  fireEvent.change(search, { target: { value: query } });
  if (option === undefined) {
    fireEvent.keyDown(search, { key: "Enter" });
  } else {
    const list = screen.getByRole("listbox", { name: en.keys.picker.options });
    fireEvent.click(within(list).getByText(option));
  }
}

async function pressWrite(): Promise<void> {
  const write = await screen.findByRole("button", { name: en.common.actions.writeToKeyboard });
  await sleep(560);
  fireEvent.click(write);
}

beforeEach(() => {
  setLang("en");
  resetSession();
  acknowledgeVendor();
  resetKeysStore();
  resetAssignStore();
  calls = [];
  window.location.hash = "#/keys";
});

afterEach(() => {
  cleanup();
  clearMocks();
  clearToasts();
  statusStore.set(INITIAL_STATUS);
  appInfoStore.set(DEFAULT_APP_INFO);
  resetKeysStore();
  resetAssignStore();
  window.location.hash = "";
  setLang("en");
});

describe("the board", () => {
  it("shows what changed keys do, switches layers with L and the control, and announces the layer", async () => {
    install("keys_ready");
    await renderPage();
    expect(keyOption(CAPS).getAttribute("aria-label")).toBe("CTRL (Caps Lock)");
    expect(keyOption(CAPS).getAttribute("data-flag")).toBe("true");
    expect(keyOption(J).textContent).toContain("⚠");

    fireEvent.keyDown(keyOption(CAPS), { key: "l" });
    expect(screen.getByRole("listbox", { name: t.board.label.fn })).toBeTruthy();
    expect(screen.getByRole("radio", { name: en.keys.layers.fn }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText(t.layer.shown.fn)).toBeTruthy();
    // On the Fn layer F2 shows its built-in, muted, and F1 its shortcut.
    expect(keyOption(F2).textContent).toContain("Vol −");
    const describedBy = keyOption(F2).getAttribute("aria-describedby") ?? "";
    expect(document.getElementById(describedBy)?.textContent).toContain("Volume down");
    expect(keyOption(2).getAttribute("aria-label")).toBe("CTRL+C (F1)");

    fireEvent.click(screen.getByRole("radio", { name: en.keys.layers.base }));
    expect(screen.getByRole("listbox", { name: t.board.label.base })).toBeTruthy();
    expect(screen.getByText(t.layer.shown.base)).toBeTruthy();
  });

  it("keeps the Fn key unselectable with the reason, and lists the knob's entries as kept as read", async () => {
    install("keys_ready");
    await renderPage();
    const fn = keyOption(FN);
    expect(fn.getAttribute("aria-disabled")).toBe("true");
    expect(fn.getAttribute("title")).toBe(en.keys.fnLocked);
    selectKey(FN);
    expect(within(summary()).getByText(t.summary.none)).toBeTruthy();

    const knob = keyOption(KNOB);
    act(() => {
      knob.focus();
    });
    fireEvent.keyDown(knob, { key: "Enter" });
    const popover = screen.getByRole("dialog", { name: en.common.pickers.keyboard.knobTitle });
    expect(within(popover).getByText(en.common.badge.kept)).toBeTruthy();
    expect(within(popover).getAllByRole("listitem")).toHaveLength(7);
    expect(within(popover).getByText(en.keys.knobEntry({ n: 16, layer: "Base", text: "Default" }))).toBeTruthy();
  });

  it("summarises the selected key: kind, slot, the other layer and the bytes", async () => {
    install("keys_ready");
    await renderPage();
    selectKey(Q);
    const box = summary();
    expect(within(box).getByText("Q")).toBeTruthy();
    expect(within(box).getByText(t.summary.otherLayer.fn)).toBeTruthy();
    await within(box).findByText("Macro 3 · Type A");

    fireEvent.keyDown(keyOption(Q), { key: "L" });
    const fnBox = summary();
    expect(within(fnBox).getByText("Fn + Q")).toBeTruthy();
    expect(within(fnBox).getByText(en.keys.origin.macro_slot)).toBeTruthy();
    expect(within(fnBox).getByText("2 steps · Plays once")).toBeTruthy();
    expect(within(fnBox).getByRole("link", { name: t.summary.openMacros }).getAttribute("href")).toBe("#/keys?tab=macros");
    expect(fnBox.textContent).toContain(t.summary.bytes({ layer: en.keys.layers.fn, hex: "10140200" }));
  });
});

describe("the selected key's panel", () => {
  it("comes into view after a click on a key, but not when the arrows move the selection", async () => {
    install("keys_ready");
    await renderPage();
    const scrolled: Element[] = [];
    // (jsdom has no scrollIntoView: a stand-in that records which element asked.)
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value(this: Element) {
        scrolled.push(this);
      },
    });
    try {
      const key = keyOption(R);
      fireEvent.pointerDown(key, { button: 0, pointerId: 1, clientX: 5, clientY: 5 });
      fireEvent.pointerUp(key, { button: 0, pointerId: 1, clientX: 5, clientY: 5 });
      await waitFor(() => {
        expect(scrolled.some((element) => element.classList.contains("keys-assign__summary-anchor"))).toBe(true);
      });
      scrolled.length = 0;
      selectKey(Q);
      await sleep(30);
      expect(scrolled.some((element) => element.classList.contains("keys-assign__summary-anchor"))).toBe(false);
    } finally {
      Reflect.deleteProperty(Element.prototype, "scrollIntoView");
    }
  });
});

describe("draft → review → write", () => {
  it("drafts a shortcut, a built-in replacement and a swap, and writes the recorded plan", async () => {
    const source = install("keys_apply_ok");
    await renderPage();

    selectKey(R);
    fireEvent.click(within(summary()).getByRole("button", { name: t.actions.change }));
    await pick("ctrl c");
    await waitFor(() => {
      expect(screen.queryByRole("combobox", { name: en.keys.picker.search })).toBeNull();
    });
    expect(keyOption(R).getAttribute("aria-label")).toBe("CTRL+C (R)");
    expect(applyBar().textContent).toContain(t.apply.count({ n: 1 }));

    fireEvent.keyDown(keyOption(R), { key: "l" });
    selectKey(F2);
    const warning = en.keys.builtinWarning({ key: "Fn + F2 (#3)", function: "Volume down" });
    expect(within(summary()).getByText(warning)).toBeTruthy();
    fireEvent.click(within(summary()).getByRole("button", { name: t.actions.change }));
    const dialog = await screen.findByRole("dialog", { name: en.keys.picker.title({ key: "Fn + F2" }) });
    expect(within(dialog).getByText(warning)).toBeTruthy();
    await pick("back button", "Back button (X1)");

    fireEvent.keyDown(keyOption(F2), { key: "l" });
    selectKey(CAPS);
    fireEvent.click(within(summary()).getByRole("button", { name: t.actions.swap }));
    const swap = await screen.findByRole("dialog", { name: t.swap.title({ key: "Caps Lock" }) });
    const other = within(swap).getByRole("combobox", { name: t.swap.label });
    fireEvent.change(other, { target: { value: "Backt" } });
    fireEvent.keyDown(other, { key: "Enter" });
    expect(
      within(swap).getByText(t.swap.result({ a: "Caps Lock", aNew: "Mute", b: "Backtick", bNew: "Left Ctrl" })),
    ).toBeTruthy();
    fireEvent.click(within(swap).getByRole("button", { name: t.swap.confirm }));

    expect(applyBar().textContent).toContain(t.apply.count({ n: 4 }));
    expect(draftEntries()).toEqual([{ page: "keys", tab: "assign", count: 4 }]);
    expect(keyOption(BACKTICK).textContent).toContain("●");

    fireEvent.click(within(applyBar()).getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    expect(preparedIntents()).toEqual([recordedIntent(source)]);

    await pressWrite();
    await screen.findByText(en.flow.footer.safetyBackup);
    expect(count("apply_keymap_edit")).toBe(1);
    // A complete write spends the draft.
    expect(assignState().draft.targets).toEqual([]);
    expect(draftEntries()).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: en.common.actions.close }));
    await waitFor(() => {
      expect(count("get_keys_state")).toBe(2);
    });
    // The refreshed view: Caps now does Mute.
    await waitFor(() => {
      expect(keyOption(CAPS).getAttribute("aria-label")).toBe("Mute (Caps Lock)");
    });
  });

  it("writes nothing when the review is cancelled, and keeps the draft", async () => {
    install("keys_preview_assign");
    await renderPage();
    const state = keysStore.get().state as KeysState;
    act(() => {
      updateDraft((draft) => setTarget(state, draft, "base", R, { kind: "combo", modifiers: [0xe0], key: 0x06 }));
    });
    fireEvent.click(within(applyBar()).getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.cancel }));
    await waitFor(() => {
      expect(screen.queryByText(REVIEW_TITLE)).toBeNull();
    });
    expect(count("apply_keymap_edit")).toBe(0);
    expect(count("discard_plan")).toBe(1);
    expect(assignState().draft.targets).toHaveLength(1);
  });

  it("asks for the explicit confirmation of an essential built-in, and answers the refusal in the flow", async () => {
    const source = install("keys_refused_critical");
    await renderPage();
    fireEvent.click(screen.getByRole("radio", { name: en.keys.layers.fn }));
    selectKey(TWO);
    const confirmLabel = en.keys.builtinConfirmProfile({ k: 2, n: 1 });
    const box = summary();
    expect(within(box).getByText(en.keys.builtinProfile({ k: 2, n: 1 }))).toBeTruthy();
    const change = within(box).getByRole("button", { name: t.actions.change });
    expect(change).toHaveProperty("disabled", true);
    expect(within(box).getByText(t.actions.reasons.confirmFirst)).toBeTruthy();

    fireEvent.click(within(box).getByRole("checkbox", { name: confirmLabel }));
    expect(assignState().draft.confirmed).toEqual([TWO]);
    fireEvent.click(within(summary()).getByRole("button", { name: t.actions.change }));
    const dialog = await screen.findByRole("dialog", { name: en.keys.picker.title({ key: "Fn + 2" }) });
    expect(within(dialog).getByText(t.picker.criticalConfirmed)).toBeTruthy();
    await pick("0x04");
    expect(assignState().draft.targets).toEqual([{ layer: "fn", key: TWO, action: { kind: "key", usage: 4 } }]);

    // Withdrawn on the page: the library refuses, and the flow asks again.
    fireEvent.click(within(summary()).getByRole("checkbox", { name: confirmLabel }));
    fireEvent.click(within(applyBar()).getByRole("button", { name: en.common.actions.reviewAndWrite }));
    const again = await screen.findByRole("button", { name: en.keys.review.reviewAgain });
    const flow = again.closest("dialog") as HTMLElement;
    expect(preparedIntents()).toEqual([recordedIntent(source)]);
    expect(again).toHaveProperty("disabled", true);
    fireEvent.click(within(flow).getByRole("checkbox", { name: confirmLabel }));
    await waitFor(() => {
      expect(within(flow).getByRole("button", { name: en.keys.review.reviewAgain })).toHaveProperty("disabled", false);
    });
    fireEvent.click(within(flow).getByRole("button", { name: en.keys.review.reviewAgain }));
    await waitFor(() => {
      expect(preparedIntents()).toHaveLength(2);
    });
    expect(preparedIntents()[1]?.builtin_acks).toEqual([{ key: TWO, critical: true }]);
    expect(count("apply_keymap_edit")).toBe(0);

    // The confirmation given in the flow stays with the draft.
    fireEvent.click(await screen.findByRole("button", { name: en.common.actions.close }));
    await waitFor(() => {
      expect(assignState().draft.confirmed).toEqual([TWO]);
    });
  });

  it("keeps the draft after a failed write and offers to reset the keys left on emptied macros", async () => {
    install("keys_apply_failed_cleared");
    await renderPage();
    const state = keysStore.get().state as KeysState;
    act(() => {
      updateDraft((draft) => setTarget(state, draft, "base", R, { kind: "key", usage: 0x05 }));
    });
    fireEvent.click(within(applyBar()).getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    await pressWrite();
    await screen.findByText(en.flow.report.clearedTitle);
    expect(assignState().draft.targets).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: en.common.actions.close }));
    const list = await screen.findByRole("heading", { name: t.broken.title });
    const card = list.closest("section") as HTMLElement;
    expect(within(card).getByText(t.broken.emptySlot({ key: "Q (#45)", slot: "Macro 2" }))).toBeTruthy();
    expect(within(card).getByText(t.broken.emptySlot({ key: "Fn + W (#46)", slot: "Macro 3" }))).toBeTruthy();
    fireEvent.click(within(card).getByRole("button", { name: t.broken.resetLabel({ key: "Q (#45)" }) }));
    expect(within(card).getByText(t.broken.inDraft)).toBeTruthy();
    expect(assignState().draft.targets.map((target) => [target.layer, target.key])).toEqual([
      ["base", R],
      ["base", Q],
    ]);
  });

  it("keeps the write's report on screen when the keys state goes away after the write", async () => {
    install("keys_apply_failed_cleared");
    window.location.hash = "#/keys?tab=assign";
    render(
      <>
        <KeysPage />
        <ToastRegion />
      </>,
    );
    await screen.findByRole("listbox", { name: t.board.label.base });
    const state = keysStore.get().state as KeysState;
    act(() => {
      updateDraft((draft) => setTarget(state, draft, "base", R, { kind: "key", usage: 0x05 }));
    });
    fireEvent.click(within(applyBar()).getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    await pressWrite();
    await screen.findByText(en.flow.report.clearedTitle);
    // The keyboard drops: the status says so after the write's reply, and the keys state goes.
    act(() => {
      statusStore.set({ ...statusStore.get(), snapshot_revision: null, connection: { state: "searching" } });
    });
    expect(keysStore.get().state).toBeNull();
    // The report (emptied macros, Undo) stays until the user closes it.
    expect(screen.getByText(en.flow.report.clearedTitle)).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.close }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(screen.getByText(en.keys.empty.title)).toBeTruthy();
  });

  it("resets a key that refers to an empty macro with one click", async () => {
    install("keys_ready");
    await renderPage();
    const card = screen.getByRole("heading", { name: t.broken.title }).closest("section") as HTMLElement;
    fireEvent.click(within(card).getByRole("button", { name: t.broken.resetLabel({ key: "J (#74)" }) }));
    expect(assignState().draft.targets).toEqual([{ layer: "base", key: J, action: { kind: "default" } }]);
    expect(keyOption(J).getAttribute("aria-label")).toBe("J");
    expect(keyOption(J).textContent).toContain("●");
  });

  it("shows an out-of-date plan and reads the keys state again", async () => {
    install("keys_ready", (command) =>
      command === "prepare_keymap_edit"
        ? { kind: "stale_plan", detail: { detail: "plan_invalidated", reason: "snapshot_changed" }, message: "stale" }
        : null,
    );
    await renderPage();
    const state = keysStore.get().state as KeysState;
    act(() => {
      updateDraft((draft) => setTarget(state, draft, "base", R, { kind: "key", usage: 0x05 }));
    });
    fireEvent.click(within(applyBar()).getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(en.errors.kind.stale_plan.title);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.close }));
    await waitFor(() => {
      expect(count("get_keys_state")).toBe(2);
    });
    expect(assignState().draft.targets).toHaveLength(1);
  });
});

describe("when writing isn't possible", () => {
  it("says why Review and write can't be used, and discards the draft when the profile changes", async () => {
    install("keys_ready");
    await renderPage();
    const state = keysStore.get().state as KeysState;
    act(() => {
      updateDraft((draft) => setTarget(state, draft, "base", R, { kind: "key", usage: 0x05 }));
    });
    act(() => {
      statusStore.set({ ...statusStore.get(), write_gate: { allowed: false, reasons: ["not_connected"] } });
    });
    const review = within(applyBar()).getByRole("button", { name: en.common.actions.reviewAndWrite });
    expect(review).toHaveProperty("disabled", true);
    expect(within(applyBar()).getByText(en.errors.writeBlocked.not_connected)).toBeTruthy();

    act(() => {
      keysStore.set({ ...keysStore.get(), state: { ...state, profile: 2 } });
    });
    expect(assignState().draft.targets).toEqual([]);
    expect(await screen.findByText(t.draftDiscarded({ n: 2 }))).toBeTruthy();
    expect(draftEntries()).toEqual([]);
  });
});

describe("when the keys can't be changed", () => {
  it("keeps a draft while disconnected and discards it when the keyboard is connected again", async () => {
    install("keys_ready");
    await renderPage();
    const state = keysStore.get().state as KeysState;
    act(() => {
      updateDraft((draft) => setTarget(state, draft, "base", R, { kind: "key", usage: 0x05 }));
    });
    act(() => {
      keysStore.set({ ...keysStore.get(), state: null });
    });
    expect(assignState().draft.targets).toHaveLength(1);
    act(() => {
      keysStore.set({ ...keysStore.get(), state: { ...state, generation: state.generation + 1 } });
    });
    expect(assignState().draft.targets).toEqual([]);
    expect(await screen.findByText(t.draftDiscardedReconnect)).toBeTruthy();
  });

  it("shows the keys but offers no change when OpenWraith can't change them", async () => {
    install("keys_ready");
    await renderPage();
    const state = keysStore.get().state as KeysState;
    act(() => {
      keysStore.set({
        ...keysStore.get(),
        state: { ...state, editable: false, disabled: { reason: "unsupported", message: "unsupported" } },
      });
    });
    selectKey(R);
    const box = summary();
    expect(within(box).getByRole("button", { name: t.actions.change })).toHaveProperty("disabled", true);
    expect(within(box).getByRole("button", { name: t.actions.swap })).toHaveProperty("disabled", true);
    expect(within(box).getByText(en.keys.disabled.unsupported)).toBeTruthy();
    fireEvent.keyDown(keyOption(R), { key: "Enter" });
    expect(screen.queryByRole("combobox", { name: en.keys.picker.search })).toBeNull();
    const card = screen.getByRole("heading", { name: t.broken.title }).closest("section") as HTMLElement;
    expect(within(card).getByRole("button", { name: t.broken.resetLabel({ key: "J (#74)" }) })).toHaveProperty(
      "disabled",
      true,
    );
  });
});

describe("in Turkish", () => {
  it("shows the tab's own words", async () => {
    setLang("tr");
    install("keys_ready");
    render(<Harness />);
    const tr = DICTIONARIES.tr.keysAssign;
    await screen.findByRole("listbox", { name: tr.board.label.base });
    fireEvent.keyDown(keyOption(CAPS), { key: "l" });
    expect(screen.getByRole("listbox", { name: tr.board.label.fn })).toBeTruthy();
    expect(screen.getByText(tr.summary.none)).toBeTruthy();
    expect(screen.getByText(tr.apply.idle)).toBeTruthy();
  });
});
