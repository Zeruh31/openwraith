// Actuation > Actuation & Rapid Trigger: the W75 with each key's actuation
// point or Rapid Trigger sensitivity, selection tools, the presets menu, an
// inspector that edits a local draft, and the sticky apply bar. Writing goes
// only through the shared WriteFlow: `prepare_trigger_edit` (or
// `prepare_preset_edit` for a draft filled from a preset; read-only plan from
// a fresh read) → review → explicit Write → `apply_trigger_edit` (one packet
// per key, the keyboard's save wait) → report with the safety backup and
// Undo. Nothing is sent while a slider is dragged or a value typed.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ApplyBar } from "../../components/ApplyBar";
import { Button } from "../../components/Button";
import { Callout } from "../../components/Feedback";
import { Card, PageHeader } from "../../components/Layout";
import type { KeyVisual } from "../../components/keyboard";
import { notEditableText } from "../../flow/labels";
import { EditReportView } from "../../flow/Report";
import { WriteFlow, type OnProgress } from "../../flow/WriteFlow";
import { keyName, useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type {
  AppError,
  EditReportView as EditReport,
  PlanToken,
  PresetRef,
  TriggerEditPreview,
} from "../../ipc/bindings";
import { api } from "../../ipc/commands";
import { previewUi } from "../../preview/ui";
import { readAgain } from "../../shell/actions";
import { NotReadEmptyState } from "../../shell/NotReadEmptyState";
import { ReadAt } from "../../shell/ReadAt";
import { isLocked, useOperationLock } from "../../state/operation";
import { useSnapshot } from "../../state/snapshot";
import { useStatus } from "../../state/status";
import { ActuationKeyboard } from "./ActuationKeyboard";
import {
  applyPreset,
  discardDraft,
  effectiveDraft,
  ownerOf,
  seedState,
  setDraft as storeDraft,
  setFlowOpen,
  setSelection,
  useActuationState,
} from "./draftStore";
import { Inspector } from "./Inspector";
import {
  basicActuation,
  buildIntent,
  changedKeys,
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
  sameSettingKeys,
  selectedValues,
  summarize,
  targetOf,
  unitOf,
  valueTexts,
  type ShowMode,
  type TriggerDraft,
} from "./model";
import PresetsMenu from "./presets";
import { seedFrom, type ActuationPreviewUi } from "./preview";
import { editErrorNote, editPrepareError } from "./shared/EditParts";
import { gateReason } from "./shared/editTexts";
import { Toolbar } from "./Toolbar";
import { NothingToChange, TriggerReview } from "./TriggerReview";
import "./actuation.css";

/** The Actuation & Rapid Trigger page. */
export default function ActuationPage() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const { snapshot, stale, loading } = useSnapshot();
  const lock = useOperationLock();
  const t = dict.actuation;

  const [seed] = useState(() => seedFrom(previewUi<ActuationPreviewUi>("actuation")));
  const [show, setShow] = useState<ShowMode>(seed.show);
  const identityKey = ownerOf(snapshot);
  const stored = useActuationState();
  const selection = stored.selection;
  const [flowOpen, setFlowShown] = useState(false);
  const [autoOpen, setAutoOpen] = useState(seed.open);
  // The name of the user preset the draft was filled from (for the review).
  const [userPresetName, setUserPresetName] = useState<string | null>(null);

  // The draft and the selection live in a store that survives leaving the
  // page; it drops a draft made on another profile or connection itself
  // (with a toast), whether this page is shown or not.
  const draft = effectiveDraft(stored, identityKey);
  const dirty = isDirty(draft);

  const setDraft = (next: TriggerDraft) => {
    storeDraft(identityKey, next);
  };
  const discard = discardDraft;

  // Browser-preview scenarios preset a selection and a draft (never outside
  // `vite --mode mock`); the review they may open waits for them.
  useEffect(() => {
    seedState(seed.draft, seed.selection);
  }, [seed]);
  const seeded = !isDirty(seed.draft) || isDirty(stored.draft);

  // A flow that went away with the page counts as closed (no toast then).
  useEffect(
    () => () => {
      setFlowOpen(false);
    },
    [],
  );

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

  // Why nothing can be written now, whatever is selected (also the presets' reason).
  const baseReason =
    snapshot === null
      ? dict.errors.writeBlocked.not_connected
      : !snapshot.triggers.editable
        ? t.disabled[snapshot.triggers.disabled ?? "limits_unknown"]
        : stale
          ? dict.errors.writeBlocked.snapshot_stale
          : gateReason(status, dict);
  // Why "Review and write…" can't be used now (shown under the button).
  let blockReason: string | null = baseReason;
  if (blockReason !== null) {
    // Said above.
  } else if (selection.size === 0) {
    blockReason = t.apply.reasons.selectKeys;
  } else if (summary.keys.length === 0) {
    blockReason = t.apply.reasons.noEditable;
  } else if (summary.changing.length === 0) {
    blockReason = t.apply.reasons.nothing;
  }

  const onApply = () => {
    if (flowOpen || isLocked() || blockReason !== null) return;
    setFlowShown(true);
  };

  // A preset fills the draft and the selection, then opens the review.
  const onPreset = (chosenPreset: PresetRef, next: TriggerDraft, chosen: ReadonlySet<number>, name: string) => {
    if (flowOpen || isLocked()) return;
    applyPreset(identityKey, next, chosen, chosenPreset);
    setUserPresetName(chosenPreset.kind === "user" ? name : null);
    setFlowShown(true);
  };
  const preset = stored.preset;

  // The intent is built when the flow prepares (and again on "Review again"),
  // from the state the user sees then, with that snapshot's revision.
  const prepare = async (): Promise<TriggerEditPreview> => {
    if (snapshot === null) {
      const error: AppError = { kind: "not_connected", detail: null, message: "no keyboard values to plan from" };
      throw error;
    }
    if (preset !== null && isDirty(draft) && summary.keys.length > 0) {
      // The preset's own values, planned by the backend (refused, never clamped, when out of range).
      return api.preparePresetEdit({ preset, keys: [...summary.keys], based_on: snapshot.revision });
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

  const closeFlow = () => {
    setFlowShown(false);
    setAutoOpen(false);
  };

  // A complete write makes the draft spent, also when the result arrives
  // after the flow went away (history navigation during the write; the
  // shell shows the report then).
  const apply = (token: PlanToken, onProgress: OnProgress) =>
    api.applyTriggerEdit(token, onProgress).then((report) => {
      if (report.headline === "done") discardDraft();
      return report;
    });

  const showFlow = flowOpen || (autoOpen && seeded && snapshot !== null);
  // The preset's name for the review: a built-in one by id, a user preset's as it was chosen.
  const builtinNames: Readonly<Record<string, string>> = t.presets.builtin;
  const presetLabel =
    preset === null
      ? null
      : preset.kind === "builtin"
        ? (builtinNames[preset.id] ?? preset.id)
        : (userPresetName ?? t.presets.userFallback);
  // The store's "discarded" toast is not shown for changes the flow makes.
  useEffect(() => {
    setFlowOpen(showFlow);
  }, [showFlow]);
  const leftOut = summary.leftOut.length;
  const parts = [t.apply.willChange({ n: summary.changing.length })];
  if (summary.unchanged.length > 0) parts.push(t.apply.already({ n: summary.unchanged.length }));
  if (leftOut > 0) parts.push(t.apply.leftOut({ n: leftOut }));

  return (
    <div className="act-page">
      <PageHeader title={dict.shell.nav.actuation} description={t.description} />
      {snapshot === null ? (
        <Card>
          <NotReadEmptyState
            icon="keyboard"
            title={t.empty.title}
            body={t.empty.body}
            failed={t.empty.body}
            loading={loading}
            error={false}
          />
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
            actions={<ReadAt time={snapshot.read_at} />}
          >
            <Toolbar<ShowMode>
              selection={selection}
              onSelect={setSelection}
              selectable={selectable}
              changed={changed}
              same={(current) => sameSettingKeys(current, keys, selectable)}
              changedHint={t.select.changedHint}
              show={{
                value: show,
                onChange: setShow,
                options: [
                  { value: "actuation", label: t.show.actuation },
                  { value: "rapid_trigger", label: t.show.rapidTrigger },
                ],
              }}
              extra={
                <PresetsMenu
                  keys={keys}
                  selection={selection}
                  draft={draft}
                  unitUm={unitUm}
                  disabledReason={baseReason}
                  onApply={onPreset}
                />
              }
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
          review={(preview) => (
            <TriggerReview preview={preview} leftOut={leftOut} unitUm={unitUm} presetName={presetLabel} />
          )}
          nothingToDo={() => <NothingToChange />}
          confirmLabel={() => dict.common.actions.writeToKeyboard}
          writesSettings
          prepareError={editPrepareError(unitUm)}
          errorNote={editErrorNote(dict, format, unitUm)}
          apply={apply}
          result={(report) => <EditReportView report={report} />}
          onClose={closeFlow}
        />
      ) : null}
    </div>
  );
}
