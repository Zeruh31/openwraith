// The actuation page's logic, as pure functions: the draft the inspector
// edits, what it does to each selected key, the intent sent to
// `prepare_trigger_edit`, selection helpers and the texts of trigger values.
//
// Nothing here talks to the keyboard. The backend plans the edit again from
// a fresh read (`prepare_trigger_edit`) and is the authority; the local
// computation only drives the preview on the keys and the apply bar.

import { ALL_KEYS, onlySelectable, type Selection } from "../../components/keyboard";
import { W75_KEYS } from "../../generated/w75Keys";
import type { Dict } from "../../i18n";
import type { Format } from "../../i18n/format";
import type {
  DeviceSnapshot,
  KeyTriggerView,
  RtIntent,
  TriggerEditIntent,
  TriggerLimitsView,
  TriggerModeView,
  TriggerValue,
} from "../../ipc/bindings";

/** The Rapid Trigger choice in the inspector. */
export type RtChoice = "keep" | "off" | "same" | "separate";

/** What the keys show: their actuation point or their Rapid Trigger sensitivity. */
export type ShowMode = "actuation" | "rapid_trigger";

/** The unwritten change: applied to every selected key that can be changed. */
export interface TriggerDraft {
  /** New actuation point in trigger units; `null` keeps each key's own. */
  actuation: number | null;
  /** The Rapid Trigger change; `keep` leaves each key's own. */
  rt: RtChoice;
  /** Sensitivity for `same`, in trigger units. */
  sensitivity: number;
  /** Press sensitivity for `separate`, in trigger units. */
  press: number;
  /** Release sensitivity for `separate`, in trigger units. */
  release: number;
}

/** 0.30 mm: the sensitivity offered when no selected key has Rapid Trigger on. */
export const DEFAULT_SENSITIVITY = 15;

/** No change. */
export const EMPTY_DRAFT: TriggerDraft = {
  actuation: null,
  rt: "keep",
  sensitivity: DEFAULT_SENSITIVITY,
  press: DEFAULT_SENSITIVITY,
  release: DEFAULT_SENSITIVITY,
};

/** The W75's limits (PROTOCOL 4.11.1), used only when the snapshot has none (editing is then disabled). */
export const FALLBACK_LIMITS: TriggerLimitsView = { min_actuation: 5, max_actuation: 170, max_sensitivity: 85 };

/** The keyboard's distance unit in micrometres (0.02 mm on the W75). */
export const DEFAULT_UNIT_UM = 20;

/** The mode bytes of the three trigger modes (PROTOCOL 4.11.3). */
const MODE_BYTE: Record<Exclude<TriggerModeView, "unknown">, number> = {
  off: 0x00,
  rapid_trigger: 0x0c,
  rapid_trigger_separate: 0x0d,
};

/** Whether the draft changes anything. */
export function isDirty(draft: TriggerDraft): boolean {
  return draft.actuation !== null || draft.rt !== "keep";
}

/** The draft with no change, keeping the slider values for the next time. */
export function clearDraft(draft: TriggerDraft): TriggerDraft {
  return { ...draft, actuation: null, rt: "keep" };
}

/** The Rapid Trigger part of the intent. */
export function rtIntentOf(draft: TriggerDraft): RtIntent | null {
  switch (draft.rt) {
    case "keep":
      return null;
    case "off":
      return { mode: "off" };
    case "same":
      return { mode: "same", sensitivity: draft.sensitivity };
    case "separate":
      return { mode: "separate", press: draft.press, release: draft.release };
  }
}

/**
 * A key's settings after the draft, as the library computes them: the
 * actuation point if it changes; Rapid Trigger off keeps the stored
 * sensitivities; `same` sets press = release.
 */
export function targetOf(current: TriggerValue, draft: TriggerDraft): TriggerValue {
  const target: TriggerValue = { ...current };
  if (draft.actuation !== null) {
    target.actuation = draft.actuation;
  }
  switch (draft.rt) {
    case "keep":
      break;
    case "off":
      target.mode = "off";
      target.mode_byte = MODE_BYTE.off;
      break;
    case "same":
      target.mode = "rapid_trigger";
      target.mode_byte = MODE_BYTE.rapid_trigger;
      target.rt_press = draft.sensitivity;
      target.rt_release = draft.sensitivity;
      break;
    case "separate":
      target.mode = "rapid_trigger_separate";
      target.mode_byte = MODE_BYTE.rapid_trigger_separate;
      target.rt_press = draft.press;
      target.rt_release = draft.release;
      break;
  }
  return target;
}

