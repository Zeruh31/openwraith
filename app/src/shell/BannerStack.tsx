// App-wide banners above every page, in priority order: simulator, fault,
// interrupted operation, keyboard reset, settings that look reset (or a
// changed firmware), an OpenWraith command using the keyboard, another
// program, paused game profiles, a key-depth test that may still be open,
// safety folder, stale values, untested firmware.

import type { ReactNode } from "react";

import type { DeviceStatus } from "../ipc/bindings";

import { Button } from "../components/Button";
import { Callout } from "../components/Feedback";
import { Disclosure, TechnicalDetails } from "../components/Layout";
import { useDict } from "../i18n";
import { useFormat } from "../i18n/format";
import { api } from "../ipc/commands";
import { navigate, tabOf, tabTarget, useRoute } from "../routes";
import { useAppInfo } from "../state/appInfo";
import { useOperationLock } from "../state/operation";
import { cliPurposeStore, useStatus } from "../state/status";
import { useStore } from "../state/store";
import { readAgain, recheckOtherProgram, reconnect, resumeAutoProfiles, toastError } from "./actions";
import { useReadBlockedReason } from "../state/operation";

/** The route of the Recovery card. */
export const RECOVERY_ROUTE = { page: "backup", params: { recover: "1" } } as const;

/** Whether the Recovery card should be offered: profiles look reset, the firmware changed, or a reset report is unresolved. */
export function recoveryAttention(status: Pick<DeviceStatus, "recovery" | "connection">): boolean {
  const recovery = status.recovery;
  return (
    recovery.reset_suspected.length > 0 ||
    recovery.firmware_changed ||
    // While the keyboard's own report is shown (`reset_seen`), its banner says it.
    (recovery.reset_reported && status.connection.state !== "reset_seen")
  );
}

