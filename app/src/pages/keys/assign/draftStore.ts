// Keys > Assignments: the draft, the shown layer and the selected key, kept
// outside the component so that they survive switching tabs and pages (as
// the Actuation and Lighting drafts do). A draft belongs to one profile of
// one connection (`generation/profile` of the keys state it was made on):
// when the keyboard is on another profile or was connected again, the draft
// no longer applies and is discarded, with a toast unless the write flow is
// open. A new read of the same profile keeps the draft and drops only the
// changes the keyboard already has.
//
// The shell's draft registry (nav dot, tab dot, the profile-switch
// confirmation "Keys > Assignments: 2 unwritten changes") knows about the
// draft whether the tab is shown or not.

import { getDict } from "../../../i18n";
import type { KeysState, LayerView } from "../../../ipc/bindings";
import { registerDraft } from "../../../state/drafts";
import { createStore, useStore } from "../../../state/store";
import { toast } from "../../../state/toasts";
import { keysStore } from "../shared/keysState";
import { changeCount, EMPTY_DRAFT, normalizeDraft, type AssignDraft } from "./model";

/** The tab's state. */
export interface AssignState {
  /** `generation/profile` of the keys state the draft was made on; `null` adopts the next one. */
  owner: string | null;
  draft: AssignDraft;
  /** The layer the board shows. */
  layer: LayerView;
  /** The selected key (single selection), if any. */
  selected: number | null;
}

const INITIAL: AssignState = { owner: null, draft: EMPTY_DRAFT, layer: "base", selected: null };

const store = createStore<AssignState>(INITIAL);

/** The owner key of a keys state. */
export function ownerOf(state: Pick<KeysState, "generation" | "profile"> | null): string | null {
  return state === null ? null : `${state.generation}/${state.profile}`;
}

/** The tab's state; re-renders on change. */
export function useAssignState(): AssignState {
  return useStore(store);
}

/** The tab's state (outside React; tests). */
export function assignState(): AssignState {
  return store.get();
}

/** Changes the draft (made on the keys state read last). */
export function updateDraft(update: (draft: AssignDraft) => AssignDraft): void {
  store.set((state) => {
    const draft = update(state.draft);
    if (draft === state.draft) return state;
    return { ...state, owner: state.owner ?? ownerOf(keysStore.get().state), draft };
  });
}

/** Shows a layer. */
export function setLayer(layer: LayerView): void {
  store.set((state) => (state.layer === layer ? state : { ...state, layer }));
}

/** Selects a key (or none). */
export function setSelected(selected: number | null): void {
  store.set((state) => (state.selected === selected ? state : { ...state, selected }));
}

/** Discards the draft (the layer and the selection stay). */
export function discardDraft(): void {
  store.set((state) => (state.draft === EMPTY_DRAFT ? state : { ...state, draft: EMPTY_DRAFT }));
}

let flowOpen = false;

/**
 * Marks the tab's write flow as open or closed: while it is open, a profile
 * or connection change comes from (or is explained by) that flow, so there
 * is no "discarded" toast then.
 */
export function setFlowOpen(open: boolean): void {
  flowOpen = open;
}

/** What a browser-preview scenario presets. */
export interface AssignSeedState {
  draft?: AssignDraft | undefined;
  layer?: LayerView | undefined;
  selected?: number | undefined;
}

/** Seeds the state once (browser-preview scenarios); ignored when a draft exists. */
export function seedAssign(seed: AssignSeedState): void {
  if (seed.draft === undefined && seed.layer === undefined && seed.selected === undefined) return;
  const state = store.get();
  if (changeCount(state.draft) > 0) return;
  store.set({
    owner: state.owner,
    draft: seed.draft ?? state.draft,
    layer: seed.layer ?? state.layer,
    selected: seed.selected ?? state.selected,
  });
}

function profileOf(owner: string): number {
  return Number(owner.split("/")[1]);
}

/**
 * Follows the keys state: a draft made on another profile or connection is
 * discarded for good; on the same profile, changes the keyboard already has
 * are dropped from it.
 */
function followKeys(): void {
  const keys = keysStore.get().state;
  const current = ownerOf(keys);
  const state = store.get();
  if (keys === null || current === null) return;
  if (state.owner === null) {
    store.set({ ...state, owner: current, draft: normalizeDraft(keys, state.draft) });
    return;
  }
  if (state.owner !== current) {
    const previous = state.owner;
    const discarded = changeCount(state.draft) > 0;
    store.set({ ...state, owner: current, draft: EMPTY_DRAFT });
    if (discarded && !flowOpen) {
      const words = getDict().keysAssign;
      const profile = profileOf(current);
      toast({
        kind: "info",
        title: profileOf(previous) === profile ? words.draftDiscardedReconnect : words.draftDiscarded({ n: profile }),
      });
    }
    return;
  }
  const draft = normalizeDraft(keys, state.draft);
  if (draft !== state.draft) store.set({ ...state, draft });
}

// The shell's draft registry: one entry for this tab, re-registered when
// the count changes (the newest registration replaces the older one).
let unregister: (() => void) | null = null;
let registered = 0;

function syncRegistry(): void {
  const count = changeCount(store.get().draft);
  if (count === registered) return;
  registered = count;
  if (count === 0) {
    unregister?.();
    unregister = null;
  } else {
    unregister = registerDraft("keys", discardDraft, { tab: "assign", count });
  }
}

keysStore.subscribe(followKeys);
store.subscribe(syncRegistry);
followKeys();
syncRegistry();

/** Resets the tab's state (tests). */
export function resetAssignStore(): void {
  store.set(INITIAL);
  flowOpen = false;
  unregister?.();
  unregister = null;
  registered = 0;
}
