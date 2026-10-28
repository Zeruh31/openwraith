//! Files the app knows, by token (paths never cross the IPC boundary), and
//! the listing of the safety-backup folder.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use openwraith_hid::{BACKUP_EXTENSION, rfc3339_utc};

use crate::dto::{FileRef, FileToken, SafetyBackupEntry, SafetyReason};
use crate::map::clean;
use crate::tokens::TokenMint;

/// At most this many files are remembered (least recently used first out).
const MAX_FILES: usize = 256;

/// At most this many safety backups are listed.
const MAX_LISTED: usize = 200;

/// Where a known file came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    /// Opened by the user.
    User,
    /// A safety backup.
    Safety,
    /// A destination the user chose for a new backup.
    Destination,
}

/// A known file.
#[derive(Debug, Clone)]
pub(crate) struct KnownFile {
    /// Its path.
    pub(crate) path: PathBuf,
    /// Where it came from.
    pub(crate) kind: FileKind,
}

/// Token -> file.
#[derive(Debug, Default)]
pub(crate) struct FileRegistry {
    entries: VecDeque<(FileToken, KnownFile)>,
}

/// The file reference of `path` with `token`.
pub(crate) fn file_ref(token: FileToken, path: &Path) -> FileRef {
    FileRef {
        token,
        file_name: path
            .file_name()
            .map(|n| clean(&n.to_string_lossy()))
            .unwrap_or_default(),
        display_path: clean(&path.display().to_string()),
    }
}

impl FileRegistry {
    /// Registers `path` (the same path and kind keep their token).
    pub(crate) fn register(
        &mut self,
        mint: &mut TokenMint,
        path: PathBuf,
        kind: FileKind,
    ) -> FileRef {
        if let Some(pos) = self
            .entries
            .iter()
            .position(|(_, f)| f.path == path && f.kind == kind)
        {
            if let Some(entry) = self.entries.remove(pos) {
                let reference = file_ref(entry.0.clone(), &entry.1.path);
                self.entries.push_back(entry);
                return reference;
            }
        }
        let token = mint.file();
        let reference = file_ref(token.clone(), &path);
        self.entries.push_back((token, KnownFile { path, kind }));
        while self.entries.len() > MAX_FILES {
            self.entries.pop_front();
        }
        reference
    }

    /// The file of `token`, if known.
    pub(crate) fn get(&self, token: &FileToken) -> Option<KnownFile> {
        self.entries
            .iter()
            .find(|(t, _)| t == token)
            .map(|(_, f)| f.clone())
    }
}

/// The reason in a safety-backup file name.
pub(crate) fn safety_reason(reason: &str) -> SafetyReason {
    match reason {
        "set-trigger" => SafetyReason::SetTrigger,
        "set-light" => SafetyReason::SetLight,
        "restore" => SafetyReason::Restore,
        "set-keys" => SafetyReason::SetKeys,
        "set-colors" => SafetyReason::SetColors,
        "set-deadband" => SafetyReason::SetDeadband,
        "set-socd" => SafetyReason::SetSocd,
        "import" => SafetyReason::Import,
        _ => SafetyReason::Other,
    }
}

/// The parts of a safety-backup file name
/// `openwraith-<product>-<YYYYMMDD-HHMMSS>-<reason>[-N].owbackup.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SafetyName {
    /// The product code.
    pub(crate) product: Option<String>,
    /// `YYYYMMDD-HHMMSS`.
    pub(crate) stamp: String,
    /// The reason (without a `-N` suffix).
    pub(crate) reason: String,
}

