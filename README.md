# OpenWraith

OpenWraith is an open-source replacement for the browser-based configurator of
the **Wraith W75** Hall-effect (magnetic switch) keyboard. It talks to the
keyboard directly over USB HID. No browser, no account, and no third-party
analytics are involved.

This repository contains a Rust library, the `openwraith` command line tool
and the **OpenWraith desktop app** (Tauri 2, Windows first). They show the
keyboard's settings, make lossless backups of one or all four onboard
profiles, restore them, and change the actuation point, Rapid Trigger, the
main light, key assignments (base and Fn layer), macros, advanced keys (DKS,
Mod-Tap, Toggle), per-key colours, the dead band and SOCD. They also share
one profile as a file and show live key depths.

> **Disclaimer.** OpenWraith is an independent project. It is **not affiliated
> with, endorsed by, or supported by Wraith** or the keyboard's manufacturer.
> "Wraith" and "W75" are used only to say which hardware this works with. The
> protocol was documented for interoperability from USB captures of the
> maintainer's own keyboard and from an analysis of the public web
> configurator; no vendor code is included (see
> [docs/PROTOCOL.md](docs/PROTOCOL.md)). Use at your own risk.

## Status: Phase 5b (desktop app v0.2, first part)

| Works now | Planned |
|---|---|
| **Desktop app v0.1** (Windows): backup and restore, actuation and Rapid Trigger, main light; English and Turkish | Phase 5b-2 of the app: notification-area (tray) icon, start with Windows, notifications, automatic backups, reset detection and recovery, profile per game, profile names |
| **Desktop app v0.2, first part (in development; simulator-tested):** Keys page (assignments, macros, advanced keys), trigger presets, dead band, SOCD, live key depth, key colours, profile export and import | A hardware run of the app's v0.2 editors (A11 onwards) |
| `openwraith list` / `info` / `show`: devices, model, firmware, triggers, lighting, keymap, macros, advanced keys, SOCD (read-only) | Linux and macOS builds of the app |
| `openwraith backup`: lossless backup of the active profile (read-only) | Switch types, side light, polling rate, Win-Lock (no confirmed write command yet) |
| `openwraith backup --all-profiles`: all four onboard profiles (switches profiles, nothing else) | Other models of the family (W60, W65, W100) |
| `openwraith restore`: restore a backup, with a plan, a confirmation and read-back verification | |
| `openwraith set trigger` / `set light`: actuation, Rapid Trigger (also presets), main light | |
| `openwraith profile [select N]`: show or switch the active profile | |
| **v0.2, hardware-tested (H12-H24):** `set key` / `set macro` / `set advanced` / `set colors` / `set deadband` / `set socd`, `travel` (live key depth), `monitor`, `profile export` / `profile import` | |

Supported hardware: Wraith W75 (product code `W75HEARGB`) connected with the
USB cable, firmware `V3.22.07` (USB `2e3c:c368`). The read-only commands were
checked on the maintainer's keyboard, and so were the commands that write
(the results log in [docs/HARDWARE_TESTS.md](docs/HARDWARE_TESTS.md)).
The desktop app's hardware checks (A1-A10 in the same file) were run on
2026-10-27. **The v0.2 editors of the CLI (the last row) were checked on
the keyboard on 2026-10-27 (H12-H24).** That session found two things the
keyboard does that OpenWraith had not modelled (writing macro slot 0 empties
the other macros; SOCD on/off is one setting for all profiles); both are
handled now, and their re-check on the keyboard (H26, H27) passed on
2026-10-28. The optional H25 was not run. The desktop app's v0.2 editors
drive the same library paths; they were tested on the simulator and with
recorded fixtures only, not yet on the keyboard. The packets use the
command forms the vendor site sent and the keyboard accepted in the
captures, built the way the site's code builds them; the cases captured in
sessions 2-3 match byte for byte (tests compare them), and values whose
effect is still untested are marked "not yet verified on a W75" in the
plan. The product IDs `2e3c:c365`
to `2e3c:c367` are accepted as the spec allows, but unverified.

## Desktop app

The app does what the CLI does, with a visual keyboard and a confirmation
for every write. It uses the same library, so every rule under
[Safety](#safety) applies to it too.

| Page | What you can do |
|---|---|
| **Keyboard** | Connection, model, firmware, active profile; read again; release the keyboard. The header switches the active profile (1-4). |
| **Keys** (v0.2) | **Assignments:** what each key does on the base and Fn layers; change it (a key, a media key, a mouse button, a shortcut with one or two modifiers, a macro, an advanced key), reset it or swap two keys; keys that refer to an empty slot are listed with a one-click reset. **Macros:** the keyboard's 10 macro slots and a macro library on this PC; steps typed or recorded from the keyboard, four play modes, a check that every key is released. **Advanced keys:** Dynamic keystroke (DKS), Tap or hold (Mod-Tap) and Toggle. |
| **Actuation** | **Actuation & Rapid Trigger:** select keys on the W75 (click, Shift + click, drag, Ctrl + A, WASD, arrows, a row, keys with the same setting), set the actuation point (0.02 mm steps) and Rapid Trigger; trigger presets (five built in, your own saved on this PC). **Dead band** (per profile), **SOCD** (the switch and up to 20 key pairs) and **Key depth** (a live view of how far each key is pressed). |
| **Lighting** | **Effect:** the main light's effect (the keyboard's list plus Custom), colour, brightness and speed. **Key colours:** a colour per key, with select, brush, eyedropper, fill, gradient and undo. |
| **Backup & profiles** | Back up all four profiles or the active one; restore a backup (all or some profiles) after a plan preview; **share a profile** (export one profile to a `.owprofile.json` file; import a profile file into any profile, choosing the parts); list and restore the safety backups. |
| **Settings** | Language (Windows' language, English, Türkçe), theme (Windows' theme, light, dark), where your trigger presets are managed, one-time tips, the safety-backups folder, About. |

Not in this version yet: the notification-area (tray) icon, starting with
Windows, notifications, automatic backups, reset detection and recovery,
profiles that follow the game in front, profile names; the side light,
polling rate, switch types and calibration, Win-Lock, and mouse or media
steps in macros (there is no confirmed write command for them yet).

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
3. **Back up all four profiles once** (Backup & profiles), so you can always
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
  site and press "I closed it, check again". A read in progress stops
  too, so a backup of the active profile fails instead of being saved.
- **Profile changes on the keyboard** (Fn + 1-4) are noticed: the app reads
  the keyboard again and refuses any plan made for the old profile.
- **A keyboard reset** (Fn + Space) stops the app at once; it sends nothing
  until you choose "Read the keyboard again".
- **Closing during a write** is deferred: the window asks whether to keep it
  open or to stop safely (at the next safe point, after the save wait) and
  close.
- **An interrupted write** (a crash or a power loss) is shown at every
  start until you dismiss it, with Undo for its safety backup.
- **Don't run the CLI while the app is open:** both would talk to the
  keyboard. "Release keyboard" (header) closes it without quitting the app.
- Firmware other than 3.22.07 is marked "not tested yet" on the Keyboard
  page and in every confirmation.
- No account, no telemetry, no network. The app shows only its own content
  (a strict content security policy and only the app's own commands are
  allowed to the window).

What the v0.2 editors add to that:

- **Changes are drafts until you write them,** per tab; the profile switch
  lists unwritten changes before it discards them. One write is one planned
  operation, with its safety backup and Undo, like every other write.
