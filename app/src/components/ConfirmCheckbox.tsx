// An explicit confirmation for a risky change (an override of one of the
// keyboard's essential Fn shortcuts, a change Undo can't bring back): a
// boxed checkbox in the danger (or warning) colour, with the consequence as
// its label. Nothing is ever pre-ticked; the caller keeps the state.

import { useId, type ReactNode } from "react";

import { Icon } from "./Icon";

/** Props of {@link ConfirmCheckbox}. */
export interface ConfirmCheckboxProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** The consequence being confirmed ("Fn + 2 will no longer switch to profile 2 …"). */
  label: ReactNode;
  /** More text under the label. */
  description?: ReactNode | undefined;
  tone?: "danger" | "warning" | undefined;
  disabled?: boolean | undefined;
}

/** A checkbox that confirms a consequence. */
export function ConfirmCheckbox({
  checked,
  onChange,
  label,
  description,
  tone = "danger",
  disabled = false,
}: ConfirmCheckboxProps) {
  const id = useId();
  return (
    <div
      className={`ow-confirm ow-confirm--${tone}${checked ? " ow-confirm--checked" : ""}${disabled ? " ow-confirm--disabled" : ""}`}
    >
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        aria-describedby={description === undefined ? undefined : `${id}-d`}
        onChange={(event) => {
          onChange(event.currentTarget.checked);
        }}
      />
      <div className="ow-confirm__text">
        <label htmlFor={id}>
          <Icon name="warning" size={16} className="ow-confirm__icon" />
          <span>{label}</span>
        </label>
        {description === undefined ? null : (
          <p id={`${id}-d`} className="ow-small">
            {description}
          </p>
        )}
      </div>
    </div>
  );
}
