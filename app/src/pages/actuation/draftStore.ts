// The Actuation page's draft and key selection, kept outside the component
// so that they survive switching pages (as the Lighting draft does). A draft
// belongs to one profile of one connection (its owner, `generation/profile`):
// when the keyboard is on another profile or was connected again, the draft
// no longer applies and is discarded for good, whether the page is shown or
// not. The selection is kept (the keys are the same). A draft filled from a
// trigger preset remembers the preset, so that its review plans the preset
// itself (`prepare_preset_edit`) until the draft is edited.

import { getDict } from "../../i18n";
import type { PresetRef } from "../../ipc/bindings";
import { registerDraft } from "../../state/drafts";
import { snapshotStore } from "../../state/snapshot";
import { createStore, useStore } from "../../state/store";
import { toast } from "../../state/toasts";
import { clearDraft, EMPTY_DRAFT, isDirty, type TriggerDraft } from "./model";

/** The draft, the connection and profile it was made for, and the selected keys. */
export interface ActuationState {
  /** `generation/profile` of the snapshot the draft was made on; `null` adopts the next one. */
  owner: string | null;
  draft: TriggerDraft;
  selection: ReadonlySet<number>;
  /** The preset the draft was filled from (cleared when the draft is edited). */
  preset: PresetRef | null;
}

const INITIAL: ActuationState = { owner: null, draft: EMPTY_DRAFT, selection: new Set(), preset: null };

const store = createStore<ActuationState>(INITIAL);

/** The owner key of a snapshot. */
export function ownerOf(snapshot: { generation: number; profile: number } | null): string | null {
  return snapshot === null ? null : `${snapshot.generation}/${snapshot.profile}`;
}

/** The draft as it applies to the keyboard read last (none while another one's). */
export function effectiveDraft(state: ActuationState, current: string | null): TriggerDraft {
  return state.owner === null || state.owner === current ? state.draft : clearDraft(state.draft);
}

/** The stored state; re-renders on change. */
export function useActuationState(): ActuationState {
  return useStore(store);
}

/** Replaces the draft, made on the keyboard read as `owner` (it is no longer a preset's). */
export function setDraft(owner: string | null, draft: TriggerDraft): void {
  store.set((state) => ({ ...state, owner, draft, preset: null }));
}

/** Fills the draft from a preset for `selection`, made on the keyboard read as `owner`. */
export function applyPreset(
  owner: string | null,
  draft: TriggerDraft,
  selection: ReadonlySet<number>,
  preset: PresetRef,
): void {
  store.set({ owner, draft, selection, preset });
}

/** Replaces the selection. */
export function setSelection(selection: ReadonlySet<number>): void {
  store.set((state) => ({ ...state, selection }));
}

/** Discards the draft (the slider values stay for the next time). */
export function discardDraft(): void {
  store.set((state) =>
    isDirty(state.draft) || state.preset !== null ? { ...state, draft: clearDraft(state.draft), preset: null } : state,
  );
}

/** Seeds the state once (browser-preview scenarios); ignored when a draft or a selection exists. */
export function seedState(draft: TriggerDraft, selection: ReadonlySet<number>): void {
  const state = store.get();
  if (isDirty(state.draft) || state.selection.size > 0) return;
  if (!isDirty(draft) && selection.size === 0) return;
  store.set({ owner: state.owner, draft, selection, preset: null });
}

/** Whether the page's write flow is open (see {@link setFlowOpen}). */
let flowOpen = false;

/**
 * Marks the page's write flow as open or closed: while it is open, a
 * profile or connection change comes from (or is explained by) that flow,
 * so there is no "discarded" toast then.
 */
export function setFlowOpen(open: boolean): void {
  flowOpen = open;
}

function profileOf(owner: string): number {
  return Number(owner.split("/")[1]);
}

/**
 * Follows the keyboard: a draft made on another profile or connection is
 * discarded for good (going back to the first profile later does not bring
 * it back), with a toast unless the page's write flow is open.
 */
function followSnapshot(): void {
  const current = ownerOf(snapshotStore.get().snapshot);
  const state = store.get();
  if (current === null || state.owner === current) return;
  if (state.owner === null) {
    store.set({ ...state, owner: current });
    return;
  }
  const previous = state.owner;
  const discarded = isDirty(state.draft);
  store.set({ ...state, owner: current, draft: clearDraft(state.draft), preset: null });
  if (discarded && !flowOpen) {
    const words = getDict().actuation;
    const profile = profileOf(current);
    toast({
      kind: "info",
      title: profileOf(previous) === profile ? words.draftDiscardedReconnect : words.draftDiscarded({ n: profile }),
    });
  }
}

// The shell's draft registry (nav dot, the profile-switch confirmation)
// knows about the draft as long as it applies to the keyboard read last, on
// this page or not.
let unregister: (() => void) | null = null;
function syncRegistry(): void {
  const current = ownerOf(snapshotStore.get().snapshot);
  const dirty = isDirty(effectiveDraft(store.get(), current));
  if (dirty && unregister === null) {
    unregister = registerDraft("actuation", discardDraft, { tab: "trigger" });
  } else if (!dirty && unregister !== null) {
    unregister();
    unregister = null;
  }
}

snapshotStore.subscribe(() => {
  followSnapshot();
  syncRegistry();
});
store.subscribe(syncRegistry);
followSnapshot();
syncRegistry();

/** Resets the store (tests). */
export function resetActuationStore(): void {
  store.set(INITIAL);
  flowOpen = false;
}
