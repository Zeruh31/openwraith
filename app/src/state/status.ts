// The device status pushed by the backend on the `subscribe` channel, and
// the one-shot notices.

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
};

/** The latest device status. */
export const statusStore = createStore<DeviceStatus>(INITIAL_STATUS);

const noticeListeners = new Set<(notice: Notice) => void>();

/** Applies a status unless an equal or newer one is already shown. */
export function applyStatus(status: DeviceStatus): void {
  statusStore.set((current) => (status.revision >= current.revision ? status : current));
}

/** Delivers a notice to every listener. */
export function emitNotice(notice: Notice): void {
  for (const listener of [...noticeListeners]) {
    listener(notice);
  }
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
 * Subscribes to the backend's events (replacing an earlier subscriber, e.g.
 * after a webview reload) and applies the current status.
 */
export async function connectEvents(): Promise<void> {
  const status = await api.subscribe(handleAppEvent);
  applyStatus(status);
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
