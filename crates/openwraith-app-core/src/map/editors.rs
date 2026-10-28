//! The views of the colour, dead-band and SOCD editors, their plans'
//! writes and warnings, and their refusals.

use openwraith_hid::protocol::{
    Distance, Model, ProfileId, Rgb, SocdMode, SocdPair, WriteBuildError,
};
use openwraith_hid::{EditError, EditWarning, PersistenceCheckMode, PlannedEditWrite};

use super::error::app_kind;
use super::keys::{away_profile, note_view};
use super::{clean, hex, key_ref_index, sat16, table_ref};
use crate::dto::{
    AppError, AppErrorKind, ColorsDisabled, ColorsState, DeadBandDisabled, DeadBandState,
    DeadBandValue, DeviceSnapshot, EditFieldView, EditProblemView, EditWarningView, EditWriteView,
    ErrorDetail, EvidenceView, KeptColorView, KeyColorView, KeyDeadBandView, PersistencePlanView,
    SocdDisabled, SocdKeyView, SocdModeView, SocdPairView, SocdStateView, UnverifiedNoteView,
};
use crate::ops::editors::{EditorsCache, TableRead};

/// The Custom light mode, which shows custom colour table 1.
pub(crate) const CUSTOM_MODE: u8 = 0x0A;

/// The trigger unit OpenWraith writes (0.02 mm).
const UNIT_UM: u32 = Distance::MICROMETRES_PER_UNIT;

// ----- refusals -----

/// A refused edit (nothing was written).
pub(crate) fn refused(problem: EditProblemView, message: impl Into<String>) -> AppError {
    let message = message.into();
    AppError::with(
        AppErrorKind::InvalidRequest,
        ErrorDetail::Edit { problem },
        format!("{message}; nothing was written"),
    )
}

/// The key of a HID usage on the W75 layout, as a reference (`#0` when no
/// key has it).
fn usage_key(model: &Model, usage: u8) -> crate::dto::KeyRef {
    let index = model
        .keys
        .iter()
        .find(|k| k.hid_usage == Some(usage) && Some(k.index) != model.knob_index)
        .map_or(0, |k| k.index);
    key_ref_index(index)
}

/// A library refusal of a colour, dead-band or SOCD edit.
pub(crate) fn edit_error(error: &EditError, model: &Model) -> AppError {
    let with = |problem: EditProblemView| AppError {
        kind: app_kind(error.kind()),
        detail: Some(ErrorDetail::Edit { problem }),
        message: format!("{error}; nothing was written"),
    };
    match error {
        EditError::Invalid { reason } => with(EditProblemView::Other {
            message: clean(reason),
        }),
        EditError::UnsupportedCurrentTable { table, reason } => {
            with(EditProblemView::UnsupportedCurrentTable {
                table: table_ref(*table),
                reason: clean(reason),
            })
        }
        EditError::DistanceUnit { unit_um } => AppError::with(
            app_kind(error.kind()),
            ErrorDetail::DistanceUnit { unit_um: *unit_um },
            format!("{error}; nothing was written"),
        ),
        EditError::Build(build) => with(build_problem(build, model)),
        EditError::Placement(_) | EditError::NoFreeSlot { .. } => with(EditProblemView::Other {
            message: clean(&error.to_string()),
        }),
        EditError::SlotAlreadyBound { .. }
        | EditError::BindingToEmptySlot { .. }
        | EditError::SlotStillReferenced { .. } => with(EditProblemView::Other {
            message: clean(&error.to_string()),
        }),
        EditError::Session(e) => AppError::from(e),
        EditError::Backup(e) => AppError::from(e),
        EditError::Restore(e) => AppError::from(e),
    }
}

/// A value the library could not build.
fn build_problem(error: &WriteBuildError, model: &Model) -> EditProblemView {
    match error {
        WriteBuildError::KeyNotColourable { index } => {
            if model.knob_index == Some(*index) {
                EditProblemView::Knob
            } else {
                EditProblemView::NotAKey { index: *index }
            }
        }
        WriteBuildError::DeadBandOutOfRange { which, value, .. } => EditProblemView::OutOfRange {
            field: if *which == "top" {
                EditFieldView::Top
            } else {
                EditFieldView::Bottom
            },
            value: u16::from(*value),
            min: 0,
            max: model.write_ceilings.max_dead_band,
        },
        WriteBuildError::SocdSameKey { usage } => EditProblemView::SocdSameKey {
            key: usage_key(model, *usage),
        },
        WriteBuildError::SocdDuplicateKey { usage } => EditProblemView::SocdDuplicateKey {
            key: usage_key(model, *usage),
        },
        WriteBuildError::TooManySocdPairs { .. } => EditProblemView::TooManyKeys {
            max: u16::from(model.write_ceilings.socd_max_pairs),
        },
        other => EditProblemView::Other {
            message: clean(&other.to_string()),
        },
    }
}

// ----- plans -----

