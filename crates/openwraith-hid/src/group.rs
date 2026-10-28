//! The keymap group's write order and its cancellation rules, shared by
//! restores and edits (one implementation).
//!
//! The keymap group (both keymap layers, the ten macro slots and the forty
//! advanced-key slots) is written in the vendor site's order: changed macro
//! slots ascending, the base keymap, the Fn keymap, changed advanced-key
//! slots ascending (spec 4.8.2). The Fn keymap is the group's commit
//! trigger: the keyboard keeps a base keymap only when an Fn-keymap write
//! follows it, and an advanced-key slot only with a complete Fn keymap in
//! the same save window (CONFIRMED on hardware: spec 4.8.2, H20 of
//! 2026-10-27), so it is written whenever the base keymap or an
//! advanced-key slot is (re-sent unchanged if need be), before the
//! advanced-key slots. Macro slots are kept without it (CONFIRMED, H19).
//!
//! [`GroupCursor`] decides, table by table, whether to send, skip or stop:
//!
//! 1. A cancellation before any group table was sent stops at once.
//! 2. Once the base keymap was sent, the trigger is due: a cancellation
//!    skips every other table up to the trigger (except those that complete
//!    a binding, see 3), sends the trigger, and takes effect after it. A
//!    sent macro slot alone does not make it due (macro slots are kept
//!    without it, H19), and the advanced-key slots come after it.
//! 3. A write that *completes a binding* (the content of a slot that a
//!    layer written in this group refers to, a layer write that removes
//!    the reference to a slot cleared in this group, or a macro slot 1-9
//!    written again after macro slot 0, see below) is sent even after a
//!    cancellation once a table it completes was sent
//!    ([`GroupWrite::binding_tables`]), so that a sent binding never stays
//!    pointed at an empty or stale slot because of a cancel. A macro slot
//!    that a written layer newly refers to is needed whenever that layer
//!    still goes out after a cancellation (the Fn keymap also whenever the
//!    base keymap does, as the trigger), so it completes whatever makes the
//!    layer go out ([`link_new_references`]). This also holds in a group
//!    without a trigger (a hardware-test mode): once a group table was
//!    sent, a cancellation skips what is not needed instead of stopping.
//! 4. A failure while the trigger is due leaves the base keymap unsaved
//!    ([`base_unsaved_on_stop`](GroupCursor::base_unsaved_on_stop)).
//!
//! [`dangling_after`] finds the references that point at empty slots after
//! an operation stopped part-way.
//!
//! # Macro slot 0
//!
//! Any write to macro slot 0 (`19 00`, empty or not) makes the keyboard
//! empty macro slots 1-9 of the profile; writes to slots 1-9 touch only
//! their own slot (CONFIRMED on hardware: H24 and `hw/v02/macro-exp.txt`,
//! 2026-10-27). The vendor site never shows this because it writes all ten
//! slots in ascending order on every save. A group that writes slot 0
//! therefore writes every slot 1-9 whose content after the group is not
//! empty right after it, in ascending order and before the keymap layers,
//! also when that content does not change (a re-send,
//! [`ResendReason::MacroSlot0Clears`]). Those writes complete slot 0
//! (rule 3): once slot 0 was sent, they are sent even after a
//! cancellation. After the operation's own slot-0 write, slots 1-9 are
//! expected to read empty (not "changed after planning"). When it stops
//! part-way, the slots left empty are reported ([`ClearedMacro`]) next to
//! the keys left referring to them ([`DanglingBinding`]).

use openwraith_protocol::{
    AdvancedSlotId, KeyPos, KeymapData, KeymapReference, Layer, MacroSlotId, TableId,
};
use serde::Serialize;

/// Why a planned write re-sends content that the plan does not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResendReason {
    /// A macro slot 1-9 written again right after macro slot 0: writing
    /// slot 0 makes the keyboard empty slots 1-9 (see the
    /// [module documentation](self)).
    MacroSlot0Clears,
}

impl ResendReason {
    /// Why, in words (for a plan's summary of the write).
    pub fn text(self) -> &'static str {
        match self {
            ResendReason::MacroSlot0Clears => {
                "writing macro slot 0 makes the keyboard clear slots 1-9"
            }
        }
    }
}

