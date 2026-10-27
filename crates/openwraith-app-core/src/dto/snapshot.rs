//! The settings of the active profile as the pages show them.

use serde::Serialize;

use super::common::{DeviceIdentityView, KeyRef, LightModeView, LightValue, TriggerValue};

/// The settings of the active profile, read from the keyboard.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeviceSnapshot {
    /// Increases with every read; intents carry it as `based_on`.
    pub revision: u32,
    /// The connection it was read in.
    pub generation: u32,
    /// When it was read (RFC 3339).
    pub read_at: String,
    /// The profile it holds.
    pub profile: u8,
    /// The keyboard.
    pub device: DeviceIdentityView,
    /// The polling rate in Hz, if known.
    pub poll_rate_hz: Option<u32>,
    /// Actuation and Rapid Trigger.
    pub triggers: TriggerState,
    /// The main light.
    pub light: LightState,
    /// Replies that could not be decoded.
    pub undecoded: u16,
}

/// Actuation and Rapid Trigger of every key.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TriggerState {
    /// The keyboard's distance unit in micrometres (20 on the W75).
    pub unit_um: Option<u32>,
    /// Whether trigger settings can be changed.
    pub editable: bool,
    /// Why not.
    pub disabled: Option<TriggerDisabled>,
    /// The value ranges, in trigger units.
    pub limits: Option<TriggerLimitsView>,
    /// All 82 keys in index order (the knob included, not selectable).
    pub keys: Vec<KeyTriggerView>,
}

/// Why trigger settings cannot be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TriggerDisabled {
    /// The keyboard reports a distance unit other than 0.02 mm.
    DistanceUnit,
    /// The trigger parameters (`21/04`) are missing or unusable.
    LimitsUnknown,
    /// The keyboard is not a supported model.
    UnsupportedModel,
}

/// Value ranges of trigger settings, in trigger units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TriggerLimitsView {
    /// Smallest actuation point.
    pub min_actuation: u16,
    /// Largest actuation point.
    pub max_actuation: u16,
    /// Largest Rapid Trigger sensitivity (the smallest is 1).
    pub max_sensitivity: u16,
}

/// The trigger settings of one key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeyTriggerView {
    /// The key.
    pub key: KeyRef,
    /// Whether it can be selected (false for the knob).
    pub selectable: bool,
    /// Its settings, if read and decodable.
    pub value: Option<TriggerValue>,
    /// Why its settings cannot be changed, if so.
    pub not_editable: Option<NotEditable>,
}

/// Why the settings of a key cannot be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum NotEditable {
    /// The knob has no trigger settings.
    Knob,
    /// The key was not read.
    NotRead,
    /// Its reply cannot be decoded.
    Undecodable,
    /// Its mode is not one of `00`, `0C`, `0D`.
    UnknownMode {
        /// The mode byte.
        mode: u8,
    },
    /// Its second travel value differs from the actuation point.
    SecondTravelDiffers,
    /// Mode `0C` with different press and release values.
    PressReleaseDiffer,
}

/// The main light.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LightState {
    /// The current record, if read and decodable.
    pub current: Option<LightValue>,
    /// The modes that can be written, in device order, plus Custom.
    pub modes: Vec<LightModeView>,
    /// Largest brightness.
    pub max_brightness: u8,
    /// Largest speed.
    pub max_speed: u8,
    /// Whether the main light can be changed.
    pub editable: bool,
    /// Why not.
    pub disabled: Option<LightDisabled>,
}

/// Why the main light cannot be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum LightDisabled {
    /// The record was not read.
    NotRead,
    /// The record is not a valid 12-byte record.
    InvalidRecord,
    /// The mode list (`0A`) is missing or unusable.
    LimitsUnknown,
    /// The keyboard is not a supported model.
    UnsupportedModel,
}
