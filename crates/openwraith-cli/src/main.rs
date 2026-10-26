//! `openwraith`: command line tool for the Wraith W75 keyboard.
//!
//! `list`, `info`, `show` and `backup` (without `--all-profiles`) are
//! read-only: they open a transport type that cannot send a write (see the
//! `openwraith-hid` crate). The
//! commands that write (`profile select`, `set`, `restore`, and
//! `backup --all-profiles`, which only switches profiles) go through the
//! guarded write path: consent, the "other program" gate, a verified safety
//! backup before any change, stop-and-wait writes and read-back
//! verification. Their logic lives in [`commands`].

#![forbid(unsafe_code)]

mod args;
mod commands;
mod device;
mod render;
mod report;
#[cfg(test)]
mod sim_tests;
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
    BACKUP_SCHEMA_VERSION, BACKUP_SCOPE, Backup, BackupError, ProfilesError, ProfilesFailure,
    Progress, RestoreError, Session, SessionError, SessionStats, Transport, TransportError,
    WriteError, read_all_with, read_summary,
};
use openwraith_protocol::{DeviceEvent, ProfileId};

use crate::args::{ProfileAction, RestoreArgs, SetCommand};
use crate::commands::{Exit, Globals, OutputTarget, UsageError, tool_options};
use crate::device::{Connector, DeviceError, HidConnector};
use crate::ui::{Terminal, Ui};
use crate::view::{
    AllView, InfoView, JSON_NOTE, JsonOutput, KeymapView, LightView, MacrosView, TriggersView,
    UndecodedNote, undecoded_notes,
};

const ABOUT: &str = "Configuration tool for the Wraith W75 keyboard \
(not affiliated with or endorsed by Wraith)";

const LONG_ABOUT: &str = "Configuration tool for the Wraith W75 Hall-effect keyboard.

Shows device information, trigger (actuation / Rapid Trigger) settings, \
lighting, keymap and macros, saves lossless backups, and restores them.

list, info, show and backup are READ-ONLY (backup --all-profiles excepted, \
see below): they never send a command that changes, resets or calibrates the \
keyboard. These commands write to the keyboard: profile select, set, restore, \
and backup --all-profiles (profile switches only). set and restore save a \
verified safety backup first (see --safety-dir), show what will change and ask \
for confirmation (unless --yes); every write is checked by reading it back. \
profile select and backup --all-profiles only switch profiles and do not ask.

The keyboard saves a change about 1 s after it is written; a profile switch \
or unplugging it before that loses the change. Commands that write therefore \
wait 2 s after the last write before they switch profiles or finish, and \
restore reads every written profile again after switching away and back \
(\"persisted\", or \"LOST\" and exit code 1). A profile change on the keyboard \
or a lost connection before the keyboard has saved is reported as \"MAY BE \
LOST\" (exit code 1).

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
    /// The ten macro slots.
    Macros,
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
        | Command::Restore(_) => {
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
        Command::Set {
            what: SetCommand::Trigger(args),
        } => commands::set_trigger(connector, ui, globals, args),
        Command::Set {
            what: SetCommand::Light(args),
        } => commands::set_light(connector, ui, globals, args),
        Command::Restore(args) => commands::restore(connector, ui, globals, args),
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
/// unsolicited profile reports. Ignored reports alone do not count: late
/// chunks of our own retried requests are ignored too. A false alarm is
/// harmless.
fn other_program_warning(stats: &SessionStats, events: &[DeviceEvent]) -> Option<&'static str> {
    let profile_report = events
        .iter()
        .any(|e| matches!(e, DeviceEvent::ProfileReport { .. }));
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
             `openwraith backup --all-profiles` saves all four",
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
        WriteError::Build(_) | WriteError::UnsupportedModel { .. } => None,
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
        TransportError::Init(_) | TransportError::Malformed(_) => None,
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
        // `set` only covers the confirmed writes (triggers, main light).
        for what in [
            "socd",
            "poll-rate",
            "switch",
            "side-light",
            "deadband",
            "keymap",
        ] {
            assert!(
                Cli::try_parse_from(["openwraith", "set", what, "--yes"]).is_err(),
                "set {what}"
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
        let profile = DeviceEvent::ProfileReport {
            profile: 2,
            extra: 0,
        };
        assert!(other_program_warning(&quiet, &[profile]).is_some());
    }
}
