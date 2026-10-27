// The toast region (aria-live polite), rendered once by the app shell.

import { useDict } from "../i18n";
import { dismissToast, useToasts, type Toast } from "../state/toasts";
import { IconButton } from "./Button";
import { Icon, type IconName } from "./Icon";
import { TechnicalDetails } from "./Layout";

const ICONS: Record<Toast["kind"], IconName> = {
  info: "info",
  success: "check",
  warning: "warning",
  danger: "error",
};

/** One toast. */
export function ToastItem({ toast }: { toast: Toast }) {
  const dict = useDict();
  return (
    <div className={`ow-toast ow-toast--${toast.kind}`}>
      <Icon name={ICONS[toast.kind]} className="ow-toast__icon" />
      <div className="ow-toast__content">
        <p className="ow-toast__title">{toast.title}</p>
        {toast.body === undefined ? null : <p className="ow-toast__body">{toast.body}</p>}
        {toast.details === undefined ? null : <TechnicalDetails>{toast.details}</TechnicalDetails>}
      </div>
      <IconButton
        icon="close"
        size="sm"
        label={dict.common.actions.dismiss}
        onClick={() => {
          dismissToast(toast.id);
        }}
      />
    </div>
  );
}

/** Where toasts appear (bottom right). */
export function ToastRegion() {
  const toasts = useToasts();
  return (
    <div className="ow-toasts" aria-live="polite" aria-relevant="additions">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} />
      ))}
    </div>
  );
}
