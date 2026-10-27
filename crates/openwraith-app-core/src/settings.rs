//! The user's settings file (`settings.json`), saved atomically.

use std::io::Write;
use std::path::Path;

use crate::dto::{AppError, AppErrorKind, ErrorDetail, Settings};

/// Reads the settings; the defaults when the file is missing or invalid.
pub fn load_settings(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Saves the settings atomically (a temporary file moved into place).
pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), AppError> {
    let io_error = |e: &dyn std::fmt::Display| {
        AppError::with(
            AppErrorKind::Io,
            ErrorDetail::Io {
                file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
            },
            format!("cannot save the settings to {}: {e}", path.display()),
        )
    };
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| io_error(&e))?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| io_error(&e))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".settings-")
        .suffix(".partial")
        .tempfile_in(dir)
        .map_err(|e| io_error(&e))?;
    temp.write_all(json.as_bytes()).map_err(|e| io_error(&e))?;
    temp.as_file().sync_all().map_err(|e| io_error(&e))?;
    temp.persist(path).map_err(|e| io_error(&e.error))?;
    Ok(())
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
}
