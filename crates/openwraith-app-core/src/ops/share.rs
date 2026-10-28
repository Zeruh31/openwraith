//! Sharing one profile as a file. An export is the library's
//! `Backup::export_profile` (or `export_profile_parts` with the chosen
//! categories) of a fresh read or of a backup file (one the user opened, a
//! safety backup or an automatic backup): no USB serial number, the raw
//! replies authoritative. An import is a restore
//! with the library's `ImportOptions`: the file is untrusted input that the
//! library validates (only the chosen categories, the knob, the Fn key and
//! locked keys keep the keyboard's entries, a critical Fn override needs
//! its own explicit permission), compared with the chosen profile of the
//! keyboard and written through the restore's safety backup, journal,
//! persistence check and Undo.

use std::sync::Arc;

use openwraith_hid::protocol::{EntryKind, KeyAssignment, KeyPos, Layer, ProfileId, ReadRequest};
use openwraith_hid::{
    AdvancedSlotView, Backup, BlockCause, ImportOptions, ImportSelection, KeymapGroupState,
    MacroSlotView, PlanAction, PreparedRestore, ProfileSet, ProfilesOptions, RestoreOptions,
    SkipReason, SnapshotView, WriteOptions, prepare_restore, read_profiles,
};

use super::consent;
use super::progress::CancelPolicy;
use super::restore::{check_device, load_backup, summary_of};
use super::snapshot::{ReadFail, ReadVia};
use crate::backend::DeviceBackend;
use crate::dto::{
    AppError, AppErrorKind, EditProblemView, ErrorDetail, ExportIntent, ExportSource,
    ExportedProfile, FileToken, ImportIntent, ImportPreview, ImportSummaryView, IntentField,
    KeyRef, OpKind, PlanInvalidation, PlanToken, ReadPurpose, RestoreReportView, ShareContentsView,
    ShareSelectionView, SharedMacroView,
};
use crate::files::FileKind;
use crate::map::editors::refused;
use crate::map::keys::program_view;
use crate::map::{clean, key_ref_index, restore_plan_view};
use crate::sink::ProgressSink;
use crate::worker::{PendingPlan, PlanKind, Worker, stale};

/// The longest export label (the library keeps at most 120 characters).
const MAX_LABEL: usize = 120;

/// The file extension of a profile export.
pub(crate) const PROFILE_EXTENSION: &str = "owprofile.json";

/// The categories ticked by default for an import (the library's default:
/// triggers and lighting; keys, dead band and SOCD only when chosen).
pub(crate) fn default_selection() -> ShareSelectionView {
    selection_view(ImportSelection::default())
}

fn selection_view(s: ImportSelection) -> ShareSelectionView {
    ShareSelectionView {
        triggers: s.triggers,
        lighting: s.lighting,
        keys: s.keymap_group,
        dead_band: s.dead_band,
        socd: s.socd,
    }
}

fn selection(s: ShareSelectionView) -> ImportSelection {
    ImportSelection {
        triggers: s.triggers,
        lighting: s.lighting,
        keymap_group: s.keys,
        dead_band: s.dead_band,
        socd: s.socd,
    }
}

/// What a profile of a file holds.
pub(crate) fn contents(view: SnapshotView<'_>) -> ShareContentsView {
    let has = |read: ReadRequest| view.response(read).is_some();
    let triggers = view
        .model()
        .is_some_and(|m| m.positions().any(|p| has(ReadRequest::KeyTrigger(p))));
    let group = KeymapGroupState::from_snapshot(view).ok();
    let macros = group.as_ref().map_or_else(Vec::new, |g| {
        openwraith_hid::protocol::MacroSlotId::all()
            .filter_map(|slot| match g.macro_program(slot) {
                MacroSlotView::Empty => None,
                MacroSlotView::Program { program } => Some(SharedMacroView {
                    slot: slot.get(),
                    program: Some(program_view(&program)),
                }),
                MacroSlotView::Raw { .. } => Some(SharedMacroView {
                    slot: slot.get(),
                    program: None,
                }),
            })
            .collect()
    });
    let advanced_slots = group.as_ref().map_or(0, |g| {
        openwraith_hid::protocol::AdvancedSlotId::all()
            .filter(|slot| !matches!(g.advanced(*slot), AdvancedSlotView::Empty))
            .count()
    });
    ShareContentsView {
        present: ShareSelectionView {
            triggers,
            lighting: has(ReadRequest::MainLight),
            keys: has(ReadRequest::Keymap(Layer::Base)) && has(ReadRequest::Keymap(Layer::Fn)),
            dead_band: has(ReadRequest::DeadBand),
            socd: has(ReadRequest::SocdPairs),
        },
        macros,
        advanced_slots: u8::try_from(advanced_slots).unwrap_or(u8::MAX),
    }
}

