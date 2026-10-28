// Pure logic of "Share a profile" (no React, no IPC; tested): the parts a
// profile file holds and an import writes, the export and import intents,
// the profile switches they need, the Fn built-in a file replaces, and the
// texts of macro steps and refusals.

import { usageName } from "../../../components/actions/catalog";
import { tableName } from "../../../flow/labels";
import type { Dict } from "../../../i18n";
import type { Format } from "../../../i18n/format";
import type {
  AutoBackupEntry,
  BackupFileSummary,
  EditProblemView,
  ExportIntent,
  FileToken,
  ImportIntent,
  ImportSummaryView,
  KeyRef,
  MacroPlayView,
  MacroStepView,
  ShareSelectionView,
} from "../../../ipc/bindings";
import { KEYBOARD_PROFILES, sortedUnique } from "../model";

/** A part of a profile (a category of settings in a profile file). */
export type SharePart = keyof ShareSelectionView;

/** The parts in the order the page lists them. */
export const SHARE_PARTS: readonly SharePart[] = ["triggers", "lighting", "keys", "dead_band", "socd"];

/** The most characters of an export's name (`ExportIntent.label`). */
export const LABEL_MAX = 120;

/** A selection with nothing chosen. */
export function noParts(): ShareSelectionView {
  return { triggers: false, lighting: false, keys: false, dead_band: false, socd: false };
}

/** The chosen parts, in list order. */
export function chosenParts(selection: ShareSelectionView): SharePart[] {
  return SHARE_PARTS.filter((part) => selection[part]);
}

/** The selection with one part chosen or not. */
export function withPart(selection: ShareSelectionView, part: SharePart, on: boolean): ShareSelectionView {
  return { ...selection, [part]: on };
}

/**
 * The parts ticked when a file is opened: the backend's defaults (actuation
 * and lighting), limited to the parts the file holds.
 */
export function initialInclude(summary: ImportSummaryView): ShareSelectionView {
  const present = summary.contents.present;
  const selection = noParts();
  for (const part of SHARE_PARTS) selection[part] = summary.default_include[part] && present[part];
  return selection;
}

/** The selection limited to the parts the file holds (a part it lacks can't be imported). */
export function includeOf(selection: ShareSelectionView, present: ShareSelectionView): ShareSelectionView {
  const result = noParts();
  for (const part of SHARE_PARTS) result[part] = selection[part] && present[part];
  return result;
}

/** The profiles the keyboard has, plus any other the file holds (for the "Profile of the file" choice). */
export function fileProfiles(summary: ImportSummaryView): number[] {
  return sortedUnique(summary.file.profiles);
}

/** The profile of the file offered first: the backend's choice, else the file's first profile. */
export function defaultFrom(summary: ImportSummaryView): number | null {
  return summary.profile ?? fileProfiles(summary)[0] ?? null;
}

/** The keyboard profile offered first: the active one (no switch needed), else 1. */
export function defaultInto(active: number | null): number {
  return active !== null && KEYBOARD_PROFILES.includes(active) ? active : 1;
}

/**
 * The profile switches comparing an import needs: the target profile when
 * it isn't the active one (or the active one isn't known). The backend
 * checks this against a fresh read and asks again when it differs.
 */
export function importSwitches(into: number, active: number | null): number[] {
  return into === active ? [] : [into];
}

/** Whether exporting a profile from the keyboard switches profiles (reads another profile). */
export function exportNeedsSwitch(profile: number, active: number | null): boolean {
  return profile !== active;
}

/** What {@link buildImportIntent} needs. */
export interface ImportIntentInput {
  source: FileToken;
  /** The chosen profile of the file. */
  from: number | null;
  /** The profile the backend offered for the file (`from: null` means it). */
  fileProfile: number | null;
  into: number;
  include: ShareSelectionView;
  /** The keys whose critical Fn shortcut the user agreed may be replaced (indices). */
  allowedCritical: readonly number[];
  allowFirmware: boolean;
  switches: readonly number[];
}

/** The import intent; `from` is `null` when the file's own profile is imported. */
export function buildImportIntent(input: ImportIntentInput): ImportIntent {
  return {
    source: input.source,
    from: input.from === null || input.from === input.fileProfile ? null : input.from,
    into: input.into,
    include: { ...input.include },
    allowed_critical_keys: sortedUnique(input.allowedCritical),
    allow_firmware_mismatch: input.allowFirmware,
    confirmed_switches: sortedUnique(input.switches),
  };
}

/**
 * An export's name as the intent carries it: trimmed, `null` when empty. The
 * field holds at most {@link LABEL_MAX} characters; the backend removes
 * control characters and keeps at most that many.
 */
