//! The colour, dead-band and SOCD editors and the trigger presets against
//! the simulator: views without traffic, plans, the drift check, the
//! journal, the refresh of the written table, SOCD pushes from the keyboard
//! (Fn + Right Shift) and refusals.

use std::time::Duration;

use openwraith_hid::protocol::{
    DeadBandData, Distance, HidUsage, SocdMode, SocdPair, SocdPairsData, SocdWrite, W75,
};
use openwraith_hid::{SimTable, sim::FACTORY_LIGHT};

use super::harness::{Harness, pos, profile};
use crate::dto::{
    AppErrorKind, ColorEditIntent, DeadBandEditIntent, DeadBandHintKind, EditFieldView,
    EditProblemView, EditWarningView, ErrorDetail, EvidenceView, Headline, KeyColorIntent,
    LightModeKey, Notice, PlanInvalidation, PresetEditIntent, PresetRef, PresetRtView, RtIntent,
    SocdEditIntent, SocdPairIntent, UserPresetIntent,
};

const W: u8 = 46;
const A: u8 = 68;
const S: u8 = 69;
const D: u8 = 70;
const Q: u8 = 45;
const KNOB: u8 = 15;

pub(super) fn colors(h: &Harness, colors: &[(u8, &str)], switch: bool) -> ColorEditIntent {
    ColorEditIntent {
        colors: colors
            .iter()
            .map(|(key, color)| KeyColorIntent {
                key: *key,
                color: (*color).to_string(),
            })
            .collect(),
        switch_to_custom: switch,
        check_persistence: false,
        based_on: h.colors().revision,
    }
}

fn custom1(h: &Harness) -> Vec<u8> {
    h.with_sim(|s| {
        s.persisted_table(profile(1), SimTable::CustomColors1)
            .unwrap()
            .to_vec()
    })
}

/// A ready keyboard whose light is not in the Custom mode.
fn ready_static_light() -> Harness {
    let mut h = Harness::new(1);
    h.with_sim(|s| s.set_table(profile(1), SimTable::MainLight, FACTORY_LIGHT.to_vec()));
    h.until_ready();
    h
}

// ----- colours -----

#[test]
fn the_colours_view_needs_no_traffic_and_keeps_slots_outside_the_layout() {
    let mut h = Harness::new(1);
    h.with_sim(|s| {
        let mut table = s
            .table(profile(1), SimTable::CustomColors1)
            .unwrap()
            .to_vec();
        table[103 * 3..103 * 3 + 3].copy_from_slice(&[1, 2, 3]);
        table[usize::from(W) * 3..usize::from(W) * 3 + 3].copy_from_slice(&[0xff, 0, 0]);
        s.set_table(profile(1), SimTable::CustomColors1, table);
    });
    h.until_ready();
    let before = h.packets();
    let view = h.colors();
    assert_eq!(h.packets(), before, "no traffic");
    assert!(view.editable);
    assert_eq!(
        view.keys.len(),
        W75.keys.len() - 1,
        "every key but the knob"
    );
    assert!(!view.keys.iter().any(|k| k.key.index == KNOB));
    let w = view.keys.iter().find(|k| k.key.index == W).unwrap();
    assert_eq!(w.color, "#ff0000");
    assert!(
        view.kept
            .iter()
            .any(|k| k.slot == 103 && k.color == "#010203")
    );
    assert!(view.shown_now, "the seed's light is Custom");
    assert_eq!(view.light_mode.map(|m| m.key), Some(LightModeKey::Custom));
    assert!(view.can_switch_to_custom);
}

