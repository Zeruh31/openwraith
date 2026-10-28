// Keys > Assignments: the draft and what it changes. The draft holds the
// action each changed key should get (per layer, in the order the keys were
// first changed), the pairs made with "Swap with…" and the essential Fn
// built-ins (Fn + 1..4, Fn + Space) the user explicitly agreed to replace.
//
// Targets are absolute ("Caps Lock gets Mute"), so a draft stays meaningful
// when the keys state is read again: a target the keyboard already has is
// dropped. A swap pair is written as the library's `swap` only while both
// keys still get exactly each other's current entry; otherwise its two keys
// are written as plain assignments. Nothing here talks to the keyboard: the
// draft becomes intent changes (`KeymapChangeIntent`) that the shared
// KeymapEditFlow prepares, shows and writes.

import { sameAction } from "../../../components/actions";
import type {
  ActionValue,
  BuiltinAckIntent,
  KeyAssignmentsView,
  KeyEntryView,
  KeymapChangeIntent,
  KeysState,
  LayerView,
} from "../../../ipc/bindings";
import { change } from "../shared/intents";
import { keyAt } from "../shared/labels";

/** The action one key should get on one layer. */
export interface DraftTarget {
  layer: LayerView;
  key: number;
  action: ActionValue;
}

/** Two keys swapped with "Swap with…". */
export interface SwapPair {
  layer: LayerView;
  first: number;
  second: number;
}

/** The tab's unwritten changes. */
export interface AssignDraft {
  /** One target per changed key and layer, in the order the keys were first changed. */
  targets: readonly DraftTarget[];
  /** Pairs made with "Swap with…" (each key in at most one pair). */
  swaps: readonly SwapPair[];
  /** Fn-layer keys whose essential built-in the user explicitly agreed to replace. */
  confirmed: readonly number[];
}

/** No changes. */
export const EMPTY_DRAFT: AssignDraft = { targets: [], swaps: [], confirmed: [] };

/** A key's entry on a layer. */
export function entryOn(key: KeyAssignmentsView, layer: LayerView): KeyEntryView {
  return layer === "base" ? key.base : key.fn_layer;
}

/** Whether a key's entry on a layer can be changed (never the knob, the Fn key or an Fn-locked entry). */
export function isAssignable(key: KeyAssignmentsView, layer: LayerView): boolean {
  return layer === "base" ? key.assignable_base : key.assignable_fn;
}

/** Whether changing the key on this layer replaces one of the keyboard's essential shortcuts (Fn + 1..4, Fn + Space). */
export function isCritical(key: KeyAssignmentsView, layer: LayerView): boolean {
  return layer === "fn" && key.builtin?.critical === true;
}

/** What the keyboard has on a key now (`null`: an entry kept as read, or no such key). */
export function currentValue(state: KeysState, layer: LayerView, index: number): ActionValue | null {
  const key = keyAt(state, index);
  return key === undefined ? null : entryOn(key, layer).value;
}

/** The draft's target of a key, if it has one. */
export function targetOf(draft: AssignDraft, layer: LayerView, index: number): DraftTarget | undefined {
  return draft.targets.find((target) => target.layer === layer && target.key === index);
}

/** Whether the draft changes a key. */
export function isPending(draft: AssignDraft, layer: LayerView, index: number): boolean {
  return targetOf(draft, layer, index) !== undefined;
}

/** What a key will do after the draft is written (`null`: kept as read). */
export function effectiveValue(state: KeysState, draft: AssignDraft, layer: LayerView, index: number): ActionValue | null {
  return targetOf(draft, layer, index)?.action ?? currentValue(state, layer, index);
}

function inPair(pair: SwapPair, layer: LayerView, index: number): boolean {
  return pair.layer === layer && (pair.first === index || pair.second === index);
}

function withoutPairsOf(swaps: readonly SwapPair[], layer: LayerView, indices: readonly number[]): SwapPair[] {
  return swaps.filter((pair) => !indices.some((index) => inPair(pair, layer, index)));
}

/**
 * Sets what a key should do. Choosing what the keyboard already has removes
 * the key from the draft; a key that was part of a swap leaves the pair
 * (its partner keeps its own target).
 */
export function setTarget(
  state: KeysState,
  draft: AssignDraft,
  layer: LayerView,
  index: number,
  action: ActionValue,
): AssignDraft {
  const swaps = withoutPairsOf(draft.swaps, layer, [index]);
  const current = currentValue(state, layer, index);
  const unchanged = current !== null && sameAction(current, action);
  const existing = draft.targets.findIndex((target) => target.layer === layer && target.key === index);
  let targets: DraftTarget[];
  if (unchanged) {
    targets = draft.targets.filter((_, i) => i !== existing);
  } else if (existing >= 0) {
    targets = draft.targets.map((target, i) => (i === existing ? { layer, key: index, action } : target));
  } else {
    targets = [...draft.targets, { layer, key: index, action }];
  }
  return { ...draft, targets, swaps };
}

/** Takes a key out of the draft (it keeps what the keyboard has). */
export function dropTarget(draft: AssignDraft, layer: LayerView, index: number): AssignDraft {
  if (!isPending(draft, layer, index)) return draft;
  return {
    ...draft,
    targets: draft.targets.filter((target) => !(target.layer === layer && target.key === index)),
    swaps: withoutPairsOf(draft.swaps, layer, [index]),
  };
}

/** Whether two keys can swap what they do (both writable, and they don't already do the same). */
export function canSwap(state: KeysState, draft: AssignDraft, layer: LayerView, first: number, second: number): boolean {
  if (first === second) return false;
  const a = effectiveValue(state, draft, layer, first);
  const b = effectiveValue(state, draft, layer, second);
  return a !== null && b !== null && !sameAction(a, b);
}

