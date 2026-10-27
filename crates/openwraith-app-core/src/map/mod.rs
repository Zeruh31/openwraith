//! Mappings from the library's types to the IPC data types.

mod change;
mod error;
mod plan;
mod report;
mod snapshot;

pub(crate) use plan::restore_plan_view;
pub(crate) use report::{edit_report_view, profile_switch_report, restore_report_view};
pub(crate) use snapshot::{
    backup_summary, key_trigger_view, light_state, light_value, snapshot_from_backup,
    trigger_value_of_setting,
};

use openwraith_hid::ForeignActivity;
use openwraith_protocol::{CustomTable, KeyPos, Layer, LightMode, MATRIX_COLS, TableId, W75};

use crate::dto::{
    CommitTriggerReasonView, ForeignActivityView, KeyRef, LayerView, LightModeKey, LightModeView,
    TableRef,
};

/// `usize` as `u16`, saturating.
pub(crate) fn sat16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// `u64` as `u32`, saturating.
pub(crate) fn sat32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// `u32` as `u8`, saturating.
pub(crate) fn sat8(value: u32) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

/// Text from the device or a file, without control and bidi characters
/// (rendered as plain text by the frontend anyway).
pub(crate) fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    *c,
                    '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
                )
        })
        .collect()
}

/// Lowercase hex without spaces.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The key at `index` (the W75 layout; `#N` outside it).
pub(crate) fn key_ref_index(index: u8) -> KeyRef {
    KeyRef {
        index,
        row: index / MATRIX_COLS,
        col: index % MATRIX_COLS,
        name: W75.key_name(index),
    }
}

/// The key at `pos`.
pub(crate) fn key_ref(pos: KeyPos) -> KeyRef {
    key_ref_index(pos.index())
}

/// The key at a `usize` index (entries of plan changes).
pub(crate) fn key_ref_usize(index: usize) -> KeyRef {
    key_ref_index(u8::try_from(index).unwrap_or(u8::MAX))
}

/// A table reference.
pub(crate) fn table_ref(table: TableId) -> TableRef {
    match table {
        TableId::Profile => TableRef::Profile,
        TableId::MainLight => TableRef::MainLight,
        TableId::SideLight => TableRef::SideLight,
        TableId::ActiveCustomTable => TableRef::ActiveCustomTable,
        TableId::CustomColors { table } => TableRef::CustomColors {
            number: match table {
                CustomTable::Custom1 => 1,
                CustomTable::Custom2 => 2,
            },
        },
        TableId::Keymap { layer } => TableRef::Keymap {
            layer: layer_view(layer),
        },
        TableId::Macro { slot } => TableRef::Macro { slot: slot.get() },
        TableId::AdvancedKey { slot } => TableRef::AdvancedKey { slot: slot.get() },
        TableId::Triggers => TableRef::Triggers,
        TableId::DeadBand => TableRef::DeadBand,
        TableId::SocdPairs => TableRef::SocdPairs,
        TableId::SocdSettings => TableRef::SocdSettings,
        TableId::Socd => TableRef::Socd,
        TableId::SwitchTypes => TableRef::SwitchTypes,
        TableId::PollRate => TableRef::PollRate,
        TableId::WinLockAndRollover => TableRef::WinLockAndRollover,
    }
}

/// A layer.
pub(crate) fn layer_view(layer: Layer) -> LayerView {
    match layer {
        Layer::Base => LayerView::Base,
        Layer::Fn => LayerView::Fn,
    }
}

/// A commit-trigger reason.
pub(crate) fn commit_reason(
    reason: openwraith_hid::CommitTriggerReason,
) -> CommitTriggerReasonView {
    match reason {
        openwraith_hid::CommitTriggerReason::BaseKeymap => CommitTriggerReasonView::BaseKeymap,
        openwraith_hid::CommitTriggerReason::MacrosOrAdvancedKeys => {
            CommitTriggerReasonView::MacrosOrAdvancedKeys
        }
    }
}

/// Foreign activity.
pub(crate) fn foreign(activity: ForeignActivity) -> ForeignActivityView {
    ForeignActivityView {
        status_reports: sat32(activity.status_reports),
        foreign_acks: sat32(activity.foreign_acks),
        profile_reports: sat32(activity.profile_reports),
    }
}

/// A light mode.
pub(crate) fn light_mode(mode: LightMode) -> LightModeView {
    let key = match mode {
        LightMode::Static => LightModeKey::Static,
        LightMode::Breath => LightModeKey::Breath,
        LightMode::Wave => LightModeKey::Wave,
        LightMode::Neon => LightModeKey::Neon,
        LightMode::Radar => LightModeKey::Radar,
        LightMode::Reactive => LightModeKey::Reactive,
        LightMode::Aurora => LightModeKey::Aurora,
        LightMode::Ripple => LightModeKey::Ripple,
        LightMode::Twinkle => LightModeKey::Twinkle,
        LightMode::Custom => LightModeKey::Custom,
        LightMode::Cross => LightModeKey::Cross,
        LightMode::SpeedRespond => LightModeKey::SpeedRespond,
        LightMode::AutoRipple => LightModeKey::AutoRipple,
        LightMode::Striation => LightModeKey::Striation,
        LightMode::Fireworks => LightModeKey::Fireworks,
        LightMode::MusicRhythm => LightModeKey::MusicRhythm,
        LightMode::Unknown(_) => LightModeKey::Unknown,
    };
    LightModeView { id: mode.id(), key }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_and_bidi_characters_are_removed() {
        assert_eq!(clean("W75\u{1b}[31m\u{202E}x\n"), "W75[31mx");
        assert_eq!(clean("Wraith W75"), "Wraith W75");
    }

    #[test]
    fn keys_and_tables() {
        let w = key_ref_index(46);
        assert_eq!((w.row, w.col, w.name.as_str()), (2, 2, "W"));
        assert_eq!(key_ref_index(103).name, "#103");
        assert_eq!(
            table_ref(TableId::CustomColors {
                table: CustomTable::Custom1
            }),
            TableRef::CustomColors { number: 1 }
        );
        assert_eq!(light_mode(LightMode::Unknown(5)).id, 5);
        assert_eq!(sat16(70_000), u16::MAX);
    }
}
