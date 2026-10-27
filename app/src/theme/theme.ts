// The theme: `system` follows the operating system (prefers-color-scheme);
// `light` and `dark` override it through `data-theme` on <html>.

import { useSyncExternalStore } from "react";

import type { ThemeSetting } from "../ipc/bindings";

/** Applies a theme setting to the document. */
export function applyTheme(setting: ThemeSetting, root: HTMLElement = document.documentElement): void {
  if (setting === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", setting);
  }
}

const DARK_QUERY = "(prefers-color-scheme: dark)";

function systemDark(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia(DARK_QUERY).matches;
}

/** The theme in effect: the explicit choice, else the system's. */
export function resolvedTheme(root: HTMLElement = document.documentElement): "light" | "dark" {
  const chosen = root.getAttribute("data-theme");
  if (chosen === "light" || chosen === "dark") {
    return chosen;
  }
  return systemDark() ? "dark" : "light";
}

function subscribe(listener: () => void): () => void {
  const observer = new MutationObserver(listener);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  const media = typeof window.matchMedia === "function" ? window.matchMedia(DARK_QUERY) : null;
  media?.addEventListener("change", listener);
  return () => {
    observer.disconnect();
    media?.removeEventListener("change", listener);
  };
}

function snapshot(): "light" | "dark" {
  return resolvedTheme();
}

/** The theme in effect; re-renders when it changes. */
export function useResolvedTheme(): "light" | "dark" {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}
