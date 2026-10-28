// Words for what the backend says about the edits of the Actuation tabs
// (dead band, SOCD, presets): refusals (`ErrorDetail::Edit`, which the
// shared error view leaves to the pages), plan warnings, and why "Review
// and write…" can't be used right now.

import { unverifiedWhat } from "../../../components/Badge";
import { keyName, type Dict } from "../../../i18n";
import type { Format } from "../../../i18n/format";
import type { AppError, DeviceStatus, EditFieldView, EditProblemView, EditWarningView } from "../../../ipc/bindings";
import { tableName } from "../../../flow/labels";

/** The refusal of an edit, if the error is one. */
export function editProblemOf(error: AppError): EditProblemView | null {
  return error.detail?.detail === "edit" ? error.detail.problem : null;
}

const DISTANCE_FIELDS: readonly EditFieldView[] = ["top", "bottom", "actuation", "sensitivity", "press", "release"];

/** One sentence that explains a refused edit (nothing was written). */
export function editProblemText(problem: EditProblemView, dict: Dict, format: Format, unitUm: number): string {
  const p = dict.actuation.edit.problems;
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
    case "out_of_range": {
      const distance = DISTANCE_FIELDS.includes(problem.field);
      const show = (value: number) => (distance ? format.mm(value, unitUm) : format.number(value));
      return p.out_of_range({
        field: dict.actuation.edit.fields[problem.field],
        value: show(problem.value),
        min: show(problem.min),
        max: show(problem.max),
      });
    }
    case "no_hid_usage":
      return p.no_hid_usage({ key: keyName(dict, problem.key.name) });
    case "socd_same_key":
      return p.socd_same_key({ key: keyName(dict, problem.key.name) });
    case "socd_duplicate_key":
      return p.socd_duplicate_key({ key: keyName(dict, problem.key.name) });
    case "unknown_preset":
      return p.unknown_preset;
    case "preset_name":
      return p.preset_name;
    case "too_many_presets":
      return p.too_many_presets({ max: problem.max });
    case "unsupported_current_table":
      return p.unsupported_current_table({ table: tableName(problem.table, dict) });
    case "not_available":
      return p.not_available;
    case "other":
      return p.other;
  }
}

/** The text of a plan warning. */
export function editWarningText(warning: EditWarningView, dict: Dict): string {
  const w = dict.actuation.edit.warnings;
  switch (warning.code) {
    case "not_shown_until_custom_mode":
      return w.not_shown_until_custom_mode;
    case "socd_power_all_profiles":
      return w.socd_power_all_profiles;
    case "socd_hotkey":
      return w.socd_hotkey;
    case "unverified":
      return w.unverified({ what: unverifiedWhat(warning.note, dict) });
    case "not_undoable":
      return w.not_undoable({ table: tableName(warning.table, dict) });
    case "other":
      return w.other;
  }
}

/**
 * Why nothing can be written now because of the connection or another
 * operation, or `null`: the live key-depth view by name (it holds the
 * keyboard until it is stopped), else the write gate's first reason, else
 * "busy".
 */
export function gateReason(status: DeviceStatus, dict: Dict): string | null {
  if (status.busy?.kind === "depth_view") return dict.actuation.edit.depthRunning;
  if (!status.write_gate.allowed) return dict.errors.writeBlocked[status.write_gate.reasons[0] ?? "not_connected"];
  if (status.busy !== null) return dict.errors.writeBlocked.busy;
  return null;
}
