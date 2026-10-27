//! Restore plans as views.

use std::collections::BTreeMap;

use openwraith_hid::{BlockCause, PlanAction, PlanItem, PlanWarning, RestorePlan, SkipReason};

use super::change::change_view;
use super::{clean, commit_reason, hex, key_ref, layer_view, sat16, table_ref};
use crate::dto::{
    BlockCauseView, PlanActionView, PlanItemView, PlanTotals, PlanWarningView, ProfilePlanView,
    ProfileTotals, RestorePlanView, SkipReasonView,
};

/// At most this many changed entries are listed per write.
const MAX_CHANGES: usize = 200;

fn block_cause(cause: &BlockCause) -> BlockCauseView {
    match cause {
        BlockCause::UnwritableAdvancedKey { slot, reason } => {
            BlockCauseView::UnwritableAdvancedKey {
                slot: *slot,
                reason: clean(reason),
            }
        }
        BlockCause::NonCanonicalMacro { slot, reason } => BlockCauseView::NonCanonicalMacro {
            slot: *slot,
            rule: (*reason).to_string(),
        },
        BlockCause::MalformedReference { layer, index } => BlockCauseView::MalformedReference {
            layer: layer_view(*layer),
            index: *index,
        },
        BlockCause::Incomplete { table } => BlockCauseView::Incomplete {
            table: table_ref(*table),
        },
        BlockCause::FnKeymapNotWritable { reason } => BlockCauseView::FnKeymapNotWritable {
            reason: commit_reason(*reason),
        },
        BlockCause::CriticalFnOverride { index } => {
            BlockCauseView::CriticalFnOverride { index: *index }
        }
        BlockCause::UnwritableImportedEntry {
            layer,
            index,
            entry,
        } => BlockCauseView::UnwritableImportedEntry {
            layer: layer_view(*layer),
            index: *index,
            entry: hex(entry),
        },
    }
}

/// A skip reason.
pub(crate) fn skip_reason(reason: &SkipReason) -> SkipReasonView {
    match reason {
        SkipReason::UnconfirmedCommand { command } => SkipReasonView::UnconfirmedCommand {
            command: (*command).to_string(),
        },
        SkipReason::NoKnownCommand => SkipReasonView::NoKnownCommand,
        SkipReason::UnsupportedLightMode { mode } => {
            SkipReasonView::UnsupportedLightMode { mode: *mode }
        }
        SkipReason::InvalidLightRecord => SkipReasonView::InvalidLightRecord,
        SkipReason::Knob => SkipReasonView::Knob,
        SkipReason::UnknownTriggerMode { mode } => {
            SkipReasonView::UnknownTriggerMode { mode: *mode }
        }
        SkipReason::SecondTravelDiffers => SkipReasonView::SecondTravelDiffers,
        SkipReason::PressReleaseDiffer => SkipReasonView::PressReleaseDiffer,
        SkipReason::OutOfRange => SkipReasonView::OutOfRange,
        SkipReason::TriggerLimitsUnknown => SkipReasonView::TriggerLimitsUnknown,
        SkipReason::DeadBandOutOfRange => SkipReasonView::DeadBandOutOfRange,
        SkipReason::DeadBandUnitUnknown => SkipReasonView::DeadBandUnitUnknown,
        SkipReason::DistanceUnitChanged {
            backup_um,
            keyboard_um,
        } => SkipReasonView::DistanceUnitChanged {
            backup_um: *backup_um,
            keyboard_um: *keyboard_um,
        },
        SkipReason::KeymapGroupBlocked { cause } => SkipReasonView::KeymapGroupBlocked {
            cause: block_cause(cause),
        },
        SkipReason::NotInKeyboard => SkipReasonView::NotInKeyboard,
        SkipReason::Unusable => SkipReasonView::Unusable,
        SkipReason::ReadOnlyBytes => SkipReasonView::ReadOnlyBytes,
        SkipReason::UnwritableSocdPairs { rule } => SkipReasonView::UnwritableSocdPairs {
            rule: (*rule).to_string(),
        },
        SkipReason::NotImported => SkipReasonView::NotImported,
    }
}

