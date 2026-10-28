// Pure logic of the Backup & Restore page (no React, no IPC; tested):
// the restore scope and its profile switches, the restore intent, the plan
// grouped per profile, and the counts of a restore report.

import type {
  ItemReportView,
  KeyRef,
  PlanActionView,
  PlanItemView,
  RestoreIntent,
  RestorePlanView,
  RestoreReportView,
  SkipReasonView,
  TableRef,
  TriggerBytesView,
} from "../../ipc/bindings";

/** The W75's profiles. */
export const KEYBOARD_PROFILES: readonly number[] = [1, 2, 3, 4];

/** Sorted, without duplicates. */
export function sortedUnique(values: Iterable<number>): number[] {
  return [...new Set(values)].sort((a, b) => a - b);
}

/** The profiles offered as checkboxes: 1–4 plus any other profile the file holds. */
export function profileChoices(fileProfiles: readonly number[]): number[] {
  return sortedUnique([...KEYBOARD_PROFILES, ...fileProfiles]);
}

/** The profiles a restore covers: the selected ones that the file holds, ascending. */
export function restoreScope(fileProfiles: readonly number[], selected: ReadonlySet<number>): number[] {
  return sortedUnique(fileProfiles.filter((profile) => selected.has(profile)));
}

/**
 * The profiles that comparing switches to: every profile in scope except
 * the active one (all of them when the active profile isn't known). The
 * backend checks this against a fresh read and asks again when it differs.
 */
export function switchesFor(scope: readonly number[], active: number | null): number[] {
  return scope.filter((profile) => profile !== active);
}

/** What {@link buildRestoreIntent} needs. */
export interface RestoreIntentInput {
  source: string;
  fileProfiles: readonly number[];
  scope: readonly number[];
  allowFirmwareMismatch: boolean;
  switches: readonly number[];
}

/** The restore intent; `profiles` is `null` when every profile of the file is restored. */
export function buildRestoreIntent(input: RestoreIntentInput): RestoreIntent {
  const all = sortedUnique(input.fileProfiles);
  const scope = sortedUnique(input.scope);
  const everything = all.length === scope.length && all.every((profile, i) => scope[i] === profile);
  return {
    source: input.source,
    profiles: everything ? null : scope,
    allow_firmware_mismatch: input.allowFirmwareMismatch,
    confirmed_switches: sortedUnique(input.switches),
  };
}

/** Runs of consecutive numbers ("1", "3–5"), for "Macros 1–10" and "All profiles (1–4)". */
export function compressRanges(values: readonly number[]): string[] {
  const sorted = sortedUnique(values);
  const parts: string[] = [];
  let start: number | null = null;
  let previous: number | null = null;
  const flush = () => {
    if (start === null || previous === null) return;
    parts.push(start === previous ? String(start) : `${start}–${previous}`);
  };
  for (const value of sorted) {
    if (previous !== null && value === previous + 1) {
      previous = value;
      continue;
    }
    flush();
    start = value;
    previous = value;
  }
  flush();
  return parts;
}

/** Whether the profiles are exactly the keyboard's four. */
export function isAllProfiles(profiles: readonly number[]): boolean {
  const sorted = sortedUnique(profiles);
  return sorted.length === KEYBOARD_PROFILES.length && sorted.every((p, i) => KEYBOARD_PROFILES[i] === p);
}

/** A safety-backup time stamp (`YYYYMMDD-HHMMSS`, UTC) as RFC 3339, or `null`. */
export function stampToIso(stamp: string): string | null {
  const match = /^(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})$/.exec(stamp);
  if (match === null) return null;
  const [, y, mo, d, h, mi, s] = match;
  return `${y ?? ""}-${mo ?? ""}-${d ?? ""}T${h ?? ""}:${mi ?? ""}:${s ?? ""}Z`;
}

// ---------- plan ----------

/** A planned write. */
export type WriteAction = Extract<PlanActionView, { action: "write" }>;
/** A table (or key) that will be written. */
export interface WriteItem {
  table: TableRef;
  key: KeyRef | null;
  action: WriteAction;
}

