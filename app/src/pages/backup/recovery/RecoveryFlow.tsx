// The confirmation flow of a recovery: prepare (`prepare_recovery` compares
// the chosen automatic backups with the keyboard, switching to the other
// profiles and back when the user agreed; several backups are composed into
// one) → the standard restore review → explicit Write (armed after 500 ms;
// Enter never writes) → progress → the restore report (Undo = its safety
// backup). The plan is a restore plan: `apply_restore` writes it with its
// token. Nothing is written before Write.

import { ErrorView } from "../../../flow/ErrorView";
import { WriteFlow, type OnProgress, type PrepareErrorView } from "../../../flow/WriteFlow";
import { useDict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type { AppError, RecoveryIntent, RestorePreview, RestoreReportView } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { readAgain } from "../../../shell/actions";
import { safetyDirPath, useAppInfo } from "../../../state/appInfo";
import { vendorTickRequired } from "../../../state/session";
import { groupPlan, planChanges } from "../model";
import { RestoreNothing, RestoreReview } from "../RestoreReview";
import { RestoreResult } from "../RestoreResult";
import { numberList } from "../texts";

/** The firmware versions a refused comparison named. */
export interface FirmwareSeen {
  backup: string | null;
  keyboard: string | null;
}

/** Props of {@link RecoveryFlow}. */
export interface RecoveryFlowProps {
  intent: RecoveryIntent;
  /** The backend wants the user to agree to these switches first (the flow is closed). */
  onConsent: (profiles: number[]) => void;
  /** The backend refused a firmware mismatch (the card shows "Restore anyway"). */
  onFirmware: (seen: FirmwareSeen) => void;
  /** Called after the apply ran (a new safety backup exists, the hold may be gone). */
  onRestored: () => void;
  /** Runs the same comparison again (after LOST). */
  onRunAgain: () => void;
  onClose: () => void;
}

/** The confirmation flow of a recovery. Mount it to open it. */
export function RecoveryFlow({ intent, onConsent, onFirmware, onRestored, onRunAgain, onClose }: RecoveryFlowProps) {
  const dict = useDict();
  const format = useFormat();
  const info = useAppInfo();
  const t = dict.backupRestore;
  const switches = intent.confirmed_switches;

  const prepare = async (onProgress: OnProgress): Promise<RestorePreview> => {
    if (switches.length > 0 && vendorTickRequired()) {
      // Another program used the keyboard since the switches were agreed to
      // (Retry in a flow that stayed open): the comparison switches
      // profiles, so the consent asks again, with the vendor tick.
      onConsent([...switches]);
      const error: AppError = {
        kind: "writes_blocked",
        detail: null,
        message: "the vendor-site confirmation is needed again",
      };
      throw error;
    }
    return api.prepareRecovery(intent, onProgress).catch((error: unknown) => {
      const appError = toAppError(error);
      const detail = appError.detail;
      if (detail?.detail === "needs_switch_consent") {
        // The keyboard is on another profile than shown: ask for the switches the backend named.
        onConsent(detail.profiles);
      } else if (detail?.detail === "firmware_mismatch") {
        onFirmware({ backup: detail.backup, keyboard: detail.keyboard });
      }
      throw appError;
    });
  };

  // The comparison may have switched profiles before it failed: say so.
  const prepareError = (error: AppError): PrepareErrorView => ({
    body: <ErrorView error={error} written={switches.length > 0 ? "switches" : "nothing"} />,
  });

  const scopeOf = (preview: RestorePreview) => preview.plan.profiles.map((p) => p.profile);
  const dir = safetyDirPath(info) ?? "";
  return (
    <WriteFlow<RestorePreview, RestoreReportView>
      kind="recovery_check"
      prepare={prepare}
      token={(preview) => preview.token}
      title={(preview) => {
        const profiles = scopeOf(preview);
        return t.recovery.reviewTitle({ list: numberList(profiles, format), count: profiles.length });
      }}
      review={(preview) => <RestoreReview preview={preview} />}
      nothingToDo={(preview) => <RestoreNothing preview={preview} />}
      confirmLabel={(preview) => t.review.confirm({ n: planChanges(groupPlan(preview.plan)) })}
      writesSettings
      safetyNote={(preview) => {
        const profiles = scopeOf(preview);
        return t.review.safetyNote({ list: numberList(profiles, format), count: profiles.length, dir });
      }}
      prepareError={prepareError}
      apply={(token, onProgress) => api.applyRestore(token, onProgress).finally(onRestored)}
      result={(report) => (
        <RestoreResult
          report={report}
          onRunAgain={onRunAgain}
          onReadAgain={() => {
            onClose();
            readAgain();
          }}
        />
      )}
      onClose={onClose}
    />
  );
}
