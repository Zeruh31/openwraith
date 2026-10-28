//! The local macro library ("Library (this PC)"): named macros in a small
//! JSON file in the app's data directory (`macro-library.json`, next to the
//! `state` folder), identified by the hash of their program (the keyboard's
//! slots show the same hash, so the Keys page can name them).
//!
//! The file is written atomically. A file that cannot be read (invalid
//! JSON, another format, an invalid macro, a duplicate) is never
//! overwritten: it is renamed to `macro-library.invalid-<stamp>.json`, the
//! library starts empty and the result carries a notice once.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::CoreConfig;
use crate::dto::{
    AppError, AppErrorKind, ErrorDetail, LibraryMacroView, LibraryNotice, MacroLibraryView,
    MacroProgramView,
};
use crate::map::clean;
use crate::map::keys::{content_hash, program_view};
use crate::ops::keymap::macro_program;
use crate::shared::lock;

/// The file's `format`.
const FORMAT: &str = "openwraith-macro-library";
/// The file's `version`.
const VERSION: u32 = 1;
/// The most macros in the library.
pub(crate) const MAX_ENTRIES: usize = 200;
/// The longest name, in characters.
pub(crate) const MAX_NAME: usize = 64;
/// The file name.
const FILE_NAME: &str = "macro-library.json";

/// One process at a time reads and writes the file (the app is a single
/// instance).
static FILE_LOCK: Mutex<()> = Mutex::new(());

/// The file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    format: String,
    version: u32,
    macros: Vec<StoredMacro>,
}

/// One macro of the file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredMacro {
    name: String,
    program: MacroProgramView,
    saved_at: String,
}

/// Where the library lives: next to the app's `state` folder.
pub(crate) fn path(config: &CoreConfig) -> PathBuf {
    config
        .state_dir
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(&config.state_dir)
        .join(FILE_NAME)
}

fn io_error(path: &Path, error: &dyn std::fmt::Display) -> AppError {
    AppError::with(
        AppErrorKind::Io,
        ErrorDetail::Io {
            file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
        },
        format!("cannot use the macro library {}: {error}", path.display()),
    )
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::new(AppErrorKind::InvalidRequest, message)
}

/// A checked name: no control characters, trimmed, 1-64 characters.
fn checked_name(name: &str) -> Result<String, AppError> {
    let name = clean(name).trim().to_string();
    let count = name.chars().count();
    if count == 0 || count > MAX_NAME {
        return Err(invalid(format!(
            "a macro name has 1 to {MAX_NAME} characters; nothing was saved"
        )));
    }
    Ok(name)
}

/// The entries of a file's text, or why it is not a valid library.
fn parse(text: &str) -> Result<Vec<LibraryMacroView>, String> {
    let file: LibraryFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if file.format != FORMAT || file.version != VERSION {
        return Err(format!("format {} version {}", file.format, file.version));
    }
    if file.macros.len() > MAX_ENTRIES {
        return Err(format!("{} macros", file.macros.len()));
    }
    let mut entries: Vec<LibraryMacroView> = Vec::new();
    for stored in file.macros {
        let program = macro_program(&stored.program).map_err(|e| e.message)?;
        let name = checked_name(&stored.name).map_err(|e| e.message)?;
        let hash = content_hash(&program);
        if entries.iter().any(|e| e.content_hash == hash) {
            return Err(format!("macro {hash} twice"));
        }
        entries.push(LibraryMacroView {
            content_hash: hash,
            name,
            program: program_view(&program),
            saved_at: clean(&stored.saved_at),
        });
    }
    Ok(entries)
}

/// `20261024-073000` of now.
fn stamp(config: &CoreConfig) -> String {
    openwraith_hid::rfc3339_utc(config.now())
        .replace(['-', ':'], "")
        .replace('T', "-")
        .trim_end_matches('Z')
        .to_string()
}

/// Moves an unreadable library aside; returns the name it was kept as.
fn keep_invalid(path: &Path, config: &CoreConfig) -> Result<String, AppError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let base = format!("macro-library.invalid-{}", stamp(config));
    let mut name = format!("{base}.json");
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{base}-{n}.json");
        n += 1;
    }
    std::fs::rename(path, dir.join(&name)).map_err(|e| io_error(path, &e))?;
    Ok(name)
}

