// The sticky bar under an editor: what will change, Discard, and "Review
// and write…". Pages keep a local draft and write only through this bar
// (which opens the confirmation flow), never while a control is dragged.

import { useEffect, useRef, type ReactNode } from "react";

import { useDict } from "../i18n";
import { Button } from "./Button";

/** Props of {@link ApplyBar}. */
export interface ApplyBarProps {
  /** What the draft changes ("4 keys will change · 1 left out"); `null` when there is no draft. */
  summary: ReactNode;
  /** The draft changes something (Review and write… can be used). */
  dirty: boolean;
  /** There is a draft to discard (default: `dirty`). */
  canDiscard?: boolean | undefined;
  onDiscard: () => void;
  onApply: () => void;
  /** When set, "Review and write…" is disabled and this text says why. */
  disabledReason?: string | null | undefined;
  /** The apply button's label (default "Review and write…"). */
  applyLabel?: string | undefined;
}

/** The sticky apply bar of an editor page. */
export function ApplyBar({
  summary,
  dirty,
  canDiscard,
  onDiscard,
  onApply,
  disabledReason,
  applyLabel,
}: ApplyBarProps) {
  const dict = useDict();
  const ref = useRef<HTMLDivElement>(null);

  // The bar is sticky at the bottom of the page's scroll container: the
  // container keeps that much room (`scroll-padding-bottom` in shell.css),
  // so a control that gets the focus is scrolled above the bar, never under
  // it. The bar's height changes when it wraps.
  useEffect(() => {
    const bar = ref.current;
    const scroller = bar?.closest<HTMLElement>(".ow-main") ?? null;
    if (bar === null || scroller === null) return undefined;
    const update = () => {
      scroller.style.setProperty("--applybar-height", `${String(bar.offsetHeight)}px`);
    };
    update();
    const observer = typeof ResizeObserver === "function" ? new ResizeObserver(update) : null;
    observer?.observe(bar);
    return () => {
      observer?.disconnect();
      scroller.style.removeProperty("--applybar-height");
    };
  }, []);

  return (
    <div ref={ref} className="ow-applybar" role="region" aria-label={applyLabel ?? dict.common.actions.reviewAndWrite}>
      <div className="ow-applybar__summary" aria-live="polite">
        {summary}
      </div>
      <div className="ow-applybar__actions">
        <Button onClick={onDiscard} disabled={!(canDiscard ?? dirty)}>
          {dict.common.actions.discard}
        </Button>
        <Button
          variant="primary"
          onClick={onApply}
          disabled={!dirty}
          disabledReason={dirty ? (disabledReason ?? null) : null}
        >
          {applyLabel ?? dict.common.actions.reviewAndWrite}
        </Button>
      </div>
    </div>
  );
}
