// The Keyboard page: connection state with its actions, the keyboard's
// facts, the "before you change anything" guide and the safety folder.

import { useState, type ReactNode } from "react";

import { Button } from "../../components/Button";
import { Countdown } from "../../components/Countdown";
import { Callout, ProgressBar, Spinner } from "../../components/Feedback";
import { Card, KeyValueList, PageHeader, TechnicalDetails, type KeyValueItem } from "../../components/Layout";
import { ErrorView } from "../../flow/ErrorView";
import { useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import { navigate } from "../../routes";
import { readAgain, reconnect, release, revealSafetyDir, selectDevice } from "../../shell/actions";
import { useAppInfo } from "../../state/appInfo";
import { useOperationLock } from "../../state/operation";
import { profileLabel, useProfileNames } from "../../state/profileNames";
import { useSnapshot } from "../../state/snapshot";
import { useStatus } from "../../state/status";
import { buildDateText } from "./buildDate";
import "./device.css";
import { useReadBlockedReason } from "../../state/operation";

const GUIDE_KEY = "openwraith.guideDismissed";

function guideDismissed(): boolean {
  try {
    return window.localStorage.getItem(GUIDE_KEY) === "1";
  } catch {
    return false;
  }
}

function ConnectionCard() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const lock = useOperationLock();
  const t = dict.device.connection;
  const c = status.connection;
  let body: ReactNode;
  let actions: ReactNode = null;
  switch (c.state) {
    case "starting":
      body = (
        <p className="ow-row">
          <Spinner />
          <span>{t.starting}</span>
        </p>
      );
      break;
    case "searching":
      body = (
        <p className="ow-row">
          <Spinner />
          <span>{t.searching}</span>
        </p>
      );
      break;
    case "several_keyboards":
      body = (
        <div className="ow-stack">
          <p>{t.severalKeyboards}</p>
          <ul className="dv-devices">
            {c.devices.map((entry, index) => {
              // Two keyboards of the same model may look the same: they are
              // numbered, and the end of the connection ID tells them apart.
              const n = index + 1;
              const rowId = `dv-device-${entry.id}`;
              return (
                <li key={entry.id} className="dv-device">
                  <div id={rowId}>
                    <p className="ow-strong">{t.deviceTitle({ n })}</p>
                    <p>
                      {t.deviceLine({
                        product: entry.product ?? dict.common.unknown,
                        usb: `${format.hex4(entry.vendor_id)}:${format.hex4(entry.product_id)}`,
                      })}
                    </p>
                    <p className="ow-muted ow-small">
                      {entry.serial === null || entry.serial === ""
                        ? t.deviceId({ id: entry.id.slice(-4) })
                        : t.serial({ serial: entry.serial })}
                    </p>
                  </div>
                  <Button
                    size="sm"
                    variant="primary"
                    disabled={lock.busy}
                    aria-describedby={rowId}
                    onClick={() => {
                      selectDevice(entry.id);
                    }}
                  >
                    {t.choose({ n })}
                  </Button>
                </li>
              );
            })}
          </ul>
        </div>
      );
      break;
    case "opening":
      body = (
        <p className="ow-row">
          <Spinner />
          <span>{t.opening}</span>
        </p>
      );
      break;
    case "open_failed":
      body = (
        <div className="ow-stack">
          <p>{t.openFailed}</p>
          <p className="ow-muted">
            <Countdown key={status.revision} remainingMs={c.retry_in_ms} render={(seconds) => t.retryingIn({ seconds })} />
          </p>
          <ErrorView error={c.error} written="none" />
        </div>
      );
      actions = (
        <Button icon="refresh" disabled={lock.busy} onClick={reconnect}>
          {dict.common.actions.retry}
        </Button>
      );
      break;
    case "observing":
      body = (
        <div className="ow-stack">
          <p className="ow-row">
            <Spinner />
            <span>{dict.shell.status.observing}</span>
          </p>
          <p className="ow-muted">{t.observing}</p>
        </div>
      );
      break;
    case "other_program":
      // The banner above explains it and has "I closed it, check again".
      body = <p>{t.otherProgram}</p>;
      break;
    case "reading":
      body = (
        <div className="ow-stack">
          <p>{t.reading}</p>
          <ProgressBar
            value={c.done}
            max={c.total}
            label={dict.shell.status.reading({ done: c.done, total: c.total })}
            valueText={dict.flow.progress.count({ done: c.done, total: c.total })}
          />
        </div>
      );
      break;
    case "read_failed":
      body = (
        <div className="ow-stack">
          <p>{t.readFailed}</p>
          <ErrorView error={c.error} written="nothing" />
        </div>
      );
      actions = (
        <Button icon="refresh" disabled={lock.busy} onClick={reconnect}>
          {dict.common.actions.retry}
        </Button>
      );
      break;
    case "ready":
      // "Read again" is on the Keyboard card below.
      body = <p>{status.paused_for_cli ? t.pausedForCli : t.ready}</p>;
      actions = (
        <Button icon="plug" disabled={lock.busy} onClick={release} title={dict.shell.popover.releaseHint}>
          {t.release}
        </Button>
      );
      break;
    case "lost":
      body = (
        <div className="ow-stack">
          <p>{t.lost}</p>
          <TechnicalDetails>{c.error.message}</TechnicalDetails>
        </div>
      );
      actions = (
        <Button icon="plug" disabled={lock.busy} onClick={reconnect}>
          {t.connect}
        </Button>
      );
      break;
    case "reset_seen":
      body = <p>{t.resetSeen}</p>;
      actions = (
        <Button variant="primary" icon="refresh" disabled={lock.busy} onClick={reconnect}>
          {t.reconnect}
        </Button>
      );
      break;
    case "unsupported":
      body = <p>{t.unsupported({ product: c.product ?? dict.common.unknown })}</p>;
      break;
    case "released":
      body = <p>{t.released}</p>;
      actions = (
        <Button variant="primary" icon="plug" disabled={lock.busy} onClick={reconnect}>
          {t.connect}
        </Button>
      );
      break;
    case "fault":
      body = (
        <div className="ow-stack">
          <p>{t.fault}</p>
          <TechnicalDetails>{c.message}</TechnicalDetails>
        </div>
      );
      break;
  }
  return (
    <Card title={t.title} footer={actions === null ? undefined : <div className="ow-row">{actions}</div>}>
      {body}
    </Card>
  );
}

