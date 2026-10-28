// The Recovery card (`#/backup?recover=1`, and whenever the keyboard looks
// reset): what OpenWraith noticed in plain words (the keyboard's reset
// report, a firmware change, profiles back to their factory settings),
// "Reconnect and compare" after the keyboard's own reset report (a user
// click), and per profile "Restore from [automatic backup]" into the
// standard restore review, "Restore all profiles" (the newest automatic
// backup of every profile, composed by the backend) and "It's fine, keep
// them". OpenWraith never restores anything without asking: reading the
// card sends nothing to the keyboard, and every restore is a review that
// writes only after Write.

import { useEffect, useId, useRef, useState, type ReactNode, type Ref } from "react";

import { Button, IconButton } from "../../../components/Button";
import { Checkbox } from "../../../components/Controls";
import { ConfirmDialog } from "../../../components/Dialog";
import { Callout, Spinner } from "../../../components/Feedback";
import { Card } from "../../../components/Layout";
import { ErrorView } from "../../../flow/ErrorView";
import { useVendorConfirmation, VendorNote } from "../../../flow/vendorTick";
import { useDict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type { AppError, FileToken, RecoveryIntent, RecoveryProfileView, RecoveryView } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { navigate } from "../../../routes";
import { reconnect, toastError } from "../../../shell/actions";
import { isLocked } from "../../../state/operation";
import { useStatus } from "../../../state/status";
import { useDeviceBlock } from "../availability";
import { useDemoAction, type BackupDemo } from "../demo";
import { switchesFor } from "../model";
import { numberList } from "../texts";
import {
  backupOptionText,
  buildRecoveryIntent,
  chosenBackup,
  comparedText,
  evidenceText,
  firmwareVersion,
  heldProfiles,
  keepBlock,
  knownFirmware,
  needsFirmwareTick,
  profileTitle,
  restoreAllRequest,
  singleRequest,
  stateNote,
  summarySentences,
  tablesText,
  type RecoveryRequest,
} from "./model";
import { RecoveryFlow, type FirmwareSeen } from "./RecoveryFlow";

type ViewState = { s: "loading" } | { s: "ok"; view: RecoveryView } | { s: "error"; error: AppError };

/** Props of {@link ProfileRow}. */
interface ProfileRowProps {
  profile: RecoveryProfileView;
  /** The automatic backup picked for it (`undefined`: its newest). */
  picked: FileToken | undefined;
  onPick: (token: FileToken) => void;
  /** Why its restore can't start now, or `null`. */
  reason: string | null;
  onRestore: () => void;
}

/** One profile: how it compares, why it looks reset, and "Restore from [automatic backup]". */
function ProfileRow({ profile, picked, onPick, reason, onRestore }: ProfileRowProps) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.backupRestore.recovery;
  const selectId = useId();
  const evidenceId = useId();
  const suspected = profile.state === "reset_suspected";
  const entry = chosenBackup(profile, picked);
  const note = stateNote(profile, dict, format);
  const compared = suspected ? comparedText(profile, dict, format) : null;
  const chip = suspected ? " br-chip--warning" : profile.state === "fine" ? " br-chip--accent" : " br-chip--muted";
  return (
    <li className={`br-recovery__profile${suspected ? " br-recovery__profile--held" : ""}`}>
      <div className="br-recovery__head">
        <p className="ow-strong">{profileTitle(profile, dict)}</p>
        <span className={`br-chip${chip}`}>{t.states[profile.state]}</span>
      </div>
      {suspected && profile.evidence.length > 0 ? (
        <div className="br-recovery__evidence">
          <p id={evidenceId} className="ow-small ow-strong">
            {t.evidenceTitle}
          </p>
          <ul aria-labelledby={evidenceId}>
            {profile.evidence.map((evidence, index) => (
              <li key={`${evidence.evidence}-${index}`}>{evidenceText(evidence, dict, format)}</li>
            ))}
          </ul>
        </div>
      ) : null}
      {suspected && profile.factory_now.length > 0 ? (
        <p>{t.factoryNow({ list: tablesText(profile.factory_now, dict, format) })}</p>
      ) : null}
      {profile.changed.length > 0 && profile.state !== "fine" ? (
        <p className="ow-muted ow-small">{t.changed({ list: tablesText(profile.changed, dict, format) })}</p>
      ) : null}
      {compared === null ? null : <p className="ow-muted ow-small">{compared}</p>}
      {note === null ? null : <p className="ow-muted ow-small">{note}</p>}
      {entry === null ? (
        profile.state === "no_known_good" ? null : <p className="ow-muted ow-small">{t.noBackups}</p>
      ) : (
        <div className="br-recovery__restore">
          <div className="br-share__field">
            <label htmlFor={selectId} className="ow-field__label">
              {t.restoreFrom({ n: profile.profile })}
            </label>
            <select
              id={selectId}
              className="ow-input br-share__select"
              value={entry.file.token}
              onChange={(event) => {
                onPick(event.currentTarget.value);
              }}
            >
              {profile.backups.map((backup) => (
                <option key={backup.file.token} value={backup.file.token}>
                  {backupOptionText(backup, dict, format)}
                </option>
              ))}
            </select>
          </div>
          <Button variant={suspected ? "primary" : "secondary"} icon="restore" disabledReason={reason} onClick={onRestore}>
            {t.restore({ n: profile.profile })}
          </Button>
        </div>
      )}
    </li>
  );
}

