// Building key-assignment intents (`KeymapEditIntent`): one constructor per
// change kind, the built-in acknowledgements a set of changes needs, and the
// intent itself with the keys state's revision as `based_on`. Intents carry
// no raw bytes: only actions, programs and definitions the library checks.

import type {
  ActionValue,
  AdvancedDefView,
  BuiltinAckIntent,
  KeymapChangeIntent,
  KeymapEditIntent,
  KeysState,
  LayerView,
  MacroProgramView,
} from "../../../ipc/bindings";

/** The most changes one intent may carry (the library's limit). */
export const MAX_CHANGES = 200;

/** Constructors of the changes (slots are the library's, 0-based; `null` lets the library choose one). */
export const change = {
  assign: (layer: LayerView, key: number, action: ActionValue): KeymapChangeIntent => ({ change: "assign", layer, key, action }),
  reset: (layer: LayerView, key: number): KeymapChangeIntent => ({ change: "reset", layer, key }),
  swap: (layer: LayerView, first: number, second: number): KeymapChangeIntent => ({ change: "swap", layer, first, second }),
  bindMacro: (layer: LayerView, key: number, program: MacroProgramView, slot: number | null = null): KeymapChangeIntent => ({
    change: "bind_macro",
    layer,
    key,
    program,
    slot,
  }),
  setMacro: (slot: number, program: MacroProgramView): KeymapChangeIntent => ({ change: "set_macro", slot, program }),
  clearMacro: (slot: number): KeymapChangeIntent => ({ change: "clear_macro", slot }),
  deleteMacro: (slot: number): KeymapChangeIntent => ({ change: "delete_macro", slot }),
  bindAdvanced: (layer: LayerView, key: number, def: AdvancedDefView, slot: number | null = null): KeymapChangeIntent => ({
    change: "bind_advanced",
    layer,
    key,
    def,
    slot,
  }),
  setAdvanced: (slot: number, def: AdvancedDefView): KeymapChangeIntent => ({ change: "set_advanced", slot, def }),
  clearAdvanced: (slot: number): KeymapChangeIntent => ({ change: "clear_advanced", slot }),
  deleteAdvanced: (slot: number): KeymapChangeIntent => ({ change: "delete_advanced", slot }),
} as const;

/** The options of an intent besides its changes. */
export interface IntentOptions {
  /** Acknowledged built-ins (critical ones only after the user's explicit confirmation). */
  acks?: readonly BuiltinAckIntent[] | undefined;
  keepOrphans?: boolean | undefined;
  /** Only after an `unsupported_current_table` refusal on a macro or advanced slot. */
  allowNotUndoable?: boolean | undefined;
  /** "Check that the keyboard kept it" (never pre-ticked). */
  checkPersistence?: boolean | undefined;
}

/** The Fn-layer key a change puts something else on, if any (a reset gives back the default: no ack). */
function fnTargets(item: KeymapChangeIntent): number[] {
  switch (item.change) {
    case "assign":
      return item.layer === "fn" && item.action.kind !== "default" ? [item.key] : [];
    case "bind_macro":
    case "bind_advanced":
      return item.layer === "fn" ? [item.key] : [];
    case "swap":
      return item.layer === "fn" ? [item.first, item.second] : [];
    case "reset":
    case "set_macro":
    case "clear_macro":
    case "delete_macro":
    case "set_advanced":
    case "clear_advanced":
    case "delete_advanced":
      return [];
  }
}

/** A built-in a set of changes would replace. */
export interface NeededAck {
  key: number;
  critical: boolean;
}

/**
 * The built-in Fn functions the changes replace (one per key): each needs
 * an acknowledgement; a critical one (Fn + 1..4, Fn + Space) only after the
 * user's explicit confirmation.
 */
export function neededAcks(changes: readonly KeymapChangeIntent[], state: KeysState | null): NeededAck[] {
  const needed = new Map<number, NeededAck>();
  for (const item of changes) {
    for (const index of fnTargets(item)) {
      const builtin = state?.keys.find((key) => key.key.index === index)?.builtin ?? null;
      if (builtin !== null) needed.set(index, { key: index, critical: builtin.critical });
    }
  }
  return [...needed.values()];
}

/**
 * The acknowledgements for an intent: the non-critical built-ins the changes
 * replace (the page showed their warning), plus the critical ones the user
 * confirmed (`confirmed`). A critical built-in without confirmation is left
 * out, so the library refuses it and the flow asks.
 */
export function acksFor(
  changes: readonly KeymapChangeIntent[],
  state: KeysState | null,
  confirmed: readonly BuiltinAckIntent[] = [],
): BuiltinAckIntent[] {
  const acks: BuiltinAckIntent[] = [];
  for (const need of neededAcks(changes, state)) {
    if (!need.critical) acks.push({ key: need.key, critical: false });
    else if (confirmed.some((ack) => ack.key === need.key && ack.critical)) acks.push({ key: need.key, critical: true });
  }
  return acks;
}

/** The intent for `changes` against the keys state (`based_on` = its revision); `null` without changes or state. */
export function buildKeymapIntent(
  changes: readonly KeymapChangeIntent[],
  state: KeysState | null,
  options: IntentOptions = {},
): KeymapEditIntent | null {
  if (state === null || changes.length === 0) return null;
  return {
    changes: [...changes],
    builtin_acks: acksFor(changes, state, options.acks ?? []),
    keep_orphans: options.keepOrphans ?? false,
    allow_not_undoable: options.allowNotUndoable ?? false,
    check_persistence: options.checkPersistence ?? false,
    based_on: state.revision,
  };
}
