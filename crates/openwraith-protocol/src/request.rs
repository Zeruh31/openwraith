//! Read requests (spec section 3) and how their responses are recognised
//! (sections 2.3, 2.5 and 2.8).
//!
//! [`ReadRequest`] is the complete list of commands Phase 1 may send. Every
//! variant is a read labelled CONFIRMED in the master command table; nothing
//! here can write, reset or calibrate.

use std::fmt;
use std::time::Duration;

use crate::decode::Status;
use crate::frame::{OutPacket, REPORT_LEN, Report};
use crate::layout::KeyPos;
use crate::model::Model;
use crate::response::ResponseCollector;

/// One of the two per-key custom colour tables (spec section 4.7.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CustomTable {
    /// Custom 1 (read sub `0x80`).
    Custom1,
    /// Custom 2 (read sub `0x81`).
    Custom2,
}

impl CustomTable {
    /// Both tables.
    pub const ALL: [CustomTable; 2] = [CustomTable::Custom1, CustomTable::Custom2];

    /// 0 for Custom 1, 1 for Custom 2 (the value used on the wire).
    pub const fn number(self) -> u8 {
        match self {
            CustomTable::Custom1 => 0,
            CustomTable::Custom2 => 1,
        }
    }

    /// The table for wire value 0 or 1.
    pub const fn from_number(n: u8) -> Option<Self> {
        match n {
            0 => Some(CustomTable::Custom1),
            1 => Some(CustomTable::Custom2),
            _ => None,
        }
    }
}

/// Keymap layer (spec section 4.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Layer {
    /// Base layer (read sub `0x80`).
    Base,
    /// Fn layer, active while Fn is held (read sub `0x82`).
    Fn,
}

impl Layer {
    /// Both layers.
    pub const ALL: [Layer; 2] = [Layer::Base, Layer::Fn];
}

/// A validated macro slot number (0..=9, spec section 4.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
pub struct MacroSlotId(u8);

impl MacroSlotId {
    /// Number of macro slots.
    pub const COUNT: u8 = 10;

    /// Creates a slot id, or `None` if `slot` is not in 0..=9.
    pub const fn new(slot: u8) -> Option<Self> {
        if slot < Self::COUNT {
            Some(MacroSlotId(slot))
        } else {
            None
        }
    }

    /// The slot number (0..=9).
    pub const fn get(self) -> u8 {
        self.0
    }

    /// All ten slots in order.
    pub fn all() -> impl Iterator<Item = MacroSlotId> {
        (0..Self::COUNT).map(MacroSlotId)
    }
}

impl TryFrom<u8> for MacroSlotId {
    type Error = String;

    fn try_from(slot: u8) -> Result<Self, Self::Error> {
        MacroSlotId::new(slot).ok_or_else(|| format!("macro slot {slot} is not in 0..=9"))
    }
}

impl From<MacroSlotId> for u8 {
    fn from(slot: MacroSlotId) -> u8 {
        slot.0
    }
}

/// Every read request Phase 1 can send.
///
/// All variants are reads labelled CONFIRMED in the spec's master command
/// table (section 3; each variant names its row). There is intentionally no
/// way to express a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ReadRequest {
    /// `0D 00`: device info string, 2 chunks (row 12).
    DeviceInfo,
    /// `01 00`: status and capability flags (row 1). Also the site's heartbeat.
    Status,
    /// `0E 00`: Fn-layer locked keys, a 22-byte key mask (row 13).
    FnLockedKeys,
    /// `0A 00`: main-light mode list (row 11).
    MainLightModes,
    /// `02 00`: side-light mode list (row 2).
    SideLightModes,
    /// `07 01`: main light effect record (row 3). The reply carries `07 00`.
    MainLight,
    /// `08 01`: side light effect record (row 5). The reply carries `08 00`.
    SideLight,
    /// `09 20`: active custom colour table (row 7). The reply carries the slot in byte 1.
    ActiveCustomTable,
    /// `09 80` / `09 81`: custom colour table, 8 chunks (row 8).
    CustomColors(CustomTable),
    /// `18 80` / `18 82`: keymap layer, 10 chunks (row 22).
    Keymap(Layer),
    /// `19 80`..`19 89`: one macro slot, 5 chunks (row 24).
    Macro(MacroSlotId),
    /// `22 80`: all 40 advanced-key slots, one chunk per slot (row 42).
    AdvancedKeys,
    /// `24 01`: SOCD pairs, 2 chunks (row 44).
    SocdPairs,
    /// `24 02`: SOCD power and hotkey (row 45).
    SocdSettings,
    /// `25 00`: supported switch types (row 48).
    SwitchTypeList,
    /// `25 02`: per-key switch type table, 3 chunks (row 49).
    SwitchTypes,
    /// `26 00`: dead band table, 5 chunks (row 51).
    DeadBand,
    /// `21 .. 18 04`: global trigger parameters (row 32).
    TriggerGlobal,
    /// `21 .. 18 05 row col`: one key's trigger settings (row 33).
    KeyTrigger(KeyPos),
    /// `21 .. 01 09`: polling rate (row 38). Byte 6 must stay 0 (`01` would set it).
    PollRate,
    /// `21 .. 00 11`: active onboard profile (row 40).
    ActiveProfile,
}

