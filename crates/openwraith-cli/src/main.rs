//! `openwraith`: read-only command line tool for the Wraith W75 keyboard.
//!
//! Subcommands: `list`, `info`, `backup` and `show`. None of them can change
//! anything on the keyboard: the session API only sends read requests (see
//! the `openwraith-hid` crate).

#![forbid(unsafe_code)]

mod device;
mod render;
mod table;
#[cfg(test)]
mod test_support;
mod view;

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use openwraith_hid::{
    Backup, BackupError, BackupOptions, Progress, Session, SessionError, SessionStats, ToolInfo,
    Transport, TransportError, read_all_with, read_summary,
};
use openwraith_protocol::DeviceEvent;

use crate::device::DeviceError;
use crate::view::{
    AllView, InfoView, JSON_NOTE, JsonOutput, KeymapView, LightView, MacrosView, TriggersView,
    UndecodedNote, undecoded_notes,
};

const ABOUT: &str = "Read-only configuration tool for the Wraith W75 keyboard \
(not affiliated with or endorsed by Wraith)";

const LONG_ABOUT: &str = "Read-only configuration tool for the Wraith W75 Hall-effect keyboard.

Shows device information, trigger (actuation / Rapid Trigger) settings, \
lighting, keymap and macros, and saves lossless backups of the active onboard \
profile (other onboard profiles are not included).

This version is READ-ONLY: it never sends a command that changes, resets or \
calibrates the keyboard. Close the vendor web configurator before using it: \
it receives OpenWraith's replies too and reacts to them.

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
    },
    /// Save every setting of the active onboard profile to a backup file.
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
    },
    /// Show settings as readable tables, from the keyboard or from a backup.
    Show {
        /// What to show.
        #[arg(value_enum)]
        what: Section,
        /// Read from a backup file instead of the keyboard.
        #[arg(long, value_name = "BACKUP_FILE")]
        from: Option<PathBuf>,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
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
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            report_error(&err);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> anyhow::Result<()> {
    match &cli.command {
        Command::List => cmd_list(),
        Command::Info { json, from } => cmd_info(cli, *json, from.as_deref()),
        Command::Backup { output, force } => cmd_backup(cli, output.as_deref(), *force),
        Command::Show { what, from, json } => cmd_show(cli, *what, from.as_deref(), *json),
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

fn cmd_info(cli: &Cli, json: bool, from: Option<&Path>) -> anyhow::Result<()> {
    let (view, notes) = match from {
        Some(path) => {
            let backup = Backup::load(path)?;
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
    // `-o DIR` puts the file, under its default name, into DIR.
    let output_dir = output.filter(|p| p.is_dir());
    let output_file = output.filter(|p| !p.is_dir());
    // Fail before reading the keyboard if the chosen file is taken.
    if let Some(path) = output_file {
        if !force && path.exists() {
            return Err(BackupError::AlreadyExists {
                path: path.to_path_buf(),
            }
            .into());
        }
    }
    let mut session = device::open_session(cli.device.as_deref())?;
    let backup = read_keyboard(cli, &mut session)?;
    let path = match (output_file, output_dir) {
        (Some(file), _) => file.to_path_buf(),
        (None, Some(dir)) => dir.join(backup.default_file_name()),
        (None, None) => PathBuf::from(backup.default_file_name()),
    };
    backup.save(&path, force)?;
    emit(
        &(render::backup_summary(&backup, &path)
            + &render::undecoded_footer(&undecoded_notes(&backup))),
    )
}

fn cmd_show(cli: &Cli, what: Section, from: Option<&Path>, json: bool) -> anyhow::Result<()> {
    let backup = match from {
        Some(path) => Backup::load(path)?,
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

/// Reads everything from the keyboard, with a progress line on a terminal.
fn read_keyboard<T: Transport>(cli: &Cli, session: &mut Session<T>) -> anyhow::Result<Backup> {
    let interactive = std::io::stderr().is_terminal();
    eprintln!("Reading the keyboard (read-only)...");
    let options = BackupOptions {
        tool: ToolInfo {
            name: "openwraith".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        created_at: None,
    };
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
/// for (the vendor site's heartbeat replies) or unsolicited profile reports.
/// Ignored reports alone do not count: late chunks of our own retried
/// requests are ignored too. A false alarm is harmless.
fn other_program_warning(stats: &SessionStats, events: &[DeviceEvent]) -> Option<&'static str> {
    let profile_report = events
        .iter()
        .any(|e| matches!(e, DeviceEvent::ProfileReport { .. }));
    (stats.status_reports > 0 || profile_report).then_some(OTHER_PROGRAM_WARNING)
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
        | BackupError::DuplicateEntry { .. } => {
            Some("expected a file written by `openwraith backup` (*.owbackup.json)")
        }
        BackupError::UnsupportedModel { .. }
        | BackupError::UnsupportedSchema { .. }
        | BackupError::Io { .. } => None,
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
        // There is no write subcommand.
        for name in ["set", "write", "restore", "reset", "calibrate"] {
            assert!(Cli::try_parse_from(["openwraith", name]).is_err(), "{name}");
        }
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
