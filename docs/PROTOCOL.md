# Wraith W75 HID Protocol Specification

Reverse engineered for interoperability from the official browser configurator (https://wraith.software/, WebHID) and from USB captures of the maintainer's own keyboard. This document describes behaviour in our own words. It contains no code from the site.

| Item | Value |
|---|---|
| Device under test | Wraith W75, PID `0xC368`, firmware `V3.22.07` (build `Aug  6 2026,13:46:54`) |
| Site build analysed | v1 UI (`v1.html` + `wmIndex.js` + `agreement.js`); wraith.software always serves v1 |
| Spec revision | 2026-10-27 (capture session 3, section 7.4; the 2026-10-24 revision replaced the earlier draft, whose corrections are listed in Appendix A) |
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
  - `c04`-`c13`: `c04` section 4.5.3; `c05`, `c06` section 7.2 (C2); `c07` section 6.1 (row d); `c08`-`c13` section 7.3 (capture session 2).
  - Capture session 3 (2026-10-27, one page session on profile 1, section 7.4):
    - `c14` = `capture_14_travel_test.json` (connect, then the travel test after MARK `BEFORE_C12_TRAVEL_TEST` at +43273; MARK `AFTER_C12_TRAVEL_TEST` +138233, `BEFORE_C17_SOCD` +159834)
    - `c15` = `capture_15_socd.json` (SOCD pair, switch and the keyboard's Fn + R-Shift toggle; MARK `AFTER_C17_SOCD` +133899, `BEFORE_C16_ADVANCED` +155987)
    - `c16` = `capture_16_advanced_keys.json` (advanced keys DKS, MT, TGL, a profile 1 -> 2 -> 1 round trip; ends with MARK `BEFORE_C15_SPECIAL_CODES` +590418)
    - `c17` = `capture_17_keymap_codes_led_map.json` (special keymap codes on the Fn layer, Custom1 LEDs of Up and Right; MARK `END_SESSION_3` +437175)
    - c15, c16 and c17 continue c14's page session and have **no OPEN entry**: each starts with the MARK that ends the previous file (c15 #0 = c14 #1387, c16 #0 = c15 #376, c17 #0 = c16 #4545).
- **Times** are milliseconds after the capture's `OPEN` entry (written `+1811` or `t=1811`). OPEN timestamps: c01/c02 `1792809086131`, c03 `1792809574699`, c14 `1793103756673`. For c15, c16 and c17 times are milliseconds after the file's entry #0 (t = `1793103916507`, `1793104072494`, `1793104662912`), unless written "c14-OPEN +". `capN#i` is entry index i.
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
   - the empty-mask trigger write the site sends on every connect and after every `0x11` profile report (`21 00 00 00 18 00` + zeros; section 4.11.4);
   - running a reset when an input report `0x14` with byte 5 = `01` arrives (section 4.6.3);
   - the `resetTrigger` toggle that silently skips every other trigger reset (section 4.11.6);
   - forcing knob default codes into keymap slots 16-21 on every keymap write (section 4.10);
   - zeroing colour slots that are not in our key layout (section 4.7.6).
5. Serialise requests: one outstanding request at a time, never interleave two multi-chunk writes (section 2.8).

**Command risk table**

| Command | Effect | Level | Confidence of effect |
|---|---|---|---|
| `14 00 00 00 01 01` | Reset current profile (scope not verified) | **DANGEROUS** | Bytes CONFIRMED (the site sent them in c07 #991, answering the keyboard's reset notice #990); what this write itself resets is not verified (Q4) |
| `21 00 00 00 18 06` | Reset all key triggers of the active profile | **DANGEROUS** | CONFIRMED (c11) |
| `21 00 00 00 18 0F 00` / `10 00`, `08 00` / `08 01` | Start/stop calibration. An interrupted or careless calibration can leave keys misbehaving until recalibrated (GUESS) | **DANGEROUS** | CODE-ONLY |
| `21 00 00 00 18 07 <row> <col>` | Single-key calibration. Dead code in the site, never exercised | **DANGEROUS** (do not implement) | CODE-ONLY |
| `14 03 00 00 01 MM` | Gamepad mode. The UI says the device restarts; PID probably changes | **DANGEROUS** | CODE-ONLY |
| `21 00 00 00 00 09 01 RR` | Poll rate. The UI says the device restarts | CAUTION | CODE-ONLY |
| `14 00 00 00 01 00 ...` | Win-Lock/6-key options. **Byte 5 must stay `00`**; `01` turns it into the reset above | CAUTION | CODE-ONLY |
| `18 00` / `18 02`, `19 0n`, `22 nn`, `26 01`, `25 01`, `24 00`, `09 00` / `09 01` | Whole-table writes | CAUTION: read-modify-write and back up first | CONFIRMED for `09 00`, `18 00` / `18 02`, `19 0n`, `26 01` and `22 nn` with zero content (capture session 2); since session 3 also for `18 02` with real content (c17), non-empty `22 nn` DKS/MT/TGL slots (c16, c17) and `24 00` (c15); CODE-ONLY for `25 01` and `09 01` |
| Input `14 ... [5]=01` | The site treats it as "reset everything" and sends a reset | Hazard on the **receive** side: ignore it | CONFIRMED (c07: the keyboard sent it after Fn + Space was held (#990) and the site answered with its own reset 1 ms later (#991)) |

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
| 5..62 | payload | Bytes after `5+len` are padding. The device leaves **stale buffer data** there (`09 80` chunk 7, `25 02` chunk 2), so always honour byte 4. **One exception:** the long variant of the live travel stream (`21 .. 03 01`, section 4.11.7) declares length 3 but carries live data up to byte 36; decode that report without truncating at byte 4. | CONFIRMED / OBSERVED (stale data; travel-stream exception, c14) |

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
- **Latency** (c01, c03; later captures where noted):
  - Single-packet reads: 0-1 ms median, up to 9 ms (c01). CONFIRMED.
  - A request can wait behind another command's multi-chunk stream: c03 heartbeat at +2723 waited 19 ms behind `18 80` chunks; c02 heartbeat at +163749 took 111 ms during the `0x09` flood. One c02 heartbeat (+165764) got **no reply at all**. OBSERVED.
  - Per-key trigger writes are acked after 4-7 ms (c03). OBSERVED.
  - `21`/`11` answers after 39-65 ms over six independent connects (c01 42, c03 43, c05 39, c06 45, c08 65, c14 59; c02, c04 and c07 duplicate c01, c03 and c06). OBSERVED. Cause unknown. The about 150 ms allowance of section 2.8 still has margin.
  - While the travel test is open (c14 +85986..+123439) the 38 heartbeats were all answered within 0-1 ms; stream reports interleave with replies and never replace them (158/158 heartbeats answered in c14). OBSERVED.
- **Multi-chunk replies:** one request makes the device stream every chunk back to back, 0-3 ms apart. Each chunk carries its own index (byte 3) and length (byte 4); the last chunk is shorter. CONFIRMED.
- **Pipelining:** the site sometimes sends a second request before the first reply arrives (c01 +1080 `24 02` then `24 01`; +1142 `09 20` then `09 80`; +2756 heartbeat between per-key reads; +3394 trigger write and read in the same ms). Replies came back in request order. CONFIRMED. Our app should still serialise.
- **Unsolicited reports:**
  - An unsolicited `0x01` status report followed the `0x17` ack in both captured leaves: c02 (`17` at +137930, ack +137943, status +137946) and c07 (`17` at +118182, ack +118194, status +118199), each 3-5 ms after the ack and each just after a mode-changing `07 00` write. OBSERVED (n=2). GUESS: the firmware pushes status when the music-rhythm state is set. These are the only two `0x17` packets in the captures; later `07 00` writes (same mode, no `0x17`) caused no push.
  - **SOCD toggle on the keyboard** (CONFIRMED, c15): each press of Fn + R-Shift sent one unsolicited `24 02 00 00 06 PP 00 00 00 fa e5` + 52 zero bytes, byte-identical in layout to the `24 02` read reply, with PP = the SOCD power (01 -> 00 on the first press, 00 -> 01 on the second; 2 presses, 2 pushes, 5772 ms apart). No `24 01`, `24 00` or status report accompanied either push, and neither followed a request (#218 came 277 ms after a heartbeat reply, #231 50 ms after one). Section 4.15. A `24 02` report is therefore not always a reply: every captured site read is followed by its two `24 01` chunks within 0-3 ms on a quiet link and within up to 13 ms while other replies are queued (c04 #11910), while both pushes came with no `24 01` within ±1 s. GUESS: a `24 02` with no `24 01` within tens of ms (for example 50-100 ms; 10 ms would misclassify c04 #11910) and no `24 02` request of our own pending is a keyboard toggle. Either way its content is the current state.
  - **Live travel stream** `0x21` byte 5 = `01` (existence and depth parse CONFIRMED, c14). It is sent only while the test is open and only when a key's depth changes (OBSERVED; section 4.11.7). None appears in c01-c13 or after the close in c14-c17.
  - **No report when advanced keys fire** (OBSERVED, c16): during the typing test of DKS, MT and TGL keys (c16 #4337-#4544) the vendor collection carried only heartbeat pairs.
  - Handled in the site code but never seen (CODE-ONLY): `0x21` byte 5 = `08`/`0F` (calibration status). The other two unsolicited reports the site handles are captured: the `0x21` byte 5 = `11` profile push (c04 x11, c08, c10, c16 #603, #1088; section 4.5.3) and the `0x14` byte 5 = `01` reset notification (c07 #990 `14 00 00 00 01 01`; section 6.1 row d).

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
| `18 00` / `18 02` | 528 | 56 -> 10 packets (last 24) | none | CONFIRMED (c09, c13, c16, c17; Fn layer with real content in c17) |
| `19 0n` (n = 0-9) | 256 per slot | 58 -> 5 packets (last 24) | after each chunk waits <= 10 zero-ms timer ticks for an input `0x19` with byte 1 < `0x80` | CONFIRMED (c09, c13) |
| `09 00` / `09 01` | 396 | 54 -> 8 packets (last 18) | 5 ms sleep after each chunk (`agreement.js:1725`); 7-10 ms apart on the wire (c17) | CONFIRMED for `09 00` (c02, c17) |
| `25 01` | 132 | 58 -> 3 packets (58, 58, 16) | none | CODE-ONLY |
| `26 01` | 264 | 58 -> 5 packets (last 32) | none | CONFIRMED (c12) |
| `24 00` | 80 | 2 packets `24 00 00 <chunk> 28`, byte 4 = 40 | none: power `24 03` and both chunks leave within 0-2 ms | CONFIRMED (c15, 12 bursts) |
| `22 nn` (nn = 0-39) | 1 packet per slot, byte 4 = 50 | - | none: the 40 slot writes leave in 7-12 ms (c16) | CONFIRMED, all-zero content (c09, c13) and non-empty DKS/MT/TGL slots (c16, c17) |

`0x09` pacing on the wire (OBSERVED, c02): an isolated frame shows 7-8 ms gaps (+160405..+160460). While the colour ring is dragged the site starts new frames before old ones finish, so chunks of different frames interleave (chunk index sequence 0,0,1,0,1,2,0,1,2,3,... from +160978) and the median OUT gap is 0 ms.

### 2.7 Acknowledgements

**General rules** (CONFIRMED, capture session 2): an acknowledgement never carries a chunk index (byte 3 is `00` in every ack), and unless stated otherwise it has length 0. A chunk can therefore only be matched to its ack by sending one chunk at a time and waiting for the ack before the next (stop-and-wait). Match acks on bytes 0-4 (plus byte 5 for the `0x21` family) and ignore the stale bytes after the declared length. Several acks do **not** echo byte 1 of the write.

| Write | Device reply | Latency | Confidence |
|---|---|---|---|
| `0x21` trigger writes | `21 00 00 00 01 00`. Byte 5 is `00` even after a `0x0C` write, so it is not a mode echo (GUESS: status OK). The site has no handler for it. | 2-7 ms, max 34 ms | OBSERVED (c01 +3394 -> +3400; c03 16 writes -> 16 acks; c10) |
| `21 .. 18 02` / `18 03` travel test open / close | `21 00 00 00 01 00` + 57 zero bytes, byte-identical to the trigger-write ack (byte 5 = `00` for sub-commands `00`, `0C`, `02` and `03` alike, consistent with the "status OK" GUESS). Across c01-c17 this ack has only one byte variant, so it cannot be attributed to a specific `0x21` write. | within 1 ms | OBSERVED (c14 #981 -> #982, #1312 -> #1313) |
| `21/12` profile select | `21 00 00 00 02 12 01`, then the push `21 00 00 00 03 11 PP 00` | 3-4 ms | CONFIRMED (c04, c08, c10, c16) |
| `0x17` music-rhythm leave | `17 00 00 00 00`. The site does not wait for it: in both samples the next `07 00` is sent in the same millisecond the ack arrives. An unsolicited `0x01` status report follows 3-5 ms after the ack. | 12-13 ms | OBSERVED (c02 +137930 -> +137943; c07 +118182 -> +118194) |
| `09 00` colour chunks | `09 00 00 00 00` (length 0), within about 1 ms in isolated bursts. **1681 acks for 2064 chunks** in c02 (c02 has 1682 length-0 `0x09` replies; one of them is the `09 20` answer from the connect). The missing acks all fall in two flood bursts (+160978: 1 missing, up to 66 chunks outstanding; +164554: 382 missing, up to 448 outstanding); every isolated 8-chunk burst, and the 296-, 192-, 96- and 56-chunk bursts, is fully acked, which supports stop-and-wait. The ack is byte-identical to the `09 20` reply; the site's dispatcher also treats every such ack as "custom slot 1 selected" (`agreement.js:287-289`). | about 1 ms | CONFIRMED |
| `07 00` light write | **no reply** (10 writes, 0 replies in c02; 2 writes, 0 replies in c17) | - | CONFIRMED |
| `09 00` colour chunks, session 3 | 16 of 16 chunks acked `09 00 00 00 00` (two isolated 8-chunk bursts, not flooded) | 0-3 ms, median 1 | CONFIRMED (c17) |
| `18 00` / `18 02` keymap chunks | `18 00 00 00 00` for **both** layers: byte 1 is not echoed, so the ack cannot identify the layer. 80 of 80 chunks acked in session 2; 140 of 140 in c17, including 70 Fn-layer chunks of which 12 carried non-zero bytes. | 0-32 ms, median 1 | CONFIRMED (c09, c13, c17) |
| `19 0n` macro chunks | `19 00 00 00 00` for **every** slot: the slot is not echoed. 300 of 300 chunks acked (session 2), 350 of 350 (c17). The site waits for an input `0x19` with byte 1 < `0x80` (`agreement.js:481-487, 1549-1552`), which this ack satisfies. | first chunk of a slot up to 28 ms, max 63 ms; c17 median 1, max 28 | CONFIRMED (c09, c13, c17) |
| `22 SS` advanced-key slot | `22 00 00 00 00` + zeros, empty or not: the slot is not echoed. 240 of 240 writes acked in session 2 (zero content), 440 of 440 in c16 (20 non-empty) and 280 of 280 in c17 (21 non-empty). The site does not wait, so acks queue up behind its pipelined writes. When a backlog forms (c16 bursts B1, B3, B5, B7, B10; up to 33-45 outstanding) acks drain at about 2 ms per report; in other bursts (B4, B6) every write was acked within 0-1 ms. The 40-chunk `22 80` read also streams at about 2 ms per chunk (77-78 ms). Never pipeline our own writes: the ack does not identify the slot. | up to 102 ms behind the site's queue (session 2 max 98 ms; c16 max 87 ms; c17 median 29.5, max 102 ms) | CONFIRMED (zero and non-empty content) |
| `26 01` dead-band chunks | `26 01 00 00 00`: byte 1 **is** echoed. 185 of 185 chunks acked. | 0-4 ms | CONFIRMED (c12) |
| `0x14` writes | `14 00 00 00 00 00` + 57 zero bytes (length 0, byte 5 = `00`). One sample: c07 #1017 (+90980) answers the site's reset write #991 (+90161, `14 00 00 00 01 01`): it arrives in queue order, 2 ms before #1018, the first reply to the `26 00` read #992 sent 1 ms after #991. Byte 5 = `00`, so the site does not take it for a reset notice. Ack byte 1 = `00` equals the write's byte 1, so whether it echoes cannot be told. The keyboard was finishing its own reset (notice #990), so the latency may not be typical (GUESS: it answers once the reset is done). The site shows a "Success" toast on any `0x14` input (except byte 1 = 2) and clears its `isResetTrigger` flag (`agreement.js:320-330`). Replies to a Reset-button write and to the Win-Lock, 6-key and gamepad writes are not captured (Q4, Q5). | about 820 ms (n=1) | OBSERVED (c07 #991 -> #1017, n=1); site handling CODE-ONLY |
| `24 00` SOCD pair chunks | `24 00 00 00 00` + 58 zero bytes (length 0, no stale data). Byte 3 is `00` for chunk 1 too, so no chunk index. 24 acks for 24 chunks in 12 bursts; the first 0-2 ms after the burst's first OUT, the second 3-4 ms after the first (GUESS: the firmware applies the table after chunk 1). Ack byte 1 = `00` equals the write's byte 1, so whether it echoes cannot be told. The site has no handler for it (its `0x24` branch acts only on byte 1 = 2 or 1, `agreement.js:800-841`). | 0-4 ms | OBSERVED (c15) |
| `24 03` SOCD power | **no reply**: each burst of 3 OUT got exactly 2 IN; a lone `24 03 00 00 01 00` in c07 (#994, sent by the site's reset flow while the device was busy) got none while every other queued request was answered in order. | - | OBSERVED (n=1 lone sample plus the 2-of-3 count) |
| `25` writes | The site parses no reply. Unknown whether the device sends one. | - | - |

"Foreign" acks: an ack-shaped report that answers none of our writes (for example the ack of the vendor site's empty-mask trigger write at connect and after every `0x11` report, section 4.11.4) shows that another program is writing to the keyboard. OpenWraith counts them and refuses to write while it sees them. Session 3 adds three cases:

- The `21 00 00 00 01 00` ack also answers the vendor site's travel-test open and close (c14), so it may come from any `0x21` write of another program.
- The vendor site's SOCD chunk writes are acked `24 00 00 00 00` (c15). OpenWraith (HEAD 9c74926) does not count this ack as foreign yet; the vendor site is still caught by its heartbeat, but a program without a heartbeat that writes SOCD would go unnoticed (CONFIRMED by code reading and a mock probe; ARCHITECTURE "Still open").
- The keyboard's own unsolicited `24 02` SOCD push (section 2.5) is a user action on the keyboard, like Fn + 1..4, **not** another program. A live travel-stream report (`21 .. 01`) while our own test is closed does show that some program has a travel test open.

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
4. Tolerate unsolicited `0x01` reports at any time, the keyboard's `24 02` SOCD push (section 2.5; take its p[0] as the current SOCD power) and, while any travel test is open, `21 .. 01` stream reports (section 4.11.7). A stream report never matches a `21/05` reply (byte 5 = `05`) or the trigger ack (byte 4 = `01`, byte 5 = `00`), so reads can continue during a live view.
5. Never interleave two multi-chunk writes; throttle colour streaming (section 4.7.6).

### 2.9 Key index, key mask and distance units

**Key index** (CONFIRMED). 132 positions = 6 rows x 22 columns, **index = row x 22 + col**.

- The per-key trigger read carries row in byte 6 (0..5) and column in byte 7, and the device echoes both in reply bytes 15/16 (246 of 246 replies in c01 and c03).
- The W75 uses 82 positions (section 5). W = row 2, col 2 = index 46 (CONFIRMED by c03).
- Confirmed for: `0x21/05` reads, `0x21` trigger-write mask, `0x0E`, `0x09` colour tables (c02: the 81 slots the site painted are exactly the 82 read positions minus the knob at 15; c17: the LEDs under Up 102 and Right 125 lit from their slots (USER-REPORTED)), `0x18` (base layer W in c09, Caps in c13, E/G/H in c16; Fn layer F1-F4, W and Caps in c17, where Fn+W was an explicitly instructed test and the others rest on the user's "everything I assigned works") and the travel-test open mask (c14). OBSERVED: travel-stream row/col bytes (c14: W, Esc, Space, Right, A). CODE-ONLY for `0x25`, `0x26` and calibration masks (captured tables are uniform).

**22-byte key mask** (used by `0x0E`, the `0x21` trigger write, travel-test open and calibration progress):

- Byte j (0..21) = column j, bit b (0..5) = row b, key index = b x 22 + j. Bits 6-7 unused.
- Confidence: CODE-ONLY orientation (`agreement.js:300-310, 544-552, 962, 1809`), consistent with captures. The c03 W write (byte 2 = `0x04`) cannot tell the orientation apart because W sits on the diagonal (row 2 = col 2). The `0x0E` reply (bit 5 set in byte **11**) is only possible with byte = column, since there are only 6 rows. A Q-only trigger write (capture plan C8) would confirm it for `0x21`.
- W75 mask of all 82 positions (used by the travel-test open): `3f 26 3f 1f 1f 1f 3f 1f 1f 1f 3f 3f 2f 31 3f 2f 00 00 00 00 00 00`. CONFIRMED (c14 #981, byte 21 = `2f`, so the knob at 15 is included).
- W75 mask of the 81 selectable keys (Select All, knob excluded): `3f 26 3f 1f 1f 1f 3f 1f 1f 1f 3f 3f 2f 31 3f 2e 00 00 00 00 00 00`.

**Distance units** (CONFIRMED):

- Travel, Rapid Trigger sensitivity and dead band are integers in *trigger units*. unit = `rawUnit / 100` mm, or `/1000` when the high-precision flag is set; rawUnit 0 is treated as 10 (`agreement.js:497-501`).
- W75: rawUnit = 2 (`21/04` reply byte 7) and high precision = 0 (status byte 18 bit 0), so **1 unit = 0.02 mm**. Examples: `0x55` = 85 = 1.70 mm, `0x32` = 1.00 mm, `0x0F` = 0.30 mm.
- Exception: DKS press points use **0.1 mm** units (wire values OBSERVED in c16: 15 and 30 for the UI's 1.5 mm and 3.0 mm; the unit is supported by the user's typing test but not proven, section 4.14).
- The live travel stream reports depth in trigger units: a full press reads 164-170 = up to the `21/04` maximum (c14, section 4.11.7).

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
| `0x21` | byte 5 = 1 | live key travel: parses only the depth (bytes 8-9, little-endian) into one bar; ignores byte 4, bytes 6-7 (evaluated and discarded) and the long-variant fields (seen in c14, section 4.11.7) | 488-495 |
| `0x21` | byte 5 = 4 | max travel, unit, min travel | 496-515 |
| `0x21` | byte 5 = 5 | per-key trigger record | 516-519 |
| `0x21` | byte 5 = 9 | poll rate (only 1/2/4/8 accepted) | 520-525 |
| `0x21` | byte 5 = 8 or 15 | calibration status | 526-568 |
| `0x21` | byte 5 = `0x11` | active onboard profile, then a full reload. The else branch (634-640) is unreachable because byte 6 is unsigned. | 569-655 |
| `0x22` | byte 1 = `0x80` | advanced-key slot (chunk index = slot) | 656-799 |
| `0x24` | byte 1 = 2 / 1 | SOCD power (also the keyboard's unsolicited Fn + R-Shift push, c15: `refreshPrcsValue` sets the switch from code, which fires no change event, so the site writes nothing back) / SOCD pairs (appended to the list, never cleared here; section 6.5) | 800-841 |
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
| 15 | `14` | `00`, byte 5 = `01` | W | **Reset current profile** | `14 00 00 00 01 01` | CONFIRMED (bytes, c07 #991); effect and scope CODE-ONLY (Q4) | **D** | §4.6.3 |
| 16 | `14` | IN, byte 5 = `01` | IN | Device reset notification (site then sends a reset) | `14 00 00 00 01 01`, sent when Fn+Space is held 3-5 s (c07) | CONFIRMED | **D** (receive-side hazard) | §4.6.3 |
| 17 | `14` | `01` | W | Set sleep timers (PIDs `C370`/`C371` only, not W75) | [8..9] light sleep s BE16, [10..11] keyboard sleep s BE16 | CODE-ONLY | | §4.6.5 |
| 18 | `14` | `02` | R | Get sleep timers (not W75) | reply [1]=2, same fields | CODE-ONLY | | §4.6.5 |
| 19 | `14` | `03` | W | **Gamepad mode** | `14 03 00 00 01 MM`, MM 0 off / 1 Xbox 360 / 2 Classical; device restarts | CODE-ONLY | **D** | §4.6.4 |
| 20 | `17` | `00`, `00 00` | W | Music rhythm leave | `17 00 00 00 02 00 00`; ack `17 00 00 00 00`; sent before every main-mode change | CONFIRMED | | [§4.7](#47-lighting) |
| 21 | `17` | `00`, `01 01` | W | Music rhythm enter | `17 00 00 00 02 01 01`; not offered on W75 | CODE-ONLY | | [§4.7](#47-lighting) |
| 22 | `18` | `80` / `82` | R | Read keymap base / Fn layer | 10 chunks, 528 B = 132 x 4 B | CONFIRMED (framing; after c09 the base table reads back the site's knob defaults at 16-21 and `01 00 00 00` at 121; after c17 OpenWraith's snapshot read back the non-zero Fn table byte for byte); entry kinds other than `00`, `07`, `10`, `12`, `13` and `17` CODE-ONLY | | [§4.8](#48-keymap-0x18) |
| 23 | `18` | `00` / `02` | W | Write keymap base / Fn layer | 10 chunks of 56 B (last 24), each acked `18 00 00 00 00`. Plain remap W -> A = entry 46 `00 04 00 00` (typing test passed); advanced-key bindings `17 SS SS 00` (c16); Fn layer Mute `07 12`, wheel down `12 01 ff 00`, mouse `12 00 10 00`, `07 25`, Calculator `07 32`, Ctrl+Shift+A `13 e0 e1 04` (c17); default = `00 00 00 00` | CONFIRMED (c09, c13, c16, c17; base and Fn layer with real content) | C | [§4.8](#48-keymap-0x18) |
| 24 | `19` | `80`-`89` | R | Read macro slot 0-9 | 5 chunks, 256 B per slot | CONFIRMED (framing, all-FF content); slot format CODE-ONLY on the read side | | [§4.9](#49-macros-0x19) |
| 25 | `19` | `00`-`09` | W | Write macro slot 0-9 | 5 chunks of 58 B (last 24), each acked `19 00 00 00 00` (sub always `00`). Keyboard-step slot layout of §4.9.3 matches c13 byte for byte (typing test passed) | CONFIRMED (keyboard steps, play once); mouse steps and other play modes CODE-ONLY | C | [§4.9](#49-macros-0x19) |
| 26 | `21` | byte 5 = `00` / `0C` | W | Trigger write (actuation, Rapid Trigger) for a key set | [4]=`18`, [5] mode, [6..27] key mask, [28..35] travel/sensitivity lo/hi | CONFIRMED | | [§4.11](#411-actuation-and-rapid-trigger-0x21) |
| 27 | `21` | byte 5 = `0D` | W | Trigger write, RT with separate press/release | same layout; byte 30 = press, byte 31 = release (W press 0.30 `0f`, release 0.50 `19`) | CONFIRMED (c10) | | [§4.11](#411-actuation-and-rapid-trigger-0x21) |
| 28 | `21` | IN byte 5 = `00` | IN | Trigger write ack (also the travel-test open/close ack) | `21 00 00 00 01 00` | OBSERVED | | [§4.11](#411-actuation-and-rapid-trigger-0x21) |
| 29 | `21` | IN byte 5 = `01` | IN | Live travel stream (travel test) | one key per report: [6] row, [7] col, depth = bytes 8-9 little-endian (units, 0-170 on the W75); short variant byte 4 = `05`, long variant byte 4 = `03` with data to byte 36 | CONFIRMED (c14; depth value and units; endianness CODE-ONLY); row/col, variants and long-variant fields OBSERVED | | §4.11.7 |
| 30 | `21` | byte 5 = `02` | W | Open travel test | [6..27] mask of all layout keys (W75: the 82-position mask, knob included) | CONFIRMED (c14) | | §4.11.7 |
| 31 | `21` | byte 5 = `03` | W | Close travel test | `21 00 00 00 18 03`; no stream report afterwards | CONFIRMED (c14) | | §4.11.7 |
| 32 | `21` | byte 5 = `04` | R | Global trigger parameters | reply `04 aa 02 02 00 05`: max 3.40 mm, unit 0.02 mm, min 0.10 mm | CONFIRMED | | [§4.11](#411-actuation-and-rapid-trigger-0x21) |
| 33 | `21` | byte 5 = `05` | R | Per-key trigger read | request row/col; 12-B reply echoing row/col | CONFIRMED | | [§4.11](#411-actuation-and-rapid-trigger-0x21) |
| 34 | `21` | byte 5 = `06` | W | **Reset all key triggers** (active profile) | `21 00 00 00 18 06`; ack `21 00 00 00 01`, then about 1 s later the device pushes `21 00 00 00 02 11 PP`; all 82 keys read `00 55 55 01 01` afterwards | CONFIRMED (c11) | **D** | §4.11.6 |
| 35 | `21` | byte 5 = `07` | W | **Single-key calibration** (dead code in the site) | `21 00 00 00 18 07 row col` | CODE-ONLY | **D** | §4.11.8 |
| 36 | `21` | byte 5 = `08` | W / IN | **All-key calibration** start (`08 00`) / stop (`08 01`) and status (devices without the any-key flag; not the W75 path) | status [6]: 0 done, 1 progress + mask, 2/4 fail | CODE-ONLY | **D** | §4.11.8 |
| 37 | `21` | byte 5 = `0F` / `10` | W / IN | **Any-key calibration** start (`0F 00`) / stop (`10 00`) and status `0F` (W75 path) | as above | CODE-ONLY | **D** | §4.11.8 |
| 38 | `21` | byte 5 = `09` | R | Read polling rate | `21 00 00 00 01 09` -> `.. 02 09 RR`; W75 RR = `08` (8 kHz) | CONFIRMED | | §4.6.1 |
| 39 | `21` | byte 5 = `09`, byte 6 = `01` | W | Set polling rate | `21 00 00 00 00 09 01 RR`; device restarts per UI | CODE-ONLY | C | §4.6.1 |
| 40 | `21` | byte 5 = `11` | R | Read active onboard profile | `21 00 00 00 00 11` -> `.. 03 11 PP XX`; observed `11 01 01` | CONFIRMED | | [§4.5](#45-onboard-profiles) |
| 41 | `21` | byte 5 = `12` | W | Select onboard profile | `21 00 00 00 00 12 PP` (PP 1-4); ack `21 00 00 00 02 12 01` within 4 ms, immediately followed by the push `21 00 00 00 03 11 PP 00`. The site then re-reads everything and sends only the no-op empty-mask trigger write (section 4.11.4) | CONFIRMED (c08, c10, c16) | | [§4.5](#45-onboard-profiles) |
| 42 | `22` | `80` | R | Read 40 advanced-key slots | 40 chunks x 40 B, chunk index = slot; payload = write bytes 5-44 | CONFIRMED (framing; TGL content read back in c16; DKS/MT read-back not captured) | | [§4.14](#414-advanced-keys-0x22) |
| 43 | `22` | `00`-`27` | W | Write one advanced-key slot | `22 NN 00 00 32 TT ...`, each acked `22 00 00 00 00`. DKS `01` routes + 4 actions + steps, MT `02` hold, tap, time, TGL `03` action | CONFIRMED (all-zero content c09/c13; DKS, MT, TGL in c16/c17 with typing tests); RS/RKRT/MDS CODE-ONLY | C | [§4.14](#414-advanced-keys-0x22) |
| 44 | `24` | `01` | R | Read SOCD pairs | 2 chunks x 40 B = 10 x 4-B entries each | CONFIRMED (framing, all-zero content) | | [§4.15](#415-socd-0x24) |
| 45 | `24` | `02` | R / IN | Read SOCD power and hotkey; also pushed unsolicited when Fn + R-Shift toggles SOCD on the keyboard | reply / push `PP 00 00 00 fa e5` (PP power; Fn + R-Shift) | CONFIRMED (push c15) | | [§4.15](#415-socd-0x24) |
| 46 | `24` | `00` | W | Write SOCD pairs | 2 packets `24 00 00 <chunk> 28`, 10 x [1, model, key1 HID, key2 HID] each; each acked `24 00 00 00 00` | CONFIRMED (bytes, c15); ack `24 00 00 00 00` OBSERVED; firmware behaviour of the models untested | C | [§4.15](#415-socd-0x24) |
| 47 | `24` | `03` | W | SOCD on/off | `24 03 00 00 01 PP`; no ack | CONFIRMED (framing, c15); no ack (OBSERVED, n=1 lone sample, section 2.7); effect not read back while on | | [§4.15](#415-socd-0x24) |
| 48 | `25` | `00` | R | Supported switch types | `00 01 01` = 1 type, ID 1 (HM1) | CONFIRMED | | [§4.13](#413-switch-types-0x25) |
| 49 | `25` | `02` | R | Per-key switch type table | 3 chunks, 132 B; W75 all `01` | CONFIRMED (framing, uniform content); per-key mapping CODE-ONLY | | [§4.13](#413-switch-types-0x25) |
| 50 | `25` | `01` | W | Write per-key switch type table | 3 chunks (58, 58, 16) | CODE-ONLY | C | [§4.13](#413-switch-types-0x25) |
| 51 | `26` | `00` | R | Read dead band (the site's "reset dead band" sends the same bytes) | 5 chunks, 264 B = 132 x (top, bottom) | CONFIRMED (framing, all-zero content) | | [§4.12](#412-dead-band-0x26) |
| 52 | `26` | `01` | W | Write dead band | 5 chunks of 58 B (last 32), acked `26 01`. Byte 2 x index = top, 2 x index + 1 = bottom, 0.02 mm units (W top 0.10 mm = offset 92 `05`, bottom 0.20 mm = offset 93 `0a`) | CONFIRMED (c12) | C | [§4.12](#412-dead-band-0x26) |

**Counts (updated 2026-10-27, capture session 3):** 52 rows. CONFIRMED 38 (of which rows 5, 7, 13, 22, 24, 25, 29, 42, 43, 44, 46, 47 and 49 are confirmed only for framing/bytes or for part of the content format), OBSERVED 1, CODE-ONLY 13, GUESS 0. DANGEROUS 7 rows (15, 16, 19, 34, 35, 36, 37). Session 3 moved rows 29, 30, 31, 46 and 47 from CODE-ONLY to CONFIRMED. See sections 7.3 and 7.4 for what sessions 2 and 3 established.

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

- If the string contains `dongle` the site counts dongles and shows its mode-change alert (`language.json` `sleepSetTip7`) when it sees more than one (`wmIndex.js:6640-6645`). CODE-ONLY.
- Site quirk: it decodes bytes 5..len-1 instead of 5..5+len-1 (`wmIndex.js:6625`). Harmless here. **Our parser:** take `len` bytes from byte 5, cut at the first NUL, split on commas.

### 4.2 Status and capabilities (0x01)

**Request:** byte 0 = `0x01`, everything else 0 (`agreement.js:900-923`).

- `initInfo` sends it once during connect and waits for the reply (c01 +914, c03 +921).
- `heartbeat` then sends it every 1000 ms without waiting (`wmIndex.js:6138`). Observed intervals 985-1842 ms.
- **The heartbeat is not required.** CODE-ONLY: later replies only drive the battery/link display and the mode-change reload. USER-REPORTED: blocking it for 90 s changed nothing on the keyboard. Our app needs one `0x01` at connect to read the flags; periodic polling is optional.

**Reply:** all 267 independent status reports in the captures (c02: 198 including c01's 44; c03: 69) are identical, one of them unsolicited (c02 +137946; a second unsolicited one is in c07 at +118199, also right after a `0x17` ack). Session 3 adds 158 (c14), 156 (c15), 591 (c16) and 436 (c17) replies, all this same value, including while SOCD was on and while profile 2 was active:

`01 00 00 00 12 | 00 00 00 00 00 00 00 00 00 01 00 00 df 04 01 01 00 00`

`p[i]` = byte 5+i. The site parses `p[0..17]` (`agreement.js:19-72`).

| Byte | p | Site meaning | W75 value | Confidence |
|---|---|---|---|---|
| 1 | - | Link-mode token. If it changes between two reports the site shows its mode-change alert (`sleepSetTip7`) and reloads after 1 s (`agreement.js:46-57`) | `00` | CODE-ONLY |
| 5 | 0 | read, then ignored | `00` | - |
| 6, 7 | 1, 2 | not parsed | `00` | - |
| 8 | 3 | 1 = charging (battery PIDs only) | `00` | CODE-ONLY |
| 9 | 4 | battery percent (battery PIDs only) | `00` | CODE-ONLY |
| 10 | 5 | link type: 0 = USB, 1 = 2.4 GHz, other = blank | `00` | CODE-ONLY |
| 11-13 | 6-8 | not parsed | `00` | - |
| 14 | 9 | 1 = SOCD ("PRCS") supported | `01` | CONFIRMED (SOCD reads sent, SOCD tab visible). Not the SOCD power: it stayed `01` in all 156 replies of c15, with SOCD off and on (OBSERVED). The SOCD state is only in `24 02` reads and pushes |
| 15, 16 | 10, 11 | not parsed | `00` | - |
| 17 | 12 | capability flags A (below) | `DF` | see below |
| 18 | 13 | capability flags B (below) | `04` | see below |
| 19 | 14 | **not parsed by the site** | `01` | unknown. Not the active profile: `01` in all 7 replies while profile 2 was active (c16 #603..#1087). Not the SOCD power either (c15). OBSERVED. GUESS: Win-Lock state. Use `21/11` or the `11 PP` push for the active profile |
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

c14 (session 3) repeats c03: up to the empty-mask write (entry 650 in both) the two have 213 identical non-heartbeat OUT packets in the same order; only reply contents differ (macro slots `00` vs `FF`, the `07` foreground, W/A/S/D and Q triggers). Timings: `21/11` +2049 -> reply +2108 (59 ms), empty-mask write +3100 -> ack +3105. OBSERVED.

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

- Latency 39-65 ms over six independent connects (others about 1 ms; section 2.5). OBSERVED.
- Numbering: the `#profiles` radios in `v1.html:608-629` are ProfileDef, Profile1..Profile4. On a `0x11` reply the site ticks `"Profile"+PP` and hides ProfileDef (`agreement.js:578, 624`). So there are **4 onboard profiles numbered 1-4 on the wire** (CODE-ONLY). Whether 0 is valid is unknown.
- `PP` = 1 in both captures. Whether that is the factory default is not shown.

#### 4.5.3 Select a profile: `0x21`, byte 5 = `0x12` (CONFIRMED)

- **Request:** `21 00 00 00 00 12 PP` + zeros (`agreement.js:2255-2265`), PP = 1-4.
- The site sends it when a profile tab is clicked and PP differs from `presetNumber` (`wmIndex.js:1014-1023`). It does not await a reply, does not update `presetNumber` and re-reads nothing by itself.
- The site's UI resyncs only when a `0x21` report with byte 5 = `0x11` arrives. The keyboard also pushes one when the profile is changed on the keyboard (Fn + 1..4 per `fnKeys.json`): OBSERVED 2026-10-27 in desktop-app test A9 (the idle, unfocused app, which only listens, followed Fn + 2 at once), not yet seen in a raw capture.
- **On the wire** (OBSERVED, c04, 11 site-initiated switches from entry 8293 on, all four profiles): the keyboard acks `21 00 00 00 00 12 PP` with `21 00 00 00 02 12 01` within 3-17 ms and pushes `21 00 00 00 03 11 PP 00` right after it (0-2 ms later). Byte 7 of that push is `00`, while replies to our `21/11` query carry `01`. The meaning of ack byte 6 (`01`) is unknown. Every `0x11` report in c04 follows a site-sent `0x12`; **no keyboard-initiated switch has been captured** (the Fn + 1..4 question of Q2 stays open).
- **Captured again** (CONFIRMED, c08 x2, c10 x2, c16 x2): ack `21 00 00 00 02 12 01` within 3-4 ms, then the push. Selecting a profile changes no setting, and the tables read afterwards are the target profile's. **Correction (2026-10-27):** the site does send one more write after a select: after each `0x11` push it sends the no-op empty-mask trigger write `21 00 00 00 18 00` + zeros, acked `21 00 00 00 01 00` (c08 #1137, #1624; c10 #607, #1108; c16 #908, #1393), from the same `0x11` handler code as at connect (`agreement.js:641-643`, section 4.11.4). A profile click therefore produces two writes, not one. CONFIRMED.
- **Our app:** after sending `0x12`, poll `21/11` until it reports the new profile, then re-read every table (section 6.10). OpenWraith waits up to 500 ms for the ack and the push (reading the push before any `21/11` query, whose reply has the same shape), confirms with `21/11` and retries once. It never selects profile 0 (Q11).
- **Delayed commit: a switch soon after a write loses the write** (CONFIRMED on hardware, 2026-10-24, OpenWraith hardware test H9 and a dedicated probe). A trigger write in profile 2 is acked and reads back correctly at once, but if `21/12` selects another profile within about 1 s of the write, the change is discarded: after re-selecting profile 2 the old value reads back. Probe results (write Q in profile 2, wait D, select profile 1, wait 2.5 s, select profile 2, read Q): D = 0, 150, 400 and 800 ms -> LOST; D = 1000, 1100, 1200, 1300 and 1500 ms (x2) -> PERSISTED. A write made with at least 2 s before the next select (separate CLI commands) also persisted. Interpretation (GUESS): the firmware keeps edits in a RAM working copy of the active profile and commits it to flash about 1 s after the last write; a profile switch before that reloads the profile from flash. Read-back verification cannot detect this, because it reads the RAM copy. Consequences: wait at least 2 s after the last write before any profile switch, and verify writes to a non-active profile by re-selecting it and reading again. Whether a USB power loss within 1 s of a write loses it too is untested (very likely). The vendor site has the same problem when a user edits and switches profiles quickly.

#### 4.5.4 What the site re-reads after a `0x11` report (`agreement.js:572-633`)

- **Re-read** (CONFIRMED c01/c03): `07 01`, `21/04`, `21/05` x 82, `19 80`..`89`, `22 80`, `24 02`/`01`, `26 00`, `18 80`, `18 82`. GUESS: these tables are per profile.
- **Not re-read:** `08 01` side light, `09 80`/`81` custom colours, `21/09` poll rate, `25 02` switch types, `0E`, `01` status (Win-Lock/6-key bits come only from the first `01` reply). GUESS: global settings, or an omission in the site.
- OBSERVED (c04, after the site switched profiles): W's `21/05` reads `05 0c 32 32 0f 0f` (1.00 mm, RT 0.30) only in profile 1 and `05 00 55 55 01 01` in profiles 2-4; the main light `07` reads mode `0x0A` in profile 1 and `0x02` in profiles 2-4. So **triggers and the main light are per profile**. `09 80`/`81`, `21/09` and `25 02` were never re-read after a switch, so their scope is still unknown.
- OBSERVED (c10, capture session 2): after the c09 keymap Applies on profile 1, profile 2 read an all-zero base keymap (`18 80`) while profile 1 read the site's knob defaults, and profile 2 read every macro slot as all `FF` while profile 1 read all `00`. So **the base keymap and the macro slots are per profile** too.
- OBSERVED (c16, capture session 3): TGL H was written to advanced-key slot 0 of profile 1 with no keymap binding (c16 #489, +179146); about 10 s later (#601, +189418) the site selected profile 2, whose `22 80` read returned all 40 slots empty (#833-#873), then profile 1, whose read returned slot 0 = `03 00 0b 00 00` (#1320). So **advanced-key slots are per profile, and an unbound slot is kept** by the firmware. The same round trip again shows macro slots and the base keymap per profile; `18 82`, `24 01`/`02` and `26 00` read identically (factory or empty) in both profiles, so the trip says nothing about their scope.
- OBSERVED (OpenWraith pre-restore safety snapshot after c17, which wrote nothing but the `21/12` profile selects): profile 1 held the six non-zero Fn-layer entries of c17 and advanced slots 0-2, while profiles 2-4 read an all-zero Fn table and 40 empty slots. So **the Fn keymap (`18 82`) is per profile** too. Custom table 2 read the factory 34 blue slots in all four profiles, so its scope is still unknown.
- Which settings really are per profile is open question Q3 (partly answered above). Still unknown: `26 00`, `09 81`, `09 20`, `08 01`, `24 01`/`02`, `25 02`, `21/09` and the status Win-Lock / 6-key bits; they read identically in every capture. OpenWraith treats them as possibly global: before writing one in a restore it re-reads it and skips the write when it already matches.

### 4.6 Global settings

#### 4.6.1 Polling rate: `0x21`, byte 5 = `0x09`

- **Read (CONFIRMED, c01 +1720, c03 +1689):** `21 00 00 00 01 09` + zeros -> `21 00 00 00 02 09 RR`.
  - RR: `01` = 1 kHz, `02` = 2 kHz, `04` = 4 kHz, `08` = 8 kHz (`wmIndex.js:6072-6083`, `v1.html:1254-1257`). The site ignores other values. **W75: `08`.**
- **Write (CODE-ONLY, CAUTION):** `21 00 00 00 00 09 01 RR` (`agreement.js:1872-1883`). GUESS: byte 6 = 1 means "set".
  - The UI first asks the user to confirm and warns of a restart (`language.json` `rateTip2`; `wmIndex.js:802-808, 1394-1408`). No reply is awaited. GUESS: the device re-enumerates.
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

#### 4.6.3 Reset current profile: `0x14 00`, byte 5 = `01` (**DANGEROUS**; bytes CONFIRMED, scope open)

- **Request:** `14 00 00 00 01 01` + zeros (`agreement.js:1747-1757`). CONFIRMED: c07 #991 is the site sending exactly these bytes, 1 ms after the keyboard's reset notice #990 (section 6.1 row d). A reset started from the Reset button has not been captured.
- **UI:** the Reset button asks the user to confirm a reset of the current profile (`language.json` `resetAlert`; `wmIndex.js:1322-1326`). GUESS: the firmware resets only the active profile. Scope is open question Q4; c07 gives one reply sample (section 2.7).
- **The site's full reset flow** (`wmIndex.js:1331-1381`, verified):
  1. Send `14 00 00 00 01 01` (no wait for a reply).
  2. Call `resetTrigger`, which sends `21 00 00 00 18 06` **only if** its flag `isResetTrigger` is set (section 4.11.6).
  3. If `isInitDeadBand` is false, send `26 00` (the dead-band **read** request; section 4.12).
  4. Clear RS advanced keys in memory, save the localStorage mirror, sleep 100 ms, send `24 03 00 00 01 00` (SOCD off).
  5. Re-read `07 01`, `21/04`, `21/05` x 82; sleep 100 ms; clear keymaps and macro bindings **in memory only** (not re-read).
  6. Re-dispatch the selected profile tab. Onboard mode: nothing is sent. Software mode: full profile push.
- **Device-initiated reset (receive-side hazard):** any input report with byte 0 = `0x14` and byte 5 = `0x01` (any byte 1) makes the site mark the pending confirmation as a full reset and press its own OK button in code, so it runs the whole flow above without asking; that flow sends another reset (`agreement.js:314-319`). CONFIRMED (c07 #990 -> #991).
  - The keyboard sends it when Fn + Space is held about 3-5 s (c07, section 6.1 row d; CONFIRMED, hold time USER-REPORTED), matching Fn + Space in `fnKeys.json`.
  - Risk 1 (GUESS): if the firmware acked `14 00 00 00 01 01` with byte 5 = `01`, the reset would re-trigger itself repeatedly. In c07 the only later `0x14` input was `14 00 00 00 00 00` (#1017, byte 5 = `00`), most likely the in-order reply to the site's reset write #991 (section 2.7), so the site did not loop (OBSERVED, n=1); the reply to a Reset-button write, sent while the keyboard is not already resetting, is untested (Q4).
  - Risk 2 (GUESS): `setGamePadMode(1)` sends `14 03 00 00 01 01`; if the firmware echoed it, the site would reset the current profile.
- **Our app:** explicit confirmation, back up all tables first, never react to an incoming `0x14` with a reset.

#### 4.6.4 Gamepad mode: `0x14 03` (**DANGEROUS**, CODE-ONLY)

- **Request:** `14 03 00 00 01 MM`, MM = 0 Disabled, 1 Xbox 360, 2 Classical (`agreement.js:1758-1769`; `wmIndex.js:682-705`; `v1.html:829-857`).
- The UI first asks for confirmation and warns that the keyboard restarts (`language.json` `gamePadTip1`; `wmIndex.js:689-701`). No reply awaited.
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
- Session 3 (CONFIRMED): connect read c14 #141 `0a 04 03 ff 30 97 00 00 00 00 01 00` (Custom, fg `ff3097`). Both `07 00` writes of the c17 colour edits (#2239 +342193, #2280 +354446) were `07 00 00 00 0e | 0a 04 03 00 00 ff 00 00 00 00 01 00 00 00`: mode `0x0A`, brightness 4, speed 3, **fg `0000ff` = the clicked swatch**, bg black, direction 0, fullColor 1, power on, 2 pad bytes. No reply. OpenWraith's later read returned exactly these 12 bytes. The site reads fg from its foreground label (`wmIndex.js:4928-4931`), which a colour pick repaints only while the Foreground radio is selected (`wmIndex.js:5363-5365`; with Background selected bg would change instead). So in Custom mode every picked colour also overwrites the effect's foreground. GUESS: in mode `0x0A` the keyboard shows the per-key table and ignores fg (not checked directly).
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
| `0A` | Custom (per-key, section 4.7.6) | added by the site | yes | - | - (0) | swatch still sent: fg = the last picked colour (CONFIRMED, c17) | - | hidden |
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

- Slot = key index (row x 22 + col): CONFIRMED for the 81 slots the site painted in c02 (exactly the read positions minus the knob). **LED check (c17, C11 b):** painting only slot 102 (Up), then only 125 (Right) blue lit the LEDs under Up and Right (USER-REPORTED: "the LEDs I assigned work too"; no per-key answer was given). So LED slot = key index for Up and Right; slot 103 is not the Up LED and slot 126 is not the Right LED. Left (123) and Down (124) were not painted alone, and the LEDs of 103/126, if any, are unknown.
- GUESS: the keyboard shows a table only in mode `0x0A`. The site edits tables only in mode 10.

**Read `09 80` / `09 81`** (CONFIRMED): request `09 80 00 00 00`; 8 replies `09 80 <idx hi> <idx lo> <len>` + data. Chunks 0-6 len `0x36` (54 B = 18 slots), chunk 7 len `0x12` (18 B = 6 slots). Slot = 18 x chunk + k. About 2 ms apart (c01 +1143..+1156, +1161..+1171). Chunk 7 carries stale bytes after `len` (c01 +1156); honour `len`.

**Write `09 00` / `09 01`** (CONFIRMED for `00`, CODE-ONLY for `01`; CAUTION):

- Same chunking: 8 reports `09 SS <idx hi> <idx lo> <len>` + data, 54 B per chunk, last 18 B.
- Each chunk is acknowledged with `09 00 00 00 00` (len 0); 1681 acks for 2064 chunks in c02, 16 of 16 in c17 (0-3 ms, median 1; section 2.7).
- **Swatch edit on the wire** (CONFIRMED, c17): the Custom1 step produced exactly 2 full tables (#2223-#2238 +342130, #2264-#2279 +354382; 12.2 s apart), chunks 7-10 ms apart, each table followed 8-9 ms later by one `07 00` (section 4.7.2). This is the swatch handler: `setRgb`, `setCustomRgb` (one table, only in mode 10), `setDeviceLight` (`wmIndex.js:1766-1783, 5370-5375`; `setCustomLight` `agreement.js:1685-1726`). The RGB-box handlers (`wmIndex.js:1739-1765`) produce the same pattern; the swatch path is inferred from the exact `0000ff`. Only checked keys get the new colour (`setKeyColor`, `wmIndex.js:7053-7064`), so the second table kept slot 102 blue. No `09 01`, `09 20`, `17` or `07 01` was sent; Deselect All and key selection send nothing.
- **A write always replaces all 132 slots.** The site zero-fills a buffer and fills only slots of keys in its layout file (writing each key's stored colour, including the knob at 15, whose stored colour was black). Every other slot becomes `000000`. CONFIRMED: device-side slots 103 and 126 were red at connect (c01) and were `000000` already in the first burst (c02 +160405).
- **It is not a preview command.** No other `0x09` writer exists; the same call is used for every edit. The table persists across a page reload (the final c02 table, 81 slots `ff0000`, reads back unchanged in c03 +1150..+1164). CONFIRMED. Persistence across a USB power cycle is untested (open question Q1).
- **Flood behaviour** (OBSERVED, c02): 258 full-table bursts (2064 chunks) in about 15 s (+160405..+175203), median 8 ms between burst starts. Dragging the intensity ring starts a new table write on every mouse-move without waiting for the previous one, so bursts interleave on the wire.

**Active slot query `09 20`** (bytes CONFIRMED, meaning CODE-ONLY): request `09 20 00 00 00`; W75 answers `09 00 00 00 00` (c01 +1142, c03 +1150). The site reads byte 1: 0 = Custom1, 1 = Custom2 (`agreement.js:286-293`). The reply is byte-identical to a write ack. How the device chooses the displayed table is unknown; clicking Custom1/Custom2 in the site only writes that table (GUESS: the last-written table becomes active).

**Observed contents** (CONFIRMED):

- c01 (before any change): Custom1 = 8 red slots {46, 68, 69, 70, 103, 124, 125, 126}; Custom2 = 34 blue slots {45-56, 58, 68-78, 90-99}.
  - {46, 68-70} = W, A, S, D. The other four are offset by +1 from the inferred arrow keys {102, 123, 124, 125}; slots 103 (row 4, col 15) and 126 (row 5, col 16) are not keys in our layout. Whether LED slot = key index for every key is open question Q8 (answered for Up and Right in c17, see above).
- After c02: Custom1 = 81 red slots (every selectable key); Custom2 unchanged. **The factory Custom1 pattern is gone from the user's keyboard.**
- Session 3: c14 #147.. read Custom1 = 81 x `ff0000`, 51 x `000000`. The final c17 table: `0000ff` at 102 and 125, `ff0000` at the other 79 selectable keys, `000000` at the knob (15) and the 50 non-layout slots; OpenWraith's snapshot read it back byte for byte. OpenWraith then restored Custom1 from a backup (section 7.4). Profiles 2-4 still hold the factory 8-slot Custom1 pattern.

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
- Colour edit in Custom mode: full `09 00`/`01` table on every colour event, plus `07 00` on click/release. Swatch click CONFIRMED (c17): one `09 00` table, then one `07 00` carrying the picked colour as fg.

#### 4.7.9 Recommendations for our app

1. W75: hide side light, never write `0x08`; offer Custom (`0x0A`) although it is not in the list; no music mode.
2. Read-modify-write the 12-byte `0x07` record; send len 14 with two zero pad bytes; expect no reply. Use `0x0A` byte n-1 as max brightness and n-2 as max speed (site convention, unverified on W75).
3. Keep both 132-slot tables client-side from the `09 80/81` reads. Write all 8 chunks serialised, waiting for each `09 00 00 00 00` ack (timeout + one retry). Throttle to "on release" or <= 10 Hz. **Preserve slots outside our key map** (e.g. 103, 126) instead of zeroing them.
4. Optionally send `17 .. 00 00` before a mode change for parity with the site.
5. Do not interleave a `09 20` query with table writes; their replies are identical.
6. Per-key colours can be painted by key index, including Up and Right (c17); Left (123) and Down (124) are untested (Q8). When the user edits per-key colours, do not touch the `0x07` record's fg: unlike the site, write only the table (read-modify-write the `0x07` record only for its own edits).

### 4.8 Keymap (0x18)

Keymap writes were captured in session 2 (c09: W -> A and back; c13: a macro bound to Caps and removed) and session 3 (c16: advanced-key bindings; c17: special codes on the Fn layer). The write framing and acks, plain key entries `00 HH 00 00`, macro entries `10 39 SS 00`, advanced-key bindings `17 SS SS 00`, multimedia `07`, mouse `12` and two-modifier combinations `13` are CONFIRMED; the other entry kinds are CODE-ONLY. Fn-layer writes with real content were captured in c17.

#### 4.8.1 Read `18 80` (base) / `18 82` (Fn layer) (CONFIRMED framing)

- Request `18 80 00 00 00` or `18 82 00 00 00`, no payload (`agreement.js:1067-1091`).
- Reply: 10 reports `18 8x 00 <chunk 0..9> <len>`, len `0x38` (56) for chunks 0-8 and `0x18` (24) for chunk 9 = 528 bytes = 132 entries x 4 bytes. Entry for key k at offset 4k (chunk k / 14, offset (k mod 14) x 4); no entry crosses a chunk. CONFIRMED.
- Timing (c01): `18 80` at +1084, chunk 9 at +1103; `18 82` at +1108.
- **W75 content:** all `00` in both tables in every capture up to c08. CONFIRMED. Since the site's c09 Apply, profile 1's base table reads the site's forced knob defaults at 16-21 and `01 00 00 00` at 121 (section 4.10); profile 2's still reads all `00` (c10, c16 #886). After c17, OpenWraith's safety snapshot read profile 1's base table with the c16 bindings at 47/72/73 and the Fn table with the six c17 entries, each equal to the last site write byte for byte; profiles 2-4 read an all-zero Fn table (section 4.5.4). `00 00 00 00` means "no override" (the site writes it for unmapped keys, `agreement.js:1148-1151, 1221-1224`). That the firmware then uses its built-in default is inferred from the keyboard typing normally.
- Sub-commands other than `00`/`02`/`80`/`82` are never used by the site; whether `01`/`81` exists is unknown.

#### 4.8.2 Write `18 00` (base) / `18 02` (Fn layer) (CONFIRMED, both layers with real content; CAUTION)

- Always the whole 528-byte table: 10 reports `18 00|02 00 <chunk> <len>` + table bytes [chunk x 56, +len), len `0x38` x 9 then `0x18` (`setKeyValue` `agreement.js:1102-1187`, `setFnKeyValue` `1188-1262`). No wait between chunks in the site. CONFIRMED (c09, c13, c16, c17).
- **Ack:** `18 00 00 00 00` per chunk, for both layers (section 2.7). CONFIRMED. The site ignores `0x18` inputs other than `80`/`82`.
- **Read-back:** c10 and c11 read back the site's c09 table byte for byte, including its forced knob defaults at 16-21 and `01 00 00 00` at 121 (section 4.10). CONFIRMED. **Non-zero Fn layer (c17):** every `18 02` write of c17 carried non-zero entries; the final table (bursts 5 and 6, byte-identical) = idx 2 (F1) `07 12 00 00`, 3 (F2) `12 01 ff 00`, 4 (F3) `12 00 10 00`, 5 (F4) `07 25 00 00`, 46 (W) `07 32 00 00`, 66 (Caps) `13 e0 e1 04`, every other entry `00 00 00 00` (knob slots 16-19 and 121 included). The remaps were meant for the base layer, but the Fn layer was selected, so the base table stayed the same in all 7 bursts. OpenWraith's safety snapshot 19 s after the session read the Fn table back byte for byte, and the user reported that every assignment worked (Fn+W -> Calculator was an explicitly instructed test). CONFIRMED. Persistence of these site-written Fn entries across a profile switch was not observed (OpenWraith restored the backup afterwards).
- Sent only from the keymap "Apply" flow (section 6.7). Each Apply is a full 110-packet burst: c16 has 11 and c17 has 7 (770 packets), every packet acked (CONFIRMED). The catalogue picks ask for confirmation (string `applyTip2`) and click Apply on OK (`wmIndex.js:4563-4566`); the combination path has no confirm (`wmIndex.js:3502-3534, 4453-4474`), so the Caps burst and the byte-identical last burst of c17 most likely came from the Apply button (inferred).
- How the site fills entries it was not asked to change (verified, `agreement.js:1102-1160, 1218-1236`):
  - Base layer, no remap: `00 00 00 00`, except: the layout key `KeyFn` gets code1 = 1 (`01 <layout hid> 00 00`) unless the layout supplies its own code1; layout keys with an explicit code1 get `<code1> <hid> 00 00`; keys named M1-M4 take code1/hid/code3 from `keys.json`; knob virtual keys get their layout codes; then (W75 has a knob, PID != `0xC365`) **slots 16-21 are overwritten with knob defaults** (section 4.10).
  - Fn layer, no remap: `00 00 00 00`, except unmapped knob-region slots (layout codes), then slots 16-19 forced to knob defaults if the layout has knob regions. **On the W75 this never happens** (CONFIRMED): the W75 layout's `RotaryKnob` entry (index 15) has no `knobRegions`, so `getKnobSlotDefs` returns no slots (`wmIndex.js:3422-3436`), and the Fn writer adds knob defaults only if the device has a knob and the layout defines knob regions (`agreement.js:1227-1235`), which fails here, while the base writer needs only `hasRotaryKnob` and PID != `0xC365` (`agreement.js:1154-1160`). Every captured Fn write (c09, c13, c16, all 7 in c17) has `00 00 00 00` at 16-19 while every base write has the defaults at 16-21. A Fn read with 16-19 = 0 is normal.
- Resetting one key in the site removes its remap and runs the full Apply; the entry returns to `00 00 00 00`.
- **The base layer is saved only when the Fn layer is written after it** (CONFIRMED on hardware, 2026-10-24, OpenWraith restore test and two probes; each check = wait for the commit, select profile 2, select profile 1, read):
  - `18 00` alone (W -> A, and knob slot 16 -> `00`): acked and read back correctly at once, but **gone after the profile switch**, even with a 6 s wait before the switch. So this is not the ~1 s commit delay of section 4.5.3.
  - `18 00` followed by `18 02` (Fn table with one change at the unused index 130): **both** layers kept.
  - `18 02` alone (index 130 -> `00 04 00 00` and back): kept.
  - Interpretation (GUESS): the firmware saves the keymap pair (perhaps the whole keymap/macro/advanced block) when it receives `18 02`; the site never notices because every Apply writes macros, `18 00`, `18 02`, then `22` in that order. Whether macro slots (`19`) written without a following `18 02` are kept is untested.
  - **Advanced-key slots written after `18 02`** (OBSERVED, c16): in every site burst the `22` writes come last with nothing after them but heartbeats. In c16 burst B1 the TGL slot written 1-3 ms after `18 02` (#477-#487 then #489-#533) still read back after a profile 2 -> 1 round trip about 10 s later (#1320), so the site's order persists advanced keys; whether `18 02` or an idle commit saved it cannot be told. OpenWraith's restore after session 3 wrote both keymaps and three `22` clears in the site's order (base, Fn, then `22`; `plan_keymap_group` in `crates/openwraith-hid/src/restore.rs` puts any macro slots first) and they persisted after a profile switch (OpenWraith restore read-back, 2026-10-27): a second sample of "`22` after `18 02`". Untested: `22` written before `18 02` (OpenWraith's `fn_last` order, used only when neither keymap layer differs) and a `22` group with no `18 02` at all.
  - **Rule for our app:** whenever `18 00` is written, write `18 02` right after it (with its current or target content, even if unchanged), and keep the site's order for the whole group: macros, base, Fn, advanced keys. Verify persistence by reading after a profile switch.
  - OpenWraith also ends a group that writes macro or advanced-key slots while neither keymap layer differs with the unchanged `18 02` (a precaution, GUESS: those slots may depend on the same trigger; the order is then macros, advanced keys, Fn). Its restore never writes `18 00` without the `18 02` after it.
- **Our app (recommendation):** read the table, change the 4 bytes of the edited key, write all 10 chunks back. Keep `00 00 00 00` for unmapped positions (what the device returned), including 121 (Fn) and 15-21. Whether a single chunk may be written alone is unknown. Batch edits and write only the changed table(s) instead of the site's 110-packet burst per assignment, but never drop the `18 00` -> `18 02` pairing above.
- Example (base-layer W, index 46 -> A): offset 184 = chunk 3, payload offset 16 (report byte 21). Send `18 00 00 03 38` + 56 bytes with `00 04 00 00` at payload 16..19, and all other chunks unchanged (chunk 9 header `18 00 00 09 18`).

#### 4.8.3 Entry encoding (4 bytes `b0 b1 b2 b3`; CONFIRMED for `00`, `07` (indices 12, 32), `10`, `12` (wheel down, mask `10`), `13` (two modifiers) and `17`; the rest CODE-ONLY)

`keys.json` values are hex strings parsed base 16 (`wmIndex.js:4230-4233`); gamepad codes in `gamePadKey.js` are decimal. Session 3 evidence: c16 (`17`, typing test) and c17 (`07`, `12`, `13` on the Fn layer, written, read back by OpenWraith and reported working by the user; only Fn+W was an explicitly instructed test, the others rely on "everything I assigned works" and on the user having held Fn).

| b0 | Category | b1 | b2 | b3 |
|---|---|---|---|---|
| `00` | Keyboard key: USB HID usage (page 0x07). `00 00 00 00` = no override (CONFIRMED) | usage | 00 | 00 |
| `01` | Fn (only written for the `KeyFn` position) | layout hid (`0xFA` in other layouts; the site labels `00 FA` as "Fn" on read) | 00 | 00 |
| `02` | Lighting: `02` = All Lighting (cycle effects) | sub | 00 | 00 |
| `03` | Brightness: `00` +, `01` - | sub | 00 | 00 |
| `04` | Effect speed: `00` +, `01` - | sub | 00 | 00 |
| `05` | Direction: `02` = cycle | sub | 00 | 00 |
| `06` | Colour: `00` full colour, `01` foreground cycle, `02` background cycle | sub | 00 | 00 |
| `07` | Multimedia (vendor index, table below). CONFIRMED for `32` Calculator (Fn+W, instructed test) and `12` Mute (blanket user report); `25` only as written bytes (c17) | index | 00 | 00 |
| `10` | Macro | the physical key's own usage (ignored on read) | slot 0..9 (reader masks `& 0x1F`) | 00 |
| `11` | Function: `00` lock keyboard, `02` lock Win, `04` all lights off | sub | 00 | 00 |
| `12` | Mouse, kind first (CONFIRMED, c17: wheel down `12 01 ff 00`, the site's "Mouse Back" `12 00 10 00`) | kind: `00` button, `01` wheel | button mask `01` L, `02` R, `04` M, `08` Forward, `10` Back (the site's labels; direction unverified, see below); wheel `01` up, `FF` down | 00 |
| `13` | Key combination (UI: 1-2 modifiers + exactly 1 key). CONFIRMED for two modifiers: Ctrl+Shift+A = `13 e0 e1 04` (c17) | usage 1 (modifier E0-E7) | usage 2 (second modifier, or the key) | usage 3 (the key) or 00 |
| `17` | Advanced key (section 4.14). CONFIRMED (c16: E `17 00 00 00`, G `17 01 01 00`, H `17 02 02 00`, typing test) | slot | slot | 00 |
| `18` | Gamepad "Classical" (shown only on PID `0xC367`) | control 0..32 | 00 | 00 |
| `19` | Gamepad Xbox 360 (shown only on PID `0xC366`) | control 0..24 | 00 | 00 |

- Mouse on the wire: the site's model holds (code1 `0x12`, hidCode = button, code3 = kind) and writes `12 <code3> <hidCode> <code4>`, i.e. kind first, swapping only for code1 `0x12` (`agreement.js:1209-1213`; the reader swaps back, `399-417`). Use `12 00 <button> 00` / `12 01 <dir> 00`. CONFIRMED by c17 (if the firmware read the swapped form, Fn+F3 would do nothing and Fn+F2 would left-click). **Which direction mask `10` produces is not verified:** "Back" is only the site's label (`keys.json` MouseBack hidCode `10`, MouseForward `8`). In the usual HID mouse button order `08` = button 4 (Back on Windows) and `10` = button 5 (Forward), so the site's labels may be reversed. Label `08`/`10` by value until tested. Wheel up and L/R/M buttons follow the same layout but are untested.
- Key combination on the wire: the site builds code1 `0x13` with hidCode/code3/code4 = the picked keys in pick order (`wmIndex.js:3519-3524, 4459-4465`) and writes them unchanged (`agreement.js:1214-1219`). The UI accepts at most 2 modifiers (usage E0-E7, `isCtrKey` `wmIndex.js:3676-3678`) first, then exactly one non-modifier; a modifier picked after the key restarts the list (`wmIndex.js:3563-3577`). So one modifier gives `13 <mod> <key> 00` (CODE-ONLY). Whether the firmware accepts a non-modifier in b1/b2 or other orders is untested; keep the site's form.
- Predefined combinations (`keys.json`): Win+D `13 E3 07`, Alt+F4 `13 E2 3D`, Win+L `13 E3 0F`, Ctrl+Z `13 E0 1D`, Ctrl+C `13 E0 06`, Ctrl+V `13 E0 19`, Ctrl+X `13 E0 1B`, Ctrl+S `13 E0 16`, Ctrl+N `13 E0 11`, Ctrl+P `13 E0 13` (b3 = 00).
- Multimedia indices (b0 = `07`): `10` Vol+, `11` Vol-, `12` Mute, `13` Play/Pause, `14` Next, `15` Previous, `16` Stop, `20` Mail, `21` Browser Search, `22` Favorites, `23` Browser Home, `24` code name BrowserForward but Chinese label "previous page" (contradictory), `25` BrowserBack / "next page" (contradictory), `26` Browser Stop, `27` Refresh, `30` Media player, `32` Calculator, `34` My Computer.
  - `24` / `25` labels (OBSERVED, c17): the UI text comes from `language.json` by code name. The item with code BrowserBack (writes `07 25`) is labelled "Browser Forward" in en, and its tr, zh-cn, kr, jp, pt-br, ar and es labels also say forward, but "back" in ru, de, pl and fr; BrowserForward (`07 24`) is labelled "Browser Back" in en, and its tr label also says back. In c17 the user picked a browser forward/back item for Fn+F4 and the site wrote `07 25 00 00` (#1086); which label was clicked and which way the browser navigated are not recorded. GUESS: `25` = AC Forward, because indices 21, 23 and 24-27 line up with the low byte of HID Consumer usages 0x221-0x227 (20 Mail and 22 Favorites break the pattern). Label `07 24` / `07 25` by value with a neutral note until tested; never derive the label from the site's code name.
- HID usages offered by the site (b0 = `00`): `04`-`1D` A-Z, `1E`-`27` 1-0, `28`-`2C` Enter Esc Backspace Tab Space, `2D`-`31` `- = [ ] \`, `33`-`38` `; ' grave , . /` (`32` not offered), `39` Caps Lock, `3A`-`45` F1-F12, `46`-`4E` PrtSc ScrLk Pause Ins Home PgUp Del End PgDn, `4F`-`52` Right Left Down Up, `53`-`63` Num Lock and keypad, `64` Intl backslash, `65` Menu, `68`-`73` F13-F24, `87`-`8B` JIS keys, `E0`-`E7` LCtrl LShift LAlt LGui RCtrl RShift RAlt RGui.
- Gamepad controls (GUESS that they apply to the W75): Classical 0-15 axes, 16 trigger, 17-28 buttons 1-12, 29-32 hat; Xbox 0-7 sticks, 8/9 LT/RT, 10-13 A/B/X/Y, 14 start, 15 select, 16 menu, 17-20 d-pad, 21/22 L3/R3, 23/24 LB/RB.
- Advanced-key bindings: the list-click binding builds code1 `0x17` (23), hidCode = slot, code3 = slot (`wmIndex.js:4124-4134`), written as `17 <slot> <slot> 00` (`agreement.js:1141-1146`); the reader finds the slot by b1 (`agreement.js:381-398`). `17 00 00 00` is a real binding to slot 0, not "empty": it is non-zero only in b0. The binding goes only into the layer selected in the UI (c16 Fn tables stayed all zero). Fn-layer bindings are untested.
- How the site decodes a read (`agreement.js:340-476`): `10` = macro (slot from b2, falling back to matching a macro ID); other entries are decoded only when the physical key's layout code is in `keys.json` **and** the bytes differ from that catalogue entry, so a non-macro entry at the Fn position is never decoded. Entries the site cannot map are dropped from its model and lost on the next Apply.

#### 4.8.4 Fn layer

- `18 82`/`18 02` hold overrides active while Fn is held; same entry format (CONFIRMED, c17: the same 4-byte encoding as the base layer, see 4.8.2). Zero entries leave the firmware's built-in Fn functions in place. The site cannot read the built-ins; it only labels them from `fnKeys.json`.
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

- **Override of built-ins (OBSERVED, partly answers Q14):** c17 put Fn-layer entries on F1-F4 (built-ins: media player, volume -, volume +, mute per the first `fnKeysConfig` entry of `fnKeys.json`), W and Caps (no built-in); the base table stayed `00 00 00 00` at 2, 3, 4, 5, 46 and 66 in all 7 bursts, so any changed effect can only come from the Fn layer. The user reported that every assignment works. So, most likely, a non-zero Fn-layer entry takes effect on Fn+key and replaces the built-in at F1-F4. Caveats: the test prompt asked for F1 **without** Fn (the remaps were meant for the base layer) and the user gave no per-key result, so this rests on a blanket report and on the inference that Fn was held; Fn+W, the one explicitly instructed Fn test, has no built-in and shows only that Fn-layer entries work. Fn+1..4 (profiles), Fn+Space (reset), Fn+Backspace and the lighting keys are untested; do not write Fn-layer entries there until a test shows what happens. Our app's Fn-layer editor should warn that an entry hides the built-in function.
- **Fn key entry (index 121, base layer):** with the site's `01 00 00 00` at 121 (the session-3 connect read c14 #115.. was all zero; every c16/c17 Apply wrote `01 00 00 00`, read back in c16 #1372 and by OpenWraith after c17) the Fn key works as Fn (Fn+W -> Calculator worked); with the factory `00 00 00 00` Fn also worked (c07 Fn+Space reset, all-zero tables). Both encodings keep Fn working (CONFIRMED); `01 FA 00 00` is untested. This does not show that the firmware reads entry 121 at all. Keep the device's value.

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
| 1 | 1 | play mode: 0 once, 1 N times (uses count), 2 toggle (one press starts, the next press stops), 3 repeat while held |
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
| 16 | `07 12 00 00` | Mute | if knob and PID != `0xC365` | if knob and layout has knob regions (never on the W75) | press |
| 17 | `07 11 00 00` | Volume - | same | same | rotate left |
| 18 | `07 10 00 00` | Volume + | same | same | rotate right |
| 19 | `02 02 00 00` | All Lighting | same | same | ? |
| 20 | `03 01 00 00` | Brightness - | same | not written | ? |
| 21 | `03 00 00 00` | Brightness + | same | not written | ? |

- These overwrites replace any user remap of those slots on every Apply. **The factory W75 tables are all zero at 15-21** (CONFIRMED), so the firmware's knob behaviour does not depend on these entries being set. All 11 base writes of c16 and all 7 of c17 carry the base defaults (c16 #3994: chunk 1 payload 8-31) and `01 00 00 00` at 121; profile 1 reads them back (c16 #1371), profile 2's base table reads all zero (c16 #886).
- **The W75 layout has no knob regions** (CONFIRMED): its `RotaryKnob` entry (index 15, hidCode 0) has no `knobRegions` field, so `getKnobSlotDefs` returns an empty list (`wmIndex.js:3422-3436`) while `hasRotaryKnob` is true (`wmIndex.js:6174-6177`). The Fn column above therefore never applies on the W75 (every captured Fn write is zero at 16-19, section 4.8.2), and the site offers no knob-region remapping on the W75: the knob part of capture plan C16 cannot be done in the site. Which keymap slots the knob uses, if any, needs an app test (open question Q7).
- The `0x18` read parser maps non-layout slots only if they are listed as knob regions (`agreement.js:339-346`).
- **Our app:** keep the device's values for 15-21 unless the user explicitly remaps the knob.

### 4.11 Actuation and Rapid Trigger (0x21)

**Header:** `21 00 00 00 <len>`; byte 5 selects the function. Most requests use the constant `0x18` in byte 4 regardless of their real size; poll-rate read uses 1, poll-rate write 0, profile read/select 0. Send the same bytes as the site.

| Byte 5 | Dir | Function | Confidence |
|---|---|---|---|
| `00`, `0C`, `0D` | W | trigger write; byte 5 = mode (4.11.3) | CONFIRMED (`0D` in c10) |
| `00` | IN | trigger write ack `21 00 00 00 01 00` (also acks the travel-test open and close) | OBSERVED |
| `01` | IN | live travel stream | CONFIRMED (c14) |
| `02` / `03` | W | open / close travel test | CONFIRMED (c14) |
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

#### 4.11.4 Empty-mask write after every `0x11` report (CONFIRMED; do not copy)

After each `0x11` report the site fires its RT checkbox handler with no keys selected (`agreement.js:641-643` -> `wmIndex.js:1902-1913`), sending mode 0, all-zero mask, travel and sensitivity 0. This happens at connect (c01 +3394, c03 +2835, c14 +3100) and after every site-initiated profile switch (c08 #1137, #1624; c10 #607, #1108; c16 #908, #1393). The device acks `21 00 00 00 01 00`; the next trigger pass is unchanged. With nothing to apply, the ack says nothing about range checks. OpenWraith's foreign-ack detector (section 2.7) sees this ack whenever the vendor site connects or follows a profile change; OpenWraith must never send this write itself.

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
- **Site toggle (do not copy):** `resetTrigger` checks a flag `isResetTrigger` (initially true, `variable.js:361`). Any incoming `0x14` report clears it (`agreement.js:320-321`); a call with the flag clear only sets it again and sends nothing (`agreement.js:942-944`). So after any `0x14` reply (a Win-Lock change, a previous reset) the next trigger reset is silently skipped. In the full reset the check runs right after the `0x14` reset is sent, so that reset's own reply most likely arrives too late to matter (one sample: in c07 the reply #1017 came about 820 ms after the write #991, section 2.7; the keyboard's own reset notice #990 had already cleared the flag, and no `21 .. 18 06` was sent).

#### 4.11.7 Travel test (CONFIRMED, c14)

Captured in c14 (capture plan C12, part; c14 #896 +43273 .. #1344 +138233): the test was opened and closed once; the user pressed W slowly to the bottom, held it and released it slowly, then pressed Esc, Space and Right once each, then W and A together. 253 stream reports in total.

**Requests**

- **Open:** `21 00 00 00 18 02` + 22-byte key mask at bytes 6-27, bytes 28-62 `00` (c14 #981 +85986). The site builds the mask from every layout key (for each key it sets bit row of byte column; `openTriggerTest`, `agreement.js:955-972`), so the W75 sends the 82-position mask of section 2.9, knob included (byte 21 = `2f`, not the Select-All `2e`). Sent only from the Travel Test switch's `input` handler (`wmIndex.js:811-818`, open at 814); nothing else is sent around it. CONFIRMED.
- **Close:** `21 00 00 00 18 03` + zeros (`closeTriggerTest`, `agreement.js:974-984`; c14 #1312 +123439). CONFIRMED. Sent from the same handler when the switch turns off (`wmIndex.js:816`). The site also turns the switch off itself when leaving the Actuation tab while the test is on (`wmIndex.js:1187-1190`), **every time the Calibration sub-tab is selected**, even if the test was never on (`wmIndex.js:742-746`; not exercised in c14), and on a profile click in software-profile mode (`wmIndex.js:1101-1103`, not reached on the W75).
- **Ack:** open and close are each acked within 1 ms by `21 00 00 00 01 00` + 57 zero bytes, the trigger-write ack (#982, #1313; section 2.7). OBSERVED.
- **After the close** no stream report appears again: c14 #1314-#1387 hold only 36 heartbeat pairs and 2 MARKs, and c15-c17 (same session, keys pressed) contain none; neither do c01-c13. CONFIRMED. Whether the device would also stop by itself (timeout, disconnect, sleep) is untested; the stream was still live 34 s after the open (#1305 +120127). GUESS: the test is a mode that must be closed explicitly.

**Stream reports** (input `0x21`, byte 5 = `01`). Each report carries exactly one key; bytes 1-3 are `00 00 00` (no chunk index) in all 253.

- **Event-driven** (OBSERVED): a report is sent only when a key's depth changes. Nothing is sent at rest or while a key is held still: 7.4 s passed between the open and the first W movement (#999 +93370), nothing came during the 5.7 s bottom hold (#1065 +98682 -> #1078 +104414) or between the separate key tests (e.g. #1135 +108399 -> #1144 +112733); only heartbeats were on the wire. There is no snapshot of all keys when the test opens.
- **Two variants**, told apart by byte 4 (OBSERVED): 147 short (`05`) and 106 long (`03`).
  - Short: `21 00 00 00 05 01 RR CC DL DH` + 53 zero bytes; length 5 exactly covers bytes 5-9. Example #1157 `21 00 00 00 05 01 00 00 a9 00` (Esc at 169).
  - Long: declares length 3, but live data runs to byte 36 (the exception of section 2.3); a parser that honours byte 4 would lose the depth. Example #999 `21 00 00 00 03 01 02 02 11 00 ff ff 9b 09 ad 09 01 11 00 82 09 82 09 00 00 00 00 00 00 00 00 87 06 ff ff 00 00` + zeros (W at 17).

| Byte | Content | Variant | Confidence |
|---|---|---|---|
| 4 | `05` short / `03` long (not a length in the long one) | both | OBSERVED |
| 5 | `01` | both | CONFIRMED |
| 6 | row of the key that moved | both | OBSERVED |
| 7 | column of the key that moved | both | OBSERVED |
| 8, 9 | depth lo, hi in trigger units, 0 at rest | both | CONFIRMED (value, units); hi byte CODE-ONLY |
| 10-11 | `ff ff` (106/106) | long | OBSERVED |
| 12-13, 14-15 | per-key constants, LE16 (W 2459 / 2477, Esc 2390 / 2394, Right 2371 / 2377, A 2393 / 2412) | long | OBSERVED; meaning GUESS |
| 16 | motion: `01` deeper than this key's previous report (61/61, plus W's first report #999, which has no predecessor), `02` shallower (40/40), `00` exactly at depth 0 (4/4); flips with finger wobble | long (`00` in the short one) | OBSERVED |
| 17-18 | copy of bytes 8-9 (106/106) | long | OBSERVED |
| 19-20, 21-22 | two LE16 sensor readings, falling as the key goes down | long | OBSERVED |
| 23-30 | `00` | long | OBSERVED |
| 31-32 | per-key constant, LE16 (W 1671, Esc 1492, Right 1542, A 1647) | long | OBSERVED; meaning GUESS |
| 33-34 | `ff ff` | long | OBSERVED |
| 35-36 | a word that changes over time and is not tied to the key (`00 00` #999-#1065, `97 0a` #1082-#1135, `0b 00` #1174, `33 00` #1227, `0b 00` #1302/#1305); meaning unknown | long | OBSERVED |
| 37-62 | `00` | long | OBSERVED |

- **Row and column** (OBSERVED; the site evaluates bytes 6-7 and discards them, `agreement.js:489-490`): index = row x 22 + col. The keys appeared in the order the user pressed them: W `02 02` (#999), Esc `00 00` (#1144), Space `05 06` (#1179), Right `05 0f` (#1213), A `03 02` (#1249). Swapping row and column is impossible for Space and Right and would give E for A. This resolves the earlier GUESS and confirms the section 5 positions of Esc, A, Space and Right.
- **Depth** (CONFIRMED for value and units): trigger units (W75 1 unit = 0.02 mm, from `21/04`), 0 at rest. Full-press peaks: W 170 (#1065, and #1274 with A), A 170 (#1264), Esc 169, Right 169, Space 164 (3.28 mm; GUESS: limited by its stabiliser). 170 = `aa` = the `21/04` maximum travel (c14 #349 `21 00 00 00 06 04 aa 02 02 00 05`). No report exceeds 170, so byte 9 was `00` in 253/253; that it is the high byte rests on the site code only (CODE-ONLY on the W75). Depth in mm = value x unit.
- **Sensor readings** (OBSERVED): LE16, proven by the byte-boundary crossing #1048 `0c 08` (2060, depth 127) -> #1049 `ff 07` (2047, depth 128). W reads 2454-2458 at rest, 2434 at depth 17 and 1678 at depth 170. The two values are equal in 104/106 reports and differ only in the depth-0 reports #1135 (2456 / 2454) and #1305 (2393 / 2380). Depth is a non-linear function of the reading: about 2 counts per unit near the top, about 5-12 near the bottom; monotonic to within 1 unit, and the same reading maps to depths at most 1 unit apart on press and release (2047: 128 on the press, 127 on the release #1102).
- **Calibration GUESS:** bytes 12-13 lie within 0-3 counts of the key's rest reading and bytes 31-32 just below the deepest reading seen (W 1678 at 170, Right 1557 at 169); normalising (top - reading) / (top - bottom) with them gives 0.977 for W and 0.982 for Right, both at depth 169. GUESS: 12-13 = calibrated top, 31-32 = calibrated bottom, 14-15 = a second top or a noise bound; one of the two readings is filtered and the other raw. Do not base features on these fields; a capture around a calibration (C20) or of more keys would test the GUESS.
- **Rate, slow motion** (OBSERVED): long reports on a ~72 ms tick: 80 of the 99 same-key long-to-long gaps were 72 ms, most others multiples of it (144, 216, 288, 432 ms) or re-phasings (#1005 +88, #1064 +41). Steps of 1-6 units. The slow W press (5.3 s down, 5.7 s held, 4.0 s up; the user had estimated about 2 s and 1 s) produced 57 long reports from 17 to 170 and no short one; the slow release produced 45 long and 5 short among 50. So a slow or partial press gives at most about 14 updates per second, with up to about 72 ms extra latency.
- **Rate, fast motion** (OBSERVED): short reports in bursts. Same-key short-to-short gaps within bursts: median 1-3 ms per key (W 2, Esc 1, Space 2, Right 3, A 2), minimum 0, per-key maximum 4-8 ms (apart from W's slow-release gaps and one 48 ms Esc turn-around). Each short report is at least 3 units from that key's previous report (median 8). A tap gives 30-31 reports (Esc 31, Space 30, Right 30): about 50-93 ms of motion, 98-242 ms from first to last report including the dwell at the bottom. Example: Esc #1144-#1157, 14 reports in 10 ms, depths 31, 53, 70, 84, 99, 109, 119, 129, 136, 143, 150, 157, 164, 169. GUESS: non-zero reports switch to the short variant above a speed somewhere between about 0.08 and 0.28 units/ms; depth-0 reports do not follow this rule (4 of 6 came long despite high speed).
- **First report** (OBSERVED): the slow W press first appeared at depth 17 (0.34 mm) with nothing below it; fast presses first appeared at 22-54. A view may not show the top ~0.3 mm of a slow press.
- **End of motion** (OBSERVED): every release ended with a depth-0 report for that key (6/6: #1135 W, #1174 Esc, #1302 W and #1305 A came long, 4-31 ms after the previous report and so off the 72 ms grid; #1208 Space and #1242 Right came short). A small settle after a fast press is caught by a long report up to 72 ms later (Right #1226 short 168 at +116712, #1227 long 169 at +116784).
- **Two keys at once** (OBSERVED, #1249-#1305): the reports of W and A interleave in one stream in order of change, one key per report; there is no combined report. Each key keeps its own rate (median short-to-short gap 2 ms for both, maximum 5). 26 reports in 41 ms on the press, 29 in 48 ms on the release; same-key runs of 1-5, plus one run of 19 W reports while only W was still moving.
- Request/reply traffic is unaffected while the test is open (section 2.5).

**How the site uses it** (CONFIRMED, `agreement.js:488-495`): only bytes 8-9, drawn as one bar (`testjindu`) of height depth / max travel x (max travel x unit x 100 x 0.62) px = depth in mm x 62 px (W75 full press 210.8 px). Byte 4, bytes 6-7 and the long-variant fields are unused, so their meaning cannot be learned from the site code; the site's single bar flickers between keys when two move.

**For our app** (recommendation; GUESS where marked):

- A live per-key depth view is feasible and cheap (GUESS, derived from the observations above): zero traffic at rest; fast taps update every 0.5-3 ms in steps of at least 3 units (0.06 mm); slow movement updates about every 72 ms in 1-6 unit steps; every release ends at 0. Open with the 82-position mask, keep per-key state starting at depth 0, update it per report and coalesce to the latest value per key per animation frame (smooth for fast motion, visibly steppy for slow presses). Decode the long variant without truncating at byte 4. Close the test when leaving the view, on exit and on error paths.
- Drain input continuously while the test is open: one tap is about 30 reports. Windows queues 32 input reports per handle by default (`HidD_SetNumInputBuffers`, maximum 512); hidapi 2.6.7, which OpenWraith uses, sets 64 on open. A stalled reader can still lose reports, including the final depth-0 one.
- Stream reports seen while our own test is closed mean another program has a travel test open (section 2.7).
- **Not a basis for software analog or gamepad emulation** (GUESS): in the slow, partial-press regime where analog control matters the device reports only on the ~72 ms tick; fast motion arrives quickly but quantised to at least 0.06 mm; the resolution is 0-170 units (about 7.4 bits); every program with the vendor interface open receives the stream; its persistence across disconnect or sleep and any effect on normal key output are unknown. Real analog output belongs to the firmware gamepad mode (section 4.6.4, DANGEROUS, CODE-ONLY). Present the travel test as a visualiser and diagnostic tool only.
- Whether a mask with fewer keys limits the streamed keys is untested (Q33).

#### 4.11.8 Calibration (**DANGEROUS**, CODE-ONLY)

Requests `21 00 00 00 18 <sub> <arg>` (`agreement.js:1884-1895`; UI `wmIndex.js:1946-1998`):

| Device | Start | Stop |
|---|---|---|
| Any-key calibration flag set (**W75**, flags A bit 0) | `0F 00` | `10 00` |
| Otherwise | `08 00` | `08 01` |

- Single key: `21 .. 18 07 <row> <col>` exists but nothing calls it.
- Procedure (UI text): Start; press each key to the bottom; wait for its colour to change; Stop. The button is disabled 2 s after each click. The site sends Stop automatically when the user leaves the Calibration sub-tab or the Actuation tab.
- Status input, byte 5 = `08` or `0F` (`agreement.js:526-568`): byte 6 = 0 finished/stopped; 1 progress, bytes 7-28 = 22-byte mask of keys calibrated so far; 2 or 4 failed. No handler for byte 5 = `10`. Nothing captured yet.
- Possible diagnostics (GUESS): the long variant of the travel stream carries per-key constants that may be the calibrated top and bottom sensor readings (section 4.11.7). A travel-test capture before and after a calibration would test this.

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

- **Read** (CONFIRMED framing): `22 80 00 00 00` -> 40 chunks, bytes 2-3 = slot (BE16), byte 4 = `0x28` (40), byte 5 = type. W75: all 40 slots all `00` (c01 +1008). One read = 1600 bytes, streamed at about 2 ms per chunk (c16 #833-#873, 78 ms).
  - **Read-back of a non-empty slot** (CONFIRMED for TGL, c16 #1320): `22 80 00 00 28 03 00 0b 00 00` + zeros; read payload bytes 5-44 are byte-identical to write bytes 5-44 (the parser reads the slot from bytes 2-3, `agreement.js:657`, and TGL at `725-739`). So read chunk k = slot k, payload byte 0 = type, then the write offsets. A DKS needs write bytes 5-41, which fit in 40. OpenWraith's snapshot after c17 read slots 0-2 (DKS, MT, TGL) equal to the first 40 bytes of the site's writes (write bytes 45-54 are zero). A DKS/MT read-back has not been captured from the site.
- **Write** (CAUTION): one packet per slot, `22 <slot> 00 00 32 <type> ...` (byte 4 = 50). On every keymap Apply the site writes **all 40 slots**; empty slots go out as zeros, clearing them (c16 B2 cleared slot 0 this way). Each write is acked `22 00 00 00 00`, empty or not (section 2.7); the site parses no reply. CONFIRMED with all-zero content (c09, c13: 240 writes) and with non-empty DKS, MT and TGL slots (c16: 440 writes, 20 non-empty; c17: 280 writes, 21 non-empty, each with a typing test in c16). Read 40 B / write 50 B: the sizes are asymmetric. OpenWraith so far only ever writes all-zero slots (clearing a slot).
- 40 slots; at most 40 entries, at most 5 MDS.
- **Slot allocation** (OBSERVED, c16; `getAdvIndex` `wmIndex.js:3790-3803`, called when a type tab opens, `2029, 2073, 2096`): a new advanced key gets the lowest free slot. First attempt: TGL in slot 0; after it was cleared, DKS 0, MT 1, TGL 2. Slots are dense from 0, and a slot's number is independent of the bound key. Our app should use the same policy so that a later site Apply does not shuffle the table.
- **One dialog = one advanced key** (GUESS from c16's first attempt): the user configured DKS, MT and TGL tabs in one dialog and pressed OK; only TGL H went out (B1, slot 0), because the type radios share one name (`advTypeRadio`) and OK keeps only the selected tab's key. The dialog OK itself fires the full Apply (`wmIndex.js:2375`).
- **Per profile; unbound slots are kept** (OBSERVED, c16, section 4.5.4): the unbound TGL slot of B1 survived a profile 2 -> 1 round trip while profile 2 read 40 empty slots. The UI must therefore show slots with no binding ("orphans"); the site's list delete is its only path that clears them.
- **No HID traffic when advanced keys fire** (OBSERVED, c16 #4337-#4544): during the typing test only heartbeats were on the vendor collection, so the vendor interface cannot show live advanced-key state (e.g. TGL on/off).

| Type (byte 5) | Name | Offered on W75 |
|---|---|---|
| 0 | empty | |
| 1 | DKS (dynamic keystroke) | yes |
| 2 | MT (tap / hold) | yes |
| 3 | TGL (toggle) | yes |
| 4 | RS (deeper of two keys wins) | no (flag B bit 3 = 0) |
| 5 | RKRT (send another key on release) | no |
| 6 | MDS (delay, then up to 6 keys) | no |

**Action code:** 4 bytes in keymap format (code1, hid, code3, code4); code1 0 = keyboard key (CONFIRMED: E `00 08 00 00`, R `00 15 00 00`, L-Ctrl `00 e0 00 00`, G `00 0a 00 00`, H `00 0b 00 00`, c16). Mouse (code1 `0x12`) is written kind-first (`12 <code3> <hid> <code4>`) only in DKS actions and keymap entries; MT/TGL/RKRT write it unswapped; the site's DKS reader does not swap back (site bug). Moot on W75 (DKS mouse flag 0). Our app should use one canonical form `12 <kind> <value> 00`.

**DKS (type 1)** (CONFIRMED, c16; writer `agreement.js:1945-1966`, UI `wmIndex.js:3725-3779`):

| Byte | Field |
|---|---|
| 6 | route1: first press point, **0.1 mm** units; UI default 15 (1.5 mm), range 1..route2 |
| 7 | route2: deep press point = max travel x 10 - 4 = 30 (3.0 mm) on the W75; not editable |
| 8 | route3: release-from-bottom point; always = route2 |
| 9 | route4: final release point; the UI forces it = route1 |
| 10+8k .. 13+8k | action for key slot k = 0..3 (keymap format) |
| 14+8k .. 17+8k | step byte for slot k at stages 0..3 |

- **Captured** (c16 #2302, +389858, byte-identical in bursts B4-B11 and in all 7 c17 bursts): slot 0 = `22 00 00 00 32 01 0f 1e 1e 0f | 00 08 00 00 | 01 00 00 00 | 00 15 00 00 | 00 01 00 00` + zeros. Row 0 = E with steps `01 00 00 00` (tap at stage 0), row 1 = R with steps `00 01 00 00` (tap at stage 1), rows 2-3 zero. Bound to E (index 47). Typing test: a light press on E typed "e", a full press "er". The byte offsets match the table exactly.
- **Stages:** wire stage 0 = the first press point (route1, UI column 1 with arrow `down1`), wire stage 1 = bottom-out (route2, column 2, `down2`) (`v1.html:391-402`). The UI's "stage 1" / "stage 2" columns are wire indices 0 / 1. A UI dot (a 20 px label, `wmIndex.js:3739-3741`) is step value 1 = a single tap. CONFIRMED for value 1 at stages 0 and 1; values 2/3 and stages 2/3 are untested.
- **Routes** (OBSERVED): written `0f 1e 1e 0f` (15, 30, 30, 15) with the UI showing 1.5 mm (default) and 3.0 mm. R2 = R3 = 170 x 0.02 mm x 10 - 4 = 30 (`wmIndex.js:4161-4170`; the W75 is not the special-cased product); R4 is forced equal to R1 (`wmIndex.js:2268-2282`); the tab labels show route / 10 in mm (`wmIndex.js:2032-2040`). The values match 0.1 mm units, and the user's distinct light ("e") and full ("er") presses fit 1.5 / 3.0 mm thresholds (in 0.02 mm units they would be 0.30 / 0.60 mm, which a deliberate light press could hardly separate). The unit is not proven; a test with route1 = 5 versus 25 would settle it.

Step values the UI produces: 0 nothing, 1 single tap, 2 held through this stage, 3 new hold starting right after a held stage. Stage 3 only gets 0 or 1. Firmware: 1 = tap CONFIRMED (stages 0, 1); GUESS for the rest: 2 hold, 3 release then press again.

**MT (type 2)** (CONFIRMED, c16; writer `agreement.js:1968-1977`): bytes 6-9 = **hold** action, 10-13 = **tap** action, 14 = hold threshold in 10 ms units (UI slider 15-100 = 150-1000 ms, default 15; `wmIndex.js:2205-2213`, `v1.html:384`). The UI's tap key (`mtEdit1`) is the first entry of the slot's key list and the hold key (`mtEdit2`) the second (`v1.html:373-378`, `wmIndex.js:2067-2068`), and the writer puts the second (hold) entry first. Captured slot 1 (c16 #3072, +443286, identical in later bursts and in c17): `22 01 00 00 32 02 00 e0 00 00 00 0a 00 00 1e` = hold L-Ctrl, tap G, `1e` = 30 = 300 ms as set in the UI. Bound to G (index 72). Tapping G typed "g": the tap field is verified by effect. The hold field (L-Ctrl after more than 300 ms) and the firmware's timing are only seen on the wire, not tested.

**TGL (type 3)** (CONFIRMED, c16; writer `agreement.js:1979-1984`): bytes 6-9 = action. Captured `22 02 00 00 32 03 00 0b 00 00` (slot 2, H; c16 #3563 and later) and, in the first attempt, `22 00 00 00 32 03 00 0b 00 00` (slot 0, #489). Bound to H (index 73), H toggled into a held "hhhh" (auto-repeat while toggled on). Whether a second press releases it was not reported.

**RS (type 4), RKRT (type 5), MDS (type 6)** (CODE-ONLY, not offered on the W75): RS bytes 6-9 key A code, 10/11 A row/col, 12-15 key B code, 16/17 B row/col. RKRT bytes 6-9 the bound key, 12-15 the key sent on release. MDS bytes 6-7 delay ms BE16, 8 mode (0 after delay, 1 after all keys), 9 key count <= 6, 10-15 HID usages.

**Binding to a key** (CONFIRMED for DKS/MT/TGL, c16): DKS/MT/TGL/RKRT (and RS bound from the list) set the key's `0x18` entry to `17 <slot> <slot> 00`. Captured in the base table (c16 #3998, #4002, burst B10, and B11): E (47, chunk 3 payload 20) `17 00 00 00`, G (72, chunk 5 payload 8) `17 01 01 00`, H (73, chunk 5 payload 12) `17 02 02 00`; the Fn table stayed all zero (section 4.8.3). Bind with a read-modify-write of the base table followed by the mandatory `18 02` (section 4.8.2). RS created from its dialog binds both member keys with `17 00 00 00` (the firmware presumably finds them by row/col; CODE-ONLY). MDS is never bound.

**Apply bursts** (CONFIRMED, c16; `wmIndex.js:1530-1551`): the advanced dialog's OK (`wmIndex.js:2375`), the confirm after a list-click binding (`4137-4139`), a key reset (`1506-1527`) and a list delete (`4075`) all fire the same full Apply: `19 00`-`09` x 5, `18 00` x 10, `18 02` x 10, `22 00`-`27` x 40 = 110 packets, with nothing after them but heartbeats (B1 is followed about 10 s later by the user's profile round trip). The button is re-enabled 300 ms after each Apply (`1547`). c16 has 11 bursts (#359, #1575, #2163, #2443, #2675, #2929, #3155, #3421, #3647, #3889, #4113); each differs from the previous one only in the `22` slot or base chunks 3/5 that the user changed, and B11 is byte-identical to B10. So every non-trivial save in the advanced UI costs a 110-packet whole-table rewrite. Our app should write only the changed `22` slot(s) plus a read-modify-write of `18 00` and the `18 02` commit; that a single-slot `22` write leaves the other 39 slots untouched is a GUESS (each slot is its own packet), to be verified with a read-back.

### 4.15 SOCD (0x24)

The site calls it "PRCS"; the UI tab is "SOCD". Code `agreement.js:2029-2106`, parser `800-841`.

Captured in c15 (capture plan C17, profile 1): pair A/D with the first model option, Apply; SOCD switch on; Fn + R-Shift pressed twice on the keyboard; the pair changed to the second model option, Apply; the pair deleted; SOCD off. 12 write bursts (B01-B12).

- **Power and hotkey read** (CONFIRMED): `24 02` -> `24 02 00 00 06 | 00 00 00 00 fa e5` (c01 +1080; every `24 02` read in c01-c16 is this value).
  - p[0] = power (1 = on); W75: **off**. Also confirmed in the keyboard's push (below).
  - p[1..3] (OBSERVED): `00` in every read and in both pushes, with power `00` and `01`, so they do not hold the power state. They also do not hold a 1-based pair count, if the A/D pair was really stored (not read back). Whether they carry the model is not decided: at both push times the only stored pair had model 0. Meaning unknown; keep them raw.
  - p[4], p[5] shown as "Hotkey: Fn + R-Shift": the site hard-codes `0xFA` as "Fn"; `0xE5` = R-Shift. **CONFIRMED as the on-keyboard SOCD toggle**: pressing Fn + R-Shift produced the pushes. The site has no command that writes p[4..5] (its only `0x24` builders are `agreement.js:2029-2106`, at 2033, 2045, 2064, 2095), so the hotkey cannot be changed through the site. Treat p[4..5] as read-only.
- **Keyboard push** (CONFIRMED, c15 #218 +85499 and #231 +91271): each Fn + R-Shift press sent one unsolicited `24 02 00 00 06 PP 00 00 00 fa e5` + 52 zero bytes; only PP changed: 01 (set by the site) -> `00` on the first press -> `01` on the second. The user saw the site's switch change by itself. Details and how to tell a push from a read reply: section 2.5. The site parses it as the current state (`agreement.js:800-808` -> `refreshPrcsValue`, `wmIndex.js:358-364`), which sets the switch from code without firing a change event, so the site writes nothing back (the next SOCD write was the user's Apply 7.3 s later); it does not update its mirror's `prcsPower`. The keyboard can thus change SOCD power at any time without any program. Reading PP as the state **after** the toggle matches the site's parser and the sequence, but no read confirmed it. The status report did not change with the SOCD state and no status report accompanied the pushes (section 4.2).
- **Power write** (CONFIRMED framing, c15): `24 03 00 00 01 PP` (PP `00` off / `01` on) + zeros (`setPrcsPower`, `agreement.js:2029-2039`). **No ack** (OBSERVED, section 2.7). Effect (OBSERVED, not proven): the profile-1 read-back of power `00` and all-zero pairs (c16 #1360-#1364, c14-OPEN +513625) came about 247 s after B12 (c14-OPEN +266224) and after a profile 1 -> 2 -> 1 round trip, but the final state equals the factory state and no read happened while SOCD was on. One `24 02` read right after a power-on write would settle it.
- **Pairs read** (CONFIRMED framing): `24 01` -> 2 chunks `24 01 00 00 28` and `24 01 00 01 28` (byte 4 = `0x28` = 10 entries x 4 bytes each); W75 all zero, also after c15 (c16 #878/#879 in profile 2, #1363/#1364 in profile 1). The parser (`agreement.js:817`) treats an entry as empty if byte 0 = 0 or the entry is `07 FF FF FF` (never seen on the W75), and treats any other non-zero byte 0 as in use. A non-empty `24 01` read-back has never been captured.
- **Pairs write** (CONFIRMED, c15; CAUTION): 2 packets `24 00 00 <chunk> 28` + 10 entries of 4 bytes `[active = 1, model, key1 HID usage, key2 HID usage]` (`setPrcsData`, `agreement.js:2051-2087`; packing `2069-2081` writes the HID code of each pair key). Byte 2 = `00`, byte 3 = chunk 0/1, byte 4 = `0x28` in both; everything after the 40 data bytes is `00`. Pair A/D with the first model option = entry 0 `01 00 04 07` (A = usage `04`, D = `07`, not key indices 68/70); with the second option `01 01 04 07` (c15 #104, #247). The site packs pairs from entry 0 with no gaps; unused entries are `00 00 00 00`, also after a delete (no `07 FF FF FF`). At most 20 pairs. On non-US layouts the site sends key2 `0x31` as `0x32`. Each chunk is acked `24 00 00 00 00` (section 2.7). Firmware behaviour of the pair (A and D pressed together) was not tested.
- **Write bursts** (CONFIRMED): every SOCD action sends exactly power, then chunk 0, then chunk 1, within 0-2 ms and without waiting. Triggers (`wmIndex.js:2383-2393, 2421-2434`): the Apply button (always both, also with zero pairs; B01-B06, B08-B10), the SOCD switch (its change handler clicks Apply; B07 on, B12 off) and the **Delete** button (it removes the pair and always clicks Apply; B11). Creating a pair, picking keys and picking a model send nothing. A separate Apply the user remembered after Delete left no burst, so the site never registered that click. The site rewrites the whole SOCD state on every click; our app should send one debounced burst per real change.
- **Incomplete pairs** (CODE-ONLY): Apply always sends `24 03` first; `setPrcsData` then returns without sending either `24 00` chunk if any pair lacks key1 or key2 (alert `prcsTip4`, `agreement.js:2053-2060`), so an incomplete pair yields a lone `24 03`. (The earlier text "nothing is sent if any pair is incomplete" was wrong.) When a key is picked the site rejects a key already used in another pair and key1 == key2 (`isPrcsKeyDuplicate`, `wmIndex.js:3670-3675`, called from `3600-3621` and `4541-4554`), and it allows at most 20 pairs (`wmIndex.js:2404, 2418`). Our app should apply the same rules; whether the firmware itself rejects such tables is unknown.
- **Our writer (recommendation):** copy these bytes exactly. Send `24 03` with no ack expected plus a short settle, then each `24 00` chunk stop-and-wait on `24 00 00 00 00`. Whether chunk 0 can be written alone is untested.

| Model byte | UI radio (1-based) | UI meaning (our paraphrase of the en strings `prcsModel1`-`prcsModel4` in `language.json`) |
|---|---|---|
| 0 | Model1 (`prcsModel1`, default) | the key pressed first is interrupted (last input wins) |
| 1 | Model2 (`prcsModel2`) | key1 interrupts key2 |
| 2 | Model3 (`prcsModel3`) | key2 interrupts key1 |
| 3 | Model4 (`prcsModel4`) | the later key is not sent; it only interrupts the first |

The UI numbers the radios 1-4 (`v1.html:1327-1376`), the wire 0-3: the radio handlers set the model to 0..3 and send nothing (`wmIndex.js:2448-2465`); new pairs default to model 0 (`variable.js:157-165`; `prcsModel1` checked). Model 0 (B01-B09) and 1 (B10) are seen on the wire (OBSERVED); 2 and 3 are CODE-ONLY; the firmware behaviour of every model is untested. Show the vendor's 1-based names, store and write 0-3.

---

## 5. W75 key table

**What is confirmed and what is not:**

- The **82 positions** (row, col, index) are CONFIRMED: they are exactly the positions the site reads with `21/05`, in the same order in all three read passes of c01 and of c03, and the device echoes each row/col.
- The **names and HID codes** are CONFIRMED (2026-10-24, capture plan C1 done): the site's layout file `https://wraith.software/config/keys/W75HEARGB.json?t=v1` (layout type `us`, 82 keys with `index, name, code, hidCode, x, y, width, height`) was checked mechanically against this table. All 82 indices match and all 80 HID codes match; the two remaining entries are the knob (index 15, code `RotaryKnob`, hidCode `0`, empty name) and Fn (index 121, code `KeyFn`, hidCode `0x00`). The file is kept outside the repo for reference only and must not be committed.
- The **x, y, w** column is still the screenshot estimate; the layout file has exact pixel geometry, which our app should re-derive (in our own key-unit format) rather than copy.
- The structure offsets are derived from the index (CONFIRMED formulas for `0x21` mask, `0x09` and, since c09/c13/c16/c17, `0x18`; CODE-ONLY for `0x25`, `0x26`). The table was generated and checked mechanically against c01 (positions) and c02 (painted colour slots = these 82 minus the knob).

**Column guide:**

- **Mask byte / bit:** report byte (6 + col) and bit value (1 << row) in any 22-byte key mask (`0x21` trigger write bytes 6-27, `0x0E`, travel-test, calibration progress).
- **0x09 c:o** = chunk (byte 3) : payload offset of the R byte (G, B follow) in the colour tables. Report byte = 5 + offset.
- **0x18 c:o** = chunk : payload offset of b0 of the 4-byte keymap entry.
- **0x25 c:o** = chunk : payload offset of the switch-type byte.
- **0x26 c:o** = chunk : payload offset of the dead-band top byte (bottom = +1).
- **HID** = standard USB HID usage for the legend. It is a GUESS that the layout file uses exactly this code; the factory keymap tables are all zero, so the device does not tell us.
- **x, y, w** = approximate position and width in key units measured from the screenshot (GUESS; for drawing only).
- **Evidence:** C = confirmed by a user action and capture; I+ = inferred with an independent anchor; I = inferred from row order only. Session 3 sources: travel-stream row/col (c14: Esc, A, Space, Right), `0x18` entries with a working effect (c16 base layer: E, G, H; c17 Fn layer: F1-F4, W, Caps, where only Fn+W was an explicitly instructed test), Custom1 LEDs (c17: Up, Right; user report).

| Key | Row | Col | Index | Mask byte / bit | 0x09 c:o | 0x18 c:o | 0x25 c:o | 0x26 c:o | HID | x, y, w | Evidence |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Esc | 0 | 0 | 0 | 6 / `0x01` | 0:0 | 0:0 | 0:0 | 0:0 | 0x29 | 0,0,1 | C (travel stream `00 00`, c14) |
| F1 | 0 | 2 | 2 | 8 / `0x01` | 0:6 | 0:8 | 0:2 | 0:4 | 0x3A | 1.25,0,1 | C (Fn-layer `07 12`, c17) |
| F2 | 0 | 3 | 3 | 9 / `0x01` | 0:9 | 0:12 | 0:3 | 0:6 | 0x3B | 2.25,0,1 | C (Fn-layer `12 01 ff 00`, c17) |
| F3 | 0 | 4 | 4 | 10 / `0x01` | 0:12 | 0:16 | 0:4 | 0:8 | 0x3C | 3.25,0,1 | C (Fn-layer `12 00 10 00`, c17) |
| F4 | 0 | 5 | 5 | 11 / `0x01` | 0:15 | 0:20 | 0:5 | 0:10 | 0x3D | 4.25,0,1 | C (Fn-layer `07 25`, c17) |
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
| W | 2 | 2 | 46 | 8 / `0x04` | 2:30 | 3:16 | 0:46 | 1:34 | 0x1A | 2.5,2.25,1 | C (c03, c09, c14 stream `02 02`, c17 Fn-layer `07 32`) |
| E | 2 | 3 | 47 | 9 / `0x04` | 2:33 | 3:20 | 0:47 | 1:36 | 0x08 | 3.5,2.25,1 | C (DKS binding, c16) |
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
| Caps Lock | 3 | 0 | 66 | 6 / `0x08` | 3:36 | 4:40 | 1:8 | 2:16 | 0x39 | 0,3.25,1.75 | C (macro c13, Fn-layer `13 e0 e1 04` c17) |
| A | 3 | 2 | 68 | 8 / `0x08` | 3:42 | 4:48 | 1:10 | 2:20 | 0x04 | 1.75,3.25,1 | C (travel stream `03 02`, c14; factory Custom1) |
| S | 3 | 3 | 69 | 9 / `0x08` | 3:45 | 4:52 | 1:11 | 2:22 | 0x16 | 2.75,3.25,1 | I+ (factory Custom1) |
| D | 3 | 4 | 70 | 10 / `0x08` | 3:48 | 5:0 | 1:12 | 2:24 | 0x07 | 3.75,3.25,1 | I+ (factory Custom1) |
| F | 3 | 5 | 71 | 11 / `0x08` | 3:51 | 5:4 | 1:13 | 2:26 | 0x09 | 4.75,3.25,1 | I |
| G | 3 | 6 | 72 | 12 / `0x08` | 4:0 | 5:8 | 1:14 | 2:28 | 0x0A | 5.75,3.25,1 | C (MT binding, c16) |
| H | 3 | 7 | 73 | 13 / `0x08` | 4:3 | 5:12 | 1:15 | 2:30 | 0x0B | 6.75,3.25,1 | C (TGL binding, c16) |
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
| Up | 4 | 14 | 102 | 20 / `0x10` | 5:36 | 7:16 | 1:44 | 3:30 | 0x52 | 14.15,4.4,1 | C (Custom1 LED, c17) |
| L-Ctrl | 5 | 0 | 110 | 6 / `0x20` | 6:6 | 7:48 | 1:52 | 3:46 | 0xE0 | 0,5.25,1.25 | I |
| L-Win | 5 | 1 | 111 | 7 / `0x20` | 6:9 | 7:52 | 1:53 | 3:48 | 0xE3 | 1.25,5.25,1.25 | I |
| L-Alt | 5 | 2 | 112 | 8 / `0x20` | 6:12 | 8:0 | 1:54 | 3:50 | 0xE2 | 2.5,5.25,1.25 | I |
| Space | 5 | 6 | 116 | 12 / `0x20` | 6:24 | 8:16 | 2:0 | 4:0 | 0x2C | 3.75,5.25,6.25 | C (travel stream `05 06`, c14) |
| R-Alt | 5 | 10 | 120 | 16 / `0x20` | 6:36 | 8:32 | 2:4 | 4:8 | 0xE6 | 10,5.25,1 | I |
| Fn | 5 | 11 | 121 | 17 / `0x20` | 6:39 | 8:36 | 2:5 | 4:10 | see note | 11,5.25,1 | I+ (only 0x0E bit) |
| R-Ctrl | 5 | 12 | 122 | 18 / `0x20` | 6:42 | 8:40 | 2:6 | 4:12 | 0xE4 | 12,5.25,1 | I |
| Left | 5 | 13 | 123 | 19 / `0x20` | 6:45 | 8:44 | 2:7 | 4:14 | 0x50 | 13.15,5.4,1 | I |
| Down | 5 | 14 | 124 | 20 / `0x20` | 6:48 | 8:48 | 2:8 | 4:16 | 0x51 | 14.15,5.4,1 | I |
| Right | 5 | 15 | 125 | 21 / `0x20` | 6:51 | 8:52 | 2:9 | 4:18 | 0x4F | 15.15,5.4,1 | C (travel stream `05 0f`, c14; Custom1 LED, c17) |

**Notes on the key table**

- **Knob (index 15):** no HID usage. The site reads and writes trigger data for it like any key and writes its stored colour into `0x09` tables, but Select All never includes it. Whether it has any Hall-effect meaning is unknown.
- **Fn (index 121):** the factory base and Fn tables contain `00 00 00 00` here. The site writes `01 00 00 00` for the layout's `KeyFn` entry (hidCode `0x00`, no code1; `agreement.js:1108, 1148-1149`) and labels a read-back `00 FA` as Fn. Both `00 00 00 00` and `01 00 00 00` keep the Fn key working (CONFIRMED, c07 and c17; section 4.8.4); `01 FA 00 00` is untested. Keep the device's value.
- **Unused positions** (never read via `21/05`, written as 0 by the site in whole-table writes):
  - row 0: col 1, 16-21 (indices 16-21 are knob keymap slots, section 4.10)
  - row 1: col 13, 16-21
  - row 2: col 13, 16-21
  - row 3: col 1, 13, 16-21
  - row 4: col 1, 12, 15-21
  - row 5: col 3-5, 7-9, 16-21
- **Factory colour anomaly:** the device's original Custom1 table lit slots 103 (row 4, col 15) and 126 (row 5, col 16), which are not positions in this table, plus 124 and 125 (Down, Right). That looks like "WASD + arrows" shifted by one from the expected 102/123/124/125. Either the preset was authored for another matrix, or LED slots differ from key indices in the arrow cluster. **Partly answered (c17):** painting slot 102 lit the LED under Up and slot 125 the LED under Right (user report), so for those two the LED slot is the key index, and the "shifted arrows" reading (103 = Up, 126 = Right) is refuted. Left (123) and Down (124) were not painted alone, and what slots 103 and 126 drive is unknown. GUESS: the factory preset is off by one or was authored for another matrix. Open question Q8.

---

## 6. Site state model, the "settings reset" root cause, and what our app must store

### 6.1 Persistence (root cause of the "settings reset" report)

**Verified facts carried over from the earlier draft (2026-10-24):**

1. Settings written with `0x07 00` survive a USB unplug/replug without any save command, so the device persists them itself. USER-REPORTED. In addition CONFIRMED by capture: the last `07 00` record and the final `09 00` per-key table written in c02 read back unchanged in c03, a later page session.
2. The 1 s `0x01` heartbeat is not required to keep settings. USER-REPORTED (blocked for 90 s, nothing changed); CODE-ONLY reasoning in section 4.2.
3. The site keeps a mirror of all 5 profiles in `localStorage["W75HEARGB"]` (about 37 KB: lighting, side light, keymaps, Fn keys, macros, advanced keys, per-key triggers for 82 keys). USER-REPORTED (inspected in the browser). **Correction:** for the W75 the site does **not** render its UI from that mirror. On every connect the device's `0x11` reply makes the site delete the mirror, rebuild its state from device reads, and write the mirror again (section 6.5).

**Updated conclusion:**

- The keyboard (V3.22.07) answers the onboard-profile query `21/11` (CONFIRMED), so the site runs in **onboard-profile mode**. In this mode:
  - connecting writes exactly one packet, the no-op empty-mask trigger write (CONFIRMED: c01 +3394, c03 +2835, c14 +3100; three byte-identical trigger passes);
  - clicking a profile tab sends `21 00 00 00 00 12 PP` and, after the keyboard's `0x11` push, the same no-op empty-mask trigger write (CONFIRMED: c08, c10, c16; section 4.5.3). The earlier "exactly one packet" was wrong, but nothing that changes a setting is sent;
  - the site never pushes stored or default profile data to the keyboard (CONFIRMED for connect and for profile clicks (c04, c08, c10, c16: only `21/12` plus the no-op empty-mask write)).
- **The earlier hypothesis "clearing browser data makes the site push defaults on the next profile switch" is refuted for this keyboard and firmware.** Clearing site data loses only data that never lived on the keyboard (section 6.8). The precondition (onboard mode) is CONFIRMED; the profile-switch path is CONFIRMED (C3 done).
- The hypothesis exactly describes the site's **software-profile mode** (section 6.6). The site uses it only if no `0x11` report arrives: GUESS that older W75 firmware lacked `21/11` (unknown, open question Q31), or a tab click in the roughly 45-70 ms between the UI appearing (after the dead-band read, about +1744 in c01) and the `0x11` reply being processed (+1811).

**Remaining candidate explanations for a "reset"** (to be tested; none is proven):

| # | Mechanism | Confidence |
|---|---|---|
| a | Different onboard profiles hold different settings; a profile change from the site or on the keyboard (Fn + 1..4) shows another profile's data | GUESS (per-profile scope unverified, Q3) |
| b | **Stale writes after a profile switch.** After `21/12` the site re-reads nothing until a `0x11` report arrives (`wmIndex.js:1017-1023`). Any whole-table write in that window (keymap Apply: `19`, `18 00`, `18 02`, `22`; dead band `26 01`; switch `25 01`; colours `09 00/01`) copies the old profile's data into the new one. Even after a `0x11` report the site does not re-read side light, colour tables, switch types, poll rate, `0E` or status. Without a `0x11` report, clicking back to the original tab sends nothing. | mechanism CODE-ONLY; effect GUESS |
| c | **Lossy whole-table rewrites.** Every keymap Apply rewrites all 10 macro slots, both keymaps and all 40 advanced-key slots from the site's model; entries the site cannot decode are dropped; knob slots 16-21 are forced to defaults | CODE-ONLY |
| d | **Reset paths.** The Reset button sends `14 00 00 00 01 01`. **Fn + Space held for about 3-5 s resets the keyboard; a short tap does nothing.** On reset the keyboard sends an unsolicited `14 00 00 00 01 01` input report; if the site is open it immediately answers with its own `14 00 00 00 01 01` plus `26` (dead band) and `24 03 00 00 01` (SOCD), then re-reads `07 01` and all 82 triggers (c07, +90.16 s .. +91.44 s from OPEN). After the reset every key reads `00 55 55 01 01` (1.70 mm, RT off) and the main light reads `02 04 03 ff 00 00` (mode 2, colour kept). The final ack is a bare `14` with zero payload (#1017, the in-order reply to the site's write #991; section 2.7), so the site does not loop. Whether the reset clears all onboard profiles or only the active one is still open (Q4). | CONFIRMED (c07; long-press timing USER-REPORTED) |
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

**SOCD in the site's model** (CODE-ONLY, `agreement.js:800-841`): the `24 02` handler updates only the switch (also for the keyboard's Fn + R-Shift push, which the site follows without writing; it does not update the `prcsPower` field of `curProfile` in the mirror). The `24 01` handler pushes a new pair for every non-empty entry of each chunk into the global `prcses` list and never clears it (`agreement.js:811-840`); only the reload flows reset it (`agreement.js:585-597` in the `0x11` handler, `2300-2315`), and the software-mode profile switch replaces it from the mirror (`wmIndex.js:1048-1052`). The next Apply or power toggle writes the first 20 entries of that list (`wmIndex.js:2386-2388` -> `setPrcsData`). **Cross-program hazard** (GUESS): every program with the vendor interface open receives every input report (on Windows; `crates/openwraith-hid/src/session.rs:43-49`), so if another program reads `24 01` while the vendor site is open and pairs exist, the site would list duplicate pairs (visible after its next list refresh) and its next Apply or SOCD toggle would write them to the keyboard. This strengthens the "close the vendor site" rule for SOCD reads; with the W75's current all-zero pairs it is harmless.

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
| Click a profile tab | `21 .. 00 12 PP`, then after the `0x11` push the no-op `21 00 00 00 18 00` + zeros (CONFIRMED, c08, c10, c16) | - | onboard |
| Click a profile tab | full push (6.6) | mirror or defaults | software |
| Rename a profile | none | - | both |
| Keymap Apply (also after binding a macro, saving/deleting an advanced key, confirming any key assignment, resetting a key; an RS save can start it twice, un-awaited) | `19 00-09` (50), `18 00` (10), `18 02` (10), `22 00-27` (40), in this order (CONFIRMED: 11 bursts in c16, 7 in c17, one per Apply trigger: dialog OK, list delete, confirmed catalogue pick, key reset or a direct Apply click) | in-memory model | both |
| Macro dialog OK | none (storage only) | - | both |
| Lighting mode / colour / brightness / speed | `17 .. 00 00` on mode select, `07 00` / `08 00`, `09 00` / `09 01` whole table (a Custom-mode swatch click: one `09 00` table + one `07 00` with the colour as fg, c17) | UI, seeded from the keyboard | both |
| Trigger sliders / RT toggles | `21 .. 18 <mode>` for the selected keys | UI | both |
| Travel test on / off (off also on leaving the Actuation tab and on every Calibration sub-tab click) | `21 .. 18 02 <mask>` / `21 .. 18 03` (CONFIRMED, c14) | - | both |
| Calibration start / stop | `21 .. 18 0F 00` / `10 00` (W75) | - | both |
| Reset Trigger | `21 .. 18 06` (subject to the toggle), re-read | - | both |
| Dead-band sliders | `26 01` x 5 (whole table) | keyboard data | both |
| SOCD toggle / Apply / Delete | `24 03`, `24 00` x 2 (CONFIRMED, c15) | keyboard data | both |
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
| Q1 | **Mostly resolved.** C2: triggers (`0x21`, incl. RT bytes 30-31) and `0x07` persist across reconnect and USB power loss. OpenWraith H8 (2026-10-27): after our base keymap write (`18 00` + the `18 02` commit trigger, section 4.8.2) a USB power cycle left all four profiles byte-identical to the backup taken before it, so the keymap pair persists across power loss too. Dead band and Fn keymap persist across a profile switch (section 4.12, 4.8.2). Advanced-key slots written after `18 02` persist across a profile switch (c16, section 4.8.2). Macro slots, advanced keys and non-zero dead band across a power cycle remain untested. **Earlier partial answer:** triggers and `0x07` persist across reconnect and USB power loss. Still open: do the remaining writes persist across a USB power cycle: per-key colours (`0x09`, shown only across a page reload), keymap, macros, dead band, SOCD, switch types? Are trigger bytes beyond the declared length (29-35) applied? | C2, then reconnects in C7, C13, C14 |
| Q2 | **Resolved: `21/12` is acked with `21 .. 02 12 01` and followed by a `21 .. 03 11 PP 00` push (c04, section 4.5.3); a switch made on the keyboard (Fn + 1..4) is pushed too (OBSERVED in app test A9, 2026-10-27).** Still open: the exact bytes of the keyboard-side push (expected to match the `11 PP` push; a raw capture would confirm). | C6 |
| Q3 | **Partly resolved (c04, c10): triggers (`21/05`), the main light (`07`), the base keymap (`18 80`) and the macro slots (`19 8n`) are per profile (section 4.5.4). Session 3 (OBSERVED): advanced keys (`22 80`, c16 round trip and OpenWraith snapshot) and the Fn keymap (`18 82`, snapshot) are per profile too; custom table 1 was already known from an OpenWraith backup.** Still open: which of the others are per onboard profile and which are global (`08`, `09 81` Custom2, SOCD power and pairs, dead band, switch types, poll rate, Win-Lock/6-key, gamepad)? c15 changed SOCD only in profile 1 and profile 1 ended in the factory state, so the c16 round trip cannot tell; each needs a profile-differing write. | C5, app test (an all-profile backup shows which tables differ between profiles) |
| Q4 | **Reply: one sample (c07).** The site's reset write #991 was answered `14 00 00 00 00 00` (#1017, byte 5 = 0, about 820 ms later, section 2.7), so that reset did not loop. Still open: what does `14 00 00 00 01 01` reset (current profile or all)? Does it include triggers and dead band? Is the reply to a Reset-button write the same, and does it ever carry byte 5 = 1 (which would make the site loop)? | C23 |
| Q5 | What do replies to other `0x14` writes look like (Win-Lock, 6-key, gamepad)? Does `14 03 .. 01 01` (Xbox) get echoed with byte 5 = 1, colliding with the site's reset trigger? | C9, C22 |
| Q6 | **Resolved.** C1 checked the layout file against section 5 (indices, names, HID codes; Fn `KeyFn` hidCode `0x00` without code1). Its `RotaryKnob` entry (index 15) has **no knob regions** (section 4.10). Session 3 confirmed further positions by user action: Esc, A, Space, Right (c14 stream), E, G, H (c16), F1-F4, Caps (c17), Up, Right (c17 LEDs). The remaining C12 keys would finish the per-key check at no risk. | C1, C12 (remainder) |
| Q7 | Knob: which keymap slots does the knob use, if any; what do slots 19-21 do; does the site's forced overwrite of 16-21 change knob behaviour; does index 15 have any Hall-effect meaning? **Narrowed (c16, c17):** the W75 layout has no `knobRegions`, so the site cannot remap the knob and never writes knob defaults into the Fn table. Not answerable through the site. | app test |
| Q8 | **Partly resolved (c17):** the LED slot equals the key index for Up (102) and Right (125), so 103 is not the Up LED and 126 not the Right LED (user report). Still open: Left (123) and Down (124), the rest of the keys, and which LEDs, if any, slots 103 and 126 drive. | C11 (remainder: Left, Down) |
| Q9 | Key-mask orientation for `0x21` writes (byte = column, bit = row) on an off-diagonal key; exact Select-All mask (knob excluded?) | C8 |
| Q10 | Meaning of status byte 19 (`01`), byte 7 of the `0x11` reply (`01`), flags A bit 6; does status byte 20 follow the keyboard's Win-Lock toggle? **Narrowed:** status byte 19 is not the active profile (`01` while profile 2 was active, c16) and not the SOCD power (c15). | C6, C9 |
| Q11 | Are onboard profiles numbered 1-4 only, or is 0 a hidden default profile? | app test (not offered by the site) |
| Q12 | Does the keyboard acknowledge `0x18` writes (**yes**, section 2.7)? Can a single chunk be written alone? Which encoding does an unmapped key / the Fn key need? **Partly resolved:** at the Fn key (121) both `00 00 00 00` and the site's `01 00 00 00` keep Fn working (c07, c17); `01 FA 00 00` is untested. Does sub `01`/`81` exist? Does a non-zero `18 02` written alone survive a profile switch (4.8.2 shows it for a small change at index 130; c17's site-written Fn entries were not checked across a switch)? | app test |
| Q13 | Macro details: mouse button codes (writer `01/02/04` vs reader `00/01/02`), delay placement, count field in modes 0/2/3, use of macro ID and copy flag, empty slot `00` vs `FF`, step limit (62 vs 20 groups), media steps | C14 |
| Q14 | Does a non-zero Fn-layer entry override a built-in Fn function? Is multimedia index `24` Browser Forward or Back? **Partly answered (c17, OBSERVED):** Fn-layer entries on F1-F4 most likely replace the built-ins (blanket user report "everything works", base entries zero; section 4.8.4). To make it firm, the user should confirm that Fn+F1..F4 and Fn+Caps were tested with Fn held and that plain F1-F4, Caps and W were unchanged. Still open: the direction of `07 24` / `07 25` (the en/tr item "Browser Forward" writes `07 25`; which way the browser went was not recorded; app test: two Fn-layer keys with `07 24` and `07 25` on a page with history); whether entries on Fn+1..4, Fn+Space or Fn+Backspace are obeyed (risky: a deliberate, reversible test on a spare profile). | app test |
| Q15 | Trigger details: meaning of the second travel value (read bytes 8/12, write 29/33) and whether it may differ; `21/04` byte 8; modes other than `00/0C/0D`; meaning of the ack byte; does the firmware clamp or reject out-of-range values? | app test (the site always writes equal values and stays in range) |
| Q16 | Does mode `0D` (separate press/release) work as the code suggests? | C8 |
| Q17 | **Resolved (c14, section 4.11.7):** bytes 6/7 = row/col of the key that moved; rate about 72 ms per long report on slow motion and 0-3 ms short-report bursts on fast motion; open and close are acked `21 00 00 00 01 00`. Remaining stream details are Q34. | done 2026-10-27 |
| Q18 | Which status reports does the W75 send during any-key calibration (`0F`/`10`); is there a reply to stop? | C20 |
| Q19 | **Mostly resolved (c16, c17, section 4.14):** DKS, MT and TGL layouts and the `22 00 00 00 00` acks are CONFIRMED; DKS step value 1 at stages 0 and 1 is a tap; routes are written as 15/30/30/15 for 1.5/3.0 mm. Still open: proof of the 0.1 mm unit (route1 = 5 vs 25); DKS step values 2/3 and stages 2/3; the MT hold path (L-Ctrl after 300 ms) and whether byte 14 is really 10 ms per unit (15 vs 100); whether a second TGL press releases the key and whether TGL takes anything other than a keyboard usage; a non-empty DKS/MT `22 80` read-back. | app test |
| Q20 | **Partly resolved (c15):** the hotkey p[4..5] = Fn + R-Shift is CONFIRMED as the on-keyboard SOCD toggle, which pushes `24 02`; the site has no command that writes it, so it cannot be changed through the site. Still open: the meaning of p[1..3] (always `00`, with power on and off; the model of a stored pair is not excluded). | app test |
| Q21 | Is there a real dead-band reset command (the site sends the read request)? Is byte order top/bottom right? | C13, C23 |
| Q22 | Does the switch-type ID change firmware behaviour? | C18 (low value: W75 reports only HM1) |
| Q23 | Lighting semantics: speed scale direction, Full Color effect (also in Static/Custom where it is hidden), brightness 0 vs power byte, `A5 5A`, order of max speed / max brightness, record bytes 17-18; does the `0x07` fg matter in Custom mode `0x0A` (the site overwrote it with the swatch in c17; did only the painted keys turn blue while every other key stayed red)? | C10 (max speed/brightness order: not resolvable on W75, both 4); ask the user (fg in mode `0x0A`) |
| Q24 | How does the keyboard choose between Custom1 and Custom2? Is the `09 20` reply really the active slot? What does a `09 01` ack look like? | C11 |
| Q25 | Were colour writes lost during the site's flood (1681 acks for 2064 chunks)? Flash wear? | app test (throttled writes with ack checking) |
| Q26 | Is the `17 .. 00 00` leave packet needed on the W75? Is the unsolicited `0x01` after `0x17` a status push? | app test |
| Q27 | Are modes `05`, `0D`, `64` or other unlisted modes valid? Does writing `0x08` do anything on the W75? | app test (low value, avoid) |
| Q28 | Do poll-rate and gamepad writes re-enumerate the device; which PID per gamepad mode; what is `0xC365`? | C21, C22 |
| Q29 | Does the heartbeat have any firmware-side effect? Is status byte 1 (link token) ever non-zero on the wired W75? | (user test done for 90 s; low value) |
| Q30 | Meaning of `0x0D` fields `W669`, `34`, `KB`, `ML` | not resolvable by captures |
| Q31 | Did older W75 firmware lack `21/11` (which would put the site in software-profile mode and explain earlier "resets")? | not resolvable on this keyboard |
| Q32 | Why does `21/11` take 39-65 ms (six connects; c14 59 ms) (flash read?); must other commands wait for it? | app test |
| Q33 | Travel-test lifecycle and side effects: does a mask with fewer keys limit which keys stream (only the full 82-position mask was captured)? Does the test stay open after a USB disconnect, a host-side close, a page reload, a profile switch or keyboard sleep if no `18 03` is sent? Is a `18 03` sent while the test is closed harmless (the site sends one on every Calibration sub-tab click, not captured)? Does an open test change normal key output, Rapid Trigger or actuation (the standard keyboard interface was not captured in c14)? | C12 (remainder: Calibration-tab click), app test |
| Q34 | Travel-stream details: what rule picks the short (`05`) versus the long (`03`) variant (a speed threshold GUESS that depth-0 reports break)? Is the ~72 ms tick per key or global, and why does its phase shift (#1005, #1064)? Is there a start threshold (a slow press first reported depth 17, a release reported depth 10 before 0)? What are long-variant bytes 10-15, 31-36, and why do 19-20 and 21-22 differ only at rest? Is byte 9 the depth high byte (not testable on the W75, max 170)? Does the depth include the dead band or the switch-type travel (dead band was 0 in c14)? Does the knob (15) or Fn (121) send stream reports? | C12 (remainder), C20 (calibration GUESS), app test |
| Q35 | SOCD write details: is `24 03` really never acked (one lone sample, during the reset flow; test: send it alone to an idle keyboard and listen about 100 ms)? Does a `24 03 .. 01` write take effect, and does the push's p[0] give the state after the toggle (one `24 02` read right after a power-on write settles both)? Does the keyboard toggle persist across a profile switch and a USB power cycle, and does it follow the delayed commit of 4.5.3? Does the push also happen with no pair stored? Does a non-empty `24 01` read back as written? Can chunk 0 be written alone? Firmware behaviour of models 0-3 (nobody pressed A and D together), duplicate keys across pairs, key1 == key2, more than 20 pairs? | app test |
| Q36 | Advanced-key write rules: is a `22` slot kept when no `18 02` is written in the same group (write one slot, wait 2 s, switch profile and back, read `22 80`)? Can a single slot be written without the other 39? Does a `17 SS SS 00` binding work in the Fn layer, and does the firmware read the slot from b1, b2 or both? What happens with a binding to an empty slot, or an orphan slot without binding (possible after the site's delete, which cannot see other profiles)? Is the ~2 ms per-report drain (acks and `22 80` chunks) a firmware limit or the endpoint's interval? | app test |
| Q37 | Keymap code variants: which direction mouse masks `08` and `10` produce (the site labels `10` Back; standard HID numbering makes it button 5, usually Forward); wheel up `12 01 01 00` and buttons L/R/M; a one-modifier combination `13 <mod> <key> 00`, right-side modifiers E4-E7, a non-modifier in b1/b2. | app test |

**Top 5:** Q1 (persistence of non-lighting writes), Q2 (profile select behaviour), Q3 (per-profile scope), Q4 + Q5 (reset scope and `0x14` reply format, including the site's self-reset hazard), Q8 (LED mapping; Q6, the authoritative layout, is resolved).

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
| C11 | **PARTLY DONE 2026-10-27** (c17: (b) for Up and Right only; LED slot = key index for both, section 4.7.6. Not done: (a), (c), Left, Down). **Custom tables and LED mapping.** Lighting > Custom: (a) click Custom2 (expect 8 x `09 01` + acks), reload, check the `09 20` reply; (b) Custom1: Deselect All, select only Up, pick blue, note which LED lights; repeat for Left, Down, Right (MARK each); (c) click Custom1 / Custom2 back and forth and note which table the keyboard shows. | `09 01` ack bytes; `09 20` byte 1; written slots 102/123/124/125 vs lit LEDs | Q8, Q24 | low (repaints Custom1) |
| C12 | **PARTLY DONE 2026-10-27** (c14: test opened and closed once; W, Esc, Space, Right, then W + A pressed; Q17 resolved, section 4.11.7. Not done: the other keys, the knob, Fn and the Calibration sub-tab click). **Travel test.** Actuation > Travel: Travel Test on. MARK and press, one at a time: Esc, F1, Del, the knob (press), Grave, Backspace, Home, Tab, Backslash, PgUp, Caps, Enter, PgDn, L-Shift, R-Shift, Up, L-Ctrl, Space, R-Alt, Fn, R-Ctrl, Left, Down, Right, W. Travel Test off. Then click the Calibration sub-tab once and leave it. | `21 .. 18 02 <82-key mask>`, `21` byte 5 = `01` stream (bytes 6/7 = row/col?), `21 .. 18 03`; another `18 03` on the Calibration tab | Q6, Q17 | none |
| C13 | **Dead band.** Actuation > Dead Band: select W, Top 0.10 mm, Bottom 0.20 mm. Reload and reconnect. Then set both back to 0. | `26 01` x 5 with chunk 1 bytes 39/40 = `05 0a`; read-back | Q1, Q21 | low |
| C14 | **Macros.** Keymap > Macro: (a) create a macro "A down, 100 ms, A up, 200 ms, left click down/up, middle click down/up, right click down/up" and save it **without** assigning (expect no `19` write); (b) assign it to Caps with "Operate 3 times", Apply, reload and reconnect; (c) assign the same macro to an Fn-layer key, Apply; (d) on the Caps binding select each of the four play modes, Apply after each; (e) delete the macro, Apply. | Slot layout, mouse codes, delays, count, copy flag (bit 7), mode byte, zeroed slots, keymap `10 39 SS 00` | Q1, Q13 | medium (rewrites macro/keymap/advanced tables) |
| C15 | **DONE 2026-10-27, with changes** (c17: all remaps went to the Fn layer because it was selected: F1 Mute `07 12`, F2 wheel down `12 01 ff 00`, F3 "Mouse Back" `12 00 10 00`, F4 `07 25`, Caps Ctrl+Shift+A `13 e0 e1 04`, W Calculator `07 32`; no Brightness+ entry; the user reported all working; undone by an OpenWraith restore. Sections 4.8.2-4.8.4). **Fn layer and special codes.** Fn layer: Fn+W -> Multimedia Mute, Fn+E -> Brightness+, Fn+F4 -> Calculator; base layer: Caps -> Ctrl+Shift+A, one key -> Mouse Wheel Down, another -> Mouse Back, another -> Browser Forward. Apply. Physically test whether Fn+F4 now opens the calculator instead of muting. Undo afterwards. | Entries `07 12`, `03 00`, `07 32`, `13 E0 E1 04`, `12 01 FF 00`, `12 00 10 00`, `07 24` or `07 25` | Q14 | medium |
| C16 | **DONE 2026-10-27, except the knob part** (c16: DKS on E with E at UI stage 1 and R at UI stage 2 (wire stages 0 and 1) as taps, MT tap G / hold L-Ctrl / 300 ms, TGL H, typing test passed; the W75 layout has no knob regions, so the knob part cannot be done in the site; deleted by an OpenWraith restore. Section 4.14). **Advanced keys and knob.** Keymap > Advanced: a DKS on one key with 4 actions and mixed tap/hold bars; an MT (tap A, hold L-Ctrl, 300 ms); a TGL; Apply. If the knob shows clickable regions, remap "rotate right" to PgDn and Apply. Delete all afterwards. | `22` slot writes, `18` entries `17 SS SS 00`, knob-region slot index | Q7, Q19 | medium |
| C17 | **DONE 2026-10-27** (c15: pair A/D with Model1, then Model2; switch on and off; Fn + R-Shift pressed twice on the keyboard; pair deleted. Section 4.15). **SOCD.** Create pair A/D with Model 1, Apply; toggle the SOCD switch on and off. Delete the pair afterwards. | `24 03`, `24 00` x 2, entry format | Q20 | low |
| C18 | **Switch type.** Actuation > Switch: select W, click the HM1 button. | `25 01` x 3 | Q22 | low |
| C19 | **Reset Trigger.** Actuation > Travel: click "Reset Trigger" and confirm. Re-apply W 1.00 mm / RT 0.30 mm afterwards. (Reload the page first: any earlier `0x14` reply in the same page session, e.g. from C9, makes the site silently skip the next trigger reset; if no `21 .. 18 06` appears, click it again.) | `21 .. 18 06` and the re-read; the firmware's default trigger values | Q15 (defaults) | medium (all triggers reset) |
| C20 | **Calibration (optional).** Only if you accept recalibrating: Actuation > Calibration, Start, press W and A fully, wait for the colour change, Stop. | `21 .. 18 0F 00`, status reports, `21 .. 18 10 00` | Q18 | **high** (calibration state) |
| C21 | **Polling rate (optional; the keyboard restarts).** 8 kHz -> 1 kHz (accept the prompt), wait for reconnect, -> 8 kHz. | `21 .. 00 09 01 01` / `.. 08`, CLOSE/OPEN, PID | Q28 | medium |
| C22 | **Gamepad (optional; the keyboard restarts).** Only after C9 and a full backup (C2 capture), on a profile you can afford to lose: Keymap > GamePad -> Xbox (accept), note the PID in the new OPEN entry, then back to Disabled. If the site starts a reset by itself (look for `14 00 00 00 01 01`), stop and report. | `14 03 .. 01 01`, its reply (byte 5?), the new PID | Q5, Q28 | **high** (restart; possible self-reset by the site) |
| C23 | **Reset current profile (DESTRUCTIVE, last).** Only after a full backup capture of every profile: switch to a profile you can afford to lose, click Reset and confirm. Then switch to the other profiles and check they are untouched. | `14 00 00 00 01 01` reply bytes; whether `21 .. 18 06`, `26 00`, `24 03 .. 00` follow; which tables changed | Q4, Q21 | **high** |

---

### 7.3 Capture session 2 (2026-10-24 evening) – results

Captures `c08`-`c13` (`captures/capture_08_profile_switch.json` … `capture_13_macro_ab_on_caps.json`). Plan items done: C3, C7, C8 (a, b; the Select-All part was skipped), C13, C14 (create, bind to Caps, typing test, unbind), C19. All confirmations are already reflected in the master table (section 3).

- **Profiles (C3):** `21/12` select is acked and followed by an unsolicited `11 PP` push; the site re-reads everything and writes nothing (Correction 2026-10-27: it also sends the no-op empty-mask trigger write after each `0x11` push, section 4.5.3). **Main light and per-key triggers are per onboard profile** (profile 2 kept factory triggers and its own light while profile 1 had Q 1.50 mm and W RT 0D; the user saw the light change). **The base keymap and the macro slots are per profile too** (OBSERVED, c10: profile 2 read an all-zero base keymap and all-`FF` macro slots while profile 1 read the site's c09 knob defaults and all-`00` slots). Q3 is answered for these four; Fn keymap, advanced keys, dead band, SOCD, colour tables and switch types are still unverified for scope. **Update (OpenWraith all-profile backup, 2026-10-24):** profile 1 also differs from profiles 2-4 in **custom colour table 1** (`09 80`), so it is per profile too (OBSERVED); profiles 2-4 are identical to each other. Dead band, Fn keymap, advanced keys, SOCD, switch types, side light, custom table 2, poll rate and Win-Lock read the same in all four profiles, but they hold factory values everywhere, so their scope is still unknown.
- **Key mask orientation (C8, Q9): CONFIRMED** byte = 6 + column, bit = 1 << row (Q at row 2, col 1 -> byte 7 = `0x04`).
- **Separate press/release (C8, Q16): CONFIRMED** mode `0D`, byte 30 press, byte 31 release.
- **Reset Trigger (C19): CONFIRMED**, see row 34; only the active profile was affected as far as the captures show (profile 2 was already at factory values, so this is not proof).
- **Keymap / macros / advanced keys (C7, C14):** every Apply rewrites 10 macro slots (5 chunks each), both keymap layers (10 chunks each) and 40 advanced-key slots: 110 packets, every packet acked with a zero-length report of the same cmd. The site also forces its knob defaults into the base keymap (slots 16-21: `07 12`, `07 11`, `07 10`, `02 02`, `03 01`, `03 00`) and `01 00 00 00` at Fn (121) although the device had all zeros before; these now read back from the device. **The lossy whole-table rewrite (section 6.1 row c) is CONFIRMED.**
- **Dead band (C13): CONFIRMED** layout and units; the site rewrites the whole 264-byte table on every slider step (37 full writes in about 20 s).
- **Acknowledgements:** every write of the session was acked (section 2.7); acks carry no chunk index, and `18`, `19` and `22` acks do not echo byte 1.
- **Implications for our writer:** read the table from the device first, change only the target entries, write it back once (debounced), and verify by reading it back; never synthesise defaults for entries we did not change.
- **Never captured** (keep these CODE-ONLY until a capture or a hardware test shows them): a trigger mask with more than one key; a read-back of a non-empty macro slot; a non-zero dead-band read; Fn-layer writes with non-zero content; non-empty advanced-key writes; persistence of keymap, macros and dead band across a power cycle (Q1). (Update 2026-10-27: Fn-layer writes with non-zero content and non-empty advanced-key writes were captured in session 3, section 7.4.)

### 7.4 Capture session 3 (2026-10-27) – results

Captures `c14`-`c17` (`captures/capture_14_travel_test.json` … `capture_17_keymap_codes_led_map.json`), one page session on profile 1; c15-c17 have no OPEN entry (section 0.2). Plan items done: C12 (part), C15 (on the Fn layer instead of the base layer), C16 (without the knob part), C17, and C11 (b) for Up and Right only. All confirmations are reflected in the master table (section 3; rows 29, 30, 31, 46 and 47 moved from CODE-ONLY to CONFIRMED) and in the subsystem sections.

- **Connect (c14, section 4.4):** identical to c03 in its OUT sequence; `21/11` answered after 59 ms (39-65 ms over six independent connects, section 2.5).
- **Travel test (c14, C12 part, section 4.11.7):** open `21 00 00 00 18 02` + the 82-position mask and close `21 00 00 00 18 03` are CONFIRMED, each acked `21 00 00 00 01 00` within 1 ms; 253 stream reports for W (slow), Esc, Space, Right, then W + A; none after the close in c14-c17. Bytes 6/7 = row/col of the key that moved (Q17 resolved), depth in bytes 8-9 (0-170 units, 0.02 mm), one key per report, nothing at rest. Two variants: short (byte 4 = `05`) during fast motion, in 0-3 ms bursts; long (byte 4 = `03`, data to byte 36: direction, sensor readings and per-key constants) on a ~72 ms tick during slow motion. Every release ends with a depth-0 report. Two keys interleave in one stream. Not done: the other C12 keys, the knob, Fn and the Calibration-tab close.
- **SOCD (c15, C17, section 4.15):** 12 write bursts, each power `24 03 00 00 01 PP` + two `24 00` chunks within 0-2 ms; each chunk acked `24 00 00 00 00`, the power write not acked. Pair A/D = entry `01 00 04 07` (HID usages; UI Model1 = wire 0), then `01 01 04 07` (Model2 = 1). Apply, the SOCD switch and the Delete button each send a full burst; delete writes zeros. **Fn + R-Shift on the keyboard pushes an unsolicited `24 02` with the new power** (2 presses, 2 pushes); the site follows it without writing. The status report never changed (status bytes 14 and 19 are not the SOCD power). The power write's effect was not read back while on.
- **Advanced keys (c16, C16, section 4.14):** first attempt: one dialog keeps one advanced key, so only TGL H went out, unbound, to slot 0; it survived a profile 2 -> 1 round trip while profile 2 read empty slots (**advanced keys are per profile; unbound slots are kept**); the next burst cleared it. Second attempt: DKS slot 0 (E at wire stage 0, R at stage 1, routes `0f 1e 1e 0f`), MT slot 1 (hold L-Ctrl, tap G, `1e` = 300 ms), TGL slot 2 (H), bound `17 SS SS 00` at E, G and H in the base layer; slots allocated lowest-free. 11 Apply bursts of 110 packets, every packet acked. Typing test: E light "e" / full "er", G tap "g", H toggled "hhhh". No HID traffic when the keys fire. The W75 layout has no knob regions, so the knob part of C16 cannot be done in the site and the Fn table never gets knob defaults. Also seen: after each `0x11` push the site sends the no-op empty-mask trigger write (section 4.11.4), and status byte 19 stayed `01` while profile 2 was active.
- **Special codes and LED map (c17, C15 and C11 b, sections 4.8, 4.7.6):** the Fn layer was selected, so every remap went to `18 02`: F1 `07 12` (Mute), F2 `12 01 ff 00` (wheel down), F3 `12 00 10 00` (the site's "Mouse Back"), F4 `07 25`, W `07 32` (Calculator), Caps `13 e0 e1 04` (Ctrl+Shift+A). 7 Apply bursts of 110 packets, all 770 acked; the base table stayed unchanged. The user reported that every assignment works (Fn+W explicitly tested); the Q14 override conclusion rests on that report. Custom1: two `09 00` tables (16 of 16 chunks acked) each followed by one `07 00` that carried the picked colour as fg; Up (102), then Right (125) painted blue, and the LEDs under Up and Right lit blue (user report), so LED slot = key index for these two.
- **OpenWraith read-back and restore after the session:** OpenWraith's pre-restore safety snapshot (12:45:19Z, 19 s after `END_SESSION_3`) read profile 1's base and Fn keymaps and Custom1 equal to the final site writes byte for byte, and advanced slots 0-2 equal to the first 40 bytes of the slot writes; profiles 2-4 read an all-zero Fn table and empty advanced slots (**the Fn keymap is per profile**, OBSERVED), and Custom2 read the factory 34 blue slots in all four profiles. OpenWraith then restored the keyboard from a backup: base and Fn keymaps, three advanced-key clears, Custom1 and the main light were written, verified and persisted after a profile switch (OpenWraith restore read-back, 2026-10-27). Its keymap group used the site's order (base, Fn, then the `22` clears), a second sample of "`22` after `18 02`" persisting (section 4.8.2).
- **Corrections to earlier text:** the site does write after a profile select (the empty-mask no-op; sections 4.5.3, 6.1, 6.7, 7.3); an incomplete SOCD pair still sends `24 03` (section 4.15); the mouse masks `08`/`10` keep the site's Forward/Back labels only as labels (section 4.8.3); status byte 19 is not the active profile (section 4.2); the Fn-layer knob defaults never apply on the W75 (sections 4.8.2, 4.10).
- **Implications for our app:** a live per-key travel view is cheap but must close the test, drain continuously and decode the long variant past byte 4; it is a visualiser, not a basis for software analog input. The keyboard's `24 02` push is a user action, not another program, and must refresh any SOCD state shown or backed up. SOCD, advanced-key, Fn-layer and custom-colour writers can copy the captured bytes, written stop-and-wait, one changed table or slot at a time, never dropping the `18 00` -> `18 02` pairing.
- **Still never captured** (after session 3): a non-empty DKS/MT `22 80` read-back from the site; a non-empty `24 01` read; a `24 02` read while SOCD was on; DKS step values 2/3 and stages 2/3; the MT hold path; a TGL release; an advanced-key binding in the Fn layer; a `22` group without `18 02`; persistence of site-written non-zero Fn entries across a profile switch; Left/Down LEDs; `09 01`; a multi-key trigger mask; stream reports for the knob, Fn and the other keys.

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
| Trigger reset skipped in the full reset | "skipped when the reset's 0x14 reply comes first" vs "skipped after an earlier 0x14 reply" | The flag is checked right after the reset is sent, so an **earlier** `0x14` reply is what normally causes the skip; the reset's own reply most likely arrives later (one sample: about 820 ms in c07, #991 -> #1017) | `agreement.js:320-321, 940-954`; `wmIndex.js:1331-1336` |
| `0x0D` field numbering | "field 4 / 5" vs "field 5 / 6" | 0-based: field 4 = product, field 5 = version | `wmIndex.js:6623-6637` |
| Win-Lock bit mapping | write bytes vs read bits | write bytes 12-15 = bits 0-3 = Win, Shift+Tab, Alt+F4, Alt+Tab (checkbox 4 is Alt+F4, checkbox 3 is Alt+Tab) | `agreement.js:1728-1746`; `wmIndex.js:1709-1732, 4187-4199`; `v1.html:1035-1063` |
| Heartbeats after the travel-test close (session 3) | 39 vs 36 heartbeat pairs in c14 #1314-#1387 | 36 pairs plus 2 MARKs; the `18 03` pair (#1312/#1313) lies outside that range | c14 recount |
| Slow W press and release counts | 58 / 55 stream reports | press #999-#1065: 57 long reports, no short; release #1078-#1135: 50 reports (45 long, 5 short) | c14 recount |
| Travel-stream labels | row/col and "sent only on change" proposed as CONFIRMED | OBSERVED: the site evaluates bytes 6-7 and discards them and says nothing about when the firmware sends; only the depth parse is CONFIRMED | `agreement.js:488-495` |
| OpenWraith restore order after session 3 | "OpenWraith's order (`22` before `18 02`) persisted too" | The restore wrote both keymaps, so `plan_keymap_group` used the site's order (base, Fn, then `22`); `22` before `18 02` is still untested | `crates/openwraith-hid/src/restore.rs` |
| SOCD read-back delay after the last SOCD write | about 91 s vs about 247 s | about 247 s: the c16 times were relative to c16 #0 and B12's to c15 #0; relative to the c14 OPEN, B12 is +266224 and the read +513625 | c14-c16 time bases (section 0.2) |
| Telling a `24 02` push from a read reply | "`24 01` always follows within 0-2 ms, so a 10 ms window suffices" | read chunks lag up to 13 ms when other replies are queued (c04 #11910), so a window of tens of ms is needed (GUESS) | c04, c16 |
| Fn-layer override of F1-F4 (Q14) | proposed as CONFIRMED | OBSERVED: the user's report is generic and the test prompt asked for F1 without Fn | c17; section 4.8.4 |
| LED test conclusion (Q8) | "slot 126 is not the Left LED" | under the arrow-shift alternative slot 126 would be Right; the test shows 103 is not Up and 126 is not Right; Left and Down are untested | section 5 notes |
