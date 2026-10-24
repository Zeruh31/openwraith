//! Restoring a backup: planning, preparation (with a safety backup) and
//! execution with verification.
//!
//! # Flow
//!
//! 1. [`prepare_restore`] checks the backup against the keyboard (model;
//!    firmware unless allowed), reads every profile the restore touches plus
//!    the active one as the **safety backup** (switching profiles with
//!    `21/12` and back if more than one is needed) and plans the restore
//!    ([`plan_restore`]). Nothing but profile switches is written.
//! 2. The caller saves the safety backup with [`Backup::save_safety`]
//!    (which verifies the saved file), shows the plan and obtains consent.
//! 3. [`execute_restore`] writes the plan. It needs the [`SavedBackup`] of
//!    the prepared safety backup. For each profile with changes (the
//!    originally active one first, then ascending) it selects the profile,
//!    and for each table confirms the active profile, re-reads exactly the
//!    bytes the write changes, skips the write when the keyboard already
//!    holds the target (a table that is global and was written in an earlier
//!    profile), aborts with `changed_since_plan` when the bytes are neither
//!    the planned state nor something this restore wrote, and otherwise
//!    writes it through the [`Writer`] (stop-and-wait, read-back
//!    verification, one more attempt with the whole table). Before each
//!    profile switch it waits for the keyboard to save
//!    ([`CommitSettle`](crate::CommitSettle)); a cancellation asked for
//!    during that wait stops it before the next profile is selected. When
//!    everything was written, the persistence check selects every written
//!    profile again after switching away and reads the written tables again
//!    ([`PersistenceCheck`]). A profile change or a lost connection before
//!    the keyboard saved turns the last profile's writes into
//!    [`ItemResult::MaybeLost`]. Finally it
//!    selects the original profile again and, when a table whose profile
//!    scope is unknown was written and the plan covers several profiles,
//!    reads every profile again to check that each still holds the backup's
//!    bytes. Failures are reported in the [`RestoreReport`]; there is never
//!    an automatic rollback: the undo command restores the safety backup.
//!
//! [`dry_run_restore`] plans against a plain read of the active profile and
//! never writes; [`plan_restore`] plans offline against another backup.
//!
//! # What is written
//!
//! Exactly the backup's bytes, table by table (never re-encoded from a
//! decoded model). The only normalisation: an empty macro slot (all `00` or
//! all `FF`) is written as 256 x `00`. Defaults the backup does not contain
//! (knob or Fn keymap defaults) are never written. Tables without a confirmed
//! write command (side light, custom colours 2, active custom table, SOCD,
//! switch types, polling rate, Win-Lock / 6-key) are compared and reported
//! as skipped when they differ. The keymap group (both keymap layers, the
//! ten macro slots and the forty advanced-key slots) is planned as one unit
//! and skipped as a whole when it would need a non-empty advanced-key write
//! (format CODE-ONLY), a non-canonical macro, or holds a malformed reference.
//!
//! Trigger and dead-band values are stored in the keyboard's distance unit
//! (`21/04` unit x status high-precision flag). They are only restored when
//! the backup and the keyboard both use the 0.02 mm unit; otherwise the same
//! numbers would mean other distances, and they are skipped
//! ([`SkipReason::DistanceUnitChanged`]). Values are never converted.

use std::collections::{BTreeSet, HashMap};
use std::ops::ControlFlow;
use std::path::PathBuf;

use openwraith_protocol::{
    AdvancedSlotId, ColorTableData, CustomTable, DeadBandData, DeadBandWrite, Distance, HexBytes,
    KeyMask, KeyPos, KeymapData, KeymapReference, Layer, LightLimits, LightRecord, MacroData,
    MacroSlotId, MainLightWrite, Mismatch, MismatchKind, Model, ProfileId, RawResponse,
    ReadRequest, TableId, TriggerLimits, TriggerSetting, TriggerWrite, WriteBuildError,
    WriteRequest,
};
use serde::Serialize;

use crate::backup::{Backup, BackupOptions, Progress, SnapshotView, read_all_cancellable};
use crate::error::{ErrorInfo, ErrorKind, RestoreError, WriteError};
use crate::profiles::{
    ProfileSet, ProfilesOptions, ProfilesProgress, read_other_profiles, read_profiles,
};
use crate::safety::SavedBackup;
use crate::session::{ForeignActivity, Session};
use crate::summary::identify;
use crate::transport::{Transport, WriteTransport};
use crate::writer::{CommitWait, PacketProgress, WriteConsent, WriteOptions, WriteWarning, Writer};

/// The active profile after [`Session::writer`] failed: unknown after a
/// reset or a lost connection; the profile another program or the keyboard
/// reported if it reported one; else `fallback` (the profile last confirmed
/// before, nothing having been sent since).
pub(crate) fn profile_after_gate_failure<T: Transport>(
    session: &Session<T>,
    error: &WriteError,
    fallback: Option<u8>,
) -> Option<u8> {
    match error {
        WriteError::KeyboardReset { .. } | WriteError::Disconnected { .. } => None,
        WriteError::ProfileChanged { reported, .. }
        | WriteError::ProfileChangedBeforeSave { reported, .. } => Some(*reported),
        _ => session.last_foreign_profile_report().or(fallback),
    }
}

/// Bytes per advanced-key slot in the `22 80` read.
const ADVANCED_SLOT_LEN: usize = 40;

/// The bytes a write changes, one entry per verify read (`None` when the
/// reply is missing or unusable).
pub(crate) type TableState = Vec<Option<Vec<u8>>>;

/// Options of a restore.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreOptions {
    /// The profiles to restore; `None` = every profile in the backup.
    pub profiles: Option<ProfileSet>,
    /// Restore even when the keyboard runs other firmware than the backup was
    /// made with (the main use case is restoring after a firmware update;
    /// table formats may differ, so it must be allowed explicitly).
    pub allow_firmware_mismatch: bool,
    /// Tool and time recorded in the safety backup.
    pub backup: BackupOptions,
    /// Writer timeouts.
    pub write: WriteOptions,
}

/// Progress of a restore or an edit. Returning [`ControlFlow::Break`] from
/// the callback cancels at the next table boundary; the original profile is
/// always selected again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum WriteProgress {
    /// Reading the safety backup.
    SafetyBackup {
        /// What is being read.
        progress: ProfilesProgress,
    },
    /// Comparing the backup with the keyboard.
    Planning,
    /// Writing a table: `packet` 0 marks the start of the table (a
    /// cancellation point), then 1..=`of` for each packet. `send` is 2 while
    /// the whole table is sent a second time (after a failed verification).
    Writing {
        /// The profile.
        profile: u8,
        /// The table.
        table: TableId,
        /// The key (trigger writes).
        #[serde(skip_serializing_if = "Option::is_none")]
        key: Option<KeyPos>,
        /// The packet about to be sent (0 = start of the table).
        packet: usize,
        /// Packets in the table.
        of: usize,
        /// Which sending of the table (1, or 2 for the retry).
        send: u32,
    },
    /// Reading every written profile again to check the result.
    Verifying,
    /// Selecting the original profile again.
    Returning {
        /// The original profile.
        profile: u8,
    },
    /// Waiting for the keyboard to save the last write before switching
    /// profiles or finishing (it saves about 1 s after the last write; see
    /// [`CommitSettle`](crate::CommitSettle)). The wait itself cannot be
    /// cancelled (the keyboard must have saved before any switch, also the
    /// one back to the original profile). A restore waits before it selects
    /// the next profile, and checks for a cancellation right after the wait:
    /// a cancellation asked for here or during the wait stops it before that
    /// profile is selected.
    WaitingForCommit {
        /// The profile whose edits are being saved (the profile the last
        /// write went to).
        #[serde(skip_serializing_if = "Option::is_none")]
        profile: Option<u8>,
        /// How long the wait lasts, in milliseconds.
        wait_ms: u64,
    },
    /// The persistence check of a restore: selecting `profile` again after
    /// switching away, and reading back what was written to it. Not a
    /// cancellation point.
    CheckingPersistence {
        /// The profile.
        profile: u8,
    },
}

impl WriteProgress {
    /// The progress event of a [`CommitWait`].
    pub(crate) fn waiting(wait: CommitWait) -> Self {
        WriteProgress::WaitingForCommit {
            profile: wait.profile,
            wait_ms: wait.wait_ms,
        }
    }
}

/// A restore plan: what would be written, per profile and table. For
/// display; it cannot be executed on its own (see [`PreparedRestore`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestorePlan {
    /// Product code of the keyboard.
    pub product: Option<String>,
    /// Creation time of the backup that is restored.
    pub backup_created_at: String,
    /// When the keyboard's state was read.
    pub keyboard_read_at: String,
    /// Firmware in the backup.
    pub backup_firmware: Option<String>,
    /// Firmware of the keyboard.
    pub keyboard_firmware: Option<String>,
    /// The profile that was active when the keyboard was read.
    pub original_profile: Option<u8>,
    /// One entry per profile in scope, ascending.
    pub profiles: Vec<ProfilePlan>,
    /// Differences that do not block the restore.
    pub warnings: Vec<PlanWarning>,
}

impl RestorePlan {
    /// Every planned write with its profile, in execution order within each
    /// profile.
    pub fn writes(&self) -> impl Iterator<Item = (ProfileId, &PlanItem, &PlannedWrite)> {
        self.profiles.iter().flat_map(|p| {
            p.items.iter().filter_map(move |item| match &item.action {
                PlanAction::Write(write) => Some((p.profile, item, write)),
                _ => None,
            })
        })
    }

    /// Whether anything would be written.
    pub fn has_writes(&self) -> bool {
        self.writes().next().is_some()
    }

    /// Number of table writes.
    pub fn write_count(&self) -> usize {
        self.writes().count()
    }

    /// Number of write packets (excluding profile switches).
    pub fn packet_count(&self) -> usize {
        self.writes().map(|(_, _, w)| w.packets).sum()
    }

    /// Every skipped item with its profile.
    pub fn skips(&self) -> impl Iterator<Item = (ProfileId, &PlanItem)> {
        self.profiles.iter().flat_map(|p| {
            p.items
                .iter()
                .filter(|item| matches!(item.action, PlanAction::Skip { .. }))
                .map(move |item| (p.profile, item))
        })
    }

    /// The profiles that were not compared (dry run against one profile).
    pub fn not_compared(&self) -> Vec<ProfileId> {
        self.profiles
            .iter()
            .filter(|p| !p.compared)
            .map(|p| p.profile)
            .collect()
    }
}

/// The plan for one profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfilePlan {
    /// The profile.
    pub profile: ProfileId,
    /// Whether the keyboard's state of this profile was compared. A dry run
    /// reads only the active profile; the others are not compared.
    pub compared: bool,
    /// Every table, in execution order (writes) followed by the tables that
    /// cannot be written.
    pub items: Vec<PlanItem>,
}

/// One table (or, for triggers, one key) of a profile plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanItem {
    /// The table.
    pub table: TableId,
    /// The key, for per-key trigger items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<KeyPos>,
    /// What happens to it.
    pub action: PlanAction,
}

