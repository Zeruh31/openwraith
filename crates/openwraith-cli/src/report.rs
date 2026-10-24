//! Text output of the write path: restore plans and reports, edit plans and
//! edit reports.

use std::collections::BTreeMap;

use openwraith_hid::{
    EditReport, EntryChange, ItemReport, ItemResult, LightEditPlan, Outcome, PlanAction, PlanItem,
    PlanWarning, PlannedWrite, RestorePlan, RestoreReport, TriggerEditPlan, WriteWarning,
};
use openwraith_protocol::{
    Distance, LightMode, LightRecord, Model, ProfileId, ReadRequest, Rgb, TableId, TriggerSetting,
    TriggerWriteMode,
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

/// One changed entry of a planned write, in words.
fn change_text(table: TableId, change: &EntryChange, model: Option<&Model>) -> String {
    let (old, new) = (&change.old, &change.new);
    match table {
        TableId::Triggers => format!("{} -> {}", trigger_bytes_text(old), trigger_bytes_text(new)),
        TableId::MainLight => format!("{} -> {}", light_bytes_text(old), light_bytes_text(new)),
        TableId::Keymap { .. } => format!(
            "{}: {} -> {}",
            key_label(model, change.index),
            spaced(old),
            spaced(new)
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
        TableId::AdvancedKey { .. } => {
            let slot = |b: &[u8]| {
                if b.is_empty() {
                    "empty".to_string()
                } else {
                    format!("in use (type {})", b.first().copied().unwrap_or(0))
                }
            };
            format!("{} -> {}", slot(old), slot(new))
        }
        _ => format!("{} -> {}", spaced(old), spaced(new)),
    }
}

/// The changed entries of a planned write, at most [`MAX_CHANGES`].
fn changes_text(table: TableId, write: &PlannedWrite, model: Option<&Model>) -> Vec<String> {
    let mut lines: Vec<String> = write
        .changes
        .iter()
        .take(MAX_CHANGES)
        .map(|c| change_text(table, c, model))
        .collect();
    if write.changes.len() > MAX_CHANGES {
        lines.push(format!(
            "... and {} more entries",
            write.changes.len() - MAX_CHANGES
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
    let mut out = String::from(if dry_run {
        "Restore plan (dry run: nothing is written)\n"
    } else {
        "Restore plan\n"
    });
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
                    let changes = changes_text(item.table, write, model);
                    let mut first = true;
                    for line in changes.iter().map(String::as_str).chain(
                        // A write without listed changes still gets a row.
                        changes.is_empty().then_some(""),
                    ) {
                        if first {
                            let what = match item.key {
                                Some(_) => item_name(item, model),
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
            out.push_str(&format!("  - {}\n", warning_text(warning)));
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
    out.push('\n');
    if plan.has_writes() {
        out.push_str(&format!(
            "Total: {} ({}) in profile {}",
            count(plan.write_count(), "write"),
            count(plan.packet_count(), "packet"),
            list(&profiles)
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

/// A plan warning in words.
fn warning_text(warning: &PlanWarning) -> String {
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
    }
}

/// What happened to one item, in words.
fn result_text(result: &ItemResult) -> String {
    match result {
        ItemResult::Written { sends, packets, .. } => {
            let mut text = format!("written and verified ({})", count(*packets, "packet"));
            if *sends > 1 {
                text.push_str(", after a second attempt");
            }
            text
        }
        ItemResult::AlreadyMatched => "already held the backup's values (not written)".into(),
        ItemResult::Failed { error, mismatch } => {
            let mut text = format!("FAILED: {}", error.message);
            if let Some(m) = mismatch {
                let offsets = m.differing_offsets();
                if !offsets.is_empty() {
                    let shown: Vec<String> =
                        offsets.iter().take(8).map(ToString::to_string).collect();
                    text.push_str(&format!(
                        " (read-back differs at byte offset(s) {}{})",
                        shown.join(", "),
                        if offsets.len() > 8 { ", ..." } else { "" }
                    ));
                }
            }
            text
        }
        ItemResult::NotAttempted => "not attempted".into(),
    }
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
        row.push(result_text(&item.result));
        table.row(row);
        if let ItemResult::Written { warnings: w, .. } = &item.result {
            for warning in w {
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

/// The headline of a report.
fn outcome_text(outcome: Outcome, written: usize, total: usize, error: Option<&str>) -> String {
    match outcome {
        Outcome::Completed => format!(
            "Done: {} of {total} written and verified by reading them back.",
            count(written, "table")
        ),
        Outcome::NothingToDo => "Nothing to write.".to_string(),
        Outcome::Cancelled => format!(
            "Cancelled: {} written and verified before stopping; the rest was not written.",
            count(written, "table")
        ),
        Outcome::Failed => format!(
            "FAILED: {}\n{}",
            error.unwrap_or("unknown error"),
            if written > 0 {
                format!(
                    "Something WAS written: {} written and verified before the failure.",
                    count(written, "table")
                )
            } else {
                "Nothing was written to the keyboard's settings.".to_string()
            }
        ),
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
            report.written(),
            items.len(),
            report.error.as_ref().map(|e| e.message.as_str())
        )
    );
    if !items.is_empty() {
        out.push_str(&item_rows(&items, model));
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

/// Text of an edit report.
pub fn edit_report(report: &EditReport, model: Option<&Model>) -> String {
    let items: Vec<(Option<u8>, &ItemReport)> =
        report.items.iter().map(|item| (None, item)).collect();
    let mut out = format!(
        "\n{}\n",
        outcome_text(
            report.outcome,
            report.written(),
            items.len(),
            report.error.as_ref().map(|e| e.message.as_str())
        )
    );
    if !items.is_empty() {
        out.push_str(&item_rows(&items, model));
    }
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
            "W (#46): 00 00 00 00 -> 00 04 00 00"
        );
    }
}
