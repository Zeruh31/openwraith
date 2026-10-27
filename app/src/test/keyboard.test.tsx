// The W75 drawing (geometry against the library's key table) and the key
// selection model, plus the keyboard component's keyboard interaction.

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import {
  ALL_KEYS,
  ARROWS,
  addRange,
  applyMarquee,
  invert,
  KEYBOARD_HEIGHT_U,
  KEYBOARD_WIDTH_U,
  KeyboardView,
  KNOB_INDEX,
  keysBetween,
  moveFocus,
  only,
  sameSetting,
  selectAll,
  shapeOf,
  toggle,
  W75_GEOMETRY,
  WASD,
} from "../components/keyboard";
import { W75_KEYS } from "../generated/w75Keys";

const SELECTABLE = new Set(ALL_KEYS);
const W = 46;
const A = 68;
const S = 69;
const D = 70;
const E = 47;
const Q = 45;

describe("W75 geometry", () => {
  it("holds exactly the library's 82 keys", () => {
    const geometry = W75_GEOMETRY.map((k) => k.index).sort((a, b) => a - b);
    const library = W75_KEYS.map((k) => k.index).sort((a, b) => a - b);
    expect(geometry).toEqual(library);
    expect(geometry).toHaveLength(82);
  });

  it("draws the knob as a circle at index 15", () => {
    const knob = W75_KEYS.find((k) => k.knob);
    expect(knob?.index).toBe(KNOB_INDEX);
    expect(shapeOf(KNOB_INDEX)?.shape).toBe("knob");
    expect(W75_GEOMETRY.filter((k) => k.shape === "knob")).toHaveLength(1);
  });

  it("has no overlapping keys", () => {
    const overlaps: string[] = [];
    for (const [i, a] of W75_GEOMETRY.entries()) {
      for (const b of W75_GEOMETRY.slice(i + 1)) {
        const eps = 1e-9;
        const apart =
          a.x + a.w <= b.x + eps || b.x + b.w <= a.x + eps || a.y + a.h <= b.y + eps || b.y + b.h <= a.y + eps;
        if (!apart) overlaps.push(`${a.index}/${b.index}`);
      }
    }
    expect(overlaps).toEqual([]);
  });

  it("keeps every key inside the bounding box", () => {
    for (const key of W75_GEOMETRY) {
      expect(key.x).toBeGreaterThanOrEqual(0);
      expect(key.y).toBeGreaterThanOrEqual(0);
      expect(key.x + key.w).toBeLessThanOrEqual(KEYBOARD_WIDTH_U);
      expect(key.y + key.h).toBeLessThanOrEqual(KEYBOARD_HEIGHT_U);
    }
  });

  it("orders keys of a matrix row left to right by column", () => {
    for (const row of [0, 1, 2, 3, 4, 5]) {
      const keys = W75_KEYS.filter((k) => k.row === row && !k.knob);
      const xs = keys.map((k) => shapeOf(k.index)?.x ?? -1);
      expect(xs).toEqual([...xs].sort((a, b) => a - b));
    }
  });
});

describe("selection model", () => {
  it("toggles keys and never selects the knob", () => {
    const one = toggle(new Set(), W, SELECTABLE);
    expect([...one]).toEqual([W]);
    expect(toggle(one, W, SELECTABLE).size).toBe(0);
    expect(toggle(new Set(), KNOB_INDEX, SELECTABLE).size).toBe(0);
    expect(only(KNOB_INDEX, SELECTABLE).size).toBe(0);
    expect([...only(W, SELECTABLE)]).toEqual([W]);
  });

  it("selects all 81 keys and inverts without the knob", () => {
    const all = selectAll(SELECTABLE);
    expect(all.size).toBe(81);
    expect(all.has(KNOB_INDEX)).toBe(false);
    const inverted = invert(new Set(WASD), SELECTABLE);
    expect(inverted.size).toBe(77);
    expect(inverted.has(W)).toBe(false);
    expect(inverted.has(KNOB_INDEX)).toBe(false);
  });

  it("leaves out keys the page marks unselectable", () => {
    const limited = new Set(ALL_KEYS.filter((k) => k !== W));
    expect(toggle(new Set(), W, limited).size).toBe(0);
    expect(selectAll(limited).has(W)).toBe(false);
  });

  it("adds the rectangle between two keys (Shift + click)", () => {
    expect(keysBetween(Q, E).sort((a, b) => a - b)).toEqual([Q, W, E]);
    const range = addRange(new Set([D]), Q, S, SELECTABLE);
    expect([...range].sort((a, b) => a - b)).toEqual([Q, W, A, S, D].sort((a, b) => a - b));
  });

  it("adds and removes keys with a marquee", () => {
    const rect = { x0: 1.8, y0: 2.4, x1: 3.0, y1: 3.6 };
    const added = applyMarquee(new Set(), rect, "add", SELECTABLE);
    expect([...added].sort((a, b) => a - b)).toEqual([Q, W, A, S].sort((a, b) => a - b));
    const removed = applyMarquee(new Set([Q, W, A, S, D]), rect, "remove", SELECTABLE);
    expect([...removed]).toEqual([D]);
    const knobArea = applyMarquee(new Set(), { x0: 15, y0: 0, x1: 16.25, y1: 1 }, "add", SELECTABLE);
    expect(knobArea.has(KNOB_INDEX)).toBe(false);
  });

  it("selects keys with the same setting", () => {
    const values: Record<number, string> = { [W]: "a", [A]: "a", [S]: "b" };
    const same = sameSetting(W, (index) => values[index] ?? null, SELECTABLE);
    expect([...same].sort((a, b) => a - b)).toEqual([W, A].sort((a, b) => a - b));
    expect(sameSetting(D, (index) => values[index] ?? null, SELECTABLE).size).toBe(0);
  });

  it("moves the focus spatially", () => {
    expect(moveFocus(W, "right")).toBe(E);
    expect(moveFocus(W, "left")).toBe(Q);
    expect(moveFocus(W, "down")).toBe(S);
    expect(moveFocus(0, "left")).toBe(0);
    expect(ARROWS).toContain(moveFocus(102, "down"));
  });
});

