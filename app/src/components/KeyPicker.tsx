// Picks one physical key (an SOCD pair's key, a key to swap with): a
// combobox that filters the keys by name as you type (Up/Down, Enter,
// Esc), plus "Pick on the board…", which opens the W75 drawing as a
// single-select listbox in a dialog. Picking only reports the key.

import { useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

import { fold } from "./actions/catalog";
import { Button } from "./Button";
import { Dialog } from "./Dialog";
import { KeyboardView, KNOB_INDEX, type KeyVisual } from "./keyboard";
import { useDict } from "../i18n";

/** A key the picker offers. */
export interface KeyOption {
  /** The key index (the library's). */
  index: number;
  /** The accessible name ("Left Shift"). */
  name: string;
  /** The short legend for the board ("Shift"); default: the name. */
  legend?: string | undefined;
  /** Why it can't be picked (shown, not hidden); `null`/absent when it can. */
  disabledReason?: string | null | undefined;
}

/** Props of {@link KeyPicker}. */
export interface KeyPickerProps {
  /** The visible label ("Key 1"). */
  label: string;
  value: number | null;
  onChange: (index: number) => void;
  options: readonly KeyOption[];
  disabled?: boolean | undefined;
  /** A line under the field. */
  description?: ReactNode | undefined;
  /** What the board shows on each key (default: the option's legend). */
  keyVisual?: ((index: number) => KeyVisual) | undefined;
}

/** The key picker. */
export function KeyPicker({ label, value, onChange, options, disabled = false, description, keyVisual }: KeyPickerProps) {
  const dict = useDict();
  const t = dict.common.pickers.keyPicker;
  const id = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [text, setText] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [board, setBoard] = useState(false);
  const current = options.find((option) => option.index === value) ?? null;
  const shownText = text ?? current?.name ?? "";

  const filtered = useMemo(() => {
    const q = fold(text ?? "");
    if (q === "") return options;
    return options.filter((option) => {
      const name = fold(option.name);
      const legend = fold(option.legend ?? "");
      return name.startsWith(q) || legend.startsWith(q) || name.split(" ").some((word) => word.startsWith(q));
    });
  }, [options, text]);

  const choose = (option: KeyOption | undefined) => {
    if (option === undefined || (option.disabledReason ?? null) !== null) return;
    onChange(option.index);
    setText(null);
    setOpen(false);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        if (!open) {
          setOpen(true);
          setActive(Math.max(0, filtered.findIndex((option) => option.index === value)));
        } else {
          setActive((index) => Math.min(filtered.length - 1, index + 1));
        }
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive((index) => Math.max(0, index - 1));
        break;
      case "Enter":
        if (open) {
          event.preventDefault();
          choose(filtered[active]);
        }
        break;
      case "Escape":
        if (open || text !== null) {
          event.preventDefault();
          event.stopPropagation();
          setOpen(false);
          setText(null);
        }
        break;
      default:
        break;
    }
  };

  const optionId = (index: number) => `${id}-o-${index}`;
  const offered = useMemo(() => new Map(options.map((option) => [option.index, option])), [options]);
  const visual = (index: number): KeyVisual => {
    const option = offered.get(index);
    const base = keyVisual?.(index);
    if (option === undefined) return { ...(base ?? { legend: "", name: String(index) }), selectable: false };
    const pickable = (option.disabledReason ?? null) === null;
    return {
      legend: option.legend ?? option.name,
      name: option.name,
      ...base,
      selectable: pickable,
      tooltip: option.disabledReason ?? base?.tooltip ?? null,
    };
  };

  return (
    <div className="ow-keypicker">
      <label htmlFor={`${id}-input`} className="ow-field__label">
        {label}
      </label>
      <div className="ow-keypicker__row">
        <div className="ow-keypicker__combo">
          <input
            ref={inputRef}
            id={`${id}-input`}
            type="text"
            className="ow-input"
            role="combobox"
            aria-expanded={open}
            aria-controls={`${id}-list`}
            aria-autocomplete="list"
            aria-activedescendant={open && filtered[active] !== undefined ? optionId(active) : undefined}
            aria-describedby={description === undefined ? undefined : `${id}-d`}
            autoComplete="off"
            spellCheck={false}
            disabled={disabled}
            value={shownText}
            onChange={(event) => {
              setText(event.currentTarget.value);
              setOpen(true);
              setActive(0);
            }}
            onFocus={(event) => {
              event.currentTarget.select();
            }}
            onBlur={() => {
              setOpen(false);
              setText(null);
            }}
            onKeyDown={onKeyDown}
          />
          <div
            id={`${id}-list`}
            role="listbox"
            aria-label={t.options({ label })}
            className="ow-keypicker__list"
            hidden={!open}
          >
            {filtered.length === 0 ? (
              <p className="ow-small ow-muted ow-keypicker__none">{t.noMatch}</p>
            ) : (
              filtered.map((option, index) => (
                <div
                  key={option.index}
                  id={optionId(index)}
                  role="option"
                  aria-selected={index === active}
                  aria-disabled={(option.disabledReason ?? null) !== null || undefined}
                  className={`ow-keypicker__option${index === active ? " ow-keypicker__option--active" : ""}`}
                  title={option.disabledReason ?? undefined}
                  onPointerDown={(event) => {
                    // Keep the focus in the field (blur would close the list first).
                    event.preventDefault();
                  }}
                  onClick={() => {
                    choose(option);
                  }}
                >
                  {option.name}
                  {option.disabledReason === undefined || option.disabledReason === null ? null : (
                    <span className="ow-small ow-muted"> · {option.disabledReason}</span>
                  )}
                </div>
              ))
            )}
          </div>
        </div>
        <Button
          size="sm"
          icon="pointer"
          disabled={disabled}
          onClick={() => {
            setBoard(true);
          }}
        >
          {t.pickOnBoard}
        </Button>
      </div>
      {description === undefined ? null : (
        <p id={`${id}-d`} className="ow-small ow-muted">
          {description}
        </p>
      )}
      <Dialog
        open={board}
        title={t.boardTitle}
        size="lg"
        onClose={() => {
          setBoard(false);
        }}
        actions={
          <Button
            onClick={() => {
              setBoard(false);
            }}
          >
            {dict.common.actions.cancel}
          </Button>
        }
      >
        {board ? (
          <div className="ow-stack">
            <p className="ow-small ow-muted">{t.boardHint}</p>
            <KeyboardView
              label={t.boardLabel}
              selection="single"
              keyVisual={visual}
              selected={value === null ? new Set<number>() : new Set([value])}
              knobLabel={dict.keys.knob}
              onSelectionChange={(next) => {
                const [index] = [...next];
                // Esc on the board clears its selection: here it closes the dialog.
                if (index === undefined) {
                  setBoard(false);
                  return;
                }
                if (index === KNOB_INDEX) return;
                const option = offered.get(index);
                if (option === undefined || (option.disabledReason ?? null) !== null) return;
                onChange(index);
                setBoard(false);
              }}
            />
          </div>
        ) : null}
      </Dialog>
    </div>
  );
}
