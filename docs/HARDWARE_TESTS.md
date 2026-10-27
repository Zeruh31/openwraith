# Hardware tests

Every automated test runs against captures or the device simulator; none of
them opens a HID device. The commands that write have therefore only been
checked against the simulator. This checklist is run **by hand** on a real
Wraith W75, in order: H0-H11 for the Phase 2 writes, H12-H25 for the v0.2
editors of the CLI, A1-A10 for the desktop app. Record the full output of
every command (add `--verbose` for session statistics) and keep every file
it writes.

File names below are examples; use your own. `openwraith` is
`target\release\openwraith.exe` after `cargo build --release`.

## Preconditions

- Firmware `V3.22.07`, USB cable (not the 2.4 GHz dongle), one keyboard
  connected.
- **Close every wraith.software tab** (close the tab itself; a background tab
  may not be detected) and every other keyboard program. Keep it closed for
  the whole session, except in H11.
- Do not switch profiles on the keyboard while a command runs (except in
  the optional H7c, which does it on purpose).
- Every command that writes prints the safety backup it saved and the undo
  command (`openwraith restore <safety backup>`). If anything looks wrong,
  stop and run that undo command.
- The keyboard saves a write about 1 s after it; a profile switch or an
  unplug before that loses it (PROTOCOL.md 4.5.3, "Delayed commit", found
  during the 2026-10-24 session). Every command that writes therefore prints
  `Waiting for the keyboard to save profile N (2 s)...` and pauses for 2 s
  before it switches profiles or finishes. Do not switch profiles or unplug
  anything while it waits; if that happens, the command reports the write
  as `MAY BE LOST` and exits with code 1. Ctrl+C during the wait stops a
  restore before it selects the next profile.

## H0. Setup

Close the vendor site (see above). Do not unplug anything.

Pass: `openwraith list` shows exactly one keyboard.

## H1. Backup of the active profile

```text
openwraith backup -o before.owbackup.json
openwraith profile
```

Pass: the backup is saved (read-only command, nothing sent but reads), and
`profile` names the active profile (call it P below; normally 1).

## H2. All-profile backup

```text
openwraith backup --all-profiles -o all.owbackup.json
openwraith profile
openwraith show all --from all.owbackup.json --profile 2
```

Pass: the lighting changes briefly while profiles 1-4 are read; afterwards
`profile` reports P again. The backup lists profiles 1, 2, 3, 4.

Compare: note which tables of unknown scope (custom colours 1, dead band, Fn
keymap, advanced keys, side light, SOCD, switch types, polling rate,
Win-Lock) are identical in all four profiles (`show ... --profile N` for each
N, or compare the JSON). Identical copies after different settings in the
vendor site suggest a shared table (PROTOCOL.md Q3).

## H3. Nothing to restore

```text
openwraith restore all.owbackup.json --dry-run
openwraith restore all.owbackup.json
```

Pass: both report that nothing has to be written (the real run saves a safety
backup, switches profiles to read them, returns to P and exits 0 without a
question). If anything is planned, answer `N` and report the plan.

## H4. Trigger edit

```text
openwraith set trigger W --actuation 1.20
openwraith show triggers
```

Answer `y`. Pass: the plan shows W only (`now -> 1.20 mm`); stderr shows
`Waiting for the keyboard to save profile 1 (2 s)...`; the report says
`written and verified (1 packet)` and ends with `Waited 2 s after the last
write so that the keyboard could save it ...` (an edit does not switch
profiles, so the lighting does not change); `show triggers` shows W at
1.20 mm. Type on W: it actuates noticeably earlier/later than before,
nothing else changed.

## H5. Restore undoes the edit

```text
openwraith restore before.owbackup.json
openwraith restore before.owbackup.json --dry-run
```

Pass: the plan of the first command lists only W's trigger (packets: 1).
Answer `y`. After the write, stderr shows `Waiting for the keyboard to save
profile 1 (2 s)...`, then the lighting changes briefly: the restore switches
to profile 2 and back to prove that the keyboard saved the write. The report
says `written and verified (1 packet), persisted` and `Persistence check:
profile 1 selected again after switching away; 1 written table still holds
what was written (the keyboard saved it).` The dry run afterwards reports
nothing to write.

## H6. Light mode change and its restore

```text
openwraith set light --mode static --color 0000ff
openwraith restore before.owbackup.json
```

Answer `y` both times. Pass: the keyboard turns static blue, then back to
the backed-up effect (after the restore's 2 s wait the lighting briefly
shows profile 2 while the persistence check switches away and back). The
restore plan shows "main light with mode change (17 00, 07 00)": the
music-rhythm leave is sent only because the mode changes. The restore report
says `persisted` for the main light. No warning about another program
appears.

## H7. Restoring another profile than the active one

