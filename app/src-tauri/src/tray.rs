//! The opt-in notification-area icon (design B5): the core's
//! [`TrayModel`] as a menu, the icon's three looks (normal, disconnected,
//! attention) and the clicks.
//!
//! * The menu: header, Profile 1-4, Pause game profiles, Back up now, the
//!   attention items, Open OpenWraith, Quit. A click on a core item calls
//!   [`WorkerHandle::tray_action`] on a blocking thread (a profile switch and
//!   "Back up now" are operations); when the core says the window is needed
//!   (drafts, a pending plan, blocked writes, no vendor-site acknowledgement
//!   yet), the window opens at the route it names.
//! * A left click on the icon opens the window; the right click shows the
//!   menu.
//! * The core calls [`TraySink::update`] from any thread; the icon and the
//!   menu change only on the main thread (posted without waiting, so the
//!   worker never blocks on the user interface).
//!
//! The pure part (the menu entries, the ids, the tooltip, the icon's
//! pixels) is unit-tested; nothing in the tests creates a real icon.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use openwraith_app_core::shell::{
    TrayAction, TrayAttention, TrayHeader, TrayModel, TrayOutcome, TrayState,
};
use openwraith_app_core::{TraySink, WorkerHandle};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::texts::{self, Lang};
use crate::window;

/// The id of the one tray icon.
const TRAY_ID: &str = "openwraith";

/// Windows shows at most 127 characters of a tray tooltip.
const TOOLTIP_MAX: usize = 127;

// ----- the pure part: the menu model -----

/// What a menu entry does when clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    /// A click the core handles.
    Action(TrayAction),
    /// "Open OpenWraith".
    Open,
    /// "Quit".
    Quit,
}

/// How a menu entry looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A plain item.
    Text,
    /// An item with a check mark (`true` = checked).
    Check(bool),
    /// A separator line.
    Separator,
}

/// One entry of the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its id (see [`parse`]); empty for separators.
    pub id: String,
    /// Its text as shown (a `&` of user text doubled, see [`menu_text`]).
    pub text: String,
    /// It can be clicked.
    pub enabled: bool,
    /// How it looks.
    pub kind: EntryKind,
}

impl Entry {
    fn text(id: &str, text: String, enabled: bool) -> Self {
        Entry {
            id: id.to_string(),
            text,
            enabled,
            kind: EntryKind::Text,
        }
    }

    fn check(id: String, text: String, enabled: bool, checked: bool) -> Self {
        Entry {
            id,
            text,
            enabled,
            kind: EntryKind::Check(checked),
        }
    }

    fn separator() -> Self {
        Entry {
            id: String::new(),
            text: String::new(),
            enabled: false,
            kind: EntryKind::Separator,
        }
    }
}

/// The id of an attention item.
fn attention_id(item: TrayAttention) -> &'static str {
    match item {
        TrayAttention::ResetReview => "attention-reset",
        TrayAttention::OtherProgramCheck => "attention-other-program",
        TrayAttention::TravelStop => "attention-travel",
        TrayAttention::AutoPaused => "attention-auto-paused",
    }
}

/// The text key of an attention item.
fn attention_key(item: TrayAttention) -> &'static str {
    match item {
        TrayAttention::ResetReview => "tray.attention.reset",
        TrayAttention::OtherProgramCheck => "tray.attention.otherProgram",
        TrayAttention::TravelStop => "tray.attention.travel",
        TrayAttention::AutoPaused => "tray.attention.autoPaused",
    }
}

const ATTENTION: [TrayAttention; 4] = [
    TrayAttention::ResetReview,
    TrayAttention::OtherProgramCheck,
    TrayAttention::TravelStop,
    TrayAttention::AutoPaused,
];

/// What the menu entry `id` does; `None` for the header, separators and
/// unknown ids.
pub fn parse(id: &str) -> Option<MenuCommand> {
    match id {
        "open" => return Some(MenuCommand::Open),
        "quit" => return Some(MenuCommand::Quit),
        "back-up-now" => return Some(MenuCommand::Action(TrayAction::BackUpNow)),
        // The id says what the click wants (a check item flips its own mark
        // before the click arrives, so the mark cannot be trusted).
        "games-pause" => return Some(MenuCommand::Action(TrayAction::SetAutoPaused(true))),
        "games-resume" => return Some(MenuCommand::Action(TrayAction::SetAutoPaused(false))),
        _ => {}
    }
    if let Some(n) = id.strip_prefix("profile-") {
        return match n.parse::<u8>() {
            Ok(p @ 1..=4) => Some(MenuCommand::Action(TrayAction::SwitchProfile(p))),
            _ => None,
        };
    }
    ATTENTION
        .into_iter()
        .find(|item| attention_id(*item) == id)
        .map(|item| MenuCommand::Action(TrayAction::Attention(item)))
}

