// Keys preview scenarios (Assignments, Macros, Advanced keys). The recorded
// keyboard is the core's seeded one (`keys_seed`: remapped Caps, a DKS on E,
// a Mod-Tap on G, a broken reference on J, an orphan macro, a kept-as-read
// advanced slot). With `open`, the Assignments tab opens the review of its
// seeded draft by itself; the mock answers with the fixture's recorded plan
// and report whatever the draft, so the macro-slot-0 flows can be seen there
// too. Macro and advanced-key flows are otherwise clicked through on their
// own tabs.

import type { KeysAssignPreviewUi } from "../../pages/keys/assign/preview";
import type { PreviewScenario } from "../types";

function assign(state: KeysAssignPreviewUi): Record<string, unknown> {
  return { keysAssign: state };
}

const ASSIGN = "#/keys?tab=assign";
const MACROS = "#/keys?tab=macros";
const ADVANCED = "#/keys?tab=advanced";

/** The draft of `keys_preview_assign` / `keys_apply_ok` (R → Ctrl+C, Fn F2 → back, swap Caps and `). */
const RECORDED_ASSIGN: KeysAssignPreviewUi = {
  targets: [
    { layer: "base", key: 48, action: { kind: "combo", modifiers: [224], key: 6 } },
    { layer: "fn", key: 3, action: { kind: "mouse", action: "back" } },
    { layer: "base", key: 66, action: { kind: "media", index: 18 } },
    { layer: "base", key: 22, action: { kind: "key", usage: 224 } },
  ],
  swaps: [{ layer: "base", first: 66, second: 22 }],
};

/** A draft that opens a review; the plan shown is the fixture's own. */
const ESC_ON_BACKQUOTE: KeysAssignPreviewUi = {
  targets: [{ layer: "base", key: 22, action: { kind: "key", usage: 41 } }],
  open: true,
};

/** The Keys page's preview scenarios. */
export const scenarios: PreviewScenario[] = [
  { id: "keys_assign", fixture: "keys_ready", route: ASSIGN },
  { id: "keys_assign_builtin", fixture: "keys_ready", route: ASSIGN, ui: assign({ layer: "fn", key: 3 }) },
  { id: "keys_assign_critical", fixture: "keys_ready", route: ASSIGN, ui: assign({ layer: "fn", key: 24 }) },
  { id: "keys_assign_draft", fixture: "keys_ready", route: ASSIGN, ui: assign(RECORDED_ASSIGN) },
  { id: "keys_preview_assign", fixture: "keys_preview_assign", route: ASSIGN, ui: assign({ ...RECORDED_ASSIGN, open: true }) },
  { id: "keys_apply_ok", fixture: "keys_apply_ok", route: ASSIGN, ui: assign({ ...RECORDED_ASSIGN, open: true }) },
  {
    id: "keys_refused_critical",
    fixture: "keys_refused_critical",
    route: ASSIGN,
    ui: assign({ layer: "fn", targets: [{ layer: "fn", key: 24, action: { kind: "key", usage: 4 } }], open: true }),
  },
  { id: "keys_preview_slot0", fixture: "keys_preview_slot0", route: ASSIGN, ui: assign(ESC_ON_BACKQUOTE) },
  {
    id: "keys_apply_cancelled_after_slot0",
    fixture: "keys_apply_cancelled_after_slot0",
    route: ASSIGN,
    ui: assign(ESC_ON_BACKQUOTE),
  },
  { id: "keys_apply_failed_cleared", fixture: "keys_apply_failed_cleared", route: ASSIGN, ui: assign(ESC_ON_BACKQUOTE) },
  { id: "keys_apply_checked", fixture: "keys_apply_checked", route: ASSIGN, ui: assign(ESC_ON_BACKQUOTE) },
  { id: "keys_macros", fixture: "keys_ready", route: MACROS },
  { id: "keys_macros_failed_cleared", fixture: "keys_apply_failed_cleared", route: MACROS },
  { id: "keys_macros_checked", fixture: "keys_apply_checked", route: MACROS },
  { id: "keys_library_invalid", fixture: "keys_library_invalid", route: MACROS },
  { id: "keys_advanced", fixture: "keys_ready", route: ADVANCED },
  { id: "keys_preview_advanced", fixture: "keys_preview_advanced", route: ADVANCED },
  { id: "keys_apply_advanced", fixture: "keys_apply_advanced", route: ADVANCED },
  { id: "keys_preview_advanced_edit", fixture: "keys_preview_advanced_edit", route: ADVANCED },
  { id: "keys_not_connected", fixture: "searching", route: ASSIGN },
  { id: "keys_other_program", fixture: "other_program", route: ASSIGN },
];
