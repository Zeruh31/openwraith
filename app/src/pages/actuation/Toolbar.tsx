// The tools above the keyboard of the trigger and dead band tabs: selection
// shortcuts (all, none, invert, same setting, changed keys, WASD, arrows, a
// row), what the keys show, and the tab's own tools (the trigger tab's
// presets).

import { useId, type ReactNode } from "react";

import { Button } from "../../components/Button";
import { SegmentedControl } from "../../components/Controls";
import { ARROWS, invert, onlySelectable, selectAll, WASD, type Selection } from "../../components/keyboard";
import { useDict } from "../../i18n";
import { ROW_IDS, rowKeys, type RowId } from "./model";

/** The "Show on keys" switch of a tab. */
export interface ShowSwitch<S extends string> {
  value: S;
  onChange: (value: S) => void;
  options: readonly { value: S; label: string }[];
}

/** Props of {@link Toolbar}. */
export interface ToolbarProps<S extends string> {
  selection: Selection;
  onSelect: (next: Set<number>) => void;
  selectable: ReadonlySet<number>;
  /** Keys that differ from the basic setting. */
  changed: ReadonlySet<number>;
  /** "Same setting": the keys that share a setting with a selected key. */
  same: (selection: Selection) => Set<number>;
  /** The tooltip of "Changed". */
  changedHint: string;
  show: ShowSwitch<S>;
  /** The tab's own tools, after the selection tools. */
  extra?: ReactNode | undefined;
}

function isRowId(value: string): value is RowId {
  return (ROW_IDS as readonly string[]).includes(value);
}

/** Selection tools and the "Show on keys" switch. */
export function Toolbar<S extends string>({
  selection,
  onSelect,
  selectable,
  changed,
  same,
  changedHint,
  show,
  extra,
}: ToolbarProps<S>) {
  const dict = useDict();
  const t = dict.actuation.select;
  const rowId = useId();
  const empty = selection.size === 0;
  return (
    <div className="act-toolbar">
      <div className="act-toolbar__select" role="group" aria-label={t.label}>
        <span className="act-toolbar__label" aria-hidden="true">
          {t.label}
        </span>
        <Button
          size="sm"
          onClick={() => {
            onSelect(selectAll(selectable));
          }}
        >
          {t.all}
        </Button>
        <Button
          size="sm"
          disabled={empty}
          onClick={() => {
            onSelect(new Set());
          }}
        >
          {t.none}
        </Button>
        <Button
          size="sm"
          onClick={() => {
            onSelect(invert(selection, selectable));
          }}
        >
          {t.invert}
        </Button>
        <Button
          size="sm"
          disabled={empty}
          title={t.sameHint}
          onClick={() => {
            onSelect(same(selection));
          }}
        >
          {t.same}
        </Button>
        <Button
          size="sm"
          disabled={changed.size === 0}
          title={changedHint}
          onClick={() => {
            onSelect(onlySelectable(changed, selectable));
          }}
        >
          {t.changed}
        </Button>
        <Button
          size="sm"
          onClick={() => {
            onSelect(onlySelectable(WASD, selectable));
          }}
        >
          {t.wasd}
        </Button>
        <Button
          size="sm"
          onClick={() => {
            onSelect(onlySelectable(ARROWS, selectable));
          }}
        >
          {t.arrows}
        </Button>
        <label htmlFor={rowId} className="sr-only">
          {t.rowLabel}
        </label>
        <select
          id={rowId}
          className="ow-input act-toolbar__row"
          value=""
          onChange={(event) => {
            const value = event.currentTarget.value;
            if (isRowId(value)) onSelect(onlySelectable(rowKeys(value), selectable));
          }}
        >
          <option value="">{t.rowPlaceholder}</option>
          {ROW_IDS.map((row) => (
            <option key={row} value={row}>
              {t.rows[row]}
            </option>
          ))}
        </select>
      </div>
      {extra === undefined ? null : <div className="act-toolbar__extra">{extra}</div>}
      <div className="act-toolbar__show">
        <span className="act-toolbar__label" aria-hidden="true">
          {dict.actuation.show.label}
        </span>
        <SegmentedControl<S>
          label={dict.actuation.show.label}
          size="sm"
          value={show.value}
          onChange={show.onChange}
          options={show.options}
        />
      </div>
    </div>
  );
}
