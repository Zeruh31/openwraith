// The backup chosen for restoring: opened with the system file dialog, from
// the safety-backup list, or from the route (`#/backup?restore=<token>`,
// used by Undo). Opening a file is a read of the file only; the keyboard
// isn't involved. File paths never leave Rust: the backend hands out tokens.

import { useCallback, useRef, useState } from "react";

import type { AppError, BackupFileSummary, DialogText, FileToken } from "../../ipc/bindings";
import { api } from "../../ipc/commands";
import { toAppError } from "../../ipc/errors";

/** The chosen backup and how loading it went. */
export interface BackupSource {
  /** The loaded backup, or `null`. */
  summary: BackupFileSummary | null;
  /** Increases with every loaded backup (the wizard starts over). */
  loadId: number;
  /** A safety backup is being opened. */
  loading: boolean;
  /** Opening failed (the file can't be used). */
  error: AppError | null;
  /** Shows the open dialog and loads the chosen file. */
  openFile: (dialog: DialogText) => void;
  /** Loads a safety backup (or any file the backend gave a token for). */
  openSafety: (token: FileToken) => void;
  /**
   * Summarizes the loaded backup again, so that its comparison with the
   * connected keyboard (same model, same firmware) is current. The wizard
   * keeps its state; failures are ignored (the old summary stays).
   */
  recheck: () => void;
}

interface State {
  summary: BackupFileSummary | null;
  loadId: number;
  loading: boolean;
  error: AppError | null;
}

/** The backup chosen for restoring. */
export function useBackupSource(): BackupSource {
  const [state, setState] = useState<State>({ summary: null, loadId: 0, loading: false, error: null });
  const seq = useRef(0);
  const dialogOpen = useRef(false);

  const loaded = useCallback((mine: number, summary: BackupFileSummary) => {
    if (mine !== seq.current) return;
    setState((current) => ({ summary, loadId: current.loadId + 1, loading: false, error: null }));
  }, []);

  const failed = useCallback((mine: number, error: unknown) => {
    if (mine !== seq.current) return;
    setState((current) => ({ ...current, summary: null, loading: false, error: toAppError(error) }));
  }, []);

  const openFile = useCallback(
    (dialog: DialogText) => {
      if (dialogOpen.current) return;
      dialogOpen.current = true;
      seq.current += 1;
      const mine = seq.current;
      api
        .openBackupFile(dialog)
        .then(
          (summary) => {
            if (summary !== null) {
              loaded(mine, summary);
            } else if (mine === seq.current) {
              setState((current) => (current.loading ? { ...current, loading: false } : current));
            }
          },
          (error: unknown) => {
            failed(mine, error);
          },
        )
        .finally(() => {
          dialogOpen.current = false;
        });
    },
    [loaded, failed],
  );

  const openSafety = useCallback(
    (token: FileToken) => {
      seq.current += 1;
      const mine = seq.current;
      setState((current) => ({ ...current, loading: true, error: null }));
      api.openSafetyBackup(token).then(
        (summary) => {
          loaded(mine, summary);
        },
        (error: unknown) => {
          failed(mine, error);
        },
      );
    },
    [loaded, failed],
  );

  const token = state.summary?.file.token ?? null;
  const recheck = useCallback(() => {
    if (token === null) return;
    const mine = seq.current;
    // `open_safety_backup` summarizes any backup file the backend gave a token for.
    api.openSafetyBackup(token).then(
      (summary) => {
        if (mine !== seq.current) return;
        setState((current) =>
          current.summary?.file.token === summary.file.token ? { ...current, summary } : current,
        );
      },
      () => {
        // The previous summary stays; prepare_restore checks model and firmware itself.
      },
    );
  }, [token]);

  return { ...state, openFile, openSafety, recheck };
}
