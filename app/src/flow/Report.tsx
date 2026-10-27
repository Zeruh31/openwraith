// Report building blocks shared by every page: table and key names, the
// per-item result badge (icon + word + colour, never colour alone), the
// report footer (safety backup, Undo, active profile) and the edit report
// used by the actuation and lighting pages.

import { Button } from "../components/Button";
import { Callout } from "../components/Feedback";
import { Icon, type IconName } from "../components/Icon";
import { Disclosure, TechnicalDetails } from "../components/Layout";
import { keyName, useDict, type Dict } from "../i18n";
import { useFormat } from "../i18n/format";
import type {
  EditReportView as EditReport,
  FileRef,
  ForeignActivityView,
  Headline,
  ItemReportView,
  KeyRef,
  TableRef,
} from "../ipc/bindings";
import { api } from "../ipc/commands";
import { toAppError } from "../ipc/errors";
import { navigate } from "../routes";
import { useStatus } from "../state/status";
import { toast } from "../state/toasts";
import { describeDetail, tableName, warningText } from "./labels";

/** A table's localized name. */
export function TableName({ table }: { table: TableRef }) {
  const dict = useDict();
  return <>{tableName(table, dict)}</>;
}

/** A key's localized accessible name ("Left Shift"), or its legend ("W"). */
export function KeyName({ keyRef }: { keyRef: Pick<KeyRef, "name"> }) {
  const dict = useDict();
  return <>{keyName(dict, keyRef.name)}</>;
}

/** How a result looks. */
export type ResultTone = "success" | "neutral" | "warning" | "danger" | "critical";

/** The badge of an item's result: tone, icon and words. */
export function itemResult(item: ItemReportView, dict: Dict): { tone: ResultTone; icon: IconName; text: string } {
  const results = dict.flow.results;
  const result = item.result;
  switch (result.result) {
    case "written":
      if (item.resent_unchanged) return { tone: "neutral", icon: "refresh", text: results.resentUnchanged };
      return result.persistence === "persisted"
        ? { tone: "success", icon: "check-double", text: results.persisted }
        : { tone: "success", icon: "check", text: results.written };
    case "already_matched":
      return { tone: "neutral", icon: "equal", text: results.alreadyMatched };
    case "maybe_lost":
      return result.warnings.some((w) => w.code === "base_keymap_not_saved")
        ? { tone: "critical", icon: "warning", text: results.maybeLostBaseKeymap }
        : { tone: "warning", icon: "warning", text: results.maybeLost };
    case "lost":
      return { tone: "danger", icon: "error", text: results.lost };
    case "failed":
      return {
        tone: "danger",
        icon: "error",
        text: results.failed({ reason: dict.errors.kind[result.error.kind].title }),
      };
    case "not_attempted":
      return { tone: "neutral", icon: "minus", text: results.notAttempted };
  }
}

/** An item's result as icon + word + colour. */
export function ItemResultBadge({ item }: { item: ItemReportView }) {
  const dict = useDict();
  const { tone, icon, text } = itemResult(item, dict);
  return (
    <span className={`ow-result ow-result--${tone}`}>
      <Icon name={icon} size={16} />
      <span>{text}</span>
    </span>
  );
}

/** Warnings, mismatches and errors of one item, for "Technical details". */
export function ItemDetails({ item }: { item: ItemReportView }) {
  const dict = useDict();
  const format = useFormat();
  const result = item.result;
  const lines: string[] = [];
  if (result.result === "written" || result.result === "lost" || result.result === "maybe_lost") {
    lines.push(dict.flow.items.sends({ n: result.sends }));
    for (const warning of result.warnings) lines.push(warningText(warning, dict, format));
  }
  const mismatch =
    result.result === "lost" ? result.mismatch : result.result === "failed" ? result.mismatch : null;
  if (result.result === "failed") {
    const detail = result.error.detail === null ? null : describeDetail(result.error.detail, dict, format);
    if (detail !== null) lines.push(detail);
    lines.push(result.error.message);
  }
  if (mismatch !== null) {
    lines.push(`${dict.errors.mismatchKind[mismatch.kind]} (${mismatch.read})`);
    if (mismatch.offsets.length > 0) {
      lines.push(dict.flow.items.offsets({ list: mismatch.offsets.join(", ") }));
    }
    lines.push(`${dict.flow.items.expected}: ${mismatch.expected_hex}`);
    lines.push(`${dict.flow.items.got}: ${mismatch.got_hex}`);
  }
  if (lines.length === 0) {
    return null;
  }
  return <TechnicalDetails>{lines.join("\n")}</TechnicalDetails>;
}

/** Props of {@link ReportFooter}. */
export interface ReportFooterProps {
  safetyBackup: FileRef;
  activeProfileNow: number | null;
  foreignActivity: ForeignActivityView | null;
  /** The profile an edit was for (named in the report). */
  editedProfile?: number | undefined;
}

/** Connection states in which the keyboard is not (or no longer) connected. */
const DISCONNECTED: readonly string[] = [
  "starting",
  "searching",
  "several_keyboards",
  "open_failed",
  "lost",
  "reset_seen",
  "unsupported",
  "released",
  "fault",
];

