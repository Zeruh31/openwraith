// Text for backend codes: progress steps, tables, write warnings, error
// details. Every function takes the dictionary, so it works in and outside
// React.

import { DICTIONARIES, keyName, type Dict } from "../i18n";
import type { Format } from "../i18n/format";
import type {
  AppError,
  BindingView,
  CommitTriggerReasonView,
  DeviceStatus,
  ErrorDetail,
  KeymapProblemView,
  KeyRef,
  LayerView,
  NotEditable,
  ProgressStep,
  ResendReasonView,
  TableRef,
  WriteBlockReason,
  WriteWarningView,
} from "../ipc/bindings";

/**
 * "Another operation is running", or the live key-depth view by name: it
 * holds the keyboard until it is stopped, and only the Key depth tab stops it.
 */
export function busyText(status: Pick<DeviceStatus, "busy">, dict: Dict): string {
  return status.busy?.kind === "depth_view" ? dict.errors.depthViewRunning : dict.errors.writeBlocked.busy;
}

/** A write-gate reason as text ("busy" names the live key-depth view). */
export function writeBlockText(reason: WriteBlockReason, status: Pick<DeviceStatus, "busy">, dict: Dict): string {
  return reason === "busy" ? busyText(status, dict) : dict.errors.writeBlocked[reason];
}

/** "0x05". */
export function hexByte(value: number): string {
  return `0x${value.toString(16).padStart(2, "0")}`;
}

/** The label of a progress step ("Reading profile 2"). */
export function stepLabel(step: ProgressStep, dict: Dict): string {
  const steps = dict.flow.steps;
  switch (step.step) {
    case "checking_other_programs":
      return steps.checking_other_programs;
    case "reading":
      return step.profile === null
        ? steps.reading[step.purpose]
        : steps.readingProfile[step.purpose]({ n: step.profile });
    case "selecting_profile":
      return steps.selecting_profile({ n: step.profile });
    case "returning_to_profile":
      return steps.returning_to_profile({ n: step.profile });
    case "comparing":
      return steps.comparing;
    case "saving_safety_backup":
      return steps.saving_safety_backup;
    case "saving_file":
      return steps.saving_file;
    case "writing":
      return steps.writing({ n: step.profile });
    case "waiting_for_keyboard_save":
      return steps.waiting_for_keyboard_save;
    case "checking_persistence":
      return steps.checking_persistence({ n: step.profile });
    case "final_check":
      return steps.final_check;
    case "refreshing_view":
      return steps.refreshing_view;
  }
}

/** The name of a keyboard table ("Fn keymap", "Macro 3"). Slots are shown 1-based. */
export function tableName(table: TableRef, dict: Dict): string {
  const tables = dict.flow.tables;
  switch (table.table) {
    case "custom_colors":
      return tables.custom_colors({ n: table.number });
    case "keymap":
      return tables.keymap[table.layer];
    case "macro":
      return tables.macro({ n: table.slot + 1 });
    case "advanced_key":
      return tables.advanced_key({ n: table.slot + 1 });
    case "profile":
    case "main_light":
    case "side_light":
    case "active_custom_table":
    case "triggers":
    case "dead_band":
    case "socd_pairs":
    case "socd_settings":
    case "socd":
    case "switch_types":
    case "poll_rate":
    case "win_lock_and_rollover":
      return tables[table.table];
  }
}

/** The text of a write warning. */
export function warningText(warning: WriteWarningView, dict: Dict, format: Format): string {
  const w = dict.flow.warnings;
  switch (warning.code) {
    case "ack_timeout":
      return w.ack_timeout({ packet: warning.packet, of: warning.of, send: warning.send });
    case "optional_ack_missing":
      return w.optional_ack_missing({ packet: warning.packet });
    case "verify_reread":
      return w.verify_reread;
    case "retried":
      return w.retried;
    case "select_ack_missing":
      return w.select_ack_missing({ profile: warning.profile });
    case "profile_push_missing":
      return w.profile_push_missing({ profile: warning.profile });
    case "select_retried":
      return w.select_retried({ profile: warning.profile });
    case "waited_for_late_ack":
      return w.waited_for_late_ack;
    case "waited_for_commit":
      return w.waited_for_commit({ seconds: format.seconds(warning.waited_ms) });
    case "base_keymap_not_saved":
      return w.base_keymap_not_saved;
    case "unexpected_ack":
      return w.unexpected_ack({ packet: warning.packet });
    case "socd_toggled_on_keyboard":
      return w.socd_toggled_on_keyboard({ on: warning.power });
  }
}

/** Why a key can't be edited. */
export function notEditableText(reason: NotEditable, dict: Dict): string {
  const texts = dict.errors.notEditable;
  switch (reason.reason) {
    case "unknown_mode":
      return texts.unknown_mode({ mode: hexByte(reason.mode) });
    case "knob":
    case "not_read":
    case "undecodable":
    case "second_travel_differs":
    case "press_release_differ":
      return texts[reason.reason];
  }
}

