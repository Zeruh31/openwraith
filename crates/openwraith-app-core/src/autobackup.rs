//! The known-good store on disk: the automatic backups.
//!
//! ```text
//! <app local data>/known-good/<keyboard>/profile-<N>/openwraith-<product>-<YYYYMMDD-HHMMSS>-auto-<reason>[-<n>].owbackup.json
//!                                       /profile-<N>/pinned.json      (the names the user keeps)
//!                                       /recovery/openwraith-<product>-<YYYYMMDD-HHMMSS>-recovery[-<n>].owbackup.json
//! ```
//!
//! `<keyboard>` is the SHA-256 of the product code and the USB serial number,
//! so the folder name does not show the serial number, and keyboards that
//! report a serial number never share a known-good backup. Without both it
//! is `unidentified`, and **the Wraith W75 reports no USB serial number**:
//! every W75 on this computer shares the `unidentified` folder (adding the
//! product or the USB ids would not tell two W75s apart either). The app
//! says so where the automatic backups are shown, compares within one run
//! only reads of the same connected keyboard (a keyboard on another
//! connection starts its comparisons afresh) and never restores without the
//! user's review. Every file
//! of a `profile-<N>` folder is a complete single-profile read of profile N
//! (schema 1) made by this app; nothing else is ever written there. Saving
//! is atomic (the library's `Backup::save`); the newest `keep` files of a
//! profile are kept, pinned files are never deleted and do not count, and
//! files whose names this module did not make are never touched. A
//! `pinned.json` that cannot be read deletes nothing (fail closed: the
//! rotation is skipped until it can be read again). Safety backups live in
//! another folder and are never touched either.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use openwraith_hid::{BACKUP_EXTENSION, Backup, BackupError};
use serde::{Deserialize, Serialize};

use crate::dto::{AppError, AppErrorKind, ErrorDetail};

/// The store's folder under the app's local data.
pub(crate) const STORE_DIR: &str = "known-good";

/// The folder of a keyboard without a product code or USB serial number.
pub(crate) const UNIDENTIFIED: &str = "unidentified";

/// The pins of a profile folder.
const PINS_FILE: &str = "pinned.json";

/// The folder of the composed backups of "Restore all profiles".
const RECOVERY_DIR: &str = "recovery";

/// How many composed backups are kept.
const RECOVERY_KEEP: usize = 5;

/// The default number of automatic backups kept per profile.
pub(crate) const DEFAULT_KEEP: u8 = 10;

/// The most automatic backups the user may keep per profile.
pub(crate) const MAX_KEEP: u8 = 100;

/// Why an automatic backup was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum AutoReason {
    /// The first complete read of the profile (nothing to compare with).
    FirstRead,
    /// The read after a write of OpenWraith that completed.
    AfterWrite,
    /// A read that differs from the last automatic backup (changed by the
    /// keyboard, another program or the user elsewhere), not reset-like.
    ChangedElsewhere,
    /// "It's fine, keep them": the user accepted the settings read after a
    /// suspected reset.
    KeptCurrent,
    /// "Back up now" in the tray.
    Manual,
}

impl AutoReason {
    /// Every reason.
    pub(crate) const ALL: [AutoReason; 5] = [
        AutoReason::FirstRead,
        AutoReason::AfterWrite,
        AutoReason::ChangedElsewhere,
        AutoReason::KeptCurrent,
        AutoReason::Manual,
    ];

    /// The reason in a file name.
    pub(crate) fn slug(self) -> &'static str {
        match self {
            AutoReason::FirstRead => "first-read",
            AutoReason::AfterWrite => "after-write",
            AutoReason::ChangedElsewhere => "changed-elsewhere",
            AutoReason::KeptCurrent => "kept-current",
            AutoReason::Manual => "manual",
        }
    }

    fn from_slug(slug: &str) -> Option<AutoReason> {
        AutoReason::ALL.into_iter().find(|r| r.slug() == slug)
    }
}

/// The folder name of a keyboard: SHA-256 of `product`, a NUL byte and
/// `serial`, as hex; [`UNIDENTIFIED`] without both.
pub(crate) fn device_key(product: Option<&str>, serial: Option<&str>) -> String {
    match (product, serial.map(str::trim)) {
        (Some(product), Some(serial)) if !product.is_empty() && !serial.is_empty() => {
            let mut bytes = product.as_bytes().to_vec();
            bytes.push(0);
            bytes.extend_from_slice(serial.as_bytes());
            crate::sha256::sha256_hex(&bytes)
        }
        _ => UNIDENTIFIED.to_string(),
    }
}

