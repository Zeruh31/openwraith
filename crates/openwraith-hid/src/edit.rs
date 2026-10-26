//! Editing settings of the active profile: per-key triggers and the main
//! light (behind `openwraith set` and a GUI's sliders).
//!
//! An edit has two steps, like a restore:
//!
//! 1. [`plan_trigger_edit`] / [`plan_light_edit`] read the current values
//!    (read-only, generic over [`Transport`]) and compute the new ones from
//!    the user's intent, validated against the keyboard's limits. The plan
//!    shows "current -> new" before anything is written.
//! 2. [`apply_trigger_edit`] / [`apply_light_edit`] write the plan. They need
//!    a [`SavedBackup`] of the active profile (made before, see
//!    [`Backup::save_safety`](crate::Backup::save_safety)) and consent. Each
//!    key (or the light record) is re-read first: if it no longer holds the
//!    planned current value the edit stops with `changed_since_plan`; if it
//!    already holds the new value nothing is written. Writes are single-key
//!    trigger packets (the only captured form) and a main-light write with
//!    the music-rhythm leave only when the mode changes; every write is
//!    verified by reading it back.
//!
//! Edits never switch profiles. A GUI commits on release (one edit per
//! gesture), never a stream of writes.
//!
//! The keyboard saves a write about 1 s after it (spec 4.5.3, "Delayed
//! commit"): a profile switch or a power loss before that loses it, and the
//! read-back cannot tell. An edit therefore waits for the writer's
//! [`CommitSettle`](crate::CommitSettle) (2 s) after its last write before
//! it returns ([`EditReport::waited_for_commit_ms`]). Unlike a restore, it
//! does not prove the save by switching profiles away and back: that would
//! change the lighting for every small edit, so its items stay
//! [`Persistence::NotChecked`](crate::Persistence::NotChecked) and the wait
//! is relied on (any later profile switch of the same session waits too).
//! The active profile is read again after the last write and at the end of
//! the wait; when another profile became active (a switch on the keyboard
//! need not be reported) or the connection was lost before the save, the
//! edit fails and its writes are reported as
//! [`ItemResult::MaybeLost`].

use std::ops::ControlFlow;
use std::path::PathBuf;

use openwraith_protocol::{
    ActiveProfile, Distance, FromResponse, KeyMask, KeyPos, LightLimits, LightMode, LightModeList,
    LightRecord, MainLightWrite, ProfileId, ReadRequest, Rgb, TableId, TriggerGlobal,
    TriggerLimits, TriggerSetting, TriggerWrite, TriggerWriteMode, WriteRequest,
};
use serde::Serialize;

use crate::error::{ErrorInfo, RestoreError, SessionError};
use crate::restore::{
    Checked, ItemReport, ItemResult, Outcome, TableFailure, WriteProgress, confirm_profile,
    profile_after_gate_failure, write_checked,
};
use crate::safety::SavedBackup;
use crate::session::{ForeignActivity, Session};
use crate::summary::identify;
use crate::transport::{Transport, WriteTransport};
use crate::writer::{PacketProgress, WriteConsent, WriteOptions};

/// What to do with Rapid Trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "rapid_trigger", rename_all = "snake_case")]
pub enum RtEdit {
    /// Rapid Trigger off (mode `00`); the press and release values are kept.
    Off,
    /// Rapid Trigger with one sensitivity for press and release (mode `0C`).
    Same {
        /// The sensitivity.
        sensitivity: Distance,
    },
    /// Rapid Trigger with separate press and release sensitivity (mode `0D`).
    Separate {
        /// Press sensitivity.
        press: Distance,
        /// Release sensitivity.
        release: Distance,
    },
}

/// A trigger edit: the keys and what to change (at least one of the two).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TriggerEdit {
    /// The keys (not the knob).
    pub keys: Vec<KeyPos>,
    /// New actuation point, if it changes.
    pub actuation: Option<Distance>,
    /// New Rapid Trigger setting, if it changes.
    pub rapid_trigger: Option<RtEdit>,
}