/// What happens to a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PlanAction {
    /// The keyboard already holds the backup's bytes.
    Unchanged,
    /// The table is written.
    Write(PlannedWrite),
    /// The table differs but is not written.
    Skip {
        /// Machine-readable reason.
        reason: SkipReason,
        /// Human-readable explanation.
        message: String,
    },
    /// The backup has no reply for this table.
    NotInBackup,
}

/// A planned write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedWrite {
    /// The request (not serialized; see `kind` and `summary`).
    #[serde(skip)]
    pub request: WriteRequest,
    /// Request kind, e.g. `"keymap"`.
    pub kind: &'static str,
    /// Human-readable description, e.g. `base keymap (18 00)`.
    pub summary: String,
    /// Packets the write sends (its first attempt).
    pub packets: usize,
    /// The entries that change (key index, colour slot, macro slot, ...),
    /// with the keyboard's bytes and the backup's bytes.
    pub changes: Vec<EntryChange>,
}

/// One changed entry of a planned write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryChange {
    /// Entry index: the key index (keymap, dead band, trigger), the colour
    /// slot, the macro or advanced-key slot, or 0 for the light record.
    pub index: usize,
    /// The keyboard's bytes (empty for an empty macro or advanced-key slot).
    pub old: HexBytes,
    /// The backup's bytes (empty for an empty macro or advanced-key slot).
    pub new: HexBytes,
}

/// Why a differing table is not written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum SkipReason {
    /// The write command exists but is not confirmed on hardware.
    UnconfirmedCommand {
        /// The command, e.g. `08 00`.
        command: &'static str,
    },
    /// No write command is known for this table.
    NoKnownCommand,
    /// The backup's light mode is not in the keyboard's mode list.
    UnsupportedLightMode {
        /// The mode ID.
        mode: u8,
    },
    /// The backup's light record has a value out of the keyboard's range.
    InvalidLightRecord,
    /// The knob has no trigger settings a user can change.
    Knob,
    /// The backup's trigger mode is not one of `00`, `0C`, `0D`.
    UnknownTriggerMode {
        /// The mode byte.
        mode: u8,
    },
    /// The backup's second travel value differs from the actuation point
    /// (never captured; cannot be written exactly).
    SecondTravelDiffers,
    /// Mode `0C` with different press and release sensitivity (cannot be
    /// written exactly).
    PressReleaseDiffer,
    /// A value is outside the keyboard's current range.
    OutOfRange,
    /// The keyboard's trigger parameters (`21/04`) are missing or unusable.
    TriggerLimitsUnknown,
    /// A dead-band value is above the maximum.
    DeadBandOutOfRange,
    /// The keyboard's distance unit is unknown.
    DeadBandUnitUnknown,
    /// The backup's distance unit differs from the keyboard's, or one of
    /// them is unknown or not 0.02 mm: the stored numbers would mean other
    /// distances (they are never converted).
    DistanceUnitChanged {
        /// The backup's unit in micrometres, if known.
        backup_um: Option<u32>,
        /// The keyboard's unit in micrometres, if known.
        keyboard_um: Option<u32>,
    },
    /// The keymap group cannot be restored as a whole.
    KeymapGroupBlocked {
        /// Why.
        cause: BlockCause,
    },
    /// The keyboard did not answer this read.
    NotInKeyboard,
    /// The backup's reply cannot be used (wrong chunk lengths or size).
    Unusable,
}

/// Why the keymap group (keymaps, macros, advanced keys) is not restored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockCause {
    /// An advanced-key slot of the backup is not empty and differs from the
    /// keyboard: non-empty advanced-key writes are CODE-ONLY (the 50-byte
    /// write format differs from the 40-byte read format).
    NonEmptyAdvancedKey {
        /// The slot.
        slot: u8,
    },
    /// A macro slot that must change is not in the canonical flat layout.
    NonCanonicalMacro {
        /// The slot.
        slot: u8,
        /// Which rule it breaks.
        reason: &'static str,
    },
    /// A keymap of the backup holds a malformed macro or advanced-key
    /// reference.
    MalformedReference {
        /// The layer.
        layer: Layer,
        /// The key index.
        index: u8,
    },
    /// A table of the group is missing or unusable in the backup or on the
    /// keyboard.
    Incomplete {
        /// The table.
        table: TableId,
    },
}

/// A difference that does not block the restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "warning", rename_all = "snake_case")]
pub enum PlanWarning {
    /// The firmware differs (the mismatch was allowed).
    FirmwareDiffers {
        /// Firmware in the backup.
        backup: Option<String>,
        /// Firmware of the keyboard.
        keyboard: Option<String>,
    },
    /// A device constant (mode lists, Fn-locked keys, trigger parameters,
    /// switch-type list) differs; it is never written.
    ConstantDiffers {
        /// The read whose reply differs.
        request: ReadRequest,
    },
    /// The capability flags (status bytes 17-18) differ.
    CapabilityFlagsDiffer,
    /// Tables whose profile scope is unknown (they may be shared by all
    /// profiles) are written, and the restore does not cover every profile:
    /// the other profiles may change too.
    SharedTablesMayChangeOtherProfiles {
        /// The tables written.
        tables: Vec<TableId>,
        /// The profiles the restore does not cover.
        profiles: Vec<u8>,
    },
    /// The backup holds different copies of a table whose scope is unknown,
    /// while the keyboard holds the same copy in every profile it was read
    /// in: the table is probably shared, and the backup cannot be restored
    /// exactly (the final check will report it).
    LikelySharedTableDiffers {
        /// The table.
        table: TableId,
    },
}

/// Compares a backup with the keyboard's state (another backup) and plans
/// the restore. Pure: nothing is read or written.
///
/// `current` is the keyboard's state: the safety backup of
/// [`prepare_restore`], a plain read for a dry run, or a backup file for an
/// offline plan. Profiles that `current` does not hold are listed as not
/// compared.
pub fn plan_restore(
    target: &Backup,
    current: &Backup,
    options: &RestoreOptions,
) -> Result<RestorePlan, RestoreError> {
    let (model, scope) = check_target(target, options)?;
    let mut warnings = check_keyboard(
        target,
        current.device.product.as_deref(),
        current.device.firmware.as_deref(),
        options.allow_firmware_mismatch,
    )?;
    warnings.extend(constant_warnings(target.primary(), current.primary()));

    let mut profiles = Vec::new();
    for profile in scope.iter() {
        let Some(t) = target.snapshot(profile) else {
            return Err(RestoreError::ProfileNotInBackup {
                profile: profile.get(),
            });
        };
        let plan = match current.snapshot(profile) {
            Some(c) => ProfilePlan {
                profile,
                compared: true,
                items: plan_profile(model, t, c),
            },
            None => ProfilePlan {
                profile,
                compared: false,
                items: Vec::new(),
            },
        };
        profiles.push(plan);
    }
    warnings.extend(scope_warnings(target, current, &profiles));
    Ok(RestorePlan {
        product: current.device.product.clone(),
        backup_created_at: target.created_at.clone(),
        keyboard_read_at: current.created_at.clone(),
        backup_firmware: target.device.firmware.clone(),
        keyboard_firmware: current.device.firmware.clone(),
        original_profile: current.active_profile,
        profiles,
        warnings,
    })
}

/// Reads the active profile (read-only, no profile switch) and plans the
/// restore against it. Other profiles are not compared. Generic over
/// [`Transport`]: it cannot write.
pub fn dry_run_restore<T: Transport>(
    session: &mut Session<T>,
    target: &Backup,
    options: &RestoreOptions,
    progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<RestorePlan, RestoreError> {
    check_target(target, options)?;
    let current = read_all_cancellable(session, &options.backup, progress)?;
    plan_restore(target, &current, options)
}

/// A restore that is ready to be executed: the safety backup that was read
/// and the plan made against it. Not serializable and not deserializable:
/// only [`prepare_restore`] creates one.
#[derive(Debug, Clone)]
pub struct PreparedRestore {
    safety: Backup,
    target: Backup,
    plan: RestorePlan,
    original_profile: ProfileId,
    options: RestoreOptions,
}

impl PreparedRestore {
    /// The safety backup (save it with [`Backup::save_safety`]).
    pub fn safety(&self) -> &Backup {
        &self.safety
    }

    /// The plan.
    pub fn plan(&self) -> &RestorePlan {
        &self.plan
    }

    /// The profile that was active before the restore.
    pub fn original_profile(&self) -> ProfileId {
        self.original_profile
    }

    /// The options the restore was prepared with; [`execute_restore`] uses
    /// the same ones (its writer timeouts included).
    pub fn options(&self) -> &RestoreOptions {
        &self.options
    }
}

/// Checks `target` against the keyboard, reads the safety backup (the
/// active profile and every profile in scope, switching profiles and back
/// when needed; `consent` covers these switches) and plans the restore.
///
/// Nothing but profile switches is written. When only the active profile is
/// in scope, nothing at all is written and the safety backup is a
/// single-profile backup.
pub fn prepare_restore<T: WriteTransport>(
    session: &mut Session<T>,
    target: &Backup,
    options: &RestoreOptions,
    consent: WriteConsent,
    mut progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> Result<PreparedRestore, RestoreError> {
    let (_, scope) = check_target(target, options)?;
    let (_, info, _) = identify(session)?;
    check_keyboard(
        target,
        info.product.as_deref(),
        info.firmware.as_deref(),
        options.allow_firmware_mismatch,
    )?;
    let profiles_options = ProfilesOptions {
        backup: options.backup.clone(),
        write: options.write.clone(),
    };
    let safety = read_profiles(session, scope, consent, &profiles_options, |p| {
        progress(WriteProgress::SafetyBackup { progress: p })
    })?;
    if progress(WriteProgress::Planning).is_break() {
        return Err(RestoreError::Cancelled);
    }
    let plan = plan_restore(target, &safety, options)?;
    let original_profile = safety
        .active_profile
        .and_then(ProfileId::new)
        .ok_or_else(|| RestoreError::InvalidBackup {
            reason: "the keyboard did not report an active profile".to_string(),
        })?;
    Ok(PreparedRestore {
        safety,
        target: target.clone(),
        plan,
        original_profile,
        options: options.clone(),
    })
}

/// How a restore or an edit ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Every planned write was made and verified.
    Completed,
    /// There was nothing to write.
    NothingToDo,
    /// The user cancelled; the remaining tables were not written.
    Cancelled,
    /// Something failed; see the error and the items.
    Failed,
}

/// Whether a written table was seen to survive a profile switch.
///
/// A read-back right after a write reads the keyboard's working copy, which
/// it saves about 1 s later (spec 4.5.3, "Delayed commit"); only a read after
/// switching profiles away and back shows what the keyboard saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Persistence {
    /// Not read again after a profile switch. Edits of the active profile
    /// are not (switching would change the lighting); the operation waited
    /// for the [`CommitSettle`](crate::CommitSettle) instead. A restore that
    /// was cancelled or failed does not check either. A write whose save was
    /// interrupted is reported as [`ItemResult::MaybeLost`] instead.
    NotChecked,
    /// Read again after switching profiles away and back: it still held what
    /// was written, so the keyboard saved it.
    Persisted,
}

