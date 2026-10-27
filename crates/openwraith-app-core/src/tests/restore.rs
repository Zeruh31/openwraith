//! Restores and Undo.

use openwraith_hid::SimTable;
use openwraith_hid::protocol::{Layer, MacroSlotId, ReadRequest};

use super::harness::{Harness, pos, profile};
use super::seeds::with_device_info;
use crate::dto::{
    AppErrorKind, CancelState, ErrorDetail, FileToken, Headline, ItemResultView, OutcomeView,
    PersistenceView, PlanActionView, PlanWarningView, ProgressStep, RestoreIntent, RtIntent,
    SimAction, TableRef,
};
use crate::worker::Conn;

const W: u8 = 46;
const FACTORY: [u8; 9] = [0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0];
const DIM_STATIC: [u8; 12] = [0x00, 0x01, 0x02, 0x00, 0xff, 0x00, 0, 0, 0, 0, 0, 0];

fn intent(source: &FileToken, profiles: Option<Vec<u8>>, switches: Vec<u8>) -> RestoreIntent {
    RestoreIntent {
        source: source.clone(),
        profiles,
        allow_firmware_mismatch: false,
        confirmed_switches: switches,
    }
}

/// Ready, with an all-profile backup; then W of profile 1 and the light of
/// profile 2 changed on the keyboard.
fn changed_after_backup() -> (Harness, FileToken) {
    let mut h = Harness::ready();
    let (file, _) = h.backup_all("all.owbackup.json");
    h.with_sim(|s| {
        s.set_trigger(profile(1), pos(W), FACTORY);
        s.set_table(profile(2), SimTable::MainLight, DIM_STATIC.to_vec());
    });
    (h, file)
}

