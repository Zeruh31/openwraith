//! The commands that may write to the keyboard: `profile [select N]`,
//! `backup --all-profiles` and `restore`, and the helpers every command that
//! writes shares (the `set` editors are in [`crate::edits`], `profile export`
//! / `import` in [`crate::share`], `travel` and `monitor` in
//! [`crate::live`]).
//!
//! Their logic is generic over a [`Connector`] (which opens the sessions)
//! and a [`Ui`] (console output and the confirmation question), so the
//! CLI's unit tests run exactly this code against the device simulator. The
//! binary itself only ever uses the HID connector.
//!
//! Every command that changes settings follows the same steps: read the
//! current state (read-only), save a verified safety backup, show what will
//! change, ask (unless `--yes`), write through the guarded write path of
//! `openwraith-hid` (stop-and-wait, read-back verification), and report what
//! was written and how to undo it.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use openwraith_hid::{
    Backup, BackupError, BackupOptions, ConsentOrigin, ProfileSet, ProfilesOptions,
    ProfilesProgress, Progress, RestoreError, RestoreOptions, RestorePlan, RestoreReport,
    SavedBackup, Session, ToolInfo, Transport, TravelState, WriteConsent, WriteError, WriteOptions,
    WriteProgress, WriteTransport, dry_run_restore, execute_restore, plan_restore, prepare_restore,
    read_all_cancellable, read_all_profiles, read_summary,
};
use openwraith_protocol::{Model, ProfileId};
use serde::Serialize;

use crate::args::{ProfileAction, RestoreArgs};
use crate::device::Connector;
use crate::report;
use crate::ui::Ui;
use crate::view::{JSON_NOTE, undecoded_notes};
use crate::{other_program_warning, render};

/// Options shared by every command.
#[derive(Debug, Clone, Default)]
pub struct Globals {
    /// `--verbose`: print session statistics.
    pub verbose: bool,
    /// `--safety-dir`: where safety backups go.
    pub safety_dir: Option<PathBuf>,
}

/// How a command ended (errors are returned separately).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Everything planned was done (exit code 0).
    Success,
    /// The command reported a failure or the change was not confirmed
    /// (exit code 1; the output says whether anything was written).
    Failure,
}

/// A usage error found after argument parsing (exit code 2).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct UsageError(pub String);

/// Printed before every command that writes.
pub const CLOSE_SITE_NOTE: &str = "Close every wraith.software tab first (close the tab itself: \
a tab in the background may not be detected). It receives OpenWraith's replies and writes to \
the keyboard on its own.";

/// Printed when a change was not confirmed.
pub(crate) const NOT_CONFIRMED: &str =
    "Not confirmed: nothing was written. The safety backup is kept.\n";

