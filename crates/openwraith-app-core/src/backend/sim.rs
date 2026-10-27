//! The developer simulator backend: the library's `DeviceSim` (delayed
//! commit, keymap commit rule, virtual clock) behind a transport that can be
//! unplugged, reset and paced in real time. Only in tests and debug builds
//! with the `sim` feature; never in a release build.
//!
//! Control actions ([`SimAction`]) change the shared simulator state at
//! once; transports take the state's lock for each packet, so an action
//! lands between two packets, also in the middle of an operation.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use openwraith_hid::protocol::capture::Capture;
use openwraith_hid::protocol::{OutPacket, ProfileId, REPORT_LEN, Report, W75, WritePacket};
use openwraith_hid::{
    Backup, BackupOptions, DeviceListing, DeviceSim, MockTransport, Session, SimTable, ToolInfo,
    Transport, TransportError, UsbInfo, WriteTransport, read_all_with,
};

use super::{DeviceBackend, DeviceEntry};
use crate::clock::Clock;
use crate::dto::{AppError, AppErrorKind, SimAction, SimInfo};
use crate::tokens::TokenMint;

/// The capture the simulator is seeded from: c06 (W = 1.00 mm with Rapid
/// Trigger 0.30 mm, main light Custom red).
const SEED_CAPTURE: &str =
    include_str!("../../../../captures/capture_06_powercycle_W_persisted.json");

/// How simulated time passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pacing {
    /// Waits take real time (the dev app).
    RealTime,
    /// Waits take no time (tests, fixtures).
    Virtual,
}

/// Options of the simulator backend.
#[derive(Debug, Clone, PartialEq)]
pub struct SimOptions {
    /// How time passes.
    pub pacing: Pacing,
    /// How many simulated keyboards are connected (0, 1 or 2).
    pub devices: u8,
    /// Real-time pacing: real time per simulated time (1.0 = as on the
    /// keyboard; `OPENWRAITH_SIM_SPEED`).
    pub speed: f64,
}

impl SimOptions {
    /// One keyboard in real time, at the speed given by
    /// `OPENWRAITH_SIM_SPEED` (default 1.0).
    pub fn realtime_from_env() -> Self {
        let speed = std::env::var("OPENWRAITH_SIM_SPEED")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|s| s.is_finite())
            .map_or(1.0, |s| s.clamp(0.05, 100.0));
        SimOptions {
            pacing: Pacing::RealTime,
            devices: 1,
            speed,
        }
    }

    /// Virtual time (tests).
    pub fn virtual_time(devices: u8) -> Self {
        SimOptions {
            pacing: Pacing::Virtual,
            devices,
            speed: 0.0,
        }
    }
}

/// The state of one simulated keyboard.
pub(crate) struct SimState {
    /// The simulated keyboard.
    pub(crate) sim: DeviceSim,
    unplugged: bool,
    unplug_after: Option<u16>,
    reset_pending: bool,
    open_failures: u32,
    post_reset_packets: u32,
    heartbeat: Report,
    paused: bool,
    blocked: bool,
}

impl SimState {
    fn unplug(&mut self) {
        self.unplugged = true;
        // The power is gone at once: edits not saved yet are lost.
        self.sim.power_cycle();
        self.unplug_after = None;
    }

    /// Counts a packet towards "unplug after N packets".
    fn count_packet(&mut self) {
        if let Some(left) = self.unplug_after {
            let left = left.saturating_sub(1);
            if left == 0 {
                self.unplug();
            } else {
                self.unplug_after = Some(left);
            }
        }
    }

    /// Lets `duration` of virtual time pass, dropping whatever arrives (no
    /// program has the keyboard open).
    fn advance(&mut self, duration: Duration) {
        let target = self.sim.elapsed().saturating_add(duration);
        for _ in 0..10_000 {
            let now = self.sim.elapsed();
            if now >= target {
                break;
            }
            if Transport::read(&mut self.sim, target - now).is_err() {
                break;
            }
        }
    }
}

