// The macro editor (one keyboard slot or a new macro): the key that runs it
// (required for a new macro, optional for a slot no key uses, shown for a
// slot that has keys), how it plays (the four play modes with the library's
// evidence), its steps (simple or detailed view), the recorder, the live
// checks (once it has changes), copying a library macro's steps, saving it
// to the library, and undoing its changes. Every edit changes only the tab's
// draft.

import { useEffect, useId, useRef, useState } from "react";

import { ACTION_CATALOG } from "../../../components/actions";
import { EvidenceBadge } from "../../../components/Badge";
import { Button, IconButton } from "../../../components/Button";
import { Checkbox, RadioGroup } from "../../../components/Controls";
import { Callout } from "../../../components/Feedback";
import { TechnicalDetails } from "../../../components/Layout";
import { useDict } from "../../../i18n";
import type {
  KeysState,
  LibraryMacroView,
  MacroPlayKindView,
  MacroPlayView,
  MacroProgramView,
  MacroStepView,
} from "../../../ipc/bindings";
import { bindingsText, type LabelContext } from "../shared";
import { Checks } from "./Checks";
import type { StepsView } from "./draftStore";
import { KeyBinding } from "./KeyBinding";
import {
  bindOf,
  canBind,
  chosenBindings,
  editedSlots,
  editOf,
  macroProblems,
  ownMacroSlot,
  programOf,
  setAck,
  setBind,
  setProgram,
  slotState,
  withSteps,
  type MacroLimits,
  type MacrosDraft,
  type Target,
} from "./model";
import { Recorder } from "./Recorder";
import type { DelayMode } from "./recording";
import { StepsEditor } from "./StepsEditor";

const PLAY_MODES: readonly MacroPlayKindView[] = ["once", "times", "toggle", "while_held"];

/** The play value of a mode (a new "times" plays twice, or the count it had). */
function playOf(mode: MacroPlayKindView, current: MacroPlayView): MacroPlayView {
  if (mode === "times") return current.mode === "times" ? current : { mode: "times", count: 2 };
  return { mode };
}

/** Props of {@link MacroEditor}. */
export interface MacroEditorProps {
  state: KeysState;
  ctx: LabelContext;
  target: Target;
  /** The editor's title ("Macro 3 · Type A", "New macro"). */
  name: string;
  draft: MacrosDraft;
  onDraft: (draft: MacrosDraft) => void;
  limits: MacroLimits;
  view: StepsView;
  onView: (view: StepsView) => void;
  delayMode: DelayMode;
  onDelayMode: (mode: DelayMode) => void;
  fixedMs: number;
  onFixedMs: (ms: number) => void;
  library: readonly LibraryMacroView[];
  onSaveToLibrary: (program: MacroProgramView) => void;
  /** Undo the slot's changes, or drop the new macro. */
  onRevert: () => void;
  /** Keep a slot the draft deletes. */
  onKeep: () => void;
  onClose: () => void;
  /** Editing is possible (the keys state is editable). */
  editable: boolean;
  /** Take the focus on open (the user opened it). */
  autoFocus: boolean;
}

