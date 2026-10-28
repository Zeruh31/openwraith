// The DKS editor's grid: one row per key, one cell per stage (1 Press,
// 2 Bottom, 3 Lift, 4 Release), as an ARIA grid with a roving tabindex: the
// grid is one Tab stop, the arrow keys move between cells (Home/End within a
// row, Ctrl + Home/End to the first/last cell), and Space or Enter on a
// stage cell changes it to the next step the keyboard accepts there. The
// "step strip" under the grid offers every step of the current cell as a
// radio group (steps the keyboard doesn't accept after the earlier ones are
// disabled). Each row shows the catalogue's evidence for its steps.
//
// Changing a cell only changes the editor's draft definition.

import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";

import { usageName } from "../../../components/actions";
import { EvidenceBadge } from "../../../components/Badge";
import { Button, IconButton } from "../../../components/Button";
import { SegmentedControl } from "../../../components/Controls";
import { useDict, type Dict } from "../../../i18n";
import type { DksRowView, DksStepView } from "../../../ipc/bindings";
import { allowedSteps, cycleStep, DKS_ROWS, DKS_STAGES, DKS_STEPS, rowRule, usedRows } from "./model";

/** Props of {@link DksGrid}. */
export interface DksGridProps {
  /** The four row positions. */
  rows: readonly (DksRowView | null)[];
  /** One stage of the row at `position` changed. */
  onStep: (position: number, stage: number, step: DksStepView) => void;
  /** The key of the row at `position` should be chosen (`null`: a new row); `opener` is the button's id. */
  onPickKey: (position: number | null, opener: string) => void;
  onRemove: (position: number) => void;
  disabled?: boolean | undefined;
  /** Id prefix of the buttons (the editor gives the focus back to them). */
  idBase: string;
}

/** The last column (the remove buttons). */
const LAST_COLUMN = 5;

type StageKey = "s1" | "s2" | "s3" | "s4";

function stageKey(stage: number): StageKey {
  return stage === 0 ? "s1" : stage === 1 ? "s2" : stage === 2 ? "s3" : "s4";
}

/** The visible name of a stage ("2 Bottom"). */
export function stageTitle(stage: number, dict: Dict): string {
  return dict.keysAdvanced.dks.stages[stageKey(stage)];
}

/** The id of a grid element (the editor focuses key buttons by it). */
export function cellId(idBase: string, position: number | "add", column: number): string {
  return `${idBase}-c-${String(position)}-${column}`;
}

