// Lighting > Key colours: the pure logic (draft, history, gradient, intent,
// colour names, review grouping, blocks, preview drafts).

import { describe, expect, it } from "vitest";

import { DICTIONARIES } from "../../../i18n";
import type { ColorChangeView, ColorsState, DeviceStatus } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import { colorNameText, colorText, editProblemText, keysListText, legendText, warningText } from "./labels";
import {
  afterColors,
  beforeColors,
  buildColorIntent,
  colorCounts,
  colorName,
  colorsBlock,
  draftChanges,
  draftFromPreview,
  EMPTY_HISTORY,
  gradientColors,
  HISTORY_LIMIT,
  keyboardColors,
  mixHex,
  normalize,
  normalizeHistory,
  paint,
  paintEach,
  record,
  redo,
  sameColors,
  shownColor,
  switchApplies,
  undo,
  groupChanges,
  type ColorHistory,
  type ColorMap,
} from "./model";

const en = DICTIONARIES.en;
const tr = DICTIONARIES.tr;

/** The recorded colours of the simulator (every key red, Custom effect). */
function readyState(): ColorsState {
  const call = FIXTURES["colors_ready"]?.calls.find((c) => c.command === "get_colors_state");
  if (call?.command !== "get_colors_state" || call.ok === null) throw new Error("no colours state");
  return structuredClone(call.ok);
}

const W = 46;
const A = 68;
const S = 69;
const D = 70;
const ESC = 0;
const RIGHT = 125;

function map(entries: [number, string][]): ColorMap {
  return new Map(entries);
}

describe("the draft", () => {
  const state = readyState();
  const current = keyboardColors(state);

  it("lists every key of the layout but the knob", () => {
    expect(current.size).toBe(81);
    expect(current.has(15)).toBe(false);
    expect(current.get(W)).toBe("#ff0000");
  });

  it("paints keys, and a key set back to the keyboard's colour leaves the draft", () => {
    const one = paint(new Map(), current, [W, A], "#0000FF");
    expect([...one]).toEqual([
      [W, "#0000ff"],
      [A, "#0000ff"],
    ]);
    const back = paint(one, current, [W], "#ff0000");
    expect([...back]).toEqual([[A, "#0000ff"]]);
    // Same colour again: the same map (no undo step).
    expect(paint(back, current, [A], "#0000ff")).toBe(back);
    // The knob and unknown slots are ignored.
    expect(paint(back, current, [15, 200], "#00ff00")).toBe(back);
    // An invalid colour is ignored.
    expect(paint(back, current, [W], "blue")).toBe(back);
  });

  it("paints each key with its own colour", () => {
    const next = paintEach(new Map(), current, map([[W, "#00ff00"], [A, "#ff0000"], [S, "#0000ff"]]));
    expect([...next]).toEqual([
      [W, "#00ff00"],
      [S, "#0000ff"],
    ]);
  });

  it("shows the draft over the keyboard", () => {
    const draft = map([[W, "#0000ff"]]);
    expect(shownColor(draft, current, W)).toBe("#0000ff");
    expect(shownColor(draft, current, A)).toBe("#ff0000");
    expect(shownColor(draft, current, 15)).toBeNull();
  });

  it("drops keys that now have their colour, or that the keyboard no longer lists", () => {
    const draft = map([[W, "#0000ff"], [A, "#00ff00"], [200, "#ffffff"]]);
    const now = new Map(current);
    now.set(W, "#0000ff");
    expect([...normalize(draft, now)]).toEqual([[A, "#00ff00"]]);
    const kept = map([[A, "#00ff00"]]);
    expect(normalize(kept, now)).toBe(kept);
  });

  it("lists its changes in key order", () => {
    const draft = map([[D, "#0000ff"], [W, "#0000ff"], [A, "#ff0000"]]);
    expect(draftChanges(draft, current)).toEqual([
      { key: W, before: "#ff0000", after: "#0000ff" },
      { key: D, before: "#ff0000", after: "#0000ff" },
    ]);
  });

  it("compares drafts by content", () => {
    expect(sameColors(map([[W, "#0000ff"]]), map([[W, "#0000ff"]]))).toBe(true);
    expect(sameColors(map([[W, "#0000ff"]]), map([[W, "#00ff00"]]))).toBe(false);
    expect(sameColors(map([[W, "#0000ff"]]), map([]))).toBe(false);
  });
});

