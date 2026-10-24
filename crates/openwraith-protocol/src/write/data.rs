//! Table contents for writes: keymaps (`18`), macro slots (`19`), custom
//! colour table 1 (`09 00`) and the dead band (`26 01`).
//!
//! Every type here is built from a validated [`RawResponse`] (a reply read
//! from the keyboard or stored in a backup), never from a decoded model: the
//! bytes written are the bytes that were read, apart from the edits a caller
//! makes explicitly with the `set_*` methods.

use std::fmt;

use super::ids::AdvancedSlotId;
use crate::decode::Rgb;
use crate::error::WriteBuildError;
use crate::hex;
use crate::layout::{KeyPos, MATRIX_SIZE};
use crate::model::{Model, known_model};
use crate::request::{CustomTable, Layer, MacroSlotId, ReadRequest};
use crate::response::RawResponse;

/// Size of a keymap layer in bytes (132 entries x 4).
pub const KEYMAP_LEN: usize = MATRIX_SIZE * 4;
/// Size of a macro slot in bytes.
pub const MACRO_SLOT_LEN: usize = 256;
/// Size of a custom colour table in bytes (132 slots x RGB).
pub const COLOR_TABLE_LEN: usize = MATRIX_SIZE * 3;
/// Size of the dead-band table in bytes (132 keys x top, bottom).
pub const DEAD_BAND_LEN: usize = MATRIX_SIZE * 2;

/// Offset of the first macro step.
const MACRO_STEPS_OFFSET: usize = 8;
/// Largest step length (bytes) of a canonical macro (62 steps).
const MACRO_MAX_STEP_BYTES: u8 = 248;

/// The complete table bytes of `response` as a boxed array of exactly `N`
/// bytes. Chunk lengths are checked by [`RawResponse::table_bytes`].
fn table_array<const N: usize>(
    response: &RawResponse,
    what: &'static str,
) -> Result<Box<[u8; N]>, WriteBuildError> {
    let bytes = response.table_bytes(what)?;
    let got = bytes.len();
    bytes
        .into_boxed_slice()
        .try_into()
        .map_err(|_| WriteBuildError::TableLength {
            what,
            expected: N,
            got,
        })
}

fn wrong_request(response: &RawResponse, expected: &'static str) -> WriteBuildError {
    WriteBuildError::WrongRequest {
        expected,
        got: response.request(),
    }
}

/// Copies `N` bytes starting at `offset` (missing bytes are 0).
fn copy_at<const N: usize>(bytes: &[u8], offset: usize) -> [u8; N] {
    let mut out = [0u8; N];
    if let Some(src) = bytes.get(offset..offset + N) {
        out.copy_from_slice(src);
    }
    out
}

/// Overwrites `N` bytes starting at `offset` (ignored when out of range).
fn store_at<const N: usize>(bytes: &mut [u8], offset: usize, value: [u8; N]) {
    if let Some(dst) = bytes.get_mut(offset..offset + N) {
        dst.copy_from_slice(&value);
    }
}

/// Implements hex `Debug` and hex-string `Serialize` for a boxed-array newtype.
macro_rules! hex_table {
    ($ty:ident) => {
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    f,
                    concat!(stringify!($ty), "({})"),
                    hex::to_spaced_hex(&self.0[..])
                )
            }
        }

        #[cfg(feature = "serde")]
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&hex::to_hex(&self.0[..]))
            }
        }
    };
}

/// A complete keymap layer (528 bytes = 132 entries x 4; spec section 4.8),
/// written with `18 00` (base) or `18 02` (Fn).
///
/// Obtained from a `18 80` / `18 82` reply with
/// [`from_response`](Self::from_response); the only edit is
/// [`set_entry`](Self::set_entry) (read-modify-write). Entries the caller does
/// not change keep the device's bytes, including the knob slots 15-21 and Fn
/// (121): unlike the vendor site, nothing is filled in by default.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct KeymapData(Box<[u8; KEYMAP_LEN]>);

hex_table!(KeymapData);

impl KeymapData {
    /// The layer and table of a `18 80` (base) or `18 82` (Fn) reply.
    pub fn from_response(response: &RawResponse) -> Result<(Layer, KeymapData), WriteBuildError> {
        let ReadRequest::Keymap(layer) = response.request() else {
            return Err(wrong_request(response, "a keymap table"));
        };
        Ok((layer, KeymapData(table_array(response, "keymap")?)))
    }

