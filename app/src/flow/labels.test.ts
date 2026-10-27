// Text for every write warning, in English and Turkish: nothing empty, and
// the SOCD switch on the keyboard says whether SOCD is on now.

import { describe, expect, it } from "vitest";

import { DICTIONARIES } from "../i18n";
import { makeFormat } from "../i18n/format";
import type { WriteWarningView } from "../ipc/bindings";
import { warningText } from "./labels";

const LANGS = ["en", "tr"] as const;

const WARNINGS: WriteWarningView[] = [
  { code: "ack_timeout", packet: 2, of: 10, send: 1 },
  { code: "optional_ack_missing", packet: 1 },
  { code: "verify_reread" },
  { code: "retried" },
  { code: "select_ack_missing", profile: 2 },
  { code: "profile_push_missing", profile: 2 },
  { code: "select_retried", profile: 2 },
  { code: "waited_for_late_ack" },
  { code: "waited_for_commit", waited_ms: 1500 },
  { code: "base_keymap_not_saved" },
  { code: "unexpected_ack", packet: 1 },
  { code: "socd_toggled_on_keyboard", power: true },
];

describe.each(LANGS)("write warnings (%s)", (lang) => {
  const dict = DICTIONARIES[lang];
  const format = makeFormat(lang);

  it("explains every write warning", () => {
    for (const warning of WARNINGS) {
      const text = warningText(warning, dict, format);
      expect(text.trim().length, warning.code).toBeGreaterThan(10);
      expect(text).not.toContain("undefined");
    }
  });

  it("says whether SOCD is on after a switch on the keyboard", () => {
    const on = warningText({ code: "socd_toggled_on_keyboard", power: true }, dict, format);
    const off = warningText({ code: "socd_toggled_on_keyboard", power: false }, dict, format);
    expect(on).not.toBe(off);
    expect(on).toContain(lang === "en" ? "now on" : "açık");
    expect(off).toContain(lang === "en" ? "now off" : "kapalı");
  });
});
