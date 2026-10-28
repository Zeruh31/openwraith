// Words for the keymap group: what a key does (an entry or an action, with
// macro and advanced-key slots named from the keys state and the library),
// slot contents, play modes, built-ins and the slots the action picker
// offers. Every function takes the dictionary (no hard-coded text).

import {
  fixedActionName,
  usageName,
  type ActionSlotOption,
} from "../../../components/actions";
import { builtinName, keyRefText } from "../../../flow/labels";
import { keyName, type Dict } from "../../../i18n";
import type {
  ActionValue,
  AdvancedDefView,
  AdvancedSlotStateView,
  BindingView,
  KeyAssignmentsView,
  KeyEntryView,
  KeysState,
  LayerView,
  MacroPlayView,
  MacroProgramView,
  MacroSlotStateView,
  SlotContentView,
} from "../../../ipc/bindings";

/** What the labels may look up: the keys state and library names by content hash. */
export interface LabelContext {
  state: KeysState | null;
  libraryNames?: ReadonlyMap<string, string> | undefined;
}

/** A key's localized name ("Left Shift"), with "Fn + " on the Fn layer. */
export function keyOnLayer(name: string, layer: LayerView, dict: Dict): string {
  const local = keyName(dict, name);
  return layer === "fn" ? dict.keys.onFn({ key: local }) : local;
}

/** The keys state's entry of a key index. */
export function keyAt(state: KeysState | null, index: number): KeyAssignmentsView | undefined {
  return state?.keys.find((key) => key.key.index === index);
}

/** A macro's play mode ("Plays 3 times"). */
export function playText(play: MacroPlayView, dict: Dict): string {
  const modes = dict.keys.playModes;
  return play.mode === "times" ? modes.times({ n: play.count }) : modes[play.mode];
}

/** A macro program in short ("4 steps · Plays once"). */
export function programText(program: MacroProgramView, dict: Dict): string {
  return dict.keys.slot.macro({ steps: program.steps.length, play: playText(program.play, dict) });
}

/** An advanced-key definition in short ("Tap or hold · A / Left Ctrl · 300 ms"). */
export function advancedDefText(def: AdvancedDefView, dict: Dict): string {
  const type = dict.keys.advancedTypes[def.type];
  switch (def.type) {
    case "dks": {
      const keys = def.rows.filter((row) => row !== null).map((row) => usageName(row.key, dict));
      return keys.length === 0 ? type : `${type} · ${keys.join(", ")}`;
    }
    case "mod_tap":
      return `${type} · ${usageName(def.tap, dict)} / ${usageName(def.hold, dict)} · ${def.hold_ms} ms`;
    case "toggle":
      return `${type} · ${usageName(def.key, dict)}`;
  }
}

/** A slot content of the plan ("Empty", "4 steps · Plays once", "Toggle · H", "Kept as read"). */
export function slotContentText(content: SlotContentView, dict: Dict): string {
  switch (content.kind) {
    case "empty":
      return dict.keys.slot.empty;
    case "macro":
      return programText(content.program, dict);
    case "advanced":
      return advancedDefText(content.def, dict);
    case "kept_as_read":
      return dict.keys.slot.keptAsRead;
  }
}

function macroSlot(state: KeysState | null, slot: number): MacroSlotStateView | undefined {
  return state?.macros.find((m) => m.slot === slot);
}

function advancedSlot(state: KeysState | null, slot: number): AdvancedSlotStateView | undefined {
  return state?.advanced.find((a) => a.slot === slot);
}

/** A macro slot's name: "Macro 3 · Type A" when the library knows its program, else "Macro 3". */
export function macroName(slot: number, ctx: LabelContext, dict: Dict): string {
  const content = macroSlot(ctx.state, slot)?.content;
  const name = content?.state === "program" ? ctx.libraryNames?.get(content.content_hash) : undefined;
  return name === undefined ? dict.keys.action.macro({ n: slot + 1 }) : dict.keys.action.macroNamed({ n: slot + 1, name });
}

/** An advanced-key slot's name: "Advanced key 2 · Tap or hold" when its type is known. */
export function advancedName(slot: number, ctx: LabelContext, dict: Dict): string {
  const content = advancedSlot(ctx.state, slot)?.content;
  return content?.state === "def"
    ? dict.keys.action.advancedTyped({ n: slot + 1, type: dict.keys.advancedTypes[content.def.type] })
    : dict.keys.action.advanced({ n: slot + 1 });
}

