// Settings > Background (design B5, B11): the notification-area icon, start
// with Windows (only with the icon), notifications and automatic backups.
// Each change is saved at once; the backend then tells the shell (tray icon,
// Run entry) and says when Windows didn't take it (the setting stays saved).
// What Windows will do at the next sign-in (`windows_start`) is read again
// with the settings; while it differs from the switch, a callout under the
// switch says why and how to fix it (Task Manager's choice is the user's:
// OpenWraith never changes it).

import { useEffect, useId, useRef, useState } from "react";

import { Button } from "../../components/Button";
import { Switch } from "../../components/Controls";
import { Callout, Spinner } from "../../components/Feedback";
import { Card } from "../../components/Layout";
import { useRestoreFocus } from "../../components/useRestoreFocus";
import { ErrorView } from "../../flow/ErrorView";
import { useDict } from "../../i18n";
import type { Dict } from "../../i18n";
import type { AppError, BackgroundSettingsIntent, BackgroundSettingsView } from "../../ipc/bindings";
import { toAppError } from "../../ipc/errors";
import { navigate } from "../../routes";
import { useAppInfo } from "../../state/appInfo";
import {
  backgroundIntent,
  loadBackgroundSettings,
  saveBackgroundSettings,
  useBackgroundSettings,
} from "../../state/background";
import { toast } from "../../state/toasts";

/** The number of automatic backups kept per profile: 1-100. */
export const KEEP_MIN = 1;
export const KEEP_MAX = 100;

/** The typed "keep" value, or `null` when it isn't a whole number from 1 to 100. */
export function parseKeep(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d{1,3}$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return value >= KEEP_MIN && value <= KEEP_MAX ? value : null;
}

/** The "keep N per profile" field: saved on Save, Enter or leaving the field. */
function KeepField({
  view,
  disabled,
  onSave,
}: {
  view: BackgroundSettingsView;
  disabled: boolean;
  onSave: (keep: number) => void;
}) {
  const dict = useDict();
  const t = dict.settings.background;
  const id = useId();
  const [text, setText] = useState(String(view.auto_backup.keep));
  const [shownFor, setShownFor] = useState(view.auto_backup.keep);
  // A newer saved value replaces what the field shows.
  if (shownFor !== view.auto_backup.keep) {
    setShownFor(view.auto_backup.keep);
    setText(String(view.auto_backup.keep));
  }
  const parsed = parseKeep(text);
  const commit = () => {
    if (parsed !== null && parsed !== view.auto_backup.keep) onSave(parsed);
  };
  return (
    <div className="st-keep">
      <label htmlFor={id} className="ow-field__label">
        {t.keep}
      </label>
      <div className="ow-row">
        <input
          id={id}
          type="number"
          inputMode="numeric"
          min={KEEP_MIN}
          max={KEEP_MAX}
          step={1}
          className="ow-input st-keep__input"
          value={text}
          disabled={disabled}
          aria-invalid={parsed === null}
          aria-describedby={`${id}-help`}
          onChange={(event) => {
            setText(event.currentTarget.value);
          }}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              commit();
            }
          }}
        />
        <Button size="sm" disabled={disabled || parsed === null || parsed === view.auto_backup.keep} onClick={commit}>
          {t.keepSave}
        </Button>
      </div>
      <p id={`${id}-help`} className={`ow-field__notice${parsed === null ? " ow-field__notice--error" : ""}`}>
        {parsed === null ? t.keepInvalid : t.keepHelp({ n: parsed })}
      </p>
    </div>
  );
}

/**
 * The callout's texts while Windows won't do what the switch says, else `null`. An entry left
 * behind without the icon doesn't point to the switch (disabled then, and turning the icon on and
 * off doesn't retry the removal): Task Manager, or a new start, whose check removes it. A simulator
 * build (`sim`) never has an entry, so it has nothing to fix there.
 */
export function windowsStartProblem(
  view: BackgroundSettingsView,
  t: Dict["settings"]["background"]["windowsStart"],
  sim = false,
): { title: string; body: string } | null {
  const wanted = view.tray_enabled && view.start_with_windows;
  switch (view.windows_start) {
    case "off_in_task_manager":
      return wanted ? { title: t.offInTaskManagerTitle, body: t.offInTaskManagerBody } : null;
    case "entry_missing":
      return wanted ? { title: t.entryMissingTitle, body: t.entryMissingBody } : null;
    case "unavailable":
      return wanted && !sim ? { title: t.unavailableTitle, body: t.unavailableBody } : null;
    case "still_on":
      if (wanted) return null;
      return { title: t.stillOnTitle, body: view.tray_enabled ? t.stillOnBody : t.stillOnNoTrayBody };
    case "on":
    case "off":
      return null;
  }
}

