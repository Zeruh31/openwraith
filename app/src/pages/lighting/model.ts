// The Lighting page's pure logic: the local draft, which controls an effect
// uses, the intent sent to `prepare_light_edit`, the change summary and why
// "Review and write…" is disabled. Nothing here talks to the keyboard.

import type {
  DeviceStatus,
  LightEditIntent,
  LightModeKey,
  LightModeView,
  LightState,
  LightValue,
  WriteBlockReason,
} from "../../ipc/bindings";

/** Unwritten changes of the main light; `null` keeps the keyboard's value. */
export interface LightDraft {
  readonly mode: number | null;
  /** "#rrggbb" lowercase. */
  readonly color: string | null;
  readonly brightness: number | null;
  readonly speed: number | null;
}

/** No changes. */
export const EMPTY_DRAFT: LightDraft = { mode: null, color: null, brightness: null, speed: null };

/** Whether any field of the draft is set (also fields the chosen effect hides). */
export function hasDraftFields(draft: LightDraft): boolean {
  return draft.mode !== null || draft.color !== null || draft.brightness !== null || draft.speed !== null;
}

/**
 * The controls an effect uses (PROTOCOL §4.7.3, from the vendor site's code,
 * CODE-ONLY): Custom shows the stored per-key colours (no colour, no speed),
 * Static and Speed response have no speed. Brightness applies to every
 * effect. The full-colour flag only matters for animated effects.
 */
export interface ModeControls {
  color: boolean;
  speed: boolean;
  fullColor: boolean;
}

const NO_COLOR: ReadonlySet<LightModeKey> = new Set<LightModeKey>(["custom"]);
const NO_SPEED: ReadonlySet<LightModeKey> = new Set<LightModeKey>(["static", "custom", "speed_respond"]);
const NO_FULL_COLOR: ReadonlySet<LightModeKey> = new Set<LightModeKey>(["static", "custom"]);

/** The controls of an effect. */
export function controlsFor(key: LightModeKey): ModeControls {
  return { color: !NO_COLOR.has(key), speed: !NO_SPEED.has(key), fullColor: !NO_FULL_COLOR.has(key) };
}

/** The effects offered as tiles: the keyboard's list (device order, plus Custom), never music rhythm. */
export function offeredModes(light: LightState): LightModeView[] {
  return light.modes.filter((mode) => mode.key !== "music_rhythm");
}

/** Whether `id` is an effect OpenWraith may write. */
export function isOffered(light: LightState, id: number): boolean {
  return offeredModes(light).some((mode) => mode.id === id);
}

/** The view of a mode id: from the keyboard's list, else the current record, else unknown. */
export function modeView(light: LightState, id: number): LightModeView {
  const listed = light.modes.find((mode) => mode.id === id);
  if (listed !== undefined) return listed;
  if (light.current?.mode.id === id) return light.current.mode;
  return { id, key: "unknown" };
}

/** A mode id as "0x05". */
export function hexId(id: number): string {
  return `0x${id.toString(16).padStart(2, "0").toUpperCase()}`;
}

/** What the page shows: the draft over the keyboard's values. */
export interface ShownLight {
  mode: LightModeView;
  color: string;
  brightness: number;
  speed: number;
  controls: ModeControls;
  /** The shown effect can't be written (an unknown effect on the keyboard): one must be chosen first. */
  needsEffect: boolean;
}

/** The values shown for `draft` over the keyboard's `current` light. */
export function shownLight(light: LightState, current: LightValue, draft: LightDraft): ShownLight {
  const mode = draft.mode === null ? current.mode : modeView(light, draft.mode);
  return {
    mode,
    color: draft.color ?? current.color,
    brightness: draft.brightness ?? current.brightness,
    speed: draft.speed ?? current.speed,
    controls: controlsFor(mode.key),
    needsEffect: !isOffered(light, mode.id),
  };
}

/** The editable fields of the draft. */
export type DraftField = keyof LightDraft;

/** Sets one field of the draft; a value equal to the keyboard's is stored as `null` (no change). */
export function withField(draft: LightDraft, current: LightValue, field: DraftField, value: number | string): LightDraft {
  switch (field) {
    case "mode":
      return { ...draft, mode: value === current.mode.id ? null : Number(value) };
    case "color": {
      const color = String(value).toLowerCase();
      return { ...draft, color: color === current.color ? null : color };
    }
    case "brightness":
      return { ...draft, brightness: value === current.brightness ? null : Number(value) };
    case "speed":
      return { ...draft, speed: value === current.speed ? null : Number(value) };
  }
}

