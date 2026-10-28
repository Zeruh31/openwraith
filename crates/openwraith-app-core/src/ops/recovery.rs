//! The known-good store in use: automatic backups, reset detection and the
//! Recovery card (design B6, brief 3.1-3.6).
//!
//! **Promotion.** Every complete read of the active profile with no other
//! program heard ([`Worker::observe_read`]) is compared with the newest
//! automatic backup of that profile of that keyboard (`assess`):
//!
//! * no automatic backup yet: the read becomes the first one (B6 (d));
//! * not reset-like: a read that differs is saved as a new automatic backup
//!   at once (B6 (b)); after a write of OpenWraith that completed with
//!   nothing lost or maybe lost, the tables it wrote are its own changes and
//!   the next complete read (the read after the operation, or the next one
//!   for edits that re-read only what they changed) is saved (B6 (a)); a
//!   write that may not have been saved waits for a read on a later
//!   connection;
//! * a table OpenWraith wrote is its own change **only while it holds what
//!   the write left in it**: a clean write records the SHA-256 of the
//!   table's canonical form (`KnownGoodTable::canonical`) from its own
//!   re-read patched into the complete read made right before it (edits) or
//!   from the backup it wrote (restores, imports). A reset or another
//!   program that changed the table since is evidence again. A table whose
//!   content is not known (a write running or maybe lost, an edit whose
//!   re-read did not happen) counts as the write's only in this run and
//!   until another program is heard;
//! * reset-like (R1 the keyboard's reset notification, R2 another firmware,
//!   R3 triggers or two keymap-group tables back to the factory form, not by
//!   OpenWraith's own writes): nothing is saved, the read is held with its
//!   factory tables until "Restore" or "It's fine, keep them" (B6 (c)), and
//!   `Notice::ResetSuspected` is sent once per profile and backup. Only a
//!   restore that completed, or an import that completed and wrote every
//!   table held at its factory form, releases a held profile: a restore
//!   that may not have been saved, a partial import or an edit explain
//!   nothing about it. Such a write accepts the firmware the keyboard had
//!   when it started (R2 against that firmware is no evidence, and the
//!   next read is saved); R2 against any other firmware still is.
//! * R1 counts once per profile, at its first comparison after the report,
//!   for the rest of the run (restoring or keeping one profile does not
//!   disarm it for the profiles not read yet); the report's banner goes
//!   once every profile with automatic backups was compared and nothing is
//!   held (at the comparison that completes it, or with "It's fine, keep
//!   them"). At the first comparison after the report, and at any while the
//!   report stands, a read whose keyboard-wide SOCD power is back at its
//!   reset form is not saved (the backup from before the reset keeps the
//!   power "Restore all profiles" writes).
//! * The first comparison after the command line tool wrote under its
//!   exclusive lock (same connection, no reset heard) takes factory forms as
//!   the user's own change (R1 and R2 still count).
//! * The W75 reports no USB serial number, so every W75 shares one store
//!   folder (`autobackup`): a keyboard on another connection (another HID
//!   path) starts this run's comparisons afresh, the reset report aside.
//! * OpenWraith's own writes not yet saved as an automatic backup are kept
//!   on disk too (`<keyboard>/own-writes.json` in the store, with what each
//!   table held after the write; edits re-read only what they wrote and are
//!   saved at the next complete read): a run that ends first (the app
//!   closed) does not take its own edit for a reset at the next start, and
//!   a reset while the app was closed is still found (the table no longer
//!   holds what was written). Tables of unknown content are not loaded.
//!
//! Nothing here writes to the keyboard. A recovery is the standard restore:
//! [`Worker::prepare_recovery`] loads the automatic backups (composing them
//! for "Restore all profiles"), and `prepare_restore` compares them with the
//! keyboard (`recovery_check`); the plan is written only by `apply_restore`,
//! with its review, safety backup, journal and Undo.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use openwraith_hid::protocol::{ProfileId, RawResponse};
use openwraith_hid::{
    Backup, FirmwareId, KnownGoodTable, Outcome, ProfileAssessment, ResetEvidence, SnapshotView,
    TableVerdict, assess,
};

use crate::autobackup::{self, AutoReason, StoreFile};
use crate::backend::DeviceBackend;
use crate::config::CoreConfig;
use crate::dto::{
    AppError, AppErrorKind, AutoBackupEntry, AutoBackupReason, AutoBackupsView, EditProblemView,
    FileToken, FirmwareChangeView, FirmwareView, KnownGoodTableView, Notice, OpKind,
    RecoveryIntent, RecoveryProfileState, RecoveryProfileView, RecoveryStatusView, RecoveryView,
    ResetEvidenceView, RestoreIntent, RestorePreview,
};
use crate::files::FileKind;
use crate::map::clean;
use crate::map::editors::refused;
use crate::shared::Shared;
use crate::sink::ProgressSink;
use crate::worker::Worker;

/// Writes of OpenWraith to a profile since its last automatic backup: a
/// table they wrote is its own change, not evidence of a reset, **only while
/// it holds what the write left in it**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnWrite {
    /// The tables written, each with what the write left in it when that is
    /// known: the SHA-256 of the table's canonical form
    /// (`KnownGoodTable::canonical`), from the write's own re-read (edits)
    /// or the backup it restored (restores, imports). `None` while the write
    /// runs, when it may not have been saved, or when nothing re-read it:
    /// such a table counts as the write's whatever it holds, but only in
    /// this run and until another program is heard (it is never loaded
    /// from `own-writes.json`).
    pub(crate) tables: BTreeMap<KnownGoodTable, Option<String>>,
    /// A write is running (its outcome is not known yet).
    pub(crate) running: bool,
    /// A write may not have been saved (lost, maybe lost, failed,
    /// cancelled part-way): no automatic backup on this connection.
    pub(crate) uncertain: bool,
    /// The connection of the last write.
    pub(crate) generation: u32,
    /// A restore of the whole profile.
    pub(crate) restore: bool,
    /// A write that completed and resolved the profile's hold (a restore,
    /// or an import of every table held at its factory form).
    pub(crate) resolved: bool,
    /// The keyboard's firmware when the write started (the last complete
    /// read's). A completed restore (or a write that resolved the hold)
    /// made on it accepts that firmware: R2 against it is no evidence. R2
    /// against any other firmware still is.
    pub(crate) firmware: Option<FirmwareView>,
}

impl OwnWrite {
    /// The tables the write explains in `read`: the ones that hold what it
    /// left in them (or, content unknown, any table it wrote in this run).
    fn explains(&self, read: SnapshotView<'_>) -> BTreeSet<KnownGoodTable> {
        self.tables
            .iter()
            .filter(|(table, expected)| match expected {
                Some(digest) => table_digest(**table, read).as_deref() == Some(digest.as_str()),
                None => true,
            })
            .map(|(table, _)| *table)
            .collect()
    }

