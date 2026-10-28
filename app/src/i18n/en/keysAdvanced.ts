// Keys > Advanced keys: the tab's own texts. Shared Keys words (layers,
// action names, slot words, the review) are in keys.ts.

import type { AdvancedDefView, DksStepView } from "../../ipc/bindings";

export const keysAdvanced = {
  title: "Advanced keys",
  intro:
    "Advanced keys let one key do more: different keys at different depths, a tap that differs from a hold, or a key that stays down until you press it again.",
  /** The tiles that start a new advanced key. */
  create: {
    title: "New advanced key",
    hint: "Choose a kind. You choose the key and set it up next; nothing is written until you review it.",
    noFreeSlot: "Every advanced-key slot is in use. Delete an advanced key first.",
    sentences: {
      dks: "Up to four keys at four points of one key press: a light press can type one key, a full press another.",
      mod_tap: "A quick tap sends one key; holding it a little longer holds another one, such as Ctrl.",
      toggle: "One press holds a key down until you press it again.",
    } satisfies Record<AdvancedDefView["type"], string>,
  },
  /** The cards of the slots in use. */
  list: {
    title: (p: { n: number }) => `On the keyboard (profile ${p.n})`,
    empty: "There are no advanced keys on this profile yet.",
    newTitle: "New, not written yet",
  },
  card: {
    usedBy: "Used by",
    orphan: "No key uses it: the keyboard keeps it, but it does nothing.",
    kept: (p: { type: number }) =>
      `OpenWraith can't read this kind of advanced key (type ${p.type}). It is kept exactly as the keyboard stores it.`,
    edit: "Edit…",
    editNamed: (p: { name: string }) => `Edit ${p.name}`,
    assign: "Assign to a key…",
    assignNamed: (p: { name: string }) => `Assign ${p.name} to a key`,
    delete: "Delete",
    deleteNamed: (p: { name: string }) => `Delete ${p.name}`,
    keep: "Keep it",
    keepNamed: (p: { name: string }) => `Keep ${p.name}`,
    drop: "Drop this change",
    dropNamed: (p: { name: string }) => `Drop the change of ${p.name}`,
    tags: {
      new: "New",
      changed: "Changed",
      deleted: "Will be deleted",
      assigned: (p: { key: string }) => `Will be used by ${p.key}`,
    },
    deleteNote: (p: { keys: string }) => `Back to Default: ${p.keys}.`,
    newOn: (p: { key: string }) => `On ${p.key}`,
    now: (p: { text: string }) => `Now: ${p.text}`,
    technical: {
      slot: (p: { slot: number }) => `Slot ${p.slot} (counted from 0).`,
      kept: (p: { type: string; hex: string }) => `Type byte ${p.type}, stored bytes ${p.hex}.`,
      chosen: (p: { slot: number }) =>
        `The slot is chosen when it is written: the slot only this key uses, else the lowest free one (now slot ${p.slot}, counted from 0).`,
      noneFree: "The slot is chosen when it is written; no slot is free right now.",
      firstPoint: (p: { value: number }) => `First point ${p.value} × 0.1 mm.`,
      row: (p: { n: number; usage: string; steps: string }) => `Row ${p.n}: usage ${p.usage}, steps ${p.steps}.`,
      holdTime: (p: { value: number }) => `Hold time ${p.value} ms.`,
    },
  },
  /** The editor dialog. */
  editor: {
    titleNew: (p: { type: string }) => `New advanced key: ${p.type}`,
    titleEdit: (p: { name: string }) => `Edit ${p.name}`,
    target: "Key on the keyboard",
    targetHint: "The key that does this. What it does now is replaced.",
    layer: "Layer",
    now: (p: { text: string }) => `Now: ${p.text}`,
    needTarget: "Choose the key on the keyboard first.",
    needConfirm: "Tick the box to confirm replacing this shortcut.",
    blocked: {
      fn_key: "The Fn key can't be reassigned.",
      knob: "The knob is managed by the keyboard.",
      fn_locked: "The keyboard locks this key on the Fn layer.",
      not_assignable: "This key can't be changed on this layer.",
    },
    add: "Add to draft",
    update: "Update draft",
    change: "Change…",
    changeKey: (p: { what: string; key: string }) => `${p.what}: ${p.key}. Change…`,
    pickTitle: (p: { what: string }) => `Choose the key: ${p.what}`,
  },
  /** The dynamic keystroke (DKS) editor. */
  dks: {
    firstPoint: "First point",
    firstPointHint: (p: { max: string }) =>
      `How far the key goes down before stage 1, and back up before stage 4. Stages 2 and 3 happen at the bottom-out point (${p.max}).`,
    gauge: "Points of this dynamic keystroke",
    gaugeFirst: "First point (stages 1 and 4)",
    gaugeBottom: "Bottom-out point (stages 2 and 3)",
    grid: "What each key does at each point",
    gridHint: "Move with the arrow keys; Space or Enter changes a cell. You can also choose below.",
    keyColumn: "Key",
    actionsColumn: "Remove",
    stages: {
      s1: "1 Press",
      s2: "2 Bottom",
      s3: "3 Lift",
      s4: "4 Release",
    },
    stageNames: {
      s1: "pressing past the first point",
      s2: "reaching the bottom",
      s3: "lifting from the bottom",
      s4: "releasing past the first point",
    },
    steps: {
      none: "Nothing",
      tap: "Tap",
      hold: "Hold to the next point",
      hold_again: "Let go and press again",
    } satisfies Record<DksStepView, string>,
    stepsShort: {
      none: "Nothing",
      tap: "Tap",
      hold: "Hold",
      hold_again: "Again",
    } satisfies Record<DksStepView, string>,
    cellLabel: (p: { key: string; stage: string; step: string }) => `${p.key}, ${p.stage}: ${p.step}`,
    rowKey: (p: { key: string }) => `Key ${p.key}. Change…`,
    removeRow: (p: { key: string }) => `Remove ${p.key}`,
    addRow: "Add a key",
    addRowFull: "A dynamic keystroke has at most four keys.",
    rowKeyWhat: (p: { n: number }) => `key ${p.n}`,
    stripLabel: (p: { key: string; stage: string }) => `${p.key} when ${p.stage}`,
    needRow: "Add at least one key.",
    rowEmpty: (p: { key: string }) => `${p.key} does nothing at any point: choose a step or remove it.`,
    rowInvalid: (p: { key: string }) => `The keyboard doesn't accept these steps for ${p.key}.`,
    pointRange: "The first point is outside the keyboard's range.",
    quickTitle: "Quick starts",
    quickHint: "They fill in the steps of the first keys; change the keys afterwards with their buttons.",
    quick: {
      twoDepths: "A light press types one key, a full press another",
      pressLift: "One key on the press, another when you lift from the bottom",
      holdThenSwitch: "Hold one key until the bottom, then hold another there",
    },
    sentence: {
      stage1: (p: { mm: string }) => `Light press (${p.mm})`,
      stage2: "Full press",
      stage3: "Lifting from the bottom",
      stage4: (p: { mm: string }) => `Letting go (${p.mm})`,
      tap: (p: { key: string }) => `taps ${p.key}`,
      hold: (p: { key: string }) => `holds ${p.key} until the next point`,
      hold_again: (p: { key: string }) => `lets go of ${p.key} and presses it again`,
      line: (p: { stage: string; actions: string }) => `${p.stage}: ${p.actions}.`,
      nothing: "Nothing happens yet.",
    },
  },
  /** The Tap-or-hold editor. */
  mt: {
    tap: "On a tap",
    hold: "On a hold",
    holdTime: "Hold time",
    holdTimeHint: (p: { min: string; max: string }) =>
      `How long the key has to stay down to count as a hold (${p.min} to ${p.max}).`,
    ms: (p: { n: number }) => `${p.n} ms`,
    useConfirmed: (p: { ms: string }) => `Use ${p.ms} (tested)`,
    timeRange: "The hold time is outside the keyboard's range.",
    sentence: (p: { tap: string; hold: string; ms: string }) =>
      `A tap sends ${p.tap}. Held longer than ${p.ms}, it holds ${p.hold}.`,
  },
  /** The Toggle editor. */
  toggle: {
    key: "Key",
    sentence: (p: { key: string }) => `One press holds ${p.key} down; the next press lets it go.`,
  },
  /** Assigning a slot no key uses. */
  assign: {
    title: (p: { name: string }) => `Assign ${p.name} to a key`,
    hint: "The key you choose runs this advanced key. What it does now is replaced.",
    confirm: "Add to draft",
  },
  /** The unwritten changes. */
  draft: {
    idle: "No unwritten changes.",
    notWritten: "Not written yet",
    count: (p: { n: number }) => (p.n === 1 ? "1 change" : `${p.n} changes`),
    discarded: (p: { n: number }) =>
      `The unwritten advanced-key changes were discarded: the keyboard is on profile ${p.n} now.`,
    discardedReconnect: "The unwritten advanced-key changes were discarded: the keyboard was connected again.",
  },
} as const;
