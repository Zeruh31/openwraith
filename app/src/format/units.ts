// Trigger distances: the keyboard counts in units of 0.02 mm (20 µm). Every
// value in the IPC is an integer number of units; millimetres are only for
// display and typing.

/** The distance unit the app can write (0.02 mm). */
export const DEFAULT_UNIT_UM = 20;

/** `units` in millimetres. */
export function unitsToMm(units: number, unitUm: number = DEFAULT_UNIT_UM): number {
  return (units * unitUm) / 1000;
}

/** `units` as a millimetre number with two decimals and the given decimal separator ("1.70"). */
export function formatMmNumber(
  units: number,
  decimalSeparator: "." | ",",
  unitUm: number = DEFAULT_UNIT_UM,
): string {
  const text = unitsToMm(units, unitUm).toFixed(2);
  return decimalSeparator === "." ? text : text.replace(".", ",");
}

/** The result of parsing a typed distance. */
export type ParsedMm =
  | { ok: true; units: number; rounded: boolean }
  | { ok: false };

/**
 * Parses a typed distance in millimetres ("1.2", "1,2", "1,20", "1.20 mm")
 * into whole units, rounding to the nearest unit.
 */
export function parseMmText(text: string, unitUm: number = DEFAULT_UNIT_UM): ParsedMm {
  const cleaned = text.trim().replace(/\s*mm$/i, "").trim().replace(",", ".");
  if (!/^(\d+(\.\d*)?|\.\d+)$/.test(cleaned)) {
    return { ok: false };
  }
  const millimetres = Number(cleaned);
  if (!Number.isFinite(millimetres)) {
    return { ok: false };
  }
  const exact = (millimetres * 1000) / unitUm;
  const units = Math.round(exact);
  return { ok: true, units, rounded: Math.abs(exact - units) > 1e-6 };
}

/** Which limit a value was clamped to. */
export type Clamped = "min" | "max" | null;

/** `units` clamped to `min..=max`. */
export function clampUnits(units: number, min: number, max: number): { units: number; clamped: Clamped } {
  if (units < min) {
    return { units: min, clamped: "min" };
  }
  if (units > max) {
    return { units: max, clamped: "max" };
  }
  return { units, clamped: null };
}
