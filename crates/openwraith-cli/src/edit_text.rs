//! Text output of the v0.2 edit plans (keymap group, colours, dead band,
//! SOCD): "now -> new", the writes in order, what is not verified on
//! hardware and what happens after the write.

use openwraith_hid::{
    ColorEditPlan, DeadBandEditPlan, EditWarning, KeymapEditPlan, PersistenceCheckMode,
    PlannedEditWrite, SocdEditPlan, persistence_check_tables,
};
use openwraith_protocol::{
    Distance, EntryKind, KeyAssignment, Layer, MediaKey, Model, MouseAction, MouseButton,
    ProfileId, SocdMode, SocdPair, TableId, WriteRequest,
};

use crate::report::{commit_trigger_text, resend_text};
use crate::table::{Table, wrap_list};

/// Wrap long lists at this width.
const WIDTH: usize = 100;

/// At most this many changed keys are listed (colours, dead band).
const MAX_ROWS: usize = 24;

/// A layer in the tables (`base`, `Fn`).
pub fn layer_text(layer: Layer) -> &'static str {
    match layer {
        Layer::Base => "base",
        Layer::Fn => "Fn",
    }
}

/// A multimedia key in words (`media Mute`).
fn media_text(key: MediaKey) -> String {
    match key.name() {
        Some(name) => format!("media {name}"),
        None => format!("media {:#04x}", key.get()),
    }
}

/// A mouse action in words. Mask `08` is the back button (X1) and `10` the
/// forward button (X2), the reverse of the vendor site's labels (hardware
/// test H18).
fn mouse_text(action: MouseAction) -> &'static str {
    match action {
        MouseAction::Button(MouseButton::Left) => "mouse left button",
        MouseAction::Button(MouseButton::Right) => "mouse right button",
        MouseAction::Button(MouseButton::Middle) => "mouse middle button",
        MouseAction::Button(MouseButton::Back) => "mouse back button (X1)",
        MouseAction::Button(MouseButton::Forward) => "mouse forward button (X2)",
        MouseAction::WheelUp => "mouse wheel up",
        MouseAction::WheelDown => "mouse wheel down",
    }
}

/// A key assignment in words.
pub fn assignment_text(assignment: &KeyAssignment) -> String {
    match assignment {
        KeyAssignment::Default => "default".to_string(),
        KeyAssignment::Key(usage) => usage.to_string(),
        KeyAssignment::Media(key) => media_text(*key),
        KeyAssignment::Mouse(action) => mouse_text(*action).to_string(),
        KeyAssignment::Combo(combo) => format!("combination {combo}"),
        KeyAssignment::Macro(slot) => format!("macro slot {}", slot.get()),
        KeyAssignment::Advanced(slot) => format!("advanced-key slot {}", slot.get()),
    }
}

/// A keymap entry in words. On the Fn layer a default entry names the
/// built-in function it keeps.
pub fn entry_text(entry: &EntryKind, layer: Layer, index: u8, model: &Model) -> String {
    match entry {
        EntryKind::Assignable(KeyAssignment::Default) if layer == Layer::Fn => {
            match model.fn_builtin(index) {
                Some(builtin) => format!("default (built-in: {})", builtin.function),
                None => "default".to_string(),
            }
        }
        EntryKind::Assignable(assignment) => assignment_text(assignment),
        EntryKind::Preserved(action) => format!("{action} (kept as read)"),
    }
}

/// A SOCD mode in words.
pub fn socd_mode_text(mode: SocdMode) -> &'static str {
    match mode {
        SocdMode::M1 => "the last input wins",
        SocdMode::M2 => "key 1 wins",
        SocdMode::M3 => "key 2 wins",
        SocdMode::M4 => "the later key only interrupts the first",
    }
}

/// A SOCD pair in words (`A / D, mode 1: the last input wins`).
pub fn socd_pair_text(pair: &SocdPair) -> String {
    format!(
        "{} / {}, mode {}: {}",
        pair.key1,
        pair.key2,
        pair.mode.number(),
        socd_mode_text(pair.mode)
    )
}