/// One planned write.
pub(crate) fn write_view(write: &PlannedEditWrite) -> EditWriteView {
    EditWriteView {
        table: table_ref(write.table),
        packets: sat16(write.packets),
        summary: clean(&write.summary),
    }
}

/// A plan warning of these editors.
pub(crate) fn warning_view(warning: &EditWarning) -> EditWarningView {
    match warning {
        EditWarning::NotShownUntilCustomMode => EditWarningView::NotShownUntilCustomMode,
        EditWarning::SocdPowerAllProfiles => EditWarningView::SocdPowerAllProfiles,
        EditWarning::SocdHotkey => EditWarningView::SocdHotkey,
        EditWarning::Unverified(note) => EditWarningView::Unverified {
            note: note_view(note),
        },
        EditWarning::NotUndoable { table } => EditWarningView::NotUndoable {
            table: table_ref(*table),
        },
        other => EditWarningView::Other {
            message: format!("{other:?}"),
        },
    }
}

/// The persistence check of a plan (never forced for these editors).
pub(crate) fn persistence_plan(
    writes: &[PlannedEditWrite],
    mode: PersistenceCheckMode,
    requested: bool,
    profile: ProfileId,
) -> PersistencePlanView {
    let tables = openwraith_hid::persistence_check_tables(writes, mode);
    PersistencePlanView {
        requested,
        forced: false,
        will_run: !tables.is_empty(),
        tables: tables.into_iter().map(table_ref).collect(),
        away_profile: away_profile(profile),
    }
}

/// `"#rrggbb"` lowercase.
pub(crate) fn rgb_text(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)
}

// ----- key colours -----

/// The colours view of the cache; the light mode comes from the current
/// snapshot (a light edit patches it).
pub(crate) fn colors_state(cache: &EditorsCache, snapshot: Option<&DeviceSnapshot>) -> ColorsState {
    let light = snapshot.map(|s| &s.light);
    let mode = light.and_then(|l| l.current.as_ref()).map(|c| c.mode);
    let shown_now = mode.is_some_and(|m| m.id == CUSTOM_MODE);
    let can_switch_to_custom =
        light.is_some_and(|l| l.editable && l.modes.iter().any(|m| m.id == CUSTOM_MODE));
    let (disabled, keys, kept) = match (cache.model, &cache.colors.data.custom1) {
        (None, _) => (
            Some(ColorsDisabled::UnsupportedModel),
            Vec::new(),
            Vec::new(),
        ),
        (Some(_), TableRead::NotRead) => (Some(ColorsDisabled::NotRead), Vec::new(), Vec::new()),
        (Some(_), TableRead::Unreadable(message)) => (
            Some(ColorsDisabled::Unreadable {
                message: message.clone(),
            }),
            Vec::new(),
            Vec::new(),
        ),
        (Some(model), TableRead::Ok(table)) => {
            let keys: Vec<KeyColorView> = model
                .keys
                .iter()
                .filter(|k| Some(k.index) != model.knob_index)
                .map(|k| KeyColorView {
                    key: key_ref_index(k.index),
                    color: rgb_text(table.color(k.index).unwrap_or_default()),
                })
                .collect();
            let kept = (0u8..)
                .zip(&table.colors)
                .filter(|(i, c)| !c.is_black() && !keys.iter().any(|k| k.key.index == *i))
                .map(|(slot, c)| KeptColorView {
                    slot,
                    color: rgb_text(*c),
                })
                .collect();
            (None, keys, kept)
        }
    };
    ColorsState {
        revision: cache.colors.revision,
        generation: cache.generation,
        profile: cache.profile,
        read_at: cache.read_at.clone(),
        editable: disabled.is_none(),
        disabled,
        keys,
        kept,
        active_table: cache.colors.data.active,
        light_mode: mode,
        shown_now,
        can_switch_to_custom,
    }
}

// ----- dead band -----

/// The largest dead-band value in trigger units: the vendor site's slider
/// maximum of 1.00 mm in the keyboard's unit, at most the model's ceiling
/// (the library's rule; 50 on the W75).
pub(crate) fn dead_band_max(model: &Model, unit_um: u32) -> u16 {
    if unit_um == 0 {
        return 0;
    }
    u16::try_from(1000u32.div_ceil(unit_um))
        .unwrap_or(u16::MAX)
        .min(model.write_ceilings.max_dead_band)
}

