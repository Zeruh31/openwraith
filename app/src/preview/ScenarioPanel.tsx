// The browser preview's scenario picker (mock mode only; never in the app).

import { useState } from "react";

import { Button } from "../components/Button";
import { useDict } from "../i18n";
import type { PreviewParams } from "./installMocks";
import { SCENARIO_GROUPS } from "./registry";
import "./preview.css";

/** Props of {@link ScenarioPanel}. */
export interface ScenarioPanelProps {
  params: PreviewParams;
  current: string;
}

/** A small floating panel to load another scenario, language, theme or speed. */
export function ScenarioPanel({ params, current }: ScenarioPanelProps) {
  const dict = useDict();
  const t = dict.shell.preview;
  const [open, setOpen] = useState(false);
  const [scenario, setScenario] = useState(current);
  const [lang, setLang] = useState(params.lang ?? "system");
  const [theme, setTheme] = useState(params.theme ?? "system");
  const [speed, setSpeed] = useState(String(params.speed));

  const load = () => {
    const query = new URLSearchParams({ scenario, lang, theme, speed });
    if (params.freeze) query.set("freeze", "1");
    if (!params.fallback) query.set("fallback", "0");
    window.location.assign(`${window.location.pathname}?${query.toString()}`);
  };

  if (!open) {
    return (
      <button
        type="button"
        className="ow-preview-panel__toggle"
        onClick={() => {
          setOpen(true);
        }}
      >
        {t.show}
      </button>
    );
  }
  return (
    <aside className="ow-preview-panel" aria-label={t.title}>
      <label>
        <span>{t.scenario}</span>
        <select
          value={scenario}
          onChange={(event) => {
            setScenario(event.currentTarget.value);
          }}
        >
          {SCENARIO_GROUPS.map((group) => (
            <optgroup key={group.page} label={group.page}>
              {group.scenarios.map((item) => (
                <option key={`${group.page}-${item.id}`} value={item.id}>
                  {item.id}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
      </label>
      <label>
        <span>{t.language}</span>
        <select
          value={lang}
          onChange={(event) => {
            setLang(event.currentTarget.value as typeof lang);
          }}
        >
          <option value="system">{dict.settings.language.system}</option>
          <option value="en">{dict.settings.language.english}</option>
          <option value="tr">{dict.settings.language.turkish}</option>
        </select>
      </label>
      <label>
        <span>{t.theme}</span>
        <select
          value={theme}
          onChange={(event) => {
            setTheme(event.currentTarget.value as typeof theme);
          }}
        >
          <option value="system">{dict.settings.theme.system}</option>
          <option value="light">{dict.settings.theme.light}</option>
          <option value="dark">{dict.settings.theme.dark}</option>
        </select>
      </label>
      <label>
        <span>{t.speed}</span>
        <select
          value={speed}
          onChange={(event) => {
            setSpeed(event.currentTarget.value);
          }}
        >
          <option value="0">{t.instant}</option>
          <option value="1">{"1×"}</option>
          <option value="4">{"4×"}</option>
        </select>
      </label>
      <div className="ow-row">
        <Button size="sm" variant="primary" onClick={load}>
          {t.apply}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => {
            setOpen(false);
          }}
        >
          {t.hide}
        </Button>
      </div>
    </aside>
  );
}
