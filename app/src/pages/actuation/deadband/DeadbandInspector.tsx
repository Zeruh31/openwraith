// The dead band editor under the keyboard: what is selected and what it has
// now, the top and bottom dead zones, the travel gauge of the first
// selected key, and the soft warnings against the keys' actuation points.
// Every control changes the local draft only.

import { useId } from "react";

import { Switch } from "../../../components/Controls";
import { Callout } from "../../../components/Feedback";
import { Card, Disclosure } from "../../../components/Layout";
import { TravelGauge, W75_TRAVEL_MM } from "../../../components/TravelGauge";
import { keyName, useDict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type { DeadBandValue, KeyDeadBandView, KeyTriggerView } from "../../../ipc/bindings";
import { DistanceSlider } from "../Inspector";
import { KEY_NAMES, valueTexts } from "../model";
import { initialValue, targetOf, type DeadBandDraft, type DeadBandHint } from "./model";

const NAMES_SHOWN = 8;

/** Props of {@link DeadbandInspector}. */
export interface DeadbandInspectorProps {
  /** Selected key indices the state lists (ascending). */
  selected: readonly number[];
  keys: ReadonlyMap<number, KeyDeadBandView>;
  /** The values of the selected keys. */
  values: readonly DeadBandValue[];
  draft: DeadBandDraft;
  onDraft: (next: DeadBandDraft) => void;
  /** The largest value, in trigger units. */
  max: number;
  unitUm: number;
  /** The trigger settings of the last read (the keys' actuation points). */
  triggers: ReadonlyMap<number, KeyTriggerView>;
  /** The draft's soft warnings. */
  hints: readonly DeadBandHint[];
  canEdit: boolean;
}

/** The selection summary and the dead band editor. */
export function DeadbandInspector({
  selected,
  keys,
  values,
  draft,
  onDraft,
  max,
  unitUm,
  triggers,
  hints,
  canEdit,
}: DeadbandInspectorProps) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.actuation.deadband;
  const topTitle = useId();
  const bottomTitle = useId();
  const gaugeTitle = useId();
  const texts = valueTexts(dict, format, unitUm);
  const nameOf = (index: number) => keyName(dict, keys.get(index)?.key.name ?? KEY_NAMES.get(index) ?? String(index));
  const names = selected.map(nameOf);
  const shownNames =
    names.length <= NAMES_SHOWN
      ? format.list(names)
      : `${names.slice(0, NAMES_SHOWN).join(", ")} ${dict.common.andMore({ n: names.length - NAMES_SHOWN })}`;
  const disabled = !canEdit;
  const range = t.range({ min: texts.mm(0), max: texts.mm(max) });

  const nowLine = (field: "top" | "bottom") => {
    const all = values.map((value) => value[field]);
    const low = Math.min(...all);
    const high = Math.max(...all);
    const label = field === "top" ? t.top.title : t.bottom.title;
    return low === high
      ? t.nowSame({ label, value: texts.mm(low) })
      : t.nowMixed({ label, min: texts.number(low), max: texts.mm(high) });
  };

  // The gauge shows the first selected key: its dead zones after the draft and its actuation point.
  const first = selected[0];
  const firstKey = first === undefined ? undefined : keys.get(first);
  const firstTarget = firstKey === undefined ? null : targetOf(firstKey.value, draft);
  const firstActuation = first === undefined ? undefined : triggers.get(first)?.value?.actuation;
  const toMm = (units: number) => (units * unitUm) / 1000;
  const formatMm = (mm: number) => texts.mm(Math.round((mm * 1000) / unitUm));

  return (
    <Card title={t.inspector.title} className="act-inspector-card">
      <div className="act-inspector">
        <section className="act-inspector__summary" aria-label={t.inspector.title}>
          {selected.length === 0 ? (
            <p className="ow-muted">{t.inspector.none}</p>
          ) : (
            <>
              <p className="ow-strong">{dict.actuation.inspector.count({ n: selected.length })}</p>
              <p className="act-inspector__names">{shownNames}</p>
              {values.length === 0 ? null : (
                <div>
                  <p className="ow-muted ow-small">{dict.actuation.inspector.now}</p>
                  <ul className="act-inspector__now">
                    <li className="ow-num">{nowLine("top")}</li>
                    <li className="ow-num">{nowLine("bottom")}</li>
                  </ul>
                </div>
              )}
            </>
          )}
          {canEdit ? null : <p className="ow-muted ow-small">{dict.actuation.inspector.readOnly}</p>}
        </section>

        <section className="act-inspector__section" aria-labelledby={topTitle}>
          <h3 id={topTitle} className="act-inspector__title">
            {t.top.title}
          </h3>
          <Switch
            checked={draft.top !== null}
            disabled={disabled}
            label={t.top.change}
            onChange={(on) => {
              onDraft({ ...draft, top: on ? initialValue(values, "top", max) : null });
            }}
          />
          {draft.top === null ? (
            <p className="ow-muted ow-small">{t.top.unchanged}</p>
          ) : (
            <DistanceSlider
              label={t.top.label}
              value={draft.top}
              min={0}
              max={max}
              onChange={(units) => {
                onDraft({ ...draft, top: units });
              }}
              unitUm={unitUm}
              texts={texts}
              hint={`${t.top.hint} ${range}`}
              disabled={disabled}
            />
          )}
        </section>

        <section className="act-inspector__section" aria-labelledby={bottomTitle}>
          <h3 id={bottomTitle} className="act-inspector__title">
            {t.bottom.title}
          </h3>
          <Switch
            checked={draft.bottom !== null}
            disabled={disabled}
            label={t.bottom.change}
            onChange={(on) => {
              onDraft({ ...draft, bottom: on ? initialValue(values, "bottom", max) : null });
            }}
          />
          {draft.bottom === null ? (
            <p className="ow-muted ow-small">{t.bottom.unchanged}</p>
          ) : (
            <DistanceSlider
              label={t.bottom.label}
              value={draft.bottom}
              min={0}
              max={max}
              onChange={(units) => {
                onDraft({ ...draft, bottom: units });
              }}
              unitUm={unitUm}
              texts={texts}
              hint={`${t.bottom.hint} ${range}`}
              disabled={disabled}
            />
          )}
        </section>

        {first === undefined || firstTarget === null ? null : (
          <section className="act-inspector__section act-deadband__gauge" aria-labelledby={gaugeTitle}>
            <h3 id={gaugeTitle} className="act-inspector__title">
              {t.gauge.title({ key: nameOf(first) })}
            </h3>
            <TravelGauge
              label={t.gauge.label({ key: nameOf(first) })}
              formatMm={formatMm}
              maxMm={W75_TRAVEL_MM}
              actuationMm={firstActuation === undefined ? null : toMm(firstActuation)}
              deadZones={{ topMm: toMm(firstTarget.top), bottomMm: toMm(firstTarget.bottom) }}
              orientation="horizontal"
              length={220}
              showLegend
            />
          </section>
        )}

        {hints.length === 0 ? null : (
          <div className="act-inspector__summary">
            {hints.map((hint) => (
              <Callout key={`${hint.key}-${hint.kind}`} kind="warning">
                {t.hints[hint.kind]({ key: nameOf(hint.key), actuation: texts.mm(hint.actuation) })}
              </Callout>
            ))}
          </div>
        )}

        <div className="act-inspector__summary">
          <Disclosure summary={t.explainTitle}>
            <p className="ow-small">{t.explain}</p>
          </Disclosure>
        </div>
      </div>
    </Card>
  );
}
