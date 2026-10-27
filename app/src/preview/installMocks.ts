// The browser preview (`npm run dev:preview`, `vite --mode mock`): replaces
// the Tauri IPC with the recorded fixtures before the app starts.
//
// URL: http://localhost:1421/?scenario=<id>&lang=en|tr&theme=light|dark
//      &speed=0|1|4&freeze=1&fallback=0#/<route>
// - scenario: a scenario id from `scenarios/*.ts` or a fixture id;
// - lang, theme: override the fixture's settings;
// - speed: time factor for recorded progress (0 = at once);
// - freeze=1: no animations and no event replay (stable screenshots);
// - fallback=0: unrecorded commands fail instead of borrowing other fixtures.

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { createElement, type ReactNode } from "react";

import type { LanguageSetting, ThemeSetting } from "../ipc/bindings";
import { createMockBackend } from "./mockBackend";
import { findScenario, fixtureById, FIXTURES } from "./registry";
import { ScenarioPanel } from "./ScenarioPanel";
import type { Fixture } from "./types";
import { setPreviewUi } from "./ui";

/** The preview's URL parameters. */
export interface PreviewParams {
  scenario: string;
  lang: LanguageSetting | null;
  theme: ThemeSetting | null;
  speed: number;
  freeze: boolean;
  fallback: boolean;
}

/** Reads the preview parameters from a query string. */
export function readPreviewParams(search: string): PreviewParams {
  const query = new URLSearchParams(search);
  const lang = query.get("lang");
  const theme = query.get("theme");
  const speed = Number(query.get("speed") ?? "1");
  return {
    scenario: query.get("scenario") ?? "ready",
    lang: lang === "en" || lang === "tr" || lang === "system" ? lang : null,
    theme: theme === "light" || theme === "dark" || theme === "system" ? theme : null,
    speed: Number.isFinite(speed) && speed >= 0 ? speed : 1,
    freeze: query.get("freeze") === "1",
    fallback: query.get("fallback") !== "0",
  };
}

/** The fixture's final recorded status (for scenarios that start at the end). */
function finalStatus(fixture: Fixture): Fixture["initial_status"] {
  let status = fixture.initial_status;
  for (const item of fixture.events) {
    if (item.event.event === "status") status = item.event.status;
  }
  return status;
}

/** Installs the mocked IPC; returns the scenario panel to render. */
export function installMocks(): ReactNode {
  const params = readPreviewParams(window.location.search);
  const scenario = findScenario(params.scenario) ?? findScenario("ready");
  const source = scenario === undefined ? undefined : fixtureById(scenario.fixture);
  if (scenario === undefined || source === undefined) {
    return null;
  }
  const fixture = structuredClone(source);
  if (params.lang !== null) fixture.settings.language = params.lang;
  if (params.theme !== null) fixture.settings.theme = params.theme;
  if (scenario.startAtEnd === true) {
    fixture.initial_status = finalStatus(fixture);
    fixture.calls = [];
    fixture.events = [];
  }
  if (window.location.hash === "") {
    const restore = fixture.ui?.["restore"];
    const route =
      typeof restore === "string" && scenario.route === "#/backup" ? `${scenario.route}?restore=${restore}` : scenario.route;
    window.location.hash = route;
  }
  setPreviewUi(scenario.ui);
  if (params.freeze) {
    document.documentElement.classList.add("ow-freeze");
  }
  mockWindows("main");
  const backend = createMockBackend(fixture, {
    speed: params.speed,
    replayEvents: !params.freeze,
    fallback: params.fallback,
    library: FIXTURES,
  });
  mockIPC((command, args) => backend.handle(command, args));
  return createElement(ScenarioPanel, { params, current: scenario.id });
}
