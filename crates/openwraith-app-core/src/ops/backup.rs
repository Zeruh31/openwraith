//! Backups to a file the user chose: the active profile (read-only) or all
//! four profiles (switches profiles with W1 and back; writes no setting).

use std::path::PathBuf;
use std::sync::Arc;

use openwraith_hid::protocol::ProfileId;
use openwraith_hid::{Backup, ProfilesOptions, WriteOptions, read_all_profiles};

use super::consent;
use super::profile::{read_active, save_wait};
use super::progress::CancelPolicy;
use super::snapshot::{ReadFail, ReadVia};
use crate::backend::DeviceBackend;
use crate::dto::{
    AllProfilesPreview, AppError, AppErrorKind, BackupSaved, FileRef, FileToken, LastReport,
    OpKind, PlanInvalidation, PlanToken, ProgressStep, ReadPurpose,
};
use crate::files::FileKind;
use crate::map::{clean, foreign, sat16};
use crate::sink::ProgressSink;
use crate::worker::{PendingPlan, PlanKind, Worker, stale};

impl<B: DeviceBackend> Worker<B> {
    /// The destination of a backup, by token.
    pub(crate) fn destination(&self, token: &FileToken) -> Result<(PathBuf, FileRef), AppError> {
        let files = self.shared.files();
        match files.get(token) {
            Some(file) if file.kind == FileKind::Destination => {
                let reference = crate::files::file_ref(token.clone(), &file.path);
                Ok((file.path, reference))
            }
            _ => Err(AppError::new(
                AppErrorKind::FileUnknown,
                "choose where to save the backup first",
            )),
        }
    }

    /// Saves `backup` to `path` (the save dialog already asked before
    /// replacing a file).
    fn save_backup(
        &self,
        backup: &Backup,
        path: &std::path::Path,
        file: FileRef,
        foreign_activity: Option<crate::dto::ForeignActivityView>,
    ) -> Result<BackupSaved, AppError> {
        backup.save(path, true).map_err(|e| AppError::from(&e))?;
        let undecoded: usize = backup
            .snapshots()
            .iter()
            .map(|s| s.undecoded().count())
            .sum();
        Ok(BackupSaved {
            file,
            profiles: backup.profile_numbers(),
            created_at: clean(&backup.created_at),
            firmware: backup.device.firmware.as_deref().map(clean),
            undecoded: sat16(undecoded),
            foreign_activity,
        })
    }

    /// `backup_active_profile`: a read-only backup of the active profile.
    pub(crate) fn backup_active_profile(
        &mut self,
        destination: &FileToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<BackupSaved, AppError> {
        self.check_readable()?;
        let (path, file) = self.destination(destination)?;
        self.invalidate(PlanInvalidation::Operation);
        self.run_op(
            OpKind::BackupActive,
            sink,
            CancelPolicy::Boundaries,
            |w, progress| {
                let before = w.session.as_ref().map(|s| s.foreign_activity());
                let backup = w
                    .full_read(ReadVia::Op(progress, ReadPurpose::Backup))
                    .map_err(ReadFail::into_error)?;
                let seen = match (&w.session, before) {
                    (Some(session), Some(before)) => {
                        Some(session.foreign_activity().since(&before)).filter(|a| !a.is_quiet())
                    }
                    _ => None,
                };
                progress.step(ProgressStep::SavingFile);
                let saved = w.save_backup(&backup, &path, file, seen.map(foreign))?;
                w.set_snapshot(&backup);
                w.shared.set_last_report(LastReport::Backup {
                    saved: saved.clone(),
                });
                Ok(saved)
            },
        )
    }

    /// `prepare_all_profile_backup`: the profiles it will switch through.
    pub(crate) fn prepare_all_profile_backup(
        &mut self,
        destination: &FileToken,
    ) -> Result<AllProfilesPreview, AppError> {
        self.check_writable()?;
        let (_, file) = self.destination(destination)?;
        let token = destination.clone();
        self.run_op(
            OpKind::BackupAll,
            Arc::new(crate::sink::NoProgress),
            CancelPolicy::Never,
            |w, _| {
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let original = read_active(session)?;
                let save_wait_ms = save_wait(session);
                let visits = ProfileId::ALL
                    .iter()
                    .map(|p| p.get())
                    .filter(|p| *p != original)
                    .collect();
                let plan_token = w.store_plan(
                    PlanKind::AllProfileBackup,
                    PendingPlan::AllProfileBackup { destination: token },
                );
                Ok(AllProfilesPreview {
                    token: plan_token,
                    destination: file,
                    original,
                    visits,
                    save_wait_ms,
                })
            },
        )
    }

    /// `apply_all_profile_backup`: reads all four profiles (switching and
    /// back) and saves them.
    pub(crate) fn apply_all_profile_backup(
        &mut self,
        token: &PlanToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<BackupSaved, AppError> {
        let PendingPlan::AllProfileBackup { destination } =
            self.take_plan(token, PlanKind::AllProfileBackup)?
        else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        let (path, file) = self.destination(&destination)?;
        self.check_writable()?;
        self.run_op(
            OpKind::BackupAll,
            sink,
            CancelPolicy::Boundaries,
            |w, progress| {
                w.ensure_quiet(progress)?;
                let options = ProfilesOptions {
                    backup: w.config.backup_options(w.sim),
                    write: WriteOptions::default(),
                };
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let before = session.foreign_activity();
                let result = read_all_profiles(session, consent::application(), &options, |p| {
                    progress.profiles(ReadPurpose::Backup, p)
                });
                let seen =
                    Some(session.foreign_activity().since(&before)).filter(|a| !a.is_quiet());
                let backup = match result {
                    Ok(backup) => backup,
                    Err(error) => {
                        w.active_profile = error.active_profile_now;
                        if error.active_profile_now != w.snapshot.as_ref().map(|s| s.profile) {
                            w.reread = true;
                        }
                        return Err(AppError::from(&error));
                    }
                };
                progress.step(ProgressStep::SavingFile);
                let saved = w.save_backup(&backup, &path, file, seen.map(foreign))?;
                w.set_snapshot(&backup);
                w.shared.set_last_report(LastReport::Backup {
                    saved: saved.clone(),
                });
                Ok(saved)
            },
        )
    }
}
