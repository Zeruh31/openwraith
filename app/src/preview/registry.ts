// Every preview scenario: the page-composed ones (`scenarios/*.ts`) and one
// per recorded fixture.

import type { PageId } from "../routes";
import { fixtureIds, fixtures } from "./fixtures";
import { scenarios as actuation } from "./scenarios/actuation";
import { scenarios as backupRestore } from "./scenarios/backupRestore";
import { scenarios as lighting } from "./scenarios/lighting";
import { scenarios as shell } from "./scenarios/shell";
import type { Fixture, PreviewScenario } from "./types";

/** A group of scenarios in the picker. */
export interface ScenarioGroup {
  page: PageId | "fixtures";
  scenarios: readonly PreviewScenario[];
}

/** The route a bare fixture opens on, from its id. */
export function routeForFixture(id: string): string {
  if (id.startsWith("trigger_")) return "#/actuation";
  if (id.startsWith("light_")) return "#/lighting";
  if (id.startsWith("backup_") || id.startsWith("restore_") || id === "safety_list" || id === "undo_preview") {
    return "#/backup";
  }
  return "#/device";
}

/** The scenario groups shown in the picker. */
export const SCENARIO_GROUPS: readonly ScenarioGroup[] = [
  { page: "device", scenarios: shell },
  { page: "actuation", scenarios: actuation },
  { page: "lighting", scenarios: lighting },
  { page: "backup", scenarios: backupRestore },
  {
    page: "fixtures",
    scenarios: fixtureIds.map((id) => ({ id: `fixture:${id}`, fixture: id, route: routeForFixture(id) })),
  },
];

/** A scenario by id (`ready`, `fixture:ready` or a bare fixture id). */
export function findScenario(id: string): PreviewScenario | undefined {
  for (const group of SCENARIO_GROUPS) {
    const found = group.scenarios.find((scenario) => scenario.id === id);
    if (found !== undefined) return found;
  }
  if (id in fixtures) {
    return { id, fixture: id, route: routeForFixture(id) };
  }
  return undefined;
}

/** A fixture by id. */
export function fixtureById(id: string): Fixture | undefined {
  return fixtures[id];
}

/** The fixture library (for fallbacks). */
export const FIXTURES: Readonly<Record<string, Fixture>> = fixtures;
