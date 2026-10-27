// The Lighting page (owned by the lighting page agent).

import type { LightDisabled, LightModeKey } from "../../ipc/bindings";
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
    customNote:
      "Per-key colours can't be edited in this version; the keyboard shows its stored per-key colours.",
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
} as const;
