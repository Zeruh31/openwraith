//! Switch types `25 00` (supported list) and `25 02` (per-key table)
//! (spec section 4.13).

use super::{FromResponse, at, expect_request, need};
use crate::error::DecodeError;
use crate::hex::HexBytes;
use crate::layout::MATRIX_SIZE;
use crate::request::ReadRequest;
use crate::response::RawResponse;

/// Name of a switch type ID. Only HM1, the W75's only type, is known.
pub fn switch_type_name(id: u8) -> Option<&'static str> {
    match id {
        1 => Some("HM1 (magnetic, 3.4 mm)"),
        _ => None,
    }
}

/// Supported switch types (`25 00` reply `00 01 01` on the W75: count 1, ID 1; CONFIRMED).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SwitchTypeList {
    /// Payload bytes 0-1: number of types (BE16).
    pub count: u16,
    /// The type IDs (as many as `count` announces and the payload holds).
    pub ids: Vec<u8>,
    /// The declared payload.
    pub raw: HexBytes,
}

impl FromResponse for SwitchTypeList {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "SwitchTypeList", |r| {
            r == ReadRequest::SwitchTypeList
        })?;
        let p = response.payload();
        need(&p, "switch type list", 2)?;
        let count = u16::from_be_bytes([at(&p, 0), at(&p, 1)]);
        let ids = p.iter().skip(2).take(usize::from(count)).copied().collect();
        Ok(SwitchTypeList {
            count,
            ids,
            raw: HexBytes(p),
        })
    }
}

/// Per-key switch type (`25 02`, 132 bytes; CONFIRMED framing, per-key
/// mapping CODE-ONLY). The W75 reports `01` everywhere, also for positions
/// without a key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SwitchTypeTable {
    /// 132 switch type IDs indexed by key index.
    pub per_key: Vec<u8>,
    /// The 132 table bytes.
    pub raw: HexBytes,
}

impl FromResponse for SwitchTypeTable {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "SwitchTypeTable", |r| {
            r == ReadRequest::SwitchTypes
        })?;
        let bytes = response.table_bytes("switch types")?;
        need(&bytes, "switch types", MATRIX_SIZE)?;
        Ok(SwitchTypeTable {
            per_key: bytes.iter().copied().take(MATRIX_SIZE).collect(),
            raw: HexBytes(bytes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::{single, table};

    #[test]
    fn list() {
        let l = SwitchTypeList::from_response(&single(
            ReadRequest::SwitchTypeList,
            0x25,
            0x00,
            &[0x00, 0x01, 0x01],
        ))
        .unwrap();
        assert_eq!(l.count, 1);
        assert_eq!(l.ids, vec![1]);
        assert_eq!(switch_type_name(1), Some("HM1 (magnetic, 3.4 mm)"));
        assert_eq!(switch_type_name(7), None);
    }

    #[test]
    fn list_with_bogus_count_is_bounded() {
        let l = SwitchTypeList::from_response(&single(
            ReadRequest::SwitchTypeList,
            0x25,
            0x00,
            &[0xFF, 0xFF, 0x01],
        ))
        .unwrap();
        assert_eq!(l.ids, vec![1]);
    }

    #[test]
    fn per_key_table() {
        let t =
            SwitchTypeTable::from_response(&table(ReadRequest::SwitchTypes, &[1u8; 132])).unwrap();
        assert_eq!(t.per_key.len(), 132);
        assert!(t.per_key.iter().all(|id| *id == 1));
    }
}
