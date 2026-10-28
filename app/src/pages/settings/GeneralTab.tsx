// Settings > General: language, theme, your trigger presets (managed on the
// Actuation tab), the one-time tips and the safety backups folder.

import { useEffect, useState } from "react";

import { Button } from "../../components/Button";
import { RadioGroup } from "../../components/Controls";
import { Callout } from "../../components/Feedback";
import { Card, TechnicalDetails } from "../../components/Layout";
import { useDict } from "../../i18n";
import type { LanguageSetting, ThemeSetting } from "../../ipc/bindings";
import { api } from "../../ipc/commands";
import { toAppError } from "../../ipc/errors";
import { navigate, tabTarget } from "../../routes";
import { revealSafetyDir } from "../../shell/actions";
import { useAppInfo } from "../../state/appInfo";
import { resetHints, useHints } from "../../state/hints";
import { setSettings, useSettings } from "../../state/settings";
import { toast } from "../../state/toasts";

/** How many presets of their own the user saved (`null` while unknown). */
function useUserPresetCount(): number | null {
  const [count, setCount] = useState<number | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .listPresets()
      .then((presets) => {
        if (alive) setCount(presets.user.length);
      })
      .catch(() => {
        // The card still says where presets are managed.
      });
    return () => {
      alive = false;
    };
  }, []);
  return count;
}

/** Trigger presets (B11 `presets`): where they are kept and managed. */
function PresetsCard() {
  const dict = useDict();
  const t = dict.settings.presets;
  const count = useUserPresetCount();
  return (
    <Card title={t.title}>
      <div className="ow-stack">
        <p className="ow-muted">{t.body}</p>
        {count === null ? null : <p>{t.count({ n: count })}</p>}
        <div className="ow-row">
          <Button
            size="sm"
            onClick={() => {
              navigate(tabTarget("actuation", "trigger"));
            }}
          >
            {t.open}
          </Button>
        </div>
      </div>
    </Card>
  );
}

/** One-time tips (B11 `hints_shown`): how many were dismissed; show them again. */
function TipsCard() {
  const dict = useDict();
  const t = dict.settings.tips;
  const hints = useHints();
  const [busy, setBusy] = useState(false);
  const count = hints.shown.length;
  return (
    <Card title={t.title}>
      <div className="ow-stack">
        <p className="ow-muted">{t.body}</p>
        {hints.loaded ? <p>{t.count({ n: count })}</p> : null}
        <div className="ow-row">
          <Button
            size="sm"
            disabled={busy || !hints.loaded || count === 0}
            onClick={() => {
              setBusy(true);
              resetHints()
                .then(() => {
                  toast({ kind: "success", title: t.done });
                })
                .catch((error: unknown) => {
                  toast({ kind: "danger", title: dict.settings.saveFailed, details: toAppError(error).message });
                })
                .finally(() => {
                  setBusy(false);
                });
            }}
          >
            {t.reset}
          </Button>
        </div>
      </div>
    </Card>
  );
}

/** Settings > General. */
export function GeneralTab() {
  const dict = useDict();
  const t = dict.settings;
  const settings = useSettings();
  const info = useAppInfo();
  return (
    <div className="st-grid">
      <Card title={t.language.title}>
        <RadioGroup<LanguageSetting>
          legend={t.language.title}
          hideLegend
          value={settings.language}
          options={[
            { value: "system", label: t.language.system },
            { value: "en", label: <span lang="en">{t.language.english}</span> },
            { value: "tr", label: <span lang="tr">{t.language.turkish}</span> },
          ]}
          onChange={(language) => {
            void setSettings({ ...settings, language });
          }}
        />
      </Card>
      <Card title={t.theme.title}>
        <RadioGroup<ThemeSetting>
          legend={t.theme.title}
          hideLegend
          value={settings.theme}
          options={[
            { value: "system", label: t.theme.system },
            { value: "light", label: t.theme.light },
            { value: "dark", label: t.theme.dark },
          ]}
          onChange={(theme) => {
            void setSettings({ ...settings, theme });
          }}
        />
      </Card>
      <PresetsCard />
      <TipsCard />
      <Card title={t.safety.title} className="st-wide">
        <p className="ow-muted">{t.safety.body}</p>
        {info.safety_dir.state === "ok" ? (
          <div className="st-folder">
            <code className="ow-break ow-selectable">{info.safety_dir.path}</code>
            <Button size="sm" icon="folder" onClick={revealSafetyDir}>
              {t.safety.show}
            </Button>
          </div>
        ) : (
          <Callout kind="danger" title={dict.shell.banners.safetyDirTitle}>
            <TechnicalDetails>{info.safety_dir.error.message}</TechnicalDetails>
          </Callout>
        )}
      </Card>
    </div>
  );
}
