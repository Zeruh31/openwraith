// The Actuation & Rapid Trigger page: the W75 with each key's actuation
// point or Rapid Trigger sensitivity, selection tools, an inspector that
// edits a local draft, and the sticky apply bar. Writing goes only through
// the shared WriteFlow: `prepare_trigger_edit` (read-only plan from a fresh
// read) → review → explicit Write → `apply_trigger_edit` (one packet per
// key, the keyboard's save wait) → report with the safety backup and Undo.
// Nothing is sent while a slider is dragged or a value typed.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ApplyBar } from "../../components/ApplyBar";
import { Button, IconButton } from "../../components/Button";
import { Callout, EmptyState, Spinner } from "../../components/Feedback";
import { Card, PageHeader } from "../../components/Layout";
import type { KeyVisual } from "../../components/keyboard";
import { notEditableText } from "../../flow/labels";
import { EditReportView } from "../../flow/Report";
import { WriteFlow } from "../../flow/WriteFlow";
import { keyName, useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { AppError, EditReportView as EditReport, TriggerEditPreview } from "../../ipc/bindings";
import { api } from "../../ipc/commands";
import { previewUi } from "../../preview/ui";
import { readAgain } from "../../shell/actions";
import { useDraft } from "../../state/drafts";
import { isLocked, useOperationLock } from "../../state/operation";
import { useSnapshot } from "../../state/snapshot";
import { useStatus } from "../../state/status";
import { toast } from "../../state/toasts";
import { ActuationKeyboard } from "./ActuationKeyboard";
import { Inspector } from "./Inspector";
import {
  basicActuation,
  buildIntent,
  changedKeys,
  clearDraft,
  differsFromBasic,
  isDirty,
  isEditable,
  isRtOn,
  KEY_NAMES,
  keysByIndex,
  legendOf,
  limitsOf,
  sameValue,
  selectableOf,
  selectedValues,
  summarize,
  targetOf,
  unitOf,
  valueTexts,
  type ShowMode,
  type TriggerDraft,
} from "./model";
import { seedFrom, type ActuationPreviewUi } from "./preview";
import { Toolbar } from "./Toolbar";
import { NothingToChange, TriggerReview } from "./TriggerReview";
import "./actuation.css";

/** The draft and the connection and profile it was made for. */
interface OwnedDraft {
  /** `generation/profile` of the snapshot it was made on; `null` adopts the first one. */
  owner: string | null;
  draft: TriggerDraft;
}

interface Identity {
  key: string;
  profile: number;
}

/** The Actuation & Rapid Trigger page. */
export default function ActuationPage() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const { snapshot, stale, loading } = useSnapshot();
  const lock = useOperationLock();
  const t = dict.actuation;

  const [seed] = useState(() => seedFrom(previewUi<ActuationPreviewUi>("actuation")));
  const [selection, setSelection] = useState<ReadonlySet<number>>(seed.selection);
  const [show, setShow] = useState<ShowMode>(seed.show);
  const identity: Identity | null =
    snapshot === null ? null : { key: `${snapshot.generation}/${snapshot.profile}`, profile: snapshot.profile };
  const identityKey = identity?.key ?? null;

  const [owned, setOwned] = useState<OwnedDraft>(() => ({ owner: identityKey, draft: seed.draft }));
  const [flowOpen, setFlowOpen] = useState(false);
  const [autoOpen, setAutoOpen] = useState(seed.open);

  // A draft belongs to one connection and profile: on another profile or
  // after a reconnect it no longer applies (the toast below says so).
  const draft = owned.owner === null || owned.owner === identityKey ? owned.draft : clearDraft(owned.draft);
  const dirty = isDirty(draft);

  const setDraft = (next: TriggerDraft) => {
    setOwned({ owner: identityKey, draft: next });
  };
  const discard = useCallback(() => {
    setOwned((current) => ({ ...current, draft: clearDraft(current.draft) }));
  }, []);
  useDraft("actuation", dirty, discard);

  const lastIdentity = useRef<Identity | null>(identity);
  useEffect(() => {
    if (identity === null) return;
    const previous = lastIdentity.current;
    lastIdentity.current = identity;
    if (previous === null || previous.key === identity.key) return;
    // The draft is dropped for good: going back to the first profile later
    // must not bring back a draft the user was told was discarded.
    const discarded = (owned.owner === previous.key || owned.owner === null) && isDirty(owned.draft);
    setOwned((current) =>
      current.owner === previous.key || current.owner === null
        ? { owner: identity.key, draft: clearDraft(current.draft) }
        : current,
    );
    // While a write flow is open, the change comes from that flow's own
    // operation (or its report explains it): no "discarded" toast then.
    if (discarded && !flowOpen) {
      toast({
        kind: "info",
        title:
          previous.profile === identity.profile
            ? dict.actuation.draftDiscardedReconnect
            : dict.actuation.draftDiscarded({ n: identity.profile }),
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- runs when the connection or profile changes
  }, [identityKey]);

  // "Set values": the inspector is below the keyboard, out of view on most
  // windows; the apply bar (always in view) jumps to it.
  const inspectorRef = useRef<HTMLDivElement>(null);
  const focusInspector = () => {
    const inspector = inspectorRef.current;
    if (inspector === null) return;
    if (typeof inspector.scrollIntoView === "function") inspector.scrollIntoView({ block: "start" });
    const first = inspector.querySelector<HTMLElement>(
      "input:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex='-1'])",
    );
    (first ?? inspector).focus();
  };

  const keys = useMemo(() => keysByIndex(snapshot), [snapshot]);
  const limits = limitsOf(snapshot);
  const unitUm = unitOf(snapshot);
  const basic = basicActuation(limits);
  const texts = useMemo(() => valueTexts(dict, format, unitUm), [dict, format, unitUm]);
  const selectable = useMemo(() => selectableOf(keys), [keys]);
  const changed = useMemo(() => changedKeys(keys, basic), [keys, basic]);
  const summary = useMemo(() => summarize(selection, keys, draft), [selection, keys, draft]);
  const values = useMemo(() => selectedValues(selection, keys), [selection, keys]);
  const selected = useMemo(() => [...selection].sort((a, b) => a - b), [selection]);
  const editable = snapshot?.triggers.editable === true;
  const canEdit = editable && !stale;

  const tooltips = useMemo(() => {
    const map = new Map<number, string>();
    for (const [index, key] of keys) {
      if (key.not_editable !== null && key.selectable) map.set(index, notEditableText(key.not_editable, dict));
    }
    return map;
  }, [keys, dict]);

  const keyVisual = useCallback(
    (index: number): KeyVisual => {
      const view = keys.get(index);
      const libraryName = view?.key.name ?? KEY_NAMES.get(index) ?? String(index);
      const name = keyName(dict, libraryName);
      const legend = legendOf(dict, libraryName);
      if (view?.selectable !== true) {
        return { legend, name, selectable: false };
      }
      const current = view.value;
      if (current === null) {
        return {
          legend,
          name,
          value: t.values.unknown,
          marker: "warning",
          description: [t.keyInfo.unknown, tooltips.get(index)].filter(Boolean).join(", "),
        };
      }
      const target = dirty && isEditable(view) && selection.has(index) ? targetOf(current, draft) : null;
      const pending = target !== null && !sameValue(target, current);
      const shown = pending ? target : current;
      const parts = [texts.spoken(current)];
      if (pending) parts.push(t.keyInfo.draft({ next: texts.setting(target) }));
      if (differsFromBasic(current, basic)) parts.push(t.keyInfo.changed);
      const reason = tooltips.get(index);
      if (reason !== undefined) parts.push(reason);
      return {
        legend,
        name,
        value: texts.cap(shown, show),
        chip: isRtOn(shown) ? t.chip : null,
        marker: view.not_editable !== null ? "warning" : pending ? "pending" : null,
        description: parts.join(", "),
        emphasis: pending,
      };
    },
    [keys, dict, t, tooltips, dirty, selection, draft, texts, basic, show],
  );

  // Why "Review and write…" can't be used now (shown under the button).
  let blockReason: string | null = null;
  if (snapshot === null) {
    blockReason = dict.errors.writeBlocked.not_connected;
  } else if (!snapshot.triggers.editable) {
    blockReason = t.disabled[snapshot.triggers.disabled ?? "limits_unknown"];
  } else if (stale) {
    blockReason = dict.errors.writeBlocked.snapshot_stale;
  } else if (!status.write_gate.allowed) {
    const reason = status.write_gate.reasons[0];
    blockReason = dict.errors.writeBlocked[reason ?? "not_connected"];
  } else if (lock.busy) {
    blockReason = dict.errors.writeBlocked.busy;
  } else if (selection.size === 0) {
    blockReason = t.apply.reasons.selectKeys;
  } else if (summary.keys.length === 0) {
    blockReason = t.apply.reasons.noEditable;
  } else if (summary.changing.length === 0) {
    blockReason = t.apply.reasons.nothing;
  }

  const onApply = () => {
    if (flowOpen || isLocked() || blockReason !== null) return;
    setFlowOpen(true);
  };

  // The intent is built when the flow prepares (and again on "Review again"),
  // from the state the user sees then, with that snapshot's revision.
  const prepare = async (): Promise<TriggerEditPreview> => {
    if (snapshot === null) {
      const error: AppError = { kind: "not_connected", detail: null, message: "no keyboard values to plan from" };
      throw error;
    }
    const intent = buildIntent(summary, draft, snapshot.revision);
    if (intent === null) {
      // E.g. "Review again" after the profile changed and the draft was dropped.
      const error: AppError = {
        kind: "invalid_request",
        detail: { detail: "invalid_intent", field: summary.keys.length === 0 ? "keys" : "nothing_to_change" },
        message: "nothing to prepare: no change or no keys that can be changed",
      };
      throw error;
    }
    return api.prepareTriggerEdit(intent);
  };

  const closeFlow = (result: EditReport | null) => {
    setFlowOpen(false);
    setAutoOpen(false);
    if (result?.headline === "done") discard();
  };

  const showFlow = flowOpen || (autoOpen && snapshot !== null);
  const leftOut = summary.leftOut.length;
  const parts = [t.apply.willChange({ n: summary.changing.length })];
  if (summary.unchanged.length > 0) parts.push(t.apply.already({ n: summary.unchanged.length }));
  if (leftOut > 0) parts.push(t.apply.leftOut({ n: leftOut }));

  return (
    <div className="act-page">
      <PageHeader title={t.title} description={t.description} />
      {snapshot === null ? (
        <Card>
          <EmptyState icon="keyboard" title={t.empty.title}>
            {loading || status.connection.state === "reading" ? (
              <p className="ow-row">
                <Spinner />
                <span>{t.empty.reading}</span>
              </p>
            ) : (
              <p>{t.empty.body}</p>
            )}
          </EmptyState>
        </Card>
      ) : (
        <>
          {snapshot.triggers.disabled === null ? null : (
            <Callout kind="warning" role="status">
              {t.disabled[snapshot.triggers.disabled]}
            </Callout>
          )}
          {stale ? (
            <Callout
              kind="info"
              actions={
                <Button icon="refresh" size="sm" disabled={lock.busy} onClick={readAgain}>
                  {dict.common.actions.readAgain}
                </Button>
              }
            >
              {t.stale}
            </Callout>
          ) : null}
          <Card
            title={t.board.title({ n: snapshot.profile })}
            className="act-board-card"
            actions={
              <span className="act-read-at">
                <span className="ow-muted ow-small ow-num">{t.board.readAt({ time: format.time(snapshot.read_at) })}</span>
                <IconButton
                  icon="refresh"
                  size="sm"
                  label={t.board.readAgain}
                  disabled={lock.busy || status.connection.state !== "ready"}
                  onClick={readAgain}
                />
              </span>
            }
          >
            <Toolbar
              selection={selection}
              onSelect={setSelection}
              keys={keys}
              selectable={selectable}
              changed={changed}
              show={show}
              onShow={setShow}
            />
            <ActuationKeyboard
              label={t.board.label}
              keyVisual={keyVisual}
              selected={selection}
              onSelectionChange={setSelection}
              knobLabel={t.board.knob}
              changed={changed}
              tooltips={tooltips}
            />
            <ul className="act-legend" aria-label={t.legend.label}>
              <li>
                <span className="act-legend__mark act-legend__mark--check" aria-hidden="true">
                  ✓
                </span>
                {t.legend.selected}
              </li>
              <li>
                <span className="act-legend__mark act-legend__mark--pending" aria-hidden="true">
                  ●
                </span>
                {t.legend.pending}
              </li>
              <li>
                <span className="act-legend__mark act-legend__mark--warning" aria-hidden="true">
                  ⚠
                </span>
                {t.legend.warning}
              </li>
              <li>
                <span className="act-legend__chip" aria-hidden="true">
                  {t.chip}
                </span>
                {t.legend.rt}
              </li>
              {show === "rapid_trigger" ? <li>{t.legend.pressRelease}</li> : null}
              <li>
                <span className="act-legend__changed" aria-hidden="true" />
                {t.legend.changed({ value: texts.mm(basic) })}
              </li>
            </ul>
            <p className="ow-muted ow-small">{t.select.hint}</p>
          </Card>
          <div ref={inspectorRef} tabIndex={-1} className="act-inspector-anchor">
            <Inspector
              selected={selected}
              leftOut={summary.leftOut}
              keys={keys}
              values={values}
              draft={draft}
              onDraft={setDraft}
              limits={limits}
              unitUm={unitUm}
              texts={texts}
              canEdit={canEdit}
            />
          </div>
          <ApplyBar
            summary={
              <div className="act-apply-summary">
                {dirty ? (
                  <p className="ow-num">
                    <span className="ow-strong">{t.apply.notWritten}</span>
                    {parts.map((part) => (
                      <span key={part}> · {part}</span>
                    ))}
                  </p>
                ) : (
                  <p className="ow-muted">{t.apply.idle}</p>
                )}
                {selection.size > 0 && canEdit ? (
                  <Button size="sm" variant="ghost" onClick={focusInspector}>
                    {t.apply.setValues}
                  </Button>
                ) : null}
              </div>
            }
            dirty={dirty}
            onDiscard={discard}
            onApply={onApply}
            disabledReason={blockReason}
          />
        </>
      )}
      {showFlow ? (
        <WriteFlow<TriggerEditPreview, EditReport>
          kind="trigger_edit"
          prepare={prepare}
          token={(preview) => preview.token}
          title={(preview) =>
            preview.nothing_to_do
              ? t.review.titleNothing
              : t.review.title({ n: preview.changing, profile: preview.profile })
          }
          review={(preview) => <TriggerReview preview={preview} leftOut={leftOut} unitUm={unitUm} />}
          nothingToDo={() => <NothingToChange />}
          confirmLabel={() => dict.common.actions.writeToKeyboard}
          writesSettings
          apply={(token, onProgress) => api.applyTriggerEdit(token, onProgress)}
          result={(report) => <EditReportView report={report} />}
          onClose={closeFlow}
        />
      ) : null}
    </div>
  );
}