/// The planned change of one key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyEdit {
    /// The key.
    pub pos: KeyPos,
    /// The key's name.
    pub name: String,
    /// The settings now.
    pub current: TriggerSetting,
    /// The settings after the edit.
    pub target: TriggerSetting,
    /// The write (`None` when the key already has the target settings).
    #[serde(skip)]
    request: Option<WriteRequest>,
}

impl KeyEdit {
    /// Whether the key changes.
    pub fn changes(&self) -> bool {
        self.request.is_some()
    }
}

/// A planned trigger edit; made by [`plan_trigger_edit`], not
/// deserializable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TriggerEditPlan {
    profile: ProfileId,
    product: Option<String>,
    keys: Vec<KeyEdit>,
}

impl TriggerEditPlan {
    /// The active profile the plan was made for.
    pub fn profile(&self) -> ProfileId {
        self.profile
    }

    /// Every key, in the requested order.
    pub fn keys(&self) -> &[KeyEdit] {
        &self.keys
    }

    /// Whether anything would be written.
    pub fn has_changes(&self) -> bool {
        self.keys.iter().any(KeyEdit::changes)
    }
}

/// A main-light edit: what to change (at least one field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct LightEdit {
    /// New effect mode. On a mode change the direction is set to the mode's
    /// default (spec section 4.7.3).
    pub mode: Option<LightMode>,
    /// New foreground colour.
    pub foreground: Option<Rgb>,
    /// New brightness.
    pub brightness: Option<u8>,
    /// New speed.
    pub speed: Option<u8>,
}

/// A planned main-light edit; made by [`plan_light_edit`], not
/// deserializable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LightEditPlan {
    profile: ProfileId,
    product: Option<String>,
    current: LightRecord,
    target: LightRecord,
    #[serde(skip)]
    request: Option<WriteRequest>,
}

impl LightEditPlan {
    /// The active profile the plan was made for.
    pub fn profile(&self) -> ProfileId {
        self.profile
    }

    /// The record now.
    pub fn current(&self) -> &LightRecord {
        &self.current
    }

    /// The record after the edit.
    pub fn target(&self) -> &LightRecord {
        &self.target
    }

    /// Whether anything would be written.
    pub fn has_changes(&self) -> bool {
        self.request.is_some()
    }

    /// Whether the edit changes the mode (and so sends the music-rhythm
    /// leave first).
    pub fn changes_mode(&self) -> bool {
        self.current.as_bytes()[0] != self.target.as_bytes()[0]
    }
}

/// What an edit did. Serializable for a GUI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditReport {
    /// How it ended.
    pub outcome: Outcome,
    /// The error, if it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    /// How the user agreed.
    pub consent: WriteConsent,
    /// The profile that was edited.
    pub profile: u8,
    /// The profile that is active now, as far as known (as last read, or as
    /// a profile report of a switch named it).
    pub active_profile_now: Option<u8>,
    /// One entry per planned write.
    pub items: Vec<ItemReport>,
    /// The safety backup made before the edit.
    pub safety_backup: PathBuf,
    /// The command that undoes the edit.
    pub undo_command: String,
    /// Reports of another program seen while writing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreign_activity: Option<ForeignActivity>,
    /// How long the edit waited at the end for the keyboard to save its last
    /// write, in milliseconds; `None` when nothing was pending (or the
    /// writer could not wait after a reset or a lost connection).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waited_for_commit_ms: Option<u64>,
}

impl EditReport {
    /// Whether every planned write was made and verified.
    pub fn is_success(&self) -> bool {
        matches!(self.outcome, Outcome::Completed | Outcome::NothingToDo)
    }