/** A key whose trigger settings will be written. */
export interface TriggerRow {
  key: KeyRef;
  now: TriggerBytesView | null;
  backup: TriggerBytesView | null;
  packets: number;
}

/** A table (or key) that differs but isn't restored. */
export interface SkipItem {
  table: TableRef;
  key: KeyRef | null;
  reason: SkipReasonView;
  message: string;
}

/** The plan of one profile, grouped for the review. */
export interface ProfileGroup {
  profile: number;
  compared: boolean;
  /** Per-key trigger writes. */
  triggers: TriggerRow[];
  /** The other writes in execution order (the re-sent Fn keymap included). */
  writes: WriteItem[];
  skips: SkipItem[];
  unchanged: TableRef[];
  notInBackup: TableRef[];
  /** What the user sees as changes: trigger keys and tables, without re-sent unchanged tables. */
  changes: number;
  packets: number;
}

function addItem(group: ProfileGroup, item: PlanItemView): void {
  const action = item.action;
  switch (action.action) {
    case "write": {
      group.packets += action.packets;
      // A re-sent Fn keymap or macro slot changes no setting.
      if (!action.resent_unchanged && action.resent === null) group.changes += 1;
      if (item.table.table === "triggers" && item.key !== null) {
        const change = action.changes.find((c) => c.kind === "trigger");
        group.triggers.push({
          key: item.key,
          now: change?.kind === "trigger" ? change.old : null,
          backup: change?.kind === "trigger" ? change.new : null,
          packets: action.packets,
        });
      } else {
        group.writes.push({ table: item.table, key: item.key, action });
      }
      break;
    }
    case "skip":
      group.skips.push({ table: item.table, key: item.key, reason: action.reason, message: action.message });
      break;
    case "unchanged":
      group.unchanged.push(item.table);
      break;
    case "not_in_backup":
      group.notInBackup.push(item.table);
      break;
  }
}

/** Groups each profile's plan items for the review. */
export function groupPlan(plan: RestorePlanView): ProfileGroup[] {
  return plan.profiles.map((profile) => {
    const group: ProfileGroup = {
      profile: profile.profile,
      compared: profile.compared,
      triggers: [],
      writes: [],
      skips: [],
      unchanged: [],
      notInBackup: [],
      changes: 0,
      packets: 0,
    };
    for (const item of profile.items) addItem(group, item);
    return group;
  });
}

/** The number of changes of a grouped plan. */
export function planChanges(groups: readonly ProfileGroup[]): number {
  return groups.reduce((sum, group) => sum + group.changes, 0);
}

/** The profiles that have changes. */
export function profilesWithChanges(groups: readonly ProfileGroup[]): number[] {
  return groups.filter((group) => group.changes > 0).map((group) => group.profile);
}

/** A display group of table names ("Macros 1–10"). */
export type TableGroup =
  | { kind: "table"; table: TableRef }
  | { kind: "macros"; slots: number[] }
  | { kind: "advanced_keys"; slots: number[] };

function tableKey(table: TableRef): string {
  switch (table.table) {
    case "custom_colors":
      return `custom_colors:${table.number}`;
    case "keymap":
      return `keymap:${table.layer}`;
    case "macro":
      return `macro:${table.slot}`;
    case "advanced_key":
      return `advanced_key:${table.slot}`;
    default:
      return table.table;
  }
}

/**
 * Table names for a compact list: duplicates removed, macros and advanced
 * keys gathered into one entry each (slots 1-based), first-seen order.
 */
export function condenseTables(tables: readonly TableRef[]): TableGroup[] {
  const groups: TableGroup[] = [];
  const seen = new Set<string>();
  let macros: { kind: "macros"; slots: number[] } | null = null;
  let advanced: { kind: "advanced_keys"; slots: number[] } | null = null;
  for (const table of tables) {
    const key = tableKey(table);
    if (seen.has(key)) continue;
    seen.add(key);
    if (table.table === "macro") {
      if (macros === null) {
        macros = { kind: "macros", slots: [] };
        groups.push(macros);
      }
      macros.slots.push(table.slot + 1);
    } else if (table.table === "advanced_key") {
      if (advanced === null) {
        advanced = { kind: "advanced_keys", slots: [] };
        groups.push(advanced);
      }
      advanced.slots.push(table.slot + 1);
    } else {
      groups.push({ kind: "table", table });
    }
  }
  return groups;
}

