// The tools above the keyboard: selection shortcuts (all, none, invert, same
// setting, changed keys, WASD, arrows, a row) and what the keys show.

import { useId } from "react";

import { Button } from "../../components/Button";
import { SegmentedControl } from "../../components/Controls";
import { ARROWS, invert, onlySelectable, selectAll, WASD, type Selection } from "../../components/keyboard";
import { useDict } from "../../i18n";
import type { KeyTriggerView } from "../../ipc/bindings";
import { ROW_IDS, rowKeys, sameSettingKeys, type RowId, type ShowMode } from "./model";

/** Props of {@link Toolbar}. */
export interface ToolbarProps {
  selection: Selection;
  onSelect: (next: Set<number>) => void;
  keys: ReadonlyMap<number, KeyTriggerView>;
  selectable: ReadonlySet<number>;
  /** Keys that differ from the basic setting. */
  changed: ReadonlySet<number>;
  show: ShowMode;
  onShow: (show: ShowMode) => void;
}

function isRowId(value: string): value is RowId {
  return (ROW_IDS as readonly string[]).includes(value);
}

/** Selection tools and the "Show on keys" switch. */
export function Toolbar({ selection, onSelect, keys, selectable, changed, show, onShow }: ToolbarProps) {
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
            onSelect(sameSettingKeys(selection, keys, selectable));
          }}
        >
          {t.same}
        </Button>
        <Button
          size="sm"
          disabled={changed.size === 0}
          title={t.changedHint}
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
      <div className="act-toolbar__show">
        <span className="act-toolbar__label" aria-hidden="true">
          {dict.actuation.show.label}
        </span>
        <SegmentedControl<ShowMode>
          label={dict.actuation.show.label}
          size="sm"
          value={show}
          onChange={onShow}
          options={[
            { value: "actuation", label: dict.actuation.show.actuation },
            { value: "rapid_trigger", label: dict.actuation.show.rapidTrigger },
          ]}
        />
      </div>
    </div>
  );
}
