//! A backup (or fresh replies) as the settings snapshot the pages show, and
//! backup files as summaries.

use openwraith_hid::{Backup, DeviceIdentity};
use openwraith_protocol::{
    Distance, FromResponse, KeyPos, KeyTrigger, LightLimits, LightMode, LightModeList, LightRecord,
    Model, RawResponse, ReadRequest, TriggerLimits, TriggerMode, TriggerSetting, TriggerWriteMode,
    W75, WriteBuildError,
};

use super::{clean, key_ref, light_mode, sat16};
use crate::config::SIM_TOOL_NAME;
use crate::dto::{
    BackupFileKind, BackupFileSummary, DeviceIdentityView, DeviceSnapshot, FileRef, KeyTriggerView,
    LightDisabled, LightState, LightValue, NotEditable, SafetyReason, ToolView, TriggerDisabled,
    TriggerLimitsView, TriggerModeView, TriggerState, TriggerValue,
};

/// The keyboard's identity.
pub(crate) fn device_identity(device: &DeviceIdentity) -> DeviceIdentityView {
    DeviceIdentityView {
        model: device.model.as_deref().map(clean),
        product: device.product.as_deref().map(clean),
        firmware: device.firmware.as_deref().map(clean),
        build_date: device.build_date.as_deref().map(clean),
        vendor_id: device.usb.as_ref().map(|u| u.vendor_id),
        product_id: device.usb.as_ref().map(|u| u.product_id),
    }
}

/// A trigger mode.
fn mode_view(mode: TriggerMode) -> TriggerModeView {
    match mode {
        TriggerMode::Off => TriggerModeView::Off,
        TriggerMode::RapidTrigger => TriggerModeView::RapidTrigger,
        TriggerMode::RapidTriggerSeparate => TriggerModeView::RapidTriggerSeparate,
        TriggerMode::Unknown(_) => TriggerModeView::Unknown,
    }
}

/// The settings of a decoded `21/05` reply.
fn trigger_value(trigger: &KeyTrigger) -> TriggerValue {
    TriggerValue {
        mode: mode_view(trigger.mode),
        mode_byte: trigger.mode.byte(),
        actuation: trigger.actuation.units(),
        rt_press: trigger.rt_press.units(),
        rt_release: trigger.rt_release.units(),
    }
}

/// The settings of a writable trigger setting.
pub(crate) fn trigger_value_of_setting(setting: &TriggerSetting) -> TriggerValue {
    TriggerValue {
        mode: match setting.mode {
            TriggerWriteMode::Off => TriggerModeView::Off,
            TriggerWriteMode::RapidTrigger => TriggerModeView::RapidTrigger,
            TriggerWriteMode::RapidTriggerSeparate => TriggerModeView::RapidTriggerSeparate,
        },
        mode_byte: setting.mode.byte(),
        actuation: setting.actuation.units(),
        rt_press: setting.rt_press.units(),
        rt_release: setting.rt_release.units(),
    }
}

/// The view of one key from its `21/05` reply (if read).
pub(crate) fn key_trigger_view(
    model: &Model,
    pos: KeyPos,
    raw: Option<&RawResponse>,
) -> KeyTriggerView {
    let knob = model.knob_index == Some(pos.index());
    let value = raw
        .and_then(|r| KeyTrigger::from_response(r).ok())
        .map(|t| trigger_value(&t));
    let not_editable = if knob {
        Some(NotEditable::Knob)
    } else {
        match raw {
            None => Some(NotEditable::NotRead),
            Some(raw) => match TriggerSetting::from_response(raw) {
                Ok(_) => None,
                Err(WriteBuildError::UnknownTriggerMode { mode }) => {
                    Some(NotEditable::UnknownMode { mode })
                }
                Err(WriteBuildError::SecondTravelDiffers { .. }) => {
                    Some(NotEditable::SecondTravelDiffers)
                }
                Err(WriteBuildError::PressReleaseDiffer { .. }) => {
                    Some(NotEditable::PressReleaseDiffer)
                }
                Err(_) => Some(NotEditable::Undecodable),
            },
        }
    };
    KeyTriggerView {
        key: key_ref(pos),
        selectable: !knob,
        value,
        not_editable,
    }
}

/// The main light record as a value.
pub(crate) fn light_value(record: &LightRecord) -> LightValue {
    let color = record.foreground();
    LightValue {
        mode: light_mode(record.mode()),
        brightness: record.brightness(),
        speed: record.speed(),
        color: format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b),
        full_color: record.full_color() != 0,
        power_on: record.power() == 0,
    }
}