/// What happened to one planned write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ItemResult {
    /// Written and verified by reading it back.
    Written {
        /// How often the whole table was sent (1 or 2).
        sends: u32,
        /// Packets sent.
        packets: usize,
        /// Everything noteworthy.
        warnings: Vec<WriteWarning>,
        /// Whether it was read again after a profile switch.
        persistence: Persistence,
    },
    /// Written and verified by reading it back, but read again after a
    /// profile switch it no longer held what was written: the keyboard did
    /// not keep it (spec 4.5.3, "Delayed commit"); `mismatch` shows what it
    /// holds instead. The restore failed.
    Lost {
        /// How often the whole table was sent (1 or 2).
        sends: u32,
        /// Packets sent.
        packets: usize,
        /// Everything noteworthy about the write.
        warnings: Vec<WriteWarning>,
        /// What was read after the profile switch.
        mismatch: Mismatch,
    },
    /// Written and verified by reading it back, but before the keyboard had
    /// had the commit settle to save it, the active profile changed (on the
    /// keyboard or by another program) or the connection was lost
    /// ([`Writer::commit_interrupted`]): the keyboard may have discarded it
    /// (spec 4.5.3, "Delayed commit"). It was not read again; the operation
    /// failed.
    MaybeLost {
        /// How often the whole table was sent (1 or 2).
        sends: u32,
        /// Packets sent.
        packets: usize,
        /// Everything noteworthy about the write.
        warnings: Vec<WriteWarning>,
    },
    /// The keyboard already held the target bytes when the table was about
    /// to be written (for example a global table written in an earlier
    /// profile).
    AlreadyMatched,
    /// Failed; later tables were not attempted.
    Failed {
        /// The error.
        error: ErrorInfo,
        /// The last read-back difference, if the write could not be verified.
        #[serde(skip_serializing_if = "Option::is_none")]
        mismatch: Option<Mismatch>,
    },
    /// Not attempted (an earlier table failed, or the operation was
    /// cancelled).
    NotAttempted,
}

/// The result of one planned write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemReport {
    /// The table.
    pub table: TableId,
    /// The key, for trigger writes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<KeyPos>,
    /// Human-readable description of the write.
    pub summary: String,
    /// What happened.
    pub result: ItemResult,
}

/// The results for one profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileReport {
    /// The profile.
    pub profile: u8,
    /// One entry per planned write, in execution order.
    pub items: Vec<ItemReport>,
}

/// A difference found by the final check after a restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinalMismatch {
    /// The profile.
    pub profile: u8,
    /// The difference.
    pub mismatch: Mismatch,
}

/// The final check: every profile in scope read again after a table whose
/// profile scope is unknown was written (and the plan compares several
/// profiles). It compares every written table and every unknown-scope table
/// the plan left unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinalCheck {
    /// The profiles read.
    pub profiles: Vec<u8>,
    /// Tables that no longer hold the backup's bytes (a global table written
    /// with different contents in two profiles, or another program).
    pub mismatches: Vec<FinalMismatch>,
}

/// The persistence check after a restore: every profile with written tables
/// was selected again after switching away from it (so that the keyboard
/// reloaded what it had saved), and its written tables were read again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersistenceCheck {
    /// The profiles read again, in order.
    pub profiles: Vec<u8>,
    /// Written tables that still held what was written
    /// ([`Persistence::Persisted`]).
    pub persisted: usize,
    /// Written tables that did not ([`ItemResult::Lost`]).
    pub lost: usize,
    /// Written tables that could not be checked: a table that may be shared
    /// by all profiles and holds what this restore wrote to it in another
    /// profile ([`Persistence::NotChecked`]; the final check compares it
    /// with the backup).
    pub not_checked: usize,
}

/// What [`execute_restore`] did. Serializable for a GUI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreReport {
    /// How it ended.
    pub outcome: Outcome,
    /// The error, if it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    /// How the user agreed.
    pub consent: WriteConsent,
    /// The profile that was active before the restore.
    pub original_profile: u8,
    /// The profile that is active now, as far as known.
    pub active_profile_now: Option<u8>,
    /// Per profile, in the order they were written.
    pub profiles: Vec<ProfileReport>,
    /// The persistence check, if one was made (after every planned write
    /// was made).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persistence: Option<PersistenceCheck>,
    /// The final check, if one was made.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_check: Option<FinalCheck>,
    /// The safety backup made before the restore.
    pub safety_backup: PathBuf,
    /// The command that undoes the restore.
    pub undo_command: String,
    /// Reports of another program seen while writing (the operation stops
    /// after the table during which they arrived; seen during the last table,
    /// they only appear here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreign_activity: Option<ForeignActivity>,
}

impl RestoreReport {
    /// Whether every planned write was made and verified (skipped tables
    /// allowed).
    pub fn is_success(&self) -> bool {
        matches!(self.outcome, Outcome::Completed | Outcome::NothingToDo)
    }

    /// Every item with its profile.
    pub fn items(&self) -> impl Iterator<Item = (u8, &ItemReport)> {
        self.profiles
            .iter()
            .flat_map(|p| p.items.iter().map(move |i| (p.profile, i)))
    }

    /// Number of tables written and verified, not counting the ones the
    /// keyboard did not keep or may not have saved ([`lost`](Self::lost),
    /// [`maybe_lost`](Self::maybe_lost)).
    pub fn written(&self) -> usize {
        self.items()
            .filter(|(_, i)| matches!(i.result, ItemResult::Written { .. }))
            .count()
    }

    /// Whether any table was written (profile switches aside), also when
    /// the keyboard did not keep it or may not have saved it: the keyboard
    /// received write packets, so its settings may differ from before.
    pub fn anything_written(&self) -> bool {
        self.items().any(|(_, i)| i.result.was_sent())
    }

    /// Number of tables that were written and verified but, read again
    /// after a profile switch, no longer held what was written
    /// ([`ItemResult::Lost`]).
    pub fn lost(&self) -> usize {
        self.items()
            .filter(|(_, i)| matches!(i.result, ItemResult::Lost { .. }))
            .count()
    }

    /// Number of tables that were written and verified but may not have
    /// been saved ([`ItemResult::MaybeLost`]).
    pub fn maybe_lost(&self) -> usize {
        self.items()
            .filter(|(_, i)| matches!(i.result, ItemResult::MaybeLost { .. }))
            .count()
    }
}

impl ItemResult {
    /// Whether the table was written (sent and verified by reading it
    /// back), whatever became of it afterwards.
    pub fn was_sent(&self) -> bool {
        matches!(
            self,
            ItemResult::Written { .. } | ItemResult::Lost { .. } | ItemResult::MaybeLost { .. }
        )
    }

    /// A written item whose save is unconfirmed ([`Persistence::NotChecked`])
    /// as [`ItemResult::MaybeLost`]; any other result unchanged.
    pub(crate) fn into_maybe_lost(self) -> ItemResult {
        match self {
            ItemResult::Written {
                sends,
                packets,
                warnings,
                persistence: Persistence::NotChecked,
            } => ItemResult::MaybeLost {
                sends,
                packets,
                warnings,
            },
            other => other,
        }
    }
}

