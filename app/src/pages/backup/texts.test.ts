// Text for every backend code of the restore plan and report, in English
// and Turkish: nothing empty, slots shown 1-based, Turkish without case
// suffixes on numbers.

import { describe, expect, it } from "vitest";

import { DICTIONARIES } from "../../i18n";
import { makeFormat } from "../../i18n/format";
import type {
  BlockCauseView,
  PlanWarningView,
  RestoreReportView,
  SkipReasonView,
  TriggerBytesView,
} from "../../ipc/bindings";
import { FIXTURES } from "../../preview/registry";
import { tallyReport } from "./model";
import {
  advancedText,
  blockCauseText,
  deadBandText,
  keyNamesText,
  macroText,
  persistenceSentences,
  planWarningView,
  profilesLabel,
  restoreHeadline,
  skipReasonText,
  tableGroupName,
  triggerText,
} from "./texts";

const LANGS = ["en", "tr"] as const;

const CAUSES: BlockCauseView[] = [
  { code: "non_empty_advanced_key", slot: 0 },
  { code: "non_canonical_macro", slot: 2, rule: "trailing bytes" },
  { code: "malformed_reference", layer: "base", index: 7 },
  { code: "incomplete", table: { table: "macro", slot: 4 } },
  { code: "fn_keymap_not_writable", reason: "base_keymap" },
];

const SKIPS: SkipReasonView[] = [
  { code: "unconfirmed_command", command: "0x0b" },
  { code: "no_known_command" },
  { code: "unsupported_light_mode", mode: 15 },
  { code: "invalid_light_record" },
  { code: "knob" },
  { code: "unknown_trigger_mode", mode: 5 },
  { code: "second_travel_differs" },
  { code: "press_release_differ" },
  { code: "out_of_range" },
  { code: "trigger_limits_unknown" },
  { code: "dead_band_out_of_range" },
  { code: "dead_band_unit_unknown" },
  { code: "distance_unit_changed", backup_um: 10, keyboard_um: null },
  ...CAUSES.map((cause): SkipReasonView => ({ code: "keymap_group_blocked", cause })),
  { code: "not_in_keyboard" },
  { code: "unusable" },
];

const WARNINGS: PlanWarningView[] = [
  { code: "firmware_differs", backup: "3.23.01", keyboard: null },
  { code: "constant_differs", request: "21/04 distance unit" },
  { code: "capability_flags_differ" },
  { code: "shared_tables_may_change_other_profiles", tables: [{ table: "side_light" }], profiles: [2, 3] },
  { code: "likely_shared_table_differs", table: { table: "poll_rate" } },
];

function report(fixtureId: string): RestoreReportView {
  const call = FIXTURES[fixtureId]?.calls.find((c) => c.command === "apply_restore");
  if (call?.command !== "apply_restore" || call.ok === null) throw new Error(`no restore report in ${fixtureId}`);
  return call.ok;
}

