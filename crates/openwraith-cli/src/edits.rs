//! `openwraith set ...`: the editors of the active profile (triggers and
//! presets, the main light, key assignments, macros, advanced keys, per-key
//! colours, the dead band, SOCD).
//!
//! Every editor runs the same steps ([`run`]): arguments checked before the
//! keyboard is opened (usage errors, exit code 2); the current values read
//! and the change planned (read-only); with `--dry-run` the plan is shown
//! and nothing else happens (read-only connection, no safety backup);
//! otherwise a verified safety backup, the plan ("now -> new", the writes
//! in order, what is not verified on hardware, whether the persistence
//! check will run), `[y/N]` or `--yes`, the guarded write of
//! `openwraith-hid`, and the report with the undo command. Exit code 1 when
//! anything failed, was lost or may be lost.

use std::ops::ControlFlow;
use std::path::Path;

use openwraith_hid::{
    ColorEdit, ColorEditPlan, CommitTriggerMode, DeadBandEdit, DeadBandEditPlan, EditOptions,
    EditReport, KeymapChange, KeymapEdit, KeymapEditPlan, KeymapGroupState, LightEdit,
    LightEditPlan, PersistenceCheckMode, SavedBackup, Session, SocdEdit, SocdEditPlan, SocdState,
    Transport, TriggerEdit, TriggerEditPlan, WriteConsent, WriteOptions, WriteProgress,
    WriteTransport, apply_color_edit, apply_dead_band_edit, apply_keymap_edit, apply_light_edit,
    apply_socd_edit, apply_trigger_edit, plan_color_edit, plan_dead_band_edit, plan_keymap_edit_on,
    plan_light_edit, plan_socd_edit, plan_trigger_edit, preset_edit, read_summary, trigger_preset,
};
use openwraith_protocol::write::{Dks, DksRow, ModTap, Toggle};
use openwraith_protocol::{
    AdvancedKeyDef, AdvancedSlotId, BuiltinAck, Distance, EntryKind, HidUsage, KeyAssignment,
    KeyPos, Layer, MacroProgram, MacroSlotId, Model, Rgb, SocdMode, SocdPair, WriteRequest,
};
use serde::Serialize;

use crate::args::{
    AdvancedKind, CheckPersistence, CommitTriggerArg, EditFlags, KeySpec, KeyUse, SetAdvancedArgs,
    SetColorsArgs, SetDeadBandArgs, SetKeyArgs, SetLightArgs, SetMacroArgs, SetSocdArgs,
    SetTriggerArgs, WriteFlags, parse_keymap_key,
};
use crate::commands::{
    CLOSE_SITE_NOTE, Exit, Globals, NOT_CONFIRMED, UsageError, confirm, ensure_quiet, print_json,
    report_session, require_prompt_or_yes, safety_backup, safety_dir, write_step,
};
use crate::device::Connector;
use crate::edit_text;
use crate::parse::{DksRowSpec, KeyAssign, KeyColor, SocdPairSpec};
use crate::report::{self, PersistNote};
use crate::ui::Ui;
use crate::view::JSON_NOTE;

/// A planned edit, ready to be shown and written.
pub(crate) struct Planned<P> {
    /// The library's plan.
    plan: P,
    /// The keyboard's model, for key names in the report.
    model: Option<&'static Model>,
    /// The plan in words.
    text: String,
    /// Every write in order, with its name (for `--packets`).
    writes: Vec<(String, WriteRequest)>,
    /// Printed after the plan when nothing would be written.
    nothing: String,
    /// The confirmation question.
    question: String,
}

impl<P> Planned<P> {
    fn has_changes(&self) -> bool {
        !self.writes.is_empty()
    }

    /// The plan, with the packets when asked for.
    fn shown(&self, packets: bool) -> String {
        let mut text = self.text.clone();
        if packets && self.has_changes() {
            text.push_str(&edit_text::packets_text(&self.writes));
        }
        text
    }

    /// The packets for JSON output.
    fn packet_lists(&self) -> Vec<PacketList> {
        self.writes
            .iter()
            .map(|(name, request)| PacketList {
                write: name.clone(),
                packets: request.steps().iter().map(|s| s.packet.summary()).collect(),
            })
            .collect()
    }
}

/// One editor: how it plans and writes.
pub(crate) trait EditCommand {
    /// The library's plan type.
    type Plan: Serialize;

    /// The command as typed, for JSON output (`set key`).
    fn name(&self) -> &'static str;

    /// The suffix of the safety backup's file name (`set-key`).
    fn reason(&self) -> &'static str;

    /// Reads the keyboard (read-only) and plans the edit.
    fn plan<T: Transport>(&self, session: &mut Session<T>) -> anyhow::Result<Planned<Self::Plan>>;

    /// Writes the plan.
    fn apply<T: WriteTransport>(
        &self,
        session: &mut Session<T>,
        plan: &Self::Plan,
        saved: &SavedBackup,
        consent: WriteConsent,
        progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
    ) -> EditReport;

    /// How the report speaks about the persistence check.
    fn persistence(&self) -> PersistNote;
}

/// The packets of one write (JSON output).
#[derive(Debug, Serialize)]
struct PacketList {
    /// The write.
    write: String,
    /// Spaced hex of each packet (trailing zero bytes omitted).
    packets: Vec<String>,
}

/// JSON output of `set ... --json`.
#[derive(Debug, Serialize)]
struct EditJson<'a, P: Serialize> {
    /// The command.
    command: &'static str,
    /// Whether this was a dry run (nothing written).
    dry_run: bool,
    /// The plan.
    plan: &'a P,
    /// The packets of every planned write (`--packets`).
    #[serde(skip_serializing_if = "Option::is_none")]
    packets: Option<Vec<PacketList>>,
    /// The safety backup made before writing.
    #[serde(skip_serializing_if = "Option::is_none")]
    safety_backup: Option<&'a SavedBackup>,
    /// Whether the user confirmed (absent for a dry run or when there was
    /// nothing to write).
    #[serde(skip_serializing_if = "Option::is_none")]
    confirmed: Option<bool>,
    /// What was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<&'a EditReport>,
    /// Confidence note of every JSON output.
    note: &'static str,
}