    /// The 528 table bytes.
    pub fn as_bytes(&self) -> &[u8; KEYMAP_LEN] {
        &self.0
    }

    /// The 4-byte entry of the key at `pos`.
    pub fn entry(&self, pos: KeyPos) -> [u8; 4] {
        copy_at(&self.0[..], usize::from(pos.index()) * 4)
    }

    /// Replaces the 4-byte entry of the key at `pos`; every other byte is kept.
    pub fn set_entry(&mut self, pos: KeyPos, entry: [u8; 4]) {
        store_at(&mut self.0[..], usize::from(pos.index()) * 4, entry);
    }

    /// Every entry that refers to a macro slot (`10 xx SS 00`, slot =
    /// `SS & 0x1F`) or an advanced-key slot (`17 SS SS 00`), in index order
    /// (entry encoding CODE-ONLY, macro binding CONFIRMED by c13).
    ///
    /// Entries with command byte `10` or `17` that do not fit these forms
    /// (macro slot >= 10, advanced slot >= 40, `17` with different slot
    /// bytes, a non-zero last byte) are reported as
    /// [`KeymapReference::Malformed`], never silently accepted.
    pub fn references(&self) -> Vec<KeymapReference> {
        (0u8..)
            .zip(self.0.chunks_exact(4))
            .filter_map(|(index, entry)| reference(index, copy_at(entry, 0)))
            .collect()
    }
}

/// Classifies one keymap entry as a reference, if it is one.
fn reference(index: u8, entry: [u8; 4]) -> Option<KeymapReference> {
    let [b0, b1, b2, b3] = entry;
    let malformed = |reason| KeymapReference::Malformed {
        index,
        entry,
        reason,
    };
    match b0 {
        0x10 if b3 != 0 => Some(malformed("macro entry with a non-zero last byte")),
        0x10 => Some(match MacroSlotId::new(b2 & 0x1F) {
            Some(slot) => KeymapReference::Macro { index, slot },
            None => malformed("macro slot outside 0..=9"),
        }),
        0x17 if b1 != b2 => Some(malformed(
            "advanced-key entry with two different slot bytes",
        )),
        0x17 if b3 != 0 => Some(malformed("advanced-key entry with a non-zero last byte")),
        0x17 => Some(match AdvancedSlotId::new(b1) {
            Some(slot) => KeymapReference::Advanced { index, slot },
            None => malformed("advanced-key slot outside 0..=39"),
        }),
        _ => None,
    }
}

/// A keymap entry that refers to another table (see
/// [`KeymapData::references`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case", tag = "kind"))]
pub enum KeymapReference {
    /// `10 xx SS 00`: the key plays macro slot `SS & 0x1F`.
    Macro {
        /// Key index of the entry.
        index: u8,
        /// The referenced macro slot.
        slot: MacroSlotId,
    },
    /// `17 SS SS 00`: the key uses advanced-key slot `SS`.
    Advanced {
        /// Key index of the entry.
        index: u8,
        /// The referenced advanced-key slot.
        slot: AdvancedSlotId,
    },
    /// An entry with command byte `10` or `17` that fits neither form.
    Malformed {
        /// Key index of the entry.
        index: u8,
        /// The entry as stored.
        entry: [u8; 4],
        /// Which rule it breaks.
        reason: &'static str,
    },
}

impl KeymapReference {
    /// Key index of the entry.
    pub fn index(&self) -> u8 {
        match *self {
            KeymapReference::Macro { index, .. }
            | KeymapReference::Advanced { index, .. }
            | KeymapReference::Malformed { index, .. } => index,
        }
    }
}

/// The bytes of a macro slot in the canonical flat layout (spec section
/// 4.9.3; keyboard steps CONFIRMED by c13).
///
/// Only [`MacroData::from_response`] creates one, after checking that byte 1
/// (play mode) is at most 3, byte 2 is 0, byte 3 (step bytes `n`) is a
/// multiple of 4 in 4..=248, bytes 6-7 are 0, every step's second byte is
/// `00`, `01`, `10` or `11`, and bytes `8 + n ..` are 0.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CanonicalMacro(Box<[u8; MACRO_SLOT_LEN]>);

hex_table!(CanonicalMacro);

impl CanonicalMacro {
    /// The 256 slot bytes.
    pub fn as_bytes(&self) -> &[u8; MACRO_SLOT_LEN] {
        &self.0
    }