/// One simulated keyboard, shared by its transports and the controls.
pub(crate) struct SimCell {
    state: Mutex<SimState>,
    resume: Condvar,
}

impl SimCell {
    fn lock(&self) -> Result<MutexGuard<'_, SimState>, TransportError> {
        self.state
            .lock()
            .map_err(|_| TransportError::Read("the simulator state is poisoned".to_string()))
    }

    /// The lock even after a panic elsewhere (for infallible reads).
    fn lock_any(&self) -> MutexGuard<'_, SimState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A connected simulated keyboard.
pub(crate) struct SimDevice {
    path: String,
    serial: String,
    cell: Arc<SimCell>,
}

/// The reset notification the keyboard sends after Fn + Space.
fn reset_report() -> Report {
    let mut bytes = [0u8; REPORT_LEN];
    bytes[..6].copy_from_slice(&[0x14, 0, 0, 0, 0x01, 0x01]);
    Report::new(bytes)
}

/// The seed backup (c06) and the captured status report, built once.
fn seed() -> Result<&'static (Backup, Report), AppError> {
    static SEED: OnceLock<Result<(Backup, Report), String>> = OnceLock::new();
    SEED.get_or_init(|| {
        let capture = Capture::from_json(SEED_CAPTURE).map_err(|e| e.to_string())?;
        let heartbeat = capture
            .in_reports()
            .find(|(_, r)| r.cmd() == 0x01)
            .map(|(_, r)| r.clone())
            .ok_or_else(|| "the seed capture has no status report".to_string())?;
        let options = BackupOptions {
            tool: ToolInfo {
                name: "openwraith-sim-seed".to_string(),
                version: "0".to_string(),
            },
            created_at: Some(std::time::UNIX_EPOCH + Duration::from_secs(1_792_827_000)),
        };
        let mut session = Session::new(MockTransport::from_capture(&capture));
        let backup = read_all_with(&mut session, &options, |_| {}).map_err(|e| e.to_string())?;
        Ok((backup, heartbeat))
    })
    .as_ref()
    .map_err(|e| AppError::internal(format!("cannot seed the simulator: {e}")))
}

/// The seed backup (c06).
#[cfg(test)]
pub(crate) fn seed_backup() -> Backup {
    seed()
        .map(|(b, _)| b.clone())
        .unwrap_or_else(|e| panic!("{e}"))
}

/// A simulator seeded from c06; profiles 2-4 have factory triggers and the
/// factory light, so that the profiles differ like on the real keyboard.
fn seeded_sim(serial: &str) -> Result<(DeviceSim, Report), AppError> {
    let (backup, heartbeat) = seed()?;
    Ok((sim_from(backup, serial), heartbeat.clone()))
}

/// A simulator seeded from `backup` (see [`seeded_sim`]).
pub(crate) fn sim_from(backup: &Backup, serial: &str) -> DeviceSim {
    let mut sim = DeviceSim::from_backup(backup);
    let mut factory = DeviceSim::from_backup(backup);
    factory.factory_reset_all();
    for profile in ProfileId::ALL.into_iter().skip(1) {
        for key in W75.positions() {
            if let Some(record) = factory.trigger(profile, key) {
                sim.set_trigger(profile, key, record);
            }
        }
        sim.set_table(
            profile,
            SimTable::MainLight,
            openwraith_hid::sim::FACTORY_LIGHT.to_vec(),
        );
    }
    sim.set_usb_info(Some(UsbInfo {
        vendor_id: 0x2E3C,
        product_id: 0xC368,
        product: Some("Wraith W75 (simulated)".to_string()),
        manufacturer: None,
        serial: Some(serial.to_string()),
    }));
    sim
}

