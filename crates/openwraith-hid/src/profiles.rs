//! Reading several onboard profiles: the all-profile backup (schema 2).
//!
//! The keyboard only answers for its **active** profile, so reading another
//! profile means switching to it with `21/12` (write W1, CONFIRMED: the
//! keyboard acknowledges it and pushes the new profile; nothing else changes)
//! and switching back afterwards. [`read_profiles`]:
//!
//! 1. reads the active profile P with the plain [`read_all`](crate::read_all)
//!    plan, without writing anything;
//! 2. passes the "other program" gate of [`Session::writer`];
//! 3. for each requested profile Q other than P, in ascending order: selects
//!    Q, confirms it with `21/11` and reads the complete plan again (with
//!    `21/11` checked at the start and the end);
//! 4. selects P again and confirms it.
//!
//! On any failure it selects P again (best effort) unless the keyboard reset
//! or the connection was lost; the error carries the profile that is active
//! now ([`ProfilesError::active_profile_now`]). Profile switches change the
//! lighting briefly; they write nothing else. A session that wrote a setting
//! before waits for the keyboard to save it before the first switch
//! ([`Writer::select_profile`]); a session without writes never waits.

use std::ops::ControlFlow;
use std::time::SystemTime;

use openwraith_protocol::ProfileId;
use serde::Serialize;

use crate::backup::{Backup, BackupOptions, Progress, Snapshot, read_snapshot, rfc3339_utc};
use crate::error::{BackupError, ProfilesError, ProfilesFailure, WriteError};
use crate::restore::profile_after_gate_failure;
use crate::session::Session;
use crate::transport::WriteTransport;
use crate::writer::{WriteConsent, WriteOptions, Writer};

/// A set of onboard profiles (1-4).
///
/// ```
/// use openwraith_hid::ProfileSet;
/// use openwraith_hid::protocol::ProfileId;
///
/// let set: ProfileSet = [2, 4].into_iter().filter_map(ProfileId::new).collect();
/// assert_eq!(set.iter().map(ProfileId::get).collect::<Vec<_>>(), [2, 4]);
/// assert_eq!(ProfileSet::ALL.len(), 4);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ProfileSet(u8);

impl ProfileSet {
    /// All four profiles.
    pub const ALL: ProfileSet = ProfileSet(0b1111);

    /// No profile.
    pub const fn empty() -> Self {
        ProfileSet(0)
    }

    /// Only `profile`.
    pub fn only(profile: ProfileId) -> Self {
        let mut set = ProfileSet::empty();
        set.insert(profile);
        set
    }

    /// Adds `profile`.
    pub fn insert(&mut self, profile: ProfileId) {
        self.0 |= Self::bit(profile);
    }

    /// Whether `profile` is in the set.
    pub fn contains(&self, profile: ProfileId) -> bool {
        self.0 & Self::bit(profile) != 0
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Number of profiles in the set.
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// The profiles in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = ProfileId> + '_ {
        ProfileId::ALL.into_iter().filter(|p| self.contains(*p))
    }

    fn bit(profile: ProfileId) -> u8 {
        1u8 << (profile.get() - 1)
    }
}

impl FromIterator<ProfileId> for ProfileSet {
    fn from_iter<I: IntoIterator<Item = ProfileId>>(iter: I) -> Self {
        let mut set = ProfileSet::empty();
        for profile in iter {
            set.insert(profile);
        }
        set
    }
}

impl Serialize for ProfileSet {
    /// A list of profile numbers, e.g. `[1, 3]`.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(ProfileId::get))
    }
}

/// Options of [`read_profiles`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfilesOptions {
    /// Tool and creation time recorded in the backup.
    pub backup: BackupOptions,
    /// Timeouts of the profile switches (and the gate).
    pub write: WriteOptions,
}

/// Progress of [`read_profiles`]. Returning [`ControlFlow::Break`] from the
/// callback cancels after the current request; the original profile is then
/// selected again and the result is [`ProfilesFailure::Cancelled`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum ProfilesProgress {
    /// Reading a profile: `profile` is `None` for the first (active) one,
    /// whose number is not known yet.
    Reading {
        /// The profile being read.
        profile: Option<u8>,
        /// Requests completed.
        done: usize,
        /// Requests in total.
        total: usize,
    },
    /// About to switch to another profile.
    Selecting {
        /// The profile.
        profile: u8,
    },
    /// Switching back to the original profile.
    Returning {
        /// The original profile.
        profile: u8,
    },
    /// Waiting for the keyboard to save an earlier write of the same
    /// session before the next profile switch (see
    /// [`CommitSettle`](crate::CommitSettle)). Never happens in a session
    /// that wrote nothing; the return value is ignored.
    WaitingForCommit {
        /// How long the wait lasts, in milliseconds.
        wait_ms: u64,
    },
}

/// Reads all four onboard profiles ([`read_profiles`] with
/// [`ProfileSet::ALL`]).
pub fn read_all_profiles<T: WriteTransport>(
    session: &mut Session<T>,
    consent: WriteConsent,
    options: &ProfilesOptions,
    progress: impl FnMut(ProfilesProgress) -> ControlFlow<()>,
) -> Result<Backup, ProfilesError> {
    read_profiles(session, ProfileSet::ALL, consent, options, progress)
}

