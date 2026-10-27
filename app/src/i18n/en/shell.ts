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
    intro: "This operation finished while its window was closed or reloaded. Here is what happened.",
  },
  nav: {
    label: "Pages",
    device: "Keyboard",
    actuation: "Actuation & Rapid Trigger",
    lighting: "Lighting",
    backup: "Backup & Restore",
    settings: "Settings",
    unwritten: "Unwritten changes",
  },
  header: {
    profileLabel: "Active profile",
    profileOption: (p: { n: number }) => `Profile ${p.n}`,
    switcherBlocked: {
      busy: "Busy",
      notReady: "Not connected",
      blocked: "Writing is paused",
    },
    statusButton: "Keyboard connection details",
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
    drafts: (p: { pages: string }) => `Unwritten changes on ${p.pages} will be discarded.`,
    confirm: (p: { n: number }) => `Switch to profile ${p.n}`,
    already: (p: { n: number }) => `The keyboard is already on profile ${p.n}.`,
  },
  notices: {
    profileChanged: (p: { to: number }) => `The keyboard switched to profile ${p.to}`,
    profileChangedBody: "The settings were read again.",
    connected: "Keyboard connected",
    disconnected: "Keyboard disconnected",
    switched: (p: { n: number }) => `Profile ${p.n} is active`,
    switchWarnings: "The keyboard answered unusually; see the technical details.",
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