/// User text (a profile name, the model) for a Windows menu: `&` marks a
/// mnemonic there, so a literal `&` is doubled; control characters go.
pub fn menu_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .replace('&', "&&")
}

fn profile_label(lang: Lang, profile: u8, name: Option<&str>) -> String {
    let mut params = std::collections::BTreeMap::new();
    params.insert("profile".to_string(), profile.to_string());
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => {
            params.insert("name".to_string(), name.to_string());
            texts::render(lang, "tray.profileNamed", &params)
        }
        None => texts::render(lang, "tray.profile", &params),
    }
}

/// The first line: "OpenWraith · Wraith W75 · Profile 2 (FPS)" or
/// "OpenWraith · Keyboard not connected" (plain text, `&` not doubled).
pub fn header_text(header: &TrayHeader, lang: Lang) -> String {
    let mut parts = vec![texts::text(lang, "tray.appName")];
    match header {
        TrayHeader::Connected {
            model,
            profile,
            name,
        } => {
            let model = model.as_deref().map(str::trim).filter(|m| !m.is_empty());
            if let Some(model) = model {
                parts.push(model.to_string());
            }
            if let Some(profile) = profile {
                parts.push(profile_label(lang, *profile, name.as_deref()));
            }
            if parts.len() == 1 {
                parts.push(texts::text(lang, "tray.connected"));
            }
        }
        TrayHeader::NotConnected => parts.push(texts::text(lang, "tray.notConnected")),
    }
    parts.join(" · ")
}

/// The menu of `model` in `lang`.
pub fn entries(model: &TrayModel, lang: Lang) -> Vec<Entry> {
    let mut out = vec![
        Entry::text(
            "header",
            menu_text(&header_text(&model.header, lang)),
            false,
        ),
        Entry::separator(),
    ];
    for item in &model.profiles {
        out.push(Entry::check(
            format!("profile-{}", item.profile),
            menu_text(&profile_label(lang, item.profile, item.name.as_deref())),
            item.enabled,
            item.checked,
        ));
    }
    out.push(Entry::separator());
    if let Some(paused) = model.auto_paused {
        let id = if paused {
            "games-resume"
        } else {
            "games-pause"
        };
        out.push(Entry::check(
            id.to_string(),
            texts::text(lang, "tray.pauseGames"),
            true,
            paused,
        ));
    }
    out.push(Entry::text(
        "back-up-now",
        texts::text(lang, "tray.backUpNow"),
        model.back_up_now,
    ));
    if !model.attention.is_empty() {
        out.push(Entry::separator());
        for item in &model.attention {
            out.push(Entry::text(
                attention_id(*item),
                texts::text(lang, attention_key(*item)),
                true,
            ));
        }
    }
    out.push(Entry::separator());
    out.push(Entry::text("open", texts::text(lang, "tray.open"), true));
    out.push(Entry::text("quit", texts::text(lang, "tray.quit"), true));
    out
}

/// At most `max` characters (an ellipsis replaces the rest).
fn shorten(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        return text;
    }
    let mut short: String = text.chars().take(max.saturating_sub(1)).collect();
    short.push('…');
    short
}

/// The icon's tooltip: the header, and the attention in words (the icon's
/// colour is not the only sign).
pub fn tooltip(model: &TrayModel, lang: Lang) -> String {
    let header = header_text(&model.header, lang);
    let text = if model.state == TrayState::Attention {
        let mut params = std::collections::BTreeMap::new();
        params.insert("status".to_string(), header);
        texts::render(lang, "tray.needsAttention", &params)
    } else {
        header
    };
    shorten(text, TOOLTIP_MAX)
}

// ----- the pure part: the icon's pixels (RGBA) -----

/// The disconnected look: grey, half transparent.
pub fn dimmed(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for px in out.as_chunks_mut::<4>().0 {
        let grey = (u32::from(px[0]) * 30 + u32::from(px[1]) * 59 + u32::from(px[2]) * 11) / 100;
        let grey = u8::try_from(grey).unwrap_or(u8::MAX);
        px[0] = grey;
        px[1] = grey;
        px[2] = grey;
        px[3] = u8::try_from(u32::from(px[3]) * 55 / 100).unwrap_or(u8::MAX);
    }
    out
}

