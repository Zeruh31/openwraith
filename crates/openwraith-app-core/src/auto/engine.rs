//! The per-game automatic profiles as a pure state machine on a virtual
//! clock (design B7). Nothing here talks to the keyboard, the platform or a
//! file: the worker feeds it what happened (the program in front, a process
//! that exited, a manual profile switch, the active profile), asks it with
//! the gates it computed whether to switch now ([`Engine::poll`]), performs
//! the switch (`ops/auto_profile.rs`, the only caller of the library's
//! profile switcher) and reports the result.
//!
//! Episodes: one per process (pid) of a game with a rule.
//!
//! * A ruled game in front for at least [`DWELL`] switches to its profile,
//!   once per episode (alt-tab away and back does not switch again).
//! * A manual switch (the keyboard's profile report, also one heard while
//!   the app observes, reads or waits for a write's gate; the header, the
//!   tray) marks the episode of the game in front, the episode whose rule
//!   holds the profile and the one of a switch on its way *overridden*: no
//!   more automatic switch for them (a switch on its way is not retried and
//!   holds nothing).
//! * The return to the default profile holds nothing: a later manual switch
//!   with no game around is no override.
//! * The first active profile known after an unknown one (a reconnect, a
//!   reset) is an observation, at that tick and the next ones: when it is
//!   not the profile the engine set, that hold ends silently.
//! * A game that exited (gone for at least [`EXIT_GRACE`]): another ruled
//!   game in front gets its profile (unless overridden); else the default
//!   profile, **only if** the game that exited is the one whose rule
//!   switched to the active profile and that profile is still active (a
//!   profile the user chose, also one a game's rule then merely kept, is
//!   never switched away; nor is a profile another running game set).
//!   Alt-tab away never switches.
//! * While the keyboard is not there (not connected, connecting) a switch
//!   waits silently: no deferral, no skip; nothing is assumed about the
//!   active profile while it is unknown.
//! * The engine never assumes that Fn + 1..4 still switch profiles on the
//!   keyboard (the user may have replaced them): it learns manual switches
//!   only from reports and from the active profile it is told.
//!
//! A switch whose gates are closed waits (`Deferred` once per episode, also
//! across alt-tabs), is retried at every poll and is skipped
//! [`DEFER_LIMIT`] after it first waited (`Skipped`). A switch is
//! scheduled no earlier than the gates' `not_before` (the keyboard's commit
//! settle after a write) and [`MIN_GAP`] after the previous automatic
//! switch. More than [`HOURLY_LIMIT`] switches within an hour, or any
//! failed switch, pause the automation until the user resumes it.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use crate::dto::{
    AutoCauseView, AutoEpisodeState, AutoHoldView, AutoLineView, AutoPauseReason, AutoWaitReason,
    AutoWaitView, AutomationStatusView,
};
use crate::shell::ForegroundProgram;

/// How long a ruled game must be in front before its profile is set.
pub(crate) const DWELL: Duration = Duration::from_millis(1500);

/// How long a game must be gone before its exit counts.
pub(crate) const EXIT_GRACE: Duration = Duration::from_secs(3);

/// How long a switch waits for its gates before it is skipped.
pub(crate) const DEFER_LIMIT: Duration = Duration::from_secs(30);

/// The least time between two automatic switches.
pub(crate) const MIN_GAP: Duration = Duration::from_secs(5);

/// The most automatic switches within [`HOUR`]; one more pauses them.
pub(crate) const HOURLY_LIMIT: usize = 30;

/// The window of [`HOURLY_LIMIT`].
pub(crate) const HOUR: Duration = Duration::from_secs(3600);

/// A rule as the engine uses it (only rules that are on).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule {
    /// The rule's id.
    pub(crate) id: String,
    /// The game's name.
    pub(crate) label: String,
    /// The program's file name, lower case.
    pub(crate) exe: String,
    /// The folder it must be in ([`normalize_folder`]); `None` = any.
    pub(crate) folder: Option<String>,
    /// The profile.
    pub(crate) profile: u8,
}

/// The engine's configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct EngineConfig {
    /// Switch profiles automatically.
    pub(crate) enabled: bool,
    /// The profile to return to when a game exits.
    pub(crate) default_profile: Option<u8>,
    /// The rules that are on, in the user's order.
    pub(crate) rules: Vec<Rule>,
}

/// A folder as rules compare it: lower case, `\` separators, no trailing
/// separator.
pub(crate) fn normalize_folder(folder: &str) -> String {
    folder
        .trim()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// The file name and folder of a program in front; `None` when nothing is
/// known about it.
pub(crate) fn identity(program: &ForegroundProgram) -> Option<(String, Option<String>)> {
    let from_path = program.path.as_deref().and_then(|p| {
        let p = p.replace('/', "\\");
        let (folder, name) = match p.rfind('\\') {
            Some(at) => (Some(p[..at].to_string()), p[at + 1..].to_string()),
            None => (None, p.clone()),
        };
        (!name.trim().is_empty()).then_some((name, folder))
    });
    let exe = program
        .exe
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .or_else(|| from_path.as_ref().map(|(name, _)| name.clone()))?;
    let folder = from_path
        .and_then(|(_, folder)| folder)
        .map(|f| normalize_folder(&f))
        .filter(|f| !f.is_empty());
    Some((exe.to_lowercase(), folder))
}

/// The rule for a program: same file name; a rule with a folder only for a
/// program in that folder. A program whose folder is unknown (only its file
/// name was reported) falls back to the file name; the platform feed in use
/// (`active-win-pos-rs`) reports the full path or nothing at all, so this
/// fallback serves other feeds and tests only. A rule with the program's
/// folder wins over one without; then the user's order.
pub(crate) fn match_rule<'r>(rules: &'r [Rule], program: &ForegroundProgram) -> Option<&'r Rule> {
    let (exe, folder) = identity(program)?;
    let candidates = rules.iter().filter(|r| r.exe == exe);
    match folder {
        Some(folder) => {
            let mut fallback = None;
            for rule in candidates {
                match &rule.folder {
                    Some(f) if *f == folder => return Some(rule),
                    None if fallback.is_none() => fallback = Some(rule),
                    _ => {}
                }
            }
            fallback
        }
        None => {
            let all: Vec<&Rule> = candidates.collect();
            all.iter()
                .find(|r| r.folder.is_none())
                .or_else(|| all.first())
                .copied()
        }
    }
}

/// The rule that a switch is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuleRef {
    /// Its id.
    pub(crate) id: String,
    /// The game's name.
    pub(crate) label: String,
}

impl RuleRef {
    fn of(rule: &Rule) -> Self {
        RuleRef {
            id: rule.id.clone(),
            label: rule.label.clone(),
        }
    }
}

/// An automatic switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    /// The rule (`None`: back to the default profile).
    pub(crate) rule: Option<RuleRef>,
    /// The profile.
    pub(crate) profile: u8,
    /// What for.
    pub(crate) cause: AutoCauseView,
}

