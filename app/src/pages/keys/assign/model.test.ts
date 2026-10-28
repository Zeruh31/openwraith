// Keys > Assignments: the draft model and the key caps, on the keys state the
// core recorded from the simulator (keys_ready: base Caps → Left Ctrl,
// ` → Mute, E → DKS, G → Tap or hold, H → Toggle, Home kept as read,
// J → empty Macro 6; Fn W → Up, Fn Q → Macro 3, Fn F1 → Left Ctrl + C).

import { describe, expect, it } from "vitest";

import { DICTIONARIES } from "../../../i18n";
import type { KeymapEditIntent, KeysState } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import { actionCap, brokenKeys, keyCap, notAssignableText } from "./caps";
import {
  canSwap,
  changeCount,
  confirmedAcks,
  draftChanges,
  dropTarget,
  effectiveValue,
  EMPTY_DRAFT,
  normalizeDraft,
  setConfirmed,
  setTarget,
  slotUsers,
  swapKeys,
  withConfirmed,
  type AssignDraft,
} from "./model";
import { seedFrom } from "./preview";

const en = DICTIONARIES.en;

const R = 48;
const F2 = 3;
const CAPS = 66;
const BACKTICK = 22;
const HOME = 37;
const J = 74;
const Q = 45;
const W = 46;
const TWO = 24;
const FN = 121;

function keysState(id: string, nth = 0): KeysState {
  const calls = FIXTURES[id]?.calls.filter((c) => c.command === "get_keys_state") ?? [];
  const call = calls[nth];
  if (call?.command !== "get_keys_state" || call.ok === null) throw new Error(`no keys state in ${id}`);
  return structuredClone(call.ok);
}

function recordedIntent(id: string): KeymapEditIntent {
  const call = FIXTURES[id]?.calls.find((c) => c.command === "prepare_keymap_edit");
  if (call?.command !== "prepare_keymap_edit") throw new Error(`no prepare in ${id}`);
  return call.args.intent;
}

const state = keysState("keys_ready");