/// The folder name of the keyboard a backup was read from.
pub(crate) fn device_key_of(backup: &Backup) -> String {
    device_key(
        backup.device.product.as_deref(),
        backup.device.usb.as_ref().and_then(|u| u.serial.as_deref()),
    )
}

/// The folder of a keyboard.
pub(crate) fn device_dir(root: &Path, key: &str) -> PathBuf {
    root.join(key)
}

/// The folder of a profile of a keyboard.
pub(crate) fn profile_dir(root: &Path, key: &str, profile: u8) -> PathBuf {
    device_dir(root, key).join(format!("profile-{profile}"))
}

/// The parts of a file name of the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoreName {
    /// The product code in the name.
    pub(crate) product: Option<String>,
    /// `YYYYMMDD-HHMMSS` (UTC).
    pub(crate) stamp: String,
    /// What follows the stamp, without the `-<n>` suffix (`auto-first-read`,
    /// `recovery`).
    pub(crate) tag: String,
    /// The number of a later file with the same stamp (0 for the first).
    pub(crate) n: u32,
}

impl StoreName {
    /// The reason of an automatic backup.
    pub(crate) fn reason(&self) -> Option<AutoReason> {
        AutoReason::from_slug(self.tag.strip_prefix("auto-")?)
    }

    /// The time in the name as RFC 3339 (`2026-10-24T07:30:00Z`).
    pub(crate) fn time(&self) -> String {
        let s = &self.stamp;
        format!(
            "{}-{}-{}T{}:{}:{}Z",
            &s[0..4],
            &s[4..6],
            &s[6..8],
            &s[9..11],
            &s[11..13],
            &s[13..15]
        )
    }

    /// The order of files: newer stamp first, then the higher number.
    fn newer_first(a: &StoreName, b: &StoreName) -> std::cmp::Ordering {
        b.stamp.cmp(&a.stamp).then(b.n.cmp(&a.n))
    }
}

/// Whether `bytes[at..at + 15]` is `YYYYMMDD-HHMMSS`.
fn is_stamp(bytes: &[u8], at: usize) -> bool {
    bytes.get(at..at + 15).is_some_and(|s| {
        s.iter().enumerate().all(|(i, b)| {
            if i == 8 {
                *b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
    })
}

/// Parses a file name of the store.
pub(crate) fn parse_name(name: &str) -> Option<StoreName> {
    let stem = name
        .strip_suffix(&format!(".{BACKUP_EXTENSION}"))?
        .strip_prefix("openwraith-")?;
    let bytes = stem.as_bytes();
    let at = (1..stem.len()).find(|&i| bytes.get(i - 1) == Some(&b'-') && is_stamp(bytes, i))?;
    let product = stem
        .get(..at - 1)
        .filter(|p| !p.is_empty())
        .map(str::to_string);
    let stamp = stem.get(at..at + 15)?.to_string();
    let rest = stem.get(at + 15..)?.strip_prefix('-')?;
    let (tag, n) = match rest.rsplit_once('-') {
        Some((head, n))
            if !head.is_empty() && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) =>
        {
            (head, n.parse().ok()?)
        }
        _ => (rest, 0),
    };
    if tag.is_empty() {
        return None;
    }
    Some(StoreName {
        product,
        stamp,
        tag: tag.to_string(),
        n,
    })
}

/// `YYYYMMDD-HHMMSS` of an RFC 3339 UTC time (`created_at` of a backup).
pub(crate) fn stamp_of(created_at: &str) -> String {
    let digits: String = created_at
        .chars()
        .filter(char::is_ascii_digit)
        .take(14)
        .collect();
    if digits.len() == 14 {
        format!("{}-{}", &digits[..8], &digits[8..])
    } else {
        "00000000-000000".to_string()
    }
}

/// The product code of a backup for a file name.
fn product_of(backup: &Backup) -> String {
    let product: String = backup
        .device
        .product
        .as_deref()
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if product.is_empty() {
        "keyboard".to_string()
    } else {
        product
    }
}

/// A file name of the store.
fn file_name(product: &str, stamp: &str, tag: &str, n: u32) -> String {
    let suffix = if n == 0 {
        String::new()
    } else {
        format!("-{n}")
    };
    format!("openwraith-{product}-{stamp}-{tag}{suffix}.{BACKUP_EXTENSION}")
}

/// One file of the store.
#[derive(Debug, Clone)]
pub(crate) struct StoreFile {
    /// Its path.
    pub(crate) path: PathBuf,
    /// Its file name.
    pub(crate) file_name: String,
    /// Its parsed name.
    pub(crate) name: StoreName,
    /// The user keeps it (never deleted).
    pub(crate) pinned: bool,
}

fn io_error(path: &Path, action: &str, e: &dyn std::fmt::Display) -> AppError {
    AppError::with(
        AppErrorKind::Io,
        ErrorDetail::Io {
            file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
        },
        format!("cannot {action} {}: {e}", path.display()),
    )
}

/// The files of `dir` whose names have a tag accepted by `tag`, newest
/// first. A missing folder lists none.
fn list_tagged(dir: &Path, tag: impl Fn(&str) -> bool) -> Result<Vec<StoreFile>, AppError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_error(dir, "list", &e)),
    };
    // Unreadable pins show as unpinned here; the rotation reads them
    // itself and deletes nothing then.
    let pins = read_pins(dir).unwrap_or_default();
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some(name) = parse_name(&file_name) else {
            continue;
        };
        if !tag(&name.tag) || !entry.path().is_file() {
            continue;
        }
        found.push(StoreFile {
            path: entry.path(),
            pinned: pins.contains(&file_name),
            file_name,
            name,
        });
    }
    found.sort_by(|a, b| {
        StoreName::newer_first(&a.name, &b.name).then(b.file_name.cmp(&a.file_name))
    });
    Ok(found)
}

