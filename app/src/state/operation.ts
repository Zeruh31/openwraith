// The operation lock: one flow at a time. The backend refuses a second
// operation anyway (`operation_running`); this local lock is taken
// synchronously in click handlers so a double click never starts two flows.

import { useDict } from "../i18n";
import type { OpKind } from "../ipc/bindings";
import { useStatus } from "./status";
import { createStore, useStore } from "./store";

const lockStore = createStore<string | null>(null);

/** Takes the lock for `owner`; `false` when someone else holds it. */
export function tryLock(owner: string): boolean {
  const holder = lockStore.get();
  if (holder !== null && holder !== owner) {
    return false;
  }
  lockStore.set(owner);
  return true;
}

/** Releases the lock if `owner` holds it. */
export function unlock(owner: string): void {
  if (lockStore.get() === owner) {
    lockStore.set(null);
  }
}

/** Whether anyone holds the local lock (outside React). */
export function isLocked(): boolean {
  return lockStore.get() !== null;
}

/** The operation state for disabling controls. */
export interface OperationLock {
  /** An operation runs in the backend or a flow is open here. */
  busy: boolean;
  /** The backend's running operation, if any. */
  kind: OpKind | null;
  /** A flow is open in this window. */
  local: boolean;
}

/** The operation state; re-renders when it changes. */
export function useOperationLock(): OperationLock {
  const status = useStatus();
  const holder = useStore(lockStore);
  return {
    busy: status.busy !== null || holder !== null,
    kind: status.busy?.kind ?? null,
    local: holder !== null,
  };
}

/**
 * Why "Read again" is unavailable now (`null`: it is available): an
 * OpenWraith command holds the keyboard, and the worker sends nothing then.
 */
export function useReadBlockedReason(): string | null {
  const status = useStatus();
  const dict = useDict();
  return status.paused_for_cli ? dict.backupRestore.reasons.pausedForCli : null;
}
