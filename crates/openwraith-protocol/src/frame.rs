//! Report framing (spec sections 2.2 to 2.4).
//!
//! Every report in both directions uses report ID 1 and a 63-byte payload.
//! Offsets in this crate follow the spec: **byte 0 is the first byte after the
//! report ID**.
//!
//! | Byte | Field |
//! |---|---|
//! | 0 | command |
//! | 1 | sub-command / table (echoed in replies, with exceptions) |
//! | 2-3 | chunk index, 16-bit big-endian (byte 2 is always 0 on the W75) |
//! | 4 | declared payload length, at most 58 |
//! | 5..62 | payload; bytes after `5 + len` are padding or stale buffer data |

use std::fmt;

use crate::error::FrameError;
use crate::hex;

/// HID report ID used for every request and reply (CONFIRMED).
pub const REPORT_ID: u8 = 0x01;
/// Length of a report payload, without the report ID (CONFIRMED).
pub const REPORT_LEN: usize = 63;
/// Number of header bytes before the payload (cmd, sub, chunk hi, chunk lo, len).
pub const HEADER_LEN: usize = 5;
/// Largest payload a single report can carry (`0x3A`, CONFIRMED).
pub const MAX_CHUNK_PAYLOAD: usize = REPORT_LEN - HEADER_LEN;

/// A 63-byte output report (request) ready to be sent.
///
/// There is deliberately **no public constructor**: the only way to obtain an
/// `OutPacket` is [`ReadRequest::packet`](crate::ReadRequest::packet), which
/// only produces documented read requests. This is what makes Phase 1
/// read-only by construction.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct OutPacket([u8; REPORT_LEN]);

impl OutPacket {
    /// Builds a zero-padded packet from its leading bytes. Crate-private.
    pub(crate) fn from_prefix(prefix: &[u8]) -> Self {
        let mut bytes = [0u8; REPORT_LEN];
        for (dst, src) in bytes.iter_mut().zip(prefix) {
            *dst = *src;
        }
        OutPacket(bytes)
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
}

impl fmt::Debug for OutPacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OutPacket({})", hex::to_spaced_hex(trimmed(&self.0)))
    }
}

/// A 63-byte input report (reply or unsolicited report), report ID stripped.
///
/// Parsing never fails because of the contents: byte 4 may declare fewer
/// bytes than the report carries (spec section 2.4) and the device leaves
/// stale data after the declared length (section 2.3). Use
/// [`payload`](Report::payload) to get only the declared bytes.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Report([u8; REPORT_LEN]);

impl Report {
    /// Wraps a 63-byte payload (byte 0 = command).
    pub fn new(bytes: [u8; REPORT_LEN]) -> Self {
        Report(bytes)
    }

    /// Parses a buffer as returned by a HID read.
    ///
    /// Accepts a bare 63-byte payload, or 64 bytes whose first byte is report
    /// ID 1 (hidapi returns the report ID first for numbered reports).
    pub fn from_hid_buffer(buf: &[u8]) -> Result<Self, FrameError> {
        let payload = match buf.len() {
            REPORT_LEN => buf,
            len if len == REPORT_LEN + 1 => {
                let (id, rest) = buf.split_at(1);
                if id != [REPORT_ID] {
                    return Err(FrameError::WrongReportId {
                        id: id.first().copied().unwrap_or_default(),
                    });
                }
                rest
            }
            len => return Err(FrameError::BadLength { len }),
        };
        let mut bytes = [0u8; REPORT_LEN];
        bytes.copy_from_slice(payload);
        Ok(Report(bytes))
    }

    /// Parses 63 bytes of hex text (the capture-file format, e.g. `"0d 00 00 00 32 57 ..."`).
    pub fn from_hex(text: &str) -> Result<Self, FrameError> {
        Self::from_hid_buffer(&hex::parse_hex(text)?)
    }

    /// All 63 bytes.
    pub fn as_bytes(&self) -> &[u8; REPORT_LEN] {
        &self.0
    }

    /// Byte `i` of the report (0 = command); 0 when `i` is out of range.
    pub fn byte(&self, i: usize) -> u8 {
        self.0.get(i).copied().unwrap_or(0)
    }

    /// Command byte (byte 0).
    pub fn cmd(&self) -> u8 {
        self.0[0]
    }

    /// Sub-command byte (byte 1).
    pub fn sub(&self) -> u8 {
        self.0[1]
    }

    /// Chunk index: bytes 2-3 as a big-endian 16-bit value (spec section 2.3).
    pub fn chunk_index(&self) -> u16 {
        u16::from_be_bytes([self.0[2], self.0[3]])
    }

    /// Declared payload length (byte 4), as sent by the device.
    pub fn declared_len(&self) -> u8 {
        self.0[4]
    }

    /// The declared payload: bytes `5 .. 5 + len`, with `len` clamped to 58.
    /// Stale bytes after the declared length are not included.
    pub fn payload(&self) -> &[u8] {
        let len = usize::from(self.declared_len()).min(MAX_CHUNK_PAYLOAD);
        &self.0[HEADER_LEN..HEADER_LEN + len]
    }

