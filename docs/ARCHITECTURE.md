# OpenWraith architecture

This document explains how the code is organised, how the read-only guarantee
of Phase 1 is enforced, how data flows from the keyboard to a backup file, and
how to add another keyboard model. The protocol itself is described in
[PROTOCOL.md](PROTOCOL.md); section numbers below ("spec 2.8") refer to it.

## Crate layout

```
openwraith-cli  (bin "openwraith")      commands, text/JSON output
      |
openwraith-hid                          transports, session, backup
      |
openwraith-protocol                     pure protocol: no I/O
```

A future Tauri app will sit next to the CLI and use the same two libraries.

### `openwraith-protocol` (no I/O)

| Module | Contents |
|---|---|
| `frame` | `REPORT_ID`, `REPORT_LEN` (63), `MAX_CHUNK_PAYLOAD` (58). `OutPacket` (no public constructor) and `Report` (any 63-byte input report; tolerates byte-4 quirks and stale bytes, spec 2.3 / 2.4). |
| `request` | `ReadRequest`: one variant per read command of the spec, all CONFIRMED. Each knows its packet bytes, its `ResponseSpec` (how replies are recognised, spec 2.3 / 2.8), its chunk layout and its timeout. `full_read_plan` lists everything a backup reads. |
| `response` | `ResponseCollector` (accepts matching chunks, ignores the rest, flags a second copy of a chunk with different data as `Offer::Conflict`, detects completion) and `RawResponse` (a complete, validated reply; the lossless unit of a backup). |
| `decode` | Typed models for every reply (`DeviceInfo`, `Status`, `LightEffect`, `ColorTable`, `KeymapTable`, `MacroSlot`, `KeyTrigger`, ...). Every model keeps its raw bytes. Field docs repeat the spec's confidence labels. |
| `event` | Classification of unsolicited reports: keyboard reset (`14 .. 01 01`), profile reports, status pushes. |
| `layout`, `model` | Key positions (`KeyPos`, index = row x 22 + col), key masks, the W75 key table and the `Model` description (spec 5). |
| `units` | `Distance` in trigger units of 0.02 mm. |
| `capture` (feature) | Parser for the JSON capture files and the request/reply pairing used by tests and the mock transport. |

### `openwraith-hid`

