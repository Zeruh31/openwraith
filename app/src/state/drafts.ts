// Unwritten changes per page. Pages register their draft so that the nav
// shows a dot and the profile-switch confirmation can list them.

import { useEffect, useRef } from "react";

import type { PageId } from "../routes";
import { createStore, useStore } from "./store";

interface DraftEntry {
  discard: () => void;
}

const draftStore = createStore<Readonly<Partial<Record<PageId, DraftEntry>>>>({});

/**
 * Registers the page's draft while `active` is true. `discard` is called
 * when the shell discards drafts (it should clear the page's draft).
 */
export function useDraft(page: PageId, active: boolean, discard: () => void): void {
  const discardRef = useRef(discard);
  useEffect(() => {
    discardRef.current = discard;
  });
  useEffect(() => {
    if (!active) {
      return undefined;
    }
    const entry: DraftEntry = {
      discard: () => {
        discardRef.current();
      },
    };
    draftStore.set((drafts) => ({ ...drafts, [page]: entry }));
    return () => {
      draftStore.set((drafts) => {
        if (drafts[page] !== entry) {
          return drafts;
        }
        return Object.fromEntries(Object.entries(drafts).filter(([key]) => key !== page));
      });
    };
  }, [page, active]);
}

/**
 * Registers a draft that lives outside a component (it survives leaving the
 * page); returns the function that removes the registration again.
 */
export function registerDraft(page: PageId, discard: () => void): () => void {
  const entry: DraftEntry = { discard };
  draftStore.set((drafts) => ({ ...drafts, [page]: entry }));
  return () => {
    draftStore.set((drafts) => {
      if (drafts[page] !== entry) {
        return drafts;
      }
      return Object.fromEntries(Object.entries(drafts).filter(([key]) => key !== page));
    });
  };
}

/** The pages that have unwritten changes. */
export function useDraftPages(): PageId[] {
  const drafts = useStore(draftStore);
  return (Object.keys(drafts) as PageId[]).filter((page) => drafts[page] !== undefined);
}

/** The pages that have unwritten changes (outside React). */
export function draftPages(): PageId[] {
  const drafts = draftStore.get();
  return (Object.keys(drafts) as PageId[]).filter((page) => drafts[page] !== undefined);
}

/** Discards the drafts of `pages` (all when omitted). */
export function discardDrafts(pages?: readonly PageId[]): void {
  const drafts = draftStore.get();
  for (const page of pages ?? (Object.keys(drafts) as PageId[])) {
    drafts[page]?.discard();
  }
}
