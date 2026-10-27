// OpenWraith's own line icons for the lighting effects (24 × 24, stroke =
// currentColor). Decorative: the effect's name is always shown next to it.

import type { ReactElement } from "react";

import type { LightModeKey } from "../../ipc/bindings";

const PATHS: Record<LightModeKey, ReactElement> = {
  static: (
    <>
      <rect x="4.5" y="4.5" width="15" height="15" rx="3" />
      <rect x="8.5" y="8.5" width="7" height="7" rx="1.5" fill="currentColor" stroke="none" />
    </>
  ),
  breath: (
    <>
      <circle cx="12" cy="12" r="3" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="6" opacity="0.7" />
      <circle cx="12" cy="12" r="9.5" opacity="0.35" />
    </>
  ),
  wave: (
    <>
      <path d="M2.5 9c2.4-3.2 4.8-3.2 7.2 0s4.8 3.2 7.2 0 3.2-2.4 4.6-1.6" />
      <path d="M2.5 15c2.4-3.2 4.8-3.2 7.2 0s4.8 3.2 7.2 0 3.2-2.4 4.6-1.6" opacity="0.5" />
    </>
  ),
  neon: (
    <>
      <path d="M5 7h10.5a3.5 3.5 0 0 1 0 7H8.5a3.5 3.5 0 0 0 0 7H19" />
      <path d="M3 4.5l1.5 1M21 18l-1.5-1" opacity="0.5" />
    </>
  ),
  radar: (
    <>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="4.5" opacity="0.5" />
      <path d="M12 12L18.4 5.6" />
      <circle cx="12" cy="12" r="1.2" fill="currentColor" stroke="none" />
    </>
  ),
  reactive: (
    <>
      <rect x="8.5" y="8.5" width="7" height="7" rx="1.5" />
      <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M18.7 5.3l-2.1 2.1M7.4 16.6l-2.1 2.1" />
    </>
  ),
  aurora: (
    <>
      <path d="M6 3.5c-2.5 4 2.5 8.5 0 17" />
      <path d="M12 3.5c-2.5 4 2.5 8.5 0 17" opacity="0.7" />
      <path d="M18 3.5c-2.5 4 2.5 8.5 0 17" opacity="0.45" />
    </>
  ),
  ripple: (
    <>
      <circle cx="12" cy="17" r="1.4" fill="currentColor" stroke="none" />
      <path d="M8 17a4 4 0 0 1 8 0" />
      <path d="M4.5 17a7.5 7.5 0 0 1 15 0" opacity="0.7" />
      <path d="M1.5 17a10.5 10.5 0 0 1 21 0" opacity="0.4" />
    </>
  ),
  twinkle: (
    <>
      <path d="M10 3.5l1.6 5.4 5.4 1.6-5.4 1.6L10 17.5l-1.6-5.4L3 10.5l5.4-1.6z" />
      <path d="M18 15l.8 2.2 2.2.8-2.2.8L18 21l-.8-2.2-2.2-.8 2.2-.8z" />
    </>
  ),
  custom: (
    <>
      <rect x="3.5" y="3.5" width="7.5" height="7.5" rx="1.5" fill="currentColor" stroke="none" />
      <rect x="13" y="3.5" width="7.5" height="7.5" rx="1.5" />
      <rect x="3.5" y="13" width="7.5" height="7.5" rx="1.5" />
      <rect x="13" y="13" width="7.5" height="7.5" rx="1.5" fill="currentColor" stroke="none" opacity="0.5" />
    </>
  ),
  cross: (
    <>
      <path d="M12 3v18M3 12h18" />
      <path d="M9.5 5.5L12 3l2.5 2.5M9.5 18.5L12 21l2.5-2.5M5.5 9.5L3 12l2.5 2.5M18.5 9.5L21 12l-2.5 2.5" />
    </>
  ),
  speed_respond: (
    <>
      <path d="M3.5 17a8.5 8.5 0 1 1 17 0" />
      <path d="M12 17l4.5-5.5" />
      <circle cx="12" cy="17" r="1.2" fill="currentColor" stroke="none" />
      <path d="M6 12.5l1.2.8M12 8.5v1.4M18 12.5l-1.2.8" opacity="0.6" />
    </>
  ),
  auto_ripple: (
    <>
      <circle cx="12" cy="12" r="2" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="5.5" opacity="0.6" />
      <path d="M20.5 12a8.5 8.5 0 1 1-2.5-6" />
      <path d="M18.5 2.8V6.4H14.9" />
    </>
  ),
  striation: (
    <>
      <rect x="3.5" y="3.5" width="17" height="17" rx="3" />
      <path d="M3.8 13.5l9.7-9.7M7.5 20.3L20.3 7.5M14 20.2l6.2-6.2" />
    </>
  ),
  fireworks: (
    <>
      <circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none" />
      <path d="M12 2.5v4M12 17.5v4M2.5 12h4M17.5 12h4M5.3 5.3l2.8 2.8M15.9 15.9l2.8 2.8M18.7 5.3l-2.8 2.8M8.1 15.9l-2.8 2.8" />
    </>
  ),
  music_rhythm: <path d="M5 14v6M9.5 9v11M14 5v15M18.5 11v9" />,
  unknown: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M9.6 9.4a2.5 2.5 0 1 1 3.6 2.2c-.7.4-1.2 1-1.2 1.8v.6" />
      <path d="M12 17v.3" />
    </>
  ),
};

/** The icon of a lighting effect (decorative). */
export function ModeIcon({ mode, size = 24 }: { mode: LightModeKey; size?: number | undefined }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className="li-mode-icon"
    >
      {PATHS[mode]}
    </svg>
  );
}
