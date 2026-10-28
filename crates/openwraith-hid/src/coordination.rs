//! Coordination between OpenWraith programs that use the same keyboard
//! (the command line tool and the desktop app; design part B, B9).
//!
//! [`DeviceLock`] is an operating-system file lock on `device.lock` in the
//! per-user OpenWraith directory ([`default_dir`]):
//!
//! | System | Lock file |
//! |---|---|
//! | Windows | `%LOCALAPPDATA%\OpenWraith\device.lock` |
//! | Linux / BSD | `$XDG_DATA_HOME/openwraith/device.lock`, else `$HOME/.local/share/openwraith/device.lock` |
//! | macOS | `$HOME/Library/Application Support/OpenWraith/device.lock` |
//!
//! A program that writes to the keyboard holds it **exclusively**
//! ([`DeviceLock::try_exclusive`]) for as long as it writes; a program that
//! only reads holds it **shared** ([`DeviceLock::try_shared`]). Several
//! readers can hold it at once, a writer only alone. Neither call waits: a
//! lock held by another program is [`LockError::Busy`] at once, and nothing
//! has been sent to the keyboard. The lock is
//! [`File::try_lock`](std::fs::File::try_lock) /
//! [`File::try_lock_shared`](std::fs::File::try_lock_shared) (`LockFileEx`
//! on Windows, `flock` on Unix; Rust 1.89): the operating system releases it
//! when the holding process ends, however it ends, so a crash never leaves
//! a stale lock. [`DeviceLock::probe`] tells a program that holds none
//! whether others hold it ([`LockState`]).
//!
//! Next to it, `device.lock.json` says who holds the lock ([`LockInfo`]:
//! process id, tool, purpose, mode, start time), for messages such as "the
//! OpenWraith app is writing; try again" ([`DeviceLock::holder`]). It is
//! **advisory only**: it is written after the lock was taken (by an
//! exclusive holder always; by a shared one unless another reader's
//! description is there) and removed when the holder that wrote it lets go;
//! it may be stale after a crash, or missing while other readers still hold
//! the lock, and a corrupt or stale file never blocks anything.
//!
//! Every function takes the directory, so tests use a temporary one.

use std::ffi::OsString;
use std::fmt;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::backup::rfc3339_utc;

/// File name of the lock file.
pub const LOCK_FILE_NAME: &str = "device.lock";
/// File name of the advisory holder description.
pub const INFO_FILE_NAME: &str = "device.lock.json";
/// [`LockInfo::tool`] of the command line tool.
pub const TOOL_CLI: &str = "openwraith";
/// [`LockInfo::tool`] of the desktop app.
pub const TOOL_APP: &str = "openwraith-app";
/// The largest `device.lock.json` [`DeviceLock::holder`] reads.
const INFO_MAX_BYTES: u64 = 16 * 1024;
/// The longest purpose kept in [`LockInfo`], in characters.
const PURPOSE_MAX: usize = 200;

/// How a lock is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockMode {
    /// Alone: the holder writes to the keyboard.
    Exclusive,
    /// Together with other readers: the holder only reads.
    Shared,
}

impl fmt::Display for LockMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LockMode::Exclusive => "exclusive",
            LockMode::Shared => "shared",
        })
    }
}

/// Who holds the lock, as `device.lock.json` says (advisory; see the
/// [module documentation](self)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockInfo {
    /// The holder's process id.
    pub pid: u32,
    /// The program: [`TOOL_CLI`], [`TOOL_APP`] or another name.
    pub tool: String,
    /// What it is doing, e.g. `set trigger` or `automatic profile switch`.
    pub purpose: String,
    /// How it holds the lock.
    pub mode: LockMode,
    /// When it took the lock (RFC 3339, UTC).
    pub started: String,
}

impl LockInfo {
    /// Whether the holder is the desktop app.
    pub fn is_app(&self) -> bool {
        self.tool == TOOL_APP
    }

