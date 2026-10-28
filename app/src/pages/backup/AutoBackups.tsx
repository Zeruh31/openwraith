// The automatic backups: the copies of each profile OpenWraith saved when it
// read the keyboard and the profile had changed (newest first), with
// "Restore…" (opens it in the restore wizard: the normal compare → review →
// confirm flow), "Show in folder" and "Pin" (a pinned backup is never
// deleted). OpenWraith keeps the newest N per profile; pinned ones and
// safety backups are never deleted. Listing reads file names only; nothing is
// loaded until one is chosen, and nothing here talks to the keyboard.

import { useEffect, useState, type ReactNode, type Ref } from "react";

import { Button, IconButton } from "../../components/Button";
import { Callout, EmptyState, Spinner } from "../../components/Feedback";
import { Card } from "../../components/Layout";
import { ErrorView } from "../../flow/ErrorView";
import { useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { AppError, AutoBackupEntry, AutoBackupsView, FileToken } from "../../ipc/bindings";
import { api } from "../../ipc/commands";
import { toAppError } from "../../ipc/errors";
import { navigate } from "../../routes";
import { toastError } from "../../shell/actions";
import { useStatus } from "../../state/status";

/** Rows shown at first and added by "Show more". */
export const AUTO_PAGE = 20;

type ListState = { s: "loading" } | { s: "ok"; view: AutoBackupsView } | { s: "error"; error: AppError };

/** Props of {@link AutoBackups}. */
export interface AutoBackupsProps {
  /** Changing it reads the list again (after a restore, or after the Recovery card kept the settings). */
  reloadKey: number;
  /** Opens the backup in the restore wizard. */
  onRestore: (entry: AutoBackupEntry) => void;
  /** The card's title (focused when a route asks for the automatic backups). */
  titleRef?: Ref<HTMLSpanElement> | undefined;
}

/** The "Automatic backups" card. */
export function AutoBackups({ reloadKey, onRestore, titleRef }: AutoBackupsProps) {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const t = dict.backupRestore.autoBackups;
  const [state, setState] = useState<ListState>({ s: "loading" });
  const [limit, setLimit] = useState(AUTO_PAGE);
  const [manual, setManual] = useState(0);
  const [pinning, setPinning] = useState<FileToken | null>(null);

  // Every complete read may add one: read the list again after each.
  const readKey = `${status.generation}|${status.snapshot_revision ?? "none"}`;
  useEffect(() => {
    let alive = true;
    api.listAutoBackups().then(
      (view) => {
        if (alive) setState({ s: "ok", view });
      },
      (error: unknown) => {
        if (alive) setState({ s: "error", error: toAppError(error) });
      },
    );
    return () => {
      alive = false;
    };
  }, [reloadKey, manual, readKey]);

  const togglePin = (entry: AutoBackupEntry) => {
    if (pinning !== null) return;
    setPinning(entry.file.token);
    api
      .pinAutoBackup(entry.file.token, !entry.pinned)
      .then(
        (view) => {
          setState({ s: "ok", view });
        },
        (error: unknown) => {
          toastError(error);
        },
      )
      .finally(() => {
        setPinning(null);
      });
  };

  let body: ReactNode;
  let keep: number | null = null;
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
      const view = state.view;
      keep = view.keep;
      const entries = view.entries;
      const shown = entries.slice(0, limit);
      body = (
        <>
          {view.enabled ? null : (
            <Callout
              kind="info"
              actions={
                <Button
                  size="sm"
                  icon="settings"
                  onClick={() => {
                    navigate({ page: "settings", params: { tab: "background" } });
                  }}
                >
                  {t.openSettings}
                </Button>
              }
            >
              <p>{t.off}</p>
            </Callout>
          )}
          {view.connected_keyboard || entries.length === 0 ? null : (
            <p className="ow-muted ow-small">{t.everyKeyboard}</p>
          )}
          {view.unidentified ? <p className="ow-muted ow-small">{t.unidentified}</p> : null}
          {entries.length === 0 ? (
            <EmptyState icon="backup" title={t.empty} />
          ) : (
            <>
              <div className="ow-table-wrap">
                <table className="ow-table br-safety br-auto">
                  <thead>
                    <tr>
                      <th scope="col" className="ow-align-start">
                        {t.columns.made}
                      </th>
                      <th scope="col" className="ow-align-start">
                        {t.columns.profile}
                      </th>
                      <th scope="col" className="ow-align-start">
                        {t.columns.reason}
                      </th>
                      <th scope="col" className="ow-align-end">
                        <span className="sr-only">{t.columns.actions}</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {shown.map((entry) => {
                      // Seconds: several reads within one minute make backups that must still be told apart.
                      const date = format.dateTimeSeconds(entry.created_at);
                      const names = { n: entry.profile, date };
                      return (
                        <tr key={entry.file.token}>
                          <th scope="row" className="ow-align-start ow-num br-safety__date">
                            {date}
                          </th>
                          <td>{dict.common.profile({ n: entry.profile })}</td>
                          <td>
                            <span className="br-auto__reason">
                              <span>{t.reasons[entry.reason]}</span>
                              {entry.pinned ? <span className="br-chip br-chip--accent">{t.kept}</span> : null}
                            </span>
                          </td>
                          <td className="ow-align-end">
                            <div className="br-safety__actions">
                              <Button
                                size="sm"
                                icon="restore"
                                aria-label={t.restoreRow(names)}
                                onClick={() => {
                                  onRestore(entry);
                                }}
                              >
                                {t.restore}
                              </Button>
                              <Button
                                size="sm"
                                icon="lock"
                                variant={entry.pinned ? "secondary" : "ghost"}
                                className={entry.pinned ? "br-auto__pin br-auto__pin--on" : "br-auto__pin"}
                                aria-pressed={entry.pinned}
                                aria-label={t.keepRow(names)}
                                title={t.keepHint}
                                disabled={pinning !== null}
                                onClick={() => {
                                  togglePin(entry);
                                }}
                              >
                                {t.keep}
                              </Button>
                              <IconButton
                                size="sm"
                                icon="folder"
                                label={t.showRow(names)}
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
              {entries.length > AUTO_PAGE ? (
                <div className="br-safety__more">
                  <p className="ow-muted ow-small">{t.count({ shown: shown.length, total: entries.length })}</p>
                  {entries.length > limit ? (
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => {
                        setLimit((current) => current + AUTO_PAGE);
                      }}
                    >
                      {dict.common.showMore({ n: Math.min(AUTO_PAGE, entries.length - limit) })}
                    </Button>
                  ) : null}
                </div>
              ) : null}
            </>
          )}
        </>
      );
      break;
    }
  }

  return (
    <Card
      title={
        <span ref={titleRef} tabIndex={-1} className="br-auto-card__title">
          {t.title}
        </span>
      }
      className="br-auto-card"
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
          <Button
            size="sm"
            icon="folder"
            onClick={() => {
              api.reveal({ target: "auto_backup_dir" }).catch(toastError);
            }}
          >
            {t.showFolder}
          </Button>
        </>
      }
    >
      {keep === null ? null : <p className="ow-muted">{t.intro({ keep })}</p>}
      {body}
    </Card>
  );
}
