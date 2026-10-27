//! Main-light edits.

use openwraith_hid::SimTable;

use super::harness::{Harness, profile};
use crate::dto::{
    AppErrorKind, CancelState, ErrorDetail, Headline, IntentField, LightModeKey, ProgressStep,
};

#[test]
fn a_mode_change_sends_the_leave_and_the_light_and_persists() {
    let mut h = Harness::ready();
    let light = h.snapshot().light;
    assert_eq!(
        light.current.as_ref().unwrap().mode.key,
        LightModeKey::Custom
    );
    assert!(light.modes.iter().any(|m| m.key == LightModeKey::Static));
    assert!(light.modes.iter().any(|m| m.key == LightModeKey::Custom));
    assert!(
        !light
            .modes
            .iter()
            .any(|m| m.key == LightModeKey::MusicRhythm)
    );
    let preview = h
        .prepare_light(h.light_intent(Some(0x00), Some("#0000ff"), None, None))
        .unwrap();
    assert!(preview.changes_mode);
    assert_eq!(preview.packets, 2);
    assert_eq!(preview.target.mode.key, LightModeKey::Static);
    assert_eq!(preview.target.color, "#0000ff");
    assert_eq!(h.writes(), 0);
    let watch = h.watch();
    let report = h
        .apply_light(&Harness::token(&preview.token), &watch)
        .unwrap();
    assert_eq!(report.headline, Headline::Done, "{report:?}");
    let writes = h.write_bytes();
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0][0], 0x17, "the music-rhythm leave first");
    assert_eq!(writes[1][..2], [0x07, 0x00]);
    let saved = h.with_sim(|s| {
        s.persisted_table(profile(1), SimTable::MainLight)
            .map(<[u8]>::to_vec)
    });
    let saved = saved.unwrap();
    assert_eq!(saved[0], 0x00);
    assert_eq!(saved[3..6], [0, 0, 0xff]);
    // Not cancellable once the safety backup is read.
    let writing: Vec<_> = watch
        .all()
        .into_iter()
        .filter(|e| matches!(e.step, ProgressStep::Writing { .. }))
        .collect();
    assert!(!writing.is_empty());
    assert!(
        writing
            .iter()
            .all(|e| e.cancel == CancelState::NotCancellable)
    );
    let after = h.snapshot().light.current.unwrap();
    assert_eq!(after.mode.key, LightModeKey::Static);
    assert_eq!(after.color, "#0000ff");
    h.assert_clean();
}

#[test]
fn a_brightness_change_sends_only_the_light() {
    let mut h = Harness::ready();
    let preview = h
        .prepare_light(h.light_intent(None, None, Some(2), None))
        .unwrap();
    assert!(!preview.changes_mode);
    assert_eq!(preview.packets, 1);
    let report = h
        .apply_light(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert_eq!(report.headline, Headline::Done);
    let writes = h.write_bytes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0][..2], [0x07, 0x00]);
    assert_eq!(h.snapshot().light.current.unwrap().brightness, 2);
    h.assert_clean();
}

#[test]
fn invalid_light_intents_are_refused() {
    let mut h = Harness::ready();
    let before = h.packets();
    let error = h
        .prepare_light(h.light_intent(Some(0x64), None, None, None))
        .unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::InvalidIntent {
            field: IntentField::Mode
        })
    );
    let error = h
        .prepare_light(h.light_intent(None, Some("blue"), None, None))
        .unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::InvalidIntent {
            field: IntentField::Color
        })
    );
    let error = h
        .prepare_light(h.light_intent(None, None, Some(5), None))
        .unwrap_err();
    assert_eq!(error.kind, AppErrorKind::InvalidRequest);
    let error = h
        .prepare_light(h.light_intent(None, None, None, None))
        .unwrap_err();
    assert_eq!(
        error.detail,
        Some(ErrorDetail::InvalidIntent {
            field: IntentField::NothingToChange
        })
    );
    assert_eq!(h.packets(), before);
}
