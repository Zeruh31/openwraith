//! Writes (Phase 2): the nine write forms confirmed on the keyboard.
//!
//! This module is the only place in the workspace that can produce bytes
//! which change the keyboard. It is deliberately narrow:
//!
//! * A [`WritePacket`] is a separate type from the read-only
//!   [`OutPacket`](crate::OutPacket). It has no public constructor, no
//!   `From`, no `Deserialize` and no mutable access; the only way to obtain
//!   one is [`WriteRequest::steps`]. There is no conversion between the two
//!   packet types, so read-only code (which only ever handles `OutPacket`s)
//!   cannot send a write.
//! * [`WriteRequest`] has one variant per write form below, and nothing
//!   else. Every parameter is a validated type, so every constructible
//!   request produces one of these forms. `WriteRequest` is `Serialize` (for
//!   plans and dry runs) but never `Deserialize`: a stored plan cannot be
//!   executed.
//! * A unit test enumerates a large sample of constructible requests and
//!   checks every packet against an independent description of the forms.
//!
//! | # | Write | Bytes (rest of the 63 bytes = 0) | Ack | Variant |
//! |---|---|---|---|---|
//! | W1 | Select profile | `21 00 00 00 00 12 PP`, PP 1..=4 | `21 00 00 00 02 12` + push `21 00 00 00 03 11 PP 00` | [`WriteRequest::SelectProfile`] |
//! | W2 | Music-rhythm leave | `17 00 00 00 02 00 00`, only right before a `07 00` that changes the mode | `17 00 00 00 00` (optional) | first step of [`WriteRequest::MainLight`] |
//! | W3 | Main light | `07 00 00 00 0E` + 12-byte record + `00 00` | none | [`WriteRequest::MainLight`] |
//! | W4 | Custom colours 1 | 8 x `09 00 00 cc LL` + table, LL = `36` x 7, `12` | `09 00 00 00 00` per chunk | [`WriteRequest::CustomColors1`] |
//! | W5 | Keymap | 10 x `18 SS 00 cc LL` + table, SS `00` base / `02` Fn, LL = `38` x 9, `18` | `18 00 00 00 00` (both layers) | [`WriteRequest::Keymap`] |
//! | W6 | Macro slot n | 5 x `19 0n 00 cc LL` + slot, LL = `3A` x 4, `18` | `19 00 00 00 00` (every slot) | [`WriteRequest::Macro`] |
//! | W7 | Advanced-key clear | `22 SS 00 00 32` + 50 x `00`, SS `00`..`27` | `22 00 00 00 00` | [`WriteRequest::AdvancedKeyClear`] |
//! | W8 | Trigger | `21 00 00 00 18 MM` + mask (bytes 6-27) + `A A P R Ah Ah Ph Rh` (bytes 28-35), MM `00`/`0C`/`0D` | `21 00 00 00 01 00` | [`WriteRequest::Trigger`] |
//! | W9 | Dead band | 5 x `26 01 00 cc LL` + table, LL = `3A` x 4, `20` | `26 01 00 00 00` | [`WriteRequest::DeadBand`] |
//!
//! All nine are CONFIRMED on hardware (captures c02, c03, c08-c13). Acks
//! never carry a chunk index, so a chunk can only be matched to its ack by
//! sending one packet at a time and waiting (stop-and-wait).
//!
//! Everything else stays impossible to build here: any `0x14` (reset,
//! Win-Lock, 6-key, gamepad, sleep), music-rhythm enter, the side light
//! (`08 00`), custom table 2 (`09 01`), SOCD (`24`), switch types (`25 01`),
//! reset triggers (`21/06`), calibration (`21` `07`/`08`/`0F`/`10`), the
//! travel test (`21` `02`/`03`), set poll rate (`21/09`), an empty-mask
//! trigger write and a non-empty advanced-key slot.
//!
//! # Example
//!
//! ```
//! use openwraith_protocol::write::{ProfileId, WriteRequest};
//!
//! let request = WriteRequest::SelectProfile(ProfileId::new(2).unwrap());
//! let steps = request.steps();
//! assert_eq!(steps.len(), 1);
//! assert_eq!(steps[0].packet.as_bytes()[..8], [0x21, 0, 0, 0, 0, 0x12, 0x02, 0]);
//! // Plans and dry runs can be serialized for display.
//! assert_eq!(serde_json::to_string(&request)?, r#"{"select_profile":2}"#);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Write packets cannot be built from arbitrary bytes:
//!
//! ```compile_fail
//! let packet = openwraith_protocol::write::WritePacket([0x14; 63]);
//! ```
//!
//! and a write request cannot be deserialized (a serialized plan is for
//! display only):
//!
//! ```compile_fail
//! let request: openwraith_protocol::write::WriteRequest =
//!     serde_json::from_str(r#"{"select_profile":2}"#).unwrap();
//! ```

