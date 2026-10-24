//! Supported keyboard models and USB identifiers (spec section 2.1).
//!
//! A model is pure data, so adding another keyboard of the family means adding
//! a key table and a [`Model`] value to [`MODELS`].

use crate::layout::{Key, KeyMask, KeyPos, W75_KEYS};

/// USB vendor ID of the Wraith keyboards (CONFIRMED).
pub const VENDOR_ID: u16 = 0x2E3C;

/// Product IDs shared by W60, W65, W75 and W100 (`0xC368` CONFIRMED, others CODE-ONLY).
pub const PRODUCT_IDS: [u16; 4] = [0xC365, 0xC366, 0xC367, 0xC368];

/// HID usage page of the vendor configuration collection (CONFIRMED).
pub const CONFIG_USAGE_PAGE: u16 = 0xFF1B;

/// HID usage of the vendor configuration collection (CONFIRMED).
pub const CONFIG_USAGE: u16 = 0x0091;

/// Static description of a keyboard model.
///
/// The model is identified by field 4 of the `0x0D` device-info string
/// ([`DeviceInfo::product`](crate::DeviceInfo::product)), **not** by the USB
/// product ID (CONFIRMED).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Model {
    /// Human-readable name, e.g. `"Wraith W75"`.
    pub name: &'static str,
    /// Product code reported in field 4 of the `0x0D` string, e.g. `"W75HEARGB"`.
    pub product_code: &'static str,
    /// USB vendor ID.
    pub vendor_id: u16,
    /// USB product IDs the model may enumerate with.
    pub product_ids: &'static [u16],
    /// Physical keys, sorted by index.
    pub keys: &'static [Key],
    /// Index of the rotary knob, if the model has one.
    pub knob_index: Option<u8>,
    /// Index of the Fn key, if known.
    pub fn_index: Option<u8>,
}

/// The Wraith W75 (75 % layout, 82 positions, rotary knob).
pub static W75: Model = Model {
    name: "Wraith W75",
    product_code: "W75HEARGB",
    vendor_id: VENDOR_ID,
    product_ids: &PRODUCT_IDS,
    keys: &W75_KEYS,
    // INFERRED (strong): the only read position that Select All never includes.
    knob_index: Some(15),
    // GUESS (strong): the only bit of the 0x0E Fn-locked mask.
    fn_index: Some(121),
};

/// Every model this crate knows.
pub static MODELS: [&Model; 1] = [&W75];

/// Looks up a model by the product code of the `0x0D` string (exact match).
pub fn model_for_product(product_code: &str) -> Option<&'static Model> {
    MODELS
        .iter()
        .copied()
        .find(|m| m.product_code == product_code)
}

impl Model {
    /// Whether `pid` is one of the model's USB product IDs.
    pub fn supports_pid(&self, pid: u16) -> bool {
        self.product_ids.contains(&pid)
    }

    /// The key at `index`, if the layout has one.
    pub fn key(&self, index: u8) -> Option<&'static Key> {
        self.keys.iter().find(|k| k.index == index)
    }

    /// The key at `pos`, if the layout has one.
    pub fn key_at(&self, pos: KeyPos) -> Option<&'static Key> {
        self.key(pos.index())
    }

    /// Finds a key by name, ignoring ASCII case (e.g. `"w"`, `"caps lock"`).
    pub fn key_by_name(&self, name: &str) -> Option<&'static Key> {
        self.keys
            .iter()
            .find(|k| k.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Positions of every key of the layout, in index order.
    pub fn positions(&self) -> impl Iterator<Item = KeyPos> + 'static {
        self.keys.iter().filter_map(Key::pos)
    }

    /// Mask with every position of the layout (knob included).
    pub fn layout_mask(&self) -> KeyMask {
        KeyMask::from_positions(self.positions())
    }

    /// Mask of the keys a user can select for trigger edits (knob excluded).
    pub fn selectable_mask(&self) -> KeyMask {
        let knob = self.knob_index;
        KeyMask::from_positions(self.positions().filter(|p| Some(p.index()) != knob))
    }

    /// Display name of the key at `index`, or `"#<index>"` for positions
    /// outside the layout.
    pub fn key_name(&self, index: u8) -> String {
        match self.key(index) {
            Some(key) => key.name.to_string(),
            None => format!("#{index}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w75_description() {
        assert_eq!(W75.product_code, "W75HEARGB");
        assert_eq!(W75.keys.len(), 82);
        assert_eq!(W75.key(15).map(|k| k.name), Some("Knob"));
        assert_eq!(W75.key(121).map(|k| k.name), Some("Fn"));
        assert_eq!(W75.key(46).map(|k| k.name), Some("W"));
        assert!(W75.key(16).is_none());
        assert!(W75.supports_pid(0xC368));
        assert!(!W75.supports_pid(0xC370));
        assert_eq!(W75.key_by_name("caps lock").map(|k| k.index), Some(66));
        assert_eq!(W75.key_name(103), "#103");
        assert_eq!(model_for_product("W75HEARGB"), Some(&W75));
        assert!(model_for_product("AP61HEARGB").is_none());
    }

    #[test]
    fn selectable_mask_excludes_knob() {
        assert_eq!(W75.layout_mask().positions().len(), 82);
        assert_eq!(W75.selectable_mask().positions().len(), 81);
        assert_eq!(W75.selectable_mask().0[15], 0x2e);
    }
}
