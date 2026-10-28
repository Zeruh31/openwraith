//! The device status and the events pushed to the frontend.

use serde::{Deserialize, Serialize};

use super::auto::{AutoCauseView, AutoPauseReason, AutoWaitReason, AutomationStatusView};
use super::common::{DeviceId, DeviceIdentityView, FileRef, ForeignActivityView, OpId};
use super::error::AppError;
use super::progress::ProgressEvent;
use super::recovery::{FirmwareView, RecoveryStatusView, ResetEvidenceView};
use super::travel::{DepthStopReason, TravelCloseView, TravelStatusView};

/// An event on the `subscribe` channel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AppEvent {
    /// The full status, sent on every change.
    Status {
        /// The status.
        status: Box<DeviceStatus>,
    },
    /// A one-shot message.
    Notice {
        /// The message.
        notice: Notice,
    },
}

/// A one-shot message (a toast).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "notice", rename_all = "snake_case")]
pub enum Notice {
    /// The keyboard reported another active profile (Fn + 1..4 on the
    /// keyboard, or another program); the settings were read again.
    ProfileChangedOnKeyboard {
        /// The profile shown before, if known.
        from: Option<u8>,
        /// The profile the keyboard reports now.
        to: u8,
    },
    /// The pending plan can no longer be applied.
    PlanInvalidated {
        /// Why.
        reason: PlanInvalidation,
    },
    /// A keyboard was connected and read.
    Connected,
    /// The keyboard was disconnected.
    Disconnected,
    /// SOCD was switched on the keyboard (Fn + Right Shift): the SOCD view
    /// is updated and a pending plan made against the old power (SOCD edit,
    /// restore or import touching SOCD) was invalidated.
    SocdToggledOnKeyboard {
        /// SOCD on after the toggle (every profile).
        power: bool,
    },
    /// The live key-depth view stopped.
    DepthViewStopped {
        /// Why.
        reason: DepthStopReason,
        /// How the close went.
        close: TravelCloseView,
    },
    /// A key-depth test may still be open on the keyboard (a marker of an
    /// earlier run, or stream reports nobody here asked for): offer
    /// `close_stray_travel_test`.
    StrayTravelTest,
    /// A profile looks reset compared with its last automatic backup: its
    /// factory tables are held (no automatic backup replaces them) and the
    /// Recovery card (`get_recovery`) offers a restore or "It's fine, keep
    /// them". Nothing was written. Sent once per profile and backup.
    ResetSuspected {
        /// The profile.
        profile: u8,
        /// Why.
        evidence: Vec<ResetEvidenceView>,
    },
    /// The keyboard runs another firmware than when its automatic backups
    /// were read (an update very likely resets the settings). Sent once per
    /// change.
    FirmwareChanged {
        /// The automatic backups' firmware.
        known: FirmwareView,
        /// The keyboard's firmware now.
        current: FirmwareView,
    },
    /// The settings file could not be read: it was kept aside and the
    /// defaults are used.
    SettingsFileInvalid {
        /// The name it was kept as (`settings.invalid-<stamp>.json`), if
        /// it could be moved.
        kept_as: Option<String>,
    },
    /// An automatic backup could not be saved (nothing else changed).
    AutoBackupFailed {
        /// The profile.
        profile: u8,
        /// Technical English.
        message: String,
    },
    /// A game rule switched the profile (or a game exited and the default
    /// profile is back).
    AutoSwitched {
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
    /// An automatic switch waits (sent once per switch; it is skipped
    /// after 30 s).
    AutoSwitchDeferred {
        /// The rule (`null`: back to the default profile).
        rule_id: Option<String>,
        /// The game's name.
        label: Option<String>,
        /// The profile it will switch to.
        profile: u8,
        /// What for.
        cause: AutoCauseView,
        /// Why it waits.
        reason: AutoWaitReason,
    },
    /// An automatic switch waited 30 s and was skipped.
    AutoSwitchSkipped {
        /// The rule (`null`: back to the default profile).
        rule_id: Option<String>,
        /// The game's name.
        label: Option<String>,
        /// The profile it would have switched to.
        profile: u8,
        /// What for.
        cause: AutoCauseView,
        /// Why.
        reason: AutoWaitReason,
    },
    /// The automatic profiles were paused.
    AutoProfilePaused {
        /// Why.
        reason: AutoPauseReason,
    },
    /// An OpenWraith command (the CLI) uses the keyboard: the app listens
    /// and sends nothing until it has finished, then reads the keyboard
    /// again (`DeviceStatus.paused_for_cli`).
    PausedForCli {
        /// What the command said it does (`set trigger`, `backup`, ...), if
        /// known.
        purpose: Option<String>,
    },
}

/// Why a plan was invalidated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum PlanInvalidation {
    /// A newer plan replaced it.
    Superseded,
    /// It was older than 10 minutes.
    Expired,
    /// The token is not known (already used or never issued).
    Unknown,
    /// The keyboard was disconnected or reconnected.
    Disconnected,
    /// The keyboard reset itself.
    ResetSeen,
    /// Another program is using the keyboard.
    OtherProgram,
    /// The active profile changed.
    ProfileChanged,
    /// The values the plan was made against changed.
    ValuesChanged,
    /// The settings shown were read again.
    SnapshotChanged,
    /// Another operation ran.
    Operation,
    /// The window was reloaded.
    Resubscribed,
    /// The user discarded it.
    Discarded,
}

