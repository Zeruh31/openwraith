// Shared editors: the three badges, the confirm checkbox, the travel gauge
// and its text alternative, the step list (Alt + Up/Down), the colour picker
// (sliders, hex, swatches, recent colours), the key picker (combobox and
// "pick on the board") and the keyboard's new options (single selection,
// activation, sublegend, swatch, fill, badge, knob popover).

import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Badge, badgeTooltip, EvidenceBadge } from "../components/Badge";
import { ColorPicker, recentColors } from "../components/color/ColorPicker";
import { hexToHsv, hsvToHex } from "../components/color/hsv";
import { ConfirmCheckbox } from "../components/ConfirmCheckbox";
import { KeyboardView, type KeyVisual } from "../components/keyboard";
import { KeyPicker, type KeyOption } from "../components/KeyPicker";
import { StepList } from "../components/StepList";
import { gaugeLines, TravelGauge } from "../components/TravelGauge";
import { DICTIONARIES, setLang } from "../i18n";

const en = DICTIONARIES.en;
const W = 46;
const A = 68;

beforeEach(() => {
  setLang("en");
});

afterEach(() => {
  cleanup();
});

describe("badges", () => {
  it("say their meaning in text and in the tooltip", () => {
    render(
      <>
        <Badge kind="unverified" notes={[{ what: "DKS tap at stage 4", question: "Q19" }]} />
        <Badge kind="kept" />
        <Badge kind="keyboard_wide" />
      </>,
    );
    const unverified = screen.getByText(en.common.badge.unverified).closest(".ow-badge");
    expect(unverified?.getAttribute("title")).toContain("a tap at stage 4 of a dynamic keystroke");
    expect(unverified?.getAttribute("title")).not.toContain("Q19");
    // The tip is the badge's description, not part of its text (or of a control's name around it).
    const describedBy = unverified?.getAttribute("aria-describedby") ?? "";
    expect(document.getElementById(describedBy)?.textContent).toContain(en.common.badge.unverifiedTip);
    expect(document.getElementById(describedBy)?.hidden).toBe(true);
    expect(screen.getByText(en.common.badge.kept).closest(".ow-badge")?.getAttribute("title")).toBe(en.common.badge.keptTip);
    expect(screen.getByText(en.common.badge.keyboardWide)).toBeTruthy();
  });

  it("translate the library's notes and never show its question ids", () => {
    const notes = [
      { what: "key combination with a right-side modifier", question: "Q37" },
      { what: "a note this version doesn't know", question: "Q99" },
    ];
    const tr = DICTIONARIES.tr;
    const tip = badgeTooltip("unverified", tr, notes);
    expect(tip).toContain(tr.common.badge.notes["key combination with a right-side modifier"] ?? "missing");
    expect(tip).not.toMatch(/Q\d+|right-side|doesn't know/u);
    expect(badgeTooltip("unverified", en, [notes[1] as (typeof notes)[number]])).toBe(en.common.badge.unverifiedTip);
    // In a button, the badge adds its text to the name, not its tooltip.
    render(
      <button type="button">
        R-Ctrl <Badge kind="unverified" notes={notes} size="sm" />
      </button>,
    );
    expect(screen.getByRole("button", { name: `R-Ctrl ${en.common.badge.unverified}` })).toBeTruthy();
  });

  it("follow the evidence exactly", () => {
    const { container, rerender } = render(<EvidenceBadge evidence={{ status: "confirmed" }} />);
    expect(container.textContent).toBe("");
    rerender(<EvidenceBadge evidence={{ status: "unverified", notes: [] }} />);
    expect(container.textContent).toContain(en.common.badge.unverified);
    rerender(<EvidenceBadge evidence={null} />);
    expect(container.textContent).toBe("");
    rerender(<EvidenceBadge evidence={null} keptWhenNone />);
    expect(container.textContent).toContain(en.common.badge.kept);
  });
});

describe("the confirm checkbox", () => {
  it("is never pre-ticked and reports the tick", () => {
    const onChange = vi.fn();
    render(<ConfirmCheckbox checked={false} onChange={onChange} label="Fn + 2 will no longer switch" />);
    const box = screen.getByRole("checkbox", { name: "Fn + 2 will no longer switch" });
    expect(box).toHaveProperty("checked", false);
    fireEvent.click(box);
    expect(onChange).toHaveBeenCalledWith(true);
  });
});

describe("the travel gauge", () => {
  const formatMm = (mm: number) => `${mm.toFixed(2)} mm`;

  it("has a text alternative for every mark", () => {
    const props = {
      formatMm,
      actuationMm: 1.2,
      rt: { pressMm: 0.3, releaseMm: 0.5 },
      deadZones: { topMm: 0.1, bottomMm: 0.2 },
      points: [{ mm: 1.5, label: "Press" }],
      depthMm: 0.8,
      peakMm: 2.1,
    };
    expect(gaugeLines(props, en)).toEqual([
      en.common.pickers.gauge.top,
      "Top dead zone: the first 0.10 mm",
      "Bottom dead zone: the last 0.20 mm",
      "Actuation point at 1.20 mm",
      "Rapid Trigger: presses again after 0.30 mm downwards",
      "Rapid Trigger: releases after 0.50 mm upwards",
      "Press at 1.50 mm",
      "Pressed 0.80 mm now",
      "Deepest so far 2.10 mm",
      "Bottom: fully pressed (3.40 mm)",
    ]);
    render(<TravelGauge {...props} label="W: key travel" />);
    const gauge = screen.getByRole("group", { name: "W: key travel" });
    const description = document.getElementById(gauge.getAttribute("aria-describedby") ?? "");
    expect(description?.textContent).toContain("Actuation point at 1.20 mm");
  });
});

describe("the step list", () => {
  function Steps({ onMove }: { onMove: (from: number, to: number) => void }) {
    const [items, setItems] = useState(["A down", "A up", "B down"]);
    return (
      <StepList
        label="Steps"
        items={items}
        itemKey={(item) => item}
        itemLabel={(item) => item}
        renderItem={(item) => <span>{item}</span>}
        onMove={(from, to) => {
          onMove(from, to);
          setItems((list) => {
            const next = [...list];
            const [moved] = next.splice(from, 1);
            if (moved !== undefined) next.splice(to, 0, moved);
            return next;
          });
        }}
        onRemove={(index) => {
          setItems((list) => list.filter((_, i) => i !== index));
        }}
      />
    );
  }

  it("moves the focused step with Alt + Up/Down and announces it", () => {
    const onMove = vi.fn();
    render(<Steps onMove={onMove} />);
    const list = screen.getByRole("list", { name: "Steps" });
    const first = within(list).getAllByRole("listitem")[0] as HTMLElement;
    act(() => {
      first.focus();
    });
    fireEvent.keyDown(first, { key: "ArrowDown", altKey: true });
    expect(onMove).toHaveBeenCalledWith(0, 1);
    const items = within(list).getAllByRole("listitem");
    expect(items.map((item) => item.textContent.includes("A down"))).toEqual([false, true, false]);
    expect(document.activeElement?.textContent).toContain("A down");
    expect(screen.getByRole("status").textContent).toBe(en.common.pickers.steps.moved({ from: 1, to: 2 }));
    fireEvent.keyDown(first, { key: "ArrowDown" });
    expect(onMove).toHaveBeenCalledTimes(1);
  });

  it("has move and remove buttons; the ends can't move further", () => {
    const onMove = vi.fn();
    render(<Steps onMove={onMove} />);
    expect(screen.getByRole("button", { name: `${en.common.pickers.steps.moveUp}: A down` })).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", { name: `${en.common.pickers.steps.moveDown}: B down` })).toHaveProperty("disabled", true);
    fireEvent.click(screen.getByRole("button", { name: `${en.common.pickers.steps.remove}: A up` }));
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
  });
});

describe("the colour picker", () => {
  it("converts between hex and HSV", () => {
    expect(hsvToHex({ h: 0, s: 1, v: 1 })).toBe("#ff0000");
    expect(hsvToHex({ h: 120, s: 1, v: 1 })).toBe("#00ff00");
    expect(hexToHsv("#0000ff")).toEqual({ h: 240, s: 1, v: 1 });
    expect(hexToHsv("#808080", 200).h).toBe(200);
  });

  function Picker({ onChange }: { onChange: (hex: string) => void }) {
    const [value, setValue] = useState("#ff0000");
    return (
      <ColorPicker
        label="Colour"
        value={value}
        swatches={[{ hex: "#0000ff", name: "Blue" }]}
        onChange={(hex) => {
          onChange(hex);
          setValue(hex);
        }}
      />
    );
  }

  it("has a hue slider and a saturation/brightness slider that work with the keyboard", () => {
    const onChange = vi.fn();
    render(<Picker onChange={onChange} />);
    const hue = screen.getByRole("slider", { name: en.common.pickers.color.hue });
    expect(hue.getAttribute("aria-valuenow")).toBe("0");
    fireEvent.keyDown(hue, { key: "ArrowRight", shiftKey: true });
    expect(hue.getAttribute("aria-valuenow")).toBe("10");
    expect(onChange).toHaveBeenLastCalledWith("#ff2a00");
    const area = screen.getByRole("slider", { name: en.common.pickers.color.area });
    expect(area.getAttribute("aria-valuetext")).toBe(en.common.pickers.color.areaValue({ s: 100, v: 100 }));
    fireEvent.keyDown(area, { key: "ArrowDown", shiftKey: true });
    expect(area.getAttribute("aria-valuetext")).toBe(en.common.pickers.color.areaValue({ s: 100, v: 90 }));
  });

  it("takes a hex value and swatches, and remembers recent colours", () => {
    recentColors.set([]);
    const onChange = vi.fn();
    render(<Picker onChange={onChange} />);
    const hex = screen.getByRole("textbox", { name: en.common.pickers.color.hex });
    fireEvent.focus(hex);
    fireEvent.change(hex, { target: { value: "0f0" } });
    fireEvent.blur(hex);
    expect(onChange).toHaveBeenLastCalledWith("#00ff00");
    fireEvent.focus(hex);
    fireEvent.change(hex, { target: { value: "nope" } });
    fireEvent.blur(hex);
    expect(screen.getByText(en.common.fields.invalidHex)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Blue, #0000ff" }));
    expect(onChange).toHaveBeenLastCalledWith("#0000ff");
    expect(recentColors.get()).toEqual(["#0000ff", "#00ff00"]);
    const recent = screen.getByRole("group", { name: en.common.pickers.color.recent });
    expect(within(recent).getAllByRole("button")).toHaveLength(2);
  });
});

describe("the key picker", () => {
  const OPTIONS: KeyOption[] = [
    { index: A, name: "A" },
    { index: W, name: "W" },
    { index: 90, name: "Caps Lock", disabledReason: "Not here" },
  ];

  it("filters as you type and picks with the keyboard", () => {
    const onChange = vi.fn();
    render(<KeyPicker label="Key 1" value={null} onChange={onChange} options={OPTIONS} />);
    const input = screen.getByRole("combobox", { name: "Key 1" });
    fireEvent.change(input, { target: { value: "w" } });
    expect(input.getAttribute("aria-expanded")).toBe("true");
    const list = screen.getByRole("listbox");
    expect(within(list).getAllByRole("option").map((option) => option.textContent)).toEqual(["W"]);
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith(W);
  });

  it("picks on the board (a single-select listbox in a dialog)", () => {
    const onChange = vi.fn();
    render(<KeyPicker label="Key 1" value={A} onChange={onChange} options={OPTIONS} />);
    expect(screen.getByRole("combobox", { name: "Key 1" })).toHaveProperty("value", "A");
    fireEvent.click(screen.getByRole("button", { name: en.common.pickers.keyPicker.pickOnBoard }));
    const board = screen.getByRole("listbox", { name: en.common.pickers.keyPicker.boardLabel });
    expect(board.getAttribute("aria-multiselectable")).toBe("false");
    const caps = board.querySelector<HTMLElement>('[data-key-index="90"]');
    expect(caps?.getAttribute("aria-disabled")).toBe("true");
    const w = board.querySelector<HTMLElement>(`[data-key-index="${W}"]`);
    if (w === null) throw new Error("no W");
    fireEvent.pointerDown(w, { button: 0, pointerId: 1, clientX: 5, clientY: 5 });
    fireEvent.pointerUp(w, { button: 0, pointerId: 1, clientX: 5, clientY: 5 });
    expect(onChange).toHaveBeenCalledWith(W);
  });

  it("closes the board with Escape, also while the picker has a value", () => {
    const onChange = vi.fn();
    render(<KeyPicker label="Key 1" value={A} onChange={onChange} options={OPTIONS} />);
    fireEvent.click(screen.getByRole("button", { name: en.common.pickers.keyPicker.pickOnBoard }));
    const board = screen.getByRole("listbox", { name: en.common.pickers.keyPicker.boardLabel });
    fireEvent.keyDown(board, { key: "Escape" });
    expect(screen.queryByRole("listbox", { name: en.common.pickers.keyPicker.boardLabel })).toBeNull();
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("the keyboard's new options", () => {
  const visual = (index: number): KeyVisual => ({
    legend: index === W ? "W" : String(index),
    name: index === W ? "W" : String(index),
    sublegend: index === W ? "Q" : null,
    badge: index === W ? "M3" : null,
    swatch: index === A ? "#ff0000" : null,
    fill: index === A ? 0.5 : null,
  });

  it("selects one key at a time and activates with Enter and double-click", () => {
    const onActivate = vi.fn();
    function Board() {
      const [selected, setSelected] = useState<Set<number>>(new Set());
      return (
        <KeyboardView
          label="Board"
          selection="single"
          keyVisual={visual}
          selected={selected}
          onSelectionChange={setSelected}
          knobLabel="Knob"
          onActivate={onActivate}
        />
      );
    }
    render(<Board />);
    const board = screen.getByRole("listbox", { name: "Board" });
    expect(board.getAttribute("aria-multiselectable")).toBe("false");
    const key = (index: number) => board.querySelector<HTMLElement>(`[data-key-index="${index}"]`) as HTMLElement;
    const click = (index: number) => {
      fireEvent.pointerDown(key(index), { button: 0, pointerId: 1, clientX: 5, clientY: 5 });
      fireEvent.pointerUp(key(index), { button: 0, pointerId: 1, clientX: 5, clientY: 5 });
    };
    click(W);
    click(A);
    expect(key(W).getAttribute("aria-selected")).toBe("false");
    expect(key(A).getAttribute("aria-selected")).toBe("true");
    act(() => {
      key(W).focus();
    });
    fireEvent.keyDown(key(W), { key: "Enter" });
    expect(onActivate).toHaveBeenLastCalledWith(W);
    expect(key(W).getAttribute("aria-selected")).toBe("true");
    fireEvent.doubleClick(key(A));
    expect(onActivate).toHaveBeenLastCalledWith(A);
  });

  it("draws sublegends, badges, swatches and fills (decorative; the name says the rest)", () => {
    render(<KeyboardView label="Board" keyVisual={visual} selected={new Set()} onSelectionChange={() => undefined} knobLabel="Knob" />);
    const w = document.querySelector<HTMLElement>(`[data-key-index="${W}"]`);
    expect(w?.querySelector(".ow-key__sublegend")?.textContent).toBe("Q");
    expect(w?.querySelector(".ow-key__badge")?.textContent).toBe("M3");
    const a = document.querySelector<HTMLElement>(`[data-key-index="${A}"]`);
    expect(a?.querySelector<HTMLElement>(".ow-key__swatch")?.style.backgroundColor).toBe("rgb(255, 0, 0)");
    expect(a?.querySelector<HTMLElement>(".ow-key__fill")?.style.height).toBe("50%");
  });

  it("opens the knob's information from the knob (it stays unselectable)", () => {
    render(
      <KeyboardView
        label="Board"
        keyVisual={visual}
        selected={new Set()}
        onSelectionChange={() => undefined}
        knobLabel="Knob"
        knobInfo={<p>Knob entry 16: Kept as read</p>}
      />,
    );
    const knob = document.querySelector<HTMLElement>('[data-key-index="15"]') as HTMLElement;
    expect(knob.getAttribute("aria-haspopup")).toBe("dialog");
    expect(knob.getAttribute("aria-expanded")).toBe("false");
    act(() => {
      knob.focus();
    });
    fireEvent.keyDown(knob, { key: "Enter" });
    const popover = screen.getByRole("dialog", { name: en.common.pickers.keyboard.knobTitle });
    expect(popover.textContent).toContain("Knob entry 16: Kept as read");
    expect(knob.getAttribute("aria-selected")).toBe("false");
    fireEvent.keyDown(popover, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
