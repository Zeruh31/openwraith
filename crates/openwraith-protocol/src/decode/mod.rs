//! Typed models decoded from complete replies (spec section 4).
//!
//! Every model keeps the bytes it was decoded from (`raw`, the concatenated
//! declared payloads) so nothing is lost even where the meaning of a byte is
//! unknown. Field docs repeat the spec's confidence labels; CODE-ONLY and
//! GUESS fields are interpretations, the raw bytes are the truth.
//!
//! Use the [`FromResponse`] trait for a specific model, or [`decode`] to get a
//! [`Decoded`] value for any reply.

mod advanced;
mod info;
mod keymap;
mod light;
mod macros;
mod socd;
mod status;
mod switch;
mod trigger;

pub use advanced::{AdvancedKey, AdvancedKeySlot, AdvancedKeys};
pub use info::DeviceInfo;
pub use keymap::{KeyAction, KeymapEntry, KeymapTable};
pub use light::{
    ActiveCustomTable, ColorTable, LightDirection, LightEffect, LightMode, LightModeList, Rgb,
};
pub use macros::{MacroContent, MacroPlayMode, MacroSlot, MacroStep};
pub use socd::{SocdEntry, SocdPairs, SocdSettings};
pub use status::{
    ActiveProfile, CapabilityFlagsA, CapabilityFlagsB, FnLockedKeys, LinkType, PollRate, Status,
    WinLockOptions,
};
pub use switch::{SwitchTypeList, SwitchTypeTable, switch_type_name};
pub use trigger::{DeadBand, DeadBandTable, KeyTrigger, TriggerGlobal, TriggerMode};

use crate::error::DecodeError;
use crate::request::ReadRequest;
use crate::response::RawResponse;

/// A model that can be decoded from the complete reply to its request.
pub trait FromResponse: Sized {
    /// Decodes `response`. Fails with [`DecodeError::WrongRequest`] if the
    /// reply belongs to another kind of request.
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError>;
}

/// Any decoded reply.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Decoded {
    /// Reply to [`ReadRequest::DeviceInfo`].
    DeviceInfo(DeviceInfo),
    /// Reply to [`ReadRequest::Status`].
    Status(Status),
    /// Reply to [`ReadRequest::FnLockedKeys`].
    FnLockedKeys(FnLockedKeys),
    /// Reply to [`ReadRequest::MainLightModes`].
    MainLightModes(LightModeList),
    /// Reply to [`ReadRequest::SideLightModes`].
    SideLightModes(LightModeList),
    /// Reply to [`ReadRequest::MainLight`].
    MainLight(LightEffect),
    /// Reply to [`ReadRequest::SideLight`].
    SideLight(LightEffect),
    /// Reply to [`ReadRequest::ActiveCustomTable`].
    ActiveCustomTable(ActiveCustomTable),
    /// Reply to [`ReadRequest::CustomColors`].
    CustomColors(ColorTable),
    /// Reply to [`ReadRequest::Keymap`].
    Keymap(KeymapTable),
    /// Reply to [`ReadRequest::Macro`].
    Macro(MacroSlot),
    /// Reply to [`ReadRequest::AdvancedKeys`].
    AdvancedKeys(AdvancedKeys),
    /// Reply to [`ReadRequest::SocdPairs`].
    SocdPairs(SocdPairs),
    /// Reply to [`ReadRequest::SocdSettings`].
    SocdSettings(SocdSettings),
    /// Reply to [`ReadRequest::SwitchTypeList`].
    SwitchTypeList(SwitchTypeList),
    /// Reply to [`ReadRequest::SwitchTypes`].
    SwitchTypes(SwitchTypeTable),
    /// Reply to [`ReadRequest::DeadBand`].
    DeadBand(DeadBandTable),
    /// Reply to [`ReadRequest::TriggerGlobal`].
    TriggerGlobal(TriggerGlobal),
    /// Reply to [`ReadRequest::KeyTrigger`].
    KeyTrigger(KeyTrigger),
    /// Reply to [`ReadRequest::PollRate`].
    PollRate(PollRate),
    /// Reply to [`ReadRequest::ActiveProfile`].
    ActiveProfile(ActiveProfile),
}

