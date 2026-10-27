// Text for the restore plan and report: skip reasons, block causes, plan
// warnings, decoded values and headlines. Every function takes the
// dictionary and the formatters, so they work in and outside React (tested).

import { hexByte, tableName } from "../../flow/labels";
import { keyName, type Dict } from "../../i18n";
import type { Format } from "../../i18n/format";
import type {
  AdvancedSummary,
  BlockCauseView,
  DeadBandPair,
  LightModeView,
  MacroSummary,
  PersistenceCheckView,
  PlanWarningView,
  RestoreReportView,
  SkipReasonView,
  TriggerBytesView,
  TriggerValue,
} from "../../ipc/bindings";
import { compressRanges, isAllProfiles, sortedUnique, type RestoreTally, type TableGroup } from "./model";

/** "1, 2 and 4" (localized list of numbers). */
export function numberList(values: readonly number[], format: Format): string {
  return format.list(values.map(String));
}

/** "All profiles (1–4)", "Profile 2 only" or "Profiles 1 and 3". */
export function profilesLabel(profiles: readonly number[], dict: Dict, format: Format): string {
  const t = dict.backupRestore.profiles;
  const sorted = sortedUnique(profiles);
  const [first] = sorted;
  if (isAllProfiles(sorted)) return t.all({ range: compressRanges(sorted).join(", ") });
  if (sorted.length === 1 && first !== undefined) return t.one({ n: first });
  return t.some({ list: numberList(sorted, format) });
}

/** The Rapid Trigger part of a trigger value ("RT 0.30 mm", "RT off"). */
export function rtText(value: TriggerValue, dict: Dict, format: Format): string {
  const c = dict.backupRestore.changes;
  switch (value.mode) {
    case "off":
      return c.rtOff;
    case "rapid_trigger":
      return c.rt({ value: format.mm(value.rt_press) });
    case "rapid_trigger_separate":
      return c.rtSeparate({ press: format.mmNumber(value.rt_press), release: format.mm(value.rt_release) });
    case "unknown":
      return c.unknownMode({ mode: hexByte(value.mode_byte) });
  }
}

/** A key's trigger record ("1.70 mm · RT off"), "Undecoded value", or "–" when missing. */
export function triggerText(view: TriggerBytesView | null, dict: Dict, format: Format): string {
  if (view === null) return "–";
  if (view.state === "raw") return dict.backupRestore.changes.raw;
  if (view.value.mode === "unknown") return rtText(view.value, dict, format);
  return dict.backupRestore.changes.trigger({
    actuation: format.mm(view.value.actuation),
    rt: rtText(view.value, dict, format),
  });
}

/** A light effect's name ("Wave", "Unknown effect (0x05)"). */
export function lightModeName(mode: LightModeView, dict: Dict): string {
  if (mode.key === "unknown") return dict.backupRestore.changes.unknownEffect({ id: hexByte(mode.id) });
  return dict.lighting.modes[mode.key];
}

/** A macro slot's contents ("4 steps", "empty"). */
export function macroText(summary: MacroSummary, dict: Dict): string {
  const c = dict.backupRestore.changes;
  switch (summary.state) {
    case "empty":
      return c.macroEmpty;
    case "steps":
      return c.macroSteps({ n: summary.count });
    case "in_use":
      return c.macroInUse;
  }
}

/** An advanced-key slot's contents ("empty", "in use (type 0x03)"). */
export function advancedText(summary: AdvancedSummary, dict: Dict): string {
  const c = dict.backupRestore.changes;
  return summary.state === "empty" ? c.advancedEmpty : c.advancedInUse({ kind: hexByte(summary.kind) });
}

/** A key's dead band ("top 0.10 mm, bottom 0.20 mm", "not set"). */
export function deadBandText(pair: DeadBandPair | null, dict: Dict, format: Format): string {
  const c = dict.backupRestore.changes;
  return pair === null ? c.deadBandNone : c.deadBand({ top: format.mm(pair.top), bottom: format.mm(pair.bottom) });
}

/** A distance unit in micrometres as millimetres ("0.02 mm"), or "Unknown". */
function unitText(um: number | null, dict: Dict, format: Format): string {
  return um === null ? dict.common.unknown : format.mm(1, um);
}

/** Why the key assignments can't be restored (a clause that completes the skip reason). */
export function blockCauseText(cause: BlockCauseView, dict: Dict): string {
  const b = dict.backupRestore.blockCauses;
  switch (cause.code) {
    case "non_empty_advanced_key":
      return b.non_empty_advanced_key({ slot: cause.slot + 1 });
    case "non_canonical_macro":
      return b.non_canonical_macro({ slot: cause.slot + 1 });
    case "malformed_reference":
      return b.malformed_reference({ layer: dict.backupRestore.layers[cause.layer], index: cause.index });
    case "incomplete":
      return b.incomplete({ table: tableName(cause.table, dict) });
    case "fn_keymap_not_writable":
      return b.fn_keymap_not_writable;
  }
}

/** Technical detail of a block cause (English rule names), or `null`. */
export function blockCauseTechnical(cause: BlockCauseView): string | null {
  return cause.code === "non_canonical_macro" ? cause.rule : null;
}