/// The name of a key for messages (`Caps Lock (#66)`).
fn key_label(model: &Model, index: u8) -> String {
    format!("{} (#{index})", model.key_name(index))
}

/// The writes of a plan in order, with their packets and why the Fn keymap
/// (or an unchanged macro slot) is written.
pub fn writes_table(writes: &[PlannedEditWrite]) -> String {
    let mut table = Table::new(&["Write", "Packets", "Why"]).right_align(&[1]);
    for write in writes {
        let mut why: Vec<String> = Vec::new();
        if let Some(reason) = write.commit_trigger {
            why.push(commit_trigger_text(reason, write.resent_unchanged));
        }
        if let Some(reason) = write.resent {
            why.push(resend_text(reason));
        }
        // A macro slot 1-9 after the plan's slot-0 write; any other write
        // that completes something (a macro slot included: one a written
        // layer newly refers to) completes a binding.
        match (write.completes_binding, write.follows_macro_slot_0()) {
            (true, true) if write.resent.is_none() => why.push(
                "written after macro slot 0, which makes the keyboard clear slots 1-9: written \
                 even after Ctrl+C"
                    .to_string(),
            ),
            // The summary of the re-send already says why.
            (true, true) => why.push("written even after Ctrl+C".to_string()),
            (true, false) => {
                why.push("completes a binding: written even after Ctrl+C".to_string());
            }
            (false, _) => {}
        }
        // The summary of a re-sent Fn keymap or macro slot already says why.
        let name = if write.resent_unchanged || write.resent.is_some() {
            write.request().to_string()
        } else {
            write.summary.clone()
        };
        table.row(vec![name, write.packets.to_string(), why.join("; ")]);
    }
    table.render("  ")
}

/// The lowest profile other than `profile` (where the persistence check
/// switches to).
pub fn away_profile(profile: ProfileId) -> ProfileId {
    ProfileId::ALL
        .into_iter()
        .find(|p| *p != profile)
        .unwrap_or(profile)
}

/// What happens after the write: the commit wait and the persistence check
/// of `mode` (`forced` by the plan), which reads `checked` again
/// ([`persistence_check_tables`]; empty: no profile switch).
pub fn after_write_text(
    mode: PersistenceCheckMode,
    profile: ProfileId,
    forced: bool,
    checked: &[TableId],
) -> String {
    let away = away_profile(profile);
    let check = match (forced, mode) {
        (true, _) | (false, PersistenceCheckMode::Always) => format!(
            "then switches to profile {away} and back and reads every written table again, to \
             check that the keyboard kept it (the lighting changes briefly)"
        ),
        (false, PersistenceCheckMode::Auto) if checked.is_empty() => {
            "and does not switch profiles: hardware tests showed that the keyboard keeps these \
             tables (--check-persistence always checks anyway)"
                .to_string()
        }
        (false, PersistenceCheckMode::Auto) => {
            let tables: Vec<String> = checked.iter().map(ToString::to_string).collect();
            format!(
                "then switches to profile {away} and back and reads {} again, whose keeping is \
                 not proven on hardware yet (the lighting changes briefly; --check-persistence)",
                tables.join(", ")
            )
        }
        (false, PersistenceCheckMode::Never) => {
            "and does not switch profiles to check it (--check-persistence never)".to_string()
        }
    };
    format!(
        "After writing, OpenWraith waits 2 s for the keyboard to save (do not switch profiles or \
         unplug it meanwhile), {check}.\n"
    )
}

