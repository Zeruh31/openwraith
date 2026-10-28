//! Windows notifications, shown from Rust (`tauri-plugin-notification`;
//! no webview permission). Design B5.
//!
//! The core decides which notices become a notification (the user's
//! settings, at most one per kind every 10 minutes) and hands them to the
//! sink here; this module only puts them into words and shows them. A click
//! on a notification focuses the main window and goes to the notification's
//! route (`Notification::on_action`).
//!
//! Notifications are a convenience: Windows holds them back under Do not
//! disturb, and an installed app needs its Start menu shortcut with the
//! app's AppUserModelID (the installers make it); a development build shows
//! them under PowerShell's name. The banners in the window and the tray
//! icon stay what the user relies on.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use openwraith_app_core::shell::OsNotification;
use openwraith_app_core::{NotificationSink, WorkerHandle};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::Notification;

use crate::texts::{self, Lang};
use crate::window;

/// How many shown notifications keep their route for a click.
const ROUTES_KEPT: usize = 32;

/// The routes of the notifications shown last, by notification id.
#[derive(Debug, Default)]
pub struct RouteBook {
    next: i32,
    entries: VecDeque<(i32, Option<String>)>,
}

impl RouteBook {
    /// Remembers `route` for a new notification and returns its id
    /// (positive, unique among the kept ones).
    pub fn add(&mut self, route: Option<String>) -> i32 {
        self.next = if self.next >= i32::MAX - 1 {
            1
        } else {
            self.next + 1
        };
        let id = self.next;
        self.entries.retain(|(kept, _)| *kept != id);
        self.entries.push_back((id, route));
        while self.entries.len() > ROUTES_KEPT {
            self.entries.pop_front();
        }
        id
    }

    /// The route of notification `id`: `None` when unknown, `Some(None)`
    /// when it has none.
    pub fn route(&self, id: i32) -> Option<Option<String>> {
        self.entries
            .iter()
            .find(|(kept, _)| *kept == id)
            .map(|(_, route)| route.clone())
    }
}

/// User text for a notification: no control characters, at most 64
/// characters.
fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect::<String>()
        .trim()
        .to_string()
}

/// The title and the body of a core notification in `lang`: the values
/// filled in, a missing game name and an unknown firmware version in words,
/// a wait reason as its sentence.
pub fn compose(lang: Lang, notification: &OsNotification) -> (String, String) {
    let mut params: BTreeMap<String, String> = notification
        .params
        .iter()
        .map(|(k, v)| (k.clone(), clean(v)))
        .collect();
    let label_missing = params.get("label").is_none_or(String::is_empty);
    if label_missing {
        params.insert(
            "label".to_string(),
            texts::text(lang, "notifications.aGame"),
        );
    }
    for version in ["known", "current"] {
        if params.get(version).is_some_and(String::is_empty) {
            params.insert(
                version.to_string(),
                texts::text(lang, "notifications.versionUnknown"),
            );
        }
    }
    if let Some(reason) = params.get("reason").cloned() {
        let key = format!("notifications.reasons.{reason}");
        let sentence = texts::get(lang, &key)
            .map(str::to_string)
            .unwrap_or_else(|| texts::text(lang, "notifications.reasons.other"));
        params.insert("reason".to_string(), sentence);
    }
    let title = texts::render(lang, &format!("{}.title", notification.key), &params);
    let body = texts::render(lang, &format!("{}.body", notification.key), &params);
    (title, body)
}

/// Shows notifications (managed as `Arc<Notifier<R>>`).
pub struct Notifier<R: Runtime> {
    app: AppHandle<R>,
    routes: Mutex<RouteBook>,
}

impl<R: Runtime> Notifier<R> {
    /// Shows `title` and `body`; a click leads to `route`. Best effort.
    fn show(&self, title: String, body: String, route: Option<String>) {
        let Some(plugin) = self.app.try_state::<Notification<R>>() else {
            return;
        };
        let id = self
            .routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .add(route);
        let _ = plugin.builder().id(id).title(title).body(body).show();
    }

    /// A click on notification `id` (a background thread).
    fn clicked(&self, id: i32) {
        let route = self
            .routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .route(id);
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || match route.flatten() {
            Some(route) => window::open_route(&app, &route),
            None => window::focus_main(&app),
        });
    }
}

/// The core's notification sink.
struct Sink<R: Runtime>(Arc<Notifier<R>>);

impl<R: Runtime> NotificationSink for Sink<R> {
    fn show(&self, notification: &OsNotification) {
        let (title, body) = compose(texts::current(&self.0.app), notification);
        self.0.show(title, body, Some(notification.route.clone()));
    }
}

/// Shows one of the glue's own notifications (`<key>.title`,
/// `<key>.body`); a click opens the window (at `route`, if given).
pub fn show_text<R: Runtime>(app: &AppHandle<R>, key: &str, route: Option<&str>) {
    let Some(notifier) = app
        .try_state::<Arc<Notifier<R>>>()
        .map(|state| Arc::clone(state.inner()))
    else {
        return;
    };
    let lang = texts::current(app);
    notifier.show(
        texts::text(lang, &format!("{key}.title")),
        texts::text(lang, &format!("{key}.body")),
        route.map(str::to_string),
    );
}

