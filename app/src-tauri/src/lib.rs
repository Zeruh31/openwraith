//! The OpenWraith desktop app: the Tauri glue around
//! [`openwraith_app_core`] (commands, channels, dialogs, window lifecycle,
//! and the background agent of design B5/B7: the tray icon, close to the
//! notification area, start with Windows, notifications, the foreground
//! feed of the per-game profiles).
//!
//! A release build talks to a real keyboard through hidapi; it opens a
//! connected keyboard at start (listening for 2 s before the first request).
//! A debug build with the feature `sim` uses the developer simulator instead
//! and contains no HID code; a release build with it does not compile.
//!
//! A second start (also the Run entry's `--tray` start) only brings the
//! running window to the front: the single-instance plugin ends it before
//! `setup`, so it never opens the keyboard.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

#[cfg(all(feature = "sim", not(debug_assertions)))]
compile_error!("the simulator must never be part of a release build");

mod autostart;
/// The names of the IPC commands (the single source).
pub mod command_names;
mod commands;
mod dialogs;
mod foreground;
mod notify;
mod paths;
mod shell;
mod sinks;
mod texts;
mod tray;
mod window;

use std::sync::atomic::AtomicBool;
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
            commands::list_auto_backups,
            commands::pin_auto_backup,
            commands::get_recovery,
            commands::keep_current_settings,
            commands::prepare_recovery,
            commands::get_background_settings,
            commands::set_background_settings,
            commands::get_profile_names,
            commands::set_profile_names,
            commands::get_auto_profile_state,
            commands::set_auto_profile_rules,
            commands::pause_auto_profile,
            commands::list_running_programs,
            commands::pick_game_program,
            commands::detect_next_program,
            commands::report_ui_state,
            $($extra),*
        ]
    };
}

/// Starts the device worker, loads the settings and sets up the background
/// agent. The window is created hidden (`tauri.conf.json`) and shown here,
/// unless the Run entry started the app (`--tray`) with the tray icon on.
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
    let worker = Arc::new(worker);
    app.manage(Arc::clone(&worker));

    let settings_path = paths::settings_path(&handle)?;
    let settings = openwraith_app_core::load_settings(&settings_path);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_theme(commands::native_theme(settings.theme));
    }
    app.manage(SettingsState {
        path: settings_path,
        value: Mutex::new(settings),
    });

    let background = worker.background_settings();
    app.manage(window::CloseExplained(AtomicBool::new(
        background.close_to_tray_explained,
    )));
    notify::install(&handle, &worker);
    // Without a tray icon the window must show (it is the only way in).
    let tray_shown = tray::install(&handle, &worker, background.tray_enabled);
    worker.set_shell_hook(Arc::new(shell::Shell(handle.clone())));
    worker.set_program_source(Arc::new(foreground::Programs));
    if let Ok(feed) = foreground::start(Arc::clone(&worker)) {
        app.manage(feed);
    }
    let wanted = background.tray_enabled && background.start_with_windows;
    match autostart::reconcile_when(std::env::args(), wanted) {
        autostart::ReconcileWhen::Never => {}
        autostart::ReconcileWhen::Now => autostart::reconcile(&handle, wanted),
        autostart::ReconcileWhen::Later => {
            // Started from the Run entry at logon: no write to the Run key
            // while Windows processes it; the settings are read again then
            // (the user may have turned the option on meanwhile).
            let later = handle.clone();
            let settings = Arc::clone(&worker);
            let _ = std::thread::Builder::new()
                .name("openwraith-autostart-check".to_string())
                .spawn(move || {
                    std::thread::sleep(autostart::LOGON_DELAY);
                    let now = settings.background_settings();
                    autostart::reconcile(&later, now.tray_enabled && now.start_with_windows);
                });
        }
    }

    if !window::starts_hidden(std::env::args(), tray_shown) {
        window::focus_main(&handle);
    }
    Ok(())
}

/// Runs the app until it exits.
pub fn run() -> Result<(), String> {
    let builder = tauri::Builder::default()
        // First, so that a second start (also with `--tray`) only focuses
        // this window and ends before its own setup.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::focus_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init());
    #[cfg(not(feature = "sim"))]
    let builder = builder.plugin(autostart::plugin());
    let builder = builder
        .setup(setup)
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
