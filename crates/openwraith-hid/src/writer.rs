//! Writing to the keyboard (Phase 2): consent, the "other program" gate and
//! the [`Writer`].
//!
//! # How a write is made safe
//!
//! * **Type gate.** Only a [`Session`] over a [`WriteTransport`] has
//!   [`Session::writer`]; read-only code is generic over
//!   [`Transport`](crate::Transport) and cannot reach it. Only
//!   [`WriteRequest`]s (the nine write forms confirmed on hardware) can be
//!   sent.
//! * **Back up first.** The only public way to write a table is
//!   [`Writer::write_guarded`], which needs a [`SavedBackup`] of the active
//!   profile of this keyboard, confirms the active profile with `21/11` and
//!   re-reads the bytes it would replace (nothing is sent when the keyboard
//!   already holds them). The higher-level operations
//!   ([`execute_restore`](crate::execute_restore),
//!   [`apply_trigger_edit`](crate::apply_trigger_edit),
//!   [`apply_light_edit`](crate::apply_light_edit)) take a `SavedBackup` too
//!   and additionally refuse a table that changed since it was planned.
//!   Profile selects ([`Writer::select_profile`]) change no stored setting
//!   and need no backup.
//! * **Consent.** Creating a writer needs a [`WriteConsent`], which records
//!   how the user agreed (prompt, command-line flag, application).
//! * **Other programs.** [`Session::writer`] first drains and classifies
//!   every queued report and makes sure the session observed the keyboard
//!   for at least [`WriteOptions::gate_observation`] (listening for the
//!   rest). Status reports we did not request (the vendor site's heartbeat
//!   replies), acknowledgements of writes we did not send and profile
//!   reports we did not cause refuse the writer with
//!   [`WriteError::OtherProgramActive`] (or [`WriteError::ProfileChanged`]
//!   when only the active profile changed); nothing is written. Browsers
//!   throttle timers in background tabs (to about once a minute), so a
//!   hidden vendor tab may go unnoticed: users must close the tab. A
//!   long-lived session can start the gate afresh with
//!   [`Session::rearm_gate`].
//! * **Stop-and-wait.** Every packet is sent on its own and its
//!   acknowledgement awaited (acks carry no chunk index). On an ack timeout
//!   the rest of the table is not sent; a single chunk is never sent again.
//!   A late acknowledgement of an earlier table that is still expected is
//!   waited for before the next table with the same acknowledgement starts,
//!   so it can never be taken for that table's first ack.
//! * **The main light is built from a fresh read.** Whether the
//!   music-rhythm leave (`17`) precedes the `07` write depends on the mode
//!   the keyboard has *now*, so the write is rebuilt from a fresh `0A` and
//!   `07` read right before it is sent.
//! * **Verification.** Every write is read back and compared byte for byte
//!   ([`WriteRequest::check`]). A mismatch is read once more after a short
//!   pause; if it persists, the **whole** table is sent once more (rebuilt
//!   from a fresh read where the bytes depend on the current state) and
//!   verified again, else [`WriteError::VerifyFailed`]. Each table of each
//!   profile is sent at most twice per writer ([`WriteError::RepeatedWrite`]).
//! * **Reset notifications are never answered.** After the keyboard's reset
//!   notification or a transport failure the writer is *poisoned*: every
//!   later call fails without sending anything, not even a switch back to the
//!   original profile.
//! * **Profiles.** An unexpected profile report aborts the operation. A
//!   foreign acknowledgement or status report seen while writing lets the
//!   current table finish and then refuses further writes (the caller
//!   switches back to the original profile).

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::time::Duration;

use openwraith_protocol::{
    AckOffer, AckSpec, AckTracker, ActiveProfile, DeviceEvent, DeviceInfo, FromResponse, KeyMask,
    LightLimits, LightModeList, LightRecord, MainLightWrite, Mismatch, Model, ProfileId,
    RawResponse, ReadRequest, Report, TableId, WriteRequest, WriteStep, model_for_product,
};
use serde::Serialize;

use crate::backup::{Progress, Snapshot, read_snapshot};
use crate::error::{ProfilesFailure, SessionError, WriteError};
use crate::safety::SavedBackup;
use crate::session::{Expect, Fatal, ForeignActivity, Session, Unrelated};
use crate::transport::WriteTransport;

/// How a pending late acknowledgement is waited for: in slices of this
/// length, so the wait ends soon after it arrives.
const SETTLE_SLICE: Duration = Duration::from_millis(50);

