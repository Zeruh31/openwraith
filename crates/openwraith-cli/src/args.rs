//! Arguments of the commands that write (`profile select`, `profile
//! export` / `import`, `set`, `restore`, `travel`) and of `monitor`, and the
//! basic value parsers: key names, millimetres, Rapid Trigger values,
//! colours and light modes. The parsers of the v0.2 editors (key
//! assignments, macro steps, DKS rows, SOCD pairs, durations) are in
//! [`crate::parse`].
//!
//! Millimetres are parsed as decimal text into micrometres (never through a
//! float) and must be an exact multiple of the trigger unit (0.02 mm).

use std::path::PathBuf;
use std::time::Duration;

use clap::{ArgGroup, Args, Subcommand, ValueEnum};
use openwraith_hid::{ImportSelection, PersistenceCheckMode, RtEdit};
use openwraith_protocol::{Distance, KeyPos, Layer, LightMode, MODELS, MacroPlay, Model, Rgb};

use crate::parse::{
    DksRowSpec, KeyAssign, KeyColor, MacroSteps, SocdPairSpec, parse_dks_row, parse_hold_time,
    parse_key_assignment, parse_key_color, parse_macro_steps, parse_play, parse_seconds,
    parse_socd_pair, parse_tenths, parse_usage,
};

/// `openwraith profile ...`.
#[derive(Debug, Subcommand)]
pub enum ProfileAction {
    /// Switch to onboard profile N (WRITES: a profile switch, nothing else).
    Select {
        /// The profile (1-4).
        #[arg(value_parser = clap::value_parser!(u8).range(1..=4))]
        profile: u8,
    },
    /// Save one onboard profile as a file to share or to import into
    /// another profile (no USB serial number in it). Reading a profile that
    /// is not active switches to it and back (asks first).
    Export(ProfileExportArgs),
    /// Import one profile of a file into a profile of the keyboard (WRITES;
    /// shows the plan and asks first; triggers and lighting unless
    /// --include says otherwise).
    Import(ProfileImportArgs),
}

/// `openwraith set ...`.
#[derive(Debug, Subcommand)]
pub enum SetCommand {
    /// Actuation point and Rapid Trigger of keys of the active profile (WRITES).
    Trigger(SetTriggerArgs),
    /// Main light of the active profile: effect, colour, brightness, speed (WRITES).
    Light(SetLightArgs),
    /// Key assignments on the base or Fn layer of the active profile
    /// (WRITES the keymap group).
    Key(SetKeyArgs),
    /// A macro bound to a key, or a macro slot's content (WRITES the keymap
    /// group).
    Macro(SetMacroArgs),
    /// An advanced key (DKS, Mod-Tap, Toggle) bound to a key, or an
    /// advanced-key slot (WRITES the keymap group).
    Advanced(SetAdvancedArgs),
    /// Per-key colours of custom colour table 1 of the active profile (WRITES).
    Colors(SetColorsArgs),
    /// Top and bottom dead zones of keys of the active profile (WRITES).
    Deadband(SetDeadBandArgs),
    /// SOCD power (one setting of every profile) and the key pairs of the
    /// active profile (WRITES).
    Socd(SetSocdArgs),
}

/// Options of every command that changes settings.
#[derive(Debug, Clone, Default, Args)]
pub struct WriteFlags {
    /// Do not ask for confirmation.
    #[arg(short, long, conflicts_with = "dry_run")]
    pub yes: bool,

    /// Only read the keyboard and show the plan; never writes (no safety
    /// backup, read-only connection).
    #[arg(long)]
    pub dry_run: bool,

    /// Also print the hex of every packet the plan would send.
    #[arg(long)]
    pub packets: bool,

    /// Print the plan and the result as JSON (messages and the question go
    /// to stderr).
    #[arg(long)]
    pub json: bool,
}

/// `--check-persistence`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum CheckPersistence {
    /// Check the tables whose keeping is not proven on hardware (today only
    /// advanced-key slots written without the Fn keymap before them, which
    /// --commit-trigger skip checks anyway).
    #[default]
    Auto,
    /// Check every written table.
    Always,
    /// Never switch profiles to check.
    Never,
}

