//! What the app tells the core at start.

use std::path::PathBuf;
use std::time::SystemTime;

use openwraith_hid::{BackupOptions, ToolInfo};

use crate::dto::{AppError, AppErrorKind, SafetyDirView};

/// Firmware versions the app was tested with (a banner and a line in every
/// confirmation for others; never blocking).
pub const TESTED_FIRMWARE: &[&str] = &["3.22.07"];

/// The core's configuration.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    /// Where safety backups go (the CLI's directory in real mode, a separate
    /// one for the simulator), or why none could be determined (writes are
    /// blocked then).
    pub safety_dir: Result<PathBuf, AppError>,
    /// Where the operation journal lives.
    pub state_dir: PathBuf,
    /// The app's version.
    pub version: String,
    /// The operating system's locale, if known.
    pub os_locale: Option<String>,
    /// The source repository URL.
    pub repository: String,
    /// A fixed time for backups (tests and fixtures only).
    pub fixed_time: Option<SystemTime>,
    /// A fixed key for tokens (tests and fixtures only; random otherwise).
    pub token_seed: Option<u64>,
    /// Where the installer put the license files (`LICENSE.txt` and
    /// [`NOTICES_FILE`]): the app's resource directory.
    pub legal_dir: Option<PathBuf>,
}

/// The third-party notices shipped with the app (next to `LICENSE.txt`).
pub const NOTICES_FILE: &str = "THIRD-PARTY-NOTICES.txt";

impl CoreConfig {
    /// A configuration with the safety directory resolved by the library
    /// (`OPENWRAITH_SAFETY_DIR`, else the per-user directory shared with the
    /// CLI).
    pub fn new(state_dir: PathBuf, version: impl Into<String>) -> Self {
        let safety_dir = openwraith_hid::safety::default_dir()
            .map_err(|e| AppError::new(AppErrorKind::SafetyDirUnavailable, e.to_string()));
        CoreConfig {
            safety_dir,
            state_dir,
            version: version.into(),
            os_locale: None,
            repository: "https://github.com/Zeruh31/openwraith".to_string(),
            fixed_time: None,
            token_seed: None,
            legal_dir: None,
        }
    }

    /// The safety directory for display.
    pub(crate) fn safety_dir_view(&self) -> SafetyDirView {
        match &self.safety_dir {
            Ok(path) => SafetyDirView::Ok {
                path: path.display().to_string(),
            },
            Err(error) => SafetyDirView::Unavailable {
                error: error.clone(),
            },
        }
    }

    /// The tool recorded in backups made by the app.
    pub(crate) fn tool(&self, sim: bool) -> ToolInfo {
        ToolInfo {
            name: if sim {
                SIM_TOOL_NAME.to_string()
            } else {
                APP_TOOL_NAME.to_string()
            },
            version: self.version.clone(),
        }
    }

    /// Backup options with the app's tool and the configured time.
    pub(crate) fn backup_options(&self, sim: bool) -> BackupOptions {
        BackupOptions {
            tool: self.tool(sim),
            created_at: self.fixed_time,
        }
    }

    /// The journal file of the running write.
    pub(crate) fn journal_path(&self) -> PathBuf {
        self.state_dir.join("operation-journal.json")
    }

    /// Where a journal left behind by an interrupted write is kept until the
    /// user dismisses it (moved there at start, so that later writes, which
    /// use [`journal_path`](Self::journal_path), never touch it).
    pub(crate) fn interrupted_path(&self) -> PathBuf {
        self.state_dir.join("interrupted-operation.json")
    }

    /// Now, or the fixed time.
    pub(crate) fn now(&self) -> SystemTime {
        self.fixed_time.unwrap_or_else(SystemTime::now)
    }
}

/// The tool name of backups made by the app.
pub(crate) const APP_TOOL_NAME: &str = "openwraith-app";

/// The tool name of backups made by the developer simulator.
pub(crate) const SIM_TOOL_NAME: &str = "openwraith-app-sim";