#[test]
fn a_restore_of_all_profiles_writes_the_differences_and_proves_they_persisted() {
    let (mut h, file) = changed_after_backup();
    let writes_before = h.writes();
    let watch = h.watch();
    let preview = h
        .prepare_restore(intent(&file, None, vec![2, 3, 4]), &watch)
        .unwrap();
    assert_eq!(preview.switches, vec![2, 3, 4]);
    assert!(!preview.nothing_to_write);
    assert_eq!(preview.plan.totals.writes, 2);
    assert_eq!(preview.source.profiles, vec![1, 2, 3, 4]);
    assert_eq!(preview.source.same_model, Some(true));
    // The comparison only switched profiles.
    let new: Vec<u8> = h.selects().into_iter().skip(4).collect();
    assert_eq!(new, vec![2, 3, 4, 1]);
    assert_eq!(h.writes() - writes_before, 4);
    let p1 = &preview.plan.profiles[0];
    assert!(p1.items.iter().any(|i| i.table == TableRef::Triggers
        && i.key.as_ref().is_some_and(|k| k.index == W)
        && matches!(i.action, PlanActionView::Write { .. })));

    let watch = h.watch();
    let report = h
        .apply_restore(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:#?}");
    assert_eq!(report.outcome, OutcomeView::Completed);
    let persistence = report.persistence.as_ref().unwrap();
    assert_eq!((persistence.persisted, persistence.lost), (2, 0));
    assert_eq!(report.counts.written, 2);
    assert_eq!(report.counts.persisted, 2);
    assert!(
        report
            .profiles
            .iter()
            .flat_map(|p| &p.items)
            .all(|i| matches!(
                i.result,
                ItemResultView::Written {
                    persistence: PersistenceView::Persisted,
                    ..
                }
            ))
    );
    assert_eq!(report.active_profile_now, Some(1));
    assert_eq!(
        h.with_sim(|s| s.persisted_trigger(profile(1), pos(W))),
        Some([0x0C, 50, 50, 15, 15, 0, 0, 0, 0])
    );
    let steps = watch.steps();
    assert!(
        steps
            .iter()
            .any(|s| matches!(s, ProgressStep::CheckingPersistence { .. }))
    );
    assert!(
        steps
            .iter()
            .any(|s| matches!(s, ProgressStep::WaitingForKeyboardSave { .. }))
    );
    assert!(
        steps
            .iter()
            .any(|s| matches!(s, ProgressStep::SavingSafetyBackup))
    );
    let safety = h.safety_files();
    assert_eq!(safety.len(), 1);
    assert!(
        safety[0]
            .to_string_lossy()
            .ends_with("-restore.owbackup.json")
    );
    assert_eq!(h.snapshot().profile, 1);
    assert!(!h.worker.config.journal_path().exists());
    assert_eq!(*watch.journal_seen.lock().unwrap(), Some(true));
    h.assert_clean();
}

#[test]
fn the_comparison_needs_consent_for_its_switches() {
    let (mut h, file) = changed_after_backup();
    let before = h.packets();
    let error = h
        .prepare_restore(intent(&file, None, vec![]), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::NeedsSwitchConsent);
    assert_eq!(
        error.detail,
        Some(ErrorDetail::NeedsSwitchConsent {
            profiles: vec![2, 3, 4]
        })
    );
    let sent = h.packets_since(before);
    assert_eq!(sent.len(), 1, "only the active-profile read");
    assert_eq!(
        ReadRequest::from_packet(sent[0].as_bytes()),
        Some(ReadRequest::ActiveProfile)
    );
}

#[test]
fn profiles_not_in_the_backup_are_refused_without_traffic() {
    let mut h = Harness::ready();
    let destination = h.destination("one.owbackup.json");
    h.backup_active(&destination, &h.watch()).unwrap();
    let file = h.user_file(h.dir.path().join("one.owbackup.json"));
    let before = h.packets();
    let error = h
        .prepare_restore(intent(&file, Some(vec![3]), vec![3]), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::InvalidBackup);
    assert_eq!(
        error.detail,
        Some(ErrorDetail::ProfileNotInBackup { profile: 3 })
    );
    assert_eq!(h.packets(), before);
}

#[test]
fn a_backup_of_another_model_or_firmware_is_refused_without_traffic() {
    let mut h = Harness::ready();
    let other = h.dir.path().join("other.owbackup.json");
    with_device_info("AP61HEARGB", "3.22.07")
        .save(&other, false)
        .unwrap();
    let file = h.user_file(other);
    let before = h.packets();
    let error = h
        .prepare_restore(intent(&file, None, vec![]), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::ModelMismatch);
    let newer = h.dir.path().join("newer.owbackup.json");
    with_device_info("W75HEARGB", "3.23.01")
        .save(&newer, false)
        .unwrap();
    let file = h.user_file(newer);
    let error = h
        .prepare_restore(intent(&file, None, vec![]), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::FirmwareMismatch);
    assert_eq!(h.packets(), before);
    // Allowed explicitly, the plan carries a warning.
    let preview = h
        .prepare_restore(
            RestoreIntent {
                allow_firmware_mismatch: true,
                ..intent(&file, None, vec![])
            },
            &h.watch(),
        )
        .unwrap();
    assert!(
        preview
            .plan
            .warnings
            .iter()
            .any(|w| matches!(w, PlanWarningView::FirmwareDiffers { .. }))
    );
    assert_eq!(preview.source.same_firmware, Some(false));
}

#[test]
fn a_backup_that_matches_has_nothing_to_write() {
    let mut h = Harness::ready();
    let (file, _) = h.backup_all("all.owbackup.json");
    let preview = h
        .prepare_restore(intent(&file, None, vec![2, 3, 4]), &h.watch())
        .unwrap();
    assert!(preview.nothing_to_write);
    assert!(preview.token.is_none());
    assert!(h.safety_files().is_empty());
}

#[test]
fn a_slow_save_is_reported_lost() {
    let (mut h, file) = changed_after_backup();
    h.sim.apply(SimAction::SlowSave { ms: 5000 }).unwrap();
    let preview = h
        .prepare_restore(intent(&file, Some(vec![2]), vec![2]), &h.watch())
        .unwrap();
    let report = h
        .apply_restore(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Lost, "{report:#?}");
    assert_eq!(report.counts.lost, 1);
    assert_eq!(
        report.error.as_ref().unwrap().kind,
        AppErrorKind::NotPersisted
    );
    h.assert_clean();
}

#[test]
fn an_unplug_while_waiting_for_the_save_is_maybe_lost() {
    let (mut h, file) = changed_after_backup();
    let preview = h
        .prepare_restore(intent(&file, Some(vec![1]), vec![]), &h.watch())
        .unwrap();
    let watch = h.watch();
    let sim = h.sim.clone();
    let mut done = false;
    watch.on_event(move |event, _| {
        if !done && matches!(event.step, ProgressStep::WaitingForKeyboardSave { .. }) {
            done = true;
            sim.apply(SimAction::Unplug).unwrap();
        }
    });
    let report = h
        .apply_restore(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.headline, Headline::MaybeLost, "{report:#?}");
    assert!(matches!(h.worker.conn, Conn::Lost { .. }));
    assert!(h.worker.session.is_none());
    h.assert_clean();
}

#[test]
fn stop_inside_the_keymap_group_still_sends_the_fn_keymap() {
    let mut h = Harness::ready();
    let (file, _) = h.backup_all("all.owbackup.json");
    h.with_sim(|s| {
        let mut base = s
            .table(profile(1), SimTable::Keymap(Layer::Base))
            .unwrap()
            .to_vec();
        base[usize::from(W) * 4 + 2] ^= 0x01;
        s.set_table(profile(1), SimTable::Keymap(Layer::Base), base);
        let mut slot = vec![0u8; 256];
        slot[..8].copy_from_slice(&[0, 0, 0, 4, 0x01, 0x04, 0x00, 0x00]);
        s.set_table(
            profile(1),
            SimTable::Macro(MacroSlotId::new(0).unwrap()),
            slot,
        );
    });
    let preview = h
        .prepare_restore(intent(&file, Some(vec![1]), vec![]), &h.watch())
        .unwrap();
    let tables: Vec<TableRef> = preview.plan.profiles[0]
        .items
        .iter()
        .filter(|i| matches!(i.action, PlanActionView::Write { .. }))
        .map(|i| i.table)
        .collect();
    assert_eq!(
        tables,
        vec![
            TableRef::Macro { slot: 0 },
            TableRef::Keymap {
                layer: crate::dto::LayerView::Base
            },
            TableRef::Keymap {
                layer: crate::dto::LayerView::Fn
            },
        ]
    );
    let watch = h.watch();
    watch.cancel_when(super::harness::is_write_start);
    let report = h
        .apply_restore(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.outcome, OutcomeView::Cancelled, "{report:#?}");
    let items = &report.profiles[0].items;
    assert!(matches!(items[0].result, ItemResultView::Written { .. }));
    assert!(matches!(items[1].result, ItemResultView::NotAttempted));
    assert!(matches!(items[2].result, ItemResultView::Written { .. }));
    assert!(items[2].resent_unchanged);
    assert!(
        watch
            .all()
            .iter()
            .any(|e| e.cancel == CancelState::FinishingKeymapGroup)
    );
    // The Fn keymap went out after the stop.
    let fn_writes = h
        .write_bytes()
        .into_iter()
        .filter(|b| b[..2] == [0x18, 0x02])
        .count();
    assert_eq!(fn_writes, 10);
    h.assert_clean();
}

#[test]
fn undo_restores_the_safety_backup_of_an_edit() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(100), Some(RtIntent::Off)))
        .unwrap();
    let report = h
        .apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    let undo = report.safety_backup.token.clone();
    let preview = h
        .prepare_restore(intent(&undo, None, vec![]), &h.watch())
        .unwrap();
    assert_eq!(preview.source.kind, crate::dto::BackupFileKind::Safety);
    assert_eq!(
        preview.source.safety_reason,
        Some(crate::dto::SafetyReason::SetTrigger)
    );
    assert_eq!(preview.plan.totals.writes, 1);
    let report = h
        .apply_restore(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:#?}");
    assert_eq!(
        h.with_sim(|s| s.persisted_trigger(profile(1), pos(W))),
        Some([0x0C, 50, 50, 15, 15, 0, 0, 0, 0])
    );
    // Undo made its own safety backup.
    assert_eq!(h.safety_files().len(), 2);
    h.assert_clean();
}

