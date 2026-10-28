// Keys > Macros, pure logic: the simple view's items and the step edits, the
// live checks (the library's rules) and "Release all at end", the draft and
// the key-assignment changes it becomes (checked against the intents the
// core's tests recorded from the simulator), and the recorder.

import { describe, expect, it } from "vitest";

import { KEY_CODE_USAGES } from "../../../generated/hidCodes";
import type { KeymapEditIntent, KeysState, MacroProgramView, MacroStepView } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import type { Fixture } from "../../../preview/types";
import {
  addNew,
  addTap,
  canBind,
  canTry,
  chosenBindings,
  draftChanges,
  draftCount,
  draftProblems,
  editedSlots,
  EMPTY_DRAFT,
  heldAtEnd,
  libraryMatch,
  limitsOf,
  macroProblems,
  moveItem,
  moveStep,
  ownMacroSlot,
  reconcile,
  releaseAll,
  removeItem,
  setAck,
  setBind,
  setProgram,
  setSlotEdit,
  simpleItems,
  stepKey,
  tryKeys,
  updateStep,
  updateNew,
  type MacroLimits,
} from "./model";
import { DICTIONARIES } from "../../../i18n";
import { bindDisabledReason } from "./KeyBinding";
import { record, RECORDER_START, usageOfCode, type RecorderOptions, type RecorderState } from "./recording";
import { isStepKey } from "./StepsEditor";

const A = 0x04;
const B = 0x05;
const C = 0x06;
const SHIFT = 0xe1;
const LIMITS: MacroLimits = { maxSteps: 40, maxDelayMs: 65535, maxTimes: 255 };

function fixture(id: string): Fixture {
  const found = FIXTURES[id];
  if (found === undefined) throw new Error(`no fixture ${id}`);
  return structuredClone(found);
}

function keysState(id: string, index = 0): KeysState {
  const calls = fixture(id).calls.filter((c) => c.command === "get_keys_state");
  const call = calls[index];
  if (call?.command !== "get_keys_state" || call.ok === null) throw new Error(`no keys state in ${id}`);
  return call.ok;
}

function recordedIntent(id: string): KeymapEditIntent {
  const call = fixture(id).calls.find((c) => c.command === "prepare_keymap_edit");
  if (call?.command !== "prepare_keymap_edit") throw new Error(`no prepare in ${id}`);
  return call.args.intent;
}

const press = (usage: number, delay = 0): MacroStepView => ({ usage, motion: "press", delay_after_ms: delay });
const release = (usage: number, delay = 0): MacroStepView => ({ usage, motion: "release", delay_after_ms: delay });
const once = (steps: MacroStepView[]): MacroProgramView => ({ play: { mode: "once" }, steps });

describe("the simple view", () => {
  it("groups a press right followed by its release as a tap", () => {
    const steps = [press(SHIFT, 5), press(A, 20), release(A, 10), release(SHIFT, 0)];
    expect(simpleItems(steps)).toEqual([
      { kind: "press", usage: SHIFT, start: 0, length: 1, holdMs: 0, waitMs: 5 },
      { kind: "tap", usage: A, start: 1, length: 2, holdMs: 20, waitMs: 10 },
      { kind: "release", usage: SHIFT, start: 3, length: 1, holdMs: 0, waitMs: 0 },
    ]);
  });

  it("moves and removes a tap as one item; the step that ends up last never waits", () => {
    const steps = [press(A, 10), release(A, 10), press(B, 10), release(B, 0)];
    // Moved items keep their waits (B's release waited 0, so A follows at once).
    expect(moveItem(steps, 1, 0)).toEqual([press(B, 10), release(B, 0), press(A, 10), release(A, 0)]);
    expect(removeItem(steps, 1)).toEqual([press(A, 10), release(A, 0)]);
    expect(moveStep(steps, 3, 0)).toEqual([release(B, 0), press(A, 10), release(A, 10), press(B, 0)]);
  });

  it("keeps a step's key while it is changed or moved (the focus follows it)", () => {
    const steps = [press(A, 10), release(A, 10), press(B, 10), release(B, 0)];
    const keys = steps.map((step) => stepKey(step));
    expect(new Set(keys).size).toBe(4);
    expect(updateStep(steps, 1, { delay_after_ms: 99 }).map((step) => stepKey(step))).toEqual(keys);
    expect(moveStep(steps, 3, 0).map((step) => stepKey(step))).toEqual([keys[3], keys[0], keys[1], keys[2]]);
  });

  it("adds a tap after the last step, which then waits", () => {
    expect(addTap([], C, 10)).toEqual([press(C, 10), release(C, 0)]);
    expect(addTap([press(A, 30), release(A, 0)], B, 10)).toEqual([press(A, 30), release(A, 10), press(B, 10), release(B, 0)]);
  });
});