/// The attention look: an amber dot with a dark ring in the lower right
/// corner (a third of the icon wide).
pub fn with_badge(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    const AMBER: [u8; 4] = [0xF5, 0x9E, 0x0B, 0xFF];
    const RING: [u8; 4] = [0x1F, 0x1F, 0x1F, 0xFF];
    let mut out = rgba.to_vec();
    let size = width.min(height);
    if size < 4 || out.len() < (width as usize) * (height as usize) * 4 {
        return out;
    }
    // In pixels times two, so that the centre can sit between pixels.
    let radius2 = i64::from(size) / 3;
    let ring2 = (radius2 * 3 / 4).max(1);
    let cx2 = i64::from(width) * 2 - radius2 - 1;
    let cy2 = i64::from(height) * 2 - radius2 - 1;
    for y in 0..height {
        for x in 0..width {
            let dx = i64::from(x) * 2 + 1 - cx2;
            let dy = i64::from(y) * 2 + 1 - cy2;
            let d2 = dx * dx + dy * dy;
            let colour = if d2 <= ring2 * ring2 {
                AMBER
            } else if d2 <= radius2 * radius2 {
                RING
            } else {
                continue;
            };
            let at = ((y as usize) * (width as usize) + x as usize) * 4;
            out[at..at + 4].copy_from_slice(&colour);
        }
    }
    out
}

/// A plain square for builds without a window icon.
fn fallback_rgba(size: u32) -> Vec<u8> {
    [0x3B, 0x82, 0xF6, 0xFF].repeat((size * size) as usize)
}

// ----- the Tauri part -----

/// The three looks of the icon.
struct Icons {
    normal: Image<'static>,
    disconnected: Image<'static>,
    attention: Image<'static>,
}

impl Icons {
    fn new(base: Option<&Image<'_>>) -> Self {
        let (rgba, width, height) = match base {
            Some(image) if image.width() > 0 && image.height() > 0 => {
                (image.rgba().to_vec(), image.width(), image.height())
            }
            _ => (fallback_rgba(32), 32, 32),
        };
        Icons {
            disconnected: Image::new_owned(dimmed(&rgba), width, height),
            attention: Image::new_owned(with_badge(&rgba, width, height), width, height),
            normal: Image::new_owned(rgba, width, height),
        }
    }

    fn get(&self, state: TrayState) -> Image<'static> {
        match state {
            TrayState::Normal => self.normal.clone(),
            TrayState::Disconnected => self.disconnected.clone(),
            TrayState::Attention => self.attention.clone(),
        }
    }
}

/// What the icon shows now.
#[derive(Default)]
struct Inner {
    /// The core's model, as received last.
    model: Option<TrayModel>,
    /// The icon is wanted (the setting is on).
    shown: bool,
    /// What the icon shows: the language, the entries, the tooltip, the
    /// look.
    rendered: Option<(Lang, Vec<Entry>, String, TrayState)>,
}

/// The notification-area icon (managed as `Arc<Tray<R>>`).
pub struct Tray<R: Runtime> {
    app: AppHandle<R>,
    icons: Icons,
    inner: Mutex<Inner>,
}

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The core's tray sink: keeps the model and posts a redraw to the main
/// thread.
struct Sink<R: Runtime>(Arc<Tray<R>>);

impl<R: Runtime> TraySink for Sink<R> {
    fn update(&self, model: &TrayModel) {
        lock(&self.0.inner).model = Some(model.clone());
        self.0.post_render(false);
    }
}

impl<R: Runtime> Tray<R> {
    /// Whether the icon is shown (close-to-tray applies only then).
    pub fn is_shown(&self) -> bool {
        // Our lock is released before Tauri's tray list is asked.
        let wanted = lock(&self.inner).shown;
        wanted && self.app.tray_by_id(TRAY_ID).is_some()
    }

    /// Redraws on the main thread (without waiting).
    pub fn post_render(self: &Arc<Self>, force: bool) {
        let tray = Arc::clone(self);
        let _ = self.app.run_on_main_thread(move || tray.render(force));
    }