/// How byte 1 of a reply is matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubMatch {
    /// Any value (byte 1 carries data, e.g. the status link token).
    Any,
    /// Exactly this value.
    Exact(u8),
    /// Any value strictly below this one (`09 20` reply: byte 1 = active slot).
    Below(u8),
}

impl SubMatch {
    /// Whether `sub` satisfies the rule.
    pub const fn accepts(self, sub: u8) -> bool {
        match self {
            SubMatch::Any => true,
            SubMatch::Exact(v) => sub == v,
            SubMatch::Below(v) => sub < v,
        }
    }
}

/// How to recognise the replies to a request and when the reply is complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResponseSpec {
    /// Byte 0 of every reply.
    pub cmd: u8,
    /// Rule for byte 1.
    pub sub: SubMatch,
    /// Required byte 5 (function selector of the `0x21` family).
    pub selector: Option<u8>,
    /// Required row/col echo in bytes 15/16 (`21/05` replies).
    pub echo: Option<KeyPos>,
    /// Number of chunks; the reply is complete when chunks `0..chunks` (byte 3) arrived.
    pub chunks: u16,
}

impl ResponseSpec {
    /// Whether `report` is (a chunk of) a reply described by this spec.
    pub fn matches(&self, report: &Report) -> bool {
        report.cmd() == self.cmd
            && self.sub.accepts(report.sub())
            && report.chunk_index() < self.chunks
            && self.selector.is_none_or(|s| report.byte(5) == s)
            && self
                .echo
                .is_none_or(|pos| report.byte(15) == pos.row() && report.byte(16) == pos.col())
    }
}

/// Chunk layout of a multi-chunk table reply (spec section 2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableLayout {
    /// Number of chunks.
    pub chunks: u16,
    /// Payload bytes in every full chunk.
    pub chunk_len: usize,
    /// Total payload bytes; the last chunk carries the remainder.
    pub total_len: usize,
}

impl TableLayout {
    /// Declared length chunk `index` should carry.
    pub const fn expected_len(&self, index: u16) -> usize {
        let start = index as usize * self.chunk_len;
        if start >= self.total_len {
            0
        } else if self.total_len - start < self.chunk_len {
            self.total_len - start
        } else {
            self.chunk_len
        }
    }
}

impl ReadRequest {
    /// The 63-byte output report for this request, byte-identical to what the
    /// vendor site sends (including its byte-4 values, spec section 2.4).
    pub fn packet(&self) -> OutPacket {
        match *self {
            ReadRequest::DeviceInfo => OutPacket::from_prefix(&[0x0D]),
            ReadRequest::Status => OutPacket::from_prefix(&[0x01]),
            ReadRequest::FnLockedKeys => OutPacket::from_prefix(&[0x0E]),
            ReadRequest::MainLightModes => OutPacket::from_prefix(&[0x0A]),
            ReadRequest::SideLightModes => OutPacket::from_prefix(&[0x02]),
            ReadRequest::MainLight => OutPacket::from_prefix(&[0x07, 0x01]),
            ReadRequest::SideLight => OutPacket::from_prefix(&[0x08, 0x01]),
            ReadRequest::ActiveCustomTable => OutPacket::from_prefix(&[0x09, 0x20]),
            ReadRequest::CustomColors(table) => {
                OutPacket::from_prefix(&[0x09, 0x80 | table.number()])
            }
            ReadRequest::Keymap(Layer::Base) => OutPacket::from_prefix(&[0x18, 0x80]),
            ReadRequest::Keymap(Layer::Fn) => OutPacket::from_prefix(&[0x18, 0x82]),
            ReadRequest::Macro(slot) => OutPacket::from_prefix(&[0x19, 0x80 | slot.get()]),
            ReadRequest::AdvancedKeys => OutPacket::from_prefix(&[0x22, 0x80]),
            ReadRequest::SocdPairs => OutPacket::from_prefix(&[0x24, 0x01]),
            ReadRequest::SocdSettings => OutPacket::from_prefix(&[0x24, 0x02]),
            ReadRequest::SwitchTypeList => OutPacket::from_prefix(&[0x25, 0x00]),
            ReadRequest::SwitchTypes => OutPacket::from_prefix(&[0x25, 0x02]),
            ReadRequest::DeadBand => OutPacket::from_prefix(&[0x26, 0x00]),
            ReadRequest::TriggerGlobal => OutPacket::from_prefix(&[0x21, 0, 0, 0, 0x18, 0x04]),
            ReadRequest::KeyTrigger(pos) => {
                OutPacket::from_prefix(&[0x21, 0, 0, 0, 0x18, 0x05, pos.row(), pos.col()])
            }
            ReadRequest::PollRate => OutPacket::from_prefix(&[0x21, 0, 0, 0, 0x01, 0x09]),
            ReadRequest::ActiveProfile => OutPacket::from_prefix(&[0x21, 0, 0, 0, 0x00, 0x11]),
        }
    }