describe("undo and redo", () => {
  const one = map([[W, "#0000ff"]]);
  const two = map([[W, "#0000ff"], [A, "#0000ff"]]);

  it("records edits, undoes and redoes them", () => {
    let history: ColorHistory = EMPTY_HISTORY;
    history = record(history, one);
    history = record(history, two);
    expect(history.present).toBe(two);
    expect(history.past).toHaveLength(2);
    history = undo(history);
    expect(history.present).toBe(one);
    history = undo(history);
    expect(history.present.size).toBe(0);
    expect(undo(history)).toBe(history);
    history = redo(history);
    expect(history.present).toBe(one);
    history = redo(history);
    expect(history.present).toBe(two);
    expect(redo(history)).toBe(history);
  });

  it("records nothing for an edit that changes nothing, and a new edit drops the redo steps", () => {
    const history = record(EMPTY_HISTORY, one);
    expect(record(history, map([[W, "#0000ff"]]))).toBe(history);
    const undone = undo(history);
    const edited = record(undone, two);
    expect(edited.future).toEqual([]);
  });

  it("keeps a limited number of steps", () => {
    let history: ColorHistory = EMPTY_HISTORY;
    for (let i = 0; i < HISTORY_LIMIT + 20; i += 1) {
      history = record(history, map([[W, `#0000${(i % 256).toString(16).padStart(2, "0")}`]]));
    }
    expect(history.past).toHaveLength(HISTORY_LIMIT);
  });

  it("follows new keyboard colours in every step", () => {
    const state = readyState();
    const now = keyboardColors(state);
    now.set(W, "#0000ff");
    const history = record(record(EMPTY_HISTORY, one), two);
    const next = normalizeHistory(history, now);
    expect([...next.present]).toEqual([[A, "#0000ff"]]);
    expect(next.past.map((step) => step.size)).toEqual([0, 0]);
    expect(normalizeHistory(EMPTY_HISTORY, now)).toBe(EMPTY_HISTORY);
  });
});

describe("gradients", () => {
  it("mixes colours in sRGB", () => {
    expect(mixHex("#000000", "#ffffff", 0)).toBe("#000000");
    expect(mixHex("#000000", "#ffffff", 1)).toBe("#ffffff");
    expect(mixHex("#ff0000", "#0000ff", 0.5)).toBe("#800080");
    expect(mixHex("#ff0000", "#0000ff", 2)).toBe("#0000ff");
  });

  it("runs from the left to the right of the selection", () => {
    const colors = gradientColors([ESC, 14, 7], "#ff0000", "#0000ff", "horizontal");
    expect(colors.get(ESC)).toBe("#ff0000");
    expect(colors.get(14)).toBe("#0000ff");
    const middle = colors.get(7) ?? "";
    expect(middle).not.toBe("#ff0000");
    expect(middle).not.toBe("#0000ff");
  });

  it("runs from the top to the bottom, keys in one row getting one colour", () => {
    const colors = gradientColors([W, 46 - 22, A, RIGHT, 124], "#ffffff", "#000000", "vertical");
    expect(colors.get(24)).toBe("#ffffff");
    expect(colors.get(RIGHT)).toBe("#000000");
    expect(colors.get(124)).toBe(colors.get(RIGHT));
    // The knob and keys off the board are left out.
    expect(gradientColors([15, 300], "#ffffff", "#000000", "vertical").size).toBe(0);
  });

  it("gives a single position the start colour", () => {
    expect([...gradientColors([W], "#FF0000", "#0000ff", "horizontal")]).toEqual([[W, "#ff0000"]]);
  });
});