mod data;
mod ids;
mod light;
mod trigger;

#[cfg(test)]
mod tests;

use std::fmt;
use std::time::Duration;

pub use crate::error::WriteBuildError;
pub use data::{
    COLOR_TABLE_LEN, CanonicalMacro, ColorTableData, DEAD_BAND_LEN, DeadBandData, DeadBandWrite,
    KEYMAP_LEN, KeymapData, KeymapReference, MACRO_SLOT_LEN, MacroData,
};
pub use ids::{AdvancedSlotId, ProfileId};
pub use light::{LIGHT_RECORD_LEN, LightLimits, LightRecord, MainLightWrite};
pub use trigger::{TriggerGrouping, TriggerLimits, TriggerSetting, TriggerWrite, TriggerWriteMode};

use crate::frame::{HEADER_LEN, REPORT_ID, REPORT_LEN, Report, trimmed};
use crate::hex::{self, HexBytes};
use crate::request::{CustomTable, Layer, MacroSlotId, ReadRequest};
use crate::response::RawResponse;

/// Pause after each custom-colour chunk (the vendor site sleeps 5 ms,
/// spec section 2.6).
pub const COLOR_CHUNK_GAP: Duration = Duration::from_millis(5);

/// Settle time after a main-light write, which is not acknowledged.
pub const LIGHT_SETTLE: Duration = Duration::from_millis(20);

/// Bytes per advanced-key slot in the `22 80` read (the write carries 50).
const ADVANCED_READ_SLOT_LEN: usize = 40;

/// Payload length of an advanced-key write (`0x32` = 50 bytes).
const ADVANCED_WRITE_LEN: u8 = 0x32;

/// A 63-byte output report that changes the keyboard.
///
/// There is deliberately **no public constructor**: the only way to obtain a
/// `WritePacket` is [`WriteRequest::steps`], which only produces the nine
/// confirmed write forms. It is a different type from the read-only
/// [`OutPacket`](crate::OutPacket), and neither converts into the other.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct WritePacket([u8; REPORT_LEN]);

impl WritePacket {
    /// Header + payload, zero padded. Private to this module.
    fn new(header: [u8; HEADER_LEN], payload: &[u8]) -> Self {
        let mut bytes = [0u8; REPORT_LEN];
        bytes[..HEADER_LEN].copy_from_slice(&header);
        for (dst, src) in bytes[HEADER_LEN..].iter_mut().zip(payload) {
            *dst = *src;
        }
        WritePacket(bytes)
    }

    /// The 63 payload bytes (byte 0 = command), without the report ID.
    pub fn as_bytes(&self) -> &[u8; REPORT_LEN] {
        &self.0
    }

    /// The 64-byte buffer to hand to `hid_write`: report ID 1 followed by the
    /// 63 payload bytes (spec section 2.2).
    pub fn to_hid_report(&self) -> [u8; REPORT_LEN + 1] {
        let mut out = [0u8; REPORT_LEN + 1];
        out[0] = REPORT_ID;
        out[1..].copy_from_slice(&self.0);
        out
    }

    /// Command byte (byte 0).
    pub fn cmd(&self) -> u8 {
        self.0[0]
    }

    /// Sub-command byte (byte 1).
    pub fn sub(&self) -> u8 {
        self.0[1]
    }

