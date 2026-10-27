//! The live travel test (spec section 4.11.7): open (`21 .. 18 02` + key
//! mask) and close (`21 .. 18 03`). CONFIRMED by c14 (#981 and #1312, each
//! acked `21 00 00 00 01 00` within 1 ms).
//!
//! These are not table writes: nothing is stored, there is no read-back and
//! no commit. They are a separate request type, [`TravelTestRequest`], whose
//! [`step`](TravelTestRequest::step) is (with [`WriteRequest::steps`]) the
//! only other producer of [`WritePacket`]s. While the test is open the
//! keyboard streams one `21 .. 01` report per depth change
//! ([`TravelReport`](crate::decode::TravelReport)); it must always be
//! closed again.

use std::fmt;
use std::time::Duration;

use super::{AckSpec, WritePacket, WriteStep};
use crate::error::WriteBuildError;
use crate::layout::KeyMask;
use crate::model::{Model, known_model};

/// How long to wait for the ack of an open or close (the keyboard answered
/// within 1 ms in c14).
pub const TRAVEL_ACK_TIMEOUT: Duration = Duration::from_millis(500);

/// The key mask of a travel-test open: always every position of the
/// model's layout, knob included (the only form captured, c14 #981; whether
/// a smaller mask limits the stream is open question Q33).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TravelTestMask(KeyMask);

impl TravelTestMask {
    /// The layout mask of the model of [`MODELS`](crate::MODELS) with
    /// `model`'s product code (W75: `3f 26 3f 1f 1f 1f 3f 1f 1f 1f 3f 3f 2f
    /// 31 3f 2f` + 6 zero bytes).
    pub fn full_layout(model: &Model) -> Result<Self, WriteBuildError> {
        let known = known_model(model).ok_or(WriteBuildError::UnknownModel {
            product: model.product_code,
        })?;
        Ok(TravelTestMask(known.layout_mask()))
    }

    /// The mask.
    pub fn mask(&self) -> &KeyMask {
        &self.0
    }
}

impl fmt::Debug for TravelTestMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TravelTestMask({:?})", self.0)
    }
}

/// Open or close the live travel test (T1, T2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TravelTestRequest {
    /// T1: `21 00 00 00 18 02` + the mask at bytes 6-27.
    Open(TravelTestMask),
    /// T2: `21 00 00 00 18 03`.
    Close,
}

impl TravelTestRequest {
    /// The single packet, acked `21 00 00 00 01 00` (the trigger-write ack).
    pub fn step(&self) -> WriteStep {
        let packet = match self {
            TravelTestRequest::Open(mask) => {
                let mut payload = [0u8; 23];
                payload[0] = 0x02;
                payload[1..].copy_from_slice(mask.mask().as_bytes());
                WritePacket::new([0x21, 0x00, 0x00, 0x00, 0x18], &payload)
            }
            TravelTestRequest::Close => WritePacket::new([0x21, 0x00, 0x00, 0x00, 0x18], &[0x03]),
        };
        WriteStep {
            packet,
            ack: AckSpec::Report {
                header: [0x21, 0x00, 0x00, 0x00, 0x01],
                selector: Some(0x00),
                required: true,
            },
            gap_after: Duration::ZERO,
        }
    }
}

impl fmt::Display for TravelTestRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TravelTestRequest::Open(_) => f.write_str("open the travel test (21/18 02)"),
            TravelTestRequest::Close => f.write_str("close the travel test (21/18 03)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::W75;

    #[test]
    fn packets() {
        let open = TravelTestRequest::Open(TravelTestMask::full_layout(&W75).unwrap()).step();
        assert_eq!(
            open.packet.summary(),
            "21 00 00 00 18 02 3f 26 3f 1f 1f 1f 3f 1f 1f 1f 3f 3f 2f 31 3f 2f"
        );
        let close = TravelTestRequest::Close.step();
        assert_eq!(close.packet.summary(), "21 00 00 00 18 03");
        assert_eq!(open.ack, close.ack);
        assert!(open.ack.is_required());
        assert_eq!(
            TravelTestRequest::Close.to_string(),
            "close the travel test (21/18 03)"
        );
        let unknown = Model {
            product_code: "AP61HEARGB",
            ..W75
        };
        assert!(TravelTestMask::full_layout(&unknown).is_err());
    }
}