/** One line explaining an error's structured detail, or `null`. */
export function describeDetail(detail: ErrorDetail, dict: Dict, format: Format): string | null {
  const d = dict.errors.detail;
  const unknown = dict.common.unknown;
  switch (detail.detail) {
    case "other_program":
      return d.other_program({
        status: detail.evidence.status_reports,
        acks: detail.evidence.foreign_acks,
        profiles: detail.evidence.profile_reports,
      });
    case "profile_changed":
      return detail.expected === null
        ? d.profile_changed_reported({ reported: detail.reported })
        : d.profile_changed({ expected: detail.expected, reported: detail.reported });
    case "profile_changed_before_save":
      return d.profile_changed_before_save({ reported: detail.reported });
    case "firmware_mismatch":
      return d.firmware_mismatch({ backup: detail.backup ?? unknown, keyboard: detail.keyboard ?? unknown });
    case "model_mismatch":
      return d.model_mismatch({ backup: detail.backup ?? unknown, keyboard: detail.keyboard ?? unknown });
    case "unsupported_model":
      return d.unsupported_model({ product: detail.product ?? unknown });
    case "profile_not_in_backup":
      return d.profile_not_in_backup({ n: detail.profile });
    case "active_profile_now":
      return detail.profile === null ? d.active_profile_unknown : d.active_profile_now({ n: detail.profile });
    case "key_not_editable":
      return d.key_not_editable({ key: keyName(dict, detail.key.name), reason: notEditableText(detail.reason, dict) });
    case "value_out_of_range": {
      const distance = detail.field !== "brightness" && detail.field !== "speed";
      const show = (value: number) => (distance ? format.mm(value) : format.number(value));
      return d.value_out_of_range({
        field: dict.errors.rangeField[detail.field],
        value: show(detail.value),
        min: show(detail.min),
        max: show(detail.max),
      });
    }
    case "invalid_intent":
      return dict.errors.intentField[detail.field];
    case "distance_unit":
      return detail.unit_um === null ? d.distance_unit_unknown : d.distance_unit({ um: detail.unit_um });
    case "needs_switch_consent":
      return d.needs_switch_consent({ list: format.list(detail.profiles.map(String)) });
    case "writes_blocked":
      return format.list(detail.reasons.map((reason) => dict.errors.writeBlocked[reason]));
    case "plan_invalidated":
      return dict.errors.planInvalidation[detail.reason];
    case "verify_failed":
      return d.verify_failed({ table: tableName(detail.table, dict), sends: detail.sends });
    case "commit_trigger_incomplete":
      return d.commit_trigger_incomplete({ packet: detail.packet, of: detail.of });
    case "io":
      return detail.file_name === null ? null : d.io({ file: detail.file_name });
    case "unsupported_schema":
      return d.unsupported_schema({ found: detail.found, supported: detail.supported });
    case "keymap":
      return keymapProblemText(detail.problem, dict);
    case "edit":
      // Colour, dead band, SOCD and preset refusals: their pages explain
      // them (the detail's problem) until a shared text exists.
      return null;
  }
}

/** A key with its layer and number, as refusals name it: "Fn + F1 (#2)", "Caps Lock (#90)". */
export function keyRefText(key: Pick<KeyRef, "index" | "name">, layer: LayerView, dict: Dict): string {
  const k = dict.errors.keymap;
  const name = keyName(dict, key.name);
  return layer === "fn" ? k.keyRefFn({ key: name, index: key.index }) : k.keyRef({ key: name, index: key.index });
}

/** A binding (a key on a layer) as refusals name it. */
export function bindingText(binding: BindingView, dict: Dict): string {
  return keyRefText(binding.key, binding.layer, dict);
}

/**
 * Why a key-assignment change was refused (nothing was written). Built-ins
 * are always on the Fn layer; technical reasons stay in the English message
 * (under "Technical details").
 */
