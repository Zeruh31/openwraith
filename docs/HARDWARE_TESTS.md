# Hardware tests

Every automated test runs against captures or the device simulator; none of
them opens a HID device. The commands that write have therefore only been
checked against the simulator. This checklist is run **by hand** on a real
Wraith W75, in order: H0-H11 for the Phase 2 writes, H12-H25 for the v0.2
editors of the CLI, H26-H27 for the fixes made after them, A1-A10 for the
desktop app, A11-A22 for its v0.2 editors, A23-A35 for what runs in the
background (tray, notifications, automatic backups, recovery, game
profiles, the device lock shared with the CLI). Record the full output of every command (add `--verbose` for
session statistics) and keep every file it writes.

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

Optional macro-persistence check (H19 step 2 later showed that macro slots
are kept without any `18 02`, PROTOCOL.md 4.8.2; OpenWraith keeps the
site's order for a keymap group: macro slots, base keymap, `18 02`,
advanced-key slots):

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
   followed by `18 00` + `18 02` persists". If instead the group writes
   only the macro slot (no base keymap), it sends no Fn keymap at all:
   macro slots are saved on their own (H19 step 2).
5. Pass: every changed item, the macro slot included, says `persisted`
   (exit code 0). Then unplug the keyboard, plug it in again and run
   `openwraith show macros`: the macro is still there and plays on its key.
   A macro slot reported `LOST` would contradict H19 and H22: record the
   output.
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

**Status:** H12-H24 were run on 2026-10-27 with the CLI of commit `118040f`
(results log below; the findings are in PROTOCOL.md 7.5). H13 step 4 and the
optional H25 were not run. The expected output quoted in H12-H24 is that
CLI's. What the session found changed it: macro slots, SOCD and every Fn-layer
code tested in H18 are now known to work and to be kept, so their "not yet
verified" and "across a power loss" notes are gone and `set` no longer runs
the persistence check for them by default; the dead band and the SOCD pairs
are per profile and the SOCD power is one setting of the whole keyboard; a
macro slot-0 write re-sends slots 1-9; the Fn keymap is written before
advanced-key slots as a confirmed requirement and no longer after macro-only
changes. H26 and H27 re-check these fixes on the keyboard.

**Before H12:** H0-H2 as above (vendor site closed, one keyboard), then

```text
openwraith backup --all-profiles -o v02-before.owbackup.json
```

This file is what H24 restores at the end. Profile 1 is the working
profile; profile 2 is the spare profile for the tables whose profile scope
was unknown before this session (dead band, SOCD; H16 and H21 showed the
dead band and the SOCD pairs per profile and the SOCD power keyboard-wide);
profile 3 receives the import of H23 and the optional H25. Changes made in
H14-H21 stay on the keyboard until the power cycle of H22 (unless a step
says to undo them).

Every `set` command prints its plan, asks `[y/N]` and saves a safety backup
first; answer `y` unless the plan looks wrong. `--dry-run` shows the plan
without writing (read-only), `--packets` adds the hex of every packet.
After a write the command waits 2 s for the keyboard to save. With the
default `--check-persistence auto`, tables whose keeping was not proven on
hardware (in the CLI of this session: macro slots, SOCD, advanced-key slots
written without the Fn keymap before them) are read again after a switch to
the lowest other profile and back: the lighting changes briefly, and the
report says `persisted` or `LOST`. Since this session only advanced-key
slots written without the Fn keymap before them are checked this way
(`--commit-trigger skip` forces the check anyway); `--check-persistence
always` checks every written table.

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
   1.60 --dry-run` (allowed: read-only) and `openwraith set trigger W
   --actuation 1.60 --yes` while pressing keys (1.60, not W's current 1.70,
   so that there is something to write): refused with `the keyboard is
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

Pass: one write of 5 packets `26 01`, verified, no profile switch (the dead
band is known to be kept). Profile 2 shows W at `0.10 mm / 0.20 mm`. Record
what profile 1 shows for W: the same (a shared table) or its own value (per
profile): the dead band's scope (Q3). Keep it for H22. (On 2026-10-27 the
plan still warned `dead band may apply to every profile`; profile 1 showed
`0.00 mm / 0.00 mm`: the dead band is per profile, and the note is gone.)

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
F2 --allow-builtin F3 --allow-builtin F4`. Pass: the refusal names the key
(`Fn + F1 (#2) is the built-in function "media player"`; the CLI of
2026-10-27 said `Fn + key #2`); the plan says `Fn + F1 (#2) will no longer
do its built-in function "media player" ...` for each, writes only the Fn
keymap (10 packets) and, since this test confirmed them, names no
unverified code (on 2026-10-27 it still named Q14 and Q37). Then, with Fn held, in a browser with a page history: Fn + F1 and
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

## Re-tests after the H12-H24 findings (H26, H27, CLI)

The session H12-H24 found two firmware effects that OpenWraith did not
model (PROTOCOL.md 7.5): a write to macro slot 0 empties macro slots 1-9 of
the active profile, and the SOCD power is one setting of the whole keyboard.
The code now handles both; these two tests check that on the keyboard
(about 10 minutes). Same preconditions as H12; run their simulator version
first: `cargo test -p openwraith-cli hardware_tests_h26_and_h27 --
--nocapture` runs the same commands in the same order against the
simulator and prints their output.

```text
openwraith backup --all-profiles -o before-h26.owbackup.json
```

### H26. Macro slot 0 re-sends slots 1-9 (spare profile 4)

```text
openwraith profile select 4
openwraith set macro --slot 1 --steps "B+, B-"
openwraith set macro --slot 2 --steps "C+, C-"
openwraith set macro --slot 0 --steps "A+, A-" --dry-run
openwraith set macro --slot 0 --steps "A+, A-"
openwraith show macros
openwraith set macro --slot 0 --clear
openwraith show macros
```

Pass: the first two commands write only their own slot (5 packets each, no
Fn keymap: macro slots are saved on their own). The slot-0 plan lists
`macro slot 0 (19 00)`, then `macro slot 1 (19 01)` and `macro slot 2
(19 02)`, each `re-sent: writing macro slot 0 makes the keyboard clear slots
1-9; written even after Ctrl+C` (15 packets, in that order, no keymap
write); it changes only slot 0 (`macro slot 0: empty -> A+, A- (Once)`).
After it, `show macros` lists slots 0, 1 and 2 with their steps. The
`--clear` of slot 0 re-sends slots 1 and 2 the same way, and afterwards
slots 1 and 2 still hold `B+, B-` and `C+, C-`. A slot 1 or 2 shown empty
means that the re-send did not work: record the output and restore
`before-h26.owbackup.json` (the restore re-sends them too).

### H27. Multi-profile restore with the keyboard-wide SOCD power

```text
openwraith profile select 2
openwraith set socd --on --pair A/D
openwraith profile select 3
openwraith set socd --pair Left/Right
openwraith profile select 4
openwraith backup --all-profiles -o before-h27.owbackup.json
openwraith restore before-h26.owbackup.json --dry-run --against before-h27.owbackup.json --packets
openwraith restore before-h26.owbackup.json
openwraith show socd
openwraith backup --all-profiles -o after-h27.owbackup.json
openwraith restore before-h26.owbackup.json --dry-run --against after-h27.owbackup.json
openwraith profile select 1
```

