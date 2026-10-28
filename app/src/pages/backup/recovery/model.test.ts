// The Recovery card's logic on the views the core recorded on the simulator:
// which profiles look reset, the comparisons the card can start (one
// profile, every profile from its newest automatic backup), their intents,
// "Restore anyway" after a firmware change, and the evidence in plain words
// (EN and TR, no empty text, Turkish without case suffixes on numbers).

import { describe, expect, it } from "vitest";

import { DICTIONARIES } from "../../../i18n";
import { makeFormat } from "../../../i18n/format";
import type { RecoveryProfileView, RecoveryView, ResetEvidenceView } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import {
  ageText,
  backupOptionText,
  buildRecoveryIntent,
  chosenBackup,
  comparedText,
  evidenceText,
  heldProfiles,
  keepBlock,
  knownFirmware,
  needsFirmwareTick,
  profileTitle,
  restoreAllRequest,
  singleRequest,
  stateNote,
  summarySentences,
  tablesText,
} from "./model";

const en = DICTIONARIES.en;
const tr = DICTIONARIES.tr;
const fmtEn = makeFormat("en");
const LANGS = ["en", "tr"] as const;

/** The `index`-th recorded `get_recovery` of a fixture. */
function recorded(fixtureId: string, index = 0): RecoveryView {
  const call = FIXTURES[fixtureId]?.calls.filter((c) => c.command === "get_recovery")[index];
  if (call?.command !== "get_recovery" || call.ok === null) throw new Error(`no recovery view in ${fixtureId}`);
  return call.ok;
}

function profileOf(view: RecoveryView, n: number): RecoveryProfileView {
  const profile = view.profiles.find((p) => p.profile === n);
  if (profile === undefined) throw new Error(`no profile ${n}`);
  return profile;
}

const EVIDENCE: ResetEvidenceView[] = [
  { evidence: "reset_seen" },
  {
    evidence: "firmware_changed",
    known: { version: "3.22.07", build_date: null },
    current: { version: "3.23.01", build_date: null },
  },
  { evidence: "triggers_to_factory" },
  { evidence: "keymap_group_to_factory", tables: ["base_keymap", "macros"] },
];

describe("the comparisons the card offers", () => {
  it("knows which profiles look reset", () => {
    expect(heldProfiles(recorded("recovery_reset_suspected"))).toEqual([1]);
    expect(heldProfiles(recorded("recovery_nothing_wrong"))).toEqual([]);
    expect(heldProfiles(recorded("recovery_restore_ok", 1))).toEqual([]);
  });

  it("offers “It's fine, keep them” unless a reconnection or a comparison must come first", () => {
    expect(keepBlock(true, 1, false)).toBe("reconnect");
    expect(keepBlock(true, 0, true)).toBe("reconnect");
    // Nothing held while profiles were not compared since the report.
    expect(keepBlock(false, 0, true)).toBe("compare");
    // Something held: kept (the report may stay for the others).
    expect(keepBlock(false, 2, true)).toBeNull();
    // Nothing held, everything compared: it acknowledges the report.
    expect(keepBlock(false, 0, false)).toBeNull();
  });

  it("compares one profile with the automatic backup picked, else its newest", () => {
    const profile = profileOf(recorded("recovery_keep_current"), 1);
    const newest = profile.backups[0];
    if (newest === undefined) throw new Error("no backup");
    expect(chosenBackup(profile, undefined)).toBe(newest);
    expect(chosenBackup(profile, "f-unknown")).toBe(newest);
    expect(chosenBackup(profileOf(recorded("recovery_keep_current"), 2), undefined)).toBeNull();
    const request = singleRequest(1, newest);
    expect(request).toEqual({ profiles: [1], files: ["f-90f303b6f8f26cf4"] });
    // The intent the recorded comparison was made with.
    const call = FIXTURES["recovery_restore_ok"]?.calls.find((c) => c.command === "prepare_recovery");
    if (call?.command !== "prepare_recovery") throw new Error("no recorded comparison");
    expect(buildRecoveryIntent(request, false, [])).toEqual(call.args.intent);
  });

  it("restores every profile from its newest automatic backup in one request", () => {
    const view = recorded("recovery_restore_all_preview");
    const all = restoreAllRequest(view);
    expect(all?.profiles).toEqual([1, 2, 3, 4]);
    const calls = FIXTURES["recovery_restore_all_preview"]?.calls.filter((c) => c.command === "prepare_recovery") ?? [];
    const [refused, agreed] = calls;
    if (refused?.command !== "prepare_recovery" || agreed?.command !== "prepare_recovery" || all === null) {
      throw new Error("no recorded comparisons");
    }
    expect(all.files).toEqual(refused.args.intent.files);
    expect(buildRecoveryIntent(all, false, [4, 2, 3, 2])).toEqual(agreed.args.intent);
    // Not offered when the backend says it isn't possible.
    expect(restoreAllRequest({ ...view, can_restore_all: false })).toBeNull();
    expect(restoreAllRequest(recorded("recovery_reset_suspected"))).toBeNull();
  });

  it("needs “Restore anyway” after a firmware change, or after the backend refused one", () => {
    const changed = recorded("recovery_firmware_changed");
    expect(needsFirmwareTick(changed, [1], false)).toBe(true);
    expect(needsFirmwareTick(changed, [2], false)).toBe(false);
    const same = recorded("recovery_reset_suspected");
    expect(needsFirmwareTick(same, [1], false)).toBe(false);
    expect(needsFirmwareTick(same, [1], true)).toBe(true);
    expect(knownFirmware(changed)?.version).toBe("3.22.07");
    expect(knownFirmware(same)).toBeNull();
    // The profile's own evidence names it too.
    expect(knownFirmware({ ...changed, firmware_changed: null })?.version).toBe("3.22.07");
  });
});