impl CheckPersistence {
    /// The library's mode.
    pub fn mode(self) -> PersistenceCheckMode {
        match self {
            CheckPersistence::Auto => PersistenceCheckMode::Auto,
            CheckPersistence::Always => PersistenceCheckMode::Always,
            CheckPersistence::Never => PersistenceCheckMode::Never,
        }
    }
}

/// Options of the v0.2 editors: [`WriteFlags`] and the persistence check.
#[derive(Debug, Clone, Default, Args)]
pub struct EditFlags {
    #[command(flatten)]
    pub write: WriteFlags,

    // A `help` attribute rather than a doc comment: the text names the
    // effect of every value.
    #[arg(
        long,
        value_enum,
        default_value_t,
        value_name = "WHEN",
        help = concat!(
            "Switch to another profile and back after writing to check that the ",
            "keyboard kept it (the lighting changes briefly): auto = only tables not yet ",
            "proven on hardware (today only advanced-key slots written without the Fn ",
            "keymap before them), always, never [default: auto]"
        )
    )]
    pub check_persistence: CheckPersistence,
}

/// `--layer`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum LayerArg {
    /// The base layer (keys pressed alone).
    #[default]
    Base,
    /// The Fn layer (keys pressed with Fn).
    Fn,
}

impl LayerArg {
    /// The protocol's layer.
    pub fn layer(self) -> Layer {
        match self {
            LayerArg::Base => Layer::Base,
            LayerArg::Fn => Layer::Fn,
        }
    }
}

/// `--commit-trigger`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum CommitTriggerArg {
    /// The vendor site's order: the Fn keymap (18 02) is written after
    /// every base keymap and before the advanced-key slots (the keyboard
    /// keeps an advanced-key slot when the Fn keymap is written in the same
    /// save, and loses it without; macro slots need none).
    #[default]
    Auto,
    /// Hardware test only: advanced-key slots without the Fn keymap before
    /// them (refused when a keymap layer changes; forces the persistence
    /// check).
    Skip,
}

/// `openwraith set trigger KEY...|--all [--actuation MM] [--rapid-trigger P[/R]] [--preset NAME] ...`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("change")
        .required(true)
        .multiple(true)
        .args(["actuation", "rapid_trigger", "no_rapid_trigger", "preset"])
))]
pub struct SetTriggerArgs {
    /// Keys: names as `openwraith show triggers` prints them (case-insensitive,
    /// e.g. W, "Caps Lock", L-Shift), key indices (#46) or matrix positions
    /// (r2c2). The knob has no trigger settings.
    #[arg(
        required_unless_present = "all",
        conflicts_with = "all",
        value_name = "KEY",
        value_parser = parse_key
    )]
    pub keys: Vec<KeySpec>,

    /// Every key of the keyboard (except the knob).
    #[arg(long)]
    pub all: bool,

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

    /// A preset: typing (2.00 mm, RT off), balanced (1.20 mm, RT 0.30),
    /// fast (0.80 mm, RT 0.16), competitive (0.40 mm, RT 0.10), factory
    /// (1.70 mm, RT off with 0.02 mm stored: the factory record).
    #[arg(
        long,
        value_name = "NAME",
        value_parser = ["typing", "balanced", "fast", "competitive", "factory"],
        conflicts_with_all = ["actuation", "rapid_trigger", "no_rapid_trigger"]
    )]
    pub preset: Option<String>,

    #[command(flatten)]
    pub flags: WriteFlags,
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

    #[command(flatten)]
    pub flags: WriteFlags,
}