fn json<'a, E: EditCommand>(
    edit: &E,
    planned: &'a Planned<E::Plan>,
    flags: &WriteFlags,
    saved: Option<&'a SavedBackup>,
    confirmed: Option<bool>,
    report: Option<&'a EditReport>,
) -> EditJson<'a, E::Plan> {
    EditJson {
        command: edit.name(),
        dry_run: flags.dry_run,
        plan: &planned.plan,
        packets: flags.packets.then(|| planned.packet_lists()),
        safety_backup: saved,
        confirmed,
        report,
        note: JSON_NOTE,
    }
}

/// Runs one editor: a dry run on a read-only connection, or the guarded
/// edit.
pub(crate) fn run<C: Connector, U: Ui, E: EditCommand>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    edit: &E,
    flags: &WriteFlags,
) -> anyhow::Result<Exit> {
    if flags.dry_run {
        ui.note(CLOSE_SITE_NOTE);
        ui.note("Reading the keyboard (read-only; nothing is written)...");
        // A read-only session: this transport cannot send a write.
        let mut session = connector.open_read()?;
        let result = edit.plan(&mut session);
        report_session(ui, globals, &session);
        let planned = result?;
        if flags.json {
            print_json(ui, &json(edit, &planned, flags, None, None, None))?;
        } else {
            ui.print(&planned.shown(flags.packets))?;
            ui.print(if planned.has_changes() {
                "Dry run: nothing was written.\n"
            } else {
                &planned.nothing
            })?;
        }
        return Ok(Exit::Success);
    }
    require_prompt_or_yes(ui, flags.yes)?;
    let dir = safety_dir(globals)?;
    ui.note(CLOSE_SITE_NOTE);
    ui.enable_cancel();
    let mut session = connector.open_write()?;
    let result = run_on(&mut session, ui, edit, flags, &dir);
    report_session(ui, globals, &session);
    result
}

fn run_on<T: WriteTransport, E: EditCommand>(
    session: &mut Session<T>,
    ui: &mut impl Ui,
    edit: &E,
    flags: &WriteFlags,
    dir: &Path,
) -> anyhow::Result<Exit> {
    let planned = edit.plan(session)?;
    ensure_quiet(session)?;
    if !planned.has_changes() {
        if flags.json {
            print_json(ui, &json(edit, &planned, flags, None, None, None))?;
        } else {
            ui.print(&planned.text)?;
            ui.print(&planned.nothing)?;
        }
        return Ok(Exit::Success);
    }
    let saved = safety_backup(session, ui, dir, edit.reason())?;
    let shown = planned.shown(flags.packets);
    if !flags.json {
        ui.print(&shown)?;
    } else if !flags.yes {
        // The JSON document comes after the answer: show the plan on stderr
        // first, so that the question is never answered blind.
        ui.note(&shown);
    }
    let Some(consent) = confirm(ui, flags.yes, &planned.question)? else {
        if flags.json {
            print_json(
                ui,
                &json(edit, &planned, flags, Some(&saved), Some(false), None),
            )?;
        } else {
            ui.print(NOT_CONFIRMED)?;
        }
        return Ok(Exit::Failure);
    };
    let report = edit.apply(session, &planned.plan, &saved, consent, |p| {
        write_step(ui, &p)
    });
    ui.clear_progress();
    if flags.json {
        print_json(
            ui,
            &json(
                edit,
                &planned,
                flags,
                Some(&saved),
                Some(true),
                Some(&report),
            ),
        )?;
    } else {
        ui.print(&report::edit_report(
            &report,
            planned.model,
            edit.persistence(),
        ))?;
    }
    Ok(if report.is_success() {
        Exit::Success
    } else {
        Exit::Failure
    })
}

/// The model of the keyboard, checked for the 0.02 mm unit (trigger and
/// dead-band edits).
fn model_with_unit<T: Transport>(
    session: &mut Session<T>,
    check_unit: bool,
) -> anyhow::Result<&'static Model> {
    let summary = read_summary(session)?;
    let Some(model) = summary.model() else {
        anyhow::bail!("unsupported keyboard; nothing was written");
    };
    if check_unit {
        let unit = summary
            .trigger_global
            .unit_micrometres(summary.status.flags_b.high_precision);
        if unit != Distance::MICROMETRES_PER_UNIT {
            anyhow::bail!(
                "the keyboard reports a distance unit of {unit} um; OpenWraith only handles \
                 0.02 mm units. Nothing was written."
            );
        }
    }
    Ok(model)
}

/// Resolves keys for `purpose` (usage error otherwise), or every key but
/// the knob with `all`.
fn resolve_keys(
    model: &Model,
    keys: &[KeySpec],
    all: bool,
    purpose: KeyUse,
) -> Result<Vec<KeyPos>, UsageError> {
    if all {
        return Ok(model
            .positions()
            .filter(|p| Some(p.index()) != model.knob_index)
            .collect());
    }
    keys.iter()
        .map(|k| k.resolve_as(model, purpose).map_err(UsageError))
        .collect()
}

/// The library's options of an edit with `mode`.
fn edit_options(mode: PersistenceCheckMode) -> EditOptions {
    EditOptions {
        write: WriteOptions::default(),
        persistence: mode,
    }
}

/// The writes of a v0.2 plan with their names.
fn named(writes: &[openwraith_hid::PlannedEditWrite]) -> Vec<(String, WriteRequest)> {
    writes
        .iter()
        .map(|w| (w.summary.clone(), w.request().clone()))
        .collect()
}

/// `"1 key"`, `"3 keys"`.
fn keys_count(n: usize) -> String {
    if n == 1 {
        "1 key".to_string()
    } else {
        format!("{n} keys")
    }
}

