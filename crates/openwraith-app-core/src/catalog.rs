//! The frontend's generated tables of the Keys page (written by the
//! bindings test into `app/src/generated/`, checked up to date):
//!
//! * the action catalogue of the ActionPicker (`actionCatalog.ts`), built
//!   from the protocol crate: the HID usages, multimedia indices and mouse
//!   actions the vendor site offers, the shortcut and binding rules, the
//!   DKS step grammar (every row the library accepts), the Mod-Tap and macro
//!   limits, each with the library's evidence;
//! * the macro recorder's table `KeyboardEvent.code` -> HID usage
//!   (`hidCodes.ts`): the W3C UI Events `code` values of the USB HID
//!   keyboard page, limited to the usages the keyboard accepts.

use std::fmt::Write as _;

use openwraith_hid::protocol::write::{Dks, DksRow, DksStep, MacroStep, ModTap};
use openwraith_hid::protocol::{
    AdvancedKeyDef, AdvancedLimits, AdvancedSlotId, Combo, HidUsage, KeyAssignment, Layer,
    MacroPlay, MacroProgram, MacroSlotId, MediaKey, Model, MouseAction, MouseButton, ReadRequest,
    TriggerGlobal,
};
use serde_json::{Value, json};

use crate::dto::{
    ActionCatalog, ActionValue, BindingRuleView, CatalogEntry, CatalogGroup, CatalogLayerView,
    CatalogModifier, CatalogTab, ComboRuleView, DksRowRuleView, EvidenceView, LayoutKeyActionView,
    MacroPlayKindView, MacroRulesView, ModTapRuleView, PlayModeRuleView, SlotKindView,
};
use crate::map::keys::{
    action_id, action_value, dks_step_view, evidence_view, mouse_label, notes_evidence,
};
use crate::map::{hex, layer_view};

/// The W3C UI Events `code` values of the keyboard page, with their HID
/// usage. `OSLeft` / `OSRight` are older names of the Meta keys.
pub(crate) fn key_codes() -> Vec<(String, u8)> {
    let mut codes: Vec<(String, u8)> = Vec::new();
    for (i, letter) in ('A'..='Z').enumerate() {
        codes.push((format!("Key{letter}"), 0x04 + u8::try_from(i).unwrap()));
    }
    for digit in 1..=9u8 {
        codes.push((format!("Digit{digit}"), 0x1D + digit));
    }
    codes.push(("Digit0".into(), 0x27));
    for (name, usage) in [
        ("Enter", 0x28),
        ("Escape", 0x29),
        ("Backspace", 0x2A),
        ("Tab", 0x2B),
        ("Space", 0x2C),
        ("Minus", 0x2D),
        ("Equal", 0x2E),
        ("BracketLeft", 0x2F),
        ("BracketRight", 0x30),
        ("Backslash", 0x31),
        ("Semicolon", 0x33),
        ("Quote", 0x34),
        ("Backquote", 0x35),
        ("Comma", 0x36),
        ("Period", 0x37),
        ("Slash", 0x38),
        ("CapsLock", 0x39),
    ] {
        codes.push((name.into(), usage));
    }
    for n in 1..=12u8 {
        codes.push((format!("F{n}"), 0x39 + n));
    }
    for (name, usage) in [
        ("PrintScreen", 0x46),
        ("ScrollLock", 0x47),
        ("Pause", 0x48),
        ("Insert", 0x49),
        ("Home", 0x4A),
        ("PageUp", 0x4B),
        ("Delete", 0x4C),
        ("End", 0x4D),
        ("PageDown", 0x4E),
        ("ArrowRight", 0x4F),
        ("ArrowLeft", 0x50),
        ("ArrowDown", 0x51),
        ("ArrowUp", 0x52),
        ("NumLock", 0x53),
        ("NumpadDivide", 0x54),
        ("NumpadMultiply", 0x55),
        ("NumpadSubtract", 0x56),
        ("NumpadAdd", 0x57),
        ("NumpadEnter", 0x58),
    ] {
        codes.push((name.into(), usage));
    }
    for n in 1..=9u8 {
        codes.push((format!("Numpad{n}"), 0x58 + n));
    }
    for (name, usage) in [
        ("Numpad0", 0x62),
        ("NumpadDecimal", 0x63),
        ("IntlBackslash", 0x64),
        ("ContextMenu", 0x65),
    ] {
        codes.push((name.into(), usage));
    }
    for n in 13..=24u8 {
        codes.push((format!("F{n}"), 0x5B + n));
    }
    for (name, usage) in [
        ("IntlRo", 0x87),
        ("KanaMode", 0x88),
        ("IntlYen", 0x89),
        ("Convert", 0x8A),
        ("NonConvert", 0x8B),
        ("ControlLeft", 0xE0),
        ("ShiftLeft", 0xE1),
        ("AltLeft", 0xE2),
        ("MetaLeft", 0xE3),
        ("ControlRight", 0xE4),
        ("ShiftRight", 0xE5),
        ("AltRight", 0xE6),
        ("MetaRight", 0xE7),
        ("OSLeft", 0xE3),
        ("OSRight", 0xE7),
    ] {
        codes.push((name.into(), usage));
    }
    codes
}

