//! `DeviceSim`: a stateful simulator of the Wraith W75 for tests (feature
//! `mock`). No HID device is involved.
//!
//! The simulator holds four onboard profiles. Main light, per-key triggers,
//! the base keymap and the macro slots are per profile (OBSERVED on
//! hardware); every table whose scope is unknown can be configured as
//! per-profile or global with a [`ScopeConfig`]. Device constants (`0D`,
//! `0E`, `0A`, `02`, `21/04`, `25 00`) are replayed from the seed backup.
//!
//! Read requests are answered with exactly framed replies. Write packets are
//! parsed by an implementation of the nine write forms written
//! independently from the protocol crate's builders (design section 1);
//! applying them produces the acknowledgements and pushes the keyboard
//! sends (the profile push after a select, the status push after a
//! music-rhythm leave). **Any packet outside the allowed read and write
//! forms is recorded in [`DeviceSim::violations`] and gets no reply**; so is
//! every packet sent after the host read a reset notification, a
//! music-rhythm leave that is not immediately followed by a mode-changing
//! main-light write, a mode-changing main-light write without that leave,
//! and any `0x14`. Tests end with
//! [`DeviceSim::assert_clean`].
//!
//! Failures can be injected: a dropped or delayed acknowledgement, a write
//! without effect, a reset notification, a disconnect, a keyboard-side
//! profile change, foreign reports, a select that is acknowledged but does
//! not switch, a missing profile push, and tables that never take writes
//! (verification mismatch).
//!
//! # Virtual time
//!
//! The simulator has a virtual clock ([`Transport::now`]). A
//! [`read`](Transport::read) that finds nothing queued "waits": it advances
//! the clock to the next report scheduled within the timeout
//! ([`push_input_at`](DeviceSim::push_input_at),
//! [`delay_ack_by`](DeviceSim::delay_ack_by)) or by the whole timeout. So
//! observation periods, acknowledgement timeouts and expectation windows
//! behave as on a real keyboard, without any test sleeping.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use openwraith_protocol::{
    CustomTable, KeyPos, Layer, MacroSlotId, Model, OutPacket, ProfileId, REPORT_LEN, ReadRequest,
    Report, W75, WritePacket, is_keyboard_reset,
};

use crate::backup::Backup;
use crate::error::TransportError;
use crate::transport::{Transport, UsbInfo, WriteTransport};

/// Whether a simulated table belongs to each profile or is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Each onboard profile has its own copy.
    PerProfile,
    /// One copy shared by all profiles.
    Global,
}

/// The scope of every table whose real scope is unknown (design section 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeConfig {
    /// Fn keymap (`18 82`).
    pub fn_keymap: Scope,
    /// Advanced keys (`22 80`).
    pub advanced_keys: Scope,
    /// Dead band (`26 00`).
    pub dead_band: Scope,
    /// Custom colour table 1 (`09 80`).
    pub custom_colors1: Scope,
    /// Custom colour table 2 (`09 81`).
    pub custom_colors2: Scope,
    /// Active custom table (`09 20`).
    pub active_custom_table: Scope,
    /// Side light (`08 01`).
    pub side_light: Scope,
    /// SOCD pairs and settings (`24 01`, `24 02`).
    pub socd: Scope,
    /// Per-key switch types (`25 02`).
    pub switch_types: Scope,
    /// Polling rate (`21/09`).
    pub poll_rate: Scope,
    /// Status report (`01`, including the Win-Lock and 6-key bits).
    pub status: Scope,
}

impl ScopeConfig {
    /// Every unknown table with the same scope.
    pub fn uniform(scope: Scope) -> Self {
        ScopeConfig {
            fn_keymap: scope,
            advanced_keys: scope,
            dead_band: scope,
            custom_colors1: scope,
            custom_colors2: scope,
            active_custom_table: scope,
            side_light: scope,
            socd: scope,
            switch_types: scope,
            poll_rate: scope,
            status: scope,
        }
    }
}

impl Default for ScopeConfig {
    /// A guess: the tables the vendor site re-reads after a profile switch
    /// (Fn keymap, advanced keys, dead band, SOCD) per profile, the others
    /// global.
    fn default() -> Self {
        ScopeConfig {
            fn_keymap: Scope::PerProfile,
            advanced_keys: Scope::PerProfile,
            dead_band: Scope::PerProfile,
            socd: Scope::PerProfile,
            ..ScopeConfig::uniform(Scope::Global)
        }
    }
}

/// A writable table of the simulator (for test setup and checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimTable {
    /// Main light record (12 bytes, per profile).
    MainLight,
    /// Custom colour table 1 (396 bytes).
    CustomColors1,
    /// A keymap layer (528 bytes; base per profile).
    Keymap(Layer),
    /// A macro slot (256 bytes, per profile).
    Macro(MacroSlotId),
    /// All 40 advanced-key slots (1600 bytes).
    AdvancedKeys,
    /// Dead band (264 bytes).
    DeadBand,
}

impl SimTable {
    fn len(self) -> usize {
        match self {
            SimTable::MainLight => 12,
            SimTable::CustomColors1 => 396,
            SimTable::Keymap(_) => 528,
            SimTable::Macro(_) => 256,
            SimTable::AdvancedKeys => 1600,
            SimTable::DeadBand => 264,
        }
    }

    fn of_request(request: ReadRequest) -> Option<SimTable> {
        match request {
            ReadRequest::MainLight => Some(SimTable::MainLight),
            ReadRequest::CustomColors(CustomTable::Custom1) => Some(SimTable::CustomColors1),
            ReadRequest::Keymap(layer) => Some(SimTable::Keymap(layer)),
            ReadRequest::Macro(slot) => Some(SimTable::Macro(slot)),
            ReadRequest::AdvancedKeys => Some(SimTable::AdvancedKeys),
            ReadRequest::DeadBand => Some(SimTable::DeadBand),
            _ => None,
        }
    }

    fn request(self) -> ReadRequest {
        match self {
            SimTable::MainLight => ReadRequest::MainLight,
            SimTable::CustomColors1 => ReadRequest::CustomColors(CustomTable::Custom1),
            SimTable::Keymap(layer) => ReadRequest::Keymap(layer),
            SimTable::Macro(slot) => ReadRequest::Macro(slot),
            SimTable::AdvancedKeys => ReadRequest::AdvancedKeys,
            SimTable::DeadBand => ReadRequest::DeadBand,
        }
    }
}