    /// Number of writes made and verified, not counting the ones the
    /// keyboard may not have saved ([`maybe_lost`](Self::maybe_lost)).
    pub fn written(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.result, ItemResult::Written { .. }))
            .count()
    }

    /// Number of writes made and verified that the keyboard may not have
    /// saved ([`ItemResult::MaybeLost`]: the active profile changed or the
    /// connection was lost before the save).
    pub fn maybe_lost(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.result, ItemResult::MaybeLost { .. }))
            .count()
    }

    /// Whether anything was written, also when the keyboard may not have
    /// saved it.
    pub fn anything_written(&self) -> bool {
        self.items.iter().any(|i| i.result.was_sent())
    }

    fn fail(&mut self, error: ErrorInfo) {
        self.outcome = Outcome::Failed;
        if self.error.is_none() {
            self.error = Some(error);
        }
    }
}

fn invalid(reason: impl Into<String>) -> RestoreError {
    RestoreError::InvalidRequest {
        reason: reason.into(),
    }
}

fn decode_error(request: ReadRequest, source: openwraith_protocol::DecodeError) -> RestoreError {
    RestoreError::from(SessionError::Decode { request, source })
}

/// The active profile, which must be 1-4.
fn active_profile<T: Transport>(session: &mut Session<T>) -> Result<ProfileId, RestoreError> {
    let active: ActiveProfile = session.read(ReadRequest::ActiveProfile)?;
    ProfileId::new(active.profile).ok_or_else(|| {
        invalid(format!(
            "the keyboard reports active profile {}; only profiles 1-4 are supported",
            active.profile
        ))
    })
}

/// Reads the current trigger settings of the keys and plans the edit
/// (read-only).
pub fn plan_trigger_edit<T: Transport>(
    session: &mut Session<T>,
    edit: &TriggerEdit,
) -> Result<TriggerEditPlan, RestoreError> {
    if edit.actuation.is_none() && edit.rapid_trigger.is_none() {
        return Err(invalid(
            "nothing to change: give an actuation point or a Rapid Trigger setting",
        ));
    }
    if edit.keys.is_empty() {
        return Err(invalid("no key given"));
    }
    let (_, info, model) = identify(session)?;
    let profile = active_profile(session)?;
    let global_raw = session.request(ReadRequest::TriggerGlobal)?;
    let global = TriggerGlobal::from_response(&global_raw)
        .map_err(|e| decode_error(ReadRequest::TriggerGlobal, e))?;
    let limits = TriggerLimits::new(model, &global);
    if !limits.is_supported() {
        return Err(invalid(
            "the keyboard's trigger parameters (21/04) do not allow trigger edits",
        ));
    }
    let mut keys: Vec<KeyEdit> = Vec::with_capacity(edit.keys.len());
    for &pos in &edit.keys {
        if keys.iter().any(|k| k.pos == pos) {
            continue;
        }
        let name = model.key_name(pos.index());
        if model.key_at(pos).is_none() || Some(pos.index()) == model.knob_index {
            return Err(invalid(format!(
                "{name} ({pos}) has no trigger settings that can be changed"
            )));
        }
        let raw = session.request(ReadRequest::KeyTrigger(pos))?;
        let (_, current) = TriggerSetting::from_response(&raw).map_err(|e| {
            invalid(format!(
                "cannot edit {name}: its current settings cannot be written back ({e})"
            ))
        })?;
        let target = edited(current, edit);
        let write = TriggerWrite::new(model, KeyMask::from_positions([pos]), target, &limits)
            .map_err(|e| invalid(format!("{name}: {e}")))?;
        let request = (target != current).then_some(WriteRequest::Trigger(write));
        keys.push(KeyEdit {
            pos,
            name,
            current,
            target,
            request,
        });
    }
    Ok(TriggerEditPlan {
        profile,
        product: info.product,
        keys,
    })
}

/// The settings after applying `edit` to `current`.
fn edited(current: TriggerSetting, edit: &TriggerEdit) -> TriggerSetting {
    let mut target = current;
    if let Some(actuation) = edit.actuation {
        target.actuation = actuation;
    }
    match edit.rapid_trigger {
        None => {}
        Some(RtEdit::Off) => target.mode = TriggerWriteMode::Off,
        Some(RtEdit::Same { sensitivity }) => {
            target.mode = TriggerWriteMode::RapidTrigger;
            target.rt_press = sensitivity;
            target.rt_release = sensitivity;
        }
        Some(RtEdit::Separate { press, release }) => {
            target.mode = TriggerWriteMode::RapidTriggerSeparate;
            target.rt_press = press;
            target.rt_release = release;
        }
    }
    target
}

