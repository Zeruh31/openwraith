// Per-run acknowledgements: the "wraith.software tabs are closed" tick is
// required for the first write of an app run and again after any episode in
// which another program used the keyboard.
//
// Every transition into `other_program` starts a new episode. A tick given in
// a dialog belongs to the episode it was given in (see
// `flow/vendorTick.tsx`), so a tick given before another program appeared
// never counts after it, even in a dialog that stayed open.

import { statusStore } from "./status";
import { createStore, useStore } from "./store";

interface SessionState {
  vendorAcknowledged: boolean;
  /** Counts the transitions into `other_program` in this app run. */
  episode: number;
}

const sessionStore = createStore<SessionState>({ vendorAcknowledged: false, episode: 0 });

let watching = false;
let wasOtherProgram = false;

/** Re-arms the tick whenever the status shows another program (once per app run). */
export function startSessionWatch(): void {
  if (watching) {
    return;
  }
  watching = true;
  const check = () => {
    const otherProgram = statusStore.get().connection.state === "other_program";
    if (otherProgram && !wasOtherProgram) {
      const current = sessionStore.get();
      sessionStore.set({ vendorAcknowledged: false, episode: current.episode + 1 });
    }
    wasOtherProgram = otherProgram;
  };
  statusStore.subscribe(check);
  check();
}

/** Records that the user confirmed the vendor tabs are closed. */
export function acknowledgeVendor(): void {
  const current = sessionStore.get();
  if (!current.vendorAcknowledged) {
    sessionStore.set({ ...current, vendorAcknowledged: true });
  }
}

/** Resets the acknowledgement (tests). */
export function resetSession(): void {
  sessionStore.set({ vendorAcknowledged: false, episode: 0 });
  wasOtherProgram = false;
}

/** The vendor tick of this run. */
export interface VendorTick {
  /** The next write needs the tick. */
  required: boolean;
  /** The current other-program episode (a tick counts only within it). */
  episode: number;
  /** Records the tick (call it when the write starts). */
  acknowledge: () => void;
}

/** Whether the next write needs the vendor tick, and how to give it. */
export function useVendorTick(): VendorTick {
  const state = useStore(sessionStore);
  return { required: !state.vendorAcknowledged, episode: state.episode, acknowledge: acknowledgeVendor };
}

/** Calls `listener` after every change of the acknowledgement; returns the unsubscribe function. */
export function subscribeSession(listener: () => void): () => void {
  return sessionStore.subscribe(listener);
}

/** Whether the next write needs the vendor tick (outside React, at call time). */
export function vendorTickRequired(): boolean {
  return !sessionStore.get().vendorAcknowledged;
}
