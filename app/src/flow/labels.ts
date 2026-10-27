// Text for backend codes: progress steps, tables, write warnings, error
// details. Every function takes the dictionary, so it works in and outside
// React.

import { keyName, type Dict } from "../i18n";
import type { Format } from "../i18n/format";
import type {
  AppError,
  ErrorDetail,
  NotEditable,
  ProgressStep,
  TableRef,
  WriteWarningView,
} from "../ipc/bindings";

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
  }
}

/** The localized title of an error. */
export function errorTitle(error: AppError, dict: Dict): string {
  return dict.errors.kind[error.kind].title;
}

/** The keyboard's profile after an error, if the error says. */
export function activeProfileOf(error: AppError): number | null | undefined {
  return error.detail?.detail === "active_profile_now" ? error.detail.profile : undefined;
}
