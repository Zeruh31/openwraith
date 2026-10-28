// Settings > Profile names on the mocked IPC, answered with what the core's
// tests recorded (settings_background: get/set_profile_names): the connected
// model's four fields, "Save names" with the exact intent (empty = no name),
// the names then shown by the header switcher and the Keyboard page, and the
// tab bar of Settings with the About tab's updated "Not in this version".

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../../i18n";
import type { ProfileNamesIntent, ProfileNamesView } from "../../ipc/bindings";
import { FIXTURES } from "../../preview/registry";
import { ProfileSwitcher } from "../../shell/Header";
import { profileNamesStore, resetProfileNames } from "../../state/profileNames";
import { INITIAL_STATUS, statusStore } from "../../state/status";
import { clearToasts, getToasts } from "../../state/toasts";
import DevicePage from "../device";
import SettingsPage from "./index";
import { productsToShow } from "./ProfileNamesTab";

const en = DICTIONARIES.en;
const t = en.settings.profiles;

let sent: ProfileNamesIntent[] = [];

function recorded(command: "get_profile_names" | "set_profile_names"): ProfileNamesView {
  const call = FIXTURES["settings_background"]?.calls.find((c) => c.command === command);
  if (call === undefined || call.ok === null) throw new Error(`no recorded ${command}`);
  return structuredClone(call.ok) as ProfileNamesView;
}

function recordedIntent(): ProfileNamesIntent {
  const call = FIXTURES["settings_background"]?.calls.find((c) => c.command === "set_profile_names");
  if (call?.command !== "set_profile_names") throw new Error("no recorded set_profile_names");
  return structuredClone(call.args.names);
}

function install(): void {
  let view = recorded("get_profile_names");
  mockIPC((command, args) => {
    switch (command) {
      case "get_profile_names":
        return view;
      case "set_profile_names":
        sent.push((args as { names: ProfileNamesIntent }).names);
        view = recorded("set_profile_names");
        return view;
      case "get_snapshot":
        return null;
      default:
        throw new Error(`unexpected ${command}`);
    }
  });
}

beforeEach(() => {
  setLang("en");
  sent = [];
  clearToasts();
  resetProfileNames();
  const status = FIXTURES["settings_background"]?.initial_status;
  if (status === undefined) throw new Error("no recorded status");
  statusStore.set(structuredClone(status));
});

afterEach(() => {
  cleanup();
  clearMocks();
  clearToasts();
  resetProfileNames();
  statusStore.set(INITIAL_STATUS);
});

describe("Settings > Profile names", () => {
  it("saves the four names of the connected model, empty meaning no name", async () => {
    install();
    window.location.hash = "#/settings?tab=profiles";
    render(<SettingsPage />);
    const card = (await screen.findByText(t.product({ product: "W75HEARGB" }))).closest("section");
    if (card === null) throw new Error("no card");
    const form = within(card);
    expect(form.getByText(t.connected)).toBeTruthy();
    const save = form.getByRole<HTMLButtonElement>("button", { name: t.save });
    expect(save.disabled).toBe(true);
    const intent = recordedIntent();
    intent.names.forEach((name, index) => {
      fireEvent.change(form.getByRole("textbox", { name: t.name({ n: index + 1 }) }), {
        target: { value: name ?? "  " },
      });
    });
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    await waitFor(() => {
      expect(sent).toEqual([intent]);
    });
    await waitFor(() => {
      expect(getToasts().map((toast) => toast.title)).toEqual([t.saved]);
    });
    expect(profileNamesStore.get()).toEqual(recorded("set_profile_names"));
  });

  it("shows the names in the header switcher and on the Keyboard page", () => {
    profileNamesStore.set(recorded("set_profile_names"));
    render(<ProfileSwitcher />);
    // Profile 1 is active: "Profile 1 · Work" under the switcher.
    expect(screen.getByText("Profile 1 · Work")).toBeTruthy();
    const group = screen.getByRole("radiogroup", { name: en.shell.header.profileLabel });
    expect(within(group).getByRole("radio", { name: en.shell.header.profileOptionNamed({ n: 2, name: "FPS" }) })).toBeTruthy();
    // A profile without a name keeps its number.
    expect(within(group).getByRole("radio", { name: "3" })).toBeTruthy();
    cleanup();
    install();
    render(<DevicePage />);
    expect(screen.getByText("Profile 1 · Work")).toBeTruthy();
  });

  it("lists the connected model first, then others with names", () => {
    const view: ProfileNamesView = {
      current_product: "W75HEARGB",
      products: [
        { product: "OTHER", names: ["A", null, null, null] },
        { product: "W75HEARGB", names: [null, null, null, null] },
      ],
    };
    expect(productsToShow(view, null)).toEqual(["W75HEARGB", "OTHER"]);
    expect(productsToShow({ current_product: null, products: [] }, null)).toEqual([]);
    expect(productsToShow({ current_product: null, products: [] }, "W75HEARGB")).toEqual(["W75HEARGB"]);
  });
});

describe("the Settings tabs", () => {
  it("are links with the current one marked, and About no longer lists the background features", async () => {
    mockIPC(() => null);
    window.location.hash = "#/settings?tab=about";
    render(<SettingsPage />);
    const tabs = screen.getByRole("navigation", { name: en.shell.tabs.label({ page: en.shell.nav.settings }) });
    const names = within(tabs)
      .getAllByRole("link")
      .map((link) => link.textContent);
    const labels = en.shell.tabs.pages.settings;
    expect(names).toEqual([labels.general, labels.background, labels.games, labels.profiles, labels.about]);
    expect(within(tabs).getByRole("link", { name: labels.about }).getAttribute("aria-current")).toBe("page");
    expect(within(tabs).getByRole("link", { name: labels.games }).getAttribute("href")).toBe("#/settings?tab=games");
    const notIncluded = await screen.findByText(en.settings.about.notIncluded);
    for (const word of ["notification", "Windows", "automatic", "reset", "per-game"]) {
      expect(notIncluded.textContent).not.toContain(word);
      expect(DICTIONARIES.tr.settings.about.notIncluded).not.toContain("otomatik");
    }
    expect(screen.getByText(en.settings.descriptions.about)).toBeTruthy();
  });
});