/// Parses a safety-backup file name.
pub(crate) fn parse_safety_name(name: &str) -> Option<SafetyName> {
    let stem = name
        .strip_suffix(&format!(".{BACKUP_EXTENSION}"))?
        .strip_prefix("openwraith-")?;
    let bytes = stem.as_bytes();
    let is_stamp = |at: usize| {
        bytes.get(at..at + 15).is_some_and(|s| {
            s.iter().enumerate().all(|(i, b)| {
                if i == 8 {
                    *b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
        })
    };
    let at = (1..stem.len()).find(|&i| bytes.get(i - 1) == Some(&b'-') && is_stamp(i))?;
    let product = stem.get(..at - 1).filter(|p| !p.is_empty()).map(clean);
    let stamp = stem.get(at..at + 15)?.to_string();
    let rest = stem.get(at + 15..)?.strip_prefix('-').unwrap_or("");
    let reason = match rest.rsplit_once('-') {
        Some((head, n)) if !head.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => head,
        _ => rest,
    };
    Some(SafetyName {
        product,
        stamp,
        reason: reason.to_string(),
    })
}

/// The safety backups in `dir`, newest first, at most 200 (from their names
/// and modification times; nothing is loaded). A missing folder lists none.
pub(crate) fn list_safety(
    dir: &Path,
) -> std::io::Result<Vec<(PathBuf, SafetyName, Option<SystemTime>)>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(parsed) = parse_safety_name(&name) else {
            continue;
        };
        let modified = entry.metadata().ok().and_then(|m| m.modified().ok());
        found.push((entry.path(), parsed, modified));
    }
    found.sort_by(|a, b| {
        b.1.stamp
            .cmp(&a.1.stamp)
            .then(b.2.cmp(&a.2))
            .then(b.0.cmp(&a.0))
    });
    found.truncate(MAX_LISTED);
    Ok(found)
}

/// A listed safety backup as an entry.
pub(crate) fn safety_entry(
    file: FileRef,
    name: &SafetyName,
    modified: Option<SystemTime>,
) -> SafetyBackupEntry {
    SafetyBackupEntry {
        file,
        product: name.product.clone(),
        stamp: name.stamp.clone(),
        reason: safety_reason(&name.reason),
        modified_at: modified.map(rfc3339_utc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_names_parse() {
        let parsed =
            parse_safety_name("openwraith-W75HEARGB-20261024-073000-set-trigger.owbackup.json")
                .unwrap();
        assert_eq!(parsed.product.as_deref(), Some("W75HEARGB"));
        assert_eq!(parsed.stamp, "20261024-073000");
        assert_eq!(safety_reason(&parsed.reason), SafetyReason::SetTrigger);
        let numbered =
            parse_safety_name("openwraith-W75HEARGB-20261024-073000-restore-3.owbackup.json")
                .unwrap();
        assert_eq!(safety_reason(&numbered.reason), SafetyReason::Restore);
        for (reason, expected) in [
            ("set-keys", SafetyReason::SetKeys),
            ("set-colors", SafetyReason::SetColors),
            ("set-deadband", SafetyReason::SetDeadband),
            ("set-socd", SafetyReason::SetSocd),
            ("import", SafetyReason::Import),
            ("something-new", SafetyReason::Other),
        ] {
            assert_eq!(safety_reason(reason), expected, "{reason}");
        }
        assert!(parse_safety_name("notes.txt").is_none());
        assert!(parse_safety_name("openwraith-undated.owbackup.json").is_none());
    }

    #[test]
    fn the_registry_keeps_tokens_per_path() {
        let mut mint = TokenMint::new(Some(3));
        let mut files = FileRegistry::default();
        let a = files.register(&mut mint, PathBuf::from("a.json"), FileKind::User);
        let again = files.register(&mut mint, PathBuf::from("a.json"), FileKind::User);
        assert_eq!(a.token, again.token);
        let b = files.register(&mut mint, PathBuf::from("a.json"), FileKind::Safety);
        assert_ne!(a.token, b.token);
        assert_eq!(files.get(&a.token).unwrap().kind, FileKind::User);
        for n in 0..MAX_FILES {
            files.register(&mut mint, PathBuf::from(format!("{n}")), FileKind::User);
        }
        assert!(files.get(&a.token).is_none());
    }
}
