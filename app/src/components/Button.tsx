// Buttons. A disabled button can show why it is disabled (`disabledReason`),
// as text below it, so that nothing is disabled without an explanation.

import { useId, type ButtonHTMLAttributes, type ReactNode, type Ref } from "react";

import { Icon, type IconName } from "./Icon";

/** The look of a button. */
export type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";

/** Props of {@link Button}. */
export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "type"> {
  variant?: ButtonVariant | undefined;
  size?: "md" | "sm" | undefined;
  /** An icon before the label. */
  icon?: IconName | undefined;
  /**
   * When set, the button is disabled and this text is shown below it. Pass
   * `null` (not `undefined`) while there is no reason, so the button keeps
   * its identity when a reason appears.
   */
  disabledReason?: string | null | undefined;
  /** `submit` only inside forms; default `button`. */
  type?: "button" | "submit" | undefined;
  children?: ReactNode | undefined;
  ref?: Ref<HTMLButtonElement> | undefined;
}

/** A button with an optional icon and a visible disabled reason. */
export function Button({
  variant = "secondary",
  size = "md",
  icon,
  disabledReason,
  disabled,
  className,
  type = "button",
  children,
  ref,
  ...rest
}: ButtonProps) {
  const reasonId = useId();
  // A caller that passes `disabledReason` (even `null`) always gets the
  // wrapper, so the button keeps its identity (and focus) when the reason
  // comes and goes.
  const wrapped = disabledReason !== undefined;
  const hasReason = disabledReason !== undefined && disabledReason !== null && disabledReason !== "";
  const classes = ["ow-button", `ow-button--${variant}`, `ow-button--${size}`, className]
    .filter(Boolean)
    .join(" ");
  const button = (
    <button
      {...rest}
      ref={ref}
      type={type}
      className={classes}
      disabled={disabled === true || hasReason}
      aria-describedby={hasReason ? reasonId : rest["aria-describedby"]}
    >
      {icon === undefined ? null : <Icon name={icon} size={size === "sm" ? 16 : 18} />}
      {children === undefined ? null : <span className="ow-button__label">{children}</span>}
    </button>
  );
  if (!wrapped) {
    return button;
  }
  return (
    <span className="ow-button-wrap">
      {button}
      {hasReason ? (
        <span id={reasonId} className="ow-button__reason">
          {disabledReason}
        </span>
      ) : null}
    </span>
  );
}

/** Props of {@link IconButton}. */
export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "type" | "children"> {
  icon: IconName;
  /** The accessible name (also the tooltip). */
  label: string;
  variant?: ButtonVariant | undefined;
  size?: "md" | "sm" | undefined;
  ref?: Ref<HTMLButtonElement> | undefined;
}

/** A square button with only an icon; `label` is its accessible name and tooltip. */
export function IconButton({ icon, label, variant = "ghost", size = "md", className, ref, ...rest }: IconButtonProps) {
  const classes = ["ow-button", "ow-button--icon", `ow-button--${variant}`, `ow-button--${size}`, className]
    .filter(Boolean)
    .join(" ");
  return (
    <button {...rest} ref={ref} type="button" className={classes} aria-label={label} title={label}>
      <Icon name={icon} size={size === "sm" ? 16 : 18} />
    </button>
  );
}
