// Tabs and drafts: `?tab=` routes, drafts per page and tab with counts, the
// nav dot (any tab of a page), the dot per tab, and the profile-switch
// confirmation that lists unwritten drafts per tab ("Keys > Macros:
// 1 unwritten change").

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { PageHeader } from "../components/Layout";
import { DICTIONARIES, setLang } from "../i18n";
import type { DeviceStatus } from "../ipc/bindings";
import { hasTabs, href, PAGE_TABS, parseRoute, tabOf } from "../routes";
import { draftText, ProfileSwitcher } from "../shell/Header";
import { Nav } from "../shell/Nav";
import { PageTabs, TabbedPage } from "../shell/PageTabs";
import {
  discardDraftOf,
  discardDrafts,
  draftEntries,
  draftPages,
  registerDraft,
  useDraft,
} from "../state/drafts";
import { acknowledgeVendor, resetSession } from "../state/session";
import { INITIAL_STATUS, statusStore } from "../state/status";

const en = DICTIONARIES.en;
const tr = DICTIONARIES.tr;

const READY: DeviceStatus = {
  ...INITIAL_STATUS,
  revision: 10,
  generation: 1,
  connection: { state: "ready" },
  active_profile: 1,
  firmware_tested: true,
  write_gate: { allowed: true, reasons: [] },
  snapshot_revision: 1,
};

function Draft(props: { page: "keys" | "actuation"; count: boolean | number; tab?: string; discard?: () => void }) {
  useDraft(props.page, props.count, props.discard ?? (() => undefined), props.tab);
  return null;
}

beforeEach(() => {
  setLang("en");
  resetSession();
  acknowledgeVendor();
  window.location.hash = "";
});

afterEach(() => {
  cleanup();
  clearMocks();
  discardDrafts();
  statusStore.set(INITIAL_STATUS);
  window.location.hash = "";
});

describe("routes with tabs", () => {
  it("parse the Keys page and its tab", () => {
    const route = parseRoute("#/keys?tab=macros");
    expect(route.page).toBe("keys");
    expect(tabOf(route, "keys")).toBe("macros");
  });

  it("fall back to the first tab for a missing or unknown tab", () => {
    expect(tabOf(parseRoute("#/keys"), "keys")).toBe("assign");
    expect(tabOf(parseRoute("#/keys?tab=nope"), "keys")).toBe("assign");
    expect(tabOf(parseRoute("#/actuation?tab=socd"), "actuation")).toBe("socd");
    expect(tabOf(parseRoute("#/lighting?tab=colors"), "lighting")).toBe("colors");
  });

  it("list the tabs of each page with tabs", () => {
    expect(PAGE_TABS.keys).toEqual(["assign", "macros", "advanced"]);
    expect(PAGE_TABS.actuation).toEqual(["trigger", "deadband", "socd", "depth"]);
    expect(PAGE_TABS.lighting).toEqual(["effect", "colors"]);
    expect(hasTabs("keys")).toBe(true);
    expect(hasTabs("backup")).toBe(false);
    expect(href({ page: "keys", params: { tab: "advanced" } })).toBe("#/keys?tab=advanced");
  });
});

describe("drafts per tab", () => {
  it("register one entry per tab with its count; the page has drafts while any tab has one", () => {
    const { rerender } = render(
      <>
        <Draft page="keys" tab="macros" count={1} />
        <Draft page="keys" tab="assign" count={true} />
      </>,
    );
    expect(draftPages()).toEqual(["keys"]);
    expect(draftEntries()).toEqual([
      { page: "keys", tab: "macros", count: 1 },
      { page: "keys", tab: "assign", count: null },
    ]);
    rerender(
      <>
        <Draft page="keys" tab="macros" count={3} />
        <Draft page="keys" tab="assign" count={0} />
      </>,
    );
    expect(draftEntries()).toEqual([{ page: "keys", tab: "macros", count: 3 }]);
    rerender(<Draft page="keys" tab="macros" count={0} />);
    expect(draftPages()).toEqual([]);
  });

  it("discard every tab of a page, or one tab", () => {
    const macros = vi.fn();
    const assign = vi.fn();
    render(
      <>
        <Draft page="keys" tab="macros" count={1} discard={macros} />
        <Draft page="keys" tab="assign" count={2} discard={assign} />
      </>,
    );
    discardDraftOf("keys", "assign");
    expect(assign).toHaveBeenCalledTimes(1);
    expect(macros).not.toHaveBeenCalled();
    discardDrafts(["keys"]);
    expect(macros).toHaveBeenCalledTimes(1);
    expect(assign).toHaveBeenCalledTimes(2);
  });

  it("keep module-store drafts working (page-wide, replaced by a new registration)", () => {
    const first = registerDraft("actuation", () => undefined);
    expect(draftEntries()).toEqual([{ page: "actuation", tab: null, count: null }]);
    const second = registerDraft("actuation", () => undefined, { tab: "deadband", count: 2 });
    expect(draftEntries()).toHaveLength(2);
    first();
    second();
    expect(draftPages()).toEqual([]);
  });

  it("are named per tab in both languages", () => {
    expect(draftText({ page: "keys", tab: "macros", count: 1 }, en)).toBe("Keys > Macros: 1 unwritten change");
    expect(draftText({ page: "keys", tab: "assign", count: 3 }, en)).toBe("Keys > Assignments: 3 unwritten changes");
    expect(draftText({ page: "actuation", tab: null, count: null }, en)).toBe("Actuation: unwritten changes");
    expect(draftText({ page: "keys", tab: "macros", count: 1 }, tr)).toBe("Tuşlar > Makrolar: 1 yazılmamış değişiklik");
  });
});

