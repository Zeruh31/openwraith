// Keys > Assignments: the W75 with what each key does on the base or the Fn
// layer (a segmented control; L switches), a single selected key with its
// summary and actions (Change… with the action picker, Reset to default,
// Swap with…), the keys that refer to an empty slot (each with a one-click
// "Reset key" draft) and the sticky apply bar. Every action only changes the
// draft; "Review and write…" opens the shared KeymapEditFlow: prepare (no
// keyboard traffic) → review (Layer | Key | Now | New, the writes in order
// with their own reasons) → Write (armed after 500 ms) → result with Undo.
//
// Built-in Fn functions: a warning before an ordinary one is replaced; the
// essential ones (Fn + 1..4 switch profiles, Fn + Space resets) can be
// changed only after the explicit confirmation, which becomes the library's
// critical acknowledgement. The Fn key and the knob can't be selected; the
// knob's stored entries are listed in its popover, kept as read.

import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";

import { ActionPickerDialog, type ActionSlotOption } from "../../../components/actions";
import { ApplyBar } from "../../../components/ApplyBar";
import { Badge } from "../../../components/Badge";
import { Callout } from "../../../components/Feedback";
import { SegmentedControl } from "../../../components/Controls";
import { Card, TechnicalDetails } from "../../../components/Layout";
import { KeyboardView, revealNearest, usePointerPick, type KeyVisual } from "../../../components/keyboard";
import { builtinName, keyRefText } from "../../../flow/labels";
import { useDict, type Dict } from "../../../i18n";
import type {
  ActionValue,
  BrokenReferenceView,
  EditReportView,
  KeyAssignmentsView,
  KeysState,
  KnobEntryView,
  LayerView,
} from "../../../ipc/bindings";
import { previewUi } from "../../../preview/ui";
import { isLocked } from "../../../state/operation";
import {
  actionText,
  advancedPickerOptions,
  keyAt,
  KeymapEditFlow,
  keyOnLayer,
  libraryNames,
  macroPickerOptions,
  refetchKeys,
  useMacroLibrary,
  type FlowAnswers,
  type KeysTabProps,
  type LabelContext,
} from "../shared";
import { BrokenReferences } from "./BrokenReferences";
import { brokenKeys, keyCap } from "./caps";
import {
  discardDraft,
  seedAssign,
  setFlowOpen,
  setLayer,
  setSelected,
  updateDraft,
  useAssignState,
} from "./draftStore";
import { KeySummary } from "./KeySummary";
import { SwapDialog } from "./SwapDialog";
import {
  changeCount,
  confirmedAcks,
  draftChanges,
  dropTarget,
  entryOn,
  isAssignable,
  isConfirmed,
  isCritical,
  setConfirmed,
  setTarget,
  slotUsers,
  swapKeys,
  withConfirmed,
  type AssignDraft,
} from "./model";
import { seedFrom, type KeysAssignPreviewUi } from "./preview";
import "./keysAssign.css";

const LAYERS: readonly LayerView[] = ["base", "fn"];

/** Whether a key press comes from a field (where L is typed, not a layer switch) or a dialog. */
function inField(target: EventTarget): boolean {
  return (
    target instanceof Element &&
    target.closest("input, textarea, select, [contenteditable='true'], dialog") !== null
  );
}

/**
 * The picker's slot options with "used by" taken from the draft (a slot the
 * draft frees can be chosen; one the draft gives to another key can't).
 */
function withDraftUsers(
  options: readonly ActionSlotOption[],
  kind: "macro" | "advanced",
  state: KeysState,
  draft: AssignDraft,
  layer: LayerView,
  index: number,
  dict: Dict,
): ActionSlotOption[] {
  return options.map((option) => {
    const users = slotUsers(state, draft, kind, option.slot).filter(
      (user) => !(user.layer === layer && user.key === index),
    );
    const names = users.map((user) => keyOnLayer(keyAt(state, user.key)?.key.name ?? String(user.key), user.layer, dict));
    return { ...option, disabledReason: names.length === 0 ? null : dict.keys.slot.usedBy({ keys: names.join(", ") }) };
  });
}

