//! Progress of one operation: library progress mapped to
//! [`ProgressStep`]s, throttled, with the operation's cancellation state.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use openwraith_hid::protocol::{Layer, TableId};
use openwraith_hid::{ProfilesProgress, Progress, WriteProgress};

use crate::clock::{Clock, millis};
use crate::dto::{CancelState, OpId, ProgressEvent, ProgressStep, ReadPurpose};
use crate::map::{key_ref, sat8, sat16, sat32, table_ref};
use crate::shared::Shared;
use crate::sink::ProgressSink;

/// Repeated read and packet events are sent at most this often.
const MIN_INTERVAL: Duration = Duration::from_millis(50);

/// Where Stop takes effect in the current phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelPolicy {
    /// Never (profile switch; a light edit once its safety backup is read).
    Never,
    /// After the current request or table (reads, backups, trigger edits).
    Boundaries,
    /// A restore: at table boundaries; inside the keymap group, once a group
    /// table was sent, the group's Fn keymap still goes out.
    Restore,
}

/// The progress of one operation.
pub(crate) struct OpProgress {
    op: OpId,
    seq: u32,
    started: Instant,
    clock: Arc<dyn Clock>,
    sink: Arc<dyn ProgressSink>,
    shared: Arc<Shared>,
    cancel: Arc<AtomicBool>,
    last_at: Option<Instant>,
    last_name: Option<&'static str>,
    /// Where Stop takes effect.
    pub(crate) policy: CancelPolicy,
    item: u16,
    /// Writes of the operation (for "item of items").
    pub(crate) items: u16,
    group_open: bool,
}

fn step_name(step: &ProgressStep) -> &'static str {
    match step {
        ProgressStep::CheckingOtherPrograms { .. } => "checking_other_programs",
        ProgressStep::Reading { .. } => "reading",
        ProgressStep::SelectingProfile { .. } => "selecting_profile",
        ProgressStep::ReturningToProfile { .. } => "returning_to_profile",
        ProgressStep::Comparing => "comparing",
        ProgressStep::SavingSafetyBackup => "saving_safety_backup",
        ProgressStep::SavingFile => "saving_file",
        ProgressStep::Writing { .. } => "writing",
        ProgressStep::WaitingForKeyboardSave { .. } => "waiting_for_keyboard_save",
        ProgressStep::CheckingPersistence { .. } => "checking_persistence",
        ProgressStep::FinalCheck => "final_check",
        ProgressStep::RefreshingView => "refreshing_view",
    }
}

/// Whether a step is sent even right after another one.
fn always_sent(step: &ProgressStep) -> bool {
    match step {
        ProgressStep::Reading { done, total, .. } => done == total || *done <= 1,
        ProgressStep::Writing {
            packet, packets, ..
        } => *packet <= 1 || packet == packets,
        _ => true,
    }
}

/// Whether Stop waits for the step to end.
fn not_now(step: &ProgressStep) -> bool {
    matches!(
        step,
        ProgressStep::CheckingOtherPrograms { .. }
            | ProgressStep::WaitingForKeyboardSave { .. }
            | ProgressStep::CheckingPersistence { .. }
            | ProgressStep::FinalCheck
            | ProgressStep::ReturningToProfile { .. }
            | ProgressStep::SavingSafetyBackup
            | ProgressStep::SavingFile
            | ProgressStep::RefreshingView
    )
}

/// Whether a table belongs to the keymap group.
fn in_keymap_group(table: TableId) -> bool {
    matches!(
        table,
        TableId::Keymap { .. } | TableId::Macro { .. } | TableId::AdvancedKey { .. }
    )
}

impl OpProgress {
    /// The progress of operation `op`.
    pub(crate) fn new(
        op: OpId,
        clock: Arc<dyn Clock>,
        sink: Arc<dyn ProgressSink>,
        shared: Arc<Shared>,
        cancel: Arc<AtomicBool>,
        policy: CancelPolicy,
    ) -> Self {
        let started = clock.now();
        OpProgress {
            op,
            seq: 0,
            started,
            clock,
            sink,
            shared,
            cancel,
            last_at: None,
            last_name: None,
            policy,
            item: 0,
            items: 0,
            group_open: false,
        }
    }