/// Everything the header and the pages need to know about the keyboard.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeviceStatus {
    /// Increases with every change.
    pub revision: u32,
    /// Increases with every new connection.
    pub generation: u32,
    /// The connection state.
    pub connection: ConnectionState,
    /// The connected keyboard, once read.
    pub device: Option<DeviceIdentityView>,
    /// The active profile, if known.
    pub active_profile: Option<u8>,
    /// Whether the firmware is one the app was tested with.
    pub firmware_tested: Option<bool>,
    /// Whether settings may be written now, and why not.
    pub write_gate: WriteGate,
    /// The running operation.
    pub busy: Option<OpInfo>,
    /// The revision of the settings snapshot, if one was read.
    pub snapshot_revision: Option<u32>,
    /// Whether the snapshot may be out of date (another program).
    pub snapshot_stale: bool,
    /// The window was asked to close while an operation runs.
    pub close_requested: bool,
    /// An operation that was interrupted last time (app closed or crashed).
    pub interrupted: Option<InterruptedOp>,
    /// Whether this is the developer simulator.
    pub sim: bool,
    /// Whether a key-depth test is (or may be) open on the keyboard.
    pub travel: TravelStatusView,
    /// Whether the keyboard looks reset (the Recovery card).
    pub recovery: RecoveryStatusView,
    /// The per-game automatic profiles (the header's **Auto** chip).
    pub automation: AutomationStatusView,
    /// An OpenWraith command (the CLI) uses the keyboard: the app sends
    /// nothing until it has finished.
    pub paused_for_cli: bool,
}

/// The connection state machine.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ConnectionState {
    /// The app is starting.
    Starting,
    /// No keyboard is connected; looking for one.
    Searching,
    /// Several keyboards are connected; the user chooses one.
    SeveralKeyboards {
        /// The keyboards.
        devices: Vec<DeviceEntryView>,
    },
    /// Opening a keyboard.
    Opening {
        /// The keyboard.
        device: DeviceEntryView,
    },
    /// The keyboard could not be opened; retried automatically.
    OpenFailed {
        /// Why.
        error: AppError,
        /// When the next attempt is made.
        retry_in_ms: u32,
    },
    /// Listening (sending nothing) to make sure no other program uses the
    /// keyboard.
    Observing {
        /// How long the observation lasts.
        required_ms: u32,
    },
    /// Another program is using the keyboard; OpenWraith only listens.
    OtherProgram {
        /// What was seen.
        evidence: ForeignActivityView,
    },
    /// Reading the settings.
    Reading {
        /// Requests done.
        done: u16,
        /// Requests in total.
        total: u16,
    },
    /// Reading failed; the user can try again.
    ReadFailed {
        /// Why.
        error: AppError,
    },
    /// Connected and read.
    Ready,
    /// The connection was lost; reconnects automatically.
    Lost {
        /// Why.
        error: AppError,
    },
    /// The keyboard reset itself (Fn + Space); nothing more is sent until
    /// the user reconnects.
    ResetSeen,
    /// The keyboard is not a supported model.
    Unsupported {
        /// The product code it reports.
        product: Option<String>,
    },
    /// The user released the keyboard.
    Released,
    /// An internal error; writes are blocked until the app restarts.
    Fault {
        /// Technical message.
        message: String,
    },
}

/// A connected keyboard in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeviceEntryView {
    /// Its id for `select_device`.
    pub id: DeviceId,
    /// USB vendor ID.
    pub vendor_id: u16,
    /// USB product ID.
    pub product_id: u16,
    /// USB product string.
    pub product: Option<String>,
    /// USB serial number.
    pub serial: Option<String>,
    /// USB interface number (-1 if unknown).
    pub interface: i32,
}

/// Whether settings may be written now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct WriteGate {
    /// True when nothing blocks writes.
    pub allowed: bool,
    /// Every reason that blocks them.
    pub reasons: Vec<WriteBlockReason>,
}

/// Why writes are blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum WriteBlockReason {
    NotConnected,
    Observing,
    Busy,
    OtherProgram,
    ResetSeen,
    SnapshotStale,
    SafetyDirUnavailable,
    Fault,
    /// An OpenWraith command (the CLI) uses the keyboard.
    PausedForCli,
}

/// The running operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct OpInfo {
    /// Its id.
    pub op: OpId,
    /// What it is.
    pub kind: OpKind,
    /// Its last progress event (so a reloaded window can show the panel).
    pub last: Option<ProgressEvent>,
}

/// Kinds of operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum OpKind {
    Refresh,
    Connect,
    ProfileSwitch,
    TriggerEdit,
    LightEdit,
    BackupActive,
    BackupAll,
    RestorePrepare,
    Restore,
    Recheck,
    /// A keymap-group edit (keys, macros, advanced keys).
    KeymapEdit,
    /// A key-colour edit (custom table 1).
    ColorEdit,
    /// A dead-band edit.
    DeadBandEdit,
    /// A SOCD edit.
    SocdEdit,
    /// The live key-depth view.
    DepthView,
    /// The close of a key-depth test left open.
    TravelClose,
    /// An export of one profile to a file.
    Export,
    /// The comparison of an import.
    ImportPrepare,
    /// An import of a profile file.
    Import,
    /// The comparison of automatic backups with the keyboard for a
    /// recovery (reads, and profile switches the user agreed to; the write
    /// is a `restore`).
    RecoveryCheck,
    /// A profile switch made by a game rule (the profile select and the
    /// read of the new profile; nothing else is written).
    AutoSwitch,
}

/// An operation that was interrupted (the journal was left behind).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct InterruptedOp {
    /// What it was.
    pub kind: OpKind,
    /// When it started (RFC 3339).
    pub started_at: String,
    /// The profiles it wrote to.
    pub profiles: Vec<u8>,
    /// The safety backup made before it, if it still exists.
    pub safety_backup: Option<FileRef>,
}
