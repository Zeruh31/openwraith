// The Advanced keys tab's draft, kept outside the component so that it
// survives switching tabs and pages. A draft belongs to one profile of one
// connection (its owner, `generation/profile` of the keys state): when the
// keys state comes from another profile or connection, the draft no longer
// applies and is discarded for good, with a toast unless the tab's write
// flow is open. The shell's draft registry (tab dot, nav dot, the
// profile-switch confirmation) knows about the draft with its count.

import { getDict } from "../../../i18n";
import type { BuiltinAckIntent, KeysState } from "../../../ipc/bindings";
import { registerDraft } from "../../../state/drafts";
import { createStore, useStore } from "../../../state/store";
import { toast } from "../../../state/toasts";
import { keysStore } from "../shared";
import { addItem, EMPTY_DRAFT, removeItem, withAck, type AdvancedDraft, type NewDraftItem } from "./model";

/** The draft and the keys state it was made on. */
export interface AdvancedStoreState {
  /** `generation/profile` of the keys state the draft was made on; `null` adopts the next one. */
  owner: string | null;
  draft: AdvancedDraft;
}

const INITIAL: AdvancedStoreState = { owner: null, draft: EMPTY_DRAFT };

const store = createStore<AdvancedStoreState>(INITIAL);

let nextId = 1;

/** The owner key of a keys state. */
export function ownerOf(state: Pick<KeysState, "generation" | "profile"> | null): string | null {
  return state === null ? null : `${state.generation}/${state.profile}`;
}

/** The draft as it applies to the keys state `owner` (none while it belongs to another one). */
export function effectiveDraft(stored: AdvancedStoreState, owner: string | null): AdvancedDraft {
  return stored.owner === null || stored.owner === owner ? stored.draft : EMPTY_DRAFT;
}

/** The stored draft; re-renders on change. */
export function useAdvancedStore(): AdvancedStoreState {
  return useStore(store);
}

/** The stored draft (outside React, tests). */
export function advancedStore(): AdvancedStoreState {
  return store.get();
}

/** Adds a change made on the keys state `owner` (replacing `replaces`, an item being re-edited), with its confirmation. */
export function draftChange(
  owner: string | null,
  item: NewDraftItem,
  options: { replaces?: string | null; ack?: BuiltinAckIntent | null } = {},
): void {
  const id = `a${nextId}`;
  nextId += 1;
  store.set((state) => {
    const base = effectiveDraft(state, owner);
    return { owner, draft: withAck(addItem(base, { ...item, id }, options.replaces ?? null), options.ack ?? null) };
  });
}

/** Removes one change. */
export function dropChange(owner: string | null, id: string): void {
  store.set((state) => ({ owner, draft: removeItem(effectiveDraft(state, owner), id) }));
}

/** Keeps critical built-ins the user confirmed in the write flow (so the next review doesn't ask again). */
export function keepAcks(owner: string | null, acks: readonly BuiltinAckIntent[]): void {
  if (acks.length === 0) return;
  store.set((state) => {
    let draft = effectiveDraft(state, owner);
    // A spent draft keeps nothing.
    if (draft.items.length === 0) return state;
    for (const ack of acks) draft = withAck(draft, ack);
    return { owner, draft };
  });
}

/** Discards the draft. */
export function discardAdvancedDraft(): void {
  store.set((state) => (state.draft.items.length === 0 && state.draft.acks.length === 0 ? state : { ...state, draft: EMPTY_DRAFT }));
}

/** Whether the tab's write flow is open (no "discarded" toast for changes it makes). */
let flowOpen = false;

/** Marks the tab's write flow as open or closed. */
export function setAdvancedFlowOpen(open: boolean): void {
  flowOpen = open;
}

function profileOf(owner: string): number {
  return Number(owner.split("/")[1]);
}

/**
 * Follows the keys state: a draft made on another profile or connection is
 * discarded for good (going back later does not bring it back), with a
 * toast unless the write flow is open.
 */
function followKeys(): void {
  const current = ownerOf(keysStore.get().state);
  const state = store.get();
  if (current === null || state.owner === current) return;
  if (state.owner === null) {
    store.set({ ...state, owner: current });
    return;
  }
  const previous = state.owner;
  const discarded = state.draft.items.length > 0;
  store.set({ owner: current, draft: EMPTY_DRAFT });
  if (discarded && !flowOpen) {
    const words = getDict().keysAdvanced.draft;
    const profile = profileOf(current);
    toast({
      kind: "info",
      title: profileOf(previous) === profile ? words.discardedReconnect : words.discarded({ n: profile }),
    });
  }
}

// The registry entry (tab "advanced" of the Keys page) with the count of changes.
let unregister: (() => void) | null = null;
let registeredCount = 0;
function syncRegistry(): void {
  const current = ownerOf(keysStore.get().state);
  const count = effectiveDraft(store.get(), current).items.length;
  if (count === registeredCount && (count === 0) === (unregister === null)) return;
  unregister?.();
  unregister = count > 0 ? registerDraft("keys", discardAdvancedDraft, { tab: "advanced", count }) : null;
  registeredCount = count;
}

keysStore.subscribe(() => {
  followKeys();
  syncRegistry();
});
store.subscribe(syncRegistry);
followKeys();
syncRegistry();

/** Resets the store (tests). */
export function resetAdvancedStore(): void {
  store.set(INITIAL);
  flowOpen = false;
}