/// The plan's warnings in words, as a "Notes" block (empty without any).
pub fn warnings_text(warnings: &[EditWarning], model: Option<&Model>) -> String {
    if warnings.is_empty() {
        return String::new();
    }
    let key = |index: u8| match model {
        Some(model) => key_label(model, index),
        None => format!("#{index}"),
    };
    let mut out = String::from("Notes:\n");
    for warning in warnings {
        let text = match warning {
            EditWarning::FnBuiltinOverridden {
                index,
                function,
                critical,
            } => {
                let mut text = format!(
                    "Fn + {} will no longer do its built-in function \"{function}\" while this \
                     profile is active (--allow-builtin)",
                    key(*index)
                );
                if *critical {
                    text.push_str(
                        "; a CRITICAL one: profile switches or the reset on the keyboard depend \
                         on it (--allow-critical-builtin). Setting the entry back to default \
                         brings it back",
                    );
                }
                text
            }
            EditWarning::Unverified(note) => format!(
                "not yet verified on a W75: {} ({}). The vendor site sends the same bytes",
                note.what, note.question
            ),
            EditWarning::SocdPowerAllProfiles => {
                "the SOCD power is one setting of the whole keyboard: switching it changes every \
                 profile (the pairs belong to the active profile)"
                    .to_string()
            }
            EditWarning::MalformedReferencePreserved { layer, index } => format!(
                "{} on the {} layer holds a malformed macro or advanced-key entry; it is kept as \
                 read",
                key(*index),
                layer_text(*layer)
            ),
            EditWarning::DanglingReferencePreserved { layer, index, slot } => format!(
                "{} on the {} layer already refers to the empty {slot}; it is kept as read",
                key(*index),
                layer_text(*layer)
            ),
            EditWarning::TransientBinding { slot } => format!(
                "in the vendor site's order the keymap is written before {slot}, so for a moment \
                 the key uses the slot's old content; the slot is written even after Ctrl+C"
            ),
            EditWarning::OrphanSlotKept { slot } => format!(
                "{slot} keeps its content although no key refers to it any more (--keep-slot, or \
                 content an undo could not write back)"
            ),
            EditWarning::NotShownUntilCustomMode => {
                "the keyboard shows custom colour table 1 only in the Custom light mode; add \
                 --custom-mode to switch to it"
                    .to_string()
            }
            EditWarning::SocdHotkey => {
                "Fn + Right Shift on the keyboard also switches SOCD on and off".to_string()
            }
            EditWarning::NotUndoable { table } => format!(
                "{table} holds content in a form OpenWraith does not write: the undo command \
                 cannot bring it back"
            ),
        };
        out.push_str(&format!("  - {text}.\n"));
    }
    out
}

/// Text of a keymap-group plan.
pub fn keymap_plan(plan: &KeymapEditPlan, model: &Model, mode: PersistenceCheckMode) -> String {
    let mut out = format!("Keymap group of profile {}:\n", plan.profile());
    if plan.entry_changes().is_empty() {
        out.push_str("  No key assignment changes.\n");
    } else {
        let mut table = Table::new(&["Layer", "Key", "Now", "New"]);
        for change in plan.entry_changes() {
            table.row(vec![
                layer_text(change.layer).to_string(),
                change.name.clone(),
                entry_text(&change.before, change.layer, change.index, model),
                entry_text(&change.after, change.layer, change.index, model),
            ]);
        }
        out.push_str(&table.render("  "));
    }
    if !plan.slot_changes().is_empty() {
        out.push_str("Slots:\n");
        for change in plan.slot_changes() {
            let text =
                |content: &Option<String>| content.clone().unwrap_or_else(|| "empty".to_string());
            out.push_str(&format!(
                "  {}: {} -> {}\n",
                change.slot,
                text(&change.before),
                text(&change.after)
            ));
        }
    }
    if plan.has_changes() {
        out.push_str("Writes, in this order (the vendor site's order):\n");
        out.push_str(&writes_table(plan.writes()));
    }
    if plan.forces_persistence_check() {
        out.push_str(
            "--commit-trigger skip (hardware test): the Fn keymap (18 02) is NOT written before \
             the advanced-key slots, as the vendor site would (without it the keyboard does not \
             keep them; macro slots are kept); the persistence check always runs.\n",
        );
    }
    out.push_str(&warnings_text(plan.warnings(), Some(model)));
    if plan.has_changes() {
        out.push_str(&after_write_text(
            mode,
            plan.profile(),
            plan.forces_persistence_check(),
            &persistence_check_tables(plan.writes(), mode),
        ));
    }
    out
}

