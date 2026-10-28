// Choice controls: checkbox, switch, radio group, segmented control, tabs.

import { useId, useRef, type KeyboardEvent, type ReactNode } from "react";

/** Props of {@link Checkbox}. */
export interface CheckboxProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: ReactNode;
  description?: ReactNode | undefined;
  disabled?: boolean | undefined;
}

/** A labelled checkbox with an optional description. */
export function Checkbox({ checked, onChange, label, description, disabled = false }: CheckboxProps) {
  const id = useId();
  return (
    <div className={`ow-check${disabled ? " ow-check--disabled" : ""}`}>
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        aria-describedby={description === undefined ? undefined : `${id}-d`}
        onChange={(event) => {
          onChange(event.currentTarget.checked);
        }}
      />
      <div>
        <label htmlFor={id}>{label}</label>
        {description === undefined ? null : (
          <p id={`${id}-d`} className="ow-muted ow-small">
            {description}
          </p>
        )}
      </div>
    </div>
  );
}

/** Props of {@link Switch}. */
export interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: ReactNode;
  disabled?: boolean | undefined;
  /**
   * Help under the label (the switch's accessible description), e.g. why it
   * is disabled.
   */
  description?: ReactNode | undefined;
}

/** An on/off switch (`role=switch`). */
export function Switch({ checked, onChange, label, disabled = false, description }: SwitchProps) {
  const id = useId();
  const button = (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      aria-describedby={description === undefined ? undefined : `${id}-d`}
      className="ow-switch"
      onClick={() => {
        onChange(!checked);
      }}
    >
      <span className="ow-switch__thumb" />
    </button>
  );
  if (description === undefined) {
    return (
      <div className="ow-switch-row">
        {button}
        <label htmlFor={id} className="ow-switch__label">
          {label}
        </label>
      </div>
    );
  }
  return (
    <div className={`ow-switch-row ow-switch-row--described${disabled ? " ow-switch-row--disabled" : ""}`}>
      {button}
      <div>
        <label htmlFor={id} className="ow-switch__label">
          {label}
        </label>
        <p id={`${id}-d`} className="ow-muted ow-small">
          {description}
        </p>
      </div>
    </div>
  );
}

/** One option of a radio group or segmented control. */
export interface ChoiceOption<T extends string | number> {
  value: T;
  label: ReactNode;
  description?: ReactNode | undefined;
  disabled?: boolean | undefined;
}

/** Props of {@link RadioGroup}. */
export interface RadioGroupProps<T extends string | number> {
  legend: ReactNode;
  /** Hide the legend visually (it stays the group's accessible name). */
  hideLegend?: boolean | undefined;
  options: readonly ChoiceOption<T>[];
  value: T | null;
  onChange: (value: T) => void;
  disabled?: boolean | undefined;
}

/** A vertical group of native radio buttons with optional descriptions. */
export function RadioGroup<T extends string | number>({
  legend,
  hideLegend = false,
  options,
  value,
  onChange,
  disabled = false,
}: RadioGroupProps<T>) {
  const name = useId();
  return (
    <fieldset className="ow-radio-group" disabled={disabled}>
      <legend className={hideLegend ? "sr-only" : "ow-radio-group__legend"}>{legend}</legend>
      {options.map((option) => {
        const id = `${name}-${String(option.value)}`;
        return (
          <div key={String(option.value)} className="ow-radio">
            <input
              id={id}
              type="radio"
              name={name}
              checked={value === option.value}
              disabled={option.disabled === true}
              aria-describedby={option.description === undefined ? undefined : `${id}-d`}
              onChange={() => {
                onChange(option.value);
              }}
            />
            <div>
              <label htmlFor={id}>{option.label}</label>
              {option.description === undefined ? null : (
                <p id={`${id}-d`} className="ow-muted ow-small">
                  {option.description}
                </p>
              )}
            </div>
          </div>
        );
      })}
    </fieldset>
  );
}