/// The main light from its `0A` and `07 01` replies.
pub(crate) fn light_state(
    model: Option<&Model>,
    modes: Option<&RawResponse>,
    light: Option<&RawResponse>,
) -> LightState {
    let record = light.map(LightRecord::from_response);
    let current = record
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(light_value);
    let list = modes.and_then(|r| LightModeList::from_response(r).ok());
    let limits = match (model, &list) {
        (Some(model), Some(list)) => Some(LightLimits::new(model, list)),
        _ => None,
    };
    let modes_view = limits
        .as_ref()
        .map(|l| {
            l.writable_modes()
                .into_iter()
                .filter(|m| *m != LightMode::MusicRhythm)
                .map(light_mode)
                .collect()
        })
        .unwrap_or_default();
    let disabled = if model.is_none() {
        Some(LightDisabled::UnsupportedModel)
    } else if record.is_none() {
        Some(LightDisabled::NotRead)
    } else if current.is_none() {
        Some(LightDisabled::InvalidRecord)
    } else if limits.is_none() {
        Some(LightDisabled::LimitsUnknown)
    } else {
        None
    };
    LightState {
        current,
        modes: modes_view,
        max_brightness: limits.as_ref().map_or(0, LightLimits::max_brightness),
        max_speed: limits.as_ref().map_or(0, LightLimits::max_speed),
        editable: disabled.is_none(),
        disabled,
    }
}

/// The trigger state of the primary snapshot of `backup`.
fn trigger_state(backup: &Backup) -> TriggerState {
    let model = backup.model();
    let layout = model.unwrap_or(&W75);
    let view = backup.primary();
    let high_precision = view.status().is_some_and(|s| s.flags_b.high_precision);
    let global = view.trigger_global();
    let unit_um = global.map(|g| g.unit_micrometres(high_precision));
    let limits = match (model, global) {
        (Some(model), Some(global)) => {
            Some(TriggerLimits::new(model, global)).filter(TriggerLimits::is_supported)
        }
        _ => None,
    };
    let disabled = if model.is_none() {
        Some(TriggerDisabled::UnsupportedModel)
    } else if unit_um != Some(Distance::MICROMETRES_PER_UNIT) {
        Some(TriggerDisabled::DistanceUnit)
    } else if limits.is_none() {
        Some(TriggerDisabled::LimitsUnknown)
    } else {
        None
    };
    let keys = layout
        .positions()
        .map(|pos| key_trigger_view(layout, pos, view.response(ReadRequest::KeyTrigger(pos))))
        .collect();
    TriggerState {
        unit_um,
        editable: disabled.is_none(),
        disabled,
        limits: limits.map(|l| TriggerLimitsView {
            min_actuation: l.min_actuation().units(),
            max_actuation: l.max_actuation().units(),
            max_sensitivity: l.max_sensitivity().units(),
        }),
        keys,
    }
}

/// The snapshot of the primary profile of `backup`.
pub(crate) fn snapshot_from_backup(
    backup: &Backup,
    revision: u32,
    generation: u32,
) -> DeviceSnapshot {
    let view = backup.primary();
    DeviceSnapshot {
        revision,
        generation,
        read_at: backup.created_at.clone(),
        profile: backup.active_profile.unwrap_or(0),
        device: device_identity(&backup.device),
        poll_rate_hz: view.poll_rate().and_then(|p| p.hz),
        triggers: trigger_state(backup),
        light: light_state(
            backup.model(),
            view.response(ReadRequest::MainLightModes),
            view.response(ReadRequest::MainLight),
        ),
        undecoded: sat16(view.undecoded().count()),
    }
}

/// A backup file as a summary for the restore wizard.
pub(crate) fn backup_summary(
    file: FileRef,
    kind: BackupFileKind,
    backup: &Backup,
    connected: Option<&DeviceIdentityView>,
    safety_reason: Option<SafetyReason>,
) -> BackupFileSummary {
    let device = device_identity(&backup.device);
    let undecoded: usize = backup
        .snapshots()
        .iter()
        .map(|s| s.undecoded().count())
        .sum();
    BackupFileSummary {
        file,
        kind,
        created_at: clean(&backup.created_at),
        tool: ToolView {
            name: clean(&backup.tool.name),
            version: clean(&backup.tool.version),
        },
        made_by_simulator: backup.tool.name == SIM_TOOL_NAME,
        same_model: connected.map(|c| c.product == device.product),
        same_firmware: connected.map(|c| c.firmware == device.firmware),
        model: device.model,
        product: device.product,
        firmware: device.firmware,
        schema_version: backup.schema_version,
        profiles: backup.profile_numbers(),
        active_profile: backup.active_profile,
        safety_reason,
        undecoded: sat16(undecoded),
    }
}
