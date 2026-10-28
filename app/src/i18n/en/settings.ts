// The Settings page.

export const settings = {
  title: "Settings",
  description: "Language, appearance, your presets and tips, and information about OpenWraith.",
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
  presets: {
    title: "Trigger presets",
    body: "Your own presets are kept on this PC, in OpenWraith's settings. Save, apply or delete them with Presets… on the Actuation & Rapid Trigger tab.",
    count: (p: { n: number }) =>
      p.n === 0 ? "You haven't saved a preset yet." : p.n === 1 ? "You saved 1 preset." : `You saved ${p.n} presets.`,
    open: "Open Actuation & Rapid Trigger",
  },
  tips: {
    title: "Tips",
    body: "OpenWraith shows a few tips once, for example on the Keys page, until you choose Got it.",
    count: (p: { n: number }) =>
      p.n === 0 ? "No tip has been dismissed." : p.n === 1 ? "1 tip dismissed." : `${p.n} tips dismissed.`,
    reset: "Show tips again",
    done: "The tips will be shown again.",
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
    notIncluded:
      "Not in this version: the side light, polling rate, switch types and calibration, mouse and media steps in macros, the notification-area icon, starting with Windows, automatic backups, reset alerts and per-game profiles.",
    simulator: "This is a simulator build for development.",
  },
  saveFailed: "The setting couldn't be saved.",
} as const;