/// One packet the host sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimPacket {
    /// A read request.
    Out(OutPacket),
    /// A write.
    Write(WritePacket),
}

impl SimPacket {
    /// The 63 bytes.
    pub fn as_bytes(&self) -> &[u8; REPORT_LEN] {
        match self {
            SimPacket::Out(p) => p.as_bytes(),
            SimPacket::Write(p) => p.as_bytes(),
        }
    }

    /// Whether this is a write.
    pub fn is_write(&self) -> bool {
        matches!(self, SimPacket::Write(_))
    }
}

/// A packet the simulator refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// 1-based number of the packet.
    pub packet: usize,
    /// Hex of the packet.
    pub bytes: String,
    /// What is wrong with it.
    pub reason: String,
}

/// The factory trigger record of every key (`00 55 55 01 01 00 00 00 00`:
/// 1.70 mm, Rapid Trigger off; c11).
pub const FACTORY_TRIGGER: [u8; 9] = [0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0];

/// The factory main light of profiles 2-4 in capture c08.
pub const FACTORY_LIGHT: [u8; 12] = [0x02, 0x04, 0x03, 0xff, 0, 0, 0, 0, 0, 0, 0x01, 0];

#[derive(Debug, Clone, Default)]
struct Faults {
    drop_ack: HashSet<usize>,
    delay_ack: HashSet<usize>,
    delay_ack_by: HashMap<usize, Duration>,
    drop_effect: HashSet<usize>,
    frozen: HashSet<SimTable>,
    reset_after: Option<usize>,
    disconnect_after: Option<usize>,
    switch_after: Vec<(usize, u8)>,
    inject_after: Vec<(usize, Report)>,
    ignore_selects: usize,
    omit_pushes: usize,
}

/// What a parsed write does.
enum Effect {
    Select(u8),
    Leave,
    Light([u8; 12]),
    Chunk {
        table: SimTable,
        offset: usize,
        data: Vec<u8>,
    },
    AdvancedClear(usize),
    Trigger {
        keys: Vec<KeyPos>,
        record: [u8; 9],
    },
}

/// A stateful W75 simulator implementing [`Transport`] and
/// [`WriteTransport`]. See the [module documentation](self).
#[derive(Debug, Clone)]
pub struct DeviceSim {
    model: &'static Model,
    scope: ScopeConfig,
    active: u8,
    replay: HashMap<(u8, ReadRequest), Vec<Report>>,
    tables: HashMap<(u8, SimTable), Vec<u8>>,
    triggers: HashMap<(u8, u8), [u8; 9]>,
    queue: VecDeque<Report>,
    delayed: Vec<Report>,
    /// Reports that arrive at a virtual time (offset from `epoch`).
    scheduled: Vec<(Duration, Report)>,
    epoch: Instant,
    clock: Duration,
    log: Vec<SimPacket>,
    violations: Vec<Violation>,
    faults: Faults,
    writes: usize,
    leave_pending: bool,
    reset_delivered: bool,
    disconnected: bool,
    usb: Option<UsbInfo>,
}

impl DeviceSim {
    /// A simulator seeded from a backup (schema 1 or 2), with the default
    /// [`ScopeConfig`]. Profiles missing from the backup are copies of its
    /// primary snapshot; the active profile is the backup's.
    pub fn from_backup(backup: &Backup) -> Self {
        Self::from_backup_with(backup, ScopeConfig::default())
    }

    /// [`from_backup`](Self::from_backup) with a scope configuration.
    pub fn from_backup_with(backup: &Backup, scope: ScopeConfig) -> Self {
        let active = backup
            .active_profile
            .filter(|p| ProfileId::new(*p).is_some())
            .unwrap_or(1);
        let mut sim = DeviceSim {
            model: backup.model().unwrap_or(&W75),
            scope,
            active,
            replay: HashMap::new(),
            tables: HashMap::new(),
            triggers: HashMap::new(),
            queue: VecDeque::new(),
            delayed: Vec::new(),
            scheduled: Vec::new(),
            epoch: Instant::now(),
            clock: Duration::ZERO,
            log: Vec::new(),
            violations: Vec::new(),
            faults: Faults::default(),
            writes: 0,
            leave_pending: false,
            reset_delivered: false,
            disconnected: false,
            usb: backup.device.usb.clone(),
        };
        // Global tables from the primary snapshot first, then each profile.
        sim.seed(0, backup.primary());
        for profile in ProfileId::ALL {
            let view = backup.snapshot(profile).unwrap_or_else(|| backup.primary());
            sim.seed(profile.get(), view);
        }
        sim
    }

    /// Stores the replies of `view` as the state of `profile` (0 = the
    /// global tables).
    fn seed(&mut self, profile: u8, view: crate::backup::SnapshotView<'_>) {
        for entry in view.entries() {
            let raw = &entry.raw;
            let request = raw.request();
            let global = self.request_scope(request) == Scope::Global;
            if global != (profile == 0) {
                continue;
            }
            if let Some(table) = SimTable::of_request(request) {
                let bytes = match table {
                    SimTable::MainLight => raw.payload(),
                    _ => raw.table_bytes("sim").unwrap_or_else(|_| raw.payload()),
                };
                self.tables.insert((profile, table), bytes);
                continue;
            }
            match request {
                ReadRequest::KeyTrigger(pos) => {
                    if let Some(record) = raw
                        .payload()
                        .get(1..10)
                        .and_then(|b| <[u8; 9]>::try_from(b).ok())
                    {
                        self.triggers.insert((profile, pos.index()), record);
                    }
                }
                ReadRequest::ActiveProfile => {}
                _ => {
                    self.replay
                        .insert((profile, request), raw.reports().to_vec());
                }
            }
        }
    }

    /// The scope of a read's data.
    fn request_scope(&self, request: ReadRequest) -> Scope {
        let s = &self.scope;
        match request {
            ReadRequest::MainLight
            | ReadRequest::Keymap(Layer::Base)
            | ReadRequest::Macro(_)
            | ReadRequest::KeyTrigger(_)
            | ReadRequest::ActiveProfile => Scope::PerProfile,
            ReadRequest::Keymap(Layer::Fn) => s.fn_keymap,
            ReadRequest::AdvancedKeys => s.advanced_keys,
            ReadRequest::DeadBand => s.dead_band,
            ReadRequest::CustomColors(CustomTable::Custom1) => s.custom_colors1,
            ReadRequest::CustomColors(CustomTable::Custom2) => s.custom_colors2,
            ReadRequest::ActiveCustomTable => s.active_custom_table,
            ReadRequest::SideLight => s.side_light,
            ReadRequest::SocdPairs | ReadRequest::SocdSettings => s.socd,
            ReadRequest::SwitchTypes => s.switch_types,
            ReadRequest::PollRate => s.poll_rate,
            ReadRequest::Status => s.status,
            ReadRequest::DeviceInfo
            | ReadRequest::FnLockedKeys
            | ReadRequest::MainLightModes
            | ReadRequest::SideLightModes
            | ReadRequest::TriggerGlobal
            | ReadRequest::SwitchTypeList => Scope::Global,
        }
    }