/** Whether two settings are the same (what the library compares before writing). */
export function sameValue(a: TriggerValue, b: TriggerValue): boolean {
  return a.mode === b.mode && a.actuation === b.actuation && a.rt_press === b.rt_press && a.rt_release === b.rt_release;
}

/** A key whose settings can be changed. */
export type EditableKey = KeyTriggerView & { value: TriggerValue; not_editable: null };

/** Whether the key's settings can be changed (read, decodable, not the knob). */
export function isEditable(key: KeyTriggerView | undefined): key is EditableKey {
  return key !== undefined && key.selectable && key.not_editable === null && key.value !== null;
}

/** Whether Rapid Trigger is on. */
export function isRtOn(value: TriggerValue): boolean {
  return value.mode === "rapid_trigger" || value.mode === "rapid_trigger_separate";
}

/** The keys of a snapshot by index. */
export function keysByIndex(snapshot: DeviceSnapshot | null): Map<number, KeyTriggerView> {
  return new Map((snapshot?.triggers.keys ?? []).map((key) => [key.key.index, key]));
}

/** The keys that can be selected (every key but the knob). */
export function selectableOf(keys: ReadonlyMap<number, KeyTriggerView>): Set<number> {
  if (keys.size === 0) {
    return new Set(ALL_KEYS);
  }
  return new Set(ALL_KEYS.filter((index) => keys.get(index)?.selectable === true));
}

/** The value ranges, in trigger units. */
export function limitsOf(snapshot: DeviceSnapshot | null): TriggerLimitsView {
  return snapshot?.triggers.limits ?? FALLBACK_LIMITS;
}

/** The distance unit for showing values, in micrometres. */
export function unitOf(snapshot: DeviceSnapshot | null): number {
  return snapshot?.triggers.unit_um ?? DEFAULT_UNIT_UM;
}

/**
 * The basic setting's actuation point: half the maximum travel (85 = 1.70 mm
 * on the W75). It is the value every key had when the W75 was first read and
 * the vendor tool's value for a fresh profile (PROTOCOL 4.11.2); nothing
 * confirms it is the factory default, so the page calls it "basic".
 */
export function basicActuation(limits: TriggerLimitsView): number {
  return Math.floor(limits.max_actuation / 2);
}

/** Whether a key's settings differ from the basic setting (basic actuation point, Rapid Trigger off). */
export function differsFromBasic(value: TriggerValue | null, basic: number): boolean {
  return value !== null && (value.mode !== "off" || value.actuation !== basic);
}

/** What the draft does to the selected keys. */
export interface DraftSummary {
  /** Selected keys that can be changed: they go into the intent (ascending index). */
  keys: number[];
  /** Of those, the keys whose settings change. */
  changing: number[];
  /** Of those, the keys that already have the drafted settings. */
  unchanged: number[];
  /** Selected keys that can't be changed safely; they are left out. */
  leftOut: number[];
}

/** Sorts the selection and splits it into changing, unchanged and left-out keys. */
export function summarize(
  selection: Selection,
  keys: ReadonlyMap<number, KeyTriggerView>,
  draft: TriggerDraft,
): DraftSummary {
  const summary: DraftSummary = { keys: [], changing: [], unchanged: [], leftOut: [] };
  const dirty = isDirty(draft);
  for (const index of [...selection].sort((a, b) => a - b)) {
    const key = keys.get(index);
    if (key?.selectable === false) {
      continue;
    }
    if (!isEditable(key)) {
      summary.leftOut.push(index);
      continue;
    }
    summary.keys.push(index);
    if (dirty && !sameValue(targetOf(key.value, draft), key.value)) {
      summary.changing.push(index);
    } else {
      summary.unchanged.push(index);
    }
  }
  return summary;
}

/**
 * The intent for `prepare_trigger_edit`: the selected keys that can be
 * changed (one packet per changing key is the backend's rule), only the
 * fields the draft changes, and the snapshot revision the user saw.
 * `null` when there is nothing to send.
 */
export function buildIntent(summary: DraftSummary, draft: TriggerDraft, basedOn: number): TriggerEditIntent | null {
  if (!isDirty(draft) || summary.keys.length === 0) {
    return null;
  }
  return {
    keys: [...summary.keys],
    actuation: draft.actuation,
    rapid_trigger: rtIntentOf(draft),
    based_on: basedOn,
  };
}

