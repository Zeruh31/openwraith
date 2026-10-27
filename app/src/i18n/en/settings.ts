// The Settings page.

export const settings = {
  title: "Settings",
  description: "Language, appearance and information about OpenWraith.",
  language: {
    title: "Language",
    system: "Same as Windows",
    english: "English",
    turkish: "Türkçe",
  },
  theme: {
    title: "Theme",
    system: "Same as Windows",
    light: "Light",
    dark: "Dark",
  },
  safety: {
    title: "Safety backups folder",
    body: "OpenWraith saves a safety backup here before every change. The command-line tool uses the same folder, and the environment variable OPENWRAITH_SAFETY_DIR changes it for both.",
    show: "Show",
  },
  about: {
    title: "About OpenWraith",
    version: "Version",
    license: "License",
    licenseValue: "GPL-3.0-or-later",
    repository: "Source code",
    copyright: "Copyright (C) 2026 OpenWraith contributors",
    warranty:
      "This program comes with ABSOLUTELY NO WARRANTY. It is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License, version 3 or later.",
    thirdParty:
      "OpenWraith includes open-source components by other authors, each under its own license. The license texts are in THIRD-PARTY-NOTICES.txt, next to LICENSE.txt in the installation folder.",
    showLicenses: "Show license files",
    privacy: "No account, no telemetry, no network: OpenWraith only talks to the keyboard.",
    cliUndo: "You can also undo a change with the command-line tool:",
    cliUndoCommand: "openwraith restore <safety backup file>",
    notIncluded: "Not in this version: key assignments, macros, per-key colours, side light, SOCD and polling rate.",
    simulator: "This is a simulator build for development.",
  },
  saveFailed: "The setting couldn't be saved.",
} as const;