    /// Storage slot of a read's data for `profile` (0 = global).
    fn slot(&self, request: ReadRequest, profile: u8) -> u8 {
        match self.request_scope(request) {
            Scope::PerProfile => profile,
            Scope::Global => 0,
        }
    }

    // ----- state access for tests -----

    /// The scope configuration.
    pub fn scope(&self) -> &ScopeConfig {
        &self.scope
    }

    /// The active profile.
    pub fn active_profile(&self) -> u8 {
        self.active
    }

    /// Sets the active profile without any report (test setup).
    pub fn set_active_profile(&mut self, profile: ProfileId) {
        self.active = profile.get();
    }

    /// The bytes of `table` as `profile` sees them.
    pub fn table(&self, profile: ProfileId, table: SimTable) -> Option<&[u8]> {
        let slot = self.slot(table.request(), profile.get());
        self.tables.get(&(slot, table)).map(Vec::as_slice)
    }

    /// Replaces the bytes of `table` as `profile` sees them (for a global
    /// table: for every profile).
    pub fn set_table(&mut self, profile: ProfileId, table: SimTable, bytes: Vec<u8>) {
        let slot = self.slot(table.request(), profile.get());
        self.tables.insert((slot, table), bytes);
    }

    /// The trigger record (`MM A A P R Ah Ah Ph Rh`) of a key in `profile`.
    pub fn trigger(&self, profile: ProfileId, pos: KeyPos) -> Option<[u8; 9]> {
        self.triggers.get(&(profile.get(), pos.index())).copied()
    }

    /// Replaces the trigger record of a key in `profile`.
    pub fn set_trigger(&mut self, profile: ProfileId, pos: KeyPos, record: [u8; 9]) {
        self.triggers.insert((profile.get(), pos.index()), record);
    }

    /// Puts every profile (and every global table) into a factory-like
    /// state: factory triggers (except the knob's) and light, zero keymaps,
    /// empty (`FF`) macros, zero advanced keys, dead band and colour table 1.
    pub fn factory_reset_all(&mut self) {
        for profile in 0..=ProfileId::COUNT {
            for table in [
                SimTable::CustomColors1,
                SimTable::Keymap(Layer::Base),
                SimTable::Keymap(Layer::Fn),
                SimTable::AdvancedKeys,
                SimTable::DeadBand,
            ] {
                self.tables.insert((profile, table), vec![0; table.len()]);
            }
            for slot in MacroSlotId::all() {
                self.tables
                    .insert((profile, SimTable::Macro(slot)), vec![0xFF; 256]);
            }
            if profile > 0 {
                self.tables
                    .insert((profile, SimTable::MainLight), FACTORY_LIGHT.to_vec());
                // The knob has no writable trigger settings; it keeps its record.
                let knob = self.model.knob_index;
                for pos in self.model.positions().filter(|p| Some(p.index()) != knob) {
                    self.triggers
                        .insert((profile, pos.index()), FACTORY_TRIGGER);
                }
            }
        }
    }

    // ----- failure injection -----

    /// Queues `report` now, as if it arrived before the next request.
    pub fn push_input(&mut self, report: Report) {
        self.queue.push_back(report);
    }

    /// Delivers `report` `after` the current virtual time (a heartbeat of
    /// another program, a late push, ...).
    pub fn push_input_at(&mut self, after: Duration, report: Report) {
        let due = self.clock.saturating_add(after);
        self.scheduled.push((due, report));
    }

    /// Virtual time since the simulator was created.
    pub fn elapsed(&self) -> Duration {
        self.clock
    }

    /// Queues `report` right after the `packet`-th packet (1-based, reads
    /// and writes), before that packet's reply.
    pub fn inject_after_packets(&mut self, packet: usize, report: Report) {
        self.faults.inject_after.push((packet, report));
    }

    /// The `write`-th write (1-based, writes only) gets no acknowledgement.
    pub fn drop_ack(&mut self, write: usize) {
        self.faults.drop_ack.insert(write);
    }

    /// The acknowledgement of the `write`-th write arrives only after the
    /// next packet.
    pub fn delay_ack(&mut self, write: usize) {
        self.faults.delay_ack.insert(write);
    }

    /// The acknowledgement of the `write`-th write arrives `delay` (virtual
    /// time) after the write, independently of later packets.
    pub fn delay_ack_by(&mut self, write: usize, delay: Duration) {
        self.faults.delay_ack_by.insert(write, delay);
    }

    /// The `write`-th write is acknowledged but has no effect.
    pub fn drop_effect(&mut self, write: usize) {
        self.faults.drop_effect.insert(write);
    }

    /// Writes to `table` are acknowledged but never take effect.
    pub fn freeze(&mut self, table: SimTable) {
        self.faults.frozen.insert(table);
    }

    /// The keyboard sends its reset notification right after the
    /// `packet`-th packet.
    pub fn reset_after_packets(&mut self, packet: usize) {
        self.faults.reset_after = Some(packet);
    }

    /// Every transport call after the `packet`-th packet fails.
    pub fn disconnect_after_packets(&mut self, packet: usize) {
        self.faults.disconnect_after = Some(packet);
    }

    /// The user switches to `profile` on the keyboard right after the
    /// `packet`-th packet (the keyboard pushes the new profile).
    pub fn switch_profile_after_packets(&mut self, packet: usize, profile: ProfileId) {
        self.faults.switch_after.push((packet, profile.get()));
    }

    /// The next `n` profile selects are acknowledged but do not switch (and
    /// send no push).
    pub fn ignore_next_selects(&mut self, n: usize) {
        self.faults.ignore_selects = n;
    }

    /// The next `n` profile selects switch without sending the profile push.
    pub fn omit_next_pushes(&mut self, n: usize) {
        self.faults.omit_pushes = n;
    }

