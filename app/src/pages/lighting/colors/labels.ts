// Words for Lighting > Key colours that depend on values: colour names for
// screen readers ("red, #FF0000"), key legends and names, refusals of a
// colour edit (`ErrorDetail` "edit"), plan warnings and why "Review and
// write…" is disabled.

import { tableName } from "../../../flow/labels";
import { keyName, type Dict } from "../../../i18n";
import type { ColorsDisabled, EditProblemView, EditWarningView } from "../../../ipc/bindings";
import { colorName, hexText, type ColorsBlock } from "./model";

/** The name of a colour ("dark blue"). */
export function colorNameText(dict: Dict, hex: string): string {
  const t = dict.lighting.colors;
  const name = colorName(hex);
  switch (name.kind) {
    case "black":
    case "white":
      return t.names[name.kind];
    case "grey":
    case "hue": {
      const base = t.names[name.kind === "grey" ? "grey" : name.hue];
      return name.shade === null ? base : t.shades[name.shade]({ name: base });
    }
  }
}

/** A colour as screen readers hear it: "red, #FF0000". */
export function colorText(dict: Dict, hex: string): string {
  return dict.lighting.colors.describe({ name: colorNameText(dict, hex), hex: hexText(hex) });
}

/** The legend printed on a key cap ("SHIFT", "W"). */
export function legendText(dict: Dict, name: string): string {
  const legends: Readonly<Record<string, string>> = dict.lighting.colors.legends;
  return legends[name] ?? name;
}

/** "W, A, S, D" or "Esc, F1, F2, F3, F4, F5 and 75 more". */
export function keysListText(dict: Dict, names: readonly string[], limit = 8): string {
  const shown = names.slice(0, limit).map((name) => keyName(dict, name));
  const list = shown.join(", ");
  return names.length > limit ? dict.lighting.colors.review.keysMore({ list, n: names.length - limit }) : list;
}

/** Why the colours can't be changed. */
export function disabledText(dict: Dict, disabled: ColorsDisabled | null): string {
  const t = dict.lighting.colors.disabled;
  switch (disabled?.reason ?? "not_read") {
    case "not_read":
      return t.not_read;
    case "unreadable":
      return t.unreadable;
    case "unsupported_model":
      return t.unsupported_model;
  }
}

/** Why a colour edit was refused. */
export function editProblemText(dict: Dict, problem: EditProblemView): string {
  const p = dict.lighting.colors.problems;
  switch (problem.problem) {
    case "no_changes":
      return p.no_changes;
    case "too_many_keys":
      return p.too_many_keys({ max: problem.max });
    case "duplicate_key":
      return p.duplicate_key({ key: keyName(dict, problem.key.name) });
    case "not_a_key":
      return p.not_a_key({ index: problem.index });
    case "knob":
      return p.knob;
    case "invalid_color":
      return p.invalid_color;
    case "out_of_range":
      return p.out_of_range({ value: problem.value, min: problem.min, max: problem.max });
    case "unsupported_current_table":
      return p.unsupported_current_table({ table: tableName(problem.table, dict) });
    case "not_available":
      return p.not_available;
    // Refusals of the other editors (SOCD, presets) never come from a colour edit.
    case "no_hid_usage":
    case "socd_same_key":
    case "socd_duplicate_key":
    case "unknown_preset":
    case "preset_name":
    case "too_many_presets":
    case "other":
      return p.other;
  }
}

/** The text of a plan warning, or `null` for warnings that belong to other editors. */
export function warningText(dict: Dict, warning: EditWarningView): string | null {
  const t = dict.lighting.colors;
  switch (warning.code) {
    case "not_shown_until_custom_mode":
      return t.review.notShownBody;
    case "unverified":
      return t.review.unverified;
    case "not_undoable":
      return t.review.notUndoable({ table: tableName(warning.table, dict) });
    case "other":
      return t.review.otherWarning;
    case "socd_power_all_profiles":
    case "socd_hotkey":
      return null;
  }
}

/** The apply bar's disabled reason, or `null`. */
export function blockText(dict: Dict, block: ColorsBlock | null): string | null {
  if (block === null) return null;
  switch (block.kind) {
    case "not_ready":
      return dict.lighting.applyBar.notReady;
    case "busy":
      return dict.lighting.applyBar.busy;
    case "gate":
      return dict.errors.writeBlocked[block.reason];
    case "not_editable":
      return dict.lighting.colors.applyBar.notEditable;
  }
}
