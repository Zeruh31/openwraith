// The dead band tab's logic, as pure functions: the draft (a new top and/or
// bottom dead zone for the selected keys), what it does to each key, the
// intent for `prepare_dead_band_edit`, selection helpers, and the soft
// warnings against each key's actuation point. The dead band belongs to the
// active profile (confirmed on hardware).
//
// Nothing here talks to the keyboard; the backend plans the edit and is the
// authority (values above its `max` are refused, never clamped).

import { ALL_KEYS, KNOB_INDEX, onlySelectable, type Selection } from "../../../components/keyboard";
import { W75_TRAVEL_MM } from "../../../components/TravelGauge";
import type {
  DeadBandEditIntent,
  DeadBandHintKind,
  DeadBandState,
  DeadBandValue,
  KeyDeadBandView,
  KeyTriggerView,
} from "../../../ipc/bindings";

/** The unwritten change: applied to every selected key. */
export interface DeadBandDraft {
  /** New top dead zone in trigger units; `null` keeps each key's own. */
  top: number | null;
  /** New bottom dead zone in trigger units; `null` keeps each key's own. */
  bottom: number | null;
}

/** No change. */
export const EMPTY_DEAD_BAND: DeadBandDraft = { top: null, bottom: null };

/** What the keys show. */
export type DeadBandShow = "top" | "bottom";

/** The keyboard's distance unit when the state has none (0.02 mm on the W75). */
export const DEFAULT_UNIT_UM = 20;

/** Whether the draft changes anything. */
export function isDeadBandDirty(draft: DeadBandDraft): boolean {
  return draft.top !== null || draft.bottom !== null;
}

/** The keys of a state by index (every layout key but the knob). */
export function deadBandKeys(state: DeadBandState | null): Map<number, KeyDeadBandView> {
  return new Map((state?.keys ?? []).map((key) => [key.key.index, key]));
}

/** The distance unit for showing values, in micrometres. */
export function deadBandUnit(state: DeadBandState | null): number {
  return state?.unit_um ?? DEFAULT_UNIT_UM;
}

/** The full key travel in trigger units (170 = 3.40 mm on the W75). */
export function travelUnits(unitUm: number): number {
  return Math.round((W75_TRAVEL_MM * 1000) / unitUm);
}

/** A key's values after the draft. */
export function targetOf(value: DeadBandValue, draft: DeadBandDraft): DeadBandValue {
  return { top: draft.top ?? value.top, bottom: draft.bottom ?? value.bottom };
}

/** Whether two values are the same. */
export function sameDeadBand(a: DeadBandValue, b: DeadBandValue): boolean {
  return a.top === b.top && a.bottom === b.bottom;
}

/** Whether a key has a dead zone. */
export function hasDeadZone(value: DeadBandValue): boolean {
  return value.top > 0 || value.bottom > 0;
}

/** The keys that can be selected (every key the state lists; all layout keys but the knob before a read). */
export function selectableOf(keys: ReadonlyMap<number, KeyDeadBandView>): Set<number> {
  if (keys.size === 0) return new Set(ALL_KEYS.filter((index) => index !== KNOB_INDEX));
  return new Set(ALL_KEYS.filter((index) => keys.has(index)));
}

/** The keys that have a dead zone. */
export function zonedKeys(keys: ReadonlyMap<number, KeyDeadBandView>): Set<number> {
  const found = new Set<number>();
  for (const [index, key] of keys) if (hasDeadZone(key.value)) found.add(index);
  return found;
}

/** Every key whose values equal those of one of the selected keys. */
export function sameDeadBandKeys(
  selection: Selection,
  keys: ReadonlyMap<number, KeyDeadBandView>,
  selectable: ReadonlySet<number>,
): Set<number> {
  const wanted = new Set<string>();
  for (const index of selection) {
    const key = keys.get(index);
    if (key !== undefined) wanted.add(`${key.value.top}/${key.value.bottom}`);
  }
  return onlySelectable(
    [...keys.values()].filter((key) => wanted.has(`${key.value.top}/${key.value.bottom}`)).map((key) => key.key.index),
    selectable,
  );
}

