//! Writing to the keyboard (Phase 2): consent, the "other program" gate and
//! the [`Writer`].
//!
//! # How a write is made safe
//!
//! * **Type gate.** Only a [`Session`] over a [`WriteTransport`] has
//!   [`Session::writer`]; read-only code is generic over
//!   [`Transport`](crate::Transport) and cannot reach it. Only
//!   [`WriteRequest`]s (the write forms W1-W11 confirmed by captures and on
//!   hardware) and the travel-test open and close
//!   ([`travel`](crate::travel)) can be sent, and every packet passes the
//!   egress allowlist right before it is sent.
//! * **Key-depth tests.** No table is written while the keyboard may stream
//!   key depths: while a close was not acknowledged
//!   ([`WriteError::TravelTestMaybeOpen`]) or while stream reports arrive
//!   that no test of this session opened ([`WriteError::TravelTestOpen`]).
//!   Profile selects stay allowed.
//! * **The keyboard's own switches.** SOCD switched with Fn + Right Shift
//!   during a SOCD write is never overridden: a read-back that then differs
//!   fails with [`WriteError::ChangedOnKeyboard`] without a second
//!   attempt.
//! * **Back up first.** The only public way to write a table is
//!   [`Writer::write_guarded`], which needs a [`SavedBackup`] of the active
//!   profile of this keyboard, confirms the active profile with `21/11` and
//!   re-reads the bytes it would replace (nothing is sent when the keyboard
//!   already holds them), and its variant for the Fn keymap,
//!   [`Writer::write_commit_trigger`], which has the same guards but sends a
//!   matching table too (see below). The higher-level operations
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
//! * **Commit settle.** The keyboard keeps edits in a working copy of the
//!   active profile and saves them about 1 s after the last write; a profile
//!   switch (or a power loss) before that loses them, and a read-back cannot
//!   tell, because it reads the working copy (CONFIRMED on hardware, spec
//!   4.5.3 "Delayed commit"). The session records when the last write that
//!   changes a setting was sent ([`Session::last_write_at`]), and
//!   [`Writer::select_profile`] does not send a select before
//!   [`WriteOptions::commit_settle`] (2 s) has passed since then: it listens
//!   for the rest (still classifying every report) and reports the wait
//!   ([`CommitWait`], [`WriteWarning::WaitedForCommit`]). Every profile
//!   switch of this crate goes through it. Operations that write wait for
//!   the settle before they return ([`Writer::wait_for_commit`]), so a
//!   finished operation means the keyboard had time to save. At the end of
//!   a wait the active profile is read again (a switch made on the keyboard
//!   need not be reported), and a profile change seen before the settle had
//!   passed fails with [`WriteError::ProfileChangedBeforeSave`]: the write
//!   may not have been saved ([`Writer::commit_interrupted`]). The settle
//!   cannot be turned off outside tests ([`CommitSettle`]).
//! * **The base keymap needs the Fn keymap after it.** The keyboard saves a
//!   base-keymap write (`18 00`) only when an Fn-keymap write (`18 02`)
//!   follows it; on its own it is gone after a profile switch, even after
//!   the commit settle (CONFIRMED on hardware, spec 4.8.2). Since
//!   [`Writer::write_guarded`] sends nothing when the keyboard already holds
//!   a table, the Fn keymap is re-sent as a [`CommitTrigger`] with
//!   [`Writer::write_commit_trigger`], which only accepts an Fn-keymap write
//!   and keeps every other guard. A read-back cannot show whether such a
//!   re-send arrived (the table reads the same either way), so a commit
//!   trigger also needs an acknowledgement for **every** packet: an ack
//!   timeout counts as a failed verification (one more attempt with the
//!   whole table, then [`WriteError::CommitTriggerIncomplete`]). The writer
//!   remembers a base keymap sent without a complete Fn-keymap write after
//!   it ([`Writer::base_keymap_unsaved`]); a profile switch made then
//!   reports [`WriteWarning::BaseKeymapNotSaved`].

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::time::Duration;

use openwraith_protocol::{
    AckOffer, AckSpec, AckTracker, ActiveProfile, DeviceEvent, DeviceInfo, FromResponse, KeyMask,
    KeymapData, Layer, LightLimits, LightModeList, LightRecord, MainLightWrite, Mismatch, Model,
    ProfileId, RawResponse, ReadRequest, Report, TableId, WriteRequest, WriteStep,
    model_for_product,
};
use serde::Serialize;

use crate::backup::{Progress, Snapshot, read_snapshot};
use crate::error::{ProfilesFailure, SessionError, WriteError};
use crate::safety::SavedBackup;
use crate::session::{Expect, Fatal, ForeignActivity, Session, TravelState, Unrelated};
use crate::transport::WriteTransport;

/// How a pending late acknowledgement is waited for: in slices of this
/// length, so the wait ends soon after it arrives.
const SETTLE_SLICE: Duration = Duration::from_millis(50);