    /// Whether the holder is the command line tool.
    pub fn is_cli(&self) -> bool {
        self.tool == TOOL_CLI
    }
}

/// The lock could not be taken. Nothing was sent to the keyboard.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// Another program holds the lock in a mode that excludes this one.
    #[error("{}", busy_message(*wanted, holder.as_ref()))]
    Busy {
        /// The mode asked for.
        wanted: LockMode,
        /// Who holds it, if `device.lock.json` says so (advisory).
        holder: Option<LockInfo>,
    },
    /// No per-user directory for the lock could be determined.
    #[error("cannot determine the directory of the device lock: {reason}")]
    NoDirectory {
        /// Why.
        reason: String,
    },
    /// The lock file could not be created or locked.
    #[error("cannot use the device lock {}: {source}", path.display())]
    Io {
        /// The lock file.
        path: PathBuf,
        /// The I/O error.
        #[source]
        source: std::io::Error,
    },
}

impl LockError {
    /// Whether another program holds the lock (as opposed to a problem
    /// with the lock file itself).
    pub fn is_busy(&self) -> bool {
        matches!(self, LockError::Busy { .. })
    }
}

/// The message of [`LockError::Busy`].
fn busy_message(wanted: LockMode, holder: Option<&LockInfo>) -> String {
    // A reader is refused only by a writer; a writer by anyone.
    let doing = match wanted {
        LockMode::Exclusive => "using",
        LockMode::Shared => "writing to",
    };
    match holder {
        Some(info) if info.is_app() => "the OpenWraith app is writing; try again".to_string(),
        Some(info) if info.is_cli() => format!(
            "another openwraith command is using the keyboard (process {}: {}); try again when \
             it has finished",
            info.pid,
            printable(&info.purpose)
        ),
        Some(info) => format!(
            "{} (process {}) is using the keyboard; try again when it has finished",
            printable(&info.tool),
            info.pid
        ),
        None => format!(
            "another OpenWraith program (the OpenWraith app?) is {doing} the keyboard; try again"
        ),
    }
}

/// `text` without control characters (it comes from a file anyone can
/// write).
fn printable(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(PURPOSE_MAX)
        .collect()
}

/// How the lock is held by others, as [`DeviceLock::probe`] found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockState {
    /// Nobody holds it.
    Free,
    /// One or more readers hold it.
    Shared,
    /// A writer holds it.
    Exclusive,
}

/// A held device lock. Dropping it lets go of the lock and removes
/// `device.lock.json` if it still describes this holder.
#[derive(Debug)]
pub struct DeviceLock {
    /// The open lock file; closing it releases the lock.
    _file: File,
    info: LockInfo,
    info_path: PathBuf,
}

impl DeviceLock {
    /// Takes the lock in `dir` exclusively (for a program that writes), or
    /// fails at once with [`LockError::Busy`] when any other holder has it.
    /// `dir` is created if needed. `tool` and `purpose` go into
    /// `device.lock.json`.
    pub fn try_exclusive(dir: &Path, tool: &str, purpose: &str) -> Result<DeviceLock, LockError> {
        Self::take(dir, LockMode::Exclusive, tool, purpose)
    }

    /// Takes the lock in `dir` shared (for a program that only reads), or
    /// fails at once with [`LockError::Busy`] when another program holds it
    /// exclusively. `dir` is created if needed.
    pub fn try_shared(dir: &Path, tool: &str, purpose: &str) -> Result<DeviceLock, LockError> {
        Self::take(dir, LockMode::Shared, tool, purpose)
    }

    /// Who holds the lock in `dir`, as `device.lock.json` says: `None` when
    /// the file is missing, unreadable, too large or not a holder
    /// description. Advisory only: the description may be stale (its holder
    /// ended without removing it) and is never used to decide whether the
    /// lock is free; [`try_exclusive`](Self::try_exclusive) and
    /// [`try_shared`](Self::try_shared) decide that. Never waits.
    pub fn holder(dir: &Path) -> Option<LockInfo> {
        let file = File::open(dir.join(INFO_FILE_NAME)).ok()?;
        let mut text = String::new();
        file.take(INFO_MAX_BYTES + 1)
            .read_to_string(&mut text)
            .ok()?;
        if u64::try_from(text.len()).unwrap_or(u64::MAX) > INFO_MAX_BYTES {
            return None;
        }
        serde_json::from_str(&text).ok()
    }

