// Keys > Macros: the tab's pure logic. A macro is a list of key steps (press
// or release, then a wait) played in one of four ways. This file holds:
// - the simple view's items (a tap = a press right followed by its release)
//   and the edits both views make (add, move, remove; the last step never
//   waits);
// - the live checks, which mirror the library's rules (every key released by
//   the end, at most 40 steps, waits 0-65535 ms, a repeat count 1-255) and the
//   "Release all at end" fix;
// - the tab's draft (edits of the keyboard's slots and new macros) and the
//   key-assignment changes it becomes: one Write sends them all as one
//   library operation.
// Nothing here talks to the keyboard; the library checks every intent again.

import type {
  BuiltinAckIntent,
  EditReportView,
  KeyAssignmentsView,
  KeymapChangeIntent,
  KeysState,
  LayerView,
  LibraryMacroView,
  MacroPlayView,
  MacroProgramView,
  MacroSlotStateView,
  MacroStepView,
} from "../../../ipc/bindings";
import { change } from "../shared/intents";

/** The value ranges of the macro editor (from the keys state's limits). */
export interface MacroLimits {
  maxSteps: number;
  maxDelayMs: number;
  maxTimes: number;
}

/** The limits of a keys state. */
export function limitsOf(state: KeysState): MacroLimits {
  return {
    maxSteps: state.limits.macro_max_steps,
    maxDelayMs: state.limits.macro_max_delay_ms,
    maxTimes: state.limits.macro_max_times,
  };
}

/** The wait after a step added by hand (ms). */
export const DEFAULT_DELAY_MS = 10;

/** A new macro: no steps, played once. */
export const EMPTY_PROGRAM: MacroProgramView = { play: { mode: "once" }, steps: [] };

// ---------- programs ----------

function samePlay(a: MacroPlayView, b: MacroPlayView): boolean {
  if (a.mode !== b.mode) return false;
  return a.mode !== "times" || b.mode !== "times" || a.count === b.count;
}

/** Whether two programs play the same steps the same way. */
export function sameProgram(a: MacroProgramView, b: MacroProgramView): boolean {
  return (
    samePlay(a.play, b.play) &&
    a.steps.length === b.steps.length &&
    a.steps.every((step, index) => {
      const other = b.steps[index];
      return (
        other !== undefined &&
        other.usage === step.usage &&
        other.motion === step.motion &&
        other.delay_after_ms === step.delay_after_ms
      );
    })
  );
}

// Steps have no ids: the editors key them by object identity, and a step
// changed in place (its key, motion or wait) keeps its key, so that its
// fields keep the focus and a moved step keeps it too.
const STEP_KEYS = new WeakMap<MacroStepView, number>();
let nextStepKey = 1;

/** A key of a step that stays the same while it is moved or changed. */
export function stepKey(step: MacroStepView | undefined): string {
  if (step === undefined) return "none";
  let key = STEP_KEYS.get(step);
  if (key === undefined) {
    key = nextStepKey;
    nextStepKey += 1;
    STEP_KEYS.set(step, key);
  }
  return `s${key}`;
}

/** `next` replaces `step`: it keeps the step's key. */
function changed(step: MacroStepView, next: MacroStepView): MacroStepView {
  const key = STEP_KEYS.get(step);
  if (key !== undefined) STEP_KEYS.set(next, key);
  return next;
}

/** The steps with the last one's wait forced to 0 (the keyboard plays nothing after it). */
export function normalizeSteps(steps: readonly MacroStepView[]): MacroStepView[] {
  return steps.map((step, index) =>
    index === steps.length - 1 && step.delay_after_ms !== 0 ? changed(step, { ...step, delay_after_ms: 0 }) : step,
  );
}

/** The program with new steps (the last one never waits). */
export function withSteps(program: MacroProgramView, steps: readonly MacroStepView[]): MacroProgramView {
  return { play: program.play, steps: normalizeSteps(steps) };
}

/** The program a keyboard slot holds (`null` when it is empty or kept as read). */
export function slotProgram(slot: MacroSlotStateView | undefined): MacroProgramView | null {
  return slot?.content.state === "program" ? slot.content.program : null;
}

