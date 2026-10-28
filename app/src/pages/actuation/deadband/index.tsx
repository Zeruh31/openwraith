// Actuation > Dead band: the W75 with each key's top or bottom dead zone,
// the same selection tools as the trigger tab, an editor for the selected
// keys (top and bottom, 0 to 1.00 mm in 0.02 mm steps on the W75), the
// travel gauge and soft warnings against the keys' actuation points. The
// dead band belongs to the active profile (confirmed on hardware), so the
// tab has no scope badge. Writing goes only through the shared WriteFlow:
// `prepare_dead_band_edit` (planned from the last read) → review → Write →
// `apply_dead_band_edit` → report with Undo; nothing is sent while a slider
// moves.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ApplyBar } from "../../../components/ApplyBar";
import { Button } from "../../../components/Button";
import { Callout } from "../../../components/Feedback";
import { Card, PageHeader } from "../../../components/Layout";
import type { KeyVisual } from "../../../components/keyboard";
import { EditReportView } from "../../../flow/Report";
import { WriteFlow, type OnProgress } from "../../../flow/WriteFlow";
import { keyName, useDict, type Dict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type {
  AppError,
  DeadBandDisabled,
  DeadBandEditPreview,
  DeadBandState,
  EditReportView as EditReport,
  PlanToken,
} from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { readAgain } from "../../../shell/actions";
import { NotReadEmptyState } from "../../../shell/NotReadEmptyState";
import { ReadAt } from "../../../shell/ReadAt";
import { isLocked, useOperationLock } from "../../../state/operation";
import { useSnapshot } from "../../../state/snapshot";
import { useStatus } from "../../../state/status";
import { ActuationKeyboard } from "../ActuationKeyboard";
import { KEY_NAMES, keysByIndex, legendOf, valueTexts } from "../model";
import { editErrorNote, editPrepareError, PersistenceOption } from "../shared/EditParts";
import { gateReason } from "../shared/editTexts";
import { usePart } from "../shared/partStore";
import { ownerOf, useTabDraft } from "../shared/tabDraft";
import { Toolbar } from "../Toolbar";
import { DeadbandInspector } from "./DeadbandInspector";
import { DeadbandReview } from "./DeadbandReview";
import {
  buildDeadBandIntent,
  deadBandKeys,
  deadBandUnit,
  isDeadBandDirty,
  localHints,
  sameDeadBand,
  sameDeadBandKeys,
  selectableOf,
  selectedDeadBands,
  summarize,
  targetOf,
  travelUnits,
  zonedKeys,
  type DeadBandDraft,
  type DeadBandShow,
} from "./model";
import { deadBandDrafts, deadBandPart } from "./store";
import "../actuation.css";

/** Every key of the dead band can be changed when the tab can (no per-key reasons). */
const NO_TOOLTIPS: ReadonlyMap<number, string> = new Map();

/** Why the dead band can't be changed, in words. */
export function deadBandDisabledText(disabled: DeadBandDisabled, dict: Dict): string {
  const t = dict.actuation.deadband.disabled;
  switch (disabled.reason) {
    case "not_read":
      return t.not_read;
    case "distance_unit":
      return t.distance_unit;
    case "unreadable":
      return t.unreadable;
    case "unsupported_model":
      return t.unsupported_model;
  }
}

/** The Dead band tab. */
export default function DeadbandTab() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const lock = useOperationLock();
  const { snapshot } = useSnapshot();
  const view = usePart(deadBandPart);
  const t = dict.actuation.deadband;
  const state = view.state;

  const [show, setShow] = useState<DeadBandShow>("top");
  const [flowOpen, setFlowShown] = useState(false);
  // The dead band the open flow was started on: the flow stays mounted (with
  // its progress or report) when the keyboard goes away meanwhile.
  const [flowState, setFlowState] = useState<DeadBandState | null>(null);
  const [checkPersistence, setCheckPersistence] = useState(false);
  const stored = useTabDraft(deadBandDrafts);
  const identityKey = ownerOf(snapshot);
  const draft = deadBandDrafts.effective(stored, identityKey);
  const dirty = isDeadBandDirty(draft);
  const selection = stored.selection;

  const setDraft = (next: DeadBandDraft) => {
    deadBandDrafts.setDraft(identityKey, next);
  };

  // A flow that went away with the tab counts as closed.
  useEffect(
    () => () => {
      deadBandDrafts.setFlowOpen(false);
    },
    [],
  );
  useEffect(() => {
    deadBandDrafts.setFlowOpen(flowOpen);
  }, [flowOpen]);

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

  const keys = useMemo(() => deadBandKeys(state), [state]);
  const triggers = useMemo(() => keysByIndex(snapshot), [snapshot]);
  const unitUm = deadBandUnit(state);
  const max = state?.max ?? 0;
  const travel = travelUnits(unitUm);
  const texts = useMemo(() => valueTexts(dict, format, unitUm), [dict, format, unitUm]);
  const selectable = useMemo(() => selectableOf(keys), [keys]);
  const zoned = useMemo(() => zonedKeys(keys), [keys]);
  const summary = useMemo(() => summarize(selection, keys, draft), [selection, keys, draft]);
  const values = useMemo(() => selectedDeadBands(selection, keys), [selection, keys]);
  const hints = useMemo(() => localHints(summary, keys, triggers, draft, travel), [summary, keys, triggers, draft, travel]);
  const editable = state?.editable === true;
  const canEdit = editable && !view.stale;

  const keyVisual = useCallback(
    (index: number): KeyVisual => {
      const entry = keys.get(index);
      const libraryName = entry?.key.name ?? KEY_NAMES.get(index) ?? String(index);
      const name = keyName(dict, libraryName);
      const legend = legendOf(dict, libraryName);
      if (entry === undefined) return { legend, name, selectable: false };
      const current = entry.value;
      const target = dirty && selection.has(index) ? targetOf(current, draft) : null;
      const pending = target !== null && !sameDeadBand(target, current);
      const shown = pending ? target : current;
      const parts = [t.keyInfo.now({ top: texts.mm(current.top), bottom: texts.mm(current.bottom) })];
      if (pending) parts.push(t.keyInfo.draft({ top: texts.mm(target.top), bottom: texts.mm(target.bottom) }));
      const value = show === "top" ? shown.top : shown.bottom;
      return {
        legend,
        name,
        value: value > 0 || pending ? texts.number(value) : null,
        marker: pending ? "pending" : null,
        description: parts.join(", "),
        emphasis: pending,
      };
    },
    [keys, dict, t, dirty, selection, draft, texts, show],
  );

  // Why nothing can be written now, and why "Review and write…" can't be used.
  let blockReason: string | null = null;
  if (state === null) {
    blockReason = dict.errors.writeBlocked.not_connected;
  } else if (!state.editable) {
    blockReason = state.disabled === null ? t.disabled.unsupported_model : deadBandDisabledText(state.disabled, dict);
  } else if (view.stale) {
    blockReason = dict.errors.writeBlocked.snapshot_stale;
  } else {
    blockReason = gateReason(status, dict);
  }
  if (blockReason === null) {
    if (selection.size === 0) blockReason = dict.actuation.apply.reasons.selectKeys;
    else if (summary.changing.length === 0) blockReason = t.apply.nothing;
  }

  // The latest state, summary and draft when the flow prepares (again on "Review again" and the persistence option).
  const latest = useRef({ state, summary, draft, checkPersistence });
  useEffect(() => {
    latest.current = { state, summary, draft, checkPersistence };
  });

  const onApply = () => {
    if (flowOpen || isLocked() || blockReason !== null) return;
    // Set at once: the flow prepares in its own mount effect, before this
    // component's effects run again (the option is never carried over).
    latest.current = { state, summary, draft, checkPersistence: false };
    setCheckPersistence(false);
    setFlowState(state);
    setFlowShown(true);
  };

  const prepare = async (): Promise<DeadBandEditPreview> => {
    const now = latest.current;
    if (now.state === null) {
      const error: AppError = { kind: "not_connected", detail: null, message: "no dead band to plan from" };
      throw error;
    }
    const intent = buildDeadBandIntent(now.summary, now.draft, now.state.revision, now.checkPersistence);
    if (intent === null) {
      const error: AppError = {
        kind: "invalid_request",
        detail: { detail: "edit", problem: { problem: "no_changes" } },
        message: "nothing to prepare: no dead band change",
      };
      throw error;
    }
    return api.prepareDeadBandEdit(intent).catch((error: unknown) => {
      // A plan made from an older read: show the current values for the next try.
      const appError = toAppError(error);
      if (appError.kind === "stale_plan") deadBandPart.refetch();
      throw appError;
    });
  };

  // A write makes the draft spent when it completed; the tab shows the values read back.
  const apply = (token: PlanToken, onProgress: OnProgress) =>
    api.applyDeadBandEdit(token, onProgress).then((report) => {
      if (report.headline === "done") deadBandDrafts.discard();
      deadBandPart.refetch();
      return report;
    });

  const closeFlow = () => {
    setFlowShown(false);
  };

  const parts = [dict.actuation.apply.willChange({ n: summary.changing.length })];
  if (summary.unchanged.length > 0) parts.push(dict.actuation.apply.already({ n: summary.unchanged.length }));

  return (
    <div className="act-page">
      <PageHeader title={dict.shell.nav.actuation} description={t.description} />
      {state === null ? (
        <Card>
          <NotReadEmptyState
            icon="keyboard"
            title={t.empty.title}
            body={t.empty.body}
            failed={t.empty.failed}
            loading={view.loading}
            error={view.error !== null}
          />
        </Card>
      ) : (
        <>
          {state.disabled === null ? null : (
            <Callout kind="warning" role="status">
              {deadBandDisabledText(state.disabled, dict)}
            </Callout>
          )}
          {view.stale ? (
            <Callout
              kind="info"
              actions={
                <Button icon="refresh" size="sm" disabled={lock.busy} onClick={readAgain}>
                  {dict.common.actions.readAgain}
                </Button>
              }
            >
              {dict.actuation.stale}
            </Callout>
          ) : null}
          <Card
            title={dict.actuation.board.title({ n: state.profile })}
            className="act-board-card"
            actions={<ReadAt time={state.read_at} />}
          >
            <Toolbar<DeadBandShow>
              selection={selection}
              onSelect={deadBandDrafts.setSelection}
              selectable={selectable}
              changed={zoned}
              same={(current) => sameDeadBandKeys(current, keys, selectable)}
              changedHint={t.changedHint}
              show={{
                value: show,
                onChange: setShow,
                options: [
                  { value: "top", label: t.show.top },
                  { value: "bottom", label: t.show.bottom },
                ],
              }}
            />
            <ActuationKeyboard
              label={dict.actuation.board.label}
              keyVisual={keyVisual}
              selected={selection}
              onSelectionChange={deadBandDrafts.setSelection}
              knobLabel={t.knob}
              changed={zoned}
              tooltips={NO_TOOLTIPS}
            />
            <ul className="act-legend" aria-label={dict.actuation.legend.label}>
              <li>
                <span className="act-legend__mark act-legend__mark--check" aria-hidden="true">
                  ✓
                </span>
                {dict.actuation.legend.selected}
              </li>
              <li>
                <span className="act-legend__mark act-legend__mark--pending" aria-hidden="true">
                  ●
                </span>
                {dict.actuation.legend.pending}
              </li>
              <li>
                <span className="act-legend__changed" aria-hidden="true" />
                {t.legend.zoned}
              </li>
              <li>{show === "top" ? t.legend.showTop : t.legend.showBottom}</li>
            </ul>
            <p className="ow-muted ow-small">{dict.actuation.select.hint}</p>
          </Card>
          <div ref={inspectorRef} tabIndex={-1} className="act-inspector-anchor">
            <DeadbandInspector
              selected={summary.keys}
              keys={keys}
              values={values}
              draft={draft}
              onDraft={setDraft}
              max={max}
              unitUm={unitUm}
              triggers={triggers}
              hints={hints}
              canEdit={canEdit}
            />
          </div>
          <ApplyBar
            summary={
              <div className="act-apply-summary">
                {dirty ? (
                  <p className="ow-num">
                    <span className="ow-strong">{dict.actuation.apply.notWritten}</span>
                    {parts.map((part) => (
                      <span key={part}> · {part}</span>
                    ))}
                  </p>
                ) : (
                  <p className="ow-muted">{t.apply.idle}</p>
                )}
                {selection.size > 0 && canEdit ? (
                  <Button size="sm" variant="ghost" onClick={focusInspector}>
                    {dict.actuation.apply.setValues}
                  </Button>
                ) : null}
              </div>
            }
            dirty={dirty}
            onDiscard={deadBandDrafts.discard}
            onApply={onApply}
            disabledReason={blockReason}
          />
        </>
      )}
      {flowOpen ? (
        <WriteFlow<DeadBandEditPreview, EditReport>
          kind="dead_band_edit"
          prepare={prepare}
          token={(preview) => preview.token}
          title={(preview) =>
            preview.nothing_to_do ? t.review.titleNothing : t.review.title({ n: preview.changes.length, profile: preview.profile })
          }
          review={(preview) => <DeadbandReview preview={preview} unitUm={deadBandUnit(state ?? flowState)} />}
          nothingToDo={() => <Callout kind="info" title={t.review.nothing} />}
          confirmLabel={() => dict.common.actions.writeToKeyboard}
          writesSettings
          prepareError={editPrepareError(deadBandUnit(state ?? flowState))}
          errorNote={editErrorNote(dict, format, deadBandUnit(state ?? flowState))}
          reviewOptions={(preview, reprepare) => (
            <PersistenceOption
              persistence={preview.persistence}
              onChange={(checked) => {
                latest.current = { ...latest.current, checkPersistence: checked };
                setCheckPersistence(checked);
                reprepare();
              }}
            />
          )}
          apply={apply}
          result={(report) => <EditReportView report={report} />}
          onClose={closeFlow}
        />
      ) : null}
    </div>
  );
}
