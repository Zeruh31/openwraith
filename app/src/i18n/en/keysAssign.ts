// Keys > Assignments: the tab's own texts (the board with its layer switch,
// the selected key, swapping, keys that refer to empty slots, the apply
// bar). Shared Keys words (layers, actions, built-ins, the review) are in
// keys.ts.

import type { AdvancedDefView, LayerView } from "../../ipc/bindings";

export const keysAssign = {
  title: "Assignments",
  intro:
    "Select a key to see what it does on the base and Fn layers, and change it. Nothing is written until you review the changes and press Write.",
  layer: {
    label: "Layer",
    hint: "Press L to switch between the base and Fn layers.",
    shown: {
      base: "Showing the base layer",
      fn: "Showing the Fn layer",
    } satisfies Record<LayerView, string>,
  },
  board: {
    title: "What the keys do",
    label: {
      base: "Keys on the base layer",
      fn: "Keys on the Fn layer",
    } satisfies Record<LayerView, string>,
    hint: "Click a key, or move with the arrow keys and press Space, to select it. Enter or a double-click opens Change….",
  },
  legend: {
    label: "What the marks on the keys mean",
    remapped: "Changed: what the key does now, with its own legend below",
    builtin: "The keyboard's own Fn function",
    pending: "Not written yet",
    broken: "Refers to an empty macro or advanced key",
  },
  /** Short texts on key caps (the full meaning is the key's description). */
  cap: {
    macro: (p: { n: number }) => `M${p.n}`,
    advanced: (p: { n: number }) => `A${p.n}`,
    advancedTypes: {
      dks: "DKS",
      mod_tap: "MT",
      toggle: "TGL",
    } satisfies Record<AdvancedDefView["type"], string>,
  },
  /** A key's accessible description and tooltip. */
  keyInfo: {
    now: (p: { what: string }) => `Now: ${p.what}.`,
    draft: (p: { next: string }) => `Not written yet: ${p.next}.`,
    broken: "It refers to an empty slot, so it does nothing.",
    unverified: "Not yet verified on a W75.",
  },
  notAssignable: {
    fnLocked: "This key's Fn-layer entry can't be changed: the keyboard keeps it for the Fn key.",
  },
  knob: {
    entriesTitle: (p: { layer: string }) => `Stored entries (${p.layer})`,
    bytes: (p: { n: number; hex: string }) => `Knob entry ${p.n}: ${p.hex}`,
  },
  summary: {
    title: "Selected key",
    none: "Select a key on the board to see what it does and change it.",
    does: "What it does",
    origin: "Kind",
    slot: "Slot content",
    emptySlot: "Empty: the key does nothing until the slot is filled or the key is reset.",
    builtin: "Keyboard's own function",
    otherLayer: {
      base: "On the base layer",
      fn: "On the Fn layer",
    } satisfies Record<LayerView, string>,
    after: "After writing",
    openMacros: "Open the Macros tab",
    openAdvanced: "Open the Advanced keys tab",
    bytes: (p: { layer: string; hex: string }) => `${p.layer}: ${p.hex}`,
    keptText: (p: { text: string }) => `Read as: ${p.text}`,
    builtinReplaced: (p: { key: string; function: string }) =>
      `${p.key} doesn't run the keyboard's own function (${p.function}) on this profile now. Reset to default brings it back.`,
  },
  actions: {
    label: "Change the selected key",
    change: "Change…",
    reset: "Reset to default",
    swap: "Swap with…",
    keep: "Keep what the keyboard has",
    reasons: {
      alreadyDefault: "The key already does its default.",
      keptAsRead: "This entry is kept as read, so it can't be moved. You can still change it or reset it.",
      confirmFirst: "Tick the confirmation above to change this key.",
    },
  },
  picker: {
    criticalConfirmed: "You confirmed replacing this shortcut. It is replaced only when you write the change.",
  },
  swap: {
    title: (p: { key: string }) => `Swap ${p.key} with…`,
    intro: "The two keys exchange what they do on this layer. Nothing is written until you review and write.",
    label: "Other key",
    result: (p: { a: string; aNew: string; b: string; bNew: string }) =>
      `${p.a} will do: ${p.aNew}. ${p.b} will do: ${p.bNew}.`,
    confirm: "Swap",
    pickKey: "Choose the other key.",
    reasons: {
      kept: "Kept as read: can't be moved",
      same: "Already does the same",
      critical: "Select this key first and confirm replacing its shortcut",
    },
  },
  broken: {
    title: "Keys that refer to an empty slot",
    intro:
      "These keys refer to a macro or advanced key that holds nothing, so they do nothing. Reset them here, or fill the slot on the Macros or Advanced keys tab.",
    emptySlot: (p: { key: string; slot: string }) => `${p.key} refers to ${p.slot}, which is empty.`,
    malformed: (p: { key: string }) => `${p.key} holds a reference OpenWraith can't read.`,
    reset: "Reset key",
    resetLabel: (p: { key: string }) => `Reset ${p.key}`,
    inDraft: "Reset in the draft (not written yet).",
    notResettable: "OpenWraith can't reset this key here.",
  },
  apply: {
    idle: "No unwritten key changes.",
    notWritten: "Not written yet",
    count: (p: { n: number }) => (p.n === 1 ? "1 key will change" : `${p.n} keys will change`),
  },
  draftDiscarded: (p: { n: number }) =>
    `Unwritten key changes were discarded: the keyboard is now on profile ${p.n}.`,
  draftDiscardedReconnect: "Unwritten key changes were discarded: the keyboard was connected again.",
} as const;
