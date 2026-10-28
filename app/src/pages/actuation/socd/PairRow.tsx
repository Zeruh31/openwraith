// One SOCD pair being edited: key 1 and key 2 (KeyPicker: type a name or
// pick on the board), the mode (with "Not yet verified" from the library's
// evidence for the chosen mode), Remove, and what breaks the site's rules.

import { useId } from "react";

import { EvidenceBadge } from "../../../components/Badge";
import { Button } from "../../../components/Button";
import { KeyPicker, type KeyOption } from "../../../components/KeyPicker";
import { keyName, useDict, type Dict } from "../../../i18n";
import type { EvidenceView } from "../../../ipc/bindings";
import { KEY_NAMES } from "../model";
import { modeName } from "./labels";
import { SOCD_MODES, type PairProblem, type SocdPairDraft } from "./model";

/** Props of {@link PairRow}. */
export interface PairRowProps {
  /** 1-based position. */
  number: number;
  row: SocdPairDraft;
  onChange: (next: SocdPairDraft) => void;
  onRemove: () => void;
  /** Every key a pair can use. */
  options: readonly KeyOption[];
  /** Keys in other pairs, with their pair number. */
  used: ReadonlyMap<number, number>;
  problems: readonly PairProblem[];
  /** The evidence of each mode, from the state. */
  evidence: (mode: number) => EvidenceView | null;
  disabled: boolean;
}

/** What is wrong with a pair, in words. */
export function problemText(problem: PairProblem, dict: Dict): string {
  const t = dict.actuation.socd.problems;
  const name = (index: number) => keyName(dict, KEY_NAMES.get(index) ?? String(index));
  switch (problem.kind) {
    case "incomplete":
      return t.incomplete;
    case "same_key":
      return t.sameKey({ key: name(problem.key) });
    case "duplicate":
      return t.duplicate({ key: name(problem.key), pair: problem.pair });
    case "mode":
      return t.mode;
  }
}

/** A pair's editor. */
export function PairRow({ number, row, onChange, onRemove, options, used, problems, evidence, disabled }: PairRowProps) {
  const dict = useDict();
  const t = dict.actuation.socd.pairs;
  const modeId = useId();
  const problemId = useId();

  // Keys in other pairs (and this pair's other key) are shown with the reason, not hidden.
  const optionsFor = (other: number | null): KeyOption[] =>
    options.map((option) => {
      const pair = used.get(option.index);
      const reason = pair !== undefined ? t.usedIn({ n: pair }) : option.index === other ? t.otherKey : null;
      return { ...option, disabledReason: reason };
    });

  return (
    <fieldset className="act-socd-pair" aria-describedby={problems.length === 0 ? undefined : problemId}>
      <legend className="act-socd-pair__legend">{t.pairN({ n: number })}</legend>
      <div className="act-socd-pair__fields">
        <KeyPicker
          label={t.key1}
          value={row.key1}
          options={optionsFor(row.key2)}
          disabled={disabled}
          onChange={(index) => {
            onChange({ ...row, key1: index });
          }}
        />
        <KeyPicker
          label={t.key2}
          value={row.key2}
          options={optionsFor(row.key1)}
          disabled={disabled}
          onChange={(index) => {
            onChange({ ...row, key2: index });
          }}
        />
        <div className="act-socd-pair__mode">
          <label htmlFor={modeId} className="ow-field__label">
            {t.mode}
          </label>
          <div className="ow-row">
            <select
              id={modeId}
              className="ow-input"
              value={row.mode}
              disabled={disabled}
              onChange={(event) => {
                onChange({ ...row, mode: Number(event.currentTarget.value) });
              }}
            >
              {SOCD_MODES.map((mode) => (
                <option key={mode} value={mode}>
                  {t.modeOption({ n: mode, name: modeName(mode, dict) })}
                </option>
              ))}
            </select>
            <EvidenceBadge evidence={evidence(row.mode)} size="sm" />
          </div>
        </div>
        <div className="act-socd-pair__remove">
          <Button size="sm" variant="ghost" icon="trash" disabled={disabled} onClick={onRemove}>
            {t.remove}
          </Button>
        </div>
      </div>
      {problems.length === 0 ? null : (
        <ul id={problemId} className="act-socd-pair__problems">
          {problems.map((problem, i) => (
            <li key={`${problem.kind}-${i}`} className="ow-field__notice ow-field__notice--error">
              {problemText(problem, dict)}
            </li>
          ))}
        </ul>
      )}
    </fieldset>
  );
}
