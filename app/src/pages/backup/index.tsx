// The Backup & profiles page: the Recovery card (`#/backup?recover=1`, and
// whenever the keyboard looks reset), back up (all profiles or the active
// one), restore a backup (choose → profiles → compare → review → confirm →
// progress → report), share a profile (export, import), the automatic
// backups and the safety backups. `#/backup?restore=<token>` (Undo, the
// interrupted-operation banner) opens that backup in the wizard.

import { useEffect, useRef, useState } from "react";

import { PageHeader } from "../../components/Layout";
import { useDict } from "../../i18n";
import { previewUi } from "../../preview/ui";
import { navigate, useRoute } from "../../routes";
import { useStatus } from "../../state/status";
import { AutoBackups } from "./AutoBackups";
import { BackupCard } from "./BackupCard";
import type { BackupDemo } from "./demo";
import { useDemoAction } from "./demo";
import { RecoveryCard } from "./recovery/RecoveryCard";
import { RestoreCard } from "./RestoreCard";
import { SafetyBackups } from "./SafetyBackups";
import ShareProfile from "./share";
import { useBackupSource } from "./useBackupSource";
import "./backup.css";

/** Brings an element into view and focuses it. */
function reveal(element: HTMLElement | null): void {
  if (element === null) return;
  if (typeof element.scrollIntoView === "function") element.scrollIntoView({ block: "start" });
  element.focus();
}

/** The Backup & Restore page. */
export default function BackupRestorePage() {
  const dict = useDict();
  const route = useRoute();
  const source = useBackupSource();
  const demo = previewUi<BackupDemo>("backup");
  const [safetyKey, setSafetyKey] = useState(0);
  const [autoKey, setAutoKey] = useState(0);
  const [recoveryKey, setRecoveryKey] = useState(0);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const recoveryRef = useRef<HTMLSpanElement>(null);
  const autoRef = useRef<HTMLSpanElement>(null);
  const handled = useRef<string | null>(null);
  const routeToken = route.params["restore"];
  const recoverRoute = route.params["recover"] === "1";
  const autoRoute = route.params["section"] === "auto";
  const [demoOpensFile] = useState(
    () => demo?.run === "open_file" || (demo?.run === "compare" && routeToken === undefined),
  );
  const { openSafety, openFile, recheck, loadId } = source;
  const status = useStatus();
  /** Undo: the load after which the restore wizard is brought into view. */
  const revealAfter = useRef<number | null>(null);

  // The Recovery card: opened by the route (a notification, the tray) or by
  // a suspected reset, and kept open until closed (it then says what was
  // done, e.g. that the settings were kept).
  const recovery = status.recovery;
  const attention = recovery.reset_reported || recovery.firmware_changed || recovery.reset_suspected.length > 0;
  const [recoveryOpen, setRecoveryOpen] = useState(() => recoverRoute || attention);
  const [seen, setSeen] = useState({ attention, recoverRoute });
  if (seen.attention !== attention || seen.recoverRoute !== recoverRoute) {
    // Opened when a reset is suspected or the route asks for it (state derived during render).
    setSeen({ attention, recoverRoute });
    if (attention || recoverRoute) setRecoveryOpen(true);
  }
  const revealRecovery = useRef(recoverRoute);
  useEffect(() => {
    if (!recoverRoute) return;
    revealRecovery.current = true;
    // The route opened the card; the page's own route stays clean.
    navigate("backup");
  }, [recoverRoute]);
  useEffect(() => {
    if (!recoveryOpen || !revealRecovery.current) return;
    revealRecovery.current = false;
    reveal(recoveryRef.current);
  });

  // "Show automatic backups" (Settings, a notification): the card in view.
  useEffect(() => {
    if (!autoRoute) return;
    navigate("backup");
    reveal(autoRef.current);
  }, [autoRoute]);

  const focusRestore = () => {
    reveal(headingRef.current);
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

  /** The Recovery card restored or kept settings: new safety or automatic backups may exist. */
  const afterRecovery = () => {
    setSafetyKey((key) => key + 1);
    setAutoKey((key) => key + 1);
  };

  /** A restore or an import made a new safety backup (and may have ended a suspected reset). */
  const afterWrite = () => {
    afterRecovery();
    setRecoveryKey((key) => key + 1);
  };

  return (
    <div className="br-page">
      <PageHeader title={dict.backupRestore.title} description={dict.backupRestore.description} />
      {recoveryOpen ? (
        <RecoveryCard
          reloadKey={recoveryKey}
          demo={demo}
          titleRef={recoveryRef}
          onChanged={afterRecovery}
          onClose={() => {
            setRecoveryOpen(false);
          }}
        />
      ) : null}
      <div className="br-grid">
        <BackupCard demo={demo?.run} />
        <RestoreCard
          source={source}
          demo={demo}
          autoCompare={demo?.run === "compare"}
          headingRef={headingRef}
          onRestored={afterWrite}
        />
      </div>
      <ShareProfile demo={demo} onImported={afterWrite} />
      <AutoBackups
        reloadKey={autoKey}
        titleRef={autoRef}
        onRestore={(entry) => {
          openSafety(entry.file.token);
          focusRestore();
        }}
      />
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