    /// How other programs hold the lock in `dir` now, for a program that
    /// holds none itself (the desktop app pauses while a command of the
    /// command line tool holds it). Never waits and never writes
    /// `device.lock.json`, but it takes the lock for a moment (shared, then
    /// exclusive): a program that tries to take it in exactly that moment
    /// is refused as if the lock were held, which is safe (it sends nothing
    /// and says "try again"). Probe at a calm pace (about once a second),
    /// not in a tight loop. A missing lock file is [`LockState::Free`] and
    /// is not created.
    pub fn probe(dir: &Path) -> Result<LockState, LockError> {
        let path = dir.join(LOCK_FILE_NAME);
        let file = match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LockState::Free),
            Err(source) => return Err(LockError::Io { path, source }),
        };
        let io = |source| LockError::Io {
            path: path.clone(),
            source,
        };
        match file.try_lock_shared() {
            Ok(()) => file.unlock().map_err(io)?,
            Err(TryLockError::WouldBlock) => return Ok(LockState::Exclusive),
            Err(TryLockError::Error(source)) => return Err(io(source)),
        }
        match file.try_lock() {
            Ok(()) => Ok(LockState::Free),
            Err(TryLockError::WouldBlock) => Ok(LockState::Shared),
            Err(TryLockError::Error(source)) => Err(io(source)),
        }
    }

    /// How this lock is held.
    pub fn mode(&self) -> LockMode {
        self.info.mode
    }

    /// The description written to `device.lock.json`.
    pub fn info(&self) -> &LockInfo {
        &self.info
    }

    fn take(
        dir: &Path,
        mode: LockMode,
        tool: &str,
        purpose: &str,
    ) -> Result<DeviceLock, LockError> {
        let path = dir.join(LOCK_FILE_NAME);
        let io = |source| LockError::Io {
            path: path.clone(),
            source,
        };
        std::fs::create_dir_all(dir).map_err(io)?;
        // Read and write access: Windows refuses to lock a file opened for
        // appending only. The file is never truncated or written.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(io)?;
        let locked = match mode {
            LockMode::Exclusive => file.try_lock(),
            LockMode::Shared => file.try_lock_shared(),
        };
        match locked {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(LockError::Busy {
                    wanted: mode,
                    holder: Self::holder(dir),
                });
            }
            Err(TryLockError::Error(source)) => return Err(io(source)),
        }
        let info = LockInfo {
            pid: std::process::id(),
            tool: printable(tool),
            purpose: printable(purpose),
            mode,
            started: rfc3339_utc(SystemTime::now()),
        };
        let info_path = dir.join(INFO_FILE_NAME);
        // An exclusive holder is alone: any description is stale. A reader
        // keeps the description of another reader (who may still hold the
        // lock) and replaces a writer's (stale: no writer can hold it now).
        let describe = match mode {
            LockMode::Exclusive => true,
            LockMode::Shared => {
                Self::holder(dir).is_none_or(|existing| existing.mode == LockMode::Exclusive)
            }
        };
        if describe {
            // Advisory: a failure to describe the holder never fails the
            // lock.
            let _ = write_info(dir, &info_path, &info);
        }
        Ok(DeviceLock {
            _file: file,
            info,
            info_path,
        })
    }
}

impl Drop for DeviceLock {
    fn drop(&mut self) {
        // Remove the description only while it still describes this holder
        // (another reader may have replaced it since). The lock itself is
        // released when the file is closed, right after this.
        let ours = File::open(&self.info_path)
            .ok()
            .and_then(|file| {
                let mut text = String::new();
                file.take(INFO_MAX_BYTES)
                    .read_to_string(&mut text)
                    .ok()
                    .map(|_| text)
            })
            .and_then(|text| serde_json::from_str::<LockInfo>(&text).ok())
            .is_some_and(|info| info == self.info);
        if ours {
            let _ = std::fs::remove_file(&self.info_path);
        }
    }
}

