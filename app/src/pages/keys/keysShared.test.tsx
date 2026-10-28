// The Keys page's shared parts on the mocked IPC: the preview's mock backend
// answers with the fixtures the core's tests recorded from the real worker on
// the simulator (keys_*), through the real `api`, `invoke` and `Channel`.
//
// Covered: the keys state fetch and the page's shared states, intents
// (built-in acknowledgements, based_on), the review (Layer | Key | Now |
// New, writes in order with their own reasons, re-sends after Macro 1, the
// commit note from the plan, warnings), the critical-built-in refusal
// answered in the flow, the persistence option, and the results (emptied
// macros and keys left on them, the persistence check, Stop).

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ToastRegion } from "../../components/Toasts";
import { DICTIONARIES, setLang } from "../../i18n";
import type {
  AppError,
  DeviceStatus,
  EditReportView,
  KeymapEditIntent,
  KeymapEditPreview,
  KeysState,
} from "../../ipc/bindings";
import { createMockBackend } from "../../preview/mockBackend";
import { FIXTURES } from "../../preview/registry";
import type { Fixture } from "../../preview/types";
import { appInfoStore, DEFAULT_APP_INFO } from "../../state/appInfo";
import { acknowledgeVendor, resetSession } from "../../state/session";
import { INITIAL_STATUS, statusStore } from "../../state/status";
import { clearToasts } from "../../state/toasts";
import KeysPage from "./index";
import {
  acksFor,
  BuiltinNotice,
  buildKeymapIntent,
  change,
  commitNoteText,
  keysBlockReason,
  KeymapEditFlow,
  KeymapReview,
  loadMacroLibrary,
  reviewCounts,
  reviewWhat,
  neededAcks,
  resetKeysStore,
  type FlowAnswers,
} from "./shared";

const en = DICTIONARIES.en;
/** The key-assignment review's title on profile 1 (its words follow what the plan changes). */
const REVIEW_TITLE = /^Change .+ on profile 1\?$/u;

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

/** Whether the label of a key's Fn built-in is confirmed on a W75 (the words hedge otherwise). */
function labelConfirmed(state: KeysState, index: number): boolean {
  return state.keys.find((key) => key.key.index === index)?.builtin?.label_confirmed ?? false;
}

function recordedPreview(source: Fixture): KeymapEditPreview {
  const call = source.calls.find((c) => c.command === "prepare_keymap_edit");
  if (call?.command !== "prepare_keymap_edit" || call.ok === null) throw new Error(`no plan in ${source.id}`);
  return call.ok;
}