    /// Sets the USB identity reported by [`Transport::usb_info`].
    pub fn set_usb_info(&mut self, usb: Option<UsbInfo>) {
        self.usb = usb;
    }

    // ----- what happened -----

    /// Every packet the host sent, in order.
    pub fn written(&self) -> &[SimPacket] {
        &self.log
    }

    /// The write packets the host sent, in order.
    pub fn write_packets(&self) -> impl Iterator<Item = &WritePacket> {
        self.log.iter().filter_map(|p| match p {
            SimPacket::Write(w) => Some(w),
            SimPacket::Out(_) => None,
        })
    }

    /// Number of write packets sent.
    pub fn writes_sent(&self) -> usize {
        self.writes
    }

    /// Packets the simulator refused.
    pub fn violations(&self) -> &[Violation] {
        &self.violations
    }

    /// Whether the host read a reset notification.
    pub fn reset_delivered(&self) -> bool {
        self.reset_delivered
    }

    /// Panics with the list of violations if there are any (test helper).
    ///
    /// # Panics
    ///
    /// When [`violations`](Self::violations) is not empty.
    pub fn assert_clean(&self) {
        assert!(
            self.violations.is_empty(),
            "the simulator refused {} packet(s): {:#?}",
            self.violations.len(),
            self.violations
        );
    }

    // ----- protocol -----

    fn violation(&mut self, bytes: &[u8; REPORT_LEN], reason: impl Into<String>) {
        let end = bytes
            .iter()
            .rposition(|b| *b != 0)
            .map_or(5, |i| (i + 1).max(5));
        let hex: Vec<String> = bytes[..end].iter().map(|b| format!("{b:02x}")).collect();
        self.violations.push(Violation {
            packet: self.log.len(),
            bytes: hex.join(" "),
            reason: reason.into(),
        });
    }

    /// Bookkeeping before a packet is processed; false if the packet must be
    /// refused (disconnected).
    fn begin_packet(&mut self, packet: SimPacket) -> Result<(), TransportError> {
        if self.disconnected {
            return Err(TransportError::Write(
                "the simulated keyboard is disconnected".into(),
            ));
        }
        let bytes = *packet.as_bytes();
        self.log.push(packet);
        if self.reset_delivered {
            self.violation(
                &bytes,
                "packet sent after the keyboard's reset notification",
            );
        }
        if bytes[0] == 0x14 {
            self.violation(&bytes, "0x14 is never sent");
        }
        let n = self.log.len();
        // Released acks of an earlier write.
        let delayed = std::mem::take(&mut self.delayed);
        self.queue.extend(delayed);
        // Scheduled events.
        if self.faults.reset_after == Some(n) {
            self.queue.push_back(report(&[0x14, 0, 0, 0, 0x01, 0x01]));
        }
        let switches: Vec<u8> = self
            .faults
            .switch_after
            .iter()
            .filter(|(at, _)| *at == n)
            .map(|(_, p)| *p)
            .collect();
        for profile in switches {
            self.active = profile;
            self.queue
                .push_back(report(&[0x21, 0, 0, 0, 0x03, 0x11, profile, 0x00]));
        }
        let injected: Vec<Report> = self
            .faults
            .inject_after
            .iter()
            .filter(|(at, _)| *at == n)
            .map(|(_, r)| r.clone())
            .collect();
        self.queue.extend(injected);
        Ok(())
    }

    fn end_packet(&mut self) {
        if self.faults.disconnect_after == Some(self.log.len()) {
            self.disconnected = true;
        }
    }

    /// Queues the reply to a read request.
    fn answer(&mut self, request: ReadRequest) {
        let profile = self.active;
        if request == ReadRequest::ActiveProfile {
            self.queue
                .push_back(report(&[0x21, 0, 0, 0, 0x03, 0x11, profile, 0x01]));
            return;
        }
        if let ReadRequest::KeyTrigger(pos) = request {
            if let Some(record) = self.triggers.get(&(profile, pos.index())).copied() {
                let mut bytes = vec![0x21, 0, 0, 0, 0x0c, 0x05];
                bytes.extend_from_slice(&record);
                bytes.extend_from_slice(&[pos.row(), pos.col()]);
                self.queue.push_back(report(&bytes));
            }
            return;
        }
        let slot = self.slot(request, profile);
        if let Some(table) = SimTable::of_request(request) {
            if let Some(bytes) = self.tables.get(&(slot, table)).cloned() {
                self.queue.extend(frame(request, &bytes));
            }
            return;
        }
        if let Some(reports) = self.replay.get(&(slot, request)).cloned() {
            self.queue.extend(reports);
        }
    }