/** What the draft does to the selected keys. */
export interface DeadBandSummary {
  /** Selected keys the state lists: they go into the intent (ascending index). */
  keys: number[];
  /** Of those, the keys whose values change. */
  changing: number[];
  /** Of those, the keys that already have the drafted values. */
  unchanged: number[];
}

/** Sorts the selection and splits it into changing and unchanged keys. */
export function summarize(
  selection: Selection,
  keys: ReadonlyMap<number, KeyDeadBandView>,
  draft: DeadBandDraft,
): DeadBandSummary {
  const summary: DeadBandSummary = { keys: [], changing: [], unchanged: [] };
  const dirty = isDeadBandDirty(draft);
  for (const index of [...selection].sort((a, b) => a - b)) {
    const key = keys.get(index);
    if (key === undefined) continue;
    summary.keys.push(index);
    if (dirty && !sameDeadBand(targetOf(key.value, draft), key.value)) summary.changing.push(index);
    else summary.unchanged.push(index);
  }
  return summary;
}

/**
 * The intent for `prepare_dead_band_edit`: the selected keys, only the
 * values the draft changes, the persistence option and the revision of the
 * state the user saw. `null` when there is nothing to send.
 */
export function buildDeadBandIntent(
  summary: DeadBandSummary,
  draft: DeadBandDraft,
  basedOn: number,
  checkPersistence: boolean,
): DeadBandEditIntent | null {
  if (!isDeadBandDirty(draft) || summary.keys.length === 0) return null;
  return {
    keys: [...summary.keys],
    top: draft.top,
    bottom: draft.bottom,
    check_persistence: checkPersistence,
    based_on: basedOn,
  };
}

/** The values of the selected keys the state lists. */
export function selectedDeadBands(selection: Selection, keys: ReadonlyMap<number, KeyDeadBandView>): DeadBandValue[] {
  const values: DeadBandValue[] = [];
  for (const index of [...selection].sort((a, b) => a - b)) {
    const key = keys.get(index);
    if (key !== undefined) values.push(key.value);
  }
  return values;
}

/** The value a slider starts from: the first selected key's (the common one when they share it), else 0. */
export function initialValue(values: readonly DeadBandValue[], field: "top" | "bottom", max: number): number {
  const first = values[0]?.[field] ?? 0;
  return Math.min(max, Math.max(0, first));
}

/** A soft warning: a dead zone that reaches the key's actuation point (never a refusal). */
export interface DeadBandHint {
  key: number;
  kind: DeadBandHintKind;
  /** The key's actuation point, in trigger units. */
  actuation: number;
}

/**
 * The soft warnings of the draft for the changing keys, from each key's
 * actuation point (the trigger settings of the last read): a top dead zone
 * at or past the actuation point, or a bottom dead zone deeper than the
 * travel left below it. The backend computes the same warnings for the
 * review; these are the page's preview.
 */
export function localHints(
  summary: DeadBandSummary,
  keys: ReadonlyMap<number, KeyDeadBandView>,
  triggers: ReadonlyMap<number, KeyTriggerView>,
  draft: DeadBandDraft,
  travel: number,
): DeadBandHint[] {
  const hints: DeadBandHint[] = [];
  for (const index of summary.changing) {
    const key = keys.get(index);
    const actuation = triggers.get(index)?.value?.actuation;
    if (key === undefined || actuation === undefined) continue;
    const target = targetOf(key.value, draft);
    if (target.top > 0 && target.top >= actuation) hints.push({ key: index, kind: "top_reaches_actuation", actuation });
    if (target.bottom > 0 && target.bottom > travel - actuation) {
      hints.push({ key: index, kind: "bottom_reaches_actuation", actuation });
    }
  }
  return hints;
}
