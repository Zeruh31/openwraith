# Hardware tests (Phase 2)

Every automated test runs against captures or the device simulator; none of
them opens a HID device. The commands that write have therefore only been
checked against the simulator. This checklist is run **by hand** on a real
Wraith W75, in order. Record the full output of every command (add
`--verbose` for session statistics) and keep every file it writes.

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
openwraith restore backups/openwraith-W75HEARGB-20261024-151752.owbackup.json --dry-run
openwraith restore backups/openwraith-W75HEARGB-20261024-151752.owbackup.json
```

Pass: the plan writes the base keymap with zeros at the knob entries (16-21)
and at 121, and no defaults are added. After `y`, check that the knob still
controls the volume (the factory state had zeros there). If it does not, run
the printed undo command.

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

## Reporting

For each test record: pass or fail, the full output, and for a failure the
safety backup path and the undo command that was printed. Attach the
`--verbose` statistics of any command that warned about another program.