describe("the intent", () => {
  it("is null without changes", () => {
    const state = readyState();
    expect(buildColorIntent(state, new Map(), { switchToCustom: false, checkPersistence: false })).toBeNull();
    expect(buildColorIntent(state, map([[W, "#ff0000"]]), { switchToCustom: false, checkPersistence: false })).toBeNull();
  });

  it("sends the changed keys in key order, based on the colours' revision (the recorded intent)", () => {
    const state = readyState();
    const draft = paint(new Map(), keyboardColors(state), [D, S, A, W], "#0000ff");
    expect(buildColorIntent(state, draft, { switchToCustom: false, checkPersistence: false })).toEqual({
      based_on: 1,
      check_persistence: false,
      colors: [
        { color: "#0000ff", key: W },
        { color: "#0000ff", key: A },
        { color: "#0000ff", key: S },
        { color: "#0000ff", key: D },
      ],
      switch_to_custom: false,
    });
  });

  it("switches to Custom only when asked, when not shown and when the light allows it", () => {
    const state = readyState();
    const draft = map([[W, "#00ff00"]]);
    // Custom is on: nothing to switch.
    expect(switchApplies(state, true)).toBe(false);
    expect(buildColorIntent(state, draft, { switchToCustom: true, checkPersistence: false })?.switch_to_custom).toBe(false);
    const wave: ColorsState = { ...state, shown_now: false, light_mode: { id: 2, key: "wave" } };
    expect(buildColorIntent(wave, draft, { switchToCustom: false, checkPersistence: true })).toMatchObject({
      switch_to_custom: false,
      check_persistence: true,
    });
    expect(buildColorIntent(wave, draft, { switchToCustom: true, checkPersistence: false })?.switch_to_custom).toBe(true);
    expect(switchApplies({ ...wave, can_switch_to_custom: false }, true)).toBe(false);
  });
});

describe("the review", () => {
  const key = (index: number, name: string) => ({ index, name, row: Math.floor(index / 22), col: index % 22 });
  const changes: ColorChangeView[] = [
    { key: key(D, "D"), before: "#ff0000", after: "#0000ff" },
    { key: key(W, "W"), before: "#ff0000", after: "#0000ff" },
    { key: key(ESC, "Esc"), before: "#00ff00", after: "#ffffff" },
  ];

  it("groups changes by old and new colour, largest group first", () => {
    const groups = groupChanges(changes);
    expect(groups).toHaveLength(2);
    expect(groups[0]?.keys.map((k) => k.name)).toEqual(["W", "D"]);
    expect(groups[1]).toMatchObject({ before: "#00ff00", after: "#ffffff" });
  });

  it("builds the boards before and after", () => {
    const current = keyboardColors(readyState());
    const before = beforeColors(current, changes);
    const after = afterColors(current, changes);
    expect(before.get(ESC)).toBe("#00ff00");
    expect(after.get(ESC)).toBe("#ffffff");
    expect(after.get(W)).toBe("#0000ff");
    expect(after.get(S)).toBe("#ff0000");
    expect(colorCounts(after)).toEqual([
      { color: "#ff0000", count: 78 },
      { color: "#0000ff", count: 2 },
      { color: "#ffffff", count: 1 },
    ]);
  });
});

describe("colour names", () => {
  it("names colours roughly, for screen readers", () => {
    const cases: [string, string][] = [
      ["#ff0000", "red"],
      ["#ff8000", "orange"],
      ["#ffff00", "yellow"],
      ["#80ff00", "lime"],
      ["#00ff00", "green"],
      ["#00ffff", "cyan"],
      ["#0000ff", "blue"],
      ["#8000ff", "purple"],
      ["#ff00ff", "magenta"],
      ["#ff0080", "pink"],
      ["#804000", "brown"],
      ["#000080", "dark blue"],
      ["#8080ff", "light blue"],
      ["#808080", "grey"],
      ["#ffffff", "white"],
      ["#000000", "black (off)"],
    ];
    for (const [hex, name] of cases) expect(colorNameText(en, hex)).toBe(name);
    expect(colorName("#101010")).toEqual({ kind: "black" });
  });

  it("says the colour and its hex value", () => {
    expect(colorText(en, "#ff0000")).toBe("red, #FF0000");
    expect(colorText(tr, "#000080")).toBe("koyu mavi, #000080");
    expect(colorText(tr, "#000000")).toBe("siyah (kapalı), #000000");
  });
});

