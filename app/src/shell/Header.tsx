// The header: app name, the connection pill (with a details popover), the
// "Live" pill of the key-depth view, the "Auto" chip of a game rule, the
// busy indicator and the profile switcher (with the local profile names,
// "Profile 2 · FPS").

import { useEffect, useId, useRef, useState } from "react";

import { Button } from "../components/Button";
import { SegmentedControl } from "../components/Controls";
import { Callout, Spinner } from "../components/Feedback";
import { Icon } from "../components/Icon";
import { KeyValueList, type KeyValueItem } from "../components/Layout";
import { stepLabel, warningText } from "../flow/labels";
import { WriteFlow } from "../flow/WriteFlow";
import { useDict, type Dict } from "../i18n";
import { useFormat } from "../i18n/format";
import type { DeviceStatus, ProfileSwitchPreview, ProfileSwitchReport } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { hasTabs, href, navigate, tabTarget } from "../routes";
import { useAppInfo } from "../state/appInfo";
import { useDraftEntries, type DraftInfo } from "../state/drafts";
import { isLocked, useOperationLock } from "../state/operation";
import { profileLabel, useProfileNames } from "../state/profileNames";
import { useStatus } from "../state/status";
import { toast } from "../state/toasts";
import { readAgain, reconnect, release } from "./actions";
import { toggleDevSimPanel, useDevSimPanelOpen } from "./DevSimPanel";
import { tabLabel } from "./PageTabs";
import { canConnect, connectionSummary } from "./statusText";
import { useReadBlockedReason } from "../state/operation";

/** OpenWraith's mark (original artwork: a keycap with an open ring). */
export function AppLogo({ size = 28 }: { size?: number | undefined }) {
  return (
    <svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true" focusable="false" className="ow-logo">
      <rect x="32" y="32" width="960" height="960" rx="200" fill="#15202a" />
      <rect x="192" y="208" width="640" height="640" rx="120" fill="#0a6c96" />
      <rect x="264" y="248" width="496" height="496" rx="88" fill="#d4eefa" />
      <path
        d="M 512 344 A 152 152 0 1 1 404.5 388.5"
        fill="none"
        stroke="#0a6c96"
        strokeWidth="56"
        strokeLinecap="round"
      />
    </svg>
  );
}

/** "Keys > Macros: 1 unwritten change" (the page alone for a page-wide draft). */
export function draftText(entry: DraftInfo, dict: Dict): string {
  const page = dict.shell.nav[entry.page];
  let where = page;
  if (entry.tab !== null && hasTabs(entry.page)) {
    where = dict.shell.drafts.where({ page, tab: tabLabel(dict, entry.page, entry.tab) });
  }
  return entry.count === null
    ? dict.shell.drafts.entryUncounted({ where })
    : dict.shell.drafts.entry({ where, count: entry.count });
}

