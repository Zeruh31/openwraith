// Keys > Macros on the mocked IPC: the preview's mock backend answers with
// the fixtures the core's tests recorded from the real worker on the
// simulator (keys_*), through the real `api`, `invoke` and `Channel`; the
// macro library of this PC is a small in-test model.
//
// Covered: the slots (names from the library, macros no key runs, keys left
// on an empty slot), draft → review → write → report (a new macro on
// Fn + R with the persistence check and "Try it"; Macro 1 with the re-sent
// macros and a failed write that empties them), cancel, a refused critical
// built-in (answered in the editor or in the review), a disconnected
// keyboard, Assign… / Remove / Delete, the live checks with "Release all at
// end", the recorder, the library (save, rename, delete, put on a key), the
// draft discarded on another profile, and the Turkish texts.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ToastRegion } from "../../../components/Toasts";
import { DICTIONARIES, setLang } from "../../../i18n";
import type {
  EditReportView,
  KeymapEditIntent,
  KeysState,
  LibraryMacroView,
  MacroLibraryView,
  MacroProgramView,
} from "../../../ipc/bindings";
import { createMockBackend } from "../../../preview/mockBackend";
import { FIXTURES } from "../../../preview/registry";
import type { Fixture } from "../../../preview/types";
import { appInfoStore, DEFAULT_APP_INFO } from "../../../state/appInfo";
import { draftEntries } from "../../../state/drafts";
import { acknowledgeVendor, resetSession } from "../../../state/session";
import { INITIAL_STATUS, statusStore } from "../../../state/status";
import { clearToasts, getToasts } from "../../../state/toasts";
import KeysPage from "../index";
import { keysStore, resetKeysStore } from "../shared";
import { macrosState, resetMacrosStore, setTarget } from "./draftStore";

const en = DICTIONARIES.en;
/** The key-assignment review's title on profile 1 (its words follow what the plan changes). */
const REVIEW_TITLE = /^Change .+ on profile 1\?$/u;
const t = en.keysMacros;

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

function recordedState(source: Fixture, index = 0): KeysState {
  const call = source.calls.filter((c) => c.command === "get_keys_state")[index];
  if (call?.command !== "get_keys_state" || call.ok === null) throw new Error(`no keys state in ${source.id}`);
  return call.ok;
}

function recordedIntent(source: Fixture): KeymapEditIntent {
  const call = source.calls.find((c) => c.command === "prepare_keymap_edit");
  if (call?.command !== "prepare_keymap_edit") throw new Error(`no prepare in ${source.id}`);
  return call.args.intent;
}

function recordedLibrary(): MacroLibraryView {
  const call = fixture("keys_ready").calls.find((c) => c.command === "list_macro_library");
  if (call?.command !== "list_macro_library" || call.ok === null) throw new Error("no library in keys_ready");
  return call.ok;
}

/** The library of this PC as the core keeps it (identity = content hash). */
function libraryModel(state: KeysState) {
  let library = recordedLibrary();
  const hashOf = (program: MacroProgramView): string => {
    for (const slot of state.macros) {
      if (slot.content.state === "program" && JSON.stringify(slot.content.program) === JSON.stringify(program)) {
        return slot.content.content_hash;
      }
    }
    return library.entries.find((e) => JSON.stringify(e.program) === JSON.stringify(program))?.content_hash ?? "0000000000000abc";
  };
  return (command: string, args: Record<string, unknown>): MacroLibraryView | undefined => {
    switch (command) {
      case "list_macro_library":
        return structuredClone(library);
      case "save_library_macro": {
        const program = args["program"] as MacroProgramView;
        const hash = hashOf(program);
        const entry: LibraryMacroView = { content_hash: hash, name: String(args["name"]), program, saved_at: "2026-10-28T08:00:00Z" };
        const others = library.entries.filter((e) => e.content_hash !== hash);
        library = { entries: [...others, entry], notice: null };
        return structuredClone(library);
      }
      case "rename_library_macro":
        library = {
          entries: library.entries.map((e) => (e.content_hash === args["hash"] ? { ...e, name: String(args["name"]) } : e)),
          notice: null,
        };
        return structuredClone(library);
      case "delete_library_macro":
        library = { entries: library.entries.filter((e) => e.content_hash !== args["hash"]), notice: null };
        return structuredClone(library);
      default:
        return undefined;
    }
  };
}

