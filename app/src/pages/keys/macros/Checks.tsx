// The live checks of a macro (the library's rules, checked while editing):
// every key released by the end (with the "Release all at end" fix), at most
// 40 steps, waits of 0-65535 ms, a repeat count of 1-255, and a key for a new
// macro. Shown as they arise; the review can't be opened while any is left.

import { usageName } from "../../../components/actions";
import { Button } from "../../../components/Button";
import { Callout } from "../../../components/Feedback";
import { useDict, type Dict } from "../../../i18n";
import { useFormat, type Format } from "../../../i18n/format";
import type { MacroStepView } from "../../../ipc/bindings";
import { releaseAll, type MacroLimits, type MacroProblem } from "./model";

/** A check in words (step numbers as the editor shows them, from 1). */
export function problemText(problem: MacroProblem, limits: MacroLimits, dict: Dict, format: Format): string {
  const t = dict.keysMacros.checks;
  switch (problem.kind) {
    case "no_steps":
      return t.no_steps;
    case "too_long":
      return t.too_long({ steps: problem.steps, max: problem.max });
    case "released_unpressed":
      return t.released_unpressed({ n: problem.step + 1, key: usageName(problem.usage, dict) });
    case "pressed_twice":
      return t.pressed_twice({ n: problem.step + 1, key: usageName(problem.usage, dict) });
    case "held":
      return t.held({ keys: format.list(problem.usages.map((usage) => usageName(usage, dict))), n: problem.usages.length });
    case "delay":
      return t.delay({ n: problem.step + 1, max: limits.maxDelayMs });
    case "times":
      return t.times({ max: limits.maxTimes });
  }
}

/** Props of {@link Checks}. */
export interface ChecksProps {
  problems: readonly MacroProblem[];
  needsKey: boolean;
  steps: readonly MacroStepView[];
  limits: MacroLimits;
  onSteps: (steps: MacroStepView[]) => void;
  disabled: boolean;
}

/** The checks that keep a macro from being written, with their fixes. */
export function Checks({ problems, needsKey, steps, limits, onSteps, disabled }: ChecksProps) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.keysMacros.checks;
  if (problems.length === 0 && !needsKey) return null;
  const held = problems.some((problem) => problem.kind === "held");
  const fixed = held ? releaseAll(steps, limits) : null;
  return (
    <Callout
      kind="warning"
      title={t.title}
      actions={
        held ? (
          <Button
            size="sm"
            disabled={disabled}
            disabledReason={fixed === null ? t.releaseAllNoRoom : null}
            onClick={() => {
              if (fixed !== null) onSteps(fixed);
            }}
          >
            {t.releaseAll}
          </Button>
        ) : undefined
      }
    >
      <ul className="keys-macros__checks">
        {needsKey ? <li>{t.needsKey}</li> : null}
        {problems.map((problem, index) => (
          <li key={`${problem.kind}-${index}`}>{problemText(problem, limits, dict, format)}</li>
        ))}
      </ul>
    </Callout>
  );
}