    /// Every byte after the header (bytes 5..62), ignoring the declared length.
    pub fn data(&self) -> &[u8] {
        &self.0[HEADER_LEN..]
    }

    /// Lowercase hex of all 63 bytes without separators.
    pub fn to_hex(&self) -> String {
        hex::to_hex(&self.0)
    }

    /// Short hex dump of the header and declared payload, for messages.
    pub fn summary(&self) -> String {
        let end = HEADER_LEN + self.payload().len();
        hex::to_spaced_hex(&self.0[..end])
    }
}

impl fmt::Debug for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Report({})", self.summary())
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Report {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Report {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Report::from_hex(&text).map_err(serde::de::Error::custom)
    }
}

/// The bytes up to the last non-zero byte, but at least the 5-byte header.
pub(crate) fn trimmed(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|b| *b != 0)
        .map_or(0, |i| i + 1)
        .max(HEADER_LEN)
        .min(bytes.len());
    &bytes[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(prefix: &[u8]) -> Report {
        let mut bytes = [0u8; REPORT_LEN];
        bytes[..prefix.len()].copy_from_slice(prefix);
        Report::new(bytes)
    }

    #[test]
    fn constants() {
        assert_eq!(MAX_CHUNK_PAYLOAD, 0x3A);
        assert_eq!(REPORT_LEN, 63);
    }

    #[test]
    fn header_fields() {
        let r = report(&[0x09, 0x80, 0x00, 0x07, 0x12, 0xff]);
        assert_eq!(r.cmd(), 0x09);
        assert_eq!(r.sub(), 0x80);
        assert_eq!(r.chunk_index(), 7);
        assert_eq!(r.declared_len(), 0x12);
        assert_eq!(r.payload().len(), 0x12);
        assert_eq!(r.payload()[0], 0xff);
        assert_eq!(r.data().len(), 58);
    }

    #[test]
    fn chunk_index_is_big_endian() {
        assert_eq!(
            report(&[0x22, 0x80, 0x01, 0x02, 0x28]).chunk_index(),
            0x0102
        );
    }

    #[test]
    fn stale_bytes_after_declared_length_are_ignored() {
        // 25 02 chunk 2: 16 valid bytes, followed by stale 0x01 bytes (spec 4.13).
        let mut bytes = [0x01u8; REPORT_LEN];
        bytes[..5].copy_from_slice(&[0x25, 0x02, 0x00, 0x02, 0x10]);
        let r = Report::new(bytes);
        assert_eq!(r.payload(), &[0x01; 16]);
    }

    #[test]
    fn declared_length_is_clamped() {
        let r = report(&[0x01, 0x00, 0x00, 0x00, 0xFF]);
        assert_eq!(r.payload().len(), MAX_CHUNK_PAYLOAD);
    }

    #[test]
    fn hid_buffer_with_and_without_report_id() {
        let mut with_id = [0u8; 64];
        with_id[0] = REPORT_ID;
        with_id[1] = 0x0D;
        let a = Report::from_hid_buffer(&with_id).unwrap();
        assert_eq!(a.cmd(), 0x0D);

        let without_id = [0x0Du8; 63];
        let b = Report::from_hid_buffer(&without_id).unwrap();
        assert_eq!(b.cmd(), 0x0D);

        with_id[0] = 0x02;
        assert_eq!(
            Report::from_hid_buffer(&with_id),
            Err(FrameError::WrongReportId { id: 0x02 })
        );
        assert_eq!(
            Report::from_hid_buffer(&[0u8; 10]),
            Err(FrameError::BadLength { len: 10 })
        );
    }

    #[test]
    fn from_hex_capture_format() {
        let text = "21 00 00 00 03 11 01 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00";
        let r = Report::from_hex(text).unwrap();
        assert_eq!(r.payload(), &[0x11, 0x01, 0x01]);
        assert_eq!(r.summary(), "21 00 00 00 03 11 01 01");
        assert!(Report::from_hex("21 00").is_err());
    }

    #[test]
    fn out_packet_hid_report_has_leading_id() {
        let p = OutPacket::from_prefix(&[0x21, 0, 0, 0, 0x18, 0x05, 2, 2]);
        let hid = p.to_hid_report();
        assert_eq!(hid.len(), 64);
        assert_eq!(hid[0], REPORT_ID);
        assert_eq!(&hid[1..9], &[0x21, 0, 0, 0, 0x18, 0x05, 2, 2]);
        assert!(hid[9..].iter().all(|b| *b == 0));
        assert_eq!(format!("{p:?}"), "OutPacket(21 00 00 00 18 05 02 02)");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn report_serde_round_trip() {
        let r = report(&[0x07, 0x00, 0x00, 0x00, 0x0c, 0x0a, 0x04]);
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json.len(), 63 * 2 + 2);
        let back: Report = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }
}
