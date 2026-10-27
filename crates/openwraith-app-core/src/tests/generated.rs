//! Generated frontend files: checked by default, rewritten with an
//! environment variable (`UPDATE_BINDINGS=1`, `UPDATE_FIXTURES=1`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The repository root.
pub(crate) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Whether `var` asks to rewrite the generated files.
fn update(var: &str) -> bool {
    std::env::var_os(var).is_some_and(|v| v == "1")
}

/// Normalises line endings (a checkout may convert them).
fn normal(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Checks that `dir` holds exactly `files` (with the extension `ext`), or
/// rewrites it when `var` is set.
pub(crate) fn check_or_update(
    dir: &Path,
    ext: &str,
    files: &BTreeMap<String, String>,
    var: &str,
    command: &str,
) {
    let existing: BTreeMap<String, String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == ext))
                .map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let text = std::fs::read_to_string(e.path()).unwrap_or_default();
                    (name, normal(&text))
                })
                .collect()
        })
        .unwrap_or_default();
    if update(var) {
        std::fs::create_dir_all(dir).unwrap();
        for name in existing.keys().filter(|n| !files.contains_key(*n)) {
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        for (name, text) in files {
            if existing.get(name) != Some(text) {
                std::fs::write(dir.join(name), text).unwrap();
            }
        }
        return;
    }
    let missing: Vec<_> = files
        .keys()
        .filter(|n| !existing.contains_key(*n))
        .collect();
    let extra: Vec<_> = existing
        .keys()
        .filter(|n| !files.contains_key(*n))
        .collect();
    let changed: Vec<_> = files
        .iter()
        .filter(|(n, t)| existing.get(*n).is_some_and(|e| e != *t))
        .map(|(n, _)| n)
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty() && changed.is_empty(),
        "{} is out of date (missing {missing:?}, extra {extra:?}, changed {changed:?}); run `{command}`",
        dir.display()
    );
}
