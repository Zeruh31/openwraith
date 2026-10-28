// Words for key-assignment refusals, write reasons and the edit report's new
// parts, in both languages.

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { describeDetail, keymapProblemText, keyRefText, writeReasonTexts } from "../flow/labels";
import { EditReportView } from "../flow/Report";
import { DICTIONARIES, setLang } from "../i18n";
import { makeFormat } from "../i18n/format";
import type { EditReportView as EditReport, KeymapProblemView, KeyRef } from "../ipc/bindings";
import { FIXTURES } from "../preview/registry";

const en = DICTIONARIES.en;
const tr = DICTIONARIES.tr;
const F1: KeyRef = { index: 2, row: 0, col: 2, name: "F1" };
const Q: KeyRef = { index: 45, row: 2, col: 1, name: "Q" };

const PROBLEMS: KeymapProblemView[] = [
  { problem: "no_changes" },
  { problem: "too_many_changes", max: 200 },
  { problem: "not_a_key", index: 200 },
  { problem: "slot_out_of_range", kind: "macro", slot: 12 },
  { problem: "not_offered", what: "media", value: 0x40 },
  { problem: "invalid_combo", reason: "x" },
  { problem: "macro_too_long", steps: 41, max: 40 },
  { problem: "macro_unbalanced", step: 3, reason: "x" },
  { problem: "macro_last_delay" },
  { problem: "macro_times_zero" },
  { problem: "dks_shape" },
  { problem: "dks_point", value: 35, max: 30 },
  { problem: "dks_steps", row: 1, reason: "x" },
  { problem: "mt_hold_time", value_ms: 1100, min_ms: 150, max_ms: 1000 },
  { problem: "swap_not_writable", layer: "base", key: Q },
  { problem: "knob" },
  { problem: "fn_key" },
  { problem: "fn_locked", key: Q },
  { problem: "builtin_not_acknowledged", key: F1, function: "media player", critical: false },
  { problem: "no_usage", key: Q },
  { problem: "unknown_model" },
  { problem: "no_free_slot", kind: "advanced" },
  { problem: "slot_already_bound", slot: { table: "macro", slot: 2 }, by: { layer: "fn", key: Q } },
  { problem: "binding_to_empty_slot", binding: { layer: "base", key: Q }, slot: { table: "macro", slot: 5 } },
  { problem: "slot_still_referenced", slot: { table: "advanced_key", slot: 0 }, by: [{ layer: "base", key: Q }] },
  { problem: "new_slot_replaces_edit", layer: "fn", key: Q, slot: { table: "macro", slot: 2 } },
  { problem: "unsupported_current_table", table: { table: "macro", slot: 4 }, reason: "x" },
  { problem: "other", message: "x" },
];

beforeEach(() => {
  setLang("en");
});

afterEach(() => {
  cleanup();
});

describe("key-assignment refusals", () => {
  it("are explained in both languages, naming keys with their layer and number", () => {
    for (const dict of [en, tr]) {
      for (const problem of PROBLEMS) expect(keymapProblemText(problem, dict).length).toBeGreaterThan(10);
    }
    expect(keyRefText(F1, "fn", en)).toBe("Fn + F1 (#2)");
    expect(keyRefText(Q, "base", tr)).toBe("Q (#45)");
    expect(keymapProblemText({ problem: "builtin_not_acknowledged", key: F1, function: "media player", critical: false }, en)).toBe(
      en.errors.keymap.builtin_not_acknowledged({ key: "Fn + F1 (#2)", function: "Media player" }),
    );
    expect(keymapProblemText({ problem: "dks_point", value: 35, max: 30 }, tr)).toContain("3,0 mm");
    expect(keymapProblemText({ problem: "slot_out_of_range", kind: "macro", slot: 12 }, en)).toContain("Macro 13");
    expect(
      describeDetail({ detail: "keymap", problem: { problem: "fn_key" } }, en, makeFormat("en")),
    ).toBe(en.errors.keymap.fn_key);
  });
});

describe("write reasons", () => {
  it("come from the write's own fields", () => {
    const none = { commit_trigger: null, resent_unchanged: false, resent: null };
    expect(writeReasonTexts(none, en)).toEqual([]);
    expect(writeReasonTexts({ ...none, commit_trigger: "base_keymap", resent_unchanged: true }, en)).toEqual([
      en.flow.writeReasons.baseKeymapUnchanged,
    ]);
    expect(writeReasonTexts({ ...none, commit_trigger: "advanced_keys" }, en)).toEqual([
      en.flow.writeReasons.advancedKeysChanged,
    ]);
    expect(
      writeReasonTexts({ ...none, resent: "macro_slot0_clears", follows_macro_slot_0: true, completes_binding: true }, en),
    ).toEqual([en.flow.resent.macro_slot0_clears, en.flow.writeReasons.completesBinding]);
    expect(writeReasonTexts({ ...none, follows_macro_slot_0: true, clears: true }, en)).toEqual([
      en.flow.writeReasons.followsMacroSlot0,
      en.flow.writeReasons.clears,
    ]);
  });
});

describe("the edit report", () => {
  function recordedReport(id: string): EditReport {
    const call = FIXTURES[id]?.calls.find((c) => c.command === "apply_keymap_edit");
    if (call?.command !== "apply_keymap_edit" || call.ok === null) throw new Error(`no report in ${id}`);
    return structuredClone(call.ok);
  }

  it("says when Stop came but every remaining write was needed", () => {
    const report = { ...recordedReport("keys_apply_ok"), cancel_requested: true, outcome: "completed" as const };
    render(<EditReportView report={report} />);
    expect(screen.getByText(en.flow.report.cancelRequested)).toBeTruthy();
  });

  it("shows nothing extra for an ordinary edit", () => {
    render(<EditReportView report={recordedReport("keys_apply_ok")} />);
    expect(screen.queryByText(en.flow.report.cancelRequested)).toBeNull();
    expect(screen.queryByText(en.flow.report.clearedTitle)).toBeNull();
    expect(screen.queryByText(en.flow.report.persistenceTitle)).toBeNull();
    expect(screen.getByText(en.flow.footer.safetyBackup)).toBeTruthy();
  });
});