describe("texts", () => {
  it("lists keys with a limit", () => {
    expect(keysListText(en, ["W", "A", "S", "D"])).toBe("W, A, S, D");
    expect(keysListText(en, ["Esc", "F1", "F2"], 2)).toBe("Escape, F1 and 1 more");
    expect(keysListText(tr, ["Esc", "F1", "F2"], 2)).toBe("Escape, F1 ve 1 tuş daha");
  });

  it("uses short cap legends", () => {
    expect(legendText(en, "L-Shift")).toBe("SHIFT");
    expect(legendText(en, "W")).toBe("W");
  });

  it("explains every refusal of a colour edit", () => {
    const key = { index: W, row: 2, col: 2, name: "W" };
    expect(editProblemText(en, { problem: "knob" })).toBe(en.lighting.colors.problems.knob);
    expect(editProblemText(en, { problem: "duplicate_key", key })).toBe("W was given twice.");
    expect(editProblemText(tr, { problem: "too_many_keys", max: 81 })).toContain("81");
    expect(
      editProblemText(en, { problem: "unsupported_current_table", table: { table: "custom_colors", number: 1 }, reason: "x" }),
    ).toContain(en.flow.tables.custom_colors({ n: 1 }));
    expect(editProblemText(en, { problem: "other", message: "x" })).toBe(en.lighting.colors.problems.other);
  });

  it("words the plan's warnings and leaves out other editors' warnings", () => {
    expect(warningText(en, { code: "not_shown_until_custom_mode" })).toBe(en.lighting.colors.review.notShownBody);
    expect(warningText(en, { code: "unverified", note: { what: "x", question: "Q1" } })).toBe(
      en.lighting.colors.review.unverified,
    );
    expect(warningText(en, { code: "socd_hotkey" })).toBeNull();
  });
});

describe("why writing is blocked", () => {
  const ready = FIXTURES["colors_ready"]?.initial_status;
  if (ready === undefined) throw new Error("no status");

  it("is not blocked when ready and editable", () => {
    expect(colorsBlock(ready, false, false, readyState())).toBeNull();
  });

  it("names the first reason", () => {
    const lost: DeviceStatus = {
      ...ready,
      connection: { state: "lost", error: { kind: "disconnected", detail: null, message: "" } },
      write_gate: { allowed: false, reasons: ["not_connected"] },
    };
    expect(colorsBlock(lost, false, false, readyState())).toEqual({ kind: "gate", reason: "not_connected" });
    expect(colorsBlock(ready, true, false, readyState())).toEqual({ kind: "busy" });
    expect(colorsBlock(ready, false, true, readyState())).toEqual({ kind: "gate", reason: "snapshot_stale" });
    expect(colorsBlock(ready, false, false, { ...readyState(), editable: false })).toEqual({ kind: "not_editable" });
    expect(colorsBlock(ready, false, false, null)).toEqual({ kind: "not_editable" });
  });
});

describe("preview drafts", () => {
  it("takes valid entries only", () => {
    expect(draftFromPreview(undefined)).toEqual({ colors: new Map(), switchToCustom: false });
    const seed = draftFromPreview({ colors: { "46": "#0000FF", "68": "nope", x: "#ffffff", "999": "#ffffff" }, switchToCustom: true });
    expect([...seed.colors]).toEqual([[W, "#0000ff"]]);
    expect(seed.switchToCustom).toBe(true);
  });
});
