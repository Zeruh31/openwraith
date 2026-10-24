# OpenWraith

OpenWraith is an open-source replacement for the browser-based configurator of
the **Wraith W75** Hall-effect (magnetic switch) keyboard. It talks to the
keyboard directly over USB HID. No browser, no account, and no third-party
analytics are involved.

The goal is a desktop app (Tauri 2). This repository currently contains the
first phase: a Rust library and a **read-only** command line tool that shows
the keyboard's settings and makes lossless backups of them.

> **Disclaimer.** OpenWraith is an independent project. It is **not affiliated
> with, endorsed by, or supported by Wraith** or the keyboard's manufacturer.
> "Wraith" and "W75" are used only to say which hardware this works with. The
> protocol was documented for interoperability from USB captures of the
> maintainer's own keyboard and from an analysis of the public web
> configurator; no vendor code is included (see
> [docs/PROTOCOL.md](docs/PROTOCOL.md)). Use at your own risk.

## Status: Phase 1 (read-only)

| Works now | Planned |
|---|---|
| `openwraith list`: find connected keyboards | Restore a backup (after the write commands are verified on real hardware) |
| `openwraith info`: model, firmware, active profile, capabilities | Editing triggers, lighting, keymap and macros |
| `openwraith backup`: lossless backup to a JSON file | Desktop app (Tauri 2) |
| `openwraith show`: triggers, lighting, keymap, macros as tables or JSON | Other models of the family (W60, W65, W100) |

Supported hardware: Wraith W75 (product code `W75HEARGB`) connected with the
USB cable. The protocol was checked against USB captures of firmware
`V3.22.07` (USB `2e3c:c368`); **OpenWraith itself has not yet been run on
real hardware**, only against replayed captures. The product IDs
`2e3c:c365` to `2e3c:c367` are accepted as the spec allows, but unverified.

## Safety

- **Phase 1 never changes the keyboard.** The code can only build *read*
  requests: the type that a transport sends has no public constructor, and
  every way to create one is a documented read. There is no write, reset or
  calibration code at all. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
- **Keyboard resets are never answered.** Holding Fn + Space for a few seconds
  resets the keyboard, which then sends a reset notification. The vendor site
  answers it with a reset command of its own; OpenWraith stops and tells you
  instead.
- **Close the vendor web configurator first.** Close every browser tab of the
  vendor configurator before running `openwraith info`, `backup` or `show`.
  Some of its write acknowledgements are byte-identical to OpenWraith's
  replies, so they cannot be filtered out
  ([PROTOCOL.md](docs/PROTOCOL.md) 4.7.9). The site also receives
  OpenWraith's replies: it then reloads, rewrites its stored copy of your
  settings, sends a (no-op) trigger write and can file per-key trigger values
  under the wrong keys (PROTOCOL.md 4.5.1, 4.11.2, 6.5). OpenWraith prints a
  warning when it notices another program; reload the site before changing
  anything there.
- **A backup covers the active onboard profile only.** The W75 has four
  onboard profiles; reading another one requires switching profiles, which is
  a write and is not part of Phase 1. Triggers and the main light are per
  profile (seen in the captures). The backup also contains the custom colour
  tables, polling rate and switch types, which are probably global
  (unverified, PROTOCOL.md open question Q3).
