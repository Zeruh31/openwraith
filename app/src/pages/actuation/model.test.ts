// The actuation page's logic: targets as the library computes them, the
// intent (only touched fields, keys that can't be changed left out), the
// summary of the apply bar, the starting values of the sliders, selection
// helpers and the value texts in both languages.

import { describe, expect, it } from "vitest";

import { KNOB_INDEX } from "../../components/keyboard";
import { DICTIONARIES } from "../../i18n";
import { makeFormat } from "../../i18n/format";
import type { DeviceSnapshot, TriggerValue } from "../../ipc/bindings";
import { FIXTURES } from "../../preview/registry";
import {
  basicActuation,
  buildIntent,
  changedKeys,
  chooseRt,
  clearDraft,
  differsFromBasic,
  EMPTY_DRAFT,
  initialActuation,
  initialSensitivity,
  initialSeparate,
  isDirty,
  keysByIndex,
  legendOf,
  limitsOf,
  nowSummary,
  rowKeys,
  ROW_IDS,
  rtIntentOf,
  sameSettingKeys,
  selectableOf,
  selectedValues,
  summarize,
  targetOf,
  valueTexts,
  type TriggerDraft,
} from "./model";
import { seedFrom } from "./preview";

const W = 46;
const A = 68;
const S = 69;
const D = 70;
const Q = 45;
const E = 47;
const ESC = 0;

function snapshotOf(id: string): DeviceSnapshot {
  const snapshot = FIXTURES[id]?.initial_snapshot;
  if (snapshot === undefined || snapshot === null) throw new Error(`no snapshot in ${id}`);
  return snapshot;
}

// trigger_ready_mixed: WASD 1.20 mm + RT 0.30, Q 0D 0.30/0.50, E 0C with
// press ≠ release (can't be changed), everything else 1.70 mm RT off.
const MIXED = snapshotOf("trigger_ready_mixed");
const KEYS = keysByIndex(MIXED);
const LIMITS = limitsOf(MIXED);

const OFF: TriggerValue = { mode: "off", mode_byte: 0, actuation: 85, rt_press: 1, rt_release: 1 };
const RT: TriggerValue = { mode: "rapid_trigger", mode_byte: 12, actuation: 60, rt_press: 15, rt_release: 15 };
const SEPARATE: TriggerValue = {
  mode: "rapid_trigger_separate",
  mode_byte: 13,
  actuation: 85,
  rt_press: 15,
  rt_release: 25,
};

const draft = (patch: Partial<TriggerDraft>): TriggerDraft => ({ ...EMPTY_DRAFT, ...patch });

describe("the fixture", () => {
  it("holds the mixed seed the tests rely on", () => {
    expect(KEYS.get(W)?.value).toMatchObject({ mode: "rapid_trigger", actuation: 60, rt_press: 15 });
    expect(KEYS.get(Q)?.value?.mode).toBe("rapid_trigger_separate");
    expect(KEYS.get(E)?.not_editable).toEqual({ reason: "press_release_differ" });
    expect(KEYS.get(KNOB_INDEX)?.selectable).toBe(false);
    expect(LIMITS).toEqual({ min_actuation: 5, max_actuation: 170, max_sensitivity: 85 });
  });
});

describe("targetOf", () => {
  it("changes only the actuation point when Rapid Trigger is kept", () => {
    expect(targetOf(RT, draft({ actuation: 50 }))).toEqual({ ...RT, actuation: 50 });
  });

  it("turns Rapid Trigger off and keeps the stored sensitivities", () => {
    expect(targetOf(RT, draft({ rt: "off" }))).toEqual({ ...RT, mode: "off", mode_byte: 0 });
  });

  it("sets one sensitivity for press and release (mode 0C)", () => {
    expect(targetOf(OFF, draft({ rt: "same", sensitivity: 20 }))).toEqual({
      mode: "rapid_trigger",
      mode_byte: 0x0c,
      actuation: 85,
      rt_press: 20,
      rt_release: 20,
    });
  });

  it("sets separate press and release (mode 0D)", () => {
    expect(targetOf(OFF, draft({ actuation: 60, rt: "separate", press: 10, release: 30 }))).toEqual({
      mode: "rapid_trigger_separate",
      mode_byte: 0x0d,
      actuation: 60,
      rt_press: 10,
      rt_release: 30,
    });
  });
});