    /// Parses a write packet (an implementation of design section 1 that is
    /// independent of the protocol crate's builders).
    fn parse_write(&self, b: &[u8; REPORT_LEN]) -> Result<Effect, String> {
        let zero_from = |i: usize| b.get(i..).unwrap_or_default().iter().all(|x| *x == 0);
        match b[0] {
            0x21 => {
                if b[1..4] != [0, 0, 0] {
                    return Err("0x21 write with non-zero bytes 1-3".into());
                }
                match (b[4], b[5]) {
                    (0x00, 0x12) => {
                        if (1..=4).contains(&b[6]) && zero_from(7) {
                            Ok(Effect::Select(b[6]))
                        } else {
                            Err("profile select outside 1-4 or with extra bytes".into())
                        }
                    }
                    (0x18, mode @ (0x00 | 0x0C | 0x0D)) => {
                        let selectable = self.model.selectable_mask();
                        let mut keys = Vec::new();
                        for col in 0..22u8 {
                            let byte = b[6 + usize::from(col)];
                            for row in 0..8u8 {
                                if byte & (1 << row) == 0 {
                                    continue;
                                }
                                let pos = KeyPos::new(row, col)
                                    .filter(|p| selectable.contains(*p))
                                    .ok_or_else(|| {
                                        format!("trigger mask selects r{row}c{col}, which is not a selectable key")
                                    })?;
                                keys.push(pos);
                            }
                        }
                        if keys.is_empty() {
                            return Err("trigger write with an empty key mask".into());
                        }
                        if b[29] != b[28] || b[33] != b[32] {
                            return Err("second travel differs from the actuation point".into());
                        }
                        if !zero_from(36) {
                            return Err("trigger write with bytes after byte 35".into());
                        }
                        let mut record = [0u8; 9];
                        record[0] = mode;
                        record[1..].copy_from_slice(&b[28..36]);
                        Ok(Effect::Trigger { keys, record })
                    }
                    (len, sel) => Err(format!(
                        "0x21 sub-function {len:02x}/{sel:02x} is not an allowed write"
                    )),
                }
            }
            0x17 => {
                if b[..7] == [0x17, 0, 0, 0, 0x02, 0, 0] && zero_from(7) {
                    Ok(Effect::Leave)
                } else {
                    Err("music rhythm other than the leave packet".into())
                }
            }
            0x07 => {
                if b[1..5] == [0, 0, 0, 0x0E] && zero_from(17) && b[5] != 0x64 {
                    let mut record = [0u8; 12];
                    record.copy_from_slice(&b[5..17]);
                    Ok(Effect::Light(record))
                } else {
                    Err("main light write with a bad header, mode 0x64 or extra bytes".into())
                }
            }
            0x09 if b[1] == 0x00 => chunk(b, SimTable::CustomColors1, 54),
            0x09 => Err("custom colour table other than 1".into()),
            0x18 if b[1] == 0x00 => chunk(b, SimTable::Keymap(Layer::Base), 56),
            0x18 if b[1] == 0x02 => chunk(b, SimTable::Keymap(Layer::Fn), 56),
            0x18 => Err("keymap write sub other than 00 / 02".into()),
            0x19 => match MacroSlotId::new(b[1]) {
                Some(slot) => chunk(b, SimTable::Macro(slot), 58),
                None => Err("macro write to a slot outside 0-9".into()),
            },
            0x22 => {
                if b[1] < 40 && b[2..5] == [0, 0, 0x32] && zero_from(5) {
                    Ok(Effect::AdvancedClear(usize::from(b[1])))
                } else {
                    Err("advanced-key write with content, a bad slot or a bad header".into())
                }
            }
            0x26 if b[1] == 0x01 => {
                let max = self.model.write_ceilings.max_dead_band;
                let effect = chunk(b, SimTable::DeadBand, 58)?;
                if let Effect::Chunk { data, .. } = &effect {
                    if let Some(value) = data.iter().find(|v| u16::from(**v) > max) {
                        return Err(format!(
                            "dead-band value {value} is above the maximum {max}"
                        ));
                    }
                }
                Ok(effect)
            }
            other => Err(format!(
                "command {other:02x} with sub {:02x} is never written",
                b[1]
            )),
        }
    }

    fn table_mut(&mut self, table: SimTable) -> &mut Vec<u8> {
        let slot = self.slot(table.request(), self.active);
        self.tables
            .entry((slot, table))
            .or_insert_with(|| vec![0; table.len()])
    }

    /// Applies a write; returns the reports the keyboard sends.
    fn apply(&mut self, effect: Effect, write: usize) -> Vec<Report> {
        let no_effect = self.faults.drop_effect.contains(&write);
        let blocked = |sim: &Self, table: SimTable| no_effect || sim.faults.frozen.contains(&table);
        match effect {
            Effect::Select(profile) => {
                if self.faults.ignore_selects > 0 {
                    self.faults.ignore_selects -= 1;
                    return vec![report(&[0x21, 0, 0, 0, 0x02, 0x12, 0x01])];
                }
                if !no_effect {
                    self.active = profile;
                }
                let mut out = vec![report(&[0x21, 0, 0, 0, 0x02, 0x12, 0x01])];
                if self.faults.omit_pushes > 0 {
                    self.faults.omit_pushes -= 1;
                } else {
                    out.push(report(&[0x21, 0, 0, 0, 0x03, 0x11, self.active, 0x00]));
                }
                out
            }
            Effect::Leave => {
                self.leave_pending = true;
                let slot = self.slot(ReadRequest::Status, self.active);
                let status = self
                    .replay
                    .get(&(slot, ReadRequest::Status))
                    .and_then(|r| r.first().cloned())
                    .unwrap_or_else(|| report(&[0x01, 0, 0, 0, 0x12]));
                vec![report(&[0x17, 0, 0, 0, 0]), status]
            }
            Effect::Light(record) => {
                if !blocked(self, SimTable::MainLight) {
                    *self.table_mut(SimTable::MainLight) = record.to_vec();
                }
                Vec::new()
            }
            Effect::Chunk {
                table,
                offset,
                data,
            } => {
                if !blocked(self, table) {
                    let bytes = self.table_mut(table);
                    if let Some(dst) = bytes.get_mut(offset..offset + data.len()) {
                        dst.copy_from_slice(&data);
                    }
                }
                let ack: &[u8] = match table {
                    SimTable::CustomColors1 => &[0x09, 0, 0, 0, 0],
                    SimTable::Keymap(_) => &[0x18, 0, 0, 0, 0],
                    SimTable::Macro(_) => &[0x19, 0, 0, 0, 0],
                    SimTable::DeadBand => &[0x26, 0x01, 0, 0, 0],
                    SimTable::MainLight | SimTable::AdvancedKeys => &[],
                };
                vec![report(ack)]
            }
            Effect::AdvancedClear(slot) => {
                if !blocked(self, SimTable::AdvancedKeys) {
                    let bytes = self.table_mut(SimTable::AdvancedKeys);
                    if let Some(dst) = bytes.get_mut(slot * 40..slot * 40 + 40) {
                        dst.fill(0);
                    }
                }
                vec![report(&[0x22, 0, 0, 0, 0])]
            }
            Effect::Trigger { keys, record } => {
                if !no_effect {
                    for pos in keys {
                        self.triggers.insert((self.active, pos.index()), record);
                    }
                }
                vec![report(&[0x21, 0, 0, 0, 0x01, 0x00])]
            }
        }
    }
}

/// Parses one table chunk of a write.
fn chunk(b: &[u8; REPORT_LEN], table: SimTable, chunk_len: usize) -> Result<Effect, String> {
    let total = table.len();
    if b[2] != 0 {
        return Err("chunk index above 255".into());
    }
    let start = usize::from(b[3]) * chunk_len;
    if start >= total {
        return Err(format!("chunk {} is beyond the table", b[3]));
    }
    let len = chunk_len.min(total - start);
    if usize::from(b[4]) != len {
        return Err(format!(
            "chunk {} declares {} bytes, expected {len}",
            b[3], b[4]
        ));
    }
    if b[5 + len..].iter().any(|x| *x != 0) {
        return Err("bytes after the chunk data".into());
    }
    Ok(Effect::Chunk {
        table,
        offset: start,
        data: b[5..5 + len].to_vec(),
    })
}

