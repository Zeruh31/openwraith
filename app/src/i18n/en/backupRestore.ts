// The Backup & profiles page (owned by the backup-restore page agent): backing
// up, the restore wizard, the plan review, the restore report, sharing a
// profile (export and import), the Recovery card, the automatic backups and
// the list of safety backups. Every backend code has a text (typed records
// below).

import type {
  AutoBackupReason,
  BackupFileKind,
  BlockCauseView,
  KnownGoodTableView,
  LayerView,
  PlanWarningView,
  RecoveryProfileState,
  ResetEvidenceView,
  SafetyReason,
  ShareSelectionView,
  SkipReasonView,
} from "../../ipc/bindings";

export const backupRestore = {
  title: "Backup & profiles",
  description: "Save the keyboard's settings to a file, bring them back later and share a profile.",
  /** Why an action is unavailable (shown under the button). */
  reasons: {
    notConnected: "Connect the keyboard and wait until it has been read.",
    busy: "Wait until the current operation has finished.",
    pausedForCli: "An OpenWraith command is using the keyboard. Wait until it has finished.",
    otherProgram: "Another program is using the keyboard. Close every wraith.software tab, then check again.",
    resetSeen: "The keyboard reset itself. Read it again first.",
    noProfile: "Choose at least one profile.",
    otherKeyboard: "This backup is for another keyboard.",
    firmwareTick: "Tick “Restore anyway” to compare this backup.",
  },
  /** Titles of the system file dialogs. */
  dialog: {
    filter: "OpenWraith backups",
    saveAll: "Save the backup of all profiles",
    saveActive: "Save the backup of the active profile",
    open: "Choose a backup to restore",
  },
  backup: {
    title: "Back up",
    intro: "A backup file holds the keyboard's settings: actuation, Rapid Trigger, lighting, key assignments and more.",
    all: "Back up all four profiles…",
    recommended: "Recommended",
    allHint:
      "The keyboard switches through its profiles while it is read (the lighting changes briefly) and returns to the profile it was on. No setting is changed.",
    active: "Back up the active profile only…",
    activeHint: (p: { n: number }) => `Reads profile ${p.n} without switching profiles.`,
    activeHintUnknown: "Reads the active profile without switching profiles.",
    reviewTitle: "Back up all four profiles?",
    reviewSwitches: (p: { visits: string; original: number }) =>
      `The keyboard switches to profiles ${p.visits} one after another (the lighting changes briefly), then returns to profile ${p.original}. No setting is changed.`,
    reviewNoSwitches: "Only the active profile needs to be read; no profile switch is needed.",
    reviewSaveWait: (p: { seconds: string }) =>
      `First, OpenWraith waits ${p.seconds} for the keyboard to save an earlier change.`,
    reviewStop: "You can stop it at any time; the keyboard then returns to the profile it was on.",
    reviewFile: "File",
    confirm: "Back up all profiles",
    saved: "Backup saved",
    noFileSaved: "No backup file was saved.",
    savedSummary: (p: { profiles: string; firmware: string }) => `${p.profiles} · firmware ${p.firmware}`,
    savedAt: (p: { date: string }) => `Made on ${p.date}.`,
    file: "File:",
    undecoded: (p: { n: number }) =>
      p.n === 1
        ? "1 of the keyboard's replies couldn't be decoded; it is saved as it was read."
        : `${p.n} of the keyboard's replies couldn't be decoded; they are saved as they were read.`,
    foreignActivity:
      "Another program talked to the keyboard while it was read. Close every wraith.software tab and back up again to be sure.",
  },
  /** How the profiles of a backup are described. */
  profiles: {
    all: (p: { range: string }) => `All profiles (${p.range})`,
    one: (p: { n: number }) => `Profile ${p.n} only`,
    some: (p: { list: string }) => `Profiles ${p.list}`,
  },
  restore: {
    title: "Restore",
    intro:
      "Bring the settings of a backup back to the keyboard. OpenWraith first compares and shows what would change; nothing is written until you confirm.",
    step1: "Choose a backup",
    step2: "Choose profiles",
    step3: "Compare with the keyboard",
    openFile: "Open file…",
    openOther: "Open another file…",
    orSafety: "You can also restore one of the automatic or safety backups listed below.",
    opening: "Opening the backup…",
    kinds: {
      user: "Backup file",
      safety: "Safety backup",
      automatic: "Automatic backup",
    } satisfies Record<BackupFileKind, string>,
    facts: {
      made: "Made",
      keyboard: "Keyboard",
      firmware: "Firmware",
      profiles: "Profiles",
      tool: "Made with",
      reason: "Reason",
    },
    simulator: "Made by the developer simulator: it doesn't hold a real keyboard's settings.",
    undecoded: (p: { n: number }) =>
      p.n === 1
        ? "1 of the replies in this backup couldn't be decoded when it was made."
        : `${p.n} of the replies in this backup couldn't be decoded when it was made.`,
    otherKeyboardTitle: "This backup is for another keyboard",
    otherKeyboardBody: (p: { model: string }) =>
      `It was made on ${p.model}. OpenWraith restores a backup only to the same model.`,
    notChecked: "No keyboard is connected, so OpenWraith hasn't checked yet whether this backup fits it.",
    notInBackup: "Not in this backup",
    firmwareTitle: "This backup was made with different firmware",
    firmwareBody: (p: { backup: string; keyboard: string }) =>
      `Backup: firmware ${p.backup}. Keyboard: firmware ${p.keyboard}. Restoring after a firmware update is the usual case, but the format of some settings may have changed between versions.`,
    firmwareAnyway: "Restore anyway",
    firmwareAnywayHint: "OpenWraith still compares first and shows every change before it writes anything.",
    compareHint: "OpenWraith reads the keyboard and shows what would change. Nothing is written until you confirm.",
    compare: "Compare with keyboard…",
    consentTitle: "Switch profiles to compare?",
    consentBody: (p: { list: string; count: number; active: number }) =>
      p.count === 1
        ? `Comparing reads profile ${p.list}: the keyboard switches to it and back to profile ${p.active} (the lighting changes briefly). No setting is written.`
        : `Comparing reads profiles ${p.list}: the keyboard switches to each of them and back to profile ${p.active} (the lighting changes briefly). No setting is written.`,
    consentBodyUnknown: (p: { list: string; count: number }) =>
      p.count === 1
        ? `Comparing reads profile ${p.list}: the keyboard switches to it and back (the lighting changes briefly). No setting is written.`
        : `Comparing reads profiles ${p.list}: the keyboard switches to each of them and back (the lighting changes briefly). No setting is written.`,
    consentConfirm: "Switch and compare",
  },
  review: {
    title: "Restore this backup?",
    undoTitle: (p: { list: string; count: number; date: string }) =>
      p.count === 1
        ? `Undo: restore profile ${p.list} as it was on ${p.date}?`
        : `Undo: restore profiles ${p.list} as they were on ${p.date}?`,
    undoNote: (p: { list: string; count: number }) =>
      p.count === 1
        ? `This puts the whole profile back as it was then; later changes to profile ${p.list} are undone too.`
        : `This puts these profiles back as they were then; later changes to profiles ${p.list} are undone too.`,
    /** The review of an automatic backup (from the list of automatic backups). */
    autoTitle: (p: { list: string; count: number; date: string }) =>
      p.count === 1
        ? `Restore profile ${p.list} as it was on ${p.date}?`
        : `Restore profiles ${p.list} as they were on ${p.date}?`,
    facts: {
      file: "Backup",
      backupMade: "Backup made",
      keyboardRead: "Keyboard read",
      activeProfile: "Active profile",
    },
    madeWith: (p: { date: string; firmware: string }) => `${p.date} · firmware ${p.firmware}`,
    returnsTo: (p: { n: number }) => `Profile ${p.n}; the keyboard returns to it at the end`,
    switched: (p: { list: string; count: number }) =>
      p.count === 1
        ? `To compare, the keyboard switched to profile ${p.list} and back.`
        : `To compare, the keyboard switched to profiles ${p.list} and back.`,
    profileSummary: (p: { n: number; changes: number }) =>
      p.changes === 0
        ? `Profile ${p.n}: no changes`
        : p.changes === 1
          ? `Profile ${p.n}: 1 change`
          : `Profile ${p.n}: ${p.changes} changes`,
    profileSkipped: (p: { n: number }) => `${p.n} not restored`,
    notCompared: "This profile wasn't compared with the keyboard.",
    willWrite: "Will be written",
    notRestored: "Not restored",
    alreadyMatches: (p: { n: number }) => `Already matches (${p.n})`,
    notInBackup: (p: { n: number }) => `Not in the backup (${p.n})`,
    triggers: (p: { n: number }) =>
      p.n === 1 ? "Actuation & Rapid Trigger: 1 key" : `Actuation & Rapid Trigger: ${p.n} keys`,
    columns: {
      key: "Key",
      now: "Now",
      backup: "Backup",
      setting: "Setting",
    },
    macros: (p: { list: string }) => `Macros ${p.list}`,
    advancedKeys: (p: { list: string }) => `Advanced keys ${p.list}`,
    commitNote: "Writing it also makes the keyboard save the key assignments.",
    moreChanges: (p: { n: number }) =>
      p.n === 1 ? "1 more change isn't shown." : `${p.n} more changes aren't shown.`,
    totals: (p: { n: number; list: string; count: number }) =>
      `${p.n === 1 ? "1 change" : `${p.n} changes`} in ${p.count === 1 ? "profile" : "profiles"} ${p.list}`,
    packets: (p: { writes: number; packets: number }) => `${p.writes} table writes, ${p.packets} packets in total`,
    packetsProfile: (p: { n: number; writes: number; packets: number }) =>
      `Profile ${p.n}: ${p.writes} table writes, ${p.packets} packets`,
    confirm: (p: { n: number }) => (p.n === 1 ? "Restore 1 change" : `Restore ${p.n} changes`),
    nothing: "Nothing to write: the keyboard already matches this backup.",
    nothingSkips: "These parts of the backup can't be restored:",
    safetyNote: (p: { list: string; count: number; dir: string }) =>
      p.count === 1
        ? `A safety backup of profile ${p.list} will be saved to ${p.dir} first.`
        : `A safety backup of profiles ${p.list} will be saved to ${p.dir} first.`,
  },
  /** Old → new values in the plan review. */
  changes: {
    rtOff: "RT off",
    rt: (p: { value: string }) => `RT ${p.value}`,
    rtSeparate: (p: { press: string; release: string }) => `RT ${p.press} / ${p.release}`,
    unknownMode: (p: { mode: string }) => `Unknown mode ${p.mode}`,
    trigger: (p: { actuation: string; rt: string }) => `${p.actuation} · ${p.rt}`,
    raw: "Undecoded value",
    light: {
      effect: "Effect",
      color: "Colour",
      brightness: "Brightness",
      speed: "Speed",
    },
    unknownEffect: (p: { id: string }) => `Unknown effect (${p.id})`,
    keymapKeys: (p: { n: number; keys: string }) =>
      p.n === 1 ? `1 key differs (${p.keys})` : `${p.n} keys differ (${p.keys})`,
    deadBand: (p: { top: string; bottom: string }) => `top ${p.top}, bottom ${p.bottom}`,
    deadBandNone: "not set",
    colors: (p: { n: number }) => (p.n === 1 ? "1 key colour differs" : `${p.n} key colours differ`),
    colorNone: "none",
    macroEmpty: "empty",
    macroSteps: (p: { n: number }) => (p.n === 1 ? "1 step" : `${p.n} steps`),
    macroInUse: "in use",
    advancedEmpty: "empty",
    advancedInUse: (p: { kind: string }) => `in use (type ${p.kind})`,
    entries: (p: { n: number }) => (p.n === 1 ? "1 entry differs" : `${p.n} entries differ`),
  },
  result: {
    headline: {
      done: (p: { n: number }) =>
        p.n === 1
          ? "Restore complete. The change was written, verified and kept by the keyboard (checked after a profile switch)."
          : `Restore complete. All ${p.n} changes were written, verified and kept by the keyboard (checked after a profile switch).`,
      doneUnchecked: (p: { n: number }) =>
        p.n === 1
          ? "Restore complete. The change was written and verified."
          : `Restore complete. ${p.n} changes were written and verified.`,
      nothingToDo: "Nothing to write: the keyboard already matched this backup.",
      cancelled: (p: { written: number; rest: number }) =>
        `Stopped. ${p.written === 1 ? "1 change was" : `${p.written} changes were`} written before stopping; ${p.rest === 1 ? "1 was" : `${p.rest} were`} not.`,
      notWritten: "Nothing was written.",
      failed: (p: { written: number; total: number }) =>
        `The restore stopped with an error. ${p.written} of ${p.total} changes were written and verified; the rest were not.`,
      maybeLost: (p: { n: number }) =>
        p.n === 1
          ? "Written and verified, but the keyboard may not have saved 1 change. Read the keyboard again to check, or undo."
          : `Written and verified, but the keyboard may not have saved ${p.n} changes. Read the keyboard again to check, or undo.`,
      lost: (p: { n: number }) =>
        p.n === 1
          ? "The keyboard didn't keep 1 change. Run the restore again, or undo it."
          : `The keyboard didn't keep ${p.n} changes. Run the restore again, or undo it.`,
    },
    persistenceTitle: "Persistence check",
    persistence: {
      profiles: (p: { list: string; count: number }) =>
        p.count === 1
          ? `Profile ${p.list} was selected again after switching away.`
          : `Profiles ${p.list} were selected again after switching away.`,
      kept: (p: { n: number }) =>
        p.n === 1
          ? "1 change was still there after switching back."
          : `${p.n} changes were still there after switching back.`,
      lost: (p: { n: number }) =>
        p.n === 1 ? "1 change was gone after switching back." : `${p.n} changes were gone after switching back.`,
      notChecked: (p: { n: number }) =>
        p.n === 1
          ? "1 change couldn't be checked: SOCD was switched on the keyboard after it was written."
          : `${p.n} changes couldn't be checked: SOCD was switched on the keyboard after they were written.`,
    },
    finalTitle: "Final check",
    finalOk: (p: { list: string; count: number }) =>
      p.count === 1
        ? `SOCD is switched on or off for all profiles at once. Profile ${p.list} was read again and has the restored setting.`
        : `SOCD is switched on or off for all profiles at once. Profiles ${p.list} were read again and have the restored setting.`,
    finalMismatches:
      "SOCD is switched on or off for all profiles at once, but this profile doesn't have the restored setting (another program may have changed it):",
    finalMismatch: (p: { n: number; table: string }) => `Profile ${p.n}: ${p.table}`,
    clearedTitle: "Macros left empty",
    clearedBody:
      "Writing Macro 1 makes the keyboard clear Macros 2–10, and the restore stopped before writing these again. Undo brings them back.",
    clearedItem: (p: { n: number; slot: number }) => `Profile ${p.n}: Macro ${p.slot}`,
    danglingTitle: "Keys that now do nothing",
    danglingBody:
      "The restore stopped part-way, so these keys refer to an empty macro or advanced key. Undo brings them back.",
    danglingItem: (p: { n: number; key: string; target: string }) => `Profile ${p.n}: ${p.key} → ${p.target}`,
    counts: {
      title: "In numbers",
      written: "Written and verified",
      kept: "Kept after a profile switch",
      maybeLost: "May not have been saved",
      lost: "Lost",
      failed: "Failed",
      notAttempted: "Not written",
      alreadyMatched: "Already matched",
      resent: "Re-sent unchanged",
      skipped: "Not restored",
    },
    nextSteps: "Next steps",
    runAgain: "Run the restore again",
    runAgainHint: "To run it again, choose the backup on the Backup & profiles page and compare it with the keyboard.",
    connectFirst: "Connect the keyboard first.",
  },
  safety: {
    title: "Safety backups",
    intro: "OpenWraith saves one automatically before every change. Restoring one undoes the changes made since.",
    empty: "No safety backups yet. OpenWraith makes one automatically before every change.",
    loadFailed: "The list of safety backups couldn't be read.",
    columns: {
      made: "Made",
      reason: "Reason",
      keyboard: "Keyboard",
      actions: "Actions",
    },
    restore: "Restore…",
    restoreRow: (p: { date: string; reason: string }) => `Restore the safety backup from ${p.date} (${p.reason})`,
    showRow: (p: { date: string }) => `Show the safety backup from ${p.date} in its folder`,
    refresh: "Refresh the list",
    showFolder: "Show the folder",
    count: (p: { shown: number; total: number }) => `Showing ${p.shown} of ${p.total}`,
  },
  /** The automatic backups (the newest copies of each profile OpenWraith made when it read the keyboard). */
  autoBackups: {
    title: "Automatic backups",
    intro: (p: { keep: number }) =>
      `When OpenWraith reads the keyboard and a profile has changed, it saves a copy of that profile here. It keeps the newest ${p.keep} of each profile; pinned ones and safety backups are never deleted.`,
    off: "Automatic backups are off: OpenWraith makes no new ones and doesn't check the keyboard for a reset. The ones listed here can still be restored.",
    openSettings: "Open the background settings",
    everyKeyboard:
      "No keyboard has been read yet, so these are the automatic backups of every keyboard OpenWraith has read.",
    empty: "No automatic backups yet. OpenWraith makes the first one when it reads the keyboard.",
    loadFailed: "The list of automatic backups couldn't be read.",
    unidentified:
      "This keyboard reports no serial number: automatic backups of another W75 on this PC would be mixed with these.",
    columns: {
      made: "Made",
      profile: "Profile",
      reason: "Reason",
      actions: "Actions",
    },
    reasons: {
      first_read: "First read",
      after_write: "After a change made in OpenWraith",
      changed_elsewhere: "Changed outside OpenWraith",
      kept_current: "Kept after a possible reset",
      manual: "“Back up now” in the notification area",
    } satisfies Record<AutoBackupReason, string>,
    /** A pinned backup (never deleted). */
    kept: "Pinned",
    keep: "Pin",
    keepRow: (p: { n: number; date: string }) => `Pin the automatic backup of profile ${p.n} from ${p.date}`,
    keepHint: "A pinned backup is never deleted and doesn't count towards the newest ones.",
    restore: "Restore…",
    restoreRow: (p: { n: number; date: string }) => `Restore the automatic backup of profile ${p.n} from ${p.date}`,
    showRow: (p: { n: number; date: string }) =>
      `Show the automatic backup of profile ${p.n} from ${p.date} in its folder`,
    refresh: "Refresh the list",
    showFolder: "Show the folder",
    count: (p: { shown: number; total: number }) => `Showing ${p.shown} of ${p.total}`,
  },
  /** The Recovery card: settings that look reset, compared with the automatic backups. */
  recovery: {
    title: "Recovery",
    never: "OpenWraith never restores anything without asking.",
    loadFailed: "OpenWraith couldn't read what it knows about a reset.",
    attentionTitle: "The keyboard's settings may have been reset",
    fineTitle: "Nothing looks reset",
    fineBody: "Every profile OpenWraith has compared matches its newest automatic backup.",
    resetReported: (p: { time: string }) =>
      `The keyboard reported a reset (at ${p.time}). Holding Fn + Space puts the keyboard back to its factory settings.`,
    resetReportedNoTime:
      "The keyboard reported a reset. Holding Fn + Space puts the keyboard back to its factory settings.",
    firmwareChanged: (p: { known: string; current: string }) =>
      `The keyboard's firmware changed from ${p.known} to ${p.current}. A firmware update usually resets the settings.`,
    heldProfiles: (p: { list: string; count: number }) =>
      p.count === 1
        ? `Profile ${p.list} looks reset: it differs from its automatic backup.`
        : `Profiles ${p.list} look reset: they differ from their automatic backups.`,
    reconnectHint:
      "Reconnect so that OpenWraith reads the keyboard again and compares it with its automatic backups. Nothing is written.",
    reconnect: "Reconnect and compare",
    keep: "It's fine, keep them",
    keepHint:
      "If you reset the keyboard on purpose or changed these settings yourself: OpenWraith saves the current settings as the newest automatic backups and stops warning. Nothing is sent to the keyboard. The older automatic backups stay in the list until newer ones replace them; pin one to keep it.",
    keepFirst: "Reconnect and compare first.",
    keepCompareFirst: "Switch to the profiles not compared yet first; OpenWraith compares each one when it reads it.",
    unidentified:
      "This keyboard reports no serial number, so OpenWraith can't tell it from another W75: settings that look reset may also mean that another keyboard of this model was connected. Check the date of the automatic backup before you restore it.",
    kept: "OpenWraith keeps the current settings as they are.",
    autoOff: "Automatic backups are off, so OpenWraith doesn't compare the keyboard with them.",
    openSettings: "Open the background settings",
    profiles: "Profiles",
    profileNamed: (p: { n: number; name: string }) => `Profile ${p.n} · ${p.name}`,
    states: {
      no_known_good: "No automatic backup yet",
      not_checked: "Not compared yet",
      fine: "Matches its automatic backup",
      reset_suspected: "Looks reset",
    } satisfies Record<RecoveryProfileState, string>,
    notes: {
      no_known_good: "OpenWraith makes the first automatic backup the next time it reads this profile.",
      not_checked: "OpenWraith compares it the next time it reads this profile, for example when you switch to it.",
      fine: (p: { date: string }) => `Read on ${p.date}; it matched its automatic backup.`,
    },
    evidenceTitle: "Why it looks reset",
    evidence: {
      reset_seen: "The keyboard reported a reset before this profile was read.",
      firmware_changed: (p: { known: string; current: string }) =>
        `The automatic backup was made with firmware ${p.known}; the keyboard now has ${p.current}.`,
      triggers_to_factory: "Actuation and Rapid Trigger are back to the factory settings, unlike in the automatic backup.",
      keymap_group_to_factory: (p: { list: string }) =>
        `Back to the factory settings, unlike in the automatic backup: ${p.list}.`,
    } satisfies Record<ResetEvidenceView["evidence"], unknown>,
    factoryNow: (p: { list: string }) => `At the factory settings now: ${p.list}.`,
    changed: (p: { list: string }) => `Also different from the automatic backup: ${p.list}.`,
    comparedWith: (p: { date: string; age: string }) =>
      `Compared with the automatic backup from ${p.date}, made ${p.age} before the comparison.`,
    comparedWithDate: (p: { date: string }) => `Compared with the automatic backup from ${p.date}.`,
    /** The settings compared with the automatic backups. */
    tables: {
      triggers: "Actuation & Rapid Trigger",
      main_light: "Lighting",
      custom_colors1: "Key colours",
      base_keymap: "Key assignments",
      fn_keymap: "Fn layer key assignments",
      macros: "Macros",
      advanced_keys: "Advanced keys",
      dead_band: "Dead band",
      socd_pairs: "SOCD key pairs",
    } satisfies Record<KnownGoodTableView, string>,
    /** How old a backup was when it was compared. */
    age: {
      underMinute: "less than a minute",
      minutes: (p: { n: number }) => (p.n === 1 ? "1 minute" : `${p.n} minutes`),
      hours: (p: { n: number }) => (p.n === 1 ? "1 hour" : `${p.n} hours`),
      days: (p: { n: number }) => (p.n === 1 ? "1 day" : `${p.n} days`),
    },
    restoreFrom: (p: { n: number }) => `Automatic backup for profile ${p.n}`,
    backupOption: (p: { date: string; reason: string; kept: boolean }) =>
      p.kept ? `${p.date} · ${p.reason} · pinned` : `${p.date} · ${p.reason}`,
    restore: (p: { n: number }) => `Restore profile ${p.n}…`,
    noBackups: "There is no automatic backup of this profile to restore from.",
    restoreAll: "Restore all profiles…",
    restoreAllHint: (p: { list: string }) =>
      `Restores profiles ${p.list} from their newest automatic backups, in one review.`,
    compareHint: "OpenWraith first compares and shows what would change; nothing is written until you confirm.",
    notCheckedNote: "Profiles that weren't read since then are compared the next time OpenWraith reads them.",
    firmwareTitle: "The automatic backups were made with other firmware",
    firmwareBody: (p: { backup: string; keyboard: string }) =>
      `Automatic backups: firmware ${p.backup}. Keyboard: firmware ${p.keyboard}. Restoring after a firmware update is the usual case, but the format of some settings may have changed between versions.`,
    reviewTitle: (p: { list: string; count: number }) =>
      p.count === 1
        ? `Restore profile ${p.list} from its automatic backup?`
        : `Restore profiles ${p.list} from their automatic backups?`,
  },
  /** Why a differing table isn't restored. */
  skipReasons: {
    unconfirmed_command:
      "OpenWraith doesn't write this setting yet: writing it hasn't been confirmed on the keyboard.",
    no_known_command: "OpenWraith doesn't know how to write this setting.",
    unsupported_light_mode: (p: { mode: string }) => `The backup uses a light effect (${p.mode}) that can't be written.`,
    invalid_light_record: "The light settings in the backup are invalid.",
    knob: "The knob has no trigger settings.",
    unknown_trigger_mode: (p: { mode: string }) => `The backup uses an unknown trigger mode (${p.mode}).`,
    second_travel_differs: "The key uses a second actuation travel that OpenWraith doesn't change.",
    press_release_differ: "The key's Rapid Trigger press and release values differ where they should match.",
    out_of_range: "A value in the backup is outside what the keyboard accepts.",
    trigger_limits_unknown: "The keyboard's trigger limits aren't known.",
    dead_band_out_of_range: "A dead-band value in the backup is outside what the keyboard accepts.",
    dead_band_unit_unknown: "The keyboard's dead-band unit isn't known.",
    distance_unit_changed: (p: { backup: string; keyboard: string }) =>
      `The distance unit differs (backup ${p.backup}, keyboard ${p.keyboard}).`,
    keymap_group_blocked: (p: { cause: string }) =>
      `Key assignments, macros and advanced keys can only be restored together, and that isn't safe here: ${p.cause}`,
    not_in_keyboard: "The keyboard doesn't have this setting.",
    unusable: "This part of the backup can't be used.",
    read_only_bytes: "Only values that OpenWraith never writes differ (the SOCD shortcut).",
    unwritable_socd_pairs: "The SOCD pairs in the backup aren't in a form OpenWraith can write.",
    not_imported: "This setting isn't part of the import.",
  } satisfies Record<SkipReasonView["code"], unknown>,
  /** Why the key assignments can't be restored (completes `keymap_group_blocked`). */
  blockCauses: {
    unwritable_advanced_key: (p: { slot: number }) =>
      `advanced key ${p.slot} in the backup isn't a DKS, tap-or-hold or toggle key that OpenWraith can write.`,
    non_canonical_macro: (p: { slot: number }) => `macro ${p.slot} in the backup has an unusual format.`,
    malformed_reference: (p: { layer: string; index: number }) =>
      `${p.layer} in the backup has a broken entry (number ${p.index}).`,
    incomplete: (p: { table: string }) => `this part wasn't read completely: ${p.table}.`,
    fn_keymap_not_writable:
      "the Fn layer key assignments can't be written, so the keyboard wouldn't save the key assignments.",
    /** `keys`: every replaced shortcut by name ("Fn + 2 and Fn + Space"). */
    critical_fn_override: (p: { keys: string; count: number }) =>
      p.count === 1
        ? `the imported Fn layer replaces one of the keyboard's own shortcuts, ${p.keys}, and that wasn't allowed.`
        : `the imported Fn layer replaces the keyboard's own shortcuts ${p.keys}, and that wasn't allowed.`,
    unwritable_imported_entry: (p: { layer: string; index: number }) =>
      `${p.layer} in the file have an entry OpenWraith doesn't write (key number ${p.index}). Only keys, media keys, mouse buttons, key combinations, macros and advanced keys are imported.`,
  } satisfies Record<BlockCauseView["code"], unknown>,
  /** Keymap layers inside a sentence. */
  layers: {
    base: "the key assignments",
    fn: "the Fn layer key assignments",
  } satisfies Record<LayerView, string>,
  /** Differences that don't block the restore. */
  planWarnings: {
    firmware_differs: (p: { backup: string; keyboard: string }) =>
      `The backup was made with firmware ${p.backup}; the keyboard has ${p.keyboard}.`,
    constant_differs: "A value that normally never changes differs between the backup and the keyboard.",
    capability_flags_differ: "The keyboard reports different capabilities than when the backup was made.",
    socd_power_differs_in_backup: (p: { profile: number; on: boolean }) =>
      `The profiles in the backup have SOCD switched on and off differently, but the keyboard has one SOCD switch for all profiles. The setting of profile ${p.profile} (active when the backup was made) is restored: SOCD ${p.on ? "on" : "off"}.`,
    socd_power_not_imported: (p: { on: boolean }) =>
      `Only the SOCD key pairs are imported. Whether SOCD is on is one setting for all profiles, not part of a profile, so it stays ${p.on ? "on" : "off"}.`,
    imported: (p: { from: number; into: number }) =>
      `Profile ${p.from} of the file is written to profile ${p.into} of the keyboard.`,
    imported_macros: (p: { slots: string }) => `Macros from the file are written: ${p.slots}.`,
    fn_builtin_overridden: (p: { key: string; kind: "other" | "profile" | "reset" }) =>
      p.kind === "reset"
        ? `The Fn layer replaces Fn + ${p.key}, which (held) resets the keyboard. What happens when it is replaced has not been tested.`
        : p.kind === "profile"
          ? `The Fn layer replaces Fn + ${p.key}, the keyboard's own switch to profile ${p.key}.`
          : `The Fn layer replaces one of the keyboard's own Fn shortcuts: Fn + ${p.key}.`,
    import_kept_as_read: (p: { layer: string; keys: string }) =>
      `In the file, ${p.layer} differ at positions that are never imported (the knob, the Fn key, keys the keyboard locks): ${p.keys}. The keyboard's entries are kept.`,
    unverified: (p: { layer: string; index: number }) =>
      `An imported entry in ${p.layer} (key number ${p.index}) isn't verified on a W75 yet. The Wraith website sends the same bytes.`,
  } satisfies Record<PlanWarningView["code"], unknown>,
  /** Why a safety backup was made. */
  safetyReasons: {
    set_trigger: "Before an actuation change",
    set_light: "Before a lighting change",
    restore: "Before a restore",
    set_keys: "Before a key change",
    set_colors: "Before a key-colour change",
    set_deadband: "Before a dead-band change",
    set_socd: "Before a SOCD change",
    import: "Before an import",
    other: "Safety backup",
  } satisfies Record<SafetyReason, string>,
  /** Share a profile: export one profile to a file, import a profile file into a profile. */
  share: {
    title: "Share a profile",
    intro:
      "Save one profile to a file to share it or keep it, and import a profile file into any of the keyboard's profiles.",
    noSerial: "A profile file never holds the keyboard's USB serial number.",
    /** Titles and filter names of the system file dialogs. */
    dialog: {
      save: "Save the profile file",
      saveFilter: "OpenWraith profile files",
      open: "Choose a profile file to import",
      openFilter: "OpenWraith profile files and backups",
      openBackup: "Choose the backup to export a profile from",
    },
    profileOption: (p: { n: number; active: boolean }) => (p.active ? `Profile ${p.n} (active)` : `Profile ${p.n}`),
    /** The parts of a profile, as the import offers them. */
    parts: {
      triggers: "Actuation & Rapid Trigger",
      lighting: "Lighting and key colours",
      keys: "Key assignments, macros and advanced keys",
      dead_band: "Dead band",
      socd: "SOCD key pairs",
    } satisfies Record<keyof ShareSelectionView, string>,
    export: {
      title: "Export a profile",
      intro:
        "Save one profile as a profile file (.owprofile.json): read from the keyboard now, or taken from a backup file or an automatic backup.",
      /** Where the profile is read from. */
      source: "Read the profile from",
      sources: {
        keyboard: "The keyboard now",
        file: "A backup file",
        automatic: "An automatic backup",
      },
      sourceHints: {
        keyboard: "OpenWraith reads the profile from the connected keyboard.",
        file: "A backup you saved, or a safety backup. The keyboard isn't used.",
        automatic: "A copy OpenWraith made when it read the keyboard. The keyboard isn't used.",
      },
      profile: "Profile",
      chooseFile: "Choose a backup…",
      chooseOther: "Choose another backup…",
      fileProfile: "Profile of the backup",
      fileMade: (p: { date: string; firmware: string }) => `Made on ${p.date} · firmware ${p.firmware}`,
      automatic: "Automatic backup",
      automaticOption: (p: { n: number; date: string; reason: string }) => `Profile ${p.n} · ${p.date} · ${p.reason}`,
      noAutomatic: "No automatic backups yet: OpenWraith makes them when it reads the keyboard.",
      automaticFailed: "The automatic backups couldn't be listed.",
      name: "Name (optional)",
      nameHint: "Saved in the file, for example “Tournament”. You choose the file name next.",
      parts: "What the file holds",
      partsHint:
        "Dead band and SOCD key pairs are left out unless you tick them: the dead band is usually tuned to one keyboard, and some games forbid SOCD.",
      holds: "Whoever imports the file chooses again which of these parts to use.",
      macrosNote: "Macros are saved as the keys they press, so anyone you give the file to can read them.",
      /** Why the export can't start yet (shown under the button). */
      reasons: {
        noPart: "Choose at least one part to export.",
        noFile: "Choose a backup file first.",
        noAutomatic: "Choose an automatic backup first.",
      },
      switchNote: (p: { n: number }) =>
        `Reading profile ${p.n} switches the keyboard to it and back (the lighting changes briefly). No setting is changed.`,
      button: (p: { n: number }) => `Export profile ${p.n}…`,
      consentTitle: "Switch profiles to read it?",
      consentBody: (p: { n: number; active: number }) =>
        `To read profile ${p.n}, the keyboard switches to it and back to profile ${p.active} (the lighting changes briefly). No setting is changed.`,
      consentBodyUnknown: (p: { n: number }) =>
        `To read profile ${p.n}, the keyboard switches to it and back (the lighting changes briefly). No setting is changed.`,
      consentConfirm: "Switch and export",
      saved: "Profile file saved",
      savedSummary: (p: { n: number; firmware: string }) => `Profile ${p.n} · firmware ${p.firmware}`,
      switched: (p: { n: number }) => `To read it, the keyboard switched to profile ${p.n} and back.`,
      file: "File:",
      noFileSaved: "No profile file was saved.",
    },
    /** What a profile file holds. */
    contents: {
      title: "What the file holds",
      titleOf: (p: { n: number }) => `What profile ${p.n} of the file holds`,
      otherProfile: (p: { n: number }) => `The review shows what profile ${p.n} of the file holds.`,
      none: "No settings OpenWraith can read.",
      missing: (p: { list: string }) => `Not in the file: ${p.list}.`,
      advanced: (p: { n: number }) => (p.n === 1 ? "1 advanced key is in use." : `${p.n} advanced keys are in use.`),
      macrosTitle: "Macros, as the keys they press",
      macro: (p: { n: number; steps: number; play: string }) =>
        p.steps === 1 ? `Macro ${p.n} · 1 step · ${p.play}` : `Macro ${p.n} · ${p.steps} steps · ${p.play}`,
      macroUnknown: (p: { n: number }) =>
        `Macro ${p.n} can't be shown as steps, so the key assignments, macros and advanced keys can't be imported from this file.`,
      press: (p: { key: string }) => `Press ${p.key}`,
      release: (p: { key: string }) => `Release ${p.key}`,
      wait: (p: { step: string; ms: string }) => `${p.step}, then wait ${p.ms} ms`,
    },
    import: {
      title: "Import a profile",
      intro:
        "Write the settings of a profile file into one of the keyboard's profiles. OpenWraith first compares and shows what would change; nothing is written until you confirm.",
      open: "Open a profile file…",
      openOther: "Open another profile file…",
      backupsToo: "A backup file works too: its active profile is offered first.",
      facts: {
        made: "Made",
        keyboard: "Keyboard",
        firmware: "Firmware",
        profiles: "Profiles in the file",
        tool: "Made with",
      },
      simulator: "Made by the developer simulator: it doesn't hold a real keyboard's settings.",
      otherKeyboardTitle: "This file is for another keyboard",
      otherKeyboardBody: (p: { model: string }) =>
        `It was made on ${p.model}. OpenWraith imports a profile only into the same model.`,
      notChecked: "No keyboard is connected, so OpenWraith hasn't checked yet whether this file fits it.",
      from: "Profile of the file",
      into: "Import into profile",
      intoHint: (p: { n: number }) =>
        `Comparing reads profile ${p.n}: the keyboard switches to it and back (the lighting changes briefly).`,
      parts: "What to import",
      partsHint:
        "Actuation and lighting are chosen by default. Choose the other parts only if you want them exactly as in the file.",
      notInFile: "Not in this file.",
      keysHint: "Replaces the profile's key assignments, macros and advanced keys as a whole.",
      socdHint: "Whether SOCD is on is one setting for the whole keyboard: it stays as it is.",
      firmwareTitle: "This file was made with different firmware",
      firmwareBody: (p: { file: string; keyboard: string }) =>
        `File: firmware ${p.file}. Keyboard: firmware ${p.keyboard}. The format of some settings may have changed between versions.`,
      firmwareAnyway: "Import anyway",
      firmwareAnywayHint: "OpenWraith still compares first and shows every change before it writes anything.",
      review: "Compare and review…",
      compareHint: "OpenWraith reads the keyboard and shows what would change. Nothing is written until you confirm.",
    },
    /** Why the import can't be reviewed yet (shown under the button). */
    reasons: {
      noPart: "Choose at least one part to import.",
      otherKeyboard: "This file is for another keyboard.",
      firmwareTick: "Tick “Import anyway” to compare this file.",
    },
    /** The import's review (the restore review, in import words). */
    review: {
      title: (p: { from: number; into: number }) => `Import profile ${p.from} of the file into profile ${p.into}?`,
      chosen: (p: { list: string }) => `Chosen parts: ${p.list}.`,
      confirm: (p: { n: number }) => (p.n === 1 ? "Import 1 change" : `Import ${p.n} changes`),
      safetyNote: (p: { n: number; dir: string }) =>
        `A safety backup of profile ${p.n} will be saved to ${p.dir} first.`,
      file: "File",
      fileMade: "File made",
      columnFile: "File",
      profileSkipped: (p: { n: number }) => `${p.n} not imported`,
      notImported: "Not imported",
      notInFile: (p: { n: number }) => `Not in the file (${p.n})`,
      nothing: "Nothing to write: the keyboard already matches the chosen parts of the file.",
      nothingSkips: "These parts of the file can't be imported:",
      keymapGroupBlocked: (p: { cause: string }) =>
        `Key assignments, macros and advanced keys can only be imported together, and that isn't safe here: ${p.cause}`,
      blocked: "Some of the chosen parts can't be imported as they are; “Not imported” below says why.",
    },
    /** The file's Fn layer replaces one of the keyboard's own shortcuts (its own tick). */
    critical: {
      title: "The file replaces one of the keyboard's own shortcuts",
      profile: (p: { k: number; n: number }) =>
        p.k === p.n
          ? `In the file, Fn + ${p.k} does something else. On the keyboard, Fn + ${p.k} switches to profile ${p.k}, the profile you are importing into, so on this profile it switches nowhere and replacing it takes nothing away. It still needs your confirmation.`
          : `In the file, Fn + ${p.k} does something else. On the keyboard, Fn + ${p.k} switches to profile ${p.k}; after this import you couldn't switch to profile ${p.k} from the keyboard while profile ${p.n} is active, only from OpenWraith.`,
      reset: "In the file, Fn + Space does something else. On the keyboard, Fn + Space (held) resets it. What happens when it is replaced has not been tested.",
      other: (p: { key: string }) => `In the file, Fn + ${p.key} replaces one of the keyboard's own shortcuts.`,
      notImported: "The key assignments, macros and advanced keys are imported only if you allow it below.",
      confirmProfile: (p: { k: number; n: number }) =>
        p.k === p.n
          ? `Import it anyway: replace Fn + ${p.k} on profile ${p.n} (it only switches to this same profile)`
          : `Import it anyway: Fn + ${p.k} will no longer switch to profile ${p.k} while profile ${p.n} is active`,
      confirmReset: (p: { n: number }) =>
        `Import it anyway: Fn + Space will no longer reset the keyboard while profile ${p.n} is active`,
      confirmOther: (p: { key: string; n: number }) =>
        `Import it anyway: Fn + ${p.key} will no longer do its own job while profile ${p.n} is active`,
      /** One tick for several replaced shortcuts, each explained in its own note above. */
      confirmAll: (p: { keys: string; n: number }) =>
        `Import it anyway: replace ${p.keys} on profile ${p.n}, with the consequences described above`,
      /** A key in the list of {@link confirmAll}. */
      fnKey: (p: { key: string }) => `Fn + ${p.key}`,
    },
    /** The import's result (the restore result, in import words). */
    result: {
      headline: {
        done: (p: { n: number }) =>
          p.n === 1
            ? "Import complete. The change was written, verified and kept by the keyboard (checked after a profile switch)."
            : `Import complete. All ${p.n} changes were written, verified and kept by the keyboard (checked after a profile switch).`,
        doneUnchecked: (p: { n: number }) =>
          p.n === 1
            ? "Import complete. The change was written and verified."
            : `Import complete. ${p.n} changes were written and verified.`,
        nothingToDo: "Nothing to write: the keyboard already matched the chosen parts of the file.",
        cancelled: (p: { written: number; rest: number }) =>
          `Stopped. ${p.written === 1 ? "1 change was" : `${p.written} changes were`} written before stopping; ${p.rest === 1 ? "1 was" : `${p.rest} were`} not.`,
        notWritten: "Nothing was written.",
        failed: (p: { written: number; total: number }) =>
          `The import stopped with an error. ${p.written} of ${p.total} changes were written and verified; the rest were not.`,
        maybeLost: (p: { n: number }) =>
          p.n === 1
            ? "Written and verified, but the keyboard may not have saved 1 change. Read the keyboard again to check, or undo."
            : `Written and verified, but the keyboard may not have saved ${p.n} changes. Read the keyboard again to check, or undo.`,
        lost: (p: { n: number }) =>
          p.n === 1
            ? "The keyboard didn't keep 1 change. Run the import again, or undo it."
            : `The keyboard didn't keep ${p.n} changes. Run the import again, or undo it.`,
      },
      clearedBody:
        "Writing Macro 1 makes the keyboard clear Macros 2–10, and the import stopped before writing these again. Undo brings them back.",
      danglingBody:
        "The import stopped part-way, so these keys refer to an empty macro or advanced key. Undo brings them back.",
      skipped: "Not imported",
      runAgain: "Run the import again",
    },
    /** Refusals of an export or an import (`ErrorDetail` "edit"; nothing was sent). */
    refusals: {
      no_changes: "Choose at least one part to import.",
      not_available: "This isn't available for this keyboard.",
      unsupported_current_table: (p: { table: string }) =>
        `OpenWraith can't import ${p.table} as it is in the file.`,
      other: "OpenWraith refused it before anything was sent to the keyboard.",
    },
  },
} as const;