/// Tool and time recorded in backups made by the CLI.
pub fn tool_options() -> BackupOptions {
    BackupOptions {
        tool: ToolInfo {
            name: "openwraith".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        created_at: None,
    }
}

/// The directory for safety backups: `--safety-dir`, else
/// `OPENWRAITH_SAFETY_DIR`, else the per-user data directory. Fails before
/// anything is written if none can be determined.
pub(crate) fn safety_dir(globals: &Globals) -> anyhow::Result<PathBuf> {
    match &globals.safety_dir {
        Some(dir) => Ok(dir.clone()),
        None => Ok(openwraith_hid::safety::default_dir()?),
    }
}

/// Refuses early (before the keyboard is opened) when the user cannot be
/// asked and did not pass `--yes`.
pub(crate) fn require_prompt_or_yes(ui: &impl Ui, yes: bool) -> anyhow::Result<()> {
    if yes || ui.can_prompt() {
        return Ok(());
    }
    Err(UsageError(
        "this command asks for confirmation before writing, but stdin is not a terminal; add \
         --yes to confirm in advance. Nothing was read or written."
            .to_string(),
    )
    .into())
}

/// The user's consent: `--yes`, or a "y" answer. `None` when declined or
/// cancelled.
pub(crate) fn confirm(
    ui: &mut impl Ui,
    yes: bool,
    question: &str,
) -> anyhow::Result<Option<WriteConsent>> {
    if ui.cancelled() {
        return Ok(None);
    }
    if yes {
        return Ok(Some(WriteConsent::new(ConsentOrigin::CommandLineFlag)));
    }
    if !ui.can_prompt() {
        return Err(UsageError(
            "cannot ask for confirmation (stdin is not a terminal); add --yes".to_string(),
        )
        .into());
    }
    let answer = ui.confirm(question)?;
    Ok((answer && !ui.cancelled()).then(|| WriteConsent::new(ConsentOrigin::InteractivePrompt)))
}

/// After a command that talked to the keyboard: statistics with
/// `--verbose`, and a warning if another program was active.
pub(crate) fn report_session<T: Transport>(
    ui: &mut impl Ui,
    globals: &Globals,
    session: &Session<T>,
) {
    if globals.verbose {
        let stats = serde_json::to_string(session.stats()).unwrap_or_default();
        ui.note(&format!("session statistics: {stats}"));
    }
    if let Some(warning) = other_program_warning(session.stats(), session.events()) {
        ui.note(&format!("warning: {warning}"));
    }
}

/// Refuses before the safety backup and the question when the session saw
/// another program, or key-depth reports of a test left open (the writer's
/// gate would refuse later anyway).
pub(crate) fn ensure_quiet<T: Transport>(session: &Session<T>) -> anyhow::Result<()> {
    let evidence = session.foreign_activity();
    if !evidence.is_quiet() {
        return Err(WriteError::OtherProgramActive { evidence }.into());
    }
    if session.travel_state() == TravelState::MaybeOpen {
        return Err(WriteError::TravelTestMaybeOpen.into());
    }
    match session.stray_travel_since_gate() {
        0 => Ok(()),
        stray_reports => Err(WriteError::TravelTestOpen { stray_reports }.into()),
    }
}

/// Where the active profile is now, after a failure.
pub(crate) fn note_active_profile(ui: &mut impl Ui, now: Option<u8>) {
    match now {
        Some(p) => ui.note(&format!("The keyboard is on profile {p} now.")),
        None => ui.note(
            "The active profile is not known now; check it with `openwraith profile` (it may \
             have been switched).",
        ),
    }
}

pub(crate) fn read_progress_text(p: &Progress) -> String {
    format!("reading {:>3}/{:<3} {}", p.done, p.total, p.request)
}

fn profiles_progress_text(p: &ProfilesProgress) -> String {
    match p {
        ProfilesProgress::Reading {
            profile,
            done,
            total,
        } => {
            let which =
                profile.map_or_else(|| "active profile".to_string(), |p| format!("profile {p}"));
            format!("reading {which}: {done}/{total}")
        }
        ProfilesProgress::Selecting { profile } => format!("switching to profile {profile}"),
        ProfilesProgress::Returning { profile } => format!("switching back to profile {profile}"),
        ProfilesProgress::WaitingForCommit { wait_ms } => report::commit_wait_text(None, *wait_ms),
    }
}

fn write_progress_text(p: &WriteProgress) -> String {
    match p {
        WriteProgress::SafetyBackup {
            progress: ProfilesProgress::WaitingForCommit { wait_ms },
        } => report::commit_wait_text(None, *wait_ms),
        WriteProgress::SafetyBackup { progress } => {
            format!("safety backup: {}", profiles_progress_text(progress))
        }
        WriteProgress::Planning => "comparing".to_string(),
        WriteProgress::Writing {
            profile,
            table,
            key,
            packet,
            of,
            send,
        } => {
            let key = key.map(|k| format!(" {k}")).unwrap_or_default();
            let retry = if *send > 1 { " (sending it again)" } else { "" };
            if *packet == 0 {
                format!("profile {profile}: writing {table}{key}{retry}")
            } else {
                format!("profile {profile}: writing {table}{key}, packet {packet}/{of}{retry}")
            }
        }
        WriteProgress::Verifying => "reading the written profiles again".to_string(),
        WriteProgress::Returning { profile } => format!("switching back to profile {profile}"),
        WriteProgress::WaitingForCommit { profile, wait_ms } => {
            report::commit_wait_text(*profile, *wait_ms)
        }
        WriteProgress::CheckingPersistence { profile } => {
            format!("profile {profile}: reading it again after a profile switch")
        }
    }
}

/// Shows progress and turns a Ctrl+C into a cancellation request.
pub(crate) fn step(ui: &mut impl Ui, text: &str) -> ControlFlow<()> {
    ui.progress(text);
    if ui.cancelled() {
        ControlFlow::Break(())
    } else {
        ControlFlow::Continue(())
    }
}

/// Whether a progress event is a wait for the keyboard to save.
fn is_commit_wait(p: &WriteProgress) -> bool {
    matches!(
        p,
        WriteProgress::WaitingForCommit { .. }
            | WriteProgress::SafetyBackup {
                progress: ProfilesProgress::WaitingForCommit { .. }
            }
    )
}

/// The progress of an operation that writes: a wait for the keyboard to
/// save (up to 2 s, when the output would otherwise stand still) is a note
/// that stays, everything else a progress line.
pub(crate) fn write_step(ui: &mut impl Ui, p: &WriteProgress) -> ControlFlow<()> {
    if is_commit_wait(p) {
        ui.note(&write_progress_text(p));
        return if ui.cancelled() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        };
    }
    step(ui, &write_progress_text(p))
}

