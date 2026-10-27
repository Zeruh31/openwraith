//! Results of operations.

use serde::Serialize;

use super::common::{CommitTriggerReasonView, FileRef, ForeignActivityView, KeyRef, TableRef};
use super::error::AppError;

/// How an operation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum OutcomeView {
    Completed,
    NothingToDo,
    Cancelled,
    Failed,
}

/// The one-line verdict of a report (the CLI's precedence: nothing to do;
/// lost; may be lost; failed or not written; cancelled; done).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum Headline {
    Done,
    NothingToDo,
    Cancelled,
    NotWritten,
    Failed,
    MaybeLost,
    Lost,
}

/// What a trigger or light edit did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct EditReportView {
    /// The verdict.
    pub headline: Headline,
    /// How it ended.
    pub outcome: OutcomeView,
    /// The error, if it failed.
    pub error: Option<AppError>,
    /// The profile that was edited.
    pub profile: u8,
    /// The profile active now, if known.
    pub active_profile_now: Option<u8>,
    /// One entry per planned write.
    pub items: Vec<ItemReportView>,
    /// How long it waited at the end for the keyboard to save.
    pub waited_for_commit_ms: Option<u32>,
    /// The safety backup made before it (Undo restores it).
    pub safety_backup: FileRef,
    /// Another program seen while writing.
    pub foreign_activity: Option<ForeignActivityView>,
    /// Whether anything was written.
    pub anything_written: bool,
}

/// What a restore did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RestoreReportView {
    /// The verdict.
    pub headline: Headline,
    /// How it ended.
    pub outcome: OutcomeView,
    /// The error, if it failed.
    pub error: Option<AppError>,
    /// The profile active before the restore.
    pub original_profile: u8,
    /// The profile active now, if known.
    pub active_profile_now: Option<u8>,
    /// Per profile, in the order they were written.
    pub profiles: Vec<ProfileReportView>,
    /// The persistence check, if one was made.
    pub persistence: Option<PersistenceCheckView>,
    /// The final check, if one was made.
    pub final_check: Option<FinalCheckView>,
    /// Counts.
    pub counts: ReportCounts,
    /// The safety backup made before it (Undo restores it).
    pub safety_backup: FileRef,
    /// Another program seen while writing.
    pub foreign_activity: Option<ForeignActivityView>,
    /// Whether any table was written.
    pub anything_written: bool,
}

/// The results of one profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProfileReportView {
    /// The profile.
    pub profile: u8,
    /// One entry per planned write, in execution order.
    pub items: Vec<ItemReportView>,
}

/// The result of one planned write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ItemReportView {
    /// The table.
    pub table: TableRef,
    /// The key, for trigger writes.
    pub key: Option<KeyRef>,
    /// Set when this was the keymap group's commit trigger.
    pub commit_trigger: Option<CommitTriggerReasonView>,
    /// An unchanged Fn keymap re-sent as the commit trigger.
    pub resent_unchanged: bool,
    /// What happened.
    pub result: ItemResultView,
}

/// What happened to one planned write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ItemResultView {
    /// Written and verified.
    Written {
        /// Sends of the whole table (1 or 2).
        sends: u8,
        /// Packets sent.
        packets: u16,
        /// Noteworthy events.
        warnings: Vec<WriteWarningView>,
        /// Whether it was read again after a profile switch.
        persistence: PersistenceView,
    },
    /// Written and verified, but gone after a profile switch.
    Lost {
        /// Sends of the whole table.
        sends: u8,
        /// Packets sent.
        packets: u16,
        /// Noteworthy events.
        warnings: Vec<WriteWarningView>,
        /// What was read after the switch.
        mismatch: MismatchView,
    },
    /// Written and verified, but the keyboard may not have saved it.
    MaybeLost {
        /// Sends of the whole table.
        sends: u8,
        /// Packets sent.
        packets: u16,
        /// Noteworthy events.
        warnings: Vec<WriteWarningView>,
    },
    /// The keyboard already held it; nothing was written.
    AlreadyMatched,
    /// Failed.
    Failed {
        /// Why.
        error: AppError,
        /// The last read-back difference.
        mismatch: Option<MismatchView>,
    },
    /// Not written.
    NotAttempted,
}

