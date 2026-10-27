//! Progress of a running operation (one channel per operation).

use serde::Serialize;

use super::common::{KeyRef, OpId, TableRef};

/// One progress event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProgressEvent {
    /// The operation.
    pub op: OpId,
    /// Increases by one per event of the operation.
    pub seq: u32,
    /// Time since the operation started.
    pub elapsed_ms: u32,
    /// What is happening.
    pub step: ProgressStep,
    /// Whether Stop is possible now.
    pub cancel: CancelState,
}

/// What an operation is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum ProgressStep {
    /// Listening to make sure no other program uses the keyboard.
    CheckingOtherPrograms {
        /// How much longer.
        remaining_ms: u32,
    },
    /// Reading settings.
    Reading {
        /// Why.
        purpose: ReadPurpose,
        /// The profile, when several are read.
        profile: Option<u8>,
        /// Requests done.
        done: u16,
        /// Requests in total.
        total: u16,
    },
    /// Switching to a profile.
    SelectingProfile {
        /// The profile.
        profile: u8,
    },
    /// Switching back to the original profile.
    ReturningToProfile {
        /// The profile.
        profile: u8,
    },
    /// Comparing the backup with the keyboard.
    Comparing,
    /// Saving the safety backup.
    SavingSafetyBackup,
    /// Saving the backup file.
    SavingFile,
    /// Writing a table (`packet` 0 = its start).
    Writing {
        /// The profile.
        profile: u8,
        /// The table.
        table: TableRef,
        /// The key (trigger writes).
        key: Option<KeyRef>,
        /// This write's number (1-based) among the operation's writes.
        item: u16,
        /// Writes in the operation.
        items: u16,
        /// The packet about to be sent (0 = start of the table).
        packet: u16,
        /// Packets in the table.
        packets: u16,
        /// Which sending of the table (2 = sent again).
        send: u8,
    },
    /// Waiting for the keyboard to save.
    WaitingForKeyboardSave {
        /// The profile being saved, if known.
        profile: Option<u8>,
        /// How long the wait lasts.
        wait_ms: u32,
    },
    /// Reading a written profile again after a profile switch.
    CheckingPersistence {
        /// The profile.
        profile: u8,
    },
    /// Reading every profile again (final check).
    FinalCheck,
    /// Reading the changed values for the view.
    RefreshingView,
}

/// Why settings are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ReadPurpose {
    /// For the pages.
    Snapshot,
    /// For the safety backup.
    SafetyBackup,
    /// For a backup file.
    Backup,
    /// To compare with a backup.
    Compare,
}

/// Whether Stop is possible now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum CancelState {
    /// Stop now takes effect after the current step.
    Available,
    /// Not at this moment (waits and checks); later steps may allow it.
    NotNow,
    /// Stop was asked for.
    Requested,
    /// Stop was asked for; the keymap group's Fn keymap is still sent.
    FinishingKeymapGroup,
    /// The operation cannot be stopped.
    NotCancellable,
}
