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
//! write command (side light, custom colours 2, active custom table, switch
//! types, polling rate, Win-Lock / 6-key) are compared and reported as
//! skipped when they differ. SOCD (power and pairs) is written as one burst
//! (`24 03`, `24 00`) when the backup's pairs are in the vendor site's form;
//! a difference only in the read-only bytes of `24 02` (the hotkey) is
//! skipped ([`SkipReason::ReadOnlyBytes`]). The keymap group (both keymap
//! layers, the ten macro slots and the forty advanced-key slots) is planned
//! as one unit and skipped as a whole when it would need an advanced-key
//! slot that is not a DKS, Mod-Tap or Toggle OpenWraith can write
//! ([`BlockCause::UnwritableAdvancedKey`]), a non-canonical macro, or holds
//! a malformed reference.
//!
//! The keymap group is written in the vendor site's order (spec 4.8.2):
//! every macro slot that changes (content and clears), the base keymap, the
//! Fn keymap, the advanced-key slots that change (content and clears). The
//! keyboard keeps a base-keymap write (`18 00`) only when an Fn-keymap write
//! (`18 02`) follows it (CONFIRMED on hardware), so whenever the base keymap
//! is written, the Fn keymap is written right after it, re-sent unchanged if
//! it already holds the backup's bytes ([`CommitTriggerReason::BaseKeymap`];
//! a [`CommitTrigger`] is sent even when the keyboard holds it). When macro
//! or advanced-key slots are written but neither keymap layer differs, the
//! unchanged Fn keymap is written between the macro slots and the
//! advanced-key slots, as the site does (a precaution for the macro slots,
//! whose need for it is untested;
//! [`CommitTriggerReason::MacrosOrAdvancedKeys`]; advanced-key slots written
//! after a complete Fn keymap persisted twice). If the Fn keymap cannot be
//! written, the group is not written at all
//! ([`BlockCause::FnKeymapNotWritable`]). A cancellation asked for after a
//! group table was sent skips the group's other tables up to that Fn-keymap
//! write ([`ItemResult::NotAttempted`]), still sends it (an `18 02` alone is
//! kept) and takes effect after it; a slot that a written layer refers to
//! ([`PlannedWrite::completes_binding`]) is still written after a
//! cancellation. A commit trigger must reach the keyboard complete: every
//! packet acknowledged, or the whole table is sent once more
//! ([`Writer::write_commit_trigger`]). If the restore stops between the base
//! keymap and the Fn keymap anyway (another program, a failure), the base
//! keymap is reported [`ItemResult::MaybeLost`] with
//! [`WriteWarning::BaseKeymapNotSaved`]; a key left referring to an empty
//! slot is listed in [`RestoreReport::dangling`]. The persistence check
//! stays the safety net: a table the keyboard did not keep is reported lost.
//!
//! # Importing one profile into another
//!
//! With [`RestoreOptions::import`] a restore compares one profile of a file
//! (an exported profile, [`Backup::export_profile`]) with **another**
//! profile of the keyboard and writes only the chosen categories
//! ([`ImportSelection`]; the others are skipped as
//! [`SkipReason::NotImported`]). The file is untrusted input. Triggers,
//! the main light, the dead band and SOCD go through the same validated
//! builders as any restore. The keymap layers and custom colour table 1
//! are not written as the file holds them but merged into the keyboard's
//! tables: positions that are not keys, the knob and its slots, the Fn key,
//! Fn-layer keys the keyboard locks and colour slots outside the layout
//! keep the keyboard's bytes ([`PlanWarning::ImportKeptAsRead`]); every
//! other entry must be one an editor could write
//! ([`KeyAssignment::classify`](openwraith_protocol::KeyAssignment::classify),
//! [`check_placement`]), else the keymap group is not imported
//! ([`BlockCause::UnwritableImportedEntry`]); a macro must be key presses
//! and releases OpenWraith can show; an Fn layer that would replace a
//! critical built-in function (Fn + 1..4, Fn + Space) is refused unless
//! allowed explicitly ([`BlockCause::CriticalFnOverride`]). An exported
//! file ([`BACKUP_SCOPE_EXPORT`](crate::BACKUP_SCOPE_EXPORT)) is only ever
//! imported: a plain restore of it is refused.
//!
//! Trigger and dead-band values are stored in the keyboard's distance unit
//! (`21/04` unit x status high-precision flag). They are only restored when
//! the backup and the keyboard both use the 0.02 mm unit; otherwise the same
//! numbers would mean other distances, and they are skipped
//! ([`SkipReason::DistanceUnitChanged`]). Values are never converted.

use std::collections::{BTreeSet, HashMap};
use std::ops::ControlFlow;
use std::path::PathBuf;

use openwraith_protocol::write::ADVANCED_SLOT_LEN;
use openwraith_protocol::{
    AdvancedSlotData, AdvancedSlotId, BuiltinAck, ColorTableData, CustomTable, DeadBandData,
    DeadBandWrite, Distance, EntryKind, HexBytes, KeyAssignment, KeyMask, KeyPos, KeymapData,
    KeymapReference, Layer, LightLimits, LightRecord, MacroData, MacroProgram, MacroSlotId,
    MainLightWrite, Mismatch, MismatchKind, Model, PlacementError, ProfileId, RawResponse,
    ReadRequest, Rgb, SocdPairsData, SocdWrite, TableId, TriggerLimits, TriggerSetting,
    TriggerWrite, UnverifiedNote, WriteBuildError, WriteRequest, check_placement,
};
use serde::Serialize;

use crate::backup::{
    BACKUP_SCOPE_EXPORT, Backup, BackupOptions, Progress, SnapshotView, read_all_cancellable,
};
use crate::error::{ErrorInfo, ErrorKind, RestoreError, WriteError};
use crate::group::{
    DanglingBinding, GroupCursor, GroupView, GroupWrite, Step, dangling_after, layer_index,
};
use crate::persist::{CheckOutcome, recheck, reload_profile, socd_toggled};
use crate::profiles::{
    ProfileSet, ProfilesOptions, ProfilesProgress, read_other_profiles, read_profiles,
};
use crate::safety::SavedBackup;
use crate::session::{ForeignActivity, Session};
use crate::summary::identify;
use crate::transport::{Transport, WriteTransport};
use crate::writer::{
    CommitTrigger, CommitWait, PacketProgress, WriteConsent, WriteOptions, WriteWarning, Writer,
};

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
    /// Import one profile of the file into another profile of the keyboard
    /// instead of restoring profiles to themselves (see the
    /// [module documentation](self)).
    pub import: Option<ImportOptions>,
}

/// Which categories of settings an import writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ImportSelection {
    /// Per-key actuation and Rapid Trigger.
    pub triggers: bool,
    /// Main light and custom colour table 1.
    pub lighting: bool,
    /// Keymap layers, macros and advanced keys.
    pub keymap_group: bool,
    /// Dead band (its profile scope is unknown).
    pub dead_band: bool,
    /// SOCD (its profile scope is unknown).
    pub socd: bool,
}

impl Default for ImportSelection {
    /// Triggers and lighting; keys, dead band and SOCD only when chosen.
    fn default() -> Self {
        ImportSelection {
            triggers: true,
            lighting: true,
            keymap_group: false,
            dead_band: false,
            socd: false,
        }
    }
}

impl ImportSelection {
    /// Whether `table` belongs to a chosen category (tables without a write
    /// command are compared as always).
    pub fn includes(&self, table: TableId) -> bool {
        match table {
            TableId::Triggers => self.triggers,
            TableId::MainLight
            | TableId::CustomColors {
                table: CustomTable::Custom1,
            } => self.lighting,
            TableId::Keymap { .. } | TableId::Macro { .. } | TableId::AdvancedKey { .. } => {
                self.keymap_group
            }
            TableId::DeadBand => self.dead_band,
            TableId::Socd => self.socd,
            _ => true,
        }
    }
}

/// Import one profile of a file into a profile of the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ImportOptions {
    /// The profile of the file.
    pub from: ProfileId,
    /// The profile of the keyboard that is written.
    pub into: ProfileId,
    /// The categories written.
    pub include: ImportSelection,
    /// Allow an Fn layer that replaces a critical built-in function
    /// (Fn + 1..4, Fn + Space).
    pub allow_critical_fn_overrides: bool,
}

