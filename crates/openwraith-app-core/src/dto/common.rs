//! Small types shared by many IPC messages: tokens, key and file references,
//! device identity, trigger and light values, table references.

use serde::{Deserialize, Serialize};

/// An opaque, single-use token of a pending plan (`"p-<16 hex>"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(transparent)]
pub struct PlanToken(pub String);

/// An opaque token for a file known to the app (`"f-<16 hex>"`). Paths never
/// cross the IPC boundary; the frontend refers to files by token.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(transparent)]
pub struct FileToken(pub String);

/// The id of one running operation (`"o-<n>"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(transparent)]
pub struct OpId(pub String);

/// The id of a connected keyboard (`"d-<16 hex>"`, a keyed hash of its
/// path; the path itself stays in Rust).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(transparent)]
pub struct DeviceId(pub String);

/// A key of the layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeyRef {
    /// Key index (row x 22 + col).
    pub index: u8,
    /// Matrix row.
    pub row: u8,
    /// Matrix column.
    pub col: u8,
    /// The library's key name (`"W"`, `"L-Shift"`, `"Knob"`).
    pub name: String,
}

/// A file the app knows, by token, with names for display only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FileRef {
    /// The token to refer to the file in later commands.
    pub token: FileToken,
    /// The file name.
    pub file_name: String,
    /// The full path, for display only.
    pub display_path: String,
}

/// Which keyboard something describes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeviceIdentityView {
    /// Model name (`"Wraith W75"`).
    pub model: Option<String>,
    /// Product code (`"W75HEARGB"`).
    pub product: Option<String>,
    /// Firmware (`"3.22.07"`).
    pub firmware: Option<String>,
    /// Firmware build date as the keyboard reports it.
    pub build_date: Option<String>,
    /// USB vendor ID.
    pub vendor_id: Option<u16>,
    /// USB product ID.
    pub product_id: Option<u16>,
}

/// Evidence of another program on the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ForeignActivityView {
    /// Status reports nobody here requested (the vendor site's heartbeat).
    pub status_reports: u32,
    /// Write acknowledgements of writes OpenWraith did not send.
    pub foreign_acks: u32,
    /// Profile reports OpenWraith did not cause.
    pub profile_reports: u32,
}

/// The trigger mode of a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TriggerModeView {
    /// Rapid Trigger off (`00`).
    Off,
    /// Rapid Trigger with one sensitivity (`0C`).
    RapidTrigger,
    /// Rapid Trigger with separate press and release (`0D`).
    RapidTriggerSeparate,
    /// Any other mode byte.
    Unknown,
}

/// Trigger settings of a key, in trigger units of 0.02 mm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TriggerValue {
    /// The mode.
    pub mode: TriggerModeView,
    /// The mode byte as read.
    pub mode_byte: u8,
    /// Actuation point.
    pub actuation: u16,
    /// Rapid Trigger press sensitivity.
    pub rt_press: u16,
    /// Rapid Trigger release sensitivity.
    pub rt_release: u16,
}

/// A main-light effect, by key for translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum LightModeKey {
    Static,
    Breath,
    Wave,
    Neon,
    Radar,
    Reactive,
    Aurora,
    Ripple,
    Twinkle,
    Custom,
    Cross,
    SpeedRespond,
    AutoRipple,
    Striation,
    Fireworks,
    MusicRhythm,
    Unknown,
}

/// A main-light mode: its id on the wire and its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LightModeView {
    /// The mode id.
    pub id: u8,
    /// The key for its name.
    pub key: LightModeKey,
}

/// The main light record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LightValue {
    /// The effect.
    pub mode: LightModeView,
    /// Brightness (0..max).
    pub brightness: u8,
    /// Speed (0..max).
    pub speed: u8,
    /// Foreground colour, `"#rrggbb"` lowercase.
    pub color: String,
    /// The "Full Color" flag (CODE-ONLY).
    pub full_color: bool,
    /// Whether the lighting is on (CODE-ONLY).
    pub power_on: bool,
}

/// A keymap layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum LayerView {
    /// The base layer.
    Base,
    /// The Fn layer.
    Fn,
}

/// Why the Fn keymap is re-sent as the keymap group's commit trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum CommitTriggerReasonView {
    /// The base keymap is written right before it (confirmed rule).
    BaseKeymap,
    /// Macro or advanced-key slots are written (precaution).
    MacrosOrAdvancedKeys,
}

/// A table of the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "table", rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum TableRef {
    Profile,
    MainLight,
    SideLight,
    ActiveCustomTable,
    /// Custom colour table 1 or 2.
    CustomColors {
        number: u8,
    },
    Keymap {
        layer: LayerView,
    },
    /// A macro slot; the library's slot 0..=9 (the UI shows slot + 1).
    Macro {
        slot: u8,
    },
    /// An advanced-key slot 0..=39 (the UI shows slot + 1).
    AdvancedKey {
        slot: u8,
    },
    Triggers,
    DeadBand,
    SocdPairs,
    SocdSettings,
    /// SOCD power and pairs, written together.
    Socd,
    SwitchTypes,
    PollRate,
    WinLockAndRollover,
}

/// Title and file-type filter of a file dialog, from the UI's dictionaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct DialogText {
    /// The dialog title.
    pub title: String,
    /// The name of the file-type filter.
    pub filter: String,
}

/// What a backup file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum BackupKind {
    /// The active profile.
    Active,
    /// All four profiles.
    AllProfiles,
}

/// What to show in the file manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "target", rename_all = "snake_case", deny_unknown_fields)]
pub enum RevealTarget {
    /// The safety-backup folder.
    SafetyDir,
    /// The license files shipped with the app (`LICENSE.txt` and
    /// `THIRD-PARTY-NOTICES.txt` in its installation folder).
    LegalNotices,
    /// A known file.
    File {
        /// The file.
        file: FileToken,
    },
}

/// The answer to the close guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum CloseChoice {
    /// Keep the app open; the operation goes on.
    KeepOpen,
    /// Stop at the next safe point, then close.
    StopAndClose,
}
