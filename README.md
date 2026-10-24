# OpenWraith

OpenWraith is an open-source replacement for the browser-based configurator of
the **Wraith W75** Hall-effect (magnetic switch) keyboard. It talks to the
keyboard directly over USB HID. No browser, no account, and no third-party
analytics are involved.

The goal is a desktop app (Tauri 2). This repository currently contains a Rust
library and the `openwraith` command line tool. It shows the keyboard's
settings, makes lossless backups of one or all four onboard profiles,
restores them, and changes triggers and the main light.

> **Disclaimer.** OpenWraith is an independent project. It is **not affiliated
> with, endorsed by, or supported by Wraith** or the keyboard's manufacturer.
> "Wraith" and "W75" are used only to say which hardware this works with. The
> protocol was documented for interoperability from USB captures of the
> maintainer's own keyboard and from an analysis of the public web
> configurator; no vendor code is included (see
> [docs/PROTOCOL.md](docs/PROTOCOL.md)). Use at your own risk.

## Status: Phase 2 (backup, restore, edits)

| Works now | Planned |
|---|---|
| `openwraith list` / `info` / `show`: devices, model, firmware, triggers, lighting, keymap, macros (read-only) | Desktop app (Tauri 2) |
| `openwraith backup`: lossless backup of the active profile (read-only) | Keymap and macro editing |
| `openwraith backup --all-profiles`: all four onboard profiles (switches profiles, nothing else) | SOCD, switch types, side light, polling rate, Win-Lock (no confirmed write command yet) |
| `openwraith restore`: restore a backup, with a plan, a confirmation and read-back verification | Other models of the family (W60, W65, W100) |
| `openwraith set trigger` / `set light`: actuation, Rapid Trigger, main light | |
| `openwraith profile [select N]`: show or switch the active profile | |

Supported hardware: Wraith W75 (product code `W75HEARGB`) connected with the
USB cable, firmware `V3.22.07` (USB `2e3c:c368`). The read-only commands were
checked on the maintainer's keyboard. **The commands that write (Phase 2)
have so far only run against a device simulator; the hardware tests
([docs/HARDWARE_TESTS.md](docs/HARDWARE_TESTS.md)) are still to be done.** The product IDs `2e3c:c365` to `2e3c:c367` are accepted as the
spec allows, but unverified.

## Safety

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
  write format), that whole group is skipped and the plan says why.
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
  back to the original profile and reports what was written. A second Ctrl+C
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

### Linux

Install `pkg-config` and the udev development files (Debian/Ubuntu:
`sudo apt install pkg-config libudev-dev`), then `cargo build --release`.
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

The USB captures in `captures/` can be replayed instead of a keyboard:

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
| `docs/PROTOCOL.md` | The protocol specification, with a confidence label on every fact. |
| `docs/ARCHITECTURE.md` | Crate layout, how read-only paths and writes are guarded, data flow, backup format, adding a model. |
| `docs/HARDWARE_TESTS.md` | The hardware test checklist (H0-H11) for the commands that write, run by hand. |
| `captures/` | Real USB traffic of the maintainer's W75, used by the tests. |

```text
cargo test --workspace                              # never touches a real keyboard
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Every test runs against the captures (through a mock transport) or against a
simulated keyboard; no test opens a HID device. Contributions must never copy
code or data files from the vendor's site: work from
[docs/PROTOCOL.md](docs/PROTOCOL.md) and from your own captures.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

---

## Türkçe

OpenWraith, **Wraith W75** manyetik (Hall-effect) klavyenin tarayıcı tabanlı
yapılandırma sitesinin yerine geçmeyi amaçlayan açık kaynaklı bir projedir.
Klavyeyle doğrudan USB HID üzerinden konuşur; tarayıcı, hesap veya üçüncü
taraf analitik betiği gerekmez. Hedef bir masaüstü uygulamasıdır (Tauri 2). Bu
depoda şu an bir Rust kütüphanesi ve `openwraith` komut satırı aracı var. Araç
klavyenin ayarlarını gösterir, tek profilin ya da dört onboard profilin
hepsinin kayıpsız yedeğini alır, yedeği geri yükler, tetikleme ayarlarını ve
ana ışığı değiştirir.

> **Uyarı.** OpenWraith bağımsız bir projedir; Wraith veya üretici ile **hiçbir
> bağlantısı yoktur, onlar tarafından onaylanmamış veya desteklenmemektedir.**
> Protokol, birlikte çalışabilirlik amacıyla bakımcının kendi klavyesinin USB
> kayıtlarından ve herkese açık yapılandırma sitesinin incelenmesinden
> belgelenmiştir; sitenin kodu projede yer almaz. Kullanım sorumluluğu size
> aittir.

**Durum: Faz 2 (yedek, geri yükleme, ayar değiştirme).** Salt okunur
komutlar: `list` (bağlı klavyeler, hiçbiri açılmaz), `info`, `profile`,
`backup` (aktif profil), `show` (tablo veya `--json`) ve
`restore ... --dry-run`. Klavyeye yazan komutlar: `backup --all-profiles`
(yalnızca profil geçişi yapar), `profile select N`, `set trigger`, `set light`
ve `restore`. Salt okunur komutlar bakımcının klavyesinde (firmware
V3.22.07, USB `2e3c:c368`) denendi. **Yazan komutlar şimdiye kadar yalnızca
klavye simülatörüne karşı test edildi; gerçek donanım testleri
([docs/HARDWARE_TESTS.md](docs/HARDWARE_TESTS.md)) henüz yapılmadı.** Tuş ataması ve makro düzenleme, SOCD, switch tipi, yan ışık,
polling rate ve Win-Lock yazmaları ile masaüstü uygulaması sonraki aşamalarda
gelecek.

**Güvenlik:**

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
  tümüyle atlanır ve plan nedenini söyler.
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
  profile geri geçer ve neyin yazıldığını bildirir. İkinci Ctrl+C hemen
  durdurur. Geri yüklemenin son denetimi yarıda kesilmez.
- Metin çıktısında `*` ile işaretlenen değerlerin anlamı üreticinin sitesinin
  kodundan alınmıştır ve henüz USB trafiğinde doğrulanmamıştır; JSON
  çıktısındaki `note` alanı da bunu belirtir. Esas olan ham baytlardır.

**Derleme (Windows):** [rustup](https://rustup.rs/) ile Rust'ı kurun (MSVC
araç zinciri ve Visual Studio Build Tools içindeki "Desktop development with
C++" bileşeni gerekir), sonra depo klasöründe `cargo build --release`
çalıştırın. Program `target\release\openwraith.exe` olur; ayrıca sürücü
gerekmez. Linux için yukarıdaki udev kuralına bakın.

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

Protokol belgesi: [docs/PROTOCOL.md](docs/PROTOCOL.md). Mimari:
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Lisans: GPL-3.0-or-later.