describe("the live checks", () => {
  it("accept the keyboard's own macros", () => {
    const state = keysState("keys_ready");
    for (const slot of state.macros) {
      if (slot.content.state === "program") expect(macroProblems(slot.content.program, limitsOf(state))).toEqual([]);
    }
  });

  it("find what the library refuses: no steps, too many, unbalanced, held keys, waits, counts", () => {
    expect(macroProblems(once([]), LIMITS)).toEqual([{ kind: "no_steps" }]);
    const long = Array.from({ length: 21 }, () => [press(A), release(A)]).flat();
    expect(macroProblems(once(long), LIMITS)).toEqual([{ kind: "too_long", steps: 42, max: 40 }]);
    expect(macroProblems(once([release(A), press(A), press(A), release(A)]), LIMITS)).toEqual([
      { kind: "released_unpressed", step: 0, usage: A },
      { kind: "pressed_twice", step: 2, usage: A },
    ]);
    expect(macroProblems(once([press(SHIFT), press(A), release(A)]), LIMITS)).toEqual([{ kind: "held", usages: [SHIFT] }]);
    expect(macroProblems(once([press(A, Number.NaN), release(A, 70000), press(B, 1.5), release(B)]), LIMITS)).toEqual([
      { kind: "delay", step: 0 },
      { kind: "delay", step: 1 },
      { kind: "delay", step: 2 },
    ]);
    for (const count of [0, 256, Number.NaN, 2.5]) {
      expect(macroProblems({ play: { mode: "times", count }, steps: [press(A), release(A)] }, LIMITS)).toEqual([{ kind: "times" }]);
    }
    expect(macroProblems({ play: { mode: "times", count: 255 }, steps: [press(A), release(A)] }, LIMITS)).toEqual([]);
  });

  it("fix keys left held by releasing them at the end, the last pressed first", () => {
    const steps = [press(SHIFT, 5), press(A, 5), release(A, 5), press(B, 0)];
    expect(heldAtEnd(steps)).toEqual([SHIFT, B]);
    expect(releaseAll(steps, LIMITS)).toEqual([press(SHIFT, 5), press(A, 5), release(A, 5), press(B, 0), release(B, 0), release(SHIFT, 0)]);
    expect(releaseAll(steps, { ...LIMITS, maxSteps: 5 })).toBeNull();
  });
});

