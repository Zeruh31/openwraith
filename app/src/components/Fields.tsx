// Value controls: slider, millimetre field, colour field. None of them talks
// to the keyboard: they change a local draft, and pages write only after an
// explicit "Review and write…" (see `ApplyBar` and `flow/WriteFlow`).

import { useId, useState, type KeyboardEvent, type ReactNode } from "react";

import { normalizeHex } from "../format/color";
import { clampUnits, DEFAULT_UNIT_UM } from "../format/units";
import { useDict } from "../i18n";
import { useFormat } from "../i18n/format";

/** Props of {@link Slider}. */
export interface SliderProps {
  label: ReactNode;
  /** Hide the label visually (it stays the accessible name). */
  hideLabel?: boolean | undefined;
  /** Integer value. */
  value: number;
  min: number;
  max: number;
  step?: number | undefined;
  /** Step for Shift + arrow keys (default 5 × step). */
  bigStep?: number | undefined;
  onChange: (value: number) => void;
  /** The spoken value ("1.20 mm", "2 of 4"). */
  valueText: (value: number) => string;
  /** Show a tick per step (for short discrete ranges). */
  ticks?: boolean | undefined;
  /** A paired input shown next to the slider (e.g. an `MmField`). */
  field?: ReactNode | undefined;
  /** Text under the slider. */
  hint?: ReactNode | undefined;
  disabled?: boolean | undefined;
}

/**
 * A range slider in integer units. Dragging only calls `onChange` (a local
 * draft); nothing is written to the keyboard until the page's Apply.
 */
export function Slider({
  label,
  hideLabel = false,
  value,
  min,
  max,
  step = 1,
  bigStep,
  onChange,
  valueText,
  ticks = false,
  field,
  hint,
  disabled = false,
}: SliderProps) {
  const id = useId();
  const big = bigStep ?? step * 5;
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (!event.shiftKey) {
      return;
    }
    let delta = 0;
    if (event.key === "ArrowRight" || event.key === "ArrowUp") delta = big;
    if (event.key === "ArrowLeft" || event.key === "ArrowDown") delta = -big;
    if (delta !== 0) {
      event.preventDefault();
      onChange(Math.min(max, Math.max(min, value + delta)));
    }
  };
  const tickValues: number[] = [];
  if (ticks) {
    for (let v = min; v <= max; v += step) tickValues.push(v);
  }
  return (
    <div className="ow-slider">
      <label htmlFor={id} className={hideLabel ? "sr-only" : "ow-field__label"}>
        {label}
      </label>
      <div className="ow-slider__row">
        <input
          id={id}
          type="range"
          className="ow-slider__input"
          min={min}
          max={max}
          step={step}
          value={value}
          disabled={disabled}
          aria-valuetext={valueText(value)}
          aria-describedby={hint === undefined ? undefined : `${id}-hint`}
          list={ticks ? `${id}-ticks` : undefined}
          onKeyDown={onKeyDown}
          onChange={(event) => {
            onChange(Number(event.currentTarget.value));
          }}
        />
        {field === undefined ? null : <div className="ow-slider__field">{field}</div>}
      </div>
      {ticks ? (
        <datalist id={`${id}-ticks`}>
          {tickValues.map((v) => (
            <option key={v} value={v} />
          ))}
        </datalist>
      ) : null}
      {hint === undefined ? null : (
        <p id={`${id}-hint`} className="ow-muted ow-small">
          {hint}
        </p>
      )}
    </div>
  );
}

/** The example a field's "enter a distance" hint gives: the middle of its own range (1.20 mm when it fits). */
export function mmExample(min: number, max: number): number {
  if (min <= 60 && 60 <= max) return 60;
  return Math.min(max, Math.max(min, Math.round((min + max) / 2)));
}

/** Props of {@link MmField}. */
export interface MmFieldProps {
  /** The accessible name (also used in the stepper buttons' names). */
  label: string;
  hideLabel?: boolean | undefined;
  /** Value in trigger units (0.02 mm). */
  value: number;
  /** Limits in trigger units. */
  min: number;
  max: number;
  onChange: (units: number) => void;
  unitUm?: number | undefined;
  disabled?: boolean | undefined;
}

/**
 * A millimetre input over integer units: accepts "1.2", "1,2", "1,20";
 * on blur or Enter it rounds to 0.02 mm and clamps to the limits, with an
 * inline notice. The − / + buttons step by one unit (five with Shift).
 */
