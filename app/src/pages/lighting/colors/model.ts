// Lighting > Key colours: the pure logic. The draft (the wanted colour of
// each key that differs from the keyboard's), its undo/redo history, the
// painting operations (one key, a set of keys, a gradient), the intent sent
// to `prepare_color_edit`, colour names for screen readers, the review's
// grouping of changes and why "Review and write…" is disabled. Nothing here
// talks to the keyboard.

import { centerOf, shapeOf } from "../../../components/keyboard";
import { hexToRgb, normalizeHex } from "../../../format/color";
import type { ColorChangeView, ColorEditIntent, ColorsState, DeviceStatus, WriteBlockReason } from "../../../ipc/bindings";

/** Wanted colours by key index ("#rrggbb" lowercase); only keys whose colour differs from the keyboard's. */
export type ColorMap = ReadonlyMap<number, string>;

/** No changes. */
export const EMPTY_COLORS: ColorMap = new Map<number, string>();

/** The keyboard's colours by key index (every key of the layout but the knob). */
export function keyboardColors(state: ColorsState): Map<number, string> {
  return new Map(state.keys.map((entry) => [entry.key.index, entry.color]));
}

/** The colour a key shows on the page: the draft's, else the keyboard's (`null` for a key without a colour, the knob). */
export function shownColor(draft: ColorMap, current: ReadonlyMap<number, string>, index: number): string | null {
  return draft.get(index) ?? current.get(index) ?? null;
}

/**
 * Sets `keys` to `color` in the draft. A key set back to the keyboard's own
 * colour leaves the draft; keys the keyboard has no colour for (the knob)
 * are ignored. Returns the same map when nothing changes.
 */
export function paint(draft: ColorMap, current: ReadonlyMap<number, string>, keys: Iterable<number>, color: string): ColorMap {
  const colors = new Map<number, string>();
  for (const key of keys) colors.set(key, color);
  return paintEach(draft, current, colors);
}

/** Sets each key of `colors` to its colour (see {@link paint}). */
export function paintEach(draft: ColorMap, current: ReadonlyMap<number, string>, colors: ReadonlyMap<number, string>): ColorMap {
  let next: Map<number, string> | null = null;
  for (const [key, raw] of colors) {
    const now = current.get(key);
    const color = normalizeHex(raw);
    if (now === undefined || color === null) continue;
    const before = draft.get(key);
    if (color === now) {
      if (before !== undefined) {
        next ??= new Map(draft);
        next.delete(key);
      }
    } else if (before !== color) {
      next ??= new Map(draft);
      next.set(key, color);
    }
  }
  return next ?? draft;
}

/**
 * The draft against new keyboard colours (after a write or a new read):
 * keys that now have their wanted colour, and keys the keyboard no longer
 * lists, leave it. Returns the same map when nothing changes.
 */
export function normalize(draft: ColorMap, current: ReadonlyMap<number, string>): ColorMap {
  let next: Map<number, string> | null = null;
  for (const [key, color] of draft) {
    const now = current.get(key);
    if (now === undefined || now === color) {
      next ??= new Map(draft);
      next.delete(key);
    }
  }
  return next ?? draft;
}

/** Whether two drafts hold the same colours. */
export function sameColors(a: ColorMap, b: ColorMap): boolean {
  if (a === b) return true;
  if (a.size !== b.size) return false;
  for (const [key, color] of a) {
    if (b.get(key) !== color) return false;
  }
  return true;
}

// ---------- history ----------

/** The draft with its undo and redo stacks (newest last). */
export interface ColorHistory {
  readonly present: ColorMap;
  readonly past: readonly ColorMap[];
  readonly future: readonly ColorMap[];
}

/** Steps kept for undo. */
export const HISTORY_LIMIT = 100;

/** An empty history. */
export const EMPTY_HISTORY: ColorHistory = { present: EMPTY_COLORS, past: [], future: [] };

