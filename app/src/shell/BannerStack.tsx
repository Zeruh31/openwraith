// App-wide banners above every page, in priority order: simulator, fault,
// interrupted operation, keyboard reset, another program, a key-depth test
// that may still be open, safety folder, stale values, untested firmware.

import type { ReactNode } from "react";

import { Button } from "../components/Button";
import { Callout } from "../components/Feedback";
import { Disclosure, TechnicalDetails } from "../components/Layout";
import { useDict } from "../i18n";
import { useFormat } from "../i18n/format";
import { api } from "../ipc/commands";
import { navigate, tabOf, tabTarget, useRoute } from "../routes";
import { useAppInfo } from "../state/appInfo";
import { useOperationLock } from "../state/operation";
import { useStatus } from "../state/status";
import { readAgain, recheckOtherProgram, reconnect, toastError } from "./actions";

/** The banners that apply now. */
export function BannerStack() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const info = useAppInfo();
  const lock = useOperationLock();
  const route = useRoute();
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
                navigate("backup");
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
            <Button size="sm" icon="refresh" disabled={lock.busy} onClick={readAgain}>
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
