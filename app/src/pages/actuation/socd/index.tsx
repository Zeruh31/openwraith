// Actuation > SOCD: the power switch (one setting of the whole keyboard, a
// confirmed fact: "Applies to all profiles"; Fn + Right Shift toggles it
// too) and the pairs of the active profile (at most 20, each two different
// keys with a mode; a key in one pair only). Modes 2 and 3 carry "Not yet
// verified" from the library's evidence. When the keyboard's own toggle
// pushes the new power, the tab shows it (the shell toasts it and the
// backend invalidates a pending SOCD plan). Writing goes only through the
// shared WriteFlow: `prepare_socd_edit` → review → Write → `apply_socd_edit`
// → report with Undo.

import { useEffect, useMemo, useRef, useState } from "react";

import { ApplyBar } from "../../../components/ApplyBar";
import { Badge, EvidenceBadge } from "../../../components/Badge";
import { Button } from "../../../components/Button";
import { ConfirmCheckbox } from "../../../components/ConfirmCheckbox";
import { Switch } from "../../../components/Controls";
import { Callout } from "../../../components/Feedback";
import type { KeyOption } from "../../../components/KeyPicker";
import { Card, Disclosure, PageHeader, TechnicalDetails } from "../../../components/Layout";
import { EditReportView } from "../../../flow/Report";
import { WriteFlow, type OnProgress, type PrepareErrorView } from "../../../flow/WriteFlow";
import { keyName, useDict, type Dict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type {
  AppError,
  EditReportView as EditReport,
  PlanToken,
  SocdDisabled,
  SocdEditPreview,
  SocdStateView,
} from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { readAgain } from "../../../shell/actions";
import { NotReadEmptyState } from "../../../shell/NotReadEmptyState";
import { ReadAt } from "../../../shell/ReadAt";
import { isLocked, useOperationLock } from "../../../state/operation";
import { useSnapshot } from "../../../state/snapshot";
import { useNotices, useStatus } from "../../../state/status";
import { DEFAULT_UNIT_UM, legendOf } from "../model";
import { EditRefusal, editErrorNote, PersistenceOption } from "../shared/EditParts";
import { editProblemOf, gateReason } from "../shared/editTexts";
import { usePart } from "../shared/partStore";
import { ownerOf, useTabDraft } from "../shared/tabDraft";
import { modeHint, modeName } from "./labels";
import {
  buildSocdIntent,
  canQuickAdd,
  checkPairs,
  DEFAULT_MODE,
  isSocdDirty,
  modeEvidence,
  nextId,
  normalize,
  offeredKeys,
  QUICK_PAIRS,
  rowsOf,
  shownPower,
  SOCD_MODES,
  usedKeys,
  type QuickPair,
  type SocdDraft,
  type SocdPairDraft,
} from "./model";
import { PairRow } from "./PairRow";
import { SocdReview } from "./SocdReview";
import { socdDrafts, socdPart } from "./store";
import "../actuation.css";
import { useReadBlockedReason } from "../../../state/operation";

/** Why SOCD can't be changed, in words. */
export function socdDisabledText(disabled: SocdDisabled, dict: Dict): string {
  const t = dict.actuation.socd.disabled;
  switch (disabled.reason) {
    case "not_read":
      return t.not_read;
    case "unreadable":
      return t.unreadable;
    case "unsupported_model":
      return t.unsupported_model;
  }
}

const QUICK: readonly QuickPair[] = ["ad", "ws", "arrows"];

/** The SOCD tab. */
export default function SocdTab() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const lock = useOperationLock();
  const readBlocked = useReadBlockedReason();
  const { snapshot } = useSnapshot();
  const view = usePart(socdPart);
  const t = dict.actuation.socd;
  const state = view.state;

  const [flowOpen, setFlowShown] = useState(false);
  // The SOCD values the open flow was started on: the flow stays mounted
  // (with its progress or report) when the keyboard goes away meanwhile.
  const [flowState, setFlowState] = useState<SocdStateView | null>(null);
  const [checkPersistence, setCheckPersistence] = useState(false);
  const stored = useTabDraft(socdDrafts);
  const identityKey = ownerOf(snapshot);
  // The power Fn + Right Shift set on the keyboard, for this profile's read.
  const [toggled, setToggled] = useState<{ power: boolean; owner: string | null } | null>(null);
  const draft = socdDrafts.effective(stored, identityKey);

  const setDraft = (next: SocdDraft) => {
    socdDrafts.setDraft(identityKey, state === null ? next : normalize(next, state));
  };

  useEffect(
    () => () => {
      socdDrafts.setFlowOpen(false);
    },
    [],
  );
  useEffect(() => {
    socdDrafts.setFlowOpen(flowOpen);
  }, [flowOpen]);

  // Fn + Right Shift on the keyboard: show the new power (the shell toasts it).
  useNotices((notice) => {
    if (notice.notice !== "socd_toggled_on_keyboard") return;
    setToggled({ power: notice.power, owner: identityKey });
    socdPart.refetch();
  });

  // A power change in the draft that the keyboard now has anyway is no change.
  const keyboardPower = state?.power;
  useEffect(() => {
    if (state === null) return;
    const current = socdDrafts.effective(socdDrafts.store.get(), identityKey);
    const next = normalize(current, state);
    if (next.power !== current.power || next.pairs !== current.pairs) socdDrafts.setDraft(identityKey, next);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only the keyboard's values matter
  }, [keyboardPower, state?.revision]);

  const options = useMemo<KeyOption[]>(() => {
    if (state === null) return [];
    const list: KeyOption[] = [];
    for (const entry of state.keys) {
      if (entry.key === null) continue;
      list.push({ index: entry.key.index, name: keyName(dict, entry.key.name), legend: legendOf(dict, entry.key.name) });
    }
    return list;
  }, [state, dict]);
  const offered = useMemo(() => (state === null ? new Set<number>() : offeredKeys(state)), [state]);

  // The note stays only while it is still true: same profile and read, and the switch shows that power.
  const toggledNote = toggled !== null && toggled.owner === identityKey && state?.power === toggled.power ? toggled.power : null;

  const rows = state === null ? [] : rowsOf(draft, state);
  const max = state?.max_pairs ?? 20;
  const check = checkPairs(rows, max);
  const dirty = isSocdDirty(draft);
  const editable = state?.editable === true;
  const canEdit = editable && !view.stale;
  const replacing = state !== null && !state.pairs_writable;
  // (The review keeps the evidence of the values it was opened on when the keyboard goes away.)
  const evidenceOf = state ?? flowState;
  const evidence = (mode: number) => (evidenceOf === null ? null : modeEvidence(evidenceOf, mode));

  const setRows = (next: readonly SocdPairDraft[]) => {
    setDraft({ ...draft, pairs: next });
  };
  const addRow = (key1: number | null, key2: number | null) => {
    setRows([...rows, { id: nextId(rows), key1, key2, mode: DEFAULT_MODE }]);
  };

  // Why "Review and write…" can't be used now.
  let blockReason: string | null = null;
  if (state === null) {
    blockReason = dict.errors.writeBlocked.not_connected;
  } else if (!state.editable) {
    blockReason = state.disabled === null ? t.disabled.unsupported_model : socdDisabledText(state.disabled, dict);
  } else if (view.stale) {
    blockReason = dict.errors.writeBlocked.snapshot_stale;
  } else {
    blockReason = gateReason(status, dict);
  }
  if (blockReason === null) {
    if (!dirty) blockReason = t.apply.nothing;
    else if (draft.pairs !== null && !check.ok) blockReason = check.tooMany ? t.apply.tooMany({ max }) : t.apply.fixPairs;
    else if (replacing && draft.pairs !== null && !draft.allowNotUndoable) blockReason = t.apply.confirmReplace;
    // The keyboard can't take the power alone while it keeps this profile's
    // pairs in another form (the write re-sends the pairs).
    else if (replacing && draft.pairs === null && draft.power !== null) blockReason = t.apply.powerNeedsPairs;
  }

  // The latest state and draft when the flow prepares (again on "Review again" and the persistence option).
  const latest = useRef({ state, draft, checkPersistence });
  useEffect(() => {
    latest.current = { state, draft, checkPersistence };
  });

  const onApply = () => {
    if (flowOpen || isLocked() || blockReason !== null) return;
    // Set at once: the flow prepares in its own mount effect, before this
    // component's effects run again (the option is never carried over).
    latest.current = { state, draft, checkPersistence: false };
    setCheckPersistence(false);
    setToggled(null);
    setFlowState(state);
    setFlowShown(true);
  };

  const prepare = async (): Promise<SocdEditPreview> => {
    const now = latest.current;
    if (now.state === null) {
      const error: AppError = { kind: "not_connected", detail: null, message: "no SOCD values to plan from" };
      throw error;
    }
    const intent = buildSocdIntent(now.state, now.draft, now.checkPersistence);
    if (intent === null) {
      const error: AppError = {
        kind: "invalid_request",
        detail: { detail: "edit", problem: { problem: "no_changes" } },
        message: "nothing to prepare: no SOCD change",
      };
      throw error;
    }
    return api.prepareSocdEdit(intent).catch((error: unknown) => {
      const appError = toAppError(error);
      if (appError.kind === "stale_plan") socdPart.refetch();
      throw appError;
    });
  };

  const apply = (token: PlanToken, onProgress: OnProgress) =>
    api.applySocdEdit(token, onProgress).then((report) => {
      if (report.headline === "done") socdDrafts.discard();
      socdPart.refetch();
      return report;
    });

  // Pairs the keyboard has in another form: replacing them asks for an explicit confirmation.
  const prepareError = (error: AppError): PrepareErrorView | null => {
    const problem = editProblemOf(error);
    if (problem === null) return null;
    if (problem.problem === "unsupported_current_table" && latest.current.draft.pairs === null) {
      // Only the power changes, and the keyboard keeps this profile's pairs in another form.
      return {
        body: (
          <EditRefusal error={error} unitUm={DEFAULT_UNIT_UM}>
            <p>{t.apply.powerNeedsPairs}</p>
          </EditRefusal>
        ),
        retry: { label: t.replace.reviewAgain, disabledReason: t.apply.powerNeedsPairs },
      };
    }
    if (problem.problem === "unsupported_current_table") {
      return {
        body: (
          <EditRefusal error={error} unitUm={DEFAULT_UNIT_UM}>
            <ConfirmCheckbox
              checked={draft.allowNotUndoable}
              tone="warning"
              label={t.replace.confirm}
              onChange={(checked) => {
                setDraft({ ...draft, allowNotUndoable: checked });
              }}
            />
          </EditRefusal>
        ),
        retry: {
          label: t.replace.reviewAgain,
          disabledReason: draft.allowNotUndoable ? null : t.replace.needed,
        },
      };
    }
    return { body: <EditRefusal error={error} unitUm={DEFAULT_UNIT_UM} /> };
  };

  const power = state === null ? false : shownPower(draft, state);
  const summaryParts: string[] = [];
  if (state !== null && draft.power !== null) summaryParts.push(draft.power ? t.apply.powerOn : t.apply.powerOff);
  if (draft.pairs !== null) summaryParts.push(t.apply.pairs({ n: draft.pairs.length }));

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
              {socdDisabledText(state.disabled, dict)}
            </Callout>
          )}
          {view.stale ? (
            <Callout
              kind="info"
              actions={
                <Button icon="refresh" size="sm" disabled={lock.busy} disabledReason={readBlocked} onClick={readAgain}>
                  {dict.common.actions.readAgain}
                </Button>
              }
            >
              {dict.actuation.stale}
            </Callout>
          ) : null}
          <Card title={t.power.title} className="act-socd-power">
            <div className="act-socd-power__row">
              <Switch
                checked={power}
                disabled={!canEdit}
                label={t.power.label}
                onChange={(on) => {
                  setDraft({ ...draft, power: on });
                }}
              />
              <Badge kind="keyboard_wide" />
            </div>
            <p className="ow-small">{t.power.scope}</p>
            <p className="ow-small ow-muted">{t.power.hotkey}</p>
            {draft.power === null ? null : (
              <p className="ow-small ow-strong">{t.power.pending({ on: draft.power })}</p>
            )}
            {toggledNote === null ? null : (
              <Callout kind="info" role="status">
                {t.power.toggled({ on: toggledNote })}
              </Callout>
            )}
            {replacing && draft.pairs === null && draft.power !== null ? (
              <p className="ow-field__notice ow-field__notice--warning">{t.apply.powerNeedsPairs}</p>
            ) : null}
          </Card>
          <Callout kind="warning" title={t.games.title}>
            {t.games.body}
          </Callout>
          <Card
            title={t.pairs.title({ n: state.profile })}
            className="act-socd-pairs"
            actions={<ReadAt time={state.read_at} />}
          >
            <p>{t.pairs.intro}</p>
            {replacing ? (
              <Callout kind="warning" title={t.replace.title}>
                <p className="act-warnings__line">
                  <span>{t.replace.body}</span>
                  <Badge kind="kept" size="sm" />
                </p>
                {draft.pairs === null ? (
                  <Button
                    size="sm"
                    disabled={!canEdit}
                    onClick={() => {
                      setRows([]);
                    }}
                  >
                    {t.replace.start}
                  </Button>
                ) : (
                  <ConfirmCheckbox
                    checked={draft.allowNotUndoable}
                    tone="warning"
                    label={t.replace.confirm}
                    disabled={!canEdit}
                    onChange={(checked) => {
                      setDraft({ ...draft, allowNotUndoable: checked });
                    }}
                  />
                )}
                <TechnicalDetails>{[state.unwritable_reason ?? "", state.raw_pairs_hex].join("\n").trim()}</TechnicalDetails>
              </Callout>
            ) : null}
            <p className="ow-small ow-muted ow-num">{t.pairs.count({ n: rows.length, max })}</p>
            {rows.length === 0 ? (
              <p className="ow-muted">{replacing && draft.pairs === null ? t.pairs.notShown : t.pairs.none}</p>
            ) : (
              <div className="act-socd-pairs__list">
                {rows.map((row, i) => (
                  <PairRow
                    key={row.id}
                    number={i + 1}
                    row={row}
                    options={options}
                    used={usedKeys(rows, row.id)}
                    problems={check.problems.get(row.id) ?? []}
                    evidence={evidence}
                    disabled={!canEdit}
                    onChange={(next) => {
                      setRows(rows.map((candidate) => (candidate.id === row.id ? next : candidate)));
                    }}
                    onRemove={() => {
                      setRows(rows.filter((candidate) => candidate.id !== row.id));
                    }}
                  />
                ))}
              </div>
            )}
            {check.tooMany ? (
              <p className="ow-field__notice ow-field__notice--error">{t.apply.tooMany({ max })}</p>
            ) : null}
            <div className="act-socd-pairs__add" role="group" aria-label={t.pairs.addLabel}>
              <Button
                size="sm"
                icon="plus"
                disabled={!canEdit || rows.length >= max || (replacing && draft.pairs === null)}
                onClick={() => {
                  addRow(null, null);
                }}
              >
                {t.pairs.add}
              </Button>
              {QUICK.map((quick) => {
                const [a, b] = QUICK_PAIRS[quick];
                const enabled = canEdit && !(replacing && draft.pairs === null) && canQuickAdd(rows, quick, max, offered);
                return (
                  <Button
                    key={quick}
                    size="sm"
                    disabled={!enabled}
                    title={enabled ? undefined : t.pairs.quickUsed}
                    onClick={() => {
                      addRow(a, b);
                    }}
                  >
                    {t.pairs.quick[quick]}
                  </Button>
                );
              })}
            </div>
            <Disclosure summary={t.modesTitle}>
              <dl className="act-socd-modes">
                {SOCD_MODES.map((mode) => (
                  <div key={mode} className="act-socd-modes__item">
                    <dt className="ow-row">
                      <span className="ow-strong">{t.pairs.modeOption({ n: mode, name: modeName(mode, dict) })}</span>
                      <EvidenceBadge evidence={evidence(mode)} size="sm" />
                    </dt>
                    <dd className="ow-small">{modeHint(mode, dict)}</dd>
                  </div>
                ))}
              </dl>
            </Disclosure>
          </Card>
          <ApplyBar
            summary={
              <div className="act-apply-summary">
                {dirty ? (
                  <p className="ow-num">
                    <span className="ow-strong">{dict.actuation.apply.notWritten}</span>
                    {summaryParts.map((part) => (
                      <span key={part}> · {part}</span>
                    ))}
                  </p>
                ) : (
                  <p className="ow-muted">{t.apply.idle}</p>
                )}
              </div>
            }
            dirty={dirty}
            onDiscard={socdDrafts.discard}
            onApply={onApply}
            disabledReason={blockReason}
          />
        </>
      )}
      {flowOpen ? (
        <WriteFlow<SocdEditPreview, EditReport>
          kind="socd_edit"
          prepare={prepare}
          token={(preview) => preview.token}
          title={(preview) => (preview.nothing_to_do ? t.review.titleNothing : t.review.title)}
          review={(preview) => <SocdReview preview={preview} evidence={evidence} />}
          nothingToDo={() => <Callout kind="info" title={t.review.nothing} />}
          confirmLabel={() => dict.common.actions.writeToKeyboard}
          writesSettings
          prepareError={prepareError}
          errorNote={editErrorNote(dict, format, DEFAULT_UNIT_UM)}
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
          onClose={() => {
            setFlowShown(false);
          }}
        />
      ) : null}
    </div>
  );
}
