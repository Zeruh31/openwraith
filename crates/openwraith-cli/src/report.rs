//! Text output of the write path: restore plans and reports, edit plans and
//! edit reports.

use std::collections::BTreeMap;

use openwraith_hid::{
    BlockCause, CommitTriggerReason, DanglingBinding, EditReport, EntryChange, ImportSelection,
    ItemReport, ItemResult, LightEditPlan, Outcome, Persistence, PersistenceCheck,
    PersistenceCheckMode, PlanAction, PlanItem, PlanWarning, PlannedWrite, RestorePlan,
    RestoreReport, SkipReason, TriggerEditPlan, WriteWarning,
};
use openwraith_protocol::{
    AdvancedSlotData, Distance, HidUsage, KeyAction, LightMode, LightRecord, Model, ProfileId,
    ReadRequest, Rgb, SocdMode, TableId, TriggerSetting, TriggerWriteMode,
};

use crate::table::{Table, wrap_list};

/// Wrap long lists at this width.
const WIDTH: usize = 100;

/// At most this many changed entries are listed per table.
const MAX_CHANGES: usize = 6;

/// `"1 packet"`, `"10 packets"`.
fn count(n: usize, what: &str) -> String {
    if n == 1 {
        format!("{n} {what}")
    } else {
        format!("{n} {what}s")
    }
}

/// A duration in milliseconds as seconds for the user, rounded up to a
/// tenth: `2 s`, `1.5 s`.
pub fn seconds_text(ms: u64) -> String {
    let tenths = ms.saturating_add(99) / 100;
    if tenths % 10 == 0 {
        format!("{} s", tenths / 10)
    } else {
        format!("{}.{} s", tenths / 10, tenths % 10)
    }
}

/// The note shown while a write operation waits for the keyboard to save.
pub fn commit_wait_text(profile: Option<u8>, wait_ms: u64) -> String {
    let what = profile.map_or_else(String::new, |p| format!(" profile {p}"));
    format!(
        "Waiting for the keyboard to save{what} ({})...",
        seconds_text(wait_ms)
    )
}

/// `1, 2, 3` -> `"1, 2 and 3"`.
fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// A trigger record `MM A A P R Ah Ah Ph Rh` in words.
pub fn trigger_bytes_text(bytes: &[u8]) -> String {
    if bytes.len() < 9 {
        return format!("raw {}", spaced(bytes));
    }
    let actuation = Distance::from_lo_hi(bytes[1], bytes[5]);
    let press = Distance::from_lo_hi(bytes[3], bytes[7]);
    let release = Distance::from_lo_hi(bytes[4], bytes[8]);
    let second = Distance::from_lo_hi(bytes[2], bytes[6]);
    let mut text = match bytes[0] {
        0x00 => format!("{actuation}, RT off"),
        0x0C => format!("{actuation}, RT {press}"),
        0x0D => format!("{actuation}, RT {press} / {release}"),
        mode => format!("{actuation}, mode {mode:#04x}"),
    };
    if second != actuation {
        text.push_str(&format!(" (second travel {second})"));
    }
    text
}

/// Trigger settings in words (`1.20 mm, RT 0.30 mm`).
pub fn trigger_text(setting: &TriggerSetting) -> String {
    match setting.mode {
        TriggerWriteMode::Off => format!("{}, RT off", setting.actuation),
        TriggerWriteMode::RapidTrigger => {
            format!("{}, RT {}", setting.actuation, setting.rt_press)
        }
        TriggerWriteMode::RapidTriggerSeparate => format!(
            "{}, RT {} / {}",
            setting.actuation, setting.rt_press, setting.rt_release
        ),
    }
}

/// A 12-byte main-light record in words.
pub fn light_bytes_text(bytes: &[u8]) -> String {
    if bytes.len() < 12 {
        return format!("raw {}", spaced(bytes));
    }
    let mode = LightMode::from_id(bytes[0]);
    let colour = Rgb::new(bytes[3], bytes[4], bytes[5]);
    let mut text = format!(
        "{mode}, colour {colour}, brightness {}, speed {}",
        bytes[1], bytes[2]
    );
    // Power byte: 0 = lighting on, 1 = off (CODE-ONLY).
    if bytes[11] != 0 {
        text.push_str(", lighting switched off");
    }
    text
}

/// A light record in words.
pub fn light_text(record: &LightRecord) -> String {
    light_bytes_text(record.as_bytes())
}

fn spaced(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn key_label(model: Option<&Model>, index: usize) -> String {
    let index = u8::try_from(index).unwrap_or(u8::MAX);
    match model {
        Some(model) => format!("{} (#{index})", model.key_name(index)),
        None => format!("#{index}"),
    }
}

/// The contents of a macro slot in words.
fn macro_text(bytes: &[u8]) -> String {
    match bytes {
        [] => "empty".to_string(),
        [_, _, _, n, ..] if *n >= 4 && n % 4 == 0 => format!("{} step(s)", n / 4),
        _ => "in use".to_string(),
    }
}

/// The contents of an advanced-key slot in words.
fn advanced_text(bytes: &[u8]) -> String {
    if bytes.iter().all(|b| *b == 0) {
        return "empty".to_string();
    }
    match AdvancedSlotData::from_read_slot(bytes) {
        Ok(Some(data)) => match data.def() {
            Some(def) => def.to_string(),
            None => format!("in use (type {})", data.type_byte()),
        },
        _ => format!("in use (type {})", bytes.first().copied().unwrap_or(0)),
    }
}

/// One SOCD pair entry (`01 mode key1 key2`) in words.
fn socd_entry_text(bytes: &[u8]) -> String {
    let key = |u: u8| HidUsage::new(u).map_or_else(|| format!("{u:#04x}"), |u| u.to_string());
    match *bytes {
        [] | [0, 0, 0, 0] => "no pair".to_string(),
        [0x01, mode, key1, key2] => match SocdMode::from_wire(mode) {
            Some(mode) => format!("{} / {} mode {}", key(key1), key(key2), mode.number()),
            None => format!("{} / {} mode {mode:#04x}", key(key1), key(key2)),
        },
        _ => spaced(bytes),
    }
}

/// One changed entry of a SOCD write: index 0 is the power, 1-20 the
/// pairs.
fn socd_change_text(change: &EntryChange) -> String {
    if change.index == 0 {
        let power = |b: &[u8]| match b {
            [0] => "off".to_string(),
            [1] => "on".to_string(),
            other => spaced(other),
        };
        return format!("power {} -> {}", power(&change.old), power(&change.new));
    }
    format!(
        "pair {}: {} -> {}",
        change.index,
        socd_entry_text(&change.old),
        socd_entry_text(&change.new)
    )
}

/// One changed entry of a planned write, in words.
fn change_text(table: TableId, change: &EntryChange, model: Option<&Model>) -> String {
    let (old, new) = (&change.old, &change.new);
    match table {
        TableId::Triggers => format!("{} -> {}", trigger_bytes_text(old), trigger_bytes_text(new)),
        TableId::MainLight => format!("{} -> {}", light_bytes_text(old), light_bytes_text(new)),
        TableId::Keymap { .. } => format!(
            "{}: {} -> {}",
            key_label(model, change.index),
            keymap_entry_text(old),
            keymap_entry_text(new)
        ),
        TableId::DeadBand => {
            let pair = |b: &[u8]| match b {
                [top, bottom] => format!(
                    "top {}, bottom {}",
                    Distance::from(*top),
                    Distance::from(*bottom)
                ),
                other => spaced(other),
            };
            format!(
                "{}: {} -> {}",
                key_label(model, change.index),
                pair(old),
                pair(new)
            )
        }
        TableId::CustomColors { .. } => {
            let colour = |b: &[u8]| match b {
                [r, g, b] => Rgb::new(*r, *g, *b).to_string(),
                other => spaced(other),
            };
            format!(
                "{}: {} -> {}",
                key_label(model, change.index),
                colour(old),
                colour(new)
            )
        }
        TableId::Macro { .. } => format!("{} -> {}", macro_text(old), macro_text(new)),
        TableId::AdvancedKey { .. } => format!("{} -> {}", advanced_text(old), advanced_text(new)),
        TableId::Socd => socd_change_text(change),
        _ => format!("{} -> {}", spaced(old), spaced(new)),
    }
}

/// A keymap entry in words, with its bytes: `A (00 04 00 00)`.
fn keymap_entry_text(bytes: &[u8]) -> String {
    match <[u8; 4]>::try_from(bytes) {
        Ok(entry) => format!("{} ({})", KeyAction::decode(entry), spaced(bytes)),
        Err(_) => spaced(bytes),
    }
}

/// The changed entries of a planned write, at most [`MAX_CHANGES`] unless
/// `all` (an import lists every keymap entry it writes).
fn changes_text(
    table: TableId,
    write: &PlannedWrite,
    model: Option<&Model>,
    all: bool,
) -> Vec<String> {
    let limit = if all { usize::MAX } else { MAX_CHANGES };
    let mut lines: Vec<String> = write
        .changes
        .iter()
        .take(limit)
        .map(|c| change_text(table, c, model))
        .collect();
    if write.changes.len() > limit {
        lines.push(format!(
            "... and {} more entries",
            write.changes.len() - limit
        ));
    }
    lines
}

/// A planned item's name: the table, with the key for per-key items.
fn item_name(item: &PlanItem, model: Option<&Model>) -> String {
    match (item.table, item.key) {
        (TableId::Triggers, Some(pos)) => {
            let name = model.map_or_else(|| pos.to_string(), |m| m.key_name(pos.index()));
            format!("trigger of {name} ({pos})")
        }
        (table, _) => table.to_string(),
    }
}

/// Table names with consecutive macro and advanced-key slots merged
/// (`macro slots 1-9`).
fn table_names(tables: &[TableId]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut macros: Vec<u8> = Vec::new();
    let mut advanced: Vec<u8> = Vec::new();
    for table in tables {
        match table {
            TableId::Macro { slot } => macros.push(slot.get()),
            TableId::AdvancedKey { slot } => advanced.push(slot.get()),
            other => out.push(other.to_string()),
        }
    }
    let ranges = |what: &str, slots: &[u8]| -> Option<String> {
        if slots.is_empty() {
            return None;
        }
        let mut parts: Vec<String> = Vec::new();
        let mut start = slots[0];
        let mut prev = slots[0];
        for &s in &slots[1..] {
            if s == prev + 1 {
                prev = s;
                continue;
            }
            parts.push(range_text(start, prev));
            start = s;
            prev = s;
        }
        parts.push(range_text(start, prev));
        let noun = if slots.len() == 1 { "slot" } else { "slots" };
        Some(format!("{what} {noun} {}", parts.join(", ")))
    };
    out.extend(ranges("macro", &macros));
    out.extend(ranges("advanced-key", &advanced));
    out
}

fn range_text(start: u8, end: u8) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    }
}

