// The visual W75: absolutely positioned keys inside a multi-select listbox.
//
// Mouse: click toggles, double-click selects only that key, Shift + click
// adds the rectangle from the last clicked key, dragging (4 px or more)
// draws a marquee that adds keys (Alt + drag removes; Esc cancels).
// Keyboard: arrows move the focus spatially, Shift + arrow also adds,
// Space toggles, Ctrl + A selects all, Ctrl + I inverts, Esc clears.
// The knob is drawn as a circle and can never be selected.

import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

import { KEYBOARD_HEIGHT_U, KEYBOARD_WIDTH_U, KNOB_INDEX, W75_GEOMETRY } from "./geometry";
import {
  ALL_KEYS,
  addRange,
  applyMarquee,
  invert,
  moveFocus,
  only,
  selectAll,
  toggle,
  type Direction,
  type Selection,
  type UnitRect,
} from "./selection";

/** What a key shows. */
export interface KeyVisual {
  /** The short legend on the cap ("W", "Shift"). */
  legend: string;
  /** The accessible name ("W", "Left Shift"). */
  name: string;
  /** The value on the cap ("1,20"); none when omitted. */
  value?: string | null | undefined;
  /** A small chip ("RT"). */
  chip?: string | null | undefined;
  /** A marker: a pending draft (●) or a key that can't be edited (⚠). */
  marker?: "pending" | "warning" | null | undefined;
  /** Extra spoken text ("Now 1,70 mm → 1,20 mm, not written yet"), given as the key's accessible description. */
  description?: string | null | undefined;
  /** A page-defined mark, rendered as `data-flag` on the key (the page styles it). */
  flag?: boolean | undefined;
  /** The key's tooltip (e.g. why it can't be changed). */
  tooltip?: string | null | undefined;
  /** Show the value in the accent colour (a draft preview). */
  emphasis?: boolean | undefined;
  /** Whether the key can be selected (default true; the knob never). */
  selectable?: boolean | undefined;
}

/** Props of {@link KeyboardView}. */
export interface KeyboardViewProps {
  /** The accessible name of the listbox. */
  label: string;
  /** What each key shows (called for every key index of the W75). */
  keyVisual: (index: number) => KeyVisual;
  selected: Selection;
  onSelectionChange: (next: Set<number>) => void;
  /** The knob's accessible name and tooltip ("The knob has no trigger settings"). */
  knobLabel: string;
  /** Read-only: shows values, but selection does not change. */
  disabled?: boolean | undefined;
  /** Fixed size of one key unit in pixels (default: from the width, 32-56 px). */
  unitPx?: number | undefined;
}

const DRAG_THRESHOLD_PX = 4;

function unitFor(width: number): number {
  return Math.min(56, Math.max(32, Math.floor(width / 16.5)));
}

interface DragState {
  pointerId: number;
  startX: number;
  startY: number;
  originLeft: number;
  originTop: number;
  mode: "add" | "remove";
  base: Selection;
  key: number | null;
  shift: boolean;
  marquee: UnitRect | null;
}