/// Sets the notifications up in `setup`: the click handler and the core's
/// sink.
pub fn install<R: Runtime>(app: &AppHandle<R>, worker: &WorkerHandle) {
    let notifier = Arc::new(Notifier {
        app: app.clone(),
        routes: Mutex::new(RouteBook::default()),
    });
    app.manage(Arc::clone(&notifier));
    if let Some(plugin) = app.try_state::<Notification<R>>() {
        let on_click = Arc::clone(&notifier);
        let _ = plugin.on_action(move |performed| {
            if performed.action_id() != "tap" {
                return;
            }
            match performed.notification() {
                Some(shown) => on_click.clicked(shown.id()),
                // Unreadable: at least the window comes to the front.
                None => on_click.clicked(0),
            }
        });
    }
    worker.set_notification_sink(Arc::new(Sink(notifier)));
}

#[cfg(test)]
mod tests {
    use openwraith_app_core::shell::NotificationKind;

    use super::*;

    fn note(key: &str, params: &[(&str, &str)]) -> OsNotification {
        OsNotification {
            kind: NotificationKind::AutoSwitched,
            key: key.to_string(),
            params: params
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            route: "#/settings?tab=games".to_string(),
        }
    }

    #[test]
    fn every_core_notification_is_put_into_words() {
        // The parameters the core sends with each key (core auto/notify.rs).
        let cases = [
            note("notifications.keyboardReset", &[]),
            note("notifications.resetSuspected", &[("profile", "2")]),
            note(
                "notifications.firmwareChanged",
                &[("known", "3.22.07"), ("current", "")],
            ),
            note("notifications.autoBackupFailed", &[("profile", "1")]),
            note(
                "notifications.autoSwitched",
                &[("profile", "3"), ("label", "CS2")],
            ),
            note("notifications.autoSwitchedBack", &[("profile", "1")]),
            note("notifications.autoSwitchWaiting", &[("profile", "2")]),
            note(
                "notifications.autoSwitchSkipped",
                &[
                    ("profile", "2"),
                    ("label", "VALORANT"),
                    ("reason", "other_program"),
                ],
            ),
            note("notifications.autoPausedRateLimit", &[]),
            note("notifications.autoPausedFailed", &[]),
        ];
        // Every key the core sends has a case here.
        for key in openwraith_app_core::shell::NOTIFICATION_KEYS {
            assert!(cases.iter().any(|c| c.key == *key), "no case for {key}");
        }
        for lang in [Lang::En, Lang::Tr] {
            for case in &cases {
                let (title, body) = compose(lang, case);
                for text in [&title, &body] {
                    assert!(!text.contains('{'), "{}: unfilled {text}", case.key);
                    assert!(!text.starts_with("notifications."), "{}: no text", case.key);
                }
            }
        }
        let (title, body) = compose(Lang::En, &cases[4]);
        assert_eq!(title, "Profile 3 is active");
        assert_eq!(
            body,
            "CS2 is in front, so OpenWraith switched to its profile."
        );
        let (_, body) = compose(Lang::Tr, &cases[2]);
        assert!(
            body.contains("3.22.07") && body.contains("bilinmiyor"),
            "{body}"
        );
        let (_, body) = compose(Lang::En, &cases[6]);
        assert!(body.starts_with("A game uses profile 2"), "{body}");
        let (_, body) = compose(Lang::En, &cases[7]);
        assert!(
            body.ends_with("30 seconds: another program was using the keyboard."),
            "{body}"
        );
    }

    #[test]
    fn unknown_reasons_and_odd_labels_stay_readable() {
        let skipped = note(
            "notifications.autoSwitchSkipped",
            &[
                ("profile", "4"),
                ("label", "  Game\u{7}\n"),
                ("reason", "new_reason"),
            ],
        );
        let (_, body) = compose(Lang::Tr, &skipped);
        assert!(body.starts_with("Game Profil 4"), "{body}");
        assert!(body.ends_with("klavye hazır değildi."), "{body}");
        let long = "x".repeat(200);
        let switched = note(
            "notifications.autoSwitched",
            &[("profile", "1"), ("label", &long)],
        );
        let (_, body) = compose(Lang::En, &switched);
        assert!(body.starts_with(&"x".repeat(64)) && !body.contains(&"x".repeat(65)));
    }

    #[test]
    fn the_route_book_keeps_the_last_routes() {
        let mut book = RouteBook::default();
        let first = book.add(Some("#/device".to_string()));
        let none = book.add(None);
        assert!(first > 0 && none > first);
        assert_eq!(book.route(first), Some(Some("#/device".to_string())));
        assert_eq!(book.route(none), Some(None));
        assert_eq!(book.route(9999), None);
        for _ in 0..ROUTES_KEPT {
            book.add(Some("#/backup".to_string()));
        }
        assert_eq!(book.route(first), None, "the oldest route is forgotten");
        assert_eq!(book.entries.len(), ROUTES_KEPT);
        // The ids wrap without reaching zero or a negative number.
        book.next = i32::MAX - 1;
        assert_eq!(book.add(None), 1);
    }
}