    /// Recognises a captured or logged output report as one of our read
    /// requests. Returns `None` for writes and for anything that is not
    /// byte-identical to [`packet`](Self::packet).
    pub fn from_packet(bytes: &[u8]) -> Option<ReadRequest> {
        if bytes.len() != REPORT_LEN {
            return None;
        }
        let byte = |i: usize| bytes.get(i).copied().unwrap_or(0);
        let candidate = match (byte(0), byte(1)) {
            (0x0D, 0x00) => ReadRequest::DeviceInfo,
            (0x01, 0x00) => ReadRequest::Status,
            (0x0E, 0x00) => ReadRequest::FnLockedKeys,
            (0x0A, 0x00) => ReadRequest::MainLightModes,
            (0x02, 0x00) => ReadRequest::SideLightModes,
            (0x07, 0x01) => ReadRequest::MainLight,
            (0x08, 0x01) => ReadRequest::SideLight,
            (0x09, 0x20) => ReadRequest::ActiveCustomTable,
            (0x09, 0x80) => ReadRequest::CustomColors(CustomTable::Custom1),
            (0x09, 0x81) => ReadRequest::CustomColors(CustomTable::Custom2),
            (0x18, 0x80) => ReadRequest::Keymap(Layer::Base),
            (0x18, 0x82) => ReadRequest::Keymap(Layer::Fn),
            (0x19, sub @ 0x80..=0x89) => ReadRequest::Macro(MacroSlotId::new(sub - 0x80)?),
            (0x22, 0x80) => ReadRequest::AdvancedKeys,
            (0x24, 0x01) => ReadRequest::SocdPairs,
            (0x24, 0x02) => ReadRequest::SocdSettings,
            (0x25, 0x00) => ReadRequest::SwitchTypeList,
            (0x25, 0x02) => ReadRequest::SwitchTypes,
            (0x26, 0x00) => ReadRequest::DeadBand,
            (0x21, 0x00) => match byte(5) {
                0x04 => ReadRequest::TriggerGlobal,
                0x05 => ReadRequest::KeyTrigger(KeyPos::new(byte(6), byte(7))?),
                0x09 => ReadRequest::PollRate,
                0x11 => ReadRequest::ActiveProfile,
                _ => return None,
            },
            _ => return None,
        };
        (candidate.packet().as_bytes()[..] == *bytes).then_some(candidate)
    }

