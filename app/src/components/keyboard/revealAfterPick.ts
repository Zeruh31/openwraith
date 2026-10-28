// After a key is picked on a board with the pointer, the panel that shows
// (or acts on) the selection may be below the window's fold (narrow windows
// stack it under the board): bring it into view. Arrow-key moves inside the
// board never scroll: the board stays where the user is working.

import { useRef } from "react";

/** How long after a pointer press a selection change counts as made by the pointer (ms). */
const POINTER_MS = 1000;

/** Scrolls `element` into view (nearest edge; no animation under reduced motion). */
export function revealNearest(element: HTMLElement | null): void {
  if (element === null || typeof element.scrollIntoView !== "function") return;
  const reduce = typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  element.scrollIntoView({ block: "nearest", behavior: reduce ? "auto" : "smooth" });
}

/** Tracks pointer presses on a board; `byPointer()` tells whether the current selection change came from one. */
export function usePointerPick(): { onPointerDownCapture: () => void; byPointer: () => boolean } {
  const at = useRef<number | null>(null);
  return {
    onPointerDownCapture: () => {
      at.current = Date.now();
    },
    byPointer: () => {
      const last = at.current;
      at.current = null;
      return last !== null && Date.now() - last < POINTER_MS;
    },
  };
}