#[test]
fn a_colour_edit_writes_only_the_keys_and_keeps_every_other_slot() {
    let mut h = Harness::ready();
    let before = custom1(&h);
    let preview = h
        .prepare_colors(colors(&h, &[(W, "#00ff00"), (A, "#0000ff")], false))
        .unwrap();
    assert_eq!(preview.changes.len(), 2);
    assert_eq!(preview.changes[0].after, "#00ff00");
    assert!(preview.shown_now);
    assert!(!preview.switches_to_custom);
    assert_eq!(preview.writes.len(), 1);
    assert!(!preview.persistence.will_run, "not pre-ticked");
    assert_eq!(h.writes(), 0, "prepare writes nothing");
    let revision = h.colors().revision;
    let watch = h.watch();
    let report = h
        .apply_colors(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(*watch.journal_seen.lock().unwrap(), Some(true));
    assert!(
        h.safety_files()
            .iter()
            .any(|p| p.to_string_lossy().contains("set-colors"))
    );
    let after = custom1(&h);
    for (slot, (old, new)) in before.chunks(3).zip(after.chunks(3)).enumerate() {
        match slot {
            46 => assert_eq!(new, [0, 0xff, 0]),
            68 => assert_eq!(new, [0, 0, 0xff]),
            _ => assert_eq!(old, new, "slot {slot} kept"),
        }
    }
    let view = h.colors();
    assert_ne!(view.revision, revision, "the cache was refreshed");
    assert_eq!(
        view.keys.iter().find(|k| k.key.index == W).unwrap().color,
        "#00ff00"
    );
    // The token is single-use.
    let again = h
        .apply_colors(&Harness::token(&preview.token), &h.watch())
        .unwrap_err();
    assert_eq!(again.kind, AppErrorKind::StalePlan);
    h.assert_clean();
}

#[test]
fn switching_to_custom_is_opt_in() {
    let mut h = ready_static_light();
    assert!(!h.colors().shown_now);
    let preview = h
        .prepare_colors(colors(&h, &[(W, "#00ff00")], false))
        .unwrap();
    assert!(
        preview
            .warnings
            .contains(&EditWarningView::NotShownUntilCustomMode)
    );
    assert_eq!(preview.writes.len(), 1);
    let preview = h
        .prepare_colors(colors(&h, &[(W, "#00ff00")], true))
        .unwrap();
    assert!(preview.switches_to_custom);
    assert_eq!(preview.writes.len(), 2);
    let report = h
        .apply_colors(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    let light = h.with_sim(|s| {
        s.persisted_table(profile(1), SimTable::MainLight)
            .unwrap()
            .to_vec()
    });
    assert_eq!(light[0], 0x0A, "the Custom mode");
    assert_eq!(
        h.snapshot().light.current.unwrap().mode.key,
        LightModeKey::Custom,
        "the light was read again"
    );
    assert!(h.colors().shown_now);
    h.assert_clean();
}

#[test]
fn colour_intents_are_checked_before_any_traffic() {
    let mut h = Harness::ready();
    let before = h.packets();
    for (intent, problem) in [
        (
            colors(&h, &[(KNOB, "#ffffff")], false),
            EditProblemView::Knob,
        ),
        (
            colors(&h, &[(W, "red")], false),
            EditProblemView::InvalidColor,
        ),
        (
            colors(&h, &[(W, "#ff00001")], false),
            EditProblemView::InvalidColor,
        ),
        (colors(&h, &[], false), EditProblemView::NoChanges),
        (
            colors(&h, &[(W, "#ff0000"), (W, "#00ff00")], false),
            EditProblemView::DuplicateKey {
                key: crate::map::key_ref_index(W),
            },
        ),
    ] {
        let error = h.prepare_colors(intent).unwrap_err();
        assert_eq!(error.detail, Some(ErrorDetail::Edit { problem }));
    }
    let mut stale = colors(&h, &[(W, "#ff0000")], false);
    stale.based_on += 1;
    let error = h.prepare_colors(stale).unwrap_err();
    assert_eq!(error.kind, AppErrorKind::StalePlan);
    assert_eq!(h.packets(), before);
}

#[test]
fn the_colour_drift_check_refuses_a_table_changed_since_the_plan() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_colors(colors(&h, &[(W, "#00ff00")], false))
        .unwrap();
    h.with_sim(|s| {
        let mut table = s
            .table(profile(1), SimTable::CustomColors1)
            .unwrap()
            .to_vec();
        table[usize::from(W) * 3] ^= 0x11;
        s.set_table(profile(1), SimTable::CustomColors1, table);
    });
    let writes = h.writes();
    let error = h
        .apply_colors(&Harness::token(&preview.token), &h.watch())
        .unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::PlanInvalidated {
            reason: PlanInvalidation::ValuesChanged
        })
    );
    assert_eq!(h.writes(), writes, "nothing written");
    assert!(h.safety_files().is_empty(), "nothing saved");
}

// ----- dead band -----