This is the step that failed on 2026-10-24 (run as H9 in that session's
numbering, the "hardware test H9" of PROTOCOL.md 4.5.3): the restore said
"written and verified" for Q in profile 2, switched back to profile 1 within
milliseconds, and the keyboard dropped the write. OpenWraith now waits for
the keyboard to save before every profile switch and proves the save.

```text
openwraith profile select 2
openwraith set trigger Q --actuation 2.00
openwraith profile select 1
openwraith restore all.owbackup.json --profile 2
openwraith profile
```

Answer `y` where asked. Expected output of the restore (shortened; the
simulator run of the same sequence prints exactly these lines):

```text
Profile 2
  Write                Packets  Change
  trigger of Q (r2c1)        1  2.00 mm, RT off -> 1.70 mm, RT off
...
Done: 1 table of 1 written and verified by reading them back.
  Profile  What                 Result
        2  trigger of Q (r2c1)  written and verified (1 packet), persisted
Persistence check: profile 2 selected again after switching away; 1 written table still holds what was written (the keyboard saved it).
Active profile: 1 (as before).
```

and on stderr, after the write, `Waiting for the keyboard to save profile 2
(2 s)...`. Q's value depends on what `all.owbackup.json` holds for profile 2
(factory: 1.70 mm). Pass: the lines above (`persisted`, the persistence
check without `LOST`), exit code 0; `profile` reports 1; `show triggers`
after `profile select 2` shows Q at its value from `all.owbackup.json` (then
`profile select 1` again). The lighting changes six times in all. Before
the question, the safety backup switches to profile 2 and back to profile 1
(two changes). After you answer `y` it changes four more times: to profile
2 (write), to profile 1 after the 2 s wait, to profile 2 (read back), to
profile 1. With `--yes` (H7b) all six follow each other without a pause for
the question. Do not touch the keyboard meanwhile: a profile switch on it
before the keyboard has saved makes the restore report `MAY BE LOST` and
exit with code 1.

### H7b. The exact failing scenario, checked from outside

Repeat the sequence that lost the write, then check profile 2 with an
all-profile backup made by a separate command (reads after a profile
switch come from what the keyboard saved):

```text
openwraith profile select 2
openwraith set trigger Q --actuation 2.00
openwraith profile select 1
openwraith restore all.owbackup.json --profile 2 --yes
openwraith backup --all-profiles -o after-h7.owbackup.json
openwraith restore all.owbackup.json --dry-run --against after-h7.owbackup.json --profile 2
```

Pass: the restore reports `persisted` for Q (as in H7), and the last
command reports nothing to write in profile 2 (on 2026-10-24 the same check
still showed Q at 2.00 mm). If the restore reports `LOST` instead, the
keyboard needs longer than 2 s to save: record the output and report it
(the restore exits with code 1; running it again or the printed undo
command recovers).

### H7c (optional). A switch on the keyboard during the wait

This checks that OpenWraith never reports a write as done when the
keyboard switched profiles before saving it, and it answers part of
PROTOCOL.md Q2 (does the keyboard report a switch made on it?). Start on
profile 1:

```text
openwraith set trigger W --actuation 1.30 --yes --verbose
```

As soon as stderr shows `Waiting for the keyboard to save profile 1
(2 s)...`, press Fn + 2 on the keyboard (within about half a second). Pass:
the command exits with code 1 and prints `FAILED: the active onboard profile
changed to 2 before the keyboard had saved the last write to profile 1
...`, the line `1 table written and verified, but the keyboard may not have
saved it ...`, `written and verified (1 packet), but MAY BE LOST` for W, and
`Active profile: 2`. It never says `Done` or `Waited 2 s`. Record whether
the command stopped right after the key press (the keyboard reported the
switch; stderr then also warns about another program, because such a
report looks like one, which is expected here) or only at the end of the
2 s (it did not; OpenWraith found the switch by reading the active
profile). Then:

```text
openwraith profile select 1
openwraith show triggers
openwraith restore before.owbackup.json
```

Record W's value from `show triggers` (the old value means the keyboard
dropped the write, as PROTOCOL.md 4.5.3 predicts), then answer `y` to the
restore if it plans anything.

## H8. Persistence

Unplug the keyboard, wait a few seconds, plug it in again.

```text
openwraith backup --all-profiles -o after-replug.owbackup.json
openwraith restore all.owbackup.json --dry-run --against after-replug.owbackup.json
```

Pass: nothing to write in any profile (the changes of H4-H7 and their
restores survived the power cycle; PROTOCOL.md Q1). A plain
`restore --dry-run` against the keyboard compares only the active profile, so
it cannot answer this for the other profiles.

Optional (power loss right after a command): run
`openwraith set trigger W --actuation 1.30 --yes`, unplug the cable as soon
as the command has returned (it returns only after its 2 s wait), plug it in
again and run `openwraith show triggers`. Pass: W at 1.30 mm. Then
`openwraith restore before.owbackup.json` (answer `y`).

## H9 (optional). Keymap group