/// How the user agreed to a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentOrigin {
    /// The user answered a confirmation prompt.
    InteractivePrompt,
    /// The user passed a command-line flag (such as `--yes`) or ran a command
    /// whose only effect is the write (such as `profile select`).
    CommandLineFlag,
    /// An application (such as a GUI) obtained the user's agreement.
    Application,
}

/// The user's agreement to write to the keyboard; required to create a
/// [`Writer`] and recorded in reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct WriteConsent {
    origin: ConsentOrigin,
}

impl WriteConsent {
    /// Consent obtained in the given way.
    pub fn new(origin: ConsentOrigin) -> Self {
        WriteConsent { origin }
    }

    /// How the consent was obtained.
    pub fn origin(&self) -> ConsentOrigin {
        self.origin
    }
}

/// Timeouts and windows of a [`Writer`]. The pauses after a main-light write
/// and after each colour chunk are part of the write itself
/// ([`WriteStep::gap_after`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOptions {
    /// Wait for the acknowledgement of one packet (default 500 ms).
    pub ack_timeout: Duration,
    /// Wait for the acknowledgement and push of a profile select (500 ms).
    pub profile_push_timeout: Duration,
    /// Wait for the optional acknowledgement of the music-rhythm leave
    /// (150 ms; the vendor site does not wait for it at all).
    pub optional_ack_timeout: Duration,
    /// Window in which the status push after a music-rhythm leave is
    /// expected (100 ms after its acknowledgement).
    pub status_push_window: Duration,
    /// Pause before the second read-back of a write that did not match
    /// (100 ms).
    pub verify_reread_delay: Duration,
    /// Quiet period before a table is sent a second time (200 ms), to absorb
    /// late acknowledgements.
    pub retry_quiet: Duration,
    /// How long the session must have observed the keyboard before the first
    /// write (2 s); [`Session::writer`] listens for the rest.
    pub gate_observation: Duration,
    /// How long a late acknowledgement of one of our packets is still
    /// accepted as ours (2 s).
    pub late_ack_window: Duration,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions {
            ack_timeout: Duration::from_millis(500),
            profile_push_timeout: Duration::from_millis(500),
            optional_ack_timeout: Duration::from_millis(150),
            status_push_window: Duration::from_millis(100),
            verify_reread_delay: Duration::from_millis(100),
            retry_quiet: Duration::from_millis(200),
            gate_observation: Duration::from_secs(2),
            late_ack_window: Duration::from_secs(2),
        }
    }
}

/// Something noteworthy that happened during a write that still succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "warning", rename_all = "snake_case")]
pub enum WriteWarning {
    /// A required acknowledgement did not arrive in time; the rest of the
    /// table was not sent, and the result was checked by reading it back.
    AckTimeout {
        /// The packet (1-based).
        packet: usize,
        /// Packets in the table.
        of: usize,
        /// Which sending of the table (1 or 2).
        send: u32,
    },
    /// The optional acknowledgement of the music-rhythm leave did not arrive
    /// (the main-light write was sent anyway, as the vendor site does).
    OptionalAckMissing {
        /// The packet (1-based).
        packet: usize,
    },
    /// The first read-back differed, a second one 100 ms later was used.
    VerifyReread,
    /// The whole table was sent a second time.
    Retried,
    /// The acknowledgement of a profile select did not arrive; the switch
    /// was confirmed by reading the active profile.
    SelectAckMissing {
        /// The selected profile.
        profile: u8,
    },
    /// The profile push after a select did not arrive; the switch was
    /// confirmed by reading the active profile.
    ProfilePushMissing {
        /// The selected profile.
        profile: u8,
    },
    /// The first select did not switch the profile; it was sent once more.
    SelectRetried {
        /// The selected profile.
        profile: u8,
    },
    /// A late acknowledgement of an earlier write was still expected; the
    /// write waited for it (or for its window to end) before starting.
    WaitedForLateAck,
}

/// What a table write did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriteOutcome {
    /// The table written.
    pub table: TableId,
    /// The request kind (e.g. `"trigger"`).
    pub kind: &'static str,
    /// The active profile, as last confirmed.
    pub profile: Option<u8>,
    /// How often the whole table was sent (1, or 2 after a retry). For a
    /// profile select: the number of select packets (0 when the profile was
    /// already active, 2 after a retry).
    pub sends: u32,
    /// Packets sent in total.
    pub packets: usize,
    /// Everything noteworthy.
    pub warnings: Vec<WriteWarning>,
}