fn profile_id(profile: u8) -> Result<ProfileId, AppError> {
    ProfileId::new(profile).ok_or_else(|| {
        super::invalid(
            ErrorDetail::InvalidIntent {
                field: IntentField::Profiles,
            },
            format!("profile {profile} does not exist (1-4)"),
        )
    })
}

/// The label of an export (control characters removed, trimmed).
fn label(text: Option<&str>) -> Option<String> {
    let text: String = text?
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_LABEL)
        .collect();
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The profile of a file an import takes by default: the file's own (an
/// export holds one), else the one that was active.
fn file_profile(backup: &Backup) -> Option<u8> {
    match backup.profile_numbers().as_slice() {
        [only] => Some(*only),
        _ => backup.active_profile,
    }
}

/// `open_import_file` of a file the user chose: its summary and what the
/// profile to import holds.
pub(crate) fn import_summary(
    shared: &crate::shared::Shared,
    path: &std::path::Path,
) -> Result<ImportSummaryView, AppError> {
    let file = shared.register_file(path.to_path_buf(), FileKind::User);
    summary_of_token(shared, &file.token)
}

/// The import summary of a known file.
pub(crate) fn summary_of_token(
    shared: &crate::shared::Shared,
    token: &FileToken,
) -> Result<ImportSummaryView, AppError> {
    let (backup, kind, path) = load_backup(shared, token)?;
    let connected = shared.snapshot().map(|s| s.device);
    let file = summary_of(token, &backup, kind, &path, connected.as_ref());
    let profile = file_profile(&backup);
    let contents = profile
        .and_then(ProfileId::new)
        .and_then(|p| backup.snapshot(p))
        .map_or_else(
            || ShareContentsView {
                present: ShareSelectionView {
                    triggers: false,
                    lighting: false,
                    keys: false,
                    dead_band: false,
                    socd: false,
                },
                macros: Vec::new(),
                advanced_slots: 0,
            },
            contents,
        );
    Ok(ImportSummaryView {
        file,
        profile,
        contents,
        default_include: default_selection(),
    })
}

/// How a plan leaves out chosen categories as a whole.
#[derive(Debug, Default)]
struct Blocked {
    /// A critical Fn override the user did not allow blocked the keys (the
    /// key the library stopped at).
    critical: Option<u8>,
    /// The keys were blocked by something else (an entry, macro or slot of
    /// the file OpenWraith does not write, an incomplete table): no
    /// permission imports them.
    keys_otherwise: bool,
    /// The SOCD pairs are not in the vendor site's form.
    socd: bool,
}

impl Blocked {
    /// A chosen category is left out as a whole.
    fn any(&self) -> bool {
        self.critical.is_some() || self.keys_otherwise || self.socd
    }

    /// A chosen category is left out for a cause the critical permission
    /// does not lift.
    fn otherwise(&self) -> bool {
        self.keys_otherwise || self.socd
    }
}

