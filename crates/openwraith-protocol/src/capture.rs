//! USB capture files (feature `capture`).
//!
//! The files in `captures/` are JSON arrays of entries
//! `{t, dir, id, data, ...}` with `dir` one of `OPEN`, `OUT`, `IN`, `MARK`,
//! `CLOSE` and `data` the 63-byte payload as space-separated hex (report ID 1
//! not included). See spec section 0.2.
//!
//! [`Capture::pair_reads`] pairs every captured read request with the input
//! reports that answer it, using the same [`ResponseCollector`] logic as a live
//! session. It copes with the vendor site's pipelining (several requests sent
//! before the first reply) and with unrelated traffic in between. The
//! `openwraith-hid` mock transport and this crate's tests are built on it.
//!
//! Parsing a string does no I/O; reading the file is up to the caller.

use crate::error::{FrameError, ResponseError};
use crate::frame::{REPORT_ID, REPORT_LEN, Report};
use crate::hex::parse_hex;
use crate::request::ReadRequest;
use crate::response::{Offer, RawResponse, ResponseCollector};

/// A request that is still unanswered this long after it was sent is given
/// up (the slowest captured reply arrived about 0.85 s late, after a reset).
pub const PENDING_EXPIRY_MS: u64 = 2000;

/// Error while parsing a capture file.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// The text is not a JSON array of capture entries.
    #[error("capture is not valid JSON: {0}")]
    Json(String),
    /// An entry is malformed.
    #[error("capture entry {index}: {message}")]
    Entry {
        /// Index of the entry in the array.
        index: usize,
        /// What is wrong.
        message: String,
    },
}

/// What a capture entry records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureEvent {
    /// The device was opened.
    Open {
        /// USB vendor ID.
        vid: u16,
        /// USB product ID.
        pid: u16,
        /// USB product string.
        name: String,
    },
    /// An output report sent by the host (63 bytes, report ID stripped).
    ///
    /// Kept as plain bytes: captured packets can be writes, and an
    /// [`OutPacket`](crate::OutPacket) can only come from a read request.
    Out([u8; REPORT_LEN]),
    /// An input report received from the device.
    In(Report),
    /// A marker inserted by the person capturing.
    Mark(String),
    /// The device was closed.
    Close,
}

/// One capture entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureEntry {
    /// Position in the file (0-based).
    pub index: usize,
    /// Timestamp in milliseconds (Unix epoch).
    pub t: u64,
    /// What happened.
    pub event: CaptureEvent,
}

/// A parsed capture file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capture {
    /// All entries in file order.
    pub entries: Vec<CaptureEntry>,
}

/// A captured read request and the reports that answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchange {
    /// The request.
    pub request: ReadRequest,
    /// Entry index of the `OUT` packet.
    pub out_index: usize,
    /// Timestamp of the `OUT` packet.
    pub t: u64,
    /// `(entry index, report)` of every reply chunk, in arrival order.
    pub replies: Vec<(usize, Report)>,
    /// Whether every chunk arrived.
    pub complete: bool,
}

impl Exchange {
    /// The reply reports in arrival order.
    pub fn reports(&self) -> Vec<Report> {
        self.replies.iter().map(|(_, r)| r.clone()).collect()
    }

    /// The complete reply, ready to decode.
    pub fn response(&self) -> Result<RawResponse, ResponseError> {
        RawResponse::from_reports(self.request, self.reports())
    }
}

/// Result of [`Capture::pair_reads`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pairing {
    /// One exchange per captured read request, in `OUT` order.
    pub exchanges: Vec<Exchange>,
    /// Entry indices of `IN` reports that answer none of the captured reads
    /// (write acks, reset notifications, lost-request replies, ...).
    pub unmatched: Vec<usize>,
}

#[derive(serde::Deserialize)]
struct RawEntry {
    t: f64,
    dir: String,
    id: Option<u8>,
    data: Option<String>,
    vid: Option<u16>,
    pid: Option<u16>,
    name: Option<String>,
    label: Option<String>,
}

impl Capture {
    /// Parses the JSON text of a capture file.
    pub fn from_json(text: &str) -> Result<Capture, CaptureError> {
        let raw: Vec<RawEntry> =
            serde_json::from_str(text).map_err(|e| CaptureError::Json(e.to_string()))?;
        let entries = raw
            .into_iter()
            .enumerate()
            .map(|(index, entry)| parse_entry(index, entry))
            .collect::<Result<_, _>>()?;
        Ok(Capture { entries })
    }