/// What [`Writer::write_guarded`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum GuardedWrite {
    /// The keyboard already held the bytes; nothing was sent.
    AlreadyMatched {
        /// The active profile (confirmed with `21/11`).
        profile: u8,
    },
    /// Written and verified by reading it back.
    Written(WriteOutcome),
}

/// What [`Writer::select_profile`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectOutcome {
    /// The profile that is now active.
    pub profile: ProfileId,
    /// Whether a select was sent (false: the profile was already active).
    pub switched: bool,
    /// Everything noteworthy.
    pub warnings: Vec<WriteWarning>,
}

/// Progress inside one table write: the packet about to be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PacketProgress {
    /// The packet (1-based).
    pub packet: usize,
    /// Packets in the table.
    pub of: usize,
    /// Which sending of the table (1 or 2).
    pub send: u32,
}

impl<T: WriteTransport> Session<T> {
    /// A [`Writer`] with the default [`WriteOptions`], after the "other
    /// program" gate (see the [module documentation](crate::writer)).
    ///
    /// Fails with [`WriteError::OtherProgramActive`] if the session saw
    /// status reports it did not request, acknowledgements of writes it did
    /// not send or profile reports it did not cause (only since the last
    /// [`rearm_gate`](Session::rearm_gate), if any), with
    /// [`WriteError::ProfileChanged`] when the only evidence is a report of
    /// another active profile than the one last read (a profile change on
    /// the keyboard or by another program), and with
    /// [`WriteError::KeyboardReset`] / [`WriteError::Disconnected`] after a
    /// reset notification or a transport failure. Nothing is written by this
    /// call.
    pub fn writer(&mut self, consent: WriteConsent) -> Result<Writer<'_, T>, WriteError> {
        self.writer_with(consent, WriteOptions::default())
    }

    /// A [`Writer`] with custom options (see [`writer`](Self::writer)).
    pub fn writer_with(
        &mut self,
        consent: WriteConsent,
        options: WriteOptions,
    ) -> Result<Writer<'_, T>, WriteError> {
        if let Some(fatal) = self.fatal() {
            return Err(fatal_error(fatal));
        }
        // Everything that queued up meanwhile (for example while the user
        // answered a prompt), then listen until the gate has observed the
        // keyboard long enough (a listen can end early, e.g. on a transport
        // that does not block).
        loop {
            self.listen(Duration::ZERO)?;
            let observed = self.observed_since_gate();
            if observed >= options.gate_observation {
                break;
            }
            self.listen(options.gate_observation - observed)?;
        }
        let activity = self.activity_since_gate();
        if !activity.is_quiet() {
            if activity.status_reports == 0 && activity.foreign_acks == 0 {
                // Only profile reports: another active profile than the one
                // we last read is a profile change (on the keyboard or by
                // another program); the same profile is another program's
                // query reply.
                if let (Some(reported), Some(known)) = (
                    self.last_foreign_profile_report(),
                    self.last_known_profile(),
                ) {
                    if reported != known {
                        return Err(WriteError::ProfileChanged {
                            expected: Some(known),
                            reported,
                        });
                    }
                }
            }
            return Err(WriteError::OtherProgramActive { evidence: activity });
        }
        Ok(Writer {
            session: self,
            consent,
            options,
            known_profile: None,
            identity: None,
            sends: HashMap::new(),
            stop: None,
        })
    }
}

/// Which table a send counts against ([`WriteError::RepeatedWrite`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SendKey {
    profile: u8,
    table: TableId,
    keys: Option<KeyMask>,
}

/// The keyboard this writer talks to, read once (`0D`).
#[derive(Debug, Clone)]
struct Identity {
    product: Option<String>,
    model: &'static Model,
}

/// Writes to the keyboard; created by [`Session::writer`]. See the
/// [module documentation](crate::writer).
///
/// Tables are written with [`write_guarded`](Writer::write_guarded), which
/// needs a safety backup; the unguarded primitive is not public:
///
/// ```compile_fail
/// use openwraith_hid::protocol::WriteRequest;
/// use openwraith_hid::{WriteTransport, Writer};
/// fn unguarded<T: WriteTransport>(writer: &mut Writer<'_, T>, request: &WriteRequest) {
///     let _ = writer.write_observed(request, &mut |_| {});
/// }
/// ```
pub struct Writer<'s, T: WriteTransport> {
    pub(crate) session: &'s mut Session<T>,
    consent: WriteConsent,
    options: WriteOptions,
    known_profile: Option<ProfileId>,
    identity: Option<Identity>,
    sends: HashMap<SendKey, u32>,
    stop: Option<ForeignActivity>,
}