This is the case that stopped H24: SOCD on in every profile (one setting),
pairs in two profiles (A / D in profile 2, Left / Right in profile 3),
another profile active, and H26's macros in profile 4. In H24 the second
SOCD write was refused because the first one had already switched the power
of every profile. A dry run without `--against` reads only the active
profile (profile 4) and would plan the power change there; against the
all-profile backup made just before it, it compares every profile like the
real restore. Pass: the first `set socd` plan says that the power changes
every profile; the second one changes only profile 3's pairs (power `on ->
on`). The dry run and the plan the restore prints show two SOCD writes,
profile 2 and profile 3 (each `24 03 00 00 01` then two `24 00`
chunks), and `SOCD power (all profiles): on -> off` once, in profile 2's;
profiles 1 and 4 get no SOCD write. Profile 4 empties macro slots 1 and 2
(`2 step(s) -> empty`; slot 0 is already empty after H26's `--clear`). The
restore finishes with exit code 0: no `changed after planning`, every
written item `persisted`, and a final check `the SOCD power, one setting of
the whole keyboard, read again in profile 4: it holds the restored power`.
`openwraith show socd` shows `Power (all profiles): off` and no pair; the
last dry run reports nothing to write in all four profiles. Do not press Fn
+ Right Shift during the restore.

Optional, H13 step 4 (a key-depth test left open by a crashed program) as
described there.

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

## Desktop app editors (A11-A22, Phase 5b-1)

The v0.2 editors of the app (Keys, Actuation > Presets / Dead band / SOCD /
Key depth, Lighting > Key colours, Backup & profiles > Share a profile) use
the library write paths that H12-H27 checked with the CLI. These tests check
what the app adds on top: the plan in words, the order of the writes and
their reasons, the reports and their Undo, the stops of the live view and
the SOCD push. So far these screens have run only against the simulator and
recorded fixtures. The tests are ordered from the lowest risk up.

Build with `pwsh tools/release-build.ps1`, then install
`target\release\bundle\nsis\OpenWraith_0.2.0_x64-setup.exe` (it replaces
the A1 install; Settings > About shows 0.2.0, the build with the **Keys**
page) or run `target\release\openwraith-app.exe`. A11-A35 run on this same
0.2.0 install; older installers (0.1.0) may still be in
`target\release\bundle\nsis`, do not take one of them. Same
preconditions as above (vendor site closed, cable, one keyboard); never run
the CLI while the app is open. To see every screen of this section first
without the keyboard, run the preview (`npm --prefix app run dev:preview`,
http://localhost:1421, the **Preview** button lists the scenarios).

Profile 4 is the spare profile for the edits and profile 3 receives the
import (as in H26 and H23); profiles 1 and 2 are not edited. Every write
opens its review first: tick "Every wraith.software tab is closed", read the
plan, wait until the write button (**Write to keyboard**, or **Restore …** /
**Import …**) is armed (about half a second), then click it. "Check that
the keyboard kept it" is never pre-ticked; leave it off unless a step says
otherwise. Every report has **Undo…**: it opens the restore review of the
safety backup saved just before that write (in the CLI's folder,
`%LOCALAPPDATA%\OpenWraith\safety-backups`; the Safety backups section of
Backup & profiles lists them too). If anything looks wrong, stop and use it.
A21 puts all four profiles back at the end anyway; if the app itself
misbehaves, close it and run `openwraith restore a11-start.owbackup.json`.

- **A11. Setup and all-profile backup.** With the app closed:

  ```text
  openwraith backup --all-profiles -o before-a11.owbackup.json
  openwraith profile select 4
  openwraith show macros
  openwraith set macro --slot 0 --steps "Z+, Z-"
  openwraith profile select 1
  ```

  The `set macro --slot 0` gives Macro 1 (slot 0) of profile 4 a macro for
  A18: the app writes Macro 1 only when it already holds one (a new macro
  goes into Macros 2-10 first). Skip it if `show macros` already lists a
  macro in slot 0; if slots 1-9 hold macros, its plan re-sends them (H26).
  Optional and read-only: `openwraith monitor --duration 20s` while you
  press Fn + 3, Fn + 4 and Fn + 1; record the profile pushes (the labels of
  Fn + 3 and Fn + 4 are still unverified). Then start the app. Pass: no
  "Simulator" chip in the header; the nav reads Keyboard · Keys · Actuation
  · Lighting · Backup & profiles · Settings; Keys shows profile 1's
  assignments ("Read at …") and its tip once. Switch to profile 4 in the
  header (switch consent), then Backup & profiles > **Back up all four
  profiles…**, saved as `a11-start.owbackup.json`. Pass: the lighting goes
  through the other profiles and back to 4. This file is the rollback point
  of A12-A20.
- **A12. Key depth view (nothing stored).** Actuation > Key depth.
  1. Tick the vendor box, **Start**. Press W slowly to the bottom, tap Esc,
     hold W and A. Pass: the header shows **Live** and the profile switcher
     is disabled; the caps fill from the top with their depth (about
     3.40 mm at the bottom), the deepest point stays with a "max" chip, the
     followed key gets a gauge and a 5-second strip chart; slow presses
     update about 14 times a second. While the app has the focus, the
     pressed keys neither scroll the page nor press buttons. The marker
     `%LOCALAPPDATA%\io.github.zeruh31.openwraith\state\travel-test-open`
     exists while the view runs. Switch to Notepad and type a sentence:
     typing works (Q33). Back in the app, **Stop**. Pass: "Stopped.", the
     pill is gone, the marker file is gone.
  2. **Start**, then click the **Dead band** tab. Pass: the **Live** pill
     disappears at once, the profile switcher works again, no "The key-depth
     view may still be on" banner; back on Key depth it says "Not running."
  3. **Start**, minimise the window (Windows + Down or the title bar),
     wait 3 s, bring it back. Pass: "Stopped because the window was
     hidden."
  4. **Start**, press Fn + 2. Pass: "Stopped: the keyboard switched
     profiles.", the header shows profile 2. Switch back to profile 4 in
     the header.
  5. Optional, a view left open by a crash: **Start**, then end OpenWraith
     in Task Manager (End task) while the view runs. Type in Notepad
     (record whether it works). Start OpenWraith again. Pass: the banner
     "The key-depth view may still be on in the keyboard" with **Go to Key
     depth**; on that tab tick the vendor box and click **Stop it**: "The
     keyboard's key-depth view is stopped.", the banner is gone. If no
     banner appears, record it, close the app and run `openwraith travel
     --close` (a close while closed is harmless, H13 step 3).

  Afterwards, with the app closed, `openwraith monitor --duration 10s`
  while pressing keys shows no key-depth reports.
- **A13. Key colours and Undo (profile 4).** Lighting > Key colours. Click
  Left, Down, Up and Right (a click toggles a key), pick blue in the colour
  panel, **Fill selection**. Pass: the four caps turn blue in the draft and
  each key's description reads "Now …; after writing blue, #0000FF (not
  written yet)". Press Ctrl+Z (the draft goes back, announced) and Ctrl+Y
  (blue again). If the effect is not Custom, a callout names the current
  effect: tick "Also switch the effect to Custom when writing" (never
  pre-ticked). **Review and write…**. Pass: the Now / After writing mini
  boards, one change group (4 keys to blue), "Key colours (table 1)" in
  the technical details, the switch to Custom only if ticked; after the
  write the four LEDs are blue (LED slot = key index, H14) and the other
  keys keep their colours. Then **Undo…** > **Restore …**: the colours (and
  the effect) are back as before; "Other stored colours" kept as read are
  unchanged.
- **A14. Dead band (profile 4).** Actuation > Dead band. Select W, set the
  top dead zone to 0.10 mm and the bottom one to 0.20 mm; the gauge shows
  both zones. **Review and write…**. Pass: "Change the dead band of 1 key
  on profile 4?", no "all profiles" badge, one write. Tick "Check that the
  keyboard kept it" here (the one test that exercises it): the report says
  the change was kept after a switch to profile 1 and back (the lighting
  changes briefly). After **Read again** W shows 0.10 / 0.20 mm and still
  types normally. **Undo…** > **Restore 1 change**: W back to its old
  values.
- **A15. SOCD, with Fn + Right Shift while the tab is open (profile 4).**
  Actuation > SOCD. Before editing: the switch carries "Applies to all
  profiles", the pairs are titled "Pairs on profile 4", the mode menu marks
  modes 2 and 3 "Not yet verified". Do not press Fn + Right Shift while a
  write runs.
  1. Turn SOCD on and click the quick pair **A + D** (mode 1, Last input
     wins). **Review and write…**. Pass: "SOCD: off → on" with the note
     that this changes it on every profile, then the pair on profile 4;
     written, verified. In Notepad: hold A (a repeats), press D: only d
     repeats; let go of D: a again.
  2. With no draft and the tab open, press Fn + Right Shift. Pass: the toast
     "SOCD turned off on the keyboard (Fn + Right Shift).", the switch shows
     off and the note "SOCD was turned off with Fn + Right Shift. The switch
     shows it now." Press it again: "turned on", the switch and the note
     follow (the note never contradicts the switch).
  3. Set pair 1 to mode 4, **Review and write…**, and while the review is
     open (before Write) press Fn + Right Shift. Pass: the review says
     "This plan is out of date: …" and cannot write. Close it, press Fn +
     Right Shift again (on), review again: SOCD stays on, pair 1 mode 1 → 4.
     Write; record what mode 4 does with A and D held (H21: as named).
  4. Roll back: Backup & profiles > Safety backups, restore the SOCD safety
     backup made before step 1 (the older one). Pass: SOCD off on every
     profile, no pair on profile 4.
- **A16. Trigger presets (profile 4).** Actuation > Actuation & Rapid
  Trigger > **Presets…**: five built-in presets with their values. Choose
  **Competitive**, apply to **WASD**. Pass: the review says "Preset:
  Competitive", W, A, S, D to 0.40 mm with Rapid Trigger 0.10 mm, one
  packet per key (H15); written; typing on them feels it. **Undo…** brings
  the old values back. Then select W, set 1.20 mm and Rapid Trigger 0.30 mm
  in the editor (draft only), **Presets…** > **Save as a preset**, name
  "A16": it is listed under "Your presets (this PC)" and Settings > Trigger
  presets counts it. Delete it and **Discard** the draft. Saving and
  deleting send nothing to the keyboard.
- **A17. Base-layer remap and Undo (profile 4).** Keys > Assignments, Base
  layer. Select Caps, **Change…**, Keys tab, Left Ctrl; select F12,
  **Change…**, Media, Mute. Pass: the caps show the new action in the
  accent colour with the original as a small second line; the selected-key
  panel shows Now and After writing. **Review and write…**. Pass: Layer |
  Key | Now | New with both keys; "What OpenWraith writes, in this order":
  the key assignments, then the Fn layer marked "Unchanged" with the reason
  that the keyboard keeps changed assignments only when a complete Fn layer
  follows them (20 packets in all, H17); no profile switch. After the
  write: Caps + C copies in Notepad, F12 mutes; Fn + 1: Caps is Caps Lock
  again on profile 1 (per profile); back to profile 4. **Undo…** >
  **Restore …**: Caps and F12 as before. Checks without writing: the Fn key
  can't be selected (its tooltip says why); the knob's popover lists its
  entries "Kept as read"; on the Fn layer, selecting 2 shows the danger
  callout "Fn + 2 … switch to profile 2 while profile 4 is active" with a
  checkbox, and **Change…** stays disabled until it is ticked. Do not tick
  it; **Discard**.
- **A18. A recorded macro, then a Macro 1 write with Macros 2 and 3 in use
  (profile 4).** Keys > Macros; Macro 1 shows A11's macro (`Z`, "No key
  runs this macro") with the note that writing Macro 1 empties Macros
  2-10.
  1. **New macro…**, key Base layer PgUp, **Record**, type `ab`; the
     recording stops 2 s after the last key (steps: press A, release A,
     press B, release B; waits as typed). Plays once. A second **New
     macro…** on PgDn, recording `c`. **Review and write…**. Pass: "Change
     macros on profile 4?"; the two macros go into Macros 2 and 3 (record
     the slots the plan names; Macro 1 is not written), then the key
     assignments and the unchanged Fn layer. After the write, "Try it":
     PgUp types `ab`, PgDn types `c`.
  2. **Edit** Macro 1: record `x` in place of its steps and let it run from
     Home (the key option in the editor). **Review and write…**. Pass: the
     plan writes Macro 1, then Macro 2 and Macro 3 marked "Unchanged,
     written again: writing Macro 1 makes the keyboard clear Macros 2–10.",
     then the key assignments with the Fn layer. Write. Pass: the report
     lists no emptied macro and no key left on an empty slot; Home types
     `x`, PgUp still `ab`, PgDn still `c`; after **Read again** Macros 2
     and 3 still hold their steps. A Macro 2 or 3 shown empty, or PgUp /
     PgDn doing nothing, means the re-send did not work: record the report
     and use **Undo…** (the restore re-sends them too).

  Keep the three macros for A20.
- **A19. An advanced key (DKS), then its deletion (profile 4).** Keys >
  Advanced keys > **Dynamic keystroke (DKS)**. Layer Base, key E, first
  point 1.5 mm, quick start "A light press types one key, a full press
  another", then E and R as its keys. Pass: the sentence below the grid
  says that a light press (1.5 mm) taps E and a full press taps R; the
  slot number appears only under
  Technical details. **Add to draft**, **Review and write…**. Pass:
  "Change advanced keys on profile 4?"; the key assignments, then the Fn
  layer marked "Unchanged" with the reason that the keyboard keeps advanced
  keys only when a complete Fn layer comes before them, then the
  advanced-key slot. After the write: a light press on E types `e`, a full
  press `er` (H20). Then **Delete** on its card ("Will be deleted"),
  **Review and write…**: E back to Default and the slot cleared in one
  plan. Pass: E types `e` normally and the card is gone. Optional: **Tap
  or hold** on G, hold Left Ctrl, 300 ms (tested): a tap types `g`, a hold
  is Ctrl; delete it the same way.
- **A20. Export a profile and import it into another.** Backup & profiles >
  Share a profile > Export: profile 4 (active, a pure read, no consent),
  name "A20", **Export profile 4…**, save `p4.owprofile.json`. Pass: "Profile
  file saved"; the file has `"usb": null` and "Label: A20" in its note.
  Import: **Open a
  profile file…** `p4.owprofile.json`. Pass: the summary card lists A18's
  macros as steps ("Macro 2 · 4 steps · Plays once: Press A, …"). Import
  into profile 3, keep "Actuation and Rapid Trigger" and "Lighting and key
  colours" and also tick "Key assignments, macros and advanced keys";
  **Compare and review…** > switch consent > **Switch and compare**. Pass:
  "Import profile 4 of the file into profile 3?" with what changes, no
  critical Fn callout. **Import N changes**. Pass: "Import complete. … kept
  by the keyboard (checked after a profile switch)", the keyboard back on
  profile 4. Fn + 3: PgUp types `ab`, PgDn `c`, Home `x`; back to profile
  4 in the header.
- **A21. Final all-profile compare.** Backup & profiles > Restore: **Open
  file…** `a11-start.owbackup.json`, all four profiles, **Compare with
  keyboard…** (switch consent). Pass: the review lists only what is left
  from A18-A20 (profile 4's macros and keys, profile 3's import) and
  whatever an earlier step did not undo; profiles 1 and 2 have nothing to
  write. **Restore N changes**. Pass: every item written, verified and kept
  (checked after a profile switch), the keyboard back on profile 4.
  Compare again: nothing to restore in any profile. Close the app,
  then

  ```text
  openwraith backup --all-profiles -o after-a21.owbackup.json
  openwraith restore a11-start.owbackup.json --dry-run --against after-a21.owbackup.json
  openwraith restore before-a11.owbackup.json
  ```

  Pass: the dry run finds nothing to write in all four profiles; the last
  restore removes A11's Macro 1 again (if A11 seeded it) and ends with a
  clean final check.
- **A22 (optional). Narrator.** With Narrator on: Keys > Assignments (the
  board, the selected-key panel, the action picker's search and tabs), the
  DKS editor (arrow keys move in the grid, Space changes a cell, the
  sentence is read after a change) and Key depth with "Announce the depth"
  (at most four times a second). Write nothing; **Discard** the drafts.
  Record what Narrator reads and anything it skips.

## Background agent, recovery and game profiles (A23-A35, Phase 5b-2)

Version 0.2.0 adds what runs around the editors: the notification-area
(tray) icon, close to tray, start with Windows, Windows notifications,
automatic backups with reset detection and the Recovery card, profiles
that follow the game in front, local profile names, and the device lock
with which the app and the CLI take turns. None of it adds a write: a
recovery is the standard restore with its review, an automatic switch is
the profile select (`21/12`) followed by reads, and automatic backups
reuse the reads the app makes anyway. So far all of it ran only on the
simulator, with recorded fixtures and in unit tests of its Windows parts;
the icon, the notifications, the Run entry, the foreground watcher and the
lock against a real CLI have never run. The tests go from the lowest risk
up; the deliberate factory reset (A35) comes last.

Build with `pwsh tools/release-build.ps1` (the installer
`target\release\bundle\nsis\OpenWraith_0.2.0_x64-setup.exe`) and
`cargo build --release` (the CLI). `openwraith --version` must print
0.2.0: an older CLI does not know the device lock, and A29, A32 and A33
run CLI commands while the app is open **on purpose**. Everywhere else the
rule of A1-A22 stays: run the CLI only with the app closed, and close it
with **Quit** in the icon's menu (once the icon is on, closing the window
only hides it). A23 installs 0.2.0 over 0.1.0 (and says how to get back
to 0.1.0 first when A11 already installed 0.2.0); the 0.1.0 installer in
`target\release\bundle\nsis` serves only as the starting point of that
upgrade. Same preconditions as above (vendor site closed, cable, one
keyboard), except A33, which opens the site on purpose.

Profile 4 is the spare profile (the edits, the game rule's profile),
profile 1 the default profile the game rule returns to, profile 3 the
manual choice of A30; profiles 1-3 are only switched to, except one key
on profile 1 in A30 (step 4, undone at once).
Every write opens its review (tick "Every wraith.software tab is closed",
the write button arms after half a second, Enter never writes) and its
report has **Undo…**. The rollback point of the whole section is
`before-a23.owbackup.json` (A23); A35 takes its own backup right before the
reset.

Where to look (`%LOCALAPPDATA%` = `C:\Users\<you>\AppData\Local`,
`%APPDATA%` = `C:\Users\<you>\AppData\Roaming`):

| What | Where |
|---|---|
| The installed app | `%LOCALAPPDATA%\OpenWraith\openwraith-app.exe` |
| Settings | `%APPDATA%\io.github.zeruh31.openwraith\settings.json` |
| Automatic backups | `%LOCALAPPDATA%\io.github.zeruh31.openwraith\known-good\unidentified\profile-N\`, with `pinned.json`; `known-good\unidentified\recovery\` holds the backups composed for "Restore all profiles". `unidentified`: the W75 reports no USB serial number, so every W75 on this PC shares the folder |
| Automation log | `%LOCALAPPDATA%\io.github.zeruh31.openwraith\state\automation.log` |
| Device lock | `%LOCALAPPDATA%\OpenWraith\device.lock`; `device.lock.json` next to it while a program holds it |
| Safety backups | `%LOCALAPPDATA%\OpenWraith\safety-backups` (never deleted) |
| Start with Windows | `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v OpenWraith` |

Windows notifications: turn Do not disturb off for this section. OpenWraith
shows at most one notification of each kind every 10 minutes; Recent
activity on Settings > Games, the in-app toasts (while the window is open)
and the banners show every event. A click counts only while the
notification is on screen; from the notification centre it may only bring
the window up (record what it does). Each test below that shows a
notification clicks it once; where a click must lead:

| Notification | A click opens |
|---|---|
| "OpenWraith is still running" (the first close to the icon) | the window |
| "Profile N is active" (a game switched; only with "Tell me when a game switches the profile") | Settings > Games |
| "A profile switch is waiting", "A profile switch was skipped", "Game profiles paused" | Settings > Games |
| "The keyboard was reset" | the Keyboard page, with the reset banner |
| "Profile N looks reset", "The keyboard's firmware changed" | Backup & profiles, Recovery card |
| "Automatic backup failed" | Backup & profiles, at Automatic backups |

- **A23. Install 0.2.0 over 0.1.0; defaults and the first automatic
  backups (nothing written).** Start from 0.1.0: either the A1 install,
  or, when A11 already installed 0.2.0, **Quit** it, uninstall it
  (Windows Settings > Apps; leave **Delete the application data**
  unticked), rename `%LOCALAPPDATA%\io.github.zeruh31.openwraith\known-good`
  to `known-good-a11` (so that A23 starts without automatic backups) and
  install `target\release\bundle\nsis\OpenWraith_0.1.0_x64-setup.exe`. In
  0.1.0 set the theme (or the language) to a value you will recognise, a
  setting the upgrade must keep, then close it. Copy `settings.json` aside
  as `settings-0.1.0.json`. Then:

  ```text
  openwraith backup --all-profiles -o before-a23.owbackup.json
  ```

  1. Run `OpenWraith_0.2.0_x64-setup.exe`. When it reports an older
     version, keep its default **Uninstall before installing**; in the old
     version's uninstaller that opens, leave **Delete the application
     data** unticked (it would delete the settings and, from 0.2.0 on, the
     automatic backups). Pass: Windows Settings > Apps lists one
     OpenWraith, 0.2.0; the Start menu has one OpenWraith shortcut;
     `%LOCALAPPDATA%\OpenWraith\` holds `openwraith-app.exe` and the
     `safety-backups` folder with every file it had; `settings.json` is
     still there.
  2. Start OpenWraith from the Start menu. Pass: Settings > About shows
     0.2.0; the theme (or language) you set is kept (the old file was
     read, its new fields took their defaults): no "settings file" toast
     and no `settings.invalid-*.json` next to `settings.json`. Settings >
     Background: "Keep OpenWraith running in the notification area" off,
     "Start OpenWraith with Windows" off and disabled ("Turn on the
     notification-area icon first…"), "Tell me when the keyboard looks
     reset…" on, "Tell me when a game switches the profile" off, "Back up
     each profile automatically" on with 10 kept. Settings > Games: off.
     No OpenWraith icon by the clock; the `reg query` above finds no
     `OpenWraith` value.
  3. Backup & profiles > Automatic backups lists one backup of the active
     profile, reason "First read", and says that the keyboard reports no
     serial number; **Show the folder** opens `known-good\unidentified`.
     Switch to profiles 2, 3 and 4 in the header and back to 1. Pass: each
     profile gets one "First read" backup; **Read again** (Keyboard page)
     without a change adds no backup. Click **Pin** on profile 4's
     backup: it shows "Pinned" and `profile-4\pinned.json` names its
     file. No Recovery card and no Recovery banner anywhere.

  Roll back: nothing was written to the keyboard. To go back to 0.1.0,
  uninstall 0.2.0 (leave "Delete the application data" unticked), install
  the 0.1.0 installer and put `settings-0.1.0.json` back as
  `settings.json`.
- **A24. A second start and profile names (nothing sent).**
  1. With the app open, start it again from the Start menu, then run
     `& "$env:LOCALAPPDATA\OpenWraith\openwraith-app.exe" --tray` in
     PowerShell. Pass: both times the open window comes to the front; no
     second window, no second `openwraith-app.exe` in Task Manager, and the
     first app keeps its connection (no "Checking that no other program…"
     again).
  2. Settings > Profile names: "Work" for 1, "FPS" for 2, nothing for 3,
     "Spare" for 4; **Save names**. Pass: "The names were saved."; the
     header reads "Profile 1 · Work"; the connection popover and the
     Keyboard page use the names; Profile 3 has none. The names are in
     `settings.json` (`profile_names`), not on the keyboard: no safety
     backup appeared.

  Roll back: empty the four fields and **Save names** (the clean-up after
  A35 does it).
- **A25. The notification-area icon, close to tray and Quit.** Settings >
  Background: turn on "Keep OpenWraith running in the notification area".
  Pass: the OpenWraith icon appears by the clock (or under the arrow of
  hidden icons); its tooltip reads "OpenWraith · Wraith W75 · Profile 1
  (Work)"; "Start OpenWraith with Windows" can now be turned on.
  1. Close the window (X). Pass: the window goes, the icon and the process
     stay, and a notification "OpenWraith is still running … To quit,
     choose Quit in the tray icon's menu." appears. Click it: the window
     comes back. Close the window again: no second notification;
     `settings.json` now has `"close_to_tray_explained": true`.
  2. Left-click the icon. Pass: the window comes back on the page it
     showed.
  3. Backup & profiles > **Back up all four profiles…**, save
     `a25.owbackup.json`, and while the lighting goes through the profiles
     right-click the icon and choose **Quit**. Pass: the app does not quit;
     the window comes to the front with the close guard; **Keep open** lets
     the backup finish (back on profile 1, the file saved).
  4. With the app idle, **Quit**. Pass: the icon and the process are gone.

  Roll back: nothing was written (the backup only switched profiles and
  back).
- **A26. Start with Windows: the quoted Run value, Task Manager's choice
  and the removal.** Start OpenWraith (the icon comes back: the setting is
  kept). First, reading only:
  `reg query HKLM\Software\Microsoft\Windows\CurrentVersion\Run /v OpenWraith`
  finds nothing (a value there was left by a build before 5b-3 that ran as
  administrator: delete it from an elevated prompt with the same path and
  `reg delete ... /v OpenWraith /f`, and record that it was there).
  1. Settings > Background: turn on "Start OpenWraith with Windows". Pass:
     no error and no callout under the switch;
     `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v OpenWraith`
     shows `OpenWraith    REG_SZ    "C:\...\openwraith-app.exe" --tray`
     with the installed path in quotes; regedit shows the same value under
     that key (Type `REG_SZ`, the Data starting with `"`); the HKLM query
     above still finds nothing (also when OpenWraith runs as
     administrator, if you try that); Task Manager > Startup apps lists
     OpenWraith as Enabled.
  2. Sign out and sign in again. Pass: no window opens; the icon is there
     and its tooltip names the keyboard and the profile (it connected and
     read the keyboard while hidden); opening the window shows the
     keyboard ready. Wait three minutes, then `reg query` again: the value
     is still there, unchanged (a start from the Run entry checks the entry
     only after two minutes and rewrites nothing that matches).
  3. Task Manager > Startup apps: disable OpenWraith, then switch back to
     OpenWraith's Settings > Background. Pass: under the switch a warning
     "Windows won't start OpenWraith: it is turned off in Task Manager"
     with **Check again**, and the switch stays on. It stays when you open
     another tab and come back, and after **Quit** and a new start. Turn
     the switch off and on again: the callout comes back and Task Manager
     still shows Disabled (OpenWraith never writes Task Manager's choice);
     `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v OpenWraith`
     shows a `REG_BINARY` whose first byte is `03` (record the bytes).
     Enable OpenWraith in Task Manager again and switch back to OpenWraith
     (or **Check again**): the callout goes; record the first byte now
     (`02` expected).
  4. Optional, a missing entry: with the switch on,
     `reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v OpenWraith /f`,
     then **Check again**. Pass: "Windows won't start OpenWraith: its start
     entry is missing". **Quit** and start OpenWraith from the Start menu:
     the value is back, quoted as in step 1, and the callout is gone (the
     start-up check writes a missing entry the settings want).
  5. Settings > Background: turn the notification-area icon off. Pass:
     "Start OpenWraith with Windows" turns off and is disabled with its
     reason; `reg query` finds no `OpenWraith` value under Run; no callout;
     the icon goes; the window stays open. Turn the icon on again (start
     with Windows stays off) for the next tests.
  6. The uninstaller's removal, and that an upgrade keeps Task Manager's
     choice, are checked at the very end (the optional uninstall check
     after A35's clean-up).

  Roll back: `reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run
  /v OpenWraith /f` removes a value left behind.
- **A27. The icon's menu.** Quit and start the app again (a new run: the
  vendor box was not ticked yet in this run). Right-click the icon. Pass: a
  disabled first line "OpenWraith · Wraith W75 · Profile 1 (Work)",
  Profile 1 (Work), Profile 2 (FPS), Profile 3, Profile 4 (Spare) with a
  check on the active one, **Back up now**, **Open OpenWraith**, **Quit**;
  no "Pause game profiles" (game profiles are off).
  1. Choose **Profile 4**. Pass: the window opens on the Keyboard page
     instead and the profile stays 1. Confirm the vendor site for this
     run: Actuation > Key depth, tick the vendor box, **Start**, **Stop**
     (nothing is stored). Close the window, choose **Profile 4**, then
     **Profile 1**, then **Profile 4** in the icon's menu. Pass: the
     keyboard switches each time (the lighting changes), the check mark and
     the first line follow, and no safety backup appears (a switch changes
     no setting).
  2. Open Actuation, change W's actuation in the editor without writing
     (a draft), close the window, choose **Profile 1**. Pass: the window
     opens on the Keyboard page and the profile stays 4. **Discard** the
     draft.
  3. Choose **Back up now**. Pass: no switch; Automatic backups shows a new
     backup of profile 4 with the reason "“Back up now” in the
     notification area", or none when nothing changed since the newest one
     (that one is kept).

  Roll back: nothing was written.
- **A28. Automatic backups after an edit, rotation, pinning, and export of
  chosen parts (profile 4).** Settings > Background: set "Automatic
  backups kept per profile" to **2**, **Save**. Profile 4 is active;
  write down W's actuation (Actuation).
  1. Set W to 1.20 mm, review and write, then **Read again** (Keyboard
     page). Pass: Automatic backups shows "After a change made in
     OpenWraith" for profile 4. Repeat with 1.30 mm and 1.40 mm (**Read
     again** after each). Pass: profile 4 keeps its two newest unpinned
     automatic backups plus the pinned "First read" of A23 (the list and
     the folder agree); the safety-backups folder holds all three safety
     backups of these writes besides every older one; nothing says
     "looks reset".
  2. Settings > Background > **Show automatic backups**. Pass: Backup &
     profiles opens at the Automatic backups section.
  3. Share a profile > Export: "Read the profile from" **The keyboard
     now**, Profile 4, the default parts (Actuation & Rapid Trigger,
     Lighting and key colours, Key assignments… ticked; Dead band and SOCD
     key pairs unticked), name "A28", **Export profile 4…**, save
     `a28-keyboard.owprofile.json`. Pass: no switch, "Profile file saved";
     in Notepad the file's `note` ends with "Parts: triggers, lighting,
     keys. Label: A28" and `"usb": null`.
  4. Export again with **An automatic backup**: profile 4's pinned "First
     read"; tick **Dead band**, untick **Key assignments, macros and
     advanced keys**; save `a28-auto.owprofile.json`. Pass: the keyboard is
     not touched (no lighting change, no consent); the note says "Parts:
     triggers, lighting, dead band."; with every part unticked the export
     button is disabled.
  5. Import > **Open a profile file…** `a28-auto.owprofile.json`. Pass:
     "What the file holds" says "Not in the file:" with the key
     assignments and the SOCD key pairs. Close it without importing.

  Roll back: Automatic backups, profile 4's pinned "First read" >
  **Restore…**. Pass: the standard restore review (W 1.40 mm → the value
  you wrote down); **Restore 1 change**: written, verified, kept. Set the
  number kept back to **10**, **Save**.
