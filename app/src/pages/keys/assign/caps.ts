// Keys > Assignments: what each key cap shows on the chosen layer. A changed
// key shows what it does now (in the accent colour) with its own legend as
// the small second line; on the Fn layer a key that runs one of the
// keyboard's own functions shows that function as the muted second line; a
// drafted change is shown as it will be, with the "not written yet" dot; a
// key that refers to an empty slot gets the warning mark. The full text (now,
// after writing, evidence) is the key's accessible description and tooltip.

import { actionId, catalogEntry, entryName, hex2, usageEntry } from "../../../components/actions";
import type { KeyVisual } from "../../../components/keyboard";
import { builtinCap, builtinName } from "../../../flow/labels";
import { W75_KEYS } from "../../../generated/w75Keys";
import { keyName, type Dict } from "../../../i18n";
import type { ActionValue, KeyAssignmentsView, KeysState, LayerView } from "../../../ipc/bindings";
import { actionText, entryText, keyAt, type LabelContext } from "../shared/labels";
import { entryOn, isAssignable, targetOf, type AssignDraft } from "./model";

const LIBRARY_NAMES: ReadonlyMap<number, string> = new Map(W75_KEYS.map((key) => [key.index, key.name]));

/** The legend printed on a key cap, by the library's key name ("CAPS"; word legends are the same in every language). */
export function capLegend(dict: Dict, name: string): string {
  const legends: Readonly<Record<string, string>> = dict.actuation.legends;
  return legends[name] ?? name;
}

/** The short cap text of a HID usage ("CTRL", "A", "F13"). */
function usageCap(usage: number, dict: Dict): string {
  const entry = usageEntry(usage);
  return entry === undefined ? `0x${hex2(usage)}` : capLegend(dict, entry.name);
}

/** The short text an action shows on a cap ("CTRL+C", "Mute", "M3", "DKS"). */
export function actionCap(value: ActionValue, state: KeysState, dict: Dict): string {
  const t = dict.keysAssign.cap;
  switch (value.kind) {
    case "default":
      return dict.keys.action.default;
    case "key":
      return usageCap(value.usage, dict);
    case "combo":
      return [...value.modifiers, value.key].map((usage) => usageCap(usage, dict)).join("+");
    case "media":
    case "mouse": {
      const entry = catalogEntry(actionId(value));
      return entry === undefined ? actionId(value) : entryName(entry, dict);
    }
    case "macro":
      return t.macro({ n: value.slot + 1 });
    case "advanced": {
      const content = state.advanced.find((slot) => slot.slot === value.slot)?.content;
      return content?.state === "def" ? t.advancedTypes[content.def.type] : t.advanced({ n: value.slot + 1 });
    }
  }
}

/**
 * What an action means for a key on a layer, in words: the Fn layer's
 * default as the built-in it runs ("Default: Media player").
 */
export function valueText(
  value: ActionValue,
  key: KeyAssignmentsView,
  layer: LayerView,
  labels: LabelContext,
  dict: Dict,
): string {
  if (value.kind === "default" && layer === "fn" && key.builtin !== null) {
    return dict.keys.action.defaultBuiltin({ function: builtinName(key.builtin.index, key.builtin.function, dict) });
  }
  return actionText(value, labels, dict);
}

/** Why a key can't be changed on a layer (the Fn key, an Fn-locked entry), or `null`. */
export function notAssignableText(key: KeyAssignmentsView, layer: LayerView, dict: Dict): string | null {
  if (isAssignable(key, layer)) return null;
  if (key.kind === "fn_key") return dict.keys.fnLocked;
  if (key.kind === "knob") return dict.keys.knob;
  return dict.keysAssign.notAssignable.fnLocked;
}

/** What the caps of one layer show. */
export interface CapContext {
  state: KeysState;
  draft: AssignDraft;
  layer: LayerView;
  labels: LabelContext;
  dict: Dict;
}

/** Keys that refer to an empty slot or hold a malformed reference, as `layer:index`. */
export function brokenKeys(state: KeysState): ReadonlySet<string> {
  return new Set(state.broken.map((item) => `${item.layer}:${item.key.index}`));
}

/** The cap of one key on the context's layer. */
export function keyCap(ctx: CapContext, index: number, broken: ReadonlySet<string>): KeyVisual {
  const { state, draft, layer, labels, dict } = ctx;
  const t = dict.keysAssign;
  const key = keyAt(state, index);
  const libraryName = key?.key.name ?? LIBRARY_NAMES.get(index) ?? String(index);
  const name = keyName(dict, libraryName);
  const legend = capLegend(dict, libraryName);
  if (key === undefined || key.kind === "knob") return { legend, name, selectable: false };
  const blocked = notAssignableText(key, layer, dict);
  if (blocked !== null) return { legend, name, selectable: false, tooltip: blocked, description: blocked };

  const entry = entryOn(key, layer);
  const target = targetOf(draft, layer, index);
  const shown = target?.action ?? entry.value;
  const builtin = layer === "fn" ? key.builtin : null;

  let capText = legend;
  let sublegend: string | null;
  let flag = false;
  if (shown === null) {
    // An entry kept as read (no target: a target is always an action).
    sublegend = entry.preserved === null ? null : dict.keys.preservedKinds[entry.preserved.kind];
    flag = true;
  } else if (shown.kind === "default") {
    // A short label on the cap (about 48 px wide); the full name is in the description and the panel.
    sublegend = builtin === null ? null : builtinCap(builtin.index, builtin.function, dict);
  } else {
    capText = actionCap(shown, state, dict);
    sublegend = legend;
    flag = true;
  }

  const isBroken = broken.has(`${layer}:${index}`);
  const parts = [t.keyInfo.now({ what: entryText(entry, key, labels, dict) })];
  if (entry.evidence?.status === "unverified") parts.push(t.keyInfo.unverified);
  if (isBroken) parts.push(t.keyInfo.broken);
  if (target !== undefined) parts.push(t.keyInfo.draft({ next: valueText(target.action, key, layer, labels, dict) }));
  const description = parts.join(" ");

  return {
    legend: capText,
    name,
    sublegend,
    flag,
    marker: target !== undefined ? "pending" : isBroken ? "warning" : null,
    description,
    tooltip: description,
  };
}
