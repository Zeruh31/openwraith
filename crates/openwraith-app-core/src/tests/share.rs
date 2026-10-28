//! Sharing a profile: the export (a pure read of the active profile, a
//! switch only after consent, or a backup file; no USB serial) and the
//! import (a restore with the library's import options: the file as
//! untrusted input, the chosen categories only, a critical Fn override
//! only with its own permission).

use std::time::Duration;

use openwraith_hid::protocol::Layer;
use openwraith_hid::{Backup, SimTable};

use super::harness::{Harness, pos, profile};
use super::keymap::{SPACE, TWO, W};
use crate::dto::{
    ActionValue, AppErrorKind, BlockCauseView, BuiltinAckIntent, EditProblemView, ErrorDetail,
    ExportIntent, ExportSource, ExportedProfile, FileToken, Headline, ImportIntent, ImportPreview,
    KeymapChangeIntent, LayerView, Notice, PlanActionView, PlanInvalidation, ShareSelectionView,
    SkipReasonView,
};

fn export_intent(destination: FileToken, profile: u8, switch: bool) -> ExportIntent {
    ExportIntent {
        source: ExportSource::Keyboard,
        profile,
        label: Some("My W75 \u{7}setup".to_string()),
        destination,
        confirmed_switch: switch,
    }
}

fn export(
    h: &mut Harness,
    name: &str,
    profile: u8,
    switch: bool,
) -> (ExportedProfile, std::path::PathBuf) {
    let destination = h.destination(name);
    let watch = h.watch();
    let exported = h
        .export(export_intent(destination.token, profile, switch), &watch)
        .unwrap();
    (exported, h.dir.path().join(name))
}

fn defaults() -> ShareSelectionView {
    crate::ops::share::default_selection()
}

fn import_intent(
    file: FileToken,
    into: u8,
    include: ShareSelectionView,
    switches: Vec<u8>,
) -> ImportIntent {
    ImportIntent {
        source: file,
        from: None,
        into,
        include,
        allowed_critical_keys: Vec::new(),
        allow_firmware_mismatch: false,
        confirmed_switches: switches,
    }
}

#[test]
fn exporting_the_active_profile_is_a_pure_read_without_the_serial() {
    let mut h = Harness::ready();
    let writes = h.writes();
    let (exported, path) = export(&mut h, "mine.owprofile.json", 1, false);
    assert_eq!(h.writes(), writes, "nothing written, no switch");
    assert!(!exported.switched);
    assert_eq!(exported.profile, 1);
    assert!(exported.contents.present.triggers && exported.contents.present.keys);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("SIM-1"), "no USB serial in the file");
    let file = Backup::load(&path).unwrap();
    assert_eq!(file.scope, openwraith_hid::BACKUP_SCOPE_EXPORT);
    assert_eq!(file.profile_numbers(), [1]);
    assert!(file.note.contains("Label: My W75 setup"), "{}", file.note);
    assert!(file.device.usb.is_none());
}

#[test]
fn exporting_another_profile_switches_only_after_consent() {
    let mut h = Harness::ready();
    let destination = h.destination("p2.owprofile.json");
    let before = h.packets();
    let error = h
        .export(
            export_intent(destination.token.clone(), 2, false),
            &h.watch(),
        )
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::NeedsSwitchConsent);
    assert_eq!(
        error.detail,
        Some(ErrorDetail::NeedsSwitchConsent { profiles: vec![2] })
    );
    assert_eq!(h.packets(), before, "nothing sent");
    let exported = h
        .export(export_intent(destination.token, 2, true), &h.watch())
        .unwrap();
    assert!(exported.switched);
    assert_eq!(h.selects(), [2, 1], "to profile 2 and back");
    let file = Backup::load(&h.dir.path().join("p2.owprofile.json")).unwrap();
    assert_eq!(file.profile_numbers(), [2]);
    h.assert_clean();
}

#[test]
fn a_profile_can_be_exported_from_a_backup_file_without_traffic() {
    let mut h = Harness::ready();
    let (all, _) = h.backup_all("all.owbackup.json");
    let destination = h.destination("p3.owprofile.json");
    let before = h.packets();
    let exported = h
        .export(
            ExportIntent {
                source: ExportSource::File { file: all },
                profile: 3,
                label: None,
                destination: destination.token,
                confirmed_switch: false,
            },
            &h.watch(),
        )
        .unwrap();
    assert_eq!(h.packets(), before);
    assert_eq!(exported.profile, 3);
    assert!(!exported.switched);
}