/// Executes a prepared restore (see the [module documentation](self)), with
/// the options it was prepared with ([`PreparedRestore::options`]).
///
/// `saved` must be the saved copy of [`PreparedRestore::safety`]. Never
/// panics and never rolls back; failures, cancellation and the final state
/// are described in the returned report. `active_profile_now` is only set
/// from what the keyboard reported (`None` when nothing was read).
pub fn execute_restore<T: WriteTransport>(
    session: &mut Session<T>,
    prepared: &PreparedRestore,
    saved: &SavedBackup,
    consent: WriteConsent,
    mut progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> RestoreReport {
    let original = prepared.original_profile;
    let order = execution_order(&prepared.plan, original);
    let mut report = RestoreReport {
        outcome: Outcome::Completed,
        error: None,
        consent,
        original_profile: original.get(),
        active_profile_now: None,
        profiles: order
            .iter()
            .map(|profile| ProfileReport {
                profile: profile.get(),
                items: prepared
                    .plan
                    .writes()
                    .filter(|(p, _, _)| p == profile)
                    .map(|(_, item, write)| ItemReport {
                        table: item.table,
                        key: item.key,
                        summary: write.summary.clone(),
                        result: ItemResult::NotAttempted,
                    })
                    .collect(),
            })
            .collect(),
        persistence: None,
        final_check: None,
        safety_backup: saved.path().to_path_buf(),
        undo_command: saved.undo_command(),
        foreign_activity: None,
    };
    if !saved.describes(&prepared.safety) {
        let error = RestoreError::SafetyBackupMismatch {
            path: saved.path().to_path_buf(),
        };
        report.fail(ErrorInfo::new(error.kind(), &error));
        return report;
    }
    if order.is_empty() {
        report.outcome = Outcome::NothingToDo;
        return report;
    }
    let mut writer = match session.writer_with(consent, prepared.options.write.clone()) {
        Ok(writer) => writer,
        Err(e) => {
            report.fail(ErrorInfo::new(e.kind(), &e));
            // Nothing was sent; the profile is the one prepare_restore
            // returned to, unless the keyboard reported another one.
            report.active_profile_now =
                profile_after_gate_failure(session, &e, Some(original.get()));
            return report;
        }
    };

    let mut run = Run {
        prepared,
        progress: &mut progress,
        written: HashMap::new(),
        cancel: false,
    };
    let result = run.all_profiles(&mut writer, &order, &mut report);
    match result {
        Ok(()) => {}
        Err(Stop::Cancelled) => report.outcome = Outcome::Cancelled,
        Err(Stop::Failed(error)) => report.fail(error),
    }

    // Prove that the keyboard saved what was written: a read-back right
    // after a write reads its working copy (spec 4.5.3).
    if report.outcome == Outcome::Completed && !writer.is_poisoned() {
        if let Err(error) = run.persistence_check(&mut writer, original, &mut report) {
            report.fail(error);
        }
    }

    // Back to the original profile, unless nothing may be sent any more
    // (after the commit settle: select_profile waits for it).
    report.active_profile_now = writer.known_profile().map(ProfileId::get);
    if !writer.is_poisoned() {
        let _ = (run.progress)(WriteProgress::Returning {
            profile: original.get(),
        });
        let progress = &mut *run.progress;
        let returned = writer.select_profile_observed(original, &mut |wait| {
            let _ = progress(WriteProgress::waiting(wait));
        });
        match returned {
            Ok(selected) => report.active_profile_now = Some(selected.profile.get()),
            Err(e) => {
                report.active_profile_now = writer.known_profile().map(ProfileId::get);
                if report.error.is_none() {
                    report.fail(ErrorInfo::new(e.kind(), &e));
                }
            }
        }
    }

    if report.outcome == Outcome::Completed && needs_final_check(&prepared.plan, &report) {
        if let Err(error) = run.final_check(&mut writer, original, &mut report) {
            report.fail(error);
        }
        report.active_profile_now = writer.known_profile().map(ProfileId::get);
    }

    // Finished only when the keyboard had time to save the last write
    // (usually the switches above already waited).
    if !writer.is_poisoned() {
        let progress = &mut *run.progress;
        let waited = writer.wait_for_commit_observed(&mut |wait| {
            let _ = progress(WriteProgress::waiting(wait));
        });
        if let Err(e) = waited {
            report.fail(ErrorInfo::new(e.kind(), &e));
        }
    }
    // The last profile's writes were verified in the keyboard's working copy
    // only: after a profile change or a lost connection before the save they
    // may be gone.
    if writer.commit_interrupted() {
        if let Some(profile) = writer.session().last_write_profile() {
            report.mark_maybe_lost(profile);
        }
    }
    report.foreign_activity = writer.foreign_activity_seen();
    report
}

impl RestoreReport {
    /// Marks the report failed with `error` (the first error wins).
    fn fail(&mut self, error: ErrorInfo) {
        self.outcome = Outcome::Failed;
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    /// Marks the written, unconfirmed items of `profile` as
    /// [`ItemResult::MaybeLost`] (see [`Writer::commit_interrupted`]). Only
    /// the profile the last write went to can hold unsaved writes: every
    /// switch waits for the commit settle first.
    fn mark_maybe_lost(&mut self, profile: u8) {
        for item in self
            .profiles
            .iter_mut()
            .filter(|p| p.profile == profile)
            .flat_map(|p| p.items.iter_mut())
        {
            let result = std::mem::replace(&mut item.result, ItemResult::NotAttempted);
            item.result = result.into_maybe_lost();
        }
    }

    fn item_mut(&mut self, profile: ProfileId, index: usize) -> Option<&mut ItemReport> {
        self.profiles
            .iter_mut()
            .find(|p| p.profile == profile.get())
            .and_then(|p| p.items.get_mut(index))
    }
}

/// Why execution stopped.
pub(crate) enum Stop {
    Cancelled,
    Failed(ErrorInfo),
}

impl From<WriteError> for Stop {
    fn from(error: WriteError) -> Self {
        Stop::Failed(ErrorInfo::new(error.kind(), &error))
    }
}

/// The state of one execution.
struct Run<'a, P: FnMut(WriteProgress) -> ControlFlow<()>> {
    prepared: &'a PreparedRestore,
    progress: &'a mut P,
    /// What this restore wrote, per table (and key): a global table written
    /// in an earlier profile reads back like this.
    written: HashMap<(TableId, Option<KeyPos>), Vec<TableState>>,
    cancel: bool,
}

impl<P: FnMut(WriteProgress) -> ControlFlow<()>> Run<'_, P> {
    fn all_profiles<T: WriteTransport>(
        &mut self,
        writer: &mut Writer<'_, T>,
        order: &[ProfileId],
        report: &mut RestoreReport,
    ) -> Result<(), Stop> {
        for &profile in order {
            self.one_profile(writer, profile, report)?;
        }
        Ok(())
    }

    fn one_profile<T: WriteTransport>(
        &mut self,
        writer: &mut Writer<'_, T>,
        profile: ProfileId,
        report: &mut RestoreReport,
    ) -> Result<(), Stop> {
        let writes: Vec<(TableId, Option<KeyPos>, &PlannedWrite)> = self
            .prepared
            .plan
            .writes()
            .filter(|(p, _, _)| *p == profile)
            .map(|(_, item, write)| (item.table, item.key, write))
            .collect();
        if writes.is_empty() {
            return Ok(());
        }
        for (index, (table, key, write)) in writes.into_iter().enumerate() {
            if index == 0 {
                // The keyboard must save the previous profile's writes before
                // this profile is selected. Wait for that first, so that a
                // cancellation asked for during the wait (seen at the table
                // boundary right below) stops before the switch.
                let progress = &mut *self.progress;
                let cancel = &mut self.cancel;
                writer.wait_for_commit_observed(&mut |wait| {
                    if progress(WriteProgress::waiting(wait)).is_break() {
                        *cancel = true;
                    }
                })?;
            }
            if self.cancel || self.should_stop(profile, table, key, write) {
                return Err(Stop::Cancelled);
            }
            if index == 0 {
                // Nothing is left to wait for (unless the session wrote in
                // between, which it does not).
                let progress = &mut *self.progress;
                writer.select_profile_observed(profile, &mut |wait| {
                    let _ = progress(WriteProgress::waiting(wait));
                })?;
            }
            let result = self.one_table(writer, profile, table, key, write);
            let (item_result, stop) = match result {
                Ok(item_result) => (item_result, None),
                Err(TableFailure { error, mismatch }) => (
                    ItemResult::Failed {
                        error: error.clone(),
                        mismatch,
                    },
                    Some(Stop::Failed(error)),
                ),
            };
            if let Some(item) = report.item_mut(profile, index) {
                item.result = item_result;
            }
            if let Some(stop) = stop {
                return Err(stop);
            }
        }
        confirm_profile(writer, profile)?;
        Ok(())
    }

    /// Reports the start of a table; true when the caller asked to stop.
    fn should_stop(
        &mut self,
        profile: ProfileId,
        table: TableId,
        key: Option<KeyPos>,
        write: &PlannedWrite,
    ) -> bool {
        let start = WriteProgress::Writing {
            profile: profile.get(),
            table,
            key,
            packet: 0,
            of: write.packets,
            send: 1,
        };
        if (self.progress)(start).is_break() {
            self.cancel = true;
        }
        self.cancel
    }

    fn one_table<T: WriteTransport>(
        &mut self,
        writer: &mut Writer<'_, T>,
        profile: ProfileId,
        table: TableId,
        key: Option<KeyPos>,
        write: &PlannedWrite,
    ) -> Result<ItemResult, TableFailure> {
        let request = &write.request;
        let reads = request.verify_reads();
        let planned = self
            .prepared
            .safety
            .snapshot(profile)
            .map(|s| {
                let replies: Vec<RawResponse> = reads
                    .iter()
                    .filter_map(|r| s.response(*r).cloned())
                    .collect();
                state_of(request, &replies)
            })
            .unwrap_or_default();
        let state_key = (table, key);
        // Only a table whose profile scope is unknown may hold what this
        // restore wrote in an earlier profile (it may be global). Tables that
        // are per profile must still hold exactly the planned state.
        let written_here = if possibly_global(table) {
            self.written.get(&state_key).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };

        let progress = &mut *self.progress;
        let cancel = &mut self.cancel;
        let mut on_packet = |p: PacketProgress| {
            let step = WriteProgress::Writing {
                profile: profile.get(),
                table,
                key,
                packet: p.packet,
                of: p.of,
                send: p.send,
            };
            if progress(step).is_break() {
                *cancel = true;
            }
        };
        let result = write_checked(
            writer,
            profile,
            request,
            &planned,
            &written_here,
            &mut on_packet,
        )?;
        if matches!(result, ItemResult::Written { .. }) {
            self.written
                .entry(state_key)
                .or_default()
                .push(expected_state(request));
        }
        Ok(result)
    }

    /// The persistence check (spec 4.5.3, "Delayed commit"): a read-back
    /// right after a write reads the keyboard's working copy, which it saves
    /// about 1 s later, and a profile switch before that drops it. Every
    /// profile with written tables is therefore selected again after
    /// switching away from it, so that the keyboard reloads what it saved,
    /// and its written tables are read again: the other profiles first, the
    /// original one last (the restore ends there). When the profile to check
    /// is the active one, the check switches to another profile first (the
    /// original one, or for the original itself the lowest other profile).
    /// Every switch waits for the commit settle first.
    ///
    /// A table that no longer holds what was written becomes
    /// [`ItemResult::Lost`] and fails the restore; a table that may be shared
    /// by all profiles and holds what this restore wrote in another profile
    /// stays [`Persistence::NotChecked`] (the final check compares it).
    /// Lost writes are reported; an `Err` is a failed read or switch. Not
    /// cancellable: it only reads and switches profiles.
    fn persistence_check<T: WriteTransport>(
        &mut self,
        writer: &mut Writer<'_, T>,
        original: ProfileId,
        report: &mut RestoreReport,
    ) -> Result<(), ErrorInfo> {
        let written: Vec<ProfileId> = report
            .profiles
            .iter()
            .filter(|p| {
                p.items
                    .iter()
                    .any(|i| matches!(i.result, ItemResult::Written { .. }))
            })
            .filter_map(|p| ProfileId::new(p.profile))
            .collect();
        if written.is_empty() {
            return Ok(());
        }
        let order = persistence_order(&written, original, writer.known_profile());
        let mut check = PersistenceCheck {
            profiles: Vec::new(),
            persisted: 0,
            lost: 0,
            not_checked: 0,
        };
        let mut lost_in: Vec<String> = Vec::new();
        let visited =
            self.visit_for_persistence(writer, original, &order, report, &mut check, &mut lost_in);
        // Kept even when a switch or a read failed: what was checked so far
        // (nothing to report when no profile was read again).
        report.persistence = (!check.profiles.is_empty()).then_some(check);
        visited?;
        if !lost_in.is_empty() {
            return Err(ErrorInfo {
                kind: ErrorKind::NotPersisted,
                message: format!(
                    "{} written and verified, but read again after a profile switch they no \
                     longer held what was written: the keyboard did not keep them (it saves \
                     about 1 s after the last write). Run the same restore again to write them \
                     once more, or undo with the command below",
                    lost_in.join(", ")
                ),
            });
        }
        Ok(())
    }

    /// The visits of the persistence check, in `order`; records the results
    /// in `report` and `check`, and the profiles with lost tables in
    /// `lost_in`.
    fn visit_for_persistence<T: WriteTransport>(
        &mut self,
        writer: &mut Writer<'_, T>,
        original: ProfileId,
        order: &[ProfileId],
        report: &mut RestoreReport,
        check: &mut PersistenceCheck,
        lost_in: &mut Vec<String>,
    ) -> Result<(), ErrorInfo> {
        let failure = |e: WriteError| ErrorInfo::new(e.kind(), &e);
        for &profile in order {
            let _ = (self.progress)(WriteProgress::CheckingPersistence {
                profile: profile.get(),
            });
            let progress = &mut *self.progress;
            let mut on_wait = |wait| {
                let _ = progress(WriteProgress::waiting(wait));
            };
            let mut selected = writer
                .select_profile_observed(profile, &mut on_wait)
                .map_err(failure)?;
            if !selected.switched {
                // It is the active profile: leave it, so that selecting it
                // again reloads what the keyboard saved.
                let away = if profile == original {
                    ProfileId::ALL
                        .into_iter()
                        .find(|p| *p != profile)
                        .unwrap_or(original)
                } else {
                    original
                };
                writer
                    .select_profile_observed(away, &mut on_wait)
                    .map_err(failure)?;
                selected = writer
                    .select_profile_observed(profile, &mut on_wait)
                    .map_err(failure)?;
            }
            if !selected.switched {
                // Only a switch reloads the profile; never report a read of
                // the working copy as persisted.
                return Err(failure(WriteError::ProfileChanged {
                    expected: Some(profile.get()),
                    reported: selected.profile.get(),
                }));
            }
            check.profiles.push(profile.get());
            let lost_before = check.lost;
            self.check_profile_persisted(writer, profile, report, check)
                .map_err(failure)?;
            if check.lost > lost_before {
                lost_in.push(format!(
                    "profile {profile}: {} table(s)",
                    check.lost - lost_before
                ));
            }
        }
        Ok(())
    }

    /// Reads back the written tables of `profile` (just selected again after
    /// a switch) and marks each persisted or lost.
    fn check_profile_persisted<T: WriteTransport>(
        &self,
        writer: &mut Writer<'_, T>,
        profile: ProfileId,
        report: &mut RestoreReport,
        check: &mut PersistenceCheck,
    ) -> Result<(), WriteError> {
        let writes: Vec<&PlannedWrite> = self
            .prepared
            .plan
            .writes()
            .filter(|(p, _, _)| *p == profile)
            .map(|(_, _, w)| w)
            .collect();
        for (index, write) in writes.into_iter().enumerate() {
            let Some(item) = report.item_mut(profile, index) else {
                continue;
            };
            let ItemResult::Written {
                sends,
                packets,
                warnings,
                ..
            } = &item.result
            else {
                continue;
            };
            let (sends, packets, warnings) = (*sends, *packets, warnings.clone());
            let request = &write.request;
            let mut replies = Vec::new();
            for read in request.verify_reads() {
                replies.push(writer.read(read)?);
            }
            // A table that may be shared by all profiles can hold what this
            // restore wrote to it in another profile; the final check
            // compares it with the backup.
            let written_elsewhere = possibly_global(item.table)
                && self
                    .written
                    .get(&(item.table, item.key))
                    .is_some_and(|states| states.contains(&state_of(request, &replies)));
            item.result = match request.check(&replies) {
                Ok(()) => {
                    check.persisted += 1;
                    ItemResult::Written {
                        sends,
                        packets,
                        warnings,
                        persistence: Persistence::Persisted,
                    }
                }
                Err(_) if written_elsewhere => {
                    check.not_checked += 1;
                    ItemResult::Written {
                        sends,
                        packets,
                        warnings,
                        persistence: Persistence::NotChecked,
                    }
                }
                Err(mismatch) => {
                    check.lost += 1;
                    ItemResult::Lost {
                        sends,
                        packets,
                        warnings,
                        mismatch,
                    }
                }
            };
        }
        Ok(())
    }

    /// Reads every profile in scope again and compares with the backup every
    /// written table, and every table whose profile scope is unknown that the
    /// plan left unchanged (a shared table written in another profile changes
    /// it too). Not cancellable: it only reads and switches profiles, and it
    /// always returns to the original profile.
    fn final_check<T: WriteTransport>(
        &mut self,
        writer: &mut Writer<'_, T>,
        original: ProfileId,
        report: &mut RestoreReport,
    ) -> Result<(), ErrorInfo> {
        let _ = (self.progress)(WriteProgress::Verifying);
        let scope: Vec<ProfileId> = self
            .prepared
            .plan
            .profiles
            .iter()
            .map(|p| p.profile)
            .collect();
        let product = self.prepared.safety.device.product.clone();
        let mut snapshots: Vec<(ProfileId, Vec<RawResponse>)> = Vec::new();
        if scope.contains(&original) {
            let snapshot = writer
                .read_snapshot(original, &mut |_| ControlFlow::Continue(()))
                .map_err(|e| ErrorInfo::new(e.kind(), &e))?;
            snapshots.push((original, raws(&snapshot.entries)));
        }
        let others: Vec<ProfileId> = scope.iter().copied().filter(|p| *p != original).collect();
        let progress = &mut *self.progress;
        // Shown, but a cancellation request is ignored: every table was
        // already written, and stopping here would only skip the check.
        let read = read_other_profiles(writer, original, &others, product.as_deref(), &mut |p| {
            let _ = progress(WriteProgress::SafetyBackup { progress: p });
            ControlFlow::Continue(())
        })
        .map_err(|e| ErrorInfo::new(e.kind(), &e))?;
        for (profile, snapshot) in others.iter().zip(read) {
            snapshots.push((*profile, raws(&snapshot.entries)));
        }

        let mut mismatches = Vec::new();
        for (profile, replies) in &snapshots {
            let done = report
                .profiles
                .iter()
                .find(|p| p.profile == profile.get())
                .map(|p| p.items.as_slice())
                .unwrap_or_default();
            let writes = self
                .prepared
                .plan
                .writes()
                .filter(|(p, _, _)| p == profile)
                .map(|(_, _, w)| w);
            for (write, item) in writes.zip(done) {
                if !matches!(
                    item.result,
                    ItemResult::Written { .. } | ItemResult::AlreadyMatched
                ) {
                    continue;
                }
                if let Err(mismatch) = write.request.check(replies) {
                    mismatches.push(FinalMismatch {
                        profile: profile.get(),
                        mismatch,
                    });
                }
            }
            // Tables of unknown scope that this profile's plan left
            // unchanged must still hold the backup's bytes.
            let (Some(plan), Some(target)) = (
                self.prepared
                    .plan
                    .profiles
                    .iter()
                    .find(|p| p.profile == *profile),
                self.prepared.target.snapshot(*profile),
            ) else {
                continue;
            };
            for item in plan
                .items
                .iter()
                .filter(|i| matches!(i.action, PlanAction::Unchanged) && possibly_global(i.table))
            {
                if let Some(mismatch) = unchanged_mismatch(item.table, target, replies) {
                    mismatches.push(FinalMismatch {
                        profile: profile.get(),
                        mismatch,
                    });
                }
            }
        }
        let failed = !mismatches.is_empty();
        report.final_check = Some(FinalCheck {
            profiles: snapshots.iter().map(|(p, _)| p.get()).collect(),
            mismatches,
        });
        if failed {
            return Err(ErrorInfo {
                kind: ErrorKind::VerifyFailed,
                message: "after writing every profile, some tables no longer hold the backup's \
                          bytes (a table shared by all profiles, or another program); see the \
                          final check"
                    .to_string(),
            });
        }
        Ok(())
    }
}

