// Hash routes: `#/device` (default), `#/keys`, `#/actuation`, `#/lighting`,
// `#/backup`, `#/settings`, with optional query parameters (`#/backup?restore=<FileToken>`
// opens a backup for restoring: Undo and the interrupted-operation banner;
// `#/keys?tab=macros` opens a page's tab; `#/backup?recover=1` opens the
// Recovery card; `#/settings?tab=games` opens Settings > Games).

import { useSyncExternalStore } from "react";

/** The pages of the app. */
export const PAGE_IDS = ["device", "keys", "actuation", "lighting", "backup", "settings"] as const;

/** A page of the app. */
export type PageId = (typeof PAGE_IDS)[number];

/** A parsed route. */
export interface Route {
  page: PageId;
  /** Query parameters after `?` in the hash. */
  params: Readonly<Record<string, string>>;
}

/**
 * The tabs of the pages that have tabs (`?tab=<id>`), in display order; the
 * first one is shown when the route names none (or an unknown one).
 */
export const PAGE_TABS = {
  keys: ["assign", "macros", "advanced"],
  actuation: ["trigger", "deadband", "socd", "depth"],
  lighting: ["effect", "colors"],
  settings: ["general", "background", "games", "profiles", "about"],
} as const satisfies Partial<Record<PageId, readonly string[]>>;

/** A page with tabs. */
export type TabbedPageId = keyof typeof PAGE_TABS;

/** A tab of a page with tabs. */
export type TabId<P extends TabbedPageId> = (typeof PAGE_TABS)[P][number];

/** Whether `page` has tabs. */
export function hasTabs(page: PageId): page is TabbedPageId {
  return page in PAGE_TABS;
}

/** The route's tab of `page`: `?tab=` when it names one of the page's tabs, else the first tab. */
export function tabOf<P extends TabbedPageId>(route: Route, page: P): TabId<P> {
  const tabs: readonly TabId<P>[] = PAGE_TABS[page];
  const wanted = route.params["tab"];
  const found = tabs.find((tab) => tab === wanted);
  return found ?? (tabs[0] as TabId<P>);
}

/** The route of a page's tab (`#/keys?tab=macros`). */
export function tabTarget<P extends TabbedPageId>(page: P, tab: TabId<P>): RouteTarget {
  return { page, params: { tab } };
}

/** A page, or a page with parameters. */
export type RouteTarget = PageId | { page: PageId; params?: Record<string, string> };

const DEFAULT_PAGE: PageId = "device";
const STORAGE_KEY = "openwraith.lastRoute";

function isPageId(value: string): value is PageId {
  return (PAGE_IDS as readonly string[]).includes(value);
}

/** Parses a location hash (`#/backup?restore=f-1`). Unknown pages give the device page. */
export function parseRoute(hash: string): Route {
  const raw = hash.replace(/^#/, "").replace(/^\//, "");
  const [path = "", query = ""] = raw.split("?", 2);
  const page = isPageId(path) ? path : DEFAULT_PAGE;
  const params: Record<string, string> = {};
  for (const [key, value] of new URLSearchParams(query)) {
    params[key] = value;
  }
  return { page, params };
}

/** The hash for a route (`#/backup?restore=f-1`). */
export function href(target: RouteTarget): string {
  const page = typeof target === "string" ? target : target.page;
  const params = typeof target === "string" ? undefined : target.params;
  const query = params && Object.keys(params).length > 0 ? `?${new URLSearchParams(params).toString()}` : "";
  return `#/${page}${query}`;
}

/** Goes to a route. */
export function navigate(target: RouteTarget): void {
  const next = href(target);
  if (window.location.hash !== next) {
    window.location.hash = next;
  }
}

function subscribe(listener: () => void): () => void {
  window.addEventListener("hashchange", listener);
  return () => {
    window.removeEventListener("hashchange", listener);
  };
}

function currentHash(): string {
  return window.location.hash;
}

/** The current route; re-renders when the hash changes. */
export function useRoute(): Route {
  const hash = useSyncExternalStore(subscribe, currentHash, currentHash);
  return parseRoute(hash);
}

/**
 * The current tab of `page` and a function that goes to another tab (the
 * page's other query parameters are dropped).
 */
export function useTab<P extends TabbedPageId>(page: P): [TabId<P>, (tab: TabId<P>) => void] {
  const route = useRoute();
  const tab = route.page === page ? tabOf(route, page) : (PAGE_TABS[page][0] as TabId<P>);
  return [
    tab,
    (next: TabId<P>) => {
      navigate(tabTarget(page, next));
    },
  ];
}

/** Remembers the route (browser storage, best effort). */
export function rememberRoute(route: Route): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, route.page);
  } catch {
    // Storage may be unavailable; the route is only a convenience.
  }
}

/** Restores the last page when the app starts without a hash. */
export function restoreLastRoute(): void {
  if (window.location.hash !== "") {
    return;
  }
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored !== null && isPageId(stored)) {
      window.location.hash = href(stored);
    }
  } catch {
    // Storage may be unavailable.
  }
}
