// Layout rules for keyboard and high-contrast users: a control that gets the
// focus is scrolled above the sticky apply bar, never under it (WCAG 2.4.11),
// and colour swatches keep their colour in Windows high contrast.

import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ApplyBar } from "../components/ApplyBar";
import componentsCss from "../components/components.css?raw";
import shellCss from "../shell/shell.css?raw";

/** The body of the first rule for `selector` in `css` (after `from`, if given). */
function rule(css: string, selector: string, from = 0): string {
  const start = css.indexOf(`${selector} {`, from);
  expect(start).toBeGreaterThanOrEqual(0);
  return css.slice(start, css.indexOf("}", start));
}

describe("the sticky apply bar", () => {
  it("tells its scroll container how much room to keep, and takes it back when it goes", () => {
    const { unmount, container } = render(
      <main className="ow-main">
        <ApplyBar summary={null} dirty={false} onDiscard={() => undefined} onApply={() => undefined} />
      </main>,
    );
    const main = container.querySelector<HTMLElement>(".ow-main");
    expect(main?.style.getPropertyValue("--applybar-height")).toMatch(/^\d+px$/);
    unmount();
    expect(main?.style.getPropertyValue("--applybar-height")).toBe("");
  });

  it("is kept clear of focused controls by the scroll container's padding", () => {
    expect(rule(shellCss, ".ow-main")).toContain(
      "scroll-padding-bottom: calc(var(--applybar-height, 0px) + var(--space-4));",
    );
  });
});

describe("Windows high contrast", () => {
  it("keeps the colour of swatches, with a visible boundary", () => {
    const media = componentsCss.indexOf("@media (forced-colors: active)");
    expect(media).toBeGreaterThanOrEqual(0);
    const body = rule(componentsCss, ".ow-swatch,\n  .ow-colorfield__swatch", media);
    expect(body).toContain("forced-color-adjust: none;");
    expect(body).toContain("border: 1px solid CanvasText;");
  });

  it("draws the state of on/off switches with system colours", () => {
    const media = componentsCss.indexOf("@media (forced-colors: active) {\n  .ow-switch {");
    expect(media).toBeGreaterThanOrEqual(0);
    expect(rule(componentsCss, ".ow-switch", media)).toContain("border-color: ButtonText;");
    const thumb = rule(componentsCss, ".ow-switch__thumb", media);
    expect(thumb).toContain("forced-color-adjust: none;");
    expect(thumb).toContain("background: ButtonText;");
    const on = rule(componentsCss, '.ow-switch[aria-checked="true"]', media);
    expect(on).toContain("background: Highlight;");
    expect(rule(componentsCss, '.ow-switch[aria-checked="true"] .ow-switch__thumb', media)).toContain(
      "background: HighlightText;",
    );
    expect(rule(componentsCss, ".ow-switch:disabled .ow-switch__thumb", media)).toContain("background: GrayText;");
  });
});
