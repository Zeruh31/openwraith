// Which operation reports this window has shown, and which writes are being
// watched by a mounted flow. The shell's LastReportDialog uses both to show
// the report of an operation whose flow went away while it ran (a reloaded
// window, a page that unmounted, history navigation): such a report (LOST,
// MAY BE LOST, the safety backup and Undo) must never be lost silently.

import { useEffect, useMemo, useRef } from "react";

import type { LastReport } from "../ipc/bindings";

/** Flows (and runners) whose apply has started and not settled, while mounted. */
let pending = 0;

/** Reports this window has shown (their JSON). */
const seen = new Set<string>();

/**
 * Records that a mounted flow watches a running operation. Call the
 * returned function (idempotent) when its result arrived or the flow
 * unmounts, whichever comes first.
 */
export function beginApply(): () => void {
  pending += 1;
  let ended = false;
  return () => {
    if (ended) return;
    ended = true;
    pending -= 1;
  };
}

/** A mounted flow is waiting for an operation's result. */
export function appliesPending(): boolean {
  return pending > 0;
}

/** How a flow watches its operation. */
export interface OperationWatch {
  /** The operation starts: the flow watches it while mounted. */
  start: () => void;
  /** Its result (or error) arrived. */
  settle: () => void;
  /** The flow is mounted (a result that arrives after unmounting isn't shown by it). */
  mounted: () => boolean;
}

/**
 * Watches an operation while the component is mounted: when it unmounts
 * before the result arrives, the shell shows the report instead. Survives
 * StrictMode's second effect run.
 */
export function useOperationWatch(): OperationWatch {
  const running = useRef(false);
  const end = useRef<(() => void) | null>(null);
  const isMounted = useRef(false);
  useEffect(() => {
    isMounted.current = true;
    if (running.current && end.current === null) end.current = beginApply();
    return () => {
      isMounted.current = false;
      end.current?.();
      end.current = null;
    };
  }, []);
  return useMemo(
    () => ({
      start: () => {
        running.current = true;
        end.current?.();
        end.current = isMounted.current ? beginApply() : null;
      },
      settle: () => {
        running.current = false;
        end.current?.();
        end.current = null;
      },
      mounted: () => isMounted.current,
    }),
    [],
  );
}

/** The report inside a `LastReport`. */
export function reportOf(last: LastReport): unknown {
  switch (last.kind) {
    case "edit":
    case "restore":
    case "import":
    case "profile_switch":
      return last.report;
    case "backup":
      return last.saved;
  }
}

/** Records that a report was shown (a flow showed its result). */
export function markReportSeen(report: unknown): void {
  seen.add(JSON.stringify(report));
}

/** Whether this window already showed `last`. */
export function isReportSeen(last: LastReport): boolean {
  return seen.has(JSON.stringify(reportOf(last)));
}

/** Forgets everything (tests). */
export function resetReports(): void {
  pending = 0;
  seen.clear();
}