/** Records an edit: the present goes onto the undo stack and the redo stack is dropped (a no-op edit records nothing). */
export function record(history: ColorHistory, next: ColorMap): ColorHistory {
  if (sameColors(history.present, next)) return history;
  const past = [...history.past, history.present];
  return { present: next, past: past.length > HISTORY_LIMIT ? past.slice(past.length - HISTORY_LIMIT) : past, future: [] };
}

/** Goes one step back; the same history when there is nothing to undo. */
export function undo(history: ColorHistory): ColorHistory {
  const previous = history.past[history.past.length - 1];
  if (previous === undefined) return history;
  return { present: previous, past: history.past.slice(0, -1), future: [...history.future, history.present] };
}

/** Goes one step forward again; the same history when there is nothing to redo. */
export function redo(history: ColorHistory): ColorHistory {
  const next = history.future[history.future.length - 1];
  if (next === undefined) return history;
  return { present: next, past: [...history.past, history.present], future: history.future.slice(0, -1) };
}

/**
 * Every step of the history against new keyboard colours (see
 * {@link normalize}); steps that become equal to their neighbour stay (undo
 * then simply changes nothing visible).
 */
export function normalizeHistory(history: ColorHistory, current: ReadonlyMap<number, string>): ColorHistory {
  const present = normalize(history.present, current);
  if (present === history.present && history.past.length === 0 && history.future.length === 0) return history;
  return {
    present,
    past: history.past.map((step) => normalize(step, current)),
    future: history.future.map((step) => normalize(step, current)),
  };
}

// ---------- gradient ----------

/** The direction of a gradient. */
export type GradientDirection = "horizontal" | "vertical";

/** The gradient directions, in menu order. */
export const GRADIENT_DIRECTIONS: readonly GradientDirection[] = ["horizontal", "vertical"];

function channelHex(value: number): string {
  return Math.round(Math.min(255, Math.max(0, value)))
    .toString(16)
    .padStart(2, "0");
}

/** The colour `t` (0..1) of the way from `a` to `b`, mixed in sRGB. */
export function mixHex(a: string, b: string, t: number): string {
  const from = hexToRgb(a) ?? { r: 0, g: 0, b: 0 };
  const to = hexToRgb(b) ?? { r: 0, g: 0, b: 0 };
  const k = Math.min(1, Math.max(0, t));
  return `#${channelHex(from.r + (to.r - from.r) * k)}${channelHex(from.g + (to.g - from.g) * k)}${channelHex(from.b + (to.b - from.b) * k)}`;
}

/**
 * A gradient over `keys` from `from` (left or top) to `to` (right or
 * bottom), by each key's centre on the drawing. Keys in the same column (or
 * row) get the same colour; a single position gets `from`.
 */
export function gradientColors(keys: Iterable<number>, from: string, to: string, direction: GradientDirection): Map<number, string> {
  const positions: { key: number; at: number }[] = [];
  for (const key of keys) {
    const shape = shapeOf(key);
    if (shape === undefined || shape.shape !== "key") continue;
    const center = centerOf(shape);
    positions.push({ key, at: direction === "horizontal" ? center.x : center.y });
  }
  const result = new Map<number, string>();
  if (positions.length === 0) return result;
  const min = Math.min(...positions.map((p) => p.at));
  const max = Math.max(...positions.map((p) => p.at));
  for (const { key, at } of positions) {
    result.set(key, max > min ? mixHex(from, to, (at - min) / (max - min)) : normalizeHex(from) ?? from);
  }
  return result;
}

// ---------- the intent ----------

/** The options of a colour edit that are not colours. */
export interface ColorOptions {
  /** "Also switch the effect to Custom" (opt-in; only used when the keyboard isn't in Custom). */
  switchToCustom: boolean;
  /** "Check that the keyboard kept it" (opt-in). */
  checkPersistence: boolean;
}

/** Whether the switch to Custom would be part of the intent. */
export function switchApplies(state: ColorsState, switchToCustom: boolean): boolean {
  return switchToCustom && !state.shown_now && state.can_switch_to_custom;
}

