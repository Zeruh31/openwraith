//! Value parsers of the v0.2 editors: key assignments (`KEY=ACTION`), HID
//! usages by name, macro steps, DKS rows, SOCD pairs, key colours, 0.1 mm
//! press points and durations.
//!
//! Names are matched as `openwraith show` prints them (case, spaces, `-` and
//! `_` ignored; the key aliases such as `caps` or `equal`). Values the vendor
//! configurator does not offer are refused here already: the protocol's
//! validated atoms ([`HidUsage`], [`MediaKey`], [`Combo`], [`MacroProgram`])
//! decide, so that a typo never reaches the keyboard.

use std::time::Duration;

use openwraith_protocol::write::{DksRow, DksStep, MacroStep};
use openwraith_protocol::{
    AdvancedSlotId, Combo, HidUsage, KeyAssignment, MacroPlay, MacroProgram, MacroSlotId, MediaKey,
    MouseAction, MouseButton, Rgb, SocdMode,
};

use crate::args::{KeySpec, KeyUse, normalise, normalise_alias, parse_color, parse_key_as};

/// One `KEY=ACTION` of `set key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyAssign {
    /// The key.
    pub key: KeySpec,
    /// What it does.
    pub action: KeyAssignment,
}

/// One `KEY=RRGGBB` of `set colors`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyColor {
    /// The key.
    pub key: KeySpec,
    /// Its colour.
    pub color: Rgb,
}

/// The steps of `set macro --steps`, checked like a macro program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroSteps(pub Vec<MacroStep>);

/// One `--row KEY:STAGES` of a DKS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DksRowSpec(pub DksRow);

/// One `--pair KEY1/KEY2[:MODE]` of `set socd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocdPairSpec {
    /// Key 1.
    pub key1: HidUsage,
    /// Key 2.
    pub key2: HidUsage,
    /// The mode, if given.
    pub mode: Option<SocdMode>,
}

impl SocdPairSpec {
    /// Whether this names the same two keys as `(key1, key2)`, in either
    /// order.
    pub fn same_keys(&self, key1: HidUsage, key2: HidUsage) -> bool {
        (self.key1 == key1 && self.key2 == key2) || (self.key1 == key2 && self.key2 == key1)
    }
}

/// Short forms of usage names that are not key names of the layout.
const USAGE_ALIASES: &[(&str, &str)] = &[
    ("ctrl", "L-Ctrl"),
    ("control", "L-Ctrl"),
    ("shift", "L-Shift"),
    ("alt", "L-Alt"),
    ("win", "L-Win"),
    ("gui", "L-Win"),
    ("super", "L-Win"),
    ("altgr", "R-Alt"),
    ("prtsc", "Print Screen"),
    ("printscreen", "Print Screen"),
    ("ins", "Insert"),
    ("scrolllock", "Scroll Lock"),
    ("numlock", "Num Lock"),
    ("pgup", "PgUp"),
    ("pgdn", "PgDn"),
    ("spacebar", "Space"),
    ("bksp", "Backspace"),
];

/// Multimedia names: the vendor's labels (`media_name`) plus short forms.
/// `24` and `25` are given by number only (their label is contradictory,
/// spec 4.8.3).
const MEDIA_ALIASES: &[(&str, u8)] = &[
    ("volumeup", 0x10),
    ("volup", 0x10),
    ("volumedown", 0x11),
    ("voldown", 0x11),
    ("mute", 0x12),
    ("playpause", 0x13),
    ("play", 0x13),
    ("next", 0x14),
    ("nexttrack", 0x14),
    ("previous", 0x15),
    ("prev", 0x15),
    ("previoustrack", 0x15),
    ("stop", 0x16),
    ("mail", 0x20),
    ("search", 0x21),
    ("browsersearch", 0x21),
    ("favorites", 0x22),
    ("favourites", 0x22),
    ("home", 0x23),
    ("browserhome", 0x23),
    ("browserstop", 0x26),
    ("refresh", 0x27),
    ("mediaplayer", 0x30),
    ("player", 0x30),
    ("calculator", 0x32),
    ("calc", 0x32),
    ("mycomputer", 0x34),
    ("computer", 0x34),
];