// ----- set trigger -----

/// `set trigger`.
struct TriggerCmd {
    keys: Vec<KeySpec>,
    all: bool,
    actuation: Option<Distance>,
    rapid_trigger: Option<openwraith_hid::RtEdit>,
    preset: Option<&'static openwraith_hid::TriggerPreset>,
}

impl EditCommand for TriggerCmd {
    type Plan = TriggerEditPlan;

    fn name(&self) -> &'static str {
        "set trigger"
    }

    fn reason(&self) -> &'static str {
        "set-trigger"
    }

    fn plan<T: Transport>(&self, session: &mut Session<T>) -> anyhow::Result<Planned<Self::Plan>> {
        let model = model_with_unit(session, true)?;
        let keys = resolve_keys(model, &self.keys, self.all, KeyUse::Trigger)?;
        let edit = match self.preset {
            Some(preset) => preset_edit(preset, keys),
            None => TriggerEdit {
                keys,
                actuation: self.actuation,
                rapid_trigger: self.rapid_trigger,
            },
        };
        let plan = plan_trigger_edit(session, &edit)?;
        let writes: Vec<(String, WriteRequest)> = plan
            .keys()
            .iter()
            .filter_map(|k| {
                k.request()
                    .map(|r| (format!("trigger of {} ({})", k.name, k.pos), r.clone()))
            })
            .collect();
        let mut text = report::trigger_edit_plan(&plan);
        if let Some(preset) = self.preset {
            text.insert_str(0, &format!("Preset \"{}\".\n", preset.id));
        }
        Ok(Planned {
            question: format!(
                "Write the new trigger settings of {} key(s) to profile {}?",
                writes.len(),
                plan.profile()
            ),
            text,
            writes,
            nothing: "Nothing to change: every key already has these settings. Nothing was \
                      written.\n"
                .to_string(),
            model: Some(model),
            plan,
        })
    }

    fn apply<T: WriteTransport>(
        &self,
        session: &mut Session<T>,
        plan: &Self::Plan,
        saved: &SavedBackup,
        consent: WriteConsent,
        progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
    ) -> EditReport {
        apply_trigger_edit(
            session,
            plan,
            saved,
            consent,
            &WriteOptions::default(),
            progress,
        )
    }

    fn persistence(&self) -> PersistNote {
        PersistNote::NotNeeded
    }
}

/// `openwraith set trigger KEY...|--all [--actuation MM] [--rapid-trigger P[/R]] [--preset NAME] ...`.
pub fn set_trigger<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &SetTriggerArgs,
) -> anyhow::Result<Exit> {
    let preset = match &args.preset {
        Some(id) => {
            Some(trigger_preset(id).ok_or_else(|| UsageError(format!("no preset named {id:?}")))?)
        }
        None => None,
    };
    let cmd = TriggerCmd {
        keys: args.keys.clone(),
        all: args.all,
        actuation: args.actuation,
        rapid_trigger: args.rapid_trigger_edit(),
        preset,
    };
    run(connector, ui, globals, &cmd, &args.flags)
}

// ----- set light -----

/// `set light`.
struct LightCmd(LightEdit);

impl EditCommand for LightCmd {
    type Plan = LightEditPlan;

    fn name(&self) -> &'static str {
        "set light"
    }

    fn reason(&self) -> &'static str {
        "set-light"
    }

    fn plan<T: Transport>(&self, session: &mut Session<T>) -> anyhow::Result<Planned<Self::Plan>> {
        let model = read_summary(session)?.model();
        let plan = plan_light_edit(session, &self.0)?;
        let writes = plan
            .request()
            .map(|r| vec![("main light".to_string(), r.clone())])
            .unwrap_or_default();
        Ok(Planned {
            question: format!("Write the new main light to profile {}?", plan.profile()),
            text: report::light_edit_plan(&plan),
            writes,
            nothing: "Nothing to change: the main light already has these settings. Nothing was \
                      written.\n"
                .to_string(),
            model,
            plan,
        })
    }

    fn apply<T: WriteTransport>(
        &self,
        session: &mut Session<T>,
        plan: &Self::Plan,
        saved: &SavedBackup,
        consent: WriteConsent,
        progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
    ) -> EditReport {
        apply_light_edit(
            session,
            plan,
            saved,
            consent,
            &WriteOptions::default(),
            progress,
        )
    }

    fn persistence(&self) -> PersistNote {
        PersistNote::NotNeeded
    }
}

/// `openwraith set light [--mode NAME] [--color RRGGBB] [--brightness N] [--speed N]`.
pub fn set_light<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &SetLightArgs,
) -> anyhow::Result<Exit> {
    let cmd = LightCmd(LightEdit {
        mode: args.mode,
        foreground: args.color,
        brightness: args.brightness,
        speed: args.speed,
    });
    run(connector, ui, globals, &cmd, &args.flags)
}

// ----- the keymap group: set key / set macro / set advanced -----

/// An advanced key as given on the command line; the bound key's own usage
/// fills the defaults of Mod-Tap and Toggle.
#[derive(Debug, Clone)]
enum AdvancedSpec {
    Dks {
        point: u8,
        rows: Vec<DksRow>,
    },
    ModTap {
        tap: Option<HidUsage>,
        hold: HidUsage,
        threshold_10ms: u8,
    },
    Toggle {
        key: Option<HidUsage>,
    },
}