/** The keys of the draft whose colour differs from the keyboard's, in index order. */
export function draftChanges(draft: ColorMap, current: ReadonlyMap<number, string>): { key: number; before: string; after: string }[] {
  const changes: { key: number; before: string; after: string }[] = [];
  for (const [key, after] of draft) {
    const before = current.get(key);
    if (before !== undefined && before !== after) changes.push({ key, before, after });
  }
  return changes.sort((a, b) => a.key - b.key);
}

/** The intent for the draft, or `null` when no key changes. Keys are sent in index order. */
export function buildColorIntent(state: ColorsState, draft: ColorMap, options: ColorOptions): ColorEditIntent | null {
  const changes = draftChanges(draft, keyboardColors(state));
  if (changes.length === 0) return null;
  return {
    colors: changes.map((change) => ({ key: change.key, color: change.after })),
    switch_to_custom: switchApplies(state, options.switchToCustom),
    check_persistence: options.checkPersistence,
    based_on: state.revision,
  };
}

// ---------- the review ----------

/** Planned changes that share their old and new colour. */
export interface ChangeGroup {
  before: string;
  after: string;
  /** The keys, in index order. */
  keys: ColorChangeView["key"][];
}

/** The plan's changes grouped by old and new colour (largest group first, then by the first key). */
export function groupChanges(changes: readonly ColorChangeView[]): ChangeGroup[] {
  const groups = new Map<string, ChangeGroup>();
  for (const change of changes) {
    const id = `${change.before}>${change.after}`;
    const group = groups.get(id);
    if (group === undefined) groups.set(id, { before: change.before, after: change.after, keys: [change.key] });
    else group.keys.push(change.key);
  }
  const list = [...groups.values()];
  for (const group of list) group.keys.sort((a, b) => a.index - b.index);
  return list.sort((a, b) => b.keys.length - a.keys.length || (a.keys[0]?.index ?? 0) - (b.keys[0]?.index ?? 0));
}

/** How many keys show each colour (most used first, then by colour). */
export function colorCounts(colors: ReadonlyMap<number, string>): { color: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const color of colors.values()) counts.set(color, (counts.get(color) ?? 0) + 1);
  return [...counts.entries()]
    .map(([color, count]) => ({ color, count }))
    .sort((a, b) => b.count - a.count || a.color.localeCompare(b.color));
}

/** The board after a plan: the keyboard's colours with the plan's `after` values. */
export function afterColors(current: ReadonlyMap<number, string>, changes: readonly ColorChangeView[]): Map<number, string> {
  const next = new Map(current);
  for (const change of changes) next.set(change.key.index, change.after);
  return next;
}

/** The board before a plan: the keyboard's colours with the plan's `before` values (read for the plan). */
export function beforeColors(current: ReadonlyMap<number, string>, changes: readonly ColorChangeView[]): Map<number, string> {
  const next = new Map(current);
  for (const change of changes) next.set(change.key.index, change.before);
  return next;
}

// ---------- colour names ----------

/** The hue family of a colour name. */
export type ColorHue =
  | "red"
  | "orange"
  | "brown"
  | "yellow"
  | "lime"
  | "green"
  | "cyan"
  | "blue"
  | "purple"
  | "magenta"
  | "pink";

/** A colour's name in parts (the words are in the dictionaries). */
export type ColorName =
  | { kind: "black" }
  | { kind: "white" }
  | { kind: "grey"; shade: "dark" | "light" | null }
  | { kind: "hue"; hue: ColorHue; shade: "dark" | "light" | null };

function hueFamily(h: number, l: number): ColorHue {
  if (h < 15 || h >= 345) return "red";
  if (h < 45) return l < 0.33 ? "brown" : "orange";
  if (h < 70) return "yellow";
  if (h < 95) return "lime";
  if (h < 150) return "green";
  if (h < 195) return "cyan";
  if (h < 255) return "blue";
  if (h < 285) return "purple";
  if (h < 320) return "magenta";
  return "pink";
}

