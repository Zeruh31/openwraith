//! Arguments of the commands that write (`profile select`, `set`, `restore`)
//! and their value parsers: key names, millimetres, Rapid Trigger values,
//! colours and light modes.
//!
//! Millimetres are parsed as decimal text into micrometres (never through a
//! float) and must be an exact multiple of the trigger unit (0.02 mm).

use std::path::PathBuf;

use clap::{ArgGroup, Args, Subcommand};
use openwraith_hid::RtEdit;
use openwraith_protocol::{Distance, KeyPos, LightMode, MODELS, Model, Rgb};

/// `openwraith profile ...`.
#[derive(Debug, Subcommand)]
pub enum ProfileAction {
    /// Switch to onboard profile N (WRITES: a profile switch, nothing else).
    Select {
        /// The profile (1-4).
        #[arg(value_parser = clap::value_parser!(u8).range(1..=4))]
        profile: u8,
    },
}

/// `openwraith set ...`.
#[derive(Debug, Subcommand)]
pub enum SetCommand {
    /// Actuation point and Rapid Trigger of keys of the active profile (WRITES).
    Trigger(SetTriggerArgs),
    /// Main light of the active profile: effect, colour, brightness, speed (WRITES).
    Light(SetLightArgs),
}

/// `openwraith set trigger KEY... [--actuation MM] [--rapid-trigger P[/R] | --no-rapid-trigger]`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("change")
        .required(true)
        .multiple(true)
        .args(["actuation", "rapid_trigger", "no_rapid_trigger"])
))]
pub struct SetTriggerArgs {
    /// Keys: names as `openwraith show triggers` prints them (case-insensitive,
    /// e.g. W, "Caps Lock", L-Shift), key indices (#46) or matrix positions
    /// (r2c2). The knob has no trigger settings.
    #[arg(required = true, value_name = "KEY", value_parser = parse_key)]
    pub keys: Vec<KeySpec>,

    /// New actuation point in millimetres (e.g. 1.20; a multiple of 0.02).
    #[arg(long, value_name = "MM", value_parser = parse_distance)]
    pub actuation: Option<Distance>,

    /// Turn Rapid Trigger on with this sensitivity in millimetres, or with
    /// separate press / release sensitivities (e.g. 0.30 or 0.30/0.50).
    #[arg(
        long,
        value_name = "P[/R]",
        value_parser = parse_rapid_trigger,
        conflicts_with = "no_rapid_trigger"
    )]
    pub rapid_trigger: Option<RtEdit>,

    /// Turn Rapid Trigger off (the sensitivities are kept).
    #[arg(long)]
    pub no_rapid_trigger: bool,

    /// Do not ask for confirmation.
    #[arg(short, long)]
    pub yes: bool,
}

impl SetTriggerArgs {
    /// The Rapid Trigger change, if any.
    pub fn rapid_trigger_edit(&self) -> Option<RtEdit> {
        if self.no_rapid_trigger {
            Some(RtEdit::Off)
        } else {
            self.rapid_trigger
        }
    }
}

/// `openwraith set light [--mode NAME] [--color RRGGBB] [--brightness N] [--speed N]`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("change")
        .required(true)
        .multiple(true)
        .args(["mode", "color", "brightness", "speed"])
))]
pub struct SetLightArgs {
    /// Effect: static, breath, wave, neon, radar, reactive, aurora, ripple,
    /// twinkle, cross, speed-respond, auto-ripple, striation, fireworks (as
    /// far as the keyboard lists them) or custom (the per-key colour table).
    #[arg(long, value_name = "NAME", value_parser = parse_light_mode)]
    pub mode: Option<LightMode>,

    /// Foreground colour as hex RRGGBB (e.g. 0000ff).
    #[arg(long, value_name = "RRGGBB", value_parser = parse_color)]
    pub color: Option<Rgb>,

    /// Brightness (0 to the keyboard's maximum, 4 on the W75).
    #[arg(long, value_name = "N")]
    pub brightness: Option<u8>,

    /// Effect speed (0 to the keyboard's maximum, 4 on the W75).
    #[arg(long, value_name = "N")]
    pub speed: Option<u8>,

    /// Do not ask for confirmation.
    #[arg(short, long)]
    pub yes: bool,
}

/// `openwraith restore FILE ...`.
#[derive(Debug, Args)]
pub struct RestoreArgs {
    /// The backup to restore (*.owbackup.json).
    #[arg(value_name = "BACKUP_FILE")]
    pub file: PathBuf,

    /// Restore only this onboard profile (repeatable) [default: every profile in the backup].
    #[arg(long = "profile", value_name = "N", value_parser = clap::value_parser!(u8).range(1..=4))]
    pub profiles: Vec<u8>,

