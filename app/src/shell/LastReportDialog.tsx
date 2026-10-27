// The report of an operation whose flow went away while it ran: a reloaded
// window (the operation was busy at start-up), a page that unmounted, history
// navigation. When the status shows that such an operation ended and no
// mounted flow watched it, the report is fetched (`get_last_report`) and
// shown here, with its safety backup and Undo. Reports this window already
// showed are not shown again.

import { lazy, Suspense, useEffect, useRef, useState, type ReactNode } from "react";

import { Button } from "../components/Button";
import { Dialog } from "../components/Dialog";
import { Spinner } from "../components/Feedback";
import { warningText } from "../flow/labels";
import { EditReportView } from "../flow/Report";
import { getDict, useDict } from "../i18n";
import { getFormat } from "../i18n/format";
import type { LastReport, OpInfo, OpKind } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { href, navigate, useRoute } from "../routes";
import { appliesPending, isReportSeen, markReportSeen, reportOf } from "../state/reports";
import { statusStore } from "../state/status";
import { toast } from "../state/toasts";
import { readAgain } from "./actions";

/** Operations that leave a report. */
const REPORT_KINDS: readonly OpKind[] = [
  "trigger_edit",
  "light_edit",
  "restore",
  "profile_switch",
  "backup_all",
  "backup_active",
];

// The backup page's report views, loaded only when needed.
const RestoreResult = lazy(() => import("../pages/backup/RestoreResult").then((m) => ({ default: m.RestoreResult })));
const BackupSavedView = lazy(() => import("../pages/backup/BackupCard").then((m) => ({ default: m.BackupSavedView })));

function ignore(): void {
  // A report that can't be fetched is not shown (the safety-backup list
  // still has the safety backup).
}

/** Shows a profile switch's report as the header does: a toast. */
function toastSwitch(last: Extract<LastReport, { kind: "profile_switch" }>): void {
  const dict = getDict();
  const format = getFormat();
  const report = last.report;
  toast({
    kind: "success",
    title: dict.shell.notices.switched({ n: report.active_profile_now ?? report.to }),
    body: report.warnings.length > 0 ? dict.shell.notices.switchWarnings : undefined,
    details:
      report.warnings.length > 0 ? report.warnings.map((w) => warningText(w, dict, format)).join("\n") : undefined,
  });
}

/** The dialog with the report of an operation no flow watched to the end. */
export function LastReportDialog() {
  const dict = useDict();
  const [shown, setShown] = useState<LastReport | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  // Undo (in the report's footer) navigates: the dialog closes then.
  const routeKey = href(useRoute());
  const shownOn = useRef(routeKey);
  useEffect(() => {
    if (routeKey !== shownOn.current) {
      shownOn.current = routeKey;
      setShown(null);
    }
  }, [routeKey]);

  useEffect(() => {
    let alive = true;
    // What exists at start-up was shown (or not) by an earlier window.
    api
      .getLastReport()
      .then((last) => {
        if (last !== null) markReportSeen(reportOf(last));
      })
      .catch(ignore);
    let previous: OpInfo | null = statusStore.get().busy;
    const check = () => {
      const busy = statusStore.get().busy;
      const ended = previous !== null && (busy === null || busy.op !== previous.op) ? previous : null;
      previous = busy;
      if (ended === null || !REPORT_KINDS.includes(ended.kind) || appliesPending()) return;
      api
        .getLastReport()
        .then((last) => {
          if (!alive || last === null || isReportSeen(last) || appliesPending()) return;
          markReportSeen(reportOf(last));
          if (last.kind === "profile_switch") {
            toastSwitch(last);
          } else {
            setShown(last);
          }
        })
        .catch(ignore);
    };
    const unsubscribe = statusStore.subscribe(check);
    // A reload in the middle of a write: F5 / Ctrl+R are refused while a
    // flow watches a running operation (Tauri can't turn them off).
    const onKey = (event: KeyboardEvent) => {
      const reload = event.key === "F5" || ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "r");
      if (reload && appliesPending()) event.preventDefault();
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      alive = false;
      unsubscribe();
      window.removeEventListener("keydown", onKey, true);
    };
  }, []);

  if (shown === null) return null;
  const close = () => {
    setShown(null);
  };
  let body: ReactNode;
  switch (shown.kind) {
    case "edit":
      body = <EditReportView report={shown.report} />;
      break;
    case "restore":
      body = (
        <RestoreResult
          report={shown.report}
          onRunAgain={() => {
            close();
            navigate("backup");
          }}
          onReadAgain={() => {
            close();
            readAgain();
          }}
        />
      );
      break;
    case "backup":
      body = <BackupSavedView saved={shown.saved} />;
      break;
    case "profile_switch":
      body = null;
      break;
  }
  return (
    <Dialog
      open
      title={dict.shell.lastReport.title}
      onClose={close}
      size="lg"
      initialFocus={closeRef}
      className="ow-flow"
      actions={
        <Button ref={closeRef} variant="primary" onClick={close}>
          {dict.common.actions.close}
        </Button>
      }
    >
      <p className="ow-muted">{dict.shell.lastReport.intro}</p>
      <Suspense fallback={<Spinner label={dict.common.loading} />}>{body}</Suspense>
    </Dialog>
  );
}
