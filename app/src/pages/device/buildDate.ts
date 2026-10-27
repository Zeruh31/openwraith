// The firmware's build date: the keyboard reports C's `__DATE__,__TIME__`
// ("Aug  6 2026,13:46:54", English month, a double space before one-digit
// days). It is shown in the UI's language; anything else is shown tidied.

import type { Format } from "../../i18n/format";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** The build date as a local date and time, or `null` when it isn't in the expected form. */
export function parseBuildDate(raw: string): Date | null {
  const match = /^\s*([A-Za-z]{3})\s+(\d{1,2})\s+(\d{4})\s*,?\s*(\d{1,2}):(\d{2}):(\d{2})\s*$/.exec(raw);
  if (match === null) return null;
  const [, monthName, day, year, hour, minute, second] = match;
  const month = MONTHS.findIndex((m) => m.toLowerCase() === monthName?.toLowerCase());
  if (month < 0) return null;
  const date = new Date(
    Number(year),
    month,
    Number(day),
    Number(hour),
    Number(minute),
    Number(second),
  );
  // Reject impossible dates (Feb 31 rolls over).
  return date.getMonth() === month && date.getDate() === Number(day) ? date : null;
}

/** The build date for display ("6 Aug 2026, 13:46" / "6 Ağu 2026 13:46"). */
export function buildDateText(raw: string, format: Format): string {
  const date = parseBuildDate(raw);
  if (date !== null) return format.dateTime(date.toISOString());
  // Unknown form: whitespace collapsed, a space after the comma.
  return raw.replace(/\s+/g, " ").replace(/\s*,\s*/g, ", ").trim();
}