/// The group of a HID usage.
fn key_group(usage: u8) -> CatalogGroup {
    match usage {
        0x04..=0x1D => CatalogGroup::Letters,
        0x1E..=0x27 => CatalogGroup::Digits,
        0x28..=0x2C => CatalogGroup::Basic,
        0x2D..=0x38 => CatalogGroup::Symbols,
        0x3A..=0x45 | 0x68..=0x73 => CatalogGroup::Function,
        0x49..=0x4E => CatalogGroup::Navigation,
        0x4F..=0x52 => CatalogGroup::Arrows,
        0x53..=0x63 => CatalogGroup::Keypad,
        0x64 | 0x87..=0x8B => CatalogGroup::International,
        0xE0..=0xE7 => CatalogGroup::Modifiers,
        _ => CatalogGroup::System,
    }
}

/// The group of a multimedia index.
fn media_group(index: u8) -> CatalogGroup {
    match index {
        0x10..=0x16 => CatalogGroup::Playback,
        0x21..=0x27 => CatalogGroup::Browser,
        _ => CatalogGroup::Apps,
    }
}

/// Both layers with the evidence of `assignment` there.
fn layers(assignment: &KeyAssignment) -> Vec<CatalogLayerView> {
    Layer::ALL
        .into_iter()
        .map(|layer| CatalogLayerView {
            layer: layer_view(layer),
            evidence: evidence_view(assignment.evidence(layer)),
        })
        .collect()
}

/// A catalogue entry.
fn entry(
    model: &Model,
    tab: CatalogTab,
    group: CatalogGroup,
    assignment: KeyAssignment,
    name: String,
    aliases: Vec<String>,
) -> CatalogEntry {
    let value = action_value(&assignment);
    // Any key with a usage gives the same bytes for these kinds.
    let pos = model
        .keys
        .iter()
        .find(|k| k.hid_usage.is_some())
        .and_then(|k| k.pos())
        .unwrap();
    let modifier = matches!(assignment, KeyAssignment::Key(u) if u.is_modifier());
    CatalogEntry {
        id: action_id(&value),
        tab,
        group,
        name,
        aliases,
        hex: hex(&assignment.entry(model, pos).unwrap()),
        layers: layers(&assignment),
        modifier,
        combo_key: matches!(assignment, KeyAssignment::Key(_)) && !modifier,
        value,
    }
}

/// The DKS and Mod-Tap limits of the simulator's W75 (its `21/04`).
fn limits(model: &Model) -> AdvancedLimits {
    let backup = crate::backend::sim::seed_backup();
    let global: TriggerGlobal = backup
        .primary()
        .get(ReadRequest::TriggerGlobal)
        .unwrap()
        .unwrap();
    AdvancedLimits::new(model, &global, 20).unwrap()
}