/// The progress of an all-profile backup (see [`write_step`]).
pub(crate) fn profiles_step(ui: &mut impl Ui, p: &ProfilesProgress) -> ControlFlow<()> {
    if matches!(p, ProfilesProgress::WaitingForCommit { .. }) {
        return write_step(ui, &WriteProgress::SafetyBackup { progress: *p });
    }
    step(ui, &profiles_progress_text(p))
}

/// Reads the active profile (read-only) and saves it as a verified safety
/// backup.
pub(crate) fn safety_backup<T: Transport>(
    session: &mut Session<T>,
    ui: &mut impl Ui,
    dir: &Path,
    reason: &str,
) -> anyhow::Result<SavedBackup> {
    ui.note("Making a safety backup of the active profile (read-only)...");
    let backup = read_all_cancellable(session, &tool_options(), |p| {
        step(ui, &read_progress_text(&p))
    });
    ui.clear_progress();
    let saved = backup?.save_safety(dir, reason)?;
    ui.note(&format!("Safety backup saved: {}", saved.path().display()));
    Ok(saved)
}

// ----- profile -----

/// `openwraith profile` (read-only), `openwraith profile select N`, and
/// `profile export` / `profile import` (in [`crate::share`]).
pub fn profile<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    action: Option<&ProfileAction>,
) -> anyhow::Result<Exit> {
    match action {
        None => {
            let mut session = connector.open_read()?;
            let result = read_summary(&mut session);
            report_session(ui, globals, &session);
            let summary = result?;
            let device = &summary.device;
            ui.print(&format!(
                "{} ({}), firmware {}\nActive profile: {} (onboard profiles 1-4)\n",
                device.model.as_deref().unwrap_or("Unknown model"),
                device.product.as_deref().unwrap_or("?"),
                device.firmware.as_deref().unwrap_or("unknown"),
                summary.active_profile
            ))?;
            Ok(Exit::Success)
        }
        Some(ProfileAction::Select { profile }) => {
            let target = ProfileId::new(*profile)
                .ok_or_else(|| UsageError(format!("profile {profile} does not exist (1-4)")))?;
            ui.note(CLOSE_SITE_NOTE);
            let mut session = connector.open_write()?;
            let result = select_profile(&mut session, ui, target);
            report_session(ui, globals, &session);
            result
        }
        Some(ProfileAction::Export(args)) => crate::share::export(connector, ui, globals, args),
        Some(ProfileAction::Import(args)) => crate::share::import(connector, ui, globals, args),
    }
}

fn select_profile<T: WriteTransport>(
    session: &mut Session<T>,
    ui: &mut impl Ui,
    target: ProfileId,
) -> anyhow::Result<Exit> {
    // Model check and the current profile, read-only.
    let summary = read_summary(session)?;
    if summary.active_profile == target.get() {
        ui.print(&format!(
            "Profile {target} is already active; nothing was sent.\n"
        ))?;
        return Ok(Exit::Success);
    }
    let mut writer = session.writer(WriteConsent::new(ConsentOrigin::CommandLineFlag))?;
    let outcome = writer.select_profile(target)?;
    for warning in &outcome.warnings {
        ui.note(&format!("note: {}", report::write_warning_text(warning)));
    }
    ui.print(&format!(
        "Switched from profile {} to profile {} (confirmed by reading the active profile).\n",
        summary.active_profile, outcome.profile
    ))?;
    Ok(Exit::Success)
}

// ----- backup --all-profiles -----