/// The automatic backups of a profile folder, newest first.
pub(crate) fn list_profile(dir: &Path) -> Result<Vec<StoreFile>, AppError> {
    list_tagged(dir, |tag| {
        tag.strip_prefix("auto-")
            .is_some_and(|slug| AutoReason::from_slug(slug).is_some())
    })
}

/// The keyboards in the store (folder names), sorted.
pub(crate) fn list_devices(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut keys: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|k| {
            k == UNIDENTIFIED || (k.len() == 64 && k.chars().all(|c| c.is_ascii_hexdigit()))
        })
        .collect();
    keys.sort();
    keys
}

/// The newest automatic backup of a profile folder that loads, with the
/// backup (a damaged file is skipped; at most five are tried).
pub(crate) fn baseline(dir: &Path) -> Option<(StoreFile, Backup)> {
    let files = list_profile(dir).ok()?;
    files.into_iter().take(5).find_map(|file| {
        Backup::load(&file.path)
            .ok()
            .filter(|b| !b.is_all_profiles())
            .map(|b| (file, b))
    })
}

/// The next free number for `stamp` in `files`.
fn next_number(files: &[StoreFile], stamp: &str) -> u32 {
    files
        .iter()
        .filter(|f| f.name.stamp == stamp)
        .map(|f| f.name.n.saturating_add(1))
        .max()
        .unwrap_or(0)
}

/// Saves `backup` in `dir` with `tag` (never replacing a file): the
/// number after the stamp keeps the files with the same stamp in order.
fn save_tagged(
    dir: &Path,
    backup: &Backup,
    tag: &str,
    existing: &[StoreFile],
) -> Result<PathBuf, AppError> {
    std::fs::create_dir_all(dir).map_err(|e| io_error(dir, "create", &e))?;
    let stamp = stamp_of(&backup.created_at);
    let product = product_of(backup);
    let mut n = next_number(existing, &stamp);
    for _ in 0..1000 {
        let path = dir.join(file_name(&product, &stamp, tag, n));
        match backup.save(&path, false) {
            Ok(()) => return Ok(path),
            Err(BackupError::AlreadyExists { .. }) => n = n.saturating_add(1),
            Err(e) => return Err(AppError::from(&e)),
        }
    }
    Err(io_error(dir, "find a free name in", &"too many files"))
}

