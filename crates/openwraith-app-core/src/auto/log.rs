//! The automation log: `<state dir>/automation.log`, one JSON object per
//! line (time and event, with the rule id), rotated to `automation.1.log`
//! at 256 KiB (one old file is kept). Best effort: a log that cannot be
//! written changes nothing else.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::dto::AutoActivityView;

/// The log's file name.
pub(crate) const LOG_FILE: &str = "automation.log";

/// The rotated log's file name.
pub(crate) const OLD_LOG_FILE: &str = "automation.1.log";

/// The log is rotated beyond this size.
pub(crate) const MAX_LOG_BYTES: u64 = 256 * 1024;

/// The log file in `state_dir`.
pub(crate) fn path(state_dir: &Path) -> PathBuf {
    state_dir.join(LOG_FILE)
}

/// Appends `entries` to the log in `state_dir` (rotating it first when it
/// is too big).
pub(crate) fn append(state_dir: &Path, entries: &[AutoActivityView]) {
    if entries.is_empty() {
        return;
    }
    let _ = append_inner(state_dir, entries, MAX_LOG_BYTES);
}

fn append_inner(state_dir: &Path, entries: &[AutoActivityView], max: u64) -> std::io::Result<()> {
    std::fs::create_dir_all(state_dir)?;
    let file = path(state_dir);
    if std::fs::metadata(&file).is_ok_and(|m| m.len() >= max) {
        std::fs::rename(&file, state_dir.join(OLD_LOG_FILE))?;
    }
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)?;
    let mut text = String::new();
    for entry in entries {
        if let Ok(line) = serde_json::to_string(entry) {
            text.push_str(&line);
            text.push('\n');
        }
    }
    out.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{AutoCauseView, AutoEventView};

    fn entry(n: u8) -> AutoActivityView {
        AutoActivityView {
            at: "2026-10-24T07:30:00Z".to_string(),
            event: AutoEventView::Switched {
                rule_id: Some(format!("r-{n}")),
                label: Some("Game".to_string()),
                from: Some(1),
                to: 2,
                cause: AutoCauseView::GameInFront,
            },
        }
    }

    #[test]
    fn the_log_records_the_rule_and_rotates() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), &[entry(1)]);
        let text = std::fs::read_to_string(path(dir.path())).unwrap();
        assert!(text.contains("\"rule_id\":\"r-1\""), "{text}");
        for n in 0..40 {
            append_inner(dir.path(), &[entry(n)], 1024).unwrap();
        }
        assert!(dir.path().join(OLD_LOG_FILE).is_file(), "rotated");
        assert!(std::fs::metadata(path(dir.path())).unwrap().len() < 2048);
    }
}
