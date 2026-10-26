# Wraith W75 HID Protocol Specification

Reverse engineered for interoperability from the official browser configurator (https://wraith.software/, WebHID) and from USB captures of the maintainer's own keyboard. This document describes behaviour in our own words. It contains no code from the site.

| Item | Value |
|---|---|
| Device under test | Wraith W75, PID `0xC368`, firmware `V3.22.07` (build `Aug  6 2026,13:46:54`) |
| Site build analysed | v1 UI (`v1.html` + `wmIndex.js` + `agreement.js`); wraith.software always serves v1 |
| Spec revision | 2026-10-24 (replaces the earlier draft; corrections are listed in Appendix A) |
| Status | Work in progress. Read the confidence label on every row before relying on it. |

---

## 0. Conventions

### 0.1 Confidence labels

Every table row and every non-trivial statement carries one of these labels.

| Label | Meaning |
|---|---|
| **CONFIRMED** | The site code builds or parses it this way **and** a real W75 capture shows the same bytes. |
| **OBSERVED** | Seen in a W75 capture, but the site code neither builds nor parses it (capture-only). |
| **CODE-ONLY** | Taken from the site code, HTML or JSON configs. Not seen on the wire, or seen only as a constant/uniform value that cannot distinguish the field. |
| **USER-REPORTED** | A test the maintainer ran by hand that is not in the capture files. |
| **GUESS** | A hypothesis. Never treat it as fact. |

"Confirmed (framing)" means the packet framing and sizes are confirmed while the meaning of the contents is CODE-ONLY.

### 0.2 References

- **Code** citations such as `agreement.js:1798` and `wmIndex.js:6606` refer to the deobfuscated site scripts (`scratchpad/site_deob/`). `v1.html`, `index.html` and `config/*.json` refer to the raw site files (`scratchpad/site_raw/`). Line numbers are those of the deobfuscated files.
- **Captures** are in `captures/`:
  - `c01` = `capture_01_connect.json` (connect and initial read, no user change)
  - `c02` = `capture_02_light.json` (contains all 900 entries of c01 verbatim, then a lighting change after MARK `BEFORE_LIGHT_CHANGE` at +86475 ms)
  - `c03` = `capture_03_actuation_W.json` (fresh page load and connect, then W actuation 1.00 mm and Rapid Trigger 0.30 mm)
  - c01 and c02 are **one** connect sample. c01 and c03 are the two independent connect samples.
- **Times** are milliseconds after the capture's `OPEN` entry (written `+1811` or `t=1811`). OPEN timestamps: c01/c02 `1792809086131`, c03 `1792809574699`. `capN#i` is entry index i.
- **Capture format:** a JSON array of entries `{t, dir, ...}`. `dir` is `OPEN` (with `vid`, `pid`, `name`, `collections`), `OUT`/`IN` (with `id` = report ID and `data` = 63 space-separated hex bytes of payload), `MARK` (with `label`) or `CLOSE`.

### 0.3 Byte numbering

- **Byte 0** is the first byte after the report ID. All offsets in this document use this numbering, which is also how the site indexes its arrays.
- `p[i]` means payload byte i = byte `5+i`.
- Multi-byte values are noted as `BE16` (big-endian) or as separate `lo`/`hi` bytes where the protocol splits them.
- `RR`, `PP`, `MM` etc. are placeholders explained next to each layout.

---

## 1. Overview and safety

### 1.1 What the protocol is

- The keyboard exposes a vendor HID collection (usage page `0xFF1B`, usage `0x91`) with one 63-byte input and one 63-byte output report, both report ID 1 (section 2).
- Every request starts with a command byte (byte 0) and a sub-command or table byte (byte 1). The `0x21` family selects its function in byte 5 instead.
- Tables (keymap, macros, colours, dead band, ...) are moved in chunks of up to 58 payload bytes.
- **The keyboard stores its configuration itself.** No save or commit command exists in the site, and settings written with `0x07 00` survived a USB replug (USER-REPORTED, section 6.1).
- **The keyboard has 4 onboard profiles** (CONFIRMED that the site runs in onboard-profile mode for the W75; section 4.5).

### 1.2 Safety rules for our application

These rules apply to the code we write. They are engineering policy, not protocol facts.

1. **Never send a DANGEROUS command without an explicit confirmation from the user in the UI**, and offer a backup (full table read) first.
2. **Never write a whole table from a model we built ourselves.** Read the table, change only the entries the user edited, write it back (read-modify-write). The site rebuilds tables from a lossy in-memory model and destroys data it does not understand (section 6).
3. **Never probe unknown command bytes** on the user's keyboard.
4. **Never reproduce these site behaviours:**
   - the empty-mask trigger write the site sends on every connect (`21 00 00 00 18 00` + zeros; section 4.11.4);
   - running a reset when an input report `0x14` with byte 5 = `01` arrives (section 4.6.3);
   - the `resetTrigger` toggle that silently skips every other trigger reset (section 4.11.6);
   - forcing knob default codes into keymap slots 16-21 on every keymap write (section 4.10);
   - zeroing colour slots that are not in our key layout (section 4.7.6).
5. Serialise requests: one outstanding request at a time, never interleave two multi-chunk writes (section 2.8).

**Command risk table**

| Command | Effect | Level | Confidence of effect |
|---|---|---|---|
| `14 00 00 00 01 01` | Reset current profile (scope not verified) | **DANGEROUS** | CODE-ONLY |
| `21 00 00 00 18 06` | Reset all key triggers of the active profile | **DANGEROUS** | CONFIRMED (c11) |
| `21 00 00 00 18 0F 00` / `10 00`, `08 00` / `08 01` | Start/stop calibration. An interrupted or careless calibration can leave keys misbehaving until recalibrated (GUESS) | **DANGEROUS** | CODE-ONLY |
| `21 00 00 00 18 07 <row> <col>` | Single-key calibration. Dead code in the site, never exercised | **DANGEROUS** (do not implement) | CODE-ONLY |
| `14 03 00 00 01 MM` | Gamepad mode. The UI says the device restarts; PID probably changes | **DANGEROUS** | CODE-ONLY |
| `21 00 00 00 00 09 01 RR` | Poll rate. The UI says the device restarts | CAUTION | CODE-ONLY |
| `14 00 00 00 01 00 ...` | Win-Lock/6-key options. **Byte 5 must stay `00`**; `01` turns it into the reset above | CAUTION | CODE-ONLY |
| `18 00` / `18 02`, `19 0n`, `22 nn`, `26 01`, `25 01`, `24 00`, `09 00` / `09 01` | Whole-table writes | CAUTION: read-modify-write and back up first | CONFIRMED for `09 00`, `18 00` / `18 02`, `19 0n`, `26 01` and `22 nn` with zero content (capture session 2); CODE-ONLY for `25 01`, `24 00`, `09 01` and non-empty `22 nn` |
| Input `14 ... [5]=01` | The site treats it as "reset everything" and sends a reset | Hazard on the **receive** side: ignore it | CODE-ONLY |

### 1.3 Firmware update is out of band

- The site checks `config/firmware.json` for a newer version of the product and offers a download of a `.rar` archive labelled "Firmware Update Tool" from `config/firmware/<file>` (`wmIndex.js:242-320`). The brief describes the tool inside as a Windows `.exe`; we have not examined the archive.
- **No HID command in the site performs a firmware update.** Firmware update is not part of this protocol and is out of scope for our application. CODE-ONLY.

---

## 2. Transport and packet framing

### 2.1 Device selection

| Item | Value | Confidence |
|---|---|---|
| Vendor ID | `0x2E3C` | CONFIRMED (filter `brandConfig.js:105-134`; c01 OPEN `vid` 11836) |
| Product IDs | `0xC365`, `0xC366`, `0xC367`, `0xC368` (shared by W60, W65, W75, W100) | CONFIRMED for `0xC368` (c01/c03 OPEN `pid` 50024); others CODE-ONLY |
| Collection | usage page `0xFF1B`, usage `0x91`; output report 1, input report 1, no feature report | CONFIRMED (c01/c03 OPEN `collections`) |
| USB interface | interface 2 (`MI_02`) on Windows | USER-REPORTED (earlier draft) |
| USB product string | `Wraith W75` (OPEN `name`). The site does not use it | OBSERVED |

**The model is identified by the `0x0D` info string, not by the PID.** CONFIRMED.

- The site sends `0x0D`, takes comma field index 4 (0-based) of chunk 0, `W75HEARGB`, and looks it up in `config/device.json` by VID, PID and product (`wmIndex.js:6582-6668`).
- `device.json` lists the Wraith family under company `mile_wraith`, each model on all four PIDs:

| Model | Product code | Canvas size in device.json |
|---|---|---|
| W60 | `AP61HEARGB` | 587 x 207 |
| W65 | `EM65QMZCHMHEARGB` | 648 x 218 |
| W75 | `W75HEARGB` | 648 x 264 |
| W100 | `GK108USQMZCHMHEARGB` | 881 x 267 |

**What the PID means** (CODE-ONLY / GUESS):

- The site pre-selects the gamepad mode from the PID: `0xC366` = Xbox 360, `0xC367` = Classical, any other = Disabled (`wmIndex.js:6093-6106`, `6380-6386`). Gamepad keymap codes are only shown on the matching PID.
- GUESS: the firmware re-enumerates with a different PID per gamepad mode (`0xC368` = keyboard only, the user's board). The meaning of `0xC365` is unknown. `0xC365` appears in 271 `device.json` entries from many vendors, while `0xC366`/`C367`/`C368` occur only as a triple (GUESS: `0xC365` is a generic/legacy PID).
- The only W75-relevant PID special case: the base-layer keymap write skips the knob default codes on `0xC365` (`agreement.js:1154`).
- Not the W75: `0xC370`/`0xC371` get battery and sleep-timer UI (`wmIndex.js:5902-5909, 5983-6011`); `0xC345` hides Actuation, SOCD and advanced keys (`wmIndex.js:5939-5981, 6061-6064`).

**Our app:** open VID `0x2E3C` with usage page `0xFF1B` / usage `0x91`, accept PIDs `0xC365`-`0xC368`, send `0x0D`, require field 4 = `W75HEARGB`.

### 2.2 Reports

- One report ID (1) in each direction, 63-byte payload, no feature reports. CONFIRMED: every OUT and IN in c01-c03 is 63 bytes with ID 1.
- hidapi (engineering note): write 64 bytes (`0x01` followed by the 63-byte payload). Reads of a numbered report return the report ID first on every hidapi backend; strip it so offsets match this document.
- Always send a full, zero-padded 63-byte payload. The site's `getSleepTimer` builds only a 6-byte buffer (`agreement.js:1772`); that a padded report is equivalent is a GUESS, and the W75 never sends this command anyway.

### 2.3 Packet header

| Byte | Field | Notes | Confidence |
|---|---|---|---|
| 0 | cmd | Command | CONFIRMED |
| 1 | sub / table | Table reads mostly use `0x80+n`, the matching writes use `n` (`18`, `19`, `22`, `09`). | CONFIRMED |
| 2-3 | chunk index | **Byte 3 = chunk index** (values 0..`0x27` seen). The site treats bytes 2-3 as a **16-bit big-endian** index: writes put `idx>>8` in byte 2, readers compute `[2]<<8 \| [3]` (`agreement.js:267, 332, 657, 812, 862, 879, 1174-1175, 1310` and others). Byte 2 is 0 in every capture. | byte 3 CONFIRMED; 16-bit BE CODE-ONLY |
| 4 | payload length | Number of valid bytes from byte 5, at most 58 (`0x3A`). Some requests declare a smaller length than the data they carry (section 2.4). | CONFIRMED |
| 5..62 | payload | Bytes after `5+len` are padding. The device leaves **stale buffer data** there (`09 80` chunk 7, `25 02` chunk 2), so always honour byte 4. | CONFIRMED / OBSERVED (stale data) |

**Byte 1 echo.** The device echoes byte 1 in its replies, except: `07 01` -> reply `07 00`, `08 01` -> `08 00`, and `09 20` -> `09 SS` (byte 1 carries the active custom slot). CONFIRMED (c01 +1134, +1138, +1142; all other pairs echo).

### 2.4 Requests whose byte 4 does not match their data

| Request | Declared byte 4 | Data actually at | Confidence that the firmware ignores byte 4 |
|---|---|---|---|
| Get active profile `21`/`11` | 0 | byte 5 | CONFIRMED (device answered: c01 +1769 -> +1811, c03 +1738 -> +1781) |
| Per-key trigger write `21`, mode at byte 5 | `0x18` (24 -> bytes 5..28) | bytes 5..35 | GUESS. Acked, but no read-back after the writes exists yet (capture plan C2 resolves it) |
| Win-Lock/6-key `14 00` | 1 | bytes 5, 12-15, 18 | CODE-ONLY |
| Sleep timers `14 01` / `14 02` | 0 | bytes 8-11 | CODE-ONLY |
| Set poll rate `21`/`09` | 0 | bytes 5-7 | CODE-ONLY |
| Select profile `21`/`12` | 0 | bytes 5-6 | CODE-ONLY |

- These are consistent (length matches data): reset `14 00 .. 01 01`, gamepad `14 03`, poll-rate read `21 .. 01 09`, SOCD power `24 03`, music rhythm `17` (length 2), advanced-key writes `22` (length 50).
- Several `0x21` sub-functions declare `0x18` while using only 1-3 bytes. Harmless.
- **Recommendation:** reproduce the site's bytes exactly, including its byte-4 values.

### 2.5 Request/response model

- **No sequence numbers, no checksums.** The site has one input handler (`readHidData`, `agreement.js:11-898`) that routes each IN report by byte 0 (plus byte 1 or byte 5 for some commands) and sets a per-command "done" flag; the requesting function polls that flag. CODE-ONLY. Before the model is identified a temporary handler deals only with `0x0D` chunk 0 (`wmIndex.js:6618-6648`).
- **Latency** (c01, c03):
  - Single-packet reads: 0-1 ms median, up to 9 ms (c01). CONFIRMED.
  - A request can wait behind another command's multi-chunk stream: c03 heartbeat at +2723 waited 19 ms behind `18 80` chunks; c02 heartbeat at +163749 took 111 ms during the `0x09` flood. One c02 heartbeat (+165764) got **no reply at all**. OBSERVED.
  - Per-key trigger writes are acked after 4-7 ms (c03). OBSERVED.
  - `21`/`11` answers after 42-43 ms (c01, c03). CONFIRMED. Cause unknown.
- **Multi-chunk replies:** one request makes the device stream every chunk back to back, 0-3 ms apart. Each chunk carries its own index (byte 3) and length (byte 4); the last chunk is shorter. CONFIRMED.
- **Pipelining:** the site sometimes sends a second request before the first reply arrives (c01 +1080 `24 02` then `24 01`; +1142 `09 20` then `09 80`; +2756 heartbeat between per-key reads; +3394 trigger write and read in the same ms). Replies came back in request order. CONFIRMED. Our app should still serialise.
- **Unsolicited reports:**
  - An unsolicited `0x01` status report followed the `0x17` ack in both captured leaves: c02 (`17` at +137930, ack +137943, status +137946) and c07 (`17` at +118182, ack +118194, status +118199), each 3-5 ms after the ack and each just after a mode-changing `07 00` write. OBSERVED (n=2). GUESS: the firmware pushes status when the music-rhythm state is set. These are the only two `0x17` packets in the captures; later `07 00` writes (same mode, no `0x17`) caused no push.
  - Handled in the site code but never seen (CODE-ONLY): `0x21` byte 5 = `01` (live travel stream), `0x21` byte 5 = `08`/`0F` (calibration status), `0x14` byte 5 = `01` (reset notification), `0x21` byte 5 = `11` (profile change).

**Reply sizes per request** (CONFIRMED by byte 4 values in c01/c03)

| Request | Chunks x length | Total | Content | Site's completion condition |
|---|---|---|---|---|
| `0D 00` | 2 x 50 | 100 | device info | only chunk 0 parsed |
| `18 80` / `18 82` | 9 x 56 + 24 | 528 = 132 x 4 | keymap base / Fn | chunk 9 (`agreement.js:477`) |
| `19 80`..`19 89` | 4 x 58 + 24 | 256 | one macro slot | chunk >= 4 or length < 58 (`agreement.js:1357`) |
| `22 80` | 40 x 40 | 1600 | advanced keys, one chunk per slot | chunk 39 (`agreement.js:659, 796`) |
| `24 01` | 2 x 40 | 80 | SOCD pairs | chunk 1 (`agreement.js:837`) |
| `25 02` | 58 + 58 + 16 | 132 | per-key switch type | page 2 (`agreement.js:873`) |
| `26 00` | 4 x 58 + 32 | 264 = 132 x 2 | dead band | page 4 (`agreement.js:892`) |
| `09 80` / `09 81` | 7 x 54 + 18 | 396 = 132 x 3 | custom colour table 1 / 2 | chunk 7 (`agreement.js:283`) |

Single-packet replies (byte 4): `01` 18; `0E` 22; `0A` 18; `02` 4; `07` 12; `08` 12; `09 20` -> `09 00` 0; `24 02` 6; `25 00` 3; `0x21` with byte 5 = `04` 6, `05` 12, `09` 2, `11` 3. CONFIRMED.

### 2.6 Write chunking

| Write | Table size | Chunk payload | Pacing in the site | Confidence |
|---|---|---|---|---|
| `18 00` / `18 02` | 528 | 56 -> 10 packets (last 24) | none | CONFIRMED (c09, c13; Fn layer with zero content only) |
| `19 0n` (n = 0-9) | 256 per slot | 58 -> 5 packets (last 24) | after each chunk waits <= 10 zero-ms timer ticks for an input `0x19` with byte 1 < `0x80` | CONFIRMED (c09, c13) |
| `09 00` / `09 01` | 396 | 54 -> 8 packets (last 18) | 5 ms sleep after each chunk (`agreement.js:1725`) | CONFIRMED for `09 00` (c02) |
| `25 01` | 132 | 58 -> 3 packets (58, 58, 16) | none | CODE-ONLY |
| `26 01` | 264 | 58 -> 5 packets (last 32) | none | CONFIRMED (c12) |
| `24 00` | 80 | 2 packets, byte 4 = 40 | none | CODE-ONLY |
| `22 nn` (nn = 0-39) | 1 packet per slot, byte 4 = 50 | - | none | CONFIRMED with all-zero content (c09, c13); non-empty content CODE-ONLY |

`0x09` pacing on the wire (OBSERVED, c02): an isolated frame shows 7-8 ms gaps (+160405..+160460). While the colour ring is dragged the site starts new frames before old ones finish, so chunks of different frames interleave (chunk index sequence 0,0,1,0,1,2,0,1,2,3,... from +160978) and the median OUT gap is 0 ms.

### 2.7 Acknowledgements

**General rules** (CONFIRMED, capture session 2): an acknowledgement never carries a chunk index (byte 3 is `00` in every ack), and unless stated otherwise it has length 0. A chunk can therefore only be matched to its ack by sending one chunk at a time and waiting for the ack before the next (stop-and-wait). Match acks on bytes 0-4 (plus byte 5 for the `0x21` family) and ignore the stale bytes after the declared length. Several acks do **not** echo byte 1 of the write.

| Write | Device reply | Latency | Confidence |
|---|---|---|---|
| `0x21` trigger writes | `21 00 00 00 01 00`. Byte 5 is `00` even after a `0x0C` write, so it is not a mode echo (GUESS: status OK). The site has no handler for it. | 2-7 ms, max 34 ms | OBSERVED (c01 +3394 -> +3400; c03 16 writes -> 16 acks; c10) |
| `21/12` profile select | `21 00 00 00 02 12 01`, then the push `21 00 00 00 03 11 PP 00` | 3-4 ms | CONFIRMED (c04, c08, c10) |
| `0x17` music-rhythm leave | `17 00 00 00 00`. The site does not wait for it: in both samples the next `07 00` is sent in the same millisecond the ack arrives. An unsolicited `0x01` status report follows 3-5 ms after the ack. | 12-13 ms | OBSERVED (c02 +137930 -> +137943; c07 +118182 -> +118194) |
| `09 00` colour chunks | `09 00 00 00 00` (length 0), within about 1 ms in isolated bursts. **1681 acks for 2064 chunks** in c02 (c02 has 1682 length-0 `0x09` replies; one of them is the `09 20` answer from the connect). The missing acks all fall in two flood bursts (+160978: 1 missing, up to 66 chunks outstanding; +164554: 382 missing, up to 448 outstanding); every isolated 8-chunk burst, and the 296-, 192-, 96- and 56-chunk bursts, is fully acked, which supports stop-and-wait. The ack is byte-identical to the `09 20` reply; the site's dispatcher also treats every such ack as "custom slot 1 selected" (`agreement.js:287-289`). | about 1 ms | CONFIRMED |
| `07 00` light write | **no reply** (10 writes, 0 replies in c02) | - | CONFIRMED |
| `18 00` / `18 02` keymap chunks | `18 00 00 00 00` for **both** layers: byte 1 is not echoed. 80 of 80 chunks acked. | 0-24 ms | CONFIRMED (c09, c13) |
| `19 0n` macro chunks | `19 00 00 00 00` for **every** slot: the slot is not echoed. 300 of 300 chunks acked. The site waits for an input `0x19` with byte 1 < `0x80` (`agreement.js:481-487, 1549-1552`), which this ack satisfies. | first chunk of a slot up to 28 ms, max 63 ms | CONFIRMED (c09, c13) |
| `22 SS` advanced-key slot | `22 00 00 00 00`: the slot is not echoed. 240 of 240 writes acked (all with zero content). The site does not wait, so acks queue up behind its pipelined writes. | up to 98 ms behind the site's queue | CONFIRMED (zero content) |
| `26 01` dead-band chunks | `26 01 00 00 00`: byte 1 **is** echoed. 185 of 185 chunks acked. | 0-4 ms | CONFIRMED (c12) |
| `0x14` writes | The site shows a "Success" toast on any `0x14` input (except byte 1 = 2) and clears its `isResetTrigger` flag (`agreement.js:320-330`). Reply format unknown. | - | CODE-ONLY |
| `24`, `25` writes | The site parses no reply. Unknown whether the device sends one. | - | - |

"Foreign" acks: an ack-shaped report that answers none of our writes (for example the ack of the vendor site's empty-mask trigger write at connect, section 4.11.4) shows that another program is writing to the keyboard. OpenWraith counts them and refuses to write while it sees them.

### 2.8 Site waiting behaviour, and the policy for our app

| Wait pattern in the site | Used by | Effective maximum (estimate) |
|---|---|---|
| <= 10 iterations of a 0-ms timer | `initInfo`, `readMaxTriggerTravel`, `readTriggerData` (per key), `readPollRate`, `setMacroValue` (per chunk), `readDeadBand` | about 10-25 ms (browsers clamp nested timers to about 4 ms) |
| <= 10 iterations of a 1-ms timer | `readLightList`, `readSideLightList`, `initLightValue`, `initSideLightValue`, `initCustomLightValue` | about 10-40 ms |
| Deadline, polling every 2 ms | `initKeyValue` 250 ms, `initFnKeyState` 200, `initMacroValue` 150 per slot, `initAdvancedKeys` 500, `initPrcsData` 200, `initSwitchList` 200, `initKeySwitch` 200 | as listed |
| <= 2000 iterations of a 1-ms timer | `0x0D` (`wmIndex.js:6653-6657`) | about 2-8 s |
| No wait at all | every write, plus heartbeat, `09 20`, `21/11` | - |

- **The site never retries.** On timeout it continues with whatever it has.
- Fixed sleeps: 5 ms after each `0x09` chunk; 100 ms after the trigger read and 50 ms after the keymap read in the profile reload (`agreement.js:584, 605`); 100 ms steps in the reset flows; about 900 ms of UI animation between `0x0D` and the first `0x01` (c01 0 -> +914).

**Policy for our app (engineering advice, not protocol fact):**

1. One outstanding request at a time. Drain stale input reports before each request.
2. Match replies on byte 0, plus byte 1 for tables and `0x09`, plus byte 5 for `0x21`. Match `0x21/05` replies on the row/col echo (bytes 15-16), not arrival order.
3. About 100 ms timeout for single-packet replies (allow about 150 ms for `21/11`). For tables, collect until the known last chunk index. Retry once (a heartbeat reply was lost in c02).
4. Tolerate unsolicited `0x01` reports at any time.
5. Never interleave two multi-chunk writes; throttle colour streaming (section 4.7.6).

### 2.9 Key index, key mask and distance units

**Key index** (CONFIRMED). 132 positions = 6 rows x 22 columns, **index = row x 22 + col**.

- The per-key trigger read carries row in byte 6 (0..5) and column in byte 7, and the device echoes both in reply bytes 15/16 (246 of 246 replies in c01 and c03).
- The W75 uses 82 positions (section 5). W = row 2, col 2 = index 46 (CONFIRMED by c03).
- Confirmed for: `0x21/05` reads, `0x21` trigger-write mask, `0x0E`, `0x09` colour tables (c02: the 81 slots the site painted are exactly the 82 read positions minus the knob at 15). CODE-ONLY for `0x18`, `0x25`, `0x26`, travel-test and calibration masks (captured tables are uniform).

**22-byte key mask** (used by `0x0E`, the `0x21` trigger write, travel-test open and calibration progress):

- Byte j (0..21) = column j, bit b (0..5) = row b, key index = b x 22 + j. Bits 6-7 unused.
- Confidence: CODE-ONLY orientation (`agreement.js:300-310, 544-552, 962, 1809`), consistent with captures. The c03 W write (byte 2 = `0x04`) cannot tell the orientation apart because W sits on the diagonal (row 2 = col 2). The `0x0E` reply (bit 5 set in byte **11**) is only possible with byte = column, since there are only 6 rows. A Q-only trigger write (capture plan C8) would confirm it for `0x21`.
- W75 mask of all 82 positions (used by the travel-test open): `3f 26 3f 1f 1f 1f 3f 1f 1f 1f 3f 3f 2f 31 3f 2f 00 00 00 00 00 00`.
- W75 mask of the 81 selectable keys (Select All, knob excluded): `3f 26 3f 1f 1f 1f 3f 1f 1f 1f 3f 3f 2f 31 3f 2e 00 00 00 00 00 00`.

**Distance units** (CONFIRMED):

- Travel, Rapid Trigger sensitivity and dead band are integers in *trigger units*. unit = `rawUnit / 100` mm, or `/1000` when the high-precision flag is set; rawUnit 0 is treated as 10 (`agreement.js:497-501`).
- W75: rawUnit = 2 (`21/04` reply byte 7) and high precision = 0 (status byte 18 bit 0), so **1 unit = 0.02 mm**. Examples: `0x55` = 85 = 1.70 mm, `0x32` = 1.00 mm, `0x0F` = 0.30 mm.
- Exception: DKS press points use **0.1 mm** units (CODE-ONLY, section 4.14).

### 2.10 Input dispatcher of the site

`readHidData` (`agreement.js:11-898`). Every branch requires report ID 1. Branches are independent, so one report can trigger several of them. CODE-ONLY (the branches that real replies hit are CONFIRMED by the matching captures).

| Byte 0 | Extra condition | Site effect | Lines |
|---|---|---|---|
| `0x01` | - | capability flags, Win-Lock bits (first reply only), 6-key, music, link token, battery | 19-72 |
| `0x0A` | - | main-light mode list, max speed, max brightness | 73-147 |
| `0x02` | - | side-light mode list, max speed, max brightness | 148-218 |
| `0x07` / `0x08` | - | main / side light effect record (12 bytes) | 219-264 |
| `0x09` | byte 1 = `0x80`/`0x81` | custom colour table 1/2, 18 keys per chunk | 265-285 |
| `0x09` | byte 1 = 0/1 | ticks custom slot radio 1/2 (also fires on every `09 00` write ack) | 286-293 |
| `0x0E` | - | Fn-layer non-remappable key bitmap | 295-313 |
| `0x14` | byte 5 = 1 | **runs the whole "reset" flow, including sending a reset** | 314-319 |
| `0x14` | any | clears `isResetTrigger`; byte 1 = 2: sleep timers; else "Success" toast | 320-330 |
| `0x18` | byte 1 = `0x80`/`0x82` | keymap base / Fn, 14 keys per chunk | 331-480 |
| `0x19` | byte 1 >= `0x80` / < `0x80` | macro read chunk / macro write ack | 481-487 |
| `0x21` | byte 5 = 1 | live key travel | 488-495 |
| `0x21` | byte 5 = 4 | max travel, unit, min travel | 496-515 |
| `0x21` | byte 5 = 5 | per-key trigger record | 516-519 |
| `0x21` | byte 5 = 9 | poll rate (only 1/2/4/8 accepted) | 520-525 |
| `0x21` | byte 5 = 8 or 15 | calibration status | 526-568 |
| `0x21` | byte 5 = `0x11` | active onboard profile, then a full reload. The else branch (634-640) is unreachable because byte 6 is unsigned. | 569-655 |
| `0x22` | byte 1 = `0x80` | advanced-key slot (chunk index = slot) | 656-799 |
| `0x24` | byte 1 = 2 / 1 | SOCD power / SOCD pairs | 800-841 |
| `0x25` | byte 1 = 0 / 2 | switch-type list / per-key switch type | 842-877 |
| `0x26` | byte 1 = 0 | dead-band pages (29 keys per page) | 878-895 |
| `0x0D` | byte 3 = 0 | device info (temporary handler) | `wmIndex.js:6623-6647` |

Not handled by the site: the `0x17` ack, the `0x21` write ack (byte 5 = 0), `0x0D` chunk 1, and any reply to `18`/`22`/`24`/`25`/`26` writes.

---

## 3. Master command table

Every command the site builds (`agreement.js`, plus `0x0D` from `wmIndex.js`; `wmIndex.js` has no other `sendReport` call), plus input-only reports the site handles. Direction: R = request with reply, W = write, IN = device-to-host report. Risk: **D** = DANGEROUS, C = CAUTION (whole-table overwrite or device restart), blank = safe to send.

| # | cmd | sub / selector | Dir | Name | Payload summary | Confidence | Risk | Section |
|---|---|---|---|---|---|---|---|---|
| 1 | `01` | `00` | R | Status / capabilities (also the 1 s heartbeat) | reply 18 B: link token [1], battery [8..10], SOCD [14], flags A [17], flags B [18], Win-Lock bits [20], music [21], 6-key [22] | CONFIRMED | | [§4.2](#42-status-and-capabilities-0x01) |
| 2 | `02` | `00` | R | Side-light mode list | mode IDs, `A5 5A`, max speed, max brightness. W75: no modes | CONFIRMED | | [§4.7](#47-lighting) |
| 3 | `07` | `01` | R | Read main light effect | reply `07 00 .. 0C` + 12-B record | CONFIRMED | | [§4.7](#47-lighting) |
| 4 | `07` | `00` | W | Write main light effect | len `0E`: mode, brightness, speed, fg RGB, bg RGB, direction, fullColor, power, 2 pad. No reply | CONFIRMED | | [§4.7](#47-lighting) |
| 5 | `08` | `01` | R | Read side light effect | same record as `07`. W75 answers although it has no side modes | CONFIRMED (bytes); "side light" meaning CODE-ONLY | | [§4.7](#47-lighting) |
| 6 | `08` | `00` | W | Write side light effect | same layout as `07 00` | CODE-ONLY | | [§4.7](#47-lighting) |
| 7 | `09` | `20` | R | Active custom-colour slot | reply `09 SS 00 00 00` (SS 0 = Custom1, 1 = Custom2); identical to a write ack | CONFIRMED (bytes); meaning CODE-ONLY | | [§4.7](#47-lighting) |
| 8 | `09` | `80` / `81` | R | Read custom colour table 1 / 2 | 8 chunks, 396 B = 132 x RGB | CONFIRMED | | [§4.7](#47-lighting) |
| 9 | `09` | `00` | W | Write custom colour table 1 | 8 chunks (54 B, last 18), each acked `09 00 00 00 00`; replaces all 132 slots | CONFIRMED | C | [§4.7](#47-lighting) |
| 10 | `09` | `01` | W | Write custom colour table 2 | same as `09 00` | CODE-ONLY | C | [§4.7](#47-lighting) |
| 11 | `0A` | `00` | R | Main-light mode list | 14 mode IDs, `A5 5A`, max speed 4, max brightness 4 | CONFIRMED | | [§4.7](#47-lighting) |
| 12 | `0D` | `00` | R | Device info | 2 chunks ASCII: `W669,34,KB,ML,W75HEARGB,V3.22.07` + build date | CONFIRMED | | [§4.1](#41-device-info-0x0d) |
| 13 | `0E` | `00` | R | Fn-layer locked keys | 22-B key mask; W75: index 121 | CONFIRMED (bytes); meaning CODE-ONLY; "121 = Fn" GUESS | | [§4.3](#43-fn-layer-locked-keys-0x0e) |
| 14 | `14` | `00`, byte 5 = `00` | W | Win-Lock options and N-key/6-key | [4]=1, [12] Win, [13] Shift+Tab, [14] Alt+F4, [15] Alt+Tab, [18] 6-key | CODE-ONLY | C (byte 5 must be 0) | §4.6.2 |
| 15 | `14` | `00`, byte 5 = `01` | W | **Reset current profile** | `14 00 00 00 01 01` | CODE-ONLY | **D** | §4.6.3 |
| 16 | `14` | IN, byte 5 = `01` | IN | Device reset notification (site then sends a reset) | `14 00 00 00 01 01`, sent when Fn+Space is held 3-5 s (c07) | CONFIRMED | **D** (receive-side hazard) | §4.6.3 |
| 17 | `14` | `01` | W | Set sleep timers (PIDs `C370`/`C371` only, not W75) | [8..9] light sleep s BE16, [10..11] keyboard sleep s BE16 | CODE-ONLY | | §4.6.5 |
| 18 | `14` | `02` | R | Get sleep timers (not W75) | reply [1]=2, same fields | CODE-ONLY | | §4.6.5 |
| 19 | `14` | `03` | W | **Gamepad mode** | `14 03 00 00 01 MM`, MM 0 off / 1 Xbox 360 / 2 Classical; device restarts | CODE-ONLY | **D** | §4.6.4 |
| 20 | `17` | `00`, `00 00` | W | Music rhythm leave | `17 00 00 00 02 00 00`; ack `17 00 00 00 00`; sent before every main-mode change | CONFIRMED | | [§4.7](#47-lighting) |
| 21 | `17` | `00`, `01 01` | W | Music rhythm enter | `17 00 00 00 02 01 01`; not offered on W75 | CODE-ONLY | | [§4.7](#47-lighting) |
| 22 | `18` | `80` / `82` | R | Read keymap base / Fn layer | 10 chunks, 528 B = 132 x 4 B | CONFIRMED (framing; after c09 the base table reads back the site's knob defaults at 16-21 and `01 00 00 00` at 121); entry kinds other than plain key `00 HH 00 00` and macro `10 39 SS 00` CODE-ONLY | | [§4.8](#48-keymap-0x18) |
| 23 | `18` | `00` / `02` | W | Write keymap base / Fn layer | 10 chunks of 56 B (last 24), each acked `18 00 00 00 00`. Plain remap W -> A = entry 46 `00 04 00 00` (typing test passed); default = `00 00 00 00` | CONFIRMED (c09, c13; base layer with real content, Fn layer zero content only) | C | [§4.8](#48-keymap-0x18) |
| 24 | `19` | `80`-`89` | R | Read macro slot 0-9 | 5 chunks, 256 B per slot | CONFIRMED (framing, all-FF content); slot format CODE-ONLY on the read side | | [§4.9](#49-macros-0x19) |
| 25 | `19` | `00`-`09` | W | Write macro slot 0-9 | 5 chunks of 58 B (last 24), each acked `19 00 00 00 00` (sub always `00`). Keyboard-step slot layout of §4.9.3 matches c13 byte for byte (typing test passed) | CONFIRMED (keyboard steps, play once); mouse steps and other play modes CODE-ONLY | C | [§4.9](#49-macros-0x19) |
| 26 | `21` | byte 5 = `00` / `0C` | W | Trigger write (actuation, Rapid Trigger) for a key set | [4]=`18`, [5] mode, [6..27] key mask, [28..35] travel/sensitivity lo/hi | CONFIRMED | | [§4.11](#411-actuation-and-rapid-trigger-0x21) |
| 27 | `21` | byte 5 = `0D` | W | Trigger write, RT with separate press/release | same layout; byte 30 = press, byte 31 = release (W press 0.30 `0f`, release 0.50 `19`) | CONFIRMED (c10) | | [§4.11](#411-actuation-and-rapid-trigger-0x21) |
| 28 | `21` | IN byte 5 = `00` | IN | Trigger write ack | `21 00 00 00 01 00` | OBSERVED | | [§4.11](#411-actuation-and-rapid-trigger-0x21) |
| 29 | `21` | IN byte 5 = `01` | IN | Live travel stream (travel test) | depth = [9]<<8 \| [8] | CODE-ONLY | | §4.11.7 |
| 30 | `21` | byte 5 = `02` | W | Open travel test | [6..27] mask of all layout keys | CODE-ONLY | | §4.11.7 |
| 31 | `21` | byte 5 = `03` | W | Close travel test | `21 00 00 00 18 03` | CODE-ONLY | | §4.11.7 |
| 32 | `21` | byte 5 = `04` | R | Global trigger parameters | reply `04 aa 02 02 00 05`: max 3.40 mm, unit 0.02 mm, min 0.10 mm | CONFIRMED | | [§4.11](#411-actuation-and-rapid-trigger-0x21) |
| 33 | `21` | byte 5 = `05` | R | Per-key trigger read | request row/col; 12-B reply echoing row/col | CONFIRMED | | [§4.11](#411-actuation-and-rapid-trigger-0x21) |
| 34 | `21` | byte 5 = `06` | W | **Reset all key triggers** (active profile) | `21 00 00 00 18 06`; ack `21 00 00 00 01`, then about 1 s later the device pushes `21 00 00 00 02 11 PP`; all 82 keys read `00 55 55 01 01` afterwards | CONFIRMED (c11) | **D** | §4.11.6 |
| 35 | `21` | byte 5 = `07` | W | **Single-key calibration** (dead code in the site) | `21 00 00 00 18 07 row col` | CODE-ONLY | **D** | §4.11.8 |
| 36 | `21` | byte 5 = `08` | W / IN | **All-key calibration** start (`08 00`) / stop (`08 01`) and status (devices without the any-key flag; not the W75 path) | status [6]: 0 done, 1 progress + mask, 2/4 fail | CODE-ONLY | **D** | §4.11.8 |
| 37 | `21` | byte 5 = `0F` / `10` | W / IN | **Any-key calibration** start (`0F 00`) / stop (`10 00`) and status `0F` (W75 path) | as above | CODE-ONLY | **D** | §4.11.8 |
| 38 | `21` | byte 5 = `09` | R | Read polling rate | `21 00 00 00 01 09` -> `.. 02 09 RR`; W75 RR = `08` (8 kHz) | CONFIRMED | | §4.6.1 |
| 39 | `21` | byte 5 = `09`, byte 6 = `01` | W | Set polling rate | `21 00 00 00 00 09 01 RR`; device restarts per UI | CODE-ONLY | C | §4.6.1 |
| 40 | `21` | byte 5 = `11` | R | Read active onboard profile | `21 00 00 00 00 11` -> `.. 03 11 PP XX`; observed `11 01 01` | CONFIRMED | | [§4.5](#45-onboard-profiles) |
| 41 | `21` | byte 5 = `12` | W | Select onboard profile | `21 00 00 00 00 12 PP` (PP 1-4); ack `21 00 00 00 02 12 01` within 4 ms, immediately followed by the push `21 00 00 00 03 11 PP 00`. The site then re-reads everything and writes nothing | CONFIRMED (c08, c10) | | [§4.5](#45-onboard-profiles) |
| 42 | `22` | `80` | R | Read 40 advanced-key slots | 40 chunks x 40 B, chunk index = slot | CONFIRMED (framing, all-zero content) | | [§4.14](#414-advanced-keys-0x22) |
| 43 | `22` | `00`-`27` | W | Write one advanced-key slot | `22 NN 00 00 32 TT ...`, each acked `22 00 00 00 00` | CONFIRMED (framing and acks with all-zero content, c09/c13); non-empty slot format CODE-ONLY | C | [§4.14](#414-advanced-keys-0x22) |
| 44 | `24` | `01` | R | Read SOCD pairs | 2 chunks x 40 B = 10 x 4-B entries each | CONFIRMED (framing, all-zero content) | | [§4.15](#415-socd-0x24) |
| 45 | `24` | `02` | R | Read SOCD power and hotkey | reply `00 00 00 00 fa e5` (off; Fn + R-Shift) | CONFIRMED | | [§4.15](#415-socd-0x24) |
| 46 | `24` | `00` | W | Write SOCD pairs | 2 packets, [4]=40, 10 x [1, model, key1, key2] | CODE-ONLY | C | [§4.15](#415-socd-0x24) |
| 47 | `24` | `03` | W | SOCD on/off | `24 03 00 00 01 PP` | CODE-ONLY | | [§4.15](#415-socd-0x24) |
| 48 | `25` | `00` | R | Supported switch types | `00 01 01` = 1 type, ID 1 (HM1) | CONFIRMED | | [§4.13](#413-switch-types-0x25) |
| 49 | `25` | `02` | R | Per-key switch type table | 3 chunks, 132 B; W75 all `01` | CONFIRMED (framing, uniform content); per-key mapping CODE-ONLY | | [§4.13](#413-switch-types-0x25) |
| 50 | `25` | `01` | W | Write per-key switch type table | 3 chunks (58, 58, 16) | CODE-ONLY | C | [§4.13](#413-switch-types-0x25) |
| 51 | `26` | `00` | R | Read dead band (the site's "reset dead band" sends the same bytes) | 5 chunks, 264 B = 132 x (top, bottom) | CONFIRMED (framing, all-zero content) | | [§4.12](#412-dead-band-0x26) |
| 52 | `26` | `01` | W | Write dead band | 5 chunks of 58 B (last 32), acked `26 01`. Byte 2 x index = top, 2 x index + 1 = bottom, 0.02 mm units (W top 0.10 mm = offset 92 `05`, bottom 0.20 mm = offset 93 `0a`) | CONFIRMED (c12) | C | [§4.12](#412-dead-band-0x26) |

**Counts (updated 2026-10-24 evening, capture session 2):** 52 rows. CONFIRMED 33 (of which rows 5, 7, 13, 22, 24, 25, 42, 43, 44 and 49 are confirmed only for framing/bytes or for part of the content format), OBSERVED 1, CODE-ONLY 18, GUESS 0. DANGEROUS 7 rows (15, 16, 19, 34, 35, 36, 37). See section 7.3 for what session 2 established.

---

## 4. Subsystems

### 4.1 Device info (0x0D)

**Request:** byte 0 = `0x0D`, everything else 0. CONFIRMED.

**Reply:** 2 chunks, byte 4 = `0x32` (50) in both, ASCII padded with NUL (c01 +0, identical in c03).

| Chunk (byte 3) | Content |
|---|---|
| 0 | `W669,34,KB,ML,W75HEARGB,V3.22.07` |
| 1 | `Aug  6 2026,13:46:54` (build date and time; the site ignores this chunk) |

| Field (0-based, comma separated) | Value | Meaning | Confidence |
|---|---|---|---|
| 0 | `W669` | GUESS: platform/project code | GUESS |
| 1 | `34` | unknown | - |
| 2 | `KB` | GUESS: device class "keyboard" | GUESS |
| 3 | `ML` | GUESS: vendor/ODM code (matches the `mile_` company prefix) | GUESS |
| 4 | `W75HEARGB` | product code, matched against `device.json`; also the site's localStorage key | CONFIRMED |
| 5 | `V3.22.07` | firmware version. The site strips control characters, removes every `V` and turns `_` into `.` -> `3.22.07` | CONFIRMED (value); cleaning CODE-ONLY |

- If the string contains `dongle` the site counts dongles and shows the "device has switched modes" alert when it sees more than one (`wmIndex.js:6640-6645`). CODE-ONLY.
- Site quirk: it decodes bytes 5..len-1 instead of 5..5+len-1 (`wmIndex.js:6625`). Harmless here. **Our parser:** take `len` bytes from byte 5, cut at the first NUL, split on commas.

### 4.2 Status and capabilities (0x01)

**Request:** byte 0 = `0x01`, everything else 0 (`agreement.js:900-923`).

- `initInfo` sends it once during connect and waits for the reply (c01 +914, c03 +921).
- `heartbeat` then sends it every 1000 ms without waiting (`wmIndex.js:6138`). Observed intervals 985-1842 ms.
- **The heartbeat is not required.** CODE-ONLY: later replies only drive the battery/link display and the mode-change reload. USER-REPORTED: blocking it for 90 s changed nothing on the keyboard. Our app needs one `0x01` at connect to read the flags; periodic polling is optional.

**Reply:** all 267 independent status reports in the captures (c02: 198 including c01's 44; c03: 69) are identical, one of them unsolicited (c02 +137946; a second unsolicited one is in c07 at +118199, also right after a `0x17` ack):

`01 00 00 00 12 | 00 00 00 00 00 00 00 00 00 01 00 00 df 04 01 01 00 00`

`p[i]` = byte 5+i. The site parses `p[0..17]` (`agreement.js:19-72`).

| Byte | p | Site meaning | W75 value | Confidence |
|---|---|---|---|---|
| 1 | - | Link-mode token. If it changes between two reports the site shows "device has switched modes" and reloads after 1 s (`agreement.js:46-57`) | `00` | CODE-ONLY |
| 5 | 0 | read, then ignored | `00` | - |
| 6, 7 | 1, 2 | not parsed | `00` | - |
| 8 | 3 | 1 = charging (battery PIDs only) | `00` | CODE-ONLY |
| 9 | 4 | battery percent (battery PIDs only) | `00` | CODE-ONLY |
| 10 | 5 | link type: 0 = USB, 1 = 2.4 GHz, other = blank | `00` | CODE-ONLY |
| 11-13 | 6-8 | not parsed | `00` | - |
| 14 | 9 | 1 = SOCD ("PRCS") supported | `01` | CONFIRMED (SOCD reads sent, SOCD tab visible) |
| 15, 16 | 10, 11 | not parsed | `00` | - |
| 17 | 12 | capability flags A (below) | `DF` | see below |
| 18 | 13 | capability flags B (below) | `04` | see below |
| 19 | 14 | **not parsed by the site** | `01` | unknown. GUESS: active profile or Win-Lock state |
| 20 | 15 | Win-Lock option bitmask: bit 0 block Win key, bit 1 block Shift+Tab, bit 2 block Alt+F4, bit 3 block Alt+Tab. Loaded only from the first reply after connect (`agreement.js:40-43`, `wmIndex.js:4187-4199`) | `01` | CODE-ONLY (matches the site default `other = 1`) |
| 21 | 16 | 1 = music-rhythm lighting active. If 1 at load, the site itself sends "enter" + a `07 00` write with mode `0x64` (`wmIndex.js:6475-6479`) | `00` | CODE-ONLY |
| 22 | 17 | bit 0 = 6-key rollover (0 = N-key) | `00` | CODE-ONLY |

**Flags A (byte 17 = `0xDF`)**

| Bit | Site flag | Effect in the site | W75 | Confidence |
|---|---|---|---|---|
| 0 | anyKeyCalibration | calibration uses `21/0F` + `21/10` instead of `21/08` | 1 | CODE-ONLY |
| 1 | customLight | per-key Custom mode; `09 20/80/81` reads | 1 | CONFIRMED |
| 2 | moreSwitch | Actuation > Switch sub-tab; `25` reads | 1 | CONFIRMED |
| 3 | pollRate | Actuation > Polling Rate sub-tab; `21/09` read | 1 | CONFIRMED |
| 4 | deadBand | Actuation > Dead Band sub-tab; `26 00` read | 1 | CONFIRMED |
| 5 | musicRhythm | pseudo light mode 100 offered | 0 | CODE-ONLY |
| 6 | (not read by the site) | - | 1 | unknown |
| 7 | allAnd6keySwitch | Other tab: N-key / 6-key selector | 1 | CODE-ONLY |

**Flags B (byte 18 = `0x04`)**

| Bit | Site flag | Effect | W75 | Confidence |
|---|---|---|---|---|
| 0 | highPrecision | unit denominator 1000 instead of 100 | 0 | CONFIRMED (unit 0.02 mm = 2/100) |
| 1 | (unused) | - | 0 | - |
| 2 | gamePad | Keymap > GamePad sub-tab | 1 | CODE-ONLY |
| 3 | RS | advanced-key type RS offered | 0 | CODE-ONLY |
| 4 | RKRT | advanced-key type RKRT offered | 0 | CODE-ONLY |
| 5 | DKSMouse | mouse keys allowed in DKS | 0 | CODE-ONLY |
| 6 | MDS | advanced-key type MDS offered | 0 | CODE-ONLY |

Consequence for the W75 UI: SOCD, Switch, Polling Rate, Dead Band, Calibration (any-key), Custom lighting, N-key/6-key, GamePad; advanced keys limited to DKS, MT and TGL; no music mode; no battery or sleep timers.

### 4.3 Fn-layer locked keys (0x0E)

- **Request:** `0E 00 00 00 00` (`agreement.js:1264-1280`, 200 ms timeout). CONFIRMED.
- **Reply:** 22-byte key mask (section 2.9). If byte i bit j is set, key index j x 22 + i cannot be remapped on the Fn layer (`agreement.js:295-313`). CODE-ONLY meaning.
- **W75 value:** `0e 00 00 00 16 | 00 x11, 20, 00 x10` = byte 11 bit 5 = **index 121** (row 5, col 11). Identical in c01, c02, c03. CONFIRMED.
- Site behaviour: on the Fn layer such keys are labelled "Not Remappable", lose their remap dot and cannot be selected (`wmIndex.js:3262, 3343, 3498, 7005-7010`). On the base layer the `KeyFn` key is blocked separately.
- Read only at connect, not in the profile reload.
- GUESS (strong): index 121 is the **Fn** key. It is the 6th key of row 5 in the screenshot (L-Ctrl, L-Win, L-Alt, Space, R-Alt, **Fn**, R-Ctrl, arrows).

### 4.4 Connect sequence

The site's order (`wmIndex.js:5811-6438`, `agreement.js:569-655`). Times from c01; c03 matches within about 10 ms up to +1200, then runs faster (`21/11` at +1738, `18 82` at +2747, empty-mask write at +2835). CONFIRMED.

| t (ms, c01) | Request | Site function / purpose |
|---|---|---|
| 0 | `0D 00` | device info |
| 914 | `01 00` | status/capabilities |
| 916 | `0E 00` | Fn-locked keys |
| 919-1004 | `19 80` .. `19 89` | macro slots (all `FF`) |
| 1008 | `22 80` | advanced keys (all `00`) |
| 1080 | `24 02`, `24 01` | SOCD (only if status byte 14 = 1) |
| 1084, 1108 | `18 80`, `18 82` | keymaps (all `00`) |
| 1126, 1131 | `0A 00`, `02 00` | light mode lists |
| 1134, 1138 | `07 01`, `08 01` | main / side light |
| 1142-1171 | `09 20`, `09 80`, `09 81` | custom colours (flag A bit 1) |
| 1176 | `21 .. 18 04` | global trigger parameters |
| 1182-1711 | `21 .. 18 05 row col` x 82 | per-key triggers |
| 1720 | `21 .. 01 09` | poll rate (flag A bit 3) |
| 1726, 1730 | `25 00`, `25 02` | switch types (flag A bit 2) |
| 1736 | `26 00` | dead band (flag A bit 4) |
| about 1756 | - | 1 s heartbeat timer starts (first tick +2756) |
| 1769 | `21 .. 00 11` | active profile; right after sending it the site saves its localStorage mirror |
| 1811 | reply `21 .. 03 11 01 01` | site deletes the mirror and **reloads a subset**: `07 01`, `21/04`, `21/05` x 82, 100 ms sleep, `19 80`..`89`, `22 80`, `24 02`/`01`, `26 00`, `18 80`, `18 82` (+1812..+3303) |
| 3394 | `21 00 00 00 18 00` + zeros | **empty-mask trigger write** (side effect of the site un-ticking its RT checkbox). Acked `00` at +3400. No effect: all three trigger read passes are byte-identical. Do not copy. |
| 3394-4281 | `21/05` x 82 | third trigger read |

Battery PIDs also send `14 02`. A minimal sequence for our app: `0D` -> `01` -> `0E` -> `21/11` -> the tables it needs, each read once (recommendation).

### 4.5 Onboard profiles

#### 4.5.1 Two profile modes in the site

- **Onboard (hardware) profiles.** The site enters this mode as soon as **any** `0x21` report with byte 5 = `0x11` arrives: `presetNumber` starts at -1 (`variable.js:362`) and any reply byte is >= 0 (`agreement.js:569-572`). **The W75 runs in this mode** (c01 +1811, c03 +1781). CONFIRMED.
- **Software profiles.** Used only when no `0x11` report ever arrives. The site keeps five host-side profiles in localStorage and pushes a whole profile to the keyboard on every profile-tab click (section 6.6). Not used for the W75 on firmware V3.22.07.

#### 4.5.2 Read the active profile: `0x21`, byte 5 = `0x11` (CONFIRMED)

Request (`agreement.js:2245-2254`; the site does not wait for the reply):

| Byte | Value |
|---|---|
| 0 | `0x21` |
| 1-3 | `00` |
| 4 | `00` (zero although byte 5 carries data) |
| 5 | `0x11` |
| 6-62 | `00` |

Reply `21 00 00 00 03 | 11 PP XX`:

| Byte | Observed | Meaning | Confidence |
|---|---|---|---|
| 4 | `03` | payload length | CONFIRMED |
| 5 | `11` | echo | CONFIRMED |
| 6 | `01` | active onboard profile (1 = tab "Profile 1") | CONFIRMED value; range 1-4 CODE-ONLY |
| 7 | `01` | not parsed by the site. GUESS: profile count, capability flag or similar | unknown |

- Latency 42-43 ms (others about 1 ms). CONFIRMED.
- Numbering: the `#profiles` radios in `v1.html:608-629` are ProfileDef, Profile1..Profile4. On a `0x11` reply the site ticks `"Profile"+PP` and hides ProfileDef (`agreement.js:578, 624`). So there are **4 onboard profiles numbered 1-4 on the wire** (CODE-ONLY). Whether 0 is valid is unknown.
- `PP` = 1 in both captures. Whether that is the factory default is not shown.

#### 4.5.3 Select a profile: `0x21`, byte 5 = `0x12` (CONFIRMED)

- **Request:** `21 00 00 00 00 12 PP` + zeros (`agreement.js:2255-2265`), PP = 1-4.
- The site sends it when a profile tab is clicked and PP differs from `presetNumber` (`wmIndex.js:1014-1023`). It does not await a reply, does not update `presetNumber` and re-reads nothing by itself.
- The site's UI resyncs only when a `0x21` report with byte 5 = `0x11` arrives. GUESS: it also pushes one when the profile is changed on the keyboard (Fn + 1..4 per `fnKeys.json`).
- **On the wire** (OBSERVED, c04, 11 site-initiated switches from entry 8293 on, all four profiles): the keyboard acks `21 00 00 00 00 12 PP` with `21 00 00 00 02 12 01` within 3-17 ms and pushes `21 00 00 00 03 11 PP 00` right after it (0-2 ms later). Byte 7 of that push is `00`, while replies to our `21/11` query carry `01`. The meaning of ack byte 6 (`01`) is unknown. Every `0x11` report in c04 follows a site-sent `0x12`; **no keyboard-initiated switch has been captured** (the Fn + 1..4 question of Q2 stays open).
- **Captured again** (CONFIRMED, c08 x2, c10 x2): ack `21 00 00 00 02 12 01` within 3-4 ms, then the push. Selecting a profile changes no setting: the site writes nothing after a select, and the tables read afterwards are the target profile's.
- **Our app:** after sending `0x12`, poll `21/11` until it reports the new profile, then re-read every table (section 6.10). OpenWraith waits up to 500 ms for the ack and the push (reading the push before any `21/11` query, whose reply has the same shape), confirms with `21/11` and retries once. It never selects profile 0 (Q11).
- **Delayed commit: a switch soon after a write loses the write** (CONFIRMED on hardware, 2026-10-24, OpenWraith hardware test H9 and a dedicated probe). A trigger write in profile 2 is acked and reads back correctly at once, but if `21/12` selects another profile within about 1 s of the write, the change is discarded: after re-selecting profile 2 the old value reads back. Probe results (write Q in profile 2, wait D, select profile 1, wait 2.5 s, select profile 2, read Q): D = 0, 150, 400 and 800 ms -> LOST; D = 1000, 1100, 1200, 1300 and 1500 ms (x2) -> PERSISTED. A write made with at least 2 s before the next select (separate CLI commands) also persisted. Interpretation (GUESS): the firmware keeps edits in a RAM working copy of the active profile and commits it to flash about 1 s after the last write; a profile switch before that reloads the profile from flash. Read-back verification cannot detect this, because it reads the RAM copy. Consequences: wait at least 2 s after the last write before any profile switch, and verify writes to a non-active profile by re-selecting it and reading again. Whether a USB power loss within 1 s of a write loses it too is untested (very likely). The vendor site has the same problem when a user edits and switches profiles quickly.

#### 4.5.4 What the site re-reads after a `0x11` report (`agreement.js:572-633`)

- **Re-read** (CONFIRMED c01/c03): `07 01`, `21/04`, `21/05` x 82, `19 80`..`89`, `22 80`, `24 02`/`01`, `26 00`, `18 80`, `18 82`. GUESS: these tables are per profile.
- **Not re-read:** `08 01` side light, `09 80`/`81` custom colours, `21/09` poll rate, `25 02` switch types, `0E`, `01` status (Win-Lock/6-key bits come only from the first `01` reply). GUESS: global settings, or an omission in the site.
- OBSERVED (c04, after the site switched profiles): W's `21/05` reads `05 0c 32 32 0f 0f` (1.00 mm, RT 0.30) only in profile 1 and `05 00 55 55 01 01` in profiles 2-4; the main light `07` reads mode `0x0A` in profile 1 and `0x02` in profiles 2-4. So **triggers and the main light are per profile**. `09 80`/`81`, `21/09` and `25 02` were never re-read after a switch, so their scope is still unknown.
- OBSERVED (c10, capture session 2): after the c09 keymap Applies on profile 1, profile 2 read an all-zero base keymap (`18 80`) while profile 1 read the site's knob defaults, and profile 2 read every macro slot as all `FF` while profile 1 read all `00`. So **the base keymap and the macro slots are per profile** too.
- Which settings really are per profile is open question Q3 (partly answered above). Still unknown: `18 82` (Fn keymap), `22 80`, `26 00`, `09 80`/`81`, `09 20`, `08 01`, `24 01`/`02`, `25 02`, `21/09` and the status Win-Lock / 6-key bits; they read identically in every capture. OpenWraith treats them as possibly global: before writing one in a restore it re-reads it and skips the write when it already matches.

### 4.6 Global settings

#### 4.6.1 Polling rate: `0x21`, byte 5 = `0x09`

- **Read (CONFIRMED, c01 +1720, c03 +1689):** `21 00 00 00 01 09` + zeros -> `21 00 00 00 02 09 RR`.
  - RR: `01` = 1 kHz, `02` = 2 kHz, `04` = 4 kHz, `08` = 8 kHz (`wmIndex.js:6072-6083`, `v1.html:1254-1257`). The site ignores other values. **W75: `08`.**
- **Write (CODE-ONLY, CAUTION):** `21 00 00 00 00 09 01 RR` (`agreement.js:1872-1883`). GUESS: byte 6 = 1 means "set".
  - The UI asks first: "Switch polling rate the device will restart" (`wmIndex.js:802-808, 1394-1408`). No reply is awaited. GUESS: the device re-enumerates.
- Hiding 8 kHz on 2.4 GHz applies only to PIDs `0xC370`/`0xC371`, not the W75. Gated by flag A bit 3.

#### 4.6.2 Win-Lock options and N-key/6-key: `0x14 00`, byte 5 = `00` (CODE-ONLY)

Request (`agreement.js:1728-1746`), sent by the Other tab (`wmIndex.js:1709-1732`, `v1.html:1031-1087`):

| Byte | Value |
|---|---|
| 0 | `0x14` |
| 1 | `0x00` |
| 2-3 | `00` |
| 4 | `0x01` (as the site sends it) |
| 5 | **`0x00`, mandatory** (`01` = reset, section 4.6.3) |
| 6-11 | `00` |
| 12 | 1 = block Windows key while Win-Lock is on (`perCheckbox1`, `disableWinKey`) |
| 13 | 1 = block Shift+Tab (`perCheckbox2`) |
| 14 | 1 = block Alt+F4 (`perCheckbox4`) |
| 15 | 1 = block Alt+Tab (`perCheckbox3`) |
| 16-17 | `00` |
| 18 | 1 = 6-key rollover, 0 = N-key |

- The write uses one byte per option; the read-back is the bitmask in status byte 20 (bit 0 Win, 1 Shift+Tab, 2 Alt+F4, 3 Alt+Tab) and status byte 22 bit 0 (6-key). Verified against the checkbox mapping in `wmIndex.js:4187-4199`.
- The site does not wait for a reply. Any `0x14` input shows "Success" and clears `isResetTrigger` (section 4.11.6).
- Whether these options are per profile is unknown.

#### 4.6.3 Reset current profile: `0x14 00`, byte 5 = `01` (**DANGEROUS**, CODE-ONLY)

- **Request:** `14 00 00 00 01 01` + zeros (`agreement.js:1747-1757`).
- **UI:** the Reset button asks "Are you sure you want to reset the current profile?" (`language.json` `resetAlert`; `wmIndex.js:1322-1326`). GUESS: the firmware resets only the active profile. Scope and reply are open question Q4.
- **The site's full reset flow** (`wmIndex.js:1331-1381`, verified):
  1. Send `14 00 00 00 01 01` (no wait for a reply).
  2. Call `resetTrigger`, which sends `21 00 00 00 18 06` **only if** its flag `isResetTrigger` is set (section 4.11.6).
  3. If `isInitDeadBand` is false, send `26 00` (the dead-band **read** request; section 4.12).
  4. Clear RS advanced keys in memory, save the localStorage mirror, sleep 100 ms, send `24 03 00 00 01 00` (SOCD off).
  5. Re-read `07 01`, `21/04`, `21/05` x 82; sleep 100 ms; clear keymaps and macro bindings **in memory only** (not re-read).
  6. Re-dispatch the selected profile tab. Onboard mode: nothing is sent. Software mode: full profile push.
- **Device-initiated reset (receive-side hazard):** any input report with byte 0 = `0x14` and byte 5 = `0x01` (any byte 1) makes the site confirm "reset all" in code and run the whole flow above, which sends another reset (`agreement.js:314-319`).
  - GUESS: intended for a reset started on the keyboard (Fn + Space per `fnKeys.json`).
  - Risk 1 (GUESS): if the firmware acked `14 00 00 00 01 01` with byte 5 = `01`, the reset would re-trigger itself repeatedly.
  - Risk 2 (GUESS): `setGamePadMode(1)` sends `14 03 00 00 01 01`; if the firmware echoed it, the site would reset the current profile.
- **Our app:** explicit confirmation, back up all tables first, never react to an incoming `0x14` with a reset.

#### 4.6.4 Gamepad mode: `0x14 03` (**DANGEROUS**, CODE-ONLY)

- **Request:** `14 03 00 00 01 MM`, MM = 0 Disabled, 1 Xbox 360, 2 Classical (`agreement.js:1758-1769`; `wmIndex.js:682-705`; `v1.html:829-857`).
- The UI asks first ("Note that the device will restart!", `language.json` `gamePadTip1`). No reply awaited.
- After restart the site infers the mode from the PID (`0xC366` Xbox, `0xC367` Classical). That the W75 re-enumerates like this is a GUESS.
- Gated by flag B bit 2 (set on the W75). See Risk 2 above.

#### 4.6.5 Sleep timers: `0x14 01` / `0x14 02` (CODE-ONLY; wireless PIDs `0xC370`/`0xC371` only, **not the W75**)

- **Read:** request `14 02` (the site sends a 6-byte buffer). Reply byte 1 = 2: bytes 8-9 = light-sleep seconds (BE16), bytes 10-11 = keyboard-sleep seconds (BE16, shown as minutes) (`agreement.js:320-326`).
- **Write:** `14 01 00 00 00 00 00 00 LL LL KK KK` (byte 4 left 0), seconds BE16 (`agreement.js:1780-1797`).
- UI rules (`wmIndex.js:5992-6005`): light sleep >= 5 s and <= keyboard sleep; keyboard sleep 5-60000 minutes in code (5-1000 in the HTML). Values above 1092 minutes overflow the u16; the site does not guard against it.

### 4.7 Lighting

Commands: `0x0A`, `0x02`, `0x07`, `0x08`, `0x09`, `0x17`. No other lighting opcode exists in `agreement.js`.

#### 4.7.1 Capability discovery

- Status `0x01` flags A bit 1 = custom per-key tables supported (W75: 1), bit 5 = music rhythm supported (W75: 0); status byte 21 = music rhythm active (W75: 0). Section 4.2.
- **Main-light mode list `0x0A`** (`agreement.js:73-147, 1560-1576`). Request `0A 00 00 00 00`. Reply `0A 00 00 00 n` + n bytes, n >= 4:

| Payload index | Meaning | Confidence |
|---|---|---|
| 0 .. n-5 | supported mode IDs, device order | CONFIRMED |
| n-4, n-3 | `A5 5A`, ignored by the site. GUESS: marker/terminator | bytes CONFIRMED; meaning GUESS |
| n-2 | max speed (speed slider max; min 0) | CODE-ONLY (W75 has 4 in both bytes, so the order cannot be told from the capture) |
| n-1 | max brightness (brightness slider max; min 0) | CODE-ONLY (same reason) |

- W75 (c01 +1127, c03 +1142): n = `0x12`, payload `02 01 03 09 04 00 0e 0f 06 07 0b 0c 10 08 a5 5a 04 04` = 14 modes, max speed 4, max brightness 4. CONFIRMED.
- When the device list is non-empty the site appends `10` (Custom; radio hidden unless flag A bit 1) and, only if flag A bit 5, `100` (Musical Rhythm). **Mode `0x0A` is not in the W75 list, but the W75 accepts it, stores it and reads it back** (c02 +137943 write, c03 +1147 read). CONFIRMED.
- **Side-light mode list `0x02`** (`agreement.js:148-218, 1577-1593`): same layout. W75 (c01 +1132): `02 00 00 00 04 | a5 5a 04 04` = **no side modes**, so the site hides the "Ambient" tab. CONFIRMED. On models with side modes the site always offers side mode 5 "Close" (CODE-ONLY).

#### 4.7.2 Effect record: `0x07` (main) and `0x08` (side)

- **Read** (CONFIRMED): request `07 01 00 00 00` / `08 01 00 00 00`. Reply `07 00 00 00 0C` + 12 bytes (byte 1 = `00`, not `01`). Match on byte 0. The site needs len >= 12 and maps direction >= 6 to 0.
- **Write** (CONFIRMED for `07`, CODE-ONLY for `08`): `07 00 00 00 0E` + 14 bytes. **No reply.** Persisted by the device: c03 (a later page session) reads back the last value written in c02; the maintainer verified survival across a USB replug (USER-REPORTED).

| Byte | p | Field | Values / notes | Confidence |
|---|---|---|---|---|
| 0 | - | cmd | `07` main, `08` side | CONFIRMED |
| 1 | - | sub | `00` write; `01` read request; read reply carries `00` | CONFIRMED |
| 2-3 | - | `00` | | CONFIRMED |
| 4 | - | len | write `0E`; read reply `0C` | CONFIRMED |
| 5 | 0 | mode | table 4.7.3 | CONFIRMED (`02`, `0A` observed) |
| 6 | 1 | brightness | 0..max brightness (W75 0..4); c02 shows 4 -> 0 (+152227) -> 4 (+153148), single writes with nothing else changed | CONFIRMED |
| 7 | 2 | speed | 0..max speed (W75 0..4). Only `03` observed. Whether 0 is slowest is unknown | CODE-ONLY |
| 8-10 | 3-5 | foreground R, G, B | 0..255 (observed `ff0000`, `fcfcfc`, `f3f3f3`, `fe2145`, `ff103b`, `f9103a`, `ff00c2`) | CONFIRMED |
| 11-13 | 6-8 | background R, G, B | 0..255. Only `000000` observed | CODE-ONLY |
| 14 | 9 | direction | table 4.7.4. Values >= 6 read as 0 | CODE-ONLY (only 0 observed) |
| 15 | 10 | fullColor | 0/1, UI checkbox "Full Color". GUESS: 1 = rainbow colours instead of the foreground | CODE-ONLY (constant `01`); meaning GUESS |
| 16 | 11 | power | **0 = lighting on, 1 = off** (UI switch "Power" sends 0 when checked, `wmIndex.js:4943, 6471`) | CODE-ONLY (0 observed while lit) |
| 17-18 | 12-13 | padding | never set by the site; counted in write len 14; absent from the 12-byte read | CONFIRMED |

Captured records:

- Main light at first connect (c01 +1135): `02 04 03 ff ff ff 00 00 00 00 01 00` = Wave, brightness 4, speed 3, white, bg black, direction 0, fullColor 1, power on. CONFIRMED. (Whether this is the factory state is not shown.)
- User change (c02 +137943): `0a 04 03 ff 00 00 00 00 00 00 01 00 00 00` = **mode `0x0A` = Custom (per-key)**, foreground red (the site's per-mode default colour), power on. The earlier draft called this "static red"; it is Custom mode showing a red per-key table (section 4.7.6).
- Side record (c01 +1138): `02 04 03 ff 00 00 00 00 00 00 01 00`. The W75 answers `08 01` although it reports no side modes.

#### 4.7.3 Mode IDs

Names from `language.json` (`lightMode<N>`, `sideLightMode<N>`). Control visibility from `refreshColorView` (`wmIndex.js:4989-5175`), CODE-ONLY. Hidden controls are still sent with their current values.

| ID | Name (en) | In W75 `0x0A` list | Brightness | Speed | Direction choices (site default) | FG | BG | Full Color |
|---|---|---|---|---|---|---|---|---|
| `00` | Static | yes | yes | - | - (0) | yes | - | hidden |
| `01` | Breath | yes | yes | yes | - (0) | yes | yes | yes |
| `02` | Wave | yes | yes | yes | left 1, right 0, up 2, down 3, gather 5, spread 4 (0) | yes | yes | yes |
| `03` | Neon | yes | yes | yes | - (0) | yes | yes | yes |
| `04` | Radar | yes | yes | yes | anticlockwise 1, clockwise 0 (0) | yes | yes | yes |
| `05` | (empty string) | no | - | - | - | - | - | - |
| `06` | Reactive | yes | yes | yes | - (0) | yes | yes | yes |
| `07` | Aurora | yes | yes | yes | gather 5, spread 4 (4) | yes | yes | yes |
| `08` | Ripple | yes | yes | yes | - (0) | yes | yes | yes |
| `09` | Twinkle | yes | yes | yes | - (0) | yes | yes | yes |
| `0A` | Custom (per-key, section 4.7.6) | added by the site | yes | - | - (0) | swatch still sent | - | hidden |
| `0B` | Cross | yes | yes | yes | - (0) | yes | yes | yes |
| `0C` | Speed Respond | yes | yes | - | up 2, down 3 (2) | yes | yes | yes |
| `0D` | (empty string) | no | - | - | - | - | - | - |
| `0E` | Auto Ripple | yes | yes | yes | - (0) | yes | yes | yes |
| `0F` | Striation | yes | yes | yes | left 1, right 0 (0) | yes | yes | yes |
| `10` | Fireworks | yes | yes | yes | - (0) | yes | yes | yes |
| `64` (100) | Musical Rhythm | site pseudo-mode, only if flag A bit 5 | - | - | - | - | - | - |

Side-light modes (other models only, CODE-ONLY): 0 Static, 1 Breath, 2 Wave, 3 Collision, 4 Neon, 5 Close (power = 1).

#### 4.7.4 Direction values (CODE-ONLY; only 0 observed)

| Value | UI button |
|---|---|
| 0 | Right (Radar: clockwise) |
| 1 | Left (Radar: anticlockwise) |
| 2 | Up |
| 3 | Down |
| 4 | Spread |
| 5 | Gather |

From the radio values in `v1.html:928-933`. On each mode selection the site checks a default radio (0, or 4 for Aurora, 2 for Speed Respond) and sends whichever is checked.

#### 4.7.5 Site-only lighting state

- Per-mode colour memory `colorArray1`-`4` (main fg/bg, side fg/bg; 20 entries each) lives only in localStorage. The protocol record carries one fg/bg pair. Whether the firmware remembers per-mode colours is unknown.
- Defaults for a fresh local profile: mode 2, brightness 4, speed 3, direction 0, fullColor 1, fg red, bg black, power 0 (`wmIndex.js:5886-5897`).
- The localStorage mirror does not restore the per-key tables (`objToLight`, `wmIndex.js:5736-5750`), so the site's only source for them is the device read at connect.

#### 4.7.6 Per-key custom colour tables (`0x09`)

**Table format** (CONFIRMED): 132 slots x 3 bytes (R, G, B) = 396 bytes; slot i at byte 3i. Two tables: **Custom1** (write sub `00`, read sub `80`) and **Custom2** (write `01`, read `81`).

- Slot = key index (row x 22 + col): CONFIRMED for the 81 slots the site painted in c02 (exactly the read positions minus the knob).
- GUESS: the keyboard shows a table only in mode `0x0A`. The site edits tables only in mode 10.

**Read `09 80` / `09 81`** (CONFIRMED): request `09 80 00 00 00`; 8 replies `09 80 <idx hi> <idx lo> <len>` + data. Chunks 0-6 len `0x36` (54 B = 18 slots), chunk 7 len `0x12` (18 B = 6 slots). Slot = 18 x chunk + k. About 2 ms apart (c01 +1143..+1156, +1161..+1171). Chunk 7 carries stale bytes after `len` (c01 +1156); honour `len`.

**Write `09 00` / `09 01`** (CONFIRMED for `00`, CODE-ONLY for `01`; CAUTION):

- Same chunking: 8 reports `09 SS <idx hi> <idx lo> <len>` + data, 54 B per chunk, last 18 B.
- Each chunk is acknowledged with `09 00 00 00 00` (len 0); 1681 acks for 2064 chunks in c02 (section 2.7).
- **A write always replaces all 132 slots.** The site zero-fills a buffer and fills only slots of keys in its layout file (writing each key's stored colour, including the knob at 15, whose stored colour was black). Every other slot becomes `000000`. CONFIRMED: device-side slots 103 and 126 were red at connect (c01) and were `000000` already in the first burst (c02 +160405).
- **It is not a preview command.** No other `0x09` writer exists; the same call is used for every edit. The table persists across a page reload (the final c02 table, 81 slots `ff0000`, reads back unchanged in c03 +1150..+1164). CONFIRMED. Persistence across a USB power cycle is untested (open question Q1).
- **Flood behaviour** (OBSERVED, c02): 258 full-table bursts (2064 chunks) in about 15 s (+160405..+175203), median 8 ms between burst starts. Dragging the intensity ring starts a new table write on every mouse-move without waiting for the previous one, so bursts interleave on the wire.

**Active slot query `09 20`** (bytes CONFIRMED, meaning CODE-ONLY): request `09 20 00 00 00`; W75 answers `09 00 00 00 00` (c01 +1142, c03 +1150). The site reads byte 1: 0 = Custom1, 1 = Custom2 (`agreement.js:286-293`). The reply is byte-identical to a write ack. How the device chooses the displayed table is unknown; clicking Custom1/Custom2 in the site only writes that table (GUESS: the last-written table becomes active).

**Observed contents** (CONFIRMED):

- c01 (before any change): Custom1 = 8 red slots {46, 68, 69, 70, 103, 124, 125, 126}; Custom2 = 34 blue slots {45-56, 58, 68-78, 90-99}.
  - {46, 68-70} = W, A, S, D. The other four are offset by +1 from the inferred arrow keys {102, 123, 124, 125}; slots 103 (row 4, col 15) and 126 (row 5, col 16) are not keys in our layout. Whether LED slot = key index for every key is open question Q8.
- After c02: Custom1 = 81 red slots (every selectable key); Custom2 unchanged. **The factory Custom1 pattern is gone from the user's keyboard.**

**Colour controls in the site** (CODE-ONLY, `wmIndex.js:5219-5303, 5370-5383, 1736-1784`): the draggable intensity ring scales the current colour by angle/360 and streams full tables on mousedown/mousemove, then one `07 00` on mouseup; the hue wheel sends one table plus one `07 00` per click; RGB boxes and swatches send both on every input. The first streamed colour `f2f2f2` is 0.95 x the white foreground read at connect, consistent with this.

**Reset custom colours** (CODE-ONLY): the Reset button in the Custom row writes a 132-slot black table to the selected slot (`wmIndex.js:1409-1411, 1676-1683`).

#### 4.7.7 Music rhythm (`0x17`)

| Byte | Value |
|---|---|
| 0 | `0x17` |
| 1-3 | `00` |
| 4 | `02` |
| 5 | 1 = enter, 0 = leave |
| 6 | same as byte 5 (purpose of the duplicate unknown) |

- **Leave** (`00 00`) is sent before every main-mode change other than 100, even on devices without music support (`agreement.js:96-107`). The device acks `17 00 00 00 00`; the site does not wait (c02 +137930 -> ack +137943 and c07 +118182 -> ack +118194, each logged after the next `07 00` write of the same millisecond). CONFIRMED. The earlier draft's "begin edit / `02`" was this packet; `02` is the length.
- **Enter** (`01 01`) when the user picks mode 100, or automatically at load when status byte 21 = 1; followed by a `07 00` write with mode `0x64` (CODE-ONLY). The audio-reactive effect needs a separately downloaded Windows plugin (`wmIndex.js:1689-1708`); its protocol is unknown.
- **W75: not supported** (flag A bit 5 = 0). Whether the leave packet is needed at all on the W75 is unknown (GUESS: not needed).

#### 4.7.8 Sequences (site)

- Connect: `0A`, `02`, `07 01`, `08 01`, then (flag A bit 1) `09 20`, `09 80`, `09 81`. CONFIRMED.
- After a `0x11` report: only `07 01` is re-read. CONFIRMED.
- Mode change: `17 .. 00 00`, then `07 00`. CONFIRMED.
- Brightness, speed, direction, Full Color, Power: one `07 00` write; sliders write on release. Brightness CONFIRMED, others CODE-ONLY. Power switched on in the main tab triggers a `07 01` re-read.
- Colour edit in Custom mode: full `09 00`/`01` table on every colour event, plus `07 00` on click/release.

#### 4.7.9 Recommendations for our app

1. W75: hide side light, never write `0x08`; offer Custom (`0x0A`) although it is not in the list; no music mode.
2. Read-modify-write the 12-byte `0x07` record; send len 14 with two zero pad bytes; expect no reply. Use `0x0A` byte n-1 as max brightness and n-2 as max speed (site convention, unverified on W75).
3. Keep both 132-slot tables client-side from the `09 80/81` reads. Write all 8 chunks serialised, waiting for each `09 00 00 00 00` ack (timeout + one retry). Throttle to "on release" or <= 10 Hz. **Preserve slots outside our key map** (e.g. 103, 126) instead of zeroing them.
4. Optionally send `17 .. 00 00` before a mode change for parity with the site.
5. Do not interleave a `09 20` query with table writes; their replies are identical.

### 4.8 Keymap (0x18)

Keymap writes were captured in session 2 (c09: W -> A and back; c13: a macro bound to Caps and removed). The write framing and acks, plain key entries `00 HH 00 00` and macro entries `10 39 SS 00` are CONFIRMED; the other entry kinds are CODE-ONLY. Fn-layer writes were only captured with zero content.

#### 4.8.1 Read `18 80` (base) / `18 82` (Fn layer) (CONFIRMED framing)

- Request `18 80 00 00 00` or `18 82 00 00 00`, no payload (`agreement.js:1067-1091`).
- Reply: 10 reports `18 8x 00 <chunk 0..9> <len>`, len `0x38` (56) for chunks 0-8 and `0x18` (24) for chunk 9 = 528 bytes = 132 entries x 4 bytes. Entry for key k at offset 4k (chunk k / 14, offset (k mod 14) x 4); no entry crosses a chunk. CONFIRMED.
- Timing (c01): `18 80` at +1084, chunk 9 at +1103; `18 82` at +1108.
- **W75 content:** all `00` in both tables in every capture up to c08. CONFIRMED. Since the site's c09 Apply, profile 1's base table reads the site's forced knob defaults at 16-21 and `01 00 00 00` at 121 (section 4.10); profile 2's still reads all `00` (c10). `00 00 00 00` means "no override" (the site writes it for unmapped keys, `agreement.js:1148-1151, 1221-1224`). That the firmware then uses its built-in default is inferred from the keyboard typing normally.
- Sub-commands other than `00`/`02`/`80`/`82` are never used by the site; whether `01`/`81` exists is unknown.

#### 4.8.2 Write `18 00` (base) / `18 02` (Fn layer) (CONFIRMED; Fn layer with zero content only; CAUTION)

- Always the whole 528-byte table: 10 reports `18 00|02 00 <chunk> <len>` + table bytes [chunk x 56, +len), len `0x38` x 9 then `0x18` (`setKeyValue` `agreement.js:1102-1187`, `setFnKeyValue` `1188-1262`). No wait between chunks in the site. CONFIRMED (c09, c13).
- **Ack:** `18 00 00 00 00` per chunk, for both layers (section 2.7). CONFIRMED. The site ignores `0x18` inputs other than `80`/`82`.
- **Read-back:** c10 and c11 read back the site's c09 table byte for byte, including its forced knob defaults at 16-21 and `01 00 00 00` at 121 (section 4.10). CONFIRMED. A read-back of a non-zero Fn layer has never been captured.
- Sent only from the keymap "Apply" flow (section 6.7).
- How the site fills entries it was not asked to change (verified, `agreement.js:1102-1160, 1218-1236`):
  - Base layer, no remap: `00 00 00 00`, except: the layout key `KeyFn` gets code1 = 1 (`01 <layout hid> 00 00`) unless the layout supplies its own code1; layout keys with an explicit code1 get `<code1> <hid> 00 00`; keys named M1-M4 take code1/hid/code3 from `keys.json`; knob virtual keys get their layout codes; then (W75 has a knob, PID != `0xC365`) **slots 16-21 are overwritten with knob defaults** (section 4.10).
  - Fn layer, no remap: `00 00 00 00`, except unmapped knob-region slots (layout codes), then slots 16-19 forced to knob defaults if the layout has knob regions.
- Resetting one key in the site removes its remap and runs the full Apply; the entry returns to `00 00 00 00`.
- **The base layer is saved only when the Fn layer is written after it** (CONFIRMED on hardware, 2026-10-24, OpenWraith restore test and two probes; each check = wait for the commit, select profile 2, select profile 1, read):
  - `18 00` alone (W -> A, and knob slot 16 -> `00`): acked and read back correctly at once, but **gone after the profile switch**, even with a 6 s wait before the switch. So this is not the ~1 s commit delay of section 4.5.3.
  - `18 00` followed by `18 02` (Fn table with one change at the unused index 130): **both** layers kept.
  - `18 02` alone (index 130 -> `00 04 00 00` and back): kept.
  - Interpretation (GUESS): the firmware saves the keymap pair (perhaps the whole keymap/macro/advanced block) when it receives `18 02`; the site never notices because every Apply writes macros, `18 00`, `18 02`, then `22` in that order. Whether macro slots (`19`) and advanced-key slots (`22`) written without a following `18 02` are kept is untested.
  - **Rule for our app:** whenever `18 00` is written, write `18 02` right after it (with its current or target content, even if unchanged), and keep the site's order for the whole group: macros, base, Fn, advanced keys. Verify persistence by reading after a profile switch.
  - OpenWraith also ends a group that writes macro or advanced-key slots while neither keymap layer differs with the unchanged `18 02` (a precaution, GUESS: those slots may depend on the same trigger; the order is then macros, advanced keys, Fn). Its restore never writes `18 00` without the `18 02` after it.
- **Our app (recommendation):** read the table, change the 4 bytes of the edited key, write all 10 chunks back. Keep `00 00 00 00` for unmapped positions (what the device returned), including 121 (Fn) and 15-21. Whether a single chunk may be written alone is unknown.
- Example (base-layer W, index 46 -> A): offset 184 = chunk 3, payload offset 16 (report byte 21). Send `18 00 00 03 38` + 56 bytes with `00 04 00 00` at payload 16..19, and all other chunks unchanged (chunk 9 header `18 00 00 09 18`).

#### 4.8.3 Entry encoding (4 bytes `b0 b1 b2 b3`; CODE-ONLY except `00 00 00 00`)

`keys.json` values are hex strings parsed base 16; gamepad codes in `gamePadKey.js` are decimal.

| b0 | Category | b1 | b2 | b3 |
|---|---|---|---|---|
| `00` | Keyboard key: USB HID usage (page 0x07). `00 00 00 00` = no override (CONFIRMED) | usage | 00 | 00 |
| `01` | Fn (only written for the `KeyFn` position) | layout hid (`0xFA` in other layouts; the site labels `00 FA` as "Fn" on read) | 00 | 00 |
| `02` | Lighting: `02` = All Lighting (cycle effects) | sub | 00 | 00 |
| `03` | Brightness: `00` +, `01` - | sub | 00 | 00 |
| `04` | Effect speed: `00` +, `01` - | sub | 00 | 00 |
| `05` | Direction: `02` = cycle | sub | 00 | 00 |
| `06` | Colour: `00` full colour, `01` foreground cycle, `02` background cycle | sub | 00 | 00 |
| `07` | Multimedia (vendor index, table below) | index | 00 | 00 |
| `10` | Macro | the physical key's own usage (ignored on read) | slot 0..9 (reader masks `& 0x1F`) | 00 |
| `11` | Function: `00` lock keyboard, `02` lock Win, `04` all lights off | sub | 00 | 00 |
| `12` | Mouse | kind: `00` button, `01` wheel | button mask `01` L, `02` R, `04` M, `08` Forward, `10` Back; wheel `01` up, `FF` down | 00 |
| `13` | Key combination (UI: 1-2 modifiers + exactly 1 key) | usage 1 (modifier E0-E7) | usage 2 | usage 3 or 00 |
| `17` | Advanced key (section 4.14) | slot | slot | 00 |
| `18` | Gamepad "Classical" (shown only on PID `0xC367`) | control 0..32 | 00 | 00 |
| `19` | Gamepad Xbox 360 (shown only on PID `0xC366`) | control 0..24 | 00 | 00 |

- Mouse on the wire: the site's model holds (code1 `0x12`, hidCode = button, code3 = kind) and writes `12 <code3> <hidCode> <code4>`, i.e. kind first. Use `12 00 <button> 00` / `12 01 <dir> 00`.
- Predefined combinations (`keys.json`): Win+D `13 E3 07`, Alt+F4 `13 E2 3D`, Win+L `13 E3 0F`, Ctrl+Z `13 E0 1D`, Ctrl+C `13 E0 06`, Ctrl+V `13 E0 19`, Ctrl+X `13 E0 1B`, Ctrl+S `13 E0 16`, Ctrl+N `13 E0 11`, Ctrl+P `13 E0 13` (b3 = 00).
- Multimedia indices (b0 = `07`): `10` Vol+, `11` Vol-, `12` Mute, `13` Play/Pause, `14` Next, `15` Previous, `16` Stop, `20` Mail, `21` Browser Search, `22` Favorites, `23` Browser Home, `24` code name BrowserForward but Chinese label "previous page" (contradictory), `25` BrowserBack / "next page" (contradictory), `26` Browser Stop, `27` Refresh, `30` Media player, `32` Calculator, `34` My Computer.
- HID usages offered by the site (b0 = `00`): `04`-`1D` A-Z, `1E`-`27` 1-0, `28`-`2C` Enter Esc Backspace Tab Space, `2D`-`31` `- = [ ] \`, `33`-`38` `; ' grave , . /` (`32` not offered), `39` Caps Lock, `3A`-`45` F1-F12, `46`-`4E` PrtSc ScrLk Pause Ins Home PgUp Del End PgDn, `4F`-`52` Right Left Down Up, `53`-`63` Num Lock and keypad, `64` Intl backslash, `65` Menu, `68`-`73` F13-F24, `87`-`8B` JIS keys, `E0`-`E7` LCtrl LShift LAlt LGui RCtrl RShift RAlt RGui.
- Gamepad controls (GUESS that they apply to the W75): Classical 0-15 axes, 16 trigger, 17-28 buttons 1-12, 29-32 hat; Xbox 0-7 sticks, 8/9 LT/RT, 10-13 A/B/X/Y, 14 start, 15 select, 16 menu, 17-20 d-pad, 21/22 L3/R3, 23/24 LB/RB.
- How the site decodes a read (`agreement.js:340-476`): `10` = macro (slot from b2, falling back to matching a macro ID); other entries are decoded only when the physical key's layout code is in `keys.json` **and** the bytes differ from that catalogue entry, so a non-macro entry at the Fn position is never decoded. Entries the site cannot map are dropped from its model and lost on the next Apply.

#### 4.8.4 Fn layer

- `18 82`/`18 02` hold overrides active while Fn is held; same entry format (CODE-ONLY). Zero entries leave the firmware's built-in Fn functions in place. The site cannot read the built-ins; it only labels them from `fnKeys.json`.
- W75 built-in Fn functions (labels only; GUESS that the firmware does exactly this):

| Fn + key | Function | Fn + key | Function |
|---|---|---|---|
| Up / Down | Brightness + / - | F1 | Media player |
| Left | Effect direction | F2 / F3 | Volume - / + |
| Right | Foreground colour switch | F4 | Mute |
| Backslash | All Lighting (cycle effects) | F5 | Stop |
| `-` / `=` | Speed - / + | F6 / F7 / F8 | Previous / Play-Pause / Next |
| Backspace | All lights off | F9 | Mail |
| Space | Reset | F10 | Browser Home |
| A / S | Windows / Mac mode | F11 | Calculator |
| L-Win | Win-key lock | F12 | Browser Search |
| Del | Insert | Home | End |
| 1 / 2 / 3 / 4 | Profile 1 / 2 / 3 / 4 | | |

- Whether a non-zero Fn-layer entry overrides a built-in (e.g. Fn+1 profile switch) is open question Q14.

### 4.9 Macros (0x19)

#### 4.9.1 Read `19 8n` (n = 0..9)

- Request `19 8n 00 00 00`, slots in order, 150 ms timeout per slot (`agreement.js:1438-1466`). CONFIRMED.
- Reply: 5 reports, chunks 0-3 len `0x3A` (58), chunk 4 len `0x18` (24) = **256 bytes per slot**; padding after `len` is `FF` in chunk 4. About 8 ms per slot (c01 from +919). CONFIRMED.
- **W75 content:** every slot is all `FF` (factory, all captures up to c09). CONFIRMED. After the site has written empty slots (256 x `00`) they read back as 256 x `00` (OBSERVED, c10). The site treats a slot as empty when header byte 3 is < 4 or > 248 (covers `FF` and `00`). A read-back of a **non-empty** slot has never been captured, so the read layout of non-empty content stays CODE-ONLY.
- Chunk c holds slot bytes [c x 58, +len) ("flat" layout, matching the writer). CODE-ONLY: the site's parser also supports a "page" layout that repeats the header in every chunk, and actually picks it for all-FF slots; the captures cannot tell the layouts apart.

#### 4.9.2 Write `19 0n` (CONFIRMED for keyboard steps played once; CAUTION)

- `setMacroValue` (`agreement.js:1468-1558`) rewrites **all 10 slots** on every Apply: 5 reports `19 0n 00 <c> <len>` with len 58, 58, 58, 58, 24.
- Slot assignment: macro bindings are collected base layer first, then Fn layer, in insertion order; binding i goes into slot i, and that slot number is written into the key's keymap entry b2. Unused slots are 256 x `00` (not `FF`).
- After each chunk the site waits up to 10 zero-ms ticks for an input `0x19` with byte 1 < `0x80` (treated as ack), then continues anyway. **Ack:** `19 00 00 00 00` for every slot (section 2.7). CONFIRMED (c09, c13).
- c13 slot 0 = `00 00 00 10 00 01 00 00 | 04 10 00 64 | 04 00 00 64 | 05 10 00 64 | 05 00 00 00` (ID 0, play once, 16 step bytes, count 1: A down 100 ms, A up 100 ms, B down 100 ms, B up), matching the layout of 4.9.3 byte for byte; the typing test passed. CONFIRMED.
- **Macros must be written before the keymap** that references their slots.

#### 4.9.3 Slot layout (256 bytes, CODE-ONLY)

| Offset | Size | Meaning |
|---|---|---|
| 0 | 1 | bits 0-6: macro ID (the site's library index 0..9). Bit 7: copy flag, set when the same macro was already written into a lower slot in this Apply (the reader skips steps for copies) |
| 1 | 1 | play mode: 0 once, 1 N times (uses count), 2 press to start / press again to stop, 3 repeat while held |
| 2 | 1 | 0 |
| 3 | 1 | step data length in bytes = steps x 4 |
| 4-5 | 2 | repeat count, BE16. The site writes the "times" input (default 1) whatever the mode, so a "play once" macro is usually written with `00 01` |
| 6-7 | 2 | 0 |
| 8.. | 4 x n | steps |

Step (4 bytes): +0 code (HID usage, or mouse/other code); +1 `(state << 4) | type` with state 1 press / 0 release and type 0 keyboard / 1 anything else (the site uses type 1 for any non-keyboard catalogue key, e.g. media); +2..+3 delay in ms BE16, **the delay to wait after this step** (last step 0; the reader shifts delays back by one). Typical +1 values: `10` key down, `00` key up, `11` mouse down, `01` mouse up.

- Capacity: the reader accepts <= 248 step bytes (62 steps); the UI allows 20 press/release groups; at most 10 macros and 10 bindings.
- **Unresolved mismatch:** the writer sends mouse buttons as `01` L, `02` R, `04` M; the reader decodes `00` L, `01` M, `02` R. Open question Q13.
- Example (CODE-ONLY): macro ID 0, play once, "A down, 30 ms, A up" in slot 0, bound to Caps (index 66): slot 0 chunk 0 = `19 00 00 00 3a | 00 00 00 08 00 01 00 00 04 10 00 1e 04 00 00 00` + zeros; chunks 1-3 zero (len `3a`), chunk 4 zero (len `18`); slots 1-9 zero; base keymap entry at offset 264 (chunk 4, payload 40) = `10 39 00 00`.

#### 4.9.4 Macro names

Macro names exist only in the site's localStorage. The slot has no name field; names read back from the device are generated as "Macro<n>". Our app must store names locally (section 6.9).

### 4.10 Rotary knob slots

- **The W75 layout has a rotary knob.** The site draws the round knob panel only for a layout entry with code `RotaryKnob` (`wmIndex.js:6174-6190`), and the W75 screenshot shows that panel to the right of Del. INFERRED (strong). The knob body is index 15 (row 0, col 15): it is the only one of the 82 read positions that Select All never includes (c02). INFERRED (strong).
- **Forced keymap defaults** (verified in code, `agreement.js:1092-1101, 1154-1160, 1226-1236`). After building a keymap table the site overwrites:

| Index | Entry written | Meaning (`keys.json`) | Base `18 00` | Fn `18 02` | GUESSED physical role |
|---|---|---|---|---|---|
| 16 | `07 12 00 00` | Mute | if knob and PID != `0xC365` | if knob and layout has knob regions | press |
| 17 | `07 11 00 00` | Volume - | same | same | rotate left |
| 18 | `07 10 00 00` | Volume + | same | same | rotate right |
| 19 | `02 02 00 00` | All Lighting | same | same | ? |
| 20 | `03 01 00 00` | Brightness - | same | not written | ? |
| 21 | `03 00 00 00` | Brightness + | same | not written | ? |

- These overwrites replace any user remap of those slots on every Apply. **The factory W75 tables are all zero at 15-21** (CONFIRMED), so the firmware's knob behaviour does not depend on these entries being set. Whether the W75's knob regions use slots 16-18 is unknown (layout file not available). Open question Q7.
- The `0x18` read parser maps non-layout slots only if they are listed as knob regions (`agreement.js:339-346`).
- **Our app:** keep the device's values for 15-21 unless the user explicitly remaps the knob.

### 4.11 Actuation and Rapid Trigger (0x21)

**Header:** `21 00 00 00 <len>`; byte 5 selects the function. Most requests use the constant `0x18` in byte 4 regardless of their real size; poll-rate read uses 1, poll-rate write 0, profile read/select 0. Send the same bytes as the site.

| Byte 5 | Dir | Function | Confidence |
|---|---|---|---|
| `00`, `0C`, `0D` | W | trigger write; byte 5 = mode (4.11.3) | CONFIRMED (`0D` in c10) |
| `00` | IN | trigger write ack `21 00 00 00 01 00` | OBSERVED |
| `01` | IN | live travel stream | CODE-ONLY |
| `02` / `03` | W | open / close travel test | CODE-ONLY |
| `04` | R | global trigger parameters | CONFIRMED |
| `05` | R | one key's trigger settings | CONFIRMED |
| `06` | W | **reset all triggers** | CONFIRMED (c11) |
| `07` | W | **single-key calibration** (no caller in the site) | CODE-ONLY |
| `08` | W / IN | **all-key calibration** start/stop and status (devices without the any-key flag) | CODE-ONLY |
| `09` | R / W | polling rate (section 4.6.1) | read CONFIRMED; write CODE-ONLY |
| `0F` / `10` | W / IN | **any-key calibration** start / stop and status (W75 path) | CODE-ONLY |
| `11` / `12` | R / W | active profile / select profile (section 4.5) | CONFIRMED |

GUESS: byte 5 acts as both sub-command and trigger mode; the firmware may treat `00`/`0C`/`0D` as three write sub-commands.

#### 4.11.1 `21/04`: global trigger parameters (CONFIRMED)

- Request `21 00 00 00 18 04` + zeros (`agreement.js:987-1005`). Reply `21 00 00 00 06 | 04 aa 02 02 00 05` (c01 +1177).

| Byte | W75 | Meaning | Confidence |
|---|---|---|---|
| 5 | `04` | echo | CONFIRMED |
| 6 | `AA` | max travel, low byte | CONFIRMED |
| 7 | `02` | rawUnit (0 treated as 10). W75 unit 0.02 mm | CONFIRMED |
| 8 | `02` | **unknown**, not read by the site | - |
| 9 | `00` | max travel, high byte -> max = 170 = **3.40 mm** | CONFIRMED (value); hi/lo split CODE-ONLY |
| 10 | `05` | min travel = 5 = **0.10 mm** | CONFIRMED |

If min travel is 0 the site substitutes a unit-dependent value (CODE-ONLY). Read `21/04` before interpreting any distance.

#### 4.11.2 `21/05`: read one key (CONFIRMED)

- Request `21 00 00 00 18 05 <row> <col>` + zeros (`agreement.js:1008-1045`). The index space allows columns 0-21; the W75 uses rows 0-5 and columns 0-15 (CONFIRMED read set).
- Reply len `0x0C`. All 246 replies in c01 and c03 were `05 00 55 55 01 01 00 00 00 00 <row> <col>`.

| Byte | W75 as read | Meaning | Confidence |
|---|---|---|---|
| 5 | `05` | echo | CONFIRMED |
| 6 | `00` | mode: `00` RT off, `0C` RT, `0D` RT with separate press/release | CONFIRMED (`0C`: c04-c06 W; `0D`: c10 W) |
| 7 | `55` | actuation travel, low byte (85 = 1.70 mm) | CONFIRMED |
| 8 | `55` | second travel value, low byte; equals byte 7; ignored by the site | GUESS: reset/release point |
| 9 | `01` | RT press sensitivity (interval1), low byte | CONFIRMED |
| 10 | `01` | RT release sensitivity (interval2), low byte | CONFIRMED |
| 11 | `00` | travel, high byte | CODE-ONLY |
| 12 | `00` | second travel, high byte | GUESS (mirrors the write) |
| 13 | `00` | press sensitivity, high byte | CODE-ONLY |
| 14 | `00` | release sensitivity, high byte | CODE-ONLY |
| 15 | row | echo of request byte 6 | OBSERVED (246/246) |
| 16 | col | echo of request byte 7 | OBSERVED (246/246) |

- As read on the W75: RT off, actuation 1.70 mm, sensitivity 0.02/0.02 mm on all 82 positions. Nothing shows that these are factory defaults.
- The site sends one request per layout key and assigns replies **by arrival order** (one lost reply shifts every later key). Our app must match on the row/col echo.
- The site's default for a fresh local profile differs: mode 0, travel floor(max/2) = 85, sensitivity 5/5 (`wmIndex.js:1055-1063, 4851-4861`).

#### 4.11.3 Trigger write (byte 5 = mode)

`setAnyTriggerValue` (`agreement.js:1798-1830`). One packet applies **one** value set to every key in the mask.

| Byte | Content | Notes |
|---|---|---|
| 0 | `21` | |
| 1-3 | `00 00 00` | |
| 4 | `18` | constant; the real data runs to byte 35 |
| 5 | mode | `00` RT off; `0C` RT, same press/release; `0D` RT, separate press/release (`wmIndex.js:827, 839, 851, 1903, 1915`) |
| 6-27 | 22-byte key mask | byte 6+col, bit = row (section 2.9) |
| 28 | travel, low byte | actuation point in units |
| 29 | travel, low byte (copy) | the site always repeats byte 28 |
| 30 | press sensitivity, low byte | interval1 |
| 31 | release sensitivity, low byte | interval2; equals byte 30 unless mode `0D` |
| 32, 33 | travel, high byte (twice) | |
| 34 | press sensitivity, high byte | |
| 35 | release sensitivity, high byte | |
| 36-62 | `00` | |

- Layout CONFIRMED for modes `00` and `0C` by the 15 W writes in c03 (+34733..+48277) and for mode `0D` by c10 (W press 0.30 mm `0f`, release 0.50 mm `19`).
- **Ack** `21 00 00 00 01 00`, 5-7 ms later (OBSERVED). The site neither handles nor waits for it. GUESS: byte 5 = status OK.
- **Read-back** (OBSERVED, c10): after the `0D` write W reads `05 0d 55 55 0f 19 00 00 00 00 02 02`. So write byte 28 maps to read byte 7 (payload p[2]), 29 to p[3], 30 to p[4], 31 to p[5], and the high bytes 32-35 to p[6..9]. The second travel value (byte 29 / p[3]) has only ever been written equal to the actuation point.
- **Key mask:** only single-key masks have been captured (c03 W; c10 Q mask byte 7 = `04`, W). A mask with more than one key has **never** been captured, so multi-key writes stay CODE-ONLY. OpenWraith writes one key per packet.

**c03 W edit, byte by byte** (CONFIRMED). The user set W to 1.00 mm, then enabled RT at 0.30 mm:

- +34733: travel `0x35` (1.06 mm), mode `00` (slider released after a drag from 1.70 mm). Then `0x34`, `0x33`, `0x32` as single steps (+/- button or arrow keys; the capture cannot tell which). Sensitivity stays `01 01` (the device values).
- Enabling RT: mode `0C`, sensitivity `19 19` = 25 = 0.50 mm (the slider's initial value floor(0.5/unit), never overwritten because W was in mode 0).
- Ten single steps `18 18` down to `0f 0f`. Final packet (+48277):

```
21 00 00 00 18 0c 00 00 04 00 x19 32 32 0f 0f 00 00 00 00 00 ...
byte 5  = 0c   mode RT, same press/release
byte 8  = 04   mask column 2, bit 2 (row 2) -> index 46 = W
byte 28 = 32   travel 50 = 1.00 mm (byte 29 copy)
byte 30 = 0f   press 0.30 mm; byte 31 = 0f release 0.30 mm; bytes 32-35 = 00
```

Ack 6 ms later. No save command followed.

#### 4.11.4 Empty-mask write at connect (CONFIRMED; do not copy)

After the `0x11` reply the site fires its RT checkbox handler with no keys selected (`agreement.js:641-643` -> `wmIndex.js:1902-1913`), sending mode 0, all-zero mask, travel and sensitivity 0 (c01 +3394, c03 +2835). The device acks `00`; the next trigger pass is unchanged. With nothing to apply, the ack says nothing about range checks.

#### 4.11.5 Multi-key edits and value ranges

- With several keys selected the site sends one packet: union mask + one value set. Site quirk: travel and mode come from the UI, but the sensitivities come from the **last** selected key (`agreement.js:1806-1813`). CODE-ONLY: a multi-key mask has never been captured (the Select-All part of capture plan C8 was skipped). OpenWraith sends one packet per key, each with that key's own values, until a hardware test confirms multi-key masks.
- `setAllTriggerValue` (grouping keys with identical settings, one packet per group) runs only in software-profile mode (`wmIndex.js:1087`), never on the W75.
- UI ranges (units; W75 unit 0.02 mm; CODE-ONLY):

| Field | Min | Max | Step | UI initial value | W75 in mm | Source |
|---|---|---|---|---|---|---|
| Actuation travel | min travel (5) | max travel (170) | 1 | key's value | 0.10-3.40 | `wmIndex.js:5393-5419` |
| RT press / release | 1 | ceil(max/2) = 85 | 1 | floor(0.5/unit) = 25 | 0.02-1.70 | `wmIndex.js:6411-6418` |
| Dead band top / bottom | 0 | ceil(1/unit) = 50 | 1 | 2 (0.04 mm) | 0.00-1.00 | `wmIndex.js:6419-6426` |

- With switch types enabled, the actuation maximum is the smallest floor(switch travel / unit) among the selected keys (HM1: 3.4 mm -> 170).
- Writes happen on slider release, once per +/- click, and on Enter/blur in the mm box.
- Firmware clamping is unknown: never send values outside these ranges. The site has no "continuous RT" option.

#### 4.11.6 `21/06`: reset all triggers (**DANGEROUS**, CONFIRMED in c11)

- Request `21 00 00 00 18 06` (`agreement.js:940-953`).
- "Reset Trigger" button: send, sleep 100 ms, re-read `21/04` and all `21/05` (`wmIndex.js:1382-1392`). Also step 2 of the full reset (section 4.6.3).
- **Site toggle (do not copy):** `resetTrigger` checks a flag `isResetTrigger` (initially true, `variable.js:361`). Any incoming `0x14` report clears it (`agreement.js:320-321`); a call with the flag clear only sets it again and sends nothing (`agreement.js:942-944`). So after any `0x14` reply (a Win-Lock change, a previous reset) the next trigger reset is silently skipped. In the full reset the check runs right after the `0x14` reset is sent, so that reset's own reply most likely arrives too late to matter (GUESS on timing).

#### 4.11.7 Travel test (CODE-ONLY)

- **Open:** `21 00 00 00 18 02` + 22-byte mask of all layout keys at bytes 6-27 (W75: the 82-position mask, section 2.9). Sent when the "Travel Test" switch turns on (`wmIndex.js:811-818`).
- **Close:** `21 00 00 00 18 03`. Sent when the switch turns off, when leaving the Actuation tab while the test is on (`wmIndex.js:1187-1190`), and **every time the Calibration sub-tab is selected**, even if the test was never on (`wmIndex.js:742-746`).
- **Stream** while open: input `0x21` with byte 5 = `01`; depth = byte 9 << 8 | byte 8 (units). Bytes 6, 7 are not used by the site (GUESS: row/col of the reporting key). The site draws one bar.

#### 4.11.8 Calibration (**DANGEROUS**, CODE-ONLY)

Requests `21 00 00 00 18 <sub> <arg>` (`agreement.js:1884-1895`; UI `wmIndex.js:1946-1998`):

| Device | Start | Stop |
|---|---|---|
| Any-key calibration flag set (**W75**, flags A bit 0) | `0F 00` | `10 00` |
| Otherwise | `08 00` | `08 01` |

- Single key: `21 .. 18 07 <row> <col>` exists but nothing calls it.
- Procedure (UI text): Start; press each key to the bottom; wait for its colour to change; Stop. The button is disabled 2 s after each click. The site sends Stop automatically when the user leaves the Calibration sub-tab or the Actuation tab.
- Status input, byte 5 = `08` or `0F` (`agreement.js:526-568`): byte 6 = 0 finished/stopped; 1 progress, bytes 7-28 = 22-byte mask of keys calibrated so far; 2 or 4 failed. No handler for byte 5 = `10`. Nothing captured yet.

### 4.12 Dead band (0x26)

`readDeadBand`, `setDeadBand`, `resetDeadBand` (`agreement.js:2180-2244`; parser `878-895`).

- **Read** (CONFIRMED framing): request `26 00 00 00 00` + zeros (c01 +1736). Reply: 5 chunks, byte 3 = 0..4, byte 4 = `3A 3A 3A 3A 20` = 264 bytes = 132 keys x 2. Chunk p, pair i -> key p x 29 + i. Byte 2i = **top**, 2i+1 = **bottom** (order CODE-ONLY: every captured value is 0). W75: all zero.
- **Write** (CONFIRMED, c12; CAUTION): `26 01 <chunk hi> <chunk lo> <len>` + data; flat array [2 x idx] = top, [2 x idx + 1] = bottom; chunks 58, 58, 58, 58, 32; positions without a key sent as 0. W (index 46) = flat offsets 92/93 = chunk 1, report bytes 39/40: W top 0.10 mm wrote `05` at 92 and bottom 0.20 mm `0a` at 93. Each chunk is acked `26 01 00 00 00` (section 2.7). The site writes the whole table on every slider change (`wmIndex.js:1926-1945`; 37 full writes in about 20 s in c12).
- **Non-zero read-back and persistence** (CONFIRMED on hardware, 2026-10-24, OpenWraith probe): writing W top 0.10 mm / bottom 0.20 mm with `26 01` alone, waiting for the commit and switching profile 2 -> 1 read back `05 0a` at W, so the read order equals the write order (byte 2 x idx = top) and a dead-band write is saved on its own (unlike the base keymap, section 4.8.2). The value was then set back to `00 00`.
- UI: 0-1.00 mm, initial 0.04 mm; help text recommends 0.1 mm. GUESS: top = ignored travel from rest, bottom = ignored travel before bottom-out.
- **"Reset dead band" is the read request** (CODE-ONLY): `resetDeadBand` sends exactly `26 00` (`agreement.js:2235-2244` vs `2180-2189`). Used only in the full reset. GUESS: it resets nothing; the `0x14` reset may clear dead bands itself.
- **`isInitDeadBand` flag** (verified): the reply handler sets it when page 4 arrives (`agreement.js:892-894`); `readDeadBand` clears it after its bounded wait (`agreement.js:2190-2195`). So it is normally false, but after `resetDeadBand` (which never clears it) or a page 4 that arrives after the wait, it stays true until the next `readDeadBand`. While true, the full reset skips `26 00` and a software-mode profile push skips the dead band.

### 4.13 Switch types (0x25)

- **List** (CONFIRMED): `25 00` -> `25 00 00 00 03 | 00 01 01` (c01 +1726): count BE16 = 1, ID 1. ID 1 = **HM1** in `config/switch.json` (3.4 mm), shown as "Magnetic Switch" ("Wraith Flow Magnetic Switch" on Wraith, `language.js:56-61`).
- **Per-key table** (CONFIRMED framing): `25 02` -> 3 chunks, byte 4 = `3A 3A 10` = 132 bytes, one switch ID per key index (key = chunk x 58 + i). W75: all `01`, including positions without a key. Chunk 2 carries stale `01` bytes after its 16 valid bytes (OBSERVED).
- **Write** (CODE-ONLY, CAUTION): `25 01 <chunk hi> <chunk lo> <len> <ids>`, chunks 58, 58, 16. Positions without a key are sent as 0 (differs from the `01` the device reports). Sent by clicking a switch button with keys selected (`wmIndex.js:4884-4891`).
- Catalogue: IDs 1-26, mostly 3.4 mm (IDs 7, 10, 24: 3.9 mm; 16: 3.6 mm; 22, 23: 3.5 mm; 26: 3.3 mm).
- Effect: in the site the switch type only changes the UI's max actuation; the help text asks for recalibration after changing switches. GUESS: the firmware picks a magnet/ADC curve from the ID. Moot on the W75 (HM1 only).

### 4.14 Advanced keys (0x22)

- **Read** (CONFIRMED framing): `22 80 00 00 00` -> 40 chunks, bytes 2-3 = slot (BE16), byte 4 = `0x28` (40), byte 5 = type. W75: all 40 slots all `00` (c01 +1008). One read = 1600 bytes.
- **Write** (CAUTION): one packet per slot, `22 <slot> 00 00 32 <type> ...` (byte 4 = 50). On every keymap Apply the site writes **all 40 slots**; empty slots go out as zeros, clearing them. Each write is acked `22 00 00 00 00` (section 2.7); the site parses no reply. Framing and acks CONFIRMED with all-zero content (c09, c13: 240 writes); **non-empty slots have never been captured**, so their write format stays CODE-ONLY. Read 40 B / write 50 B: the sizes are asymmetric. OpenWraith only ever writes all-zero slots (clearing a slot).
- 40 slots; at most 40 entries, at most 5 MDS.

| Type (byte 5) | Name | Offered on W75 |
|---|---|---|
| 0 | empty | |
| 1 | DKS (dynamic keystroke) | yes |
| 2 | MT (tap / hold) | yes |
| 3 | TGL (toggle) | yes |
| 4 | RS (deeper of two keys wins) | no (flag B bit 3 = 0) |
| 5 | RKRT (send another key on release) | no |
| 6 | MDS (delay, then up to 6 keys) | no |

**Action code:** 4 bytes in keymap format (code1, hid, code3, code4); code1 0 = keyboard key. Mouse (code1 `0x12`) is written kind-first (`12 <code3> <hid> <code4>`) only in DKS actions and keymap entries; MT/TGL/RKRT write it unswapped; the site's DKS reader does not swap back (site bug). Moot on W75 (DKS mouse flag 0). Our app should use one canonical form `12 <kind> <value> 00`.

**DKS (type 1)** (CODE-ONLY):

| Byte | Field |
|---|---|
| 6 | route1: first press point, **0.1 mm** units; UI default 15 (1.5 mm), range 1..route2 |
| 7 | route2: deep press point = max travel x 10 - 4 = 30 (3.0 mm) on the W75; not editable |
| 8 | route3: release-from-bottom point; always = route2 |
| 9 | route4: final release point; the UI forces it = route1 |
| 10+8k .. 13+8k | action for key slot k = 0..3 |
| 14+8k .. 17+8k | step byte for slot k at stages 0..3 |

Step values the UI produces: 0 nothing, 1 single tap, 2 held through this stage, 3 new hold starting right after a held stage. Stage 3 only gets 0 or 1. GUESS for the firmware: 1 tap, 2 hold, 3 release then press again.

**MT (type 2):** bytes 6-9 = **hold** action, 10-13 = **tap** action, 14 = hold threshold in 10 ms units (UI 15-100 = 150-1000 ms, default 15). CODE-ONLY.

**TGL (type 3):** bytes 6-9 = action. CODE-ONLY.

**RS (type 4), RKRT (type 5), MDS (type 6)** (CODE-ONLY, not offered on the W75): RS bytes 6-9 key A code, 10/11 A row/col, 12-15 key B code, 16/17 B row/col. RKRT bytes 6-9 the bound key, 12-15 the key sent on release. MDS bytes 6-7 delay ms BE16, 8 mode (0 after delay, 1 after all keys), 9 key count <= 6, 10-15 HID usages.

**Binding to a key** (CODE-ONLY): DKS/MT/TGL/RKRT (and RS bound from the list) set the key's `0x18` entry to `17 <slot> <slot> 00`. RS created from its dialog binds both member keys with `17 00 00 00` (the firmware presumably finds them by row/col). MDS is never bound.

### 4.15 SOCD (0x24)

The site calls it "PRCS"; the UI tab is "SOCD". Code `agreement.js:2029-2106`, parser `800-841`.

- **Power and hotkey read** (CONFIRMED): `24 02` -> `24 02 00 00 06 | 00 00 00 00 fa e5` (c01 +1080).
  - p[0] = power (1 = on); W75: **off**.
  - p[1..3] unknown (0).
  - p[4], p[5] shown as "Hotkey: Fn + R-Shift": the site hard-codes `0xFA` as "Fn"; `0xE5` = R-Shift. GUESS: the on-keyboard SOCD toggle hotkey.
- **Power write** (CODE-ONLY): `24 03 00 00 01 <0|1>`.
- **Pairs read** (CONFIRMED framing): `24 01` -> 2 chunks (byte 3 = 0, 1), byte 4 = `0x28` = 10 entries x 4 bytes each; W75 all zero. Entry `[active = 1, model, key1 HID, key2 HID]` (CODE-ONLY); empty if byte 0 = 0 or the entry is `07 FF FF FF`.
- **Pairs write** (CODE-ONLY, CAUTION): 2 packets `24 00 <chunk hi> <chunk lo> 28` + 10 entries; unused entries zero; at most 20 pairs. On non-US layouts the site sends key2 `0x31` as `0x32`. On Apply and on every power toggle the site sends power, then both data chunks (`wmIndex.js:2383-2393`); nothing is sent if any pair is incomplete.

| Model | UI meaning |
|---|---|
| 0 | the key pressed first is interrupted (last input wins) |
| 1 | key1 interrupts key2 |
| 2 | key2 interrupts key1 |
| 3 | the later key is not sent; it only interrupts the first |

---

## 5. W75 key table

**What is confirmed and what is not:**

- The **82 positions** (row, col, index) are CONFIRMED: they are exactly the positions the site reads with `21/05`, in the same order in all three read passes of c01 and of c03, and the device echoes each row/col.
- The **names and HID codes** are CONFIRMED (2026-10-24, capture plan C1 done): the site's layout file `https://wraith.software/config/keys/W75HEARGB.json?t=v1` (layout type `us`, 82 keys with `index, name, code, hidCode, x, y, width, height`) was checked mechanically against this table. All 82 indices match and all 80 HID codes match; the two remaining entries are the knob (index 15, code `RotaryKnob`, hidCode `0`, empty name) and Fn (index 121, code `KeyFn`, hidCode `0x00`). The file is kept outside the repo for reference only and must not be committed.
- The **x, y, w** column is still the screenshot estimate; the layout file has exact pixel geometry, which our app should re-derive (in our own key-unit format) rather than copy.
- The structure offsets are derived from the index (CONFIRMED formulas for `0x21` mask and `0x09`; CODE-ONLY for `0x18`, `0x25`, `0x26`). The table was generated and checked mechanically against c01 (positions) and c02 (painted colour slots = these 82 minus the knob).

**Column guide:**

- **Mask byte / bit:** report byte (6 + col) and bit value (1 << row) in any 22-byte key mask (`0x21` trigger write bytes 6-27, `0x0E`, travel-test, calibration progress).
- **0x09 c:o** = chunk (byte 3) : payload offset of the R byte (G, B follow) in the colour tables. Report byte = 5 + offset.
- **0x18 c:o** = chunk : payload offset of b0 of the 4-byte keymap entry.
- **0x25 c:o** = chunk : payload offset of the switch-type byte.
- **0x26 c:o** = chunk : payload offset of the dead-band top byte (bottom = +1).
- **HID** = standard USB HID usage for the legend. It is a GUESS that the layout file uses exactly this code; the factory keymap tables are all zero, so the device does not tell us.
- **x, y, w** = approximate position and width in key units measured from the screenshot (GUESS; for drawing only).
- **Evidence:** C = confirmed by a user action and capture; I+ = inferred with an independent anchor; I = inferred from row order only.

| Key | Row | Col | Index | Mask byte / bit | 0x09 c:o | 0x18 c:o | 0x25 c:o | 0x26 c:o | HID | x, y, w | Evidence |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Esc | 0 | 0 | 0 | 6 / `0x01` | 0:0 | 0:0 | 0:0 | 0:0 | 0x29 | 0,0,1 | I |
| F1 | 0 | 2 | 2 | 8 / `0x01` | 0:6 | 0:8 | 0:2 | 0:4 | 0x3A | 1.25,0,1 | I |
| F2 | 0 | 3 | 3 | 9 / `0x01` | 0:9 | 0:12 | 0:3 | 0:6 | 0x3B | 2.25,0,1 | I |
| F3 | 0 | 4 | 4 | 10 / `0x01` | 0:12 | 0:16 | 0:4 | 0:8 | 0x3C | 3.25,0,1 | I |
| F4 | 0 | 5 | 5 | 11 / `0x01` | 0:15 | 0:20 | 0:5 | 0:10 | 0x3D | 4.25,0,1 | I |
| F5 | 0 | 6 | 6 | 12 / `0x01` | 0:18 | 0:24 | 0:6 | 0:12 | 0x3E | 5.5,0,1 | I |
| F6 | 0 | 7 | 7 | 13 / `0x01` | 0:21 | 0:28 | 0:7 | 0:14 | 0x3F | 6.5,0,1 | I |
| F7 | 0 | 8 | 8 | 14 / `0x01` | 0:24 | 0:32 | 0:8 | 0:16 | 0x40 | 7.5,0,1 | I |
| F8 | 0 | 9 | 9 | 15 / `0x01` | 0:27 | 0:36 | 0:9 | 0:18 | 0x41 | 8.5,0,1 | I |
| F9 | 0 | 10 | 10 | 16 / `0x01` | 0:30 | 0:40 | 0:10 | 0:20 | 0x42 | 9.75,0,1 | I |
| F10 | 0 | 11 | 11 | 17 / `0x01` | 0:33 | 0:44 | 0:11 | 0:22 | 0x43 | 10.75,0,1 | I |
| F11 | 0 | 12 | 12 | 18 / `0x01` | 0:36 | 0:48 | 0:12 | 0:24 | 0x44 | 11.75,0,1 | I |
| F12 | 0 | 13 | 13 | 19 / `0x01` | 0:39 | 0:52 | 0:13 | 0:26 | 0x45 | 12.75,0,1 | I |
| Del | 0 | 14 | 14 | 20 / `0x01` | 0:42 | 1:0 | 0:14 | 0:28 | 0x4C | 14,0,1 | I |
| Knob (RotaryKnob) | 0 | 15 | 15 | 21 / `0x01` | 0:45 | 1:4 | 0:15 | 0:30 | see note | 15.3,0,1 | I+ (knob panel, not in Select All) |
| Grave ~ | 1 | 0 | 22 | 6 / `0x02` | 1:12 | 1:32 | 0:22 | 0:44 | 0x35 | 0,1.25,1 | I |
| 1 ! | 1 | 1 | 23 | 7 / `0x02` | 1:15 | 1:36 | 0:23 | 0:46 | 0x1E | 1,1.25,1 | I |
| 2 @ | 1 | 2 | 24 | 8 / `0x02` | 1:18 | 1:40 | 0:24 | 0:48 | 0x1F | 2,1.25,1 | I |
| 3 # | 1 | 3 | 25 | 9 / `0x02` | 1:21 | 1:44 | 0:25 | 0:50 | 0x20 | 3,1.25,1 | I |
| 4 $ | 1 | 4 | 26 | 10 / `0x02` | 1:24 | 1:48 | 0:26 | 0:52 | 0x21 | 4,1.25,1 | I |
| 5 % | 1 | 5 | 27 | 11 / `0x02` | 1:27 | 1:52 | 0:27 | 0:54 | 0x22 | 5,1.25,1 | I |
| 6 ^ | 1 | 6 | 28 | 12 / `0x02` | 1:30 | 2:0 | 0:28 | 0:56 | 0x23 | 6,1.25,1 | I |
| 7 & | 1 | 7 | 29 | 13 / `0x02` | 1:33 | 2:4 | 0:29 | 1:0 | 0x24 | 7,1.25,1 | I |
| 8 * | 1 | 8 | 30 | 14 / `0x02` | 1:36 | 2:8 | 0:30 | 1:2 | 0x25 | 8,1.25,1 | I |
| 9 ( | 1 | 9 | 31 | 15 / `0x02` | 1:39 | 2:12 | 0:31 | 1:4 | 0x26 | 9,1.25,1 | I |
| 0 ) | 1 | 10 | 32 | 16 / `0x02` | 1:42 | 2:16 | 0:32 | 1:6 | 0x27 | 10,1.25,1 | I |
| - _ | 1 | 11 | 33 | 17 / `0x02` | 1:45 | 2:20 | 0:33 | 1:8 | 0x2D | 11,1.25,1 | I |
| = + | 1 | 12 | 34 | 18 / `0x02` | 1:48 | 2:24 | 0:34 | 1:10 | 0x2E | 12,1.25,1 | I |
| Backspace | 1 | 14 | 36 | 20 / `0x02` | 2:0 | 2:32 | 0:36 | 1:14 | 0x2A | 13,1.25,2 | I |
| Home | 1 | 15 | 37 | 21 / `0x02` | 2:3 | 2:36 | 0:37 | 1:16 | 0x4A | 15.3,1.25,1 | I |
| Tab | 2 | 0 | 44 | 6 / `0x04` | 2:24 | 3:8 | 0:44 | 1:30 | 0x2B | 0,2.25,1.5 | I |
| Q | 2 | 1 | 45 | 7 / `0x04` | 2:27 | 3:12 | 0:45 | 1:32 | 0x14 | 1.5,2.25,1 | I |
| W | 2 | 2 | 46 | 8 / `0x04` | 2:30 | 3:16 | 0:46 | 1:34 | 0x1A | 2.5,2.25,1 | C |
| E | 2 | 3 | 47 | 9 / `0x04` | 2:33 | 3:20 | 0:47 | 1:36 | 0x08 | 3.5,2.25,1 | I |
| R | 2 | 4 | 48 | 10 / `0x04` | 2:36 | 3:24 | 0:48 | 1:38 | 0x15 | 4.5,2.25,1 | I |
| T | 2 | 5 | 49 | 11 / `0x04` | 2:39 | 3:28 | 0:49 | 1:40 | 0x17 | 5.5,2.25,1 | I |
| Y | 2 | 6 | 50 | 12 / `0x04` | 2:42 | 3:32 | 0:50 | 1:42 | 0x1C | 6.5,2.25,1 | I |
| U | 2 | 7 | 51 | 13 / `0x04` | 2:45 | 3:36 | 0:51 | 1:44 | 0x18 | 7.5,2.25,1 | I |
| I | 2 | 8 | 52 | 14 / `0x04` | 2:48 | 3:40 | 0:52 | 1:46 | 0x0C | 8.5,2.25,1 | I |
| O | 2 | 9 | 53 | 15 / `0x04` | 2:51 | 3:44 | 0:53 | 1:48 | 0x12 | 9.5,2.25,1 | I |
| P | 2 | 10 | 54 | 16 / `0x04` | 3:0 | 3:48 | 0:54 | 1:50 | 0x13 | 10.5,2.25,1 | I |
| [ { | 2 | 11 | 55 | 17 / `0x04` | 3:3 | 3:52 | 0:55 | 1:52 | 0x2F | 11.5,2.25,1 | I |
| ] } | 2 | 12 | 56 | 18 / `0x04` | 3:6 | 4:0 | 0:56 | 1:54 | 0x30 | 12.5,2.25,1 | I |
| Backslash | 2 | 14 | 58 | 20 / `0x04` | 3:12 | 4:8 | 1:0 | 2:0 | 0x31 | 13.5,2.25,1.5 | I |
| PgUp | 2 | 15 | 59 | 21 / `0x04` | 3:15 | 4:12 | 1:1 | 2:2 | 0x4B | 15.3,2.25,1 | I |
| Caps Lock | 3 | 0 | 66 | 6 / `0x08` | 3:36 | 4:40 | 1:8 | 2:16 | 0x39 | 0,3.25,1.75 | I |
| A | 3 | 2 | 68 | 8 / `0x08` | 3:42 | 4:48 | 1:10 | 2:20 | 0x04 | 1.75,3.25,1 | I+ (factory Custom1) |
| S | 3 | 3 | 69 | 9 / `0x08` | 3:45 | 4:52 | 1:11 | 2:22 | 0x16 | 2.75,3.25,1 | I+ (factory Custom1) |
| D | 3 | 4 | 70 | 10 / `0x08` | 3:48 | 5:0 | 1:12 | 2:24 | 0x07 | 3.75,3.25,1 | I+ (factory Custom1) |
| F | 3 | 5 | 71 | 11 / `0x08` | 3:51 | 5:4 | 1:13 | 2:26 | 0x09 | 4.75,3.25,1 | I |
| G | 3 | 6 | 72 | 12 / `0x08` | 4:0 | 5:8 | 1:14 | 2:28 | 0x0A | 5.75,3.25,1 | I |
| H | 3 | 7 | 73 | 13 / `0x08` | 4:3 | 5:12 | 1:15 | 2:30 | 0x0B | 6.75,3.25,1 | I |
| J | 3 | 8 | 74 | 14 / `0x08` | 4:6 | 5:16 | 1:16 | 2:32 | 0x0D | 7.75,3.25,1 | I |
| K | 3 | 9 | 75 | 15 / `0x08` | 4:9 | 5:20 | 1:17 | 2:34 | 0x0E | 8.75,3.25,1 | I |
| L | 3 | 10 | 76 | 16 / `0x08` | 4:12 | 5:24 | 1:18 | 2:36 | 0x0F | 9.75,3.25,1 | I |
| ; : | 3 | 11 | 77 | 17 / `0x08` | 4:15 | 5:28 | 1:19 | 2:38 | 0x33 | 10.75,3.25,1 | I |
| ' " | 3 | 12 | 78 | 18 / `0x08` | 4:18 | 5:32 | 1:20 | 2:40 | 0x34 | 11.75,3.25,1 | I |
| Enter | 3 | 14 | 80 | 20 / `0x08` | 4:24 | 5:40 | 1:22 | 2:44 | 0x28 | 12.75,3.25,2.25 | I |
| PgDn | 3 | 15 | 81 | 21 / `0x08` | 4:27 | 5:44 | 1:23 | 2:46 | 0x4E | 15.3,3.25,1 | I |
| L-Shift | 4 | 0 | 88 | 6 / `0x10` | 4:48 | 6:16 | 1:30 | 3:2 | 0xE1 | 0,4.25,2.25 | I |
| Z | 4 | 2 | 90 | 8 / `0x10` | 5:0 | 6:24 | 1:32 | 3:6 | 0x1D | 2.25,4.25,1 | I |
| X | 4 | 3 | 91 | 9 / `0x10` | 5:3 | 6:28 | 1:33 | 3:8 | 0x1B | 3.25,4.25,1 | I |
| C | 4 | 4 | 92 | 10 / `0x10` | 5:6 | 6:32 | 1:34 | 3:10 | 0x06 | 4.25,4.25,1 | I |
| V | 4 | 5 | 93 | 11 / `0x10` | 5:9 | 6:36 | 1:35 | 3:12 | 0x19 | 5.25,4.25,1 | I |
| B | 4 | 6 | 94 | 12 / `0x10` | 5:12 | 6:40 | 1:36 | 3:14 | 0x05 | 6.25,4.25,1 | I |
| N | 4 | 7 | 95 | 13 / `0x10` | 5:15 | 6:44 | 1:37 | 3:16 | 0x11 | 7.25,4.25,1 | I |
| M | 4 | 8 | 96 | 14 / `0x10` | 5:18 | 6:48 | 1:38 | 3:18 | 0x10 | 8.25,4.25,1 | I |
| , < | 4 | 9 | 97 | 15 / `0x10` | 5:21 | 6:52 | 1:39 | 3:20 | 0x36 | 9.25,4.25,1 | I |
| . > | 4 | 10 | 98 | 16 / `0x10` | 5:24 | 7:0 | 1:40 | 3:22 | 0x37 | 10.25,4.25,1 | I |
| / ? | 4 | 11 | 99 | 17 / `0x10` | 5:27 | 7:4 | 1:41 | 3:24 | 0x38 | 11.25,4.25,1 | I |
| R-Shift | 4 | 13 | 101 | 19 / `0x10` | 5:33 | 7:12 | 1:43 | 3:28 | 0xE5 | 12.25,4.25,1.75 | I |
| Up | 4 | 14 | 102 | 20 / `0x10` | 5:36 | 7:16 | 1:44 | 3:30 | 0x52 | 14.15,4.4,1 | I |
| L-Ctrl | 5 | 0 | 110 | 6 / `0x20` | 6:6 | 7:48 | 1:52 | 3:46 | 0xE0 | 0,5.25,1.25 | I |
| L-Win | 5 | 1 | 111 | 7 / `0x20` | 6:9 | 7:52 | 1:53 | 3:48 | 0xE3 | 1.25,5.25,1.25 | I |
| L-Alt | 5 | 2 | 112 | 8 / `0x20` | 6:12 | 8:0 | 1:54 | 3:50 | 0xE2 | 2.5,5.25,1.25 | I |
| Space | 5 | 6 | 116 | 12 / `0x20` | 6:24 | 8:16 | 2:0 | 4:0 | 0x2C | 3.75,5.25,6.25 | I |
| R-Alt | 5 | 10 | 120 | 16 / `0x20` | 6:36 | 8:32 | 2:4 | 4:8 | 0xE6 | 10,5.25,1 | I |
| Fn | 5 | 11 | 121 | 17 / `0x20` | 6:39 | 8:36 | 2:5 | 4:10 | see note | 11,5.25,1 | I+ (only 0x0E bit) |
| R-Ctrl | 5 | 12 | 122 | 18 / `0x20` | 6:42 | 8:40 | 2:6 | 4:12 | 0xE4 | 12,5.25,1 | I |
| Left | 5 | 13 | 123 | 19 / `0x20` | 6:45 | 8:44 | 2:7 | 4:14 | 0x50 | 13.15,5.4,1 | I |
| Down | 5 | 14 | 124 | 20 / `0x20` | 6:48 | 8:48 | 2:8 | 4:16 | 0x51 | 14.15,5.4,1 | I |
| Right | 5 | 15 | 125 | 21 / `0x20` | 6:51 | 8:52 | 2:9 | 4:18 | 0x4F | 15.15,5.4,1 | I |

**Notes on the key table**

- **Knob (index 15):** no HID usage. The site reads and writes trigger data for it like any key and writes its stored colour into `0x09` tables, but Select All never includes it. Whether it has any Hall-effect meaning is unknown.
- **Fn (index 121):** the device's base and Fn tables contain `00 00 00 00` here. The site would write `01 <layout hid> 00 00` for a `KeyFn` entry without code1, and labels a read-back `00 FA` as Fn. What the layout file and firmware really use is unknown. Keep the device's value.
- **Unused positions** (never read via `21/05`, written as 0 by the site in whole-table writes):
  - row 0: col 1, 16-21 (indices 16-21 are knob keymap slots, section 4.10)
  - row 1: col 13, 16-21
  - row 2: col 13, 16-21
  - row 3: col 1, 13, 16-21
  - row 4: col 1, 12, 15-21
  - row 5: col 3-5, 7-9, 16-21
- **Factory colour anomaly:** the device's original Custom1 table lit slots 103 (row 4, col 15) and 126 (row 5, col 16), which are not positions in this table, plus 124 and 125 (Down, Right). That looks like "WASD + arrows" shifted by one from the expected 102/123/124/125. Either the preset was authored for another matrix, or LED slots differ from key indices in the arrow cluster. Open question Q8.

---

## 6. Site state model, the "settings reset" root cause, and what our app must store

### 6.1 Persistence (root cause of the "settings reset" report)

**Verified facts carried over from the earlier draft (2026-10-24):**

1. Settings written with `0x07 00` survive a USB unplug/replug without any save command, so the device persists them itself. USER-REPORTED. In addition CONFIRMED by capture: the last `07 00` record and the final `09 00` per-key table written in c02 read back unchanged in c03, a later page session.
2. The 1 s `0x01` heartbeat is not required to keep settings. USER-REPORTED (blocked for 90 s, nothing changed); CODE-ONLY reasoning in section 4.2.
3. The site keeps a mirror of all 5 profiles in `localStorage["W75HEARGB"]` (about 37 KB: lighting, side light, keymaps, Fn keys, macros, advanced keys, per-key triggers for 82 keys). USER-REPORTED (inspected in the browser). **Correction:** for the W75 the site does **not** render its UI from that mirror. On every connect the device's `0x11` reply makes the site delete the mirror, rebuild its state from device reads, and write the mirror again (section 6.5).

**Updated conclusion:**

- The keyboard (V3.22.07) answers the onboard-profile query `21/11` (CONFIRMED), so the site runs in **onboard-profile mode**. In this mode:
  - connecting writes exactly one packet, the no-op empty-mask trigger write (CONFIRMED: c01 +3394, c03 +2835; three byte-identical trigger passes);
  - clicking a profile tab sends exactly one packet, `21 00 00 00 00 12 PP` (CODE-ONLY until captured; capture plan C3, C4);
  - the site never pushes stored or default profile data to the keyboard (CODE-ONLY for profile clicks; CONFIRMED for connect).
- **The earlier hypothesis "clearing browser data makes the site push defaults on the next profile switch" is refuted for this keyboard and firmware.** Clearing site data loses only data that never lived on the keyboard (section 6.8). The precondition (onboard mode) is CONFIRMED; the profile-switch path itself is CODE-ONLY until capture C3/C4.
- The hypothesis exactly describes the site's **software-profile mode** (section 6.6). The site uses it only if no `0x11` report arrives: GUESS that older W75 firmware lacked `21/11` (unknown, open question Q31), or a tab click in the roughly 45-70 ms between the UI appearing (after the dead-band read, about +1744 in c01) and the `0x11` reply being processed (+1811).

**Remaining candidate explanations for a "reset"** (to be tested; none is proven):

| # | Mechanism | Confidence |
|---|---|---|
| a | Different onboard profiles hold different settings; a profile change from the site or on the keyboard (Fn + 1..4) shows another profile's data | GUESS (per-profile scope unverified, Q3) |
| b | **Stale writes after a profile switch.** After `21/12` the site re-reads nothing until a `0x11` report arrives (`wmIndex.js:1017-1023`). Any whole-table write in that window (keymap Apply: `19`, `18 00`, `18 02`, `22`; dead band `26 01`; switch `25 01`; colours `09 00/01`) copies the old profile's data into the new one. Even after a `0x11` report the site does not re-read side light, colour tables, switch types, poll rate, `0E` or status. Without a `0x11` report, clicking back to the original tab sends nothing. | mechanism CODE-ONLY; effect GUESS |
| c | **Lossy whole-table rewrites.** Every keymap Apply rewrites all 10 macro slots, both keymaps and all 40 advanced-key slots from the site's model; entries the site cannot decode are dropped; knob slots 16-21 are forced to defaults | CODE-ONLY |
| d | **Reset paths.** The Reset button sends `14 00 00 00 01 01`. **Fn + Space held for about 3-5 s resets the keyboard; a short tap does nothing.** On reset the keyboard sends an unsolicited `14 00 00 00 01 01` input report; if the site is open it immediately answers with its own `14 00 00 00 01 01` plus `26` (dead band) and `24 03 00 00 01` (SOCD), then re-reads `07 01` and all 82 triggers (c07, +60.15 s .. +61.43 s). After the reset every key reads `00 55 55 01 01` (1.70 mm, RT off) and the main light reads `02 04 03 ff 00 00` (mode 2, colour kept). The final ack is a bare `14` with zero payload, so the site does not loop. Whether the reset clears all onboard profiles or only the active one is still open (Q4). | CONFIRMED (c07; long-press timing USER-REPORTED) |
| e | **Colour slot wipe.** Every `09` write zeroes slots outside the site's layout (factory slots 103/126 erased on the first edit) | CONFIRMED |
| f | **Software-mode push** in the short connect window, or on firmware without `21/11` | CODE-ONLY |

### 6.2 Which site build runs on wraith.software

`index.html` always loads `v1.html` for `wraith.software` and strips the version switch (`index.html:30, 46, 51, 57, 80`). Everything in this document describes v1. The "config import/export" phrase in the meta description belongs to both pages; v1 has no import/export (section 6.11). CODE-ONLY.

### 6.3 Browser storage used by the site

| Key | Written at | Content | On the keyboard? |
|---|---|---|---|
| `<product>` (e.g. `W75HEARGB`) | `wmIndex.js` 1136, 1144, 1349, 1540, 1731, 2374, 2392, 2486, 2902, 4074, 4765, 4986, 6410; removed at `agreement.js:577` on every `0x11` report | profile-set mirror (JSON, section 6.4) | partial copy only |
| `language` | `wmIndex.js:614` | UI language | no |
| `theme` | `themes.js:262` | UI theme | no |
| `versionSwitch.current` | `version-switch.js` (inactive on wraith.software) | v1/v2 | no |

- No device state in sessionStorage, cookies, IndexedDB or Cache Storage.
- **Clear Cache** (Settings) clears all of localStorage (not only the mirror), sessionStorage, cookies, IndexedDB, service workers and Cache Storage, then reloads. **It sends no HID packet** (`clearCache.js:10-25, 110-115`). The reload reconnects automatically to an already-permitted device (`navigator.hid.getDevices()`, `wmIndex.js:4200-4207, 4262-4297`).
- All four W75 PIDs map to the same key `W75HEARGB`.

### 6.4 Mirror schema (`localStorage[product]`, CODE-ONLY)

Top level `{ curIndex: 0..4, profiles: [5 x Profile] }`; JavaScript Maps are stored as arrays of `[key, value]` pairs (`wmIndex.js:5535-5541`), read back by `jsonToProfileSet` / `objTo*` (`wmIndex.js:5542-5766`). Missing key: 5 default profiles are built. Present with exactly 5 profiles: `curProfile = profiles[curIndex]`. Present with another count: no defaults are built and `curProfile` is never assigned (GUESS: later code fails).

| Field | Content | Keyboard counterpart | Notes |
|---|---|---|---|
| `index` | 0-4 | - | |
| `name` | string | **none** | storage only |
| `light` | power, mode, brightness, speed, direction, fullColor, fg/bg colour, `keysColor1/2` | `07`; `09` tables | per-key colour Maps are saved but not restored |
| `sideLight` | same | `08` | never filled from the keyboard at connect |
| `colorArray1`-`4` | 20 colours each (per-mode fg/bg memory, main/side) | **none** | restored fully |
| `defKeys`, `fnKeys` | Map keyIndex -> remapped key (9 fields) | `18 80`/`18 82` | only non-default keys |
| `macros` | list of {index, name, steps} | `19` slots (bound macros only) | |
| `advKeys` | list of {index 0-39, type, duration, mode, keys, route1-4, steps[16]} | `22` | `mode` saved but not restored |
| `defRecMacros`, `fnRecMacros` | Map keyIndex -> {index, type, count} | keymap `10` entries + slot header | |
| `triggers` | Map keyIndex -> {mode, travel, interval1, interval2, deadbandTop, deadbandBottom} | `21/05`, `26` | units of 0.02 mm on W75 |
| `prcsPower`, `prcses` | SOCD on/off and pairs {index, name, model, key1, key2} | `24` | |
| `other` | Win-Lock bit field (default 1) | status byte 20 / `14 00` bytes 12-15 | set from the keyboard at connect |
| `is6Key` | bool | status byte 22 / `14 00` byte 18 | saved, never restored, never read from the keyboard into the profile |

### 6.5 The mirror in onboard-profile mode

What the site does on each `0x11` report (`agreement.js:572-654`; order CONFIRMED by c01 +1812..+4282):

1. Sets `profileSet.curIndex` to the reported profile.
2. **Removes** `localStorage[product]` (`agreement.js:577`).
3. Hides the "Profile Default" tab; the user sees Profile 1-4.
4. Re-reads the subset in section 4.5.4; clears keymaps, macro bindings, macros, advanced keys and SOCD in memory first.
5. Ticks the matching profile tab and fires its handler (no packet, the index equals `presetNumber`); shows "Success" only if the index changed.
6. Clears the key selection and turns the RT checkbox off -> the empty-mask trigger write (+3394).
7. `refreshProfileTrigger` (`wmIndex.js:4758-4767`) **writes the mirror back** from memory. If the tab is closed between the delete (+1811) and this save (about +3394), the mirror, including profile names, is gone.
8. Re-reads `21/05` x 82 again.

**The mirror's per-profile mapping is unreliable in this mode** (CODE-ONLY): `curProfile` is chosen from the stored `curIndex` at page load (`wmIndex.js:5855`) and the `0x11` handler never re-points it. Keyboard data therefore keeps landing in the slot loaded first, while the JSON records `curIndex = PP`. `sideLight` and `is6Key` are never refreshed from the keyboard. `curProfile.macros` and `.advKeys` keep any storage fallback set at connect.

**Connect-time data sources** (`wmIndex.js:6107-6132, 6387-6410`): main light, triggers, dead band, SOCD come from the keyboard; keymaps and macro bindings come from the keyboard if any non-default entry exists, **otherwise from storage** (the W75's tables are all zero, so the fallback is taken, then erased again by the `0x11` handler); macros and advanced keys from the keyboard if non-empty, else storage. Profile names, the selected tab and the colour memory come from storage.

### 6.6 Software-profile mode: the full-profile push (CODE-ONLY; not used on W75 V3.22.07)

The only code path that writes a complete profile (`wmIndex.js:1024-1105`), run on every profile-tab click when `presetNumber` = -1:

| Order | Function | Packets | Value pushed when storage was cleared |
|---|---|---|---|
| 1 | `setMacroValue` (started without await, interleaves with the rest) | `19 0n` x 50 | all 10 slots zero |
| 2 | `setKeyValue` | `18 00` x 10 | zeros (plus Fn/knob/M1-M4 layout defaults) |
| 3 | `setFnKeyValue` | `18 02` x 10 | zeros (plus knob defaults) |
| 4 | `setAdvancedKeys` | `22 nn` x 40 | type 0 in every slot |
| 5 | `setLightValue` main | `07 00` | `02 04 03 ff 00 00 00 00 00 00 01 00` (Wave, red) |
| 6 | `setLightValue` side | `08 00` | same |
| 7 | `setAllTriggerValue` | one `21` per distinct setting | all keys: mode 0, travel 85 (1.70 mm), sensitivity 5/5 (0.10 mm) |
| 8 | `setDeadBand` (only if `isInitDeadBand` is false) | `26 01` x 5 | 0/0 |
| 9 | `setPrcsPower`, `setPrcsData` | `24 03`, `24 00` x 2 | off, empty |
| 10 | `setPerformanceValue` | `14 00 .. 00` | `other` = 1, 6-key 0 (always 0 after a reload) |
| 11 | `closeTriggerTest` (if the test is on) | `21 .. 18 03` | - |

**Visible signature of such a push after clearing storage:** the main light turns **red Wave** (the W75 read white Wave in c01), RT sensitivity becomes 0.10 mm on every key (device currently 0.02 mm), remaps/macros/advanced keys/SOCD are cleared, dead band 0.

### 6.7 Every device write the site can make, and its data source

| UI action | Packets | Data source | Mode |
|---|---|---|---|
| Connect / reload | `21 00 00 00 18 00` + zeros (no-op) | - | onboard only |
| Click a profile tab | `21 .. 00 12 PP` | - | onboard |
| Click a profile tab | full push (6.6) | mirror or defaults | software |
| Rename a profile | none | - | both |
| Keymap Apply (also after binding a macro, saving/deleting an advanced key, confirming any key assignment, resetting a key; an RS save can start it twice, un-awaited) | `19 00-09` (50), `18 00` (10), `18 02` (10), `22 00-27` (40) | in-memory model | both |
| Macro dialog OK | none (storage only) | - | both |
| Lighting mode / colour / brightness / speed | `17 .. 00 00` on mode select, `07 00` / `08 00`, `09 00` / `09 01` whole table | UI, seeded from the keyboard | both |
| Trigger sliders / RT toggles | `21 .. 18 <mode>` for the selected keys | UI | both |
| Travel test on / off | `21 .. 18 02 <mask>` / `21 .. 18 03` | - | both |
| Calibration start / stop | `21 .. 18 0F 00` / `10 00` (W75) | - | both |
| Reset Trigger | `21 .. 18 06` (subject to the toggle), re-read | - | both |
| Dead-band sliders | `26 01` x 5 (whole table) | keyboard data | both |
| SOCD toggle / Apply | `24 03`, `24 00` x 2 | keyboard data | both |
| Other-tab checkboxes | `14 00 .. 00` | UI | both |
| Switch type | `25 01` x 3 (whole table) | UI over keyboard data | both |
| Poll rate | `21 .. 00 09 01 RR` | UI | both |
| Reset (current profile) | section 4.6.3 | - | both |
| Gamepad radio | `14 03 .. 01 MM` | - | both |
| Clear Cache | none | - | both |

### 6.8 Data that exists only in browser storage

| Data | Why it is not on the keyboard |
|---|---|
| Profile names | the rename handler only writes storage; no packet carries text (`wmIndex.js:1109-1148`) |
| Macro names | the slot header has no name field; read-back names are generated |
| Macros not bound to any key | only bound macros (max 10) are written; unbound ones vanish from the UI on reconnect |
| Macro edits not yet applied | the macro dialog's OK only writes storage; the keyboard is written on Apply |
| Per-mode colour memory (`colorArray1`-`4`) | the keyboard stores only the active mode's colours |
| Advanced-key and SOCD names | generated by the site |
| UI language and theme | interface preferences |

Everything else is read from the keyboard: keymaps, bound macros, advanced keys, lighting (including per-key tables), triggers, dead band, switch types, SOCD, poll rate, Win-Lock/6-key flags, active profile. All CODE-ONLY.

### 6.9 What our app must store locally

| Item | Reason | Format suggestion (engineering) |
|---|---|---|
| Profile names (per onboard profile 1-4, per device serial/product) | not on the keyboard | app config file |
| Macro library: names, all macros including unbound and unapplied ones, step lists in our own model | the keyboard keeps only bound macros, without names | app config file |
| Per-mode colour memory (optional) | convenience only | app config file |
| Optional names for advanced keys and SOCD pairs | convenience only | app config file |
| **Raw backups** of every table per onboard profile (`07`, `08`, `09 80/81`, `18 80/82`, `19 80-89`, `21/04`, `21/05` x 82, `21/09`, `22 80`, `24 01/02`, `25 02`, `26 00`, status) | the keyboard has no export; needed before any DANGEROUS command and for restore | timestamped files, raw bytes plus decoded view |
| UI preferences | not device data | app config |

**Do not** store a "desired state" that is pushed to the keyboard automatically on connect or profile switch. The keyboard is the source of truth; local files are names, libraries and backups.

### 6.10 Implementation rules derived from the site analysis

1. Detect onboard profiles with `21/11` (allow about 150 ms). Switch with `21/12`, then confirm with `21/11` and **re-read every table**: `01`, `07 01`, `08 01`, `09 80`, `09 81`, `18 80`, `18 82`, `19 80`-`89`, `21/04`, `21/05` x 82, `21/09`, `22 80`, `24 01`, `24 02`, `25 02`, `26 00`. Never write anything as a side effect of switching or connecting.
2. Read-modify-write every table from the raw bytes; preserve entries we do not understand and positions outside our layout.
3. Macro slots before keymap entries that reference them.
4. Do not copy: the empty-mask write, the auto-reset on input `0x14` byte 5 = 1, the `resetTrigger` toggle, the knob default overwrite, the colour-slot zeroing, the `0x09` flood.

### 6.11 Profile import/export

None in v1 (CODE-ONLY): the only `JSON.parse` is the mirror loader; the only downloads are the music-rhythm plugin and the firmware tool. There is no site file format to stay compatible with.

---

## 7. Open questions and capture plan

### 7.1 Open questions

Ordered by value for our application. "Resolved by" lists the capture-plan items of section 7.2; "app test" means it needs a deliberate test with our own tool later, not a site capture.

| # | Question | Resolved by |
|---|---|---|
| Q1 | **Mostly resolved.** C2: triggers (`0x21`, incl. RT bytes 30-31) and `0x07` persist across reconnect and USB power loss. OpenWraith H8 (2026-10-27): after our base keymap write (`18 00` + the `18 02` commit trigger, section 4.8.2) a USB power cycle left all four profiles byte-identical to the backup taken before it, so the keymap pair persists across power loss too. Dead band and Fn keymap persist across a profile switch (section 4.12, 4.8.2). Macro slots, advanced keys and non-zero dead band across a power cycle remain untested. **Earlier partial answer:** triggers and `0x07` persist across reconnect and USB power loss. Still open: do the remaining writes persist across a USB power cycle: per-key colours (`0x09`, shown only across a page reload), keymap, macros, dead band, SOCD, switch types? Are trigger bytes beyond the declared length (29-35) applied? | C2, then reconnects in C7, C13, C14 |
| Q2 | **Partly resolved (c04): `21/12` is acked with `21 .. 02 12 01` and followed by a `21 .. 03 11 PP 00` push (section 4.5.3).** Still open: does the keyboard push a `0x11` report when the profile is changed on the keyboard (Fn + 1..4)? | C3, C6 |
| Q3 | **Partly resolved (c04, c10): triggers (`21/05`), the main light (`07`), the base keymap (`18 80`) and the macro slots (`19 8n`) are per profile (section 4.5.4).** Still open: which of the others are per onboard profile and which are global (`08`, `09` tables, Fn keymap, advanced keys, SOCD, dead band, switch types, poll rate, Win-Lock/6-key, gamepad)? | C5, app test (an all-profile backup shows which tables differ between profiles) |
| Q4 | What does `14 00 00 00 01 01` reset (current profile or all)? Does it include triggers and dead band? What is its reply, and does that reply carry byte 5 = 1 (which would make the site loop)? | C23 |
| Q5 | What do replies to other `0x14` writes look like (Win-Lock, 6-key, gamepad)? Does `14 03 .. 01 01` (Xbox) get echoed with byte 5 = 1, colliding with the site's reset trigger? | C9, C22 |
| Q6 | What does the layout file `W75HEARGB.json` contain (names, codes, indices, Fn code1, knob regions, layout type)? Are the inferred legends in section 5 right? | C1, C12 |
| Q7 | Knob: which keymap slots do the knob regions use; what do slots 19-21 do; does the site's forced overwrite of 16-21 change knob behaviour; does index 15 have any Hall-effect meaning? | C1, C7, C16 |
| Q8 | Is the colour-table slot equal to the key index for every key? Which LEDs, if any, do slots 103 and 126 drive? | C11 |
| Q9 | Key-mask orientation for `0x21` writes (byte = column, bit = row) on an off-diagonal key; exact Select-All mask (knob excluded?) | C8 |
| Q10 | Meaning of status byte 19 (`01`), byte 7 of the `0x11` reply (`01`), flags A bit 6; does status byte 20 follow the keyboard's Win-Lock toggle? | C3, C6, C9 |
| Q11 | Are onboard profiles numbered 1-4 only, or is 0 a hidden default profile? | app test (not offered by the site) |
| Q12 | Does the keyboard acknowledge `0x18` writes? Can a single chunk be written alone? Which encoding does an unmapped key / the Fn key need (`00 00 00 00` vs `01 FA 00 00`)? Does sub `01`/`81` exist? | C7 (sub `01`/`81`: app test) |
| Q13 | Macro details: mouse button codes (writer `01/02/04` vs reader `00/01/02`), delay placement, count field in modes 0/2/3, use of macro ID and copy flag, empty slot `00` vs `FF`, step limit (62 vs 20 groups), media steps | C14 |
| Q14 | Does a non-zero Fn-layer entry override a built-in Fn function? Is multimedia index `24` Browser Forward or Back? | C15 |
| Q15 | Trigger details: meaning of the second travel value (read bytes 8/12, write 29/33) and whether it may differ; `21/04` byte 8; modes other than `00/0C/0D`; meaning of the ack byte; does the firmware clamp or reject out-of-range values? | app test (the site always writes equal values and stays in range) |
| Q16 | Does mode `0D` (separate press/release) work as the code suggests? | C8 |
| Q17 | Live travel stream: rate, bytes 6/7 (row/col?), acks for open/close | C12 |
| Q18 | Which status reports does the W75 send during any-key calibration (`0F`/`10`); is there a reply to stop? | C20 |
| Q19 | DKS units (0.1 mm) and step semantics; advanced-key write acks; MT/TGL encodings | C16 |
| Q20 | SOCD reply bytes p[1..3]; can the hotkey (Fn + R-Shift) be changed? | C17 |
| Q21 | Is there a real dead-band reset command (the site sends the read request)? Is byte order top/bottom right? | C13, C23 |
| Q22 | Does the switch-type ID change firmware behaviour? | C18 (low value: W75 reports only HM1) |
| Q23 | Lighting semantics: speed scale direction, Full Color effect (also in Static/Custom where it is hidden), brightness 0 vs power byte, `A5 5A`, order of max speed / max brightness, record bytes 17-18 | C10 (max speed/brightness order: not resolvable on W75, both 4) |
| Q24 | How does the keyboard choose between Custom1 and Custom2? Is the `09 20` reply really the active slot? What does a `09 01` ack look like? | C11 |
| Q25 | Were colour writes lost during the site's flood (1681 acks for 2064 chunks)? Flash wear? | app test (throttled writes with ack checking) |
| Q26 | Is the `17 .. 00 00` leave packet needed on the W75? Is the unsolicited `0x01` after `0x17` a status push? | app test |
| Q27 | Are modes `05`, `0D`, `64` or other unlisted modes valid? Does writing `0x08` do anything on the W75? | app test (low value, avoid) |
| Q28 | Do poll-rate and gamepad writes re-enumerate the device; which PID per gamepad mode; what is `0xC365`? | C21, C22 |
| Q29 | Does the heartbeat have any firmware-side effect? Is status byte 1 (link token) ever non-zero on the wired W75? | (user test done for 90 s; low value) |
| Q30 | Meaning of `0x0D` fields `W669`, `34`, `KB`, `ML` | not resolvable by captures |
| Q31 | Did older W75 firmware lack `21/11` (which would put the site in software-profile mode and explain earlier "resets")? | not resolvable on this keyboard |
| Q32 | Why does `21/11` take 42 ms (flash read?); must other commands wait for it? | app test |

**Top 5:** Q1 (persistence of non-lighting writes), Q2 (profile select behaviour), Q3 (per-profile scope), Q4 + Q5 (reset scope and `0x14` reply format, including the site's self-reset hazard), Q6 + Q8 (authoritative layout and LED mapping).

### 7.2 Capture plan

Instructions for the maintainer. Use the existing capture hook with logging on, add a **MARK before every step**, wait about 5 s between steps, and note anything visible on the keyboard. Items are deduplicated and ordered by value; risky items are last. Unless stated otherwise, restore the original setting at the end of the item.

| # | Actions in the site (or on the keyboard) | Expected / what to look for | Resolves | Risk |
|---|---|---|---|---|
| C1 | **DONE 2026-10-24** (key table confirmed, section 5). **No keyboard needed.** In the browser open `https://wraith.software/config/keys/W75HEARGB.json?t=v1` and save it **outside the repo** for reference only (do not commit it). | Per-key index, name, code, code1, hidCode, geometry, knob regions; check against section 5 | Q6, Q7, Q12 (Fn encoding) | none |
| C2 | **DONE 2026-10-24** (c05: page reload + reconnect; c06: USB unplugged 10 s, replugged, reconnected. Both times W replied `05 0c 32 32 0f 0f 00 00 00 00 02 02`, main light `0a 04 03 ff 00 00`, active profile 1; the connect-time empty-mask `21 .. 18 00` write changed nothing. Triggers and main lighting persist across reconnect and power loss: CONFIRMED). **Power-cycle read-back.** Close the site, unplug the keyboard, wait 10 s, plug in, open the site, connect, change nothing, wait 10 s. | `21/05` reply for row 2 col 2 (W) = `05 0c 32 32 0f 0f 00 00 00 00 02 02`; `09 80` = 81 red slots; `07 01` = Custom red | Q1 (triggers, colours, bytes 29-35) | none |
| C3 | **Profile switch from the site.** Stay on the Keymap tab with no keys selected. MARK `BEFORE_PROFILE2`, click Profile 2, wait 5 s; MARK `BEFORE_PROFILE1`, click Profile 1, wait 5 s. | OUT `21 00 00 00 00 12 02` / `12 01`; any IN `21 .. 11 PP`; the re-read that follows; **no** `07`/`18`/`19`/`22`/`26` writes; status byte 19 | Q2, Q10 | low |
| C4 | **Clear Cache round trip.** Settings (gear) -> Clear Cache; let the page reload and reconnect by itself; MARK; click Profile 2, then Profile 1. Then click the pencil next to Profile 3, type a name, press Enter. Check the keyboard's lighting and actuation are unchanged. | Only writes: the empty-mask `21 .. 18 00` and `21 .. 12 PP`; rename produces no OUT packet | confirms section 6.1 conclusion | low |
| C5 | **Per-profile scope.** Switch to Profile 2. Change: lighting mode to Breath; W actuation to 2.00 mm; W dead band top 0.10 mm; one Win-Lock checkbox; SOCD on. Switch to Profile 1, then back to Profile 2 (MARK each). Then undo the Profile 2 changes. | Which reads differ between profiles after each switch | Q3 | low |
| C6 | **Keyboard-side switches** with the site open and idle: MARK, press Fn+2, wait 5 s, press Fn+1. MARK, press Fn+L-Win (Win-Lock on), wait 5 s, press it again. | Unsolicited `21 .. 11 PP`; changes in status bytes 19/20 | Q2, Q10 | low |
| C7 | **Keymap write.** Keymap tab, base layer: select W, Remap -> A, Apply. Reload the page and reconnect. Select W, Reset, Apply. Reload again. | Full Apply burst (`19` x 50, `18 00` x 10, `18 02` x 10, `22` x 40), any acks, entry `00 04 00 00` at offset 184, what is written at 121 and 15-21; read-back after reload | Q1, Q7, Q12 | medium (rewrites all keymap/macro/advanced tables) |
| C8 | **Trigger mask and modes.** Actuation > Travel: (a) select only Q, set 1.50 mm; (b) select only W, enable Rapid Trigger and "Separate press/release sensitivity", Press 0.30 mm, Release 0.50 mm; (c) Select All, press "+" once, then "-" once. Restore Q and W afterwards. | (a) mask byte 7 = `0x04` (byte = column); (b) mode `0D`, bytes 30/31 = `0f 19`; (c) one packet with mask `3f 26 3f 1f 1f 1f 3f 1f 1f 1f 3f 3f 2f 31 3f 2e` | Q9, Q16 | low |
| C9 | **Other tab.** Tick and untick each Win-Lock option one at a time (Disable Windows key, Shift+Tab, Alt+Tab, Alt+F4); switch N Key -> 6 Key -> N Key. Then reload and reconnect. | `14 00` frames and the **exact reply bytes** (byte 5!); status bytes 20/22 after reload | Q5, Q10 | low |
| C10 | **Lighting parameters.** Static, pure blue, toggle Full Color on and off; Breath, speed 0 then 4 (say which is faster); Wave, click each of the six direction buttons; Power off, wait 3 s, on; Brightness 0 (say whether LEDs go fully dark), then back to 4. | `07 00` records; direction values 1, 0, 2, 3, 5, 4; power byte 16 = `01` then `00` + `07 01` re-read | Q23 | low |
| C11 | **Custom tables and LED mapping.** Lighting > Custom: (a) click Custom2 (expect 8 x `09 01` + acks), reload, check the `09 20` reply; (b) Custom1: Deselect All, select only Up, pick blue, note which LED lights; repeat for Left, Down, Right (MARK each); (c) click Custom1 / Custom2 back and forth and note which table the keyboard shows. | `09 01` ack bytes; `09 20` byte 1; written slots 102/123/124/125 vs lit LEDs | Q8, Q24 | low (repaints Custom1) |
| C12 | **Travel test.** Actuation > Travel: Travel Test on. MARK and press, one at a time: Esc, F1, Del, the knob (press), Grave, Backspace, Home, Tab, Backslash, PgUp, Caps, Enter, PgDn, L-Shift, R-Shift, Up, L-Ctrl, Space, R-Alt, Fn, R-Ctrl, Left, Down, Right, W. Travel Test off. Then click the Calibration sub-tab once and leave it. | `21 .. 18 02 <82-key mask>`, `21` byte 5 = `01` stream (bytes 6/7 = row/col?), `21 .. 18 03`; another `18 03` on the Calibration tab | Q6, Q17 | none |
| C13 | **Dead band.** Actuation > Dead Band: select W, Top 0.10 mm, Bottom 0.20 mm. Reload and reconnect. Then set both back to 0. | `26 01` x 5 with chunk 1 bytes 39/40 = `05 0a`; read-back | Q1, Q21 | low |
| C14 | **Macros.** Keymap > Macro: (a) create a macro "A down, 100 ms, A up, 200 ms, left click down/up, middle click down/up, right click down/up" and save it **without** assigning (expect no `19` write); (b) assign it to Caps with "Operate 3 times", Apply, reload and reconnect; (c) assign the same macro to an Fn-layer key, Apply; (d) on the Caps binding select each of the four play modes, Apply after each; (e) delete the macro, Apply. | Slot layout, mouse codes, delays, count, copy flag (bit 7), mode byte, zeroed slots, keymap `10 39 SS 00` | Q1, Q13 | medium (rewrites macro/keymap/advanced tables) |
| C15 | **Fn layer and special codes.** Fn layer: Fn+W -> Multimedia Mute, Fn+E -> Brightness+, Fn+F4 -> Calculator; base layer: Caps -> Ctrl+Shift+A, one key -> Mouse Wheel Down, another -> Mouse Back, another -> Browser Forward. Apply. Physically test whether Fn+F4 now opens the calculator instead of muting. Undo afterwards. | Entries `07 12`, `03 00`, `07 32`, `13 E0 E1 04`, `12 01 FF 00`, `12 00 10 00`, `07 24` or `07 25` | Q14 | medium |
| C16 | **Advanced keys and knob.** Keymap > Advanced: a DKS on one key with 4 actions and mixed tap/hold bars; an MT (tap A, hold L-Ctrl, 300 ms); a TGL; Apply. If the knob shows clickable regions, remap "rotate right" to PgDn and Apply. Delete all afterwards. | `22` slot writes, `18` entries `17 SS SS 00`, knob-region slot index | Q7, Q19 | medium |
| C17 | **SOCD.** Create pair A/D with Model 1, Apply; toggle the SOCD switch on and off. Delete the pair afterwards. | `24 03`, `24 00` x 2, entry format | Q20 | low |
| C18 | **Switch type.** Actuation > Switch: select W, click the HM1 button. | `25 01` x 3 | Q22 | low |
| C19 | **Reset Trigger.** Actuation > Travel: click "Reset Trigger" and confirm. Re-apply W 1.00 mm / RT 0.30 mm afterwards. (Reload the page first: any earlier `0x14` reply in the same page session, e.g. from C9, makes the site silently skip the next trigger reset; if no `21 .. 18 06` appears, click it again.) | `21 .. 18 06` and the re-read; the firmware's default trigger values | Q15 (defaults) | medium (all triggers reset) |
| C20 | **Calibration (optional).** Only if you accept recalibrating: Actuation > Calibration, Start, press W and A fully, wait for the colour change, Stop. | `21 .. 18 0F 00`, status reports, `21 .. 18 10 00` | Q18 | **high** (calibration state) |
| C21 | **Polling rate (optional; the keyboard restarts).** 8 kHz -> 1 kHz (accept the prompt), wait for reconnect, -> 8 kHz. | `21 .. 00 09 01 01` / `.. 08`, CLOSE/OPEN, PID | Q28 | medium |
| C22 | **Gamepad (optional; the keyboard restarts).** Only after C9 and a full backup (C2 capture), on a profile you can afford to lose: Keymap > GamePad -> Xbox (accept), note the PID in the new OPEN entry, then back to Disabled. If the site starts a reset by itself (look for `14 00 00 00 01 01`), stop and report. | `14 03 .. 01 01`, its reply (byte 5?), the new PID | Q5, Q28 | **high** (restart; possible self-reset by the site) |
| C23 | **Reset current profile (DESTRUCTIVE, last).** Only after a full backup capture of every profile: switch to a profile you can afford to lose, click Reset and confirm. Then switch to the other profiles and check they are untouched. | `14 00 00 00 01 01` reply bytes; whether `21 .. 18 06`, `26 00`, `24 03 .. 00` follow; which tables changed | Q4, Q21 | **high** |

---

### 7.3 Capture session 2 (2026-10-24 evening) – results

Captures `c08`-`c13` (`captures/capture_08_profile_switch.json` … `capture_13_macro_ab_on_caps.json`). Plan items done: C3, C7, C8 (a, b; the Select-All part was skipped), C13, C14 (create, bind to Caps, typing test, unbind), C19. All confirmations are already reflected in the master table (section 3).

- **Profiles (C3):** `21/12` select is acked and followed by an unsolicited `11 PP` push; the site re-reads everything and writes nothing. **Main light and per-key triggers are per onboard profile** (profile 2 kept factory triggers and its own light while profile 1 had Q 1.50 mm and W RT 0D; the user saw the light change). **The base keymap and the macro slots are per profile too** (OBSERVED, c10: profile 2 read an all-zero base keymap and all-`FF` macro slots while profile 1 read the site's c09 knob defaults and all-`00` slots). Q3 is answered for these four; Fn keymap, advanced keys, dead band, SOCD, colour tables and switch types are still unverified for scope. **Update (OpenWraith all-profile backup, 2026-10-24):** profile 1 also differs from profiles 2-4 in **custom colour table 1** (`09 80`), so it is per profile too (OBSERVED); profiles 2-4 are identical to each other. Dead band, Fn keymap, advanced keys, SOCD, switch types, side light, custom table 2, poll rate and Win-Lock read the same in all four profiles, but they hold factory values everywhere, so their scope is still unknown.
- **Key mask orientation (C8, Q9): CONFIRMED** byte = 6 + column, bit = 1 << row (Q at row 2, col 1 -> byte 7 = `0x04`).
- **Separate press/release (C8, Q16): CONFIRMED** mode `0D`, byte 30 press, byte 31 release.
- **Reset Trigger (C19): CONFIRMED**, see row 34; only the active profile was affected as far as the captures show (profile 2 was already at factory values, so this is not proof).
- **Keymap / macros / advanced keys (C7, C14):** every Apply rewrites 10 macro slots (5 chunks each), both keymap layers (10 chunks each) and 40 advanced-key slots: 110 packets, every packet acked with a zero-length report of the same cmd. The site also forces its knob defaults into the base keymap (slots 16-21: `07 12`, `07 11`, `07 10`, `02 02`, `03 01`, `03 00`) and `01 00 00 00` at Fn (121) although the device had all zeros before; these now read back from the device. **The lossy whole-table rewrite (section 6.1 row c) is CONFIRMED.**
- **Dead band (C13): CONFIRMED** layout and units; the site rewrites the whole 264-byte table on every slider step (37 full writes in about 20 s).
- **Acknowledgements:** every write of the session was acked (section 2.7); acks carry no chunk index, and `18`, `19` and `22` acks do not echo byte 1.
- **Implications for our writer:** read the table from the device first, change only the target entries, write it back once (debounced), and verify by reading it back; never synthesise defaults for entries we did not change.
- **Never captured** (keep these CODE-ONLY until a capture or a hardware test shows them): a trigger mask with more than one key; a read-back of a non-empty macro slot; a non-zero dead-band read; Fn-layer writes with non-zero content; non-empty advanced-key writes; persistence of keymap, macros and dead band across a power cycle (Q1).

## Appendix A. Corrections to the earlier draft

| Earlier draft said | Corrected |
|---|---|
| Header byte 2 "always `0x00`" | Bytes 2-3 are a 16-bit big-endian chunk index in the site code; byte 3 confirmed as chunk index; byte 2 was 0 in all captures |
| Byte 4 = payload length | True for replies; several requests declare less than they carry (section 2.4) |
| Responses echo the header | Except `07 01` -> `07 00`, `08 01` -> `08 00`, `09 20` -> `09 SS` |
| `0x01` contains the active profile? | Capabilities/status report (section 4.2); the active profile is `21/11`. Byte 19 is unparsed (GUESS) |
| `0x0E` unknown config | Fn-layer non-remappable key mask; W75: index 121 |
| `0x02` `a5 5a 04 04` | Side-light mode list: no modes, trailer `A5 5A`, max speed 4, max brightness 4 |
| `0x0A` list of supported modes? | Yes: mode IDs, `A5 5A`, max speed, max brightness |
| `0x07` layout `mode, brightness, speed, R, G, B, ?, ?, ?, ?, 01` | 12-byte record: mode, brightness, speed, fg RGB, bg RGB, direction, fullColor, power (0 = on) |
| "Static red" `0a 04 03 ff 00 00 ...` | Mode `0x0A` is **Custom** (per-key table); Static is `0x00` |
| `0x09 00` per-key live preview | Custom1 table write (Custom2 = `09 01`), persistent; streamed during colour drags |
| `0x09 20` 396-byte table | Active custom slot query, len-0 reply |
| `0x17 00` "begin edit", `02` | Music rhythm leave; `02` is the length; payload `00 00` |
| `0x18` 1056 B | 528 B per table (the site reads each table twice per connect) |
| `0x19` 10 x 512 B | 256 B per slot |
| `0x21 00` read "18 05 row col" | `0x21` family keyed by byte 5; `18` is byte 4 (constant); `05` per-key read, `04` global |
| `0x22 80` 3200 B all `ff` | Advanced keys, 1600 B per read, all `00` |
| `0x24 01` / `02` unknown, 160 B | SOCD pairs (80 B per read) / SOCD power + hotkey |
| `0x25 02` per-key flag table? | Per-key switch type (`01` = HM1) |
| `0x26` 528 B unknown | Dead band, 264 B per read |
| The site renders the UI from the localStorage mirror | Not on the W75: onboard-profile mode; the mirror is deleted and rebuilt on every connect |
| Reset caused by pushing defaults after clearing browser data | Refuted for V3.22.07 in onboard mode; describes software-profile mode only (section 6.1) |
| `dir` in OPEN, OUT, IN, MARK | Also `CLOSE`; MARK entries carry `label` |

## Appendix B. Editorial notes: contradictions between analyses and how they were resolved

The subsystem analyses disagreed on a few points. Each was settled by re-reading the site code or the captures.

| Topic | Disagreement | Resolution | Evidence |
|---|---|---|---|
| Key-mask orientation | One analysis called byte = column, bit = row CONFIRMED by the W write | CODE-ONLY orientation, capture-consistent: W is on the diagonal (row 2 = col 2); the `0x0E` reply (byte 11) independently requires byte = column | `agreement.js:300-310, 1809`; c01 +917; c03 +34733 |
| Does the W75 layout have a rotary knob? | Keymap analysis: "most likely none"; layout analysis: knob at index 15 | Knob present (INFERRED, strong): the screenshot shows the round panel drawn only for a `RotaryKnob` entry, and index 15 is the only read position Select All never includes | `wmIndex.js:6174-6190`; screenshot; c02 painted slots |
| Does a colour write include the knob slot? | "Knob never written" vs "every layout key written" | Every layout key's stored colour is written, the knob's included (it was black); only non-layout slots are forced to 0 | `agreement.js:1685-1700`; c02 |
| `0x09` ack count | 1682 vs 1681 | 1682 length-0 `0x09` replies in c02, one of which answers the connect-time `09 20`: 1681 write acks for 2064 chunks | `captures` recount |
| Trigger writes in c03 | 15 vs 16 | 16 `0x21` writes = 1 empty-mask connect write + 15 W writes; 16 acks | recount |
| `isInitDeadBand` | "always false, dead band always sent" vs "can stick true" | Normally false (cleared after each `readDeadBand`), but stays true after `resetDeadBand` or a late page-4 reply until the next `readDeadBand` | `agreement.js:878-895, 2180-2197, 2235-2244` |
| Trigger reset skipped in the full reset | "skipped when the reset's 0x14 reply comes first" vs "skipped after an earlier 0x14 reply" | The flag is checked right after the reset is sent, so an **earlier** `0x14` reply is what normally causes the skip; the reset's own reply most likely arrives later (GUESS on timing) | `agreement.js:320-321, 940-954`; `wmIndex.js:1331-1336` |
| `0x0D` field numbering | "field 4 / 5" vs "field 5 / 6" | 0-based: field 4 = product, field 5 = version | `wmIndex.js:6623-6637` |
| Win-Lock bit mapping | write bytes vs read bits | write bytes 12-15 = bits 0-3 = Win, Shift+Tab, Alt+F4, Alt+Tab (checkbox 4 is Alt+F4, checkbox 3 is Alt+Tab) | `agreement.js:1728-1746`; `wmIndex.js:1709-1732, 4187-4199`; `v1.html:1035-1063` |
