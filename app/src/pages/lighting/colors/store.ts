// Lighting > Key colours: the keyboard's colours (`get_colors_state`, from
// the last full read, no keyboard traffic) and the tab's draft.
//
// The colours are fetched again whenever the status announces a new
// snapshot revision, and after this tab's own write (a colour write patches
// the colours without a new snapshot). The draft lives outside the
// component so that it survives switching tabs and pages; it belongs to one
// profile of one connection (its basis) and is discarded, with a notice,
// when the keyboard is on another profile or was connected again, whether
// the tab is shown or not. The draft keeps an undo/redo history.

import { useEffect } from "react";

import { getDict } from "../../../i18n";
import type { AppError, ColorsState } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { registerDraft } from "../../../state/drafts";
import { snapshotStore } from "../../../state/snapshot";
import { statusStore } from "../../../state/status";
import { createStore, useStore } from "../../../state/store";
import { toast } from "../../../state/toasts";
import { basisOf } from "../draftStore";
import {
  EMPTY_HISTORY,
  keyboardColors,
  normalizeHistory,
  record,
  redo as redoStep,
  undo as undoStep,
  type ColorHistory,
  type ColorMap,
} from "./model";

// ---------- the keyboard's colours ----------

interface ColorsStoreState {
  state: ColorsState | null;
  /** The status's snapshot revision the colours were fetched for. */
  fetchedFor: number | null;
  loading: boolean;
  error: AppError | null;
}

const NO_COLORS: ColorsStoreState = { state: null, fetchedFor: null, loading: false, error: null };

/** The keyboard's colours (tests set it directly). */
export const colorsStore = createStore<ColorsStoreState>(NO_COLORS);

let requested: number | null = null;
/** Bumped by every fetch, so an answer to an older one is ignored. */
let fetchSeq = 0;

function fetchColors(revision: number): void {
  fetchSeq += 1;
  const seq = fetchSeq;
  requested = revision;
  colorsStore.set({ ...colorsStore.get(), loading: true });
  api
    .getColorsState()
    .then((state) => {
      if (seq !== fetchSeq) return;
      requested = null;
      colorsStore.set({ state, fetchedFor: revision, loading: false, error: null });
    })
    .catch((error: unknown) => {
      if (seq !== fetchSeq) return;
      requested = null;
      colorsStore.set({ ...colorsStore.get(), fetchedFor: revision, loading: false, error: toAppError(error) });
    });
}

/** Fetches the colours when the status's snapshot revision differs from the one they were fetched for. */
export function syncColors(): void {
  const revision = statusStore.get().snapshot_revision;
  const current = colorsStore.get();
  if (revision === null) {
    requested = null;
    fetchSeq += 1;
    if (current.state !== null || current.loading || current.fetchedFor !== null || current.error !== null) {
      colorsStore.set(NO_COLORS);
    }
    return;
  }
  if (current.fetchedFor === revision || requested === revision) return;
  fetchColors(revision);
}

let syncing = false;

/** Keeps the colours in step with the status (once per app run; started by the tab). */
export function startColorsSync(): void {
  if (syncing) return;
  syncing = true;
  statusStore.subscribe(syncColors);
  syncColors();
}

/** Fetches the colours again (after this tab's write, or a refusal that said they were out of date). */
export function refetchColors(): void {
  const revision = statusStore.get().snapshot_revision;
  if (revision === null) {
    syncColors();
    return;
  }
  fetchColors(revision);
}

/** The colours as the tab sees them. */
export interface ColorsView {
  state: ColorsState | null;
  loading: boolean;
  error: AppError | null;
}

/** The keyboard's colours; starts the sync on first use. */
export function useColorsState(): ColorsView {
  const store = useStore(colorsStore);
  useEffect(() => {
    startColorsSync();
  }, []);
  return { state: store.state, loading: store.loading, error: store.error };
}

// ---------- the draft ----------

/** The tab's draft and the profile it was made for. */
export interface ColorsDraftState {
  /** `"<generation>:<profile>"` of the snapshot the draft was made on; `null` before the first one. */
  basis: string | null;
  history: ColorHistory;
  /** "Also switch the effect to Custom" (opt-in, never pre-ticked). */
  switchToCustom: boolean;
}

const NO_DRAFT: ColorsDraftState = { basis: null, history: EMPTY_HISTORY, switchToCustom: false };

const draftStore = createStore<ColorsDraftState>(NO_DRAFT);