#[test]
fn an_opened_file_is_compared_with_the_keyboard_once_one_has_been_read() {
    let mut h = Harness::new(1);
    let path = h.dir.path().join("opened.owbackup.json");
    with_device_info("W75HEARGB", "3.22.07")
        .save(&path, false)
        .unwrap();
    let file = h.user_file(path);
    // Opened before any keyboard was read: no comparison yet.
    let before = crate::ops::restore::summarize_file(&h.shared, &file).unwrap();
    assert_eq!(before.same_model, None);
    assert_eq!(before.same_firmware, None);
    assert_eq!(before.kind, crate::dto::BackupFileKind::User);

    h.until_ready();
    let packets = h.packets();
    let after = crate::ops::restore::summarize_file(&h.shared, &file).unwrap();
    assert_eq!(after.file.token, file);
    assert_eq!(after.same_model, Some(true));
    assert_eq!(after.same_firmware, Some(true));
    // Summarizing reads the file (from disk), never the keyboard.
    assert_eq!(h.packets(), packets);

    // A backup destination isn't a backup to summarize.
    let destination = h.destination("new.owbackup.json");
    let error = crate::ops::restore::summarize_file(&h.shared, &destination.token).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::FileUnknown);
    h.assert_clean();
}

#[test]
fn a_file_overwritten_after_it_was_opened_is_read_again() {
    let mut h = Harness::ready();
    let name = "mine.owbackup.json";
    let path = h.dir.path().join(name);
    let destination = h.destination(name);
    h.backup_active(&destination, &h.watch()).unwrap();
    let file = h.user_file(path.clone());
    let first = crate::ops::restore::summarize_file(&h.shared, &file).unwrap();
    assert_eq!(first.profiles, vec![1]);
    // Overwritten by an all-profile backup to the same name.
    let (again, _) = h.backup_all(name);
    assert_eq!(again, file, "the same path keeps its token");
    let second = crate::ops::restore::summarize_file(&h.shared, &file).unwrap();
    assert_eq!(second.profiles, vec![1, 2, 3, 4], "read from disk again");
    // The restore plans from the file on disk, too.
    let preview = h
        .prepare_restore(intent(&file, None, vec![2, 3, 4]), &h.watch())
        .unwrap();
    assert_eq!(preview.source.profiles, vec![1, 2, 3, 4]);
    assert_eq!(preview.plan.profiles.len(), 4);
    // A deleted file is an error, not a plan.
    std::fs::remove_file(&path).unwrap();
    let error = crate::ops::restore::summarize_file(&h.shared, &file).unwrap_err();
    assert!(
        matches!(error.kind, AppErrorKind::Io | AppErrorKind::InvalidBackup),
        "{error:?}"
    );
    let writes = h.writes();
    let error = h
        .prepare_restore(intent(&file, None, vec![2, 3, 4]), &h.watch())
        .unwrap_err();
    assert!(
        matches!(error.kind, AppErrorKind::Io | AppErrorKind::InvalidBackup),
        "{error:?}"
    );
    assert_eq!(h.writes(), writes, "nothing was sent");
    h.assert_clean();
}
