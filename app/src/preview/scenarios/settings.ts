// Settings preview scenarios: the tabs (general, background, games, profile
// names, about) on recorded fixtures.
//
// The mock keeps the background settings, the profile names and the game
// rules in memory, so a change made in the preview stays (also through the
// Games tab's 2 s refresh). The `games_*` fixtures start with their rules
// saved; what happens on its own (a game in front, an automatic switch, a
// switch that waits) follows the recorded timeline.

import type { PreviewScenario } from "../types";

/** The settings page's preview scenarios. */
export const scenarios: PreviewScenario[] = [
  { id: "settings_general", fixture: "ready", route: "#/settings?tab=general" },
  // Background: the notification-area icon, start with Windows, notifications, automatic backups.
  { id: "settings_background", fixture: "settings_background", route: "#/settings?tab=background" },
  {
    id: "settings_background_shell_error",
    fixture: "settings_background_shell_error",
    route: "#/settings?tab=background",
  },
  // Start with Windows is on, but turned off in Task Manager: the lasting callout.
  {
    id: "settings_background_task_manager",
    fixture: "settings_background_task_manager",
    route: "#/settings?tab=background",
  },
  // The settings file could not be read: the notice, then the defaults.
  { id: "settings_invalid", fixture: "settings_invalid", route: "#/settings?tab=background" },
  { id: "settings_profile_names", fixture: "settings_background", route: "#/settings?tab=profiles" },
  { id: "settings_about", fixture: "ready", route: "#/settings?tab=about" },
  // Games: off, with running programs to add.
  { id: "games_settings", fixture: "games_settings", route: "#/settings?tab=games" },
  // A game in front: its profile after 1.5 s (the header's Auto chip).
  { id: "games_live", fixture: "games_live", route: "#/settings?tab=games" },
  // The game closed: back to the default profile.
  { id: "games_returned_to_default", fixture: "games_returned_to_default", route: "#/settings?tab=games" },
  // Unwritten changes: the switch waits, then is skipped after 30 s.
  { id: "games_deferred_drafts", fixture: "games_deferred_drafts", route: "#/settings?tab=games" },
  // Another program used the keyboard: the switch waits.
  { id: "games_deferred_other_program", fixture: "games_deferred_other_program", route: "#/settings?tab=games" },
  // Paused by the user for this run.
  { id: "games_paused", fixture: "games_paused", route: "#/settings?tab=games" },
  // The keyboard ignored a switch: paused by the app; Resume switches.
  { id: "games_paused_failed", fixture: "games_paused_failed", route: "#/settings?tab=games" },
  // An OpenWraith command holds the keyboard: the game's switch waits.
  { id: "games_paused_for_cli", fixture: "paused_for_cli", route: "#/settings?tab=games" },
];
