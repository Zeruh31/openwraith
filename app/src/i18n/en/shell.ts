// The app frame: navigation, header, status pill, banners, close guard,
// notices, the simulator panel and the browser-preview panel.

/** "1 status message", "2 status messages". */
function counted(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** What another program left on the keyboard, without zero counts ("a, b and c"). */
function evidenceText(p: { status: number; acks: number; profiles: number }): string {
  const parts = [
    p.status > 0 ? counted(p.status, "status message", "status messages") : null,
    p.acks > 0 ? counted(p.acks, "reply to another program's command", "replies to another program's commands") : null,
    p.profiles > 0 ? counted(p.profiles, "profile report", "profile reports") : null,
  ].filter((part): part is string => part !== null);
  if (parts.length === 0) return "messages";
  const last = parts.pop() ?? "";
  return parts.length === 0 ? last : `${parts.join(", ")} and ${last}`;
}

export const shell = {
  boot: {
    loading: "Starting OpenWraith…",
    failedTitle: "OpenWraith can't reach its background service",
    failedBody:
      "The window opened, but the part of OpenWraith that talks to the keyboard didn't answer. Nothing was written. Restart the app; if this keeps happening, report it with the technical details.",
  },
  skipToContent: "Skip to content",
  lastReport: {
    title: "Result of the last operation",
    intro: "This operation finished while its page wasn't open. Here is what happened.",
  },
  nav: {
    label: "Pages",
    device: "Keyboard",
    keys: "Keys",
    actuation: "Actuation",
    lighting: "Lighting",
    backup: "Backup & profiles",
    settings: "Settings",
    unwritten: "Unwritten changes",
  },
  /** The tabs of the pages that have tabs (`?tab=`). */
  tabs: {
    label: (p: { page: string }) => `Sections of ${p.page}`,
    unwritten: "Unwritten changes on this tab",
    pages: {
      keys: {
        assign: "Assignments",
        macros: "Macros",
        advanced: "Advanced keys",
      },
      actuation: {
        trigger: "Actuation & Rapid Trigger",
        deadband: "Dead band",
        socd: "SOCD",
        depth: "Key depth",
      },
      lighting: {
        effect: "Effect",
        colors: "Key colours",
      },
      settings: {
        general: "General",
        background: "Background",
        games: "Games",
        profiles: "Profile names",
        about: "About",
      },
    },
  },
  /** Sections of pages without tabs. */
  sections: {
    share: "Share a profile",
  },
  /** A tab or section that a later part of this version fills in. */
  placeholder: {
    title: "Not built yet",
    body: "This part of OpenWraith is still being built. It doesn't read or write anything on the keyboard yet.",
  },
  /** Unwritten changes, as the shell lists them. */
  drafts: {
    where: (p: { page: string; tab: string }) => `${p.page} > ${p.tab}`,
    entry: (p: { where: string; count: number }) =>
      p.count === 1 ? `${p.where}: 1 unwritten change` : `${p.where}: ${p.count} unwritten changes`,
    entryUncounted: (p: { where: string }) => `${p.where}: unwritten changes`,
  },
  header: {
    profileLabel: "Active profile",
    profileOption: (p: { n: number }) => `Profile ${p.n}`,
    switcherBlocked: {
      busy: "Busy",
      notReady: "Not connected",
      blocked: "Writing is paused",
      pausedForCli: "An OpenWraith command is running",
    },
    /** A profile in the switcher, for screen readers ("Profile 2, FPS"). */
    profileOptionNamed: (p: { n: number; name: string }) => `Profile ${p.n}, ${p.name}`,
    auto: "Auto",
    autoTitle: (p: { label: string; profile: string }) =>
      `${p.label} switched the keyboard to ${p.profile}. Choosing a profile yourself keeps your choice until the game closes. Open Settings > Games.`,
    autoPaused: "Auto paused",
    autoPausedTitle: "Automatic game profiles are paused. Open Settings > Games to resume them.",
    statusButton: "Keyboard connection details",
    live: "Live",
    liveTitle: "The key-depth view is running. Open Actuation > Key depth to stop it.",
  },
  status: {
    starting: "Starting…",
    searching: "Looking for the keyboard…",
    severalKeyboards: "Several keyboards connected",
    opening: "Opening the keyboard…",
    openFailed: (p: { seconds: string }) => `Couldn't open the keyboard · trying again in ${p.seconds}`,
    observing: "Checking that no other program is using the keyboard…",
    otherProgram: "Another program is using the keyboard",
    reading: (p: { done: number; total: number }) => `Reading the keyboard… ${p.done}/${p.total}`,
    readFailed: "Couldn't read the keyboard",
    ready: (p: { model: string; firmware: string }) => `${p.model} · FW ${p.firmware}`,
    readyNoFirmware: (p: { model: string }) => p.model,
    lost: "Keyboard disconnected",
    resetSeen: "The keyboard reset itself",
    unsupported: "Unsupported keyboard",
    released: "Keyboard released",
    fault: "Internal error: restart the app",
    simulatorBadge: "Simulator",
  },
  popover: {
    title: "Keyboard",
    connection: "Connection",
    model: "Model",
    product: "Product code",
    firmware: "Firmware",
    usb: "USB ID",
    activeProfile: "Active profile",
    readAgain: "Read again",
    chooseKeyboard: "Choose keyboard…",
    release: "Release keyboard",
    releaseHint: "Closes OpenWraith's connection so other programs have the keyboard to themselves.",
    connect: "Connect",
  },
  busy: {
    working: "Working…",
  },
  banners: {
    simulator:
      "Simulator: no keyboard is used. Everything you do here goes to a simulated W75.",
    faultTitle: "OpenWraith stopped using the keyboard",
    faultBody:
      "An internal error happened. Writing is turned off until you restart the app. Nothing more will be sent to the keyboard.",
    interruptedTitle: "An earlier change was interrupted",
    interruptedBody: (p: { what: string; when: string; profiles: string }) =>
      `${p.what} started on ${p.when} (profiles ${p.profiles}) didn't finish: OpenWraith was closed or crashed while writing. The keyboard may hold part of the change. The safety backup made before it can undo it.`,
    interruptedNoBackup: "No safety backup was recorded for it.",
    resetTitle: "The keyboard reset itself",
    resetBody:
      "The keyboard reported a factory reset (Fn + Space). OpenWraith stopped and sent nothing. Read the keyboard again before changing anything; a backup can bring your settings back.",
    resetReadAgain: "Read the keyboard again",
    resetRestore: "Restore a backup…",
    otherProgramTitle: "Another program is using the keyboard",
    otherProgramBody:
      "This is probably the wraith.software site. Close that browser tab completely; switching to another tab isn't enough. Writing is paused until then.",
    otherProgramCheck: "I closed it, check again",
    otherProgramDetails: "What OpenWraith saw",
    evidence: (p: { status: number; acks: number; profiles: number }) =>
      `OpenWraith received ${evidenceText(p)} that it didn't ask for.`,
    safetyDirTitle: "The safety-backup folder can't be used",
    safetyDirBody:
      "OpenWraith saves a safety backup before every change and can't do that now, so writing is turned off. Reading still works.",
    staleTitle: "The values shown may be out of date",
    staleBody: "Something changed on the keyboard since it was read. Read it again before changing anything.",
    strayTravelTitle: "The key-depth view may still be on in the keyboard",
    strayTravelBody:
      "An earlier key-depth view may not have been stopped (OpenWraith closed, or the keyboard was disconnected). Typing keeps working; stop it on the Key depth tab.",
    strayTravelOpen: "Go to Key depth",
    recoveryTitle: "Your keyboard's settings may have been reset",
    recoveryProfiles: (p: { list: string; count: number }) =>
      p.count === 1
        ? `Profile ${p.list} now looks like factory settings, unlike its last automatic backup.`
        : `Profiles ${p.list} now look like factory settings, unlike their last automatic backups.`,
    recoveryReported: "The keyboard reported a factory reset (Fn + Space held).",
    recoveryNothingChanged: (p: { count: number }) =>
      p.count === 1
        ? "OpenWraith changed nothing and paused its automatic backups of this profile. Compare it with your automatic backups and restore what you need."
        : "OpenWraith changed nothing and paused its automatic backups of these profiles. Compare them with your automatic backups and restore what you need.",
    recoveryNever: "OpenWraith never restores anything without asking.",
    recoveryReview: "Review and restore…",
    firmwareTitle: "The keyboard's firmware changed",
    firmwareBody:
      "A firmware update usually resets the keyboard's settings. Compare it with the automatic backups made before the update.",
    pausedForCliTitle: "An OpenWraith command is using the keyboard",
    pausedForCliBody:
      "The command-line tool is running. OpenWraith sends nothing until it has finished, then reads the keyboard again. Writing and game profiles wait until then.",
    pausedForCliPurpose: "Command",
    autoPausedTitle: "Game profiles are paused",
    autoPausedRateLimit:
      "Game rules switched profiles more than 30 times in an hour, so OpenWraith paused them. They stay paused until you resume them or restart OpenWraith.",
    autoPausedFailed:
      "A game rule's profile switch didn't work, so OpenWraith paused the game profiles. They stay paused until you resume them or restart OpenWraith.",
    autoResume: "Resume",
    autoOpenSettings: "Open Settings > Games",
    untestedTitle: "Firmware not tested yet",
    untestedBody: (p: { firmware: string; tested: string }) =>
      `This keyboard runs firmware ${p.firmware}; OpenWraith was tested with ${p.tested}. Reading is safe. Check the result after every change.`,
  },
  closeGuard: {
    title: "OpenWraith is writing to the keyboard",
    body: "Closing now could leave changes unsaved. It will finish in a few seconds.",
    keepOpen: "Keep open",
    stopAndClose: "Stop safely and close",
    stopping:
      "Stopping safely: OpenWraith finishes the current step, waits for the keyboard to save and then closes.",
  },
  switcher: {
    title: (p: { n: number }) => `Switch to profile ${p.n}?`,
    body: "The keyboard's lighting changes to that profile's; no setting is changed.",
    saveWait: (p: { seconds: string }) =>
      `OpenWraith first waits ${p.seconds} for the keyboard to save your last change.`,
    drafts: "These unwritten changes will be discarded:",
    confirm: (p: { n: number }) => `Switch to profile ${p.n}`,
    already: (p: { n: number }) => `The keyboard is already on profile ${p.n}.`,
    autoHolds: (p: { label: string; n: number }) =>
      `A game rule (${p.label}) chose profile ${p.n}. If you switch now, OpenWraith keeps your choice until the game closes.`,
  },
  notices: {
    profileChanged: (p: { to: number }) => `The keyboard switched to profile ${p.to}`,
    profileChangedBody: "The settings were read again.",
    connected: "Keyboard connected",
    disconnected: "Keyboard disconnected",
    switched: (p: { n: number }) => `Profile ${p.n} is active`,
    socdToggled: (p: { on: boolean }) =>
      p.on ? "SOCD turned on on the keyboard (Fn + Right Shift)." : "SOCD turned off on the keyboard (Fn + Right Shift).",
    switchWarnings: "The keyboard answered unusually; see the technical details.",
    autoSwitched: (p: { label: string; n: number }) => `${p.label}: profile ${p.n} is active`,
    autoSwitchedBack: (p: { n: number }) => `The game closed: profile ${p.n} is active again`,
    autoWaiting: (p: { label: string; n: number }) => `${p.label}: the switch to profile ${p.n} waits`,
    autoWaitingBack: (p: { n: number }) => `The switch back to profile ${p.n} waits`,
    autoWaitingBody: (p: { reason: string }) => `It waits because ${p.reason}. It is skipped after 30 seconds.`,
    autoSkipped: (p: { label: string; n: number }) => `${p.label}: profile ${p.n} wasn't switched`,
    autoSkippedBack: (p: { n: number }) => `The switch back to profile ${p.n} was skipped`,
    autoSkippedBody: (p: { reason: string }) =>
      `OpenWraith waited 30 seconds because ${p.reason}. Switch by hand if you need that profile.`,
    autoPaused: "Game profiles are paused",
    autoPausedRateLimit: "More than 30 automatic switches in an hour.",
    autoPausedFailed: "A profile switch didn't work.",
    resetSuspected: (p: { n: number }) => `Profile ${p.n} looks reset`,
    resetSuspectedBody: "OpenWraith changed nothing. Review it on Backup & profiles.",
    firmwareChanged: (p: { known: string; current: string }) =>
      `The keyboard's firmware changed from ${p.known} to ${p.current}`,
    firmwareChangedBody: "Firmware updates usually reset the settings. Compare it with your automatic backups.",
    settingsInvalid: "OpenWraith's settings file couldn't be read",
    settingsInvalidKept: (p: { file: string }) =>
      `OpenWraith started with the default settings and kept the old file as ${p.file}.`,
    settingsInvalidLost: "OpenWraith started with the default settings.",
    autoBackupFailed: (p: { n: number }) => `The automatic backup of profile ${p.n} failed`,
    autoBackupFailedBody: "Your settings on the keyboard are unchanged. OpenWraith tries again at the next read.",
  },
  /** Per-game automatic profiles, in the shell and on Settings > Games. */
  automation: {
    /** Why an automatic switch waits; completes "It waits because …". */
    waitReasons: {
      not_connected: "the keyboard isn't connected",
      connecting: "OpenWraith is still connecting to the keyboard",
      busy: "another operation is running",
      plan_pending: "a change is open for review",
      drafts: "there are unwritten changes",
      depth_view: "the key-depth view is running",
      travel_test: "a key-depth test may still be on in the keyboard",
      other_program: "another program is using the keyboard",
      paused_for_cli: "an OpenWraith command is using the keyboard",
      reset_seen: "the keyboard reset itself",
      released: "the keyboard is released",
      fault: "OpenWraith stopped after an internal error",
      recent_foreign_activity: "another program used the keyboard less than 2 minutes ago",
      keymap_unsaved: "a key assignment is still being saved",
      profile_changing: "the profile is changing",
    },
  },
  devSim: {
    title: "Simulator controls",
    hint: "Developer tools: events happen on the simulated keyboard, as if done by hand.",
    unplug: "Unplug",
    replug: "Plug in again",
    heartbeat: "Vendor site traffic",
    switchReport: "Fn + 2 (with report)",
    switchSilent: "Fn + 3 (silent)",
    reset: "Factory reset",
    slowSave: "Slow save (5 s)",
    dropAck: "Drop the next reply",
    unplugAfter: "Unplug after 3 packets",
    info: "Simulated keyboard",
    activeProfile: "Active profile",
    unsavedEdits: "Unsaved edits",
    discardedEdits: "Discarded edits",
    writesSent: "Writes sent",
    violations: "Rule violations",
    unplugged: "Unplugged",
  },
  preview: {
    title: "Preview",
    scenario: "Scenario",
    language: "Language",
    theme: "Theme",
    speed: "Speed",
    instant: "Instant",
    apply: "Load",
    hide: "Hide",
    show: "Preview",
  },
} as const;
