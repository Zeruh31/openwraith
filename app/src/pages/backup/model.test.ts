// The page's pure logic: restore scope, switch consent, the intent, plan
// grouping and report counts (checked against the recorded fixtures).

import { describe, expect, it } from "vitest";

import type { ItemReportView, RestorePreview, RestoreReportView, TableRef } from "../../ipc/bindings";
import { FIXTURES } from "../../preview/registry";
import {
  buildRestoreIntent,
  compressRanges,
  condenseTables,
  groupPlan,
  groupReportItems,
  isAllProfiles,
  planChanges,
  profileChoices,
  profilesWithChanges,
  restoreScope,
  severity,
  stampToIso,
  switchesFor,
  tallyReport,
  worstItem,
} from "./model";

function lastOk(fixtureId: string, command: "prepare_restore"): RestorePreview;
function lastOk(fixtureId: string, command: "apply_restore"): RestoreReportView;
function lastOk(fixtureId: string, command: "prepare_restore" | "apply_restore"): RestorePreview | RestoreReportView {
  const fixture = FIXTURES[fixtureId];
  if (fixture === undefined) throw new Error(`no fixture ${fixtureId}`);
  for (const call of [...fixture.calls].reverse()) {
    if (call.command === command && call.ok !== null) return call.ok;
  }
  throw new Error(`${fixtureId} has no successful ${command}`);
}

const item = (result: ItemReportView["result"], extra: Partial<ItemReportView> = {}): ItemReportView => ({
  table: { table: "main_light" },
  key: null,
  commit_trigger: null,
  resent_unchanged: false,
  result,
  ...extra,
});

const triggerItem = (index: number, result: ItemReportView["result"]): ItemReportView =>
  item(result, { table: { table: "triggers" }, key: { index, row: 2, col: index - 44, name: `K${index}` } });

const WRITTEN: ItemReportView["result"] = {
  result: "written",
  sends: 1,
  packets: 1,
  warnings: [],
  persistence: "not_checked",
};

describe("restore scope and switches", () => {
  it("offers profiles 1–4 plus any other profile of the file", () => {
    expect(profileChoices([2])).toEqual([1, 2, 3, 4]);
    expect(profileChoices([1, 5])).toEqual([1, 2, 3, 4, 5]);
  });

  it("restores only selected profiles that the file holds", () => {
    expect(restoreScope([1, 2, 3, 4], new Set([4, 2]))).toEqual([2, 4]);
    expect(restoreScope([2], new Set([1, 2, 3]))).toEqual([2]);
    expect(restoreScope([1, 2], new Set())).toEqual([]);
  });

  it("switches to every profile in scope except the active one", () => {
    expect(switchesFor([1, 2, 3, 4], 1)).toEqual([2, 3, 4]);
    expect(switchesFor([1], 1)).toEqual([]);
    expect(switchesFor([2], 1)).toEqual([2]);
    expect(switchesFor([1, 2], null)).toEqual([1, 2]);
  });

  it("builds the intent with profiles = null when the whole file is restored", () => {
    expect(
      buildRestoreIntent({
        source: "f-1",
        fileProfiles: [1, 2, 3, 4],
        scope: [4, 3, 2, 1],
        allowFirmwareMismatch: false,
        switches: [4, 2, 3],
      }),
    ).toEqual({ source: "f-1", profiles: null, allow_firmware_mismatch: false, confirmed_switches: [2, 3, 4] });
    expect(
      buildRestoreIntent({ source: "f-2", fileProfiles: [1, 2, 3, 4], scope: [2], allowFirmwareMismatch: true, switches: [2] }),
    ).toEqual({ source: "f-2", profiles: [2], allow_firmware_mismatch: true, confirmed_switches: [2] });
  });
});

describe("small helpers", () => {
  it("compresses ranges", () => {
    expect(compressRanges([1, 2, 3, 4])).toEqual(["1–4"]);
    expect(compressRanges([5, 1, 2, 3, 7, 8])).toEqual(["1–3", "5", "7–8"]);
    expect(compressRanges([])).toEqual([]);
  });

  it("recognizes all four profiles", () => {
    expect(isAllProfiles([4, 3, 2, 1])).toBe(true);
    expect(isAllProfiles([1, 2, 3])).toBe(false);
  });

  it("parses safety-backup stamps (UTC)", () => {
    expect(stampToIso("20261024-073000")).toBe("2026-10-24T07:30:00Z");
    expect(stampToIso("2026-10-24")).toBeNull();
  });

  it("condenses table names: macros and advanced keys gathered, duplicates dropped", () => {
    const tables: TableRef[] = [
      { table: "dead_band" },
      { table: "macro", slot: 0 },
      { table: "macro", slot: 1 },
      { table: "keymap", layer: "fn" },
      { table: "advanced_key", slot: 3 },
      { table: "macro", slot: 1 },
      { table: "dead_band" },
    ];
    expect(condenseTables(tables)).toEqual([
      { kind: "table", table: { table: "dead_band" } },
      { kind: "macros", slots: [1, 2] },
      { kind: "table", table: { table: "keymap", layer: "fn" } },
      { kind: "advanced_keys", slots: [4] },
    ]);
  });
});