describe("the draft", () => {
  it("drops a key set back to what the keyboard has", () => {
    let draft = setTarget(state, EMPTY_DRAFT, "base", R, { kind: "key", usage: 0x05 });
    expect(changeCount(draft)).toBe(1);
    draft = setTarget(state, draft, "base", R, { kind: "default" });
    expect(changeCount(draft)).toBe(0);
    // Caps is Left Ctrl now: choosing Left Ctrl is no change.
    expect(setTarget(state, EMPTY_DRAFT, "base", CAPS, { kind: "key", usage: 0xe0 })).toEqual(EMPTY_DRAFT);
  });

  it("keeps the order in which keys were first changed, replacing a key's target in place", () => {
    let draft = setTarget(state, EMPTY_DRAFT, "base", R, { kind: "key", usage: 0x05 });
    draft = setTarget(state, draft, "fn", F2, { kind: "mouse", action: "back" });
    draft = setTarget(state, draft, "base", R, { kind: "key", usage: 0x06 });
    expect(draft.targets.map((t) => [t.layer, t.key])).toEqual([
      ["base", R],
      ["fn", F2],
    ]);
    expect(effectiveValue(state, draft, "base", R)).toEqual({ kind: "key", usage: 0x06 });
  });

  it("writes the recorded plan's changes: two assignments and a swap of both keys' current entries", () => {
    let draft = setTarget(state, EMPTY_DRAFT, "base", R, { kind: "combo", modifiers: [0xe0], key: 0x06 });
    draft = setTarget(state, draft, "fn", F2, { kind: "mouse", action: "back" });
    draft = swapKeys(state, draft, "base", CAPS, BACKTICK);
    expect(effectiveValue(state, draft, "base", CAPS)).toEqual({ kind: "media", index: 0x12 });
    expect(effectiveValue(state, draft, "base", BACKTICK)).toEqual({ kind: "key", usage: 0xe0 });
    expect(changeCount(draft)).toBe(4);
    expect(draftChanges(state, draft)).toEqual(recordedIntent("keys_preview_assign").changes);
  });

  it("writes a swap after a key's own change as two assignments, and swapping back as nothing", () => {
    let draft = setTarget(state, EMPTY_DRAFT, "base", CAPS, { kind: "key", usage: 0x04 });
    draft = swapKeys(state, draft, "base", CAPS, BACKTICK);
    expect(draftChanges(state, draft)).toEqual([
      { change: "assign", layer: "base", key: CAPS, action: { kind: "media", index: 0x12 } },
      { change: "assign", layer: "base", key: BACKTICK, action: { kind: "key", usage: 0x04 } },
    ]);
    const swapped = swapKeys(state, EMPTY_DRAFT, "base", CAPS, BACKTICK);
    expect(swapKeys(state, swapped, "base", CAPS, BACKTICK)).toMatchObject({ targets: [], swaps: [] });
  });

  it("writes the default as a reset and leaves a pair once one of its keys changes", () => {
    let draft = swapKeys(state, EMPTY_DRAFT, "base", CAPS, BACKTICK);
    draft = setTarget(state, draft, "base", CAPS, { kind: "default" });
    expect(draft.swaps).toEqual([]);
    expect(draftChanges(state, draft)).toEqual([
      { change: "reset", layer: "base", key: CAPS },
      { change: "assign", layer: "base", key: BACKTICK, action: { kind: "key", usage: 0xe0 } },
    ]);
    expect(changeCount(dropTarget(draft, "base", CAPS))).toBe(1);
  });

  it("can't move an entry kept as read, nor swap keys that already do the same", () => {
    expect(canSwap(state, EMPTY_DRAFT, "base", HOME, R)).toBe(false);
    expect(swapKeys(state, EMPTY_DRAFT, "base", HOME, R)).toBe(EMPTY_DRAFT);
    expect(canSwap(state, EMPTY_DRAFT, "base", R, 49)).toBe(false);
    expect(canSwap(state, EMPTY_DRAFT, "base", R, R)).toBe(false);
    // Replacing (not moving) a kept entry is allowed.
    expect(changeCount(setTarget(state, EMPTY_DRAFT, "base", HOME, { kind: "default" }))).toBe(1);
  });

  it("knows which keys use a slot after the draft", () => {
    expect(slotUsers(state, EMPTY_DRAFT, "macro", 2)).toEqual([{ layer: "fn", key: Q }]);
    const moved = setTarget(state, EMPTY_DRAFT, "fn", Q, { kind: "default" });
    expect(slotUsers(state, moved, "macro", 2)).toEqual([]);
    const bound = setTarget(state, moved, "base", W, { kind: "macro", slot: 2 });
    expect(slotUsers(state, bound, "macro", 2)).toEqual([{ layer: "base", key: W }]);
  });

  it("keeps the critical confirmations as acknowledgements", () => {
    let draft = setConfirmed(EMPTY_DRAFT, TWO, true);
    expect(confirmedAcks(draft)).toEqual([{ key: TWO, critical: true }]);
    draft = withConfirmed(draft, [{ key: 116, critical: true }, { key: 3, critical: false }]);
    expect(draft.confirmed).toEqual([TWO, 116]);
    expect(setConfirmed(draft, TWO, false).confirmed).toEqual([116]);
  });

  it("drops what the keyboard already has after a new read, and returns the same draft otherwise", () => {
    const draft = setTarget(state, EMPTY_DRAFT, "base", R, { kind: "key", usage: 0x05 });
    expect(normalizeDraft(state, draft)).toBe(draft);
    const after = structuredClone(state);
    const r = after.keys.find((key) => key.key.index === R);
    if (r === undefined) throw new Error("no R");
    r.base = { ...r.base, value: { kind: "key", usage: 0x05 }, id: "key:05", origin: { origin: "remapped" } };
    expect(normalizeDraft(after, draft).targets).toEqual([]);
  });

  it("forgets a pair whose keys are no longer both in the draft", () => {
    const draft: AssignDraft = {
      ...EMPTY_DRAFT,
      targets: [{ layer: "base", key: CAPS, action: { kind: "media", index: 0x12 } }],
      swaps: [{ layer: "base", first: CAPS, second: BACKTICK }],
    };
    expect(normalizeDraft(state, draft).swaps).toEqual([]);
  });
});