    /// Timestamp of the first entry (normally `OPEN`).
    pub fn start_time(&self) -> u64 {
        self.entries.first().map_or(0, |e| e.t)
    }

    /// Product ID of the first `OPEN` entry.
    pub fn open_pid(&self) -> Option<u16> {
        self.entries.iter().find_map(|e| match e.event {
            CaptureEvent::Open { pid, .. } => Some(pid),
            _ => None,
        })
    }

    /// Entry index of the first `MARK` with this label.
    pub fn mark(&self, label: &str) -> Option<usize> {
        self.entries.iter().position(|e| match &e.event {
            CaptureEvent::Mark(l) => l == label,
            _ => false,
        })
    }

    /// `(entry index, bytes)` of every `OUT` packet.
    pub fn out_packets(&self) -> impl Iterator<Item = (usize, &[u8; REPORT_LEN])> {
        self.entries.iter().filter_map(|e| match &e.event {
            CaptureEvent::Out(bytes) => Some((e.index, bytes)),
            _ => None,
        })
    }

    /// `(entry index, report)` of every `IN` report.
    pub fn in_reports(&self) -> impl Iterator<Item = (usize, &Report)> {
        self.entries.iter().filter_map(|e| match &e.event {
            CaptureEvent::In(report) => Some((e.index, report)),
            _ => None,
        })
    }

    /// Pairs every captured read request with its reply reports.
    ///
    /// Each `IN` report goes to the oldest pending request whose
    /// [`ResponseSpec`](crate::ResponseSpec) accepts it. Requests still
    /// unanswered after [`PENDING_EXPIRY_MS`], or at `CLOSE`, end incomplete.
    pub fn pair_reads(&self) -> Pairing {
        let mut pending: Vec<(Exchange, ResponseCollector)> = Vec::new();
        let mut done: Vec<Exchange> = Vec::new();
        let mut unmatched = Vec::new();

        for entry in &self.entries {
            // Give up on requests that waited too long.
            let (expired, kept): (Vec<_>, Vec<_>) = pending
                .into_iter()
                .partition(|(ex, _)| entry.t.saturating_sub(ex.t) > PENDING_EXPIRY_MS);
            pending = kept;
            done.extend(expired.into_iter().map(|(ex, _)| ex));

            match &entry.event {
                CaptureEvent::Out(bytes) => {
                    if let Some(request) = ReadRequest::from_packet(bytes) {
                        let exchange = Exchange {
                            request,
                            out_index: entry.index,
                            t: entry.t,
                            replies: Vec::new(),
                            complete: false,
                        };
                        pending.push((exchange, request.collector()));
                    }
                }
                CaptureEvent::In(report) => {
                    let taker = pending.iter_mut().position(|(ex, collector)| {
                        let accepted = collector.offer(report) == Offer::Accepted;
                        if accepted {
                            ex.replies.push((entry.index, report.clone()));
                            ex.complete = collector.is_complete();
                        }
                        accepted
                    });
                    match taker {
                        Some(i) if pending.get(i).is_some_and(|(ex, _)| ex.complete) => {
                            done.push(pending.remove(i).0);
                        }
                        Some(_) => {}
                        None => unmatched.push(entry.index),
                    }
                }
                CaptureEvent::Close => done.extend(pending.drain(..).map(|(ex, _)| ex)),
                CaptureEvent::Open { .. } | CaptureEvent::Mark(_) => {}
            }
        }
        done.extend(pending.into_iter().map(|(ex, _)| ex));
        done.sort_by_key(|ex| ex.out_index);
        Pairing {
            exchanges: done,
            unmatched,
        }
    }
}