/// The dead-band view of the cache.
pub(crate) fn dead_band_state(cache: &EditorsCache) -> DeadBandState {
    let max = match (cache.model, cache.unit_um) {
        (Some(model), Some(unit)) => dead_band_max(model, unit),
        _ => 0,
    };
    let disabled = match (cache.model, cache.unit_um, &cache.dead_band.data) {
        (None, _, _) => Some(DeadBandDisabled::UnsupportedModel),
        (_, unit, _) if unit != Some(UNIT_UM) => {
            Some(DeadBandDisabled::DistanceUnit { unit_um: unit })
        }
        (_, _, TableRead::NotRead) => Some(DeadBandDisabled::NotRead),
        (_, _, TableRead::Unreadable(message)) => Some(DeadBandDisabled::Unreadable {
            message: message.clone(),
        }),
        (_, _, TableRead::Ok(_)) => None,
    };
    let keys = match (cache.model, &cache.dead_band.data) {
        (Some(model), TableRead::Ok(table)) => model
            .keys
            .iter()
            .filter(|k| Some(k.index) != model.knob_index)
            .map(|k| {
                let [top, bottom] = raw_entry(table, k.index);
                KeyDeadBandView {
                    key: key_ref_index(k.index),
                    value: DeadBandValue {
                        top: u16::from(top),
                        bottom: u16::from(bottom),
                    },
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    DeadBandState {
        revision: cache.dead_band.revision,
        generation: cache.generation,
        profile: cache.profile,
        read_at: cache.read_at.clone(),
        editable: disabled.is_none(),
        disabled,
        unit_um: cache.unit_um,
        max,
        keys,
    }
}

/// `[top, bottom]` of key `index` in trigger units, as stored.
pub(crate) fn raw_entry(table: &openwraith_hid::protocol::DeadBandTable, index: u8) -> [u8; 2] {
    let at = usize::from(index) * 2;
    [
        table.raw.0.get(at).copied().unwrap_or(0),
        table.raw.0.get(at + 1).copied().unwrap_or(0),
    ]
}

// ----- SOCD -----

/// Whether a SOCD mode is verified on a W75. This mirrors the library's
/// evidence of a SOCD write (`SocdWrite::evidence`, which app code may not
/// name): modes 1 and 4 are confirmed (H21), modes 2 and 3 are not (Q35).
/// `tests::editors` checks it against the library for every mode, so a
/// flip there fails the build until this follows.
pub(crate) fn socd_mode_evidence(mode: SocdMode) -> EvidenceView {
    match mode {
        SocdMode::M1 | SocdMode::M4 => EvidenceView::Confirmed,
        SocdMode::M2 | SocdMode::M3 => EvidenceView::Unverified {
            notes: vec![UnverifiedNoteView {
                what: "SOCD modes 2 and 3".to_string(),
                question: "Q35".to_string(),
            }],
        },
    }
}

/// A key of a pair, by its usage.
pub(crate) fn socd_key_view(model: &Model, usage: u8) -> SocdKeyView {
    let key = model
        .keys
        .iter()
        .find(|k| k.hid_usage == Some(usage) && Some(k.index) != model.knob_index)
        .map(|k| key_ref_index(k.index));
    SocdKeyView { usage, key }
}

/// A pair.
pub(crate) fn socd_pair_view(model: &Model, pair: &SocdPair) -> SocdPairView {
    SocdPairView {
        key1: socd_key_view(model, pair.key1.get()),
        key2: socd_key_view(model, pair.key2.get()),
        mode: pair.mode.number(),
    }
}

/// The SOCD view of the cache.
pub(crate) fn socd_state(cache: &EditorsCache) -> SocdStateView {
    let modes = SocdMode::ALL
        .into_iter()
        .map(|mode| SocdModeView {
            mode: mode.number(),
            evidence: socd_mode_evidence(mode),
        })
        .collect();
    let mut view = SocdStateView {
        revision: cache.socd.revision,
        generation: cache.generation,
        profile: cache.profile,
        read_at: cache.read_at.clone(),
        editable: false,
        disabled: None,
        power: false,
        pairs: Vec::new(),
        pairs_writable: false,
        unwritable_reason: None,
        raw_pairs_hex: String::new(),
        modes,
        max_pairs: 0,
        keys: Vec::new(),
    };
    let Some(model) = cache.model else {
        view.disabled = Some(SocdDisabled::UnsupportedModel);
        return view;
    };
    view.max_pairs = model.write_ceilings.socd_max_pairs;
    view.keys = model
        .keys
        .iter()
        .filter(|k| Some(k.index) != model.knob_index)
        .filter_map(|k| {
            k.hid_usage.map(|usage| SocdKeyView {
                usage,
                key: Some(key_ref_index(k.index)),
            })
        })
        .collect();
    match &cache.socd.data {
        TableRead::NotRead => view.disabled = Some(SocdDisabled::NotRead),
        TableRead::Unreadable(message) => {
            view.disabled = Some(SocdDisabled::Unreadable {
                message: message.clone(),
            });
        }
        TableRead::Ok(state) => {
            if state.power > 1 {
                view.disabled = Some(SocdDisabled::Unreadable {
                    message: format!("the SOCD power byte is {:02x}, not 00 or 01", state.power),
                });
            }
            view.power = state.power == 1;
            view.pairs = state
                .pairs
                .iter()
                .flatten()
                .map(|p| socd_pair_view(model, p))
                .collect();
            view.pairs_writable = state.pairs.is_some();
            view.unwritable_reason = state.unwritable.map(str::to_string);
            view.raw_pairs_hex = hex(&state.raw_pairs.0);
            view.editable = view.disabled.is_none();
        }
    }
    view
}