describe("plan grouping", () => {
  it("groups the recorded all-profile plan per profile", () => {
    const preview = lastOk("restore_preview", "prepare_restore");
    const groups = groupPlan(preview.plan);
    expect(groups.map((g) => g.profile)).toEqual([1, 2, 3, 4]);
    const [one, two, three] = groups;
    // Profile 1: the W trigger, the base keymap, and the Fn keymap re-sent as the commit trigger.
    expect(one?.triggers.map((row) => row.key.name)).toEqual(["W"]);
    expect(one?.writes.map((w) => w.table)).toEqual([
      { table: "keymap", layer: "base" },
      { table: "keymap", layer: "fn" },
    ]);
    expect(one?.writes[1]?.action.resent_unchanged).toBe(true);
    expect(one?.changes).toBe(2);
    expect(one?.packets).toBe(21);
    expect(one?.unchanged).toHaveLength(61);
    // Profile 2: the main light, and an advanced key that blocks the keymap group.
    expect(two?.changes).toBe(1);
    expect(two?.skips.map((s) => s.reason.code)).toEqual(["keymap_group_blocked"]);
    expect(three?.changes).toBe(0);
    expect(planChanges(groups)).toBe(3);
    expect(profilesWithChanges(groups)).toEqual([1, 2]);
  });

  it("finds nothing to write in a matching plan", () => {
    const preview = lastOk("restore_nothing_to_write", "prepare_restore");
    expect(preview.token).toBeNull();
    expect(planChanges(groupPlan(preview.plan))).toBe(0);
  });
});

describe("report counts", () => {
  it("counts a persisted restore without the re-sent Fn keymap", () => {
    const tally = tallyReport(lastOk("restore_ok_persisted", "apply_restore"));
    expect(tally).toMatchObject({ total: 3, sent: 3, verified: 3, kept: 3, resent: 1, lost: 0, maybeLost: 0 });
  });

  it("counts LOST, MAY BE LOST and a stop inside the keymap group", () => {
    expect(tallyReport(lastOk("restore_lost", "apply_restore"))).toMatchObject({ total: 1, lost: 1, kept: 0 });
    expect(tallyReport(lastOk("restore_maybe_lost_disconnect", "apply_restore"))).toMatchObject({
      total: 2,
      maybeLost: 2,
      resent: 1,
    });
    expect(tallyReport(lastOk("restore_cancelled_in_keymap_group", "apply_restore"))).toMatchObject({
      total: 2,
      sent: 1,
      notAttempted: 1,
      resent: 1,
    });
  });

  it("ranks results and picks the worst", () => {
    const lost = item({ result: "lost", sends: 1, packets: 1, warnings: [], mismatch: {
      table: { table: "main_light" },
      read: "main light",
      kind: "differs",
      offsets: [0],
      expected_hex: "00",
      got_hex: "01",
    } });
    const persisted = item({ ...WRITTEN, persistence: "persisted" });
    const baseKeymap = item({ result: "maybe_lost", sends: 1, packets: 10, warnings: [{ code: "base_keymap_not_saved" }] });
    const maybe = item({ result: "maybe_lost", sends: 1, packets: 1, warnings: [] });
    expect(severity(baseKeymap)).toBeGreaterThan(severity(maybe));
    expect(severity(item(WRITTEN))).toBeGreaterThan(severity(persisted));
    expect(worstItem([persisted, maybe, lost, item(WRITTEN)])).toBe(lost);
    expect(worstItem([])).toBeUndefined();
  });

  it("groups many per-key trigger results into one row where the first one was", () => {
    const keys = [46, 68, 69, 70].map((index) => triggerItem(index, WRITTEN));
    const light = item(WRITTEN);
    const entries = groupReportItems([light, ...keys]);
    expect(entries.map((e) => e.kind)).toEqual(["item", "triggers"]);
    const group = entries[1];
    expect(group?.kind === "triggers" ? group.items : []).toHaveLength(4);
    // Fewer than four stay separate.
    expect(groupReportItems([light, ...keys.slice(0, 3)]).map((e) => e.kind)).toEqual(["item", "item", "item", "item"]);
  });
});
