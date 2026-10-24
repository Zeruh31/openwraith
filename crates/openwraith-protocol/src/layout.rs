//! Key matrix, key masks and the W75 key table (spec sections 2.9 and 5).
//!
//! The firmware addresses 132 positions = 6 rows x 22 columns, with
//! **index = row x 22 + col** (CONFIRMED). Every per-key table uses this index:
//! colours (`0x09`), keymaps (`0x18`), switch types (`0x25`), dead band
//! (`0x26`) and the 22-byte key masks.

use std::fmt;

/// Number of matrix rows.
pub const MATRIX_ROWS: u8 = 6;
/// Number of matrix columns.
pub const MATRIX_COLS: u8 = 22;
/// Number of matrix positions (and of entries in every per-key table).
pub const MATRIX_SIZE: usize = MATRIX_ROWS as usize * MATRIX_COLS as usize;
/// Length of a key mask in bytes (one byte per column).
pub const KEY_MASK_LEN: usize = MATRIX_COLS as usize;

/// A validated matrix position (row 0..6, column 0..22).
///
/// ```
/// use openwraith_protocol::KeyPos;
/// let w = KeyPos::new(2, 2).unwrap();
/// assert_eq!(w.index(), 46);
/// assert_eq!(KeyPos::from_index(46), Some(w));
/// assert!(KeyPos::new(6, 0).is_none());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "KeyPosRepr", into = "KeyPosRepr"))]
pub struct KeyPos {
    row: u8,
    col: u8,
}

impl KeyPos {
    /// Creates a position, or `None` if it is outside the 6 x 22 matrix.
    pub const fn new(row: u8, col: u8) -> Option<Self> {
        if row < MATRIX_ROWS && col < MATRIX_COLS {
            Some(KeyPos { row, col })
        } else {
            None
        }
    }

    /// Creates a position from a key index (0..132).
    pub const fn from_index(index: u8) -> Option<Self> {
        Self::new(index / MATRIX_COLS, index % MATRIX_COLS)
    }

    /// Row (0..6).
    pub const fn row(self) -> u8 {
        self.row
    }

    /// Column (0..22).
    pub const fn col(self) -> u8 {
        self.col
    }

    /// Key index = row x 22 + col (0..132).
    pub const fn index(self) -> u8 {
        self.row * MATRIX_COLS + self.col
    }

    /// Byte of a 22-byte key mask that holds this key (= column).
    /// In a `0x21` trigger write the mask starts at report byte 6.
    pub const fn mask_byte(self) -> usize {
        self.col as usize
    }

    /// Bit value of this key inside its mask byte (= `1 << row`).
    pub const fn mask_bit(self) -> u8 {
        1 << self.row
    }

    /// `(chunk, payload offset)` of the R byte in a `0x09` colour table
    /// (54-byte chunks, 3 bytes per key).
    pub const fn color_offset(self) -> (u16, usize) {
        chunk_offset(self.index(), 3, 54)
    }

    /// `(chunk, payload offset)` of byte b0 in a `0x18` keymap table
    /// (56-byte chunks, 4 bytes per key).
    pub const fn keymap_offset(self) -> (u16, usize) {
        chunk_offset(self.index(), 4, 56)
    }

    /// `(chunk, payload offset)` of the switch-type byte in the `0x25` table
    /// (58-byte chunks, 1 byte per key).
    pub const fn switch_offset(self) -> (u16, usize) {
        chunk_offset(self.index(), 1, 58)
    }

    /// `(chunk, payload offset)` of the top byte in the `0x26` dead-band table
    /// (58-byte chunks, 2 bytes per key; bottom = offset + 1).
    pub const fn dead_band_offset(self) -> (u16, usize) {
        chunk_offset(self.index(), 2, 58)
    }

    /// Iterates over all 132 matrix positions in index order.
    pub fn all() -> impl Iterator<Item = KeyPos> {
        (0..MATRIX_SIZE as u8).filter_map(KeyPos::from_index)
    }
}

const fn chunk_offset(index: u8, entry: usize, chunk_len: usize) -> (u16, usize) {
    let flat = index as usize * entry;
    ((flat / chunk_len) as u16, flat % chunk_len)
}

