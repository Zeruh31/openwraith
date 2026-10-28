//! `openwraith`: command line tool for the Wraith W75 keyboard.
//!
//! `list`, `info`, `show`, `monitor` and `backup` (without
//! `--all-profiles`) are read-only: they open a transport type that cannot
//! send a write (see the `openwraith-hid` crate). The commands that write
//! (`profile select`, `profile export` of a profile that is not active and
//! `backup --all-profiles`, which only switch profiles; `set`, `restore`,
//! `profile import`; `travel`, which only opens and closes the key-depth
//! test) go through the guarded write path: consent, the "other program"
//! gate, a verified safety backup before any change, stop-and-wait writes
//! and read-back verification. Their logic lives in [`commands`],
//! [`edits`], [`share`] and [`live`].

#![forbid(unsafe_code)]

mod args;
mod commands;
mod device;
mod edit_text;
mod edits;
mod live;
mod parse;
mod render;
mod report;
mod share;
#[cfg(test)]
mod sim_tests;
#[cfg(test)]
mod sim_tests_v02;
mod table;
#[cfg(test)]
mod test_support;
mod ui;
mod view;

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use openwraith_hid::backup::BACKUP_NOTE;
use openwraith_hid::{
    BACKUP_SCHEMA_VERSION, BACKUP_SCOPE, Backup, BackupError, EditError, ProfilesError,
    ProfilesFailure, Progress, RestoreError, Session, SessionError, SessionStats, Transport,
    TransportError, WriteError, read_all_with, read_summary,
};
use openwraith_protocol::{DeviceEvent, PlacementError, ProfileId, WriteBuildError};

use crate::args::{MonitorArgs, ProfileAction, RestoreArgs, SetCommand, TravelArgs};
use crate::commands::{Exit, Globals, OutputTarget, UsageError, tool_options};
use crate::device::{Connector, DeviceError, HidConnector};
use crate::ui::{Terminal, Ui};
use crate::view::{
    AdvancedView, AllView, InfoView, JSON_NOTE, JsonOutput, KeymapView, LightView, MacrosView,
    SocdView, TriggersView, UndecodedNote, undecoded_notes,
};

const ABOUT: &str = "Configuration tool for the Wraith W75 keyboard \
(not affiliated with or endorsed by Wraith)";

const LONG_ABOUT: &str = "Configuration tool for the Wraith W75 Hall-effect keyboard.

Shows device information, trigger (actuation / Rapid Trigger) settings, \
lighting, keymap, macros, advanced keys and SOCD, saves lossless backups, and \
restores them; edits triggers, the main light, key assignments, macros, \
advanced keys, per-key colours, the dead band and SOCD; shares one profile as \
a file; shows live key depths.

list, info, show, monitor and backup are READ-ONLY (backup --all-profiles \
excepted, see below): they never send a command that changes, resets or \
calibrates the keyboard. These commands write to the keyboard: profile select, \
set, restore, profile import, backup --all-profiles and profile export of a \
profile that is not active (profile switches only), and travel (only the \
key-depth test's open and close). set, restore and profile import save a \
verified safety backup first (see --safety-dir), show what will change and ask \
for confirmation (unless --yes; with --dry-run they only show the plan); every \
write is checked by reading it back. profile select and backup --all-profiles \
only switch profiles and do not ask.

The keyboard saves a change about 1 s after it is written; a profile switch \
or unplugging it before that loses the change. Commands that write therefore \
wait 2 s after the last write before they switch profiles or finish. restore, \
and set with --check-persistence always, read what was written again after \
switching away and back (\"persisted\", or \"LOST\" and exit code 1); hardware \
tests showed that the keyboard keeps every table set writes, so set does not \
by default. A profile change on the \
keyboard or a lost connection before the keyboard has saved is reported as \
\"MAY BE LOST\" (exit code 1).

Close the vendor web configurator first, the whole browser tab (a tab in the \
background may not be detected): it receives OpenWraith's replies too and \
reacts to them.

Values marked * come from the vendor configurator's code and are not yet \
confirmed on the wire (see docs/PROTOCOL.md).

OpenWraith is not affiliated with or endorsed by Wraith.";

/// Command line arguments.
#[derive(Debug, Parser)]
#[command(name = "openwraith", version, about = ABOUT, long_about = LONG_ABOUT)]
struct Cli {
    /// Keyboard to use when several are connected (a path printed by `openwraith list`).
    #[arg(long, global = true, value_name = "PATH")]
    device: Option<String>,

    /// Print session statistics (requests, retries, ignored reports) to stderr.
    #[arg(short, long, global = true)]
    verbose: bool,

    // A `help` attribute rather than a doc comment: rustdoc would read the
    // backslashes and angle brackets as markup.
    #[arg(
        long,
        global = true,
        value_name = "DIR",
        help = concat!(
            "Directory for the safety backups made before every change ",
            "[default: $OPENWRAITH_SAFETY_DIR, else the per-user data directory, ",
            "e.g. %LOCALAPPDATA%\\OpenWraith\\safety-backups on Windows]"
        )
    )]
    safety_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List connected Wraith keyboards (enumeration only; nothing is opened).
    List,
    /// Show the model, firmware, active profile and capabilities.
    Info {
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
        /// Read from a backup file instead of the keyboard.
        #[arg(long, value_name = "BACKUP_FILE")]
        from: Option<PathBuf>,
        /// With --from: the profile of an all-profile backup to show
        /// [default: the profile that was active when it was made].
        #[arg(
            long,
            value_name = "N",
            requires = "from",
            value_parser = clap::value_parser!(u8).range(1..=4)
        )]
        profile: Option<u8>,
    },
    /// Save every setting of the active onboard profile (with --all-profiles:
    /// of all four) to a backup file.
    Backup {
        // A `help` attribute rather than a doc comment: rustdoc would read
        // the angle brackets as HTML tags.
        #[arg(
            short,
            long,
            value_name = "FILE",
            help = concat!(
                "Output file, or an existing directory for the default name ",
                "[default: openwraith-<product>-<YYYYMMDD-HHMMSS>.owbackup.json ",
                "in the current directory, time in UTC]"
            )
        )]
        output: Option<PathBuf>,
        /// Overwrite the output file if it already exists.
        #[arg(long)]
        force: bool,
        /// Read all four onboard profiles. This WRITES profile switches: it
        /// switches to each profile (the lighting changes briefly) and back
        /// to the active one; nothing else is written.
        #[arg(long)]
        all_profiles: bool,
    },
    /// Show settings as readable tables, from the keyboard or from a backup.
    Show {
        /// What to show.
        #[arg(value_enum)]
        what: Section,
        /// Read from a backup file instead of the keyboard.
        #[arg(long, value_name = "BACKUP_FILE")]
        from: Option<PathBuf>,
        /// With --from: the profile of an all-profile backup to show
        /// [default: the profile that was active when it was made].
        #[arg(
            long,
            value_name = "N",
            requires = "from",
            value_parser = clap::value_parser!(u8).range(1..=4)
        )]
        profile: Option<u8>,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Show the active onboard profile; `profile select N` switches to
    /// another one (WRITES).
    Profile {
        #[command(subcommand)]
        action: Option<ProfileAction>,
    },
    /// Change settings of the active profile (WRITES; asks first).
    Set {
        #[command(subcommand)]
        what: SetCommand,
    },
    /// Restore a backup (WRITES; shows the plan and asks first). With
    /// --dry-run it only compares and never writes.
    Restore(RestoreArgs),
    /// Show live key depths (the keyboard's travel test; WRITES only the
    /// test's open and close, nothing is stored). With --close: only close a
    /// test that may still be running.
    Travel(TravelArgs),
    /// Print what the keyboard sends on its own (profile switches, the SOCD
    /// switch, key-depth reports, other programs' traffic). Read-only:
    /// nothing is sent.
    Monitor(MonitorArgs),
}