/**
 * Swaps what two keys do (after the draft's own changes). Keys kept as read
 * can't be moved: the draft is returned unchanged then.
 */
export function swapKeys(state: KeysState, draft: AssignDraft, layer: LayerView, first: number, second: number): AssignDraft {
  if (!canSwap(state, draft, layer, first, second)) return draft;
  const a = effectiveValue(state, draft, layer, first);
  const b = effectiveValue(state, draft, layer, second);
  if (a === null || b === null) return draft;
  let next = setTarget(state, draft, layer, first, b);
  next = setTarget(state, next, layer, second, a);
  if (!isPending(next, layer, first) || !isPending(next, layer, second)) return next;
  return { ...next, swaps: [...next.swaps, { layer, first, second }] };
}

/** Records (or withdraws) the explicit agreement to replace a key's essential Fn built-in. */
export function setConfirmed(draft: AssignDraft, index: number, confirmed: boolean): AssignDraft {
  const has = draft.confirmed.includes(index);
  if (has === confirmed) return draft;
  return {
    ...draft,
    confirmed: confirmed ? [...draft.confirmed, index] : draft.confirmed.filter((key) => key !== index),
  };
}

/** Adds the critical acknowledgements the write flow learned (the user ticked them there). */
export function withConfirmed(draft: AssignDraft, acks: readonly BuiltinAckIntent[]): AssignDraft {
  let next = draft;
  for (const ack of acks) {
    if (ack.critical) next = setConfirmed(next, ack.key, true);
  }
  return next;
}

/** The critical acknowledgements the user gave (for the write flow). */
export function confirmedAcks(draft: AssignDraft): BuiltinAckIntent[] {
  return draft.confirmed.map((key) => ({ key, critical: true }));
}

/** Whether the user agreed to replace the essential built-in of a key. */
export function isConfirmed(draft: AssignDraft, index: number): boolean {
  return draft.confirmed.includes(index);
}

/** How many key entries the draft changes. */
export function changeCount(draft: AssignDraft): number {
  return draft.targets.length;
}

/** Whether a pair still swaps exactly the two keys' current entries. */
function isPureSwap(state: KeysState, draft: AssignDraft, pair: SwapPair): boolean {
  const a = targetOf(draft, pair.layer, pair.first);
  const b = targetOf(draft, pair.layer, pair.second);
  const nowA = currentValue(state, pair.layer, pair.first);
  const nowB = currentValue(state, pair.layer, pair.second);
  return (
    a !== undefined &&
    b !== undefined &&
    nowA !== null &&
    nowB !== null &&
    sameAction(a.action, nowB) &&
    sameAction(b.action, nowA)
  );
}

/**
 * The draft against a newer keys state: targets the keyboard now has, keys
 * that can't be changed any more and pairs without both keys are dropped.
 * Returns the same object when nothing changes.
 */
export function normalizeDraft(state: KeysState, draft: AssignDraft): AssignDraft {
  const targets = draft.targets.filter((target) => {
    const key = keyAt(state, target.key);
    if (key === undefined || !isAssignable(key, target.layer)) return false;
    const current = entryOn(key, target.layer).value;
    return current === null || !sameAction(current, target.action);
  });
  const swaps = draft.swaps.filter(
    (pair) =>
      targets.some((t) => t.layer === pair.layer && t.key === pair.first) &&
      targets.some((t) => t.layer === pair.layer && t.key === pair.second),
  );
  const confirmed = draft.confirmed.filter((index) => keyAt(state, index)?.builtin?.critical === true);
  if (
    targets.length === draft.targets.length &&
    swaps.length === draft.swaps.length &&
    confirmed.length === draft.confirmed.length
  ) {
    return draft;
  }
  return { targets, swaps, confirmed };
}

/**
 * The intent changes of the draft, in the order the keys were changed: a
 * pair that still swaps both keys' current entries as one `swap`, the
 * default as `reset`, everything else as `assign`.
 */
export function draftChanges(state: KeysState, draft: AssignDraft): KeymapChangeIntent[] {
  const changes: KeymapChangeIntent[] = [];
  const done = new Set<string>();
  const id = (layer: LayerView, index: number) => `${layer}:${index}`;
  for (const target of draft.targets) {
    if (done.has(id(target.layer, target.key))) continue;
    const pair = draft.swaps.find((p) => inPair(p, target.layer, target.key));
    if (pair !== undefined && isPureSwap(state, draft, pair)) {
      changes.push(change.swap(pair.layer, pair.first, pair.second));
      done.add(id(pair.layer, pair.first));
      done.add(id(pair.layer, pair.second));
      continue;
    }
    done.add(id(target.layer, target.key));
    changes.push(
      target.action.kind === "default"
        ? change.reset(target.layer, target.key)
        : change.assign(target.layer, target.key, target.action),
    );
  }
  return changes;
}

/** A key on a layer. */
export interface LayerKey {
  layer: LayerView;
  key: number;
}

/**
 * The keys that will use a macro or advanced-key slot after the draft (the
 * library allows one key per slot).
 */
export function slotUsers(state: KeysState, draft: AssignDraft, kind: "macro" | "advanced", slot: number): LayerKey[] {
  const users: LayerKey[] = [];
  for (const key of state.keys) {
    for (const layer of ["base", "fn"] as const) {
      const value = effectiveValue(state, draft, layer, key.key.index);
      if (value?.kind === kind && value.slot === slot) users.push({ layer, key: key.key.index });
    }
  }
  return users;
}
