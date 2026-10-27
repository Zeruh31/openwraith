// A minimal external store for React's `useSyncExternalStore`: the app's
// global state (status, snapshot, settings, toasts, drafts) lives in a few of
// these instead of a state library.

import { useSyncExternalStore } from "react";

/** A value that React components can subscribe to. */
export interface Store<T> {
  /** The current value. */
  get: () => T;
  /** Replaces the value (or derives it from the previous one) and notifies subscribers. */
  set: (next: T | ((previous: T) => T)) => void;
  /** Calls `listener` after every change; returns the unsubscribe function. */
  subscribe: (listener: () => void) => () => void;
}

/** Creates a store holding `initial`. */
export function createStore<T>(initial: T): Store<T> {
  let value = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => value,
    set(next) {
      const updated =
        typeof next === "function" ? (next as (previous: T) => T)(value) : next;
      if (Object.is(updated, value)) {
        return;
      }
      value = updated;
      for (const listener of [...listeners]) {
        listener();
      }
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

/** The store's current value; re-renders on change. */
export function useStore<T>(store: Store<T>): T {
  return useSyncExternalStore(store.subscribe, store.get, store.get);
}
