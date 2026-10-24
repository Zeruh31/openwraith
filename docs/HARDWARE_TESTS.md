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
- Do not switch profiles on the keyboard while a command runs.
- Every command that writes prints the safety backup it saved and the undo
  command (`openwraith restore <safety backup>`). If anything looks wrong,
  stop and run that undo command.

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

Answer `y`. Pass: the plan shows W only (`now -> 1.20 mm`); the report says
written and verified; `show triggers` shows W at 1.20 mm. Type on W: it
actuates noticeably earlier/later than before, nothing else changed.

## H5. Restore undoes the edit

```text
openwraith restore before.owbackup.json
openwraith restore before.owbackup.json --dry-run
```

Pass: the plan of the first command lists only W's trigger (packets: 1).
Answer `y`; the report says written and verified. The dry run afterwards
reports nothing to write.

## H6. Light mode change and its restore

```text
openwraith set light --mode static --color 0000ff
openwraith restore before.owbackup.json
```

Answer `y` both times. Pass: the keyboard turns static blue, then back to
the backed-up effect. The restore plan shows "main light with mode change
(17 00, 07 00)": the music-rhythm leave is sent only because the mode changes.
No warning about another program appears.

## H7. Restoring another profile than the active one

```text
openwraith profile select 2
openwraith set trigger Q --actuation 2.00
openwraith profile select 1
openwraith restore all.owbackup.json --profile 2
openwraith profile
```

Answer `y` where asked. Pass: the restore plan lists profile 2 / trigger of Q
only; the report ends with "Active profile: 1 (as before)"; `profile` reports
1; `show triggers` after `profile select 2` shows Q at its value from
`all.owbackup.json` (then `profile select 1` again).

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