/// Reads the current main light record and plans the edit (read-only).
pub fn plan_light_edit<T: Transport>(
    session: &mut Session<T>,
    edit: &LightEdit,
) -> Result<LightEditPlan, RestoreError> {
    if edit.mode.is_none()
        && edit.foreground.is_none()
        && edit.brightness.is_none()
        && edit.speed.is_none()
    {
        return Err(invalid(
            "nothing to change: give a mode, colour, brightness or speed",
        ));
    }
    let (_, info, model) = identify(session)?;
    let profile = active_profile(session)?;
    let modes_raw = session.request(ReadRequest::MainLightModes)?;
    let modes = LightModeList::from_response(&modes_raw)
        .map_err(|e| decode_error(ReadRequest::MainLightModes, e))?;
    let limits = LightLimits::new(model, &modes);
    let current_raw = session.request(ReadRequest::MainLight)?;
    let current = LightRecord::from_response(&current_raw).map_err(|e| invalid(e.to_string()))?;
    let mut target = current;
    if let Some(mode) = edit.mode {
        if mode != current.mode() {
            target = target
                .with_mode(mode)
                .with_direction(LightRecord::default_direction(mode));
        }
    }
    if let Some(color) = edit.foreground {
        target = target.with_foreground(color);
    }
    if let Some(brightness) = edit.brightness {
        target = target.with_brightness(brightness);
    }
    if let Some(speed) = edit.speed {
        target = target.with_speed(speed);
    }
    let write =
        MainLightWrite::new(&current, target, &limits).map_err(|e| invalid(e.to_string()))?;
    let request = (target != current).then_some(WriteRequest::MainLight(write));
    Ok(LightEditPlan {
        profile,
        product: info.product,
        current,
        target,
        request,
    })
}

/// One write of an edit: the table, key, request and the bytes it was
/// planned against.
struct EditWrite {
    table: TableId,
    key: Option<KeyPos>,
    request: WriteRequest,
    planned: Vec<Option<Vec<u8>>>,
}