/** Props of {@link SegmentedControl}. */
export interface SegmentedControlProps<T extends string | number> {
  /** The accessible name of the group. */
  label: string;
  options: readonly ChoiceOption<T>[];
  value: T | null;
  onChange: (value: T) => void;
  disabled?: boolean | undefined;
  size?: "sm" | "md" | undefined;
  /**
   * Arrow keys select the next option (radio behaviour, default). When
   * false, arrows only move focus and Space/Enter select (for choices that
   * open a confirmation, such as the profile switcher).
   */
  selectOnFocus?: boolean | undefined;
  /** Describes why the control is disabled (id of a visible text). */
  describedBy?: string | undefined;
}

/** A row of mutually exclusive buttons (`role=radiogroup`) with arrow-key navigation. */
export function SegmentedControl<T extends string | number>({
  label,
  options,
  value,
  onChange,
  disabled = false,
  size = "md",
  selectOnFocus = true,
  describedBy,
}: SegmentedControlProps<T>) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const enabled = options.map((o) => !disabled && o.disabled !== true);
  const checkedIndex = options.findIndex((o) => o.value === value);
  const firstEnabled = enabled.indexOf(true);
  const tabIndexOf = (index: number) =>
    (checkedIndex >= 0 && enabled[checkedIndex] === true ? checkedIndex : firstEnabled) === index ? 0 : -1;

  const move = (from: number, step: number) => {
    for (let i = 1; i <= options.length; i += 1) {
      const next = (from + step * i + options.length * i) % options.length;
      if (enabled[next] === true) {
        refs.current[next]?.focus();
        const option = options[next];
        if (selectOnFocus && option !== undefined) {
          onChange(option.value);
        }
        return;
      }
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      event.preventDefault();
      move(index, 1);
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      event.preventDefault();
      move(index, -1);
    }
  };

  return (
    <div
      className={`ow-segmented ow-segmented--${size}`}
      role="radiogroup"
      aria-label={label}
      aria-describedby={describedBy}
      aria-disabled={disabled || undefined}
    >
      {options.map((option, index) => (
        <button
          key={String(option.value)}
          ref={(element) => {
            refs.current[index] = element;
          }}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          tabIndex={tabIndexOf(index)}
          disabled={enabled[index] !== true}
          className="ow-segmented__option"
          onClick={() => {
            onChange(option.value);
          }}
          onKeyDown={(event) => {
            onKeyDown(event, index);
          }}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/** One tab of {@link Tabs}. */
export interface TabItem<T extends string> {
  id: T;
  label: ReactNode;
  content: ReactNode;
}

/** Props of {@link Tabs}. */
export interface TabsProps<T extends string> {
  label: string;
  tabs: readonly TabItem<T>[];
  value: T;
  onChange: (id: T) => void;
}

/** Tabs (`role=tablist`) with arrow-key navigation; only the selected panel is rendered. */
export function Tabs<T extends string>({ label, tabs, value, onChange }: TabsProps<T>) {
  const base = useId();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const selected = tabs.find((tab) => tab.id === value) ?? tabs[0];
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    let next = -1;
    if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
    if (event.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = tabs.length - 1;
    const tab = tabs[next];
    if (tab !== undefined) {
      event.preventDefault();
      refs.current[next]?.focus();
      onChange(tab.id);
    }
  };
  return (
    <div className="ow-tabs">
      <div className="ow-tabs__list" role="tablist" aria-label={label}>
        {tabs.map((tab, index) => (
          <button
            key={tab.id}
            ref={(element) => {
              refs.current[index] = element;
            }}
            id={`${base}-tab-${tab.id}`}
            type="button"
            role="tab"
            aria-selected={tab.id === selected?.id}
            aria-controls={`${base}-panel-${tab.id}`}
            tabIndex={tab.id === selected?.id ? 0 : -1}
            className="ow-tabs__tab"
            onClick={() => {
              onChange(tab.id);
            }}
            onKeyDown={(event) => {
              onKeyDown(event, index);
            }}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {selected === undefined ? null : (
        <div
          id={`${base}-panel-${selected.id}`}
          role="tabpanel"
          aria-labelledby={`${base}-tab-${selected.id}`}
          className="ow-tabs__panel"
        >
          {selected.content}
        </div>
      )}
    </div>
  );
}