    /// How the replies to this request are recognised.
    pub fn response_spec(&self) -> ResponseSpec {
        let simple = |cmd: u8, sub: SubMatch, chunks: u16| ResponseSpec {
            cmd,
            sub,
            selector: None,
            echo: None,
            chunks,
        };
        let family21 = |selector: u8, echo: Option<KeyPos>| ResponseSpec {
            cmd: 0x21,
            sub: SubMatch::Exact(0x00),
            selector: Some(selector),
            echo,
            chunks: 1,
        };
        match *self {
            ReadRequest::DeviceInfo => simple(0x0D, SubMatch::Exact(0x00), 2),
            // Byte 1 of the status reply is the link-mode token (section 4.2).
            ReadRequest::Status => simple(0x01, SubMatch::Any, 1),
            ReadRequest::FnLockedKeys => simple(0x0E, SubMatch::Exact(0x00), 1),
            ReadRequest::MainLightModes => simple(0x0A, SubMatch::Exact(0x00), 1),
            ReadRequest::SideLightModes => simple(0x02, SubMatch::Exact(0x00), 1),
            // `07 01` is answered with `07 00`; the spec says to match on byte 0.
            ReadRequest::MainLight => simple(0x07, SubMatch::Any, 1),
            ReadRequest::SideLight => simple(0x08, SubMatch::Any, 1),
            // `09 20` is answered with `09 SS 00 00 00`, SS = active slot.
            ReadRequest::ActiveCustomTable => simple(0x09, SubMatch::Below(0x80), 1),
            ReadRequest::CustomColors(table) => {
                simple(0x09, SubMatch::Exact(0x80 | table.number()), 8)
            }
            ReadRequest::Keymap(Layer::Base) => simple(0x18, SubMatch::Exact(0x80), 10),
            ReadRequest::Keymap(Layer::Fn) => simple(0x18, SubMatch::Exact(0x82), 10),
            ReadRequest::Macro(slot) => simple(0x19, SubMatch::Exact(0x80 | slot.get()), 5),
            ReadRequest::AdvancedKeys => simple(0x22, SubMatch::Exact(0x80), 40),
            ReadRequest::SocdPairs => simple(0x24, SubMatch::Exact(0x01), 2),
            ReadRequest::SocdSettings => simple(0x24, SubMatch::Exact(0x02), 1),
            ReadRequest::SwitchTypeList => simple(0x25, SubMatch::Exact(0x00), 1),
            ReadRequest::SwitchTypes => simple(0x25, SubMatch::Exact(0x02), 3),
            ReadRequest::DeadBand => simple(0x26, SubMatch::Exact(0x00), 5),
            ReadRequest::TriggerGlobal => family21(0x04, None),
            ReadRequest::KeyTrigger(pos) => family21(0x05, Some(pos)),
            ReadRequest::PollRate => family21(0x09, None),
            ReadRequest::ActiveProfile => family21(0x11, None),
        }
    }

    /// Chunk layout of the reply for multi-chunk tables; `None` for
    /// single-report replies.
    pub fn table_layout(&self) -> Option<TableLayout> {
        let layout = |chunks: u16, chunk_len: usize, total_len: usize| {
            Some(TableLayout {
                chunks,
                chunk_len,
                total_len,
            })
        };
        match self {
            ReadRequest::DeviceInfo => layout(2, 50, 100),
            ReadRequest::CustomColors(_) => layout(8, 54, 396),
            ReadRequest::Keymap(_) => layout(10, 56, 528),
            ReadRequest::Macro(_) => layout(5, 58, 256),
            ReadRequest::AdvancedKeys => layout(40, 40, 1600),
            ReadRequest::SocdPairs => layout(2, 40, 80),
            ReadRequest::SwitchTypes => layout(3, 58, 132),
            ReadRequest::DeadBand => layout(5, 58, 264),
            _ => None,
        }
    }

    /// Whether `report` is (a chunk of) the reply to this request.
    pub fn matches(&self, report: &Report) -> bool {
        self.response_spec().matches(report)
    }

    /// A collector that accumulates the reply reports of this request.
    pub fn collector(self) -> ResponseCollector {
        ResponseCollector::new(self)
    }

    /// Suggested total time to wait for a complete reply before retrying once
    /// (spec section 2.8: about 100 ms for single replies, about 150 ms for
    /// `21/11`, the site's deadlines for tables). A device that is busy after
    /// a keyboard reset answered about 0.8 s late (c07), so callers retry once.
    pub fn timeout(&self) -> Duration {
        let ms = match self {
            ReadRequest::DeviceInfo => 500,
            ReadRequest::ActiveProfile => 150,
            ReadRequest::Keymap(_) => 250,
            ReadRequest::Macro(_) => 150,
            ReadRequest::AdvancedKeys => 500,
            ReadRequest::CustomColors(_)
            | ReadRequest::SocdPairs
            | ReadRequest::SwitchTypes
            | ReadRequest::DeadBand => 200,
            _ => 100,
        };
        Duration::from_millis(ms)
    }

