//! What the frontend asks for. Every intent is validated in Rust.

use serde::{Deserialize, Serialize};

use super::common::FileToken;

/// Change the trigger settings of some keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct TriggerEditIntent {
    /// Key indices (not the knob; 1 to 81 distinct keys).
    pub keys: Vec<u8>,
    /// New actuation point in trigger units, if it changes.
    pub actuation: Option<u16>,
    /// New Rapid Trigger setting, if it changes.
    pub rapid_trigger: Option<RtIntent>,
    /// The snapshot revision the user saw.
    pub based_on: u32,
}

/// A Rapid Trigger setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum RtIntent {
    /// Rapid Trigger off (`00`).
    Off,
    /// One sensitivity for press and release (`0C`).
    Same {
        /// Sensitivity in trigger units.
        sensitivity: u16,
    },
    /// Separate press and release sensitivity (`0D`).
    Separate {
        /// Press sensitivity in trigger units.
        press: u16,
        /// Release sensitivity in trigger units.
        release: u16,
    },
}

/// Change the main light (only the fields given).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct LightEditIntent {
    /// New mode id.
    pub mode: Option<u8>,
    /// New colour, `"#rrggbb"`.
    pub color: Option<String>,
    /// New brightness.
    pub brightness: Option<u8>,
    /// New speed.
    pub speed: Option<u8>,
    /// The snapshot revision the user saw.
    pub based_on: u32,
}

/// Restore a backup file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RestoreIntent {
    /// The backup.
    pub source: FileToken,
    /// The profiles to restore; `null` = every profile in the file.
    pub profiles: Option<Vec<u8>>,
    /// Restore although the firmware differs.
    pub allow_firmware_mismatch: bool,
    /// The profiles the user agreed to switch to for the comparison (every
    /// profile in scope except the active one).
    pub confirmed_switches: Vec<u8>,
}