/** An approximate everyday name of a colour (for screen readers; the hex value is said too). */
export function colorName(hex: string): ColorName {
  const rgb = hexToRgb(hex) ?? { r: 0, g: 0, b: 0 };
  const r = rgb.r / 255;
  const g = rgb.g / 255;
  const b = rgb.b / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  const l = (max + min) / 2;
  const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  if (l < 0.07) return { kind: "black" };
  if (l > 0.93) return { kind: "white" };
  const shade = l < 0.3 ? "dark" : l > 0.72 ? "light" : null;
  if (d < 0.06 || s < 0.12) return { kind: "grey", shade };
  let h: number;
  if (max === r) h = 60 * (((g - b) / d) % 6);
  else if (max === g) h = 60 * ((b - r) / d + 2);
  else h = 60 * ((r - g) / d + 4);
  if (h < 0) h += 360;
  const hue = hueFamily(h, l);
  // "Dark brown" is still brown; a brown is already dark.
  return { kind: "hue", hue, shade: hue === "brown" ? null : shade };
}

/** "#FF0000" (upper case, as screen readers spell it out). */
export function hexText(hex: string): string {
  return (normalizeHex(hex) ?? hex).toUpperCase();
}

// ---------- tools ----------

/** The board's tools. */
export type ColorTool = "select" | "brush" | "eyedropper";

/** The tools, in toolbar order. */
export const COLOR_TOOLS: readonly ColorTool[] = ["select", "brush", "eyedropper"];

/** Quick colours of the colour picker (the effect tab's eight, plus black = off). */
export const SWATCHES = [
  { hex: "#ff0000", name: "red" },
  { hex: "#ff8000", name: "orange" },
  { hex: "#ffff00", name: "yellow" },
  { hex: "#00ff00", name: "green" },
  { hex: "#00ffff", name: "cyan" },
  { hex: "#0000ff", name: "blue" },
  { hex: "#8000ff", name: "purple" },
  { hex: "#ffffff", name: "white" },
  { hex: "#000000", name: "off" },
] as const satisfies readonly { hex: string; name: string }[];

/** The name key of a quick colour. */
export type Swatch = (typeof SWATCHES)[number]["name"];

// ---------- blocks ----------

/** Why "Review and write…" is disabled. */
export type ColorsBlock =
  | { kind: "not_ready" }
  | { kind: "busy" }
  | { kind: "gate"; reason: WriteBlockReason }
  | { kind: "not_editable" };

/** The first reason that blocks writing the colour draft, or `null`. */
export function colorsBlock(status: DeviceStatus, busy: boolean, stale: boolean, state: ColorsState | null): ColorsBlock | null {
  if (status.connection.state !== "ready") {
    const reason = status.write_gate.reasons[0];
    return reason === undefined ? { kind: "not_ready" } : { kind: "gate", reason };
  }
  if (busy) return { kind: "busy" };
  if (stale) return { kind: "gate", reason: "snapshot_stale" };
  if (!status.write_gate.allowed) {
    const reason = status.write_gate.reasons[0];
    return reason === undefined ? { kind: "not_ready" } : { kind: "gate", reason };
  }
  if (state === null || !state.editable) return { kind: "not_editable" };
  return null;
}

// ---------- browser preview ----------

/** The tab's preview-scenario state, `previewUi("lightingColors")`: a preset draft. */
export interface ColorsPreviewUi {
  /** Key index (as a string) → "#rrggbb". */
  colors?: Record<string, unknown>;
  switchToCustom?: unknown;
}

/** The draft of a preview scenario (entries that aren't valid are ignored). */
export function draftFromPreview(ui: ColorsPreviewUi | undefined): { colors: ColorMap; switchToCustom: boolean } {
  const colors = new Map<number, string>();
  for (const [key, value] of Object.entries(ui?.colors ?? {})) {
    const index = Number(key);
    const color = typeof value === "string" ? normalizeHex(value) : null;
    if (Number.isInteger(index) && index >= 0 && index <= 131 && color !== null) colors.set(index, color);
  }
  return { colors, switchToCustom: ui?.switchToCustom === true };
}
