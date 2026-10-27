// Small feedback elements: spinner, progress bar, callout, empty state,
// unverified marker.

import type { ReactNode } from "react";

import { useDict } from "../i18n";
import { Icon, type IconName } from "./Icon";

/** Props of {@link Spinner}. */
export interface SpinnerProps {
  /** Read by screen readers. */
  label?: string | undefined;
  size?: number | undefined;
}

/** A spinning ring (a static ring with reduced motion). */
export function Spinner({ label, size = 16 }: SpinnerProps) {
  return (
    <span className="ow-spinner" role={label === undefined ? undefined : "status"}>
      <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="3" opacity="0.25" />
        <path d="M21 12a9 9 0 0 0-9-9" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
      </svg>
      {label === undefined ? null : <span className="sr-only">{label}</span>}
    </span>
  );
}

/** Props of {@link ProgressBar}. */
export interface ProgressBarProps {
  /** Done so far; `null` for an indeterminate bar. */
  value: number | null;
  max?: number | undefined;
  /** The accessible name. */
  label: string;
  /** The accessible value text ("45/113"). */
  valueText?: string | undefined;
  tone?: "accent" | "success" | "warning" | undefined;
}

/** A determinate (or indeterminate) progress bar. */
export function ProgressBar({ value, max = 100, label, valueText, tone = "accent" }: ProgressBarProps) {
  const ratio = value === null || max <= 0 ? null : Math.min(1, Math.max(0, value / max));
  return (
    <div
      className={`ow-progress ow-progress--${tone}${ratio === null ? " ow-progress--indeterminate" : ""}`}
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={ratio === null ? undefined : max}
      aria-valuenow={ratio === null || value === null ? undefined : value}
      aria-valuetext={valueText}
    >
      <div className="ow-progress__bar" style={{ width: ratio === null ? undefined : `${ratio * 100}%` }} />
    </div>
  );
}

/** The kinds of callouts. */
export type CalloutKind = "info" | "warning" | "danger" | "success" | "sim";

const CALLOUT_ICONS: Record<CalloutKind, IconName> = {
  info: "info",
  warning: "warning",
  danger: "error",
  success: "check",
  sim: "sim",
};

/** Props of {@link Callout}. */
export interface CalloutProps {
  kind?: CalloutKind | undefined;
  title?: ReactNode | undefined;
  children?: ReactNode | undefined;
  /** Buttons shown under the text. */
  actions?: ReactNode | undefined;
  /** `alert` for messages that must interrupt; default none (static text). */
  role?: "alert" | "status" | undefined;
  className?: string | undefined;
}

/** A boxed message with an icon, a title, text and optional actions. */
export function Callout({ kind = "info", title, children, actions, role, className }: CalloutProps) {
  return (
    <div className={`ow-callout ow-callout--${kind}${className === undefined ? "" : ` ${className}`}`} role={role}>
      <Icon name={CALLOUT_ICONS[kind]} className="ow-callout__icon" size={20} />
      <div className="ow-callout__content">
        {title === undefined ? null : <p className="ow-callout__title">{title}</p>}
        {children === undefined ? null : <div className="ow-callout__body">{children}</div>}
        {actions === undefined ? null : <div className="ow-callout__actions">{actions}</div>}
      </div>
    </div>
  );
}

/** Props of {@link EmptyState}. */
export interface EmptyStateProps {
  icon?: IconName | undefined;
  title: ReactNode;
  children?: ReactNode | undefined;
  action?: ReactNode | undefined;
}

/** A placeholder for an empty list or an unavailable view. */
export function EmptyState({ icon = "info", title, children, action }: EmptyStateProps) {
  return (
    <div className="ow-empty">
      <Icon name={icon} size={28} className="ow-empty__icon" />
      <p className="ow-empty__title">{title}</p>
      {children === undefined ? null : <div className="ow-empty__body">{children}</div>}
      {action === undefined ? null : <div className="ow-empty__action">{action}</div>}
    </div>
  );
}

/** Marks a fact that comes from the vendor's code and isn't confirmed on the keyboard. */
export function UnverifiedMarker() {
  const dict = useDict();
  return (
    <span className="ow-unverified" title={dict.common.unverified.tooltip}>
      <Icon name="info" size={14} />
      <span>{dict.common.unverified.label}</span>
      <span className="sr-only">{dict.common.unverified.tooltip}</span>
    </span>
  );
}