describe("the draft", () => {
  const state = keysState("keys_ready");
  const typeA = state.macros[2]?.content;
  if (typeA?.state !== "program") throw new Error("slot 2 holds Type A in keys_ready");

  it("keeps only real changes: the slot's own content, a key for a slot that has keys and removing an empty slot are none", () => {
    expect(setSlotEdit(EMPTY_DRAFT, state, 2, { program: typeA.program })).toEqual(EMPTY_DRAFT);
    expect(setSlotEdit(EMPTY_DRAFT, state, 2, { bind: { layer: "base", key: 80 } })).toEqual(EMPTY_DRAFT);
    expect(setSlotEdit(EMPTY_DRAFT, state, 0, { remove: true })).toEqual(EMPTY_DRAFT);
    expect(canBind(state.macros[3])).toBe(true);
    expect(canBind(state.macros[2])).toBe(false);
    const edited = setSlotEdit(EMPTY_DRAFT, state, 2, { program: once([press(B), release(B)]) });
    expect(draftCount(edited)).toBe(1);
    // Removing drops the other changes; keeping it again leaves nothing.
    const removed = setSlotEdit(edited, state, 2, { remove: true });
    expect(removed.slots).toEqual([{ slot: 2, program: null, bind: null, remove: true }]);
    expect(setSlotEdit(removed, state, 2, { remove: false })).toEqual(EMPTY_DRAFT);
  });

  it("becomes changes in the library's order: removals, slot contents and keys, then new macros", () => {
    let draft = addNew(EMPTY_DRAFT, 7, once([press(B), release(B)]));
    draft = updateNew(draft, 7, { bind: { layer: "fn", key: 48 } });
    draft = setSlotEdit(draft, state, 3, { bind: { layer: "base", key: 80 } });
    draft = setSlotEdit(draft, state, 0, { program: once([press(A), release(A)]), bind: { layer: "base", key: 79 } });
    draft = setSlotEdit(draft, state, 4, { program: once([press(C), release(C)]) });
    draft = setSlotEdit(draft, state, 2, { remove: true });
    draft = setSlotEdit(draft, state, 3, { remove: true });
    draft = setSlotEdit(draft, state, 3, { remove: false, bind: { layer: "base", key: 80 } });
    expect(draftChanges(draft, state)).toEqual([
      { change: "delete_macro", slot: 2 },
      { change: "bind_macro", layer: "base", key: 79, program: once([press(A), release(A)]), slot: 0 },
      { change: "assign", layer: "base", key: 80, action: { kind: "macro", slot: 3 } },
      { change: "set_macro", slot: 4, program: once([press(C), release(C)]) },
      { change: "bind_macro", layer: "fn", key: 48, program: once([press(B), release(B)]), slot: null },
    ]);
    expect(draftCount(draft)).toBe(5);
    // An orphan is removed (clear), a macro keys use is deleted (unbind + clear).
    expect(draftChanges(setSlotEdit(EMPTY_DRAFT, state, 3, { remove: true }), state)).toEqual([{ change: "clear_macro", slot: 3 }]);
  });

  it("reproduces the intents the core recorded on the simulator", () => {
    const cleared = keysState("keys_apply_failed_cleared");
    const slot0 = setProgram(EMPTY_DRAFT, cleared, { kind: "slot", slot: 0 }, once(addTap([], C, 10)));
    expect(draftChanges(slot0, cleared)).toEqual(recordedIntent("keys_apply_failed_cleared").changes);

    const checked = keysState("keys_apply_checked");
    let added = addNew(EMPTY_DRAFT, 1);
    added = setBind(added, checked, { kind: "new", id: 1 }, { layer: "fn", key: 48 });
    added = setProgram(added, checked, { kind: "new", id: 1 }, once([press(B, 50), release(B)]));
    expect(draftChanges(added, checked)).toEqual(recordedIntent("keys_apply_checked").changes);
  });

  it("asks for a key for a new macro and lists every program's problems", () => {
    let draft = addNew(EMPTY_DRAFT, 1, once([press(A)]));
    draft = setSlotEdit(draft, state, 4, { program: once([]) });
    expect(draftProblems(draft, LIMITS)).toEqual([
      { target: { kind: "slot", slot: 4 }, problems: [{ kind: "no_steps" }], needsKey: false },
      { target: { kind: "new", id: 1 }, problems: [{ kind: "held", usages: [A] }], needsKey: true },
    ]);
    // An untouched new macro is no change yet.
    expect(draftCount(addNew(EMPTY_DRAFT, 2))).toBe(0);
    expect(draftProblems(addNew(EMPTY_DRAFT, 2), LIMITS)).toEqual([]);
  });

  it("follows a newer read: edits that no longer apply go away", () => {
    const before = keysState("keys_apply_failed_cleared", 0);
    const after = keysState("keys_apply_failed_cleared", 1);
    // Slot 1 (Q) was emptied by the write: removing it is no longer possible.
    let draft = setSlotEdit(EMPTY_DRAFT, before, 1, { remove: true });
    // Slot 0 had no key; afterwards it holds a macro no key runs: a key for it stays.
    draft = setSlotEdit(draft, before, 0, { program: once([press(A), release(A)]), bind: { layer: "base", key: 80 } });
    const next = reconcile(draft, after);
    expect(next.slots).toEqual([{ slot: 0, program: once([press(A), release(A)]), bind: { layer: "base", key: 80 }, remove: false }]);
    expect(reconcile(next, after)).toBe(next);
  });

  it("keeps a new macro off a key whose macro the draft changes", () => {
    // Fn + Q runs Macro 3 (slot 2). With slot 2 edited, a new macro on Fn + Q would go into slot 2.
    const en = DICTIONARIES.en;
    const draft = setSlotEdit(EMPTY_DRAFT, state, 2, { program: once([press(C), release(C)]) });
    expect(editedSlots(draft)).toEqual([2]);
    const q = state.keys.find((key) => key.key.index === 45);
    if (q === undefined) throw new Error("no Q");
    expect(q.fn_layer.origin).toEqual({ origin: "macro_slot", slot: 2 });
    expect(bindDisabledReason(q, "fn", [], en, editedSlots(draft))).toBe(en.keysMacros.editor.keyDisabled.slotEdited({ n: 3 }));
    expect(bindDisabledReason(q, "fn", [], en, [])).toBeNull();
    // On the base layer Q runs no macro: free.
    expect(bindDisabledReason(q, "base", [], en, editedSlots(draft))).toBeNull();
    // A removed slot frees its keys (the delete goes first).
    expect(editedSlots(setSlotEdit(draft, state, 2, { remove: true }))).toEqual([]);
  });

  it("knows the keys taken by other macros, the critical confirmations and the keys to try", () => {
    let draft = addNew(EMPTY_DRAFT, 1, once([press(A), release(A)]));
    draft = updateNew(draft, 1, { bind: { layer: "fn", key: 48 } });
    draft = setSlotEdit(draft, state, 2, { program: once([press(B), release(B)]) });
    expect(chosenBindings(draft, null)).toEqual([{ layer: "fn", key: 48 }]);
    expect(chosenBindings(draft, { kind: "new", id: 1 })).toEqual([]);
    expect(tryKeys(draft, state)).toEqual([
      { layer: "fn", name: "Q" },
      { layer: "fn", name: "R" },
    ]);
    const acked = setAck(draft, 24, true);
    expect(acked.acks).toEqual([{ key: 24, critical: true }]);
    expect(setAck(acked, 24, false).acks).toEqual([]);
  });

  it("offers Try it only after a write that reached the active profile", () => {
    const report = (id: string) => {
      const call = fixture(id).calls.find((c) => c.command === "apply_keymap_edit");
      if (call?.command !== "apply_keymap_edit" || call.ok === null) throw new Error(`no report in ${id}`);
      return call.ok;
    };
    expect(canTry(report("keys_apply_checked"))).toBe(true);
    expect(canTry(report("keys_apply_failed_cleared"))).toBe(false);
    expect(canTry(report("keys_apply_cancelled_after_slot0"))).toBe(false);
    expect(canTry({ ...report("keys_apply_checked"), active_profile_now: 2 })).toBe(false);
  });

  it("matches library macros by content hash, else by steps", () => {
    const library = [
      { content_hash: "x", name: "By steps", program: once([press(C), release(C)]), saved_at: "" },
      { content_hash: typeA.content_hash, name: "By hash", program: once([]), saved_at: "" },
    ];
    expect(libraryMatch(typeA.program, library, typeA.content_hash)?.name).toBe("By hash");
    expect(libraryMatch(once([press(C), release(C)]), library)?.name).toBe("By steps");
    expect(libraryMatch(once([press(B), release(B)]), library)).toBeUndefined();
  });

  it("knows the slot a new macro reuses because its key runs it alone (Macro 1 too)", () => {
    // R (base) runs Macro 1 alone, Q (base) Macro 2, W (Fn) Macro 3; the recorded plan reuses Macro 1.
    const state = keysState("keys_preview_new_on_macro1");
    const index = (name: string) => {
      const found = state.keys.find((key) => key.key.name === name)?.key.index;
      if (found === undefined) throw new Error(`no key ${name}`);
      return found;
    };
    const R = index("R");
    const recorded = recordedIntent("keys_preview_new_on_macro1").changes[0];
    expect(recorded).toMatchObject({ change: "bind_macro", layer: "base", key: R, slot: null });
    expect(ownMacroSlot(state, EMPTY_DRAFT, { layer: "base", key: R })).toBe(0);
    expect(ownMacroSlot(state, EMPTY_DRAFT, { layer: "base", key: index("Q") })).toBe(1);
    expect(ownMacroSlot(state, EMPTY_DRAFT, { layer: "fn", key: index("W") })).toBe(2);
    // A key that runs no macro on that layer gets a free slot.
    expect(ownMacroSlot(state, EMPTY_DRAFT, { layer: "fn", key: R })).toBeNull();
    expect(ownMacroSlot(state, EMPTY_DRAFT, { layer: "base", key: index("W") })).toBeNull();
    expect(ownMacroSlot(state, EMPTY_DRAFT, null)).toBeNull();
    // Deleting Macro 1 in the same draft frees R first.
    const deleting = setSlotEdit(EMPTY_DRAFT, state, 0, { remove: true });
    expect(ownMacroSlot(state, deleting, { layer: "base", key: R })).toBeNull();
  });
});

