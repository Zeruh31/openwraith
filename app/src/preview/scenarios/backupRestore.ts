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
  // Share a profile.
  { id: "export_done", fixture: "export_done", route: "#/backup", ui: backup({ share: "export" }) },
  {
    id: "export_needs_switch",
    fixture: "export_needs_switch",
    route: "#/backup",
    ui: backup({ share: "export", exportProfile: 3 }),
  },
  { id: "import_summary", fixture: "import_summary", route: "#/backup", ui: backup({ share: "open_import" }) },
  { id: "import_preview", fixture: "import_preview", route: "#/backup", ui: backup({ share: "import", into: 2 }) },
  {
    id: "import_refused_critical",
    fixture: "import_refused_critical",
    route: "#/backup",
    ui: backup({ share: "import", into: 2, keys: true }),
  },
  {
    id: "import_keys_blocked_other",
    fixture: "import_keys_blocked_other",
    route: "#/backup",
    ui: backup({ share: "import", into: 2, keys: true }),
  },
  { id: "import_apply_ok", fixture: "import_apply_ok", route: "#/backup", ui: backup({ share: "import", into: 2, keys: true }) },
  // Export options: the parts to include (dead band and SOCD off), and other sources.
  { id: "export_options", fixture: "export_options", route: "#/backup", ui: backup({ share: "export" }) },
  {
    id: "export_from_auto_backup",
    fixture: "export_from_auto_backup",
    route: "#/backup",
    ui: backup({ share: "export", exportFrom: "automatic" }),
  },
  { id: "export_from_file", fixture: "export_options", route: "#/backup", ui: backup({ share: "export", exportFrom: "file" }) },
  {
    id: "export_from_backup_file",
    fixture: "export_from_backup_file",
    route: "#/backup",
    ui: backup({ share: "export", exportFrom: "file" }),
  },
  // Automatic backups: the list, Keep (pin), Restore… into the restore wizard.
  { id: "auto_backups_list", fixture: "auto_backups_list", route: "#/backup" },
  { id: "auto_backups_off", fixture: "auto_backups_off", route: "#/backup?section=auto" },
  { id: "auto_backups_no_keyboard", fixture: "auto_backups_no_keyboard", route: "#/backup?section=auto" },
  // The Recovery card (`?recover=1`; it also opens on its own while a reset is suspected).
  { id: "recovery_nothing_wrong", fixture: "recovery_nothing_wrong", route: "#/backup?recover=1" },
  // The keyboard reported a reset: "Reconnect and compare", then profile 1 looks reset.
  { id: "recovery_reset_seen", fixture: "recovery_reset_seen", route: "#/backup?recover=1" },
  { id: "recovery_reset_suspected", fixture: "recovery_reset_suspected", route: "#/backup?recover=1" },
  { id: "recovery_keep_current", fixture: "recovery_keep_current", route: "#/backup?recover=1" },
  {
    id: "recovery_restore_ok",
    fixture: "recovery_restore_ok",
    route: "#/backup?recover=1",
    ui: backup({ recovery: "restore" }),
  },
  {
    id: "recovery_restore_all_preview",
    fixture: "recovery_restore_all_preview",
    route: "#/backup?recover=1",
    ui: backup({ recovery: "restore_all" }),
  },
  { id: "recovery_firmware_changed", fixture: "recovery_firmware_changed", route: "#/backup?recover=1" },
];