Restore the pre-c09 backup `backups/openwraith-W75HEARGB-20261024-151752.owbackup.json`
(made at 15:17 UTC, before the vendor site wrote its knob defaults in c08/c09):

```text
openwraith restore backups/openwraith-W75HEARGB-20261024-151752.owbackup.json --dry-run --packets
openwraith restore backups/openwraith-W75HEARGB-20261024-151752.owbackup.json
```

On 2026-10-24 this restore wrote only the base keymap (`18 00`, 10
packets); after the 2 s wait the persistence check found it gone and the
restore reported `LOST` (exit code 1). Probes then showed that the keyboard
keeps the base keymap only when the Fn keymap (`18 02`) is written after it
(PROTOCOL.md 4.8.2). OpenWraith now writes the Fn keymap right after the base
keymap, unchanged when it already matches. Expected plan (the simulator run
of the same restore prints exactly these lines; other rows appear if other
tables differ):

```text
Profile 1 (active)
  Write                Packets  Change
  base keymap (18 00)       10  #16 (#16): 07 12 00 00 -> 00 00 00 00
                                ...
                                ... and 1 more entries
                                  18 00 00 00 38
                                  ...
                                  18 00 00 09 18
  Fn keymap (18 02)         10  unchanged, re-sent: the keyboard keeps the base keymap only when the Fn keymap is written after it
                                  18 02 00 00 38
                                  ...
                                  18 02 00 09 18
...
Total: 2 writes (20 packets) in profile 1.
```

and after `y`:

```text
Done: 1 table of 1 written and verified by reading them back; the Fn keymap (18 02) was also re-sent unchanged to make the keyboard save the base keymap.
  Profile  What                                                                           Result
        1  base keymap (18 00)                                                            written and verified (10 packets), persisted
        1  Fn keymap (18 02) re-sent unchanged to make the keyboard save the base keymap  re-sent in full, every packet acknowledged (10 packets)
Persistence check: profile 1 selected again after switching away; 1 written table still holds what was written (the keyboard saved it).
```

(The unchanged Fn keymap reads the same whether the keyboard kept it or not,
so the persistence check does not count it; what it proves is that the base
keymap was kept.)

Pass: the plan writes the base keymap with zeros at the knob entries (16-21)
and at 121, no defaults are added, and the Fn keymap follows it (10 + 10 =
20 packets, the `18 00` packets before the `18 02` packets); the report says
`persisted` for the base keymap and `every packet acknowledged` for the Fn
keymap, and exits with code 0. Then check that the knob still
controls the volume (the factory state had zeros there). If it does not, or
the restore reports `LOST`, run the printed undo command and report the
output.

## H10 (optional, later). Multi-key trigger mask

Needs an experimental option that does not exist yet
(`TriggerGrouping::SameSetting`, CODE-ONLY). When it exists: write one setting
to two keys in one packet, then read back both keys with `show triggers`.
Pass: both keys hold the setting, no other key changed.

## H11 (optional, uses the vendor site briefly). Read layouts

1. `openwraith backup -o before-h11.owbackup.json`.
2. Open the vendor site, set W's dead band to top 0.10 mm / bottom 0.20 mm and
   record a test macro (for example "A down, A up, B down, B up"), then
   **close the tab**.
3. `openwraith backup -o h11.owbackup.json` and
   `openwraith show triggers --from h11.owbackup.json` /
   `openwraith show macros --from h11.owbackup.json`.

Pass: the dead band of W reads `05 0a` at table offsets 92/93 (0.10 / 0.20 mm),
and the macro slot is in the canonical flat layout (the restore plan does not
block the keymap group). Then restore `before-h11.owbackup.json` and check
with `--dry-run` that nothing is left to write.

Optional macro-persistence check (PROTOCOL.md 4.8.2: whether macro slots
are kept without a following `18 02` is untested; OpenWraith always writes
an `18 02` after the macro slots of a keymap group, in the site's order:
macro slots, `18 02`, advanced-key slots):

4. `openwraith restore h11.owbackup.json --dry-run --packets`, then
   `openwraith restore h11.owbackup.json` and answer `y`. The plan writes
   W's dead band first (`26 01`, 5 packets; step 2 changed it, and step 3
   restored the state before), then the keymap group: the macro slot
   (`19 0n`, 5 packets) is its first write, followed by the base keymap
   (`18 00`, 10 packets) and the Fn keymap (`18 02`, 10 packets). The base
   keymap differs because every Apply of the site rewrites its knob
   defaults at base slots 16-21 (PROTOCOL.md 4.10), and the keyboard holds
   zeros there since H9; recording a macro reaches the keyboard only
   through such an Apply. So this check confirms that "a macro slot
   followed by `18 00` + `18 02` persists", not the macros-only
   precaution. If instead the group ends with `Fn keymap (18 02)` and the
   change `unchanged, re-sent as a precaution (untested guess): ...`
   (no base keymap), record it.
