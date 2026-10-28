//! Trigger presets: the library's built-in table and the user's presets
//! (stored in the settings file). A preset is applied as a trigger edit
//! (`prepare_preset_edit` stores a trigger plan that `apply_trigger_edit`
//! writes): no write path of its own. A value outside the keyboard's
//! `21/04` limits is refused, never clamped.

use openwraith_hid::protocol::{Distance, W75};
use openwraith_hid::{RtEdit, TRIGGER_PRESETS, TriggerEdit, preset_edit, trigger_preset};

use crate::backend::DeviceBackend;
use crate::config::CoreConfig;
use crate::dto::{
    AppError, AppErrorKind, EditFieldView, EditProblemView, PresetEditIntent, PresetRef,
    PresetRtView, PresetsView, RtIntent, TriggerEditPreview, TriggerLimitsView, TriggerPresetView,
    UserPresetIntent, UserPresetView,
};
use crate::map::editors::refused;
use crate::settings::{self, StoredPreset};
use crate::worker::Worker;

/// The most user presets.
pub(crate) const MAX_USER_PRESETS: usize = 32;

/// The longest preset name.
const MAX_NAME: usize = 40;

/// The limits presets are checked against: the keyboard's (`21/04`) when
/// one was read, else the W75's.
fn limits_or_model(limits: Option<TriggerLimitsView>) -> TriggerLimitsView {
    limits.unwrap_or(TriggerLimitsView {
        min_actuation: W75.write_ceilings.min_actuation,
        max_actuation: W75.write_ceilings.max_actuation,
        max_sensitivity: W75.write_ceilings.max_sensitivity,
    })
}

fn rt_view(rt: RtEdit) -> PresetRtView {
    match rt {
        RtEdit::Off => PresetRtView::Off,
        RtEdit::OffWith { press, release } => PresetRtView::OffWith {
            press: press.units(),
            release: release.units(),
        },
        RtEdit::Same { sensitivity } => PresetRtView::Same {
            sensitivity: sensitivity.units(),
        },
        RtEdit::Separate { press, release } => PresetRtView::Separate {
            press: press.units(),
            release: release.units(),
        },
    }
}

/// The problem of a value outside the limits, if any.
fn out_of_range(field: EditFieldView, value: u16, min: u16, max: u16) -> Option<EditProblemView> {
    (value < min || value > max).then_some(EditProblemView::OutOfRange {
        field,
        value,
        min,
        max,
    })
}

/// Why a user preset cannot be applied with `limits`, if so.
fn problem(actuation: u16, rt: RtIntent, limits: TriggerLimitsView) -> Option<EditProblemView> {
    let max = limits.max_sensitivity;
    out_of_range(
        EditFieldView::Actuation,
        actuation,
        limits.min_actuation,
        limits.max_actuation,
    )
    .or(match rt {
        RtIntent::Off => None,
        RtIntent::Same { sensitivity } => {
            out_of_range(EditFieldView::Sensitivity, sensitivity, 1, max)
        }
        RtIntent::Separate { press, release } => out_of_range(EditFieldView::Press, press, 1, max)
            .or(out_of_range(EditFieldView::Release, release, 1, max)),
    })
}

fn user_view(preset: &StoredPreset, limits: TriggerLimitsView) -> UserPresetView {
    UserPresetView {
        id: preset.id.clone(),
        name: preset.name.clone(),
        actuation: preset.actuation,
        rapid_trigger: preset.rapid_trigger,
        refused: problem(preset.actuation, preset.rapid_trigger, limits),
    }
}

/// Every preset, checked against `limits` (the keyboard's, if read).
pub(crate) fn presets_view(
    stored: &[StoredPreset],
    limits: Option<TriggerLimitsView>,
) -> PresetsView {
    let checked = limits_or_model(limits);
    PresetsView {
        builtin: TRIGGER_PRESETS
            .iter()
            .map(|p| TriggerPresetView {
                id: p.id.to_string(),
                actuation: p.actuation.units(),
                rapid_trigger: rt_view(p.rapid_trigger),
            })
            .collect(),
        user: stored.iter().map(|p| user_view(p, checked)).collect(),
        limits,
        max_user: crate::map::sat16(MAX_USER_PRESETS),
    }
}

fn settings_path(config: &CoreConfig) -> Result<&std::path::Path, AppError> {
    config.settings_path.as_deref().ok_or_else(|| {
        AppError::new(
            AppErrorKind::Io,
            "this build has no settings file; presets cannot be saved",
        )
    })
}

/// `list_presets`.
pub(crate) fn list(
    config: &CoreConfig,
    limits: Option<TriggerLimitsView>,
) -> Result<PresetsView, AppError> {
    let stored = settings::load_presets(settings_path(config)?);
    Ok(presets_view(&stored, limits))
}

/// A cleaned preset name (control characters removed, trimmed, 1 to 40
/// characters).
fn name(text: &str) -> Result<String, AppError> {
    let name: String = text.chars().filter(|c| !c.is_control()).collect();
    let name = name.trim().to_string();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(refused(
            EditProblemView::PresetName,
            "a preset name has 1 to 40 characters",
        ));
    }
    Ok(name)
}