pub(super) fn dead_band(
    h: &Harness,
    keys: &[u8],
    top: Option<u16>,
    bottom: Option<u16>,
) -> DeadBandEditIntent {
    DeadBandEditIntent {
        keys: keys.to_vec(),
        top,
        bottom,
        check_persistence: false,
        based_on: h.dead_band().revision,
    }
}

#[test]
fn the_dead_band_view_and_its_limit_follow_the_library() {
    let h = Harness::ready();
    let view = h.dead_band();
    assert!(view.editable, "{view:?}");
    assert_eq!(view.unit_um, Some(20));
    // The library's own maximum (the slider's 1.00 mm, the W75 ceiling).
    assert_eq!(
        view.max,
        DeadBandData::max_units(20).min(W75.write_ceilings.max_dead_band)
    );
    assert_eq!(view.max, 50);
    assert_eq!(view.keys.len(), W75.keys.len() - 1);
}

#[test]
fn a_dead_band_edit_writes_its_table_and_reads_it_again() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_dead_band(dead_band(&h, &[W, A], Some(5), Some(10)))
        .unwrap();
    assert_eq!(preview.changes.len(), 2);
    assert_eq!(preview.changes[0].after.top, 5);
    assert_eq!(preview.writes.len(), 1);
    let watch = h.watch();
    let report = h
        .apply_dead_band(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(*watch.journal_seen.lock().unwrap(), Some(true));
    let table = h.with_sim(|s| {
        s.persisted_table(profile(1), SimTable::DeadBand)
            .unwrap()
            .to_vec()
    });
    assert_eq!(table[usize::from(W) * 2..usize::from(W) * 2 + 2], [5, 10]);
    assert_eq!(table[usize::from(A) * 2..usize::from(A) * 2 + 2], [5, 10]);
    let w = h
        .dead_band()
        .keys
        .into_iter()
        .find(|k| k.key.index == W)
        .unwrap();
    assert_eq!((w.value.top, w.value.bottom), (5, 10));
    h.assert_clean();
}

#[test]
fn dead_band_values_out_of_range_are_refused_not_clamped() {
    let mut h = Harness::ready();
    let before = h.packets();
    let error = h
        .prepare_dead_band(dead_band(&h, &[W], Some(51), None))
        .unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::OutOfRange {
                field: EditFieldView::Top,
                value: 51,
                min: 0,
                max: 50
            }
        })
    );
    let error = h
        .prepare_dead_band(dead_band(&h, &[KNOB], Some(1), None))
        .unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::Knob
        })
    );
    let error = h
        .prepare_dead_band(dead_band(&h, &[W], None, None))
        .unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::NoChanges
        })
    );
    assert_eq!(h.packets(), before);
}

#[test]
fn dead_zones_that_reach_the_actuation_point_are_hinted() {
    let mut h = Harness::ready();
    // W actuates at 1.00 mm (50 units) in the seed.
    let actuation = h
        .snapshot()
        .triggers
        .keys
        .iter()
        .find(|k| k.key.index == W)
        .and_then(|k| k.value)
        .unwrap()
        .actuation;
    let top = actuation.min(50);
    let preview = h
        .prepare_dead_band(dead_band(&h, &[W], Some(top), None))
        .unwrap();
    assert!(
        preview
            .hints
            .iter()
            .any(|hint| hint.kind == DeadBandHintKind::TopReachesActuation)
    );
}