/// Text of a restore plan.
///
/// `dry_run` adds the explanation for profiles that were not compared;
/// `packets` lists the hex of every packet.
pub fn restore_plan(
    plan: &RestorePlan,
    model: Option<&Model>,
    dry_run: bool,
    packets: bool,
) -> String {
    let import = plan
        .warnings
        .iter()
        .any(|w| matches!(w, PlanWarning::Imported { .. }));
    let what = if import {
        "Import plan"
    } else {
        "Restore plan"
    };
    let mut out = if dry_run {
        format!("{what} (dry run: nothing is written)\n")
    } else {
        format!("{what}\n")
    };
    let firmware = |f: &Option<String>| f.clone().unwrap_or_else(|| "unknown".into());
    out.push_str(&format!(
        "  Backup    made {}, firmware {}\n",
        plan.backup_created_at,
        firmware(&plan.backup_firmware)
    ));
    out.push_str(&format!(
        "  Keyboard  {}, firmware {}, read {}, active profile {}\n",
        plan.product.clone().unwrap_or_else(|| "unknown".into()),
        firmware(&plan.keyboard_firmware),
        plan.keyboard_read_at,
        plan.original_profile
            .map_or_else(|| "unknown".to_string(), |p| p.to_string())
    ));

    let mut not_compared: Vec<String> = Vec::new();
    for profile in &plan.profiles {
        if !profile.compared {
            not_compared.push(profile.profile.to_string());
            continue;
        }
        let active = if plan.original_profile == Some(profile.profile.get()) {
            " (active)"
        } else {
            ""
        };
        out.push_str(&format!("\nProfile {}{active}\n", profile.profile));
        let mut unchanged: Vec<TableId> = Vec::new();
        let mut missing: Vec<TableId> = Vec::new();
        let mut writes = Table::new(&["Write", "Packets", "Change"]).right_align(&[1]);
        let mut skips: Vec<String> = Vec::new();
        for item in &profile.items {
            match &item.action {
                PlanAction::Unchanged => unchanged.push(item.table),
                PlanAction::NotInBackup => missing.push(item.table),
                PlanAction::Write(write) => {
                    // An import lists every keymap entry it takes from the file.
                    let all = import && matches!(item.table, TableId::Keymap { .. });
                    let mut changes = changes_text(item.table, write, model, all);
                    if let Some(reason) = write.commit_trigger {
                        changes.push(commit_trigger_text(reason, write.is_resent_unchanged()));
                    }
                    let mut first = true;
                    for line in changes.iter().map(String::as_str).chain(
                        // A write without listed changes still gets a row.
                        changes.is_empty().then_some(""),
                    ) {
                        if first {
                            let what = match item.key {
                                Some(_) => item_name(item, model),
                                // The reason is in the change column.
                                None if write.commit_trigger.is_some() => write.request.to_string(),
                                None => write.summary.clone(),
                            };
                            writes.row(vec![what, write.packets.to_string(), line.to_string()]);
                            first = false;
                        } else {
                            writes.row(vec![String::new(), String::new(), line.to_string()]);
                        }
                    }
                    if packets {
                        for step in write.request.steps() {
                            writes.row(vec![
                                String::new(),
                                String::new(),
                                format!("  {}", step.packet),
                            ]);
                        }
                    }
                }
                PlanAction::Skip { message, .. } => {
                    skips.push(format!("{}: {message}", item_name(item, model)));
                }
            }
        }
        let write_count = profile
            .items
            .iter()
            .filter(|i| matches!(i.action, PlanAction::Write(_)))
            .count();
        if write_count == 0 && skips.is_empty() {
            out.push_str("  Nothing to write: the keyboard already holds the backup's values.\n");
        } else if write_count == 0 {
            out.push_str("  Nothing that can be written differs.\n");
        } else {
            out.push_str(&writes.render("  "));
        }
        if !skips.is_empty() {
            out.push_str("  Not written:\n");
            for skip in &skips {
                out.push_str(&format!("    - {skip}\n"));
            }
        }
        if let Some(index) = critical_fn_block(&profile.items) {
            out.push_str(&format!(
                "  To import the keys anyway, add --allow-critical-fn-overrides (Fn + {} will \
                 then no longer do its built-in function, a profile switch or the reset, while \
                 this profile is active).\n",
                key_label(model, usize::from(index))
            ));
        }
        if !unchanged.is_empty() {
            out.push_str("  Unchanged (the keyboard already holds the backup's values):\n");
            out.push_str(&wrap_list(&table_names(&unchanged), "    ", WIDTH));
        }
        if !missing.is_empty() {
            out.push_str("  Not in the backup (not compared):\n");
            out.push_str(&wrap_list(&table_names(&missing), "    ", WIDTH));
        }
    }

    if !not_compared.is_empty() {
        let noun = if not_compared.len() == 1 {
            "Profile"
        } else {
            "Profiles"
        };
        out.push_str(&format!("\n{noun} {}: not compared. ", list(&not_compared)));
        if dry_run {
            out.push_str(
                "A dry run reads only the active profile; a real run shows the full plan before \
                 changing anything, and only profile switches happen before you answer.\n",
            );
        } else {
            out.push_str("The keyboard's state of these profiles is not known.\n");
        }
    }

    if !plan.warnings.is_empty() {
        out.push_str("\nWarnings\n");
        for warning in &plan.warnings {
            out.push_str(&format!("  - {}\n", warning_text(warning, model)));
        }
    }

    let profiles: Vec<String> = plan
        .profiles
        .iter()
        .filter(|p| {
            p.items
                .iter()
                .any(|i| matches!(i.action, PlanAction::Write(_)))
        })
        .map(|p| p.profile.to_string())
        .collect();
    let skipped = plan.skips().count();
    let compared = plan.profiles.iter().filter(|p| p.compared).count();
    out.push('\n');
    if plan.has_writes() {
        out.push_str(&format!(
            "Total: {} ({}) in profile {}",
            count(plan.write_count(), "write"),
            count(plan.packet_count(), "packet"),
            list(&profiles)
        ));
    } else if compared == 0 {
        out.push_str("Total: nothing compared");
        if dry_run {
            out.push_str(" (dry run)");
        }
    } else if !not_compared.is_empty() {
        out.push_str(&format!(
            "Total: nothing to write in the compared profile(s); profile(s) {} not compared",
            list(&not_compared)
        ));
    } else {
        out.push_str("Total: nothing to write");
    }
    if skipped > 0 {
        out.push_str(&format!(
            "; {} not written (see above)",
            count(skipped, "table")
        ));
    }
    out.push_str(".\n");
    out
}