/** Installs the mock backend for a fixture and puts its status into the store. */
function install(id: string): Fixture {
  const source = fixture(id);
  const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
  mockIPC((command, args) => {
    calls.push({ command, args: (args ?? {}) as Record<string, unknown> });
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

async function pressWrite(): Promise<void> {
  const write = await screen.findByRole("button", { name: en.common.actions.writeToKeyboard });
  await sleep(560);
  fireEvent.click(write);
}

function renderFlow(source: Fixture, onClose: (report: EditReportView | null, answers: FlowAnswers) => void = vi.fn()) {
  const state = recordedState(source);
  const intent = recordedIntent(source);
  render(
    <>
      <KeymapEditFlow state={state} changes={intent.changes} onClose={onClose} />
      <ToastRegion />
    </>,
  );
  return { state, intent };
}

beforeEach(() => {
  setLang("en");
  resetSession();
  acknowledgeVendor();
  resetKeysStore();
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
  window.location.hash = "";
});

describe("intents", () => {
  it("reproduce the recorded plan's intent, with the non-critical built-in acknowledged", () => {
    const source = fixture("keys_preview_assign");
    const state = recordedState(source);
    const recorded = recordedIntent(source);
    expect(buildKeymapIntent(recorded.changes, state)).toEqual(recorded);
    expect(recorded.builtin_acks).toEqual([{ key: 3, critical: false }]);
  });

  it("leave a critical built-in out until the user confirmed it", () => {
    const source = fixture("keys_refused_critical");
    const state = recordedState(source);
    const changes = recordedIntent(source).changes;
    expect(neededAcks(changes, state)).toEqual([{ key: 24, critical: true }]);
    expect(acksFor(changes, state)).toEqual([]);
    expect(acksFor(changes, state, [{ key: 24, critical: true }])).toEqual([{ key: 24, critical: true }]);
  });

  it("need no acknowledgement to give a built-in back, nor on the base layer", () => {
    const state = recordedState(fixture("keys_ready"));
    expect(neededAcks([change.reset("fn", 24), change.assign("fn", 24, { kind: "default" })], state)).toEqual([]);
    expect(neededAcks([change.assign("base", 24, { kind: "key", usage: 4 })], state)).toEqual([]);
    expect(neededAcks([change.swap("fn", 2, 3)], state)).toEqual([
      { key: 2, critical: false },
      { key: 3, critical: false },
    ]);
  });

  it("carry the keys state's revision and the options", () => {
    const state = recordedState(fixture("keys_ready"));
    const intent = buildKeymapIntent([change.clearMacro(3)], state, { checkPersistence: true, keepOrphans: true });
    expect(intent).toEqual({
      changes: [{ change: "clear_macro", slot: 3 }],
      builtin_acks: [],
      keep_orphans: true,
      allow_not_undoable: false,
      check_persistence: true,
      based_on: state.revision,
    });
    expect(buildKeymapIntent([], state)).toBeNull();
    expect(buildKeymapIntent([change.clearMacro(3)], null)).toBeNull();
  });
});

describe("the commit note", () => {
  it("comes from the plan's own reasons", () => {
    expect(commitNoteText(recordedPreview(fixture("keys_preview_assign")).writes, en)).toBe(
      en.keys.review.commitNote.baseKeymap,
    );
    expect(commitNoteText(recordedPreview(fixture("keys_preview_advanced")).writes, en)).toBe(
      en.keys.review.commitNote.baseKeymap,
    );
    expect(commitNoteText(recordedPreview(fixture("keys_apply_failed_cleared")).writes, en)).toBe(
      en.keys.review.commitNote.ownOnly,
    );
    // A content-only change of an advanced key: the complete Fn keymap goes first, for the slot.
    expect(commitNoteText(recordedPreview(fixture("keys_preview_advanced_edit")).writes, en)).toBe(
      en.keys.review.commitNote.advancedKeys,
    );
  });

  it("is shown in the review of a content-only advanced-key change", async () => {
    install("keys_preview_advanced_edit");
    renderFlow(fixture("keys_preview_advanced_edit"));
    expect(await screen.findByText(en.keys.review.commitNote.advancedKeys)).toBeTruthy();
  });
});

describe("the review's title and summary", () => {
  it("follow what the plan changes", () => {
    expect(reviewWhat(recordedPreview(fixture("keys_preview_assign")))).toBe("keys");
    expect(reviewWhat(recordedPreview(fixture("keys_preview_slot0")))).toBe("mixed");
    expect(reviewWhat(recordedPreview(fixture("keys_apply_failed_cleared")))).toBe("macros");
    expect(reviewWhat(recordedPreview(fixture("keys_preview_advanced")))).toBe("advanced");
    expect(reviewWhat(recordedPreview(fixture("keys_preview_advanced_edit")))).toBe("advanced");
    expect(en.keys.review.title({ n: 1, what: "macros" })).toBe("Change macros on profile 1?");
    const counts = reviewCounts(recordedPreview(fixture("keys_preview_slot0")));
    expect(counts).toMatchObject({ keys: 1, advanced: 0 });
    expect(counts.macros).toBeGreaterThan(0);
    expect(en.keys.review.summary({ keys: 0, macros: 1, advanced: 0, writes: 3 })).toBe("Changes 1 macro, in 3 writes.");
  });

  it("give the Fn layer's commit reason once, in short words", () => {
    expect(en.flow.writeReasons.baseKeymapChanged).not.toContain("(");
    expect(en.flow.writeReasons.advancedKeysChanged).not.toContain("(");
  });
});

describe("the words for Fn + 1..4", () => {
  it("never say that Fn + k can't switch to profile k while profile k is active", () => {
    for (const dict of [en, DICTIONARIES.tr]) {
      const same = dict.keys.builtinProfile({ k: 2, n: 2 });
      const other = dict.keys.builtinProfile({ k: 2, n: 1 });
      expect(same).not.toBe(other);
      expect(dict.keys.builtinConfirmProfile({ k: 2, n: 2 })).not.toBe(dict.keys.builtinConfirmProfile({ k: 2, n: 1 }));
    }
    expect(en.keys.builtinProfile({ k: 2, n: 2 })).toContain("the profile you are changing");
    expect(en.keys.builtinProfile({ k: 2, n: 2 })).not.toContain("while profile 2 is active");
    expect(en.keys.builtinConfirmProfile({ k: 1, n: 1 })).toBe("Replace Fn + 1 on profile 1 (it only switches to this same profile)");
    // A label not confirmed on a W75 yet is hedged (its badge says "Not yet verified").
    expect(en.keys.builtinProfile({ k: 3, n: 1, likely: true })).toContain("most likely");
    expect(en.keys.builtinProfile({ k: 2, n: 1 })).not.toContain("most likely");
  });
});

describe("the macro library", () => {
  it("says once that a damaged library file was set aside", async () => {
    install("keys_library_invalid");
    loadMacroLibrary(en, true);
    render(<ToastRegion />);
    expect(await screen.findByText(en.keys.library.invalidKept)).toBeTruthy();
    loadMacroLibrary(en);
    await sleep(10);
    expect(screen.getAllByText(en.keys.library.invalidKept)).toHaveLength(1);
    expect(calls.filter((c) => c.command === "list_macro_library")).toHaveLength(1);
  });
});

describe("the Keys page", () => {
  it("loads the keys state and shows the tabs", async () => {
    install("keys_ready");
    render(<KeysPage />);
    await screen.findByText(en.keys.board.title({ n: 1 }), { exact: false });
    expect(screen.getByRole("heading", { level: 1, name: en.keys.title })).toBeTruthy();
    const tabs = screen.getByRole("navigation", { name: en.shell.tabs.label({ page: en.shell.nav.keys }) });
    expect(within(tabs).getAllByRole("link").map((link) => link.textContent)).toEqual([
      en.shell.tabs.pages.keys.assign,
      en.shell.tabs.pages.keys.macros,
      en.shell.tabs.pages.keys.advanced,
    ]);
    expect(within(tabs).getByRole("link", { name: en.shell.tabs.pages.keys.assign }).getAttribute("aria-current")).toBe("page");
    expect(calls.filter((c) => c.command === "get_keys_state")).toHaveLength(1);
  });

  it("says why nothing is shown without a keyboard", () => {
    mockIPC(() => null);
    render(<KeysPage />);
    expect(screen.getByText(en.keys.empty.title)).toBeTruthy();
    expect(screen.getByText(en.keys.empty.body)).toBeTruthy();
  });

  it("explains why changes can't be written now", () => {
    const state = recordedState(fixture("keys_ready"));
    const ready: DeviceStatus = { ...INITIAL_STATUS, write_gate: { allowed: true, reasons: [] } };
    const view = { state, last: state, loading: false, error: null, stale: false };
    expect(keysBlockReason(view, ready, en)).toBeNull();
    expect(keysBlockReason({ ...view, stale: true }, ready, en)).toBe(en.errors.writeBlocked.snapshot_stale);
    expect(keysBlockReason({ ...view, state: null }, ready, en)).toBe(en.errors.writeBlocked.not_connected);
    expect(keysBlockReason({ ...view, state: { ...state, editable: false, disabled: { reason: "unsupported", message: "x" } } }, ready, en)).toBe(
      en.keys.disabled.unsupported,
    );
    expect(
      keysBlockReason(view, { ...ready, write_gate: { allowed: false, reasons: ["other_program"] } }, en),
    ).toBe(en.errors.writeBlocked.other_program);
  });
});

describe("the review", () => {
  it("shows Layer | Key | Now | New, the writes in order with their reasons and the built-in warning", async () => {
    const source = install("keys_preview_assign");
    renderFlow(source);
    await screen.findByText(REVIEW_TITLE);
    expect(preparedIntents()).toEqual([recordedIntent(source)]);

    const table = screen.getByRole("table");
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual([
      en.keys.review.columns.layer,
      en.keys.review.columns.key,
      en.keys.review.columns.now,
      en.keys.review.columns.new,
    ]);
    const f2 = within(table).getByRole("rowheader", { name: "Fn + F2 (#3)" }).closest("tr") as HTMLElement;
    expect(f2.textContent).toContain("Default: Volume down");
    expect(f2.textContent).toContain("Back button (X1)");
    const r = within(table).getByRole("rowheader", { name: "R (#48)" }).closest("tr") as HTMLElement;
    expect(r.textContent).toContain("Left Ctrl + C");

    const writes = screen.getByRole("heading", { name: en.keys.review.writesTitle }).parentElement as HTMLElement;
    const items = within(writes).getAllByRole("listitem");
    expect(items.map((item) => item.querySelector(".ow-strong")?.textContent)).toEqual([
      en.flow.tables.keymap.base,
      en.flow.tables.keymap.fn,
    ]);
    // The Fn layer changes here, so it is not "re-sent unchanged".
    expect(items[1]?.textContent).toContain(en.flow.writeReasons.baseKeymapChanged);
    expect(within(writes).getByText(en.keys.review.commitNote.baseKeymap)).toBeTruthy();

    expect(screen.getByText(en.keys.review.warnings.builtin({ key: "Fn + F2 (#3)", function: "Volume down" }))).toBeTruthy();
    expect(
      screen.getByText(en.keys.review.warnings.dangling_reference_preserved({ key: "J (#74)", slot: "Macro 6" })),
    ).toBeTruthy();
    // "Check that the keyboard kept it" is offered, not pre-ticked.
    const check = screen.getByRole("checkbox", { name: en.keys.review.persistence.check({ m: 2 }) });
    expect(check).toHaveProperty("checked", false);
  });

  it("prepares again with the persistence check when it is ticked", async () => {
    install("keys_preview_assign");
    renderFlow(fixture("keys_preview_assign"));
    const check = await screen.findByRole("checkbox", { name: en.keys.review.persistence.check({ m: 2 }) });
    fireEvent.click(check);
    await waitFor(() => {
      expect(preparedIntents()).toHaveLength(2);
    });
    expect(preparedIntents()[0]?.check_persistence).toBe(false);
    expect(preparedIntents()[1]?.check_persistence).toBe(true);
    expect(calls.some((c) => c.command === "discard_plan")).toBe(true);
  });

  it("lists the macros re-sent after Macro 1 with their reason", async () => {
    install("keys_preview_slot0");
    renderFlow(fixture("keys_preview_slot0"));
    await screen.findByText(REVIEW_TITLE);
    const writes = screen.getByRole("heading", { name: en.keys.review.writesTitle }).parentElement as HTMLElement;
    const items = within(writes).getAllByRole("listitem");
    expect(items.map((item) => item.querySelector(".ow-strong")?.textContent)).toEqual([
      "Macro 1",
      "Macro 2",
      "Macro 3",
      en.flow.tables.keymap.base,
      en.flow.tables.keymap.fn,
    ]);
    expect(items[0]?.textContent).not.toContain(en.keys.review.unchanged);
    expect(items[1]?.textContent).toContain(en.keys.review.unchanged);
    expect(items[1]?.textContent).toContain(en.flow.resent.macro_slot0_clears);
    expect(items[1]?.textContent).toContain(en.flow.writeReasons.completesBinding);
    expect(items[4]?.textContent).toContain(en.flow.writeReasons.baseKeymapUnchanged);
    const slots = screen.getByRole("heading", { name: en.keys.review.slotsTitle }).parentElement as HTMLElement;
    expect(slots.textContent).toContain("Macro 1: Empty → 2 steps · Plays once");
  });

  it("asks for the explicit confirmation a critical built-in needs, then prepares again with it", async () => {
    install("keys_refused_critical");
    renderFlow(fixture("keys_refused_critical"));
    const confirm = await screen.findByRole("checkbox", { name: en.keys.builtinConfirmProfile({ k: 2, n: 1 }) });
    expect(confirm).toHaveProperty("checked", false);
    expect(screen.getByText(en.keys.builtinProfile({ k: 2, n: 1 }))).toBeTruthy();
    expect(
      screen.getByText(en.errors.keymap.builtin_not_acknowledged_critical({ key: "Fn + 2 (#24)", function: "Profile 2" })),
    ).toBeTruthy();
    const again = screen.getByRole("button", { name: en.keys.review.reviewAgain });
    expect(again).toHaveProperty("disabled", true);
    expect(screen.getByText(en.keys.review.criticalNeeded)).toBeTruthy();
    fireEvent.click(confirm);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: en.keys.review.reviewAgain })).toHaveProperty("disabled", false);
    });
    fireEvent.click(screen.getByRole("button", { name: en.keys.review.reviewAgain }));
    await waitFor(() => {
      expect(preparedIntents()).toHaveLength(2);
    });
    expect(preparedIntents()[0]?.builtin_acks).toEqual([]);
    expect(preparedIntents()[1]?.builtin_acks).toEqual([{ key: 24, critical: true }]);
  });
});