/// What the engine reports (the worker turns these into notices, the
/// recent activity and the automation log).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EngineEvent {
    /// A switch waits (once per switch).
    Deferred {
        target: Target,
        reason: AutoWaitReason,
    },
    /// A switch waited [`DEFER_LIMIT`] and was skipped.
    Skipped {
        target: Target,
        reason: AutoWaitReason,
    },
    /// A switch was made.
    Switched { target: Target, from: Option<u8> },
    /// A switch failed (the automation is paused right after).
    Failed { target: Target, message: String },
    /// The user switched profiles while a rule held one or a game was in
    /// front.
    Overridden { rule: Option<RuleRef>, profile: u8 },
    /// The automation was paused.
    Paused { reason: AutoPauseReason },
    /// It was resumed.
    Resumed,
}

/// What the worker found about switching now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Gates {
    /// The first reason a switch may not be sent now.
    pub(crate) block: Option<AutoWaitReason>,
    /// No switch before this (the commit settle of the last write).
    pub(crate) not_before: Option<Instant>,
    /// The active profile, if known.
    pub(crate) active: Option<u8>,
}

/// The program in front.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Front {
    program: ForegroundProgram,
    /// The rule of a game with a rule.
    rule: Option<Rule>,
    since: Instant,
}

impl Front {
    fn pid(&self) -> u32 {
        self.program.pid
    }
}

/// One process of a game with a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Episode {
    rule: Rule,
    switched: bool,
    overridden: bool,
    skipped: bool,
    exited_at: Option<Instant>,
    /// Since when its switch waits, and why (announced once per episode,
    /// kept across alt-tabs so the skip comes [`DEFER_LIMIT`] after the
    /// first wait).
    deferred: Option<(Instant, AutoWaitReason)>,
}

impl Episode {
    fn new(rule: Rule) -> Self {
        Episode {
            rule,
            switched: false,
            overridden: false,
            skipped: false,
            exited_at: None,
            deferred: None,
        }
    }
}

/// A switch the engine wants to make.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    target: Target,
    /// The episode it is for (switches into a game).
    episode: Option<u32>,
    /// Since when its gates are closed, and why.
    blocked: Option<(Instant, AutoWaitReason)>,
}

/// The profile the engine set last (until the user chose another one, or
/// the episode that holds it ended).
#[derive(Debug, Clone, PartialEq, Eq)]
struct EngineSet {
    profile: u8,
    rule: Option<RuleRef>,
    /// The episode whose rule holds it.
    episode: Option<u32>,
    /// The engine switched to it (a profile that was already active, such
    /// as one the user chose, is only kept: no return to the default when
    /// its game exits).
    switched: bool,
}

/// See the [module documentation](self).
#[derive(Debug, Default)]
pub(crate) struct Engine {
    config: EngineConfig,
    paused: Option<AutoPauseReason>,
    front: Option<Front>,
    episodes: BTreeMap<u32, Episode>,
    engine_set: Option<EngineSet>,
    pending: Option<Pending>,
    in_flight: Option<Pending>,
    /// The user switched profiles while the switch in flight was on its
    /// way: it is not retried and holds nothing.
    flight_overridden: bool,
    last_switch: Option<Instant>,
    switches: VecDeque<Instant>,
    events: Vec<EngineEvent>,
    active: Option<u8>,
    /// OpenWraith's own process (never a game).
    own_pid: u32,
}

impl Engine {
    /// The configuration.
    pub(crate) fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Replaces the configuration (the rules were saved). Episodes of rules
    /// that are gone end; the game in front is matched again (its dwell
    /// starts now when it just got a rule).
    pub(crate) fn configure(&mut self, now: Instant, config: EngineConfig) {
        self.config = config;
        if !self.config.enabled {
            self.pending = None;
            self.episodes.clear();
            self.engine_set = None;
            if let Some(front) = &mut self.front {
                front.rule = None;
            }
            return;
        }
        let rules = self.config.rules.clone();
        let find = |id: &str| rules.iter().find(|r| r.id == id).cloned();
        self.episodes.retain(|_, e| match find(&e.rule.id) {
            Some(rule) if rule.exe == e.rule.exe && rule.folder == e.rule.folder => {
                if rule.profile != e.rule.profile && !e.overridden {
                    // Another profile for the game: it applies at its next
                    // dwell in front.
                    e.switched = false;
                    e.skipped = false;
                    e.deferred = None;
                }
                e.rule = rule;
                true
            }
            _ => false,
        });
        // A hold whose episode ended with its rule is gone.
        if self
            .engine_set
            .as_ref()
            .and_then(|s| s.episode)
            .is_some_and(|pid| !self.episodes.contains_key(&pid))
        {
            self.engine_set = None;
        }
        // The program in front is matched again: a game that just got a
        // rule starts its dwell now.
        let own_pid = self.own_pid;
        if let Some(front) = &mut self.front {
            let rule = match_rule(&self.config.rules, &front.program).cloned();
            match rule {
                Some(rule) if front.program.pid != own_pid => {
                    let fresh = !self.episodes.contains_key(&front.program.pid);
                    self.episodes
                        .entry(front.program.pid)
                        .or_insert_with(|| Episode::new(rule.clone()));
                    if fresh {
                        front.since = now;
                    }
                    front.rule = Some(rule);
                }
                _ => front.rule = None,
            }
        }
        // A pending switch for a rule that is gone is dropped.
        if let Some(p) = &self.pending {
            let keep = match &p.target.rule {
                Some(rule) => self
                    .config
                    .rules
                    .iter()
                    .any(|r| r.id == rule.id && r.profile == p.target.profile),
                None => self.config.default_profile == Some(p.target.profile),
            };
            if !keep {
                self.pending = None;
            }
        }
    }

    /// The program in front changed (`None`: nothing is known about it).
    pub(crate) fn foreground(
        &mut self,
        now: Instant,
        program: Option<&ForegroundProgram>,
        own_pid: u32,
    ) {
        self.own_pid = own_pid;
        let Some(program) = program.filter(|p| p.pid != 0 && identity(p).is_some()) else {
            // Nothing known about it: like a program without a rule.
            self.set_front(None);
            return;
        };
        if self.front.as_ref().is_some_and(|f| f.pid() == program.pid) {
            return;
        }
        // OpenWraith itself in front counts like any program without a
        // rule (alt-tab away never switches).
        let rule = if self.config.enabled && program.pid != own_pid {
            match_rule(&self.config.rules, program).cloned()
        } else {
            None
        };
        if let Some(rule) = &rule {
            self.episodes
                .entry(program.pid)
                .or_insert_with(|| Episode::new(rule.clone()));
        }
        self.set_front(Some(Front {
            program: program.clone(),
            rule,
            since: now,
        }));
    }

    fn set_front(&mut self, front: Option<Front>) {
        // A switch into a game that left the front is dropped: alt-tab away
        // never switches.
        let front_pid = front.as_ref().map(Front::pid);
        if let Some(p) = &self.pending
            && p.target.cause == AutoCauseView::GameInFront
            && p.episode != front_pid
        {
            self.pending = None;
        }
        self.front = front;
    }

    /// The processes whose exit the engine waits for (the platform checks
    /// them and reports [`process_exited`](Self::process_exited)).
    pub(crate) fn watched_pids(&self) -> Vec<u32> {
        self.episodes
            .iter()
            .filter(|(_, e)| e.exited_at.is_none())
            .map(|(pid, _)| *pid)
            .collect()
    }

