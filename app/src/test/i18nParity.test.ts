// The Turkish dictionaries have exactly the English keys, no empty text, and
// repeat English only for allowlisted words (names, units, key legends).

import { describe, expect, it } from "vitest";

import { DICTIONARIES, resolveLang } from "../i18n";

/** Values that may be the same in both languages. */
const SAME_ALLOWED = new Set([
  "OpenWraith",
  "Rapid Trigger",
  "Fn",
  "USB",
  "Esc",
  "Escape",
  "Tab",
  "Home",
  "Enter",
  "Delete",
  "Page Up",
  "Page Down",
  "Caps Lock",
  "Model",
  "Firmware",
  "GPL-3.0-or-later",
  "English",
  "Türkçe",
  "mm",
  "Neon",
  "Radar",
  "Aurora",
  "Shift",
  "Ctrl",
  "Alt",
  "SOCD",
  "Profile ?",
  "Profil ?",
]);

/**
 * Whether a value may be identical in both languages: allowlisted words,
 * short codes and legends (no lowercase letters, e.g. "RT", "WASD", "1×"),
 * and examples such as "#ff8800".
 */
function sameAllowed(value: string): boolean {
  return SAME_ALLOWED.has(value) || !/\p{Ll}/u.test(value) || /^#[0-9a-f]{3,6}$/i.test(value);
}

type Tree = Readonly<Record<string, unknown>>;

function walk(en: Tree, tr: Tree, path: string, problems: string[]): void {
  const enKeys = Object.keys(en).sort();
  const trKeys = Object.keys(tr).sort();
  for (const key of enKeys) if (!(key in tr)) problems.push(`missing in tr: ${path}${key}`);
  for (const key of trKeys) if (!(key in en)) problems.push(`extra in tr: ${path}${key}`);
  for (const key of enKeys) {
    const a = en[key];
    const b = tr[key];
    const at = `${path}${key}`;
    if (typeof a !== typeof b) {
      problems.push(`type differs: ${at}`);
    } else if (typeof a === "string" && typeof b === "string") {
      if (b.trim() === "") problems.push(`empty in tr: ${at}`);
      if (a === b && !sameAllowed(a)) problems.push(`not translated: ${at} = ${JSON.stringify(a)}`);
    } else if (typeof a === "object" && a !== null && typeof b === "object" && b !== null) {
      walk(a as Tree, b as Tree, `${at}.`, problems);
    }
  }
}

describe("dictionaries", () => {
  it("have the same keys, and Turkish is complete and translated", () => {
    const problems: string[] = [];
    walk(DICTIONARIES.en, DICTIONARIES.tr, "", problems);
    expect(problems).toEqual([]);
  });

  it("have no empty English text", () => {
    const empty: string[] = [];
    const visit = (tree: Tree, path: string) => {
      for (const [key, value] of Object.entries(tree)) {
        if (typeof value === "string" && value.trim() === "") empty.push(`${path}${key}`);
        else if (typeof value === "object" && value !== null) visit(value as Tree, `${path}${key}.`);
      }
    };
    visit(DICTIONARIES.en, "");
    expect(empty).toEqual([]);
  });

  it("format numbers into Turkish messages without case suffixes", () => {
    const tr = DICTIONARIES.tr;
    expect(tr.shell.notices.profileChanged({ to: 3 })).toBe("Klavyede 3 numaralı profile geçildi");
    expect(tr.flow.progress.waitProfile({ n: 2 })).toContain("profil 2");
    expect(DICTIONARIES.en.flow.headline.doneKeys({ n: 1 })).toContain("1 key written");
    expect(DICTIONARIES.en.flow.headline.doneKeys({ n: 4 })).toContain("4 keys written");
  });
});

describe("resolveLang", () => {
  it("follows the operating system for `system`", () => {
    expect(resolveLang("system", "tr-TR", ["en-US"])).toBe("tr");
    expect(resolveLang("system", "en-GB", ["tr"])).toBe("en");
    expect(resolveLang("system", null, ["tr-TR"])).toBe("tr");
    expect(resolveLang("system", null, ["de-DE"])).toBe("en");
    expect(resolveLang("system", null, [])).toBe("en");
  });

  it("uses an explicit choice", () => {
    expect(resolveLang("tr", "en-US", [])).toBe("tr");
    expect(resolveLang("en", "tr-TR", [])).toBe("en");
  });
});