describe("the recorder", () => {
  const options = (mode: RecorderOptions["mode"], fixedMs = 25): RecorderOptions => ({
    mode,
    fixedMs,
    maxSteps: 40,
    maxDelayMs: 65535,
  });
  const play = (events: [string, boolean, number, boolean?][], opts: RecorderOptions): RecorderState =>
    events.reduce(
      (state, [code, down, at, repeat]) => record(state, { code, down, at, repeat: repeat ?? false }, opts),
      RECORDER_START,
    );

  it("maps KeyboardEvent.code through the generated table; the editor offers every recordable key", () => {
    expect(usageOfCode("KeyA")).toBe(A);
    expect(usageOfCode("ShiftLeft")).toBe(SHIFT);
    expect(usageOfCode("OSLeft")).toBe(0xe3);
    expect(usageOfCode("AudioVolumeMute")).toBeNull();
    expect(usageOfCode("toString")).toBeNull();
    for (const usage of Object.values(KEY_CODE_USAGES)) expect(isStepKey(usage)).toBe(true);
  });

  it("takes the waits as typed, fixed or none; the last step never waits", () => {
    const events: [string, boolean, number][] = [
      ["KeyA", true, 1000],
      ["KeyA", false, 1030],
      ["KeyB", true, 1100.4],
      ["KeyB", false, 101100],
    ];
    expect(play(events, options("typed")).steps).toEqual([press(A, 30), release(A, 70), press(B, 65535), release(B, 0)]);
    expect(play(events, options("fixed")).steps).toEqual([press(A, 25), release(A, 25), press(B, 25), release(B, 0)]);
    expect(play(events, options("none")).steps).toEqual([press(A), release(A), press(B), release(B)]);
    expect(play(events, options("fixed", Number.NaN)).steps[0]?.delay_after_ms).toBe(0);
  });

  it("ignores repeats, presses of held keys and releases of keys pressed before; counts keys it can't send", () => {
    const state = play(
      [
        ["KeyB", false, 0],
        ["KeyA", true, 10],
        ["KeyA", true, 40, true],
        ["KeyA", true, 50],
        ["AudioVolumeMute", true, 60],
        ["AudioVolumeMute", false, 65],
        ["KeyA", false, 70],
      ],
      options("none"),
    );
    expect(state.steps).toEqual([press(A), release(A)]);
    expect(state.skipped).toBe(1);
    expect(state.held).toEqual([]);
  });

  it("takes a press only while its release and those of every held key still fit", () => {
    const small = { ...options("none"), maxSteps: 4 };
    const state = play(
      [
        ["ShiftLeft", true, 0],
        ["KeyA", true, 10],
        ["KeyB", true, 20],
        ["KeyA", false, 30],
        ["ShiftLeft", false, 40],
      ],
      small,
    );
    expect(state.full).toBe(true);
    expect(state.steps).toEqual([press(SHIFT), press(A), release(A), release(SHIFT)]);
    expect(macroProblems(once(state.steps), { ...LIMITS, maxSteps: 4 })).toEqual([]);
  });
});
