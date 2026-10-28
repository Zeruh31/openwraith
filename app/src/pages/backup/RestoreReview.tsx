// The review of a restore plan (the WriteFlow's body): the facts, warnings,
// then one section per profile with what will be written, what isn't
// restored and why, and what already matches; totals at the end. An import
// of a profile file uses the same review in import words (`variant`).

import { createContext, useContext, type ReactNode } from "react";

import { Callout } from "../../components/Feedback";
import { Icon } from "../../components/Icon";
import {
  ColorSwatch,
  Disclosure,
  KeyValueList,
  Table,
  TechnicalDetails,
  type KeyValueItem,
  type TableColumn,
} from "../../components/Layout";
import { KeyName, TableName } from "../../flow/Report";
import { keyName, useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { ChangeView, KeyRef, LightBytesView, PlanWarningView, RestorePreview, TableRef } from "../../ipc/bindings";
import {
  condenseTables,
  groupPlan,
  planChanges,
  profilesWithChanges,
  type ProfileGroup,
  type SkipItem,
  type TriggerRow,
  type WriteItem,
} from "./model";
import {
  advancedText,
  blockCauseTechnical,
  deadBandText,
  keyNamesText,
  lightModeName,
  macroText,
  numberList,
  planWarningView,
  skipReasonText,
  tableGroupName,
  triggerText,
  type ReviewVariant,
} from "./texts";

/** Changes listed per table before "N more" (keys with dead bands or colours). */
const LIST_LIMIT = 12;

/** The flow the review belongs to (its sections read it for their words). */
const VariantContext = createContext<ReviewVariant>("restore");

/** The review's words that differ between a restore and an import. */
function useReviewWords() {
  const dict = useDict();
  const variant = useContext(VariantContext);
  const t = dict.backupRestore.review;
  const s = dict.backupRestore.share.review;
  const importing = variant === "import";
  return {
    variant,
    importing,
    backupColumn: importing ? s.columnFile : t.columns.backup,
    profileSkipped: importing ? s.profileSkipped : t.profileSkipped,
    notRestored: importing ? s.notImported : t.notRestored,
    notInBackup: importing ? s.notInFile : t.notInBackup,
    fileLabel: importing ? s.file : t.facts.file,
    madeLabel: importing ? s.fileMade : t.facts.backupMade,
    nothing: importing ? s.nothing : t.nothing,
    nothingSkips: importing ? s.nothingSkips : t.nothingSkips,
  };
}

type ChangeOf<K extends ChangeView["kind"]> = Extract<ChangeView, { kind: K }>;

function ofKind<K extends ChangeView["kind"]>(changes: readonly ChangeView[], kind: K): ChangeOf<K>[] {
  return changes.filter((change): change is ChangeOf<K> => change.kind === kind);
}

/** A table, or a key of a per-key table ("Triggers · W"). */
function ItemName({ table, keyRef }: { table: TableRef; keyRef: KeyRef | null }) {
  return keyRef === null ? (
    <TableName table={table} />
  ) : (
    <>
      <TableName table={table} /> · <KeyName keyRef={keyRef} />
    </>
  );
}

/** Before → after on one line, the new value emphasised. */
function FromTo({ from, to }: { from: string; to: string }) {
  return (
    <p className="br-fromto">
      <span>{from}</span>
      <span aria-hidden="true"> → </span>
      <strong>{to}</strong>
    </p>
  );
}

function TriggerTable({ rows }: { rows: readonly TriggerRow[] }) {
  const dict = useDict();
  const format = useFormat();
  const words = useReviewWords();
  const c = dict.backupRestore.review.columns;
  const columns: TableColumn<TriggerRow>[] = [
    { key: "key", header: c.key, rowHeader: true, render: (row) => <KeyName keyRef={row.key} /> },
    { key: "now", header: c.now, render: (row) => triggerText(row.now, dict, format) },
    {
      key: "backup",
      header: words.backupColumn,
      render: (row) => <strong>{triggerText(row.backup, dict, format)}</strong>,
    },
  ];
  return <Table columns={columns} rows={rows} rowKey={(row) => String(row.key.index)} limit={LIST_LIMIT} />;
}

interface LightRow {
  key: string;
  label: string;
  now: ReactNode;
  backup: ReactNode;
  changed: boolean;
}

function LightChange({ old, next }: { old: LightBytesView; next: LightBytesView }) {
  const dict = useDict();
  const format = useFormat();
  const words = useReviewWords();
  const c = dict.backupRestore.changes;
  const cols = dict.backupRestore.review.columns;
  if (old.state === "raw" || next.state === "raw") {
    const hex = (view: LightBytesView) => (view.state === "raw" ? view.hex : c.raw);
    return (
      <>
        <p>{c.raw}</p>
        <TechnicalDetails>{`${hex(old)} → ${hex(next)}`}</TechnicalDetails>
      </>
    );
  }
  const a = old.value;
  const b = next.value;
  const rows: LightRow[] = [
    {
      key: "effect",
      label: c.light.effect,
      now: lightModeName(a.mode, dict),
      backup: lightModeName(b.mode, dict),
      changed: a.mode.id !== b.mode.id,
    },
    {
      key: "color",
      label: c.light.color,
      now: <ColorSwatch color={a.color} />,
      backup: <ColorSwatch color={b.color} />,
      changed: a.color !== b.color,
    },
    {
      key: "brightness",
      label: c.light.brightness,
      now: format.number(a.brightness),
      backup: format.number(b.brightness),
      changed: a.brightness !== b.brightness,
    },
    {
      key: "speed",
      label: c.light.speed,
      now: format.number(a.speed),
      backup: format.number(b.speed),
      changed: a.speed !== b.speed,
    },
  ];
  const columns: TableColumn<LightRow>[] = [
    { key: "setting", header: cols.setting, rowHeader: true, render: (row) => row.label },
    { key: "now", header: cols.now, render: (row) => row.now },
    {
      key: "backup",
      header: words.backupColumn,
      render: (row) => (row.changed ? <strong>{row.backup}</strong> : row.backup),
    },
  ];
  return (
    <Table
      columns={columns}
      rows={rows}
      rowKey={(row) => row.key}
      rowClass={(row) => (row.changed ? "br-changed" : undefined)}
    />
  );
}

/** The changes of one table, decoded where possible. */
function ChangeList({ changes }: { changes: readonly ChangeView[] }) {
  const dict = useDict();
  const format = useFormat();
  const c = dict.backupRestore.changes;
  const keymap = ofKind(changes, "keymap");
  const deadBand = ofKind(changes, "dead_band");
  const colors = ofKind(changes, "color");
  const raw = ofKind(changes, "raw");
  return (
    <div className="br-changes">
      {changes.map((change, index) => {
        switch (change.kind) {
          case "trigger":
            return (
              <FromTo key={index} from={triggerText(change.old, dict, format)} to={triggerText(change.new, dict, format)} />
            );
          case "light":
            return <LightChange key={index} old={change.old} next={change.new} />;
          case "macro":
            return <FromTo key={index} from={macroText(change.old, dict)} to={macroText(change.new, dict)} />;
          case "advanced_key":
            return <FromTo key={index} from={advancedText(change.old, dict)} to={advancedText(change.new, dict)} />;
          case "keymap":
          case "dead_band":
          case "color":
          case "raw":
            return null;
        }
      })}
      {keymap.length === 0 ? null : (
        <>
          <p>{c.keymapKeys({ n: keymap.length, keys: keyNamesText(keymap.map((k) => k.key.name), dict) })}</p>
          <TechnicalDetails>
            {keymap.map((k) => `${keyName(dict, k.key.name)}: ${k.old_hex} → ${k.new_hex}`).join("\n")}
          </TechnicalDetails>
        </>
      )}
      {deadBand.length === 0 ? null : (
        <ul className="br-keylist">
          {deadBand.slice(0, LIST_LIMIT).map((d) => (
            <li key={d.key.index}>
              <span className="ow-strong">
                <KeyName keyRef={d.key} />
              </span>
              <FromTo from={deadBandText(d.old, dict, format)} to={deadBandText(d.new, dict, format)} />
            </li>
          ))}
          {deadBand.length > LIST_LIMIT ? (
            <li className="ow-muted">{dict.common.andMore({ n: deadBand.length - LIST_LIMIT })}</li>
          ) : null}
        </ul>
      )}
      {colors.length === 0 ? null : (
        <>
          <p>{c.colors({ n: colors.length })}</p>
          <ul className="br-keylist">
            {colors.slice(0, LIST_LIMIT).map((k) => (
              <li key={k.key.index}>
                <span className="ow-strong">
                  <KeyName keyRef={k.key} />
                </span>
                <span className="br-fromto">
                  {k.old === null ? <span>{c.colorNone}</span> : <ColorSwatch color={k.old} />}
                  <span aria-hidden="true"> → </span>
                  {k.new === null ? <strong>{c.colorNone}</strong> : <ColorSwatch color={k.new} />}
                </span>
              </li>
            ))}
            {colors.length > LIST_LIMIT ? (
              <li className="ow-muted">{dict.common.andMore({ n: colors.length - LIST_LIMIT })}</li>
            ) : null}
          </ul>
        </>
      )}
      {raw.length === 0 ? null : (
        <>
          <p>{c.entries({ n: raw.length })}</p>
          <TechnicalDetails>{raw.map((r) => `#${r.index}: ${r.old_hex} → ${r.new_hex}`).join("\n")}</TechnicalDetails>
        </>
      )}
    </div>
  );
}

/** One table that will be written. */
function WriteRow({ item }: { item: WriteItem }) {
  const dict = useDict();
  const t = dict.backupRestore.review;
  const action = item.action;
  const commit = action.commit_trigger;
  return (
    <li className="br-item">
      <p className="br-item__title">
        {action.resent_unchanged && commit !== null ? (
          dict.flow.commitTrigger[commit]
        ) : (
          <ItemName table={item.table} keyRef={item.key} />
        )}
      </p>
      {action.resent_unchanged ? null : <ChangeList changes={action.changes} />}
      {action.more_changes > 0 ? <p className="ow-muted ow-small">{t.moreChanges({ n: action.more_changes })}</p> : null}
      {commit !== null && !action.resent_unchanged ? <p className="ow-muted ow-small">{t.commitNote}</p> : null}
      {action.resent === null ? null : <p className="ow-muted ow-small">{dict.flow.resent[action.resent]}</p>}
    </li>
  );
}

/** One table that differs but isn't restored, with the reason. */
function SkipRow({ item }: { item: SkipItem }) {
  const dict = useDict();
  const format = useFormat();
  const words = useReviewWords();
  const cause = item.reason.code === "keymap_group_blocked" ? blockCauseTechnical(item.reason.cause) : null;
  return (
    <li className="br-item">
      <p className="br-item__title">
        <ItemName table={item.table} keyRef={item.key} />
      </p>
      <p>{skipReasonText(item.reason, dict, format, words.variant)}</p>
      <TechnicalDetails>{cause === null ? item.message : `${item.message}\n${cause}`}</TechnicalDetails>
    </li>
  );
}

/** The plan's warnings as callouts (shared-table warnings in amber). */
export function PlanWarnings({ warnings }: { warnings: readonly PlanWarningView[] }) {
  const dict = useDict();
  const format = useFormat();
  if (warnings.length === 0) return null;
  return (
    <div className="br-warnings">
      {warnings.map((warning, index) => {
        const view = planWarningView(warning, dict, format);
        return (
          <Callout key={`${warning.code}-${index}`} kind={view.tone}>
            <p>{view.text}</p>
            {view.technical === null ? null : <TechnicalDetails>{view.technical}</TechnicalDetails>}
          </Callout>
        );
      })}
    </div>
  );
}

/** One profile's plan as an expandable section (open when it has changes). */
function ProfilePlan({ group }: { group: ProfileGroup }) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.backupRestore.review;
  const words = useReviewWords();
  const writes = group.triggers.length + group.writes.length;
  const unchanged = condenseTables(group.unchanged);
  const missing = condenseTables(group.notInBackup);
  return (
    <details className="br-acc" open={writes > 0}>
      <summary className="br-acc__summary">
        <Icon name="chevron" size={16} className="br-acc__chevron" />
        <span className="br-acc__title">{t.profileSummary({ n: group.profile, changes: group.changes })}</span>
        {group.skips.length > 0 ? (
          <span className="br-chip br-chip--warning">{words.profileSkipped({ n: group.skips.length })}</span>
        ) : null}
      </summary>
      <div className="br-acc__body">
        {group.compared ? null : <p className="ow-muted">{t.notCompared}</p>}
        {writes === 0 ? null : (
          <section className="br-section">
            <h3 className="br-section__title">{t.willWrite}</h3>
            <ul className="br-items">
              {group.triggers.length === 0 ? null : (
                <li className="br-item">
                  <p className="br-item__title">{t.triggers({ n: group.triggers.length })}</p>
                  <TriggerTable rows={group.triggers} />
                </li>
              )}
              {group.writes.map((item, index) => (
                <WriteRow key={index} item={item} />
              ))}
            </ul>
          </section>
        )}
        {group.skips.length === 0 ? null : (
          <section className="br-section">
            <h3 className="br-section__title">{words.notRestored}</h3>
            <ul className="br-items">
              {group.skips.map((item, index) => (
                <SkipRow key={index} item={item} />
              ))}
            </ul>
          </section>
        )}
        {unchanged.length === 0 ? null : (
          <Disclosure summary={t.alreadyMatches({ n: group.unchanged.length })}>
            <p className="ow-muted">{format.list(unchanged.map((g) => tableGroupName(g, dict)))}</p>
          </Disclosure>
        )}
        {missing.length === 0 ? null : (
          <Disclosure summary={words.notInBackup({ n: group.notInBackup.length })}>
            <p className="ow-muted">{format.list(missing.map((g) => tableGroupName(g, dict)))}</p>
          </Disclosure>
        )}
      </div>
    </details>
  );
}

