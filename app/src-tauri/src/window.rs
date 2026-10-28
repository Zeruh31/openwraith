//! Window and app lifecycle: the close guard while an operation runs, close
//! to the notification area (design B5), Quit, the focus check, single
//! instance, routes from the tray and from notifications, shutdown.
//!
//! * The tray icon on: closing the window hides it (`prevent_close` +
//!   hide). Hiding stops a live key-depth view, never a write: an operation
//!   keeps running. The first time, a notification explains where the app
//!   went (`close_to_tray_explained`).
//! * The tray icon off: closing the window quits, with the close guard
//!   while an operation that may write runs (as before).
//! * Quit (the tray menu) while such an operation runs shows the window
//!   with the same close guard.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use openwraith_app_core::WorkerHandle;
use openwraith_app_core::dto::DepthStopRequest;
use tauri::{AppHandle, Manager, RunEvent, Runtime, Url, Window, WindowEvent};

use crate::{foreground, notify, tray};

/// The flag of the Run entry: start hidden in the notification area.
pub const TRAY_FLAG: &str = "--tray";

/// Whether this start stays hidden: started by the Run entry (`--tray`)
/// and the tray icon is on (without it the window could not be reached).
pub fn starts_hidden<S: AsRef<str>>(args: impl IntoIterator<Item = S>, tray_enabled: bool) -> bool {
    tray_enabled && args.into_iter().any(|a| a.as_ref() == TRAY_FLAG)
}

/// Whether the first close to the tray was explained (cached from the
/// settings at start, so that a close does no file I/O).
pub struct CloseExplained(pub AtomicBool);

/// Brings the main window to the front (a second instance was started, the
/// tray icon or a notification was clicked).
pub fn focus_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Shows the main window if it is hidden (the tray icon went away).
pub fn show_if_hidden<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main")
        && !window.is_visible().unwrap_or(true)
    {
        let _ = window.show();
    }
}

/// Whether `route` is a route of the app (`#/backup?recover=1`): a hash
/// path of letters, digits and `/ ? = & _ -` only, so that it can never be
/// anything but a string in the page.
pub fn valid_route(route: &str) -> bool {
    route.len() <= 128
        && route.starts_with("#/")
        && route
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"#/?=&_-".contains(&b))
}

/// Whether the page shows the app's own frontend (the production origin,
/// or the development server in debug builds), so that a route is only ever
/// set in our page.
pub fn app_origin(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    match url.scheme() {
        "tauri" => host == "localhost",
        "http" | "https" => {
            host == "tauri.localhost" || (cfg!(debug_assertions) && host == "localhost")
        }
        _ => false,
    }
}

/// Focuses the main window and goes to `route` (the frontend follows the
/// location hash), then tells the page (`openwraith:navigate`, see
/// `app/src/shell/routeEvents.ts`) so that the keyboard focus moves to the
/// opened page. Main thread.
pub fn open_route<R: Runtime>(app: &AppHandle<R>, route: &str) {
    focus_main(app);
    if !valid_route(route) {
        return;
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if !window.url().is_ok_and(|url| app_origin(&url)) {
        return;
    }
    // A JSON string literal: the route is a value, never code.
    let Ok(quoted) = serde_json::to_string(route) else {
        return;
    };
    let _ = window.eval(route_script(&quoted));
}

/// The script `open_route` runs: sets the hash (when it differs), then
/// dispatches the route event the frontend answers by focusing the page.
/// `quoted` is the route as a JSON string literal (a value, never code).
fn route_script(quoted: &str) -> String {
    format!(
        "if (window.location.hash !== {quoted}) {{ window.location.hash = {quoted}; }} \
         window.dispatchEvent(new CustomEvent(\"openwraith:navigate\", {{ detail: {quoted} }}));"
    )
}

fn worker<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<WorkerHandle>> {
    app.try_state::<Arc<WorkerHandle>>()
        .map(|state| Arc::clone(state.inner()))
}

/// Quit (the tray menu): exits, unless an operation that may write runs;
/// then the window shows the close guard ("stop safely and close").
pub fn quit<R: Runtime>(app: &AppHandle<R>) {
    match worker(app) {
        Some(worker) if worker.close_requested() => focus_main(app),
        _ => app.exit(0),
    }
}

/// Hides the window into the notification area: a live view stops (never
/// a write), and the first time a notification explains it.
fn hide_to_tray<R: Runtime>(window: &Window<R>, worker: &WorkerHandle) {
    let _ = window.hide();
    worker.stop_depth_view(DepthStopRequest::WindowHidden);
    let app = window.app_handle();
    let Some(explained) = app.try_state::<CloseExplained>() else {
        return;
    };
    if explained.0.swap(true, Ordering::SeqCst) {
        return;
    }
    notify::show_text(app, "tray.closeExplained", None);
    if let Some(worker) = self::worker(app) {
        // A small file write: off the main thread.
        tauri::async_runtime::spawn_blocking(move || {
            let _ = worker.mark_close_to_tray_explained();
        });
    }
}

/// Window events: a close goes to the notification area while the tray
/// icon is shown, else it is deferred while an operation runs (the
/// frontend shows the close guard); focus triggers the active-profile
/// check.
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let Some(worker) = worker(window.app_handle()) else {
        return;
    };
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            if tray::is_shown(window.app_handle()) {
                api.prevent_close();
                hide_to_tray(window, &worker);
            } else if worker.close_requested() {
                api.prevent_close();
            }
        }
        WindowEvent::Focused(true) => worker.window_focused(),
        _ => {}
    }
}

