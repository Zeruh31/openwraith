// Settings > Games: per-game automatic profiles (design B7). Off by
// default; turning them on shows the explanation once (its tick also
// stands for the vendor-site confirmation of automatic switches). Each rule
// is the user's consent for it. Every change is saved at once (the rules
// live in settings.json; nothing is written to the keyboard here: the
// automatic switch itself is the backend's, through its gates).

import { useEffect, useId, useRef, useState } from "react";

import { Button } from "../../../components/Button";
import { Switch } from "../../../components/Controls";
import { Callout, EmptyState, Spinner } from "../../../components/Feedback";
import { Card, Disclosure } from "../../../components/Layout";
import { useRestoreFocus } from "../../../components/useRestoreFocus";
import { ErrorView } from "../../../flow/ErrorView";
import { useDict, type Dict } from "../../../i18n";
import { useFormat, type Format } from "../../../i18n/format";
import type { AppError, AutoActivityView, AutoProfileRuleView } from "../../../ipc/bindings";
import { navigate, tabTarget } from "../../../routes";
import { useHint, useHints } from "../../../state/hints";
import { PROFILES, profileLabel, useProfileNames } from "../../../state/profileNames";
import { toast } from "../../../state/toasts";
import { AddGameDialog } from "./AddGameDialog";
import { ConsentDialog, HowItWorksList } from "./HowItWorks";
import {
  eventText,
  lineText,
  withDefaultProfile,
  withEnabled,
  withRuleAdded,
  withRuleChanged,
  withRuleRemoved,
  type NewRule,
  type RulePatch,
} from "./model";
import { AddGameButton, RulesTable } from "./RulesTable";
import { useAutoProfiles } from "./useAutoProfiles";

/** The one-time explanation before game profiles are turned on (remembered with the tips). */
export const CONSENT_HINT = "settings.games.consent";

/** A time in the activity list: the time of day today, else the date too. */
function activityTime(at: string, format: Format): string {
  const date = new Date(at);
  const today = new Date();
  return !Number.isNaN(date.getTime()) && date.toDateString() === today.toDateString()
    ? format.time(at)
    : format.dateTime(at);
}

function saveFailedToast(error: AppError, dict: Dict): void {
  toast({ kind: "danger", title: dict.settings.games.saveFailed, details: error.message });
}

/** The recent activity (the last 20, newest first). */
function RecentActivity({ recent, profile }: { recent: readonly AutoActivityView[]; profile: (n: number) => string }) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.settings.games.recent;
  return (
    <Card title={t.title} className="st-wide">
      {recent.length === 0 ? (
        <p className="ow-muted">{t.empty}</p>
      ) : (
        <ol className="st-activity">
          {recent.map((entry, index) => (
            <li key={`${entry.at}-${String(index)}`} className="st-activity__item">
              <time dateTime={entry.at} className="st-activity__time ow-num ow-muted">
                {activityTime(entry.at, format)}
              </time>
              <span>{eventText(entry.event, dict, profile)}</span>
            </li>
          ))}
        </ol>
      )}
    </Card>
  );
}