fn parse_entry(index: usize, raw: RawEntry) -> Result<CaptureEntry, CaptureError> {
    let err = |message: String| CaptureError::Entry { index, message };
    // Timestamps are integral milliseconds; the conversion saturates.
    let t = raw.t.max(0.0) as u64;
    let payload = |raw: &RawEntry| -> Result<[u8; REPORT_LEN], CaptureError> {
        if raw.id != Some(REPORT_ID) {
            return Err(err(format!("report ID {:?}, expected 1", raw.id)));
        }
        let text = raw
            .data
            .as_deref()
            .ok_or_else(|| err("missing data".into()))?;
        let bytes = parse_hex(text).map_err(|e: FrameError| err(e.to_string()))?;
        <[u8; REPORT_LEN]>::try_from(bytes.as_slice())
            .map_err(|_| err(format!("{} payload bytes, expected 63", bytes.len())))
    };
    let event = match raw.dir.as_str() {
        "OPEN" => CaptureEvent::Open {
            vid: raw.vid.unwrap_or_default(),
            pid: raw.pid.unwrap_or_default(),
            name: raw.name.clone().unwrap_or_default(),
        },
        "OUT" => CaptureEvent::Out(payload(&raw)?),
        "IN" => CaptureEvent::In(Report::new(payload(&raw)?)),
        "MARK" => CaptureEvent::Mark(raw.label.clone().unwrap_or_default()),
        "CLOSE" => CaptureEvent::Close,
        other => return Err(err(format!("unknown dir {other:?}"))),
    };
    Ok(CaptureEntry { index, t, event })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex63(prefix: &str) -> String {
        let mut bytes = parse_hex(prefix).unwrap();
        bytes.resize(REPORT_LEN, 0);
        crate::hex::to_spaced_hex(&bytes)
    }

    fn entry(t: u64, dir: &str, prefix: &str) -> String {
        format!(
            r#"{{"t":{t},"dir":"{dir}","id":1,"data":"{}"}}"#,
            hex63(prefix)
        )
    }

    #[test]
    fn pairs_pipelined_requests() {
        // The site sends `24 02` and `24 01` back to back, then both replies arrive.
        let json = format!(
            "[{},{},{},{},{},{},{},{}]",
            r#"{"t":0,"dir":"OPEN","vid":11836,"pid":50024,"name":"Wraith W75"}"#,
            entry(1, "OUT", "24 02"),
            entry(1, "OUT", "24 01"),
            entry(2, "IN", "01 00 00 00 12"),
            entry(2, "IN", "24 02 00 00 06 00 00 00 00 fa e5"),
            entry(3, "IN", "24 01 00 00 28"),
            entry(4, "IN", "24 01 00 01 28"),
            r#"{"t":5,"dir":"MARK","label":"END"}"#,
        );
        let capture = Capture::from_json(&json).unwrap();
        assert_eq!(capture.open_pid(), Some(0xC368));
        assert_eq!(capture.mark("END"), Some(7));
        let pairing = capture.pair_reads();
        assert_eq!(pairing.exchanges.len(), 2);
        assert_eq!(pairing.exchanges[0].request, ReadRequest::SocdSettings);
        assert!(pairing.exchanges[0].complete);
        assert_eq!(pairing.exchanges[1].request, ReadRequest::SocdPairs);
        assert!(pairing.exchanges[1].complete);
        assert_eq!(pairing.exchanges[1].replies.len(), 2);
        assert_eq!(pairing.unmatched, vec![3]);
        assert!(pairing.exchanges[1].response().is_ok());
    }

    #[test]
    fn unanswered_requests_expire() {
        let json = format!(
            "[{},{},{}]",
            entry(0, "OUT", "01"),
            entry(5000, "OUT", "01"),
            entry(5001, "IN", "01 00 00 00 12"),
        );
        let pairing = Capture::from_json(&json).unwrap().pair_reads();
        assert_eq!(pairing.exchanges.len(), 2);
        assert!(!pairing.exchanges[0].complete);
        assert!(pairing.exchanges[1].complete);
    }

    #[test]
    fn writes_are_not_paired() {
        let json = format!(
            "[{},{}]",
            entry(0, "OUT", "14 00 00 00 01 01"),
            entry(1, "IN", "14 00 00 00 00"),
        );
        let pairing = Capture::from_json(&json).unwrap().pair_reads();
        assert!(pairing.exchanges.is_empty());
        assert_eq!(pairing.unmatched, vec![1]);
    }

    #[test]
    fn rejects_malformed_entries() {
        assert!(Capture::from_json("{}").is_err());
        assert!(Capture::from_json(r#"[{"t":0,"dir":"IN","id":1,"data":"01 02"}]"#).is_err());
        assert!(Capture::from_json(r#"[{"t":0,"dir":"IN","id":2,"data":"01"}]"#).is_err());
        assert!(Capture::from_json(r#"[{"t":0,"dir":"SIDEWAYS"}]"#).is_err());
    }
}
