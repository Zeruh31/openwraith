// "Share a profile" logic: the parts an import starts with, the intents, the
// switches they need, the Fn built-in a file replaces, and the texts of
// macro steps, refusals and the import's review and report (EN and TR).

import { describe, expect, it } from "vitest";

import { DICTIONARIES } from "../../../i18n";
import { makeFormat } from "../../../i18n/format";
import type { ImportSummaryView, RestoreReportView } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import { tallyReport } from "../model";
import { restoreHeadline, skipReasonText } from "../texts";
import { criticalTexts } from "./ImportFlow";
import {
  buildExportIntent,
  buildImportIntent,
  builtinOf,
  chosenParts,
  defaultFrom,
  defaultInto,
  editProblemText,
  exportLabel,
  exportNeedsSwitch,
  importSwitches,
  includeOf,
  initialInclude,
  macroStepText,
  noParts,
  partsText,
  withPart,
} from "./model";

const en = DICTIONARIES.en;
const tr = DICTIONARIES.tr;
const fmtEn = makeFormat("en");
const fmtTr = makeFormat("tr");

/** The recorded summary of the shared profile file (macro 2 types A). */
function recordedSummary(): ImportSummaryView {
  const call = FIXTURES["import_summary"]?.calls.find((c) => c.command === "open_import_file");
  if (call?.command !== "open_import_file" || call.ok === null) throw new Error("no recorded summary");
  return call.ok;
}

function recordedImportReport(): RestoreReportView {
  const call = FIXTURES["import_apply_ok"]?.calls.find((c) => c.command === "apply_import");
  if (call?.command !== "apply_import" || call.ok === null) throw new Error("no recorded import report");
  return call.ok;
}

describe("parts", () => {
  it("starts with the backend's defaults, limited to what the file holds", () => {
    const summary = recordedSummary();
    expect(chosenParts(initialInclude(summary))).toEqual(["triggers", "lighting"]);
    const lacking: ImportSummaryView = {
      ...summary,
      contents: { ...summary.contents, present: { ...summary.contents.present, lighting: false } },
    };
    expect(chosenParts(initialInclude(lacking))).toEqual(["triggers"]);
  });

  it("drops a part the file lacks from the choice", () => {
    const present = { ...noParts(), triggers: true, keys: true };
    const chosen = withPart(withPart(noParts(), "keys", true), "socd", true);
    expect(includeOf(chosen, present)).toEqual({ ...noParts(), keys: true });
  });

  it("lists the parts in words, in list order", () => {
    const parts = chosenParts({ ...noParts(), socd: true, triggers: true });
    expect(parts).toEqual(["triggers", "socd"]);
    expect(partsText(parts, en)).toBe("Actuation and Rapid Trigger; SOCD key pairs");
    expect(partsText(parts, tr)).toBe("Aktivasyon ve Rapid Trigger; SOCD tuş çiftleri");
  });
});

describe("profiles and switches", () => {
  it("offers the file's own profile and the active keyboard profile first", () => {
    const summary = recordedSummary();
    expect(defaultFrom(summary)).toBe(1);
    expect(defaultFrom({ ...summary, profile: null, file: { ...summary.file, profiles: [3, 2] } })).toBe(2);
    expect(defaultInto(3)).toBe(3);
    expect(defaultInto(null)).toBe(1);
  });

  it("switches only to a profile that isn't the active one", () => {
    expect(importSwitches(2, 2)).toEqual([]);
    expect(importSwitches(2, 1)).toEqual([2]);
    expect(importSwitches(1, null)).toEqual([1]);
    expect(exportNeedsSwitch(1, 1)).toBe(false);
    expect(exportNeedsSwitch(3, 1)).toBe(true);
    expect(exportNeedsSwitch(1, null)).toBe(true);
  });
});

describe("intents", () => {
  it("builds the import intent; the file's own profile is sent as null", () => {
    const include = { ...noParts(), triggers: true, keys: true };
    const intent = buildImportIntent({
      source: "f-1",
      from: 1,
      fileProfile: 1,
      into: 2,
      include,
      allowedCritical: [],
      allowFirmware: true,
      switches: [2, 2],
    });
    expect(intent).toEqual({
      source: "f-1",
      from: null,
      into: 2,
      include,
      allowed_critical_keys: [],
      allow_firmware_mismatch: true,
      confirmed_switches: [2],
    });
    expect(intent.include).not.toBe(include);
    const other = buildImportIntent({
      source: "f-1",
      from: 3,
      fileProfile: 1,
      into: 1,
      include,
      allowedCritical: [116, 24, 24],
      allowFirmware: false,
      switches: [],
    });
    expect(other).toMatchObject({ from: 3, allowed_critical_keys: [24, 116], confirmed_switches: [] });
  });

  it("builds the export intent from the keyboard, with a trimmed name or none", () => {
    expect(buildExportIntent({ profile: 3, name: "  Tournament ", destination: "f-2", confirmedSwitch: true })).toEqual({
      source: { source: "keyboard" },
      profile: 3,
      label: "Tournament",
      destination: "f-2",
      confirmed_switch: true,
    });
    expect(exportLabel("   ")).toBeNull();
    expect(exportLabel("")).toBeNull();
  });
});