    /// Shows the model on the icon. Main thread only.
    fn render(&self, force: bool) {
        let Some(icon) = self.app.tray_by_id(TRAY_ID) else {
            return;
        };
        let lang = texts::current(&self.app);
        let (entries, tip, state, look_changed) = {
            let mut inner = lock(&self.inner);
            let Some(model) = inner.model.clone() else {
                return;
            };
            let entries = entries(&model, lang);
            let tip = tooltip(&model, lang);
            let look_changed = inner
                .rendered
                .as_ref()
                .is_none_or(|(_, _, _, state)| *state != model.state);
            let same = inner.rendered.as_ref().is_some_and(|(l, e, t, s)| {
                *l == lang && *e == entries && *t == tip && *s == model.state
            });
            if same && !force {
                return;
            }
            inner.rendered = Some((lang, entries.clone(), tip.clone(), model.state));
            (entries, tip, model.state, look_changed || force)
        };
        if let Ok(menu) = build_menu(&self.app, &entries) {
            let _ = icon.set_menu(Some(menu));
        }
        let _ = icon.set_tooltip(Some(tip));
        if look_changed {
            let _ = icon.set_icon(Some(self.icons.get(state)));
        }
    }

    /// Shows or removes the icon. Main thread only.
    fn set_shown(self: &Arc<Self>, shown: bool) -> Result<(), String> {
        lock(&self.inner).shown = shown;
        if !shown {
            lock(&self.inner).rendered = None;
            drop(self.app.remove_tray_by_id(TRAY_ID));
            // Never leave the app without a way back to its window.
            window::show_if_hidden(&self.app);
            return Ok(());
        }
        if self.app.tray_by_id(TRAY_ID).is_some() {
            self.render(true);
            return Ok(());
        }
        let lang = texts::current(&self.app);
        let model = lock(&self.inner).model.clone();
        let state = model.as_ref().map_or(TrayState::Disconnected, |m| m.state);
        let tip = model
            .as_ref()
            .map_or_else(|| texts::text(lang, "tray.appName"), |m| tooltip(m, lang));
        let mut builder = TrayIconBuilder::with_id(TRAY_ID)
            .icon(self.icons.get(state))
            .tooltip(tip)
            .show_menu_on_left_click(false);
        if let Some(model) = &model {
            let menu = build_menu(&self.app, &entries(model, lang))
                .map_err(|e| format!("cannot build the tray menu: {e}"))?;
            builder = builder.menu(&menu);
        }
        builder
            .build(&self.app)
            .map_err(|e| format!("cannot show the tray icon: {e}"))?;
        lock(&self.inner).rendered = None;
        self.render(true);
        Ok(())
    }

    /// Shows or removes the icon from any thread; waits (at most 5 s) for
    /// the main thread's answer.
    pub fn set_shown_from_any_thread(self: &Arc<Self>, shown: bool) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let tray = Arc::clone(self);
        self.app
            .run_on_main_thread(move || {
                let _ = tx.send(tray.set_shown(shown));
            })
            .map_err(|e| format!("cannot reach the main thread: {e}"))?;
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "the tray icon did not answer".to_string())?
    }
}

/// Builds a native menu. Main thread only (the items are made there).
fn build_menu<R: Runtime>(app: &AppHandle<R>, entries: &[Entry]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for entry in entries {
        match entry.kind {
            EntryKind::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            EntryKind::Text => menu.append(&MenuItem::with_id(
                app,
                entry.id.as_str(),
                &entry.text,
                entry.enabled,
                None::<&str>,
            )?)?,
            EntryKind::Check(checked) => menu.append(&CheckMenuItem::with_id(
                app,
                entry.id.as_str(),
                &entry.text,
                entry.enabled,
                checked,
                None::<&str>,
            )?)?,
        }
    }
    Ok(menu)
}

/// The tray of the app, if installed.
pub fn get<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<Tray<R>>> {
    app.try_state::<Arc<Tray<R>>>()
        .map(|state| Arc::clone(state.inner()))
}

/// Whether the tray icon is shown (close-to-tray applies only then).
pub fn is_shown<R: Runtime>(app: &AppHandle<R>) -> bool {
    get(app).is_some_and(|tray| tray.is_shown())
}

/// Redraws the menu (the language changed).
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    if let Some(tray) = get(app) {
        tray.post_render(true);
    }
}

/// Sets the tray up in `setup` (main thread): the click handlers, the
/// core's sink, and the icon when the setting is on. Returns whether the
/// icon is shown.
pub fn install<R: Runtime>(app: &AppHandle<R>, worker: &WorkerHandle, shown: bool) -> bool {
    let tray = Arc::new(Tray {
        app: app.clone(),
        icons: Icons::new(app.default_window_icon()),
        inner: Mutex::new(Inner::default()),
    });
    app.manage(Arc::clone(&tray));
    // Registered once for the app (not per icon: the icon comes and goes
    // with the setting).
    app.on_menu_event(on_menu_event);
    app.on_tray_icon_event(|app, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            window::focus_main(app);
        }
    });
    worker.set_tray_sink(Arc::new(Sink(Arc::clone(&tray))));
    shown && tray.set_shown(true).is_ok()
}

