// The review body of a trigger edit (inside the shared WriteFlow): every key
// the backend planned from a fresh read, Now → New, changed keys first; the
// keys that already have the settings collapsed; keys left out; and the
// packet count under "Technical details".

import { Callout } from "../../components/Feedback";
import { Disclosure, Table, TechnicalDetails, type TableColumn } from "../../components/Layout";
import { KeyName } from "../../flow/Report";
import { useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { KeyEditView, TriggerEditPreview, TriggerValue } from "../../ipc/bindings";
import { isRtOn, valueTexts, type ValueTexts } from "./model";

const ROWS_SHOWN = 12;

/** The changed part of a setting is bold; the unchanged part is muted. */
function Part({ changed, text }: { changed: boolean; text: string }) {
  return changed ? <strong>{text}</strong> : <span className="ow-muted">{text}</span>;
}

/** The new setting, with the parts that change in bold. */
function NewSetting({ current, target, texts }: { current: TriggerValue; target: TriggerValue; texts: ValueTexts }) {
  const actuationChanged = current.actuation !== target.actuation;
  const rtChanged =
    current.mode !== target.mode ||
    (isRtOn(target) && (current.rt_press !== target.rt_press || current.rt_release !== target.rt_release));
  return (
    <span className="act-review__new">
      <span aria-hidden="true">→ </span>
      <Part changed={actuationChanged} text={texts.mm(target.actuation)} />
      <span aria-hidden="true"> · </span>
      <Part changed={rtChanged} text={texts.rt(target)} />
    </span>
  );
}

/** Props of {@link TriggerReview}. */
export interface TriggerReviewProps {
  preview: TriggerEditPreview;
  /** Selected keys left out of the intent (can't be changed). */
  leftOut: number;
  /** The keyboard's distance unit, in micrometres. */
  unitUm: number;
  /** The preset the plan comes from, if any. */
  presetName?: string | null | undefined;
}

/** What the trigger edit will write. */
export function TriggerReview({ preview, leftOut, unitUm, presetName = null }: TriggerReviewProps) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.actuation.review;
  const texts = valueTexts(dict, format, unitUm);
  const changing = preview.keys.filter((key) => key.changes);
  const unchanged = preview.keys.filter((key) => !key.changes);
  const keyColumn: TableColumn<KeyEditView> = {
    key: "key",
    header: t.key,
    rowHeader: true,
    render: (row) => <KeyName keyRef={row.key} />,
  };
  const columns: TableColumn<KeyEditView>[] = [
    keyColumn,
    { key: "now", header: t.now, render: (row) => texts.setting(row.current) },
    {
      key: "next",
      header: t.next,
      render: (row) => <NewSetting current={row.current} target={row.target} texts={texts} />,
    },
  ];
  const unchangedColumns: TableColumn<KeyEditView>[] = [
    keyColumn,
    { key: "now", header: t.now, render: (row) => texts.setting(row.current) },
  ];
  return (
    <div className="ow-stack act-review">
      {presetName === null ? null : <p className="ow-strong">{t.preset({ name: presetName })}</p>}
      <p>{t.intro({ profile: preview.profile })}</p>
      {changing.length === 0 ? null : (
        <Table
          columns={columns}
          rows={changing}
          rowKey={(row) => String(row.key.index)}
          limit={ROWS_SHOWN}
          caption={<span className="sr-only">{dict.actuation.apply.willChange({ n: changing.length })}</span>}
        />
      )}
      {unchanged.length === 0 ? null : (
        <Disclosure summary={t.unchanged({ n: unchanged.length })}>
          <Table
            columns={unchangedColumns}
            rows={unchanged}
            rowKey={(row) => String(row.key.index)}
            limit={ROWS_SHOWN}
          />
        </Disclosure>
      )}
      {leftOut === 0 ? null : <Callout kind="warning">{t.leftOut({ n: leftOut })}</Callout>}
      <TechnicalDetails>{t.packets({ n: preview.packets })}</TechnicalDetails>
    </div>
  );
}

/** The body when the backend found nothing to change. */
export function NothingToChange() {
  const dict = useDict();
  return <Callout kind="info" title={dict.actuation.review.nothing} />;
}