impl AdvancedSpec {
    /// The definition, with `own` (the bound key's usage) as the default
    /// tap or toggle key.
    fn def(&self, own: Option<HidUsage>) -> Result<AdvancedKeyDef, UsageError> {
        let need_key = |what: &str, flag: &str| {
            UsageError(format!(
                "give the {what} with --{flag}: without a bound key there is no default"
            ))
        };
        Ok(match self {
            AdvancedSpec::Dks { point, rows } => {
                let mut slots: [Option<DksRow>; 4] = [None; 4];
                for (slot, row) in slots.iter_mut().zip(rows) {
                    *slot = Some(*row);
                }
                AdvancedKeyDef::Dks(Dks {
                    first_point: *point,
                    rows: slots,
                })
            }
            AdvancedSpec::ModTap {
                tap,
                hold,
                threshold_10ms,
            } => AdvancedKeyDef::ModTap(ModTap {
                hold: *hold,
                tap: tap.or(own).ok_or_else(|| need_key("tap key", "tap"))?,
                threshold_10ms: *threshold_10ms,
            }),
            AdvancedSpec::Toggle { key } => AdvancedKeyDef::Toggle(Toggle {
                key: key.or(own).ok_or_else(|| need_key("toggled key", "key"))?,
            }),
        })
    }
}

/// What a keymap-group command changes.
#[derive(Debug, Clone)]
enum KeymapIntent {
    Assign(Vec<KeyAssign>),
    BindMacro {
        key: KeySpec,
        program: MacroProgram,
        slot: Option<MacroSlotId>,
    },
    UnbindMacro {
        key: KeySpec,
    },
    SetMacroSlot {
        slot: MacroSlotId,
        program: Option<MacroProgram>,
    },
    BindAdvanced {
        key: KeySpec,
        spec: AdvancedSpec,
        slot: Option<AdvancedSlotId>,
    },
    UnbindAdvanced {
        key: KeySpec,
    },
    SetAdvancedSlot {
        slot: AdvancedSlotId,
        spec: Option<AdvancedSpec>,
    },
}

/// `set key`, `set macro`, `set advanced`.
struct KeymapCmd {
    name: &'static str,
    reason: &'static str,
    intent: KeymapIntent,
    layer: Layer,
    /// `--allow-builtin` (false) and `--allow-critical-builtin` (true).
    acks: Vec<(KeySpec, bool)>,
    keep_slot: bool,
    commit_trigger: CommitTriggerMode,
    mode: PersistenceCheckMode,
}

impl KeymapCmd {
    /// The library's edit, resolved against the keyboard's group.
    fn edit(&self, state: &KeymapGroupState) -> Result<KeymapEdit, UsageError> {
        let model = state.model();
        let layer = self.layer;
        let resolve = |key: &KeySpec| key.resolve_as(model, KeyUse::Keymap).map_err(UsageError);
        let own_usage = |pos: KeyPos| {
            model
                .key_at(pos)
                .and_then(|k| k.hid_usage)
                .and_then(HidUsage::new)
        };
        let bound = |key: &KeySpec, pos: KeyPos, macro_slot: bool| -> Result<(), UsageError> {
            let entry = state.entry(layer, pos);
            let ok = matches!(
                (&entry, macro_slot),
                (EntryKind::Assignable(KeyAssignment::Macro(_)), true)
                    | (EntryKind::Assignable(KeyAssignment::Advanced(_)), false)
            );
            if ok {
                return Ok(());
            }
            Err(UsageError(format!(
                "{key} on the {} layer is not bound to {} (it is {}); nothing was written",
                edit_text::layer_text(layer),
                if macro_slot {
                    "a macro"
                } else {
                    "an advanced key"
                },
                edit_text::entry_text(&entry, layer, pos.index(), model)
            )))
        };
        let changes = match &self.intent {
            KeymapIntent::Assign(assigns) => assigns
                .iter()
                .map(|a| {
                    Ok(KeymapChange::Assign {
                        layer,
                        pos: resolve(&a.key)?,
                        assignment: a.action,
                    })
                })
                .collect::<Result<Vec<_>, UsageError>>()?,
            KeymapIntent::BindMacro { key, program, slot } => vec![KeymapChange::BindMacro {
                layer,
                pos: resolve(key)?,
                program: program.clone(),
                slot: *slot,
            }],
            KeymapIntent::UnbindMacro { key } => {
                let pos = resolve(key)?;
                bound(key, pos, true)?;
                vec![KeymapChange::Unbind { layer, pos }]
            }
            KeymapIntent::SetMacroSlot { slot, program } => vec![KeymapChange::SetMacroSlot {
                slot: *slot,
                program: program.clone(),
            }],
            KeymapIntent::BindAdvanced { key, spec, slot } => {
                let pos = resolve(key)?;
                vec![KeymapChange::BindAdvanced {
                    layer,
                    pos,
                    def: spec.def(own_usage(pos))?,
                    slot: *slot,
                }]
            }
            KeymapIntent::UnbindAdvanced { key } => {
                let pos = resolve(key)?;
                bound(key, pos, false)?;
                vec![KeymapChange::Unbind { layer, pos }]
            }
            KeymapIntent::SetAdvancedSlot { slot, spec } => vec![KeymapChange::SetAdvancedSlot {
                slot: *slot,
                def: spec.as_ref().map(|s| s.def(None)).transpose()?,
            }],
        };
        let builtin_acks = self
            .acks
            .iter()
            .map(|(key, critical)| {
                Ok(BuiltinAck {
                    pos: resolve(key)?,
                    critical: *critical,
                })
            })
            .collect::<Result<Vec<_>, UsageError>>()?;
        Ok(KeymapEdit {
            changes,
            builtin_acks,
            keep_orphans: self.keep_slot,
            commit_trigger: self.commit_trigger,
            allow_not_undoable: false,
        })
    }
}

impl EditCommand for KeymapCmd {
    type Plan = KeymapEditPlan;