/// Sections of `openwraith show`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Section {
    /// Actuation point, Rapid Trigger and dead band of every key.
    Triggers,
    /// Light effect and per-key colour tables.
    Light,
    /// Remapped keys on the base and Fn layers.
    Keymap,
    /// The ten macro slots, with their steps and the keys bound to them.
    Macros,
    /// The forty advanced-key slots (DKS, Mod-Tap, Toggle) and their keys.
    Advanced,
    /// SOCD power and key pairs.
    Socd,
    /// Everything, including advanced keys, SOCD and switch types.
    All,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(Exit::Success) => ExitCode::SUCCESS,
        Ok(Exit::Failure) => ExitCode::FAILURE,
        Err(err) => {
            report_error(&err);
            exit_code(&err)
        }
    }
}

/// 2 for usage errors found after parsing, else 1.
fn exit_code(err: &anyhow::Error) -> ExitCode {
    if is_usage_error(err) {
        ExitCode::from(2)
    } else {
        ExitCode::FAILURE
    }
}

/// Whether `err` is (or wraps) a [`UsageError`].
fn is_usage_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|c| c.downcast_ref::<UsageError>().is_some())
}

fn globals(cli: &Cli) -> Globals {
    Globals {
        verbose: cli.verbose,
        safety_dir: cli.safety_dir.clone(),
    }
}

fn run(cli: &Cli) -> anyhow::Result<Exit> {
    match &cli.command {
        Command::List => cmd_list().map(|()| Exit::Success),
        Command::Info {
            json,
            from,
            profile,
        } => cmd_info(cli, *json, from.as_deref(), *profile).map(|()| Exit::Success),
        Command::Backup {
            output,
            force,
            all_profiles: false,
        } => cmd_backup(cli, output.as_deref(), *force).map(|()| Exit::Success),
        Command::Show {
            what,
            from,
            profile,
            json,
        } => cmd_show(cli, *what, from.as_deref(), *profile, *json).map(|()| Exit::Success),
        Command::Backup {
            all_profiles: true, ..
        }
        | Command::Profile { .. }
        | Command::Set { .. }
        | Command::Restore(_)
        | Command::Travel(_)
        | Command::Monitor(_) => {
            let mut connector = HidConnector::new(cli.device.clone());
            let mut ui = Terminal::new();
            run_command(&cli.command, &globals(cli), &mut connector, &mut ui)
        }
    }
}