/// At most this many queued reports are drained after a listening window.
const SETTLE_DRAIN_MAX: usize = 512;

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
    /// A rule the user set up beforehand (an automatic profile switch for a
    /// game): only profile selects are made with it
    /// ([`ProfileSwitcher`](crate::ProfileSwitcher)). The library enforces
    /// it: a [`Writer`] with this consent selects profiles and reads, and
    /// refuses every other write ([`WriteError::ConsentOnlyForSelects`]), as
    /// do the restore, the edits and the travel test. The other origins are
    /// not restricted; which code may use which consent is checked by the
    /// application's own source rules.
    StandingRule,
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

    /// Fails with [`WriteError::ConsentOnlyForSelects`] when this consent
    /// may only select profiles ([`ConsentOrigin::StandingRule`]) and
    /// something else is about to be written.
    pub(crate) fn ensure_not_standing_rule(self) -> Result<(), WriteError> {
        match self.origin {
            ConsentOrigin::StandingRule => Err(WriteError::ConsentOnlyForSelects),
            _ => Ok(()),
        }
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
    /// How long the keyboard is left alone after the last write that
    /// changed a setting before a profile select is sent or an operation
    /// returns (default 2 s; see [`CommitSettle`]).
    pub commit_settle: CommitSettle,
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
            commit_settle: CommitSettle::default(),
        }
    }
}

/// How long a [`Writer`] leaves the keyboard alone after the last write that
/// changed a setting, before it selects another profile or reports an
/// operation as done.
///
/// The keyboard keeps edits in a working copy of the active profile and
/// saves them about 1 s after the last write. A profile switch before that
/// reloads the profile, and the edits are lost; a read-back cannot tell,
/// because it reads the working copy. Measured on hardware (spec 4.5.3,
/// "Delayed commit"): a trigger write was lost when the next select followed
/// it within 0, 150, 400 or 800 ms, and kept from 1000 ms on. A power loss in
/// that window loses it too (desktop-app hardware test A10: an unplug during
/// the wait lost the write).
///
/// The default is 2 s, twice the measured threshold. A caller may choose a
/// longer settle, or a shorter one down to [`CommitSettle::MINIMUM`]; there
/// is no way to turn it off, except for tests against the simulator
/// (`CommitSettle::disabled_for_tests`, feature `mock`).
///
/// ```
/// use std::time::Duration;
/// use openwraith_hid::CommitSettle;
///
/// assert_eq!(CommitSettle::default().duration(), Duration::from_secs(2));
/// // Never shorter than the minimum.
/// assert_eq!(CommitSettle::at_least(Duration::ZERO).duration(), CommitSettle::MINIMUM);
/// assert_eq!(CommitSettle::at_least(Duration::from_secs(5)).duration(), Duration::from_secs(5));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommitSettle(Duration);

impl CommitSettle {
    /// The default settle: 2 s, twice the commit delay measured on the
    /// keyboard (writes were lost up to 800 ms and kept from 1000 ms on).
    pub const DEFAULT: CommitSettle = CommitSettle(Duration::from_secs(2));

    /// The shortest settle [`at_least`](Self::at_least) gives: 1.5 s, the
    /// measured commit delay (about 1 s) with a margin.
    pub const MINIMUM: Duration = Duration::from_millis(1500);

    /// A settle of `duration`, but never shorter than
    /// [`MINIMUM`](Self::MINIMUM).
    pub fn at_least(duration: Duration) -> Self {
        CommitSettle(duration.max(Self::MINIMUM))
    }

    /// No settle at all: profile selects follow writes at once, exactly the
    /// behaviour that lost writes on the keyboard. Only for tests that
    /// reproduce that loss on the simulator; it exists only with the `mock`
    /// feature, so no build that talks to a keyboard can use it.
    #[cfg(any(test, feature = "mock"))]
    pub fn disabled_for_tests() -> Self {
        CommitSettle(Duration::ZERO)
    }

    /// The settle duration.
    pub fn duration(self) -> Duration {
        self.0
    }
}

impl Default for CommitSettle {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A wait for the keyboard to save the last write (see [`CommitSettle`]),
/// reported before it starts so that a user interface can show "waiting for
/// the keyboard to save".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CommitWait {
    /// The profile the last write went to (its edits are being saved), as
    /// the writer had confirmed it.
    pub profile: Option<u8>,
    /// How long the wait lasts, in milliseconds.
    pub wait_ms: u64,
    /// The writer's commit settle, in milliseconds.
    pub settle_ms: u64,
}

/// Whole milliseconds of `duration` (saturating).
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
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
    /// A profile select waited for the keyboard to save the last write
    /// first ([`CommitSettle`]).
    WaitedForCommit {
        /// How long it waited, in milliseconds.
        waited_ms: u64,
    },
    /// A base keymap was written, but the operation stopped before the
    /// Fn-keymap write that makes the keyboard save it ([`CommitTrigger`]):
    /// the keyboard will drop it at the next profile switch or power loss
    /// (spec 4.8.2). In a [`SelectOutcome`]: the switch was made while such
    /// a base keymap was unsaved ([`Writer::base_keymap_unsaved`]), so the
    /// keyboard dropped it.
    BaseKeymapNotSaved,
    /// The keyboard acknowledged a packet for which no acknowledgement was
    /// expected (the SOCD power write `24 03`, spec 2.7); it was taken as
    /// ours.
    UnexpectedAck {
        /// The packet (1-based).
        packet: usize,
    },
    /// SOCD was switched on the keyboard (Fn + Right Shift) while it was
    /// written (the read-back still matched what was written, so the
    /// keyboard's switch may have been overwritten), or after it and before
    /// the persistence check read it again (which then does not count it,
    /// [`Persistence::NotChecked`](crate::Persistence::NotChecked)).
    SocdToggledOnKeyboard {
        /// Whether SOCD is on now, as last read.
        power: bool,
    },
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

