// The visual W75: absolutely positioned keys inside a listbox (multi-select
// by default, or single-select).
//
// Multi-select: click toggles, double-click selects only that key, Shift +
// click adds the rectangle from the last clicked key, dragging (4 px or
// more) draws a marquee that adds keys (Alt + drag removes; Esc cancels).
// Keyboard: arrows move the focus spatially, Shift + arrow also adds,
// Space toggles, Ctrl + A selects all, Ctrl + I inverts, Esc clears.
// Single-select: click or Space selects that key only (no marquee, no
// ranges); Esc clears.
// Both: Enter or a double-click "activates" a key (`onActivate`, e.g. to
// open the action picker). The knob is drawn as a circle and can never be
// selected; with `knobInfo` it opens a small popover (click, Enter, Space).

import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from "react";

import { useDict } from "../../i18n";

import { Icon } from "../Icon";
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
  /** A second, smaller line under the legend (e.g. the key's original legend when it is remapped). */
  sublegend?: string | null | undefined;
  /** A colour shown on the cap ("#rrggbb"); say it in `description` too ("red, #ff0000"). */
  swatch?: string | null | undefined;
  /** How far the cap is filled from the top, 0..1 (live key depth). */
  fill?: number | null | undefined;
  /** A short tag in the cap's corner ("M3", "DKS"); the full meaning belongs in `description`. */
  badge?: string | null | undefined;
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
  /** `multi` (default) or `single` selection. */
  selection?: "multi" | "single" | undefined;
  /** Enter or a double-click on a selectable key (after selecting it). */
  onActivate?: ((index: number) => void) | undefined;
  /** What the knob stores, shown in a popover from the knob (it stays unselectable). */
  knobInfo?: ReactNode | undefined;
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
  selection = "multi",
  onActivate,
  knobInfo,
}: KeyboardViewProps) {
  const dict = useDict();
  const idPrefix = useId();
  const single = selection === "single";
  const [knobOpen, setKnobOpen] = useState(false);
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
      if (!single && event.shiftKey && !selected.has(next)) change(toggle(selected, next, selectable));
      return;
    }
    if (focus === KNOB_INDEX && knobInfo !== undefined && (event.key === " " || event.key === "Enter")) {
      event.preventDefault();
      setKnobOpen((open) => !open);
      return;
    }
    if (event.key === "Enter") {
      if (selectable.has(focus) && !disabled) {
        event.preventDefault();
        if (!selected.has(focus)) change(single ? only(focus, selectable) : toggle(selected, focus, selectable));
        onActivate?.(focus);
      }
      return;
    }
    if (event.key === " ") {
      event.preventDefault();
      change(single ? only(focus, selectable) : toggle(selected, focus, selectable));
      setAnchor(focus);
    } else if (single) {
      if (event.key === "Escape") {
        if (knobOpen) {
          event.preventDefault();
          setKnobOpen(false);
        } else if (selected.size > 0) {
          event.preventDefault();
          change(new Set());
        }
      }
    } else if (ctrl && event.key.toLowerCase() === "a") {
      event.preventDefault();
      change(selectAll(selectable));
    } else if (ctrl && event.key.toLowerCase() === "i") {
      event.preventDefault();
      change(invert(selected, selectable));
    } else if (event.key === "Escape") {
      if (knobOpen) {
        event.preventDefault();
        setKnobOpen(false);
      } else if (drag !== null) {
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
    if (single || drag === null || drag.pointerId !== event.pointerId) return;
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
    if (index === KNOB_INDEX) {
      if (knobInfo !== undefined) setKnobOpen((open) => !open);
      return;
    }
    if (single) {
      if (selectable.has(index)) change(only(index, selectable));
      setAnchor(index);
    } else if (finished.shift && anchor !== null) {
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
      {/* The board and, beside it (never inside the listbox), the knob's popover. */}
      <div className="ow-keyboard-stage" style={{ width: KEYBOARD_WIDTH_U * unit }}>
        <div
          ref={boardRef}
          className="ow-keyboard"
          role="listbox"
          aria-label={label}
          aria-multiselectable={single ? "false" : "true"}
          aria-disabled={disabled || undefined}
          style={{ width: KEYBOARD_WIDTH_U * unit, height: KEYBOARD_HEIGHT_U * unit }}
          onKeyDown={onKeyDown}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerCancel}
          onDoubleClick={(event) => {
            // With pointer capture the event may target the board itself: fall
            // back to the key the last click toggled (only then).
            const index = keyAt(event.target) ?? (event.target === event.currentTarget ? anchor : null);
            if (index === null || index === KNOB_INDEX) return;
            change(only(index, selectable));
            if (selectable.has(index) && !disabled) onActivate?.(index);
          }}
        >
          {W75_GEOMETRY.map((shape) => {
            const visual = visuals.get(shape.index);
            if (visual === undefined) return null;
            const isKnob = shape.shape === "knob" || shape.index === KNOB_INDEX;
            const isSelected = !isKnob && shown.has(shape.index);
            // The name starts with the visible text (legend, value, chip, in
            // that order, so speech input finds the key by what it shows); a
            // legend that is not the key's full name ("ESC", "⌫", a Turkish
            // name for an English legend) is followed by the name. The longer
            // text is the accessible description.
            const named =
              visual.legend.toLowerCase() === visual.name.toLowerCase()
                ? visual.name
                : `${visual.legend} (${visual.name})`;
            const spoken = [named, visual.value, visual.chip].filter(Boolean).join(" ");
            const hasDescription = !isKnob && typeof visual.description === "string" && visual.description !== "";
            const descriptionId = `${idPrefix}-d${shape.index}`;
            const tooltip = isKnob ? knobLabel : (visual.tooltip ?? undefined);
            const fill = typeof visual.fill === "number" ? Math.min(1, Math.max(0, visual.fill)) : null;
            const knobPopup = isKnob && knobInfo !== undefined;
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
                aria-haspopup={knobPopup ? "dialog" : undefined}
                aria-expanded={knobPopup ? knobOpen : undefined}
                aria-controls={knobPopup && knobOpen ? `${idPrefix}-knob` : undefined}
                title={tooltip}
                data-flag={!isKnob && visual.flag === true ? "true" : undefined}
                tabIndex={focus === shape.index ? 0 : -1}
                className={[
                  "ow-key",
                  isKnob ? "ow-key--knob" : "",
                  isSelected ? "ow-key--selected" : "",
                  visual.emphasis === true ? "ow-key--emphasis" : "",
                  visual.marker === "warning" ? "ow-key--warning" : "",
                  knobPopup ? "ow-key--knob-info" : "",
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
                {isKnob ? (
                  knobPopup ? (
                    <span aria-hidden="true" className="ow-key__knob-mark">
                      <Icon name="info" size={14} />
                    </span>
                  ) : null
                ) : (
                  <span aria-hidden="true" className="ow-key__content">
                    {fill === null ? null : (
                      <span className="ow-key__fill" style={{ height: `${Math.round(fill * 100)}%` }} />
                    )}
                    {visual.swatch === undefined || visual.swatch === null ? null : (
                      <span className="ow-key__swatch" style={{ backgroundColor: visual.swatch }} />
                    )}
                    <span className="ow-key__legend">{visual.legend}</span>
                    {visual.sublegend === undefined || visual.sublegend === null ? null : (
                      <span className="ow-key__sublegend">{visual.sublegend}</span>
                    )}
                    {visual.badge === undefined || visual.badge === null ? null : (
                      <span className="ow-key__badge">{visual.badge}</span>
                    )}
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
        {knobInfo !== undefined && knobOpen ? (
          <KnobPopover
            id={`${idPrefix}-knob`}
            title={dict.common.pickers.keyboard.knobTitle}
            closeLabel={dict.common.actions.close}
            unit={unit}
            onClose={() => {
              setKnobOpen(false);
              keyRefs.current.get(KNOB_INDEX)?.focus();
            }}
          >
            {knobInfo}
          </KnobPopover>
        ) : null}
      </div>
    </div>
  );
}

/** Props of {@link KnobPopover}. */
interface KnobPopoverProps {
  id: string;
  title: string;
  closeLabel: string;
  unit: number;
  onClose: () => void;
  children: ReactNode;
}

/** The knob's information next to the knob (non-modal; Esc, the close button or a click outside closes it). */
function KnobPopover({ id, title, closeLabel, unit, onClose, children }: KnobPopoverProps) {
  const ref = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  useEffect(() => {
    closeRef.current = onClose;
  });
  useEffect(() => {
    const onPointer = (event: globalThis.PointerEvent) => {
      const target = event.target;
      if (ref.current === null || !(target instanceof Node) || ref.current.contains(target)) return;
      const onKnob = target instanceof Element && target.closest(`[data-key-index="${KNOB_INDEX}"]`) !== null;
      if (!onKnob) closeRef.current();
    };
    document.addEventListener("pointerdown", onPointer);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
    };
  }, []);
  const knob = W75_GEOMETRY.find((shape) => shape.index === KNOB_INDEX);
  const top = knob === undefined ? 0 : (knob.y + knob.h) * unit + 4;
  return (
    <div
      ref={ref}
      id={id}
      role="dialog"
      aria-label={title}
      className="ow-knob-popover"
      style={{ top }}
      onPointerDown={(event) => {
        event.stopPropagation();
      }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Escape") {
          event.preventDefault();
          onClose();
        }
      }}
    >
      <div className="ow-knob-popover__header">
        <p className="ow-strong">{title}</p>
        <button type="button" className="ow-knob-popover__close" aria-label={closeLabel} onClick={onClose}>
          <Icon name="close" size={14} />
        </button>
      </div>
      <div className="ow-knob-popover__body">{children}</div>
    </div>
  );
}