/// The key index of a critical built-in Fn function that blocks the keymap
/// group of an import, if one does.
fn critical_fn_block(items: &[PlanItem]) -> Option<u8> {
    items.iter().find_map(|item| match &item.action {
        PlanAction::Skip {
            reason:
                SkipReason::KeymapGroupBlocked {
                    cause: BlockCause::CriticalFnOverride { index },
                },
            ..
        } => Some(*index),
        _ => None,
    })
}

/// Whether an import's plan leaves out a category the user chose to
/// import: the keymap group blocked as a whole, or SOCD pairs it cannot
/// write. (A category that is merely unchanged is not left out.)
pub fn import_blocked(plan: &RestorePlan, include: &ImportSelection) -> bool {
    plan.skips().any(|(_, item)| {
        include.includes(item.table)
            && matches!(
                &item.action,
                PlanAction::Skip {
                    reason: SkipReason::KeymapGroupBlocked { .. }
                        | SkipReason::UnwritableSocdPairs { .. },
                    ..
                }
            )
    })
}

/// A plan warning in words.
fn warning_text(warning: &PlanWarning, model: Option<&Model>) -> String {
    let layer_name = |layer: &openwraith_protocol::Layer| match layer {
        openwraith_protocol::Layer::Base => "base",
        openwraith_protocol::Layer::Fn => "Fn",
    };
    match warning {
        PlanWarning::FirmwareDiffers { backup, keyboard } => format!(
            "the backup was made with firmware {}, the keyboard runs {} (allowed with \
             --allow-firmware-mismatch); table formats may differ",
            backup.as_deref().unwrap_or("unknown"),
            keyboard.as_deref().unwrap_or("unknown")
        ),
        PlanWarning::ConstantDiffers {
            request: ReadRequest::TriggerGlobal,
        } => "the trigger parameters (21/04) differ between the backup and the keyboard (they \
              are never written); stored trigger and dead-band values are only restored when \
              both use 0.02 mm units"
            .to_string(),
        PlanWarning::ConstantDiffers { request } => format!(
            "the device constant {request} differs between the backup and the keyboard (it is \
             never written)"
        ),
        PlanWarning::CapabilityFlagsDiffer => {
            "the capability flags (status bytes 17-18) differ between the backup and the \
             keyboard; they include the distance unit (high precision), so trigger and \
             dead-band values are only restored when both use 0.02 mm units"
                .to_string()
        }
        PlanWarning::SharedTablesMayChangeOtherProfiles { tables, profiles } => {
            let tables: Vec<String> = tables.iter().map(ToString::to_string).collect();
            let profiles: Vec<String> = profiles.iter().map(ToString::to_string).collect();
            format!(
                "{} may be shared by all profiles (not known yet); writing it may also change \
                 profile(s) {}, which this restore does not cover",
                list(&tables),
                list(&profiles)
            )
        }
        PlanWarning::LikelySharedTableDiffers { table } => format!(
            "the backup holds different copies of {table} in its profiles, but the keyboard \
             holds the same copy in every profile: it is probably shared by all profiles, so \
             the backup cannot be restored exactly (the final check will report it)"
        ),
        PlanWarning::Imported { from, into } => {
            format!("profile {from} of the file is imported into profile {into} of the keyboard")
        }
        PlanWarning::ImportedMacros { slots } => {
            let slots: Vec<String> = slots.iter().map(ToString::to_string).collect();
            format!("the import writes macro slot(s) {}", list(&slots))
        }
        PlanWarning::FnBuiltinOverridden { index, critical } => format!(
            "the Fn layer replaces the built-in function of Fn + {}{}",
            key_label(model, usize::from(*index)),
            if *critical { " (a critical one)" } else { "" }
        ),
        PlanWarning::ImportKeptAsRead { layer, indices } => {
            let keys: Vec<String> = indices
                .iter()
                .map(|i| key_label(model, usize::from(*i)))
                .collect();
            format!(
                "the file's {} keymap differs at {}, which are never imported (not keys, the \
                 knob, the Fn key or keys the keyboard locks): the keyboard's entries are kept",
                layer_name(layer),
                list(&keys)
            )
        }
        PlanWarning::Unverified { layer, index, note } => format!(
            "{} on the {} layer: {note} is not yet verified on a W75 (the vendor site sends the \
             same bytes)",
            key_label(model, usize::from(*index)),
            layer_name(layer)
        ),
    }
}

/// Warnings of a write in words.
pub fn write_warning_text(warning: &WriteWarning) -> String {
    match warning {
        WriteWarning::AckTimeout { packet, of, send } => format!(
            "packet {packet} of {of} was not acknowledged in time (attempt {send}); the rest of \
             that attempt was not sent and the result was checked by reading it back"
        ),
        WriteWarning::OptionalAckMissing { packet } => {
            format!("the optional acknowledgement of packet {packet} did not arrive")
        }
        WriteWarning::VerifyReread => "the first read-back differed; a second one matched".into(),
        WriteWarning::Retried => "the whole table was sent a second time".into(),
        WriteWarning::SelectAckMissing { profile } => format!(
            "the switch to profile {profile} was not acknowledged; it was confirmed by reading \
             the active profile"
        ),
        WriteWarning::ProfilePushMissing { profile } => format!(
            "the keyboard did not announce the switch to profile {profile}; it was confirmed by \
             reading the active profile"
        ),
        WriteWarning::SelectRetried { profile } => {
            format!("the switch to profile {profile} was sent a second time")
        }
        WriteWarning::WaitedForLateAck => {
            "waited for a late acknowledgement of the previous write before starting".into()
        }
        WriteWarning::WaitedForCommit { waited_ms } => format!(
            "waited {} for the keyboard to save the last write before switching profiles",
            seconds_text(*waited_ms)
        ),
        WriteWarning::BaseKeymapNotSaved => {
            "the operation stopped before the Fn keymap (18 02) was written after it; the \
             keyboard keeps the base keymap only then, so it will be lost at the next profile \
             switch or power loss"
                .into()
        }
        WriteWarning::UnexpectedAck { packet } => format!(
            "the keyboard acknowledged packet {packet}, which it is not seen to acknowledge; it \
             was taken as ours"
        ),
        WriteWarning::SocdToggledOnKeyboard { power } => format!(
            "SOCD was switched on the keyboard (Fn + Right Shift) during the write or before it \
             was read again, so that switch may have been overwritten, and a difference in the \
             power alone does not count as lost; SOCD is now {}",
            if *power { "on" } else { "off" }
        ),
    }
}

