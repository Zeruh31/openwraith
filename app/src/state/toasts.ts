// Toasts: short, non-blocking messages (aria-live polite).

import { createStore, useStore } from "./store";

/** How a toast looks. */
export type ToastKind = "info" | "success" | "warning" | "danger";

/** A toast on screen. */
export interface Toast {
  id: number;
  kind: ToastKind;
  title: string;
  body?: string | undefined;
  /** Technical text shown under "Technical details". */
  details?: string | undefined;
}

/** What `toast()` takes. */
export interface ToastInput {
  kind?: ToastKind | undefined;
  title: string;
  body?: string | undefined;
  details?: string | undefined;
  /** Milliseconds until it closes by itself (default 6000; 0 keeps it). */
  timeoutMs?: number | undefined;
}

const MAX_TOASTS = 4;
const toastStore = createStore<readonly Toast[]>([]);
let nextId = 1;

/** Shows a toast; returns its id. */
export function toast(input: ToastInput): number {
  const id = nextId;
  nextId += 1;
  const item: Toast = {
    id,
    kind: input.kind ?? "info",
    title: input.title,
    body: input.body,
    details: input.details,
  };
  toastStore.set((list) => [...list, item].slice(-MAX_TOASTS));
  const timeout = input.timeoutMs ?? (input.details === undefined ? 6000 : 0);
  if (timeout > 0) {
    window.setTimeout(() => {
      dismissToast(id);
    }, timeout);
  }
  return id;
}

/** Closes a toast. */
export function dismissToast(id: number): void {
  toastStore.set((list) => list.filter((t) => t.id !== id));
}

/** The toasts on screen. */
export function useToasts(): readonly Toast[] {
  return useStore(toastStore);
}

/** The toasts shown now (outside React; tests). */
export function getToasts(): readonly Toast[] {
  return toastStore.get();
}

/** Removes every toast (tests). */
export function clearToasts(): void {
  toastStore.set([]);
}
