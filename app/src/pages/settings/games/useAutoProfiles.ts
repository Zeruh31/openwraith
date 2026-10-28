// Settings > Games' data: `get_auto_profile_state` (no keyboard traffic),
// fetched on mount, every 2 seconds while the tab is shown (the live status
// line changes without a notice), on the automation's notices and when the
// status' automation part changes; saves and pauses replace it with their
// answer. An answer older than one already shown is dropped, and no fetch
// starts while a save runs (it could answer with the state from before the
// save after the save's own answer); one follows the save.

import { useCallback, useEffect, useRef, useState } from "react";

import type { AppError, AutoProfileRulesIntent, AutoProfileStateView } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { useNotices, useStatus } from "../../../state/status";

/** How often the live status line is fetched again. */
export const POLL_MS = 2000;

/** What the Games tab works with. */
export interface AutoProfiles {
  /** The state (`null` until the first answer). */
  view: AutoProfileStateView | null;
  /** The first fetch failed (and nothing is shown yet). */
  loadError: AppError | null;
  /** A save or pause runs (the controls wait). */
  saving: boolean;
  /** Fetches again. */
  reload: () => void;
  /** Saves the rules; resolves with the error when it was refused (`null` when saved). */
  save: (intent: AutoProfileRulesIntent) => Promise<AppError | null>;
  /** Pauses or resumes for this run; resolves with the error when it was refused. */
  pause: (paused: boolean) => Promise<AppError | null>;
}

const AUTOMATION_NOTICES: readonly string[] = [
  "auto_switched",
  "auto_switch_deferred",
  "auto_switch_skipped",
  "auto_profile_paused",
  "profile_changed_on_keyboard",
  "paused_for_cli",
];

/** The automatic profiles' state for Settings > Games. */
export function useAutoProfiles(): AutoProfiles {
  const status = useStatus();
  const [view, setView] = useState<AutoProfileStateView | null>(null);
  const [loadError, setLoadError] = useState<AppError | null>(null);
  const [saving, setSaving] = useState(false);
  const alive = useRef(true);
  /** Sequence numbers: requests are numbered, and an answer older than the one shown is dropped. */
  const issued = useRef(0);
  const shown = useRef(0);
  /** Saves and pauses in flight (no fetch meanwhile). */
  const saves = useRef(0);

  const show = useCallback((seq: number, next: AutoProfileStateView) => {
    if (!alive.current || seq < shown.current) return;
    shown.current = seq;
    setView(next);
    setLoadError(null);
  }, []);

  const reload = useCallback(() => {
    if (saves.current > 0) return;
    issued.current += 1;
    const seq = issued.current;
    api
      .getAutoProfileState()
      .then((next) => {
        show(seq, next);
      })
      .catch((error: unknown) => {
        if (alive.current && shown.current === 0) setLoadError(toAppError(error));
      });
  }, [show]);

  const run = useCallback(
    async (call: () => Promise<AutoProfileStateView>): Promise<AppError | null> => {
      issued.current += 1;
      const seq = issued.current;
      saves.current += 1;
      setSaving(true);
      try {
        show(seq, await call());
        return null;
      } catch (error) {
        return toAppError(error);
      } finally {
        saves.current -= 1;
        if (alive.current) {
          setSaving(false);
          // What changed meanwhile (the live line) is fetched once now.
          if (saves.current === 0) reload();
        }
      }
    },
    [show, reload],
  );

  useEffect(() => {
    alive.current = true;
    reload();
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "hidden") reload();
    }, POLL_MS);
    return () => {
      alive.current = false;
      window.clearInterval(timer);
    };
  }, [reload]);

  useNotices((notice) => {
    if (AUTOMATION_NOTICES.includes(notice.notice)) reload();
  });

  // The status carries the automation's summary (holding, waiting, paused).
  const automationKey = JSON.stringify(status.automation);
  const firstKey = useRef(automationKey);
  useEffect(() => {
    if (automationKey === firstKey.current) return;
    firstKey.current = automationKey;
    reload();
  }, [automationKey, reload]);

  return {
    view,
    loadError,
    saving,
    reload,
    save: (intent) => run(() => api.setAutoProfileRules(intent)),
    pause: (paused) => run(() => api.pauseAutoProfile(paused)),
  };
}
