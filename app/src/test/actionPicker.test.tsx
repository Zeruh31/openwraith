// The action catalogue helpers (ids, names, search in English and Turkish,
// hex and aliases, typed shortcuts, evidence) and the action picker
// (combobox + listbox with aria-activedescendant, tabs, the shortcut
// builder, slot options, layer rules and badges from the catalogue).

import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  ACTION_CATALOG,
  actionId,
  ActionPicker,
  bindingEvidence,
  comboEvidence,
  entryName,
  isValidCombo,
  parseComboQuery,
  searchCatalog,
  type ActionSlotOption,
} from "../components/actions";
import { DICTIONARIES, setLang } from "../i18n";
import type { ActionValue, CatalogEntry } from "../ipc/bindings";

const en = DICTIONARIES.en;
const tr = DICTIONARIES.tr;

function entry(id: string): CatalogEntry {
  const found = ACTION_CATALOG.entries.find((item) => item.id === id);
  if (found === undefined) throw new Error(`no catalogue entry ${id}`);
  return found;
}

beforeEach(() => {
  setLang("en");
});

afterEach(() => {
  cleanup();
  setLang("en");
});

describe("the catalogue", () => {
  it("forms the library's ids", () => {
    for (const item of ACTION_CATALOG.entries) expect(actionId(item.value)).toBe(item.id);
    expect(actionId({ kind: "combo", modifiers: [0xe0, 0xe1], key: 0x04 })).toBe("combo:e0+e1+04");
    expect(actionId({ kind: "macro", slot: 3 })).toBe("macro:3");
    expect(actionId({ kind: "advanced", slot: 12 })).toBe("advanced:12");
  });

  it("names actions in English and Turkish", () => {
    expect(entryName(entry("media:12"), en)).toBe("Mute");
    expect(entryName(entry("media:12"), tr)).toBe("Sessiz");
    expect(entryName(entry("key:e0"), tr)).toBe("Sol Ctrl");
    expect(entryName(entry("key:04"), tr)).toBe("A");
    expect(entryName(entry("key:54"), en)).toBe("Keypad /");
    expect(entryName(entry("mouse:back"), tr)).toBe("Geri düğmesi (X1)");
  });

  it("finds actions by English and Turkish names, aliases and hex", () => {
    expect(searchCatalog("mute", "base")[0]?.id).toBe("media:12");
    expect(searchCatalog("sessiz", "base")[0]?.id).toBe("media:12");
    expect(searchCatalog("0x04", "base")[0]?.id).toBe("key:04");
    expect(searchCatalog("KeyA", "base")[0]?.id).toBe("key:04");
    expect(searchCatalog("geri sil", "base")[0]?.id).toBe("key:2a");
    expect(searchCatalog("ses", "fn").map((item) => item.id)).toEqual(expect.arrayContaining(["media:10", "media:11"]));
    expect(searchCatalog("no such action", "base")).toEqual([]);
  });

  it("reads typed shortcuts", () => {
    expect(parseComboQuery("ctrl c")).toEqual({ modifiers: [0xe0], key: 0x06 });
    expect(parseComboQuery("Ctrl + Shift + S")).toEqual({ modifiers: [0xe0, 0xe1], key: 0x16 });
    expect(parseComboQuery("sağ alt 4")).toEqual({ modifiers: [0xe6], key: 0x21 });
    // "r" and "l" are keys unless a modifier follows them.
    expect(parseComboQuery("ctrl r")).toEqual({ modifiers: [0xe0], key: 0x15 });
    expect(parseComboQuery("ctrl+r")).toEqual({ modifiers: [0xe0], key: 0x15 });
    expect(parseComboQuery("ctrl l")).toEqual({ modifiers: [0xe0], key: 0x0f });
    expect(parseComboQuery("win l")).toEqual({ modifiers: [0xe3], key: 0x0f });
    expect(parseComboQuery("alt r")).toEqual({ modifiers: [0xe2], key: 0x15 });
    expect(parseComboQuery("right ctrl r")).toEqual({ modifiers: [0xe4], key: 0x15 });
    expect(parseComboQuery("r ctrl l")).toEqual({ modifiers: [0xe4], key: 0x0f });
    expect(parseComboQuery("ctrl shift alt a")).toBeNull();
    expect(parseComboQuery("ctrl")).toBeNull();
    expect(parseComboQuery("mute")).toBeNull();
    expect(isValidCombo([0xe0, 0xe0], 0x04)).toBe(false);
    expect(isValidCombo([0xe0], 0xe1)).toBe(false);
  });

  it("takes evidence from the catalogue only", () => {
    expect(comboEvidence([0xe0], 0x06, "base").status).toBe("confirmed");
    expect(comboEvidence([0xe4], 0x06, "base").status).toBe("unverified");
    expect(bindingEvidence("macro", "fn")?.status).toBe("confirmed");
    expect(bindingEvidence("advanced", "fn")?.status).toBe("unverified");
  });
});

const MACROS: ActionSlotOption[] = [
  { slot: 2, label: "Macro 3 · Type A", description: "2 steps · Plays once", evidence: { status: "confirmed" } },
  { slot: 3, label: "Macro 4", evidence: { status: "confirmed" }, disabledReason: "Used by Q" },
];

const ADVANCED: ActionSlotOption[] = [
  { slot: 0, label: "Advanced key 1 · Dynamic keystroke (DKS)", evidence: { status: "confirmed" } },
];

