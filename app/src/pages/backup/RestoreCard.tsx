// The restore wizard: 1. choose a backup, 2. choose profiles (and opt in to
// a firmware mismatch), 3. compare with the keyboard. Comparing other
// profiles switches profiles, so the user agrees to those switches first;
// the backend is the authority and asks again (`needs_switch_consent`)
// when its fresh read differs. The plan is reviewed in a WriteFlow and
// written only after an explicit confirmation, with the plan's token.

import { useId, useState, type Ref } from "react";

import { Button } from "../../components/Button";
import { Checkbox } from "../../components/Controls";
import { ConfirmDialog } from "../../components/Dialog";
import { Callout, Spinner } from "../../components/Feedback";
import { Card } from "../../components/Layout";
import { ErrorView } from "../../flow/ErrorView";
import { useVendorConfirmation, VendorNote } from "../../flow/vendorTick";
import { WriteFlow, type OnProgress } from "../../flow/WriteFlow";
import { useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { AppError, BackupFileSummary, RestoreIntent, RestorePreview, RestoreReportView } from "../../ipc/bindings";
import { api } from "../../ipc/commands";
import { toAppError } from "../../ipc/errors";
import { readAgain } from "../../shell/actions";
import { safetyDirPath, useAppInfo } from "../../state/appInfo";
import { isLocked } from "../../state/operation";
import { vendorTickRequired } from "../../state/session";
import { useStatus } from "../../state/status";
import { useDeviceBlock } from "./availability";
import { useDemoAction, type BackupDemo } from "./demo";
import { FileSummary } from "./FileSummary";
import { buildRestoreIntent, groupPlan, planChanges, profileChoices, restoreScope, switchesFor } from "./model";
import { RestoreNothing, RestoreReview } from "./RestoreReview";
import { RestoreResult } from "./RestoreResult";
import { numberList } from "./texts";
import type { BackupSource } from "./useBackupSource";

/** A numbered step heading. */
function StepTitle({
  n,
  id,
  children,
  ref,
}: {
  n: number;
  id: string;
  children: string;
  ref?: Ref<HTMLHeadingElement> | undefined;
}) {
  return (
    <h3 id={id} className="br-step__title" tabIndex={-1} ref={ref}>
      <span className="br-step__num" aria-hidden="true">
        {n}
      </span>
      <span>{children}</span>
    </h3>
  );
}

function ignore(): void {
  // A profile that isn't in the backup can't be chosen.
}

interface FirmwareSeen {
  backup: string | null;
  keyboard: string | null;
}

/** Props of {@link RestoreSteps}. */
interface RestoreStepsProps {
  summary: BackupFileSummary;
  demo: BackupDemo | undefined;
  /** Compare once right after mounting (browser preview only). */
  autoCompare: boolean;
  onRestored: () => void;
}

/** Steps 2 and 3 for one loaded backup (remounted for every new backup). */
function RestoreSteps({ summary, demo, autoCompare, onRestored }: RestoreStepsProps) {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const info = useAppInfo();
  const block = useDeviceBlock();
  // Comparing other profiles switches profiles (the first device write of
  // the run, perhaps): the same vendor-site confirmation as every write.
  const vendor = useVendorConfirmation();
  const t = dict.backupRestore.restore;
  const reasons = dict.backupRestore.reasons;
  const profilesId = useId();
  const compareId = useId();
  const [selected, setSelected] = useState<ReadonlySet<number>>(() => {
    const wanted = demo?.profiles;
    return new Set(wanted === undefined ? summary.profiles : summary.profiles.filter((p) => wanted.includes(p)));
  });
  const [allowFirmware, setAllowFirmware] = useState(demo?.allowFirmware === true);
  const [firmwareSeen, setFirmwareSeen] = useState<FirmwareSeen | null>(null);
  const [consent, setConsent] = useState<number[] | null>(null);
  const [flow, setFlow] = useState<{ id: number; intent: RestoreIntent } | null>(null);
  const [flowCount, setFlowCount] = useState(0);

  const scope = restoreScope(summary.profiles, selected);
  const firmwareDiffers = summary.same_firmware === false || firmwareSeen !== null;
  const reason =
    block ??
    (summary.same_model === false
      ? reasons.otherKeyboard
      : scope.length === 0
        ? reasons.noProfile
        : firmwareDiffers && !allowFirmware
          ? reasons.firmwareTick
          : null);

  const openFlow = (switches: readonly number[]) => {
    const id = flowCount + 1;
    setFlowCount(id);
    setFlow({
      id,
      intent: buildRestoreIntent({
        source: summary.file.token,
        fileProfiles: summary.profiles,
        scope,
        allowFirmwareMismatch: allowFirmware,
        switches,
      }),
    });
  };

  /** Asks for the profile switches (when there are any), then prepares the plan. */
  const begin = () => {
    const switches = switchesFor(scope, status.active_profile);
    setFlow(null);
    if (switches.length > 0) {
      setConsent(switches);
    } else {
      openFlow([]);
    }
  };

  const onCompare = () => {
    if (reason !== null || flow !== null || consent !== null || isLocked()) return;
    begin();
  };

  useDemoAction(autoCompare, onCompare);

  const prepare = async (intent: RestoreIntent, onProgress: OnProgress): Promise<RestorePreview> => {
    if (intent.confirmed_switches.length > 0 && vendorTickRequired()) {
      // Another program used the keyboard since the switches were agreed to
      // (Retry or "Review again" in a dialog that stayed open): the
      // comparison switches profiles, so the consent dialog asks again, with
      // the vendor tick, before anything is sent.
      setFlow(null);
      setConsent([...intent.confirmed_switches]);
      const error: AppError = {
        kind: "writes_blocked",
        detail: null,
        message: "the vendor-site confirmation is needed again",
      };
      throw error;
    }
    return api.prepareRestore(intent, onProgress).catch((error: unknown) => {
      const appError = toAppError(error);
      const detail = appError.detail;
      if (detail?.detail === "needs_switch_consent") {
        // The keyboard is on another profile than shown: ask for the switches the backend named.
        setFlow(null);
        setConsent(detail.profiles);
      } else if (detail?.detail === "firmware_mismatch") {
        setFirmwareSeen({ backup: detail.backup, keyboard: detail.keyboard });
      }
      throw appError;
    });
  };

  const dir = safetyDirPath(info) ?? "";
  const scopeOf = (preview: RestorePreview) => preview.plan.profiles.map((p) => p.profile);
  const unknown = dict.common.unknown;

  return (
    <>
      <section className="br-step" aria-labelledby={profilesId}>
        <StepTitle n={2} id={profilesId}>
          {t.step2}
        </StepTitle>
        <div className="br-profiles" role="group" aria-labelledby={profilesId}>
          {profileChoices(summary.profiles).map((profile) => {
            const inFile = summary.profiles.includes(profile);
            if (!inFile) {
              // The note stays outside the dimmed checkbox, so it keeps its contrast.
              return (
                <div key={profile} className="br-profile-off">
                  <Checkbox checked={false} disabled label={dict.common.profile({ n: profile })} onChange={ignore} />
                  <p className="ow-muted ow-small">{t.notInBackup}</p>
                </div>
              );
            }
            return (
              <Checkbox
                key={profile}
                checked={selected.has(profile)}
                label={dict.common.profile({ n: profile })}
                onChange={(checked) => {
                  setSelected((current) => {
                    const next = new Set(current);
                    if (checked) next.add(profile);
                    else next.delete(profile);
                    return next;
                  });
                }}
              />
            );
          })}
        </div>
        {firmwareDiffers ? (
          <Callout kind="warning" title={t.firmwareTitle}>
            <p>
              {t.firmwareBody({
                backup: summary.firmware ?? firmwareSeen?.backup ?? unknown,
                keyboard: status.device?.firmware ?? firmwareSeen?.keyboard ?? unknown,
              })}
            </p>
            <Checkbox
              checked={allowFirmware}
              onChange={setAllowFirmware}
              label={t.firmwareAnyway}
              description={t.firmwareAnywayHint}
            />
          </Callout>
        ) : null}
      </section>
      <section className="br-step" aria-labelledby={compareId}>
        <StepTitle n={3} id={compareId}>
          {t.step3}
        </StepTitle>
        <p className="ow-muted">{t.compareHint}</p>
        <div className="ow-row">
          <Button variant="primary" icon="refresh" disabledReason={reason} onClick={onCompare}>
            {t.compare}
          </Button>
        </div>
      </section>
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
          if (vendor.missing) return;
          vendor.acknowledge();
          const switches = consent;
          setConsent(null);
          if (switches !== null) openFlow(switches);
        }}
      >
        {consent === null ? null : (
          <div className="ow-stack">
            <p>
              {status.active_profile === null
                ? t.consentBodyUnknown({ list: numberList(consent, format), count: consent.length })
                : t.consentBody({
                    list: numberList(consent, format),
                    count: consent.length,
                    active: status.active_profile,
                  })}
            </p>
          </div>
        )}
      </ConfirmDialog>
      {flow === null ? null : (
        <WriteFlow<RestorePreview, RestoreReportView>
          key={flow.id}
          kind="restore_prepare"
          prepare={(onProgress) => prepare(flow.intent, onProgress)}
          token={(preview) => preview.token}
          title={(preview) => {
            const profiles = scopeOf(preview);
            const words = {
              list: numberList(profiles, format),
              count: profiles.length,
              date: format.dateTime(preview.source.created_at),
            };
            switch (preview.source.kind) {
              case "safety":
                return dict.backupRestore.review.undoTitle(words);
              case "automatic":
                return dict.backupRestore.review.autoTitle(words);
              case "user":
                return dict.backupRestore.review.title;
            }
          }}
          review={(preview) => <RestoreReview preview={preview} />}
          nothingToDo={(preview) => <RestoreNothing preview={preview} />}
          confirmLabel={(preview) => dict.backupRestore.review.confirm({ n: planChanges(groupPlan(preview.plan)) })}
          writesSettings
          safetyNote={(preview) => {
            const profiles = scopeOf(preview);
            return dict.backupRestore.review.safetyNote({
              list: numberList(profiles, format),
              count: profiles.length,
              dir,
            });
          }}
          apply={(token, onProgress) => api.applyRestore(token, onProgress).finally(onRestored)}
          result={(report) => (
            <RestoreResult
              report={report}
              onRunAgain={begin}
              onReadAgain={() => {
                setFlow(null);
                readAgain();
              }}
            />
          )}
          onClose={() => {
            setFlow(null);
          }}
        />
      )}
    </>
  );
}