/// Reads the active profile and every profile of `which` (see the
/// [module documentation](self)).
///
/// When `which` holds no profile other than the active one, nothing is
/// written and the result is a single-profile backup (schema 1); otherwise
/// it is an all-profile backup (schema 2) whose primary snapshot is the
/// originally active profile.
pub fn read_profiles<T: WriteTransport>(
    session: &mut Session<T>,
    which: ProfileSet,
    consent: WriteConsent,
    options: &ProfilesOptions,
    mut progress: impl FnMut(ProfilesProgress) -> ControlFlow<()>,
) -> Result<Backup, ProfilesError> {
    let created_at = rfc3339_utc(options.backup.created_at.unwrap_or_else(SystemTime::now));

    // 1. The active profile: a pure read.
    let primary = read_snapshot(session, None, &mut |p: Progress| {
        progress(ProfilesProgress::Reading {
            profile: None,
            done: p.done,
            total: p.total,
        })
    })
    .map_err(|e| ProfilesError {
        failure: backup_failure(e),
        active_profile_now: None,
    })?;
    let Some(original) = primary.profile.and_then(ProfileId::new) else {
        return Err(ProfilesError {
            failure: ProfilesFailure::UnknownActiveProfile {
                profile: primary.profile,
            },
            active_profile_now: primary.profile,
        });
    };
    let others: Vec<ProfileId> = which.iter().filter(|p| *p != original).collect();
    let usb = session.transport().usb_info();
    if others.is_empty() {
        return Ok(Backup::from_snapshots(
            primary,
            Vec::new(),
            options.backup.tool.clone(),
            created_at,
            usb,
        ));
    }

    // 2. The gate, before the first profile switch.
    let mut writer = match session.writer_with(consent, options.write.clone()) {
        Ok(writer) => writer,
        Err(e) => {
            let active_profile_now = profile_after_gate_failure(session, &e, Some(original.get()));
            return Err(ProfilesError {
                failure: e.into(),
                active_profile_now,
            });
        }
    };

    // 3. and 4.
    let product = primary.info.product.clone();
    let snapshots = read_other_profiles(
        &mut writer,
        original,
        &others,
        product.as_deref(),
        &mut progress,
    )?;
    Ok(Backup::from_snapshots(
        primary,
        snapshots,
        options.backup.tool.clone(),
        created_at,
        usb,
    ))
}

/// Selects each profile of `others`, reads its complete snapshot, and
/// selects `original` again. On failure selects `original` (best effort)
/// unless the writer is poisoned.
pub(crate) fn read_other_profiles<T: WriteTransport>(
    writer: &mut Writer<'_, T>,
    original: ProfileId,
    others: &[ProfileId],
    product: Option<&str>,
    progress: &mut dyn FnMut(ProfilesProgress) -> ControlFlow<()>,
) -> Result<Vec<Snapshot>, ProfilesError> {
    match read_each_profile(writer, original, others, product, progress) {
        Ok(snapshots) => Ok(snapshots),
        Err(failure) => Err(ProfilesError {
            failure,
            active_profile_now: return_to(writer, original),
        }),
    }
}

fn read_each_profile<T: WriteTransport>(
    writer: &mut Writer<'_, T>,
    original: ProfileId,
    others: &[ProfileId],
    product: Option<&str>,
    progress: &mut dyn FnMut(ProfilesProgress) -> ControlFlow<()>,
) -> Result<Vec<Snapshot>, ProfilesFailure> {
    let mut snapshots = Vec::with_capacity(others.len());
    for &profile in others {
        let selecting = ProfilesProgress::Selecting {
            profile: profile.get(),
        };
        if progress(selecting).is_break() {
            return Err(ProfilesFailure::Cancelled);
        }
        if let Some(evidence) = writer.foreign_activity_seen() {
            return Err(WriteError::OtherProgramActive { evidence }.into());
        }
        writer.select_profile_observed(profile, &mut |wait| {
            let _ = progress(ProfilesProgress::WaitingForCommit {
                wait_ms: wait.wait_ms,
            });
        })?;
        let snapshot = writer
            .read_snapshot(profile, &mut |p: Progress| {
                progress(ProfilesProgress::Reading {
                    profile: Some(profile.get()),
                    done: p.done,
                    total: p.total,
                })
            })
            .map_err(|failure| match failure {
                ProfilesFailure::Backup(e) => backup_failure(e),
                other => other,
            })?;
        if snapshot.info.product.as_deref() != product {
            return Err(ProfilesFailure::ProductChanged {
                profile: profile.get(),
                product: snapshot.info.product,
            });
        }
        snapshots.push(snapshot);
    }
    if let Some(evidence) = writer.foreign_activity_seen() {
        return Err(WriteError::OtherProgramActive { evidence }.into());
    }
    // Returning is not cancellable.
    let _ = progress(ProfilesProgress::Returning {
        profile: original.get(),
    });
    writer.select_profile(original)?;
    Ok(snapshots)
}

/// Selects `original` again after a failure, unless the writer is poisoned.
/// Returns the profile that is active now, as far as known.
pub(crate) fn return_to<T: WriteTransport>(
    writer: &mut Writer<'_, T>,
    original: ProfileId,
) -> Option<u8> {
    if !writer.is_poisoned() {
        if let Ok(selected) = writer.select_profile(original) {
            return Some(selected.profile.get());
        }
    }
    writer.known_profile().map(ProfileId::get)
}

/// A cancelled read is [`ProfilesFailure::Cancelled`], not a backup error.
fn backup_failure(error: BackupError) -> ProfilesFailure {
    match error {
        BackupError::Cancelled => ProfilesFailure::Cancelled,
        other => ProfilesFailure::Backup(other),
    }
}