describe("Fn + k replaced on profile k", () => {
  // Fn + 1 (#23) switches to profile 1: on profile 1 it switches nowhere.
  const ONE = { index: 23, row: 1, col: 1, name: "1" };
  const TWO = { index: 24, row: 1, col: 2, name: "2" };

  function calloutOf(text: string): HTMLElement {
    const callout = screen.getByText(text).closest(".ow-callout");
    if (!(callout instanceof HTMLElement)) throw new Error(`no callout around ${text}`);
    return callout;
  }

  it("is a warning in the review, like the page's notice, while Fn + 2 stays a danger", () => {
    const source = fixture("keys_preview_assign");
    const state = recordedState(source);
    const recorded = recordedPreview(source);
    const preview: KeymapEditPreview = {
      ...recorded,
      profile: 1,
      warnings: [
        { code: "fn_builtin_overridden", key: ONE, function: "profile 1", critical: true },
        { code: "fn_builtin_overridden", key: TWO, function: "profile 2", critical: true },
      ],
    };
    render(<KeymapReview preview={preview} state={state} />);
    const own = calloutOf(en.keys.builtinProfile({ k: 1, n: 1, likely: !labelConfirmed(state, 23) }));
    expect(own.className).toContain("ow-callout--warning");
    expect(within(own).getByText(en.keys.review.ownProfileTitle)).toBeTruthy();
    expect(within(own).queryByText(en.keys.review.criticalTitle)).toBeNull();
    const other = calloutOf(en.keys.builtinProfile({ k: 2, n: 1, likely: !labelConfirmed(state, 24) }));
    expect(other.className).toContain("ow-callout--danger");
    expect(within(other).getByText(en.keys.review.criticalTitle)).toBeTruthy();

    // The page's notice for the same key and profile says the same.
    cleanup();
    const keyView = state.keys.find((key) => key.key.index === 23);
    if (keyView === undefined) throw new Error("no key 23");
    render(<BuiltinNotice keyView={keyView} profile={1} confirmed={false} onConfirm={vi.fn()} />);
    const notice = calloutOf(en.keys.builtinProfile({ k: 1, n: 1, likely: !labelConfirmed(state, 23) }));
    expect(notice.className).toContain("ow-callout--warning");
    expect(within(notice).getByText(en.keys.review.ownProfileTitle)).toBeTruthy();
  });

  it("is a warning in the refusal too, with its confirmation", async () => {
    const source = fixture("keys_refused_critical");
    const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
    const refusal: AppError = {
      kind: "invalid_request",
      detail: {
        detail: "keymap",
        problem: { problem: "builtin_not_acknowledged", key: ONE, function: "profile 1", critical: true },
      },
      message: "Fn + 1 (#23) is the built-in function \"profile 1\"; nothing was written",
    };
    mockIPC((command, args) => {
      calls.push({ command, args: (args ?? {}) as Record<string, unknown> });
      if (command === "prepare_keymap_edit") throw refusal;
      return backend.handle(command, args);
    });
    statusStore.set({ ...source.initial_status, write_gate: { allowed: true, reasons: [] }, busy: null });
    appInfoStore.set(source.app_info);
    const state = recordedState(source);
    expect(state.profile).toBe(1);
    render(
      <KeymapEditFlow
        state={state}
        changes={[{ change: "assign", layer: "fn", key: 23, action: { kind: "key", usage: 4 } }]}
        onClose={vi.fn()}
      />,
    );
    const confirm = await screen.findByRole("checkbox", { name: en.keys.builtinConfirmProfile({ k: 1, n: 1 }) });
    const callout = confirm.closest(".ow-callout");
    if (!(callout instanceof HTMLElement)) throw new Error("no callout");
    expect(callout.className).toContain("ow-callout--warning");
    expect(within(callout).getByText(en.keys.review.ownProfileTitle)).toBeTruthy();
    expect(within(callout).queryByText(en.keys.review.criticalTitle)).toBeNull();
  });
});

