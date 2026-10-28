# OpenWraith architecture

This document explains how the code is organised, how read-only code paths
are kept read-only, how the guarded write path (Phase 2) works, how data flows
from the keyboard to a backup file and back, and how to add another keyboard
model. The protocol itself is described in [PROTOCOL.md](PROTOCOL.md);
section numbers below ("spec 2.8") refer to it.

## Crate layout

```
openwraith-cli  (bin "openwraith")   openwraith-app (app/src-tauri, Tauri 2)
      |                                    |   commands, channels, dialogs
      |                              openwraith-app-core (no Tauri)
      |                                    |   device worker, operations,
      |                                    |   IPC types, simulator backend
openwraith-hid                          transports, session, backup, writer,
      |                                 restore, edits, safety backups,
      |                                 mock + simulator (feature "mock")
openwraith-protocol                     pure protocol: no I/O
```

The desktop app (see "The desktop app" below) sits next to the CLI and uses
the same two libraries.

### `openwraith-protocol` (no I/O)

| Module | Contents |
|---|---|
| `frame` | `REPORT_ID`, `REPORT_LEN` (63), `MAX_CHUNK_PAYLOAD` (58). `OutPacket` (no public constructor; read requests only) and `Report` (any 63-byte input report; tolerates byte-4 quirks and stale bytes, spec 2.3 / 2.4). |
| `request` | `ReadRequest`: one variant per read command of the spec, all CONFIRMED. Each knows its packet bytes, its `ResponseSpec` (how replies are recognised, spec 2.3 / 2.8), its chunk layout and its timeout. `full_read_plan` lists everything a backup reads. |
| `response` | `ResponseCollector` (accepts matching chunks, ignores the rest, flags a second copy of a chunk with different data as `Offer::Conflict`, detects completion) and `RawResponse` (a complete, validated reply; the lossless unit of a backup). |
| `decode` | Typed models for every reply (`DeviceInfo`, `Status`, `LightEffect`, `ColorTable`, `KeymapTable`, `MacroSlot`, `KeyTrigger`, ...) and of the travel stream (`TravelReport`, short and long variant, spec 4.11.7). Every model keeps its raw bytes. Field docs repeat the spec's confidence labels. |
| `write` | The confirmed write forms (W1-W11 and the travel test's T1/T2 below). `WritePacket` (no public constructor, separate from `OutPacket`), `WriteRequest` (Serialize only) with `steps()` (packets, acknowledgement spec, pauses), `verify_reads()` and `check()`, `TravelTestRequest::step()` (the only other producer of a `WritePacket`), `AckSpec` / `AckTracker` (`AckSpec::Possible`: an acknowledgement that may come and is never waited for), `TableId`, `is_permitted_packet` (a coarse egress allowlist of packet headers), and the validated table data built from raw replies: `KeymapData`, `MacroData` (canonical flat layout only), `ColorTableData` (custom table 1 only; `set_key_colors` changes only keys of the layout), `DeadBandData` / `DeadBandWrite` (range-checked), `LightRecord` / `LightLimits` / `MainLightWrite`, `TriggerSetting` / `TriggerLimits` / `TriggerWrite` with the closed `TriggerWriteMode` (`00`, `0C`, `0D`; no `From<u8>`), `SocdPairsData` / `SocdWrite` (the site's pair rules), `AdvancedSlotData` / `AdvancedKeyDef` / `AdvancedLimits` (DKS, Mod-Tap, Toggle), `MacroProgram` (keyboard-step macros). Key assignments are the closed `KeyAssignment` over validated atoms (`HidUsage`, `MediaKey`, `MouseAction`, `Combo`; only values the vendor site offers); `KeyAssignment::classify` keeps every other entry as read (`EntryKind::Preserved`), `check_placement` refuses the knob, the Fn key, Fn-locked keys and unacknowledged built-in Fn functions (`BuiltinAck`). `Evidence` / `UnverifiedNote` say what is not verified on hardware. `ProfileId`, `AdvancedSlotId`. |
| `event` | Classification of unsolicited reports: keyboard reset (`14 .. 01 01`), profile reports, the travel stream, the SOCD push of Fn + Right Shift (`24 02 .. 06 PP`), status pushes. |
| `layout`, `model` | Key positions (`KeyPos`, index = row x 22 + col), key masks, the W75 key table and the `Model` description (spec 5), including its `WriteCeilings` (absolute limits for written values) and its built-in Fn functions (`FnBuiltin`; Fn + 1..4 and Fn + Space are critical). |
| `units` | `Distance` in trigger units of 0.02 mm. |
| `capture` (feature) | Parser for the JSON capture files and the request/reply pairing used by tests and the mock transport. |

### `openwraith-hid`