    /// Spaced hex of the bytes up to the last non-zero one (at least the
    /// 5-byte header), e.g. `21 00 00 00 00 12 02`.
    pub fn summary(&self) -> String {
        hex::to_spaced_hex(trimmed(&self.0))
    }
}

impl fmt::Debug for WritePacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WritePacket({})", self.summary())
    }
}

impl fmt::Display for WritePacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for WritePacket {
    /// All 63 bytes as lowercase hex, like [`Report`].
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::to_hex(&self.0))
    }
}

/// How the keyboard acknowledges one write packet (spec section 2.7 and the
/// table in the [module documentation](self)).
///
/// Acks are matched on bytes 0-4, plus byte 5 for the `0x21` family; bytes
/// after the declared length are stale and ignored. An ack never carries a
/// chunk index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AckSpec {
    /// No reply is sent (main light `07 00`, CONFIRMED: 10 writes, 0 replies).
    None,
    /// One report whose bytes 0-4 equal `header` (and byte 5 equals
    /// `selector`, if given).
    Report {
        /// Bytes 0-4 of the ack.
        header: [u8; HEADER_LEN],
        /// Required byte 5 (the `0x21` family).
        selector: Option<u8>,
        /// Whether a missing ack is an error. `false` only for the
        /// music-rhythm leave, whose ack the vendor site does not wait for.
        required: bool,
    },
    /// The profile-select ack `21 00 00 00 02 12` and the push
    /// `21 00 00 00 03 11 PP 00` (bytes 4 and 7 ignored), in either order
    /// (c08, c10: ack and push 3-4 ms after the select).
    ProfileSelected(ProfileId),
}

impl AckSpec {
    /// Whether a missing ack is an error.
    pub fn is_required(&self) -> bool {
        match self {
            AckSpec::None => false,
            AckSpec::Report { required, .. } => *required,
            AckSpec::ProfileSelected(_) => true,
        }
    }

    /// Whether the keyboard sends anything at all for this packet.
    pub fn expects_reply(&self) -> bool {
        !matches!(self, AckSpec::None)
    }

    /// A tracker that recognises the ack (and push) of one packet.
    pub fn tracker(&self) -> AckTracker {
        AckTracker {
            spec: *self,
            ack: false,
            push: false,
        }
    }
}

/// The required `Report` ack spec with `header`.
const fn ack(header: [u8; HEADER_LEN]) -> AckSpec {
    AckSpec::Report {
        header,
        selector: None,
        required: true,
    }
}

/// What an [`AckTracker`] did with an offered report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AckOffer {
    /// The report is the expected ack.
    Ack,
    /// The report is the expected profile push (profile select only).
    Push,
    /// The report is not (or no longer) expected for this packet.
    Ignored,
}

/// Recognises the acknowledgement of one write packet; created by
/// [`AckSpec::tracker`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AckTracker {
    spec: AckSpec,
    ack: bool,
    push: bool,
}

impl AckTracker {
    /// The spec this tracker follows.
    pub fn spec(&self) -> AckSpec {
        self.spec
    }

    /// Offers an input report. The first matching ack (and, for a profile
    /// select, the first matching push) is taken; anything else, including a
    /// second copy, is [`AckOffer::Ignored`].
    pub fn offer(&mut self, report: &Report) -> AckOffer {
        match self.spec {
            AckSpec::None => AckOffer::Ignored,
            AckSpec::Report {
                header, selector, ..
            } => {
                let bytes = report.as_bytes();
                let matches = bytes[..HEADER_LEN] == header
                    && selector.is_none_or(|s| report.byte(HEADER_LEN) == s);
                if matches && !self.ack {
                    self.ack = true;
                    AckOffer::Ack
                } else {
                    AckOffer::Ignored
                }
            }
            AckSpec::ProfileSelected(profile) => {
                if !self.ack && is_select_ack(report) {
                    self.ack = true;
                    AckOffer::Ack
                } else if !self.push && is_profile_push(report, profile) {
                    self.push = true;
                    AckOffer::Push
                } else {
                    AckOffer::Ignored
                }
            }
        }
    }

