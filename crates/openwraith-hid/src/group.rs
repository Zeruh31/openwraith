//! The keymap group's write order and its cancellation rules, shared by
//! restores and edits (one implementation).
//!
//! The keymap group (both keymap layers, the ten macro slots and the forty
//! advanced-key slots) is written in the vendor site's order: changed macro
//! slots ascending, the base keymap, the Fn keymap, changed advanced-key
//! slots ascending (spec 4.8.2). The Fn keymap is the group's commit
//! trigger: the keyboard keeps a base keymap only when an Fn-keymap write
//! follows it, so it is written whenever the base keymap is (re-sent
//! unchanged if need be), and between the macro and the advanced-key slots
//! when only slots change.
//!
//! [`GroupCursor`] decides, table by table, whether to send, skip or stop:
//!
//! 1. A cancellation before any group table was sent stops at once.
//! 2. Once a group table before the trigger was sent, the trigger is due: a
//!    cancellation skips every other table up to the trigger (except those
//!    that complete a binding, see 3), sends the trigger, and takes effect
//!    after it.
//! 3. A write that *completes a binding* (the content of a slot that a
//!    layer written in this group refers to, or a layer write that removes
//!    the reference to a slot cleared in this group) is sent even after a
//!    cancellation once a table it completes was sent
//!    ([`GroupWrite::binding_tables`]), so that a sent binding never stays
//!    pointed at an empty or stale slot because of a cancel.
//! 4. A failure while the trigger is due leaves the base keymap unsaved
//!    ([`base_unsaved_on_stop`](GroupCursor::base_unsaved_on_stop)).
//!
//! [`dangling_after`] finds the references that point at empty slots after
//! an operation stopped part-way.

use openwraith_protocol::{
    AdvancedSlotId, KeymapData, KeymapReference, Layer, MacroSlotId, TableId,
};
use serde::Serialize;

/// What the cursor needs to know about one planned write.
pub(crate) trait GroupWrite {
    /// The table written.
    fn table(&self) -> TableId;
    /// Whether it is the keymap group's commit trigger (the Fn keymap).
    fn is_trigger(&self) -> bool;
    /// The tables whose binding this write completes (see the
    /// [module documentation](self), rule 3): once one of them was sent, it
    /// is sent even after a cancellation.
    fn binding_tables(&self) -> &[TableId];
}

/// What to do with the next write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// Send it.
    Send,
    /// Leave it out (a cancellation); it stays not attempted.
    Skip,
    /// Stop now (a cancellation before any group table was sent).
    Stop,
}

/// Whether `table` belongs to the keymap group.
pub(crate) fn is_keymap_group(table: TableId) -> bool {
    matches!(
        table,
        TableId::Keymap { .. } | TableId::Macro { .. } | TableId::AdvancedKey { .. }
    )
}

/// The cancellation state of one profile's writes (see the
/// [module documentation](self)).
#[derive(Debug, Clone, Default)]
pub(crate) struct GroupCursor {
    trigger_at: Option<usize>,
    trigger_due: bool,
    after_trigger: bool,
    skipped: bool,
    /// The tables sent so far.
    sent: Vec<TableId>,
}

impl GroupCursor {
    /// A cursor for `writes`, in execution order.
    pub(crate) fn new<W: GroupWrite>(writes: &[W]) -> Self {
        GroupCursor {
            trigger_at: writes.iter().position(GroupWrite::is_trigger),
            ..GroupCursor::default()
        }
    }

    /// Decides about write `index`. `cancelled` says whether a cancellation
    /// was asked for already; otherwise `announce` reports the start of the
    /// table (a progress event) and says whether the caller asks to stop
    /// now. The trigger's start is always announced (and the answer only
    /// recorded by the caller): it is sent whatever happens once it is due.
    pub(crate) fn before<W: GroupWrite>(
        &mut self,
        index: usize,
        write: &W,
        cancelled: bool,
        announce: &mut dyn FnMut() -> bool,
    ) -> Step {
        if self.trigger_due && self.trigger_at == Some(index) {
            // Sent whatever happens: the group's other tables need it.
            let _ = announce();
            return Step::Send;
        }
        let cancel = cancelled || announce();
        if self.trigger_due || self.after_trigger {
            let completes = write
                .binding_tables()
                .iter()
                .any(|table| self.sent.contains(table));
            if !cancel || completes {
                return Step::Send;
            }
            self.skipped = true;
            return Step::Skip;
        }
        if cancel {
            return Step::Stop;
        }
        Step::Send
    }

    /// Records the result of write `index`: whether it was sent (written,
    /// possibly lost).
    pub(crate) fn after<W: GroupWrite>(&mut self, index: usize, write: &W, sent: bool) {
        if sent {
            self.sent.push(write.table());
        }
        let before_trigger = self.trigger_at.is_some_and(|at| index < at);
        if before_trigger && sent && is_keymap_group(write.table()) {
            self.trigger_due = true;
        }
        if self.trigger_at == Some(index) && sent {
            self.trigger_due = false;
            self.after_trigger = true;
        }
    }

    /// Whether a group table before the trigger was sent and the trigger
    /// was not: a stop now leaves the base keymap unsaved.
    pub(crate) fn base_unsaved_on_stop(&self) -> bool {
        self.trigger_due
    }

    /// Whether tables were skipped because of a cancellation: the
    /// cancellation takes effect after the last write that was sent.
    pub(crate) fn cancelled_with_skips(&self) -> bool {
        self.skipped
    }
}

/// A slot of the keymap group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", content = "slot", rename_all = "snake_case")]
pub enum SlotRef {
    /// A macro slot (`19`).
    Macro(MacroSlotId),
    /// An advanced-key slot (`22`).
    Advanced(AdvancedSlotId),
}

