// A colour picker for key colours: a saturation/brightness square and a hue
// slider (both `role="slider"`, usable with the arrow keys: Shift for steps
// of 10, Home/End), a hex field, quick swatches and the recently used
// colours. Every change only reports the colour (a draft); nothing is
// written from here. Swatches keep their colour in Windows high contrast.

import { useId, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

import { normalizeHex } from "../../format/color";
import { useDict } from "../../i18n";
import { createStore, useStore } from "../../state/store";
import { hexToHsv, hsvToHex, type Hsv } from "./hsv";

const RECENT_KEY = "openwraith.recentColors";
const RECENT_MAX = 8;

function loadRecent(): string[] {
  try {
    const raw = window.localStorage.getItem(RECENT_KEY);
    const parsed: unknown = raw === null ? [] : JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map((item) => (typeof item === "string" ? normalizeHex(item) : null))
      .filter((item): item is string => item !== null)
      .slice(0, RECENT_MAX);
  } catch {
    return [];
  }
}

/** The recently used colours (newest first; remembered in this browser, best effort). */
export const recentColors = createStore<readonly string[]>(loadRecent());

/** Puts a colour first in the recent colours. */
export function rememberColor(hex: string): void {
  const color = normalizeHex(hex);
  if (color === null) return;
  recentColors.set((list) => {
    const next = [color, ...list.filter((item) => item !== color)].slice(0, RECENT_MAX);
    try {
      window.localStorage.setItem(RECENT_KEY, JSON.stringify(next));
    } catch {
      // Storage may be unavailable; the list is only a convenience.
    }
    return next;
  });
}

/** A quick colour. */
export interface ColorChoice {
  hex: string;
  name: string;
}

/** Props of {@link ColorPicker}. */
export interface ColorPickerProps {
  /** The group's accessible name ("Colour for the selected keys"). */
  label: string;
  /** "#rrggbb". */
  value: string;
  /** Every change (also while dragging). */
  onChange: (hex: string) => void;
  swatches?: readonly ColorChoice[] | undefined;
  /** Show and fill the recent colours (default true). */
  recent?: boolean | undefined;
  disabled?: boolean | undefined;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** The colour picker. */
export function ColorPicker({ label, value, onChange, swatches = [], recent = true, disabled = false }: ColorPickerProps) {
  const dict = useDict();
  const t = dict.common.pickers.color;
  const id = useId();
  const recentList = useStore(recentColors);
  // The controls' HSV with the colour it stands for: a value set from
  // outside (another key selected, undo) moves the controls, and the hue of
  // greys (which have none) is kept.
  const [shown, setShown] = useState<{ hex: string; hsv: Hsv }>(() => ({ hex: value, hsv: hexToHsv(value) }));
  const hsv = shown.hex === value ? shown.hsv : hexToHsv(value, shown.hsv.h);
  const setHsv = (next: Hsv) => {
    setShown({ hex: hsvToHex(next), hsv: next });
  };
  const [hexDraft, setHexDraft] = useState<string | null>(null);
  const [invalid, setInvalid] = useState(false);
  const areaRef = useRef<HTMLDivElement>(null);
  const hueRef = useRef<HTMLDivElement>(null);
  const dragging = useRef<"area" | "hue" | null>(null);
  const changedByKeys = useRef(false);

  const set = (next: Hsv) => {
    setHsv(next);
    const hex = hsvToHex(next);
    if (hex !== value) onChange(hex);
  };

  const pick = (hex: string) => {
    setInvalid(false);
    setHsv(hexToHsv(hex, hsv.h));
    if (hex !== value) onChange(hex);
    if (recent) rememberColor(hex);
  };

  const fromPointer = (event: PointerEvent<HTMLDivElement>, target: "area" | "hue") => {
    const element = target === "area" ? areaRef.current : hueRef.current;
    if (element === null) return;
    const box = element.getBoundingClientRect();
    const x = box.width > 0 ? clamp((event.clientX - box.left) / box.width, 0, 1) : 0;
    const y = box.height > 0 ? clamp((event.clientY - box.top) / box.height, 0, 1) : 0;
    if (target === "area") set({ ...hsv, s: x, v: 1 - y });
    else set({ ...hsv, h: Math.min(359, x * 360) });
  };

  function onPointerDown(event: PointerEvent<HTMLDivElement>, target: "area" | "hue") {
    if (disabled || event.button !== 0) return;
    dragging.current = target;
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Pointer capture is best effort.
    }
    fromPointer(event, target);
  }

  function onPointerMove(event: PointerEvent<HTMLDivElement>, target: "area" | "hue") {
    if (dragging.current === target) fromPointer(event, target);
  }

  function onPointerUp(target: "area" | "hue") {
    if (dragging.current === target && recent) rememberColor(hsvToHex(hsv));
    dragging.current = null;
  }

  const onAreaKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? 0.1 : 0.01;
    let next: Hsv | null = null;
    if (event.key === "ArrowRight") next = { ...hsv, s: clamp(hsv.s + step, 0, 1) };
    if (event.key === "ArrowLeft") next = { ...hsv, s: clamp(hsv.s - step, 0, 1) };
    if (event.key === "ArrowUp") next = { ...hsv, v: clamp(hsv.v + step, 0, 1) };
    if (event.key === "ArrowDown") next = { ...hsv, v: clamp(hsv.v - step, 0, 1) };
    if (event.key === "Home") next = { ...hsv, s: 0 };
    if (event.key === "End") next = { ...hsv, s: 1 };
    if (next !== null && !disabled) {
      event.preventDefault();
      changedByKeys.current = true;
      set(next);
    }
  };

  const onHueKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? 10 : 1;
    let h: number | null = null;
    if (event.key === "ArrowRight" || event.key === "ArrowUp") h = Math.min(359, Math.round(hsv.h) + step);
    if (event.key === "ArrowLeft" || event.key === "ArrowDown") h = Math.max(0, Math.round(hsv.h) - step);
    if (event.key === "Home") h = 0;
    if (event.key === "End") h = 359;
    if (h !== null && !disabled) {
      event.preventDefault();
      changedByKeys.current = true;
      set({ ...hsv, h });
    }
  };

  const onSliderBlur = () => {
    if (changedByKeys.current && recent) rememberColor(hsvToHex(hsv));
    changedByKeys.current = false;
  };

  const commitHex = () => {
    if (hexDraft === null) return;
    const hex = normalizeHex(hexDraft);
    setHexDraft(null);
    if (hex === null) {
      setInvalid(true);
      return;
    }
    pick(hex);
  };

  const s = Math.round(hsv.s * 100);
  const v = Math.round(hsv.v * 100);
  const hue = Math.round(hsv.h);
  const pure = hsvToHex({ h: hsv.h, s: 1, v: 1 });

  const swatchButton = (hex: string, name: string, key: string) => (
    <button
      key={key}
      type="button"
      className="ow-colorpicker__swatch"
      style={{ backgroundColor: hex }}
      aria-label={name === hex ? hex : `${name}, ${hex}`}
      aria-pressed={hex === value}
      title={name === hex ? hex : `${name} (${hex})`}
      disabled={disabled}
      onClick={() => {
        pick(hex);
      }}
    />
  );

  return (
    <div className={`ow-colorpicker${disabled ? " ow-colorpicker--disabled" : ""}`} role="group" aria-label={label}>
      <div className="ow-colorpicker__main">
        <div
          ref={areaRef}
          className="ow-colorpicker__area"
          style={{ backgroundColor: pure }}
          role="slider"
          tabIndex={disabled ? -1 : 0}
          aria-label={t.area}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={s}
          aria-valuetext={t.areaValue({ s, v })}
          aria-disabled={disabled || undefined}
          onKeyDown={onAreaKey}
          onBlur={onSliderBlur}
          onPointerDown={(event) => {
            onPointerDown(event, "area");
          }}
          onPointerMove={(event) => {
            onPointerMove(event, "area");
          }}
          onPointerUp={() => {
            onPointerUp("area");
          }}
        >
          <span className="ow-colorpicker__area-thumb" style={{ left: `${s}%`, top: `${100 - v}%` }} />
        </div>
        <div
          ref={hueRef}
          className="ow-colorpicker__hue"
          role="slider"
          tabIndex={disabled ? -1 : 0}
          aria-label={t.hue}
          aria-orientation="horizontal"
          aria-valuemin={0}
          aria-valuemax={359}
          aria-valuenow={hue}
          aria-valuetext={t.hueValue({ deg: hue })}
          aria-disabled={disabled || undefined}
          onKeyDown={onHueKey}
          onBlur={onSliderBlur}
          onPointerDown={(event) => {
            onPointerDown(event, "hue");
          }}
          onPointerMove={(event) => {
            onPointerMove(event, "hue");
          }}
          onPointerUp={() => {
            onPointerUp("hue");
          }}
        >
          <span className="ow-colorpicker__hue-thumb" style={{ left: `${(hue / 359) * 100}%` }} />
        </div>
      </div>
      <div className="ow-colorpicker__row">
        <span className="ow-colorpicker__preview" style={{ backgroundColor: value }} role="img" aria-label={t.current({ hex: value })} />
        <label htmlFor={`${id}-hex`} className="ow-small ow-strong">
          {t.hex}
        </label>
        <input
          id={`${id}-hex`}
          type="text"
          className="ow-input ow-mono ow-colorpicker__hex"
          value={hexDraft ?? value}
          disabled={disabled}
          spellCheck={false}
          maxLength={7}
          aria-invalid={invalid || undefined}
          aria-describedby={invalid ? `${id}-error` : undefined}
          onFocus={() => {
            setHexDraft(value);
          }}
          onChange={(event) => {
            setHexDraft(event.currentTarget.value);
          }}
          onBlur={commitHex}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              commitHex();
            }
          }}
        />
      </div>
      {invalid ? (
        <p id={`${id}-error`} className="ow-small ow-danger-text">
          {dict.common.fields.invalidHex}
        </p>
      ) : null}
      {swatches.length === 0 ? null : (
        <div className="ow-colorpicker__swatches" role="group" aria-label={dict.common.fields.swatches}>
          {swatches.map((swatch) => swatchButton(swatch.hex, swatch.name, `s-${swatch.hex}`))}
        </div>
      )}
      {!recent || recentList.length === 0 ? null : (
        <div className="ow-colorpicker__swatches" role="group" aria-label={t.recent}>
          {recentList.map((hex) => swatchButton(hex, hex, `r-${hex}`))}
        </div>
      )}
    </div>
  );
}