    /// Whether everything expected has arrived (always true for
    /// [`AckSpec::None`]).
    pub fn is_complete(&self) -> bool {
        match self.spec {
            AckSpec::None => true,
            AckSpec::Report { .. } => self.ack,
            AckSpec::ProfileSelected(_) => self.ack && self.push,
        }
    }

    /// Whether the ack arrived.
    pub fn ack_received(&self) -> bool {
        self.ack
    }

    /// Whether the profile push arrived (profile select only).
    pub fn push_received(&self) -> bool {
        self.push
    }
}

/// `21 00 00 00 02 12 ..`: the profile-select ack (byte 6 = `01` in every
/// sample, meaning unknown, not checked).
fn is_select_ack(report: &Report) -> bool {
    report.as_bytes()[..6] == [0x21, 0x00, 0x00, 0x00, 0x02, 0x12]
}

/// `21 00 00 00 .. 11 PP ..`: a profile report naming `profile`.
fn is_profile_push(report: &Report, profile: ProfileId) -> bool {
    let b = report.as_bytes();
    b[..4] == [0x21, 0x00, 0x00, 0x00] && b[5] == 0x11 && b[6] == profile.get()
}

/// Whether `report` has the shape of an acknowledgement of one of the write
/// forms: `21 00 00 00 01 00` (trigger write; also the vendor site's
/// empty-mask write, reset triggers and travel-test close),
/// `21 00 00 00 02 12` (profile select), `09`/`17`/`18`/`19`/`22`
/// `00 00 00 00` and `26 01 00 00 00`.
///
/// A session uses this to notice acknowledgements of another program's
/// writes. Note that `09 00 00 00 00` is also the reply to the `09 20` read
/// (active custom table, spec section 4.7.6): a caller that sent that read
/// must account for it.
pub fn is_write_ack(report: &Report) -> bool {
    let b = report.as_bytes();
    match b[..HEADER_LEN] {
        [0x21, 0x00, 0x00, 0x00, 0x01] => b[5] == 0x00,
        [0x21, 0x00, 0x00, 0x00, 0x02] => b[5] == 0x12,
        [0x09 | 0x17 | 0x18 | 0x19 | 0x22, 0x00, 0x00, 0x00, 0x00] => true,
        [0x26, 0x01, 0x00, 0x00, 0x00] => true,
        _ => false,
    }
}

/// One packet of a write, with its ack and the pause after it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct WriteStep {
    /// The packet to send.
    pub packet: WritePacket,
    /// How the keyboard acknowledges it.
    pub ack: AckSpec,
    /// Pause after the packet (and its ack): [`COLOR_CHUNK_GAP`] after each
    /// colour chunk, [`LIGHT_SETTLE`] after a main-light write, else zero.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "gap_after_ms", serialize_with = "serialize_millis")
    )]
    pub gap_after: Duration,
}

#[cfg(feature = "serde")]
fn serialize_millis<S: serde::Serializer>(
    duration: &Duration,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_u64(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
}

/// A table (or setting) of the keyboard, as named in restore plans and
/// reports. Covers every table a backup holds, including those Phase 2 never
/// writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TableId {
    /// The active onboard profile (`21/11`, written with `21/12`).
    Profile,
    /// Main light effect record (`07`).
    MainLight,
    /// Side light effect record (`08`; never written).
    SideLight,
    /// Active custom colour table (`09 20`; no write command known).
    ActiveCustomTable,
    /// Custom colour table 1 or 2 (`09 80` / `09 81`; only 1 is written).
    CustomColors {
        /// Which table.
        table: CustomTable,
    },
    /// Keymap layer (`18`).
    Keymap {
        /// Which layer.
        layer: Layer,
    },
    /// One macro slot (`19`).
    Macro {
        /// Which slot.
        slot: MacroSlotId,
    },
    /// One advanced-key slot (`22`; only cleared).
    AdvancedKey {
        /// Which slot.
        slot: AdvancedSlotId,
    },
    /// Per-key trigger settings (`21/05`, written with the `21` trigger write).
    Triggers,
    /// Dead band (`26`).
    DeadBand,
    /// SOCD pairs (`24 01`; never written).
    SocdPairs,
    /// SOCD power and hotkey (`24 02`; never written).
    SocdSettings,
    /// Per-key switch types (`25 02`; never written).
    SwitchTypes,
    /// Polling rate (`21/09`; never written).
    PollRate,
    /// Win-Lock options and N-key / 6-key rollover (status bytes 20 and 22;
    /// never written).
    WinLockAndRollover,
}