/// Where `backup` saves: `-o FILE`, `-o DIR` (default name in DIR) or the
/// default name in the current directory.
#[derive(Debug, Clone)]
pub struct OutputTarget {
    file: Option<PathBuf>,
    dir: Option<PathBuf>,
}

impl OutputTarget {
    /// Checks `-o` before the keyboard is read: an existing file is refused
    /// unless `force`.
    pub fn check(output: Option<&Path>, force: bool) -> anyhow::Result<Self> {
        let dir = output.filter(|p| p.is_dir()).map(Path::to_path_buf);
        let file = output.filter(|p| !p.is_dir()).map(Path::to_path_buf);
        if let Some(path) = &file
            && !force
            && path.exists()
        {
            return Err(BackupError::AlreadyExists { path: path.clone() }.into());
        }
        Ok(OutputTarget { file, dir })
    }

    /// The file to write `backup` to.
    pub fn path_for(&self, backup: &Backup) -> PathBuf {
        match (&self.file, &self.dir) {
            (Some(file), _) => file.clone(),
            (None, Some(dir)) => dir.join(backup.default_file_name()),
            (None, None) => PathBuf::from(backup.default_file_name()),
        }
    }
}

/// `openwraith backup --all-profiles`: reads all four onboard profiles
/// (switching profiles and back) into a schema-2 backup.
pub fn backup_all_profiles<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    output: Option<&Path>,
    force: bool,
) -> anyhow::Result<Exit> {
    let target = OutputTarget::check(output, force)?;
    ui.note(CLOSE_SITE_NOTE);
    ui.enable_cancel();
    let mut session = connector.open_write()?;
    let result = backup_all_profiles_on(&mut session, ui, &target, force);
    report_session(ui, globals, &session);
    result
}

fn backup_all_profiles_on<T: WriteTransport>(
    session: &mut Session<T>,
    ui: &mut impl Ui,
    target: &OutputTarget,
    force: bool,
) -> anyhow::Result<Exit> {
    let summary = read_summary(session)?;
    ui.note(&format!(
        "Reading onboard profiles 1-4. This switches profiles (the lighting changes briefly) \
         and returns to profile {}; nothing else is written.",
        summary.active_profile
    ));
    let options = ProfilesOptions {
        backup: tool_options(),
        write: WriteOptions::default(),
    };
    let consent = WriteConsent::new(ConsentOrigin::CommandLineFlag);
    let result = read_all_profiles(session, consent, &options, |p| profiles_step(ui, &p));
    ui.clear_progress();
    let backup = match result {
        Ok(backup) => backup,
        Err(e) => {
            note_active_profile(ui, e.active_profile_now);
            return Err(e.into());
        }
    };
    let path = target.path_for(&backup);
    backup.save(&path, force)?;
    ui.print(
        &(render::backup_summary(&backup, &path)
            + &render::undecoded_footer(&undecoded_notes(&backup))),
    )?;
    Ok(Exit::Success)
}

// ----- restore -----

/// How a restore (or an import) runs: the options of `restore` and
/// `profile import` that are not [`RestoreOptions`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct RestoreRun<'a> {
    /// The file that is restored.
    pub file: &'a Path,
    /// Only compare and show the plan.
    pub dry_run: bool,
    /// With `dry_run`: compare with this file instead of the keyboard.
    pub against: Option<&'a Path>,
    /// Do not ask.
    pub yes: bool,
    /// JSON output.
    pub json: bool,
    /// Print every packet.
    pub packets: bool,
    /// The safety backup's reason (`restore`, `import`).
    pub reason: &'static str,
}

/// JSON output of `restore --json`.
#[derive(Debug, Serialize)]
struct RestoreJson<'a> {
    /// The backup that is restored.
    backup: String,
    /// Whether this was a dry run (nothing written).
    dry_run: bool,
    /// The file compared with instead of the keyboard (`--against`).
    #[serde(skip_serializing_if = "Option::is_none")]
    against: Option<String>,
    /// The safety backup made before writing.
    #[serde(skip_serializing_if = "Option::is_none")]
    safety_backup: Option<&'a SavedBackup>,
    /// The plan.
    plan: &'a RestorePlan,
    /// The packets of every planned write (`--packets`).
    #[serde(skip_serializing_if = "Option::is_none")]
    packets: Option<Vec<report::PacketGroup>>,
    /// Whether the user confirmed the writes (absent for a dry run or
    /// when there was nothing to write).
    #[serde(skip_serializing_if = "Option::is_none")]
    confirmed: Option<bool>,
    /// What was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<&'a RestoreReport>,
    /// Confidence note of every JSON output.
    note: &'static str,
}

