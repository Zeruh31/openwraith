// Export a profile: choose the profile and an optional name, then the file
// in the system save dialog (the backend hands back a token, never a path).
// The active profile is a pure read; another profile is read by switching
// to it and back, which the user agrees to first (with the vendor-site
// confirmation, as every switch). The backend is the authority and asks
// again (`needs_switch_consent`) when the active profile isn't what the page
// showed. Nothing is written to the keyboard's settings.

import { useId, useRef, useState } from "react";

import { Button } from "../../../components/Button";
import { ConfirmDialog } from "../../../components/Dialog";
import { Callout } from "../../../components/Feedback";
import { Icon } from "../../../components/Icon";
import { useVendorConfirmation, VendorNote } from "../../../flow/vendorTick";
import { OperationRunner } from "../../../flow/WriteFlow";
import { useDict } from "../../../i18n";
import type { ExportedProfile, ExportIntent, FileRef } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { toastError } from "../../../shell/actions";
import { isLocked } from "../../../state/operation";
import { statusStore, useStatus } from "../../../state/status";
import { useDeviceBlock } from "../availability";
import { SavedFile } from "../BackupCard";
import { useDemoAction, type BackupDemo } from "../demo";
import { KEYBOARD_PROFILES } from "../model";
import { buildExportIntent, defaultInto, exportNeedsSwitch, LABEL_MAX } from "./model";
import { ShareContents } from "./ShareContents";

/** A saved export, as the result dialog shows it. */
export function ExportedView({ exported }: { exported: ExportedProfile }) {
  const dict = useDict();
  const share = dict.backupRestore.share;
  const t = share.export;
  return (
    <div className="ow-report br-saved">
      <Callout kind="success" title={t.saved}>
        <p>{t.savedSummary({ n: exported.profile, firmware: exported.firmware ?? dict.common.unknown })}</p>
        {exported.switched ? <p>{t.switched({ n: exported.profile })}</p> : null}
      </Callout>
      <SavedFile file={exported.file} label={t.file} />
      <ShareContents contents={exported.contents} title={share.contents.title} />
      <p className="ow-muted ow-small">{share.noSerial}</p>
    </div>
  );
}

/** An export waiting for its file or its switch consent. */
interface Pending {
  profile: number;
  name: string;
  destination: FileRef;
}

/** Props of {@link ExportPanel}. */
export interface ExportPanelProps {
  demo: BackupDemo | undefined;
}

/** The "Export a profile" half of "Share a profile". */
export function ExportPanel({ demo }: ExportPanelProps) {
  const dict = useDict();
  const status = useStatus();
  const block = useDeviceBlock();
  const vendor = useVendorConfirmation();
  const share = dict.backupRestore.share;
  const t = share.export;
  const headingId = useId();
  const profileId = useId();
  const nameId = useId();
  const nameHintId = useId();
  const active = status.active_profile;
  // `null` follows the active profile until the user picks one.
  const [picked, setPicked] = useState<number | null>(null);
  const profile = picked ?? defaultInto(active);
  const [name, setName] = useState("");
  const [consent, setConsent] = useState<Pending | null>(null);
  const [running, setRunning] = useState<{ id: number; pending: Pending; intent: ExportIntent } | null>(null);
  const runs = useRef(0);
  const choosing = useRef(false);

  const run = (pending: Pending, confirmedSwitch: boolean) => {
    runs.current += 1;
    setRunning({
      id: runs.current,
      pending,
      intent: buildExportIntent({
        profile: pending.profile,
        name: pending.name,
        destination: pending.destination.token,
        confirmedSwitch,
      }),
    });
  };

  /** Asks for the file, then for the switch (when another profile is read), then exports. */
  const start = (target: number) => {
    if (choosing.current || running !== null || consent !== null || block !== null || isLocked()) return;
    choosing.current = true;
    const label = name;
    api
      .chooseExportDestination(target, { title: share.dialog.save, filter: share.dialog.saveFilter })
      .then(
        (destination) => {
          if (destination === null) return;
          const pending: Pending = { profile: target, name: label, destination };
          // The status of now, not of the render that started the dialog.
          if (exportNeedsSwitch(target, statusStore.get().active_profile)) {
            setConsent(pending);
          } else {
            run(pending, false);
          }
        },
        (error: unknown) => {
          toastError(error);
        },
      )
      .finally(() => {
        choosing.current = false;
      });
  };

  useDemoAction(demo?.share === "export", () => {
    const target = demo?.exportProfile ?? profile;
    setPicked(target);
    start(target);
  });

  const switching = active !== null && exportNeedsSwitch(profile, active);
  return (
    <section className="br-share__panel" aria-labelledby={headingId}>
      <h3 id={headingId} className="br-share__title">
        <Icon name="backup" size={18} className="br-share__icon" />
        <span>{t.title}</span>
      </h3>
      <p className="ow-muted">{t.intro}</p>
      <div className="br-share__fields">
        <div className="br-share__field">
          <label htmlFor={profileId} className="ow-field__label">
            {t.profile}
          </label>
          <select
            id={profileId}
            className="ow-input br-share__select"
            value={profile}
            onChange={(event) => {
              setPicked(Number(event.currentTarget.value));
            }}
          >
            {KEYBOARD_PROFILES.map((n) => (
              <option key={n} value={n}>
                {share.profileOption({ n, active: n === active })}
              </option>
            ))}
          </select>
        </div>
        <div className="br-share__field br-share__field--wide">
          <label htmlFor={nameId} className="ow-field__label">
            {t.name}
          </label>
          <input
            id={nameId}
            type="text"
            className="ow-input br-share__name"
            value={name}
            maxLength={LABEL_MAX}
            autoComplete="off"
            spellCheck={false}
            aria-describedby={nameHintId}
            onChange={(event) => {
              setName(event.currentTarget.value);
            }}
          />
          <p id={nameHintId} className="ow-muted ow-small">
            {t.nameHint}
          </p>
        </div>
      </div>
      <p>{t.holds}</p>
      <p className="ow-muted ow-small">{t.macrosNote}</p>
      {switching ? <p className="ow-muted ow-small">{t.switchNote({ n: profile })}</p> : null}
      <div className="ow-row">
        <Button
          variant="primary"
          icon="backup"
          disabledReason={block}
          onClick={() => {
            start(profile);
          }}
        >
          {t.button({ n: profile })}
        </Button>
      </div>
      <ConfirmDialog
        open={consent !== null}
        title={t.consentTitle}
        confirmLabel={t.consentConfirm}
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
          run(pending, true);
        }}
      >
        {consent === null ? null : (
          <p>
            {active === null
              ? t.consentBodyUnknown({ n: consent.profile })
              : t.consentBody({ n: consent.profile, active })}
          </p>
        )}
      </ConfirmDialog>
      {running === null ? null : (
        <OperationRunner<ExportedProfile>
          key={running.id}
          kind="export"
          title={dict.flow.opKinds.export}
          run={(onProgress) =>
            api.exportProfile(running.intent, onProgress).catch((error: unknown) => {
              const appError = toAppError(error);
              if (appError.detail?.detail === "needs_switch_consent") {
                // The keyboard is on another profile than shown: ask for the switch first.
                setRunning(null);
                setConsent(running.pending);
              }
              throw appError;
            })
          }
          result={(exported) => <ExportedView exported={exported} />}
          errorNote={() => t.noFileSaved}
          onClose={() => {
            setRunning(null);
          }}
        />
      )}
    </section>
  );
}