/**
 * The intent for the draft, or `null` when nothing changes. Only fields that
 * differ from the keyboard's are sent, and only those the chosen effect
 * uses: a colour picked before switching to Custom, or a speed picked before
 * switching to Static, is left out.
 */
export function buildIntent(light: LightState, draft: LightDraft, revision: number): LightEditIntent | null {
  const current = light.current;
  if (current === null) return null;
  const shown = shownLight(light, current, draft);
  const mode = draft.mode !== null && draft.mode !== current.mode.id ? draft.mode : null;
  const color = shown.controls.color && draft.color !== null && draft.color !== current.color ? draft.color : null;
  const brightness =
    draft.brightness !== null && draft.brightness !== current.brightness ? draft.brightness : null;
  const speed = shown.controls.speed && draft.speed !== null && draft.speed !== current.speed ? draft.speed : null;
  if (mode === null && color === null && brightness === null && speed === null) return null;
  return { mode, color, brightness, speed, based_on: revision };
}

/** One change of an intent, for the apply bar. */
export type LightChange =
  | { field: "mode"; from: LightModeView; to: LightModeView }
  | { field: "color"; from: string; to: string }
  | { field: "brightness"; from: number; to: number }
  | { field: "speed"; from: number; to: number };

/** The changes an intent makes, in display order. */
export function intentChanges(light: LightState, current: LightValue, intent: LightEditIntent): LightChange[] {
  const changes: LightChange[] = [];
  if (intent.mode !== null) changes.push({ field: "mode", from: current.mode, to: modeView(light, intent.mode) });
  if (intent.color !== null) changes.push({ field: "color", from: current.color, to: intent.color });
  if (intent.brightness !== null) {
    changes.push({ field: "brightness", from: current.brightness, to: intent.brightness });
  }
  if (intent.speed !== null) changes.push({ field: "speed", from: current.speed, to: intent.speed });
  return changes;
}

/** The preview's light intensity for a brightness (0.12 at 0 up to 1 at the maximum). */
export function previewOpacity(brightness: number, max: number): number {
  if (max <= 0) return 1;
  const ratio = Math.min(1, Math.max(0, brightness / max));
  return Math.round((0.12 + 0.88 * ratio) * 100) / 100;
}

/** Why "Review and write…" is disabled. */
export type ApplyBlock =
  | { kind: "not_ready" }
  | { kind: "busy" }
  | { kind: "gate"; reason: WriteBlockReason }
  | { kind: "not_editable" }
  | { kind: "choose_effect" };

/** The first reason that blocks writing the draft, or `null`. */
export function applyBlock(
  status: DeviceStatus,
  busy: boolean,
  stale: boolean,
  light: LightState,
  shown: ShownLight | null,
): ApplyBlock | null {
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
  if (!light.editable || light.current === null || shown === null) return { kind: "not_editable" };
  if (shown.needsEffect) return { kind: "choose_effect" };
  return null;
}

/** The page's preview-scenario state, `previewUi("lighting")`: a preset draft. */
export interface LightingPreviewUi {
  draft?: Partial<Record<DraftField, unknown>>;
}

function integerOrNull(value: unknown): number | null {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= 255 ? value : null;
}

/** The draft of a preview scenario (fields that aren't valid are ignored). */
export function draftFromPreview(ui: LightingPreviewUi | undefined): LightDraft {
  const raw = ui?.draft;
  if (raw === undefined) return EMPTY_DRAFT;
  const color = typeof raw.color === "string" && /^#[0-9a-f]{6}$/.test(raw.color) ? raw.color : null;
  return {
    mode: integerOrNull(raw.mode),
    color,
    brightness: integerOrNull(raw.brightness),
    speed: integerOrNull(raw.speed),
  };
}

/** Quick colours offered next to the colour picker. */
export const QUICK_COLORS = [
  { hex: "#ff0000", name: "red" },
  { hex: "#ff8000", name: "orange" },
  { hex: "#ffff00", name: "yellow" },
  { hex: "#00ff00", name: "green" },
  { hex: "#00ffff", name: "cyan" },
  { hex: "#0000ff", name: "blue" },
  { hex: "#8000ff", name: "purple" },
  { hex: "#ffffff", name: "white" },
] as const satisfies readonly { hex: string; name: string }[];

/** The name key of a quick colour (its words are in the lighting dictionary). */
export type QuickColor = (typeof QUICK_COLORS)[number]["name"];
