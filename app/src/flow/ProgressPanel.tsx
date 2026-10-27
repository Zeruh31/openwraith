// The progress of a running operation: a step timeline, the current step's
// details (counts, the keyboard-save wait), the Stop control and the
// "keep it plugged in" note.

import { Button } from "../components/Button";
import { Countdown, useElapsed } from "../components/Countdown";
import { ProgressBar, Spinner } from "../components/Feedback";
import { Icon } from "../components/Icon";
import { keyName, useDict } from "../i18n";
import { useFormat } from "../i18n/format";
import type { CancelState, ProgressEvent } from "../ipc/bindings";
import { stepLabel, tableName } from "./labels";
import type { ProgressModel, ProgressPhase } from "./progressModel";

/** Props of {@link CommitWaitMeter}. */
export interface CommitWaitMeterProps {
  /** How long OpenWraith waits for the keyboard to save. */
  waitMs: number;
  /** The profile being saved, if known. */
  profile: number | null;
}

/**
 * "Waiting for the keyboard to save profile N…": a bar over the wait with a
 * countdown and the reason. Remount (new `key`) for every new wait. With
 * reduced motion only the text is shown.
 */
export function CommitWaitMeter({ waitMs, profile }: CommitWaitMeterProps) {
  const dict = useDict();
  const format = useFormat();
  const elapsed = useElapsed(100, waitMs > 0);
  const done = Math.min(elapsed, waitMs);
  const left = format.seconds(Math.max(0, waitMs - elapsed));
  const title = profile === null ? dict.flow.progress.waitGeneric : dict.flow.progress.waitProfile({ n: profile });
  const ratio = waitMs > 0 ? done / waitMs : 1;
  return (
    <div className="ow-meter">
      <p className="ow-meter__title">
        <Icon name="lock" size={16} />
        <span>{title}</span>
      </p>
      <div
        className="ow-meter__track"
        role="progressbar"
        aria-label={title}
        aria-valuemin={0}
        aria-valuemax={waitMs}
        aria-valuenow={Math.round(done)}
        aria-valuetext={dict.flow.progress.waitLeft({ seconds: left })}
      >
        <div className="ow-meter__bar" style={{ width: `${ratio * 100}%` }} />
      </div>
      <p className="ow-meter__left ow-num">{dict.flow.progress.waitLeft({ seconds: left })}</p>
      <p className="ow-muted ow-small">{dict.flow.progress.waitExplain}</p>
    </div>
  );
}

/** The phases so far: done ones ticked, the last one in progress. */
export function StepTimeline({ phases, finished = false }: { phases: readonly ProgressPhase[]; finished?: boolean }) {
  const dict = useDict();
  if (phases.length === 0) {
    return null;
  }
  return (
    <ol className="ow-timeline" aria-label={dict.flow.progress.steps}>
      {phases.map((phase, index) => {
        const current = !finished && index === phases.length - 1;
        return (
          <li key={`${phase.key}-${phase.seq}`} className={`ow-timeline__item${current ? " ow-timeline__item--current" : ""}`}>
            <span className="ow-timeline__mark">
              {current ? <Spinner size={14} /> : <Icon name="check" size={14} />}
            </span>
            <span>{stepLabel(phase.step, dict)}</span>
            <span className="sr-only">{current ? dict.flow.progress.current : dict.flow.progress.done}</span>
          </li>
        );
      })}
    </ol>
  );
}