/// App events: an exit asked for by the user while an operation runs is
/// deferred like a close; on exit the platform feed and the worker stop and
/// the keyboard is closed.
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent) {
    match event {
        RunEvent::ExitRequested { code, api, .. } => {
            // `app.exit` (Quit, "stop safely and close") passes a code.
            if code.is_none()
                && let Some(worker) = worker(app)
                && worker.close_requested()
            {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            if let Some(feed) = app.try_state::<foreground::Feed>() {
                feed.stop();
            }
            if let Some(worker) = worker(app) {
                worker.shutdown();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_tray_start_with_the_tray_on_stays_hidden() {
        assert!(starts_hidden(["openwraith-app.exe", "--tray"], true));
        assert!(
            !starts_hidden(["openwraith-app.exe", "--tray"], false),
            "without the tray icon the window must show"
        );
        assert!(!starts_hidden(["openwraith-app.exe"], true));
        assert!(!starts_hidden(
            ["openwraith-app.exe", "--tray=1", "-tray"],
            true
        ));
        assert!(!starts_hidden(Vec::<String>::new(), true));
    }

    #[test]
    fn the_route_script_sets_the_hash_then_asks_the_page_to_focus() {
        let quoted = serde_json::to_string("#/backup?recover=1").unwrap();
        let script = route_script(&quoted);
        assert_eq!(
            script,
            "if (window.location.hash !== \"#/backup?recover=1\") { window.location.hash = \"#/backup?recover=1\"; } \
             window.dispatchEvent(new CustomEvent(\"openwraith:navigate\", { detail: \"#/backup?recover=1\" }));"
        );
    }

    #[test]
    fn only_app_routes_are_set() {
        for good in [
            "#/backup?recover=1",
            "#/settings?tab=games",
            "#/device",
            "#/backup",
            "#/actuation?tab=depth",
        ] {
            assert!(valid_route(good), "{good}");
        }
        for bad in [
            "",
            "#",
            "/backup",
            "javascript:alert(1)",
            "#/x\";alert(1);//",
            "#/a b",
            "#/a'b",
            "#/a\\b",
            "#/ü",
        ] {
            assert!(!valid_route(bad), "{bad}");
        }
        assert!(!valid_route(&format!("#/{}", "a".repeat(200))));
    }

    #[test]
    fn routes_go_only_to_the_apps_own_page() {
        let ok = |s: &str| Url::parse(s).is_ok_and(|u| app_origin(&u));
        assert!(ok("http://tauri.localhost/"));
        assert!(ok("https://tauri.localhost/index.html#/device"));
        assert!(ok("tauri://localhost/"));
        assert!(!ok("https://wraith.software/"));
        assert!(!ok("http://tauri.localhost.example.com/"));
        assert!(!ok("file:///C:/x.html"));
        assert!(!ok("about:blank"));
        // The development server only in debug builds.
        assert_eq!(ok("http://localhost:1420/"), cfg!(debug_assertions));
    }
}
