// Backup & Restore preview scenarios (owned by the backup-restore page
// agent). Each opens #/backup on a recorded fixture; `ui.backup.run` starts
// the flow the fixture recorded (open a file, compare, back up) so the
// review or the result can be screenshot after one or two clicks.
//
// The mock answers each command with the fixture's next recorded call,
// whatever the arguments: in `restore_preview` and
// `restore_firmware_mismatch` the first recorded `prepare_restore` is the
// refused one, so the switch confirmation (or the error) appears once
// before the plan.

import type { BackupDemo } from "../../pages/backup/demo";
import type { PreviewScenario } from "../types";

function backup(demo: BackupDemo): Record<string, unknown> {
  return { backup: demo };
}

/** The backup and restore page's preview scenarios. */
export const scenarios: PreviewScenario[] = [
  { id: "backup_page", fixture: "ready", route: "#/backup" },
  { id: "backup_not_connected", fixture: "searching", route: "#/backup" },
  { id: "backup_other_program", fixture: "other_program", route: "#/backup" },
  { id: "backup_active_ok", fixture: "backup_active_ok", route: "#/backup", ui: backup({ run: "backup_active" }) },
  { id: "backup_all_preview", fixture: "backup_all_preview", route: "#/backup", ui: backup({ run: "backup_all" }) },
  { id: "backup_all_ok", fixture: "backup_all_ok", route: "#/backup", ui: backup({ run: "backup_all" }) },
  { id: "backup_all_cancelled", fixture: "backup_all_cancelled", route: "#/backup", ui: backup({ run: "backup_all" }) },
  { id: "safety_list", fixture: "safety_list", route: "#/backup" },
  { id: "restore_summary_all", fixture: "restore_summary_all", route: "#/backup", ui: backup({ run: "open_file" }) },
  { id: "restore_preview", fixture: "restore_preview", route: "#/backup", ui: backup({ run: "compare" }) },
  { id: "restore_ok_persisted", fixture: "restore_ok_persisted", route: "#/backup", ui: backup({ run: "compare" }) },
  {
    id: "restore_lost",
    fixture: "restore_lost",
    route: "#/backup",
    ui: backup({ run: "compare", profiles: [2] }),
  },
  {
    id: "restore_maybe_lost_disconnect",
    fixture: "restore_maybe_lost_disconnect",
    route: "#/backup",
    ui: backup({ run: "compare", profiles: [1] }),
  },
  {
    id: "restore_cancelled_in_keymap_group",
    fixture: "restore_cancelled_in_keymap_group",
    route: "#/backup",
    ui: backup({ run: "compare", profiles: [1] }),
  },
  {
    id: "restore_firmware_mismatch",
    fixture: "restore_firmware_mismatch",
    route: "#/backup",
    ui: backup({ run: "open_file" }),
  },
  {
    id: "restore_needs_switch_consent",
    fixture: "restore_needs_switch_consent",
    route: "#/backup",
    ui: backup({ run: "compare" }),
  },
  {
    id: "restore_nothing_to_write",
    fixture: "restore_nothing_to_write",
    route: "#/backup",
    ui: backup({ run: "compare" }),
  },
  { id: "undo_preview", fixture: "undo_preview", route: "#/backup", ui: backup({ run: "compare" }) },
  { id: "restore_invalid_file", fixture: "restore_invalid_file", route: "#/backup", ui: backup({ run: "open_file" }) },
];
