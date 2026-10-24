//! Hex helpers and the [`HexBytes`] wrapper used for lossless raw data.

use std::fmt;
use std::ops::Deref;

use crate::error::FrameError;

/// Raw bytes that serialize as a compact lowercase hex string.
///
/// Every decoded model keeps the bytes it was decoded from in a `HexBytes`
/// field, so a backup can always be re-decoded or restored byte for byte.
/// Deserialization accepts hex with or without whitespace between bytes.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct HexBytes(pub Vec<u8>);

impl HexBytes {
    /// Parses hex text such as `"0d 00 32"` or `"0d0032"`.
    pub fn from_hex(text: &str) -> Result<Self, FrameError> {
        parse_hex(text).map(HexBytes)
    }

    /// Lowercase hex without separators, e.g. `"0d0032"`.
    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }

    /// Lowercase hex with a space between bytes, e.g. `"0d 00 32"`.
    pub fn to_spaced_hex(&self) -> String {
        to_spaced_hex(&self.0)
    }
}

impl Deref for HexBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for HexBytes {
    fn from(bytes: Vec<u8>) -> Self {
        HexBytes(bytes)
    }
}

impl From<&[u8]> for HexBytes {
    fn from(bytes: &[u8]) -> Self {
        HexBytes(bytes.to_vec())
    }
}

impl fmt::Debug for HexBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HexBytes({})", self.to_spaced_hex())
    }
}

impl fmt::Display for HexBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_spaced_hex())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for HexBytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for HexBytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        HexBytes::from_hex(&text).map_err(serde::de::Error::custom)
    }
}

/// Lowercase hex without separators.
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        push_byte(&mut out, *byte);
    }
    out
}

/// Lowercase hex with single spaces between bytes.
pub(crate) fn to_spaced_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        push_byte(&mut out, *byte);
    }
    out
}

fn push_byte(out: &mut String, byte: u8) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    out.push(char::from(DIGITS[usize::from(byte >> 4)]));
    out.push(char::from(DIGITS[usize::from(byte & 0x0F)]));
}

/// Parses hex text; ASCII whitespace between digits is ignored.
pub(crate) fn parse_hex(text: &str) -> Result<Vec<u8>, FrameError> {
    let mut out = Vec::with_capacity(text.len() / 2);
    let mut high: Option<u8> = None;
    for ch in text.chars() {
        if ch.is_ascii_whitespace() {
            continue;
        }
        let nibble = ch
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
            .ok_or_else(|| FrameError::BadHex(format!("unexpected character {ch:?}")))?;
        match high.take() {
            Some(h) => out.push((h << 4) | nibble),
            None => high = Some(nibble),
        }
    }
    if high.is_some() {
        return Err(FrameError::BadHex("odd number of hex digits".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let bytes = vec![0x0d, 0x00, 0xff, 0x5a];
        assert_eq!(to_hex(&bytes), "0d00ff5a");
        assert_eq!(to_spaced_hex(&bytes), "0d 00 ff 5a");
        assert_eq!(parse_hex("0d 00 FF 5a").unwrap(), bytes);
        assert_eq!(parse_hex("0d00ff5a").unwrap(), bytes);
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(parse_hex("0").is_err());
        assert!(parse_hex("zz").is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_as_string() {
        let value = HexBytes(vec![1, 2, 0xab]);
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, "\"0102ab\"");
        let back: HexBytes = serde_json::from_str("\"01 02 ab\"").unwrap();
        assert_eq!(back, value);
    }
}
