//! The OpenWraith desktop app.

// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = openwraith_app_lib::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
