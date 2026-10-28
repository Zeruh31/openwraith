//! The user's settings file (`settings.json`), saved atomically: the
//! settings the frontend edits (language, theme), the user's trigger
//! presets, the hints already shown, the background settings (tray, start
//! with Windows, notifications, automatic backups), the per-game rules and
//! the local profile names. Fields this version does not know (written by a
//! newer one) are kept when it saves. Every read-modify-write of the file
//! holds one lock, so the settings and the presets never overwrite each
//! other's changes.
//!
//! The fields added in v0.2 (`tray_enabled`, `start_with_windows`,
//! `close_to_tray_explained`, `notifications`, `auto_backup`,
//! `auto_profile`, `profile_names`) are read leniently: a missing or
//! unreadable field is its default, never a reason to lose the file, and
//! fields of a newer version inside them are kept when this version saves.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::dto::{
    AppError, AppErrorKind, AutoBackupSettings, AutoProfileRule, AutoProfileSettings,
    BackgroundSettingsIntent, BackgroundSettingsView, EditProblemView, ErrorDetail,
    LanguageSetting, RtIntent, Settings, ShellChanges, ThemeSetting,
};
use crate::map::editors::refused;

/// Serializes every read-modify-write of a settings file.
static SETTINGS_IO: Mutex<()> = Mutex::new(());

/// At most this many hints are remembered.
const MAX_HINTS: usize = 128;

/// The keys of the v0.2 fields (kept in [`SettingsFile::other`] and read
/// leniently).
const TRAY_ENABLED: &str = "tray_enabled";
const START_WITH_WINDOWS: &str = "start_with_windows";
const CLOSE_TO_TRAY_EXPLAINED: &str = "close_to_tray_explained";
const NOTIFICATIONS: &str = "notifications";
const AUTO_BACKUP: &str = "auto_backup";
const AUTO_PROFILE: &str = "auto_profile";
const PROFILE_NAMES: &str = "profile_names";

/// The longest local profile name.
pub(crate) const MAX_PROFILE_NAME: usize = 32;

/// The most per-game rules.
pub(crate) const MAX_RULES: usize = 64;

/// A user's trigger preset as stored in the settings file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredPreset {
    /// `u-<n>`.
    pub(crate) id: String,
    /// The name (1 to 40 characters).
    pub(crate) name: String,
    /// Actuation point in trigger units.
    pub(crate) actuation: u16,
    /// Rapid Trigger.
    pub(crate) rapid_trigger: RtIntent,
}

/// The whole file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default)]
    language: LanguageSetting,
    #[serde(default)]
    theme: ThemeSetting,
    /// Kept as JSON values: an entry this version cannot read is skipped,
    /// never a reason to lose the file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    presets: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hints_shown: Vec<String>,
    /// Fields of other versions.
    #[serde(flatten)]
    other: Map<String, Value>,
}

impl SettingsFile {
    fn presets(&self) -> Vec<StoredPreset> {
        self.presets
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect()
    }