/// The commands whose logic lives in [`commands`] (the ones that may write,
/// and `profile`), with any connector and console. The binary passes the
/// HID connector; unit tests pass the device simulator.
fn run_command<C: Connector, U: Ui>(
    command: &Command,
    globals: &Globals,
    connector: &mut C,
    ui: &mut U,
) -> anyhow::Result<Exit> {
    match command {
        Command::Backup {
            output,
            force,
            all_profiles: true,
        } => commands::backup_all_profiles(connector, ui, globals, output.as_deref(), *force),
        Command::Profile { action } => commands::profile(connector, ui, globals, action.as_ref()),
        Command::Set { what } => match what {
            SetCommand::Trigger(args) => edits::set_trigger(connector, ui, globals, args),
            SetCommand::Light(args) => edits::set_light(connector, ui, globals, args),
            SetCommand::Key(args) => edits::set_key(connector, ui, globals, args),
            SetCommand::Macro(args) => edits::set_macro(connector, ui, globals, args),
            SetCommand::Advanced(args) => edits::set_advanced(connector, ui, globals, args),
            SetCommand::Colors(args) => edits::set_colors(connector, ui, globals, args),
            SetCommand::Deadband(args) => edits::set_dead_band(connector, ui, globals, args),
            SetCommand::Socd(args) => edits::set_socd(connector, ui, globals, args),
        },
        Command::Restore(args) => commands::restore(connector, ui, globals, args),
        Command::Travel(args) => live::travel(connector, ui, globals, args),
        Command::Monitor(args) => live::monitor(connector, ui, globals, args),
        Command::List
        | Command::Info { .. }
        | Command::Show { .. }
        | Command::Backup {
            all_profiles: false,
            ..
        } => Err(anyhow::anyhow!(
            "internal error: this command does not use a connector"
        )),
    }
}

fn cmd_list() -> anyhow::Result<()> {
    let devices = device::list()?;
    if devices.is_empty() {
        return emit("No supported keyboard found.\n");
    }
    let mut out =
        String::from("Supported keyboards (configuration interface; nothing was opened):\n");
    for d in &devices {
        out.push_str(&format!("  {}\n      {}\n", d.path(), device::describe(d)));
    }
    emit(&out)
}

fn cmd_info(cli: &Cli, json: bool, from: Option<&Path>, profile: Option<u8>) -> anyhow::Result<()> {
    let (view, notes) = match from {
        Some(path) => {
            let backup = load_profile(path, profile)?;
            (
                InfoView::from_backup(&backup, Some(path)),
                undecoded_notes(&backup),
            )
        }
        None => {
            // Refuses unknown models before sending anything but the
            // device-info request.
            let mut session = device::open_session(cli.device.as_deref())?;
            let result = read_summary(&mut session);
            report_session(cli, &session);
            (InfoView::from_summary(&result?), Vec::new())
        }
    };
    if json {
        emit_json(&view, notes)
    } else {
        emit(&(render::info(&view) + &render::undecoded_footer(&notes)))
    }
}

fn cmd_backup(cli: &Cli, output: Option<&Path>, force: bool) -> anyhow::Result<()> {
    // Fail before reading the keyboard if the chosen file is taken.
    let target = OutputTarget::check(output, force)?;
    let mut session = device::open_session(cli.device.as_deref())?;
    let backup = read_keyboard(cli, &mut session)?;
    let path = target.path_for(&backup);
    backup.save(&path, force)?;
    emit(
        &(render::backup_summary(&backup, &path)
            + &render::undecoded_footer(&undecoded_notes(&backup))),
    )
}

fn cmd_show(
    cli: &Cli,
    what: Section,
    from: Option<&Path>,
    profile: Option<u8>,
    json: bool,
) -> anyhow::Result<()> {
    let backup = match from {
        Some(path) => load_profile(path, profile)?,
        None => {
            let mut session = device::open_session(cli.device.as_deref())?;
            read_keyboard(cli, &mut session)?
        }
    };
    let model = backup.model();
    let notes = undecoded_notes(&backup);
    if json {
        // Serialised directly (not through `serde_json::Value`) so that
        // fields keep their declaration order.
        return match what {
            Section::Triggers => emit_json(&TriggersView::new(&backup), notes),
            Section::Light => emit_json(&LightView::new(&backup), notes),
            Section::Keymap => emit_json(&KeymapView::new(&backup), notes),
            Section::Macros => emit_json(&MacrosView::new(&backup), notes),
            Section::Advanced => emit_json(&AdvancedView::new(&backup), notes),
            Section::Socd => emit_json(&SocdView::new(&backup), notes),
            Section::All => emit_json(&AllView::new(&backup, from), notes),
        };
    }
    let high_precision = backup.status().is_some_and(|s| s.flags_b.high_precision);
    let unit_um = backup
        .trigger_global()
        .map(|g| g.unit_micrometres(high_precision));
    let text = match what {
        Section::Triggers => render::triggers(&TriggersView::new(&backup), unit_um),
        Section::Light => render::light(&LightView::new(&backup), model),
        Section::Keymap => render::keymap(&KeymapView::new(&backup), model),
        Section::Macros => render::macros(&MacrosView::new(&backup)),
        Section::Advanced => render::with_legend(render::advanced(&AdvancedView::new(&backup))),
        Section::Socd => render::with_legend(render::socd(&SocdView::new(&backup))),
        Section::All => render::all(&AllView::new(&backup, from), model, unit_um),
    };
    emit(&(text + &render::undecoded_footer(&notes)))
}

/// Loads a backup file for `info` / `show`, reduced to the requested
/// profile. For an all-profile backup without `--profile` the primary
/// snapshot (the profile that was active when it was made) is shown, and a
/// note on stderr names the others.
fn load_profile(path: &Path, profile: Option<u8>) -> anyhow::Result<Backup> {
    let backup = Backup::load(path)?;
    let (backup, note) = select_profile(backup, profile)?;
    if let Some(note) = note {
        eprintln!("note: {}", printable(&note));
    }
    Ok(backup)
}

