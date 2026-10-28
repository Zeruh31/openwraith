// The result of a restore: the headline (CLI precedence: lost, then may be
// lost, then failed/cancelled), what a stop part-way left behind (macros
// emptied by the Macro 1 write, keys referring to an empty slot), the
// persistence and final checks, each item's result, the counts, the next
// steps and the report footer (safety backup with Undo, the active profile,
// foreign activity). An import of a profile file shows the same report in
// import words (`variant`).

import { Button } from "../../components/Button";
import { Callout } from "../../components/Feedback";
import { Disclosure, KeyValueList, TechnicalDetails, type KeyValueItem } from "../../components/Layout";
import { describeDetail, tableName } from "../../flow/labels";
import { HEADLINE_TONE, ItemDetails, ItemResultBadge, KeyName, ReportFooter, TableName } from "../../flow/Report";
import { keyName, useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { FinalCheckView, ItemReportView, ProfileReportView, RestoreReportView } from "../../ipc/bindings";
import { useStatus } from "../../state/status";
import { groupReportItems, tallyReport, worstItem, type RestoreTally } from "./model";
import { numberList, persistenceSentences, restoreHeadline, type ReviewVariant } from "./texts";

function ResultRow({ item }: { item: ItemReportView }) {
  const dict = useDict();
  return (
    <li className="ow-report__item">
      <span className="ow-report__name">
        {item.key === null ? (
          <TableName table={item.table} />
        ) : (
          <>
            <TableName table={item.table} /> · <KeyName keyRef={item.key} />
          </>
        )}
        {item.resent === null ? null : <span className="ow-muted ow-small"> · {dict.flow.resent[item.resent]}</span>}
      </span>
      <ItemResultBadge item={item} />
      <ItemDetails item={item} />
    </li>
  );
}

/** Macros a stop part-way left empty and keys left referring to an empty slot (Undo restores both). */
function LeftBehind({ report, variant }: { report: RestoreReportView; variant: ReviewVariant }) {
  const dict = useDict();
  const r = dict.backupRestore.result;
  const words = variant === "import" ? dict.backupRestore.share.result : r;
  if (report.cleared_macros.length === 0 && report.dangling.length === 0) return null;
  return (
    <>
      {report.cleared_macros.length === 0 ? null : (
        <Callout kind="danger" title={r.clearedTitle}>
          <p>{words.clearedBody}</p>
          <ul>
            {report.cleared_macros.map((c) => (
              <li key={`${c.profile}-${c.slot}`}>{r.clearedItem({ n: c.profile, slot: c.slot + 1 })}</li>
            ))}
          </ul>
        </Callout>
      )}
      {report.dangling.length === 0 ? null : (
        <Callout kind="danger" title={r.danglingTitle}>
          <p>{words.danglingBody}</p>
          <ul>
            {report.dangling.map((d) => {
              const key = keyName(dict, d.key.name);
              return (
                <li key={`${d.profile}-${d.layer}-${d.key.index}`}>
                  {r.danglingItem({
                    n: d.profile,
                    key: d.layer === "fn" ? `Fn + ${key}` : key,
                    target: tableName(d.slot, dict),
                  })}
                </li>
              );
            })}
          </ul>
        </Callout>
      )}
    </>
  );
}

/** Many per-key trigger results as one row (the worst result shown), expandable. */
function TriggerGroupRow({ items }: { items: readonly ItemReportView[] }) {
  const dict = useDict();
  const worst = worstItem(items);
  return (
    <li className="ow-report__item">
      <span className="ow-report__name">{dict.backupRestore.review.triggers({ n: items.length })}</span>
      {worst === undefined ? <span /> : <ItemResultBadge item={worst} />}
      <Disclosure summary={dict.common.actions.showAll}>
        <ul className="ow-report__items">
          {items.map((item, index) => (
            <ResultRow key={`${item.key?.index ?? "t"}-${index}`} item={item} />
          ))}
        </ul>
      </Disclosure>
    </li>
  );
}

function ProfileResult({ profile }: { profile: ProfileReportView }) {
  const dict = useDict();
  return (
    <section className="br-section">
      <h3 className="br-section__title">{dict.common.profile({ n: profile.profile })}</h3>
      <ul className="ow-report__items">
        {groupReportItems(profile.items).map((entry, index) =>
          entry.kind === "item" ? (
            <ResultRow key={index} item={entry.item} />
          ) : (
            <TriggerGroupRow key={index} items={entry.items} />
          ),
        )}
      </ul>
    </section>
  );
}

function FinalCheck({ check }: { check: FinalCheckView }) {
  const dict = useDict();
  const format = useFormat();
  const r = dict.backupRestore.result;
  return (
    <section className="br-section">
      <h3 className="br-section__title">{r.finalTitle}</h3>
      {check.mismatches.length === 0 ? (
        <p>{r.finalOk({ list: numberList(check.profiles, format), count: check.profiles.length })}</p>
      ) : (
        <>
          <p className="ow-danger-text">{r.finalMismatches}</p>
          <ul>
            {check.mismatches.map((m, index) => (
              <li key={index}>
                {r.finalMismatch({ n: m.profile, table: tableName(m.mismatch.table, dict) })}
              </li>
            ))}
          </ul>
          <TechnicalDetails>
            {check.mismatches
              .map((m) => `${m.mismatch.read}: ${m.mismatch.expected_hex} → ${m.mismatch.got_hex}`)
              .join("\n")}
          </TechnicalDetails>
        </>
      )}
    </section>
  );
}

function Counts({ tally, skipped, variant }: { tally: RestoreTally; skipped: number; variant: ReviewVariant }) {
  const dict = useDict();
  const format = useFormat();
  const c = dict.backupRestore.result.counts;
  const labels: Record<keyof typeof c, string> =
    variant === "import" ? { ...c, skipped: dict.backupRestore.share.result.skipped } : c;
  const all: [keyof typeof c, number][] = [
    ["written", tally.verified],
    ["kept", tally.kept],
    ["maybeLost", tally.maybeLost],
    ["lost", tally.lost],
    ["failed", tally.failed],
    ["notAttempted", tally.notAttempted],
    ["alreadyMatched", tally.alreadyMatched],
    ["resent", tally.resent],
    ["skipped", skipped],
  ];
  const items: KeyValueItem[] = all
    .filter(([, value]) => value > 0)
    .map(([key, value]) => ({ key, label: labels[key], value: <span className="ow-num">{format.number(value)}</span> }));
  if (items.length === 0) return null;
  return (
    <section className="br-section">
      <h3 className="br-section__title">{c.title}</h3>
      <KeyValueList items={items} />
    </section>
  );
}

/** Props of {@link RestoreResult}. */
export interface RestoreResultProps {
  report: RestoreReportView;
  /**
   * Prepares the same restore again (after LOST). Without it (the shell's
   * last-report dialog, where the backup is no longer loaded) a hint says
   * how to run it again instead.
   */
  onRunAgain?: (() => void) | undefined;
  /** Reads the keyboard again (after MAY BE LOST). */
  onReadAgain: () => void;
  /** An import of a profile file uses import words (default: a restore). */
  variant?: ReviewVariant | undefined;
}

/** The result of a restore. */
export function RestoreResult({ report, onRunAgain, onReadAgain, variant = "restore" }: RestoreResultProps) {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const r = dict.backupRestore.result;
  const tally = tallyReport(report);
  const ready = status.connection.state === "ready";
  const error = report.error;
  return (
    <div className="ow-report br-result">
      <Callout kind={HEADLINE_TONE[report.headline]} title={restoreHeadline(report, tally, dict, variant)}>
        {error === null ? null : (
          <>
            <p>{dict.errors.kind[error.kind].title}</p>
            {error.detail === null ? null : <p>{describeDetail(error.detail, dict, format)}</p>}
            <TechnicalDetails>{error.message}</TechnicalDetails>
          </>
        )}
      </Callout>
      <LeftBehind report={report} variant={variant} />
      {report.persistence === null ? null : (
        <section className="br-section">
          <h3 className="br-section__title">{r.persistenceTitle}</h3>
          <p>{persistenceSentences(report.persistence, dict, format).join(" ")}</p>
        </section>
      )}
      {report.final_check === null ? null : <FinalCheck check={report.final_check} />}
      {report.profiles.map((profile) => (
        <ProfileResult key={profile.profile} profile={profile} />
      ))}
      <Counts tally={tally} skipped={report.counts.skipped} variant={variant} />
      {report.headline === "lost" || report.headline === "maybe_lost" ? (
        <section className="br-section">
          <h3 className="br-section__title">{r.nextSteps}</h3>
          <div className="ow-row">
            {report.headline === "lost" ? (
              onRunAgain === undefined ? (
                <p>{r.runAgainHint}</p>
              ) : (
                <Button variant="primary" icon="refresh" disabledReason={ready ? null : r.connectFirst} onClick={onRunAgain}>
                  {variant === "import" ? dict.backupRestore.share.result.runAgain : r.runAgain}
                </Button>
              )
            ) : (
              <Button variant="primary" icon="refresh" disabledReason={ready ? null : r.connectFirst} onClick={onReadAgain}>
                {dict.common.actions.readAgain}
              </Button>
            )}
          </div>
        </section>
      ) : null}
      <ReportFooter
        safetyBackup={report.safety_backup}
        activeProfileNow={report.active_profile_now}
        foreignActivity={report.foreign_activity}
      />
    </div>
  );
}
