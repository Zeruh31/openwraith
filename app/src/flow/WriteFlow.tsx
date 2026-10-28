// The confirmation flow of every write: prepare (read-only plan) → review →
// explicit Write → progress → result. Writes happen only through the
// backend's `apply_*` commands with the plan's single-use token, after the
// user pressed Write in this dialog.
//
// Rules enforced here:
// - Cancel has the initial focus; Write is armed after 500 ms and Enter
//   never activates it;
// - exactly one apply per confirmation (a double click applies once);
// - Write is disabled while the write gate is closed (the reasons are
//   shown), after a plan-invalidated notice (with "Review again"), and until
//   the vendor-site tick is given when it is required;
// - the running dialog can't be dismissed; Stop follows the cancel state.

import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { Button } from "../components/Button";
import { Dialog } from "../components/Dialog";
import { Callout } from "../components/Feedback";
import { useDict } from "../i18n";
import type { AppError, OpKind, PlanInvalidation, PlanToken, ProgressEvent } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { toAppError } from "../ipc/errors";
import { href, useRoute } from "../routes";
import { safetyDirPath, useAppInfo } from "../state/appInfo";
import { tryLock, unlock } from "../state/operation";
import { markReportSeen, useOperationWatch } from "../state/reports";
import { useNotices, useStatus } from "../state/status";
import { ErrorView, type WrittenNote } from "./ErrorView";
import { writeBlockText } from "./labels";
import { EMPTY_PROGRESS, reduceProgress, type ProgressModel } from "./progressModel";
import { ProgressPanel } from "./ProgressPanel";
import { useVendorConfirmation, VendorNote } from "./vendorTick";

/** A progress listener. */
export type OnProgress = (event: ProgressEvent) => void;

/** A page's own view of a refused prepare (see {@link WriteFlowProps.prepareError}). */
export interface PrepareErrorView {
  /** Shown instead of the standard error. */
  body: ReactNode;
  /** Relabels the retry button and can hold it back (with the reason shown) until the page's condition is met. */
  retry?: { label: string; disabledReason: string | null } | undefined;
}

/** Props of {@link WriteFlow}; `P` is the prepare result (a preview), `R` the apply result (a report). */
export interface WriteFlowProps<P, R> {
  kind: OpKind;
  /** Builds the plan (read-only); runs when the flow opens. */
  prepare: (onProgress: OnProgress) => Promise<P>;
  /** The plan's token; `null` means there is nothing to do. */
  token: (plan: P) => PlanToken | null;
  title: (plan: P) => string;
  /** The page's review body. */
  review: (plan: P) => ReactNode;
  /** Shown instead of the review when there is nothing to do. */
  nothingToDo?: ((plan: P) => ReactNode) | undefined;
  confirmLabel: (plan: P) => string;
  /** The write changes settings: shows the safety-backup line and the save-wait note. */
  writesSettings: boolean;
  /** Replaces the default safety-backup line (profile N, from the status). */
  safetyNote?: ((plan: P) => ReactNode) | undefined;
  /** A small dialog (the profile switch). */
  compact?: boolean | undefined;
  apply: (token: PlanToken, onProgress: OnProgress) => Promise<R>;
  /** The page's result body (e.g. `<EditReportView report={r}/>`). */
  result: (result: R) => ReactNode;
  /** Close right after the result arrives (the caller shows it, e.g. as a toast). */
  closeOnResult?: boolean | undefined;
  /** Called once when the flow closes, with the result if the write ran. */
  onClose: (result: R | null) => void;
  /** An extra line under a failed apply (e.g. "No backup file was saved."). */
  errorNote?: ((error: AppError) => ReactNode) | undefined;
  /** Milliseconds before Write can be pressed (default 500). */
  armDelayMs?: number | undefined;
  /**
   * A page's own view of a refused prepare, e.g. the confirmation a refusal
   * asks for (the retry then prepares with it); `null` keeps the standard
   * error view.
   */
  prepareError?: ((error: AppError) => PrepareErrorView | null) | undefined;
  /**
   * Review controls in the footer that are part of the plan's intent (e.g.
   * "Check that the keyboard kept it"): `reprepare` discards the plan and
   * prepares a new one, so Write is armed again only for the new plan.
   */
  reviewOptions?: ((plan: P, reprepare: () => void) => ReactNode) | undefined;
}

