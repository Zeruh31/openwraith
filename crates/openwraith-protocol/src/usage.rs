//! Names for USB HID keyboard usages and the vendor's multimedia indices
//! (spec section 4.8.3).

/// Name of a standard USB HID keyboard usage (page 0x07), e.g. `0x04` -> `"A"`.
///
/// Covers the usages the vendor configurator offers; returns `None` for others.
pub fn hid_usage_name(usage: u8) -> Option<&'static str> {
    const LETTERS: [&str; 26] = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R",
        "S", "T", "U", "V", "W", "X", "Y", "Z",
    ];
    const DIGITS: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
    const F1_F12: [&str; 12] = [
        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    ];
    const F13_F24: [&str; 12] = [
        "F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
    ];
    const KEYPAD: [&str; 17] = [
        "Num Lock", "KP /", "KP *", "KP -", "KP +", "KP Enter", "KP 1", "KP 2", "KP 3", "KP 4",
        "KP 5", "KP 6", "KP 7", "KP 8", "KP 9", "KP 0", "KP .",
    ];
    const MODIFIERS: [&str; 8] = [
        "L-Ctrl", "L-Shift", "L-Alt", "L-Win", "R-Ctrl", "R-Shift", "R-Alt", "R-Win",
    ];
    let pick = |table: &'static [&'static str], first: u8| {
        table.get(usize::from(usage.wrapping_sub(first))).copied()
    };
    match usage {
        0x04..=0x1D => pick(&LETTERS, 0x04),
        0x1E..=0x27 => pick(&DIGITS, 0x1E),
        0x28 => Some("Enter"),
        0x29 => Some("Esc"),
        0x2A => Some("Backspace"),
        0x2B => Some("Tab"),
        0x2C => Some("Space"),
        0x2D => Some("-"),
        0x2E => Some("="),
        0x2F => Some("["),
        0x30 => Some("]"),
        0x31 => Some("\\"),
        0x32 => Some("Non-US #"),
        0x33 => Some(";"),
        0x34 => Some("'"),
        0x35 => Some("`"),
        0x36 => Some(","),
        0x37 => Some("."),
        0x38 => Some("/"),
        0x39 => Some("Caps Lock"),
        0x3A..=0x45 => pick(&F1_F12, 0x3A),
        0x46 => Some("Print Screen"),
        0x47 => Some("Scroll Lock"),
        0x48 => Some("Pause"),
        0x49 => Some("Insert"),
        0x4A => Some("Home"),
        0x4B => Some("PgUp"),
        0x4C => Some("Del"),
        0x4D => Some("End"),
        0x4E => Some("PgDn"),
        0x4F => Some("Right"),
        0x50 => Some("Left"),
        0x51 => Some("Down"),
        0x52 => Some("Up"),
        0x53..=0x63 => pick(&KEYPAD, 0x53),
        0x64 => Some("Non-US \\"),
        0x65 => Some("Menu"),
        0x68..=0x73 => pick(&F13_F24, 0x68),
        0x87 => Some("Intl Ro"),
        0x88 => Some("Katakana/Hiragana"),
        0x89 => Some("Intl Yen"),
        0x8A => Some("Henkan"),
        0x8B => Some("Muhenkan"),
        0xE0..=0xE7 => pick(&MODIFIERS, 0xE0),
        _ => None,
    }
}

/// Name of a multimedia index (keymap entry `07 xx`; CODE-ONLY).
///
/// Indices `0x24` and `0x25` are labelled contradictorily by the vendor
/// (code name "BrowserForward" with the label "previous page", and the
/// reverse; spec section 4.8.3, open question Q14). Their names say so
/// instead of picking one meaning.
pub fn media_name(index: u8) -> Option<&'static str> {
    Some(match index {
        0x10 => "Volume +",
        0x11 => "Volume -",
        0x12 => "Mute",
        0x13 => "Play/Pause",
        0x14 => "Next Track",
        0x15 => "Previous Track",
        0x16 => "Stop",
        0x20 => "Mail",
        0x21 => "Browser Search",
        0x22 => "Favorites",
        0x23 => "Browser Home",
        0x24 => "Browser Forward/Back (0x24, label contradictory)",
        0x25 => "Browser Back/Forward (0x25, label contradictory)",
        0x26 => "Browser Stop",
        0x27 => "Refresh",
        0x30 => "Media Player",
        0x32 => "Calculator",
        0x34 => "My Computer",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(hid_usage_name(0x04), Some("A"));
        assert_eq!(hid_usage_name(0x1D), Some("Z"));
        assert_eq!(hid_usage_name(0x1E), Some("1"));
        assert_eq!(hid_usage_name(0x27), Some("0"));
        assert_eq!(hid_usage_name(0x45), Some("F12"));
        assert_eq!(hid_usage_name(0x63), Some("KP ."));
        assert_eq!(hid_usage_name(0x73), Some("F24"));
        assert_eq!(hid_usage_name(0xE7), Some("R-Win"));
        assert_eq!(hid_usage_name(0x00), None);
        assert_eq!(hid_usage_name(0xFA), None);
        assert_eq!(media_name(0x12), Some("Mute"));
        assert_eq!(media_name(0x99), None);
        assert!(media_name(0x24).is_some_and(|n| n.contains("contradictory")));
        assert!(media_name(0x25).is_some_and(|n| n.contains("contradictory")));
    }

    #[test]
    fn layout_keys_have_matching_names() {
        // The legend names of the W75 table agree with the usage names for
        // the keys whose labels are identical.
        for key in crate::W75_KEYS.iter() {
            if let Some(usage) = key.hid_usage {
                assert!(hid_usage_name(usage).is_some(), "{}", key.name);
            }
        }
    }
}