/** Installs the mock backend for a fixture (and the library model) and puts its status into the store. */
function install(id: string, overrides: Record<string, (args: Record<string, unknown>) => unknown> = {}): Fixture {
  const source = fixture(id);
  const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
  const library = libraryModel(recordedState(source));
  mockIPC((command, args) => {
    const record = (args ?? {}) as Record<string, unknown>;
    calls.push({ command, args: record });
    const override = overrides[command];
    if (override !== undefined) return override(record);
    const answer = library(command, record);
    if (answer !== undefined) return answer;
    return backend.handle(command, args);
  });
  statusStore.set({ ...source.initial_status, write_gate: { allowed: true, reasons: [] }, busy: null });
  appInfoStore.set(source.app_info);
  return source;
}

function preparedIntents(): KeymapEditIntent[] {
  return calls.filter((c) => c.command === "prepare_keymap_edit").map((c) => c.args["intent"] as KeymapEditIntent);
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

async function renderTab(): Promise<void> {
  render(
    <>
      <KeysPage />
      <ToastRegion />
    </>,
  );
  await screen.findByRole("heading", { name: t.slots.title({ n: 1 }) });
  await screen.findByRole("list", { name: t.library.listLabel });
}

function slotRow(slot: number): HTMLElement {
  const list = screen.getByRole("list", { name: t.slots.listLabel });
  const row = within(list).getAllByRole("listitem")[slot];
  if (row === undefined) throw new Error(`no row ${slot}`);
  return row;
}

function editor(name: string): HTMLElement {
  return screen.getByRole("region", { name: `${t.editor.label}: ${name}` });
}

function chooseKey(region: HTMLElement, name: string): void {
  const input = within(region).getByRole("combobox", { name: t.editor.key });
  fireEvent.focus(input);
  fireEvent.change(input, { target: { value: name } });
  const list = within(region).getByRole("listbox", { name: en.common.pickers.keyPicker.options({ label: t.editor.key }) });
  fireEvent.click(within(list).getByRole("option", { name }));
}

function addTap(region: HTMLElement, usage: number): void {
  fireEvent.change(within(region).getByLabelText(t.editor.addKey), { target: { value: String(usage) } });
  fireEvent.click(within(region).getByRole("button", { name: t.editor.addTap }));
}

function reviewButton(): HTMLElement {
  return screen.getByRole("button", { name: en.common.actions.reviewAndWrite });
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
  resetMacrosStore();
  calls = [];
  window.location.hash = "#/keys?tab=macros";
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  clearMocks();
  clearToasts();
  statusStore.set(INITIAL_STATUS);
  appInfoStore.set(DEFAULT_APP_INFO);
  resetKeysStore();
  resetMacrosStore();
  window.location.hash = "";
});

