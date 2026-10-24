//! Collecting the reply reports of a request (spec sections 2.5 and 2.8).
//!
//! The protocol has no sequence numbers. A reply is recognised by its header
//! ([`ResponseSpec`]) and is complete when every chunk
//! index of the table has arrived. Unrelated reports (another application's
//! traffic, unsolicited status pushes) are simply not accepted.

use crate::decode::{self, Decoded};
use crate::error::{DecodeError, ResponseError};
use crate::frame::{HEADER_LEN, Report};
use crate::request::{ReadRequest, ResponseSpec};

/// What a [`ResponseCollector`] did with an offered report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    /// The report is a chunk of this reply and was stored.
    Accepted,
    /// The report matches, but its chunk index was already received with the
    /// same data; the first copy is kept.
    Duplicate,
    /// The report matches and its chunk index was already received, but with
    /// **different** data: the keyboard's settings changed between two
    /// replies (for example because another program wrote them while we were
    /// reading). The first copy is kept; the caller should not complete the
    /// reply from these chunks but read it again from scratch.
    Conflict,
    /// The report does not belong to this reply.
    Ignored,
}

/// Accumulates the reply reports of one [`ReadRequest`].
#[derive(Debug, Clone)]
pub struct ResponseCollector {
    request: ReadRequest,
    spec: ResponseSpec,
    chunks: Vec<Option<Report>>,
}

impl ResponseCollector {
    /// An empty collector for `request`.
    pub fn new(request: ReadRequest) -> Self {
        let spec = request.response_spec();
        ResponseCollector {
            request,
            spec,
            chunks: vec![None; usize::from(spec.chunks)],
        }
    }

    /// The request this collector belongs to.
    pub fn request(&self) -> ReadRequest {
        self.request
    }

    /// Offers an input report. Matching chunks are stored, everything else is
    /// ignored. A second copy of a chunk is compared with the first one (see
    /// [`Offer::Duplicate`] and [`Offer::Conflict`]).
    pub fn offer(&mut self, report: &Report) -> Offer {
        if !self.spec.matches(report) {
            return Offer::Ignored;
        }
        match self.chunks.get_mut(usize::from(report.chunk_index())) {
            Some(slot @ None) => {
                *slot = Some(report.clone());
                Offer::Accepted
            }
            Some(Some(existing)) if same_data(existing, report) => Offer::Duplicate,
            Some(Some(_)) => Offer::Conflict,
            None => Offer::Ignored,
        }
    }

    /// Whether every chunk has arrived.
    pub fn is_complete(&self) -> bool {
        self.chunks.iter().all(Option::is_some)
    }

    /// Whether at least one chunk has arrived.
    pub fn has_started(&self) -> bool {
        self.chunks.iter().any(Option::is_some)
    }

    /// Number of chunks received so far.
    pub fn received(&self) -> usize {
        self.chunks.iter().filter(|c| c.is_some()).count()
    }

    /// Number of chunks the reply has.
    pub fn expected(&self) -> u16 {
        self.spec.chunks
    }

    /// Chunk indices that are still missing.
    pub fn missing(&self) -> Vec<u16> {
        (0..self.spec.chunks)
            .zip(&self.chunks)
            .filter(|(_, c)| c.is_none())
            .map(|(i, _)| i)
            .collect()
    }

    /// Forgets every received chunk (used before a retry).
    pub fn reset(&mut self) {
        self.chunks.iter_mut().for_each(|c| *c = None);
    }

    /// Turns the collected chunks into a [`RawResponse`], or reports which
    /// chunks are missing.
    pub fn finish(self) -> Result<RawResponse, ResponseError> {
        let missing = self.missing();
        if !missing.is_empty() {
            return Err(ResponseError::Incomplete {
                request: self.request,
                missing,
                expected: self.spec.chunks,
            });
        }
        Ok(RawResponse {
            request: self.request,
            reports: self.chunks.into_iter().flatten().collect(),
        })
    }
}