export function exportLabel(name: string): string | null {
  const trimmed = name.trim();
  return trimmed === "" ? null : trimmed;
}

/** Where an export reads the profile: the keyboard now, a backup file, or an automatic backup. */
export type ExportFrom = "keyboard" | "file" | "automatic";

/** The export sources in the order the page offers them. */
export const EXPORT_SOURCES: readonly ExportFrom[] = ["keyboard", "file", "automatic"];

/**
 * The parts an export holds at first (design B4): everything but the dead
 * band and the SOCD key pairs, which suit one keyboard and one game more
 * than a shared profile.
 */
export function defaultExportInclude(): ShareSelectionView {
  return { triggers: true, lighting: true, keys: true, dead_band: false, socd: false };
}

/** The profile of a backup file offered first for an export: its active profile, else its first. */
export function exportFileProfile(summary: Pick<BackupFileSummary, "profiles" | "active_profile">): number | null {
  const profiles = sortedUnique(summary.profiles);
  const active = summary.active_profile;
  return active !== null && profiles.includes(active) ? active : (profiles[0] ?? null);
}

/** An automatic backup as an export choice ("Profile 1 · 24 Oct 2026, 07:30:00 · First read"). */
export function automaticOptionText(entry: AutoBackupEntry, dict: Dict, format: Format): string {
  return dict.backupRestore.share.export.automaticOption({
    n: entry.profile,
    date: format.dateTimeSeconds(entry.created_at),
    reason: dict.backupRestore.autoBackups.reasons[entry.reason],
  });
}

/** What {@link buildExportIntent} needs. */
export interface ExportIntentInput {
  profile: number;
  name: string;
  destination: FileToken;
  /** The user agreed to switch profiles (read from the keyboard only). */
  confirmedSwitch: boolean;
  /** The backup file or automatic backup the profile is taken from; `null` reads the keyboard. */
  file: FileToken | null;
  /** The parts the file holds (at least one). */
  include: ShareSelectionView;
}

/**
 * The export intent. A file source never switches profiles (the keyboard
 * isn't used), so it never carries the switch consent.
 */
export function buildExportIntent(input: ExportIntentInput): ExportIntent {
  return {
    source: input.file === null ? { source: "keyboard" } : { source: "file", file: input.file },
    profile: input.profile,
    label: exportLabel(input.name),
    destination: input.destination,
    confirmed_switch: input.file === null && input.confirmedSwitch,
    include: { ...input.include },
  };
}

/** Which of the keyboard's own Fn shortcuts a key is. */
export type Builtin = { kind: "profile"; k: number } | { kind: "reset" } | { kind: "other" };

/** The Fn built-in of a key: Fn + 1..4 switch profiles, Fn + Space (held) resets the keyboard. */
export function builtinOf(key: Pick<KeyRef, "name">): Builtin {
  const n = Number(key.name);
  if (Number.isInteger(n) && KEYBOARD_PROFILES.includes(n)) return { kind: "profile", k: n };
  if (key.name === "Space") return { kind: "reset" };
  return { kind: "other" };
}

/**
 * The parts in words ("Actuation and Rapid Trigger; Lighting and key
 * colours"): semicolons, because the parts' names have commas and "and".
 */
export function partsText(parts: readonly SharePart[], dict: Dict): string {
  return parts.map((part) => dict.backupRestore.share.parts[part]).join("; ");
}

/** How a macro plays, in words. */
export function playText(play: MacroPlayView, dict: Dict): string {
  const modes = dict.keys.playModes;
  return play.mode === "times" ? modes.times({ n: play.count }) : modes[play.mode];
}

/** One macro step in words ("Press A, then wait 20 ms", "Release A"). */
export function macroStepText(step: MacroStepView, dict: Dict, format: Format): string {
  const c = dict.backupRestore.share.contents;
  const key = usageName(step.usage, dict);
  const text = step.motion === "press" ? c.press({ key }) : c.release({ key });
  return step.delay_after_ms > 0 ? c.wait({ step: text, ms: format.number(step.delay_after_ms) }) : text;
}

/** Why an export or an import was refused before anything was sent (`ErrorDetail` "edit"). */
export function editProblemText(problem: EditProblemView, dict: Dict): string {
  const r = dict.backupRestore.share.refusals;
  switch (problem.problem) {
    case "no_changes":
      return r.no_changes;
    case "not_available":
      return r.not_available;
    case "unsupported_current_table":
      return r.unsupported_current_table({ table: tableName(problem.table, dict) });
    default:
      // The editors' own refusals (keys, colours, presets) can't come from a
      // share; the English message stays under "Technical details".
      return r.other;
  }
}