- To back up another profile, switch to it (in the vendor configurator, or
  with the keyboard's profile keys, labelled Fn + 1..4; unverified), close the
  configurator and run `openwraith backup` again.
- Do not switch profiles while a backup runs. OpenWraith re-reads the active
  profile at the end and fails if it changed.

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

```text
openwraith list                         # connected keyboards (nothing is opened)
openwraith info                         # model, firmware, profile, capabilities
openwraith backup                       # writes openwraith-W75HEARGB-<UTC time>.owbackup.json
openwraith backup -o my-w75.owbackup.json
openwraith show triggers                # actuation / Rapid Trigger / dead band per key
openwraith show light                   # light effect and per-key colour tables
openwraith show keymap                  # remapped keys on the base and Fn layers
openwraith show macros                  # the ten macro slots
openwraith show all --from my-w75.owbackup.json   # read a backup instead of the keyboard
openwraith show triggers --json         # machine-readable output
```

Global options: `--device <PATH>` picks a keyboard when several are connected
(paths come from `openwraith list`), `--verbose` prints session statistics.
`backup` refuses to overwrite an existing file unless `--force` is given.

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

### Trying it without a keyboard

The USB captures in `captures/` can be replayed instead of a keyboard:

```text
cargo run -p openwraith-hid --features mock --example mock_backup -- captures/capture_06_powercycle_W_persisted.json sample.owbackup.json
cargo run -p openwraith-cli -- show all --from sample.owbackup.json
```

## Development

| Path | What |
|---|---|
| `crates/openwraith-protocol` | Pure protocol library: framing, read requests, response matching, decoders, W75 key table. No I/O. |
| `crates/openwraith-hid` | Transport trait, hidapi transport, read session, device summary, backup, capture-replay mock. |
| `crates/openwraith-cli` | The `openwraith` command. |
| `docs/PROTOCOL.md` | The protocol specification, with a confidence label on every fact. |
| `docs/ARCHITECTURE.md` | Crate layout, read-only guarantee, data flow, adding a model. |
| `captures/` | Real USB traffic of the maintainer's W75, used by the tests. |

```text
cargo test --workspace                              # never touches a real keyboard
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Every test runs against the captures through a mock transport; no test opens a
HID device. Contributions must never copy code or data files from the vendor's
site: work from [docs/PROTOCOL.md](docs/PROTOCOL.md) and from your own
captures.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

---

## Türkçe

OpenWraith, **Wraith W75** manyetik (Hall-effect) klavyenin tarayıcı tabanlı
yapılandırma sitesinin yerine geçmeyi amaçlayan açık kaynaklı bir projedir.
Klavyeyle doğrudan USB HID üzerinden konuşur; tarayıcı, hesap veya üçüncü
taraf analitik betiği gerekmez. Hedef bir masaüstü uygulamasıdır (Tauri 2). Bu
depoda şu an birinci aşama var: bir Rust kütüphanesi ve klavyenin ayarlarını
gösteren, kayıpsız yedek alan **salt okunur** bir komut satırı aracı.

> **Uyarı.** OpenWraith bağımsız bir projedir; Wraith veya üretici ile **hiçbir
> bağlantısı yoktur, onlar tarafından onaylanmamış veya desteklenmemektedir.**
> Protokol, birlikte çalışabilirlik amacıyla bakımcının kendi klavyesinin USB
> kayıtlarından ve herkese açık yapılandırma sitesinin incelenmesinden
> belgelenmiştir; sitenin kodu projede yer almaz. Kullanım sorumluluğu size
> aittir.

**Durum: Faz 1 (salt okunur).** `list` (bağlı klavyeleri listeler, hiçbirini
açmaz), `info` (model, firmware, aktif profil, özellikler), `backup` (JSON
dosyasına kayıpsız yedek) ve `show` (tetikleme, ışık, tuş ataması, makrolar;
tablo veya `--json`) komutları hazır. Yedeği geri yükleme, ayar değiştirme ve
masaüstü uygulaması sonraki aşamalarda gelecek. Protokol, V3.22.07
firmware'li bir klavyenin (USB `2e3c:c368`) USB kayıtlarıyla kontrol edildi;
**OpenWraith henüz gerçek donanımda çalıştırılmadı**, yalnızca kayıtlar
yeniden oynatılarak test edildi. `2e3c:c365`-`2e3c:c367` ürün kimlikleri
spesifikasyona göre kabul edilir ama doğrulanmamıştır.

**Güvenlik:**

- Faz 1 klavyede **hiçbir şeyi değiştirmez**. Kod yalnızca okuma isteği
  üretebilir; yazma, sıfırlama veya kalibrasyon kodu hiç yoktur.
- Fn + Space birkaç saniye basılı tutulursa klavye kendini sıfırlar ve bir
  bildirim gönderir. Site bu bildirime kendi sıfırlama komutuyla cevap verir;
  OpenWraith ise **asla cevap vermez**, durur ve size haber verir.
- **Önce üreticinin yapılandırma sitesini kapatın.** `openwraith info`,
  `backup` veya `show` komutlarını çalıştırmadan önce sitenin açık olduğu tüm
  tarayıcı sekmelerini kapatın. Sitenin bazı yazma onayları OpenWraith'in
  beklediği yanıtlarla bayt bayt aynıdır, bu yüzden ayıklanamaz
  (PROTOCOL.md 4.7.9). Ayrıca site OpenWraith'in yanıtlarını da alır: bunun
  üzerine ayarları yeniden okur, tarayıcıda sakladığı kopyayı yeniden yazar,
  etkisiz bir tetikleme yazması gönderir ve tuş başına tetikleme değerlerini
  yanlış tuşlara kaydedebilir (PROTOCOL.md 4.5.1, 4.11.2, 6.5). OpenWraith
  başka bir programın klavyeyle konuştuğunu fark ederse uyarı verir; o
  durumda sitede bir şey değiştirmeden önce sayfayı yenileyin.
- Yedek yalnızca **aktif onboard profili** kapsar. Tetikleme ayarları ve ana
  ışık profile özeldir (kayıtlarda görüldü). Yedek ayrıca özel renk
  tablolarını, polling rate'i ve switch tiplerini de içerir; bunlar muhtemelen
  tüm profillerde ortaktır (doğrulanmadı, PROTOCOL.md açık soru Q3).
- Başka bir profili yedeklemek için o profile geçin (yapılandırma sitesinden
  ya da klavyede Fn + 1..4 olarak etiketlenmiş profil tuşlarıyla;
  doğrulanmadı), siteyi kapatın ve `openwraith backup` komutunu yeniden
  çalıştırın.
- Yedek alınırken profil değiştirmeyin. OpenWraith sonunda aktif profili
  yeniden okur ve değiştiyse hata verir.
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
openwraith list
openwraith info
openwraith backup -o w75-yedek.owbackup.json
openwraith show triggers
openwraith show all --from w75-yedek.owbackup.json
```

Yedek dosyası, her yanıt raporunu ham hâliyle (63 bayt, hex) ve çözülmüş
görünümüyle birlikte saklar; ham veri esastır. Dosya yüklenirken çözülmüş
görünümler, cihaz bilgisi ve aktif profil ham veriden yeniden üretilir. Protokol belgesi:
[docs/PROTOCOL.md](docs/PROTOCOL.md). Mimari:
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Lisans: GPL-3.0-or-later.
