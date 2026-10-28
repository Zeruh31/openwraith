// Actuation > Key depth: a live view of how far each key is pressed. Start
// opens the keyboard's travel test (no plan dialog, but the wraith.software
// tick is still required); the caps fill from the top with the depth in
// millimetres, the deepest point is held, the key that moved last (or the
// selected one) gets a gauge and a strip chart, and "Show where keys
// trigger" estimates each key's state from its settings (labelled as an
// estimate). The view stops by itself when the tab or page is left, the
// window is hidden, the keyboard goes away or resets, another program or a
// profile change appears, or after 5 minutes without movement ("Paused…
// Resume"). While it runs the backend is busy, so every other write is
// refused; the tab says so. A test another run may have left open on the
// keyboard is offered "Stop it". The view is silent for screen readers
// unless "Announce" is on (at most four times a second).
//
// While the view runs, the keys being tested don't act on the page: they
// would scroll it (Space, arrows, Page Up/Down, Home/End) or press the
// focused control. Keys without Ctrl, Alt or Win are held back (Tab too: it
// is a key users test), except in a text field; Esc stops the view. A view
// this tab didn't start (the tab was left and opened again before it ended)
// still gets Stop.
//
// A stop sent right after Start can reach the backend before the view is
// registered there (the start runs on a worker thread, the stop doesn't): it
// is sent again while this tab's start is on its way, so leaving the page or
// hiding the window at once still keeps the test from running.

import { useCallback, useEffect, useId, useMemo, useRef, useState, type RefObject } from "react";

import { Button } from "../../../components/Button";
import { Checkbox, Switch } from "../../../components/Controls";
import { Callout } from "../../../components/Feedback";
import { Card, PageHeader, TechnicalDetails } from "../../../components/Layout";
import { KeyboardView, type KeyVisual } from "../../../components/keyboard";
import { TravelGauge, W75_TRAVEL_MM } from "../../../components/TravelGauge";
import { ErrorView } from "../../../flow/ErrorView";
import { useVendorConfirmation, VendorNote } from "../../../flow/vendorTick";
import { keyName, useDict, type Dict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type { AppError, DepthStopRequest, DepthViewReport, TravelCloseView, TriggerValue } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { tryLock, unlock } from "../../../state/operation";
import { useSnapshot } from "../../../state/snapshot";
import { useStatus } from "../../../state/status";
import { toast } from "../../../state/toasts";
import { isRtOn, KEY_NAMES, keysByIndex, legendOf, unitOf, valueTexts } from "../model";
import { travelUnits } from "../deadband/model";
import { applyFrame, clearPeaks, EMPTY_DEPTH, type DepthModel } from "./model";
import { StripChart } from "./StripChart";
import "../actuation.css";

/** The view's phase. */
type Phase = "idle" | "running" | "stopping";

/** How often "Announce" may speak, in milliseconds (at most four times a second). */
const ANNOUNCE_MS = 250;

/** How often a stop the backend couldn't apply yet is sent again, and for how long at most (ms). */
const STOP_RETRY_MS = 40;
const STOP_RETRY_LIMIT_MS = 5000;

function ignore(): void {
  // A stop that comes after the view ended is harmless.
}

/**
 * Sends a stop of the live view. The backend registers the view on a worker
 * thread a moment after Start (the start command runs asynchronously, the
 * stop doesn't), so a stop sent in that gap finds no view and answers
 * `false`. While `starting()` (this tab's start hasn't ended), the stop is
 * sent again until the backend takes it: the view then stops at once, or
 * never opens the keyboard's test. Resolves with the last answer.
 */
export async function sendDepthStop(reason: DepthStopRequest, starting: () => boolean): Promise<boolean> {
  const until = Date.now() + STOP_RETRY_LIMIT_MS;
  for (;;) {
    const stopped = await api.stopDepthView(reason);
    if (stopped || !starting() || Date.now() >= until) return stopped;
    await new Promise<void>((resolve) => {
      window.setTimeout(resolve, STOP_RETRY_MS);
    });
  }
}

/** Whether a key event belongs to a text field (typed text is never held back). */
function inTextField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if (!(target instanceof HTMLInputElement)) return false;
  return !["checkbox", "radio", "button", "submit", "reset", "range", "color", "file"].includes(target.type);
}