5. Pass: every changed item, the macro slot included, says `persisted`
   (exit code 0). Then unplug the keyboard, plug it in again and run
   `openwraith show macros`: the macro is still there and plays on its key.
   A macro slot reported `LOST` would mean that macros need more than the
   `18 02` after them: record the output.
6. Restore `before-h11.owbackup.json` again (answer `y`). Expected plan:
   the dead band, the macro clear, the base keymap (zeros at 16-21 again)
   and the Fn keymap after it. Check with `--dry-run` that nothing is left
   to write.

## v0.2 editors (H12-H25, CLI)

These tests check every write path that v0.2 adds before the desktop app is
built on it. They are ordered from the lowest risk up: a read-only monitor,
the key-depth test (nothing stored), then tables whose write form is
confirmed (colours, presets, dead band, keymaps), then the forms whose
effect or persistence is still open (macros, advanced keys, SOCD), the
power cycle that checks them all, and finally sharing and undoing.

Run `cargo test -p openwraith-cli sim_tests_v02 -- --nocapture` first: it
prints the plans and reports of the same commands against the simulator, so
you know what to expect.

**Before H12:** H0-H2 as above (vendor site closed, one keyboard), then

```text
openwraith backup --all-profiles -o v02-before.owbackup.json
```

This file is what H24 restores at the end. Profile 1 is the working
profile; profile 2 is the spare profile for the tables whose profile scope
is unknown (dead band, SOCD); profile 3 receives the import of H23 and the
optional H25. Changes made in H14-H21 stay on the keyboard until the power
cycle of H22 (unless a step says to undo them).

Every `set` command prints its plan, asks `[y/N]` and saves a safety backup
first; answer `y` unless the plan looks wrong. `--dry-run` shows the plan
without writing (read-only), `--packets` adds the hex of every packet.
After a write the command waits 2 s for the keyboard to save. With the
default `--check-persistence auto`, tables whose keeping is not proven on
hardware yet (macro slots, SOCD, advanced-key slots written without the Fn
keymap before them) are read again after a switch to the lowest other
profile and back: the lighting changes briefly, and the report says
`persisted` or `LOST`.

### H12. Monitor (read-only)

```text
openwraith monitor --duration 40s
```

While it listens: press Fn + 2, wait 3 s, press Fn + 1; press Fn + Right
Shift twice (no SOCD pair is stored yet); type a few letters.

Pass: lines `profile report: profile 2 is active (raw: 21 ..)` and then
profile 1; two lines `SOCD push: SOCD is now on/off (raw: 24 02 00 00 06 PP
..; Fn + Right Shift ...)`, the second one `off`; no `status report(s)
nobody here asked for` and no warning about another program. The command
sends nothing at all (not even a read). The `raw:` hex is what arrived
(the header, the declared payload and every byte up to the last non-zero
one), never rebuilt from the decoded fields. Record: the full raw bytes of
both profile pushes, not only the last byte (PROTOCOL Q2), that the SOCD
push also comes with no pair stored and that PP is the state after the
toggle (Q35). If SOCD ended on, press Fn + Right Shift once more and check
with `openwraith show socd`.

### H13. Live key depth (the travel test)

The test stores nothing, but the keyboard keeps streaming until it is
closed; `travel` closes it on every way out.

1. `openwraith travel --duration 60s`. Press W slowly to the bottom and let
   go, tap Esc fast, hold W and A together, then press Ctrl+C. Pass: the
   live line shows bars for the keys that move; after Ctrl+C `Key-depth
   test: stopped on request (Ctrl+C).` and `The keyboard acknowledged the
   close: the test is off.`, a table with W, Esc and A (deepest about
   3.40 mm at the bottom), exit code 0. Record the deepest values and
   whether slow presses update about 14 times a second (Q34).
2. `openwraith travel --duration 60s`, then switch to a text editor and type
   a sentence while the test is open (normal typing, Rapid Trigger on W if
   set; Q33). Back in the terminal, press Fn + 2. Pass: `the keyboard
   switched to profile 2 (Fn + 2); the test was closed`, exit code 0. Then
   `openwraith profile select 1`.
3. `openwraith travel --close` with no test open. Pass: `The keyboard
   acknowledged the close` and `No key-depth report arrived before it`: a
   close while closed is harmless (Q33).
