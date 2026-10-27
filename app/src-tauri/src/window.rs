//! Window and app lifecycle: the close guard while an operation runs, the
//! focus check, single instance, shutdown.

use std::sync::Arc;

use openwraith_app_core::WorkerHandle;
use tauri::{AppHandle, Manager, RunEvent, Runtime, Window, WindowEvent};

/// Brings the main window to the front (a second instance was started).
pub fn focus_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn worker<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<WorkerHandle>> {
    app.try_state::<Arc<WorkerHandle>>()
        .map(|state| Arc::clone(state.inner()))
}

/// Window events: a close while an operation runs is deferred (the frontend
/// shows the close guard); focus triggers the active-profile check.
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let Some(worker) = worker(window.app_handle()) else {
        return;
    };
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            if worker.close_requested() {
                api.prevent_close();
            }
        }
        WindowEvent::Focused(true) => worker.window_focused(),
        _ => {}
    }
}

/// App events: an exit asked for by the user while an operation runs is
/// deferred like a close; on exit the worker stops and the keyboard is
/// closed.
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent) {
    match event {
        RunEvent::ExitRequested { code, api, .. } => {
            // `app.exit` (after "stop safely and close") passes a code.
            if code.is_none()
                && let Some(worker) = worker(app)
                && worker.close_requested()
            {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            if let Some(worker) = worker(app) {
                worker.shutdown();
            }
        }
        _ => {}
    }
}