    fn name(&self) -> &'static str {
        self.name
    }

    fn reason(&self) -> &'static str {
        self.reason
    }

    fn plan<T: Transport>(&self, session: &mut Session<T>) -> anyhow::Result<Planned<Self::Plan>> {
        let state = KeymapGroupState::read(session)?;
        let model = state.model();
        let edit = self.edit(&state)?;
        let plan = plan_keymap_edit_on(&state, &edit)?;
        let writes = named(plan.writes());
        let packets: usize = plan.writes().iter().map(|w| w.packets).sum();
        Ok(Planned {
            question: format!(
                "Write the keymap group of profile {} ({} write(s), {packets} packets)?",
                plan.profile(),
                writes.len()
            ),
            text: edit_text::keymap_plan(&plan, model, self.mode),
            writes,
            nothing: "Nothing to change: the keyboard already holds this. Nothing was written.\n"
                .to_string(),
            model: Some(model),
            plan,
        })
    }

    fn apply<T: WriteTransport>(
        &self,
        session: &mut Session<T>,
        plan: &Self::Plan,
        saved: &SavedBackup,
        consent: WriteConsent,
        progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
    ) -> EditReport {
        apply_keymap_edit(
            session,
            plan,
            saved,
            consent,
            &edit_options(self.mode),
            progress,
        )
    }

    fn persistence(&self) -> PersistNote {
        if self.commit_trigger == CommitTriggerMode::OmitForSlotsOnly {
            PersistNote::CommitTriggerSkipped
        } else {
            PersistNote::Mode(self.mode)
        }
    }
}

/// `--commit-trigger` and `--check-persistence` together.
fn commit_and_mode(
    commit: CommitTriggerArg,
    flags: &EditFlags,
) -> Result<(CommitTriggerMode, PersistenceCheckMode), UsageError> {
    match commit {
        CommitTriggerArg::Auto => Ok((CommitTriggerMode::Auto, flags.check_persistence.mode())),
        CommitTriggerArg::Skip if flags.check_persistence == CheckPersistence::Never => {
            Err(UsageError(
                "--commit-trigger skip always runs the persistence check; leave out \
                 --check-persistence never"
                    .to_string(),
            ))
        }
        CommitTriggerArg::Skip => Ok((
            CommitTriggerMode::OmitForSlotsOnly,
            PersistenceCheckMode::Always,
        )),
    }
}

/// The `--allow-builtin` / `--allow-critical-builtin` keys.
fn acks(allow: &[KeySpec], allow_critical: &[KeySpec]) -> Vec<(KeySpec, bool)> {
    allow
        .iter()
        .map(|k| (k.clone(), false))
        .chain(allow_critical.iter().map(|k| (k.clone(), true)))
        .collect()
}

/// `openwraith set key KEY=ACTION... [--layer base|fn]`.
pub fn set_key<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &SetKeyArgs,
) -> anyhow::Result<Exit> {
    let cmd = KeymapCmd {
        name: "set key",
        reason: "set-key",
        intent: KeymapIntent::Assign(args.assignments.clone()),
        layer: args.layer.layer(),
        acks: acks(&args.allow_builtin, &args.allow_critical_builtin),
        keep_slot: false,
        commit_trigger: CommitTriggerMode::Auto,
        mode: args.flags.check_persistence.mode(),
    };
    run(connector, ui, globals, &cmd, &args.flags.write)
}

/// The usage error of a `set macro` / `set advanced` form that does not
/// exist.
fn forms_error(command: &str, forms: &str) -> UsageError {
    UsageError(format!("{command}: give one of {forms}"))
}

/// `openwraith set macro ...`.
pub fn set_macro<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &SetMacroArgs,
) -> anyhow::Result<Exit> {
    const FORMS: &str = "KEY --steps \"...\", KEY --unbind, --slot N --steps \"...\" or --slot \
                         N --clear";
    if args.keep_slot && !args.unbind {
        return Err(UsageError("--keep-slot only goes with --unbind".to_string()).into());
    }
    let slot = args.slot.and_then(MacroSlotId::new);
    let program = args
        .steps
        .as_ref()
        .map(|steps| MacroProgram::new(args.play, steps.0.clone()))
        .transpose()
        .map_err(|e| UsageError(e.to_string()))?;
    let intent = match (&args.key, program, args.unbind, args.clear, slot) {
        (Some(key), Some(program), false, false, slot) => KeymapIntent::BindMacro {
            key: key.clone(),
            program,
            slot,
        },
        (Some(key), None, true, false, None) => KeymapIntent::UnbindMacro { key: key.clone() },
        (None, Some(program), false, false, Some(slot)) => KeymapIntent::SetMacroSlot {
            slot,
            program: Some(program),
        },
        (None, None, false, true, Some(slot)) => KeymapIntent::SetMacroSlot {
            slot,
            program: None,
        },
        _ => return Err(forms_error("set macro", FORMS).into()),
    };
    let (commit_trigger, mode) = commit_and_mode(args.commit_trigger, &args.flags)?;
    let cmd = KeymapCmd {
        name: "set macro",
        reason: "set-macro",
        intent,
        layer: args.layer.layer(),
        acks: acks(&args.allow_builtin, &args.allow_critical_builtin),
        keep_slot: args.keep_slot,
        commit_trigger,
        mode,
    };
    run(connector, ui, globals, &cmd, &args.flags.write)
}

/// The key and the kind of `set advanced` (both optional, either order of
/// presence: `E dks`, `toggle` with --slot, `E` with --unbind).
fn key_and_kind(
    key: Option<&str>,
    kind: Option<&str>,
) -> Result<(Option<KeySpec>, Option<AdvancedKind>), UsageError> {
    let parse_kind = |word: &str| {
        AdvancedKind::from_word(word)
            .ok_or_else(|| UsageError(format!("unknown kind {word:?}: dks, mod-tap or toggle")))
    };
    match (key, kind) {
        (Some(key), Some(kind)) => Ok((
            Some(parse_keymap_key(key).map_err(UsageError)?),
            Some(parse_kind(kind)?),
        )),
        (Some(word), None) => match AdvancedKind::from_word(word) {
            Some(kind) => Ok((None, Some(kind))),
            None => Ok((Some(parse_keymap_key(word).map_err(UsageError)?), None)),
        },
        (None, _) => Ok((None, None)),
    }
}