/** The banners that apply now. */
export function BannerStack() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const info = useAppInfo();
  const lock = useOperationLock();
  const readBlocked = useReadBlockedReason();
  const route = useRoute();
  const cliPurpose = useStore(cliPurposeStore);
  const b = dict.shell.banners;
  const banners: ReactNode[] = [];
  const connection = status.connection;

  if (info.sim || status.sim) {
    banners.push(
      <Callout key="sim" kind="sim" className="ow-banner">
        {b.simulator}
      </Callout>,
    );
  }

  if (connection.state === "fault") {
    banners.push(
      <Callout key="fault" kind="danger" title={b.faultTitle} role="alert" className="ow-banner">
        <p>{b.faultBody}</p>
        <TechnicalDetails>{connection.message}</TechnicalDetails>
      </Callout>,
    );
  }

  const interrupted = status.interrupted;
  if (interrupted !== null) {
    const backup = interrupted.safety_backup;
    banners.push(
      <Callout
        key="interrupted"
        kind="danger"
        title={b.interruptedTitle}
        className="ow-banner"
        actions={
          <>
            {backup === null ? null : (
              <Button
                size="sm"
                icon="undo"
                onClick={() => {
                  navigate({ page: "backup", params: { restore: backup.token } });
                }}
              >
                {dict.common.actions.undo}
              </Button>
            )}
            <Button
              size="sm"
              onClick={() => {
                api.dismissInterrupted().catch(toastError);
              }}
            >
              {dict.common.actions.dismiss}
            </Button>
          </>
        }
      >
        <p>
          {b.interruptedBody({
            what: dict.flow.opKinds[interrupted.kind],
            when: format.dateTime(interrupted.started_at),
            profiles: format.list(interrupted.profiles.map(String)),
          })}
        </p>
        {backup === null ? <p>{b.interruptedNoBackup}</p> : null}
      </Callout>,
    );
  }

  if (connection.state === "reset_seen") {
    banners.push(
      <Callout
        key="reset"
        kind="danger"
        title={b.resetTitle}
        role="alert"
        className="ow-banner"
        actions={
          <>
            <Button size="sm" icon="refresh" disabled={lock.busy} onClick={reconnect}>
              {b.resetReadAgain}
            </Button>
            <Button
              size="sm"
              icon="restore"
              onClick={() => {
                navigate(RECOVERY_ROUTE);
              }}
            >
              {b.resetRestore}
            </Button>
          </>
        }
      >
        <p>{b.resetBody}</p>
      </Callout>,
    );
  }

  // Settings that look reset, or a changed firmware: the Recovery card
  // (never shown on Backup & profiles, which opens the card itself whenever
  // this banner would show, and drops `?recover=1` from its route). Nothing
  // is restored without a click there and the standard restore review.
  const onRecovery = route.page === "backup";
  if (recoveryAttention(status) && !onRecovery) {
    const recovery = status.recovery;
    const suspected = [...recovery.reset_suspected].sort((x, y) => x - y);
    banners.push(
      <Callout
        key="recovery"
        kind={recovery.reset_suspected.length > 0 ? "danger" : "warning"}
        title={recovery.firmware_changed ? b.firmwareTitle : b.recoveryTitle}
        role="alert"
        className="ow-banner"
        actions={
          <Button
            size="sm"
            variant="primary"
            icon="restore"
            onClick={() => {
              navigate(RECOVERY_ROUTE);
            }}
          >
            {b.recoveryReview}
          </Button>
        }
      >
        {recovery.firmware_changed ? <p>{b.firmwareBody}</p> : null}
        {recovery.reset_reported ? <p>{b.recoveryReported}</p> : null}
        {suspected.length > 0 ? (
          <p>{b.recoveryProfiles({ list: format.list(suspected.map(String)), count: suspected.length })}</p>
        ) : null}
        {suspected.length > 0 ? <p>{b.recoveryNothingChanged({ count: suspected.length })}</p> : null}
        <p className="ow-strong">{b.recoveryNever}</p>
      </Callout>,
    );
  }

  // An OpenWraith command (the CLI) holds the keyboard: the app only listens.
  if (status.paused_for_cli) {
    banners.push(
      <Callout key="cli" kind="info" title={b.pausedForCliTitle} role="status" className="ow-banner">
        <p>{b.pausedForCliBody}</p>
        {cliPurpose === null ? null : (
          <p>
            {b.pausedForCliPurpose}
            {": "}
            <code className="ow-mono ow-selectable">{cliPurpose}</code>
          </p>
        )}
      </Callout>,
    );
  }

  if (connection.state === "other_program") {
    const evidence = connection.evidence;
    banners.push(
      <Callout
        key="other"
        kind="warning"
        title={b.otherProgramTitle}
        role="alert"
        className="ow-banner"
        actions={
          <Button size="sm" variant="primary" disabled={lock.busy} onClick={recheckOtherProgram}>
            {b.otherProgramCheck}
          </Button>
        }
      >
        <p>{b.otherProgramBody}</p>
        <Disclosure summary={b.otherProgramDetails}>
          <p>
            {b.evidence({
              status: evidence.status_reports,
              acks: evidence.foreign_acks,
              profiles: evidence.profile_reports,
            })}
          </p>
        </Disclosure>
      </Callout>,
    );
  }

  // Game profiles paused by the app itself (too many switches, a failed
  // switch); a pause the user chose shows only as the header's chip.
  const paused = status.automation.paused;
  const onGames = route.page === "settings" && route.params["tab"] === "games";
  if (status.automation.enabled && paused !== null && paused.reason !== "user" && !onGames) {
    banners.push(
      <Callout
        key="auto-paused"
        kind="warning"
        title={b.autoPausedTitle}
        className="ow-banner"
        actions={
          <>
            <Button size="sm" variant="primary" onClick={resumeAutoProfiles}>
              {b.autoResume}
            </Button>
            <Button
              size="sm"
              onClick={() => {
                navigate(tabTarget("settings", "games"));
              }}
            >
              {b.autoOpenSettings}
            </Button>
          </>
        }
      >
        <p>{paused.reason === "rate_limit" ? b.autoPausedRateLimit : b.autoPausedFailed}</p>
        {paused.reason === "switch_failed" ? <TechnicalDetails>{paused.message}</TechnicalDetails> : null}
      </Callout>,
    );
  }

  // A key-depth test an earlier run (or a lost connection) may have left
  // open. Stopping it sends a packet, so it is done on the Key depth tab
  // (the vendor-site tick, a user click); that tab shows its own prompt.
  const travel = status.travel;
  const stray = travel.state === "maybe_open" || travel.stray_reports > 0 || travel.marker_left;
  const onDepthTab = route.page === "actuation" && tabOf(route, "actuation") === "depth";
  if (stray && status.busy?.kind !== "depth_view" && !onDepthTab) {
    banners.push(
      <Callout
        key="travel"
        kind="warning"
        title={b.strayTravelTitle}
        className="ow-banner"
        actions={
          <Button
            size="sm"
            icon="chevron"
            onClick={() => {
              navigate(tabTarget("actuation", "depth"));
            }}
          >
            {b.strayTravelOpen}
          </Button>
        }
      >
        <p>{b.strayTravelBody}</p>
      </Callout>,
    );
  }

  if (info.safety_dir.state === "unavailable") {
    banners.push(
      <Callout key="safety" kind="danger" title={b.safetyDirTitle} className="ow-banner">
        <p>{b.safetyDirBody}</p>
        <TechnicalDetails>{info.safety_dir.error.message}</TechnicalDetails>
      </Callout>,
    );
  }

  if (status.snapshot_stale && connection.state !== "other_program") {
    banners.push(
      <Callout
        key="stale"
        kind="info"
        title={b.staleTitle}
        className="ow-banner"
        actions={
          connection.state === "ready" ? (
            <Button size="sm" icon="refresh" disabled={lock.busy} disabledReason={readBlocked} onClick={readAgain}>
              {dict.common.actions.readAgain}
            </Button>
          ) : undefined
        }
      >
        <p>{b.staleBody}</p>
      </Callout>,
    );
  }

  if (status.firmware_tested === false) {
    banners.push(
      <Callout key="firmware" kind="info" title={b.untestedTitle} className="ow-banner">
        <p>
          {b.untestedBody({
            firmware: status.device?.firmware ?? dict.common.unknown,
            tested: format.list(info.tested_firmware),
          })}
        </p>
      </Callout>,
    );
  }

  if (banners.length === 0) {
    return null;
  }
  return <div className="ow-banners">{banners}</div>;
}