impl fmt::Display for TableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TableId::Profile => f.write_str("active profile"),
            TableId::MainLight => f.write_str("main light"),
            TableId::SideLight => f.write_str("side light"),
            TableId::ActiveCustomTable => f.write_str("active custom colour table"),
            TableId::CustomColors { table } => {
                write!(f, "custom colours {}", table.number() + 1)
            }
            TableId::Keymap { layer: Layer::Base } => f.write_str("base keymap"),
            TableId::Keymap { layer: Layer::Fn } => f.write_str("Fn keymap"),
            TableId::Macro { slot } => write!(f, "macro slot {}", slot.get()),
            TableId::AdvancedKey { slot } => write!(f, "advanced-key slot {slot}"),
            TableId::Triggers => f.write_str("triggers"),
            TableId::DeadBand => f.write_str("dead band"),
            TableId::SocdPairs => f.write_str("SOCD pairs"),
            TableId::SocdSettings => f.write_str("SOCD settings"),
            TableId::SwitchTypes => f.write_str("switch types"),
            TableId::PollRate => f.write_str("polling rate"),
            TableId::WinLockAndRollover => f.write_str("Win-Lock and rollover"),
        }
    }
}

/// Every write Phase 2 can send (see the [module documentation](self)).
///
/// Each variant holds validated data, so every constructible request
/// produces only the confirmed write forms. Serialize only.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum WriteRequest {
    /// W1: select an onboard profile.
    SelectProfile(ProfileId),
    /// W3, preceded by W2 when the mode changes.
    MainLight(MainLightWrite),
    /// W4: custom colour table 1 (there is no way to write table 2).
    CustomColors1(ColorTableData),
    /// W5: a whole keymap layer.
    Keymap(Layer, KeymapData),
    /// W6: one macro slot.
    Macro(MacroSlotId, MacroData),
    /// W7: clear one advanced-key slot (all-zero content; non-empty slots
    /// cannot be written: their write format is CODE-ONLY).
    AdvancedKeyClear(AdvancedSlotId),
    /// W8: trigger settings for a set of keys.
    Trigger(TriggerWrite),
    /// W9: the whole dead-band table, validated against the keyboard's range
    /// and the model's ceiling ([`DeadBandWrite::new`]).
    DeadBand(DeadBandWrite),
}

