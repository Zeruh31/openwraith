//! `DeviceSim`: a stateful simulator of the Wraith W75 for tests (feature
//! `mock`). No HID device is involved.
//!
//! The simulator holds four onboard profiles. Main light, per-key triggers,
//! both keymap layers, the macro and advanced-key slots, custom colours 1,
//! the dead band and the SOCD pairs are per profile by default, and the
//! SOCD power is one setting of the whole keyboard (OBSERVED on hardware);
//! the scope of these and of the tables never written can be configured as
//! per-profile or global with a [`ScopeConfig`]. Device constants (`0D`,
//! `0E`, `0A`, `02`, `21/04`, `25 00`) are replayed from the seed backup.
//!
//! Read requests are answered with exactly framed replies. Write packets are
//! parsed by an implementation of the write forms (W1-W11, the travel-test
//! open and close T1/T2) written independently from the protocol crate's
//! builders (design section 1);
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
//! profile change (right after a packet, or at a virtual time with or
//! without a profile report: [`switch_profile_at`](DeviceSim::switch_profile_at)),
//! foreign reports, a select that is acknowledged but does not switch, a
//! missing profile push, and tables that never take writes (verification
//! mismatch).
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
//!
//! # Delayed commit
//!
//! The real keyboard keeps edits in a working copy of the active profile and
//! saves them about 1 s after the last write; a profile switch before that
//! reloads the profile and the edits are lost (CONFIRMED on hardware,
//! spec 4.5.3 "Delayed commit"). The simulator models this by default:
//!
//! * every write that changes a setting goes to the **working copy** of the
//!   active profile, and reads of the active profile return it (so a
//!   read-back right after a write always matches);
//! * the working copy is **saved** once [`commit_delay`](DeviceSim::commit_delay)
//!   (default [`DEFAULT_COMMIT_DELAY`], 1 s of virtual time) has passed since
//!   the last such write;
//! * a profile select (ours or one made on the keyboard) or a
//!   [`power_cycle`](DeviceSim::power_cycle) before that **discards** the
//!   unsaved edits ([`discarded_edits`](DeviceSim::discarded_edits) counts
//!   it).
//!
//! The model is a guess at the firmware's behaviour (GUESS): it treats the
//! global tables (the SOCD power) like the per-profile ones (a switch drops
//! their unsaved edits too). [`set_commit_delay`](DeviceSim::set_commit_delay)
//! with zero turns it off (every write is saved at once, except the keymap
//! edits below); only tests that need instant saves use it.
//!
//! # Keymap-group edits that need an Fn-keymap write
//!
//! On the real keyboard an `18 00` (base keymap) write on its own is acked
//! and reads back at once, but is gone after a profile switch, even after
//! 6 s; written with an `18 02` (Fn keymap) after it, both layers are kept
//! (CONFIRMED on hardware, spec 4.8.2). The simulator models this with
//! [`KeymapCommit`] (on by default in every test): base-keymap edits in the
//! working copy are **not** saved by the quiet-time commit. A **complete**
//! `18 02` write (every chunk applied) releases them to it: they are saved
//! together with the Fn keymap once the commit delay has passed since the
//! last `18 02` chunk, and a profile switch before that loses both layers.
//! A partial `18 02` releases nothing (whether it would on the keyboard is
//! unknown; every hardware probe sent complete tables). An edit made after
//! the `18 02` waits for the next one.
//!
//! An advanced-key slot (`22`) is kept only when a complete `18 02` write
//! is applied in the same save window (from the first unsaved edit to the
//! save or the reload of the working copy): written before the slot, as
//! the vendor site does, the slot is kept; a slot written without any
//! `18 02` reads back at once but is gone after a profile switch
//! (CONFIRMED on hardware, H20, 2026-10-27: `hw/v02/h20-2b.txt` and
//! `h20-2.txt`). A complete `18 02` after the slot releases it like a base
//! keymap (GUESS: never tested). With the delayed commit off every write is
//! saved at once, so each write is a window of its own. A macro slot (`19`)
//! written alone is kept (CONFIRMED, H19, `hw/v02/h19-2.txt`). The `18 02`
//! chunks themselves, the macro slots, the dead band, SOCD, triggers, the
//! main light and the colour tables keep the quiet-time commit
//! ([`KeymapCommit::macros_need_fn_write`] makes macros wait too, to show
//! that the persistence check reports such a loss).
//!
//! # Writing macro slot 0 clears slots 1-9
//!
//! On the real keyboard any write to macro slot 0 (`19 00`, empty or not)
//! empties macro slots 1-9 of the active profile; writes to slots 1-9 touch
//! only their own slot, and other profiles keep theirs (CONFIRMED on
//! hardware: H24 and `hw/v02/macro-exp.txt`, 2026-10-27; the vendor site
//! never shows it because it always writes all ten slots in ascending
//! order). The slots read back as all `FF` afterwards (OBSERVED,
//! `hw/v02/v02-final.owbackup.json`). The simulator empties them (256 x
//! `FF`) in the working copy when the **first** chunk of a slot-0 write is
//! applied (GUESS: the hardware re-read came after all five chunks), and
//! saves the cleared slots like any other macro edit (a profile switch
//! before the commit brings them back).
//!
//! # Keyboard events
//!
//! [`press_socd_hotkey_at`](DeviceSim::press_socd_hotkey_at) switches the
//! SOCD power (of every profile, by default) on the keyboard and pushes the
//! `24 02` report, as Fn + Right Shift does (c15, H12, H21). While a travel test is open (`21 .. 18 02`), a
//! [`travel_script`](DeviceSim::travel_script) is streamed as short and long
//! `21 .. (05|03) 01` reports (c14 shapes); reports due while the test is
//! closed are not sent. [`leave_travel_open`](DeviceSim::leave_travel_open)
//! models a test a crashed run left open, and [`DeviceSim::assert_clean`]
//! refuses a test left open by the code under test (unless
//! [`allow_travel_left_open`](DeviceSim::allow_travel_left_open)).

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use openwraith_protocol::{
    CustomTable, KeyPos, KeymapData, KeymapReference, Layer, MacroSlotId, Model, OutPacket,
    ProfileId, REPORT_LEN, RawResponse, ReadRequest, Report, W75, WritePacket, is_keyboard_reset,
};

use crate::backup::Backup;
use crate::error::TransportError;
use crate::group::SlotRef;
use crate::transport::{Transport, UsbInfo, WriteTransport};

/// Whether a simulated table belongs to each profile or is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Each onboard profile has its own copy.
    PerProfile,
    /// One copy shared by all profiles.
    Global,
}

/// The scope of the tables a test may choose: the ones OpenWraith writes,
/// whose scope hardware showed (the defaults), and the ones never written,
/// whose scope is unknown (design section 2).
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
    /// SOCD power and hotkey bytes (`24 02`).
    pub socd_power: Scope,
    /// SOCD pairs (`24 01`).
    pub socd_pairs: Scope,
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
            socd_power: scope,
            socd_pairs: scope,
            switch_types: scope,
            poll_rate: scope,
            status: scope,
        }
    }
}

impl Default for ScopeConfig {
    /// What hardware showed: the Fn keymap, advanced keys, custom colours 1
    /// (spec 7.3 and 7.4), the dead band (H16) and the SOCD pairs (H21) per
    /// profile, the SOCD power one setting of the whole keyboard (H21:
    /// switched on in profile 2, it reads on in every profile, and Fn +
    /// Right Shift switches it off in all of them; 2026-10-27). The tables
    /// never written are global.
    fn default() -> Self {
        ScopeConfig {
            fn_keymap: Scope::PerProfile,
            advanced_keys: Scope::PerProfile,
            dead_band: Scope::PerProfile,
            socd_pairs: Scope::PerProfile,
            custom_colors1: Scope::PerProfile,
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
    /// SOCD pairs (80 bytes, `24 01`).
    SocdPairs,
    /// SOCD power and hotkey (6 bytes, `24 02`; byte 0 = power).
    SocdSettings,
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
            SimTable::SocdPairs => 80,
            SimTable::SocdSettings => 6,
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
            ReadRequest::SocdPairs => Some(SimTable::SocdPairs),
            ReadRequest::SocdSettings => Some(SimTable::SocdSettings),
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
            SimTable::SocdPairs => ReadRequest::SocdPairs,
            SimTable::SocdSettings => ReadRequest::SocdSettings,
        }
    }

    /// Whether the table is a single-report reply whose bytes are the
    /// payload (not a chunked table).
    fn is_record(self) -> bool {
        matches!(self, SimTable::MainLight | SimTable::SocdSettings)
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

/// How long after the last write the simulated keyboard saves its working
/// copy (1 s): the hardware probe lost writes when the next profile select
/// followed within 800 ms and kept them from 1000 ms on (spec 4.5.3).
pub const DEFAULT_COMMIT_DELAY: Duration = Duration::from_secs(1);

/// Data bytes per keymap chunk (`18 00` / `18 02`).
const KEYMAP_CHUNK_LEN: usize = 56;

/// Which keymap-group edits the simulated keyboard saves only together with
/// a complete `18 02` (Fn keymap) write, instead of by the quiet-time commit
/// alone (see "Keymap-group edits that need an Fn-keymap write" in the
/// [module documentation](self)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeymapCommit {
    /// Base-keymap (`18 00`) edits wait for a complete `18 02` write after
    /// them, which releases them to the quiet-time commit; a profile switch
    /// or a power cycle before that commit loses them (CONFIRMED on
    /// hardware, spec 4.8.2). Default: on.
    pub base_needs_fn_write: bool,
    /// Macro (`19`) edits wait for a complete `18 02` write after them too.
    /// Off by default: a macro slot written alone is kept (CONFIRMED on
    /// hardware, H19 step 2, 2026-10-27); the switch exists to show that
    /// the persistence check reports such a loss.
    pub macros_need_fn_write: bool,
    /// Advanced-key (`22`) edits are saved only when a complete `18 02`
    /// write is applied in the same save window: before them (CONFIRMED on
    /// hardware, H20 step 2, 2026-10-27: `18 02` then `22` was kept, a lone
    /// `22` was lost after a profile switch) or after them (GUESS, never
    /// tested; it releases them like a base keymap). Default: on.
    pub advanced_need_fn_write: bool,
}

impl Default for KeymapCommit {
    /// The hardware findings: the base keymap and the advanced-key slots
    /// need the `18 02`, macro slots do not.
    fn default() -> Self {
        KeymapCommit {
            base_needs_fn_write: true,
            macros_need_fn_write: false,
            advanced_need_fn_write: true,
        }
    }
}

impl KeymapCommit {
    /// Whether edits of `table` wait for an `18 02` write under this rule.
    fn waits_for_fn_write(self, table: SimTable) -> bool {
        match table {
            SimTable::Keymap(Layer::Base) => self.base_needs_fn_write,
            SimTable::Macro(_) => self.macros_need_fn_write,
            SimTable::AdvancedKeys => self.advanced_need_fn_write,
            SimTable::Keymap(Layer::Fn)
            | SimTable::MainLight
            | SimTable::CustomColors1
            | SimTable::DeadBand
            | SimTable::SocdPairs
            | SimTable::SocdSettings => false,
        }
    }
}

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
    /// Profile switches on the keyboard at a virtual time: (due, profile,
    /// whether the keyboard pushes a profile report).
    switch_at: Vec<(Duration, u8, bool)>,
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
    /// A non-empty advanced-key slot (DKS, MT, TGL): slot and 40 bytes.
    AdvancedSlot(usize, [u8; 40]),
    Trigger {
        keys: Vec<KeyPos>,
        record: [u8; 9],
    },
    /// SOCD power `24 03` (0 or 1).
    SocdPower(u8),
    /// Travel test open (`21 .. 18 02` + the layout mask).
    TravelOpen,
    /// Travel test close (`21 .. 18 03`).
    TravelClose,
}

