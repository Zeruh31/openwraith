//! The per-game automatic profiles and the shell around the background
//! agent (design B5, B7):
//!
//! * [`engine`]: the pure state machine (virtual clock) that decides when to
//!   switch;
//! * [`programs`]: program tokens, generic names, rules;
//! * [`notify`]: which notices become OS notifications (throttled);
//! * [`tray`]: the tray model and the rules of a tray click;
//! * [`log`]: the rotating automation log.
//!
//! [`AutoCore`] holds all of it in [`Shared`](crate::shared::Shared), so that
//! direct commands (the rules, the pause, the platform feed) never wait for
//! the worker; the worker polls the engine at every tick with the gates it
//! computed and makes the switch in `ops/auto_profile.rs`.

pub(crate) mod engine;
pub(crate) mod log;
pub(crate) mod notify;
pub(crate) mod programs;
pub(crate) mod tray;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::SyncSender;
use std::time::Instant;

use self::engine::{Engine, EngineEvent, Target};
use self::notify::Notifier;
use self::programs::ProgramRegistry;
use crate::dto::{
    AutoActivityView, AutoEventView, AutoProfileSettings, AutoProfileStateView,
    BackgroundSettingsView, Notice, UiStateReport,
};
use crate::shell::{ForegroundProgram, TrayModel};

/// How many events the recent activity keeps.
pub(crate) const RECENT: usize = 20;

/// A waiting "detect the next program I open".
struct Detect {
    id: u64,
    reply: SyncSender<ForegroundProgram>,
    /// The program in front when the detection started (not the answer).
    baseline: Option<u32>,
}

/// The automatic profiles' state shared by the worker and the command
/// handlers. See the [module documentation](self).
#[derive(Default)]
pub(crate) struct AutoCore {
    /// The engine.
    pub(crate) engine: Engine,
    /// The rules as stored.
    pub(crate) settings: AutoProfileSettings,
    /// The background settings (tray, notifications) as stored.
    pub(crate) background: BackgroundSettingsView,
    /// What the frontend reported.
    pub(crate) ui: UiStateReport,
    /// The programs offered for rules.
    pub(crate) programs: ProgramRegistry,
    /// The last [`RECENT`] events, newest first.
    pub(crate) recent: VecDeque<AutoActivityView>,
    /// The OS notification throttle.
    pub(crate) notifier: Notifier,
    /// The settings file (profile names for the tray).
    pub(crate) settings_path: Option<PathBuf>,
    /// The local names of the profiles of `names_product`.
    pub(crate) names: Vec<Option<String>>,
    /// The product the names are for (`None` = not loaded).
    pub(crate) names_product: Option<Option<String>>,
    /// The tray model shown last.
    pub(crate) last_tray: Option<TrayModel>,
    /// The program in front, as reported last.
    pub(crate) front: Option<ForegroundProgram>,
    detect: Option<Detect>,
    detect_id: u64,
}

impl std::fmt::Debug for AutoCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutoCore")
            .field("engine", &self.engine)
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl AutoCore {
    /// Loads the stored settings (at start, and after they were saved).
    pub(crate) fn load(
        &mut self,
        now: Instant,
        settings: AutoProfileSettings,
        background: BackgroundSettingsView,
    ) {
        self.engine
            .configure(now, programs::engine_config(&settings));
        self.settings = settings;
        self.background = background;
    }

    /// The program in front changed (the platform feed); a waiting
    /// detection gets it when it is a new program.
    pub(crate) fn foreground(&mut self, now: Instant, program: Option<ForegroundProgram>) {
        let own = std::process::id();
        self.engine.foreground(now, program.as_ref(), own);
        if let Some(p) = &program
            && p.pid != own
            && p.pid != 0
            && engine::identity(p).is_some()
            && let Some(detect) = &self.detect
            && detect.baseline != Some(p.pid)
            && detect.reply.try_send(p.clone()).is_ok()
        {
            self.detect = None;
        }
        self.front = program;
    }

    /// Starts "detect the next program I open": the next program that comes
    /// to the front (not the one in front now, not OpenWraith) is sent on
    /// `reply`. A detection already waiting ends without an answer.
    pub(crate) fn start_detect(&mut self, reply: SyncSender<ForegroundProgram>) -> u64 {
        self.detect_id = self.detect_id.wrapping_add(1);
        self.detect = Some(Detect {
            id: self.detect_id,
            reply,
            baseline: self.front.as_ref().map(|p| p.pid),
        });
        self.detect_id
    }

    /// Whether a detection waits (tests).
    #[cfg(test)]
    pub(crate) fn detecting(&self) -> bool {
        self.detect.is_some()
    }

    /// Ends the detection `id` if it still waits.
    pub(crate) fn stop_detect(&mut self, id: u64) {
        if self.detect.as_ref().is_some_and(|d| d.id == id) {
            self.detect = None;
        }
    }

    /// Turns engine events into the recent activity and the notices (sent
    /// by the caller, outside the lock); returns the new activity entries
    /// (for the log) and the notices.
    pub(crate) fn record(
        &mut self,
        events: Vec<EngineEvent>,
        at: &str,
    ) -> (Vec<AutoActivityView>, Vec<Notice>) {
        let mut entries = Vec::new();
        let mut notices = Vec::new();
        for event in events {
            let (view, notice) = convert(event);
            if let Some(view) = view {
                let entry = AutoActivityView {
                    at: at.to_string(),
                    event: view,
                };
                self.recent.push_front(entry.clone());
                self.recent.truncate(RECENT);
                entries.push(entry);
            }
            if let Some(notice) = notice {
                notices.push(notice);
            }
        }
        (entries, notices)
    }

    /// Settings > Games.
    pub(crate) fn state_view(&self, programs_available: bool) -> AutoProfileStateView {
        AutoProfileStateView {
            enabled: self.settings.enabled,
            default_profile: self.settings.default_profile,
            rules: self
                .settings
                .rules
                .iter()
                .map(programs::rule_view)
                .collect(),
            status: self.engine.status(),
            line: self.engine.line(),
            recent: self.recent.iter().cloned().collect(),
            tray_enabled: self.background.tray_enabled,
            programs_available,
        }
    }

    /// The local names of `product`'s profiles (read from the settings
    /// file when the product changed).
    pub(crate) fn names_for(&mut self, product: Option<&str>) -> Vec<Option<String>> {
        let wanted = product.map(str::to_string);
        if self.names_product.as_ref() != Some(&wanted) {
            self.names = match (&self.settings_path, &wanted) {
                (Some(path), Some(product)) => names_in(path, product),
                _ => vec![None; 4],
            };
            self.names_product = Some(wanted);
        }
        self.names.clone()
    }

    /// Forgets the cached profile names (they were saved).
    pub(crate) fn forget_names(&mut self) {
        self.names_product = None;
    }
}

