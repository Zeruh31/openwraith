// The SOCD tab's logic, as pure functions: the draft (the power switch, which
// is one setting of the whole keyboard, and the pairs of the active
// profile), the site's rules for pairs (two different keys, a key in one
// pair only, at most 20 pairs, modes 1-4), quick-add pairs, and the intent
// for `prepare_socd_edit`. The backend checks the same rules again before
// any traffic and is the authority.

import type { EvidenceView, SocdEditIntent, SocdKeyView, SocdPairView, SocdStateView } from "../../../ipc/bindings";

/** One pair being edited (`id` keeps rows apart while they are edited). */
export interface SocdPairDraft {
  id: number;
  /** Key index of key 1, `null` until chosen. */
  key1: number | null;
  /** Key index of key 2, `null` until chosen. */
  key2: number | null;
  /** The resolution mode, 1-4. */
  mode: number;
}

/** The unwritten change. */
export interface SocdDraft {
  /** The new power (every profile), `null` when it doesn't change. */
  power: boolean | null;
  /** The pairs after the edit, `null` when they don't change. */
  pairs: readonly SocdPairDraft[] | null;
  /** Replace pairs that aren't in the site's form although Undo can't write them back. */
  allowNotUndoable: boolean;
}

/** No change. */
export const EMPTY_SOCD: SocdDraft = { power: null, pairs: null, allowNotUndoable: false };

/** The resolution modes (the vendor site's numbers). */
export const SOCD_MODES = [1, 2, 3, 4] as const;

/** The mode new pairs get: 1, "last input wins" (confirmed on hardware). */
export const DEFAULT_MODE = 1;

/** The quick-add pairs: A + D, W + S, Left + Right (key indices). */
export const QUICK_PAIRS = {
  ad: [68, 70],
  ws: [46, 69],
  arrows: [123, 125],
} as const satisfies Record<string, readonly [number, number]>;

/** A quick-add pair. */
export type QuickPair = keyof typeof QUICK_PAIRS;

/** Whether the draft changes anything (the page drops parts that equal the keyboard's). */
export function isSocdDirty(draft: SocdDraft): boolean {
  return draft.power !== null || draft.pairs !== null;
}

/** The pairs of the state as draft rows (ids 1..n). */
export function pairsOf(state: SocdStateView): SocdPairDraft[] {
  return state.pairs.map((pair, i) => ({
    id: i + 1,
    key1: pair.key1.key?.index ?? null,
    key2: pair.key2.key?.index ?? null,
    mode: pair.mode,
  }));
}

/** Whether draft rows equal the state's pairs (same keys and modes, same order). */
export function samePairs(rows: readonly SocdPairDraft[], pairs: readonly SocdPairView[]): boolean {
  return (
    rows.length === pairs.length &&
    rows.every((row, i) => {
      const pair = pairs[i];
      return (
        pair !== undefined &&
        row.key1 === (pair.key1.key?.index ?? null) &&
        row.key2 === (pair.key2.key?.index ?? null) &&
        row.mode === pair.mode
      );
    })
  );
}

/**
 * The draft after a change, with parts that equal the keyboard's dropped
 * (so the draft is "dirty" only when it changes something).
 */
export function normalize(draft: SocdDraft, state: SocdStateView): SocdDraft {
  const power = draft.power === state.power ? null : draft.power;
  const pairs = draft.pairs !== null && state.pairs_writable && samePairs(draft.pairs, state.pairs) ? null : draft.pairs;
  return { power, pairs, allowNotUndoable: pairs === null ? false : draft.allowNotUndoable };
}

/** The rows shown: the draft's pairs, else the keyboard's (none when they can't be shown as keys). */
export function rowsOf(draft: SocdDraft, state: SocdStateView): readonly SocdPairDraft[] {
  if (draft.pairs !== null) return draft.pairs;
  return state.pairs_writable ? pairsOf(state) : [];
}

/** The next free row id. */
export function nextId(rows: readonly SocdPairDraft[]): number {
  return rows.reduce((max, row) => Math.max(max, row.id), 0) + 1;
}

/** What is wrong with a row, by the site's rules. */
export type PairProblem =
  | { kind: "incomplete" }
  | { kind: "same_key"; key: number }
  /** The key is also in pair `pair` (1-based). */
  | { kind: "duplicate"; key: number; pair: number }
  | { kind: "mode" };