/// Loads the library (an unreadable file is kept aside, see the
/// [module documentation](self)).
fn load(path: &Path, config: &CoreConfig) -> Result<MacroLibraryView, AppError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(MacroLibraryView {
                entries: Vec::new(),
                notice: None,
            });
        }
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => String::new(),
        Err(e) => return Err(io_error(path, &e)),
    };
    match parse(&text) {
        Ok(entries) => Ok(MacroLibraryView {
            entries,
            notice: None,
        }),
        Err(_) => {
            let kept_as = keep_invalid(path, config)?;
            Ok(MacroLibraryView {
                entries: Vec::new(),
                notice: Some(LibraryNotice::InvalidFileKept { kept_as }),
            })
        }
    }
}

/// Saves the library atomically.
fn store(path: &Path, entries: &[LibraryMacroView]) -> Result<(), AppError> {
    let file = LibraryFile {
        format: FORMAT.to_string(),
        version: VERSION,
        macros: entries
            .iter()
            .map(|e| StoredMacro {
                name: e.name.clone(),
                program: e.program.clone(),
                saved_at: e.saved_at.clone(),
            })
            .collect(),
    };
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| io_error(path, &e))?;
    let json = serde_json::to_string_pretty(&file).map_err(|e| io_error(path, &e))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".macro-library-")
        .suffix(".partial")
        .tempfile_in(dir)
        .map_err(|e| io_error(path, &e))?;
    temp.write_all(json.as_bytes())
        .map_err(|e| io_error(path, &e))?;
    temp.as_file().sync_all().map_err(|e| io_error(path, &e))?;
    temp.persist(path).map_err(|e| io_error(path, &e.error))?;
    Ok(())
}

/// Loads, changes and saves the library under the lock.
fn change(
    config: &CoreConfig,
    f: impl FnOnce(&mut Vec<LibraryMacroView>) -> Result<(), AppError>,
) -> Result<MacroLibraryView, AppError> {
    let _guard = lock(&FILE_LOCK);
    let path = path(config);
    let mut library = load(&path, config)?;
    f(&mut library.entries)?;
    store(&path, &library.entries)?;
    Ok(library)
}

/// `list_macro_library`.
pub(crate) fn list(config: &CoreConfig) -> Result<MacroLibraryView, AppError> {
    let _guard = lock(&FILE_LOCK);
    load(&path(config), config)
}

/// `save_library_macro`: adds a macro, or renames the entry that holds the
/// same program.
pub(crate) fn save(
    config: &CoreConfig,
    name: &str,
    program: &MacroProgramView,
) -> Result<MacroLibraryView, AppError> {
    let name = checked_name(name)?;
    let program = macro_program(program)?;
    let hash = content_hash(&program);
    let saved_at = openwraith_hid::rfc3339_utc(config.now());
    change(config, |entries| {
        if let Some(entry) = entries.iter_mut().find(|e| e.content_hash == hash) {
            entry.name = name;
            return Ok(());
        }
        if entries.len() >= MAX_ENTRIES {
            return Err(invalid(format!(
                "the library holds at most {MAX_ENTRIES} macros; nothing was saved"
            )));
        }
        entries.push(LibraryMacroView {
            content_hash: hash,
            name,
            program: program_view(&program),
            saved_at,
        });
        Ok(())
    })
}

/// The entry with `hash`.
fn find<'a>(
    entries: &'a mut [LibraryMacroView],
    hash: &str,
) -> Result<&'a mut LibraryMacroView, AppError> {
    entries
        .iter_mut()
        .find(|e| e.content_hash == hash)
        .ok_or_else(|| invalid("that macro is not in the library"))
}

/// `rename_library_macro`.
pub(crate) fn rename(
    config: &CoreConfig,
    hash: &str,
    name: &str,
) -> Result<MacroLibraryView, AppError> {
    let name = checked_name(name)?;
    change(config, |entries| {
        find(entries, hash)?.name = name;
        Ok(())
    })
}