    /// A v0.2 field: its default when missing or unreadable.
    fn get<T: DeserializeOwned + Default>(&self, key: &str) -> T {
        self.other
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Sets a v0.2 field; inside an object, fields this version does not
    /// know are kept.
    fn set<T: Serialize>(&mut self, key: &str, value: &T) {
        let Ok(new) = serde_json::to_value(value) else {
            return;
        };
        match (self.other.get_mut(key), new) {
            (Some(Value::Object(old)), Value::Object(new)) => {
                for (k, v) in new {
                    old.insert(k, v);
                }
            }
            (_, new) => {
                self.other.insert(key.to_string(), new);
            }
        }
    }

    fn background(&self) -> BackgroundSettingsView {
        let auto_backup: AutoBackupSettings = self.get(AUTO_BACKUP);
        let tray_enabled: bool = self.get(TRAY_ENABLED);
        BackgroundSettingsView {
            tray_enabled,
            start_with_windows: tray_enabled && self.get::<bool>(START_WITH_WINDOWS),
            close_to_tray_explained: self.get(CLOSE_TO_TRAY_EXPLAINED),
            notifications: self.get(NOTIFICATIONS),
            auto_backup: AutoBackupSettings {
                keep: auto_backup.keep.clamp(1, crate::autobackup::MAX_KEEP),
                ..auto_backup
            },
        }
    }

    fn profile_names(&self) -> BTreeMap<String, Vec<Option<String>>> {
        let mut names: BTreeMap<String, Vec<Option<String>>> = self.get(PROFILE_NAMES);
        for list in names.values_mut() {
            list.resize(4, None);
        }
        names
    }
}

/// How the file looks on disk.
enum OnDisk {
    Missing,
    Valid(SettingsFile),
    /// Not a settings file (kept aside before it is replaced).
    Invalid,
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    SETTINGS_IO.lock().unwrap_or_else(|e| e.into_inner())
}

fn read_file(path: &Path) -> OnDisk {
    match std::fs::read_to_string(path) {
        Err(_) => OnDisk::Missing,
        Ok(text) => match serde_json::from_str::<SettingsFile>(&text) {
            Ok(file) => OnDisk::Valid(file),
            Err(_) => OnDisk::Invalid,
        },
    }
}

fn io_error(path: &Path, e: &dyn std::fmt::Display) -> AppError {
    AppError::with(
        AppErrorKind::Io,
        ErrorDetail::Io {
            file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
        },
        format!("cannot save the settings to {}: {e}", path.display()),
    )
}

/// `settings.invalid-<stamp>[-N].json` next to `path`.
fn invalid_name(path: &Path) -> PathBuf {
    let stamp = openwraith_hid::rfc3339_utc(std::time::SystemTime::now())
        .replace(['-', ':'], "")
        .replace('T', "-")
        .trim_end_matches('Z')
        .to_string();
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut candidate = dir.join(format!("settings.invalid-{stamp}.json"));
    let mut n = 1;
    while candidate.exists() && n < 1000 {
        candidate = dir.join(format!("settings.invalid-{stamp}-{n}.json"));
        n += 1;
    }
    candidate
}

/// Writes `file` atomically; a file on disk that is not a settings file is
/// kept as `settings.invalid-<stamp>.json` first.
fn write_file(path: &Path, file: &SettingsFile, invalid_on_disk: bool) -> Result<(), AppError> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| io_error(path, &e))?;
    if invalid_on_disk {
        // Best effort: the settings are saved either way.
        let _ = std::fs::rename(path, invalid_name(path));
    }
    let json = serde_json::to_string_pretty(file).map_err(|e| io_error(path, &e))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".settings-")
        .suffix(".partial")
        .tempfile_in(dir)
        .map_err(|e| io_error(path, &e))?;
    temp.write_all(json.as_bytes())
        .map_err(|e| io_error(path, &e))?;
    temp.as_file().sync_all().map_err(|e| io_error(path, &e))?;
    temp.persist(path).map_err(|e| io_error(path, &e.error))?;
    Ok(())
}

/// Changes the file under the lock: `f` gets the file as read (the
/// defaults when it is missing or invalid).
fn modify<T>(path: &Path, f: impl FnOnce(&mut SettingsFile) -> T) -> Result<T, AppError> {
    let _guard = lock();
    let (mut file, invalid) = match read_file(path) {
        OnDisk::Valid(file) => (file, false),
        OnDisk::Missing => (SettingsFile::default(), false),
        OnDisk::Invalid => (SettingsFile::default(), true),
    };
    let result = f(&mut file);
    write_file(path, &file, invalid)?;
    Ok(result)
}

fn current(path: &Path) -> SettingsFile {
    let _guard = lock();
    match read_file(path) {
        OnDisk::Valid(file) => file,
        OnDisk::Missing | OnDisk::Invalid => SettingsFile::default(),
    }
}

/// Reads the settings; the defaults when the file is missing or invalid.
pub fn load_settings(path: &Path) -> Settings {
    let file = current(path);
    Settings {
        language: file.language,
        theme: file.theme,
    }
}

/// Saves the settings atomically (a temporary file moved into place); the
/// presets, the hints and fields of other versions in the file are kept.
pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), AppError> {
    modify(path, |file| {
        file.language = settings.language;
        file.theme = settings.theme;
    })
}

/// The user's trigger presets (entries that cannot be read are skipped).
pub(crate) fn load_presets(path: &Path) -> Vec<StoredPreset> {
    current(path).presets()
}

/// Replaces the user's trigger presets with what `f` makes of them.
pub(crate) fn update_presets<T>(
    path: &Path,
    f: impl FnOnce(&mut Vec<StoredPreset>) -> Result<T, AppError>,
) -> Result<(Vec<StoredPreset>, T), AppError> {
    let _guard = lock();
    let (mut file, invalid) = match read_file(path) {
        OnDisk::Valid(file) => (file, false),
        OnDisk::Missing => (SettingsFile::default(), false),
        OnDisk::Invalid => (SettingsFile::default(), true),
    };
    let mut presets = file.presets();
    let result = f(&mut presets)?;
    file.presets = presets
        .iter()
        .filter_map(|p| serde_json::to_value(p).ok())
        .collect();
    write_file(path, &file, invalid)?;
    Ok((presets, result))
}

/// The hints already shown.
pub(crate) fn hints_shown(path: &Path) -> Vec<String> {
    current(path).hints_shown
}

