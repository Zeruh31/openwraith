// Locale-aware formatting: millimetres, numbers, dates, durations, lists.

import { formatMmNumber, parseMmText, type ParsedMm } from "../format/units";
import { getLang, useLang } from "./index";
import type { Lang } from "./types";

/** Formatters for one language. */
export interface Format {
  /** "1.70 mm" / "1,70 mm" from trigger units. */
  mm(units: number, unitUm?: number): string;
  /** "1.70" / "1,70" from trigger units (no unit). */
  mmNumber(units: number, unitUm?: number): string;
  /** Parses typed millimetres ("1,2", "1.20 mm") into units. */
  parseMm(text: string, unitUm?: number): ParsedMm;
  /** A whole number with grouping ("8,000" / "8.000"). */
  number(value: number): string;
  /** A date ("27 Oct 2026" / "27 Eki 2026"). */
  date(iso: string): string;
  /** A time of day ("14:02:05"). */
  time(iso: string): string;
  /** A date and time ("27 Oct 2026, 14:02"). */
  dateTime(iso: string): string;
  /** A date and time with seconds ("27 Oct 2026, 14:02:05"), for items made in the same minute. */
  dateTimeSeconds(iso: string): string;
  /** A duration in whole seconds, rounded up ("2 s" / "2 sn"). */
  seconds(ms: number): string;
  /** A list ("1, 2 and 3" / "1, 2 ve 3"). */
  list(items: readonly string[]): string;
  /** A 4-digit hexadecimal USB ID ("2E3C"). */
  hex4(value: number): string;
}

const LOCALES: Record<Lang, string> = { en: "en-GB", tr: "tr-TR" };

function parseDate(iso: string): Date | null {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? null : date;
}

/** The formatters for `lang`. */
export function makeFormat(lang: Lang): Format {
  const locale = LOCALES[lang];
  const separator = lang === "tr" ? "," : ".";
  const numberFormat = new Intl.NumberFormat(locale);
  const dateFormat = new Intl.DateTimeFormat(locale, { dateStyle: "medium" });
  const timeFormat = new Intl.DateTimeFormat(locale, { timeStyle: "medium" });
  const dateTimeFormat = new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" });
  const dateTimeSecondsFormat = new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "medium" });
  const listFormat = new Intl.ListFormat(locale, { style: "long", type: "conjunction" });
  return {
    mm: (units, unitUm) => `${formatMmNumber(units, separator, unitUm)} mm`,
    mmNumber: (units, unitUm) => formatMmNumber(units, separator, unitUm),
    parseMm: (text, unitUm) => parseMmText(text, unitUm),
    number: (value) => numberFormat.format(value),
    date: (iso) => {
      const date = parseDate(iso);
      return date === null ? iso : dateFormat.format(date);
    },
    time: (iso) => {
      const date = parseDate(iso);
      return date === null ? iso : timeFormat.format(date);
    },
    dateTime: (iso) => {
      const date = parseDate(iso);
      return date === null ? iso : dateTimeFormat.format(date);
    },
    dateTimeSeconds: (iso) => {
      const date = parseDate(iso);
      return date === null ? iso : dateTimeSecondsFormat.format(date);
    },
    seconds: (ms) => {
      const seconds = Math.max(0, Math.ceil(ms / 1000));
      return lang === "tr" ? `${seconds} sn` : `${seconds} s`;
    },
    list: (items) => listFormat.format(items),
    hex4: (value) => value.toString(16).toUpperCase().padStart(4, "0"),
  };
}

const cache = new Map<Lang, Format>();

function formatFor(lang: Lang): Format {
  let format = cache.get(lang);
  if (format === undefined) {
    format = makeFormat(lang);
    cache.set(lang, format);
  }
  return format;
}

/** The formatters of the current language; re-renders when it changes. */
export function useFormat(): Format {
  return formatFor(useLang());
}

/** The formatters of the current language (outside React). */
export function getFormat(): Format {
  return formatFor(getLang());
}