#[test]
fn an_import_writes_the_chosen_categories_into_the_chosen_profile() {
    let mut h = Harness::ready();
    let (_, path) = export(&mut h, "p1.owprofile.json", 1, false);
    let summary = crate::ops::share::import_summary(&h.shared, &path).unwrap();
    assert_eq!(summary.profile, Some(1));
    assert_eq!(summary.default_include, defaults());
    assert!(defaults().triggers && defaults().lighting);
    assert!(!defaults().keys && !defaults().dead_band && !defaults().socd);
    let token = summary.file.file.token.clone();
    // Comparing with profile 2 switches to it: consent first.
    let writes = h.writes();
    let error = h
        .prepare_import(
            import_intent(token.clone(), 2, defaults(), vec![]),
            &h.watch(),
        )
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::NeedsSwitchConsent);
    assert_eq!(h.writes(), writes, "no switch without consent");
    let preview = h
        .prepare_import(import_intent(token, 2, defaults(), vec![2]), &h.watch())
        .unwrap();
    assert_eq!((preview.from, preview.into), (1, 2));
    assert!(!preview.nothing_to_write);
    assert!(preview.critical_fn_overrides.is_empty());
    assert!(!preview.critical_blocked);
    assert_eq!(preview.switches, [2]);
    let watch = h.watch();
    let report = h
        .apply_import(&preview.token.clone().unwrap(), &watch)
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(*watch.journal_seen.lock().unwrap(), Some(true));
    assert!(
        h.safety_files()
            .iter()
            .any(|p| p.to_string_lossy().contains("import"))
    );
    let w = |p: u8| h.with_sim(|s| s.persisted_trigger(profile(p), pos(46)).unwrap());
    assert_eq!(w(2), w(1), "profile 1's triggers in profile 2");
    assert_eq!(h.snapshot().profile, 1, "back on the original profile");
    // A reloaded window shows the report in import words.
    assert!(matches!(
        h.shared.last_report(),
        Some(crate::dto::LastReport::Import { .. })
    ));
    // An import token is not a restore token, and is single-use.
    let again = h
        .apply_restore(&preview.token.unwrap(), &h.watch())
        .unwrap_err();
    assert_eq!(again.kind, AppErrorKind::StalePlan);
    h.assert_clean();
}

/// Replaces the Fn-layer entries of `keys` on profile 1 (each a critical
/// built-in, acknowledged on the Keys path), exports profile 1 and returns
/// the file's token.
fn export_critical(h: &mut Harness, keys: &[u8]) -> FileToken {
    export_critical_with(h, keys, |_| {})
}

/// [`export_critical`], with `tweak` applied to the simulator before the
/// export (something else in profile 1 that an import can't take).
fn export_critical_with(
    h: &mut Harness,
    keys: &[u8],
    tweak: impl FnOnce(&mut openwraith_hid::DeviceSim),
) -> FileToken {
    let mut intent = h.keymap_intent(
        keys.iter()
            .zip(0x04u8..)
            .map(|(key, usage)| KeymapChangeIntent::Assign {
                layer: LayerView::Fn,
                key: *key,
                action: ActionValue::Key { usage },
            })
            .collect(),
    );
    intent.builtin_acks = keys
        .iter()
        .map(|key| BuiltinAckIntent {
            key: *key,
            critical: true,
        })
        .collect();
    let preview = h.prepare_keymap(intent).unwrap();
    h.apply_keymap(&preview.token.unwrap(), &h.watch()).unwrap();
    h.with_sim(tweak);
    h.refresh().unwrap();
    let (_, path) = export(h, "critical.owprofile.json", 1, false);
    crate::ops::share::import_summary(&h.shared, &path)
        .unwrap()
        .file
        .file
        .token
}

fn with_keys() -> ShareSelectionView {
    ShareSelectionView {
        keys: true,
        ..defaults()
    }
}

#[test]
fn a_critical_fn_override_needs_its_own_permission() {
    let mut h = Harness::ready();
    // Profile 1: Fn + 2 replaced (critical built-in, acknowledged).
    let token = export_critical(&mut h, &[TWO]);
    let refused = h
        .prepare_import(
            import_intent(token.clone(), 2, with_keys(), vec![2]),
            &h.watch(),
        )
        .unwrap();
    let indices = |p: &crate::dto::ImportPreview| {
        p.critical_fn_overrides
            .iter()
            .map(|k| k.index)
            .collect::<Vec<_>>()
    };
    assert_eq!(indices(&refused), [TWO]);
    assert!(refused.critical_blocked && refused.category_blocked);
    let mut allowed = import_intent(token, 2, with_keys(), vec![2]);
    allowed.allowed_critical_keys = vec![TWO];
    let preview = h.prepare_import(allowed, &h.watch()).unwrap();
    assert_eq!(indices(&preview), [TWO], "the callout stays after the tick");
    assert!(!preview.critical_blocked);
    assert!(!preview.category_blocked);
}

