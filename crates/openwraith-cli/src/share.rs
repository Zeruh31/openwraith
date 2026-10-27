//! `openwraith profile export` and `openwraith profile import`: one onboard
//! profile as a file to share (no USB serial number in it), and that file
//! imported into a chosen profile of the keyboard.
//!
//! An import is a restore with [`ImportOptions`]: the file's profile is
//! compared with the chosen profile of the keyboard and only the chosen
//! categories are written (triggers and lighting by default), through the
//! same plan, safety backup, question, persistence check and undo command
//! as `openwraith restore`. The file is untrusted input: triggers, the
//! light, the dead band and SOCD go through the validated builders; the
//! keymap layers and the key colours are merged into the keyboard's tables,
//! so the knob, the Fn key, positions outside the layout, Fn-layer keys the
//! keyboard locks and colour slots outside the layout keep the keyboard's
//! bytes; any other key assignment must be one `openwraith set key` could
//! write (else the keys are not imported); a macro must be steps
//! OpenWraith can show; and an Fn layer that would replace a critical
//! built-in function (Fn + 1..4, Fn + Space) is refused unless
//! `--allow-critical-fn-overrides`. A plain `openwraith restore` of an
//! exported file is refused.

use openwraith_hid::{
    Backup, BackupError, ImportOptions, ProfileSet, ProfilesOptions, RestoreError, RestoreOptions,
    Session, Transport, WriteOptions, WriteTransport, read_all_cancellable, read_profiles,
    read_summary,
};
use openwraith_protocol::{MacroData, MacroProgram, MacroSlotId, ProfileId, ReadRequest};

use crate::args::{ImportPart, ProfileExportArgs, ProfileImportArgs};
use crate::commands::{
    CLOSE_SITE_NOTE, Exit, Globals, RestoreRun, UsageError, confirm, note_active_profile,
    profiles_step, read_progress_text, report_session, require_prompt_or_yes, run_restore, step,
    tool_options,
};
use crate::device::Connector;
use crate::ui::Ui;

/// `openwraith profile export N -o FILE [--from BACKUP] [--label TEXT] [--force]`.
pub fn export<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &ProfileExportArgs,
) -> anyhow::Result<Exit> {
    let profile = ProfileId::new(args.profile)
        .ok_or_else(|| UsageError(format!("profile {} does not exist (1-4)", args.profile)))?;
    // Before the keyboard is read.
    if !args.force && args.output.exists() {
        return Err(BackupError::AlreadyExists {
            path: args.output.clone(),
        }
        .into());
    }
    let backup = match &args.from {
        Some(path) => Backup::load(path)?,
        None => match read_profile(connector, ui, globals, profile, args.yes)? {
            Some(backup) => backup,
            None => {
                ui.print("Not confirmed: nothing was read or written.\n")?;
                return Ok(Exit::Failure);
            }
        },
    };
    let exported = backup.export_profile(profile, args.label.as_deref())?;
    exported.save(&args.output, args.force)?;
    let device = &exported.device;
    ui.print(&format!(
        "Profile {profile} exported to {}\n  Keyboard  {} ({}), firmware {}\n  Contents  {} \
         replies of profile {profile}; no USB serial number\nImport it with `openwraith profile \
         import {} --into N` (triggers and lighting by default; --include keys,deadband,socd \
         for more).\n",
        args.output.display(),
        device.model.as_deref().unwrap_or("unknown model"),
        device.product.as_deref().unwrap_or("?"),
        device.firmware.as_deref().unwrap_or("?"),
        exported.entries.len(),
        args.output.display()
    ))?;
    Ok(Exit::Success)
}

/// Reads `profile` from the keyboard: a pure read through a read-only
/// session when it is active, otherwise a switch to it and back (asked
/// first; without a terminal it needs `--yes`, checked before a writable
/// session is opened). `None` when declined.
fn read_profile<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    profile: ProfileId,
    yes: bool,
) -> anyhow::Result<Option<Backup>> {
    ui.enable_cancel();
    // A read-only session first: this transport cannot send a write.
    let mut session = connector.open_read()?;
    let active = read_active_on(&mut session, ui, profile);
    report_session(ui, globals, &session);
    drop(session);
    if let Some(backup) = active? {
        return Ok(Some(backup));
    }
    // Reading it switches profiles: asked, so it needs a terminal or --yes.
    require_prompt_or_yes(ui, yes)?;
    ui.note(CLOSE_SITE_NOTE);
    let mut session = connector.open_write()?;
    let result = read_profile_on(&mut session, ui, profile, yes);
    report_session(ui, globals, &session);
    result
}

/// Reads `profile` through a read-only session when it is the active one;
/// `None` when another profile is active.
fn read_active_on<T: Transport>(
    session: &mut Session<T>,
    ui: &mut impl Ui,
    profile: ProfileId,
) -> anyhow::Result<Option<Backup>> {
    let summary = read_summary(session)?;
    if summary.active_profile != profile.get() {
        return Ok(None);
    }
    ui.note(&format!(
        "Reading profile {profile} (the active one; read-only)..."
    ));
    let backup = read_all_cancellable(session, &tool_options(), |p| {
        step(ui, &read_progress_text(&p))
    });
    ui.clear_progress();
    Ok(Some(backup?))
}