/** Settings > Games. */
export function GamesTab() {
  const dict = useDict();
  const t = dict.settings.games;
  const names = useProfileNames();
  const hints = useHints();
  const consentHint = useHint(CONSENT_HINT);
  const { view, loadError, saving, reload, save, pause } = useAutoProfiles();
  const [consent, setConsent] = useState(false);
  const [adding, setAdding] = useState(false);
  const addRef = useRef<HTMLButtonElement>(null);
  /** A removed row took the focus with it: "Add a game…" gets it once it is enabled again. */
  const focusAdd = useRef(false);
  const defaultId = useId();

  const moveFocusToAdd = () => {
    const button = addRef.current;
    if (focusAdd.current && button !== null && !button.disabled) {
      focusAdd.current = false;
      button.focus();
    }
  };
  useEffect(moveFocusToAdd, [saving]);
  useRestoreFocus(saving);

  const profile = (n: number) => profileLabel(dict, n, names[n - 1] ?? null);

  if (view === null) {
    return loadError === null ? (
      <Spinner label={dict.common.loading} size={24} />
    ) : (
      <div className="ow-stack">
        <p className="ow-strong">{t.loadFailed}</p>
        <ErrorView error={loadError} written="none" />
        <div className="ow-row">
          <Button icon="refresh" onClick={reload}>
            {dict.common.actions.retry}
          </Button>
        </div>
      </div>
    );
  }

  const saveIntent = (intent: Parameters<typeof save>[0]) => {
    void save(intent).then((error) => {
      if (error !== null) saveFailedToast(error, dict);
    });
  };

  const turn = (enabled: boolean) => {
    if (enabled && (!hints.loaded || consentHint.show)) {
      setConsent(true);
      return;
    }
    saveIntent(withEnabled(view, enabled));
  };

  const changeRule = (id: string, patch: RulePatch) => {
    saveIntent(withRuleChanged(view, id, patch));
  };

  const removeRule = (rule: AutoProfileRuleView) => {
    void save(withRuleRemoved(view, rule.id)).then((error) => {
      if (error !== null) {
        saveFailedToast(error, dict);
        return;
      }
      toast({ kind: "success", title: t.rules.removed({ label: rule.label }) });
      // The row and its button are gone: the focus goes to "Add a game…".
      focusAdd.current = true;
      moveFocusToAdd();
    });
  };

  const addRule = async (rule: NewRule): Promise<AppError | null> => {
    const error = await save(withRuleAdded(view, rule));
    if (error === null) setAdding(false);
    return error;
  };

  const paused = view.status.paused;
  return (
    <div className="st-grid">
      <Card className="st-wide">
        <div className="ow-stack">
          <Switch
            checked={view.enabled}
            disabled={saving}
            label={<span className="ow-strong">{t.toggle}</span>}
            description={t.toggleHelp}
            onChange={turn}
          />
          <p className="st-games__line" role="status">
            <span className="ow-strong">{t.statusLabel}</span>
            {": "}
            <span>{lineText(view.line, dict, profile)}</span>
          </p>
          {view.enabled ? (
            <div className="st-games__pause">
              <Button
                size="sm"
                disabled={saving}
                onClick={() => {
                  void pause(paused === null).then((error) => {
                    if (error !== null) saveFailedToast(error, dict);
                  });
                }}
              >
                {paused === null ? t.pause : t.resume}
              </Button>
              <span className="ow-muted ow-small">{t.pauseHelp}</span>
            </div>
          ) : null}
          {view.tray_enabled ? null : (
            <Callout
              kind="info"
              actions={
                <Button
                  size="sm"
                  onClick={() => {
                    navigate(tabTarget("settings", "background"));
                  }}
                >
                  {t.openBackground}
                </Button>
              }
            >
              <p>{t.needsTray}</p>
            </Callout>
          )}
        </div>
      </Card>

      <Card
        title={t.rules.title}
        className="st-wide"
        actions={
          <AddGameButton
            buttonRef={addRef}
            disabled={saving}
            onClick={() => {
              setAdding(true);
            }}
          />
        }
      >
        <div className="ow-stack">
          {view.rules.length === 0 ? (
            <EmptyState icon="info" title={t.rules.empty} />
          ) : (
            <RulesTable rules={view.rules} disabled={saving} onChange={changeRule} onRemove={removeRule} />
          )}
          <div className="st-games__default">
            <label htmlFor={defaultId} className="ow-field__label">
              {t.defaultLabel}
            </label>
            <select
              id={defaultId}
              className="ow-input"
              value={view.default_profile === null ? "" : String(view.default_profile)}
              disabled={saving}
              aria-describedby={`${defaultId}-help`}
              onChange={(event) => {
                const value = event.currentTarget.value;
                saveIntent(withDefaultProfile(view, value === "" ? null : Number(value)));
              }}
            >
              <option value="">{t.defaultStay}</option>
              {PROFILES.map((n) => (
                <option key={n} value={String(n)}>
                  {t.defaultOption({ label: profile(n) })}
                </option>
              ))}
            </select>
            <p id={`${defaultId}-help`} className="ow-field__notice">
              {t.defaultHelp}
            </p>
          </div>
        </div>
      </Card>

      <RecentActivity recent={view.recent} profile={profile} />

      <div className="st-wide">
        <Disclosure summary={t.howItWorks.title}>
          <HowItWorksList />
        </Disclosure>
      </div>

      {consent ? (
        <ConsentDialog
          onCancel={() => {
            setConsent(false);
          }}
          onConfirm={() => {
            setConsent(false);
            consentHint.dismiss();
            saveIntent(withEnabled(view, true));
          }}
        />
      ) : null}

      {adding ? (
        <AddGameDialog
          programsAvailable={view.programs_available}
          initialProfile={view.rules[view.rules.length - 1]?.profile ?? 2}
          onAdd={addRule}
          onClose={() => {
            setAdding(false);
          }}
        />
      ) : null}
    </div>
  );
}