/// Whether two copies of a chunk carry the same data: the header and the
/// declared payload. Bytes after the declared length are stale buffer
/// contents (spec section 2.3) and may differ between two otherwise identical
/// replies, so they are not compared.
fn same_data(a: &Report, b: &Report) -> bool {
    a.as_bytes().get(..HEADER_LEN) == b.as_bytes().get(..HEADER_LEN) && a.payload() == b.payload()
}

/// A complete reply: every chunk of one request, ordered by chunk index.
///
/// This is the lossless unit of a backup. Decoders take a `RawResponse` and
/// keep its bytes in their `raw` fields.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "RawResponseRepr"))]
pub struct RawResponse {
    request: ReadRequest,
    reports: Vec<Report>,
}

impl RawResponse {
    /// Validates that `reports` form exactly the complete reply to `request`
    /// (every report matches, no chunk missing). Order does not matter;
    /// duplicates of a chunk are rejected.
    pub fn from_reports(
        request: ReadRequest,
        reports: impl IntoIterator<Item = Report>,
    ) -> Result<Self, ResponseError> {
        let mut collector = request.collector();
        for report in reports {
            if collector.offer(&report) != Offer::Accepted {
                return Err(ResponseError::Unrelated {
                    request,
                    report: report.summary(),
                });
            }
        }
        collector.finish()
    }

    /// The request this reply answers.
    pub fn request(&self) -> ReadRequest {
        self.request
    }

    /// The reply reports, ordered by chunk index.
    pub fn reports(&self) -> &[Report] {
        &self.reports
    }

    /// The first (for single replies: the only) report.
    pub fn first(&self) -> Option<&Report> {
        self.reports.first()
    }

    /// Concatenated declared payloads of all chunks, in chunk order.
    pub fn payload(&self) -> Vec<u8> {
        self.reports
            .iter()
            .flat_map(|r| r.payload().iter().copied())
            .collect()
    }

    /// The table bytes of a multi-chunk reply, checking that every chunk
    /// declares the length the spec gives it (section 2.5). For single-report
    /// replies this is the declared payload.
    pub fn table_bytes(&self, what: &'static str) -> Result<Vec<u8>, DecodeError> {
        let Some(layout) = self.request.table_layout() else {
            return Ok(self.payload());
        };
        let mut out = Vec::with_capacity(layout.total_len);
        for report in &self.reports {
            let chunk = report.chunk_index();
            let expected = layout.expected_len(chunk);
            let got = usize::from(report.declared_len());
            if got != expected {
                return Err(DecodeError::ChunkLength {
                    what,
                    chunk,
                    expected,
                    got,
                });
            }
            out.extend_from_slice(report.payload());
        }
        Ok(out)
    }

    /// Decodes the reply into its typed model.
    pub fn decode(&self) -> Result<Decoded, DecodeError> {
        decode::decode(self)
    }
}

/// Unvalidated serde form of [`RawResponse`].
#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct RawResponseRepr {
    request: ReadRequest,
    reports: Vec<Report>,
}

#[cfg(feature = "serde")]
impl TryFrom<RawResponseRepr> for RawResponse {
    type Error = ResponseError;