describe("the key caps", () => {
  const ctx = (draft: AssignDraft, layer: "base" | "fn") => ({
    state,
    draft,
    layer,
    labels: { state },
    dict: en,
  });
  const broken = brokenKeys(state);

  it("show a changed key's action in the accent colour with its own legend below", () => {
    const caps = keyCap(ctx(EMPTY_DRAFT, "base"), CAPS, broken);
    expect(caps).toMatchObject({ legend: "CTRL", sublegend: "CAPS", flag: true, name: "Caps Lock", marker: null });
    expect(caps.description).toBe(en.keysAssign.keyInfo.now({ what: "Left Ctrl" }));
    expect(keyCap(ctx(EMPTY_DRAFT, "fn"), 2, broken)).toMatchObject({ legend: "CTRL+C", sublegend: "F1", flag: true });
  });

  it("show the keyboard's own Fn function as the muted second line, in a short label", () => {
    const f2 = keyCap(ctx(EMPTY_DRAFT, "fn"), F2, broken);
    expect(f2).toMatchObject({ legend: "F2", sublegend: "Vol −", flag: false });
    // The full name stays in the description.
    expect(f2.description).toContain("Volume down");
    // Every built-in has a short label in both languages, and no two keys read the same.
    for (const dict of [en, DICTIONARIES.tr]) {
      const caps: Readonly<Record<string, string>> = dict.keys.builtinCaps;
      const names: Readonly<Record<string, string>> = dict.keys.builtins;
      expect(Object.keys(caps).sort()).toEqual(Object.keys(names).sort());
      expect(new Set(Object.values(caps)).size).toBe(Object.values(caps).length);
      for (const label of Object.values(caps)) expect(label.length).toBeLessThanOrEqual(12);
    }
    expect(keyCap(ctx(EMPTY_DRAFT, "base"), F2, broken)).toMatchObject({ legend: "F2", sublegend: null, flag: false });
  });

  it("mark entries kept as read, broken references and drafted changes", () => {
    expect(keyCap(ctx(EMPTY_DRAFT, "base"), HOME, broken)).toMatchObject({
      legend: "HOME",
      sublegend: "Lighting function",
      flag: true,
    });
    const j = keyCap(ctx(EMPTY_DRAFT, "base"), J, broken);
    expect(j).toMatchObject({ legend: "M6", marker: "warning" });
    expect(j.description).toContain(en.keysAssign.keyInfo.broken);
    const draft = setTarget(state, EMPTY_DRAFT, "base", J, { kind: "default" });
    const fixed = keyCap(ctx(draft, "base"), J, broken);
    expect(fixed).toMatchObject({ legend: "J", marker: "pending", flag: false });
    expect(fixed.description).toContain(en.keysAssign.keyInfo.draft({ next: "Default" }));
  });

  it("make the Fn key unselectable with the reason, and name advanced keys by type", () => {
    expect(keyCap(ctx(EMPTY_DRAFT, "base"), FN, broken)).toMatchObject({
      selectable: false,
      tooltip: en.keys.fnLocked,
    });
    const fn = state.keys.find((key) => key.key.index === FN);
    if (fn === undefined) throw new Error("no Fn");
    expect(notAssignableText(fn, "fn", en)).toBe(en.keys.fnLocked);
    expect(keyCap(ctx(EMPTY_DRAFT, "base"), 47, broken).legend).toBe("DKS");
    expect(actionCap({ kind: "advanced", slot: 1 }, state, en)).toBe("MT");
    expect(actionCap({ kind: "advanced", slot: 5 }, state, en)).toBe("A6");
    expect(actionCap({ kind: "mouse", action: "back" }, state, en)).toBe("Back button (X1)");
    expect(actionCap({ kind: "key", usage: 0x28 }, state, DICTIONARIES.tr)).toBe("ENTER");
  });
});

describe("the preview seed", () => {
  it("presets nothing outside a scenario, and a recorded plan's draft inside one", () => {
    expect(seedFrom(undefined)).toEqual({ layer: undefined, selected: undefined, draft: undefined, open: false });
    const seed = seedFrom({
      layer: "fn",
      key: F2,
      targets: [
        { layer: "base", key: CAPS, action: { kind: "media", index: 0x12 } },
        { layer: "base", key: BACKTICK, action: { kind: "key", usage: 0xe0 } },
      ],
      swaps: [{ layer: "base", first: CAPS, second: BACKTICK }],
      open: true,
    });
    expect(seed.open).toBe(true);
    expect(seed.selected).toBe(F2);
    expect(draftChanges(state, seed.draft ?? EMPTY_DRAFT)).toEqual([
      { change: "swap", layer: "base", first: CAPS, second: BACKTICK },
    ]);
  });
});
