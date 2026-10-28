// Keys > Macros: the draft, the macro open in the editor and the editor's
// settings, kept outside the component so that they survive switching tabs
// and pages. A draft belongs to one profile of one connection (its owner,
// `generation/profile` of the keys state): when the keyboard is on another
// profile or was connected again, the draft no longer applies and is
// discarded (with a toast unless the tab's write flow is open). A newer read
// of the same profile keeps the draft, without the edits that no longer
// apply. The shell's draft registry (the tab's dot, the profile-switch
// confirmation "Keys > Macros: 2 unwritten changes") follows the draft.

import { getDict } from "../../../i18n";
import type { KeysState } from "../../../ipc/bindings";
import { registerDraft } from "../../../state/drafts";
import { createStore, useStore } from "../../../state/store";
import { toast } from "../../../state/toasts";
import { keysStore } from "../shared/keysState";
import { draftCount, EMPTY_DRAFT, reconcile, type MacrosDraft, type Target, type TryKey } from "./model";
import type { DelayMode } from "./recording";

/** How the editor shows the steps. */
export type StepsView = "simple" | "detailed";

/** The tab's state. */
export interface MacrosState {
  /** `generation/profile` of the keys state the draft was made on; `null` adopts the next one. */
  owner: string | null;
  draft: MacrosDraft;
  /** The macro open in the editor. */
  target: Target | null;
  /** The next id of a new macro. */
  nextId: number;
  view: StepsView;
  delayMode: DelayMode;
  /** The recorder's fixed wait (ms). */
  fixedMs: number;
  /** The keys to try the macros of the last write with ("Try it"); `null` when none. */
  tryIt: TryKey[] | null;
}

const INITIAL: MacrosState = {
  owner: null,
  draft: EMPTY_DRAFT,
  target: null,
  nextId: 1,
  view: "simple",
  delayMode: "typed",
  fixedMs: 10,
  tryIt: null,
};

const store = createStore<MacrosState>(INITIAL);

/** The owner key of a keys state. */
export function ownerOf(state: Pick<KeysState, "generation" | "profile"> | null): string | null {
  return state === null ? null : `${state.generation}/${state.profile}`;
}

/** The tab's state; re-renders on change. */
export function useMacrosState(): MacrosState {
  return useStore(store);
}

/** The tab's state now. */
export function macrosState(): MacrosState {
  return store.get();
}

/** Changes the tab's state. */
export function updateMacros(update: (state: MacrosState) => MacrosState): void {
  store.set(update);
}

/** Replaces the draft (made on the keys state `owner`). */
export function setDraft(owner: string | null, draft: MacrosDraft): void {
  store.set((state) => ({ ...state, owner: owner ?? state.owner, draft }));
}

/** Opens a macro in the editor (or closes it). */
export function setTarget(target: Target | null): void {
  store.set((state) => ({ ...state, target }));
}

/** Discards the draft (the editor's settings stay). */
export function discardDraft(): void {
  store.set((state) =>
    state.draft === EMPTY_DRAFT && state.target?.kind !== "new" ? state : { ...state, draft: EMPTY_DRAFT, target: state.target?.kind === "new" ? null : state.target },
  );
}

/** The draft was written: it is spent; the keys to try the result with are kept. */
export function spendDraft(tryIt: TryKey[] | null): void {
  store.set((state) => ({
    ...state,
    draft: EMPTY_DRAFT,
    target: state.target?.kind === "new" ? null : state.target,
    tryIt: tryIt === null || tryIt.length === 0 ? null : tryIt,
  }));
}

/** Closes "Try it". */
export function closeTryIt(): void {
  store.set((state) => (state.tryIt === null ? state : { ...state, tryIt: null }));
}

let flowOpen = false;

/** Marks the tab's write flow as open: a profile or connection change it makes discards nothing with a toast. */
export function setFlowOpen(open: boolean): void {
  flowOpen = open;
}

function profileOf(owner: string): number {
  return Number(owner.split("/")[1]);
}

/** Follows the keys state: another profile or connection discards the draft; a newer read reconciles it. */
function followKeys(): void {
  const keys = keysStore.get().state;
  const current = ownerOf(keys);
  const state = store.get();
  if (keys === null || current === null) return;
  if (state.owner === null) {
    store.set({ ...state, owner: current, draft: reconcile(state.draft, keys) });
    return;
  }
  if (state.owner === current) {
    const draft = reconcile(state.draft, keys);
    if (draft !== state.draft) store.set({ ...state, draft });
    return;
  }
  const discarded = draftCount(state.draft) > 0;
  const previous = state.owner;
  store.set({
    ...state,
    owner: current,
    draft: EMPTY_DRAFT,
    target: null,
    tryIt: null,
  });
  if (discarded && !flowOpen) {
    const words = getDict().keysMacros;
    const profile = profileOf(current);
    toast({
      kind: "info",
      title: profileOf(previous) === profile ? words.draftDiscardedReconnect : words.draftDiscarded({ n: profile }),
    });
  }
}

// The registry knows about the draft while it has changes, on this tab or not.
let unregister: (() => void) | null = null;
let registeredCount = 0;
function syncRegistry(): void {
  const count = draftCount(store.get().draft);
  if (count === registeredCount && (count === 0) === (unregister === null)) return;
  unregister?.();
  unregister = null;
  registeredCount = count;
  if (count > 0) unregister = registerDraft("keys", discardDraft, { tab: "macros", count });
}

keysStore.subscribe(followKeys);
store.subscribe(syncRegistry);
followKeys();
syncRegistry();

/** Resets the tab's state (tests). */
export function resetMacrosStore(): void {
  store.set(INITIAL);
  flowOpen = false;
}
