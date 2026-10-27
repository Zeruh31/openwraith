//! One-click actuation / Rapid Trigger presets, applied through the normal
//! trigger edit ([`plan_trigger_edit`](crate::plan_trigger_edit): one key
//! per packet, validated against the keyboard's `21/04` limits; a value out
//! of range is refused, never clamped).
//!
//! The values are defaults chosen for v0.2 (open decision 6 of the design);
//! every distance is a multiple of the 0.02 mm unit.

use openwraith_protocol::{Distance, KeyPos};
use serde::Serialize;

use crate::edit::{RtEdit, TriggerEdit};

/// A named actuation / Rapid Trigger preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TriggerPreset {
    /// Stable identifier (`typing`, `balanced`, `fast`, `competitive`,
    /// `factory`).
    pub id: &'static str,
    /// Actuation point.
    pub actuation: Distance,
    /// Rapid Trigger setting.
    pub rapid_trigger: RtEdit,
}

/// The built-in presets: typing 2.00 mm (RT off), balanced 1.20 mm (RT
/// 0.30 mm), fast 0.80 mm (RT 0.16 mm), competitive 0.40 mm (RT 0.10 mm),
/// factory 1.70 mm with RT off and both sensitivities 0.02 mm: the factory
/// record `00 55 55 01 01` of every key (c07, c11).
pub static TRIGGER_PRESETS: [TriggerPreset; 5] = [
    TriggerPreset {
        id: "typing",
        actuation: Distance(100),
        rapid_trigger: RtEdit::Off,
    },
    TriggerPreset {
        id: "balanced",
        actuation: Distance(60),
        rapid_trigger: RtEdit::Same {
            sensitivity: Distance(15),
        },
    },
    TriggerPreset {
        id: "fast",
        actuation: Distance(40),
        rapid_trigger: RtEdit::Same {
            sensitivity: Distance(8),
        },
    },
    TriggerPreset {
        id: "competitive",
        actuation: Distance(20),
        rapid_trigger: RtEdit::Same {
            sensitivity: Distance(5),
        },
    },
    TriggerPreset {
        id: "factory",
        actuation: Distance(85),
        rapid_trigger: RtEdit::OffWith {
            press: Distance(1),
            release: Distance(1),
        },
    },
];

/// The preset with this identifier.
pub fn trigger_preset(id: &str) -> Option<&'static TriggerPreset> {
    TRIGGER_PRESETS.iter().find(|p| p.id == id)
}

/// The trigger edit that applies `preset` to `keys`.
pub fn preset_edit(preset: &TriggerPreset, keys: Vec<KeyPos>) -> TriggerEdit {
    TriggerEdit {
        keys,
        actuation: Some(preset.actuation),
        rapid_trigger: Some(preset.rapid_trigger),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_unit_multiples_within_the_w75_range() {
        let ceilings = openwraith_protocol::W75_WRITE_CEILINGS;
        for preset in &TRIGGER_PRESETS {
            let actuation = preset.actuation.units();
            assert!(
                (ceilings.min_actuation..=ceilings.max_actuation).contains(&actuation),
                "{}",
                preset.id
            );
            if let RtEdit::Same { sensitivity } = preset.rapid_trigger {
                assert!((1..=ceilings.max_sensitivity).contains(&sensitivity.units()));
            }
        }
        assert_eq!(
            trigger_preset("fast").map(|p| p.actuation),
            Some(Distance(40))
        );
        assert!(trigger_preset("turbo").is_none());
        let edit = preset_edit(&TRIGGER_PRESETS[0], vec![]);
        assert_eq!(edit.actuation, Some(Distance(100)));
        // The factory preset writes the factory record exactly.
        let factory = trigger_preset("factory").expect("factory");
        assert_eq!(factory.actuation, Distance(0x55));
        assert_eq!(
            factory.rapid_trigger,
            RtEdit::OffWith {
                press: Distance(1),
                release: Distance(1)
            }
        );
    }
}