/** The details of the current step. */
export function CurrentStep({ event }: { event: ProgressEvent }) {
  const dict = useDict();
  const progress = dict.flow.progress;
  const step = event.step;
  switch (step.step) {
    case "reading":
      return (
        <div className="ow-current-step">
          <ProgressBar
            value={step.done}
            max={step.total}
            label={stepLabel(step, dict)}
            valueText={progress.count({ done: step.done, total: step.total })}
          />
          <p className="ow-muted ow-small ow-num">{progress.count({ done: step.done, total: step.total })}</p>
        </div>
      );
    case "writing": {
      const what = step.key === null ? tableName(step.table, dict) : keyName(dict, step.key.name);
      return (
        <div className="ow-current-step">
          {step.items > 0 ? (
            <ProgressBar
              value={step.item}
              max={step.items}
              label={stepLabel(step, dict)}
              valueText={progress.item({ what, item: step.item, items: step.items })}
            />
          ) : null}
          <p className="ow-num">
            {step.items > 0 ? progress.item({ what, item: step.item, items: step.items }) : what}
          </p>
          {step.packets > 1 ? (
            <p className="ow-muted ow-small ow-num">{progress.packet({ packet: step.packet + 1, packets: step.packets })}</p>
          ) : null}
          {step.send >= 2 ? <p className="ow-small ow-warning-text">{progress.resend}</p> : null}
        </div>
      );
    }
    case "waiting_for_keyboard_save":
      return <CommitWaitMeter key={event.seq} waitMs={step.wait_ms} profile={step.profile} />;
    case "checking_other_programs":
      return (
        <p className="ow-current-step ow-muted" key={event.seq}>
          <Countdown remainingMs={step.remaining_ms} render={(seconds) => progress.checkingOthers({ seconds })} />
        </p>
      );
    case "selecting_profile":
    case "returning_to_profile":
    case "comparing":
    case "saving_safety_backup":
    case "saving_file":
    case "checking_persistence":
    case "final_check":
    case "refreshing_view":
      return null;
  }
}

/** Props of {@link StopControl}. */
export interface StopControlProps {
  cancel: CancelState | null;
  /** Stop was clicked here and no newer state arrived yet. */
  requested: boolean;
  onStop: () => void;
}

/** The Stop button, or why stopping isn't possible now. */
export function StopControl({ cancel, requested, onStop }: StopControlProps) {
  const dict = useDict();
  const state: CancelState = requested && (cancel === "available" || cancel === null) ? "requested" : (cancel ?? "not_now");
  if (state === "available") {
    return (
      <Button icon="stop" onClick={onStop}>
        {dict.flow.cancel.available}
      </Button>
    );
  }
  if (state === "not_cancellable") {
    return <p className="ow-muted ow-small">{dict.flow.cancel.not_cancellable}</p>;
  }
  return (
    <div className="ow-stop">
      <Button icon="stop" disabled>
        {dict.flow.cancel.available}
      </Button>
      <p className="ow-muted ow-small" aria-live="polite">
        {dict.flow.cancel[state]}
      </p>
    </div>
  );
}

/** Props of {@link ProgressPanel}. */
export interface ProgressPanelProps {
  model: ProgressModel;
  /** Shown before the first event. */
  waitingText?: string | undefined;
  /** Called by Stop; without it no Stop control is shown. */
  onStop?: (() => void) | undefined;
  stopRequested?: boolean | undefined;
}

/**
 * What the live region says: the current phase's name, nothing finer.
 * Counters and countdowns change many times a second; they are read on
 * demand (the bars' value texts), so only a new phase is announced.
 */
function phaseAnnouncement(model: ProgressModel, dict: ReturnType<typeof useDict>): string {
  const phase = model.phases[model.phases.length - 1];
  return phase === undefined ? "" : stepLabel(phase.step, dict);
}

/** The progress of a running operation. */
export function ProgressPanel({ model, waitingText, onStop, stopRequested = false }: ProgressPanelProps) {
  const dict = useDict();
  return (
    <div className="ow-progress-panel">
      {model.current === null ? (
        <p className="ow-row">
          <Spinner />
          <span>{waitingText ?? dict.shell.busy.working}</span>
        </p>
      ) : (
        <>
          <StepTimeline phases={model.phases} />
          <p className="sr-only" aria-live="polite" data-testid="progress-live">
            {phaseAnnouncement(model, dict)}
          </p>
          <CurrentStep event={model.current} />
        </>
      )}
      <p className="ow-progress-panel__note">
        <Icon name="plug" size={16} />
        <span>{dict.flow.progress.keepPlugged}</span>
      </p>
      {onStop === undefined ? null : (
        <StopControl cancel={model.cancel} requested={stopRequested} onStop={onStop} />
      )}
    </div>
  );
}
