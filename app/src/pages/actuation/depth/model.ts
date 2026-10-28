// The key-depth view's logic, as pure functions: what the frames of the live
// view (`start_depth_view`: at most 60 per second, only keys that moved)
// leave on each key (latest depth, the deepest point held since the start),
// the recent history of each key for the strip chart, and an estimate of
// where each key triggers from its actuation and Rapid Trigger settings.
//
// The estimate is only that: the keyboard reports slow presses about 14
// times a second and fast ones less often, so a short press between two
// reports can be missed. It is labelled "estimated" on the page.

import type { DepthFrame, TriggerValue } from "../../../ipc/bindings";

/** What the view knows about one key. */
export interface KeyDepth {
  /** The latest depth, in trigger units (0 at rest). */
  depth: number;
  /** The deepest depth since the view started (or the peaks were cleared). */
  peak: number;
}

/** A point of a key's recent history. */
export interface DepthPoint {
  /** Milliseconds since the test was opened. */
  t: number;
  /** Depth in trigger units. */
  depth: number;
}

/** The estimated trigger state of a key (see {@link estimateStep}). */
export interface TriggerEstimate {
  /** The key counts as pressed. */
  on: boolean;
  /** Rapid Trigger: the deepest point while on, the highest point while off. */
  extreme: number;
  /** Rapid Trigger: the key passed its actuation point since it was last fully released. */
  armed: boolean;
}

/** The view's state. */
export interface DepthModel {
  keys: ReadonlyMap<number, KeyDepth>;
  /** The key that moved last (the deepest one of the latest frame). */
  last: number | null;
  /** The recent depth of every key that moved, the last {@link HISTORY_MS}. */
  history: ReadonlyMap<number, readonly DepthPoint[]>;
  /** The estimated trigger state of every key that moved. */
  estimate: ReadonlyMap<number, TriggerEstimate>;
  /** Frames received. */
  frames: number;
  /** Reports the backend dropped (they came faster than they were drained). */
  dropped: number;
  /** `open_ms` of the latest frame. */
  at: number;
}

/** Nothing received yet. */
export const EMPTY_DEPTH: DepthModel = {
  keys: new Map(),
  last: null,
  history: new Map(),
  estimate: new Map(),
  frames: 0,
  dropped: 0,
  at: 0,
};

/** How much history the strip chart shows, in milliseconds. */
export const HISTORY_MS = 5000;

/**
 * One step of the estimate. Without Rapid Trigger the key is on at or past
 * its actuation point. With Rapid Trigger it turns on at the actuation
 * point, then off as soon as it rises by the release sensitivity from its
 * deepest point, and on again as soon as it goes down by the press
 * sensitivity from its highest point, until it is fully released. An
 * unknown mode gives no estimate (always off).
 */
export function estimateStep(previous: TriggerEstimate | undefined, depth: number, value: TriggerValue): TriggerEstimate {
  const actuation = value.actuation;
  switch (value.mode) {
    case "unknown":
      return { on: false, extreme: depth, armed: false };
    case "off":
      return { on: depth >= actuation, extreme: depth, armed: depth >= actuation };
    case "rapid_trigger":
    case "rapid_trigger_separate": {
      const press = Math.max(1, value.rt_press);
      const release = Math.max(1, value.rt_release);
      const state = previous ?? { on: false, extreme: 0, armed: false };
      if (depth <= 0) return { on: false, extreme: 0, armed: false };
      if (!state.armed) {
        return depth >= actuation ? { on: true, extreme: depth, armed: true } : { on: false, extreme: depth, armed: false };
      }
      if (state.on) {
        const deepest = Math.max(state.extreme, depth);
        return depth <= deepest - release ? { on: false, extreme: depth, armed: true } : { on: true, extreme: deepest, armed: true };
      }
      const highest = Math.min(state.extreme, depth);
      return depth >= highest + press ? { on: true, extreme: depth, armed: true } : { on: false, extreme: highest, armed: true };
    }
  }
}

/** The points from `since` on, and the one before them (where the line comes from). */
export function trimHistory(points: readonly DepthPoint[], since: number): DepthPoint[] {
  const inside = points.findIndex((point) => point.t >= since);
  if (inside === -1) return points.slice(-1);
  return points.slice(Math.max(0, inside - 1));
}

/**
 * The model after a frame. `settings` are the keys' trigger settings (the
 * last read) for the estimate; `holdPeak` keeps each key's deepest point
 * (else the peak follows the latest frame).
 */
export function applyFrame(
  model: DepthModel,
  frame: DepthFrame,
  settings: ReadonlyMap<number, TriggerValue>,
  holdPeak = true,
): DepthModel {
  if (frame.keys.length === 0 && frame.dropped === 0) return model;
  const keys = new Map(model.keys);
  const history = new Map(model.history);
  const estimate = new Map(model.estimate);
  let last: number | null = model.last;
  let deepest = -1;
  const since = frame.open_ms - HISTORY_MS;
  for (const sample of frame.keys) {
    const previous = keys.get(sample.index);
    const reached = Math.max(sample.peak, sample.depth);
    const peak = holdPeak ? Math.max(previous?.peak ?? 0, reached) : reached;
    keys.set(sample.index, { depth: sample.depth, peak });
    // A press that went deeper between two reports shows as a spike.
    const points = trimHistory(history.get(sample.index) ?? [], since);
    if (sample.peak > sample.depth) points.push({ t: frame.open_ms, depth: sample.peak });
    points.push({ t: frame.open_ms, depth: sample.depth });
    history.set(sample.index, points);
    const value = settings.get(sample.index);
    if (value !== undefined) {
      let state = estimate.get(sample.index);
      if (sample.peak > sample.depth) state = estimateStep(state, sample.peak, value);
      estimate.set(sample.index, estimateStep(state, sample.depth, value));
    }
    if (reached > deepest) {
      deepest = reached;
      last = sample.index;
    }
  }
  return {
    keys,
    last,
    history,
    estimate,
    frames: model.frames + 1,
    dropped: model.dropped + frame.dropped,
    at: Math.max(model.at, frame.open_ms),
  };
}

/** The model with every key's held peak set back to its latest depth. */
export function clearPeaks(model: DepthModel): DepthModel {
  const keys = new Map<number, KeyDepth>();
  for (const [index, key] of model.keys) keys.set(index, { depth: key.depth, peak: key.depth });
  return { ...model, keys };
}

/** A key's history within the chart's window ending at `at`. */
export function windowOf(points: readonly DepthPoint[], at: number): DepthPoint[] {
  const from = at - HISTORY_MS;
  return points.filter((point) => point.t >= from);
}

/** The strip chart's polyline ("x,y x,y …") in a `width` × `height` box; depth grows downwards. */
export function chartPoints(
  points: readonly DepthPoint[],
  at: number,
  travel: number,
  width: number,
  height: number,
): string {
  const from = at - HISTORY_MS;
  const x = (t: number) => Math.round(((t - from) / HISTORY_MS) * width * 10) / 10;
  const y = (depth: number) => Math.round((Math.min(travel, Math.max(0, depth)) / travel) * height * 10) / 10;
  // The point before the window too, so the line enters from the left edge (the SVG clips it).
  const shown = trimHistory(points, from);
  if (shown.length === 0) return "";
  // Hold the last value up to the right edge (frames come only on movement).
  const lastPoint = shown[shown.length - 1];
  const line = shown.map((point) => `${x(point.t)},${y(point.depth)}`);
  if (lastPoint !== undefined && lastPoint.t < at) line.push(`${x(at)},${y(lastPoint.depth)}`);
  return line.join(" ");
}