- **The plan says why each table is written.** The keyboard keeps key
  assignments only when a complete Fn layer is written after them, and an
  advanced key only after a complete Fn layer; the plan shows these extra
  writes with their own reason, never a fixed sentence.
- **Macro 1** (slot 0 on the wire): writing it makes the keyboard empty
  macros 2-10. OpenWraith writes the macros that must keep their content
  again right after it (also after Stop) and puts new macros into 2-10
  first. The plan shows these rows as re-sent; if a write fails part-way,
  the report lists the emptied macros and the keys left on them, and Undo
  brings them back.
- **The keyboard's own Fn shortcuts:** replacing an Fn built-in shows a
  warning; Fn + 1-4 (profile switch) and Fn + Space (held: reset) need an
  extra confirmation tick, and replacing Fn + Space has not been tested.
- **"Not yet verified" badges** mark values the Wraith website sends but
  OpenWraith hasn't yet tested on a W75. They come from the library, never
  from a list in the app: for example SOCD modes 2 and 3, a DKS tap at the
  last stage, Mod-Tap hold times other than 300 ms, advanced keys on the Fn
  layer, shortcuts with right-hand modifiers.
- **SOCD:** the on/off switch is one setting of the whole keyboard (it
  applies to every profile); the pairs belong to the profile. Fn + Right
  Shift toggles it on the keyboard; the app notices, updates the view and
  asks you to review a pending SOCD change again.
- **Dead band** is per profile; values above 1.00 mm are refused, never
  clamped. Presets you save are checked against the keyboard's limits and
  refused when out of range.
- **"Check that the keyboard kept it"** (switches to another profile and
  back) is optional and not ticked: every table these editors write was
  seen to persist on the keyboard.
- **Live key depth** keeps the keyboard busy: nothing else is written while
  it runs, and the header shows **Live**. It stops when you press Stop,
  leave the tab, hide or close the window, after 5 minutes without movement
  and in the other cases the library stops it. If a view may still be on in
  the keyboard (OpenWraith closed or the cable came out while it ran), a
  banner leads to the Key depth tab, where "Stop it" turns it off; typing
  keeps working meanwhile, but OpenWraith may refuse to write settings until
  it is stopped.
- **Sharing a profile:** an exported file never contains the keyboard's USB
  serial number, but macros travel as the keys they press, so anyone you
  give the file to can read them. Importing is a restore of the parts you
  choose (actuation and lighting by default) into the profile you choose,
  with the usual plan, safety backup and Undo; a file that replaces
  Fn + 1-4 or Fn + Space needs its own tick.

Where the app keeps its files: safety backups as above; its settings, your
trigger presets and the tips you dismissed in
`%APPDATA%\io.github.zeruh31.openwraith\settings.json`; the macro library in
`%LOCALAPPDATA%\io.github.zeruh31.openwraith\macro-library.json`; the
operation journal (an interrupted write) and the key-depth marker
(`travel-test-open`: a view that may still be on) in
`%LOCALAPPDATA%\io.github.zeruh31.openwraith\state\`.

## Safety

These rules are the library's; the CLI and the desktop app follow them
alike.

- **Read-only commands stay read-only.** `list`, `info`, `show`, `monitor`,
  `profile` (without `select`), `restore --dry-run`, `profile import
  --dry-run`, every `set ... --dry-run`, `backup` (without
  `--all-profiles`) and `profile export` of the active profile open the
  keyboard through a transport type that cannot send a write at all
  (checked by the compiler, see
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)).
- **These commands write:** `profile select`, `set`, `restore`, `profile
  import`, `backup --all-profiles` and `profile export` of a profile that is
  not active (profile switches only), and `travel` (only the key-depth
  test's open and close). They can only send the write commands that the
  vendor site sent and the keyboard accepted in the captures
  (docs/PROTOCOL.md section 3), in the captured command form (table
  contents whose effect neither the captures nor OpenWraith's hardware tests
  have shown are marked "not yet verified on a W75" in the plan); every
  packet also passes an allowlist right before it is sent. Reset,
  calibration, switch types, side light, custom colour table 2, polling rate,
  gamepad and Win-Lock writes cannot even be built.
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
  triggers (one key per packet), dead band, SOCD, keymaps, macros, advanced
  keys, custom colour table 1 and the main light. Tables without a
  confirmed write command (side light, custom colour table 2, switch types,
  polling rate, Win-Lock / 6-key) are compared and listed as "not written".
  Restore never adds defaults that are not in the backup (the vendor site
  writes its own knob defaults into the keymap; OpenWraith does not). If the
  keymaps, macros or advanced keys cannot be restored as a whole (for
  example because an advanced key is not a DKS, Mod-Tap or Toggle in the
  vendor site's form), that whole group is skipped and the plan says why.
  The group is written in the vendor site's order (macros, base keymap, Fn
  keymap, advanced keys). The keyboard keeps a changed base keymap only when
  the Fn keymap is written after it (found on real hardware, docs/PROTOCOL.md
  4.8.2), so the Fn keymap is then written right after it, unchanged if
  need be. The plan shows it as "Fn keymap (18 02)" with the reason in its
  Change column ("unchanged, re-sent: the keyboard keeps the base keymap only
  when the Fn keymap is written after it"); the report lists it as "Fn keymap
  (18 02) re-sent unchanged to make the keyboard save the base keymap", and
  every one of its packets must be acknowledged. The keyboard keeps an
  advanced key when the Fn keymap is written before it in the same save,
  and loses one written without it (found on real hardware, hardware test
  H20), so a restore or an edit that writes advanced-key slots while
  neither keymap changes sends the unchanged Fn keymap before them, as the
  vendor site does. Macro slots are kept on
  their own and need no Fn keymap.
- **Macro slot 0 is special.** Writing macro slot 0 makes the keyboard empty
  macro slots 1-9 of that profile (found on real hardware, docs/PROTOCOL.md
  4.9.2; the vendor site never shows it because it always writes all ten).
  So whenever OpenWraith writes slot 0, it writes every other macro that
  should stay right after it ("re-sent: writing macro slot 0 makes the
  keyboard clear slots 1-9"), also after Ctrl+C. A new macro gets the
  lowest free slot from 1 to 9, and slot 0 only when they are all used. If
  a re-send fails, the report names the macros that were left empty and
  the keys that now do nothing.
- **The keyboard saves a change about 1 second after it is written.** Until
  then it keeps it only in memory: a profile switch (or unplugging the cable)
  in that second loses it, and reading it back cannot tell (found on real
  hardware, docs/PROTOCOL.md 4.5.3). OpenWraith therefore never switches
  profiles within 2 seconds of a write, and every command that writes waits
  those 2 seconds before it finishes ("Waiting for the keyboard to save
  ..."). A restore then proves it: it switches away from every profile it
  wrote and back, reads the written tables again and reports them as
  "persisted"; a write the keyboard did not keep is reported as LOST and the
  restore fails (run it again, or undo it). Hardware tests showed that the
  keyboard keeps every table `set` writes, across a profile switch and a
  USB power cycle, so `set` only waits and the lighting does not change for
  a small edit; `--check-persistence always` checks anyway
  (`auto|always|never`).
- **Do not switch profiles on the keyboard while OpenWraith runs.** It checks
  the active profile before every table and stops if it changed. Also do not
  switch profiles or unplug the keyboard while a command is still waiting for
  the keyboard to save: the wait ends by checking the active profile again,
  and a profile change (or a lost connection) before the save is reported as
  "MAY BE LOST" with exit code 1, never as done. Check the values, then run
  the command again or undo it. Ctrl+C during that wait stops a restore
  before it selects the next profile.
- **SOCD on/off is one switch for all profiles.** The keyboard has a single
  SOCD power setting for all four profiles (Fn + Right Shift switches it
  too), while each profile has its own SOCD key pairs and its own dead band
  (found on real hardware, docs/PROTOCOL.md 4.15). `set socd --on` / `--off`
  says that it changes every profile. A restore writes the power once, the
  backup's setting of the profile that was active when the backup was made,
  and shows it once ("SOCD power (all profiles): on -> off"); when it wrote
  the power in another profile than the one it ends in, it reads the power
  once more there at the end (the "final check"). Importing a profile takes
  only its SOCD pairs and leaves the keyboard's power as it is.
- **Key assignments are guarded.** The knob's entries and the Fn key are
  never changed; an Fn-layer entry that would hide one of the keyboard's
  built-in Fn functions (`openwraith show keymap` lists them) needs
  `--allow-builtin KEY`, and one that would hide a profile switch (Fn +
  1..4) or the reset (Fn + Space) needs `--allow-critical-builtin KEY`.
  Only the entry kinds the vendor site writes can be set; any other entry
  the keyboard holds is kept as read. A key never refers to an empty macro
  or advanced-key slot, and one slot belongs to one key.
- **The live key-depth view is always closed.** `openwraith travel` closes
  the keyboard's travel test at the end, on Ctrl+C (which only asks to stop;
  the third Ctrl+C exits at once), on a profile switch, on another program
  and on errors. If the keyboard still streams key depths from a test left
  open (after a crash), commands that write refuse until `openwraith travel
  --close`.
- Press **Ctrl+C** to stop: OpenWraith finishes the current table, switches
  back to the original profile and reports what was written. Inside the
  keymap group (macros, keymaps, advanced keys) it skips the group's other
  tables but still writes what the group needs: the Fn keymap once the
  base keymap was written (the keyboard would not keep the base keymap
  otherwise; macro slots alone need none), any macro or advanced-key slot
  that a key written or still to be written refers to (or a keymap that
  moves a key away from a slot already rewritten), and, once macro slot 0
  was written, the other macros it would otherwise leave empty (up to nine
  more slots), then stops. When those were the last planned writes,
  everything planned was written: the report says "Ctrl+C received" and
  the exit code stays 0. A second Ctrl+C stops immediately; macros not yet
  written again after slot 0 then stay empty until you undo. The final
  check of a restore is not interrupted.

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
openwraith show keymap                  # remapped keys on both layers, built-in Fn functions
openwraith show macros                  # the ten macro slots, as steps, with their keys
openwraith show advanced                # DKS / Mod-Tap / Toggle slots and their keys
openwraith show socd                    # SOCD power and key pairs
openwraith show all --from my-w75.owbackup.json   # read a backup instead of the keyboard
openwraith show triggers --json         # machine-readable output
openwraith restore my-w75.owbackup.json --dry-run # compare a backup with the keyboard
openwraith monitor --duration 30s       # what the keyboard sends on its own (sends nothing)
```

