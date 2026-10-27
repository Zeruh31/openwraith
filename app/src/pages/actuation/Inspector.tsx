// The editor under the keyboard: what is selected and what it has now, the
// actuation point and the Rapid Trigger setting. Every control changes the
// local draft only; nothing is sent to the keyboard until "Review and
// write…" and the confirmation.

import { useId } from "react";

import { RadioGroup, Switch, type ChoiceOption } from "../../components/Controls";
import { Callout } from "../../components/Feedback";
import { MmField, Slider } from "../../components/Fields";
import { Card, Disclosure } from "../../components/Layout";
import { notEditableText } from "../../flow/labels";
import { keyName, useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { KeyTriggerView, TriggerLimitsView, TriggerValue } from "../../ipc/bindings";
import {
  chooseRt,
  initialActuation,
  KEY_NAMES,
  nowSummary,
  type RtChoice,
  type TriggerDraft,
  type ValueTexts,
} from "./model";

const NAMES_SHOWN = 8;

/** Props of {@link Inspector}. */
export interface InspectorProps {
  /** Selected key indices (ascending). */
  selected: readonly number[];
  /** Selected keys that can't be changed. */
  leftOut: readonly number[];
  keys: ReadonlyMap<number, KeyTriggerView>;
  /** Settings of the selected keys that can be changed. */
  values: readonly TriggerValue[];
  draft: TriggerDraft;
  onDraft: (next: TriggerDraft) => void;
  limits: TriggerLimitsView;
  unitUm: number;
  texts: ValueTexts;
  /** The controls can be used (the keyboard is read, editable and current). */
  canEdit: boolean;
}

/** A distance slider with its millimetre field. */
function DistanceSlider({
  label,
  value,
  min,
  max,
  onChange,
  unitUm,
  texts,
  hint,
  disabled,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (units: number) => void;
  unitUm: number;
  texts: ValueTexts;
  hint?: string | undefined;
  disabled: boolean;
}) {
  return (
    <Slider
      label={label}
      value={value}
      min={min}
      max={max}
      step={1}
      bigStep={5}
      onChange={onChange}
      valueText={texts.mm}
      hint={hint}
      disabled={disabled}
      field={
        <MmField
          label={label}
          hideLabel
          value={value}
          min={min}
          max={max}
          onChange={onChange}
          unitUm={unitUm}
          disabled={disabled}
        />
      }
    />
  );
}

/** The selection summary and the actuation / Rapid Trigger editor. */
export function Inspector({
  selected,
  leftOut,
  keys,
  values,
  draft,
  onDraft,
  limits,
  unitUm,
  texts,
  canEdit,
}: InspectorProps) {
  const dict = useDict();
  const format = useFormat();
  const actuationTitle = useId();
  const rtTitle = useId();
  const t = dict.actuation;
  const names = selected.map((index) => keyName(dict, KEY_NAMES.get(index) ?? String(index)));
  const shownNames =
    names.length <= NAMES_SHOWN
      ? format.list(names)
      : `${names.slice(0, NAMES_SHOWN).join(", ")} ${dict.common.andMore({ n: names.length - NAMES_SHOWN })}`;
  const now = nowSummary(values, dict, format, texts);
  const disabled = !canEdit;
  const rtOptions: ChoiceOption<RtChoice>[] = [
    { value: "keep", label: t.rt.keep, description: t.rt.keepHint },
    { value: "off", label: t.rt.off, description: t.rt.offHint },
    { value: "same", label: t.rt.same, description: t.rt.sameHint },
    { value: "separate", label: t.rt.separate, description: t.rt.separateHint },
  ];

  return (
    <Card title={t.inspector.title} className="act-inspector-card">
      <div className="act-inspector">
        <section className="act-inspector__summary" aria-label={t.inspector.title}>
          {selected.length === 0 ? (
            <p className="ow-muted">{t.inspector.none}</p>
          ) : (
            <>
              <p className="ow-strong">{t.inspector.count({ n: selected.length })}</p>
              <p className="act-inspector__names">{shownNames}</p>
              {now.length === 0 ? null : (
                <div>
                  <p className="ow-muted ow-small">{t.inspector.now}</p>
                  <ul className="act-inspector__now">
                    {now.map((line) => (
                      <li key={line} className="ow-num">
                        {line}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
              {leftOut.map((index) => {
                const key = keys.get(index);
                const name = keyName(dict, key?.key.name ?? KEY_NAMES.get(index) ?? String(index));
                const reason = key?.not_editable ?? { reason: "not_read" as const };
                return (
                  <Callout key={index} kind="warning">
                    <p>{t.inspector.leftOut({ key: name })}</p>
                    <p className="ow-muted ow-small">{notEditableText(reason, dict)}</p>
                  </Callout>
                );
              })}
            </>
          )}
          {canEdit ? null : <p className="ow-muted ow-small">{t.inspector.readOnly}</p>}
        </section>

        <section className="act-inspector__section" aria-labelledby={actuationTitle}>
          <h3 id={actuationTitle} className="act-inspector__title">
            {t.actuationControl.title}
          </h3>
          <Switch
            checked={draft.actuation !== null}
            disabled={disabled}
            label={t.actuationControl.change}
            onChange={(on) => {
              onDraft({ ...draft, actuation: on ? initialActuation(values, limits) : null });
            }}
          />
          {draft.actuation === null ? (
            <p className="ow-muted ow-small">{t.actuationControl.unchanged}</p>
          ) : (
            <DistanceSlider
              label={t.actuationControl.label}
              value={draft.actuation}
              min={limits.min_actuation}
              max={limits.max_actuation}
              onChange={(units) => {
                onDraft({ ...draft, actuation: units });
              }}
              unitUm={unitUm}
              texts={texts}
              hint={`${t.actuationControl.hint} ${t.actuationControl.range({
                min: texts.mm(limits.min_actuation),
                max: texts.mm(limits.max_actuation),
              })}`}
              disabled={disabled}
            />
          )}
        </section>

        <section className="act-inspector__section" aria-labelledby={rtTitle}>
          <h3 id={rtTitle} className="act-inspector__title">
            {t.rt.title}
          </h3>
          <RadioGroup<RtChoice>
            legend={t.rt.title}
            hideLegend
            options={rtOptions}
            value={draft.rt}
            disabled={disabled}
            onChange={(choice) => {
              onDraft(chooseRt(draft, choice, values, limits));
            }}
          />
          {draft.rt === "same" ? (
            <DistanceSlider
              label={t.rt.sensitivity}
              value={draft.sensitivity}
              min={1}
              max={limits.max_sensitivity}
              onChange={(units) => {
                onDraft({ ...draft, sensitivity: units });
              }}
              unitUm={unitUm}
              texts={texts}
              hint={t.rt.sensitivityHint}
              disabled={disabled}
            />
          ) : null}
          {draft.rt === "separate" ? (
            <>
              <DistanceSlider
                label={t.rt.press}
                value={draft.press}
                min={1}
                max={limits.max_sensitivity}
                onChange={(units) => {
                  onDraft({ ...draft, press: units });
                }}
                unitUm={unitUm}
                texts={texts}
                disabled={disabled}
              />
              <DistanceSlider
                label={t.rt.release}
                value={draft.release}
                min={1}
                max={limits.max_sensitivity}
                onChange={(units) => {
                  onDraft({ ...draft, release: units });
                }}
                unitUm={unitUm}
                texts={texts}
                hint={t.rt.sensitivityHint}
                disabled={disabled}
              />
            </>
          ) : null}
          <Disclosure summary={t.rt.explainTitle}>
            <p className="ow-small">{t.rt.explain}</p>
          </Disclosure>
        </section>
      </div>
    </Card>
  );
}
