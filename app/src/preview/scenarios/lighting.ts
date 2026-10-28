// Lighting preview scenarios (owned by the lighting page agent). Each uses a
// generated fixture; `ui.lighting.draft` presets the page's draft so that
// "Review and write…" sends the intent the fixture recorded (the mock answers
// with the recorded plan and report).

import type { ColorsPreviewUi } from "../../pages/lighting/colors/model";
import type { LightingPreviewUi } from "../../pages/lighting/model";
import type { PreviewScenario } from "../types";

const COLORS = "#/lighting?tab=colors";
const GREEN = "#00ff00";
const BLUE = "#0000ff";

function colors(map: Record<string, string>, switchToCustom = false): Record<string, unknown> {
  const state: ColorsPreviewUi = { colors: map, switchToCustom };
  return { lightingColors: state };
}

function draft(value: NonNullable<LightingPreviewUi["draft"]>): Record<string, unknown> {
  const ui: LightingPreviewUi = { draft: value };
  return { lighting: ui };
}

/** The lighting page's preview scenarios. */
export const scenarios: PreviewScenario[] = [
  { id: "light_ready", fixture: "light_ready", route: "#/lighting" },
  {
    id: "light_preview_mode_change",
    fixture: "light_preview_mode_change",
    route: "#/lighting",
    ui: draft({ mode: 0, color: "#0000ff" }),
  },
  { id: "light_apply_ok", fixture: "light_apply_ok", route: "#/lighting", ui: draft({ mode: 0, color: "#0000ff" }) },
  {
    id: "light_apply_brightness_only",
    fixture: "light_apply_brightness_only",
    route: "#/lighting",
    ui: draft({ brightness: 2 }),
  },
  { id: "light_unknown_mode", fixture: "light_unknown_mode", route: "#/lighting" },
  { id: "light_power_off", fixture: "light_power_off", route: "#/lighting" },
  {
    id: "light_apply_verify_failed",
    fixture: "light_apply_verify_failed",
    route: "#/lighting",
    ui: draft({ brightness: 1 }),
  },
  { id: "light_wave_draft", fixture: "light_ready", route: "#/lighting", ui: draft({ mode: 2, color: "#00ffff", speed: 1 }) },
  { id: "light_not_connected", fixture: "lost", route: "#/lighting" },
  { id: "light_other_program", fixture: "other_program", route: "#/lighting" },
  // Key colours: the draft is seeded (`lightingColors`) so "Review and write…" shows the recorded plan.
  { id: "colors_ready", fixture: "colors_ready", route: COLORS },
  { id: "colors_preview_not_shown", fixture: "colors_preview_not_shown", route: COLORS, ui: colors({ "46": GREEN, "68": GREEN }) },
  {
    id: "colors_preview_switch",
    fixture: "colors_preview_switch",
    route: COLORS,
    ui: colors({ "46": GREEN, "68": GREEN }, true),
  },
  {
    id: "colors_apply_ok",
    fixture: "colors_apply_ok",
    route: COLORS,
    ui: colors({ "46": BLUE, "68": BLUE, "69": BLUE, "70": BLUE }),
  },
  { id: "colors_not_connected", fixture: "searching", route: COLORS },
];