/**
 * While `active`: the keys being tested don't scroll the page or press the
 * focused control (keydown and keyup, in the capture phase: Space presses a
 * button on keyup). Keys with Ctrl, Alt or Win, and keys typed into a text
 * field, pass. Esc calls `onEscape`.
 */
export function useHeldKeys(active: boolean, onEscape: () => void): void {
  const escapeRef = useRef(onEscape);
  useEffect(() => {
    escapeRef.current = onEscape;
  });
  useEffect(() => {
    if (!active) return undefined;
    const hold = (event: KeyboardEvent) => {
      if (event.ctrlKey || event.altKey || event.metaKey) return;
      if (inTextField(event.target)) return;
      event.preventDefault();
      event.stopPropagation();
      if (event.type === "keydown" && event.key === "Escape") escapeRef.current();
    };
    window.addEventListener("keydown", hold, true);
    window.addEventListener("keyup", hold, true);
    return () => {
      window.removeEventListener("keydown", hold, true);
      window.removeEventListener("keyup", hold, true);
    };
  }, [active]);
}

/** Moves the focus off the Start button to the board, and the board into view. */
function focusBoard(board: RefObject<HTMLDivElement | null>): void {
  const element = board.current;
  if (element === null) return;
  element.focus({ preventScroll: true });
  const reduce = typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (typeof element.scrollIntoView === "function") element.scrollIntoView({ block: "start", behavior: reduce ? "auto" : "smooth" });
}

/** What the close of the keyboard's test means, or `null` when it was confirmed. */
export function closeText(close: TravelCloseView, dict: Dict): string | null {
  switch (close.close) {
    case "acked":
      return null;
    case "not_acked":
      return dict.actuation.depth.close.notAcked;
    case "not_sent":
      return dict.actuation.depth.close.notSent;
  }
}

