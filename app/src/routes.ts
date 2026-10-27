// Hash routes: `#/device` (default), `#/actuation`, `#/lighting`, `#/backup`,
// `#/settings`, with optional query parameters (`#/backup?restore=<FileToken>`
// opens a backup for restoring: Undo and the interrupted-operation banner).

import { useSyncExternalStore } from "react";

/** The pages of the app. */
export const PAGE_IDS = ["device", "actuation", "lighting", "backup", "settings"] as const;

/** A page of the app. */
export type PageId = (typeof PAGE_IDS)[number];

/** A parsed route. */
export interface Route {
  page: PageId;
  /** Query parameters after `?` in the hash. */
  params: Readonly<Record<string, string>>;
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