/// Writes `device.lock.json` through a temporary file in `dir` and a
/// rename, so a reader never sees half a description.
fn write_info(dir: &Path, path: &Path, info: &LockInfo) -> std::io::Result<()> {
    let mut json = serde_json::to_string_pretty(info).map_err(std::io::Error::other)?;
    json.push('\n');
    let mut temp = tempfile::Builder::new()
        .prefix(".device-lock-")
        .suffix(".partial")
        .tempfile_in(dir)?;
    temp.write_all(json.as_bytes())?;
    temp.persist(path).map(|_| ()).map_err(|e| e.error)
}

/// The per-user OpenWraith directory that holds the device lock (see the
/// [module documentation](self)). The command line tool and the desktop app
/// must both use it.
pub fn default_dir() -> Result<PathBuf, LockError> {
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

/// [`default_dir`] with an injectable environment. Relative values are
/// ignored, so the current directory is never used by accident.
fn dir_from(
    var: impl Fn(&str) -> Option<OsString>,
    platform: Platform,
) -> Result<PathBuf, LockError> {
    let absolute = |name: &str| {
        var(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let unavailable = |reason: &str| LockError::NoDirectory {
        reason: reason.to_string(),
    };
    match platform {
        Platform::Windows => absolute("LOCALAPPDATA")
            .map(|base| base.join("OpenWraith"))
            .ok_or_else(|| unavailable("LOCALAPPDATA is not set")),
        Platform::MacOs => absolute("HOME")
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join("OpenWraith")
            })
            .ok_or_else(|| unavailable("HOME is not set")),
        Platform::Unix => absolute("XDG_DATA_HOME")
            .or_else(|| absolute("HOME").map(|home| home.join(".local").join("share")))
            .map(|base| base.join("openwraith"))
            .ok_or_else(|| unavailable("neither XDG_DATA_HOME nor HOME is set")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("temporary directory")
    }

    fn busy_holder(result: Result<DeviceLock, LockError>) -> Option<LockInfo> {
        match result {
            Err(LockError::Busy { holder, .. }) => holder,
            Err(other) => panic!("not busy: {other}"),
            Ok(_) => panic!("the lock was taken"),
        }
    }

    #[test]
    fn an_exclusive_lock_excludes_everyone_else() {
        let dir = dir();
        let app = DeviceLock::try_exclusive(dir.path(), TOOL_APP, "automatic profile switch")
            .expect("free lock");
        assert_eq!(app.mode(), LockMode::Exclusive);
        // A second handle in the same process: refused at once, both ways.
        let holder = busy_holder(DeviceLock::try_exclusive(
            dir.path(),
            TOOL_CLI,
            "set trigger",
        ))
        .expect("described");
        assert!(holder.is_app());
        assert_eq!(holder.pid, std::process::id());
        assert_eq!(holder.mode, LockMode::Exclusive);
        assert_eq!(holder.purpose, "automatic profile switch");
        let err = DeviceLock::try_shared(dir.path(), TOOL_CLI, "show").expect_err("busy");
        assert!(err.is_busy());
        assert_eq!(err.to_string(), "the OpenWraith app is writing; try again");
        // Let go: the lock is free and its description gone.
        drop(app);
        assert_eq!(DeviceLock::holder(dir.path()), None);
        let cli = DeviceLock::try_exclusive(dir.path(), TOOL_CLI, "set trigger").expect("free");
        assert!(cli.info().is_cli());
    }

    #[test]
    fn shared_locks_coexist_and_exclude_a_writer() {
        let dir = dir();
        let first = DeviceLock::try_shared(dir.path(), TOOL_CLI, "show").expect("free");
        let second = DeviceLock::try_shared(dir.path(), TOOL_APP, "refresh").expect("shared too");
        assert_eq!(second.mode(), LockMode::Shared);
        let err = DeviceLock::try_exclusive(dir.path(), TOOL_APP, "write").expect_err("busy");
        // The first reader's description stays: the writer is told who.
        assert!(matches!(
            &err,
            LockError::Busy {
                wanted: LockMode::Exclusive,
                holder: Some(LockInfo { purpose, .. }),
            } if purpose == "show"
        ));
        assert!(err.to_string().contains("show"), "{err}");
        // The second reader's drop leaves it.
        drop(second);
        assert_eq!(
            DeviceLock::holder(dir.path()).map(|h| h.purpose),
            Some("show".to_string())
        );
        let second = DeviceLock::try_shared(dir.path(), TOOL_APP, "refresh").expect("shared");
        // The first reader's drop removes it, although a reader remains.
        drop(first);
        assert_eq!(DeviceLock::holder(dir.path()), None);
        assert!(DeviceLock::try_exclusive(dir.path(), TOOL_APP, "write").is_err());
        drop(second);
        assert!(DeviceLock::try_exclusive(dir.path(), TOOL_APP, "write").is_ok());
    }

    #[test]
    fn a_reader_replaces_a_stale_writers_description() {
        let dir = dir();
        let writer = DeviceLock::try_exclusive(dir.path(), TOOL_APP, "write").expect("free");
        let stale = writer.info().clone();
        // The writer crashed: its lock is gone, its description stays.
        drop(writer);
        std::fs::write(
            dir.path().join(INFO_FILE_NAME),
            serde_json::to_string(&stale).expect("json"),
        )
        .expect("written");
        let reader = DeviceLock::try_shared(dir.path(), TOOL_CLI, "show").expect("free");
        assert_eq!(DeviceLock::holder(dir.path()).as_ref(), Some(reader.info()));
    }

    #[test]
    fn a_probe_tells_how_others_hold_the_lock() {
        let dir = dir();
        // No lock file yet: free, and none is created.
        assert_eq!(
            DeviceLock::probe(dir.path()).expect("probe"),
            LockState::Free
        );
        assert!(!dir.path().join(LOCK_FILE_NAME).exists());
        let reader = DeviceLock::try_shared(dir.path(), TOOL_CLI, "show").expect("free");
        let described = DeviceLock::holder(dir.path());
        assert_eq!(
            DeviceLock::probe(dir.path()).expect("probe"),
            LockState::Shared
        );
        drop(reader);
        assert_eq!(
            DeviceLock::probe(dir.path()).expect("probe"),
            LockState::Free
        );
        let writer = DeviceLock::try_exclusive(dir.path(), TOOL_CLI, "set").expect("free");
        assert_eq!(
            DeviceLock::probe(dir.path()).expect("probe"),
            LockState::Exclusive
        );
        // The probe never touched the description, and released the lock.
        assert_eq!(DeviceLock::holder(dir.path()).as_ref(), Some(writer.info()));
        assert!(described.is_some());
        drop(writer);
        assert!(DeviceLock::try_exclusive(dir.path(), TOOL_APP, "write").is_ok());
    }

    #[test]
    fn a_corrupt_description_never_blocks() {
        let dir = dir();
        std::fs::write(dir.path().join(INFO_FILE_NAME), b"{ not json").expect("written");
        assert_eq!(DeviceLock::holder(dir.path()), None);
        let lock = DeviceLock::try_exclusive(dir.path(), TOOL_CLI, "restore").expect("free");
        // Replaced by this holder's description.
        assert_eq!(DeviceLock::holder(dir.path()).as_ref(), Some(lock.info()));
        // While held, a corrupt description gives an unnamed holder.
        std::fs::write(dir.path().join(INFO_FILE_NAME), b"\x00\x01garbage").expect("written");
        let err = DeviceLock::try_shared(dir.path(), TOOL_APP, "read").expect_err("busy");
        assert!(matches!(&err, LockError::Busy { holder: None, .. }));
        assert!(err.to_string().contains("try again"), "{err}");
        // Not ours any more: the drop leaves the foreign file alone.
        drop(lock);
        assert!(dir.path().join(INFO_FILE_NAME).exists());
        // A huge file is not read.
        let huge = format!(
            "\"{}\"",
            "x".repeat(usize::try_from(INFO_MAX_BYTES).unwrap_or(0))
        );
        std::fs::write(dir.path().join(INFO_FILE_NAME), huge).expect("written");
        assert_eq!(DeviceLock::holder(dir.path()), None);
    }

    #[test]
    fn a_stale_description_never_blocks() {
        let dir = dir();
        // Left by a holder that crashed: the OS released its lock.
        let stale = LockInfo {
            pid: 999_999,
            tool: TOOL_APP.to_string(),
            purpose: "automatic profile switch".to_string(),
            mode: LockMode::Exclusive,
            started: "2026-10-27T10:00:00Z".to_string(),
        };
        std::fs::write(
            dir.path().join(INFO_FILE_NAME),
            serde_json::to_string(&stale).expect("json"),
        )
        .expect("written");
        // Readable, but only advisory: the lock is free.
        assert_eq!(DeviceLock::holder(dir.path()), Some(stale.clone()));
        let lock = DeviceLock::try_exclusive(dir.path(), TOOL_CLI, "set key").expect("free");
        let now = DeviceLock::holder(dir.path()).expect("described");
        assert_ne!(now, stale);
        assert_eq!(now.pid, std::process::id());
        drop(lock);
        assert_eq!(DeviceLock::holder(dir.path()), None);
        let shared = DeviceLock::try_shared(dir.path(), TOOL_CLI, "show");
        assert!(shared.is_ok());
    }

    #[test]
    fn busy_messages_name_the_holder() {
        let info = |tool: &str| LockInfo {
            pid: 42,
            tool: tool.to_string(),
            purpose: "set\u{1b}[31m trigger".to_string(),
            mode: LockMode::Exclusive,
            started: String::new(),
        };
        let busy = |holder| LockError::Busy {
            wanted: LockMode::Exclusive,
            holder,
        };
        assert_eq!(
            busy(Some(info(TOOL_APP))).to_string(),
            "the OpenWraith app is writing; try again"
        );
        let cli = busy(Some(info(TOOL_CLI))).to_string();
        assert!(cli.contains("process 42: set[31m trigger"), "{cli}");
        assert!(!cli.contains('\u{1b}'));
        assert!(busy(None).to_string().contains("try again"));
    }

    /// An environment with these variables.
    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(*v))
        }
    }

    #[test]
    fn the_default_directory() {
        let local = if cfg!(windows) {
            r"C:\Users\u\AppData\Local"
        } else {
            "/home/u/AppData/Local"
        };
        assert_eq!(
            dir_from(env(&[("LOCALAPPDATA", local)]), Platform::Windows).expect("dir"),
            PathBuf::from(local).join("OpenWraith")
        );
        assert!(matches!(
            dir_from(env(&[("LOCALAPPDATA", "relative")]), Platform::Windows),
            Err(LockError::NoDirectory { .. })
        ));
        if !cfg!(windows) {
            assert_eq!(
                dir_from(env(&[("HOME", "/home/u")]), Platform::Unix).expect("dir"),
                PathBuf::from("/home/u/.local/share/openwraith")
            );
            assert_eq!(
                dir_from(
                    env(&[("XDG_DATA_HOME", "/data"), ("HOME", "/home/u")]),
                    Platform::Unix
                )
                .expect("dir"),
                PathBuf::from("/data/openwraith")
            );
            assert_eq!(
                dir_from(env(&[("HOME", "/Users/u")]), Platform::MacOs).expect("dir"),
                PathBuf::from("/Users/u/Library/Application Support/OpenWraith")
            );
        }
        assert!(dir_from(env(&[]), Platform::Unix).is_err());
    }
}