    /// Only compare and show the plan; never writes. Reads the active profile
    /// of the keyboard, or another backup with --against.
    #[arg(long)]
    pub dry_run: bool,

    /// With --dry-run: compare with this backup file instead of the keyboard
    /// (no keyboard needed).
    #[arg(long, value_name = "BACKUP_FILE", requires = "dry_run")]
    pub against: Option<PathBuf>,

    /// Do not ask for confirmation.
    #[arg(short, long, conflicts_with = "dry_run")]
    pub yes: bool,

    /// Restore even if the keyboard runs other firmware than the backup was
    /// made with (for example after a firmware update). Check the plan.
    #[arg(long)]
    pub allow_firmware_mismatch: bool,

    /// Also print the hex of every packet the plan would send.
    #[arg(long)]
    pub packets: bool,

    /// Print the plan and the result as JSON (messages and the question go to stderr).
    #[arg(long)]
    pub json: bool,
}

/// A key as given on the command line, resolved against the keyboard's
/// model once it is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySpec {
    /// A key name (case, spaces, `-` and `_` ignored).
    Name(String),
    /// A key index (`#46`).
    Index(u8),
    /// A matrix position (`r2c2`).
    Pos(KeyPos),
}

impl std::fmt::Display for KeySpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeySpec::Name(name) => f.write_str(name),
            KeySpec::Index(index) => write!(f, "#{index}"),
            KeySpec::Pos(pos) => write!(f, "{pos}"),
        }
    }
}

/// Short forms accepted for some key names.
const KEY_ALIASES: &[(&str, &str)] = &[
    ("caps", "Caps Lock"),
    ("capslock", "Caps Lock"),
    ("escape", "Esc"),
    ("delete", "Del"),
    ("grave", "`"),
    ("backtick", "`"),
    ("minus", "-"),
    ("equal", "="),
    ("equals", "="),
    ("lbracket", "["),
    ("rbracket", "]"),
    ("backslash", "\\"),
    ("semicolon", ";"),
    ("quote", "'"),
    ("apostrophe", "'"),
    ("comma", ","),
    ("period", "."),
    ("dot", "."),
    ("slash", "/"),
    ("pageup", "PgUp"),
    ("pagedown", "PgDn"),
    ("return", "Enter"),
];

/// Lowercase without spaces, `-` and `_` (`"L-Shift"` -> `"lshift"`), but a
/// name made only of those characters (`"-"`) is kept.
fn normalise(name: &str) -> String {
    let trimmed = name.trim();
    let squeezed: String = trimmed
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect();
    if squeezed.is_empty() {
        trimmed.to_string()
    } else {
        squeezed
    }
}

impl KeySpec {
    /// The key of `model` this spec names.
    pub fn resolve(&self, model: &Model) -> Result<KeyPos, String> {
        let pos = match self {
            KeySpec::Name(name) => find_by_name(model, name)
                .and_then(|k| k.pos())
                .ok_or_else(|| format!("{} has no key named {name:?}", model.name))?,
            KeySpec::Index(index) => model
                .key(*index)
                .and_then(|k| k.pos())
                .ok_or_else(|| format!("{} has no key with index #{index}", model.name))?,
            KeySpec::Pos(pos) => {
                model
                    .key_at(*pos)
                    .ok_or_else(|| format!("{} has no key at {pos}", model.name))?;
                *pos
            }
        };
        if Some(pos.index()) == model.knob_index {
            return Err(format!(
                "{} (the knob) has no trigger settings that can be changed",
                model.key_name(pos.index())
            ));
        }
        Ok(pos)
    }
}

/// A key of `model` by name, ignoring case, spaces, `-` and `_`, or by alias.
fn find_by_name(model: &Model, name: &str) -> Option<&'static openwraith_protocol::Key> {
    let wanted = normalise(name);
    let wanted = KEY_ALIASES
        .iter()
        .find(|(alias, _)| *alias == wanted)
        .map_or(wanted.clone(), |(_, real)| normalise(real));
    model.keys.iter().find(|k| normalise(k.name) == wanted)
}