function renderPicker(props: { layer?: "base" | "fn"; value?: ActionValue | null; onPick?: (value: ActionValue) => void } = {}) {
  const onPick = props.onPick ?? vi.fn();
  render(
    <ActionPicker
      layer={props.layer ?? "base"}
      value={props.value ?? null}
      onPick={onPick}
      macros={MACROS}
      advanced={ADVANCED}
    />,
  );
  return onPick;
}

function combobox(): HTMLInputElement {
  return screen.getByRole("combobox", { name: en.keys.picker.search });
}

function activeOption(): HTMLElement {
  const id = combobox().getAttribute("aria-activedescendant");
  if (id === null) throw new Error("no active option");
  const option = document.getElementById(id);
  if (option === null) throw new Error("active option not rendered");
  return option;
}

describe("the action picker", () => {
  it("is a combobox driving a listbox through aria-activedescendant", () => {
    const onPick = renderPicker();
    const input = combobox();
    expect(document.activeElement).toBe(input);
    expect(input.getAttribute("aria-controls")).toBe(screen.getByRole("listbox", { name: en.keys.picker.options }).id);
    expect(activeOption().getAttribute("aria-selected")).toBe("true");
    expect(activeOption().textContent).toContain("A");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(activeOption().textContent).toContain("B");
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onPick).toHaveBeenCalledWith({ kind: "key", usage: 0x05 });
  });

  it("has the seven tabs and starts on the current action's tab", () => {
    renderPicker({ value: { kind: "media", index: 0x12 } });
    const tabs = screen.getByRole("tablist", { name: en.keys.picker.tabsLabel });
    expect(within(tabs).getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "Keys",
      "Media",
      "Mouse",
      "Shortcuts",
      "Macros",
      "Advanced",
      "Default",
    ]);
    expect(within(tabs).getByRole("tab", { name: "Media" }).getAttribute("aria-selected")).toBe("true");
    expect(activeOption().textContent).toContain("Mute");
    expect(activeOption().textContent).toContain(en.keys.picker.current);
  });

  it("shows the catalogue's evidence as badges", () => {
    renderPicker({ value: { kind: "media", index: 0x12 } });
    const list = screen.getByRole("listbox", { name: en.keys.picker.options });
    const volume = within(list).getAllByRole("option").find((option) => option.textContent.startsWith("Volume up"));
    expect(volume?.textContent).toContain(en.common.badge.unverified);
    const mute = within(list).getAllByRole("option").find((option) => option.textContent.startsWith("Mute"));
    expect(mute?.textContent).not.toContain(en.common.badge.unverified);
  });

  it("searches every tab and offers a typed shortcut first", () => {
    const onPick = renderPicker();
    fireEvent.change(combobox(), { target: { value: "mute" } });
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getByRole("status").textContent).toBe(en.keys.picker.results({ n: 1 }));
    expect(activeOption().textContent).toContain("Mute");

    fireEvent.change(combobox(), { target: { value: "ctrl c" } });
    expect(activeOption().textContent).toContain("Left Ctrl + C");
    fireEvent.keyDown(combobox(), { key: "Enter" });
    expect(onPick).toHaveBeenCalledWith({ kind: "combo", modifiers: [0xe0], key: 0x06 });

    fireEvent.keyDown(combobox(), { key: "Escape" });
    expect(combobox().value).toBe("");
    expect(screen.getByRole("tablist")).toBeTruthy();
  });

  it("finds by Turkish names in Turkish", () => {
    setLang("tr");
    renderPicker();
    const input = screen.getByRole("combobox", { name: tr.keys.picker.search });
    fireEvent.change(input, { target: { value: "sessiz" } });
    const id = input.getAttribute("aria-activedescendant") ?? "";
    expect(document.getElementById(id)?.textContent).toContain("Sessiz");
  });

  it("builds a shortcut from one or two modifiers and a key", () => {
    const onPick = renderPicker();
    fireEvent.click(screen.getByRole("tab", { name: "Shortcuts" }));
    const use = () => screen.getByRole("button", { name: /^Use / });
    expect(use()).toHaveProperty("disabled", true);
    expect(screen.getByText(en.keys.picker.shortcut.needModifier)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Left Ctrl" }));
    fireEvent.click(screen.getByRole("button", { name: "Left Shift" }));
    const rightAlt = screen.getByRole("button", { name: new RegExp(`^Right Alt`) });
    expect(rightAlt.getAttribute("aria-disabled")).toBe("true");
    expect(rightAlt.textContent).toContain(en.common.badge.unverified);
    fireEvent.change(screen.getByRole("combobox", { name: en.keys.picker.shortcut.key }), { target: { value: "22" } });
    fireEvent.click(use());
    expect(onPick).toHaveBeenCalledWith({ kind: "combo", modifiers: [0xe0, 0xe1], key: 22 });
  });

  it("offers the keyboard's slots; a slot another key uses can't be picked", () => {
    const onPick = renderPicker();
    fireEvent.click(screen.getByRole("tab", { name: "Macros" }));
    const options = within(screen.getByRole("listbox")).getAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual([
      expect.stringContaining("Macro 3 · Type A"),
      expect.stringContaining("Used by Q"),
    ]);
    fireEvent.click(options[1] as HTMLElement);
    expect(onPick).not.toHaveBeenCalled();
    fireEvent.click(options[0] as HTMLElement);
    expect(onPick).toHaveBeenCalledWith({ kind: "macro", slot: 2 });
  });

  it("marks advanced keys on the Fn layer as not yet verified (the binding rule)", () => {
    renderPicker({ layer: "fn" });
    fireEvent.click(screen.getByRole("tab", { name: "Advanced" }));
    const option = within(screen.getByRole("listbox")).getAllByRole("option")[0];
    expect(option?.textContent).toContain(en.common.badge.unverified);
  });
});
