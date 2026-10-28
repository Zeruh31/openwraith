// The draft of an Actuation tab (dead band, SOCD), kept outside the
// component so that it survives leaving the tab, as the trigger tab's draft
// does. A draft belongs to one profile of one connection (its owner,
// `generation/profile` of the snapshot): when the keyboard is on another
// profile or was connected again, it no longer applies and is discarded for
// good (with a toast unless the tab's write flow is open). The selection is
// kept. While the draft changes something it is registered with the shell
// under its tab (nav dot, tab dot, profile-switch confirmation).

import { getDict, type Dict } from "../../../i18n";
import type { TabId } from "../../../routes";
import { registerDraft } from "../../../state/drafts";
import { snapshotStore } from "../../../state/snapshot";
import { createStore, useStore, type Store } from "../../../state/store";
import { toast } from "../../../state/toasts";

/** The stored state of a tab's draft. */
export interface TabDraftState<D> {
  /** `generation/profile` of the snapshot the draft was made on; `null` adopts the next one. */
  owner: string | null;
  draft: D;
  selection: ReadonlySet<number>;
}

/** How a tab's draft behaves. */
export interface TabDraftOptions<D> {
  tab: TabId<"actuation">;
  /** No change. */
  empty: D;
  /** Whether the draft changes anything. */
  isDirty: (draft: D) => boolean;
  /** The toast when the draft is dropped (`reconnect`: same profile, new connection). */
  discardedText: (dict: Dict, profile: number, reconnect: boolean) => string;
}

/** A tab's draft store. */
export interface TabDraftStore<D> {
  store: Store<TabDraftState<D>>;
  /** The draft as it applies to the keyboard read last (none while another one's). */
  effective: (state: TabDraftState<D>, owner: string | null) => D;
  /** Replaces the draft, made on the keyboard read as `owner`. */
  setDraft: (owner: string | null, draft: D) => void;
  setSelection: (selection: ReadonlySet<number>) => void;
  /** Discards the draft (the selection stays). */
  discard: () => void;
  /** Marks the tab's write flow as open: profile changes during it come from (or are explained by) the flow. */
  setFlowOpen: (open: boolean) => void;
  /** Resets the store (tests). */
  reset: () => void;
}

/** The owner key of a snapshot. */
export function ownerOf(snapshot: { generation: number; profile: number } | null): string | null {
  return snapshot === null ? null : `${snapshot.generation}/${snapshot.profile}`;
}

function profileOf(owner: string): number {
  return Number(owner.split("/")[1]);
}

/** Creates a tab's draft store and connects it to the snapshot and the shell's draft registry. */
export function createTabDraftStore<D>(options: TabDraftOptions<D>): TabDraftStore<D> {
  const initial: TabDraftState<D> = { owner: null, draft: options.empty, selection: new Set() };
  const store = createStore<TabDraftState<D>>(initial);
  let flowOpen = false;
  let unregister: (() => void) | null = null;

  const effective = (state: TabDraftState<D>, owner: string | null): D =>
    state.owner === null || state.owner === owner ? state.draft : options.empty;

  const discard = () => {
    store.set((state) => (options.isDirty(state.draft) ? { ...state, draft: options.empty } : state));
  };

  const followSnapshot = () => {
    const current = ownerOf(snapshotStore.get().snapshot);
    const state = store.get();
    if (current === null || state.owner === current) return;
    if (state.owner === null) {
      store.set({ ...state, owner: current });
      return;
    }
    const previous = state.owner;
    const discarded = options.isDirty(state.draft);
    store.set({ ...state, owner: current, draft: options.empty });
    if (discarded && !flowOpen) {
      const profile = profileOf(current);
      toast({ kind: "info", title: options.discardedText(getDict(), profile, profileOf(previous) === profile) });
    }
  };

  const syncRegistry = () => {
    const current = ownerOf(snapshotStore.get().snapshot);
    const dirty = options.isDirty(effective(store.get(), current));
    if (dirty && unregister === null) {
      unregister = registerDraft("actuation", discard, { tab: options.tab });
    } else if (!dirty && unregister !== null) {
      unregister();
      unregister = null;
    }
  };

  snapshotStore.subscribe(() => {
    followSnapshot();
    syncRegistry();
  });
  store.subscribe(syncRegistry);
  followSnapshot();
  syncRegistry();

  return {
    store,
    effective,
    setDraft: (owner, draft) => {
      store.set((state) => ({ ...state, owner, draft }));
    },
    setSelection: (selection) => {
      store.set((state) => ({ ...state, selection }));
    },
    discard,
    setFlowOpen: (open) => {
      flowOpen = open;
    },
    reset: () => {
      store.set(initial);
      flowOpen = false;
    },
  };
}

/** The stored state of a tab's draft; re-renders on change. */
export function useTabDraft<D>(drafts: TabDraftStore<D>): TabDraftState<D> {
  return useStore(drafts.store);
}