fn warning(warning: &PlanWarning) -> PlanWarningView {
    match warning {
        PlanWarning::FirmwareDiffers { backup, keyboard } => PlanWarningView::FirmwareDiffers {
            backup: backup.as_deref().map(clean),
            keyboard: keyboard.as_deref().map(clean),
        },
        PlanWarning::ConstantDiffers { request } => PlanWarningView::ConstantDiffers {
            request: request.to_string(),
        },
        PlanWarning::CapabilityFlagsDiffer => PlanWarningView::CapabilityFlagsDiffer,
        PlanWarning::SharedTablesMayChangeOtherProfiles { tables, profiles } => {
            PlanWarningView::SharedTablesMayChangeOtherProfiles {
                tables: tables.iter().copied().map(table_ref).collect(),
                profiles: profiles.clone(),
            }
        }
        PlanWarning::LikelySharedTableDiffers { table } => {
            PlanWarningView::LikelySharedTableDiffers {
                table: table_ref(*table),
            }
        }
        PlanWarning::Imported { from, into } => PlanWarningView::Imported {
            from: *from,
            into: *into,
        },
        PlanWarning::ImportedMacros { slots } => PlanWarningView::ImportedMacros {
            slots: slots.clone(),
        },
        PlanWarning::FnBuiltinOverridden { index, critical } => {
            PlanWarningView::FnBuiltinOverridden {
                index: *index,
                critical: *critical,
            }
        }
        PlanWarning::ImportKeptAsRead { layer, indices } => PlanWarningView::ImportKeptAsRead {
            layer: layer_view(*layer),
            indices: indices.clone(),
        },
        PlanWarning::Unverified { layer, index, note } => PlanWarningView::Unverified {
            layer: layer_view(*layer),
            index: *index,
            what: note.what.to_string(),
            question: note.question.to_string(),
        },
    }
}

fn item(item: &PlanItem) -> PlanItemView {
    let action = match &item.action {
        PlanAction::Unchanged => PlanActionView::Unchanged,
        PlanAction::NotInBackup => PlanActionView::NotInBackup,
        PlanAction::Write(write) => PlanActionView::Write {
            packets: sat16(write.packets),
            changes: write
                .changes
                .iter()
                .take(MAX_CHANGES)
                .map(|c| change_view(item.table, c))
                .collect(),
            more_changes: sat16(write.changes.len().saturating_sub(MAX_CHANGES)),
            commit_trigger: write.commit_trigger.map(commit_reason),
            resent_unchanged: write.is_resent_unchanged(),
        },
        PlanAction::Skip { reason, message } => PlanActionView::Skip {
            reason: skip_reason(reason),
            message: clean(message),
        },
    };
    PlanItemView {
        table: table_ref(item.table),
        key: item.key.map(key_ref),
        action,
    }
}

/// A restore plan as a view.
pub(crate) fn restore_plan_view(plan: &RestorePlan) -> RestorePlanView {
    let mut per_profile: BTreeMap<u8, (usize, usize)> = BTreeMap::new();
    for (profile, _, write) in plan.writes() {
        let entry = per_profile.entry(profile.get()).or_default();
        entry.0 += 1;
        entry.1 += write.packets;
    }
    RestorePlanView {
        product: plan.product.as_deref().map(clean),
        backup_created_at: clean(&plan.backup_created_at),
        keyboard_read_at: clean(&plan.keyboard_read_at),
        backup_firmware: plan.backup_firmware.as_deref().map(clean),
        keyboard_firmware: plan.keyboard_firmware.as_deref().map(clean),
        original_profile: plan.original_profile,
        profiles: plan
            .profiles
            .iter()
            .map(|p| ProfilePlanView {
                profile: p.profile.get(),
                compared: p.compared,
                items: p.items.iter().map(item).collect(),
            })
            .collect(),
        warnings: plan.warnings.iter().map(warning).collect(),
        totals: PlanTotals {
            writes: sat16(plan.write_count()),
            packets: sat16(plan.packet_count()),
            per_profile: per_profile
                .into_iter()
                .map(|(profile, (writes, packets))| ProfileTotals {
                    profile,
                    writes: sat16(writes),
                    packets: sat16(packets),
                })
                .collect(),
        },
    }
}
