// The safety backups OpenWraith saved before each change (newest first),
// with "Restore…" (opens it in the restore wizard: the normal compare →
// review → confirm flow) and "Show in folder". Listing reads file names
// only; nothing is loaded until one is chosen.

import { useEffect, useState, type ReactNode } from "react";

import { Button, IconButton } from "../../components/Button";
import { EmptyState, Spinner } from "../../components/Feedback";
import { Card } from "../../components/Layout";
import { ErrorView } from "../../flow/ErrorView";
import { useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { AppError, SafetyBackupEntry } from "../../ipc/bindings";
import { api } from "../../ipc/commands";
import { toAppError } from "../../ipc/errors";
import { revealSafetyDir, toastError } from "../../shell/actions";
import { safetyDirPath, useAppInfo } from "../../state/appInfo";
import { useStatus } from "../../state/status";
import { stampToIso } from "./model";

/** Rows shown at first and added by "Show more". */
export const SAFETY_PAGE = 20;

type ListState = { s: "loading" } | { s: "ok"; entries: SafetyBackupEntry[] } | { s: "error"; error: AppError };

/** Props of {@link SafetyBackups}. */
export interface SafetyBackupsProps {
  /** Changing it reads the list again (after a restore made a new safety backup). */
  reloadKey: number;
  /** Opens the backup in the restore wizard. */
  onRestore: (entry: SafetyBackupEntry) => void;
}

/** The "Safety backups" card. */
export function SafetyBackups({ reloadKey, onRestore }: SafetyBackupsProps) {
  const dict = useDict();
  const format = useFormat();
  const info = useAppInfo();
  const status = useStatus();
  const t = dict.backupRestore.safety;
  const [state, setState] = useState<ListState>({ s: "loading" });
  const [limit, setLimit] = useState(SAFETY_PAGE);
  const [manual, setManual] = useState(0);

  useEffect(() => {
    let alive = true;
    api.listSafetyBackups().then(
      (entries) => {
        if (alive) setState({ s: "ok", entries });
      },
      (error: unknown) => {
        if (alive) setState({ s: "error", error: toAppError(error) });
      },
    );
    return () => {
      alive = false;
    };
  }, [reloadKey, manual]);

  // Seconds: several quick edits within one minute make backups that must
  // still be told apart (the user picks one for Undo).
  const dateOf = (entry: SafetyBackupEntry): string => {
    const iso = stampToIso(entry.stamp) ?? entry.modified_at;
    return iso === null ? entry.stamp : format.dateTimeSeconds(iso);
  };
  // The model's name ("Wraith W75") when it is the connected keyboard's
  // product; the product code stays in the tooltip.
  const keyboardOf = (entry: SafetyBackupEntry): string => {
    const device = status.device;
    if (entry.product !== null && device !== null && device.product === entry.product && device.model !== null) {
      return device.model;
    }
    return entry.product ?? dict.common.unknown;
  };

  let body: ReactNode;
  switch (state.s) {
    case "loading":
      body = (
        <p className="ow-row" role="status">
          <Spinner />
          <span>{dict.common.loading}</span>
        </p>
      );
      break;
    case "error":
      body = (
        <>
          <p>{t.loadFailed}</p>
          <ErrorView error={state.error} written="none" />
        </>
      );
      break;
    case "ok": {
      const entries = state.entries;
      if (entries.length === 0) {
        body = <EmptyState icon="lock" title={t.empty} />;
        break;
      }
      const shown = entries.slice(0, limit);
      body = (
        <>
          <div className="ow-table-wrap">
            <table className="ow-table br-safety">
              <thead>
                <tr>
                  <th scope="col" className="ow-align-start">
                    {t.columns.made}
                  </th>
                  <th scope="col" className="ow-align-start">
                    {t.columns.reason}
                  </th>
                  <th scope="col" className="ow-align-start">
                    {t.columns.keyboard}
                  </th>
                  <th scope="col" className="ow-align-end">
                    <span className="sr-only">{t.columns.actions}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {shown.map((entry) => {
                  const date = dateOf(entry);
                  const reason = dict.backupRestore.safetyReasons[entry.reason];
                  return (
                    <tr key={entry.file.token}>
                      <th scope="row" className="ow-align-start ow-num br-safety__date">
                        {date}
                      </th>
                      <td>{reason}</td>
                      <td title={entry.product ?? undefined}>{keyboardOf(entry)}</td>
                      <td className="ow-align-end">
                        <div className="br-safety__actions">
                          <Button
                            size="sm"
                            icon="restore"
                            aria-label={t.restoreRow({ date, reason })}
                            onClick={() => {
                              onRestore(entry);
                            }}
                          >
                            {t.restore}
                          </Button>
                          <IconButton
                            size="sm"
                            icon="folder"
                            label={t.showRow({ date })}
                            onClick={() => {
                              api.reveal({ target: "file", file: entry.file.token }).catch(toastError);
                            }}
                          />
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {entries.length > SAFETY_PAGE ? (
            <div className="br-safety__more">
              <p className="ow-muted ow-small">{t.count({ shown: shown.length, total: entries.length })}</p>
              {entries.length > limit ? (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    setLimit((current) => current + SAFETY_PAGE);
                  }}
                >
                  {dict.common.showMore({ n: Math.min(SAFETY_PAGE, entries.length - limit) })}
                </Button>
              ) : null}
            </div>
          ) : null}
        </>
      );
      break;
    }
  }

  const dir = safetyDirPath(info);
  return (
    <Card
      title={t.title}
      className="br-safety-card"
      actions={
        <>
          <IconButton
            icon="refresh"
            size="sm"
            label={t.refresh}
            onClick={() => {
              setManual((n) => n + 1);
            }}
          />
          {dir === null ? null : (
            <Button size="sm" icon="folder" onClick={revealSafetyDir}>
              {t.showFolder}
            </Button>
          )}
        </>
      }
    >
      <p className="ow-muted">{t.intro}</p>
      {dir === null ? null : <p className="ow-muted ow-small ow-mono ow-break ow-selectable">{dir}</p>}
      {body}
    </Card>
  );
}