describe("a slot Undo couldn't bring back", () => {
  it("asks to continue without Undo for it, and prepares again only after the tick", async () => {
    const source = install("keys_refused_not_undoable");
    renderFlow(source);
    expect(await screen.findByText(en.keys.review.notUndoableTitle)).toBeTruthy();
    const confirm = screen.getByRole("checkbox", { name: en.keys.review.notUndoableConfirm });
    expect(confirm).toHaveProperty("checked", false);
    const again = screen.getByRole("button", { name: en.keys.review.reviewAgain });
    expect(again).toHaveProperty("disabled", true);
    expect(screen.getByText(en.keys.review.notUndoableNeeded)).toBeTruthy();
    fireEvent.click(confirm);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: en.keys.review.reviewAgain })).toHaveProperty("disabled", false);
    });
    fireEvent.click(screen.getByRole("button", { name: en.keys.review.reviewAgain }));
    await waitFor(() => {
      expect(preparedIntents()).toHaveLength(2);
    });
    expect(preparedIntents().map((intent) => intent.allow_not_undoable)).toEqual([false, true]);
    // The plan says what can't be undone.
    expect(await screen.findByRole("button", { name: en.common.actions.writeToKeyboard })).toBeTruthy();
    expect(calls.filter((c) => c.command === "apply_keymap_edit")).toHaveLength(0);
  });
});