/// Why an Fn-keymap write is the keymap group's commit trigger, in words
/// (for the plan's change column). `unchanged`: it re-sends the keyboard's
/// Fn keymap only for this reason; otherwise it is written because it
/// differs, and the text explains its place in the group.
pub(crate) fn commit_trigger_text(reason: CommitTriggerReason, unchanged: bool) -> String {
    const BASE: &str =
        "the keyboard keeps the base keymap only when the Fn keymap is written after it";
    const SLOTS: &str = "the keyboard may keep macro and advanced-key slots only when the Fn \
                         keymap is written after them";
    match (reason, unchanged) {
        (CommitTriggerReason::BaseKeymap, true) => format!("unchanged, re-sent: {BASE}"),
        (CommitTriggerReason::BaseKeymap, false) => {
            format!("written right after the base keymap: {BASE}")
        }
        (CommitTriggerReason::MacrosOrAdvancedKeys, true) => {
            format!("unchanged, re-sent as a precaution (untested guess): {SLOTS}")
        }
        (CommitTriggerReason::MacrosOrAdvancedKeys, false) => {
            format!("written after the macro slots as a precaution (untested guess): {SLOTS}")
        }
    }
}

/// ` (read-back differs at byte offset(s) 1, 2)`, or nothing.
fn offsets_text(mismatch: &openwraith_protocol::Mismatch) -> String {
    let offsets = mismatch.differing_offsets();
    if offsets.is_empty() {
        return String::new();
    }
    let shown: Vec<String> = offsets.iter().take(8).map(ToString::to_string).collect();
    format!(
        " (read-back differs at byte offset(s) {}{})",
        shown.join(", "),
        if offsets.len() > 8 { ", ..." } else { "" }
    )
}

/// What happened to one item, in words.
fn result_text(result: &ItemResult) -> String {
    let written = |packets: usize, sends: u32| {
        let mut text = format!("written and verified ({})", count(packets, "packet"));
        if sends > 1 {
            text.push_str(", after a second attempt");
        }
        text
    };
    match result {
        ItemResult::Written {
            sends,
            packets,
            persistence,
            ..
        } => {
            let mut text = written(*packets, *sends);
            if *persistence == Persistence::Persisted {
                text.push_str(", persisted");
            }
            text
        }
        ItemResult::Lost {
            sends,
            packets,
            mismatch,
            ..
        } => format!(
            "{}, then LOST: gone after a profile switch, the keyboard did not keep it{}",
            written(*packets, *sends),
            offsets_text(mismatch)
        ),
        ItemResult::MaybeLost {
            sends,
            packets,
            warnings,
        } if warnings.contains(&WriteWarning::BaseKeymapNotSaved) => format!(
            "{}, but WILL BE LOST: the restore stopped before the Fn keymap (18 02) that makes \
             the keyboard save it (it drops it at the next profile switch or power loss)",
            written(*packets, *sends)
        ),
        ItemResult::MaybeLost { sends, packets, .. } => format!(
            "{}, but MAY BE LOST: the profile changed or the connection was lost before the \
             keyboard had time to save it",
            written(*packets, *sends)
        ),
        ItemResult::AlreadyMatched => "already held the backup's values (not written)".into(),
        ItemResult::Failed { error, mismatch } => {
            let mut text = format!("FAILED: {}", error.message);
            if let Some(m) = mismatch {
                text.push_str(&offsets_text(m));
            }
            text
        }
        ItemResult::NotAttempted => "not attempted".into(),
    }
}

/// The result of an unchanged Fn keymap re-sent as the keymap group's
/// commit trigger: it reads back the same either way, so what counts is
/// that every packet was acknowledged (and the persistence check does not
/// read it again).
fn resent_text(packets: usize, sends: u32) -> String {
    let mut text = format!(
        "re-sent in full, every packet acknowledged ({})",
        count(packets, "packet")
    );
    if sends > 1 {
        text.push_str(", after a second attempt");
    }
    text
}

/// The result of the persistence check of a restore (`what` = "restore")
/// or an edit (`what` = "command"), in words.
fn persistence_text(check: &PersistenceCheck, what: &str) -> String {
    persistence_text_with(
        check,
        &format!("Run the same {what} again, or undo it with the command below."),
    )
}

/// [`persistence_text`] with `advice` after a lost table.
fn persistence_text_with(check: &PersistenceCheck, advice: &str) -> String {
    let profiles: Vec<String> = check.profiles.iter().map(ToString::to_string).collect();
    let noun = if profiles.len() == 1 {
        "profile"
    } else {
        "profiles"
    };
    let it = |n: usize| if n == 1 { "it" } else { "them" };
    let mut parts: Vec<String> = Vec::new();
    if check.persisted > 0 {
        parts.push(format!(
            "{} still {} what was written (the keyboard saved {})",
            count(check.persisted, "written table"),
            if check.persisted == 1 {
                "holds"
            } else {
                "hold"
            },
            it(check.persisted)
        ));
    }
    if check.lost > 0 {
        parts.push(format!(
            "{} LOST (the keyboard did not keep {})",
            count(check.lost, "written table"),
            it(check.lost)
        ));
    }
    if check.not_checked > 0 {
        parts.push(format!(
            "{} could not be checked (a table that may be shared by all profiles and was \
             written in another profile too, which the final check compares, or SOCD switched \
             on the keyboard after it was written; see the notes of the table)",
            count(check.not_checked, "written table")
        ));
    }
    if parts.is_empty() {
        parts.push("no written table to read again".to_string());
    }
    let mut text = format!(
        "Persistence check: {noun} {} selected again after switching away; {}.",
        list(&profiles),
        parts.join("; ")
    );
    if check.lost > 0 {
        text.push(' ');
        text.push_str(advice);
    }
    text.push('\n');
    text
}

fn item_report_name(item: &ItemReport, model: Option<&Model>) -> String {
    match (item.table, item.key) {
        (TableId::Triggers, Some(pos)) => {
            let name = model.map_or_else(|| pos.to_string(), |m| m.key_name(pos.index()));
            format!("trigger of {name} ({pos})")
        }
        _ => item.summary.clone(),
    }
}

/// Lines for the items of a report (one row per item, warnings below).
fn item_rows(items: &[(Option<u8>, &ItemReport)], model: Option<&Model>) -> String {
    let with_profile = items.iter().any(|(p, _)| p.is_some());
    let mut table = if with_profile {
        Table::new(&["Profile", "What", "Result"]).right_align(&[0])
    } else {
        Table::new(&["What", "Result"])
    };
    let mut warnings: Vec<String> = Vec::new();
    for (profile, item) in items {
        let mut row = Vec::new();
        if with_profile {
            row.push(profile.map(|p| p.to_string()).unwrap_or_default());
        }
        row.push(item_report_name(item, model));
        row.push(match &item.result {
            ItemResult::Written { sends, packets, .. } if item.resent_unchanged => {
                resent_text(*packets, *sends)
            }
            result => result_text(result),
        });
        table.row(row);
        if let ItemResult::Written { warnings: w, .. }
        | ItemResult::Lost { warnings: w, .. }
        | ItemResult::MaybeLost { warnings: w, .. } = &item.result
        {
            // The row of a may-be-lost item already says why.
            let in_row = matches!(item.result, ItemResult::MaybeLost { .. });
            for warning in w {
                if in_row && *warning == WriteWarning::BaseKeymapNotSaved {
                    continue;
                }
                warnings.push(format!(
                    "{}: {}",
                    item_report_name(item, model),
                    write_warning_text(warning)
                ));
            }
        }
    }
    let mut out = table.render("  ");
    if !warnings.is_empty() {
        out.push_str("  Notes:\n");
        for w in warnings {
            out.push_str(&format!("    - {w}\n"));
        }
    }
    out
}

