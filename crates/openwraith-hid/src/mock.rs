//! A [`Transport`] that replays captured USB traffic (feature `mock`).
//!
//! [`MockTransport::from_capture`] pairs every read request in a capture file
//! (`captures/*.json`) with the reports that answered it. When a session
//! writes a request, the mock queues the reply reports that followed the
//! identical packet in the capture, in their original order. Each captured
//! exchange is served once, in capture order; when a request has been served
//! as often as it was captured, the last reply is served again.
//!
//! Tests can make the traffic harder: [`with_noise`](MockTransport::with_noise)
//! interleaves unrelated reports with every reply chunk,
//! [`schedule`](MockTransport::schedule) injects unsolicited reports (such as
//! the keyboard's reset notification) after a given write, and
//! [`silence`](MockTransport::silence) makes a request go unanswered.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use openwraith_protocol::capture::{Capture, CaptureError, CaptureEvent};
use openwraith_protocol::{OutPacket, ReadRequest, Report};

use crate::error::TransportError;
use crate::transport::{Transport, UsbInfo};

/// A capture file could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum MockError {
    /// The file could not be read.
    #[error("cannot read {}: {source}", path.display())]
    Io {
        /// The capture file.
        path: PathBuf,
        /// The I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The file is not a valid capture.
    #[error(transparent)]
    Capture(#[from] CaptureError),
}

/// A transport that answers read requests from captured traffic.
#[derive(Debug, Clone, Default)]
pub struct MockTransport {
    replies: HashMap<ReadRequest, Vec<Vec<Report>>>,
    served: HashMap<ReadRequest, usize>,
    silent: HashSet<ReadRequest>,
    noise: Vec<Report>,
    noise_pos: usize,
    scheduled: Vec<(usize, Report)>,
    queue: VecDeque<Report>,
    written: Vec<OutPacket>,
    usb: Option<UsbInfo>,
}

impl MockTransport {
    /// A mock that answers nothing until replies are added.
    pub fn new() -> Self {
        Self::default()
    }

    /// A mock serving every complete read exchange of `capture`. The USB
    /// identity comes from the capture's `OPEN` entry.
    pub fn from_capture(capture: &Capture) -> Self {
        let mut mock = MockTransport::new();
        for exchange in capture.pair_reads().exchanges {
            if exchange.complete {
                mock.add_reply(exchange.request, exchange.reports());
            }
        }
        mock.usb = capture.entries.iter().find_map(|e| match &e.event {
            CaptureEvent::Open { vid, pid, name } => Some(UsbInfo {
                vendor_id: *vid,
                product_id: *pid,
                product: (!name.is_empty()).then(|| name.clone()),
                manufacturer: None,
                serial: None,
            }),
            _ => None,
        });
        mock
    }

    /// Parses capture JSON text and builds a mock from it.
    pub fn from_capture_json(text: &str) -> Result<Self, MockError> {
        Ok(Self::from_capture(&Capture::from_json(text)?))
    }

    /// Reads a capture file and builds a mock from it.
    pub fn from_capture_file(path: impl AsRef<Path>) -> Result<Self, MockError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| MockError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_capture_json(&text)
    }

    /// Adds one reply (its reports in arrival order) for `request`. Replies
    /// for the same request are served in the order they were added.
    pub fn add_reply(&mut self, request: ReadRequest, reports: Vec<Report>) {
        self.replies.entry(request).or_default().push(reports);
    }

    /// Replaces every reply for `request` (including captured ones) with
    /// this one, served on every write of the request.
    pub fn set_reply(&mut self, request: ReadRequest, reports: Vec<Report>) {
        self.replies.insert(request, vec![reports]);
        self.served.remove(&request);
    }

    /// Interleaves `noise` with the replies: one noise report (cycling
    /// through the list) is queued before every reply chunk.
    pub fn with_noise(mut self, noise: Vec<Report>) -> Self {
        self.noise = noise;
        self.noise_pos = 0;
        self
    }

    /// Queues `report` right after the `after_writes`-th write (1-based),
    /// before that request's reply. Use it for unsolicited reports.
    pub fn schedule(&mut self, after_writes: usize, report: Report) {
        self.scheduled.push((after_writes, report));
    }

    /// Queues `report` immediately, as if it had arrived before the next
    /// request (stale input).
    pub fn push_input(&mut self, report: Report) {
        self.queue.push_back(report);
    }

    /// Never answer `request` (to test timeouts).
    pub fn silence(&mut self, request: ReadRequest) {
        self.silent.insert(request);
    }

    /// Sets the USB identity reported by [`Transport::usb_info`].
    pub fn set_usb_info(&mut self, usb: Option<UsbInfo>) {
        self.usb = usb;
    }

    /// Every packet written so far, in order.
    pub fn written(&self) -> &[OutPacket] {
        &self.written
    }

    /// Number of input reports queued but not yet read.
    pub fn pending_input(&self) -> usize {
        self.queue.len()
    }

    fn queue_reply(&mut self, request: ReadRequest) {
        if self.silent.contains(&request) {
            return;
        }
        let Some(replies) = self.replies.get(&request) else {
            return;
        };
        let served = self.served.entry(request).or_default();
        let reply = replies.get(*served).or_else(|| replies.last());
        *served += 1;
        let Some(reply) = reply.cloned() else {
            return;
        };
        for report in reply {
            if let Some(noise) = self.noise.get(self.noise_pos).cloned() {
                self.queue.push_back(noise);
                self.noise_pos = (self.noise_pos + 1) % self.noise.len();
            }
            self.queue.push_back(report);
        }
    }
}

impl Transport for MockTransport {
    fn write(&mut self, packet: &OutPacket) -> Result<(), TransportError> {
        self.written.push(packet.clone());
        let count = self.written.len();
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.scheduled)
            .into_iter()
            .partition(|(after, _)| *after == count);
        self.scheduled = later;
        self.queue.extend(due.into_iter().map(|(_, report)| report));
        // Every OutPacket is a read request; anything else gets no reply.
        if let Some(request) = ReadRequest::from_packet(packet.as_bytes()) {
            self.queue_reply(request);
        }
        Ok(())
    }

    /// Returns the next queued report at once, or `None` (an immediate
    /// "timeout") when the queue is empty. The mock never sleeps.
    fn read(&mut self, _timeout: Duration) -> Result<Option<Report>, TransportError> {
        Ok(self.queue.pop_front())
    }

    fn usb_info(&self) -> Option<UsbInfo> {
        self.usb.clone()
    }
}