/// Progress of a restore or an edit. Returning [`ControlFlow::Break`] from
/// the callback cancels at the next table boundary; the original profile is
/// always selected again. Inside a restore's keymap group, once a group
/// table was sent, the cancellation skips the group's other tables but still
/// sends the group's Fn keymap (`18 02`), without which the keyboard does not
/// keep the base keymap (spec 4.8.2), and takes effect after it.
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
    /// Human-readable description, e.g. `base keymap (18 00)`, or `Fn keymap
    /// (18 02) re-sent unchanged to make the keyboard save the base keymap`.
    pub summary: String,
    /// Packets the write sends (its first attempt).
    pub packets: usize,
    /// The entries that change (key index, colour slot, macro slot, ...),
    /// with the keyboard's bytes and the backup's bytes. Empty for an
    /// unchanged Fn keymap re-sent as the commit trigger.
    pub changes: Vec<EntryChange>,
    /// Set when this write is the keymap group's commit trigger: the Fn
    /// keymap (`18 02`), sent even when the keyboard already holds it (see
    /// [`CommitTriggerReason`]). Without `changes` it re-sends the
    /// keyboard's Fn keymap unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_trigger: Option<CommitTriggerReason>,
    /// Whether the write completes a binding of a layer written in the same
    /// keymap group (the content of a slot a written layer refers to, or a
    /// layer that stops referring to a slot cleared in the group): it is
    /// still written after a cancellation, so that no key is left referring
    /// to an empty or stale slot.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub completes_binding: bool,
    /// The tables whose binding it completes (written layers that refer to
    /// the slot, or the macro slots cleared before the layer).
    #[serde(skip)]
    pub(crate) binding_tables: Vec<TableId>,
}

impl PlannedWrite {
    /// Whether this write re-sends an unchanged Fn keymap as the commit
    /// trigger of the keymap group (the keyboard held the backup's Fn keymap
    /// when the restore was planned).
    pub fn is_resent_unchanged(&self) -> bool {
        self.commit_trigger.is_some() && self.changes.is_empty()
    }
}

/// Why the Fn keymap (`18 02`) is written as the keymap group's commit
/// trigger, also when the keyboard already holds it.
///
/// The keyboard saves the base keymap (`18 00`) only when the Fn keymap is
/// written after it: written alone, the base keymap reads back at once but
/// is gone after a profile switch, even 6 s later (CONFIRMED on hardware,
/// spec 4.8.2). The vendor site always writes the macro slots, the base
/// keymap, the Fn keymap and the advanced-key slots, in that order, and a
/// restore keeps that order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitTriggerReason {
    /// The base keymap is written right before it (CONFIRMED rule). It is
    /// the next write after the base keymap.
    BaseKeymap,
    /// Neither keymap layer differs, but macro or advanced-key slots are
    /// written: the unchanged Fn keymap is placed after the macro slots and
    /// before the advanced-key slots, the vendor site's order (a precaution
    /// for the macro slots, whose need for it is untested; advanced-key
    /// slots written after a complete `18 02` persisted, spec 4.8.2).
    MacrosOrAdvancedKeys,
}

impl CommitTriggerReason {
    /// The summary of an unchanged Fn keymap re-sent for this reason.
    fn resent_summary(self) -> &'static str {
        match self {
            CommitTriggerReason::BaseKeymap => {
                "Fn keymap (18 02) re-sent unchanged to make the keyboard save the base keymap"
            }
            CommitTriggerReason::MacrosOrAdvancedKeys => {
                "Fn keymap (18 02) re-sent unchanged to make sure the keyboard saves the macro \
                 and advanced-key slots (precaution)"
            }
        }
    }
}

/// One changed entry of a planned write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryChange {
    /// Entry index: the key index (keymap, dead band, trigger), the colour
    /// slot, the macro or advanced-key slot, 0 for the light record; for
    /// SOCD 0 is the power byte and 1-20 the pair entries 0-19.
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
    /// Only bytes that are never written differ (the SOCD hotkey bytes of
    /// `24 02`).
    ReadOnlyBytes,
    /// The backup's SOCD pairs are not in the vendor site's form (gaps,
    /// `07 FF FF FF` entries, keys the site does not offer).
    UnwritableSocdPairs {
        /// Which rule they break.
        rule: &'static str,
    },
    /// An import does not write this category ([`ImportSelection`]).
    NotImported,
}