describe.each(LANGS)("restore texts (%s)", (lang) => {
  const dict = DICTIONARIES[lang];
  const format = makeFormat(lang);

  it("explains every skip reason and block cause", () => {
    for (const reason of SKIPS) {
      const text = skipReasonText(reason, dict, format);
      expect(text.trim().length, reason.code).toBeGreaterThan(10);
      expect(text).not.toContain("undefined");
    }
    // Slots are shown 1-based; the index of a broken entry is not a slot.
    expect(blockCauseText({ code: "non_empty_advanced_key", slot: 0 }, dict)).toContain("1");
    expect(blockCauseText({ code: "non_canonical_macro", slot: 2, rule: "x" }, dict)).toContain("3");
    expect(blockCauseText({ code: "malformed_reference", layer: "fn", index: 7 }, dict)).toContain("7");
  });

  it("explains every plan warning; shared tables are amber", () => {
    for (const warning of WARNINGS) {
      const view = planWarningView(warning, dict, format);
      expect(view.text.trim().length, warning.code).toBeGreaterThan(10);
      expect(view.tone).toBe(warning.code.includes("shared") ? "warning" : "info");
    }
    expect(planWarningView(WARNINGS[1] as PlanWarningView, dict, format).technical).toBe("21/04 distance unit");
  });

  it("decodes trigger, macro, advanced-key and dead-band values", () => {
    const decoded = (mode: "off" | "rapid_trigger" | "rapid_trigger_separate", press: number, release: number): TriggerBytesView => ({
      state: "decoded",
      value: { mode, mode_byte: 0, actuation: 85, rt_press: press, rt_release: release },
      second_travel: 85,
    });
    const sep = lang === "tr" ? "," : ".";
    expect(triggerText(decoded("rapid_trigger", 15, 15), dict, format)).toBe(`1${sep}70 mm · RT 0${sep}30 mm`);
    expect(triggerText(decoded("rapid_trigger_separate", 15, 25), dict, format)).toBe(
      `1${sep}70 mm · RT 0${sep}30 / 0${sep}50 mm`,
    );
    expect(triggerText(decoded("off", 1, 1), dict, format)).toBe(`1${sep}70 mm · ${dict.backupRestore.changes.rtOff}`);
    expect(triggerText({ state: "raw", hex: "00" }, dict, format)).toBe(dict.backupRestore.changes.raw);
    expect(triggerText(null, dict, format)).toBe("–");
    expect(macroText({ state: "steps", count: 4 }, dict)).toContain("4");
    expect(advancedText({ state: "in_use", kind: 3 }, dict)).toContain("0x03");
    expect(deadBandText({ top: 5, bottom: 10 }, dict, format)).toContain(`0${sep}20 mm`);
    expect(deadBandText(null, dict, format)).toBe(dict.backupRestore.changes.deadBandNone);
  });

  it("names condensed tables and lists keys", () => {
    expect(tableGroupName({ kind: "macros", slots: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] }, dict)).toContain("1–10");
    expect(tableGroupName({ kind: "advanced_keys", slots: [4] }, dict)).toBe(dict.flow.tables.advanced_key({ n: 4 }));
    expect(keyNamesText(["W", "A", "S", "D", "L-Shift"], dict, 3)).toBe(
      `W, A, S ${dict.common.andMore({ n: 2 })}`,
    );
  });

  it("describes the profiles of a backup", () => {
    expect(profilesLabel([1, 2, 3, 4], dict, format)).toContain("1–4");
    expect(profilesLabel([2], dict, format)).toContain("2");
    expect(profilesLabel([1, 3], dict, format)).toContain(lang === "tr" ? "1 ve 3" : "1 and 3");
  });

  it("has a headline for every recorded restore result", () => {
    for (const id of ["restore_ok_persisted", "restore_lost", "restore_maybe_lost_disconnect", "restore_cancelled_in_keymap_group"]) {
      const r = report(id);
      const text = restoreHeadline(r, tallyReport(r), dict);
      expect(text.length, id).toBeGreaterThan(10);
    }
    const ok = report("restore_ok_persisted");
    expect(restoreHeadline(ok, tallyReport(ok), dict)).toBe(dict.backupRestore.result.headline.done({ n: 3 }));
    const lost = report("restore_lost");
    expect(restoreHeadline(lost, tallyReport(lost), dict)).toBe(dict.backupRestore.result.headline.lost({ n: 1 }));
  });

  it("explains the persistence check", () => {
    const sentences = persistenceSentences({ profiles: [2, 1], persisted: 3, lost: 1, not_checked: 1 }, dict, format);
    expect(sentences).toHaveLength(4);
    // Nothing kept: no "0 tables still hold…".
    expect(persistenceSentences({ profiles: [2], persisted: 0, lost: 1, not_checked: 0 }, dict, format)).toEqual([
      dict.backupRestore.result.persistence.profiles({ list: "2", count: 1 }),
      dict.backupRestore.result.persistence.lost({ n: 1 }),
    ]);
  });
});

describe("Turkish copy", () => {
  const tr = DICTIONARIES.tr.backupRestore;
  const format = makeFormat("tr");

  it("never attaches a case suffix to a number", () => {
    const texts = [
      tr.restore.consentBody({ list: format.list(["2", "3", "4"]), count: 3, active: 1 }),
      tr.backup.reviewSwitches({ visits: "2, 3 ve 4", original: 1 }),
      tr.review.undoTitle({ list: "2", count: 1, date: "24 Eki 2026 10:30" }),
      tr.result.persistence.profiles({ list: "2 ve 1", count: 2 }),
    ];
    for (const text of texts) {
      expect(text).not.toMatch(/\d['’]/u);
    }
    expect(texts[0]).toContain("2, 3 ve 4 numaralı profiller");
    expect(texts[0]).toContain("1 numaralı profile");
  });

  it("uses no plural after a number", () => {
    expect(tr.review.confirm({ n: 3 })).toBe("3 değişikliği geri yükle");
    expect(tr.review.triggers({ n: 4 })).toBe("Aktivasyon ve Rapid Trigger: 4 tuş");
  });
});
