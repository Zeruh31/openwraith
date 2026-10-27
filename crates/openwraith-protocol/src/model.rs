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
    /// Absolute ceilings for the values OpenWraith writes to this model.
    pub write_ceilings: WriteCeilings,
    /// The firmware's built-in Fn-layer functions, sorted by key index
    /// (spec section 4.8.4). An Fn-layer entry at one of these keys hides
    /// the built-in function.
    pub fn_builtins: &'static [FnBuiltin],
}

/// A built-in function of the Fn layer: what Fn + key does while the
/// keymap's Fn-layer entry for the key is `00 00 00 00` (spec section
/// 4.8.4).
///
/// The labels come from the vendor site (`fnKeys.json`) and are GUESS,
/// except where the table of the model says CONFIRMED.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FnBuiltin {
    /// Key index (row x 22 + col).
    pub index: u8,
    /// Short English description, e.g. `"profile 2"`.
    pub function: &'static str,
    /// Whether replacing it can lock the user out of a keyboard function
    /// that has no other way back on the keyboard (profile switch, reset).
    /// Such an override needs a separate acknowledgement.
    pub critical: bool,
}

/// Absolute value ceilings for writes to one model, compiled into this
/// crate.
///
/// The write builders intersect the limits the keyboard reports (`21/04`,
/// `0A`) with these ceilings, and always take them from the model in
/// [`MODELS`] with the same product code (never from a [`Model`] the caller
/// built). So neither fabricated limits nor a forged reply can produce a value
/// above them. Distances are trigger units of 0.02 mm, the only unit
/// OpenWraith writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteCeilings {
    /// Smallest actuation point, in trigger units.
    pub min_actuation: u16,
    /// Largest actuation point, in trigger units.
    pub max_actuation: u16,
    /// Largest Rapid Trigger sensitivity, in trigger units.
    pub max_sensitivity: u16,
    /// Largest dead-band value (top or bottom), in trigger units.
    pub max_dead_band: u16,
    /// Largest main-light brightness.
    pub max_light_brightness: u8,
    /// Largest main-light speed.
    pub max_light_speed: u8,
    /// Main-light mode IDs that may ever be written (never `0x64`).
    pub light_modes: &'static [u8],
    /// Largest DKS press point, in **0.1 mm** units (the deep press point
    /// of spec section 4.14).
    pub dks_bottom: u8,
    /// Smallest Mod-Tap hold threshold, in 10 ms units.
    pub mt_threshold_min: u8,
    /// Largest Mod-Tap hold threshold, in 10 ms units.
    pub mt_threshold_max: u8,
    /// Largest number of steps of a macro.
    pub macro_max_steps: u8,
    /// Largest number of SOCD pairs.
    pub socd_max_pairs: u8,
}

/// The W75's write ceilings: actuation 0.10-3.40 mm (5..=170 units), Rapid
/// Trigger sensitivity up to 1.70 mm (85), dead band up to 1.00 mm (50),
/// brightness and speed up to 4, and the light modes of its `0A` list plus
/// Custom (`0x0A`). These are the values the keyboard reports (`21/04`, `0A`;
/// spec sections 4.7.1 and 4.11.5).
///
/// The vendor site's UI limits: DKS deep press point 3.0 mm (30 x 0.1 mm =
/// max travel x 10 - 4, `wmIndex.js:4161-4170`), Mod-Tap hold threshold
/// 150-1000 ms (`wmIndex.js:2205-2213`), 20 press/release groups = 40 macro
/// steps (`wmIndex.js:2994-2999`) and 20 SOCD pairs (`wmIndex.js:2404,
/// 2418`).
pub const W75_WRITE_CEILINGS: WriteCeilings = WriteCeilings {
    min_actuation: 5,
    max_actuation: 170,
    max_sensitivity: 85,
    max_dead_band: 50,
    max_light_brightness: 4,
    max_light_speed: 4,
    light_modes: &[
        0x00, 0x01, 0x02, 0x03, 0x04, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0E, 0x0F, 0x10,
    ],
    dks_bottom: 30,
    mt_threshold_min: 15,
    mt_threshold_max: 100,
    macro_max_steps: 40,
    socd_max_pairs: 20,
};

