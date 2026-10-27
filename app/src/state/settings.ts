// The user's settings (language, theme), saved by the backend.

import { getDict, resolveLang, setLang } from "../i18n";
import type { Settings } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { toAppError } from "../ipc/errors";
import { applyTheme } from "../theme/theme";
import { appInfoStore } from "./appInfo";
import { createStore, useStore } from "./store";
import { toast } from "./toasts";

/** The settings in effect. */
export const settingsStore = createStore<Settings>({ language: "system", theme: "system" });

/** Applies settings to the document (theme) and the dictionaries (language). */
export function applySettings(settings: Settings): void {
  applyTheme(settings.theme);
  setLang(resolveLang(settings.language, appInfoStore.get().os_locale, navigator.languages));
}

/** The settings; re-renders when they change. */
export function useSettings(): Settings {
  return useStore(settingsStore);
}

/** Saves new settings (applied at once; restored when saving fails). */
export async function setSettings(next: Settings): Promise<void> {
  const previous = settingsStore.get();
  settingsStore.set(next);
  applySettings(next);
  try {
    const saved = await api.setSettings(next);
    settingsStore.set(saved);
    applySettings(saved);
  } catch (error) {
    settingsStore.set(previous);
    applySettings(previous);
    const appError = toAppError(error);
    toast({ kind: "danger", title: getDict().settings.saveFailed, details: appError.message });
  }
}
