// Shell actions that call the backend and report failures as toasts.

import { getDict } from "../i18n";
import type { AppError } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { toAppError } from "../ipc/errors";
import { tryLock, unlock } from "../state/operation";
import { toast } from "../state/toasts";

/** Shows an error as a toast (title from the dictionary, the message as details). */
export function toastError(error: unknown): AppError {
  const appError = toAppError(error);
  toast({ kind: "danger", title: getDict().errors.kind[appError.kind].title, details: appError.message });
  return appError;
}

/** Actions whose call has not settled yet (a double click runs them once). */
const inFlight = new Set<string>();

/**
 * Runs the action `name` once at a time: a second click while the first call
 * is pending does nothing. It also holds the local operation lock (when free)
 * so that buttons are disabled at once, before the backend's busy status
 * arrives. Errors are toasts, except `quiet` ones (harmless repeats, e.g.
 * `operation_running` when another action got there first).
 */
function run(name: string, start: () => Promise<unknown>, quiet: readonly AppError["kind"][] = []): void {
  if (inFlight.has(name)) return;
  inFlight.add(name);
  const owner = `shell:${name}`;
  const locked = tryLock(owner);
  start()
    .catch((error: unknown) => {
      const appError = toAppError(error);
      if (!quiet.includes(appError.kind)) toastError(appError);
    })
    .finally(() => {
      inFlight.delete(name);
      if (locked) unlock(owner);
    });
}

/** Reads the keyboard again (progress shows in the header). */
export function readAgain(): void {
  run(
    "refresh",
    () =>
      api.refresh(() => {
        // The header's busy indicator shows the progress from the status.
      }),
    ["operation_running"],
  );
}

/** Closes and reopens the connection (also "read the keyboard again" after a reset). */
export function reconnect(): void {
  run("reconnect", () => api.reconnect(), ["operation_running"]);
}

/** Releases the keyboard. */
export function release(): void {
  run("release", () => api.disconnect(), ["operation_running"]);
}

/** "I closed it, check again" (a second click after the first one worked finds nothing to recheck). */
export function recheckOtherProgram(): void {
  run("recheck", () => api.recheckOtherProgram(), ["operation_running", "invalid_request"]);
}

/** Shows the safety-backup folder in the file manager. */
export function revealSafetyDir(): void {
  run("reveal-safety", () => api.reveal({ target: "safety_dir" }));
}

/** Shows the license files (LICENSE.txt, THIRD-PARTY-NOTICES.txt) in the file manager. */
export function revealLegalNotices(): void {
  run("reveal-legal", () => api.reveal({ target: "legal_notices" }));
}

/** Forgets pending actions (tests). */
export function resetActions(): void {
  inFlight.clear();
}
