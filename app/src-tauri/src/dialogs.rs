//! File dialogs, from Rust only (the frontend never sees a path; it gets a
//! file token). Called on a blocking thread, never on the main thread.

use std::path::PathBuf;

use openwraith_app_core::dto::{AppError, AppErrorKind, DialogText};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;

/// The extensions a backup may have (`*.owbackup.json` is a `*.json`).
const BACKUP_EXTENSIONS: &[&str] = &["json"];

fn builder<R: Runtime>(
    app: &AppHandle<R>,
    text: &DialogText,
) -> tauri_plugin_dialog::FileDialogBuilder<R> {
    let mut builder = app
        .dialog()
        .file()
        .set_title(text.title.clone())
        .add_filter(text.filter.clone(), BACKUP_EXTENSIONS);
    if let Some(window) = app.get_webview_window("main") {
        builder = builder.set_parent(&window);
    }
    builder
}

fn path_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorKind::Io,
        format!("the chosen file has no usable path: {error}"),
    )
}

/// The open dialog for a backup file; `None` when cancelled.
pub fn pick_backup<R: Runtime>(
    app: &AppHandle<R>,
    text: &DialogText,
) -> Result<Option<PathBuf>, AppError> {
    builder(app, text)
        .blocking_pick_file()
        .map(|path| path.into_path().map_err(path_error))
        .transpose()
}

/// The save dialog for a new backup; `None` when cancelled. The dialog asks
/// before replacing a file.
pub fn choose_destination<R: Runtime>(
    app: &AppHandle<R>,
    text: &DialogText,
    file_name: &str,
) -> Result<Option<PathBuf>, AppError> {
    builder(app, text)
        .set_file_name(file_name.to_string())
        .blocking_save_file()
        .map(|path| path.into_path().map_err(path_error))
        .transpose()
}