// ---------- report ----------

/** The counts of a restore report as the user sees them (re-sent unchanged tables apart). */
export interface RestoreTally {
  /** Planned changes (not re-sent, not already matching). */
  total: number;
  /** Sent to the keyboard (written, may be lost or lost). */
  sent: number;
  /** Written and verified. */
  verified: number;
  /** Written, verified and kept after a profile switch. */
  kept: number;
  maybeLost: number;
  lost: number;
  failed: number;
  notAttempted: number;
  alreadyMatched: number;
  /** Tables re-sent unchanged (the Fn keymap as the keymap's commit trigger). */
  resent: number;
}

/** Counts a restore report's items. */
export function tallyReport(report: RestoreReportView): RestoreTally {
  const tally: RestoreTally = {
    total: 0,
    sent: 0,
    verified: 0,
    kept: 0,
    maybeLost: 0,
    lost: 0,
    failed: 0,
    notAttempted: 0,
    alreadyMatched: 0,
    resent: 0,
  };
  for (const profile of report.profiles) {
    for (const item of profile.items) {
      if (item.resent_unchanged) {
        tally.resent += 1;
        continue;
      }
      const result = item.result;
      if (result.result === "already_matched") {
        tally.alreadyMatched += 1;
        continue;
      }
      tally.total += 1;
      switch (result.result) {
        case "written":
          tally.sent += 1;
          tally.verified += 1;
          if (result.persistence === "persisted") tally.kept += 1;
          break;
        case "maybe_lost":
          tally.sent += 1;
          tally.maybeLost += 1;
          break;
        case "lost":
          tally.sent += 1;
          tally.lost += 1;
          break;
        case "failed":
          tally.failed += 1;
          break;
        case "not_attempted":
          tally.notAttempted += 1;
          break;
      }
    }
  }
  return tally;
}

/** How bad an item's result is (higher is worse). */
export function severity(item: ItemReportView): number {
  const result = item.result;
  switch (result.result) {
    case "lost":
      return 7;
    case "failed":
      return 6;
    case "maybe_lost":
      return result.warnings.some((w) => w.code === "base_keymap_not_saved") ? 5 : 4;
    case "not_attempted":
      return 3;
    case "written":
      return result.persistence === "persisted" ? 1 : 2;
    case "already_matched":
      return 0;
  }
}

/** The item with the worst result, or `undefined` for an empty list. */
export function worstItem(items: readonly ItemReportView[]): ItemReportView | undefined {
  let worst: ItemReportView | undefined;
  for (const item of items) {
    if (worst === undefined || severity(item) > severity(worst)) worst = item;
  }
  return worst;
}

/** A row of a profile's results: one item, or the per-key trigger writes together. */
export type ReportEntry = { kind: "item"; item: ItemReportView } | { kind: "triggers"; items: ItemReportView[] };

/**
 * Groups per-key trigger results into one row (where the first of them
 * was) when there are at least `threshold` of them.
 */
export function groupReportItems(items: readonly ItemReportView[], threshold = 4): ReportEntry[] {
  const triggers = items.filter((item) => item.table.table === "triggers" && item.key !== null);
  if (triggers.length < threshold) {
    return items.map((item) => ({ kind: "item", item }));
  }
  const entries: ReportEntry[] = [];
  let placed = false;
  for (const item of items) {
    if (item.table.table === "triggers" && item.key !== null) {
      if (!placed) {
        entries.push({ kind: "triggers", items: triggers });
        placed = true;
      }
      continue;
    }
    entries.push({ kind: "item", item });
  }
  return entries;
}
