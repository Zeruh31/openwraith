// The key-depth view's logic on the frames recorded from the simulator
// (depth_live: W pressed slowly, A tapped quickly, D) and the trigger
// settings of the same read (W: 1.00 mm with Rapid Trigger 0.30 mm; A and D:
// 1.70 mm, Rapid Trigger off).

import { describe, expect, it } from "vitest";

import type { DepthFrame, TriggerValue } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import { keysByIndex } from "../model";
import { applyFrame, chartPoints, clearPeaks, EMPTY_DEPTH, estimateStep, HISTORY_MS, trimHistory, type DepthModel } from "./model";

const W = 46;
const A = 68;
const D = 70;

function frames(): DepthFrame[] {
  const ui = FIXTURES["depth_live"]?.ui;
  return structuredClone(ui?.["depth_frames"] as DepthFrame[]);
}

function settings(): Map<number, TriggerValue> {
  const map = new Map<number, TriggerValue>();
  for (const [index, key] of keysByIndex(FIXTURES["depth_live"]?.initial_snapshot ?? null)) {
    if (key.value !== null) map.set(index, key.value);
  }
  return map;
}

function play(list: readonly DepthFrame[], holdPeak = true): DepthModel {
  const values = settings();
  return list.reduce((model, frame) => applyFrame(model, frame, values, holdPeak), EMPTY_DEPTH);
}

describe("frames", () => {
  it("leave the latest depth and the deepest point on each key", () => {
    const model = play(frames());
    expect(model.frames).toBe(10);
    expect(model.keys.get(W)).toEqual({ depth: 0, peak: 120 });
    // A was tapped faster than the reports: the deepest point came between two of them.
    expect(model.keys.get(A)).toEqual({ depth: 0, peak: 150 });
    expect(model.keys.get(D)).toEqual({ depth: 0, peak: 90 });
    expect(model.at).toBe(476);
  });

  it("follow the latest depth only when the peak isn't held, and clear peaks on request", () => {
    expect(play(frames(), false).keys.get(W)).toEqual({ depth: 0, peak: 0 });
    const held = play(frames().slice(0, 4));
    expect(held.keys.get(W)).toEqual({ depth: 80, peak: 80 });
    expect(clearPeaks(play(frames())).keys.get(A)).toEqual({ depth: 0, peak: 0 });
  });

  it("make the deepest key of the latest frame the key that moved last", () => {
    expect(play(frames().slice(0, 3)).last).toBe(A);
    expect(play(frames().slice(0, 4)).last).toBe(W);
  });

  it("keep each key's recent history, with a spike for a press between two reports", () => {
    const model = play(frames());
    expect(model.history.get(A)).toEqual([
      { t: 204, depth: 150 },
      { t: 204, depth: 70 },
      { t: 289, depth: 0 },
    ]);
    expect(trimHistory([{ t: 0, depth: 1 }, { t: 10, depth: 2 }, { t: 20, depth: 3 }], 15)).toEqual([
      { t: 10, depth: 2 },
      { t: 20, depth: 3 },
    ]);
    expect(trimHistory([{ t: 0, depth: 1 }], 15)).toEqual([{ t: 0, depth: 1 }]);
  });

  it("draw the strip chart over the last five seconds, holding the last value to the right edge", () => {
    const points = [
      { t: 1000, depth: 0 },
      { t: 6000, depth: 85 },
    ];
    expect(HISTORY_MS).toBe(5000);
    expect(chartPoints(points, 6000, 170, 100, 10)).toBe("0,0 100,5");
    expect(chartPoints([{ t: 5000, depth: 170 }], 6000, 170, 100, 10)).toBe("80,10 100,10");
    expect(chartPoints([], 6000, 170, 100, 10)).toBe("");
  });
});

describe("where keys trigger (estimated)", () => {
  const off: TriggerValue = { actuation: 85, mode: "off", mode_byte: 0, rt_press: 1, rt_release: 1 };
  const rt: TriggerValue = { actuation: 50, mode: "rapid_trigger", mode_byte: 12, rt_press: 15, rt_release: 15 };

  it("is on at or past the actuation point without Rapid Trigger", () => {
    expect(estimateStep(undefined, 84, off).on).toBe(false);
    expect(estimateStep(undefined, 85, off).on).toBe(true);
  });

  it("follows Rapid Trigger: on at the actuation point, off after rising by the release, on again after the press", () => {
    let state = estimateStep(undefined, 40, rt);
    expect(state.on).toBe(false);
    state = estimateStep(state, 80, rt);
    expect(state.on).toBe(true);
    state = estimateStep(state, 120, rt);
    expect(state).toEqual({ on: true, extreme: 120, armed: true });
    state = estimateStep(state, 110, rt);
    expect(state.on).toBe(true);
    state = estimateStep(state, 100, rt);
    expect(state.on).toBe(false);
    state = estimateStep(state, 90, rt);
    expect(state.on).toBe(false);
    state = estimateStep(state, 105, rt);
    expect(state.on).toBe(true);
    // Fully released: the next press needs the actuation point again.
    state = estimateStep(state, 0, rt);
    expect(state).toEqual({ on: false, extreme: 0, armed: false });
    expect(estimateStep(state, 30, rt).on).toBe(false);
  });

  it("gives no estimate for an unknown mode", () => {
    expect(estimateStep(undefined, 160, { ...off, mode: "unknown" }).on).toBe(false);
  });

  it("runs on the recorded frames", () => {
    // W: 15, 40, 80 (on), 120, 60 (rose 60 ≥ 0.30 mm: off), 0.
    const values = settings();
    const list = frames();
    const after = (n: number) => list.slice(0, n).reduce((model, frame) => applyFrame(model, frame, values), EMPTY_DEPTH);
    expect(after(4).estimate.get(W)?.on).toBe(true);
    expect(after(9).estimate.get(W)?.on).toBe(false);
    // A (1.70 mm, no Rapid Trigger) reached 150 between two reports: on, then off at rest.
    expect(after(3).estimate.get(A)?.on).toBe(false);
    expect(play(list).estimate.get(A)?.on).toBe(false);
  });
});
