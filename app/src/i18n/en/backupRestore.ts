// The Backup & Restore page (owned by the backup-restore page agent): backing
// up, the restore wizard, the plan review, the restore report and the list
// of safety backups. Every backend code has a text (typed records below).

import type {
  BackupFileKind,
  BlockCauseView,
  LayerView,
  PlanWarningView,
  SafetyReason,
  SkipReasonView,
} from "../../ipc/bindings";

export const backupRestore = {
  title: "Backup & Restore",
  description: "Save the keyboard's settings to a file and bring them back later.",
  /** Why an action is unavailable (shown under the button). */
  reasons: {
    notConnected: "Connect the keyboard and wait until it has been read.",
    busy: "Wait until the current operation has finished.",
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
    orSafety: "You can also restore one of the safety backups listed below.",
    opening: "Opening the backup…",
    kinds: {
      user: "Backup file",
      safety: "Safety backup",
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
          ? "1 change to a setting shared between profiles couldn't be checked separately."
          : `${p.n} changes to settings shared between profiles couldn't be checked separately.`,
    },
    finalTitle: "Final check",
    finalOk: (p: { list: string; count: number }) =>
      p.count === 1
        ? `Profile ${p.list} was read again and holds the backup's values.`
        : `Profiles ${p.list} were read again and hold the backup's values.`,
    finalMismatches: "These settings no longer hold the backup's values:",
    finalMismatch: (p: { n: number; table: string }) => `Profile ${p.n}: ${p.table}`,
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
    runAgainHint: "To run it again, choose the backup on the Backup & Restore page and compare it with the keyboard.",
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
    critical_fn_override: (p: { index: number }) =>
      `the imported Fn layer replaces one of the keyboard's own shortcuts (a profile switch or the reset, key number ${p.index}), and that wasn't allowed.`,
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
    shared_tables_may_change_other_profiles: (p: { tables: string; profiles: string }) =>
      `These settings may be shared between profiles: ${p.tables}. Restoring them can also change profiles ${p.profiles}.`,
    likely_shared_table_differs: (p: { table: string }) =>
      `This setting is probably shared between profiles and differs from the backup: ${p.table}.`,
    imported: (p: { from: number; into: number }) =>
      `Profile ${p.from} of the file is written to profile ${p.into} of the keyboard.`,
    imported_macros: (p: { slots: string }) => `Macros from the file are written: ${p.slots}.`,
    fn_builtin_overridden: (p: { index: number }) =>
      `The Fn layer replaces one of the keyboard's own Fn shortcuts (key number ${p.index}).`,
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
    other: "Safety backup",
  } satisfies Record<SafetyReason, string>,
} as const;