#[test]
fn the_persistence_check_of_an_editor_is_opt_in_and_runs_when_asked() {
    let mut h = Harness::ready();
    let mut intent = dead_band(&h, &[W], Some(3), None);
    intent.check_persistence = true;
    let preview = h.prepare_dead_band(intent).unwrap();
    assert!(preview.persistence.requested && preview.persistence.will_run);
    assert_eq!(preview.persistence.away_profile, 2);
    let report = h
        .apply_dead_band(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(report.persistence.unwrap().persisted, 1);
    assert_eq!(h.selects(), [2, 1], "away and back");
    h.assert_clean();
}

// ----- SOCD -----

pub(super) fn socd(
    h: &Harness,
    power: Option<bool>,
    pairs: Option<&[(u8, u8, u8)]>,
) -> SocdEditIntent {
    SocdEditIntent {
        power,
        pairs: pairs.map(|p| {
            p.iter()
                .map(|&(key1, key2, mode)| SocdPairIntent { key1, key2, mode })
                .collect()
        }),
        allow_not_undoable: false,
        check_persistence: false,
        based_on: h.socd().revision,
    }
}

#[test]
fn the_socd_mode_evidence_follows_the_library() {
    // The view mirrors `SocdWrite::evidence` (app code may not name it).
    for mode in SocdMode::ALL {
        let pair = SocdPair {
            mode,
            key1: HidUsage::new(0x04).unwrap(),
            key2: HidUsage::new(0x07).unwrap(),
        };
        let notes =
            SocdWrite::new(true, SocdPairsData::from_pairs(&W75, &[pair]).unwrap()).evidence();
        let view = crate::map::editors::socd_mode_evidence(mode);
        match view {
            EvidenceView::Confirmed => assert!(notes.is_empty(), "{mode:?}"),
            EvidenceView::Unverified { notes: view_notes } => {
                assert_eq!(view_notes.len(), notes.len(), "{mode:?}");
                for (v, n) in view_notes.iter().zip(&notes) {
                    assert_eq!((v.what.as_str(), v.question.as_str()), (n.what, n.question));
                }
            }
        }
    }
}

#[test]
fn the_socd_view_shows_the_power_the_pairs_and_the_modes() {
    let h = Harness::ready();
    let view = h.socd();
    assert!(view.editable, "{view:?}");
    assert!(!view.power);
    assert!(view.pairs.is_empty());
    assert!(view.pairs_writable);
    assert_eq!(view.max_pairs, 20);
    assert_eq!(view.modes.len(), 4);
    assert!(view.modes.iter().all(|m| matches!(
        (m.mode, &m.evidence),
        (1 | 4, EvidenceView::Confirmed) | (2 | 3, EvidenceView::Unverified { .. })
    )));
    assert!(
        view.keys
            .iter()
            .any(|k| k.usage == 0x04 && k.key.as_ref().is_some_and(|k| k.index == A))
    );
}

#[test]
fn a_socd_edit_writes_the_burst_and_warns_about_the_whole_keyboard() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_socd(socd(&h, Some(true), Some(&[(A, D, 1), (W, S, 2)])))
        .unwrap();
    assert!(!preview.power_before && preview.power_after);
    assert_eq!(preview.pairs_after.len(), 2);
    assert_eq!(preview.pairs_after[1].mode, 2);
    assert!(
        preview
            .warnings
            .contains(&EditWarningView::SocdPowerAllProfiles)
    );
    assert!(preview.warnings.contains(&EditWarningView::SocdHotkey));
    assert!(
        preview
            .warnings
            .iter()
            .any(|w| matches!(w, EditWarningView::Unverified { note } if note.question == "Q35")),
        "mode 2 is not yet verified"
    );
    let watch = h.watch();
    let report = h
        .apply_socd(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    assert_eq!(
        *watch.journal_seen.lock().unwrap(),
        Some(true),
        "the journal names the SOCD edit while it writes"
    );
    let view = h.socd();
    assert!(view.power);
    assert_eq!(view.pairs.len(), 2);
    assert_eq!(view.pairs[0].key1.key.as_ref().map(|k| k.index), Some(A));
    let pairs = h.with_sim(|s| {
        s.persisted_table(profile(1), SimTable::SocdPairs)
            .unwrap()
            .to_vec()
    });
    assert_eq!(pairs[..8], [0x01, 0x00, 0x04, 0x07, 0x01, 0x01, 0x1a, 0x16]);
    h.assert_clean();
}

#[test]
fn socd_pairs_follow_the_site_rules() {
    let mut h = Harness::ready();
    let before = h.packets();
    let same = h
        .prepare_socd(socd(&h, None, Some(&[(A, A, 1)])))
        .unwrap_err();
    assert!(matches!(
        same.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::SocdSameKey { .. }
        })
    ));
    let twice = h
        .prepare_socd(socd(&h, None, Some(&[(A, D, 1), (D, W, 1)])))
        .unwrap_err();
    assert!(matches!(
        twice.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::SocdDuplicateKey { .. }
        })
    ));
    let mode = h
        .prepare_socd(socd(&h, None, Some(&[(A, D, 5)])))
        .unwrap_err();
    assert!(matches!(
        mode.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::OutOfRange {
                field: EditFieldView::SocdMode,
                ..
            }
        })
    ));
    let knob = h
        .prepare_socd(socd(&h, None, Some(&[(KNOB, D, 1)])))
        .unwrap_err();
    assert!(matches!(
        knob.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::Knob
        })
    ));
    let nothing = h.prepare_socd(socd(&h, None, None)).unwrap_err();
    assert!(matches!(
        nothing.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::NoChanges
        })
    ));
    assert_eq!(h.packets(), before);
}

