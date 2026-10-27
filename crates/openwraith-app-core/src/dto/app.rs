//! App information and settings.

use serde::{Deserialize, Serialize};

use super::error::AppError;

/// Facts about this build of the app.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AppInfo {
    /// The app's version.
    pub version: String,
    /// Whether this is the developer simulator (no keyboard is used).
    pub sim: bool,
    /// The operating system.
    pub platform: PlatformView,
    /// The operating system's locale (`"tr-TR"`), if known.
    pub os_locale: Option<String>,
    /// Where safety backups go.
    pub safety_dir: SafetyDirView,
    /// Firmware versions the app was tested with.
    pub tested_firmware: Vec<String>,
    /// The source repository.
    pub repository: String,
}

/// The operating system family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum PlatformView {
    Windows,
    Linux,
    Macos,
}

impl PlatformView {
    /// The platform this build runs on.
    pub fn current() -> Self {
        if cfg!(windows) {
            PlatformView::Windows
        } else if cfg!(target_os = "macos") {
            PlatformView::Macos
        } else {
            PlatformView::Linux
        }
    }
}

/// The safety-backup folder, or why there is none (writes are blocked then).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SafetyDirView {
    /// The folder.
    Ok {
        /// Its path, for display.
        path: String,
    },
    /// No folder could be determined.
    Unavailable {
        /// Why.
        error: AppError,
    },
}

/// The user's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// The language of the user interface.
    pub language: LanguageSetting,
    /// The colour theme.
    pub theme: ThemeSetting,
}

/// The language setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum LanguageSetting {
    /// The operating system's language (Turkish or English).
    #[default]
    System,
    /// English.
    En,
    /// Turkish.
    Tr,
}

/// The theme setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ThemeSetting {
    /// Follow the operating system.
    #[default]
    System,
    /// Light.
    Light,
    /// Dark.
    Dark,
}