/// The backup reduced to one profile (see [`load_profile`]), and a note.
fn select_profile(backup: Backup, profile: Option<u8>) -> anyhow::Result<(Backup, Option<String>)> {
    let held: Vec<String> = backup
        .profile_numbers()
        .iter()
        .map(ToString::to_string)
        .collect();
    let primary = backup.active_profile;
    let primary_text = primary.map_or_else(|| "?".to_string(), |p| p.to_string());
    let Some(wanted) = profile else {
        let note = backup.is_all_profiles().then(|| {
            format!(
                "all-profile backup (profiles {}); showing profile {primary_text}, the one that \
                 was active when it was made. Use --profile N for another.",
                held.join(", ")
            )
        });
        return Ok((backup, note));
    };
    if primary == Some(wanted) {
        return Ok((backup, None));
    }
    let snapshot = ProfileId::new(wanted).and_then(|id| backup.snapshot(id));
    let Some(snapshot) = snapshot else {
        return Err(UsageError(format!(
            "the backup holds no profile {wanted} (it holds profile(s) {}){}",
            held.join(", "),
            if backup.is_all_profiles() {
                ""
            } else {
                "; `openwraith backup --all-profiles` saves all four"
            }
        ))
        .into());
    };
    let mut single = backup.clone();
    single.entries = snapshot.entries().to_vec();
    single.active_profile = Some(wanted);
    single.profiles.clear();
    single.schema_version = BACKUP_SCHEMA_VERSION;
    single.scope = BACKUP_SCOPE.to_string();
    single.note = BACKUP_NOTE.to_string();
    let note = format!(
        "showing profile {wanted} of an all-profile backup (read after switching to it; profile \
         {primary_text} was active when the backup was made)"
    );
    Ok((single, Some(note)))
}

/// Reads everything from the keyboard, with a progress line on a terminal.
fn read_keyboard<T: Transport>(cli: &Cli, session: &mut Session<T>) -> anyhow::Result<Backup> {
    let interactive = std::io::stderr().is_terminal();
    eprintln!("Reading the keyboard (read-only)...");
    let options = tool_options();
    let result = read_all_with(session, &options, |p: Progress| {
        if interactive {
            eprint!(
                "\r  {:>3}/{:<3} {:<48}",
                p.done,
                p.total,
                p.request.to_string()
            );
        }
    });
    if interactive {
        eprint!("\r{:60}\r", "");
    }
    report_session(cli, session);
    Ok(result?)
}

/// Printed when another program used the keyboard during a command.
const OTHER_PROGRAM_WARNING: &str = "another program was talking to the keyboard (probably the \
vendor web configurator). It also received OpenWraith's replies; reload that page before \
changing anything there, and close it before using OpenWraith next time.";

/// After a command that talked to the keyboard: statistics with `--verbose`,
/// and always a warning if another program was active.
fn report_session<T: Transport>(cli: &Cli, session: &Session<T>) {
    if cli.verbose {
        let stats = serde_json::to_string(session.stats()).unwrap_or_default();
        eprintln!("session statistics: {stats}");
    }
    if let Some(warning) = other_program_warning(session.stats(), session.events()) {
        eprintln!("warning: {warning}");
    }
}

/// [`OTHER_PROGRAM_WARNING`] if the session saw status reports it did not ask
/// for (the vendor site's heartbeat replies), acknowledgements of writes it
/// did not send (such as the site's empty-mask trigger write at connect) or
/// profile reports in the form of a reply to another program's `21/11`
/// query (byte 7 `01`). The keyboard's own pushes after Fn + 1..4 (byte 7
/// `00`) and its SOCD pushes are never another program. Ignored reports
/// alone do not count: late chunks of our own retried requests are ignored
/// too. A false alarm is harmless.
fn other_program_warning(stats: &SessionStats, events: &[DeviceEvent]) -> Option<&'static str> {
    let profile_report = events
        .iter()
        .any(|e| matches!(e, DeviceEvent::ProfileReport { extra, .. } if *extra != 0x00));
    (stats.status_reports > 0 || stats.foreign_acks > 0 || profile_report)
        .then_some(OTHER_PROGRAM_WARNING)
}

/// `text` with every control character except the newline replaced by
/// U+FFFD, so that strings from a device or a backup file cannot inject
/// terminal escape sequences.
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() && c != '\n' {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

/// Writes to stdout (see [`printable`]); a closed pipe (e.g. `| head`) is
/// not an error.
fn emit(text: &str) -> anyhow::Result<()> {
    let text = printable(text);
    let mut stdout = std::io::stdout().lock();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

/// Writes a view as pretty JSON, with the [`JSON_NOTE`] and the replies
/// that could not be decoded.
fn emit_json(view: &impl serde::Serialize, undecoded: Vec<UndecodedNote>) -> anyhow::Result<()> {
    let output = JsonOutput {
        view,
        note: JSON_NOTE,
        undecoded,
    };
    let mut text = serde_json::to_string_pretty(&output)?;
    text.push('\n');
    emit(&text)
}

fn report_error(err: &anyhow::Error) {
    let top = printable(&err.to_string());
    eprintln!("error: {top}");
    for cause in err.chain().skip(1) {
        let cause = printable(&cause.to_string());
        if !top.contains(&cause) {
            eprintln!("  caused by: {cause}");
        }
    }
    if let Some(hint) = hint(err) {
        eprintln!("hint: {hint}");
    }
}

/// Advice for the errors a user can do something about.
fn hint(err: &anyhow::Error) -> Option<&'static str> {
    err.chain().find_map(|cause| {
        if let Some(e) = cause.downcast_ref::<DeviceError>() {
            device_hint(e)
        } else if let Some(e) = cause.downcast_ref::<RestoreError>() {
            restore_hint(e)
        } else if let Some(e) = cause.downcast_ref::<ProfilesError>() {
            profiles_hint(&e.failure)
        } else if let Some(e) = cause.downcast_ref::<ProfilesFailure>() {
            profiles_hint(e)
        } else if let Some(e) = cause.downcast_ref::<WriteError>() {
            write_hint(e)
        } else if let Some(e) = cause.downcast_ref::<EditError>() {
            edit_hint(e)
        } else if let Some(e) = cause.downcast_ref::<BackupError>() {
            backup_hint(e)
        } else if let Some(e) = cause.downcast_ref::<SessionError>() {
            session_hint(e)
        } else {
            cause
                .downcast_ref::<TransportError>()
                .and_then(transport_hint)
        }
    })
}

fn restore_hint(err: &RestoreError) -> Option<&'static str> {
    match err {
        RestoreError::FirmwareMismatch { .. } => Some(
            "if the keyboard's firmware was updated since the backup was made, add \
             --allow-firmware-mismatch and check the plan carefully before answering",
        ),
        RestoreError::ProfileNotInBackup { .. } => Some(
            "`openwraith info --from FILE` shows which profiles a backup holds; \
             `openwraith backup --all-profiles` saves all four (`profile import --profile N` \
             picks one of them)",
        ),
        RestoreError::InvalidBackup { .. } => {
            Some("restore needs a backup written by `openwraith backup` (*.owbackup.json)")
        }
        RestoreError::Profiles(e) => profiles_hint(&e.failure),
        RestoreError::Backup(e) => backup_hint(e),
        RestoreError::ModelMismatch { .. }
        | RestoreError::SafetyBackupMismatch { .. }
        | RestoreError::InvalidRequest { .. }
        | RestoreError::Cancelled => None,
    }
}

