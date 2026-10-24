//! Safety backups: a backup that is saved and verified **before** anything
//! is written to the keyboard.
//!
//! Every public way to write a table
//! ([`Writer::write_guarded`](crate::Writer::write_guarded),
//! [`execute_restore`](crate::execute_restore),
//! [`apply_trigger_edit`](crate::apply_trigger_edit),
//! [`apply_light_edit`](crate::apply_light_edit)) takes a [`SavedBackup`]
//! that must hold the profile it writes; the unguarded write primitive is
//! private to this crate. Only [`Backup::save_safety`] creates a
//! `SavedBackup`: it saves the backup without replacing any file, reads it
//! back and checks that it is identical. So "always back up first" is
//! enforced by the types. Profile selects change no stored setting and need
//! no backup. Safety backups are never
//! deleted automatically; the undo command is
//! `openwraith restore <file>`.
//!
//! The default directory ([`default_dir`]):
//!
//! | System | Directory |
//! |---|---|
//! | Windows | `%LOCALAPPDATA%\OpenWraith\safety-backups` |
//! | Linux / BSD | `$XDG_DATA_HOME/openwraith/safety-backups`, else `$HOME/.local/share/openwraith/safety-backups` |
//! | macOS | `$HOME/Library/Application Support/OpenWraith/safety-backups` |
//!
//! The environment variable `OPENWRAITH_SAFETY_DIR` overrides it. If no
//! directory can be determined, the operation fails before anything is
//! written; it never falls back to the current directory.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use openwraith_protocol::ProfileId;
use serde::Serialize;

use crate::backup::{BACKUP_EXTENSION, Backup};
use crate::error::BackupError;

/// Environment variable that overrides the safety-backup directory.
pub const SAFETY_DIR_ENV: &str = "OPENWRAITH_SAFETY_DIR";

/// The directory for safety backups (see the [module documentation](self)).
pub fn default_dir() -> Result<PathBuf, BackupError> {
    dir_from(|name| std::env::var_os(name), Platform::current())
}

/// The operating-system family that decides the default directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Windows,
    MacOs,
    Unix,
}

impl Platform {
    fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Unix
        }
    }
}

/// [`default_dir`] with an injectable environment.
fn dir_from(
    var: impl Fn(&str) -> Option<OsString>,
    platform: Platform,
) -> Result<PathBuf, BackupError> {
    let non_empty = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = non_empty(SAFETY_DIR_ENV) {
        return Ok(dir);
    }
    // Relative values are ignored (XDG base directory rules), so the current
    // directory is never used by accident.
    let absolute = |name: &str| non_empty(name).filter(|p| p.is_absolute());
    let unavailable = |reason: &str| BackupError::SafetyDirUnavailable {
        reason: format!("{reason}; set {SAFETY_DIR_ENV} to choose a directory"),
    };
    match platform {
        Platform::Windows => absolute("LOCALAPPDATA")
            .map(|base| base.join("OpenWraith").join("safety-backups"))
            .ok_or_else(|| unavailable("LOCALAPPDATA is not set")),
        Platform::MacOs => absolute("HOME")
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join("OpenWraith")
                    .join("safety-backups")
            })
            .ok_or_else(|| unavailable("HOME is not set")),
        Platform::Unix => absolute("XDG_DATA_HOME")
            .or_else(|| absolute("HOME").map(|home| home.join(".local").join("share")))
            .map(|base| base.join("openwraith").join("safety-backups"))
            .ok_or_else(|| unavailable("neither XDG_DATA_HOME nor HOME is set")),
    }
}

/// A backup that was saved and verified by [`Backup::save_safety`]. Write
/// operations take one as proof that the keyboard's state was backed up
/// first. There is no other way to create one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SavedBackup {
    path: PathBuf,
    profiles: Vec<u8>,
    created_at: String,
    product: Option<String>,
    /// The saved backup itself, to bind it to the operation it protects.
    #[serde(skip)]
    backup: Box<Backup>,
}

impl SavedBackup {
    /// The saved file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The profiles the backup holds, ascending.
    pub fn profiles(&self) -> &[u8] {
        &self.profiles
    }

    /// Whether the backup holds `profile`.
    pub fn covers(&self, profile: ProfileId) -> bool {
        self.profiles.contains(&profile.get())
    }

    /// Creation time of the backup (RFC 3339).
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    /// Product code of the keyboard.
    pub fn product(&self) -> Option<&str> {
        self.product.as_deref()
    }

    /// The command that restores this backup.
    pub fn undo_command(&self) -> String {
        undo_command(&self.path)
    }

    /// Whether this is the saved copy of exactly `backup`.
    pub fn describes(&self, backup: &Backup) -> bool {
        *self.backup == *backup
    }

    /// The backup that was saved.
    pub fn backup(&self) -> &Backup {
        &self.backup
    }
}