describe("KeyboardView", () => {
  const visual = (index: number) => ({
    legend: W75_KEYS.find((k) => k.index === index)?.name ?? "?",
    name: W75_KEYS.find((k) => k.index === index)?.name ?? "?",
    value: "1.70",
  });

  it("renders 82 options with the knob disabled", () => {
    render(
      <KeyboardView
        label="Keys"
        keyVisual={visual}
        selected={new Set()}
        onSelectionChange={() => undefined}
        knobLabel="The knob has no trigger settings"
        unitPx={40}
      />,
    );
    const options = screen.getAllByRole("option");
    expect(options).toHaveLength(82);
    const knob = options.find((o) => o.getAttribute("data-key-index") === String(KNOB_INDEX));
    expect(knob?.getAttribute("aria-disabled")).toBe("true");
  });

  it("selects all but the knob with Ctrl + A, toggles with Space and clears with Esc", () => {
    const change = vi.fn<(next: Set<number>) => void>();
    const { rerender } = render(
      <KeyboardView
        label="Keys"
        keyVisual={visual}
        selected={new Set()}
        onSelectionChange={change}
        knobLabel="knob"
        unitPx={40}
      />,
    );
    const board = screen.getByRole("listbox");
    fireEvent.keyDown(board, { key: "a", ctrlKey: true });
    const all = change.mock.calls[0]?.[0];
    expect(all?.size).toBe(81);
    expect(all?.has(KNOB_INDEX)).toBe(false);

    fireEvent.keyDown(board, { key: " " });
    expect([...(change.mock.calls[1]?.[0] ?? [])]).toEqual([0]);

    rerender(
      <KeyboardView
        label="Keys"
        keyVisual={visual}
        selected={new Set([0, W])}
        onSelectionChange={change}
        knobLabel="knob"
        unitPx={40}
      />,
    );
    fireEvent.keyDown(board, { key: "Escape" });
    expect(change.mock.calls[2]?.[0].size).toBe(0);
  });

  it("names keys by their visible text and describes them separately; flags and tooltips", () => {
    render(
      <KeyboardView
        label="Keys"
        keyVisual={(index) => ({
          ...visual(index),
          chip: index === W ? "RT" : null,
          description: index === W ? "Rapid Trigger 0.30 mm" : null,
          flag: index === W,
          tooltip: index === W ? "why" : null,
        })}
        selected={new Set()}
        onSelectionChange={() => undefined}
        knobLabel="knob"
        unitPx={40}
      />,
    );
    const w = screen.getAllByRole("option").find((o) => o.getAttribute("data-key-index") === String(W));
    if (w === undefined) throw new Error("no W");
    expect(w.getAttribute("aria-label")).toBe("W 1.70 RT");
    const describedBy = w.getAttribute("aria-describedby");
    expect(describedBy === null ? null : document.getElementById(describedBy)?.textContent).toBe(
      "Rapid Trigger 0.30 mm",
    );
    expect(w.getAttribute("data-flag")).toBe("true");
    expect(w.getAttribute("title")).toBe("why");
    const q = screen.getAllByRole("option").find((o) => o.getAttribute("data-key-index") === String(Q));
    expect(q?.hasAttribute("aria-describedby")).toBe(false);
    expect(q?.hasAttribute("data-flag")).toBe(false);
    expect(q?.hasAttribute("title")).toBe(false);
  });

  it("starts a key's name with its legend when the legend is not the full name", () => {
    render(
      <KeyboardView
        label="Keys"
        keyVisual={(index) =>
          index === 0
            ? { legend: "ESC", name: "Escape", value: "1.70", chip: "RT" }
            : index === W
              ? { legend: "w", name: "W", value: "1.70" }
              : visual(index)
        }
        selected={new Set()}
        onSelectionChange={() => undefined}
        knobLabel="knob"
        unitPx={40}
      />,
    );
    const option = (index: number) =>
      screen.getAllByRole("option").find((o) => o.getAttribute("data-key-index") === String(index));
    expect(option(0)?.getAttribute("aria-label")).toBe("ESC (Escape) 1.70 RT");
    expect(option(W)?.getAttribute("aria-label")).toBe("W 1.70");
  });

  it("changes nothing when disabled", () => {
    const change = vi.fn();
    render(
      <KeyboardView
        label="Keys"
        keyVisual={visual}
        selected={new Set()}
        onSelectionChange={change}
        knobLabel="knob"
        disabled
        unitPx={40}
      />,
    );
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "a", ctrlKey: true });
    expect(change).not.toHaveBeenCalled();
  });
});
