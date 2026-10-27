// The effect picker: one tile per effect the keyboard offers (device order,
// plus Custom), as a radio group with arrow-key navigation. Picking a tile
// only changes the page's draft; nothing is written here.

import { useRef, type KeyboardEvent } from "react";

import { useDict } from "../../i18n";
import type { LightModeView } from "../../ipc/bindings";
import { modeName } from "./labels";
import { ModeIcon } from "./ModeIcon";

/** Props of {@link EffectTiles}. */
export interface EffectTilesProps {
  /** The effects that can be written. */
  modes: readonly LightModeView[];
  /** The effect on the keyboard now (marked). */
  current: LightModeView;
  /** The effect shown (draft or current). */
  selected: number;
  onSelect: (id: number) => void;
  disabled: boolean;
  /** Id of a visible text describing the group (hints). */
  describedBy?: string | undefined;
}

/** The effect tiles. An effect on the keyboard that can't be written is shown first, disabled. */
export function EffectTiles({ modes, current, selected, onSelect, disabled, describedBy }: EffectTilesProps) {
  const dict = useDict();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const currentOffered = modes.some((mode) => mode.id === current.id);
  const selectedIndex = modes.findIndex((mode) => mode.id === selected);
  const focusIndex = selectedIndex >= 0 ? selectedIndex : 0;

  const move = (from: number, step: number) => {
    if (modes.length === 0) return;
    const next = (from + step + modes.length) % modes.length;
    const mode = modes[next];
    if (mode === undefined) return;
    refs.current[next]?.focus();
    onSelect(mode.id);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    let handled = true;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") move(index, 1);
    else if (event.key === "ArrowLeft" || event.key === "ArrowUp") move(index, -1);
    else if (event.key === "Home") move(-1, 1);
    else if (event.key === "End") move(0, -1);
    else handled = false;
    if (handled) event.preventDefault();
  };

  const marker = (
    <span className="li-tile__marker">
      <span aria-hidden="true">●</span> {dict.lighting.effects.onKeyboard}
    </span>
  );

  return (
    <div
      className="li-tiles"
      role="radiogroup"
      aria-label={dict.lighting.effects.label}
      aria-describedby={describedBy}
      aria-disabled={disabled || undefined}
    >
      {currentOffered ? null : (
        <button type="button" role="radio" aria-checked={selected === current.id} disabled className="li-tile li-tile--unknown">
          <ModeIcon mode="unknown" />
          <span className="li-tile__name">{modeName(dict, current)}</span>
          {marker}
        </button>
      )}
      {modes.map((mode, index) => (
        <button
          key={mode.id}
          ref={(element) => {
            refs.current[index] = element;
          }}
          type="button"
          role="radio"
          aria-checked={mode.id === selected}
          tabIndex={index === focusIndex ? 0 : -1}
          disabled={disabled}
          className="li-tile"
          onClick={() => {
            onSelect(mode.id);
          }}
          onKeyDown={(event) => {
            onKeyDown(event, index);
          }}
        >
          <ModeIcon mode={mode.key} />
          <span className="li-tile__name">{modeName(dict, mode)}</span>
          {mode.id === current.id ? marker : null}
        </button>
      ))}
    </div>
  );
}