/** The Key depth tab. */
export default function DepthTab() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const { snapshot } = useSnapshot();
  const vendor = useVendorConfirmation();
  const owner = useId();
  const t = dict.actuation.depth;

  const [phase, setPhase] = useState<Phase>("idle");
  const [model, setModel] = useState<DepthModel>(EMPTY_DEPTH);
  const [report, setReport] = useState<DepthViewReport | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [mayBeOpen, setMayBeOpen] = useState(false);
  const [closing, setClosing] = useState(false);
  const [holdPeak, setHoldPeak] = useState(true);
  const [showTrigger, setShowTrigger] = useState(false);
  const [announce, setAnnounce] = useState(false);
  const [announcement, setAnnouncement] = useState("");
  const [pinned, setPinned] = useState<number | null>(null);

  const unitUm = unitOf(snapshot);
  const travel = travelUnits(unitUm);
  const texts = useMemo(() => valueTexts(dict, format, unitUm), [dict, format, unitUm]);
  const triggerKeys = useMemo(() => keysByIndex(snapshot), [snapshot]);
  const settings = useMemo(() => {
    const map = new Map<number, TriggerValue>();
    for (const [index, key] of triggerKeys) if (key.value !== null) map.set(index, key.value);
    return map;
  }, [triggerKeys]);

  // The frame handler sees the latest settings without restarting the view.
  const settingsRef = useRef(settings);
  const holdRef = useRef(holdPeak);
  useEffect(() => {
    settingsRef.current = settings;
    holdRef.current = holdPeak;
  });

  const running = useRef(false);
  const mounted = useRef(true);
  const boardRef = useRef<HTMLDivElement>(null);
  // This tab's start hasn't ended (a stop that found no view yet is sent again meanwhile).
  const starting = useCallback(() => running.current, []);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      // Leaving the tab or the page stops the view.
      if (running.current) sendDepthStop("page_left", starting).catch(ignore);
    };
  }, [starting]);

  // A hidden or minimised window stops it too.
  useEffect(() => {
    const onVisibility = () => {
      if (document.visibilityState === "hidden" && running.current) sendDepthStop("window_hidden", starting).catch(ignore);
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [starting]);

  // A view this tab didn't start (it was left and opened again before the view ended).
  const otherOp = phase === "idle" && status.busy?.kind === "depth_view" ? status.busy.op : null;
  const otherView = otherOp !== null;
  // The other view this tab asked to stop (Stop stays enabled for a newer one).
  const [stoppingOp, setStoppingOp] = useState<string | null>(null);
  const stoppingOther = otherOp !== null && stoppingOp === otherOp;

  const ready = status.connection.state === "ready";
  const travelState = status.travel;
  const stray = travelState.state === "maybe_open" || travelState.stray_reports > 0 || travelState.marker_left || mayBeOpen;

  // Why Start can't be used now.
  let startReason: string | null = null;
  if (!status.write_gate.allowed) startReason = dict.errors.writeBlocked[status.write_gate.reasons[0] ?? "not_connected"];
  else if (!ready) startReason = dict.errors.writeBlocked.not_connected;
  else if (status.busy !== null) startReason = dict.errors.writeBlocked.busy;
  else if (vendor.missing) startReason = dict.flow.review.vendorTickNeeded;

  const start = () => {
    if (running.current || startReason !== null) return;
    if (!tryLock(owner)) return;
    running.current = true;
    vendor.acknowledge();
    setPhase("running");
    setReport(null);
    setError(null);
    setModel(EMPTY_DEPTH);
    focusBoard(boardRef);
    api
      .startDepthView((frame) => {
        if (!mounted.current) return;
        setModel((current) => applyFrame(current, frame, settingsRef.current, holdRef.current));
      })
      .then((result) => {
        if (!mounted.current) return;
        setReport(result);
        if (result.may_be_open) setMayBeOpen(true);
      })
      .catch((cause: unknown) => {
        if (mounted.current) setError(toAppError(cause));
      })
      .finally(() => {
        running.current = false;
        unlock(owner);
        if (mounted.current) setPhase("idle");
      });
  };

  const stop = () => {
    if (otherOp !== null) {
      if (stoppingOther) return;
      setStoppingOp(otherOp);
      api
        .stopDepthView("requested")
        .then((stopped) => {
          // (No view any more: the busy state ends it here.)
          if (!stopped && mounted.current) setStoppingOp(null);
        })
        .catch(() => {
          if (mounted.current) setStoppingOp(null);
        });
      return;
    }
    if (!running.current || phase === "stopping") return;
    setPhase("stopping");
    sendDepthStop("requested", starting)
      .then((stopped) => {
        // The backend found no view to stop: offer Stop again while this one still runs.
        if (!stopped && mounted.current && running.current) setPhase("running");
      })
      .catch(() => {
        if (mounted.current && running.current) setPhase("running");
      });
  };

  useHeldKeys(phase !== "idle", stop);

  // "Stop it": closes a test that may still be open on the keyboard (a user click only).
  const closeStray = () => {
    if (closing || running.current) return;
    if (vendor.missing) return;
    setClosing(true);
    vendor.acknowledge();
    api
      .closeStrayTravelTest()
      .then((result) => {
        const uncertain = closeText(result.close, dict);
        if (uncertain === null) {
          setMayBeOpen(false);
          toast({ kind: "success", title: t.stray.closed });
        } else {
          toast({ kind: "warning", title: uncertain });
        }
      })
      .catch((cause: unknown) => {
        const appError = toAppError(cause);
        toast({ kind: "danger", title: dict.errors.kind[appError.kind].title, details: appError.message });
      })
      .finally(() => {
        if (mounted.current) setClosing(false);
      });
  };

  let strayReason: string | null = null;
  if (phase !== "idle") strayReason = t.stray.whileRunning;
  else if (!ready) strayReason = dict.errors.writeBlocked.not_connected;
  else if (status.busy !== null) strayReason = dict.errors.writeBlocked.busy;
  else if (vendor.missing) strayReason = dict.flow.review.vendorTickNeeded;

  // The key the gauge and the chart follow: the selected one, else the one that moved last.
  const followed = pinned ?? model.last;
  const followedDepth = followed === null ? null : (model.keys.get(followed) ?? { depth: 0, peak: 0 });
  const followedValue = followed === null ? undefined : settings.get(followed);
  const nameOf = useCallback(
    (index: number) => keyName(dict, triggerKeys.get(index)?.key.name ?? KEY_NAMES.get(index) ?? String(index)),
    [dict, triggerKeys],
  );

  // Announce: the followed key's depth, at most four times a second, only while running.
  const spoken =
    followed === null || followedDepth === null ? "" : t.announce.text({ key: nameOf(followed), value: texts.mm(followedDepth.depth) });
  const spokenRef = useRef(spoken);
  useEffect(() => {
    spokenRef.current = spoken;
  });
  useEffect(() => {
    if (!announce || phase === "idle") return undefined;
    const timer = window.setInterval(() => {
      setAnnouncement((current) => (current === spokenRef.current ? current : spokenRef.current));
    }, ANNOUNCE_MS);
    return () => {
      window.clearInterval(timer);
    };
  }, [announce, phase]);

  const keyVisual = useCallback(
    (index: number): KeyVisual => {
      const libraryName = triggerKeys.get(index)?.key.name ?? KEY_NAMES.get(index) ?? String(index);
      const name = keyName(dict, libraryName);
      const legend = legendOf(dict, libraryName);
      const entry = model.keys.get(index);
      if (entry === undefined) return { legend, name, description: t.keyInfo.rest };
      const showsPeak = entry.depth === 0 && holdPeak && entry.peak > 0;
      const on = showTrigger ? (model.estimate.get(index)?.on ?? false) : false;
      const parts = [t.keyInfo.now({ value: texts.mm(entry.depth) })];
      if (holdPeak && entry.peak > 0) parts.push(t.keyInfo.peak({ value: texts.mm(entry.peak) }));
      if (showTrigger) parts.push(on ? t.keyInfo.on : t.keyInfo.off);
      return {
        legend,
        name,
        fill: entry.depth / travel,
        value: entry.depth > 0 ? texts.number(entry.depth) : showsPeak ? texts.number(entry.peak) : null,
        chip: showsPeak ? t.peakChip : null,
        badge: on ? t.onBadge : null,
        emphasis: entry.depth > 0,
        description: parts.join(", "),
      };
    },
    [triggerKeys, dict, model, t, holdPeak, showTrigger, texts, travel],
  );

  const toMm = (units: number) => (units * unitUm) / 1000;
  const formatMm = (mm: number) => texts.mm(Math.round((mm * 1000) / unitUm));
  const rtOn = followedValue !== undefined && isRtOn(followedValue);
  const stoppedText = report === null || report.reason === "requested" || report.reason === "page_left" ? null : t.stopped[report.reason];
  const closeNote = report === null ? null : closeText(report.close, dict);
  const movedKeys = [...model.keys.entries()].sort(([a], [b]) => a - b);

  return (
    <div className="act-page act-depth">
      <PageHeader title={dict.shell.nav.actuation} description={t.description} />

      {stray && phase === "idle" ? (
        <Callout
          kind="warning"
          title={t.stray.title}
          role="status"
          actions={
            <Button size="sm" icon="stop" disabledReason={closing ? null : strayReason} disabled={closing} onClick={closeStray}>
              {t.stray.stop}
            </Button>
          }
        >
          {t.stray.body}
        </Callout>
      ) : null}

      <Card title={t.controls.title} className="act-depth-controls">
        <div className="act-depth-controls__row">
          {otherView ? (
            <Button variant="primary" icon="stop" disabled={stoppingOther} onClick={stop}>
              {stoppingOther ? t.controls.stopping : t.controls.stop}
            </Button>
          ) : phase === "idle" ? (
            <Button variant="primary" disabledReason={startReason} onClick={start}>
              {report?.reason === "no_movement" ? t.controls.resume : t.controls.start}
            </Button>
          ) : (
            <Button variant="primary" icon="stop" disabled={phase === "stopping"} onClick={stop}>
              {phase === "stopping" ? t.controls.stopping : t.controls.stop}
            </Button>
          )}
          <span className="ow-small ow-muted" role="status">
            {otherView
              ? t.controls.otherView
              : phase === "idle"
                ? t.controls.idle
                : status.travel.state === "open"
                  ? t.controls.live
                  : t.controls.opening}
          </span>
        </div>
        {phase === "idle" ? null : <p className="ow-small ow-muted">{t.controls.keysHeld}</p>}
        {vendor.required && phase === "idle" ? <VendorNote confirmation={vendor} /> : null}
        <p className="ow-small">{t.note}</p>
        <div className="act-depth-controls__options">
          <Switch checked={holdPeak} label={t.options.holdPeak} onChange={setHoldPeak} />
          <Button
            size="sm"
            disabled={model.keys.size === 0}
            onClick={() => {
              setModel((current) => clearPeaks(current));
            }}
          >
            {t.options.clearPeaks}
          </Button>
          <Switch checked={showTrigger} label={t.options.showTrigger} onChange={setShowTrigger} />
          <Checkbox
            checked={announce}
            label={t.options.announce}
            description={t.options.announceHint}
            onChange={setAnnounce}
          />
        </div>
        {showTrigger ? <p className="ow-small ow-muted">{t.options.estimatedNote}</p> : null}
        {phase === "running" ? <p className="ow-small ow-muted">{t.controls.blocksWrites}</p> : null}
      </Card>

      {error === null ? null : <ErrorView error={error} />}
      {stoppedText === null ? null : (
        <Callout
          kind={report?.reason === "no_movement" || report?.reason === "time_limit" ? "info" : "warning"}
          title={stoppedText}
          role="status"
          actions={
            report?.reason === "no_movement" ? (
              <Button size="sm" disabledReason={startReason} onClick={start}>
                {t.controls.resume}
              </Button>
            ) : undefined
          }
        >
          {closeNote === null ? null : <p>{closeNote}</p>}
          {report === null ? null : (
            <TechnicalDetails>
              {t.technical({
                reports: report.reports,
                dropped: report.dropped,
                seconds: format.seconds(report.open_ms),
                close: report.close.close === "not_sent" ? `not_sent: ${report.close.reason}` : report.close.close,
              })}
            </TechnicalDetails>
          )}
        </Callout>
      )}

      <Card title={t.board.title} className="act-board-card">
        <div ref={boardRef} tabIndex={-1} className="act-board act-depth-board">
          <KeyboardView
            label={t.board.label}
            keyVisual={keyVisual}
            selection="single"
            selected={pinned === null ? new Set<number>() : new Set([pinned])}
            onSelectionChange={(next) => {
              const [index] = [...next];
              setPinned(index ?? null);
            }}
            knobLabel={t.board.knob}
          />
        </div>
        <p className="ow-muted ow-small">{t.board.hint}</p>
        <table className="sr-only">
          <caption>{t.table.caption}</caption>
          <thead>
            <tr>
              <th scope="col">{t.table.key}</th>
              <th scope="col">{t.table.now}</th>
              <th scope="col">{t.table.peak}</th>
              {showTrigger ? <th scope="col">{t.table.state}</th> : null}
            </tr>
          </thead>
          <tbody>
            {movedKeys.map(([index, entry]) => (
              <tr key={index}>
                <th scope="row">{nameOf(index)}</th>
                <td>{texts.mm(entry.depth)}</td>
                <td>{texts.mm(entry.peak)}</td>
                {showTrigger ? <td>{model.estimate.get(index)?.on === true ? t.keyInfo.on : t.keyInfo.off}</td> : null}
              </tr>
            ))}
          </tbody>
        </table>
      </Card>

      <Card title={followed === null ? t.detail.none : t.detail.title({ key: nameOf(followed) })} className="act-depth-detail">
        {followed === null || followedDepth === null ? (
          <p className="ow-muted">{t.detail.hint}</p>
        ) : (
          <div className="act-depth-detail__body">
            <div className="act-depth-detail__value">
              <p className="act-depth-detail__mm ow-num">{texts.mm(followedDepth.depth)}</p>
              {holdPeak ? <p className="ow-small ow-muted ow-num">{t.detail.peak({ value: texts.mm(followedDepth.peak) })}</p> : null}
              {showTrigger && followedValue !== undefined ? (
                <p className="ow-small">
                  {model.estimate.get(followed)?.on === true ? t.detail.estimatedOn : t.detail.estimatedOff}
                </p>
              ) : null}
            </div>
            <TravelGauge
              label={t.detail.gauge({ key: nameOf(followed) })}
              showLegend
              formatMm={formatMm}
              maxMm={W75_TRAVEL_MM}
              depthMm={toMm(followedDepth.depth)}
              peakMm={holdPeak ? toMm(followedDepth.peak) : null}
              actuationMm={showTrigger && followedValue !== undefined ? toMm(followedValue.actuation) : null}
              rt={
                showTrigger && followedValue !== undefined && rtOn
                  ? { pressMm: toMm(followedValue.rt_press), releaseMm: toMm(followedValue.rt_release) }
                  : null
              }
              orientation="horizontal"
              length={260}
            />
            <div className="act-depth-detail__chart">
              <p className="ow-small ow-muted">{t.detail.chart}</p>
              <StripChart
                points={model.history.get(followed) ?? []}
                at={model.at}
                travel={travel}
                actuation={showTrigger && followedValue !== undefined ? followedValue.actuation : null}
              />
            </div>
          </div>
        )}
      </Card>

      <p className="sr-only" aria-live="polite">
        {announce ? announcement : ""}
      </p>
    </div>
  );
}
