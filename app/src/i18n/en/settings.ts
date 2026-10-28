// The Settings page: General, Background, Games, Profile names, About.

export const settings = {
  title: "Settings",
  /** The page's description under its title, per tab. */
  descriptions: {
    general: "Language, appearance, your presets and tips, and the safety backups folder.",
    background: "Keep OpenWraith running in the background, notifications and automatic backups.",
    games: "Switch the keyboard to a game's profile when the game comes to the front.",
    profiles: "Names for the four profiles, shown in the header and in the menus.",
    about: "Version, license, and what this version doesn't do.",
  },
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
  background: {
    trayTitle: "Notification area",
    tray: "Keep OpenWraith running in the notification area",
    trayHelp:
      "Closing the window then keeps OpenWraith running next to the clock, so game profiles and reset alerts keep working. To quit, choose Quit in the icon's menu.",
    startWithWindows: "Start OpenWraith with Windows",
    startWithWindowsHelp: "It starts hidden in the notification area.",
    startNeedsTray: "Turn on the notification-area icon first: OpenWraith starts with Windows only there.",
    startUninstall:
      "Turning the icon off also removes OpenWraith from the programs that start with Windows. Uninstalling OpenWraith doesn't remove that entry, so turn it off before you uninstall.",
    notificationsTitle: "Notifications",
    notifyReset: "Tell me when the keyboard looks reset or its firmware changed",
    notifyResetHelp: "Also when an automatic backup fails.",
    notifyAutoSwitch: "Tell me when a game switches the profile",
    notifyAutoSwitchHelp:
      "A Windows notification when a game switches the profile (at most one every 10 minutes); every switch also shows in OpenWraith while its window is open.",
    notificationsNote:
      "Windows may hold notifications back while Do not disturb is on. The banners in OpenWraith and the notification-area icon always show what needs your attention.",
    autoBackupTitle: "Automatic backups",
    autoBackup: "Back up each profile automatically",
    autoBackupHelp:
      "OpenWraith keeps a copy of each profile it reads and compares later reads with it, so it can tell you when the keyboard looks reset. It uses the reads it makes anyway: nothing extra is sent to the keyboard.",
    autoBackupOff: "While this is off, OpenWraith can't notice a reset either.",
    keep: "Automatic backups kept per profile",
    keepHelp: (p: { n: number }) =>
      `The newest ${p.n} of each profile are kept. Pinned automatic backups and safety backups are never deleted.`,
    keepInvalid: "Enter a whole number from 1 to 100.",
    keepSave: "Save",
    showBackups: "Show automatic backups",
    loadFailed: "The background settings couldn't be read.",
    shellErrorTitle: "Saved, but Windows didn't take the change",
    shellErrorBody:
      "OpenWraith couldn't update its notification-area icon or its start with Windows entry. Turn the switch off and on again to write the entry anew, or turn OpenWraith on in Task Manager > Startup apps.",
  },
  games: {
    toggle: "Switch profiles automatically for games",
    toggleHelp: "When a game you added here comes to the front, OpenWraith switches the keyboard to the game's profile.",
    needsTray:
      "Game profiles work only while OpenWraith is running. To keep them working after you close the window, turn on the notification-area icon.",
    openBackground: "Open Background",
    defaultLabel: "When a game closes",
    defaultStay: "Stay on the game's profile",
    defaultOption: (p: { label: string }) => `Go back to ${p.label}`,
    defaultHelp:
      "Only when OpenWraith switched to the game's profile and it is still active: a profile you chose yourself is never changed.",
    pause: "Pause",
    resume: "Resume",
    pauseHelp: "A pause lasts until you resume or restart OpenWraith.",
    statusLabel: "Now",
    line: {
      off: "Game profiles are off.",
      paused: (p: { reason: string }) => `Paused: ${p.reason}.`,
      noRules: "On, but no game has been added yet.",
      watching: "Watching for your games.",
      dwelling: (p: { label: string; profile: string }) =>
        `${p.label} is in front: switching to ${p.profile} in a moment.`,
      switched: (p: { label: string; profile: string }) => `${p.label} is in front, with ${p.profile}.`,
      overridden: (p: { label: string; profile: string }) =>
        `${p.label} is in front. You chose another profile than ${p.profile}, so OpenWraith leaves it until the game closes.`,
      skipped: (p: { label: string; profile: string }) =>
        `${p.label} is in front, but the switch to ${p.profile} was skipped. It is tried again the next time the game starts.`,
      waiting: (p: { label: string; profile: string; reason: string }) =>
        `${p.label}: the switch to ${p.profile} waits because ${p.reason}.`,
      waitingBack: (p: { profile: string; reason: string }) =>
        `The switch back to ${p.profile} waits because ${p.reason}.`,
    },
    /** Why the game profiles are paused; completes "Paused: …". */
    pauseReasons: {
      user: "you paused them",
      rate_limit: "more than 30 automatic switches in an hour",
      switch_failed: "a profile switch didn't work",
    },
    rules: {
      title: "Your games",
      caption: "Games and their profiles",
      empty: "No games yet. Add the games that should get their own profile.",
      game: "Game",
      program: "Program",
      profile: "Profile",
      on: "Switches automatically",
      remove: "Remove",
      anyFolder: "Any folder",
      ruleOn: (p: { label: string }) => `Switch for ${p.label}`,
      profileFor: (p: { label: string }) => `Profile for ${p.label}`,
      removeRule: (p: { label: string }) => `Remove ${p.label}`,
      removed: (p: { label: string }) => `${p.label} removed.`,
      generic: "Many programs share this name, so the rule switches for all of them.",
      genericShort: "Shared name",
      add: "Add a game…",
    },
    add: {
      title: "Add a game",
      program: "Program",
      none: "No program chosen yet.",
      running: "Running programs",
      runningShow: "Choose a running program",
      runningEmpty: "No program to choose: start the game, then refresh.",
      refresh: "Refresh",
      unavailable: "This build of OpenWraith can't see running programs. Browse for the program or type its file name.",
      browse: "Browse…",
      browseTitle: "Choose a game",
      browseFilter: "Programs",
      detect: "Detect the next program I open",
      detecting: "Switch to your game now: OpenWraith takes the next program that comes to the front.",
      detectLeft: (p: { time: string }) => `${p.time} left`,
      detectNone: "No program came to the front within 30 seconds. Try again, or choose the program another way.",
      detectStop: "Stop waiting",
      typeName: "Or type its file name",
      typeNameHint: "For example game.exe. The rule then matches that name in any folder.",
      typeNameInvalid: "Enter a file name without a folder: no \\, / or :.",
      useName: "Use this name",
      folder: (p: { folder: string }) => `In ${p.folder}`,
      folderUnknown:
        "Folder unknown (the program may run as administrator): the rule matches the name in any folder.",
      name: "Name",
      nameHint: "How the game is shown in OpenWraith.",
      profile: "Profile",
      confirm: "Add game",
      needProgram: "Choose a program first.",
      needName: "Enter a name.",
      failed: "The game couldn't be added.",
    },
    recent: {
      title: "Recent activity",
      empty: "Nothing yet.",
      aGame: "A game",
      switched: (p: { label: string; from: string; to: string }) => `${p.label}: switched from ${p.from} to ${p.to}.`,
      switchedTo: (p: { label: string; to: string }) => `${p.label}: switched to ${p.to}.`,
      switchedBack: (p: { to: string }) => `A game closed: back to ${p.to}.`,
      deferred: (p: { label: string; profile: string; reason: string }) =>
        `${p.label}: the switch to ${p.profile} waited because ${p.reason}.`,
      deferredBack: (p: { profile: string; reason: string }) =>
        `The switch back to ${p.profile} waited because ${p.reason}.`,
      skipped: (p: { label: string; profile: string; reason: string }) =>
        `${p.label}: the switch to ${p.profile} was skipped because ${p.reason}.`,
      skippedBack: (p: { profile: string; reason: string }) =>
        `The switch back to ${p.profile} was skipped because ${p.reason}.`,
      overridden: (p: { label: string; profile: string }) =>
        `${p.label}: you chose ${p.profile}, so OpenWraith leaves it until the game closes.`,
      overriddenNoGame: (p: { profile: string }) => `You chose ${p.profile}.`,
      paused: (p: { reason: string }) => `Paused: ${p.reason}.`,
      resumed: "Resumed.",
    },
    howItWorks: {
      title: "How it works",
      writes: "OpenWraith writes only the profile switch, the same as pressing Fn + 1-4. Your settings stay as they are.",
      waits:
        "It switches once a game has been in front for a moment, after the keyboard has saved your last change. It never switches while you have unwritten changes, a review is open or the key-depth view runs.",
      stops:
        "It stops when another program, such as the Wraith website, uses the keyboard. It starts again after you choose “I closed it, check again” in OpenWraith, at the earliest 2 minutes after that program was last heard. With the notification-area icon on, it also starts again by itself after 3 minutes without any keyboard traffic. Close every wraith.software tab while you play.",
      manual:
        "A profile you choose yourself (Fn + 1-4, the header or the notification-area icon) always wins until the game closes.",
      exit: "When a game closes, OpenWraith goes back to the profile chosen under “When a game closes”, but only when OpenWraith switched to the game's profile and it is still active.",
      limits: "After 30 automatic switches in an hour, or when a switch fails, game profiles pause until you resume them.",
      privacy: "OpenWraith only looks at which program is in front. Nothing leaves your PC.",
    },
    consent: {
      title: "Switch profiles automatically for games?",
      intro: "Before you turn this on:",
      tick: "I understand. I'll keep every wraith.software tab closed while games switch profiles; automatic switches don't ask about it each time.",
      confirm: "Turn on",
      needsTick: "Tick the box to turn game profiles on.",
    },
    saveFailed: "The game settings couldn't be saved.",
    loadFailed: "The game settings couldn't be read.",
  },
  profiles: {
    intro:
      "Give the four profiles names, such as “Work” or “FPS”. The names stay on this PC: the keyboard doesn't store them. They are shown in the header, in the notification-area menu and on Backup & profiles.",
    product: (p: { product: string }) => `Keyboard model ${p.product}`,
    connected: "Connected now",
    name: (p: { n: number }) => `Name of profile ${p.n}`,
    placeholder: "No name",
    hint: "Up to 32 characters. Leave a field empty for no name.",
    save: "Save names",
    saved: "The names were saved.",
    empty: "Connect your keyboard once; then you can name its profiles here.",
    saveFailed: "The names couldn't be saved.",
    loadFailed: "The profile names couldn't be read.",
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
      "Not in this version: the side light, polling rate, switch types and calibration, Win-Lock, the gamepad (analog) mode, and mouse and media steps in macros.",
    simulator: "This is a simulator build for development.",
  },
  saveFailed: "The setting couldn't be saved.",
} as const;