    /// Byte 3: step data length in bytes (4 per step).
    pub fn step_bytes(&self) -> u8 {
        self.0[3]
    }
}

/// Why `bytes` is not a canonical macro slot, or `None` if it is one.
fn macro_violation(bytes: &[u8; MACRO_SLOT_LEN]) -> Option<&'static str> {
    let n = bytes[3];
    if bytes[1] > 3 {
        return Some("play mode above 3");
    }
    if bytes[2] != 0 {
        return Some("byte 2 is not 0");
    }
    if !(4..=MACRO_MAX_STEP_BYTES).contains(&n) || n % 4 != 0 {
        return Some("step length is not a multiple of 4 in 4..=248");
    }
    if bytes[6] != 0 || bytes[7] != 0 {
        return Some("bytes 6-7 are not 0");
    }
    let steps_end = MACRO_STEPS_OFFSET + usize::from(n);
    let steps = bytes.get(MACRO_STEPS_OFFSET..steps_end).unwrap_or_default();
    if steps
        .chunks_exact(4)
        .any(|step| !matches!(step.get(1), Some(0x00 | 0x01 | 0x10 | 0x11)))
    {
        return Some("a step has an unknown press/release byte");
    }
    if bytes
        .get(steps_end..)
        .unwrap_or_default()
        .iter()
        .any(|b| *b != 0)
    {
        return Some("bytes after the steps are not 0");
    }
    None
}

/// The content of one macro slot, as written with `19 0n`.
///
/// An empty slot is written as 256 x `00`, the form the vendor site uses
/// (CONFIRMED: c09/c13 wrote zeros and read back zeros). A slot read as all
/// `FF` (the factory state) is also empty; `FF` is never written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MacroData {
    /// No macro; written as 256 x `00`.
    Empty,
    /// A macro in the canonical flat layout.
    Content(CanonicalMacro),
}

impl MacroData {
    /// The slot and content of a `19 8n` reply.
    ///
    /// All `00` and all `FF` are [`MacroData::Empty`]. Any other slot must be
    /// in the canonical flat layout ([`CanonicalMacro`]); otherwise this
    /// fails with [`WriteBuildError::NonCanonicalMacro`]. That also catches
    /// the "page" read layout the vendor parser supports (spec 4.9.1,
    /// CODE-ONLY), should the firmware use it.
    pub fn from_response(
        response: &RawResponse,
    ) -> Result<(MacroSlotId, MacroData), WriteBuildError> {
        let ReadRequest::Macro(slot) = response.request() else {
            return Err(wrong_request(response, "a macro slot"));
        };
        let bytes: Box<[u8; MACRO_SLOT_LEN]> = table_array(response, "macro slot")?;
        if is_empty_slot(&bytes[..]) {
            return Ok((slot, MacroData::Empty));
        }
        match macro_violation(&bytes) {
            None => Ok((slot, MacroData::Content(CanonicalMacro(bytes)))),
            Some(reason) => Err(WriteBuildError::NonCanonicalMacro {
                slot: slot.get(),
                reason,
            }),
        }
    }

    /// Whether this is [`MacroData::Empty`].
    pub fn is_empty(&self) -> bool {
        matches!(self, MacroData::Empty)
    }

    /// The 256 bytes that are written (all `00` for an empty slot).
    pub fn to_bytes(&self) -> [u8; MACRO_SLOT_LEN] {
        match self {
            MacroData::Empty => [0u8; MACRO_SLOT_LEN],
            MacroData::Content(content) => *content.as_bytes(),
        }
    }

    /// Whether the slot bytes as read (`slot_bytes`, the reply's table bytes)
    /// hold this content: an empty slot matches all `00` and all `FF`,
    /// content matches byte for byte.
    pub fn is_equivalent(&self, slot_bytes: &[u8]) -> bool {
        match self {
            MacroData::Empty => is_empty_slot(slot_bytes),
            MacroData::Content(content) => slot_bytes == &content.as_bytes()[..],
        }
    }
}

/// Whether `bytes` is an empty macro slot: 256 x `00` or 256 x `FF`.
fn is_empty_slot(bytes: &[u8]) -> bool {
    bytes.len() == MACRO_SLOT_LEN
        && (bytes.iter().all(|b| *b == 0x00) || bytes.iter().all(|b| *b == 0xFF))
}