| Module | Contents |
|---|---|
| `transport` | `Transport` trait: `write(&OutPacket)`, `read(timeout) -> Option<Report>`, `usb_info()`. |
| `hid` | `HidBackend` (hidapi; enumerates and filters supported interfaces, nothing is opened) and `HidTransport` (an open configuration interface). |
| `session` | `Session<T: Transport>`: one request at a time, reply collection, filtering, timeout and retry, conflict detection, reset detection. |
| `summary` | `read_summary` / `DeviceSummary`: the few reads behind `openwraith info` (and a GUI's device page), with the model check shared with `read_all`. |
| `backup` | `Backup` (file format), `read_all` / `read_all_with`, save/load, typed accessors. |
| `mock` (feature `mock`) | `MockTransport`: replays the captures; can add noise, inject unsolicited reports and drop replies. |

### `openwraith-cli`

| Module | Contents |
|---|---|
| `main` | clap definitions, the four subcommands, friendly error messages with hints. |
| `device` | Choosing and opening the keyboard (`--device`, several-keyboard handling). |
| `view` | Serialisable views over a `Backup` or a `DeviceSummary` (used for both text and `--json`); `JsonOutput` adds the confidence `note` and the list of undecoded replies to every JSON output. |
| `render`, `table` | Text output. Values whose meaning the spec labels CODE-ONLY or GUESS are marked `*`, with one legend line per command. Text from the device or a backup file is stripped of control characters before it is printed. |

## The read-only guarantee

Phase 1 must not be able to send anything that changes the keyboard. This is
enforced by types, not by convention:

1. **Only `OutPacket`s can be sent.** `Transport::write` takes an `&OutPacket`
   and nothing else. `OutPacket` has no public constructor; the only way to get
   one is `ReadRequest::packet()`. A `compile_fail` doctest checks that
   `OutPacket([0x14; 63])` does not compile.
2. **`ReadRequest` contains only reads.** Its variants are the read commands
   labelled CONFIRMED in spec section 3. Parameters are range-checked types
   (`KeyPos`, `MacroSlotId`), so an invalid request cannot be built. A unit
   test enumerates all 163 constructible requests and checks each packet
   against the documented read forms (no `0x14`, no `0x17`, no write
   sub-commands, byte 6 of the poll-rate read stays 0, ...).
3. **No raw access.** `HidTransport` keeps its hidapi handle private, and no
   API in the workspace accepts raw bytes for sending. Captured packets are
   kept as plain byte arrays precisely because they can be writes.
4. **Reset notifications are never answered.** When the keyboard sends
   `14 .. 01 01` (Fn + Space held, spec 4.6.3), the session returns
   `SessionError::KeyboardReset` and sends nothing. Tests inject the real
   notification from capture c07 and assert that no further packet was
   written.
5. **The CLI has no write subcommand.** A test checks that `set`, `write`,
   `restore`, `reset` and `calibrate` are rejected by the argument parser.
6. **Tests check what was sent.** The hid tests assert that every packet the
   mock received during a full backup is a read request and never a `0x14`.

Phase 2 (restore and editing) will need a separate, deliberately designed
write path with explicit user confirmation and read-modify-write (spec 1.2).
It must not loosen the rules above for the read path.

## Data flow

### Reading from the keyboard

```
openwraith backup
  -> device::open_session
       HidBackend::new()            enumerate HID devices
       HidBackend::devices()        keep VID 0x2E3C, PIDs C365-C368,
                                    usage page 0xFF1B, usage 0x91
       HidBackend::open()           -> HidTransport
  -> read_all_with(session)
       DeviceInfo   (0D)            model check: product code must be known
       Status       (01)            capability flags select optional reads
       ReadRequest::full_read_plan  113 requests on the W75
       ActiveProfile (21/11) again  fails if the profile changed
  -> Backup::save                   JSON in a unique temporary file, synced,
                                    then moved into place (never replacing
                                    an existing file without --force)
```

`openwraith info` uses `read_summary` instead: device info (with the same
model check), status, active profile, global trigger parameters and the
polling rate.

For each request, `Session::request`:

1. drains stale input reports (another program's replies, old pushes);
2. writes `request.packet()`;
3. reads reports and offers each to the request's `ResponseCollector`.
   Matching uses byte 0, byte 1 (or byte 5 for the `0x21` family) and the
   row/col echo for per-key triggers, never arrival order (spec 2.8 rule 2);
4. ignores unrelated reports and counts them: heartbeats and pushes,
   acknowledgements of another program's writes, profile reports (recorded as
   events). Unrelated reports never extend the deadline, and the drain stops
   after `max_drain` reports;
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

The README therefore tells users to close the vendor site first, and the CLI
warns after any device command that saw status reports it did not ask for or
an unsolicited profile report.

### Reading a backup file

```
openwraith show all --from FILE
  -> Backup::load
       check "format" and "schema_version"
       RawResponse re-validated (all chunks, every report matches its request)
       a request stored twice is refused
       decoded views, device header and active profile rebuilt from the raw
       reports
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
- `scope` / `note` state that only the active onboard profile is included,
  plus the tables whose profile scope is unverified (custom colours, polling
  rate, switch types; spec 4.5.4, 6.9, 6.10, Q3). The `scope` value
  `active_profile_and_global_settings` is kept for compatibility.

## Testing

- `crates/openwraith-protocol/tests/captures.rs` decodes every reply in the
  seven captures and checks known facts (factory triggers, W key settings,
  reset behaviour).
- `crates/openwraith-protocol/tests/decode_fuzz.rs` decodes a few hundred
  header-valid replies with arbitrary lengths and contents for every
  constructible request (fixed seed): decoders must never panic.
- `crates/openwraith-hid/tests/` run sessions, summaries and full backups
  against `MockTransport` built from the captures: noise interleaving, lost
  replies and retries, a retry that completes a partial table, a table that
  changes between attempts, endless unrelated traffic (drain cap and
  deadline), timeouts, stale input, the real reset notification, profile
  changes, unknown models, undecodable replies, JSON and plain-serde round
  trips, edited headers, atomic saves.
- `crates/openwraith-cli/tests/cli.rs` runs the real binary on backup files
  replayed from the captures (`--from` only), including crafted files with
  edited headers and escape sequences.
- No test enumerates or opens a HID device. Keep it that way: hardware tests
  must be run by hand by someone who has the keyboard.

## Deferred to Phase 2 (library API for the desktop app)

The Phase 1 API is enough for the CLI. Before the Tauri app is written these
need a deliberate design, driven by the app's actual needs:

- serialisable, machine-readable error kinds (timeout, keyboard reset, not
  found, busy, ...) for command results;
- device selection in the library (only-one-device rule, open by path) and a
  serialisable device list (`DeviceDescriptor` holds a private `CString`);
- a cancellable progress callback for `read_all_with` (for example
  `ControlFlow` plus `BackupError::Cancelled`).

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
   code from the `0x0D` string, PIDs, key table, knob and Fn index) and append
   it to `MODELS`. `model_for_product` then recognises it, and
   `ReadRequest::full_read_plan` reads its keys.
4. **Units.** `Distance` assumes 0.02 mm per unit (the W75 reports
   `rawUnit` 2 in `21/04` and no high-precision flag). If the new model reports
   another unit, extend `Distance` before showing millimetres; the CLI already
   warns when the unit differs.
5. **Tests.** Add capture-based assertions for the new model (device info,
   key count, a known trigger value) like the W75 ones, and update
   `PROTOCOL.md` with anything that differs, with confidence labels.