/** Props of {@link RestoreCard}. */
export interface RestoreCardProps {
  source: BackupSource;
  demo: BackupDemo | undefined;
  /** Compare once the first backup is loaded (browser preview only). */
  autoCompare: boolean;
  /** Called after a restore was applied (a new safety backup exists). */
  onRestored: () => void;
  /** The first step's heading (focused when a safety backup is chosen below). */
  headingRef?: Ref<HTMLHeadingElement> | undefined;
}

/** The "Restore" card. */
export function RestoreCard({ source, demo, autoCompare, onRestored, headingRef }: RestoreCardProps) {
  const dict = useDict();
  const t = dict.backupRestore.restore;
  const chooseId = useId();
  const summary = source.summary;
  const openFile = () => {
    source.openFile({ title: dict.backupRestore.dialog.open, filter: dict.backupRestore.dialog.filter });
  };
  return (
    <Card title={t.title} className="br-restore">
      <p className="ow-muted">{t.intro}</p>
      <section className="br-step" aria-labelledby={chooseId}>
        <StepTitle n={1} id={chooseId} ref={headingRef}>
          {t.step1}
        </StepTitle>
        <div className="ow-row">
          <Button icon="folder" variant={summary === null ? "primary" : "secondary"} onClick={openFile}>
            {summary === null ? t.openFile : t.openOther}
          </Button>
        </div>
        {source.loading ? (
          <p className="ow-row" role="status">
            <Spinner />
            <span>{t.opening}</span>
          </p>
        ) : null}
        {source.error === null ? null : <ErrorView error={source.error} written="none" />}
        {summary === null ? (
          source.error === null ? (
            <p className="ow-muted ow-small">{t.orSafety}</p>
          ) : null
        ) : (
          <FileSummary summary={summary} />
        )}
      </section>
      {summary === null ? null : (
        <RestoreSteps
          key={source.loadId}
          summary={summary}
          demo={demo}
          autoCompare={autoCompare && source.loadId === 1}
          onRestored={onRestored}
        />
      )}
    </Card>
  );
}