impl WriteRequest {
    /// The packets to send, in order, each with its ack and the pause after
    /// it. Send them one at a time: wait for each ack before the next packet.
    pub fn steps(&self) -> Vec<WriteStep> {
        let step = |packet, ack, gap_after| WriteStep {
            packet,
            ack,
            gap_after,
        };
        match self {
            WriteRequest::SelectProfile(profile) => vec![step(
                WritePacket::new([0x21, 0x00, 0x00, 0x00, 0x00], &[0x12, profile.get()]),
                AckSpec::ProfileSelected(*profile),
                Duration::ZERO,
            )],
            WriteRequest::MainLight(write) => {
                let mut steps = Vec::with_capacity(2);
                if write.changes_mode() {
                    steps.push(step(
                        WritePacket::new([0x17, 0x00, 0x00, 0x00, 0x02], &[0x00, 0x00]),
                        AckSpec::Report {
                            header: [0x17, 0x00, 0x00, 0x00, 0x00],
                            selector: None,
                            required: false,
                        },
                        Duration::ZERO,
                    ));
                }
                steps.push(step(
                    WritePacket::new([0x07, 0x00, 0x00, 0x00, 0x0E], write.record().as_bytes()),
                    AckSpec::None,
                    LIGHT_SETTLE,
                ));
                steps
            }
            WriteRequest::CustomColors1(data) => chunked(
                [0x09, 0x00],
                data.as_bytes(),
                54,
                ack([0x09, 0x00, 0x00, 0x00, 0x00]),
                COLOR_CHUNK_GAP,
            ),
            WriteRequest::Keymap(layer, data) => chunked(
                [0x18, keymap_write_sub(*layer)],
                data.as_bytes(),
                56,
                ack([0x18, 0x00, 0x00, 0x00, 0x00]),
                Duration::ZERO,
            ),
            WriteRequest::Macro(slot, data) => chunked(
                [0x19, slot.get()],
                &data.to_bytes(),
                58,
                ack([0x19, 0x00, 0x00, 0x00, 0x00]),
                Duration::ZERO,
            ),
            WriteRequest::AdvancedKeyClear(slot) => vec![step(
                WritePacket::new([0x22, slot.get(), 0x00, 0x00, ADVANCED_WRITE_LEN], &[]),
                ack([0x22, 0x00, 0x00, 0x00, 0x00]),
                Duration::ZERO,
            )],
            WriteRequest::Trigger(write) => {
                let setting = write.setting();
                write
                    .packet_masks()
                    .into_iter()
                    .map(|mask| {
                        let mut payload = [0u8; 31];
                        payload[0] = setting.mode.byte();
                        payload[1..23].copy_from_slice(mask.as_bytes());
                        payload[23..].copy_from_slice(&setting.write_values());
                        step(
                            WritePacket::new([0x21, 0x00, 0x00, 0x00, 0x18], &payload),
                            AckSpec::Report {
                                header: [0x21, 0x00, 0x00, 0x00, 0x01],
                                selector: Some(0x00),
                                required: true,
                            },
                            Duration::ZERO,
                        )
                    })
                    .collect()
            }
            WriteRequest::DeadBand(data) => chunked(
                [0x26, 0x01],
                data.as_bytes(),
                58,
                ack([0x26, 0x01, 0x00, 0x00, 0x00]),
                Duration::ZERO,
            ),
        }
    }

    /// The table this request writes.
    pub fn table(&self) -> TableId {
        match self {
            WriteRequest::SelectProfile(_) => TableId::Profile,
            WriteRequest::MainLight(_) => TableId::MainLight,
            WriteRequest::CustomColors1(_) => TableId::CustomColors {
                table: CustomTable::Custom1,
            },
            WriteRequest::Keymap(layer, _) => TableId::Keymap { layer: *layer },
            WriteRequest::Macro(slot, _) => TableId::Macro { slot: *slot },
            WriteRequest::AdvancedKeyClear(slot) => TableId::AdvancedKey { slot: *slot },
            WriteRequest::Trigger(_) => TableId::Triggers,
            WriteRequest::DeadBand(_) => TableId::DeadBand,
        }
    }

