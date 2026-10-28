// A part of the keyboard's settings that a tab shows (the dead band, SOCD):
// fetched with a direct command that sends nothing to the keyboard (the
// backend answers from the last full read), fetched again whenever the
// status announces a new snapshot revision, when a tab that shows it is
// opened, after the tab's own write, and on demand (e.g. the keyboard's own
// SOCD toggle). Each part has its own `revision`, the `based_on` of its
// intents.

import { useEffect } from "react";

import type { AppError } from "../../../ipc/bindings";
import { toAppError } from "../../../ipc/errors";
import { statusStore, useStatus } from "../../../state/status";
import { createStore, useStore, type Store } from "../../../state/store";

/** What the store holds. */
export interface PartState<T> {
  state: T | null;
  loading: boolean;
  error: AppError | null;
}

/** The part as a tab sees it. */
export interface PartView<T> extends PartState<T> {
  /** Another program changed the keyboard since the read: writes are blocked. */
  stale: boolean;
}

/** A part's store and its fetch rules. */
export interface PartStore<T> {
  store: Store<PartState<T>>;
  /** Fetches the part when the status's snapshot revision differs from the one it was fetched for. */
  sync: () => void;
  /** Fetches the part again now (after a write, a keyboard push, a refused stale plan). */
  refetch: () => void;
  /** Follows the status from now on and fetches the part unless a fetch is under way (a tab opened). */
  open: () => void;
  /** Forgets the part (tests). */
  reset: () => void;
}

const EMPTY: PartState<never> = { state: null, loading: false, error: null };

/** Creates the store of a part fetched by `fetch`. */
export function createPartStore<T>(fetch: () => Promise<T | null>): PartStore<T> {
  const store = createStore<PartState<T>>(EMPTY);
  /** The snapshot revision the shown state was fetched for. */
  let fetchedFor: number | null = null;
  /** The revision a fetch under way was started for. */
  let pendingFor: number | null = null;
  /** Answers of older fetches are ignored. */
  let seq = 0;
  let subscribed = false;

  const clear = () => {
    seq += 1;
    fetchedFor = null;
    pendingFor = null;
    const current = store.get();
    if (current.state !== null || current.loading || current.error !== null) store.set(EMPTY);
  };

  const load = (revision: number) => {
    seq += 1;
    const mine = seq;
    pendingFor = revision;
    store.set({ ...store.get(), loading: true });
    fetch()
      .then((state) => {
        if (mine !== seq) return;
        pendingFor = null;
        fetchedFor = revision;
        store.set({ state, loading: false, error: null });
      })
      .catch((error: unknown) => {
        if (mine !== seq) return;
        pendingFor = null;
        fetchedFor = revision;
        store.set({ ...store.get(), loading: false, error: toAppError(error) });
      });
  };

  const sync = () => {
    const revision = statusStore.get().snapshot_revision;
    if (revision === null) {
      clear();
      return;
    }
    if (fetchedFor === revision || pendingFor === revision) return;
    load(revision);
  };

  const refetch = () => {
    const revision = statusStore.get().snapshot_revision;
    if (revision === null) {
      clear();
      return;
    }
    load(revision);
  };

  const open = () => {
    if (!subscribed) {
      subscribed = true;
      statusStore.subscribe(sync);
    }
    if (pendingFor === null) refetch();
  };

  const reset = () => {
    clear();
    store.set(EMPTY);
  };

  return { store, sync, refetch, open, reset };
}

/** A part in a component: fetched again when the component mounts, then kept in step with the status. */
export function usePart<T>(part: PartStore<T>): PartView<T> {
  const value = useStore(part.store);
  const status = useStatus();
  useEffect(() => {
    part.open();
  }, [part]);
  return { ...value, stale: status.snapshot_stale };
}
