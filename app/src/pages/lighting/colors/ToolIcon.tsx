// OpenWraith's own line icons for the key-colour tools (24 × 24, stroke =
// currentColor). Decorative: the tool's name is always shown next to it.

import type { ReactElement } from "react";

/** The icons of the key-colour tab. */
export type ToolIconName = "select" | "brush" | "eyedropper" | "redo" | "fill" | "gradient";

const PATHS: Record<ToolIconName, ReactElement> = {
  select: (
    <>
      <path d="M6 3.5l12 7.5-5.2 1.2L10.5 18z" />
      <path d="M12.8 12.2l4.2 5.3" />
    </>
  ),
  brush: (
    <>
      <path d="M20 4l-8.5 8.5" />
      <path d="M11.5 12.5c-2.3-.6-4.5.6-5 3.1-.3 1.6-1 2.6-2.5 3.2 2.8 1.3 6.9.9 8.4-1.6.8-1.4.4-3.3-.9-4.7z" />
    </>
  ),
  eyedropper: (
    <>
      <path d="M14.5 4.5l5 5" />
      <path d="M17.5 3a2.1 2.1 0 0 1 3 3l-2.5 2.5-3-3z" />
      <path d="M15 8.5L6.5 17 5 21l-.5-.5L8 19l8.5-8.5" />
    </>
  ),
  redo: (
    <>
      <path d="M15 14l5-5-5-5" />
      <path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13" />
    </>
  ),
  fill: (
    <>
      <rect x="4.5" y="4.5" width="15" height="15" rx="3" />
      <rect x="8" y="8" width="8" height="8" rx="1.5" fill="currentColor" stroke="none" />
    </>
  ),
  gradient: (
    <>
      <rect x="3.5" y="6.5" width="17" height="11" rx="2" />
      <path d="M7 6.5v11M10.5 6.5v11" opacity="0.8" />
      <path d="M14 6.5v11" opacity="0.5" />
      <path d="M17.5 6.5v11" opacity="0.25" />
    </>
  ),
};

/** A decorative tool icon. */
export function ToolIcon({ name, size = 18 }: { name: ToolIconName; size?: number }) {
  return (
    <svg
      className="ow-icon li-tool-icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {PATHS[name]}
    </svg>
  );
}