/// A failed table: the error and, if known, the read-back difference.
pub(crate) struct TableFailure {
    pub(crate) error: ErrorInfo,
    pub(crate) mismatch: Option<Mismatch>,
}

impl From<WriteError> for TableFailure {
    fn from(error: WriteError) -> Self {
        let mismatch = match &error {
            WriteError::VerifyFailed { mismatch, .. } => Some((**mismatch).clone()),
            _ => None,
        };
        TableFailure {
            error: ErrorInfo::new(error.kind(), &error),
            mismatch,
        }
    }
}

impl From<Stop> for TableFailure {
    fn from(stop: Stop) -> Self {
        match stop {
            Stop::Failed(error) => TableFailure {
                error,
                mismatch: None,
            },
            Stop::Cancelled => TableFailure {
                error: ErrorInfo {
                    kind: ErrorKind::Cancelled,
                    message: "cancelled".to_string(),
                },
                mismatch: None,
            },
        }
    }
}

/// Confirms with `21/11` that `profile` is active.
pub(crate) fn confirm_profile<T: WriteTransport>(
    writer: &mut Writer<'_, T>,
    profile: ProfileId,
) -> Result<(), Stop> {
    let active = writer.active_profile()?;
    if active != profile {
        return Err(writer
            .profile_changed(Some(profile.get()), active.get())
            .into());
    }
    Ok(())
}

/// One guarded table write, shared by restores and edits:
///
/// 1. refuses when another program was seen while writing;
/// 2. confirms with `21/11` that `profile` is active;
/// 3. re-reads exactly the bytes `request` changes: when the keyboard
///    already holds them the result is [`ItemResult::AlreadyMatched`]; when
///    they are neither `planned` (the state the write was planned against)
///    nor one of `also_expected` (states this operation wrote), the result
///    is a `changed_since_plan` failure and nothing is written;
/// 4. writes with [`Writer::write_observed`] (stop-and-wait, verification,
///    one more attempt with the whole table).
pub(crate) fn write_checked<T: WriteTransport>(
    writer: &mut Writer<'_, T>,
    profile: ProfileId,
    request: &WriteRequest,
    planned: &[Option<Vec<u8>>],
    also_expected: &[TableState],
    on_packet: &mut dyn FnMut(PacketProgress),
) -> Result<ItemResult, TableFailure> {
    if let Some(evidence) = writer.foreign_activity_seen() {
        return Err(WriteError::OtherProgramActive { evidence }.into());
    }
    confirm_profile(writer, profile)?;

    let reads = request.verify_reads();
    let mut fresh = Vec::with_capacity(reads.len());
    for read in &reads {
        fresh.push(writer.read(*read)?);
    }
    if request.check(&fresh).is_ok() {
        return Ok(ItemResult::AlreadyMatched);
    }
    let now = state_of(request, &fresh);
    if now.as_slice() != planned && !also_expected.contains(&now) {
        return Err(TableFailure {
            error: ErrorInfo {
                kind: ErrorKind::ChangedSincePlan,
                message: format!(
                    "profile {profile}, {}: the keyboard's data changed after the change was \
                     planned (another program or a change on the keyboard); it was not written",
                    request.table()
                ),
            },
            mismatch: None,
        });
    }
    let outcome = writer.write_observed(request, on_packet)?;
    Ok(ItemResult::Written {
        sends: outcome.sends,
        packets: outcome.packets,
        warnings: outcome.warnings,
        persistence: Persistence::NotChecked,
    })
}

/// The profiles with writes: the original one first, then ascending.
fn execution_order(plan: &RestorePlan, original: ProfileId) -> Vec<ProfileId> {
    let with_writes: BTreeSet<ProfileId> = plan.writes().map(|(p, _, _)| p).collect();
    let mut order = Vec::with_capacity(with_writes.len());
    if with_writes.contains(&original) {
        order.push(original);
    }
    order.extend(with_writes.into_iter().filter(|p| *p != original));
    order
}

/// The order of the persistence check: the written profiles other than the
/// original one, ascending, with the active one (`current`) last among them
/// (checking it needs a switch away first), then the original one if it was
/// written (the restore ends on it).
fn persistence_order(
    written: &[ProfileId],
    original: ProfileId,
    current: Option<ProfileId>,
) -> Vec<ProfileId> {
    let others: BTreeSet<ProfileId> = written.iter().copied().filter(|p| *p != original).collect();
    let mut order: Vec<ProfileId> = others
        .iter()
        .copied()
        .filter(|p| Some(*p) != current)
        .collect();
    order.extend(others.iter().copied().filter(|p| Some(*p) == current));
    if written.contains(&original) {
        order.push(original);
    }
    order
}

/// Whether a final check is needed: a table whose profile scope is unknown
/// was written, and the plan compares two or more profiles (whose copies of
/// that table may be one shared table).
fn needs_final_check(plan: &RestorePlan, report: &RestoreReport) -> bool {
    let compared = plan.profiles.iter().filter(|p| p.compared).count();
    let unknown_scope = report
        .items()
        .any(|(_, i)| matches!(i.result, ItemResult::Written { .. }) && possibly_global(i.table));
    compared >= 2 && unknown_scope
}

