//! Backups for unit tests, replayed from the USB captures (no keyboard).

use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use openwraith_hid::{Backup, BackupEntry, BackupOptions, MockTransport, Session, ToolInfo};
use openwraith_protocol::{REPORT_LEN, RawResponse, ReadRequest, Report};

/// Backup replayed from capture c06 (W = 1.00 mm + RT 0.30, Custom red).
pub fn c06_backup() -> Backup {
    backup_from("capture_06_powercycle_W_persisted")
}

/// Backup replayed from `captures/<name>.json`.
pub fn backup_from(name: &str) -> Backup {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../captures")
        .join(format!("{name}.json"));
    let mock = MockTransport::from_capture_file(path).expect("capture loads");
    let options = BackupOptions {
        tool: ToolInfo {
            name: "openwraith-tests".into(),
            version: "0".into(),
        },
        created_at: Some(UNIX_EPOCH + Duration::from_secs(1_792_827_000)),
    };
    openwraith_hid::read_all_with(&mut Session::new(mock), &options, |_| {}).expect("backup")
}

/// Replaces the reply to `request` in `backup` with a table built from
/// `table` (chunked as the keyboard sends it, with read sub-command `sub`).
pub fn replace_table(backup: &mut Backup, request: ReadRequest, cmd: u8, sub: u8, table: &[u8]) {
    let layout = request.table_layout().expect("table request");
    let reports = (0..layout.chunks).map(|chunk| {
        let start = usize::from(chunk) * layout.chunk_len;
        let len = layout.expected_len(chunk);
        let mut bytes = [0u8; REPORT_LEN];
        let [hi, lo] = chunk.to_be_bytes();
        bytes[..5].copy_from_slice(&[cmd, sub, hi, lo, u8::try_from(len).expect("len")]);
        bytes[5..5 + len].copy_from_slice(&table[start..start + len]);
        Report::new(bytes)
    });
    replace_reply(backup, request, reports.collect());
}

/// Replaces the reply to `request` in `backup` with `reports` (which must be
/// a valid reply; it may still fail to decode).
pub fn replace_reply(backup: &mut Backup, request: ReadRequest, reports: Vec<Report>) {
    let raw = RawResponse::from_reports(request, reports).expect("valid reply");
    let entry = backup
        .entries
        .iter_mut()
        .find(|e| e.request() == request)
        .expect("request in backup");
    *entry = BackupEntry::new(raw);
}

// ----- the device simulator behind the CLI's connector -----

/// A simulated keyboard shared by the sessions a test opens.
pub type SharedSim = std::rc::Rc<std::cell::RefCell<openwraith_hid::DeviceSim>>;

/// A transport over the shared simulator that can write.
pub struct SimWriter(SharedSim);

/// A transport over the shared simulator that can only read (like
/// `HidTransport`): it does not implement `WriteTransport`.
pub struct SimReader(SharedSim);

impl openwraith_hid::Transport for SimWriter {
    fn write(
        &mut self,
        packet: &openwraith_protocol::OutPacket,
    ) -> Result<(), openwraith_hid::TransportError> {
        openwraith_hid::Transport::write(&mut *self.0.borrow_mut(), packet)
    }

    fn read(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<Option<Report>, openwraith_hid::TransportError> {
        openwraith_hid::Transport::read(&mut *self.0.borrow_mut(), timeout)
    }

    fn usb_info(&self) -> Option<openwraith_hid::UsbInfo> {
        openwraith_hid::Transport::usb_info(&*self.0.borrow())
    }

    /// The simulator's virtual clock.
    fn now(&self) -> std::time::Instant {
        openwraith_hid::Transport::now(&*self.0.borrow())
    }
}

impl openwraith_hid::WriteTransport for SimWriter {
    fn write_command(
        &mut self,
        packet: &openwraith_protocol::WritePacket,
    ) -> Result<(), openwraith_hid::TransportError> {
        openwraith_hid::WriteTransport::write_command(&mut *self.0.borrow_mut(), packet)
    }
}

impl openwraith_hid::Transport for SimReader {
    fn write(
        &mut self,
        packet: &openwraith_protocol::OutPacket,
    ) -> Result<(), openwraith_hid::TransportError> {
        openwraith_hid::Transport::write(&mut *self.0.borrow_mut(), packet)
    }

