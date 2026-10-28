// Unwritten changes per page and tab. Pages register their drafts so that
// the nav shows a dot (any tab of the page), a page's tab bar shows a dot per
// tab, and the profile-switch confirmation lists them ("Keys > Macros:
// 1 unwritten change").

import { useEffect, useRef } from "react";

import type { PageId } from "../routes";
import { createStore, useStore } from "./store";

/** One registered draft, as the shell shows it. */
export interface DraftInfo {
  page: PageId;
  /** The tab it belongs to (`null`: the page as a whole). */
  tab: string | null;
  /** How many unwritten changes it holds, when the page counts them. */
  count: number | null;
}

interface DraftEntry extends DraftInfo {
  discard: () => void;
}

type DraftMap = Readonly<Record<string, DraftEntry>>;

const draftStore = createStore<DraftMap>({});

function keyOf(page: PageId, tab: string | null): string {
  return tab === null ? page : `${page}/${tab}`;
}

function countOf(active: boolean | number): number | null {
  return typeof active === "number" ? active : null;
}

function isActive(active: boolean | number): boolean {
  return typeof active === "number" ? active > 0 : active;
}

function add(entry: DraftEntry): () => void {
  const key = keyOf(entry.page, entry.tab);
  draftStore.set((drafts) => ({ ...drafts, [key]: entry }));
  return () => {
    draftStore.set((drafts) => {
      if (drafts[key] !== entry) {
        return drafts;
      }
      return Object.fromEntries(Object.entries(drafts).filter(([k]) => k !== key));
    });
  };
}

/**
 * Registers the page's draft while `active` is true (or a positive number:
 * the count of unwritten changes, shown in the profile-switch
 * confirmation). `discard` is called when the shell discards drafts (it
 * should clear the draft). `tab` names the page's tab the draft belongs to;
 * every tab registers its own draft and the nav dot shows any of them.
 */
export function useDraft(page: PageId, active: boolean | number, discard: () => void, tab?: string): void {
  const discardRef = useRef(discard);
  useEffect(() => {
    discardRef.current = discard;
  });
  const on = isActive(active);
  const count = countOf(active);
  const tabKey = tab ?? null;
  useEffect(() => {
    if (!on) {
      return undefined;
    }
    return add({
      page,
      tab: tabKey,
      count,
      discard: () => {
        discardRef.current();
      },
    });
  }, [page, tabKey, on, count]);
}

/** Options of {@link registerDraft}. */
export interface RegisterDraftOptions {
  /** The page's tab the draft belongs to. */
  tab?: string | undefined;
  /** How many unwritten changes it holds. */
  count?: number | undefined;
}

/**
 * Registers a draft that lives outside a component (it survives leaving the
 * page); returns the function that removes the registration again.
 * Registering the same page and tab again replaces the entry (e.g. with a new
 * count); only the newest registration's remover removes it.
 */
export function registerDraft(page: PageId, discard: () => void, options: RegisterDraftOptions = {}): () => void {
  return add({ page, tab: options.tab ?? null, count: options.count ?? null, discard });
}

function pagesOf(drafts: DraftMap): PageId[] {
  const pages: PageId[] = [];
  for (const entry of Object.values(drafts)) {
    if (!pages.includes(entry.page)) pages.push(entry.page);
  }
  return pages;
}

function infoOf(drafts: DraftMap): DraftInfo[] {
  return Object.values(drafts).map(({ page, tab, count }) => ({ page, tab, count }));
}

/** The pages that have unwritten changes (on any tab). */
export function useDraftPages(): PageId[] {
  return pagesOf(useStore(draftStore));
}

/** The pages that have unwritten changes (outside React). */
export function draftPages(): PageId[] {
  return pagesOf(draftStore.get());
}

/** Every registered draft, in registration order. */
export function useDraftEntries(): DraftInfo[] {
  return infoOf(useStore(draftStore));
}

/** Every registered draft (outside React). */
export function draftEntries(): DraftInfo[] {
  return infoOf(draftStore.get());
}

/** The tabs of `page` that have unwritten changes (`null` stands for the page as a whole). */
export function useDraftTabs(page: PageId): (string | null)[] {
  const drafts = useStore(draftStore);
  return Object.values(drafts)
    .filter((entry) => entry.page === page)
    .map((entry) => entry.tab);
}

/** Whether any page has unwritten changes (outside React). */
export function hasDrafts(): boolean {
  return Object.keys(draftStore.get()).length > 0;
}

/** Calls `listener` after every change of the registered drafts; returns the unsubscribe function. */
export function subscribeDrafts(listener: () => void): () => void {
  return draftStore.subscribe(listener);
}

/** Discards the drafts of `pages` (every tab; all pages when omitted). */
export function discardDrafts(pages?: readonly PageId[]): void {
  const drafts = draftStore.get();
  for (const entry of Object.values(drafts)) {
    if (pages === undefined || pages.includes(entry.page)) entry.discard();
  }
}

/** Discards the draft of one page's tab (`null`: the page-wide draft). */
export function discardDraftOf(page: PageId, tab: string | null): void {
  draftStore.get()[keyOf(page, tab)]?.discard();
}
