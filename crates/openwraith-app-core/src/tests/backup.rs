//! Backups to a file.

use openwraith_hid::Backup;

use super::harness::Harness;
use crate::dto::{AppErrorKind, ErrorDetail, ProgressStep, ReadPurpose};

#[test]
fn a_backup_of_the_active_profile_only_reads() {
    let mut h = Harness::ready();
    let destination = h.destination("active.owbackup.json");
    let watch = h.watch();
    let saved = h.backup_active(&destination, &watch).unwrap();
    assert_eq!(saved.profiles, vec![1]);
    assert_eq!(saved.firmware.as_deref(), Some("3.22.07"));
    assert_eq!(saved.file.token, destination.token);
    assert_eq!(h.writes(), 0);
    let backup = Backup::load(&h.dir.path().join("active.owbackup.json")).unwrap();
    assert_eq!(backup.profile_numbers(), vec![1]);
    assert!(watch.steps().iter().any(|s| matches!(
        s,
        ProgressStep::Reading {
            purpose: ReadPurpose::Backup,
            ..
        }
    )));
    assert!(watch.steps().contains(&ProgressStep::SavingFile));
    h.assert_clean();
}

#[test]
fn an_all_profile_backup_switches_through_the_profiles_and_back() {
    let mut h = Harness::ready();
    let destination = h.destination("all.owbackup.json");
    let preview = h.prepare_all(&destination).unwrap();
    assert_eq!(preview.original, 1);
    assert_eq!(preview.visits, vec![2, 3, 4]);
    assert_eq!(h.writes(), 0, "prepare is read-only");
    let watch = h.watch();
    let saved = h.apply_all(&preview.token, &watch).unwrap();
    assert_eq!(saved.profiles, vec![1, 2, 3, 4]);
    assert_eq!(h.selects(), vec![2, 3, 4, 1]);
    assert_eq!(h.writes(), 4, "profile selects only");
    assert_eq!(h.snapshot().profile, 1);
    let steps = watch.steps();
    assert!(steps.contains(&ProgressStep::SelectingProfile { profile: 3 }));
    assert!(steps.contains(&ProgressStep::ReturningToProfile { profile: 1 }));
    let backup = Backup::load(&h.dir.path().join("all.owbackup.json")).unwrap();
    assert!(backup.is_all_profiles());
    h.assert_clean();
}

#[test]
fn stopping_an_all_profile_backup_returns_to_the_original_profile() {
    let mut h = Harness::ready();
    let destination = h.destination("all.owbackup.json");
    let preview = h.prepare_all(&destination).unwrap();
    let watch = h.watch();
    watch.cancel_when(|e| {
        matches!(
            e.step,
            ProgressStep::Reading {
                profile: Some(2),
                ..
            }
        )
    });
    let error = h.apply_all(&preview.token, &watch).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::Cancelled);
    assert_eq!(
        error.detail,
        Some(ErrorDetail::ActiveProfileNow { profile: Some(1) })
    );
    assert_eq!(h.selects(), vec![2, 1]);
    assert!(!h.dir.path().join("all.owbackup.json").exists());
    assert_eq!(h.with_sim(|s| s.active_profile()), 1);
    h.assert_clean();
}

#[test]
fn an_unknown_destination_is_refused() {
    let mut h = Harness::ready();
    let user = h.user_file(h.dir.path().join("x.json"));
    let destination = crate::dto::FileRef {
        token: user,
        file_name: "x.json".into(),
        display_path: String::new(),
    };
    let error = h.backup_active(&destination, &h.watch()).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::FileUnknown);
}

#[test]
fn the_license_files_are_revealed_from_the_installation_folder() {
    let h = Harness::new(1);
    let handle = crate::handle::WorkerHandle::from_parts(
        h.tx.clone(),
        std::sync::Arc::clone(&h.shared),
        h.worker.config.clone(),
        None,
        None,
    );
    let target = crate::dto::RevealTarget::LegalNotices;
    // Not installed (a build without the resources): an error, no path.
    let error = handle.reveal_path(&target).unwrap_err();
    assert_eq!(error.kind, crate::dto::AppErrorKind::Io);
    let dir = h.dir.path().join("legal");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(crate::config::NOTICES_FILE), "notices").unwrap();
    assert_eq!(
        handle.reveal_path(&target).unwrap(),
        dir.join(crate::config::NOTICES_FILE)
    );
}