/** The library macro with the same steps and play mode (by the library's content hash, else by its steps). */
export function libraryMatch(
  program: MacroProgramView,
  library: readonly LibraryMacroView[],
  contentHash?: string,
): LibraryMacroView | undefined {
  if (contentHash !== undefined) {
    const byHash = library.find((entry) => entry.content_hash === contentHash);
    if (byHash !== undefined) return byHash;
  }
  return library.find((entry) => sameProgram(entry.program, program));
}

// ---------- editing steps ----------

/** One item of the simple view: a tap (press then release of one key), or a lone press or release. */
export interface SimpleItem {
  kind: "tap" | "press" | "release";
  usage: number;
  /** The index of its first step. */
  start: number;
  /** How many steps it covers (2 for a tap). */
  length: 1 | 2;
  /** How long the key is held (a tap's press wait, ms). */
  holdMs: number;
  /** The wait after the item (ms). */
  waitMs: number;
}

/** The steps as the simple view shows them (taps grouped). */
export function simpleItems(steps: readonly MacroStepView[]): SimpleItem[] {
  const items: SimpleItem[] = [];
  let index = 0;
  while (index < steps.length) {
    const step = steps[index];
    if (step === undefined) break;
    const next = steps[index + 1];
    if (step.motion === "press" && next !== undefined && next.motion === "release" && next.usage === step.usage) {
      items.push({ kind: "tap", usage: step.usage, start: index, length: 2, holdMs: step.delay_after_ms, waitMs: next.delay_after_ms });
      index += 2;
    } else {
      items.push({ kind: step.motion, usage: step.usage, start: index, length: 1, holdMs: 0, waitMs: step.delay_after_ms });
      index += 1;
    }
  }
  return items;
}

function moveBlock<T>(list: readonly T[], from: number, to: number): T[] {
  const next = [...list];
  const [moved] = next.splice(from, 1);
  if (moved === undefined) return [...list];
  next.splice(to, 0, moved);
  return next;
}

/**
 * Moves a step; the moved step keeps its wait, except that the step that ends
 * up last never waits.
 */
export function moveStep(steps: readonly MacroStepView[], from: number, to: number): MacroStepView[] {
  return normalizeSteps(moveBlock(steps, from, to));
}

/** Removes a step. */
export function removeStep(steps: readonly MacroStepView[], index: number): MacroStepView[] {
  return normalizeSteps(steps.filter((_, i) => i !== index));
}

/** Moves a simple-view item (its steps move together). */
export function moveItem(steps: readonly MacroStepView[], from: number, to: number): MacroStepView[] {
  const blocks = simpleItems(steps).map((item) => steps.slice(item.start, item.start + item.length));
  return normalizeSteps(moveBlock(blocks, from, to).flat());
}

/** Removes a simple-view item (both steps of a tap). */
export function removeItem(steps: readonly MacroStepView[], index: number): MacroStepView[] {
  const item = simpleItems(steps)[index];
  if (item === undefined) return [...steps];
  return normalizeSteps(steps.filter((_, i) => i < item.start || i >= item.start + item.length));
}

/**
 * Appends steps: the step that was last waits `delayMs` before them (it
 * could not wait while it was last), every new step but the last waits
 * `delayMs` too.
 */
export function appendSteps(
  steps: readonly MacroStepView[],
  added: readonly Pick<MacroStepView, "usage" | "motion">[],
  delayMs: number,
): MacroStepView[] {
  const head = steps.map((step, index) =>
    index === steps.length - 1 ? changed(step, { ...step, delay_after_ms: delayMs }) : step,
  );
  const tail = added.map((step) => ({ usage: step.usage, motion: step.motion, delay_after_ms: delayMs }));
  return normalizeSteps([...head, ...tail]);
}

/** Appends a tap of a key (press, then release). */
export function addTap(steps: readonly MacroStepView[], usage: number, delayMs: number): MacroStepView[] {
  return appendSteps(steps, [{ usage, motion: "press" }, { usage, motion: "release" }], delayMs);
}