/** The connection pill and its popover. */
export function DeviceStatusPill() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const lock = useOperationLock();
  const readBlocked = useReadBlockedReason();
  const names = useProfileNames();
  const [open, setOpen] = useState(false);
  const popoverId = useId();
  const wrapRef = useRef<HTMLDivElement>(null);
  const summary = connectionSummary(status, dict, format);

  useEffect(() => {
    if (!open) return undefined;
    const onPointer = (event: PointerEvent) => {
      if (wrapRef.current !== null && event.target instanceof Node && !wrapRef.current.contains(event.target)) {
        setOpen(false);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const device = status.device;
  const items: KeyValueItem[] = [
    { key: "connection", label: dict.shell.popover.connection, value: summary.text },
  ];
  if (device !== null) {
    items.push(
      { key: "model", label: dict.shell.popover.model, value: device.model ?? dict.common.unknown },
      { key: "product", label: dict.shell.popover.product, value: device.product ?? dict.common.unknown },
      { key: "firmware", label: dict.shell.popover.firmware, value: device.firmware ?? dict.common.unknown },
    );
    if (device.vendor_id !== null && device.product_id !== null) {
      items.push({
        key: "usb",
        label: dict.shell.popover.usb,
        value: <span className="ow-mono">{`${format.hex4(device.vendor_id)}:${format.hex4(device.product_id)}`}</span>,
      });
    }
  }
  items.push({
    key: "profile",
    label: dict.shell.popover.activeProfile,
    value:
      status.active_profile === null
        ? dict.common.profileUnknown
        : profileLabel(dict, status.active_profile, names[status.active_profile - 1] ?? null),
  });

  const ready = status.connection.state === "ready";
  return (
    <div className="ow-pill-wrap" ref={wrapRef}>
      <button
        type="button"
        className={`ow-pill ow-pill--${summary.tone}`}
        aria-expanded={open}
        aria-controls={popoverId}
        aria-label={`${dict.shell.header.statusButton}: ${summary.text}`}
        onClick={() => {
          setOpen((value) => !value);
        }}
      >
        <span className="ow-pill__dot" aria-hidden="true" />
        <span className="ow-pill__text">{summary.text}</span>
        <Icon name="chevron" size={14} className="ow-pill__chevron" />
      </button>
      {open ? (
        <div id={popoverId} className="ow-popover" role="dialog" aria-label={dict.shell.popover.title}>
          <KeyValueList items={items} />
          <div className="ow-popover__actions">
            {ready ? (
              <Button
                size="sm"
                icon="refresh"
                disabled={lock.busy}
                disabledReason={readBlocked}
                onClick={() => {
                  setOpen(false);
                  readAgain();
                }}
              >
                {dict.shell.popover.readAgain}
              </Button>
            ) : null}
            {status.connection.state === "several_keyboards" ? (
              <Button
                size="sm"
                icon="keyboard"
                onClick={() => {
                  setOpen(false);
                  navigate("device");
                }}
              >
                {dict.shell.popover.chooseKeyboard}
              </Button>
            ) : null}
            {canConnect(status) ? (
              <Button
                size="sm"
                icon="plug"
                disabled={lock.busy}
                onClick={() => {
                  setOpen(false);
                  reconnect();
                }}
              >
                {dict.shell.popover.connect}
              </Button>
            ) : null}
            {ready || status.connection.state === "other_program" || status.connection.state === "observing" ? (
              <Button
                size="sm"
                icon="plug"
                disabled={lock.busy}
                title={dict.shell.popover.releaseHint}
                onClick={() => {
                  setOpen(false);
                  release();
                }}
              >
                {dict.shell.popover.release}
              </Button>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}

/** The key-depth view runs (it holds the keyboard until it is stopped). */
export function depthViewLive(status: Pick<DeviceStatus, "busy" | "travel">): boolean {
  return status.busy?.kind === "depth_view" || status.travel.state === "open";
}

/** "Live" while the key-depth view runs; it leads to the tab that stops it. */
export function LivePill() {
  const dict = useDict();
  const status = useStatus();
  if (!depthViewLive(status)) return null;
  return (
    <a className="ow-live-pill" href={href(tabTarget("actuation", "depth"))}>
      <span className="ow-live-pill__dot" aria-hidden="true" />
      <span>{dict.shell.header.live}</span>
      <span className="sr-only">{dict.shell.header.liveTitle}</span>
    </a>
  );
}

/**
 * "Auto" while a game rule holds the active profile, "Auto paused" while the
 * game profiles are paused; both lead to Settings > Games.
 */
export function AutoChip() {
  const dict = useDict();
  const status = useStatus();
  const names = useProfileNames();
  const automation = status.automation;
  const target = href(tabTarget("settings", "games"));
  if (!automation.enabled) return null;
  if (automation.paused !== null) {
    return (
      <a className="ow-auto-chip ow-auto-chip--paused" href={target}>
        <span>{dict.shell.header.autoPaused}</span>
        <span className="sr-only">{dict.shell.header.autoPausedTitle}</span>
      </a>
    );
  }
  const holding = automation.holding;
  if (holding === null || (status.active_profile !== null && status.active_profile !== holding.profile)) return null;
  const title = dict.shell.header.autoTitle({
    label: holding.label,
    profile: profileLabel(dict, holding.profile, names[holding.profile - 1] ?? null),
  });
  // The explanation is the link's name (read once); no tooltip repeats it.
  return (
    <a className="ow-auto-chip" href={target}>
      <span>{dict.shell.header.auto}</span>
      <span className="sr-only">{title}</span>
    </a>
  );
}

/** The current operation's step (from the status, so it survives a reload); the live view shows the "Live" pill instead. */
export function BusyIndicator() {
  const dict = useDict();
  const status = useStatus();
  const busy = status.busy;
  if (busy === null || busy.kind === "depth_view") {
    return null;
  }
  const text = busy.last === null ? dict.flow.opKinds[busy.kind] : stepLabel(busy.last.step, dict);
  return (
    <div className="ow-busy" role="status">
      <Spinner />
      <span className="ow-busy__text">{text}</span>
    </div>
  );
}

/** Why the switcher is disabled, or `null`. */
function switcherBlock(status: DeviceStatus, busy: boolean, dict: Dict): string | null {
  const blocked = dict.shell.header.switcherBlocked;
  if (busy) return blocked.busy;
  if (status.paused_for_cli) return blocked.pausedForCli;
  switch (status.connection.state) {
    case "ready":
      return status.write_gate.allowed ? null : blocked.blocked;
    case "observing":
    case "other_program":
    case "reading":
    case "reset_seen":
    case "fault":
      return blocked.blocked;
    case "starting":
    case "searching":
    case "several_keyboards":
    case "opening":
    case "open_failed":
    case "read_failed":
    case "lost":
    case "unsupported":
    case "released":
      return blocked.notReady;
  }
}

/** The 1-4 profile switcher; a click opens the compact confirmation. */
export function ProfileSwitcher() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const lock = useOperationLock();
  const drafts = useDraftEntries();
  const names = useProfileNames();
  const reasonId = useId();
  const [target, setTarget] = useState<number | null>(null);
  const opening = useRef(false);
  // "Busy" only while the backend runs something; a review dialog open here
  // only disables the switcher (it sits behind the dialog then).
  const blocked = switcherBlock(status, status.busy !== null, dict);

  const open = (profile: number) => {
    if (blocked !== null || opening.current || isLocked() || profile === status.active_profile) return;
    opening.current = true;
    setTarget(profile);
  };

  return (
    <div className="ow-switcher">
      <SegmentedControl
        label={dict.shell.header.profileLabel}
        size="sm"
        selectOnFocus={false}
        value={status.active_profile}
        disabled={(blocked !== null || lock.local) && target === null}
        describedBy={blocked === null ? undefined : reasonId}
        options={[1, 2, 3, 4].map((n) => {
          const name = names[n - 1] ?? null;
          return {
            value: n,
            label:
              name === null ? (
                <span className="ow-num">{n}</span>
              ) : (
                <>
                  <span className="ow-num" aria-hidden="true">
                    {n}
                  </span>
                  <span className="sr-only">{dict.shell.header.profileOptionNamed({ n, name })}</span>
                </>
              ),
          };
        })}
        onChange={open}
      />
      <span className="ow-switcher__label" id={reasonId}>
        {blocked ??
          (status.active_profile === null
            ? dict.common.profileUnknown
            : profileLabel(dict, status.active_profile, names[status.active_profile - 1] ?? null))}
      </span>
      {target === null ? null : (
        <WriteFlow<ProfileSwitchPreview, ProfileSwitchReport>
          kind="profile_switch"
          compact
          writesSettings={false}
          prepare={() => api.prepareProfileSwitch(target)}
          token={(plan) => plan.token}
          title={(plan) => dict.shell.switcher.title({ n: plan.to })}
          review={(plan) => (
            <div className="ow-stack">
              <p>{dict.shell.switcher.body}</p>
              {status.automation.holding === null ? null : (
                <p>
                  {dict.shell.switcher.autoHolds({
                    label: status.automation.holding.label,
                    n: status.automation.holding.profile,
                  })}
                </p>
              )}
              {plan.save_wait_ms > 0 ? (
                <p>{dict.shell.switcher.saveWait({ seconds: format.seconds(plan.save_wait_ms) })}</p>
              ) : null}
              {drafts.length > 0 ? (
                <Callout kind="warning" title={dict.shell.switcher.drafts}>
                  <ul>
                    {drafts.map((entry) => (
                      <li key={`${entry.page}/${entry.tab ?? ""}`}>{draftText(entry, dict)}</li>
                    ))}
                  </ul>
                </Callout>
              ) : null}
            </div>
          )}
          nothingToDo={(plan) => <Callout kind="info" title={dict.shell.switcher.already({ n: plan.to })} />}
          confirmLabel={(plan) => dict.shell.switcher.confirm({ n: plan.to })}
          apply={(token, onProgress) => api.applyProfileSwitch(token, onProgress)}
          result={() => null}
          closeOnResult
          onClose={(report) => {
            opening.current = false;
            setTarget(null);
            if (report !== null) {
              toast({
                kind: "success",
                title: dict.shell.notices.switched({ n: report.active_profile_now ?? report.to }),
                body: report.warnings.length > 0 ? dict.shell.notices.switchWarnings : undefined,
                details:
                  report.warnings.length > 0
                    ? report.warnings.map((w) => warningText(w, dict, format)).join("\n")
                    : undefined,
              });
            }
          }}
        />
      )}
    </div>
  );
}

/** The simulator badge; it opens the simulator panel. */
function SimBadge() {
  const dict = useDict();
  const open = useDevSimPanelOpen();
  return (
    <button
      type="button"
      className="ow-sim-badge ow-sim-badge--button"
      aria-expanded={open}
      aria-controls="ow-devsim"
      title={dict.shell.devSim.title}
      onClick={toggleDevSimPanel}
    >
      <Icon name="sim" size={14} />
      <span>{dict.shell.status.simulatorBadge}</span>
    </button>
  );
}

/** The app header. */
export function Header() {
  const dict = useDict();
  const info = useAppInfo();
  const status = useStatus();
  return (
    <header className="ow-header">
      <div className="ow-header__brand">
        <AppLogo />
        <span className="ow-header__name">{dict.common.appName}</span>
        {info.sim || status.sim ? <SimBadge /> : null}
      </div>
      <DeviceStatusPill />
      <div className="ow-header__spacer" />
      <LivePill />
      <AutoChip />
      <BusyIndicator />
      <ProfileSwitcher />
    </header>
  );
}