    /// Short, stable name of the request kind, e.g. `"key_trigger"`.
    pub fn kind(&self) -> &'static str {
        match self {
            ReadRequest::DeviceInfo => "device_info",
            ReadRequest::Status => "status",
            ReadRequest::FnLockedKeys => "fn_locked_keys",
            ReadRequest::MainLightModes => "main_light_modes",
            ReadRequest::SideLightModes => "side_light_modes",
            ReadRequest::MainLight => "main_light",
            ReadRequest::SideLight => "side_light",
            ReadRequest::ActiveCustomTable => "active_custom_table",
            ReadRequest::CustomColors(_) => "custom_colors",
            ReadRequest::Keymap(_) => "keymap",
            ReadRequest::Macro(_) => "macro",
            ReadRequest::AdvancedKeys => "advanced_keys",
            ReadRequest::SocdPairs => "socd_pairs",
            ReadRequest::SocdSettings => "socd_settings",
            ReadRequest::SwitchTypeList => "switch_type_list",
            ReadRequest::SwitchTypes => "switch_types",
            ReadRequest::DeadBand => "dead_band",
            ReadRequest::TriggerGlobal => "trigger_global",
            ReadRequest::KeyTrigger(_) => "key_trigger",
            ReadRequest::PollRate => "poll_rate",
            ReadRequest::ActiveProfile => "active_profile",
        }
    }

    /// Every constructible request: 17 parameterless reads, 2 colour tables,
    /// 2 keymap layers, 10 macro slots and all 132 matrix positions.
    pub fn all() -> Vec<ReadRequest> {
        let mut out = vec![
            ReadRequest::DeviceInfo,
            ReadRequest::Status,
            ReadRequest::FnLockedKeys,
            ReadRequest::MainLightModes,
            ReadRequest::SideLightModes,
            ReadRequest::MainLight,
            ReadRequest::SideLight,
            ReadRequest::ActiveCustomTable,
            ReadRequest::AdvancedKeys,
            ReadRequest::SocdPairs,
            ReadRequest::SocdSettings,
            ReadRequest::SwitchTypeList,
            ReadRequest::SwitchTypes,
            ReadRequest::DeadBand,
            ReadRequest::TriggerGlobal,
            ReadRequest::PollRate,
            ReadRequest::ActiveProfile,
        ];
        out.extend(CustomTable::ALL.map(ReadRequest::CustomColors));
        out.extend(Layer::ALL.map(ReadRequest::Keymap));
        out.extend(MacroSlotId::all().map(ReadRequest::Macro));
        out.extend(KeyPos::all().map(ReadRequest::KeyTrigger));
        out
    }

    /// The requests that read everything the keyboard reports for the
    /// **active** onboard profile (spec section 6.10 rule 1), including the
    /// tables whose profile scope is unverified (custom colours, polling
    /// rate, switch types; open question Q3), in a safe order.
    ///
    /// When `status` is given, optional features are read only if the device
    /// advertises them (as the vendor site does): custom colours (flags A bit
    /// 1), switch types (bit 2), polling rate (bit 3), dead band (bit 4) and
    /// SOCD (status byte 14). Per-key triggers are read for every key of
    /// `model`.
    pub fn full_read_plan(model: &Model, status: Option<&Status>) -> Vec<ReadRequest> {
        let has = |f: fn(&Status) -> bool| status.is_none_or(f);
        let mut plan = vec![
            ReadRequest::DeviceInfo,
            ReadRequest::Status,
            ReadRequest::FnLockedKeys,
            ReadRequest::ActiveProfile,
            ReadRequest::MainLightModes,
            ReadRequest::SideLightModes,
            ReadRequest::MainLight,
            ReadRequest::SideLight,
        ];
        if has(|s| s.flags_a.custom_light) {
            plan.push(ReadRequest::ActiveCustomTable);
            plan.extend(CustomTable::ALL.map(ReadRequest::CustomColors));
        }
        plan.extend(Layer::ALL.map(ReadRequest::Keymap));
        plan.extend(MacroSlotId::all().map(ReadRequest::Macro));
        plan.push(ReadRequest::AdvancedKeys);
        if has(|s| s.socd_supported) {
            plan.push(ReadRequest::SocdSettings);
            plan.push(ReadRequest::SocdPairs);
        }
        plan.push(ReadRequest::TriggerGlobal);
        plan.extend(model.positions().map(ReadRequest::KeyTrigger));
        if has(|s| s.flags_a.poll_rate) {
            plan.push(ReadRequest::PollRate);
        }
        if has(|s| s.flags_a.more_switch) {
            plan.push(ReadRequest::SwitchTypeList);
            plan.push(ReadRequest::SwitchTypes);
        }
        if has(|s| s.flags_a.dead_band) {
            plan.push(ReadRequest::DeadBand);
        }
        plan
    }
}

