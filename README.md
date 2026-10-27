# OpenWraith

OpenWraith is an open-source replacement for the browser-based configurator of
the **Wraith W75** Hall-effect (magnetic switch) keyboard. It talks to the
keyboard directly over USB HID. No browser, no account, and no third-party
analytics are involved.

This repository contains a Rust library, the `openwraith` command line tool
and the **OpenWraith desktop app** (Tauri 2, Windows first). They show the
keyboard's settings, make lossless backups of one or all four onboard
profiles, restore them, and change the actuation point, Rapid Trigger and
the main light.

> **Disclaimer.** OpenWraith is an independent project. It is **not affiliated
> with, endorsed by, or supported by Wraith** or the keyboard's manufacturer.
> "Wraith" and "W75" are used only to say which hardware this works with. The
> protocol was documented for interoperability from USB captures of the
> maintainer's own keyboard and from an analysis of the public web
> configurator; no vendor code is included (see
> [docs/PROTOCOL.md](docs/PROTOCOL.md)). Use at your own risk.

## Status: Phase 3 (desktop app v0.1)

| Works now | Planned |
|---|---|
| **Desktop app v0.1** (Windows): backup and restore, actuation and Rapid Trigger, main light; English and Turkish | Linux and macOS builds of the app |
| `openwraith list` / `info` / `show`: devices, model, firmware, triggers, lighting, keymap, macros (read-only) | Keymap and macro editing |
| `openwraith backup`: lossless backup of the active profile (read-only) | Per-key colours, dead band |
| `openwraith backup --all-profiles`: all four onboard profiles (switches profiles, nothing else) | SOCD, switch types, side light, polling rate, Win-Lock (no confirmed write command yet) |
| `openwraith restore`: restore a backup, with a plan, a confirmation and read-back verification | Other models of the family (W60, W65, W100) |
| `openwraith set trigger` / `set light`: actuation, Rapid Trigger, main light | |
| `openwraith profile [select N]`: show or switch the active profile | |

Supported hardware: Wraith W75 (product code `W75HEARGB`) connected with the
USB cable, firmware `V3.22.07` (USB `2e3c:c368`). The read-only commands were
checked on the maintainer's keyboard, and so were the commands that write
(the results log in [docs/HARDWARE_TESTS.md](docs/HARDWARE_TESTS.md)).
**The desktop app has so far only run against the device simulator; its
hardware checks (A1-A10 in the same file) are still to be done.** The
product IDs `2e3c:c365` to `2e3c:c367` are accepted as the spec allows, but
unverified.

## Desktop app

The app does what the CLI does for backups, restores, triggers and the main
light, with a visual keyboard and a confirmation for every write. It uses
the same library, so every rule under [Safety](#safety) applies to it too.

| Page | What you can do |
|---|---|
| **Keyboard** | Connection, model, firmware, active profile; read again; release the keyboard. The header switches the active profile (1-4). |
| **Actuation & Rapid Trigger** | Select keys on the W75 (click, Shift + click, drag, Ctrl + A, WASD, arrows, a row, keys with the same setting), set the actuation point (0.02 mm steps) and Rapid Trigger: off, on with one sensitivity, or separate press and release. |
| **Lighting** | The main light's effect (the keyboard's list plus Custom), colour, brightness and speed. |
| **Backup & Restore** | Back up all four profiles or the active one; restore a backup (all or some profiles) after a plan preview; list and restore the safety backups. |
| **Settings** | Language (Windows' language, English, Türkçe), theme (Windows' theme, light, dark), the safety-backups folder, About. |

Not in v0.1: key assignments, macros, per-key colours, side light, dead band,
SOCD, polling rate and Win-Lock.

### Install (Windows 10 and 11, x64)

