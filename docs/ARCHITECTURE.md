# OpenWraith architecture

This document explains how the code is organised, how read-only code paths
are kept read-only, how the guarded write path (Phase 2) works, how data flows
from the keyboard to a backup file and back, and how to add another keyboard
model. The protocol itself is described in [PROTOCOL.md](PROTOCOL.md);
section numbers below ("spec 2.8") refer to it.

## Crate layout

```
openwraith-cli  (bin "openwraith")      commands, text/JSON output, prompts
      |
openwraith-hid                          transports, session, backup, writer,
      |                                 restore, edits, safety backups,
      |                                 mock + simulator (feature "mock")
openwraith-protocol                     pure protocol: no I/O
```

A future Tauri app will sit next to the CLI and use the same two libraries.

### `openwraith-protocol` (no I/O)

| Module | Contents |
|---|---|
| `frame` | `REPORT_ID`, `REPORT_LEN` (63), `MAX_CHUNK_PAYLOAD` (58). `OutPacket` (no public constructor; read requests only) and `Report` (any 63-byte input report; tolerates byte-4 quirks and stale bytes, spec 2.3 / 2.4). |
| `request` | `ReadRequest`: one variant per read command of the spec, all CONFIRMED. Each knows its packet bytes, its `ResponseSpec` (how replies are recognised, spec 2.3 / 2.8), its chunk layout and its timeout. `full_read_plan` lists everything a backup reads. |
| `response` | `ResponseCollector` (accepts matching chunks, ignores the rest, flags a second copy of a chunk with different data as `Offer::Conflict`, detects completion) and `RawResponse` (a complete, validated reply; the lossless unit of a backup). |
| `decode` | Typed models for every reply (`DeviceInfo`, `Status`, `LightEffect`, `ColorTable`, `KeymapTable`, `MacroSlot`, `KeyTrigger`, ...). Every model keeps its raw bytes. Field docs repeat the spec's confidence labels. |
| `write` | The nine confirmed write forms (W1-W9 below). `WritePacket` (no public constructor, separate from `OutPacket`), `WriteRequest` (Serialize only) with `steps()` (packets, acknowledgement spec, pauses), `verify_reads()` and `check()`, `AckSpec` / `AckTracker`, `TableId`, and the validated table data built from raw replies: `KeymapData`, `MacroData` (canonical flat layout only), `ColorTableData` (custom table 1 only), `DeadBandData` / `DeadBandWrite` (range-checked), `LightRecord` / `LightLimits` / `MainLightWrite`, `TriggerSetting` / `TriggerLimits` / `TriggerWrite` with the closed `TriggerWriteMode` (`00`, `0C`, `0D`; no `From<u8>`). `ProfileId`, `AdvancedSlotId`. |
| `event` | Classification of unsolicited reports: keyboard reset (`14 .. 01 01`), profile reports, status pushes. |
| `layout`, `model` | Key positions (`KeyPos`, index = row x 22 + col), key masks, the W75 key table and the `Model` description (spec 5), including its `WriteCeilings` (absolute limits for written values). |
| `units` | `Distance` in trigger units of 0.02 mm. |
| `capture` (feature) | Parser for the JSON capture files and the request/reply pairing used by tests and the mock transport. |

### `openwraith-hid`

