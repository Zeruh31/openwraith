// The Backup & Restore page: back up (all profiles or the active one),
// restore a backup (choose → profiles → compare → review → confirm →
// progress → report) and the list of safety backups. `#/backup?restore=<token>`
// (Undo, the interrupted-operation banner) opens that backup in the wizard.

import { useEffect, useRef, useState } from "react";

import { PageHeader } from "../../components/Layout";
import { useDict } from "../../i18n";
import { previewUi } from "../../preview/ui";
import { navigate, useRoute } from "../../routes";
import { useStatus } from "../../state/status";
import { BackupCard } from "./BackupCard";
import type { BackupDemo } from "./demo";
import { useDemoAction } from "./demo";
import { RestoreCard } from "./RestoreCard";
import { SafetyBackups } from "./SafetyBackups";
import { useBackupSource } from "./useBackupSource";
import "./backup.css";

/** The Backup & Restore page. */
export default function BackupRestorePage() {
  const dict = useDict();
  const route = useRoute();
  const source = useBackupSource();
  const demo = previewUi<BackupDemo>("backup");
  const [safetyKey, setSafetyKey] = useState(0);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const handled = useRef<string | null>(null);
  const routeToken = route.params["restore"];
  const [demoOpensFile] = useState(
    () => demo?.run === "open_file" || (demo?.run === "compare" && routeToken === undefined),
  );
  const { openSafety, openFile, recheck, loadId } = source;
  const status = useStatus();
  /** Undo: the load after which the restore wizard is brought into view. */
  const revealAfter = useRef<number | null>(null);

  const focusRestore = () => {
    const heading = headingRef.current;
    if (heading === null) return;
    if (typeof heading.scrollIntoView === "function") heading.scrollIntoView({ block: "start" });
    heading.focus();
  };

  // A backup opened while no keyboard was read can't say whether it fits the
  // keyboard (same model, same firmware); summarize it again once a keyboard
  // has been read, and after every new connection.
  const readKey = status.snapshot_revision === null ? null : status.generation;
  const lastReadKey = useRef(readKey);
  useEffect(() => {
    if (readKey === lastReadKey.current) return;
    lastReadKey.current = readKey;
    if (readKey !== null) recheck();
  }, [readKey, recheck]);

  // Undo and the interrupted-operation banner open a safety backup through the route.
  useEffect(() => {
    if (routeToken === undefined) {
      handled.current = null;
      return;
    }
    if (handled.current === routeToken) return;
    handled.current = routeToken;
    revealAfter.current = loadId;
    openSafety(routeToken);
    navigate("backup");
    // The page opens at the top (the Back up card): bring the restore
    // wizard into view at once, and again when the backup has loaded.
    focusRestore();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only a new route token matters
  }, [routeToken, openSafety]);

  useEffect(() => {
    const after = revealAfter.current;
    if (after === null || loadId <= after) return;
    revealAfter.current = null;
    focusRestore();
  }, [loadId]);

  useDemoAction(demoOpensFile, () => {
    openFile({ title: dict.backupRestore.dialog.open, filter: dict.backupRestore.dialog.filter });
  });

  return (
    <div className="br-page">
      <PageHeader title={dict.backupRestore.title} description={dict.backupRestore.description} />
      <div className="br-grid">
        <BackupCard demo={demo?.run} />
        <RestoreCard
          source={source}
          demo={demo}
          autoCompare={demo?.run === "compare"}
          headingRef={headingRef}
          onRestored={() => {
            setSafetyKey((key) => key + 1);
          }}
        />
      </div>
      <SafetyBackups
        reloadKey={safetyKey}
        onRestore={(entry) => {
          openSafety(entry.file.token);
          focusRestore();
        }}
      />
    </div>
  );
}