/// `openwraith set key KEY=ACTION... [--layer base|fn]`.
#[derive(Debug, Args)]
pub struct SetKeyArgs {
    // A `help` attribute rather than a doc comment: rustdoc would read the
    // angle brackets as HTML tags.
    #[arg(
        required = true,
        value_name = "KEY=ACTION",
        value_parser = parse_key_assignment,
        help = concat!(
            "Key and what it does. ACTION: default | a key name (A, L-Ctrl, F13, \"KP 1\") | ",
            "usage:0xNN | media:NAME or media:0xNN (mute, volume-up, volume-down, play-pause, ",
            "next, previous, stop, calculator, browser-back, browser-forward, ...) | ",
            "mouse:left|right|middle|back|forward|wheel-up|wheel-down (back = 0x08, forward = ",
            "0x10) | combo:L-Ctrl+L-Shift+A (1-2 modifiers, then one key) | ",
            "macro:N | advanced:N (an existing slot). Several pairs are written together ",
            "(e.g. a swap: Caps=L-Ctrl L-Ctrl=Caps). The = key is written `equal`, or ==A"
        )
    )]
    pub assignments: Vec<KeyAssign>,

    /// The layer.
    #[arg(long, value_enum, default_value_t)]
    pub layer: LayerArg,

    /// Allow an Fn-layer entry that replaces this key's built-in Fn function
    /// (repeatable; see `openwraith show keymap`).
    #[arg(long, value_name = "KEY", value_parser = parse_keymap_key)]
    pub allow_builtin: Vec<KeySpec>,

    /// Allow replacing a critical built-in Fn function (Fn + 1..4 profile
    /// switches, Fn + Space reset; repeatable).
    #[arg(long, value_name = "KEY", value_parser = parse_keymap_key)]
    pub allow_critical_builtin: Vec<KeySpec>,

    #[command(flatten)]
    pub flags: EditFlags,
}

/// `openwraith set macro ...`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("target")
        .required(true)
        .multiple(true)
        .args(["key", "slot"])
))]
pub struct SetMacroArgs {
    /// The key the macro is bound to (left out with --slot N alone).
    #[arg(value_name = "KEY", value_parser = parse_keymap_key)]
    pub key: Option<KeySpec>,

    // A `help` attribute rather than a doc comment (see above).
    #[arg(
        long,
        value_name = "STEPS",
        value_parser = parse_macro_steps,
        conflicts_with_all = ["unbind", "clear"],
        help = concat!(
            "The steps, e.g. \"A+ 100, A- 100, B+ 100, B-\": KEY+ presses, KEY- releases, then ",
            "the delay after the step in ms (none = 0; the last step has none). At most 40 ",
            "steps; every pressed key must be released"
        )
    )]
    pub steps: Option<MacroSteps>,

    /// How the macro plays: once, times:N (1-255), toggle (a press starts,
    /// the next stops) or while-held.
    #[arg(long, value_name = "MODE", value_parser = parse_play, default_value = "once")]
    pub play: MacroPlay,

    /// The layer of the key.
    #[arg(long, value_enum, default_value_t)]
    pub layer: LayerArg,

    /// The macro slot (0-9) [default for a key: the slot only this key
    /// uses, else the lowest free one in 1-9, slot 0 last]. Writing slot 0
    /// makes the keyboard clear slots 1-9, so their macros are written
    /// again right after it.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u8).range(0..=9))]
    pub slot: Option<u8>,

    /// Set the key back to its default; the slot is cleared unless
    /// --keep-slot.
    #[arg(long, requires = "key")]
    pub unbind: bool,

    /// With --unbind: keep the macro in its slot (no key refers to it then).
    #[arg(long, requires = "unbind")]
    pub keep_slot: bool,

    /// With --slot N alone: clear the slot (refused while a key refers to it).
    #[arg(long, requires = "slot", conflicts_with = "key")]
    pub clear: bool,

    /// Allow an Fn-layer binding that replaces this key's built-in Fn
    /// function (see `openwraith show keymap`).
    #[arg(long, value_name = "KEY", value_parser = parse_keymap_key)]
    pub allow_builtin: Vec<KeySpec>,

    /// Allow replacing a critical built-in Fn function (Fn + 1..4, Fn +
    /// Space).
    #[arg(long, value_name = "KEY", value_parser = parse_keymap_key)]
    pub allow_critical_builtin: Vec<KeySpec>,

    /// The Fn keymap as commit trigger (`skip`: hardware test only).
    #[arg(long, value_enum, default_value_t, value_name = "MODE")]
    pub commit_trigger: CommitTriggerArg,

    #[command(flatten)]
    pub flags: EditFlags,
}