/// What became of the written tables of a report, for its headline.
#[derive(Debug, Clone, Default)]
struct Counts {
    /// Written and verified (and, for a restore, persisted or not checked).
    written: usize,
    /// Of those, unchanged Fn keymaps re-sent as the keymap group's commit
    /// trigger (counted apart in the headline).
    resent: Resent,
    /// Written and verified, but gone after a profile switch.
    lost: usize,
    /// Written and verified, but the profile changed or the connection was
    /// lost before the keyboard had time to save them, or (see
    /// `base_unsaved`) the keyboard will not save them.
    maybe_lost: usize,
    /// Of `maybe_lost`: base keymaps after which the restore stopped before
    /// the Fn keymap (`18 02`) that makes the keyboard save them.
    base_unsaved: usize,
    /// Planned writes.
    total: usize,
}

/// The unchanged Fn keymaps a restore re-sent as the keymap group's commit
/// trigger (written, not changing any setting).
#[derive(Debug, Clone, Default)]
struct Resent {
    /// The profile of each re-send, in the order written.
    profiles: Vec<u8>,
    /// Whether one was re-sent for a base keymap.
    base: bool,
    /// Whether one was re-sent for macro or advanced-key slots (precaution).
    slots: bool,
}

impl Resent {
    /// The re-sent Fn keymaps of a restore report.
    fn of(report: &RestoreReport) -> Self {
        let mut resent = Resent::default();
        for (profile, item) in report.items() {
            if !item.resent_unchanged || !matches!(item.result, ItemResult::Written { .. }) {
                continue;
            }
            resent.profiles.push(profile);
            match item.commit_trigger {
                Some(CommitTriggerReason::MacrosOrAdvancedKeys) => resent.slots = true,
                Some(CommitTriggerReason::BaseKeymap) | None => resent.base = true,
            }
        }
        resent
    }

    /// The re-sent Fn keymaps of an edit of `profile`.
    fn of_items(profile: u8, items: &[ItemReport]) -> Self {
        let mut resent = Resent::default();
        for item in items {
            if !item.resent_unchanged || !matches!(item.result, ItemResult::Written { .. }) {
                continue;
            }
            resent.profiles.push(profile);
            match item.commit_trigger {
                Some(CommitTriggerReason::MacrosOrAdvancedKeys) => resent.slots = true,
                Some(CommitTriggerReason::BaseKeymap) | None => resent.base = true,
            }
        }
        resent
    }

    /// How many were re-sent.
    fn count(&self) -> usize {
        self.profiles.len()
    }

    /// "the Fn keymap (18 02) was also re-sent unchanged ...", or `None`
    /// when none was.
    fn clause(&self) -> Option<String> {
        if self.profiles.is_empty() {
            return None;
        }
        let why = match (self.base, self.slots) {
            (true, true) => {
                "to make the keyboard save the base keymap and, as a precaution, the macro and \
                 advanced-key slots"
            }
            (false, true) => {
                "to make sure the keyboard saves the macro and advanced-key slots (precaution)"
            }
            _ => "to make the keyboard save the base keymap",
        };
        let place = if self.profiles.len() > 1 {
            let profiles: Vec<String> = self.profiles.iter().map(ToString::to_string).collect();
            format!(" in profiles {}", list(&profiles))
        } else {
            String::new()
        };
        Some(format!(
            "the Fn keymap (18 02) was also re-sent unchanged{place} {why}"
        ))
    }
}

/// The headline of a report.
fn outcome_text(outcome: Outcome, counts: &Counts, error: Option<&str>) -> String {
    let Counts {
        written,
        resent,
        lost,
        maybe_lost,
        base_unsaved,
        total,
    } = counts;
    let (lost, maybe_lost, base_unsaved) = (*lost, *maybe_lost, *base_unsaved);
    let them = |n: usize| if n == 1 { "it" } else { "them" };
    // The tables whose settings were written, without the re-sent Fn keymaps.
    let changed = written.saturating_sub(resent.count());
    let also = resent
        .clause()
        .map(|clause| format!("; {clause}"))
        .unwrap_or_default();
    match outcome {
        Outcome::Completed => format!(
            "Done: {} of {} written and verified by reading them back{also}.",
            count(changed, "table"),
            total.saturating_sub(resent.count())
        ),
        Outcome::NothingToDo => "Nothing to write.".to_string(),
        Outcome::Cancelled => format!(
            "Cancelled: {} written and verified before stopping{also}; the rest was not written.",
            count(changed, "table")
        ),
        Outcome::Failed => {
            let mut lines = Vec::new();
            if changed > 0 {
                lines.push(format!(
                    "Something WAS written: {} written and verified before the failure{also}.",
                    count(changed, "table")
                ));
            } else if let Some(clause) = resent.clause() {
                lines.push(format!("Before the failure, {clause}."));
            }
            let interrupted = maybe_lost.saturating_sub(base_unsaved);
            if interrupted > 0 {
                lines.push(format!(
                    "{} written and verified, but the keyboard may not have saved {}: the active \
                     profile changed or the connection was lost before it had time to save (it \
                     saves about 1 s after a write). Check the values before relying on them; run \
                     the command again, or undo it with the command below.",
                    count(interrupted, "table"),
                    them(interrupted)
                ));
            }
            if base_unsaved > 0 {
                lines.push(format!(
                    "{} written and verified, but the keyboard will not keep {}: the restore \
                     stopped before the Fn keymap (18 02) that makes the keyboard save the base \
                     keymap, so it drops {} at the next profile switch or power loss. Run the \
                     same restore again, or undo it with the command below.",
                    count(base_unsaved, "base keymap"),
                    them(base_unsaved),
                    them(base_unsaved)
                ));
            }
            if lost > 0 {
                lines.push(format!(
                    "{} written and verified, but read again after a profile switch {} no \
                     longer {} what was written (see the persistence check below).",
                    count(lost, "table"),
                    if lost == 1 { "it" } else { "they" },
                    if lost == 1 { "holds" } else { "hold" }
                ));
            }
            if lines.is_empty() {
                lines.push("Nothing was written to the keyboard's settings.".to_string());
            }
            format!(
                "FAILED: {}\n{}",
                error.unwrap_or("unknown error"),
                lines.join("\n")
            )
        }
    }
}

/// The note after the headline when Ctrl+C came while the writes the
/// keymap group still needed were being made (all of them were made).
fn cancel_note(cancel_requested: bool, outcome: Outcome) -> &'static str {
    if cancel_requested && outcome == Outcome::Completed {
        "Ctrl+C received; the remaining writes were needed to keep the keyboard consistent (the \
         Fn keymap that makes it save the keys, or a slot a written key refers to), so they were \
         sent. Everything planned was written.\n"
    } else {
        ""
    }
}