fn profiles_hint(err: &ProfilesFailure) -> Option<&'static str> {
    match err {
        ProfilesFailure::Backup(e) => backup_hint(e),
        ProfilesFailure::Write(e) => write_hint(e),
        ProfilesFailure::UnknownActiveProfile { .. }
        | ProfilesFailure::ProductChanged { .. }
        | ProfilesFailure::Cancelled => None,
    }
}

fn write_hint(err: &WriteError) -> Option<&'static str> {
    match err {
        WriteError::OtherProgramActive { .. } => Some(
            "close every browser tab of the vendor configurator (close the tab itself; a tab in \
             the background may not be detected) and other keyboard software, then run the \
             command again",
        ),
        WriteError::KeyboardReset { .. } => Some(
            "the keyboard reset itself (Fn + Space held for a few seconds); OpenWraith did not \
             answer it and sent nothing afterwards. Check the keyboard with `openwraith profile` \
             and `openwraith show`, and restore the safety backup if needed",
        ),
        WriteError::Disconnected { .. } => Some(
            "the connection was lost; reconnect the keyboard, check it with `openwraith show`, \
             and restore the safety backup if needed",
        ),
        WriteError::VerifyFailed { .. } | WriteError::RepeatedWrite { .. } => Some(
            "the keyboard did not keep the written values; restore the safety backup and please \
             report this together with the output of the same command with --verbose",
        ),
        WriteError::CommitTriggerIncomplete { .. } => Some(
            "the keyboard did not acknowledge every packet of the Fn keymap, which it needs to \
             save the base keymap; check the USB connection, then run the same command \
             again or undo it with the printed command",
        ),
        WriteError::ProfileChangedBeforeSave { .. } => Some(
            "the keyboard switched profiles before it had saved the last write, so it may have \
             discarded it: select the written profile with `openwraith profile select N` and \
             check it with `openwraith show`, then run the command again or undo it with the \
             printed command. Do not switch profiles while OpenWraith writes or waits for the \
             keyboard to save",
        ),
        WriteError::ProfileNotSelected { .. }
        | WriteError::ProfileChanged { .. }
        | WriteError::InvalidProfile { .. } => Some(
            "do not switch profiles on the keyboard (or in another program) while OpenWraith \
             writes; check the active profile with `openwraith profile`",
        ),
        WriteError::Session(e) => session_hint(e),
        WriteError::NotBackedUp { .. } => {
            Some("save a safety backup of the active profile first; nothing was written")
        }
        WriteError::TravelTestMaybeOpen | WriteError::TravelTestOpen { .. } => Some(
            "a live key-depth test may still be running on the keyboard; stop it with \
             `openwraith travel --close`, then run the command again",
        ),
        WriteError::ChangedOnKeyboard { .. } => Some(
            "the setting was changed on the keyboard while it was written; check it with \
             `openwraith show` and run the command again",
        ),
        WriteError::EgressRefused { .. } => Some(
            "OpenWraith refused to send a packet it does not allow; nothing more was sent. \
             Please report this together with the output of the same command with --verbose",
        ),
        WriteError::Build(_)
        | WriteError::UnsupportedModel { .. }
        | WriteError::TravelOpenNotAcked
        | WriteError::BaseKeymapUnsaved
        | WriteError::ConsentOnlyForSelects => None,
    }
}