/// A byte written as `0xNN` (or `NN` hex with the prefix only).
fn parse_hex_byte(text: &str) -> Option<u8> {
    let hex = text
        .trim()
        .strip_prefix("0x")
        .or_else(|| text.trim().strip_prefix("0X"))?;
    u8::from_str_radix(hex, 16).ok()
}

/// A HID usage the vendor configurator offers, by name (`A`, `L-Ctrl`,
/// `"KP 1"`, `F13`, `caps`) or as `0xNN`.
pub fn parse_usage(text: &str) -> Result<HidUsage, String> {
    let text = text.trim();
    if let Some(byte) = parse_hex_byte(text) {
        return HidUsage::new(byte).ok_or_else(|| {
            format!("HID usage {byte:#04x} is not one the vendor configurator offers")
        });
    }
    let wanted = normalise_alias(text);
    let wanted = USAGE_ALIASES
        .iter()
        .find(|(alias, _)| *alias == wanted)
        .map_or(wanted, |(_, real)| normalise(real));
    HidUsage::all()
        .find(|u| u.name().is_some_and(|name| normalise(name) == wanted))
        .ok_or_else(|| {
            format!(
                "unknown key {text:?}: use a key name as `openwraith show triggers` prints it \
                 (e.g. A, \"Caps Lock\", L-Ctrl, F13, \"KP 1\") or a HID usage as 0xNN"
            )
        })
}

/// A multimedia key by name or `0xNN`.
fn parse_media(text: &str) -> Result<MediaKey, String> {
    if let Some(byte) = parse_hex_byte(text) {
        return MediaKey::new(byte).ok_or_else(|| {
            format!("multimedia index {byte:#04x} is not one the vendor configurator offers")
        });
    }
    let wanted = normalise(text);
    MEDIA_ALIASES
        .iter()
        .find(|(alias, _)| *alias == wanted)
        .map(|(_, index)| *index)
        .or_else(|| {
            MediaKey::ALL.into_iter().find(|index| {
                openwraith_protocol::usage::media_name(*index)
                    .is_some_and(|name| normalise(name) == wanted)
            })
        })
        .and_then(MediaKey::new)
        .ok_or_else(|| {
            format!(
                "unknown multimedia key {text:?}: one of mute, volume-up, volume-down, \
                 play-pause, next, previous, stop, mail, search, favorites, browser-home, \
                 browser-stop, refresh, media-player, calculator, my-computer, or 0x24 and 0x25 \
                 (browser back and forward in some order; which is which is not verified, Q14)"
            )
        })
}

/// A mouse action: `left`, `right`, `middle`, `0x08`, `0x10`, `wheel-up`,
/// `wheel-down`.
fn parse_mouse(text: &str) -> Result<MouseAction, String> {
    if let Some(byte) = parse_hex_byte(text) {
        return MouseButton::from_mask(byte)
            .map(MouseAction::Button)
            .ok_or_else(|| format!("mouse button mask {byte:#04x} is not offered"));
    }
    match normalise(text).as_str() {
        "left" => Ok(MouseAction::Button(MouseButton::Left)),
        "right" => Ok(MouseAction::Button(MouseButton::Right)),
        "middle" => Ok(MouseAction::Button(MouseButton::Middle)),
        "wheelup" => Ok(MouseAction::WheelUp),
        "wheeldown" => Ok(MouseAction::WheelDown),
        _ => Err(format!(
            "unknown mouse action {text:?}: one of left, right, middle, 0x08, 0x10 (the vendor \
             site's forward and back; their direction is not verified), wheel-up, wheel-down"
        )),
    }
}

/// `L-Ctrl+L-Shift+A`: one or two modifiers, then one key.
fn parse_combo(text: &str) -> Result<Combo, String> {
    // "KP +" contains the separator: an empty part belongs to the one before.
    let mut parts: Vec<String> = Vec::new();
    for part in text.split('+') {
        match parts.last_mut() {
            Some(last) if part.trim().is_empty() => last.push('+'),
            _ => parts.push(part.to_string()),
        }
    }
    let usages = parts
        .iter()
        .map(|p| parse_usage(p))
        .collect::<Result<Vec<_>, _>>()?;
    let Some((key, modifiers)) = usages.split_last() else {
        return Err("a combination needs modifiers and a key, e.g. combo:L-Ctrl+C".to_string());
    };
    Combo::new(modifiers, *key).map_err(|e| format!("{text:?}: {e}"))
}