describe("the nav and the tab bar", () => {
  it("show the Keys page and a dot when any of its tabs has a draft", () => {
    render(
      <>
        <Nav />
        <Draft page="keys" tab="advanced" count={1} />
      </>,
    );
    const nav = screen.getByRole("navigation", { name: en.shell.nav.label });
    const keys = within(nav).getByRole("link", { name: new RegExp(en.shell.nav.keys) });
    expect(keys.getAttribute("href")).toBe("#/keys");
    expect(within(keys).getByText(en.shell.nav.unwritten)).toBeTruthy();
    const actuation = within(nav).getByRole("link", { name: new RegExp(en.shell.nav.actuation) });
    expect(within(actuation).queryByText(en.shell.nav.unwritten)).toBeNull();
  });

  it("marks the current tab and the tabs with drafts", () => {
    act(() => {
      window.location.hash = "#/keys?tab=macros";
    });
    render(
      <>
        <PageTabs page="keys" />
        <Draft page="keys" tab="assign" count={2} />
      </>,
    );
    const bar = screen.getByRole("navigation", { name: en.shell.tabs.label({ page: en.shell.nav.keys }) });
    const links = within(bar).getAllByRole("link");
    expect(links.map((link) => link.textContent)).toEqual([
      `${en.shell.tabs.pages.keys.assign}${en.shell.tabs.unwritten}`,
      en.shell.tabs.pages.keys.macros,
      en.shell.tabs.pages.keys.advanced,
    ]);
    expect(links[1]?.getAttribute("aria-current")).toBe("page");
    expect(links[0]?.getAttribute("aria-current")).toBeNull();
    expect(links[2]?.getAttribute("href")).toBe("#/keys?tab=advanced");
  });

  it("follows every page header inside a tabbed page", () => {
    render(
      <TabbedPage page="lighting">
        <PageHeader title="x" />
      </TabbedPage>,
    );
    const bar = screen.getByRole("navigation", { name: en.shell.tabs.label({ page: en.shell.nav.lighting }) });
    expect(within(bar).getAllByRole("link")).toHaveLength(2);
    expect(screen.getByRole("heading", { level: 1 }).closest(".ow-page-header")?.nextElementSibling).toBe(bar);
  });
});

describe("the profile-switch confirmation", () => {
  it("lists the unwritten drafts per tab", async () => {
    statusStore.set({ ...READY });
    mockIPC((command) =>
      command === "prepare_profile_switch" ? { token: "p-1", from: 1, to: 2, nothing_to_do: false, save_wait_ms: 0 } : null,
    );
    render(
      <>
        <ProfileSwitcher />
        <Draft page="keys" tab="macros" count={1} />
        <Draft page="keys" tab="advanced" count={2} />
      </>,
    );
    fireEvent.click(screen.getByRole("radio", { name: "2" }));
    await screen.findByText(en.shell.switcher.title({ n: 2 }));
    expect(screen.getByText(en.shell.switcher.drafts)).toBeTruthy();
    expect(screen.getByText("Keys > Macros: 1 unwritten change")).toBeTruthy();
    expect(screen.getByText("Keys > Advanced keys: 2 unwritten changes")).toBeTruthy();
  });
});