/// The advanced key of `set advanced KEY KIND ...`, with only the options
/// of that kind.
fn advanced_spec(kind: AdvancedKind, args: &SetAdvancedArgs) -> Result<AdvancedSpec, UsageError> {
    let only = |given: &[(&str, bool)]| -> Result<(), UsageError> {
        match given.iter().find(|(_, present)| *present) {
            Some((name, _)) => Err(UsageError(format!(
                "--{name} does not belong to this kind of advanced key"
            ))),
            None => Ok(()),
        }
    };
    match kind {
        AdvancedKind::Dks => {
            only(&[
                ("tap", args.tap.is_some()),
                ("hold", args.hold.is_some()),
                ("time", args.time.is_some()),
                ("key", args.toggle_key.is_some()),
            ])?;
            if args.rows.is_empty() || args.rows.len() > 4 {
                return Err(UsageError(
                    "a DKS needs 1 to 4 rows: --row KEY:STAGES, e.g. --row E:t... --row R:.t.."
                        .to_string(),
                ));
            }
            Ok(AdvancedSpec::Dks {
                // The vendor site's default: 1.5 mm.
                point: args.point.unwrap_or(15),
                rows: args.rows.iter().map(|DksRowSpec(row)| *row).collect(),
            })
        }
        AdvancedKind::ModTap => {
            only(&[
                ("row", !args.rows.is_empty()),
                ("point", args.point.is_some()),
                ("key", args.toggle_key.is_some()),
            ])?;
            let hold = args.hold.ok_or_else(|| {
                UsageError("a Mod-Tap needs --hold KEY (the key sent when held)".to_string())
            })?;
            Ok(AdvancedSpec::ModTap {
                tap: args.tap,
                hold,
                // The vendor site's default: 300 ms (c16).
                threshold_10ms: args.time.unwrap_or(30),
            })
        }
        AdvancedKind::Toggle => {
            only(&[
                ("row", !args.rows.is_empty()),
                ("point", args.point.is_some()),
                ("tap", args.tap.is_some()),
                ("hold", args.hold.is_some()),
                ("time", args.time.is_some()),
            ])?;
            Ok(AdvancedSpec::Toggle {
                key: args.toggle_key,
            })
        }
    }
}

/// `openwraith set advanced ...`.
pub fn set_advanced<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &SetAdvancedArgs,
) -> anyhow::Result<Exit> {
    const FORMS: &str = "KEY dks|mod-tap|toggle ..., KEY --unbind, --slot N dks|mod-tap|toggle \
                         ... or --slot N --clear";
    if args.keep_slot && !args.unbind {
        return Err(UsageError("--keep-slot only goes with --unbind".to_string()).into());
    }
    let (key, kind) = key_and_kind(args.key.as_deref(), args.kind.as_deref())?;
    let slot = args.slot.and_then(AdvancedSlotId::new);
    let spec = kind.map(|kind| advanced_spec(kind, args)).transpose()?;
    let intent = match (key, spec, args.unbind, args.clear, slot) {
        (Some(key), Some(spec), false, false, slot) => {
            KeymapIntent::BindAdvanced { key, spec, slot }
        }
        (Some(key), None, true, false, None) => KeymapIntent::UnbindAdvanced { key },
        (None, Some(spec), false, false, Some(slot)) => {
            // A slot without a key has no default tap or toggled key: a
            // usage error now, before the keyboard is opened.
            spec.def(None)?;
            KeymapIntent::SetAdvancedSlot {
                slot,
                spec: Some(spec),
            }
        }
        (None, None, false, true, Some(slot)) => KeymapIntent::SetAdvancedSlot { slot, spec: None },
        _ => return Err(forms_error("set advanced", FORMS).into()),
    };
    let (commit_trigger, mode) = commit_and_mode(args.commit_trigger, &args.flags)?;
    let cmd = KeymapCmd {
        name: "set advanced",
        reason: "set-advanced",
        intent,
        layer: args.layer.layer(),
        acks: acks(&args.allow_builtin, &args.allow_critical_builtin),
        keep_slot: args.keep_slot,
        commit_trigger,
        mode,
    };
    run(connector, ui, globals, &cmd, &args.flags.write)
}

// ----- set colors -----

/// `set colors`.
struct ColorsCmd {
    colors: Vec<KeyColor>,
    all: Option<Rgb>,
    custom_mode: bool,
    mode: PersistenceCheckMode,
}

impl EditCommand for ColorsCmd {
    type Plan = ColorEditPlan;

    fn name(&self) -> &'static str {
        "set colors"
    }

    fn reason(&self) -> &'static str {
        "set-colors"
    }

    fn plan<T: Transport>(&self, session: &mut Session<T>) -> anyhow::Result<Planned<Self::Plan>> {
        let model = model_with_unit(session, false)?;
        let mut colors: Vec<(KeyPos, Rgb)> = match self.all {
            Some(color) => resolve_keys(model, &[], true, KeyUse::Color)?
                .into_iter()
                .map(|pos| (pos, color))
                .collect(),
            None => Vec::new(),
        };
        for KeyColor { key, color } in &self.colors {
            let pos = key.resolve_as(model, KeyUse::Color).map_err(UsageError)?;
            match colors.iter_mut().find(|(p, _)| *p == pos) {
                Some(entry) => entry.1 = *color,
                None => colors.push((pos, *color)),
            }
        }
        let plan = plan_color_edit(
            session,
            &ColorEdit {
                colors,
                switch_to_custom: self.custom_mode,
            },
        )?;
        let switching = plan
            .writes()
            .iter()
            .any(|w| w.table == openwraith_protocol::TableId::MainLight);
        Ok(Planned {
            question: format!(
                "Write custom colour table 1 of profile {} ({}){}?",
                plan.profile(),
                keys_count(plan.changes().len()),
                if switching {
                    " and switch the main light to the Custom mode"
                } else {
                    ""
                }
            ),
            text: edit_text::colors_plan(&plan, model, self.mode),
            writes: named(plan.writes()),
            nothing: "Nothing to change: the keys already have these colours. Nothing was \
                      written.\n"
                .to_string(),
            model: Some(model),
            plan,
        })
    }

    fn apply<T: WriteTransport>(
        &self,
        session: &mut Session<T>,
        plan: &Self::Plan,
        saved: &SavedBackup,
        consent: WriteConsent,
        progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
    ) -> EditReport {
        apply_color_edit(
            session,
            plan,
            saved,
            consent,
            &edit_options(self.mode),
            progress,
        )
    }

    fn persistence(&self) -> PersistNote {
        PersistNote::Mode(self.mode)
    }
}