/** Replaces one step's fields. */
export function updateStep(steps: readonly MacroStepView[], index: number, patch: Partial<MacroStepView>): MacroStepView[] {
  return normalizeSteps(steps.map((step, i) => (i === index ? changed(step, { ...step, ...patch }) : step)));
}

// ---------- live checks (the library's rules) ----------

/** Why a program can't be written as it is (step numbers are 0-based). */
export type MacroProblem =
  | { kind: "no_steps" }
  | { kind: "too_long"; steps: number; max: number }
  | { kind: "released_unpressed"; step: number; usage: number }
  | { kind: "pressed_twice"; step: number; usage: number }
  | { kind: "held"; usages: number[] }
  | { kind: "delay"; step: number }
  | { kind: "times" };

/** The keys still held after the last step, in the order they were pressed. */
export function heldAtEnd(steps: readonly MacroStepView[]): number[] {
  const held: number[] = [];
  for (const step of steps) {
    const at = held.indexOf(step.usage);
    if (step.motion === "press" && at < 0) held.push(step.usage);
    else if (step.motion === "release" && at >= 0) held.splice(at, 1);
  }
  return held;
}

/** Whether a repeat count is one the keyboard takes. */
export function validTimes(count: number, limits: MacroLimits): boolean {
  return Number.isInteger(count) && count >= 1 && count <= limits.maxTimes;
}

/** Whether a wait is one the keyboard takes. */
export function validDelay(ms: number, limits: MacroLimits): boolean {
  return Number.isInteger(ms) && ms >= 0 && ms <= limits.maxDelayMs;
}

/** Everything the library would refuse in a program (all of it, not only the first). */
export function macroProblems(program: MacroProgramView, limits: MacroLimits): MacroProblem[] {
  const problems: MacroProblem[] = [];
  const steps = program.steps;
  if (steps.length === 0) problems.push({ kind: "no_steps" });
  if (steps.length > limits.maxSteps) problems.push({ kind: "too_long", steps: steps.length, max: limits.maxSteps });
  if (program.play.mode === "times" && !validTimes(program.play.count, limits)) problems.push({ kind: "times" });
  const held: number[] = [];
  steps.forEach((step, index) => {
    const at = held.indexOf(step.usage);
    if (step.motion === "press") {
      if (at >= 0) problems.push({ kind: "pressed_twice", step: index, usage: step.usage });
      else held.push(step.usage);
    } else if (at < 0) {
      problems.push({ kind: "released_unpressed", step: index, usage: step.usage });
    } else {
      held.splice(at, 1);
    }
    if (index < steps.length - 1 && !validDelay(step.delay_after_ms, limits)) problems.push({ kind: "delay", step: index });
  });
  if (held.length > 0) problems.push({ kind: "held", usages: held });
  return problems;
}

/**
 * The "Release all at end" fix: releases every key still held, the one
 * pressed last first. `null` when the releases don't fit in the step limit.
 */
export function releaseAll(steps: readonly MacroStepView[], limits: MacroLimits): MacroStepView[] | null {
  const held = heldAtEnd(steps);
  if (held.length === 0) return [...steps];
  if (steps.length + held.length > limits.maxSteps) return null;
  const releases = [...held].reverse().map((usage) => ({ usage, motion: "release" as const }));
  return appendSteps(steps, releases, 0);
}

// ---------- the draft ----------

/** A key that runs a macro. */
export interface Binding {
  layer: LayerView;
  key: number;
}

/** What the draft does to one of the keyboard's slots. */
export interface SlotEdit {
  /** The library's slot (0-based). */
  slot: number;
  /** New content; `null` keeps what the slot holds. */
  program: MacroProgramView | null;
  /** A key that will run it (only for slots no key uses yet). */
  bind: Binding | null;
  /** Delete the macro (its keys go back to Default) or remove an unused one. */
  remove: boolean;
}

/** A new macro: the library puts it in a free slot (1-9 first) and binds its key. */
export interface NewMacro {
  id: number;
  program: MacroProgramView;
  /** The key that will run it (required before the review). */
  bind: Binding | null;
}

/** The tab's unwritten changes. */
export interface MacrosDraft {
  slots: readonly SlotEdit[];
  added: readonly NewMacro[];
  /** Critical built-ins (Fn + 1..4, Fn + Space) the user confirmed replacing. */
  acks: readonly BuiltinAckIntent[];
}