impl fmt::Display for KeyPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "r{}c{}", self.row, self.col)
    }
}

/// Serde representation of [`KeyPos`], validated on deserialization.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize)]
struct KeyPosRepr {
    row: u8,
    col: u8,
}

#[cfg(feature = "serde")]
impl TryFrom<KeyPosRepr> for KeyPos {
    type Error = String;

    fn try_from(repr: KeyPosRepr) -> Result<Self, Self::Error> {
        KeyPos::new(repr.row, repr.col).ok_or_else(|| {
            format!(
                "key position r{}c{} is outside the 6x22 matrix",
                repr.row, repr.col
            )
        })
    }
}

#[cfg(feature = "serde")]
impl From<KeyPos> for KeyPosRepr {
    fn from(pos: KeyPos) -> Self {
        KeyPosRepr {
            row: pos.row,
            col: pos.col,
        }
    }
}

/// A 22-byte key mask: byte j = column j, bit b = row b (spec section 2.9).
///
/// Orientation is CODE-ONLY but consistent with every capture (the `0x0E`
/// reply sets bit 5 of byte 11, which is only possible with byte = column).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct KeyMask(pub [u8; KEY_MASK_LEN]);

impl KeyMask {
    /// Builds a mask from the first 22 bytes of `bytes` (missing bytes are 0).
    pub fn from_slice(bytes: &[u8]) -> Self {
        let mut mask = [0u8; KEY_MASK_LEN];
        for (dst, src) in mask.iter_mut().zip(bytes) {
            *dst = *src;
        }
        KeyMask(mask)
    }

    /// Builds a mask with the given positions set.
    pub fn from_positions(positions: impl IntoIterator<Item = KeyPos>) -> Self {
        let mut mask = KeyMask::default();
        for pos in positions {
            mask.insert(pos);
        }
        mask
    }

    /// Sets the bit for `pos`.
    pub fn insert(&mut self, pos: KeyPos) {
        if let Some(byte) = self.0.get_mut(pos.mask_byte()) {
            *byte |= pos.mask_bit();
        }
    }

    /// Whether the bit for `pos` is set.
    pub fn contains(&self, pos: KeyPos) -> bool {
        self.0
            .get(pos.mask_byte())
            .is_some_and(|byte| byte & pos.mask_bit() != 0)
    }

    /// All positions whose bit is set, in index order. Bits 6-7 are ignored.
    pub fn positions(&self) -> Vec<KeyPos> {
        KeyPos::all().filter(|pos| self.contains(*pos)).collect()
    }

    /// Whether no bit is set.
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|b| *b == 0)
    }

    /// The raw 22 bytes.
    pub fn as_bytes(&self) -> &[u8; KEY_MASK_LEN] {
        &self.0
    }
}

impl fmt::Debug for KeyMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeyMask({})", crate::hex::to_spaced_hex(&self.0))
    }
}

/// One physical key of a model's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// Key index (row x 22 + col).
    pub index: u8,
    /// Matrix row.
    pub row: u8,
    /// Matrix column.
    pub col: u8,
    /// Short label of the key legend (US layout).
    pub name: &'static str,
    /// Standard USB HID usage (keyboard page 0x07) of the legend; `None` for
    /// keys without one (the knob and Fn).
    pub hid_usage: Option<u8>,
}

impl Key {
    const fn new(row: u8, col: u8, name: &'static str, hid_usage: Option<u8>) -> Self {
        Key {
            index: row * MATRIX_COLS + col,
            row,
            col,
            name,
            hid_usage,
        }
    }

    /// The key's matrix position.
    pub fn pos(&self) -> Option<KeyPos> {
        KeyPos::new(self.row, self.col)
    }
}

const fn k(row: u8, col: u8, name: &'static str, hid: u8) -> Key {
    Key::new(row, col, name, Some(hid))
}