describe("the keyboard's slots", () => {
  it("show names from the library, the keys that run them, macros no key runs and keys left on an empty slot", async () => {
    install("keys_ready");
    await renderTab();
    expect(screen.getByText(t.slots.used({ n: 2, total: 10 }), { exact: false })).toBeTruthy();

    const typeA = slotRow(2);
    expect(within(typeA).getByText("Macro 3 · Type A")).toBeTruthy();
    expect(within(typeA).getByText("2 steps · Plays once")).toBeTruthy();
    expect(within(typeA).getByText(t.slots.runsFrom({ keys: "Fn + Q" }))).toBeTruthy();
    expect(within(typeA).getByRole("button", { name: t.slots.actions.delete({ name: "Macro 3 · Type A" }) })).toBeTruthy();
    // Named by the library already: no "Save to library…".
    expect(within(typeA).queryByRole("button", { name: t.slots.actions.save({ name: "Macro 3 · Type A" }) })).toBeNull();

    const orphan = slotRow(3);
    expect(within(orphan).getByText(t.slots.orphan)).toBeTruthy();
    expect(within(orphan).getByText("2 steps · Plays 3 times")).toBeTruthy();
    expect(within(orphan).getByRole("button", { name: t.slots.actions.assign({ name: "Macro 4" }) })).toBeTruthy();
    expect(within(orphan).getByRole("button", { name: t.slots.actions.delete({ name: "Macro 4" }) })).toBeTruthy();
    expect(within(orphan).getByRole("button", { name: t.slots.actions.save({ name: "Macro 4" }) })).toBeTruthy();

    const dangling = slotRow(5);
    expect(within(dangling).getByText(t.slots.dangling({ keys: "J", n: 1 }))).toBeTruthy();
    expect(within(dangling).getByRole("button", { name: t.slots.actions.create({ name: "Macro 6" }) })).toBeTruthy();

    const library = screen.getByRole("list", { name: t.library.listLabel });
    expect(within(library).getByText(t.library.onKeyboard({ slots: "Macro 3" }))).toBeTruthy();
    expect(within(library).getAllByRole("listitem").map((li) => li.querySelector(".ow-strong")?.textContent)).toEqual([
      "Type A",
      "Copy",
    ]);
    // Nothing to write yet.
    expect(reviewButton()).toHaveProperty("disabled", true);
  });

  it("turn Assign… and Delete into the right changes", async () => {
    install("keys_ready");
    await renderTab();
    fireEvent.click(within(slotRow(3)).getByRole("button", { name: t.slots.actions.assign({ name: "Macro 4" }) }));
    const dialog = await screen.findByRole("dialog", { name: t.assign.title({ name: "Macro 4" }) });
    expect(within(dialog).getByRole("button", { name: t.assign.confirm })).toHaveProperty("disabled", true);
    chooseKey(dialog, "K");
    expect(within(dialog).getByText(t.editor.now({ action: "Default" }))).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: t.assign.confirm }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: t.assign.title({ name: "Macro 4" }) })).toBeNull();
    });
    expect(within(slotRow(3)).getByText(t.slots.draft.bound({ key: "K" }))).toBeTruthy();

    fireEvent.click(within(slotRow(2)).getByRole("button", { name: t.slots.actions.delete({ name: "Macro 3 · Type A" }) }));
    expect(within(slotRow(2)).getByText(t.slots.draft.deleted({ keys: "Fn + Q", n: 1 }))).toBeTruthy();
    expect(screen.getByText(t.apply.summary({ n: 2 }))).toBeTruthy();

    fireEvent.click(reviewButton());
    await waitFor(() => {
      expect(preparedIntents()).toHaveLength(1);
    });
    const k = recordedState(fixture("keys_ready")).keys.find((key) => key.key.name === "K")?.key.index;
    expect(preparedIntents()[0]?.changes).toEqual([
      { change: "delete_macro", slot: 2 },
      { change: "assign", layer: "base", key: k, action: { kind: "macro", slot: 3 } },
    ]);
    expect(preparedIntents()[0]?.based_on).toBe(recordedState(fixture("keys_ready")).revision);
    // This fixture recorded no plan: the flow shows the error, and closing it keeps the draft.
    fireEvent.click(await screen.findByRole("button", { name: en.common.actions.close }));
    expect(screen.getByText(t.apply.summary({ n: 2 }))).toBeTruthy();

    // Keep it / Undo changes.
    fireEvent.click(within(slotRow(2)).getByRole("button", { name: t.slots.actions.keep({ name: "Macro 3 · Type A" }) }));
    fireEvent.click(within(slotRow(3)).getByRole("button", { name: t.slots.actions.undo({ name: "Macro 4" }) }));
    expect(screen.queryByText(t.apply.summary({ n: 1 }))).toBeNull();
    fireEvent.click(within(slotRow(3)).getByRole("button", { name: t.slots.actions.delete({ name: "Macro 4" }) }));
    expect(within(slotRow(3)).getByText(t.slots.draft.removed)).toBeTruthy();
  });
});

