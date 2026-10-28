// The background settings (Settings > Background): the notification-area
// icon, start with Windows, notifications and automatic backups. Loaded at
// start (the toaster needs "game-switch notifications") and saved through
// `set_background_settings`, which also tells the shell (tray, Run key).

import type { BackgroundSettingsApplied, BackgroundSettingsIntent, BackgroundSettingsView } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { createStore, useStore } from "./store";

/** The settings as saved (`null` until loaded). */
export const backgroundStore = createStore<BackgroundSettingsView | null>(null);

let loading: Promise<BackgroundSettingsView> | null = null;

/** The intent that saves `view` unchanged. */
export function backgroundIntent(view: BackgroundSettingsView): BackgroundSettingsIntent {
  return {
    tray_enabled: view.tray_enabled,
    start_with_windows: view.tray_enabled && view.start_with_windows,
    notifications: { ...view.notifications },
    auto_backup: { ...view.auto_backup },
  };
}

/** Reads the settings (again); rejects with an `AppError`. */
export function loadBackgroundSettings(): Promise<BackgroundSettingsView> {
  loading ??= api
    .getBackgroundSettings()
    .then((view) => {
      backgroundStore.set(view);
      return view;
    })
    .finally(() => {
      loading = null;
    });
  return loading;
}

/** Reads the settings in the background (failures are ignored: the page that needs them asks again). */
export function ensureBackgroundSettings(): void {
  if (backgroundStore.get() !== null) return;
  loadBackgroundSettings().catch(() => {
    // Settings > Background shows the error when it is opened.
  });
}

/** Saves the settings; the result says what the shell changed. Rejects with an `AppError`. */
export async function saveBackgroundSettings(intent: BackgroundSettingsIntent): Promise<BackgroundSettingsApplied> {
  const applied = await api.setBackgroundSettings(intent);
  backgroundStore.set(applied.settings);
  return applied;
}

/** The settings (`null` until loaded); re-renders when they change. */
export function useBackgroundSettings(): BackgroundSettingsView | null {
  return useStore(backgroundStore);
}

/** Forgets the settings (tests). */
export function resetBackgroundStore(): void {
  backgroundStore.set(null);
  loading = null;
}