    fn try_from(repr: RawResponseRepr) -> Result<Self, Self::Error> {
        RawResponse::from_reports(repr.request, repr.reports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::REPORT_LEN;
    use crate::layout::KeyPos;
    use crate::request::{CustomTable, Layer};

    fn report(prefix: &[u8]) -> Report {
        let mut bytes = [0u8; REPORT_LEN];
        bytes[..prefix.len()].copy_from_slice(prefix);
        Report::new(bytes)
    }

    fn keymap_chunk(i: u8) -> Report {
        report(&[0x18, 0x80, 0x00, i, if i == 9 { 0x18 } else { 0x38 }])
    }

    #[test]
    fn collects_out_of_order_chunks_and_ignores_unrelated() {
        let mut c = ReadRequest::Keymap(Layer::Base).collector();
        assert_eq!(c.expected(), 10);
        assert!(!c.has_started());
        // Heartbeat reply and the Fn table are not ours.
        assert_eq!(c.offer(&report(&[0x01, 0, 0, 0, 0x12])), Offer::Ignored);
        assert_eq!(c.offer(&report(&[0x18, 0x82, 0, 0, 0x38])), Offer::Ignored);
        for i in (0..10).rev() {
            assert_eq!(c.offer(&keymap_chunk(i)), Offer::Accepted);
        }
        assert_eq!(c.offer(&keymap_chunk(3)), Offer::Duplicate);
        assert!(c.is_complete());
        let raw = c.finish().unwrap();
        let order: Vec<u16> = raw.reports().iter().map(Report::chunk_index).collect();
        assert_eq!(order, (0..10).collect::<Vec<_>>());
        assert_eq!(raw.table_bytes("keymap").unwrap().len(), 528);
    }

    #[test]
    fn a_second_copy_with_different_data_is_a_conflict() {
        let mut c = ReadRequest::Keymap(Layer::Base).collector();
        assert_eq!(c.offer(&keymap_chunk(3)), Offer::Accepted);
        // Chunk 3 declares 56 bytes (report bytes 5..61); bytes 61 and 62 are
        // stale buffer contents and do not count.
        let mut stale = *keymap_chunk(3).as_bytes();
        stale[62] = 0xEE;
        assert_eq!(c.offer(&Report::new(stale)), Offer::Duplicate);
        // A changed table byte does.
        let mut changed = *keymap_chunk(3).as_bytes();
        changed[10] = 0x04;
        assert_eq!(c.offer(&Report::new(changed)), Offer::Conflict);
        // The first copy is kept.
        assert_eq!(c.received(), 1);
        for i in (0..10).filter(|i| *i != 3) {
            c.offer(&keymap_chunk(i));
        }
        let raw = c.finish().unwrap();
        assert_eq!(raw.reports()[3], keymap_chunk(3));
    }

    #[test]
    fn incomplete_reply_reports_missing_chunks() {
        let mut c = ReadRequest::Keymap(Layer::Base).collector();
        c.offer(&keymap_chunk(0));
        c.offer(&keymap_chunk(9));
        assert_eq!(c.received(), 2);
        let err = c.clone().finish().unwrap_err();
        assert_eq!(
            err,
            ResponseError::Incomplete {
                request: ReadRequest::Keymap(Layer::Base),
                missing: (1..9).collect(),
                expected: 10,
            }
        );
        c.reset();
        assert_eq!(c.received(), 0);
    }

    #[test]
    fn chunk_length_mismatch_is_an_error() {
        let chunks = (0..8).map(|i| report(&[0x09, 0x80, 0, i, 0x36]));
        let raw =
            RawResponse::from_reports(ReadRequest::CustomColors(CustomTable::Custom1), chunks)
                .unwrap();
        // Chunk 7 must declare 18 bytes, not 54.
        assert!(matches!(
            raw.table_bytes("colors"),
            Err(DecodeError::ChunkLength {
                chunk: 7,
                expected: 18,
                got: 54,
                ..
            })
        ));
    }

    #[test]
    fn from_reports_rejects_foreign_and_duplicate_reports() {
        let w = ReadRequest::KeyTrigger(KeyPos::new(2, 2).unwrap());
        let wrong_echo = report(&[
            0x21, 0, 0, 0, 0x0c, 0x05, 0, 0x55, 0x55, 1, 1, 0, 0, 0, 0, 2, 1,
        ]);
        assert!(matches!(
            RawResponse::from_reports(w, [wrong_echo]),
            Err(ResponseError::Unrelated { .. })
        ));
        let ok = report(&[
            0x21, 0, 0, 0, 0x0c, 0x05, 0, 0x55, 0x55, 1, 1, 0, 0, 0, 0, 2, 2,
        ]);
        assert!(RawResponse::from_reports(w, [ok.clone()]).is_ok());
        assert!(RawResponse::from_reports(w, [ok.clone(), ok]).is_err());
        assert!(RawResponse::from_reports(w, []).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_validates_on_load() {
        let ok = report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x01]);
        let raw = RawResponse::from_reports(ReadRequest::ActiveProfile, [ok]).unwrap();
        let json = serde_json::to_string(&raw).unwrap();
        let back: RawResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, raw);
        let tampered = json.replace("\"active_profile\"", "\"poll_rate\"");
        assert!(serde_json::from_str::<RawResponse>(&tampered).is_err());
    }
}
