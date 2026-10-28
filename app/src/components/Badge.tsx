// The three markers of values (one component, text plus tooltip, never
// colour alone):
// - "Not yet verified": the library's `Evidence::Unverified` (passed through
//   the DTOs; never a list kept in the frontend);
// - "Kept as read": a value OpenWraith shows but never changes (knob slots,
//   preserved entries, unknown slot contents, non-layout colour slots);
// - "Applies to all profiles": a setting of the whole keyboard (the SOCD
//   power), a confirmed fact.

import { useId } from "react";

import { useDict, type Dict } from "../i18n";
import type { EvidenceView, UnverifiedNoteView } from "../ipc/bindings";
import { Icon, type IconName } from "./Icon";

/** The kinds of markers. */
export type BadgeKind = "unverified" | "kept" | "keyboard_wide";

/** Props of {@link Badge}. */
export interface BadgeProps {
  kind: BadgeKind;
  /** Replaces the default tooltip. */
  tooltip?: string | undefined;
  /** What isn't verified yet (the library's notes), added to the tooltip. */
  notes?: readonly UnverifiedNoteView[] | undefined;
  /** A smaller badge (on keycaps and in dense lists). */
  size?: "sm" | "md" | undefined;
}

const ICONS: Record<BadgeKind, IconName> = {
  unverified: "info",
  kept: "lock",
  keyboard_wide: "keyboard",
};

/**
 * What a library note says isn't tested yet, in the UI language (never the
 * library's English text or its research question id).
 */
export function unverifiedWhat(note: UnverifiedNoteView, dict: Dict): string {
  const notes: Readonly<Record<string, string | undefined>> = dict.common.badge.notes;
  return notes[note.what] ?? dict.common.badge.noteOther;
}

/** The tooltip text of a badge. */
export function badgeTooltip(
  kind: BadgeKind,
  dict: Dict,
  notes: readonly UnverifiedNoteView[] = [],
): string {
  const b = dict.common.badge;
  switch (kind) {
    case "unverified": {
      const known = [...new Set(notes.map((note) => unverifiedWhat(note, dict)))].filter((what) => what !== b.noteOther);
      if (known.length === 0) return b.unverifiedTip;
      return `${b.unverifiedTip} ${b.unverifiedNotes({ list: known.join("; ") })}`;
    }
    case "kept":
      return b.keptTip;
    case "keyboard_wide":
      return b.keyboardWideTip;
  }
}

/**
 * A marker with its text and a tooltip. The tooltip is the badge's
 * description (`aria-describedby` to a hidden element), never part of its
 * name: inside a button or an option the control's name stays short
 * ("Right Ctrl, Not yet verified").
 */
export function Badge({ kind, tooltip, notes, size = "md" }: BadgeProps) {
  const dict = useDict();
  const id = useId();
  const b = dict.common.badge;
  const text = kind === "unverified" ? b.unverified : kind === "kept" ? b.kept : b.keyboardWide;
  const tip = tooltip ?? badgeTooltip(kind, dict, notes);
  return (
    <span className={`ow-badge ow-badge--${kind} ow-badge--${size}`} title={tip} data-badge={kind} aria-describedby={id}>
      <Icon name={ICONS[kind]} size={size === "sm" ? 12 : 14} />
      <span>{text}</span>
      <span id={id} hidden>
        {tip}
      </span>
    </span>
  );
}

/** Props of {@link EvidenceBadge}. */
export interface EvidenceBadgeProps {
  /** The library's evidence; `null` for a value kept as read. */
  evidence: EvidenceView | null | undefined;
  /** Show "Kept as read" when there is no evidence (a value OpenWraith doesn't decode). */
  keptWhenNone?: boolean | undefined;
  size?: "sm" | "md" | undefined;
}

/**
 * "Not yet verified" exactly when the evidence says unverified; "Kept as
 * read" for a value without evidence when asked; nothing otherwise.
 */
export function EvidenceBadge({ evidence, keptWhenNone = false, size }: EvidenceBadgeProps) {
  if (evidence === null || evidence === undefined) {
    return keptWhenNone ? <Badge kind="kept" size={size} /> : null;
  }
  return evidence.status === "unverified" ? <Badge kind="unverified" notes={evidence.notes} size={size} /> : null;
}