/// Text of a restore report.
pub fn restore_report(report: &RestoreReport, model: Option<&Model>) -> String {
    let items: Vec<(Option<u8>, &ItemReport)> =
        report.items().map(|(p, item)| (Some(p), item)).collect();
    let mut out = format!(
        "\n{}\n",
        outcome_text(
            report.outcome,
            &Counts {
                written: report.written(),
                resent: Resent::of(report),
                lost: report.lost(),
                maybe_lost: report.maybe_lost(),
                base_unsaved: report
                    .items()
                    .filter(|(_, i)| match &i.result {
                        ItemResult::MaybeLost { warnings, .. } => {
                            warnings.contains(&WriteWarning::BaseKeymapNotSaved)
                        }
                        _ => false,
                    })
                    .count(),
                total: items.len(),
            },
            report.error.as_ref().map(|e| e.message.as_str())
        )
    );
    out.push_str(cancel_note(report.cancel_requested, report.outcome));
    if !items.is_empty() {
        out.push_str(&item_rows(&items, model));
    }
    if let Some(check) = &report.persistence {
        out.push_str(&persistence_text(check, "restore"));
    }
    if let Some(check) = &report.final_check {
        let profiles: Vec<String> = check.profiles.iter().map(ToString::to_string).collect();
        if check.mismatches.is_empty() {
            out.push_str(&format!(
                "Final check: profiles {} read again; every written table, and every table that \
                 may be shared by all profiles, holds the backup's values.\n",
                list(&profiles)
            ));
        } else {
            out.push_str(&format!(
                "Final check: profiles {} read again; {} no longer hold the backup's values (a \
                 table shared by all profiles, or another program):\n",
                list(&profiles),
                count(check.mismatches.len(), "table")
            ));
            for m in &check.mismatches {
                let offsets = m.mismatch.differing_offsets();
                out.push_str(&format!(
                    "  - profile {}: {} differs from the backup ({}, {})\n",
                    m.profile,
                    m.mismatch.table,
                    m.mismatch.read,
                    count(offsets.len(), "byte")
                ));
            }
        }
    }
    if let Some(activity) = &report.foreign_activity {
        out.push_str(&format!(
            "Another program was active while writing ({activity}).\n"
        ));
    }
    out.push_str(&active_profile_text(
        report.active_profile_now,
        report.original_profile,
    ));
    out.push_str(&format!(
        "Safety backup: {}\nTo undo:       {}\n",
        report.safety_backup.display(),
        report.undo_command
    ));
    out
}

fn active_profile_text(now: Option<u8>, original: u8) -> String {
    match now {
        Some(p) if p == original => format!("Active profile: {p} (as before).\n"),
        Some(p) => format!(
            "Active profile: {p}. It was {original} before; switch back with `openwraith profile \
             select {original}`.\n"
        ),
        None => "Active profile: unknown (check with `openwraith profile`).\n".to_string(),
    }
}

/// Text of a trigger edit plan.
pub fn trigger_edit_plan(plan: &TriggerEditPlan) -> String {
    let mut table = Table::new(&["Key", "Pos", "Now", "New"]);
    for key in plan.keys() {
        let new = if key.changes() {
            trigger_text(&key.target)
        } else {
            "(already set)".to_string()
        };
        table.row(vec![
            key.name.clone(),
            key.pos.to_string(),
            trigger_text(&key.current),
            new,
        ]);
    }
    format!(
        "Trigger changes in profile {} (one packet per key):\n{}",
        plan.profile(),
        table.render("  ")
    )
}

/// Text of a main-light edit plan.
pub fn light_edit_plan(plan: &LightEditPlan) -> String {
    let mut out = format!(
        "Main light of profile {}:\n  Now  {}\n  New  {}\n",
        plan.profile(),
        light_text(plan.current()),
        light_text(plan.target())
    );
    if plan.changes_mode() {
        out.push_str(
            "  The mode changes: the music-rhythm leave (17 00) is sent right before the light \
             record, as the vendor site does.\n",
        );
    }
    out
}

/// How an edit report speaks about the persistence check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistNote {
    /// Trigger and main-light edits: the keyboard is known to keep them, so
    /// they never switch profiles to check.
    NotNeeded,
    /// The v0.2 edits, with the `--check-persistence` mode.
    Mode(PersistenceCheckMode),
    /// A slot-only keymap-group edit with `--commit-trigger skip` (the
    /// persistence check always runs): writing it again the same way would
    /// lose it again.
    CommitTriggerSkipped,
}

/// Keys left referring to an empty slot, in words.
fn dangling_text(dangling: &[DanglingBinding], model: Option<&Model>) -> String {
    if dangling.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "Keys that may now refer to an empty slot (the edit stopped part-way; the undo command \
         below restores them):\n",
    );
    for d in dangling {
        let layer = match d.layer {
            openwraith_protocol::Layer::Base => "base",
            openwraith_protocol::Layer::Fn => "Fn",
        };
        out.push_str(&format!(
            "  - profile {}, {layer} layer, {} -> {}\n",
            d.profile,
            key_label(model, usize::from(d.index)),
            d.slot
        ));
    }
    out
}

/// Text of an edit report.
pub fn edit_report(report: &EditReport, model: Option<&Model>, note: PersistNote) -> String {
    let items: Vec<(Option<u8>, &ItemReport)> =
        report.items.iter().map(|item| (None, item)).collect();
    let mut out = format!(
        "\n{}\n",
        outcome_text(
            report.outcome,
            &Counts {
                written: report.written(),
                resent: Resent::of_items(report.profile, &report.items),
                lost: report.lost(),
                maybe_lost: report.maybe_lost(),
                base_unsaved: report
                    .items
                    .iter()
                    .filter(|i| match &i.result {
                        ItemResult::MaybeLost { warnings, .. } => {
                            warnings.contains(&WriteWarning::BaseKeymapNotSaved)
                        }
                        _ => false,
                    })
                    .count(),
                total: items.len(),
            },
            report.error.as_ref().map(|e| e.message.as_str())
        )
    );
    out.push_str(cancel_note(report.cancel_requested, report.outcome));
    if !items.is_empty() {
        out.push_str(&item_rows(&items, model));
    }
    if let Some(ms) = report.waited_for_commit_ms {
        out.push_str(&format!(
            "Waited {} after the last write so that the keyboard could save it (it saves about \
             1 s after a write).",
            seconds_text(ms)
        ));
        match note {
            _ if report.persistence.is_some() => {}
            PersistNote::NotNeeded => out.push_str(
                " An edit does not switch profiles to read it again, so the lighting stays as it \
                 is.",
            ),
            PersistNote::Mode(PersistenceCheckMode::Never) => {
                out.push_str(" Not checked by switching profiles (--check-persistence never).")
            }
            PersistNote::Mode(_) if report.is_success() => out.push_str(
                " Not checked by switching profiles: the keyboard is known from hardware tests \
                 to keep what was written.",
            ),
            PersistNote::Mode(_) | PersistNote::CommitTriggerSkipped => {}
        }
        out.push('\n');
    }
    if let Some(check) = &report.persistence {
        out.push_str(&match note {
            PersistNote::CommitTriggerSkipped => persistence_text_with(
                check,
                "Run the same command without --commit-trigger skip (the Fn keymap after the \
                 slot makes the keyboard save it), or undo it with the command below.",
            ),
            _ => persistence_text(check, "command"),
        });
    }
    out.push_str(&dangling_text(&report.dangling, model));
    if let Some(activity) = &report.foreign_activity {
        out.push_str(&format!(
            "Another program was active while writing ({activity}).\n"
        ));
    }
    out.push_str(&active_profile_text(
        report.active_profile_now,
        report.profile,
    ));
    out.push_str(&format!(
        "Safety backup: {}\nTo undo:       {}\n",
        report.safety_backup.display(),
        report.undo_command
    ));
    out
}

/// Packets of every planned write, grouped per profile and table (for
/// `--packets --json`).
pub fn plan_packets(plan: &RestorePlan) -> Vec<PacketGroup> {
    plan.writes()
        .map(|(profile, item, write)| PacketGroup {
            profile,
            table: item.table,
            key: item.key.map(|k| k.to_string()),
            packets: write
                .request
                .steps()
                .iter()
                .map(|s| s.packet.summary())
                .collect(),
        })
        .collect()
}

/// The packets of one planned write.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PacketGroup {
    /// The profile.
    pub profile: ProfileId,
    /// The table.
    pub table: TableId,
    /// The key, for trigger writes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Spaced hex of each packet (trailing zero bytes omitted).
    pub packets: Vec<String>,
}