describe("the result", () => {
  it("lists emptied macros and keys left on them, with Undo", async () => {
    install("keys_apply_failed_cleared");
    const onClose = vi.fn();
    renderFlow(fixture("keys_apply_failed_cleared"), onClose);
    await screen.findByText(REVIEW_TITLE);
    await pressWrite();
    const cleared = await screen.findByText(en.flow.report.clearedTitle);
    const clearedBox = cleared.closest(".ow-callout") as HTMLElement;
    expect(within(clearedBox).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["Macro 2", "Macro 3"]);
    const dangling = screen.getByText(en.flow.report.danglingTitle).closest(".ow-callout") as HTMLElement;
    expect(within(dangling).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "Q (#45) → Macro 2",
      "Fn + W (#46) → Macro 3",
    ]);
    // The re-sent macro rows say why they were written.
    expect(screen.getAllByText(en.flow.resent.macro_slot0_clears, { exact: false }).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: en.common.actions.undo })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.close }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect((onClose.mock.calls[0] as [EditReportView | null])[0]?.headline).toBe("failed");
  });

  it("says what the persistence check found", async () => {
    install("keys_apply_checked");
    renderFlow(fixture("keys_apply_checked"));
    await screen.findByText(REVIEW_TITLE);
    // The option shows the plan's own state (this recorded plan asked for the check).
    expect(screen.getByRole("checkbox", { name: en.keys.review.persistence.check({ m: 2 }) })).toHaveProperty(
      "checked",
      true,
    );
    await pressWrite();
    await screen.findByText(en.flow.report.persistenceTitle);
    expect(screen.getByText(en.flow.report.persistenceOk({ n: 2 }))).toBeTruthy();
    expect(screen.getAllByText(en.flow.results.persisted).length).toBe(2);
  });

  it("reports a stop after Macro 1: the re-sent macros were still written", async () => {
    const source = install("keys_apply_cancelled_after_slot0");
    const apply = source.calls.find((c) => c.command === "apply_keymap_edit");
    if (apply?.command !== "apply_keymap_edit" || apply.ok === null) throw new Error("no apply");
    expect(apply.ok.outcome).toBe("cancelled");
    expect(apply.ok.cancel_requested).toBe(true);
    renderFlow(source);
    await screen.findByText(REVIEW_TITLE);
    await pressWrite();
    const report = await screen.findByText(en.flow.footer.safetyBackup);
    expect(report).toBeTruthy();
    // Stop skipped writes, so the "everything was written" note is not shown.
    expect(screen.queryByText(en.flow.report.cancelRequested)).toBeNull();
    expect(screen.getAllByText(en.flow.results.notAttempted).length).toBeGreaterThan(0);
  });
});