/// The difference between the backup's copy of an unchanged table of
/// unknown scope and the keyboard's `replies`, if any.
fn unchanged_mismatch(
    table: TableId,
    target: SnapshotView<'_>,
    replies: &[RawResponse],
) -> Option<Mismatch> {
    let (read, range) = match table {
        TableId::CustomColors { table } => (ReadRequest::CustomColors(table), None),
        TableId::DeadBand => (ReadRequest::DeadBand, None),
        TableId::Keymap { layer } => (ReadRequest::Keymap(layer), None),
        TableId::AdvancedKey { slot } => {
            let start = usize::from(slot.get()) * ADVANCED_SLOT_LEN;
            (
                ReadRequest::AdvancedKeys,
                Some(start..start + ADVANCED_SLOT_LEN),
            )
        }
        _ => return None,
    };
    let part = |bytes: Vec<u8>| match &range {
        Some(range) => bytes
            .get(range.clone())
            .map(<[u8]>::to_vec)
            .unwrap_or_default(),
        None => bytes,
    };
    let expected = part(content(target.response(read)?));
    let mismatch = |kind, got: Vec<u8>| Mismatch {
        table,
        read,
        kind,
        expected: HexBytes(expected.clone()),
        got: HexBytes(got),
    };
    let Some(reply) = replies.iter().rev().find(|r| r.request() == read) else {
        return Some(mismatch(MismatchKind::MissingReply, Vec::new()));
    };
    let got = part(content(reply));
    (got != expected).then(|| mismatch(MismatchKind::Differs, got))
}

/// Whether a writable table's profile scope is unknown (it may be shared by
/// all profiles): custom colours 1, the dead band, the Fn keymap and the
/// advanced keys. Main light, triggers, base keymap and macros are per
/// profile (OBSERVED, c04 / c10).
fn possibly_global(table: TableId) -> bool {
    matches!(
        table,
        TableId::CustomColors { .. }
            | TableId::DeadBand
            | TableId::Keymap { layer: Layer::Fn }
            | TableId::AdvancedKey { .. }
    )
}

fn raws(entries: &[crate::BackupEntry]) -> Vec<RawResponse> {
    entries.iter().map(|e| e.raw.clone()).collect()
}

/// The bytes `request` changes, as read in `replies`, one entry per verify
/// read (`None` when the reply is missing or unusable).
pub(crate) fn state_of(request: &WriteRequest, replies: &[RawResponse]) -> TableState {
    request
        .verify_reads()
        .into_iter()
        .map(|read| {
            replies
                .iter()
                .rev()
                .find(|r| r.request() == read)
                .and_then(|reply| compared_bytes(request, reply))
        })
        .collect()
}

/// The bytes of `reply` that `request` writes.
fn compared_bytes(request: &WriteRequest, reply: &RawResponse) -> Option<Vec<u8>> {
    match request {
        WriteRequest::SelectProfile(_) => reply.payload().get(1..2).map(<[u8]>::to_vec),
        WriteRequest::MainLight(_) => Some(reply.payload()),
        WriteRequest::CustomColors1(_)
        | WriteRequest::Keymap(..)
        | WriteRequest::Macro(..)
        | WriteRequest::DeadBand(_) => reply.table_bytes("restore").ok(),
        WriteRequest::AdvancedKeyClear(slot) => {
            let table = reply.table_bytes("advanced keys").ok()?;
            let start = usize::from(slot.get()) * ADVANCED_SLOT_LEN;
            table
                .get(start..start + ADVANCED_SLOT_LEN)
                .map(<[u8]>::to_vec)
        }
        WriteRequest::Trigger(_) => reply.payload().get(1..10).map(<[u8]>::to_vec),
    }
}

/// What the keyboard reads back after `request` was written, in the form of
/// [`state_of`].
pub(crate) fn expected_state(request: &WriteRequest) -> TableState {
    let bytes = match request {
        WriteRequest::SelectProfile(p) => vec![p.get()],
        WriteRequest::MainLight(w) => w.record().as_bytes().to_vec(),
        WriteRequest::CustomColors1(d) => d.as_bytes().to_vec(),
        WriteRequest::Keymap(_, d) => d.as_bytes().to_vec(),
        WriteRequest::Macro(_, d) => d.to_bytes().to_vec(),
        WriteRequest::AdvancedKeyClear(_) => vec![0u8; ADVANCED_SLOT_LEN],
        WriteRequest::Trigger(w) => w.setting().read_bytes().to_vec(),
        WriteRequest::DeadBand(d) => d.as_bytes().to_vec(),
    };
    request
        .verify_reads()
        .into_iter()
        .map(|_| Some(bytes.clone()))
        .collect()
}

/// Checks the target backup: a known model, an active-profile reply, and
/// every requested profile present. Returns the model and the scope.
fn check_target(
    target: &Backup,
    options: &RestoreOptions,
) -> Result<(&'static Model, ProfileSet), RestoreError> {
    let Some(model) = target.model() else {
        return Err(RestoreError::InvalidBackup {
            reason: "the backup does not identify a supported keyboard (no device-info reply)"
                .to_string(),
        });
    };
    if target.active_profile.and_then(ProfileId::new).is_none() {
        return Err(RestoreError::InvalidBackup {
            reason: "the backup has no active-profile reply (21/11), so it is not known which \
                     profile it belongs to"
                .to_string(),
        });
    }
    let available: ProfileSet = target
        .profile_numbers()
        .into_iter()
        .filter_map(ProfileId::new)
        .collect();
    let scope = options.profiles.unwrap_or(available);
    if let Some(missing) = scope.iter().find(|p| !available.contains(*p)) {
        return Err(RestoreError::ProfileNotInBackup {
            profile: missing.get(),
        });
    }
    if scope.is_empty() {
        return Err(RestoreError::InvalidBackup {
            reason: "no profile to restore".to_string(),
        });
    }
    Ok((model, scope))
}

/// Model (hard error) and firmware (error unless allowed, then a warning).
fn check_keyboard(
    target: &Backup,
    product: Option<&str>,
    firmware: Option<&str>,
    allow_firmware_mismatch: bool,
) -> Result<Vec<PlanWarning>, RestoreError> {
    if target.device.product.as_deref() != product || product.is_none() {
        return Err(RestoreError::ModelMismatch {
            backup: target.device.product.clone(),
            keyboard: product.map(str::to_string),
        });
    }
    let mut warnings = Vec::new();
    if target.device.firmware.as_deref() != firmware {
        if !allow_firmware_mismatch {
            return Err(RestoreError::FirmwareMismatch {
                backup: target.device.firmware.clone(),
                keyboard: firmware.map(str::to_string),
            });
        }
        warnings.push(PlanWarning::FirmwareDiffers {
            backup: target.device.firmware.clone(),
            keyboard: firmware.map(str::to_string),
        });
    }
    Ok(warnings)
}

/// Warnings about tables whose profile scope is unknown (custom colours 1,
/// dead band, Fn keymap, advanced keys): written while the restore does not
/// cover every profile, or backed up with different copies while the
/// keyboard holds one copy in every profile.
fn scope_warnings(target: &Backup, current: &Backup, profiles: &[ProfilePlan]) -> Vec<PlanWarning> {
    let mut warnings = Vec::new();
    let mut written: Vec<TableId> = Vec::new();
    for item in profiles.iter().flat_map(|p| p.items.iter()) {
        if matches!(item.action, PlanAction::Write(_))
            && possibly_global(item.table)
            && !written.contains(&item.table)
        {
            written.push(item.table);
        }
    }
    let covered: Vec<u8> = profiles
        .iter()
        .filter(|p| p.compared)
        .map(|p| p.profile.get())
        .collect();
    let uncovered: Vec<u8> = ProfileId::ALL
        .into_iter()
        .map(ProfileId::get)
        .filter(|p| !covered.contains(p))
        .collect();
    if !written.is_empty() && !uncovered.is_empty() {
        warnings.push(PlanWarning::SharedTablesMayChangeOtherProfiles {
            tables: written,
            profiles: uncovered,
        });
    }
    let shared = [
        (
            TableId::CustomColors {
                table: CustomTable::Custom1,
            },
            ReadRequest::CustomColors(CustomTable::Custom1),
        ),
        (TableId::DeadBand, ReadRequest::DeadBand),
        (
            TableId::Keymap { layer: Layer::Fn },
            ReadRequest::Keymap(Layer::Fn),
        ),
    ];
    let copies = |backup: &Backup, read: ReadRequest| -> Vec<Vec<u8>> {
        backup
            .snapshots()
            .into_iter()
            .filter_map(|view| view.response(read).map(content))
            .collect()
    };
    let all_same = |copies: &[Vec<u8>]| copies.windows(2).all(|w| w[0] == w[1]);
    for (table, read) in shared {
        let (backup, keyboard) = (copies(target, read), copies(current, read));
        if backup.len() >= 2 && !all_same(&backup) && keyboard.len() >= 2 && all_same(&keyboard) {
            warnings.push(PlanWarning::LikelySharedTableDiffers { table });
        }
    }
    warnings
}

/// Device constants that differ (they are never written).
fn constant_warnings(target: SnapshotView<'_>, current: SnapshotView<'_>) -> Vec<PlanWarning> {
    let mut warnings = Vec::new();
    for request in [
        ReadRequest::MainLightModes,
        ReadRequest::SideLightModes,
        ReadRequest::FnLockedKeys,
        ReadRequest::TriggerGlobal,
        ReadRequest::SwitchTypeList,
    ] {
        if let (Some(t), Some(c)) = (target.response(request), current.response(request)) {
            if t.payload() != c.payload() {
                warnings.push(PlanWarning::ConstantDiffers { request });
            }
        }
    }
    let flags = |s: SnapshotView<'_>| {
        s.response(ReadRequest::Status)
            .map(|r| r.payload().get(12..14).map(<[u8]>::to_vec))
    };
    if let (Some(t), Some(c)) = (flags(target), flags(current)) {
        if t != c {
            warnings.push(PlanWarning::CapabilityFlagsDiffer);
        }
    }
    warnings
}

/// The declared contents of a reply: the table bytes of a multi-chunk reply,
/// else the payload.
fn content(raw: &RawResponse) -> Vec<u8> {
    raw.table_bytes("content").unwrap_or_else(|_| raw.payload())
}

fn item(table: TableId, key: Option<KeyPos>, action: PlanAction) -> PlanItem {
    PlanItem { table, key, action }
}

fn skip(table: TableId, key: Option<KeyPos>, reason: SkipReason, message: String) -> PlanItem {
    item(table, key, PlanAction::Skip { reason, message })
}

/// A planned write of `request`.
fn write_action(request: WriteRequest, changes: Vec<EntryChange>) -> PlanAction {
    PlanAction::Write(PlannedWrite {
        kind: request.kind(),
        summary: request.to_string(),
        packets: request.steps().len(),
        changes,
        request,
    })
}