/// Parses a key: `#46`, `r2c2` or a key name of a known model.
pub fn parse_key(text: &str) -> Result<KeySpec, String> {
    let text = text.trim();
    if let Some(index) = text.strip_prefix('#') {
        let index: u8 = index
            .parse()
            .map_err(|_| format!("{text:?} is not a key index (#0 to #131)"))?;
        return Ok(KeySpec::Index(index));
    }
    if let Some(pos) = parse_position(text) {
        return Ok(KeySpec::Pos(pos));
    }
    let mut known = false;
    for model in MODELS {
        if let Some(key) = find_by_name(model, text) {
            known = true;
            // The knob is refused already here.
            if Some(key.index) == model.knob_index {
                return Err(format!(
                    "{} (the knob) has no trigger settings that can be changed",
                    key.name
                ));
            }
        }
    }
    if known {
        return Ok(KeySpec::Name(text.to_string()));
    }
    Err(format!(
        "unknown key {text:?}: use a name as `openwraith show triggers` prints it (e.g. W, \
         \"Caps Lock\", L-Shift), an index (#46) or a position (r2c2)"
    ))
}

/// `r<row>c<col>` (case-insensitive).
fn parse_position(text: &str) -> Option<KeyPos> {
    let lower = text.to_ascii_lowercase();
    let rest = lower.strip_prefix('r')?;
    let (row, col) = rest.split_once('c')?;
    if row.is_empty() || col.is_empty() {
        return None;
    }
    KeyPos::new(row.parse().ok()?, col.parse().ok()?)
}

/// Parses millimetres (`1.2`, `1.20`, `0.3 mm`) into micrometres, without
/// floating point. At most three decimals.
pub fn parse_micrometres(text: &str) -> Result<u32, String> {
    let original = text;
    let text = text.trim();
    let text = text.strip_suffix("mm").unwrap_or(text).trim_end();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !digits(whole) || !digits(fraction) {
        return Err(format!(
            "{original:?} is not a distance in millimetres (e.g. 1.20)"
        ));
    }
    if fraction.len() > 3 {
        return Err(format!(
            "{original:?} has more decimals than a micrometre resolution allows"
        ));
    }
    let whole: u32 = if whole.is_empty() {
        0
    } else {
        whole
            .parse()
            .ok()
            .filter(|mm| *mm <= 1000)
            .ok_or_else(|| format!("{original:?} is too large"))?
    };
    let fraction_um: u32 = format!("{fraction:0<3}")
        .parse()
        .map_err(|_| format!("{original:?} is not a distance in millimetres"))?;
    Ok(whole * 1000 + fraction_um)
}

/// Parses millimetres into a [`Distance`] (an exact multiple of 0.02 mm).
pub fn parse_distance(text: &str) -> Result<Distance, String> {
    let um = parse_micrometres(text)?;
    let unit = Distance::MICROMETRES_PER_UNIT;
    if um % unit != 0 {
        return Err(format!(
            "{} mm is not a multiple of the keyboard's unit (0.02 mm)",
            text.trim()
        ));
    }
    u16::try_from(um / unit)
        .map(Distance::from_units)
        .map_err(|_| format!("{text:?} is too large"))
}

/// Parses `P` (Rapid Trigger, one sensitivity) or `P/R` (separate press and
/// release sensitivities).
pub fn parse_rapid_trigger(text: &str) -> Result<RtEdit, String> {
    match text.split_once('/') {
        None => Ok(RtEdit::Same {
            sensitivity: parse_distance(text)?,
        }),
        Some((press, release)) => Ok(RtEdit::Separate {
            press: parse_distance(press)?,
            release: parse_distance(release)?,
        }),
    }
}

/// Parses `RRGGBB` or `#RRGGBB`.
pub fn parse_color(text: &str) -> Result<Rgb, String> {
    Rgb::from_hex(text.trim())
        .ok_or_else(|| format!("{text:?} is not a colour; use hex RRGGBB, e.g. 0000ff"))
}

/// The modes `set light --mode` accepts: every named mode except music
/// rhythm (never written). Whether the keyboard lists a mode is checked
/// against the keyboard.
fn settable_modes() -> impl Iterator<Item = LightMode> {
    (0u8..=0x10)
        .map(LightMode::from_id)
        .filter(|m| !matches!(m, LightMode::Unknown(_)))
}

/// The command-line name of a mode (`speed-respond`, `custom`).
pub fn mode_arg_name(mode: LightMode) -> String {
    match mode {
        LightMode::Custom => "custom".to_string(),
        other => other.name().to_ascii_lowercase().replace(' ', "-"),
    }
}

