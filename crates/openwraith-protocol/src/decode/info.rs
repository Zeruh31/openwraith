//! Device info `0x0D` (spec section 4.1).

use super::{FromResponse, expect_request};
use crate::error::DecodeError;
use crate::hex::HexBytes;
use crate::request::ReadRequest;
use crate::response::RawResponse;

/// The device-info string, e.g. `W669,34,KB,ML,W75HEARGB,V3.22.07`, plus the
/// firmware build date from the second chunk.
///
/// The text fields are safe to print: control characters (which could act as
/// terminal escape sequences) are replaced by U+FFFD. The exact bytes are
/// kept in [`raw`](Self::raw).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceInfo {
    /// Chunk 0 as text, cut at the first NUL (CONFIRMED).
    pub info: String,
    /// `info` split on commas. Fields 0-3 (`W669`, `34`, `KB`, `ML`) are
    /// unknown or GUESS.
    pub fields: Vec<String>,
    /// Field 4: product code used to identify the model, e.g. `W75HEARGB` (CONFIRMED).
    pub product: Option<String>,
    /// Field 5 as sent, e.g. `V3.22.07` (CONFIRMED).
    pub firmware_raw: Option<String>,
    /// Field 5 cleaned like the vendor site does: control characters and every
    /// `V` removed, `_` turned into `.`, e.g. `3.22.07`.
    pub firmware: Option<String>,
    /// Chunk 1 as text, e.g. `Aug  6 2026,13:46:54` (build date and time;
    /// CONFIRMED value, ignored by the vendor site). `None` if empty.
    pub build_date: Option<String>,
    /// Declared payloads of both chunks, concatenated.
    pub raw: HexBytes,
}

impl FromResponse for DeviceInfo {
    fn from_response(response: &RawResponse) -> Result<Self, DecodeError> {
        expect_request(response, "DeviceInfo", |r| r == ReadRequest::DeviceInfo)?;
        let reports = response.reports();
        let chunk_text = |i: usize| reports.get(i).map(|r| text_until_nul(r.payload()));
        let text = chunk_text(0).unwrap_or_default();
        // The firmware version is cleaned from the text as sent, like the
        // vendor site does; everything else is made printable.
        let firmware = text.split(',').nth(5).map(clean_firmware);
        let info = printable(&text);
        let build_date = chunk_text(1)
            .map(|s| printable(&s))
            .filter(|s| !s.is_empty());
        let fields: Vec<String> = info.split(',').map(str::to_string).collect();
        let product = fields.get(4).map(|s| s.trim().to_string());
        let firmware_raw = fields.get(5).cloned();
        Ok(DeviceInfo {
            info,
            fields,
            product,
            firmware_raw,
            firmware,
            build_date,
            raw: HexBytes(response.payload()),
        })
    }
}

/// Bytes up to the first NUL, decoded as (lossy) UTF-8.
fn text_until_nul(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(bytes.get(..end).unwrap_or_default()).into_owned()
}

/// `text` with every control character replaced by U+FFFD.
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

/// The vendor site's version cleaning: drop control characters and every `V`,
/// turn `_` into `.`.
fn clean_firmware(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control() && *c != 'V')
        .map(|c| if c == '_' { '.' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::report;

    fn chunk(index: u8, text: &[u8]) -> crate::Report {
        let mut prefix = vec![0x0D, 0x00, 0x00, index, 0x32];
        prefix.extend_from_slice(text);
        report(&prefix)
    }

    #[test]
    fn decodes_w75_string() {
        let raw = RawResponse::from_reports(
            ReadRequest::DeviceInfo,
            [
                chunk(0, b"W669,34,KB,ML,W75HEARGB,V3.22.07"),
                chunk(1, b"Aug  6 2026,13:46:54"),
            ],
        )
        .unwrap();
        let info = DeviceInfo::from_response(&raw).unwrap();
        assert_eq!(info.info, "W669,34,KB,ML,W75HEARGB,V3.22.07");
        assert_eq!(info.fields.len(), 6);
        assert_eq!(info.product.as_deref(), Some("W75HEARGB"));
        assert_eq!(info.firmware_raw.as_deref(), Some("V3.22.07"));
        assert_eq!(info.firmware.as_deref(), Some("3.22.07"));
        assert_eq!(info.build_date.as_deref(), Some("Aug  6 2026,13:46:54"));
        assert_eq!(info.raw.len(), 100);
    }

    #[test]
    fn tolerates_short_strings() {
        let raw = RawResponse::from_reports(
            ReadRequest::DeviceInfo,
            [chunk(0, b"W669,34"), chunk(1, b"")],
        )
        .unwrap();
        let info = DeviceInfo::from_response(&raw).unwrap();
        assert_eq!(info.product, None);
        assert_eq!(info.firmware, None);
        assert_eq!(info.build_date, None);
    }

    #[test]
    fn control_characters_are_made_printable() {
        let raw = RawResponse::from_reports(
            ReadRequest::DeviceInfo,
            [
                chunk(0, b"W669,34,KB,ML,W75HEARGB,V3.22.07\x1b[5m"),
                chunk(1, b"\x1b]0;title\x07\x1b[2J"),
            ],
        )
        .unwrap();
        let info = DeviceInfo::from_response(&raw).unwrap();
        let texts = [
            Some(info.info.as_str()),
            info.product.as_deref(),
            info.firmware_raw.as_deref(),
            info.firmware.as_deref(),
            info.build_date.as_deref(),
        ];
        for text in texts.into_iter().flatten() {
            assert!(!text.chars().any(char::is_control), "{text:?}");
        }
        assert!(info.fields.iter().all(|f| !f.chars().any(char::is_control)));
        assert_eq!(info.product.as_deref(), Some("W75HEARGB"));
        assert_eq!(info.firmware_raw.as_deref(), Some("V3.22.07\u{FFFD}[5m"));
        // The raw bytes are unchanged.
        assert!(info.raw.0.contains(&0x1b));
    }

    #[test]
    fn firmware_cleaning() {
        assert_eq!(clean_firmware("V3_22_07\u{1}"), "3.22.07");
    }

    #[test]
    fn wrong_request_is_rejected() {
        let raw = crate::decode::test_util::single(ReadRequest::PollRate, 0x21, 0, &[0x09, 0x08]);
        assert!(matches!(
            DeviceInfo::from_response(&raw),
            Err(DecodeError::WrongRequest { .. })
        ));
    }
}