/// Shorthand for the [`W75_FN_BUILTINS`] table.
const fn builtin(index: u8, function: &'static str, critical: bool) -> FnBuiltin {
    FnBuiltin {
        index,
        function,
        critical,
    }
}

/// The W75's built-in Fn functions (spec section 4.8.4). Labels GUESS,
/// except R-Shift (SOCD on/off, CONFIRMED by c15). Fn + 1..4 (profiles) and
/// Fn + Space (reset) are critical.
pub static W75_FN_BUILTINS: [FnBuiltin; 31] = [
    builtin(2, "media player", false),
    builtin(3, "volume -", false),
    builtin(4, "volume +", false),
    builtin(5, "mute", false),
    builtin(6, "stop", false),
    builtin(7, "previous", false),
    builtin(8, "play/pause", false),
    builtin(9, "next", false),
    builtin(10, "mail", false),
    builtin(11, "browser home", false),
    builtin(12, "calculator", false),
    builtin(13, "browser search", false),
    builtin(14, "insert", false),
    builtin(23, "profile 1", true),
    builtin(24, "profile 2", true),
    builtin(25, "profile 3", true),
    builtin(26, "profile 4", true),
    builtin(33, "speed -", false),
    builtin(34, "speed +", false),
    builtin(36, "all lights off", false),
    builtin(37, "end", false),
    builtin(58, "all lighting", false),
    builtin(68, "Windows mode", false),
    builtin(69, "Mac mode", false),
    builtin(101, "SOCD on/off", false),
    builtin(102, "brightness +", false),
    builtin(111, "Win-key lock", false),
    builtin(116, "reset", true),
    builtin(123, "effect direction", false),
    builtin(124, "brightness -", false),
    builtin(125, "foreground colour", false),
];

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
    write_ceilings: W75_WRITE_CEILINGS,
    fn_builtins: &W75_FN_BUILTINS,
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

/// The model of [`MODELS`] with the product code of `model`: write
/// builders use it instead of the caller's [`Model`], whose fields are public
/// and could be fabricated.
pub(crate) fn known_model(model: &Model) -> Option<&'static Model> {
    model_for_product(model.product_code)
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

    /// The built-in Fn function at key `index`, if there is one.
    pub fn fn_builtin(&self, index: u8) -> Option<&'static FnBuiltin> {
        self.fn_builtins.iter().find(|b| b.index == index)
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

    #[test]
    fn known_models_come_from_the_static_table() {
        let fake = Model {
            knob_index: None,
            ..W75
        };
        // The fabricated copy resolves to the real W75 (knob at index 15).
        assert!(std::ptr::eq(known_model(&fake).unwrap(), &W75));
        let unknown = Model {
            product_code: "AP61HEARGB",
            ..W75
        };
        assert!(known_model(&unknown).is_none());
        assert!(!W75.write_ceilings.light_modes.contains(&0x64));
    }

    #[test]
    fn fn_builtins_sit_on_layout_keys() {
        assert_eq!(W75.fn_builtins.len(), 31);
        assert!(W75.fn_builtins.windows(2).all(|w| w[0].index < w[1].index));
        for b in W75.fn_builtins {
            let key = W75.key(b.index).expect("a layout key");
            assert!(key.hid_usage.is_some(), "{}", key.name);
        }
        let critical: Vec<&str> = W75
            .fn_builtins
            .iter()
            .filter(|b| b.critical)
            .map(|b| W75.key(b.index).map_or("?", |k| k.name))
            .collect();
        assert_eq!(critical, ["1", "2", "3", "4", "Space"]);
        assert_eq!(W75.fn_builtin(101).map(|b| b.function), Some("SOCD on/off"));
        assert_eq!(W75.fn_builtin(46), None);
    }
}