impl std::fmt::Display for SlotRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlotRef::Macro(slot) => write!(f, "macro slot {}", slot.get()),
            SlotRef::Advanced(slot) => write!(f, "advanced-key slot {}", slot.get()),
        }
    }
}

/// A keymap entry that refers to an empty slot after an operation stopped
/// part-way: the key does nothing until the slot is written or the entry
/// reset (the undo command restores both).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct DanglingBinding {
    /// The profile.
    pub profile: u8,
    /// The layer of the entry.
    pub layer: Layer,
    /// The key index of the entry.
    pub index: u8,
    /// The empty slot it refers to.
    pub slot: SlotRef,
}

/// The keymap group of one profile as far as it is known: both layers and
/// whether each slot holds content.
#[derive(Debug, Clone)]
pub(crate) struct GroupView {
    /// The layers (`None` when unknown).
    pub(crate) layers: [Option<KeymapData>; 2],
    /// Whether each macro slot holds content.
    pub(crate) macros: [bool; 10],
    /// Whether each advanced-key slot holds content.
    pub(crate) advanced: [bool; 40],
}

impl Default for GroupView {
    /// Nothing known: no layer, every slot empty.
    fn default() -> Self {
        GroupView {
            layers: [None, None],
            macros: [false; 10],
            advanced: [false; 40],
        }
    }
}

impl GroupView {
    /// Every reference of a known layer to an empty slot.
    pub(crate) fn dangling(&self, profile: u8) -> Vec<DanglingBinding> {
        let mut out = Vec::new();
        for (layer, data) in Layer::ALL.into_iter().zip(&self.layers) {
            let Some(data) = data else { continue };
            for reference in data.references() {
                let found = match reference {
                    KeymapReference::Macro { index, slot } => (!self
                        .macros
                        .get(usize::from(slot.get()))
                        .copied()
                        .unwrap_or(true))
                    .then_some((index, SlotRef::Macro(slot))),
                    KeymapReference::Advanced { index, slot } => (!self
                        .advanced
                        .get(usize::from(slot.get()))
                        .copied()
                        .unwrap_or(true))
                    .then_some((index, SlotRef::Advanced(slot))),
                    KeymapReference::Malformed { .. } => None,
                };
                if let Some((index, slot)) = found {
                    out.push(DanglingBinding {
                        profile,
                        layer,
                        index,
                        slot,
                    });
                }
            }
        }
        out
    }
}

/// The references that point at an empty slot in `after` but did not in
/// `before` (the state the operation started from): what a stop part-way
/// left dangling.
pub(crate) fn dangling_after(
    profile: u8,
    before: &GroupView,
    after: &GroupView,
) -> Vec<DanglingBinding> {
    let existing = before.dangling(profile);
    after
        .dangling(profile)
        .into_iter()
        .filter(|d| !existing.contains(d))
        .collect()
}

/// The layer index of `layer` in [`GroupView::layers`].
pub(crate) fn layer_index(layer: Layer) -> usize {
    match layer {
        Layer::Base => 0,
        Layer::Fn => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct W {
        table: TableId,
        trigger: bool,
        binding: Vec<TableId>,
    }

    impl GroupWrite for W {
        fn table(&self) -> TableId {
            self.table
        }
        fn is_trigger(&self) -> bool {
            self.trigger
        }
        fn binding_tables(&self) -> &[TableId] {
            &self.binding
        }
    }

    fn w(table: TableId, trigger: bool, binding: bool) -> W {
        W {
            table,
            trigger,
            binding: if binding {
                vec![TableId::Keymap { layer: Layer::Base }]
            } else {
                Vec::new()
            },
        }
    }

    fn group() -> Vec<W> {
        let slot = AdvancedSlotId::new(0).expect("slot");
        vec![
            w(
                TableId::Macro {
                    slot: MacroSlotId::new(0).expect("slot"),
                },
                false,
                false,
            ),
            w(TableId::Keymap { layer: Layer::Base }, false, false),
            w(TableId::Keymap { layer: Layer::Fn }, true, false),
            w(TableId::AdvancedKey { slot }, false, true),
            w(TableId::AdvancedKey { slot }, false, false),
            w(TableId::MainLight, false, false),
        ]
    }

    /// Runs the cursor, cancelling at the start of write `cancel_at`.
    fn run(cancel_at: usize) -> (Vec<Step>, bool) {
        let writes = group();
        let mut cursor = GroupCursor::new(&writes);
        let mut cancel = false;
        let mut steps = Vec::new();
        for (i, write) in writes.iter().enumerate() {
            let step = cursor.before(i, write, cancel, &mut || {
                cancel = cancel || i == cancel_at;
                cancel
            });
            steps.push(step);
            if step == Step::Stop {
                break;
            }
            cursor.after(i, write, step == Step::Send);
        }
        (steps, cursor.cancelled_with_skips())
    }

    #[test]
    fn cancellation_rules() {
        use Step::{Send, Skip, Stop};
        // Before any group table: stop at once.
        assert_eq!(run(0), (vec![Stop], false));
        // After the macro slot: the base keymap is skipped, the trigger
        // sent; the binding fill is not needed (its layer was not sent).
        assert_eq!(run(1), (vec![Send, Skip, Send, Skip, Skip, Skip], true));
        // At the trigger: sent; afterwards only the binding fill.
        assert_eq!(run(2), (vec![Send, Send, Send, Send, Skip, Skip], true));
        // At the last write.
        assert_eq!(run(5), (vec![Send, Send, Send, Send, Send, Skip], true));
        // No cancellation.
        assert_eq!(run(99), (vec![Send; 6], false));
    }
}