/// An Fn-keymap write (`18 02`) sent as the keyboard's **keymap commit
/// trigger**: [`Writer::write_commit_trigger`] sends it even when the
/// keyboard already holds its bytes.
///
/// The keyboard saves a base-keymap write (`18 00`) only when an Fn-keymap
/// write follows it: written alone, the base keymap is acked and reads back
/// at once, but is gone after a profile switch, even 6 s later (CONFIRMED on
/// hardware, spec 4.8.2). The vendor site never notices, because every Apply
/// writes the macros, `18 00`, `18 02` and the advanced keys in that order.
/// So after the base keymap, the Fn keymap must be written too, unchanged
/// if need be; [`Writer::write_guarded`] would skip an unchanged table.
///
/// Only an Fn-keymap write can be a commit trigger: [`CommitTrigger::fn_keymap`]
/// builds one, and `TryFrom<WriteRequest>` refuses every other request
/// (giving it back). It is no way to send anything else, and the write keeps
/// every guard of [`Writer::write_guarded`] except the skip of a matching
/// table. It is stricter in one respect: every packet must be acknowledged
/// (see [`Writer::write_commit_trigger`]).
///
/// ```
/// use openwraith_hid::CommitTrigger;
/// use openwraith_hid::protocol::{ProfileId, WriteRequest};
///
/// let select = WriteRequest::SelectProfile(ProfileId::ALL[1]);
/// assert_eq!(CommitTrigger::try_from(select.clone()), Err(select));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitTrigger(WriteRequest);

impl CommitTrigger {
    /// The Fn-keymap write of `data` as a commit trigger.
    pub fn fn_keymap(data: KeymapData) -> Self {
        CommitTrigger(WriteRequest::Keymap(Layer::Fn, data))
    }

    /// The Fn-keymap write (always a [`WriteRequest::Keymap`] of
    /// [`Layer::Fn`]).
    pub fn request(&self) -> &WriteRequest {
        &self.0
    }
}

impl TryFrom<WriteRequest> for CommitTrigger {
    /// Any request other than an Fn-keymap write, given back unchanged.
    type Error = WriteRequest;