/** The DKS grid with its step strip. */
export function DksGrid({ rows, onStep, onPickKey, onRemove, disabled = false, idBase }: DksGridProps) {
  const dict = useDict();
  const t = dict.keysAdvanced.dks;
  const labelId = useId();
  const hintId = useId();
  const stripId = useId();
  const used = usedRows(rows);
  // The focused cell (index into `used`, column) and the stage the strip edits.
  const [focus, setFocus] = useState<{ r: number; c: number }>({ r: 0, c: 1 });
  const [current, setCurrent] = useState<{ position: number; stage: number }>({ position: used[0]?.position ?? 0, stage: 0 });
  const refs = useRef(new Map<string, HTMLElement>());
  const pendingFocus = useRef<string | null>(null);

  const r = Math.min(focus.r, Math.max(0, used.length - 1));
  const c = Math.min(Math.max(0, focus.c), LAST_COLUMN);
  const currentRow = used.find((item) => item.position === current.position) ?? used[0];
  const currentStage = currentRow?.position === current.position ? current.stage : 0;

  useEffect(() => {
    const id = pendingFocus.current;
    if (id === null) return;
    pendingFocus.current = null;
    document.getElementById(id)?.focus();
  });

  const register = (row: number, column: number) => (element: HTMLElement | null) => {
    const key = `${row}:${column}`;
    if (element === null) refs.current.delete(key);
    else refs.current.set(key, element);
  };

  const moveTo = (row: number, column: number) => {
    const nextRow = Math.max(0, Math.min(used.length - 1, row));
    const nextColumn = Math.max(0, Math.min(LAST_COLUMN, column));
    setFocus({ r: nextRow, c: nextColumn });
    refs.current.get(`${nextRow}:${nextColumn}`)?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    let handled = true;
    switch (event.key) {
      case "ArrowRight":
        moveTo(r, c + 1);
        break;
      case "ArrowLeft":
        moveTo(r, c - 1);
        break;
      case "ArrowDown":
        moveTo(r + 1, c);
        break;
      case "ArrowUp":
        moveTo(r - 1, c);
        break;
      case "Home":
        if (event.ctrlKey) moveTo(0, 0);
        else moveTo(r, 0);
        break;
      case "End":
        if (event.ctrlKey) moveTo(used.length - 1, LAST_COLUMN);
        else moveTo(r, LAST_COLUMN);
        break;
      default:
        handled = false;
    }
    if (handled) event.preventDefault();
  };

  const tabIndex = (row: number, column: number) => (row === r && column === c ? 0 : -1);
  const focused = (row: number, column: number) => () => {
    setFocus({ r: row, c: column });
  };

  const remove = (row: number, position: number) => {
    // The focus goes to the row that takes this one's place, else the one above, else "Add a key".
    const target = used[row + 1] ?? used[row - 1];
    pendingFocus.current =
      target === undefined ? cellId(idBase, "add", 0) : cellId(idBase, target.position, LAST_COLUMN);
    setFocus({ r: row + 1 < used.length ? row : Math.max(0, row - 1), c: LAST_COLUMN });
    onRemove(position);
  };

  const stripKey = currentRow === undefined ? "" : usageName(currentRow.row.key, dict);
  const stripAllowed = currentRow === undefined ? [] : allowedSteps(currentRow.row.steps, currentStage);
  const full = used.length >= DKS_ROWS;

  return (
    <div className="keys-adv-dks">
      <p id={labelId} className="ow-field__label">
        {t.grid}
      </p>
      <p id={hintId} className="ow-small ow-muted">
        {t.gridHint}
      </p>
      <div
        role="grid"
        className="keys-adv-grid"
        aria-labelledby={labelId}
        aria-describedby={hintId}
        aria-rowcount={used.length + 1}
        aria-colcount={LAST_COLUMN + 1}
        aria-disabled={disabled || undefined}
        onKeyDown={onKeyDown}
      >
        <div role="row" aria-rowindex={1} className="keys-adv-grid__row keys-adv-grid__row--head">
          <div role="columnheader" className="keys-adv-grid__head">
            {t.keyColumn}
          </div>
          {DKS_STAGES.map((stage) => (
            <div key={stage} role="columnheader" className="keys-adv-grid__head keys-adv-grid__head--stage">
              {stageTitle(stage, dict)}
            </div>
          ))}
          <div role="columnheader" className="keys-adv-grid__head">
            <span className="sr-only">{t.actionsColumn}</span>
          </div>
        </div>
        {used.map(({ position, row }, index) => {
          const key = usageName(row.key, dict);
          const rule = rowRule(row.steps);
          return (
            <div key={position} role="row" aria-rowindex={index + 2} className="keys-adv-grid__row">
              <div role="rowheader" className="keys-adv-grid__key">
                <button
                  ref={register(index, 0)}
                  id={cellId(idBase, position, 0)}
                  type="button"
                  className="keys-adv-grid__keybutton"
                  tabIndex={tabIndex(index, 0)}
                  aria-label={t.rowKey({ key })}
                  disabled={disabled}
                  onFocus={focused(index, 0)}
                  onClick={() => {
                    onPickKey(position, cellId(idBase, position, 0));
                  }}
                >
                  {key}
                </button>
                <EvidenceBadge evidence={rule?.evidence ?? null} size="sm" />
              </div>
              {DKS_STAGES.map((stage) => {
                const step = row.steps[stage] ?? "none";
                const column = stage + 1;
                const isCurrent = currentRow?.position === position && currentStage === stage;
                return (
                  <div key={stage} role="gridcell" className="keys-adv-grid__cell">
                    <button
                      ref={register(index, column)}
                      id={cellId(idBase, position, column)}
                      type="button"
                      className={`keys-adv-step keys-adv-step--${step}${isCurrent ? " keys-adv-step--current" : ""}`}
                      tabIndex={tabIndex(index, column)}
                      aria-label={t.cellLabel({ key, stage: stageTitle(stage, dict), step: t.steps[step] })}
                      aria-describedby={stripId}
                      disabled={disabled}
                      data-step={step}
                      onFocus={() => {
                        setFocus({ r: index, c: column });
                        setCurrent({ position, stage });
                      }}
                      onClick={() => {
                        setCurrent({ position, stage });
                        onStep(position, stage, cycleStep(row.steps, stage));
                      }}
                    >
                      <span aria-hidden="true">{t.stepsShort[step]}</span>
                    </button>
                  </div>
                );
              })}
              <div role="gridcell" className="keys-adv-grid__cell keys-adv-grid__cell--remove">
                <IconButton
                  ref={register(index, LAST_COLUMN)}
                  id={cellId(idBase, position, LAST_COLUMN)}
                  icon="trash"
                  size="sm"
                  label={t.removeRow({ key })}
                  tabIndex={tabIndex(index, LAST_COLUMN)}
                  disabled={disabled}
                  onFocus={focused(index, LAST_COLUMN)}
                  onClick={() => {
                    remove(index, position);
                  }}
                />
              </div>
            </div>
          );
        })}
      </div>
      <div className="keys-adv-dks__add">
        <Button
          id={cellId(idBase, "add", 0)}
          size="sm"
          icon="plus"
          disabled={disabled}
          disabledReason={full ? t.addRowFull : null}
          onClick={() => {
            onPickKey(null, cellId(idBase, "add", 0));
          }}
        >
          {t.addRow}
        </Button>
      </div>
      {currentRow === undefined ? null : (
        <div className="keys-adv-strip">
          <p id={stripId} className="ow-small ow-strong">
            {t.stripLabel({ key: stripKey, stage: t.stageNames[stageKey(currentStage)] })}
          </p>
          <SegmentedControl<DksStepView>
            label={t.stripLabel({ key: stripKey, stage: t.stageNames[stageKey(currentStage)] })}
            size="sm"
            disabled={disabled}
            options={DKS_STEPS.map((step) => ({
              value: step,
              label: t.steps[step],
              disabled: !stripAllowed.includes(step),
            }))}
            value={currentRow.row.steps[currentStage] ?? "none"}
            onChange={(step) => {
              onStep(currentRow.position, currentStage, step);
            }}
          />
        </div>
      )}
    </div>
  );
}