#[test]
fn the_keyboards_socd_push_updates_the_view_and_invalidates_only_a_socd_plan() {
    let mut h = Harness::ready();
    let revision = h.socd().revision;
    // A pending SOCD plan is invalidated by the push.
    let preview = h.prepare_socd(socd(&h, None, Some(&[(A, D, 1)]))).unwrap();
    let before = h.packets();
    h.with_sim(|s| s.press_socd_hotkey_at(Duration::from_millis(10)));
    h.ticks(3);
    assert_eq!(h.packets(), before, "a push is never answered");
    assert!(
        h.notices()
            .contains(&Notice::SocdToggledOnKeyboard { power: true }),
        "{:?}",
        h.notices()
    );
    assert!(h.notices().contains(&Notice::PlanInvalidated {
        reason: PlanInvalidation::ValuesChanged
    }));
    let view = h.socd();
    assert!(view.power);
    assert_ne!(view.revision, revision);
    let error = h
        .apply_socd(&Harness::token(&preview.token), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::StalePlan);
    // A trigger plan pending at a push stays valid.
    let trigger = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    h.events.clear();
    h.with_sim(|s| s.press_socd_hotkey_at(Duration::from_millis(10)));
    h.ticks(3);
    assert!(
        h.notices()
            .contains(&Notice::SocdToggledOnKeyboard { power: false })
    );
    assert!(
        !h.notices()
            .iter()
            .any(|n| matches!(n, Notice::PlanInvalidated { .. })),
        "{:?}",
        h.notices()
    );
    let report = h
        .apply_trigger(&Harness::token(&trigger.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done);
    assert!(
        h.worker.conn == crate::worker::Conn::Ready,
        "a push is never another program"
    );
    h.assert_clean();
}

#[test]
fn a_socd_switch_on_the_keyboard_after_the_plan_stops_the_write() {
    let mut h = Harness::ready();
    let preview = h.prepare_socd(socd(&h, Some(true), None)).unwrap();
    // Toggled on the keyboard while the dialog is open, but the worker has
    // not listened yet: the apply's drift check sees the new power.
    h.with_sim(|s| s.press_socd_hotkey_at(Duration::ZERO));
    let writes = h.writes();
    let error = h
        .apply_socd(&Harness::token(&preview.token), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::StalePlan);
    assert_eq!(h.writes(), writes, "nothing written");
}

// ----- presets -----

#[test]
fn presets_list_the_library_table_and_the_users() {
    let h = Harness::ready();
    let limits = h.snapshot().triggers.limits;
    let view = crate::ops::presets::list(&h.worker.config, limits).unwrap();
    let ids: Vec<&str> = view.builtin.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        ["typing", "balanced", "fast", "competitive", "factory"]
    );
    let factory = view.builtin.iter().find(|p| p.id == "factory").unwrap();
    assert_eq!(factory.actuation, 85);
    assert_eq!(
        factory.rapid_trigger,
        PresetRtView::OffWith {
            press: 1,
            release: 1
        }
    );
    assert!(view.user.is_empty());
    // Saving, replacing and deleting a user preset.
    let saved = crate::ops::presets::save(
        &h.worker.config,
        &UserPresetIntent {
            id: None,
            name: "  Mine\u{7}  ".to_string(),
            actuation: 45,
            rapid_trigger: RtIntent::Same { sensitivity: 10 },
        },
        limits,
    )
    .unwrap();
    assert_eq!(saved.user.len(), 1);
    assert_eq!(saved.user[0].name, "Mine");
    assert!(saved.user[0].refused.is_none());
    let id = saved.user[0].id.clone();
    let replaced = crate::ops::presets::save(
        &h.worker.config,
        &UserPresetIntent {
            id: Some(id.clone()),
            name: "Renamed".to_string(),
            actuation: 46,
            rapid_trigger: RtIntent::Off,
        },
        limits,
    )
    .unwrap();
    assert_eq!(replaced.user.len(), 1);
    assert_eq!(replaced.user[0].actuation, 46);
    let deleted = crate::ops::presets::delete(&h.worker.config, &id, limits).unwrap();
    assert!(deleted.user.is_empty());
    // The settings file keeps the language and theme.
    let path = h.worker.config.settings_path.clone().unwrap();
    assert_eq!(crate::load_settings(&path), crate::dto::Settings::default());
}

