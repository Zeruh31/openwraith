//! The Tauri-free core of the OpenWraith desktop app.
//!
//! * [`WorkerHandle`]: one device worker thread owns the HID session (or the
//!   developer simulator) and runs every operation; commands send it requests
//!   and wait for the reply, long operations stream [`dto::ProgressEvent`]s.
//! * [`dto`]: the IPC data types (TypeScript bindings are generated from them
//!   by the tests).
//! * The connection state machine: listen-only observation before the first
//!   request, the "other program" state, hot-plug, reset handling.
//! * Every safety rule of the app: writes only through the library's entry
//!   points with `ConsentOrigin::Application` after an explicit confirmation
//!   (single-use plan tokens), a verified safety backup before every setting
//!   write, the drift check, the operation journal, close deferral.
//!
//! Nothing here names a Tauri type; the app crate implements the sinks
//! ([`EventSink`], [`ProgressSink`], [`ExitHook`]) over Tauri channels.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

#[cfg(all(feature = "sim", not(debug_assertions)))]
compile_error!("the simulator must never be part of a release build");

mod backend;
mod clock;
mod config;
pub mod dto;
mod files;
mod handle;
mod map;
mod ops;
mod settings;
mod shared;
mod sink;
mod tokens;
mod worker;

#[cfg(test)]
mod tests;

pub use config::{CoreConfig, TESTED_FIRMWARE};
pub use handle::WorkerHandle;
pub use settings::{load_settings, save_settings};
pub use sink::{EventSink, ExitHook, ProgressSink};

#[cfg(any(test, feature = "sim"))]
pub use backend::sim::{Pacing, SimOptions};