/// Whether a write was read again after a profile switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum PersistenceView {
    /// Not checked (edits rely on the wait).
    NotChecked,
    /// Kept after a profile switch.
    Persisted,
}

/// A read-back that did not match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MismatchView {
    /// The table.
    pub table: TableRef,
    /// The read (technical text).
    pub read: String,
    /// What went wrong.
    pub kind: MismatchKindView,
    /// Offsets that differ (at most 64).
    pub offsets: Vec<u16>,
    /// Expected bytes (hex).
    pub expected_hex: String,
    /// Bytes read (hex).
    pub got_hex: String,
}

/// What went wrong with a read-back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum MismatchKindView {
    MissingReply,
    InvalidReply,
    Differs,
}

/// Something noteworthy during a write that still succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "code", rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum WriteWarningView {
    AckTimeout { packet: u16, of: u16, send: u8 },
    OptionalAckMissing { packet: u16 },
    VerifyReread,
    Retried,
    SelectAckMissing { profile: u8 },
    ProfilePushMissing { profile: u8 },
    SelectRetried { profile: u8 },
    WaitedForLateAck,
    WaitedForCommit { waited_ms: u32 },
    BaseKeymapNotSaved,
}

/// The persistence check of a restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PersistenceCheckView {
    /// The profiles read again, in order.
    pub profiles: Vec<u8>,
    /// Tables kept.
    pub persisted: u16,
    /// Tables lost.
    pub lost: u16,
    /// Tables that could not be checked separately.
    pub not_checked: u16,
}

/// The final check of a restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FinalCheckView {
    /// The profiles read.
    pub profiles: Vec<u8>,
    /// Tables that no longer hold the backup's bytes.
    pub mismatches: Vec<FinalMismatchView>,
}

/// A difference found by the final check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FinalMismatchView {
    /// The profile.
    pub profile: u8,
    /// The difference.
    pub mismatch: MismatchView,
}

/// Counts of a restore report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[allow(missing_docs)]
pub struct ReportCounts {
    pub written: u16,
    pub persisted: u16,
    pub lost: u16,
    pub maybe_lost: u16,
    pub failed: u16,
    pub not_attempted: u16,
    pub already_matched: u16,
    pub resent_unchanged: u16,
    pub skipped: u16,
}

/// What a profile switch did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProfileSwitchReport {
    /// The profile before.
    pub from: u8,
    /// The profile asked for.
    pub to: u8,
    /// The profile active now, if known.
    pub active_profile_now: Option<u8>,
    /// How long it waited first for the keyboard to save.
    pub waited_for_commit_ms: Option<u32>,
    /// Noteworthy events.
    pub warnings: Vec<WriteWarningView>,
}

/// A saved backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BackupSaved {
    /// The file.
    pub file: FileRef,
    /// The profiles it holds.
    pub profiles: Vec<u8>,
    /// When it was made (RFC 3339).
    pub created_at: String,
    /// The keyboard's firmware.
    pub firmware: Option<String>,
    /// Replies that could not be decoded.
    pub undecoded: u16,
    /// Another program seen while reading.
    pub foreign_activity: Option<ForeignActivityView>,
}

/// The last report, for a reloaded window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LastReport {
    /// A trigger or light edit.
    Edit {
        /// The report.
        report: EditReportView,
    },
    /// A restore.
    Restore {
        /// The report.
        report: RestoreReportView,
    },
    /// A profile switch.
    ProfileSwitch {
        /// The report.
        report: ProfileSwitchReport,
    },
    /// A backup.
    Backup {
        /// The saved file.
        saved: BackupSaved,
    },
}
