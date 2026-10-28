// The macro recorder: Record turns the recording area into a key capture
// (`role="application"`, so screen readers pass the keys through); every key
// event there becomes a step (see recorder.ts) and replaces the macro's
// steps as it goes. The keys don't reach the rest of the app while it
// records (Tab, Space and F5 are recorded, not acted on). It stops on Stop,
// 2 s after the last key when no key is held, or when the macro is full and
// every key is up. The state is announced in a polite live region. Nothing
// is written to the keyboard: the result is part of the draft.

import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";

import { Button } from "../../../components/Button";
import { SegmentedControl } from "../../../components/Controls";
import { useDict } from "../../../i18n";
import type { MacroStepView } from "../../../ipc/bindings";
import type { MacroLimits } from "./model";
import { validDelay } from "./model";
import { IDLE_STOP_MS, record, RECORDER_START, type DelayMode, type RecorderState } from "./recording";

/** Props of {@link Recorder}. */
export interface RecorderProps {
  /** Replaces the macro's steps (called as the recording goes). */
  onSteps: (steps: MacroStepView[]) => void;
  limits: MacroLimits;
  delayMode: DelayMode;
  onDelayMode: (mode: DelayMode) => void;
  fixedMs: number;
  onFixedMs: (ms: number) => void;
  disabled: boolean;
}

const MODES: readonly DelayMode[] = ["typed", "fixed", "none"];

/** The recorder of a macro's steps. */
export function Recorder({ onSteps, limits, delayMode, onDelayMode, fixedMs, onFixedMs, disabled }: RecorderProps) {
  const dict = useDict();
  const t = dict.keysMacros.recorder;
  const id = useId();
  const [recording, setRecording] = useState(false);
  const [focused, setFocused] = useState(false);
  const [announcement, setAnnouncement] = useState("");
  const [skipped, setSkipped] = useState(0);
  const area = useRef<HTMLDivElement>(null);
  const startButton = useRef<HTMLButtonElement>(null);
  const machine = useRef<RecorderState>(RECORDER_START);
  const timer = useRef<number | null>(null);
  const fixedValid = validDelay(fixedMs, limits);
  const startReason = delayMode === "fixed" && !fixedValid ? t.fixedInvalid({ max: limits.maxDelayMs }) : null;

  const clearTimer = () => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
  };

  const stop = (full = false) => {
    clearTimer();
    setRecording(false);
    const n = machine.current.steps.length;
    setAnnouncement(full ? `${t.full} ${t.stopped({ n })}` : t.stopped({ n }));
  };

  // A recording stops when the editor goes away.
  useEffect(() => {
    const pending = timer;
    return () => {
      if (pending.current !== null) window.clearTimeout(pending.current);
    };
  }, []);

  // The capture area takes the focus when the recording starts; when it
  // stops, the Record button gets it back (unless the focus went elsewhere).
  const wasRecording = useRef(false);
  useEffect(() => {
    if (recording) {
      area.current?.focus();
    } else if (wasRecording.current) {
      const active = document.activeElement;
      if (active === null || active === document.body || area.current?.contains(active) === true) startButton.current?.focus();
    }
    wasRecording.current = recording;
  }, [recording]);

  const start = () => {
    if (disabled || startReason !== null) return;
    machine.current = RECORDER_START;
    setSkipped(0);
    onSteps([]);
    setRecording(true);
    setAnnouncement(t.started);
  };

  const armIdleStop = () => {
    clearTimer();
    timer.current = window.setTimeout(() => {
      timer.current = null;
      if (machine.current.held.length === 0) stop();
    }, IDLE_STOP_MS);
  };

  const onKey = (event: KeyboardEvent<HTMLDivElement>, down: boolean) => {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    const before = machine.current;
    const next = record(
      before,
      { code: event.code, down, repeat: event.repeat, at: Date.now() },
      { mode: delayMode, fixedMs, maxSteps: limits.maxSteps, maxDelayMs: limits.maxDelayMs },
    );
    machine.current = next;
    if (next.steps !== before.steps) onSteps(next.steps);
    if (next.skipped !== before.skipped) setSkipped(next.skipped);
    if (next.full && next.held.length === 0) {
      stop(true);
      return;
    }
    if (!event.repeat) armIdleStop();
  };

  let state: string = t.idle;
  if (recording) state = focused ? t.recording : t.paused;

  return (
    <div className="keys-macros__recorder ow-stack">
      <div className="keys-macros__recorder-settings">
        <div className="keys-macros__field">
          <span className="ow-field__label" aria-hidden="true">
            {t.delayMode}
          </span>
          <SegmentedControl<DelayMode>
            label={t.delayMode}
            size="sm"
            options={MODES.map((mode) => ({ value: mode, label: t.modes[mode] }))}
            value={delayMode}
            onChange={onDelayMode}
            disabled={recording}
          />
        </div>
        {delayMode === "fixed" ? (
          <div className="keys-macros__field">
            <label htmlFor={`${id}-fixed`} className="ow-field__label">
              {t.fixedMs}
            </label>
            <input
              id={`${id}-fixed`}
              type="number"
              className="ow-input ow-num keys-macros__delay-input"
              min={0}
              max={limits.maxDelayMs}
              step={1}
              value={Number.isNaN(fixedMs) ? "" : fixedMs}
              aria-invalid={!fixedValid || undefined}
              disabled={recording}
              onChange={(event) => {
                const text = event.currentTarget.value;
                onFixedMs(text === "" ? Number.NaN : Number(text));
              }}
            />
          </div>
        ) : null}
      </div>
      <div className="ow-row">
        {recording ? (
          <Button
            icon="stop"
            variant="danger"
            onClick={() => {
              stop();
            }}
          >
            {t.stop}
          </Button>
        ) : (
          <Button ref={startButton} icon="keys" disabled={disabled} disabledReason={startReason} onClick={start}>
            {t.start}
          </Button>
        )}
        <span className="ow-small ow-muted">{t.replaces}</span>
      </div>
      <div
        ref={area}
        className={`keys-macros__capture${recording ? " keys-macros__capture--on" : ""}`}
        role="application"
        aria-label={t.area}
        aria-describedby={`${id}-state`}
        tabIndex={recording ? 0 : -1}
        onKeyDown={(event) => {
          onKey(event, true);
        }}
        onKeyUp={(event) => {
          onKey(event, false);
        }}
        onFocus={() => {
          setFocused(true);
        }}
        onBlur={() => {
          setFocused(false);
        }}
        onClick={() => {
          if (recording) area.current?.focus();
        }}
      >
        <span id={`${id}-state`}>{state}</span>
      </div>
      {skipped === 0 ? null : <p className="ow-small ow-warning-text">{t.skipped({ n: skipped })}</p>}
      <ul className="keys-macros__notes ow-small ow-muted">
        <li>{t.repeats}</li>
        <li>{t.windowsKeys}</li>
        <li>{t.remapped}</li>
      </ul>
      <p className="sr-only" role="status" aria-live="polite">
        {announcement}
      </p>
    </div>
  );
}