- **A29. The CLI and the app take turns (no game profiles yet).** The app
  is open on the Keyboard page, idle, profile 4 active; run the commands in
  a terminal next to it.
  1. `openwraith monitor --duration 20s`. Pass: within about a second the
     window shows the banner "An OpenWraith command is using the keyboard"
     with "Command: monitor"; the header switcher says "An OpenWraith
     command is running"; **Read again** is disabled with the reason; the
     profiles in the icon's menu are disabled or open the window instead
     of switching.
     When the command has ended: the banner goes, the header shows
     "Checking that no other program…" for about 2 s, then ready. No
     "Another program is using the keyboard" banner at any point.
  2. `openwraith profile select 1`, then `openwraith profile select 4`.
     Pass: both run; after each the header and the icon's check mark show
     the profile the CLI selected; no automatic backup is added (nothing
     changed).
  3. `openwraith set trigger W --actuation 1.20` (profile 4) and, while it
     runs (or waits for your confirmation), look at the window: "Command:
     set trigger". Pass: after the command, Actuation shows W at
     1.20 mm; Automatic backups shows "Changed outside OpenWraith" for
     profile 4; nothing looks reset.
  4. A CLI command while the app writes: Backup & profiles > Safety backups,
     the newest one (the CLI's, made before step 3) > **Restore…**, tick
     the vendor box. Type `openwraith show triggers` in the terminal
     without Enter, click **Restore 1 change**, then press Enter in the
     terminal at once. Pass: the CLI stops at once with "the
     OpenWraith app is writing; try again" and exit code 1, before opening
     the keyboard; the restore finishes normally (kept after a profile
     switch, no "another program" note); afterwards there is no
     `device.lock.json` next to `device.lock`.

  A command refused although the app was idle means it met the app's
  once-a-second check of the lock: run it again and record how often that
  happens. Roll back: step 4 undoes step 3; check W's value against A28.
- **A30. A game rule with Notepad (profile 4).** Settings > Games: turn on
  "Switch profiles automatically for games". Pass: the explanation dialog
  opens with **Cancel** focused, **Turn on** stays disabled until "I
  understand…" is ticked, and Enter does nothing. Tick it, **Turn on**.
  Start Notepad, then **Add a game…** > **Choose a running program**:
  Notepad is listed (Windows 11's Notepad as `notepad.exe` in its
  `WindowsApps` folder); name it "Notepad", Profile 4, **Add game**. Set
  "When a game closes" to go back to profile 1. In Settings > Background
  turn on "Tell me when a game switches the profile". Close Notepad,
  switch to profile 1 in the header and close the window.
  1. Start Notepad (Windows + R, `notepad`) and keep it in front. Pass:
     about 1.5 s later the keyboard switches to profile 4; a notification
     "Profile 4 is active — Notepad is in front, so OpenWraith switched to
     its profile." (click it: Settings > Games); the header shows the
     **Auto** chip; Settings > Games says "Notepad is in front, with
     Profile 4 …" and Recent activity lists the switch; `automation.log`
     has a line with the rule's `rule_id`. Alt-tab away and back: no
     second switch.
  2. With Notepad in front, press Fn + 3. Pass: profile 3 stays; the Auto
     chip goes; the line says you chose another profile and OpenWraith
     leaves it until the game closes. Alt-tab away and back: still
     profile 3. Close Notepad. Pass: profile 3 stays (your choice is never
     switched away).
  3. Start Notepad again (a new run of it): profile 4 after about 1.5 s.
     Close Notepad. Pass: about 3 s later the keyboard is back on profile
     1; Recent activity says "A game closed: back to Profile 1 …". (The
     Windows notification for it appears only if the one of step 1 is
     more than 10 minutes old.)
  4. A write, then the game within a second: with Notepad closed and
     profile 1 active, set Q's actuation 0.10 mm away from its value
     (Actuation), **Review and write…**, tick the vendor box, click
     **Write to keyboard** and at once press Windows + R, `notepad`, Enter.
     Pass: the switch to profile 4 comes only after the write's report
     ("OpenWraith waited 2 s for the keyboard to save.") and a further
     2 s settle; Recent activity may first say the switch waited because
     another operation was running. Close Notepad (back to profile 1). Pass: Q has the new
     value after **Read again** (the write was kept). **Undo…** it.
  5. Optional, a program run as administrator: start Registry Editor
     (`regedit`; change nothing), add it with **Choose a running program**
     to profile 4. Pass: with it in front for two seconds the keyboard is
     on profile 4. If you have a game with a kernel anti-cheat, add it and
     record the line of Settings > Games while it is in front ("Watching
     for your games." means it could not be identified) and that nothing
     else happened. Remove these rules afterwards.

  Roll back: switches only, and the Undo of step 4. Keep the Notepad rule
  for A31-A33.
- **A31. Switches that wait: unwritten changes, an open review, the key
  depth view, the pause.** Notepad closed, profile 1 active, the window
  open.
  1. Unwritten changes: on Actuation change W in the editor (a draft), do
     not write, start Notepad. Pass: no switch; a notification "A profile
     switch is waiting — Notepad uses profile 4, but OpenWraith has
     unwritten changes…"; Settings > Games says the switch waits because
     there are unwritten changes. Back in OpenWraith, **Discard** the
     draft within 30 s, then click into Notepad: the switch happens. Close
     Notepad (profile 1 again). Repeat, but leave the draft for 30 s
     instead. Pass: "A profile switch was skipped" (in the app; as a
     Windows notification only if none was shown in the last 10 minutes)
     and no switch until Notepad is closed and started again. Discard the
     draft, close Notepad.
  2. An open review: change W again, **Review and write…**, leave the
     review open and start Notepad. Pass: no switch; Recent activity says
     the switch waited because a change is open for review (no Windows
     notification for this reason). Within 30 s click into OpenWraith,
     **Cancel** the review, **Discard** the draft and click into Notepad:
     the switch happens. Close Notepad.
  3. The key-depth view: Actuation > Key depth, tick the vendor box,
     **Start**; leave the window as it is (not minimised) and start
     Notepad. Pass: no switch while **Live** shows; Recent activity says
     the key-depth view is running. Within 30 s click into OpenWraith,
     **Stop**, click into Notepad: the switch happens. Close Notepad.
  4. The pause: in the icon's menu (now with "Pause game profiles"),
     choose it. Pass: it is checked; the header shows "Auto paused" (no
     banner); Notepad started now does not switch. Close Notepad, choose
     the item again (unchecked), start Notepad: it switches. Close
     Notepad.

  Roll back: nothing was written (every draft discarded).
- **A32. A CLI command while the app is in the notification area.** The
  Notepad rule is on, profile 1 active, Notepad closed, the window closed.
  1. In a terminal run `openwraith monitor --duration 10s` and at once
     start Notepad (Windows + R). Pass: no switch while the command runs
     (open the window: the CLI banner; Recent activity: the switch waits
     because an OpenWraith command is using the keyboard). A few seconds
     after the command has ended the keyboard switches to profile 4: the
     app re-armed by itself (no "check again", no two-minute wait after
     its own CLI). Close Notepad (back to profile 1).
  2. Start Notepad (profile 4), click into the terminal and run
     `openwraith profile select 2`. Pass: after the command the header
     shows profile 2 and the Auto chip is gone (the CLI's choice counts as
     yours). Close Notepad. Pass: profile 2 stays. Switch to profile 1 in
     the header.

  Roll back: switches only.
- **A33. The vendor site in a background tab.** This test opens
  wraith.software on purpose; do nothing on the site but connect. The
  Notepad rule is on, the icon on, profile 1 active, Notepad closed.
  1. Open wraith.software, connect the keyboard there, then switch the
     browser to another tab (the site's tab stays open). Pass: OpenWraith
     shows "Another program is using the keyboard"; the icon turns to its
     attention look with "Another program was using the keyboard — check
     again" in its menu. Start Notepad and keep it in front for a minute.
     Pass: no switch; after 30 s "A profile switch was skipped …: another
     program was using the keyboard." (in the app, and as a Windows
     notification unless one was shown in the last 10 minutes). Close
     Notepad.
  2. Close the site's tab and the browser window. In OpenWraith choose "I
     closed it, check again" and note the time. Start Notepad at once.
     Pass: no switch while the site was heard less than 2 minutes ago
     (the line says another program used the keyboard less than 2 minutes
     ago); skipped after 30 s. Close Notepad, wait until 2 minutes have
     passed since you closed the site, start Notepad again: the switch to
     profile 4 happens. Close Notepad (profile 1).
  3. The automatic re-arm (icon on): connect the site again, close its tab
     and the browser window, but do **not** choose "check again"; close
     OpenWraith's window and leave the keyboard untouched for 3 minutes.
     Then start Notepad (with the mouse). Pass: the switch to profile 4
     happens (game profiles re-arm after 180 s of silence); the window
     still shows
     "Another program is using the keyboard" and a write is still refused
     until "I closed it, check again". Close Notepad (back to profile 1).
     Then choose "Another program was using the keyboard — check again"
     in the icon's menu: the app checks again and is ready.

  Roll back: switches only; the site was not used to change anything (A8
  showed that a connected, untouched site writes nothing). The final
  compare after A35 covers it.
- **A34 (optional). Narrator on Settings > Games.** With Narrator on:
  the switch and its explanation dialog (Cancel focused, the tick, "Turn
  on"), the live line (read when it changes; start Notepad), the table at
  full width and as cards (make the window as narrow as it goes), the
  profile menus and switches of the rows, **Add a game…** (the running
  programs, the typed name, "Detect the next program I open" with its
  countdown and "Stop waiting"), Recent activity and "How it works". Record
  what is read and anything skipped. Write nothing.
- **A35. Recovery after a deliberate factory reset (Fn + Space).** The
  highest risk of the section: what the reset clears beyond the active
  profile is open (Q4), so all four profiles are backed up first and the
  app must only **offer** a restore. Settings > Games: turn game profiles
  off.
  1. Preparation. On profile 4 make two visible changes so that it
     differs from the factory settings: W to 1.00 mm with Rapid Trigger
     0.30 mm, and the effect to Static (write both), then **Read again**.
     Switch to profiles 1, 2 and 3 in the header and back to 4: every
     profile's newest automatic backup now matches the keyboard (nothing
     says "looks reset"). **Quit**, then

     ```text
     openwraith backup --all-profiles -o before-a35.owbackup.json
     ```

     and start OpenWraith again (it reads profile 4: no new automatic
     backup, nothing reset).
  2. The reset. On profile 4 hold Fn + Space for 5 s. Pass: at once the
     header and a banner say "The keyboard reset itself", writing is
     paused; the icon turns to its attention look with "Keyboard reset —
     review…"; a notification "The keyboard was reset — It reported a
     reset (Fn + Space held). OpenWraith never restores anything without
     asking." Click it: the window opens on the Keyboard page with the
     banner "Read the keyboard again" / "Restore a backup…". Record the
     keyboard's lighting and active profile now.
  3. Nothing by itself: leave the app alone for two minutes. Pass: no
     review opens, no safety backup appears in the folder, no automatic
     backup is added, the keyboard stays as the reset left it.
  4. **Restore a backup…** (or "Keyboard reset — review…" in the icon's
     menu). Pass: Backup & profiles with the Recovery card focused: the
     reset report with its time, "OpenWraith never restores anything
     without asking.", the note that the keyboard reports no serial
     number, and **It's fine, keep them** disabled ("Reconnect and compare
     first."). Optional: the Narrator pass of the card now (the title,
     the evidence list, the profile rows, the disabled button's reason).
  5. **Reconnect and compare**. Pass: the active profile is read and,
     when the reset changed it, the card says "Profile N looks reset"
     with why (the reset report; Actuation and Rapid Trigger back to the
     factory settings, …), "At the factory settings now: …", the
     automatic backup it was compared with and its age; a notification
     "Profile N looks reset" (click it: the Recovery card); the Recovery
     banner on the other pages. Still nothing written: no safety backup.
  6. Switch in the header to each other profile (the card says which were
     not compared yet). Pass: each one is compared when read; record which
     profiles look reset and which match their automatic backup (this
     answers Q4).
  7. **Restore all profiles…** > tick the vendor box > **Switch and
     compare**. Pass: the review "Restore profiles 1, 2, 3 and 4 from their
     automatic backups?" lists, per profile, only what the reset changed
     (on profile 4, for example, W from its factory 1.70 mm back to
     1.00 mm with Rapid Trigger 0.30 mm, and the effect back to Static;
     profiles that match have nothing to write; the SOCD power at most
     once); **Restore N changes** is armed after half a second and Enter
     does nothing. Click it. Pass: every item written,
     verified and kept after a profile switch; the keyboard back on the
     profile it was on; the card no longer lists a profile that looks
     reset, and once every profile with automatic backups was compared,
     the banner and the icon's attention go by themselves;
     `known-good\unidentified\recovery\` holds the composed backup.
  8. Final compare. **Quit**, then

     ```text
     openwraith backup --all-profiles -o after-a35.owbackup.json
     openwraith restore before-a35.owbackup.json --dry-run --against after-a35.owbackup.json
     ```

     Pass: nothing to write in any of the four profiles (the keyboard-wide
     SOCD power included).

  Roll back: if the dry run lists anything, or a step above failed,
  `openwraith restore before-a35.owbackup.json` and record the report.
  Variant, only if time allows: reset again, compare, and choose **It's
  fine, keep them**. Pass: "OpenWraith keeps the current settings as they
  are.", "Kept after a possible reset" backups appear, nothing is written;
  then `openwraith restore before-a35.owbackup.json` with the app closed.

Clean-up after A35 (app open): Settings > Games off and every rule
removed; Settings > Profile names emptied (**Save names**); Settings >
Background: the icon off (start with Windows goes with it; `reg query`
finds nothing). **Quit**, then

```text
openwraith backup --all-profiles -o after-a23.owbackup.json
openwraith restore before-a23.owbackup.json --dry-run --against after-a23.owbackup.json
openwraith restore before-a23.owbackup.json
```

Pass: the dry run lists only the edits of A35's preparation on profile 4
(W and the effect), and the last restore brings them back with a clean
final check. Optional uninstall check, for the setup program (`.exe`):
turn the icon and start with Windows on, and disable OpenWraith in Task
Manager > Startup apps (the callout of A26 step 3 appears). Then:

1. An upgrade keeps Task Manager's choice. **Quit** and run
   `OpenWraith_0.2.0_x64-setup.exe` again; after its welcome and license
   pages, on the page for an installed OpenWraith, choose the second
   option, **Uninstall OpenWraith** (the same uninstall without
   `/UPDATE` that a newer setup program runs first by default, **Uninstall
   before installing**). In the uninstaller leave **Delete the application
   data** unticked, then finish the installation with **Run OpenWraith**
   ticked. Pass: the `reg query` of A26 shows the quoted Run value again
   (the start-up check wrote it); Task Manager > Startup apps still lists
   OpenWraith as Disabled; the `Explorer\StartupApproved\Run` value still
   starts with `03`; Settings > Background shows the "turned off in Task
   Manager" callout.
2. Uninstall. **Quit** and uninstall OpenWraith (Windows Settings > Apps)
   without turning anything off, leaving "Delete the application data"
   unticked. Pass: no `OpenWraith` value under `Run`; the one under
   `Explorer\StartupApproved\Run` is still there (Task Manager's choice
   stays with the settings, and starts nothing without a Run value); Task
   Manager no longer lists OpenWraith; the safety backups and the
   automatic backups are still there.
3. Optional, the full removal (it deletes the settings and the automatic
   backups: copy `%APPDATA%\io.github.zeruh31.openwraith` and
   `%LOCALAPPDATA%\io.github.zeruh31.openwraith` aside first to keep
   them). Install the setup program again, then uninstall OpenWraith with
   "Delete the application data" ticked. Pass: no `OpenWraith` value under
   `Run` nor under `Explorer\StartupApproved\Run`; the safety backups are
   still there.

For the MSI: the Run value stays (as the README says); delete it with the
`reg delete` of A26.

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
| 2026-10-27 | H12 (monitor) | pass | Fn + 2 / Fn + 1 pushed `21 00 00 00 03 11 02 00` / `.. 11 01 00` (Q2); Fn + Right Shift pushed `24 02 00 00 06 01 ..` then `06 00`: the state after the toggle, also with no pair stored; nothing sent |
| 2026-10-27 | H13 (key depth; step 4 not run) | pass | typing works while the test is open; a close with no test open is acked; Fn + 2 made OpenWraith close the test (acked); slow press: median 72 ms between reports (p10 54, p90 123), 3-unit steps, 3.40 mm; Fn streams too; 3290 reports in 120 s (Q33, Q34) |
| 2026-10-27 | H14 (per-key colours) | pass | Left and Down turned blue: LED slot = key index (Q8); 8 packets `09 00`, no profile switch |
| 2026-10-27 | H15 (trigger presets) | pass | W, A, S, D 0.40 mm with RT 0.10 mm, 4 packets |
| 2026-10-27 | H16 (dead band, profile 2) | pass | only profile 2 shows W 0.10 / 0.20 mm: the dead band is **per profile** (Q3); the "may apply to every profile" note is fixed |
| 2026-10-27 | H17 (base-layer keys) | pass | Caps acts as Ctrl, F12 mutes; 20 packets (base keymap + unchanged Fn keymap) |
| 2026-10-27 | H18 (Fn-layer codes) | pass | `07 24` back, `07 25` forward (Q14); mouse `08` back (X1), `10` forward (X2), wheel up, left / right / middle click, L-Ctrl+C copies (Q37); F1-F4 lose their built-ins, `default` brings them back (Fn + F4 mutes); refused without `--allow-builtin` (the refusal now names the key) |
| 2026-10-27 | H19 (macros) | pass | Del plays its macro; `19 00` alone (`--commit-trigger skip`) `persisted`: macros need no `18 02`; times:3 types `aaa`, toggle and while-held as named; a macro on Fn + Q plays (Q13) |
| 2026-10-27 | H20 (advanced keys) | pass (finding) | DKS, Mod-Tap (300 ms) and Toggle work; 0.1 mm press-point unit; DKS taps at stages 1-3, a hold lasts for its own stage (Q19); a lone `22` (`--commit-trigger skip`) was `LOST` (byte offsets 9, 11, 14, 18) while `18 02` + `22` persisted: the Fn keymap before the slots is required (Q36) |
| 2026-10-27 | H21 (SOCD, profile 2) | pass (finding) | modes 1 and 4 as named, `persisted`, `24 03` never acked (Q35); the pair is only in profile 2, but the power is **one setting of the whole keyboard** (Fn + Right Shift switched every profile off; Q3) |
| 2026-10-27 | H22 (USB power cycle) | pass | nothing to write in any profile: custom colours 1, dead band, SOCD power and pairs, both keymaps, macro slots 0-4, advanced-key slots 0-5 and the active profile survived (Q1) |
| 2026-10-27 | H23 (sharing a profile) | pass | the exported file has `"usb": null`; profile 3's keys imported into profile 4: macro slots 0-4 in ascending order, both keymaps, advanced-key slots 0-5, all `persisted` |
| 2026-10-27 | H24 (restore / undo) | **fail** -> fixed | safe stop (exit 1, nothing wrong written): the first SOCD write switched every profile's power, so profile 2's re-read saw "changed after planning"; and the slot-0 macro write emptied slots 1-4 (here harmless, their target was empty). A second run completed; the last dry run found nothing to write. Fixed: SOCD power planned once, macro slot-0 re-sends (H26, H27) |
| 2026-10-27 | H25 (critical Fn function) | not run | optional |
| 2026-10-28 | H26 (macro slot 0 re-sends, profile 4) | pass | slots 1 and 2 written alone (5 packets each, no `18 02`); the slot-0 write re-sent slots 1 and 2 (15 packets) and all three `persisted` (`--check-persistence always`); its undo by restore wrote slot 0 empty and re-sent slots 1 and 2 the same way, `persisted` (fix `ffd1c8f`) |
| 2026-10-28 | H27 (multi-profile restore, keyboard-wide SOCD power) | pass | pairs in profiles 2 (A / D) and 3 (Left / Right), power on, profile 4 active: the plan showed `SOCD power (all profiles): on -> off` once, in profile 2's write; all 4 writes `persisted`, profile 3's SOCD write (the one H24 refused) included; final check clean, exit 0; afterwards nothing to write in any profile against the all-profile backup |
| 2026-10-29 | Short app check, build `07a213a` (instead of the full A11-A35 run) | pass | The maintainer chose a short check of the riskiest parts; the rest of A11-A35 was not run. Installed over the earlier 0.2.0 with the setup program |
| 2026-10-29 | A12 step 1 (key depth view) | pass | header **Live**, profile switcher busy; caps fill to 3.40 mm with "max" chips, the followed key's gauge and 5-second chart. Steps 2-5 not run |
| 2026-10-29 | A13 (key colours and Undo, profile 4) | pass | four arrow keys blue on the keyboard, others unchanged; **Undo…** restored the previous colours |
| 2026-10-29 | A30 steps 1 and 3, short (game rule with Notepad) | pass | Notepad listed among the running programs (5b-3's process list); in front -> profile 4, closed -> back to profile 1 a few seconds later. Steps 2, 4, 5 not run |
| 2026-10-29 | A26 steps 1 and 3 (start with Windows, Task Manager) | pass | HKLM has no value; HKCU Run `OpenWraith` = `"C:\Users\<user>\AppData\Local\OpenWraith\openwraith-app.exe" --tray` (quoted); Task Manager lists `openwraith-app.exe` Enabled. Disabled in Task Manager: the lasting callout appeared, the switch stayed on, the Run value was unchanged, StartupApproved `03 00 00 00` + 8 bytes (a FILETIME); enabled again: `02 00 00 00 00 00 00 00 00 00 00 00`, callout gone. Steps 2, 4, 5 and the uninstall check not run |