/// A menu click (main thread).
fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let Some(command) = parse(event.id().as_ref()) else {
        return;
    };
    match command {
        MenuCommand::Open => window::focus_main(app),
        MenuCommand::Quit => window::quit(app),
        MenuCommand::Action(action) => {
            let Some(worker) = app
                .try_state::<Arc<WorkerHandle>>()
                .map(|state| Arc::clone(state.inner()))
            else {
                return;
            };
            // A check item flipped its own mark: show the model's again.
            if let Some(tray) = get(app) {
                tray.post_render(true);
            }
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let outcome = worker.tray_action(action);
                if let TrayOutcome::OpenWindow { route, .. } = outcome {
                    let target = app.clone();
                    let _ = app.run_on_main_thread(move || window::open_route(&target, &route));
                }
                if let Some(tray) = get(&app) {
                    tray.post_render(true);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use openwraith_app_core::shell::{TrayAttention, TrayProfileItem};

    use super::*;

    fn model() -> TrayModel {
        TrayModel {
            state: TrayState::Normal,
            header: TrayHeader::Connected {
                model: Some("Wraith W75".to_string()),
                profile: Some(2),
                name: Some("FPS".to_string()),
            },
            profiles: (1..=4)
                .map(|p| TrayProfileItem {
                    profile: p,
                    name: (p == 2).then(|| "FPS".to_string()),
                    checked: p == 2,
                    enabled: true,
                })
                .collect(),
            auto_paused: None,
            back_up_now: true,
            attention: Vec::new(),
        }
    }

    fn ids(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn the_menu_follows_the_model() {
        let entries = entries(&model(), Lang::En);
        assert_eq!(
            ids(&entries),
            [
                "header",
                "",
                "profile-1",
                "profile-2",
                "profile-3",
                "profile-4",
                "",
                "back-up-now",
                "",
                "open",
                "quit"
            ]
        );
        assert_eq!(entries[0].text, "OpenWraith · Wraith W75 · Profile 2 (FPS)");
        assert!(!entries[0].enabled, "the header is not a command");
        assert_eq!(entries[3].text, "Profile 2 (FPS)");
        assert_eq!(entries[3].kind, EntryKind::Check(true));
        assert_eq!(entries[2].kind, EntryKind::Check(false));
        assert_eq!(entries[2].text, "Profile 1");
        assert_eq!(entries.last().map(|e| e.text.as_str()), Some("Quit"));
    }

    #[test]
    fn the_menu_shows_pause_attention_and_disabled_items() {
        let mut m = model();
        m.auto_paused = Some(false);
        m.back_up_now = false;
        m.state = TrayState::Attention;
        m.attention = vec![TrayAttention::ResetReview, TrayAttention::AutoPaused];
        for item in &mut m.profiles {
            item.enabled = false;
        }
        let entries = entries(&m, Lang::Tr);
        let pause = entries.iter().find(|e| e.id == "games-pause");
        assert_eq!(
            pause.map(|e| (e.text.as_str(), e.kind)),
            Some(("Oyun profillerini duraklat", EntryKind::Check(false)))
        );
        let backup = entries.iter().find(|e| e.id == "back-up-now");
        assert_eq!(backup.map(|e| e.enabled), Some(false));
        assert!(
            entries
                .iter()
                .filter(|e| e.id.starts_with("profile-"))
                .all(|e| !e.enabled)
        );
        let attention: Vec<&str> = entries
            .iter()
            .filter(|e| e.id.starts_with("attention-"))
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(
            attention,
            [
                "Klavye sıfırlandı — gözden geçirin…",
                "Oyun profilleri duraklatıldı — gözden geçirin…"
            ]
        );
        // Paused: the item resumes.
        m.auto_paused = Some(true);
        let entries = super::entries(&m, Lang::En);
        assert!(
            entries
                .iter()
                .any(|e| e.id == "games-resume" && e.kind == EntryKind::Check(true))
        );
    }

    #[test]
    fn every_entry_id_maps_to_its_command() {
        let mut m = model();
        m.auto_paused = Some(false);
        m.attention = ATTENTION.to_vec();
        for entry in entries(&m, Lang::En) {
            let command = parse(&entry.id);
            match entry.id.as_str() {
                "" | "header" => assert_eq!(command, None),
                "open" => assert_eq!(command, Some(MenuCommand::Open)),
                "quit" => assert_eq!(command, Some(MenuCommand::Quit)),
                _ => assert!(
                    matches!(command, Some(MenuCommand::Action(_))),
                    "{} has no action",
                    entry.id
                ),
            }
        }
        assert_eq!(
            parse("profile-3"),
            Some(MenuCommand::Action(TrayAction::SwitchProfile(3)))
        );
        assert_eq!(
            parse("games-pause"),
            Some(MenuCommand::Action(TrayAction::SetAutoPaused(true)))
        );
        assert_eq!(
            parse("games-resume"),
            Some(MenuCommand::Action(TrayAction::SetAutoPaused(false)))
        );
        assert_eq!(
            parse("back-up-now"),
            Some(MenuCommand::Action(TrayAction::BackUpNow))
        );
        for item in ATTENTION {
            assert_eq!(
                parse(attention_id(item)),
                Some(MenuCommand::Action(TrayAction::Attention(item)))
            );
            assert!(texts::get(Lang::En, attention_key(item)).is_some());
            assert!(texts::get(Lang::Tr, attention_key(item)).is_some());
        }
        for bad in [
            "profile-0",
            "profile-5",
            "profile-x",
            "profile-",
            "attention-",
            "x",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_header_and_tooltip_say_the_state_in_words() {
        assert_eq!(
            header_text(&TrayHeader::NotConnected, Lang::En),
            "OpenWraith · Keyboard not connected"
        );
        assert_eq!(
            header_text(&TrayHeader::NotConnected, Lang::Tr),
            "OpenWraith · Klavye bağlı değil"
        );
        let bare = TrayHeader::Connected {
            model: None,
            profile: None,
            name: None,
        };
        assert_eq!(
            header_text(&bare, Lang::En),
            "OpenWraith · Keyboard connected"
        );
        let unnamed = TrayHeader::Connected {
            model: Some(" ".to_string()),
            profile: Some(1),
            name: Some(String::new()),
        };
        assert_eq!(header_text(&unnamed, Lang::Tr), "OpenWraith · Profil 1");

        let mut m = model();
        assert_eq!(
            tooltip(&m, Lang::En),
            "OpenWraith · Wraith W75 · Profile 2 (FPS)"
        );
        m.state = TrayState::Attention;
        assert_eq!(
            tooltip(&m, Lang::En),
            "OpenWraith · Wraith W75 · Profile 2 (FPS) — needs your attention"
        );
        m.header = TrayHeader::Connected {
            model: Some("W".repeat(300)),
            profile: None,
            name: None,
        };
        let tip = tooltip(&m, Lang::Tr);
        assert_eq!(tip.chars().count(), TOOLTIP_MAX);
        assert!(tip.ends_with('…'));
    }

    #[test]
    fn user_text_cannot_make_mnemonics() {
        assert_eq!(menu_text("Work & Play"), "Work && Play");
        assert_eq!(menu_text("a\tb\nc"), "abc");
        let mut m = model();
        m.profiles[0].name = Some("R&D".to_string());
        let entries = entries(&m, Lang::En);
        assert_eq!(entries[2].text, "Profile 1 (R&&D)");
    }

    #[test]
    fn the_icon_looks_differ_by_state() {
        // A 6x6 opaque red square.
        let red = [0xFF, 0x00, 0x00, 0xFF].repeat(36);
        let dim = dimmed(&red);
        assert_eq!(dim.len(), red.len());
        assert!(
            dim.as_chunks::<4>()
                .0
                .iter()
                .all(|px| px[0] == px[1] && px[1] == px[2])
        );
        assert!(dim.as_chunks::<4>().0.iter().all(|px| px[3] < 0xFF));

        let badge = with_badge(&red, 6, 6);
        assert_eq!(badge.len(), red.len());
        // The top-left pixel is untouched, the lower right corner is amber.
        assert_eq!(&badge[0..4], &red[0..4]);
        let corner = ((4 * 6) + 4) * 4;
        assert_eq!(&badge[corner..corner + 4], &[0xF5, 0x9E, 0x0B, 0xFF]);
        assert_ne!(badge, red);
        // Too small or short buffers are left alone.
        assert_eq!(with_badge(&red[..8], 6, 6), red[..8].to_vec());
        assert_eq!(fallback_rgba(2).len(), 16);
    }
}
