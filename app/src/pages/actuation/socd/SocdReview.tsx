// The review body of a SOCD edit (inside the shared WriteFlow): the power
// before → after (one setting of the whole keyboard), the pairs after the
// write with their modes (modes 2 and 3 "Not yet verified", from the
// library's evidence), the pairs now, the plan's warnings, and the writes
// under "Technical details".

import { Badge, EvidenceBadge } from "../../../components/Badge";
import { Disclosure, Table, TechnicalDetails, type TableColumn } from "../../../components/Layout";
import { useDict } from "../../../i18n";
import type { EvidenceView, SocdEditPreview, SocdPairView } from "../../../ipc/bindings";
import { EditWarnings, planTechnical } from "../shared/EditParts";
import { modeName, pairKeysText } from "./labels";
import { samePlanPairs } from "./model";

/** Props of {@link SocdReview}. */
export interface SocdReviewProps {
  preview: SocdEditPreview;
  /** The evidence of each mode, from the state. */
  evidence: (mode: number) => EvidenceView | null;
}

interface Row {
  n: number;
  pair: SocdPairView;
}

/** What the SOCD edit will write. */
export function SocdReview({ preview, evidence }: SocdReviewProps) {
  const dict = useDict();
  const t = dict.actuation.socd;
  const powerChanges = preview.power_before !== preview.power_after;
  const pairsChange = !samePlanPairs(preview.pairs_before, preview.pairs_after);
  const columns: TableColumn<Row>[] = [
    { key: "n", header: t.review.pair, rowHeader: true, render: (row) => row.n },
    { key: "keys", header: t.review.keys, render: (row) => pairKeysText(row.pair, dict) },
    {
      key: "mode",
      header: t.review.mode,
      render: (row) => (
        <span className="ow-row">
          <span>{modeName(row.pair.mode, dict)}</span>
          <EvidenceBadge evidence={evidence(row.pair.mode)} size="sm" />
        </span>
      ),
    },
  ];
  const rows = (pairs: readonly SocdPairView[]): Row[] => pairs.map((pair, i) => ({ n: i + 1, pair }));
  const onOff = (on: boolean) => (on ? t.power.on : t.power.off);

  return (
    <div className="ow-stack act-review">
      <p>{t.review.intro({ profile: preview.profile })}</p>
      {powerChanges ? (
        <div className="act-socd-review__power">
          <p className="ow-strong">
            {t.review.power({ from: onOff(preview.power_before), to: onOff(preview.power_after) })}
          </p>
          <Badge kind="keyboard_wide" size="sm" />
        </div>
      ) : (
        <p className="ow-muted">{t.review.powerSame({ state: onOff(preview.power_after) })}</p>
      )}
      {pairsChange ? (
        <>
          <h3 className="act-inspector__title">{t.review.pairsAfter({ n: preview.pairs_after.length, profile: preview.profile })}</h3>
          {preview.pairs_after.length === 0 ? (
            <p>{t.review.noPairs}</p>
          ) : (
            <Table columns={columns} rows={rows(preview.pairs_after)} rowKey={(row) => String(row.n)} />
          )}
          <Disclosure summary={t.review.pairsBefore({ n: preview.pairs_before.length })}>
            {preview.pairs_before.length === 0 ? (
              <p>{t.review.noPairs}</p>
            ) : (
              <Table columns={columns} rows={rows(preview.pairs_before)} rowKey={(row) => String(row.n)} />
            )}
          </Disclosure>
        </>
      ) : (
        <p className="ow-muted">{t.review.pairsSame({ n: preview.pairs_after.length })}</p>
      )}
      <EditWarnings warnings={preview.warnings} />
      <TechnicalDetails>{planTechnical(preview.writes, preview.packets, dict)}</TechnicalDetails>
    </div>
  );
}
