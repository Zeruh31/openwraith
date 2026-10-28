// Pure logic of the Recovery card (no React, no IPC; tested): which profiles
// look reset, the comparisons the card can start (one profile from the
// automatic backup the user picked, or every profile from its newest one),
// their intents and profile switches, and the evidence in plain words.
// Nothing here decides to restore: every comparison starts from a click and
// ends in the standard restore review.

import type { Dict } from "../../../i18n";
import type { Format } from "../../../i18n/format";
import type {
  AutoBackupEntry,
  FileToken,
  FirmwareView,
  KnownGoodTableView,
  RecoveryIntent,
  RecoveryProfileView,
  RecoveryView,
  ResetEvidenceView,
} from "../../../ipc/bindings";
import { sortedUnique } from "../model";

/** A comparison the card can start: the profiles and one automatic backup for each. */
export interface RecoveryRequest {
  /** Ascending. */
  profiles: number[];
  /** In the order of `profiles`. */
  files: FileToken[];
}

/** The profiles that look reset (their factory tables are held). */
export function heldProfiles(view: RecoveryView): number[] {
  return sortedUnique(view.profiles.filter((p) => p.state === "reset_suspected").map((p) => p.profile));
}

/**
 * Why "It's fine, keep them" can't be clicked now: the keyboard reset itself
 * (`"reconnect"`: reconnect and compare first), or nothing is held while
 * profiles were not compared since the keyboard's reset report (`"compare"`:
 * switch to them first). `null`: it can (it keeps the held profiles, or
 * acknowledges a report whose comparisons all came out fine).
 */
export function keepBlock(resetSeen: boolean, held: number, notChecked: boolean): "reconnect" | "compare" | null {
  if (resetSeen) return "reconnect";
  if (held === 0 && notChecked) return "compare";
  return null;
}

/** A profile's automatic backup chosen in the card: the one picked, else its newest. */
export function chosenBackup(profile: RecoveryProfileView, token: FileToken | undefined): AutoBackupEntry | null {
  return profile.backups.find((entry) => entry.file.token === token) ?? profile.backups[0] ?? null;
}

/** One profile from one of its automatic backups. */
export function singleRequest(profile: number, entry: AutoBackupEntry): RecoveryRequest {
  return { profiles: [profile], files: [entry.file.token] };
}

/**
 * "Restore all profiles": the newest automatic backup of every profile that
 * has one (the backend composes them into one backup). `null` when the
 * backend says it isn't possible (fewer than two profiles, or their newest
 * backups have different firmware).
 */
export function restoreAllRequest(view: RecoveryView): RecoveryRequest | null {
  if (!view.can_restore_all) return null;
  const parts = [...view.profiles]
    .sort((a, b) => a.profile - b.profile)
    .flatMap((p) => {
      const newest = p.backups[0];
      return newest === undefined ? [] : [{ profile: p.profile, file: newest.file.token }];
    });
  if (parts.length < 2) return null;
  return { profiles: parts.map((p) => p.profile), files: parts.map((p) => p.file) };
}

/** The intent of a comparison; `switches` are the profiles the user agreed to switch to. */
export function buildRecoveryIntent(
  request: RecoveryRequest,
  allowFirmwareMismatch: boolean,
  switches: readonly number[],
): RecoveryIntent {
  return {
    files: [...request.files],
    allow_firmware_mismatch: allowFirmwareMismatch,
    confirmed_switches: sortedUnique(switches),
  };
}

/**
 * Whether comparing `profiles` needs "Restore anyway": the newest automatic
 * backup of one of them was made with other firmware, or the backend
 * refused a mismatch already (`seen`).
 */
export function needsFirmwareTick(view: RecoveryView, profiles: readonly number[], seen: boolean): boolean {
  return seen || view.profiles.some((p) => profiles.includes(p.profile) && p.firmware_differs);
}

/** A firmware version for a sentence ("3.22.07"), or "Unknown". */
export function firmwareVersion(firmware: FirmwareView | null | undefined, dict: Dict): string {
  return firmware?.version ?? dict.common.unknown;
}

/** The firmware of the automatic backups, as far as the card knows it. */
export function knownFirmware(view: RecoveryView): FirmwareView | null {
  if (view.firmware_changed !== null) return view.firmware_changed.known;
  for (const profile of view.profiles) {
    for (const evidence of profile.evidence) {
      if (evidence.evidence === "firmware_changed") return evidence.known;
    }
  }
  return null;
}