/// The entries of size `entry` that differ between `old` and `new`.
fn entry_changes(old: &[u8], new: &[u8], entry: usize) -> Vec<EntryChange> {
    old.chunks(entry)
        .zip(new.chunks(entry))
        .enumerate()
        .filter(|(_, (o, n))| o != n)
        .map(|(index, (o, n))| EntryChange {
            index,
            old: HexBytes::from(o),
            new: HexBytes::from(n),
        })
        .collect()
}

/// A whole-slot change, with empty slots (all `00` or all `FF`) shown empty.
fn slot_change(index: usize, old: &[u8], new: &[u8]) -> Vec<EntryChange> {
    let shown = |bytes: &[u8]| {
        if bytes.iter().all(|b| *b == 0) || bytes.iter().all(|b| *b == 0xFF) {
            HexBytes::default()
        } else {
            HexBytes::from(bytes)
        }
    };
    vec![EntryChange {
        index,
        old: shown(old),
        new: shown(new),
    }]
}

/// The distance unit of a snapshot in micrometres (`21/04` unit x the
/// status high-precision flag), if both replies are there.
fn distance_unit(view: SnapshotView<'_>) -> Option<u32> {
    let global = view.trigger_global()?;
    let status = view.status()?;
    Some(global.unit_micrometres(status.flags_b.high_precision))
}

/// `None` when trigger and dead-band values of the backup mean the same
/// distances on the keyboard (both units known and 0.02 mm); otherwise the
/// skip reason.
fn unit_problem(t: SnapshotView<'_>, c: SnapshotView<'_>) -> Option<(SkipReason, String)> {
    let (backup_um, keyboard_um) = (distance_unit(t), distance_unit(c));
    let ok = Some(Distance::MICROMETRES_PER_UNIT);
    if backup_um == ok && keyboard_um == ok {
        return None;
    }
    let show =
        |unit: Option<u32>| unit.map_or_else(|| "unknown".to_string(), |u| format!("{u} um"));
    Some((
        SkipReason::DistanceUnitChanged {
            backup_um,
            keyboard_um,
        },
        format!(
            "the backup stores distances in units of {}, the keyboard uses {}; OpenWraith only \
             restores 0.02 mm (20 um) units, so the stored numbers would mean other distances",
            show(backup_um),
            show(keyboard_um)
        ),
    ))
}

/// The plan of one profile, in execution order.
fn plan_profile(model: &Model, t: SnapshotView<'_>, c: SnapshotView<'_>) -> Vec<PlanItem> {
    let mut items = Vec::new();
    items.extend(plan_triggers(model, t, c));
    items.push(plan_dead_band(model, t, c));
    items.extend(plan_keymap_group(t, c));
    items.push(plan_colors1(t, c));
    items.push(plan_main_light(model, t, c));
    items.extend(plan_unwritable(t, c));
    items
}

/// Per key: unchanged keys are not listed; one item per differing key.
fn plan_triggers(model: &Model, t: SnapshotView<'_>, c: SnapshotView<'_>) -> Vec<PlanItem> {
    let limits = c
        .trigger_global()
        .map(|g| TriggerLimits::new(model, g))
        .filter(TriggerLimits::is_supported);
    let units = unit_problem(t, c);
    let mut items = Vec::new();
    for pos in model.positions() {
        let key = Some(pos);
        let request = ReadRequest::KeyTrigger(pos);
        let (target, current) = match (t.response(request), c.response(request)) {
            (Some(target), Some(current)) => (target, current),
            (None, _) => {
                items.push(item(TableId::Triggers, key, PlanAction::NotInBackup));
                continue;
            }
            (Some(_), None) => {
                items.push(skip(
                    TableId::Triggers,
                    key,
                    SkipReason::NotInKeyboard,
                    format!("the keyboard did not report the trigger settings of {pos}"),
                ));
                continue;
            }
        };
        let old = current.payload().get(1..10).map(<[u8]>::to_vec);
        let new = target.payload().get(1..10).map(<[u8]>::to_vec);
        if old.is_some() && old == new {
            continue;
        }
        let name = model.key_name(pos.index());
        if Some(pos.index()) == model.knob_index {
            items.push(skip(
                TableId::Triggers,
                key,
                SkipReason::Knob,
                format!("{name} (the knob) has no writable trigger settings"),
            ));
            continue;
        }
        let setting = match TriggerSetting::from_response(target) {
            Ok((_, setting)) => setting,
            Err(e) => {
                items.push(skip(
                    TableId::Triggers,
                    key,
                    trigger_skip(&e),
                    format!("{name}: {e}"),
                ));
                continue;
            }
        };
        let Some(limits) = limits else {
            items.push(skip(
                TableId::Triggers,
                key,
                SkipReason::TriggerLimitsUnknown,
                "the keyboard's trigger parameters (21/04) are missing or unusable".to_string(),
            ));
            continue;
        };
        if let Some((reason, message)) = &units {
            items.push(skip(
                TableId::Triggers,
                key,
                reason.clone(),
                format!("{name}: {message}"),
            ));
            continue;
        }
        match TriggerWrite::new(model, KeyMask::from_positions([pos]), setting, &limits) {
            Ok(write) => {
                let changes = vec![EntryChange {
                    index: usize::from(pos.index()),
                    old: HexBytes(old.unwrap_or_default()),
                    new: HexBytes(new.unwrap_or_default()),
                }];
                items.push(item(
                    TableId::Triggers,
                    key,
                    write_action(WriteRequest::Trigger(write), changes),
                ));
            }
            Err(e) => items.push(skip(
                TableId::Triggers,
                key,
                trigger_skip(&e),
                format!("{name}: {e}"),
            )),
        }
    }
    if items.is_empty() {
        items.push(item(TableId::Triggers, None, PlanAction::Unchanged));
    }
    items
}

fn trigger_skip(error: &WriteBuildError) -> SkipReason {
    match error {
        WriteBuildError::UnknownTriggerMode { mode } => {
            SkipReason::UnknownTriggerMode { mode: *mode }
        }
        WriteBuildError::SecondTravelDiffers { .. } => SkipReason::SecondTravelDiffers,
        WriteBuildError::PressReleaseDiffer { .. } => SkipReason::PressReleaseDiffer,
        WriteBuildError::TriggerValueOutOfRange { .. }
        | WriteBuildError::KeyNotSelectable { .. } => SkipReason::OutOfRange,
        WriteBuildError::TriggerLimitsUnsupported => SkipReason::TriggerLimitsUnknown,
        WriteBuildError::DeadBandOutOfRange { .. } => SkipReason::DeadBandOutOfRange,
        _ => SkipReason::Unusable,
    }
}

fn plan_dead_band(model: &Model, t: SnapshotView<'_>, c: SnapshotView<'_>) -> PlanItem {
    let table = TableId::DeadBand;
    let (target, current) = match (
        t.response(ReadRequest::DeadBand),
        c.response(ReadRequest::DeadBand),
    ) {
        (Some(target), Some(current)) => (target, current),
        (None, _) => return item(table, None, PlanAction::NotInBackup),
        (Some(_), None) => {
            return skip(
                table,
                None,
                SkipReason::NotInKeyboard,
                "the keyboard did not report its dead band".to_string(),
            );
        }
    };
    let (old, new) = (content(current), content(target));
    if old == new {
        return item(table, None, PlanAction::Unchanged);
    }
    let data = match DeadBandData::from_response(target) {
        Ok(data) => data,
        Err(e) => return skip(table, None, SkipReason::Unusable, e.to_string()),
    };
    let high_precision = c.status().is_some_and(|s| s.flags_b.high_precision);
    let Some(unit) = c
        .trigger_global()
        .map(|g| g.unit_micrometres(high_precision))
    else {
        return skip(
            table,
            None,
            SkipReason::DeadBandUnitUnknown,
            "the keyboard's distance unit (21/04) is unknown".to_string(),
        );
    };
    if let Some((reason, message)) = unit_problem(t, c) {
        return skip(table, None, reason, message);
    }
    match DeadBandWrite::new(model, data, DeadBandData::max_units(unit)) {
        Ok(write) => item(
            table,
            None,
            write_action(WriteRequest::DeadBand(write), entry_changes(&old, &new, 2)),
        ),
        Err(e) => skip(table, None, trigger_skip(&e), e.to_string()),
    }
}

fn plan_colors1(t: SnapshotView<'_>, c: SnapshotView<'_>) -> PlanItem {
    let request = ReadRequest::CustomColors(CustomTable::Custom1);
    let table = TableId::CustomColors {
        table: CustomTable::Custom1,
    };
    let (target, current) = match (t.response(request), c.response(request)) {
        (Some(target), Some(current)) => (target, current),
        (None, _) => return item(table, None, PlanAction::NotInBackup),
        (Some(_), None) => {
            return skip(
                table,
                None,
                SkipReason::NotInKeyboard,
                "the keyboard did not report custom colour table 1".to_string(),
            );
        }
    };
    let (old, new) = (content(current), content(target));
    if old == new {
        return item(table, None, PlanAction::Unchanged);
    }
    match ColorTableData::from_response(target) {
        Ok(data) => item(
            table,
            None,
            write_action(
                WriteRequest::CustomColors1(data),
                entry_changes(&old, &new, 3),
            ),
        ),
        Err(e) => skip(table, None, SkipReason::Unusable, e.to_string()),
    }
}

fn plan_main_light(model: &Model, t: SnapshotView<'_>, c: SnapshotView<'_>) -> PlanItem {
    let table = TableId::MainLight;
    let (target, current) = match (
        t.response(ReadRequest::MainLight),
        c.response(ReadRequest::MainLight),
    ) {
        (Some(target), Some(current)) => (target, current),
        (None, _) => return item(table, None, PlanAction::NotInBackup),
        (Some(_), None) => {
            return skip(
                table,
                None,
                SkipReason::NotInKeyboard,
                "the keyboard did not report its main light".to_string(),
            );
        }
    };
    let (old, new) = (current.payload(), target.payload());
    if old == new {
        return item(table, None, PlanAction::Unchanged);
    }
    let target_record = match LightRecord::from_response(target) {
        Ok(record) => record,
        Err(e) => return skip(table, None, SkipReason::Unusable, e.to_string()),
    };
    let (Ok(current_record), Some(modes)) =
        (LightRecord::from_response(current), c.main_light_modes())
    else {
        return skip(
            table,
            None,
            SkipReason::NotInKeyboard,
            "the keyboard's light record or mode list (0A) is missing or unusable".to_string(),
        );
    };
    match MainLightWrite::new(
        &current_record,
        target_record,
        &LightLimits::new(model, modes),
    ) {
        Ok(write) => item(
            table,
            None,
            write_action(WriteRequest::MainLight(write), slot_change(0, &old, &new)),
        ),
        Err(WriteBuildError::UnsupportedLightMode { mode }) => skip(
            table,
            None,
            SkipReason::UnsupportedLightMode { mode },
            format!("light mode {mode:#04x} is not in the keyboard's mode list"),
        ),
        Err(e) => skip(table, None, SkipReason::InvalidLightRecord, e.to_string()),
    }
}

