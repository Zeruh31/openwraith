//! The per-game automatic profiles (Settings > Games), their live state in
//! the status (the header's **Auto** chip), the coexistence with the CLI
//! and what the frontend reports about itself (drafts, the vendor-site
//! acknowledgement). Program paths never cross the IPC boundary: programs
//! are referred to by [`ProgramToken`].

use serde::{Deserialize, Serialize};

/// An opaque token of a program the app found for a rule (the file dialog,
/// the running programs, "detect the next program"): `"g-<16 hex>"`. Its
/// path stays in Rust.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(transparent)]
pub struct ProgramToken(pub String);

/// A program offered for a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProgramView {
    /// The token for `set_auto_profile_rules`.
    pub token: ProgramToken,
    /// The program's file name (`game.exe`).
    pub exe: String,
    /// Its folder, for display only (`null` when unknown, for example a
    /// program running as administrator).
    pub folder: Option<String>,
    /// The name is shared by many programs (`javaw.exe`, `steam.exe`, ...):
    /// a rule for it switches for all of them.
    pub generic_name: bool,
}

/// Where a rule's program comes from (`set_auto_profile_rules`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleProgramIntent {
    /// The program of the existing rule with the same `id`, unchanged.
    Keep,
    /// A program the app offered (dialog, running list, detection).
    Picked {
        /// Its token.
        token: ProgramToken,
    },
    /// A program file name the user typed (`game.exe`): any folder.
    Name {
        /// The file name (no folder).
        exe: String,
    },
}

/// One rule as the Games table sends it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct AutoProfileRuleIntent {
    /// The id of an existing rule; `null` for a new one (the app gives it
    /// an id).
    pub id: Option<String>,
    /// The game's name as the user sees it (at most 64 characters).
    pub label: String,
    /// The program.
    pub program: RuleProgramIntent,
    /// The profile (1-4).
    pub profile: u8,
    /// The rule is on.
    pub enabled: bool,
}

/// Every per-game rule and the switch itself (`set_auto_profile_rules`;
/// replaces the stored rules). Creating a rule is the user's consent for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct AutoProfileRulesIntent {
    /// Switch profiles automatically for games.
    pub enabled: bool,
    /// The profile to return to when a game exits (only if the active
    /// profile is still the one a rule set); `null` = stay.
    pub default_profile: Option<u8>,
    /// The rules (at most 64).
    pub rules: Vec<AutoProfileRuleIntent>,
}

/// A stored rule, for the Games table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AutoProfileRuleView {
    /// Its id.
    pub id: String,
    /// The game's name.
    pub label: String,
    /// The program's file name.
    pub exe: String,
    /// The folder it must be in, for display (`null` = any folder).
    pub folder: Option<String>,
    /// The profile (1-4).
    pub profile: u8,
    /// The rule is on.
    pub enabled: bool,
    /// The name is shared by many programs (a warning).
    pub generic_name: bool,
}

/// Why the automatic profiles are paused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum AutoPauseReason {
    /// The user paused them (Settings > Games or the tray).
    User,
    /// More than 30 automatic switches within an hour.
    RateLimit,
    /// A switch failed (nothing more is tried until the user resumes).
    SwitchFailed {
        /// Technical English.
        message: String,
    },
}

/// Why an automatic switch waits (and, after 30 s, why it was skipped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AutoWaitReason {
    /// No keyboard is connected and read.
    NotConnected,
    /// The keyboard is being opened, observed or read.
    Connecting,
    /// Another operation runs.
    Busy,
    /// A plan waits for the user's review.
    PlanPending,
    /// A page has changes that are not written yet.
    Drafts,
    /// The live key-depth view runs.
    DepthView,
    /// A key-depth test is (or may still be) open on the keyboard.
    TravelTest,
    /// Another program uses the keyboard (the Wraith website).
    OtherProgram,
    /// An OpenWraith command (the CLI) uses the keyboard.
    PausedForCli,
    /// The keyboard reset itself; it must be read again first.
    ResetSeen,
    /// The user released the keyboard.
    Released,
    /// OpenWraith hit an internal error.
    Fault,
    /// Another program was heard in the last 2 minutes.
    RecentForeignActivity,
    /// A key assignment waits for the keyboard to keep it.
    KeymapUnsaved,
    /// The profile is changing on the keyboard.
    ProfileChanging,
}

/// What an automatic switch is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AutoCauseView {
    /// A game with a rule is in front.
    GameInFront,
    /// A game exited: back to the default profile.
    GameExited,
}

/// How the episode of the game in front stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AutoEpisodeState {
    /// In front for less than 1.5 s.
    Dwelling,
    /// Its profile was set (or was already active).
    Switched,
    /// The user chose another profile meanwhile: no more automatic switch
    /// while the game runs.
    Overridden,
    /// The switch waited 30 s and was skipped.
    Skipped,
}