#[test]
fn every_critical_fn_override_is_named_and_each_needs_the_permission() {
    let mut h = Harness::ready();
    // Profile 1: Fn + 2 and Fn + Space (reset) replaced.
    let token = export_critical(&mut h, &[TWO, SPACE]);
    let indices = |p: &crate::dto::ImportPreview| {
        p.critical_fn_overrides
            .iter()
            .map(|k| k.index)
            .collect::<Vec<_>>()
    };
    let refused = h
        .prepare_import(
            import_intent(token.clone(), 2, with_keys(), vec![2]),
            &h.watch(),
        )
        .unwrap();
    assert_eq!(
        indices(&refused),
        [TWO, SPACE],
        "both keys, not the first only"
    );
    assert!(refused.critical_blocked);
    // A permission that names only Fn + 2 never imports Fn + Space.
    let writes = h.writes();
    let mut partial = import_intent(token.clone(), 2, with_keys(), vec![2]);
    partial.allowed_critical_keys = vec![TWO];
    let error = h.prepare_import(partial, &h.watch()).unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::PlanInvalidated {
            reason: crate::dto::PlanInvalidation::ValuesChanged
        })
    );
    // Both named: the keys are imported, and the preview still lists both.
    let mut allowed = import_intent(token, 2, with_keys(), vec![2]);
    allowed.allowed_critical_keys = vec![TWO, SPACE];
    let preview = h.prepare_import(allowed, &h.watch()).unwrap();
    assert_eq!(indices(&preview), [TWO, SPACE]);
    assert!(!preview.critical_blocked && !preview.category_blocked);
    let report = h.apply_import(&preview.token.unwrap(), &h.watch()).unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert!(h.writes() > writes);
    h.assert_clean();
}

/// Base W of profile 1 holds an entry OpenWraith does not write.
fn unwritable_base_w(s: &mut openwraith_hid::DeviceSim) {
    let mut base = s
        .table(profile(1), SimTable::Keymap(Layer::Base))
        .unwrap()
        .to_vec();
    let at = usize::from(W) * 4;
    base[at..at + 4].copy_from_slice(&[0x55, 0, 0, 0]);
    s.set_table(profile(1), SimTable::Keymap(Layer::Base), base);
}

/// The SOCD pairs of profile 1 have a gap: not in the vendor site's form.
fn unwritable_socd_pairs(s: &mut openwraith_hid::DeviceSim) {
    let mut pairs = vec![0u8; 80];
    pairs[4..8].copy_from_slice(&[0x01, 0x00, 0x04, 0x07]);
    s.set_table(profile(1), SimTable::SocdPairs, pairs);
}

fn critical_indices(p: &ImportPreview) -> Vec<u8> {
    p.critical_fn_overrides.iter().map(|k| k.index).collect()
}

/// The keymap group's skip cause in an import plan, if the keys are left out.
fn keys_cause(p: &ImportPreview) -> Option<BlockCauseView> {
    p.plan
        .profiles
        .iter()
        .flat_map(|profile| profile.items.iter())
        .find_map(|item| match &item.action {
            PlanActionView::Skip {
                reason: SkipReasonView::KeymapGroupBlocked { cause },
                ..
            } => Some(cause.clone()),
            _ => None,
        })
}

#[test]
fn keys_left_out_for_another_cause_name_no_critical_override() {
    let mut h = Harness::ready();
    // Profile 1: Fn + 2 replaced, and base W holds an entry OpenWraith does
    // not write, so the keys are never imported, permission or not.
    let token = export_critical_with(&mut h, &[TWO], unwritable_base_w);
    for allowed in [vec![], vec![TWO]] {
        let mut intent = import_intent(token.clone(), 3, with_keys(), vec![3]);
        intent.allowed_critical_keys = allowed.clone();
        let preview = h.prepare_import(intent, &h.watch()).unwrap();
        assert!(
            matches!(
                keys_cause(&preview),
                Some(BlockCauseView::UnwritableImportedEntry { index: W, .. })
            ),
            "{allowed:?}: {:?}",
            keys_cause(&preview)
        );
        assert!(
            critical_indices(&preview).is_empty(),
            "{allowed:?}: nothing is replaced, so nothing to allow"
        );
        assert!(!preview.critical_blocked, "{allowed:?}");
        assert!(preview.category_blocked, "{allowed:?}");
        assert!(preview.other_cause_blocked, "{allowed:?}: the general note");
    }
    h.assert_clean();
}