/// Custom colour table 1 (396 bytes = 132 x RGB, slot = key index; spec
/// section 4.7.6), written with `09 00`.
///
/// Only a `09 80` (Custom 1) reply is accepted: writing Custom 2 (`09 01`) is
/// CODE-ONLY and not offered. Slots outside the key layout keep the device's
/// colours (the factory table lit slots 103 and 126).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ColorTableData(Box<[u8; COLOR_TABLE_LEN]>);

hex_table!(ColorTableData);

impl ColorTableData {
    /// The table of a `09 80` reply. Any other reply, including `09 81`
    /// (Custom 2), is refused.
    pub fn from_response(response: &RawResponse) -> Result<ColorTableData, WriteBuildError> {
        if response.request() != ReadRequest::CustomColors(CustomTable::Custom1) {
            return Err(wrong_request(response, "custom colour table 1"));
        }
        Ok(ColorTableData(table_array(response, "colour table")?))
    }

    /// The 396 table bytes.
    pub fn as_bytes(&self) -> &[u8; COLOR_TABLE_LEN] {
        &self.0
    }

    /// Colour of the slot of `pos`.
    pub fn color(&self, pos: KeyPos) -> Rgb {
        let [r, g, b] = copy_at(&self.0[..], usize::from(pos.index()) * 3);
        Rgb::new(r, g, b)
    }

    /// Replaces the colour of the slot of `pos`; every other slot is kept.
    pub fn set_color(&mut self, pos: KeyPos, color: Rgb) {
        store_at(
            &mut self.0[..],
            usize::from(pos.index()) * 3,
            [color.r, color.g, color.b],
        );
    }
}

/// The dead-band table (264 bytes = 132 x (top, bottom) in trigger units;
/// spec section 4.12), written with `26 01`. Layout and units CONFIRMED by
/// c12 (W top 0.10 mm = offset 92 `05`, bottom 0.20 mm = offset 93 `0a`).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DeadBandData(Box<[u8; DEAD_BAND_LEN]>);

hex_table!(DeadBandData);

impl DeadBandData {
    /// The table of a `26 00` reply.
    pub fn from_response(response: &RawResponse) -> Result<DeadBandData, WriteBuildError> {
        if response.request() != ReadRequest::DeadBand {
            return Err(wrong_request(response, "the dead-band table"));
        }
        Ok(DeadBandData(table_array(response, "dead band")?))
    }

    /// The 264 table bytes.
    pub fn as_bytes(&self) -> &[u8; DEAD_BAND_LEN] {
        &self.0
    }

    /// `[top, bottom]` of the key at `pos`, in trigger units.
    pub fn entry(&self, pos: KeyPos) -> [u8; 2] {
        copy_at(&self.0[..], usize::from(pos.index()) * 2)
    }

    /// Replaces `[top, bottom]` of the key at `pos`; every other byte is kept.
    pub fn set_entry(&mut self, pos: KeyPos, entry: [u8; 2]) {
        store_at(&mut self.0[..], usize::from(pos.index()) * 2, entry);
    }

    /// Largest dead-band value in trigger units: `ceil(1000 um / unit)`
    /// (the site's slider maximum of 1.00 mm, spec section 4.11.5). 50 on the
    /// W75 (unit 20 um). A unit of 0 gives 0.
    pub fn max_units(unit_micrometres: u32) -> u16 {
        if unit_micrometres == 0 {
            return 0;
        }
        u16::try_from(1000u32.div_ceil(unit_micrometres)).unwrap_or(u16::MAX)
    }

    /// Checks that every top and bottom value is at most `max_units`.
    pub fn validate(&self, max_units: u16) -> Result<(), WriteBuildError> {
        for (index, pair) in (0u8..).zip(self.0.chunks_exact(2)) {
            for (which, value) in ["top", "bottom"].into_iter().zip(pair) {
                if u16::from(*value) > max_units {
                    return Err(WriteBuildError::DeadBandOutOfRange {
                        index,
                        which,
                        value: *value,
                        max: max_units,
                    });
                }
            }
        }
        Ok(())
    }
}

/// A validated dead-band write (`26 01`): a [`DeadBandData`] whose every
/// value is within the keyboard's range and the model's ceiling.
///
/// Built only by [`DeadBandWrite::new`], so an unchecked table cannot reach
/// [`WriteRequest::DeadBand`](super::WriteRequest::DeadBand) (spec section
/// 4.11.5: values outside the range are never sent; whether the firmware
/// clamps them is unknown).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DeadBandWrite(DeadBandData);