#[test]
fn presets_outside_the_limits_are_refused_not_clamped() {
    let mut h = Harness::ready();
    let limits = h.snapshot().triggers.limits.unwrap();
    let error = crate::ops::presets::save(
        &h.worker.config,
        &UserPresetIntent {
            id: None,
            name: "Too deep".to_string(),
            actuation: limits.max_actuation + 1,
            rapid_trigger: RtIntent::Off,
        },
        Some(limits),
    )
    .unwrap_err();
    assert!(matches!(
        error.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::OutOfRange {
                field: EditFieldView::Actuation,
                ..
            }
        })
    ));
    // A preset written by hand (or for another keyboard) is listed as
    // refused and cannot be applied.
    let path = h.worker.config.settings_path.clone().unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{"language":"system","theme":"system","presets":[{"id":"u-7","name":"Hand","actuation":9999,"rapid_trigger":{"mode":"off"}}]}"#,
    )
    .unwrap();
    let view = crate::ops::presets::list(&h.worker.config, Some(limits)).unwrap();
    assert!(view.user[0].refused.is_some());
    let before = h.packets();
    let error = h
        .prepare_preset(PresetEditIntent {
            preset: PresetRef::User {
                id: "u-7".to_string(),
            },
            keys: vec![W],
            based_on: h.snapshot().revision,
        })
        .unwrap_err();
    assert!(matches!(
        error.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::OutOfRange { .. }
        })
    ));
    let unknown = h
        .prepare_preset(PresetEditIntent {
            preset: PresetRef::Builtin {
                id: "turbo".to_string(),
            },
            keys: vec![W],
            based_on: h.snapshot().revision,
        })
        .unwrap_err();
    assert!(matches!(
        unknown.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::UnknownPreset { .. }
        })
    ));
    assert_eq!(h.packets(), before);
}

#[test]
fn a_preset_is_applied_as_a_trigger_edit() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_preset(PresetEditIntent {
            preset: PresetRef::Builtin {
                id: "factory".to_string(),
            },
            keys: vec![W, Q],
            based_on: h.snapshot().revision,
        })
        .unwrap();
    assert_eq!(preview.keys.len(), 2);
    // The trigger edit's own apply writes it.
    let report = h
        .apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    let record = h.with_sim(|s| s.persisted_trigger(profile(1), pos(W)).unwrap());
    // The factory record `00 55 55 01 01` exactly.
    assert_eq!(record[..5], [0x00, 0x55, 0x55, 0x01, 0x01]);
    assert_eq!(
        Distance::from_units(u16::from(record[1])),
        openwraith_hid::trigger_preset("factory").unwrap().actuation
    );
    h.assert_clean();
}

#[test]
fn stop_during_the_safety_read_writes_and_saves_nothing() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_socd(socd(&h, Some(true), Some(&[(A, D, 4)])))
        .unwrap();
    let watch = h.watch();
    watch.cancel_when(|e| {
        matches!(
            e.step,
            crate::dto::ProgressStep::Reading {
                purpose: crate::dto::ReadPurpose::SafetyBackup,
                ..
            }
        )
    });
    let writes = h.writes();
    let error = h
        .apply_socd(&Harness::token(&preview.token), &watch)
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::Cancelled);
    assert_eq!(h.writes(), writes);
    assert!(h.safety_files().is_empty());
    assert!(!h.worker.config.journal_path().exists());
}

// ----- drift checks and own revisions -----

#[test]
fn the_dead_band_drift_check_refuses_a_table_changed_since_the_plan() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_dead_band(dead_band(&h, &[W], Some(5), Some(10)))
        .unwrap();
    h.with_sim(|s| {
        let mut table = s.table(profile(1), SimTable::DeadBand).unwrap().to_vec();
        table[usize::from(W) * 2] = 7;
        s.set_table(profile(1), SimTable::DeadBand, table);
    });
    let writes = h.writes();
    let error = h
        .apply_dead_band(&Harness::token(&preview.token), &h.watch())
        .unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::PlanInvalidated {
            reason: PlanInvalidation::ValuesChanged
        })
    );
    assert_eq!(h.writes(), writes, "nothing written");
    assert!(h.safety_files().is_empty(), "nothing saved");
}

