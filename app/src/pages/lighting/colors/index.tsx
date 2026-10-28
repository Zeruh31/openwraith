// Lighting > Key colours: the colour of each key of the active profile
// (custom colour table 1, which the keyboard shows with the Custom effect).
//
// The tools only change a local draft (no IPC while painting): Select +
// "Fill selection" (the keyboard path), Brush (paint one key), Eyedropper
// (take a key's colour), "Fill all keys" and a gradient over the selection;
// the draft has its own undo/redo. "Review and write…" opens the shared
// confirmation flow: the backend plans the edit (read-only), the review
// shows the boards before and after and what changes, the user presses
// Write, the backend saves a safety backup, writes, verifies, waits for the
// keyboard to save and reads the colours again; the report offers Undo.
// "Also switch the effect to Custom" is opt-in; slots outside the layout
// (the knob, unused positions) are kept as read.

import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";

import { ApplyBar } from "../../../components/ApplyBar";
import { Badge } from "../../../components/Badge";
import { Button } from "../../../components/Button";
import { ColorPicker, rememberColor } from "../../../components/color/ColorPicker";
import { Checkbox, SegmentedControl } from "../../../components/Controls";
import { Callout, EmptyState, Spinner } from "../../../components/Feedback";
import { ColorField } from "../../../components/Fields";
import { KeyboardView, KNOB_INDEX, revealNearest, usePointerPick, type KeyVisual } from "../../../components/keyboard";
import { Card, ColorSwatch, PageHeader, ProfileBadge, TechnicalDetails } from "../../../components/Layout";
import { EditReportView } from "../../../flow/Report";
import { WriteFlow, type OnProgress, type PrepareErrorView } from "../../../flow/WriteFlow";
import { W75_KEYS } from "../../../generated/w75Keys";
import { keyName, useDict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type {
  AppError,
  ColorEditPreview,
  ColorsState,
  EditReportView as EditReport,
  PlanToken,
} from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { previewUi } from "../../../preview/ui";
import { href, navigate, tabTarget } from "../../../routes";
import { readAgain } from "../../../shell/actions";
import { ReadAt } from "../../../shell/ReadAt";
import { connectionSummary } from "../../../shell/statusText";
import { isLocked, useOperationLock } from "../../../state/operation";
import { useSnapshot } from "../../../state/snapshot";
import { useStatus } from "../../../state/status";
import { basisOf } from "../draftStore";
import { modeName } from "../labels";
import { ColorsReview } from "./ColorsReview";
import { blockText, colorText, disabledText, editProblemText, legendText } from "./labels";
import {
  buildColorIntent,
  COLOR_TOOLS,
  colorsBlock,
  draftChanges,
  draftFromPreview,
  EMPTY_COLORS,
  GRADIENT_DIRECTIONS,
  gradientColors,
  keyboardColors,
  paint,
  paintEach,
  shownColor,
  switchApplies,
  SWATCHES,
  type ColorMap,
  type ColorsPreviewUi,
  type ColorTool,
  type GradientDirection,
} from "./model";
import {
  editColors,
  redoColors,
  refetchColors,
  seedColorsDraft,
  setColorsFlowOpen,
  setSwitchToCustom,
  spendColorsDraft,
  undoColors,
  useColorsDraft,
  useColorsState,
} from "./store";
import { ToolIcon } from "./ToolIcon";
import "../lighting.css";
import "./colors.css";

/** The library's key names by index (the knob included). */
const KEY_NAMES: ReadonlyMap<number, string> = new Map(W75_KEYS.map((key) => [key.index, key.name]));

const NO_KEYS: ReadonlySet<number> = new Set<number>();

const NO_BREAK_SPACE = String.fromCharCode(0xa0);

/** What the next prepare uses (kept current after each render; the review options update it at once). */
interface FlowInputs {
  state: ColorsState | null;
  draft: ColorMap;
  switchToCustom: boolean;
  checkPersistence: boolean;
}

function noChanges(): AppError {
  return {
    kind: "invalid_request",
    detail: { detail: "edit", problem: { problem: "no_changes" } },
    message: "nothing to prepare: no key colour differs from the keyboard's",
  };
}

/** Whether a key event comes from a text field (its own undo applies there). */
function fromTextField(target: EventTarget): boolean {
  return (
    target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement
  );
}

/** The Key colours tab. */
export default function ColorsTab() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const { snapshot, stale } = useSnapshot();
  const lock = useOperationLock();
  const view = useColorsState();
  const stored = useColorsDraft();
  const toolHintId = useId();
  const t = dict.lighting.colors;

  const [tool, setTool] = useState<ColorTool>("select");
  const [backTool, setBackTool] = useState<Exclude<ColorTool, "eyedropper">>("select");
  const [selection, setSelection] = useState<ReadonlySet<number>>(NO_KEYS);
  const [paintColor, setPaintColor] = useState("#ffffff");
  const [gradientTo, setGradientTo] = useState("#0000ff");
  const [direction, setDirection] = useState<GradientDirection>("horizontal");
  const [announcement, setAnnouncement] = useState("");
  const [flowOpen, setFlowOpen] = useState(false);
  // The colours the open flow was started on (shown by the review if the colours go away meanwhile).
  const [flowState, setFlowState] = useState<ColorsState | null>(null);
  const opening = useRef(false);
  const paintRef = useRef<HTMLDivElement>(null);
  const pointerPick = usePointerPick();

  const state = view.state;
  const stateBasis = state === null ? null : `${String(state.generation)}:${String(state.profile)}`;
  const basis = basisOf(snapshot) ?? stateBasis;
  // The draft applies only to the colours of its own profile and connection.
  const draft: ColorMap =
    state !== null && (stored.basis === null || stored.basis === stateBasis) ? stored.history.present : EMPTY_COLORS;
  const canUndo = stored.history.past.length > 0 && (stored.basis === null || stored.basis === stateBasis);
  const canRedo = stored.history.future.length > 0 && (stored.basis === null || stored.basis === stateBasis);
  const current = useMemo(() => (state === null ? new Map<number, string>() : keyboardColors(state)), [state]);
  const readOnly = stale || state === null || !state.editable;

  const inputs = useRef<FlowInputs>({ state, draft, switchToCustom: stored.switchToCustom, checkPersistence: false });
  useEffect(() => {
    inputs.current = { ...inputs.current, state, draft, switchToCustom: stored.switchToCustom };
  });

  // Browser-preview scenarios may preset a draft (never outside `vite --mode mock`).
  useEffect(() => {
    const seed = draftFromPreview(previewUi<ColorsPreviewUi>("lightingColors"));
    seedColorsDraft(seed.colors, seed.switchToCustom);
  }, []);

  // A flow that went away with the tab counts as closed.
  useEffect(
    () => () => {
      setColorsFlowOpen(false);
    },
    [],
  );

  const announce = (text: string) => {
    // The same text twice is still announced (it differs by a no-break space).
    setAnnouncement((previous) => (previous === text ? text + NO_BREAK_SPACE : text));
  };

  const spokenKey = (index: number) => keyName(dict, KEY_NAMES.get(index) ?? String(index));

  /** Sets `keys` to `color` (one undo step) and says what happened. */
  const fill = (keys: readonly number[], color: string, said: (n: number) => string) => {
    if (readOnly || keys.length === 0) return;
    const next = paint(draft, current, keys, color);
    if (next === draft) {
      announce(keys.length === 1 ? t.announce.unchanged({ key: spokenKey(keys[0] ?? 0) }) : t.announce.nothingChanged);
      return;
    }
    editColors(basis, (present) => paint(present, current, keys, color));
    rememberColor(color);
    announce(said(keys.length));
  };

  const fillGradient = () => {
    if (readOnly || selection.size < 2) return;
    const colors = gradientColors(selection, paintColor, gradientTo, direction);
    const next = paintEach(draft, current, colors);
    if (next === draft) {
      announce(t.announce.nothingChanged);
      return;
    }
    editColors(basis, (present) => paintEach(present, current, colors));
    announce(
      t.announce.gradient({ n: colors.size, from: colorText(dict, paintColor), to: colorText(dict, gradientTo) }),
    );
  };

  const onBoardChange = (next: Set<number>) => {
    if (tool === "select") {
      // The first keys picked with the pointer: show the colour panel (below the board in narrow windows).
      if (selection.size === 0 && next.size > 0 && pointerPick.byPointer()) {
        window.requestAnimationFrame(() => {
          revealNearest(paintRef.current);
        });
      }
      setSelection(next);
      return;
    }
    const index = [...next][0];
    if (index === undefined) return;
    if (tool === "brush") {
      fill([index], paintColor, () => t.announce.painted({ key: spokenKey(index), color: colorText(dict, paintColor) }));
      return;
    }
    const color = shownColor(draft, current, index);
    if (color === null) return;
    setPaintColor(color);
    setTool(backTool);
    announce(t.announce.picked({ key: spokenKey(index), color: colorText(dict, color) }));
  };

  const chooseTool = (next: ColorTool) => {
    if (next !== "eyedropper") setBackTool(next);
    else if (tool !== "eyedropper") setBackTool(tool);
    setTool(next);
  };

  const doUndo = () => {
    if (!canUndo) return;
    undoColors();
    const after = stored.history.past[stored.history.past.length - 1] ?? EMPTY_COLORS;
    announce(t.announce.undone({ n: draftChanges(after, current).length }));
  };

  const doRedo = () => {
    if (!canRedo) return;
    redoColors();
    const after = stored.history.future[stored.history.future.length - 1] ?? EMPTY_COLORS;
    announce(t.announce.redone({ n: draftChanges(after, current).length }));
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (flowOpen || !(event.ctrlKey || event.metaKey) || event.altKey || fromTextField(event.target)) return;
    const key = event.key.toLowerCase();
    if (key === "z" && !event.shiftKey) {
      event.preventDefault();
      doUndo();
    } else if (key === "y" || (key === "z" && event.shiftKey)) {
      event.preventDefault();
      doRedo();
    }
  };

  const keyVisual = useCallback(
    (index: number): KeyVisual => {
      const library = KEY_NAMES.get(index) ?? String(index);
      const base = { legend: legendText(dict, library), name: keyName(dict, library) };
      const now = current.get(index);
      if (index === KNOB_INDEX || now === undefined) return { ...base, selectable: false };
      const next = draft.get(index);
      return {
        ...base,
        swatch: next ?? now,
        marker: next === undefined ? null : "pending",
        description:
          next === undefined
            ? colorText(dict, now)
            : dict.lighting.colors.board.changed({ now: colorText(dict, now), next: colorText(dict, next) }),
      };
    },
    [dict, current, draft],
  );

  const changes = state === null ? [] : draftChanges(draft, current);
  const dirty = changes.length > 0;
  const switching = state !== null && switchApplies(state, stored.switchToCustom);
  const block = colorsBlock(status, status.busy !== null, stale, state);
  const ready = status.connection.state === "ready";

  const openFlow = () => {
    if (opening.current || !dirty || block !== null || isLocked()) return;
    opening.current = true;
    inputs.current = { ...inputs.current, checkPersistence: false };
    setColorsFlowOpen(true);
    setFlowState(state);
    setFlowOpen(true);
  };

  const onFlowClose = () => {
    opening.current = false;
    setColorsFlowOpen(false);
    setFlowOpen(false);
  };

  const prepare = async (): Promise<ColorEditPreview> => {
    const { state: planState, draft: planDraft, switchToCustom, checkPersistence } = inputs.current;
    const intent =
      planState === null ? null : buildColorIntent(planState, planDraft, { switchToCustom, checkPersistence });
    if (intent === null) throw noChanges();
    try {
      return await api.prepareColorEdit(intent);
    } catch (error: unknown) {
      const appError = toAppError(error);
      // The colours changed since they were read: fetch them again for the next try.
      if (appError.kind === "stale_plan") refetchColors();
      throw appError;
    }
  };

  // Written colours make the draft spent, also when the result arrives after
  // the tab went away (the shell shows the report then); the colours are
  // fetched again either way (a colour write brings no new snapshot).
  const apply = (token: PlanToken, onProgress: OnProgress) =>
    api.applyColorEdit(token, onProgress).then((report) => {
      if (report.anything_written) spendColorsDraft();
      refetchColors();
      return report;
    });

  const prepareError = (error: AppError): PrepareErrorView | null => {
    if (error.detail?.detail !== "edit") return null;
    const problem = error.detail.problem;
    return {
      body: (
        <Callout
          kind={problem.problem === "no_changes" ? "warning" : "danger"}
          title={dict.errors.kind[error.kind].title}
          role="alert"
        >
          <p>{editProblemText(dict, problem)}</p>
          <p className="ow-strong">{dict.flow.nothingWritten}</p>
          {error.message === "" ? null : <TechnicalDetails>{error.message}</TechnicalDetails>}
        </Callout>
      ),
    };
  };

  // The flow stays mounted whatever happens to the colours (a lost
  // connection during the write clears them): the progress and the report
  // stay in view. It sits at the same place in every render below.
  const flowElement = !flowOpen ? null : (
    <WriteFlow<ColorEditPreview, EditReport>
      kind="color_edit"
      prepare={prepare}
      token={(preview) => preview.token}
      title={(preview) => (preview.nothing_to_do ? t.review.nothingTitle : t.review.title({ n: preview.profile }))}
      review={(preview) => {
        const shown = state ?? flowState;
        return shown === null ? null : <ColorsReview preview={preview} state={shown} />;
      }}
      nothingToDo={() => <Callout kind="info" title={t.review.nothing} />}
      confirmLabel={() => dict.common.actions.writeToKeyboard}
      writesSettings
      prepareError={prepareError}
      reviewOptions={(preview, reprepare) => (
        <div className="li-colors-review__options">
          {!preview.shown_now && state?.can_switch_to_custom === true ? (
            <Checkbox
              checked={preview.switches_to_custom}
              label={t.review.switchOption}
              description={t.notShown.switchHint}
              onChange={(checked) => {
                inputs.current = { ...inputs.current, switchToCustom: checked };
                setSwitchToCustom(checked);
                reprepare();
              }}
            />
          ) : null}
          {preview.persistence.forced ? (
            <p className="ow-small">{t.review.persistence.forced}</p>
          ) : (
            <Checkbox
              checked={preview.persistence.requested}
              label={t.review.persistence.check({ m: preview.persistence.away_profile })}
              onChange={(checked) => {
                inputs.current = { ...inputs.current, checkPersistence: checked };
                reprepare();
              }}
            />
          )}
        </div>
      )}
      apply={apply}
      result={(report) => <EditReportView report={report} />}
      onClose={onFlowClose}
    />
  );

  const header = (
    <PageHeader
      title={dict.lighting.title}
      description={t.description}
      actions={
        state === null ? undefined : <ReadAt time={state.read_at} />
      }
    />
  );

  if (state === null) {
    const waiting = view.loading || (ready && view.error === null);
    return (
      <div className="li-page li-colors">
        {header}
        {waiting ? (
          <p className="ow-row">
            <Spinner />
            <span>{t.empty.loading}</span>
          </p>
        ) : ready ? (
          <EmptyState
            icon="light"
            title={t.disabled.title}
            action={
              <Button icon="refresh" disabled={lock.busy} onClick={readAgain}>
                {dict.common.actions.readAgain}
              </Button>
            }
          >
            <p>{t.empty.failed}</p>
          </EmptyState>
        ) : (
          <EmptyState
            icon="plug"
            title={t.empty.title}
            action={
              <Button
                onClick={() => {
                  navigate("device");
                }}
              >
                {dict.lighting.empty.goToKeyboard}
              </Button>
            }
          >
            <p>{connectionSummary(status, dict, format).text}</p>
          </EmptyState>
        )}
        {flowElement}
      </div>
    );
  }

  const effect = state.light_mode === null ? null : modeName(dict, state.light_mode);
  const selectedColors = new Set([...selection].map((index) => shownColor(draft, current, index)));
  const onlyColor = selectedColors.size === 1 ? [...selectedColors][0] : undefined;
  const swatches = SWATCHES.map((swatch) => ({
    hex: swatch.hex,
    name: swatch.name === "off" ? t.paint.off : dict.lighting.color.names[swatch.name],
  }));
  const fillName = colorText(dict, paintColor);

  const summary = dirty ? (
    <span className="li-summary">
      <span className="ow-strong">{dict.lighting.applyBar.notWritten}</span>
      <span className="li-summary__part">{t.applyBar.changes({ n: changes.length })}</span>
      {switching ? <span className="li-summary__part">{t.applyBar.switches}</span> : null}
    </span>
  ) : (
    <span className="ow-muted">{dict.lighting.applyBar.noChanges}</span>
  );

  return (
    <div className="li-page li-colors" onKeyDown={onKeyDown}>
      {header}

      <Callout kind="info" title={t.perProfile.title}>
        <p>{t.perProfile.body({ n: state.profile })}</p>
      </Callout>

      {state.editable ? null : (
        <Callout kind="warning" title={t.disabled.title}>
          <p>{disabledText(dict, state.disabled)}</p>
          {state.disabled?.reason === "unreadable" ? <TechnicalDetails>{state.disabled.message}</TechnicalDetails> : null}
        </Callout>
      )}

      {state.shown_now ? null : (
        <Callout
          kind="warning"
          title={t.notShown.title}
          actions={
            <a className="li-link" href={href(tabTarget("lighting", "effect"))}>
              {t.notShown.goToEffect}
            </a>
          }
        >
          <p>{effect === null ? t.notShown.bodyUnknown : t.notShown.body({ effect })}</p>
          {state.can_switch_to_custom ? (
            <Checkbox
              checked={stored.switchToCustom}
              label={t.notShown.switch}
              description={t.notShown.switchHint}
              disabled={readOnly}
              onChange={setSwitchToCustom}
            />
          ) : (
            <p>{t.notShown.cannotSwitch}</p>
          )}
        </Callout>
      )}

      <div className="li-colors__layout">
        <Card title={t.board.title} className="li-colors__board" actions={<ProfileBadge profile={state.profile} />}>
          <div className="li-colors__toolbar">
            <SegmentedControl<ColorTool>
              label={t.tools.label}
              size="sm"
              value={tool}
              disabled={readOnly}
              describedBy={toolHintId}
              options={COLOR_TOOLS.map((value) => ({
                value,
                label: (
                  <span className="li-colors__tool">
                    <ToolIcon name={value} size={16} />
                    <span>{t.tools.names[value]}</span>
                  </span>
                ),
              }))}
              onChange={chooseTool}
            />
            <div className="li-colors__history" role="group" aria-label={t.tools.history}>
              <Button size="sm" icon="undo" disabled={!canUndo || readOnly} title={t.tools.undoHint} onClick={doUndo}>
                {t.tools.undo}
              </Button>
              <Button
                size="sm"
                icon="undo"
                className="li-colors__redo"
                disabled={!canRedo || readOnly}
                title={t.tools.undoHint}
                onClick={doRedo}
              >
                {t.tools.redo}
              </Button>
            </div>
          </div>
          <p id={toolHintId} className="ow-muted ow-small">
            {t.tools.hints[tool]}
          </p>
          <div onPointerDownCapture={pointerPick.onPointerDownCapture}>
          <KeyboardView
            label={t.board.label}
            keyVisual={keyVisual}
            selected={tool === "select" ? selection : NO_KEYS}
            selection={tool === "select" ? "multi" : "single"}
            onSelectionChange={onBoardChange}
            knobLabel={t.board.knob}
            disabled={readOnly}
          />
          </div>
          <div className="li-colors__selection">
            <span className="ow-small" aria-live="polite">
              {t.board.selected({ n: selection.size })}
              {selection.size === 0 ? null : (
                <>
                  {" · "}
                  {onlyColor === undefined || onlyColor === null ? t.board.mixed : t.board.selectedColor({ color: colorText(dict, onlyColor) })}
                </>
              )}
            </span>
            <span className="ow-row">
              <Button
                size="sm"
                variant="ghost"
                disabled={readOnly}
                onClick={() => {
                  setTool("select");
                  setSelection(new Set(state.keys.map((entry) => entry.key.index)));
                }}
              >
                {t.board.selectAll}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={selection.size === 0}
                onClick={() => {
                  setSelection(NO_KEYS);
                }}
              >
                {t.board.clearSelection}
              </Button>
            </span>
          </div>
          {state.kept.length === 0 ? null : (
            <section className="li-colors__kept">
              <h3 className="li-colors__kept-title">
                <span>{t.kept.title}</span>
                <Badge kind="kept" size="sm" />
              </h3>
              <p className="ow-small ow-muted">{t.kept.body}</p>
              <ul className="li-colors__kept-list">
                {state.kept.map((kept) => (
                  <li key={kept.slot} className="li-colors__cell">
                    <ColorSwatch color={kept.color} showHex={false} />
                    <span>
                      {kept.slot === KNOB_INDEX
                        ? t.kept.knob({ color: colorText(dict, kept.color) })
                        : t.kept.slot({ slot: kept.slot, color: colorText(dict, kept.color) })}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </Card>

        <div ref={paintRef} className="li-colors__tools-anchor">
        <Card title={t.paint.title} className="li-colors__tools">
          <ColorPicker label={t.paint.picker} value={paintColor} onChange={setPaintColor} swatches={swatches} disabled={readOnly} />
          <div className="li-colors__controls">
            <div className="li-colors__actions">
              <Button
                disabled={readOnly}
                disabledReason={readOnly || selection.size > 0 ? null : t.paint.needSelection}
                onClick={() => {
                  fill([...selection], paintColor, (n) => t.announce.filled({ n, color: fillName }));
                }}
              >
                {t.paint.fillSelection}
              </Button>
              <Button
                disabled={readOnly}
                onClick={() => {
                  fill(
                    state.keys.map((entry) => entry.key.index),
                    paintColor,
                    (n) => t.announce.filled({ n, color: fillName }),
                  );
                }}
              >
                {t.paint.fillAll}
              </Button>
            </div>
            <section className="li-colors__gradient">
              <h3 className="li-colors__gradient-title">
                <ToolIcon name="gradient" size={16} />
                <span>{t.gradient.title}</span>
              </h3>
              <ColorField label={t.gradient.to} value={gradientTo} onChange={setGradientTo} disabled={readOnly} />
              <p className="ow-small ow-muted">{t.gradient.hint}</p>
              <SegmentedControl<GradientDirection>
                label={t.gradient.direction}
                size="sm"
                value={direction}
                disabled={readOnly}
                options={GRADIENT_DIRECTIONS.map((value) => ({ value, label: t.gradient.directions[value] }))}
                onChange={setDirection}
              />
              <Button
                disabled={readOnly}
                disabledReason={readOnly || selection.size >= 2 ? null : t.gradient.needTwo}
                onClick={fillGradient}
              >
                {t.gradient.fill}
              </Button>
            </section>
          </div>
        </Card>
        </div>
      </div>

      <p className="sr-only" role="status" aria-live="polite">
        {announcement}
      </p>

      <ApplyBar
        summary={summary}
        dirty={dirty}
        onDiscard={() => {
          editColors(basis, () => EMPTY_COLORS);
        }}
        onApply={openFlow}
        disabledReason={blockText(dict, block)}
      />
      {flowElement}
    </div>
  );
}