/** Compared settings in words ("Actuation and Rapid Trigger and Lighting"). */
export function tablesText(tables: readonly KnownGoodTableView[], dict: Dict, format: Format): string {
  return format.list(tables.map((table) => dict.backupRestore.recovery.tables[table]));
}

/** Why a profile looks reset, in plain words. */
export function evidenceText(evidence: ResetEvidenceView, dict: Dict, format: Format): string {
  const e = dict.backupRestore.recovery.evidence;
  switch (evidence.evidence) {
    case "reset_seen":
      return e.reset_seen;
    case "firmware_changed":
      return e.firmware_changed({
        known: firmwareVersion(evidence.known, dict),
        current: firmwareVersion(evidence.current, dict),
      });
    case "triggers_to_factory":
      return e.triggers_to_factory;
    case "keymap_group_to_factory":
      return e.keymap_group_to_factory({ list: tablesText(evidence.tables, dict, format) });
  }
}

/** How old a backup was ("3 days", "less than a minute"). */
export function ageText(seconds: number, dict: Dict): string {
  const a = dict.backupRestore.recovery.age;
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return a.underMinute;
  if (s < 3600) return a.minutes({ n: Math.floor(s / 60) });
  if (s < 86400) return a.hours({ n: Math.floor(s / 3600) });
  return a.days({ n: Math.floor(s / 86400) });
}

/** "Profile 2 · FPS" with a local name, else "Profile 2". */
export function profileTitle(profile: Pick<RecoveryProfileView, "profile" | "name">, dict: Dict): string {
  const name = profile.name?.trim() ?? "";
  return name === ""
    ? dict.common.profile({ n: profile.profile })
    : dict.backupRestore.recovery.profileNamed({ n: profile.profile, name });
}

/** An automatic backup as a choice ("24 Oct 2026, 07:30:00 · First read"). */
export function backupOptionText(entry: AutoBackupEntry, dict: Dict, format: Format): string {
  return dict.backupRestore.recovery.backupOption({
    date: format.dateTimeSeconds(entry.created_at),
    reason: dict.backupRestore.autoBackups.reasons[entry.reason],
    kept: entry.pinned,
  });
}

/** Which automatic backup a reset-like profile was compared with, and how old it was then. */
export function comparedText(profile: RecoveryProfileView, dict: Dict, format: Format): string | null {
  const r = dict.backupRestore.recovery;
  if (profile.known_good_at === null) return null;
  const date = format.dateTime(profile.known_good_at);
  return profile.known_good_age_s === null
    ? r.comparedWithDate({ date })
    : r.comparedWith({ date, age: ageText(profile.known_good_age_s, dict) });
}

/** What the card says first: the reset report, the firmware change and the profiles that look reset. */
export function summarySentences(view: RecoveryView, dict: Dict, format: Format): string[] {
  const r = dict.backupRestore.recovery;
  const sentences: string[] = [];
  if (view.reset_reported) {
    sentences.push(
      view.reset_reported_at === null
        ? r.resetReportedNoTime
        : r.resetReported({ time: format.dateTimeSeconds(view.reset_reported_at) }),
    );
  }
  if (view.firmware_changed !== null) {
    sentences.push(
      r.firmwareChanged({
        known: firmwareVersion(view.firmware_changed.known, dict),
        current: firmwareVersion(view.firmware_changed.current, dict),
      }),
    );
  }
  const held = heldProfiles(view);
  if (held.length > 0) {
    sentences.push(r.heldProfiles({ list: format.list(held.map(String)), count: held.length }));
  }
  return sentences;
}

/** The note under a profile that doesn't look reset (or `null`). */
export function stateNote(profile: RecoveryProfileView, dict: Dict, format: Format): string | null {
  const notes = dict.backupRestore.recovery.notes;
  switch (profile.state) {
    case "no_known_good":
      return notes.no_known_good;
    case "not_checked":
      return notes.not_checked;
    case "fine":
      return profile.checked_at === null ? null : notes.fine({ date: format.dateTime(profile.checked_at) });
    case "reset_suspected":
      return null;
  }
}
