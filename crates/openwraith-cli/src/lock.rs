//! The device lock shared with the desktop app
//! ([`openwraith_hid::coordination`]).
//!
//! Every command that opens the keyboard takes the lock before it opens it
//! and holds it until it ends: **exclusively** when it may write (`set`,
//! `restore`, `profile select`, `profile import`, `profile export` of the
//! keyboard (it may switch profiles), `backup --all-profiles`, `travel`),
//! **shared** when it only reads (`info`, `show`, `backup`, `profile`,
//! `monitor`, and every `--dry-run` against the keyboard). Commands that
//! read only files (`--from`, `restore --dry-run --against`) and `list`
//! take none.
//!
//! When another OpenWraith program holds the lock in a way that excludes
//! the command (the desktop app writing, or another command), the command
//! stops at once with "the OpenWraith app is writing; try again", before
//! the keyboard is opened: nothing is sent. A lock that cannot be used at
//! all (no per-user directory, an I/O error) does not stop the command: it
//! warns that other OpenWraith programs are not told, and the "other
//! program" gate of every write still protects the keyboard.

use std::path::PathBuf;

use openwraith_hid::coordination::TOOL_CLI;
use openwraith_hid::{DeviceLock, LockError, LockMode};

/// What a command needs of the lock: its mode and the purpose recorded in
/// `device.lock.json` (e.g. `set trigger`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockNeed {
    /// Exclusive for a command that may write, shared for one that reads.
    pub mode: LockMode,
    /// What the command does.
    pub purpose: String,
}

impl LockNeed {
    /// A command that may write.
    pub fn write(purpose: impl Into<String>) -> Self {
        LockNeed {
            mode: LockMode::Exclusive,
            purpose: purpose.into(),
        }
    }

    /// A command that only reads.
    pub fn read(purpose: impl Into<String>) -> Self {
        LockNeed {
            mode: LockMode::Shared,
            purpose: purpose.into(),
        }
    }
}

/// Takes the lock in `dir` as `need` says. Fails at once (with
/// [`LockError::Busy`]) when another program holds it; any other problem
/// with the lock is passed to `warn` and the command goes on without it.
/// `None` when no lock was taken.
pub fn take(
    dir: Result<Option<PathBuf>, LockError>,
    need: &LockNeed,
    warn: &mut dyn FnMut(&str),
) -> anyhow::Result<Option<DeviceLock>> {
    let unusable = |e: &LockError| {
        format!(
            "warning: {e}; other OpenWraith programs (the desktop app) are not told that this \
             command uses the keyboard"
        )
    };
    let dir = match dir {
        Ok(Some(dir)) => dir,
        Ok(None) => return Ok(None),
        Err(e) => {
            warn(&unusable(&e));
            return Ok(None);
        }
    };
    let taken = match need.mode {
        LockMode::Exclusive => DeviceLock::try_exclusive(&dir, TOOL_CLI, &need.purpose),
        LockMode::Shared => DeviceLock::try_shared(&dir, TOOL_CLI, &need.purpose),
    };
    match taken {
        Ok(lock) => Ok(Some(lock)),
        Err(e @ LockError::Busy { .. }) => Err(e.into()),
        Err(e) => {
            warn(&unusable(&e));
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwraith_hid::coordination::TOOL_APP;

    #[test]
    fn a_busy_lock_stops_and_an_unusable_one_warns() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let mut warnings = Vec::new();
        let mut warn = |w: &str| warnings.push(w.to_string());

        // Free: taken, released when dropped.
        let lock = take(
            Ok(Some(dir.path().into())),
            &LockNeed::write("set"),
            &mut warn,
        )
        .expect("taken")
        .expect("a lock");
        assert_eq!(lock.mode(), LockMode::Exclusive);
        assert_eq!(lock.info().tool, TOOL_CLI);
        drop(lock);

        // Held by the app: refused at once.
        let app = DeviceLock::try_exclusive(dir.path(), TOOL_APP, "write").expect("app");
        let err = take(
            Ok(Some(dir.path().into())),
            &LockNeed::read("show"),
            &mut warn,
        )
        .expect_err("busy");
        assert_eq!(err.to_string(), "the OpenWraith app is writing; try again");
        drop(app);

        // No directory: a warning, no lock, the command goes on.
        let none = take(
            Err(LockError::NoDirectory {
                reason: "LOCALAPPDATA is not set".into(),
            }),
            &LockNeed::write("set"),
            &mut warn,
        )
        .expect("goes on");
        assert!(none.is_none());
        // A directory that cannot be created (it is a file).
        let file = dir.path().join("file");
        std::fs::write(&file, b"x").expect("written");
        let none = take(Ok(Some(file)), &LockNeed::read("show"), &mut warn).expect("goes on");
        assert!(none.is_none());
        // Nothing to coordinate with.
        assert!(
            take(Ok(None), &LockNeed::write("set"), &mut warn)
                .expect("no lock")
                .is_none()
        );
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings[0].contains("LOCALAPPDATA is not set"),
            "{warnings:?}"
        );
        assert!(warnings.iter().all(|w| w.contains("are not told")));
    }
}