/// `openwraith restore FILE ...`.
pub fn restore<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &RestoreArgs,
) -> anyhow::Result<Exit> {
    let target = Backup::load(&args.file)?;
    // Before the keyboard is opened.
    let held = target.profile_numbers();
    if let Some(missing) = args.profiles.iter().find(|p| !held.contains(p)) {
        return Err(RestoreError::ProfileNotInBackup { profile: *missing }.into());
    }
    let profiles = if args.profiles.is_empty() {
        None
    } else {
        Some(
            args.profiles
                .iter()
                .filter_map(|p| ProfileId::new(*p))
                .collect::<ProfileSet>(),
        )
    };
    let options = RestoreOptions {
        profiles,
        allow_firmware_mismatch: args.allow_firmware_mismatch,
        backup: tool_options(),
        write: WriteOptions::default(),
        import: None,
    };
    let run = RestoreRun {
        file: &args.file,
        dry_run: args.dry_run,
        against: args.against.as_deref(),
        yes: args.yes,
        json: args.json,
        packets: args.packets,
        reason: "restore",
    };
    run_restore(connector, ui, globals, &target, &options, &run)
}

/// Runs a prepared restore (or import) of `target`: a dry run, or the
/// guarded restore with its safety backup and question.
pub(crate) fn run_restore<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    target: &Backup,
    options: &RestoreOptions,
    run: &RestoreRun<'_>,
) -> anyhow::Result<Exit> {
    if run.dry_run {
        return restore_dry_run(connector, ui, globals, run, target, options);
    }
    require_prompt_or_yes(ui, run.yes)?;
    let dir = safety_dir(globals)?;
    ui.note(CLOSE_SITE_NOTE);
    ui.enable_cancel();
    let mut session = connector.open_write()?;
    let result = restore_on(&mut session, ui, run, target, options, &dir);
    report_session(ui, globals, &session);
    result
}

fn restore_dry_run<C: Connector>(
    connector: &mut C,
    ui: &mut impl Ui,
    globals: &Globals,
    args: &RestoreRun<'_>,
    target: &Backup,
    options: &RestoreOptions,
) -> anyhow::Result<Exit> {
    let plan = match args.against {
        Some(path) => {
            let current = Backup::load(path)?;
            plan_restore(target, &current, options)?
        }
        None => {
            ui.note(CLOSE_SITE_NOTE);
            ui.note("Reading the keyboard's active profile (read-only; nothing is written)...");
            // A read-only session: this transport cannot send a write.
            let mut session = connector.open_read()?;
            let result = dry_run_restore(&mut session, target, options, |p| {
                step(ui, &read_progress_text(&p))
            });
            ui.clear_progress();
            report_session(ui, globals, &session);
            result?
        }
    };
    if args.json {
        print_json(
            ui,
            &RestoreJson {
                backup: args.file.display().to_string(),
                dry_run: true,
                against: args.against.map(|p| p.display().to_string()),
                safety_backup: None,
                plan: &plan,
                packets: args.packets.then(|| report::plan_packets(&plan)),
                confirmed: None,
                report: None,
                note: JSON_NOTE,
            },
        )?;
    } else {
        ui.print(&report::restore_plan(
            &plan,
            target.model(),
            true,
            args.packets,
        ))?;
        if let Some(import) = &options.import
            && plan.original_profile != Some(import.into.get())
        {
            ui.print(&format!(
                "Profile {into} is not active: a dry run cannot read it without switching \
                     profiles. Select it first (`openwraith profile select {into}`), or run \
                     without --dry-run (the plan is shown and you are asked before anything is \
                     written).\n",
                into = import.into
            ))?;
        }
    }
    Ok(Exit::Success)
}