4. A test left open by a crash: `openwraith travel --duration 120s`, then
   close the terminal window while it runs. In a new terminal run
   `openwraith monitor --duration 15s` and press some keys. Record whether
   `key-depth report(s): a key-depth test is open` lines appear (the test
   outlived the program; Q33). Then `openwraith set trigger W --actuation
   1.70 --dry-run` (allowed: read-only) and `openwraith set trigger W
   --actuation 1.70 --yes` while pressing keys: refused with `the keyboard is
   streaming key depths ... (openwraith travel --close). Nothing was
   written` (only if the stream was seen). Finally `openwraith travel
   --close` while pressing a key: `Before it, N key-depth report(s)
   arrived: a test was still open.`; a last `monitor --duration 10s` shows
   no more stream reports.

   Run step 4 only if normal typing worked in step 2. Before closing the
   terminal, have a way to type `openwraith travel --close` that does not
   depend on this keyboard: the Windows on-screen keyboard (`osk.exe`), a
   second keyboard, or a prepared `travel-close.cmd` (containing
   `openwraith travel --close`) started with the mouse. If none of these
   works, unplug and re-plug the keyboard and record whether that ends the
   stream (Q33).
5. `openwraith travel --duration 10s --json --keys W > w.ndjson` and press
   W: one JSON object per report, the last one `"event":"stopped"`.

### H14. Per-key colours (custom table 1)

```text
openwraith show light
openwraith set colors Left=0000ff Down=0000ff --dry-run --packets
openwraith set colors Left=0000ff Down=0000ff
openwraith show light
```

Pass: the plan lists only Left and Down, 8 packets `09 00`, `Every other
slot of the table (... #103 and #126) keeps its bytes`, no profile switch.
If the main light is not in the Custom mode the plan says so; run it with
`--custom-mode` then (the light switches to Custom). Left and Down light up
blue: record that the LED slot equals the key index for Left (123) and Down
(124) (Q8). `show light` afterwards: the slots `#103` and `#126` keep what
they held before. Keep the colours for H22.

### H15. Trigger presets

```text
openwraith set trigger W A S D --preset competitive
openwraith show triggers
```

Pass: 4 packets (one per key), W, A, S and D at 0.40 mm with Rapid Trigger
0.10 mm in `show triggers`; typing on them feels it. Undo with the printed
undo command (answer `y`).

### H16. Dead band (spare profile 2)

```text
openwraith profile select 2
openwraith set deadband W --top 0.10 --bottom 0.20
openwraith profile select 1
openwraith backup --all-profiles -o h16.owbackup.json
openwraith show triggers --from h16.owbackup.json --profile 2
openwraith show triggers --from h16.owbackup.json --profile 1
```

Pass: the plan warns `dead band may apply to every profile`, one write of
5 packets `26 01`, verified, no profile switch (the dead band is known to be
kept). Profile 2 shows W at `0.10 mm / 0.20 mm`. Record what profile 1
shows for W: the same (a shared table) or its own value (per profile):
the dead band's scope (Q3). Keep it for H22.

### H17. Base-layer key assignments

```text
openwraith set key Caps=L-Ctrl F12=media:mute --dry-run --packets
openwraith set key Caps=L-Ctrl F12=media:mute
openwraith show keymap
```

Pass: 20 packets (10 x `18 00`, then 10 x `18 02` with `unchanged,
re-sent: the keyboard keeps the base keymap only when the Fn keymap is
written after it`), no profile switch, `Not checked by switching profiles:
the keyboard is known from hardware tests to keep what was written`. Caps
acts as Ctrl (Caps + C copies), F12 mutes. Keep both for H22.

### H18. Fn-layer codes and built-in functions

```text
openwraith show keymap
openwraith set key --layer fn F1=media:0x24 F2=media:0x25 F3=mouse:0x08 F4=mouse:0x10 Q=mouse:wheel-up E=combo:L-Ctrl+C Z=mouse:left X=mouse:right C=mouse:middle
```

The second command is refused: F1-F4 have built-in Fn functions (`show
keymap` lists them). Run it again with `--allow-builtin F1 --allow-builtin
F2 --allow-builtin F3 --allow-builtin F4`. Pass: the plan says `Fn + F1
(#2) will no longer do its built-in function "media player" ...` for each,
names the unverified codes (Q14, Q37), writes only the Fn keymap (10
packets). Then, with Fn held, in a browser with a page history: Fn + F1 and
Fn + F2 (record which one goes back, Q14), Fn + F3 and Fn + F4 (record
which mouse button they are, Q37), Fn + Q scrolls up, Fn + E copies (a
one-modifier combination, Q37), Fn + Z / X / C click left / right / middle.
Plain F1-F4, Q, E, Z, X, C are unchanged; Fn + F5 still does its built-in.
`openwraith profile select 2`, `openwraith profile select 1`: the Fn entries
still work (Q12). Undo with the printed undo command.

### H19. Macros

1. A macro bound to a key, in the vendor site's order:

   ```text
   openwraith set macro Del --steps "A+ 100, A- 100, B+ 100, B-"
   openwraith show macros
   ```

   Pass: the plan writes `macro slot 0 (19 00)` (5 packets), the base keymap
   and the unchanged Fn keymap; the persistence check switches to profile 2
   and back and reports `persisted` for the slot and the base keymap. In a
   text editor Del types `ab`. `show macros` reads the slot back in the
   flat layout (`As steps: "A+ 100, A- 100, B+ 100, B-"`, `bound to Del
   (Base layer)`; Q13).