| Module | Contents |
|---|---|
| `transport` | `Transport` trait: `write(&OutPacket)`, `read(timeout) -> Option<Report>`, `usb_info()`, `now()` (the clock every timeout is measured on: the system clock, or the simulator's virtual clock). `WriteTransport: Transport` adds `write_command(&WritePacket)`. |
| `hid` | `HidBackend` (hidapi; enumerates and filters supported interfaces, nothing is opened), `HidTransport` (read-only) and `WritableHidTransport` (opened with `HidBackend::open_writable`). |
| `session` | `Session<T: Transport>`: one request at a time, reply collection, filtering, timeout and retry, conflict detection, reset detection, retry of reads interrupted by a signal (`TransportError::Interrupted`, Ctrl+C on Linux), and the classification of unrelated reports (status reports, write acknowledgements and profile reports of another program: `foreign_activity()`, `last_foreign_at()`; `listen()`; `rearm_gate()` starts the writer's gate afresh in a long-lived session). The keyboard's own pushes are never another program: SOCD pushes become events (`socd_pushes()`), travel-stream reports are queued for an open test or counted as stray (`travel_state()`: `Closed`, `Open`, `MaybeOpen`; `stray_travel_since_gate()`). `last_write_at()` / `last_write_profile()`: when and to which profile the last write that changes a setting was sent (for the commit settle; kept per session, so it spans writers). `base_keymap_unsaved()`: a base keymap sent without a complete Fn keymap after it. Every write packet passes `is_permitted_packet` right before it is sent; a refused one poisons the session (`WriteError::EgressRefused`). |
| `summary` | `read_summary` / `DeviceSummary`: the few reads behind `openwraith info` and `openwraith profile`, with the model check shared with `read_all`. |
| `backup` | `Backup` (file format, schema 1 and 2), `SnapshotView`, `read_all` / `read_all_with` / `read_all_cancellable` / `read_all_observed` (for a long-lived session: before every request it classifies what arrived, sending nothing, and asks a callback about the foreign activity since the gate mark, so a GUI can stop a read as soon as another program is heard), save/load, typed accessors. |
| `writer` | `WriteConsent`, `WriteOptions` (with `CommitSettle`), the "other program" gate in `Session::writer`, and `Writer`: `write_guarded` (the only public table write: needs a `SavedBackup`) and `write_commit_trigger` (the same guards, for a `CommitTrigger`, the Fn keymap only, sent even when the keyboard holds it: the keyboard keeps a base keymap only when the Fn keymap is written after it; every packet must be acknowledged, else one more send of the whole table, then `WriteError::CommitTriggerIncomplete`), `base_keymap_unsaved()` (a base keymap sent without a complete Fn-keymap write after it; a profile switch then reports `WriteWarning::BaseKeymapNotSaved`), stop-and-wait writes with read-back verification, profile selection behind the commit settle (`select_profile[_observed]`, `commit_pending`, `wait_for_commit[_observed]`, `CommitWait`), detection of a profile change or lost connection before the keyboard saved (`commit_interrupted`, `WriteError::ProfileChangedBeforeSave`), poisoning after a reset or a lost connection. |
| `profiles` | `read_profiles` / `read_all_profiles`: the all-profile backup (switches profiles and back). |
| `safety` | `default_dir`, `Backup::save_safety` and `SavedBackup` (the proof that a verified safety backup exists). |
| `restore` | `plan_restore` (pure), `dry_run_restore` (read-only), `prepare_restore`, `execute_restore` (with the persistence check), the import of one profile of a file into another (`RestoreOptions::import`, `ImportOptions`, `ImportSelection`), and the serialisable plan, progress and report types (`Persistence`, `PersistenceCheck`, `ItemResult::Lost`, `ItemResult::MaybeLost` (also a base keymap left without its Fn keymap, with `WriteWarning::BaseKeymapNotSaved`), `CommitTriggerReason` on the Fn-keymap write that makes the keyboard save the keymap group, `RestoreReport::dangling`). |
| `edit` | One edit engine for every editor of the active profile: `plan_*_edit` (read-only) and `apply_*_edit` for triggers, the main light, the keymap group (`KeymapGroupState`, `KeymapChange`, `KeymapEdit`: key assignments on both layers, macros, advanced keys), custom colours 1, the dead band and SOCD (`SocdState`); `EditOptions` with `PersistenceCheckMode`, `EditError`, `EditWarning`, `EditReport`. See "Edits" below. |
| `presets` | `TRIGGER_PRESETS` (typing, balanced, fast, competitive, factory) and `preset_edit`: a preset is a normal trigger edit. `factory` writes the factory record `00 55 55 01 01` exactly (`RtEdit::OffWith`: RT off with 0.02 mm stored); part B's reset detection must take the automatic backup after such a write as the new known-good baseline, not as a reset. |
| `travel` | The live key-depth view: `Writer::open_travel_test` returns a `TravelTest` guard that borrows the writer and always closes the test; `live_travel` runs the loop; `Writer::close_stray_travel_test`. See "The travel test" below. |
| `switcher` | `ProfileSwitcher` (`Session::profile_switcher`): profile selects for standing rules (consent `standing_rule`), nothing else. |
| `group`, `persist` (crate-private) | The keymap group's cursor (cancellation rules, completing bindings, dangling references) shared by restores and edits; the persistence check and its evidence table (`persistence_confirmed`). |
| `error` | Error types and `ErrorKind` (serialisable, for a GUI). |
| `mock` (feature `mock`) | `MockTransport`: replays the captures; can add noise, inject unsolicited reports and drop replies. Read-only. |
| `sim` (feature `mock`) | `DeviceSim`: a stateful W75 with four profiles, an independent parser of the write forms (W1-W11, T1/T2), fault injection, a virtual clock (reads "wait" without sleeping; reports and acks can be scheduled at a virtual time; `sent_times()` gives the time of every packet) and the keyboard's **delayed commit** (writes go to a working copy of the active profile, saved 1 s of virtual time after the last write; a select or `power_cycle()` before that discards them, counted by `discarded_edits()`; `persisted_table` / `persisted_trigger` show the saved state; on by default), the **keymap commit rule** (`KeymapCommit`, on by default: the timer never saves base-keymap edits on their own; a complete `18 02` write, every chunk applied, releases them to it, so base and Fn keymap are saved 1 s after the last `18 02` chunk and a switch before that loses both; a partial `18 02` releases nothing; `has_edits_waiting_for_fn_write()`; `advanced_need_fn_write`, on by default, keeps advanced-key edits only when a complete `18 02` is applied in the same save window: before them as on hardware (H20), or after them, a GUESS that was never tested; `macros_need_fn_write`, off by default because a macro slot is kept alone (H19), makes macro edits wait for an `18 02` too, so that the tests can show that the persistence check reports such a loss), the **macro slot-0 clear** (the first chunk of any `19 00` write empties slots 1-9 of the active profile to all `FF`, saved like any macro edit; the chunk is a GUESS, hardware showed only that the slots were empty after the whole write), table scopes as found on hardware by default (`ScopeConfig`: the dead band and the SOCD pairs per profile, the SOCD power one setting of the whole keyboard, which Fn + Right Shift toggles; a test may choose otherwise, and the tables OpenWraith never writes stay configurable), profile switches made on the keyboard at a virtual time, with or without a profile report (`switch_profile_at`; the profile is reloaded as of that time), and keyboard events: Fn + Right Shift (`press_socd_hotkey_at`), scripted key movements for the travel stream (`travel_script`), a test left open by a crashed run (`leave_travel_open`). |

### `openwraith-cli`

| Module | Contents |
|---|---|
| `main` | clap definitions, the read-only commands (`list`, `info`, `show`, `backup`), `--from` / `--profile` for backup files, friendly error messages with hints, exit codes. |
| `args` | Arguments of the commands that write and of `monitor`, and the basic value parsers: key names (`W`, `"Caps Lock"`, `#46`, `r2c2`), refused per purpose (`KeyUse`: the knob everywhere, the Fn key for keymap entries), millimetres (decimal text to micrometres, no floats, multiples of 0.02 mm), Rapid Trigger values, colours, light modes. |
| `parse` | The value parsers of the v0.2 editors: `KEY=ACTION` (key assignments of every confirmed category), HID usages by name, macro steps (`A+ 100, A- 100, B+ 100, B-`), DKS rows (`E:t...`), SOCD pairs (`A/D:2`), key colours, 0.1 mm press points, durations. Values the vendor site does not offer are refused while parsing (exit code 2). |
| `commands` | `profile [select]`, `backup --all-profiles` and `restore` (and the restore flow an import reuses), generic over a `Connector` and a `Ui`, plus the helpers every command that writes shares (the question, the safety backup, the "other program" check, progress). |
| `edits` | Every `set` editor through one flow (`EditCommand`, `run`): `--dry-run` on a read-only connection, the safety backup, the plan, the question, the guarded write, the report. |
| `edit_text` | Text of the v0.2 plans (keymap group, colours, dead band, SOCD): "now -> new", the writes in order, the notes (unverified values, built-in Fn functions, scope), what happens after the write. |
| `live` | `travel` (the live key-depth view, always closed again; Ctrl+C only asks to stop) and `travel --close`; `monitor` (read-only: prints what the keyboard sends on its own). |
| `share` | `profile export` (one profile as a file, no USB serial) and `profile import` (a restore with `ImportOptions`). |
| `device` | Choosing and opening the keyboard (`--device`, several-keyboard handling); the `Connector` trait and `HidConnector`. |
| `ui` | The `Ui` trait (output, progress, the `[y/N]` question, cancellation, `enable_cancel_close_first` for the key-depth view) and `Terminal` (stdout/stderr/stdin, Ctrl+C handler). |
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
   `dry_run_restore`, every `plan_*_edit`, `KeymapGroupState::read`,
   `SocdState::read`, `Session::listen`) provably cannot write.
   `Session::writer` exists only for `Session<T: WriteTransport>`; a
   `compile_fail` doctest checks that `Session<HidTransport>` has none.
4. **Read-only commands open a read-only transport.** `list`, `info`,
   `show`, `backup`, `profile`, `monitor`, `restore --dry-run`, `profile
   import --dry-run` and every `set ... --dry-run` open `HidTransport`,
   which does not implement `WriteTransport`. Only `profile select`,
   `profile import`, `set`, `restore`, `travel` and `backup
   --all-profiles` open `WritableHidTransport`. `profile export` reads
   through `HidTransport` first and opens `WritableHidTransport` only when
   the profile is not active (it switches to it and back; without a
   terminal that needs `--yes`, checked before the writable session is
   opened).
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

Only these forms, all sent by the vendor site and accepted by the keyboard
in the captures (byte layouts in the `write` module documentation and spec
section 3; W1-W9 checked on hardware by OpenWraith in Phase 2, W10, W11 and
T1/T2 in capture session 3, and all of them with OpenWraith's own writes in
the hardware session H12-H24 of 2026-10-27):

| # | Write | Packets | Acknowledgement |
|---|---|---|---|
| W1 | `21/12` select profile 1-4 | 1 | `21 .. 02 12` and the push `21 .. 03 11 PP 00` |
| W2 | `17 00 .. 02 00 00` music-rhythm leave | 1, only right before a mode-changing W3 | `17 00 00 00 00` (optional) |
| W3 | `07 00` main light | 1 | none |
| W4 | `09 00` custom colour table 1 | 8 | `09 00 00 00 00` each |
| W5 | `18 00` / `18 02` keymap base / Fn | 10 | `18 00 00 00 00` each |
| W6 | `19 0n` macro slot (a write to slot 0 empties slots 1-9, rule 6 below) | 5 | `19 00 00 00 00` each |
| W7 | `22 SS` advanced-key slot cleared (all zero) | 1 | `22 00 00 00 00` |
| W8 | `21 .. 18 MM` trigger, MM in `00`/`0C`/`0D`, **one key per packet** | 1 | `21 .. 01 00` |
| W9 | `26 01` dead band | 5 | `26 01 00 00 00` each |
| W10 | `22 SS` advanced-key slot with a DKS, Mod-Tap or Toggle (c16) | 1 | `22 00 00 00 00` |
| W11 | SOCD: `24 03 .. 01 PP` power (one setting of the whole keyboard), then `24 00` pair chunks 0 and 1 (the active profile's pairs; c15) | 3 | power: none expected (`AckSpec::Possible`, listened for during a 50 ms pause), chunks: `24 00 00 00 00` |
| T1 / T2 | `21 .. 18 02` + the full layout mask: open the travel test; `21 .. 18 03`: close it (c14) | 1 | `21 .. 01 00` |

`WritePacket` has no public constructor and no conversion from `OutPacket`
or bytes; `WriteRequest::steps()` and `TravelTestRequest::step()` are the
only ways to get one. W2 is not a variant of its own: it can only appear as
the first step of a main-light write that changes the mode. Keymap entries
are built only from the confirmed categories (keyboard usage, multimedia,
mouse, combination, macro and advanced-key bindings; `KeyAssignment`), with
the values the vendor site offers. Every other output packet (`0x14` resets,
Win-Lock and gamepad, `17` enter, `08 00`, `09 01`, `25 01`, `21/06`,
calibration, poll-rate set, `24 01` / `24 02` as writes, a travel-test open
with another mask, the vendor site's empty-mask trigger write) is
impossible to construct. A protocol test checks every constructible request
(seeded samples of 2,316 and 992 packets) against an independent predicate
written from the table above, and checks that no write packet parses as a
read. As a last line, `Session` checks every write packet against
`is_permitted_packet` (a coarse allowlist of headers) right before it is
sent.

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
`command_line_flag`, `application`, `standing_rule` for the profile
switcher below); it is written into every report.

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

The session remembers a base keymap sent without a complete Fn-keymap write
after it (`Session::base_keymap_unsaved()`, also `Writer::base_keymap_unsaved()`;
set by any `18 00` packet and cleared by a verified Fn-keymap write whose
packets were all acknowledged; kept by the session, so it spans writers and
the `ProfileSwitcher` refuses to switch while it is set). A profile switch
made by a writer while it is set adds
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
commit"). A power loss in that second loses the write too (desktop-app
test A10: an unplug during the wait lost it). Four rules follow, all in
`openwraith-hid`, a fifth for the keymap group, parts of which the
keyboard does not save on a timer at all, and a sixth for macro slot 0:

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
   says to run the restore again or undo it. A SOCD write whose power alone
   differs after a switch on the keyboard (Fn + Right Shift) since the write
   is left `not_checked` (`PersistenceCheck::not_checked` counts it): the
   power is then the user's.
4. **A switch before the save is never reported as done.** The keyboard
   can be switched on its own keys (or by another program) during the
   settle. The W75 reports such a switch (PROTOCOL.md Q2, observed in app
   test A9), but the code does not rely on it. So: a profile report during a wait ends the wait at once
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
   lost before the settle had passed (an unplug loses unsaved edits,
   hardware test A10). Restores and edits then report the written, unconfirmed tables of
   that profile as `ItemResult::MaybeLost` ("written and verified, but may
   not have been saved"), not `written`, and the known active profile is
   the one the keyboard reported. A switch found at the end of a wait counts
   as made before the save, because its time is unknown. A reset
   notification is not counted: it reports a reset of the settings.
5. **The base keymap needs the Fn keymap after it, advanced-key slots
   need it before them (commit trigger).** The
   settle is not enough for the base keymap: written alone (`18 00`), it
   was gone after a profile switch even 6 s later; followed by an `18 02`,
   both layers were kept, and `18 02`, the dead band, triggers, the main
   light and custom colours 1 are kept on their own (CONFIRMED on hardware
   2026-10-24, [PROTOCOL.md](PROTOCOL.md) 4.8.2; this is how hardware test
   H9 lost its write). Restores and edits therefore write the keymap group
   in the vendor site's order: every macro slot that changes (content and
   clears), the base keymap, the Fn keymap right after it, the advanced-key
   slots. Whenever the base keymap is written the Fn keymap follows,
   re-sent unchanged if it already matches
   (`CommitTriggerReason::BaseKeymap`; shown as "Fn keymap (18 02) re-sent
   unchanged to make the keyboard save the base keymap"). The keyboard
   keeps an advanced-key slot (`22`) only when a complete Fn keymap is
   written in the same save window (CONFIRMED, hardware test H20,
   2026-10-27: a lone `22` was lost at the next profile switch, `18 02`
   then `22` was kept); macro slots (`19`) are kept without it
   (CONFIRMED, H19). So a group that writes advanced-key slots while
   neither layer differs gets the unchanged Fn keymap before them, after
   the macro slots, as the site does (`CommitTriggerReason::AdvancedKeys`,
   a required write, not a precaution); a group that writes only macro
   slots gets none. Only the hardware-test mode of the CLI
   (`--commit-trigger skip`) leaves it out, and then the persistence check
   always runs. If the Fn keymap cannot be
   written (an unusable reply), the whole group is skipped
   (`BlockCause::FnKeymapNotWritable`, or `Incomplete` when the Fn keymap is
   missing): the base keymap is never written alone. The Fn-keymap write
   must arrive complete: every packet acknowledged, else the whole table
   is sent once more (see the write gate above). A Ctrl+C after a group
   table was sent skips the group's other tables (they stay
   `NotAttempted`, a not-yet-written base keymap included), still sends
   the Fn keymap once the base keymap was sent (an `18 02` alone is kept;
   a sent macro slot alone does not need it, so after a Ctrl+C during the
   macro slots it is skipped too, and with it any Fn-layer change that
   would bind a key to a macro slot the Ctrl+C skipped) and every write
   that completes a binding (`completes_binding`: an advanced-key slot a
   written layer refers to, a layer that moves a key away from a macro
   slot already written in the group, cleared or rewritten, a macro slot
   1-9 after slot 0, and a macro slot that a layer newly refers to
   whenever that layer still goes out: `group::link_new_references`), and
   stops after them; when
   those were the last planned writes, the restore did complete and ends
   normally, persistence check included, with
   `RestoreReport::cancel_requested` set (the CLI says "Ctrl+C received"
   and exits 0). If the restore stops between
   the base keymap and the Fn keymap for another reason (another program,
   an incomplete trigger, a failure), the base keymap is reported
   `MaybeLost` with the warning `WriteWarning::BaseKeymapNotSaved` ("will
   be lost" in the CLI), not as written. The persistence check stays the
   safety net: it reads the base keymap (and every other changed table)
   again after a profile switch; it does not count an unchanged re-sent Fn
   keymap, which reads the same whether the keyboard kept it or not. An
   advanced-key slot written without the Fn keymap before it is reported
   lost (H20 step 2).
6. **Macro slot 0 empties slots 1-9.** Any write to macro slot 0 makes the
   keyboard empty macro slots 1-9 of the active profile; a write to another
   slot touches only that slot (CONFIRMED, hardware test H24 and a
   dedicated experiment, 2026-10-27; PROTOCOL.md 4.9.2). The vendor site
   never shows it because it always writes all ten slots in ascending
   order. So every plan that writes slot 0 (a restore, an import, an edit,
   the clear of an orphan slot, an undo) writes every slot 1-9 whose target
   content is not empty right after it, in ascending order and before the
   keymap layers (`PlannedWrite::resent` / `PlannedEditWrite::resent`,
   `ResendReason`: "re-sent: writing macro slot 0 makes the keyboard clear
   slots 1-9"). Once slot 0 was sent these re-sends are sent even after a
   cancellation (the cursor treats them like `completes_binding`), and so
   is a layer write that moves a key off a slot slot 0 emptied. The re-read
   before each later macro write expects the slots 1-9 that this plan's own
   slot-0 write emptied (all `FF` or all `00`), so they are not "changed
   after planning". A re-send that fails is reported with the macros left
   empty (`cleared_macros`) and the keys that now refer to an empty slot
   (`dangling`), also the keys of a layer write the keyboard may drop: a
   base keymap whose Fn keymap never came (`BaseKeymapNotSaved`) or one the
   persistence check found lost goes back to its old entries, which may
   point at a slot slot 0 emptied (`restore::left_behind` counts both the old
   and the new entries of such a write). A new macro takes the lowest free slot in 1-9 and slot 0
   only when 1-9 are taken; a slot 1-9 whose content OpenWraith cannot
   write back blocks a restore or import (`non_canonical_macro`) and makes
   an edit that writes slot 0 `not undoable` (refused unless allowed).

Edits switch away and back only for tables whose keeping is not proven on
hardware (see "Edits" below); trigger, light, colour, dead-band and keymap
edits rely on rules 2 and 4: they check the active profile after the last
write and at the end of the wait, and report the wait
(`EditReport::waited_for_commit_ms`); their items stay `not_checked` (or
become `maybe_lost`). A cancelled or failed restore or edit skips the
persistence check too (its writes were still followed by the wait).

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
`execute_restore`, every `apply_*_edit`) takes a
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
      SOCD (power and pairs in one burst), keymap group (every changed
      macro slot, after slot 0 every slot 1-9 that keeps content, 18 00,
      18 02 right after it, the advanced-key slots; no layer differs: the
      macro slots, the unchanged 18 02 only if advanced-key slots follow,
      the advanced-key slots), custom colours 1, main light (W2 only on a
      mode change):
        cancelled (Ctrl+C)? stop here; inside the keymap group after a
                            group table was sent: skip the group's
                            other tables, send its 18 02 if the base
                            keymap was sent, the slot-0 re-sends and
                            the writes that complete a binding, stop
                            after them
        confirm 21/11
        re-read exactly the bytes the write changes
        already the target?        -> AlreadyMatched (e.g. a macro slot
                                      this restore's slot-0 write emptied,
                                      the SOCD power written in an earlier
                                      profile); the 18 02 commit trigger
                                      is sent anyway
        neither planned nor ours?  -> changed_since_plan, stop (ours: the
                                      SOCD power this restore wrote, macro
                                      slots 1-9 its slot-0 write emptied)
        else write (stop-and-wait, verify, one whole-table retry)
      confirm 21/11
    everything written? -> persistence check (not cancellable): every
      written profile selected again after switching away (settle first),
      its written tables read again: persisted, or lost (-> failed)
    select the original profile again (unless poisoned; settle first)
    the SOCD power changed in a profile other than the original one?
      -> final check (not cancellable): read the power once more in the
         original profile (one keyboard-wide setting; a switch on the
         keyboard since the write is the user's)
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
one unit and skipped as a whole when it would need an advanced-key slot that
is not a DKS, Mod-Tap or Toggle in the vendor site's form
(`AdvancedSlotData::from_read_slot`), a non-canonical macro, holds a
malformed reference, or would need the Fn keymap as its commit trigger (rule
5 above) while the backup's Fn keymap cannot be written. SOCD is restored
as the vendor site's burst (power and pairs) when the backup's pairs are in
the site's form; a difference only in the hotkey bytes, which are never
written, is skipped (`read_only_bytes`). The power is one setting of the
whole keyboard and the pairs belong to each profile (hardware tests H21,
H24): the target power is the backup's power of the profile that was
active when the backup was made (`socd_power_differs_in_backup` when a
restored profile holds another one), every SOCD write sends it, a
profile's SOCD is written only when its pairs differ, and when only the
power differs it is written once (in the original profile if the restore
covers it, else in the first profile written, with that profile's
unchanged pairs). The plan shows the power change once, as entry 0 of the
first SOCD write (the CLI: "SOCD power (all profiles): on -> off"); after
it every profile is expected to read the new power, and only that: a later
SOCD write that reads the old power again (switched back with Fn + Right
Shift, or by another program) stops with "changed after planning" instead
of switching it once more. A write that completes a binding (a
slot a written layer refers to) carries `completes_binding` and is written
even after a cancellation. The Fn-keymap write that is the commit trigger carries
`PlannedWrite::commit_trigger` (and, when it re-sends the keyboard's Fn
keymap unchanged, no changes; `is_resent_unchanged`); the CLI shows the
reason in the plan and counts it apart in the report ("the Fn keymap (18 02)
was also re-sent unchanged ..."). A macro slot re-sent after slot 0
(rule 6 above) carries `PlannedWrite::resent` and no changes; the CLI
shows the reason, and the app counts it as a write but not as a change. An
empty macro slot (all `00` or all `FF`) is written as 256 x `00`; nothing else is ever
normalised, and defaults the backup does not contain are never written.

Trigger and dead-band values are raw counts of the keyboard's distance unit
(`21/04` raw unit x the status high-precision flag). They are restored only
when the backup and the keyboard both use 0.02 mm units; otherwise every
differing trigger key and the dead band are skipped with
`distance_unit_changed` (the same numbers would mean other distances; values
are never converted). This matters after a firmware update, the main use case
of a restore.

Table scopes (spec 4.5.4, 7.3-7.5) as the planner uses them:

| Table | Scope | Evidence |
|---|---|---|
| Triggers, main light, base keymap, macro slots | per profile | c04, c10 |
| Fn keymap, advanced-key slots | per profile | c16, OpenWraith snapshot after c17 |
| Custom colours 1 | per profile | OpenWraith all-profile backup (2026-10-24) |
| Dead band | per profile | H16 |
| SOCD pairs | per profile | H21 |
| SOCD power | **one setting of the whole keyboard** | H21, H24 |
| Custom colours 2, active custom table, side light, switch types, poll rate, Win-Lock / 6-key | unknown | never written, only compared |

There is no "possibly shared" class any more: the plan warnings about
shared tables were removed, and the only keyboard-wide value, the SOCD
power, is planned once (above).

**Importing a profile.** `RestoreOptions::import` (`ImportOptions { from,
into, include, allow_critical_fn_overrides }`) compares profile `from` of a
file (usually an export, `Backup::export_profile`: one profile, schema 1,
scope `profile_export`, no USB serial) with profile `into` of the keyboard
and writes only the chosen categories (`ImportSelection`: triggers and
lighting by default; the keymap group, the dead band and SOCD only when
chosen); the others are skipped as `not_imported`. The file is untrusted
input. Triggers, the main light, the dead band and SOCD go through the same
validated builders as any restore; of SOCD only the pairs are imported, and
the power, a setting of the whole keyboard, keeps the keyboard's value
(`socd_power_not_imported`, a plan note). The keymap layers and custom colours 1
are not written as the file holds them but merged into the keyboard's
tables, read-modify-write: positions that are not keys, the knob and its
slots (15-21), the Fn key (121), Fn-layer keys the keyboard locks (`0E`)
and colour slots outside the layout keep the keyboard's bytes (a plan
note, `import_kept_as_read`, so a file from a keyboard the vendor site
configured still imports); every other differing entry must be one an
editor could write (`KeyAssignment::classify`, `check_placement`), else
the keymap group is blocked (`unwritable_imported_entry`); an imported
macro must be steps `MacroProgram` can express (`non_canonical_macro`);
an Fn layer that would replace a critical built-in function (Fn + 1..4,
Fn + Space) blocks the keymap group (`critical_fn_override`) unless
allowed; entries whose effect is not verified on a W75 (since the hardware
session H12-H24 only an advanced-key binding on the Fn layer, Q36) get an
`unverified` note. A plain restore of an exported file (scope
`profile_export`) is refused (`invalid_request`): it can only be
imported. Everything else (safety backup, plan, persistence check, undo)
is the restore's. `Backup::load` refuses files above 8 MiB before reading
them.

A dry run (`restore --dry-run`) reads only the active profile through the
read-only transport (`dry_run_restore`, no `21/12`) and lists the other
profiles as not compared; `--against FILE` plans against another backup
without a keyboard. A serialised plan is for display only: only a
`PreparedRestore` (not deserialisable) can be executed.

### Edits

Every editor of the active profile has two steps. `plan_*_edit` reads the
current values (read-only, generic over `Transport`) and computes the new
ones from the user's intent, validated against the keyboard's limits and
the protocol's rules; the plan shows "now -> new", every write in order and
what is not verified on hardware (`EditWarning`). `apply_*_edit` writes it
through one engine: the safety-backup check (`SavedBackup` of the active
profile and product), the "other program" gate, every write through the
guarded write of the restore (re-read right before: `changed_since_plan`
when it changed, nothing written when it already holds the new bytes;
stop-and-wait, verification, one more send of the whole table), the keymap
group's cursor (below), the active-profile check, the commit wait,
`MaybeLost` when the save was interrupted, and the persistence check.

| Editor | Writes | Notes |
|---|---|---|
| `plan_trigger_edit` (and `preset_edit`) | W8, one key per packet | multi-key masks are CODE-ONLY (H10) |
| `plan_light_edit` | W3, W2 only on a mode change | the direction is set to the mode's default |
| `plan_keymap_edit` / `plan_keymap_edit_on` (`KeymapGroupState`) | W6, W5, W10, W7 in the group order | below |
| `plan_color_edit` | W4, then W2 + W3 to Custom with `switch_to_custom` | only keys of the layout change; the knob, #103, #126 and the other slots keep their bytes; the `07` foreground is never touched |
| `plan_dead_band_edit` | W9 | 0.02 mm units only; per profile (H16) |
| `plan_socd_edit` (`SocdState`) | W11 | reads only `24 02` and `24 01` (the status report does not hold the SOCD power); the whole burst every time, as the site sends it; keeping the pairs needs them in the site's form; switching the power changes every profile (`EditWarning::SocdPowerAllProfiles`, H21), the pairs are the active profile's |

**The keymap group.** `KeymapGroupState` reads both layers, the ten macro
slots, the forty advanced-key slots, the Fn-locked mask (`0E`) and the DKS
limits (`21/04`). `KeymapChange` is applied in order: `Assign` (default,
key, multimedia, mouse, combination, or a binding to an existing slot),
`BindMacro` / `BindAdvanced` (content and binding: the slot only this key
uses, else the lowest slot that is empty and unreferenced in both layers,
as the site does; for a macro the lowest such slot in 1-9, and slot 0 only
when 1-9 are taken, rule 6 above), `SetMacroSlot` / `SetAdvancedSlot` (content only),
`Unbind`. One slot per key (`SlotAlreadyBound`), never a binding to an
empty slot (`BindingToEmptySlot`), a slot whose last binding goes away is
cleared in the same group unless `keep_orphans`; existing malformed or
dangling entries are kept byte for byte (a warning). Placement
(`check_placement`): never the knob (15) or the Fn key (121), on either
layer; never an Fn-locked key on the Fn layer; an Fn-layer entry that
hides a built-in Fn function needs a `BuiltinAck`, a critical one (Fn +
1..4, Fn + Space) one with `critical`. The writes follow the site's order
(rules 5 and 6 above): changed macro slots (after slot 0 the slots 1-9
that keep content, re-sent), the base keymap, the Fn keymap (re-sent
unchanged after a base keymap, or before the advanced-key slots when no
layer changes; never for macro slots alone), changed advanced-key slots. The cursor
(`group`) keeps the restore's cancellation rules: a Ctrl+C before any group
table stops at once; after one, the other tables up to the Fn keymap are
skipped, the Fn keymap is still sent once the base keymap was, and a write
that completes a binding (`completes_binding`: a slot a written layer
refers to, a layer that moves a key away from a macro slot already cleared
or rewritten in the group, a macro slot that a layer still sent newly
refers to) is written even then, and so are the macro re-sends after a
slot-0 write, so that no key is left pointing at an empty slot or at a
slot's new or old content. A cancellation whose remaining writes
were all needed sets `EditReport::cancel_requested` with the outcome
`Completed`. A failure part-way lists the keys
that may refer to an empty slot (`EditReport::dangling`); the undo command
restores them. `CommitTriggerMode::OmitForSlotsOnly` (a hardware test of
slots without the Fn keymap) is refused when a layer changes and forces
the persistence check.

**The persistence check of an edit** (`EditOptions::persistence`):
`Never`; `Always`; `Auto` (the default) checks only when a written table is
not known from hardware tests to be kept after the commit settle
(`persist::persistence_confirmed`: triggers, the main light, the dead band,
custom colours 1, both keymap layers, macro slots (H19) and SOCD (H21) are;
advanced-key slots only after a complete Fn keymap in the same group, H20).
Each of them also survived a USB power cycle (H22), so the edit plans no
longer carry a "not tested across a power loss" note. In practice `Auto`
checks only under `--commit-trigger skip`, which forces the check anyway.
It switches to the lowest other profile and back (after the settle) and
reads the written tables again (an unchanged re-sent Fn keymap is not
counted); a table that no longer holds what was written is reported
`lost` and fails the edit. `apply_trigger_edit` and `apply_light_edit`
keep their Phase 2 signatures and never check. An entry of
`persistence_confirmed` (one line each) is flipped after the hardware test
that proves it ([HARDWARE_TESTS.md](HARDWARE_TESTS.md)).
`persistence_check_tables(plan.writes(), mode)` tells a plan text before
the question which tables the check will read again (empty: no profile
switch). SOCD can also be switched on the keyboard (Fn + Right Shift): the
SOCD push count is taken before a SOCD write's fresh reads (a switch from
then on is `SocdToggledOnKeyboard { power }` on a verified write, or
`ChangedOnKeyboard` without a retry), and again right after the write; when
it grew by the persistence check (edit or restore) or the restore's final
check and only the power byte differs, the item stays `NotChecked` with that
warning instead of `lost` (the pairs are still compared).

### The travel test (live key depth)

The keyboard's travel test (spec 4.11.7) streams the depth of every key
that moves until it is closed; it stores nothing. `Writer::open_travel_test`
passes the usual gate, waits for the commit settle (nothing may be pending
while the stream runs), confirms the active profile, sends T1 with the
model's full layout mask (the only captured form) and waits for its
acknowledgement (else it sends T2 at once and fails). It returns a
`TravelTest` guard that borrows the writer mutably, so no select, table
write or read can run while the test is open. `TravelTest::poll` drains the
stream; a profile report (Fn + 1..4) or another program's report closes the
test and stops, a reset notification stops without sending anything (it is
never answered). `close` sends T2 (one retry); the guard's `Drop` closes on
every other path, errors and panics included. A close that is never
acknowledged leaves the session in `TravelState::MaybeOpen`, which refuses
table writes until `Writer::close_stray_travel_test` gets an
acknowledgement. Stream reports that arrive while no test of the session is
open (a test a crashed run left open) are counted as stray and refuse table
writes the same way; they are never taken for another program, and
neither is the SOCD push of Fn + Right Shift. `live_travel` runs the whole
loop for a user interface (one frame per slice, a maximum time).

### Profile selects for standing rules

`Session::profile_switcher()` gives a `ProfileSwitcher` (consent origin
`standing_rule`), the only consent a background rule (an automatic profile
per game, part B) gets: it can only select profiles, through
`Writer::select_profile_observed` (so it waits for the commit settle), and
refuses while a base keymap waits for its Fn keymap, while a travel test may
be open, and after another program was seen. `compile_fail` doctests check
that it has no table write. The consent itself is enforced too: a `Writer`
created with `standing_rule` consent selects profiles and reads, but refuses
every other write before anything is sent (`write_with` after its select
branch, `write_commit_trigger`, `open_travel_test`,
`close_stray_travel_test`; `execute_restore` and the edits refuse before
they open a writer), with `WriteError::ConsentOnlyForSelects`
(`invalid_request`). The other origins are not restricted by the library;
part B's source rules decide which code may build which consent.

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
  another program, or key-depth reports of a test left open, is refused
  (the writer's gate would refuse later anyway).
- Every `set` editor runs through one flow (`edits::run`): the arguments
  are checked before the keyboard is opened (exit code 2: unknown keys and
  names, values the vendor site does not offer, the knob, the Fn key for
  keymap entries, unbalanced macros); `--dry-run` plans on a read-only
  connection and writes nothing (no safety backup, no question);
  otherwise the safety backup, the plan (`--packets` adds every packet's
  hex), the question (`--yes`), the write and the report with the undo
  command (`--json`: the plan, the packets, the safety backup and the
  report as one JSON document; the plan goes to stderr before the
  question). `--check-persistence auto|always|never` maps to
  `PersistenceCheckMode`; `--commit-trigger skip` maps to
  `CommitTriggerMode::OmitForSlotsOnly`, forces the check and refuses
  `--check-persistence never`.
- `travel` opens the key-depth test only through `live_travel`; Ctrl+C
  only asks to stop (`Ui::enable_cancel_close_first`: the flag is polled
  every 16 ms slice and the test is closed first); a third Ctrl+C exits at
  once and names `openwraith travel --close`. `travel --close` sends T2
  alone (`close_stray_travel_test`). `monitor` only listens
  (`Session::listen` in 100 ms slices on a read-only connection) and names
  each pushed report: profile reports, SOCD pushes, stray key-depth
  reports, other programs' status reports and acknowledgements.
- Ctrl+C sets a flag that the progress callbacks turn into a cancellation at
  the next table boundary; the original profile is selected again. Inside a
  restore's keymap group, once a group table was sent, the group's other
  tables are skipped but the writes the group still needs go out: its Fn
  keymap (`18 02`) after a sent base keymap, because without it the
  keyboard would not keep the base keymap (rule 5 above), the slots a
  written key needs, and after macro slot 0 every other macro slot that
  keeps content (rule 6); the message the CLI prints says so
  (`ui::FIRST_CTRL_C`), and the second Ctrl+C warns that macros not yet
  written again after slot 0 stay empty. A Ctrl+C
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
  forms, the egress allowlist and read/write disjointness;
  `crates/openwraith-protocol/tests/write_captures.rs` checks that the
  builders produce the captured packets byte for byte (c02, c03, c08-c13),
  including a read-modify-write of the keymap that equals the vendor site's
  write and the c09 chunks that differ only by the site's forced knob
  defaults; `write_captures_v02.rs` does the same for the v0.2 forms
  (c13 macro, c16 DKS / Mod-Tap / Toggle and their bindings, c17 Fn-layer
  codes and custom colours, c15 SOCD bursts and pushes, c14 travel open,
  close and stream).
- `crates/openwraith-hid/tests/{keymap_edit,edits_v02,socd,travel,import,macro_slot0,keymap_group_cancel}.rs`
  run the v0.2 editors, the travel guard and the import against
  `DeviceSim`: the captured packets, the group order and its commit
  trigger, placement refusals, slots, Ctrl+C at every table boundary,
  failures part-way (dangling bindings), the persistence check (`Auto`,
  and `Lost` with `KeymapCommit::macros_need_fn_write`), the SOCD toggle on
  the keyboard during a write, the travel test closed on every path
  (panics included) and stray streams, undo by restore. Regression tests
  for the hardware session H12-H24: `macro_slot0.rs` (the `macro-exp.txt`
  experiment, the re-sends after slot 0 in edits, restores and imports,
  Ctrl+C after slot 0, a failed re-send naming the emptied macros and the
  dangling keys, a base keymap the keyboard drops (its Fn keymap failed,
  or the persistence check found it lost) naming the key it leaves on a
  slot slot 0 emptied, a slot-0 write cut short after its first packet
  listing the macros it emptied, a slot 1-9 that cannot be written again
  blocking a restore of slot 0 and only warned about in an edit the user
  allows, the allocation from slot 1, the H24 multi-profile restore and
  its undo); `keymap_group_cancel.rs` (a Ctrl+C after a macro slot sends
  no Fn keymap, so no key is bound to a macro slot the Ctrl+C skipped; a
  macro slot a layer newly refers to goes out with that layer); in
  `restore.rs` the H24 SOCD case (SOCD on in every profile, A / D only in
  profile 2, profile 4 active: one SOCD burst, the final check passes,
  every profile reads off), the power written once (in the original
  profile before a profile written anyway), the backup's active-profile
  power (also when that is not the first profile restored), an import that
  keeps the keyboard's power, a Fn + Right Shift press between two SOCD
  writes stopping the restore ("changed after planning") instead of being
  switched back silently, and a press after the write that the final check
  does not count as a mismatch; in `keymap_edit.rs` the H20 step 2 case (a lone `22` is lost,
  `18 02` then `22` is kept, a lone macro slot is kept); in `socd.rs` and
  `edits_v02.rs` the edits that no longer switch profiles to check SOCD,
  macro slots or colours.
- `crates/openwraith-hid/tests/` run sessions, summaries and full backups
  against `MockTransport` built from the captures (noise, lost replies and
  retries, partial tables, endless unrelated traffic, timeouts, stale input,
  the real reset notification, profile changes, unknown models, undecodable
  replies, JSON and plain-serde round trips, edited headers, atomic saves),
  and the writer, all-profile backups, restore and edits against `DeviceSim`
  (round trips, both scope configurations of the tables whose scope a test
chooses, ack
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
  persists, with either setting of `KeymapCommit::macros_need_fn_write` /
  `advanced_need_fn_write`;
  the same plan without the trigger (the old order, made in a unit test of
  `restore`) is reported lost by the persistence check, and so is the
  restore with the trigger when the commit settle is turned off (test-only
  option). Further tests: the group order (macros, `18 00`, `18 02`, `22`;
  with `advanced_need_fn_write` on, the default, the advanced-key clear
  after the trigger persists, and so does everything else), macro slots
  alone written without the Fn keymap (reported lost only with
  `macros_need_fn_write` on), advanced-key slots alone after the unchanged
  Fn keymap, no
  cancellation between the base and the Fn keymap, a Ctrl+C during the
  first macro slot (the base and Fn keymaps are skipped, the outcome
  `Cancelled`, the macro saved), a Ctrl+C after a macro slot never sending
  an Fn keymap that binds a key to a macro slot it skipped, and a macro
  slot that a written layer newly refers to going out with that layer
  (`tests/keymap_group_cancel.rs`, restores and edits), a Ctrl+C
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
  prints both). `src/sim_tests_v02.rs` runs the v0.2 commands the same
  way and compares their packets with capture session 3: `set key` (a
  swap, and the c17 Fn-layer codes), `set macro` (c13, the binding, the
  re-sends after slot 0, `--commit-trigger skip` kept), `set
  advanced` (c16 DKS, Mod-Tap, Toggle, a lone slot write lost under
  `--commit-trigger skip` at the byte offsets H20 reported, unbinding),
  `set colors` (c17), `set deadband`, `set socd` (four c15 bursts),
  presets, dry runs (reads only), `travel` (c14 open and close, Ctrl+C, a
  profile switch, JSON, a test left open and `travel --close`), `monitor`
  (sends nothing at all), `profile export` / `import`, and the re-tests
  H26 and H27 command by command as HARDWARE_TESTS.md lists them
  (`hardware_tests_h26_and_h27_in_the_simulator`; `cargo test -p
  openwraith-cli sim_tests_v02 -- --nocapture` prints the outputs).
- `crates/openwraith-cli/tests/cli.rs` runs the real binary on backup files
  replayed from the captures or made with the simulator (`--from`,
  `restore --dry-run --against`), including crafted files with edited headers
  and escape sequences, plus help texts and argument errors.
- No test enumerates or opens a HID device. Keep it that way: hardware tests
  must be run by hand by someone who has the keyboard. The checklist (H0-H11
  for Phase 2, H12-H25 for the v0.2 editors, H26-H27 for the fixes after
  them: preconditions, exact commands, what to compare, pass criteria) is
  [HARDWARE_TESTS.md](HARDWARE_TESTS.md).

## The desktop app

### Crates

| Crate | Contents |
|---|---|
| `crates/openwraith-app-core` | Everything but Tauri: the device worker and its connection state machine, the operations (prepare / apply), the IPC data types (`dto`; the TypeScript types are generated from them), the mappings from the library's types, the file registry, the operation journal, settings, the simulator backend (feature `sim`, or tests) and all Rust tests of the app. `#![forbid(unsafe_code)]`; `unwrap`, `expect` and `panic` are denied outside tests. |
| `app/src-tauri` (`openwraith-app`) | The Tauri glue: commands (`commands.rs`, names in `command_names.rs`), the core's sinks over `tauri::ipc::Channel` (`sinks.rs`: progress, events and the live key-depth frames), file dialogs from Rust (`dialogs.rs`), the close guard and the focus check (`window.rs`), paths (`paths.rs`). |

`cargo test --workspace` needs no Node: the core's tests use the simulator
under `cfg(test)`. The Tauri crate builds without a built frontend
(`build.rs` writes a placeholder `app/dist/index.html` when it is missing,
and, in debug builds only, a placeholder `legal/THIRD-PARTY-NOTICES.txt`).
`cargo build` without `-p` builds the default members only (the CLI and the
libraries, Rust 1.85); the `--workspace` commands include the Tauri crate
(Rust 1.90).

### Threads

```
WebView --invoke--> command (async) --admission--> spawn_blocking(handle.call) --mpsc--> worker thread
        <-Channel-- ChannelProgress / ChannelEvents <------------ progress, status, notices <--+
direct commands (status, snapshot, cancel, files) read shared state; they never wait for the worker
```

One thread (`openwraith-device`) owns the backend (hidapi is created inside
it) and the session, and runs one operation at a time. **Admission:** an
operation command sets the busy flag (compare-exchange) before it queues the
request; a second operation fails at once with `operation_running` and is
never queued. The worker clears the flag after the reply. Cancellation is a
flag per operation that every library progress callback turns into
`ControlFlow::Break`. A panic in a handler or a tick drops the session (the
keyboard is closed) and enters `Fault`: writes stay blocked until a restart.
Nothing leaves `Fault`: `reconnect`, `select_device` and `disconnect` are
refused with `writes_blocked` (the worker's state after an unwound panic is
not trusted). A panic releases the busy flag only when the worker took it
itself (its own reads after a profile change or a focus check), so a
request admitted meanwhile keeps its admission and the window can still be
closed. "Stop safely and close" is checked wherever an operation ends:
after every request, after the worker's own reads and after every tick; the
exit hook runs once.

### Connection state machine

```
Starting -> Searching --1--> Opening -> Observing (2 s, listen only) -quiet-> Reading -> Ready
              |  >1 -> SeveralKeyboards -select-^      | status report / foreign ack -> OtherProgram
              |                                        | profile reports only -> rearm, keep observing
              +-- Lost (auto: same keyboard back, or exactly one) <- transport error / no longer listed
              +-- OpenFailed (backoff 2/5/10/30 s)     Reading fails -> ReadFailed (manual retry)
              +-- Unsupported (after 0D)  ResetSeen (manual reconnect only)  Released  Fault
```

- **Observing.** After opening, the worker only listens for 2 s
  (`WriteOptions::default().gate_observation`) before its first request: the
  vendor site acts on every `21/11` reply it sees, so nothing is sent until
  the keyboard is known to be quiet. This also satisfies the writer's gate,
  so later writes start at once.
- **OtherProgram.** Status reports or acknowledgements we did not cause:
  the worker sends **nothing**, reads included. Every full read (connect,
  refresh, the safety read before a write, the reads after a switch or a
  restore, a backup of the active profile) goes through
  `read_all_observed`: before each request what arrived is classified
  (nothing is sent for that), and the read stops once another program was
  heard, so at most the request in flight when its report arrived is
  completed, and nothing at all is sent when the evidence was there before
  the read. The focus check and the view refresh after an edit (the edited
  keys, the light) stop the same way. The snapshot is marked
  stale, the plan invalidated and writes blocked until "check again"
  (`Session::rearm_gate`, observe again, read everything again). The gate
  is re-armed only when profile reports are the *only* activity since the
  gate mark: a read (connect, refresh, a re-read, the safety read) that
  hears another program and then a profile change ends with
  `other_program_active` and the state `OtherProgram`, never with a retry
  that would drop the evidence. After an operation whose library call heard
  another program, the worker refreshes nothing (no key or light re-read,
  no full read with its `21/11`): the state becomes `OtherProgram` at once
  and the snapshot is stale (after a profile switch it is dropped, since it
  describes the old profile).
- **Profile changes on the keyboard.** Only profile reports, and still only
  profile reports 500 ms later: the gate is re-armed, the plan invalidated,
  the settings read again and a notice sent (A9: the W75 reports Fn + N at
  once, PROTOCOL Q2). A silent switch, should another firmware not report it,
  is caught by a single `21/11` read when the window gets the focus after
  30 s without traffic, and in any case by the drift check of every write.
- **ResetSeen.** The reset notification drops the session at once; nothing
  is sent and nothing reconnects until the user asks for it.
- Every new session increments the generation; plans and snapshots carry it.
- Before every request and every tick the worker updates the state from
  what the session saw (`keyboard_reset_seen`, `connection_lost`, foreign
  activity), so nothing is ever sent on a session that saw a reset.

### Keeping the keyboard open (decision)

The worker keeps one session open while connected. On Windows hidapi opens
the interface with shared access and every handle receives every input
report (Linux hidraw allows several opens), so the open handle blocks no
other program. While idle the app sends nothing; it only listens, which
gives live detection of the vendor site, of resets, unplugs and profile
switches, an input queue that never overflows, the commit-settle memory of
the session across writers, and a gate that is already satisfied. "Release
keyboard" closes the handle; "Connect" opens it again. macOS (later) opens
HID devices exclusively by default: use hidapi's shared mode there, or
release the keyboard while idle.

### Prepare, confirm, apply

Every write is two commands. **Prepare** reads (generic over `Transport`
where it talks to the keyboard, so it cannot reach a writer), validates the
intent against the snapshot the user saw (`based_on`) and stores the plan
behind an opaque single-use token; at most one plan exists. The plan is
invalidated by a new connection, a reset, another program, a profile change,
any other operation, `discard_plan`, a reloaded window (`subscribe`), and
after 10 minutes. **Apply** takes the plan (it is gone even when a check
fails), checks the token's kind and generation and the write gate, listens
for the rest of the 2 s observation (`CheckingOtherPrograms`), reads the
profile again for the safety backup, compares it with the plan (**drift
check**: another profile or other bytes give `stale_plan`, and nothing is
saved or written), saves and verifies the safety backup, writes the
**operation journal**, and only then calls the library. The consent
(`WriteConsent::new(Application)`) is built in one function
(`ops/consent.rs`) and used only by the apply steps and by the steps the
user started explicitly: the restore's and the import's prepare step, whose
comparison switches profiles after the user confirmed those switches
(`confirmed_switches` must equal the switches computed from a fresh
`21/11`), the export of a profile that is not active (switch agreed), the
live key-depth view and the close of a key-depth test left open (both a
user click).

| UI action | Library calls | Writes |
|---|---|---|
| connect / refresh / focus check | `read_all_observed`, `Session::read` | none |
| trigger edit | `read_summary` (unit 0.02 mm), `plan_trigger_edit`; `apply_trigger_edit` (`WriteOptions::default()`) | W8, one key per packet |
| light edit | `plan_light_edit`; `apply_light_edit` | W3 (+ W2 on a mode change) |
| profile switch | `Session::writer` + `Writer::select_profile_observed` | W1 |
| backup of the active profile | `read_all_observed`, `Backup::save` | none |
| backup of all profiles | `read_all_profiles` | W1 only |
| restore (and Undo) | `prepare_restore`; `save_safety`, `execute_restore` | as planned by the library |
| keys, macros, advanced keys (`keymap_edit`) | `KeymapGroupState::from_snapshot` (the cached group, no traffic), `plan_keymap_edit`; `apply_keymap_edit` | the keymap group, in the library's order (macro slots incl. re-sends after slot 0, base, Fn, advanced slots) |
| key colours (`color_edit`) | `plan_color_edit`; `apply_color_edit` | W4 (+ W2/W3 when the user opts into Custom) |
| dead band (`dead_band_edit`) | `plan_dead_band_edit`; `apply_dead_band_edit` | W9 |
| SOCD (`socd_edit`) | `plan_socd_edit`; `apply_socd_edit` | W11 |
| trigger preset (`trigger_edit`) | `presets::preset_edit` -> the trigger plan; `apply_trigger_edit` | W8 (no write path of its own) |
| live key depth (`depth_view`) | `travel::live_travel` (`CancelPolicy::Always`) | T1, T2 |
| close a test left open (`travel_close`) | `Writer::close_stray_travel_test` | T2 |
| export a profile (`export`) | `read_all_observed` / `read_profiles` (switch consent), `Backup::export_profile` | W1 only (another profile) |
| import a profile (`import_prepare`, `import`) | `prepare_restore` with `ImportOptions`; `save_safety`, `execute_restore` | as planned by the library |

Every edit applies the same way: a drift check on exactly the tables the
plan writes (the keymap group: both layers, every macro slot and `22 80`;
SOCD: `24 01` / `24 02`), the safety backup (reasons `set-keys`,
`set-colors`, `set-deadband`, `set-socd`, `import`; the backup page names
them), the journal, the library call, then a read of what was written
(after a keymap edit or an import, a full read). A keymap plan is made
against the keymap group of the last full read without talking to the
keyboard; the apply step's drift check and the library's re-read of each
table before writing it keep that safe. Stop after the first keymap-group
table makes the library finish what keeps the group consistent (the Fn
keymap after the base keymap, bound slots, macro slots re-sent after slot
0); the progress shows `finishing_keymap_group` meanwhile, and the report
says when Stop came too late (`cancel_requested` with outcome `completed`).
Every edit report is the shared `EditReportView` (now with the persistence
check, `cancel_requested`, the macros emptied by a failed slot-0 write and
the keys left on them); an import report is the restore report
(`LastReport::Import`, shown in import words).

Nothing else may write. A source-scan test (`tests::source_rules`) checks:

- no app code uses `write_guarded`, `write_commit_trigger`, `write_command`
  (the simulator backend aside), `WritePacket`, `WriteRequest`,
  `TriggerGrouping` / `SameSetting`, `CommitSettle`, `SessionOptions`,
  `writer_with`, hand-built `WriteOptions`, `HidTransport` or `unsafe`;
- no app code builds or patches a table, a slot or a raw entry, or decodes
  raw entries past the library's views (`set_entry`, `KeymapData`,
  `MacroData`, `CanonicalMacro`, `AdvancedSlotData`, `SocdWrite`,
  `SocdPairsData`, `TravelTestRequest`, `ColorTableData`, `DeadBandData`,
  `DeadBandWrite`, `from_read_slot`, `KeyAction::decode*`,
  `MacroProgram::decode`), and none leaves out the Fn keymap that makes the
  keyboard keep a group (`OmitForSlotsOnly`, a hardware-test mode);
- the intents the frontend sends carry no bytes, hex or paths (keys by
  index, colours as `#rrggbb`, values in trigger units);
- only the profile switch creates a writer, and `ops/travel.rs` one more,
  used for nothing but `close_stray_travel_test`; `live_travel` and
  `close_stray_travel_test` are called only in `ops/travel.rs`, and
  `.profile_switcher(` (standing rules, Phase 5b-2) only in
  `ops/auto_profile.rs`;
- each editor's apply calls its own library entry point, and the preset
  path ends in the trigger plan;
- the consent is built once and used in exactly 14 places (the apply steps
  and the explicitly started steps above);
- hidapi is opened only by the HID backend.

**Editor views and revisions.** Every full read (connect, refresh, after a
keymap edit, a restore or an import, a profile switch) caches the views of
the editors besides the snapshot: the keymap group (`get_keys_state`) and
the colour table, the dead band and SOCD (`get_colors_state`,
`get_dead_band_state`, `get_socd_state`). These commands never talk to the
keyboard (`null` before the first read). Each view has its own `revision`,
which its intents send as `based_on`: an edit of one part never changes
another part's revision, so a colour draft survives a dead-band write, and
trigger or light edits leave a Keys draft valid. A trigger preset uses the
snapshot's revision like a trigger edit. The keyboard's own SOCD switch
(Fn + Right Shift pushes `24 02 .. PP`) updates the SOCD view, sends
`Notice::SocdToggledOnKeyboard` and invalidates a pending plan made against
the old power: a SOCD edit, a restore that compared the SOCD table, or an
import that writes it (an import keeps the keyboard's power). Other plans
stay valid. The push is never taken for another program.

**Live key depth.** `start_depth_view(channel)` holds the busy flag while
it runs (every other operation is refused with `operation_running`; the
write gate shows `busy`) and returns when the view stopped, with a
`DepthViewReport`. Frames (`DepthFrame`: the keys that moved, with depth
and held peak in trigger units) are coalesced to at most 60 a second and
sent only on movement; the keyboard sends about 14 reports a second on a
slow press. It stops on `stop_depth_view` (Stop, the window hidden, the tab
or page left), app exit (the window closes after the close, without the
close guard), a reloaded window (`subscribe`) or a failed channel, 5
minutes without movement, 60 minutes, a reset, a lost connection, another
program and a profile switch on the keyboard; every stop also sends
`Notice::DepthViewStopped`. The handle registers the view at its
admission, so a stop sent before the worker starts it (a tab switched at
once) is kept and the test is never opened. Before T1 the worker writes the
marker `<state dir>/travel-test-open` and removes it only after a close it
sent was acknowledged (T2); a start refused before anything was opened
leaves the marker as it was (an earlier run's stays, its own goes). A
marker found at start, or stream
reports while no test is open, set `DeviceStatus.travel` (`marker_left`,
`stray_reports`; `maybe_open` after an unacknowledged close) and send
`Notice::StrayTravelTest`; `close_stray_travel_test` runs on a user click
only. Travel samples never enter the session's events.

**Export and import.** `choose_export_destination` and `open_import_file`
open the Rust dialogs and hand out file tokens. An export is
`Backup::export_profile` (scope `profile_export`, no USB serial; the whole
profile, since the library has no category filter yet); the active profile
is a pure read, another profile needs `confirmed_switch`. An import is a
restore with `ImportOptions`: the chosen categories (triggers and lighting
by default), into a chosen profile (`confirmed_switches` when it is not
active), a critical Fn override (Fn + 1..4, Fn + Space) only when every
such key is named in `allowed_critical_keys`. The library stops at the
first critical key, so the app finds them all (`critical_fn_overrides`, in
the blocked and the allowed preview alike, each with its own danger note)
and refuses a permission that doesn't cover the file (`stale_plan`). Keys
the library blocks for another cause (an entry, macro or slot OpenWraith
doesn't write) replace nothing, so the list is empty then;
`other_cause_blocked` marks every chosen part left out for a cause the
permission doesn't lift (the review's general note). An
import token is a different plan kind from a restore token, so each apply
refuses the other.

### Files

Paths never cross the IPC boundary. Files are known by token
(`FileRegistry`); tokens come from the Rust file dialogs, the safety-backup
listing, operation results and the journal. Safety backups go to the CLI's
folder (`OPENWRAITH_SAFETY_DIR`, else `%LOCALAPPDATA%\OpenWraith\safety-backups`)
and are never deleted; every report carries its safety backup, and Undo
restores it through the normal restore flow. The **journal**
(`<app local data>/state/operation-journal.json`) is written atomically
after the safety backup and before the first write packet of every write
(a trigger, light, keymap, colour, dead-band or SOCD edit, a restore or an
import), and removed when the report is built. A
journal left behind is moved at the next start to
`state/interrupted-operation.json` and shown with its safety backup at
every start until the user dismisses it: later writes use
`operation-journal.json` again and never overwrite or delete it, and
"Dismiss" deletes only the interrupted one (never the journal of a write
that runs). A newer interruption replaces an older one that was never
dismissed. A journal that cannot be written stops the operation before
anything is written.

`open_safety_backup(file)` loads a safety backup, and also summarizes again
any backup file token the app handed out (a safety backup or a file the user
opened; not a backup destination). A summary compares the file with the
keyboard read last (`same_model`, `same_firmware`), so the Backup & Restore
page summarizes the chosen file again after every new read of a keyboard; a
file opened while no keyboard was read is compared once one is. The restore
itself checks model and firmware again when it is prepared. A backup file
is read from disk every time it is summarized or a restore is prepared
(there is no cache): a file overwritten after it was opened (a new backup
under the same name, the CLI) is never planned or written from an older
copy, and a deleted file is an error, not a plan.

| What | Where (Windows) |
|---|---|
| Safety backups | the CLI's folder: `OPENWRAITH_SAFETY_DIR`, else `%LOCALAPPDATA%\OpenWraith\safety-backups` |
| Operation journal | `%LOCALAPPDATA%\io.github.zeruh31.openwraith\state\operation-journal.json`; an interrupted one: `interrupted-operation.json` next to it |
| Settings (language, theme, trigger presets, tips shown) | `%APPDATA%\io.github.zeruh31.openwraith\settings.json` (unknown fields are kept; an unreadable file is kept as `settings.invalid-<stamp>.json` before it is replaced) |
| Macro library ("this PC") | `%LOCALAPPDATA%\io.github.zeruh31.openwraith\macro-library.json` (matched to keyboard slots by content hash; an unreadable file is kept as `macro-library.invalid-<stamp>.json`) |
| Key-depth test marker | `%LOCALAPPDATA%\io.github.zeruh31.openwraith\state\travel-test-open` (a live view that may still be on) |
| WebView2 data | `%LOCALAPPDATA%\io.github.zeruh31.openwraith\EBWebView\` |
| Simulator safety backups, journal, marker and macro library | `OPENWRAITH_SIM_DIR`, else `%LOCALAPPDATA%\io.github.zeruh31.openwraith\sim\` |

### Closing

A close (or an exit asked for by the user) while an operation runs is
deferred, and the frontend shows the close guard. "Stop safely and close"
sets the cancel flag and exits when the operation has returned (the library
has waited for the save and returned to the original profile by then). When
idle, the app exits normally and the worker closes the keyboard.

### Simulator and browser preview

`npm --prefix app run dev` builds with the feature `sim`: the backend is the
library's `DeviceSim` (seeded from capture c06; profiles 2-4 with factory
triggers and light) behind a transport that can be unplugged, reset, slowed
down and paced in real time (`OPENWRAITH_SIM_SPEED`). Its safety backups and
journal live under `<app local data>/sim/`. A release build with `sim` does
not compile (`compile_error!`), and a simulator build contains no HID code.
The core's tests drive the same worker synchronously in virtual time. In the
app, the header's "Simulator" badge opens the simulator controls (unplug,
replug, vendor traffic, keyboard profile switches, reset, slow save, dropped
reply, unplug after N packets) and shows the simulator's counters.

The browser preview replays fixtures (`app/src/preview/fixtures/*.ts`) that
the core's tests record from the real worker on the simulator (statuses,
snapshots, events, and command calls with their progress). They are typed
TypeScript literals, so `tsc` checks them against the generated bindings.

`npm --prefix app run dev:preview` (`vite --mode mock`) installs
`@tauri-apps/api/mocks`' `mockIPC` before the app starts. `preview/mockBackend.ts`
answers each command as follows:

- it uses the fixture's next recorded call of that command (the last one
  again when they are used up);
- it replays the recorded progress on the real `Channel` through
  `__TAURI_INTERNALS__.runCallback`, the same ordering path as the real IPC;
- it keeps a small status model, so that busy states, profile switches and
  banners behave like the app's;
- after the fixture's last call, it replays the recorded events, so the app
  ends in the recorded final state (events recorded before the first call,
  such as a connection that hears a key-depth test left open, are replayed
  at `subscribe`);
- the live key-depth view replays the frames the fixture recorded
  (`ui.depth_frames`, since a fixture has no field for a channel) at their
  recorded times, holds the busy flag and the "open" travel state, and
  resolves with the recorded report when the page stops it (or at once for
  a stop the backend decided);
- the macro library of "this PC" and the tips shown are kept in memory,
  starting from the recorded ones.

Commands that a fixture didn't record are answered from its state,
synthesized (profile switch, reconnect, simulator actions), or borrowed from
another fixture (`fallback=0` turns that off); the editors' views answer
`null` while the fixture has no keyboard read. Each page composes its
scenarios in `preview/scenarios/<page>.ts` (a fixture, a route with its
`?tab=`, and UI state the page reads with `previewUi`, e.g. a seeded draft
that opens the review). Only `main.tsx`'s
`MODE === "mock"` branch imports the preview. The production build drops it,
and `npm run build` fails if `dist/` contains preview code. The "Preview"
button in the header's middle opens the scenario picker; `freeze=1` hides it
for screenshots.

### Builds and modes

| Build | How | Keyboard | Simulator commands |
|---|---|---|---|
| Release | `npm --prefix app run tauri build` | hidapi; opens a connected keyboard at start (listening first) | not compiled, not registered, not allowed |
| Developer simulator | `npm --prefix app run dev` (debug, `--features sim`, `tauri.sim.conf.json`) | `DeviceSim`; no HID code compiled | registered; allowed by the `dev-sim` capability |
| Developer, real keyboard | `npm --prefix app run dev:hid` (maintainers) | hidapi | not compiled |
| Browser preview | `npm --prefix app run dev:preview` (Vite, no Rust) | recorded fixtures through `mockIPC` | answered by the mock |

A release build cannot enter simulator mode:

- `#[cfg(all(feature = "sim", not(debug_assertions)))] compile_error!` in
  both `openwraith-app-core` and the Tauri crate, so
  `cargo build -p openwraith-app --release --features sim` fails;
- the simulator backend, its controls and the `dev_sim_*` handlers are
  behind `cfg(feature = "sim")`; `OPENWRAITH_SIM_*` variables are read only
  there;
- `tauri.conf.json` enables only the `main` capability; only
  `tauri.sim.conf.json` adds `dev-sim`, and it sets `bundle.active = false`;
  `tests::commands_sync` checks both files and that only `tauri dev` scripts
  use the simulator.

The release build's Rust code contains no simulator backend, no `dev_sim_*`
handler, no `OPENWRAITH_SIM*` variable and no capture. Two kinds of `sim`
text remain in the executable: the tool name `openwraith-app-sim` (to
recognize backups made by the simulator), and the names of the
`dev_sim_*` commands inside Tauri's embedded permission manifest
(`build.rs` declares a permission for every command, so that the
simulator's capability can refer to them). The release build registers no
`dev_sim_*` command and no enabled capability grants those permissions, so
an invoke of one is refused. The embedded
frontend (compressed inside the executable) renders the simulator panel
only on the Vite dev server (`import.meta.env.DEV`, which `npm run dev` and
the preview use), and its commands live in a separate `devApi`, so the
production bundle contains neither; `npm run build` fails if `dist/`
contains `dev_sim_`. The i18n dictionaries still hold the simulator's
strings (the badge and the banner render only when the backend reports
`sim`).

**Runtime check.** The simulator build was started with WebView2's remote
debugging and driven through the Chrome DevTools protocol: the window boots
under the minimal capability set (only `allow-<command>`, no `core:`
permission), `invoke` and the `Channel`s work (status, progress with the
save wait), and a light edit, its Undo (a restore with the persistence
check) and the safety-backup list ran end to end on the simulator.

### Installer

`tauri build` makes two installers from the release build:

- **NSIS** (`OpenWraith_<version>_x64-setup.exe`): per user (no
  administrator rights) into `%LOCALAPPDATA%\OpenWraith\`, English and
  Turkish (chosen from Windows' language). Its uninstaller deletes only the
  files it installed (the executable and itself) and then the folder if it
  is empty, so the CLI's `safety-backups` folder next to it is never
  touched; its optional "delete app data" removes only
  `%APPDATA%` / `%LOCALAPPDATA%\io.github.zeruh31.openwraith`.
- **MSI** (`OpenWraith_<version>_x64_en-US.msi`): per machine, English.

Both download Microsoft's WebView2 bootstrapper only when WebView2 is
missing; the VC runtime is linked statically. The installers are not signed
yet (SmartScreen warns on the first start).

- **Metadata.** `bundle.publisher` is "OpenWraith contributors" (without it
  Tauri takes the identifier's second part, "github", as the publisher of
  the installers, the uninstall entry and the executable's version
  resource), `bundle.homepage` is the repository, `bundle.copyright` a
  copyright line, the NSIS installer has the app's icon, and the MSI
  upgrade code is pinned (`faa01cb5-c09e-5745-8cee-10ba5a17f5a6`, the value
  Tauri derived from the product name), so renaming the product can never
  break upgrades. `tests::commands_sync` checks all of them.
- **Licenses.** Both installers ship `LICENSE.txt` (the GPL) and
  `THIRD-PARTY-NOTICES.txt` in the installation folder (`bundle.resources`);
  Settings > About shows the copyright, the no-warranty statement and a
  button that shows both files. `app/scripts/third-party-notices.mjs` (part
  of `npm run build:web`) writes the notices: every crate linked into the
  app (`cargo metadata`, normal dependencies for the host target) with the
  license files it ships, or its SPDX expression, authors and a standard
  text when it ships none; where the source of the MPL-2.0 crates is; and
  the frontend packages from Vite's `build.license` output. A release build
  without the notices fails (`build.rs`); a debug build gets a placeholder.
- **Release command.** `pwsh tools/release-build.ps1` runs `tauri build`
  with `--remap-path-prefix` for the user profile, the Cargo home and the
  repository (through `CARGO_ENCODED_RUSTFLAGS`, for that build only), then
  fails if the executable still contains the user-profile path. Rust embeds
  source paths (panic locations); without the remap every installer would
  carry the builder's Windows user name.

### The frontend

`app/src` is React 19 and TypeScript (strict). It uses no UI, state, router
or i18n library.

- **IPC.** `ipc/commands.ts` is the only importer of `@tauri-apps/api/core`
  (ESLint `no-restricted-imports`). It holds one `api.*` function per command,
  and every rejection is an `AppError`.
- **State.** `state/` holds small `useSyncExternalStore` stores:
  - the status, from the `subscribe` channel (an older revision never
    replaces a newer one);
  - the snapshot, refetched when `snapshot_revision` changes;
  - the settings, the toasts and the per-page drafts;
  - the local operation lock, taken in click handlers so that a double click
    never opens two flows. It only disables and guards the controls; the
    "busy" texts come from the backend's status (a review dialog that is
    merely open is not "Another operation is running"). A flow releases it
    in the event that closes it, so the opener is enabled again when the
    dialog returns the focus to it (the dialog also retries for a few
    frames, then falls back to the page's main region, never the body);
  - the vendor-site acknowledgement, required for the first write of a run
    and again after any `OtherProgram` episode. A tick belongs to the
    episode it was given in, so a dialog that stayed open across an episode
    asks again; the restore's switch-consent dialog asks for it too (its
    comparison switches profiles), and Retry or "Review again" of a
    comparison that switches profiles goes back to that dialog when the
    tick is needed again.
- **Writes.** Every write goes through `flow/WriteFlow` (prepare, review,
  Write, progress, result); a page can word a refused prepare itself
  (`prepareError`, e.g. the keymap and editor refusals) and add review
  options that discard the plan and prepare again (`reviewOptions`, e.g.
  the persistence check or "also switch the effect to Custom"). The review's title asks the question ("Change
  the lighting of profile 1?"); while it runs and on the result the dialog
  has a neutral title ("Lighting change"). A page can add a line under a
  failed apply (`errorNote`, e.g. "No backup file was saved."):
  - Cancel has the initial focus. Write is armed after 500 ms, and Enter
    never activates it.
  - Each confirmation applies once.
  - Write is disabled while `write_gate` is closed (the reasons are shown)
    and after a `plan_invalidated` notice ("Review again" prepares anew).
  - `WaitingForKeyboardSave` is shown as a countdown meter. Stop follows the
    `CancelState`.
  - Read-only operations with progress use `OperationRunner`. Edit reports
    use the shared `EditReportView` and `ReportFooter` (safety backup, Show
    in folder, Undo = `#/backup?restore=<token>`).
  - The review's notes (safety backup, vendor site, the vendor tick, the
    firmware and save-wait notes, the write-block reasons) sit in the
    dialog's footer, between the scrolling plan and the buttons, so they are
    always in view.
  - A report is never lost: when a flow goes away while its operation runs
    (a reloaded window, a page that unmounted), the shell's
    `LastReportDialog` fetches `get_last_report` once the status shows the
    operation ended and shows the report with its safety backup and Undo.
    F5 / Ctrl+R are refused while a flow watches a running operation.
    There "Run the restore again" is a hint (the backup is no longer
    loaded), and a write that ends after its page went away still spends
    its draft (the page's apply call clears it).
- **Pages.** Pages are lazy routes (`#/device`, `#/keys`, `#/actuation`,
  `#/lighting`, `#/backup`, `#/settings`), each in its own error boundary.
  Keys, Actuation and Lighting have tabs (`?tab=`, `routes.ts` `PAGE_TABS`;
  an unknown tab falls back to the first): Keys `assign`, `macros`,
  `advanced`; Actuation `trigger`, `deadband`, `socd`, `depth`; Lighting
  `effect`, `colors`. The tab bar (`shell/PageTabs`) is a row of links with
  `aria-current` (each tab is a URL), injected under the page header, with a
  dot on each tab that has a draft. The W75 drawing is
  `components/keyboard`:
  - our own key-unit geometry, from PROTOCOL.md section 5;
  - a pure selection model, where the knob is never selectable;
  - a multi-select listbox component. Each key's accessible name starts
    with its visible text: the legend (followed by the full name in
    parentheses when the legend is not the full name, e.g. "ESC (Escape)",
    "SPACE (Boşluk)"), the value and the chip, separated by spaces; longer
    text is its accessible description. Pages mark keys through `KeyVisual.flag` (`data-flag`) and
    `KeyVisual.tooltip`, and draw on caps with `sublegend`, `swatch` (a
    colour, always also in the description), `fill` (key depth) and
    `badge`; `selection: "single"` (`aria-multiselectable=false`),
    `onActivate` (Enter or a double-click) and a knob popover serve the Keys
    tabs.
- **Shared editor components** (`components/`): `Badge` / `EvidenceBadge`
  ("Not yet verified" only from the library's `EvidenceView`, "Kept as
  read", "Applies to all profiles"), `ActionPicker` (combobox + listbox with
  `aria-activedescendant`; searches English and Turkish names, library
  names, `KeyboardEvent.code` aliases and hex; a shortcut builder of one or
  two modifiers and a key), `KeyPicker`, `ColorPicker` (S/V square and hue
  as `role="slider"`), `StepList` (Alt + Up/Down moves a step),
  `TravelGauge` (with a text alternative), `ConfirmCheckbox` (never
  pre-ticked) and `Tip` (a one-time tip, remembered in `hints_shown`;
  Settings shows them again). The action catalogue (every assignable
  action with its evidence, the DKS grammar, the Mod-Tap rule) and the
  macro recorder's `KeyboardEvent.code` table are generated from Rust into
  `app/src/generated/`.
- **Keys page.** `pages/keys/shared` holds the keys-state store (refetched
  when `snapshot_revision` changes), the intent builders, the words for
  actions and slots, and `KeymapEditFlow`: the review (Layer | Key | Now |
  New, then the writes in execution order, each explained by its own
  fields: the commit trigger, a re-send after macro slot 0, a binding
  written even after Stop), the refusals answered inside the flow (a
  critical Fn built-in, a slot kept as read) and the optional persistence
  check (never pre-ticked; ticking it prepares again).
- **Header and banners.** The header shows a **Live** pill while the
  key-depth view runs (`busy.kind === "depth_view"` or
  `travel.state === "open"`); it links to the tab that stops it, and the
  busy reason everywhere names the view. A banner leads to the Key depth
  tab when a test may still be open (`travel.state === "maybe_open"`,
  stray reports or the marker), except on that tab, which shows its own
  "Stop it"; the close is sent there, after the vendor-site tick.
- **Drafts.** Unwritten changes register with `state/drafts` per page and
  tab, with a count (the nav dot is any tab's draft; the profile-switch
  confirmation lists "Keys > Macros: 1 unwritten change"):
  `registerDraft(page, discard, {tab, count})` for a draft kept in a module
  store that survives leaving the page (every editor tab, Actuation with
  its key selection), `useDraft` for one that lives in a page component.
  The stores follow their view themselves (the owner is the connection's
  generation and the profile): a draft made on another profile or
  connection is discarded (with a toast) whether its page is shown or not,
  and never comes back; a newer read of the same profile drops only the
  changes the keyboard already has. The Keys tabs keep absolute targets
  (what a key should end up doing), so a draft survives a re-read; the key
  colours draft has its own undo history.
- **Live key depth** (Actuation > Key depth). Start needs the vendor-site
  tick, like a write; the tab stops the view with `page_left` when it
  unmounts and with `window_hidden` on `visibilitychange`, shows why the
  backend stopped it ("Paused…" with Resume after 5 minutes without
  movement), and is silent for screen readers unless "Announce the depth"
  is on (at most four times a second), with a hidden table of the depths.
- **Layout.** `.ow-main` is the only scroll container and the containing
  block of absolutely positioned descendants (`position: relative`), so the
  window itself never scrolls and the header stays in place. A page's
  sticky apply bar sets `--applybar-height` on it (a `ResizeObserver`), and
  its `scroll-padding-bottom` keeps a focused control above the bar
  (WCAG 2.4.11). In Windows high contrast, colour swatches keep their
  colour (`forced-color-adjust: none`) with a `CanvasText` boundary.
- **i18n.** `i18n/en/*.ts` are the shape source. Every `i18n/tr/*.ts` is
  typed `Messages<typeof en>`, so a missing key is a compile error. Tests
  check that Turkish is complete and that no `.tsx` contains UI literals.
- **Theme.** `theme/tokens.css` holds the tokens. The theme follows the
  system unless `data-theme` overrides it. A test checks WCAG contrast
  (4.5:1 for text, 3:1 for boundaries) in both themes.

### Fitness tests

| Test | Checks |
|---|---|
| `tests::connection`, `tests::gate` | observation, other programs (zero packets), profile changes, the focus check, unplug and replug, reset (zero packets until reconnect), several keyboards, open backoff, unsupported model, drift check, stale and single-use tokens, panic -> fault |
| `tests::foreign` | another program heard during a read is never re-armed away (refresh and the first read); a read stops sending once another program is heard (connect, refresh, the safety read: at most the request in flight), and sends nothing when it was heard before; the focus check and the view refresh after an edit stop the same way; after a trigger, light, keymap, colour, dead-band or SOCD edit, a profile switch or a restore that heard another program, nothing more is sent; each v0.2 editor checks for other programs before its first write |
| `tests::fault` | nothing leaves `Fault`; a panic in the worker's own read releases the busy flag, a panic in a tick keeps a queued request's admission; "Stop safely and close" during the worker's own read exits once |
| `tests::trigger`, `tests::light`, `tests::profile`, `tests::backup`, `tests::restore` | the operations end to end on the simulator: one key per packet, the journal during the write, the wait for the save, persisted / lost / may be lost, cancellation (keymap group included), the consent echo, Undo |
| `tests::admission` | a second operation is refused at once; close deferral; the interrupted-operation journal (kept by later writes until dismissed; dismissing never deletes a running write's journal) |
| `tests::keymap` | keys, macros and advanced keys end to end: consent and single-use tokens, the drift check on the group tables, re-sends after macro slot 0 (also after Stop), a failed write with emptied macros and dangling keys, critical Fn built-ins, the persistence option, the journal, swap, deletes before the changes that reuse a key's slot, a new slot on a key whose slot the edit also changes refused, refused intents |
| `tests::editors` | colours, dead band, SOCD and presets: own revisions (a write of one leaves the others' drafts valid), the drift checks (colours, dead band, SOCD power and pairs), the SOCD journal, refusals before traffic (never clamped; preset names 1-40 characters, at most 32 presets), the opt-in switch to Custom, the keyboard's SOCD push (only a plan made against the SOCD power is invalidated: SOCD edit, restore comparing SOCD, import writing SOCD; tests in `tests::restore` and `tests::share` too), the SOCD mode evidence against the library |
| `tests::travel` | the live view: frames coalesced and sent only on movement, every stop reason (time limit, lost connection and a reloaded window included), a stop sent before the worker started the view, the marker written before T1 and removed only after an acknowledged close (a refused start keeps an earlier run's), a marker left at start, stray reports, the close on a user click |
| `tests::share` | export (no traffic for the active profile, switch consent for another) and import (chosen categories into a chosen profile, every critical Fn override named and each needing the permission, import and restore tokens not interchangeable) |
| `tests::interleavings` | 150 seeded random sequences of commands and keyboard events (SOCD edits and pushes, stray streams and live views included) against the write-path invariants (`INTERLEAVING_SEEDS=5000` runs more) |
| `tests::source_rules`, `tests::commands_sync` | the source rules above; command names, argument names and progress channels (against the `#[tauri::command]` signatures), capabilities, CSP, installer metadata and license files, and the simulator's configuration and scripts |
| `tests::bindings`, `tests::fixtures` | the generated TypeScript (bindings, W75 keys, the action catalogue, the `KeyboardEvent.code` table, fixtures) is up to date (`UPDATE_BINDINGS=1` / `UPDATE_FIXTURES=1` rewrite them) |
| frontend (Vitest) | per tab: the draft to the exact recorded intent, review, write and report on the recorded fixtures; the DKS grid round trip for every grammar-valid row; the recorder's code table; catalogue search in English and Turkish; tab draft dots; i18n parity, no JSX literals, WCAG contrast of the tokens (badges and keycaps included) |

### Security

The window's capability (`capabilities/main.json`) allows exactly the app's
commands (`allow-<command>`, generated by `build.rs` from
`command_names.rs`) and no core or plugin permission: dialogs, the file
manager and single instance are used from Rust only. The simulator's
commands are registered only in simulator builds and allowed only by
`capabilities/dev-sim.json`, which only `tauri.sim.conf.json` enables. The
CSP allows nothing but the app itself and the IPC (`connect-src ipc:
http://ipc.localhost`); `withGlobalTauri` is off and the prototype is
frozen. No network access, no telemetry, no remote content. The VC runtime
is linked statically into release builds only (the static variant puts an
empty `msvcrt.lib` on the link path of the whole workspace build, which
breaks the other crates' doctests in debug test runs).

## Deferred (library API for the desktop app)

Done in Phase 2: serialisable error kinds (`ErrorKind`, `ErrorInfo`),
cancellable progress (`read_all_cancellable`, `WriteProgress`,
`ProfilesProgress`, all `ControlFlow` based), serialisable plans and reports.

Also done: `read_all_observed` (a read that stops as soon as another
program is heard; the desktop app's full reads use it).

Also done: `execute_restore` uses the options the restore was prepared with
(`PreparedRestore::options`), `WriteProgress::Writing` carries `send` (2
while a table is sent a second time), and a long-lived session can re-arm the
writer's gate (`Session::rearm_gate`).

Done for the desktop app (Phase 3): a serialisable device list
(`DeviceListing`, `DeviceDescriptor::listing`) and the only-one-device rule
(`choose_device`, `DeviceChoiceError`); the CLI keeps its own
`device::choose` for its messages.

Done in v0.2 part A (library and CLI): `EditError` (with `kind()`) for the
new editors; keymap, macro and advanced-key editing (`KeymapGroupState`,
`plan_keymap_edit`); custom colours, the dead band and SOCD editors; the
SOCD push as `DeviceEvent::SocdPower` and the travel stream as
`DeviceEvent::TravelStream`, neither of them counted as another program
(a stray stream refuses table writes instead); `24 00 00 00 00` known to
`is_write_ack` (another program's SOCD write is foreign activity; our own
`24 03`, never seen acknowledged, is listened for with
`AckSpec::Possible`); the travel guard and `live_travel`; the
`ProfileSwitcher`; profile export and import; the egress allowlist; the
capture tests of session 3 (`write_captures_v02.rs` loads c13-c17 by name,
outside `CAPTURES`).

Still open:

- a user-file summary command of its own: `open_safety_backup` summarizes
  any backup file token for now (see "Files" above);
- `Deserialize` for the library's intents (`TriggerEdit`, `LightEdit`,
  `RtEdit`, `ProfileSet`, `RestoreOptions`, `WriteOptions`, `KeymapEdit`):
  the app uses its own validated mirror intents (`openwraith_app_core::dto`);
- `plan_trigger_edit` / `plan_light_edit` still report problems as
  `RestoreError` (`InvalidRequest`, `SafetyBackupMismatch`);
- multi-key trigger masks (`TriggerGrouping::SameSetting`, CODE-ONLY until
  hardware test H10 in [HARDWARE_TESTS.md](HARDWARE_TESTS.md));
- the evidence the hardware session H12-H24 left open (PROTOCOL.md 7.5):
  the `Evidence` values that stay unverified (multimedia indices other than
  `12`, `24`, `25` and `32`; a DKS tap at stage 4 and "hold again"; Mod-Tap
  hold times other than 300 ms; advanced-key bindings on the Fn layer; SOCD
  modes 2 and 3), the labels of `W75_FN_BUILTINS` other than F1-F4, 1, 2
  and R-Shift (`FnBuiltin::confirmed`), and the simulator's GUESSes (the chunk
  of a slot-0 macro write that clears slots 1-9, an `18 02` written after
  advanced-key slots, `set_travel_survives_select`); flip them one result
  at a time. Wheel down and two-modifier combinations count as confirmed by
  c17's blanket report and were not tested again; H26, H27 and H13 step 4
  are still to run;
- a SOCD push that arrives exactly while our own `24 02` read is pending is
  taken as the reply (it carries the current state); during the
  verification of a SOCD write such a toggle could go unnoticed;
- the content of keymap entries and colour slots is checked by the editors
  and the import (`classify`, `check_placement`), not by the egress
  allowlist: `KeymapData::set_entry` and `ColorTableData::set_color` stay
  public (the hid crate needs them), so a library caller using
  `Writer::write_guarded` directly, or a plain restore of a hand-edited
  backup file, can still write any entry category or the knob slots;
- `WriteTransport::write_command` is a public trait method; a caller holding
  a transport bypasses the gate and the allowlist (nothing in the workspace
  calls it outside `Session::send`).

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