/** A knob entry in words: the decoded action, else the kind of entry, else the library's text. */
function knobEntryText(entry: KnobEntryView, state: KeysState, dict: Dict): string {
  if (entry.hex === "00000000") return dict.keys.action.default;
  if (entry.value !== null) return actionText(entry.value, { state }, dict);
  if (entry.preserved !== null) return dict.keys.preservedKinds[entry.preserved.kind];
  return entry.text;
}

/** What the knob stores (shown in its popover, kept as read). */
function KnobInfo({ state, layer }: { state: KeysState; layer: LayerView }) {
  const dict = useDict();
  const t = dict.keysAssign.knob;
  const entries = state.knob.filter((entry) => entry.layer === layer);
  return (
    <div className="ow-stack keys-assign-knob">
      <p className="ow-small">{dict.keys.knob}</p>
      <p className="ow-small ow-strong keys-entry">
        <span>{t.entriesTitle({ layer: dict.keys.layers[layer] })}</span>
        <Badge kind="kept" size="sm" />
      </p>
      <ul className="keys-assign-knob__list ow-small">
        {entries.map((entry) => (
          <li key={`${entry.layer}-${entry.index}`}>
            {dict.keys.knobEntry({
              n: entry.index,
              layer: dict.keys.layerShort[entry.layer],
              text: knobEntryText(entry, state, dict),
            })}
          </li>
        ))}
      </ul>
      <TechnicalDetails>{entries.map((entry) => t.bytes({ n: entry.index, hex: entry.hex })).join("\n")}</TechnicalDetails>
    </div>
  );
}