impl DeadBandWrite {
    /// Checks every top and bottom value of `data` against `max_units` (see
    /// [`DeadBandData::max_units`]) and the dead-band ceiling of the model in
    /// [`MODELS`](crate::MODELS) with `model`'s product code (50 units =
    /// 1.00 mm on the W75), whichever is smaller.
    pub fn new(
        model: &Model,
        data: DeadBandData,
        max_units: u16,
    ) -> Result<DeadBandWrite, WriteBuildError> {
        let known = known_model(model).ok_or(WriteBuildError::UnknownModel {
            product: model.product_code,
        })?;
        data.validate(max_units.min(known.write_ceilings.max_dead_band))?;
        Ok(DeadBandWrite(data))
    }

    /// The table that is written.
    pub fn data(&self) -> &DeadBandData {
        &self.0
    }

    /// The 264 table bytes.
    pub fn as_bytes(&self) -> &[u8; DEAD_BAND_LEN] {
        self.0.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::table;
    use crate::model::W75;

    fn keymap_reply(layer: Layer, bytes: &[u8]) -> RawResponse {
        table(ReadRequest::Keymap(layer), bytes)
    }

    fn macro_reply(slot: u8, bytes: &[u8]) -> RawResponse {
        table(ReadRequest::Macro(MacroSlotId::new(slot).unwrap()), bytes)
    }

    /// c13 macro slot 0: "A down 100 ms, A up 100 ms, B down 100 ms, B up".
    fn c13_slot() -> Vec<u8> {
        let mut bytes = vec![0u8; 256];
        bytes[..24].copy_from_slice(&[
            0x00, 0x00, 0x00, 0x10, 0x00, 0x01, 0x00, 0x00, 0x04, 0x10, 0x00, 0x64, 0x04, 0x00,
            0x00, 0x64, 0x05, 0x10, 0x00, 0x64, 0x05, 0x00, 0x00, 0x00,
        ]);
        bytes
    }

    #[test]
    fn keymap_read_modify_write() {
        let (layer, mut data) =
            KeymapData::from_response(&keymap_reply(Layer::Fn, &[0u8; 528])).unwrap();
        assert_eq!(layer, Layer::Fn);
        let w = KeyPos::new(2, 2).unwrap();
        assert_eq!(data.entry(w), [0; 4]);
        data.set_entry(w, [0x00, 0x04, 0x00, 0x00]);
        assert_eq!(data.entry(w), [0x00, 0x04, 0x00, 0x00]);
        // Offset 184 = 46 x 4; nothing else changed.
        assert_eq!(data.as_bytes()[185], 0x04);
        assert_eq!(data.as_bytes().iter().filter(|b| **b != 0).count(), 1);
        // The last entry (index 131) is addressable.
        let last = KeyPos::from_index(131).unwrap();
        data.set_entry(last, [1, 2, 3, 4]);
        assert_eq!(&data.as_bytes()[524..], &[1, 2, 3, 4]);
    }

    #[test]
    fn keymap_rejects_other_replies() {
        let dead_band = table(ReadRequest::DeadBand, &[0u8; 264]);
        assert!(matches!(
            KeymapData::from_response(&dead_band),
            Err(WriteBuildError::WrongRequest { .. })
        ));
    }

    #[test]
    fn keymap_references() {
        let mut bytes = vec![0u8; 528];
        let mut put = |index: usize, entry: [u8; 4]| {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&entry);
        };
        put(16, [0x07, 0x12, 0, 0]); // knob default: not a reference
        put(66, [0x10, 0x39, 0x00, 0x00]); // Caps -> macro slot 0 (c13)
        put(67, [0x10, 0x39, 0x23, 0x00]); // slot 0x23 & 0x1F = 3
        put(68, [0x10, 0x39, 0x0A, 0x00]); // slot 10: malformed
        put(69, [0x17, 0x05, 0x05, 0x00]); // advanced slot 5
        put(70, [0x17, 0x05, 0x06, 0x00]); // different slot bytes
        put(71, [0x17, 0x28, 0x28, 0x00]); // slot 40: malformed
        put(72, [0x10, 0x39, 0x01, 0x01]); // non-zero last byte
        let (_, data) = KeymapData::from_response(&keymap_reply(Layer::Base, &bytes)).unwrap();
        let refs = data.references();
        let slot = |n| MacroSlotId::new(n).unwrap();
        assert_eq!(
            refs[..3],
            [
                KeymapReference::Macro {
                    index: 66,
                    slot: slot(0)
                },
                KeymapReference::Macro {
                    index: 67,
                    slot: slot(3)
                },
                KeymapReference::Malformed {
                    index: 68,
                    entry: [0x10, 0x39, 0x0A, 0x00],
                    reason: "macro slot outside 0..=9"
                },
            ]
        );
        assert_eq!(
            refs[3],
            KeymapReference::Advanced {
                index: 69,
                slot: AdvancedSlotId::new(5).unwrap()
            }
        );
        assert!(matches!(
            refs[4],
            KeymapReference::Malformed { index: 70, .. }
        ));
        assert!(matches!(
            refs[5],
            KeymapReference::Malformed { index: 71, .. }
        ));
        assert!(matches!(
            refs[6],
            KeymapReference::Malformed { index: 72, .. }
        ));
        assert_eq!(refs.len(), 7);
        assert_eq!(refs[6].index(), 72);
    }