/// The kinds of advanced key (`KIND` of `set advanced`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedKind {
    /// Dynamic keystroke: up to four keys at four stages of the travel.
    Dks,
    /// Tap one key, hold for another.
    ModTap,
    /// A press holds the key down, the next releases it.
    Toggle,
}

impl AdvancedKind {
    /// The kind named by `word` (`dks`, `mod-tap`, `mt`, `toggle`, `tgl`).
    pub fn from_word(word: &str) -> Option<Self> {
        match crate::args::normalise(word).as_str() {
            "dks" => Some(AdvancedKind::Dks),
            "modtap" | "mt" => Some(AdvancedKind::ModTap),
            "toggle" | "tgl" => Some(AdvancedKind::Toggle),
            _ => None,
        }
    }
}

/// `openwraith set advanced ...`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("target")
        .required(true)
        .multiple(true)
        .args(["key", "slot"])
))]
pub struct SetAdvancedArgs {
    /// The key the advanced key is bound to (left out with --slot N).
    #[arg(value_name = "KEY")]
    pub key: Option<String>,

    /// What it is: dks, mod-tap or toggle (left out with --unbind and --clear).
    #[arg(value_name = "KIND")]
    pub kind: Option<String>,

    /// DKS: the first press point in mm, in steps of 0.1 mm (1.5 = the
    /// vendor site's default; at most the keyboard's deep point, 3.0 mm on
    /// the W75).
    #[arg(long, value_name = "MM", value_parser = parse_tenths)]
    pub point: Option<u8>,

    // A `help` attribute rather than a doc comment (see above).
    #[arg(
        long = "row",
        value_name = "KEY:STAGES",
        value_parser = parse_dks_row,
        help = concat!(
            "DKS: a key and what it does at the four stages (press past the point, ",
            "bottom-out, lift from the bottom, release past the point): 4 characters, . ",
            "nothing, t tap, h hold to the next stage, r release and press again (after a ",
            "hold), e.g. E:t... or R:.t.. (repeatable, at most 4 rows)"
        )
    )]
    pub rows: Vec<DksRowSpec>,

    /// Mod-Tap: the key sent when the key is tapped [default: the key itself].
    #[arg(long, value_name = "KEY", value_parser = parse_usage)]
    pub tap: Option<openwraith_protocol::HidUsage>,

    /// Mod-Tap: the key sent when the key is held.
    #[arg(long, value_name = "KEY", value_parser = parse_usage)]
    pub hold: Option<openwraith_protocol::HidUsage>,

    /// Mod-Tap: how long a press counts as a tap, 150-1000 ms in steps of
    /// 10 ms [default: 300ms].
    #[arg(long, value_name = "DURATION", value_parser = parse_hold_time)]
    pub time: Option<u8>,

    /// Toggle: the key that is held [default: the key itself].
    #[arg(long = "key", value_name = "KEY", value_parser = parse_usage)]
    pub toggle_key: Option<openwraith_protocol::HidUsage>,

    /// The layer of the key.
    #[arg(long, value_enum, default_value_t)]
    pub layer: LayerArg,

    /// The advanced-key slot (0-39) [default for a key: the slot only this
    /// key uses, else the lowest free one].
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u8).range(0..=39))]
    pub slot: Option<u8>,

    /// Set the key back to its default; the slot is cleared unless
    /// --keep-slot.
    #[arg(long)]
    pub unbind: bool,

    /// With --unbind: keep the advanced key in its slot (no key refers to it
    /// then).
    #[arg(long, requires = "unbind")]
    pub keep_slot: bool,

    /// With --slot N alone: clear the slot (refused while a key refers to it).
    #[arg(long, requires = "slot", conflicts_with = "unbind")]
    pub clear: bool,

    /// Allow an Fn-layer binding that replaces this key's built-in Fn
    /// function (see `openwraith show keymap`).
    #[arg(long, value_name = "KEY", value_parser = parse_keymap_key)]
    pub allow_builtin: Vec<KeySpec>,

    /// Allow replacing a critical built-in Fn function (Fn + 1..4, Fn +
    /// Space).
    #[arg(long, value_name = "KEY", value_parser = parse_keymap_key)]
    pub allow_critical_builtin: Vec<KeySpec>,

    /// The Fn keymap as commit trigger (`skip`: hardware test only).
    #[arg(long, value_enum, default_value_t, value_name = "MODE")]
    pub commit_trigger: CommitTriggerArg,

    #[command(flatten)]
    pub flags: EditFlags,
}