/// Writes a planned trigger edit (see the [module documentation](self)).
pub fn apply_trigger_edit<T: WriteTransport>(
    session: &mut Session<T>,
    plan: &TriggerEditPlan,
    saved: &SavedBackup,
    consent: WriteConsent,
    options: &WriteOptions,
    progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> EditReport {
    let writes = plan
        .keys
        .iter()
        .filter_map(|k| {
            k.request.clone().map(|request| EditWrite {
                table: TableId::Triggers,
                key: Some(k.pos),
                request,
                planned: vec![Some(k.current.read_bytes().to_vec())],
            })
        })
        .collect();
    apply(
        session,
        plan.profile,
        plan.product.as_deref(),
        writes,
        saved,
        consent,
        options,
        progress,
    )
}

/// Writes a planned main-light edit (see the [module documentation](self)).
pub fn apply_light_edit<T: WriteTransport>(
    session: &mut Session<T>,
    plan: &LightEditPlan,
    saved: &SavedBackup,
    consent: WriteConsent,
    options: &WriteOptions,
    progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> EditReport {
    let writes = plan
        .request
        .clone()
        .map(|request| EditWrite {
            table: TableId::MainLight,
            key: None,
            request,
            planned: vec![Some(plan.current.as_bytes().to_vec())],
        })
        .into_iter()
        .collect();
    apply(
        session,
        plan.profile,
        plan.product.as_deref(),
        writes,
        saved,
        consent,
        options,
        progress,
    )
}

#[allow(clippy::too_many_arguments)]
fn apply<T: WriteTransport>(
    session: &mut Session<T>,
    profile: ProfileId,
    product: Option<&str>,
    writes: Vec<EditWrite>,
    saved: &SavedBackup,
    consent: WriteConsent,
    options: &WriteOptions,
    mut progress: impl FnMut(WriteProgress) -> ControlFlow<()>,
) -> EditReport {
    let mut report = EditReport {
        outcome: Outcome::Completed,
        error: None,
        consent,
        profile: profile.get(),
        active_profile_now: Some(profile.get()),
        items: writes
            .iter()
            .map(|w| ItemReport {
                table: w.table,
                key: w.key,
                summary: w.request.to_string(),
                commit_trigger: None,
                resent_unchanged: false,
                result: ItemResult::NotAttempted,
            })
            .collect(),
        safety_backup: saved.path().to_path_buf(),
        undo_command: saved.undo_command(),
        foreign_activity: None,
        waited_for_commit_ms: None,
    };
    if !saved.covers(profile) || saved.product() != product {
        let error = RestoreError::SafetyBackupMismatch {
            path: saved.path().to_path_buf(),
        };
        report.fail(ErrorInfo::new(error.kind(), &error));
        return report;
    }
    if writes.is_empty() {
        report.outcome = Outcome::NothingToDo;
        return report;
    }
    let mut writer = match session.writer_with(consent, options.clone()) {
        Ok(writer) => writer,
        Err(e) => {
            report.fail(ErrorInfo::new(e.kind(), &e));
            report.active_profile_now =
                profile_after_gate_failure(session, &e, Some(profile.get()));
            return report;
        }
    };
    let mut cancel = false;
    for (index, write) in writes.iter().enumerate() {
        let start = WriteProgress::Writing {
            profile: profile.get(),
            table: write.table,
            key: write.key,
            packet: 0,
            of: write.request.steps().len(),
            send: 1,
        };
        if cancel || progress(start).is_break() {
            report.outcome = Outcome::Cancelled;
            break;
        }
        let mut on_packet = |p: PacketProgress| {
            let step = WriteProgress::Writing {
                profile: profile.get(),
                table: write.table,
                key: write.key,
                packet: p.packet,
                of: p.of,
                send: p.send,
            };
            if progress(step).is_break() {
                cancel = true;
            }
        };
        let result = write_checked(
            &mut writer,
            profile,
            Checked::Table(&write.request),
            &write.planned,
            &[],
            &mut on_packet,
        );
        match result {
            Ok(result) => {
                if let Some(item) = report.items.get_mut(index) {
                    item.result = result;
                }
            }
            Err(TableFailure { error, mismatch }) => {
                if let Some(item) = report.items.get_mut(index) {
                    item.result = ItemResult::Failed {
                        error: error.clone(),
                        mismatch,
                    };
                }
                report.fail(error);
                break;
            }
        }
    }
    if !writer.is_poisoned() && report.outcome != Outcome::Failed {
        if let Err(stop) = confirm_profile(&mut writer, profile) {
            let failure = TableFailure::from(stop);
            report.fail(failure.error);
        }
    }
    // Done only when the keyboard had time to save the last write (also
    // after a failure or a cancellation: something may have been written).
    // The wait ends by reading the active profile again: a switch made on
    // the keyboard during it (reported or not) may have discarded the
    // writes. After a switch already seen, nothing is left to wait for.
    if !writer.is_poisoned() {
        let waited = writer.wait_for_commit_observed(&mut |wait| {
            let _ = progress(WriteProgress::waiting(wait));
        });
        match waited {
            Ok(wait) => report.waited_for_commit_ms = wait.map(|w| w.wait_ms),
            Err(e) => report.fail(ErrorInfo::new(e.kind(), &e)),
        }
    }
    // Verified in the keyboard's working copy only: after a profile change
    // or a lost connection before the save they may be gone.
    if writer.commit_interrupted() && writer.session().last_write_profile() == Some(profile.get()) {
        for item in &mut report.items {
            let result = std::mem::replace(&mut item.result, ItemResult::NotAttempted);
            item.result = result.into_maybe_lost();
        }
    }
    // As last read or reported (a profile report of a switch included).
    report.active_profile_now = writer.known_profile().map(ProfileId::get);
    report.foreign_activity = writer.foreign_activity_seen();
    report
}