/** No changes. */
export const EMPTY_DRAFT: MacrosDraft = { slots: [], added: [], acks: [] };

/** What the editor shows: a keyboard slot or a new macro. */
export type Target = { kind: "slot"; slot: number } | { kind: "new"; id: number };

/** The keyboard's slot state. */
export function slotState(state: KeysState, slot: number): MacroSlotStateView | undefined {
  return state.macros.find((item) => item.slot === slot);
}

/** The draft's edit of a slot. */
export function editOf(draft: MacrosDraft, slot: number): SlotEdit | undefined {
  return draft.slots.find((edit) => edit.slot === slot);
}

/** The draft's new macro. */
export function newOf(draft: MacrosDraft, id: number): NewMacro | undefined {
  return draft.added.find((item) => item.id === id);
}

/** Whether a slot can get a key here (no key uses it yet). */
export function canBind(slot: MacroSlotStateView | undefined): boolean {
  return slot !== undefined && slot.bindings.length === 0;
}

/** Whether a slot holds something that can be deleted or removed. */
export function canRemove(slot: MacroSlotStateView | undefined): boolean {
  return slot !== undefined && slot.content.state !== "empty";
}

/** The program the editor shows for a target (the draft's, else the slot's, else empty). */
export function programOf(draft: MacrosDraft, state: KeysState, target: Target): MacroProgramView {
  if (target.kind === "new") return newOf(draft, target.id)?.program ?? EMPTY_PROGRAM;
  return editOf(draft, target.slot)?.program ?? slotProgram(slotState(state, target.slot)) ?? EMPTY_PROGRAM;
}

/**
 * The slot a new macro on `bind` goes into because the key already runs it
 * on its own (the library reuses that slot rather than a free one), or
 * `null` (a free slot, Macros 2–10 first). The library reuses it only when
 * Undo could write its content back, which an empty slot or one shown as
 * steps always allows; a slot kept as read may go either way, so it gives
 * `null` here and the review says which. A slot the draft deletes frees the
 * key first.
 */
export function ownMacroSlot(state: KeysState, draft: MacrosDraft, bind: Binding | null): number | null {
  if (bind === null) return null;
  const key = state.keys.find((item) => item.key.index === bind.key);
  const origin = key === undefined ? null : (bind.layer === "fn" ? key.fn_layer : key.base).origin;
  if (origin?.origin !== "macro_slot") return null;
  const slot = slotState(state, origin.slot);
  if (slot === undefined || slot.content.state === "kept_as_read" || editOf(draft, slot.slot)?.remove === true) return null;
  const [only, ...others] = slot.bindings;
  return only !== undefined && others.length === 0 && only.layer === bind.layer && only.key.index === bind.key ? slot.slot : null;
}

/** The key a target will run from (the draft's choice). */
export function bindOf(draft: MacrosDraft, target: Target): Binding | null {
  return target.kind === "new" ? (newOf(draft, target.id)?.bind ?? null) : (editOf(draft, target.slot)?.bind ?? null);
}

function cleanEdit(edit: SlotEdit, slot: MacroSlotStateView | undefined): SlotEdit | null {
  if (edit.remove) return canRemove(slot) ? { slot: edit.slot, program: null, bind: null, remove: true } : null;
  const original = slotProgram(slot);
  const program = edit.program !== null && original !== null && sameProgram(edit.program, original) ? null : edit.program;
  const bind = canBind(slot) ? edit.bind : null;
  if (program === null && bind === null) return null;
  return { slot: edit.slot, program, bind, remove: false };
}

/**
 * Changes a slot's edit: content equal to the slot's own is no change, a key
 * is kept only for a slot no key uses, removing drops the other changes; an
 * edit that changes nothing goes away.
 */
