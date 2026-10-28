// Shell scenarios: connection states, banners, the profile switch, the close
// guard, the background agent's chips and banners (shell-owned).

import type { PreviewScenario } from "../types";

/** The shell's preview scenarios. */
export const scenarios: PreviewScenario[] = [
  { id: "ready", fixture: "ready", route: "#/device" },
  { id: "ready_sim", fixture: "ready_sim", route: "#/device" },
  { id: "searching", fixture: "searching", route: "#/device" },
  { id: "several_keyboards", fixture: "several_keyboards", route: "#/device" },
  { id: "observing", fixture: "observing", route: "#/device" },
  { id: "other_program", fixture: "other_program", route: "#/device" },
  { id: "reading", fixture: "reading", route: "#/device" },
  { id: "untested_firmware", fixture: "untested_firmware", route: "#/device" },
  { id: "reset_seen", fixture: "reset_seen", route: "#/device" },
  { id: "unsupported", fixture: "unsupported", route: "#/device" },
  { id: "open_failed", fixture: "open_failed", route: "#/device" },
  { id: "lost", fixture: "lost", route: "#/device" },
  { id: "released", fixture: "released", route: "#/device", startAtEnd: true },
  { id: "fault", fixture: "fault", route: "#/device" },
  { id: "interrupted", fixture: "interrupted", route: "#/device" },
  { id: "profile_switch", fixture: "profile_switch", route: "#/device" },
  { id: "profile_switch_refused", fixture: "profile_switch_refused", route: "#/device" },
  { id: "close_requested", fixture: "close_requested", route: "#/device" },
  { id: "settings", fixture: "ready", route: "#/settings" },
  // A key-depth test left open by an earlier run: the banner and its "Stop it".
  { id: "stray_travel_test", fixture: "depth_stray_marker", route: "#/device" },
  // Settings look reset: the Recovery banner (it leads to #/backup?recover=1).
  { id: "recovery_banner", fixture: "recovery_keep_current", route: "#/device" },
  { id: "firmware_changed_banner", fixture: "recovery_firmware_changed", route: "#/device" },
  // An OpenWraith command holds the keyboard: the banner, writes blocked.
  { id: "paused_for_cli", fixture: "paused_for_cli", route: "#/device" },
  // A game rule holds the profile: the header's Auto chip.
  { id: "auto_chip", fixture: "games_live", route: "#/device" },
  // The game profiles are paused: by the user (header chip), by the app (banner).
  { id: "auto_paused", fixture: "games_paused", route: "#/device" },
  { id: "auto_paused_failed", fixture: "games_paused_failed", route: "#/device" },
  // The settings file could not be read: the notice stays until dismissed.
  { id: "settings_file_invalid", fixture: "settings_invalid", route: "#/device" },
];