/// What `set key` sets a key to (the part after `=`).
pub fn parse_action(text: &str) -> Result<KeyAssignment, String> {
    let text = text.trim();
    let (kind, value) = match text.split_once(':') {
        Some((kind, value)) => (normalise(kind), value),
        None => (String::new(), text),
    };
    match kind.as_str() {
        "" if normalise(text) == "default" => Ok(KeyAssignment::Default),
        "" | "key" => parse_usage(value).map(KeyAssignment::Key),
        "usage" => parse_hex_byte(value)
            .ok_or_else(|| format!("{value:?} is not a HID usage; write it as 0xNN"))
            .and_then(|byte| {
                HidUsage::new(byte).ok_or_else(|| {
                    format!("HID usage {byte:#04x} is not one the vendor configurator offers")
                })
            })
            .map(KeyAssignment::Key),
        "media" => parse_media(value).map(KeyAssignment::Media),
        "mouse" => parse_mouse(value).map(KeyAssignment::Mouse),
        "combo" => parse_combo(value).map(KeyAssignment::Combo),
        "macro" => value
            .trim()
            .parse::<u8>()
            .ok()
            .and_then(MacroSlotId::new)
            .map(KeyAssignment::Macro)
            .ok_or_else(|| format!("{value:?} is not a macro slot (0-9)")),
        "advanced" => value
            .trim()
            .parse::<u8>()
            .ok()
            .and_then(AdvancedSlotId::new)
            .map(KeyAssignment::Advanced)
            .ok_or_else(|| format!("{value:?} is not an advanced-key slot (0-39)")),
        _ => Err(format!(
            "unknown action {text:?}: use default, a key name, usage:0xNN, media:NAME, \
             mouse:NAME, combo:MOD+KEY, macro:N or advanced:N"
        )),
    }
}

/// Splits `KEY=VALUE`; `==VALUE` is the `=` key.
fn split_key_value(text: &str) -> Result<(&str, &str), String> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("==") {
        return Ok(("=", rest));
    }
    match text.split_once('=') {
        Some((key, value)) if !key.trim().is_empty() && !value.trim().is_empty() => {
            Ok((key, value))
        }
        _ => Err(format!("{text:?} is not KEY=VALUE")),
    }
}

/// `KEY=ACTION` of `set key` (the key: neither the knob nor Fn).
pub fn parse_key_assignment(text: &str) -> Result<KeyAssign, String> {
    let (key, action) = split_key_value(text)?;
    Ok(KeyAssign {
        key: parse_key_as(key, KeyUse::Keymap)?,
        action: parse_action(action)?,
    })
}

/// `KEY=RRGGBB` of `set colors`.
pub fn parse_key_color(text: &str) -> Result<KeyColor, String> {
    let (key, color) = split_key_value(text)?;
    Ok(KeyColor {
        key: parse_key_as(key, KeyUse::Color)?,
        color: parse_color(color)?,
    })
}

/// `once`, `times:N` (1-255), `toggle`, `while-held`.
pub fn parse_play(text: &str) -> Result<MacroPlay, String> {
    let wanted = normalise(text);
    if let Some(count) = wanted.strip_prefix("times:") {
        return count
            .parse::<u8>()
            .ok()
            .filter(|n| *n > 0)
            .map(MacroPlay::Times)
            .ok_or_else(|| format!("{text:?}: the count must be 1-255"));
    }
    match wanted.as_str() {
        "once" => Ok(MacroPlay::Once),
        "toggle" => Ok(MacroPlay::Toggle),
        "whileheld" | "hold" => Ok(MacroPlay::WhileHeld),
        _ => Err(format!(
            "unknown play mode {text:?}: once, times:N, toggle or while-held"
        )),
    }
}

