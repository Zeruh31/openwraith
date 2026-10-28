// "Read at 10:30:00" and the button that reads the keyboard again: one
// wording and one control (an icon button with its name as the tooltip) on
// every page that shows values read from the keyboard.

import { IconButton } from "../components/Button";
import { useDict } from "../i18n";
import { useFormat } from "../i18n/format";
import { useOperationLock } from "../state/operation";
import { useStatus } from "../state/status";
import { readAgain } from "./actions";

/** Props of {@link ReadAt}. */
export interface ReadAtProps {
  /** When the values were read (RFC 3339). */
  time: string;
  /** Shown before the time, e.g. "Keys on profile 1". */
  prefix?: string | undefined;
}

/** When the values shown were read, with "Read the keyboard again". */
export function ReadAt({ time, prefix }: ReadAtProps) {
  const dict = useDict();
  const format = useFormat();
  const lock = useOperationLock();
  const status = useStatus();
  const text = dict.common.readAt.time({ time: format.time(time) });
  return (
    <span className="ow-read-at">
      <span className="ow-muted ow-small ow-num">{prefix === undefined ? text : `${prefix} · ${text}`}</span>
      <IconButton
        icon="refresh"
        size="sm"
        label={dict.common.readAt.again}
        disabled={lock.busy || status.connection.state !== "ready"}
        onClick={readAgain}
      />
    </span>
  );
}