describe("the draft", () => {
  it("is dirty only when it changes something", () => {
    expect(isDirty(EMPTY_DRAFT)).toBe(false);
    expect(isDirty(draft({ actuation: 60 }))).toBe(true);
    expect(isDirty(draft({ rt: "off" }))).toBe(true);
  });

  it("keeps the slider values when cleared", () => {
    const cleared = clearDraft(draft({ actuation: 60, rt: "separate", press: 9, release: 11 }));
    expect(isDirty(cleared)).toBe(false);
    expect(cleared).toMatchObject({ press: 9, release: 11 });
  });

  it("maps the Rapid Trigger choice to the intent", () => {
    expect(rtIntentOf(EMPTY_DRAFT)).toBeNull();
    expect(rtIntentOf(draft({ rt: "off" }))).toEqual({ mode: "off" });
    expect(rtIntentOf(draft({ rt: "same", sensitivity: 12 }))).toEqual({ mode: "same", sensitivity: 12 });
    expect(rtIntentOf(draft({ rt: "separate", press: 3, release: 4 }))).toEqual({
      mode: "separate",
      press: 3,
      release: 4,
    });
  });
});

describe("summarize and buildIntent", () => {
  it("splits the selection into changing, unchanged and left-out keys", () => {
    const selection = new Set([D, W, E, ESC, A, S]);
    const summary = summarize(selection, KEYS, draft({ actuation: 60 }));
    expect(summary.keys).toEqual([ESC, W, A, S, D]);
    expect(summary.changing).toEqual([ESC]);
    expect(summary.unchanged).toEqual([W, A, S, D]);
    expect(summary.leftOut).toEqual([E]);
  });

  it("never counts the knob", () => {
    const summary = summarize(new Set([KNOB_INDEX, W]), KEYS, draft({ actuation: 50 }));
    expect(summary.keys).toEqual([W]);
    expect(summary.leftOut).toEqual([]);
  });

  it("sends only the touched fields and leaves out keys that can't be changed", () => {
    const summary = summarize(new Set([Q, W, E]), KEYS, draft({ actuation: 70 }));
    expect(buildIntent(summary, draft({ actuation: 70 }), 7)).toEqual({
      keys: [Q, W],
      actuation: 70,
      rapid_trigger: null,
      based_on: 7,
    });
    const rtOnly = draft({ rt: "same", sensitivity: 15 });
    expect(buildIntent(summarize(new Set([ESC]), KEYS, rtOnly), rtOnly, 3)).toEqual({
      keys: [ESC],
      actuation: null,
      rapid_trigger: { mode: "same", sensitivity: 15 },
      based_on: 3,
    });
  });

  it("builds nothing without a change or without keys that can be changed", () => {
    expect(buildIntent(summarize(new Set([W]), KEYS, EMPTY_DRAFT), EMPTY_DRAFT, 1)).toBeNull();
    const d = draft({ actuation: 60 });
    expect(buildIntent(summarize(new Set([E]), KEYS, d), d, 1)).toBeNull();
    expect(buildIntent(summarize(new Set(), KEYS, d), d, 1)).toBeNull();
  });

  it("counts a key as unchanged when the draft matches it", () => {
    const same = draft({ actuation: 60, rt: "same", sensitivity: 15 });
    expect(summarize(new Set([W, A, S, D]), KEYS, same).changing).toEqual([]);
    const off = draft({ rt: "off" });
    expect(summarize(new Set([ESC]), KEYS, off).changing).toEqual([]);
  });
});

describe("starting values", () => {
  it("starts the actuation point from the common value, else the first key's", () => {
    expect(initialActuation([RT, RT], LIMITS)).toBe(60);
    expect(initialActuation([OFF, RT], LIMITS)).toBe(85);
    expect(initialActuation([], LIMITS)).toBe(85);
  });

  it("starts the sensitivity from keys with Rapid Trigger on, never from the stored 0.02 mm", () => {
    expect(initialSensitivity([RT, OFF], LIMITS)).toBe(15);
    expect(initialSensitivity([OFF, OFF], LIMITS)).toBe(15);
    expect(initialSensitivity([{ ...RT, rt_press: 10, rt_release: 10 }], LIMITS)).toBe(10);
    expect(initialSensitivity([RT, { ...RT, rt_press: 10, rt_release: 10 }], LIMITS)).toBe(15);
  });

  it("starts separate press and release from keys with Rapid Trigger on", () => {
    expect(initialSeparate([SEPARATE, OFF], LIMITS)).toEqual({ press: 15, release: 25 });
    expect(initialSeparate([OFF], LIMITS)).toEqual({ press: 15, release: 15 });
  });

  it("carries the sensitivity over when switching between the two Rapid Trigger modes", () => {
    const same = chooseRt(EMPTY_DRAFT, "same", [OFF], LIMITS);
    expect(same).toMatchObject({ rt: "same", sensitivity: 15 });
    const separate = chooseRt({ ...same, sensitivity: 9 }, "separate", [OFF], LIMITS);
    expect(separate).toMatchObject({ rt: "separate", press: 9, release: 9 });
    expect(chooseRt({ ...separate, press: 7 }, "same", [OFF], LIMITS)).toMatchObject({ sensitivity: 7 });
    expect(chooseRt(separate, "keep", [OFF], LIMITS).rt).toBe("keep");
  });
});

