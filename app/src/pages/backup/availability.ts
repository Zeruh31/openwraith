// Whether the page's keyboard actions (backing up, comparing) can start now.
// The backend is the authority (it refuses with `not_connected` or
// `operation_running`); this only disables the buttons with a reason.

import { useDict } from "../../i18n";
import { useOperationLock } from "../../state/operation";
import { useStatus } from "../../state/status";

/** Why the keyboard actions are unavailable now, or `null` when they can start. */
export function useDeviceBlock(): string | null {
  const dict = useDict();
  const status = useStatus();
  const lock = useOperationLock();
  const reasons = dict.backupRestore.reasons;
  switch (status.connection.state) {
    case "ready":
      return lock.busy ? reasons.busy : null;
    case "other_program":
      return reasons.otherProgram;
    case "reset_seen":
      return reasons.resetSeen;
    case "starting":
    case "searching":
    case "several_keyboards":
    case "opening":
    case "open_failed":
    case "observing":
    case "reading":
    case "read_failed":
    case "lost":
    case "unsupported":
    case "released":
    case "fault":
      return reasons.notConnected;
  }
}