    /// Whether R2 against `current` is the firmware this write accepted.
    fn accepts_firmware(&self, current: &FirmwareView) -> bool {
        (self.restore || self.resolved)
            && !self.uncertain
            && self.firmware.as_ref() == Some(current)
    }
}

/// What `table` holds in `view`, as [`OwnWrite::tables`] keeps it.
fn table_digest(table: KnownGoodTable, view: SnapshotView<'_>) -> Option<String> {
    table
        .canonical(view)
        .map(|form| crate::sha256::sha256_hex(&form))
}

/// The last comparison of a profile.
#[derive(Debug, Clone)]
pub(crate) struct ProfileCheck {
    /// When the profile was read (`created_at` of the read).
    pub(crate) checked_at: String,
    /// When the automatic backup it was compared with was read.
    pub(crate) known_good_at: Option<String>,
    /// How old that backup was then, in seconds.
    pub(crate) known_good_age_s: Option<u32>,
    /// Why it looks reset (empty: fine).
    pub(crate) evidence: Vec<ResetEvidenceView>,
    /// Tables in their factory form now (not by OpenWraith's writes).
    pub(crate) factory_now: Vec<KnownGoodTable>,
    /// Tables changed otherwise.
    pub(crate) changed: Vec<KnownGoodTable>,
    /// The read held while the profile looks reset (never saved as an
    /// automatic backup until the user keeps it).
    pub(crate) held: Option<Arc<Backup>>,
}

/// The keyboard of the last complete read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoreDevice {
    /// Its folder in the store.
    pub(crate) key: String,
    /// Its product code.
    pub(crate) product: Option<String>,
    /// Its firmware.
    pub(crate) firmware: FirmwareView,
}

/// The known-good state of this run (in [`Shared`], behind one lock that
/// also serializes every change of the store's files).
#[derive(Debug, Default)]
pub(crate) struct KnownGoodState {
    /// The keyboard of the last complete read.
    pub(crate) device: Option<StoreDevice>,
    /// R1: the keyboard reported a reset during this run (when), not yet
    /// resolved by a restore or "It's fine, keep them" (the banner).
    pub(crate) reset_reported: Option<String>,
    /// R1 counts at the first comparison of each profile not yet compared
    /// since the report (stays armed for this keyboard when the banner is
    /// resolved).
    pub(crate) r1_armed: bool,
    /// Profiles compared since the reset report (R1 counts for the first
    /// comparison of each profile after it).
    pub(crate) reset_applied: BTreeSet<u8>,
    /// The connection (HID path) of the last complete read.
    pub(crate) device_id: Option<String>,
    /// The last comparison of each profile.
    pub(crate) profiles: BTreeMap<u8, ProfileCheck>,
    /// OpenWraith's writes per profile since its last automatic backup.
    pub(crate) own: BTreeMap<u8, OwnWrite>,
    /// `ResetSuspected` sent: (profile, backup file).
    notified: BTreeSet<(u8, String)>,
    /// `FirmwareChanged` sent: (known, current).
    firmware_notified: BTreeSet<(String, String)>,
    /// The store folder whose `own-writes.json` was loaded this run.
    own_loaded: Option<String>,
}

/// The file of the own writes of a keyboard (in its store folder).
const OWN_FILE: &str = "own-writes.json";

/// An own write as `own-writes.json` keeps it.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct OwnRecord {
    /// Table -> SHA-256 of what the write left in it (`null`: unknown, not
    /// loaded again).
    tables: BTreeMap<String, Option<String>>,
    uncertain: bool,
    restore: bool,
    #[serde(default)]
    resolved: bool,
    /// The keyboard's firmware when the write started.
    #[serde(default)]
    firmware: Option<FirmwareRecord>,
}

/// A firmware as `own-writes.json` keeps it.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct FirmwareRecord {
    version: Option<String>,
    build_date: Option<String>,
}

/// `own-writes.json`: profile -> own write.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct OwnFile {
    #[serde(default)]
    profiles: BTreeMap<u8, OwnRecord>,
}

fn table_slug(table: KnownGoodTable) -> &'static str {
    match table {
        KnownGoodTable::Triggers => "triggers",
        KnownGoodTable::MainLight => "main_light",
        KnownGoodTable::CustomColors1 => "custom_colors_1",
        KnownGoodTable::BaseKeymap => "base_keymap",
        KnownGoodTable::FnKeymap => "fn_keymap",
        KnownGoodTable::Macros => "macros",
        KnownGoodTable::AdvancedKeys => "advanced_keys",
        KnownGoodTable::DeadBand => "dead_band",
        KnownGoodTable::SocdPairs => "socd_pairs",
    }
}

impl KnownGoodState {
    /// Writes the own writes of the keyboard read last (best effort: a
    /// file that cannot be written only loses this protection across
    /// runs).
    fn save_own(&self, root: &Path) {
        let Some(device) = &self.device else {
            return;
        };
        let path = autobackup::device_dir(root, &device.key).join(OWN_FILE);
        if self.own.is_empty() {
            let _ = std::fs::remove_file(&path);
            return;
        }
        let file = OwnFile {
            profiles: self
                .own
                .iter()
                .map(|(p, o)| {
                    (
                        *p,
                        OwnRecord {
                            tables: o
                                .tables
                                .iter()
                                .map(|(t, digest)| (table_slug(*t).to_string(), digest.clone()))
                                .collect(),
                            // A write still running when the app ends may
                            // not have been saved.
                            uncertain: o.uncertain || o.running,
                            restore: o.restore,
                            resolved: o.resolved,
                            firmware: o.firmware.as_ref().map(|f| FirmwareRecord {
                                version: f.version.clone(),
                                build_date: f.build_date.clone(),
                            }),
                        },
                    )
                })
                .collect(),
        };
        let Ok(json) = serde_json::to_string_pretty(&file) else {
            return;
        };
        let dir = autobackup::device_dir(root, &device.key);
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let Ok(mut temp) = tempfile::Builder::new()
            .prefix(".own-writes-")
            .suffix(".partial")
            .tempfile_in(&dir)
        else {
            return;
        };
        use std::io::Write as _;
        if temp.write_all(json.as_bytes()).is_ok() {
            let _ = temp.persist(&path);
        }
    }

