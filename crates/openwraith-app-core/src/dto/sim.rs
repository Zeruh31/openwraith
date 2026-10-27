//! The developer simulator's controls. The types exist in every build; the
//! commands that use them are registered only in simulator builds.

use serde::{Deserialize, Serialize};

/// Something to do to the simulated keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum SimAction {
    /// Unplug the keyboard (unsaved edits are lost).
    Unplug,
    /// Plug it in again.
    Replug,
    /// Another program's heartbeat (the vendor site's status query reply).
    VendorHeartbeat {
        /// How many.
        count: u8,
        /// The interval.
        interval_ms: u32,
    },
    /// A profile switch on the keyboard (Fn + 1..4).
    KeyboardSwitch {
        /// The profile.
        profile: u8,
        /// When, from now.
        after_ms: u32,
        /// Whether the keyboard reports it.
        report: bool,
    },
    /// The keyboard's reset notification (Fn + Space held).
    KeyboardReset,
    /// The keyboard saves only this long after a write (5 s loses writes).
    SlowSave {
        /// The commit delay.
        ms: u32,
    },
    /// The next write gets no acknowledgement.
    DropNextAck,
    /// Unplug after this many more packets.
    UnplugAfterPackets {
        /// Packets.
        packets: u16,
    },
}

/// The simulated keyboard's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SimInfo {
    /// Its active profile.
    pub active_profile: u8,
    /// Whether it holds edits not saved yet.
    pub unsaved_edits: bool,
    /// How often unsaved edits were discarded.
    pub discarded_edits: u32,
    /// Write packets received.
    pub writes_sent: u32,
    /// Packets it refused (must stay 0).
    pub violations: u32,
    /// Whether it is unplugged.
    pub unplugged: bool,
}