/** The assignments tab. */
export default function AssignTab({ state, view, blockReason }: KeysTabProps) {
  const dict = useDict();
  const summaryRef = useRef<HTMLDivElement>(null);
  const pointerPick = usePointerPick();
  const t = dict.keysAssign;
  const stored = useAssignState();
  const { draft, layer, selected } = stored;
  const library = useMacroLibrary();
  const names = useMemo(() => libraryNames(library.library), [library.library]);
  const labels = useMemo<LabelContext>(() => ({ state, libraryNames: names }), [state, names]);

  const [pickerOpen, setPickerOpen] = useState(false);
  const [swapOpen, setSwapOpen] = useState(false);
  const [flowShown, setFlowShown] = useState(false);
  const [announcement, setAnnouncement] = useState("");

  // Browser-preview scenarios preset a layer, a key and a draft (never
  // outside `vite --mode mock`); the review they may open waits for them.
  const [seed] = useState(() => seedFrom(previewUi<KeysAssignPreviewUi>("keysAssign")));
  const [autoOpen, setAutoOpen] = useState(seed.open);
  useEffect(() => {
    seedAssign({ draft: seed.draft, layer: seed.layer, selected: seed.selected });
  }, [seed]);

  // A flow that went away with the tab counts as closed (no toast then).
  useEffect(
    () => () => {
      setFlowOpen(false);
    },
    [],
  );

  const count = changeCount(draft);
  const keyView = selected === null ? undefined : keyAt(state, selected);
  // The draft can be changed unless the keys can't be edited or the view is out of date.
  const editBlock = !state.editable || view.stale ? blockReason : null;
  const broken = useMemo(() => brokenKeys(state), [state]);

  const keyVisual = useCallback(
    (index: number): KeyVisual => keyCap({ state, draft, layer, labels, dict }, index, broken),
    [state, draft, layer, labels, dict, broken],
  );
  const selection = useMemo(() => (selected === null ? new Set<number>() : new Set([selected])), [selected]);
  const knobInfo = useMemo(() => <KnobInfo state={state} layer={layer} />, [state, layer]);

  const switchLayer = (next: LayerView) => {
    setLayer(next);
    setAnnouncement(t.layer.shown[next]);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "l" && event.key !== "L") return;
    if (event.ctrlKey || event.altKey || event.metaKey || inField(event.target)) return;
    event.preventDefault();
    switchLayer(layer === "base" ? "fn" : "base");
  };

  const canChange = (key: KeyAssignmentsView | undefined): key is KeyAssignmentsView =>
    key !== undefined &&
    editBlock === null &&
    isAssignable(key, layer) &&
    (!isCritical(key, layer) || isConfirmed(draft, key.key.index));

  const openPicker = (index: number) => {
    if (canChange(keyAt(state, index))) setPickerOpen(true);
  };

  const pick = (value: ActionValue) => {
    if (keyView === undefined) return;
    const index = keyView.key.index;
    updateDraft((current) => setTarget(state, current, layer, index, value));
    setPickerOpen(false);
  };

  const resetBroken = (item: BrokenReferenceView) => {
    updateDraft((current) => setTarget(state, current, item.layer, item.key.index, { kind: "default" }));
  };

  const onApply = () => {
    if (flowShown || isLocked() || blockReason !== null || count === 0) return;
    setFlowShown(true);
  };

  const showFlow = flowShown || (autoOpen && count > 0);
  // The store's "discarded" toast is not shown for changes the flow makes.
  useEffect(() => {
    setFlowOpen(showFlow);
  }, [showFlow]);

  // A complete write spends the draft, also when the result arrives after
  // the tab went away (the shell shows the report then). The flow stays
  // open for its result (also one a preview scenario opened by itself).
  const onApplied = (report: EditReportView) => {
    setFlowShown(true);
    if (report.headline === "done") discardDraft();
  };

  const onFlowClose = (report: EditReportView | null, answers: FlowAnswers) => {
    setFlowShown(false);
    setAutoOpen(false);
    // Confirmations given in the flow stay with a draft that wasn't written.
    if (report?.headline !== "done") updateDraft((current) => withConfirmed(current, answers.acks));
    // The keys state after a write (or a refused, out-of-date plan).
    refetchKeys();
  };

  let pickerIntro = null;
  if (keyView !== undefined && layer === "fn" && keyView.builtin !== null) {
    const builtin = keyView.builtin;
    const key = keyRefText(keyView.key, "fn", dict);
    pickerIntro = isCritical(keyView, layer) ? (
      <Callout kind="danger" title={dict.keys.review.criticalTitle}>
        <p>{t.picker.criticalConfirmed}</p>
      </Callout>
    ) : entryOn(keyView, "fn").origin.origin === "default" ? (
      <Callout kind="warning">
        <p>{dict.keys.builtinWarning({ key, function: builtinName(builtin.index, builtin.function, dict) })}</p>
      </Callout>
    ) : null;
  }

  return (
    <div className="keys-assign" onKeyDown={onKeyDown}>
      <p className="ow-muted">{t.intro}</p>
      <div className="keys-assign__panes">
        <Card
          title={t.board.title}
          className="keys-assign-board-card"
          actions={
            <SegmentedControl
              label={t.layer.label}
              size="sm"
              options={LAYERS.map((value) => ({ value, label: dict.keys.layers[value] }))}
              value={layer}
              onChange={switchLayer}
            />
          }
        >
          <div className="ow-stack">
            <div
              className={`keys-assign-board keys-assign-board--${layer}`}
              onPointerDownCapture={pointerPick.onPointerDownCapture}
            >
              <KeyboardView
                label={t.board.label[layer]}
                selection="single"
                keyVisual={keyVisual}
                selected={selection}
                onSelectionChange={(next) => {
                  const [index] = [...next];
                  setSelected(index ?? null);
                  // A click on a key shows its panel (stacked below the board in narrow windows).
                  if (index !== undefined && pointerPick.byPointer()) {
                    window.requestAnimationFrame(() => {
                      revealNearest(summaryRef.current);
                    });
                  }
                }}
                onActivate={openPicker}
                knobLabel={dict.keys.knob}
                knobInfo={knobInfo}
              />
            </div>
            <ul className="keys-assign-legend ow-small" aria-label={t.legend.label}>
              <li>
                <span className="keys-assign-legend__mark keys-assign-legend__mark--remapped" aria-hidden="true" />
                {t.legend.remapped}
              </li>
              {layer === "fn" ? (
                <li>
                  <span className="keys-assign-legend__mark keys-assign-legend__mark--builtin" aria-hidden="true" />
                  {t.legend.builtin}
                </li>
              ) : null}
              <li>
                <span className="keys-assign-legend__symbol keys-assign-legend__symbol--pending" aria-hidden="true">
                  ●
                </span>
                {t.legend.pending}
              </li>
              <li>
                <span className="keys-assign-legend__symbol keys-assign-legend__symbol--warning" aria-hidden="true">
                  ⚠
                </span>
                {t.legend.broken}
              </li>
            </ul>
            <p className="ow-small ow-muted">
              {t.board.hint} {t.layer.hint}
            </p>
            <p className="sr-only" role="status" aria-live="polite">
              {announcement}
            </p>
          </div>
        </Card>
        <div ref={summaryRef} className="keys-assign__summary-anchor">
        <KeySummary
          state={state}
          draft={draft}
          layer={layer}
          keyView={keyView}
          labels={labels}
          editBlock={editBlock}
          onChange={() => {
            if (keyView !== undefined) openPicker(keyView.key.index);
          }}
          onReset={() => {
            if (keyView !== undefined) {
              const index = keyView.key.index;
              updateDraft((current) => setTarget(state, current, layer, index, { kind: "default" }));
            }
          }}
          onSwap={() => {
            if (canChange(keyView)) setSwapOpen(true);
          }}
          onKeep={() => {
            if (keyView !== undefined) {
              const index = keyView.key.index;
              updateDraft((current) => dropTarget(current, layer, index));
            }
          }}
          onResetBroken={resetBroken}
          onConfirm={(confirmed) => {
            if (keyView !== undefined) {
              const index = keyView.key.index;
              updateDraft((current) => setConfirmed(current, index, confirmed));
            }
          }}
        />
        </div>
      </div>
      <BrokenReferences items={state.broken} draft={draft} editBlock={editBlock} onReset={resetBroken} />
      <ApplyBar
        summary={
          count > 0 ? (
            <p className="ow-num">
              <span className="ow-strong">{t.apply.notWritten}</span>
              <span> · {t.apply.count({ n: count })}</span>
            </p>
          ) : (
            <p className="ow-muted">{t.apply.idle}</p>
          )
        }
        dirty={count > 0}
        onDiscard={discardDraft}
        onApply={onApply}
        disabledReason={blockReason}
      />
      {pickerOpen && keyView !== undefined ? (
        <ActionPickerDialog
          open
          title={dict.keys.picker.title({ key: keyOnLayer(keyView.key.name, layer, dict) })}
          onClose={() => {
            setPickerOpen(false);
          }}
          intro={pickerIntro}
          layer={layer}
          value={entryOn(keyView, layer).value}
          macros={
            keyView.macro_capable
              ? withDraftUsers(
                  macroPickerOptions(labels, layer, keyView.key.index, dict),
                  "macro",
                  state,
                  draft,
                  layer,
                  keyView.key.index,
                  dict,
                )
              : undefined
          }
          advanced={
            state.advanced_editable
              ? withDraftUsers(
                  advancedPickerOptions(labels, layer, keyView.key.index, dict),
                  "advanced",
                  state,
                  draft,
                  layer,
                  keyView.key.index,
                  dict,
                )
              : undefined
          }
          onPick={pick}
        />
      ) : null}
      {swapOpen && keyView !== undefined ? (
        <SwapDialog
          state={state}
          draft={draft}
          layer={layer}
          keyView={keyView}
          labels={labels}
          onClose={() => {
            setSwapOpen(false);
          }}
          onSwap={(other) => {
            const index = keyView.key.index;
            updateDraft((current) => swapKeys(state, current, layer, index, other));
            setSwapOpen(false);
          }}
        />
      ) : null}
      {showFlow ? (
        <KeymapEditFlow
          state={state}
          changes={draftChanges(state, draft)}
          acks={confirmedAcks(draft)}
          libraryNames={names}
          onApplied={onApplied}
          onClose={onFlowClose}
        />
      ) : null}
    </div>
  );
}