// The shell's draft registry (nav dot, tab dot, the profile-switch
// confirmation with the count) knows about the draft as long as it exists.
let unregister: (() => void) | null = null;
let registeredCount = 0;
draftStore.subscribe(() => {
  const count = draftStore.get().history.present.size;
  if (count === registeredCount) return;
  registeredCount = count;
  const remove = unregister;
  unregister = count > 0 ? registerDraft("lighting", discardColorsDraft, { tab: "colors", count }) : null;
  // Registering again replaces the entry; the old remover then does nothing.
  if (count === 0) remove?.();
});

/** The draft state; re-renders on change. */
export function useColorsDraft(): ColorsDraftState {
  return useStore(draftStore);
}

/** The draft state (outside React). */
export function getColorsDraft(): ColorsDraftState {
  return draftStore.get();
}

/** Applies an edit made on `basis` (recorded for undo); a draft of another basis is not built upon. */
export function editColors(basis: string | null, update: (draft: ColorMap) => ColorMap): void {
  draftStore.set((state) => {
    const same = basis === null || state.basis === null || state.basis === basis;
    const history = same ? state.history : EMPTY_HISTORY;
    const next = record(history, update(history.present));
    return next === state.history && same ? state : { ...state, basis: basis ?? state.basis, history: next };
  });
}

/** Undoes the last edit of the draft. */
export function undoColors(): void {
  draftStore.set((state) => {
    const history = undoStep(state.history);
    return history === state.history ? state : { ...state, history };
  });
}

/** Redoes the last undone edit of the draft. */
export function redoColors(): void {
  draftStore.set((state) => {
    const history = redoStep(state.history);
    return history === state.history ? state : { ...state, history };
  });
}

/** Sets "Also switch the effect to Custom". */
export function setSwitchToCustom(on: boolean): void {
  draftStore.set((state) => (state.switchToCustom === on ? state : { ...state, switchToCustom: on }));
}

/** Discards the draft and its history (the shell's discard, a spent draft). */
export function discardColorsDraft(): void {
  draftStore.set((state) =>
    state.history === EMPTY_HISTORY && !state.switchToCustom ? state : { ...state, history: EMPTY_HISTORY, switchToCustom: false },
  );
}

/** Spends the draft after a write (the keyboard has the colours now; undo is the report's Undo). */
export function spendColorsDraft(): void {
  discardColorsDraft();
}

/**
 * Moves the draft to a new basis. Returns whether a draft made on another
 * basis was discarded.
 */
export function rebaseColors(basis: string): boolean {
  const state = draftStore.get();
  if (state.basis === basis) return false;
  if (state.basis === null) {
    draftStore.set({ ...state, basis });
    return false;
  }
  const dropped = state.history.present.size > 0;
  draftStore.set({ basis, history: EMPTY_HISTORY, switchToCustom: false });
  return dropped;
}

/** Whether the tab's write flow is open (no "discarded" notice then: the flow explains it). */
let flowOpen = false;

/** Marks the tab's write flow as open or closed. */
export function setColorsFlowOpen(open: boolean): void {
  flowOpen = open;
}

/** Follows the keyboard: another profile or a new connection discards the draft for good. */
function followSnapshot(): void {
  const basis = basisOf(snapshotStore.get().snapshot);
  if (basis !== null && rebaseColors(basis) && !flowOpen) {
    const words = getDict().lighting.colors.discarded;
    toast({ kind: "info", title: words.title, body: words.body });
  }
}

/** Follows new colours of the same basis: keys that now have their wanted colour leave the draft. */
function followColors(): void {
  const colors = colorsStore.get().state;
  const draft = draftStore.get();
  if (colors === null || draft.basis === null || `${colors.generation}:${colors.profile}` !== draft.basis) return;
  const history = normalizeHistory(draft.history, keyboardColors(colors));
  if (history !== draft.history) draftStore.set({ ...draft, history });
}

snapshotStore.subscribe(followSnapshot);
colorsStore.subscribe(followColors);
followSnapshot();

/** Seeds the draft once (browser-preview scenarios); ignored when a draft exists. */
export function seedColorsDraft(colors: ColorMap, switchToCustom: boolean): void {
  const state = draftStore.get();
  if (state.history.present.size > 0 || colors.size === 0) return;
  draftStore.set({ ...state, history: { present: colors, past: [], future: [] }, switchToCustom });
}

/** Resets both stores (tests). */
export function resetColorsStores(): void {
  requested = null;
  fetchSeq += 1;
  colorsStore.set(NO_COLORS);
  draftStore.set(NO_DRAFT);
  flowOpen = false;
}