    #[test]
    fn macro_empty_forms() {
        for fill in [0x00u8, 0xFF] {
            let (slot, data) = MacroData::from_response(&macro_reply(7, &[fill; 256])).unwrap();
            assert_eq!(slot.get(), 7);
            assert_eq!(data, MacroData::Empty);
            assert_eq!(data.to_bytes(), [0u8; 256], "FF is never written");
        }
        assert!(MacroData::Empty.is_equivalent(&[0x00; 256]));
        assert!(MacroData::Empty.is_equivalent(&[0xFF; 256]));
        assert!(!MacroData::Empty.is_equivalent(&[0x00; 255]));
        let mut mixed = [0u8; 256];
        mixed[0] = 0xFF;
        assert!(!MacroData::Empty.is_equivalent(&mixed));
    }

    #[test]
    fn macro_c13_slot_is_canonical() {
        let bytes = c13_slot();
        let (_, data) = MacroData::from_response(&macro_reply(0, &bytes)).unwrap();
        let MacroData::Content(content) = &data else {
            panic!("expected content, got {data:?}");
        };
        assert_eq!(content.step_bytes(), 16);
        assert_eq!(&data.to_bytes()[..], &bytes[..]);
        assert!(data.is_equivalent(&bytes));
        assert!(!data.is_equivalent(&[0u8; 256]));
        assert!(!MacroData::Empty.is_equivalent(&bytes));
    }

    #[test]
    fn non_canonical_macros_are_refused() {
        let base = c13_slot();
        let cases: [(usize, u8, &str); 7] = [
            (1, 4, "play mode above 3"),
            (2, 1, "byte 2 is not 0"),
            (3, 0x0E, "step length is not a multiple of 4 in 4..=248"),
            (3, 252, "step length is not a multiple of 4 in 4..=248"),
            (6, 1, "bytes 6-7 are not 0"),
            (9, 0x20, "a step has an unknown press/release byte"),
            (24, 0x01, "bytes after the steps are not 0"),
        ];
        for (offset, value, reason) in cases {
            let mut bytes = base.clone();
            bytes[offset] = value;
            assert_eq!(
                MacroData::from_response(&macro_reply(2, &bytes)),
                Err(WriteBuildError::NonCanonicalMacro { slot: 2, reason }),
                "offset {offset}"
            );
        }
        // Step length 0 with other content is not empty either.
        let mut bytes = vec![0u8; 256];
        bytes[0] = 0x01;
        assert!(MacroData::from_response(&macro_reply(0, &bytes)).is_err());
        // The largest canonical slot: 62 steps.
        let mut bytes = vec![0u8; 256];
        bytes[3] = 248;
        bytes[8..].iter_mut().step_by(4).for_each(|b| *b = 0x04);
        assert!(MacroData::from_response(&macro_reply(9, &bytes)).is_ok());
    }

    #[test]
    fn color_table_accepts_only_custom1() {
        let custom2 = table(ReadRequest::CustomColors(CustomTable::Custom2), &[0u8; 396]);
        assert!(matches!(
            ColorTableData::from_response(&custom2),
            Err(WriteBuildError::WrongRequest { .. })
        ));
        let mut bytes = vec![0u8; 396];
        bytes[103 * 3] = 0xFF;
        let mut data = ColorTableData::from_response(&table(
            ReadRequest::CustomColors(CustomTable::Custom1),
            &bytes,
        ))
        .unwrap();
        let w = KeyPos::new(2, 2).unwrap();
        data.set_color(w, Rgb::new(1, 2, 3));
        assert_eq!(data.color(w), Rgb::new(1, 2, 3));
        assert_eq!(&data.as_bytes()[138..141], &[1, 2, 3]);
        // A slot outside the layout keeps its colour.
        assert_eq!(data.as_bytes()[309], 0xFF);
    }