/// Packet counts per profile (for the confirmation question).
pub fn packets_per_profile(plan: &RestorePlan) -> BTreeMap<u8, usize> {
    let mut map = BTreeMap::new();
    for (profile, _, write) in plan.writes() {
        *map.entry(profile.get()).or_insert(0) += write.packets;
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwraith_protocol::{MacroSlotId, W75};

    #[test]
    fn trigger_records_in_words() {
        assert_eq!(
            trigger_bytes_text(&[0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0]),
            "1.70 mm, RT off"
        );
        assert_eq!(
            trigger_bytes_text(&[0x0c, 0x32, 0x32, 0x0f, 0x0f, 0, 0, 0, 0]),
            "1.00 mm, RT 0.30 mm"
        );
        assert_eq!(
            trigger_bytes_text(&[0x0d, 0x55, 0x55, 0x0f, 0x19, 0, 0, 0, 0]),
            "1.70 mm, RT 0.30 mm / 0.50 mm"
        );
        assert!(trigger_bytes_text(&[0x0c, 0x32, 0x30, 1, 1, 0, 0, 0, 0]).contains("second"));
        assert!(trigger_bytes_text(&[1, 2]).starts_with("raw"));
    }

    #[test]
    fn light_records_in_words() {
        // c02: Custom red, full colour 1, power byte 0 (on).
        let text = light_bytes_text(&[0x0a, 4, 3, 0xff, 0, 0, 0, 0, 0, 0, 1, 0]);
        assert_eq!(
            text,
            "Custom (per-key), colour #ff0000, brightness 4, speed 3"
        );
        assert!(
            light_bytes_text(&[0, 4, 3, 0, 0, 0xff, 0, 0, 0, 0, 0, 1])
                .ends_with("lighting switched off")
        );
    }

    #[test]
    fn table_lists_merge_slot_ranges() {
        let mut tables: Vec<TableId> = MacroSlotId::all()
            .filter(|s| s.get() != 4)
            .map(|slot| TableId::Macro { slot })
            .collect();
        tables.insert(0, TableId::DeadBand);
        assert_eq!(table_names(&tables), ["dead band", "macro slots 0-3, 5-9"]);
        assert_eq!(
            table_names(&[TableId::Macro {
                slot: MacroSlotId::new(2).expect("slot")
            }]),
            ["macro slot 2"]
        );
    }

    #[test]
    fn seconds_are_rounded_up_to_a_tenth() {
        assert_eq!(seconds_text(2000), "2 s");
        assert_eq!(seconds_text(1999), "2 s");
        assert_eq!(seconds_text(1450), "1.5 s");
        assert_eq!(
            commit_wait_text(Some(2), 2000),
            "Waiting for the keyboard to save profile 2 (2 s)..."
        );
        assert_eq!(
            commit_wait_text(None, 300),
            "Waiting for the keyboard to save (0.3 s)..."
        );
    }

    #[test]
    fn a_lost_write_is_reported_with_the_undo_and_rerun_hints() {
        use openwraith_hid::{
            ConsentOrigin, ErrorInfo, ErrorKind, PersistenceCheck, ProfileReport, WriteConsent,
        };
        use openwraith_protocol::{HexBytes, KeyPos, Mismatch, MismatchKind};
        let q = KeyPos::new(2, 1).expect("Q");
        let lost = ItemResult::Lost {
            sends: 1,
            packets: 1,
            warnings: Vec::new(),
            mismatch: Mismatch {
                table: TableId::Triggers,
                read: ReadRequest::KeyTrigger(q),
                kind: MismatchKind::Differs,
                expected: HexBytes(vec![0, 0x55, 0x55, 1, 1, 0, 0, 0, 0]),
                got: HexBytes(vec![0, 0x64, 0x64, 1, 1, 0, 0, 0, 0]),
            },
        };
        let report = RestoreReport {
            outcome: Outcome::Failed,
            error: Some(ErrorInfo {
                kind: ErrorKind::NotPersisted,
                message: "profile 2: 1 table(s) written and verified, but gone".into(),
            }),
            consent: WriteConsent::new(ConsentOrigin::CommandLineFlag),
            original_profile: 1,
            active_profile_now: Some(1),
            profiles: vec![ProfileReport {
                profile: 2,
                items: vec![ItemReport {
                    table: TableId::Triggers,
                    key: Some(q),
                    summary: "trigger".into(),
                    commit_trigger: None,
                    resent_unchanged: false,
                    result: lost,
                }],
            }],
            persistence: Some(PersistenceCheck {
                profiles: vec![2],
                persisted: 0,
                lost: 1,
                not_checked: 0,
            }),
            final_check: None,
            safety_backup: "safety.owbackup.json".into(),
            undo_command: "openwraith restore safety.owbackup.json".into(),
            foreign_activity: None,
            dangling: Vec::new(),
            cancel_requested: false,
        };
        let text = restore_report(&report, Some(&W75));
        for expected in [
            "FAILED: profile 2: 1 table(s) written and verified, but gone",
            "1 table written and verified, but read again after a profile switch it no longer \
             holds what was written (see the persistence check below).",
            "trigger of Q (r2c1)",
            "written and verified (1 packet), then LOST: gone after a profile switch, the \
             keyboard did not keep it (read-back differs at byte offset(s) 1, 2)",
            "Persistence check: profile 2 selected again after switching away; 1 written table \
             LOST (the keyboard did not keep it). Run the same restore again, or undo it with \
             the command below.",
            "To undo:       openwraith restore safety.owbackup.json",
        ] {
            assert!(text.contains(expected), "{expected}\n---\n{text}");
        }
        // Only what was observed: nothing claims the earlier values are back.
        assert!(!text.contains("earlier values"), "{text}");
        assert!(!text.contains("Nothing was written"), "{text}");
        assert!(report.anything_written());
    }

    #[test]
    fn the_persistence_check_counts_every_kind_of_table() {
        let check = PersistenceCheck {
            profiles: vec![2, 3],
            persisted: 2,
            lost: 0,
            not_checked: 1,
        };
        assert_eq!(
            persistence_text(&check, "restore"),
            "Persistence check: profiles 2 and 3 selected again after switching away; 2 written \
             tables still hold what was written (the keyboard saved them); 1 written table could \
             not be checked (a table that may be shared by all profiles and was written in \
             another profile too, which the final check compares, or SOCD switched on the \
             keyboard after it was written; see the notes of the table).\n"
        );
        let only_unchecked = PersistenceCheck {
            profiles: vec![3],
            persisted: 0,
            lost: 0,
            not_checked: 1,
        };
        let text = persistence_text(&only_unchecked, "restore");
        assert!(!text.contains("saved"), "{text}");
        assert!(
            text.contains("1 written table could not be checked"),
            "{text}"
        );
    }

    #[test]
    fn the_keymap_commit_trigger_is_explained() {
        assert_eq!(
            commit_trigger_text(CommitTriggerReason::BaseKeymap, true),
            "unchanged, re-sent: the keyboard keeps the base keymap only when the Fn keymap is \
             written after it"
        );
        // A changed Fn keymap is written because it differs: the text says
        // why it stands where it does, not that it is only a trigger.
        assert_eq!(
            commit_trigger_text(CommitTriggerReason::BaseKeymap, false),
            "written right after the base keymap: the keyboard keeps the base keymap only when \
             the Fn keymap is written after it"
        );
        assert_eq!(
            commit_trigger_text(CommitTriggerReason::MacrosOrAdvancedKeys, true),
            "unchanged, re-sent as a precaution (untested guess): the keyboard may keep macro \
             and advanced-key slots only when the Fn keymap is written after them"
        );
        assert_eq!(
            commit_trigger_text(CommitTriggerReason::MacrosOrAdvancedKeys, false),
            "written after the macro slots as a precaution (untested guess): the keyboard may \
             keep macro and advanced-key slots only when the Fn keymap is written after them"
        );
        let note = write_warning_text(&WriteWarning::BaseKeymapNotSaved);
        assert!(
            note.contains("before the Fn keymap (18 02) was written"),
            "{note}"
        );
        assert!(note.contains("will be lost"), "{note}");
        let headline = outcome_text(
            Outcome::Completed,
            &Counts {
                written: 3,
                resent: Resent {
                    profiles: vec![1],
                    base: false,
                    slots: true,
                },
                total: 3,
                ..Counts::default()
            },
            None,
        );
        assert_eq!(
            headline,
            "Done: 2 tables of 2 written and verified by reading them back; the Fn keymap (18 02) \
             was also re-sent unchanged to make sure the keyboard saves the macro and \
             advanced-key slots (precaution)."
        );
    }

    #[test]
    fn re_sends_in_several_profiles_for_different_reasons_are_all_named() {
        let resent = |profile: u8, reason: CommitTriggerReason| ItemReport {
            table: TableId::Keymap {
                layer: openwraith_protocol::Layer::Fn,
            },
            key: None,
            summary: "Fn keymap (18 02) re-sent unchanged".into(),
            commit_trigger: Some(reason),
            resent_unchanged: profile > 0,
            result: ItemResult::Written {
                sends: 1,
                packets: 10,
                warnings: Vec::new(),
                persistence: Persistence::NotChecked,
            },
        };
        let changed = |table: TableId| ItemReport {
            table,
            key: None,
            summary: table.to_string(),
            commit_trigger: None,
            resent_unchanged: false,
            result: ItemResult::Written {
                sends: 1,
                packets: 10,
                warnings: Vec::new(),
                persistence: Persistence::Persisted,
            },
        };
        let base = TableId::Keymap {
            layer: openwraith_protocol::Layer::Base,
        };
        let slot = TableId::Macro {
            slot: MacroSlotId::new(0).expect("slot"),
        };
        let report = RestoreReport {
            outcome: Outcome::Completed,
            error: None,
            consent: openwraith_hid::WriteConsent::new(
                openwraith_hid::ConsentOrigin::CommandLineFlag,
            ),
            original_profile: 1,
            active_profile_now: Some(1),
            profiles: vec![
                openwraith_hid::ProfileReport {
                    profile: 1,
                    items: vec![changed(base), resent(1, CommitTriggerReason::BaseKeymap)],
                },
                openwraith_hid::ProfileReport {
                    profile: 2,
                    items: vec![
                        changed(slot),
                        resent(2, CommitTriggerReason::MacrosOrAdvancedKeys),
                    ],
                },
            ],
            persistence: None,
            final_check: None,
            safety_backup: "safety.owbackup.json".into(),
            undo_command: "openwraith restore safety.owbackup.json".into(),
            foreign_activity: None,
            dangling: Vec::new(),
            cancel_requested: false,
        };
        let text = restore_report(&report, Some(&W75));
        assert!(
            text.contains(
                "Done: 2 tables of 2 written and verified by reading them back; the Fn keymap \
                 (18 02) was also re-sent unchanged in profiles 1 and 2 to make the keyboard \
                 save the base keymap and, as a precaution, the macro and advanced-key slots."
            ),
            "{text}"
        );
        assert!(
            text.contains("re-sent in full, every packet acknowledged (10 packets)"),
            "{text}"
        );
    }

    #[test]
    fn a_base_keymap_left_without_its_fn_keymap_is_reported_as_lost() {
        use openwraith_hid::{ConsentOrigin, ErrorInfo, ErrorKind, ProfileReport, WriteConsent};
        let item = |layer, result| ItemReport {
            table: TableId::Keymap { layer },
            key: None,
            summary: format!("{layer:?} keymap"),
            commit_trigger: (layer == openwraith_protocol::Layer::Fn)
                .then_some(CommitTriggerReason::BaseKeymap),
            resent_unchanged: layer == openwraith_protocol::Layer::Fn,
            result,
        };
        let report = RestoreReport {
            outcome: Outcome::Failed,
            error: Some(ErrorInfo {
                kind: ErrorKind::OtherProgramActive,
                message: "another program seems to be using the keyboard".into(),
            }),
            consent: WriteConsent::new(ConsentOrigin::CommandLineFlag),
            original_profile: 1,
            active_profile_now: Some(1),
            profiles: vec![ProfileReport {
                profile: 1,
                items: vec![
                    item(
                        openwraith_protocol::Layer::Base,
                        ItemResult::MaybeLost {
                            sends: 1,
                            packets: 10,
                            warnings: vec![WriteWarning::BaseKeymapNotSaved],
                        },
                    ),
                    item(
                        openwraith_protocol::Layer::Fn,
                        ItemResult::Failed {
                            error: ErrorInfo {
                                kind: ErrorKind::OtherProgramActive,
                                message: "another program".into(),
                            },
                            mismatch: None,
                        },
                    ),
                ],
            }],
            persistence: None,
            final_check: None,
            safety_backup: "safety.owbackup.json".into(),
            undo_command: "openwraith restore safety.owbackup.json".into(),
            foreign_activity: None,
            dangling: Vec::new(),
            cancel_requested: false,
        };
        let text = restore_report(&report, Some(&W75));
        for expected in [
            "1 base keymap written and verified, but the keyboard will not keep it: the restore \
             stopped before the Fn keymap (18 02) that makes the keyboard save the base keymap",
            "written and verified (10 packets), but WILL BE LOST: the restore stopped before the \
             Fn keymap (18 02) that makes the keyboard save it",
            "To undo:       openwraith restore safety.owbackup.json",
        ] {
            assert!(text.contains(expected), "{expected}\n---\n{text}");
        }
        // Not as written, and not as a profile change or a lost connection.
        assert!(!text.contains("Something WAS written"), "{text}");
        assert!(!text.contains("MAY BE LOST"), "{text}");
        assert!(
            !text.contains("profile changed or the connection"),
            "{text}"
        );
        // The row says it; no extra note.
        assert!(!text.contains("Notes:"), "{text}");
    }

    #[test]
    fn writes_that_may_not_have_been_saved_are_reported_as_such() {
        use openwraith_hid::{ConsentOrigin, EditReport, ErrorInfo, ErrorKind, WriteConsent};
        use openwraith_protocol::KeyPos;
        let q = KeyPos::new(2, 1).expect("Q");
        let report = EditReport {
            outcome: Outcome::Failed,
            error: Some(ErrorInfo {
                kind: ErrorKind::ProfileChanged,
                message: "the active onboard profile changed to 2 before the keyboard had saved \
                          the last write to profile 1"
                    .into(),
            }),
            consent: WriteConsent::new(ConsentOrigin::CommandLineFlag),
            profile: 1,
            active_profile_now: Some(2),
            items: vec![ItemReport {
                table: TableId::Triggers,
                key: Some(q),
                summary: "trigger".into(),
                commit_trigger: None,
                resent_unchanged: false,
                result: ItemResult::MaybeLost {
                    sends: 1,
                    packets: 1,
                    warnings: Vec::new(),
                },
            }],
            safety_backup: "safety.owbackup.json".into(),
            undo_command: "openwraith restore safety.owbackup.json".into(),
            foreign_activity: None,
            waited_for_commit_ms: None,
            persistence: None,
            warnings: Vec::new(),
            dangling: Vec::new(),
            cancel_requested: false,
        };
        let text = edit_report(&report, Some(&W75), PersistNote::NotNeeded);
        for expected in [
            "FAILED: the active onboard profile changed to 2 before the keyboard had saved",
            "1 table written and verified, but the keyboard may not have saved it",
            "written and verified (1 packet), but MAY BE LOST",
            "Active profile: 2. It was 1 before",
            "To undo:       openwraith restore safety.owbackup.json",
        ] {
            assert!(text.contains(expected), "{expected}\n---\n{text}");
        }
        assert!(!text.contains("Something WAS written"), "{text}");
        assert!(report.anything_written());
        assert_eq!((report.written(), report.maybe_lost()), (0, 1));
    }

    #[test]
    fn keymap_changes_name_the_key() {
        let change = EntryChange {
            index: 46,
            old: vec![0, 0, 0, 0].into(),
            new: vec![0, 4, 0, 0].into(),
        };
        assert_eq!(
            change_text(
                TableId::Keymap {
                    layer: openwraith_protocol::Layer::Base
                },
                &change,
                Some(&W75)
            ),
            "W (#46): default (00 00 00 00) -> A (00 04 00 00)"
        );
    }
}
