//! A short device summary: model, firmware, capabilities and active profile
//! (what `openwraith info` shows and what a GUI shows on connect).

use openwraith_protocol::{
    ActiveProfile, DeviceInfo, FromResponse, Model, PollRate, RawResponse, ReadRequest, Status,
    TriggerGlobal, model_for_product,
};
use serde::Serialize;

use crate::backup::DeviceIdentity;
use crate::error::{BackupError, SessionError};
use crate::session::Session;
use crate::transport::Transport;

/// Reads the device info and checks that the product is a supported model
/// (spec section 2.1: the model is identified by the `0x0D` string). Nothing
/// else is sent to an unsupported keyboard.
pub(crate) fn identify<T: Transport>(
    session: &mut Session<T>,
) -> Result<(RawResponse, DeviceInfo, &'static Model), BackupError> {
    let raw = session.request(ReadRequest::DeviceInfo)?;
    let info = DeviceInfo::from_response(&raw).map_err(|source| SessionError::Decode {
        request: ReadRequest::DeviceInfo,
        source,
    })?;
    let model = info
        .product
        .as_deref()
        .and_then(model_for_product)
        .ok_or_else(|| BackupError::UnsupportedModel {
            product: info.product.clone(),
            info: info.info.clone(),
        })?;
    Ok((raw, info, model))
}

/// Model, firmware, capabilities and active profile of a keyboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceSummary {
    /// Model, product code, firmware, device-info string and USB identity.
    pub device: DeviceIdentity,
    /// Status and capability flags (`01`).
    pub status: Status,
    /// Active onboard profile (`21/11`).
    pub active_profile: u8,
    /// Polling rate (`21/09`), if the keyboard reports the feature.
    pub poll_rate: Option<PollRate>,
    /// Travel range and unit (`21/04`).
    pub trigger_global: TriggerGlobal,
}

impl DeviceSummary {
    /// The model, if this version knows it (always true for a summary that
    /// [`read_summary`] returned).
    pub fn model(&self) -> Option<&'static Model> {
        self.device.product.as_deref().and_then(model_for_product)
    }
}

/// Reads the few values of a [`DeviceSummary`] (read-only): device info,
/// status, active profile, global trigger parameters and, if supported, the
/// polling rate.
///
/// Refuses an unknown model with [`BackupError::UnsupportedModel`] after the
/// device-info request, before sending anything else.
pub fn read_summary<T: Transport>(session: &mut Session<T>) -> Result<DeviceSummary, BackupError> {
    let (_, info, model) = identify(session)?;
    let status: Status = session.read(ReadRequest::Status)?;
    let profile: ActiveProfile = session.read(ReadRequest::ActiveProfile)?;
    let trigger_global: TriggerGlobal = session.read(ReadRequest::TriggerGlobal)?;
    let poll_rate = if status.flags_a.poll_rate {
        Some(session.read::<PollRate>(ReadRequest::PollRate)?)
    } else {
        None
    };
    Ok(DeviceSummary {
        device: DeviceIdentity::new(&info, Some(model), session.transport().usb_info()),
        status,
        active_profile: profile.profile,
        poll_rate,
        trigger_global,
    })
}