describe("draft → review → write → report", () => {
  it("writes a new macro on Fn + R and offers Try it", async () => {
    const source = install("keys_apply_checked");
    await renderTab();
    fireEvent.click(screen.getByRole("button", { name: t.slots.newMacro }));
    const region = editor(t.slots.newRow({ n: 1 }));
    expect(document.activeElement).toBe(region);
    expect(within(region).getByText(t.checks.needsKey)).toBeTruthy();
    fireEvent.click(within(region).getByRole("radio", { name: en.keys.layers.fn }));
    chooseKey(region, "R");
    addTap(region, 0x05);
    expect(within(region).getByText("Tap B")).toBeTruthy();
    fireEvent.click(within(region).getByRole("radio", { name: t.editor.views.detailed }));
    fireEvent.change(within(region).getByLabelText(t.editor.stepDelay({ n: 1 })), { target: { value: "50" } });
    expect(within(region).getByLabelText(t.editor.stepDelay({ n: 2 }))).toHaveProperty("disabled", true);
    expect(screen.getByText(t.apply.summary({ n: 1 }))).toBeTruthy();
    expect(within(slotRow(10)).getByText(t.slots.draft.bound({ key: "Fn + R" }))).toBeTruthy();

    fireEvent.click(reviewButton());
    await screen.findByText(REVIEW_TITLE);
    const recorded = recordedIntent(source);
    expect(preparedIntents()[0]).toEqual({ ...recorded, check_persistence: false });
    // The option shows the plan's own state (this recorded plan was made with the check).
    expect(screen.getByRole("checkbox", { name: en.keys.review.persistence.check({ m: 2 }) })).toHaveProperty("checked", true);
    await pressWrite();
    await screen.findByText(en.flow.report.persistenceOk({ n: 2 }));
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.close }));

    // The draft is spent; Try it names the key.
    await screen.findByRole("heading", { name: t.tryIt.title });
    expect(screen.getByText(t.tryIt.body({ keys: "Fn + R" }))).toBeTruthy();
    expect(screen.getByText(t.tryIt.fnHint)).toBeTruthy();
    const field = screen.getByLabelText(t.tryIt.field);
    fireEvent.change(field, { target: { value: "b" } });
    fireEvent.click(screen.getByRole("button", { name: t.tryIt.clear }));
    expect(field).toHaveProperty("value", "");
    expect(screen.queryByText(t.apply.summary({ n: 1 }))).toBeNull();
    expect(macrosState().draft.added).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: t.tryIt.done }));
    expect(screen.queryByRole("heading", { name: t.tryIt.title })).toBeNull();
  });

  it("keeps the draft when a write stopped before anything was written", async () => {
    const recorded = fixture("keys_apply_checked").calls.find((c) => c.command === "apply_keymap_edit");
    if (recorded?.command !== "apply_keymap_edit" || recorded.ok === null) throw new Error("no recorded report");
    const report: EditReportView = { ...recorded.ok, outcome: "cancelled", headline: "cancelled", anything_written: false };
    install("keys_apply_checked", {
      apply_keymap_edit: () => report,
    });
    await renderTab();
    fireEvent.click(screen.getByRole("button", { name: t.slots.newMacro }));
    const region = editor(t.slots.newRow({ n: 1 }));
    fireEvent.click(within(region).getByRole("radio", { name: en.keys.layers.fn }));
    chooseKey(region, "R");
    addTap(region, 0x05);
    fireEvent.click(reviewButton());
    await screen.findByText(REVIEW_TITLE);
    await pressWrite();
    await waitFor(() => {
      expect(calls.filter((c) => c.command === "apply_keymap_edit")).toHaveLength(1);
    });
    fireEvent.click(await screen.findByRole("button", { name: en.common.actions.close }));
    // Nothing written: the new macro stays in the draft, and nothing is offered to try.
    expect(macrosState().draft.added).toHaveLength(1);
    expect(screen.getByText(t.apply.summary({ n: 1 }))).toBeTruthy();
    expect(screen.queryByRole("heading", { name: t.tryIt.title })).toBeNull();
  });

  it("writes Macro 1: the review explains the re-sent macros, a failed write lists the emptied ones", async () => {
    const source = install("keys_apply_failed_cleared");
    await renderTab();
    // An empty Macro 1 no key runs: Create… points to New macro… (Macro 1 empties the others).
    const create = within(slotRow(0)).getByRole("button", { name: t.slots.actions.create({ name: "Macro 1" }) });
    expect(create).toHaveProperty("disabled", true);
    expect(within(slotRow(0)).getByText(t.slots.slot0.createInstead)).toBeTruthy();
    expect(screen.getByText(t.slots.newMacroHint)).toBeTruthy();
    // A Macro 1 edit (e.g. one restored from an earlier draft) still writes through the review.
    act(() => {
      setTarget({ kind: "slot", slot: 0 });
    });
    const region = editor("Macro 1");
    expect(within(region).getByText(t.slots.slot0.note)).toBeTruthy();
    // A slot no key uses: the key is optional.
    expect(within(region).getByRole("checkbox", { name: t.editor.bindOptional })).toHaveProperty("checked", false);
    addTap(region, 0x06);
    fireEvent.click(reviewButton());
    await screen.findByText(REVIEW_TITLE);
    expect(preparedIntents()).toEqual([recordedIntent(source)]);

    const writes = screen.getByRole("heading", { name: en.keys.review.writesTitle }).parentElement as HTMLElement;
    const items = within(writes).getAllByRole("listitem");
    expect(items.map((item) => item.querySelector(".ow-strong")?.textContent)).toEqual(["Macro 1", "Macro 2", "Macro 3"]);
    expect(items[1]?.textContent).toContain(en.flow.resent.macro_slot0_clears);
    expect(within(writes).getByText(en.keys.review.commitNote.ownOnly)).toBeTruthy();

    await pressWrite();
    const cleared = await screen.findByText(en.flow.report.clearedTitle);
    expect(within(cleared.closest(".ow-callout") as HTMLElement).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "Macro 2",
      "Macro 3",
    ]);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.close }));
    // Something was written: the draft is spent; nothing to try after a failure.
    expect(screen.queryByText(t.apply.summary({ n: 1 }))).toBeNull();
    expect(screen.queryByRole("heading", { name: t.tryIt.title })).toBeNull();

    // The next read shows the keys left on the emptied slots.
    const status = statusStore.get();
    statusStore.set({ ...status, snapshot_revision: (status.snapshot_revision ?? 0) + 1 });
    await waitFor(() => {
      expect(within(slotRow(1)).getByText(t.slots.dangling({ keys: "Q", n: 1 }))).toBeTruthy();
    });
    expect(within(slotRow(2)).getByText(t.slots.dangling({ keys: "Fn + W", n: 1 }))).toBeTruthy();
    expect(within(slotRow(0)).getByText(t.slots.orphan)).toBeTruthy();
  });

  it("says when a new macro goes into Macro 1 because its key runs Macro 1 alone", async () => {
    // R runs Macro 1 on its own: the library reuses Macro 1 for a new macro on R (not a free slot).
    install("keys_preview_new_on_macro1");
    await renderTab();
    expect(within(slotRow(0)).getByText(t.slots.runsFrom({ keys: "R" }))).toBeTruthy();
    expect(screen.getByText(t.slots.newMacroHint)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: t.slots.newMacro }));
    const region = editor(t.slots.newRow({ n: 1 }));
    expect(within(region).queryByText(t.slots.slot0.reuses)).toBeNull();
    chooseKey(region, "R");
    expect(within(region).getByText(t.slots.slot0.reuses)).toBeTruthy();
    expect(within(slotRow(10)).getByText(t.slots.newSlotOwn({ n: 1 }))).toBeTruthy();
    // Another key: a free slot again.
    chooseKey(region, "E");
    expect(within(region).queryByText(t.slots.slot0.reuses)).toBeNull();
    expect(within(slotRow(10)).getByText(t.slots.newSlot)).toBeTruthy();
    chooseKey(region, "R");
    addTap(region, 0x0a);

    // The recorded plan: Macro 1 written, Macros 2 and 3 re-sent after it.
    fireEvent.click(reviewButton());
    await screen.findByText(REVIEW_TITLE);
    const writes = screen.getByRole("heading", { name: en.keys.review.writesTitle }).parentElement as HTMLElement;
    const items = within(writes).getAllByRole("listitem");
    expect(items.map((item) => item.querySelector(".ow-strong")?.textContent).slice(0, 3)).toEqual(["Macro 1", "Macro 2", "Macro 3"]);
    expect(items[1]?.textContent).toContain(en.flow.resent.macro_slot0_clears);
  });

  it("keeps the draft when the review is cancelled", async () => {
    install("keys_preview_slot0");
    await renderTab();
    act(() => {
      setTarget({ kind: "slot", slot: 0 });
    });
    addTap(editor("Macro 1"), 0x06);
    fireEvent.click(reviewButton());
    await screen.findByText(REVIEW_TITLE);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.cancel }));
    await waitFor(() => {
      expect(screen.queryByText(REVIEW_TITLE)).toBeNull();
    });
    expect(calls.some((c) => c.command === "discard_plan")).toBe(true);
    expect(calls.some((c) => c.command === "apply_keymap_edit")).toBe(false);
    expect(screen.getByText(t.apply.summary({ n: 1 }))).toBeTruthy();
    expect(within(slotRow(0)).getByText(t.slots.draft.changed)).toBeTruthy();
  });

  it("asks to confirm replacing Fn + 2, in the editor or in the review, and keeps the answer", async () => {
    install("keys_refused_critical");
    await renderTab();
    fireEvent.click(screen.getByRole("button", { name: t.slots.newMacro }));
    const region = editor(t.slots.newRow({ n: 1 }));
    fireEvent.click(within(region).getByRole("radio", { name: en.keys.layers.fn }));
    chooseKey(region, "2");
    const confirmLabel = en.keys.builtinConfirmProfile({ k: 2, n: 1 });
    expect(within(region).getByText(en.keys.builtinProfile({ k: 2, n: 1 }))).toBeTruthy();
    expect(within(region).getByRole("checkbox", { name: confirmLabel })).toHaveProperty("checked", false);
    addTap(region, 0x04);

    // Not confirmed: the library refuses and the review asks.
    fireEvent.click(reviewButton());
    await screen.findByText(
      en.errors.keymap.builtin_not_acknowledged_critical({ key: "Fn + 2 (#24)", function: "Profile 2" }),
    );
    expect(preparedIntents()[0]?.builtin_acks).toEqual([]);
    const dialog = screen.getByRole("dialog");
    fireEvent.click(within(dialog).getByRole("checkbox", { name: confirmLabel }));
    fireEvent.click(within(dialog).getByRole("button", { name: en.common.actions.close }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    // The answer is kept in the draft: the editor shows it, the next review sends it.
    expect(within(editor(t.slots.newRow({ n: 1 }))).getByRole("checkbox", { name: confirmLabel })).toHaveProperty("checked", true);
    fireEvent.click(reviewButton());
    await waitFor(() => {
      expect(preparedIntents()).toHaveLength(2);
    });
    expect(preparedIntents()[1]?.builtin_acks).toEqual([{ key: 24, critical: true }]);
    expect(preparedIntents()[1]?.changes).toEqual([
      {
        change: "bind_macro",
        layer: "fn",
        key: 24,
        program: { play: { mode: "once" }, steps: [
          { usage: 4, motion: "press", delay_after_ms: 10 },
          { usage: 4, motion: "release", delay_after_ms: 0 },
        ] },
        slot: null,
      },
    ]);
  });

  it("can't be opened without a keyboard, and says why", async () => {
    install("keys_ready");
    await renderTab();
    fireEvent.click(within(slotRow(3)).getByRole("button", { name: t.slots.actions.delete({ name: "Macro 4" }) }));
    expect(reviewButton()).toHaveProperty("disabled", false);
    act(() => {
      statusStore.set({ ...statusStore.get(), write_gate: { allowed: false, reasons: ["not_connected"] } });
    });
    expect(reviewButton()).toHaveProperty("disabled", true);
    expect(screen.getByText(en.errors.writeBlocked.not_connected)).toBeTruthy();
  });
});