/** The rows' problems by row id, and whether there are too many rows. */
export interface PairCheck {
  problems: ReadonlyMap<number, readonly PairProblem[]>;
  tooMany: boolean;
  /** Every rule holds. */
  ok: boolean;
}

/** Checks the rows against the site's rules (`max`: the most pairs, 20). */
export function checkPairs(rows: readonly SocdPairDraft[], max: number): PairCheck {
  const problems = new Map<number, PairProblem[]>();
  const firstPair = new Map<number, number>();
  rows.forEach((row, i) => {
    const list: PairProblem[] = [];
    if (row.key1 === null || row.key2 === null) list.push({ kind: "incomplete" });
    if (row.key1 !== null && row.key1 === row.key2) list.push({ kind: "same_key", key: row.key1 });
    for (const key of new Set([row.key1, row.key2])) {
      if (key === null) continue;
      const seen = firstPair.get(key);
      if (seen === undefined) firstPair.set(key, i + 1);
      else if (seen !== i + 1) list.push({ kind: "duplicate", key, pair: seen });
    }
    if (!(SOCD_MODES as readonly number[]).includes(row.mode)) list.push({ kind: "mode" });
    if (list.length > 0) problems.set(row.id, list);
  });
  const tooMany = rows.length > max;
  return { problems, tooMany, ok: problems.size === 0 && !tooMany };
}

/** The key indices used by rows other than `except` (row id), with their 1-based pair number. */
export function usedKeys(rows: readonly SocdPairDraft[], except: number | null): Map<number, number> {
  const used = new Map<number, number>();
  rows.forEach((row, i) => {
    if (row.id === except) return;
    for (const key of [row.key1, row.key2]) if (key !== null && !used.has(key)) used.set(key, i + 1);
  });
  return used;
}

/** Whether a quick-add pair can be added: both keys free and room for one more. */
export function canQuickAdd(rows: readonly SocdPairDraft[], pair: QuickPair, max: number, offered: ReadonlySet<number>): boolean {
  const [a, b] = QUICK_PAIRS[pair];
  const used = usedKeys(rows, null);
  return rows.length < max && offered.has(a) && offered.has(b) && !used.has(a) && !used.has(b);
}

/** The key indices a pair can use (layout keys with a HID usage). */
export function offeredKeys(state: SocdStateView): Set<number> {
  const keys = new Set<number>();
  for (const view of state.keys) if (view.key !== null) keys.add(view.key.index);
  return keys;
}

/** A mode's evidence from the state (the library's, never a list kept here). */
export function modeEvidence(state: SocdStateView, mode: number): EvidenceView | null {
  return state.modes.find((view) => view.mode === mode)?.evidence ?? null;
}

/** The power shown: the draft's, else the keyboard's. */
export function shownPower(draft: SocdDraft, state: SocdStateView): boolean {
  return draft.power ?? state.power;
}

/**
 * The intent for `prepare_socd_edit`: only what changes, the persistence
 * option and the revision of the state the user saw. `null` when there is
 * nothing to send or a row breaks a rule.
 */
export function buildSocdIntent(state: SocdStateView, draft: SocdDraft, checkPersistence: boolean): SocdEditIntent | null {
  const normalized = normalize(draft, state);
  if (!isSocdDirty(normalized)) return null;
  let pairs: SocdEditIntent["pairs"] = null;
  if (normalized.pairs !== null) {
    if (!checkPairs(normalized.pairs, state.max_pairs).ok) return null;
    pairs = normalized.pairs.map((row) => ({ key1: row.key1 ?? 0, key2: row.key2 ?? 0, mode: row.mode }));
  }
  return {
    power: normalized.power,
    pairs,
    allow_not_undoable: normalized.allowNotUndoable,
    check_persistence: checkPersistence,
    based_on: state.revision,
  };
}

/** Whether two pair lists of a plan are the same (by HID usage and mode). */
export function samePlanPairs(a: readonly SocdPairView[], b: readonly SocdPairView[]): boolean {
  return (
    a.length === b.length &&
    a.every((pair, i) => {
      const other = b[i];
      return (
        other !== undefined &&
        pair.key1.usage === other.key1.usage &&
        pair.key2.usage === other.key2.usage &&
        pair.mode === other.mode
      );
    })
  );
}

/** The library key name of a pair's key, or `null` for a usage that isn't on the layout. */
export function socdKeyName(view: SocdKeyView): string | null {
  return view.key?.name ?? null;
}