/// Tables without a confirmed write command: compared only.
fn plan_unwritable(t: SnapshotView<'_>, c: SnapshotView<'_>) -> Vec<PlanItem> {
    let unconfirmed = |command| SkipReason::UnconfirmedCommand { command };
    let tables = [
        (
            TableId::SideLight,
            ReadRequest::SideLight,
            unconfirmed("08 00"),
        ),
        (
            TableId::CustomColors {
                table: CustomTable::Custom2,
            },
            ReadRequest::CustomColors(CustomTable::Custom2),
            unconfirmed("09 01"),
        ),
        (
            TableId::ActiveCustomTable,
            ReadRequest::ActiveCustomTable,
            SkipReason::NoKnownCommand,
        ),
        (
            TableId::SocdPairs,
            ReadRequest::SocdPairs,
            unconfirmed("24 00"),
        ),
        (
            TableId::SocdSettings,
            ReadRequest::SocdSettings,
            unconfirmed("24 03"),
        ),
        (
            TableId::SwitchTypes,
            ReadRequest::SwitchTypes,
            unconfirmed("25 01"),
        ),
        (
            TableId::PollRate,
            ReadRequest::PollRate,
            unconfirmed("21/09 set"),
        ),
        (
            TableId::WinLockAndRollover,
            ReadRequest::Status,
            unconfirmed("14 00"),
        ),
    ];
    tables
        .into_iter()
        .map(|(table, request, reason)| {
            let compare = |raw: &RawResponse| -> Vec<u8> {
                match table {
                    // The active slot is byte 1 of the reply.
                    TableId::ActiveCustomTable => {
                        raw.first().map(|r| vec![r.sub()]).unwrap_or_default()
                    }
                    // Win-Lock options (byte 20) and 6-key rollover (byte 22).
                    TableId::WinLockAndRollover => {
                        let p = raw.payload();
                        vec![
                            p.get(15).copied().unwrap_or(0),
                            p.get(17).copied().unwrap_or(0),
                        ]
                    }
                    _ => content(raw),
                }
            };
            match (t.response(request), c.response(request)) {
                (None, _) => item(table, None, PlanAction::NotInBackup),
                (Some(_), None) => skip(
                    table,
                    None,
                    SkipReason::NotInKeyboard,
                    format!("the keyboard did not report {table}"),
                ),
                (Some(target), Some(current)) if compare(target) == compare(current) => {
                    item(table, None, PlanAction::Unchanged)
                }
                (Some(_), Some(_)) => {
                    let message = match &reason {
                        SkipReason::UnconfirmedCommand { command } => format!(
                            "{table} differs, but its write command ({command}) is not confirmed \
                             on hardware; it is not written"
                        ),
                        _ => format!(
                            "{table} differs, but no write command is known; it is not written"
                        ),
                    };
                    skip(table, None, reason, message)
                }
            }
        })
        .collect()
}

/// The state of one table of the keymap group.
enum GroupState {
    NotInBackup,
    NotInKeyboard,
    Unchanged,
    /// The table differs: the write, or why it cannot be made.
    Differs(Result<(WriteRequest, Vec<EntryChange>), BlockCause>),
}

/// The keymap group: both layers, ten macro slots and forty advanced-key
/// slots, planned as one unit (see the [module documentation](self)).
fn plan_keymap_group(t: SnapshotView<'_>, c: SnapshotView<'_>) -> Vec<PlanItem> {
    let pair = |request| (t.response(request), c.response(request));

    let layers: Vec<(TableId, GroupState)> = Layer::ALL
        .into_iter()
        .map(|layer| {
            let table = TableId::Keymap { layer };
            let state = match pair(ReadRequest::Keymap(layer)) {
                (None, _) => GroupState::NotInBackup,
                (Some(_), None) => GroupState::NotInKeyboard,
                (Some(target), Some(current)) => {
                    let (old, new) = (content(current), content(target));
                    if old == new {
                        GroupState::Unchanged
                    } else {
                        GroupState::Differs(match KeymapData::from_response(target) {
                            Ok((layer, data)) => Ok((
                                WriteRequest::Keymap(layer, data),
                                entry_changes(&old, &new, 4),
                            )),
                            Err(_) => Err(BlockCause::Incomplete { table }),
                        })
                    }
                }
            };
            (table, state)
        })
        .collect();

    let macros: Vec<(MacroSlotId, TableId, GroupState)> = MacroSlotId::all()
        .map(|slot| {
            let table = TableId::Macro { slot };
            let state = match pair(ReadRequest::Macro(slot)) {
                (None, _) => GroupState::NotInBackup,
                (Some(_), None) => GroupState::NotInKeyboard,
                (Some(target), Some(current)) => {
                    let (old, new) = (content(current), content(target));
                    match MacroData::from_response(target) {
                        Ok((_, data)) if data.is_equivalent(&old) => GroupState::Unchanged,
                        Ok((slot, data)) => GroupState::Differs(Ok((
                            WriteRequest::Macro(slot, data),
                            slot_change(usize::from(slot.get()), &old, &new),
                        ))),
                        Err(_) if old == new => GroupState::Unchanged,
                        Err(WriteBuildError::NonCanonicalMacro { slot, reason }) => {
                            GroupState::Differs(Err(BlockCause::NonCanonicalMacro { slot, reason }))
                        }
                        Err(_) => GroupState::Differs(Err(BlockCause::Incomplete { table })),
                    }
                }
            };
            (slot, table, state)
        })
        .collect();

    let advanced: Vec<(TableId, GroupState)> = {
        let (target, current) = pair(ReadRequest::AdvancedKeys);
        let target_bytes = target.map(content);
        let current_bytes = current.map(content);
        AdvancedSlotId::all()
            .map(|slot| {
                let table = TableId::AdvancedKey { slot };
                let start = usize::from(slot.get()) * ADVANCED_SLOT_LEN;
                let range = start..start + ADVANCED_SLOT_LEN;
                let state = match (&target_bytes, &current_bytes) {
                    (None, _) => GroupState::NotInBackup,
                    (Some(_), None) => GroupState::NotInKeyboard,
                    (Some(t), Some(c)) => match (t.get(range.clone()), c.get(range)) {
                        (Some(new), Some(old)) if new == old => GroupState::Unchanged,
                        (Some(new), Some(old)) if new.iter().all(|b| *b == 0) => {
                            GroupState::Differs(Ok((
                                WriteRequest::AdvancedKeyClear(slot),
                                slot_change(usize::from(slot.get()), old, new),
                            )))
                        }
                        (Some(_), Some(_)) => {
                            GroupState::Differs(Err(BlockCause::NonEmptyAdvancedKey {
                                slot: slot.get(),
                            }))
                        }
                        _ => GroupState::Differs(Err(BlockCause::Incomplete { table })),
                    },
                };
                (table, state)
            })
            .collect()
    };

    let all_states = || {
        layers
            .iter()
            .map(|(table, state)| (*table, state))
            .chain(macros.iter().map(|(_, table, state)| (*table, state)))
            .chain(advanced.iter().map(|(table, state)| (*table, state)))
    };
    let differs = all_states().any(|(_, state)| matches!(state, GroupState::Differs(_)));
    let mut cause = None;
    if differs {
        cause = all_states().find_map(|(table, state)| match state {
            GroupState::NotInBackup | GroupState::NotInKeyboard => {
                Some(BlockCause::Incomplete { table })
            }
            GroupState::Differs(Err(cause)) => Some(cause.clone()),
            _ => None,
        });
        if cause.is_none() {
            cause = Layer::ALL.into_iter().find_map(|layer| {
                let target = t.response(ReadRequest::Keymap(layer))?;
                let (_, data) = KeymapData::from_response(target).ok()?;
                data.references().into_iter().find_map(|r| match r {
                    KeymapReference::Malformed { index, .. } => {
                        Some(BlockCause::MalformedReference { layer, index })
                    }
                    _ => None,
                })
            });
        }
    }

    let to_item = |table: TableId, state: &GroupState| -> PlanItem {
        match state {
            GroupState::NotInBackup => item(table, None, PlanAction::NotInBackup),
            GroupState::NotInKeyboard => skip(
                table,
                None,
                SkipReason::NotInKeyboard,
                format!("the keyboard did not report {table}"),
            ),
            GroupState::Unchanged => item(table, None, PlanAction::Unchanged),
            GroupState::Differs(result) => match (&cause, result) {
                (Some(cause), _) => skip(
                    table,
                    None,
                    SkipReason::KeymapGroupBlocked {
                        cause: cause.clone(),
                    },
                    format!(
                        "{table} differs, but the keymap group (keymaps, macros, advanced keys) \
                         cannot be restored as a whole: {}",
                        block_message(cause)
                    ),
                ),
                (None, Ok((request, changes))) => {
                    item(table, None, write_action(request.clone(), changes.clone()))
                }
                (None, Err(cause)) => skip(
                    table,
                    None,
                    SkipReason::KeymapGroupBlocked {
                        cause: cause.clone(),
                    },
                    block_message(cause),
                ),
            },
        }
    };

    if cause.is_some() {
        return all_states()
            .map(|(table, state)| to_item(table, state))
            .collect();
    }

    // Execution order: macro content before the keymaps that bind it,
    // unbinding before clearing.
    let referenced: BTreeSet<u8> = Layer::ALL
        .into_iter()
        .filter_map(|layer| t.response(ReadRequest::Keymap(layer)))
        .filter_map(|raw| KeymapData::from_response(raw).ok())
        .flat_map(|(_, data)| data.references())
        .filter_map(|r| match r {
            KeymapReference::Macro { slot, .. } => Some(slot.get()),
            _ => None,
        })
        .collect();
    let bound_first = |slot: &MacroSlotId, state: &GroupState| {
        referenced.contains(&slot.get()) && matches!(state, GroupState::Differs(Ok(_)))
    };
    let mut items = Vec::new();
    items.extend(
        macros
            .iter()
            .filter(|(slot, _, state)| bound_first(slot, state))
            .map(|(_, table, state)| to_item(*table, state)),
    );
    items.extend(layers.iter().map(|(table, state)| to_item(*table, state)));
    items.extend(
        macros
            .iter()
            .filter(|(slot, _, state)| !bound_first(slot, state))
            .map(|(_, table, state)| to_item(*table, state)),
    );
    items.extend(advanced.iter().map(|(table, state)| to_item(*table, state)));
    items
}

/// Human-readable form of a [`BlockCause`].
fn block_message(cause: &BlockCause) -> String {
    match cause {
        BlockCause::NonEmptyAdvancedKey { slot } => format!(
            "advanced-key slot {slot} of the backup is not empty, and writing non-empty \
             advanced keys is not confirmed on hardware"
        ),
        BlockCause::NonCanonicalMacro { slot, reason } => {
            format!("macro slot {slot} of the backup is not in the known layout ({reason})")
        }
        BlockCause::MalformedReference { layer, index } => format!(
            "the backup's {} keymap has a malformed macro or advanced-key entry at key index \
             {index}",
            match layer {
                Layer::Base => "base",
                Layer::Fn => "Fn",
            }
        ),
        BlockCause::Incomplete { table } => {
            format!("{table} is missing or unusable in the backup or on the keyboard")
        }
    }
}