/// What the plan leaves out of the chosen categories as a whole (the keys
/// blocked, and why; SOCD pairs not in the site's form).
fn blocked(plan: &openwraith_hid::RestorePlan, include: ImportSelection) -> Blocked {
    let mut blocked = Blocked::default();
    for (_, item) in plan.skips() {
        if let PlanAction::Skip { reason, .. } = &item.action {
            match reason {
                SkipReason::KeymapGroupBlocked { cause } if include.includes(item.table) => {
                    if let BlockCause::CriticalFnOverride { index } = cause {
                        blocked.critical.get_or_insert(*index);
                    } else {
                        blocked.keys_otherwise = true;
                    }
                }
                SkipReason::UnwritableSocdPairs { .. } if include.includes(item.table) => {
                    blocked.socd = true;
                }
                _ => {}
            }
        }
    }
    blocked
}

/// Every key whose Fn-layer entry in the file replaces a critical built-in
/// function (Fn + 1..4, Fn + Space) of the keyboard's profile `into`. The
/// library stops at the first such key, so the complete list for the
/// consent is found here, with the library's own rules: only when the keys
/// are imported, never at a key the keyboard locks on the Fn layer (it keeps
/// the keyboard's entry), and only for an entry that differs from the
/// keyboard's and is an assignment other than Default (Default brings the
/// built-in back; an entry OpenWraith does not write blocks the keys
/// anyway). The caller asks only when the plan did not block the keys for
/// another cause.
fn critical_overrides(
    prepared: &PreparedRestore,
    into: ProfileId,
    include: ImportSelection,
) -> Vec<KeyRef> {
    if !include.keymap_group {
        return Vec::new();
    }
    let safety = prepared.safety();
    let (Some(model), Some(keyboard), Some(file)) = (
        safety.model(),
        safety.snapshot(into),
        prepared.target_snapshot(into),
    ) else {
        return Vec::new();
    };
    let fn_layer = |view: SnapshotView<'_>| {
        view.keymaps()
            .into_iter()
            .find(|table| table.layer == Layer::Fn)
            .cloned()
    };
    let (Some(on_keyboard), Some(in_file)) = (fn_layer(keyboard), fn_layer(file)) else {
        return Vec::new();
    };
    let locked = keyboard
        .fn_locked_keys()
        .or_else(|| safety.primary().fn_locked_keys())
        .map_or_else(|| model.layout_mask(), |locked| locked.mask);
    model
        .fn_builtins
        .iter()
        .filter(|builtin| builtin.critical)
        .filter_map(|builtin| {
            let pos = KeyPos::from_index(builtin.index)?;
            if locked.contains(pos) {
                return None;
            }
            let old = on_keyboard.entry(builtin.index)?;
            let new = in_file.entry(builtin.index)?;
            if old.raw == new.raw {
                return None;
            }
            let bytes = <[u8; 4]>::try_from(&new.raw[..]).ok()?;
            match KeyAssignment::classify(model, pos, bytes) {
                EntryKind::Assignable(assignment) if assignment != KeyAssignment::Default => {
                    Some(key_ref_index(builtin.index))
                }
                _ => None,
            }
        })
        .collect()
}