2. **A macro slot without a following Fn keymap** (does the keyboard keep
   it? Q1, Q36):

   ```text
   openwraith set macro --slot 0 --steps "A+ 50, A-, C+ 50, C-" --commit-trigger skip
   ```

   Pass: the plan writes only `macro slot 0 (19 00)`, says `--commit-trigger
   skip (hardware test)`, and the forced persistence check switches away and
   back. Record whether the slot is `persisted` (Del now types `ac`) or
   `LOST` (exit code 1: macros need the Fn keymap after them; run the same
   command without `--commit-trigger skip` to repair it).
3. Optional, play modes (Q13): `openwraith set macro Del --steps "A+ 50, A-"
   --play times:3` (one press types `aaa`?), then `--play toggle` and
   `--play while-held`. Record what each does.
4. Optional, a macro on the Fn layer (Q13): `openwraith set macro Q --layer
   fn --steps "X+, X-"` (Fn + Q types `x`?). Undo it with its printed undo
   command.

Keep the macro on Del (with the content the last step left) for H22.

### H20. Advanced keys

1. ```text
   openwraith set advanced E dks --point 1.5 --row E:t... --row R:.t..
   openwraith set advanced G mod-tap --hold L-Ctrl
   openwraith set advanced H toggle
   openwraith show advanced
   ```

   Pass: each plan writes the base keymap, the unchanged Fn keymap and then
   the slot (`22 0N`, `completes a binding`), no persistence check (slots
   after a complete Fn keymap are known to be kept). E: a light press types
   `e`, a full press `er` (as in c16). G: a tap types `g`; held longer than 300 ms it is
   Ctrl (G held + C copies; Q19). H: one press holds `h` (auto-repeat), the
   next releases it (Q19). `show advanced`: `3 of 40 slots used` with the
   three definitions and their keys.
2. **A lone advanced-key write without the Fn keymap** (Q36):

   ```text
   openwraith set advanced E dks --point 1.5 --row E:..t. --commit-trigger skip
   ```

   Pass: only `advanced-key slot 0 (22 00)` is written, then the forced
   persistence check. Record `persisted` or `LOST`, and what E now does
   (a tap when the key comes up from the bottom: stage 3, Q19).