impl fmt::Display for ReadRequest {
    /// E.g. `key trigger r2c2 (21/05)`, `macro 3 (19 83)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = self.packet();
        let b = p.as_bytes();
        match self {
            ReadRequest::DeviceInfo => write!(f, "device info (0D)"),
            ReadRequest::Status => write!(f, "status (01)"),
            ReadRequest::FnLockedKeys => write!(f, "Fn-locked keys (0E)"),
            ReadRequest::MainLightModes => write!(f, "main light modes (0A)"),
            ReadRequest::SideLightModes => write!(f, "side light modes (02)"),
            ReadRequest::MainLight => write!(f, "main light (07 01)"),
            ReadRequest::SideLight => write!(f, "side light (08 01)"),
            ReadRequest::ActiveCustomTable => write!(f, "active custom colour table (09 20)"),
            ReadRequest::CustomColors(t) => {
                write!(f, "custom colours {} (09 {:02X})", t.number() + 1, b[1])
            }
            ReadRequest::Keymap(Layer::Base) => write!(f, "base keymap (18 80)"),
            ReadRequest::Keymap(Layer::Fn) => write!(f, "Fn keymap (18 82)"),
            ReadRequest::Macro(slot) => write!(f, "macro {} (19 {:02X})", slot.get(), b[1]),
            ReadRequest::AdvancedKeys => write!(f, "advanced keys (22 80)"),
            ReadRequest::SocdPairs => write!(f, "SOCD pairs (24 01)"),
            ReadRequest::SocdSettings => write!(f, "SOCD settings (24 02)"),
            ReadRequest::SwitchTypeList => write!(f, "switch type list (25 00)"),
            ReadRequest::SwitchTypes => write!(f, "switch types (25 02)"),
            ReadRequest::DeadBand => write!(f, "dead band (26 00)"),
            ReadRequest::TriggerGlobal => write!(f, "trigger parameters (21/04)"),
            ReadRequest::KeyTrigger(pos) => write!(f, "key trigger {pos} (21/05)"),
            ReadRequest::PollRate => write!(f, "polling rate (21/09)"),
            ReadRequest::ActiveProfile => write!(f, "active profile (21/11)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::W75;
    use std::collections::HashSet;

    /// The read forms documented in the spec. Any packet outside this list
    /// would be a write, a reset or an unknown command.
    fn is_documented_read(b: &[u8; REPORT_LEN]) -> bool {
        let rest_zero = |from: usize| b[from..].iter().all(|x| *x == 0);
        match (b[0], b[1]) {
            (0x0D | 0x01 | 0x0E | 0x0A | 0x02, 0x00) => rest_zero(2),
            (0x07 | 0x08, 0x01) => rest_zero(2),
            (0x09, 0x20 | 0x80 | 0x81) => rest_zero(2),
            (0x18, 0x80 | 0x82) => rest_zero(2),
            (0x19, 0x80..=0x89) => rest_zero(2),
            (0x22, 0x80) => rest_zero(2),
            (0x24, 0x01 | 0x02) => rest_zero(2),
            (0x25, 0x00 | 0x02) => rest_zero(2),
            (0x26, 0x00) => rest_zero(2),
            (0x21, 0x00) => {
                b[2] == 0
                    && b[3] == 0
                    && match (b[4], b[5]) {
                        (0x18, 0x04) => rest_zero(6),
                        (0x18, 0x05) => b[6] < 6 && b[7] < 22 && rest_zero(8),
                        // Byte 6 = 1 would turn the poll-rate read into a write.
                        (0x01, 0x09) => rest_zero(6),
                        (0x00, 0x11) => rest_zero(6),
                        _ => false,
                    }
            }
            _ => false,
        }
    }

    #[test]
    fn every_constructible_request_is_a_documented_read() {
        let all = ReadRequest::all();
        assert_eq!(all.len(), 17 + 2 + 2 + 10 + 132);
        for request in &all {
            let packet = request.packet();
            let b = packet.as_bytes();
            assert!(is_documented_read(b), "{request} built {packet:?}");
            // Never a reset (0x14), music rhythm (0x17) or table write.
            assert_ne!(b[0], 0x14);
            assert_ne!(b[0], 0x17);
            if matches!(b[0], 0x18 | 0x19 | 0x22) {
                assert!(b[1] >= 0x80, "{request} uses a write sub-command");
            }
            if b[0] == 0x21 {
                assert!(
                    matches!(b[5], 0x04 | 0x05 | 0x09 | 0x11),
                    "{request} uses a 0x21 write selector"
                );
                if b[5] != 0x05 {
                    assert_eq!(b[6], 0, "{request}: byte 6 = 1 would make it a write");
                }
            }
        }
    }

    #[test]
    fn all_is_exhaustive_and_unique() {
        let all = ReadRequest::all();
        let unique: HashSet<_> = all.iter().collect();
        assert_eq!(unique.len(), all.len());
        // Compile-time reminder: adding a variant must extend `all()`.
        for request in &all {
            match request {
                ReadRequest::DeviceInfo
                | ReadRequest::Status
                | ReadRequest::FnLockedKeys
                | ReadRequest::MainLightModes
                | ReadRequest::SideLightModes
                | ReadRequest::MainLight
                | ReadRequest::SideLight
                | ReadRequest::ActiveCustomTable
                | ReadRequest::CustomColors(_)
                | ReadRequest::Keymap(_)
                | ReadRequest::Macro(_)
                | ReadRequest::AdvancedKeys
                | ReadRequest::SocdPairs
                | ReadRequest::SocdSettings
                | ReadRequest::SwitchTypeList
                | ReadRequest::SwitchTypes
                | ReadRequest::DeadBand
                | ReadRequest::TriggerGlobal
                | ReadRequest::KeyTrigger(_)
                | ReadRequest::PollRate
                | ReadRequest::ActiveProfile => {}
            }
        }
    }

    #[test]
    fn packets_round_trip_through_from_packet() {
        for request in ReadRequest::all() {
            let packet = request.packet();
            assert_eq!(ReadRequest::from_packet(packet.as_bytes()), Some(request));
        }
    }

    #[test]
    fn from_packet_rejects_writes() {
        let mut b = [0u8; REPORT_LEN];
        let mut set = |prefix: &[u8]| {
            b = [0u8; REPORT_LEN];
            b[..prefix.len()].copy_from_slice(prefix);
            ReadRequest::from_packet(&b)
        };
        assert_eq!(set(&[0x14, 0, 0, 0, 1, 1]), None); // reset
        assert_eq!(set(&[0x07, 0x00, 0, 0, 0x0e, 0x0a]), None); // light write
        assert_eq!(set(&[0x09, 0x00, 0, 0, 0x36]), None); // colour write
        assert_eq!(set(&[0x21, 0, 0, 0, 0x18, 0x06]), None); // reset triggers
        assert_eq!(set(&[0x21, 0, 0, 0, 0x00, 0x09, 0x01, 0x08]), None); // set poll rate
        assert_eq!(set(&[0x21, 0, 0, 0, 0x00, 0x12, 0x02]), None); // select profile
        assert_eq!(set(&[0x24, 0x03, 0, 0, 1]), None); // SOCD power
        assert_eq!(set(&[0x0D, 0x00, 0, 0, 0, 0xFF]), None); // trailing garbage
        assert_eq!(set(&[0x0D]), Some(ReadRequest::DeviceInfo));
        assert_eq!(ReadRequest::from_packet(&[0x0D; 10]), None);
    }

    #[test]
    fn exact_site_bytes() {
        let prefix = |r: ReadRequest, n: usize| r.packet().as_bytes()[..n].to_vec();
        assert_eq!(
            prefix(ReadRequest::KeyTrigger(KeyPos::new(2, 2).unwrap()), 9),
            [0x21, 0, 0, 0, 0x18, 0x05, 2, 2, 0]
        );
        assert_eq!(
            prefix(ReadRequest::PollRate, 7),
            [0x21, 0, 0, 0, 0x01, 0x09, 0]
        );
        assert_eq!(
            prefix(ReadRequest::ActiveProfile, 7),
            [0x21, 0, 0, 0, 0x00, 0x11, 0]
        );
        assert_eq!(
            prefix(ReadRequest::Macro(MacroSlotId::new(9).unwrap()), 3),
            [0x19, 0x89, 0]
        );
        assert_eq!(
            prefix(ReadRequest::CustomColors(CustomTable::Custom2), 2),
            [0x09, 0x81]
        );
        assert_eq!(prefix(ReadRequest::Keymap(Layer::Fn), 2), [0x18, 0x82]);
    }

    #[test]
    fn table_layouts_match_spec_sizes() {
        for request in ReadRequest::all() {
            let spec = request.response_spec();
            match request.table_layout() {
                Some(layout) => {
                    assert_eq!(layout.chunks, spec.chunks, "{request}");
                    let sum: usize = (0..layout.chunks).map(|i| layout.expected_len(i)).sum();
                    assert_eq!(sum, layout.total_len, "{request}");
                    assert!(layout.chunk_len <= crate::MAX_CHUNK_PAYLOAD);
                }
                None => assert_eq!(spec.chunks, 1, "{request}"),
            }
        }
        let keymap = ReadRequest::Keymap(Layer::Base).table_layout().unwrap();
        assert_eq!(keymap.expected_len(8), 56);
        assert_eq!(keymap.expected_len(9), 24);
        let colors = ReadRequest::CustomColors(CustomTable::Custom1)
            .table_layout()
            .unwrap();
        assert_eq!(colors.expected_len(7), 18);
        let switches = ReadRequest::SwitchTypes.table_layout().unwrap();
        assert_eq!(switches.expected_len(2), 16);
    }

    #[test]
    fn response_matching() {
        let mut bytes = [0u8; REPORT_LEN];
        bytes[..17].copy_from_slice(&[
            0x21, 0, 0, 0, 0x0c, 0x05, 0x0c, 0x32, 0x32, 0x0f, 0x0f, 0, 0, 0, 0, 2, 2,
        ]);
        let reply = Report::new(bytes);
        let w = ReadRequest::KeyTrigger(KeyPos::new(2, 2).unwrap());
        let q = ReadRequest::KeyTrigger(KeyPos::new(2, 1).unwrap());
        assert!(w.matches(&reply));
        assert!(!q.matches(&reply), "row/col echo must match");
        assert!(!ReadRequest::TriggerGlobal.matches(&reply));

        // `07 01` is answered with `07 00`.
        let mut light = [0u8; REPORT_LEN];
        light[..5].copy_from_slice(&[0x07, 0x00, 0, 0, 0x0c]);
        assert!(ReadRequest::MainLight.matches(&Report::new(light)));
        assert!(!ReadRequest::SideLight.matches(&Report::new(light)));

        // `09 20` reply vs colour table chunk.
        let mut slot = [0u8; REPORT_LEN];
        slot[0] = 0x09;
        assert!(ReadRequest::ActiveCustomTable.matches(&Report::new(slot)));
        slot[1] = 0x80;
        assert!(!ReadRequest::ActiveCustomTable.matches(&Report::new(slot)));
        assert!(ReadRequest::CustomColors(CustomTable::Custom1).matches(&Report::new(slot)));

        // Chunk index outside the table.
        slot[3] = 8;
        assert!(!ReadRequest::CustomColors(CustomTable::Custom1).matches(&Report::new(slot)));
    }

    #[test]
    fn full_read_plan_covers_w75() {
        let plan = ReadRequest::full_read_plan(&W75, None);
        let unique: HashSet<_> = plan.iter().collect();
        assert_eq!(unique.len(), plan.len());
        let triggers = plan
            .iter()
            .filter(|r| matches!(r, ReadRequest::KeyTrigger(_)))
            .count();
        assert_eq!(triggers, 82);
        assert_eq!(plan.len(), ReadRequest::all().len() - 132 + 82);
        assert_eq!(plan[0], ReadRequest::DeviceInfo);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_names() {
        let json =
            serde_json::to_string(&ReadRequest::KeyTrigger(KeyPos::new(2, 2).unwrap())).unwrap();
        assert_eq!(json, r#"{"key_trigger":{"row":2,"col":2}}"#);
        let json =
            serde_json::to_string(&ReadRequest::Macro(MacroSlotId::new(3).unwrap())).unwrap();
        assert_eq!(json, r#"{"macro":3}"#);
        assert!(serde_json::from_str::<ReadRequest>(r#"{"macro":10}"#).is_err());
        let back: ReadRequest = serde_json::from_str(r#"{"keymap":"fn"}"#).unwrap();
        assert_eq!(back, ReadRequest::Keymap(Layer::Fn));
        assert_eq!(
            serde_json::to_string(&ReadRequest::DeviceInfo).unwrap(),
            r#""device_info""#
        );
    }

    #[test]
    fn display() {
        assert_eq!(
            ReadRequest::KeyTrigger(KeyPos::new(2, 2).unwrap()).to_string(),
            "key trigger r2c2 (21/05)"
        );
        assert_eq!(
            ReadRequest::Macro(MacroSlotId::new(3).unwrap()).to_string(),
            "macro 3 (19 83)"
        );
    }
}