/// `openwraith set colors KEY=RRGGBB... [--all RRGGBB] [--custom-mode]`.
pub fn set_colors<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &SetColorsArgs,
) -> anyhow::Result<Exit> {
    let cmd = ColorsCmd {
        colors: args.colors.clone(),
        all: args.all,
        custom_mode: args.custom_mode,
        mode: args.flags.check_persistence.mode(),
    };
    run(connector, ui, globals, &cmd, &args.flags.write)
}

// ----- set deadband -----

/// `set deadband`.
struct DeadBandCmd {
    keys: Vec<KeySpec>,
    all: bool,
    top: Option<Distance>,
    bottom: Option<Distance>,
    mode: PersistenceCheckMode,
}

impl EditCommand for DeadBandCmd {
    type Plan = DeadBandEditPlan;

    fn name(&self) -> &'static str {
        "set deadband"
    }

    fn reason(&self) -> &'static str {
        "set-deadband"
    }

    fn plan<T: Transport>(&self, session: &mut Session<T>) -> anyhow::Result<Planned<Self::Plan>> {
        let model = model_with_unit(session, true)?;
        let keys = resolve_keys(model, &self.keys, self.all, KeyUse::DeadBand)?;
        let plan = plan_dead_band_edit(
            session,
            &DeadBandEdit {
                keys,
                top: self.top,
                bottom: self.bottom,
            },
        )?;
        Ok(Planned {
            question: format!(
                "Write the dead band ({}; it may apply to every profile) with profile {} active?",
                keys_count(plan.changes().len()),
                plan.profile()
            ),
            text: edit_text::dead_band_plan(&plan, model, self.mode),
            writes: named(plan.writes()),
            nothing: "Nothing to change: the keys already have these dead zones. Nothing was \
                      written.\n"
                .to_string(),
            model: Some(model),
            plan,
        })
    }

    fn apply<T: WriteTransport>(
        &self,
        session: &mut Session<T>,
        plan: &Self::Plan,
        saved: &SavedBackup,
        consent: WriteConsent,
        progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
    ) -> EditReport {
        apply_dead_band_edit(
            session,
            plan,
            saved,
            consent,
            &edit_options(self.mode),
            progress,
        )
    }

    fn persistence(&self) -> PersistNote {
        PersistNote::Mode(self.mode)
    }
}

/// `openwraith set deadband KEY...|--all [--top MM] [--bottom MM]`.
pub fn set_dead_band<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &SetDeadBandArgs,
) -> anyhow::Result<Exit> {
    let cmd = DeadBandCmd {
        keys: args.keys.clone(),
        all: args.all,
        top: args.top,
        bottom: args.bottom,
        mode: args.flags.check_persistence.mode(),
    };
    run(connector, ui, globals, &cmd, &args.flags.write)
}

// ----- set socd -----

/// How `set socd` changes the pairs.
#[derive(Debug, Clone, Default)]
struct PairsChange {
    clear: bool,
    add: Vec<SocdPairSpec>,
    remove: Vec<SocdPairSpec>,
}

impl PairsChange {
    /// The pairs after the change, from the keyboard's `current` pairs, and
    /// notes for the plan. A pair given again without a mode stays as it is
    /// (mode and key order); with a mode it is stored as typed (key 1 first).
    fn apply(&self, current: Vec<SocdPair>) -> Result<(Vec<SocdPair>, Vec<String>), UsageError> {
        let mut pairs = if self.clear { Vec::new() } else { current };
        for remove in &self.remove {
            let before = pairs.len();
            pairs.retain(|p| !remove.same_keys(p.key1, p.key2));
            if pairs.len() == before {
                return Err(UsageError(format!(
                    "there is no SOCD pair {} / {} to remove",
                    remove.key1, remove.key2
                )));
            }
        }
        let mut notes = Vec::new();
        for add in &self.add {
            match (
                pairs.iter_mut().find(|p| add.same_keys(p.key1, p.key2)),
                add.mode,
            ) {
                // Already there: it keeps its mode and key order.
                (Some(_), None) => {}
                (Some(existing), Some(mode)) => {
                    let swapped = existing.key1 != add.key1;
                    if swapped && matches!(mode, SocdMode::M2 | SocdMode::M3) {
                        notes.push(format!(
                            "{} / {} was stored as {} / {}: key 1 is now {}",
                            add.key1, add.key2, existing.key1, existing.key2, add.key1
                        ));
                    }
                    *existing = SocdPair {
                        mode,
                        key1: add.key1,
                        key2: add.key2,
                    };
                }
                (None, mode) => pairs.push(SocdPair {
                    mode: mode.unwrap_or(SocdMode::M1),
                    key1: add.key1,
                    key2: add.key2,
                }),
            }
        }
        Ok((pairs, notes))
    }
}

/// `set socd`.
struct SocdCmd {
    power: Option<bool>,
    pairs: Option<PairsChange>,
    mode: PersistenceCheckMode,
}

impl EditCommand for SocdCmd {
    type Plan = SocdEditPlan;

