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
  ClearedMacroView,
  DanglingBindingView,
  EditReportView as EditReport,
  FileRef,
  ForeignActivityView,
  Headline,
  ItemReportView,
  KeyRef,
  PersistenceCheckView,
  TableRef,
} from "../ipc/bindings";
import { api } from "../ipc/commands";
import { toAppError } from "../ipc/errors";
import { navigate } from "../routes";
import { useStatus } from "../state/status";
import { toast } from "../state/toasts";
import { describeDetail, keyRefText, tableName, warningText, writeReasonTexts } from "./labels";

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

/**
 * The headline of an edit report: trigger keys are counted ("2 keys
 * written"); the light, and edits written as whole tables (keymaps, macro
 * and advanced-key slots, key colours, dead band, SOCD), are not.
 */
export function editHeadline(report: EditReport, dict: Dict): string {
  const h = dict.flow.headline;
  const counts = countItems(report.items);
  const light = report.items.some((item) => item.table.table === "main_light");
  const perKey = report.items.some((item) => item.key !== null);
  switch (report.headline) {
    case "done":
      if (light) return h.doneLight;
      return perKey ? h.doneKeys({ n: counts.written }) : h.done;
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

/** One result row: the key or the table, why it was written (re-sends, the commit trigger), the result. */
export function EditItemRow({ item }: { item: ItemReportView }) {
  const dict = useDict();
  const reasons = writeReasonTexts(item, dict);
  return (
    <li className="ow-report__item">
      <span className="ow-report__name">
        {item.key === null ? <TableName table={item.table} /> : <KeyName keyRef={item.key} />}
        {reasons.length === 0 ? null : <span className="ow-report__reason ow-small ow-muted">{reasons.join(" ")}</span>}
      </span>
      <ItemResultBadge item={item} />
      <ItemDetails item={item} />
    </li>
  );
}

/** Props of {@link LeftBehind}. */
export interface LeftBehindProps {
  cleared: readonly ClearedMacroView[];
  dangling: readonly DanglingBindingView[];
}

/**
 * What a stop part-way left behind: macros emptied by the Macro 1 write and
 * keys left referring to an empty slot (Undo restores both).
 */
export function LeftBehind({ cleared, dangling }: LeftBehindProps) {
  const dict = useDict();
  const r = dict.flow.report;
  if (cleared.length === 0 && dangling.length === 0) return null;
  return (
    <>
      {cleared.length === 0 ? null : (
        <Callout kind="danger" title={r.clearedTitle}>
          <p>{r.clearedBody}</p>
          <ul>
            {cleared.map((c) => (
              <li key={`${c.profile}-${c.slot}`}>{r.clearedItem({ n: c.slot + 1 })}</li>
            ))}
          </ul>
        </Callout>
      )}
      {dangling.length === 0 ? null : (
        <Callout kind="danger" title={r.danglingTitle}>
          <p>{r.danglingBody}</p>
          <ul>
            {dangling.map((d) => (
              <li key={`${d.profile}-${d.layer}-${d.key.index}`}>
                {r.danglingItem({ key: keyRefText(d.key, d.layer, dict), target: tableName(d.slot, dict) })}
              </li>
            ))}
          </ul>
        </Callout>
      )}
    </>
  );
}

/** The persistence check of an edit in words (kept / lost / not checked separately). */
export function persistenceTexts(check: PersistenceCheckView, dict: Dict): string[] {
  const r = dict.flow.report;
  const texts: string[] = [];
  if (check.persisted > 0 && check.lost === 0) texts.push(r.persistenceOk({ n: check.persisted }));
  if (check.lost > 0) texts.push(r.persistenceLost({ lost: check.lost }));
  if (check.not_checked > 0) texts.push(r.persistenceNotChecked({ n: check.not_checked }));
  return texts;
}

/**
 * The result of an edit (triggers, the light, key assignments): headline,
 * the wait for the save, the error if any, a note when Stop came too late
 * to skip anything, what a stop left behind (emptied macros, keys left on
 * them), the persistence check, each item's result with its reasons, and
 * the report footer (safety backup, Undo). The words "saved permanently"
 * are never used; "Saved" appears only after a persistence check.
 */
export function EditReportView({ report }: { report: EditReport }) {
  const dict = useDict();
  const format = useFormat();
  const headline = editHeadline(report, dict);
  const items = report.items;
  const renderItem = (item: ItemReportView, index: number) => (
    <EditItemRow key={`${item.key?.index ?? "t"}-${index}`} item={item} />
  );
  return (
    <div className="ow-report">
      <Callout kind={HEADLINE_TONE[report.headline]} title={headline}>
        {report.waited_for_commit_ms === null || !report.anything_written ? null : (
          <p>{dict.flow.headline.waited({ seconds: format.seconds(report.waited_for_commit_ms) })}</p>
        )}
        {report.cancel_requested && report.outcome === "completed" ? <p>{dict.flow.report.cancelRequested}</p> : null}
        {report.error === null ? null : (
          <>
            <p>{dict.errors.kind[report.error.kind].title}</p>
            {report.error.detail === null ? null : <p>{describeDetail(report.error.detail, dict, format)}</p>}
            <TechnicalDetails>{report.error.message}</TechnicalDetails>
          </>
        )}
      </Callout>
      <LeftBehind cleared={report.cleared_macros} dangling={report.dangling} />
      {report.persistence === null ? null : (
        <section className="ow-report__section">
          <h3 className="ow-report__section-title">{dict.flow.report.persistenceTitle}</h3>
          <p>{persistenceTexts(report.persistence, dict).join(" ")}</p>
        </section>
      )}
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