/// The simulated keyboards (shared by the backend and the controls).
pub(crate) fn make_devices(count: u8) -> Result<Arc<Vec<SimDevice>>, AppError> {
    let mut devices = Vec::new();
    for n in 1..=count.min(2) {
        let serial = format!("SIM-{n}");
        let (sim, heartbeat) = seeded_sim(&serial)?;
        devices.push(SimDevice {
            path: format!("sim://keyboard-{n}"),
            serial,
            cell: Arc::new(SimCell {
                state: Mutex::new(SimState {
                    sim,
                    unplugged: false,
                    unplug_after: None,
                    reset_pending: false,
                    open_failures: 0,
                    post_reset_packets: 0,
                    heartbeat,
                    paused: false,
                    blocked: false,
                }),
                resume: Condvar::new(),
            }),
        });
    }
    Ok(Arc::new(devices))
}

/// Paces a transport call: in real time, sleeps for the simulated time it
/// took.
fn pace(pacing: Pacing, speed: f64, spent: Duration) {
    if pacing == Pacing::RealTime && speed > 0.0 && !spent.is_zero() {
        std::thread::sleep(spent.mul_f64(speed));
    }
}

/// The transport of an opened simulated keyboard.
pub(crate) struct RealtimeSim {
    cell: Arc<SimCell>,
    pacing: Pacing,
    speed: f64,
    /// This connection delivered a reset notification: any later packet is
    /// a violation (counted in [`SimInfo::violations`]).
    reset_seen: bool,
}

impl RealtimeSim {
    fn send(
        &mut self,
        f: impl FnOnce(&mut DeviceSim) -> Result<(), TransportError>,
        write: bool,
    ) -> Result<(), TransportError> {
        let mut state = self.cell.lock()?;
        if write {
            while state.paused {
                state.blocked = true;
                state =
                    self.cell.resume.wait(state).map_err(|_| {
                        TransportError::Write("the simulator state is poisoned".into())
                    })?;
            }
            state.blocked = false;
        }
        if state.unplugged {
            return Err(TransportError::Write(
                "the simulated keyboard is unplugged".to_string(),
            ));
        }
        if self.reset_seen {
            state.post_reset_packets = state.post_reset_packets.saturating_add(1);
        }
        let before = state.sim.elapsed();
        f(&mut state.sim)?;
        state.count_packet();
        let spent = state.sim.elapsed().saturating_sub(before);
        drop(state);
        pace(self.pacing, self.speed, spent);
        Ok(())
    }
}

impl Transport for RealtimeSim {
    fn write(&mut self, packet: &OutPacket) -> Result<(), TransportError> {
        self.send(|sim| Transport::write(sim, packet), false)
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Report>, TransportError> {
        let mut state = self.cell.lock()?;
        if state.unplugged {
            return Err(TransportError::Read("simulated unplug".to_string()));
        }
        if state.reset_pending {
            state.reset_pending = false;
            self.reset_seen = true;
            return Ok(Some(reset_report()));
        }
        let before = state.sim.elapsed();
        let result = Transport::read(&mut state.sim, timeout);
        let spent = state.sim.elapsed().saturating_sub(before);
        drop(state);
        pace(self.pacing, self.speed, spent);
        result
    }

    fn usb_info(&self) -> Option<UsbInfo> {
        Transport::usb_info(&self.cell.lock_any().sim)
    }

    fn now(&self) -> Instant {
        Transport::now(&self.cell.lock_any().sim)
    }
}

impl WriteTransport for RealtimeSim {
    fn write_command(&mut self, packet: &WritePacket) -> Result<(), TransportError> {
        self.send(|sim| sim.write_command(packet), true)
    }
}

/// The clock of a simulated keyboard.
struct SimClock(Arc<SimCell>);

impl Clock for SimClock {
    fn now(&self) -> Instant {
        Transport::now(&self.0.lock_any().sim)
    }
}

/// A clock moved by [`DeviceBackend::idle`] when no simulated keyboard
/// exists.
struct ManualClock(Mutex<Instant>);

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The simulator backend.
pub(crate) struct SimDeviceBackend {
    devices: Arc<Vec<SimDevice>>,
    pacing: Pacing,
    speed: f64,
    ids: TokenMint,
    fallback: Arc<ManualClock>,
}

impl SimDeviceBackend {
    /// A backend over `devices`.
    pub(crate) fn new(
        devices: Arc<Vec<SimDevice>>,
        options: &SimOptions,
        seed: Option<u64>,
    ) -> Self {
        SimDeviceBackend {
            devices,
            pacing: options.pacing,
            speed: options.speed,
            ids: TokenMint::new(seed),
            fallback: Arc::new(ManualClock(Mutex::new(Instant::now()))),
        }
    }
}

impl DeviceBackend for SimDeviceBackend {
    type Transport = RealtimeSim;