/// A rule holds the active profile (the header's **Auto** chip).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AutoHoldView {
    /// The rule.
    pub rule_id: String,
    /// The game's name.
    pub label: String,
    /// The profile it set.
    pub profile: u8,
}

/// An automatic switch that waits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AutoWaitView {
    /// The rule (`null`: back to the default profile).
    pub rule_id: Option<String>,
    /// The game's name.
    pub label: Option<String>,
    /// The profile it will switch to.
    pub profile: u8,
    /// What for.
    pub cause: AutoCauseView,
    /// Why it waits.
    pub reason: AutoWaitReason,
}

/// The automatic profiles in the status.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AutomationStatusView {
    /// Switch profiles automatically for games (the setting).
    pub enabled: bool,
    /// Paused, and why.
    pub paused: Option<AutoPauseReason>,
    /// A rule holds the active profile.
    pub holding: Option<AutoHoldView>,
    /// A switch waits.
    pub waiting: Option<AutoWaitView>,
}

/// The live status line of Settings > Games.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "line", rename_all = "snake_case")]
pub enum AutoLineView {
    /// Automatic profiles are off.
    Off,
    /// They are paused.
    Paused {
        /// Why.
        reason: AutoPauseReason,
    },
    /// On, but no rule is on.
    NoRules,
    /// Waiting for a game with a rule to come to the front.
    Watching,
    /// A game with a rule is in front.
    GameInFront {
        /// The rule.
        rule_id: String,
        /// The game's name.
        label: String,
        /// Its profile.
        profile: u8,
        /// How its episode stands.
        state: AutoEpisodeState,
    },
    /// A switch waits.
    Waiting {
        /// The switch.
        wait: AutoWaitView,
    },
}

/// One entry of the recent activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AutoEventView {
    /// The profile was switched.
    Switched {
        /// The rule (`null`: back to the default profile).
        rule_id: Option<String>,
        /// The game's name.
        label: Option<String>,
        /// The profile before, if known.
        from: Option<u8>,
        /// The profile now.
        to: u8,
        /// What for.
        cause: AutoCauseView,
    },
    /// A switch waits.
    Deferred {
        /// The rule (`null`: back to the default profile).
        rule_id: Option<String>,
        /// The game's name.
        label: Option<String>,
        /// The profile.
        profile: u8,
        /// What for.
        cause: AutoCauseView,
        /// Why it waits.
        reason: AutoWaitReason,
    },
    /// A switch waited 30 s and was skipped.
    Skipped {
        /// The rule (`null`: back to the default profile).
        rule_id: Option<String>,
        /// The game's name.
        label: Option<String>,
        /// The profile.
        profile: u8,
        /// What for.
        cause: AutoCauseView,
        /// Why.
        reason: AutoWaitReason,
    },
    /// The user chose another profile while a rule held one.
    Overridden {
        /// The rule whose episode is overridden, if any.
        rule_id: Option<String>,
        /// The game's name.
        label: Option<String>,
        /// The profile the user chose.
        profile: u8,
    },
    /// The automatic profiles were paused.
    Paused {
        /// Why.
        reason: AutoPauseReason,
    },
    /// They were resumed.
    Resumed,
}

/// An entry of the recent activity with its time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AutoActivityView {
    /// When (RFC 3339, UTC).
    pub at: String,
    /// What.
    pub event: AutoEventView,
}

/// Settings > Games (`get_auto_profile_state`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AutoProfileStateView {
    /// Switch profiles automatically for games.
    pub enabled: bool,
    /// The profile to return to when a game exits.
    pub default_profile: Option<u8>,
    /// The rules.
    pub rules: Vec<AutoProfileRuleView>,
    /// The automatic profiles now.
    pub status: AutomationStatusView,
    /// The live status line.
    pub line: AutoLineView,
    /// The last 20 events, newest first.
    pub recent: Vec<AutoActivityView>,
    /// The tray is on (the app keeps running when the window is closed).
    pub tray_enabled: bool,
    /// This build can see programs (the running-programs list and the
    /// detection work).
    pub programs_available: bool,
}

/// What the frontend tells the backend about itself (`report_ui_state`):
/// the automatic profiles wait while drafts exist, and a tray profile click
/// opens the window instead until this run has the vendor-site
/// acknowledgement. Reset to all-false by every `subscribe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct UiStateReport {
    /// A page has changes that are not written yet.
    pub drafts: bool,
    /// The "every wraith.software tab is closed" tick was given in this run
    /// and is still valid (no other program was heard since).
    pub vendor_acknowledged: bool,
}
