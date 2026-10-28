// Keys > Advanced keys on the mocked IPC: the preview's mock backend answers
// with the fixtures the core's tests recorded from the real worker on the
// simulator (keys_*), through the real `api`, `invoke` and `Channel`.
//
// Covered: the DKS grammar taken from the catalogue (every grammar-valid row
// reachable in the grid, nothing else), the UI <-> intent round trip of
// every grammar-valid DKS row, the draft (one change per slot, delete =
// unbind + clear in one change, profile changes), the sentences (EN, TR),
// the evidence badges (from the catalogue and the DTOs only), the cards of
// the recorded state, and the flows: draft -> review -> write -> report
// (the intent equals the recorded one), cancel, refusals (no free slot, a
// critical built-in confirmed in the editor), a failed write, and a
// disconnected keyboard.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ACTION_CATALOG, usageName } from "../../../components/actions";
import { ToastRegion } from "../../../components/Toasts";
import { DICTIONARIES, setLang } from "../../../i18n";
import type {
  AdvancedDefView,
  AppError,
  DksStepView,
  KeymapEditIntent,
  KeymapEditPreview,
  KeysState,
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
import { buildKeymapIntent, keysStore, resetKeysStore } from "../shared";
import { AdvancedEditor, type EditorResult } from "./AdvancedEditor";
import { advancedStore, draftChange, ownerOf, resetAdvancedStore } from "./draftStore";
import {
  addItem,
  allowedSteps,
  changesOf,
  cycleStep,
  defSentence,
  DKS_STEPS,
  dksEvidence,
  EMPTY_DRAFT,
  holdTimeEvidence,
  layerBindingEvidence,
  removeItem,
  rowProblem,
  rowRule,
  setStep,
} from "./model";

const en = DICTIONARIES.en;
/** The key-assignment review's title on profile 1 (its words follow what the plan changes). */
const REVIEW_TITLE = /^Change .+ on profile 1\?$/u;
const tr = DICTIONARIES.tr;

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

function recordedState(source: Fixture): KeysState {
  const call = source.calls.find((c) => c.command === "get_keys_state");
  if (call?.command !== "get_keys_state" || call.ok === null) throw new Error(`no keys state in ${source.id}`);
  return call.ok;
}

function recordedIntent(source: Fixture): KeymapEditIntent {
  const call = source.calls.find((c) => c.command === "prepare_keymap_edit");
  if (call?.command !== "prepare_keymap_edit") throw new Error(`no prepare in ${source.id}`);
  return call.args.intent;
}

function recordedPreview(source: Fixture): KeymapEditPreview {
  const call = source.calls.find((c) => c.command === "prepare_keymap_edit");
  if (call?.command !== "prepare_keymap_edit" || call.ok === null) throw new Error(`no plan in ${source.id}`);
  return call.ok;
}

type Override = (args: Record<string, unknown>) => unknown;

/**
 * Installs the mock backend for a fixture (optionally patched) and puts its
 * status into the store; `overrides` answer some commands from elsewhere
 * (another fixture's recording, or an error).
 */
function install(id: string, overrides: Record<string, Override> = {}, patch?: (source: Fixture) => void): Fixture {
  const source = fixture(id);
  patch?.(source);
  const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
  mockIPC((command, args) => {
    const record = (args ?? {}) as Record<string, unknown>;
    calls.push({ command, args: record });
    const override = overrides[command];
    return override === undefined ? backend.handle(command, args) : override(record);
  });
  statusStore.set({ ...source.initial_status, write_gate: { allowed: true, reasons: [] }, busy: null });
  appInfoStore.set(source.app_info);
  return source;
}

/** Answers `apply_keymap_edit` with a fixture's recorded write (keys_apply_ok: `done`). */
function recordedApply(id = "keys_apply_ok"): Override {
  const backend = createMockBackend(fixture(id), { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
  return (args) => backend.handle("apply_keymap_edit", args);
}

function preparedIntents(): KeymapEditIntent[] {
  return calls.filter((c) => c.command === "prepare_keymap_edit").map((c) => c.args["intent"] as KeymapEditIntent);
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

async function pressWrite(): Promise<void> {
  const write = await screen.findByRole("button", { name: en.common.actions.writeToKeyboard });
  await sleep(560);
  fireEvent.click(write);
}

/** Focuses an element the way a user does (focus and focusin, as React listens to). */
function focus(element: HTMLElement): void {
  act(() => {
    element.focus();
  });
}

function renderTab(): void {
  window.location.hash = "#/keys?tab=advanced";
  render(
    <>
      <KeysPage />
      <ToastRegion />
    </>,
  );
}

/** The recorded DKS of keys_preview_advanced (E: tap at 1 and 4, R: hold at 2; 1.2 mm). */
function recordedDks(): Extract<AdvancedDefView, { type: "dks" }> {
  const change = recordedIntent(fixture("keys_preview_advanced")).changes[0];
  if (change?.change !== "bind_advanced" || change.def.type !== "dks") throw new Error("no recorded DKS");
  return change.def;
}

/** Every step sequence a user can build cell by cell (each stage from what the grid offers), except all-Nothing. */
function reachableRows(): DksStepView[][] {
  const rows: DksStepView[][] = [];
  const walk = (prefix: DksStepView[]) => {
    if (prefix.length === 4) {
      if (prefix.some((step) => step !== "none")) rows.push(prefix);
      return;
    }
    const none: DksStepView[] = ["none", "none", "none", "none"];
    const padded: DksStepView[] = [...prefix, ...none].slice(0, 4);
    for (const step of allowedSteps(padded, prefix.length)) {
      walk([...prefix, step]);
    }
  };
  walk([]);
  return rows;
}

const E = 0x08;
const R = 0x15;

beforeEach(() => {
  setLang("en");
  resetSession();
  acknowledgeVendor();
  resetKeysStore();
  resetAdvancedStore();
  calls = [];
});

afterEach(() => {
  clearMocks();
  clearToasts();
  statusStore.set(INITIAL_STATUS);
  appInfoStore.set(DEFAULT_APP_INFO);
  resetKeysStore();
  resetAdvancedStore();
  window.location.hash = "";
});

describe("the DKS grammar", () => {
  it("comes from the catalogue: every grammar-valid row and nothing else can be built in the grid", () => {
    const catalogue = ACTION_CATALOG.dks_rows.map((rule) => rule.steps.join("/")).sort();
    expect(catalogue.length).toBeGreaterThan(0);
    expect(reachableRows().map((steps) => steps.join("/")).sort()).toEqual(catalogue);
    for (const rule of ACTION_CATALOG.dks_rows) {
      expect(rowProblem(rule.steps)).toBeNull();
      expect(rowRule(rule.steps)?.evidence).toEqual(rule.evidence);
    }
  });

  it("allows only Nothing or Tap at stage 4, and Let go and press again only after a hold", () => {
    for (const steps of reachableRows()) expect(allowedSteps(steps, 3)).toEqual(["none", "tap"]);
    expect(allowedSteps(["tap", "none", "none", "none"], 1)).toEqual(["none", "tap", "hold"]);
    expect(allowedSteps(["hold", "none", "none", "none"], 1)).toEqual(["none", "tap", "hold", "hold_again"]);
    expect(allowedSteps(["none", "hold", "none", "none"], 2)).toContain("hold_again");
    expect(allowedSteps(["none", "tap", "none", "none"], 2)).not.toContain("hold_again");
    expect(rowProblem(["none", "none", "none", "none"])).toBe("empty");
    expect(rowProblem(["tap", "hold_again", "none", "none"])).toBe("invalid");
    expect(rowProblem(["none", "none", "none", "hold"])).toBe("invalid");
  });

  it("repairs later stages that a change made invalid, and cycles through the allowed steps", () => {
    expect(setStep(["hold", "hold_again", "hold_again", "tap"], 0, "tap")).toEqual(["tap", "hold", "hold_again", "tap"]);
    expect(setStep(["hold", "hold_again", "none", "none"], 0, "none")).toEqual(["none", "hold", "none", "none"]);
    expect(cycleStep(["tap", "none", "none", "none"], 3)).toBe("tap");
    expect(cycleStep(["tap", "none", "none", "tap"], 3)).toBe("none");
    expect(cycleStep(["hold", "hold", "none", "none"], 1)).toBe("hold_again");
    expect(cycleStep(["hold", "hold_again", "none", "none"], 1)).toBe("none");
  });

  it("takes the evidence from the catalogue and the library's rules", () => {
    expect(dksEvidence(recordedDks().rows)).toEqual({
      status: "unverified",
      notes: [{ question: "Q19", what: "DKS tap at stage 4" }],
    });
    expect(dksEvidence([{ key: E, steps: ["tap", "none", "none", "none"] }, null, null, null])).toEqual({ status: "confirmed" });
    expect(holdTimeEvidence(ACTION_CATALOG.mod_tap.default_ms)).toEqual({ status: "confirmed" });
    expect(holdTimeEvidence(310)).toEqual(ACTION_CATALOG.mod_tap.other_evidence);
    expect(holdTimeEvidence(310).status).toBe("unverified");
    expect(layerBindingEvidence("base")?.status).toBe("confirmed");
    expect(layerBindingEvidence("fn")?.status).toBe("unverified");
  });
});

describe("the draft", () => {
  const def = recordedDks();
  it("keeps one change per slot and one new advanced key per key", () => {
    let draft = addItem(EMPTY_DRAFT, { id: "a", kind: "edit", slot: 1, def });
    draft = addItem(draft, { id: "b", kind: "delete", slot: 1 });
    expect(changesOf(draft)).toEqual([{ change: "delete_advanced", slot: 1 }]);
    draft = addItem(draft, { id: "c", kind: "create", layer: "base", key: 47, def });
    draft = addItem(draft, { id: "d", kind: "create", layer: "base", key: 47, def: { type: "toggle", key: E } });
    draft = addItem(draft, { id: "e", kind: "assign", slot: 3, layer: "fn", key: 46 });
    expect(changesOf(draft)).toEqual([
      { change: "delete_advanced", slot: 1 },
      { change: "bind_advanced", layer: "base", key: 47, def: { type: "toggle", key: E }, slot: null },
      { change: "assign", layer: "fn", key: 46, action: { kind: "advanced", slot: 3 } },
    ]);
    expect(changesOf(removeItem(draft, "b"))).toHaveLength(2);
  });

  it("sends the deletes first, so a new key drafted before a delete of its old slot survives it", () => {
    // G uses slot 1: a new DKS on G, then Delete on the slot-1 card (the order the user made them).
    let draft = addItem(EMPTY_DRAFT, { id: "a", kind: "create", layer: "base", key: 72, def });
    draft = addItem(draft, { id: "b", kind: "assign", slot: 3, layer: "base", key: 73 });
    draft = addItem(draft, { id: "c", kind: "delete", slot: 1 });
    expect(draft.items.map((i) => i.kind)).toEqual(["create", "assign", "delete"]);
    expect(changesOf(draft).map((c) => c.change)).toEqual(["delete_advanced", "bind_advanced", "assign"]);
  });

  it("re-editing a new advanced key keeps its place", () => {
    let draft = addItem(EMPTY_DRAFT, { id: "a", kind: "create", layer: "base", key: 47, def });
    draft = addItem(draft, { id: "b", kind: "delete", slot: 2 });
    draft = addItem(draft, { id: "c", kind: "create", layer: "base", key: 48, def }, "a");
    expect(draft.items.map((i) => i.id)).toEqual(["c", "b"]);
  });

  it("is discarded with a toast when the keyboard is on another profile", () => {
    const state = recordedState(fixture("keys_ready"));
    draftChange(ownerOf(state), { kind: "delete", slot: 0 });
    keysStore.set({ state, last: state, fetchedFor: 1, loading: false, error: null });
    expect(advancedStore().draft.items).toHaveLength(1);
    expect(draftEntries()).toContainEqual({ page: "keys", tab: "advanced", count: 1 });
    keysStore.set({ state: { ...state, profile: 2 }, last: null, fetchedFor: 2, loading: false, error: null });
    expect(advancedStore().draft.items).toHaveLength(0);
    expect(draftEntries().some((entry) => entry.page === "keys" && entry.tab === "advanced")).toBe(false);
    expect(getToasts().map((t) => t.title)).toContain(en.keysAdvanced.draft.discarded({ n: 2 }));
  });
});

describe("the sentences", () => {
  const list = (items: readonly string[]) => new Intl.ListFormat("en-GB", { type: "conjunction" }).format(items);
  const trList = (items: readonly string[]) => new Intl.ListFormat("tr-TR", { type: "conjunction" }).format(items);

  it("say what a DKS does, stage by stage", () => {
    expect(defSentence(recordedDks(), en, "en", list)).toBe(
      "Light press (1.2 mm): taps E. Full press: holds R until the next point. Letting go (1.2 mm): taps E.",
    );
    expect(defSentence(recordedDks(), tr, "tr", trList)).toBe(
      "Hafif basış (1,2 mm): E tuşuna bir kez basar. Tam basış: R tuşunu sonraki noktaya kadar basılı tutar. Bırakırken (1,2 mm): E tuşuna bir kez basar.",
    );
    expect(defSentence({ type: "dks", first_point: 15, rows: [null, null, null, null] }, en, "en", list)).toBe(
      en.keysAdvanced.dks.sentence.nothing,
    );
  });

  it("say what a tap-or-hold and a toggle do", () => {
    expect(defSentence({ type: "mod_tap", tap: 0x0a, hold: 0xe0, hold_ms: 300 }, en, "en", list)).toBe(
      en.keysAdvanced.mt.sentence({ tap: "G", hold: usageName(0xe0, en), ms: "300 ms" }),
    );
    expect(defSentence({ type: "toggle", key: 0x0b }, en, "en", list)).toBe(en.keysAdvanced.toggle.sentence({ key: "H" }));
  });
});

describe("the DKS grid: UI <-> intent for every grammar-valid row", () => {
  const state = recordedState(fixture("keys_ready"));

  function gridRow(position = 0): HTMLElement {
    const grid = screen.getByRole("grid");
    const rows = within(grid).getAllByRole("row");
    const row = rows[position + 1];
    if (row === undefined) throw new Error("no row");
    return row;
  }

  function cells(row: HTMLElement): HTMLButtonElement[] {
    return [...row.querySelectorAll<HTMLButtonElement>("[data-step]")];
  }

  it("shows each recorded row exactly and writes it back unchanged", () => {
    const onSave = vi.fn<(result: EditorResult) => void>();
    const { rerender } = render(<div />);
    ACTION_CATALOG.dks_rows.forEach((rule, index) => {
      const def: AdvancedDefView = { type: "dks", first_point: 15, rows: [{ key: E, steps: [...rule.steps] }, null, null, null] };
      rerender(
        <AdvancedEditor
          key={index}
          state={state}
          request={{ mode: "edit", slot: 0, def, itemId: null }}
          onSave={onSave}
          onClose={vi.fn()}
        />,
      );
      const row = gridRow();
      expect(cells(row).map((cell) => cell.dataset["step"])).toEqual(rule.steps);
      expect(cells(row).map((cell) => cell.getAttribute("aria-label"))).toEqual(
        rule.steps.map((step, stage) =>
          en.keysAdvanced.dks.cellLabel({
            key: "E",
            stage: Object.values(en.keysAdvanced.dks.stages)[stage] ?? "",
            step: en.keysAdvanced.dks.steps[step],
          }),
        ),
      );
      const header = within(row).getByRole("rowheader");
      expect(within(header).queryByText(en.common.badge.unverified) !== null).toBe(rule.evidence.status === "unverified");
      fireEvent.click(screen.getByRole("button", { name: en.keysAdvanced.editor.update }));
      const saved = onSave.mock.calls.at(-1)?.[0];
      expect(saved?.def).toEqual(def);
      const intent = buildKeymapIntent(changesOf({ items: [{ id: "x", kind: "edit", slot: 0, def }], acks: [] }), state);
      expect(intent?.changes).toEqual([{ change: "set_advanced", slot: 0, def }]);
    });
    expect(onSave).toHaveBeenCalledTimes(ACTION_CATALOG.dks_rows.length);
  });

  it("builds every grammar-valid row cell by cell from the step strip", () => {
    const onSave = vi.fn<(result: EditorResult) => void>();
    const start: AdvancedDefView = { type: "dks", first_point: 15, rows: [{ key: E, steps: ["tap", "none", "none", "none"] }, null, null, null] };
    render(
      <AdvancedEditor state={state} request={{ mode: "edit", slot: 0, def: start, itemId: null }} onSave={onSave} onClose={vi.fn()} />,
    );
    for (const rule of ACTION_CATALOG.dks_rows) {
      rule.steps.forEach((step, stage) => {
        const cell = cells(gridRow())[stage];
        if (cell === undefined) throw new Error("no cell");
        focus(cell);
        const option = screen.getByRole("radio", { name: en.keysAdvanced.dks.steps[step] });
        expect(option).toHaveProperty("disabled", false);
        fireEvent.click(option);
      });
      expect(cells(gridRow()).map((cell) => cell.dataset["step"])).toEqual(rule.steps);
      fireEvent.click(screen.getByRole("button", { name: en.keysAdvanced.editor.update }));
      const def = onSave.mock.calls.at(-1)?.[0].def;
      expect(def).toEqual({ type: "dks", first_point: 15, rows: [{ key: E, steps: rule.steps }, null, null, null] });
    }
    // Every row is built through role queries, which jsdom answers slowly under a full parallel run.
  }, 30_000);

  it("is one Tab stop: the arrow keys, Home and End move the focus; a click changes a cell", () => {
    const def: AdvancedDefView = {
      type: "dks",
      first_point: 15,
      rows: [{ key: E, steps: ["tap", "none", "none", "none"] }, { key: R, steps: ["none", "tap", "none", "none"] }, null, null],
    };
    render(<AdvancedEditor state={state} request={{ mode: "edit", slot: 0, def, itemId: null }} onSave={vi.fn()} onClose={vi.fn()} />);
    const grid = screen.getByRole("grid");
    const stops = () => [...grid.querySelectorAll<HTMLElement>("button")].filter((b) => b.tabIndex === 0);
    expect(stops()).toHaveLength(1);
    const first = cells(gridRow(0))[0];
    if (first === undefined) throw new Error("no cell");
    focus(first);
    fireEvent.keyDown(first, { key: "ArrowRight" });
    expect(document.activeElement).toBe(cells(gridRow(0))[1]);
    fireEvent.keyDown(document.activeElement as Element, { key: "ArrowDown" });
    expect(document.activeElement).toBe(cells(gridRow(1))[1]);
    expect(stops()).toEqual([document.activeElement]);
    fireEvent.keyDown(document.activeElement as Element, { key: "Home" });
    expect(document.activeElement?.getAttribute("aria-label")).toBe(en.keysAdvanced.dks.rowKey({ key: "R" }));
    fireEvent.keyDown(document.activeElement as Element, { key: "End" });
    expect(document.activeElement?.getAttribute("aria-label")).toBe(en.keysAdvanced.dks.removeRow({ key: "R" }));
    // A click (as Space and Enter do) moves to the next step the keyboard accepts there.
    const r2 = cells(gridRow(1))[1];
    if (r2 === undefined) throw new Error("no cell");
    fireEvent.click(r2);
    expect(cells(gridRow(1))[1]?.dataset["step"]).toBe("hold");
    // The live sentence follows.
    expect(screen.getByText(/Full press: holds R until the next point\./)).toBeTruthy();
  });
});

describe("the Advanced keys tab", () => {
  it("shows a card per used slot, with orphans and kinds kept as read", async () => {
    install("keys_ready");
    renderTab();
    await screen.findByText(en.keys.slot.used({ n: 4, total: 40 }));
    const dks = screen.getByRole("article", { name: "Advanced key 1 · Dynamic keystroke (DKS)" });
    expect(within(dks).getByText("Light press (1.5 mm): taps E. Full press: taps R.")).toBeTruthy();
    expect(within(dks).queryByText(en.common.badge.unverified)).toBeNull();
    expect(dks.textContent).toContain(en.keysAdvanced.card.usedBy);
    const mt = screen.getByRole("article", { name: "Advanced key 2 · Tap or hold" });
    expect(mt.textContent).toContain(en.keysAdvanced.mt.sentence({ tap: "G", hold: usageName(0xe0, en), ms: "300 ms" }));
    screen.getByRole("article", { name: "Advanced key 3 · Toggle" });
    const kept = screen.getByRole("article", { name: "Advanced key 6" });
    expect(kept.textContent).toContain(en.common.badge.kept);
    expect(kept.textContent).toContain(en.keysAdvanced.card.kept({ type: 4 }));
    expect(kept.textContent).toContain(en.keysAdvanced.card.orphan);
    // A kind OpenWraith can't read can be deleted, never edited or assigned.
    expect(within(kept).queryByRole("button", { name: en.keysAdvanced.card.editNamed({ name: "Advanced key 6" }) })).toBeNull();
    expect(within(kept).getByRole("button", { name: en.keysAdvanced.card.deleteNamed({ name: "Advanced key 6" }) })).toBeTruthy();
    // Nothing to write yet.
    expect(screen.getByText(en.keysAdvanced.draft.idle)).toBeTruthy();
    expect(screen.getByRole("button", { name: en.common.actions.reviewAndWrite })).toHaveProperty("disabled", true);
  });

  it("draft -> review -> write -> report: a new DKS sends the recorded intent and spends the draft", async () => {
    const source = install("keys_preview_advanced", { apply_keymap_edit: recordedApply("keys_apply_advanced") });
    renderTab();
    await screen.findByText(en.keysAdvanced.list.empty);
    fireEvent.click(screen.getByRole("button", { name: en.keysAdvanced.editor.titleNew({ type: en.keys.advancedTypes.dks }) }));
    await screen.findByRole("heading", { name: en.keysAdvanced.editor.titleNew({ type: en.keys.advancedTypes.dks }) });
    // Without a key it can't go into the draft.
    expect(screen.getByText(en.keysAdvanced.editor.needTarget)).toBeTruthy();

    fireEvent.change(screen.getByRole("combobox", { name: en.keysAdvanced.editor.target }), { target: { value: "E" } });
    fireEvent.click(screen.getByRole("option", { name: "E" }));
    // The first row follows the chosen key.
    expect(screen.getByRole("button", { name: en.keysAdvanced.dks.rowKey({ key: "E" }) })).toBeTruthy();
    fireEvent.change(screen.getByRole("slider", { name: en.keysAdvanced.dks.firstPoint }), { target: { value: "12" } });
    fireEvent.click(screen.getByRole("button", { name: "E, 4 Release: Nothing" }));

    fireEvent.click(screen.getByRole("button", { name: en.keysAdvanced.dks.addRow }));
    await screen.findByRole("heading", { name: en.keysAdvanced.editor.pickTitle({ what: en.keysAdvanced.dks.rowKeyWhat({ n: 2 }) }) });
    fireEvent.click(screen.getByRole("option", { name: "R" }));
    focus(screen.getByRole("button", { name: "R, 1 Press: Tap" }));
    fireEvent.click(screen.getByRole("radio", { name: en.keysAdvanced.dks.steps.none }));
    focus(screen.getByRole("button", { name: "R, 2 Bottom: Nothing" }));
    fireEvent.click(screen.getByRole("radio", { name: en.keysAdvanced.dks.steps.hold }));

    expect(
      screen.getByText(
        "Light press (1.2 mm): taps E. Full press: holds R until the next point. Letting go (1.2 mm): taps E.",
      ),
    ).toBeTruthy();
    // Tap at stage 4 is not verified yet (the catalogue's rule for E's row).
    expect(within(screen.getByRole("grid")).getAllByText(en.common.badge.unverified)).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: en.keysAdvanced.editor.add }));

    const card = await screen.findByRole("article", { name: en.keys.advancedTypes.dks });
    expect(card.textContent).toContain(en.keysAdvanced.card.newOn({ key: "E" }));
    expect(screen.getByText(`· ${en.keysAdvanced.draft.count({ n: 1 })}`, { exact: false })).toBeTruthy();
    expect(draftEntries()).toContainEqual({ page: "keys", tab: "advanced", count: 1 });

    fireEvent.click(screen.getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    expect(preparedIntents()).toEqual([recordedIntent(source)]);
    expect(screen.getByText(en.keys.review.warnings.unverified({ what: en.common.badge.notes["DKS tap at stage 4"] ?? "" }))).toBeTruthy();

    await pressWrite();
    await screen.findByText(en.flow.footer.safetyBackup);
    await waitFor(() => {
      expect(advancedStore().draft.items).toHaveLength(0);
    });
    expect(draftEntries().some((entry) => entry.page === "keys" && entry.tab === "advanced")).toBe(false);
    expect(calls.filter((c) => c.command === "apply_keymap_edit")).toHaveLength(1);
  });

  it("cancel in the review writes nothing and keeps the draft", async () => {
    const source = install("keys_preview_advanced");
    const state = recordedState(source);
    draftChange(ownerOf(state), { kind: "create", layer: "base", key: 47, def: recordedDks() });
    renderTab();
    fireEvent.click(await screen.findByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.cancel }));
    await waitFor(() => {
      expect(screen.queryByText(REVIEW_TITLE)).toBeNull();
    });
    expect(calls.some((c) => c.command === "apply_keymap_edit")).toBe(false);
    expect(calls.some((c) => c.command === "discard_plan")).toBe(true);
    expect(advancedStore().draft.items).toHaveLength(1);
  });

  it("delete = unbind + clear in one change; Keep it takes it back", async () => {
    const preview = recordedPreview(fixture("keys_preview_advanced"));
    install("keys_ready", { prepare_keymap_edit: () => preview });
    renderTab();
    const name = "Advanced key 1 · Dynamic keystroke (DKS)";
    const card = await screen.findByRole("article", { name });
    fireEvent.click(within(card).getByRole("button", { name: en.keysAdvanced.card.deleteNamed({ name }) }));
    expect(card.textContent).toContain(en.keysAdvanced.card.tags.deleted);
    expect(card.textContent).toContain(en.keysAdvanced.card.deleteNote({ keys: "E" }));
    fireEvent.click(within(card).getByRole("button", { name: en.keysAdvanced.card.keepNamed({ name }) }));
    expect(card.textContent).not.toContain(en.keysAdvanced.card.tags.deleted);
    fireEvent.click(within(card).getByRole("button", { name: en.keysAdvanced.card.deleteNamed({ name }) }));
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    expect(preparedIntents()[0]?.changes).toEqual([{ change: "delete_advanced", slot: 0 }]);
  });

  it("edits a slot's definition (Tap or hold: the hold time's evidence) into one set_advanced change", async () => {
    const preview = recordedPreview(fixture("keys_preview_advanced"));
    install("keys_ready", { prepare_keymap_edit: () => preview });
    renderTab();
    const name = "Advanced key 2 · Tap or hold";
    const card = await screen.findByRole("article", { name });
    fireEvent.click(within(card).getByRole("button", { name: en.keysAdvanced.card.editNamed({ name }) }));
    await screen.findByRole("heading", { name: en.keysAdvanced.editor.titleEdit({ name }) });
    const slider = screen.getByRole("slider", { name: new RegExp(en.keysAdvanced.mt.holdTime) });
    expect(screen.queryByText(en.common.badge.unverified)).toBeNull();
    fireEvent.change(slider, { target: { value: "310" } });
    expect(screen.getAllByText(en.common.badge.unverified).length).toBeGreaterThan(0);
    expect(screen.getByText(en.keysAdvanced.mt.sentence({ tap: "G", hold: usageName(0xe0, en), ms: "310 ms" }))).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: en.keysAdvanced.mt.useConfirmed({ ms: "300 ms" }) }));
    expect(screen.queryByText(en.common.badge.unverified)).toBeNull();
    fireEvent.change(slider, { target: { value: "450" } });
    fireEvent.click(screen.getByRole("button", { name: en.keysAdvanced.editor.update }));
    expect(card.textContent).toContain(en.keysAdvanced.card.tags.changed);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    expect(preparedIntents()[0]?.changes).toEqual([
      { change: "set_advanced", slot: 1, def: { type: "mod_tap", tap: 0x0a, hold: 0xe0, hold_ms: 450 } },
    ]);
  });

  it("assigns a slot no key uses to a key", async () => {
    const preview = recordedPreview(fixture("keys_preview_advanced"));
    install("keys_ready", { prepare_keymap_edit: () => preview }, (source) => {
      const state = recordedState(source);
      const toggle = state.advanced[2];
      if (toggle === undefined) throw new Error("no slot 2");
      toggle.bindings = [];
      toggle.orphan = true;
    });
    renderTab();
    const name = "Advanced key 3 · Toggle";
    const card = await screen.findByRole("article", { name });
    expect(card.textContent).toContain(en.keysAdvanced.card.orphan);
    fireEvent.click(within(card).getByRole("button", { name: en.keysAdvanced.card.assignNamed({ name }) }));
    await screen.findByRole("heading", { name: en.keysAdvanced.assign.title({ name }) });
    fireEvent.change(screen.getByRole("combobox", { name: en.keysAdvanced.editor.target }), { target: { value: "J" } });
    fireEvent.click(screen.getByRole("option", { name: "J" }));
    fireEvent.click(screen.getByRole("button", { name: en.keysAdvanced.assign.confirm }));
    expect(card.textContent).toContain(en.keysAdvanced.card.tags.assigned({ key: "J" }));
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    expect(preparedIntents()[0]?.changes).toEqual([
      { change: "assign", layer: "base", key: 74, action: { kind: "advanced", slot: 2 } },
    ]);
  });

  it("asks for the explicit confirmation before an advanced key replaces Fn + 2", async () => {
    const preview = recordedPreview(fixture("keys_preview_advanced"));
    install("keys_ready", { prepare_keymap_edit: () => preview });
    renderTab();
    fireEvent.click(
      await screen.findByRole("button", { name: en.keysAdvanced.editor.titleNew({ type: en.keys.advancedTypes.toggle }) }),
    );
    fireEvent.click(screen.getByRole("radio", { name: en.keys.layers.fn }));
    // Advanced keys on the Fn layer aren't verified yet (the catalogue's binding rule).
    expect(screen.getAllByText(en.common.badge.unverified).length).toBeGreaterThan(0);
    fireEvent.change(screen.getByRole("combobox", { name: en.keysAdvanced.editor.target }), { target: { value: "2" } });
    fireEvent.click(screen.getByRole("option", { name: "2" }));
    expect(screen.getByText(en.keys.builtinProfile({ k: 2, n: 1 }))).toBeTruthy();
    const add = screen.getByRole("button", { name: en.keysAdvanced.editor.add });
    expect(add).toHaveProperty("disabled", true);
    expect(screen.getByText(en.keysAdvanced.editor.needConfirm)).toBeTruthy();
    const confirm = screen.getByRole("checkbox", { name: en.keys.builtinConfirmProfile({ k: 2, n: 1 }) });
    expect(confirm).toHaveProperty("checked", false);
    fireEvent.click(confirm);
    fireEvent.click(screen.getByRole("button", { name: en.keysAdvanced.editor.add }));
    expect(advancedStore().draft.acks).toEqual([{ key: 24, critical: true }]);
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    const intent = preparedIntents()[0];
    expect(intent?.builtin_acks).toEqual([{ key: 24, critical: true }]);
    expect(intent?.changes).toEqual([
      { change: "bind_advanced", layer: "fn", key: 24, def: { type: "toggle", key: 0x1f }, slot: null },
    ]);
  });

  it("shows a refusal and writes nothing", async () => {
    const refusal: AppError = {
      kind: "invalid_request",
      detail: { detail: "keymap", problem: { problem: "no_free_slot", kind: "advanced" } },
      message: "no free advanced-key slot",
    };
    const source = install("keys_preview_advanced", {
      prepare_keymap_edit: () => {
        throw refusal;
      },
    });
    draftChange(ownerOf(recordedState(source)), { kind: "create", layer: "base", key: 47, def: recordedDks() });
    renderTab();
    fireEvent.click(await screen.findByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(en.errors.keymap.no_free_slot.advanced);
    expect(screen.queryByRole("button", { name: en.common.actions.writeToKeyboard })).toBeNull();
    expect(calls.some((c) => c.command === "apply_keymap_edit")).toBe(false);
    expect(advancedStore().draft.items).toHaveLength(1);
  });

  it.each([
    ["cancelled", true],
    ["cancelled", false],
    ["failed", false],
  ] as const)("keeps the draft when a write did not complete (%s, something written: %s)", async (outcome, written) => {
    const recorded = fixture("keys_apply_advanced").calls.find((c) => c.command === "apply_keymap_edit");
    if (recorded?.command !== "apply_keymap_edit" || recorded.ok === null) throw new Error("no recorded report");
    const report = { ...recorded.ok, outcome, headline: outcome, anything_written: written };
    const source = install("keys_preview_advanced", { apply_keymap_edit: () => report });
    draftChange(ownerOf(recordedState(source)), { kind: "create", layer: "base", key: 47, def: recordedDks() });
    renderTab();
    fireEvent.click(await screen.findByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    await pressWrite();
    await waitFor(() => {
      expect(calls.filter((c) => c.command === "apply_keymap_edit")).toHaveLength(1);
    });
    await sleep(10);
    expect(advancedStore().draft.items).toHaveLength(1);
    expect(draftEntries()).toContainEqual({ page: "keys", tab: "advanced", count: 1 });
  });

  it("reports a failed write and keeps the draft", async () => {
    const failure: AppError = { kind: "disconnected", detail: null, message: "the device went away" };
    const source = install("keys_preview_advanced", {
      apply_keymap_edit: () => {
        throw failure;
      },
    });
    draftChange(ownerOf(recordedState(source)), { kind: "create", layer: "base", key: 47, def: recordedDks() });
    renderTab();
    fireEvent.click(await screen.findByRole("button", { name: en.common.actions.reviewAndWrite }));
    await screen.findByText(REVIEW_TITLE);
    await pressWrite();
    await screen.findByText(en.errors.kind.disconnected.title);
    expect(advancedStore().draft.items).toHaveLength(1);
  });

  it("says why nothing can be written while the keyboard is disconnected", async () => {
    const source = install("keys_ready");
    statusStore.set({ ...statusStore.get(), write_gate: { allowed: false, reasons: ["not_connected"] } });
    draftChange(ownerOf(recordedState(source)), { kind: "delete", slot: 0 });
    renderTab();
    const review = await screen.findByRole("button", { name: en.common.actions.reviewAndWrite });
    expect(review).toHaveProperty("disabled", true);
    expect(screen.getByText(en.errors.writeBlocked.not_connected)).toBeTruthy();
    fireEvent.click(review);
    expect(calls.some((c) => c.command === "prepare_keymap_edit")).toBe(false);
  });

  it("speaks Turkish", async () => {
    setLang("tr");
    install("keys_ready");
    renderTab();
    await screen.findByText(tr.keys.slot.used({ n: 4, total: 40 }));
    expect(screen.getByText("Hafif basış (1,5 mm): E tuşuna bir kez basar. Tam basış: R tuşuna bir kez basar.")).toBeTruthy();
    expect(screen.getByRole("heading", { name: tr.keysAdvanced.create.title })).toBeTruthy();
    expect(screen.getByText(tr.keysAdvanced.draft.idle)).toBeTruthy();
  });
});

describe("the step names", () => {
  it("cover every step in both languages", () => {
    for (const step of DKS_STEPS) {
      expect(en.keysAdvanced.dks.steps[step]).not.toBe("");
      expect(tr.keysAdvanced.dks.steps[step]).not.toBe(en.keysAdvanced.dks.steps[step]);
    }
  });
});
