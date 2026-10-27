// The W75 drawing of this page: the shared KeyboardView plus the page's two
// marks: a flag on the keys whose settings differ from the basic setting
// (`data-flag`, styled in actuation.css) and a tooltip with the reason on
// keys that can't be changed.

import { useCallback } from "react";

import { KeyboardView, type KeyboardViewProps, type KeyVisual } from "../../components/keyboard";

/** Props of {@link ActuationKeyboard}. */
export interface ActuationKeyboardProps extends KeyboardViewProps {
  /** Keys that differ from the basic setting. */
  changed: ReadonlySet<number>;
  /** Tooltips by key index (why a key can't be changed). */
  tooltips: ReadonlyMap<number, string>;
}

/** The keyboard with the page's marks. */
export function ActuationKeyboard({ changed, tooltips, keyVisual, ...props }: ActuationKeyboardProps) {
  const withMarks = useCallback(
    (index: number): KeyVisual => ({
      ...keyVisual(index),
      flag: changed.has(index),
      tooltip: tooltips.get(index) ?? null,
    }),
    [keyVisual, changed, tooltips],
  );

  return (
    <div className="act-board">
      <KeyboardView {...props} keyVisual={withMarks} />
    </div>
  );
}
