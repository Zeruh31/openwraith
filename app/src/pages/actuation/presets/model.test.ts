// Trigger presets on the recorded list (presets_ready: the library's five
// built-in presets, a valid user preset and one refused at 5.00 mm).

import { describe, expect, it } from "vitest";

import { DICTIONARIES } from "../../../i18n";
import { makeFormat } from "../../../i18n/format";
import type { DeviceSnapshot, PresetsView } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import { EMPTY_DRAFT, keysByIndex, valueTexts } from "../model";
import { cleanName, presetDraft, presetItems, presetSetting, targetKeys, valuesToSave } from "./model";

const W = 46;
const A = 68;
const S = 69;
const D = 70;
const Q = 45;

function presets(): PresetsView {
  const call = FIXTURES["presets_ready"]?.calls.find((c) => c.command === "list_presets");
  if (call?.command !== "list_presets" || call.ok === null) throw new Error("no presets");
  return structuredClone(call.ok);
}

function snapshot(): DeviceSnapshot {
  const found = FIXTURES["presets_ready"]?.initial_snapshot;
  if (found === null || found === undefined) throw new Error("no snapshot");
  return found;
}

const texts = valueTexts(DICTIONARIES.en, makeFormat("en"), 20);

describe("presets", () => {
  it("list the built-in ones first, then the user's, with their values", () => {
    const items = presetItems(presets());
    expect(items.map((item) => item.key)).toEqual([
      "builtin:typing",
      "builtin:balanced",
      "builtin:fast",
      "builtin:competitive",
      "builtin:factory",
      "user:u-1",
      "user:u-2",
    ]);
    const text = (key: string) => {
      const item = items.find((candidate) => candidate.key === key);
      if (item === undefined) throw new Error(key);
      return presetSetting(item.actuation, item.rt, texts);
    };
    expect(text("builtin:typing")).toBe("2.00 mm · RT off");
    expect(text("builtin:balanced")).toBe("1.20 mm · RT 0.30 mm");
    expect(text("builtin:fast")).toBe("0.80 mm · RT 0.16 mm");
    expect(text("builtin:competitive")).toBe("0.40 mm · RT 0.10 mm");
    expect(text("builtin:factory")).toBe("1.70 mm · RT off");
    expect(items[6]?.user?.refused).toMatchObject({ problem: "out_of_range", field: "actuation", value: 250 });
  });

  it("fill the trigger draft", () => {
    expect(presetDraft(40, { mode: "same", sensitivity: 8 })).toEqual({ ...EMPTY_DRAFT, actuation: 40, rt: "same", sensitivity: 8 });
    expect(presetDraft(85, { mode: "off_with", press: 1, release: 1 })).toEqual({ ...EMPTY_DRAFT, actuation: 85, rt: "off" });
    expect(presetDraft(30, { mode: "separate", press: 5, release: 9 })).toMatchObject({ rt: "separate", press: 5, release: 9 });
  });

  it("go to every key that can be changed, WASD or the selection", () => {
    const keys = keysByIndex(snapshot());
    expect(targetKeys("wasd", keys, new Set())).toEqual([W, A, S, D]);
    expect(targetKeys("selection", keys, new Set([D, Q, 15]))).toEqual([Q, D]);
    expect(targetKeys("all", keys, new Set())).toHaveLength(81);
  });
});

describe("saving a preset", () => {
  it("saves the draft when it sets both values, else the setting the selected keys share", () => {
    const keys = keysByIndex(snapshot());
    expect(valuesToSave({ ...EMPTY_DRAFT, actuation: 60, rt: "same", sensitivity: 12 }, new Set(), keys)).toEqual({
      actuation: 60,
      rapid_trigger: { mode: "same", sensitivity: 12 },
    });
    // Only the actuation point drafted: the keys' own setting decides, and A and S share theirs.
    expect(valuesToSave({ ...EMPTY_DRAFT, actuation: 60 }, new Set([A, S]), keys)).toEqual({
      actuation: 85,
      rapid_trigger: { mode: "off" },
    });
    // W (1.00 mm, Rapid Trigger) and A (1.70 mm) differ: nothing to save.
    expect(valuesToSave(EMPTY_DRAFT, new Set([W, A]), keys)).toBeNull();
    expect(valuesToSave(EMPTY_DRAFT, new Set([W]), keys)).toEqual({
      actuation: 50,
      rapid_trigger: { mode: "same", sensitivity: 15 },
    });
    expect(valuesToSave(EMPTY_DRAFT, new Set(), keys)).toBeNull();
  });

  it("cleans the name as the backend does", () => {
    expect(cleanName("  Valorant\u0007 ")).toBe("Valorant");
    expect(cleanName("   ")).toBe("");
  });
});