/** The settings of the selected keys that can be changed. */
export function selectedValues(selection: Selection, keys: ReadonlyMap<number, KeyTriggerView>): TriggerValue[] {
  const values: TriggerValue[] = [];
  for (const index of [...selection].sort((a, b) => a - b)) {
    const key = keys.get(index);
    if (isEditable(key)) values.push(key.value);
  }
  return values;
}

function common(values: readonly number[]): number | null {
  const first = values[0];
  if (first === undefined) return null;
  return values.every((value) => value === first) ? first : null;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** The actuation point the inspector starts from: the common one, else the first selected key's. */
export function initialActuation(values: readonly TriggerValue[], limits: TriggerLimitsView): number {
  const actuations = values.map((value) => value.actuation);
  const start = common(actuations) ?? actuations[0] ?? basicActuation(limits);
  return clamp(start, limits.min_actuation, limits.max_actuation);
}

/**
 * The sensitivity "On" starts from: the common press sensitivity of the
 * selected keys that have Rapid Trigger on, else 0.30 mm. The value a key
 * stores while Rapid Trigger is off (0.02 mm as read) is never offered.
 */
export function initialSensitivity(values: readonly TriggerValue[], limits: TriggerLimitsView): number {
  const on = values.filter(isRtOn).map((value) => value.rt_press);
  return clamp(common(on) ?? DEFAULT_SENSITIVITY, 1, limits.max_sensitivity);
}

/** The press and release sensitivities "separate" starts from (same rule as {@link initialSensitivity}). */
export function initialSeparate(
  values: readonly TriggerValue[],
  limits: TriggerLimitsView,
): { press: number; release: number } {
  const on = values.filter(isRtOn);
  const press = common(on.map((value) => value.rt_press)) ?? DEFAULT_SENSITIVITY;
  const release = common(on.map((value) => value.rt_release)) ?? press;
  return { press: clamp(press, 1, limits.max_sensitivity), release: clamp(release, 1, limits.max_sensitivity) };
}

/** The draft after choosing a Rapid Trigger option (sliders start from the selected keys). */
export function chooseRt(
  draft: TriggerDraft,
  choice: RtChoice,
  values: readonly TriggerValue[],
  limits: TriggerLimitsView,
): TriggerDraft {
  if (choice === draft.rt) {
    return draft;
  }
  if (choice === "same") {
    const sensitivity = draft.rt === "separate" ? draft.press : initialSensitivity(values, limits);
    return { ...draft, rt: choice, sensitivity };
  }
  if (choice === "separate") {
    const start =
      draft.rt === "same" ? { press: draft.sensitivity, release: draft.sensitivity } : initialSeparate(values, limits);
    return { ...draft, rt: choice, ...start };
  }
  return { ...draft, rt: choice };
}

/** A comparable text of a key's setting (Rapid Trigger values matter only while it is on). */
export function settingKey(value: TriggerValue | null): string | null {
  if (value === null) return null;
  return isRtOn(value)
    ? `${value.mode}/${value.actuation}/${value.rt_press}/${value.rt_release}`
    : `${value.mode}/${value.actuation}`;
}

/** Every selectable key whose setting equals the setting of one of the selected keys. */
export function sameSettingKeys(
  selection: Selection,
  keys: ReadonlyMap<number, KeyTriggerView>,
  selectable: ReadonlySet<number>,
): Set<number> {
  const wanted = new Set<string>();
  for (const index of selection) {
    const key = settingKey(keys.get(index)?.value ?? null);
    if (key !== null) wanted.add(key);
  }
  return onlySelectable(
    ALL_KEYS.filter((index) => {
      const key = settingKey(keys.get(index)?.value ?? null);
      return key !== null && wanted.has(key);
    }),
    selectable,
  );
}

/** The keys whose settings differ from the basic setting. */
export function changedKeys(keys: ReadonlyMap<number, KeyTriggerView>, basic: number): Set<number> {
  const changed = new Set<number>();
  for (const [index, key] of keys) {
    if (key.selectable && differsFromBasic(key.value, basic)) changed.add(index);
  }
  return changed;
}

/** The six rows of the W75 (matrix rows 0-5). */
export const ROW_IDS = ["function", "number", "top", "home", "bottom", "space"] as const;

/** A row of the W75. */
export type RowId = (typeof ROW_IDS)[number];

/** The keys of a row (never the knob). */
export function rowKeys(row: RowId): number[] {
  const number = ROW_IDS.indexOf(row);
  return W75_KEYS.filter((key) => !key.knob && key.row === number).map((key) => key.index);
}

/** The library's key names by index. */
export const KEY_NAMES: ReadonlyMap<number, string> = new Map(W75_KEYS.map((key) => [key.index, key.name]));

/** The legend printed on a key cap (word legends are short and the same in every language). */
export function legendOf(dict: Dict, name: string): string {
  const legends: Readonly<Record<string, string>> = dict.actuation.legends;
  return legends[name] ?? name;
}

/** Texts of trigger values for one language and distance unit. */
export interface ValueTexts {
  /** "1,20" (no unit). */
  number: (units: number) => string;
  /** "1,20 mm". */
  mm: (units: number) => string;
  /** The short value on a key cap. */
  cap: (value: TriggerValue, show: ShowMode) => string;
  /** "RT off", "RT 0,30 mm", "RT 0,30 / 0,50 mm". */
  rt: (value: TriggerValue) => string;
  /** "1,20 mm · RT 0,30 mm". */
  setting: (value: TriggerValue) => string;
  /** The spoken setting ("actuation 1,20 mm, Rapid Trigger 0,30 mm"). */
  spoken: (value: TriggerValue) => string;
}

/** The value texts for `dict`/`format` and the keyboard's distance unit. */
export function valueTexts(dict: Dict, format: Format, unitUm: number): ValueTexts {
  const t = dict.actuation;
  const number = (units: number) => format.mmNumber(units, unitUm);
  const mm = (units: number) => format.mm(units, unitUm);
  const rt = (value: TriggerValue): string => {
    switch (value.mode) {
      case "off":
        return t.setting.rtOff;
      case "rapid_trigger":
        return t.setting.rtSame({ value: mm(value.rt_press) });
      case "rapid_trigger_separate":
        return t.setting.rtSeparate({ press: number(value.rt_press), release: mm(value.rt_release) });
      case "unknown":
        return t.setting.rtUnknown;
    }
  };
  return {
    number,
    mm,
    cap: (value, show) => {
      if (show === "actuation") return number(value.actuation);
      switch (value.mode) {
        case "rapid_trigger":
          // Press ≠ release in mode 0C can't be changed (it is marked); show both values.
          return value.rt_press === value.rt_release
            ? number(value.rt_press)
            : t.values.separate({ press: number(value.rt_press), release: number(value.rt_release) });
        case "rapid_trigger_separate":
          return t.values.separate({ press: number(value.rt_press), release: number(value.rt_release) });
        case "off":
          return t.values.off;
        case "unknown":
          return t.values.unknown;
      }
    },
    rt,
    setting: (value) => t.setting.full({ actuation: mm(value.actuation), rt: rt(value) }),
    spoken: (value) => {
      const actuation = t.keyInfo.actuation({ value: mm(value.actuation) });
      switch (value.mode) {
        case "off":
          return `${actuation}, ${t.keyInfo.rtOff}`;
        case "rapid_trigger":
          return `${actuation}, ${t.keyInfo.rtSame({ value: mm(value.rt_press) })}`;
        case "rapid_trigger_separate":
          return `${actuation}, ${t.keyInfo.rtSeparate({ press: mm(value.rt_press), release: mm(value.rt_release) })}`;
        case "unknown":
          return actuation;
      }
    },
  };
}

/** "Now:" of the selection: the actuation point (common or range) and the Rapid Trigger groups. */
export function nowSummary(values: readonly TriggerValue[], dict: Dict, format: Format, texts: ValueTexts): string[] {
  if (values.length === 0) {
    return [];
  }
  const t = dict.actuation.inspector;
  const actuations = values.map((value) => value.actuation);
  const min = Math.min(...actuations);
  const max = Math.max(...actuations);
  const actuation =
    min === max
      ? t.actuationSame({ value: texts.mm(min) })
      : t.actuationMixed({ min: texts.number(min), max: texts.mm(max) });
  const groups = new Map<string, number>();
  for (const value of values) {
    const text = texts.rt(value);
    groups.set(text, (groups.get(text) ?? 0) + 1);
  }
  const rt =
    groups.size === 1
      ? [...groups.keys()].join("")
      : format.list([...groups.entries()].map(([what, n]) => t.rtGroup({ what, n })));
  return [actuation, rt];
}