/// The action catalogue of `model`.
pub(crate) fn action_catalog(model: &'static Model) -> ActionCatalog {
    let codes = key_codes();
    let mut entries = vec![entry(
        model,
        CatalogTab::Default,
        CatalogGroup::Default,
        KeyAssignment::Default,
        "Default".to_string(),
        Vec::new(),
    )];
    for usage in HidUsage::all() {
        let mut aliases: Vec<String> = codes
            .iter()
            .filter(|(_, u)| *u == usage.get())
            .map(|(code, _)| code.clone())
            .collect();
        aliases.push(format!("0x{:02x}", usage.get()));
        entries.push(entry(
            model,
            CatalogTab::Keys,
            key_group(usage.get()),
            KeyAssignment::Key(usage),
            usage.to_string(),
            aliases,
        ));
    }
    for index in MediaKey::ALL {
        let key = MediaKey::new(index).unwrap();
        entries.push(entry(
            model,
            CatalogTab::Media,
            media_group(index),
            KeyAssignment::Media(key),
            key.name().unwrap().to_string(),
            vec![format!("0x{index:02x}")],
        ));
    }
    let mouse = MouseButton::ALL
        .into_iter()
        .map(MouseAction::Button)
        .chain([MouseAction::WheelUp, MouseAction::WheelDown]);
    for action in mouse {
        let group = match action {
            MouseAction::Button(_) => CatalogGroup::MouseButtons,
            _ => CatalogGroup::MouseWheel,
        };
        entries.push(entry(
            model,
            CatalogTab::Mouse,
            group,
            KeyAssignment::Mouse(action),
            mouse_label(action).to_string(),
            Vec::new(),
        ));
    }
    let a = HidUsage::new(0x04).unwrap();
    let modifiers = HidUsage::all()
        .filter(|u| u.is_modifier())
        .map(|m| CatalogModifier {
            usage: m.get(),
            name: m.to_string(),
            combo_evidence: evidence_view(Combo::new(&[m], a).unwrap().evidence()),
        })
        .collect();
    let bindings = vec![
        BindingRuleView {
            kind: SlotKindView::Macro,
            layers: layers(&KeyAssignment::Macro(MacroSlotId::new(0).unwrap())),
        },
        BindingRuleView {
            kind: SlotKindView::Advanced,
            layers: layers(&KeyAssignment::Advanced(AdvancedSlotId::new(0).unwrap())),
        },
    ];
    let limits = limits(model);
    let steps = [
        DksStep::None,
        DksStep::Tap,
        DksStep::Hold,
        DksStep::HoldAgain,
    ];
    let mut dks_rows = Vec::new();
    for n in 0..256usize {
        let row = [0, 1, 2, 3].map(|stage| steps[(n >> (2 * stage)) & 3]);
        let def = AdvancedKeyDef::Dks(Dks {
            first_point: 15,
            rows: [Some(DksRow { key: a, steps: row }), None, None, None],
        });
        if def.validate(&limits).is_ok() {
            dks_rows.push(DksRowRuleView {
                steps: row.iter().map(|s| dks_step_view(*s)).collect(),
                evidence: notes_evidence(&def.evidence()),
            });
        }
    }
    let (mt_min, mt_max) = limits.mt_threshold_range();
    let mt = |threshold_10ms| {
        AdvancedKeyDef::ModTap(ModTap {
            hold: HidUsage::new(0xE0).unwrap(),
            tap: a,
            threshold_10ms,
        })
        .evidence()
    };
    let confirmed_ms: Vec<u16> = (mt_min..=mt_max)
        .filter(|t| mt(*t).is_empty())
        .map(|t| u16::from(t) * 10)
        .collect();
    let other = (mt_min..=mt_max)
        .find(|t| !mt(*t).is_empty())
        .map_or(EvidenceView::Confirmed, |t| notes_evidence(&mt(t)));
    let play_modes = [
        (MacroPlayKindView::Once, MacroPlay::Once),
        (MacroPlayKindView::Times, MacroPlay::Times(3)),
        (MacroPlayKindView::Toggle, MacroPlay::Toggle),
        (MacroPlayKindView::WhileHeld, MacroPlay::WhileHeld),
    ]
    .into_iter()
    .map(|(mode, play)| {
        let program =
            MacroProgram::new(play, vec![MacroStep::press(a, 0), MacroStep::release(a, 0)])
                .unwrap();
        PlayModeRuleView {
            mode,
            evidence: evidence_view(program.evidence()),
        }
    })
    .collect();
    let layout_keys = model
        .keys
        .iter()
        .filter_map(|k| {
            let usage = HidUsage::new(k.hid_usage?)?;
            Some(LayoutKeyActionView {
                index: k.index,
                id: action_id(&action_value(&KeyAssignment::Key(usage))),
            })
        })
        .collect();
    ActionCatalog {
        product: model.product_code.to_string(),
        entries,
        layout_keys,
        modifiers,
        combo: ComboRuleView {
            min_modifiers: 1,
            max_modifiers: 2,
        },
        bindings,
        dks_rows,
        mod_tap: ModTapRuleView {
            min_ms: u16::from(mt_min) * 10,
            max_ms: u16::from(mt_max) * 10,
            step_ms: 10,
            default_ms: confirmed_ms
                .first()
                .copied()
                .unwrap_or(u16::from(mt_min) * 10),
            confirmed_ms,
            other_evidence: other,
        },
        macros: MacroRulesView {
            max_steps: u8::try_from(MacroProgram::MAX_STEPS)
                .unwrap()
                .min(model.write_ceilings.macro_max_steps),
            max_delay_ms: u16::MAX,
            max_times: u8::MAX,
            play_modes,
        },
    }
}