#[test]
fn a_category_blocked_beside_a_critical_override_keeps_its_own_note() {
    let mut h = Harness::ready();
    // Profile 1: Fn + 2 replaced, and SOCD pairs not in the site's form.
    let token = export_critical_with(&mut h, &[TWO], unwritable_socd_pairs);
    let include = ShareSelectionView {
        socd: true,
        ..with_keys()
    };
    let refused = h
        .prepare_import(
            import_intent(token.clone(), 3, include, vec![3]),
            &h.watch(),
        )
        .unwrap();
    assert_eq!(critical_indices(&refused), [TWO]);
    assert!(refused.critical_blocked && refused.category_blocked);
    assert!(refused.other_cause_blocked, "the SOCD pairs stay out too");
    let mut allowed = import_intent(token, 3, include, vec![3]);
    allowed.allowed_critical_keys = vec![TWO];
    let preview = h.prepare_import(allowed, &h.watch()).unwrap();
    assert_eq!(critical_indices(&preview), [TWO]);
    assert!(keys_cause(&preview).is_none(), "the keys are imported");
    assert!(!preview.critical_blocked);
    assert!(preview.category_blocked && preview.other_cause_blocked);
    h.assert_clean();
}

#[test]
fn import_intents_are_checked_before_any_traffic() {
    let mut h = Harness::ready();
    let (_, path) = export(&mut h, "p1.owprofile.json", 1, false);
    let token = crate::ops::share::import_summary(&h.shared, &path)
        .unwrap()
        .file
        .file
        .token;
    let before = h.packets();
    let nothing = ShareSelectionView {
        triggers: false,
        lighting: false,
        keys: false,
        dead_band: false,
        socd: false,
    };
    let error = h
        .prepare_import(import_intent(token.clone(), 1, nothing, vec![]), &h.watch())
        .unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::NoChanges
        })
    );
    let mut other = import_intent(token, 1, defaults(), vec![]);
    other.from = Some(3);
    let error = h.prepare_import(other, &h.watch()).unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::ProfileNotInBackup { profile: 3 })
    );
    assert_eq!(h.packets(), before);
}

#[test]
fn a_socd_push_invalidates_an_import_plan_only_when_it_writes_socd() {
    let mut h = Harness::ready();
    // Profile 1 is exported with an A/D pair, then the pair is removed.
    let pairs = |pair: bool| {
        let mut pairs = vec![0u8; 80];
        if pair {
            pairs[..4].copy_from_slice(&[0x01, 0x01, 68, 70]);
        }
        pairs
    };
    h.with_sim(|s| s.set_table(profile(1), SimTable::SocdPairs, pairs(true)));
    h.refresh().unwrap();
    let (_, path) = export(&mut h, "p1.owprofile.json", 1, false);
    let token = crate::ops::share::import_summary(&h.shared, &path)
        .unwrap()
        .file
        .file
        .token;
    h.with_sim(|s| {
        s.set_table(profile(1), SimTable::SocdPairs, pairs(false));
        s.set_trigger(
            profile(1),
            pos(W),
            [0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0],
        );
    });
    h.refresh().unwrap();
    let push = |h: &mut Harness| {
        h.events.clear();
        h.with_sim(|s| s.press_socd_hotkey_at(Duration::from_millis(10)));
        h.ticks(3);
        h.notices().contains(&Notice::PlanInvalidated {
            reason: PlanInvalidation::ValuesChanged,
        })
    };
    // Writing the pairs, the plan carries the keyboard's power as read.
    let with_socd = ShareSelectionView {
        socd: true,
        ..defaults()
    };
    let preview = h
        .prepare_import(
            import_intent(token.clone(), 1, with_socd, vec![]),
            &h.watch(),
        )
        .unwrap();
    assert!(push(&mut h), "{:?}", h.notices());
    let error = h
        .apply_import(&preview.token.unwrap(), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::StalePlan);
    // Without SOCD (the default) the plan does not depend on the power.
    let preview = h
        .prepare_import(import_intent(token, 1, defaults(), vec![]), &h.watch())
        .unwrap();
    assert!(!push(&mut h), "{:?}", h.notices());
    let report = h.apply_import(&preview.token.unwrap(), &h.watch()).unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    h.assert_clean();
}
