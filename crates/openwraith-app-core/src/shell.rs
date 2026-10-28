//! What the core and the shell (the Tauri glue: the foreground thread, the
//! tray icon, the OS notifications) hand each other. Plain Rust types, no
//! Tauri type: the glue implements the sink traits
//! ([`ProgramSource`](crate::ProgramSource),
//! [`NotificationSink`](crate::NotificationSink),
//! [`TraySink`](crate::TraySink)) and calls the
//! [`WorkerHandle`](crate::WorkerHandle) methods named below. Texts are
//! keys (`notifications.*`, `tray.*`) with parameters; the glue resolves them
//! in the user's language.

use std::collections::BTreeMap;

/// The program in front, as the platform saw it (the glue polls the
/// foreground window about once a second and calls
/// [`WorkerHandle::foreground_changed`](crate::WorkerHandle::foreground_changed)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundProgram {
    /// Its process id.
    pub pid: u32,
    /// Its file name (`game.exe`), if known.
    pub exe: Option<String>,
    /// Its full path, if known. A program running as administrator is
    /// known with its path (the limited query crosses integrity levels); a
    /// program whose limited query is denied (some anti-cheat protection)
    /// is not known at all with the feed in use. Without a path the rules
    /// fall back to the file name.
    pub path: Option<String>,
    /// It could not be queried fully (no path): set by the glue for an
    /// empty path, which its foreground feed does not produce.
    pub elevated: bool,
}

/// A running program for the "add from running programs" list (the glue
/// lists processes with `sysinfo`; the core filters, sorts and gives each
/// a token).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningProgram {
    /// Its process id.
    pub pid: u32,
    /// Its file name.
    pub exe: String,
    /// Its full path, if known.
    pub path: Option<String>,
}

/// The kinds of OS notifications: at most one per kind every 10 minutes
/// (the core throttles; the banner and the tray stay authoritative).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NotificationKind {
    /// The keyboard reported a reset (Fn + Space held).
    KeyboardReset,
    /// The settings of a profile look reset (compared with its automatic
    /// backup).
    ResetSuspected,
    /// The keyboard runs another firmware than its automatic backups.
    FirmwareChanged,
    /// An automatic backup could not be saved.
    AutoBackupFailed,
    /// A game rule switched the profile (only with the "game switch"
    /// notifications on; default off).
    AutoSwitched,
    /// A game's switch waits because a page has unwritten changes.
    AutoSwitchWaiting,
    /// A game's switch waited 30 s and was skipped (another program, ...).
    AutoSwitchSkipped,
    /// The automatic profiles were paused (too many switches, a failed
    /// switch).
    AutoPaused,
}

/// Every [`OsNotification::key`] the core sends: the glue has a title and
/// a body for each (`<key>.title`, `<key>.body`; its tests iterate this
/// list, and the core's tests check that nothing else is sent).
pub const NOTIFICATION_KEYS: &[&str] = &[
    "notifications.keyboardReset",
    "notifications.resetSuspected",
    "notifications.firmwareChanged",
    "notifications.autoBackupFailed",
    "notifications.autoSwitched",
    "notifications.autoSwitchedBack",
    "notifications.autoSwitchWaiting",
    "notifications.autoSwitchSkipped",
    "notifications.autoPausedRateLimit",
    "notifications.autoPausedFailed",
];

/// An OS notification for the glue to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsNotification {
    /// Its kind.
    pub kind: NotificationKind,
    /// The text key: the glue shows `<key>.title` and `<key>.body` (for
    /// example `notifications.autoSwitched`).
    pub key: String,
    /// Values for the text (`profile`, `label`, `known`, `current`, ...).
    pub params: BTreeMap<String, String>,
    /// Where a click leads (a frontend route, `#/backup?recover=1`); the
    /// glue focuses the main window and navigates there.
    pub route: String,
}

/// How the tray icon looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    /// Connected; nothing needs the user.
    Normal,
    /// No keyboard is connected.
    Disconnected,
    /// Something needs the user (see [`TrayModel::attention`]).
    Attention,
}

/// The first line of the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayHeader {
    /// `tray.header`: "OpenWraith · W75 · Profile 2 (FPS)".
    Connected {
        /// The model (`Wraith W75`) or product code, if known.
        model: Option<String>,
        /// The active profile, if known.
        profile: Option<u8>,
        /// Its local name, if any.
        name: Option<String>,
    },
    /// `tray.notConnected`: "Keyboard not connected".
    NotConnected,
}

/// A profile item of the tray menu (radio).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayProfileItem {
    /// The profile (1-4).
    pub profile: u8,
    /// Its local name, if any (`tray.profile` / `tray.profileNamed`).
    pub name: Option<String>,
    /// It is the active profile.
    pub checked: bool,
    /// The item can be clicked (a keyboard is connected and read).
    pub enabled: bool,
}

/// Something that needs the user, shown as a tray menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrayAttention {
    /// `tray.attention.reset`: "Keyboard reset — review…" (opens the
    /// Recovery card).
    ResetReview,
    /// `tray.attention.otherProgram`: "Another program was using the
    /// keyboard — check again" (re-checks without opening the window).
    OtherProgramCheck,
    /// `tray.attention.travel`: "A key-depth test may still be running —
    /// stop it" (opens Actuation > Key depth).
    TravelStop,
    /// `tray.attention.autoPaused`: "Game profiles paused — review…"
    /// (opens Settings > Games).
    AutoPaused,
}

/// The tray icon and its menu: header, Profile 1-4, Pause game profiles,
/// Back up now, the attention items, Open OpenWraith, Quit (the last two
/// are the glue's own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayModel {
    /// The icon.
    pub state: TrayState,
    /// The first line.
    pub header: TrayHeader,
    /// Profiles 1-4.
    pub profiles: Vec<TrayProfileItem>,
    /// "Pause game profiles" (`tray.pauseGames`): `None` = the automatic
    /// profiles are off (no item); `Some(checked)`.
    pub auto_paused: Option<bool>,
    /// "Back up now" (`tray.backUpNow`) can be clicked.
    pub back_up_now: bool,
    /// The attention items.
    pub attention: Vec<TrayAttention>,
}

/// A click in the tray menu ([`WorkerHandle::tray_action`](crate::WorkerHandle::tray_action);
/// "Open OpenWraith" and "Quit" are the glue's own).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// A profile item: a manual switch (`ops/profile.rs`), or the window
    /// when it cannot be made from the tray.
    SwitchProfile(u8),
    /// "Pause game profiles" toggled.
    SetAutoPaused(bool),
    /// "Back up now": an automatic backup of the active profile (no
    /// switching).
    BackUpNow,
    /// An attention item.
    Attention(TrayAttention),
}

/// Why a tray click opens the window instead of acting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenReason {
    /// A page has unwritten changes.
    Drafts,
    /// A plan waits for the user's review.
    PlanPending,
    /// Writes are blocked now (not connected, another program, an
    /// operation, ...).
    WritesBlocked,
    /// This run has no vendor-site acknowledgement yet.
    NoVendorAcknowledgement,
    /// The click needs the window (the Recovery card, the key-depth tab,
    /// Settings > Games).
    NeedsWindow,
    /// The action failed; the window shows why.
    Failed,
}

/// What a tray click did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayOutcome {
    /// Done (nothing for the glue to show).
    Done,
    /// Nothing to do (the profile is already active).
    NothingToDo,
    /// The glue focuses the main window and navigates to `route`.
    OpenWindow {
        /// The frontend route.
        route: String,
        /// Why.
        reason: OpenReason,
    },
}