function KeyboardCard() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const { snapshot } = useSnapshot();
  const lock = useOperationLock();
  const readBlocked = useReadBlockedReason();
  const names = useProfileNames();
  const t = dict.device.keyboard;
  const device = snapshot?.device ?? status.device;
  if (device === null) {
    return null;
  }
  const items: KeyValueItem[] = [
    { key: "model", label: t.model, value: device.model ?? dict.common.unknown },
    { key: "product", label: t.product, value: <span className="ow-mono">{device.product ?? dict.common.unknown}</span> },
    {
      key: "firmware",
      label: t.firmware,
      value: (
        <span className="ow-row">
          <span className="ow-num">{device.firmware ?? dict.common.unknown}</span>
          {status.firmware_tested === null ? null : (
            <span className={`dv-chip ${status.firmware_tested ? "dv-chip--ok" : "dv-chip--warn"}`}>
              {status.firmware_tested ? t.tested : t.notTested}
            </span>
          )}
        </span>
      ),
    },
  ];
  if (device.build_date !== null) {
    items.push({ key: "build", label: t.buildDate, value: <span className="ow-num">{buildDateText(device.build_date, format)}</span> });
  }
  if (device.vendor_id !== null && device.product_id !== null) {
    items.push({
      key: "usb",
      label: t.usb,
      value: <span className="ow-mono">{`${format.hex4(device.vendor_id)}:${format.hex4(device.product_id)}`}</span>,
    });
  }
  items.push({
    key: "profile",
    label: t.activeProfile,
    value:
      status.active_profile === null
        ? dict.common.profileUnknown
        : profileLabel(dict, status.active_profile, names[status.active_profile - 1] ?? null),
  });
  if (snapshot !== null) {
    if (snapshot.poll_rate_hz !== null) {
      items.push({
        key: "poll",
        label: t.pollRate,
        value: dict.common.units.hz({ value: format.number(snapshot.poll_rate_hz) }),
      });
    }
    if (snapshot.triggers.unit_um !== null) {
      items.push({
        key: "unit",
        label: t.distanceUnit,
        value: format.mm(1, snapshot.triggers.unit_um),
      });
    }
  }
  return (
    <Card
      title={t.title}
      actions={
        status.connection.state === "ready" ? (
          <Button size="sm" icon="refresh" disabled={lock.busy} disabledReason={readBlocked} onClick={readAgain}>
            {dict.common.actions.readAgain}
          </Button>
        ) : undefined
      }
      footer={
        <p className="ow-muted ow-small">{snapshot === null ? t.notRead : t.readAt({ time: format.time(snapshot.read_at) })}</p>
      }
    >
      <KeyValueList items={items} />
      {snapshot !== null && snapshot.undecoded > 0 ? (
        <p className="ow-muted ow-small">{t.undecoded({ n: snapshot.undecoded })}</p>
      ) : null}
    </Card>
  );
}

