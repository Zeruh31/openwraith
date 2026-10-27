// The theme tokens meet WCAG contrast in both themes: 4.5:1 for text, 3:1
// for control boundaries and focus.

import { describe, expect, it } from "vitest";

import { contrastRatio } from "../format/color";
import css from "../theme/tokens.css?raw";

function block(selector: string): Record<string, string> {
  const start = css.indexOf(`${selector} {`);
  expect(start).toBeGreaterThanOrEqual(0);
  const body = css.slice(start, css.indexOf("}", start));
  const tokens: Record<string, string> = {};
  for (const match of body.matchAll(/--([a-z0-9-]+):\s*(#[0-9a-f]{6});/gi)) {
    const [, name, value] = match;
    if (name !== undefined && value !== undefined) tokens[name] = value.toLowerCase();
  }
  return tokens;
}

const THEMES = {
  light: block(":root"),
  dark: block(':root[data-theme="dark"]'),
  systemDark: block(':root:not([data-theme="light"])'),
};

const TEXT_PAIRS: [string, string][] = [
  ["text", "bg"],
  ["text", "surface"],
  ["text", "surface-2"],
  ["text-muted", "bg"],
  ["text-muted", "surface"],
  ["text-muted", "surface-2"],
  ["accent", "surface"],
  ["accent", "bg"],
  ["on-accent", "accent"],
  ["success", "surface"],
  ["success", "success-bg"],
  ["warning-fg", "warning-bg"],
  ["warning-fg", "surface"],
  ["danger", "surface"],
  ["danger", "danger-bg"],
  ["sim", "surface"],
  ["sim", "sim-bg"],
  ["text", "info-bg"],
  ["text", "warning-bg"],
  ["text", "danger-bg"],
  ["text", "success-bg"],
  ["text", "sim-bg"],
  ["key-text", "key-bg"],
  ["key-text", "key-selected-bg"],
  ["accent", "accent-weak"],
];

const UI_PAIRS: [string, string][] = [
  ["control-border", "surface"],
  ["control-border", "bg"],
  ["focus", "surface"],
  ["focus", "bg"],
  ["accent", "key-selected-bg"],
  ["accent", "surface-2"],
];

describe("theme tokens", () => {
  it("define the same tokens in every theme", () => {
    expect(Object.keys(THEMES.dark).sort()).toEqual(Object.keys(THEMES.light).sort());
    expect(THEMES.systemDark).toEqual(THEMES.dark);
  });

  for (const [theme, tokens] of Object.entries(THEMES)) {
    it(`meet text contrast in ${theme}`, () => {
      const failing = TEXT_PAIRS.map(([fg, bg]) => {
        const ratio = contrastRatio(tokens[fg] ?? "", tokens[bg] ?? "");
        return { pair: `${fg} on ${bg}`, ratio: Math.round(ratio * 100) / 100 };
      }).filter((r) => r.ratio < 4.5);
      expect(failing).toEqual([]);
    });

    it(`meet UI contrast in ${theme}`, () => {
      const failing = UI_PAIRS.map(([fg, bg]) => {
        const ratio = contrastRatio(tokens[fg] ?? "", tokens[bg] ?? "");
        return { pair: `${fg} on ${bg}`, ratio: Math.round(ratio * 100) / 100 };
      }).filter((r) => r.ratio < 3);
      expect(failing).toEqual([]);
    });
  }
});