    /// A process is gone.
    pub(crate) fn process_exited(&mut self, now: Instant, pid: u32) {
        if let Some(e) = self.episodes.get_mut(&pid)
            && e.exited_at.is_none()
        {
            e.exited_at = Some(now);
        }
        if self.front.as_ref().is_some_and(|f| f.pid() == pid) {
            self.set_front(None);
        }
    }

    /// The user switched to `profile` (the keyboard's report, the header,
    /// the tray): the episode of the game in front and the one holding the
    /// profile are overridden; a waiting switch is dropped, and one on its
    /// way is neither retried nor holds the profile when it lands.
    pub(crate) fn manual_switch(&mut self, _now: Instant, profile: u8) {
        self.active = Some(profile);
        if self
            .engine_set
            .as_ref()
            .is_some_and(|s| s.profile == profile)
        {
            return;
        }
        let mut rule = None;
        let mut overrode = self.pending.take().is_some();
        if let Some(flight) = &self.in_flight {
            // A switch on its way (waiting for its gate, or being sent):
            // not retried, and it holds nothing when it lands.
            self.flight_overridden = true;
            overrode = true;
            rule.clone_from(&flight.target.rule);
            if let Some(e) = flight.episode.and_then(|pid| self.episodes.get_mut(&pid)) {
                e.overridden = true;
            }
        }
        if let Some(front) = &self.front
            && let Some(e) = self.episodes.get_mut(&front.pid())
            && e.exited_at.is_none()
        {
            e.overridden = true;
            overrode = true;
            rule = Some(RuleRef::of(&e.rule));
        }
        if let Some(set) = self.engine_set.take() {
            overrode = true;
            if let Some(e) = set.episode.and_then(|pid| self.episodes.get_mut(&pid)) {
                e.overridden = true;
            }
            rule = rule.or(set.rule);
        }
        // Only what the engine did or wanted to do is reported (a manual
        // switch with no game around is no override).
        if overrode && self.config.enabled && self.paused.is_none() {
            self.events.push(EngineEvent::Overridden { rule, profile });
        }
    }

    /// The active profile as the worker knows it: another profile than the
    /// engine set (and no switch of the engine in flight) is a manual
    /// switch the engine was not told about.
    ///
    /// `None` means unknown (no keyboard, a reset): the first profile known
    /// after it is taken as it is, never as a manual switch. When it is not
    /// the profile the engine set, that hold ends silently right away (no
    /// override, no return to the default), so that the next look at the
    /// same profile is no manual switch either.
    pub(crate) fn observe_active(&mut self, now: Instant, active: Option<u8>) {
        let previous = self.active;
        self.active = active;
        let Some(active) = active else {
            return;
        };
        if self.in_flight.is_some()
            || !self
                .engine_set
                .as_ref()
                .is_some_and(|s| s.profile != active)
        {
            return;
        }
        if previous.is_some() {
            self.manual_switch(now, active);
        } else {
            self.engine_set = None;
        }
    }

    /// Pauses (`true`) or resumes the automation (the user's choice).
    pub(crate) fn set_paused(&mut self, paused: bool) {
        if paused {
            if self.paused.is_none() {
                self.paused = Some(AutoPauseReason::User);
                self.pending = None;
                self.events.push(EngineEvent::Paused {
                    reason: AutoPauseReason::User,
                });
            }
        } else if self.paused.take().is_some() {
            self.switches.clear();
            self.events.push(EngineEvent::Resumed);
        }
    }

    /// Why the automation is paused.
    pub(crate) fn paused(&self) -> Option<&AutoPauseReason> {
        self.paused.as_ref()
    }

    /// The events since the last call.
    pub(crate) fn take_events(&mut self) -> Vec<EngineEvent> {
        std::mem::take(&mut self.events)
    }

    fn pause(&mut self, reason: AutoPauseReason) {
        self.pending = None;
        self.paused = Some(reason.clone());
        self.events.push(EngineEvent::Paused { reason });
    }

