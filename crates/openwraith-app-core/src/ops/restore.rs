//! Restores (and Undo, which restores a safety backup).

use std::collections::BTreeSet;
use std::sync::Arc;

use openwraith_hid::protocol::ProfileId;
use openwraith_hid::{
    Backup, PreparedRestore, ProfileSet, RestoreOptions, WriteOptions, execute_restore,
    prepare_restore,
};

use super::consent;
use super::profile::read_active;
use super::progress::CancelPolicy;
use super::snapshot::{ReadFail, ReadVia};
use crate::backend::DeviceBackend;
use crate::dto::{
    AppError, AppErrorKind, BackupFileKind, BackupFileSummary, ErrorDetail, FileToken, IntentField,
    LastReport, OpKind, PlanInvalidation, PlanToken, ReadPurpose, RestoreIntent, RestorePreview,
    RestoreReportView,
};
use crate::files::{FileKind, parse_safety_name, safety_reason};
use crate::map::{backup_summary, restore_plan_view, restore_report_view};
use crate::sink::ProgressSink;
use crate::worker::{PendingPlan, PlanKind, Worker, stale};

/// Loads the backup file of `token` from disk. Every call reads the file
/// again (backups are small): a file overwritten since it was opened (by a
/// new backup to the same name, the CLI, ...) is never planned or written
/// from an older copy, and the summary the user confirms is the file's.
pub(crate) fn load_backup(
    shared: &crate::shared::Shared,
    token: &FileToken,
) -> Result<(Arc<Backup>, FileKind, std::path::PathBuf), AppError> {
    let file = shared
        .files()
        .get(token)
        .filter(|f| f.kind != FileKind::Destination)
        .ok_or_else(|| AppError::new(AppErrorKind::FileUnknown, "the backup file is not known"))?;
    let backup = Arc::new(Backup::load(&file.path).map_err(|e| AppError::from(&e))?);
    Ok((backup, file.kind, file.path))
}

/// Loads the backup file of `token` and summarizes it against the keyboard
/// read last (if any).
pub(crate) fn summarize_file(
    shared: &crate::shared::Shared,
    token: &FileToken,
) -> Result<BackupFileSummary, AppError> {
    let (backup, kind, path) = load_backup(shared, token)?;
    let connected = shared.snapshot().map(|s| s.device);
    Ok(summary_of(token, &backup, kind, &path, connected.as_ref()))
}

/// The summary of a loaded backup file.
pub(crate) fn summary_of(
    token: &FileToken,
    backup: &Backup,
    kind: FileKind,
    path: &std::path::Path,
    connected: Option<&crate::dto::DeviceIdentityView>,
) -> BackupFileSummary {
    let file = crate::files::file_ref(token.clone(), path);
    let (kind, reason) = match kind {
        FileKind::Safety => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (
                BackupFileKind::Safety,
                Some(
                    parse_safety_name(&name).map_or(crate::dto::SafetyReason::Other, |n| {
                        safety_reason(&n.reason)
                    }),
                ),
            )
        }
        FileKind::Auto => (BackupFileKind::Automatic, None),
        FileKind::User | FileKind::Destination => (BackupFileKind::User, None),
    };
    backup_summary(file, kind, backup, connected, reason)
}

/// The model and firmware of a file against the connected keyboard: nothing
/// is sent for a file of another keyboard (the library checks again).
pub(crate) fn check_device(
    backup: &Backup,
    device: Option<&crate::dto::DeviceIdentityView>,
    allow_firmware_mismatch: bool,
) -> Result<(), AppError> {
    let Some(device) = device else {
        return Ok(());
    };
    if backup.device.product != device.product {
        return Err(AppError::with(
            AppErrorKind::ModelMismatch,
            ErrorDetail::ModelMismatch {
                backup: backup.device.product.clone(),
                keyboard: device.product.clone(),
            },
            "the backup was made on another keyboard model; nothing was sent",
        ));
    }
    if backup.device.firmware != device.firmware && !allow_firmware_mismatch {
        return Err(AppError::with(
            AppErrorKind::FirmwareMismatch,
            ErrorDetail::FirmwareMismatch {
                backup: backup.device.firmware.clone(),
                keyboard: device.firmware.clone(),
            },
            "the backup was made with other firmware; allow the mismatch to restore anyway",
        ));
    }
    Ok(())
}