/// Text of a colour plan.
pub fn colors_plan(plan: &ColorEditPlan, model: &Model, mode: PersistenceCheckMode) -> String {
    let mut out = format!("Custom colour table 1 of profile {}:\n", plan.profile());
    if plan.changes().is_empty() {
        out.push_str("  Every key already has its colour.\n");
    } else {
        let mut table = Table::new(&["Key", "Now", "New"]);
        for change in plan.changes().iter().take(MAX_ROWS) {
            table.row(vec![
                model.key_name(change.pos.index()),
                change.before.to_string(),
                change.after.to_string(),
            ]);
        }
        out.push_str(&table.render("  "));
        if plan.changes().len() > MAX_ROWS {
            out.push_str(&format!(
                "  ... and {} more keys\n",
                plan.changes().len() - MAX_ROWS
            ));
        }
        out.push_str(
            "  Every other slot of the table (the knob and the positions outside the layout, \
             such as #103 and #126) keeps its bytes.\n",
        );
    }
    let switches = plan.writes().iter().any(|w| w.table == TableId::MainLight);
    if plan.shown_now() {
        out.push_str("  The main light is in the Custom mode: the colours show at once.\n");
    } else if switches {
        out.push_str(
            "  The main light switches to the Custom mode (foreground colour, brightness and \
             speed kept), in which the keyboard shows these colours.\n",
        );
    }
    if plan.has_changes() {
        out.push_str("Writes, in this order:\n");
        out.push_str(&writes_table(plan.writes()));
    }
    out.push_str(&warnings_text(plan.warnings(), Some(model)));
    if plan.has_changes() {
        out.push_str(&after_write_text(
            mode,
            plan.profile(),
            false,
            &persistence_check_tables(plan.writes(), mode),
        ));
    }
    out
}

/// A dead-band entry `[top, bottom]` in words.
fn dead_band_text(entry: [u8; 2]) -> String {
    format!(
        "top {}, bottom {}",
        Distance::from(entry[0]),
        Distance::from(entry[1])
    )
}

/// Text of a dead-band plan.
pub fn dead_band_plan(
    plan: &DeadBandEditPlan,
    model: &Model,
    mode: PersistenceCheckMode,
) -> String {
    let mut out = format!("Dead band (profile {} active):\n", plan.profile());
    if plan.changes().is_empty() {
        out.push_str("  Every key already has these dead zones.\n");
    } else {
        let mut table = Table::new(&["Key", "Now", "New"]);
        for change in plan.changes().iter().take(MAX_ROWS) {
            table.row(vec![
                model.key_name(change.pos.index()),
                dead_band_text(change.before),
                dead_band_text(change.after),
            ]);
        }
        out.push_str(&table.render("  "));
        if plan.changes().len() > MAX_ROWS {
            out.push_str(&format!(
                "  ... and {} more keys\n",
                plan.changes().len() - MAX_ROWS
            ));
        }
        out.push_str(
            "  The whole table is written; the keys not listed keep their values. (The \
             top/bottom order comes from the vendor site's code.)\n",
        );
        out.push_str("Writes:\n");
        out.push_str(&writes_table(plan.writes()));
    }
    out.push_str(&warnings_text(plan.warnings(), Some(model)));
    if plan.has_changes() {
        out.push_str(&after_write_text(
            mode,
            plan.profile(),
            false,
            &persistence_check_tables(plan.writes(), mode),
        ));
    }
    out
}

