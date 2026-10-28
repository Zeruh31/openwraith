// Trigger presets as the page uses them: the library's built-in presets and
// the user's presets (settings of this PC), the draft a preset fills, the
// keys it goes to, and what "Save as a preset" saves. The backend validates
// every value against the keyboard's limits and refuses (never clamps) what
// is out of range; nothing here decides that.

import { WASD } from "../../../components/keyboard";
import type {
  KeyTriggerView,
  PresetRef,
  PresetRtView,
  PresetsView,
  RtIntent,
  TriggerPresetView,
  UserPresetView,
} from "../../../ipc/bindings";
import { EMPTY_DRAFT, isEditable, type TriggerDraft, type ValueTexts } from "../model";

/** Where a preset goes. */
export type PresetTarget = "all" | "wasd" | "selection";

/** A preset of either kind, as the menu lists it. */
export interface PresetItem {
  ref: PresetRef;
  /** The stable key of the item ("builtin:fast", "user:u-1"). */
  key: string;
  actuation: number;
  rt: PresetRtView | RtIntent;
  /** The user's preset, or `null` for a built-in one. */
  user: UserPresetView | null;
}

/** The key of a preset reference. */
export function presetKey(ref: PresetRef): string {
  return `${ref.kind}:${ref.id}`;
}

function builtinItem(preset: TriggerPresetView): PresetItem {
  const ref: PresetRef = { kind: "builtin", id: preset.id };
  return { ref, key: presetKey(ref), actuation: preset.actuation, rt: preset.rapid_trigger, user: null };
}

function userItem(preset: UserPresetView): PresetItem {
  const ref: PresetRef = { kind: "user", id: preset.id };
  return { ref, key: presetKey(ref), actuation: preset.actuation, rt: preset.rapid_trigger, user: preset };
}

/** Every preset, built-in ones first. */
export function presetItems(view: PresetsView): PresetItem[] {
  return [...view.builtin.map(builtinItem), ...view.user.map(userItem)];
}

/**
 * The trigger draft a preset fills. A preset that turns Rapid Trigger off
 * and also sets the stored sensitivities (the factory preset) shows as "off"
 * in the draft; its review is planned from the preset itself.
 */
export function presetDraft(actuation: number, rt: PresetRtView | RtIntent): TriggerDraft {
  switch (rt.mode) {
    case "off":
    case "off_with":
      return { ...EMPTY_DRAFT, actuation, rt: "off" };
    case "same":
      return { ...EMPTY_DRAFT, actuation, rt: "same", sensitivity: rt.sensitivity };
    case "separate":
      return { ...EMPTY_DRAFT, actuation, rt: "separate", press: rt.press, release: rt.release };
  }
}

/** "1,20 mm · RT 0,30 mm" for a preset's values. */
export function presetSetting(actuation: number, rt: PresetRtView | RtIntent, texts: ValueTexts): string {
  const value = {
    actuation,
    mode_byte: 0,
    mode: "off" as const,
    rt_press: 0,
    rt_release: 0,
  };
  switch (rt.mode) {
    case "off":
    case "off_with":
      return texts.setting(value);
    case "same":
      return texts.setting({ ...value, mode: "rapid_trigger", rt_press: rt.sensitivity, rt_release: rt.sensitivity });
    case "separate":
      return texts.setting({ ...value, mode: "rapid_trigger_separate", rt_press: rt.press, rt_release: rt.release });
  }
}

/** The keys a target means: every key that can be changed, WASD, or the selection (index order). */
export function targetKeys(
  target: PresetTarget,
  keys: ReadonlyMap<number, KeyTriggerView>,
  selection: ReadonlySet<number>,
): number[] {
  const wanted =
    target === "all" ? [...keys.keys()] : target === "wasd" ? [...WASD] : [...selection];
  return wanted.filter((index) => isEditable(keys.get(index))).sort((a, b) => a - b);
}

/** What "Save as a preset" saves: actuation and Rapid Trigger, both known. */
export interface PresetValues {
  actuation: number;
  rapid_trigger: RtIntent;
}

/**
 * The values to save: the draft when it sets both the actuation point and
 * Rapid Trigger; else the one setting all selected keys share; else `null`.
 */
export function valuesToSave(
  draft: TriggerDraft,
  selection: ReadonlySet<number>,
  keys: ReadonlyMap<number, KeyTriggerView>,
): PresetValues | null {
  if (draft.actuation !== null && draft.rt !== "keep") {
    switch (draft.rt) {
      case "off":
        return { actuation: draft.actuation, rapid_trigger: { mode: "off" } };
      case "same":
        return { actuation: draft.actuation, rapid_trigger: { mode: "same", sensitivity: draft.sensitivity } };
      case "separate":
        return {
          actuation: draft.actuation,
          rapid_trigger: { mode: "separate", press: draft.press, release: draft.release },
        };
    }
  }
  let found: PresetValues | null = null;
  let text: string | null = null;
  for (const index of selection) {
    const key = keys.get(index);
    if (!isEditable(key)) continue;
    const value = key.value;
    let rt: RtIntent;
    switch (value.mode) {
      case "off":
        rt = { mode: "off" };
        break;
      case "rapid_trigger":
        rt = { mode: "same", sensitivity: value.rt_press };
        break;
      case "rapid_trigger_separate":
        rt = { mode: "separate", press: value.rt_press, release: value.rt_release };
        break;
      case "unknown":
        return null;
    }
    const next: PresetValues = { actuation: value.actuation, rapid_trigger: rt };
    const nextText = JSON.stringify(next);
    if (text !== null && text !== nextText) return null;
    found = next;
    text = nextText;
  }
  return found;
}

/** A preset name as the backend takes it: control characters removed, trimmed, at most 40 characters. */
export function cleanName(name: string): string {
  // eslint-disable-next-line no-control-regex -- removing control characters is the point
  return name.replace(/[\u0000-\u001f\u007f]/g, "").trim();
}

/** The most characters of a preset name. */
export const MAX_NAME = 40;