/** The editor of one macro. */
export function MacroEditor({
  state,
  ctx,
  target,
  name,
  draft,
  onDraft,
  limits,
  view,
  onView,
  delayMode,
  onDelayMode,
  fixedMs,
  onFixedMs,
  library,
  onSaveToLibrary,
  onRevert,
  onKeep,
  onClose,
  editable,
  autoFocus,
}: MacroEditorProps) {
  const dict = useDict();
  const t = dict.keysMacros.editor;
  const id = useId();
  const region = useRef<HTMLElement>(null);
  const slot = target.kind === "slot" ? slotState(state, target.slot) : undefined;
  const edit = target.kind === "slot" ? editOf(draft, target.slot) : undefined;
  const program = programOf(draft, state, target);
  const bind = bindOf(draft, target);
  const [bindOpen, setBindOpen] = useState(bind !== null);
  const problems = macroProblems(program, limits);
  const disabled = !editable;
  const bindable = target.kind === "new" || canBind(slot);
  const keptHex = slot?.content.state === "kept_as_read" && (edit?.program ?? null) === null ? slot.content.hex : null;
  const changed = target.kind === "new" || edit !== undefined;

  // The editor takes the focus when the user opened it (it may be far from the list),
  // with its title and key at the top of the view (it is taller than the window).
  useEffect(() => {
    const element = region.current;
    if (!autoFocus || element === null) return;
    element.focus({ preventScroll: true });
    const reduce = typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (typeof element.scrollIntoView === "function") element.scrollIntoView({ block: "start", behavior: reduce ? "auto" : "smooth" });
  }, [autoFocus]);

  const update = (next: MacroProgramView) => {
    onDraft(setProgram(draft, state, target, next));
  };
  const onSteps = (steps: MacroStepView[]) => {
    update(withSteps(program, steps));
  };

  const header = (
    <div className="ow-card__header">
      <h2 className="ow-card__title">{name}</h2>
      <IconButton icon="close" size="sm" label={t.close} onClick={onClose} />
    </div>
  );

  if (edit?.remove === true) {
    return (
      <section ref={region} className="keys-macros__editor ow-card" aria-label={`${t.label}: ${name}`} tabIndex={-1}>
        {header}
        <div className="ow-card__body">
          <Callout
            kind="info"
            actions={
              <Button size="sm" onClick={onKeep}>
                {dict.keysMacros.slots.labels.keep}
              </Button>
            }
          >
            {slot !== undefined && slot.bindings.length > 0 ? t.removedBound : t.removedOrphan}
          </Callout>
        </div>
      </section>
    );
  }

  const evidenceOf = (mode: MacroPlayKindView) => ACTION_CATALOG.macros.play_modes.find((rule) => rule.mode === mode)?.evidence;
  const count = program.play.mode === "times" ? program.play.count : null;

  return (
    <section ref={region} className="keys-macros__editor ow-card" aria-label={`${t.label}: ${name}`} tabIndex={-1}>
      {header}
      <div className="ow-card__body keys-macros__editor-body">
        {target.kind === "slot" && target.slot === 0 ? (
          <Callout kind="info">
            <p>{dict.keysMacros.slots.slot0.note}</p>
          </Callout>
        ) : null}
        {keptHex === null ? null : (
          <Callout kind="warning">
            <p>{t.keptAsRead}</p>
            <TechnicalDetails>{keptHex}</TechnicalDetails>
          </Callout>
        )}

        <section className="keys-macros__section" aria-labelledby={`${id}-bind`}>
          <h3 id={`${id}-bind`} className="keys-macros__section-title">
            {t.bindTitle}
          </h3>
          {!bindable && slot !== undefined ? (
            <p className="ow-small">{t.boundTo({ keys: bindingsText(slot.bindings, dict) })}</p>
          ) : target.kind === "new" ? (
            <>
              <p className="ow-small ow-muted">{t.bindNew}</p>
              <KeyBinding
                state={state}
                ctx={ctx}
                value={bind}
                chosen={chosenBindings(draft, target)}
                editedSlots={editedSlots(draft)}
                acks={draft.acks}
                disabled={disabled}
                onChange={(next) => {
                  onDraft(setBind(draft, state, target, next));
                }}
                onAck={(key, confirmed) => {
                  onDraft(setAck(draft, key, confirmed));
                }}
              />
              {/* The key runs Macro 1 alone: the library reuses it, and writing it empties Macros 2–10. */}
              {ownMacroSlot(state, draft, bind) === 0 ? (
                <Callout kind="info">
                  <p>{dict.keysMacros.slots.slot0.reuses}</p>
                </Callout>
              ) : null}
            </>
          ) : (
            <>
              <Checkbox
                checked={bindOpen || bind !== null}
                label={t.bindOptional}
                description={t.bindLater}
                disabled={disabled}
                onChange={(checked) => {
                  setBindOpen(checked);
                  if (!checked) onDraft(setBind(draft, state, target, null));
                }}
              />
              {bindOpen || bind !== null ? (
                <KeyBinding
                  state={state}
                  ctx={ctx}
                  value={bind}
                  chosen={chosenBindings(draft, target)}
                  acks={draft.acks}
                  disabled={disabled}
                  onChange={(next) => {
                    onDraft(setBind(draft, state, target, next));
                  }}
                  onAck={(key, confirmed) => {
                    onDraft(setAck(draft, key, confirmed));
                  }}
                />
              ) : null}
            </>
          )}
        </section>

        <section className="keys-macros__section" aria-labelledby={`${id}-play`}>
          <h3 id={`${id}-play`} className="keys-macros__section-title">
            {t.playTitle}
          </h3>
          <RadioGroup<MacroPlayKindView>
            legend={t.playTitle}
            hideLegend
            disabled={disabled}
            value={program.play.mode}
            options={PLAY_MODES.map((mode) => ({
              value: mode,
              label: (
                <span className="keys-macros__play">
                  <span>{t.play[mode]}</span>
                  <EvidenceBadge evidence={evidenceOf(mode)} size="sm" />
                </span>
              ),
              description: t.playHint[mode],
            }))}
            onChange={(mode) => {
              update({ ...program, play: playOf(mode, program.play) });
            }}
          />
          {count === null ? null : (
            <div className="keys-macros__field">
              <label htmlFor={`${id}-times`} className="ow-field__label">
                {t.times}
              </label>
              <input
                id={`${id}-times`}
                type="number"
                className="ow-input ow-num keys-macros__delay-input"
                min={1}
                max={limits.maxTimes}
                step={1}
                value={Number.isNaN(count) ? "" : count}
                disabled={disabled}
                aria-describedby={`${id}-times-hint`}
                onChange={(event) => {
                  const text = event.currentTarget.value;
                  update({ ...program, play: { mode: "times", count: text === "" ? Number.NaN : Number(text) } });
                }}
              />
              <p id={`${id}-times-hint`} className="ow-small ow-muted">
                {t.timesHint({ max: limits.maxTimes })}
              </p>
            </div>
          )}
        </section>

        <section className="keys-macros__section" aria-labelledby={`${id}-steps`}>
          <h3 id={`${id}-steps`} className="keys-macros__section-title">
            {t.stepsTitle}
          </h3>
          <StepsEditor steps={program.steps} onSteps={onSteps} view={view} onView={onView} limits={limits} disabled={disabled} />
        </section>

        <section className="keys-macros__section" aria-labelledby={`${id}-record`}>
          <h3 id={`${id}-record`} className="keys-macros__section-title">
            {dict.keysMacros.recorder.title}
          </h3>
          <Recorder
            onSteps={onSteps}
            limits={limits}
            delayMode={delayMode}
            onDelayMode={onDelayMode}
            fixedMs={fixedMs}
            onFixedMs={onFixedMs}
            disabled={disabled}
          />
        </section>

        {changed ? (
          <Checks
            problems={problems}
            needsKey={target.kind === "new" && bind === null}
            steps={program.steps}
            limits={limits}
            onSteps={onSteps}
            disabled={disabled}
          />
        ) : null}

        <div className="keys-macros__editor-foot">
          {library.length === 0 ? null : (
            <div className="keys-macros__field">
              <label htmlFor={`${id}-lib`} className="ow-field__label">
                {t.fromLibrary}
              </label>
              <select
                id={`${id}-lib`}
                className="ow-input"
                value=""
                disabled={disabled}
                onChange={(event) => {
                  const entry = library.find((item) => item.content_hash === event.currentTarget.value);
                  if (entry !== undefined) update(entry.program);
                }}
              >
                <option value="">{t.fromLibraryNone}</option>
                {library.map((entry) => (
                  <option key={entry.content_hash} value={entry.content_hash}>
                    {entry.name}
                  </option>
                ))}
              </select>
            </div>
          )}
          <div className="ow-row">
            <Button
              size="sm"
              disabledReason={problems.length === 0 ? null : t.saveInvalid}
              onClick={() => {
                onSaveToLibrary(program);
              }}
            >
              {t.saveToLibrary}
            </Button>
            {target.kind === "new" ? (
              <Button size="sm" variant="ghost" icon="trash" onClick={onRevert}>
                {t.dropNew}
              </Button>
            ) : changed ? (
              <Button size="sm" variant="ghost" icon="undo" onClick={onRevert}>
                {t.undo}
              </Button>
            ) : null}
          </div>
        </div>
      </div>
    </section>
  );
}
