//! The user's settings file (`settings.json`), saved atomically: the
//! settings the frontend edits (language, theme), the user's trigger
//! presets and the hints already shown. Fields this version does not know
//! (written by a newer one) are kept when it saves. Every read-modify-write
//! of the file holds one lock, so the settings and the presets never
//! overwrite each other's changes.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::dto::{
    AppError, AppErrorKind, ErrorDetail, LanguageSetting, RtIntent, Settings, ThemeSetting,
};

/// Serializes every read-modify-write of a settings file.
static SETTINGS_IO: Mutex<()> = Mutex::new(());

/// At most this many hints are remembered.
const MAX_HINTS: usize = 128;

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