/// `openwraith set colors KEY=RRGGBB... [--all RRGGBB] [--custom-mode]`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("what")
        .required(true)
        .multiple(true)
        .args(["colors", "all"])
))]
pub struct SetColorsArgs {
    /// Keys and their colours as hex RRGGBB (e.g. W=ff0000 "Caps Lock"=00ff00).
    #[arg(value_name = "KEY=RRGGBB", value_parser = parse_key_color)]
    pub colors: Vec<KeyColor>,

    /// This colour for every key (except the knob); KEY=RRGGBB pairs win.
    #[arg(long, value_name = "RRGGBB", value_parser = parse_color)]
    pub all: Option<Rgb>,

    /// Also switch the main light to the Custom mode, in which the keyboard
    /// shows these colours.
    #[arg(long)]
    pub custom_mode: bool,

    #[command(flatten)]
    pub flags: EditFlags,
}

/// `openwraith set deadband KEY...|--all [--top MM] [--bottom MM]`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("change")
        .required(true)
        .multiple(true)
        .args(["top", "bottom"])
))]
pub struct SetDeadBandArgs {
    /// Keys (as for `set trigger`).
    #[arg(
        required_unless_present = "all",
        conflicts_with = "all",
        value_name = "KEY",
        value_parser = parse_dead_band_key
    )]
    pub keys: Vec<KeySpec>,

    /// Every key of the keyboard (except the knob).
    #[arg(long)]
    pub all: bool,

    /// Dead zone at the top of the travel in mm (a multiple of 0.02).
    #[arg(long, value_name = "MM", value_parser = parse_distance)]
    pub top: Option<Distance>,

    /// Dead zone at the bottom of the travel in mm (a multiple of 0.02).
    #[arg(long, value_name = "MM", value_parser = parse_distance)]
    pub bottom: Option<Distance>,

    #[command(flatten)]
    pub flags: EditFlags,
}

/// `openwraith set socd [--on|--off] [--pair A/D[:1-4]]... [--remove-pair A/D]... [--clear-pairs]`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("change")
        .required(true)
        .multiple(true)
        .args(["on", "off", "pairs", "remove_pairs", "clear_pairs"])
))]
pub struct SetSocdArgs {
    /// Switch SOCD on.
    #[arg(long, conflicts_with = "off")]
    pub on: bool,

    /// Switch SOCD off.
    #[arg(long)]
    pub off: bool,

    // A `help` attribute rather than a doc comment (see above).
    #[arg(
        long = "pair",
        value_name = "KEY1/KEY2[:MODE]",
        value_parser = parse_socd_pair,
        help = concat!(
            "Add a key pair (or change the mode of an existing one), e.g. A/D or Left/Right:2. ",
            "MODE (what happens while both are held): 1 the last input wins (default for a new ",
            "pair), 2 key 1 wins, 3 key 2 wins, 4 the later key only interrupts the first. ",
            "Without :MODE an existing pair keeps its mode and key order. Repeatable; at most 20 ",
            "pairs, no key in two pairs"
        )
    )]
    pub pairs: Vec<SocdPairSpec>,

    /// Remove the pair of these two keys (repeatable).
    #[arg(long = "remove-pair", value_name = "KEY1/KEY2", value_parser = parse_socd_pair)]
    pub remove_pairs: Vec<SocdPairSpec>,

    /// Remove every pair first (also replaces pairs OpenWraith cannot read,
    /// which the undo command cannot bring back).
    #[arg(long)]
    pub clear_pairs: bool,

    #[command(flatten)]
    pub flags: EditFlags,
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