3. The DKS press point's unit (Q19): `openwraith set advanced E dks --point
   0.5 --row E:t...`, then `--point 2.5`. Record whether E types much
   earlier, then much later.

Keep the slots for H22.

### H21. SOCD (spare profile 2)

```text
openwraith profile select 2
openwraith show socd
openwraith set socd --on --pair A/D
```

Pass: the plan shows `Power off -> on` and `1. A / D, mode 1: the last
input wins`, one write of 3 packets (`24 03`, two `24 00`), verified, then
the persistence check reports `persisted`. Record whether a note says `the
keyboard acknowledged packet 1, which it is not seen to acknowledge` (the
power write was acknowledged after all; Q35). Do not press Fn + Right
Shift until the command has finished: a switch on the keyboard during the
write or before the persistence check read SOCD again is reported as `SOCD
was switched on the keyboard ...` and the item stays `not checked`, which is
no evidence either way (run the command again for H21). In a key tester (a text
field, or a website that shows held keys): hold A, then press D: with mode
1 only D counts; let go of D: A again. `openwraith set socd --pair A/D:4`
(mode 4) and repeat; record what changes (Q35). Press Fn + Right Shift:
`openwraith show socd` shows `Power: off`; `openwraith set socd --on` turns
it on again and keeps the pair. Then

```text
openwraith backup --all-profiles -o h21.owbackup.json
openwraith show socd --from h21.owbackup.json --profile 1
openwraith profile select 1
```

Record whether profile 1 shows SOCD on with A / D too (shared) or not (per
profile): the SOCD scope (Q3). Keep SOCD on in profile 2 for H22.

### H22. USB power cycle (persistence of everything above)

```text
openwraith backup --all-profiles -o before-replug.owbackup.json
```

Unplug the keyboard, wait 10 s, plug it in again.

```text
openwraith backup --all-profiles -o after-replug.owbackup.json
openwraith restore before-replug.owbackup.json --dry-run --against after-replug.owbackup.json
```

Pass: nothing to write in any profile: custom colours 1 (H14), the dead
band (H16), the base keymap (H17), the macro slot (H19), the advanced-key
slots (H20) and SOCD (H21) survived the power loss (Q1). Check the
behaviour too: Left and Down blue, Del plays its macro, E is a DKS, SOCD on
in profile 2 (Fn + 2, hold A and D). Record every table the dry run lists.

### H23. Sharing a profile

```text
openwraith profile export 1 -o p1.owprofile.json --label "H23"
openwraith profile select 3
openwraith profile import p1.owprofile.json --into 3 --dry-run
openwraith profile import p1.owprofile.json --into 3 --include keys --dry-run --packets
openwraith profile select 1
openwraith profile import p1.owprofile.json --into 3
openwraith profile import p1.owprofile.json --into 3 --include keys
```

A dry run reads only the active profile, so profile 3 is selected for the
two dry runs (with profile 1 active they would only say `not compared`).

Pass: `p1.owprofile.json` has `"usb": null` and the label in its note. The
first import writes only triggers and lighting into profile 3 (`Import
plan`, `profile 1 of the file is imported into profile 3 of the keyboard`,
the other categories `not imported`), switches to profile 3 and back, and
reports `persisted`. The `--include keys` import first lists the file's
macros as steps, then writes profile 3's keymap group in the vendor site's
order (macro slot, base keymap, Fn keymap, advanced-key slots) and reports
`persisted`. On profile 3 (Fn + 3) Caps, Del, E, G and H behave as on
profile 1. `openwraith profile select 1`.

### H24. Restoring advanced keys and SOCD (undo)

```text
openwraith backup --all-profiles -o v02-after.owbackup.json
openwraith restore v02-before.owbackup.json --dry-run --packets
openwraith restore v02-before.owbackup.json
openwraith restore v02-after.owbackup.json --profile 2
openwraith restore v02-before.owbackup.json --profile 2
openwraith restore v02-before.owbackup.json --dry-run
```

Pass: the first restore clears what H14-H23 changed: the advanced-key
slots (`22 0N` clears after the Fn keymap), the macro slot, the base keymap
with its Fn keymap, custom colours 1, the dead band and SOCD of profile 2
(`SOCD power off and pairs (24 03, 24 00)`, `power on -> off`), the
imported settings of profile 3; every item `persisted`, the final check
passes, exit code 0. The restore of `v02-after.owbackup.json --profile 2`
writes SOCD **on with the A / D pair** through the restore path (W11 with
content) and reports `persisted`; restoring profile 2 of
`v02-before.owbackup.json` again turns it off. The last dry run reports
nothing to write. (Restoring profile 1 of `v02-after` the same way checks
the restore of non-empty advanced-key slots, W10; restore `v02-before`
afterwards.)

### H25 (optional, spare profile 3). A critical Fn function replaced

```text
openwraith profile select 3
openwraith set key --layer fn 2=media:mute --allow-critical-builtin 2
```

(`--allow-builtin 2` alone is refused: Fn + 2 switches profiles.) On profile
3, press Fn + 2: record whether it mutes or still switches to profile 2
(Q14). Then on profile 3 run `openwraith set key --layer fn 2=default`
(no acknowledgement needed; it brings the built-in back) and check that
Fn + 2 switches profiles again. Finally `openwraith restore
v02-before.owbackup.json --dry-run` reports nothing to write.

## Desktop app (A1-A10, Phase 3)

The desktop app uses the same library, so H0-H11 stay valid. These checks
cover what the app adds: the long-lived session, the listen-only start, the
confirmation flow and the progress display. Use a release build of the app
(`npm --prefix app run tauri build`, then the installed app or
`target\release\openwraith-app.exe`); never run the CLI while the app is
open. Same preconditions as above (vendor site closed, cable, one keyboard).
Safety backups go to the same folder as the CLI's.

- **A1. Connect, observe, read.** Start the app with the keyboard plugged
  in. The header shows "Checking that no other program is using the
  keyboard" for about 2 s, then reading, then ready with the firmware.
  Compare the actuation and lighting pages with `openwraith show all`
  (run after closing the app). Pass: identical values.
- **A2. Trigger edit and Undo.** Select W, A, S, D; set 1.20 mm and Rapid
  Trigger 0.30 mm; review and write. The progress shows "Waiting for the
  keyboard to save". Then Undo from the report. Pass: the keys work as set,
  then as before; two safety backups in the folder.
- **A3. Separate press and release.** On one key, set Rapid Trigger with
  separate press 0.30 mm and release 0.50 mm. Pass: written; `openwraith
  show triggers` (app closed) shows `0D` with both values.
- **A4. Light mode change and Undo.** Change the effect (e.g. Custom to
  Static blue), write, check visually, Undo. Pass: both changes visible and
  kept after a profile switch on the keyboard and back.
- **A5. Profile switch right after an edit.** Make a trigger edit and switch
  the profile in the header at once. Pass: the edit is kept (switch back and
  check); the edit itself waited for the save before it finished.
- **A6. All-profile backup.** Back up all four profiles. Pass: the lighting
  changes briefly through profiles 2-4 and returns; the file loads in
  `openwraith show all --from FILE --profile 3`.
- **A7. Restore of another profile.** Change something on profile 2 with the
  CLI or the app, then restore the A6 backup for profile 2 only from profile
  1. Pass: the report says "kept after a profile switch" (persisted).
- **A8. Vendor site open.** Open wraith.software and connect it while the app
  runs, once while it is idle and once right after pressing "Read again"
  (while it reads). Pass: the app shows "Another program is talking to the
  keyboard", writing is paused and nothing is sent (a read in progress stops
  at once instead of finishing); after closing the tab completely, "I closed
  it, check again" returns to ready.
- **A9. Fn + 2 while idle.** With the app idle and focused elsewhere, press
  Fn + 2 on the keyboard. Note whether the app notices at once (a profile
  report, PROTOCOL Q2) or only when its window gets the focus again (after
  30 s). Pass: the header shows profile 2 in either case, and a plan made
  before is refused as out of date.
- **A10. Close during a restore; unplug during the save wait.** Start a
  restore of two profiles and try to close the window: the close guard
  appears; "Keep open" lets it finish. Then start a trigger edit and unplug
  the cable while "Waiting for the keyboard to save" is shown. Pass: the
  report says the change may not have been saved (MAY BE LOST), the app
  shows the keyboard as disconnected and reconnects when plugged in again.

## Reporting

For each test record: pass or fail, the full output, and for a failure the
safety backup path and the undo command that was printed. Attach the
`--verbose` statistics of any command that warned about another program.

## Results log

Wraith W75, PID `0xC368`, firmware V3.22.07, Windows 11, USB cable.

| Date | Test | Result | Notes |
|---|---|---|---|
| 2026-10-24 | H0-H1 (list, info, backup, dry runs) | pass | 0 writes, no foreign traffic |
| 2026-10-24 | H2 (all-profile backup) | pass | 4 profile selects, back on profile 1 |
| 2026-10-24 | H3 (nothing to restore) | pass | |
| 2026-10-24 | H4-H5 (trigger edit and its restore) | pass | W 1.70 -> 1.20 -> 1.70 mm |
| 2026-10-24 | RT modes 0C / 0D / 00 on W | pass | each 1 packet, verified |
| 2026-10-24 | H6 (light: brightness 2, static blue, restore) | pass | confirmed visually; `17` + `07` on the mode change |
| 2026-10-24 | H7 (restore of another profile) | **fail** -> fixed | write lost on the immediate switch back; fixed by the commit settle (`b7c861c`), PROTOCOL 4.5.3 |
| 2026-10-24 | H7b (same scenario again) | pass | `persisted`; independent all-profile check: nothing to write |
| 2026-10-24 | H9 (base keymap restore) | **fail** -> fixed | base keymap alone not saved; fixed by the `18 02` commit trigger (`5542432`), PROTOCOL 4.8.2 |
| 2026-10-27 | H9 (again) | pass | 20 packets (base + Fn re-sent), `persisted`; knob and Fn shortcuts work with all knob slots back at `00` |
| 2026-10-27 | H8 (USB power cycle) | pass | all four profiles byte-identical before and after |
| 2026-10-27 | A1 (install, connect, observe, read) | pass | NSIS installer, no admin; 2 s listen-only check, then ready with FW 3.22.07 "Tested"; app state matched the CLI baseline taken just before |
| 2026-10-27 | A2 (trigger edits) | pass | W, A, S, D 1.70 -> 0.10 mm, then 1.70 mm + RT 0.10 mm; each safety backup held the previous state; kept after the A6 profile round trip |
| 2026-10-27 | A3 (RT modes on Q) | pass | `0D` 0.30 / 0.50, `0C` 0.30, `00`; each read back from the next safety backup |
| 2026-10-27 | A4 (light mode change and Undo) | pass | Custom -> Breath kept after the A6 round trip; Undo by restoring the `set-light` safety backup brought Custom back (`persisted`) |
| 2026-10-27 | A5 (switch right after an edit) | pass | Q RT 0.30 kept after an immediate switch to profile 2 and back |
| 2026-10-27 | A6 (all-profile backup) | pass | lighting went through profiles 2-4 and back; against the CLI baseline only the edits made in A2/A4 differed |
| 2026-10-27 | A7 (restore profiles 2 and 3 from profile 3) | pass | switch consent, 2 light writes, both `persisted`; found a false "probably shared" warning (custom colours 1, also dead band in the simulator), fixed in the planner |
| 2026-10-27 | A8 (vendor site connected) | pass | "Another program is using the keyboard", writing paused, values marked stale; "I closed it, check again" returned to ready; an all-profile backup afterwards showed the site wrote nothing |
| 2026-10-27 | A9 (Fn + 2 while idle and unfocused) | pass | the app followed at once (the W75 pushes the profile report, PROTOCOL Q2) and discarded the profile-1 draft with a notice |
| 2026-10-27 | A10 (unplug during the save wait) | pass | report "written and verified, but may not have been saved", disconnected, Undo disabled until reconnect; after reconnect Q still had RT 0.30 mm: the write was really lost (PROTOCOL 4.5.3). The close-guard half was not run on hardware |
