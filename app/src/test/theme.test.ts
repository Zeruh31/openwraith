// Theme switching (system / light / dark), language choice from settings,
// and the hash routes.

import { afterEach, describe, expect, it, vi } from "vitest";

import { getLang, setLang } from "../i18n";
import { href, parseRoute } from "../routes";
import { appInfoStore, DEFAULT_APP_INFO } from "../state/appInfo";
import { applySettings } from "../state/settings";
import { applyTheme, resolvedTheme } from "../theme/theme";

afterEach(() => {
  document.documentElement.removeAttribute("data-theme");
  appInfoStore.set(DEFAULT_APP_INFO);
  setLang("en");
  vi.unstubAllGlobals();
});

function mockSystemDark(dark: boolean): void {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: dark && query.includes("dark"),
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
}

describe("theme", () => {
  it("sets and clears the override attribute", () => {
    applyTheme("dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    applyTheme("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    applyTheme("system");
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });

  it("follows the system unless overridden", () => {
    mockSystemDark(true);
    applyTheme("system");
    expect(resolvedTheme()).toBe("dark");
    applyTheme("light");
    expect(resolvedTheme()).toBe("light");
    mockSystemDark(false);
    applyTheme("system");
    expect(resolvedTheme()).toBe("light");
    applyTheme("dark");
    expect(resolvedTheme()).toBe("dark");
  });
});

describe("settings", () => {
  it("apply the theme and the language (system follows the OS locale)", () => {
    appInfoStore.set({ ...DEFAULT_APP_INFO, os_locale: "tr-TR" });
    applySettings({ language: "system", theme: "dark" });
    expect(getLang()).toBe("tr");
    expect(document.documentElement.lang).toBe("tr");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    applySettings({ language: "en", theme: "system" });
    expect(getLang()).toBe("en");
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });
});

describe("routes", () => {
  it("parse pages and parameters", () => {
    expect(parseRoute("#/backup?restore=f-1")).toEqual({ page: "backup", params: { restore: "f-1" } });
    expect(parseRoute("#/lighting")).toEqual({ page: "lighting", params: {} });
    expect(parseRoute("")).toEqual({ page: "device", params: {} });
    expect(parseRoute("#/nowhere")).toEqual({ page: "device", params: {} });
  });

  it("build hashes", () => {
    expect(href("settings")).toBe("#/settings");
    expect(href({ page: "backup", params: { restore: "f-2" } })).toBe("#/backup?restore=f-2");
  });
});
