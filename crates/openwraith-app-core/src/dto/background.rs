//! The background settings (Settings > Background), the local profile
//! names (Settings > Profile names) and the stored per-game rules.

use serde::{Deserialize, Serialize};

use super::error::AppError;

/// Which notifications OpenWraith shows (from Rust; the banner and the tray
/// stay authoritative).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(default)]
pub struct NotificationSettings {
    /// The keyboard looks reset, its firmware changed, an automatic backup
    /// failed (default on).
    pub reset: bool,
    /// A game rule switched the profile (default off).
    pub auto_switch: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        NotificationSettings {
            reset: true,
            auto_switch: false,
        }
    }
}

/// Automatic backups (the known-good store).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(default)]
pub struct AutoBackupSettings {
    /// Make automatic backups (and compare reads with them; default on).
    pub enabled: bool,
    /// How many are kept per profile, 1-100 (default 10); pinned ones and
    /// safety backups are never deleted.
    pub keep: u8,
}

impl Default for AutoBackupSettings {
    fn default() -> Self {
        AutoBackupSettings {
            enabled: true,
            keep: crate::autobackup::DEFAULT_KEEP,
        }
    }
}

/// What Windows will do at the next sign-in, as the shell read it from the
/// per-user Run entry and the Task Manager choice (read again every time
/// the background settings are read or saved; nothing here is stored).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum WindowsStartState {
    /// The switch is off and Windows does not start OpenWraith.
    Off,
    /// The switch is on and Windows starts this OpenWraith at sign-in.
    On,
    /// The switch is on, but OpenWraith is turned off in Task Manager >
    /// Startup apps (OpenWraith never changes that choice).
    OffInTaskManager,
    /// The switch is on, but Windows has no entry that starts this
    /// OpenWraith (it was removed, or it starts another copy).
    EntryMissing,
    /// The switch is off, but an entry is left and Windows still starts
    /// OpenWraith (its removal failed).
    StillOn,
    /// Not known: a simulator build, no shell, or the registry could not be
    /// read.
    #[default]
    Unavailable,
}

/// The background settings as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BackgroundSettingsView {
    /// Keep OpenWraith running in the notification area (default off).
    pub tray_enabled: bool,
    /// Start with Windows, hidden in the notification area (needs the tray).
    pub start_with_windows: bool,
    /// What Windows will do at the next sign-in (not stored: the shell
    /// reads it each time through the core's shell hook; `unavailable`
    /// without one).
    pub windows_start: WindowsStartState,
    /// The first close to the tray was explained (a notification once).
    pub close_to_tray_explained: bool,
    /// Notifications.
    pub notifications: NotificationSettings,
    /// Automatic backups.
    pub auto_backup: AutoBackupSettings,
}

/// Change the background settings (`set_background_settings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct BackgroundSettingsIntent {
    /// Keep OpenWraith running in the notification area. Off also turns
    /// `start_with_windows` off.
    pub tray_enabled: bool,
    /// Start with Windows (only with the tray).
    pub start_with_windows: bool,
    /// Notifications.
    pub notifications: NotificationSettings,
    /// Automatic backups (`keep` 1-100).
    pub auto_backup: AutoBackupSettings,
}

/// What the shell (the Tauri glue) must change after the background
/// settings changed: `null` = unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ShellChanges {
    /// Show (`true`) or remove (`false`) the tray icon.
    pub tray: Option<bool>,
    /// Add (`true`) or remove (`false`) the per-user Run entry.
    pub start_with_windows: Option<bool>,
}

impl ShellChanges {
    /// Whether nothing changes.
    pub fn is_empty(&self) -> bool {
        self.tray.is_none() && self.start_with_windows.is_none()
    }
}

/// The result of `set_background_settings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BackgroundSettingsApplied {
    /// The settings as saved (`start_with_windows` is off without the
    /// tray).
    pub settings: BackgroundSettingsView,
    /// What changed for the shell.
    pub shell: ShellChanges,
    /// The shell could not apply them (the settings are saved anyway).
    pub shell_error: Option<AppError>,
}

/// The local names of one product's profiles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProductProfileNamesView {
    /// The product code (`W75HEARGB`).
    pub product: String,
    /// Profiles 1-4 (`null` = no name).
    pub names: Vec<Option<String>>,
}

/// The local profile names (`get_profile_names`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProfileNamesView {
    /// The product of the keyboard read last, if any.
    pub current_product: Option<String>,
    /// Every product with names, and the current one.
    pub products: Vec<ProductProfileNamesView>,
}

/// Name the profiles of a product (`set_profile_names`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ProfileNamesIntent {
    /// The product code (letters, digits, `_`, `-`; at most 32).
    pub product: String,
    /// Exactly four names, profiles 1-4; `null` or empty = no name; at most
    /// 32 characters each (control characters are removed).
    pub names: Vec<Option<String>>,
}

/// A per-game rule (Settings > Games): when the program is in front, switch
/// to the profile. Creating the rule is the user's consent for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AutoProfileRule {
    /// A short stable id (`r-<n>`).
    pub id: String,
    /// The game's name as the user sees it.
    pub label: String,
    /// The program's file name (`game.exe`), compared case-insensitively.
    pub exe: String,
    /// The folder it was picked from, to tell programs with the same name
    /// apart (`null` = any).
    pub path_hint: Option<String>,
    /// The profile (1-4).
    pub profile: u8,
    /// The rule is on.
    pub enabled: bool,
}

/// The per-game automatic profiles as stored (off by default).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(default)]
pub struct AutoProfileSettings {
    /// Switch profiles automatically for games.
    pub enabled: bool,
    /// The profile to return to when a game exits (only if the active
    /// profile is still the one the rule set).
    pub default_profile: Option<u8>,
    /// The rules.
    pub rules: Vec<AutoProfileRule>,
}
