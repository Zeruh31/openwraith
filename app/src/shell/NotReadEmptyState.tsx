// What a page shows while its settings aren't read: a spinner while the
// keyboard is read, "couldn't load" with Read again after a failed fetch,
// "Connect the keyboard" only while none is connected, and otherwise the
// connection's own state in words (another program uses it, it reset, it
// isn't supported…) with a way to the Keyboard page.

import { Button } from "../components/Button";
import { EmptyState, Spinner } from "../components/Feedback";
import type { IconName } from "../components/Icon";
import { useDict } from "../i18n";
import { useFormat } from "../i18n/format";
import { navigate } from "../routes";
import { useOperationLock } from "../state/operation";
import { useStatus } from "../state/status";
import { readAgain } from "./actions";
import { connectionSummary } from "./statusText";
import { useReadBlockedReason } from "../state/operation";

/** Props of {@link NotReadEmptyState}. */
export interface NotReadEmptyStateProps {
  icon: IconName;
  /** The page's own title ("No dead band yet"). */
  title: string;
  /** The page's "Connect the keyboard…" text, shown only while no keyboard is connected. */
  body: string;
  /** The page's "couldn't load" text, shown after a failed fetch. */
  failed: string;
  /** The page's settings are being fetched. */
  loading: boolean;
  /** The last fetch failed. */
  error: boolean;
}

/** The empty state of a page whose settings aren't read. */
export function NotReadEmptyState({ icon, title, body, failed, loading, error }: NotReadEmptyStateProps) {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const lock = useOperationLock();
  const readBlocked = useReadBlockedReason();
  const state = status.connection.state;
  if (loading || state === "reading") {
    return (
      <EmptyState icon={icon} title={title}>
        <p className="ow-row">
          <Spinner />
          <span>{dict.common.notRead.reading}</span>
        </p>
      </EmptyState>
    );
  }
  if (error) {
    return (
      <EmptyState
        icon={icon}
        title={title}
        action={
          <Button
            icon="refresh"
            disabled={lock.busy || state !== "ready"}
            disabledReason={readBlocked}
            onClick={readAgain}
          >
            {dict.common.actions.readAgain}
          </Button>
        }
      >
        <p>{failed}</p>
      </EmptyState>
    );
  }
  if (state === "starting" || state === "searching" || state === "released") {
    return (
      <EmptyState icon={icon} title={title}>
        <p>{body}</p>
      </EmptyState>
    );
  }
  return (
    <EmptyState
      icon={icon}
      title={title}
      action={
        <Button
          onClick={() => {
            navigate("device");
          }}
        >
          {dict.common.notRead.goToKeyboard}
        </Button>
      }
    >
      <p>{connectionSummary(status, dict, format).text}</p>
    </EmptyState>
  );
}
