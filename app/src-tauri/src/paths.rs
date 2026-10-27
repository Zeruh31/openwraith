//! Where the app keeps its files.
//!
//! | What | Real keyboard | Simulator |
//! |---|---|---|
//! | Safety backups | the CLI's folder (`OPENWRAITH_SAFETY_DIR`, else `%LOCALAPPDATA%\OpenWraith\safety-backups`) | `<app local data>/sim/safety-backups` (or `OPENWRAITH_SIM_DIR`) |
//! | Operation journal | `<app local data>/state` | `<app local data>/sim/state` |
//! | Settings | `<app config>/settings.json` | the same |

use std::path::PathBuf;

use openwraith_app_core::CoreConfig;
use tauri::{AppHandle, Manager, Runtime};

/// The core's configuration for this build.
pub fn core_config<R: Runtime>(app: &AppHandle<R>) -> Result<CoreConfig, String> {
    let local = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("no local data directory: {e}"))?;
    let version = app.package_info().version.to_string();
    let mut config = if cfg!(feature = "sim") {
        let base = std::env::var_os("OPENWRAITH_SIM_DIR")
            .filter(|v| !v.is_empty())
            .map_or_else(|| local.join("sim"), PathBuf::from);
        let mut config = CoreConfig::new(base.join("state"), version);
        config.safety_dir = Ok(base.join("safety-backups"));
        config
    } else {
        CoreConfig::new(local.join("state"), version)
    };
    config.os_locale = sys_locale::get_locale();
    // LICENSE.txt and THIRD-PARTY-NOTICES.txt (`bundle.resources`).
    config.legal_dir = app.path().resource_dir().ok();
    Ok(config)
}

/// The settings file.
pub fn settings_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("settings.json"))
        .map_err(|e| format!("no configuration directory: {e}"))
}
