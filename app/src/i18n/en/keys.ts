// The Keys page's shared words (all three tabs and the shared components that
// show actions): layers, origins, action names and groups, the action
// picker, built-in Fn functions, the key-assignment review and its notes.
// The tabs' own texts are in keysAssign.ts, keysMacros.ts and keysAdvanced.ts.

import type {
  AdvancedDefView,
  CatalogGroup,
  CatalogTab,
  EntryOriginView,
  LayerView,
  MacroPlayView,
  PreservedKindView,
} from "../../ipc/bindings";

export const keys = {
  title: "Keys",
  description:
    "What each key does on the active profile: its assignment on the base and Fn layers, macros and advanced keys.",
  /** The page's one-time tip. */
  tip: {
    title: "How changes to keys are written",
    body: "Every change stays a draft until you choose Review and write. Before writing, OpenWraith saves a safety backup, and each report has Undo. Fn + 1 to 4 and Fn + Space are protected: replacing them needs an extra confirmation.",
  },
  board: {
    title: (p: { n: number }) => `Keys on profile ${p.n}`,
    readAt: (p: { time: string }) => `Read at ${p.time}`,
    readAgain: "Read the keyboard again",
  },
  empty: {
    title: "No key assignments to show yet",
    reading: "Reading the keyboard…",
    body: "Connect the keyboard to see and change what its keys do.",
    failed: "The key assignments couldn't be loaded.",
  },
  stale: "Something changed on the keyboard since it was read. Read it again before changing key assignments.",
  disabled: {
    unreadable_table: (p: { table: string }) =>
      `${p.table} couldn't be read, so key assignments can't be changed now. Read the keyboard again.`,
    unsupported: "OpenWraith can't change the key assignments of this keyboard.",
  },
  advancedDisabled:
    "Advanced keys can't be changed on this keyboard: OpenWraith doesn't know its distance unit.",
  layers: {
    base: "Base layer",
    fn: "Fn layer",
  } satisfies Record<LayerView, string>,
  layerShort: {
    base: "Base",
    fn: "Fn",
  } satisfies Record<LayerView, string>,
  /** A key on a layer: "Fn + Q". */
  onFn: (p: { key: string }) => `Fn + ${p.key}`,
  fnLocked:
    "The Fn key can't be reassigned: without it you couldn't reach the Fn layer or the keyboard's own shortcuts.",
  knob: "The knob is managed by the keyboard. OpenWraith shows its stored entries but never changes them.",
  knobEntry: (p: { n: number; layer: string; text: string }) => `Knob entry ${p.n} (${p.layer}): ${p.text}`,
  /** `likely`: the label isn't confirmed on a W75 yet (the "Not yet verified" badge is beside it). */
  builtinProfile: (p: { k: number; n: number; likely?: boolean }) =>
    p.k === p.n
      ? `Fn + ${p.k} ${p.likely === true ? "most likely switches" : "switches"} to profile ${p.k}, the profile you are changing, so on this profile it switches nowhere and replacing it costs nothing. The keyboard's own shortcut still needs your confirmation.`
      : `Fn + ${p.k} ${p.likely === true ? "most likely switches" : "switches"} to profile ${p.k}. If you assign something else, you can't switch to profile ${p.k} from the keyboard while profile ${p.n} is active, only from OpenWraith.`,
  builtinReset:
    "Fn + Space (held) resets the keyboard. What happens when it is replaced has not been tested.",
  builtinWarning: (p: { key: string; function: string }) =>
    `${p.key} runs the keyboard's own function (${p.function}). If you assign something else, that function is gone on this profile until you set the key back to Default.`,
  builtinConfirmProfile: (p: { k: number; n: number }) =>
    p.k === p.n
      ? `Replace Fn + ${p.k} on profile ${p.n} (it only switches to this same profile)`
      : `Fn + ${p.k} will no longer switch to profile ${p.k} while profile ${p.n} is active`,
  builtinConfirmReset: (p: { n: number }) =>
    `Fn + Space will no longer reset the keyboard while profile ${p.n} is active`,
  /** The keyboard's own Fn functions, by key index (the library's English name is the fallback). */
  /** Short keycap labels of the built-ins (the full names are in `builtins`). */
  builtinCaps: {
    "2": "Media",
    "3": "Vol −",
    "4": "Vol +",
    "5": "Mute",
    "6": "Stop",
    "7": "Prev",
    "8": "Play",
    "9": "Next",
    "10": "Mail",
    "11": "Home pg",
    "12": "Calc",
    "13": "Search",
    "14": "Insert",
    "23": "Prof 1",
    "24": "Prof 2",
    "25": "Prof 3",
    "26": "Prof 4",
    "33": "Speed −",
    "34": "Speed +",
    "36": "Lights off",
    "37": "End",
    "58": "All lights",
    "68": "Windows",
    "69": "macOS",
    "101": "SOCD",
    "102": "Bright +",
    "111": "Win lock",
    "116": "Reset",
    "123": "Direction",
    "124": "Bright −",
    "125": "Colour",
  } as Record<string, string>,
  builtins: {
    "2": "Media player",
    "3": "Volume down",
    "4": "Volume up",
    "5": "Mute",
    "6": "Stop",
    "7": "Previous track",
    "8": "Play/Pause",
    "9": "Next track",
    "10": "Mail",
    "11": "Browser home",
    "12": "Calculator",
    "13": "Browser search",
    "14": "Insert",
    "23": "Profile 1",
    "24": "Profile 2",
    "25": "Profile 3",
    "26": "Profile 4",
    "33": "Lighting speed down",
    "34": "Lighting speed up",
    "36": "All lights off",
    "37": "End",
    "58": "All lighting",
    "68": "Windows mode",
    "69": "Mac mode",
    "101": "SOCD on/off",
    "102": "Brightness up",
    "111": "Windows key lock",
    "116": "Reset (held)",
    "123": "Effect direction",
    "124": "Brightness down",
    "125": "Effect colour",
  } as Readonly<Record<string, string>>,
  builtinLabelUnconfirmed: "The keyboard's name for this function hasn't been checked on a W75 yet.",
  origin: {
    default: "Default",
    remapped: "Changed",
    macro_slot: "Runs a macro",
    advanced_slot: "Advanced key",
    preserved: "Kept as read",
  } satisfies Record<EntryOriginView["origin"], string>,
  action: {
    default: "Default",
    defaultBuiltin: (p: { function: string }) => `Default: ${p.function}`,
    macro: (p: { n: number }) => `Macro ${p.n}`,
    macroNamed: (p: { n: number; name: string }) => `Macro ${p.n} · ${p.name}`,
    advanced: (p: { n: number }) => `Advanced key ${p.n}`,
    advancedTyped: (p: { n: number; type: string }) => `Advanced key ${p.n} · ${p.type}`,
    keypad: (p: { key: string }) => `Keypad ${p.key}`,
    kept: (p: { kind: string }) => `${p.kind} (kept as read)`,
  },
  preservedKinds: {
    fn_key: "Fn key",
    lighting: "Lighting function",
    function: "Keyboard function",
    gamepad: "Gamepad input",
    key: "Key code",
    media: "Media key",
    mouse: "Mouse action",
    combo: "Shortcut",
    macro: "Macro reference",
    advanced: "Advanced-key reference",
    unknown: "Unknown entry",
  } satisfies Record<PreservedKindView, string>,
  advancedTypes: {
    dks: "Dynamic keystroke (DKS)",
    mod_tap: "Tap or hold",
    toggle: "Toggle",
  } satisfies Record<AdvancedDefView["type"], string>,
  playModes: {
    once: "Plays once",
    times: (p: { n: number }) => `Plays ${p.n} times`,
    toggle: "A press starts it, the next press stops it",
    while_held: "Repeats while held",
  } satisfies Record<MacroPlayView["mode"], unknown>,
  slot: {
    empty: "Empty",
    keptAsRead: "Kept as read",
    macro: (p: { steps: number; play: string }) =>
      p.steps === 1 ? `1 step · ${p.play}` : `${p.steps} steps · ${p.play}`,
    used: (p: { n: number; total: number }) => `${p.n} of ${p.total} used`,
    usedBy: (p: { keys: string }) => `Used by ${p.keys}`,
    orphan: "Not used by any key",
  },
  /** Groups of the action catalogue. */
  groups: {
    default: "Default",
    letters: "Letters",
    digits: "Digits",
    basic: "Basic keys",
    symbols: "Symbols",
    function: "Function keys",
    system: "System",
    navigation: "Navigation",
    arrows: "Arrow keys",
    keypad: "Keypad",
    international: "International",
    modifiers: "Modifier keys",
    playback: "Playback",
    browser: "Browser",
    apps: "Apps",
    mouse_buttons: "Mouse buttons",
    mouse_wheel: "Mouse wheel",
  } satisfies Record<CatalogGroup, string>,
  /**
   * Display names of catalogue actions by id, where the library's English
   * name isn't the best one to show (the library name stays searchable).
   */
  actionNames: {
    default: "Default",
    "key:2a": "Backspace",
    "key:2c": "Space",
    "key:2d": "Minus (-)",
    "key:2e": "Equals (=)",
    "key:2f": "Left bracket ([)",
    "key:30": "Right bracket (])",
    "key:31": "Backslash (\\)",
    "key:33": "Semicolon (;)",
    "key:34": "Quote (')",
    "key:35": "Backtick (`)",
    "key:36": "Comma (,)",
    "key:37": "Full stop (.)",
    "key:38": "Slash (/)",
    "key:46": "Print Screen",
    "key:47": "Scroll Lock",
    "key:48": "Pause",
    "key:49": "Insert",
    "key:4d": "End",
    "key:53": "Num Lock",
    "key:64": "Non-US backslash (\\)",
    "key:65": "Menu",
    "key:e7": "Right Windows",
    "media:10": "Volume up",
    "media:11": "Volume down",
    "media:12": "Mute",
    "media:13": "Play/Pause",
    "media:14": "Next track",
    "media:15": "Previous track",
    "media:16": "Stop",
    "media:20": "Mail",
    "media:21": "Browser search",
    "media:22": "Favourites",
    "media:23": "Browser home",
    "media:24": "Browser back",
    "media:25": "Browser forward",
    "media:26": "Browser stop",
    "media:27": "Refresh",
    "media:30": "Media player",
    "media:32": "Calculator",
    "media:34": "This PC",
    "mouse:left": "Left click",
    "mouse:right": "Right click",
    "mouse:middle": "Middle click",
    "mouse:back": "Back button (X1)",
    "mouse:forward": "Forward button (X2)",
    "mouse:wheel_up": "Wheel up",
    "mouse:wheel_down": "Wheel down",
  } as Readonly<Record<string, string>>,
  picker: {
    title: (p: { key: string }) => `Change ${p.key}`,
    search: "Search actions",
    searchHint: "A name, a key code such as 0x04, or a shortcut such as Ctrl C.",
    searchHintKeys: "A name or a key code such as 0x04.",
    options: "Actions",
    tabsLabel: "Kinds of actions",
    tabs: {
      keys: "Keys",
      media: "Media",
      mouse: "Mouse",
      shortcuts: "Shortcuts",
      macros: "Macros",
      advanced: "Advanced",
      default: "Default",
    } satisfies Record<CatalogTab, string>,
    results: (p: { n: number }) => (p.n === 1 ? "1 action" : `${p.n} actions`),
    noResults: "Nothing matches.",
    current: "Current",
    choose: "Choose",
    defaultHint: "Gives the key back its own function on this layer.",
    shortcut: {
      modifiers: "Modifier keys (one or two)",
      key: "Key",
      keyPlaceholderHint: "Choose the key that goes with the modifiers.",
      use: (p: { combo: string }) => `Use ${p.combo}`,
      needModifier: "Choose one or two modifier keys.",
      needKey: "Choose the key.",
      max: "At most two modifier keys.",
    },
    macros: {
      empty: "There are no macros on the keyboard yet. Make one on the Macros tab.",
    },
    advanced: {
      empty: "There are no advanced keys yet. Make one on the Advanced keys tab.",
    },
  },
  /** The macro library of this PC. */
  library: {
    invalidKept: "The macro library file couldn't be read",
    invalidKeptBody: (p: { file: string }) =>
      `OpenWraith kept it as ${p.file} and started an empty library. Your keyboard's macros are not affected.`,
  },
  /** The key-assignment review (the plan in words). */
  review: {
    /** `what`: what the plan changes (key assignments by default). */
    title: (p: { n: number; what?: "keys" | "macros" | "advanced" | "mixed" }) => {
      switch (p.what) {
        case "macros":
          return `Change macros on profile ${p.n}?`;
        case "advanced":
          return `Change advanced keys on profile ${p.n}?`;
        case "mixed":
          return `Change keys, macros and advanced keys on profile ${p.n}?`;
        default:
          return `Change key assignments on profile ${p.n}?`;
      }
    },
    nothingTitle: "Nothing to change",
    nothing: "The keyboard already has these key assignments.",
    summary: (p: { keys: number; macros: number; advanced: number; writes: number }) => {
      const parts = [
        p.keys === 0 ? null : p.keys === 1 ? "1 key" : `${p.keys} keys`,
        p.macros === 0 ? null : p.macros === 1 ? "1 macro" : `${p.macros} macros`,
        p.advanced === 0 ? null : p.advanced === 1 ? "1 advanced key" : `${p.advanced} advanced keys`,
      ].filter((part): part is string => part !== null);
      const what = parts.length === 0 ? "No visible change" : `Changes ${parts.join(" and ")}`;
      return p.writes === 1 ? `${what}, in 1 write.` : `${what}, in ${p.writes} writes.`;
    },
    entriesTitle: "Keys",
    columns: {
      layer: "Layer",
      key: "Key",
      now: "Now",
      new: "New",
    },
    slotsTitle: "Macros and advanced keys",
    writesTitle: "What OpenWraith writes, in this order",
    unchanged: "Unchanged",
    packets: (p: { n: number }) => (p.n === 1 ? "1 packet" : `${p.n} packets`),
    /** Why the keyboard needs the extra writes, from the plan's own reasons. */
    commitNote: {
      baseKeymap:
        "The keyboard keeps changed key assignments only when a complete Fn layer follows them, so OpenWraith sends the Fn layer right after them.",
      advancedKeys:
        "The keyboard keeps advanced keys only when a complete Fn layer comes before them, so OpenWraith sends the Fn layer first.",
      ownOnly: "The keyboard keeps these on their own: nothing else has to be sent.",
    },
    warningsTitle: "Please note",
    warnings: {
      builtin: (p: { key: string; function: string }) =>
        `${p.key} no longer runs the keyboard's own function (${p.function}) on this profile.`,
      unverified: (p: { what: string }) => `Not yet verified on a W75: ${p.what}.`,
      malformed_reference_preserved: (p: { key: string }) =>
        `${p.key} holds an entry OpenWraith can't read; it is written back unchanged.`,
      dangling_reference_preserved: (p: { key: string; slot: string }) =>
        `${p.key} refers to ${p.slot}, which is empty; it is written back unchanged.`,
      transient_binding: (p: { slot: string }) =>
        `For a moment a key refers to ${p.slot} before it is written. If you stop then, OpenWraith still writes it.`,
      orphan_slot_kept: (p: { slot: string }) => `${p.slot} is kept although no key uses it.`,
      not_undoable: (p: { table: string }) =>
        `Undo can't bring back what ${p.table} holds now: OpenWraith can't reproduce it exactly.`,
    },
    persistence: {
      check: (p: { m: number }) =>
        `Check that the keyboard kept it (switches to profile ${p.m} and back; the lighting changes for a moment)`,
      forced: "OpenWraith checks this change by switching profiles and back.",
      tables: (p: { list: string }) => `Checked: ${p.list}.`,
    },
    criticalTitle: "This replaces one of the keyboard's essential shortcuts",
    /** Fn + k replaced on profile k: it only switches to this same profile (a warning, not a danger). */
    ownProfileTitle: "This replaces the keyboard's shortcut to this same profile",
    criticalNeeded: "Tick the box to confirm, then review again.",
    notUndoableTitle: "Undo couldn't bring this back",
    notUndoableConfirm: "I understand that Undo can't bring back what this slot holds now",
    notUndoableNeeded: "Tick the box to continue anyway, then review again.",
    reviewAgain: "Review again",
  },
} as const;
