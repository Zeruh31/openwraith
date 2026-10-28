// The confirmation flow of an import: prepare (compares the file's profile
// with the keyboard's, switching to the target profile and back when it
// isn't the active one) → the standard restore review in import words →
// explicit Write (armed after 500 ms; Enter never writes) → progress → the
// restore report (re-sent rows, emptied macros and keys left on them, the
// persistence check, Undo).
//
// A file whose Fn layer replaces some of the keyboard's own shortcuts
// (Fn + 1..4, Fn + Space) keeps its key assignments out of the import until
// the user ticks the danger confirmation. Every replaced shortcut has its own
// danger note (in the blocked and in the allowed plan alike), and the one
// tick names every key: it becomes the intent's `allowed_critical_keys` and
// prepares a new plan, which the backend refuses when the file replaces a
// shortcut that wasn't named. Write always confirms the plan shown, and the
// tick is never carried into another flow. Keys the file can't give for
// another reason (an entry OpenWraith doesn't write, …) replace nothing: the
// backend names no shortcut then, and the review shows the general "can't be
// imported" note, also beside a shortcut note when another part stays out.

import { useRef, useState } from "react";

import { ConfirmCheckbox } from "../../../components/ConfirmCheckbox";
import { Callout } from "../../../components/Feedback";
import { ErrorView } from "../../../flow/ErrorView";
import { WriteFlow, type OnProgress, type PrepareErrorView } from "../../../flow/WriteFlow";
import { keyName, useDict, type Dict } from "../../../i18n";
import { useFormat, type Format } from "../../../i18n/format";
import type { AppError, ImportIntent, ImportPreview, KeyRef, RestoreReportView } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { readAgain } from "../../../shell/actions";
import { safetyDirPath, useAppInfo } from "../../../state/appInfo";
import { vendorTickRequired } from "../../../state/session";
import { groupPlan, planChanges } from "../model";
import { RestoreNothing, RestoreReview } from "../RestoreReview";
import { RestoreResult } from "../RestoreResult";
import { builtinOf, chosenParts, editProblemText, partsText } from "./model";

/** The firmware versions a refused comparison named. */
export interface FirmwareSeen {
  file: string | null;
  keyboard: string | null;
}

/** What the file's Fn layer replaces, in words, and the confirmation's label. */
export function criticalTexts(key: Pick<KeyRef, "name">, into: number, dict: Dict): { body: string; confirm: string } {
  const c = dict.backupRestore.share.critical;
  const builtin = builtinOf(key);
  switch (builtin.kind) {
    case "profile":
      return { body: c.profile({ k: builtin.k, n: into }), confirm: c.confirmProfile({ k: builtin.k, n: into }) };
    case "reset":
      return { body: c.reset, confirm: c.confirmReset({ n: into }) };
    case "other": {
      const name = keyName(dict, key.name);
      return { body: c.other({ key: name }), confirm: c.confirmOther({ key: name, n: into }) };
    }
  }
}

/** Fn + k imported into profile k: it switches only to the same profile (a warning, not a danger). */
function sameProfile(key: Pick<KeyRef, "name">, into: number): boolean {
  const builtin = builtinOf(key);
  return builtin.kind === "profile" && builtin.k === into;
}

/** The one confirmation for every replaced shortcut: the key's own label, or one that names them all. */
export function criticalConfirm(keys: readonly Pick<KeyRef, "name">[], into: number, dict: Dict, format: Format): string {
  const [only] = keys;
  if (keys.length === 1 && only !== undefined) return criticalTexts(only, into, dict).confirm;
  const c = dict.backupRestore.share.critical;
  const names = keys.map((key) => c.fnKey({ key: keyName(dict, key.name) }));
  return c.confirmAll({ keys: format.list(names), n: into });
}

/** The import's review: the chosen parts, every replaced Fn shortcut, then the restore review in import words. */
export function ImportReview({ preview }: { preview: ImportPreview }) {
  const dict = useDict();
  const s = dict.backupRestore.share;
  const critical = preview.critical_fn_overrides;
  return (
    <div className="br-review">
      <p>{s.review.chosen({ list: partsText(chosenParts(preview.include), dict) })}</p>
      {/* A part left out for a cause the shortcut permission doesn't lift (beside a replaced shortcut too). */}
      {preview.other_cause_blocked ? (
        <Callout kind="warning">
          <p>{s.review.blocked}</p>
        </Callout>
      ) : null}
      {critical.map((key, index) => (
        <Callout
          key={key.index}
          kind={sameProfile(key, preview.into) ? "warning" : "danger"}
          title={s.critical.title}
          role={index === 0 && preview.critical_blocked ? "alert" : undefined}
        >
          <p>{criticalTexts(key, preview.into, dict).body}</p>
          {preview.critical_blocked && index === critical.length - 1 ? <p>{s.critical.notImported}</p> : null}
        </Callout>
      ))}
      <RestoreReview preview={preview} variant="import" />
    </div>
  );
}