/// Decodes any complete reply into the matching [`Decoded`] variant.
pub fn decode(response: &RawResponse) -> Result<Decoded, DecodeError> {
    let r = response;
    Ok(match r.request() {
        ReadRequest::DeviceInfo => Decoded::DeviceInfo(DeviceInfo::from_response(r)?),
        ReadRequest::Status => Decoded::Status(Status::from_response(r)?),
        ReadRequest::FnLockedKeys => Decoded::FnLockedKeys(FnLockedKeys::from_response(r)?),
        ReadRequest::MainLightModes => Decoded::MainLightModes(LightModeList::from_response(r)?),
        ReadRequest::SideLightModes => Decoded::SideLightModes(LightModeList::from_response(r)?),
        ReadRequest::MainLight => Decoded::MainLight(LightEffect::from_response(r)?),
        ReadRequest::SideLight => Decoded::SideLight(LightEffect::from_response(r)?),
        ReadRequest::ActiveCustomTable => {
            Decoded::ActiveCustomTable(ActiveCustomTable::from_response(r)?)
        }
        ReadRequest::CustomColors(_) => Decoded::CustomColors(ColorTable::from_response(r)?),
        ReadRequest::Keymap(_) => Decoded::Keymap(KeymapTable::from_response(r)?),
        ReadRequest::Macro(_) => Decoded::Macro(MacroSlot::from_response(r)?),
        ReadRequest::AdvancedKeys => Decoded::AdvancedKeys(AdvancedKeys::from_response(r)?),
        ReadRequest::SocdPairs => Decoded::SocdPairs(SocdPairs::from_response(r)?),
        ReadRequest::SocdSettings => Decoded::SocdSettings(SocdSettings::from_response(r)?),
        ReadRequest::SwitchTypeList => Decoded::SwitchTypeList(SwitchTypeList::from_response(r)?),
        ReadRequest::SwitchTypes => Decoded::SwitchTypes(SwitchTypeTable::from_response(r)?),
        ReadRequest::DeadBand => Decoded::DeadBand(DeadBandTable::from_response(r)?),
        ReadRequest::TriggerGlobal => Decoded::TriggerGlobal(TriggerGlobal::from_response(r)?),
        ReadRequest::KeyTrigger(_) => Decoded::KeyTrigger(KeyTrigger::from_response(r)?),
        ReadRequest::PollRate => Decoded::PollRate(PollRate::from_response(r)?),
        ReadRequest::ActiveProfile => Decoded::ActiveProfile(ActiveProfile::from_response(r)?),
    })
}

/// Fails with [`DecodeError::WrongRequest`] unless `ok` holds for the request.
pub(crate) fn expect_request(
    response: &RawResponse,
    expected: &'static str,
    ok: impl FnOnce(ReadRequest) -> bool,
) -> Result<(), DecodeError> {
    if ok(response.request()) {
        Ok(())
    } else {
        Err(wrong_request(response, expected))
    }
}

/// The [`DecodeError::WrongRequest`] for decoding `response` as `expected`.
pub(crate) fn wrong_request(response: &RawResponse, expected: &'static str) -> DecodeError {
    DecodeError::WrongRequest {
        expected,
        got: response.request(),
    }
}

/// Fails with [`DecodeError::TooShort`] unless `bytes` has at least `need` bytes.
pub(crate) fn need(bytes: &[u8], what: &'static str, need: usize) -> Result<(), DecodeError> {
    if bytes.len() < need {
        Err(DecodeError::TooShort {
            what,
            need,
            got: bytes.len(),
        })
    } else {
        Ok(())
    }
}