/// Remembers that `hint` was shown; returns every hint shown.
pub(crate) fn mark_hint_shown(path: &Path, hint: &str) -> Result<Vec<String>, AppError> {
    modify(path, |file| {
        if !file.hints_shown.iter().any(|h| h == hint) {
            file.hints_shown.push(hint.to_string());
        }
        let excess = file.hints_shown.len().saturating_sub(MAX_HINTS);
        file.hints_shown.drain(..excess);
        file.hints_shown.clone()
    })
}

/// Moves an unreadable settings file aside (`settings.invalid-<stamp>.json`)
/// at start, so that it is kept and the defaults are used. `None`: the file
/// is missing or valid; `Some(name)`: it was unreadable and is kept as
/// `name` (`Some(None)`: it could not be moved; the next save moves it).
pub(crate) fn quarantine_invalid(path: &Path) -> Option<Option<String>> {
    let _guard = lock();
    match read_file(path) {
        OnDisk::Missing | OnDisk::Valid(_) => None,
        OnDisk::Invalid => {
            let target = invalid_name(path);
            Some(
                std::fs::rename(path, &target)
                    .ok()
                    .and_then(|()| target.file_name())
                    .map(|n| n.to_string_lossy().into_owned()),
            )
        }
    }
}

/// The background settings.
pub(crate) fn background(path: &Path) -> BackgroundSettingsView {
    current(path).background()
}

/// The automatic-backup settings (the defaults without a file).
pub(crate) fn auto_backup(path: Option<&Path>) -> AutoBackupSettings {
    path.map_or_else(AutoBackupSettings::default, |p| background(p).auto_backup)
}

/// Saves the background settings (validated: `keep` 1-100; without the
/// tray there is no start with Windows) and returns them with what the
/// shell must change.
pub(crate) fn set_background(
    path: &Path,
    intent: &BackgroundSettingsIntent,
) -> Result<(BackgroundSettingsView, ShellChanges), AppError> {
    let keep = intent.auto_backup.keep;
    if !(1..=crate::autobackup::MAX_KEEP).contains(&keep) {
        return Err(refused(
            EditProblemView::Other {
                message: format!(
                    "keep 1 to {} automatic backups per profile, not {keep}",
                    crate::autobackup::MAX_KEEP
                ),
            },
            "the background settings were not saved",
        ));
    }
    modify(path, |file| {
        let before = file.background();
        let start_with_windows = intent.start_with_windows && intent.tray_enabled;
        file.set(TRAY_ENABLED, &intent.tray_enabled);
        file.set(START_WITH_WINDOWS, &start_with_windows);
        file.set(NOTIFICATIONS, &intent.notifications);
        file.set(AUTO_BACKUP, &intent.auto_backup);
        let after = file.background();
        let changes = ShellChanges {
            tray: (before.tray_enabled != after.tray_enabled).then_some(after.tray_enabled),
            start_with_windows: (before.start_with_windows != after.start_with_windows)
                .then_some(after.start_with_windows),
        };
        (after, changes)
    })
}

/// Remembers that the first close to the tray was explained.
pub(crate) fn mark_close_to_tray_explained(path: &Path) -> Result<(), AppError> {
    modify(path, |file| file.set(CLOSE_TO_TRAY_EXPLAINED, &true))
}

/// The local profile names, per product (four entries each).
pub(crate) fn profile_names(path: &Path) -> BTreeMap<String, Vec<Option<String>>> {
    current(path).profile_names()
}