/// Whether a write of `table` makes the keyboard empty macro slots 1-9 of
/// the profile (macro slot 0, see the [module documentation](self)).
pub(crate) fn clears_other_macros(table: TableId) -> bool {
    matches!(table, TableId::Macro { slot } if slot.get() == 0)
}

/// The macro slots 1-9, ascending (the slots a slot-0 write empties).
pub(crate) fn slots_cleared_by_slot0() -> impl Iterator<Item = MacroSlotId> {
    MacroSlotId::all().filter(|slot| slot.get() != 0)
}

/// A macro slot that held content before an operation and is empty after
/// it stopped part-way: the operation wrote macro slot 0, which made the
/// keyboard empty the slot, and did not get to write it again. Keys bound
/// to it do nothing until it is written; the undo command restores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ClearedMacro {
    /// The profile.
    pub profile: u8,
    /// The slot.
    pub slot: MacroSlotId,
}

/// The macro slots that held content in `before` (the state the operation
/// started from) and are empty in `after`, among `kept`: the slots the
/// operation meant to leave with content (written with content after
/// slot 0).
pub(crate) fn cleared_macros(
    profile: u8,
    before: &GroupView,
    after: &GroupView,
    kept: &[MacroSlotId],
) -> Vec<ClearedMacro> {
    kept.iter()
        .filter(|slot| {
            let at = usize::from(slot.get());
            before.macros.get(at).copied().unwrap_or(false)
                && !after.macros.get(at).copied().unwrap_or(true)
        })
        .map(|slot| ClearedMacro {
            profile,
            slot: *slot,
        })
        .collect()
}

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

/// The base keymap: the table that makes the trigger due.
const BASE_KEYMAP: TableId = TableId::Keymap { layer: Layer::Base };

/// The macro slots that the entries a layer write changes refer to after
/// it and did not refer to before (`now`: the layer as the keyboard holds
/// it, `None` when unknown).
pub(crate) fn new_macro_references(
    now: Option<&KeymapData>,
    target: &KeymapData,
) -> Vec<MacroSlotId> {
    let before = now.map(KeymapData::references).unwrap_or_default();
    let mut slots: Vec<MacroSlotId> = Vec::new();
    for reference in target.references() {
        let KeymapReference::Macro { index, slot } = reference else {
            continue;
        };
        let unchanged = KeyPos::from_index(index)
            .is_some_and(|pos| now.is_some_and(|now| now.entry(pos) == target.entry(pos)));
        let had = before.iter().any(|r| {
            matches!(r, KeymapReference::Macro { index: i, slot: s } if *i == index && *s == slot)
        });
        if !unchanged && !had && !slots.contains(&slot) {
            slots.push(slot);
        }
    }
    slots
}