function GuideCard({ onDismiss }: { onDismiss: () => void }) {
  const dict = useDict();
  const t = dict.device.beforeYouStart;
  return (
    <Card
      title={t.title}
      className="dv-guide"
      footer={
        <Button size="sm" variant="ghost" onClick={onDismiss}>
          {t.dismiss}
        </Button>
      }
    >
      <ol className="dv-guide__steps">
        <li>{t.step1}</li>
        <li>
          <p>{t.step2}</p>
          <div className="dv-guide__action">
            <Button
              size="sm"
              icon="backup"
              onClick={() => {
                navigate("backup");
              }}
            >
              {t.step2Action}
            </Button>
          </div>
        </li>
        <li>{t.step3}</li>
      </ol>
    </Card>
  );
}

function SafetyFolderCard() {
  const dict = useDict();
  const info = useAppInfo();
  const t = dict.device.safetyFolder;
  return (
    <Card title={t.title}>
      <p className="ow-muted">{t.body}</p>
      {info.safety_dir.state === "ok" ? (
        <div className="dv-folder">
          <code className="ow-break ow-selectable">{info.safety_dir.path}</code>
          <Button size="sm" icon="folder" onClick={revealSafetyDir}>
            {dict.common.actions.showInFolder}
          </Button>
        </div>
      ) : (
        <Callout kind="danger" title={t.unavailable}>
          <TechnicalDetails>{info.safety_dir.error.message}</TechnicalDetails>
        </Callout>
      )}
    </Card>
  );
}

/** The Keyboard page. */
export default function DevicePage() {
  const dict = useDict();
  const [showGuide, setShowGuide] = useState(() => !guideDismissed());
  return (
    <div className="dv-page">
      <PageHeader title={dict.device.title} description={dict.device.description} />
      <div className="dv-grid">
        <div className="ow-stack">
          <ConnectionCard />
          <KeyboardCard />
        </div>
        <div className="ow-stack">
          {showGuide ? (
            <GuideCard
              onDismiss={() => {
                setShowGuide(false);
                try {
                  window.localStorage.setItem(GUIDE_KEY, "1");
                } catch {
                  // Remembering is a convenience.
                }
              }}
            />
          ) : null}
          <SafetyFolderCard />
        </div>
      </div>
      <p className="ow-muted ow-small dv-disclaimer">{dict.common.disclaimer}</p>
    </div>
  );
}