describe("selection helpers", () => {
  const selectable = selectableOf(KEYS);

  it("never offers the knob", () => {
    expect(selectable.has(KNOB_INDEX)).toBe(false);
    expect(selectable.size).toBe(81);
    for (const row of ROW_IDS) expect(rowKeys(row)).not.toContain(KNOB_INDEX);
  });

  it("selects rows by matrix row", () => {
    expect(rowKeys("function")).toContain(ESC);
    expect(rowKeys("top")).toEqual(expect.arrayContaining([Q, W, E]));
    expect(rowKeys("home")).toEqual(expect.arrayContaining([A, S, D]));
    expect(ROW_IDS.flatMap((row) => rowKeys(row))).toHaveLength(81);
  });

  it("adds the keys with the same setting as any selected key", () => {
    expect([...sameSettingKeys(new Set([W]), KEYS, selectable)].sort((a, b) => a - b)).toEqual([W, A, S, D]);
    const withOff = sameSettingKeys(new Set([W, ESC]), KEYS, selectable);
    // Every key but Q (separate) and E (press ≠ release).
    expect(withOff.size).toBe(81 - 2);
    expect(withOff.has(Q)).toBe(false);
    expect(withOff.has(E)).toBe(false);
  });

  it("finds the keys that differ from the basic setting", () => {
    const basic = basicActuation(LIMITS);
    expect(basic).toBe(85);
    expect([...changedKeys(KEYS, basic)].sort((a, b) => a - b)).toEqual([Q, W, E, A, S, D]);
    expect(differsFromBasic(OFF, basic)).toBe(false);
    expect(differsFromBasic({ ...OFF, actuation: 84 }, basic)).toBe(true);
    expect(differsFromBasic(null, basic)).toBe(false);
  });

  it("collects the values of selected keys that can be changed", () => {
    expect(selectedValues(new Set([E, W]), KEYS)).toHaveLength(1);
  });
});

describe("texts", () => {
  const en = valueTexts(DICTIONARIES.en, makeFormat("en"), 20);
  const tr = valueTexts(DICTIONARIES.tr, makeFormat("tr"), 20);

  it("shows the value on the cap in the chosen mode", () => {
    expect(en.cap(RT, "actuation")).toBe("1.20");
    expect(tr.cap(RT, "actuation")).toBe("1,20");
    expect(tr.cap(RT, "rapid_trigger")).toBe("0,30");
    expect(tr.cap(SEPARATE, "rapid_trigger")).toBe("↓0,30 ↑0,50");
    expect(en.cap({ ...RT, rt_release: 25 }, "rapid_trigger")).toBe("↓0.30 ↑0.50");
    expect(en.cap(OFF, "rapid_trigger")).toBe("–");
  });

  it("describes a setting", () => {
    expect(en.setting(RT)).toBe("1.20 mm · RT 0.30 mm");
    expect(tr.setting(OFF)).toBe("1,70 mm · RT kapalı");
    expect(en.setting(SEPARATE)).toBe("1.70 mm · RT 0.30 / 0.50 mm");
    expect(en.spoken(SEPARATE)).toBe("actuation 1.70 mm, Rapid Trigger 0.30 mm press, 0.50 mm release");
  });

  it("summarizes the selection's values", () => {
    expect(nowSummary([RT, RT], DICTIONARIES.en, makeFormat("en"), en)).toEqual([
      "Actuation 1.20 mm",
      "RT 0.30 mm",
    ]);
    expect(nowSummary([RT, OFF, OFF], DICTIONARIES.en, makeFormat("en"), en)).toEqual([
      "Actuation 1.20–1.70 mm (mixed)",
      "RT 0.30 mm on 1 key and RT off on 2 keys",
    ]);
    expect(nowSummary([RT, OFF, OFF], DICTIONARIES.tr, makeFormat("tr"), tr)[1]).toBe(
      "RT 0,30 mm (1 tuş) ve RT kapalı (2 tuş)",
    );
  });

  it("prints short word legends in both languages", () => {
    expect(legendOf(DICTIONARIES.en, "L-Shift")).toBe("SHIFT");
    expect(legendOf(DICTIONARIES.tr, "Up")).toBe("↑");
    expect(legendOf(DICTIONARIES.en, "W")).toBe("W");
  });
});

describe("preview seed", () => {
  it("is empty outside the preview", () => {
    const seed = seedFrom(undefined);
    expect(seed.selection.size).toBe(0);
    expect(isDirty(seed.draft)).toBe(false);
    expect(seed.open).toBe(false);
  });

  it("presets keys and a draft", () => {
    const seed = seedFrom({ keys: [W], actuation: 60, rt: { mode: "separate", press: 10, release: 20 }, open: true });
    expect([...seed.selection]).toEqual([W]);
    expect(seed.draft).toMatchObject({ actuation: 60, rt: "separate", press: 10, release: 20 });
    expect(seed.open).toBe(true);
  });
});