describe("the keyboard's own Fn shortcuts", () => {
  it("knows the profile switches and the reset", () => {
    expect(builtinOf({ name: "2" })).toEqual({ kind: "profile", k: 2 });
    expect(builtinOf({ name: "4" })).toEqual({ kind: "profile", k: 4 });
    expect(builtinOf({ name: "Space" })).toEqual({ kind: "reset" });
    expect(builtinOf({ name: "5" })).toEqual({ kind: "other" });
    expect(builtinOf({ name: "F1" })).toEqual({ kind: "other" });
  });

  it("names the consequence for the target profile, and that replacing the reset is untested", () => {
    const c = en.backupRestore.share.critical;
    expect(criticalTexts({ name: "2" }, 3, en)).toEqual({
      body: c.profile({ k: 2, n: 3 }),
      confirm: "Import it anyway: Fn + 2 will no longer switch to profile 2 while profile 3 is active",
    });
    expect(criticalTexts({ name: "Space" }, 1, en).body).toContain("has not been tested");
    expect(criticalTexts({ name: "Space" }, 1, tr).confirm).toBe(
      "Yine de içe aktar: 1 numaralı profil etkinken Fn + Boşluk artık klavyeyi sıfırlamayacak",
    );
    expect(criticalTexts({ name: "Esc" }, 2, en).confirm).toContain("Fn + Esc");
  });

  it("says that Fn + k on profile k only switches to the same profile", () => {
    for (const dict of [en, tr]) {
      const same = criticalTexts({ name: "2" }, 2, dict);
      const other = criticalTexts({ name: "2" }, 3, dict);
      expect(same.body).not.toBe(other.body);
      expect(same.confirm).not.toBe(other.confirm);
      expect(same.body).not.toContain(dict === en ? "while profile 2 is active" : "profil etkinken");
    }
    expect(criticalTexts({ name: "2" }, 2, en).confirm).toBe(
      "Import it anyway: replace Fn + 2 on profile 2 (it only switches to this same profile)",
    );
  });
});

describe("texts", () => {
  it("says each macro step with its key and delay", () => {
    const [macro] = recordedSummary().contents.macros;
    const steps = macro?.program?.steps ?? [];
    expect(steps.map((step) => macroStepText(step, en, fmtEn))).toEqual(["Press A, then wait 20 ms", "Release A"]);
    expect(steps.map((step) => macroStepText(step, tr, fmtTr))).toEqual([
      "A tuşuna basılır, ardından 20 ms beklenir",
      "A tuşu bırakılır",
    ]);
    expect(macroStepText({ usage: 0x04, motion: "press", delay_after_ms: 1500 }, en, fmtEn)).toBe(
      "Press A, then wait 1,500 ms",
    );
  });

  it("explains the share's refusals", () => {
    expect(editProblemText({ problem: "no_changes" }, en)).toBe(en.backupRestore.share.refusals.no_changes);
    expect(editProblemText({ problem: "unsupported_current_table", table: { table: "socd" }, reason: "x" }, en)).toContain(
      "OpenWraith can't import",
    );
    expect(editProblemText({ problem: "knob" }, tr)).toBe(tr.backupRestore.share.refusals.other);
  });

  it("uses import words in the review and the report", () => {
    const reason = { code: "keymap_group_blocked", cause: { code: "critical_fn_override", index: 24 } } as const;
    expect(skipReasonText(reason, en, fmtEn, "import")).toMatch(/^Key assignments, macros and advanced keys can only be imported together/u);
    expect(skipReasonText(reason, en, fmtEn)).toMatch(/can only be restored together/u);
    const report = recordedImportReport();
    const tally = tallyReport(report);
    expect(restoreHeadline(report, tally, en, "import")).toBe(en.backupRestore.share.result.headline.done({ n: tally.total }));
    expect(restoreHeadline(report, tally, tr, "import")).toMatch(/^İçe aktarma tamamlandı/u);
    expect(restoreHeadline(report, tally, en)).toMatch(/^Restore complete/u);
  });
});