    #[test]
    fn dead_band_entries_and_limits() {
        let mut data =
            DeadBandData::from_response(&table(ReadRequest::DeadBand, &[0u8; 264])).unwrap();
        let w = KeyPos::new(2, 2).unwrap();
        data.set_entry(w, [5, 10]);
        assert_eq!(&data.as_bytes()[92..94], &[5, 10]);
        assert_eq!(data.entry(w), [5, 10]);
        assert_eq!(DeadBandData::max_units(20), 50);
        assert_eq!(DeadBandData::max_units(30), 34);
        assert_eq!(DeadBandData::max_units(2), 500);
        assert_eq!(DeadBandData::max_units(0), 0);
        assert_eq!(data.validate(50), Ok(()));
        data.set_entry(KeyPos::from_index(131).unwrap(), [0, 51]);
        assert_eq!(
            data.validate(50),
            Err(WriteBuildError::DeadBandOutOfRange {
                index: 131,
                which: "bottom",
                value: 51,
                max: 50
            })
        );
        assert!(DeadBandData::from_response(&keymap_reply(Layer::Base, &[0u8; 528])).is_err());
    }

    #[test]
    fn dead_band_writes_are_validated_when_built() {
        let zero = DeadBandData::from_response(&table(ReadRequest::DeadBand, &[0u8; 264])).unwrap();
        let w = KeyPos::new(2, 2).unwrap();
        let mut ok = zero.clone();
        ok.set_entry(w, [5, 10]);
        let write = DeadBandWrite::new(&W75, ok.clone(), 50).unwrap();
        assert_eq!(write.data(), &ok);
        assert_eq!(&write.as_bytes()[92..94], &[5, 10]);
        // 0x33 = 51 units at offset 92 (W top), and 5.10 / 4.00 mm.
        for entry in [[0x33, 0], [0xff, 0xc8]] {
            let mut bad = zero.clone();
            bad.set_entry(w, entry);
            assert!(matches!(
                DeadBandWrite::new(&W75, bad, 50),
                Err(WriteBuildError::DeadBandOutOfRange { index: 46, .. })
            ));
        }
        // A caller-supplied maximum above the model's ceiling is capped.
        let mut big = zero.clone();
        big.set_entry(w, [60, 60]);
        assert!(matches!(
            DeadBandWrite::new(&W75, big.clone(), 500),
            Err(WriteBuildError::DeadBandOutOfRange { max: 50, .. })
        ));
        // A smaller keyboard maximum is kept.
        assert!(DeadBandWrite::new(&W75, ok.clone(), 4).is_err());
        // An unknown model is refused.
        let unknown = Model {
            product_code: "AP61HEARGB",
            ..W75
        };
        assert_eq!(
            DeadBandWrite::new(&unknown, zero, 50),
            Err(WriteBuildError::UnknownModel {
                product: "AP61HEARGB"
            })
        );
    }

    #[test]
    fn chunk_lengths_are_checked() {
        use crate::decode::test_util::report;
        // A dead-band reply whose last chunk declares 58 bytes instead of 32.
        let reports = (0..5u8).map(|i| report(&[0x26, 0x00, 0, i, 0x3A]));
        let raw = RawResponse::from_reports(ReadRequest::DeadBand, reports).unwrap();
        assert!(matches!(
            DeadBandData::from_response(&raw),
            Err(WriteBuildError::Decode(_))
        ));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serializes_as_hex() {
        let (_, data) = MacroData::from_response(&macro_reply(0, &c13_slot())).unwrap();
        let json = serde_json::to_string(&data).unwrap();
        assert!(
            json.starts_with(r#"{"content":"00000010000100000410"#),
            "{json}"
        );
        assert_eq!(
            serde_json::to_string(&MacroData::Empty).unwrap(),
            r#""empty""#
        );
        let reference = KeymapReference::Macro {
            index: 66,
            slot: MacroSlotId::new(0).unwrap(),
        };
        assert_eq!(
            serde_json::to_string(&reference).unwrap(),
            r#"{"kind":"macro","index":66,"slot":0}"#
        );
    }
}