/// Rule 3 for the macro slots a written layer newly refers to (see the
/// [module documentation](self)): such a slot must go out whenever the
/// layer still does after a cancellation, so its binding tables gain every
/// table that makes the layer go out (the layer's own binding tables, for
/// the Fn keymap also the base keymap's, since the trigger follows a sent
/// base keymap) and, in turn, the tables that make those go out.
///
/// `tables`: the group's writes in order; `newly`: per write, the macro
/// slots it newly refers to ([`new_macro_references`]; empty for anything
/// but a layer write); `binding`: per write, its binding tables (extended).
pub(crate) fn link_new_references(
    tables: &[TableId],
    newly: &[Vec<MacroSlotId>],
    binding: &mut [Vec<TableId>],
) {
    let base = tables.iter().position(|t| *t == BASE_KEYMAP);
    let fn_keymap = TableId::Keymap { layer: Layer::Fn };
    // Each pass only adds tables, so this ends.
    loop {
        let mut changed = false;
        for (layer_at, slots) in newly.iter().enumerate() {
            if slots.is_empty() {
                continue;
            }
            // What makes the layer go out after a cancellation.
            let mut forcing: Vec<TableId> = binding[layer_at].clone();
            if let (true, Some(base)) = (tables[layer_at] == fn_keymap, base) {
                forcing.extend(binding[base].iter().copied());
            }
            let mut i = 0;
            while i < forcing.len() {
                if let Some(at) = tables.iter().position(|t| *t == forcing[i]) {
                    for table in &binding[at] {
                        if !forcing.contains(table) {
                            forcing.push(*table);
                        }
                    }
                }
                i += 1;
            }
            for (at, table) in tables.iter().enumerate() {
                let TableId::Macro { slot } = table else {
                    continue;
                };
                if !slots.contains(slot) {
                    continue;
                }
                for needed in &forcing {
                    if needed != table && !binding[at].contains(needed) {
                        binding[at].push(*needed);
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// The cancellation state of one profile's writes (see the
/// [module documentation](self)).
#[derive(Debug, Clone, Default)]
pub(crate) struct GroupCursor {
    trigger_at: Option<usize>,
    trigger_due: bool,
    after_trigger: bool,
    /// A group table was sent (with or without a trigger in the plan).
    group_sent: bool,
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
        // Once a group table went out, writes it needs may follow (the
        // trigger, a binding fill, the macro slots after slot 0).
        if self.trigger_due || self.after_trigger || self.group_sent {
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
            self.group_sent |= is_keymap_group(write.table());
        }
        // Only the base keymap needs the trigger after it (see the module
        // documentation, rule 2).
        let before_trigger = self.trigger_at.is_some_and(|at| index < at);
        if before_trigger && sent && write.table() == BASE_KEYMAP {
            self.trigger_due = true;
        }
        if self.trigger_at == Some(index) && sent {
            self.trigger_due = false;
            self.after_trigger = true;
        }
    }

    /// Whether the base keymap was sent and the trigger was not: a stop now
    /// leaves the base keymap unsaved.
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
        // After the macro slot: the base keymap is skipped, and so is the
        // trigger (a macro slot is kept without it, only a sent base keymap
        // needs it); the binding fill is not needed (its layer was not
        // sent).
        assert_eq!(run(1), (vec![Send, Skip, Skip, Skip, Skip, Skip], true));
        // At the trigger: sent; afterwards only the binding fill.
        assert_eq!(run(2), (vec![Send, Send, Send, Send, Skip, Skip], true));
        // At the last write.
        assert_eq!(run(5), (vec![Send, Send, Send, Send, Send, Skip], true));
        // No cancellation.
        assert_eq!(run(99), (vec![Send; 6], false));
    }

    /// Macro slot 0, a clear of slot 1 and the re-sends of slots 2 and 3
    /// (they complete slot 0), with or without the trigger after them.
    fn slot0_group(trigger: bool) -> Vec<W> {
        let table = |n: u8| TableId::Macro {
            slot: MacroSlotId::new(n).expect("slot"),
        };
        let resend = |n: u8| W {
            table: table(n),
            trigger: false,
            binding: vec![table(0)],
        };
        let mut writes = vec![w(table(0), false, false), w(table(1), false, false)];
        writes.extend([resend(2), resend(3)]);
        if trigger {
            writes.push(w(TableId::Keymap { layer: Layer::Fn }, true, false));
        }
        writes
    }

    /// Runs the cursor over `writes`, cancelling at the start of write
    /// `cancel_at`.
    fn run_writes(writes: &[W], cancel_at: usize) -> (Vec<Step>, bool) {
        let mut cursor = GroupCursor::new(writes);
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
    fn the_macro_slots_after_slot_0_are_written_even_after_a_cancellation() {
        use Step::{Send, Skip, Stop};
        for trigger in [true, false] {
            let writes = slot0_group(trigger);
            // Macro slots are kept without the trigger: a cancellation
            // leaves it out (only a sent base keymap makes it due).
            let rest = |steps: &[Step]| {
                let mut all = steps.to_vec();
                if trigger {
                    all.push(Skip);
                }
                all
            };
            // Before slot 0: nothing is sent.
            assert_eq!(run_writes(&writes, 0), (vec![Stop], false), "{trigger}");
            // After slot 0 (also without a trigger to wait for): the clear
            // is left out, the re-sends go out.
            assert_eq!(
                run_writes(&writes, 1),
                (rest(&[Send, Skip, Send, Send]), true),
                "{trigger}"
            );
            assert_eq!(
                run_writes(&writes, 3),
                (rest(&[Send, Send, Send, Send]), trigger),
                "{trigger}"
            );
        }
    }

    /// Macro slot 1 cleared, macro slot 5 written; the base keymap moves a
    /// key off slot 1 (it completes slot 1) and the Fn keymap (the trigger)
    /// binds a key to slot 5. Ctrl+C at the start of slot 5: slot 1 went
    /// out, so the base keymap does, so the trigger does; slot 5 must go out
    /// too, or the Fn keymap would bind the key to its old content.
    #[test]
    fn a_macro_slot_a_layer_newly_refers_to_goes_out_with_the_layer() {
        use Step::{Send, Skip, Stop};
        let macro_table = |n: u8| TableId::Macro {
            slot: MacroSlotId::new(n).expect("slot"),
        };
        let base = TableId::Keymap { layer: Layer::Base };
        let fn_keymap = TableId::Keymap { layer: Layer::Fn };
        let tables = [macro_table(1), macro_table(5), base, fn_keymap];
        let newly = [
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![MacroSlotId::new(5).expect("slot")],
        ];
        let mut binding = vec![Vec::new(), Vec::new(), vec![macro_table(1)], Vec::new()];
        link_new_references(&tables, &newly, &mut binding);
        assert_eq!(binding[1], [macro_table(1)]);
        let writes: Vec<W> = tables
            .iter()
            .zip(&binding)
            .map(|(table, binding)| W {
                table: *table,
                trigger: *table == fn_keymap,
                binding: binding.clone(),
            })
            .collect();
        assert_eq!(run_writes(&writes, 0), (vec![Stop], false));
        assert_eq!(run_writes(&writes, 1), (vec![Send; 4], false));
        // Without the link, slot 5 would be skipped and the trigger sent.
        let mut unlinked = writes;
        unlinked[1].binding.clear();
        assert_eq!(
            run_writes(&unlinked, 1),
            (vec![Send, Skip, Send, Send], true)
        );
    }

    #[test]
    fn new_macro_references_are_the_changed_entries_on_a_macro_slot() {
        let slot = |n: u8| MacroSlotId::new(n).expect("slot");
        let request = openwraith_protocol::ReadRequest::Keymap(Layer::Base);
        let raw = openwraith_protocol::RawResponse::from_reports(
            request,
            crate::sim::frame(request, &[0u8; 528]),
        )
        .expect("reply");
        let (_, mut now) = KeymapData::from_response(&raw).expect("keymap");
        let pos = |index: u8| KeyPos::from_index(index).expect("key");
        // Key 2 already plays slot 3; key 4 plays slot 6.
        now.set_entry(pos(2), [0x10, 0x1f, 3, 0]);
        now.set_entry(pos(4), [0x10, 0x21, 6, 0]);
        let mut target = now.clone();
        // Key 2 keeps slot 3 (another own usage), key 4 moves to slot 7,
        // key 9 newly plays slot 7, key 11 slot 0.
        target.set_entry(pos(2), [0x10, 0x20, 3, 0]);
        target.set_entry(pos(4), [0x10, 0x21, 7, 0]);
        target.set_entry(pos(9), [0x10, 0x26, 7, 0]);
        target.set_entry(pos(11), [0x10, 0x28, 0, 0]);
        assert_eq!(
            new_macro_references(Some(&now), &target),
            [slot(7), slot(0)]
        );
        // Unknown layer: every reference counts.
        assert_eq!(
            new_macro_references(None, &target),
            [slot(3), slot(7), slot(0)]
        );
    }

    #[test]
    fn cleared_macros_are_the_kept_slots_a_stop_left_empty() {
        let slot = |n: u8| MacroSlotId::new(n).expect("slot");
        let mut before = GroupView::default();
        before.macros[..5].copy_from_slice(&[true; 5]);
        let mut after = before.clone();
        // Slot 0 went out (slots 1-9 emptied), then the re-send of slot 2.
        for n in 1..10 {
            after.macros[n] = false;
        }
        after.macros[2] = true;
        // Slot 4 was meant to be cleared; slot 7 was empty before.
        let cleared = cleared_macros(3, &before, &after, &[slot(1), slot(2), slot(3), slot(7)]);
        assert_eq!(
            cleared,
            [
                ClearedMacro {
                    profile: 3,
                    slot: slot(1)
                },
                ClearedMacro {
                    profile: 3,
                    slot: slot(3)
                }
            ]
        );
    }
}