/// Parses a light mode name.
pub fn parse_light_mode(text: &str) -> Result<LightMode, String> {
    let wanted = normalise(text);
    settable_modes()
        .find(|m| normalise(&mode_arg_name(*m)) == wanted || normalise(&m.name()) == wanted)
        .ok_or_else(|| {
            let names: Vec<String> = settable_modes().map(mode_arg_name).collect();
            format!("unknown light mode {text:?}; one of: {}", names.join(", "))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwraith_protocol::W75;

    #[test]
    fn keys_by_name_index_and_position() {
        let w = KeyPos::new(2, 2).expect("W");
        for text in ["W", "w", " w ", "#46", "r2c2", "R2C2"] {
            let spec = parse_key(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert_eq!(spec.resolve(&W75), Ok(w), "{text}");
        }
        let caps = KeyPos::new(3, 0).expect("Caps");
        for text in ["Caps Lock", "caps", "capslock", "CAPS-LOCK"] {
            assert_eq!(parse_key(text).and_then(|s| s.resolve(&W75)), Ok(caps));
        }
        let lshift = KeyPos::new(4, 0).expect("L-Shift");
        assert_eq!(
            parse_key("lshift").and_then(|s| s.resolve(&W75)),
            Ok(lshift)
        );
        // Punctuation keys keep their names.
        let minus = KeyPos::new(1, 11).expect("-");
        assert_eq!(parse_key("-").and_then(|s| s.resolve(&W75)), Ok(minus));
        assert_eq!(parse_key("minus").and_then(|s| s.resolve(&W75)), Ok(minus));
    }

    #[test]
    fn bad_keys_and_the_knob_are_refused() {
        for text in ["Knob", "knob", "#15", "r0c15"] {
            let result = parse_key(text).and_then(|s| s.resolve(&W75));
            assert!(
                result.as_ref().is_err_and(|e| e.contains("knob")),
                "{text}: {result:?}"
            );
        }
        assert!(parse_key("Knob").is_err(), "refused while parsing");
        assert!(parse_key("NoSuchKey").is_err());
        assert!(parse_key("#999").is_err());
        assert!(parse_key("r9c0").is_err());
        // A position or index without a key.
        assert!(parse_key("#1").and_then(|s| s.resolve(&W75)).is_err());
        assert!(parse_key("r0c1").and_then(|s| s.resolve(&W75)).is_err());
    }

    #[test]
    fn millimetres_are_parsed_without_floats() {
        assert_eq!(parse_micrometres("1.2"), Ok(1200));
        assert_eq!(parse_micrometres("1.20"), Ok(1200));
        assert_eq!(parse_micrometres("0.3"), Ok(300));
        assert_eq!(parse_micrometres(".3"), Ok(300));
        assert_eq!(parse_micrometres("2"), Ok(2000));
        assert_eq!(parse_micrometres("0.02 mm"), Ok(20));
        assert_eq!(parse_micrometres("1.205"), Ok(1205));
        for bad in [
            "", ".", "1.2.3", "-1", "1,2", "abc", "1.2345", "1e3", "5000",
        ] {
            assert!(parse_micrometres(bad).is_err(), "{bad:?}");
        }
        assert_eq!(parse_distance("1.20"), Ok(Distance(60)));
        assert_eq!(parse_distance("0.30"), Ok(Distance(15)));
        assert_eq!(parse_distance("3.40"), Ok(Distance(170)));
        // 0.29 is 14.5 units: not representable.
        assert!(parse_distance("0.29").is_err_and(|e| e.contains("multiple")));
        assert!(parse_distance("1.205").is_err());
    }

    #[test]
    fn rapid_trigger_values() {
        assert_eq!(
            parse_rapid_trigger("0.30"),
            Ok(RtEdit::Same {
                sensitivity: Distance(15)
            })
        );
        assert_eq!(
            parse_rapid_trigger("0.30/0.50"),
            Ok(RtEdit::Separate {
                press: Distance(15),
                release: Distance(25)
            })
        );
        assert!(parse_rapid_trigger("0.30/").is_err());
        assert!(parse_rapid_trigger("0.31").is_err());
    }

    #[test]
    fn colours_and_modes() {
        assert_eq!(parse_color("0000ff"), Ok(Rgb::new(0, 0, 0xff)));
        assert_eq!(parse_color("#FF8000"), Ok(Rgb::new(0xff, 0x80, 0)));
        assert!(parse_color("blue").is_err());
        assert_eq!(parse_light_mode("static"), Ok(LightMode::Static));
        assert_eq!(
            parse_light_mode("Speed Respond"),
            Ok(LightMode::SpeedRespond)
        );
        assert_eq!(
            parse_light_mode("speed-respond"),
            Ok(LightMode::SpeedRespond)
        );
        assert_eq!(parse_light_mode("custom"), Ok(LightMode::Custom));
        assert_eq!(parse_light_mode("auto_ripple"), Ok(LightMode::AutoRipple));
        // Music rhythm is never written.
        assert!(parse_light_mode("musical rhythm").is_err());
        assert!(parse_light_mode("music").is_err());
        assert!(parse_light_mode("disco").is_err_and(|e| e.contains("static")));
    }
}