- **Installer.** Run `OpenWraith_0.1.0_x64-setup.exe` (installs for the
  current user without administrator rights; English or Turkish, from
  Windows' language) or `OpenWraith_0.1.0_x64_en-US.msi` (per machine).
  Build them yourself as described under [Development](#development); they
  end up in `target\release\bundle\nsis\` and `target\release\bundle\msi\`.
- **Not signed yet.** Windows SmartScreen warns on the first start ("Windows
  protected your PC"): choose "More info", then "Run anyway".
- **WebView2.** Windows 11 includes it. If it is missing, the installer
  downloads Microsoft's WebView2 installer (the only network access; the app
  itself never uses the network).
- **Uninstall** in Windows Settings > Apps. Your safety backups stay where
  they are.

### Use

1. **Close every wraith.software tab** (a background tab still talks to the
   keyboard), then start OpenWraith with the keyboard connected by cable.
2. The header shows "Checking that no other program is using the keyboard"
   for 2 seconds (OpenWraith only listens), then reads the keyboard and shows
   "Wraith W75 · FW 3.22.07".
3. **Back up all four profiles once** (Backup & Restore), so you can always
   go back.
4. Change something, press **Review and write…**, read the plan and press
   the write button. The first write after each start asks you to confirm
   that the vendor site is closed.
5. The report says what was written and verified, names the **safety
   backup** made before the change, and offers **Undo…** (a normal restore of
   that safety backup, with its own plan and confirmation).

### Safety in the app

- **Nothing is written without your confirmation.** Every change is planned
  first (read-only) and shown; the write button becomes active after half a
  second, Enter never presses it, and one confirmation writes once. Sliders
  and fields change nothing on the keyboard until you confirm.
- **A safety backup comes first** for every setting change and every
  restore, in the same folder as the CLI's
  (`%LOCALAPPDATA%\OpenWraith\safety-backups`, or `OPENWRAITH_SAFETY_DIR`).
  The app shows the path; safety backups are never deleted.
- **Waiting for the keyboard to save.** The keyboard stores a change about
  1 second after receiving it; every write waits 2 seconds before it
  finishes and shows a countdown. Don't unplug the keyboard or switch
  profiles on it while that is shown. A restore also checks that the
  keyboard kept what was written (by switching away and back) and reports
  "LOST" if it did not.
- **Another program (the vendor site).** While idle the app only listens. If
  another program talks to the keyboard, OpenWraith stops sending anything,
  reads included, shows a banner and pauses writing until you close the
  site and press "I closed it, check again".
- **Profile changes on the keyboard** (Fn + 1-4) are noticed: the app reads
  the keyboard again and refuses any plan made for the old profile.
- **A keyboard reset** (Fn + Space) stops the app at once; it sends nothing
  until you choose "Read the keyboard again".
- **Closing during a write** is deferred: the window asks whether to keep it
  open or to stop safely (at the next safe point, after the save wait) and
  close.
- **An interrupted write** (a crash or a power loss) is shown at the next
  start, with Undo for its safety backup.
- **Don't run the CLI while the app is open:** both would talk to the
  keyboard. "Release keyboard" (header) closes it without quitting the app.
- Firmware other than 3.22.07 is marked "not tested yet" on the Keyboard
  page and in every confirmation.
- No account, no telemetry, no network. The app shows only its own content
  (a strict content security policy and only the app's own commands are
  allowed to the window).

Where the app keeps its files: safety backups as above; its settings in
`%APPDATA%\io.github.zeruh31.openwraith\settings.json`; the operation journal
(an interrupted write) in `%LOCALAPPDATA%\io.github.zeruh31.openwraith\state\`.

## Safety

These rules are the library's; the CLI and the desktop app follow them
alike.

- **Read-only commands stay read-only.** `list`, `info`, `show`, `profile`
  (without `select`), `restore --dry-run` and `backup` (without
  `--all-profiles`) open the keyboard through a transport type that cannot
  send a write at all (checked by the compiler, see
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)).
- **These commands write:** `profile select`, `set`, `restore`, and
  `backup --all-profiles` (profile switches only). They can only send the nine
  write commands that were confirmed on real hardware (docs/PROTOCOL.md
  section 3). Reset, calibration, travel test, SOCD, switch types, side light,
  polling rate and Win-Lock writes cannot even be built.
- **A safety backup comes first.** Before changing any setting (`set`,
  `restore`), OpenWraith reads the keyboard, saves a safety backup, reads the
  saved file back to check it, and only then shows what will change and asks
  `[y/N]` (or takes `--yes`). `profile select` and `backup --all-profiles`
  only switch profiles, change no setting and do not ask. Every write is sent one packet at a time, waiting for the
  keyboard's acknowledgement, and is checked by reading it back. At the end
  OpenWraith prints the exact command that undoes the change:
  `openwraith restore <safety backup>`. Nothing is ever rolled back
  automatically.
- **Where safety backups go:** `%LOCALAPPDATA%\OpenWraith\safety-backups` on
  Windows, `$XDG_DATA_HOME/openwraith/safety-backups` (else
  `~/.local/share/openwraith/safety-backups`) on Linux,
  `~/Library/Application Support/OpenWraith/safety-backups` on macOS. Choose
  another directory with `--safety-dir DIR` or the `OPENWRAITH_SAFETY_DIR`
  environment variable. Safety backups are never deleted automatically.
- **Close the wraith.software tab completely** before running any command,
  and especially one that writes. Switching to another tab is not enough:
  browsers slow down timers in background tabs, so a hidden tab may not be
  noticed. The vendor site receives OpenWraith's replies too; it then
  reloads, rewrites its stored copy of your settings, sends its own (no-op)
  trigger write and can file per-key values under the wrong keys
  (PROTOCOL.md 4.5.1, 4.11.2, 6.5). Before writing, OpenWraith listens for
  2 seconds and refuses if it sees another program; it warns after any
  command that noticed one.
- **Keyboard resets are never answered.** Holding Fn + Space for a few seconds
  resets the keyboard, which then sends a reset notification. The vendor site
  answers it with a reset command of its own; OpenWraith stops at once and
  sends nothing more, not even a switch back to the original profile.
- **What a restore writes.** Exactly the backup's bytes, table by table:
  triggers (one key per packet), dead band, keymaps, macros, cleared
  advanced keys, custom colour table 1 and the main light. Tables without a
  confirmed write command (side light, custom colour table 2, SOCD, switch
  types, polling rate, Win-Lock / 6-key) are compared and listed as "not
  written". Restore never adds defaults that are not in the backup (the
  vendor site writes its own knob defaults into the keymap; OpenWraith does
  not). If the keymaps, macros or advanced keys cannot be restored as a whole
  (for example because a non-empty advanced key would need an unconfirmed
  write format), that whole group is skipped and the plan says why. The
  group is written in the vendor site's order (macros, base keymap, Fn
  keymap, advanced keys). The keyboard keeps a changed base keymap only when
  the Fn keymap is written after it (found on real hardware, docs/PROTOCOL.md
  4.8.2), so the Fn keymap is then written right after it, unchanged if
  need be. The plan shows it as "Fn keymap (18 02)" with the reason in its
  Change column ("unchanged, re-sent: the keyboard keeps the base keymap only
  when the Fn keymap is written after it"); the report lists it as "Fn keymap
  (18 02) re-sent unchanged to make the keyboard save the base keymap", and
  every one of its packets must be acknowledged. As a precaution (an untested
  guess), a restore that writes only macro or advanced-key slots ends with
  the unchanged Fn keymap as well.
- **The keyboard saves a change about 1 second after it is written.** Until
  then it keeps it only in memory: a profile switch (or unplugging the cable)
  in that second loses it, and reading it back cannot tell (found on real
  hardware, docs/PROTOCOL.md 4.5.3). OpenWraith therefore never switches
  profiles within 2 seconds of a write, and every command that writes waits
  those 2 seconds before it finishes ("Waiting for the keyboard to save
  ..."). A restore then proves it: it switches away from every profile it
  wrote and back, reads the written tables again and reports them as
  "persisted"; a write the keyboard did not keep is reported as LOST and the
  restore fails (run it again, or undo it). `set trigger` / `set light` only
  wait, so that the lighting does not change for a small edit.
- **Do not switch profiles on the keyboard while OpenWraith runs.** It checks
  the active profile before every table and stops if it changed. Also do not
  switch profiles or unplug the keyboard while a command is still waiting for
  the keyboard to save: the wait ends by checking the active profile again,
  and a profile change (or a lost connection) before the save is reported as
  "MAY BE LOST" with exit code 1, never as done. Check the values, then run
  the command again or undo it. Ctrl+C during that wait stops a restore
  before it selects the next profile.
- **Shared tables are checked at the end.** For custom colour table 1, the
  dead band, the Fn keymap and the advanced keys it is not known yet whether
  each profile has its own copy. When a restore writes one of them and covers
  several profiles, it reads every profile again at the end and fails if a
  table no longer holds the backup's values (the "final check"). A restore
  limited to some profiles warns that such a write may change the others.
- Press **Ctrl+C** to stop: OpenWraith finishes the current table, switches
  back to the original profile and reports what was written. Inside the
  keymap group (macros, keymaps, advanced keys) it skips the group's other
  tables but still writes the Fn keymap, without which the keyboard would
  not keep what was already written, and stops after it. A second Ctrl+C
  stops immediately. The final check of a restore is not interrupted.

## Build (Windows)

1. Install Rust with [rustup](https://rustup.rs/) (the default
   `x86_64-pc-windows-msvc` toolchain). The MSVC toolchain needs the Visual
   Studio Build Tools with the "Desktop development with C++" workload, which
   also compiles the bundled hidapi C library.
2. Build:

   ```powershell
   git clone https://github.com/Zeruh31/openwraith
   cd openwraith
   cargo build --release
   ```

3. The program is `target\release\openwraith.exe`. No driver is needed: the
   keyboard's configuration interface is a standard HID device.

`cargo build --release` builds the command-line tool and the libraries only
(the workspace's default members); the CLI needs Rust 1.85 or newer. The
desktop app is built with `npm --prefix app run tauri build` (see
[Desktop app development](#desktop-app-development)); `cargo build -p
openwraith-cli --release` names the CLI explicitly.

### Linux

Install `pkg-config` and the udev development files (Debian/Ubuntu:
`sudo apt install pkg-config libudev-dev`), then `cargo build --release`
(the CLI only; or explicitly `cargo build -p openwraith-cli --release`).

The workspace-wide commands (`cargo test --workspace`, `cargo clippy
--workspace`) also build the desktop app, which needs Rust 1.90 and the Tauri
Linux prerequisites (Debian/Ubuntu: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`,
`libsoup-3.0-dev`, `librsvg2-dev`, `libayatana-appindicator3-dev`). The
desktop app is not tested on Linux yet.
To use the keyboard without root, add a udev rule, for example
`/etc/udev/rules.d/70-openwraith.rules`:

```
KERNEL=="hidraw*", ATTRS{idVendor}=="2e3c", ATTRS{idProduct}=="c36[5-8]", MODE="0660", TAG+="uaccess"
```

and reload it with `sudo udevadm control --reload-rules && sudo udevadm trigger`.

macOS should work through hidapi but is untested.

## Usage

Read-only:

```text
openwraith list                         # connected keyboards (nothing is opened)
openwraith info                         # model, firmware, profile, capabilities
openwraith profile                      # the active onboard profile
openwraith backup                       # active profile -> openwraith-W75HEARGB-<UTC time>.owbackup.json
openwraith backup -o my-w75.owbackup.json
openwraith show triggers                # actuation / Rapid Trigger / dead band per key
openwraith show light                   # light effect and per-key colour tables
openwraith show keymap                  # remapped keys on the base and Fn layers
openwraith show macros                  # the ten macro slots
openwraith show all --from my-w75.owbackup.json   # read a backup instead of the keyboard
openwraith show triggers --json         # machine-readable output
openwraith restore my-w75.owbackup.json --dry-run # compare a backup with the keyboard
```

Writing. `set` and `restore` save a safety backup, show the plan and ask
before they change a setting (unless `--yes`); `profile select` and
`backup --all-profiles` only switch profiles and do not ask:

```text
openwraith backup --all-profiles -o all.owbackup.json   # all four profiles (profile switches only)
openwraith profile select 2                              # switch the active profile
openwraith set trigger W --actuation 1.20                # actuation point
openwraith set trigger W A S D --rapid-trigger 0.30      # Rapid Trigger, same press / release
openwraith set trigger W --rapid-trigger 0.30/0.50       # separate press / release
openwraith set trigger "Caps Lock" --no-rapid-trigger
openwraith set light --mode static --color 0000ff --brightness 4
openwraith restore all.owbackup.json                     # every profile in the backup
openwraith restore all.owbackup.json --profile 2         # only profile 2
```

Keys are given as `openwraith show triggers` prints them (case-insensitive:
`W`, `"Caps Lock"`, `L-Shift`), as an index (`#46`) or a position (`r2c2`).
Distances are millimetres in steps of 0.02 mm. Light modes are the ones the
keyboard lists (`static`, `breath`, `wave`, ...) and `custom` (the per-key
colour table).

`restore` options: `--dry-run` only compares (it reads the active profile and
never writes; add `--against FILE` to compare with another backup instead of
the keyboard), `--profile N` (repeatable) limits the restore to some profiles,
`--packets` prints every packet in hex, `--json` prints the plan and the result
as JSON (when it asks, the plan is shown on stderr before the question),
`--allow-firmware-mismatch` is needed when the keyboard runs other firmware
than the backup was made with.

Global options: `--device <PATH>` picks a keyboard when several are connected
(paths come from `openwraith list`), `--verbose` prints session statistics,
`--safety-dir DIR` chooses where safety backups go. `backup` refuses to
overwrite an existing file unless `--force` is given. Exit codes: 0 when
everything planned was written and verified (or there was nothing to do), 1
on a failure or when the change was not confirmed (the output says whether
anything was written), 2 for a usage error.

**How sure are the values?** Decoded fields follow the confidence labels of
[docs/PROTOCOL.md](docs/PROTOCOL.md). In the text output, a `*` marks values
whose meaning comes from the vendor configurator's code and is not yet
confirmed on the wire (for example the light direction, the SOCD hotkey, the
Win-Lock bits or the dead-band order); a legend line at the end says so. The
JSON output carries a `note` field with the same message. The raw bytes are
authoritative. A reply that cannot be decoded is listed at the end of the
output (`undecoded` in JSON) and is kept raw in backups.

Example (`openwraith show triggers`, shortened):

```text
Triggers (active profile 1)
  Actuation range 0.10 mm - 3.40 mm

  Key        Idx  Pos    Actuation  RT   Press    Release  Dead band top/bottom*
  Esc          0  r0c0   1.70 mm    off  0.02 mm  0.02 mm  0.00 mm / 0.00 mm
  Q           45  r2c1   1.70 mm    off  0.02 mm  0.02 mm  0.00 mm / 0.00 mm
  W           46  r2c2   1.00 mm    on   0.30 mm  0.30 mm  0.00 mm / 0.00 mm
  ...
  82 keys, actuation 1.00 mm - 1.70 mm; Rapid Trigger on 1 key(s): W.

* meaning taken from the vendor configurator's code, not yet confirmed on the wire; see docs/PROTOCOL.md
```

Example (`openwraith restore my-w75.owbackup.json --dry-run`, shortened):

```text
Restore plan (dry run: nothing is written)
  Backup    made 2026-10-24T07:30:00Z, firmware 3.22.07
  Keyboard  W75HEARGB, firmware 3.22.07, read 2026-10-24T17:53:28Z, active profile 1

Profile 1 (active)
  Write                                       Packets  Change
  trigger of W (r2c2)                               1  1.70 mm, RT off -> 1.00 mm, RT 0.30 mm
  main light with mode change (17 00, 07 00)        2  Static, colour #0000ff, brightness 4, speed 3 -> Custom (per-key), colour #ff0000, brightness 4, speed 3
  Unchanged (the keyboard already holds the backup's values):
    dead band, base keymap, Fn keymap, custom colours 1, side light, custom colours 2, ...

Total: 2 writes (3 packets) in profile 1.
```

### After a firmware update

A firmware update can reset the keyboard's settings. To get them back:

1. Before updating, close the vendor site and run
   `openwraith backup --all-profiles -o before-update.owbackup.json`.
2. Update the firmware with the vendor's tool.
3. Run `openwraith restore before-update.owbackup.json --dry-run
   --allow-firmware-mismatch` and read the plan. Table formats may change with
   a firmware update, so check that the values make sense. If the update
   changed the keyboard's distance unit (the plan warns that the trigger
   parameters differ), trigger and dead-band values are listed as not
   written: the stored numbers would mean other distances.
