// The key selection model (pure functions over sets of key indices). The
// knob is never selected; keys a page marks unselectable are left out too.

import { centerOf, KNOB_INDEX, shapeOf, W75_GEOMETRY, type KeyShape } from "./geometry";

/** A set of selected key indices. */
export type Selection = ReadonlySet<number>;

/** Every key index except the knob, in drawing order. */
export const ALL_KEYS: readonly number[] = W75_GEOMETRY.filter((s) => s.shape === "key").map((s) => s.index);

/** W, A, S, D. */
export const WASD: readonly number[] = [46, 68, 69, 70];
/** Up, Left, Down, Right. */
export const ARROWS: readonly number[] = [102, 123, 124, 125];

/** A rectangle in key units. */
export interface UnitRect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

function canSelect(index: number, selectable: ReadonlySet<number>): boolean {
  return index !== KNOB_INDEX && selectable.has(index);
}

/** Only the selectable keys of `indices`. */
export function onlySelectable(indices: Iterable<number>, selectable: ReadonlySet<number>): Set<number> {
  const result = new Set<number>();
  for (const index of indices) {
    if (canSelect(index, selectable)) result.add(index);
  }
  return result;
}

/** Adds or removes one key. */
export function toggle(selection: Selection, index: number, selectable: ReadonlySet<number>): Set<number> {
  const next = new Set(selection);
  if (next.has(index)) {
    next.delete(index);
  } else if (canSelect(index, selectable)) {
    next.add(index);
  }
  return next;
}

/** Selects only this key (or nothing when it can't be selected). */
export function only(index: number, selectable: ReadonlySet<number>): Set<number> {
  return canSelect(index, selectable) ? new Set([index]) : new Set();
}

/** Every selectable key. */
export function selectAll(selectable: ReadonlySet<number>): Set<number> {
  return onlySelectable(ALL_KEYS, selectable);
}

/** Selected keys become unselected and the other way round. */
export function invert(selection: Selection, selectable: ReadonlySet<number>): Set<number> {
  return onlySelectable(
    ALL_KEYS.filter((index) => !selection.has(index)),
    selectable,
  );
}

/** The keys whose centres lie in the rectangle spanned by keys `a` and `b`. */
export function keysBetween(a: number, b: number): number[] {
  const sa = shapeOf(a);
  const sb = shapeOf(b);
  if (sa === undefined || sb === undefined) {
    return [];
  }
  const rect: UnitRect = {
    x0: Math.min(sa.x, sb.x),
    y0: Math.min(sa.y, sb.y),
    x1: Math.max(sa.x + sa.w, sb.x + sb.w),
    y1: Math.max(sa.y + sa.h, sb.y + sb.h),
  };
  return W75_GEOMETRY.filter((shape) => {
    const c = centerOf(shape);
    return c.x >= rect.x0 && c.x <= rect.x1 && c.y >= rect.y0 && c.y <= rect.y1;
  }).map((shape) => shape.index);
}

/** Adds the rectangle from `anchor` to `index` (Shift + click). */
export function addRange(
  selection: Selection,
  anchor: number,
  index: number,
  selectable: ReadonlySet<number>,
): Set<number> {
  const next = new Set(selection);
  for (const key of onlySelectable(keysBetween(anchor, index), selectable)) next.add(key);
  return next;
}

function intersects(shape: KeyShape, rect: UnitRect): boolean {
  return shape.x < rect.x1 && shape.x + shape.w > rect.x0 && shape.y < rect.y1 && shape.y + shape.h > rect.y0;
}

/** The keys a marquee rectangle touches. */
export function keysInRect(rect: UnitRect): number[] {
  const normal: UnitRect = {
    x0: Math.min(rect.x0, rect.x1),
    y0: Math.min(rect.y0, rect.y1),
    x1: Math.max(rect.x0, rect.x1),
    y1: Math.max(rect.y0, rect.y1),
  };
  return W75_GEOMETRY.filter((shape) => intersects(shape, normal)).map((shape) => shape.index);
}

/** The selection after a marquee drag: `base` plus (or minus) the touched keys. */
export function applyMarquee(
  base: Selection,
  rect: UnitRect,
  mode: "add" | "remove",
  selectable: ReadonlySet<number>,
): Set<number> {
  const next = new Set(base);
  for (const index of keysInRect(rect)) {
    if (mode === "remove") next.delete(index);
    else if (canSelect(index, selectable)) next.add(index);
  }
  return next;
}

/**
 * Every selectable key whose setting equals the reference key's (`valueKey`
 * gives a comparable string per key; `null` never matches).
 */
export function sameSetting(
  reference: number,
  valueKey: (index: number) => string | null,
  selectable: ReadonlySet<number>,
): Set<number> {
  const target = valueKey(reference);
  if (target === null) {
    return new Set();
  }
  return onlySelectable(
    ALL_KEYS.filter((index) => valueKey(index) === target),
    selectable,
  );
}

/** A direction for keyboard focus. */
export type Direction = "up" | "down" | "left" | "right";

/** The nearest key in a direction from `from` (spatial navigation); `from` when there is none. */
export function moveFocus(from: number, direction: Direction): number {
  const origin = shapeOf(from);
  if (origin === undefined) {
    return ALL_KEYS[0] ?? from;
  }
  const o = centerOf(origin);
  let best: { index: number; score: number } | null = null;
  for (const shape of W75_GEOMETRY) {
    if (shape.index === from) continue;
    const c = centerOf(shape);
    const dx = c.x - o.x;
    const dy = c.y - o.y;
    let primary: number;
    let secondary: number;
    switch (direction) {
      case "right":
        primary = dx;
        secondary = dy;
        break;
      case "left":
        primary = -dx;
        secondary = dy;
        break;
      case "down":
        primary = dy;
        secondary = dx;
        break;
      case "up":
        primary = -dy;
        secondary = dx;
        break;
    }
    if (primary <= 0.2) continue;
    const score = primary + 2.5 * Math.abs(secondary);
    if (best === null || score < best.score) best = { index: shape.index, score };
  }
  return best?.index ?? from;
}
