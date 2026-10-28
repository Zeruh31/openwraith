// Words for SOCD values: a mode's name and explanation, a pair's keys.

import { keyName, type Dict } from "../../../i18n";
import type { SocdKeyView, SocdPairView } from "../../../ipc/bindings";

/** "Last input wins" for mode 1 (the vendor site's numbers 1-4). */
export function modeName(mode: number, dict: Dict): string {
  const names: Readonly<Record<string, string>> = dict.actuation.socd.modes;
  return names[`m${mode}`] ?? dict.actuation.socd.modeUnknown({ mode });
}

/** What a mode does, in one sentence. */
export function modeHint(mode: number, dict: Dict): string {
  const hints: Readonly<Record<string, string>> = dict.actuation.socd.modeHints;
  return hints[`m${mode}`] ?? "";
}

/** A pair's key ("A"), or its HID usage when it isn't a key of the layout. */
export function socdKeyText(view: SocdKeyView, dict: Dict): string {
  return view.key === null
    ? dict.actuation.socd.usage({ hex: view.usage.toString(16).toUpperCase().padStart(2, "0") })
    : keyName(dict, view.key.name);
}

/** "A + D". */
export function pairKeysText(pair: SocdPairView, dict: Dict): string {
  return dict.actuation.socd.pairKeys({ key1: socdKeyText(pair.key1, dict), key2: socdKeyText(pair.key2, dict) });
}
