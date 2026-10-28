//! "Start with Windows" (design B5): the per-user Run entry
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value
//! `OpenWraith` = `<exe> --tray`) through `tauri-plugin-autostart`, used from
//! Rust only (no webview permission).
//!
//! * It needs the tray icon: the core turns it off with the tray, and
//!   turning the tray off removes the entry here. The NSIS uninstaller does
//!   not remove it (the README says so); a start without the tray shows the
//!   window anyway ([`window::starts_hidden`](crate::window::starts_hidden)).
//! * The plugin writes the machine-wide entry instead when OpenWraith runs
//!   as administrator (its `Dynamic` mode, not configurable). Turning it off
//!   checks afterwards that Windows really no longer starts OpenWraith and
//!   says so otherwise.
//! * A developer simulator build never touches the Run key: its changes
//!   are accepted and ignored (an entry would start the debug build at every
//!   logon).
//! * A start from the Run entry itself (`--tray`) never writes to the Run
//!   key while Windows is still processing it at logon (Microsoft Learn,
//!   "Run and RunOnce Registry Keys": a program run from these keys should
//!   not write to the key during its execution). The check of a leftover
//!   entry waits [`LOGON_DELAY`] and reads the settings again first.

use std::time::Duration;

use openwraith_app_core::dto::AppError;
use tauri::{AppHandle, Runtime};

/// How long a start from the Run entry waits before it checks the entry.
pub const LOGON_DELAY: Duration = Duration::from_secs(120);

/// When the start-up check of the Run entry happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileWhen {
    /// The entry is wanted: nothing to check.
    Never,
    /// Now (a normal start).
    Now,
    /// After [`LOGON_DELAY`], with the settings read again (a start from
    /// the Run entry at logon).
    Later,
}

/// When to check the Run entry at start: `args` are the program's
/// arguments, `wanted` whether the settings want the entry.
pub fn reconcile_when<I, S>(args: I, wanted: bool) -> ReconcileWhen
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if wanted {
        ReconcileWhen::Never
    } else if args
        .into_iter()
        .any(|a| a.as_ref() == crate::window::TRAY_FLAG)
    {
        ReconcileWhen::Later
    } else {
        ReconcileWhen::Now
    }
}

/// The name of the Run entry.
#[cfg(not(feature = "sim"))]
const RUN_VALUE_NAME: &str = "OpenWraith";

/// The plugin, for the builder (not in simulator builds).
#[cfg(not(feature = "sim"))]
pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_autostart::Builder::new()
        .app_name(RUN_VALUE_NAME)
        .arg(crate::window::TRAY_FLAG)
        .build()
}

#[cfg(not(feature = "sim"))]
fn error(message: String) -> AppError {
    AppError::new(openwraith_app_core::dto::AppErrorKind::Io, message)
}

/// What a check after a change found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "sim", allow(dead_code))]
pub enum Verified {
    /// Windows starts OpenWraith as wanted (or does not, as wanted).
    AsWanted,
    /// Turned on, but Windows will not start it (for example turned off in
    /// Task Manager).
    StillOff,
    /// Turned off, but an entry remains (the machine-wide one).
    StillOn,
}

/// Compares what was wanted with what Windows reports afterwards.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn verify(wanted: bool, enabled_now: bool) -> Verified {
    match (wanted, enabled_now) {
        (true, false) => Verified::StillOff,
        (false, true) => Verified::StillOn,
        _ => Verified::AsWanted,
    }
}

/// Adds (`true`) or removes (`false`) the Run entry.
#[cfg(not(feature = "sim"))]
pub fn set_enabled<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), AppError> {
    use tauri::Manager;
    let Some(manager) = app.try_state::<tauri_plugin_autostart::AutoLaunchManager>() else {
        return Err(error("start with Windows is not available".to_string()));
    };
    let changed = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    changed.map_err(|e| error(format!("cannot change the Windows start entry: {e}")))?;
    let now = manager
        .is_enabled()
        .map_err(|e| error(format!("cannot read the Windows start entry: {e}")))?;
    match verify(enabled, now) {
        Verified::AsWanted => Ok(()),
        Verified::StillOff => Err(error(
            "Windows will not start OpenWraith: the start entry is turned off (Task Manager > Startup apps)"
                .to_string(),
        )),
        Verified::StillOn => Err(error(
            "Windows still starts OpenWraith from an entry for all users (made while OpenWraith ran as \
             administrator): turn it off in Task Manager > Startup apps"
                .to_string(),
        )),
    }
}

/// Simulator builds: accepted and ignored.
#[cfg(feature = "sim")]
pub fn set_enabled<R: Runtime>(_app: &AppHandle<R>, _enabled: bool) -> Result<(), AppError> {
    Ok(())
}

/// At start: an active entry left behind while the setting is off (an
/// earlier removal failed) is removed. An entry that should exist is left
/// as it is, so that a choice made in Task Manager stays. Reads first, so
/// that a normal start writes nothing. Best effort.
#[cfg(not(feature = "sim"))]
pub fn reconcile<R: Runtime>(app: &AppHandle<R>, wanted: bool) {
    use tauri::Manager;
    if wanted {
        return;
    }
    let active = app
        .try_state::<tauri_plugin_autostart::AutoLaunchManager>()
        .and_then(|manager| manager.is_enabled().ok())
        .unwrap_or(false);
    if active {
        let _ = set_enabled(app, false);
    }
}

/// Simulator builds: nothing to do.
#[cfg(feature = "sim")]
pub fn reconcile<R: Runtime>(_app: &AppHandle<R>, _wanted: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_from_the_run_entry_checks_it_only_later() {
        let tray = crate::window::TRAY_FLAG;
        assert_eq!(
            reconcile_when(["openwraith-app.exe", tray], false),
            ReconcileWhen::Later
        );
        assert_eq!(
            reconcile_when(["openwraith-app.exe"], false),
            ReconcileWhen::Now
        );
        assert_eq!(
            reconcile_when(["openwraith-app.exe", tray], true),
            ReconcileWhen::Never
        );
        assert_eq!(
            reconcile_when(["openwraith-app.exe"], true),
            ReconcileWhen::Never
        );
    }

    #[test]
    fn a_change_is_checked_against_what_windows_reports() {
        assert_eq!(verify(true, true), Verified::AsWanted);
        assert_eq!(verify(false, false), Verified::AsWanted);
        assert_eq!(verify(true, false), Verified::StillOff);
        assert_eq!(verify(false, true), Verified::StillOn);
    }
}
