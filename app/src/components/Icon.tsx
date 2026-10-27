// OpenWraith's own line icons (24 × 24, stroke = currentColor).

import type { ReactElement } from "react";

const PATHS = {
  check: <path d="M5 12.5l4.5 4.5L19 7.5" />,
  "check-double": (
    <>
      <path d="M2.5 12.5l4 4L15 8" />
      <path d="M9.5 15.5l1 1L21 8" />
    </>
  ),
  warning: (
    <>
      <path d="M12 3.5L22 20.5H2z" />
      <path d="M12 10v5" />
      <path d="M12 17.8v.2" />
    </>
  ),
  error: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M9 9l6 6M15 9l-6 6" />
    </>
  ),
  info: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v6" />
      <path d="M12 7.8v.2" />
    </>
  ),
  keyboard: (
    <>
      <rect x="2.5" y="6" width="19" height="12" rx="2" />
      <path d="M6 10h1M9.5 10h1M13 10h1M16.5 10h1M6 14h1M8.5 14h7M17 14h1" />
    </>
  ),
  light: (
    <>
      <path d="M9 18h6M10 21h4" />
      <path d="M12 3a6 6 0 0 0-3.5 10.9c.6.5 1 1.2 1 2.1h5c0-.9.4-1.6 1-2.1A6 6 0 0 0 12 3z" />
    </>
  ),
  backup: (
    <>
      <path d="M4 15v4a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-4" />
      <path d="M12 4v11M7.5 10.5L12 15l4.5-4.5" />
    </>
  ),
  restore: (
    <>
      <path d="M4 15v4a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-4" />
      <path d="M12 15V4M7.5 8.5L12 4l4.5 4.5" />
    </>
  ),
  undo: (
    <>
      <path d="M9 14L4 9l5-5" />
      <path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11" />
    </>
  ),
  folder: <path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2.5h8.5A1.5 1.5 0 0 1 21 9v9.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5z" />,
  refresh: (
    <>
      <path d="M20 12a8 8 0 1 1-2.3-5.6" />
      <path d="M20 4v5h-5" />
    </>
  ),
  close: <path d="M6 6l12 12M18 6L6 18" />,
  chevron: <path d="M9 6l6 6-6 6" />,
  stop: <rect x="6.5" y="6.5" width="11" height="11" rx="1.5" />,
  plug: (
    <>
      <path d="M9 3v5M15 3v5" />
      <path d="M6.5 8h11v3a5.5 5.5 0 0 1-11 0z" />
      <path d="M12 16.5V21" />
    </>
  ),
  profile: (
    <>
      <rect x="4" y="4" width="7" height="7" rx="1.5" />
      <rect x="13" y="4" width="7" height="7" rx="1.5" />
      <rect x="4" y="13" width="7" height="7" rx="1.5" />
      <rect x="13" y="13" width="7" height="7" rx="1.5" />
    </>
  ),
  settings: (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1" />
    </>
  ),
  device: (
    <>
      <rect x="3" y="7" width="18" height="10" rx="2" />
      <circle cx="17.5" cy="10" r="1.2" />
      <path d="M6 13.5h8" />
    </>
  ),
  sim: (
    <>
      <rect x="6" y="6" width="12" height="12" rx="2" />
      <path d="M9.5 9.5h5v5h-5zM9 3v3M15 3v3M9 18v3M15 18v3M3 9h3M3 15h3M18 9h3M18 15h3" />
    </>
  ),
  lock: (
    <>
      <rect x="5" y="10.5" width="14" height="10" rx="2" />
      <path d="M8 10.5V8a4 4 0 0 1 8 0v2.5" />
    </>
  ),
  minus: <path d="M6 12h12" />,
  equal: <path d="M6 9.5h12M6 14.5h12" />,
  knob: (
    <>
      <circle cx="12" cy="12" r="8" />
      <path d="M12 4v4" />
    </>
  ),
} satisfies Record<string, ReactElement>;

/** The names of the icons. */
export type IconName = keyof typeof PATHS;

/** Props of {@link Icon}. */
export interface IconProps {
  name: IconName;
  /** Pixel size (default 18). */
  size?: number | undefined;
  /** An accessible name; without it the icon is decorative. */
  title?: string | undefined;
  className?: string | undefined;
}

/** A line icon. Decorative unless `title` is given. */
export function Icon({ name, size = 18, title, className }: IconProps) {
  return (
    <svg
      className={className === undefined ? "ow-icon" : `ow-icon ${className}`}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      role={title === undefined ? undefined : "img"}
      aria-hidden={title === undefined ? true : undefined}
      aria-label={title}
      focusable="false"
    >
      {PATHS[name]}
    </svg>
  );
}