/// Saves an automatic backup of profile `dir` and deletes the oldest ones
/// beyond `keep` (pinned files aside). Returns the new file.
///
/// The file is saved even when the rotation fails (a `pinned.json` that
/// cannot be read deletes nothing): the second value is that error.
pub(crate) fn save(
    dir: &Path,
    backup: &Backup,
    reason: AutoReason,
    keep: u8,
) -> Result<(PathBuf, Option<AppError>), AppError> {
    let existing = list_profile(dir)?;
    let path = save_tagged(dir, backup, &format!("auto-{}", reason.slug()), &existing)?;
    let rotation = rotate(dir, usize::from(keep.max(1))).err();
    Ok((path, rotation))
}

/// Deletes the automatic backups of profile `dir` beyond the newest `keep`
/// that are not pinned. Only files this module named are considered. Pins
/// that cannot be read delete nothing (the error).
pub(crate) fn rotate(dir: &Path, keep: usize) -> Result<Vec<PathBuf>, AppError> {
    read_pins(dir)?;
    let mut removed = Vec::new();
    let mut kept = 0usize;
    for file in list_profile(dir)? {
        if file.pinned {
            continue;
        }
        kept += 1;
        if kept > keep {
            std::fs::remove_file(&file.path).map_err(|e| io_error(&file.path, "delete", &e))?;
            removed.push(file.path);
        }
    }
    Ok(removed)
}

/// The pins of a profile folder.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Pins {
    #[serde(default)]
    pinned: Vec<String>,
}

/// The file names pinned in `dir`: none when the file is missing; an error
/// when it exists but cannot be read (never "no pins": the caller fails
/// closed). A leading byte order mark is accepted.
fn read_pins(dir: &Path) -> Result<BTreeSet<String>, AppError> {
    let path = dir.join(PINS_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(e) => return Err(io_error(&path, "read", &e)),
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    serde_json::from_str::<Pins>(text)
        .map(|p| p.pinned.into_iter().collect())
        .map_err(|e| io_error(&path, "read", &e))
}

/// Pins or unpins `file_name` in `dir` (written atomically). Pins that
/// cannot be read are left as they are (the error).
pub(crate) fn set_pinned(dir: &Path, file_name: &str, pinned: bool) -> Result<(), AppError> {
    let mut pins = read_pins(dir)?;
    let changed = if pinned {
        pins.insert(file_name.to_string())
    } else {
        pins.remove(file_name)
    };
    if !changed {
        return Ok(());
    }
    // Pins of files that are gone are dropped.
    pins.retain(|name| dir.join(name).is_file());
    let path = dir.join(PINS_FILE);
    let json = serde_json::to_string_pretty(&Pins {
        pinned: pins.into_iter().collect(),
    })
    .map_err(|e| io_error(&path, "write", &e))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".pinned-")
        .suffix(".partial")
        .tempfile_in(dir)
        .map_err(|e| io_error(&path, "write", &e))?;
    temp.write_all(json.as_bytes())
        .map_err(|e| io_error(&path, "write", &e))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| io_error(&path, "write", &e))?;
    temp.persist(&path)
        .map_err(|e| io_error(&path, "write", &e.error))?;
    Ok(())
}