export function MmField({
  label,
  hideLabel = false,
  value,
  min,
  max,
  onChange,
  unitUm = DEFAULT_UNIT_UM,
  disabled = false,
}: MmFieldProps) {
  const id = useId();
  const dict = useDict();
  const format = useFormat();
  const [draft, setDraft] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const shown = draft ?? format.mmNumber(value, unitUm);

  const commit = () => {
    if (draft === null) {
      return;
    }
    setDraft(null);
    const parsed = format.parseMm(draft, unitUm);
    if (!parsed.ok) {
      setNotice(dict.common.fields.invalidMm({ example: format.mmNumber(mmExample(min, max), unitUm) }));
      return;
    }
    const { units, clamped } = clampUnits(parsed.units, min, max);
    const text = format.mm(units, unitUm);
    if (clamped === "min") setNotice(dict.common.fields.clampedMin({ value: text }));
    else if (clamped === "max") setNotice(dict.common.fields.clampedMax({ value: text }));
    else if (parsed.rounded) setNotice(dict.common.fields.roundedTo({ value: text }));
    else setNotice(null);
    if (units !== value) {
      onChange(units);
    }
  };

  const step = (direction: 1 | -1, shift: boolean) => {
    const next = clampUnits(value + direction * (shift ? 5 : 1), min, max).units;
    setDraft(null);
    setNotice(null);
    if (next !== value) {
      onChange(next);
    }
  };

  return (
    <div className="ow-mmfield">
      <label htmlFor={id} className={hideLabel ? "sr-only" : "ow-field__label"}>
        {label}
      </label>
      <div className="ow-mmfield__row">
        <button
          type="button"
          className="ow-mmfield__step"
          aria-label={dict.common.fields.decrease({ label })}
          title={dict.common.fields.stepHint}
          disabled={disabled || value <= min}
          onClick={(event) => {
            step(-1, event.shiftKey);
          }}
        >
          −
        </button>
        <input
          id={id}
          type="text"
          inputMode="decimal"
          className="ow-input ow-num ow-mmfield__input"
          value={shown}
          disabled={disabled}
          aria-describedby={notice === null ? undefined : `${id}-notice`}
          onFocus={() => {
            setDraft(shown);
          }}
          onChange={(event) => {
            setDraft(event.currentTarget.value);
          }}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              commit();
            } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
              event.preventDefault();
              step(event.key === "ArrowUp" ? 1 : -1, event.shiftKey);
            }
          }}
        />
        <span className="ow-mmfield__unit" aria-hidden="true">
          {dict.common.units.mmUnit}
        </span>
        <button
          type="button"
          className="ow-mmfield__step"
          aria-label={dict.common.fields.increase({ label })}
          title={dict.common.fields.stepHint}
          disabled={disabled || value >= max}
          onClick={(event) => {
            step(1, event.shiftKey);
          }}
        >
          +
        </button>
      </div>
      <p id={`${id}-notice`} className="ow-field__notice" aria-live="polite">
        {notice}
      </p>
    </div>
  );
}

/** A quick colour of a {@link ColorField}. */
export interface ColorSwatchOption {
  /** "#rrggbb". */
  hex: string;
  /** The colour's name ("Red"), its accessible name. */
  name: string;
}

/** Props of {@link ColorField}. */
export interface ColorFieldProps {
  label: string;
  /** "#rrggbb". */
  value: string;
  onChange: (hex: string) => void;
  /** Quick colours shown as buttons, each with its (localized) name. */
  swatches?: readonly ColorSwatchOption[] | undefined;
  disabled?: boolean | undefined;
}

/** A colour picker plus a validated hex text field and optional quick colours. */
export function ColorField({ label, value, onChange, swatches, disabled = false }: ColorFieldProps) {
  const id = useId();
  const dict = useDict();
  const [draft, setDraft] = useState<string | null>(null);
  const [invalid, setInvalid] = useState(false);
  const commit = () => {
    if (draft === null) {
      return;
    }
    const hex = normalizeHex(draft);
    setDraft(null);
    if (hex === null) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    if (hex !== value) {
      onChange(hex);
    }
  };
  return (
    <div className="ow-colorfield">
      <span id={`${id}-label`} className="ow-field__label">
        {label}
      </span>
      <div className="ow-colorfield__row">
        <input
          type="color"
          className="ow-colorfield__picker"
          value={value}
          disabled={disabled}
          aria-label={dict.common.fields.colorPicker({ label })}
          onChange={(event) => {
            const hex = normalizeHex(event.currentTarget.value);
            if (hex !== null) {
              setInvalid(false);
              onChange(hex);
            }
          }}
        />
        <input
          type="text"
          className="ow-input ow-mono ow-colorfield__hex"
          value={draft ?? value}
          disabled={disabled}
          spellCheck={false}
          maxLength={7}
          aria-label={dict.common.fields.hexValue({ label })}
          aria-invalid={invalid || undefined}
          aria-describedby={invalid ? `${id}-error` : undefined}
          onFocus={() => {
            setDraft(value);
          }}
          onChange={(event) => {
            setDraft(event.currentTarget.value);
          }}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              commit();
            }
          }}
        />
      </div>
      {swatches === undefined || swatches.length === 0 ? null : (
        <div className="ow-colorfield__swatches" role="group" aria-label={dict.common.fields.swatches}>
          {swatches.map((swatch) => (
            <button
              key={swatch.hex}
              type="button"
              className="ow-colorfield__swatch"
              style={{ backgroundColor: swatch.hex }}
              aria-label={swatch.name}
              aria-pressed={swatch.hex === value}
              title={`${swatch.name} (${swatch.hex})`}
              disabled={disabled}
              onClick={() => {
                setInvalid(false);
                onChange(swatch.hex);
              }}
            />
          ))}
        </div>
      )}
      <p id={`${id}-error`} className="ow-field__notice ow-field__notice--error" aria-live="polite">
        {invalid ? dict.common.fields.invalidHex : null}
      </p>
    </div>
  );
}
