// The Lighting page (owned by the lighting page agent).

import type { LightDisabled, LightModeKey } from "../../ipc/bindings";
import type { ColorHue, ColorTool, GradientDirection } from "../../pages/lighting/colors/model";
import type { QuickColor } from "../../pages/lighting/model";

export const lighting = {
  title: "Lighting",
  description: "Effect, colour, brightness and speed of the main light.",
  modes: {
    static: "Static",
    breath: "Breathing",
    wave: "Wave",
    neon: "Neon",
    radar: "Radar",
    reactive: "Reactive",
    aurora: "Aurora",
    ripple: "Ripple",
    twinkle: "Twinkle",
    custom: "Custom",
    cross: "Cross",
    speed_respond: "Speed response",
    auto_ripple: "Auto ripple",
    striation: "Stripes",
    fireworks: "Fireworks",
    music_rhythm: "Music rhythm",
    unknown: "Unknown effect",
  } satisfies Record<LightModeKey, string>,
  readAt: (p: { time: string }) => `Read at ${p.time}`,
  level: (p: { value: number; max: number }) => `${p.value} of ${p.max}`,
  perProfile: {
    title: "Lighting is saved per profile",
    body: (p: { n: number }) =>
      `You're changing profile ${p.n}. Each profile keeps its own lighting; switching profiles (in the header or with Fn + 1–4) switches it too.`,
  },
  empty: {
    title: "Connect the keyboard to change its lighting",
    loading: "Reading the keyboard's lighting…",
    goToKeyboard: "Go to the Keyboard page",
  },
  disabled: {
    title: "The lighting can't be changed",
    reasons: {
      not_read: "The lighting wasn't read from the keyboard. Read the keyboard again.",
      invalid_record: "The keyboard's lighting settings couldn't be decoded, so OpenWraith leaves them alone.",
      limits_unknown:
        "The keyboard didn't report which effects and values it accepts, so OpenWraith doesn't change the lighting.",
      unsupported_model: "OpenWraith can't change the lighting of this keyboard model.",
    } satisfies Record<LightDisabled, string>,
  },
  powerOff: {
    title: "The keyboard reports its lighting as switched off",
    body: "Changes are saved, but you'll see them only after switching the lighting on (Fn + Backspace on the keyboard).",
  },
  effects: {
    title: "Effect",
    label: "Effect",
    onKeyboard: "On the keyboard",
    unknown: (p: { id: string }) => `Unknown effect (${p.id})`,
    unknownHint: "Choose an effect to change the lighting.",
  },
  controls: {
    title: "Settings",
  },
  color: {
    label: "Colour",
    customNote: "This effect shows each key's own colour.",
    editKeyColors: "Change key colours",
    fullColorNote: "This effect uses full colour (rainbow), so the colour you choose may not show.",
    names: {
      red: "Red",
      orange: "Orange",
      yellow: "Yellow",
      green: "Green",
      cyan: "Cyan",
      blue: "Blue",
      purple: "Purple",
      white: "White",
    } satisfies Record<QuickColor, string>,
  },
  brightness: {
    label: "Brightness",
    zeroHint: "0 = darkest; the keys may look off.",
  },
  speed: {
    label: "Speed",
    hint: "Whether 0 is the slowest or the fastest isn't known yet.",
    notUsed: "This effect has no speed setting.",
  },
  preview: {
    title: "Preview",
    now: "On the keyboard now",
    after: "After writing",
    describe: (p: { effect: string; color: string; brightness: string }) =>
      `${p.effect}, colour ${p.color}, brightness ${p.brightness}`,
    describeCustom: (p: { effect: string; brightness: string }) =>
      `${p.effect}, stored per-key colours, brightness ${p.brightness}`,
    note: "A rough still picture: the keyboard's animation isn't shown.",
  },
  applyBar: {
    notWritten: "Not written yet",
    noChanges: "No unwritten changes.",
    effect: (p: { from: string; to: string }) => `Effect: ${p.from} → ${p.to}`,
    color: "Colour",
    brightness: (p: { from: number; to: number }) => `Brightness: ${p.from} → ${p.to}`,
    speed: (p: { from: number; to: number }) => `Speed: ${p.from} → ${p.to}`,
    notReady: "Connect the keyboard and wait until it has been read.",
    busy: "Wait until the current operation has finished.",
    notEditable: "The lighting of this keyboard can't be changed.",
    chooseEffect: "Choose an effect first.",
  },
  review: {
    title: (p: { n: number }) => `Change the lighting of profile ${p.n}?`,
    intro: (p: { n: number }) =>
      `This changes the lighting of profile ${p.n} only; the other profiles keep their own.`,
    setting: "Setting",
    now: "Now",
    new: "New",
    changes: "(changes)",
    perKey: "Per-key colours",
    notUsed: "Not used by this effect",
    modeChange: "The effect's direction is reset to the new effect's default.",
    packets: (p: { n: number }) => (p.n === 1 ? "1 packet" : `${p.n} packets`),
    packetsModeChange: "2 packets: leaving music mode first (as the vendor site does), then the light record.",
  },
  discarded: {
    title: "Unwritten lighting changes were discarded",
    body: "The keyboard is on another profile or was connected again, and each profile has its own lighting.",
  },
  /** Lighting > Key colours. */
  colors: {
    description: "The colour of each key. The keyboard shows them with the Custom effect.",
    empty: {
      title: "Connect the keyboard to change its key colours",
      loading: "Reading the keyboard's key colours…",
      failed: "The key colours couldn't be loaded. Read the keyboard again.",
    },
    disabled: {
      title: "The key colours can't be changed",
      not_read: "The key colours weren't read from the keyboard. Read the keyboard again.",
      unreadable: "The keyboard's key colours couldn't be decoded, so OpenWraith leaves them alone.",
      unsupported_model: "OpenWraith can't change the key colours of this keyboard model.",
    },
    perProfile: {
      title: "Key colours are saved per profile",
      body: (p: { n: number }) =>
        `You're changing profile ${p.n}. Each profile keeps its own key colours; switching profiles switches them too.`,
    },
    notShown: {
      title: "The keyboard shows these colours only with the Custom effect",
      body: (p: { effect: string }) =>
        `The effect now is ${p.effect}. Colours you write are saved, but you'll see them only once the effect is Custom.`,
      bodyUnknown: "Colours you write are saved, but you'll see them only once the effect is Custom.",
      switch: "Also switch the effect to Custom when writing",
      switchHint: "Only the effect changes; the brightness stays as it is.",
      cannotSwitch: "OpenWraith can't switch this keyboard's effect to Custom.",
      goToEffect: "Go to the Effect tab",
    },
    board: {
      title: "Keys",
      label: "Keyboard: key colours",
      knob: "The knob's light isn't changed here",
      changed: (p: { now: string; next: string }) => `Now ${p.now}; after writing ${p.next} (not written yet)`,
      selected: (p: { n: number }) =>
        p.n === 0 ? "No keys selected" : p.n === 1 ? "1 key selected" : `${p.n} keys selected`,
      selectedColor: (p: { color: string }) => `Colour: ${p.color}`,
      mixed: "Several colours",
      selectAll: "Select all",
      clearSelection: "Clear selection",
    },
    tools: {
      label: "Tool",
      names: {
        select: "Select",
        brush: "Brush",
        eyedropper: "Eyedropper",
      } satisfies Record<ColorTool, string>,
      hints: {
        select:
          "Click keys to select them, drag across keys to select several, or move with the arrow keys and press Space. Then fill the selection.",
        brush: "Click a key, or move to it with the arrow keys and press Space, to paint it with the chosen colour.",
        eyedropper: "Click a key, or move to it with the arrow keys and press Space, to take its colour.",
      } satisfies Record<ColorTool, string>,
      undo: "Undo",
      redo: "Redo",
      undoHint: "Ctrl + Z undoes and Ctrl + Y redoes unwritten changes.",
      history: "Unwritten changes",
    },
    paint: {
      title: "Colour",
      picker: "Colour to paint with",
      off: "Black (off)",
      fillSelection: "Fill selection",
      fillAll: "Fill all keys",
      needSelection: "Select keys first.",
    },
    gradient: {
      title: "Gradient",
      to: "Gradient to",
      direction: "Direction",
      directions: {
        horizontal: "Left to right",
        vertical: "Top to bottom",
      } satisfies Record<GradientDirection, string>,
      hint: "From the colour above (left or top) to this one (right or bottom).",
      fill: "Fill selection with gradient",
      needTwo: "Select at least two keys.",
    },
    kept: {
      title: "Other stored colours",
      body: "The keyboard stores these colours outside the keys shown here. OpenWraith keeps them as they are.",
      knob: (p: { color: string }) => `Knob: ${p.color}`,
      slot: (p: { slot: number; color: string }) => `Slot ${p.slot}: ${p.color}`,
    },
    announce: {
      painted: (p: { key: string; color: string }) => `${p.key}: ${p.color}, not written yet`,
      unchanged: (p: { key: string }) => `${p.key} already has this colour`,
      filled: (p: { n: number; color: string }) =>
        p.n === 1 ? `1 key: ${p.color}, not written yet` : `${p.n} keys: ${p.color}, not written yet`,
      gradient: (p: { n: number; from: string; to: string }) =>
        `${p.n} keys: gradient from ${p.from} to ${p.to}, not written yet`,
      nothingChanged: "Nothing changed: the keys already have this colour.",
      picked: (p: { key: string; color: string }) => `Took ${p.color} from ${p.key}`,
      undone: (p: { n: number }) =>
        p.n === 0
          ? "Undone: no unwritten changes left"
          : p.n === 1
            ? "Undone: 1 key changes colour"
            : `Undone: ${p.n} keys change colour`,
      redone: (p: { n: number }) =>
        p.n === 0
          ? "Redone: no unwritten changes"
          : p.n === 1
            ? "Redone: 1 key changes colour"
            : `Redone: ${p.n} keys change colour`,
    },
    applyBar: {
      changes: (p: { n: number }) => (p.n === 1 ? "1 key changes colour" : `${p.n} keys change colour`),
      switches: "the effect switches to Custom",
      notEditable: "The key colours of this keyboard can't be changed.",
    },
    review: {
      title: (p: { n: number }) => `Change the key colours of profile ${p.n}?`,
      nothingTitle: "Nothing to change",
      nothing: "The keyboard already has these colours.",
      intro: (p: { n: number }) =>
        `This changes the key colours of profile ${p.n} only; the other profiles keep their own.`,
      before: "Now",
      after: "After writing",
      picture: (p: { caption: string; summary: string }) => `${p.caption}: ${p.summary}`,
      colorCount: (p: { color: string; n: number }) =>
        p.n === 1 ? `${p.color} on 1 key` : `${p.color} on ${p.n} keys`,
      moreColors: (p: { n: number }) => (p.n === 1 ? "and 1 more colour" : `and ${p.n} more colours`),
      changesTitle: "What changes",
      keys: "Keys",
      now: "Now",
      new: "New",
      keysMore: (p: { list: string; n: number }) => `${p.list} and ${p.n} more`,
      switchesToCustom: (p: { effect: string }) =>
        `The effect also changes from ${p.effect} to Custom, so the keyboard shows these colours. The brightness stays as it is.`,
      switchesToCustomUnknown: "The effect also changes to Custom, so the keyboard shows these colours.",
      notShownBody: "The colours are saved, but you'll see them only once the effect is Custom.",
      switchOption: "Also switch the effect to Custom",
      persistence: {
        check: (p: { m: number }) =>
          `Check that the keyboard kept it (switches to profile ${p.m} and back; the lighting changes for a moment)`,
        forced: "OpenWraith will check that the keyboard kept it (it switches to another profile and back).",
      },
      unverified: "Part of this change hasn't been verified on a W75 yet.",
      notUndoable: (p: { table: string }) => `Undo can't bring back ${p.table} exactly as it is now.`,
      otherWarning: "OpenWraith's library added a note to this plan (see the technical details).",
      technical: {
        write: (p: { table: string; packets: number; summary: string }) =>
          `${p.table}: ${p.packets === 1 ? "1 packet" : `${p.packets} packets`} (${p.summary})`,
        total: (p: { n: number }) => (p.n === 1 ? "1 packet in total" : `${p.n} packets in total`),
        activeTable: (p: { n: number }) => `The keyboard reports per-key colour table ${p.n} as active.`,
      },
    },
    /** Why a colour edit was refused (`ErrorDetail` "edit"); nothing was written. */
    problems: {
      no_changes: "Nothing to change: no key colour differs from the keyboard's.",
      too_many_keys: (p: { max: number }) => `Too many keys at once (at most ${p.max}).`,
      duplicate_key: (p: { key: string }) => `${p.key} was given twice.`,
      not_a_key: (p: { index: number }) => `Slot ${p.index} isn't a key of this keyboard.`,
      knob: "The knob's light can't be changed here.",
      invalid_color: "A colour isn't valid (colours are written as #RRGGBB).",
      out_of_range: (p: { value: number; min: number; max: number }) =>
        `A value is out of range: ${p.value} (allowed ${p.min} to ${p.max}).`,
      unsupported_current_table: (p: { table: string }) =>
        `OpenWraith can't reproduce ${p.table} exactly as the keyboard stores it, so it doesn't change it.`,
      not_available:
        "The key colours can't be changed now: the colour table wasn't read or this keyboard model isn't supported. Read the keyboard again.",
      other: "The change was refused. The technical details say why.",
    },
    discarded: {
      title: "Unwritten key colours were discarded",
      body: "The keyboard is on another profile or was connected again, and each profile has its own key colours.",
    },
    /** Approximate colour names for screen readers (the hex value is always said too). */
    names: {
      black: "black (off)",
      white: "white",
      grey: "grey",
      red: "red",
      orange: "orange",
      brown: "brown",
      yellow: "yellow",
      lime: "lime",
      green: "green",
      cyan: "cyan",
      blue: "blue",
      purple: "purple",
      magenta: "magenta",
      pink: "pink",
    } satisfies Record<ColorHue | "black" | "white" | "grey", string>,
    shades: {
      dark: (p: { name: string }) => `dark ${p.name}`,
      light: (p: { name: string }) => `light ${p.name}`,
    },
    /** A colour as screen readers hear it ("red, #FF0000"). */
    describe: (p: { name: string; hex: string }) => `${p.name}, ${p.hex}`,
    /** Key cap legends by the library's key name (the caps are printed in English). */
    legends: {
      Esc: "ESC",
      Del: "DEL",
      Knob: "KNOB",
      Backspace: "⌫",
      Home: "HOME",
      Tab: "TAB",
      PgUp: "PGUP",
      "Caps Lock": "CAPS",
      Enter: "ENTER",
      PgDn: "PGDN",
      "L-Shift": "SHIFT",
      "R-Shift": "SHIFT",
      Up: "↑",
      "L-Ctrl": "CTRL",
      "L-Win": "WIN",
      "L-Alt": "ALT",
      Space: "SPACE",
      "R-Alt": "ALT",
      Fn: "FN",
      "R-Ctrl": "CTRL",
      Left: "←",
      Down: "↓",
      Right: "→",
    },
  },
} as const;
