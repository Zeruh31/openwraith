// Distances, colours and locale formatting.

import { describe, expect, it } from "vitest";

import { contrastRatio, hexToRgb, isHexColor, normalizeHex } from "../format/color";
import { clampUnits, formatMmNumber, parseMmText, unitsToMm } from "../format/units";
import { makeFormat } from "../i18n/format";

describe("trigger units", () => {
  it("convert to millimetres", () => {
    expect(unitsToMm(85)).toBeCloseTo(1.7);
    expect(formatMmNumber(85, ".")).toBe("1.70");
    expect(formatMmNumber(85, ",")).toBe("1,70");
    expect(formatMmNumber(15, ".")).toBe("0.30");
    expect(formatMmNumber(170, ".")).toBe("3.40");
  });

  it("parse typed millimetres with either decimal mark", () => {
    expect(parseMmText("1,20")).toEqual({ ok: true, units: 60, rounded: false });
    expect(parseMmText("1.2")).toEqual({ ok: true, units: 60, rounded: false });
    expect(parseMmText("1,2")).toEqual({ ok: true, units: 60, rounded: false });
    expect(parseMmText(" 1.20 mm ")).toEqual({ ok: true, units: 60, rounded: false });
    expect(parseMmText(".5")).toEqual({ ok: true, units: 25, rounded: false });
  });

  it("round to 0.02 mm and say so", () => {
    expect(parseMmText("1.21")).toEqual({ ok: true, units: 61, rounded: true });
    expect(parseMmText("1,229")).toEqual({ ok: true, units: 61, rounded: true });
  });

  it("reject text that isn't a distance", () => {
    expect(parseMmText("abc").ok).toBe(false);
    expect(parseMmText("").ok).toBe(false);
    expect(parseMmText("-1").ok).toBe(false);
    expect(parseMmText("1.2.3").ok).toBe(false);
  });

  it("clamp to limits", () => {
    expect(clampUnits(3, 5, 170)).toEqual({ units: 5, clamped: "min" });
    expect(clampUnits(200, 5, 170)).toEqual({ units: 170, clamped: "max" });
    expect(clampUnits(60, 5, 170)).toEqual({ units: 60, clamped: null });
  });
});

describe("locale formatting", () => {
  it("formats millimetres and seconds per language", () => {
    expect(makeFormat("en").mm(85)).toBe("1.70 mm");
    expect(makeFormat("tr").mm(85)).toBe("1,70 mm");
    expect(makeFormat("en").seconds(2000)).toBe("2 s");
    expect(makeFormat("tr").seconds(1500)).toBe("2 sn");
    expect(makeFormat("tr").seconds(-5)).toBe("0 sn");
  });

  it("formats lists and USB ids", () => {
    expect(makeFormat("en").list(["1", "2", "3"])).toBe("1, 2 and 3");
    expect(makeFormat("tr").list(["1", "2", "3"])).toBe("1, 2 ve 3");
    expect(makeFormat("en").hex4(11836)).toBe("2E3C");
  });

  it("keeps unparseable dates as they are", () => {
    expect(makeFormat("en").date("not a date")).toBe("not a date");
  });
});

describe("colours", () => {
  it("normalise hex colours", () => {
    expect(normalizeHex("#ABC")).toBe("#aabbcc");
    expect(normalizeHex("ff8800")).toBe("#ff8800");
    expect(normalizeHex(" #FF8800 ")).toBe("#ff8800");
    expect(normalizeHex("#ff88")).toBeNull();
    expect(normalizeHex("red")).toBeNull();
    expect(isHexColor("#ff8800")).toBe(true);
    expect(isHexColor("#FF8800")).toBe(false);
    expect(hexToRgb("#0a6c96")).toEqual({ r: 10, g: 108, b: 150 });
  });

  it("compute WCAG contrast", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21);
    expect(contrastRatio("#777777", "#777777")).toBeCloseTo(1);
  });
});
