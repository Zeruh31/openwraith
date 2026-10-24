//! Pure implementation of the Wraith W75 HID configuration protocol.
//!
//! This crate does no I/O. It knows how to build **read** requests, how to
//! recognise the input reports that answer them, and how to decode the
//! collected reports into typed models. Talking to a real device is the job of
//! the `openwraith-hid` crate.
//!
//! The protocol is described in `docs/PROTOCOL.md` (our own document, written
//! from USB captures and an analysis of the vendor's web configurator). Section
//! numbers in the documentation of this crate refer to that file, and every
//! decoded field notes the confidence label the spec gives it
//! (CONFIRMED / OBSERVED / CODE-ONLY / GUESS).
//!
//! # Read-only by construction
//!
//! Phase 1 of OpenWraith must never change anything on the keyboard. The crate
//! enforces this structurally:
//!
//! * An [`OutPacket`] (the only thing a transport can send) has no public
//!   constructor. The only way to obtain one is [`ReadRequest::packet`].
//! * [`ReadRequest`] has one variant per *read* command of the spec and no
//!   others. Its payload types ([`KeyPos`], [`MacroSlotId`], ...) validate their
//!   ranges, so every constructible request is a valid read.
//! * A unit test enumerates every constructible request
//!   ([`ReadRequest::all`]) and checks that its bytes are one of the
//!   documented read forms.
//!
//! ```compile_fail
//! // Arbitrary packets cannot be built outside this crate.
//! let packet = openwraith_protocol::OutPacket([0x14; 63]);
//! ```
//!
//! # Typical flow
//!
//! ```
//! use openwraith_protocol::{Offer, ReadRequest, Report, DeviceInfo, FromResponse};
//!
//! // 1. Build the request and send `request.packet()` with a transport.
//! let request = ReadRequest::DeviceInfo;
//! assert_eq!(request.packet().as_bytes()[0], 0x0D);
//!
//! // 2. Feed every input report to a collector until it is complete.
//! let mut collector = request.collector();
//! let mut chunk0 = [0u8; 63];
//! chunk0[..5].copy_from_slice(&[0x0D, 0x00, 0x00, 0x00, 0x32]);
//! chunk0[5..37].copy_from_slice(b"W669,34,KB,ML,W75HEARGB,V3.22.07");
//! let mut chunk1 = [0u8; 63];
//! chunk1[..5].copy_from_slice(&[0x0D, 0x00, 0x00, 0x01, 0x32]);
//! chunk1[5..25].copy_from_slice(b"Aug  6 2026,13:46:54");
//! for bytes in [chunk0, chunk1] {
//!     assert_eq!(collector.offer(&Report::new(bytes)), Offer::Accepted);
//! }
//! assert!(collector.is_complete());
//!
//! // 3. Decode.
//! let response = collector.finish()?;
//! let info = DeviceInfo::from_response(&response)?;
//! assert_eq!(info.product.as_deref(), Some("W75HEARGB"));
//! assert_eq!(info.firmware.as_deref(), Some("3.22.07"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(feature = "capture")]
pub mod capture;
pub mod decode;
mod error;
pub mod event;
pub mod frame;
mod hex;
pub mod layout;
pub mod model;
pub mod request;
pub mod response;
pub mod units;
pub mod usage;

pub use decode::{
    ActiveCustomTable, ActiveProfile, AdvancedKey, AdvancedKeySlot, AdvancedKeys, CapabilityFlagsA,
    CapabilityFlagsB, ColorTable, DeadBand, DeadBandTable, Decoded, DeviceInfo, FnLockedKeys,
    FromResponse, KeyAction, KeyTrigger, KeymapEntry, KeymapTable, LightDirection, LightEffect,
    LightMode, LightModeList, LinkType, MacroContent, MacroPlayMode, MacroSlot, MacroStep,
    PollRate, Rgb, SocdEntry, SocdPairs, SocdSettings, Status, SwitchTypeList, SwitchTypeTable,
    TriggerGlobal, TriggerMode, WinLockOptions, decode, switch_type_name,
};
pub use error::{DecodeError, FrameError, ResponseError};
pub use event::{DeviceEvent, is_keyboard_reset};
pub use frame::{HEADER_LEN, MAX_CHUNK_PAYLOAD, OutPacket, REPORT_ID, REPORT_LEN, Report};
pub use hex::HexBytes;
pub use layout::{
    KEY_MASK_LEN, Key, KeyMask, KeyPos, MATRIX_COLS, MATRIX_ROWS, MATRIX_SIZE, W75_KEYS,
};
pub use model::{
    CONFIG_USAGE, CONFIG_USAGE_PAGE, MODELS, Model, PRODUCT_IDS, VENDOR_ID, W75, model_for_product,
};
pub use request::{
    CustomTable, Layer, MacroSlotId, ReadRequest, ResponseSpec, SubMatch, TableLayout,
};
pub use response::{Offer, RawResponse, ResponseCollector};
pub use units::Distance;
