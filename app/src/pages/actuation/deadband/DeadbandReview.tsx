// The review body of a dead band edit (inside the shared WriteFlow): every
// key whose dead band changes, Now → New; the backend's soft warnings
// against the keys' actuation points; the plan's warnings; and the writes
// under "Technical details".

import { Callout } from "../../../components/Feedback";
import { Table, TechnicalDetails, type TableColumn } from "../../../components/Layout";
import { KeyName } from "../../../flow/Report";
import { keyName, useDict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type { DeadBandChangeView, DeadBandEditPreview, DeadBandValue } from "../../../ipc/bindings";
import { EditWarnings, planTechnical } from "../shared/EditParts";
import { valueTexts } from "../model";

const ROWS_SHOWN = 12;

/** Props of {@link DeadbandReview}. */
export interface DeadbandReviewProps {
  preview: DeadBandEditPreview;
  unitUm: number;
}

/** What the dead band edit will write. */
export function DeadbandReview({ preview, unitUm }: DeadbandReviewProps) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.actuation.deadband;
  const texts = valueTexts(dict, format, unitUm);
  const setting = (value: DeadBandValue) => t.setting({ top: texts.mm(value.top), bottom: texts.mm(value.bottom) });
  const part = (changed: boolean, text: string) =>
    changed ? <strong>{text}</strong> : <span className="ow-muted">{text}</span>;
  const columns: TableColumn<DeadBandChangeView>[] = [
    { key: "key", header: dict.actuation.review.key, rowHeader: true, render: (row) => <KeyName keyRef={row.key} /> },
    { key: "now", header: dict.actuation.review.now, render: (row) => setting(row.before) },
    {
      key: "next",
      header: dict.actuation.review.next,
      render: (row) => (
        <span className="act-review__new">
          <span aria-hidden="true">→ </span>
          {part(row.before.top !== row.after.top, t.settingTop({ value: texts.mm(row.after.top) }))}
          <span aria-hidden="true"> · </span>
          {part(row.before.bottom !== row.after.bottom, t.settingBottom({ value: texts.mm(row.after.bottom) }))}
        </span>
      ),
    },
  ];
  return (
    <div className="ow-stack act-review">
      <p>{t.review.intro({ profile: preview.profile })}</p>
      <Table
        columns={columns}
        rows={preview.changes}
        rowKey={(row) => String(row.key.index)}
        limit={ROWS_SHOWN}
        caption={<span className="sr-only">{dict.actuation.apply.willChange({ n: preview.changes.length })}</span>}
      />
      {preview.hints.map((hint) => (
        <Callout key={`${hint.key.index}-${hint.kind}`} kind="warning">
          {t.hints[hint.kind]({ key: keyName(dict, hint.key.name), actuation: texts.mm(hint.actuation) })}
        </Callout>
      ))}
      <EditWarnings warnings={preview.warnings} />
      <TechnicalDetails>{planTechnical(preview.writes, preview.packets, dict)}</TechnicalDetails>
    </div>
  );
}
