// The Lighting page's draft, kept outside the component so that it survives
// switching pages. A draft belongs to one profile of one connection (its
// "basis"); when the keyboard is on another profile or was connected again,
// the draft no longer applies and is discarded.

import { registerDraft } from "../../state/drafts";
import { createStore, useStore } from "../../state/store";
import { EMPTY_DRAFT, hasDraftFields, type LightDraft } from "./model";

/** The draft and the profile it was made for. */
export interface DraftState {
  /** `"<generation>:<profile>"` of the snapshot the draft was made on; `null` before the first one. */
  basis: string | null;
  draft: LightDraft;
}

const INITIAL: DraftState = { basis: null, draft: EMPTY_DRAFT };

const draftStore = createStore<DraftState>(INITIAL);

// The shell's draft registry (nav dot, the profile-switch confirmation)
// knows about the draft as long as it exists, on this page or not.
let unregister: (() => void) | null = null;
draftStore.subscribe(() => {
  const dirty = hasDraftFields(draftStore.get().draft);
  if (dirty && unregister === null) {
    unregister = registerDraft("lighting", clearDraft);
  } else if (!dirty && unregister !== null) {
    unregister();
    unregister = null;
  }
});

/** The basis key of a snapshot. */
export function basisOf(snapshot: { generation: number; profile: number } | null): string | null {
  return snapshot === null ? null : `${snapshot.generation}:${snapshot.profile}`;
}

/** The stored draft state; re-renders on change. */
export function useDraftState(): DraftState {
  return useStore(draftStore);
}

/** The stored draft state (outside React). */
export function getDraftState(): DraftState {
  return draftStore.get();
}

/** Replaces the draft (made on `basis`); a draft made on another basis is not built upon. */
export function setDraft(basis: string | null, update: (draft: LightDraft) => LightDraft): void {
  draftStore.set((state) => {
    const same = basis === null || state.basis === null || state.basis === basis;
    const next = update(same ? state.draft : EMPTY_DRAFT);
    return { basis: basis ?? state.basis, draft: hasDraftFields(next) ? next : EMPTY_DRAFT };
  });
}

/** Clears the draft. */
export function clearDraft(): void {
  draftStore.set((state) => (state.draft === EMPTY_DRAFT ? state : { ...state, draft: EMPTY_DRAFT }));
}

/**
 * Moves the draft to a new basis. Returns whether a draft made on another
 * basis was discarded.
 */
export function rebase(basis: string): boolean {
  const state = draftStore.get();
  if (state.basis === basis) return false;
  if (state.basis === null) {
    draftStore.set({ basis, draft: state.draft });
    return false;
  }
  const dropped = hasDraftFields(state.draft);
  draftStore.set({ basis, draft: EMPTY_DRAFT });
  return dropped;
}

/** Seeds the draft once (browser-preview scenarios); ignored when a draft exists. */
export function seedDraft(draft: LightDraft): void {
  const state = draftStore.get();
  if (!hasDraftFields(state.draft) && hasDraftFields(draft)) {
    draftStore.set({ ...state, draft });
  }
}

/** Resets the store (tests). */
export function resetDraftStore(): void {
  draftStore.set(INITIAL);
}