/// The restore scope: the profiles asked for (checked against the backup,
/// before any traffic), or every profile in the backup.
fn scope(intent: &RestoreIntent, held: &[u8]) -> Result<Vec<u8>, AppError> {
    let Some(profiles) = &intent.profiles else {
        return Ok(held.to_vec());
    };
    let mut set = BTreeSet::new();
    for &p in profiles {
        if ProfileId::new(p).is_none() || !set.insert(p) {
            return Err(super::invalid(
                ErrorDetail::InvalidIntent {
                    field: IntentField::Profiles,
                },
                format!("profile {p} is not valid here"),
            ));
        }
        if !held.contains(&p) {
            return Err(AppError::with(
                AppErrorKind::InvalidBackup,
                ErrorDetail::ProfileNotInBackup { profile: p },
                format!("profile {p} is not in the backup"),
            ));
        }
    }
    if set.is_empty() {
        return Err(super::invalid(
            ErrorDetail::InvalidIntent {
                field: IntentField::Profiles,
            },
            "choose at least one profile",
        ));
    }
    Ok(set.into_iter().collect())
}

impl<B: DeviceBackend> Worker<B> {
    /// `prepare_restore`: compares the backup with the keyboard (switching
    /// profiles after the user agreed to it), with the safety backup read
    /// on the way. `op` is the operation shown meanwhile: `restore_prepare`,
    /// or `recovery_check` for a recovery (the plan is a restore plan either
    /// way, written by `apply_restore` after the review).
    pub(crate) fn prepare_restore(
        &mut self,
        intent: &RestoreIntent,
        sink: Arc<dyn ProgressSink>,
        op: OpKind,
    ) -> Result<RestorePreview, AppError> {
        self.check_writable()?;
        let (backup, kind, path) = load_backup(&self.shared, &intent.source)?;
        let held = backup.profile_numbers();
        let scope = scope(intent, &held)?;
        // Model and firmware first: nothing is sent for a backup of another
        // keyboard (the library checks again).
        let device = self.snapshot.as_ref().map(|s| s.device.clone());
        check_device(&backup, device.as_ref(), intent.allow_firmware_mismatch)?;
        let source = summary_of(&intent.source, &backup, kind, &path, device.as_ref());
        self.invalidate(PlanInvalidation::Superseded);
        self.run_op(op, sink, CancelPolicy::Boundaries, |w, progress| {
            let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
            let active = read_active(session)?;
            let switches: Vec<u8> = scope.iter().copied().filter(|p| *p != active).collect();
            let confirmed: BTreeSet<u8> = intent.confirmed_switches.iter().copied().collect();
            if confirmed.into_iter().collect::<Vec<_>>() != switches
                || intent.confirmed_switches.len() != switches.len()
            {
                return Err(AppError::with(
                    AppErrorKind::NeedsSwitchConsent,
                    ErrorDetail::NeedsSwitchConsent { profiles: switches },
                    "comparing switches profiles; confirm the switches first",
                ));
            }
            if !switches.is_empty() {
                w.ensure_quiet(progress)?;
            }
            let options = RestoreOptions {
                profiles: intent.profiles.as_ref().map(|_| {
                    scope
                        .iter()
                        .filter_map(|p| ProfileId::new(*p))
                        .collect::<ProfileSet>()
                }),
                allow_firmware_mismatch: intent.allow_firmware_mismatch,
                backup: w.config.backup_options(w.sim),
                write: WriteOptions::default(),
                import: None,
            };
            let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
            let prepared =
                prepare_restore(session, &backup, &options, consent::application(), |p| {
                    progress.write(p)
                });
            let prepared = match prepared {
                Ok(prepared) => prepared,
                Err(error) => {
                    if let Some(now) = error.active_profile_now() {
                        w.active_profile = Some(now);
                        if Some(now) != w.snapshot.as_ref().map(|s| s.profile) {
                            w.reread = true;
                        }
                    }
                    return Err(AppError::from(&error));
                }
            };
            let plan = prepared.plan();
            let view = restore_plan_view(plan);
            let skipped = plan.skips().count();
            let has_writes = plan.has_writes();
            let token = has_writes.then(|| {
                w.store_plan(
                    PlanKind::Restore,
                    PendingPlan::Restore {
                        prepared: Box::new(prepared),
                        skipped,
                    },
                )
            });
            Ok(RestorePreview {
                token,
                source,
                plan: view,
                nothing_to_write: !has_writes,
                switches,
            })
        })
    }