    /// Short, stable name of the request kind (the serde tag), e.g.
    /// `"select_profile"`.
    pub fn kind(&self) -> &'static str {
        match self {
            WriteRequest::SelectProfile(_) => "select_profile",
            WriteRequest::MainLight(_) => "main_light",
            WriteRequest::CustomColors1(_) => "custom_colors1",
            WriteRequest::Keymap(..) => "keymap",
            WriteRequest::Macro(..) => "macro",
            WriteRequest::AdvancedKeyClear(_) => "advanced_key_clear",
            WriteRequest::Trigger(_) => "trigger",
            WriteRequest::DeadBand(_) => "dead_band",
        }
    }

    /// The reads that show whether the write took effect (and, before a
    /// write, the current state of exactly the bytes it writes). For a
    /// trigger write: one `21/05` read per key, in index order.
    pub fn verify_reads(&self) -> Vec<ReadRequest> {
        match self {
            WriteRequest::SelectProfile(_) => vec![ReadRequest::ActiveProfile],
            WriteRequest::MainLight(_) => vec![ReadRequest::MainLight],
            WriteRequest::CustomColors1(_) => {
                vec![ReadRequest::CustomColors(CustomTable::Custom1)]
            }
            WriteRequest::Keymap(layer, _) => vec![ReadRequest::Keymap(*layer)],
            WriteRequest::Macro(slot, _) => vec![ReadRequest::Macro(*slot)],
            WriteRequest::AdvancedKeyClear(_) => vec![ReadRequest::AdvancedKeys],
            WriteRequest::Trigger(write) => write
                .positions()
                .into_iter()
                .map(ReadRequest::KeyTrigger)
                .collect(),
            WriteRequest::DeadBand(_) => vec![ReadRequest::DeadBand],
        }
    }

    /// Compares the replies to [`verify_reads`](Self::verify_reads) with what
    /// this request writes, on the raw declared bytes:
    ///
    /// * select profile: payload byte 1 of `21/11` equals the profile;
    /// * main light: the declared payload equals the 12-byte record;
    /// * colours, keymap, dead band: the table bytes are equal;
    /// * macro: the table bytes are equal, where an empty slot matches both
    ///   all `00` and all `FF`;
    /// * advanced-key clear: the slot's 40 bytes in `22 80` are all zero;
    /// * trigger: payload bytes 1..10 of each key's `21/05` reply equal
    ///   `MM A A P R Ah Ah Ph Rh`.
    ///
    /// `replies` may hold other replies too; a missing one is a mismatch. If
    /// it holds several replies to the same read, the last one counts (a
    /// re-read appended after an earlier one).
    pub fn check(&self, replies: &[RawResponse]) -> Result<(), Mismatch> {
        for read in self.verify_reads() {
            let expected = self.expected_bytes(read);
            let mismatch = |kind, got: Vec<u8>| Mismatch {
                table: self.table(),
                read,
                kind,
                expected: HexBytes(expected.clone()),
                got: HexBytes(got),
            };
            let Some(reply) = replies.iter().rev().find(|r| r.request() == read) else {
                return Err(mismatch(MismatchKind::MissingReply, Vec::new()));
            };
            let Some(got) = self.observed_bytes(reply) else {
                return Err(mismatch(MismatchKind::InvalidReply, reply.payload()));
            };
            let same = match self {
                WriteRequest::Macro(_, data) => data.is_equivalent(&got),
                _ => got == expected,
            };
            if !same {
                return Err(mismatch(MismatchKind::Differs, got));
            }
        }
        Ok(())
    }

    /// The bytes [`check`](Self::check) expects in the reply to `read`.
    fn expected_bytes(&self, read: ReadRequest) -> Vec<u8> {
        match self {
            WriteRequest::SelectProfile(profile) => vec![profile.get()],
            WriteRequest::MainLight(write) => write.record().as_bytes().to_vec(),
            WriteRequest::CustomColors1(data) => data.as_bytes().to_vec(),
            WriteRequest::Keymap(_, data) => data.as_bytes().to_vec(),
            WriteRequest::Macro(_, data) => data.to_bytes().to_vec(),
            WriteRequest::AdvancedKeyClear(_) => vec![0u8; ADVANCED_READ_SLOT_LEN],
            WriteRequest::Trigger(write) => match read {
                ReadRequest::KeyTrigger(_) => write.setting().read_bytes().to_vec(),
                _ => Vec::new(),
            },
            WriteRequest::DeadBand(data) => data.as_bytes().to_vec(),
        }
    }

    /// The bytes of `reply` that [`check`](Self::check) compares, or `None`
    /// if the reply is too short or its chunks are malformed.
    fn observed_bytes(&self, reply: &RawResponse) -> Option<Vec<u8>> {
        match self {
            WriteRequest::SelectProfile(_) => reply.payload().get(1..2).map(<[u8]>::to_vec),
            WriteRequest::MainLight(_) => Some(reply.payload()),
            WriteRequest::CustomColors1(_)
            | WriteRequest::Keymap(..)
            | WriteRequest::Macro(..)
            | WriteRequest::DeadBand(_) => reply.table_bytes("read-back").ok(),
            WriteRequest::AdvancedKeyClear(slot) => {
                let table = reply.table_bytes("advanced keys").ok()?;
                let start = usize::from(slot.get()) * ADVANCED_READ_SLOT_LEN;
                table
                    .get(start..start + ADVANCED_READ_SLOT_LEN)
                    .map(<[u8]>::to_vec)
            }
            WriteRequest::Trigger(_) => reply.payload().get(1..10).map(<[u8]>::to_vec),
        }
    }
}