fn edit_hint(err: &EditError) -> Option<&'static str> {
    match err {
        EditError::Placement(PlacementError::BuiltinNotAcknowledged {
            critical: false, ..
        }) => Some(
            "this Fn-layer key has a built-in Fn function (see `openwraith show keymap`); add \
                 --allow-builtin KEY to replace it in this profile, or choose another key",
        ),
        EditError::Placement(PlacementError::BuiltinNotAcknowledged { critical: true, .. }) => {
            Some(
                "this Fn-layer key switches profiles or resets the keyboard; replacing it needs \
                 --allow-critical-builtin KEY (only on a spare profile; setting the entry back \
                 to default brings the built-in back)",
            )
        }
        EditError::Placement(_) => Some(
            "the knob, the Fn key and the keys the keyboard locks on the Fn layer cannot be \
             reassigned; choose another key",
        ),
        EditError::NoFreeSlot { .. } => Some(
            "every slot of that kind is used; `openwraith show macros` / `show advanced` list \
             them, and an unbound one can be cleared with `--slot N --clear`",
        ),
        EditError::SlotAlreadyBound { .. } => Some(
            "one slot per key, as the vendor site does: leave out --slot (OpenWraith picks a free \
             one) or choose a slot no other key uses",
        ),
        EditError::BindingToEmptySlot { .. } | EditError::SlotStillReferenced { .. } => Some(
            "a key must never refer to an empty slot: bind it with its content (set macro KEY \
             --steps ..., set advanced KEY KIND ...) or unbind the key first",
        ),
        EditError::UnsupportedCurrentTable { .. } => Some(
            "the keyboard holds this table in a form OpenWraith cannot write back; replace it \
             completely or restore a backup; nothing was written",
        ),
        EditError::DistanceUnit { .. } => {
            Some("OpenWraith only writes the 0.02 mm unit; nothing was written")
        }
        EditError::Build(e) => build_hint(e),
        EditError::Session(e) => session_hint(e),
        EditError::Backup(e) => backup_hint(e),
        EditError::Restore(e) => restore_hint(e),
        EditError::Invalid { .. } => None,
    }
}

fn build_hint(err: &WriteBuildError) -> Option<&'static str> {
    match err {
        WriteBuildError::DksPointOutOfRange { .. } => Some(
            "the DKS press point is given in mm in steps of 0.1 (e.g. --point 1.5) and must lie \
             above the keyboard's deep point (3.0 mm on the W75)",
        ),
        WriteBuildError::DksSteps { .. } => Some(
            "DKS stages: 1st . t h, 2nd . t h r (r only after h), 3rd . t h r (r only after h or \
             r), 4th . t; every row needs at least one",
        ),
        WriteBuildError::MtThresholdOutOfRange { .. } => {
            Some("the Mod-Tap time is 150 ms to 1000 ms (--time 300ms)")
        }
        WriteBuildError::SocdDuplicateKey { .. } | WriteBuildError::SocdSameKey { .. } => Some(
            "a SOCD pair needs two different keys, and no key may be in two pairs (see \
             `openwraith show socd`)",
        ),
        WriteBuildError::TooManySocdPairs { .. } => Some("at most 20 SOCD pairs"),
        _ => None,
    }
}

fn device_hint(err: &DeviceError) -> Option<&'static str> {
    match err {
        DeviceError::NotFound => Some(
            "connect the Wraith W75 with its USB cable and try again; \
             `openwraith list` shows what is detected",
        ),
        DeviceError::UnknownPath(_) => Some("copy the path exactly as `openwraith list` prints it"),
        DeviceError::Ambiguous { .. } => None,
    }
}

fn backup_hint(err: &BackupError) -> Option<&'static str> {
    match err {
        BackupError::Session(e) => session_hint(e),
        BackupError::ProfileChanged { .. } => Some(
            "do not switch profiles (on the keyboard or in the vendor configurator) while a \
             backup is running",
        ),
        BackupError::AlreadyExists { .. } => {
            Some("choose another name with -o FILE, or add --force to overwrite")
        }
        BackupError::Json(_)
        | BackupError::NotABackup { .. }
        | BackupError::DuplicateEntry { .. }
        | BackupError::InvalidSnapshot { .. } => {
            Some("expected a file written by `openwraith backup` (*.owbackup.json)")
        }
        BackupError::SafetyDirUnavailable { .. } => Some(
            "choose a directory for safety backups with --safety-dir DIR or the \
             OPENWRAITH_SAFETY_DIR environment variable; nothing was written",
        ),
        BackupError::SafetyCheckFailed { .. } => Some(
            "the safety backup could not be read back identically, so nothing was written; \
             check the disk or choose another directory with --safety-dir",
        ),
        BackupError::UnsupportedModel { .. }
        | BackupError::UnsupportedSchema { .. }
        | BackupError::Io { .. }
        | BackupError::TooLarge { .. }
        | BackupError::Cancelled => None,
    }
}

fn session_hint(err: &SessionError) -> Option<&'static str> {
    match err {
        SessionError::Transport(e) => transport_hint(e),
        SessionError::Timeout { .. } => Some(
            "nothing was changed on the keyboard; close the vendor web configurator and other \
             keyboard software, reconnect the keyboard and try again",
        ),
        SessionError::Inconsistent { .. } => Some(
            "the keyboard's settings changed while they were being read (another program?); \
             nothing was saved. Close other keyboard software and try again",
        ),
        SessionError::KeyboardReset { .. } => Some(
            "the keyboard reset itself (Fn + Space held for a few seconds); OpenWraith did not \
             answer it and sent nothing. Values read before the reset may be outdated: run the \
             command again once the keyboard is idle",
        ),
        SessionError::Response(_) | SessionError::Decode { .. } => Some(
            "the keyboard answered in an unexpected way; please report this together with the \
             output of the same command with --verbose",
        ),
    }
}

