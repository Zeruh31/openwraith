// Actuation & Rapid Trigger preview scenarios (owned by the actuation page
// agent). Each recorded fixture opens the page; the `ui` state preselects
// keys and a draft (`previewUi("actuation")`) so the page and the recorded
// prepare/apply flows can be seen without clicking. With `open`, the review
// opens by itself; the mock answers with the fixture's recorded calls.

import type { ActuationPreviewUi } from "../../pages/actuation/preview";
import type { PreviewScenario } from "../types";

const WASD = [46, 68, 69, 70];
const W = 46;
const Q = 45;
const E = 47;

function ui(state: ActuationPreviewUi): Record<string, unknown> {
  return { actuation: state };
}

/** The actuation page's preview scenarios. */
export const scenarios: PreviewScenario[] = [
  // Page states.
  { id: "trigger_ready_mixed", fixture: "trigger_ready_mixed", route: "#/actuation" },
  {
    id: "act_mixed_selection",
    fixture: "trigger_ready_mixed",
    route: "#/actuation",
    ui: ui({ keys: [Q, E, ...WASD] }),
  },
  {
    id: "act_wasd_draft",
    fixture: "trigger_preview",
    route: "#/actuation",
    ui: ui({ keys: WASD, actuation: 60, rt: { mode: "same", sensitivity: 15 } }),
  },
  {
    id: "act_separate_rt",
    fixture: "trigger_ready_mixed",
    route: "#/actuation",
    ui: ui({ keys: [Q, W], rt: { mode: "separate", press: 10, release: 20 }, show: "rapid_trigger" }),
  },
  { id: "trigger_unit_mismatch", fixture: "trigger_unit_mismatch", route: "#/actuation", ui: ui({ keys: [W] }) },
  // Recorded flows: the review opens with the recorded plan.
  {
    id: "trigger_preview",
    fixture: "trigger_preview",
    route: "#/actuation",
    ui: ui({ keys: WASD, actuation: 60, rt: { mode: "same", sensitivity: 15 }, open: true }),
  },
  {
    id: "trigger_apply_ok",
    fixture: "trigger_apply_ok",
    route: "#/actuation",
    ui: ui({ keys: WASD, actuation: 60, rt: { mode: "same", sensitivity: 15 }, open: true }),
  },
  {
    id: "trigger_apply_maybe_lost",
    fixture: "trigger_apply_maybe_lost",
    route: "#/actuation",
    ui: ui({ keys: [W, 68], actuation: 60, open: true }),
  },
  {
    id: "trigger_nothing_to_do",
    fixture: "trigger_nothing_to_do",
    route: "#/actuation",
    ui: ui({ keys: [W], actuation: 50, rt: { mode: "same", sensitivity: 15 }, open: true }),
  },
  {
    id: "trigger_unit_mismatch_review",
    fixture: "trigger_unit_mismatch",
    route: "#/actuation",
    ui: ui({ keys: [W], actuation: 60, open: true }),
  },
  {
    id: "trigger_stale_plan",
    fixture: "trigger_stale_plan",
    route: "#/actuation",
    ui: ui({ keys: [W], actuation: 60, rt: { mode: "off" }, open: true }),
  },
  {
    id: "trigger_refused_other_program",
    fixture: "trigger_refused_other_program",
    route: "#/actuation",
    ui: ui({ keys: [W], actuation: 60, open: true }),
  },
];