/// The pairs in words, one per line (`(none)` without any).
fn pairs_text(pairs: &[SocdPair], indent: &str) -> String {
    if pairs.is_empty() {
        return format!("{indent}(none)\n");
    }
    pairs
        .iter()
        .enumerate()
        .map(|(i, pair)| format!("{indent}{}. {}\n", i + 1, socd_pair_text(pair)))
        .collect()
}

/// Text of a SOCD plan.
pub fn socd_plan(plan: &SocdEditPlan, mode: PersistenceCheckMode) -> String {
    let current = plan.current();
    let power_now = match current.power {
        0 => "off".to_string(),
        1 => "on".to_string(),
        other => format!("unknown ({other:#04x})"),
    };
    let power_new = if plan.power() { "on" } else { "off" };
    let mut out = format!(
        "SOCD (profile {} active):\n  Power (all profiles)  {power_now} -> {power_new}\n",
        plan.profile()
    );
    out.push_str(&format!("  Pairs of profile {} now:\n", plan.profile()));
    match (&current.pairs, current.unwritable) {
        (Some(pairs), _) => out.push_str(&pairs_text(pairs, "    ")),
        (None, reason) => out.push_str(&format!(
            "    not in the form OpenWraith writes ({}): {}\n",
            reason.unwrap_or("unreadable"),
            current.raw_pairs
        )),
    }
    out.push_str(&format!("  Pairs of profile {} after:\n", plan.profile()));
    out.push_str(&pairs_text(plan.pairs(), "    "));
    if plan.has_changes() {
        out.push_str(
            "Writes (the vendor site's burst: power, then both pair chunks, also when only one \
             of them changes):\n",
        );
        out.push_str(&writes_table(plan.writes()));
    }
    out.push_str(&warnings_text(plan.warnings(), None));
    if plan.has_changes() {
        out.push_str(&after_write_text(
            mode,
            plan.profile(),
            false,
            &persistence_check_tables(plan.writes(), mode),
        ));
    }
    out
}

/// The hex of every packet of `writes`, grouped per write (`--packets`).
pub fn packets_text(writes: &[(String, WriteRequest)]) -> String {
    let mut out = String::from("Packets:\n");
    for (name, request) in writes {
        out.push_str(&format!("  {name}:\n"));
        for step in request.steps() {
            out.push_str(&format!("    {}\n", step.packet));
        }
    }
    out
}

