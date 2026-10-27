// The UI language: dictionaries, the language choice and the `useDict()`
// hook. UI strings live only in the dictionaries (`en/*.ts`, `tr/*.ts`).

import type { LanguageSetting } from "../ipc/bindings";
import { createStore, useStore } from "../state/store";
import { actuation as enActuation } from "./en/actuation";
import { backupRestore as enBackupRestore } from "./en/backupRestore";
import { common as enCommon } from "./en/common";
import { device as enDevice } from "./en/device";
import { errors as enErrors } from "./en/errors";
import { flow as enFlow } from "./en/flow";
import { lighting as enLighting } from "./en/lighting";
import { settings as enSettings } from "./en/settings";
import { shell as enShell } from "./en/shell";
import { actuation as trActuation } from "./tr/actuation";
import { backupRestore as trBackupRestore } from "./tr/backupRestore";
import { common as trCommon } from "./tr/common";
import { device as trDevice } from "./tr/device";
import { errors as trErrors } from "./tr/errors";
import { flow as trFlow } from "./tr/flow";
import { lighting as trLighting } from "./tr/lighting";
import { settings as trSettings } from "./tr/settings";
import { shell as trShell } from "./tr/shell";
import type { Lang, Messages } from "./types";

export type { Lang, Messages } from "./types";

const en = {
  common: enCommon,
  shell: enShell,
  device: enDevice,
  settings: enSettings,
  flow: enFlow,
  errors: enErrors,
  backupRestore: enBackupRestore,
  actuation: enActuation,
  lighting: enLighting,
};

/** A full dictionary (all areas). */
export type Dict = Messages<typeof en>;

const tr: Dict = {
  common: trCommon,
  shell: trShell,
  device: trDevice,
  settings: trSettings,
  flow: trFlow,
  errors: trErrors,
  backupRestore: trBackupRestore,
  actuation: trActuation,
  lighting: trLighting,
};

/** The dictionaries by language. */
export const DICTIONARIES: Readonly<Record<Lang, Dict>> = { en, tr };

/** The languages, for pickers and tests. */
export const LANGS: readonly Lang[] = ["en", "tr"];

/**
 * The language for a setting: `system` follows the operating system's
 * locale, else the browser's languages; Turkish locales give Turkish,
 * everything else English.
 */
export function resolveLang(
  setting: LanguageSetting,
  osLocale: string | null,
  browserLanguages: readonly string[],
): Lang {
  if (setting !== "system") {
    return setting;
  }
  const first = osLocale ?? browserLanguages[0] ?? "en";
  return first.toLowerCase().startsWith("tr") ? "tr" : "en";
}

const langStore = createStore<Lang>("en");

/** Sets the UI language (also `<html lang>`). */
export function setLang(lang: Lang): void {
  langStore.set(lang);
  document.documentElement.lang = lang;
}

/** The current language (outside React). */
export function getLang(): Lang {
  return langStore.get();
}

/** The current dictionary (outside React, e.g. for toasts). */
export function getDict(): Dict {
  return DICTIONARIES[langStore.get()];
}

/** The current language; re-renders when it changes. */
export function useLang(): Lang {
  return useStore(langStore);
}

/** The current dictionary; re-renders when the language changes. */
export function useDict(): Dict {
  return DICTIONARIES[useStore(langStore)];
}

/** The localized accessible name of a key ("Left Shift"), or the library name ("W"). */
export function keyName(dict: Dict, name: string): string {
  const names: Readonly<Record<string, string>> = dict.common.keyNames;
  return names[name] ?? name;
}