    /// Whether Stop was asked for.
    pub(crate) fn cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// What the library is told: stop when Stop was asked for and the
    /// operation can be stopped.
    pub(crate) fn flow(&self) -> ControlFlow<()> {
        if self.policy != CancelPolicy::Never && self.cancel_requested() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    fn cancel_state(&self, step: &ProgressStep) -> CancelState {
        if self.policy == CancelPolicy::Never {
            return CancelState::NotCancellable;
        }
        if self.cancel_requested() {
            if self.policy == CancelPolicy::Restore && self.group_open {
                return CancelState::FinishingKeymapGroup;
            }
            return CancelState::Requested;
        }
        if not_now(step) {
            CancelState::NotNow
        } else {
            CancelState::Available
        }
    }

    /// Sends a step (repeated read and packet events at most every 50 ms).
    pub(crate) fn step(&mut self, step: ProgressStep) {
        let now = self.clock.now();
        let name = step_name(&step);
        let force = self.last_name != Some(name) || always_sent(&step);
        if !force
            && self
                .last_at
                .is_some_and(|t| now.saturating_duration_since(t) < MIN_INTERVAL)
        {
            return;
        }
        self.last_at = Some(now);
        self.last_name = Some(name);
        self.seq = self.seq.wrapping_add(1);
        let cancel = self.cancel_state(&step);
        let event = ProgressEvent {
            op: self.op.clone(),
            seq: self.seq,
            elapsed_ms: millis(now.saturating_duration_since(self.started)),
            step,
            cancel,
        };
        self.shared.set_last_progress(&event);
        self.sink.send(event);
    }

    /// A read of the active profile.
    pub(crate) fn read(&mut self, purpose: ReadPurpose, p: Progress) -> ControlFlow<()> {
        self.step(ProgressStep::Reading {
            purpose,
            profile: None,
            done: sat16(p.done),
            total: sat16(p.total),
        });
        self.flow()
    }

    /// Progress of reading several profiles.
    pub(crate) fn profiles(
        &mut self,
        purpose: ReadPurpose,
        p: ProfilesProgress,
    ) -> ControlFlow<()> {
        let step = match p {
            ProfilesProgress::Reading {
                profile,
                done,
                total,
            } => ProgressStep::Reading {
                purpose,
                profile,
                done: sat16(done),
                total: sat16(total),
            },
            ProfilesProgress::Selecting { profile } => ProgressStep::SelectingProfile { profile },
            ProfilesProgress::Returning { profile } => ProgressStep::ReturningToProfile { profile },
            ProfilesProgress::WaitingForCommit { wait_ms } => {
                ProgressStep::WaitingForKeyboardSave {
                    profile: None,
                    wait_ms: sat32(wait_ms),
                }
            }
        };
        self.step(step);
        self.flow()
    }

    /// Progress of a restore or an edit.
    pub(crate) fn write(&mut self, p: WriteProgress) -> ControlFlow<()> {
        match p {
            WriteProgress::SafetyBackup { progress } => {
                return self.profiles(ReadPurpose::Compare, progress);
            }
            WriteProgress::Planning => self.step(ProgressStep::Comparing),
            WriteProgress::Writing {
                profile,
                table,
                key,
                packet,
                of,
                send,
            } => {
                if packet == 0 && send == 1 {
                    self.item = self.item.saturating_add(1);
                }
                if in_keymap_group(table) {
                    if packet >= 1 {
                        self.group_open = true;
                    }
                    if table == (TableId::Keymap { layer: Layer::Fn }) && packet == of && packet > 0
                    {
                        self.group_open = false;
                    }
                } else {
                    self.group_open = false;
                }
                self.step(ProgressStep::Writing {
                    profile,
                    table: table_ref(table),
                    key: key.map(key_ref),
                    item: self.item.max(1),
                    items: self.items.max(self.item),
                    packet: sat16(packet),
                    packets: sat16(of),
                    send: sat8(send),
                });
            }
            WriteProgress::Verifying => self.step(ProgressStep::FinalCheck),
            WriteProgress::Returning { profile } => {
                self.group_open = false;
                self.step(ProgressStep::ReturningToProfile { profile });
            }
            WriteProgress::WaitingForCommit { profile, wait_ms } => {
                self.step(ProgressStep::WaitingForKeyboardSave {
                    profile,
                    wait_ms: sat32(wait_ms),
                });
            }
            WriteProgress::CheckingPersistence { profile } => {
                self.group_open = false;
                self.step(ProgressStep::CheckingPersistence { profile });
            }
        }
        self.flow()
    }
}