describe("the editor", () => {
  it("checks the steps as they change and fixes a key left held with Release all at end", async () => {
    install("keys_ready");
    await renderTab();
    fireEvent.click(within(slotRow(2)).getByRole("button", { name: t.slots.actions.edit({ name: "Macro 3 · Type A" }) }));
    const region = editor("Macro 3 · Type A");
    fireEvent.click(within(region).getByRole("radio", { name: t.editor.views.detailed }));
    // Alt + Down moves the focused step; the focus moves with it.
    const position = en.common.pickers.steps.position;
    const first = within(region).getByRole("listitem", { name: `A down (${position({ n: 1, total: 2 })})` });
    first.focus();
    fireEvent.keyDown(first, { key: "ArrowDown", altKey: true });
    expect(document.activeElement?.getAttribute("aria-label")).toBe(`A down (${position({ n: 2, total: 2 })})`);
    expect(within(region).getByText(t.checks.released_unpressed({ n: 1, key: "A" }))).toBeTruthy();
    fireEvent.keyDown(document.activeElement as HTMLElement, { key: "ArrowUp", altKey: true });
    expect(within(region).queryByText(t.checks.title)).toBeNull();
    fireEvent.click(within(region).getByRole("button", { name: `${en.common.pickers.steps.remove}: A up` }));
    expect(within(region).getByText(t.checks.held({ keys: "A", n: 1 }))).toBeTruthy();
    expect(screen.getByText(t.checks.blocked({ name: "Macro 3 · Type A" }))).toBeTruthy();
    expect(reviewButton()).toHaveProperty("disabled", true);

    fireEvent.click(within(region).getByRole("button", { name: t.checks.releaseAll }));
    expect(within(region).queryByText(t.checks.held({ keys: "A", n: 1 }))).toBeNull();
    fireEvent.change(within(region).getByLabelText(t.editor.stepDelay({ n: 1 })), { target: { value: "70000" } });
    expect(within(region).getByText(t.checks.delay({ n: 1, max: 65535 }))).toBeTruthy();
    fireEvent.change(within(region).getByLabelText(t.editor.stepDelay({ n: 1 })), { target: { value: "30" } });
    // Back to the slot's own steps: no change left.
    expect(within(region).queryByText(t.checks.title)).toBeNull();
    expect(macrosState().draft.slots).toEqual([]);

    // Play modes: Several times asks for a count (1-255).
    fireEvent.click(within(region).getByRole("radio", { name: t.editor.play.times }));
    const times = within(region).getByLabelText(t.editor.times);
    expect(times).toHaveProperty("value", "2");
    fireEvent.change(times, { target: { value: "0" } });
    expect(within(region).getByText(t.checks.times({ max: 255 }))).toBeTruthy();
    fireEvent.change(times, { target: { value: "4" } });
    expect(macrosState().draft.slots[0]?.program?.play).toEqual({ mode: "times", count: 4 });
    expect(within(slotRow(2)).getByText("2 steps · Plays 4 times")).toBeTruthy();
  });

  it("records keys: repeats ignored, keys the keyboard can't send skipped, stops 2 s after the last key", async () => {
    install("keys_ready");
    await renderTab();
    fireEvent.click(within(slotRow(2)).getByRole("button", { name: t.slots.actions.edit({ name: "Macro 3 · Type A" }) }));
    const region = editor("Macro 3 · Type A");
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
    fireEvent.click(within(region).getByRole("button", { name: t.recorder.start }));
    const area = within(region).getByRole("application", { name: t.recorder.area });
    expect(document.activeElement).toBe(area);
    expect(within(region).getByText(t.recorder.recording)).toBeTruthy();
    expect(within(region).getByText(t.recorder.started)).toBeTruthy();
    fireEvent.keyDown(area, { code: "ShiftLeft", key: "Shift" });
    act(() => {
      vi.advanceTimersByTime(20);
    });
    fireEvent.keyDown(area, { code: "KeyA", key: "A" });
    fireEvent.keyDown(area, { code: "KeyA", key: "A", repeat: true });
    fireEvent.keyDown(area, { code: "AudioVolumeMute", key: "AudioVolumeMute" });
    fireEvent.keyUp(area, { code: "AudioVolumeMute", key: "AudioVolumeMute" });
    act(() => {
      vi.advanceTimersByTime(40);
    });
    fireEvent.keyUp(area, { code: "KeyA", key: "A" });
    // A key still held: the recording doesn't stop by itself.
    act(() => {
      vi.advanceTimersByTime(2500);
    });
    expect(within(region).getByRole("button", { name: t.recorder.stop })).toBeTruthy();
    fireEvent.keyUp(area, { code: "ShiftLeft", key: "Shift" });
    act(() => {
      vi.advanceTimersByTime(2100);
    });
    expect(within(region).getByText(t.recorder.stopped({ n: 4 }))).toBeTruthy();
    expect(within(region).getByText(t.recorder.skipped({ n: 1 }))).toBeTruthy();
    expect(document.activeElement).toBe(within(region).getByRole("button", { name: t.recorder.start }));
    vi.useRealTimers();
    expect(macrosState().draft.slots[0]?.program?.steps).toEqual([
      { usage: 0xe1, motion: "press", delay_after_ms: 20 },
      { usage: 0x04, motion: "press", delay_after_ms: 40 },
      { usage: 0x04, motion: "release", delay_after_ms: 2500 },
      { usage: 0xe1, motion: "release", delay_after_ms: 0 },
    ]);
    expect(within(region).getByText("Tap A")).toBeTruthy();
    expect(within(slotRow(2)).getByText(t.slots.draft.changed)).toBeTruthy();
  });
});