fn restore_on<T: WriteTransport>(
    session: &mut Session<T>,
    ui: &mut impl Ui,
    args: &RestoreRun<'_>,
    target: &Backup,
    options: &RestoreOptions,
    dir: &Path,
) -> anyhow::Result<Exit> {
    let model: Option<&'static Model> = target.model();
    let scope: Vec<String> = match &options.profiles {
        Some(set) => set.iter().map(|p| p.to_string()).collect(),
        None => target
            .profile_numbers()
            .iter()
            .map(ToString::to_string)
            .collect(),
    };
    ui.note(&format!(
        "Reading the keyboard for the safety backup (the active profile and profile(s) {}). \
         Reading a profile that is not active switches to it (the lighting changes briefly) and \
         back; nothing else is written before you answer.",
        scope.join(", ")
    ));
    let consent_for_switches = WriteConsent::new(ConsentOrigin::CommandLineFlag);
    let prepared = match prepare_restore(session, target, options, consent_for_switches, |p| {
        write_step(ui, &p)
    }) {
        Ok(prepared) => prepared,
        Err(e) => {
            ui.clear_progress();
            if let RestoreError::Profiles(p) = &e {
                note_active_profile(ui, p.active_profile_now);
            }
            ui.note("Nothing was written (profile switches aside).");
            return Err(e.into());
        }
    };
    ui.clear_progress();
    ensure_quiet(session)?;
    let saved = prepared.safety().save_safety(dir, args.reason)?;
    ui.note(&format!("Safety backup saved: {}", saved.path().display()));
    let plan = prepared.plan();
    let plan_text = report::restore_plan(plan, model, false, args.packets);
    if !args.json {
        ui.print(&plan_text)?;
    } else if plan.has_writes() && !args.yes {
        // The JSON document (with the plan) is printed only after the
        // answer: show the plan on stderr first, so that the question is
        // never answered blind. Stdout stays pure JSON.
        ui.note(&plan_text);
    }
    // An import that leaves out a category the user chose fails (exit 1),
    // also when everything else was written.
    let import_blocked = options
        .import
        .as_ref()
        .is_some_and(|import| report::import_blocked(plan, &import.include));
    let blocked_note = "A category chosen with --include was not imported (see \"Not written\").\n";
    if !plan.has_writes() {
        if args.json {
            print_json(ui, &restore_json(args, &saved, plan, None, None))?;
        } else {
            ui.print(if plan.skips().next().is_none() {
                "Nothing to write: the keyboard already holds the backup's values. Nothing was \
                 written.\n"
            } else {
                "Nothing was written: everything that can be restored already matches; the \
                 differences listed under \"Not written\" are not restored.\n"
            })?;
            if import_blocked {
                ui.print(blocked_note)?;
            }
        }
        return Ok(if import_blocked {
            Exit::Failure
        } else {
            Exit::Success
        });
    }
    let per_profile: Vec<String> = report::packets_per_profile(plan)
        .iter()
        .map(|(p, n)| format!("profile {p}: {n} packets"))
        .collect();
    let question = format!(
        "Write {} table(s) to the keyboard ({})?",
        plan.write_count(),
        per_profile.join(", ")
    );
    let Some(consent) = confirm(ui, args.yes, &question)? else {
        if args.json {
            print_json(ui, &restore_json(args, &saved, plan, Some(false), None))?;
        } else {
            ui.print(NOT_CONFIRMED)?;
        }
        return Ok(Exit::Failure);
    };
    let report = execute_restore(session, &prepared, &saved, consent, |p| write_step(ui, &p));
    ui.clear_progress();
    if args.json {
        print_json(
            ui,
            &restore_json(args, &saved, plan, Some(true), Some(&report)),
        )?;
    } else {
        ui.print(&report::restore_report(&report, model))?;
        if import_blocked {
            ui.print(blocked_note)?;
        }
    }
    Ok(if report.is_success() && !import_blocked {
        Exit::Success
    } else {
        Exit::Failure
    })
}

/// The JSON output of a real (not dry) restore.
fn restore_json<'a>(
    args: &RestoreRun<'_>,
    saved: &'a SavedBackup,
    plan: &'a RestorePlan,
    confirmed: Option<bool>,
    report: Option<&'a RestoreReport>,
) -> RestoreJson<'a> {
    RestoreJson {
        backup: args.file.display().to_string(),
        dry_run: false,
        against: None,
        safety_backup: Some(saved),
        plan,
        packets: args.packets.then(|| report::plan_packets(plan)),
        confirmed,
        report,
        note: JSON_NOTE,
    }
}

/// Pretty JSON on stdout.
pub(crate) fn print_json(ui: &mut impl Ui, value: &impl Serialize) -> anyhow::Result<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    ui.print(&text)
}
