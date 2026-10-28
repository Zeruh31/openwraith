// One-time tips (`hints_shown` in settings.json): which ones the user
// dismissed. Loaded once from the backend; until it is known, or when it
// can't be read, no tip is shown (a tip is never worth an error).

import { useEffect } from "react";

import { api } from "../ipc/commands";
import { createStore, useStore } from "./store";

interface HintsState {
  /** The list was read from the backend. */
  loaded: boolean;
  /** Ids of the tips already dismissed. */
  shown: readonly string[];
}

const INITIAL: HintsState = { loaded: false, shown: [] };

/** The dismissed tips. */
export const hintsStore = createStore<HintsState>(INITIAL);

let loading = false;

function ignore(): void {
  // A tip that can't be remembered is simply shown again next time.
}

/** Reads the dismissed tips once. */
export function loadHints(): void {
  if (loading || hintsStore.get().loaded) return;
  loading = true;
  api
    .getHintsShown()
    .then((shown) => {
      hintsStore.set({ loaded: true, shown });
    })
    .catch(ignore)
    .finally(() => {
      loading = false;
    });
}

/** The dismissed tips; loads them on first use. */
export function useHints(): HintsState {
  useEffect(loadHints, []);
  return useStore(hintsStore);
}

/** Whether tip `id` is shown, and how to dismiss it (remembered in settings.json). */
export function useHint(id: string): { show: boolean; dismiss: () => void } {
  const hints = useHints();
  return {
    show: hints.loaded && !hints.shown.includes(id),
    dismiss: () => {
      hintsStore.set((current) => ({ loaded: true, shown: [...current.shown, id] }));
      api
        .markHintShown(id)
        .then((shown) => {
          hintsStore.set({ loaded: true, shown });
        })
        .catch(ignore);
    },
  };
}

/** "Show tips again": forgets every dismissed tip. Rejects with an AppError. */
export async function resetHints(): Promise<void> {
  const shown = await api.resetHintsShown();
  hintsStore.set({ loaded: true, shown });
}

/** Forgets the loaded state (tests). */
export function resetHintsStore(): void {
  loading = false;
  hintsStore.set(INITIAL);
}