describe("the library", () => {
  it("saves, renames and deletes macros of this PC, and puts one on a key", async () => {
    install("keys_ready");
    await renderTab();
    fireEvent.click(within(slotRow(3)).getByRole("button", { name: t.slots.actions.save({ name: "Macro 4" }) }));
    const save = await screen.findByRole("dialog", { name: t.library.saveTitle });
    expect(within(save).getByText(t.library.nameEmpty)).toBeTruthy();
    fireEvent.change(within(save).getByLabelText(t.library.name), { target: { value: "  Triple C  " } });
    fireEvent.click(within(save).getByRole("button", { name: t.library.save }));
    await screen.findByText(t.library.saved({ name: "Triple C" }));
    const saveCall = calls.find((c) => c.command === "save_library_macro");
    const slot3 = recordedState(fixture("keys_ready")).macros[3]?.content;
    expect(saveCall?.args).toEqual({ name: "Triple C", program: slot3?.state === "program" ? slot3.program : null });
    // The keyboard's macro now shows its library name.
    await waitFor(() => {
      expect(within(slotRow(3)).getByText("Macro 4 · Triple C")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: t.library.actions.rename({ name: "Copy" }) }));
    const rename = await screen.findByRole("dialog", { name: t.library.renameTitle({ name: "Copy" }) });
    fireEvent.change(within(rename).getByLabelText(t.library.name), { target: { value: "Copy C" } });
    fireEvent.click(within(rename).getByRole("button", { name: t.library.rename }));
    await screen.findByText(t.library.renamed({ name: "Copy C" }));
    expect(calls.find((c) => c.command === "rename_library_macro")?.args).toEqual({ hash: "a6859060ece5c9a3", name: "Copy C" });

    fireEvent.click(screen.getByRole("button", { name: t.library.actions.delete({ name: "Type A" }) }));
    const confirm = await screen.findByRole("dialog", { name: t.library.deleteTitle({ name: "Type A" }) });
    await sleep(560);
    fireEvent.click(within(confirm).getByRole("button", { name: t.library.deleteConfirm }));
    await screen.findByText(t.library.deleted({ name: "Type A" }));
    expect(calls.find((c) => c.command === "delete_library_macro")?.args).toEqual({ hash: "46250859bd68ec3d" });
    await waitFor(() => {
      expect(within(slotRow(2)).getByText("Macro 3")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: t.library.actions.putOnKey({ name: "Copy C" }) }));
    const region = editor(t.slots.newRow({ n: 1 }));
    expect(within(region).getByText("Tap C")).toBeTruthy();
    expect(within(region).getByText(t.checks.needsKey)).toBeTruthy();
    // Nothing was written to the keyboard.
    expect(calls.some((c) => c.command === "prepare_keymap_edit" || c.command === "apply_keymap_edit")).toBe(false);
  });
});