    fn enumerate(&mut self) -> Result<Vec<DeviceEntry>, TransportError> {
        let mut entries = Vec::new();
        for device in self.devices.iter() {
            let state = device.cell.lock()?;
            if state.unplugged {
                continue;
            }
            let usb = Transport::usb_info(&state.sim).unwrap_or(UsbInfo {
                vendor_id: 0x2E3C,
                product_id: 0xC368,
                product: None,
                manufacturer: None,
                serial: Some(device.serial.clone()),
            });
            entries.push(DeviceEntry {
                id: self.ids.device(&device.path),
                listing: DeviceListing {
                    path: device.path.clone(),
                    usb,
                    interface_number: 2,
                },
            });
        }
        Ok(entries)
    }

    fn open(&mut self, entry: &DeviceEntry) -> Result<RealtimeSim, TransportError> {
        let device = self
            .devices
            .iter()
            .find(|d| d.path == entry.listing.path)
            .ok_or_else(|| TransportError::Open {
                path: entry.listing.path.clone(),
                message: "no such simulated keyboard".to_string(),
            })?;
        let mut state = device.cell.lock()?;
        if state.unplugged {
            return Err(TransportError::Open {
                path: device.path.clone(),
                message: "the simulated keyboard is unplugged".to_string(),
            });
        }
        if state.open_failures > 0 {
            state.open_failures -= 1;
            return Err(TransportError::Open {
                path: device.path.clone(),
                message: "simulated open failure".to_string(),
            });
        }
        Ok(RealtimeSim {
            cell: Arc::clone(&device.cell),
            pacing: self.pacing,
            speed: self.speed,
            reset_seen: false,
        })
    }

    fn is_simulated(&self) -> bool {
        true
    }

    fn clock(&self) -> Arc<dyn Clock> {
        match self.devices.first() {
            Some(device) => Arc::new(SimClock(Arc::clone(&device.cell))),
            None => self.fallback.clone(),
        }
    }

    fn idle(&mut self, duration: Duration) {
        for device in self.devices.iter() {
            device.cell.lock_any().advance(duration);
        }
        if let Ok(mut now) = self.fallback.0.lock() {
            *now += duration;
        }
        pace(self.pacing, self.speed, duration);
    }

    fn idle_waits(&self) -> bool {
        self.pacing == Pacing::RealTime && self.speed > 0.0
    }
}

/// The developer controls of the simulated keyboards (the first one).
#[derive(Clone)]
pub(crate) struct SimControl {
    devices: Arc<Vec<SimDevice>>,
}

impl SimControl {
    /// Controls for `devices`.
    pub(crate) fn new(devices: Arc<Vec<SimDevice>>) -> Self {
        SimControl { devices }
    }

    fn first(&self) -> Result<&SimDevice, AppError> {
        self.devices
            .first()
            .ok_or_else(|| AppError::new(AppErrorKind::NotFound, "no simulated keyboard"))
    }

