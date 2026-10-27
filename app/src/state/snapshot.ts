// The keyboard's values (`get_snapshot`), refetched whenever the status
// announces a new snapshot revision.

import type { AppError, DeviceSnapshot } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { toAppError } from "../ipc/errors";
import { statusStore, useStatus } from "./status";
import { createStore, useStore } from "./store";

interface SnapshotState {
  snapshot: DeviceSnapshot | null;
  loading: boolean;
  error: AppError | null;
}

/** The last snapshot read from the backend. */
export const snapshotStore = createStore<SnapshotState>({ snapshot: null, loading: false, error: null });

let requested: number | null = null;

/** Fetches the snapshot when the status's snapshot revision differs from the one shown. */
export function syncSnapshot(): void {
  const revision = statusStore.get().snapshot_revision;
  const current = snapshotStore.get();
  if (revision === null) {
    requested = null;
    if (current.snapshot !== null || current.loading) {
      snapshotStore.set({ snapshot: null, loading: false, error: null });
    }
    return;
  }
  if (current.snapshot?.revision === revision || requested === revision) {
    return;
  }
  requested = revision;
  snapshotStore.set({ ...current, loading: true });
  api
    .getSnapshot()
    .then((snapshot) => {
      if (requested !== revision) {
        return;
      }
      requested = null;
      snapshotStore.set({ snapshot, loading: false, error: null });
    })
    .catch((error: unknown) => {
      if (requested !== revision) {
        return;
      }
      requested = null;
      snapshotStore.set({ ...snapshotStore.get(), loading: false, error: toAppError(error) });
    });
}

let started = false;

/** Keeps the snapshot in step with the status (once per app run). */
export function startSnapshotSync(): void {
  if (started) {
    return;
  }
  started = true;
  statusStore.subscribe(syncSnapshot);
  syncSnapshot();
}

/** The keyboard's values. */
export interface SnapshotView {
  snapshot: DeviceSnapshot | null;
  /** The values may be out of date (the status says so); edits are blocked. */
  stale: boolean;
  /** A newer snapshot is being fetched. */
  loading: boolean;
  /** Fetching failed. */
  error: AppError | null;
}

/** The keyboard's values; re-renders when they or their staleness change. */
export function useSnapshot(): SnapshotView {
  const state = useStore(snapshotStore);
  const status = useStatus();
  return {
    snapshot: state.snapshot,
    stale: status.snapshot_stale,
    loading: state.loading,
    error: state.error,
  };
}
