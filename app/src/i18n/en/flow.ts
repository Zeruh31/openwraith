// The write flow: steps, progress, the confirmation footer, results, table
// names, write warnings and commit-trigger reasons.

import type {
  CancelState,
  CommitTriggerReasonView,
  OpKind,
  ReadPurpose,
  ResendReasonView,
  WriteWarningView,
} from "../../ipc/bindings";

type Code = WriteWarningView["code"];

export const flow = {
  opKinds: {
    refresh: "Reading the keyboard",
    connect: "Connecting",
    profile_switch: "Switching profile",
    trigger_edit: "Changing actuation",
    light_edit: "Changing the lighting",
    backup_active: "Backing up the active profile",
    backup_all: "Backing up all profiles",
    restore_prepare: "Comparing with a backup",
    restore: "Restoring a backup",
    recheck: "Checking again",
    keymap_edit: "Changing key assignments",
    color_edit: "Changing key colours",
    dead_band_edit: "Changing the dead band",
    socd_edit: "Changing SOCD",
    depth_view: "Showing key depth",
    travel_close: "Stopping the key-depth view",
    export: "Exporting a profile",
    import_prepare: "Comparing with the profile file",
    import: "Importing a profile",
  } satisfies Record<OpKind, string>,
  /** Dialog titles while a write runs and on its result (the review asks a question instead). */
  titles: {
    refresh: "Reading the keyboard",
    connect: "Connecting",
    profile_switch: "Profile switch",
    trigger_edit: "Actuation change",
    light_edit: "Lighting change",
    backup_active: "Backup of the active profile",
    backup_all: "Backup of all profiles",
    restore_prepare: "Restore",
    restore: "Restore",
    recheck: "Checking again",
    keymap_edit: "Key assignment change",
    color_edit: "Key colour change",
    dead_band_edit: "Dead band change",
    socd_edit: "SOCD change",
    depth_view: "Key depth",
    travel_close: "Key-depth view",
    export: "Profile export",
    import_prepare: "Profile import",
    import: "Profile import",
  } satisfies Record<OpKind, string>,
  preparing: "Reading what the keyboard has now…",
  steps: {
    checking_other_programs: "Checking that no other program is using the keyboard",
    reading: {
      snapshot: "Reading the keyboard's settings",
      safety_backup: "Reading the current settings for the safety backup",
      backup: "Reading the keyboard",
      compare: "Reading the keyboard to compare",
    } satisfies Record<ReadPurpose, string>,
    readingProfile: {
      snapshot: (p: { n: number }) => `Reading profile ${p.n}`,
      safety_backup: (p: { n: number }) => `Reading profile ${p.n} for the safety backup`,
      backup: (p: { n: number }) => `Reading profile ${p.n}`,
      compare: (p: { n: number }) => `Reading profile ${p.n} to compare`,
    } satisfies Record<ReadPurpose, (p: { n: number }) => string>,
    selecting_profile: (p: { n: number }) => `Switching to profile ${p.n}`,
    returning_to_profile: (p: { n: number }) => `Returning to profile ${p.n}`,
    comparing: "Comparing with the backup",
    saving_safety_backup: "Saving the safety backup",
    saving_file: "Saving the backup file",
    writing: (p: { n: number }) => `Writing to profile ${p.n}`,
    waiting_for_keyboard_save: "Waiting for the keyboard to save",
    checking_persistence: (p: { n: number }) => `Checking that profile ${p.n} kept the changes`,
    final_check: "Final check",
    refreshing_view: "Reading the new values",
  },
  progress: {
    count: (p: { done: number; total: number }) => `${p.done}/${p.total}`,
    item: (p: { what: string; item: number; items: number }) => `${p.what} · ${p.item} of ${p.items}`,
    packet: (p: { packet: number; packets: number }) => `Packet ${p.packet} of ${p.packets}`,
    resend: "Sending it again: the first read-back differed.",
    keepPlugged:
      "Keep the keyboard plugged in and don't switch profiles on the keyboard until this finishes.",
    checkingOthers: (p: { seconds: string }) => `Listening for ${p.seconds} more before writing.`,
    waitProfile: (p: { n: number }) => `Waiting for the keyboard to save profile ${p.n}…`,
    waitGeneric: "Waiting for the keyboard to save…",
    waitExplain:
      "The keyboard stores a change about 1 second after receiving it; OpenWraith waits 2 seconds to be safe.",
    waitLeft: (p: { seconds: string }) => `${p.seconds} left`,
    steps: "Steps",
    done: "Done",
    current: "In progress",
  },
  cancel: {
    available: "Stop",
    requested: "Stopping after the current step…",
    finishing_keymap_group: "Finishing the key-assignment writes that belong together, so no key or macro is left half-changed…",
    not_now: "This step can't be stopped; it ends in a moment.",
    not_cancellable: "This can't be stopped once it has started.",
  } satisfies Record<CancelState, string>,
  review: {
    safetyNote: (p: { n: number; dir: string }) =>
      `A safety backup of profile ${p.n} will be saved to ${p.dir} first.`,
    safetyNoteGeneric: (p: { dir: string }) => `A safety backup will be saved to ${p.dir} first.`,
    vendorNote:
      "Make sure every wraith.software tab is closed. The site keeps talking to the keyboard even from a background tab.",
    vendorTick: "Every wraith.software tab is closed",
    vendorTickNeeded: "Confirm that the wraith.software tabs are closed.",
    firmwareNote: (p: { firmware: string }) =>
      `Firmware ${p.firmware} hasn't been tested with OpenWraith yet.`,
    saveNote:
      "Writing takes a few seconds, then OpenWraith waits about 2 seconds for the keyboard to save.",
    blocked: "Writing isn't possible right now:",
    invalidated: "This plan is out of date:",
    arming: "The Write button becomes active in a moment.",
  },
  nothingWritten: "Nothing was written.",
  nothingWrittenSwitches: (p: { n: number }) =>
    `Nothing was written (profile switches aside; the keyboard is on profile ${p.n}).`,
  nothingWrittenSwitchesUnknown: "Nothing was written (profile switches aside).",
  results: {
    persisted: "Written and verified · kept after a profile switch",
    written: "Written and verified",
    resentUnchanged: "Re-sent so the keyboard keeps the key assignments",
    alreadyMatched: "Already matched (not written)",
    maybeLost: "Written and verified, but may not have been saved",
    maybeLostBaseKeymap: "Will be lost at the next profile switch or power loss",
    lost: "LOST: the keyboard didn't keep it",
    failed: (p: { reason: string }) => `Failed: ${p.reason}`,
    notAttempted: "Not written",
  },
  headline: {
    doneKeys: (p: { n: number }) =>
      p.n === 1
        ? "Done. 1 key written and verified. The keyboard had time to save it."
        : `Done. ${p.n} keys written and verified. The keyboard had time to save them.`,
    doneLight: "The light was written and verified. The keyboard had time to save it.",
    done: "Done. Everything was written and verified. The keyboard had time to save it.",
    nothingToDo: "Nothing to change: the keyboard already has these settings.",
    cancelled: (p: { written: number; total: number }) =>
      `Stopped. ${p.written} of ${p.total} were written before stopping; the rest were not.`,
    notWritten: "Nothing was written.",
    failed: (p: { written: number; total: number }) =>
      `Writing stopped with an error. ${p.written} of ${p.total} were written and verified; the rest were not.`,
    maybeLost: (p: { n: number }) =>
      `Written and verified, but the keyboard may not have saved ${p.n} of them. Read the keyboard again to check, or undo.`,
    maybeLostOtherProfile: (p: { n: number; profile: number }) =>
      `Written and verified, but the keyboard may not have saved ${p.n} of them: it switched to another profile before saving. Your change was for profile ${p.profile}. Switch back to profile ${p.profile} and read the keyboard again to check, or undo.`,
    lost: (p: { n: number }) =>
      p.n === 1
        ? "The keyboard didn't keep 1 change. Write it again, or undo."
        : `The keyboard didn't keep ${p.n} changes. Write them again, or undo.`,
    waited: (p: { seconds: string }) => `OpenWraith waited ${p.seconds} for the keyboard to save.`,
  },
  tables: {
    profile: "Profile settings",
    main_light: "Main light",
    side_light: "Side light",
    active_custom_table: "Active per-key colour table",
    custom_colors: (p: { n: number }) => `Key colours (table ${p.n})`,
    keymap: { base: "Key assignments", fn: "Fn layer key assignments" },
    macro: (p: { n: number }) => `Macro ${p.n}`,
    advanced_key: (p: { n: number }) => `Advanced key ${p.n}`,
    triggers: "Actuation & Rapid Trigger",
    dead_band: "Dead band",
    socd_pairs: "SOCD pairs",
    socd_settings: "SOCD settings",
    socd: "SOCD",
    switch_types: "Switch types",
    poll_rate: "Polling rate",
    win_lock_and_rollover: "Win lock and rollover",
  },
  warnings: {
    ack_timeout: (p: { packet: number; of: number; send: number }) =>
      `Packet ${p.packet} of ${p.of} wasn't acknowledged in time (attempt ${p.send}).`,
    optional_ack_missing: (p: { packet: number }) =>
      `An optional acknowledgement for packet ${p.packet} didn't arrive.`,
    verify_reread: "The read-back needed a second read.",
    retried: "It was sent a second time because the first read-back differed.",
    select_ack_missing: (p: { profile: number }) =>
      `The keyboard didn't acknowledge switching to profile ${p.profile}.`,
    profile_push_missing: (p: { profile: number }) =>
      `The keyboard didn't report profile ${p.profile} after switching.`,
    select_retried: (p: { profile: number }) => `Switching to profile ${p.profile} was sent twice.`,
    waited_for_late_ack: "OpenWraith waited for a late acknowledgement.",
    waited_for_commit: (p: { seconds: string }) =>
      `OpenWraith first waited ${p.seconds} for the keyboard to save an earlier change.`,
    base_keymap_not_saved:
      "The key assignments were written, but the step that makes the keyboard save them didn't complete.",
    unexpected_ack: (p: { packet: number }) =>
      `The keyboard acknowledged packet ${p.packet}, which it usually doesn't.`,
    socd_toggled_on_keyboard: (p: { on: boolean }) =>
      `SOCD was switched on the keyboard (Fn + Right Shift) during the write or right after it, so that switch may have been overwritten. SOCD is now ${p.on ? "on" : "off"}.`,
  } satisfies Record<Code, unknown>,
  commitTrigger: {
    base_keymap: "Fn layer key assignments: re-sent unchanged so the keyboard keeps the key assignments",
    advanced_keys:
      "Fn layer key assignments: re-sent unchanged before the advanced keys so the keyboard keeps them (it loses advanced keys written without them)",
  } satisfies Record<CommitTriggerReasonView, string>,
  /** Why a macro is written again although it doesn't change (slots shown 1-based). */
  resent: {
    macro_slot0_clears: "Unchanged, written again: writing Macro 1 makes the keyboard clear Macros 2–10.",
  } satisfies Record<ResendReasonView, string>,
  footer: {
    safetyBackup: "Safety backup:",
    undoDisabled: "Connect the keyboard to undo.",
    activeProfile: (p: { n: number }) => `The keyboard is on profile ${p.n}.`,
    activeProfileWhenLost: (p: { n: number }) => `The keyboard was on profile ${p.n} when the connection was lost.`,
    activeProfileUnknown: "The active profile isn't known now.",
    editedProfile: (p: { n: number }) => `This change was for profile ${p.n}.`,
    foreignActivity:
      "Another program talked to the keyboard during this operation. Close every wraith.software tab.",
  },
  /** What an edit report adds after the headline (key-assignment edits and every other edit). */
  report: {
    cancelRequested:
      "You pressed Stop, but every remaining write was needed so that no key or macro was left half-changed, so everything planned was written.",
    clearedTitle: "Macros left empty",
    clearedBody:
      "Writing Macro 1 makes the keyboard empty Macros 2–10, and the edit stopped before writing them again. Undo brings them back.",
    clearedItem: (p: { n: number }) => `Macro ${p.n}`,
    danglingTitle: "Keys that now do nothing",
    danglingBody: "These keys refer to an empty macro or advanced-key slot. Undo brings the slot back.",
    danglingItem: (p: { key: string; target: string }) => `${p.key} → ${p.target}`,
    persistenceTitle: "Check that the keyboard kept it",
    persistenceOk: (p: { n: number }) =>
      p.n === 1
        ? "Saved: checked by switching to another profile and back. The change was still there."
        : `Saved: checked by switching to another profile and back. All ${p.n} changes were still there.`,
    persistenceLost: (p: { lost: number }) =>
      p.lost === 1 ? "1 change was gone after switching back: LOST." : `${p.lost} changes were gone after switching back: LOST.`,
    persistenceNotChecked: (p: { n: number }) =>
      p.n === 1 ? "1 change couldn't be checked separately." : `${p.n} changes couldn't be checked separately.`,
  },
  /** Why a write of a key-assignment edit is in the plan (from the plan's own fields). */
  writeReasons: {
    baseKeymapUnchanged:
      "Sent unchanged right after the key assignments: the keyboard keeps changed key assignments only when a complete Fn layer follows them.",
    baseKeymapChanged: "Written right after the key assignments, so the keyboard keeps them.",
    advancedKeysUnchanged:
      "Sent unchanged before the advanced keys: the keyboard keeps advanced keys only when a complete Fn layer comes before them.",
    advancedKeysChanged: "Written before the advanced keys, so the keyboard keeps them.",
    completesBinding: "Written even if you press Stop: a key written before refers to it.",
    followsMacroSlot0: "Written after Macro 1, which makes the keyboard empty Macros 2–10.",
    clears: "Empties this slot.",
  },
  items: {
    title: "Details",
    sends: (p: { n: number }) => (p.n === 1 ? "Sent once" : `Sent ${p.n} times`),
    offsets: (p: { list: string }) => `Differing bytes: ${p.list}`,
    expected: "Expected",
    got: "Read back",
  },
} as const;