/** Settings > Background. */
export function BackgroundTab() {
  const dict = useDict();
  const t = dict.settings.background;
  const view = useBackgroundSettings();
  const info = useAppInfo();
  const [loadError, setLoadError] = useState<AppError | null>(null);
  const [saving, setSaving] = useState(false);
  const [checking, setChecking] = useState(false);
  const [shellError, setShellError] = useState<AppError | null>(null);
  const busy = useRef(false);
  useEffect(() => {
    busy.current = saving || checking;
  }, [saving, checking]);
  useRestoreFocus(saving);

  useEffect(() => {
    let alive = true;
    loadBackgroundSettings().catch((error: unknown) => {
      if (alive) setLoadError(toAppError(error));
    });
    // Back from Task Manager: read what Windows will do again (not while a
    // change is being saved, which answers with it anyway).
    const refresh = () => {
      if (busy.current) return;
      loadBackgroundSettings().catch(() => {
        // The page keeps what it shows; "Check again" reports a failure.
      });
    };
    window.addEventListener("focus", refresh);
    return () => {
      alive = false;
      window.removeEventListener("focus", refresh);
    };
  }, []);

  if (view === null) {
    return loadError === null ? (
      <Spinner label={dict.common.loading} size={24} />
    ) : (
      <div className="ow-stack">
        <p className="ow-strong">{t.loadFailed}</p>
        <ErrorView error={loadError} written="none" />
        <div className="ow-row">
          <Button
            icon="refresh"
            onClick={() => {
              setLoadError(null);
              loadBackgroundSettings().catch((error: unknown) => {
                setLoadError(toAppError(error));
              });
            }}
          >
            {dict.common.actions.retry}
          </Button>
        </div>
      </div>
    );
  }

  const apply = (change: (intent: BackgroundSettingsIntent) => BackgroundSettingsIntent) => {
    if (saving || checking) return;
    setSaving(true);
    saveBackgroundSettings(change(backgroundIntent(view)))
      .then((applied) => {
        setShellError(applied.shell_error);
      })
      .catch((error: unknown) => {
        toast({ kind: "danger", title: dict.settings.saveFailed, details: toAppError(error).message });
      })
      .finally(() => {
        setSaving(false);
      });
  };

  const recheck = () => {
    if (saving || checking) return;
    setChecking(true);
    loadBackgroundSettings()
      .catch((error: unknown) => {
        toast({ kind: "danger", title: t.loadFailed, details: toAppError(error).message });
      })
      .finally(() => {
        setChecking(false);
      });
  };
  const startProblem = windowsStartProblem(view, t.windowsStart, info.sim);

  return (
    <div className="st-grid">
      {shellError === null ? null : (
        <div className="st-wide">
          <Callout kind="warning" title={t.shellErrorTitle} role="alert">
            <p>{t.shellErrorBody}</p>
            <ErrorView error={shellError} written="none" soft />
          </Callout>
        </div>
      )}
      <Card title={t.trayTitle} className="st-wide">
        <div className="ow-stack">
          <Switch
            checked={view.tray_enabled}
            disabled={saving}
            label={t.tray}
            description={t.trayHelp}
            onChange={(tray) => {
              // Without the icon OpenWraith doesn't start with Windows (the backend removes the entry too).
              apply((intent) => ({ ...intent, tray_enabled: tray, start_with_windows: tray && intent.start_with_windows }));
            }}
          />
          <Switch
            checked={view.tray_enabled && view.start_with_windows}
            disabled={saving || !view.tray_enabled}
            label={t.startWithWindows}
            description={view.tray_enabled ? t.startWithWindowsHelp : t.startNeedsTray}
            onChange={(start) => {
              apply((intent) => ({ ...intent, start_with_windows: start }));
            }}
          />
          {startProblem === null ? null : (
            <Callout
              kind="warning"
              title={startProblem.title}
              role="status"
              actions={
                <Button size="sm" icon="refresh" disabled={saving || checking} onClick={recheck}>
                  {t.windowsStart.recheck}
                </Button>
              }
            >
              <p>{startProblem.body}</p>
            </Callout>
          )}
          <p className="ow-muted ow-small">{t.startUninstall}</p>
        </div>
      </Card>
      <Card title={t.notificationsTitle}>
        <div className="ow-stack">
          <Switch
            checked={view.notifications.reset}
            disabled={saving}
            label={t.notifyReset}
            description={t.notifyResetHelp}
            onChange={(reset) => {
              apply((intent) => ({ ...intent, notifications: { ...intent.notifications, reset } }));
            }}
          />
          <Switch
            checked={view.notifications.auto_switch}
            disabled={saving}
            label={t.notifyAutoSwitch}
            description={t.notifyAutoSwitchHelp}
            onChange={(autoSwitch) => {
              apply((intent) => ({ ...intent, notifications: { ...intent.notifications, auto_switch: autoSwitch } }));
            }}
          />
          <p className="ow-muted ow-small">{t.notificationsNote}</p>
        </div>
      </Card>
      <Card title={t.autoBackupTitle}>
        <div className="ow-stack">
          <Switch
            checked={view.auto_backup.enabled}
            disabled={saving}
            label={t.autoBackup}
            description={view.auto_backup.enabled ? t.autoBackupHelp : `${t.autoBackupHelp} ${t.autoBackupOff}`}
            onChange={(enabled) => {
              apply((intent) => ({ ...intent, auto_backup: { ...intent.auto_backup, enabled } }));
            }}
          />
          <KeepField
            view={view}
            disabled={saving || !view.auto_backup.enabled}
            onSave={(keep) => {
              apply((intent) => ({ ...intent, auto_backup: { ...intent.auto_backup, keep } }));
            }}
          />
          <div className="ow-row">
            <Button
              size="sm"
              icon="backup"
              onClick={() => {
                navigate({ page: "backup", params: { section: "auto" } });
              }}
            >
              {t.showBackups}
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
