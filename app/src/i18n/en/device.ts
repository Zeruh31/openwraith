// The Keyboard (device status) page.

export const device = {
  title: "Keyboard",
  description: "Connection, model and firmware of your Wraith W75.",
  connection: {
    title: "Connection",
    starting: "OpenWraith is starting.",
    searching: "Connect your Wraith W75 with its USB cable. OpenWraith finds it automatically.",
    severalKeyboards: "More than one keyboard is connected. Choose the one OpenWraith should use.",
    choose: (p: { n: number }) => `Use keyboard ${p.n}`,
    deviceTitle: (p: { n: number }) => `Keyboard ${p.n}`,
    deviceId: (p: { id: string }) => `Connection ID …${p.id}`,
    deviceLine: (p: { product: string; usb: string }) => `${p.product} · USB ${p.usb}`,
    serial: (p: { serial: string }) => `Serial number ${p.serial}`,
    opening: "Opening the keyboard…",
    openFailed: "OpenWraith couldn't open the keyboard.",
    retryingIn: (p: { seconds: string }) => `Trying again in ${p.seconds}.`,
    observing:
      "Before sending anything, OpenWraith listens for a moment to make sure no other program (such as the wraith.software site) is talking to the keyboard.",
    otherProgram: "Paused: another program is using the keyboard (see the message above). OpenWraith sends nothing.",
    reading: "Reading the keyboard's settings. This takes a few seconds.",
    readFailed: "OpenWraith couldn't read the keyboard's settings. Nothing was written.",
    ready: "Connected. OpenWraith listens to the keyboard and sends nothing while you aren't changing anything.",
    lost: "The keyboard was disconnected. Plug it in again; OpenWraith reconnects automatically.",
    resetSeen:
      "The keyboard reset itself to factory settings (Fn + Space). OpenWraith stopped and sent nothing. Read the keyboard again to continue.",
    unsupported: (p: { product: string }) =>
      `This keyboard (${p.product}) isn't supported. OpenWraith works with the Wraith W75 only.`,
    released: "OpenWraith released the keyboard. Other programs have it to themselves now.",
    fault: "OpenWraith hit an internal error and stopped using the keyboard. Restart the app.",
    connect: "Connect",
    reconnect: "Read the keyboard again",
    release: "Release keyboard",
  },
  keyboard: {
    title: "Keyboard",
    model: "Model",
    product: "Product code",
    firmware: "Firmware",
    tested: "Tested",
    notTested: "Not tested yet",
    buildDate: "Firmware build",
    usb: "USB ID",
    activeProfile: "Active profile",
    pollRate: "Polling rate",
    distanceUnit: "Distance unit",
    readAt: (p: { time: string }) => `Values read at ${p.time}`,
    notRead: "Not read yet",
    undecoded: (p: { n: number }) =>
      p.n === 1
        ? "1 reply couldn't be decoded; it is kept as raw data."
        : `${p.n} replies couldn't be decoded; they are kept as raw data.`,
  },
  beforeYouStart: {
    title: "Before you change anything",
    step1: "Close every wraith.software tab. Switching to another tab isn't enough.",
    step2: "Back up all four profiles once, so you can always go back.",
    step2Action: "Go to Backup & Restore",
    step3: "Every change is backed up automatically first and can be undone.",
    dismiss: "Got it",
  },
  safetyFolder: {
    title: "Safety backups",
    body: "Before every change, OpenWraith saves the keyboard's settings here. The command-line tool uses the same folder.",
    unavailable: "The folder can't be used, so writing is turned off.",
  },
} as const;
