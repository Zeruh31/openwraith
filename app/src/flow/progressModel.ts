// Folds an operation's progress events into a timeline of phases and the
// current step (pure; tested).

import type { CancelState, OpId, ProgressEvent, ProgressStep } from "../ipc/bindings";

/** One phase on the timeline (consecutive events with the same key). */
export interface ProgressPhase {
  key: string;
  /** The phase's latest step. */
  step: ProgressStep;
  /** The sequence number of the phase's first event. */
  seq: number;
}

/** The progress of one operation. */
export interface ProgressModel {
  op: OpId | null;
  phases: readonly ProgressPhase[];
  /** The latest event. */
  current: ProgressEvent | null;
  cancel: CancelState | null;
}

/** No progress yet. */
export const EMPTY_PROGRESS: ProgressModel = { op: null, phases: [], current: null, cancel: null };

/** What makes two events belong to the same phase. */
export function phaseKey(step: ProgressStep): string {
  switch (step.step) {
    case "reading":
      return `reading:${step.purpose}:${step.profile ?? "-"}`;
    case "selecting_profile":
    case "returning_to_profile":
    case "checking_persistence":
      return `${step.step}:${step.profile}`;
    case "writing":
      return `writing:${step.profile}`;
    case "waiting_for_keyboard_save":
      return `waiting:${step.profile ?? "-"}`;
    case "checking_other_programs":
    case "comparing":
    case "saving_safety_backup":
    case "saving_file":
    case "final_check":
    case "refreshing_view":
      return step.step;
  }
}

/** The model after one more event. */
export function reduceProgress(model: ProgressModel, event: ProgressEvent): ProgressModel {
  const key = phaseKey(event.step);
  const last = model.phases[model.phases.length - 1];
  const phases =
    last?.key === key
      ? [...model.phases.slice(0, -1), { key, step: event.step, seq: last.seq }]
      : [...model.phases, { key, step: event.step, seq: event.seq }];
  return { op: event.op, phases, current: event, cancel: event.cancel };
}