fn read_profile_on<T: WriteTransport>(
    session: &mut Session<T>,
    ui: &mut impl Ui,
    profile: ProfileId,
    yes: bool,
) -> anyhow::Result<Option<Backup>> {
    let summary = read_summary(session)?;
    if summary.active_profile == profile.get() {
        ui.note(&format!(
            "Reading profile {profile} (the active one; read-only)..."
        ));
        let backup = read_all_cancellable(session, &tool_options(), |p| {
            step(ui, &read_progress_text(&p))
        });
        ui.clear_progress();
        return Ok(Some(backup?));
    }
    let question = format!(
        "Profile {profile} is not active: reading it switches to it (the lighting changes \
         briefly) and back to profile {}; nothing else is written. Read it?",
        summary.active_profile
    );
    let Some(consent) = confirm(ui, yes, &question)? else {
        return Ok(None);
    };
    let options = ProfilesOptions {
        backup: tool_options(),
        write: WriteOptions::default(),
    };
    let result = read_profiles(session, ProfileSet::only(profile), consent, &options, |p| {
        profiles_step(ui, &p)
    });
    ui.clear_progress();
    match result {
        Ok(backup) => Ok(Some(backup)),
        Err(e) => {
            note_active_profile(ui, e.active_profile_now);
            Err(e.into())
        }
    }
}

/// The macros of profile `from` of `file`, as steps (for the note before
/// an import of the keymap group).
fn macros_text(file: &Backup, from: ProfileId) -> Option<String> {
    let snapshot = file.snapshot(from)?;
    let mut lines = Vec::new();
    for slot in MacroSlotId::all() {
        let Some(raw) = snapshot.response(ReadRequest::Macro(slot)) else {
            continue;
        };
        let line = match MacroData::from_response(raw) {
            Ok((_, MacroData::Empty)) => continue,
            Ok((_, MacroData::Content(content))) => match MacroProgram::decode(&content) {
                Some(program) => format!("  slot {}: {program} ({:?})", slot.get(), program.play()),
                None => format!(
                    "  slot {}: a macro OpenWraith cannot show as steps",
                    slot.get()
                ),
            },
            Err(e) => format!("  slot {}: {e}", slot.get()),
        };
        lines.push(line);
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// `openwraith profile import FILE --into N [--profile M] [--include ...]`.
pub fn import<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &ProfileImportArgs,
) -> anyhow::Result<Exit> {
    let file = Backup::load(&args.file)?;
    let from = match args.from {
        Some(n) => ProfileId::new(n),
        None => file.active_profile.and_then(ProfileId::new),
    }
    .ok_or_else(|| {
        UsageError("the file does not say which profile it holds; give --profile N".to_string())
    })?;
    let into = ProfileId::new(args.into)
        .ok_or_else(|| UsageError(format!("profile {} does not exist (1-4)", args.into)))?;
    if !file.profile_numbers().contains(&from.get()) {
        return Err(RestoreError::ProfileNotInBackup {
            profile: from.get(),
        }
        .into());
    }
    if args.include.is_empty() {
        return Err(UsageError("--include needs at least one category".to_string()).into());
    }
    let parts: Vec<&str> = args
        .include
        .iter()
        .map(|part| match part {
            ImportPart::Triggers => "triggers",
            ImportPart::Lighting => "lighting",
            ImportPart::Keys => "keys (key assignments, macros, advanced keys)",
            ImportPart::Deadband => "dead band",
            ImportPart::Socd => "SOCD",
        })
        .collect();
    ui.note(&format!(
        "Importing profile {from} of {} into profile {into} of the keyboard: {}. The file is \
         treated as untrusted input: the knob, the Fn key and keys the keyboard locks keep \
         their entries, a key assignment OpenWraith does not write stops the keys from being \
         imported, and every other value is checked before anything is written.",
        args.file.display(),
        parts.join(", ")
    ));
    if args.include.contains(&ImportPart::Keys) {
        if let Some(text) = macros_text(&file, from) {
            ui.note(&format!(
                "Macros in the file (written to the same slots where they differ):\n{text}"
            ));
        }
    }
    let options = RestoreOptions {
        profiles: Some(ProfileSet::only(into)),
        allow_firmware_mismatch: args.allow_firmware_mismatch,
        backup: tool_options(),
        write: WriteOptions::default(),
        import: Some(ImportOptions {
            from,
            into,
            include: args.selection(),
            allow_critical_fn_overrides: args.allow_critical_fn_overrides,
        }),
    };
    let run = RestoreRun {
        file: &args.file,
        dry_run: args.flags.dry_run,
        against: None,
        yes: args.flags.yes,
        json: args.flags.json,
        packets: args.flags.packets,
        reason: "import",
    };
    run_restore(connector, ui, globals, &file, &options, &run)
}