/// `openwraith profile export N -o FILE [--from BACKUP] [--label TEXT]`.
#[derive(Debug, Args)]
pub struct ProfileExportArgs {
    /// The profile to export (1-4).
    #[arg(value_parser = clap::value_parser!(u8).range(1..=4))]
    pub profile: u8,

    /// The file to write (e.g. fps.owprofile.json).
    #[arg(short, long, value_name = "FILE")]
    pub output: PathBuf,

    /// Take the profile from this backup file instead of the keyboard (no
    /// keyboard needed).
    #[arg(long, value_name = "BACKUP_FILE")]
    pub from: Option<PathBuf>,

    /// A label stored in the file's note (at most 120 characters).
    #[arg(long, value_name = "TEXT")]
    pub label: Option<String>,

    /// Overwrite the output file if it already exists.
    #[arg(long)]
    pub force: bool,

    /// Do not ask before switching to the profile when it is not active.
    #[arg(short, long)]
    pub yes: bool,
}

/// The categories `profile import --include` takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ImportPart {
    /// Actuation and Rapid Trigger of every key.
    Triggers,
    /// The main light and custom colour table 1.
    Lighting,
    /// Key assignments (both layers), macros and advanced keys.
    Keys,
    /// The dead band.
    Deadband,
    /// The SOCD pairs (the SOCD power is one setting of the whole keyboard
    /// and keeps the keyboard's value).
    Socd,
}

/// `openwraith profile import FILE --into N ...`.
#[derive(Debug, Args)]
pub struct ProfileImportArgs {
    /// The file (a `profile export` file, or any backup).
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// The profile of the keyboard to write (1-4).
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u8).range(1..=4))]
    pub into: u8,

    /// The profile of the file to import [default: the file's own profile,
    /// for an all-profile backup the one that was active].
    #[arg(long = "profile", value_name = "N", value_parser = clap::value_parser!(u8).range(1..=4))]
    pub from: Option<u8>,

    /// What to import, comma-separated.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        value_name = "PARTS",
        default_values_t = [ImportPart::Triggers, ImportPart::Lighting]
    )]
    pub include: Vec<ImportPart>,

    /// Allow an imported Fn layer that replaces a critical built-in function
    /// (Fn + 1..4, Fn + Space).
    #[arg(long)]
    pub allow_critical_fn_overrides: bool,

    /// Import even if the keyboard runs other firmware than the file was
    /// made with. Check the plan.
    #[arg(long)]
    pub allow_firmware_mismatch: bool,

    #[command(flatten)]
    pub flags: WriteFlags,
}

impl ProfileImportArgs {
    /// The library's selection.
    pub fn selection(&self) -> ImportSelection {
        let has = |part| self.include.contains(&part);
        ImportSelection {
            triggers: has(ImportPart::Triggers),
            lighting: has(ImportPart::Lighting),
            keymap_group: has(ImportPart::Keys),
            dead_band: has(ImportPart::Deadband),
            socd: has(ImportPart::Socd),
        }
    }
}

/// `openwraith travel [--duration 30s] [--keys W,A] [--json]` and
/// `openwraith travel --close`.
#[derive(Debug, Args)]
pub struct TravelArgs {
    /// How long the test stays open at most (e.g. 30s, 2m; at most 10m).
    #[arg(
        long,
        value_name = "DURATION",
        value_parser = parse_seconds,
        default_value = "30s",
        conflicts_with = "close"
    )]
    pub duration: Duration,

    /// Only show these keys, comma-separated (the test always covers the
    /// whole keyboard).
    #[arg(
        long,
        value_name = "KEYS",
        value_delimiter = ',',
        value_parser = parse_travel_key,
        conflicts_with = "close"
    )]
    pub keys: Vec<KeySpec>,

    /// Print one JSON object per depth report and one at the end.
    #[arg(long)]
    pub json: bool,

    /// Only send the close of the test (for a test that a crashed run left
    /// open, or when another command says one may still be running).
    #[arg(long)]
    pub close: bool,
}