export function setSlotEdit(
  draft: MacrosDraft,
  state: KeysState,
  slot: number,
  patch: Partial<Omit<SlotEdit, "slot">>,
): MacrosDraft {
  const current = editOf(draft, slot) ?? { slot, program: null, bind: null, remove: false };
  const cleaned = cleanEdit({ ...current, ...patch, slot }, slotState(state, slot));
  const others = draft.slots.filter((edit) => edit.slot !== slot);
  const slots = cleaned === null ? others : [...others, cleaned].sort((a, b) => a.slot - b.slot);
  return { ...draft, slots };
}

/** Forgets a slot's edit. */
export function revertSlot(draft: MacrosDraft, slot: number): MacrosDraft {
  return { ...draft, slots: draft.slots.filter((edit) => edit.slot !== slot) };
}

/** Adds a new macro. */
export function addNew(draft: MacrosDraft, id: number, program: MacroProgramView = EMPTY_PROGRAM): MacrosDraft {
  return { ...draft, added: [...draft.added, { id, program, bind: null }] };
}

/** Changes a new macro. */
export function updateNew(draft: MacrosDraft, id: number, patch: Partial<Omit<NewMacro, "id">>): MacrosDraft {
  return { ...draft, added: draft.added.map((item) => (item.id === id ? { ...item, ...patch, id } : item)) };
}

/** Drops a new macro. */
export function removeNew(draft: MacrosDraft, id: number): MacrosDraft {
  return { ...draft, added: draft.added.filter((item) => item.id !== id) };
}

/** Sets the content of a target. */
export function setProgram(draft: MacrosDraft, state: KeysState, target: Target, program: MacroProgramView): MacrosDraft {
  return target.kind === "new" ? updateNew(draft, target.id, { program }) : setSlotEdit(draft, state, target.slot, { program });
}

/** Sets (or clears) the key of a target. */
export function setBind(draft: MacrosDraft, state: KeysState, target: Target, bind: Binding | null): MacrosDraft {
  return target.kind === "new" ? updateNew(draft, target.id, { bind }) : setSlotEdit(draft, state, target.slot, { bind });
}

/** Records the user's confirmation (or withdrawal) of replacing a critical built-in. */
export function setAck(draft: MacrosDraft, key: number, confirmed: boolean): MacrosDraft {
  const others = draft.acks.filter((ack) => ack.key !== key);
  return { ...draft, acks: confirmed ? [...others, { key, critical: true }] : others };
}

/** Whether a new macro or an edit counts as a change. */
function newCounts(item: NewMacro): boolean {
  return item.bind !== null || item.program.steps.length > 0;
}

/** How many macros the draft changes (the count shown with the tab's dot). */
export function draftCount(draft: MacrosDraft): number {
  return draft.slots.length + draft.added.filter(newCounts).length;
}

/**
 * The draft against a newer keys state (after a write elsewhere, a read):
 * edits that no longer apply go away (removing an empty slot, a key for a
 * slot another key took meanwhile), content equal to the slot's goes too.
 */
export function reconcile(draft: MacrosDraft, state: KeysState): MacrosDraft {
  const slots = draft.slots
    .map((edit) => cleanEdit(edit, slotState(state, edit.slot)))
    .filter((edit): edit is SlotEdit => edit !== null);
  const same = slots.length === draft.slots.length && slots.every((edit, i) => edit === draft.slots[i] || sameEdit(edit, draft.slots[i]));
  return same ? draft : { ...draft, slots };
}

function sameEdit(a: SlotEdit, b: SlotEdit | undefined): boolean {
  if (b === undefined) return false;
  const programs = a.program === null || b.program === null ? a.program === b.program : sameProgram(a.program, b.program);
  const binds = a.bind === null || b.bind === null ? a.bind === b.bind : a.bind.layer === b.bind.layer && a.bind.key === b.bind.key;
  return a.slot === b.slot && a.remove === b.remove && programs && binds;
}

/**
 * The key-assignment changes of the draft, in the order the library applies
 * them: removals first (they free slots), then the slots' new content and
 * keys, then the new macros (the library picks their slots).
 */