/** What an action does, in words. */
export function actionText(value: ActionValue, ctx: LabelContext, dict: Dict): string {
  if (value.kind === "macro") return macroName(value.slot, ctx, dict);
  if (value.kind === "advanced") return advancedName(value.slot, ctx, dict);
  return fixedActionName(value, dict) ?? dict.keys.action.default;
}

/**
 * What a key's entry does, in words: preserved entries by their kind ("Lighting
 * function (kept as read)"), the Fn layer's default as the built-in it runs.
 */
export function entryText(entry: KeyEntryView, key: KeyAssignmentsView | undefined, ctx: LabelContext, dict: Dict): string {
  if (entry.preserved !== null) return dict.keys.action.kept({ kind: dict.keys.preservedKinds[entry.preserved.kind] });
  const value = entry.value;
  if (value === null) return entry.label;
  if (value.kind === "default" && entry.layer === "fn" && key?.builtin !== null && key?.builtin !== undefined) {
    return dict.keys.action.defaultBuiltin({ function: builtinName(key.builtin.index, key.builtin.function, dict) });
  }
  return actionText(value, ctx, dict);
}

/** The keys that use a slot ("Q, Fn + W"). */
export function bindingsText(bindings: readonly BindingView[], dict: Dict): string {
  return bindings.map((binding) => keyOnLayer(binding.key.name, binding.layer, dict)).join(", ");
}

/** Whether a slot's bindings are only this key on this layer (or none). */
function freeFor(bindings: readonly BindingView[], layer: LayerView, key: number | null): boolean {
  return bindings.every((binding) => binding.layer === layer && binding.key.index === key);
}

/**
 * The keyboard's macros for the action picker: every non-empty slot, named
 * from the library, with its steps, its users and its evidence; a slot
 * another key uses can't be chosen (one key per slot).
 */
export function macroPickerOptions(
  ctx: LabelContext,
  layer: LayerView,
  key: number | null,
  dict: Dict,
): ActionSlotOption[] {
  const options: ActionSlotOption[] = [];
  for (const slot of ctx.state?.macros ?? []) {
    const content = slot.content;
    if (content.state === "empty") continue;
    const parts = [content.state === "program" ? programText(content.program, dict) : dict.keys.slot.keptAsRead];
    parts.push(slot.bindings.length === 0 ? dict.keys.slot.orphan : dict.keys.slot.usedBy({ keys: bindingsText(slot.bindings, dict) }));
    options.push({
      slot: slot.slot,
      label: macroName(slot.slot, ctx, dict),
      description: parts.join(" · "),
      evidence: content.state === "program" ? content.evidence : null,
      disabledReason: freeFor(slot.bindings, layer, key)
        ? null
        : dict.keys.slot.usedBy({ keys: bindingsText(slot.bindings, dict) }),
    });
  }
  return options;
}

/** The keyboard's advanced keys for the action picker (as {@link macroPickerOptions}). */
export function advancedPickerOptions(
  ctx: LabelContext,
  layer: LayerView,
  key: number | null,
  dict: Dict,
): ActionSlotOption[] {
  const options: ActionSlotOption[] = [];
  for (const slot of ctx.state?.advanced ?? []) {
    const content = slot.content;
    if (content.state === "empty") continue;
    const parts = [content.state === "def" ? advancedDefText(content.def, dict) : dict.keys.slot.keptAsRead];
    parts.push(slot.bindings.length === 0 ? dict.keys.slot.orphan : dict.keys.slot.usedBy({ keys: bindingsText(slot.bindings, dict) }));
    options.push({
      slot: slot.slot,
      label: advancedName(slot.slot, ctx, dict),
      description: parts.join(" · "),
      evidence: content.state === "def" ? content.evidence : null,
      disabledReason: freeFor(slot.bindings, layer, key)
        ? null
        : dict.keys.slot.usedBy({ keys: bindingsText(slot.bindings, dict) }),
    });
  }
  return options;
}

/** A key as refusals and reviews name it ("Fn + F1 (#2)"). */
export function keyRef(key: KeyAssignmentsView, layer: LayerView, dict: Dict): string {
  return keyRefText(key.key, layer, dict);
}