impl<T: WriteTransport> Writer<'_, T> {
    /// The consent this writer was created with.
    pub fn consent(&self) -> WriteConsent {
        self.consent
    }

    /// The writer's options.
    pub fn options(&self) -> &WriteOptions {
        &self.options
    }

    /// The session (for statistics and events).
    pub fn session(&self) -> &Session<T> {
        self.session
    }

    /// Whether the writer may not send anything any more (keyboard reset or
    /// lost connection).
    pub fn is_poisoned(&self) -> bool {
        self.session.fatal().is_some()
    }

    /// The active profile as last read or selected by this writer.
    pub fn known_profile(&self) -> Option<ProfileId> {
        self.known_profile
    }

    /// Foreign activity seen while writing, if any: once set, further table
    /// writes are refused with [`WriteError::OtherProgramActive`] (profile
    /// selects are still allowed, to return to the original profile).
    pub fn foreign_activity_seen(&self) -> Option<ForeignActivity> {
        self.stop
    }

    /// Reads the active profile (`21/11`).
    pub fn active_profile(&mut self) -> Result<ProfileId, WriteError> {
        let raw = self.read(ReadRequest::ActiveProfile)?;
        let decoded = ActiveProfile::from_response(&raw).map_err(|source| {
            WriteError::Session(SessionError::Decode {
                request: ReadRequest::ActiveProfile,
                source,
            })
        })?;
        let profile = ProfileId::new(decoded.profile).ok_or(WriteError::InvalidProfile {
            profile: decoded.profile,
        })?;
        self.known_profile = Some(profile);
        Ok(profile)
    }

    /// Sends a read request (through the session) and returns its reply.
    /// Fails without sending when the writer is poisoned, and with
    /// [`WriteError::ProfileChanged`] when an unexpected profile report
    /// arrived meanwhile.
    pub fn read(&mut self, request: ReadRequest) -> Result<RawResponse, WriteError> {
        self.ensure_usable()?;
        let events = self.session.events().len();
        let before = self.session.foreign_activity();
        let result = self.session.request(request);
        let activity = self.session.foreign_activity().since(&before);
        self.note_foreign(activity);
        let raw = result?;
        self.check_new_events(events)?;
        Ok(raw)
    }

    /// Selects onboard profile `profile` (`21/12`).
    ///
    /// Reads the active profile first and sends nothing if it is already
    /// active. Otherwise sends the select, waits for its acknowledgement and
    /// profile push, and confirms the switch with `21/11` (a missing push is
    /// only a warning when `21/11` confirms). Retries once, then fails with
    /// [`WriteError::ProfileNotSelected`].
    pub fn select_profile(&mut self, profile: ProfileId) -> Result<SelectOutcome, WriteError> {
        self.ensure_usable()?;
        self.listen_checked(Duration::ZERO)?;
        let mut active = self.active_profile()?;
        let mut warnings = Vec::new();
        if active == profile {
            return Ok(SelectOutcome {
                profile,
                switched: false,
                warnings,
            });
        }
        let request = WriteRequest::SelectProfile(profile);
        for attempt in 1..=2u32 {
            if attempt > 1 {
                warnings.push(WriteWarning::SelectRetried {
                    profile: profile.get(),
                });
            }
            for step in request.steps() {
                self.ensure_usable()?;
                self.session.send_write(&step.packet)?;
                // Read the push before any 21/11 query: the query's collector
                // would take a late push as its reply.
                let tracker = self.wait_for(step.ack, self.options.profile_push_timeout)?;
                if !tracker.ack_received() {
                    warnings.push(WriteWarning::SelectAckMissing {
                        profile: profile.get(),
                    });
                    self.session
                        .expect(Expect::Ack(step.ack), self.options.late_ack_window);
                }
                if !tracker.push_received() {
                    warnings.push(WriteWarning::ProfilePushMissing {
                        profile: profile.get(),
                    });
                    self.session
                        .expect(Expect::ProfilePush(profile), self.options.late_ack_window);
                }
            }
            active = self.active_profile()?;
            if active == profile {
                return Ok(SelectOutcome {
                    profile,
                    switched: true,
                    warnings,
                });
            }
        }
        Err(WriteError::ProfileNotSelected {
            requested: profile,
            active: active.get(),
        })
    }

    /// Writes one table to the active profile, guarded like every write of
    /// this crate. This is the only public way to send a table write.
    ///
    /// 1. Refuses when another program was seen while writing.
    /// 2. Checks that `saved` (a verified safety backup, see
    ///    [`Backup::save_safety`](crate::Backup::save_safety)) belongs to this
    ///    keyboard model (`0D`, read once per writer) and holds the active
    ///    profile, confirmed with `21/11`; else
    ///    [`WriteError::NotBackedUp`] and nothing is sent.
    /// 3. Re-reads exactly the bytes `request` writes: when the keyboard
    ///    already holds them, nothing is sent
    ///    ([`GuardedWrite::AlreadyMatched`]).
    /// 4. Writes stop-and-wait and verifies by reading back, with one more
    ///    attempt with the whole table (see the
    ///    [module documentation](crate::writer)). A main-light write is
    ///    rebuilt from a fresh read first, so the music-rhythm leave is sent
    ///    exactly when the keyboard's current mode changes.
    ///
    /// A profile select needs no backup: use
    /// [`select_profile`](Self::select_profile) (a
    /// [`WriteRequest::SelectProfile`] passed here is handled the same way).
    pub fn write_guarded(
        &mut self,
        saved: &SavedBackup,
        request: &WriteRequest,
    ) -> Result<GuardedWrite, WriteError> {
        self.write_guarded_observed(saved, request, &mut |_| {})
    }

    /// [`write_guarded`](Self::write_guarded), calling `on_packet` before
    /// every packet.
    pub fn write_guarded_observed(
        &mut self,
        saved: &SavedBackup,
        request: &WriteRequest,
        on_packet: &mut dyn FnMut(PacketProgress),
    ) -> Result<GuardedWrite, WriteError> {
        self.ensure_usable()?;
        if let WriteRequest::SelectProfile(_) = request {
            let outcome = self.write_observed(request, on_packet)?;
            return Ok(if outcome.packets == 0 {
                GuardedWrite::AlreadyMatched {
                    profile: outcome.profile.unwrap_or(0),
                }
            } else {
                GuardedWrite::Written(outcome)
            });
        }
        if let Some(evidence) = self.stop {
            return Err(WriteError::OtherProgramActive { evidence });
        }
        let product = self.identity()?.product;
        let profile = self.active_profile()?;
        if !saved.covers(profile) || saved.product() != product.as_deref() {
            return Err(WriteError::NotBackedUp {
                path: saved.path().to_path_buf(),
                profile: profile.get(),
            });
        }
        let mut fresh = Vec::new();
        for read in request.verify_reads() {
            fresh.push(self.read(read)?);
        }
        if request.check(&fresh).is_ok() {
            return Ok(GuardedWrite::AlreadyMatched {
                profile: profile.get(),
            });
        }
        self.write_observed(request, on_packet)
            .map(GuardedWrite::Written)
    }

    /// Writes `request` to the active profile: stop-and-wait per packet,
    /// read-back verification, one more attempt with the whole table. See
    /// the [module documentation](crate::writer).
    ///
    /// Crate-private: it neither checks for a safety backup nor confirms the
    /// active profile. Callers are [`write_guarded`](Self::write_guarded) and
    /// the restore and edit operations, which do both.
    pub(crate) fn write_observed(
        &mut self,
        request: &WriteRequest,
        on_packet: &mut dyn FnMut(PacketProgress),
    ) -> Result<WriteOutcome, WriteError> {
        if let WriteRequest::SelectProfile(profile) = request {
            let writes_before = self.session.stats().writes;
            let selected = self.select_profile(*profile)?;
            let packets = self.session.stats().writes.saturating_sub(writes_before);
            return Ok(WriteOutcome {
                table: TableId::Profile,
                kind: request.kind(),
                profile: Some(selected.profile.get()),
                sends: u32::try_from(packets).unwrap_or(u32::MAX),
                packets: usize::try_from(packets).unwrap_or(usize::MAX),
                warnings: selected.warnings,
            });
        }
        self.ensure_usable()?;
        self.listen_checked(Duration::ZERO)?;
        if let Some(evidence) = self.stop {
            return Err(WriteError::OtherProgramActive { evidence });
        }
        // The main light is always built from the keyboard's current mode.
        let first_request = self.rebuild(request)?;
        let request = &first_request;
        let key = self.send_key(request);
        let mut outcome = WriteOutcome {
            table: request.table(),
            kind: request.kind(),
            profile: self.known_profile.map(ProfileId::get),
            sends: 0,
            packets: 0,
            warnings: Vec::new(),
        };

        self.count_send(key)?;
        outcome.sends = 1;
        self.send_steps(request, 1, &mut outcome, on_packet)?;
        let first = match self.verify(request, &mut outcome)? {
            Ok(()) => return Ok(outcome),
            Err(mismatch) => mismatch,
        };

        // One more attempt with the whole table.
        if let Some(evidence) = self.stop {
            return Err(WriteError::OtherProgramActive { evidence });
        }
        if self.sends.get(&key).copied().unwrap_or(0) >= 2 {
            return Err(WriteError::VerifyFailed {
                table: request.table(),
                sends: 1,
                mismatch: Box::new(first),
            });
        }
        self.listen_checked(self.options.retry_quiet)?;
        let retry = self.rebuild(request)?;
        self.count_send(key)?;
        outcome.sends = 2;
        outcome.warnings.push(WriteWarning::Retried);
        self.send_steps(&retry, 2, &mut outcome, on_packet)?;
        match self.verify(&retry, &mut outcome)? {
            Ok(()) => Ok(outcome),
            Err(mismatch) => Err(WriteError::VerifyFailed {
                table: request.table(),
                sends: 2,
                mismatch: Box::new(mismatch),
            }),
        }
    }

    /// Reads one complete profile snapshot (the [`read_all`](crate::read_all)
    /// plan) of the active profile, which must be `expected` at the start
    /// and at the end.
    pub(crate) fn read_snapshot(
        &mut self,
        expected: ProfileId,
        progress: &mut dyn FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<Snapshot, ProfilesFailure> {
        self.ensure_usable()?;
        let before = self.session.foreign_activity();
        let result = read_snapshot(self.session, Some(expected.get()), progress);
        let activity = self.session.foreign_activity().since(&before);
        // Profile reports are reported by read_snapshot itself.
        self.note_foreign(ForeignActivity {
            profile_reports: 0,
            ..activity
        });
        Ok(result?)
    }

    /// The keyboard's product code and model (`0D`, read once per writer).
    fn identity(&mut self) -> Result<Identity, WriteError> {
        if let Some(identity) = &self.identity {
            return Ok(identity.clone());
        }
        let raw = self.read(ReadRequest::DeviceInfo)?;
        let info = DeviceInfo::from_response(&raw).map_err(|source| {
            WriteError::Session(SessionError::Decode {
                request: ReadRequest::DeviceInfo,
                source,
            })
        })?;
        let model = info
            .product
            .as_deref()
            .and_then(model_for_product)
            .ok_or_else(|| WriteError::UnsupportedModel {
                product: info.product.clone(),
            })?;
        let identity = Identity {
            product: info.product,
            model,
        };
        self.identity = Some(identity.clone());
        Ok(identity)
    }

    /// Fails without sending anything when the session saw a reset or lost
    /// its connection.
    fn ensure_usable(&self) -> Result<(), WriteError> {
        match self.session.fatal() {
            Some(fatal) => Err(fatal_error(fatal)),
            None => Ok(()),
        }
    }

    /// Records foreign activity seen during an operation.
    fn note_foreign(&mut self, activity: ForeignActivity) {
        if activity.status_reports == 0 && activity.foreign_acks == 0 {
            return;
        }
        let total = self.stop.unwrap_or_default();
        self.stop = Some(ForeignActivity {
            status_reports: total.status_reports + activity.status_reports,
            foreign_acks: total.foreign_acks + activity.foreign_acks,
            profile_reports: total.profile_reports,
        });
    }

    /// Fails if a profile report arrived since `events_before` events.
    fn check_new_events(&mut self, events_before: usize) -> Result<(), WriteError> {
        let reported = self
            .session
            .events()
            .get(events_before..)
            .unwrap_or_default()
            .iter()
            .find_map(|event| match event {
                DeviceEvent::ProfileReport { profile, .. } => Some(*profile),
                _ => None,
            });
        match reported {
            Some(profile) => Err(self.unexpected_profile_report(profile)),
            None => Ok(()),
        }
    }

    /// The error for a profile report we did not cause.
    fn unexpected_profile_report(&self, reported: u8) -> WriteError {
        let expected = self.known_profile.map(ProfileId::get);
        if expected == Some(reported) {
            // The same profile: another program's query reply.
            WriteError::OtherProgramActive {
                evidence: ForeignActivity {
                    profile_reports: 1,
                    ..ForeignActivity::default()
                },
            }
        } else {
            WriteError::ProfileChanged { expected, reported }
        }
    }

    /// Listens for `duration` (draining queued reports even when it is zero)
    /// and classifies what arrives.
    fn listen_checked(&mut self, duration: Duration) -> Result<(), WriteError> {
        let events = self.session.events().len();
        let summary = self.session.listen(duration)?;
        self.note_foreign(summary.activity);
        self.check_new_events(events)
    }

    /// Handles a report that is not the acknowledgement being waited for.
    fn handle_unrelated(&mut self, report: &Report) -> Result<(), WriteError> {
        match self.session.classify_unrelated(report) {
            Unrelated::Reset => Err(WriteError::KeyboardReset {
                report: report.summary(),
            }),
            Unrelated::ProfileReport { profile } => Err(self.unexpected_profile_report(profile)),
            Unrelated::ForeignStatus => {
                self.note_foreign(ForeignActivity {
                    status_reports: 1,
                    ..ForeignActivity::default()
                });
                Ok(())
            }
            Unrelated::ForeignAck => {
                self.note_foreign(ForeignActivity {
                    foreign_acks: 1,
                    ..ForeignActivity::default()
                });
                Ok(())
            }
            Unrelated::Expected | Unrelated::Other => Ok(()),
        }
    }

    /// Waits up to `timeout` for the acknowledgement described by `spec`.
    /// Unrelated reports do not extend the deadline.
    fn wait_for(&mut self, spec: AckSpec, timeout: Duration) -> Result<AckTracker, WriteError> {
        let mut tracker = spec.tracker();
        let deadline = self.session.now() + timeout;
        while !tracker.is_complete() {
            let remaining = deadline.saturating_duration_since(self.session.now());
            if remaining.is_zero() {
                break;
            }
            let Some(report) = self.session.read_input(remaining)? else {
                break;
            };
            match tracker.offer(&report) {
                AckOffer::Ack => self.session.count_ack(),
                AckOffer::Push => self.session.count_expected(),
                AckOffer::Ignored => self.handle_unrelated(&report)?,
            }
        }
        Ok(tracker)
    }

    /// Before a table starts: while a late acknowledgement of an earlier
    /// write that looks exactly like this table's acknowledgements is still
    /// expected, listen until it arrives (and is consumed) or its window
    /// ends. Otherwise it would be taken for the first packet's ack and
    /// every later packet would be sent one ack early.
    fn settle_late_acks(
        &mut self,
        steps: &[WriteStep],
        outcome: &mut WriteOutcome,
    ) -> Result<(), WriteError> {
        let mut specs: Vec<AckSpec> = Vec::new();
        for step in steps {
            if step.ack.expects_reply() && !specs.contains(&step.ack) {
                specs.push(step.ack);
            }
        }
        let mut waited = false;
        for spec in specs {
            while let Some(until) = self.session.pending_expect(&Expect::Ack(spec)) {
                let remaining = until.saturating_duration_since(self.session.now());
                if remaining.is_zero() {
                    break;
                }
                waited = true;
                self.listen_checked(remaining.min(SETTLE_SLICE))?;
            }
        }
        if waited {
            outcome.warnings.push(WriteWarning::WaitedForLateAck);
        }
        Ok(())
    }

    /// Sends the packets of `request` one at a time, waiting for each
    /// acknowledgement. Stops (without an error) after an acknowledgement
    /// timeout: the rest of the table is not sent and the caller verifies.
    fn send_steps(
        &mut self,
        request: &WriteRequest,
        send: u32,
        outcome: &mut WriteOutcome,
        on_packet: &mut dyn FnMut(PacketProgress),
    ) -> Result<(), WriteError> {
        let steps = request.steps();
        self.settle_late_acks(&steps, outcome)?;
        let of = steps.len();
        for (i, step) in steps.iter().enumerate() {
            self.ensure_usable()?;
            let packet = i + 1;
            on_packet(PacketProgress { packet, of, send });
            if step.packet.cmd() == 0x17 {
                // The keyboard pushes one status report after the leave.
                self.session.expect(
                    Expect::StatusPush,
                    self.options.optional_ack_timeout + self.options.status_push_window,
                );
            }
            self.session.send_write(&step.packet)?;
            outcome.packets += 1;
            if step.ack.expects_reply() {
                let required = step.ack.is_required();
                let timeout = if required {
                    self.options.ack_timeout
                } else {
                    self.options.optional_ack_timeout
                };
                let tracker = self.wait_for(step.ack, timeout)?;
                if !tracker.is_complete() {
                    // Absorb the late ack instead of taking it for another
                    // program's.
                    self.session
                        .expect(Expect::Ack(step.ack), self.options.late_ack_window);
                    if required {
                        outcome
                            .warnings
                            .push(WriteWarning::AckTimeout { packet, of, send });
                        return Ok(());
                    }
                    outcome
                        .warnings
                        .push(WriteWarning::OptionalAckMissing { packet });
                }
            }
            // The pause the write form asks for (the protocol crate is the
            // only source: LIGHT_SETTLE, COLOR_CHUNK_GAP).
            if !step.gap_after.is_zero() {
                self.listen_checked(step.gap_after)?;
            }
        }
        Ok(())
    }

    /// Reads `request`'s verify reads and compares them; on a mismatch reads
    /// once more after a short pause. The outer error is a failed read, the
    /// inner one the remaining difference.
    fn verify(
        &mut self,
        request: &WriteRequest,
        outcome: &mut WriteOutcome,
    ) -> Result<Result<(), Mismatch>, WriteError> {
        let replies = self.read_each(&request.verify_reads())?;
        if request.check(&replies).is_ok() {
            return Ok(Ok(()));
        }
        outcome.warnings.push(WriteWarning::VerifyReread);
        self.listen_checked(self.options.verify_reread_delay)?;
        let replies = self.read_each(&request.verify_reads())?;
        Ok(request.check(&replies))
    }

    /// Reads every request in order.
    fn read_each(&mut self, requests: &[ReadRequest]) -> Result<Vec<RawResponse>, WriteError> {
        requests.iter().map(|r| self.read(*r)).collect()
    }

    /// The request as it must be sent now: the main light is rebuilt from a
    /// fresh read (whether the music-rhythm leave is needed depends on the
    /// mode the keyboard has now, and the limits from its `0A` list and the
    /// model's ceilings); everything else is absolute and sent unchanged.
    fn rebuild(&mut self, request: &WriteRequest) -> Result<WriteRequest, WriteError> {
        let WriteRequest::MainLight(write) = request else {
            return Ok(request.clone());
        };
        let model = self.identity()?.model;
        let modes_raw = self.read(ReadRequest::MainLightModes)?;
        let modes = LightModeList::from_response(&modes_raw).map_err(|source| {
            WriteError::Session(SessionError::Decode {
                request: ReadRequest::MainLightModes,
                source,
            })
        })?;
        let current = LightRecord::from_response(&self.read(ReadRequest::MainLight)?)?;
        let rebuilt =
            MainLightWrite::new(&current, *write.record(), &LightLimits::new(model, &modes))?;
        Ok(WriteRequest::MainLight(rebuilt))
    }

    fn send_key(&self, request: &WriteRequest) -> SendKey {
        SendKey {
            profile: self.known_profile.map_or(0, ProfileId::get),
            table: request.table(),
            keys: match request {
                WriteRequest::Trigger(write) => Some(write.keys()),
                _ => None,
            },
        }
    }

    /// Counts one sending of a table; refuses a third.
    fn count_send(&mut self, key: SendKey) -> Result<(), WriteError> {
        let count = self.sends.entry(key).or_insert(0);
        if *count >= 2 {
            return Err(WriteError::RepeatedWrite {
                table: key.table,
                profile: (key.profile != 0).then_some(key.profile),
            });
        }
        *count += 1;
        Ok(())
    }
}

/// The error for a poisoned session.
fn fatal_error(fatal: &Fatal) -> WriteError {
    match fatal {
        Fatal::Reset(report) => WriteError::KeyboardReset {
            report: report.clone(),
        },
        Fatal::Disconnected(message) => WriteError::Disconnected {
            message: message.clone(),
        },
    }
}