| Module | Contents |
|---|---|
| `transport` | `Transport` trait: `write(&OutPacket)`, `read(timeout) -> Option<Report>`, `usb_info()`, `now()` (the clock every timeout is measured on: the system clock, or the simulator's virtual clock). `WriteTransport: Transport` adds `write_command(&WritePacket)`. |
| `hid` | `HidBackend` (hidapi; enumerates and filters supported interfaces, nothing is opened), `HidTransport` (read-only) and `WritableHidTransport` (opened with `HidBackend::open_writable`). |
| `session` | `Session<T: Transport>`: one request at a time, reply collection, filtering, timeout and retry, conflict detection, reset detection, retry of reads interrupted by a signal (`TransportError::Interrupted`, Ctrl+C on Linux), and the classification of unrelated reports (status reports, write acknowledgements and profile reports of another program: `foreign_activity()`; `listen()`; `rearm_gate()` starts the writer's gate afresh in a long-lived session). `last_write_at()` / `last_write_profile()`: when and to which profile the last write that changes a setting was sent (for the commit settle; kept per session, so it spans writers). |
| `summary` | `read_summary` / `DeviceSummary`: the few reads behind `openwraith info` and `openwraith profile`, with the model check shared with `read_all`. |
| `backup` | `Backup` (file format, schema 1 and 2), `SnapshotView`, `read_all` / `read_all_with` / `read_all_cancellable`, save/load, typed accessors. |
| `writer` | `WriteConsent`, `WriteOptions` (with `CommitSettle`), the "other program" gate in `Session::writer`, and `Writer`: `write_guarded` (the only public table write: needs a `SavedBackup`) and `write_commit_trigger` (the same guards, for a `CommitTrigger`, the Fn keymap only, sent even when the keyboard holds it: the keyboard keeps a base keymap only when the Fn keymap is written after it; every packet must be acknowledged, else one more send of the whole table, then `WriteError::CommitTriggerIncomplete`), `base_keymap_unsaved()` (a base keymap sent without a complete Fn-keymap write after it; a profile switch then reports `WriteWarning::BaseKeymapNotSaved`), stop-and-wait writes with read-back verification, profile selection behind the commit settle (`select_profile[_observed]`, `commit_pending`, `wait_for_commit[_observed]`, `CommitWait`), detection of a profile change or lost connection before the keyboard saved (`commit_interrupted`, `WriteError::ProfileChangedBeforeSave`), poisoning after a reset or a lost connection. |
| `profiles` | `read_profiles` / `read_all_profiles`: the all-profile backup (switches profiles and back). |
| `safety` | `default_dir`, `Backup::save_safety` and `SavedBackup` (the proof that a verified safety backup exists). |
| `restore` | `plan_restore` (pure), `dry_run_restore` (read-only), `prepare_restore`, `execute_restore` (with the persistence check), and the serialisable plan, progress and report types (`Persistence`, `PersistenceCheck`, `ItemResult::Lost`, `ItemResult::MaybeLost` (also a base keymap left without its Fn keymap, with `WriteWarning::BaseKeymapNotSaved`), `CommitTriggerReason` on the Fn-keymap write that makes the keyboard save the keymap group). |
| `edit` | `plan_trigger_edit` / `apply_trigger_edit` and `plan_light_edit` / `apply_light_edit`. |
| `error` | Error types and `ErrorKind` (serialisable, for a GUI). |
| `mock` (feature `mock`) | `MockTransport`: replays the captures; can add noise, inject unsolicited reports and drop replies. Read-only. |
| `sim` (feature `mock`) | `DeviceSim`: a stateful W75 with four profiles, an independent parser of the nine write forms, configurable table scopes, fault injection, a virtual clock (reads "wait" without sleeping; reports and acks can be scheduled at a virtual time; `sent_times()` gives the time of every packet) and the keyboard's **delayed commit** (writes go to a working copy of the active profile, saved 1 s of virtual time after the last write; a select or `power_cycle()` before that discards them, counted by `discarded_edits()`; `persisted_table` / `persisted_trigger` show the saved state; on by default), the **keymap commit rule** (`KeymapCommit`, on by default: the timer never saves base-keymap edits on their own; a complete `18 02` write, every chunk applied, releases them to it, so base and Fn keymap are saved 1 s after the last `18 02` chunk and a switch before that loses both; a partial `18 02` releases nothing; `has_edits_waiting_for_fn_write()`; `KeymapCommit::macros_and_advanced_need_fn_write` (off by default; an untested guess, the tests run both ways) makes macro and advanced-key edits wait for the `18 02` too), and profile switches made on the keyboard at a virtual time, with or without a profile report (`switch_profile_at`; the profile is reloaded as of that time). |

### `openwraith-cli`

| Module | Contents |
|---|---|
| `main` | clap definitions, the read-only commands (`list`, `info`, `show`, `backup`), `--from` / `--profile` for backup files, friendly error messages with hints, exit codes. |
| `args` | Arguments of `profile select`, `set` and `restore`, and the value parsers: key names (`W`, `"Caps Lock"`, `#46`, `r2c2`; the knob is refused), millimetres (decimal text to micrometres, no floats, multiples of 0.02 mm), Rapid Trigger values, colours, light modes. |
| `commands` | The commands that may write (`profile [select]`, `backup --all-profiles`, `set trigger`, `set light`, `restore`), generic over a `Connector` and a `Ui`. |
| `device` | Choosing and opening the keyboard (`--device`, several-keyboard handling); the `Connector` trait and `HidConnector`. |
| `ui` | The `Ui` trait (output, progress, the `[y/N]` question, cancellation) and `Terminal` (stdout/stderr/stdin, Ctrl+C handler). |
| `view` | Serialisable views over a `Backup` or a `DeviceSummary` (used for both text and `--json`); `JsonOutput` adds the confidence `note` and the list of undecoded replies to every JSON output. |
| `render`, `report`, `table` | Text output: settings tables (`render`), restore plans, restore and edit reports (`report`). Values whose meaning the spec labels CODE-ONLY or GUESS are marked `*`, with one legend line per command. Text from the device or a backup file is stripped of control characters before it is printed. |

## Read-only paths

Code that only reads must not be able to send anything that changes the
keyboard. This is enforced by types, not by convention:

1. **Read code can only send `OutPacket`s.** `Transport::write` takes an
   `&OutPacket` and nothing else. `OutPacket` has no public constructor; the
   only way to get one is `ReadRequest::packet()`. A `compile_fail` doctest
   checks that `OutPacket([0x14; 63])` does not compile.
2. **`ReadRequest` contains only reads.** Its variants are the read commands
   labelled CONFIRMED in spec section 3. Parameters are range-checked types
   (`KeyPos`, `MacroSlotId`), so an invalid request cannot be built. A unit
   test enumerates all 163 constructible requests and checks each packet
   against the documented read forms (no `0x14`, no `0x17`, no write
   sub-commands, byte 6 of the poll-rate read stays 0, ...).
3. **Writing needs a separate trait.** Sending a `WritePacket` needs
   `WriteTransport`. `Transport` has no method that could write, so
   code that is generic over `Transport` (`read_all`, `read_summary`,
   `dry_run_restore`, `plan_trigger_edit`, `plan_light_edit`) provably
   cannot write. `Session::writer` exists only for `Session<T:
   WriteTransport>`; a `compile_fail` doctest checks that
   `Session<HidTransport>` has none.
4. **Read-only commands open a read-only transport.** `list`, `info`,
   `show`, `backup`, `profile` and `restore --dry-run` open `HidTransport`,
   which does not implement `WriteTransport`. Only `profile select`, `set`,
   `restore` and `backup --all-profiles` open `WritableHidTransport`.
5. **No raw access.** The hidapi handle is private, and no API in the
   workspace accepts raw bytes for sending. Captured packets are kept as plain
   byte arrays precisely because they can be writes.
6. **Reset notifications are never answered.** When the keyboard sends
   `14 .. 01 01` (Fn + Space held, spec 4.6.3), the session returns
   `SessionError::KeyboardReset` and is marked: every later write call fails
   without sending anything. Tests inject the real notification from capture
   c07 and assert that no further packet was written.
7. **Tests check what was sent.** The hid tests assert that every packet the
   mock received during a full backup is a read request and never a `0x14`;
   the CLI tests assert the same for `restore --dry-run` against the
   simulator.

## The guarded write path (Phase 2)

### What can be written

Only these nine forms, all CONFIRMED on hardware (byte layouts in the
`write` module documentation and spec section 3):

| # | Write | Packets | Acknowledgement |
|---|---|---|---|
| W1 | `21/12` select profile 1-4 | 1 | `21 .. 02 12` and the push `21 .. 03 11 PP 00` |
| W2 | `17 00 .. 02 00 00` music-rhythm leave | 1, only right before a mode-changing W3 | `17 00 00 00 00` (optional) |
| W3 | `07 00` main light | 1 | none |
| W4 | `09 00` custom colour table 1 | 8 | `09 00 00 00 00` each |
| W5 | `18 00` / `18 02` keymap base / Fn | 10 | `18 00 00 00 00` each |
| W6 | `19 0n` macro slot | 5 | `19 00 00 00 00` each |
| W7 | `22 SS` advanced-key slot, **all-zero content only** | 1 | `22 00 00 00 00` |
| W8 | `21 .. 18 MM` trigger, MM in `00`/`0C`/`0D`, **one key per packet** | 1 | `21 .. 01 00` |
| W9 | `26 01` dead band | 5 | `26 01 00 00 00` each |

`WritePacket` has no public constructor and no conversion from `OutPacket`
or bytes; `WriteRequest::steps()` is the only way to get one. W2 is not a
variant of its own: it can only appear as the first step of a main-light
write that changes the mode. Every other output packet (`0x14` resets and
Win-Lock, `17` enter, `08 00`, `09 01`, `24`, `25 01`, `21/06`, calibration,
travel test, poll-rate set, the vendor site's empty-mask trigger write) is
impossible to construct. A protocol test checks every constructible request
(a seeded sample of 2,316 packets) against an independent predicate written
from the table above, and checks that no write packet parses as a read.

Table builders take validated raw replies, never decoded models: a restore
writes the backup's own bytes, and an edit changes exactly the bytes it was
asked to change in a table read from the keyboard.

Values are checked when a request is built: `TriggerWrite::new` (keys and
travel values), `MainLightWrite::new` (mode, brightness, speed, ...) and
`DeadBandWrite::new` (every top and bottom value). The limits come from the
keyboard's replies (`21/04`, `0A`) through `TriggerLimits::new(model, ..)` and
`LightLimits::new(model, ..)`, and are always capped by the model's
compiled-in `WriteCeilings` (W75: actuation 5-170 units, Rapid Trigger
sensitivity up to 85, dead band up to 50, brightness and speed up to 4, the
known light modes). The ceilings and the selectable keys are taken from the
model in `MODELS` with the given product code, never from the `Model` a caller
passes (its fields are public), and an unknown product is refused. The
protocol crate validates framing and ranges, not authenticity (anyone can
build a `RawResponse`), so the ceilings are what bounds a forged reply or
fabricated decoded limits.

### Consent, the "other program" gate and the writer

```
Session<T: WriteTransport>::writer(WriteConsent)
  1. drain and classify every queued report
  2. listen until the keyboard was observed for at least 2 s since the
     session opened (or since the last rearm_gate)
  3. refuse if the session saw, in that time, status reports it did not
     request (the vendor site's heartbeat replies), write acknowledgements it
     did not cause or profile reports it did not cause:
     WriteError::ProfileChanged when the only evidence is a report of another
     profile than the one last read (a switch on the keyboard or by another
     program), else WriteError::OtherProgramActive
  -> Writer
```

The evidence sticks for the session: the CLI opens a new session per
command, and a long-lived session (a GUI) calls `Session::rearm_gate` after
the user confirmed that the vendor site is closed, which needs another full
observation period before the next writer.

`WriteConsent` records how the user agreed (`interactive_prompt`,
`command_line_flag`, `application`); it is written into every report.

`Writer::write_guarded(&SavedBackup, &WriteRequest)` is the only public way
to write a table (the unguarded primitive is crate-private; a `compile_fail`
doctest checks it):

1. refuses after foreign activity; checks that the `SavedBackup` belongs to
   this keyboard model (`0D`, read once per writer) and holds the active
   profile, confirmed with `21/11` (`WriteError::NotBackedUp` otherwise);
2. re-reads exactly the bytes the write changes; when the keyboard already
   holds them nothing is sent (`AlreadyMatched`);
3. rebuilds a main-light write from a fresh `0A` + `07` read right before it
   is sent, so W2 precedes W3 exactly when the keyboard's *current* mode
   changes;
4. waits, before the first packet, for a late acknowledgement of an earlier
   write that looks exactly like this table's acknowledgements and is still
   expected (at most its 2 s window), so it cannot be taken for the first
   packet's ack;
5. sends one packet at a time and waits up to 500 ms for its
   acknowledgement (acks carry no chunk index, so only stop-and-wait can
   match them). On a timeout the rest of the table is not sent; a single chunk
   is never sent again. The pauses after W3 (20 ms) and each W4 chunk (5 ms)
   come from the write itself (`WriteStep::gap_after`);
6. verifies by reading the table back (`verify_reads` + `check`); on a
   mismatch reads once more after 100 ms;
7. if it still differs, listens 200 ms, rebuilds the request from a fresh
   read where the bytes depend on it (the main light's W2), sends the
   **whole** table once more and verifies again; otherwise
   `WriteError::VerifyFailed`;
8. counts sends per (profile, table): a third send is refused
   (`RepeatedWrite`), which stops loops and floods.

Restores and edits use the same steps 3-8 through a crate-private guarded
write that additionally refuses a table whose bytes changed since the plan.

The one exception to step 2 is the **keymap commit trigger**. The keyboard
keeps a base-keymap write (`18 00`) only when an Fn-keymap write (`18 02`)
follows it; written alone, the base keymap reads back correctly but is gone
after a profile switch, even after the commit settle (CONFIRMED on hardware,
[PROTOCOL.md](PROTOCOL.md) 4.8.2). So the Fn keymap must be sent after the
base keymap even when the keyboard already holds it.
`Writer::write_commit_trigger(&SavedBackup, &CommitTrigger)` does that with
every other guard (steps 1 and 4-8: backup and active-profile check,
stop-and-wait, verification, the `RepeatedWrite` count). A `CommitTrigger`
can only hold an Fn-keymap write: `CommitTrigger::fn_keymap(KeymapData)`
builds one, and `TryFrom<WriteRequest>` gives every other request back
(tested for every request kind), so it is no way to send anything else or to
skip a check. The restore's crate-private guarded write takes the same type
(`Checked::CommitTrigger`).

A commit trigger is checked more strictly than other writes in one respect.
An unchanged Fn keymap reads back the same whether it arrived or not, and
whether a partial `18 02` makes the keyboard save is unknown (every probe
and every vendor-site Apply sent all ten chunks). So for a commit trigger an
acknowledgement timeout in step 5 counts as a failed verification: the whole
table is sent once more (step 7, still counted by `RepeatedWrite`), and if
that send is cut short too the write fails with
`WriteError::CommitTriggerIncomplete`. The flag lives in a private `write_with`
behind the crate-private `write_trigger_observed(&CommitTrigger, ..)`, so only
a `CommitTrigger` can ask for it.

The writer also remembers a base keymap it sent without a complete
Fn-keymap write after it (`Writer::base_keymap_unsaved()`, set by any `18 00`
packet and cleared by a verified Fn-keymap write whose packets were all
acknowledged). A profile switch made while it is set adds
`WriteWarning::BaseKeymapNotSaved` to the `SelectOutcome` and clears it. The
switch is not refused: a failing operation must still be able to return to
the original profile. So a caller of the public API that forgets the trigger
is told, instead of seeing a silent success.

`Writer::select_profile` sends nothing when the profile is already active;
otherwise it waits for the commit settle (below), sends W1, waits for the
acknowledgement and the push (reading the push before any `21/11` query),
confirms with `21/11` and retries once.

During an operation an unexpected profile report or a reset notification
aborts; a foreign acknowledgement or heartbeat stops the operation after the
current table. After a reset notification or a transport failure the writer
is *poisoned*: every later call fails without sending anything, not even a
switch back to the original profile.

### Commit settle and persistence

The keyboard keeps edits in a working copy of the active profile and saves
them about 1 s after the last write. A profile switch before that reloads the
profile from what it saved, and the edits are lost; a read-back cannot tell,
because it reads the working copy. This was found on hardware: a restore of
profile 2 reported "written and verified" and switched back to profile 1
within milliseconds, and the write was gone (CONFIRMED, measured threshold
between 0.8 s and 1.0 s; [PROTOCOL.md](PROTOCOL.md) section 4.5.3, "Delayed
commit"). A power loss in that second very likely loses the write too
(untested). Four rules follow, all in `openwraith-hid`, and a fifth for the
base keymap, which the keyboard does not save on a timer at all:

1. **No switch before the save.** `Session` records when the last write
   packet that changes a setting was sent (`last_write_at`, on the
   transport clock; profile selects do not count). `Writer::select_profile`
   does not send `21/12` before `WriteOptions::commit_settle` (2 s, twice
   the measured threshold) has passed since then: it listens for the rest,
   so foreign reports, profile reports and reset notifications are still
   classified (a reset poisons the writer as always), and it reports the
   wait (`CommitWait` to `select_profile_observed`'s callback,
   `WriteWarning::WaitedForCommit` in the outcome,
   `WriteProgress::WaitingForCommit` / `ProfilesProgress::WaitingForCommit`
   to the operations' progress). Every profile switch of the crate goes
   through it: restore (including its return to the original profile and its
   checks), the all-profile backup and its return after a failure, edits,
   and the CLI's `profile select`. The time is kept by the session, so a
   long-lived session (a GUI) is covered across writers; a session that
   wrote nothing never waits.
2. **Nothing returns before the save.** `execute_restore`,
   `apply_trigger_edit` and `apply_light_edit` call
   `Writer::wait_for_commit` before they return (also after a failure or a
   cancellation, unless the writer is poisoned), so a finished operation
   means the keyboard had time to save. `write_guarded` returns right after
   the verification (several writes may follow); its callers must call
   `wait_for_commit` before reporting a change as done.
3. **A restore proves it.** After every planned write was made, the
   persistence check selects every profile with written tables again after
   switching away from it (reads after a switch come from what the keyboard
   saved) and reads the written tables again: the written profiles other
   than the original one first (ascending, the one that is active last),
   the original one last, since the restore ends there. When the profile to
   check is the active one, it first switches to the original profile (or,
   for the original itself, to the lowest other profile). Each item is
   reported `written` with `persistence: persisted`, or `lost`
   (`ItemResult::Lost`, with the difference read after the switch); a lost
   write fails the restore with `ErrorKind::NotPersisted` and the message
   says to run the restore again or undo it. A table whose profile scope is
   unknown that holds what this restore wrote to it in another profile is
   left `not_checked` (the final check compares it with the backup;
   `PersistenceCheck::not_checked` counts them).
4. **A switch before the save is never reported as done.** The keyboard
   can be switched on its own keys (or by another program) during the
   settle, and whether it reports such a switch is open (PROTOCOL.md Q2).
   So: a profile report during a wait ends the wait at once
   (`Session::listen_until` stops at it); every wait ends with a `21/11`
   read, and another profile than the one the last write went to
   (`Session::last_write_profile`) fails it; a `21/11` read
   (`Writer::active_profile`, so every table's profile check) that finds
   another profile while the settle has not passed fails too. These fail
   with `WriteError::ProfileChangedBeforeSave` (kind `profile_changed`; the
   message says that the write may not have been saved), mark the pending
   write as discarded (nothing waits for it any more; the switch back to
   the original profile goes out at once) and set
   `Writer::commit_interrupted`, which is also true when the connection was
   lost before the settle had passed (an unplug very likely loses unsaved
   edits). Restores and edits then report the written, unconfirmed tables of
   that profile as `ItemResult::MaybeLost` ("written and verified, but may
   not have been saved"), not `written`, and the known active profile is
   the one the keyboard reported. A switch found at the end of a wait counts
   as made before the save, because its time is unknown. A reset
   notification is not counted: it reports a reset of the settings.
5. **The base keymap needs the Fn keymap after it (commit trigger).** The
   settle is not enough for the base keymap: written alone (`18 00`), it
   was gone after a profile switch even 6 s later; followed by an `18 02`,
   both layers were kept, and `18 02`, the dead band, triggers, the main
   light and custom colours 1 are kept on their own (CONFIRMED on hardware
   2026-10-24, [PROTOCOL.md](PROTOCOL.md) 4.8.2; this is how hardware test
   H9 lost its write). A restore therefore writes the keymap group in the
   vendor site's order: every macro slot that changes (content and
   clears), the base keymap, the Fn keymap right after it, the
   advanced-key clears. Whenever the base keymap is written the Fn keymap
   follows, re-sent unchanged with the backup's Fn table if it already
   matches (`CommitTriggerReason::BaseKeymap`; shown as "Fn keymap (18 02)
   re-sent unchanged to make the keyboard save the base keymap"). Whether
   macro (`19`) and advanced-key (`22`) slots need the trigger too is
   untested, so as a precaution (GUESS) a group that writes such slots
   while neither keymap layer differs ends with the unchanged Fn keymap
   (`CommitTriggerReason::MacrosOrAdvancedKeys`). If the Fn keymap cannot be
   written (an unusable reply), the whole group is skipped
   (`BlockCause::FnKeymapNotWritable`, or `Incomplete` when the Fn keymap is
   missing): the base keymap is never written alone. The Fn-keymap write
   must arrive complete: every packet acknowledged, else the whole table
   is sent once more (see the write gate above). A Ctrl+C after a group
   table was sent skips the group's other tables (they stay
   `NotAttempted`, a not-yet-written base keymap included), still sends
   the Fn keymap (an `18 02` alone is kept), and stops after it; when that
   Fn keymap was the last planned write, the restore did complete and
   ends normally, persistence check included. If the restore stops between
   the base keymap and the Fn keymap for another reason (another program,
   an incomplete trigger, a failure), the base keymap is reported
   `MaybeLost` with the warning `WriteWarning::BaseKeymapNotSaved` ("will
   be lost" in the CLI), not as written. The persistence check stays the
   safety net: it reads the base keymap (and every other changed table)
   again after a profile switch; it does not count an unchanged re-sent Fn
   keymap, which reads the same whether the keyboard kept it or not. An
   advanced-key clear written after the Fn keymap would be reported lost if
   those slots did need the trigger, and running the restore again ends
   the group with the Fn keymap.

Edits (`set trigger`, `set light`) do not switch away and back: that would
change the lighting for every small edit. They rely on rules 2 and 4: they
check the active profile after the last write and at the end of the wait,
and report the wait (`EditReport::waited_for_commit_ms`); their items stay
`not_checked` (or become `maybe_lost`). A cancelled or failed restore skips
the persistence check too (its writes were still followed by the wait).

Cancellation and the wait: before a restore selects the next profile it
first waits for the settle (`Writer::wait_for_commit_observed`), then checks
for a cancellation at that profile's first table boundary, and only then
selects it. A Ctrl+C pressed while the CLI shows "Waiting for the keyboard
to save profile N" therefore stops the restore before the next profile is
selected or written. The wait itself is never cut short: the switch back to
the original profile needs it too.

`CommitSettle` cannot be turned off by accident: its public constructor
`CommitSettle::at_least` never gives less than 1.5 s, and the zero settle
(`CommitSettle::disabled_for_tests`) exists only with the `mock` feature,
which no build that talks to a keyboard enables. Tests use it to reproduce
the hardware failure on the simulator.

### Safety backups

Every public way to write a table (`Writer::write_guarded`,
`execute_restore`, `apply_trigger_edit`, `apply_light_edit`) takes a
`&SavedBackup` covering the profile it writes; the unguarded write primitive
is crate-private. Only `Backup::save_safety` creates a `SavedBackup`: it
saves the backup under a new name
(`openwraith-<product>-<stamp>-<reason>[-N].owbackup.json`) without replacing
any file, reads it back and checks that it is identical. So "back up first"
is enforced by the types. Profile selects (`Writer::select_profile`) change
no stored setting and need no backup. The directory is `--safety-dir`, else
`OPENWRAITH_SAFETY_DIR`, else the per-user data directory
(`%LOCALAPPDATA%\OpenWraith\safety-backups`,
`$XDG_DATA_HOME/openwraith/safety-backups` or `~/.local/share/...`,
`~/Library/Application Support/OpenWraith/safety-backups`); if none can be
determined the operation fails before anything is written, never falling back
to the current directory. Safety backups are never deleted automatically.

### Restore

```
openwraith restore FILE [--profile N]...
  Backup::load(FILE)
  open WritableHidTransport
  prepare_restore
    identify; model must match (hard error), firmware must match unless
    --allow-firmware-mismatch
    safety = read_profiles(active ∪ scope)    only the active profile in scope:
                                              a pure read; otherwise switches
                                              profiles (W1) and back
    plan = plan_restore(target, safety)       pure
  save_safety(dir, "restore")                 verified
  print the plan (with --json: on stderr before the question, the JSON
  document after it); nothing to write -> exit 0 without a question
  [y/N] (or --yes)
  execute_restore (with the options the restore was prepared with)
    writer (gate); refused -> report the profile the keyboard reported, or
    "unknown" after a reset or a lost connection
    for each profile with writes (the original one first, then ascending):
      wait for the commit settle              2 s since the previous
                                              profile's last write; ends
                                              with 21/11 (still that
                                              profile?)
      cancelled (Ctrl+C, also during the wait)? stop here
      select_profile                          nothing left to wait for
      for each table, in order: triggers (one key per packet), dead band,
      keymap group (every changed macro slot, 18 00, 18 02 right after
      it, advanced-key clears; no layer differs: the slots, then the
      unchanged 18 02), custom colours 1, main light (W2 only on a mode
      change):
        cancelled (Ctrl+C)? stop here; inside the keymap group after a
                            group table was sent: skip the group's
                            other tables, send its 18 02, stop after it
        confirm 21/11
        re-read exactly the bytes the write changes
        already the target?        -> AlreadyMatched (a global table written
                                      in an earlier profile); the 18 02
                                      commit trigger is sent anyway
        neither planned nor ours?  -> changed_since_plan, stop
        else write (stop-and-wait, verify, one whole-table retry)
      confirm 21/11
    everything written? -> persistence check (not cancellable): every
      written profile selected again after switching away (settle first),
      its written tables read again: persisted, or lost (-> failed)
    select the original profile again (unless poisoned; settle first)
    a table of unknown scope written and two or more profiles compared?
      -> final check (not cancellable): read every profile again, compare
         every written table and every unknown-scope table the plan left
         unchanged (a shared table written in another profile changes it)
    wait for the commit settle (usually nothing is left to wait)
    profile changed or connection lost before the save? -> the last
      profile's written, unconfirmed tables become maybe_lost
  print the report: what was written and verified (and persisted), what was
  lost or may be lost, what failed (with the read-back difference), what
  was not attempted, the active profile now, the safety backup and the undo
  command
```

`plan_restore` classifies each table of each profile as `unchanged`,
`write`, `skip` (with a machine-readable reason) or `not_in_backup`. Tables
without a confirmed write command are skipped when they differ. The keymap
group (both keymaps, ten macro slots, forty advanced-key slots) is planned as
one unit and skipped as a whole when it would need a non-empty advanced-key
write, a non-canonical macro, holds a malformed reference, or would need the
Fn keymap as its commit trigger (rule 5 above) while the backup's Fn keymap
cannot be written. The Fn-keymap write that is the commit trigger carries
`PlannedWrite::commit_trigger` (and, when it re-sends the keyboard's Fn
keymap unchanged, no changes; `is_resent_unchanged`); the CLI shows the
reason in the plan and counts it apart in the report ("the Fn keymap (18 02)
was also re-sent unchanged ..."). Such an unchanged re-send changes no other
profile, so it does not raise `SharedTablesMayChangeOtherProfiles`. An empty macro
slot (all `00` or all `FF`) is written as 256 x `00`; nothing else is ever
normalised, and defaults the backup does not contain are never written.

Trigger and dead-band values are raw counts of the keyboard's distance unit
(`21/04` raw unit x the status high-precision flag). They are restored only
when the backup and the keyboard both use 0.02 mm units; otherwise every
differing trigger key and the dead band are skipped with
`distance_unit_changed` (the same numbers would mean other distances; values
are never converted). This matters after a firmware update, the main use case
of a restore.

Plan warnings point out tables of unknown scope: written while the restore
does not cover every profile (`shared_tables_may_change_other_profiles`), or
held in different copies by the backup while the keyboard holds one copy in
every profile (`likely_shared_table_differs`).

A dry run (`restore --dry-run`) reads only the active profile through the
read-only transport (`dry_run_restore`, no `21/12`) and lists the other
profiles as not compared; `--against FILE` plans against another backup
without a keyboard. A serialised plan is for display only: only a
`PreparedRestore` (not deserialisable) can be executed.

### Edits

`set trigger` and `set light` read the current values (`plan_*_edit`,
read-only), save a safety backup of the active profile, show "now -> new",
ask, and write (`apply_*_edit`). Each key or the light record is re-read
right before writing: if it changed since planning the edit stops with
`changed_since_plan`, if it already holds the new value nothing is written.
Triggers are written one key per packet (the only captured form; multi-key
masks are CODE-ONLY, hardware test H10 in
[HARDWARE_TESTS.md](HARDWARE_TESTS.md)). A light edit sends W2 only when the
mode changes and then sets the direction to the mode's default. The write
itself goes through the same guarded steps as `Writer::write_guarded`. An
edit returns only after the commit settle (see "Commit settle and
persistence"); it does not switch profiles to prove the save.

### The CLI around it

The commands that write live in `commands`, generic over a `Connector`
(which opens read-only and writable sessions) and a `Ui` (output, progress,
the question, cancellation). The binary uses `HidConnector` and `Terminal`;
the unit tests use a connector over `DeviceSim` and a scripted console, so
they run exactly the command code without any HID device. The binary has no
simulator mode.

- Commands that write print the close-the-vendor-site note first. With stdin
  not a terminal and no `--yes` they refuse before the keyboard is opened.
- A wait for the keyboard to save is printed as a note that stays ("Waiting
  for the keyboard to save profile 2 (2 s)..."), not as a transient progress
  line, so the pause never looks like a hang. Restore reports print
  "written and verified (N packets), persisted" per item and a
  "Persistence check: ..." line; a lost write is printed as LOST with the
  bytes that differ. Edit reports say how long they waited. A write whose
  save was interrupted (profile change or lost connection, rule 4) is
  printed as "written and verified (N packets), but MAY BE LOST" under a
  headline that says the keyboard may not have saved it, with exit code 1.
- Before the safety backup and the question, a session that already saw
  another program is refused (the writer's gate would refuse later anyway).
- Ctrl+C sets a flag that the progress callbacks turn into a cancellation at
  the next table boundary; the original profile is selected again. Inside a
  restore's keymap group, once a group table was sent, the group's other
  tables are skipped but its Fn keymap (`18 02`) is still written, because
  without it the keyboard would not keep the base keymap (rule 5 above);
  the message the CLI prints says so. A Ctrl+C
  during a wait for the keyboard to save is seen right after the wait,
  before the next profile is selected. A second
  Ctrl+C exits at once with a warning. The final check of a restore is not
  cancellable. On Linux the signal interrupts the blocking HID read (`poll`
  is never restarted); the transport reports `TransportError::Interrupted`
  and the session reads again, so the first Ctrl+C never looks like a lost
  connection.
- Exit codes: 0 when everything planned was written and verified (for a
  restore: and persisted; skipped tables allowed and printed) or there was
  nothing to do, 1 on a failure (a lost write included) or when the change
  was not confirmed, 2 for usage errors.

## Data flow

### Reading from the keyboard

```
openwraith backup
  -> device::open_session
       HidBackend::new()            enumerate HID devices
       HidBackend::devices()        keep VID 0x2E3C, PIDs C365-C368,
                                    usage page 0xFF1B, usage 0x91
       HidBackend::open()           -> HidTransport (read-only)
  -> read_all_with(session)
       DeviceInfo   (0D)            model check: product code must be known
       Status       (01)            capability flags select optional reads
       ReadRequest::full_read_plan  113 requests on the W75
       ActiveProfile (21/11) again  fails if the profile changed
  -> Backup::save                   JSON in a unique temporary file, synced,
                                    then moved into place (never replacing
                                    an existing file without --force)
```

`openwraith backup --all-profiles` uses `read_all_profiles` on a writable
session: the same plan for the active profile P, the gate, then for each
other profile `21/12` select, a complete read with `21/11` checked at the
start and the end, and finally `21/12` back to P (also after a failure,
unless the writer is poisoned). It writes no setting, so in a fresh session
no switch waits for a commit settle; a long-lived session that wrote a
setting before waits once, before the first switch.

`openwraith info` uses `read_summary` instead: device info (with the same
model check), status, active profile, global trigger parameters and the
polling rate.

For each request, `Session::request`:

1. drains stale input reports (another program's replies, old pushes);
2. writes `request.packet()`;
3. reads reports and offers each to the request's `ResponseCollector`.
   Matching uses byte 0, byte 1 (or byte 5 for the `0x21` family) and the
   row/col echo for per-key triggers, never arrival order (spec 2.8 rule 2);
4. ignores unrelated reports and classifies them: heartbeats and pushes,
   acknowledgements of another program's writes, profile reports (recorded as
   events). Reports that our own writes cause (the profile push after a
   select, the status push after W2, a late acknowledgement) are expected
   and consumed. Unrelated reports never extend the deadline, and the drain
   stops after `max_drain` reports;
5. stops with an error on a reset notification;
6. completes when every chunk index has arrived; the wait is extended while
   chunks keep arriving;
7. on timeout re-sends once (spec 2.8 rule 3), keeping chunks already received;
8. compares a second copy of a chunk with the first: identical copies are
   duplicates, different ones mean the data changed while it was read. The
   partial reply is then discarded and the request re-sent; on the last
   attempt the request fails with `SessionError::Inconsistent`. A reply is
   never stitched together from two states.

**Other programs.** The filtering is not perfect, in both directions:

- Some replies cannot be told apart from another program's traffic. The
  `09 20` reply is byte-identical to every colour-write acknowledgement
  (spec 2.7, 4.7.9), and the status reply is the vendor site's heartbeat
  reply (spec 4.2).
- On Windows every handle of the configuration interface receives every
  input report, so **our replies reach the vendor site too**. The site acts
  on every `21/11` profile reply (deletes and rewrites its stored mirror,
  re-reads, sends its no-op empty-mask trigger write) and assigns `21/05`
  replies by arrival order, so our extra replies can land under the wrong
  keys in its model (spec 4.5.1, 4.11.2, 6.5).
- Browsers throttle timers in background tabs (to about once a minute), so a
  hidden vendor tab may send nothing during the 2 s gate and go unnoticed.

The README therefore tells users to close the vendor tab completely, the
writer refuses to start when it saw another program, and the CLI warns after
any device command that saw status reports it did not ask for, write
acknowledgements it did not cause, or an unsolicited profile report.

### Reading a backup file

```
openwraith show all --from FILE [--profile N]
  -> Backup::load
       check "format" and "schema_version" (1 or 2)
       RawResponse re-validated (all chunks, every report matches its request)
       a request stored twice is refused
       schema 2: every snapshot checked (profile 1-4 from its own 21/11
       reply, unique, not the primary's, same product)
       decoded views, device header and active profile rebuilt from the raw
       reports
  -> --profile N: the snapshot of profile N as a single-profile view
  -> view::* (TriggersView, LightView, ...) -> render::* or JSON
```

The same rules apply to any serde deserialization of a `Backup` (for
example a Tauri command argument): `Backup` deserializes through a private
representation (`#[serde(try_from)]`) and `BackupEntry` through its raw reply
only (`#[serde(from)]`).

## Backup format

See the `backup` module documentation for the full structure. The rules that
keep backups usable across versions:

- `entries[].raw` is the source of truth: the request and all reply reports,
  63 bytes each as hex, including stale bytes after the declared length.
- `entries[].decoded` is written for convenience but **ignored when loading**;
  it is regenerated from `raw`. A decoder change never invalidates a backup.
- `device` (except `device.usb`) and `active_profile` are derived from the raw
  `0D` and `21/11` replies when loading, so an edited header cannot
  contradict the data. Without a `0D` reply the device is unknown.
- A reply that cannot be decoded keeps its raw reports and a `decode_error`;
  the CLI lists such replies instead of silently skipping them.
- `schema_version` is checked: files from a newer schema are refused with a
  clear message instead of being half-understood.
- **Schema 1** (single profile, `read_all`) is unchanged since Phase 1, byte
  for byte, so Phase 1 binaries still read it. `scope` / `note` state that
  only the active onboard profile is included, plus the tables whose profile
  scope is unverified (custom colours, polling rate, switch types; spec 4.5.4,
  6.9, 6.10, Q3). The `scope` value `active_profile_and_global_settings` is
  kept for compatibility.
- **Schema 2** (all profiles, `read_profiles`) adds `"scope": "profiles"` and
  `"profiles": [{ "profile": n, "entries": [...] }]` for the profiles other
  than the primary. The top-level `entries` remain the primary snapshot (the
  profile that was active, read before any switch), so every accessor and
  view keeps working on it. Each snapshot is a complete read (including `0D`,
  `01` and `21/11`); there is no split into "global" and "per-profile"
  entries, because the scope of most tables is unknown. Identical copies
  across snapshots are free scope evidence. `profile` in JSON is
  informational: it is re-derived from the snapshot's raw `21/11` reply.

## Testing

- `crates/openwraith-protocol/tests/captures.rs` decodes every reply in the
  captures and checks known facts (factory triggers, W key settings, reset
  behaviour).
- `crates/openwraith-protocol/tests/decode_fuzz.rs` decodes a few hundred
  header-valid replies with arbitrary lengths and contents for every
  constructible request (fixed seed): decoders must never panic.
- `crates/openwraith-protocol/src/write/tests.rs` checks every constructible
  write against an independent predicate of the allowed forms, the forbidden
  forms and read/write disjointness;
  `crates/openwraith-protocol/tests/write_captures.rs` checks that the
  builders produce the captured packets byte for byte (c02, c03, c08-c13),
  including a read-modify-write of the keymap that equals the vendor site's
  write and the c09 chunks that differ only by the site's forced knob
  defaults.
- `crates/openwraith-hid/tests/` run sessions, summaries and full backups
  against `MockTransport` built from the captures (noise, lost replies and
  retries, partial tables, endless unrelated traffic, timeouts, stale input,
  the real reset notification, profile changes, unknown models, undecodable
  replies, JSON and plain-serde round trips, edited headers, atomic saves),
  and the writer, all-profile backups, restore and edits against `DeviceSim`
  (round trips, both scope configurations of every unknown-scope table, ack
  loss and retries, late acks of an earlier table, verification failures and
  final-check mismatches, reset notifications, disconnects, interrupted
  reads, keyboard-side profile changes, foreign heartbeats and acks, the 2 s
  gate observation and the expectation windows on the virtual clock,
  cancellation, the `RepeatedWrite` guard, writes without a safety backup).
  The simulator's delayed commit is on in every test, so a write followed
  by a select within 1 s of virtual time is lost exactly as on the
  keyboard. Regression tests for the hardware failure: the exact scenario
  (`restore --profile 2` from profile 1) persists and is reported
  `persisted` with the default settle, and is reported `lost`
  (`NotPersisted`) with the test-only zero settle; a select waits at least
  the settle after a write (packet times on the virtual clock) and not at
  all without one; the settle spans the writers of a session; an
  all-profile backup without writes takes exactly the gate's 2 s; one after
  a write waits once; an edit returns only after the settle (and a
  `power_cycle()` right after it keeps the change, while the zero settle
  loses it); a reset notification during the wait poisons the writer and
  nothing more is sent. A profile switch on the keyboard 300 ms after the
  write (`switch_profile_at`), with and without a profile report, fails an
  edit, a restore (`restore --profile 2`) and a GUI-style
  `write_guarded` + `wait_for_commit` with `ProfileChangedBeforeSave` and
  reports the write `maybe_lost` (the simulator confirms it dropped it); so
  do a switch right after the last packet and an unplug right after it.
  The simulator's keymap commit rule is on in every test too (a base
  keymap without a following complete `18 02` is never saved, and a
  complete one only releases it to the 1 s commit; the simulator's own
  tests check that a switch 300 ms after base + Fn loses both layers, that
  a truncated `18 02` saves nothing and that an Fn-only write followed at
  once by a select is lost). Regression tests for hardware test H9: the
  restore of a backup that differs only in the base keymap sends `18 00`
  x10 then `18 02` x10 (the backup's Fn table) and the base keymap
  persists, with either setting of `macros_and_advanced_need_fn_write`;
  the same plan without the trigger (the old order, made in a unit test of
  `restore`) is reported lost by the persistence check, and so is the
  restore with the trigger when the commit settle is turned off (test-only
  option). Further tests: the group order (macros, `18 00`, `18 02`, `22`;
  with the macro / advanced-key setting on, the advanced-key clear after
  the trigger is reported lost and a second run ends with the trigger and
  persists), macro slots alone ending with the unchanged Fn keymap, no
  cancellation between the base and the Fn keymap, a Ctrl+C during the
  first macro slot (the base keymap is skipped, the Fn keymap still sent,
  the outcome `Cancelled`, the macro saved in both settings), a Ctrl+C
  whose only remaining write is the trigger (the restore completes), a
  dropped ack of the first `18 02` packet (the whole table is sent again)
  and of both sends (the restore fails, the base keymap `MaybeLost` with
  `BaseKeymapNotSaved`), the same warning when another program stops the
  restore in between, an unwritable Fn keymap blocking the group, and
  `CommitTrigger` refusing every other request kind while
  `write_commit_trigger` keeps the backup, profile, other-program and
  `RepeatedWrite` guards, needs every packet acknowledged
  (`CommitTriggerIncomplete`), and clears `base_keymap_unsaved()`, whose
  profile switch otherwise reports `BaseKeymapNotSaved`.
  `tests/planner.rs` pins every skip reason, block cause and plan warning of
  `plan_restore`. Every simulator test ends with `assert_clean`: the
  simulator refuses and records any packet outside the allowed read and write
  forms (including dead-band values above 50).
- `crates/openwraith-cli/src/sim_tests.rs` runs the CLI's commands exactly as
  parsed from the command line against `DeviceSim`: dry runs (reads only,
  read-only session), restores with `--yes`, a "y" and an "n" answer, the
  undo command, all-profile backups, `set trigger` / `set light`, `profile
  select`, Ctrl+C at a table boundary and during the wait between two
  profiles (stops before the next profile), a profile switch on the
  keyboard during an edit's wait ("MAY BE LOST", exit code 1), another
  program, a failed
  verification, the refusal without a terminal or `--yes`, and the
  hardware test H7 sequence with its "Waiting for the keyboard to save" and
  "persisted" lines (`cargo test -p openwraith-cli
  all_profile_backup_then_restore_of_one_profile -- --nocapture` prints
  the output), and hardware test H9: the restore of the pre-c09 backup,
  whose `--dry-run --packets` lists ten `18 00` packets and then ten
  `18 02` packets with the reason, and whose report counts the re-sent Fn
  keymap apart (`a_base_keymap_restore_re_sends_the_unchanged_fn_keymap`
  prints both).
- `crates/openwraith-cli/tests/cli.rs` runs the real binary on backup files
  replayed from the captures or made with the simulator (`--from`,
  `restore --dry-run --against`), including crafted files with edited headers
  and escape sequences, plus help texts and argument errors.
- No test enumerates or opens a HID device. Keep it that way: hardware tests
  must be run by hand by someone who has the keyboard. The checklist (H0-H11:
  preconditions, exact commands, what to compare, pass criteria) is
  [HARDWARE_TESTS.md](HARDWARE_TESTS.md).

## Deferred (library API for the desktop app)

Done in Phase 2: serialisable error kinds (`ErrorKind`, `ErrorInfo`),
cancellable progress (`read_all_cancellable`, `WriteProgress`,
`ProfilesProgress`, all `ControlFlow` based), serialisable plans and reports.

Also done: `execute_restore` uses the options the restore was prepared with
(`PreparedRestore::options`), `WriteProgress::Writing` carries `send` (2
while a table is sent a second time), and a long-lived session can re-arm the
writer's gate (`Session::rearm_gate`).

Still open, to be designed with the Tauri app:

- device selection in the library (only-one-device rule, open by path) and a
  serialisable device list (`DeviceDescriptor` holds a private `CString`);
- `Deserialize` for the intents a GUI sends (`TriggerEdit`, `LightEdit`,
  `RtEdit`, `ProfileSet`, `RestoreOptions`, `WriteOptions`); they derive only
  `Serialize` today, so a Tauri command needs mirror types;
- an `EditError` type: `plan_*_edit` / `apply_*_edit` report problems as
  `RestoreError` (`InvalidRequest`, `SafetyBackupMismatch`);
- keymap and macro editing on top of `KeymapData::set_entry` and
  `MacroData`;
- multi-key trigger masks (`TriggerGrouping::SameSetting`, CODE-ONLY until
  hardware test H10 in [HARDWARE_TESTS.md](HARDWARE_TESTS.md)).

## Adding a model

All four Wraith models (W60, W65, W75, W100) share the vendor ID and the
product IDs `0xC365`-`0xC368`; the model is identified by field 4 of the
`0x0D` device-info string (spec 2.1). To add one:

1. **Capture it.** Record a connect sequence of the vendor site with the
   capture hook (`captures/hook.js`) and add the file to `captures/`. Do not
   copy the site's layout file into the repository.
2. **Key table.** In `crates/openwraith-protocol/src/layout.rs`, add a
   `static <MODEL>_KEYS: [Key; N]` with row, column, name and HID usage of
   every key. The positions are the ones the site reads with `21/05`; names
   and usages are facts about the keyboard (spec 5 explains how the W75 table
   was verified).
3. **Model.** In `model.rs`, add a `static <MODEL>: Model` (name, product
   code from the `0x0D` string, PIDs, key table, knob and Fn index, and its
   `WriteCeilings` from its `21/04` and `0A` replies) and append it to
   `MODELS`. `model_for_product` then recognises it, and
   `ReadRequest::full_read_plan` reads its keys.
4. **Units.** `Distance` assumes 0.02 mm per unit (the W75 reports
   `rawUnit` 2 in `21/04` and no high-precision flag). If the new model reports
   another unit, extend `Distance` before showing millimetres; the CLI already
   warns when the unit differs, `set trigger` refuses to write and `restore`
   skips trigger and dead-band values.
5. **Writes.** Before enabling writes for the new model, capture its write
   commands and confirm them as for the W75 (spec 7.3); the simulator and the
   write tests are W75-specific.
6. **Tests.** Add capture-based assertions for the new model (device info,
   key count, a known trigger value) like the W75 ones, and update
   `PROTOCOL.md` with anything that differs, with confidence labels.