    fn try_from(request: WriteRequest) -> Result<Self, Self::Error> {
        match request {
            WriteRequest::Keymap(Layer::Fn, data) => Ok(CommitTrigger::fn_keymap(data)),
            other => Err(other),
        }
    }
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
                // Only profile reports. A report of another profile than
                // the one we last read is a profile change (Fn + 1..4 on the
                // keyboard): a round trip 1 -> 2 -> 1 included, which ends
                // on the known profile but reloaded it meanwhile. Reports in
                // the push form (byte 7 = 00) only are a switch on the
                // keyboard too. Same-profile query replies (byte 7 = 01) are
                // another program's. Another program's own switch is caught
                // by its acknowledgement (a foreign ack).
                let reports = self.foreign_profile_reports();
                let known = self.last_known_profile();
                let other = known.is_some_and(|k| reports.iter().any(|(p, _)| *p != k));
                let pushes_only = reports.iter().all(|(_, extra)| *extra == 0x00);
                if let Some(&(reported, _)) = reports.last() {
                    if other || pushes_only {
                        return Err(WriteError::ProfileChanged {
                            expected: known,
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

/// How far one sending of a table got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sent {
    /// Every packet was sent and every required acknowledgement arrived.
    Complete,
    /// The required acknowledgement of `packet` (1-based) of `of` did not
    /// arrive in time; the rest of the table was not sent.
    Stopped {
        /// The packet whose acknowledgement timed out.
        packet: usize,
        /// Packets in the table.
        of: usize,
    },
}

/// What a write must achieve besides a matching read-back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Completion {
    /// The read-back matches (an ack timeout stops the send, and the
    /// read-back decides).
    Verified,
    /// The read-back matches **and** every packet was sent and acknowledged:
    /// a [`CommitTrigger`], whose read-back cannot show whether it arrived.
    EveryPacketAcked,
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

    /// Whether this writer sent a base keymap (`18 00`) and no complete
    /// Fn-keymap write (`18 02`, every packet acknowledged) followed it yet.
    /// The keyboard saves the base keymap only then: until a
    /// [`write_commit_trigger`](Self::write_commit_trigger) (or a changed Fn
    /// keymap written with [`write_guarded`](Self::write_guarded)), a
    /// profile switch or a power loss drops it, even after the commit settle
    /// (spec 4.8.2). [`select_profile`](Self::select_profile) reports such a
    /// switch with [`WriteWarning::BaseKeymapNotSaved`] and clears this.
    /// Kept by the session ([`Session::base_keymap_unsaved`]), so a later
    /// writer of the same session sees it too.
    pub fn base_keymap_unsaved(&self) -> bool {
        self.session.base_keymap_unsaved()
    }

    /// Reads the active profile (`21/11`).
    ///
    /// Fails with [`WriteError::ProfileChangedBeforeSave`] when another
    /// profile than the one the last write went to is active while the
    /// commit settle has not passed yet (the keyboard probably discarded
    /// that write); [`known_profile`](Self::known_profile) is the reported
    /// profile even then.
    pub fn active_profile(&mut self) -> Result<ProfileId, WriteError> {
        let profile = self.read_active_profile()?;
        let last = self.session.last_write_profile();
        if !self.commit_pending().is_zero() && last.is_some_and(|p| p != profile.get()) {
            return Err(self.profile_changed(last, profile.get()));
        }
        Ok(profile)
    }

    /// Reads the active profile (`21/11`) and records it as known.
    fn read_active_profile(&mut self) -> Result<ProfileId, WriteError> {
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
    /// arrived meanwhile ([`WriteError::ProfileChangedBeforeSave`] when the
    /// keyboard had not saved the last write yet).
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
    /// active. Otherwise waits until the keyboard had
    /// [`WriteOptions::commit_settle`] since the last write of this session
    /// that changed a setting (see [`wait_for_commit`](Self::wait_for_commit):
    /// a switch before that would lose the write, and a profile change seen
    /// meanwhile fails with [`WriteError::ProfileChangedBeforeSave`]), then
    /// sends the select,
    /// waits for its acknowledgement and profile push, and confirms the
    /// switch with `21/11` (a missing push is only a warning when `21/11`
    /// confirms). Retries once, then fails with
    /// [`WriteError::ProfileNotSelected`].
    ///
    /// A switch made while [`base_keymap_unsaved`](Self::base_keymap_unsaved)
    /// drops that base keymap: the outcome then carries
    /// [`WriteWarning::BaseKeymapNotSaved`]. The switch is made all the same
    /// (an operation must always be able to return to the original profile).
    pub fn select_profile(&mut self, profile: ProfileId) -> Result<SelectOutcome, WriteError> {
        self.select_profile_observed(profile, &mut |_| {})
    }

    /// [`select_profile`](Self::select_profile), calling `on_wait` before it
    /// waits for the keyboard to save the last write (not called when there
    /// is nothing to wait for).
    pub fn select_profile_observed(
        &mut self,
        profile: ProfileId,
        on_wait: &mut dyn FnMut(CommitWait),
    ) -> Result<SelectOutcome, WriteError> {
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
        // A select before the keyboard saved the last write loses it.
        if let Some(wait) = self.wait_for_commit_observed(on_wait)? {
            warnings.push(WriteWarning::WaitedForCommit {
                waited_ms: wait.wait_ms,
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
                self.session.send_select(&step.packet)?;
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
                // The switch reloaded the profile: a base keymap without
                // the Fn keymap after it is gone (spec 4.8.2).
                if self.session.base_keymap_unsaved() {
                    self.session.set_base_keymap_unsaved(false);
                    warnings.push(WriteWarning::BaseKeymapNotSaved);
                }
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

    /// How much longer the keyboard needs before it has had the commit
    /// settle ([`WriteOptions::commit_settle`]) since the last write of this
    /// session that changed a setting ([`Session::last_write_at`]); zero
    /// when nothing is pending, also when the keyboard switched profiles
    /// before the save (see [`commit_interrupted`](Self::commit_interrupted):
    /// it reloaded the profile, there is nothing left to save).
    pub fn commit_pending(&self) -> Duration {
        match self.session.last_write() {
            Some(last) if !last.discarded => (last.at + self.options.commit_settle.duration())
                .saturating_duration_since(self.session.now()),
            _ => Duration::ZERO,
        }
    }

    /// Whether the keyboard may not have saved the last write of this
    /// session that changed a setting: before the commit settle had passed
    /// since it, another profile became active (seen in a report, in a
    /// `21/11` read, or at the end of a wait; see
    /// [`WriteError::ProfileChangedBeforeSave`]) or the connection was lost
    /// (an unplugged keyboard loses unsaved edits, hardware test A10; spec
    /// 4.5.3, "Delayed commit"). Such a write was verified, but only in the
    /// keyboard's working copy. The writes concerned are those made to
    /// [`Session::last_write_profile`] since that profile was last selected.
    /// A reset notification is not counted here: it reports a reset of the
    /// keyboard's settings.
    pub fn commit_interrupted(&self) -> bool {
        let Some(last) = self.session.last_write() else {
            return false;
        };
        if last.discarded {
            return true;
        }
        let saved_by = last.at + self.options.commit_settle.duration();
        self.session.connection_lost() && self.session.fatal_at().is_some_and(|at| at < saved_by)
    }

    /// Waits until the keyboard had the commit settle since the last write
    /// that changed a setting, so that it saved the write (see
    /// [`CommitSettle`]). Listens meanwhile: reports are classified as
    /// always, and a reset notification or an unexpected profile report
    /// ends the wait with an error (a reset poisons the writer). After a
    /// wait it reads the active profile again: a profile switch made on the
    /// keyboard is not necessarily reported (spec 4.5.3, Q2), and one made
    /// during the wait may have discarded the write. A profile change seen
    /// during the wait or at its end fails with
    /// [`WriteError::ProfileChangedBeforeSave`]
    /// ([`commit_interrupted`](Self::commit_interrupted) is then true, and
    /// nothing waits for that write any more). Returns the wait, or `None`
    /// when nothing was pending.
    ///
    /// [`select_profile`](Self::select_profile) calls it before every
    /// switch; restores and edits call it before they return. A caller of
    /// [`write_guarded`](Self::write_guarded) should call it before it tells
    /// the user that a change is done.
    pub fn wait_for_commit(&mut self) -> Result<Option<CommitWait>, WriteError> {
        self.wait_for_commit_observed(&mut |_| {})
    }

    /// [`wait_for_commit`](Self::wait_for_commit), calling `on_wait` before
    /// the wait starts (not called when nothing is pending).
    pub fn wait_for_commit_observed(
        &mut self,
        on_wait: &mut dyn FnMut(CommitWait),
    ) -> Result<Option<CommitWait>, WriteError> {
        self.ensure_usable()?;
        let remaining = self.commit_pending();
        if remaining.is_zero() {
            return Ok(None);
        }
        let wait = CommitWait {
            profile: self.session.last_write_profile(),
            wait_ms: millis(remaining),
            settle_ms: millis(self.options.commit_settle.duration()),
        };
        on_wait(wait);
        // A listen can end early (a transport that does not block); keep
        // listening until the transport's clock says the settle is over.
        let mut other_program = None;
        loop {
            let left = self.commit_pending();
            if left.is_zero() {
                break;
            }
            // The listen ends at a profile report, which arrived before the
            // settle was over: a profile change came before the save.
            match self.listen_checked_until(left, true) {
                Ok(()) => {}
                Err(WriteError::ProfileChanged { reported, .. })
                    if wait.profile != Some(reported) =>
                {
                    self.session.discard_pending_write();
                    return Err(WriteError::ProfileChangedBeforeSave {
                        profile: wait.profile,
                        reported,
                    });
                }
                // Another program's query reply for the same profile: the
                // keyboard still has to save, so the wait goes on, and the
                // error is returned when it is over.
                Err(error @ WriteError::OtherProgramActive { .. }) => {
                    other_program.get_or_insert(error);
                }
                Err(error) => return Err(error),
            }
        }
        // A switch on the keyboard may come without a report: the profile
        // the write went to must still be active. When it is not, the switch
        // happened at some time during the wait, possibly before the save.
        if let Some(profile) = wait.profile {
            let active = self.read_active_profile()?;
            if active.get() != profile {
                self.session.discard_pending_write();
                return Err(WriteError::ProfileChangedBeforeSave {
                    profile: Some(profile),
                    reported: active.get(),
                });
            }
        }
        match other_program {
            Some(error) => Err(error),
            None => Ok(Some(wait)),
        }
    }

    /// Writes one table to the active profile, guarded like every write of
    /// this crate. This is the only public way to send a table write (apart
    /// from [`write_commit_trigger`](Self::write_commit_trigger), which only
    /// sends the Fn keymap).
    ///
    /// 1. Refuses when another program was seen while writing.
    /// 2. Checks that `saved` (a verified safety backup, see
    ///    [`Backup::save_safety`](crate::Backup::save_safety)) belongs to this
    ///    keyboard model (`0D`, read once per writer) and holds the active
    ///    profile, confirmed with `21/11`; else
    ///    [`WriteError::NotBackedUp`] and nothing is sent.
    /// 3. Re-reads exactly the bytes `request` writes: when the keyboard
    ///    already holds them, nothing is sent
    ///    ([`GuardedWrite::AlreadyMatched`]). After a base-keymap write the
    ///    Fn keymap must be sent even then: use
    ///    [`write_commit_trigger`](Self::write_commit_trigger) (until then
    ///    [`base_keymap_unsaved`](Self::base_keymap_unsaved) is true, and a
    ///    profile switch reports [`WriteWarning::BaseKeymapNotSaved`]).
    /// 4. Writes stop-and-wait and verifies by reading back, with one more
    ///    attempt with the whole table (see the
    ///    [module documentation](crate::writer)). A main-light write is
    ///    rebuilt from a fresh read first, so the music-rhythm leave is sent
    ///    exactly when the keyboard's current mode changes.
    ///
    /// A profile select needs no backup: use
    /// [`select_profile`](Self::select_profile) (a
    /// [`WriteRequest::SelectProfile`] passed here is handled the same way).
    ///
    /// It returns as soon as the write is verified, which reads the
    /// keyboard's working copy: the keyboard saves the write about 1 s later.
    /// Several writes can follow each other; call
    /// [`wait_for_commit`](Self::wait_for_commit) before reporting the
    /// change as done ([`select_profile`](Self::select_profile) waits on its
    /// own). When that fails with [`WriteError::ProfileChangedBeforeSave`],
    /// or the connection was lost, see
    /// [`commit_interrupted`](Self::commit_interrupted): the change may not
    /// have been saved.
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
        let profile = self.check_backed_up(saved)?;
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

    /// Writes the Fn keymap of `trigger` to the active profile **also when
    /// the keyboard already holds it**, so that the keyboard saves a base
    /// keymap written before it (see [`CommitTrigger`]; spec 4.8.2). Call it
    /// right after a base-keymap write made with
    /// [`write_guarded`](Self::write_guarded), before anything else.
    ///
    /// Every guard of [`write_guarded`](Self::write_guarded) applies, except
    /// that a matching table is sent anyway: it refuses after another
    /// program was seen, needs a `saved` safety backup of the active
    /// profile of this keyboard model (confirmed with `21/11`), writes
    /// stop-and-wait, verifies by reading back (one more attempt with the
    /// whole table) and counts towards [`WriteError::RepeatedWrite`].
    ///
    /// The read-back of an unchanged table matches whether it arrived or
    /// not, and whether a partial `18 02` makes the keyboard save is
    /// unknown. So every packet must also be acknowledged: an ack timeout
    /// (which stops the send) counts as a failed verification, the whole
    /// table is sent once more, and when that send is incomplete too the
    /// write fails with [`WriteError::CommitTriggerIncomplete`] (the base
    /// keymap then stays unsaved, see
    /// [`base_keymap_unsaved`](Self::base_keymap_unsaved)).
    ///
    /// Like any write, the keyboard saves it about 1 s later: call
    /// [`wait_for_commit`](Self::wait_for_commit) before reporting the change
    /// as done.
    pub fn write_commit_trigger(
        &mut self,
        saved: &SavedBackup,
        trigger: &CommitTrigger,
    ) -> Result<WriteOutcome, WriteError> {
        self.write_commit_trigger_observed(saved, trigger, &mut |_| {})
    }

    /// [`write_commit_trigger`](Self::write_commit_trigger), calling
    /// `on_packet` before every packet.
    pub fn write_commit_trigger_observed(
        &mut self,
        saved: &SavedBackup,
        trigger: &CommitTrigger,
        on_packet: &mut dyn FnMut(PacketProgress),
    ) -> Result<WriteOutcome, WriteError> {
        self.ensure_usable()?;
        self.consent.ensure_not_standing_rule()?;
        self.check_backed_up(saved)?;
        self.write_trigger_observed(trigger, on_packet)
    }

    /// Steps 1 and 2 of [`write_guarded`](Self::write_guarded): refuses
    /// after foreign activity, and unless `saved` holds the active profile
    /// (read with `21/11`) of this keyboard model. Returns the active
    /// profile.
    fn check_backed_up(&mut self, saved: &SavedBackup) -> Result<ProfileId, WriteError> {
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
        Ok(profile)
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
        self.write_with(request, Completion::Verified, on_packet, None)
    }

    /// [`write_observed`](Self::write_observed) with the session's SOCD push
    /// count (`socd_mark`, [`Session::socd_pushes`]) taken by the caller
    /// before it read the table to check it for changes: a SOCD switch on
    /// the keyboard from then on is reported
    /// ([`WriteWarning::SocdToggledOnKeyboard`],
    /// [`WriteError::ChangedOnKeyboard`]).
    pub(crate) fn write_observed_marked(
        &mut self,
        request: &WriteRequest,
        on_packet: &mut dyn FnMut(PacketProgress),
        socd_mark: u64,
    ) -> Result<WriteOutcome, WriteError> {
        self.write_with(request, Completion::Verified, on_packet, Some(socd_mark))
    }

    /// [`write_observed`](Self::write_observed) for a [`CommitTrigger`]
    /// (always an Fn-keymap write): besides the read-back, every packet must
    /// be sent and acknowledged, else the write counts as not verified (one
    /// more attempt with the whole table, then
    /// [`WriteError::CommitTriggerIncomplete`]). See
    /// [`write_commit_trigger`](Self::write_commit_trigger).
    ///
    /// Crate-private like `write_observed`: callers are
    /// [`write_commit_trigger_observed`](Self::write_commit_trigger_observed)
    /// and the restore, which check the backup and the active profile.
    pub(crate) fn write_trigger_observed(
        &mut self,
        trigger: &CommitTrigger,
        on_packet: &mut dyn FnMut(PacketProgress),
    ) -> Result<WriteOutcome, WriteError> {
        self.write_with(
            trigger.request(),
            Completion::EveryPacketAcked,
            on_packet,
            None,
        )
    }

    /// The write of [`write_observed`](Self::write_observed) and
    /// [`write_trigger_observed`](Self::write_trigger_observed). `socd_mark`:
    /// the SOCD push count to compare with (else the count right now).
    fn write_with(
        &mut self,
        request: &WriteRequest,
        completion: Completion,
        on_packet: &mut dyn FnMut(PacketProgress),
        socd_mark: Option<u64>,
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
        // A standing rule (an automatic switch) only ever selects profiles.
        self.consent.ensure_not_standing_rule()?;
        self.listen_checked(Duration::ZERO)?;
        if let Some(evidence) = self.stop {
            return Err(WriteError::OtherProgramActive { evidence });
        }
        // Nothing is written while the keyboard may stream key depths (spec
        // 4.11.7): a test left open must be closed first.
        self.check_travel_closed()?;
        // SOCD switched on the keyboard (Fn + Right Shift) during the write
        // (or since the caller's check of the table).
        let socd_pushes = socd_mark.unwrap_or_else(|| self.session.socd_pushes());
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
        let sent = self.send_steps(request, 1, &mut outcome, on_packet)?;
        let verified = self.verify(request, &mut outcome)?;
        let toggled = self.session.socd_pushes() > socd_pushes;
        let first = match self.unverified(request, completion, sent, verified, 1) {
            None => {
                if let (true, WriteRequest::Socd(socd)) = (toggled, request) {
                    outcome.warnings.push(WriteWarning::SocdToggledOnKeyboard {
                        power: socd.power(),
                    });
                }
                return Ok(self.verified_outcome(request, sent, outcome));
            }
            // The user switched SOCD on the keyboard meanwhile: sending the
            // table again would override that.
            Some(_) if toggled && request.table() == TableId::Socd => {
                return Err(WriteError::ChangedOnKeyboard {
                    table: TableId::Socd,
                });
            }
            Some(failure) => failure,
        };

        // One more attempt with the whole table.
        if let Some(evidence) = self.stop {
            return Err(WriteError::OtherProgramActive { evidence });
        }
        if self.sends.get(&key).copied().unwrap_or(0) >= 2 {
            return Err(first);
        }
        self.listen_checked(self.options.retry_quiet)?;
        let retry = self.rebuild(request)?;
        self.count_send(key)?;
        outcome.sends = 2;
        outcome.warnings.push(WriteWarning::Retried);
        let sent = self.send_steps(&retry, 2, &mut outcome, on_packet)?;
        let verified = self.verify(&retry, &mut outcome)?;
        match self.unverified(&retry, completion, sent, verified, 2) {
            None => Ok(self.verified_outcome(&retry, sent, outcome)),
            Some(failure) => Err(failure),
        }
    }

    /// Why send number `sends` of `request` is not verified, if it is not:
    /// the read-back differs ([`WriteError::VerifyFailed`]), or a commit
    /// trigger did not reach the keyboard complete
    /// ([`WriteError::CommitTriggerIncomplete`]).
    fn unverified(
        &self,
        request: &WriteRequest,
        completion: Completion,
        sent: Sent,
        verified: Result<(), Mismatch>,
        sends: u32,
    ) -> Option<WriteError> {
        match (verified, sent) {
            (Err(mismatch), _) => Some(WriteError::VerifyFailed {
                table: request.table(),
                sends,
                mismatch: Box::new(mismatch),
            }),
            (Ok(()), Sent::Stopped { packet, of })
                if completion == Completion::EveryPacketAcked =>
            {
                Some(WriteError::CommitTriggerIncomplete { sends, packet, of })
            }
            (Ok(()), _) => None,
        }
    }

    /// A verified write: records whether it saved a base keymap sent
    /// before it (a complete Fn-keymap write, spec 4.8.2).
    fn verified_outcome(
        &mut self,
        request: &WriteRequest,
        sent: Sent,
        outcome: WriteOutcome,
    ) -> WriteOutcome {
        if sent == Sent::Complete && matches!(request, WriteRequest::Keymap(Layer::Fn, _)) {
            self.session.set_base_keymap_unsaved(false);
        }
        outcome
    }

    /// Refuses a table write while the keyboard may stream key depths: a
    /// close that was not acknowledged ([`WriteError::TravelTestMaybeOpen`])
    /// or stream reports while no test of ours is open
    /// ([`WriteError::TravelTestOpen`]).
    pub(crate) fn check_travel_closed(&self) -> Result<(), WriteError> {
        match self.session.travel_state() {
            TravelState::MaybeOpen => return Err(WriteError::TravelTestMaybeOpen),
            TravelState::Open => {
                return Err(WriteError::TravelTestOpen { stray_reports: 0 });
            }
            TravelState::Closed => {}
        }
        let stray_reports = self.session.stray_travel_since_gate();
        if stray_reports > 0 {
            return Err(WriteError::TravelTestOpen { stray_reports });
        }
        Ok(())
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

    /// The keyboard's model (`0D`, read once per writer).
    pub(crate) fn model(&mut self) -> Result<&'static Model, WriteError> {
        Ok(self.identity()?.model)
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
    pub(crate) fn ensure_usable(&self) -> Result<(), WriteError> {
        match self.session.fatal() {
            Some(fatal) => Err(fatal_error(fatal)),
            None => Ok(()),
        }
    }

    /// Records foreign activity seen during an operation.
    pub(crate) fn note_foreign(&mut self, activity: ForeignActivity) {
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
                DeviceEvent::ProfileReport { profile, extra } => Some((*profile, *extra)),
                _ => None,
            });
        match reported {
            Some((profile, extra)) => Err(self.unexpected_profile_report(profile, extra)),
            None => Ok(()),
        }
    }

    /// The error for a profile report we did not cause (`extra`: its byte
    /// 7). A report of another profile than the known one, or of the same
    /// one in the push form (byte 7 = `00`, a switch on the keyboard), is a
    /// profile change: the reported profile becomes the known one. A
    /// same-profile query reply (byte 7 = `01`) is another program's.
    fn unexpected_profile_report(&mut self, reported: u8, extra: u8) -> WriteError {
        let expected = self.known_profile.map(ProfileId::get);
        if expected == Some(reported) && extra != 0x00 {
            // The same profile: another program's query reply.
            return WriteError::OtherProgramActive {
                evidence: ForeignActivity {
                    profile_reports: 1,
                    ..ForeignActivity::default()
                },
            };
        }
        if let Some(profile) = ProfileId::new(reported) {
            self.known_profile = Some(profile);
        }
        self.profile_changed(expected, reported)
    }

    /// The error for a change of the active profile to `reported` that we
    /// did not cause (`expected`: the profile we expected). While the commit
    /// settle since the last write has not passed and that write went to
    /// another profile, the keyboard probably discarded it: the error says
    /// so ([`WriteError::ProfileChangedBeforeSave`]), and the pending write
    /// is recorded as discarded
    /// ([`commit_interrupted`](Self::commit_interrupted)), so nothing waits
    /// for it any more.
    pub(crate) fn profile_changed(&mut self, expected: Option<u8>, reported: u8) -> WriteError {
        let last = self.session.last_write_profile();
        if !self.commit_pending().is_zero() && last != Some(reported) {
            self.session.discard_pending_write();
            return WriteError::ProfileChangedBeforeSave {
                profile: last,
                reported,
            };
        }
        WriteError::ProfileChanged { expected, reported }
    }

    /// Listens for `duration` (draining queued reports even when it is zero)
    /// and classifies what arrives.
    fn listen_checked(&mut self, duration: Duration) -> Result<(), WriteError> {
        self.listen_checked_until(duration, false)
    }

    /// [`listen_checked`](Self::listen_checked); with
    /// `stop_at_profile_report` it ends (with the error) at the first
    /// profile report we did not cause.
    fn listen_checked_until(
        &mut self,
        duration: Duration,
        stop_at_profile_report: bool,
    ) -> Result<(), WriteError> {
        let events = self.session.events().len();
        let summary = self
            .session
            .listen_until(duration, stop_at_profile_report)?;
        self.note_foreign(summary.activity);
        self.check_new_events(events)
    }

    /// Handles a report that is not the acknowledgement being waited for.
    pub(crate) fn handle_unrelated(&mut self, report: &Report) -> Result<(), WriteError> {
        match self.session.classify_unrelated(report) {
            Unrelated::Reset => Err(WriteError::KeyboardReset {
                report: report.summary(),
            }),
            Unrelated::ProfileReport { profile, extra } => {
                Err(self.unexpected_profile_report(profile, extra))
            }
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
            // The keyboard's own reports (SOCD push, travel stream): never
            // another program.
            Unrelated::KeyboardEvent
            | Unrelated::Travel
            | Unrelated::Expected
            | Unrelated::Other => Ok(()),
        }
    }

    /// Waits up to `timeout` for the acknowledgement described by `spec`.
    /// Unrelated reports do not extend the deadline.
    pub(crate) fn wait_for(
        &mut self,
        spec: AckSpec,
        timeout: Duration,
    ) -> Result<AckTracker, WriteError> {
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
    /// timeout: the rest of the table is not sent and the caller verifies
    /// ([`Sent::Stopped`]).
    fn send_steps(
        &mut self,
        request: &WriteRequest,
        send: u32,
        outcome: &mut WriteOutcome,
        on_packet: &mut dyn FnMut(PacketProgress),
    ) -> Result<Sent, WriteError> {
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
            self.session
                .send_write(&step.packet, self.known_profile.map(ProfileId::get))?;
            outcome.packets += 1;
            if let WriteRequest::Keymap(Layer::Base, _) = request {
                // In the working copy now; saved only by an 18 02 after it.
                self.session.set_base_keymap_unsaved(true);
            }
            if let AckSpec::Possible { .. } = step.ack {
                // No ack is expected (SOCD power, c15), but one may come:
                // listen for it during the pause instead of taking it for
                // another program's.
                self.listen_for_possible_ack(step.ack, step.gap_after, packet, outcome)?;
                continue;
            }
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
                        return Ok(Sent::Stopped { packet, of });
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
        Ok(Sent::Complete)
    }

    /// Listens for `window` after a packet whose acknowledgement may come
    /// but is not expected ([`AckSpec::Possible`]): a matching report is
    /// counted as ours and reported ([`WriteWarning::UnexpectedAck`]);
    /// everything else is classified as always. Never waits longer, never
    /// registers a late acknowledgement.
    fn listen_for_possible_ack(
        &mut self,
        spec: AckSpec,
        window: Duration,
        packet: usize,
        outcome: &mut WriteOutcome,
    ) -> Result<(), WriteError> {
        let mut tracker = spec.tracker();
        let deadline = self.session.now() + window;
        // After the window only queued reports are drained, a bounded number.
        let mut late = 0usize;
        loop {
            let remaining = deadline.saturating_duration_since(self.session.now());
            if remaining.is_zero() {
                if late >= SETTLE_DRAIN_MAX {
                    break;
                }
                late += 1;
            }
            let Some(report) = self.session.read_input(remaining)? else {
                break;
            };
            if tracker.offer(&report) == AckOffer::Ack {
                self.session.count_ack();
                outcome
                    .warnings
                    .push(WriteWarning::UnexpectedAck { packet });
            } else {
                self.handle_unrelated(&report)?;
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
        Fatal::Refused(packet) => WriteError::EgressRefused {
            packet: packet.clone(),
        },
    }
}