    /// `apply_restore`: the confirmed plan, after its safety backup was
    /// saved and verified, with the persistence check.
    pub(crate) fn apply_restore(
        &mut self,
        token: &PlanToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<RestoreReportView, AppError> {
        let PendingPlan::Restore {
            prepared, skipped, ..
        } = self.take_plan(token, PlanKind::Restore)?
        else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        self.apply_prepared_restore(&prepared, skipped, sink, OpKind::Restore, "restore")
    }

    /// Writes a prepared restore or import (`kind`; `reason` names its
    /// safety backup), after its safety backup was saved and verified, with
    /// the persistence check.
    pub(crate) fn apply_prepared_restore(
        &mut self,
        prepared: &PreparedRestore,
        skipped: usize,
        sink: Arc<dyn ProgressSink>,
        kind: OpKind,
        reason: &str,
    ) -> Result<RestoreReportView, AppError> {
        self.check_writable()?;
        self.run_op(kind, sink, CancelPolicy::Restore, |w, progress| {
            w.ensure_quiet(progress)?;
            let (saved, file) = w.save_safety(prepared.safety(), reason, progress)?;
            let profiles: Vec<u8> = prepared
                .plan()
                .writes()
                .map(|(p, _, _)| p.get())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            // An import changes only its chosen parts; a restore the whole
            // profile.
            let (tables, restore) = match prepared.options().import {
                Some(import) => (super::recovery::import_tables(import.include), false),
                None => super::recovery::tables_of(kind),
            };
            w.write_journal_of(kind, profiles, saved.path(), &tables, restore)?;
            progress.items = crate::map::sat16(prepared.plan().write_count());
            let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
            let report = execute_restore(session, prepared, &saved, consent::application(), |p| {
                progress.write(p)
            });
            w.remove_journal();
            let clean = super::recovery::completed_cleanly(
                report.outcome,
                report.lost(),
                report.maybe_lost(),
            );
            w.own_write_done(clean);
            if clean {
                // What each profile written holds now: the backup's.
                w.own_write_restored(prepared);
            }
            w.active_profile = report.active_profile_now;
            // No read of the result after a reset, a lost connection or
            // when another program was heard (it acts on our replies).
            let fatal = w.foreign_heard()
                || w.session
                    .as_ref()
                    .is_none_or(|s| s.keyboard_reset_seen() || s.connection_lost());
            if !fatal {
                match w.full_read(ReadVia::Op(progress, ReadPurpose::Snapshot)) {
                    Ok(backup) => {
                        w.set_snapshot(&backup);
                    }
                    Err(ReadFail::Cancelled) => w.reread = true,
                    Err(fail) => w.read_failed(&fail),
                }
            }
            let view = restore_report_view(&report, file, skipped);
            w.shared.set_last_report(if kind == OpKind::Import {
                LastReport::Import {
                    report: view.clone(),
                }
            } else {
                LastReport::Restore {
                    report: view.clone(),
                }
            });
            Ok(view)
        })
    }
}