/// The 82 key positions of the W75 with their legends and HID usages
/// (spec section 5; positions, names and HID codes CONFIRMED).
///
/// Index 15 is the rotary knob and index 121 is Fn; neither has a HID usage.
pub static W75_KEYS: [Key; 82] = [
    // Row 0
    k(0, 0, "Esc", 0x29),
    k(0, 2, "F1", 0x3A),
    k(0, 3, "F2", 0x3B),
    k(0, 4, "F3", 0x3C),
    k(0, 5, "F4", 0x3D),
    k(0, 6, "F5", 0x3E),
    k(0, 7, "F6", 0x3F),
    k(0, 8, "F7", 0x40),
    k(0, 9, "F8", 0x41),
    k(0, 10, "F9", 0x42),
    k(0, 11, "F10", 0x43),
    k(0, 12, "F11", 0x44),
    k(0, 13, "F12", 0x45),
    k(0, 14, "Del", 0x4C),
    Key::new(0, 15, "Knob", None),
    // Row 1
    k(1, 0, "`", 0x35),
    k(1, 1, "1", 0x1E),
    k(1, 2, "2", 0x1F),
    k(1, 3, "3", 0x20),
    k(1, 4, "4", 0x21),
    k(1, 5, "5", 0x22),
    k(1, 6, "6", 0x23),
    k(1, 7, "7", 0x24),
    k(1, 8, "8", 0x25),
    k(1, 9, "9", 0x26),
    k(1, 10, "0", 0x27),
    k(1, 11, "-", 0x2D),
    k(1, 12, "=", 0x2E),
    k(1, 14, "Backspace", 0x2A),
    k(1, 15, "Home", 0x4A),
    // Row 2
    k(2, 0, "Tab", 0x2B),
    k(2, 1, "Q", 0x14),
    k(2, 2, "W", 0x1A),
    k(2, 3, "E", 0x08),
    k(2, 4, "R", 0x15),
    k(2, 5, "T", 0x17),
    k(2, 6, "Y", 0x1C),
    k(2, 7, "U", 0x18),
    k(2, 8, "I", 0x0C),
    k(2, 9, "O", 0x12),
    k(2, 10, "P", 0x13),
    k(2, 11, "[", 0x2F),
    k(2, 12, "]", 0x30),
    k(2, 14, "\\", 0x31),
    k(2, 15, "PgUp", 0x4B),
    // Row 3
    k(3, 0, "Caps Lock", 0x39),
    k(3, 2, "A", 0x04),
    k(3, 3, "S", 0x16),
    k(3, 4, "D", 0x07),
    k(3, 5, "F", 0x09),
    k(3, 6, "G", 0x0A),
    k(3, 7, "H", 0x0B),
    k(3, 8, "J", 0x0D),
    k(3, 9, "K", 0x0E),
    k(3, 10, "L", 0x0F),
    k(3, 11, ";", 0x33),
    k(3, 12, "'", 0x34),
    k(3, 14, "Enter", 0x28),
    k(3, 15, "PgDn", 0x4E),
    // Row 4
    k(4, 0, "L-Shift", 0xE1),
    k(4, 2, "Z", 0x1D),
    k(4, 3, "X", 0x1B),
    k(4, 4, "C", 0x06),
    k(4, 5, "V", 0x19),
    k(4, 6, "B", 0x05),
    k(4, 7, "N", 0x11),
    k(4, 8, "M", 0x10),
    k(4, 9, ",", 0x36),
    k(4, 10, ".", 0x37),
    k(4, 11, "/", 0x38),
    k(4, 13, "R-Shift", 0xE5),
    k(4, 14, "Up", 0x52),
    // Row 5
    k(5, 0, "L-Ctrl", 0xE0),
    k(5, 1, "L-Win", 0xE3),
    k(5, 2, "L-Alt", 0xE2),
    k(5, 6, "Space", 0x2C),
    k(5, 10, "R-Alt", 0xE6),
    Key::new(5, 11, "Fn", None),
    k(5, 12, "R-Ctrl", 0xE4),
    k(5, 13, "Left", 0x50),
    k(5, 14, "Down", 0x51),
    k(5, 15, "Right", 0x4F),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn index_formula() {
        for pos in KeyPos::all() {
            assert_eq!(
                usize::from(pos.index()),
                usize::from(pos.row()) * 22 + usize::from(pos.col())
            );
            assert_eq!(KeyPos::from_index(pos.index()), Some(pos));
        }
        assert_eq!(KeyPos::all().count(), MATRIX_SIZE);
        assert!(KeyPos::from_index(132).is_none());
        assert!(KeyPos::new(0, 22).is_none());
    }

    #[test]
    fn w75_table_is_consistent() {
        let indices: BTreeSet<u8> = W75_KEYS.iter().map(|k| k.index).collect();
        assert_eq!(indices.len(), 82, "indices are unique");
        for key in &W75_KEYS {
            let pos = key.pos().expect("inside the matrix");
            assert_eq!(pos.index(), key.index);
        }
        // Table is sorted by index.
        assert!(W75_KEYS.windows(2).all(|w| w[0].index < w[1].index));
        let without_hid: Vec<&str> = W75_KEYS
            .iter()
            .filter(|k| k.hid_usage.is_none())
            .map(|k| k.name)
            .collect();
        assert_eq!(without_hid, ["Knob", "Fn"]);
    }

    #[test]
    fn w75_masks_match_spec() {
        // Spec section 2.9: all 82 positions, and the 81 selectable keys.
        let all = KeyMask::from_positions(W75_KEYS.iter().filter_map(Key::pos));
        assert_eq!(
            all.0,
            [
                0x3f, 0x26, 0x3f, 0x1f, 0x1f, 0x1f, 0x3f, 0x1f, 0x1f, 0x1f, 0x3f, 0x3f, 0x2f, 0x31,
                0x3f, 0x2f, 0, 0, 0, 0, 0, 0
            ]
        );
        let selectable = KeyMask::from_positions(
            W75_KEYS
                .iter()
                .filter(|k| k.name != "Knob")
                .filter_map(Key::pos),
        );
        assert_eq!(selectable.0[15], 0x2e);
        assert_eq!(all.positions().len(), 82);
    }

    #[test]
    fn spec_offsets() {
        // Spot checks against the section 5 table.
        let pos = |i| KeyPos::from_index(i).unwrap();
        // F1
        assert_eq!(pos(2).color_offset(), (0, 6));
        assert_eq!(pos(2).keymap_offset(), (0, 8));
        assert_eq!(pos(2).switch_offset(), (0, 2));
        assert_eq!(pos(2).dead_band_offset(), (0, 4));
        // W
        assert_eq!(pos(46).color_offset(), (2, 30));
        assert_eq!(pos(46).keymap_offset(), (3, 16));
        assert_eq!(pos(46).switch_offset(), (0, 46));
        assert_eq!(pos(46).dead_band_offset(), (1, 34));
        assert_eq!(pos(46).mask_byte(), 2);
        assert_eq!(pos(46).mask_bit(), 0x04);
        // Backslash
        assert_eq!(pos(58).switch_offset(), (1, 0));
        assert_eq!(pos(58).dead_band_offset(), (2, 0));
        // Right
        assert_eq!(pos(125).color_offset(), (6, 51));
        assert_eq!(pos(125).keymap_offset(), (8, 52));
        assert_eq!(pos(125).switch_offset(), (2, 9));
        assert_eq!(pos(125).dead_band_offset(), (4, 18));
    }

    #[test]
    fn fn_locked_mask_orientation() {
        // 0x0E reply on the W75: byte 11 = 0x20 -> index 121 (row 5, col 11).
        let mut bytes = [0u8; 22];
        bytes[11] = 0x20;
        let mask = KeyMask(bytes);
        let positions = mask.positions();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].index(), 121);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn key_pos_serde_validates() {
        let pos = KeyPos::new(2, 2).unwrap();
        let json = serde_json::to_string(&pos).unwrap();
        assert_eq!(json, r#"{"row":2,"col":2}"#);
        assert_eq!(serde_json::from_str::<KeyPos>(&json).unwrap(), pos);
        assert!(serde_json::from_str::<KeyPos>(r#"{"row":9,"col":2}"#).is_err());
    }
}