    /// Loads the own writes an earlier run kept for `key` (once per run and
    /// keyboard; this run's own records win). They count as made on
    /// another connection, and only their tables with a known content are
    /// loaded (a table whose content is unknown explains nothing in a later
    /// run: the keyboard may have been reset while the app was closed).
    fn load_own(&mut self, root: &Path, key: &str) {
        if self.own_loaded.as_deref() == Some(key) {
            return;
        }
        self.own_loaded = Some(key.to_string());
        let path = autobackup::device_dir(root, key).join(OWN_FILE);
        let Some(file) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<OwnFile>(&text).ok())
        else {
            return;
        };
        for (p, record) in file.profiles {
            if !(1..=4).contains(&p) || self.own.contains_key(&p) {
                continue;
            }
            let tables = record
                .tables
                .iter()
                .filter_map(|(slug, digest)| {
                    let table = KnownGoodTable::ALL
                        .into_iter()
                        .find(|t| table_slug(*t) == slug)?;
                    let digest = digest.as_ref().filter(|d| !d.is_empty())?;
                    Some((table, Some(digest.clone())))
                })
                .collect();
            self.own.insert(
                p,
                OwnWrite {
                    tables,
                    running: false,
                    uncertain: record.uncertain,
                    generation: u32::MAX,
                    restore: record.restore,
                    resolved: record.resolved,
                    firmware: record.firmware.map(|f| FirmwareView {
                        version: f.version,
                        build_date: f.build_date,
                    }),
                },
            );
        }
    }
}

impl KnownGoodState {
    /// What the status shows.
    pub(crate) fn status(&self) -> RecoveryStatusView {
        RecoveryStatusView {
            reset_suspected: self
                .profiles
                .iter()
                .filter(|(_, c)| c.held.is_some())
                .map(|(p, _)| *p)
                .collect(),
            reset_reported: self.reset_reported.is_some(),
            firmware_changed: self.firmware_change().is_some(),
        }
    }

    /// The firmware change a profile's comparison found, if any.
    fn firmware_change(&self) -> Option<FirmwareChangeView> {
        self.profiles.values().find_map(|c| {
            c.evidence.iter().find_map(|e| match e {
                ResetEvidenceView::FirmwareChanged { known, current } => Some(FirmwareChangeView {
                    known: known.clone(),
                    current: current.clone(),
                }),
                _ => None,
            })
        })
    }

    /// Switches to the keyboard of a read: another keyboard starts afresh
    /// (its reset report and own writes do not carry over). The same store
    /// folder on another connection (the W75 has no serial number: maybe
    /// another W75) starts its comparisons afresh too, but keeps the reset
    /// report (fail closed: R1 counts again for each profile).
    fn set_device(&mut self, device: StoreDevice, device_id: Option<&str>) {
        if self.device.as_ref().is_some_and(|d| d.key != device.key) {
            // (The other keyboard's own writes stay in its file.)
            self.own_loaded = None;
            self.profiles.clear();
            self.own.clear();
            self.reset_applied.clear();
            self.reset_reported = None;
            self.r1_armed = false;
        } else if let (Some(before), Some(now)) = (self.device_id.as_deref(), device_id)
            && before != now
        {
            self.profiles.clear();
            self.own.clear();
            self.reset_applied.clear();
        }
        self.device = Some(device);
        if let Some(id) = device_id {
            self.device_id = Some(id.to_string());
        }
    }

    /// Whether a profile with automatic backups was not compared since the
    /// reset report.
    fn unchecked_since_reset(&self, root: &Path) -> bool {
        let Some(device) = &self.device else {
            return false;
        };
        (1..=4u8).any(|p| {
            !self.reset_applied.contains(&p)
                && autobackup::list_profile(&autobackup::profile_dir(root, &device.key, p))
                    .is_ok_and(|files| !files.is_empty())
        })
    }

    /// The reset report's banner goes once nothing is held and every
    /// profile with automatic backups was compared since it.
    fn resolve_report_if_done(&mut self, root: &Path) {
        if self.reset_reported.is_some()
            && !self.profiles.values().any(|c| c.held.is_some())
            && !self.unchecked_since_reset(root)
        {
            self.reset_reported = None;
        }
    }

    /// Another program was heard: it may have changed any table, so the
    /// own writes whose content is unknown explain nothing any more (the
    /// ones with a known content are still checked against it).
    fn forget_unknown_own_writes(&mut self, root: &Path) {
        let mut changed = false;
        for own in self.own.values_mut() {
            let before = own.tables.len();
            own.tables.retain(|_, digest| digest.is_some());
            changed |= own.tables.len() != before;
        }
        if changed {
            self.save_own(root);
        }
    }
}

/// The tables an import of `include` writes (or leaves as the file has
/// them).
pub(crate) fn import_tables(include: openwraith_hid::ImportSelection) -> BTreeSet<KnownGoodTable> {
    use KnownGoodTable as T;
    let mut tables = BTreeSet::new();
    if include.triggers {
        tables.insert(T::Triggers);
    }
    if include.lighting {
        tables.extend([T::MainLight, T::CustomColors1]);
    }
    if include.keymap_group {
        tables.extend([T::BaseKeymap, T::FnKeymap, T::Macros, T::AdvancedKeys]);
    }
    if include.dead_band {
        tables.insert(T::DeadBand);
    }
    if include.socd {
        tables.insert(T::SocdPairs);
    }
    tables
}

/// The tables an operation writes (restores and imports: every table; an
/// import passes its own with [`import_tables`]) and whether it is a
/// restore of the whole profile.
pub(crate) fn tables_of(kind: OpKind) -> (BTreeSet<KnownGoodTable>, bool) {
    use KnownGoodTable as T;
    let tables: &[KnownGoodTable] = match kind {
        OpKind::TriggerEdit => &[T::Triggers],
        OpKind::LightEdit => &[T::MainLight],
        OpKind::ColorEdit => &[T::CustomColors1, T::MainLight],
        OpKind::DeadBandEdit => &[T::DeadBand],
        OpKind::SocdEdit => &[T::SocdPairs],
        OpKind::KeymapEdit => &[T::BaseKeymap, T::FnKeymap, T::Macros, T::AdvancedKeys],
        _ => &KnownGoodTable::ALL,
    };
    let restore = matches!(kind, OpKind::Restore);
    (tables.iter().copied().collect(), restore)
}

/// Whether a write completed with nothing lost and nothing maybe lost.
pub(crate) fn completed_cleanly(outcome: Outcome, lost: usize, maybe_lost: usize) -> bool {
    matches!(outcome, Outcome::Completed | Outcome::NothingToDo) && lost == 0 && maybe_lost == 0
}

// ----- mapping -----

pub(crate) fn table_view(table: KnownGoodTable) -> KnownGoodTableView {
    match table {
        KnownGoodTable::Triggers => KnownGoodTableView::Triggers,
        KnownGoodTable::MainLight => KnownGoodTableView::MainLight,
        KnownGoodTable::CustomColors1 => KnownGoodTableView::CustomColors1,
        KnownGoodTable::BaseKeymap => KnownGoodTableView::BaseKeymap,
        KnownGoodTable::FnKeymap => KnownGoodTableView::FnKeymap,
        KnownGoodTable::Macros => KnownGoodTableView::Macros,
        KnownGoodTable::AdvancedKeys => KnownGoodTableView::AdvancedKeys,
        KnownGoodTable::DeadBand => KnownGoodTableView::DeadBand,
        KnownGoodTable::SocdPairs => KnownGoodTableView::SocdPairs,
    }
}

