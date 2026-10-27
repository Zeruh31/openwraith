// Start-up: the app's facts, the settings, then the event subscription.

import type { AppError } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { toAppError } from "../ipc/errors";
import { appInfoStore } from "./appInfo";
import { startSessionWatch } from "./session";
import { applySettings, settingsStore } from "./settings";
import { startSnapshotSync } from "./snapshot";
import { connectEvents } from "./status";
import { createStore, useStore } from "./store";

/** Where start-up is. */
export type BootState = { phase: "loading" } | { phase: "ready" } | { phase: "failed"; error: AppError };

/** The start-up state. */
export const bootStore = createStore<BootState>({ phase: "loading" });

let running: Promise<void> | null = null;

/** Starts the app's state once (later calls return the same promise). */
export function boot(): Promise<void> {
  running ??= (async () => {
    try {
      const [info, settings] = await Promise.all([api.appInfo(), api.getSettings()]);
      appInfoStore.set(info);
      settingsStore.set(settings);
      applySettings(settings);
      startSnapshotSync();
      startSessionWatch();
      await connectEvents();
      bootStore.set({ phase: "ready" });
    } catch (error) {
      bootStore.set({ phase: "failed", error: toAppError(error) });
    }
  })();
  return running;
}

/** Starts again after a failure. */
export function retryBoot(): Promise<void> {
  running = null;
  bootStore.set({ phase: "loading" });
  return boot();
}

/** The start-up state; re-renders when it changes. */
export function useBoot(): BootState {
  return useStore(bootStore);
}