/** When and with which firmware the backup was made and the keyboard read. */
function PlanFacts({ preview }: { preview: RestorePreview }) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.backupRestore.review;
  const words = useReviewWords();
  const plan = preview.plan;
  const unknown = dict.common.unknown;
  const items: KeyValueItem[] = [
    {
      key: "file",
      label: words.fileLabel,
      value: <span className="ow-mono ow-break ow-selectable">{preview.source.file.file_name}</span>,
    },
    {
      key: "made",
      label: words.madeLabel,
      value: t.madeWith({ date: format.dateTime(plan.backup_created_at), firmware: plan.backup_firmware ?? unknown }),
    },
    {
      key: "read",
      label: t.facts.keyboardRead,
      value: t.madeWith({ date: format.dateTime(plan.keyboard_read_at), firmware: plan.keyboard_firmware ?? unknown }),
    },
  ];
  if (plan.original_profile !== null) {
    items.push({ key: "active", label: t.facts.activeProfile, value: t.returnsTo({ n: plan.original_profile }) });
  }
  return <KeyValueList items={items} />;
}

/** Props of {@link RestoreReview} and {@link RestoreNothing}. */
export interface RestoreReviewProps {
  preview: RestorePreview;
  /** An import of a profile file uses import words (default: a restore). */
  variant?: ReviewVariant | undefined;
}