/** The W75 as a selectable listbox of keys. */
export function KeyboardView({
  label,
  keyVisual,
  selected,
  onSelectionChange,
  knobLabel,
  disabled = false,
  unitPx,
}: KeyboardViewProps) {
  const idPrefix = useId();
  const wrapRef = useRef<HTMLDivElement>(null);
  const boardRef = useRef<HTMLDivElement>(null);
  const keyRefs = useRef(new Map<number, HTMLDivElement>());
  const [measured, setMeasured] = useState(44);
  const [focus, setFocus] = useState<number>(ALL_KEYS[0] ?? 0);
  const [anchor, setAnchor] = useState<number | null>(null);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [preview, setPreview] = useState<Set<number> | null>(null);
  const unit = unitPx ?? measured;

  useEffect(() => {
    const element = wrapRef.current;
    if (element === null || unitPx !== undefined || typeof ResizeObserver === "undefined") {
      return undefined;
    }
    const observer = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width;
      if (width !== undefined && width > 0) setMeasured(unitFor(width));
    });
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [unitPx]);

  const visuals = useMemo(() => {
    const map = new Map<number, KeyVisual>();
    for (const shape of W75_GEOMETRY) map.set(shape.index, keyVisual(shape.index));
    return map;
  }, [keyVisual]);

  const selectable = useMemo(() => {
    const set = new Set<number>();
    for (const index of ALL_KEYS) {
      if (visuals.get(index)?.selectable !== false) set.add(index);
    }
    return set;
  }, [visuals]);

  const shown = preview ?? selected;

  const change = (next: Set<number>) => {
    if (!disabled) onSelectionChange(next);
  };

  const focusKey = (index: number) => {
    setFocus(index);
    keyRefs.current.get(index)?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const ctrl = event.ctrlKey || event.metaKey;
    const arrows: Record<string, Direction> = {
      ArrowUp: "up",
      ArrowDown: "down",
      ArrowLeft: "left",
      ArrowRight: "right",
    };
    const direction = arrows[event.key];
    if (direction !== undefined) {
      event.preventDefault();
      const next = moveFocus(focus, direction);
      focusKey(next);
      if (event.shiftKey && !selected.has(next)) change(toggle(selected, next, selectable));
      return;
    }
    if (event.key === " ") {
      event.preventDefault();
      change(toggle(selected, focus, selectable));
      setAnchor(focus);
    } else if (ctrl && event.key.toLowerCase() === "a") {
      event.preventDefault();
      change(selectAll(selectable));
    } else if (ctrl && event.key.toLowerCase() === "i") {
      event.preventDefault();
      change(invert(selected, selectable));
    } else if (event.key === "Escape") {
      if (drag !== null) {
        setDrag(null);
        setPreview(null);
      } else if (selected.size > 0) {
        event.preventDefault();
        change(new Set());
      }
    }
  };

  const keyAt = (target: EventTarget | null): number | null => {
    if (!(target instanceof Element)) return null;
    const element = target.closest<HTMLElement>("[data-key-index]");
    const value = element?.dataset["keyIndex"];
    return value === undefined ? null : Number(value);
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (disabled || event.button !== 0) return;
    const board = boardRef.current;
    if (board === null) return;
    const box = board.getBoundingClientRect();
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Pointer capture is best effort.
    }
    setDrag({
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      originLeft: box.left,
      originTop: box.top,
      mode: event.altKey ? "remove" : "add",
      base: selected,
      key: keyAt(event.target),
      shift: event.shiftKey,
      marquee: null,
    });
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (drag === null || drag.pointerId !== event.pointerId) return;
    const moved = Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY);
    if (drag.marquee === null && moved < DRAG_THRESHOLD_PX) return;
    const rect: UnitRect = {
      x0: (drag.startX - drag.originLeft) / unit,
      y0: (drag.startY - drag.originTop) / unit,
      x1: (event.clientX - drag.originLeft) / unit,
      y1: (event.clientY - drag.originTop) / unit,
    };
    setDrag({ ...drag, marquee: rect });
    setPreview(applyMarquee(drag.base, rect, drag.mode, selectable));
  };

  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    if (drag === null || drag.pointerId !== event.pointerId) return;
    const finished = drag;
    setDrag(null);
    if (finished.marquee !== null) {
      const next = applyMarquee(finished.base, finished.marquee, finished.mode, selectable);
      setPreview(null);
      change(next);
      return;
    }
    const index = finished.key;
    if (index === null) return;
    setFocus(index);
    if (finished.shift && anchor !== null) {
      change(addRange(selected, anchor, index, selectable));
    } else {
      change(toggle(selected, index, selectable));
      setAnchor(index);
    }
  };

  const onPointerCancel = () => {
    setDrag(null);
    setPreview(null);
  };

  const marquee = drag?.marquee ?? null;

  return (
    <div ref={wrapRef} className={`ow-keyboard-wrap${disabled ? " ow-keyboard-wrap--disabled" : ""}`}>
      <div
        ref={boardRef}
        className="ow-keyboard"
        role="listbox"
        aria-label={label}
        aria-multiselectable="true"
        aria-disabled={disabled || undefined}
        style={{ width: KEYBOARD_WIDTH_U * unit, height: KEYBOARD_HEIGHT_U * unit }}
        onKeyDown={onKeyDown}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerCancel}
        onDoubleClick={(event) => {
          // With pointer capture the event may target the board: fall back
          // to the key the last click toggled.
          const index = keyAt(event.target) ?? anchor;
          if (index !== null && index !== KNOB_INDEX) change(only(index, selectable));
        }}
      >
        {W75_GEOMETRY.map((shape) => {
          const visual = visuals.get(shape.index);
          if (visual === undefined) return null;
          const isKnob = shape.shape === "knob" || shape.index === KNOB_INDEX;
          const isSelected = !isKnob && shown.has(shape.index);
          // The name holds the visible text (value, chip); the longer
          // description is the accessible description.
          const spoken = [visual.name, visual.value, visual.chip].filter(Boolean).join(", ");
          const hasDescription = !isKnob && typeof visual.description === "string" && visual.description !== "";
          const descriptionId = `${idPrefix}-d${shape.index}`;
          const tooltip = isKnob ? knobLabel : (visual.tooltip ?? undefined);
          return (
            <div
              key={shape.index}
              ref={(element) => {
                if (element === null) keyRefs.current.delete(shape.index);
                else keyRefs.current.set(shape.index, element);
              }}
              data-key-index={shape.index}
              role="option"
              aria-selected={isSelected}
              aria-disabled={isKnob || !selectable.has(shape.index) || undefined}
              aria-label={isKnob ? `${visual.name}: ${knobLabel}` : spoken}
              aria-describedby={hasDescription ? descriptionId : undefined}
              title={tooltip}
              data-flag={!isKnob && visual.flag === true ? "true" : undefined}
              tabIndex={focus === shape.index ? 0 : -1}
              className={[
                "ow-key",
                isKnob ? "ow-key--knob" : "",
                isSelected ? "ow-key--selected" : "",
                visual.emphasis === true ? "ow-key--emphasis" : "",
                visual.marker === "warning" ? "ow-key--warning" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              style={{
                left: shape.x * unit + 2,
                top: shape.y * unit + 2,
                width: shape.w * unit - 4,
                height: shape.h * unit - 4,
              }}
              onFocus={() => {
                setFocus(shape.index);
              }}
            >
              {isKnob ? null : (
                <span aria-hidden="true" className="ow-key__content">
                  <span className="ow-key__legend">{visual.legend}</span>
                  {visual.value === undefined || visual.value === null ? null : (
                    <span className="ow-key__value ow-num">{visual.value}</span>
                  )}
                  {visual.chip === undefined || visual.chip === null ? null : (
                    <span className="ow-key__chip">{visual.chip}</span>
                  )}
                  {visual.marker === "pending" ? <span className="ow-key__marker">●</span> : null}
                  {visual.marker === "warning" ? <span className="ow-key__marker">⚠</span> : null}
                  {isSelected ? <span className="ow-key__check">✓</span> : null}
                </span>
              )}

            </div>
          );
        })}
        {/* The keys' descriptions, outside the keys so that a key's text is only what it shows. */}
        <div hidden>
          {W75_GEOMETRY.map((shape) => {
            const description = visuals.get(shape.index)?.description;
            if (shape.index === KNOB_INDEX || typeof description !== "string" || description === "") return null;
            return (
              <span key={shape.index} id={`${idPrefix}-d${shape.index}`}>
                {description}
              </span>
            );
          })}
        </div>
        {marquee === null ? null : (
          <div
            className="ow-keyboard__marquee"
            aria-hidden="true"
            style={{
              left: Math.min(marquee.x0, marquee.x1) * unit,
              top: Math.min(marquee.y0, marquee.y1) * unit,
              width: Math.abs(marquee.x1 - marquee.x0) * unit,
              height: Math.abs(marquee.y1 - marquee.y0) * unit,
            }}
          />
        )}
      </div>
    </div>
  );
}