/// A JSON value as a TypeScript literal: one line per field of the top
/// object, one line per element of its arrays.
fn layout(value: &Value) -> String {
    let compact = |v: &Value| serde_json::to_string(v).unwrap();
    let mut out = String::from("{\n");
    if let Value::Object(map) = value {
        for (key, value) in map {
            match value {
                Value::Array(items) if !items.is_empty() => {
                    writeln!(out, "  {}: [", compact(&json!(key))).unwrap();
                    for item in items {
                        writeln!(out, "    {},", compact(item)).unwrap();
                    }
                    out.push_str("  ],\n");
                }
                _ => writeln!(out, "  {}: {},", compact(&json!(key)), compact(value)).unwrap(),
            }
        }
    }
    out.push('}');
    out
}

/// `app/src/generated/actionCatalog.ts`.
pub(crate) fn action_catalog_ts(model: &'static Model) -> String {
    let catalog = serde_json::to_value(action_catalog(model)).unwrap();
    format!(
        "// This file was generated by the bindings test of openwraith-app-core from the\n\
         // protocol crate (offered HID usages, multimedia indices, mouse actions, the DKS step\n\
         // grammar and the library's evidence). Do not edit this file manually.\n\
         import type {{ ActionCatalog }} from \"../ipc/bindings\";\n\n\
         /** Every action OpenWraith can put on a key of the {}, with its rules. */\n\
         export const ACTION_CATALOG: ActionCatalog = {};\n",
        model.name,
        layout(&catalog)
    )
}

