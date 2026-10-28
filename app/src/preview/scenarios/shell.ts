// Shell scenarios: connection states, banners, the profile switch, the close
// guard (shell-owned).

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
];