/// `openwraith monitor [--duration 30s] [--json]`.
#[derive(Debug, Args)]
pub struct MonitorArgs {
    /// How long to listen (e.g. 30s, 2m; Ctrl+C stops earlier).
    #[arg(long, value_name = "DURATION", value_parser = parse_seconds, default_value = "30s")]
    pub duration: Duration,

    /// Print one JSON object per report.
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

/// What a key is given for: decides which keys are refused and with which
/// message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyUse {
    /// Trigger settings (`set trigger`).
    Trigger,
    /// Keymap entries (`set key`, `set macro`, `set advanced`): neither the
    /// knob nor the Fn key.
    Keymap,
    /// Colours of custom table 1.
    Color,
    /// Dead zones.
    DeadBand,
    /// The live key-depth view.
    Travel,
}

impl KeyUse {
    /// Why the knob is refused for this use.
    fn knob_message(self, name: &str) -> String {
        match self {
            KeyUse::Trigger => {
                format!("{name} (the knob) has no trigger settings that can be changed")
            }
            KeyUse::Keymap => format!(
                "{name} (the knob): its keymap entries are managed by the keyboard and kept as \
                 read; OpenWraith never changes them"
            ),
            KeyUse::Color => format!("{name} (the knob) has no colour that can be changed"),
            KeyUse::DeadBand => format!("{name} (the knob) has no dead band that can be changed"),
            KeyUse::Travel => format!("{name} (the knob) has no key depth"),
        }
    }

    /// Why the Fn key is refused for this use, if it is.
    fn fn_message(self, name: &str) -> Option<String> {
        (self == KeyUse::Keymap).then(|| {
            format!(
                "{name} cannot be reassigned: without it the Fn layer and the keyboard's own Fn \
                 shortcuts could not be reached"
            )
        })
    }
}

