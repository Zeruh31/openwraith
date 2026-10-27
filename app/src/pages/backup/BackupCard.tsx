// "Back up": all four profiles (switches profiles, so it is confirmed in a
// WriteFlow first) or the active profile only (a pure read, no
// confirmation). The destination is chosen in the system save dialog; the
// backend hands back a token, never a path we could send.

import { useId, useRef, useState } from "react";

import { Button } from "../../components/Button";
import { Callout } from "../../components/Feedback";
import { Icon } from "../../components/Icon";
import { Card, KeyValueList } from "../../components/Layout";
import { OperationRunner, WriteFlow } from "../../flow/WriteFlow";
import { useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { AllProfilesPreview, BackupKind, BackupSaved, FileRef } from "../../ipc/bindings";
import { api } from "../../ipc/commands";
import { toastError } from "../../shell/actions";
import { useStatus } from "../../state/status";
import { useDeviceBlock } from "./availability";
import { useDemoAction, type BackupDemo } from "./demo";
import { numberList, profilesLabel } from "./texts";

/** What a saved backup looks like in a result dialog. */
export function BackupSavedView({ saved }: { saved: BackupSaved }) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.backupRestore.backup;
  return (
    <div className="ow-report br-saved">
      <Callout kind="success" title={t.saved}>
        <p>
          {t.savedSummary({
            profiles: profilesLabel(saved.profiles, dict, format),
            firmware: saved.firmware ?? dict.common.unknown,
          })}
        </p>
        <p>{t.savedAt({ date: format.dateTime(saved.created_at) })}</p>
      </Callout>
      <div className="br-saved__file">
        <p>
          <span className="ow-strong">{t.file}</span>{" "}
          <span className="ow-mono ow-break ow-selectable">{saved.file.file_name}</span>
        </p>
        <p className="ow-muted ow-small ow-mono ow-break ow-selectable">{saved.file.display_path}</p>
        <div className="ow-row">
          <Button
            size="sm"
            icon="folder"
            onClick={() => {
              api.reveal({ target: "file", file: saved.file.token }).catch(toastError);
            }}
          >
            {dict.common.actions.showInFolder}
          </Button>
        </div>
      </div>
      {saved.undecoded > 0 ? <p className="ow-muted">{t.undecoded({ n: saved.undecoded })}</p> : null}
      {saved.foreign_activity === null ? null : <Callout kind="warning">{t.foreignActivity}</Callout>}
    </div>
  );
}

/** The review of an all-profile backup: which profiles are visited, the file. */
export function AllProfilesReview({ preview }: { preview: AllProfilesPreview }) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.backupRestore.backup;
  return (
    <div className="ow-stack">
      <p>
        {preview.visits.length > 0
          ? t.reviewSwitches({ visits: numberList(preview.visits, format), original: preview.original })
          : t.reviewNoSwitches}
      </p>
      {preview.save_wait_ms > 0 ? <p>{t.reviewSaveWait({ seconds: format.seconds(preview.save_wait_ms) })}</p> : null}
      <KeyValueList
        items={[
          {
            key: "file",
            label: t.reviewFile,
            value: (
              <>
                <span className="ow-mono ow-break ow-selectable">{preview.destination.file_name}</span>
                <span className="br-path ow-muted ow-small ow-mono ow-break ow-selectable">
                  {preview.destination.display_path}
                </span>
              </>
            ),
          },
        ]}
      />
      <p className="ow-muted">{t.reviewStop}</p>
    </div>
  );
}

type Running = { kind: BackupKind; destination: FileRef };

/** Props of {@link BackupCard}. */
export interface BackupCardProps {
  /** The preview's demo action, if any. */
  demo: BackupDemo["run"];
}

/** The "Back up" card. */
export function BackupCard({ demo }: BackupCardProps) {
  const dict = useDict();
  const status = useStatus();
  const t = dict.backupRestore.backup;
  const block = useDeviceBlock();
  const reasonId = useId();
  const [running, setRunning] = useState<Running | null>(null);
  const choosing = useRef(false);

  const start = (kind: BackupKind) => {
    if (choosing.current || running !== null || block !== null) return;
    choosing.current = true;
    const titles = dict.backupRestore.dialog;
    const dialog = { title: kind === "all_profiles" ? titles.saveAll : titles.saveActive, filter: titles.filter };
    api
      .chooseBackupDestination(kind, dialog)
      .then(
        (destination) => {
          if (destination !== null) setRunning({ kind, destination });
        },
        (error: unknown) => {
          toastError(error);
        },
      )
      .finally(() => {
        choosing.current = false;
      });
  };

  useDemoAction(demo === "backup_all", () => {
    start("all_profiles");
  });
  useDemoAction(demo === "backup_active", () => {
    start("active");
  });

  const describedBy = block === null ? undefined : reasonId;
  return (
    <Card title={t.title} className="br-backup">
      <p className="ow-muted">{t.intro}</p>
      <div className="br-options">
        <div className="br-option">
          <div className="ow-row">
            <Button
              variant="primary"
              icon="backup"
              disabled={block !== null}
              aria-describedby={describedBy}
              onClick={() => {
                start("all_profiles");
              }}
            >
              {t.all}
            </Button>
            <span className="br-chip br-chip--accent">{t.recommended}</span>
          </div>
          <p className="ow-muted ow-small">{t.allHint}</p>
        </div>
        <div className="br-option">
          <div className="ow-row">
            <Button
              icon="backup"
              disabled={block !== null}
              aria-describedby={describedBy}
              onClick={() => {
                start("active");
              }}
            >
              {t.active}
            </Button>
          </div>
          <p className="ow-muted ow-small">
            {status.active_profile === null ? t.activeHintUnknown : t.activeHint({ n: status.active_profile })}
          </p>
        </div>
      </div>
      {block === null ? null : (
        <p id={reasonId} className="br-reason">
          <Icon name="info" size={16} />
          <span>{block}</span>
        </p>
      )}
      {running?.kind === "all_profiles" ? (
        <WriteFlow<AllProfilesPreview, BackupSaved>
          kind="backup_all"
          prepare={() => api.prepareAllProfileBackup(running.destination.token)}
          token={(preview) => preview.token}
          title={() => t.reviewTitle}
          review={(preview) => <AllProfilesReview preview={preview} />}
          confirmLabel={() => t.confirm}
          writesSettings={false}
          apply={(token, onProgress) => api.applyAllProfileBackup(token, onProgress)}
          result={(saved) => <BackupSavedView saved={saved} />}
          errorNote={() => t.noFileSaved}
          onClose={() => {
            setRunning(null);
          }}
        />
      ) : null}
      {running?.kind === "active" ? (
        <OperationRunner<BackupSaved>
          kind="backup_active"
          title={dict.flow.opKinds.backup_active}
          run={(onProgress) => api.backupActiveProfile(running.destination.token, onProgress)}
          result={(saved) => <BackupSavedView saved={saved} />}
          errorNote={() => t.noFileSaved}
          onClose={() => {
            setRunning(null);
          }}
        />
      ) : null}
    </Card>
  );
}