/** "Safety backup: <file>" with Show in folder and Undo, the active profile, foreign activity. */
export function ReportFooter({ safetyBackup, activeProfileNow, foreignActivity, editedProfile }: ReportFooterProps) {
  const dict = useDict();
  const status = useStatus();
  const connected = status.connection.state === "ready";
  const gone = DISCONNECTED.includes(status.connection.state);
  return (
    <div className="ow-report-footer">
      <div className="ow-report-footer__backup">
        <span className="ow-strong">{dict.flow.footer.safetyBackup}</span>{" "}
        <span className="ow-mono ow-break ow-selectable" title={safetyBackup.display_path}>
          {safetyBackup.file_name}
        </span>
      </div>
      <div className="ow-row">
        <Button
          size="sm"
          icon="folder"
          onClick={() => {
            api.reveal({ target: "file", file: safetyBackup.token }).catch((error: unknown) => {
              const appError = toAppError(error);
              toast({ kind: "danger", title: dict.errors.kind[appError.kind].title, details: appError.message });
            });
          }}
        >
          {dict.common.actions.showInFolder}
        </Button>
        <Button
          size="sm"
          icon="undo"
          disabledReason={connected ? null : dict.flow.footer.undoDisabled}
          onClick={() => {
            navigate({ page: "backup", params: { restore: safetyBackup.token } });
          }}
        >
          {dict.common.actions.undo}
        </Button>
      </div>
      {editedProfile === undefined ? null : <p>{dict.flow.footer.editedProfile({ n: editedProfile })}</p>}
      <p>
        {activeProfileNow === null
          ? dict.flow.footer.activeProfileUnknown
          : gone
            ? dict.flow.footer.activeProfileWhenLost({ n: activeProfileNow })
            : dict.flow.footer.activeProfile({ n: activeProfileNow })}
      </p>
      {foreignActivity === null ? null : <Callout kind="warning">{dict.flow.footer.foreignActivity}</Callout>}
    </div>
  );
}

/** The callout look of each headline. */
export const HEADLINE_TONE: Record<Headline, "success" | "info" | "warning" | "danger"> = {
  done: "success",
  nothing_to_do: "info",
  cancelled: "warning",
  not_written: "danger",
  failed: "danger",
  maybe_lost: "warning",
  lost: "danger",
};

/** Counts of an item list: written (sent), maybe lost, lost, and the total that needed writing. */
export function countItems(items: readonly ItemReportView[]): {
  written: number;
  maybeLost: number;
  lost: number;
  total: number;
} {
  let written = 0;
  let maybeLost = 0;
  let lost = 0;
  let total = 0;
  for (const item of items) {
    const kind = item.result.result;
    if (kind !== "already_matched") total += 1;
    if (kind === "written" || kind === "maybe_lost" || kind === "lost") written += 1;
    if (kind === "maybe_lost") maybeLost += 1;
    if (kind === "lost") lost += 1;
  }
  return { written, maybeLost, lost, total };
}

/** The headline of an edit report (keys or the light). */
export function editHeadline(report: EditReport, dict: Dict): string {
  const h = dict.flow.headline;
  const counts = countItems(report.items);
  const light = report.items.some((item) => item.table.table === "main_light");
  switch (report.headline) {
    case "done":
      return light ? h.doneLight : h.doneKeys({ n: counts.written });
    case "nothing_to_do":
      return h.nothingToDo;
    case "cancelled":
      return h.cancelled({ written: counts.written, total: counts.total });
    case "not_written":
      return h.notWritten;
    case "failed":
      return h.failed({ written: counts.written, total: counts.total });
    case "maybe_lost":
      // Reading the keyboard again reads the active profile: when that is
      // another one, the user has to switch back first.
      return report.active_profile_now !== null && report.active_profile_now !== report.profile
        ? h.maybeLostOtherProfile({ n: counts.maybeLost, profile: report.profile })
        : h.maybeLost({ n: counts.maybeLost });
    case "lost":
      return h.lost({ n: counts.lost });
  }
}

const ITEMS_SHOWN = 12;

/**
 * The result of a trigger or light edit: headline, the wait for the save,
 * the error if any, each item's result and the report footer. The words
 * "saved permanently" are never used: an edit's persistence isn't checked.
 */
export function EditReportView({ report }: { report: EditReport }) {
  const dict = useDict();
  const format = useFormat();
  const headline = editHeadline(report, dict);
  const items = report.items;
  const renderItem = (item: ItemReportView, index: number) => (
    <li key={`${item.key?.index ?? "t"}-${index}`} className="ow-report__item">
      <span className="ow-report__name">
        {item.key === null ? <TableName table={item.table} /> : <KeyName keyRef={item.key} />}
      </span>
      <ItemResultBadge item={item} />
      <ItemDetails item={item} />
    </li>
  );
  return (
    <div className="ow-report">
      <Callout kind={HEADLINE_TONE[report.headline]} title={headline}>
        {report.waited_for_commit_ms === null || !report.anything_written ? null : (
          <p>{dict.flow.headline.waited({ seconds: format.seconds(report.waited_for_commit_ms) })}</p>
        )}
        {report.error === null ? null : (
          <>
            <p>{dict.errors.kind[report.error.kind].title}</p>
            {report.error.detail === null ? null : <p>{describeDetail(report.error.detail, dict, format)}</p>}
            <TechnicalDetails>{report.error.message}</TechnicalDetails>
          </>
        )}
      </Callout>
      {items.length === 0 ? null : (
        <>
          <ul className="ow-report__items">{items.slice(0, ITEMS_SHOWN).map(renderItem)}</ul>
          {items.length > ITEMS_SHOWN ? (
            <Disclosure summary={dict.common.showMore({ n: items.length - ITEMS_SHOWN })}>
              <ul className="ow-report__items">
                {items.slice(ITEMS_SHOWN).map((item, i) => renderItem(item, i + ITEMS_SHOWN))}
              </ul>
            </Disclosure>
          ) : null}
        </>
      )}
      <ReportFooter
        safetyBackup={report.safety_backup}
        activeProfileNow={report.active_profile_now}
        foreignActivity={report.foreign_activity}
        editedProfile={report.profile}
      />
    </div>
  );
}