impl<B: DeviceBackend> Worker<B> {
    /// `export_profile`: reads the profile (a pure read when it is active,
    /// else a switch to it and back after the user agreed) or takes it from
    /// a backup file, and saves it as a profile file.
    pub(crate) fn export_profile(
        &mut self,
        intent: &ExportIntent,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<ExportedProfile, AppError> {
        let profile = profile_id(intent.profile)?;
        let (path, file) = self.destination(&intent.destination)?;
        let label = label(intent.label.as_deref());
        let include = intent.include.map(selection);
        if include.is_some_and(|i| i.is_empty()) {
            return Err(refused(
                EditProblemView::NoChanges,
                "choose at least one category to export",
            ));
        }
        let source_file = match &intent.source {
            ExportSource::File { file } => Some(load_backup(&self.shared, file)?.0),
            ExportSource::Keyboard => {
                self.check_readable()?;
                None
            }
        };
        if source_file.is_none() {
            self.invalidate(PlanInvalidation::Operation);
        }
        self.run_op(
            OpKind::Export,
            sink,
            CancelPolicy::Boundaries,
            |w, progress| {
                let mut switched = false;
                let backup = match source_file {
                    Some(backup) => Backup::clone(&backup),
                    None if w.snapshot.as_ref().map(|s| s.profile) == Some(profile.get()) => w
                        .full_read(ReadVia::Op(progress, ReadPurpose::Backup))
                        .map_err(ReadFail::into_error)?,
                    None => {
                        if !intent.confirmed_switch {
                            return Err(AppError::with(
                                AppErrorKind::NeedsSwitchConsent,
                                ErrorDetail::NeedsSwitchConsent {
                                    profiles: vec![profile.get()],
                                },
                                "reading the profile switches to it and back; confirm the switch first",
                            ));
                        }
                        w.check_writable()?;
                        w.ensure_quiet(progress)?;
                        let options = ProfilesOptions {
                            backup: w.config.backup_options(w.sim),
                            write: WriteOptions::default(),
                        };
                        let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                        let result = read_profiles(
                            session,
                            ProfileSet::only(profile),
                            consent::application(),
                            &options,
                            |p| progress.profiles(ReadPurpose::Backup, p),
                        );
                        switched = true;
                        match result {
                            Ok(backup) => backup,
                            Err(error) => {
                                w.active_profile = error.active_profile_now;
                                if error.active_profile_now
                                    != w.snapshot.as_ref().map(|s| s.profile)
                                {
                                    w.reread = true;
                                }
                                return Err(AppError::from(&error));
                            }
                        }
                    }
                };
                // Only the chosen parts (the library's filter), or the whole
                // profile.
                let exported = match include {
                    Some(include) => {
                        backup.export_profile_parts(profile, label.as_deref(), include)
                    }
                    None => backup.export_profile(profile, label.as_deref()),
                }
                .map_err(|e| AppError::from(&e))?;
                progress.step(crate::dto::ProgressStep::SavingFile);
                exported
                    .save(&path, true)
                    .map_err(|e| AppError::from(&e))?;
                Ok(ExportedProfile {
                    file,
                    profile: profile.get(),
                    product: exported.device.product.as_deref().map(clean),
                    firmware: exported.device.firmware.as_deref().map(clean),
                    contents: contents(exported.primary()),
                    switched,
                })
            },
        )
    }

    /// `prepare_import`: compares profile `from` of the file with profile
    /// `into` of the keyboard (switching to it and back after the user
    /// agreed), with the safety backup read on the way.
    pub(crate) fn prepare_import(
        &mut self,
        intent: &ImportIntent,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<ImportPreview, AppError> {
        self.check_writable()?;
        let (backup, kind, path) = load_backup(&self.shared, &intent.source)?;
        let into = profile_id(intent.into)?;
        let from = match intent.from {
            Some(n) => profile_id(n)?,
            None => file_profile(&backup)
                .and_then(ProfileId::new)
                .ok_or_else(|| {
                    refused(
                        EditProblemView::Other {
                            message: "the file does not say which profile it holds".to_string(),
                        },
                        "choose the profile of the file",
                    )
                })?,
        };
        if !backup.profile_numbers().contains(&from.get()) {
            return Err(AppError::with(
                AppErrorKind::InvalidBackup,
                ErrorDetail::ProfileNotInBackup {
                    profile: from.get(),
                },
                format!("profile {from} is not in the file"),
            ));
        }
        let include = intent.include;
        if !(include.triggers
            || include.lighting
            || include.keys
            || include.dead_band
            || include.socd)
        {
            return Err(refused(
                EditProblemView::NoChanges,
                "choose at least one category to import",
            ));
        }
        let device = self.snapshot.as_ref().map(|s| s.device.clone());
        check_device(&backup, device.as_ref(), intent.allow_firmware_mismatch)?;
        let source = summary_of(&intent.source, &backup, kind, &path, device.as_ref());
        self.invalidate(PlanInvalidation::Superseded);
        self.run_op(
            OpKind::ImportPrepare,
            sink,
            CancelPolicy::Boundaries,
            |w, progress| {
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let active = super::profile::read_active(session)?;
                let switches: Vec<u8> = (into.get() != active)
                    .then_some(into.get())
                    .into_iter()
                    .collect();
                if intent.confirmed_switches != switches {
                    return Err(AppError::with(
                        AppErrorKind::NeedsSwitchConsent,
                        ErrorDetail::NeedsSwitchConsent { profiles: switches },
                        "comparing switches profiles; confirm the switches first",
                    ));
                }
                if !switches.is_empty() {
                    w.ensure_quiet(progress)?;
                }
                let options = RestoreOptions {
                    profiles: Some(ProfileSet::only(into)),
                    allow_firmware_mismatch: intent.allow_firmware_mismatch,
                    backup: w.config.backup_options(w.sim),
                    write: WriteOptions::default(),
                    import: Some(ImportOptions {
                        from,
                        into,
                        include: selection(include),
                        // Checked against the complete list below, before a
                        // plan with them is offered.
                        allow_critical_fn_overrides: !intent.allowed_critical_keys.is_empty(),
                    }),
                };
                let session = w.session.as_mut().ok_or_else(Self::not_connected)?;
                let prepared =
                    prepare_restore(session, &backup, &options, consent::application(), |p| {
                        progress.write(p)
                    });
                let prepared = match prepared {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        if let Some(now) = error.active_profile_now() {
                            w.active_profile = Some(now);
                            if Some(now) != w.snapshot.as_ref().map(|s| s.profile) {
                                w.reread = true;
                            }
                        }
                        return Err(AppError::from(&error));
                    }
                };
                let blocked = blocked(prepared.plan(), selection(include));
                // Keys blocked by something else are never imported, so they
                // replace no built-in: nothing to name, nothing to allow.
                let mut critical_fn_overrides = if blocked.keys_otherwise {
                    Vec::new()
                } else {
                    critical_overrides(&prepared, into, selection(include))
                };
                if critical_fn_overrides
                    .iter()
                    .any(|key| !intent.allowed_critical_keys.contains(&key.index))
                    && !intent.allowed_critical_keys.is_empty()
                {
                    // The file replaces a critical built-in the user was not
                    // shown (the keyboard changed since the last comparison):
                    // never offer a plan with it; the flow compares again
                    // without the permission.
                    return Err(stale(PlanInvalidation::ValuesChanged));
                }
                let plan = prepared.plan();
                let view = restore_plan_view(plan);
                let skipped = plan.skips().count();
                let has_writes = plan.has_writes();
                if let Some(index) = blocked.critical
                    && !critical_fn_overrides.iter().any(|key| key.index == index)
                {
                    // (The list could not be worked out: the library's key.)
                    critical_fn_overrides.push(key_ref_index(index));
                }
                let critical_blocked = blocked.critical.is_some();
                let category_blocked = blocked.any();
                let other_cause_blocked = blocked.otherwise();
                let token: Option<PlanToken> = has_writes.then(|| {
                    w.store_plan(
                        PlanKind::Import,
                        PendingPlan::Restore {
                            prepared: Box::new(prepared),
                            skipped,
                        },
                    )
                });
                Ok(ImportPreview {
                    token,
                    source,
                    from: from.get(),
                    into: into.get(),
                    include,
                    plan: view,
                    nothing_to_write: !has_writes,
                    switches,
                    critical_fn_overrides,
                    critical_blocked,
                    category_blocked,
                    other_cause_blocked,
                })
            },
        )
    }

    /// `apply_import`: the confirmed import, written like a restore.
    pub(crate) fn apply_import(
        &mut self,
        token: &PlanToken,
        sink: Arc<dyn ProgressSink>,
    ) -> Result<RestoreReportView, AppError> {
        let PendingPlan::Restore { prepared, skipped } = self.take_plan(token, PlanKind::Import)?
        else {
            return Err(stale(PlanInvalidation::Unknown));
        };
        self.apply_prepared_restore(&prepared, skipped, sink, OpKind::Import, "import")
    }
}