4. Run `openwraith restore before-update.owbackup.json
   --allow-firmware-mismatch`, compare the plan again and answer `y`.

### Backup files

A backup (`*.owbackup.json`) is pretty-printed JSON. For every read request it
stores **all reply reports raw** (63 bytes each, as hex, including bytes the
keyboard leaves after the declared length) and a decoded view next to them.
The raw reports are authoritative: when a backup is loaded the decoded views,
the device header (model, firmware) and the active profile are rebuilt from
them, so a backup stays useful even if a later version decodes a field
differently, and an edited header cannot contradict the replies. A backup is
written to a temporary file first and then moved into place, so an
interrupted save never leaves a half-written file. Backups contain your
personal configuration; the repository's `.gitignore` excludes them.

There are two kinds:

- **Schema 1** (`openwraith backup`): the active onboard profile plus the
  tables whose profile scope is unverified (custom colour tables, polling
  rate, switch types). Phase 1 versions read these files too.
- **Schema 2** (`openwraith backup --all-profiles`): a complete read of each
  of the four profiles. The top-level entries hold the profile that was
  active; `profiles` holds the others. `show` and `info` show the active one;
  add `--profile N` for another (`openwraith show triggers --from all.owbackup.json
  --profile 2`). Older OpenWraith versions refuse these files.