fn transport_hint(err: &TransportError) -> Option<&'static str> {
    match err {
        TransportError::Open { .. } => Some(
            "close other programs that may hold the keyboard and try again; on Linux your user \
             needs access to the keyboard's /dev/hidraw device (see the udev rule in README.md)",
        ),
        TransportError::Write(_) | TransportError::Read(_) => {
            Some("the connection was lost; check the USB cable, reconnect and try again")
        }
        TransportError::Interrupted => Some(
            "reading was interrupted (Ctrl+C); check the keyboard with `openwraith profile` and \
             `openwraith show`",
        ),
        TransportError::Init(_) | TransportError::Malformed(_) | TransportError::Refused(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use openwraith_protocol::ReadRequest;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    /// `set ... -h` and `--help` say what `--check-persistence auto` checks
    /// today: macro slots and SOCD are known to be kept (H19, H21, H22).
    #[test]
    fn the_check_persistence_help_matches_what_auto_checks() {
        for what in ["socd", "macro", "key", "advanced", "deadband", "colors"] {
            let mut command = Cli::command();
            let set = command.find_subcommand_mut("set").expect("set");
            let sub = set.find_subcommand_mut(what).expect(what);
            for help in [
                sub.render_help().to_string(),
                sub.render_long_help().to_string(),
            ] {
                let help = help.split_whitespace().collect::<Vec<_>>().join(" ");
                assert!(
                    help.contains(
                        "auto = only tables not yet proven on hardware (today only \
                         advanced-key slots written without the Fn keymap before them)"
                    ),
                    "set {what}: {help}"
                );
                assert!(!help.contains("macro slots, SOCD"), "set {what}: {help}");
            }
        }
    }

    #[test]
    fn parses_subcommands() {
        let cli = Cli::try_parse_from(["openwraith", "show", "triggers", "--from", "x.json"])
            .expect("parses");
        assert!(matches!(
            cli.command,
            Command::Show {
                what: Section::Triggers,
                json: false,
                ..
            }
        ));
        let cli = Cli::try_parse_from(["openwraith", "backup", "-o", "b.json", "--device", "p"])
            .expect("parses");
        assert_eq!(cli.device.as_deref(), Some("p"));
        assert!(Cli::try_parse_from(["openwraith", "show", "colours"]).is_err());
        // No generic write, reset or calibration command; `set` and
        // `restore` need arguments.
        for name in [
            "set",
            "write",
            "restore",
            "reset",
            "calibrate",
            "reset-triggers",
        ] {
            assert!(Cli::try_parse_from(["openwraith", name]).is_err(), "{name}");
        }
        // `set` covers only the confirmed writes: the forbidden ones do not
        // exist as subcommands at all...
        for what in [
            "poll-rate",
            "switch",
            "side-light",
            "keymap",
            "calibration",
            "gamepad",
            "win-lock",
            "reset",
        ] {
            let err = Cli::try_parse_from(["openwraith", "set", what, "--yes"])
                .expect_err("not a subcommand");
            assert_eq!(
                err.kind(),
                clap::error::ErrorKind::InvalidSubcommand,
                "set {what}: {err}"
            );
        }
        // ...while the v0.2 editors exist and only need their arguments.
        for what in ["socd", "deadband", "key", "colors", "macro"] {
            let err = Cli::try_parse_from(["openwraith", "set", what, "--yes"])
                .expect_err("arguments missing");
            assert_eq!(
                err.kind(),
                clap::error::ErrorKind::MissingRequiredArgument,
                "set {what}: {err}"
            );
        }
    }

    #[test]
    fn parses_the_write_commands() {
        let parse = |args: &[&str]| {
            Cli::try_parse_from(std::iter::once("openwraith").chain(args.iter().copied()))
        };
        let cli = parse(&[
            "restore",
            "b.json",
            "--profile",
            "2",
            "--profile",
            "4",
            "--yes",
        ])
        .expect("parses");
        let Command::Restore(args) = cli.command else {
            panic!("not restore");
        };
        assert_eq!(args.profiles, [2, 4]);
        assert!(args.yes && !args.dry_run);
        assert!(parse(&["restore", "b.json", "--dry-run", "--against", "c.json"]).is_ok());
        assert!(parse(&["profile"]).is_ok());
        assert!(parse(&["profile", "select", "4"]).is_ok());
        assert!(parse(&["backup", "--all-profiles", "-o", "x.json"]).is_ok());
        assert!(parse(&["set", "trigger", "W", "Q", "--actuation", "1.2"]).is_ok());
        assert!(parse(&["set", "trigger", "W", "--rapid-trigger", "0.3/0.5"]).is_ok());
        assert!(parse(&["set", "light", "--mode", "static", "--color", "0000ff"]).is_ok());
        assert!(parse(&["show", "all", "--from", "b.json", "--profile", "3"]).is_ok());
        for bad in [
            // --against only with --dry-run; --yes makes no sense with it.
            &["restore", "b.json", "--against", "c.json"][..],
            &["restore", "b.json", "--dry-run", "--yes"],
            &["restore", "b.json", "--profile", "5"],
            &["profile", "select", "0"],
            &["profile", "select", "5"],
            // Nothing to change, or contradicting options.
            &["set", "trigger", "W"],
            &["set", "trigger", "--actuation", "1.2"],
            &[
                "set",
                "trigger",
                "W",
                "--rapid-trigger",
                "0.3",
                "--no-rapid-trigger",
            ],
            &["set", "light"],
            &["set", "light", "--mode", "musical-rhythm"],
            // Values that cannot be written exactly, and the knob.
            &["set", "trigger", "W", "--actuation", "1.21"],
            &["set", "trigger", "Knob", "--actuation", "1.2"],
            &["set", "trigger", "NoSuchKey", "--actuation", "1.2"],
            // A profile of a backup file only.
            &["show", "triggers", "--profile", "2"],
            &["info", "--profile", "2"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    /// `text` split into words like a shell would (double quotes group).
    fn shell_words(text: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut quoted = false;
        let mut started = false;
        for c in text.chars() {
            match c {
                '"' => {
                    quoted = !quoted;
                    started = true;
                }
                c if c.is_whitespace() && !quoted => {
                    if started {
                        words.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                c => {
                    word.push(c);
                    started = true;
                }
            }
        }
        if started {
            words.push(word);
        }
        words
    }

    #[test]
    fn the_documented_commands_parse() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for doc in ["README.md", "docs/HARDWARE_TESTS.md"] {
            let text = std::fs::read_to_string(root.join(doc)).expect("readable");
            let mut in_block = false;
            let mut checked = 0;
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("```") {
                    in_block = !in_block;
                    continue;
                }
                if !in_block || !trimmed.starts_with("openwraith ") {
                    continue;
                }
                // Without the comment and a redirection.
                let command = trimmed.split(" # ").next().unwrap_or(trimmed);
                let command = command.split(" > ").next().unwrap_or(command).trim();
                let words = shell_words(command);
                if let Err(e) = Cli::try_parse_from(&words) {
                    panic!("{doc}: {command}\n{e}");
                }
                checked += 1;
            }
            assert!(checked >= 20, "{doc}: only {checked} commands found");
        }
    }

    #[test]
    fn profiles_of_an_all_profile_backup_can_be_shown() {
        use std::ops::ControlFlow;
        let mut session = Session::new(test_support::sim_with_profiles());
        let all = openwraith_hid::read_all_profiles(
            &mut session,
            openwraith_hid::WriteConsent::new(openwraith_hid::ConsentOrigin::CommandLineFlag),
            &openwraith_hid::ProfilesOptions::default(),
            |_| ControlFlow::Continue(()),
        )
        .expect("all-profile backup");
        let (primary, note) = select_profile(all.clone(), None).expect("primary");
        assert_eq!(primary, all);
        assert!(note.is_some_and(|n| n.contains("profiles 1, 2, 3, 4")));
        let (second, note) = select_profile(all.clone(), Some(2)).expect("profile 2");
        assert!(note.is_some_and(|n| n.contains("showing profile 2")));
        assert_eq!(second.active_profile, Some(2));
        assert!(second.profiles.is_empty());
        assert!(!second.is_all_profiles());
        // Profile 2 has factory triggers, profile 1 has W at 1.00 mm.
        let w = |b: &Backup| b.key_trigger(46).map(|t| t.actuation.units());
        assert_eq!(w(&primary), Some(0x32));
        assert_eq!(w(&second), Some(0x55));
        let (same, note) = select_profile(all, Some(1)).expect("profile 1");
        assert_eq!(same.active_profile, Some(1));
        assert!(note.is_none());
        // A single-profile backup only holds its own profile.
        let single = test_support::c06_backup();
        let err = select_profile(single, Some(3)).expect_err("no profile 3");
        assert!(is_usage_error(&err));
        assert!(err.to_string().contains("--all-profiles"), "{err}");
    }

    #[test]
    fn hints_reach_nested_errors() {
        let err: anyhow::Error = BackupError::Session(SessionError::KeyboardReset {
            request: ReadRequest::Status,
            report: "14 00 00 00 01 01".into(),
        })
        .into();
        assert!(hint(&err).is_some_and(|h| h.contains("did not answer")));
        let err: anyhow::Error = DeviceError::NotFound.into();
        assert!(hint(&err).is_some());
        let err: anyhow::Error = BackupError::Session(SessionError::Inconsistent {
            request: ReadRequest::Status,
        })
        .into();
        assert!(hint(&err).is_some_and(|h| h.contains("nothing was saved")));
        let err: anyhow::Error = BackupError::ProfileChanged {
            before: 1,
            after: 2,
        }
        .into();
        assert!(hint(&err).is_some_and(|h| h.contains("vendor configurator")));
    }

    #[test]
    fn control_characters_never_reach_the_terminal() {
        let text = printable("Firmware 9.99\u{1b}[5m\n\u{1b}]0;title\u{7}\u{9b}2J\tend\n");
        assert!(
            !text.chars().any(|c| c.is_control() && c != '\n'),
            "{text:?}"
        );
        assert_eq!(text.matches('\n').count(), 2);
        assert!(text.starts_with("Firmware 9.99\u{FFFD}[5m\n"));
        assert_eq!(printable("plain text\n"), "plain text\n");
    }

    #[test]
    fn another_program_is_detected_by_status_and_profile_reports() {
        let quiet = SessionStats::default();
        assert_eq!(other_program_warning(&quiet, &[]), None);
        // Ignored reports alone (e.g. our own late chunks) are not enough.
        let ignored = SessionStats {
            ignored: 3,
            ..SessionStats::default()
        };
        assert_eq!(other_program_warning(&ignored, &[]), None);
        let heartbeat = SessionStats {
            status_reports: 1,
            ..SessionStats::default()
        };
        assert!(other_program_warning(&heartbeat, &[]).is_some());
        // Another program's 21/11 query reply (byte 7 = 01).
        let query = DeviceEvent::ProfileReport {
            profile: 2,
            extra: 1,
        };
        assert!(other_program_warning(&quiet, &[query]).is_some());
        // Fn + 2 on the keyboard (the push, byte 7 = 00) and Fn + Right
        // Shift: the user's own keys, never another program.
        let push = DeviceEvent::ProfileReport {
            profile: 2,
            extra: 0,
        };
        let socd = DeviceEvent::SocdPower {
            power: 1,
            rest: [0, 0, 0, 0xfa, 0xe5],
        };
        assert_eq!(other_program_warning(&quiet, &[push, socd]), None);
    }
}
