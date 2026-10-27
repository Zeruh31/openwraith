//! The persistence check, shared by restores and edits: select a written
//! profile again after switching away from it, so that the keyboard reloads
//! what it saved, and read the written tables again (spec 4.5.3, "Delayed
//! commit": a read-back right after a write reads the working copy).
//!
//! [`persistence_confirmed`] is the evidence table: which written tables are
//! known (from hardware tests) to be saved by the keyboard after the commit
//! settle. An edit checks the others by default
//! ([`PersistenceCheckMode::Auto`](crate::edit::PersistenceCheckMode::Auto)).
//! Flip an entry after a hardware test proves it, one line each.

use openwraith_protocol::{Layer, ProfileId, TableId, WriteRequest};

use crate::error::WriteError;
use crate::restore::{ItemResult, Persistence, TableState, expected_state, state_of};
use crate::transport::WriteTransport;
use crate::writer::{CommitWait, WriteWarning, Writer};

/// Whether hardware tests showed that the keyboard keeps a write of `table`
/// after the commit settle. `complete_fn_before`: an advanced-key slot was
/// written in a keymap group that sent a complete Fn keymap before it (the
/// site's order, which persisted in c16 B1 and the restore of 2026-10-27).
///
/// * triggers, main light, dead band, custom colours 1, both keymap layers
///   (the base keymap is always followed by the Fn keymap): CONFIRMED;
/// * advanced-key slots: only after a complete Fn keymap;
/// * macro slots, SOCD and everything else: not tested (hardware tests
///   H19, H21, H22 in docs/HARDWARE_TESTS.md).
pub(crate) fn persistence_confirmed(table: TableId, complete_fn_before: bool) -> bool {
    match table {
        TableId::Triggers
        | TableId::MainLight
        | TableId::DeadBand
        | TableId::CustomColors {
            table: openwraith_protocol::CustomTable::Custom1,
        }
        | TableId::Keymap {
            layer: Layer::Base | Layer::Fn,
        } => true,
        TableId::AdvancedKey { .. } => complete_fn_before,
        _ => false,
    }
}

/// Selects `profile` so that the keyboard reloads it from what it saved:
/// when `profile` is already active, switches to `away` first. Every
/// switch waits for the commit settle (`on_wait` is told). Fails unless the
/// last select really switched (a read of the working copy is never taken
/// for a reload).
pub(crate) fn reload_profile<T: WriteTransport>(
    writer: &mut Writer<'_, T>,
    profile: ProfileId,
    away: ProfileId,
    on_wait: &mut dyn FnMut(CommitWait),
) -> Result<(), WriteError> {
    let mut selected = writer.select_profile_observed(profile, on_wait)?;
    if !selected.switched {
        writer.select_profile_observed(away, on_wait)?;
        selected = writer.select_profile_observed(profile, on_wait)?;
    }
    if !selected.switched {
        return Err(WriteError::ProfileChanged {
            expected: Some(profile.get()),
            reported: selected.profile.get(),
        });
    }
    Ok(())
}

/// What [`recheck`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckOutcome {
    /// It still held what was written.
    Persisted,
    /// It did not ([`ItemResult::Lost`]).
    Lost,
    /// Not counted: not a written item, a table that may be shared and
    /// holds what the operation wrote elsewhere, or SOCD switched on the
    /// keyboard since it was written ([`socd_toggled`]).
    NotChecked,
}

/// The power SOCD has now (`now`, the state of a SOCD write's verify reads
/// as [`state_of`] builds it) when the keyboard's own switch (Fn + Right
/// Shift) was used since `socd_mark` (the session's
/// [`socd_pushes`](crate::Session::socd_pushes) right after the write) and
/// the power is all that differs from the SOCD write `request`. Such a
/// difference is the user's switch, not a write the keyboard lost.
pub(crate) fn socd_toggled(
    socd_pushes: u64,
    request: &WriteRequest,
    now: &TableState,
    socd_mark: Option<u64>,
) -> Option<bool> {
    let WriteRequest::Socd(_) = request else {
        return None;
    };
    if socd_pushes <= socd_mark? {
        return None;
    }
    let want = expected_state(request);
    match (now.as_slice(), want.as_slice()) {
        ([Some(power), pairs], [Some(written), wanted_pairs])
            if pairs == wanted_pairs && power != written =>
        {
            power.first().map(|b| *b == 1)
        }
        _ => None,
    }
}

/// Reads the tables of `request` again (the profile was just reloaded) and
/// turns a [`ItemResult::Written`] `result` into
/// [`Persistence::Persisted`] or [`ItemResult::Lost`]. A state accepted by
/// `written_elsewhere` (a possibly shared table that holds what the
/// operation wrote in another profile) stays [`Persistence::NotChecked`],
/// and so does a SOCD write whose power alone differs after a SOCD switch
/// on the keyboard since `socd_mark` ([`socd_toggled`]; with the warning
/// [`WriteWarning::SocdToggledOnKeyboard`]).
pub(crate) fn recheck<T: WriteTransport>(
    writer: &mut Writer<'_, T>,
    request: &WriteRequest,
    result: &mut ItemResult,
    written_elsewhere: &dyn Fn(&TableState) -> bool,
    socd_mark: Option<u64>,
) -> Result<CheckOutcome, WriteError> {
    let ItemResult::Written {
        sends,
        packets,
        warnings,
        ..
    } = result
    else {
        return Ok(CheckOutcome::NotChecked);
    };
    let (sends, packets, mut warnings) = (*sends, *packets, warnings.clone());
    let mut replies = Vec::new();
    for read in request.verify_reads() {
        replies.push(writer.read(read)?);
    }
    let now = state_of(request, &replies);
    let elsewhere = written_elsewhere(&now);
    let toggled = socd_toggled(writer.session().socd_pushes(), request, &now, socd_mark);
    let (outcome, item) = match request.check(&replies) {
        Ok(()) => (
            CheckOutcome::Persisted,
            ItemResult::Written {
                sends,
                packets,
                warnings,
                persistence: Persistence::Persisted,
            },
        ),
        Err(_) if elsewhere => (
            CheckOutcome::NotChecked,
            ItemResult::Written {
                sends,
                packets,
                warnings,
                persistence: Persistence::NotChecked,
            },
        ),
        Err(_) if toggled.is_some() => {
            let power = toggled.unwrap_or_default();
            let warning = WriteWarning::SocdToggledOnKeyboard { power };
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
            (
                CheckOutcome::NotChecked,
                ItemResult::Written {
                    sends,
                    packets,
                    warnings,
                    persistence: Persistence::NotChecked,
                },
            )
        }
        Err(mismatch) => (
            CheckOutcome::Lost,
            ItemResult::Lost {
                sends,
                packets,
                warnings,
                mismatch,
            },
        ),
    };
    *result = item;
    Ok(outcome)
}
