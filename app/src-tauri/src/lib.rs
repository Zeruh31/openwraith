//! The OpenWraith desktop app: the Tauri glue around
//! [`openwraith_app_core`] (commands, channels, dialogs, window lifecycle).
//!
//! A release build talks to a real keyboard through hidapi; it opens a
//! connected keyboard at start (listening for 2 s before the first request).
//! A debug build with the feature `sim` uses the developer simulator instead
//! and contains no HID code; a release build with it does not compile.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[cfg(all(feature = "sim", not(debug_assertions)))]
compile_error!("the simulator must never be part of a release build");

/// The names of the IPC commands (the single source).
pub mod command_names;
mod commands;
mod dialogs;
mod paths;
mod sinks;
mod window;

use std::sync::{Arc, Mutex};

use openwraith_app_core::WorkerHandle;
use tauri::{App, Manager, Runtime};

use crate::commands::SettingsState;
use crate::sinks::AppExit;

/// Registers the command handlers (the developer commands only in
/// simulator builds).
macro_rules! handlers {
    ($($extra:path),* $(,)?) => {
        tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::set_settings,
            commands::subscribe,
            commands::get_status,
            commands::get_snapshot,
            commands::get_last_report,
            commands::cancel_operation,
            commands::answer_close,
            commands::dismiss_interrupted,
            commands::list_safety_backups,
            commands::open_safety_backup,
            commands::open_backup_file,
            commands::choose_backup_destination,
            commands::reveal,
            commands::refresh,
            commands::select_device,
            commands::reconnect,
            commands::disconnect,
            commands::recheck_other_program,
            commands::prepare_profile_switch,
            commands::apply_profile_switch,
            commands::prepare_trigger_edit,
            commands::apply_trigger_edit,
            commands::prepare_light_edit,
            commands::apply_light_edit,
            commands::backup_active_profile,
            commands::prepare_all_profile_backup,
            commands::apply_all_profile_backup,
            commands::prepare_restore,
            commands::apply_restore,
            commands::discard_plan,
            commands::get_keys_state,
            commands::prepare_keymap_edit,
            commands::apply_keymap_edit,
            commands::list_macro_library,
            commands::save_library_macro,
            commands::rename_library_macro,
            commands::delete_library_macro,
            commands::get_colors_state,
            commands::prepare_color_edit,
            commands::apply_color_edit,
            commands::get_dead_band_state,
            commands::prepare_dead_band_edit,
            commands::apply_dead_band_edit,
            commands::get_socd_state,
            commands::prepare_socd_edit,
            commands::apply_socd_edit,
            commands::list_presets,
            commands::save_user_preset,
            commands::delete_user_preset,
            commands::prepare_preset_edit,
            commands::start_depth_view,
            commands::stop_depth_view,
            commands::close_stray_travel_test,
            commands::choose_export_destination,
            commands::export_profile,
            commands::open_import_file,
            commands::prepare_import,
            commands::apply_import,
            commands::get_hints_shown,
            commands::mark_hint_shown,
            commands::reset_hints_shown,
            $($extra),*
        ]
    };
}

/// Starts the device worker and loads the settings.
fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let config = paths::core_config(&handle)?;
    let exit = Arc::new(AppExit(handle.clone()));
    #[cfg(feature = "sim")]
    let worker = WorkerHandle::spawn_sim(
        config,
        exit,
        openwraith_app_core::SimOptions::realtime_from_env(),
    )?;
    #[cfg(not(feature = "sim"))]
    let worker = WorkerHandle::spawn_hid(config, exit)?;
    app.manage(Arc::new(worker));

    let settings_path = paths::settings_path(&handle)?;
    let settings = openwraith_app_core::load_settings(&settings_path);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_theme(commands::native_theme(settings.theme));
    }
    app.manage(SettingsState {
        path: settings_path,
        value: Mutex::new(settings),
    });
    Ok(())
}

/// Runs the app until it exits.
pub fn run() -> Result<(), String> {
    let builder = tauri::Builder::default()
        // First, so that a second start only focuses this window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::focus_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| setup(app))
        .on_window_event(window::on_window_event);
    #[cfg(feature = "sim")]
    let builder =
        builder.invoke_handler(handlers![commands::dev_sim_action, commands::dev_sim_info]);
    #[cfg(not(feature = "sim"))]
    let builder = builder.invoke_handler(handlers![]);
    let app = builder
        .build(tauri::generate_context!())
        .map_err(|e| format!("cannot start OpenWraith: {e}"))?;
    app.run(window::on_run_event);
    Ok(())
}