    /// Applies an action to the first simulated keyboard.
    pub(crate) fn apply(&self, action: SimAction) -> Result<SimInfo, AppError> {
        let device = self.first()?;
        let mut state = device
            .cell
            .lock()
            .map_err(|e| AppError::internal(e.to_string()))?;
        match action {
            SimAction::Unplug => state.unplug(),
            SimAction::Replug => state.unplugged = false,
            SimAction::VendorHeartbeat { count, interval_ms } => {
                let report = state.heartbeat.clone();
                for i in 0..u32::from(count) {
                    let at = Duration::from_millis(u64::from(interval_ms) * u64::from(i));
                    state.sim.push_input_at(at, report.clone());
                }
            }
            SimAction::KeyboardSwitch {
                profile,
                after_ms,
                report,
            } => {
                let profile = ProfileId::new(profile).ok_or_else(|| {
                    AppError::new(
                        AppErrorKind::InvalidRequest,
                        format!("profile {profile} does not exist"),
                    )
                })?;
                state.sim.switch_profile_at(
                    Duration::from_millis(u64::from(after_ms)),
                    profile,
                    report,
                );
            }
            SimAction::KeyboardReset => state.reset_pending = true,
            SimAction::SlowSave { ms } => state
                .sim
                .set_commit_delay(Duration::from_millis(u64::from(ms))),
            SimAction::DropNextAck => {
                let next = state.sim.writes_sent().saturating_add(1);
                state.sim.drop_ack(next);
            }
            SimAction::UnplugAfterPackets { packets } => {
                state.unplug_after = Some(packets.max(1));
            }
        }
        Ok(info_of(&state))
    }

    /// The first simulated keyboard's state.
    pub(crate) fn info(&self) -> Result<SimInfo, AppError> {
        let device = self.first()?;
        Ok(info_of(&device.cell.lock_any()))
    }

    /// Runs `f` on simulated keyboard `index` (test setup and checks).
    #[cfg(test)]
    pub(crate) fn with<R>(&self, index: usize, f: impl FnOnce(&mut DeviceSim) -> R) -> R {
        let device = &self.devices[index];
        f(&mut device.cell.lock_any().sim)
    }

    /// The captured status report of the vendor site (its heartbeat), to
    /// inject at a chosen packet.
    #[cfg(test)]
    pub(crate) fn heartbeat(&self) -> Report {
        self.devices[0].cell.lock_any().heartbeat.clone()
    }

    /// Packets sent after a reset notification on the same connection.
    #[cfg(test)]
    pub(crate) fn post_reset_packets(&self) -> u32 {
        self.devices
            .iter()
            .map(|d| d.cell.lock_any().post_reset_packets)
            .sum()
    }

    /// The next `n` opens of keyboard `index` fail.
    #[cfg(test)]
    pub(crate) fn fail_next_opens(&self, index: usize, n: u32) {
        self.devices[index].cell.lock_any().open_failures = n;
    }

    /// Whether a write is held by [`set_paused`](Self::set_paused).
    #[cfg(test)]
    pub(crate) fn is_blocked(&self) -> bool {
        self.devices.iter().any(|d| d.cell.lock_any().blocked)
    }

    /// Holds every write packet until [`set_paused`](Self::set_paused)
    /// with `false` (admission tests).
    #[cfg(test)]
    pub(crate) fn set_paused(&self, paused: bool) {
        for device in self.devices.iter() {
            device.cell.lock_any().paused = paused;
            device.cell.resume.notify_all();
        }
    }
}

fn info_of(state: &SimState) -> SimInfo {
    let sat = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    SimInfo {
        active_profile: state.sim.active_profile(),
        unsaved_edits: state.sim.has_unsaved_edits(),
        discarded_edits: sat(state.sim.discarded_edits()),
        writes_sent: sat(state.sim.writes_sent()),
        violations: sat(state.sim.violations().len()).saturating_add(state.post_reset_packets),
        unplugged: state.unplugged,
    }
}
