// Modal dialogs on the native <dialog> element (showModal): focus is
// trapped by the browser and returned to the opener on close.

import { useEffect, useId, useRef, useState, type ReactNode, type RefObject } from "react";

import { useDict } from "../i18n";
import { Button, IconButton } from "./Button";

/** Props of {@link Dialog}. */
export interface DialogProps {
  open: boolean;
  title: ReactNode;
  /** Called on Esc and the close button when `dismissable`. */
  onClose: () => void;
  /** Esc and the × button close it (default true). */
  dismissable?: boolean | undefined;
  size?: "sm" | "md" | "lg" | undefined;
  /** The action row (sticky at the bottom). */
  actions?: ReactNode | undefined;
  /**
   * A block between the scrolling body and the action row that never
   * scrolls away (notes and confirmations the action depends on).
   */
  footer?: ReactNode | undefined;
  /** The accessible name of the × button (default: close the dialog). */
  closeLabel?: string | undefined;
  /** The element focused on open (default: the first focusable one). */
  initialFocus?: RefObject<HTMLElement | null> | undefined;
  className?: string | undefined;
  children?: ReactNode | undefined;
}

function show(dialog: HTMLDialogElement): void {
  if (typeof dialog.showModal === "function") {
    try {
      dialog.showModal();
      return;
    } catch {
      // Not connected or already open: fall back to the attribute.
    }
  }
  dialog.setAttribute("open", "");
}

function hide(dialog: HTMLDialogElement): void {
  if (typeof dialog.close === "function") {
    dialog.close();
  } else {
    dialog.removeAttribute("open");
  }
}

/** A modal dialog with a title, a body and an action row. */
export function Dialog({
  open,
  title,
  onClose,
  dismissable = true,
  size = "md",
  actions,
  footer,
  closeLabel,
  initialFocus,
  className,
  children,
}: DialogProps) {
  const dict = useDict();
  const ref = useRef<HTMLDialogElement>(null);
  const opener = useRef<Element | null>(null);
  const titleId = useId();
  const openRef = useRef(open);
  useEffect(() => {
    openRef.current = open;
  });

  useEffect(() => {
    const dialog = ref.current;
    if (dialog === null) {
      return;
    }
    if (open && !dialog.open) {
      opener.current = document.activeElement;
      show(dialog);
      const target = initialFocus?.current;
      if (target !== null && target !== undefined) {
        target.focus();
      }
    } else if (!open && dialog.open) {
      hide(dialog);
      const element = opener.current;
      if (element instanceof HTMLElement && element.isConnected) {
        element.focus();
      }
    }
  }, [open, initialFocus]);

  useEffect(
    () => () => {
      const element = opener.current;
      if (element instanceof HTMLElement && element.isConnected) {
        element.focus();
      }
    },
    [],
  );

  return (
    <dialog
      ref={ref}
      className={`ow-dialog ow-dialog--${size}${className === undefined ? "" : ` ${className}`}`}
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        if (dismissable) {
          onClose();
        }
      }}
      onClose={() => {
        // The browser may close a dialog on its own (repeated Esc); a dialog
        // that must stay open is shown again.
        const dialog = ref.current;
        if (openRef.current && dialog !== null && !dialog.open) {
          if (dismissable) {
            onClose();
          } else {
            show(dialog);
          }
        }
      }}
    >
      {open ? (
        <div className="ow-dialog__frame">
          <div className="ow-dialog__header">
            <h2 id={titleId} className="ow-dialog__title">
              {title}
            </h2>
            {dismissable ? (
              <IconButton
                icon="close"
                label={closeLabel ?? dict.common.actions.closeDialog}
                size="sm"
                onClick={onClose}
              />
            ) : null}
          </div>
          <div className="ow-dialog__body">{children}</div>
          {footer === undefined || footer === null ? null : <div className="ow-dialog__footer">{footer}</div>}
          {actions === undefined ? null : <div className="ow-dialog__actions">{actions}</div>}
        </div>
      ) : null}
    </dialog>
  );
}

/** Props of {@link ConfirmDialog}. */
export interface ConfirmDialogProps {
  open: boolean;
  title: ReactNode;
  children?: ReactNode | undefined;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  /** The confirm button is a danger button. */
  danger?: boolean | undefined;
  /** Milliseconds before the confirm button can be pressed (default 500). */
  armDelayMs?: number | undefined;
  /** Shown between the body and the buttons; never scrolls away. */
  footer?: ReactNode | undefined;
  /**
   * When set (not `null`), the confirm button is disabled and this reason is
   * shown below it.
   */
  disabledReason?: string | null | undefined;
}

/**
 * A small confirmation: Cancel has the initial focus, the confirm button is
 * armed after a short delay and Enter never confirms.
 */
export function ConfirmDialog({
  open,
  title,
  children,
  confirmLabel,
  onConfirm,
  onCancel,
  danger = false,
  armDelayMs = 500,
  footer,
  disabledReason,
}: ConfirmDialogProps) {
  const dict = useDict();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!open) {
      return undefined;
    }
    const timer = window.setTimeout(() => {
      setArmed(true);
    }, armDelayMs);
    return () => {
      window.clearTimeout(timer);
      setArmed(false);
    };
  }, [open, armDelayMs]);
  return (
    <Dialog
      open={open}
      title={title}
      onClose={onCancel}
      size="sm"
      initialFocus={cancelRef}
      footer={footer}
      actions={
        <>
          <Button ref={cancelRef} onClick={onCancel}>
            {dict.common.actions.cancel}
          </Button>
          <Button
            variant={danger ? "danger" : "primary"}
            aria-disabled={!armed}
            disabledReason={disabledReason}
            onKeyDown={(event) => {
              if (event.key === "Enter") event.preventDefault();
            }}
            onClick={() => {
              if (armed && (disabledReason === undefined || disabledReason === null)) onConfirm();
            }}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      {children}
    </Dialog>
  );
}
