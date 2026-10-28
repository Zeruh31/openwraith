// The macro recorder's logic: key events (`KeyboardEvent.code`, down or up,
// with the time they came) become macro steps through the HID usage table
// generated from the core (codes the keyboard can't send are skipped).
// Auto-repeats are ignored, a press of a key already held and a release of a
// key pressed before the recording are ignored too. The wait after each step
// follows the delay mode: as typed (the time to the next event), fixed, or
// none. A press is taken only while its release (and the releases of every
// key held) still fit in the step limit, so a recording can always be
// finished. The component stops the recording 2 s after the last key.

import type { MacroStepView } from "../../../ipc/bindings";
import { KEY_CODE_USAGES } from "../../../generated/hidCodes";

/** How the recorder sets the wait after each step. */
export type DelayMode = "typed" | "fixed" | "none";

/** The recorder's settings. */
export interface RecorderOptions {
  mode: DelayMode;
  /** The wait of the fixed mode (ms). */
  fixedMs: number;
  maxSteps: number;
  maxDelayMs: number;
}

/** What the recorder has so far. */
export interface RecorderState {
  steps: MacroStepView[];
  /** Keys pressed during the recording and not released yet (in press order). */
  held: number[];
  /** When the last step came (ms). */
  lastAt: number | null;
  /** Key events the keyboard couldn't send (codes outside the table). */
  skipped: number;
  /** A press was refused: no room left for more presses. */
  full: boolean;
}

/** A recording that has just started. */
export const RECORDER_START: RecorderState = { steps: [], held: [], lastAt: null, skipped: 0, full: false };

/** A key event as the recorder sees it. */
export interface KeyInput {
  /** `KeyboardEvent.code`. */
  code: string;
  down: boolean;
  /** An auto-repeat (`KeyboardEvent.repeat`). */
  repeat: boolean;
  /** When it came (ms, any clock). */
  at: number;
}

/** The recording stops this long after the last key, when no key is held (ms). */
export const IDLE_STOP_MS = 2000;

/** The HID usage of a `KeyboardEvent.code`, or `null` when the keyboard can't send it. */
export function usageOfCode(code: string): number | null {
  return Object.hasOwn(KEY_CODE_USAGES, code) ? (KEY_CODE_USAGES[code] ?? null) : null;
}

function waitBefore(state: RecorderState, at: number, options: RecorderOptions): number {
  switch (options.mode) {
    case "none":
      return 0;
    case "fixed":
      return Number.isFinite(options.fixedMs) ? Math.min(options.maxDelayMs, Math.max(0, Math.round(options.fixedMs))) : 0;
    case "typed":
      return state.lastAt === null ? 0 : Math.min(options.maxDelayMs, Math.max(0, Math.round(at - state.lastAt)));
  }
}

/** The recorder after a key event (the same state when the event is ignored). */
export function record(state: RecorderState, input: KeyInput, options: RecorderOptions): RecorderState {
  if (input.repeat) return state;
  const usage = usageOfCode(input.code);
  // A key the keyboard can't send counts once (its press; the release is ignored).
  if (usage === null) return input.down ? { ...state, skipped: state.skipped + 1 } : state;
  const heldAt = state.held.indexOf(usage);
  if (input.down) {
    if (heldAt >= 0) return state;
    // This press, its release and the releases of every key held must fit.
    if (state.steps.length + state.held.length + 2 > options.maxSteps) return state.full ? state : { ...state, full: true };
  } else if (heldAt < 0) {
    return state;
  }
  const wait = waitBefore(state, input.at, options);
  const steps = state.steps.map((step, index) =>
    index === state.steps.length - 1 ? { ...step, delay_after_ms: wait } : step,
  );
  steps.push({ usage, motion: input.down ? "press" : "release", delay_after_ms: 0 });
  const held = input.down ? [...state.held, usage] : state.held.filter((_, index) => index !== heldAt);
  return { ...state, steps, held, lastAt: input.at };
}