impl fmt::Display for WriteRequest {
    /// E.g. `select profile 2 (21/12)`, `macro slot 3 (19 03)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteRequest::SelectProfile(profile) => write!(f, "select profile {profile} (21/12)"),
            WriteRequest::MainLight(write) if write.changes_mode() => {
                f.write_str("main light with mode change (17 00, 07 00)")
            }
            WriteRequest::MainLight(_) => f.write_str("main light (07 00)"),
            WriteRequest::CustomColors1(_) => f.write_str("custom colours 1 (09 00)"),
            WriteRequest::Keymap(Layer::Base, _) => f.write_str("base keymap (18 00)"),
            WriteRequest::Keymap(Layer::Fn, _) => f.write_str("Fn keymap (18 02)"),
            WriteRequest::Macro(slot, _) => {
                write!(f, "macro slot {0} (19 {0:02X})", slot.get())
            }
            WriteRequest::AdvancedKeyClear(slot) => {
                write!(f, "clear advanced-key slot {0} (22 {0:02X})", slot.get())
            }
            WriteRequest::Trigger(write) => {
                let keys: Vec<String> = write.positions().iter().map(|p| p.to_string()).collect();
                write!(f, "triggers of {} (21)", keys.join(", "))
            }
            WriteRequest::DeadBand(_) => f.write_str("dead band (26 01)"),
        }
    }
}

/// Write sub-command of a keymap layer: `00` base, `02` Fn (never `01`).
const fn keymap_write_sub(layer: Layer) -> u8 {
    match layer {
        Layer::Base => 0x00,
        Layer::Fn => 0x02,
    }
}

/// One step per `chunk_len` bytes of `table`: header `cmd sub <index BE16>
/// <len>` + the chunk.
fn chunked(
    [cmd, sub]: [u8; 2],
    table: &[u8],
    chunk_len: usize,
    ack: AckSpec,
    gap_after: Duration,
) -> Vec<WriteStep> {
    (0u16..)
        .zip(table.chunks(chunk_len))
        .map(|(index, chunk)| {
            let [hi, lo] = index.to_be_bytes();
            let len = u8::try_from(chunk.len()).unwrap_or(0);
            WriteStep {
                packet: WritePacket::new([cmd, sub, hi, lo, len], chunk),
                ack,
                gap_after,
            }
        })
        .collect()
}

/// Why a read-back did not confirm a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MismatchKind {
    /// No reply to one of the verify reads was supplied.
    MissingReply,
    /// The reply is too short or its chunks have the wrong lengths.
    InvalidReply,
    /// The keyboard holds different bytes than were written.
    Differs,
}

impl fmt::Display for MismatchKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MismatchKind::MissingReply => "no read-back reply",
            MismatchKind::InvalidReply => "the read-back reply cannot be compared",
            MismatchKind::Differs => "the read-back differs from what was written",
        })
    }
}

/// A read-back that does not match the write ([`WriteRequest::check`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[error("{table}: {kind} ({read})")]
pub struct Mismatch {
    /// The table that was written.
    pub table: TableId,
    /// The read whose reply did not match.
    pub read: ReadRequest,
    /// What went wrong.
    pub kind: MismatchKind,
    /// The compared bytes as written (for a profile: the profile number; for
    /// a trigger: `MM A A P R Ah Ah Ph Rh`; for an advanced-key clear: 40
    /// zero bytes).
    pub expected: HexBytes,
    /// The compared bytes as read (the reply's payload for
    /// [`MismatchKind::InvalidReply`], empty for a missing reply).
    pub got: HexBytes,
}

impl Mismatch {
    /// Offsets at which `expected` and `got` differ (including offsets
    /// present in only one of them).
    pub fn differing_offsets(&self) -> Vec<usize> {
        let len = self.expected.len().max(self.got.len());
        (0..len)
            .filter(|i| self.expected.get(*i) != self.got.get(*i))
            .collect()
    }
}