/// `app/src/generated/hidCodes.ts`.
pub(crate) fn key_codes_ts() -> String {
    let mut text = String::from(
        "// This file was generated by the bindings test of openwraith-app-core from the core's\n\
         // key-code table (W3C UI Events `code` values of the USB HID keyboard page 0x07).\n\
         // Do not edit this file manually.\n\n\
         /**\n\
         \x20* `KeyboardEvent.code` -> HID usage for the macro recorder: only usages the keyboard\n\
         \x20* accepts. A code that is missing here cannot be recorded.\n\
         \x20*/\n\
         export const KEY_CODE_USAGES: Readonly<Record<string, number>> = {\n",
    );
    for (code, usage) in key_codes() {
        writeln!(text, "  {code}: 0x{usage:02x},").unwrap();
    }
    text.push_str("};\n");
    text
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use openwraith_hid::protocol::W75;

    use super::*;

    #[test]
    fn every_offered_usage_has_a_key_code_and_every_code_an_offered_usage() {
        let codes = key_codes();
        let names: BTreeSet<&str> = codes.iter().map(|(c, _)| c.as_str()).collect();
        assert_eq!(names.len(), codes.len(), "codes are unique");
        for (code, usage) in &codes {
            assert!(HidUsage::new(*usage).is_some(), "{code}");
        }
        for usage in HidUsage::all() {
            assert!(
                codes.iter().any(|(_, u)| *u == usage.get()),
                "{usage} has no code"
            );
        }
        let usage = |code: &str| codes.iter().find(|(c, _)| c == code).map(|(_, u)| *u);
        assert_eq!(usage("KeyA"), Some(0x04));
        assert_eq!(usage("Digit0"), Some(0x27));
        assert_eq!(usage("F12"), Some(0x45));
        assert_eq!(usage("F13"), Some(0x68));
        assert_eq!(usage("Numpad1"), Some(0x59));
        assert_eq!(usage("MetaRight"), Some(0xE7));
    }

    #[test]
    fn the_catalogue_follows_the_library() {
        let catalog = action_catalog(&W75);
        let ids: BTreeSet<&str> = catalog.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids.len(), catalog.entries.len(), "ids are unique");
        // Default, 122 usages, 18 media, 7 mouse actions.
        assert_eq!(catalog.entries.len(), 1 + 122 + 18 + 7);
        let find = |id: &str| catalog.entries.iter().find(|e| e.id == id).unwrap();
        // 80 keys of the layout have a usage (not the knob, not Fn), each an
        // entry of the catalogue.
        assert_eq!(catalog.layout_keys.len(), 80);
        assert!(
            catalog
                .layout_keys
                .iter()
                .all(|k| ids.contains(k.id.as_str()))
        );
        assert_eq!(catalog.layout_keys[0].id, "key:29");
        assert_eq!(find("key:04").hex, "00040000");
        assert_eq!(find("media:24").name, "Browser Back");
        assert_eq!(find("mouse:back").hex, "12000800");
        assert!(find("key:e0").modifier);
        assert!(!find("key:e0").combo_key);
        assert!(find("key:04").aliases.contains(&"KeyA".to_string()));
        // Media other than 12, 24, 25, 32 is not yet verified.
        assert_eq!(find("media:12").layers[0].evidence, EvidenceView::Confirmed);
        assert!(matches!(
            find("media:26").layers[0].evidence,
            EvidenceView::Unverified { .. }
        ));
        // Right-side modifiers in shortcuts are not yet verified.
        let right = catalog.modifiers.iter().find(|m| m.usage == 0xE4).unwrap();
        assert!(matches!(
            right.combo_evidence,
            EvidenceView::Unverified { .. }
        ));
        // Advanced keys on the Fn layer are not yet verified; macros are.
        let advanced = &catalog.bindings[1];
        assert_eq!(advanced.layers[0].evidence, EvidenceView::Confirmed);
        assert!(matches!(
            advanced.layers[1].evidence,
            EvidenceView::Unverified { .. }
        ));
        assert_eq!(
            catalog.bindings[0].layers[1].evidence,
            EvidenceView::Confirmed
        );
        // The DKS grammar: a tap at stage 4 and "hold again" are unverified.
        assert!(!catalog.dks_rows.is_empty());
        assert!(catalog.dks_rows.iter().all(|r| r.steps.len() == 4));
        assert!(
            catalog
                .dks_rows
                .iter()
                .all(|r| r.steps[3] == crate::dto::DksStepView::None
                    || r.steps[3] == crate::dto::DksStepView::Tap)
        );
        assert_eq!(catalog.mod_tap.confirmed_ms, vec![300]);
        assert_eq!(catalog.mod_tap.default_ms, 300);
        assert_eq!(
            (catalog.mod_tap.min_ms, catalog.mod_tap.max_ms),
            (150, 1000)
        );
        assert_eq!(catalog.macros.max_steps, 40);
        assert!(
            catalog
                .macros
                .play_modes
                .iter()
                .all(|p| p.evidence == EvidenceView::Confirmed)
        );
        // Every value of the catalogue is accepted as an action.
        for entry in &catalog.entries {
            assert!(
                crate::ops::keymap::assignment(&entry.value).is_ok(),
                "{}",
                entry.id
            );
            assert_eq!(action_id(&entry.value), entry.id);
            if let ActionValue::Key { usage } = entry.value {
                assert_eq!(
                    entry.combo_key,
                    !HidUsage::new(usage).unwrap().is_modifier()
                );
            }
        }
    }
}