    /// Decides whether to switch now (see the [module
    /// documentation](self)); `Some` = switch, then report with
    /// [`switch_done`](Self::switch_done) or
    /// [`switch_not_started`](Self::switch_not_started).
    pub(crate) fn poll(&mut self, now: Instant, gates: &Gates) -> Option<Target> {
        // Unknown (`None`) too: nothing is assumed about an unknown profile.
        self.active = gates.active;
        while self
            .switches
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= HOUR)
        {
            self.switches.pop_front();
        }
        let ended = self.finish_exits(now);
        let return_default = self.end_hold(&ended);
        if !self.config.enabled || self.paused.is_some() || self.in_flight.is_some() {
            if self.in_flight.is_none() {
                self.pending = None;
            }
            return None;
        }
        if !ended.is_empty() {
            self.exit_target(return_default);
        }
        self.dwell_target(now);
        let pending = self.pending.as_ref()?;
        if self.active.is_some() && Some(pending.target.profile) == self.active {
            // Already active: nothing to send; the rule keeps the profile
            // (the engine did not switch to it).
            let pending = self.pending.take()?;
            self.set_by_engine(&pending, false);
            return None;
        }
        if let Some(reason) = gates.block {
            if matches!(
                reason,
                AutoWaitReason::NotConnected | AutoWaitReason::Connecting
            ) {
                // No keyboard (yet): the switch waits silently and goes out
                // once it is ready (the game still in front).
                return None;
            }
            let announced = pending
                .episode
                .and_then(|pid| self.episodes.get(&pid))
                .and_then(|e| e.deferred);
            let first = pending.blocked.is_none() && announced.is_none();
            let since = pending.blocked.or(announced).map_or(now, |(at, _)| at);
            let target = pending.target.clone();
            let episode = pending.episode;
            if now.saturating_duration_since(since) >= DEFER_LIMIT {
                self.pending = None;
                if let Some(e) = episode.and_then(|pid| self.episodes.get_mut(&pid)) {
                    e.skipped = true;
                    e.deferred = None;
                }
                self.events.push(EngineEvent::Skipped { target, reason });
                return None;
            }
            if let Some(p) = &mut self.pending {
                p.blocked = Some((since, reason));
            }
            if let Some(e) = episode.and_then(|pid| self.episodes.get_mut(&pid)) {
                e.deferred = Some((since, reason));
            }
            if first {
                self.events.push(EngineEvent::Deferred { target, reason });
            }
            return None;
        }
        if let Some(p) = &mut self.pending {
            // The gates are open: no longer waiting (the episode keeps its
            // first wait, so it is not announced again).
            p.blocked = None;
        }
        let gap = self.last_switch.map(|t| t + MIN_GAP);
        let not_before = gates.not_before.max(gap);
        if not_before.is_some_and(|t| now < t) {
            return None;
        }
        if self.switches.len() >= HOURLY_LIMIT {
            self.pause(AutoPauseReason::RateLimit);
            return None;
        }
        let pending = self.pending.take()?;
        let target = pending.target.clone();
        self.in_flight = Some(pending);
        Some(target)
    }

    /// Ends the episodes of processes gone for [`EXIT_GRACE`]; returns
    /// their pids.
    fn finish_exits(&mut self, now: Instant) -> Vec<u32> {
        let gone: Vec<u32> = self
            .episodes
            .iter()
            .filter(|(_, e)| {
                e.exited_at
                    .is_some_and(|t| now.saturating_duration_since(t) >= EXIT_GRACE)
            })
            .map(|(pid, _)| *pid)
            .collect();
        for pid in &gone {
            self.episodes.remove(pid);
            if self
                .pending
                .as_ref()
                .is_some_and(|p| p.episode == Some(*pid))
            {
                self.pending = None;
            }
        }
        gone
    }

    /// The episode that holds the profile ended (`ended`): another running
    /// game whose rule switched to the same profile takes the hold over;
    /// else the hold ends. True when the default profile is due: the
    /// engine switched to the profile and it is still active.
    fn end_hold(&mut self, ended: &[u32]) -> bool {
        let Some(set) = &mut self.engine_set else {
            return false;
        };
        if !set.episode.is_some_and(|pid| ended.contains(&pid)) {
            return false;
        }
        let heir = self.episodes.iter().find(|(_, e)| {
            e.switched && !e.overridden && e.exited_at.is_none() && e.rule.profile == set.profile
        });
        if let Some((pid, e)) = heir {
            set.episode = Some(*pid);
            set.rule = Some(RuleRef::of(&e.rule));
            return false;
        }
        let due = set.switched && Some(set.profile) == self.active;
        self.engine_set = None;
        due
    }

    /// After a game exited: another ruled game in front gets its profile;
    /// else the default profile when `return_default` (the game that exited
    /// held the profile the engine switched to, and it is still active).
    fn exit_target(&mut self, return_default: bool) {
        if let Some(front) = &self.front
            && let Some(e) = self.episodes.get(&front.pid())
        {
            if !e.overridden && e.exited_at.is_none() {
                self.pending = Some(Pending {
                    target: Target {
                        rule: Some(RuleRef::of(&e.rule)),
                        profile: e.rule.profile,
                        cause: AutoCauseView::GameInFront,
                    },
                    episode: Some(front.pid()),
                    blocked: None,
                });
            }
            return;
        }
        let Some(default) = self.config.default_profile else {
            return;
        };
        if return_default {
            self.pending = Some(Pending {
                target: Target {
                    rule: None,
                    profile: default,
                    cause: AutoCauseView::GameExited,
                },
                episode: None,
                blocked: None,
            });
        }
    }

    /// A ruled game in front for [`DWELL`] wants its profile (once per
    /// episode).
    fn dwell_target(&mut self, now: Instant) {
        let Some(front) = &self.front else {
            return;
        };
        let pid = front.pid();
        let Some(e) = self.episodes.get(&pid) else {
            return;
        };
        if e.switched || e.overridden || e.skipped || e.exited_at.is_some() {
            return;
        }
        if now.saturating_duration_since(front.since) < DWELL {
            return;
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.episode == Some(pid))
        {
            return;
        }
        // An episode whose switch already waited is not announced again
        // (alt-tab away and back).
        let blocked = e.deferred;
        self.pending = Some(Pending {
            target: Target {
                rule: Some(RuleRef::of(&e.rule)),
                profile: e.rule.profile,
                cause: AutoCauseView::GameInFront,
            },
            episode: Some(pid),
            blocked,
        });
    }

    /// Records that the engine's switch (`switched`), or a rule whose
    /// profile was already active, holds the profile. A profile the engine
    /// switched to stays "switched" when another rule keeps it.
    fn set_by_engine(&mut self, pending: &Pending, switched: bool) {
        let Some(pid) = pending.episode else {
            // Back to the default profile: no game holds it, so a later
            // manual switch overrides nothing.
            self.engine_set = None;
            return;
        };
        if let Some(e) = self.episodes.get_mut(&pid) {
            e.switched = true;
            e.deferred = None;
        }
        let switched = switched
            || self
                .engine_set
                .as_ref()
                .is_some_and(|s| s.switched && s.profile == pending.target.profile);
        self.engine_set = Some(EngineSet {
            profile: pending.target.profile,
            rule: pending.target.rule.clone(),
            episode: pending.episode,
            switched,
        });
    }

    /// The switch [`poll`](Self::poll) asked for ended: `Ok(profile)` with
    /// the profile active now, or the error.
    pub(crate) fn switch_done(
        &mut self,
        now: Instant,
        result: Result<u8, String>,
        from: Option<u8>,
    ) {
        let Some(pending) = self.in_flight.take() else {
            return;
        };
        let overridden = std::mem::take(&mut self.flight_overridden);
        match result {
            Ok(profile) => {
                self.last_switch = Some(now);
                self.switches.push_back(now);
                if overridden {
                    // The user switched while it was sent: the worker's
                    // next look tells the active profile; nothing is held.
                    if let Some(e) = pending.episode.and_then(|pid| self.episodes.get_mut(&pid)) {
                        e.switched = true;
                        e.deferred = None;
                    }
                } else {
                    self.active = Some(profile);
                    self.set_by_engine(&pending, true);
                }
                self.events.push(EngineEvent::Switched {
                    target: pending.target,
                    from,
                });
            }
            Err(message) => {
                self.events.push(EngineEvent::Failed {
                    target: pending.target,
                    message: message.clone(),
                });
                self.pause(AutoPauseReason::SwitchFailed { message });
            }
        }
    }

    /// The switch [`poll`](Self::poll) asked for was not sent (a gate closed
    /// in between): it waits again.
    pub(crate) fn switch_not_started(&mut self, now: Instant, reason: AutoWaitReason) {
        let Some(mut pending) = self.in_flight.take() else {
            return;
        };
        if std::mem::take(&mut self.flight_overridden) {
            // The user switched meanwhile (Fn + 1..4 heard in the switch's
            // gate): the switch is dropped.
            return;
        }
        if matches!(
            reason,
            AutoWaitReason::NotConnected | AutoWaitReason::Connecting
        ) {
            // The keyboard went away: wait silently for it.
            self.pending = Some(pending);
            return;
        }
        let episode = pending.episode.and_then(|pid| self.episodes.get_mut(&pid));
        let announced = episode.as_ref().and_then(|e| e.deferred);
        if pending.blocked.is_none() {
            let since = announced.map_or(now, |(at, _)| at);
            pending.blocked = Some((since, reason));
            if let Some(e) = episode {
                e.deferred = Some((since, reason));
            }
            if announced.is_none() {
                self.events.push(EngineEvent::Deferred {
                    target: pending.target.clone(),
                    reason,
                });
            }
        }
        self.pending = Some(pending);
    }

    /// The automation in the status.
    pub(crate) fn status(&self) -> AutomationStatusView {
        // Only a profile a game's rule switched to (never one the user
        // chose that a rule merely kept), while that game runs.
        let holding = self.engine_set.as_ref().and_then(|s| {
            let rule = s.rule.as_ref()?;
            let running = s
                .episode
                .and_then(|pid| self.episodes.get(&pid))
                .is_some_and(|e| !e.overridden && e.exited_at.is_none());
            (s.switched && running && Some(s.profile) == self.active).then(|| AutoHoldView {
                rule_id: rule.id.clone(),
                label: rule.label.clone(),
                profile: s.profile,
            })
        });
        AutomationStatusView {
            enabled: self.config.enabled,
            paused: self.paused.clone(),
            holding: if self.config.enabled { holding } else { None },
            waiting: self.waiting(),
        }
    }

    fn waiting(&self) -> Option<AutoWaitView> {
        let p = self.pending.as_ref()?;
        let (_, reason) = p.blocked?;
        Some(AutoWaitView {
            rule_id: p.target.rule.as_ref().map(|r| r.id.clone()),
            label: p.target.rule.as_ref().map(|r| r.label.clone()),
            profile: p.target.profile,
            cause: p.target.cause,
            reason,
        })
    }

    /// The live status line of Settings > Games.
    pub(crate) fn line(&self) -> AutoLineView {
        if !self.config.enabled {
            return AutoLineView::Off;
        }
        if let Some(reason) = &self.paused {
            return AutoLineView::Paused {
                reason: reason.clone(),
            };
        }
        if self.config.rules.is_empty() {
            return AutoLineView::NoRules;
        }
        if let Some(wait) = self.waiting() {
            return AutoLineView::Waiting { wait };
        }
        let Some(front) = &self.front else {
            return AutoLineView::Watching;
        };
        let Some(e) = self.episodes.get(&front.pid()) else {
            return AutoLineView::Watching;
        };
        let state = if e.overridden {
            AutoEpisodeState::Overridden
        } else if e.skipped {
            AutoEpisodeState::Skipped
        } else if e.switched {
            AutoEpisodeState::Switched
        } else {
            AutoEpisodeState::Dwelling
        };
        AutoLineView::GameInFront {
            rule_id: e.rule.id.clone(),
            label: e.rule.label.clone(),
            profile: e.rule.profile,
            state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: u32 = 1;

    fn rule(id: &str, exe: &str, profile: u8) -> Rule {
        Rule {
            id: id.to_string(),
            label: format!("Game {id}"),
            exe: exe.to_lowercase(),
            folder: None,
            profile,
        }
    }

    fn program(pid: u32, exe: &str) -> ForegroundProgram {
        ForegroundProgram {
            pid,
            exe: Some(exe.to_string()),
            path: Some(format!(r"C:\Games\{exe}")),
            elevated: false,
        }
    }

    /// An engine on, with game A (`a.exe` -> 2) and game B (`b.exe` -> 3),
    /// default profile 1, active profile 1.
    fn engine(t0: Instant) -> Engine {
        let mut e = Engine::default();
        e.configure(
            t0,
            EngineConfig {
                enabled: true,
                default_profile: Some(1),
                rules: vec![rule("r-1", "a.exe", 2), rule("r-2", "b.exe", 3)],
            },
        );
        e.observe_active(t0, Some(1));
        e
    }

    fn open(active: u8) -> Gates {
        Gates {
            block: None,
            not_before: None,
            active: Some(active),
        }
    }

    fn blocked(active: u8, reason: AutoWaitReason) -> Gates {
        Gates {
            block: Some(reason),
            not_before: None,
            active: Some(active),
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Polls and completes a switch at once.
    fn switch(e: &mut Engine, now: Instant, gates: &Gates) -> Option<u8> {
        let target = e.poll(now, gates)?;
        e.switch_done(now, Ok(target.profile), gates.active);
        Some(target.profile)
    }

    #[test]
    fn a_game_in_front_for_the_dwell_switches_once_per_episode() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "A.EXE")), OWN);
        assert_eq!(e.poll(t0 + ms(1499), &open(1)), None, "not before 1.5 s");
        assert!(matches!(
            e.line(),
            AutoLineView::GameInFront {
                state: AutoEpisodeState::Dwelling,
                ..
            }
        ));
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        let events = e.take_events();
        assert!(
            matches!(&events[..], [EngineEvent::Switched { from: Some(1), .. }]),
            "{events:?}"
        );
        assert_eq!(e.status().holding.unwrap().rule_id, "r-1");
        // Alt-tab away and back: the same episode, no second switch.
        e.foreground(t0 + ms(2000), Some(&program(20, "notepad.exe")), OWN);
        assert_eq!(
            e.poll(t0 + ms(9000), &open(2)),
            None,
            "alt-tab away never switches"
        );
        e.foreground(t0 + ms(9000), Some(&program(10, "a.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(20_000), &open(2)), None);
        assert!(e.take_events().is_empty());
    }

    #[test]
    fn leaving_before_the_dwell_restarts_it() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        e.foreground(t0 + ms(1000), Some(&program(20, "x.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(1600), &open(1)), None);
        e.foreground(t0 + ms(2000), Some(&program(10, "a.exe")), OWN);
        assert_eq!(
            e.poll(t0 + ms(3000), &open(1)),
            None,
            "the dwell starts again"
        );
        assert_eq!(switch(&mut e, t0 + ms(3500), &open(1)), Some(2));
    }

    #[test]
    fn a_game_whose_profile_is_active_holds_it_without_a_switch() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(1500), &open(2)), None);
        assert!(e.take_events().is_empty());
        // The user chose the profile: no Auto chip, and no return to the
        // default when the game exits.
        assert_eq!(e.status().holding, None);
        assert!(matches!(
            e.line(),
            AutoLineView::GameInFront {
                state: AutoEpisodeState::Switched,
                ..
            }
        ));
        e.process_exited(t0 + ms(5000), 10);
        assert_eq!(e.poll(t0 + ms(9000), &open(2)), None, "no surprise switch");
        assert!(e.take_events().is_empty());
    }

    #[test]
    fn a_profile_chosen_by_hand_before_the_rule_is_never_switched_away() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        // The user picked profile 2 by hand; then the game comes.
        e.manual_switch(t0, 2);
        e.foreground(t0 + ms(100), Some(&program(10, "a.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(1600), &open(2)), None);
        e.foreground(t0 + ms(2000), Some(&program(20, "notepad.exe")), OWN);
        e.process_exited(t0 + ms(3000), 10);
        assert_eq!(e.poll(t0 + ms(6100), &open(2)), None);
        assert_eq!(e.poll(t0 + ms(20_000), &open(2)), None);
        assert!(e.take_events().is_empty());
    }

    #[test]
    fn two_games_on_one_profile_return_to_the_default_after_both_exit() {
        for first_exit in [10u32, 11] {
            let t0 = Instant::now();
            let mut e = Engine::default();
            e.configure(
                t0,
                EngineConfig {
                    enabled: true,
                    default_profile: Some(1),
                    rules: vec![rule("r-1", "a.exe", 2), rule("r-2", "b.exe", 2)],
                },
            );
            e.observe_active(t0, Some(1));
            e.foreground(t0, Some(&program(10, "a.exe")), OWN);
            assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
            // B keeps the profile A switched to.
            e.foreground(t0 + ms(2000), Some(&program(11, "b.exe")), OWN);
            assert_eq!(e.poll(t0 + ms(3600), &open(2)), None);
            e.foreground(t0 + ms(4000), Some(&program(20, "notepad.exe")), OWN);
            let other = if first_exit == 10 { 11 } else { 10 };
            e.process_exited(t0 + ms(5000), first_exit);
            assert_eq!(
                e.poll(t0 + ms(9000), &open(2)),
                None,
                "the other game still runs ({first_exit})"
            );
            assert!(e.status().holding.is_some(), "{first_exit}");
            e.process_exited(t0 + ms(10_000), other);
            let back = e.poll(t0 + ms(14_000), &open(2)).expect("the default");
            assert_eq!((back.profile, back.cause), (1, AutoCauseView::GameExited));
        }
    }

    #[test]
    fn another_ruled_game_that_exits_leaves_the_running_games_profile() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        // B in front for half a second, then another program.
        e.foreground(t0 + ms(10_000), Some(&program(11, "b.exe")), OWN);
        e.foreground(t0 + ms(10_500), Some(&program(20, "discord.exe")), OWN);
        e.process_exited(t0 + ms(12_000), 11);
        assert_eq!(e.poll(t0 + ms(16_000), &open(2)), None, "A still runs");
        assert_eq!(e.status().holding.unwrap().rule_id, "r-1");
        // A exits: now the default.
        e.process_exited(t0 + ms(20_000), 10);
        let back = e.poll(t0 + ms(24_000), &open(2)).unwrap();
        assert_eq!((back.profile, back.cause), (1, AutoCauseView::GameExited));
    }

    #[test]
    fn the_hold_ends_with_its_game_its_rule_or_the_automation() {
        // No default profile: the chip goes when the game exits.
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.configure(
            t0,
            EngineConfig {
                enabled: true,
                default_profile: None,
                rules: vec![rule("r-1", "a.exe", 2)],
            },
        );
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        e.process_exited(t0 + ms(5000), 10);
        e.poll(t0 + ms(9000), &open(2));
        assert_eq!(e.status().holding, None);
        e.take_events();
        // A later manual switch overrides nothing.
        e.manual_switch(t0 + ms(10_000), 4);
        assert!(e.take_events().is_empty());

        // An exit during a pause.
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        e.set_paused(true);
        e.process_exited(t0 + ms(5000), 10);
        e.poll(t0 + ms(9000), &open(2));
        e.set_paused(false);
        assert_eq!(e.poll(t0 + ms(10_000), &open(2)), None, "no late return");
        assert_eq!(e.status().holding, None);

        // The rule deleted, and the automation turned off.
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        e.configure(
            t0 + ms(2000),
            EngineConfig {
                enabled: true,
                default_profile: Some(1),
                rules: vec![rule("r-2", "b.exe", 3)],
            },
        );
        assert_eq!(e.status().holding, None);
        e.take_events();
        e.manual_switch(t0 + ms(3000), 4);
        assert!(e.take_events().is_empty(), "nothing to override");
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        e.configure(t0 + ms(2000), EngineConfig::default());
        assert_eq!(e.engine_set, None);
    }

    #[test]
    fn a_rule_given_another_profile_applies_it_at_the_next_dwell() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        e.foreground(t0 + ms(2000), Some(&program(OWN, "openwraith.exe")), OWN);
        e.configure(
            t0 + ms(3000),
            EngineConfig {
                enabled: true,
                default_profile: Some(1),
                rules: vec![rule("r-1", "a.exe", 4), rule("r-2", "b.exe", 3)],
            },
        );
        e.foreground(t0 + ms(4000), Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(10_000), &open(2)), Some(4));
        assert!(matches!(
            e.line(),
            AutoLineView::GameInFront {
                profile: 4,
                state: AutoEpisodeState::Switched,
                ..
            }
        ));
    }

    #[test]
    fn an_unknown_active_profile_is_no_reason_to_skip_the_gates() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        // The keyboard is unplugged (active unknown); it was on profile 2.
        e.observe_active(t0, Some(2));
        let unplugged = Gates {
            block: Some(AutoWaitReason::NotConnected),
            not_before: None,
            active: None,
        };
        e.observe_active(t0, None);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(1500), &unplugged), None);
        assert_eq!(e.poll(t0 + ms(40_000), &unplugged), None);
        assert!(e.take_events().is_empty(), "no deferral, no skip");
        assert_eq!(e.status().holding, None);
        assert!(matches!(
            e.line(),
            AutoLineView::GameInFront {
                state: AutoEpisodeState::Dwelling,
                ..
            }
        ));
        // Back on profile 3: an observation, no override; the switch goes.
        e.observe_active(t0 + ms(41_000), Some(3));
        assert!(e.take_events().is_empty());
        assert_eq!(switch(&mut e, t0 + ms(41_000), &open(3)), Some(2));
    }

    #[test]
    fn a_profile_seen_after_an_unknown_one_is_no_manual_switch() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        e.take_events();
        e.observe_active(t0 + ms(2000), None);
        e.observe_active(t0 + ms(9000), Some(3));
        assert!(e.take_events().is_empty(), "not an override");
    }

    /// One tick as the worker makes it: `observe_active`, then `poll`.
    fn tick(e: &mut Engine, now: Instant, active: Option<u8>) -> Option<u8> {
        e.observe_active(now, active);
        let gates = match active {
            Some(p) => open(p),
            None => Gates {
                block: Some(AutoWaitReason::NotConnected),
                not_before: None,
                active: None,
            },
        };
        switch(e, now, &gates)
    }

    #[test]
    fn the_profile_found_after_a_reconnect_stays_an_observation_at_later_ticks() {
        for back_on in [1u8, 2] {
            let t0 = Instant::now();
            let mut e = engine(t0);
            e.foreground(t0, Some(&program(10, "a.exe")), OWN);
            assert_eq!(tick(&mut e, t0 + ms(1500), Some(1)), Some(2));
            e.take_events();
            // Unplugged (or reset), then back on `back_on`, tick after tick.
            assert_eq!(tick(&mut e, t0 + ms(4000), None), None);
            assert_eq!(tick(&mut e, t0 + ms(4100), None), None);
            for at in [5000, 5100, 5200, 9000] {
                assert_eq!(tick(&mut e, t0 + ms(at), Some(back_on)), None, "{back_on}");
            }
            assert!(e.take_events().is_empty(), "{back_on}: no override");
            if back_on == 2 {
                // Still the engine's profile: the hold stays.
                assert_eq!(e.status().holding.map(|h| h.profile), Some(2));
            } else {
                // Another profile: the hold ended silently; the game's exit
                // switches nothing.
                assert_eq!(e.status().holding, None);
                e.process_exited(t0 + ms(10_000), 10);
                assert_eq!(tick(&mut e, t0 + ms(14_000), Some(1)), None);
                assert!(e.take_events().is_empty());
            }
        }
    }

    #[test]
    fn a_manual_switch_after_the_return_to_the_default_overrides_nothing() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        let mut active = 1u8;
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        for i in 1..30u64 {
            if let Some(p) = tick(&mut e, t0 + ms(i * 100), Some(active)) {
                active = p;
            }
        }
        assert_eq!(active, 2);
        // The game exits; a program without a rule is in front.
        e.foreground(t0 + ms(3000), Some(&program(20, "notepad.exe")), OWN);
        e.process_exited(t0 + ms(3000), 10);
        for i in 31..120u64 {
            if let Some(p) = tick(&mut e, t0 + ms(i * 100), Some(active)) {
                active = p;
            }
        }
        assert_eq!(active, 1, "back to the default");
        e.take_events();
        // Ten minutes later, no game around: the user picks profile 3.
        e.manual_switch(t0 + ms(600_000), 3);
        assert!(e.take_events().is_empty(), "no game around: no override");
        assert_eq!(tick(&mut e, t0 + ms(600_100), Some(3)), None);
        assert!(e.take_events().is_empty());
    }

    #[test]
    fn a_manual_switch_while_a_switch_is_on_its_way_drops_it() {
        for sent in [false, true] {
            let t0 = Instant::now();
            let mut e = engine(t0);
            e.foreground(t0, Some(&program(10, "a.exe")), OWN);
            let target = e.poll(t0 + ms(1500), &open(1)).expect("a switch");
            assert_eq!(target.profile, 2);
            // Fn + 3 heard while the switch waits for its gate (not sent)
            // or right after it was sent.
            e.manual_switch(t0 + ms(1600), 3);
            let events = e.take_events();
            assert!(
                matches!(&events[..], [EngineEvent::Overridden { profile: 3, .. }]),
                "{sent}: {events:?}"
            );
            if sent {
                e.switch_done(t0 + ms(1700), Ok(2), Some(1));
            } else {
                e.switch_not_started(t0 + ms(1700), AutoWaitReason::ProfileChanging);
            }
            assert_eq!(e.status().holding, None, "{sent}");
            // Nothing more for this game: no retry, no return later.
            for at in [3000, 9000, 20_000] {
                assert_eq!(tick(&mut e, t0 + ms(at), Some(3)), None, "{sent}");
            }
            e.process_exited(t0 + ms(21_000), 10);
            assert_eq!(tick(&mut e, t0 + ms(25_000), Some(3)), None, "{sent}");
            let events = e.take_events();
            assert!(
                events.iter().all(|ev| !matches!(
                    ev,
                    EngineEvent::Overridden { .. } | EngineEvent::Deferred { .. }
                )),
                "{sent}: {events:?}"
            );
        }
    }

    #[test]
    fn a_waiting_switch_is_announced_once_per_episode_across_alt_tabs() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        let drafts = blocked(1, AutoWaitReason::Drafts);
        let mut now = t0;
        let mut deferred = 0;
        let mut skipped = 0;
        for _ in 0..12 {
            e.foreground(now, Some(&program(10, "a.exe")), OWN);
            now += ms(1600);
            e.poll(now, &drafts);
            e.foreground(now, Some(&program(OWN, "openwraith.exe")), OWN);
            e.poll(now + ms(100), &drafts);
            now += ms(1500);
            for event in e.take_events() {
                match event {
                    EngineEvent::Deferred { .. } => deferred += 1,
                    EngineEvent::Skipped { .. } => skipped += 1,
                    _ => {}
                }
            }
        }
        assert_eq!(deferred, 1);
        assert_eq!(skipped, 1, "30 s after the first wait");
    }

    #[test]
    fn switches_older_than_an_hour_no_longer_count() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        let mut now = t0;
        let mut active = 1;
        for i in 0..30u32 {
            e.foreground(
                now,
                Some(&program(
                    100 + i,
                    if i % 2 == 0 { "a.exe" } else { "b.exe" },
                )),
                OWN,
            );
            now += ms(6000);
            assert!(switch(&mut e, now, &open(active)).is_some(), "switch {i}");
            active = if i % 2 == 0 { 2 } else { 3 };
        }
        e.take_events();
        // An hour after the first switches: room again.
        let later = now + HOUR;
        e.foreground(
            later,
            Some(&program(500, if active == 2 { "b.exe" } else { "a.exe" })),
            OWN,
        );
        assert!(switch(&mut e, later + ms(1500), &open(active)).is_some());
        assert!(
            !e.take_events()
                .iter()
                .any(|ev| matches!(ev, EngineEvent::Paused { .. }))
        );
    }

    #[test]
    fn exit_returns_to_the_default_only_while_the_engine_holds_the_profile() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        e.process_exited(t0 + ms(5000), 10);
        assert_eq!(e.poll(t0 + ms(7999), &open(2)), None, "gone for 3 s first");
        let back = e.poll(t0 + ms(8000), &open(2)).unwrap();
        assert_eq!(
            (back.profile, back.cause, back.rule),
            (1, AutoCauseView::GameExited, None)
        );
        e.switch_done(t0 + ms(8000), Ok(1), Some(2));

        // The user chose profile 4 while the game ran: no return.
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        e.manual_switch(t0 + ms(3000), 4);
        assert!(matches!(
            &e.take_events()[..],
            [_, EngineEvent::Overridden { profile: 4, .. }]
        ));
        e.process_exited(t0 + ms(5000), 10);
        assert_eq!(e.poll(t0 + ms(9000), &open(4)), None);
        assert_eq!(e.status().holding, None);
    }

    #[test]
    fn exit_with_another_ruled_game_in_front_takes_its_profile() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        e.foreground(t0 + ms(10_000), Some(&program(11, "b.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(11_500), &open(2)), Some(3));
        // Back to A: its episode already switched.
        e.foreground(t0 + ms(20_000), Some(&program(10, "a.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(25_000), &open(3)), None);
        // B exits while A is in front: A's profile.
        e.process_exited(t0 + ms(30_000), 11);
        let t = e.poll(t0 + ms(33_000), &open(3)).unwrap();
        assert_eq!((t.profile, t.cause), (2, AutoCauseView::GameInFront));
    }

    #[test]
    fn a_manual_switch_overrides_the_episode_and_drops_a_waiting_switch() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(
            e.poll(t0 + ms(1500), &blocked(1, AutoWaitReason::Drafts)),
            None
        );
        assert!(e.status().waiting.is_some());
        e.manual_switch(t0 + ms(2000), 3);
        assert_eq!(e.status().waiting, None);
        assert_eq!(e.poll(t0 + ms(10_000), &open(3)), None, "overridden");
        assert!(matches!(
            e.line(),
            AutoLineView::GameInFront {
                state: AutoEpisodeState::Overridden,
                ..
            }
        ));
        // A profile the engine did not set, seen by a read: also manual.
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(switch(&mut e, t0 + ms(1500), &open(1)), Some(2));
        e.observe_active(t0 + ms(4000), Some(4));
        assert_eq!(e.status().holding, None);
    }

    #[test]
    fn every_gate_defers_and_skips_after_30_s() {
        use AutoWaitReason::*;
        // (Not connected and connecting wait silently: see
        // `an_unknown_active_profile_is_no_reason_to_skip_the_gates`.)
        for reason in [
            Busy,
            PlanPending,
            Drafts,
            DepthView,
            TravelTest,
            OtherProgram,
            PausedForCli,
            ResetSeen,
            Released,
            Fault,
            RecentForeignActivity,
            KeymapUnsaved,
            ProfileChanging,
        ] {
            let t0 = Instant::now();
            let mut e = engine(t0);
            e.foreground(t0, Some(&program(10, "a.exe")), OWN);
            assert_eq!(e.poll(t0 + ms(1500), &blocked(1, reason)), None);
            assert!(
                matches!(&e.take_events()[..], [EngineEvent::Deferred { reason: r, .. }] if *r == reason),
                "{reason:?}"
            );
            assert_eq!(e.status().waiting.unwrap().reason, reason);
            assert_eq!(e.poll(t0 + ms(20_000), &blocked(1, reason)), None);
            assert!(e.take_events().is_empty(), "deferred once");
            assert_eq!(e.poll(t0 + ms(31_500), &blocked(1, reason)), None);
            assert!(
                matches!(&e.take_events()[..], [EngineEvent::Skipped { .. }]),
                "{reason:?}"
            );
            assert_eq!(
                e.poll(t0 + ms(40_000), &open(1)),
                None,
                "skipped for the episode"
            );
        }
        // Opened in time: the switch goes out.
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(1500), &blocked(1, Busy)), None);
        assert_eq!(switch(&mut e, t0 + ms(29_000), &open(1)), Some(2));
    }

    #[test]
    fn the_settle_and_the_gap_schedule_the_switch() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        let settle = Gates {
            not_before: Some(t0 + ms(2500)),
            ..open(1)
        };
        assert_eq!(
            e.poll(t0 + ms(1500), &settle),
            None,
            "not before the settle"
        );
        assert!(e.take_events().is_empty(), "a settle wait is no deferral");
        assert_eq!(switch(&mut e, t0 + ms(2500), &settle), Some(2));
        // B within 5 s of the last switch: waits for the gap.
        e.foreground(t0 + ms(2600), Some(&program(11, "b.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(4100), &open(2)), None);
        assert_eq!(switch(&mut e, t0 + ms(7500), &open(2)), Some(3));
    }

    #[test]
    fn more_than_30_switches_an_hour_or_an_error_pause_the_automation() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        let mut now = t0;
        let mut active = 1;
        for i in 0..30u32 {
            e.foreground(
                now,
                Some(&program(
                    100 + i,
                    if i % 2 == 0 { "a.exe" } else { "b.exe" },
                )),
                OWN,
            );
            now += ms(6000);
            assert!(switch(&mut e, now, &open(active)).is_some(), "switch {i}");
            active = if i % 2 == 0 { 2 } else { 3 };
        }
        e.take_events();
        e.foreground(
            now,
            Some(&program(500, if active == 2 { "b.exe" } else { "a.exe" })),
            OWN,
        );
        now += ms(6000);
        assert_eq!(e.poll(now, &open(active)), None);
        assert!(matches!(
            &e.take_events()[..],
            [EngineEvent::Paused {
                reason: AutoPauseReason::RateLimit
            }]
        ));
        assert!(matches!(e.line(), AutoLineView::Paused { .. }));
        e.set_paused(false);
        assert!(matches!(&e.take_events()[..], [EngineEvent::Resumed]));

        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        let target = e.poll(t0 + ms(1500), &open(1)).unwrap();
        e.switch_done(t0 + ms(1500), Err("no ack".to_string()), Some(1));
        let events = e.take_events();
        assert!(
            matches!(&events[..], [EngineEvent::Failed { target: t, .. }, EngineEvent::Paused { reason: AutoPauseReason::SwitchFailed { .. } }] if *t == target)
        );
        e.foreground(t0 + ms(2000), Some(&program(11, "b.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(9000), &open(1)), None, "paused");
    }

    #[test]
    fn a_manual_switch_without_a_game_is_no_override() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(20, "notepad.exe")), OWN);
        e.manual_switch(t0 + ms(100), 3);
        assert!(e.take_events().is_empty());
        // With a game in front it is.
        e.foreground(t0 + ms(200), Some(&program(10, "a.exe")), OWN);
        e.manual_switch(t0 + ms(300), 4);
        assert!(matches!(
            &e.take_events()[..],
            [EngineEvent::Overridden { rule: Some(r), profile: 4 }] if r.id == "r-1"
        ));
    }

    #[test]
    fn a_user_pause_stops_everything_until_resumed() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.set_paused(true);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(5000), &open(1)), None);
        e.set_paused(false);
        assert_eq!(switch(&mut e, t0 + ms(6000), &open(1)), Some(2));
    }

    #[test]
    fn programs_match_by_name_and_folder() {
        let mut with_folder = rule("r-1", "game.exe", 2);
        with_folder.folder = Some(normalize_folder(r"C:\Games\One\"));
        let any = rule("r-2", "game.exe", 3);
        let rules = vec![any.clone(), with_folder.clone()];
        let p = |path: Option<&str>, exe: Option<&str>| ForegroundProgram {
            pid: 5,
            exe: exe.map(str::to_string),
            path: path.map(str::to_string),
            elevated: path.is_none(),
        };
        assert_eq!(
            match_rule(&rules, &p(Some(r"c:\games\one\GAME.exe"), None)),
            Some(&with_folder)
        );
        assert_eq!(
            match_rule(&rules, &p(Some(r"D:\Other\game.exe"), None)),
            Some(&any)
        );
        assert_eq!(
            match_rule(&rules, &p(None, Some("Game.exe"))),
            Some(&any),
            "elevated: the name"
        );
        assert_eq!(
            match_rule(&[with_folder.clone()], &p(None, Some("game.exe"))),
            Some(&with_folder)
        );
        assert_eq!(
            match_rule(&[with_folder], &p(Some(r"D:\x\game.exe"), None)),
            None
        );
        assert_eq!(match_rule(&rules, &p(None, None)), None, "nothing known");
    }

    #[test]
    fn openwraith_itself_or_an_unknown_window_counts_as_alt_tab_away() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(1000), &open(1)), None);
        e.foreground(t0 + ms(1100), Some(&program(OWN, "a.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(5000), &open(1)), None);
        e.foreground(t0 + ms(5000), None, OWN);
        assert_eq!(e.poll(t0 + ms(9000), &open(1)), None);
    }

    #[test]
    fn rules_that_change_end_their_episodes() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        e.configure(
            t0 + ms(500),
            EngineConfig {
                enabled: true,
                default_profile: None,
                rules: vec![rule("r-2", "b.exe", 3)],
            },
        );
        assert_eq!(e.poll(t0 + ms(5000), &open(1)), None);
        assert!(e.watched_pids().is_empty());
        // Off: nothing at all.
        let mut e = engine(t0);
        e.configure(t0, EngineConfig::default());
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert_eq!(e.poll(t0 + ms(5000), &open(1)), None);
        assert_eq!(e.line(), AutoLineView::Off);
    }

    #[test]
    fn a_switch_not_started_waits_again() {
        let t0 = Instant::now();
        let mut e = engine(t0);
        e.foreground(t0, Some(&program(10, "a.exe")), OWN);
        assert!(e.poll(t0 + ms(1500), &open(1)).is_some());
        e.switch_not_started(t0 + ms(1500), AutoWaitReason::Busy);
        assert!(matches!(
            &e.take_events()[..],
            [EngineEvent::Deferred {
                reason: AutoWaitReason::Busy,
                ..
            }]
        ));
        assert_eq!(switch(&mut e, t0 + ms(2000), &open(1)), Some(2));
    }
}