/// `save_user_preset`: a new preset, or the one with `id` replaced; values
/// outside the limits are refused.
pub(crate) fn save(
    config: &CoreConfig,
    intent: &UserPresetIntent,
    limits: Option<TriggerLimitsView>,
) -> Result<PresetsView, AppError> {
    let name = name(&intent.name)?;
    if let Some(problem) = problem(
        intent.actuation,
        intent.rapid_trigger,
        limits_or_model(limits),
    ) {
        return Err(refused(
            problem,
            "the preset has a value outside the keyboard's limits",
        ));
    }
    let path = settings_path(config)?;
    let (stored, ()) = settings::update_presets(path, |presets| {
        let entry = |id: String| StoredPreset {
            id,
            name: name.clone(),
            actuation: intent.actuation,
            rapid_trigger: intent.rapid_trigger,
        };
        match &intent.id {
            Some(id) => {
                let slot = presets.iter_mut().find(|p| p.id == *id).ok_or_else(|| {
                    refused(
                        EditProblemView::UnknownPreset {
                            id: crate::map::clean(id),
                        },
                        "there is no such preset",
                    )
                })?;
                *slot = entry(id.clone());
            }
            None => {
                if presets.len() >= MAX_USER_PRESETS {
                    return Err(refused(
                        EditProblemView::TooManyPresets {
                            max: crate::map::sat16(MAX_USER_PRESETS),
                        },
                        format!("at most {MAX_USER_PRESETS} presets"),
                    ));
                }
                let next = presets
                    .iter()
                    .filter_map(|p| p.id.strip_prefix("u-")?.parse::<u32>().ok())
                    .max()
                    .map_or(1, |n| n.saturating_add(1));
                presets.push(entry(format!("u-{next}")));
            }
        }
        Ok(())
    })?;
    Ok(presets_view(&stored, limits))
}

/// `delete_user_preset`.
pub(crate) fn delete(
    config: &CoreConfig,
    id: &str,
    limits: Option<TriggerLimitsView>,
) -> Result<PresetsView, AppError> {
    let path = settings_path(config)?;
    let (stored, ()) = settings::update_presets(path, |presets| {
        let before = presets.len();
        presets.retain(|p| p.id != id);
        if presets.len() == before {
            return Err(refused(
                EditProblemView::UnknownPreset {
                    id: crate::map::clean(id),
                },
                "there is no such preset",
            ));
        }
        Ok(())
    })?;
    Ok(presets_view(&stored, limits))
}

impl<B: DeviceBackend> Worker<B> {
    /// `prepare_preset_edit`: the preset as a trigger edit of `keys`,
    /// checked against the keyboard's limits (refused, never clamped), then
    /// planned like any trigger edit.
    pub(crate) fn prepare_preset_edit(
        &mut self,
        intent: &PresetEditIntent,
    ) -> Result<TriggerEditPreview, AppError> {
        self.check_writable()?;
        let (keys, limits) = self.trigger_keys(&intent.keys, intent.based_on)?;
        let max = limits.max_sensitivity;
        let edit = match &intent.preset {
            PresetRef::Builtin { id } => {
                let preset = trigger_preset(id).ok_or_else(|| {
                    refused(
                        EditProblemView::UnknownPreset {
                            id: crate::map::clean(id),
                        },
                        "there is no such built-in preset",
                    )
                })?;
                let rt_problem =
                    match preset.rapid_trigger {
                        RtEdit::Off => None,
                        RtEdit::Same { sensitivity } => {
                            out_of_range(EditFieldView::Sensitivity, sensitivity.units(), 1, max)
                        }
                        RtEdit::Separate { press, release }
                        | RtEdit::OffWith { press, release } => {
                            out_of_range(EditFieldView::Press, press.units(), 1, max).or(
                                out_of_range(EditFieldView::Release, release.units(), 1, max),
                            )
                        }
                    };
                let problem = out_of_range(
                    EditFieldView::Actuation,
                    preset.actuation.units(),
                    limits.min_actuation,
                    limits.max_actuation,
                )
                .or(rt_problem);
                if let Some(problem) = problem {
                    return Err(refused(
                        problem,
                        "the preset has a value outside the keyboard's limits",
                    ));
                }
                preset_edit(preset, keys)
            }
            PresetRef::User { id } => {
                let path = self.config.settings_path.as_deref().ok_or_else(|| {
                    AppError::new(AppErrorKind::Io, "this build has no settings file")
                })?;
                let stored = settings::load_presets(path);
                let preset = stored.iter().find(|p| p.id == *id).ok_or_else(|| {
                    refused(
                        EditProblemView::UnknownPreset {
                            id: crate::map::clean(id),
                        },
                        "there is no such preset",
                    )
                })?;
                if let Some(problem) = problem(preset.actuation, preset.rapid_trigger, limits) {
                    return Err(refused(
                        problem,
                        "the preset has a value outside the keyboard's limits",
                    ));
                }
                TriggerEdit {
                    keys,
                    actuation: Some(Distance::from_units(preset.actuation)),
                    rapid_trigger: Some(match preset.rapid_trigger {
                        RtIntent::Off => RtEdit::Off,
                        RtIntent::Same { sensitivity } => RtEdit::Same {
                            sensitivity: Distance::from_units(sensitivity),
                        },
                        RtIntent::Separate { press, release } => RtEdit::Separate {
                            press: Distance::from_units(press),
                            release: Distance::from_units(release),
                        },
                    }),
                }
            }
        };
        self.prepare_trigger_plan(&edit)
    }
}
