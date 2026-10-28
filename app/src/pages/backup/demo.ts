// What a browser-preview scenario starts on this page (for screenshots of the
// flows without clicking through them). Outside the preview `previewUi`
// returns `undefined`, so nothing here runs in the app.

import { useEffect, useRef } from "react";

/** The page's preview UI state (`PreviewScenario.ui.backup`). */
export interface BackupDemo {
  /** The action started once after the page opened. */
  run?: "open_file" | "compare" | "backup_all" | "backup_active" | undefined;
  /** Profiles selected before comparing (default: every profile of the file). */
  profiles?: number[] | undefined;
  /** Tick "Restore anyway" for a backup made with other firmware. */
  allowFirmware?: boolean | undefined;
  /**
   * "Share a profile": export (save dialog, then the export), open a profile
   * file, or open one and compare it (the import review).
   */
  share?: "export" | "open_import" | "import" | undefined;
  /** The profile exported (default: the active one). */
  exportProfile?: number | undefined;
  /** The profile imported into (default: the active one). */
  into?: number | undefined;
  /** Also tick "Key assignments, macros and advanced keys" for the import. */
  keys?: boolean | undefined;
}

/** The delay before a demo action starts (the page has rendered by then). */
const DEMO_DELAY_MS = 250;

/** Runs `action` once, shortly after `active` became true. */
export function useDemoAction(active: boolean, action: () => void): void {
  const actionRef = useRef(action);
  const done = useRef(false);
  useEffect(() => {
    actionRef.current = action;
  });
  useEffect(() => {
    if (!active) return undefined;
    const timer = window.setTimeout(() => {
      if (done.current) return;
      done.current = true;
      actionRef.current();
    }, DEMO_DELAY_MS);
    return () => {
      window.clearTimeout(timer);
    };
  }, [active]);
}