fn firmware_view(id: &FirmwareId) -> FirmwareView {
    FirmwareView {
        version: id.version.as_deref().map(clean),
        build_date: id.build_date.as_deref().map(clean),
    }
}

fn firmware_of(backup: &Backup) -> FirmwareView {
    FirmwareView {
        version: backup.device.firmware.as_deref().map(clean),
        build_date: backup.device.build_date.as_deref().map(clean),
    }
}

/// The evidence of an assessment without what OpenWraith's own writes
/// explain.
fn evidence_view(
    evidence: &[ResetEvidence],
    own: &BTreeSet<KnownGoodTable>,
) -> Vec<ResetEvidenceView> {
    evidence
        .iter()
        .filter_map(|e| match e {
            ResetEvidence::FirmwareChanged { known, current } => {
                Some(ResetEvidenceView::FirmwareChanged {
                    known: firmware_view(known),
                    current: firmware_view(current),
                })
            }
            ResetEvidence::TriggersToFactory => (!own.contains(&KnownGoodTable::Triggers))
                .then_some(ResetEvidenceView::TriggersToFactory),
            ResetEvidence::KeymapGroupToFactory { tables } => {
                let tables: Vec<KnownGoodTableView> = tables
                    .iter()
                    .filter(|t| !own.contains(t))
                    .map(|t| table_view(*t))
                    .collect();
                (tables.len() >= 2).then_some(ResetEvidenceView::KeymapGroupToFactory { tables })
            }
        })
        .collect()
}