    fn name(&self) -> &'static str {
        "set socd"
    }

    fn reason(&self) -> &'static str {
        "set-socd"
    }

    fn plan<T: Transport>(&self, session: &mut Session<T>) -> anyhow::Result<Planned<Self::Plan>> {
        let (pairs, notes) = match &self.pairs {
            None => (None, Vec::new()),
            Some(change) if change.clear => {
                let (pairs, notes) = change.apply(Vec::new())?;
                (Some(pairs), notes)
            }
            Some(change) => {
                let state = SocdState::read(session)?;
                let Some(current) = state.pairs else {
                    return Err(UsageError(format!(
                        "the keyboard's SOCD pairs are not in the form OpenWraith writes ({}); \
                         add --clear-pairs to replace them (the undo command cannot bring them \
                         back). Nothing was written.",
                        state.unwritable.unwrap_or("unreadable")
                    ))
                    .into());
                };
                let (pairs, notes) = change.apply(current)?;
                (Some(pairs), notes)
            }
        };
        let plan = plan_socd_edit(
            session,
            &SocdEdit {
                // --clear-pairs replaces pairs in any form: an explicit
                // choice, shown in the plan as not undoable.
                allow_not_undoable: self.pairs.as_ref().is_some_and(|c| c.clear),
                power: self.power,
                pairs,
            },
        )?;
        let mut text = edit_text::socd_plan(&plan, self.mode);
        for note in notes {
            text.push_str(&format!("Note: {note}.\n"));
        }
        Ok(Planned {
            question: format!(
                "Write SOCD (power and pairs; it may apply to every profile) with profile {} \
                 active?",
                plan.profile()
            ),
            text,
            writes: named(plan.writes()),
            nothing: "Nothing to change: SOCD already has this power and these pairs. Nothing \
                      was written.\n"
                .to_string(),
            model: None,
            plan,
        })
    }

    fn apply<T: WriteTransport>(
        &self,
        session: &mut Session<T>,
        plan: &Self::Plan,
        saved: &SavedBackup,
        consent: WriteConsent,
        progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
    ) -> EditReport {
        apply_socd_edit(
            session,
            plan,
            saved,
            consent,
            &edit_options(self.mode),
            progress,
        )
    }

    fn persistence(&self) -> PersistNote {
        PersistNote::Mode(self.mode)
    }
}

/// `openwraith set socd [--on|--off] [--pair A/D[:1-4]]... [--remove-pair A/D]... [--clear-pairs]`.
pub fn set_socd<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &SetSocdArgs,
) -> anyhow::Result<Exit> {
    let power = match (args.on, args.off) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    };
    let pairs =
        (args.clear_pairs || !args.pairs.is_empty() || !args.remove_pairs.is_empty()).then(|| {
            PairsChange {
                clear: args.clear_pairs,
                add: args.pairs.clone(),
                remove: args.remove_pairs.clone(),
            }
        });
    let cmd = SocdCmd {
        power,
        pairs,
        mode: args.flags.check_persistence.mode(),
    };
    run(connector, ui, globals, &cmd, &args.flags.write)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(u: u8) -> HidUsage {
        HidUsage::new(u).expect("usage")
    }

    fn spec(k1: u8, k2: u8, mode: Option<SocdMode>) -> SocdPairSpec {
        SocdPairSpec {
            key1: usage(k1),
            key2: usage(k2),
            mode,
        }
    }

    #[test]
    fn socd_pairs_are_added_changed_and_removed() {
        let ad = SocdPair {
            mode: SocdMode::M1,
            key1: usage(0x04),
            key2: usage(0x07),
        };
        // A mode for an existing pair (either order) replaces it, in the
        // order typed; a swap with mode 2 or 3 is noted.
        let change = PairsChange {
            add: vec![spec(0x07, 0x04, Some(SocdMode::M2))],
            ..PairsChange::default()
        };
        let (pairs, notes) = change.apply(vec![ad]).expect("changed");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].mode, SocdMode::M2);
        assert_eq!(pairs[0].key1, usage(0x07));
        assert_eq!(notes, ["D / A was stored as A / D: key 1 is now D"]);
        // The same pair again without a mode keeps it as it is (mode 2,
        // D first), whatever the order typed.
        let stored = pairs[0];
        let change = PairsChange {
            add: vec![spec(0x04, 0x07, None)],
            ..PairsChange::default()
        };
        let (pairs, notes) = change.apply(vec![stored]).expect("kept");
        assert_eq!(pairs, [stored]);
        assert!(notes.is_empty());
        // A new pair is appended with mode 1 by default.
        let change = PairsChange {
            add: vec![spec(0x50, 0x4F, None)],
            ..PairsChange::default()
        };
        let (pairs, _) = change.apply(vec![ad]).expect("added");
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[1].mode, SocdMode::M1);
        // Removing a pair that does not exist is a usage error.
        let change = PairsChange {
            remove: vec![spec(0x1A, 0x16, None)],
            ..PairsChange::default()
        };
        assert!(change.apply(vec![ad]).is_err());
        let change = PairsChange {
            clear: true,
            ..PairsChange::default()
        };
        assert_eq!(change.apply(vec![ad]).expect("cleared").0, Vec::new());
    }

    #[test]
    fn advanced_keys_take_the_bound_keys_usage_as_default() {
        let toggle = AdvancedSpec::Toggle { key: None };
        assert_eq!(
            toggle.def(Some(usage(0x0B))).expect("toggle"),
            AdvancedKeyDef::Toggle(Toggle { key: usage(0x0B) })
        );
        assert!(toggle.def(None).is_err());
        let mt = AdvancedSpec::ModTap {
            tap: None,
            hold: usage(0xE0),
            threshold_10ms: 30,
        };
        assert_eq!(
            mt.def(Some(usage(0x0A))).expect("mod-tap"),
            AdvancedKeyDef::ModTap(ModTap {
                hold: usage(0xE0),
                tap: usage(0x0A),
                threshold_10ms: 30,
            })
        );
    }
}