describe("the draft", () => {
  it("is discarded with a toast when the keyboard is on another profile", async () => {
    install("keys_ready");
    await renderTab();
    fireEvent.click(within(slotRow(3)).getByRole("button", { name: t.slots.actions.delete({ name: "Macro 4" }) }));
    expect(macrosState().draft.slots).toHaveLength(1);
    // The shell knows: the tab's dot and "Keys > Macros: 1 unwritten change".
    expect(draftEntries()).toContainEqual({ page: "keys", tab: "macros", count: 1 });
    const keys = keysStore.get();
    if (keys.state === null) throw new Error("no keys state");
    act(() => {
      keysStore.set({ ...keys, state: { ...keys.state, profile: 2 } as KeysState });
    });
    expect(macrosState().draft.slots).toEqual([]);
    expect(getToasts().map((toast) => toast.title)).toContain(t.draftDiscarded({ n: 2 }));
    expect(draftEntries().filter((entry) => entry.page === "keys")).toEqual([]);
  });

  it("speaks Turkish", async () => {
    setLang("tr");
    install("keys_ready");
    render(<KeysPage />);
    const tr = DICTIONARIES.tr.keysMacros;
    await screen.findByRole("heading", { name: tr.slots.title({ n: 1 }) });
    expect(screen.getByRole("heading", { name: tr.library.title })).toBeTruthy();
    expect(screen.getByText(tr.slots.orphan)).toBeTruthy();
    expect(screen.getByRole("button", { name: tr.slots.newMacro })).toBeTruthy();
  });
});