/// The four local names of `product` in the settings file.
fn names_in(path: &Path, product: &str) -> Vec<Option<String>> {
    let mut names = crate::settings::profile_names(path)
        .remove(product)
        .unwrap_or_default();
    names.resize(4, None);
    names
}

fn ids(target: &Target) -> (Option<String>, Option<String>) {
    (
        target.rule.as_ref().map(|r| r.id.clone()),
        target.rule.as_ref().map(|r| r.label.clone()),
    )
}

/// An engine event as recent activity and as a notice.
fn convert(event: EngineEvent) -> (Option<AutoEventView>, Option<Notice>) {
    match event {
        EngineEvent::Deferred { target, reason } => {
            let (rule_id, label) = ids(&target);
            (
                Some(AutoEventView::Deferred {
                    rule_id: rule_id.clone(),
                    label: label.clone(),
                    profile: target.profile,
                    cause: target.cause,
                    reason,
                }),
                Some(Notice::AutoSwitchDeferred {
                    rule_id,
                    label,
                    profile: target.profile,
                    cause: target.cause,
                    reason,
                }),
            )
        }
        EngineEvent::Skipped { target, reason } => {
            let (rule_id, label) = ids(&target);
            (
                Some(AutoEventView::Skipped {
                    rule_id: rule_id.clone(),
                    label: label.clone(),
                    profile: target.profile,
                    cause: target.cause,
                    reason,
                }),
                Some(Notice::AutoSwitchSkipped {
                    rule_id,
                    label,
                    profile: target.profile,
                    cause: target.cause,
                    reason,
                }),
            )
        }
        EngineEvent::Switched { target, from } => {
            let (rule_id, label) = ids(&target);
            (
                Some(AutoEventView::Switched {
                    rule_id: rule_id.clone(),
                    label: label.clone(),
                    from,
                    to: target.profile,
                    cause: target.cause,
                }),
                Some(Notice::AutoSwitched {
                    rule_id,
                    label,
                    from,
                    to: target.profile,
                    cause: target.cause,
                }),
            )
        }
        // The pause that follows carries the message.
        EngineEvent::Failed { .. } => (None, None),
        EngineEvent::Overridden { rule, profile } => (
            Some(AutoEventView::Overridden {
                rule_id: rule.as_ref().map(|r| r.id.clone()),
                label: rule.map(|r| r.label),
                profile,
            }),
            None,
        ),
        EngineEvent::Paused { reason } => (
            Some(AutoEventView::Paused {
                reason: reason.clone(),
            }),
            Some(Notice::AutoProfilePaused { reason }),
        ),
        EngineEvent::Resumed => (Some(AutoEventView::Resumed), None),
    }
}

/// Records `events` (recent activity, the automation log in `state_dir`)
/// and sends their notices.
pub(crate) fn emit(
    shared: &crate::shared::Shared,
    state_dir: &Path,
    events: Vec<EngineEvent>,
    at: &str,
) {
    if events.is_empty() {
        return;
    }
    let (entries, notices) = shared.auto().record(events, at);
    log::append(state_dir, &entries);
    for notice in notices {
        shared.notice(notice);
    }
}