/// A report from its leading bytes (rest zero).
fn report(prefix: &[u8]) -> Report {
    let mut bytes = [0u8; REPORT_LEN];
    for (dst, src) in bytes.iter_mut().zip(prefix) {
        *dst = *src;
    }
    Report::new(bytes)
}

/// The reply reports of a table read, framed like the keyboard's.
fn frame(request: ReadRequest, bytes: &[u8]) -> Vec<Report> {
    let packet = request.packet();
    let cmd = packet.cmd();
    let sub = match request {
        // `07 01` is answered with `07 00`.
        ReadRequest::MainLight => 0x00,
        _ => packet.sub(),
    };
    let Some(layout) = request.table_layout() else {
        let mut prefix = vec![cmd, sub, 0, 0, u8::try_from(bytes.len()).unwrap_or(0)];
        prefix.extend_from_slice(bytes);
        return vec![report(&prefix)];
    };
    (0..layout.chunks)
        .map(|index| {
            let start = usize::from(index) * layout.chunk_len;
            let len = layout.expected_len(index);
            let [hi, lo] = index.to_be_bytes();
            let mut prefix = vec![cmd, sub, hi, lo, u8::try_from(len).unwrap_or(0)];
            prefix.extend_from_slice(bytes.get(start..start + len).unwrap_or_default());
            report(&prefix)
        })
        .collect()
}

impl Transport for DeviceSim {
    fn write(&mut self, packet: &OutPacket) -> Result<(), TransportError> {
        self.begin_packet(SimPacket::Out(packet.clone()))?;
        let bytes = *packet.as_bytes();
        if self.leave_pending {
            self.leave_pending = false;
            self.violation(
                &bytes,
                "music-rhythm leave not followed by a main-light write",
            );
        }
        match ReadRequest::from_packet(&bytes) {
            Some(request) => self.answer(request),
            None => self.violation(&bytes, "not a documented read request"),
        }
        self.end_packet();
        Ok(())
    }

    /// Returns the next queued report at once. Otherwise "waits" on the
    /// virtual clock: up to the next report scheduled within `timeout`, or
    /// the whole timeout (then `None`). Never sleeps.
    fn read(&mut self, timeout: Duration) -> Result<Option<Report>, TransportError> {
        if self.disconnected {
            return Err(TransportError::Read(
                "the simulated keyboard is disconnected".into(),
            ));
        }
        self.release_scheduled(self.clock);
        let mut next = self.queue.pop_front();
        if next.is_none() {
            let deadline = self.clock.saturating_add(timeout);
            let due = self
                .scheduled
                .iter()
                .map(|(due, _)| *due)
                .filter(|due| *due <= deadline)
                .min();
            match due {
                Some(due) => {
                    self.clock = self.clock.max(due);
                    self.release_scheduled(self.clock);
                    next = self.queue.pop_front();
                }
                None => self.clock = deadline,
            }
        }
        if next.as_ref().is_some_and(is_keyboard_reset) {
            self.reset_delivered = true;
        }
        Ok(next)
    }

    fn usb_info(&self) -> Option<UsbInfo> {
        self.usb.clone()
    }

    /// The virtual clock (see the [module documentation](self)).
    fn now(&self) -> Instant {
        self.epoch + self.clock
    }
}

impl DeviceSim {
    /// Moves every scheduled report due at `time` into the queue, in due
    /// order.
    fn release_scheduled(&mut self, time: Duration) {
        if self.scheduled.is_empty() {
            return;
        }
        self.scheduled.sort_by_key(|(due, _)| *due);
        let due_now = self
            .scheduled
            .iter()
            .take_while(|(due, _)| *due <= time)
            .count();
        let released: Vec<(Duration, Report)> = self.scheduled.drain(..due_now).collect();
        self.queue.extend(released.into_iter().map(|(_, r)| r));
    }
}