    fn read(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<Option<Report>, openwraith_hid::TransportError> {
        openwraith_hid::Transport::read(&mut *self.0.borrow_mut(), timeout)
    }

    fn usb_info(&self) -> Option<openwraith_hid::UsbInfo> {
        openwraith_hid::Transport::usb_info(&*self.0.borrow())
    }

    /// The simulator's virtual clock.
    fn now(&self) -> std::time::Instant {
        openwraith_hid::Transport::now(&*self.0.borrow())
    }
}

/// A [`Connector`](crate::device::Connector) over a shared simulator. It
/// counts the sessions of each kind it opened.
pub struct SimConnector {
    pub sim: SharedSim,
    pub read_sessions: usize,
    pub write_sessions: usize,
}

impl SimConnector {
    pub fn new(sim: openwraith_hid::DeviceSim) -> Self {
        SimConnector {
            sim: std::rc::Rc::new(std::cell::RefCell::new(sim)),
            read_sessions: 0,
            write_sessions: 0,
        }
    }
}

impl crate::device::Connector for SimConnector {
    type Read = SimReader;
    type Write = SimWriter;

    fn open_read(&mut self) -> anyhow::Result<Session<SimReader>> {
        self.read_sessions += 1;
        Ok(Session::new(SimReader(self.sim.clone())))
    }

    fn open_write(&mut self) -> anyhow::Result<Session<SimWriter>> {
        self.write_sessions += 1;
        Ok(Session::new(SimWriter(self.sim.clone())))
    }
}

/// A scripted console: records output, answers questions from a list and
/// can simulate Ctrl+C when a progress line matches.
#[derive(Debug, Default)]
pub struct TestUi {
    pub out: String,
    pub err: String,
    pub progress: Vec<String>,
    pub questions: Vec<String>,
    /// What had been written to stderr when each question was asked.
    pub notes_at_question: Vec<String>,
    pub answers: std::collections::VecDeque<bool>,
    pub interactive: bool,
    /// Simulated Ctrl+C at the `n`-th progress line containing the text.
    pub cancel_at: Option<(String, usize)>,
    pub cancel_enabled: bool,
    cancelled: bool,
}

impl TestUi {
    /// A console on a terminal that answers `answers` in order.
    pub fn answering(answers: &[bool]) -> Self {
        TestUi {
            interactive: true,
            answers: answers.iter().copied().collect(),
            ..TestUi::default()
        }
    }

    /// A console whose stdin is not a terminal.
    pub fn non_interactive() -> Self {
        TestUi::default()
    }
}

impl crate::ui::Ui for TestUi {
    fn print(&mut self, text: &str) -> anyhow::Result<()> {
        self.out.push_str(text);
        Ok(())
    }

    fn note(&mut self, text: &str) {
        self.err.push_str(text);
        self.err.push('\n');
    }

    fn progress(&mut self, text: &str) {
        self.progress.push(text.to_string());
        if let Some((needle, n)) = &mut self.cancel_at {
            if text.contains(needle.as_str()) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    self.cancelled = true;
                }
            }
        }
    }

    fn clear_progress(&mut self) {}

    fn can_prompt(&self) -> bool {
        self.interactive
    }

    fn confirm(&mut self, question: &str) -> anyhow::Result<bool> {
        self.questions.push(question.to_string());
        self.notes_at_question.push(self.err.clone());
        Ok(self.answers.pop_front().unwrap_or(false))
    }

    fn enable_cancel(&mut self) {
        self.cancel_enabled = true;
    }

    fn cancelled(&self) -> bool {
        self.cancelled
    }
}

/// A simulator seeded from c06 in which profiles 2-4 have factory triggers
/// and the factory light, so that the profiles differ like on the real
/// keyboard (profile 1 keeps c06: W 1.00 mm RT 0.30, Custom red).
pub fn sim_with_profiles() -> openwraith_hid::DeviceSim {
    use openwraith_hid::{DeviceSim, SimTable};
    use openwraith_protocol::{ProfileId, W75};
    let backup = c06_backup();
    let mut sim = DeviceSim::from_backup(&backup);
    let mut factory = DeviceSim::from_backup(&backup);
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
    sim
}