/// Seconds since 1970 of an RFC 3339 UTC time (`2026-10-24T07:30:00Z`).
fn epoch_seconds(time: &str) -> Option<i64> {
    let digits: Vec<i64> = time
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .take(6)
        .map(|s| s.parse().ok())
        .collect::<Option<_>>()?;
    let [y, m, d, hh, mm, ss] = digits.as_slice() else {
        return None;
    };
    // Days from the civil date (H. Hinnant's algorithm).
    let y = if *m <= 2 { y - 1 } else { *y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// How much older `known` is than `current`, in seconds.
fn age_s(known: &str, current: &str) -> Option<u32> {
    let delta = epoch_seconds(current)? - epoch_seconds(known)?;
    u32::try_from(delta.max(0)).ok()
}

fn reason_view(reason: AutoReason) -> AutoBackupReason {
    match reason {
        AutoReason::FirstRead => AutoBackupReason::FirstRead,
        AutoReason::AfterWrite => AutoBackupReason::AfterWrite,
        AutoReason::ChangedElsewhere => AutoBackupReason::ChangedElsewhere,
        AutoReason::KeptCurrent => AutoBackupReason::KeptCurrent,
        AutoReason::Manual => AutoBackupReason::Manual,
    }
}

/// "Back up now" (the tray): `read`, a complete read of the active profile,
/// as an automatic backup of it. The newest automatic backup is the answer
/// when it already holds the same settings (the read's own comparison may
/// just have saved it). A profile held after a suspected reset is never
/// backed up this way (`None`: review it in the Recovery card first).
pub(crate) fn save_now(
    shared: &Shared,
    config: &CoreConfig,
    read: &Backup,
) -> Result<Option<AutoBackupEntry>, AppError> {
    let profile = read
        .active_profile
        .and_then(ProfileId::new)
        .ok_or_else(|| AppError::new(AppErrorKind::InvalidBackup, "the read has no profile"))?;
    let p = profile.get();
    // The lock serializes every change of the store's files.
    let st = shared.known_good();
    if st.profiles.get(&p).is_some_and(|c| c.held.is_some()) {
        return Ok(None);
    }
    let keep = crate::settings::auto_backup(config.settings_path.as_deref()).keep;
    let key = autobackup::device_key_of(read);
    let dir = autobackup::profile_dir(&config.known_good_dir(), &key, p);
    let same = autobackup::baseline(&dir)
        .filter(|(_, known)| assess(known, read, profile).is_ok_and(|a| a.is_unchanged()));
    let path = match same {
        Some((file, _)) => file.path,
        None => autobackup::save(&dir, read, AutoReason::Manual, keep)?.0,
    };
    let file = autobackup::list_profile(&dir)?
        .into_iter()
        .find(|f| f.path == path);
    drop(st);
    Ok(file.and_then(|f| entry(shared, f, p)))
}

/// A listed automatic backup as an entry (the file is registered).
fn entry(shared: &Shared, file: StoreFile, profile: u8) -> Option<AutoBackupEntry> {
    let reason = file.name.reason()?;
    Some(AutoBackupEntry {
        file: shared.register_file(file.path, FileKind::Auto),
        profile,
        product: file.name.product.as_deref().map(clean),
        created_at: file.name.time(),
        reason: reason_view(reason),
        pinned: file.pinned,
    })
}

// ----- observing reads -----

/// What the comparison of one read decided.
#[derive(Debug, Default)]
pub(crate) struct Observed {
    /// Notices to send (after the lock is released).
    pub(crate) notices: Vec<Notice>,
    /// The automatic backup saved, if any.
    pub(crate) saved: Option<PathBuf>,
}

/// Where a read came from, for [`observe`].
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ReadOrigin<'a> {
    /// The connection (HID path) it was read over.
    pub(crate) device_id: Option<&'a str>,
    /// The first read after the command line tool wrote under its exclusive
    /// lock on this connection (factory forms are its doing).
    pub(crate) after_cli_write: bool,
}

/// Compares a complete read of `profile` (no other program heard) with its
/// newest automatic backup and promotes or holds it (see the [module
/// documentation](self)). Writes only the store's files.
pub(crate) fn observe(
    shared: &Shared,
    root: &Path,
    read: &Backup,
    profile: ProfileId,
    generation: u32,
    keep: u8,
    origin: ReadOrigin<'_>,
) -> Observed {
    let mut out = Observed::default();
    let p = profile.get();
    let key = autobackup::device_key_of(read);
    let mut st = shared.known_good();
    st.set_device(
        StoreDevice {
            key: key.clone(),
            product: read.device.product.as_deref().map(clean),
            firmware: firmware_of(read),
        },
        origin.device_id,
    );
    st.load_own(root, &key);
    let dir = autobackup::profile_dir(root, &key, p);
    let promote =
        |reason: AutoReason, out: &mut Observed| match autobackup::save(&dir, read, reason, keep) {
            Ok((path, rotation)) => {
                out.saved = Some(path);
                if let Some(error) = rotation {
                    out.notices.push(Notice::AutoBackupFailed {
                        profile: p,
                        message: format!(
                            "older automatic backups were kept (none deleted): {}",
                            error.message
                        ),
                    });
                }
            }
            Err(error) => out.notices.push(Notice::AutoBackupFailed {
                profile: p,
                message: error.message,
            }),
        };
    let baseline = autobackup::baseline(&dir);
    let assessment = baseline
        .as_ref()
        .and_then(|(file, known)| assess(known, read, profile).ok().map(|a| (file, a)));
    let Some((file, assessment)) = assessment else {
        // B6 (d): the first complete read (or no usable backup).
        promote(AutoReason::FirstRead, &mut out);
        if st.own.remove(&p).is_some() {
            st.save_own(root);
        }
        st.reset_applied.insert(p);
        st.profiles.insert(
            p,
            ProfileCheck {
                checked_at: read.created_at.clone(),
                known_good_at: None,
                known_good_age_s: None,
                evidence: Vec::new(),
                factory_now: Vec::new(),
                changed: Vec::new(),
                held: None,
            },
        );
        st.resolve_report_if_done(root);
        return out;
    };
    let file_name = file.file_name.clone();
    let own = st.own.get(&p).cloned();
    let previous = st.profiles.get(&p).cloned();
    let held_before = previous.as_ref().is_some_and(|c| c.held.is_some());
    let own_tables: BTreeSet<KnownGoodTable> = match (&own, read.snapshot(profile)) {
        // A held profile: only a completed restore (or an import of every
        // held table) explains it, and that already released the hold; a
        // restore that may not have been saved, a partial import or an
        // edit explain nothing.
        (Some(_), _) if held_before => BTreeSet::new(),
        // Only the tables that hold what the write left in them (a reset or
        // another program since is evidence again).
        (Some(o), Some(view)) => o.explains(view),
        _ => BTreeSet::new(),
    };
    let factory_now: Vec<KnownGoodTable> = assessment
        .tables_with(TableVerdict::ChangedToFactory)
        .into_iter()
        .filter(|t| !own_tables.contains(t))
        .collect();
    let changed = changed_tables(&assessment, &own_tables);
    let mut evidence = evidence_view(&assessment.evidence, &own_tables);
    if !held_before && let Some(o) = &own {
        // The firmware a completed restore was made on is the user's
        // choice; R2 against another firmware still counts.
        evidence.retain(|e| match e {
            ResetEvidenceView::FirmwareChanged { current, .. } => !o.accepts_firmware(current),
            _ => true,
        });
    }
    if origin.after_cli_write {
        // The command line tool wrote under its exclusive lock: factory
        // forms are the user's own change there (R1 and R2 still count).
        evidence.retain(|e| matches!(e, ResetEvidenceView::FirmwareChanged { .. }));
    }
    let r1 = st.r1_armed && !st.reset_applied.contains(&p);
    st.reset_applied.insert(p);
    if evidence.is_empty() && held_before && !factory_now.is_empty() {
        // Still held for the reason found before.
        evidence = previous.map(|c| c.evidence).unwrap_or_default();
    }
    if r1 && !factory_now.is_empty() {
        evidence.insert(0, ResetEvidenceView::ResetSeen);
    }
    for e in &evidence {
        if let ResetEvidenceView::FirmwareChanged { known, current } = e {
            let pair = (format!("{known:?}"), format!("{current:?}"));
            if st.firmware_notified.insert(pair) {
                out.notices.push(Notice::FirmwareChanged {
                    known: known.clone(),
                    current: current.clone(),
                });
            }
        }
    }
    let check = ProfileCheck {
        checked_at: read.created_at.clone(),
        known_good_at: Some(assessment.known_created_at.clone()),
        known_good_age_s: age_s(&assessment.known_created_at, &read.created_at),
        evidence: evidence.clone(),
        factory_now: factory_now.clone(),
        changed,
        held: None,
    };
    if !evidence.is_empty() {
        // B6 (c): held until "Restore" or "It's fine, keep them"; nothing
        // is written, and the own writes stay the profile's.
        if st.notified.insert((p, file_name)) {
            out.notices.push(Notice::ResetSuspected {
                profile: p,
                evidence,
            });
        }
        st.profiles.insert(
            p,
            ProfileCheck {
                held: Some(Arc::new(read.clone())),
                ..check
            },
        );
        return out;
    }
    let waiting = own
        .as_ref()
        .is_some_and(|o| o.running || (o.uncertain && o.generation == generation));
    // At the first comparison after the keyboard's reset report, and at
    // every later one while the report stands, a read whose keyboard-wide
    // SOCD power is back at its reset form is not saved: the backup from
    // before the reset keeps the power "Restore all profiles" writes (a
    // restore writes the power back, and "It's fine, keep them" or the last
    // comparison end the report).
    let reset_power = assessment.socd_power == TableVerdict::ChangedToFactory
        && (r1 || st.reset_reported.is_some());
    // Another firmware that is no evidence (accepted by a restore made on
    // it) is a change to keep: the next comparison is against it.
    let firmware_changed = assessment
        .evidence
        .iter()
        .any(|e| matches!(e, ResetEvidence::FirmwareChanged { .. }));
    if !waiting && !reset_power {
        if !assessment.is_unchanged() || firmware_changed {
            let reason = if own.is_some() {
                AutoReason::AfterWrite
            } else {
                AutoReason::ChangedElsewhere
            };
            promote(reason, &mut out);
        }
        if st.own.remove(&p).is_some() {
            st.save_own(root);
        }
    }
    st.profiles.insert(p, check);
    st.resolve_report_if_done(root);
    out
}

/// The tables that changed otherwise, and the own writes' changes.
fn changed_tables(
    assessment: &ProfileAssessment,
    own: &BTreeSet<KnownGoodTable>,
) -> Vec<KnownGoodTable> {
    assessment
        .tables
        .iter()
        .filter(|t| {
            t.verdict == TableVerdict::ChangedOther
                || (t.verdict == TableVerdict::ChangedToFactory && own.contains(&t.table))
        })
        .map(|t| t.table)
        .collect()
}

/// The keyboard reported a reset (R1): the next comparison of each profile
/// holds its factory tables.
pub(crate) fn mark_reset_reported(shared: &Shared, root: &Path, at: String) {
    let mut st = shared.known_good();
    st.reset_reported = Some(at);
    st.r1_armed = true;
    st.reset_applied.clear();
    // The reset replaced whatever OpenWraith wrote: its writes explain
    // nothing any more.
    st.own.clear();
    st.save_own(root);
    // Comparisons from before the report say nothing about the settings
    // now: a profile not read since is "not compared yet" (held ones stay
    // held).
    st.profiles.retain(|_, c| c.held.is_some());
}

/// A write of `tables` (`restore`: a restore of the whole profile) to
/// `profiles` starts (from the journal).
pub(crate) fn begin_own_write(
    shared: &Shared,
    root: &Path,
    tables: &BTreeSet<KnownGoodTable>,
    restore: bool,
    profiles: &[u8],
    generation: u32,
) {
    let mut st = shared.known_good();
    if let Some(key) = st.device.as_ref().map(|d| d.key.clone()) {
        st.load_own(root, &key);
    }
    let firmware = st.device.as_ref().map(|d| d.firmware.clone());
    for p in profiles {
        let own = st.own.entry(*p).or_insert_with(|| OwnWrite {
            tables: BTreeMap::new(),
            running: false,
            uncertain: false,
            generation,
            restore: false,
            resolved: false,
            firmware: None,
        });
        // What the tables hold is not known until the write is re-read.
        for table in tables {
            own.tables.insert(*table, None);
        }
        own.running = true;
        own.generation = generation;
        own.restore |= restore;
        own.firmware.clone_from(&firmware);
    }
    st.save_own(root);
}

/// A write to `profile` that completed with nothing lost or maybe lost
/// left `view` in the keyboard (its own re-read patched into the complete
/// read made right before it, or the backup a restore wrote): what `tables`
/// hold there is recorded, and a table the view does not hold is no
/// longer counted as written.
pub(crate) fn record_own_content(
    shared: &Shared,
    root: &Path,
    profile: u8,
    view: SnapshotView<'_>,
    tables: &BTreeSet<KnownGoodTable>,
) {
    let mut st = shared.known_good();
    let Some(own) = st.own.get_mut(&profile) else {
        return;
    };
    if own.running {
        return;
    }
    for table in tables {
        if !own.tables.contains_key(table) {
            continue;
        }
        match table_digest(*table, view) {
            Some(digest) => {
                own.tables.insert(*table, Some(digest));
            }
            None => {
                own.tables.remove(table);
            }
        }
    }
    st.save_own(root);
}

/// Another program was heard: own writes of unknown content explain
/// nothing from now on.
pub(crate) fn other_program_heard(shared: &Shared, root: &Path) {
    shared.known_good().forget_unknown_own_writes(root);
}

/// The running write ended: `ok` when it completed with nothing lost or
/// maybe lost. A restore that completed resolves the reset of the profiles
/// it wrote, and so does an import that completed and wrote every table
/// held at its factory form (their held reads are dropped; the next read
/// is saved as their automatic backup). Anything else leaves a held
/// profile held.
pub(crate) fn finish_own_write(shared: &Shared, root: &Path, ok: bool) {
    let mut guard = shared.known_good();
    let st = &mut *guard;
    let mut resolved = Vec::new();
    let mut restored = Vec::new();
    let mut ended = false;
    for (p, own) in &mut st.own {
        if own.running {
            ended = true;
            own.running = false;
            own.uncertain |= !ok;
            if ok && own.restore {
                // Restored: R1 has nothing left to say about it.
                restored.push(*p);
            }
            let covers = st.profiles.get(p).is_some_and(|c| {
                c.held.is_some()
                    && (own.restore || c.factory_now.iter().all(|t| own.tables.contains_key(t)))
            });
            if ok && (own.restore || covers) {
                own.resolved = true;
                resolved.push(*p);
            }
        }
    }
    st.reset_applied.extend(restored);
    if ended {
        st.save_own(root);
    }
    if resolved.is_empty() {
        return;
    }
    for p in &resolved {
        if let Some(check) = st.profiles.get_mut(p) {
            check.held = None;
            check.evidence.clear();
            check.factory_now.clear();
        }
    }
    st.resolve_report_if_done(root);
}

// ----- the commands -----

/// The store folder of the keyboard read last, if any.
fn current_key(shared: &Shared) -> Option<String> {
    shared.known_good().device.as_ref().map(|d| d.key.clone())
}

/// `list_auto_backups`: the keyboard read last, else every keyboard in the
/// store; newest first.
pub(crate) fn list_auto_backups(shared: &Shared, config: &CoreConfig) -> AutoBackupsView {
    let root = config.known_good_dir();
    let settings = crate::settings::auto_backup(config.settings_path.as_deref());
    let current = current_key(shared);
    let keys = match &current {
        Some(key) => vec![key.clone()],
        None => autobackup::list_devices(&root),
    };
    let mut files: Vec<(StoreFile, u8)> = Vec::new();
    for key in &keys {
        for p in 1..=4u8 {
            let dir = autobackup::profile_dir(&root, key, p);
            files.extend(
                autobackup::list_profile(&dir)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|f| (f, p)),
            );
        }
    }
    files.sort_by(|(a, pa), (b, pb)| {
        b.name
            .stamp
            .cmp(&a.name.stamp)
            .then(b.name.n.cmp(&a.name.n))
            .then(pa.cmp(pb))
    });
    AutoBackupsView {
        enabled: settings.enabled,
        keep: settings.keep,
        connected_keyboard: current.is_some(),
        unidentified: keys.iter().any(|k| k == autobackup::UNIDENTIFIED),
        entries: files
            .into_iter()
            .filter_map(|(f, p)| entry(shared, f, p))
            .collect(),
    }
}

