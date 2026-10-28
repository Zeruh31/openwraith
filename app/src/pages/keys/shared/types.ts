// The contract between the Keys page and its tabs.

import type { KeysState } from "../../../ipc/bindings";
import type { KeysView } from "./keysState";

/**
 * What every Keys tab receives. The page renders a tab only once the keys
 * state is loaded; it shows the empty, reading, stale and disabled states
 * itself.
 */
export interface KeysTabProps {
  /** The keymap group of the last full read. */
  state: KeysState;
  /** The whole view (loading, error, stale). */
  view: KeysView;
  /**
   * Why a change can't be reviewed and written now (not connected, keys not
   * editable, stale values, writing paused, busy), or `null`. Tabs add their
   * own reasons ("nothing changed").
   */
  blockReason: string | null;
}