/** Props of {@link RecoveryCard}. */
export interface RecoveryCardProps {
  /** Changing it reads the card again. */
  reloadKey: number;
  demo: BackupDemo | undefined;
  /** Called after a restore was applied or the settings were kept (new backups exist). */
  onChanged: () => void;
  /** Hides the card (offered while nothing needs attention). */
  onClose: () => void;
  /** The card's title (focused when the route opens the card). */
  titleRef?: Ref<HTMLSpanElement> | undefined;
}

/** The Recovery card. */
export function RecoveryCard({ reloadKey, demo, onChanged, onClose, titleRef }: RecoveryCardProps) {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const block = useDeviceBlock();
  // Comparing other profiles switches profiles: the same vendor-site
  // confirmation as every write.
  const vendor = useVendorConfirmation();
  const t = dict.backupRestore.recovery;
  const r = dict.backupRestore.restore;
  const profilesId = useId();
  const [state, setState] = useState<ViewState>({ s: "loading" });
  const [own, setOwn] = useState(0);
  const [picked, setPicked] = useState<Readonly<Record<number, FileToken>>>({});
  const [allowFirmware, setAllowFirmware] = useState(false);
  const [firmwareSeen, setFirmwareSeen] = useState<FirmwareSeen | null>(null);
  const [keeping, setKeeping] = useState(false);
  const [kept, setKept] = useState(false);
  const [consent, setConsent] = useState<{ request: RecoveryRequest; switches: number[] } | null>(null);
  const [flow, setFlow] = useState<{ id: number; request: RecoveryRequest; intent: RecoveryIntent } | null>(null);
  const flows = useRef(0);

  // Read again when the backend's view may have changed: a new read (the
  // comparison runs on every complete read), a reset report, a restore.
  const recovery = status.recovery;
  const statusKey = [
    status.generation,
    status.snapshot_revision ?? "none",
    recovery.reset_reported,
    recovery.firmware_changed,
    recovery.reset_suspected.join(","),
  ].join("|");
  useEffect(() => {
    let alive = true;
    api.getRecovery().then(
      (view) => {
        if (alive) setState({ s: "ok", view });
      },
      (error: unknown) => {
        if (alive) setState({ s: "error", error: toAppError(error) });
      },
    );
    return () => {
      alive = false;
    };
  }, [statusKey, reloadKey, own]);

  const view = state.s === "ok" ? state.view : null;

  const firmwareReason = (request: RecoveryRequest): string | null =>
    view !== null && needsFirmwareTick(view, request.profiles, firmwareSeen !== null) && !allowFirmware
      ? dict.backupRestore.reasons.firmwareTick
      : null;
  const reasonFor = (request: RecoveryRequest): string | null => block ?? firmwareReason(request);

  const openFlow = (request: RecoveryRequest, switches: readonly number[]) => {
    flows.current += 1;
    setFlow({ id: flows.current, request, intent: buildRecoveryIntent(request, allowFirmware, switches) });
  };

  /** Asks for the profile switches (when there are any), then compares. */
  const begin = (request: RecoveryRequest) => {
    const switches = switchesFor(request.profiles, status.active_profile);
    setFlow(null);
    if (switches.length > 0) {
      setConsent({ request, switches });
    } else {
      openFlow(request, []);
    }
  };

  const onRestore = (request: RecoveryRequest) => {
    if (reasonFor(request) !== null || flow !== null || consent !== null || isLocked()) return;
    begin(request);
  };

  const keep = () => {
    if (keeping) return;
    setKeeping(true);
    api
      .keepCurrentSettings([])
      .then(
        (next) => {
          setState({ s: "ok", view: next });
          setKept(true);
          onChanged();
        },
        (error: unknown) => {
          toastError(error);
        },
      )
      .finally(() => {
        setKeeping(false);
      });
  };

  const singleFor = (profile: RecoveryProfileView): RecoveryRequest | null => {
    const entry = chosenBackup(profile, picked[profile.profile]);
    return entry === null ? null : singleRequest(profile.profile, entry);
  };

  useDemoAction(demo?.recovery !== undefined && view !== null, () => {
    if (view === null) return;
    if (demo?.recovery === "restore_all") {
      const all = restoreAllRequest(view);
      if (all !== null) onRestore(all);
      return;
    }
    const first = view.profiles.find((p) => p.state === "reset_suspected") ?? view.profiles.find((p) => p.backups.length > 0);
    const request = first === undefined ? null : singleFor(first);
    if (request !== null) onRestore(request);
  });

  let body: ReactNode;
  switch (state.s) {
    case "loading":
      body = (
        <p className="ow-row" role="status">
          <Spinner />
          <span>{dict.common.loading}</span>
        </p>
      );
      break;
    case "error":
      body = (
        <>
          <p>{t.loadFailed}</p>
          <ErrorView error={state.error} written="none" />
          <p className="ow-strong">{t.never}</p>
        </>
      );
      break;
    case "ok": {
      const current = state.view;
      const held = heldProfiles(current);
      const resetSeen = status.connection.state === "reset_seen";
      const all = restoreAllRequest(current);
      const firmwareShown =
        current.firmware_changed !== null || current.profiles.some((p) => p.firmware_differs) || firmwareSeen !== null;
      const unknown = dict.common.unknown;
      const notChecked = current.reset_reported && current.profiles.some((p) => p.state === "not_checked");
      const keepBlocked = keepBlock(resetSeen, held.length, notChecked);
      const keepReason = keepBlocked === "reconnect" ? t.keepFirst : keepBlocked === "compare" ? t.keepCompareFirst : null;
      body = (
        <>
          {current.attention ? (
            <Callout
              kind="warning"
              title={t.attentionTitle}
              actions={
                <>
                  {resetSeen ? (
                    <Button variant="primary" icon="refresh" onClick={reconnect}>
                      {t.reconnect}
                    </Button>
                  ) : null}
                  <Button
                    icon="check"
                    disabled={keeping}
                    disabledReason={keepReason}
                    onClick={keep}
                  >
                    {t.keep}
                  </Button>
                </>
              }
            >
              {summarySentences(current, dict, format).map((sentence) => (
                <p key={sentence}>{sentence}</p>
              ))}
              {resetSeen ? <p>{t.reconnectHint}</p> : null}
              <p className="ow-muted ow-small">{t.keepHint}</p>
              {current.unidentified ? <p className="ow-muted ow-small">{t.unidentified}</p> : null}
              <p className="ow-strong">{t.never}</p>
            </Callout>
          ) : (
            <Callout kind="success" title={t.fineTitle}>
              {kept ? <p role="status">{t.kept}</p> : <p>{t.fineBody}</p>}
              <p className="ow-strong">{t.never}</p>
            </Callout>
          )}
          {current.auto_backup_enabled ? null : (
            <Callout
              kind="info"
              actions={
                <Button
                  size="sm"
                  icon="settings"
                  onClick={() => {
                    navigate({ page: "settings", params: { tab: "background" } });
                  }}
                >
                  {t.openSettings}
                </Button>
              }
            >
              <p>{t.autoOff}</p>
            </Callout>
          )}
          {firmwareShown ? (
            <Callout kind="warning" title={t.firmwareTitle}>
              <p>
                {t.firmwareBody({
                  backup: firmwareSeen?.backup ?? firmwareVersion(knownFirmware(current), dict),
                  keyboard:
                    status.device?.firmware ??
                    firmwareSeen?.keyboard ??
                    current.firmware_changed?.current.version ??
                    unknown,
                })}
              </p>
              <Checkbox
                checked={allowFirmware}
                onChange={setAllowFirmware}
                label={r.firmwareAnyway}
                description={r.firmwareAnywayHint}
              />
            </Callout>
          ) : null}
          <section className="br-recovery__section" aria-labelledby={profilesId}>
            <h3 id={profilesId} className="br-section__title">
              {t.profiles}
            </h3>
            <ul className="br-recovery__profiles">
              {current.profiles.map((profile) => {
                const request = singleFor(profile);
                return (
                  <ProfileRow
                    key={profile.profile}
                    profile={profile}
                    picked={picked[profile.profile]}
                    onPick={(token) => {
                      setPicked((map) => ({ ...map, [profile.profile]: token }));
                    }}
                    reason={request === null ? null : reasonFor(request)}
                    onRestore={() => {
                      if (request !== null) onRestore(request);
                    }}
                  />
                );
              })}
            </ul>
            {notChecked ? <p className="ow-muted ow-small">{t.notCheckedNote}</p> : null}
          </section>
          {all === null ? null : (
            <div className="br-recovery__all">
              <Button
                variant={held.length > 1 || notChecked ? "primary" : "secondary"}
                icon="restore"
                disabledReason={reasonFor(all)}
                onClick={() => {
                  onRestore(all);
                }}
              >
                {t.restoreAll}
              </Button>
              <p className="ow-muted ow-small">{t.restoreAllHint({ list: numberList(all.profiles, format) })}</p>
            </div>
          )}
          <p className="ow-muted ow-small">{t.compareHint}</p>
        </>
      );
      break;
    }
  }

  const closable = view !== null && !view.attention;
  return (
    <Card
      title={
        <span ref={titleRef} tabIndex={-1} className="br-recovery__title">
          {t.title}
        </span>
      }
      className="br-recovery"
      actions={
        closable ? (
          <IconButton
            icon="close"
            size="sm"
            label={dict.common.actions.close}
            onClick={() => {
              onClose();
            }}
          />
        ) : undefined
      }
    >
      {body}
      <ConfirmDialog
        open={consent !== null}
        title={r.consentTitle}
        confirmLabel={r.consentConfirm}
        onCancel={() => {
          setConsent(null);
        }}
        footer={<VendorNote confirmation={vendor} />}
        disabledReason={vendor.missing ? dict.flow.review.vendorTickNeeded : null}
        onConfirm={() => {
          if (vendor.missing || consent === null) return;
          vendor.acknowledge();
          const pending = consent;
          setConsent(null);
          openFlow(pending.request, pending.switches);
        }}
      >
        {consent === null ? null : (
          <p>
            {status.active_profile === null
              ? r.consentBodyUnknown({ list: numberList(consent.switches, format), count: consent.switches.length })
              : r.consentBody({
                  list: numberList(consent.switches, format),
                  count: consent.switches.length,
                  active: status.active_profile,
                })}
          </p>
        )}
      </ConfirmDialog>
      {flow === null ? null : (
        <RecoveryFlow
          key={flow.id}
          intent={flow.intent}
          onConsent={(profiles) => {
            const request = flow.request;
            setFlow(null);
            setConsent({ request, switches: profiles });
          }}
          onFirmware={setFirmwareSeen}
          onRestored={() => {
            // The card now says what the restore left (not that settings were kept).
            setKept(false);
            setOwn((n) => n + 1);
            onChanged();
          }}
          onRunAgain={() => {
            begin(flow.request);
          }}
          onClose={() => {
            setFlow(null);
          }}
        />
      )}
    </Card>
  );
}
