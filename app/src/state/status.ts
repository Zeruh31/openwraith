// The device status pushed by the backend on the `subscribe` channel, and
// the one-shot notices. Notices that arrive while nothing listens yet (the
// first `subscribe` happens before the shell is shown) are kept until the
// shell's toaster takes them (`takeUndeliveredNotices`).

import { useEffect, useRef } from "react";

import type { AppEvent, DeviceStatus, Notice } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { createStore, useStore } from "./store";

/** The status before the backend has answered. */
export const INITIAL_STATUS: DeviceStatus = {
  revision: 0,
  generation: 0,
  connection: { state: "starting" },
  device: null,
  active_profile: null,
  firmware_tested: null,
  write_gate: { allowed: false, reasons: ["not_connected"] },
  busy: null,
  snapshot_revision: null,
  snapshot_stale: false,
  close_requested: false,
  interrupted: null,
  sim: false,
  travel: { state: "closed", stray_reports: 0, marker_left: false },
  recovery: { reset_suspected: [], reset_reported: false, firmware_changed: false },
  automation: { enabled: false, paused: null, holding: null, waiting: null },
  paused_for_cli: false,
};

/** The latest device status. */
export const statusStore = createStore<DeviceStatus>(INITIAL_STATUS);

/**
 * What the OpenWraith command (the CLI) said it does while the app waits
 * for it (`paused_for_cli { purpose }`, e.g. "set trigger"); `null` when
 * unknown or not paused.
 */
export const cliPurposeStore = createStore<string | null>(null);

const noticeListeners = new Set<(notice: Notice) => void>();

/** How many notices are kept while nothing listens. */
const UNDELIVERED_MAX = 20;
let undelivered: Notice[] = [];

const subscribedHooks = new Set<() => void>();

/** Applies a status unless an equal or newer one is already shown. */
export function applyStatus(status: DeviceStatus): void {
  const wasPaused = statusStore.get().paused_for_cli;
  statusStore.set((current) => (status.revision >= current.revision ? status : current));
  // The command's purpose belongs to its pause (its notice may come first).
  if (wasPaused && !statusStore.get().paused_for_cli) cliPurposeStore.set(null);
}

/** Delivers a notice to every listener (or keeps it until the toaster takes it, when nothing listens). */
export function emitNotice(notice: Notice): void {
  if (notice.notice === "paused_for_cli") cliPurposeStore.set(notice.purpose);
  if (noticeListeners.size === 0) {
    undelivered = [...undelivered, notice].slice(-UNDELIVERED_MAX);
    return;
  }
  for (const listener of [...noticeListeners]) {
    listener(notice);
  }
}

/** The notices that arrived while nothing listened, oldest first; they are given out once. */
export function takeUndeliveredNotices(): Notice[] {
  const taken = undelivered;
  undelivered = [];
  return taken;
}

/** Handles one event of the `subscribe` channel. */
export function handleAppEvent(event: AppEvent): void {
  if (event.event === "status") {
    applyStatus(event.status);
  } else {
    emitNotice(event.notice);
  }
}

/**
 * Runs `hook` after every successful `subscribe` (the backend resets what
 * the frontend reported, see `state/uiReport.ts`); returns the remover.
 */
export function onSubscribed(hook: () => void): () => void {
  subscribedHooks.add(hook);
  return () => {
    subscribedHooks.delete(hook);
  };
}

/**
 * Subscribes to the backend's events (replacing an earlier subscriber, e.g.
 * after a webview reload) and applies the current status.
 */
export async function connectEvents(): Promise<void> {
  const status = await api.subscribe(handleAppEvent);
  applyStatus(status);
  for (const hook of [...subscribedHooks]) hook();
}

/** Forgets kept notices and the CLI's purpose (tests). */
export function resetNoticeState(): void {
  undelivered = [];
  cliPurposeStore.set(null);
}

/** The device status; re-renders on every change. */
export function useStatus(): DeviceStatus {
  return useStore(statusStore);
}

/** Calls `listener` for every notice while the component is mounted. */
export function useNotices(listener: (notice: Notice) => void): void {
  const ref = useRef(listener);
  useEffect(() => {
    ref.current = listener;
  });
  useEffect(() => {
    const forward = (notice: Notice) => {
      ref.current(notice);
    };
    noticeListeners.add(forward);
    return () => {
      noticeListeners.delete(forward);
    };
  }, []);
}

/** Whether the keyboard is connected and read (the state that allows edits). */
export function isReady(status: DeviceStatus): boolean {
  return status.connection.state === "ready";
}