/// Saves a composed backup ("Restore all profiles") of keyboard `key` and
/// keeps the newest five.
pub(crate) fn save_recovery(root: &Path, key: &str, backup: &Backup) -> Result<PathBuf, AppError> {
    let dir = device_dir(root, key).join(RECOVERY_DIR);
    let existing = list_tagged(&dir, |tag| tag == "recovery")?;
    let path = save_tagged(&dir, backup, "recovery", &existing)?;
    for old in list_tagged(&dir, |tag| tag == "recovery")?
        .into_iter()
        .skip(RECOVERY_KEEP)
    {
        let _ = std::fs::remove_file(&old.path);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        let name = file_name("W75HEARGB", "20261024-073000", "auto-first-read", 0);
        assert_eq!(
            name,
            "openwraith-W75HEARGB-20261024-073000-auto-first-read.owbackup.json"
        );
        let parsed = parse_name(&name).unwrap();
        assert_eq!(parsed.product.as_deref(), Some("W75HEARGB"));
        assert_eq!(parsed.stamp, "20261024-073000");
        assert_eq!(parsed.reason(), Some(AutoReason::FirstRead));
        assert_eq!(parsed.n, 0);
        assert_eq!(parsed.time(), "2026-10-24T07:30:00Z");
        for reason in AutoReason::ALL {
            let name = file_name(
                "W75HEARGB",
                "20261024-073000",
                &format!("auto-{}", reason.slug()),
                12,
            );
            let parsed = parse_name(&name).unwrap();
            assert_eq!(parsed.reason(), Some(reason), "{name}");
            assert_eq!(parsed.n, 12);
        }
        let recovery =
            parse_name(&file_name("W75HEARGB", "20261024-073000", "recovery", 2)).unwrap();
        assert_eq!(recovery.tag, "recovery");
        assert_eq!(recovery.reason(), None);
        // Safety backups and other files are not store names.
        assert!(
            parse_name("openwraith-W75HEARGB-20261024-073000-set-trigger.owbackup.json")
                .unwrap()
                .reason()
                .is_none()
        );
        assert!(parse_name("notes.txt").is_none());
        assert!(parse_name("openwraith-W75HEARGB-20261024-073000.owbackup.json").is_none());
    }

    #[test]
    fn stamps_come_from_the_backup_time() {
        assert_eq!(stamp_of("2026-10-24T07:30:00Z"), "20261024-073000");
        assert_eq!(stamp_of("garbage"), "00000000-000000");
    }

    fn read(name: &str, created_at: &str) -> Backup {
        let mut backup = crate::backend::sim::seed_backup();
        backup.created_at = created_at.to_string();
        backup.device.product = Some(name.to_string());
        backup
    }

    #[test]
    fn unreadable_pins_delete_nothing_and_are_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let f1 = save(
            d,
            &read("W75", "2026-10-24T07:30:00Z"),
            AutoReason::FirstRead,
            2,
        )
        .unwrap()
        .0;
        let name1 = f1.file_name().unwrap().to_string_lossy().into_owned();
        set_pinned(d, &name1, true).unwrap();
        save(
            d,
            &read("W75", "2026-10-24T07:31:00Z"),
            AutoReason::Manual,
            2,
        )
        .unwrap();
        save(
            d,
            &read("W75", "2026-10-24T07:32:00Z"),
            AutoReason::Manual,
            2,
        )
        .unwrap();
        assert!(f1.is_file(), "pinned");
        // A hand edit breaks the file: nothing is deleted any more.
        let pins = d.join(PINS_FILE);
        std::fs::write(&pins, "\u{feff}{\"pinned\": [\"openwraith-").unwrap();
        let before = std::fs::read(&pins).unwrap();
        let (f4, rotation) = save(
            d,
            &read("W75", "2026-10-24T07:33:00Z"),
            AutoReason::Manual,
            2,
        )
        .unwrap();
        assert!(f4.is_file(), "the new backup is saved");
        assert!(rotation.is_some(), "the rotation was skipped");
        assert!(f1.is_file(), "the pinned file is still there");
        assert_eq!(list_profile(d).unwrap().len(), 4, "nothing deleted");
        // Pinning refuses instead of rewriting the file without the pins.
        let name4 = f4.file_name().unwrap().to_string_lossy().into_owned();
        assert!(set_pinned(d, &name4, true).is_err());
        assert_eq!(std::fs::read(&pins).unwrap(), before);
        // A byte order mark alone is fine.
        std::fs::write(&pins, format!("\u{feff}{{\"pinned\":[\"{name1}\"]}}")).unwrap();
        save(
            d,
            &read("W75", "2026-10-24T07:34:00Z"),
            AutoReason::Manual,
            2,
        )
        .unwrap();
        assert!(f1.is_file(), "the pin is honoured");
        let files = list_profile(d).unwrap();
        assert_eq!(files.len(), 3, "the newest two and the pinned one");
        assert!(files.iter().any(|f| f.pinned && f.file_name == name1));
    }

    #[test]
    fn device_keys_hide_the_serial_number() {
        let key = device_key(Some("W75HEARGB"), Some("SIM-1"));
        assert_eq!(
            key,
            "a1d7907745d6abad5020bc067dccc6598c11901c813c97ee7dcfbfeb8110ed47"
        );
        assert_ne!(key, device_key(Some("W75HEARGB"), Some("SIM-2")));
        assert_eq!(device_key(Some("W75HEARGB"), None), UNIDENTIFIED);
        assert_eq!(device_key(None, Some("SIM-1")), UNIDENTIFIED);
        assert_eq!(device_key(Some("W75HEARGB"), Some("  ")), UNIDENTIFIED);
    }
}