Writing. `set`, `restore` and `profile import` save a safety backup, show
the plan and ask before they change a setting (unless `--yes`; with
`--dry-run` they only read and show the plan); `profile select` and
`backup --all-profiles` only switch profiles and do not ask:

```text
openwraith backup --all-profiles -o all.owbackup.json   # all four profiles (profile switches only)
openwraith profile select 2                              # switch the active profile
openwraith set trigger W --actuation 1.20                # actuation point
openwraith set trigger W A S D --rapid-trigger 0.30      # Rapid Trigger, same press / release
openwraith set trigger W --rapid-trigger 0.30/0.50       # separate press / release
openwraith set trigger "Caps Lock" --no-rapid-trigger
openwraith set trigger --all --preset fast               # presets: typing, balanced, fast, competitive, factory
openwraith set light --mode static --color 0000ff --brightness 4
openwraith restore all.owbackup.json                     # every profile in the backup
openwraith restore all.owbackup.json --profile 2         # only profile 2
```

Key assignments, macros and advanced keys (the keymap group; written in
the vendor site's order, with the Fn keymap after the base keymap):

```text
openwraith set key Caps=L-Ctrl L-Ctrl=Caps               # swap two keys (base layer)
openwraith set key F12=media:mute "Right"=mouse:wheel-down
openwraith set key --layer fn Q=combo:L-Ctrl+C           # Fn + Q copies
openwraith set key --layer fn F1=media:play-pause --allow-builtin F1
openwraith set key Caps=default                          # back to the keyboard's default
openwraith set macro Caps --steps "A+ 100, A- 100, B+ 100, B-"
openwraith set macro Caps --steps "L-Ctrl+, C+, C-, L-Ctrl-" --play times:2
openwraith set macro Caps --unbind                       # key back to default, slot cleared
openwraith set advanced E dks --point 1.5 --row E:t... --row R:.t..
openwraith set advanced G mod-tap --hold L-Ctrl --time 300ms
openwraith set advanced H toggle
openwraith set advanced H --unbind
```

Colours, dead band, SOCD:

```text
openwraith set colors W=ff0000 A=ff0000 S=ff0000 D=ff0000 --custom-mode
openwraith set colors --all 00ffff Esc=ff0000
openwraith set deadband W A S D --top 0.10 --bottom 0.20
openwraith set socd --on --pair A/D --pair Left/Right:2
openwraith set socd --remove-pair A/D
openwraith set socd --off
```

Live key depth, and sharing a profile:

```text
openwraith travel --duration 60s                 # bars per key; Ctrl+C stops (the test is always closed)
openwraith travel --keys W,A,S,D --json          # one JSON object per depth report
openwraith travel --close                        # close a test a crashed run left open
openwraith profile export 1 -o fps.owprofile.json --label FPS   # no USB serial in the file
openwraith profile import fps.owprofile.json --into 3           # triggers and lighting
openwraith profile import fps.owprofile.json --into 3 --include triggers,lighting,keys
```

Keys are given as `openwraith show triggers` prints them (case-insensitive:
`W`, `"Caps Lock"`, `L-Shift`; short forms such as `caps`, `equal`,
`slash`), as an index (`#46`) or a position (`r2c2`). Distances are
millimetres in steps of 0.02 mm (a DKS press point in steps of 0.1 mm).
Light modes are the ones the keyboard lists (`static`, `breath`, `wave`,
...) and `custom` (the per-key colour table).

`set key` actions: `default`, a key name (`A`, `L-Ctrl`, `F13`, `"KP 1"`),
`usage:0xNN`, `media:NAME` (`mute`, `volume-up`, `volume-down`,
`play-pause`, `next`, `previous`, `stop`, `calculator`, `browser-back`,
`browser-forward`, ...) or `media:0xNN`,
`mouse:left|right|middle|back|forward|wheel-up|wheel-down` (`back` is the
mouse's back button X1, `0x08`, and `forward` X2, `0x10`; the vendor site
labels these two the other way round),
`combo:MOD[+MOD]+KEY`, `macro:N` and `advanced:N` (bind an existing slot). Several `KEY=ACTION` pairs are written
together. Macro steps: `KEY+` presses, `KEY-` releases, then the delay
after the step in ms (the last step has none); at most 40 steps, every
pressed key released; `--play once|times:N|toggle|while-held`. DKS rows:
`KEY:STAGES`, four characters for press, bottom-out, lift and release:
`.` nothing, `t` tap, `h` held (with auto-repeat) until the key reaches the next stage, `r`
release and press again. SOCD modes (what happens while both keys are held): 1 the last input
wins (default), 2 key 1 wins, 3 key 2 wins, 4 the later key only
interrupts the first. Values the plan marks "not yet verified on a W75"
are the vendor site's bytes whose effect OpenWraith has not tested yet:
since the hardware tests of 2026-10-27 only rarer ones, such as SOCD modes
2 and 3, a DKS tap at the release stage, "hold again", Mod-Tap hold times
other than 300 ms, advanced keys on the Fn layer, combinations with a
right-side modifier (R-Ctrl, R-Shift, R-Alt, R-GUI) and most multimedia
keys.

`set` options: `--dry-run` (plan only, read-only), `--packets` (the hex of
every packet), `--json`, `--yes`; for the v0.2 editors also
`--check-persistence auto|always|never` (switch to another profile and back
after writing to check that the keyboard kept it; `auto`, the default, only
for tables not proven on hardware, which since 2026-10-27 means only
advanced-key slots written without the Fn keymap) and, for `set macro` /
`set advanced`, `--layer base|fn`, `--slot N` and `--commit-trigger skip` (a
hardware test: advanced-key slots without the Fn keymap before them, which
the keyboard does not keep; always checked).

`restore` options: `--dry-run` only compares (it reads the active profile and
never writes; add `--against FILE` to compare with another backup instead of
the keyboard), `--profile N` (repeatable) limits the restore to some profiles,
`--packets` prints every packet in hex, `--json` prints the plan and the result
as JSON (when it asks, the plan is shown on stderr before the question),
`--allow-firmware-mismatch` is needed when the keyboard runs other firmware
than the backup was made with.

`profile import` takes the same options (except `--against`) and
`--into N` (the keyboard's profile to write), `--profile N` (the file's
profile, for an all-profile backup), `--include
triggers,lighting,keys,deadband,socd` (default: triggers and lighting) and
`--allow-critical-fn-overrides` (an imported Fn layer may then replace Fn +
1..4 or Fn + Space). Its `--dry-run` reads only the active profile: to see
the plan of an import into another profile, select that profile first
(`openwraith profile select N`), or run it without `--dry-run` (the plan is
shown before the question). `profile export N -o FILE` reads profile N (a profile
that is not active after a question, by switching to it and back), or takes
it from a backup with `--from FILE`; `--label TEXT` is stored in the file.

Global options: `--device <PATH>` picks a keyboard when several are connected
(paths come from `openwraith list`), `--verbose` prints session statistics,
`--safety-dir DIR` chooses where safety backups go. `backup` refuses to
overwrite an existing file unless `--force` is given. Exit codes: 0 when
everything planned was written and verified (or there was nothing to do), 1
on a failure, when the change was not confirmed (the output says whether
anything was written) or when `profile import` left out a category chosen
with `--include` (for example the keys, blocked by a critical Fn override:
the plan names the option that allows it), 2 for a usage error.

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
- **Profile exports** (`openwraith profile export`, e.g. `*.owprofile.json`):
  schema 1 with the scope `profile_export`: one profile, without the USB
  serial number, to share; `openwraith profile import` writes it into any
  profile. Files above 8 MiB are refused.

Triggers, the main light, both keymap layers, the macros, the advanced keys,
custom colour table 1, the dead band and the SOCD key pairs are per profile;
the SOCD power (on/off) is one setting of the whole keyboard (seen in the
captures and OpenWraith's hardware tests, PROTOCOL.md 4.5.4). For the tables
OpenWraith does not write the scope is not known yet (open question Q3); a
restore only compares them.

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
| `crates/openwraith-protocol` | Pure protocol library: framing, read requests, the confirmed write forms (W1-W11 and the travel test), response matching, decoders, W75 key table. No I/O. |
| `crates/openwraith-hid` | Transports (read-only and writable), session, device summary, backups (one or all profiles), the guarded writer, restore, edits, safety backups, the capture-replay mock and the device simulator. |
| `crates/openwraith-cli` | The `openwraith` command. |
| `crates/openwraith-app-core` | The desktop app without Tauri: the device worker (one thread owns the keyboard), the operations behind the confirmation dialogs, the IPC types (TypeScript generated from them) and the simulator backend. |
| `app/src-tauri` | The desktop app's Tauri 2 shell: commands, progress channels, file dialogs, window lifecycle. |
| `app/src` | The desktop app's frontend (React, TypeScript); `ipc/bindings` and `preview/fixtures` are generated by `cargo test -p openwraith-app-core` (`UPDATE_BINDINGS=1`, `UPDATE_FIXTURES=1`). |
| `docs/PROTOCOL.md` | The protocol specification, with a confidence label on every fact. |
| `docs/ARCHITECTURE.md` | Crate layout, how read-only paths and writes are guarded, data flow, backup format, adding a model. |
| `docs/HARDWARE_TESTS.md` | The hardware test checklist, run by hand: H0-H11 for the CLI's Phase 2 writes, H12-H25 for its v0.2 editors, H26-H27 for the fixes after them, A1-A10 for the desktop app, and the results log. |
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
W75 key list (`app/src/generated/w75Keys.ts`), the action catalogue and the
`KeyboardEvent.code` table of the macro recorder
(`app/src/generated/actionCatalog.ts`, `hidCodes.ts`) and the preview
fixtures (`app/src/preview/fixtures/`) are generated from the Rust code and
checked by `cargo test`. After changing an IPC type or a recorded scenario, run
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
onboard profilin hepsinin kayıpsız yedeğini alır, yedeği geri yükler;
aktivasyon noktasını, Rapid Trigger ayarını, ana ışığı, tuş atamalarını
(ana ve Fn katmanı), makroları, gelişmiş tuşları (DKS, Mod-Tap, Toggle),
tuş başına renkleri, ölü bölgeyi (dead band) ve SOCD'yi değiştirir. Ayrıca
bir profili dosya olarak paylaşır ve tuşların ne kadar basıldığını canlı
gösterirler.

> **Uyarı.** OpenWraith bağımsız bir projedir; Wraith veya üretici ile **hiçbir
> bağlantısı yoktur, onlar tarafından onaylanmamış veya desteklenmemektedir.**
> Protokol, birlikte çalışabilirlik amacıyla bakımcının kendi klavyesinin USB
> kayıtlarından ve herkese açık yapılandırma sitesinin incelenmesinden
> belgelenmiştir; sitenin kodu projede yer almaz. Kullanım sorumluluğu size
> aittir.

**Durum: Faz 5b (masaüstü uygulaması v0.2, ilk bölüm).** Masaüstü
uygulamasının v0.1 sürümü (Windows) yedekleme ve geri yükleme, aktivasyon ve
Rapid Trigger, ana ışık ayarlarını kapsar; arayüz Türkçe ve İngilizcedir.
Geliştirilmekte olan v0.2'nin ilk bölümü bunlara Tuşlar sayfasını
(atamalar, makrolar, gelişmiş tuşlar), tetikleme hazır ayarlarını, ölü
bölgeyi, SOCD'yi, canlı tuş derinliğini, tuş renklerini ve profil dışa/içe
aktarmayı ekler; bu düzenleyiciler simülatörde ve kaydedilmiş senaryolarla
sınandı, uygulama üzerinden klavyede henüz denenmedi. Faz 5b-2'ye
kalanlar: bildirim alanı (tepsi) simgesi, Windows ile başlatma,
bildirimler, otomatik yedekler, sıfırlama algılama ve kurtarma, oyuna göre
profil ve profil adları. Komut satırı aracının salt okunur
komutları: `list` (bağlı klavyeler, hiçbiri açılmaz), `info`, `profile`,
`backup` (aktif profil), `show` (tablo veya `--json`), `monitor` ve
`restore ... --dry-run`. Klavyeye yazan komutlar: `backup --all-profiles`
(yalnızca profil geçişi yapar), `profile select N`, `set trigger`, `set light`
ve `restore`. Salt okunur ve yazan komutlar bakımcının klavyesinde
(firmware V3.22.07, USB `2e3c:c368`) denendi; sonuçlar
[docs/HARDWARE_TESTS.md](docs/HARDWARE_TESTS.md) dosyasındaki kayıtta.
v0.2 ile gelen yazan komutlar (`set key`, `set macro`, `set advanced`,
`set colors`, `set deadband`, `set socd`, tetikleme hazır ayarları,
`travel`, `profile export` ve `profile import`) sitenin gönderip klavyenin
kabul ettiği komut biçimlerini, sitenin kodunun kurduğu gibi kullanır;
kayıt oturumları 2-3'te görülen durumlar bayt bayt aynıdır (testler
karşılaştırır), etkisi henüz denenmemiş değerler ise planda "not yet
verified on a W75" diye işaretlenir. **Bu komutlar 2026-10-27'de gerçek
klavyede denendi (aynı dosyada H12-H24).** Bu denemede OpenWraith'in
hesaba katmadığı iki klavye davranışı çıktı: 0 numaralı makro yuvasına
yazmak diğer makroları siler, SOCD'nin açık/kapalı ayarı ise tüm profiller
için tektir. İkisi de artık ele alınıyor; klavyedeki yeniden denemeleri
(H26, H27) 2026-10-28'de geçti. İsteğe bağlı H25 yapılmadı. Masaüstü
uygulamasının
donanım denetimleri (A1-A10) 2026-10-27'de yapıldı. Switch tipi, yan ışık,
polling rate ve Win-Lock yazmaları için doğrulanmış bir komut henüz yok.

### Masaüstü uygulaması

Uygulama, komut satırı aracının yaptığını görsel bir klavye ve her yazma
için bir onay penceresiyle yapar. Aynı kütüphaneyi kullandığı için
aşağıdaki güvenlik kurallarının hepsi uygulama için de geçerlidir.

| Sayfa | Neler yapılabilir |
|---|---|
| **Klavye** | Bağlantı, model, firmware, etkin profil; yeniden okuma; klavyeyi bırakma. Üst çubuktaki 1-4 düğmeleri etkin profili değiştirir. |
| **Tuşlar** (v0.2) | **Atamalar:** her tuşun temel ve Fn katmanında ne yaptığı; değiştirme (bir tuş, medya tuşu, fare düğmesi, bir ya da iki değiştirici tuşlu kısayol, makro, gelişmiş tuş), varsayılana döndürme ya da iki tuşu takas etme; boş bir yuvaya bağlı tuşlar tek tıkla sıfırlanabilir. **Makrolar:** klavyedeki 10 makro yuvası ve bu bilgisayardaki makro kitaplığı; elle yazılan ya da klavyeden kaydedilen adımlar, dört çalma biçimi, her tuşun bırakıldığı denetimi. **Gelişmiş tuşlar:** dinamik tuş vuruşu (DKS), dokun ya da basılı tut (Mod-Tap) ve Toggle. |
| **Aktivasyon** | **Aktivasyon ve Rapid Trigger:** W75 üzerinde tuş seçme (tıklama, Shift + tıklama, sürükleme, Ctrl + A, WASD, oklar, bir sıra, aynı ayardaki tuşlar), aktivasyon noktasını 0,02 mm adımlarla ve Rapid Trigger'ı ayarlama; tetikleme hazır ayarları (beş yerleşik, kendi kaydettikleriniz bu bilgisayarda). **Ölü bölge** (profil başına), **SOCD** (anahtar ve en çok 20 tuş çifti) ve **Tuş derinliği** (her tuşun ne kadar basıldığının canlı görünümü). |
| **Işık** | **Efekt:** ana ışığın efekti (klavyenin listesi ve Özel), rengi, parlaklığı ve hızı. **Tuş renkleri:** her tuşa bir renk; seçme, fırça, damlalık, boyama, renk geçişi ve geri alma. |
| **Yedek ve profiller** | Dört profilin hepsini ya da yalnızca etkin profili yedekleme; planı gördükten sonra bir yedeği (tüm ya da bazı profilleriyle) geri yükleme; **profil paylaşma** (bir profili `.owprofile.json` dosyasına dışa aktarma; bir profil dosyasını seçtiğiniz bölümlerle istediğiniz profile içe aktarma); güvenlik yedeklerini listeleme ve geri yükleme. |
| **Ayarlar** | Dil (Windows'un dili, English, Türkçe), tema (Windows'un teması, açık, koyu), tetikleme hazır ayarlarınızın nerede yönetildiği, bir kez gösterilen ipuçları, güvenlik yedeği klasörü, hakkında. |

Bu sürümde henüz olmayanlar: bildirim alanı (tepsi) simgesi, Windows ile
başlatma, bildirimler, otomatik yedekler, sıfırlama algılama ve kurtarma,
öndeki oyuna göre profil, profil adları; yan ışık, polling rate, switch
türleri ve kalibrasyon, Win-Lock ve makrolarda fare ya da medya adımları
(bunlar için henüz doğrulanmış bir yazma komutu yok).

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
   (Yedek ve profiller sayfası).
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
  yeniden denetle" düğmesine basana kadar yazmayı duraklatır. Süren bir okuma
  da durur; bu yüzden aktif profilin yedeği kaydedilmez, hata verir.
- **Klavyeden profil değiştirme** (Fn + 1-4) fark edilir: uygulama klavyeyi
  yeniden okur ve eski profil için yapılmış bir planı reddeder.
- **Klavye sıfırlanırsa** (Fn + Boşluk) uygulama hemen durur ve siz "Klavyeyi
  yeniden oku" seçeneğine basana kadar hiçbir şey göndermez.
- **Yazma sürerken pencereyi kapatmak** ertelenir: pencere açık mı kalsın,
  yoksa güvenli bir noktada (kaydetme beklendikten sonra) durdurulup kapatılsın
  mı diye sorulur.
- **Yarıda kalan bir yazma** (çökme ya da elektrik kesintisi), siz kapatana
  kadar her açılışta, güvenlik yedeğini geri almak için bir düğmeyle
  gösterilir.
- **Uygulama açıkken komut satırı aracını çalıştırmayın;** ikisi birden
  klavyeyle konuşur. Üst çubuktaki "Klavyeyi bırak" uygulamayı kapatmadan
  klavyeyi serbest bırakır.
- 3.22.07 dışındaki bir firmware, Klavye sayfasında ve her onay penceresinde
  "henüz denenmedi" olarak belirtilir.
- Hesap, telemetri ve ağ yok. Uygulama yalnızca kendi içeriğini gösterir
  (sıkı bir içerik güvenliği politikası; pencere yalnızca uygulamanın kendi
  komutlarını çağırabilir).

v0.2 düzenleyicilerinin bunlara eklediği:

- **Değişiklikler siz yazana kadar taslaktır,** her sekmede ayrı; profil
  değiştirme, yazılmamış değişiklikleri silmeden önce listeler. Bir yazma,
  diğer bütün yazmalar gibi güvenlik yedeği ve Geri al seçeneği olan tek bir
  planlı işlemdir.
- **Plan her tablonun neden yazıldığını söyler.** Klavye tuş atamalarını
  ancak ardından eksiksiz bir Fn katmanı yazılırsa, gelişmiş bir tuşu da
  ancak önünde eksiksiz bir Fn katmanı varsa saklar; plan bu ek yazmaları
  sabit bir cümleyle değil, her birinin kendi nedeniyle gösterir.
- **Makro 1** (iletişimde 0 numaralı yuva): ona yazmak klavyenin 2-10
  numaralı makroları boşaltmasına yol açar. OpenWraith içeriğini koruması
  gereken makroları hemen ardından (Durdur'a basılsa da) yeniden yazar ve
  yeni makroları önce 2-10 arasına koyar. Plan bu satırları yeniden
  gönderilen olarak gösterir; bir yazma yarıda kalırsa rapor boşalan
  makroları ve onlara bağlı kalan tuşları listeler, Geri al bunları geri
  getirir.
- **Klavyenin kendi Fn kısayolları:** bir Fn yerleşik işlevinin yerine başka
  bir şey atamak bir uyarı gösterir; Fn + 1-4 (profil değiştirme) ve
  Fn + Boşluk (basılı tutulunca sıfırlama) ek bir onay kutusu ister ve
  Fn + Boşluk'un yerine başka bir şey atamak test edilmedi.
- **"Henüz doğrulanmadı" işaretleri,** Wraith web sitesinin gönderdiği ama
  OpenWraith'in bir W75'te henüz denemediği değerleri gösterir. Bu bilgi
  uygulamadaki bir listeden değil, kütüphaneden gelir: örneğin SOCD
  modları 2 ve 3, DKS'nin son aşamasında dokunma, 300 ms dışındaki Mod-Tap
  süreleri, Fn katmanında gelişmiş tuşlar, sağdaki değiştirici tuşlarla
  kısayollar.
- **SOCD:** açma/kapama anahtarı klavyenin tamamına ait tek bir ayardır
  (her profile uygulanır); tuş çiftleri profile aittir. Fn + Sağ Shift onu
  klavyeden açıp kapatır; uygulama bunu fark eder, görünümü günceller ve
  bekleyen bir SOCD değişikliğini yeniden incelemenizi ister.
- **Ölü bölge** profil başınadır; 1,00 mm'nin üstündeki değerler
  kırpılmaz, reddedilir. Kaydettiğiniz hazır ayarlar klavyenin sınırlarına
  göre denetlenir, sınır dışındaysa reddedilir.
- **"Klavyenin sakladığını denetle"** (başka bir profile geçip geri döner)
  isteğe bağlıdır ve işaretli gelmez: bu düzenleyicilerin yazdığı her
  tablonun klavyede saklandığı görüldü.
- **Canlı tuş derinliği** klavyeyi meşgul tutar: açıkken başka hiçbir şey
  yazılmaz ve üst çubukta **Canlı** yazar. Durdur'a basınca, sekmeden
  ayrılınca, pencere gizlenince ya da kapanınca, 5 dakika hiç hareket
  olmayınca ve kütüphanenin durdurduğu öteki durumlarda kapanır. Bir
  görünüm klavyede hâlâ açık olabilirse (OpenWraith kapandıysa ya da kablo
  çıktıysa) bir uyarı Tuş derinliği sekmesine götürür; orada "Durdur" onu
  kapatır. Bu arada klavyeyle yazı yazmaya devam edebilirsiniz; ancak test
  durdurulana kadar OpenWraith ayar yazmayı reddedebilir.
- **Profil paylaşma:** dışa aktarılan dosya klavyenin USB seri numarasını
  hiçbir zaman içermez, ama makrolar bastıkları tuşlarla birlikte gider;
  dosyayı verdiğiniz herkes onları okuyabilir. İçe aktarma, seçtiğiniz
  bölümlerin (varsayılan olarak aktivasyon ve ışık) seçtiğiniz profile
  olağan plan, güvenlik yedeği ve Geri al ile geri yüklenmesidir;
  Fn + 1-4 ya da Fn + Boşluk'un yerine başka bir şey atayan bir dosya ayrı
  bir onay ister.

Uygulamanın dosyaları: güvenlik yedekleri yukarıdaki klasörde; ayarlar,
tetikleme hazır ayarlarınız ve kapattığınız ipuçları
`%APPDATA%\io.github.zeruh31.openwraith\settings.json` dosyasında; makro
kitaplığı `%LOCALAPPDATA%\io.github.zeruh31.openwraith\macro-library.json`
dosyasında; işlem günlüğü (yarıda kalan bir yazma için) ve tuş derinliği
işareti (`travel-test-open`: hâlâ açık olabilecek bir görünüm)
`%LOCALAPPDATA%\io.github.zeruh31.openwraith\state\` klasöründe.

### Güvenlik

Bu kurallar kütüphanenindir; komut satırı aracı ve masaüstü uygulaması
ikisi de bunlara uyar.

- **Salt okunur komutlar salt okunur kalır.** `list`, `info`, `show`,
  `monitor`, `profile` (`select` olmadan), `restore --dry-run`, `profile
  import --dry-run`, her `set ... --dry-run`, `backup` (`--all-profiles`
  olmadan) ve etkin profilin `profile export`'u, klavyeyi yazma
  gönderemeyen bir aktarım tipiyle açar; bunu derleyici denetler.
- **Yazan komutlar:** `profile select`, `set`, `restore`, `profile import`,
  `backup --all-profiles` ve etkin olmayan bir profilin `profile export`'u
  (yalnızca profil geçişi) ve `travel` (yalnızca tuş yolu testinin açılması
  ve kapanması). Bu komutlar yalnızca sitenin gönderip klavyenin kabul
  ettiği, kayıtlarda görülen yazma komutlarını, kayıttaki komut biçimiyle
  gönderebilir (etkisi ne kayıtlarda ne de OpenWraith'in donanım
  denemelerinde görülmüş tablo içerikleri planda "not yet verified on a W75"
  diye işaretlenir); her paket gönderilmeden hemen önce
  bir izin listesinden de geçer. Sıfırlama, kalibrasyon, switch tipi, yan ışık, 2 numaralı renk
  tablosu, polling rate, gamepad ve Win-Lock yazmaları kodda oluşturulamaz
  bile.
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
  tetikleme ayarları (her pakette tek tuş), dead band, SOCD, tuş atamaları,
  makrolar, gelişmiş tuşlar, 1 numaralı özel renk tablosu ve ana ışık.
  Doğrulanmış yazma komutu olmayan tablolar (yan ışık, 2 numaralı renk
  tablosu, switch tipleri, polling rate, Win-Lock / 6 tuş) yalnızca
  karşılaştırılır ve "not written" (yazılmadı) olarak listelenir. Geri yükleme
  yedekte olmayan varsayılanları asla eklemez (site tuş tablosuna kendi düğme
  varsayılanlarını yazar; OpenWraith yazmaz). Tuş atamaları, makrolar ve
  gelişmiş tuşlar bir bütün olarak geri yüklenemiyorsa (örneğin bir gelişmiş
  tuş sitenin biçiminde bir DKS, Mod-Tap ya da Toggle değilse) bu grup
  tümüyle atlanır ve plan nedenini söyler. Grup, sitenin sırasıyla yazılır
  (makrolar, ana tuş tablosu, Fn tuş tablosu, gelişmiş tuşlar). Klavye
  değişen ana tuş tablosunu yalnızca ardından Fn tuş tablosu yazılırsa tutar
  (gerçek klavyede bulundu, docs/PROTOCOL.md 4.8.2); bu yüzden Fn tuş
  tablosu hemen ardından, gerekirse değişmeden yeniden yazılır. Plan bunu
  "Fn keymap (18 02)" olarak, nedenini Change sütununda gösterir
  ("unchanged, re-sent: ..."); rapor "Fn keymap (18 02) re-sent unchanged to
  make the keyboard save the base keymap" olarak listeler ve paketlerinin her
  birinin onaylanmasını şart koşar. Klavye bir gelişmiş tuşu, aynı
  kayıtta önce Fn tuş tablosu yazılırsa tutar, o olmadan yazılanı ise
  kaybeder (gerçek klavyede bulundu, H20); bu yüzden tuş tabloları değişmeden gelişmiş tuş yuvası yazan bir geri yükleme
  ya da düzenleme, site gibi, değişmemiş Fn tuş tablosunu bu yuvalardan önce
  yazar. Makro yuvaları kendi başlarına da saklanır, Fn tuş tablosu
  gerektirmez.
- **0 numaralı makro yuvası özeldir.** 0 numaralı yuvaya yazmak, klavyenin
  o profildeki 1-9 numaralı makro yuvalarını boşaltmasına yol açar (gerçek
  klavyede bulundu, docs/PROTOCOL.md 4.9.2; site her seferinde on yuvanın
  hepsini yazdığı için bu orada görünmez). Bu yüzden OpenWraith 0 numaralı
  yuvayı her yazdığında, kalması gereken diğer makroları hemen ardından
  yeniden yazar ("re-sent: writing macro slot 0 makes the keyboard clear
  slots 1-9"), Ctrl+C'den sonra da. Yeni bir makro 1-9 arasındaki ilk boş
  yuvaya yazılır; 0 numaralı yuva yalnızca bunların hepsi doluysa kullanılır.
  Yeniden yazma başarısız olursa rapor, boş kalan makroları ve artık hiçbir
  şey yapmayan tuşları listeler.
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
  (yeniden çalıştırın ya da geri alın). Donanım denemeleri, klavyenin `set`
  ile yazılan her tabloyu profil değişikliğinden ve kablonun çıkarılıp
  takılmasından sonra da tuttuğunu gösterdi; bu yüzden `set` yalnızca
  bekler, küçük bir değişiklik için ışıklar değişmesin diye profil
  değiştirmez. `--check-persistence always` yine de denetler
  (`auto|always|never`).
- OpenWraith çalışırken klavyeden profil değiştirmeyin; her tablodan önce
  aktif profili denetler ve değiştiyse durur. Bir komut klavyenin kaydetmesini
  beklerken de profil değiştirmeyin ve kabloyu çıkarmayın: bekleme, aktif
  profili yeniden denetleyerek biter; kayıttan önce bir profil değişikliği
  (ya da kopan bağlantı) "MAY BE LOST" (kaybolmuş olabilir) olarak ve çıkış
  kodu 1 ile bildirilir, asla tamamlandı denmez. Değerleri denetleyin, sonra
  komutu yeniden çalıştırın ya da geri alın. Bu bekleme sırasında Ctrl+C,
  geri yüklemeyi bir sonraki profil seçilmeden önce durdurur.
- **SOCD'nin açık/kapalı ayarı tüm profiller için tektir.** Klavyede dört
  profilin hepsi için tek bir SOCD açma/kapama ayarı vardır (Fn + Sağ Shift
  de bunu değiştirir); SOCD tuş çiftleri ve dead band ise her profilin
  kendinedir (gerçek klavyede bulundu, docs/PROTOCOL.md 4.15). `set socd
  --on` / `--off` bunun her profili değiştirdiğini söyler. Geri yükleme bu
  ayarı bir kez yazar (yedek alındığında etkin olan profilin ayarını) ve
  planda bir kez gösterir ("SOCD power (all profiles): on -> off"); bu ayarı
  bittiği profilden başka bir profilde yazdıysa, sonunda orada bir kez daha
  okur ("final check"). Bir profili içe aktarmak yalnızca onun SOCD tuş
  çiftlerini alır, klavyedeki açık/kapalı ayarını olduğu gibi bırakır.
- **Tuş atamaları korunur.** Döner düğmenin girdileri ve Fn tuşu asla
  değiştirilmez; klavyenin yerleşik bir Fn işlevini (`openwraith show
  keymap` listeler) gizleyecek bir Fn katmanı girdisi `--allow-builtin TUŞ`,
  profil geçişini (Fn + 1..4) ya da sıfırlamayı (Fn + Boşluk) gizleyecek
  olanı `--allow-critical-builtin TUŞ` ister. Yalnızca sitenin yazdığı girdi
  türleri ayarlanabilir; klavyedeki başka her girdi okunduğu gibi korunur.
  Bir tuş asla boş bir makro ya da gelişmiş tuş yuvasını göstermez ve bir
  yuva tek bir tuşa aittir.
- **Canlı tuş yolu görünümü her zaman kapatılır.** `openwraith travel`
  klavyenin tuş yolu testini sonunda, Ctrl+C'de (yalnızca durmayı ister;
  üçüncü Ctrl+C hemen çıkar), profil değişikliğinde, başka bir programda ve
  hatalarda kapatır. Klavye açık kalmış bir testten (bir çökmeden sonra)
  tuş derinliği göndermeyi sürdürürse yazan komutlar `openwraith travel
  --close` çalıştırılana kadar reddeder.
- **Ctrl+C** ile durdurabilirsiniz: OpenWraith o anki tabloyu bitirir, eski
  profile geri geçer ve neyin yazıldığını bildirir. Tuş grubunun (makrolar,
  tuş tabloları, gelişmiş tuşlar) içindeyse grubun diğer tablolarını atlar,
  ama grubun gerektirdiklerini yine de yazar: ana tuş tablosu yazıldıysa Fn
  tuş tablosunu (o olmadan klavye ana tuş tablosunu tutmaz; tek başına
  makro yuvaları onu gerektirmez), yazılmış ya da hâlâ yazılacak bir tuşun
  başvurduğu makro ya da gelişmiş tuş yuvasını (veya bir tuşu yeniden
  yazılmış bir yuvadan ayıran tuş tablosunu) ve 0 numaralı makro yuvası
  yazıldıysa onun boşaltacağı diğer makroları (en çok dokuz yuva daha),
  sonra durur. Bunlar planın son yazmalarıysa planlanan her şey yazılmıştır:
  rapor "Ctrl+C received" der ve çıkış kodu 0 kalır. İkinci Ctrl+C hemen
  durdurur; 0 numaralı yuvadan sonra henüz yeniden yazılmamış makrolar o
  zaman geri alana kadar boş kalır. Geri yüklemenin son denetimi yarıda
  kesilmez.
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
openwraith set trigger --all --preset fast          # hazır ayarlar: typing, balanced, fast, competitive, factory
openwraith set light --mode static --color 0000ff
openwraith show keymap                              # iki katman ve yerleşik Fn işlevleri
openwraith show macros                              # makrolar adım olarak, bağlı tuşlarıyla
openwraith show advanced                            # DKS / Mod-Tap / Toggle yuvaları
openwraith show socd
openwraith monitor --duration 30s                   # klavyenin kendiliğinden gönderdikleri (hiçbir şey göndermez)
openwraith set key Caps=L-Ctrl L-Ctrl=Caps          # iki tuşu değiştir (ana katman)
openwraith set key --layer fn Q=combo:L-Ctrl+C      # Fn + Q kopyalar
openwraith set key --layer fn F1=media:mute --allow-builtin F1
openwraith set macro Caps --steps "A+ 100, A- 100, B+ 100, B-"
openwraith set macro Caps --unbind
openwraith set advanced E dks --point 1.5 --row E:t... --row R:.t..
openwraith set advanced G mod-tap --hold L-Ctrl --time 300ms
openwraith set advanced H toggle
openwraith set colors W=ff0000 A=ff0000 --custom-mode
openwraith set deadband W A S D --top 0.10 --bottom 0.20
openwraith set socd --on --pair A/D
openwraith set key Caps=L-Ctrl --dry-run --packets  # yalnızca plan ve paketler, yazmaz
openwraith travel --duration 60s                    # canlı tuş derinliği; Ctrl+C durdurur, test hep kapatılır
openwraith travel --close                           # çökmeden açık kalan testi kapatır
openwraith profile export 1 -o fps.owprofile.json   # USB seri numarası dosyaya girmez
openwraith profile import fps.owprofile.json --into 3 --include triggers,lighting,keys
```

Tuşlar `openwraith show triggers` çıktısındaki adlarıyla (büyük/küçük harf
fark etmez: `W`, `"Caps Lock"`, `L-Shift`; `caps`, `equal`, `slash` gibi
kısa adlar da olur), indeksle (`#46`) ya da konumla (`r2c2`) verilir.
Mesafeler milimetredir ve 0,02 mm'nin katı olmalıdır (DKS basma noktası
0,1 mm'nin katı). Komut satırına ondalık sayılar noktayla yazılır (`1.20`).

`set key` eylemleri: `default`, bir tuş adı (`A`, `L-Ctrl`, `F13`,
`"KP 1"`), `usage:0xNN`, `media:AD` (`mute`, `volume-up`, `volume-down`,
`play-pause`, `next`, `previous`, `stop`, `calculator`, `browser-back`,
`browser-forward`, ...) ya da `media:0xNN`,
`mouse:left|right|middle|back|forward|wheel-up|wheel-down` (`back` farenin
geri düğmesi X1, `0x08`; `forward` ileri düğmesi X2, `0x10`; site bu
ikisini ters adlandırır),
`combo:DEĞİŞTİRİCİ[+DEĞİŞTİRİCİ]+TUŞ`, `macro:N` ve `advanced:N` (var olan
bir yuvayı bağlar). Birden çok `TUŞ=EYLEM` birlikte yazılır. Makro
adımları: `TUŞ+` basar, `TUŞ-` bırakır, ardından adımdan sonraki bekleme
ms cinsinden gelir (son adımda bekleme olmaz); en çok 40 adım, basılan her
tuş bırakılmalı; `--play once|times:N|toggle|while-held`. DKS satırları
`TUŞ:AŞAMALAR` biçimindedir; dört karakter sırayla basma, dibe vurma,
dipten kalkma ve bırakma içindir: `.` hiçbir şey, `t` dokunma, `h` tuş
sonraki aşamaya varana kadar basılı tut (otomatik tekrarla), `r` bırakıp
yeniden bas. SOCD kipleri (iki tuş
birden basılıyken): 1 son basılan kazanır (varsayılan), 2 birinci tuş
kazanır, 3 ikinci tuş kazanır, 4 sonraki tuş yalnızca ilkini keser. Planda
"not yet verified on a W75" diye işaretlenen değerler, sitenin de
gönderdiği ama OpenWraith'in etkisini henüz denemediği baytlardır;
2026-10-27'deki donanım denemelerinden beri bunlar yalnızca daha seyrek
kullanılanlardır: SOCD kipleri 2 ve 3, DKS'de bırakma aşamasında dokunma,
"yeniden bas", 300 ms dışındaki Mod-Tap süreleri, Fn katmanındaki gelişmiş
tuşlar, sağ taraftaki bir değiştirici tuşla (R-Ctrl, R-Shift, R-Alt, R-GUI)
yapılan kombinasyonlar ve çoğu multimedya tuşu.

`set` seçenekleri: `--dry-run` (yalnızca plan, salt okunur), `--packets`
(her paketin hex'i), `--json`, `--yes`; v0.2 düzenleyicilerinde ayrıca
`--check-persistence auto|always|never` (yazdıktan sonra başka bir profile
geçip geri dönerek klavyenin sakladığını denetler; varsayılan `auto`
yalnızca donanımda kanıtlanmamış tabloları denetler, 2026-10-27'den beri
bunlar yalnızca Fn tuş tablosu olmadan yazılan gelişmiş tuş yuvalarıdır) ve
`set macro` / `set advanced` için `--layer base|fn`, `--slot N` ile
`--commit-trigger skip` (bir donanım testi: önlerinde Fn tuş tablosu
yazılmayan gelişmiş tuş yuvaları, klavye bunları tutmaz; her zaman
denetlenir). `profile import` için `--into N`, `--profile N`
(dosyanın profili), `--include` (varsayılan: triggers,lighting) ve
`--allow-critical-fn-overrides`. `profile import --dry-run` yalnızca etkin
profili okur: başka bir profile içe aktarmanın planını görmek için önce o
profili seçin (`openwraith profile select N`) ya da `--dry-run` olmadan
çalıştırın (plan sorudan önce gösterilir).
Çıkış kodları: her şey yazılıp doğrulandıysa (ya da yapılacak bir şey yoksa)
0, bir hata olduysa, değişiklik onaylanmadıysa (çıktı bir şey yazılıp
yazılmadığını söyler) ya da `profile import` `--include` ile seçilen bir
kategoriyi dışarıda bıraktıysa (örneğin kritik bir Fn kısayolu yüzünden
engellenen tuşlar; plan buna izin veren seçeneği söyler) 1, hatalı kullanımda
2.

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
dosyalarını reddeder. `openwraith profile export` bir profili paylaşmak için
`profile_export` kapsamlı bir şema 1 dosyasına yazar (USB seri numarası
olmadan); `openwraith profile import` onu herhangi bir profile yazar. 8
MiB'den büyük dosyalar reddedilir. Tetikleme ayarları, ana ışık, iki tuş
tablosu, makrolar, gelişmiş tuşlar, 1 numaralı özel renk tablosu, dead band
ve SOCD tuş çiftleri profile özeldir; SOCD'nin açık/kapalı ayarı ise tüm
klavye için tektir (kayıtlarda ve OpenWraith'in donanım denemelerinde
görüldü, PROTOCOL.md 4.5.4). OpenWraith'in yazmadığı tabloların kapsamı
henüz bilinmiyor (Q3); geri yükleme onları yalnızca karşılaştırır.

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