/// The profile folder of an automatic backup token (its file must be in
/// the store, in a `profile-<N>` folder, with an automatic backup's name).
fn auto_file(
    shared: &Shared,
    config: &CoreConfig,
    token: &FileToken,
) -> Result<(PathBuf, String, u8), AppError> {
    let unknown = || AppError::new(AppErrorKind::FileUnknown, "not an automatic backup");
    let file = shared
        .files()
        .get(token)
        .filter(|f| f.kind == FileKind::Auto)
        .ok_or_else(unknown)?;
    let root = config.known_good_dir();
    let dir = file.path.parent().ok_or_else(unknown)?.to_path_buf();
    let name = file
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(unknown)?;
    let profile = dir
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("profile-"))
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|p| (1..=4).contains(p))
        .ok_or_else(unknown)?;
    let in_store = dir
        .parent()
        .and_then(Path::parent)
        .is_some_and(|r| r == root);
    let is_auto = autobackup::parse_name(&name).is_some_and(|n| n.reason().is_some());
    if !in_store || !is_auto {
        return Err(unknown());
    }
    Ok((dir, name, profile))
}

/// `pin_auto_backup`: keeps (or no longer keeps) an automatic backup.
pub(crate) fn pin_auto_backup(
    shared: &Shared,
    config: &CoreConfig,
    token: &FileToken,
    pinned: bool,
) -> Result<AutoBackupsView, AppError> {
    let (dir, name, _) = auto_file(shared, config, token)?;
    {
        // The lock serializes the store's files (rotation).
        let _st = shared.known_good();
        if !dir.join(&name).is_file() {
            return Err(AppError::new(
                AppErrorKind::FileUnknown,
                "the automatic backup no longer exists",
            ));
        }
        autobackup::set_pinned(&dir, &name, pinned)?;
    }
    Ok(list_auto_backups(shared, config))
}