/** Props of {@link ImportFlow}. */
export interface ImportFlowProps {
  /** The import as chosen on the page (`allowed_critical_keys` is the flow's own). */
  intent: ImportIntent;
  /** The backend wants the user to agree to these switches first (the flow is closed). */
  onConsent: (profiles: number[]) => void;
  /** The backend refused a firmware mismatch (the page shows "Import anyway"). */
  onFirmware: (seen: FirmwareSeen) => void;
  /** Called after the apply ran (a new safety backup exists). */
  onImported: () => void;
  /** Runs the same import again (after LOST). */
  onRunAgain: () => void;
  onClose: () => void;
}

/** The confirmation flow of an import. Mount it to open it. */
export function ImportFlow({ intent, onConsent, onFirmware, onImported, onRunAgain, onClose }: ImportFlowProps) {
  const dict = useDict();
  const format = useFormat();
  const info = useAppInfo();
  const s = dict.backupRestore.share;
  const [allowCritical, setAllowCritical] = useState(false);
  // The keys the tick allows, as the next prepare uses them (set at once: the option prepares right away).
  const allowRef = useRef<number[]>([]);
  // Every replaced shortcut the last comparison named (the tick names exactly these).
  const [criticalKeys, setCriticalKeys] = useState<readonly KeyRef[]>([]);
  const switches = intent.confirmed_switches;

  const prepare = async (onProgress: OnProgress): Promise<ImportPreview> => {
    const next: ImportIntent = { ...intent, allowed_critical_keys: [...allowRef.current] };
    if (next.confirmed_switches.length > 0 && vendorTickRequired()) {
      // Another program used the keyboard since the switch was agreed to
      // (Retry or "Review again" in a flow that stayed open): the comparison
      // switches profiles, so the consent asks again, with the vendor tick.
      onConsent([...next.confirmed_switches]);
      const error: AppError = {
        kind: "writes_blocked",
        detail: null,
        message: "the vendor-site confirmation is needed again",
      };
      throw error;
    }
    return api.prepareImport(next, onProgress).then(
      (preview) => {
        setCriticalKeys(preview.critical_fn_overrides);
        if (allowRef.current.length > 0 && (preview.critical_blocked || preview.critical_fn_overrides.length === 0)) {
          // Blocked: never expected (the backend refuses a permission that
          // doesn't cover the file). No replaced shortcut: the keys stay out
          // for another cause, so the tick (no longer shown) allows nothing;
          // it is never kept for a later comparison.
          allowRef.current = [];
          setAllowCritical(false);
        }
        return preview;
      },
      (error: unknown) => {
        const appError = toAppError(error);
        const detail = appError.detail;
        if (detail?.detail === "needs_switch_consent") {
          onConsent(detail.profiles);
        } else if (detail?.detail === "firmware_mismatch") {
          onFirmware({ file: detail.backup, keyboard: detail.keyboard });
        } else if (detail?.detail === "plan_invalidated" && allowRef.current.length > 0) {
          // The file replaces a shortcut the tick didn't name (the keyboard
          // changed since the comparison): Retry compares again without the
          // permission, so every replaced shortcut is shown before a new tick.
          allowRef.current = [];
          setAllowCritical(false);
        }
        throw appError;
      },
    );
  };

  // The comparison may have switched profiles before it failed: say so, and
  // explain the share's own refusals (the shared labels leave "edit" to pages).
  const prepareError = (error: AppError): PrepareErrorView => {
    const detail = error.detail;
    return {
      body: (
        <>
          <ErrorView error={error} written={switches.length > 0 ? "switches" : "nothing"} />
          {detail?.detail === "edit" ? <p>{editProblemText(detail.problem, dict)}</p> : null}
        </>
      ),
    };
  };

  const dir = safetyDirPath(info) ?? "";
  return (
    <WriteFlow<ImportPreview, RestoreReportView>
      kind="import_prepare"
      prepare={prepare}
      token={(preview) => preview.token}
      title={(preview) => s.review.title({ from: preview.from, into: preview.into })}
      review={(preview) => <ImportReview preview={preview} />}
      nothingToDo={(preview) => <RestoreNothing preview={preview} variant="import" />}
      confirmLabel={(preview) => s.review.confirm({ n: planChanges(groupPlan(preview.plan)) })}
      writesSettings
      safetyNote={(preview) => s.review.safetyNote({ n: preview.into, dir })}
      prepareError={prepareError}
      reviewOptions={(preview, reprepare) => {
        const keys = preview.critical_fn_overrides.length > 0 ? preview.critical_fn_overrides : criticalKeys;
        if (keys.length === 0) return null;
        return (
          <ConfirmCheckbox
            checked={allowCritical}
            tone="danger"
            label={criticalConfirm(keys, preview.into, dict, format)}
            onChange={(checked) => {
              allowRef.current = checked ? keys.map((key) => key.index) : [];
              setAllowCritical(checked);
              reprepare();
            }}
          />
        );
      }}
      apply={(token, onProgress) => api.applyImport(token, onProgress).finally(onImported)}
      result={(report) => (
        <RestoreResult
          report={report}
          variant="import"
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