/// A scripted key movement for the travel stream: depth (trigger units)
/// at a virtual time.
#[derive(Debug, Clone, Copy)]
struct TravelPoint {
    due: Duration,
    pos: KeyPos,
    depth: u16,
    /// Short (fast motion) or long (slow motion) report.
    short: bool,
    /// Depth of the key's previous point (for the long variant's motion).
    previous: u16,
}

/// A stateful W75 simulator implementing [`Transport`] and
/// [`WriteTransport`]. See the [module documentation](self).
#[derive(Debug, Clone)]
pub struct DeviceSim {
    model: &'static Model,
    scope: ScopeConfig,
    active: u8,
    replay: HashMap<(u8, ReadRequest), Vec<Report>>,
    /// The saved tables, keyed by storage slot (0 = global).
    tables: HashMap<(u8, SimTable), Vec<u8>>,
    /// The saved trigger records, keyed by (profile, key index).
    triggers: HashMap<(u8, u8), [u8; 9]>,
    /// Unsaved edits of the active profile (the working copy), keyed like
    /// `tables`; see "Delayed commit" in the module documentation.
    unsaved_tables: HashMap<(u8, SimTable), Vec<u8>>,
    /// Unsaved trigger edits of the active profile, keyed like `triggers`.
    unsaved_triggers: HashMap<(u8, u8), [u8; 9]>,
    /// Virtual time of the last write that changed a setting, while its
    /// edits are unsaved.
    last_edit: Option<Duration>,
    commit_delay: Duration,
    /// Which keymap-group edits wait for an `18 02` write.
    keymap_commit: KeymapCommit,
    /// Unsaved edits that waited for an `18 02` write and were released to
    /// the quiet-time commit by a complete one (keyed like `tables`).
    released: HashSet<(u8, SimTable)>,
    /// The `18 02` chunks applied to the active profile since the last
    /// release or reload (bit n = chunk n).
    fn_chunks: u16,
    /// Whether a complete `18 02` write was applied since the working copy
    /// was last saved or reloaded (the save window, see [`KeymapCommit`]).
    fn_written_in_window: bool,
    discarded: usize,
    queue: VecDeque<Report>,
    delayed: Vec<Report>,
    /// Reports that arrive at a virtual time (offset from `epoch`).
    scheduled: Vec<(Duration, Report)>,
    epoch: Instant,
    clock: Duration,
    log: Vec<SimPacket>,
    /// The virtual time of each packet in `log`.
    log_times: Vec<Duration>,
    violations: Vec<Violation>,
    faults: Faults,
    writes: usize,
    leave_pending: bool,
    reset_delivered: bool,
    disconnected: bool,
    usb: Option<UsbInfo>,
    /// Whether the travel test is open.
    travel_open: bool,
    travel_opens: usize,
    travel_closes: usize,
    /// Whether a profile select keeps an open travel test running.
    travel_survives_select: bool,
    /// Whether a test left open at the end is accepted by `assert_clean`.
    travel_left_open_allowed: bool,
    /// Scripted stream reports, by due time.
    travel_points: Vec<TravelPoint>,
    /// Stream reports sent.
    travel_reports_sent: usize,
    /// Fn + Right Shift presses at a virtual time.
    socd_hotkey_at: Vec<Duration>,
    /// Whether the SOCD power write is acknowledged (never seen on hardware).
    socd_power_acked: bool,
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
        let mut sim = DeviceSim::blank(backup.model().unwrap_or(&W75), scope, active);
        sim.usb = backup.device.usb.clone();
        // Global tables from the primary snapshot first, then each profile.
        sim.seed(0, backup.primary());
        for profile in ProfileId::ALL {
            let view = backup.snapshot(profile).unwrap_or_else(|| backup.primary());
            sim.seed(profile.get(), view);
        }
        sim
    }

    /// A simulator without any table, on profile `active`, with the delayed
    /// commit and the default [`KeymapCommit`] on.
    fn blank(model: &'static Model, scope: ScopeConfig, active: u8) -> Self {
        DeviceSim {
            model,
            scope,
            active,
            replay: HashMap::new(),
            tables: HashMap::new(),
            triggers: HashMap::new(),
            unsaved_tables: HashMap::new(),
            unsaved_triggers: HashMap::new(),
            last_edit: None,
            commit_delay: DEFAULT_COMMIT_DELAY,
            keymap_commit: KeymapCommit::default(),
            released: HashSet::new(),
            fn_chunks: 0,
            fn_written_in_window: false,
            discarded: 0,
            queue: VecDeque::new(),
            delayed: Vec::new(),
            scheduled: Vec::new(),
            epoch: Instant::now(),
            clock: Duration::ZERO,
            log: Vec::new(),
            log_times: Vec::new(),
            violations: Vec::new(),
            faults: Faults::default(),
            writes: 0,
            leave_pending: false,
            reset_delivered: false,
            disconnected: false,
            usb: None,
            travel_open: false,
            travel_opens: 0,
            travel_closes: 0,
            travel_survives_select: true,
            travel_left_open_allowed: false,
            travel_points: Vec::new(),
            travel_reports_sent: 0,
            socd_hotkey_at: Vec::new(),
            socd_power_acked: false,
        }
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
                let bytes = if table.is_record() {
                    raw.payload()
                } else {
                    raw.table_bytes("sim").unwrap_or_else(|_| raw.payload())
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
            ReadRequest::SocdSettings => s.socd_power,
            ReadRequest::SocdPairs => s.socd_pairs,
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

    /// Makes `profile` active at once, without any report, like a switch on
    /// the keyboard: unsaved edits whose commit is not due yet are discarded
    /// (meant for test setup; a switch at a later virtual time is
    /// [`switch_profile_at`](Self::switch_profile_at)).
    pub fn set_active_profile(&mut self, profile: ProfileId) {
        self.discard_unsaved();
        self.active = profile.get();
    }

    /// The bytes of `table` as a read in `profile` would return them now:
    /// for the active profile its working copy (including unsaved writes),
    /// for another profile what it holds after switching to it (its saved
    /// state; unsaved edits count only once their commit is due).
    pub fn table(&self, profile: ProfileId, table: SimTable) -> Option<&[u8]> {
        self.table_view(profile, table, profile.get() == self.active)
    }

    /// The saved bytes of `table` in `profile`: what survives a profile
    /// switch or a power cycle made now (unsaved edits count only once their
    /// commit is due; edits that still wait for an `18 02` write, see
    /// [`KeymapCommit`], do not count).
    pub fn persisted_table(&self, profile: ProfileId, table: SimTable) -> Option<&[u8]> {
        self.table_view(profile, table, false)
    }

    fn table_view(&self, profile: ProfileId, table: SimTable, working: bool) -> Option<&[u8]> {
        let key = (self.slot(table.request(), profile.get()), table);
        let saved_by_timer = self.commit_due() && !self.waits(key);
        let unsaved = (working || saved_by_timer)
            .then(|| self.unsaved_tables.get(&key))
            .flatten();
        unsaved.or_else(|| self.tables.get(&key)).map(Vec::as_slice)
    }

    /// Replaces the saved bytes of `table` in `profile` (for a global table:
    /// in every profile), dropping an unsaved edit of it (test setup: as if
    /// the change had been made and saved long ago).
    pub fn set_table(&mut self, profile: ProfileId, table: SimTable, bytes: Vec<u8>) {
        let key = (self.slot(table.request(), profile.get()), table);
        self.unsaved_tables.remove(&key);
        self.released.remove(&key);
        self.tables.insert(key, bytes);
    }

    /// The trigger record (`MM A A P R Ah Ah Ph Rh`) of a key as a read in
    /// `profile` would return it now (see [`table`](Self::table)).
    pub fn trigger(&self, profile: ProfileId, pos: KeyPos) -> Option<[u8; 9]> {
        self.trigger_view(profile, pos, profile.get() == self.active)
    }

    /// The saved trigger record of a key in `profile` (see
    /// [`persisted_table`](Self::persisted_table)).
    pub fn persisted_trigger(&self, profile: ProfileId, pos: KeyPos) -> Option<[u8; 9]> {
        self.trigger_view(profile, pos, false)
    }

    fn trigger_view(&self, profile: ProfileId, pos: KeyPos, working: bool) -> Option<[u8; 9]> {
        let key = (profile.get(), pos.index());
        let unsaved = (working || self.commit_due())
            .then(|| self.unsaved_triggers.get(&key))
            .flatten();
        unsaved.or_else(|| self.triggers.get(&key)).copied()
    }

    /// Replaces the saved trigger record of a key in `profile`, dropping an
    /// unsaved edit of it (test setup).
    pub fn set_trigger(&mut self, profile: ProfileId, pos: KeyPos, record: [u8; 9]) {
        let key = (profile.get(), pos.index());
        self.unsaved_triggers.remove(&key);
        self.triggers.insert(key, record);
    }

    // ----- delayed commit -----

    /// How long after the last write that changed a setting the keyboard
    /// saves its working copy (see "Delayed commit" in the
    /// [module documentation](self)).
    pub fn commit_delay(&self) -> Duration {
        self.commit_delay
    }

    /// Sets the commit delay. `Duration::ZERO` turns the delayed commit off
    /// (every write is saved at once, except the keymap-group edits that
    /// wait for an `18 02` write, see [`KeymapCommit`]: they are saved when
    /// a complete one is applied); only for tests that need instant saves.
    /// The default is [`DEFAULT_COMMIT_DELAY`].
    pub fn set_commit_delay(&mut self, delay: Duration) {
        self.commit_delay = delay;
        self.commit_if_due();
    }

    /// Which keymap-group edits wait for an `18 02` write (see the
    /// [module documentation](self)).
    pub fn keymap_commit(&self) -> KeymapCommit {
        self.keymap_commit
    }

    /// Sets which keymap-group edits wait for an `18 02` write. Edits that
    /// are in the working copy already follow the new rule from now on.
    pub fn set_keymap_commit(&mut self, rule: KeymapCommit) {
        self.keymap_commit = rule;
        self.commit_if_due();
    }

    /// Whether the working copy holds writes that are not saved yet (their
    /// commit is not due, or they wait for an `18 02` write): a profile
    /// switch or a power cycle now would lose them.
    pub fn has_unsaved_edits(&self) -> bool {
        let waiting = self.unsaved_tables.keys().any(|key| self.waits(*key));
        let timed = !self.unsaved_triggers.is_empty()
            || self.unsaved_tables.keys().any(|key| !self.waits(*key));
        waiting || (timed && !self.commit_due())
    }

    /// Whether the working copy holds keymap-group edits that only a
    /// complete `18 02` write would release to the quiet-time commit (see
    /// [`KeymapCommit`]): until then the quiet-time commit never saves them.
    pub fn has_edits_waiting_for_fn_write(&self) -> bool {
        self.unsaved_tables.keys().any(|key| self.waits(*key))
    }

    /// How often unsaved edits were discarded: by a profile switch before
    /// their commit (ours or one made on the keyboard), or by a
    /// [`power_cycle`](Self::power_cycle).
    pub fn discarded_edits(&self) -> usize {
        self.discarded
    }

    /// The keyboard is unplugged and plugged in again: edits whose commit is
    /// not due yet are lost, nothing queued is delivered any more. The
    /// active profile and every saved setting stay.
    pub fn power_cycle(&mut self) {
        self.discard_unsaved();
        self.queue.clear();
        self.delayed.clear();
        self.scheduled.clear();
        self.leave_pending = false;
    }

    fn has_unsaved(&self) -> bool {
        !self.unsaved_tables.is_empty() || !self.unsaved_triggers.is_empty()
    }

    /// Whether the unsaved edits are due to be saved.
    fn commit_due(&self) -> bool {
        self.last_edit
            .is_some_and(|at| self.clock >= at.saturating_add(self.commit_delay))
    }

    /// Whether the unsaved edit `key` waits for an `18 02` write: its table
    /// waits under the [`KeymapCommit`] rule, and no complete `18 02` was
    /// applied since the edit (for advanced-key slots: none in the whole
    /// save window, before the edit included).
    fn waits(&self, key: (u8, SimTable)) -> bool {
        let covered_by_earlier_fn_write =
            key.1 == SimTable::AdvancedKeys && self.fn_written_in_window;
        self.keymap_commit.waits_for_fn_write(key.1)
            && !self.released.contains(&key)
            && !covered_by_earlier_fn_write
    }

    /// Saves the working copy if its commit is due, except the keymap-group
    /// edits that still wait for an `18 02` write ([`KeymapCommit`]).
    fn commit_if_due(&mut self) {
        if !self.commit_due() {
            return;
        }
        let waiting: Vec<(u8, SimTable)> = self
            .unsaved_tables
            .keys()
            .copied()
            .filter(|key| self.waits(*key))
            .collect();
        let kept: HashMap<(u8, SimTable), Vec<u8>> = waiting
            .into_iter()
            .filter_map(|key| self.unsaved_tables.remove_entry(&key))
            .collect();
        let saved = std::mem::replace(&mut self.unsaved_tables, kept);
        self.tables.extend(saved);
        // Every released edit was saved now, and the save window ends.
        self.released.clear();
        self.fn_written_in_window = false;
        self.triggers.extend(self.unsaved_triggers.drain());
        self.last_edit = None;
    }

    /// An edit was applied to the working copy of `key` (at `offset` into
    /// the table). A table that waits for an `18 02` write waits again, also
    /// when an earlier edit of it was released (an advanced-key slot does
    /// not when a complete `18 02` came earlier in the save window). An
    /// `18 02` chunk counts towards a complete `18 02` (every chunk applied
    /// since the last release or reload), which releases every waiting edit
    /// to the quiet-time commit: the commit delay runs from this chunk, the
    /// last edit. A partial `18 02` releases nothing.
    fn note_edit(&mut self, key: (u8, SimTable), offset: usize) {
        self.released.remove(&key);
        if key.1 != SimTable::Keymap(Layer::Fn) {
            return;
        }
        let chunks = SimTable::Keymap(Layer::Fn).len().div_ceil(KEYMAP_CHUNK_LEN);
        self.fn_chunks |= 1 << (offset / KEYMAP_CHUNK_LEN);
        if usize::try_from(self.fn_chunks.count_ones()).unwrap_or(0) == chunks {
            self.fn_chunks = 0;
            self.fn_written_in_window = true;
            let rule = self.keymap_commit;
            let waiting: Vec<(u8, SimTable)> = self
                .unsaved_tables
                .keys()
                .copied()
                .filter(|(_, table)| rule.waits_for_fn_write(*table))
                .collect();
            self.released.extend(waiting);
        }
    }

    /// A profile reload: saves the working copy if its commit is due, else
    /// discards it.
    fn discard_unsaved(&mut self) {
        self.commit_if_due();
        if self.has_unsaved() {
            self.unsaved_tables.clear();
            self.unsaved_triggers.clear();
            self.discarded += 1;
        }
        self.released.clear();
        self.fn_chunks = 0;
        self.fn_written_in_window = false;
        self.last_edit = None;
    }

    /// Puts every profile (and every global table) into a factory-like
    /// state: factory triggers (except the knob's) and light, zero keymaps,
    /// empty (`FF`) macros, zero advanced keys, dead band and colour table 1,
    /// SOCD off without pairs. Unsaved edits are dropped.
    pub fn factory_reset_all(&mut self) {
        self.unsaved_tables.clear();
        self.unsaved_triggers.clear();
        self.released.clear();
        self.fn_chunks = 0;
        self.fn_written_in_window = false;
        self.last_edit = None;
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
            self.tables
                .insert((profile, SimTable::SocdPairs), vec![0; 80]);
            self.tables.insert(
                (profile, SimTable::SocdSettings),
                vec![0, 0, 0, 0, 0xfa, 0xe5],
            );
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

    /// The user switches to `profile` on the keyboard `after` the current
    /// virtual time, for example while the host waits for the keyboard to
    /// save. At that moment the profile is reloaded: edits whose commit is
    /// not due by then are discarded (as of the switch time, even when a
    /// read jumps the clock past it). With `push` the keyboard pushes the
    /// new profile (`21 .. 03 11 PP 00`) at that time; without it the
    /// switch is silent (whether the keyboard reports a switch made on it is
    /// open question Q2 of the spec).
    pub fn switch_profile_at(&mut self, after: Duration, profile: ProfileId, push: bool) {
        let due = self.clock.saturating_add(after);
        self.faults.switch_at.push((due, profile.get(), push));
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

    // ----- keyboard events -----

    /// The user presses Fn + Right Shift `after` the current virtual time:
    /// the keyboard switches SOCD (power byte of `24 02`, kept) and pushes
    /// `24 02 00 00 06 PP 00 00 00 fa e5` with the new power (c15 #218,
    /// #231).
    pub fn press_socd_hotkey_at(&mut self, after: Duration) {
        let due = self.clock.saturating_add(after);
        self.socd_hotkey_at.push(due);
    }

    /// Whether the SOCD power write (`24 03`) is acknowledged with
    /// `24 00 00 00 00` (never seen on hardware; default off).
    pub fn set_socd_power_acked(&mut self, acked: bool) {
        self.socd_power_acked = acked;
    }

    /// Scripts the movement of the key at `pos` for the travel stream:
    /// `(offset from now, depth in trigger units)` points. A point at most
    /// 3 ms after the previous one that moves 3 units or more is a short
    /// report (fast motion), any other a long one (slow motion, c14 shapes);
    /// a final depth-0 report follows a script that does not end at 0.
    /// Reports due while the travel test is closed are not sent.
    pub fn travel_script(&mut self, pos: KeyPos, points: &[(Duration, u16)]) {
        let mut previous: Option<(Duration, u16)> = None;
        let add = |sim: &mut Self, at: Duration, depth: u16, previous: Option<(Duration, u16)>| {
            let (short, last) = match previous {
                Some((was_at, was)) => (
                    at.saturating_sub(was_at) <= Duration::from_millis(3)
                        && depth.abs_diff(was) >= 3,
                    was,
                ),
                None => (false, 0),
            };
            sim.travel_points.push(TravelPoint {
                due: sim.clock.saturating_add(at),
                pos,
                depth,
                short,
                previous: last,
            });
        };
        for &(at, depth) in points {
            add(self, at, depth, previous);
            previous = Some((at, depth));
        }
        if let Some((at, depth)) = previous.filter(|(_, depth)| *depth != 0) {
            add(self, at + Duration::from_millis(72), 0, Some((at, depth)));
        }
        self.travel_points.sort_by_key(|point| point.due);
    }

    /// Whether a profile select keeps an open travel test running (default
    /// true; untested on hardware, H13).
    pub fn set_travel_survives_select(&mut self, survives: bool) {
        self.travel_survives_select = survives;
    }

    /// Opens the travel test without a packet: a test an earlier run left
    /// open (the keyboard streams scripted movements from now on).
    pub fn leave_travel_open(&mut self) {
        self.travel_open = true;
        self.travel_left_open_allowed = true;
    }

    /// Accepts a travel test left open at the end ([`assert_clean`](Self::assert_clean)
    /// refuses one by default), for tests of a crashed run.
    pub fn allow_travel_left_open(&mut self) {
        self.travel_left_open_allowed = true;
    }

    /// Whether the travel test is open.
    pub fn travel_open(&self) -> bool {
        self.travel_open
    }

    /// How many travel-test opens (`21 .. 18 02`) were received.
    pub fn travel_opens(&self) -> usize {
        self.travel_opens
    }

    /// How many travel-test closes (`21 .. 18 03`) were received (a close
    /// while closed is acknowledged and counted).
    pub fn travel_closes(&self) -> usize {
        self.travel_closes
    }

    /// How many stream reports the simulator sent.
    pub fn travel_reports_sent(&self) -> usize {
        self.travel_reports_sent
    }

    /// The keymap entries of `profile` that refer to an empty macro or
    /// advanced-key slot: in the working copy, or (`persisted`) in what
    /// survives a profile switch made now.
    pub fn dangling_references(
        &self,
        profile: ProfileId,
        persisted: bool,
    ) -> Vec<(Layer, u8, SlotRef)> {
        let view = |table| {
            if persisted {
                self.persisted_table(profile, table)
            } else {
                self.table(profile, table)
            }
        };
        let empty = |bytes: Option<&[u8]>| {
            bytes.is_none_or(|b| b.iter().all(|x| *x == 0) || b.iter().all(|x| *x == 0xFF))
        };
        let mut out = Vec::new();
        for layer in Layer::ALL {
            let Some(bytes) = view(SimTable::Keymap(layer)) else {
                continue;
            };
            let Ok(raw) = RawResponse::from_reports(
                ReadRequest::Keymap(layer),
                frame(ReadRequest::Keymap(layer), bytes),
            ) else {
                continue;
            };
            let Ok((_, data)) = KeymapData::from_response(&raw) else {
                continue;
            };
            for reference in data.references() {
                match reference {
                    KeymapReference::Macro { index, slot } => {
                        if empty(view(SimTable::Macro(slot))) {
                            out.push((layer, index, SlotRef::Macro(slot)));
                        }
                    }
                    KeymapReference::Advanced { index, slot } => {
                        let start = usize::from(slot.get()) * 40;
                        let bytes =
                            view(SimTable::AdvancedKeys).and_then(|b| b.get(start..start + 40));
                        if empty(bytes) {
                            out.push((layer, index, SlotRef::Advanced(slot)));
                        }
                    }
                    KeymapReference::Malformed { .. } => {}
                }
            }
        }
        out
    }

    // ----- what happened -----

    /// Every packet the host sent, in order.
    pub fn written(&self) -> &[SimPacket] {
        &self.log
    }

    /// The virtual time ([`elapsed`](Self::elapsed)) at which each packet of
    /// [`written`](Self::written) was sent, in the same order.
    pub fn sent_times(&self) -> &[Duration] {
        &self.log_times
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
        assert!(
            !self.travel_open || self.travel_left_open_allowed,
            "the travel test was left open"
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
        self.apply_switches_until(self.clock);
        self.apply_keyboard_events_until(self.clock);
        self.commit_if_due();
        let bytes = *packet.as_bytes();
        self.log.push(packet);
        self.log_times.push(self.clock);
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
            // A switch on the keyboard reloads the profile too.
            self.discard_unsaved();
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
        let active = ProfileId::new(profile).unwrap_or(ProfileId::ALL[0]);
        if request == ReadRequest::ActiveProfile {
            self.queue
                .push_back(report(&[0x21, 0, 0, 0, 0x03, 0x11, profile, 0x01]));
            return;
        }
        // Reads of the active profile return its working copy.
        if let ReadRequest::KeyTrigger(pos) = request {
            if let Some(record) = self.trigger_view(active, pos, true) {
                let mut bytes = vec![0x21, 0, 0, 0, 0x0c, 0x05];
                bytes.extend_from_slice(&record);
                bytes.extend_from_slice(&[pos.row(), pos.col()]);
                self.queue.push_back(report(&bytes));
            }
            return;
        }
        let slot = self.slot(request, profile);
        if let Some(table) = SimTable::of_request(request) {
            if let Some(bytes) = self.table_view(active, table, true).map(<[u8]>::to_vec) {
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
                    (0x18, 0x02) => {
                        let mask = self.model.layout_mask();
                        if b[6..28] == mask.as_bytes()[..] && zero_from(28) {
                            Ok(Effect::TravelOpen)
                        } else {
                            Err("travel-test open with another mask or extra bytes".into())
                        }
                    }
                    (0x18, 0x03) => {
                        if zero_from(6) {
                            Ok(Effect::TravelClose)
                        } else {
                            Err("travel-test close with extra bytes".into())
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
            0x18 if b[1] == 0x00 => chunk(b, SimTable::Keymap(Layer::Base), KEYMAP_CHUNK_LEN),
            0x18 if b[1] == 0x02 => chunk(b, SimTable::Keymap(Layer::Fn), KEYMAP_CHUNK_LEN),
            0x18 => Err("keymap write sub other than 00 / 02".into()),
            0x19 => match MacroSlotId::new(b[1]) {
                Some(slot) => chunk(b, SimTable::Macro(slot), 58),
                None => Err("macro write to a slot outside 0-9".into()),
            },
            0x22 => {
                if b[1] >= 40 || b[2..5] != [0, 0, 0x32] {
                    return Err("advanced-key write with a bad slot or a bad header".into());
                }
                if zero_from(5) {
                    return Ok(Effect::AdvancedClear(usize::from(b[1])));
                }
                let mut slot = [0u8; 40];
                slot.copy_from_slice(&b[5..45]);
                advanced_slot_violation(&slot)
                    .map_or(Ok(()), Err)
                    .and_then(|()| {
                        if zero_from(45) {
                            Ok(Effect::AdvancedSlot(usize::from(b[1]), slot))
                        } else {
                            Err("advanced-key write with bytes after byte 44".into())
                        }
                    })
            }
            0x24 if b[1] == 0x03 => {
                if b[2..5] == [0, 0, 0x01] && b[5] <= 1 && zero_from(6) {
                    Ok(Effect::SocdPower(b[5]))
                } else {
                    Err("SOCD power write with a bad header, a value above 1 or extra bytes".into())
                }
            }
            0x24 if b[1] == 0x00 => {
                if b[2] != 0 || b[3] > 1 || b[4] != 0x28 || !zero_from(45) {
                    return Err("SOCD pair chunk with a bad index, length or extra bytes".into());
                }
                for entry in b[5..45].as_chunks::<4>().0 {
                    let valid = *entry == [0, 0, 0, 0]
                        || (entry[0] == 0x01
                            && entry[1] <= 3
                            && entry[2] != 0
                            && entry[3] != 0
                            && entry[2] != entry[3]);
                    if !valid {
                        return Err(format!("SOCD pair entry {entry:02x?} is not allowed"));
                    }
                }
                chunk(b, SimTable::SocdPairs, 40)
            }
            0x26 if b[1] == 0x01 => {
                let max = self.model.write_ceilings.max_dead_band;
                let effect = chunk(b, SimTable::DeadBand, 58)?;
                if let Effect::Chunk { data, .. } = &effect
                    && let Some(value) = data.iter().find(|v| u16::from(**v) > max)
                {
                    return Err(format!(
                        "dead-band value {value} is above the maximum {max}"
                    ));
                }
                Ok(effect)
            }
            other => Err(format!(
                "command {other:02x} with sub {:02x} is never written",
                b[1]
            )),
        }
    }

    /// The key of `table` of the active profile in `tables`.
    fn key_of(&self, table: SimTable) -> (u8, SimTable) {
        (self.slot(table.request(), self.active), table)
    }

    /// The working copy of `table` in the active profile, for a write: an
    /// unsaved edit starts from the saved bytes.
    fn table_mut(&mut self, table: SimTable) -> &mut Vec<u8> {
        let key = self.key_of(table);
        let saved = &self.tables;
        self.unsaved_tables.entry(key).or_insert_with(|| {
            saved
                .get(&key)
                .cloned()
                .unwrap_or_else(|| vec![0; table.len()])
        })
    }

    /// The firmware's side effect of a macro slot-0 write: slots 1-9 of the
    /// active profile become empty (256 x `FF`) in the working copy (see
    /// "Writing macro slot 0 clears slots 1-9" in the
    /// [module documentation](self)).
    fn clear_macros_after_slot0(&mut self) {
        for slot in MacroSlotId::all().filter(|slot| slot.get() != 0) {
            let table = SimTable::Macro(slot);
            self.table_mut(table).fill(0xFF);
            self.note_edit(self.key_of(table), 0);
        }
    }

    /// Applies a write; returns the reports the keyboard sends.
    fn apply(&mut self, effect: Effect, write: usize) -> Vec<Report> {
        let replies = self.apply_effect(effect, write);
        // With the delayed commit off, every write is saved at once.
        self.commit_if_due();
        replies
    }

    fn apply_effect(&mut self, effect: Effect, write: usize) -> Vec<Report> {
        let no_effect = self.faults.drop_effect.contains(&write);
        let blocked = |sim: &Self, table: SimTable| no_effect || sim.faults.frozen.contains(&table);
        if matches!(
            effect,
            Effect::Light(_)
                | Effect::Chunk { .. }
                | Effect::AdvancedClear(_)
                | Effect::AdvancedSlot(..)
                | Effect::Trigger { .. }
                | Effect::SocdPower(_)
        ) {
            // Every write that changes a setting restarts the commit delay.
            self.last_edit = Some(self.clock);
        }
        match effect {
            Effect::Select(profile) => {
                if self.faults.ignore_selects > 0 {
                    self.faults.ignore_selects -= 1;
                    return vec![report(&[0x21, 0, 0, 0, 0x02, 0x12, 0x01])];
                }
                if !no_effect {
                    // The profile is reloaded from its saved state.
                    self.discard_unsaved();
                    self.active = profile;
                    if !self.travel_survives_select {
                        self.travel_open = false;
                    }
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
                    if offset == 0 && matches!(table, SimTable::Macro(slot) if slot.get() == 0) {
                        self.clear_macros_after_slot0();
                    }
                    let bytes = self.table_mut(table);
                    if let Some(dst) = bytes.get_mut(offset..offset + data.len()) {
                        dst.copy_from_slice(&data);
                    }
                    // A complete Fn-keymap write releases the base keymap
                    // (spec 4.8.2) and the advanced-key slots (H20) to the
                    // quiet-time commit.
                    self.note_edit(self.key_of(table), offset);
                }
                let ack: &[u8] = match table {
                    SimTable::CustomColors1 => &[0x09, 0, 0, 0, 0],
                    SimTable::Keymap(_) => &[0x18, 0, 0, 0, 0],
                    SimTable::Macro(_) => &[0x19, 0, 0, 0, 0],
                    SimTable::DeadBand => &[0x26, 0x01, 0, 0, 0],
                    SimTable::SocdPairs => &[0x24, 0, 0, 0, 0],
                    SimTable::MainLight | SimTable::AdvancedKeys | SimTable::SocdSettings => &[],
                };
                vec![report(ack)]
            }
            Effect::AdvancedSlot(slot, data) => {
                if !blocked(self, SimTable::AdvancedKeys) {
                    let bytes = self.table_mut(SimTable::AdvancedKeys);
                    if let Some(dst) = bytes.get_mut(slot * 40..slot * 40 + 40) {
                        dst.copy_from_slice(&data);
                    }
                    self.note_edit(self.key_of(SimTable::AdvancedKeys), slot * 40);
                }
                vec![report(&[0x22, 0, 0, 0, 0])]
            }
            Effect::SocdPower(power) => {
                if !blocked(self, SimTable::SocdSettings) {
                    let bytes = self.table_mut(SimTable::SocdSettings);
                    if let Some(first) = bytes.first_mut() {
                        *first = power;
                    }
                    self.note_edit(self.key_of(SimTable::SocdSettings), 0);
                }
                if self.socd_power_acked {
                    vec![report(&[0x24, 0, 0, 0, 0])]
                } else {
                    Vec::new()
                }
            }
            Effect::TravelOpen => {
                self.travel_open = true;
                self.travel_opens += 1;
                vec![report(&[0x21, 0, 0, 0, 0x01, 0x00])]
            }
            Effect::TravelClose => {
                self.travel_open = false;
                self.travel_closes += 1;
                vec![report(&[0x21, 0, 0, 0, 0x01, 0x00])]
            }
            Effect::AdvancedClear(slot) => {
                if !blocked(self, SimTable::AdvancedKeys) {
                    let bytes = self.table_mut(SimTable::AdvancedKeys);
                    if let Some(dst) = bytes.get_mut(slot * 40..slot * 40 + 40) {
                        dst.fill(0);
                    }
                    self.note_edit(self.key_of(SimTable::AdvancedKeys), slot * 40);
                }
                vec![report(&[0x22, 0, 0, 0, 0])]
            }
            Effect::Trigger { keys, record } => {
                if !no_effect {
                    for pos in keys {
                        self.unsaved_triggers
                            .insert((self.active, pos.index()), record);
                    }
                }
                vec![report(&[0x21, 0, 0, 0, 0x01, 0x00])]
            }
        }
    }
}

/// Why 40 bytes of an advanced-key write are not a DKS, MT or TGL slot in
/// the site's form (independent of the protocol crate), if they are not.
fn advanced_slot_violation(slot: &[u8; 40]) -> Option<String> {
    let zero_from = |i: usize| slot[i..].iter().all(|x| *x == 0);
    let action_ok = |a: &[u8]| a[0] == 0 && a[2] == 0 && a[3] == 0;
    match slot[0] {
        1 => {
            let (r1, r2, r3, r4) = (slot[1], slot[2], slot[3], slot[4]);
            if r1 == 0 || r1 > r2 || r2 != r3 || r4 != r1 || r2 > 30 {
                return Some(format!("DKS routes {r1} {r2} {r3} {r4} are not allowed"));
            }
            for row in slot[5..37].as_chunks::<8>().0 {
                if !action_ok(&row[..4]) || row[4..].iter().any(|s| *s > 3) {
                    return Some("DKS row with a bad action or step".into());
                }
            }
            (!zero_from(37)).then(|| "DKS slot with bytes after byte 36".into())
        }
        2 => {
            if !action_ok(&slot[1..5]) || !action_ok(&slot[5..9]) {
                return Some("Mod-Tap with a bad action".into());
            }
            if !(15..=100).contains(&slot[9]) {
                return Some(format!("Mod-Tap threshold {} is not allowed", slot[9]));
            }
            (!zero_from(10)).then(|| "Mod-Tap slot with bytes after byte 9".into())
        }
        3 => {
            if !action_ok(&slot[1..5]) || slot[2] == 0 {
                return Some("Toggle with a bad action".into());
            }
            (!zero_from(5)).then(|| "Toggle slot with bytes after byte 4".into())
        }
        other => Some(format!("advanced-key type {other} is never written")),
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

/// A travel-stream report of `point` in the shapes of c14: short
/// `21 00 00 00 05 01 RR CC DL DH`, or long (#999) with the per-key
/// constants `9b 09`, `ad 09`, `87 06`, the motion byte and two sensor
/// readings that fall as the key goes down.
fn travel_report(point: &TravelPoint) -> Report {
    let [lo, hi] = point.depth.to_le_bytes();
    let (row, col) = (point.pos.row(), point.pos.col());
    if point.short {
        return report(&[0x21, 0, 0, 0, 0x05, 0x01, row, col, lo, hi]);
    }
    let motion = match point.depth.cmp(&point.previous) {
        _ if point.depth == 0 => 0x00,
        std::cmp::Ordering::Greater => 0x01,
        _ => 0x02,
    };
    let sensor = 2459u16
        .saturating_sub(point.depth.saturating_mul(4))
        .to_le_bytes();
    let mut bytes = vec![
        0x21, 0x00, 0x00, 0x00, 0x03, 0x01, row, col, lo, hi, 0xff, 0xff, 0x9b, 0x09, 0xad, 0x09,
        motion, 0x11, 0x00, sensor[0], sensor[1], sensor[0], sensor[1],
    ];
    bytes.resize(31, 0);
    bytes.extend_from_slice(&[0x87, 0x06, 0xff, 0xff, 0x00, 0x00]);
    report(&bytes)
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
pub(crate) fn frame(request: ReadRequest, bytes: &[u8]) -> Vec<Report> {
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
        self.apply_switches_until(self.clock);
        self.apply_keyboard_events_until(self.clock);
        self.release_scheduled(self.clock);
        let mut next = self.queue.pop_front();
        if next.is_none() {
            let deadline = self.clock.saturating_add(timeout);
            // The wait ends at the next report: a scheduled one, a keyboard
            // event, or the push of a switch on the keyboard.
            let report_due = self
                .scheduled
                .iter()
                .map(|(due, _)| *due)
                .chain(self.socd_hotkey_at.iter().copied())
                .chain(
                    self.travel_points
                        .iter()
                        .filter(|_| self.travel_open)
                        .map(|point| point.due),
                )
                .filter(|due| *due <= deadline)
                .min();
            let push_due = self
                .faults
                .switch_at
                .iter()
                .filter(|(due, _, push)| *push && *due <= deadline)
                .map(|(due, _, _)| *due)
                .min();
            let until = match (report_due, push_due) {
                (Some(a), Some(b)) => a.min(b),
                (a, b) => a.or(b).unwrap_or(deadline),
            };
            // Silent switches on the way happen at their own time.
            self.apply_switches_until(until);
            self.clock = self.clock.max(until);
            self.apply_keyboard_events_until(self.clock);
            self.release_scheduled(self.clock);
            next = self.queue.pop_front();
        }
        // Time passed: the working copy may be due to be saved.
        self.commit_if_due();
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
    /// Makes the switches on the keyboard due at or before `time` happen, in
    /// order, each at its own time (the clock is moved to it, never back):
    /// the profile is reloaded (the working copy is saved if its commit was
    /// due by then, else discarded), and a pushing switch queues its report.
    fn apply_switches_until(&mut self, time: Duration) {
        loop {
            let next = self
                .faults
                .switch_at
                .iter()
                .enumerate()
                .filter(|(_, (due, _, _))| *due <= time)
                .min_by_key(|(_, (due, _, _))| *due)
                .map(|(i, _)| i);
            let Some(index) = next else {
                break;
            };
            let (due, profile, push) = self.faults.switch_at.remove(index);
            self.clock = self.clock.max(due);
            self.discard_unsaved();
            self.active = profile;
            if push {
                self.queue
                    .push_back(report(&[0x21, 0, 0, 0, 0x03, 0x11, profile, 0x00]));
            }
        }
    }

    /// Makes the keyboard events due at or before `time` happen: Fn + Right
    /// Shift presses (SOCD switched and pushed) and travel-stream reports
    /// (sent only while the test is open; dropped otherwise).
    fn apply_keyboard_events_until(&mut self, time: Duration) {
        let presses: Vec<Duration> = self
            .socd_hotkey_at
            .iter()
            .copied()
            .filter(|due| *due <= time)
            .collect();
        self.socd_hotkey_at.retain(|due| *due > time);
        for _ in presses {
            let key = self.key_of(SimTable::SocdSettings);
            let mut settings = self
                .unsaved_tables
                .get(&key)
                .or_else(|| self.tables.get(&key))
                .cloned()
                .unwrap_or_else(|| vec![0, 0, 0, 0, 0xfa, 0xe5]);
            if let Some(power) = settings.first_mut() {
                *power = u8::from(*power == 0);
            }
            // A switch on the keyboard is kept at once.
            if let Some(unsaved) = self.unsaved_tables.get_mut(&key) {
                unsaved.clone_from(&settings);
            }
            self.tables.insert(key, settings.clone());
            let mut push = vec![0x24, 0x02, 0x00, 0x00, 0x06];
            push.extend_from_slice(&settings);
            self.queue.push_back(report(&push));
        }
        let due = self
            .travel_points
            .iter()
            .take_while(|point| point.due <= time)
            .count();
        let points: Vec<TravelPoint> = self.travel_points.drain(..due).collect();
        for point in points {
            if self.travel_open {
                self.queue.push_back(travel_report(&point));
                self.travel_reports_sent += 1;
            }
        }
    }

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
        DeviceSim::blank(&W75, ScopeConfig::default(), 1)
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
        {
            use openwraith_protocol::write::{ModTap, Toggle};
            use openwraith_protocol::{AdvancedKeyDef, AdvancedLimits, AdvancedSlotData, HidUsage};
            let global = TriggerGlobal::from_response(&raw(
                ReadRequest::TriggerGlobal,
                &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
            ))
            .expect("21/04");
            let limits = AdvancedLimits::new(&W75, &global, 20).expect("limits");
            let usage = |u| HidUsage::new(u).expect("usage");
            for def in [
                AdvancedKeyDef::Toggle(Toggle { key: usage(0x0b) }),
                AdvancedKeyDef::ModTap(ModTap {
                    hold: usage(0xe0),
                    tap: usage(0x0a),
                    threshold_10ms: 30,
                }),
            ] {
                let data = AdvancedSlotData::from_def(&def, &limits).expect("valid");
                out.push(WriteRequest::AdvancedKey(
                    AdvancedSlotId::new(7).expect("slot"),
                    data,
                ));
            }
            use openwraith_protocol::{SocdMode, SocdPair, SocdPairsData, SocdWrite};
            let pairs = SocdPairsData::from_pairs(
                &W75,
                &[SocdPair {
                    mode: SocdMode::M2,
                    key1: usage(0x04),
                    key2: usage(0x07),
                }],
            )
            .expect("pairs");
            out.push(WriteRequest::Socd(SocdWrite::new(true, pairs.clone())));
            out.push(WriteRequest::Socd(SocdWrite::new(false, pairs)));
        }
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
        use openwraith_protocol::{TravelTestMask, TravelTestRequest};
        for request in [
            TravelTestRequest::Open(TravelTestMask::full_layout(&W75).expect("mask")),
            TravelTestRequest::Close,
        ] {
            assert!(
                sim.parse_write(request.step().packet.as_bytes()).is_ok(),
                "{request}"
            );
        }
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
            ("socd pairs chunk 2", packet(&[0x24, 0x00, 0, 2, 0x28])),
            (
                "socd pair same key",
                packet(&[0x24, 0x00, 0, 0, 0x28, 1, 0, 4, 4]),
            ),
            (
                "socd pair mode 4",
                packet(&[0x24, 0x00, 0, 0, 0x28, 1, 4, 4, 7]),
            ),
            ("socd pairs read", packet(&[0x24, 0x01, 0, 0, 0x28])),
            ("socd settings read", packet(&[0x24, 0x02, 0, 0, 0x06])),
            ("socd power 2", packet(&[0x24, 0x03, 0, 0, 0x01, 0x02])),
            (
                "socd power extra",
                packet(&[0x24, 0x03, 0, 0, 0x01, 0x01, 0x01]),
            ),
            ("switch types", packet(&[0x25, 0x01, 0, 0, 0x3a])),
            ("reset triggers", packet(&[0x21, 0, 0, 0, 0x18, 0x06])),
            ("calibrate key", packet(&[0x21, 0, 0, 0, 0x18, 0x07, 2, 2])),
            ("calibrate all", packet(&[0x21, 0, 0, 0, 0x18, 0x08, 0x00])),
            ("any-key start", packet(&[0x21, 0, 0, 0, 0x18, 0x0f, 0x00])),
            ("any-key stop", packet(&[0x21, 0, 0, 0, 0x18, 0x10, 0x00])),
            ("travel open", packet(&[0x21, 0, 0, 0, 0x18, 0x02, 0x3f])),
            (
                "travel close extra",
                packet(&[0x21, 0, 0, 0, 0x18, 0x03, 0x01]),
            ),
            ("advanced type 4", packet(&[0x22, 0x00, 0, 0, 0x32, 0x04])),
            (
                "advanced mt threshold",
                packet(&[
                    0x22, 0x00, 0, 0, 0x32, 0x02, 0, 0xe0, 0, 0, 0, 0x0a, 0, 0, 0x0e,
                ]),
            ),
            (
                "advanced dks r1 > r2",
                packet(&[0x22, 0x00, 0, 0, 0x32, 0x01, 0x10, 0x0f, 0x0f, 0x10]),
            ),
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

    /// A single-key trigger write of Q (r2c1) at `actuation` units, RT off.
    fn q_trigger_packet(actuation: u16) -> WritePacket {
        let global = TriggerGlobal::from_response(&raw(
            ReadRequest::TriggerGlobal,
            &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
        ))
        .expect("21/04");
        let setting = TriggerSetting {
            mode: TriggerWriteMode::Off,
            actuation: Distance(actuation),
            rt_press: Distance(1),
            rt_release: Distance(1),
        };
        let q = KeyPos::new(2, 1).expect("Q");
        let write = TriggerWrite::new(
            &W75,
            KeyMask::from_positions([q]),
            setting,
            &TriggerLimits::new(&W75, &global),
        )
        .expect("valid");
        WriteRequest::Trigger(write).steps().remove(0).packet
    }

    fn select_packet(profile: ProfileId) -> WritePacket {
        WriteRequest::SelectProfile(profile)
            .steps()
            .remove(0)
            .packet
    }

    #[test]
    fn writes_are_saved_one_second_after_the_last_one() {
        let q = KeyPos::new(2, 1).expect("Q");
        let [p1, p2, ..] = ProfileId::ALL;
        let mut s = sim();
        s.set_trigger(p1, q, FACTORY_TRIGGER);
        s.set_trigger(p2, q, FACTORY_TRIGGER);
        assert_eq!(s.commit_delay(), DEFAULT_COMMIT_DELAY);
        let actuation = |s: &DeviceSim| s.trigger(p1, q).map(|r| r[1]);

        // A write is visible at once in the active profile (a read-back
        // matches), but it is not saved yet.
        s.write_command(&q_trigger_packet(100)).expect("sent");
        assert_eq!(actuation(&s), Some(100));
        assert_eq!(s.persisted_trigger(p1, q), Some(FACTORY_TRIGGER));
        assert!(s.has_unsaved_edits());
        s.queue.clear();
        s.write(&ReadRequest::KeyTrigger(q).packet()).expect("sent");
        assert_eq!(s.queue.pop_front().map(|r| r.as_bytes()[7]), Some(100));

        // A switch 800 ms later reloads the profile: the write is lost.
        assert!(s.read(Duration::from_millis(800)).expect("read").is_none());
        s.write_command(&select_packet(p2)).expect("sent");
        s.write_command(&select_packet(p1)).expect("sent");
        assert_eq!(s.trigger(p1, q), Some(FACTORY_TRIGGER));
        assert_eq!(s.discarded_edits(), 1);

        // Written again and left alone for 1 s: saved, it survives a switch.
        s.write_command(&q_trigger_packet(100)).expect("sent");
        s.queue.clear();
        assert!(s.read(Duration::from_secs(1)).expect("read").is_none());
        assert!(!s.has_unsaved_edits());
        assert_eq!(s.persisted_trigger(p1, q).map(|r| r[1]), Some(100));
        s.write_command(&select_packet(p2)).expect("sent");
        s.write_command(&select_packet(p1)).expect("sent");
        assert_eq!(actuation(&s), Some(100));
        assert_eq!(s.discarded_edits(), 1);

        // Unplugging right after a write loses it too.
        s.write_command(&q_trigger_packet(60)).expect("sent");
        s.power_cycle();
        assert_eq!(actuation(&s), Some(100));
        assert_eq!(s.discarded_edits(), 2);

        // Without the delayed commit every write is saved at once.
        s.set_commit_delay(Duration::ZERO);
        s.write_command(&q_trigger_packet(60)).expect("sent");
        assert!(!s.has_unsaved_edits());
        s.write_command(&select_packet(p2)).expect("sent");
        s.write_command(&select_packet(p1)).expect("sent");
        assert_eq!(actuation(&s), Some(60));
        assert_eq!(s.discarded_edits(), 2);
        // Every packet has its virtual time.
        assert_eq!(s.sent_times().len(), s.written().len());
        assert_eq!(s.sent_times()[1], Duration::ZERO);
        assert_eq!(s.sent_times()[2], Duration::from_millis(800));
        s.assert_clean();
    }

    #[test]
    fn a_switch_on_the_keyboard_at_a_time_reloads_the_profile_then() {
        let q = KeyPos::new(2, 1).expect("Q");
        let [p1, p2, ..] = ProfileId::ALL;
        let mut s = sim();
        s.set_trigger(p1, q, FACTORY_TRIGGER);
        s.set_trigger(p2, q, FACTORY_TRIGGER);

        // A silent switch 300 ms after a write: a read that waits 2 s jumps
        // past both the switch and the time the commit would have been due,
        // but the switch came first and dropped the write.
        s.write_command(&q_trigger_packet(100)).expect("sent");
        s.queue.clear();
        s.switch_profile_at(Duration::from_millis(300), p2, false);
        assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
        assert_eq!(s.elapsed(), Duration::from_secs(2));
        assert_eq!(s.active_profile(), 2);
        assert_eq!(s.discarded_edits(), 1);
        assert_eq!(s.persisted_trigger(p1, q), Some(FACTORY_TRIGGER));

        // With a push, a read gets the profile report at the switch time.
        s.switch_profile_at(Duration::from_millis(500), p1, true);
        let got = s.read(Duration::from_secs(2)).expect("read");
        assert_eq!(
            got.map(|r| r.as_bytes()[..8].to_vec()),
            Some(vec![0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x00])
        );
        assert_eq!(s.elapsed(), Duration::from_millis(2500));
        assert_eq!(s.active_profile(), 1);

        // A switch after the commit was due keeps the write.
        s.write_command(&q_trigger_packet(60)).expect("sent");
        s.queue.clear();
        s.switch_profile_at(Duration::from_millis(1500), p2, false);
        assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
        assert_eq!(s.active_profile(), 2);
        assert_eq!(s.discarded_edits(), 1);
        assert_eq!(s.persisted_trigger(p1, q).map(|r| r[1]), Some(60));

        // set_active_profile is such a switch, made at once.
        s.write_command(&q_trigger_packet(70)).expect("sent");
        s.set_active_profile(p1);
        assert_eq!(s.discarded_edits(), 2);
        assert_eq!(s.persisted_trigger(p2, q), Some(FACTORY_TRIGGER));
        s.assert_clean();
    }

    /// The ten packets of a keymap write of `layer` with `bytes`.
    fn keymap_packets(layer: Layer, bytes: &[u8]) -> Vec<WritePacket> {
        let (layer, data) =
            KeymapData::from_response(&raw(ReadRequest::Keymap(layer), bytes)).expect("keymap");
        WriteRequest::Keymap(layer, data)
            .steps()
            .into_iter()
            .map(|step| step.packet)
            .collect()
    }

    /// The five packets of a macro write of `slot` with `bytes`.
    fn macro_packets(slot: MacroSlotId, bytes: &[u8]) -> Vec<WritePacket> {
        let (slot, data) =
            MacroData::from_response(&raw(ReadRequest::Macro(slot), bytes)).expect("macro");
        WriteRequest::Macro(slot, data)
            .steps()
            .into_iter()
            .map(|step| step.packet)
            .collect()
    }

    fn send_all(s: &mut DeviceSim, packets: &[WritePacket]) {
        for packet in packets {
            s.write_command(packet).expect("sent");
        }
        s.queue.clear();
    }

    /// Selects `away`, then `back` (a profile reload).
    fn reload(s: &mut DeviceSim, away: ProfileId, back: ProfileId) {
        s.write_command(&select_packet(away)).expect("sent");
        s.write_command(&select_packet(back)).expect("sent");
        s.queue.clear();
    }

    #[test]
    fn the_base_keymap_is_saved_only_by_a_following_fn_write() {
        let [p1, p2, ..] = ProfileId::ALL;
        let base = SimTable::Keymap(Layer::Base);
        let fn_layer = SimTable::Keymap(Layer::Fn);
        let zeros = vec![0u8; 528];
        let mut s = sim();
        assert_eq!(s.keymap_commit(), KeymapCommit::default());
        assert!(s.keymap_commit().base_needs_fn_write);
        for p in [p1, p2] {
            s.set_table(p, base, zeros.clone());
            s.set_table(p, fn_layer, zeros.clone());
        }
        // W -> A and knob slot 16 -> 00, as in the hardware probe.
        let mut remapped = zeros.clone();
        remapped[46 * 4..47 * 4].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);

        // 18 00 alone: it reads back at once, but even 6 s later it is not
        // saved, and a profile switch loses it.
        send_all(&mut s, &keymap_packets(Layer::Base, &remapped));
        assert_eq!(s.table(p1, base), Some(remapped.as_slice()));
        assert!(s.read(Duration::from_secs(6)).expect("read").is_none());
        assert!(s.has_unsaved_edits());
        assert!(s.has_edits_waiting_for_fn_write());
        assert_eq!(s.persisted_table(p1, base), Some(zeros.as_slice()));
        reload(&mut s, p2, p1);
        assert_eq!(s.table(p1, base), Some(zeros.as_slice()));
        assert_eq!(s.discarded_edits(), 1);

        // 18 00, then 18 02 with unchanged content: the complete 18 02
        // releases the base keymap to the quiet-time commit, which saves both
        // layers 1 s after the last 18 02 chunk (not at once).
        send_all(&mut s, &keymap_packets(Layer::Base, &remapped));
        send_all(&mut s, &keymap_packets(Layer::Fn, &zeros));
        assert!(!s.has_edits_waiting_for_fn_write());
        assert!(s.has_unsaved_edits());
        assert_eq!(s.persisted_table(p1, base), Some(zeros.as_slice()));
        assert!(s.read(Duration::from_secs(1)).expect("read").is_none());
        assert!(!s.has_unsaved_edits());
        assert_eq!(s.persisted_table(p1, base), Some(remapped.as_slice()));
        reload(&mut s, p2, p1);
        assert_eq!(s.table(p1, base), Some(remapped.as_slice()));
        assert_eq!(s.discarded_edits(), 1);

        // 18 02 alone is kept (index 130, unused, as in the probe).
        let mut fn_table = zeros.clone();
        fn_table[130 * 4..131 * 4].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
        send_all(&mut s, &keymap_packets(Layer::Fn, &fn_table));
        assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
        reload(&mut s, p2, p1);
        assert_eq!(s.table(p1, fn_layer), Some(fn_table.as_slice()));

        // Without the rule the quiet-time commit saves the base keymap.
        s.set_keymap_commit(KeymapCommit {
            base_needs_fn_write: false,
            ..KeymapCommit::default()
        });
        send_all(&mut s, &keymap_packets(Layer::Base, &zeros));
        assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
        reload(&mut s, p2, p1);
        assert_eq!(s.table(p1, base), Some(zeros.as_slice()));
        assert_eq!(s.discarded_edits(), 1);
        s.assert_clean();
    }

    /// A complete `18 02` only releases the waiting base keymap to the
    /// quiet-time commit: it is not saved at once, a partial `18 02`
    /// releases nothing, and the Fn keymap itself keeps the quiet-time
    /// commit.
    #[test]
    fn a_complete_fn_write_releases_the_base_keymap_to_the_quiet_time_commit() {
        let [p1, p2, ..] = ProfileId::ALL;
        let base = SimTable::Keymap(Layer::Base);
        let fn_layer = SimTable::Keymap(Layer::Fn);
        let zeros = vec![0u8; 528];
        let mut remapped = zeros.clone();
        remapped[46 * 4..47 * 4].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
        let mut fn_table = zeros.clone();
        fn_table[130 * 4..131 * 4].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
        let fresh = || {
            let mut s = sim();
            for p in [p1, p2] {
                s.set_table(p, base, zeros.clone());
                s.set_table(p, fn_layer, zeros.clone());
            }
            s
        };

        // A switch 300 ms after a complete base + Fn write loses both layers.
        let mut s = fresh();
        send_all(&mut s, &keymap_packets(Layer::Base, &remapped));
        send_all(&mut s, &keymap_packets(Layer::Fn, &fn_table));
        assert!(!s.has_edits_waiting_for_fn_write());
        assert!(s.read(Duration::from_millis(300)).expect("read").is_none());
        assert!(s.has_unsaved_edits());
        reload(&mut s, p2, p1);
        assert_eq!(s.table(p1, base), Some(zeros.as_slice()));
        assert_eq!(s.table(p1, fn_layer), Some(zeros.as_slice()));
        assert_eq!(s.discarded_edits(), 1);
        s.assert_clean();

        // A truncated 18 02 (chunks 0-8 of an unchanged Fn keymap) releases
        // nothing: even 6 s later the base keymap is not saved.
        let mut s = fresh();
        let unchanged_fn = keymap_packets(Layer::Fn, &zeros);
        send_all(&mut s, &keymap_packets(Layer::Base, &remapped));
        send_all(&mut s, &unchanged_fn[..9]);
        assert!(s.has_edits_waiting_for_fn_write());
        assert!(s.read(Duration::from_secs(6)).expect("read").is_none());
        assert_eq!(s.persisted_table(p1, base), Some(zeros.as_slice()));
        reload(&mut s, p2, p1);
        assert_eq!(s.table(p1, base), Some(zeros.as_slice()));
        assert_eq!(s.discarded_edits(), 1);

        // The writer's second attempt after an ack timeout: chunk 0, then
        // the whole table. Complete, so the base keymap is saved 1 s later.
        send_all(&mut s, &keymap_packets(Layer::Base, &remapped));
        send_all(&mut s, &unchanged_fn[..1]);
        assert!(s.has_edits_waiting_for_fn_write());
        send_all(&mut s, &unchanged_fn);
        assert!(!s.has_edits_waiting_for_fn_write());
        assert!(s.read(Duration::from_secs(1)).expect("read").is_none());
        reload(&mut s, p2, p1);
        assert_eq!(s.table(p1, base), Some(remapped.as_slice()));
        assert_eq!(s.discarded_edits(), 1);

        // A base keymap written after the 18 02 waits for the next one.
        send_all(&mut s, &keymap_packets(Layer::Fn, &zeros));
        send_all(&mut s, &keymap_packets(Layer::Base, &zeros));
        assert!(s.has_edits_waiting_for_fn_write());
        assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
        reload(&mut s, p2, p1);
        assert_eq!(s.table(p1, base), Some(remapped.as_slice()));
        assert_eq!(s.discarded_edits(), 2);
        s.assert_clean();

        // An Fn-keymap write followed at once by a select is discarded.
        let mut s = fresh();
        send_all(&mut s, &keymap_packets(Layer::Fn, &fn_table));
        assert_eq!(s.table(p1, fn_layer), Some(fn_table.as_slice()));
        reload(&mut s, p2, p1);
        assert_eq!(s.table(p1, fn_layer), Some(zeros.as_slice()));
        assert_eq!(s.discarded_edits(), 1);
        s.assert_clean();
    }

    #[test]
    fn macros_wait_for_the_fn_write_only_when_configured() {
        let [p1, p2, ..] = ProfileId::ALL;
        let slot = MacroSlotId::new(0).expect("slot");
        let table = SimTable::Macro(slot);
        let zeros = vec![0u8; 528];
        let mut content = vec![0u8; 256];
        content[..12].copy_from_slice(&[0, 0, 0, 4, 0, 1, 0, 0, 0x04, 0x10, 0, 0]);
        for waits in [false, true] {
            let mut s = sim();
            s.set_keymap_commit(KeymapCommit {
                macros_need_fn_write: waits,
                ..KeymapCommit::default()
            });
            for p in [p1, p2] {
                s.set_table(p, table, vec![0xFF; 256]);
                s.set_table(p, SimTable::Keymap(Layer::Fn), zeros.clone());
            }
            // A macro write alone, left alone for 2 s.
            send_all(&mut s, &macro_packets(slot, &content));
            assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
            assert_eq!(s.has_edits_waiting_for_fn_write(), waits);
            reload(&mut s, p2, p1);
            let kept = s.table(p1, table) == Some(content.as_slice());
            assert_eq!(kept, !waits, "waits: {waits}");

            // Followed by an 18 02: kept in both settings.
            send_all(&mut s, &macro_packets(slot, &content));
            send_all(&mut s, &keymap_packets(Layer::Fn, &zeros));
            assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
            reload(&mut s, p2, p1);
            assert_eq!(s.table(p1, table), Some(content.as_slice()), "{waits}");
            s.assert_clean();
        }
    }

    /// The packet of an advanced-key write of a Toggle of A into `slot`.
    fn toggle_packet(slot: u8) -> WritePacket {
        use openwraith_protocol::write::Toggle;
        use openwraith_protocol::{
            AdvancedKeyDef, AdvancedLimits, AdvancedSlotData, AdvancedSlotId, HidUsage,
        };
        let global = TriggerGlobal::from_response(&raw(
            ReadRequest::TriggerGlobal,
            &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
        ))
        .expect("21/04");
        let limits = AdvancedLimits::new(&W75, &global, 20).expect("limits");
        let def = AdvancedKeyDef::Toggle(Toggle {
            key: HidUsage::new(0x04).expect("usage"),
        });
        let data = AdvancedSlotData::from_def(&def, &limits).expect("valid");
        WriteRequest::AdvancedKey(AdvancedSlotId::new(slot).expect("slot"), data)
            .steps()
            .remove(0)
            .packet
    }

    /// H20 step 2 (2026-10-27, `hw/v02/h20-2.txt`, `h20-2b.txt`): a lone
    /// `22` write reads back but is gone after a profile switch; an `18 02`
    /// before it in the same save window keeps it (CONFIRMED), one after it
    /// too (GUESS); an `18 02` saved before the slot was written does not.
    #[test]
    fn an_advanced_slot_is_kept_only_with_an_fn_write_in_the_same_save_window() {
        let [p1, p2, ..] = ProfileId::ALL;
        let zeros = vec![0u8; 528];
        let empty_slots = vec![0u8; 1600];
        let fresh = || {
            let mut s = sim();
            assert!(s.keymap_commit().advanced_need_fn_write);
            for p in [p1, p2] {
                s.set_table(p, SimTable::AdvancedKeys, empty_slots.clone());
                s.set_table(p, SimTable::Keymap(Layer::Fn), zeros.clone());
            }
            s
        };
        let slot4 = |s: &DeviceSim| {
            s.table(p1, SimTable::AdvancedKeys)
                .map(|b| b[4 * 40..5 * 40].iter().any(|x| *x != 0))
        };

        // 22 alone: verified in the working copy, lost after the switch.
        let mut s = fresh();
        send_all(&mut s, &[toggle_packet(4)]);
        assert_eq!(slot4(&s), Some(true));
        assert!(s.read(Duration::from_secs(3)).expect("read").is_none());
        assert!(s.has_edits_waiting_for_fn_write());
        reload(&mut s, p2, p1);
        assert_eq!(slot4(&s), Some(false));
        assert_eq!(s.discarded_edits(), 1);
        s.assert_clean();

        // 18 02, then 22 (the site's order): kept.
        let mut s = fresh();
        send_all(&mut s, &keymap_packets(Layer::Fn, &zeros));
        send_all(&mut s, &[toggle_packet(4)]);
        assert!(!s.has_edits_waiting_for_fn_write());
        assert!(s.read(Duration::from_secs(1)).expect("read").is_none());
        reload(&mut s, p2, p1);
        assert_eq!(slot4(&s), Some(true));
        assert_eq!(s.discarded_edits(), 0);

        // 22, then 18 02: released, kept (GUESS).
        let mut s = fresh();
        send_all(&mut s, &[toggle_packet(4)]);
        send_all(&mut s, &keymap_packets(Layer::Fn, &zeros));
        assert!(s.read(Duration::from_secs(1)).expect("read").is_none());
        reload(&mut s, p2, p1);
        assert_eq!(slot4(&s), Some(true));

        // An 18 02 the keyboard already saved is another window: lost.
        let mut s = fresh();
        send_all(&mut s, &keymap_packets(Layer::Fn, &zeros));
        assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
        send_all(&mut s, &[toggle_packet(4)]);
        assert!(s.has_edits_waiting_for_fn_write());
        assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
        reload(&mut s, p2, p1);
        assert_eq!(slot4(&s), Some(false));

        // Without the rule a lone 22 is kept.
        let mut s = fresh();
        s.set_keymap_commit(KeymapCommit {
            advanced_need_fn_write: false,
            ..KeymapCommit::default()
        });
        send_all(&mut s, &[toggle_packet(4)]);
        assert!(s.read(Duration::from_secs(1)).expect("read").is_none());
        reload(&mut s, p2, p1);
        assert_eq!(slot4(&s), Some(true));
        s.assert_clean();
    }

    /// H21 (2026-10-27): the SOCD power is one setting of the whole
    /// keyboard, the pairs belong to each profile, and Fn + Right Shift
    /// switches the power of every profile.
    #[test]
    fn the_socd_power_is_global_and_the_pairs_per_profile() {
        use openwraith_protocol::{HidUsage, SocdMode, SocdPair, SocdPairsData, SocdWrite};
        let [p1, p2, p3, ..] = ProfileId::ALL;
        let mut s = sim();
        s.factory_reset_all();
        let pairs = SocdPairsData::from_pairs(
            &W75,
            &[SocdPair {
                mode: SocdMode::M1,
                key1: HidUsage::new(0x04).expect("A"),
                key2: HidUsage::new(0x07).expect("D"),
            }],
        )
        .expect("pairs");
        let burst: Vec<WritePacket> = WriteRequest::Socd(SocdWrite::new(true, pairs))
            .steps()
            .into_iter()
            .map(|step| step.packet)
            .collect();
        s.write_command(&select_packet(p2)).expect("sent");
        send_all(&mut s, &burst);
        assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
        reload(&mut s, p1, p2);
        let power = |s: &DeviceSim, p| s.table(p, SimTable::SocdSettings).map(|b| b[0]);
        let first_pair = |s: &DeviceSim, p| {
            s.table(p, SimTable::SocdPairs)
                .map(|b| b[..4].to_vec())
                .expect("pairs")
        };
        for p in ProfileId::ALL {
            assert_eq!(power(&s, p), Some(1), "profile {p}");
        }
        assert_eq!(first_pair(&s, p2), [0x01, 0x00, 0x04, 0x07]);
        for p in [p1, p3] {
            assert_eq!(first_pair(&s, p), [0, 0, 0, 0], "profile {p}");
        }
        // Fn + Right Shift on profile 2: off everywhere, the pairs stay.
        s.press_socd_hotkey_at(Duration::from_millis(10));
        assert!(s.read(Duration::from_millis(50)).expect("push").is_some());
        for p in ProfileId::ALL {
            assert_eq!(power(&s, p), Some(0), "profile {p}");
        }
        assert_eq!(first_pair(&s, p2), [0x01, 0x00, 0x04, 0x07]);
        s.assert_clean();
    }

    /// The experiment of `hw/v02/macro-exp.txt` (2026-10-27): a slot-0
    /// write (empty or not) empties slots 1-9 of the active profile only;
    /// a write to another slot touches only that slot.
    #[test]
    fn a_macro_slot_0_write_clears_slots_1_to_9_of_the_active_profile() {
        let [p1, p2, ..] = ProfileId::ALL;
        let slot = |n: u8| MacroSlotId::new(n).expect("slot");
        let content = |n: u8| {
            let mut bytes = vec![0u8; 256];
            bytes[..12].copy_from_slice(&[n, 0, 0, 4, 0, 1, 0, 0, 0x04 + n, 0x10, 0, 0]);
            bytes
        };
        let mut s = sim();
        for p in [p1, p2] {
            for n in 0..10 {
                s.set_table(p, SimTable::Macro(slot(n)), content(n));
            }
        }
        // Slot 2 alone: only slot 2 changes.
        send_all(&mut s, &macro_packets(slot(2), &[0u8; 256]));
        assert_eq!(s.table(p1, SimTable::Macro(slot(2))), Some(&[0u8; 256][..]));
        for n in [0, 1, 3, 9] {
            assert_eq!(s.table(p1, SimTable::Macro(slot(n))), Some(&content(n)[..]));
        }
        // Slot 0 with content (step 4 of the experiment): slots 1-9 read
        // all FF after its first chunk already.
        let packets = macro_packets(slot(0), &content(0));
        send_all(&mut s, &packets[..1]);
        for n in 1..10 {
            assert_eq!(
                s.table(p1, SimTable::Macro(slot(n))),
                Some(&[0xFF; 256][..]),
                "slot {n}"
            );
        }
        send_all(&mut s, &packets[1..]);
        assert_eq!(s.table(p1, SimTable::Macro(slot(0))), Some(&content(0)[..]));
        // Saved like any other macro edit; profile 2 keeps its slots.
        assert!(s.read(Duration::from_secs(2)).expect("read").is_none());
        reload(&mut s, p2, p1);
        assert_eq!(
            s.persisted_table(p1, SimTable::Macro(slot(5))),
            Some(&[0xFF; 256][..])
        );
        for n in 0..10 {
            assert_eq!(s.table(p2, SimTable::Macro(slot(n))), Some(&content(n)[..]));
        }
        // A clear of slot 0 (step 3) empties them too; a switch before the
        // commit brings them back.
        s.set_table(p1, SimTable::Macro(slot(4)), content(4));
        send_all(&mut s, &macro_packets(slot(0), &[0u8; 256]));
        assert_eq!(
            s.table(p1, SimTable::Macro(slot(4))),
            Some(&[0xFF; 256][..])
        );
        reload(&mut s, p2, p1);
        assert_eq!(s.table(p1, SimTable::Macro(slot(4))), Some(&content(4)[..]));
        assert!(s.discarded_edits() >= 1);
        s.assert_clean();
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