export function draftChanges(draft: MacrosDraft, state: KeysState): KeymapChangeIntent[] {
  const changes: KeymapChangeIntent[] = [];
  for (const edit of draft.slots) {
    if (!edit.remove) continue;
    const slot = slotState(state, edit.slot);
    if (!canRemove(slot)) continue;
    changes.push(slot !== undefined && slot.bindings.length > 0 ? change.deleteMacro(edit.slot) : change.clearMacro(edit.slot));
  }
  for (const edit of draft.slots) {
    if (edit.remove) continue;
    if (edit.program !== null && edit.bind !== null) {
      changes.push(change.bindMacro(edit.bind.layer, edit.bind.key, edit.program, edit.slot));
    } else if (edit.program !== null) {
      changes.push(change.setMacro(edit.slot, edit.program));
    } else if (edit.bind !== null) {
      changes.push(change.assign(edit.bind.layer, edit.bind.key, { kind: "macro", slot: edit.slot }));
    }
  }
  for (const item of draft.added) {
    if (item.bind !== null) changes.push(change.bindMacro(item.bind.layer, item.bind.key, item.program, null));
  }
  return changes;
}

/** What keeps a target from being reviewed. */
export interface TargetProblems {
  target: Target;
  problems: MacroProblem[];
  /** A new macro without its key. */
  needsKey: boolean;
}

/** The problems of every program the draft writes (only targets that have some). */
export function draftProblems(draft: MacrosDraft, limits: MacroLimits): TargetProblems[] {
  const found: TargetProblems[] = [];
  for (const edit of draft.slots) {
    if (edit.remove || edit.program === null) continue;
    const problems = macroProblems(edit.program, limits);
    if (problems.length > 0) found.push({ target: { kind: "slot", slot: edit.slot }, problems, needsKey: false });
  }
  for (const item of draft.added) {
    if (!newCounts(item)) continue;
    const problems = macroProblems(item.program, limits);
    if (problems.length > 0 || item.bind === null) {
      found.push({ target: { kind: "new", id: item.id }, problems, needsKey: item.bind === null });
    }
  }
  return found;
}

/**
 * The macro slots whose content the draft changes (a new macro can't go to
 * a key that runs one of them: the library would write it over the change).
 */
export function editedSlots(draft: MacrosDraft): number[] {
  return draft.slots.filter((edit) => !edit.remove && edit.program !== null).map((edit) => edit.slot);
}

/** The keys the draft gives a macro (to refuse giving one key two macros). */
export function chosenBindings(draft: MacrosDraft, except: Target | null): Binding[] {
  const chosen: Binding[] = [];
  for (const edit of draft.slots) {
    if (edit.bind === null || (except?.kind === "slot" && except.slot === edit.slot)) continue;
    chosen.push(edit.bind);
  }
  for (const item of draft.added) {
    if (item.bind === null || (except?.kind === "new" && except.id === item.id)) continue;
    chosen.push(item.bind);
  }
  return chosen;
}

/** A key on a layer to try a written macro with. */
export interface TryKey {
  layer: LayerView;
  name: string;
}

function keyView(state: KeysState, index: number): KeyAssignmentsView | undefined {
  return state.keys.find((key) => key.key.index === index);
}

/** The keys that run the macros the draft writes (for "Try it" after the write). */
export function tryKeys(draft: MacrosDraft, state: KeysState): TryKey[] {
  const keys: TryKey[] = [];
  const add = (layer: LayerView, name: string | undefined) => {
    if (name !== undefined && !keys.some((key) => key.layer === layer && key.name === name)) keys.push({ layer, name });
  };
  for (const edit of draft.slots) {
    if (edit.remove) continue;
    if (edit.bind !== null) {
      add(edit.bind.layer, keyView(state, edit.bind.key)?.key.name);
    } else if (edit.program !== null) {
      for (const binding of slotState(state, edit.slot)?.bindings ?? []) add(binding.layer, binding.key.name);
    }
  }
  for (const item of draft.added) {
    if (item.bind !== null) add(item.bind.layer, keyView(state, item.bind.key)?.key.name);
  }
  return keys;
}

/** Whether the macros of a written report can be tried now (written, on the profile that is active). */
export function canTry(report: EditReportView): boolean {
  return (
    report.anything_written &&
    (report.headline === "done" || report.headline === "maybe_lost") &&
    (report.active_profile_now === null || report.active_profile_now === report.profile)
  );
}
