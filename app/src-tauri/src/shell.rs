//! The core's [`ShellHook`]: Settings > Background changed the tray icon or
//! start with Windows, and the shell follows (design B5, B11). The core has
//! saved the settings already; an error here is shown to the user and the
//! settings stay saved. The hook also says what Windows will do at the next
//! sign-in, which Settings > Background shows for as long as it lasts.

use openwraith_app_core::ShellHook;
use openwraith_app_core::dto::{AppError, AppErrorKind, ShellChanges, WindowsStartState};
use tauri::{AppHandle, Runtime};

use crate::{autostart, tray};

/// Applies background setting changes to the tray icon and the Run entry.
pub struct Shell<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> ShellHook for Shell<R> {
    fn apply(&self, changes: &ShellChanges) -> Result<(), AppError> {
        let mut problems = Vec::new();
        // The Run entry first: without the tray it must go even when the
        // icon cannot be removed.
        if let Some(start) = changes.start_with_windows
            && let Err(error) = autostart::set_enabled(start)
        {
            problems.push(error.message);
        }
        if let Some(shown) = changes.tray {
            match tray::get(&self.0) {
                Some(icon) => {
                    if let Err(problem) = icon.set_shown_from_any_thread(shown) {
                        problems.push(problem);
                    }
                }
                None => problems.push("the tray icon is not available".to_string()),
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(AppError::new(AppErrorKind::Io, problems.join("; ")))
        }
    }

    fn windows_start(&self, wanted: bool) -> WindowsStartState {
        autostart::state(wanted)
    }
}