impl WriteTransport for DeviceSim {
    fn write_command(&mut self, packet: &WritePacket) -> Result<(), TransportError> {
        self.begin_packet(SimPacket::Write(packet.clone()))?;
        self.writes += 1;
        let write = self.writes;
        let bytes = *packet.as_bytes();
        if ReadRequest::from_packet(&bytes).is_some() {
            self.violation(&bytes, "a read request sent as a write");
        }
        let effect = self.parse_write(&bytes);
        let leave_was_pending = std::mem::take(&mut self.leave_pending);
        match effect {
            Err(reason) => self.violation(&bytes, reason),
            Ok(effect) => {
                let current_mode = self
                    .table(
                        ProfileId::new(self.active).unwrap_or(ProfileId::ALL[0]),
                        SimTable::MainLight,
                    )
                    .and_then(|b| b.first().copied());
                let changes_mode =
                    matches!(&effect, Effect::Light(r) if Some(r[0]) != current_mode);
                if leave_was_pending && !changes_mode {
                    self.violation(
                        &bytes,
                        "music-rhythm leave not followed by a mode-changing main-light write",
                    );
                }
                if changes_mode && !leave_was_pending {
                    self.violation(
                        &bytes,
                        "main-light write that changes the mode without the music-rhythm leave",
                    );
                }
                let replies = self.apply(effect, write);
                if self.faults.drop_ack.contains(&write) {
                    // The keyboard stays silent.
                } else if let Some(delay) = self.faults.delay_ack_by.get(&write).copied() {
                    let due = self.clock.saturating_add(delay);
                    self.scheduled
                        .extend(replies.into_iter().map(|report| (due, report)));
                } else if self.faults.delay_ack.contains(&write) {
                    self.delayed.extend(replies);
                } else {
                    self.queue.extend(replies);
                }
            }
        }
        self.end_packet();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwraith_protocol::{
        AdvancedSlotId, ColorTableData, DeadBandData, DeadBandWrite, Distance, FromResponse,
        KeyMask, KeymapData, LightLimits, LightMode, LightModeList, LightRecord, MacroData,
        MainLightWrite, RawResponse, TriggerGlobal, TriggerLimits, TriggerSetting, TriggerWrite,
        TriggerWriteMode, WriteRequest,
    };

    fn sim() -> DeviceSim {
        DeviceSim {
            model: &W75,
            scope: ScopeConfig::default(),
            active: 1,
            replay: HashMap::new(),
            tables: HashMap::new(),
            triggers: HashMap::new(),
            queue: VecDeque::new(),
            delayed: Vec::new(),
            scheduled: Vec::new(),
            epoch: Instant::now(),
            clock: Duration::ZERO,
            log: Vec::new(),
            violations: Vec::new(),
            faults: Faults::default(),
            writes: 0,
            leave_pending: false,
            reset_delivered: false,
            disconnected: false,
            usb: None,
        }
    }

    fn raw(request: ReadRequest, bytes: &[u8]) -> RawResponse {
        RawResponse::from_reports(request, frame(request, bytes)).expect("valid reply")
    }

    fn packet(prefix: &[u8]) -> [u8; REPORT_LEN] {
        *report(prefix).as_bytes()
    }

    fn w75_modes() -> LightModeList {
        LightModeList::from_response(&raw(
            ReadRequest::MainLightModes,
            &[
                0x02, 0x01, 0x03, 0x09, 0x04, 0x00, 0x0e, 0x0f, 0x06, 0x07, 0x0b, 0x0c, 0x10, 0x08,
                0xa5, 0x5a, 0x04, 0x04,
            ],
        ))
        .expect("modes")
    }

    fn custom_red() -> LightRecord {
        LightRecord::from_response(&raw(
            ReadRequest::MainLight,
            &[0x0a, 4, 3, 0xff, 0, 0, 0, 0, 0, 0, 1, 0],
        ))
        .expect("record")
    }

    /// One request of every kind, built by the protocol crate.
    fn sample() -> Vec<WriteRequest> {
        let mut out: Vec<WriteRequest> = ProfileId::ALL
            .into_iter()
            .map(WriteRequest::SelectProfile)
            .collect();
        let limits = LightLimits::new(&W75, &w75_modes());
        let current = custom_red();
        for target in [
            current.with_brightness(1),
            current.with_mode(LightMode::from_id(0x02)),
        ] {
            out.push(WriteRequest::MainLight(
                MainLightWrite::new(&current, target, &limits).expect("valid"),
            ));
        }
        out.push(WriteRequest::CustomColors1(
            ColorTableData::from_response(&raw(
                ReadRequest::CustomColors(CustomTable::Custom1),
                &[0x11; 396],
            ))
            .expect("colours"),
        ));
        for layer in Layer::ALL {
            let (layer, data) =
                KeymapData::from_response(&raw(ReadRequest::Keymap(layer), &[0x22; 528]))
                    .expect("keymap");
            out.push(WriteRequest::Keymap(layer, data));
        }
        for slot in MacroSlotId::all() {
            let (slot, data) =
                MacroData::from_response(&raw(ReadRequest::Macro(slot), &[0xFF; 256]))
                    .expect("macro");
            out.push(WriteRequest::Macro(slot, data));
        }
        out.extend(AdvancedSlotId::all().map(WriteRequest::AdvancedKeyClear));
        let global = TriggerGlobal::from_response(&raw(
            ReadRequest::TriggerGlobal,
            &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
        ))
        .expect("21/04");
        let trigger_limits = TriggerLimits::new(&W75, &global);
        for pos in W75.selectable_mask().positions() {
            let setting = TriggerSetting {
                mode: TriggerWriteMode::RapidTriggerSeparate,
                actuation: Distance(0x55),
                rt_press: Distance(0x0f),
                rt_release: Distance(0x19),
            };
            let write = TriggerWrite::new(
                &W75,
                KeyMask::from_positions([pos]),
                setting,
                &trigger_limits,
            )
            .expect("valid");
            out.push(WriteRequest::Trigger(write));
        }
        let mut dead_band = vec![0u8; 264];
        dead_band[92] = 5;
        dead_band[93] = 10;
        out.push(WriteRequest::DeadBand(
            DeadBandWrite::new(
                &W75,
                DeadBandData::from_response(&raw(ReadRequest::DeadBand, &dead_band))
                    .expect("dead band"),
                50,
            )
            .expect("in range"),
        ));
        out
    }

    #[test]
    fn every_built_write_is_accepted_by_the_independent_parser() {
        let sim = sim();
        let mut count = 0;
        for request in sample() {
            for step in request.steps() {
                count += 1;
                assert!(
                    sim.parse_write(step.packet.as_bytes()).is_ok(),
                    "{request}: {:?}",
                    step.packet
                );
            }
        }
        assert!(count > 200, "{count}");
    }

    #[test]
    fn forbidden_packets_are_refused() {
        let sim = sim();
        let mut empty_mask = packet(&[0x21, 0, 0, 0, 0x18, 0x00]);
        empty_mask[28] = 0x55;
        empty_mask[29] = 0x55;
        let mut knob = packet(&[0x21, 0, 0, 0, 0x18, 0x00]);
        // r0c15, the knob.
        knob[6 + 15] = 0x01;
        knob[28] = 0x55;
        knob[29] = 0x55;
        let mut second_travel = packet(&[0x21, 0, 0, 0, 0x18, 0x0c]);
        second_travel[6 + 2] = 0x04;
        second_travel[28] = 0x32;
        second_travel[29] = 0x33;
        let mut bit6 = second_travel;
        bit6[29] = 0x32;
        bit6[6 + 2] = 0x44;
        // Chunk 1 with 0x33 (51 units) at its first byte.
        let mut dead_band_51 = packet(&[0x26, 0x01, 0, 1, 0x3a]);
        dead_band_51[5] = 0x33;
        let mut advanced_content = packet(&[0x22, 0x03, 0, 0, 0x32]);
        advanced_content[10] = 1;
        let forbidden: Vec<(&str, [u8; REPORT_LEN])> = vec![
            ("reset", packet(&[0x14, 0, 0, 0, 0x01, 0x01])),
            ("win-lock", packet(&[0x14, 0, 0, 0, 0x01, 0x00])),
            ("gamepad", packet(&[0x14, 0x03, 0, 0, 0x01, 0x01])),
            ("music enter", packet(&[0x17, 0, 0, 0, 0x02, 0x01, 0x01])),
            ("side light", packet(&[0x08, 0, 0, 0, 0x0e, 0x01])),
            ("custom 2", packet(&[0x09, 0x01, 0, 0, 0x36])),
            ("socd pairs", packet(&[0x24, 0x00, 0, 0, 0x28])),
            ("socd power", packet(&[0x24, 0x03, 0, 0, 0x01, 0x01])),
            ("switch types", packet(&[0x25, 0x01, 0, 0, 0x3a])),
            ("reset triggers", packet(&[0x21, 0, 0, 0, 0x18, 0x06])),
            ("calibrate key", packet(&[0x21, 0, 0, 0, 0x18, 0x07, 2, 2])),
            ("calibrate all", packet(&[0x21, 0, 0, 0, 0x18, 0x08, 0x00])),
            ("any-key start", packet(&[0x21, 0, 0, 0, 0x18, 0x0f, 0x00])),
            ("any-key stop", packet(&[0x21, 0, 0, 0, 0x18, 0x10, 0x00])),
            ("travel open", packet(&[0x21, 0, 0, 0, 0x18, 0x02, 0x3f])),
            ("travel close", packet(&[0x21, 0, 0, 0, 0x18, 0x03])),
            (
                "poll rate",
                packet(&[0x21, 0, 0, 0, 0x00, 0x09, 0x01, 0x01]),
            ),
            ("empty mask", empty_mask),
            ("knob", knob),
            ("second travel", second_travel),
            ("mask bit 6", bit6),
            ("advanced content", advanced_content),
            ("advanced slot 40", packet(&[0x22, 40, 0, 0, 0x32])),
            ("macro slot 10", packet(&[0x19, 10, 0, 0, 0x3a])),
            ("keymap sub 01", packet(&[0x18, 0x01, 0, 0, 0x38])),
            ("keymap read sub", packet(&[0x18, 0x80, 0, 0, 0x38])),
            ("short chunk", packet(&[0x18, 0x00, 0, 0, 0x37])),
            ("chunk beyond", packet(&[0x26, 0x01, 0, 5, 0x3a])),
            ("select 0", packet(&[0x21, 0, 0, 0, 0x00, 0x12, 0x00])),
            ("select 5", packet(&[0x21, 0, 0, 0, 0x00, 0x12, 0x05])),
            ("music light", packet(&[0x07, 0, 0, 0, 0x0e, 0x64, 4])),
            ("dead band read", packet(&[0x26, 0x00])),
            ("dead band 51", dead_band_51),
            ("unknown", packet(&[0x33, 0x01])),
        ];
        for (name, bytes) in forbidden {
            assert!(sim.parse_write(&bytes).is_err(), "{name} was accepted");
        }
    }

    #[test]
    fn a_leave_must_be_followed_by_a_mode_changing_light_write() {
        let current = custom_red();
        let write = MainLightWrite::new(
            &current,
            current.with_mode(LightMode::from_id(0x02)),
            &LightLimits::new(&W75, &w75_modes()),
        )
        .expect("valid");
        let steps = WriteRequest::MainLight(write).steps();
        assert_eq!(steps[0].packet.cmd(), 0x17);

        // Leave, then a read: refused.
        let mut s = sim();
        s.tables
            .insert((1, SimTable::MainLight), current.as_bytes().to_vec());
        s.write_command(&steps[0].packet).expect("sent");
        s.write(&ReadRequest::Status.packet()).expect("sent");
        assert_eq!(s.violations().len(), 1, "{:?}", s.violations());

        // Leave, then the mode-changing light write: accepted.
        let mut s = sim();
        s.tables
            .insert((1, SimTable::MainLight), current.as_bytes().to_vec());
        for step in &steps {
            s.write_command(&step.packet).expect("sent");
        }
        s.assert_clean();
        // The ack and the status push.
        assert_eq!(s.queue.len(), 2);
    }

    #[test]
    fn a_mode_change_needs_the_leave() {
        // A write planned against a stale state (mode 02) while the keyboard
        // holds mode 0a: the builder sees no mode change and sends no leave.
        let stale = custom_red().with_mode(LightMode::from_id(0x02));
        let write = MainLightWrite::new(
            &stale,
            stale.with_brightness(1),
            &LightLimits::new(&W75, &w75_modes()),
        )
        .expect("valid");
        let steps = WriteRequest::MainLight(write).steps();
        assert_eq!(steps.len(), 1);
        let mut s = sim();
        s.tables
            .insert((1, SimTable::MainLight), custom_red().as_bytes().to_vec());
        s.write_command(&steps[0].packet).expect("sent");
        assert_eq!(s.violations().len(), 1, "{:?}", s.violations());
        assert!(
            s.violations()[0]
                .reason
                .contains("without the music-rhythm leave")
        );
    }

    #[test]
    fn nothing_may_follow_a_reset_notification() {
        let mut s = sim();
        s.push_input(report(&[0x14, 0, 0, 0, 0x01, 0x01]));
        assert!(s.read(Duration::ZERO).expect("read").is_some());
        assert!(s.reset_delivered());
        s.write(&ReadRequest::Status.packet()).expect("sent");
        assert_eq!(s.violations().len(), 1);
        assert!(s.violations()[0].reason.contains("reset"));
    }

    #[test]
    fn reads_wait_on_the_virtual_clock() {
        let mut s = sim();
        let start = s.now();
        assert!(s.read(Duration::from_millis(500)).expect("read").is_none());
        assert_eq!(s.now() - start, Duration::from_millis(500));
        s.push_input_at(Duration::from_secs(1), report(&[0x01, 0, 0, 0, 0x12]));
        // Not yet due within 300 ms.
        assert!(s.read(Duration::from_millis(300)).expect("read").is_none());
        assert_eq!(s.elapsed(), Duration::from_millis(800));
        // Due within the next 2 s: the clock stops at its arrival.
        let got = s.read(Duration::from_secs(2)).expect("read");
        assert_eq!(got.map(|r| r.cmd()), Some(0x01));
        assert_eq!(s.elapsed(), Duration::from_millis(1500));
        // Queued reports come at once, without advancing the clock.
        s.push_input(report(&[0x01]));
        assert!(s.read(Duration::from_secs(5)).expect("read").is_some());
        assert_eq!(s.elapsed(), Duration::from_millis(1500));
    }

    #[test]
    fn replies_are_framed_like_the_keyboard() {
        let mut s = sim();
        s.tables
            .insert((1, SimTable::Keymap(Layer::Base)), vec![7; 528]);
        s.write(&ReadRequest::Keymap(Layer::Base).packet())
            .expect("sent");
        let reports: Vec<Report> = s.queue.drain(..).collect();
        assert_eq!(reports.len(), 10);
        let reply =
            RawResponse::from_reports(ReadRequest::Keymap(Layer::Base), reports).expect("valid");
        assert_eq!(reply.table_bytes("t").expect("table"), vec![7; 528]);
        s.write(&ReadRequest::ActiveProfile.packet()).expect("sent");
        assert_eq!(
            s.queue.pop_front().map(|r| r.as_bytes()[..8].to_vec()),
            Some(vec![0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x01])
        );
        s.assert_clean();
    }
}