/// `openwraith restore <path>`, with the path quoted when needed.
pub(crate) fn undo_command(path: &Path) -> String {
    let text = path.display().to_string();
    if text.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
        format!("openwraith restore \"{}\"", text.replace('"', "\\\""))
    } else {
        format!("openwraith restore {text}")
    }
}

impl Backup {
    /// Saves this backup as a safety backup in `dir` and verifies it.
    ///
    /// 1. Creates `dir` if needed.
    /// 2. Picks a new name `openwraith-<product>-<stamp>-<reason>[-N].owbackup.json`.
    /// 3. Saves without replacing any file (see [`save`](Self::save)).
    /// 4. Reads the file back and checks that it gives the same backup.
    ///
    /// `reason` is reduced to lowercase letters, digits and `-`.
    pub fn save_safety(&self, dir: &Path, reason: &str) -> Result<SavedBackup, BackupError> {
        std::fs::create_dir_all(dir).map_err(|source| BackupError::Io {
            action: "create",
            path: dir.to_path_buf(),
            source,
        })?;
        let reason: String = reason
            .chars()
            .map(|c| c.to_ascii_lowercase())
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        let reason = if reason.is_empty() {
            "safety".to_string()
        } else {
            reason
        };
        let base = format!(
            "openwraith-{}-{}-{reason}",
            self.file_product(),
            self.file_stamp()
        );
        let mut saved_at = None;
        for n in 1..=1000u32 {
            let name = if n == 1 {
                format!("{base}.{BACKUP_EXTENSION}")
            } else {
                format!("{base}-{n}.{BACKUP_EXTENSION}")
            };
            let path = dir.join(name);
            match self.save(&path, false) {
                Ok(()) => {
                    saved_at = Some(path);
                    break;
                }
                Err(BackupError::AlreadyExists { .. }) => continue,
                Err(e) => return Err(e),
            }
        }
        let Some(path) = saved_at else {
            return Err(BackupError::AlreadyExists {
                path: dir.join(format!("{base}.{BACKUP_EXTENSION}")),
            });
        };
        match Backup::load(&path) {
            Ok(loaded) if loaded == *self => {}
            _ => return Err(BackupError::SafetyCheckFailed { path }),
        }
        Ok(SavedBackup {
            path,
            profiles: self.profile_numbers(),
            created_at: self.created_at.clone(),
            product: self.device.product.clone(),
            backup: Box::new(self.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn override_wins() {
        let vars = [(SAFETY_DIR_ENV, "/tmp/x"), ("HOME", "/home/u")];
        assert_eq!(
            dir_from(env(&vars), Platform::Unix).ok(),
            Some(PathBuf::from("/tmp/x"))
        );
    }

    #[test]
    fn platform_defaults() {
        let home = if cfg!(windows) {
            "C:\\Users\\u"
        } else {
            "/home/u"
        };
        let vars = [("HOME", home), ("LOCALAPPDATA", home)];
        let unix = dir_from(env(&vars), Platform::Unix).unwrap();
        assert!(
            unix.ends_with("openwraith/safety-backups")
                || unix.ends_with("openwraith\\safety-backups")
        );
        assert!(unix.starts_with(home));
        assert!(unix.to_string_lossy().contains(".local"));
        let mac = dir_from(env(&vars), Platform::MacOs).unwrap();
        assert!(mac.to_string_lossy().contains("Application Support"));
        let win = dir_from(env(&vars), Platform::Windows).unwrap();
        assert!(win.to_string_lossy().contains("OpenWraith"));
        // XDG_DATA_HOME is preferred on Unix.
        let xdg = if cfg!(windows) { "C:\\data" } else { "/data" };
        let vars = [("HOME", home), ("XDG_DATA_HOME", xdg)];
        assert!(
            dir_from(env(&vars), Platform::Unix)
                .unwrap()
                .starts_with(xdg)
        );
    }

    #[test]
    fn never_the_current_directory() {
        // Nothing set, or only relative paths: an error, not ".".
        for vars in [
            &[][..],
            &[
                ("HOME", "relative"),
                ("XDG_DATA_HOME", "rel"),
                ("LOCALAPPDATA", "x"),
            ][..],
            &[(SAFETY_DIR_ENV, "")][..],
        ] {
            for platform in [Platform::Windows, Platform::MacOs, Platform::Unix] {
                assert!(matches!(
                    dir_from(env(vars), platform),
                    Err(BackupError::SafetyDirUnavailable { .. })
                ));
            }
        }
    }

    #[test]
    fn undo_commands_quote_paths() {
        assert_eq!(
            undo_command(Path::new("a.owbackup.json")),
            "openwraith restore a.owbackup.json"
        );
        assert_eq!(
            undo_command(Path::new("my dir/a.json")),
            "openwraith restore \"my dir/a.json\""
        );
    }
}