/// `get_recovery`: the Recovery card.
pub(crate) fn get_recovery(shared: &Shared, config: &CoreConfig) -> RecoveryView {
    let root = config.known_good_dir();
    let settings = crate::settings::auto_backup(config.settings_path.as_deref());
    let (device, checks, reset_reported, firmware_changed, status) = {
        let st = shared.known_good();
        (
            st.device.clone(),
            st.profiles.clone(),
            st.reset_reported.clone(),
            st.firmware_change(),
            st.status(),
        )
    };
    let names = match (
        config.settings_path.as_deref(),
        device.as_ref().and_then(|d| d.product.as_ref()),
    ) {
        (Some(path), Some(product)) => crate::settings::profile_names(path)
            .remove(product)
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut newest_firmware = Vec::new();
    let mut profiles = Vec::new();
    for p in 1..=4u8 {
        let files = device.as_ref().map_or_else(Vec::new, |d| {
            autobackup::list_profile(&autobackup::profile_dir(&root, &d.key, p)).unwrap_or_default()
        });
        let firmware_differs = match (files.first(), &device) {
            (Some(newest), Some(device)) => {
                let firmware = Backup::load(&newest.path).ok().map(|b| firmware_of(&b));
                if let Some(firmware) = &firmware {
                    newest_firmware.push(firmware.clone());
                }
                firmware.is_some_and(|f| f.version != device.firmware.version)
            }
            _ => false,
        };
        let backups: Vec<AutoBackupEntry> = files
            .into_iter()
            .filter_map(|f| entry(shared, f, p))
            .collect();
        let check = checks.get(&p);
        let state = match check {
            Some(c) if c.held.is_some() => RecoveryProfileState::ResetSuspected,
            Some(_) => RecoveryProfileState::Fine,
            None if backups.is_empty() => RecoveryProfileState::NoKnownGood,
            None => RecoveryProfileState::NotChecked,
        };
        let held = check.is_some_and(|c| c.held.is_some());
        profiles.push(RecoveryProfileView {
            profile: p,
            name: names.get(usize::from(p - 1)).cloned().flatten(),
            state,
            evidence: check
                .filter(|_| held)
                .map(|c| c.evidence.clone())
                .unwrap_or_default(),
            factory_now: check
                .filter(|_| held)
                .map(|c| c.factory_now.iter().copied().map(table_view).collect())
                .unwrap_or_default(),
            changed: check
                .map(|c| c.changed.iter().copied().map(table_view).collect())
                .unwrap_or_default(),
            known_good_at: check.and_then(|c| c.known_good_at.clone()),
            known_good_age_s: check.and_then(|c| c.known_good_age_s),
            checked_at: check.map(|c| c.checked_at.clone()),
            backups,
            firmware_differs,
        });
    }
    let with_backups = profiles.iter().filter(|p| !p.backups.is_empty()).count();
    let one_firmware = newest_firmware
        .windows(2)
        .all(|w| w[0].version == w[1].version && w[0].build_date == w[1].build_date);
    RecoveryView {
        attention: !status.reset_suspected.is_empty()
            || status.reset_reported
            || status.firmware_changed,
        reset_reported: reset_reported.is_some(),
        reset_reported_at: reset_reported,
        firmware_changed,
        profiles,
        can_restore_all: with_backups >= 2 && one_firmware,
        auto_backup_enabled: settings.enabled,
        unidentified: device
            .as_ref()
            .is_some_and(|d| d.key == autobackup::UNIDENTIFIED),
    }
}

/// `keep_current_settings` ("It's fine, keep them"): the held reads of
/// `profiles` (every held profile when empty) become their automatic
/// backups, and the keyboard's reset report is resolved once nothing is
/// held and every profile with automatic backups was compared since it.
/// Writes only the store's files.
pub(crate) fn keep_current_settings(
    shared: &Shared,
    config: &CoreConfig,
    profiles: &[u8],
) -> Result<RecoveryView, AppError> {
    if profiles.iter().any(|p| !(1..=4).contains(p)) {
        return Err(refused(
            EditProblemView::Other {
                message: "profiles are 1 to 4".to_string(),
            },
            "nothing was kept",
        ));
    }
    let root = config.known_good_dir();
    let keep = crate::settings::auto_backup(config.settings_path.as_deref()).keep;
    let mut notices = Vec::new();
    {
        let mut st = shared.known_good();
        let key = st.device.as_ref().map(|d| d.key.clone());
        let targets: Vec<u8> = st
            .profiles
            .iter()
            .filter(|(p, c)| c.held.is_some() && (profiles.is_empty() || profiles.contains(p)))
            .map(|(p, _)| *p)
            .collect();
        for p in targets {
            let (Some(key), Some(check)) = (key.as_ref(), st.profiles.get_mut(&p)) else {
                continue;
            };
            let Some(read) = check.held.clone() else {
                continue;
            };
            let dir = autobackup::profile_dir(&root, key, p);
            match autobackup::save(&dir, &read, AutoReason::KeptCurrent, keep) {
                Ok((_, rotation)) => {
                    if let Some(error) = rotation {
                        notices.push(Notice::AutoBackupFailed {
                            profile: p,
                            message: format!(
                                "older automatic backups were kept (none deleted): {}",
                                error.message
                            ),
                        });
                    }
                    check.held = None;
                    check.evidence.clear();
                    check.factory_now.clear();
                    check.known_good_at = Some(read.created_at.clone());
                    check.known_good_age_s = Some(0);
                    if st.own.remove(&p).is_some() {
                        st.save_own(&root);
                    }
                }
                Err(error) => notices.push(Notice::AutoBackupFailed {
                    profile: p,
                    message: error.message,
                }),
            }
        }
        // The report goes once nothing is held and every profile with
        // automatic backups was compared since it (with nothing held this
        // acknowledges a report whose comparisons all came out fine; a
        // profile not compared yet keeps it).
        st.resolve_report_if_done(&root);
    }
    for notice in notices {
        shared.notice(notice);
    }
    shared.republish();
    Ok(get_recovery(shared, config))
}

impl<B: DeviceBackend> Worker<B> {
    /// Compares a complete read of the active profile with its automatic
    /// backup (see the [module documentation](self)); nothing is compared
    /// when another program was heard or automatic backups are off.
    pub(crate) fn observe_read(&mut self, read: &Backup) {
        if self.foreign_heard() || read.is_all_profiles() {
            self.cli_wrote_on = None;
            return;
        }
        let Some(profile) = read.active_profile.and_then(ProfileId::new) else {
            return;
        };
        let settings = crate::settings::auto_backup(self.config.settings_path.as_deref());
        if !settings.enabled {
            self.cli_wrote_on = None;
            return;
        }
        let after_cli_write = self.cli_wrote_on.take() == Some(self.generation);
        let observed = observe(
            &self.shared,
            &self.config.known_good_dir(),
            read,
            profile,
            self.generation,
            settings.keep,
            ReadOrigin {
                device_id: self.device.as_ref().map(|d| d.id.0.as_str()),
                after_cli_write,
            },
        );
        for notice in observed.notices {
            self.shared.notice(notice);
        }
    }

    /// The keyboard reported a reset: R1 for the next comparisons.
    pub(crate) fn note_reset_reported(&self) {
        mark_reset_reported(
            &self.shared,
            &self.config.known_good_dir(),
            openwraith_hid::rfc3339_utc(self.config.now()),
        );
    }

    /// A setting write of `tables` (`restore`: a whole-profile restore) to
    /// `profiles` starts.
    pub(crate) fn own_write_started(
        &self,
        tables: &BTreeSet<KnownGoodTable>,
        restore: bool,
        profiles: &[u8],
    ) {
        begin_own_write(
            &self.shared,
            &self.config.known_good_dir(),
            tables,
            restore,
            profiles,
            self.generation,
        );
    }

    /// The running setting write ended (`ok`: completed, nothing lost or
    /// maybe lost).
    pub(crate) fn own_write_done(&self, ok: bool) {
        finish_own_write(&self.shared, &self.config.known_good_dir(), ok);
    }

    /// An edit of `kind` that completed cleanly was re-read: `fresh` is the
    /// complete read made right before it (its safety backup), `reread` what
    /// the edit read again afterwards. What the edit left in its tables is
    /// recorded (see [`OwnWrite::tables`]).
    pub(crate) fn own_write_reread(&self, kind: OpKind, fresh: &Backup, reread: Vec<RawResponse>) {
        let Some(profile) = fresh.active_profile else {
            return;
        };
        let after = super::snapshot::with_replies(fresh, reread);
        record_own_content(
            &self.shared,
            &self.config.known_good_dir(),
            profile,
            after.primary(),
            &tables_of(kind).0,
        );
    }

    /// A restore or import that completed cleanly wrote `prepared` to the
    /// keyboard: what each profile it wrote holds now is the backup's (see
    /// [`OwnWrite::tables`]).
    pub(crate) fn own_write_restored(&self, prepared: &openwraith_hid::PreparedRestore) {
        let root = self.config.known_good_dir();
        let profiles: BTreeSet<ProfileId> = prepared.plan().writes().map(|(p, _, _)| p).collect();
        let tables: BTreeSet<KnownGoodTable> = match prepared.options().import {
            Some(import) => import_tables(import.include),
            None => tables_of(OpKind::Restore).0,
        };
        for profile in profiles {
            if let Some(view) = prepared.target_snapshot(profile) {
                record_own_content(&self.shared, &root, profile.get(), view, &tables);
            }
        }
    }

    /// Another program was heard (see [`other_program_heard`]).
    pub(crate) fn own_writes_after_other_program(&self) {
        other_program_heard(&self.shared, &self.config.known_good_dir());
    }

    /// `prepare_recovery`: automatic backups compared with the keyboard by
    /// the standard restore (one profile, or several composed into one
    /// backup); the plan is applied with `apply_restore`.
    pub(crate) fn prepare_recovery(
        &mut self,
        intent: &RecoveryIntent,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<RestorePreview, AppError> {
        self.check_writable()?;
        let source = self.recovery_source(&intent.files)?;
        let restore = RestoreIntent {
            source,
            profiles: None,
            allow_firmware_mismatch: intent.allow_firmware_mismatch,
            confirmed_switches: intent.confirmed_switches.clone(),
        };
        self.prepare_restore(&restore, sink, OpKind::RecoveryCheck)
    }

    /// The backup a recovery restores: the automatic backup, or the
    /// automatic backups composed into one (saved in the store).
    fn recovery_source(&self, files: &[FileToken]) -> Result<FileToken, AppError> {
        if files.is_empty() || files.len() > 4 {
            return Err(refused(
                EditProblemView::Other {
                    message: "choose one automatic backup per profile".to_string(),
                },
                "nothing was compared",
            ));
        }
        let mut parts = Vec::new();
        for token in files {
            auto_file(&self.shared, &self.config, token)?;
            let (backup, _, _) = super::restore::load_backup(&self.shared, token)?;
            parts.push(Backup::clone(&backup));
        }
        if let [token] = files {
            return Ok(token.clone());
        }
        let composed = Backup::compose(&parts).map_err(|e| AppError::from(&e))?;
        let key = autobackup::device_key_of(&composed);
        let path = {
            let _st = self.shared.known_good();
            autobackup::save_recovery(&self.config.known_good_dir(), &key, &composed)?
        };
        Ok(self.shared.register_file(path, FileKind::Auto).token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_seconds_of_rfc3339() {
        assert_eq!(epoch_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch_seconds("2026-10-24T07:30:00Z"), Some(1_792_827_000));
        assert_eq!(epoch_seconds("2000-03-01T00:00:01Z"), Some(951_868_801));
        assert_eq!(
            age_s("2026-10-24T07:30:00Z", "2026-10-25T07:30:00Z"),
            Some(86_400)
        );
        assert_eq!(age_s("later", "2026-10-25T07:30:00Z"), None);
    }

    #[test]
    fn own_writes_explain_their_tables_only() {
        let own: BTreeSet<_> = [KnownGoodTable::Triggers].into_iter().collect();
        let evidence = vec![
            ResetEvidence::TriggersToFactory,
            ResetEvidence::KeymapGroupToFactory {
                tables: vec![KnownGoodTable::BaseKeymap, KnownGoodTable::FnKeymap],
            },
        ];
        assert_eq!(
            evidence_view(&evidence, &own),
            vec![ResetEvidenceView::KeymapGroupToFactory {
                tables: vec![KnownGoodTableView::BaseKeymap, KnownGoodTableView::FnKeymap],
            }]
        );
        let own: BTreeSet<_> = [KnownGoodTable::Triggers, KnownGoodTable::FnKeymap]
            .into_iter()
            .collect();
        assert!(evidence_view(&evidence, &own).is_empty());
        assert_eq!(tables_of(OpKind::Restore).0.len(), 9);
        assert!(tables_of(OpKind::Restore).1);
        assert!(!tables_of(OpKind::Import).1, "an import names its tables");
        assert!(!tables_of(OpKind::KeymapEdit).1);
        let lighting = openwraith_hid::ImportSelection {
            triggers: false,
            lighting: true,
            keymap_group: false,
            dead_band: false,
            socd: false,
        };
        assert_eq!(
            import_tables(lighting),
            [KnownGoodTable::MainLight, KnownGoodTable::CustomColors1]
                .into_iter()
                .collect()
        );
    }
}
