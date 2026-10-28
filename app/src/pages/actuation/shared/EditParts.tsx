// Review and result parts shared by the dead band and SOCD tabs (and the
// presets): a refused plan in plain words, the plan's warnings, the
// optional "Check that the keyboard kept it" (never pre-ticked) and the
// technical details of the writes.

import type { ReactNode } from "react";

import { EvidenceBadge } from "../../../components/Badge";
import { Checkbox } from "../../../components/Controls";
import { Callout } from "../../../components/Feedback";
import { TechnicalDetails } from "../../../components/Layout";
import { tableName } from "../../../flow/labels";
import type { PrepareErrorView } from "../../../flow/WriteFlow";
import { useDict, type Dict } from "../../../i18n";
import { useFormat, type Format } from "../../../i18n/format";
import type { AppError, EditWarningView, EditWriteView, PersistencePlanView } from "../../../ipc/bindings";
import { editProblemOf, editProblemText, editWarningText } from "./editTexts";

/** A refused edit: what was refused, that nothing was written, the backend's message. */
export function EditRefusal({ error, unitUm, children }: { error: AppError; unitUm: number; children?: ReactNode }) {
  const dict = useDict();
  const format = useFormat();
  const problem = editProblemOf(error);
  const kind = dict.errors.kind[error.kind];
  return (
    <Callout kind={error.kind === "stale_plan" ? "warning" : "danger"} title={kind.title} role="alert">
      <p>{problem === null ? kind.body : editProblemText(problem, dict, format, unitUm)}</p>
      {children}
      <p className="ow-strong">{dict.flow.nothingWritten}</p>
      {error.message === "" ? null : <TechnicalDetails>{error.message}</TechnicalDetails>}
    </Callout>
  );
}

/** The flow's view of a refused prepare: edit refusals in the page's words, others the standard view. */
export function editPrepareError(unitUm: number): (error: AppError) => PrepareErrorView | null {
  return (error) => (editProblemOf(error) === null ? null : { body: <EditRefusal error={error} unitUm={unitUm} /> });
}

/** The line under a failed write that explains an edit refusal, or nothing. */
export function editErrorNote(dict: Dict, format: Format, unitUm: number): (error: AppError) => ReactNode {
  return (error) => {
    const problem = editProblemOf(error);
    return problem === null ? null : editProblemText(problem, dict, format, unitUm);
  };
}

/** The plan's warnings, each one a callout ("Not yet verified" ones with the badge). */
export function EditWarnings({ warnings }: { warnings: readonly EditWarningView[] }) {
  const dict = useDict();
  if (warnings.length === 0) return null;
  return (
    <div className="ow-stack act-warnings">
      {warnings.map((warning, index) => (
        <Callout key={`${warning.code}-${index}`} kind={warning.code === "not_undoable" ? "warning" : "info"}>
          <p className="act-warnings__line">
            <span>{editWarningText(warning, dict)}</span>
            {warning.code === "unverified" ? (
              <EvidenceBadge evidence={{ status: "unverified", notes: [warning.note] }} size="sm" />
            ) : null}
          </p>
        </Callout>
      ))}
    </div>
  );
}

/** "Check that the keyboard kept it" in the review's footer (preparing again when it changes). */
export function PersistenceOption({
  persistence,
  onChange,
}: {
  persistence: PersistencePlanView;
  onChange: (checked: boolean) => void;
}) {
  const dict = useDict();
  const t = dict.actuation.edit.persistence;
  if (persistence.forced) return <p className="ow-small">{t.forced}</p>;
  return (
    <Checkbox
      checked={persistence.requested}
      label={t.check({ m: persistence.away_profile })}
      description={t.hint}
      onChange={onChange}
    />
  );
}

/** The writes of a plan, for "Technical details". */
export function planTechnical(writes: readonly EditWriteView[], packets: number, dict: Dict, extra: string[] = []): string {
  const t = dict.actuation.edit;
  const lines = writes.map((write) => `${tableName(write.table, dict)}: ${write.summary} · ${t.packets({ n: write.packets })}`);
  return [...lines, t.packetsTotal({ n: packets }), ...extra].join("\n");
}