#[test]
fn the_socd_drift_check_covers_the_pairs_too() {
    let mut h = Harness::ready();
    let preview = h.prepare_socd(socd(&h, Some(true), None)).unwrap();
    h.with_sim(|s| {
        let mut table = s.table(profile(1), SimTable::SocdPairs).unwrap().to_vec();
        table[..4].copy_from_slice(&[0x01, 0x00, 0x04, 0x07]);
        s.set_table(profile(1), SimTable::SocdPairs, table);
    });
    let writes = h.writes();
    let error = h
        .apply_socd(&Harness::token(&preview.token), &h.watch())
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::StalePlan, "{error:?}");
    assert_eq!(h.writes(), writes, "nothing written");
    assert!(h.safety_files().is_empty(), "nothing saved");
}

#[test]
fn each_editor_view_keeps_its_own_revision_across_the_others_writes() {
    let mut h = Harness::ready();
    // A dead-band write: the colour and SOCD drafts stay valid.
    let colours = h.colors().revision;
    let socd_rev = h.socd().revision;
    let colour_draft = colors(&h, &[(W, "#00ff00")], false);
    let socd_draft = socd(&h, Some(true), None);
    let preview = h
        .prepare_dead_band(dead_band(&h, &[W], Some(5), None))
        .unwrap();
    h.apply_dead_band(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(h.colors().revision, colours);
    assert_eq!(h.socd().revision, socd_rev);
    h.prepare_colors(colour_draft).unwrap();
    h.prepare_socd(socd_draft).unwrap();
    // A SOCD write: the colour and dead-band drafts stay valid.
    let colours = h.colors().revision;
    let dead = h.dead_band().revision;
    let colour_draft = colors(&h, &[(W, "#0000ff")], false);
    let dead_draft = dead_band(&h, &[W], Some(6), None);
    let preview = h.prepare_socd(socd(&h, Some(true), None)).unwrap();
    h.apply_socd(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(h.colors().revision, colours);
    assert_eq!(h.dead_band().revision, dead);
    h.prepare_colors(colour_draft).unwrap();
    h.prepare_dead_band(dead_draft).unwrap();
    // A colour write: the dead-band and SOCD drafts stay valid.
    let dead = h.dead_band().revision;
    let socd_rev = h.socd().revision;
    let dead_draft = dead_band(&h, &[W], Some(7), None);
    let socd_draft = socd(&h, Some(false), None);
    let preview = h
        .prepare_colors(colors(&h, &[(W, "#ff00ff")], false))
        .unwrap();
    h.apply_colors(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(h.dead_band().revision, dead);
    assert_eq!(h.socd().revision, socd_rev);
    h.prepare_dead_band(dead_draft).unwrap();
    h.prepare_socd(socd_draft).unwrap();
}

#[test]
fn preset_names_and_the_number_of_presets_are_limited() {
    let h = Harness::ready();
    let limits = h.snapshot().triggers.limits.unwrap();
    let save = |name: String| {
        crate::ops::presets::save(
            &h.worker.config,
            &UserPresetIntent {
                id: None,
                name,
                actuation: 60,
                rapid_trigger: RtIntent::Off,
            },
            Some(limits),
        )
    };
    let settings = || std::fs::read_to_string(h.worker.config.settings_path.clone().unwrap()).ok();
    // 41 characters: refused, nothing stored; 40 are fine.
    let error = save("x".repeat(41)).unwrap_err();
    assert!(matches!(
        error.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::PresetName
        })
    ));
    assert_eq!(settings(), None, "settings.json untouched");
    save("y".repeat(40)).unwrap();
    // 32 presets at most.
    for n in 2..=32 {
        save(format!("Preset {n}")).unwrap();
    }
    let before = settings();
    let error = save("One too many".to_string()).unwrap_err();
    assert!(matches!(
        error.detail,
        Some(ErrorDetail::Edit {
            problem: EditProblemView::TooManyPresets { max: 32 }
        })
    ));
    assert_eq!(settings(), before, "settings.json untouched");
    let view = crate::ops::presets::list(&h.worker.config, Some(limits)).unwrap();
    assert_eq!(view.user.len(), 32);
}
