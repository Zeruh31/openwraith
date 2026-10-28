// Keeps the keyboard focus where it was while a save disables the controls:
// a focused control that becomes disabled drops the focus to <body>, and a
// keyboard or screen-reader user would have to start over from the top.

import { useEffect, useRef } from "react";

/**
 * While `busy` the controls are disabled; when it ends, the element that
 * had the focus when it began gets it back (if the focus fell to <body>
 * meanwhile and the element is still in the document and enabled).
 */
export function useRestoreFocus(busy: boolean): void {
  const last = useRef<HTMLElement | null>(null);
  const saved = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const remember = (event: FocusEvent) => {
      if (event.target instanceof HTMLElement) last.current = event.target;
    };
    document.addEventListener("focusin", remember);
    return () => {
      document.removeEventListener("focusin", remember);
    };
  }, []);

  useEffect(() => {
    if (busy) {
      saved.current = last.current;
      return;
    }
    const element = saved.current;
    saved.current = null;
    if (element === null || !element.isConnected) return;
    const active = document.activeElement;
    if (active !== null && active !== document.body) return;
    if (element instanceof HTMLButtonElement && element.disabled) return;
    element.focus();
  }, [busy]);
}
