//! Plans shown for confirmation (prepare results).

use serde::Serialize;

use super::common::{FileRef, KeyRef, LightValue, PlanToken, TriggerValue};

/// A planned trigger edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TriggerEditPreview {
    /// The token to apply it; `null` when there is nothing to do.
    pub token: Option<PlanToken>,
    /// The profile it writes to.
    pub profile: u8,
    /// Every key, in the requested order.
    pub keys: Vec<KeyEditView>,
    /// How many keys change.
    pub changing: u16,
    /// Write packets (one per changing key).
    pub packets: u16,
    /// Every key already has these settings.
    pub nothing_to_do: bool,
}

/// One key of a trigger edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeyEditView {
    /// The key.
    pub key: KeyRef,
    /// Its settings now.
    pub current: TriggerValue,
    /// Its settings after the edit.
    pub target: TriggerValue,
    /// Whether it changes.
    pub changes: bool,
}

/// A planned main-light edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LightEditPreview {
    /// The token to apply it; `null` when there is nothing to do.
    pub token: Option<PlanToken>,
    /// The profile it writes to.
    pub profile: u8,
    /// The light now.
    pub current: LightValue,
    /// The light after the edit.
    pub target: LightValue,
    /// Whether the effect changes (the direction is reset then).
    pub changes_mode: bool,
    /// Write packets (2 when the effect changes).
    pub packets: u8,
    /// The light already has these settings.
    pub nothing_to_do: bool,
}

/// A planned profile switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProfileSwitchPreview {
    /// The token to apply it; `null` when the profile is already active.
    pub token: Option<PlanToken>,
    /// The active profile.
    pub from: u8,
    /// The profile to switch to.
    pub to: u8,
    /// The profile is already active.
    pub nothing_to_do: bool,
    /// How long the switch first waits for the keyboard to save an earlier
    /// write.
    pub save_wait_ms: u32,
}

/// A planned all-profile backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AllProfilesPreview {
    /// The token to apply it.
    pub token: PlanToken,
    /// Where the backup is saved.
    pub destination: FileRef,
    /// The active profile (the backup returns to it).
    pub original: u8,
    /// The other profiles, switched to in this order.
    pub visits: Vec<u8>,
    /// How long the first switch waits for the keyboard to save an earlier
    /// write.
    pub save_wait_ms: u32,
}