/// A delay in ms: `100`, `100ms`.
fn parse_delay(text: &str) -> Option<u16> {
    let text = text.trim();
    let digits = text.strip_suffix("ms").unwrap_or(text).trim_end();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// One macro step: `KEY+` / `KEY-`, optionally followed by the delay after
/// it (`A+ 100`, `A+100`, `"Caps Lock"- 30ms`). The last character before
/// the delay is the motion, so `L-Ctrl-` and `--` (the minus key released)
/// work.
fn parse_step(text: &str) -> Result<MacroStep, String> {
    let token = text.trim();
    let motion_of = |head: &str| -> Option<(String, bool)> {
        let head = head.trim_end();
        let motion = head.chars().last()?;
        let key = head[..head.len() - motion.len_utf8()].trim();
        if key.is_empty() {
            return None;
        }
        match motion {
            '+' => Some((key.to_string(), true)),
            '-' => Some((key.to_string(), false)),
            _ => None,
        }
    };
    // "<key><motion> <delay>", "<key><motion>", "<key><motion><delay>".
    let split = token
        .rsplit_once(char::is_whitespace)
        .and_then(|(head, tail)| Some((motion_of(head)?, parse_delay(tail)?)))
        .or_else(|| motion_of(token).map(|m| (m, 0)))
        .or_else(|| {
            let at = token.rfind(['+', '-'])?;
            let delay = parse_delay(&token[at + 1..])?;
            Some((motion_of(&token[..=at])?, delay))
        });
    let Some(((key, press), delay)) = split else {
        return Err(format!(
            "{token:?} is not a macro step: write KEY+ (press) or KEY- (release), then the \
             delay in ms, e.g. \"A+ 100\""
        ));
    };
    let usage = parse_usage(&key)?;
    Ok(if press {
        MacroStep::press(usage, delay)
    } else {
        MacroStep::release(usage, delay)
    })
}

/// `"A+ 100, A- 100, B+ 100, B-"`, checked like a macro program (balanced,
/// at most 40 steps, no delay after the last step).
pub fn parse_macro_steps(text: &str) -> Result<MacroSteps, String> {
    let steps = text
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(parse_step)
        .collect::<Result<Vec<_>, _>>()?;
    MacroProgram::new(MacroPlay::Once, steps.clone()).map_err(|e| e.to_string())?;
    Ok(MacroSteps(steps))
}

/// `KEY:STAGES` of a DKS row: 4 characters for the stages (press past the
/// point, bottom-out, lift from the bottom, release past the point): `.`
/// nothing, `t` tap, `h` hold to the next stage, `r` release and press
/// again (after a hold).
pub fn parse_dks_row(text: &str) -> Result<DksRowSpec, String> {
    let Some((key, stages)) = text.trim().rsplit_once(':') else {
        return Err(format!("{text:?} is not KEY:STAGES, e.g. E:t..."));
    };
    let key = parse_usage(key)?;
    let chars: Vec<char> = stages.trim().chars().collect();
    if chars.len() != 4 {
        return Err(format!(
            "{stages:?}: give exactly 4 stages (press, bottom, lift, release), e.g. t..."
        ));
    }
    let mut steps = [DksStep::None; 4];
    for (step, c) in steps.iter_mut().zip(&chars) {
        *step = match c.to_ascii_lowercase() {
            '.' | '-' => DksStep::None,
            't' => DksStep::Tap,
            'h' => DksStep::Hold,
            'r' => DksStep::HoldAgain,
            other => {
                return Err(format!(
                    "{other:?} is not a stage: . nothing, t tap, h hold, r release and press again"
                ));
            }
        };
    }
    Ok(DksRowSpec(DksRow { key, steps }))
}

/// `KEY1/KEY2[:MODE]` of `set socd` (MODE 1-4). Keys by name; the `/` key
/// is written `slash`.
pub fn parse_socd_pair(text: &str) -> Result<SocdPairSpec, String> {
    let text = text.trim();
    let (keys, mode) = match text.rsplit_once(':') {
        Some((keys, mode)) => {
            let number: u8 = mode
                .trim()
                .parse()
                .map_err(|_| format!("{mode:?} is not a SOCD mode (1-4)"))?;
            let mode = number
                .checked_sub(1)
                .and_then(SocdMode::from_wire)
                .ok_or_else(|| format!("{number} is not a SOCD mode (1-4)"))?;
            (keys, Some(mode))
        }
        None => (text, None),
    };
    let Some((key1, key2)) = keys.split_once('/') else {
        return Err(format!("{text:?} is not KEY1/KEY2, e.g. A/D"));
    };
    Ok(SocdPairSpec {
        key1: parse_usage(key1)?,
        key2: parse_usage(key2)?,
        mode,
    })
}

/// Millimetres in steps of 0.1 mm (a DKS press point), as tenths.
pub fn parse_tenths(text: &str) -> Result<u8, String> {
    let um = crate::args::parse_micrometres(text)?;
    if um % 100 != 0 {
        return Err(format!(
            "{} mm is not a multiple of 0.1 mm (the DKS press point's step)",
            text.trim()
        ));
    }
    u8::try_from(um / 100).map_err(|_| format!("{text:?} is too large"))
}

/// A duration: `300ms`, `1.5s`, `30s`, `2m`; a bare number is in
/// `default_unit_ms` units. Whole milliseconds, no floats.
fn parse_duration(text: &str, default_unit_ms: u64) -> Result<Duration, String> {
    let original = text;
    let text = text.trim();
    let (number, unit_ms) = if let Some(n) = text.strip_suffix("ms") {
        (n, 1)
    } else if let Some(n) = text.strip_suffix('s') {
        (n, 1000)
    } else if let Some(n) = text.strip_suffix("min") {
        (n, 60_000)
    } else if let Some(n) = text.strip_suffix('m') {
        (n, 60_000)
    } else {
        (text, default_unit_ms)
    };
    let number = number.trim();
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) || fraction.len() > 3 {
        return Err(format!(
            "{original:?} is not a duration (e.g. 300ms, 30s, 2m)"
        ));
    }
    let whole: u64 = whole
        .parse()
        .ok()
        .filter(|w| *w <= 100_000)
        .ok_or_else(|| format!("{original:?} is too long"))?;
    let fraction_thousandths: u64 = format!("{fraction:0<3}").parse().unwrap_or(0);
    let ms = whole * unit_ms + fraction_thousandths * unit_ms / 1000;
    Ok(Duration::from_millis(ms))
}

