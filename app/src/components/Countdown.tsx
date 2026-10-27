// Time helpers: the milliseconds since a component mounted (ticking), and a
// countdown text built on it. To restart either, remount the component
// (give it a new `key`).

import { useEffect, useState } from "react";

import { useFormat } from "../i18n/format";

/** Milliseconds since mount, updated every `intervalMs` while `active`. */
export function useElapsed(intervalMs = 100, active = true): number {
  const [times, setTimes] = useState(() => {
    const now = performance.now();
    return { start: now, now };
  });
  useEffect(() => {
    if (!active) {
      return undefined;
    }
    const timer = window.setInterval(() => {
      setTimes((previous) => ({ ...previous, now: performance.now() }));
    }, intervalMs);
    return () => {
      window.clearInterval(timer);
    };
  }, [active, intervalMs]);
  return Math.max(0, times.now - times.start);
}

/** Props of {@link Countdown}. */
export interface CountdownProps {
  /** Milliseconds left when the countdown appears. */
  remainingMs: number;
  /** Formats the remaining time (default: "2 s"). */
  render?: ((seconds: string) => string) | undefined;
}

/** Whole seconds left, counting down to zero ("2 s" / "2 sn"). Remount to restart. */
export function Countdown({ remainingMs, render }: CountdownProps) {
  const format = useFormat();
  const elapsed = useElapsed(250, remainingMs > 0);
  const seconds = format.seconds(Math.max(0, remainingMs - elapsed));
  return <span className="ow-num">{render === undefined ? seconds : render(seconds)}</span>;
}