/// Short forms accepted for some key names.
pub(crate) const KEY_ALIASES: &[(&str, &str)] = &[
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
pub(crate) fn normalise(name: &str) -> String {
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

/// `name` normalised, with an alias replaced by the name it stands for.
pub(crate) fn normalise_alias(name: &str) -> String {
    let wanted = normalise(name);
    KEY_ALIASES
        .iter()
        .find(|(alias, _)| *alias == wanted)
        .map_or(wanted, |(_, real)| normalise(real))
}

impl KeySpec {
    /// The key of `model` this spec names, for `purpose`.
    pub fn resolve_as(&self, model: &Model, purpose: KeyUse) -> Result<KeyPos, String> {
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
        let name = model.key_name(pos.index());
        if Some(pos.index()) == model.knob_index {
            return Err(purpose.knob_message(&name));
        }
        if Some(pos.index()) == model.fn_index {
            if let Some(message) = purpose.fn_message(&name) {
                return Err(message);
            }
        }
        Ok(pos)
    }
}

/// A key of `model` by name, ignoring case, spaces, `-` and `_`, or by alias.
pub(crate) fn find_by_name(model: &Model, name: &str) -> Option<&'static openwraith_protocol::Key> {
    let wanted = normalise_alias(name);
    model.keys.iter().find(|k| normalise(k.name) == wanted)
}

/// Parses a key for trigger settings: `#46`, `r2c2` or a key name of a
/// known model.
pub fn parse_key(text: &str) -> Result<KeySpec, String> {
    parse_key_as(text, KeyUse::Trigger)
}

/// Parses a key for keymap entries (neither the knob nor Fn).
pub fn parse_keymap_key(text: &str) -> Result<KeySpec, String> {
    parse_key_as(text, KeyUse::Keymap)
}

/// Parses a key for dead zones.
pub fn parse_dead_band_key(text: &str) -> Result<KeySpec, String> {
    parse_key_as(text, KeyUse::DeadBand)
}

/// Parses a key for the key-depth view.
pub fn parse_travel_key(text: &str) -> Result<KeySpec, String> {
    parse_key_as(text, KeyUse::Travel)
}

/// Parses a key for `purpose`: `#46`, `r2c2` or a key name of a known
/// model. Keys the purpose refuses by name are refused here already.
pub fn parse_key_as(text: &str, purpose: KeyUse) -> Result<KeySpec, String> {
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
            // The knob (and for keymaps the Fn key) is refused already here.
            if Some(key.index) == model.knob_index {
                return Err(purpose.knob_message(key.name));
            }
            if Some(key.index) == model.fn_index {
                if let Some(message) = purpose.fn_message(key.name) {
                    return Err(message);
                }
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
            assert_eq!(spec.resolve_as(&W75, KeyUse::Trigger), Ok(w), "{text}");
        }
        let caps = KeyPos::new(3, 0).expect("Caps");
        for text in ["Caps Lock", "caps", "capslock", "CAPS-LOCK"] {
            assert_eq!(
                parse_key(text).and_then(|s| s.resolve_as(&W75, KeyUse::Trigger)),
                Ok(caps)
            );
        }
        let lshift = KeyPos::new(4, 0).expect("L-Shift");
        assert_eq!(
            parse_key("lshift").and_then(|s| s.resolve_as(&W75, KeyUse::Trigger)),
            Ok(lshift)
        );
        // Punctuation keys keep their names.
        let minus = KeyPos::new(1, 11).expect("-");
        assert_eq!(
            parse_key("-").and_then(|s| s.resolve_as(&W75, KeyUse::Trigger)),
            Ok(minus)
        );
        assert_eq!(
            parse_key("minus").and_then(|s| s.resolve_as(&W75, KeyUse::Trigger)),
            Ok(minus)
        );
    }

    #[test]
    fn bad_keys_and_the_knob_are_refused() {
        for text in ["Knob", "knob", "#15", "r0c15"] {
            let result = parse_key(text).and_then(|s| s.resolve_as(&W75, KeyUse::Trigger));
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
        assert!(
            parse_key("#1")
                .and_then(|s| s.resolve_as(&W75, KeyUse::Trigger))
                .is_err()
        );
        assert!(
            parse_key("r0c1")
                .and_then(|s| s.resolve_as(&W75, KeyUse::Trigger))
                .is_err()
        );
    }

    #[test]
    fn keymap_keys_refuse_the_knob_and_fn_also_by_index() {
        for text in ["Knob", "Fn", "fn"] {
            assert!(parse_keymap_key(text).is_err(), "{text}");
        }
        for text in ["#15", "#121", "r5c11", "r0c15"] {
            let result = parse_keymap_key(text).and_then(|s| s.resolve_as(&W75, KeyUse::Keymap));
            assert!(result.is_err(), "{text}: {result:?}");
        }
        assert!(
            parse_keymap_key("Fn").is_err_and(|e| e.contains("cannot be reassigned")),
            "Fn"
        );
        // Fn has trigger settings and a colour.
        let fn_key = KeyPos::new(5, 11).expect("Fn");
        assert_eq!(
            parse_key("Fn").and_then(|s| s.resolve_as(&W75, KeyUse::Trigger)),
            Ok(fn_key)
        );
        assert_eq!(
            parse_keymap_key("Caps").and_then(|s| s.resolve_as(&W75, KeyUse::Keymap)),
            Ok(KeyPos::new(3, 0).expect("Caps"))
        );
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

    #[test]
    fn advanced_kinds_by_word() {
        assert_eq!(AdvancedKind::from_word("DKS"), Some(AdvancedKind::Dks));
        assert_eq!(
            AdvancedKind::from_word("mod-tap"),
            Some(AdvancedKind::ModTap)
        );
        assert_eq!(AdvancedKind::from_word("mt"), Some(AdvancedKind::ModTap));
        assert_eq!(
            AdvancedKind::from_word("toggle"),
            Some(AdvancedKind::Toggle)
        );
        assert_eq!(AdvancedKind::from_word("E"), None);
    }
}