/// Why the keymap group (keymaps, macros, advanced keys) is not restored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockCause {
    /// An advanced-key slot of the backup differs from the keyboard and is
    /// not a DKS, Mod-Tap or Toggle that OpenWraith can write exactly
    /// ([`AdvancedSlotData::from_read_slot`]).
    UnwritableAdvancedKey {
        /// The slot.
        slot: u8,
        /// Why.
        reason: String,
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
    /// The group must end with an Fn-keymap write (the commit trigger, see
    /// [`CommitTriggerReason`]), but the backup's Fn keymap cannot be
    /// written. The base keymap is never written alone: the keyboard would
    /// not keep it (spec 4.8.2).
    FnKeymapNotWritable {
        /// Why the Fn keymap had to be written.
        reason: CommitTriggerReason,
    },
    /// An imported Fn layer replaces a critical built-in function (Fn +
    /// 1..4, Fn + Space) and that was not allowed
    /// ([`ImportOptions::allow_critical_fn_overrides`]).
    CriticalFnOverride {
        /// The key index.
        index: u8,
    },
    /// An imported keymap entry at a key is not one OpenWraith writes: not
    /// an assignment of the confirmed categories
    /// ([`KeyAssignment::classify`](openwraith_protocol::KeyAssignment::classify);
    /// lighting, function, gamepad and unknown entries, a macro entry for
    /// another key), or one that cannot go to that key.
    UnwritableImportedEntry {
        /// The layer.
        layer: Layer,
        /// The key index.
        index: u8,
        /// The file's entry.
        entry: HexBytes,
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
    /// An import: profile `from` of the file is written to profile `into`
    /// of the keyboard.
    Imported {
        /// The profile of the file.
        from: u8,
        /// The profile of the keyboard.
        into: u8,
    },
    /// An import writes these macro slots with content (shown as steps by a
    /// user interface before anything is written).
    ImportedMacros {
        /// The slots.
        slots: Vec<u8>,
    },
    /// The planned Fn layer replaces a built-in Fn function.
    FnBuiltinOverridden {
        /// The key index.
        index: u8,
        /// Whether it is a critical one (Fn + 1..4, Fn + Space).
        critical: bool,
    },
    /// An import keeps the keyboard's entries at these positions of a
    /// layer although the file's differ: positions that are not keys, the
    /// knob and its slots, the Fn key, and Fn-layer keys the keyboard locks
    /// (`0E`) are never imported.
    ImportKeptAsRead {
        /// The layer.
        layer: Layer,
        /// The key indices.
        indices: Vec<u8>,
    },
    /// An imported keymap entry whose effect is not yet verified on a W75
    /// (the vendor site sends the same bytes).
    Unverified {
        /// The layer.
        layer: Layer,
        /// The key index.
        index: u8,
        /// What is not verified.
        note: UnverifiedNote,
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
        let source = source_profile(options, profile);
        let Some(t) = target.snapshot(source) else {
            return Err(RestoreError::ProfileNotInBackup {
                profile: source.get(),
            });
        };
        let plan = match current.snapshot(profile) {
            Some(c) => {
                let guard = options
                    .import
                    .as_ref()
                    .map(|import| ImportGuard::new(model, import, c, current.primary()));
                let mut items = plan_profile(model, t, c, guard.as_ref(), &mut warnings);
                if let Some(import) = &options.import {
                    warnings.extend(apply_import(import, &mut items));
                }
                ProfilePlan {
                    profile,
                    compared: true,
                    items,
                }
            }
            None => ProfilePlan {
                profile,
                compared: false,
                items: Vec::new(),
            },
        };
        profiles.push(plan);
    }
    if let Some(import) = &options.import {
        warnings.insert(
            0,
            PlanWarning::Imported {
                from: import.from.get(),
                into: import.into.get(),
            },
        );
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

    /// The snapshot of the file that is written to keyboard profile
    /// `profile`: the same profile, or for an import the file's `from`
    /// profile when `profile` is `into`.
    pub fn target_snapshot(&self, profile: ProfileId) -> Option<SnapshotView<'_>> {
        self.target.snapshot(source_profile(&self.options, profile))
    }
}

/// The profile of the file written to keyboard profile `profile`.
fn source_profile(options: &RestoreOptions, profile: ProfileId) -> ProfileId {
    match &options.import {
        Some(import) if import.into == profile => import.from,
        _ => profile,
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
    /// was cancelled or failed does not check either, and an unchanged Fn
    /// keymap re-sent as the commit trigger is never checked (it reads the
    /// same either way; see [`PersistenceCheck`]). A write whose save was
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
    /// (spec 4.5.3, "Delayed commit"). Also a base keymap after which the
    /// restore stopped before the Fn-keymap write that makes the keyboard
    /// save it (its warnings then hold [`WriteWarning::BaseKeymapNotSaved`]:
    /// the keyboard drops it at the next profile switch or power loss, spec
    /// 4.8.2). It was not read again; the operation failed.
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
    /// Set when the write is the keymap group's commit trigger (see
    /// [`PlannedWrite::commit_trigger`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_trigger: Option<CommitTriggerReason>,
    /// Whether it re-sends an unchanged Fn keymap as the commit trigger
    /// ([`PlannedWrite::is_resent_unchanged`]): written, but no setting
    /// changes.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub resent_unchanged: bool,
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
///
/// An unchanged Fn keymap re-sent as the keymap group's commit trigger
/// ([`ItemReport::resent_unchanged`]) is not counted and stays
/// [`Persistence::NotChecked`]: it reads the same whether the keyboard kept
/// it or not, so reading it again proves nothing. The tables it made the
/// keyboard save (the base keymap) are checked themselves.
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
    /// Keys left referring to an empty macro or advanced-key slot because
    /// the restore stopped part-way through a keymap group (the undo command
    /// restores them).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dangling: Vec<DanglingBinding>,
    /// A cancellation was asked for (Ctrl+C), but every write still made
    /// after it was needed to keep the keymap group consistent (the Fn
    /// keymap that makes the keyboard save a base keymap, a slot a written
    /// key refers to): with [`Outcome::Completed`] everything planned was
    /// written.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cancel_requested: bool,
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

    /// Number of written items that only re-sent an unchanged Fn keymap as
    /// the keymap group's commit trigger ([`ItemReport::resent_unchanged`]);
    /// they are included in [`written`](Self::written).
    pub fn resent_unchanged(&self) -> usize {
        self.items()
            .filter(|(_, i)| i.resent_unchanged && matches!(i.result, ItemResult::Written { .. }))
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
                        commit_trigger: write.commit_trigger,
                        resent_unchanged: write.is_resent_unchanged(),
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
        dangling: Vec::new(),
        cancel_requested: false,
    };
    if !saved.describes(&prepared.safety) {
        let error = RestoreError::SafetyBackupMismatch {
            path: saved.path().to_path_buf(),
        };
        report.fail(ErrorInfo::new(error.kind(), &error));
        return report;
    }
    if let Err(e) = consent.ensure_not_standing_rule() {
        report.fail(ErrorInfo::new(e.kind(), &e));
        report.active_profile_now = Some(original.get());
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
        socd_mark: None,
        packet_sent: false,
        started: Vec::new(),
    };
    let result = run.all_profiles(&mut writer, &order, &mut report);
    report.cancel_requested = run.cancel;
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
    if !report.is_success() {
        report.dangling = restore_dangling(prepared, &report, &run.started);
    }
    report
}

/// The keys a restore that stopped part-way left referring to an empty
/// slot (or may have): per written profile, the keymap group as the safety
/// backup read it with every table the restore sent applied, also a table
/// that failed after some of its packets went out (`started`: profile and
/// item index).
fn restore_dangling(
    prepared: &PreparedRestore,
    report: &RestoreReport,
    started: &[(u8, usize)],
) -> Vec<DanglingBinding> {
    let mut out = Vec::new();
    for profile_report in &report.profiles {
        let Some(profile) = ProfileId::new(profile_report.profile) else {
            continue;
        };
        let Some(before) = prepared.safety.snapshot(profile).map(group_view) else {
            continue;
        };
        let mut after = before.clone();
        let writes = prepared
            .plan
            .writes()
            .filter(|(p, _, _)| *p == profile)
            .map(|(_, _, w)| w);
        for (index, (write, item)) in writes.zip(&profile_report.items).enumerate() {
            if item.result.was_sent() || started.contains(&(profile.get(), index)) {
                apply_to_view(&mut after, &write.request);
            }
        }
        out.extend(dangling_after(profile.get(), &before, &after));
    }
    out
}

/// The keymap group of a snapshot as a [`GroupView`] (unknown layers and
/// unreadable slots count as present, so they are never reported).
pub(crate) fn group_view(view: SnapshotView<'_>) -> GroupView {
    let mut group = GroupView {
        macros: [true; 10],
        advanced: [true; 40],
        ..GroupView::default()
    };
    for layer in Layer::ALL {
        group.layers[layer_index(layer)] = view
            .response(ReadRequest::Keymap(layer))
            .and_then(|raw| KeymapData::from_response(raw).ok())
            .map(|(_, data)| data);
    }
    for slot in MacroSlotId::all() {
        if let Some(raw) = view.response(ReadRequest::Macro(slot)) {
            let bytes = content(raw);
            group.macros[usize::from(slot.get())] =
                !(bytes.iter().all(|b| *b == 0) || bytes.iter().all(|b| *b == 0xFF));
        }
    }
    if let Some(raw) = view.response(ReadRequest::AdvancedKeys) {
        let bytes = content(raw);
        for (slot, chunk) in bytes.chunks(ADVANCED_SLOT_LEN).take(40).enumerate() {
            group.advanced[slot] = chunk.iter().any(|b| *b != 0);
        }
    }
    group
}

/// Applies a sent write to a [`GroupView`].
pub(crate) fn apply_to_view(view: &mut GroupView, request: &WriteRequest) {
    match request {
        WriteRequest::Keymap(layer, data) => view.layers[layer_index(*layer)] = Some(data.clone()),
        WriteRequest::Macro(slot, data) => view.macros[usize::from(slot.get())] = !data.is_empty(),
        WriteRequest::AdvancedKeyClear(slot) => view.advanced[usize::from(slot.get())] = false,
        WriteRequest::AdvancedKey(slot, _) => view.advanced[usize::from(slot.get())] = true,
        _ => {}
    }
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

    /// The restore stopped after writing the base keymap of `profile` and
    /// before the Fn-keymap write that makes the keyboard save it: the base
    /// keymap becomes [`ItemResult::MaybeLost`] with the warning
    /// [`WriteWarning::BaseKeymapNotSaved`] (the keyboard drops it at the
    /// next profile switch or power loss, spec 4.8.2).
    fn note_base_keymap_unsaved(&mut self, profile: ProfileId) {
        let base = TableId::Keymap { layer: Layer::Base };
        for item in self
            .profiles
            .iter_mut()
            .filter(|p| p.profile == profile.get())
            .flat_map(|p| p.items.iter_mut())
            .filter(|i| i.table == base)
        {
            if let ItemResult::Written { warnings, .. } = &mut item.result {
                warnings.push(WriteWarning::BaseKeymapNotSaved);
            }
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

/// One planned write of a profile, for the [`GroupCursor`].
#[derive(Clone, Copy)]
struct RestoreWrite<'a> {
    table: TableId,
    key: Option<KeyPos>,
    write: &'a PlannedWrite,
}

impl GroupWrite for RestoreWrite<'_> {
    fn table(&self) -> TableId {
        self.table
    }

    fn is_trigger(&self) -> bool {
        self.write.commit_trigger.is_some()
    }

    fn binding_tables(&self) -> &[TableId] {
        &self.write.binding_tables
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
    /// The session's SOCD push count right after the last SOCD write (a
    /// later switch on the keyboard is not a lost write, see
    /// [`socd_toggled`]).
    socd_mark: Option<u64>,
    /// Whether a packet of the table being written went out.
    packet_sent: bool,
    /// (profile, item index) of every table at least one packet of which
    /// went out.
    started: Vec<(u8, usize)>,
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
        let writes: Vec<RestoreWrite<'_>> = self
            .prepared
            .plan
            .writes()
            .filter(|(p, _, _)| *p == profile)
            .map(|(_, item, write)| RestoreWrite {
                table: item.table,
                key: item.key,
                write,
            })
            .collect();
        if writes.is_empty() {
            return Ok(());
        }
        // The keymap group's commit trigger (the Fn keymap, spec 4.8.2) and
        // the cancellation rules around it (see `crate::group`).
        let mut cursor = GroupCursor::new(&writes);
        for (index, planned) in writes.iter().enumerate() {
            let RestoreWrite { table, key, write } = *planned;
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
            let cancelled = self.cancel;
            let step = cursor.before(index, planned, cancelled, &mut || {
                self.should_stop(profile, table, key, write)
            });
            match step {
                Step::Stop => return Err(Stop::Cancelled),
                // Left out (NotAttempted); the cancellation takes effect
                // after the writes the group still needs.
                Step::Skip => continue,
                Step::Send => {}
            }
            if index == 0 {
                // Nothing is left to wait for (unless the session wrote in
                // between, which it does not).
                let progress = &mut *self.progress;
                writer.select_profile_observed(profile, &mut |wait| {
                    let _ = progress(WriteProgress::waiting(wait));
                })?;
            }
            self.packet_sent = false;
            let result = self.one_table(writer, profile, table, key, write);
            if self.packet_sent {
                self.started.push((profile.get(), index));
            }
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
            let sent = item_result.was_sent();
            if sent && table == TableId::Socd {
                self.socd_mark = Some(writer.session().socd_pushes());
            }
            if let Some(item) = report.item_mut(profile, index) {
                item.result = item_result;
            }
            cursor.after(index, planned, sent);
            if let Some(stop) = stop {
                if cursor.base_unsaved_on_stop() {
                    report.note_base_keymap_unsaved(profile);
                }
                return Err(stop);
            }
        }
        if cursor.cancelled_with_skips() {
            // Cancelled inside the keymap group: the cancellation takes
            // effect now. (When nothing was skipped, every planned write of
            // the profile was made, and a later profile stops at its first
            // table.)
            return Err(Stop::Cancelled);
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

        // The commit trigger is sent even when the keyboard already holds it;
        // only an Fn-keymap write can be one.
        let trigger =
            match write.commit_trigger {
                Some(_) => Some(CommitTrigger::try_from(request.clone()).map_err(|other| {
                    TableFailure {
                        error: ErrorInfo {
                            kind: ErrorKind::InvalidRequest,
                            message: format!("{other} cannot be a keymap commit trigger"),
                        },
                        mismatch: None,
                    }
                })?),
                None => None,
            };
        let checked = match &trigger {
            Some(trigger) => Checked::CommitTrigger(trigger),
            None => Checked::Table(request),
        };

        let progress = &mut *self.progress;
        let cancel = &mut self.cancel;
        let packet_sent = &mut self.packet_sent;
        let mut on_packet = |p: PacketProgress| {
            *packet_sent = true;
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
            checked,
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
        // An unchanged Fn keymap re-sent as the commit trigger has nothing
        // to check (see check_profile_persisted).
        let written: Vec<ProfileId> = report
            .profiles
            .iter()
            .filter(|p| {
                p.items
                    .iter()
                    .any(|i| !i.resent_unchanged && matches!(i.result, ItemResult::Written { .. }))
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
            // When it is the active profile, leave it first, so that
            // selecting it again reloads what the keyboard saved.
            let away = if profile == original {
                ProfileId::ALL
                    .into_iter()
                    .find(|p| *p != profile)
                    .unwrap_or(original)
            } else {
                original
            };
            reload_profile(writer, profile, away, &mut on_wait).map_err(failure)?;
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
            if item.resent_unchanged {
                // It holds what the keyboard held before: reading it again
                // proves nothing. It stays NotChecked; the tables it made
                // the keyboard save are checked themselves.
                continue;
            }
            // A table that may be shared by all profiles can hold what this
            // restore wrote to it in another profile; the final check
            // compares it with the backup.
            let table = item.table;
            let states = possibly_global(table)
                .then(|| self.written.get(&(table, item.key)))
                .flatten();
            let written_elsewhere =
                |state: &TableState| states.is_some_and(|states| states.contains(state));
            match recheck(
                writer,
                &write.request,
                &mut item.result,
                &written_elsewhere,
                self.socd_mark,
            )? {
                CheckOutcome::Persisted => check.persisted += 1,
                CheckOutcome::Lost => check.lost += 1,
                CheckOutcome::NotChecked => {
                    if matches!(item.result, ItemResult::Written { .. }) {
                        check.not_checked += 1;
                    }
                }
            }
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
            let pushes = writer.session().socd_pushes();
            let mut toggled: Vec<usize> = Vec::new();
            for (index, (write, item)) in writes.zip(done).enumerate() {
                if !matches!(
                    item.result,
                    ItemResult::Written { .. } | ItemResult::AlreadyMatched
                ) {
                    continue;
                }
                if let Err(mismatch) = write.request.check(replies) {
                    // SOCD switched on the keyboard since it was written:
                    // the user's switch, not a lost write.
                    let now = state_of(&write.request, replies);
                    if socd_toggled(pushes, &write.request, &now, self.socd_mark).is_some() {
                        toggled.push(index);
                        continue;
                    }
                    mismatches.push(FinalMismatch {
                        profile: profile.get(),
                        mismatch,
                    });
                }
            }
            for index in toggled {
                let Some(item) = report.item_mut(*profile, index) else {
                    continue;
                };
                let now = power_of(replies);
                if let ItemResult::Written {
                    warnings,
                    persistence,
                    ..
                } = &mut item.result
                {
                    *persistence = Persistence::NotChecked;
                    let warning = WriteWarning::SocdToggledOnKeyboard { power: now };
                    if !warnings.contains(&warning) {
                        warnings.push(warning);
                    }
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
                self.prepared.target_snapshot(*profile),
            ) else {
                continue;
            };
            let socd_switched = self
                .socd_mark
                .is_some_and(|mark| writer.session().socd_pushes() > mark);
            for item in plan
                .items
                .iter()
                .filter(|i| matches!(i.action, PlanAction::Unchanged) && possibly_global(i.table))
            {
                let mismatch = if socd_switched && item.table == TableId::Socd {
                    // SOCD was switched on the keyboard after a SOCD write
                    // of this restore: its power is the user's; the pairs
                    // are still compared.
                    socd_mismatch(ReadRequest::SocdPairs, target, replies)
                } else {
                    unchanged_mismatch(item.table, target, replies)
                };
                if let Some(mismatch) = mismatch {
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

/// What [`write_checked`] writes.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Checked<'a> {
    /// A table write: nothing is sent when the keyboard already holds it.
    Table(&'a WriteRequest),
    /// The Fn keymap as the keymap group's commit trigger: sent also when
    /// the keyboard already holds it (spec 4.8.2).
    CommitTrigger(&'a CommitTrigger),
}

/// One guarded table write, shared by restores and edits:
///
/// 1. refuses when another program was seen while writing;
/// 2. confirms with `21/11` that `profile` is active;
/// 3. re-reads exactly the bytes the request changes: when the keyboard
///    already holds them the result is [`ItemResult::AlreadyMatched`]
///    (except for a [`Checked::CommitTrigger`], which is then sent
///    unchanged); when they are neither `planned` (the state the write was
///    planned against) nor one of `also_expected` (states this operation
///    wrote), the result is a `changed_since_plan` failure and nothing is
///    written;
/// 4. writes with [`Writer::write_observed`] (stop-and-wait, verification,
///    one more attempt with the whole table); a [`Checked::CommitTrigger`]
///    with [`Writer::write_trigger_observed`], which also needs every
///    packet acknowledged.
pub(crate) fn write_checked<T: WriteTransport>(
    writer: &mut Writer<'_, T>,
    profile: ProfileId,
    checked: Checked<'_>,
    planned: &[Option<Vec<u8>>],
    also_expected: &[TableState],
    on_packet: &mut dyn FnMut(PacketProgress),
) -> Result<ItemResult, TableFailure> {
    if let Some(evidence) = writer.foreign_activity_seen() {
        return Err(WriteError::OtherProgramActive { evidence }.into());
    }
    confirm_profile(writer, profile)?;

    let request = match checked {
        Checked::Table(request) => request,
        Checked::CommitTrigger(trigger) => trigger.request(),
    };
    let reads = request.verify_reads();
    // A SOCD switch on the keyboard from here on (also during the reads
    // below, after the power byte was read) is reported by the write.
    let socd_mark = writer.session().socd_pushes();
    let mut fresh = Vec::with_capacity(reads.len());
    for read in &reads {
        fresh.push(writer.read(*read)?);
    }
    if request.check(&fresh).is_ok() {
        if let Checked::CommitTrigger(_) = checked {
            // Unchanged: re-sent so that the keyboard saves the keymap.
            return written(writer, checked, on_packet, socd_mark);
        }
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
    written(writer, checked, on_packet, socd_mark)
}

/// Step 4 of [`write_checked`]: the write itself, with verification (for a
/// commit trigger also of every packet's acknowledgement, see
/// [`Writer::write_commit_trigger`]).
fn written<T: WriteTransport>(
    writer: &mut Writer<'_, T>,
    checked: Checked<'_>,
    on_packet: &mut dyn FnMut(PacketProgress),
    socd_mark: u64,
) -> Result<ItemResult, TableFailure> {
    let outcome = match checked {
        Checked::Table(request) => writer.write_observed_marked(request, on_packet, socd_mark)?,
        Checked::CommitTrigger(trigger) => writer.write_trigger_observed(trigger, on_packet)?,
    };
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
    if table == TableId::Socd {
        return [ReadRequest::SocdSettings, ReadRequest::SocdPairs]
            .into_iter()
            .find_map(|read| socd_mismatch(read, target, replies));
    }
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

/// The difference of one SOCD read (power byte of `24 02`, the pairs of
/// `24 01`) between the backup and the keyboard's `replies`, if any.
fn socd_mismatch(
    read: ReadRequest,
    target: SnapshotView<'_>,
    replies: &[RawResponse],
) -> Option<Mismatch> {
    let part = |raw: &RawResponse| match read {
        ReadRequest::SocdSettings => raw.payload().get(..1).map(<[u8]>::to_vec),
        _ => Some(content(raw)),
    };
    let expected = part(target.response(read)?)?;
    let mismatch = |kind, got: Vec<u8>| Mismatch {
        table: TableId::Socd,
        read,
        kind,
        expected: HexBytes(expected.clone()),
        got: HexBytes(got),
    };
    let Some(reply) = replies.iter().rev().find(|r| r.request() == read) else {
        return Some(mismatch(MismatchKind::MissingReply, Vec::new()));
    };
    let got = part(reply).unwrap_or_default();
    (got != expected).then(|| mismatch(MismatchKind::Differs, got))
}

/// SOCD's power in the `24 02` reply among `replies` (on = 1).
fn power_of(replies: &[RawResponse]) -> bool {
    replies
        .iter()
        .rev()
        .find(|r| r.request() == ReadRequest::SocdSettings)
        .and_then(|r| r.payload().first().copied())
        == Some(1)
}

/// Whether a writable table's profile scope is unknown (it may be shared by
/// all profiles): the dead band and SOCD. Main light, triggers, both keymap
/// layers, macros, advanced keys and custom colours 1 are per profile
/// (OBSERVED, c04 / c10 / c16 and the all-profile backups of 2026-10-24 and
/// 2026-10-27; spec 4.5.4, 7.3, 7.4).
pub(crate) fn possibly_global(table: TableId) -> bool {
    matches!(table, TableId::DeadBand | TableId::Socd)
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
        // The protocol crate knows the bytes (SOCD compares the power byte
        // of `24 02` and the pairs of `24 01`).
        WriteRequest::AdvancedKey(..) | WriteRequest::Socd(_) => request.observed_bytes(reply),
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
        // One expected value per verify read (SOCD compares different bytes
        // in its two reads).
        WriteRequest::AdvancedKey(..) | WriteRequest::Socd(_) => {
            return request
                .verify_reads()
                .into_iter()
                .map(|read| Some(request.expected_bytes(read)))
                .collect();
        }
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
    if let Some(import) = &options.import {
        if !available.contains(import.from) {
            return Err(RestoreError::ProfileNotInBackup {
                profile: import.from.get(),
            });
        }
        let into = ProfileSet::only(import.into);
        if options.profiles.is_some_and(|p| p != into) {
            return Err(RestoreError::InvalidRequest {
                reason: format!(
                    "an import writes only profile {}; no other profile can be chosen",
                    import.into
                ),
            });
        }
        return Ok((model, into));
    }
    if target.scope == BACKUP_SCOPE_EXPORT {
        // A shared file is untrusted input: only the import checks it.
        return Err(RestoreError::InvalidRequest {
            reason: "this file is an exported profile, not a backup: import it into a chosen \
                     profile instead (`openwraith profile import`, or Import in the app), which \
                     checks every value before anything is written"
                .to_string(),
        });
    }
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

/// Warnings about tables whose profile scope is unknown (dead band, SOCD):
/// written while the restore does not cover every profile, or backed up with
/// different copies while the keyboard holds one copy in every profile.
fn scope_warnings(target: &Backup, current: &Backup, profiles: &[ProfilePlan]) -> Vec<PlanWarning> {
    let mut warnings = Vec::new();
    let mut written: Vec<TableId> = Vec::new();
    for item in profiles.iter().flat_map(|p| p.items.iter()) {
        // An unchanged Fn keymap re-sent as the commit trigger writes the
        // bytes the keyboard holds: it changes no other profile.
        let changes = matches!(&item.action, PlanAction::Write(w) if !w.is_resent_unchanged());
        if changes && possibly_global(item.table) && !written.contains(&item.table) {
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
        (TableId::DeadBand, ReadRequest::DeadBand),
        (TableId::Socd, ReadRequest::SocdPairs),
    ];
    // Only the profiles read from the keyboard are compared: a backup copy
    // of a profile outside the restore (say profile 1, with its own
    // colours) says nothing about whether the keyboard shares the table.
    let read_profiles = current.profile_numbers();
    let copies = |backup: &Backup, read: ReadRequest| -> Vec<Vec<u8>> {
        backup
            .snapshots()
            .into_iter()
            .filter(|view| view.profile().is_some_and(|p| read_profiles.contains(&p)))
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
        commit_trigger: None,
        completes_binding: false,
        binding_tables: Vec::new(),
        request,
    })
}

/// A planned Fn-keymap write that is the keymap group's commit trigger
/// (sent even when the keyboard already holds it); without `changes` it
/// re-sends the Fn keymap unchanged.
fn trigger_action(
    trigger: CommitTrigger,
    changes: Vec<EntryChange>,
    reason: CommitTriggerReason,
) -> PlanAction {
    let request = trigger.request().clone();
    let summary = if changes.is_empty() {
        reason.resent_summary().to_string()
    } else {
        request.to_string()
    };
    PlanAction::Write(PlannedWrite {
        kind: request.kind(),
        summary,
        packets: request.steps().len(),
        changes,
        commit_trigger: Some(reason),
        completes_binding: false,
        binding_tables: Vec::new(),
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

/// The plan of one profile, in execution order. `import`: the checks of an
/// import (its warnings go to `warnings`).
fn plan_profile(
    model: &Model,
    t: SnapshotView<'_>,
    c: SnapshotView<'_>,
    import: Option<&ImportGuard>,
    warnings: &mut Vec<PlanWarning>,
) -> Vec<PlanItem> {
    let mut items = Vec::new();
    items.extend(plan_triggers(model, t, c));
    items.push(plan_dead_band(model, t, c));
    items.push(plan_socd(t, c));
    items.extend(plan_keymap_group(
        model,
        t,
        c,
        import.filter(|g| g.keymap),
        warnings,
    ));
    items.push(plan_colors1(model, t, c, import.is_some_and(|g| g.colors)));
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

/// SOCD power (`24 02` byte 0) and pairs (`24 01`), written as one burst
/// (W11). The other bytes of `24 02` (the hotkey) are never written.
fn plan_socd(t: SnapshotView<'_>, c: SnapshotView<'_>) -> PlanItem {
    let table = TableId::Socd;
    let target = (
        t.response(ReadRequest::SocdSettings),
        t.response(ReadRequest::SocdPairs),
    );
    let current = (
        c.response(ReadRequest::SocdSettings),
        c.response(ReadRequest::SocdPairs),
    );
    let ((Some(t_settings), Some(t_pairs)), (Some(c_settings), Some(c_pairs))) = (target, current)
    else {
        return match (target, current) {
            ((None, None), _) => item(table, None, PlanAction::NotInBackup),
            ((Some(_), Some(_)), _) => skip(
                table,
                None,
                SkipReason::NotInKeyboard,
                "the keyboard did not report its SOCD settings and pairs".to_string(),
            ),
            _ => skip(
                table,
                None,
                SkipReason::Unusable,
                "the backup holds only one of the SOCD replies (24 02, 24 01)".to_string(),
            ),
        };
    };
    let (t_payload, c_payload) = (t_settings.payload(), c_settings.payload());
    let (t_power, c_power) = (t_payload.first().copied(), c_payload.first().copied());
    let (t_table, c_table) = (content(t_pairs), content(c_pairs));
    if t_power == c_power && t_table == c_table {
        if t_payload.get(1..) == c_payload.get(1..) {
            return item(table, None, PlanAction::Unchanged);
        }
        return skip(
            table,
            None,
            SkipReason::ReadOnlyBytes,
            "only the SOCD bytes OpenWraith never writes differ (24 02 bytes 1-5, the hotkey)"
                .to_string(),
        );
    }
    let power = match t_power {
        Some(0) => false,
        Some(1) => true,
        other => {
            return skip(
                table,
                None,
                SkipReason::Unusable,
                format!("the backup's SOCD power byte is {other:02x?}, not 00 or 01"),
            );
        }
    };
    let pairs = match SocdPairsData::from_response(t_pairs) {
        Ok(pairs) => pairs,
        Err(WriteBuildError::UnwritableSocdPairs { reason }) => {
            return skip(
                table,
                None,
                SkipReason::UnwritableSocdPairs { rule: reason },
                format!("the backup's SOCD pairs cannot be written: {reason}"),
            );
        }
        Err(e) => return skip(table, None, SkipReason::Unusable, e.to_string()),
    };
    let mut changes = Vec::new();
    if t_power != c_power {
        changes.push(EntryChange {
            index: 0,
            old: HexBytes(c_power.into_iter().collect()),
            new: HexBytes(t_power.into_iter().collect()),
        });
    }
    changes.extend(
        entry_changes(&c_table, &t_table, 4)
            .into_iter()
            .map(|change| EntryChange {
                index: change.index + 1,
                ..change
            }),
    );
    item(
        table,
        None,
        write_action(WriteRequest::Socd(SocdWrite::new(power, pairs)), changes),
    )
}

/// Custom colour table 1. `merge` (an import): only the colours of the
/// model's keys come from the file; every other slot (the knob, 103, 126 and
/// all slots outside the layout) keeps the keyboard's bytes, as an editor
/// writes them ([`ColorTableData::set_key_colors`]).
fn plan_colors1(model: &Model, t: SnapshotView<'_>, c: SnapshotView<'_>, merge: bool) -> PlanItem {
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
    if merge {
        let merged = ColorTableData::from_response(current).and_then(|mut data| {
            let file = ColorTableData::from_response(target)?;
            let colors: Vec<(KeyPos, Rgb)> = model
                .positions()
                .filter(|pos| Some(pos.index()) != model.knob_index)
                .map(|pos| (pos, file.color(pos)))
                .collect();
            data.set_key_colors(model, &colors)?;
            Ok(data)
        });
        return match merged {
            Ok(data) if data.as_bytes()[..] == old[..] => item(table, None, PlanAction::Unchanged),
            Ok(data) => {
                let changes = entry_changes(&old, &data.as_bytes()[..], 3);
                item(
                    table,
                    None,
                    write_action(WriteRequest::CustomColors1(data), changes),
                )
            }
            Err(e) => skip(table, None, SkipReason::Unusable, e.to_string()),
        };
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
/// `import`: the layers are the keyboard's with the file's entries that pass
/// the import's checks ([`import_layer`]), and imported macros must be ones
/// OpenWraith can show as steps; the import's warnings go to `warnings`.
fn plan_keymap_group(
    model: &Model,
    t: SnapshotView<'_>,
    c: SnapshotView<'_>,
    import: Option<&ImportGuard>,
    warnings: &mut Vec<PlanWarning>,
) -> Vec<PlanItem> {
    let pair = |request| (t.response(request), c.response(request));

    let mut import_warnings: Vec<PlanWarning> = Vec::new();
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
                    } else if let Some(guard) = import {
                        match import_layer(model, guard, layer, target, current) {
                            Ok((None, notes)) => {
                                import_warnings.extend(notes);
                                GroupState::Unchanged
                            }
                            Ok((Some(write), notes)) => {
                                import_warnings.extend(notes);
                                GroupState::Differs(Ok(write))
                            }
                            Err(cause) => GroupState::Differs(Err(cause)),
                        }
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
                        // An import writes only macros OpenWraith can show
                        // as steps (key presses and releases with delays).
                        Ok((slot, MacroData::Content(content)))
                            if import.is_some() && MacroProgram::decode(&content).is_none() =>
                        {
                            GroupState::Differs(Err(BlockCause::NonCanonicalMacro {
                                slot: slot.get(),
                                reason: "an imported macro must be key presses and releases \
                                         that OpenWraith can show as steps",
                            }))
                        }
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
                        (Some(new), Some(old)) => {
                            let change = slot_change(usize::from(slot.get()), old, new);
                            GroupState::Differs(match AdvancedSlotData::from_read_slot(new) {
                                Ok(None) => Ok((WriteRequest::AdvancedKeyClear(slot), change)),
                                Ok(Some(data)) => {
                                    Ok((WriteRequest::AdvancedKey(slot, data), change))
                                }
                                Err(e) => Err(BlockCause::UnwritableAdvancedKey {
                                    slot: slot.get(),
                                    reason: e.to_string(),
                                }),
                            })
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
            // The layers as they are after the group: the written ones, and
            // the keyboard's where nothing changes.
            let after = |layer: Layer| -> Option<KeymapData> {
                let written = layers.iter().find_map(|(table, state)| match state {
                    GroupState::Differs(Ok((WriteRequest::Keymap(l, data), _)))
                        if *table == (TableId::Keymap { layer }) && *l == layer =>
                    {
                        Some(data.clone())
                    }
                    _ => None,
                });
                written.or_else(|| {
                    let raw = c
                        .response(ReadRequest::Keymap(layer))
                        .or_else(|| t.response(ReadRequest::Keymap(layer)))?;
                    KeymapData::from_response(raw).ok().map(|(_, data)| data)
                })
            };
            cause = Layer::ALL.into_iter().find_map(|layer| {
                let data = after(layer)?;
                data.references().into_iter().find_map(|r| match r {
                    KeymapReference::Malformed { index, .. } => {
                        Some(BlockCause::MalformedReference { layer, index })
                    }
                    _ => None,
                })
            });
        }
    }

    // The commit trigger (spec 4.8.2): the keyboard saves the base keymap
    // only when the Fn keymap is written after it, so the Fn keymap is
    // written whenever the base keymap is, even unchanged. Macro and
    // advanced-key slots may depend on the same trigger (untested): when
    // they are written and neither layer differs, the unchanged Fn keymap
    // is placed after the macro slots and before the advanced-key slots as
    // a precaution (GUESS; the site's order).
    let writes = |state: &GroupState| matches!(state, GroupState::Differs(Ok(_)));
    let layer_state = |wanted: Layer| {
        layers
            .iter()
            .find(|(table, _)| *table == TableId::Keymap { layer: wanted })
            .map(|(_, state)| state)
    };
    let base_state = layer_state(Layer::Base);
    let fn_state = layer_state(Layer::Fn);
    let base_written = base_state.is_some_and(writes);
    let slots_written = macros.iter().any(|(_, _, state)| writes(state))
        || advanced.iter().any(|(_, state)| writes(state));
    let reason = if base_written {
        Some(CommitTriggerReason::BaseKeymap)
    } else if slots_written {
        Some(CommitTriggerReason::MacrosOrAdvancedKeys)
    } else {
        None
    };
    // The Fn-keymap write that triggers the commit, with its changes.
    let trigger: Option<(CommitTrigger, Vec<EntryChange>, CommitTriggerReason)> =
        match (reason, fn_state) {
            (Some(reason), Some(GroupState::Differs(Ok((request, changes))))) => {
                CommitTrigger::try_from(request.clone())
                    .ok()
                    .map(|trigger| (trigger, changes.clone(), reason))
            }
            // Unchanged: the keyboard holds it (for an import, the file's
            // Fn layer may differ only where it is kept as read).
            (Some(reason), Some(GroupState::Unchanged)) => c
                .response(ReadRequest::Keymap(Layer::Fn))
                .and_then(|raw| CommitTrigger::try_from(write_request_of(raw)?).ok())
                .map(|trigger| (trigger, Vec::new(), reason)),
            _ => None,
        };
    if let (Some(reason), None, None) = (reason, &trigger, &cause) {
        // Never the base keymap (or the slots) without the trigger.
        cause = Some(BlockCause::FnKeymapNotWritable { reason });
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
    warnings.extend(import_warnings);

    // Execution order: the vendor site's (spec 4.8.2): every macro slot
    // (content and clears), the base keymap, the Fn keymap right after it
    // (the commit trigger, changed or not), the advanced-key slots.
    let base_table = TableId::Keymap { layer: Layer::Base };
    let fn_table = TableId::Keymap { layer: Layer::Fn };
    let fn_item = || match (&trigger, fn_state) {
        (Some((trigger, changes, reason)), _) => item(
            fn_table,
            None,
            trigger_action(trigger.clone(), changes.clone(), *reason),
        ),
        (None, Some(state)) => to_item(fn_table, state),
        (None, None) => item(fn_table, None, PlanAction::NotInBackup),
    };
    let mut items: Vec<PlanItem> = macros
        .iter()
        .map(|(_, table, state)| to_item(*table, state))
        .collect();
    if let Some(state) = base_state {
        items.push(to_item(base_table, state));
    }
    items.push(fn_item());
    items.extend(advanced.iter().map(|(table, state)| to_item(*table, state)));
    mark_binding_writes(&mut items, c);
    items
}

/// Flags the writes of a keymap group that complete a binding (see
/// [`PlannedWrite::completes_binding`]): an advanced-key slot written with
/// content that a written layer refers to, and a layer write that changes
/// an entry of the keyboard's layer referring to a macro slot written in
/// the same group, cleared or rewritten (the slot goes first, so a
/// cancellation must not leave the old key bound to the slot's new content
/// or to an empty slot).
fn mark_binding_writes(items: &mut [PlanItem], c: SnapshotView<'_>) {
    let written = |item: &PlanItem| match &item.action {
        PlanAction::Write(write) if !write.is_resent_unchanged() => Some(write.request.clone()),
        _ => None,
    };
    let requests: Vec<WriteRequest> = items.iter().filter_map(written).collect();
    // (slot, layer table) of every advanced-key reference of a written layer.
    let advanced_bound: Vec<(AdvancedSlotId, TableId)> = requests
        .iter()
        .filter_map(|request| match request {
            WriteRequest::Keymap(layer, data) => Some((*layer, data.references())),
            _ => None,
        })
        .flat_map(|(layer, references)| {
            references
                .into_iter()
                .filter_map(move |reference| match reference {
                    KeymapReference::Advanced { slot, .. } => {
                        Some((slot, TableId::Keymap { layer }))
                    }
                    _ => None,
                })
        })
        .collect();
    let written_macros: Vec<MacroSlotId> = requests
        .iter()
        .filter_map(|request| match request {
            WriteRequest::Macro(slot, _) => Some(*slot),
            _ => None,
        })
        .collect();
    let layer_now = |layer: Layer| -> Option<KeymapData> {
        c.response(ReadRequest::Keymap(layer))
            .and_then(|raw| KeymapData::from_response(raw).ok())
            .map(|(_, data)| data)
    };
    for item in items.iter_mut() {
        let PlanAction::Write(write) = &mut item.action else {
            continue;
        };
        let mut tables: Vec<TableId> = match &write.request {
            WriteRequest::AdvancedKey(slot, _) => advanced_bound
                .iter()
                .filter(|(bound, _)| bound == slot)
                .map(|(_, table)| *table)
                .collect(),
            WriteRequest::Keymap(layer, data) if !write.changes.is_empty() => {
                // Per entry: the key that refers to the written slot now
                // and changes in this write.
                layer_now(*layer)
                    .map(|now| {
                        now.references()
                            .into_iter()
                            .filter_map(|r| match r {
                                KeymapReference::Macro { index, slot }
                                    if written_macros.contains(&slot) =>
                                {
                                    let pos = KeyPos::from_index(index)?;
                                    (now.entry(pos) != data.entry(pos))
                                        .then_some(TableId::Macro { slot })
                                }
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };
        tables.dedup();
        write.completes_binding = !tables.is_empty();
        write.binding_tables = tables;
    }
}

/// The write request of a keymap reply (`None` when it is unusable).
fn write_request_of(raw: &RawResponse) -> Option<WriteRequest> {
    let (layer, data) = KeymapData::from_response(raw).ok()?;
    Some(WriteRequest::Keymap(layer, data))
}

/// The checks of an import (see the [module documentation](self)): the
/// file is untrusted input, so its keymap layers and colours are merged into
/// the keyboard's tables instead of being written as they are.
struct ImportGuard {
    /// The keymap group is imported: its layers are checked and merged.
    keymap: bool,
    /// The lighting is imported: custom colour table 1 is merged.
    colors: bool,
    /// The keyboard's Fn-locked keys (`0E`); the whole layout when unknown
    /// (no Fn-layer entry is imported then).
    fn_locked: KeyMask,
    /// What importing acknowledges: every non-critical built-in Fn
    /// function, the critical ones only when
    /// [`ImportOptions::allow_critical_fn_overrides`] is set.
    acks: Vec<BuiltinAck>,
}

impl ImportGuard {
    /// The checks of `import` against the keyboard's profile `c` (`primary`:
    /// the keyboard's primary snapshot, for the device constants).
    fn new(
        model: &Model,
        import: &ImportOptions,
        c: SnapshotView<'_>,
        primary: SnapshotView<'_>,
    ) -> Self {
        let fn_locked = c
            .fn_locked_keys()
            .or_else(|| primary.fn_locked_keys())
            .map_or_else(|| model.layout_mask(), |locked| locked.mask);
        let acks = model
            .fn_builtins
            .iter()
            .filter_map(|builtin| KeyPos::from_index(builtin.index))
            .map(|pos| BuiltinAck {
                pos,
                critical: import.allow_critical_fn_overrides,
            })
            .collect();
        ImportGuard {
            keymap: import.include.keymap_group,
            colors: import.include.lighting,
            fn_locked,
            acks,
        }
    }
}

/// A planned layer write and its changes.
type LayerWrite = (WriteRequest, Vec<EntryChange>);

/// The layer an import writes, and its warnings: the keyboard's layer
/// (`current`) with every entry of the file (`target`) that differs and
/// passes the checks.
///
/// * Positions that are not keys of the model, the knob and its slots
///   (15-21), the Fn key (121) and, on the Fn layer, keys the keyboard locks
///   (`0E`) keep the keyboard's bytes ([`PlanWarning::ImportKeptAsRead`]):
///   a file from a keyboard the vendor site configured still imports.
/// * Every other differing entry must be an assignment OpenWraith writes
///   ([`KeyAssignment::classify`]) that may go to that key
///   ([`check_placement`]; a built-in Fn function is replaced with a
///   warning, a critical one only when allowed). Otherwise the group is
///   blocked ([`BlockCause::UnwritableImportedEntry`],
///   [`BlockCause::CriticalFnOverride`]).
///
/// `None`: the merged layer equals the keyboard's.
fn import_layer(
    model: &Model,
    guard: &ImportGuard,
    layer: Layer,
    target: &RawResponse,
    current: &RawResponse,
) -> Result<(Option<LayerWrite>, Vec<PlanWarning>), BlockCause> {
    let table = TableId::Keymap { layer };
    let (Ok((_, file)), Ok((_, keyboard))) = (
        KeymapData::from_response(target),
        KeymapData::from_response(current),
    ) else {
        return Err(BlockCause::Incomplete { table });
    };
    let mut merged = keyboard.clone();
    let mut kept: Vec<u8> = Vec::new();
    let mut notes: Vec<PlanWarning> = Vec::new();
    for pos in KeyPos::all() {
        let (old, new) = (keyboard.entry(pos), file.entry(pos));
        if old == new {
            continue;
        }
        let index = pos.index();
        // Not an assignable key here: kept as read, never blocked.
        let position = check_placement(
            model,
            layer,
            pos,
            &KeyAssignment::Default,
            &guard.fn_locked,
            &[],
        );
        if position.is_err() {
            kept.push(index);
            continue;
        }
        let unwritable = || BlockCause::UnwritableImportedEntry {
            layer,
            index,
            entry: HexBytes::from(&new[..]),
        };
        let EntryKind::Assignable(assignment) = KeyAssignment::classify(model, pos, new) else {
            return Err(unwritable());
        };
        match check_placement(
            model,
            layer,
            pos,
            &assignment,
            &guard.fn_locked,
            &guard.acks,
        ) {
            Ok(Some(builtin)) => notes.push(PlanWarning::FnBuiltinOverridden {
                index: builtin.index,
                critical: builtin.critical,
            }),
            Ok(None) => {}
            Err(PlacementError::BuiltinNotAcknowledged {
                index,
                critical: true,
                ..
            }) => return Err(BlockCause::CriticalFnOverride { index }),
            Err(_) => return Err(unwritable()),
        }
        if let Some(note) = assignment.evidence(layer).note() {
            notes.push(PlanWarning::Unverified { layer, index, note });
        }
        merged.set_entry(pos, new);
    }
    if !kept.is_empty() {
        notes.insert(
            0,
            PlanWarning::ImportKeptAsRead {
                layer,
                indices: kept,
            },
        );
    }
    if merged == keyboard {
        return Ok((None, notes));
    }
    let changes = entry_changes(&keyboard.as_bytes()[..], &merged.as_bytes()[..], 4);
    Ok((Some((WriteRequest::Keymap(layer, merged), changes)), notes))
}

/// Applies an import's selection to the plan of profile `into`: the writes
/// and skips of excluded categories become [`SkipReason::NotImported`].
/// Returns the import's warnings (the macros it writes).
fn apply_import(import: &ImportOptions, items: &mut [PlanItem]) -> Vec<PlanWarning> {
    let mut warnings = Vec::new();
    for item in items.iter_mut() {
        if !matches!(item.action, PlanAction::Write(_) | PlanAction::Skip { .. }) {
            continue;
        }
        if !import.include.includes(item.table) {
            item.action = PlanAction::Skip {
                reason: SkipReason::NotImported,
                message: format!("{} is not imported", item.table),
            };
        }
    }
    let slots: Vec<u8> = items
        .iter()
        .filter_map(|i| match &i.action {
            PlanAction::Write(PlannedWrite {
                request: WriteRequest::Macro(slot, data),
                ..
            }) if !data.is_empty() => Some(slot.get()),
            _ => None,
        })
        .collect();
    if !slots.is_empty() {
        warnings.push(PlanWarning::ImportedMacros { slots });
    }
    warnings
}

/// Human-readable form of a [`BlockCause`].
fn block_message(cause: &BlockCause) -> String {
    match cause {
        BlockCause::UnwritableAdvancedKey { slot, reason } => format!(
            "advanced-key slot {slot} of the backup is not a DKS, Mod-Tap or Toggle OpenWraith \
             can write ({reason})"
        ),
        BlockCause::CriticalFnOverride { index } => format!(
            "the imported Fn layer replaces the built-in function of Fn + key #{index} (a \
             profile switch or the reset), which was not allowed"
        ),
        BlockCause::UnwritableImportedEntry {
            layer,
            index,
            entry,
        } => format!(
            "the file's {} keymap entry of key #{index} ({}) is not one OpenWraith writes \
             (only keys, media keys, mouse buttons, key combinations and macro or advanced-key \
             bindings are imported)",
            match layer {
                Layer::Base => "base",
                Layer::Fn => "Fn",
            },
            entry.to_spaced_hex()
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
        BlockCause::FnKeymapNotWritable { reason } => format!(
            "the backup's Fn keymap cannot be written (unusable reply), and {}",
            match reason {
                CommitTriggerReason::BaseKeymap =>
                    "the keyboard keeps the base keymap only when the Fn keymap (18 02) is \
                     written after it, so the base keymap is not written alone",
                CommitTriggerReason::MacrosOrAdvancedKeys =>
                    "the macro and advanced-key slots are only written followed by the Fn \
                     keymap (18 02), as the vendor site does (precaution)",
            }
        ),
    }
}

#[cfg(all(test, feature = "mock"))]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;
    use crate::backup::{ToolInfo, read_all_with};
    use crate::mock::MockTransport;
    use crate::sim::{DeviceSim, SimTable};
    use crate::writer::ConsentOrigin;

    /// The c06 capture replayed into a backup (zero keymaps).
    fn c06() -> Backup {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../captures/capture_06_powercycle_W_persisted.json");
        let mock = MockTransport::from_capture_file(path).expect("capture loads");
        read_all_with(&mut Session::new(mock), &options().backup, |_| {}).expect("backup")
    }

    fn options() -> RestoreOptions {
        RestoreOptions {
            backup: BackupOptions {
                tool: ToolInfo {
                    name: "openwraith-tests".into(),
                    version: "0".into(),
                },
                created_at: Some(UNIX_EPOCH + Duration::from_secs(1_792_827_000)),
            },
            ..RestoreOptions::default()
        }
    }

    /// The order before the fix of 2026-10-24 (spec 4.8.2): the base keymap
    /// without the Fn keymap after it, reproduced by taking the commit
    /// trigger out of a prepared plan. The keyboard (simulator) does not
    /// keep the base keymap, and the persistence check reports it lost.
    #[test]
    fn the_base_keymap_alone_is_reported_lost_by_the_persistence_check() {
        let target = c06();
        let mut sim = DeviceSim::from_backup(&target);
        let p1 = ProfileId::ALL[0];
        let base = SimTable::Keymap(Layer::Base);
        let mut remapped = vec![0u8; 528];
        remapped[46 * 4..47 * 4].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
        sim.set_table(p1, base, remapped.clone());
        let consent = WriteConsent::new(ConsentOrigin::CommandLineFlag);
        let mut prepared = prepare_restore(
            &mut Session::new(&mut sim),
            &target,
            &options(),
            consent,
            |_| ControlFlow::Continue(()),
        )
        .expect("prepared");
        // The plan of the fix: the base keymap, then the unchanged Fn keymap.
        let planned: Vec<(TableId, Option<CommitTriggerReason>)> = prepared
            .plan
            .writes()
            .map(|(_, item, write)| (item.table, write.commit_trigger))
            .collect();
        assert_eq!(
            planned,
            [
                (TableId::Keymap { layer: Layer::Base }, None),
                (
                    TableId::Keymap { layer: Layer::Fn },
                    Some(CommitTriggerReason::BaseKeymap)
                ),
            ]
        );
        // The old order: without it.
        for profile in &mut prepared.plan.profiles {
            profile.items.retain(
                |item| !matches!(&item.action, PlanAction::Write(w) if w.commit_trigger.is_some()),
            );
        }
        let dir = tempfile::tempdir().expect("temporary directory");
        let saved = prepared
            .safety
            .save_safety(dir.path(), "restore")
            .expect("safety backup");
        let first = sim.writes_sent();
        let report = execute_restore(
            &mut Session::new(&mut sim),
            &prepared,
            &saved,
            consent,
            |_| ControlFlow::Continue(()),
        );
        // Written (ten 18 00 packets, no 18 02), verified, given the commit
        // settle, and gone after the switch away and back.
        let keymap: Vec<(u8, u8)> = sim
            .write_packets()
            .skip(first)
            .filter(|p| p.cmd() == 0x18)
            .map(|p| (p.sub(), p.as_bytes()[3]))
            .collect();
        assert_eq!(keymap, (0..10).map(|c| (0x00, c)).collect::<Vec<_>>());
        assert_eq!(report.outcome, Outcome::Failed);
        assert_eq!(
            report.error.as_ref().map(|e| e.kind),
            Some(ErrorKind::NotPersisted)
        );
        assert_eq!(
            report.persistence,
            Some(PersistenceCheck {
                profiles: vec![1],
                persisted: 0,
                lost: 1,
                not_checked: 0
            })
        );
        assert!(matches!(
            report.items().next().map(|(_, i)| &i.result),
            Some(ItemResult::Lost { packets: 10, .. })
        ));
        assert_eq!(sim.persisted_table(p1, base), Some(remapped.as_slice()));
        assert!(sim.discarded_edits() >= 1);
        sim.assert_clean();
    }
}