describe.each(LANGS)("the card's words (%s)", (lang) => {
  const dict = DICTIONARIES[lang];
  const format = makeFormat(lang);

  it("explains every kind of evidence", () => {
    for (const evidence of EVIDENCE) {
      const text = evidenceText(evidence, dict, format);
      expect(text.trim().length, evidence.evidence).toBeGreaterThan(10);
      expect(text).not.toContain("undefined");
    }
    expect(evidenceText(EVIDENCE[1] ?? { evidence: "reset_seen" }, dict, format)).toContain("3.23.01");
    expect(evidenceText(EVIDENCE[3] ?? { evidence: "reset_seen" }, dict, format)).toContain(
      dict.backupRestore.recovery.tables.macros,
    );
  });

  it("says what happened first: the reset report, the firmware change, the profiles that look reset", () => {
    const sentences = summarySentences(recorded("recovery_reset_suspected"), dict, format);
    expect(sentences).toHaveLength(2);
    expect(sentences[1]).toBe(dict.backupRestore.recovery.heldProfiles({ list: "1", count: 1 }));
    const firmware = summarySentences(recorded("recovery_firmware_changed"), dict, format);
    expect(firmware[0]).toContain("3.22.07");
    expect(firmware[0]).toContain("3.23.01");
    expect(summarySentences(recorded("recovery_nothing_wrong"), dict, format)).toEqual([]);
  });

  it("describes every profile state and the backup a reset-like profile was compared with", () => {
    const held = profileOf(recorded("recovery_reset_suspected"), 1);
    expect(stateNote(held, dict, format)).toBeNull();
    expect(comparedText(held, dict, format)).toContain(format.dateTime("2026-10-24T07:30:00Z"));
    expect(tablesText(held.factory_now, dict, format)).toBe(
      format.list([dict.backupRestore.recovery.tables.triggers, dict.backupRestore.recovery.tables.main_light]),
    );
    const view = recorded("recovery_nothing_wrong");
    expect(stateNote(profileOf(view, 1), dict, format)).toContain(format.dateTime("2026-10-24T07:30:00Z"));
    expect(stateNote(profileOf(view, 2), dict, format)).toBe(dict.backupRestore.recovery.notes.no_known_good);
    expect(stateNote({ ...profileOf(view, 1), state: "not_checked" }, dict, format)).toBe(
      dict.backupRestore.recovery.notes.not_checked,
    );
    for (const state of ["no_known_good", "not_checked", "fine", "reset_suspected"] as const) {
      expect(dict.backupRestore.recovery.states[state].length).toBeGreaterThan(3);
    }
  });

  it("names a profile with its local name and an automatic backup with its time and reason", () => {
    expect(profileTitle({ profile: 2, name: "FPS" }, dict)).toBe(
      dict.backupRestore.recovery.profileNamed({ n: 2, name: "FPS" }),
    );
    expect(profileTitle({ profile: 2, name: "  " }, dict)).toBe(dict.common.profile({ n: 2 }));
    const entry = profileOf(recorded("recovery_keep_current"), 1).backups[0];
    if (entry === undefined) throw new Error("no backup");
    const text = backupOptionText({ ...entry, pinned: true }, dict, format);
    expect(text).toContain(dict.backupRestore.autoBackups.reasons.first_read);
    expect(text).toContain(format.dateTimeSeconds(entry.created_at));
    expect(text).not.toBe(backupOptionText(entry, dict, format));
  });
});

describe("ages", () => {
  it("rounds down to minutes, hours and days", () => {
    expect(ageText(0, en)).toBe("less than a minute");
    expect(ageText(59, en)).toBe("less than a minute");
    expect(ageText(60, en)).toBe("1 minute");
    expect(ageText(150, en)).toBe("2 minutes");
    expect(ageText(3600, en)).toBe("1 hour");
    expect(ageText(86399, en)).toBe("23 hours");
    expect(ageText(3 * 86400 + 5, en)).toBe("3 days");
    expect(ageText(-5, en)).toBe("less than a minute");
    expect(ageText(2 * 86400, tr)).toBe("2 gün");
    const held = profileOf(recorded("recovery_reset_suspected"), 1);
    expect(comparedText({ ...held, known_good_age_s: 7200 }, en, fmtEn)).toContain("made 2 hours before the comparison");
    expect(comparedText({ ...held, known_good_at: null }, en, fmtEn)).toBeNull();
  });
});