/** Why a differing table isn't restored. */
export function skipReasonText(reason: SkipReasonView, dict: Dict, format: Format): string {
  const s = dict.backupRestore.skipReasons;
  switch (reason.code) {
    case "unconfirmed_command":
      // The command code is technical: it is in the item's technical details.
      return s.unconfirmed_command;
    case "unsupported_light_mode":
      return s.unsupported_light_mode({ mode: hexByte(reason.mode) });
    case "unknown_trigger_mode":
      return s.unknown_trigger_mode({ mode: hexByte(reason.mode) });
    case "distance_unit_changed":
      return s.distance_unit_changed({
        backup: unitText(reason.backup_um, dict, format),
        keyboard: unitText(reason.keyboard_um, dict, format),
      });
    case "keymap_group_blocked":
      return s.keymap_group_blocked({ cause: blockCauseText(reason.cause, dict) });
    case "no_known_command":
    case "invalid_light_record":
    case "knob":
    case "second_travel_differs":
    case "press_release_differ":
    case "out_of_range":
    case "trigger_limits_unknown":
    case "dead_band_out_of_range":
    case "dead_band_unit_unknown":
    case "not_in_keyboard":
    case "unusable":
      return s[reason.code];
  }
}

/** A plan warning: its text, how it looks and technical details. */
export interface WarningView {
  text: string;
  tone: "warning" | "info";
  technical: string | null;
}

/** A plan warning in words. Shared-table warnings are amber; the others are informational. */
export function planWarningView(warning: PlanWarningView, dict: Dict, format: Format): WarningView {
  const w = dict.backupRestore.planWarnings;
  const unknown = dict.common.unknown;
  switch (warning.code) {
    case "firmware_differs":
      return {
        text: w.firmware_differs({ backup: warning.backup ?? unknown, keyboard: warning.keyboard ?? unknown }),
        tone: "info",
        technical: null,
      };
    case "constant_differs":
      return { text: w.constant_differs, tone: "info", technical: warning.request };
    case "capability_flags_differ":
      return { text: w.capability_flags_differ, tone: "info", technical: null };
    case "shared_tables_may_change_other_profiles":
      return {
        text: w.shared_tables_may_change_other_profiles({
          tables: format.list(warning.tables.map((table) => tableName(table, dict))),
          profiles: numberList(warning.profiles, format),
        }),
        tone: "warning",
        technical: null,
      };
    case "likely_shared_table_differs":
      return {
        text: w.likely_shared_table_differs({ table: tableName(warning.table, dict) }),
        tone: "warning",
        technical: null,
      };
  }
}

/** Names of condensed tables ("Dead band", "Macros 1–10"). */
export function tableGroupName(group: TableGroup, dict: Dict): string {
  switch (group.kind) {
    case "table":
      return tableName(group.table, dict);
    case "macros":
      return group.slots.length === 1 && group.slots[0] !== undefined
        ? dict.flow.tables.macro({ n: group.slots[0] })
        : dict.backupRestore.review.macros({ list: compressRanges(group.slots).join(", ") });
    case "advanced_keys":
      return group.slots.length === 1 && group.slots[0] !== undefined
        ? dict.flow.tables.advanced_key({ n: group.slots[0] })
        : dict.backupRestore.review.advancedKeys({ list: compressRanges(group.slots).join(", ") });
  }
}

/** Key names for a short list ("W, A, S +2 more"). */
export function keyNamesText(names: readonly string[], dict: Dict, limit = 6): string {
  const shown = names.slice(0, limit).map((name) => keyName(dict, name));
  const rest = names.length - shown.length;
  return rest > 0 ? `${shown.join(", ")} ${dict.common.andMore({ n: rest })}` : shown.join(", ");
}

/** The headline of a restore report. */
export function restoreHeadline(report: RestoreReportView, tally: RestoreTally, dict: Dict): string {
  const h = dict.backupRestore.result.headline;
  switch (report.headline) {
    case "done":
      return tally.kept >= tally.total && tally.total > 0 ? h.done({ n: tally.total }) : h.doneUnchecked({ n: tally.verified });
    case "nothing_to_do":
      return h.nothingToDo;
    case "cancelled":
      return h.cancelled({ written: tally.sent, rest: Math.max(0, tally.total - tally.sent) });
    case "not_written":
      return h.notWritten;
    case "failed":
      return h.failed({ written: tally.verified, total: tally.total });
    case "maybe_lost":
      return h.maybeLost({ n: Math.max(1, tally.maybeLost) });
    case "lost":
      return h.lost({ n: Math.max(1, tally.lost) });
  }
}

/** The persistence check in sentences. */
export function persistenceSentences(check: PersistenceCheckView, dict: Dict, format: Format): string[] {
  const p = dict.backupRestore.result.persistence;
  const sentences: string[] = [];
  if (check.profiles.length > 0) {
    const profiles = sortedUnique(check.profiles);
    sentences.push(p.profiles({ list: numberList(profiles, format), count: profiles.length }));
  }
  // "0 tables still hold…" reads badly next to a loss: say only what happened.
  if (check.persisted > 0 || check.lost === 0) sentences.push(p.kept({ n: check.persisted }));
  if (check.lost > 0) sentences.push(p.lost({ n: check.lost }));
  if (check.not_checked > 0) sentences.push(p.notChecked({ n: check.not_checked }));
  return sentences;
}
