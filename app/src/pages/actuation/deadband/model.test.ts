// The dead band tab's logic on the recorded simulator state (dead_band_ready)
// and the trigger settings of the same read.

import { describe, expect, it } from "vitest";

import type { DeadBandState, DeviceSnapshot } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import { keysByIndex } from "../model";
import {
  buildDeadBandIntent,
  deadBandKeys,
  EMPTY_DEAD_BAND,
  initialValue,
  isDeadBandDirty,
  localHints,
  sameDeadBandKeys,
  selectableOf,
  summarize,
  targetOf,
  travelUnits,
  zonedKeys,
} from "./model";

const W = 46;
const A = 68;
const S = 69;
const D = 70;
const WASD = new Set([W, A, S, D]);

function state(): DeadBandState {
  const call = FIXTURES["dead_band_ready"]?.calls.find((c) => c.command === "get_dead_band_state");
  if (call?.command !== "get_dead_band_state" || call.ok === null) throw new Error("no dead band state");
  return structuredClone(call.ok);
}

function snapshot(): DeviceSnapshot {
  const found = FIXTURES["dead_band_ready"]?.initial_snapshot;
  if (found === null || found === undefined) throw new Error("no snapshot");
  return found;
}

describe("the dead band draft", () => {
  it("changes nothing until a value is set", () => {
    expect(isDeadBandDirty(EMPTY_DEAD_BAND)).toBe(false);
    expect(isDeadBandDirty({ top: 0, bottom: null })).toBe(true);
    expect(targetOf({ top: 3, bottom: 4 }, { top: 5, bottom: null })).toEqual({ top: 5, bottom: 4 });
  });

  it("splits the selection into changing and unchanged keys, never the knob", () => {
    const base = state();
    const keyed = base.keys.map((key) => (key.key.index === S ? { ...key, value: { top: 5, bottom: 5 } } : key));
    const keys = deadBandKeys({ ...base, keys: keyed });
    const summary = summarize(new Set([...WASD, 15]), keys, { top: 5, bottom: 5 });
    expect(summary).toEqual({ keys: [W, A, S, D], changing: [W, A, D], unchanged: [S] });
    expect(selectableOf(keys).has(15)).toBe(false);
    expect(zonedKeys(keys)).toEqual(new Set([S]));
    expect(sameDeadBandKeys(new Set([S]), keys, selectableOf(keys))).toEqual(new Set([S]));
  });

  it("builds the intent with only the changed values, the persistence option and the state's revision", () => {
    const keys = deadBandKeys(state());
    const summary = summarize(WASD, keys, { top: 5, bottom: 5 });
    // The recorded intent of dead_band_apply_ok.
    expect(buildDeadBandIntent(summary, { top: 5, bottom: 5 }, 1, false)).toEqual({
      keys: [W, A, S, D],
      top: 5,
      bottom: 5,
      check_persistence: false,
      based_on: 1,
    });
    expect(buildDeadBandIntent(summary, { top: null, bottom: 3 }, 7, true)).toMatchObject({
      top: null,
      bottom: 3,
      check_persistence: true,
      based_on: 7,
    });
    expect(buildDeadBandIntent(summary, EMPTY_DEAD_BAND, 1, false)).toBeNull();
    expect(buildDeadBandIntent(summarize(new Set(), keys, { top: 5, bottom: null }), { top: 5, bottom: null }, 1, false)).toBeNull();
  });

  it("starts a slider from the first selected key's value, within the range", () => {
    expect(initialValue([{ top: 7, bottom: 2 }], "top", 50)).toBe(7);
    expect(initialValue([], "bottom", 50)).toBe(0);
    expect(initialValue([{ top: 80, bottom: 0 }], "top", 50)).toBe(50);
  });
});

describe("soft warnings", () => {
  it("warn where a dead zone reaches the actuation point, as the backend's hints do", () => {
    const keys = deadBandKeys(state());
    const triggers = keysByIndex(snapshot());
    const travel = travelUnits(20);
    expect(travel).toBe(170);
    // dead_band_preview: top 50, bottom 10 on WASD; W actuates at 1.00 mm (50).
    const draft = { top: 50, bottom: 10 };
    const hints = localHints(summarize(WASD, keys, draft), keys, triggers, draft, travel);
    expect(hints).toEqual([{ key: W, kind: "top_reaches_actuation", actuation: 50 }]);
    // A actuates at 1.70 mm (85): a bottom zone above 85 reaches it from below.
    const deep = { top: null, bottom: 90 };
    expect(localHints(summarize(new Set([A]), keys, deep), keys, triggers, deep, travel)).toEqual([
      { key: A, kind: "bottom_reaches_actuation", actuation: 85 },
    ]);
  });
});
