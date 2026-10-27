// Error texts: one title and body per error kind, the detail lines, write
// block reasons, plan invalidation reasons and other shared enums.

import type {
  AppErrorKind,
  IntentField,
  MismatchKindView,
  PlanInvalidation,
  RangeField,
  WriteBlockReason,
} from "../../ipc/bindings";

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

export const errors = {
  kind: {
    not_found: {
      title: "Keyboard not found",
      body: "OpenWraith can't find the keyboard. Check the USB cable.",
    },
    busy: {
      title: "The keyboard couldn't be opened",
      body: "Another program may be holding it. Close other keyboard tools and try again.",
    },
    other_program_active: {
      title: "Another program is using the keyboard",
      body: "This is probably the wraith.software site. Close that browser tab completely, then check again.",
    },
    timeout: {
      title: "The keyboard didn't answer in time",
      body: "Check the cable and try again.",
    },
    keyboard_reset: {
      title: "The keyboard reset itself",
      body: "It reported a factory reset (Fn + Space). OpenWraith stopped.",
    },
    disconnected: {
      title: "The keyboard was disconnected",
      body: "Plug it in again; OpenWraith reconnects automatically.",
    },
    profile_changed: {
      title: "The profile changed on the keyboard",
      body: "Profiles were switched on the keyboard (Fn + 1–4) during the operation, so OpenWraith stopped.",
    },
    changed_since_plan: {
      title: "The keyboard's settings changed",
      body: "They changed after you reviewed the change. Review it again.",
    },
    verify_failed: {
      title: "The keyboard didn't accept the change",
      body: "After writing, the keyboard returned different values, even after a second try.",
    },
    not_persisted: {
      title: "The keyboard didn't keep the change",
      body: "It was written, but it was gone after a profile switch.",
    },
    model_mismatch: {
      title: "This backup is for another keyboard",
      body: "OpenWraith restores a backup only to the same model.",
    },
    firmware_mismatch: {
      title: "The backup was made with different firmware",
      body: "Settings may be stored differently in another firmware version. You can restore anyway if you are sure.",
    },
    invalid_backup: {
      title: "The backup file can't be used",
      body: "It is damaged or isn't an OpenWraith backup.",
    },
    cancelled: {
      title: "Stopped",
      body: "The operation was stopped as you asked.",
    },
    io: {
      title: "A file couldn't be read or written",
      body: "Check that the folder exists and that you may write to it.",
    },
    unexpected_reply: {
      title: "The keyboard gave an unexpected answer",
      body: "OpenWraith stopped to stay on the safe side.",
    },
    invalid_request: {
      title: "This change isn't possible",
      body: "Check the values and try again.",
    },
    not_connected: {
      title: "No keyboard connected",
      body: "Connect the keyboard and wait until it has been read.",
    },
    operation_running: {
      title: "Another operation is running",
      body: "Wait until it has finished.",
    },
    stale_plan: {
      title: "The keyboard's settings changed",
      body: "The keyboard was read again. Check your change and review it again.",
    },
    needs_switch_consent: {
      title: "Comparing needs profile switches",
      body: "Confirm the profile switches first.",
    },
    writes_blocked: {
      title: "Writing is paused",
      body: "OpenWraith can't write right now.",
    },
    safety_dir_unavailable: {
      title: "The safety-backup folder can't be used",
      body: "OpenWraith can't save a safety backup, so it doesn't write.",
    },
    file_unknown: {
      title: "The file isn't available any more",
      body: "Open it again.",
    },
    internal: {
      title: "Something went wrong inside OpenWraith",
      body: "If this keeps happening, report it with the technical details.",
    },
    travel_test_open: {
      title: "A key-depth test is still running",
      body: "The keyboard may still be sending key depths. Stop the test first, then try again.",
    },
  } satisfies Record<AppErrorKind, { title: string; body: string }>,
  detail: {
    other_program: (p: { status: number; acks: number; profiles: number }) =>
      `OpenWraith received ${evidenceText(p)} that it didn't ask for.`,
    profile_changed: (p: { expected: number; reported: number }) =>
      `OpenWraith expected profile ${p.expected}; the keyboard reported profile ${p.reported}.`,
    profile_changed_reported: (p: { reported: number }) =>
      `The keyboard reported profile ${p.reported}.`,
    profile_changed_before_save: (p: { reported: number }) =>
      `The keyboard switched to profile ${p.reported} before it had saved the change.`,
    firmware_mismatch: (p: { backup: string; keyboard: string }) =>
      `Backup: firmware ${p.backup}. Keyboard: firmware ${p.keyboard}.`,
    model_mismatch: (p: { backup: string; keyboard: string }) =>
      `Backup: ${p.backup}. Keyboard: ${p.keyboard}.`,
    unsupported_model: (p: { product: string }) => `The keyboard reports itself as ${p.product}.`,
    profile_not_in_backup: (p: { n: number }) => `Profile ${p.n} isn't in this backup.`,
    active_profile_now: (p: { n: number }) => `The keyboard is on profile ${p.n} now.`,
    active_profile_unknown: "The active profile isn't known now.",
    key_not_editable: (p: { key: string; reason: string }) => `${p.key}: ${p.reason}`,
    value_out_of_range: (p: { field: string; value: string; min: string; max: string }) =>
      `${p.field} ${p.value} is outside the allowed range (${p.min} to ${p.max}).`,
    distance_unit: (p: { um: number }) =>
      `The keyboard reports a distance unit of ${p.um} µm; OpenWraith can only write in steps of 0.02 mm.`,
    distance_unit_unknown: "The keyboard's distance unit isn't known; OpenWraith only writes in steps of 0.02 mm.",
    needs_switch_consent: (p: { list: string }) => `Comparing switches to profiles ${p.list}.`,
    verify_failed: (p: { table: string; sends: number }) =>
      `${p.table}: the read-back differed after ${p.sends} attempts.`,
    commit_trigger_incomplete: (p: { packet: number; of: number }) =>
      `The step that makes the keyboard save stopped at packet ${p.packet} of ${p.of}.`,
    io: (p: { file: string }) => `File: ${p.file}`,
    unsupported_schema: (p: { found: number; supported: number }) =>
      `This backup was made by a newer OpenWraith (format ${p.found}; this version reads up to format ${p.supported}).`,
  },
  writeBlocked: {
    not_connected: "No keyboard is connected and read",
    observing: "OpenWraith is still checking for other programs",
    busy: "Another operation is running",
    other_program: "Another program is using the keyboard",
    reset_seen: "The keyboard reset itself; read it again first",
    snapshot_stale: "The values shown may be out of date; read the keyboard again",
    safety_dir_unavailable: "The safety-backup folder can't be used",
    fault: "OpenWraith hit an internal error; restart it",
  } satisfies Record<WriteBlockReason, string>,
  planInvalidation: {
    superseded: "A newer plan replaced it.",
    expired: "It is more than 10 minutes old.",
    unknown: "OpenWraith doesn't know this plan any more.",
    disconnected: "The keyboard was disconnected.",
    reset_seen: "The keyboard reset itself.",
    other_program: "Another program used the keyboard.",
    profile_changed: "The profile changed on the keyboard.",
    values_changed: "The keyboard's values changed.",
    snapshot_changed: "The keyboard was read again.",
    operation: "Another operation ran in between.",
    resubscribed: "The window was reloaded.",
    discarded: "It was cancelled.",
  } satisfies Record<PlanInvalidation, string>,
  notEditable: {
    knob: "The knob has no trigger settings.",
    not_read: "It wasn't read from the keyboard.",
    undecodable: "Its stored value couldn't be decoded.",
    unknown_mode: (p: { mode: string }) => `It uses an unknown trigger mode (${p.mode}).`,
    second_travel_differs: "It uses a second actuation travel that OpenWraith doesn't change.",
    press_release_differ: "Its Rapid Trigger press and release values differ where they should match.",
  },
  rangeField: {
    actuation: "Actuation point",
    sensitivity: "Sensitivity",
    press: "Press sensitivity",
    release: "Release sensitivity",
    brightness: "Brightness",
    speed: "Speed",
  } satisfies Record<RangeField, string>,
  intentField: {
    keys: "No keys were selected.",
    knob: "The knob has no trigger settings.",
    duplicate_key: "A key was listed twice.",
    nothing_to_change: "There is nothing to change.",
    mode: "This effect can't be chosen.",
    color: "The colour isn't valid.",
    profiles: "Choose at least one profile from the backup.",
    source: "The backup file isn't available any more.",
  } satisfies Record<IntentField, string>,
  mismatchKind: {
    missing_reply: "No reply",
    invalid_reply: "Invalid reply",
    differs: "Different values",
  } satisfies Record<MismatchKindView, string>,
} as const;