/// The built-in Fn functions of `model` (for `show keymap`).
pub fn fn_builtins_text(model: &Model) -> String {
    if model.fn_builtins.is_empty() {
        return String::new();
    }
    let items: Vec<String> = model
        .fn_builtins
        .iter()
        .map(|b| {
            format!(
                "{} {}{}",
                model.key_name(b.index),
                b.function,
                if b.critical { " (critical)" } else { "" }
            )
        })
        .collect();
    let checked: Vec<String> = model
        .fn_builtins
        .iter()
        .filter(|b| b.confirmed)
        .map(|b| model.key_name(b.index))
        .collect();
    let checked = if checked.is_empty() {
        String::new()
    } else {
        format!(", checked on hardware for {}", checked.join(", "))
    };
    let mut out = format!(
        "  Built-in Fn functions (Fn + key; replaced by an Fn-layer entry other than default; \
         labels from the vendor site{checked}):\n",
    );
    out.push_str(&wrap_list(&items, "    ", WIDTH));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwraith_protocol::{
        AdvancedSlotId, HidUsage, KeyAction, MacroSlotId, UnverifiedNote, W75,
    };

    #[test]
    fn assignments_and_entries_in_words() {
        let usage = |u| HidUsage::new(u).expect("usage");
        assert_eq!(assignment_text(&KeyAssignment::Default), "default");
        assert_eq!(assignment_text(&KeyAssignment::Key(usage(0xE0))), "L-Ctrl");
        assert_eq!(
            assignment_text(&KeyAssignment::Media(MediaKey::new(0x12).expect("mute"))),
            "media Mute"
        );
        assert_eq!(
            assignment_text(&KeyAssignment::Mouse(MouseAction::Button(
                MouseButton::Forward
            ))),
            "mouse forward button (X2)"
        );
        assert_eq!(
            assignment_text(&KeyAssignment::Mouse(MouseAction::Button(
                MouseButton::Back
            ))),
            "mouse back button (X1)"
        );
        assert_eq!(
            assignment_text(&KeyAssignment::Media(MediaKey::new(0x24).expect("back"))),
            "media Browser Back"
        );
        assert_eq!(
            assignment_text(&KeyAssignment::Macro(MacroSlotId::new(2).expect("slot"))),
            "macro slot 2"
        );
        assert_eq!(
            assignment_text(&KeyAssignment::Advanced(
                AdvancedSlotId::new(7).expect("slot")
            )),
            "advanced-key slot 7"
        );
        // F1 (index 2) on the Fn layer keeps its built-in function.
        let default = EntryKind::Assignable(KeyAssignment::Default);
        assert_eq!(
            entry_text(&default, Layer::Fn, 2, &W75),
            "default (built-in: media player)"
        );
        assert_eq!(entry_text(&default, Layer::Base, 2, &W75), "default");
        let fn_entry = EntryKind::Preserved(KeyAction::Fn { code: 0 });
        assert_eq!(
            entry_text(&fn_entry, Layer::Base, 121, &W75),
            "Fn (kept as read)"
        );
    }

    #[test]
    fn warnings_name_keys_and_open_questions() {
        let text = warnings_text(
            &[
                EditWarning::FnBuiltinOverridden {
                    index: 24,
                    function: "profile 2",
                    critical: true,
                },
                EditWarning::Unverified(UnverifiedNote::new(
                    "Mod-Tap hold time other than 300 ms",
                    "Q19",
                )),
                EditWarning::SocdHotkey,
            ],
            Some(&W75),
        );
        assert!(
            text.contains("Fn + 2 (#24) will no longer do its built-in function \"profile 2\""),
            "{text}"
        );
        assert!(text.contains("CRITICAL"), "{text}");
        assert!(
            text.contains("not yet verified on a W75: Mod-Tap hold time other than 300 ms (Q19)"),
            "{text}"
        );
        assert!(text.contains("Fn + Right Shift"), "{text}");
        assert_eq!(warnings_text(&[], None), "");
    }

    #[test]
    fn the_persistence_check_is_announced() {
        let p1 = ProfileId::new(1).expect("profile");
        // The only table not known to be kept: an advanced-key slot without
        // the Fn keymap before it.
        let slot = TableId::AdvancedKey {
            slot: AdvancedSlotId::new(0).expect("slot"),
        };
        let auto = after_write_text(PersistenceCheckMode::Auto, p1, false, &[slot]);
        assert!(auto.contains("waits 2 s"), "{auto}");
        assert!(
            auto.contains("switches to profile 2 and back and reads advanced-key slot 0 again"),
            "{auto}"
        );
        // Tables known to be kept: no switch is announced.
        let kept = after_write_text(PersistenceCheckMode::Auto, p1, false, &[]);
        assert!(kept.contains("does not switch profiles"), "{kept}");
        assert!(!kept.contains("switches to profile"), "{kept}");
        let never = after_write_text(PersistenceCheckMode::Never, p1, false, &[]);
        assert!(never.contains("does not switch profiles"), "{never}");
        let forced = after_write_text(PersistenceCheckMode::Never, p1, true, &[]);
        assert!(forced.contains("every written table"), "{forced}");
        assert_eq!(away_profile(ProfileId::new(2).expect("p")).get(), 1);
        let builtins = fn_builtins_text(&W75);
        assert!(builtins.contains("Space reset (critical)"), "{builtins}");
        // H18 checked F1-F4; H12 and A9 Fn + 1 and Fn + 2; c15 and H12 Right Shift.
        assert!(
            builtins.contains("checked on hardware for F1, F2, F3, F4, 1, 2, R-Shift"),
            "{builtins}"
        );
    }
}
