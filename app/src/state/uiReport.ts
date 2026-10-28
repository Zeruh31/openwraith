// What the frontend tells the backend about itself (`report_ui_state`):
// whether any page has unwritten changes (the per-game automatic profiles
// wait while it does) and whether this run has the "every wraith.software
// tab is closed" acknowledgement (a tray profile click opens the window
// instead until it has). The backend resets both to `false` at every
// `subscribe`, so the report is sent again after each one, and on every
// change of either value.

import type { UiStateReport } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { hasDrafts, subscribeDrafts } from "./drafts";
import { subscribeSession, vendorTickRequired } from "./session";
import { onSubscribed } from "./status";

/** How long a failed report waits before it is sent again. */
const RETRY_MS = 2000;

let started = false;
/** The report the backend has (`null`: unknown, send the next one). */
let sent: UiStateReport | null = null;
/** Counts the `subscribe`s: a report answered after a newer one is not what the backend keeps. */
let epoch = 0;
let inFlight = false;
let again = false;
let retryTimer: number | null = null;
const stops: (() => void)[] = [];

/** The report for the frontend's state now. */
export function currentUiState(): UiStateReport {
  return { drafts: hasDrafts(), vendor_acknowledged: !vendorTickRequired() };
}

function same(a: UiStateReport | null, b: UiStateReport): boolean {
  return a !== null && a.drafts === b.drafts && a.vendor_acknowledged === b.vendor_acknowledged;
}

function scheduleRetry(): void {
  if (retryTimer !== null) return;
  retryTimer = window.setTimeout(() => {
    retryTimer = null;
    send();
  }, RETRY_MS);
}

/** Sends the current state unless the backend already has it (one call at a time; the newest state wins). */
function send(): void {
  if (!started) return;
  const state = currentUiState();
  if (same(sent, state)) return;
  if (inFlight) {
    again = true;
    return;
  }
  inFlight = true;
  const sentIn = epoch;
  api
    .reportUiState(state)
    .then(() => {
      if (sentIn === epoch) sent = state;
    })
    .catch(() => {
      // The automation keeps its last report; try again shortly.
      sent = null;
      scheduleRetry();
    })
    .finally(() => {
      inFlight = false;
      if (again) {
        again = false;
        send();
      }
    });
}

/**
 * Starts reporting (once per app run): now, on every change of the drafts
 * or the vendor acknowledgement, and after every `subscribe`.
 */
export function startUiReport(): void {
  if (started) return;
  started = true;
  stops.push(subscribeDrafts(send), subscribeSession(send));
  stops.push(
    onSubscribed(() => {
      // `subscribe` reset the backend's copy to all-false.
      epoch += 1;
      sent = null;
      send();
    }),
  );
  send();
}

/** Stops reporting and forgets what was sent (tests). */
export function resetUiReport(): void {
  for (const stop of stops.splice(0)) stop();
  started = false;
  sent = null;
  inFlight = false;
  again = false;
  if (retryTimer !== null) {
    window.clearTimeout(retryTimer);
    retryTimer = null;
  }
}