/// `--duration` of `travel` and `monitor`: a bare number is in seconds; at
/// least 1 s, at most 10 minutes.
pub fn parse_seconds(text: &str) -> Result<Duration, String> {
    let duration = parse_duration(text, 1000)?;
    if duration < Duration::from_secs(1) || duration > Duration::from_secs(600) {
        return Err(format!("{text:?}: give 1 s to 10 minutes"));
    }
    Ok(duration)
}

/// `--time` of a Mod-Tap: a bare number is in ms; a multiple of 10 ms, as
/// 10 ms units (the range, 150-1000 ms, is checked against the keyboard).
pub fn parse_hold_time(text: &str) -> Result<u8, String> {
    let ms = parse_duration(text, 1)?.as_millis();
    if ms % 10 != 0 {
        return Err(format!("{text:?} is not a multiple of 10 ms"));
    }
    u8::try_from(ms / 10).map_err(|_| format!("{text:?} is too long (at most 1000 ms)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(u: u8) -> HidUsage {
        HidUsage::new(u).expect("offered usage")
    }

    #[test]
    fn usages_by_name_alias_and_hex() {
        for (text, expected) in [
            ("A", 0x04),
            ("a", 0x04),
            ("L-Ctrl", 0xE0),
            ("lctrl", 0xE0),
            ("ctrl", 0xE0),
            ("Caps Lock", 0x39),
            ("caps", 0x39),
            ("F13", 0x68),
            ("KP 1", 0x59),
            ("-", 0x2D),
            ("equal", 0x2E),
            ("0x04", 0x04),
            ("Print Screen", 0x46),
            ("prtsc", 0x46),
        ] {
            assert_eq!(parse_usage(text), Ok(usage(expected)), "{text}");
        }
        // Not offered by the vendor site (0x32), no usage (Fn), unknown.
        for bad in ["0x32", "0x00", "Fn", "Knob", "NoSuchKey"] {
            assert!(parse_usage(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn actions_of_every_category() {
        assert_eq!(parse_action("default"), Ok(KeyAssignment::Default));
        assert_eq!(parse_action("L-Ctrl"), Ok(KeyAssignment::Key(usage(0xE0))));
        assert_eq!(
            parse_action("usage:0x68"),
            Ok(KeyAssignment::Key(usage(0x68)))
        );
        let media = |i| KeyAssignment::Media(MediaKey::new(i).expect("media"));
        assert_eq!(parse_action("media:mute"), Ok(media(0x12)));
        assert_eq!(parse_action("media:Volume +"), Ok(media(0x10)));
        assert_eq!(parse_action("media:calculator"), Ok(media(0x32)));
        assert_eq!(parse_action("media:0x24"), Ok(media(0x24)));
        assert!(parse_action("media:0x99").is_err());
        assert_eq!(
            parse_action("mouse:wheel-down"),
            Ok(KeyAssignment::Mouse(MouseAction::WheelDown))
        );
        assert_eq!(
            parse_action("mouse:0x10"),
            Ok(KeyAssignment::Mouse(MouseAction::Button(
                MouseButton::Mask10
            )))
        );
        assert!(parse_action("mouse:0x20").is_err());
        let combo = parse_action("combo:L-Ctrl+L-Shift+A").expect("combo");
        let KeyAssignment::Combo(combo) = combo else {
            panic!("not a combo");
        };
        // c17: `13 e0 e1 04`.
        assert_eq!(combo.bytes(), [0x13, 0xE0, 0xE1, 0x04]);
        assert_eq!(
            parse_action("combo:ctrl+KP +").map(|a| match a {
                KeyAssignment::Combo(c) => c.bytes(),
                _ => [0; 4],
            }),
            Ok([0x13, 0xE0, 0x57, 0x00])
        );
        // The site's rule: 1-2 modifiers, then one key that is not one.
        for bad in [
            "combo:A",
            "combo:A+B",
            "combo:L-Ctrl+L-Shift",
            "combo:L-Ctrl+L-Shift+L-Alt+A",
        ] {
            assert!(parse_action(bad).is_err(), "{bad}");
        }
        assert_eq!(
            parse_action("macro:3"),
            Ok(KeyAssignment::Macro(MacroSlotId::new(3).expect("slot")))
        );
        assert!(parse_action("macro:10").is_err());
        assert_eq!(
            parse_action("advanced:39"),
            Ok(KeyAssignment::Advanced(
                AdvancedSlotId::new(39).expect("slot")
            ))
        );
        assert!(parse_action("advanced:40").is_err());
        assert!(parse_action("lighting:2").is_err());
    }

    #[test]
    fn key_value_pairs_and_the_equal_key() {
        let a = parse_key_assignment("Caps=L-Ctrl").expect("pair");
        assert_eq!(a.key, KeySpec::Name("Caps".into()));
        assert_eq!(a.action, KeyAssignment::Key(usage(0xE0)));
        let eq = parse_key_assignment("==A").expect("the = key");
        assert_eq!(eq.key, KeySpec::Name("=".into()));
        let to_eq = parse_key_assignment("A==").expect("to the = key");
        assert_eq!(to_eq.action, KeyAssignment::Key(usage(0x2E)));
        // (An index or a position is checked against the model later.)
        for bad in ["Caps", "=A", "Caps=", "Knob=A", "Fn=A", "Caps=Fn"] {
            assert!(parse_key_assignment(bad).is_err(), "{bad}");
        }
        let c = parse_key_color("Up=0000ff").expect("colour");
        assert_eq!(c.color, Rgb::new(0, 0, 0xff));
        assert!(parse_key_color("Knob=0000ff").is_err());
        assert!(parse_key_color("Up=blue").is_err());
    }

    #[test]
    fn macro_steps_in_every_notation() {
        let (a, b) = (usage(0x04), usage(0x05));
        // c13: A down 100 ms, A up 100 ms, B down 100 ms, B up.
        assert_eq!(
            parse_macro_steps("A+ 100, A- 100, B+ 100, B-"),
            Ok(MacroSteps(vec![
                MacroStep::press(a, 100),
                MacroStep::release(a, 100),
                MacroStep::press(b, 100),
                MacroStep::release(b, 0),
            ]))
        );
        let minus = usage(0x2D);
        let ctrl = usage(0xE0);
        let caps = usage(0x39);
        assert_eq!(
            parse_macro_steps("L-Ctrl+100ms, -+, --, L-Ctrl-"),
            Ok(MacroSteps(vec![
                MacroStep::press(ctrl, 100),
                MacroStep::press(minus, 0),
                MacroStep::release(minus, 0),
                MacroStep::release(ctrl, 0),
            ]))
        );
        assert_eq!(
            parse_macro_steps("Caps Lock+ 30, Caps Lock-"),
            Ok(MacroSteps(vec![
                MacroStep::press(caps, 30),
                MacroStep::release(caps, 0),
            ]))
        );
        // Unbalanced, a delay after the last step, too long, nonsense.
        let too_long = vec!["A+, A-"; 21].join(", ");
        for bad in [
            "A+",
            "A-",
            "A+, A+, A-",
            "A+ 100, A- 100",
            "A",
            "A* 100",
            "",
            too_long.as_str(),
        ] {
            assert!(parse_macro_steps(bad).is_err(), "{bad:?}");
        }
        assert_eq!(parse_play("once"), Ok(MacroPlay::Once));
        assert_eq!(parse_play("times:3"), Ok(MacroPlay::Times(3)));
        assert_eq!(parse_play("while-held"), Ok(MacroPlay::WhileHeld));
        assert!(parse_play("times:0").is_err());
        assert!(parse_play("times:256").is_err());
    }

    #[test]
    fn dks_rows_socd_pairs_points_and_durations() {
        assert_eq!(
            parse_dks_row("E:t..."),
            Ok(DksRowSpec(DksRow {
                key: usage(0x08),
                steps: [DksStep::Tap, DksStep::None, DksStep::None, DksStep::None],
            }))
        );
        assert_eq!(
            parse_dks_row("R:.thr").map(|r| r.0.steps),
            Ok([
                DksStep::None,
                DksStep::Tap,
                DksStep::Hold,
                DksStep::HoldAgain
            ])
        );
        for bad in ["E", "E:t..", "E:t....", "E:x...", "Fn:t..."] {
            assert!(parse_dks_row(bad).is_err(), "{bad}");
        }
        assert_eq!(
            parse_socd_pair("A/D"),
            Ok(SocdPairSpec {
                key1: usage(0x04),
                key2: usage(0x07),
                mode: None,
            })
        );
        assert_eq!(
            parse_socd_pair("Left/Right:4").map(|p| p.mode),
            Ok(Some(SocdMode::M4))
        );
        assert_eq!(
            parse_socd_pair("A/D:2").map(|p| p.mode),
            Ok(Some(SocdMode::M2))
        );
        for bad in ["A", "A/D:0", "A/D:5", "A/Fn"] {
            assert!(parse_socd_pair(bad).is_err(), "{bad}");
        }
        assert!(
            parse_socd_pair("D/A")
                .expect("pair")
                .same_keys(usage(0x04), usage(0x07))
        );
        assert_eq!(parse_tenths("1.5"), Ok(15));
        assert_eq!(parse_tenths("0.5"), Ok(5));
        assert_eq!(parse_tenths("3"), Ok(30));
        assert!(parse_tenths("1.55").is_err());
        assert_eq!(parse_seconds("30s"), Ok(Duration::from_secs(30)));
        assert_eq!(parse_seconds("30"), Ok(Duration::from_secs(30)));
        assert_eq!(parse_seconds("2m"), Ok(Duration::from_secs(120)));
        assert_eq!(parse_seconds("1.5s"), Ok(Duration::from_millis(1500)));
        for bad in ["0s", "11m", "abc", "-1s", "1.2345s"] {
            assert!(parse_seconds(bad).is_err(), "{bad}");
        }
        assert_eq!(parse_hold_time("300ms"), Ok(30));
        assert_eq!(parse_hold_time("300"), Ok(30));
        assert_eq!(parse_hold_time("0.3s"), Ok(30));
        assert!(parse_hold_time("305ms").is_err());
        assert!(parse_hold_time("5s").is_err());
    }
}
