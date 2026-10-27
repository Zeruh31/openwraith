//! The operation journal: written (atomically) after the safety backup and
//! before the first write packet of a setting write, removed once its report
//! is built. A journal left behind means the app was closed or crashed in the
//! middle of a write; the status shows it with the safety backup to restore.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dto::{AppError, AppErrorKind, ErrorDetail, OpKind};

/// The keyboard of a journal entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JournalDevice {
    pub(crate) product: Option<String>,
    pub(crate) firmware: Option<String>,
}

/// One running setting write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JournalEntry {
    pub(crate) op: OpKind,
    pub(crate) started_at: String,
    pub(crate) profiles: Vec<u8>,
    pub(crate) device: JournalDevice,
    pub(crate) safety_backup_path: String,
    pub(crate) app_version: String,
}

fn io_error(path: &Path, error: &dyn std::fmt::Display) -> AppError {
    AppError::with(
        AppErrorKind::Io,
        ErrorDetail::Io {
            file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
        },
        format!(
            "cannot write the operation journal {}: {error}; nothing was written to the keyboard",
            path.display()
        ),
    )
}

/// Writes the journal atomically. A failure means the operation must not
/// write anything.
pub(crate) fn write(path: &Path, entry: &JournalEntry) -> Result<(), AppError> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    std::fs::create_dir_all(&dir).map_err(|e| io_error(path, &e))?;
    let json = serde_json::to_string_pretty(entry).map_err(|e| io_error(path, &e))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".journal-")
        .suffix(".partial")
        .tempfile_in(&dir)
        .map_err(|e| io_error(path, &e))?;
    temp.write_all(json.as_bytes())
        .map_err(|e| io_error(path, &e))?;
    temp.as_file().sync_all().map_err(|e| io_error(path, &e))?;
    temp.persist(path).map_err(|e| io_error(path, &e.error))?;
    Ok(())
}

/// Removes the journal (nothing to do when it is missing).
pub(crate) fn remove(path: &Path) -> Result<(), AppError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error(path, &e)),
    }
}

/// At start: moves a journal left behind at `journal` (a write that never
/// finished) to `interrupted`, where only the user's dismissal deletes it;
/// the next write uses `journal` again and cannot overwrite or remove it.
/// The newest interruption replaces an older one that was never dismissed.
/// Returns the file that holds the interrupted operation: `journal` itself
/// when it cannot be moved.
pub(crate) fn adopt_left_behind(journal: &Path, interrupted: &Path) -> PathBuf {
    if !journal.exists() {
        return interrupted.to_path_buf();
    }
    match std::fs::rename(journal, interrupted) {
        Ok(()) => interrupted.to_path_buf(),
        Err(_) => journal.to_path_buf(),
    }
}

/// The journal left behind, if any (an unreadable one counts too: it can
/// only come from an interrupted write).
pub(crate) fn read(path: &Path) -> Option<Result<JournalEntry, String>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return Some(Err(e.to_string())),
    };
    Some(serde_json::from_str(&text).map_err(|e| e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state").join("operation-journal.json");
        assert!(read(&path).is_none());
        let entry = JournalEntry {
            op: OpKind::Restore,
            started_at: "2026-10-24T07:30:00Z".into(),
            profiles: vec![1, 2],
            device: JournalDevice {
                product: Some("W75HEARGB".into()),
                firmware: Some("3.22.07".into()),
            },
            safety_backup_path: "x.owbackup.json".into(),
            app_version: "0.1.0".into(),
        };
        write(&path, &entry).unwrap();
        assert_eq!(read(&path).unwrap().unwrap(), entry);
        remove(&path).unwrap();
        assert!(read(&path).is_none());
        remove(&path).unwrap();
    }

    #[test]
    fn a_journal_left_behind_moves_to_its_own_file() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("operation-journal.json");
        let interrupted = dir.path().join("interrupted-operation.json");
        assert_eq!(adopt_left_behind(&journal, &interrupted), interrupted);
        assert!(read(&interrupted).is_none());
        let mut entry = JournalEntry {
            op: OpKind::Restore,
            started_at: "2026-10-24T07:30:00Z".into(),
            profiles: vec![2],
            device: JournalDevice {
                product: None,
                firmware: None,
            },
            safety_backup_path: "a.owbackup.json".into(),
            app_version: "0.1.0".into(),
        };
        write(&journal, &entry).unwrap();
        assert_eq!(adopt_left_behind(&journal, &interrupted), interrupted);
        assert!(!journal.exists());
        assert_eq!(read(&interrupted).unwrap().unwrap(), entry);
        // Nothing new: the older entry stays.
        assert_eq!(adopt_left_behind(&journal, &interrupted), interrupted);
        assert_eq!(read(&interrupted).unwrap().unwrap(), entry);
        // A newer interruption replaces it.
        entry.op = OpKind::LightEdit;
        write(&journal, &entry).unwrap();
        adopt_left_behind(&journal, &interrupted);
        assert_eq!(read(&interrupted).unwrap().unwrap().op, OpKind::LightEdit);
    }

    #[test]
    fn a_journal_in_an_unusable_place_fails() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "x").unwrap();
        let entry = JournalEntry {
            op: OpKind::TriggerEdit,
            started_at: String::new(),
            profiles: vec![1],
            device: JournalDevice {
                product: None,
                firmware: None,
            },
            safety_backup_path: String::new(),
            app_version: String::new(),
        };
        let error = write(&blocker.join("journal.json"), &entry).unwrap_err();
        assert_eq!(error.kind, AppErrorKind::Io);
    }
}