/// A profile name as stored: control characters removed, trimmed, at most
/// 32 characters; empty is no name.
fn clean_name(name: Option<&str>) -> Option<String> {
    let text: String = name?.chars().filter(|c| !c.is_control()).collect();
    let text: String = text.trim().chars().take(MAX_PROFILE_NAME).collect();
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Names the profiles of `product` (exactly four names); returns every
/// product's names.
pub(crate) fn set_profile_names(
    path: &Path,
    product: &str,
    names: &[Option<String>],
) -> Result<BTreeMap<String, Vec<Option<String>>>, AppError> {
    let valid_product = !product.is_empty()
        && product.len() <= 32
        && product
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !valid_product || names.len() != 4 {
        return Err(refused(
            EditProblemView::Other {
                message: "four names of a product (letters, digits, '_', '-', '.')".to_string(),
            },
            "the profile names were not saved",
        ));
    }
    let names: Vec<Option<String>> = names.iter().map(|n| clean_name(n.as_deref())).collect();
    modify(path, |file| {
        let mut all = file.profile_names();
        if names.iter().all(Option::is_none) {
            all.remove(product);
        } else {
            all.insert(product.to_string(), names);
        }
        // Replaced as a whole (a product without names is removed).
        if let Ok(value) = serde_json::to_value(&all) {
            file.other.insert(PROFILE_NAMES.to_string(), value);
        }
        all
    })
}

/// The per-game rules (off by default).
pub(crate) fn auto_profile(path: &Path) -> AutoProfileSettings {
    current(path).get(AUTO_PROFILE)
}

/// Validates per-game rules: at most 64, unique ids (letters, digits and
/// `-`, at most 16), a program file name (no folder), profiles 1-4, labels
/// of at most 64 characters (control characters removed).
pub(crate) fn validate_auto_profile(
    settings: &AutoProfileSettings,
) -> Result<AutoProfileSettings, AppError> {
    let problem = |message: String| {
        refused(
            EditProblemView::Other { message },
            "the game rules were not saved",
        )
    };
    if settings.rules.len() > MAX_RULES {
        return Err(problem(format!("at most {MAX_RULES} rules")));
    }
    if settings
        .default_profile
        .is_some_and(|p| !(1..=4).contains(&p))
    {
        return Err(problem("the default profile is 1 to 4".to_string()));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut rules = Vec::new();
    for rule in &settings.rules {
        let id_ok = !rule.id.is_empty()
            && rule.id.len() <= 16
            && rule
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-');
        if !id_ok || !ids.insert(rule.id.clone()) {
            return Err(problem(format!(
                "rule id {:?} is not valid or used twice",
                rule.id
            )));
        }
        let exe = rule.exe.trim();
        if exe.is_empty()
            || exe.len() > 260
            || exe.contains(['/', '\\', ':'])
            || exe.chars().any(char::is_control)
        {
            return Err(problem(format!("{exe:?} is not a program file name")));
        }
        if !(1..=4).contains(&rule.profile) {
            return Err(problem(format!("profile {} does not exist", rule.profile)));
        }
        let label: String = rule
            .label
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        let path_hint = rule
            .path_hint
            .as_deref()
            .map(|p| {
                p.chars()
                    .filter(|c| !c.is_control())
                    .take(260)
                    .collect::<String>()
            })
            .filter(|p| !p.trim().is_empty());
        rules.push(AutoProfileRule {
            id: rule.id.clone(),
            label: label.trim().to_string(),
            exe: exe.to_string(),
            path_hint,
            profile: rule.profile,
            enabled: rule.enabled,
        });
    }
    Ok(AutoProfileSettings {
        enabled: settings.enabled,
        default_profile: settings.default_profile,
        rules,
    })
}

/// Saves the per-game rules after [`validate_auto_profile`]; returns them
/// as saved.
pub(crate) fn set_auto_profile(
    path: &Path,
    settings: &AutoProfileSettings,
) -> Result<AutoProfileSettings, AppError> {
    let valid = validate_auto_profile(settings)?;
    modify(path, |file| file.set(AUTO_PROFILE, &valid))?;
    Ok(valid)
}

/// Forgets every hint shown ("Show tips again"); returns the empty list.
pub(crate) fn reset_hints_shown(path: &Path) -> Result<Vec<String>, AppError> {
    modify(path, |file| {
        file.hints_shown.clear();
        file.hints_shown.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{LanguageSetting, ThemeSetting};

    #[test]
    fn settings_round_trip_and_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg").join("settings.json");
        assert_eq!(load_settings(&path), Settings::default());
        let settings = Settings {
            language: LanguageSetting::Tr,
            theme: ThemeSetting::Dark,
        };
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path), settings);
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load_settings(&path), Settings::default());
    }

    #[test]
    fn saving_the_settings_keeps_presets_hints_and_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"language":"en","theme":"light","tray_enabled":true,
                "presets":[{"id":"u-1","name":"Mine","actuation":60,"rapid_trigger":{"mode":"off"}},
                           {"broken":true}],
                "hints_shown":["depth_note"]}"#,
        )
        .unwrap();
        assert_eq!(load_presets(&path).len(), 1, "the broken entry is skipped");
        save_settings(
            &path,
            &Settings {
                language: LanguageSetting::Tr,
                theme: ThemeSetting::Dark,
            },
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("tray_enabled"), "{text}");
        assert_eq!(load_presets(&path)[0].name, "Mine");
        assert_eq!(hints_shown(&path), ["depth_note"]);
        assert_eq!(load_settings(&path).language, LanguageSetting::Tr);
        let hints = mark_hint_shown(&path, "keys_intro").unwrap();
        assert_eq!(hints, ["depth_note", "keys_intro"]);
        assert_eq!(mark_hint_shown(&path, "keys_intro").unwrap().len(), 2);
        assert!(reset_hints_shown(&path).unwrap().is_empty());
        assert!(hints_shown(&path).is_empty());
        assert_eq!(load_presets(&path)[0].name, "Mine", "the presets stay");
    }

    #[test]
    fn an_unreadable_file_is_kept_aside_before_it_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        save_settings(&path, &Settings::default()).unwrap();
        let kept: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("settings.invalid-"))
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(load_settings(&path), Settings::default());
    }
}
