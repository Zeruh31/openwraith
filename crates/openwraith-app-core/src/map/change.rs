//! One changed entry of a planned restore write, decoded like the CLI's
//! `report::change_text`.

use openwraith_hid::EntryChange;
use openwraith_protocol::{Distance, LightMode, TableId};

use super::{hex, key_ref_usize, light_mode, sat16};
use crate::dto::{
    AdvancedSummary, ChangeView, DeadBandPair, LightBytesView, LightValue, MacroSummary,
    TriggerBytesView, TriggerModeView, TriggerValue,
};

/// A trigger record `MM A A P R Ah Ah Ph Rh`.
fn trigger_bytes(bytes: &[u8]) -> TriggerBytesView {
    let [mode, a, a2, p, r, ah, a2h, ph, rh] = match <[u8; 9]>::try_from(bytes) {
        Ok(b) => b,
        Err(_) => return TriggerBytesView::Raw { hex: hex(bytes) },
    };
    TriggerBytesView::Decoded {
        value: TriggerValue {
            mode: match mode {
                0x00 => TriggerModeView::Off,
                0x0C => TriggerModeView::RapidTrigger,
                0x0D => TriggerModeView::RapidTriggerSeparate,
                _ => TriggerModeView::Unknown,
            },
            mode_byte: mode,
            actuation: Distance::from_lo_hi(a, ah).units(),
            rt_press: Distance::from_lo_hi(p, ph).units(),
            rt_release: Distance::from_lo_hi(r, rh).units(),
        },
        second_travel: Distance::from_lo_hi(a2, a2h).units(),
    }
}

/// A 12-byte main light record.
fn light_bytes(bytes: &[u8]) -> LightBytesView {
    let Ok(b) = <[u8; 12]>::try_from(bytes) else {
        return LightBytesView::Raw { hex: hex(bytes) };
    };
    LightBytesView::Decoded {
        value: LightValue {
            mode: light_mode(LightMode::from_id(b[0])),
            brightness: b[1],
            speed: b[2],
            color: format!("#{:02x}{:02x}{:02x}", b[3], b[4], b[5]),
            full_color: b[10] != 0,
            power_on: b[11] == 0,
        },
    }
}

/// A macro slot's contents.
fn macro_summary(bytes: &[u8]) -> MacroSummary {
    match bytes {
        [] => MacroSummary::Empty,
        [_, _, _, n, ..] if *n >= 4 && n % 4 == 0 => MacroSummary::Steps {
            count: u16::from(*n / 4),
        },
        _ => MacroSummary::InUse,
    }
}

/// An advanced-key slot's contents.
fn advanced_summary(bytes: &[u8]) -> AdvancedSummary {
    match bytes.first() {
        None => AdvancedSummary::Empty,
        Some(kind) => AdvancedSummary::InUse { kind: *kind },
    }
}

/// One changed entry of a write to `table`.
pub(crate) fn change_view(table: TableId, change: &EntryChange) -> ChangeView {
    let (old, new): (&[u8], &[u8]) = (&change.old, &change.new);
    match table {
        TableId::Triggers => ChangeView::Trigger {
            old: trigger_bytes(old),
            new: trigger_bytes(new),
        },
        TableId::MainLight => ChangeView::Light {
            old: light_bytes(old),
            new: light_bytes(new),
        },
        TableId::Keymap { .. } => ChangeView::Keymap {
            key: key_ref_usize(change.index),
            old_hex: hex(old),
            new_hex: hex(new),
        },
        TableId::DeadBand => {
            let pair = |b: &[u8]| match b {
                [top, bottom] => Some(DeadBandPair {
                    top: u16::from(*top),
                    bottom: u16::from(*bottom),
                }),
                _ => None,
            };
            ChangeView::DeadBand {
                key: key_ref_usize(change.index),
                old: pair(old),
                new: pair(new),
            }
        }
        TableId::CustomColors { .. } => {
            let colour = |b: &[u8]| match b {
                [r, g, b] => Some(format!("#{r:02x}{g:02x}{b:02x}")),
                _ => None,
            };
            ChangeView::Color {
                key: key_ref_usize(change.index),
                old: colour(old),
                new: colour(new),
            }
        }
        TableId::Macro { .. } => ChangeView::Macro {
            old: macro_summary(old),
            new: macro_summary(new),
        },
        TableId::AdvancedKey { .. } => ChangeView::AdvancedKey {
            old: advanced_summary(old),
            new: advanced_summary(new),
        },
        TableId::Profile
        | TableId::SideLight
        | TableId::ActiveCustomTable
        | TableId::SocdPairs
        | TableId::SocdSettings
        | TableId::SwitchTypes
        | TableId::PollRate
        | TableId::WinLockAndRollover => ChangeView::Raw {
            index: sat16(change.index),
            old_hex: hex(old),
            new_hex: hex(new),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwraith_protocol::HexBytes;

    fn change(index: usize, old: &[u8], new: &[u8]) -> EntryChange {
        EntryChange {
            index,
            old: HexBytes(old.to_vec()),
            new: HexBytes(new.to_vec()),
        }
    }

    #[test]
    fn trigger_changes_decode_like_the_cli() {
        // W: 1.00 mm + RT 0.30 -> 1.70 mm, RT off (the CLI prints
        // "1.00 mm, RT 0.30 mm -> 1.70 mm, RT off").
        let view = change_view(
            TableId::Triggers,
            &change(
                46,
                &[0x0C, 50, 50, 15, 15, 0, 0, 0, 0],
                &[0x00, 85, 85, 1, 1, 0, 0, 0, 0],
            ),
        );
        let ChangeView::Trigger { old, new } = view else {
            panic!("not a trigger change: {view:?}");
        };
        assert!(
            matches!(old, TriggerBytesView::Decoded { value, second_travel: 50 }
            if value.mode == TriggerModeView::RapidTrigger && value.actuation == 50 && value.rt_press == 15)
        );
        assert!(matches!(new, TriggerBytesView::Decoded { value, .. }
            if value.mode == TriggerModeView::Off && value.actuation == 85));
    }

    #[test]
    fn other_changes() {
        assert!(matches!(
            change_view(TableId::MainLight, &change(0, &[0x0A, 4, 3, 0xff, 0, 0, 0, 0, 0, 0, 1, 0], &[1, 2])),
            ChangeView::Light { old: LightBytesView::Decoded { value }, new: LightBytesView::Raw { .. } }
                if value.color == "#ff0000" && value.full_color
        ));
        assert!(matches!(
            change_view(TableId::DeadBand, &change(46, &[1, 2], &[3, 4])),
            ChangeView::DeadBand { key, old: Some(DeadBandPair { top: 1, bottom: 2 }), .. } if key.name == "W"
        ));
        assert!(matches!(
            change_view(
                TableId::Macro {
                    slot: openwraith_protocol::MacroSlotId::new(2).unwrap()
                },
                &change(2, &[0, 0, 0, 16, 1], &[])
            ),
            ChangeView::Macro {
                old: MacroSummary::Steps { count: 4 },
                new: MacroSummary::Empty
            }
        ));
    }
}