/// Byte `i` of `bytes`, or 0 if out of range (callers check lengths first).
pub(crate) fn at(bytes: &[u8], i: usize) -> u8 {
    bytes.get(i).copied().unwrap_or(0)
}

/// Four bytes starting at `i` (missing bytes are 0).
pub(crate) fn quad(bytes: &[u8], i: usize) -> [u8; 4] {
    [
        at(bytes, i),
        at(bytes, i + 1),
        at(bytes, i + 2),
        at(bytes, i + 3),
    ]
}

#[cfg(test)]
pub(crate) mod test_util {
    //! Helpers to build replies in unit tests.

    use crate::frame::{REPORT_LEN, Report};
    use crate::request::ReadRequest;
    use crate::response::RawResponse;

    /// A report from its leading bytes (rest zero).
    pub fn report(prefix: &[u8]) -> Report {
        let mut bytes = [0u8; REPORT_LEN];
        bytes[..prefix.len()].copy_from_slice(prefix);
        Report::new(bytes)
    }

    /// A single-report reply: header `cmd sub 00 00 len` + `payload`.
    pub fn single(request: ReadRequest, cmd: u8, sub: u8, payload: &[u8]) -> RawResponse {
        let mut prefix = vec![cmd, sub, 0, 0, u8::try_from(payload.len()).unwrap()];
        prefix.extend_from_slice(payload);
        RawResponse::from_reports(request, [report(&prefix)]).unwrap()
    }

    /// A multi-chunk reply built from a flat table, chunked per the spec layout.
    pub fn table(request: ReadRequest, table: &[u8]) -> RawResponse {
        let layout = request.table_layout().unwrap();
        assert_eq!(table.len(), layout.total_len);
        let spec = request.response_spec();
        let sub = match spec.sub {
            crate::request::SubMatch::Exact(s) => s,
            _ => 0,
        };
        let reports = (0..layout.chunks).map(|i| {
            let start = usize::from(i) * layout.chunk_len;
            let len = layout.expected_len(i);
            let [hi, lo] = i.to_be_bytes();
            let mut prefix = vec![spec.cmd, sub, hi, lo, u8::try_from(len).unwrap()];
            prefix.extend_from_slice(&table[start..start + len]);
            report(&prefix)
        });
        RawResponse::from_reports(request, reports).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::{single, table};
    use super::*;

    /// Asserts that decoding `$response` as each `$model` fails with
    /// [`DecodeError::WrongRequest`].
    macro_rules! assert_rejects {
        ($response:expr, $($model:ty),+ $(,)?) => {
            $(
                assert!(
                    matches!(
                        <$model>::from_response($response),
                        Err(DecodeError::WrongRequest { .. })
                    ),
                    "{} accepted {}",
                    stringify!($model),
                    $response.request()
                );
            )+
        };
    }

    #[test]
    fn every_model_rejects_replies_to_other_requests() {
        let profile = single(ReadRequest::ActiveProfile, 0x21, 0, &[0x11, 0x01, 0x01]);
        assert_rejects!(
            &profile,
            DeviceInfo,
            Status,
            FnLockedKeys,
            LightModeList,
            LightEffect,
            ActiveCustomTable,
            ColorTable,
            KeymapTable,
            MacroSlot,
            AdvancedKeys,
            SocdPairs,
            SocdSettings,
            SwitchTypeList,
            SwitchTypeTable,
            DeadBandTable,
            TriggerGlobal,
            KeyTrigger,
            PollRate,
        );
        let dead_band = table(ReadRequest::DeadBand, &[0u8; 264]);
        assert_rejects!(&dead_band, ActiveProfile);
        // The dispatcher picks the right model.
        assert!(matches!(decode(&profile), Ok(Decoded::ActiveProfile(_))));
        assert!(matches!(decode(&dead_band), Ok(Decoded::DeadBand(_))));
    }
}