export function keymapProblemText(problem: KeymapProblemView, dict: Dict): string {
  const k = dict.errors.keymap;
  const slot = (kind: "macro" | "advanced", n: number) =>
    kind === "macro" ? dict.flow.tables.macro({ n: n + 1 }) : dict.flow.tables.advanced_key({ n: n + 1 });
  switch (problem.problem) {
    case "no_changes":
    case "not_offered":
    case "invalid_combo":
    case "macro_last_delay":
    case "macro_times_zero":
    case "dks_shape":
    case "knob":
    case "fn_key":
    case "unknown_model":
    case "other":
      return k[problem.problem];
    case "too_many_changes":
      return k.too_many_changes({ max: problem.max });
    case "not_a_key":
      return k.not_a_key({ index: problem.index });
    case "slot_out_of_range":
      return k.slot_out_of_range({ slot: slot(problem.kind, problem.slot) });
    case "macro_too_long":
      return k.macro_too_long({ steps: problem.steps, max: problem.max });
    case "macro_unbalanced":
      return k.macro_unbalanced({ step: problem.step + 1 });
    case "dks_point":
      return k.dks_point({ value: mmTenths(problem.value, dict), max: mmTenths(problem.max, dict) });
    case "dks_steps":
      return k.dks_steps({ row: problem.row + 1 });
    case "mt_hold_time":
      return k.mt_hold_time({
        value: `${problem.value_ms} ms`,
        min: `${problem.min_ms} ms`,
        max: `${problem.max_ms} ms`,
      });
    case "swap_not_writable":
      return k.swap_not_writable({ key: keyRefText(problem.key, problem.layer, dict) });
    case "fn_locked":
      return k.fn_locked({ key: keyRefText(problem.key, "fn", dict) });
    case "builtin_not_acknowledged": {
      const params = { key: keyRefText(problem.key, "fn", dict), function: builtinName(problem.key.index, problem.function, dict) };
      return problem.critical ? k.builtin_not_acknowledged_critical(params) : k.builtin_not_acknowledged(params);
    }
    case "no_usage":
      return k.no_usage({ key: keyRefText(problem.key, "base", dict) });
    case "no_free_slot":
      return k.no_free_slot[problem.kind];
    case "slot_already_bound":
      return k.slot_already_bound({ slot: tableName(problem.slot, dict), key: bindingText(problem.by, dict) });
    case "binding_to_empty_slot":
      return k.binding_to_empty_slot({ key: bindingText(problem.binding, dict), slot: tableName(problem.slot, dict) });
    case "new_slot_replaces_edit":
      return k.new_slot_replaces_edit({ key: keyRefText(problem.key, problem.layer, dict), slot: tableName(problem.slot, dict) });
    case "slot_still_referenced":
      return k.slot_still_referenced({
        slot: tableName(problem.slot, dict),
        keys: problem.by.map((binding) => bindingText(binding, dict)).join(", "),
      });
    case "unsupported_current_table":
      return k.unsupported_current_table({ table: tableName(problem.table, dict) });
  }
}

/** A distance in 0.1 mm as millimetres with one decimal in the UI language ("1,2 mm"). */
function mmTenths(tenths: number, dict: Dict): string {
  const text = (tenths / 10).toFixed(1);
  return `${dict === DICTIONARIES.tr ? text.replace(".", ",") : text} mm`;
}

/** The localized name of a keyboard built-in Fn function, by key index (the library's English name otherwise). */
export function builtinName(index: number, fallback: string, dict: Dict): string {
  const names: Readonly<Record<string, string>> = dict.keys.builtins;
  return names[String(index)] ?? fallback;
}

/** The short keycap label of a keyboard built-in Fn function (its full name when there is none). */
export function builtinCap(index: number, fallback: string, dict: Dict): string {
  const caps: Readonly<Record<string, string | undefined>> = dict.keys.builtinCaps;
  return caps[String(index)] ?? builtinName(index, fallback, dict);
}

/** The localized title of an error. */
export function errorTitle(error: AppError, dict: Dict): string {
  return dict.errors.kind[error.kind].title;
}

/** The keyboard's profile after an error, if the error says. */
export function activeProfileOf(error: AppError): number | null | undefined {
  return error.detail?.detail === "active_profile_now" ? error.detail.profile : undefined;
}

/** Why a write is in a key-assignment plan or report (the plan's own fields; any may be absent). */
export interface WriteReasonFields {
  commit_trigger: CommitTriggerReasonView | null;
  resent_unchanged: boolean;
  resent: ResendReasonView | null;
  completes_binding?: boolean | undefined;
  follows_macro_slot_0?: boolean | undefined;
  clears?: boolean | undefined;
}

/**
 * The reasons of one write, in words, from its own fields (never a fixed
 * sentence): the commit trigger (the Fn keymap after a base keymap or
 * before advanced keys, sent unchanged or with changes), a macro re-sent
 * after slot 0, a write that completes a binding, one that empties a slot.
 */
export function writeReasonTexts(write: WriteReasonFields, dict: Dict): string[] {
  const r = dict.flow.writeReasons;
  const texts: string[] = [];
  if (write.commit_trigger === "base_keymap") {
    texts.push(write.resent_unchanged ? r.baseKeymapUnchanged : r.baseKeymapChanged);
  } else if (write.commit_trigger === "advanced_keys") {
    texts.push(write.resent_unchanged ? r.advancedKeysUnchanged : r.advancedKeysChanged);
  }
  if (write.resent !== null) texts.push(dict.flow.resent[write.resent]);
  else if (write.follows_macro_slot_0 === true) texts.push(r.followsMacroSlot0);
  if (write.clears === true) texts.push(r.clears);
  if (write.completes_binding === true) texts.push(r.completesBinding);
  return texts;
}