/// `delete_library_macro`.
pub(crate) fn delete(config: &CoreConfig, hash: &str) -> Result<MacroLibraryView, AppError> {
    change(config, |entries| {
        find(entries, hash)?;
        entries.retain(|e| e.content_hash != hash);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;
    use crate::dto::{KeyMotionView, MacroPlayView, MacroStepView};

    /// A configuration in `dir` at 2026-10-24T07:30:00Z.
    fn test_config(dir: &Path) -> CoreConfig {
        let mut config = CoreConfig::new(dir.join("state"), "0.2.0");
        config.fixed_time = Some(UNIX_EPOCH + Duration::from_secs(1_792_827_000));
        config
    }

    fn program(usage: u8, delay: u16) -> MacroProgramView {
        MacroProgramView {
            play: MacroPlayView::Once,
            steps: vec![
                MacroStepView {
                    usage,
                    motion: KeyMotionView::Press,
                    delay_after_ms: delay,
                },
                MacroStepView {
                    usage,
                    motion: KeyMotionView::Release,
                    delay_after_ms: 0,
                },
            ],
        }
    }

    #[test]
    fn save_rename_delete_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        assert_eq!(path(&config), dir.path().join("macro-library.json"));
        assert!(list(&config).unwrap().entries.is_empty());
        let saved = save(&config, "  Copy  ", &program(0x04, 30)).unwrap();
        assert_eq!(saved.entries.len(), 1);
        assert_eq!(saved.entries[0].name, "Copy");
        assert_eq!(saved.entries[0].saved_at, "2026-10-24T07:30:00Z");
        let hash = saved.entries[0].content_hash.clone();
        // The same program again renames it.
        let again = save(&config, "Paste", &program(0x04, 30)).unwrap();
        assert_eq!(again.entries.len(), 1);
        assert_eq!(again.entries[0].name, "Paste");
        let two = save(&config, "B", &program(0x05, 0)).unwrap();
        assert_eq!(two.entries.len(), 2);
        let renamed = rename(&config, &hash, "Again").unwrap();
        assert_eq!(renamed.entries[0].name, "Again");
        assert_eq!(list(&config).unwrap(), renamed);
        let left = delete(&config, &hash).unwrap();
        assert_eq!(left.entries.len(), 1);
        assert_eq!(left.entries[0].name, "B");
        assert!(delete(&config, &hash).is_err());
        assert!(save(&config, "", &program(0x06, 0)).is_err());
        assert!(save(&config, &"x".repeat(65), &program(0x06, 0)).is_err());
        // An invalid macro is refused before anything is saved.
        let mut held = program(0x06, 0);
        held.steps.pop();
        assert!(save(&config, "held", &held).is_err());
        assert_eq!(list(&config).unwrap().entries.len(), 1);
    }

    #[test]
    fn an_unreadable_file_is_kept_aside_and_the_library_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let file = path(&config);
        std::fs::write(&file, "{ not json").unwrap();
        let listed = list(&config).unwrap();
        assert!(listed.entries.is_empty());
        assert_eq!(
            listed.notice,
            Some(LibraryNotice::InvalidFileKept {
                kept_as: "macro-library.invalid-20261024-073000.json".to_string()
            })
        );
        assert!(!file.exists());
        let kept = dir
            .path()
            .join("macro-library.invalid-20261024-073000.json");
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "{ not json");
        // Only once.
        assert_eq!(list(&config).unwrap().notice, None);
        // A second invalid file gets another name; a valid shape with an
        // invalid macro is invalid too.
        std::fs::write(
            &file,
            r#"{"format":"openwraith-macro-library","version":1,"macros":[{"name":"x","program":{"play":{"mode":"once"},"steps":[]},"saved_at":""}]}"#,
        )
        .unwrap();
        let saved = save(&config, "New", &program(0x04, 0)).unwrap();
        assert_eq!(
            saved.notice,
            Some(LibraryNotice::InvalidFileKept {
                kept_as: "macro-library.invalid-20261024-073000-2.json".to_string()
            })
        );
        assert_eq!(saved.entries.len(), 1);
        assert!(file.exists());
    }
}