Triggers, the main light, the base keymap and the macros are per profile
(seen in the captures); for the other tables the scope is not known yet, so
restore re-reads such a table before writing it, skips it when it already
matches, and checks every profile again at the end (PROTOCOL.md open question
Q3).

### Trying it without a keyboard

The desktop app has a device simulator and a browser preview (see
[Desktop app development](#desktop-app-development)). For the CLI, the USB
captures in `captures/` can be replayed instead of a keyboard:

```text
cargo run -p openwraith-hid --features mock --example mock_backup -- captures/capture_06_powercycle_W_persisted.json sample.owbackup.json
cargo run -p openwraith-cli -- show all --from sample.owbackup.json
cargo run -p openwraith-cli -- restore sample.owbackup.json --dry-run --against sample.owbackup.json
```

## Development

| Path | What |
|---|---|
| `crates/openwraith-protocol` | Pure protocol library: framing, read requests, the nine confirmed write forms, response matching, decoders, W75 key table. No I/O. |
| `crates/openwraith-hid` | Transports (read-only and writable), session, device summary, backups (one or all profiles), the guarded writer, restore, edits, safety backups, the capture-replay mock and the device simulator. |
| `crates/openwraith-cli` | The `openwraith` command. |
| `crates/openwraith-app-core` | The desktop app without Tauri: the device worker (one thread owns the keyboard), the operations behind the confirmation dialogs, the IPC types (TypeScript generated from them) and the simulator backend. |
| `app/src-tauri` | The desktop app's Tauri 2 shell: commands, progress channels, file dialogs, window lifecycle. |
| `app/src` | The desktop app's frontend (React, TypeScript); `ipc/bindings` and `preview/fixtures` are generated by `cargo test -p openwraith-app-core` (`UPDATE_BINDINGS=1`, `UPDATE_FIXTURES=1`). |
| `docs/PROTOCOL.md` | The protocol specification, with a confidence label on every fact. |
| `docs/ARCHITECTURE.md` | Crate layout, how read-only paths and writes are guarded, data flow, backup format, adding a model. |
| `docs/HARDWARE_TESTS.md` | The hardware test checklist, run by hand: H0-H11 for the CLI's writes, A1-A10 for the desktop app, and the results log. |
| `captures/` | Real USB traffic of the maintainer's W75, used by the tests. |

```text
cargo test --workspace                              # never touches a real keyboard
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

The `--workspace` commands include the desktop app's Tauri crate: they need
Rust 1.90 or newer (and, on Linux, the Tauri prerequisites listed above).

Every test runs against the captures (through a mock transport) or against a
simulated keyboard; no test opens a HID device. Contributions must never copy
code or data files from the vendor's site: work from
[docs/PROTOCOL.md](docs/PROTOCOL.md) and from your own captures.

### Desktop app development

Prerequisites: the Rust toolchain above (the app needs Rust 1.90 or newer),
Node 22.22.2+, 24.15+ or 26+ (the versions the pinned Vite, Vitest, jsdom and
ESLint support), and WebView2 (part of Windows 11). Run these from the
repository root:

| Command | What it does |
|---|---|
| `npm --prefix app ci` | Install the exact frontend dependencies from `app/package-lock.json`. |
| `npm --prefix app run check` | TypeScript (strict), ESLint and the frontend tests (Vitest). |
| `npm --prefix app run dev:preview` | **Browser preview, no keyboard and no Rust:** the UI on recorded fixtures at <http://localhost:1421/?scenario=ready>. |
| `npm --prefix app run dev` | The desktop app on the **device simulator** (a debug build with the feature `sim`; no HID code is compiled into it). |
| `npm --prefix app run build` | Production frontend (`app/dist`), then a check that no preview code is in it. |
| `npm --prefix app run tauri build` | The release app and the Windows installers: `target\release\bundle\msi\OpenWraith_<version>_x64_en-US.msi` and `target\release\bundle\nsis\OpenWraith_<version>_x64-setup.exe`. The first run downloads the WiX and NSIS tools. It also writes `app/src-tauri/legal/THIRD-PARTY-NOTICES.txt`, which the installers ship next to `LICENSE.txt` (a release build without it fails). |
| `pwsh tools/release-build.ps1` | **The release command:** the same build with the builder's local paths (user name) remapped out of the executable, then a check that the executable contains no user-profile path. Use it for installers you hand out. |
| `npm --prefix app run dev:hid` | The app with a real keyboard (maintainers only; close the wraith.software site first). |

**Simulator mode** (`npm --prefix app run dev`). The window title says
"OpenWraith [SIMULATOR]" and a purple banner says that no keyboard is used.
The simulated W75 is the library's `DeviceSim` (seeded from a capture of the
maintainer's keyboard) with the real firmware's delays: it takes 2 seconds
to observe, waits for the keyboard to save, and so on. The **Simulator**
badge in the header opens the simulator controls: unplug and plug in again,
traffic from the vendor site, a profile switch on the keyboard (with or
without a report), a factory reset, a slow save (a write is then LOST), a
dropped reply, an unplug after 3 packets, and counters (the "violations"
counter must stay 0). `OPENWRAITH_SIM_SPEED` scales the simulated time
(default 1.0), and `OPENWRAITH_SIM_DIR` moves the simulator's safety
backups and journal (default `%LOCALAPPDATA%\io.github.zeruh31.openwraith\sim\`);
they never go to the real safety folder. The simulator exists only in debug
builds: a release build with the feature `sim` does not compile, the
simulator's commands are registered and allowed only in simulator builds,
and the simulator configuration (`app/src-tauri/tauri.sim.conf.json`) makes
no installer.

**Browser preview** (`npm --prefix app run dev:preview`). The frontend runs
in a normal browser with a mocked IPC that replays fixtures the core's tests
recorded from the real worker on the simulator. URLs take `scenario` (an id
from `app/src/preview/scenarios/*.ts` or a fixture id), `lang=en|tr`,
`theme=light|dark`, `speed=0|1|4` (progress replay speed), `freeze=1` (no
animations, no event replay and no scenario button, for screenshots),
`fallback=0` and a route after `#`, for example
`http://localhost:1421/?scenario=other_program&lang=tr&theme=dark#/device`.
Without `freeze=1`, the "Preview" button in the header picks another
scenario.

**Generated files.** The TypeScript bindings (`app/src/ipc/bindings/`), the
W75 key list (`app/src/generated/w75Keys.ts`) and the preview fixtures
(`app/src/preview/fixtures/`) are generated from the Rust code and checked
by `cargo test`. After changing an IPC type or a recorded scenario, run
`UPDATE_BINDINGS=1 cargo test -p openwraith-app-core bindings` or
`UPDATE_FIXTURES=1 cargo test -p openwraith-app-core fixtures`.

**Dependencies.** New crates and npm packages are pinned exactly, and only
versions at least two weeks old are used (`app/.npmrc` sets npm's `before`;
`tools/check-dep-dates.py` lists the crates in `Cargo.lock` released after
a cutoff date). How the app is built is described in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#the-desktop-app).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

---

## Türkçe

OpenWraith, **Wraith W75** manyetik (Hall-effect) klavyenin tarayıcı tabanlı
yapılandırma sitesinin yerine geçmeyi amaçlayan açık kaynaklı bir projedir.
Klavyeyle doğrudan USB HID üzerinden konuşur; tarayıcı, hesap veya üçüncü
taraf analitik betiği gerekmez. Bu depoda bir Rust kütüphanesi, `openwraith`
komut satırı aracı ve **OpenWraith masaüstü uygulaması** (Tauri 2, önce
Windows) var. Bunlar klavyenin ayarlarını gösterir, tek profilin ya da dört
onboard profilin hepsinin kayıpsız yedeğini alır, yedeği geri yükler,
aktivasyon noktasını, Rapid Trigger ayarını ve ana ışığı değiştirir.

> **Uyarı.** OpenWraith bağımsız bir projedir; Wraith veya üretici ile **hiçbir
> bağlantısı yoktur, onlar tarafından onaylanmamış veya desteklenmemektedir.**
> Protokol, birlikte çalışabilirlik amacıyla bakımcının kendi klavyesinin USB
> kayıtlarından ve herkese açık yapılandırma sitesinin incelenmesinden
> belgelenmiştir; sitenin kodu projede yer almaz. Kullanım sorumluluğu size
> aittir.

**Durum: Faz 3 (masaüstü uygulaması v0.1).** Masaüstü uygulaması (Windows)
yedekleme ve geri yükleme, aktivasyon ve Rapid Trigger, ana ışık ayarlarını
kapsar; arayüz Türkçe ve İngilizcedir. Komut satırı aracının salt okunur
komutları: `list` (bağlı klavyeler, hiçbiri açılmaz), `info`, `profile`,
`backup` (aktif profil), `show` (tablo veya `--json`) ve
`restore ... --dry-run`. Klavyeye yazan komutlar: `backup --all-profiles`
(yalnızca profil geçişi yapar), `profile select N`, `set trigger`, `set light`
ve `restore`. Salt okunur ve yazan komutlar bakımcının klavyesinde
(firmware V3.22.07, USB `2e3c:c368`) denendi; sonuçlar
[docs/HARDWARE_TESTS.md](docs/HARDWARE_TESTS.md) dosyasındaki kayıtta.
**Masaüstü uygulaması şimdiye kadar yalnızca cihaz simülatörüne karşı
çalıştırıldı; donanım denetimleri (aynı dosyada A1-A10) henüz yapılmadı.**
Tuş ataması ve makro düzenleme, tuş başına renkler, SOCD, switch tipi, yan
ışık, polling rate ve Win-Lock yazmaları sonraki aşamalarda gelecek.

### Masaüstü uygulaması

Uygulama, yedekleme, geri yükleme, tetikleme ayarları ve ana ışık için komut
satırı aracının yaptığını görsel bir klavye ve her yazma için bir onay
penceresiyle yapar. Aynı kütüphaneyi kullandığı için aşağıdaki güvenlik
kurallarının hepsi uygulama için de geçerlidir.

| Sayfa | Neler yapılabilir |
|---|---|
| **Klavye** | Bağlantı, model, firmware, etkin profil; yeniden okuma; klavyeyi bırakma. Üst çubuktaki 1-4 düğmeleri etkin profili değiştirir. |
| **Aktivasyon ve Rapid Trigger** | W75 üzerinde tuş seçme (tıklama, Shift + tıklama, sürükleme, Ctrl + A, WASD, oklar, bir sıra, aynı ayardaki tuşlar), aktivasyon noktasını 0,02 mm adımlarla ayarlama ve Rapid Trigger: kapalı, tek hassasiyetle açık ya da basma ve bırakma ayrı. |
| **Işık** | Ana ışığın efekti (klavyenin listesi ve Özel), rengi, parlaklığı ve hızı. |
| **Yedekle ve geri yükle** | Dört profilin hepsini ya da yalnızca etkin profili yedekleme; planı gördükten sonra bir yedeği (tüm ya da bazı profilleriyle) geri yükleme; güvenlik yedeklerini listeleme ve geri yükleme. |
| **Ayarlar** | Dil (Windows'un dili, English, Türkçe), tema (Windows'un teması, açık, koyu), güvenlik yedeği klasörü, hakkında. |

v0.1 sürümünde olmayanlar: tuş atamaları, makrolar, tuş başına renkler, yan
ışık, dead band, SOCD, polling rate ve Win-Lock.

**Kurulum (Windows 10 ve 11, x64):**

- `OpenWraith_0.1.0_x64-setup.exe` yalnızca geçerli kullanıcı için, yönetici
  izni istemeden kurar (Windows'un diline göre Türkçe ya da İngilizce);
  `OpenWraith_0.1.0_x64_en-US.msi` bilgisayardaki herkes için kurar. Kurulum
  dosyalarını aşağıdaki geliştirici komutlarıyla kendiniz üretebilirsiniz;
  `target\release\bundle\nsis\` ve `target\release\bundle\msi\` klasörlerine
  yazılırlar.
- Kurulum dosyaları henüz imzalı değil. Windows SmartScreen ilk açılışta
  "Windows bilgisayarınızı korudu" uyarısı verir: "Ek bilgi" ve ardından
  "Yine de çalıştır" seçin.
- WebView2 Windows 11'de hazır gelir. Yoksa kurulum programı Microsoft'un
  WebView2 kurulumunu indirir (tek ağ erişimi budur; uygulamanın kendisi ağı
  hiç kullanmaz).
- Kaldırmak için Windows Ayarlar > Uygulamalar. Güvenlik yedekleriniz
  silinmez.

**Kullanım:**

1. **Bütün wraith.software sekmelerini kapatın** (arka plandaki bir sekme de
   klavyeyle konuşur), sonra klavye kabloyla bağlıyken OpenWraith'i açın.
2. Üst çubukta 2 saniye boyunca "Klavyeyi başka bir programın kullanmadığı
   denetleniyor…" yazar (OpenWraith yalnızca dinler); ardından klavye okunur
   ve "Wraith W75 · FW 3.22.07" görünür.
3. Her zaman geri dönebilmek için **dört profilin hepsini bir kez yedekleyin**
   (Yedekle ve geri yükle sayfası).
4. Bir ayarı değiştirin, **İncele ve yaz…** düğmesine basın, planı okuyun ve
   yazma düğmesine basın. Her açılıştaki ilk yazmada wraith.software
   sekmelerinin kapalı olduğunu onaylamanız istenir.
5. Rapor neyin yazılıp doğrulandığını söyler, değişiklikten önce alınan
   **güvenlik yedeğini** gösterir ve **Geri al…** seçeneği sunar (bu, o
   güvenlik yedeğinin kendi planı ve onayıyla yapılan olağan bir geri
   yüklemesidir).

**Uygulamada güvenlik:**

- **Siz onaylamadan hiçbir şey yazılmaz.** Her değişiklik önce salt okunur
  olarak planlanır ve gösterilir; yazma düğmesi yarım saniye sonra etkinleşir,
  Enter tuşu ona basmaz ve bir onay tek bir yazma yapar. Kaydırıcılar ve
  alanlar siz onaylayana kadar klavyede hiçbir şeyi değiştirmez.
- **Önce güvenlik yedeği:** her ayar değişikliğinden ve her geri yüklemeden
  önce, komut satırı aracıyla aynı klasöre
  (`%LOCALAPPDATA%\OpenWraith\safety-backups` ya da `OPENWRAITH_SAFETY_DIR`)
  bir güvenlik yedeği kaydedilir. Uygulama bu yolu gösterir; güvenlik
  yedekleri hiçbir zaman silinmez.
- **Klavyenin kaydetmesi beklenir.** Klavye bir değişikliği aldıktan yaklaşık
  1 saniye sonra kaydeder; her yazma bitmeden önce 2 saniye bekler ve geri
  sayım gösterir. Bu sırada klavyeyi çıkarmayın ve klavyeden profil
  değiştirmeyin. Geri yükleme ayrıca klavyenin yazılanı sakladığını (başka
  bir profile geçip geri dönerek) denetler, saklamadıysa "KAYBOLDU" diye
  bildirir.
- **Başka bir program (üreticinin sitesi):** Boştayken uygulama yalnızca
  dinler. Klavyeyle başka bir program konuşursa OpenWraith okumalar dahil
  hiçbir şey göndermez, bir uyarı gösterir ve siz siteyi kapatıp "Kapattım,
  yeniden denetle" düğmesine basana kadar yazmayı duraklatır.
- **Klavyeden profil değiştirme** (Fn + 1-4) fark edilir: uygulama klavyeyi
  yeniden okur ve eski profil için yapılmış bir planı reddeder.
- **Klavye sıfırlanırsa** (Fn + Boşluk) uygulama hemen durur ve siz "Klavyeyi
  yeniden oku" seçeneğine basana kadar hiçbir şey göndermez.
- **Yazma sürerken pencereyi kapatmak** ertelenir: pencere açık mı kalsın,
  yoksa güvenli bir noktada (kaydetme beklendikten sonra) durdurulup kapatılsın
  mı diye sorulur.
- **Yarıda kalan bir yazma** (çökme ya da elektrik kesintisi) bir sonraki
  açılışta, güvenlik yedeğini geri almak için bir düğmeyle gösterilir.
- **Uygulama açıkken komut satırı aracını çalıştırmayın;** ikisi birden
  klavyeyle konuşur. Üst çubuktaki "Klavyeyi bırak" uygulamayı kapatmadan
  klavyeyi serbest bırakır.
- 3.22.07 dışındaki bir firmware, Klavye sayfasında ve her onay penceresinde
  "henüz denenmedi" olarak belirtilir.
- Hesap, telemetri ve ağ yok. Uygulama yalnızca kendi içeriğini gösterir
  (sıkı bir içerik güvenliği politikası; pencere yalnızca uygulamanın kendi
  komutlarını çağırabilir).

Uygulamanın dosyaları: güvenlik yedekleri yukarıdaki klasörde; ayarlar
`%APPDATA%\io.github.zeruh31.openwraith\settings.json` dosyasında; işlem
günlüğü (yarıda kalan bir yazma için) `%LOCALAPPDATA%\io.github.zeruh31.openwraith\state\`
klasöründe.

### Güvenlik

Bu kurallar kütüphanenindir; komut satırı aracı ve masaüstü uygulaması
ikisi de bunlara uyar.

- **Salt okunur komutlar salt okunur kalır.** `list`, `info`, `show`,
  `profile` (`select` olmadan), `restore --dry-run` ve `backup`
  (`--all-profiles` olmadan), klavyeyi yazma gönderemeyen bir aktarım tipiyle
  açar; bunu derleyici denetler.
- **Yazan komutlar:** `profile select`, `set`, `restore` ve
  `backup --all-profiles` (yalnızca profil geçişi). Bu komutlar yalnızca
  gerçek donanımda doğrulanmış dokuz yazma komutunu gönderebilir. Sıfırlama,
  kalibrasyon, tuş yolu testi, SOCD, switch tipi, yan ışık, polling rate ve
  Win-Lock yazmaları kodda oluşturulamaz bile.
- **Önce güvenlik yedeği.** OpenWraith bir ayarı değiştirmeden önce (`set`,
  `restore`) klavyeyi okur, bir güvenlik yedeği kaydeder, kaydedilen dosyayı
  geri okuyup denetler; ancak ondan sonra neyin değişeceğini gösterir ve
  `[y/N]` diye sorar (`--yes` ile sormaz). `profile select` ve
  `backup --all-profiles` yalnızca profil değiştirir, hiçbir ayarı
  değiştirmez ve sormaz. Her yazma paket paket gönderilir, klavyenin onayı
  beklenir ve yazılan değer klavyeden geri okunarak karşılaştırılır. Sonunda
  değişikliği geri alan komut yazdırılır:
  `openwraith restore <güvenlik yedeği>`. Hiçbir şey kendiliğinden geri
  alınmaz.
- **Güvenlik yedeklerinin yeri:** Windows'ta
  `%LOCALAPPDATA%\OpenWraith\safety-backups`, Linux'ta
  `$XDG_DATA_HOME/openwraith/safety-backups` (yoksa
  `~/.local/share/openwraith/safety-backups`), macOS'ta
  `~/Library/Application Support/OpenWraith/safety-backups`. Başka bir klasör
  için `--safety-dir KLASÖR` seçeneğini ya da `OPENWRAITH_SAFETY_DIR` ortam
  değişkenini kullanın. Güvenlik yedekleri hiçbir zaman otomatik silinmez.
- **wraith.software sekmesini tamamen kapatın.** Herhangi bir komuttan, hele
  yazan bir komuttan önce sitenin açık olduğu tüm sekmeleri kapatın; başka bir
  sekmeye geçmek yetmez. Tarayıcılar arka plandaki sekmelerin zamanlayıcılarını
  yavaşlattığı için gizli bir sekme fark edilmeyebilir. Site OpenWraith'in
  yanıtlarını da alır: bunun üzerine ayarları yeniden okur, sakladığı kopyayı
  yeniden yazar, kendi (etkisiz) tetikleme yazmasını gönderir ve tuş başına
  değerleri yanlış tuşlara kaydedebilir (PROTOCOL.md 4.5.1, 4.11.2, 6.5).
  OpenWraith yazmadan önce 2 saniye dinler ve başka bir program görürse
  yazmayı reddeder; başka bir program fark ettiği her komuttan sonra da uyarı
  verir.
- **Klavye sıfırlama bildirimine asla cevap verilmez.** Fn + Space birkaç
  saniye basılı tutulursa klavye kendini sıfırlar ve bir bildirim gönderir.
  Site bu bildirime kendi sıfırlama komutuyla cevap verir; OpenWraith ise
  hemen durur ve bir daha hiçbir şey göndermez, eski profile geri geçmeyi
  bile denemez.
- **Geri yükleme ne yazar?** Yedekteki baytların aynısını, tablo tablo:
  tetikleme ayarları (her pakette tek tuş), dead band, tuş atamaları,
  makrolar, boşaltılan gelişmiş tuşlar, 1 numaralı özel renk tablosu ve ana
  ışık. Doğrulanmış yazma komutu olmayan tablolar (yan ışık, 2 numaralı renk
  tablosu, SOCD, switch tipleri, polling rate, Win-Lock / 6 tuş) yalnızca
  karşılaştırılır ve "not written" (yazılmadı) olarak listelenir. Geri yükleme
  yedekte olmayan varsayılanları asla eklemez (site tuş tablosuna kendi düğme
  varsayılanlarını yazar; OpenWraith yazmaz). Tuş atamaları, makrolar ve
  gelişmiş tuşlar bir bütün olarak geri yüklenemiyorsa (örneğin boş olmayan
  bir gelişmiş tuş, doğrulanmamış bir yazma biçimi gerektirdiği için) bu grup
  tümüyle atlanır ve plan nedenini söyler. Grup, sitenin sırasıyla yazılır
  (makrolar, ana tuş tablosu, Fn tuş tablosu, gelişmiş tuşlar). Klavye
  değişen ana tuş tablosunu yalnızca ardından Fn tuş tablosu yazılırsa tutar
  (gerçek klavyede bulundu, docs/PROTOCOL.md 4.8.2); bu yüzden Fn tuş
  tablosu hemen ardından, gerekirse değişmeden yeniden yazılır. Plan bunu
  "Fn keymap (18 02)" olarak, nedenini Change sütununda gösterir
  ("unchanged, re-sent: ..."); rapor "Fn keymap (18 02) re-sent unchanged to
  make the keyboard save the base keymap" olarak listeler ve paketlerinin her
  birinin onaylanmasını şart koşar. Önlem olarak (denenmemiş bir tahmin),
  yalnızca makro ya da gelişmiş tuş yazan bir geri yükleme de değişmemiş Fn
  tuş tablosuyla biter.
- **Klavye bir değişikliği yazıldıktan yaklaşık 1 saniye sonra kaydeder.**
  O ana kadar değişiklik yalnızca bellektedir: bu bir saniye içinde profil
  değiştirmek (ya da kabloyu çıkarmak) değişikliği kaybettirir ve geri okuma
  bunu fark edemez (gerçek klavyede bulundu, docs/PROTOCOL.md 4.5.3). Bu
  yüzden OpenWraith bir yazmadan sonraki 2 saniye içinde asla profil
  değiştirmez ve yazan her komut bitmeden önce bu 2 saniyeyi bekler
  ("Waiting for the keyboard to save ..."). Geri yükleme ayrıca bunu kanıtlar:
  yazdığı her profilden başka bir profile geçip geri döner, yazılan tabloları
  yeniden okur ve "persisted" (kalıcı) olarak bildirir; klavyenin tutmadığı
  bir yazma LOST (kayboldu) olarak bildirilir ve geri yükleme başarısız olur
  (yeniden çalıştırın ya da geri alın). `set trigger` / `set light` yalnızca
  bekler; küçük bir değişiklik için ışıklar değişmesin diye profil
  değiştirmez.
- OpenWraith çalışırken klavyeden profil değiştirmeyin; her tablodan önce
  aktif profili denetler ve değiştiyse durur. Bir komut klavyenin kaydetmesini
  beklerken de profil değiştirmeyin ve kabloyu çıkarmayın: bekleme, aktif
  profili yeniden denetleyerek biter; kayıttan önce bir profil değişikliği
  (ya da kopan bağlantı) "MAY BE LOST" (kaybolmuş olabilir) olarak ve çıkış
  kodu 1 ile bildirilir, asla tamamlandı denmez. Değerleri denetleyin, sonra
  komutu yeniden çalıştırın ya da geri alın. Bu bekleme sırasında Ctrl+C,
  geri yüklemeyi bir sonraki profil seçilmeden önce durdurur.
- **Paylaşılan tablolar sonda denetlenir.** 1 numaralı özel renk tablosu,
  dead band, Fn tuş tablosu ve gelişmiş tuşlar için her profilin kendi kopyası
  olup olmadığı henüz bilinmiyor. Geri yükleme bunlardan birini yazar ve
  birden fazla profili kapsarsa, sonunda bütün profilleri yeniden okur ve bir
  tablo artık yedekteki değerleri taşımıyorsa hata verir ("final check").
  Yalnızca bazı profillerle sınırlı bir geri yükleme, böyle bir yazmanın diğer
  profilleri de değiştirebileceği konusunda uyarır.
- **Ctrl+C** ile durdurabilirsiniz: OpenWraith o anki tabloyu bitirir, eski
  profile geri geçer ve neyin yazıldığını bildirir. Tuş grubunun (makrolar,
  tuş tabloları, gelişmiş tuşlar) içindeyse grubun diğer tablolarını atlar,
  ama Fn tuş tablosunu yine de yazar (o olmadan klavye yazılmış olanları
  tutmaz) ve ondan sonra durur. İkinci Ctrl+C hemen durdurur. Geri
  yüklemenin son denetimi yarıda kesilmez.
- Metin çıktısında `*` ile işaretlenen değerlerin anlamı üreticinin sitesinin
  kodundan alınmıştır ve henüz USB trafiğinde doğrulanmamıştır; JSON
  çıktısındaki `note` alanı da bunu belirtir. Esas olan ham baytlardır.

### Komut satırı aracı

**Derleme (Windows):** [rustup](https://rustup.rs/) ile Rust'ı kurun (MSVC
araç zinciri ve Visual Studio Build Tools içindeki "Desktop development with
C++" bileşeni gerekir), sonra depo klasöründe `cargo build --release`
çalıştırın. Program `target\release\openwraith.exe` olur; ayrıca sürücü
gerekmez. `cargo build --release` yalnızca komut satırı aracını ve
kütüphaneleri derler (Rust 1.85 ve üstü yeterlidir); açıkça
`cargo build -p openwraith-cli --release` de yazılabilir. Linux için
yukarıdaki udev kuralına bakın. `cargo test --workspace` gibi bütün çalışma
alanını kapsayan komutlar masaüstü uygulamasını da derler: Rust 1.90 ve
Linux'ta Tauri ön koşulları gerekir (Debian/Ubuntu: `libwebkit2gtk-4.1-dev`,
`libgtk-3-dev`, `libsoup-3.0-dev`, `librsvg2-dev`,
`libayatana-appindicator3-dev`). Masaüstü uygulaması Linux'ta henüz test
edilmedi.

**Kullanım:**

```text
openwraith info
openwraith profile                                  # aktif profil
openwraith backup -o w75-yedek.owbackup.json        # aktif profil (salt okunur)
openwraith backup --all-profiles -o tum.owbackup.json   # dört profil (yalnızca profil geçişi)
openwraith show triggers
openwraith show all --from tum.owbackup.json --profile 2
openwraith restore tum.owbackup.json --dry-run      # yalnızca karşılaştırır, yazmaz
openwraith restore tum.owbackup.json                # planı gösterir, sorar, yazar, doğrular
openwraith restore tum.owbackup.json --profile 2    # yalnızca 2. profil
openwraith profile select 2
openwraith set trigger W --actuation 1.20
openwraith set trigger W --rapid-trigger 0.30/0.50
openwraith set light --mode static --color 0000ff
```

Tuşlar `openwraith show triggers` çıktısındaki adlarıyla (büyük/küçük harf
fark etmez: `W`, `"Caps Lock"`, `L-Shift`), indeksle (`#46`) ya da konumla
(`r2c2`) verilir. Mesafeler milimetredir ve 0,02 mm'nin katı olmalıdır.
Çıkış kodları: her şey yazılıp doğrulandıysa (ya da yapılacak bir şey yoksa)
0, bir hata olduysa ya da değişiklik onaylanmadıysa 1 (çıktı bir şey yazılıp
yazılmadığını söyler), hatalı kullanımda 2.

**Firmware güncellemesinden sonra:** Güncellemeden önce siteyi kapatıp
`openwraith backup --all-profiles -o guncelleme-oncesi.owbackup.json`
çalıştırın. Güncellemeden sonra önce
`openwraith restore guncelleme-oncesi.owbackup.json --dry-run --allow-firmware-mismatch`
ile planı okuyun (tablo biçimleri firmware ile değişebilir, değerlerin mantıklı
olduğunu kontrol edin), ardından aynı komutu `--dry-run` olmadan çalıştırıp
planı yeniden gözden geçirin ve `y` ile onaylayın. Güncelleme klavyenin mesafe
birimini değiştirdiyse (plan tetikleme parametrelerinin farklı olduğunu
söyler), tetikleme ve dead band değerleri yazılmaz: kayıtlı sayılar başka
mesafeler anlamına gelirdi.

**Yedek dosyaları:** Her yanıt raporu ham hâliyle (63 bayt, hex) ve çözülmüş
görünümüyle birlikte saklanır; ham veri esastır. Dosya yüklenirken çözülmüş
görünümler, cihaz bilgisi ve aktif profil ham veriden yeniden üretilir.
`openwraith backup` şema 1 dosyası yazar (aktif profil; Faz 1 sürümleri de
okuyabilir). `openwraith backup --all-profiles` şema 2 dosyası yazar: dört
profilin her biri eksiksiz okunur; `show` ve `info` aktif olanı gösterir,
başka bir profil için `--profile N` ekleyin. Eski OpenWraith sürümleri şema 2
dosyalarını reddeder. Tetikleme ayarları, ana ışık, temel tuş tablosu ve
makrolar profile özeldir (kayıtlarda görüldü); diğer tabloların kapsamı henüz
bilinmiyor, bu yüzden geri yükleme böyle bir tabloyu yazmadan önce yeniden
okur ve zaten eşleşiyorsa atlar.

### Geliştirme

**Masaüstü uygulaması:** Rust (1.90 veya üstü), Node (22.22.2 ve üstü,
24.15 ve üstü ya da 26 ve üstü) ve WebView2 gerekir. Komutlar depo
klasöründe çalıştırılır:

- `npm --prefix app ci`: `app/package-lock.json` dosyasındaki bağımlılıkları
  birebir kurar.
- `npm --prefix app run check`: TypeScript, ESLint ve arayüz testleri.
- `npm --prefix app run dev:preview`: **klavye ve Rust olmadan tarayıcı
  önizlemesi.** Arayüz, kayıtlı senaryolarla
  <http://localhost:1421/?scenario=ready&lang=tr> adresinde açılır. Adres
  `scenario`, `lang=en|tr`, `theme=light|dark`, `speed=0|1|4`, `freeze=1`
  (ekran görüntüsü için animasyonsuz) parametrelerini ve `#` sonrasında bir
  sayfa alır.
- `npm --prefix app run dev`: uygulamayı **cihaz simülatörüyle** başlatır
  (`sim` özellikli bir hata ayıklama derlemesi; içinde HID kodu yoktur).
  Pencere başlığında "[SIMULATOR]" yazar, mor bir şerit gerçek klavye
  kullanılmadığını söyler. Üst çubuktaki **Simülatör** rozeti simülatör
  denetimlerini açar: kabloyu çıkarma ve takma, üreticinin sitesinden gelen
  trafik, klavyeden profil değiştirme, fabrika ayarlarına dönme, yavaş
  kaydetme (yazılan KAYBOLUR), kaybolan bir yanıt ve sayaçlar ("ihlal"
  sayacı 0 kalmalıdır). `OPENWRAITH_SIM_SPEED` simüle edilen zamanı ölçekler;
  simülatörün güvenlik yedekleri ve günlüğü gerçek klasöre değil,
  `%LOCALAPPDATA%\io.github.zeruh31.openwraith\sim\` klasörüne (ya da
  `OPENWRAITH_SIM_DIR`) yazılır. Simülatör yalnızca hata ayıklama
  derlemelerinde vardır: `sim` özelliğiyle bir sürüm (release) derlemesi
  derlenmez ve simülatör yapılandırması kurulum dosyası üretmez.
- `npm --prefix app run tauri build`: sürüm derlemesini ve Windows kurulum
  dosyalarını (`.msi`, `.exe`) üretir. Kurulum dosyalarının `LICENSE.txt`
  yanında taşıdığı `app/src-tauri/legal/THIRD-PARTY-NOTICES.txt` dosyasını da
  yazar (bu dosya olmadan sürüm derlemesi başarısız olur).
- `pwsh tools/release-build.ps1`: **dağıtılacak kurulum dosyaları için sürüm
  komutu.** Aynı derlemeyi, derleyen kişinin yerel yollarını (kullanıcı adı)
  çalıştırılabilir dosyadan çıkararak yapar ve sonra dosyada kullanıcı
  profili yolu kalmadığını denetler.
- `npm --prefix app run dev:hid`: gerçek klavyeyle (yalnızca bakımcılar için;
  önce wraith.software sitesini kapatın).

TypeScript tipleri, W75 tuş listesi ve önizleme senaryoları Rust kodundan
üretilir ve `cargo test` bunların güncel olduğunu denetler; bir IPC tipi
değişince `UPDATE_BINDINGS=1`, kayıtlı bir senaryo değişince
`UPDATE_FIXTURES=1` ile `cargo test -p openwraith-app-core` çalıştırın.

Protokol belgesi: [docs/PROTOCOL.md](docs/PROTOCOL.md). Mimari:
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Lisans: GPL-3.0-or-later.