/** The restore review: what will change in each profile, and what won't. */
export function RestoreReview({ preview, variant = "restore" }: RestoreReviewProps) {
  return (
    <VariantContext value={variant}>
      <ReviewBody preview={preview} />
    </VariantContext>
  );
}

function ReviewBody({ preview }: { preview: RestorePreview }) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.backupRestore.review;
  const words = useReviewWords();
  const plan = preview.plan;
  const groups = groupPlan(plan);
  const scope = groups.map((group) => group.profile);
  const changed = profilesWithChanges(groups);
  const changes = planChanges(groups);
  const undo = preview.source.kind === "safety" && !words.importing;
  const technical = [
    t.packets({ writes: plan.totals.writes, packets: plan.totals.packets }),
    ...plan.totals.per_profile.map((p) => t.packetsProfile({ n: p.profile, writes: p.writes, packets: p.packets })),
  ].join("\n");
  return (
    <div className="br-review">
      {undo ? (
        <Callout kind="warning">
          <p>{t.undoNote({ list: numberList(scope, format), count: scope.length })}</p>
        </Callout>
      ) : null}
      <PlanFacts preview={preview} />
      {preview.switches.length > 0 ? (
        <p className="ow-muted">{t.switched({ list: numberList(preview.switches, format), count: preview.switches.length })}</p>
      ) : null}
      <PlanWarnings warnings={plan.warnings} />
      <div className="br-review__profiles">
        {groups.map((group) => (
          <ProfilePlan key={group.profile} group={group} />
        ))}
      </div>
      {changes > 0 ? (
        <p className="ow-strong">{t.totals({ n: changes, list: numberList(changed, format), count: changed.length })}</p>
      ) : null}
      <TechnicalDetails>{technical}</TechnicalDetails>
    </div>
  );
}

/** "Nothing to write", with the warnings and what can't be restored. */
export function RestoreNothing({ preview, variant = "restore" }: RestoreReviewProps) {
  return (
    <VariantContext value={variant}>
      <NothingBody preview={preview} />
    </VariantContext>
  );
}

function NothingBody({ preview }: { preview: RestorePreview }) {
  const dict = useDict();
  const words = useReviewWords();
  const groups = groupPlan(preview.plan).filter((group) => group.skips.length > 0);
  return (
    <div className="br-review">
      <Callout kind="info" title={words.nothing} />
      <PlanWarnings warnings={preview.plan.warnings} />
      {groups.length === 0 ? null : (
        <section className="br-section">
          <h3 className="br-section__title">{words.nothingSkips}</h3>
          {groups.map((group) => (
            <div key={group.profile} className="br-section">
              <p className="ow-strong">{dict.common.profile({ n: group.profile })}</p>
              <ul className="br-items">
                {group.skips.map((item, index) => (
                  <SkipRow key={index} item={item} />
                ))}
              </ul>
            </div>
          ))}
        </section>
      )}
    </div>
  );
}