type Phase<P, R> =
  | { s: "preparing" }
  | { s: "prepare_error"; error: AppError }
  | { s: "nothing"; plan: P }
  | { s: "review"; plan: P; token: PlanToken; invalidated: PlanInvalidation | null }
  | { s: "running"; plan: P }
  | { s: "result"; plan: P; result: R }
  | { s: "apply_error"; plan: P; error: AppError };

/** Operations whose failures may still have switched profiles. */
const SWITCHING_KINDS: readonly OpKind[] = [
  "restore_prepare",
  "restore",
  "recovery_check",
  "backup_all",
  "import_prepare",
  "import",
];

function ignore(): void {
  // Errors of best-effort calls (discard, cancel) are not shown.
}

/** The confirmation flow of a write. Mount it to open it; it calls `onClose` once. */
export function WriteFlow<P, R>(props: WriteFlowProps<P, R>) {
  const dict = useDict();
  const status = useStatus();
  const info = useAppInfo();
  const vendor = useVendorConfirmation();
  const owner = useId();
  const route = useRoute();
  const routeKey = href(route);
  const propsRef = useRef(props);
  const [phase, setPhase] = useState<Phase<P, R>>({ s: "preparing" });
  const [progress, setProgress] = useState<ProgressModel>(EMPTY_PROGRESS);
  const [stopRequested, setStopRequested] = useState(false);
  const [armed, setArmed] = useState(false);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const started = useRef(false);
  const closed = useRef(false);
  const applying = useRef(false);
  const prepareSeq = useRef(0);
  const openedOn = useRef(routeKey);
  // While the apply runs, this flow watches the operation; when it unmounts
  // first, the shell shows the report instead (LastReportDialog).
  const watch = useOperationWatch();

  useEffect(() => {
    propsRef.current = props;
  });

  // The local operation lock, held while the flow is open (released by
  // `close`, or here when the flow unmounts otherwise).
  useEffect(() => {
    tryLock(owner);
    return () => {
      unlock(owner);
    };
  }, [owner]);

  const onProgress = (event: ProgressEvent) => {
    setProgress((model) => reduceProgress(model, event));
  };

  const close = (result: R | null) => {
    if (closed.current) return;
    closed.current = true;
    // Released in the same event that removes the flow: the opener is
    // enabled again when the dialog gives the focus back to it.
    unlock(owner);
    propsRef.current.onClose(result);
  };

  const runPrepare = () => {
    prepareSeq.current += 1;
    const seq = prepareSeq.current;
    applying.current = false;
    setPhase({ s: "preparing" });
    setProgress(EMPTY_PROGRESS);
    setStopRequested(false);
    propsRef.current
      .prepare(onProgress)
      .then((plan) => {
        const token = propsRef.current.token(plan);
        if (seq !== prepareSeq.current || closed.current) {
          if (token !== null) api.discardPlan(token).catch(ignore);
          return;
        }
        setPhase(token === null ? { s: "nothing", plan } : { s: "review", plan, token, invalidated: null });
      })
      .catch((error: unknown) => {
        if (seq !== prepareSeq.current || closed.current) return;
        setPhase({ s: "prepare_error", error: toAppError(error) });
      });
  };

  // Prepare once when the flow opens (a ref guard keeps StrictMode's second effect run from preparing twice).
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    runPrepare();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- runs once per flow
  }, []);

  // Write is armed a moment after the review appears.
  const reviewToken = phase.s === "review" ? phase.token : null;
  const armDelay = props.armDelayMs ?? 500;
  useEffect(() => {
    if (reviewToken === null) return undefined;
    const timer = window.setTimeout(() => {
      setArmed(true);
    }, armDelay);
    return () => {
      window.clearTimeout(timer);
      setArmed(false);
    };
  }, [reviewToken, armDelay]);

  // Keep the focus on the safe button of each phase (Cancel, then Close).
  useEffect(() => {
    if (phase.s === "review" || phase.s === "preparing") cancelRef.current?.focus();
    else closeRef.current?.focus();
  }, [phase.s]);

  // The backend invalidated the plan (another program, a profile change, a reload…).
  useNotices((notice) => {
    if (notice.notice !== "plan_invalidated") return;
    setPhase((current) =>
      current.s === "review" && current.invalidated === null ? { ...current, invalidated: notice.reason } : current,
    );
  });

  // Navigating away from a finished flow (Undo) closes it.
  const finished = phase.s === "result" ? phase.result : undefined;
  const settled = phase.s === "result" || phase.s === "apply_error" || phase.s === "prepare_error" || phase.s === "nothing";
  useEffect(() => {
    if (routeKey !== openedOn.current && settled) {
      close(finished ?? null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only route changes matter
  }, [routeKey]);

  const gate = status.write_gate;
  const vendorMissing = vendor.missing;
  const writeBlocked = phase.s !== "review" || phase.invalidated !== null || !gate.allowed || vendorMissing;

  const onWrite = () => {
    if (phase.s !== "review" || writeBlocked || !armed || applying.current) return;
    applying.current = true;
    vendor.acknowledge();
    const { plan, token } = phase;
    setPhase({ s: "running", plan });
    setProgress(EMPTY_PROGRESS);
    setStopRequested(false);
    watch.start();
    propsRef.current
      .apply(token, onProgress)
      .finally(watch.settle)
      .then((result) => {
        if (watch.mounted()) markReportSeen(result);
        if (propsRef.current.closeOnResult === true) {
          close(result);
        } else {
          setPhase({ s: "result", plan, result });
        }
      })
      .catch((error: unknown) => {
        setPhase({ s: "apply_error", plan, error: toAppError(error) });
      });
  };

  const reprepare = () => {
    if (phase.s === "review") api.discardPlan(phase.token).catch(ignore);
    runPrepare();
  };

  const onStop = () => {
    if (progress.op === null) return;
    setStopRequested(true);
    api.cancelOperation(progress.op).catch(ignore);
  };

  const onCancel = () => {
    if (phase.s === "preparing") {
      if (progress.op !== null) {
        onStop();
        return;
      }
      close(null);
      return;
    }
    if (phase.s === "review") {
      api.discardPlan(phase.token).catch(ignore);
    }
    close(phase.s === "result" ? phase.result : null);
  };

  const written: WrittenNote = SWITCHING_KINDS.includes(props.kind) ? "switches" : "nothing";
  // The review asks its question; preparing, running and the result have neutral titles.
  const title =
    phase.s === "preparing" || phase.s === "prepare_error"
      ? dict.flow.opKinds[props.kind]
      : phase.s === "review" || phase.s === "nothing"
        ? props.title(phase.plan)
        : dict.flow.titles[props.kind];
  const dismissable = phase.s !== "running" && phase.s !== "preparing";
  const dir = safetyDirPath(info) ?? "";

  let body: ReactNode;
  let actions: ReactNode;
  // What arms Write (the vendor-site tick, the write gate) stays in view below the plan.
  let footer: ReactNode = null;
  switch (phase.s) {
    case "preparing":
      body = (
        <ProgressPanel
          model={progress}
          waitingText={dict.flow.preparing}
          stopRequested={stopRequested}
          {...(progress.op === null ? {} : { onStop })}
        />
      );
      actions = (
        <Button ref={cancelRef} onClick={onCancel} disabled={stopRequested}>
          {dict.common.actions.cancel}
        </Button>
      );
      break;
    case "prepare_error": {
      const custom = props.prepareError?.(phase.error) ?? null;
      body = custom?.body ?? <ErrorView error={phase.error} written={written} />;
      actions = (
        <>
          <Button ref={closeRef} onClick={onCancel}>
            {dict.common.actions.close}
          </Button>
          <Button
            variant="primary"
            icon="refresh"
            disabledReason={custom?.retry?.disabledReason ?? null}
            onClick={runPrepare}
          >
            {custom?.retry?.label ?? dict.common.actions.retry}
          </Button>
        </>
      );
      break;
    }
    case "nothing":
      body = props.nothingToDo?.(phase.plan) ?? <Callout kind="info" title={dict.flow.headline.nothingToDo} />;
      actions = (
        <Button ref={closeRef} variant="primary" onClick={onCancel}>
          {dict.common.actions.close}
        </Button>
      );
      break;
    case "review":
      body = (
        <>
          <div className="ow-flow__review">{props.review(phase.plan)}</div>
          {/* The plan's options and notes scroll with it; only what arms Write stays pinned below. */}
          <div className="ow-flow__notes">
            {props.reviewOptions === undefined ? null : (
              <ReviewOptions plan={phase.plan} reprepare={reprepare} render={props.reviewOptions} />
            )}
            {props.writesSettings ? (
              <p>
                {props.safetyNote?.(phase.plan) ??
                  (status.active_profile === null
                    ? dict.flow.review.safetyNoteGeneric({ dir })
                    : dict.flow.review.safetyNote({ n: status.active_profile, dir }))}
              </p>
            ) : null}
            {status.firmware_tested === false ? (
              <p className="ow-muted">
                {dict.flow.review.firmwareNote({ firmware: status.device?.firmware ?? dict.common.unknown })}
              </p>
            ) : null}
            {props.writesSettings ? <p className="ow-muted">{dict.flow.review.saveNote}</p> : null}
          </div>
          {phase.invalidated === null ? null : (
            <Callout
              kind="warning"
              title={dict.flow.review.invalidated}
              role="alert"
              actions={
                <Button icon="refresh" onClick={runPrepare}>
                  {dict.common.actions.reviewAgain}
                </Button>
              }
            >
              <p>{dict.errors.planInvalidation[phase.invalidated]}</p>
            </Callout>
          )}
        </>
      );
      footer = (
        <>
          <VendorNote confirmation={vendor} />
          {gate.allowed ? null : (
            <Callout kind="warning" title={dict.flow.review.blocked}>
              <ul>
                {gate.reasons.map((reason) => (
                  <li key={reason}>{writeBlockText(reason, status, dict)}</li>
                ))}
              </ul>
            </Callout>
          )}
        </>
      );
      actions = (
        <>
          <Button ref={cancelRef} onClick={onCancel}>
            {dict.common.actions.cancel}
          </Button>
          <Button
            variant="primary"
            disabled={writeBlocked}
            aria-disabled={!armed || undefined}
            title={armed ? undefined : dict.flow.review.arming}
            disabledReason={
              vendorMissing && gate.allowed && phase.invalidated === null ? dict.flow.review.vendorTickNeeded : null
            }
            onKeyDown={(event) => {
              if (event.key === "Enter") event.preventDefault();
            }}
            onClick={onWrite}
          >
            {props.confirmLabel(phase.plan)}
          </Button>
        </>
      );
      break;
    case "running":
      body = <ProgressPanel model={progress} onStop={onStop} stopRequested={stopRequested} />;
      actions = undefined;
      break;
    case "result":
      body = props.result(phase.result);
      actions = (
        <Button ref={closeRef} variant="primary" onClick={onCancel}>
          {dict.common.actions.close}
        </Button>
      );
      break;
    case "apply_error":
      body = (
        <>
          <ErrorView error={phase.error} written={written} />
          {props.errorNote === undefined ? null : <p>{props.errorNote(phase.error)}</p>}
        </>
      );
      actions = (
        <Button ref={closeRef} onClick={onCancel}>
          {dict.common.actions.close}
        </Button>
      );
      break;
  }

  return (
    <Dialog
      open
      title={title}
      onClose={onCancel}
      dismissable={dismissable}
      size={props.compact === true ? "sm" : "lg"}
      initialFocus={phase.s === "review" || phase.s === "preparing" ? cancelRef : closeRef}
      className="ow-flow"
      footer={footer}
      {...(actions === undefined ? {} : { actions })}
    >
      {body}
    </Dialog>
  );
}

/** Renders a flow's review options (see {@link WriteFlowProps.reviewOptions}). */
function ReviewOptions<P>({
  plan,
  reprepare,
  render,
}: {
  plan: P;
  reprepare: () => void;
  render: (plan: P, reprepare: () => void) => ReactNode;
}) {
  return <>{render(plan, reprepare)}</>;
}

/** Props of {@link OperationRunner}. */
export interface OperationRunnerProps<R> {
  kind: OpKind;
  title: string;
  /** Starts the operation (read-only operations only; no confirmation). */
  run: (onProgress: OnProgress) => Promise<R>;
  result: (result: R) => ReactNode;
  onClose: (result: R | null) => void;
  /** An extra line under an error (e.g. "No backup file was saved."). */
  errorNote?: ((error: AppError) => ReactNode) | undefined;
  /**
   * What a failure may have changed on the keyboard. By default an export or
   * a backup of every profile "may have switched profiles"; an export from a
   * backup file never touches the keyboard and passes `"nothing"`.
   */
  written?: WrittenNote | undefined;
}

type RunPhase<R> = { s: "running" } | { s: "result"; result: R } | { s: "error"; error: AppError };

/** Runs a read-only operation with progress (e.g. backing up the active profile). */
export function OperationRunner<R>(props: OperationRunnerProps<R>) {
  const dict = useDict();
  const owner = useId();
  const propsRef = useRef(props);
  const [phase, setPhase] = useState<RunPhase<R>>({ s: "running" });
  const [progress, setProgress] = useState<ProgressModel>(EMPTY_PROGRESS);
  const [stopRequested, setStopRequested] = useState(false);
  const closeRef = useRef<HTMLButtonElement>(null);
  const started = useRef(false);
  const closed = useRef(false);
  const watch = useOperationWatch();

  useEffect(() => {
    propsRef.current = props;
  });

  useEffect(() => {
    tryLock(owner);
    return () => {
      unlock(owner);
    };
  }, [owner]);

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    // Watches the operation while mounted (see WriteFlow).
    watch.start();
    propsRef.current
      .run((event) => {
        setProgress((model) => reduceProgress(model, event));
      })
      .finally(watch.settle)
      .then((result) => {
        if (watch.mounted()) markReportSeen(result);
        setPhase({ s: "result", result });
      })
      .catch((error: unknown) => {
        setPhase({ s: "error", error: toAppError(error) });
      });
  }, [watch]);

  const close = () => {
    if (closed.current) return;
    closed.current = true;
    // As in WriteFlow: the opener is enabled when the focus returns to it.
    unlock(owner);
    propsRef.current.onClose(phase.s === "result" ? phase.result : null);
  };

  const onStop = () => {
    if (progress.op === null) return;
    setStopRequested(true);
    api.cancelOperation(progress.op).catch(ignore);
  };

  return (
    <Dialog
      open
      title={props.title}
      onClose={close}
      dismissable={phase.s !== "running"}
      size="md"
      initialFocus={closeRef}
      className="ow-flow"
      {...(phase.s === "running"
        ? {}
        : {
            actions: (
              <Button ref={closeRef} variant="primary" onClick={close}>
                {dict.common.actions.close}
              </Button>
            ),
          })}
    >
      {phase.s === "running" ? (
        <ProgressPanel
          model={progress}
          stopRequested={stopRequested}
          {...(progress.op === null ? {} : { onStop })}
        />
      ) : phase.s === "result" ? (
        props.result(phase.result)
      ) : (
        <>
          <ErrorView
            error={phase.error}
            written={props.written ?? (props.kind === "backup_all" || props.kind === "export" ? "switches" : "nothing")}
          />
          {props.errorNote === undefined ? null : <p>{props.errorNote(phase.error)}</p>}
        </>
      )}
    </Dialog>
  );
}
