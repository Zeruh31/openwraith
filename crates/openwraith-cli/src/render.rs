//! Human-readable text output of the views in [`crate::view`].

use std::path::Path;

use openwraith_hid::Backup;
use openwraith_protocol::usage::hid_usage_name;
use openwraith_protocol::{
    AdvancedKey, EntryKind, KeyAction, KeyAssignment, KeyPos, KeymapEntry, KeymapTable, Layer,
    LightDirection, LightEffect, LightModeList, LinkType, MacroContent, MacroPlayMode, MacroStep,
    Model, Rgb, TriggerMode, switch_type_name,
};

use crate::edit_text;
use crate::table::{Table, wrap_list};
use crate::view::{
    AdvancedView, AllView, Binding, ExtrasView, InfoView, KeymapView, LightView, MacrosView,
    SocdView, TriggersView, UndecodedNote, key_name,
};

/// Wrap long lists at this width.
const WIDTH: usize = 100;

/// Builds indented `label  value` blocks with aligned values.
struct Fields(Vec<(String, String)>);

impl Fields {
    fn new() -> Self {
        Fields(Vec::new())
    }

    fn add(&mut self, label: &str, value: impl Into<String>) {
        self.0.push((label.to_string(), value.into()));
    }

    fn render(&self, indent: &str) -> String {
        let width = self
            .0
            .iter()
            .map(|(l, _)| l.chars().count())
            .max()
            .unwrap_or(0);
        self.0
            .iter()
            .map(|(label, value)| format!("{indent}{label:<width$}  {value}\n"))
            .collect()
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// Appended to labels and values whose meaning the spec labels CODE-ONLY,
/// GUESS or contradictory, so that they are not read as certain facts.
const MARK: &str = "*";

/// Explains [`MARK`]. Printed once at the end of a command's output when the
/// output uses the mark.
pub const LEGEND: &str = "* meaning taken from the vendor configurator's code, not yet confirmed \
on the wire; see docs/PROTOCOL.md\n";

/// `label` with the [`MARK`].
fn mark(label: &str) -> String {
    format!("{label}{MARK}")
}

/// Appends the [`LEGEND`] if `text` uses the [`MARK`].
pub(crate) fn with_legend(text: String) -> String {
    if text.contains(MARK) {
        format!("{text}\n{LEGEND}")
    } else {
        text
    }
}

fn profile_suffix(profile: Option<u8>) -> String {
    match profile {
        Some(p) => format!(" (active profile {p})"),
        None => String::new(),
    }
}

/// A length in micrometres as millimetres (`20` -> `0.02 mm`).
fn micrometres(um: u32) -> String {
    if um.is_multiple_of(10) {
        format!("{}.{:02} mm", um / 1000, um % 1000 / 10)
    } else {
        format!("{}.{:03} mm", um / 1000, um % 1000)
    }
}

/// Name of a HID usage, or its hex code.
fn usage_label(usage: u8) -> String {
    hid_usage_name(usage).map_or_else(|| format!("HID {usage:#04x}"), str::to_string)
}

/// `openwraith info`.
pub fn info(view: &InfoView) -> String {
    with_legend(info_body(view))
}

fn info_body(view: &InfoView) -> String {
    let title = match (&view.model, &view.product) {
        (Some(model), Some(product)) => format!("{model} ({product})"),
        (None, Some(product)) => format!("Unknown model ({product})"),
        _ => "Unknown keyboard".to_string(),
    };
    let mut f = Fields::new();
    let firmware = view.firmware.clone().unwrap_or_else(|| "unknown".into());
    match &view.build_date {
        Some(date) => f.add("Firmware", format!("{firmware} (built {date})")),
        None => f.add("Firmware", firmware),
    }
    f.add("Device info", view.info.clone());
    if let Some(usb) = &view.usb {
        let mut text = format!("{:04x}:{:04x}", usb.vendor_id, usb.product_id);
        if let Some(product) = &usb.product {
            text.push_str(&format!(" \"{product}\""));
        }
        if let Some(serial) = &usb.serial {
            text.push_str(&format!(", serial {serial}"));
        }
        f.add("USB", text);
    }
    f.add(
        "Active profile",
        view.active_profile.map_or_else(
            || "unknown".into(),
            |p| format!("{p} (onboard profiles 1-4)"),
        ),
    );
    if let Some(s) = &view.status {
        let link = match s.link_type {
            LinkType::Usb => "USB cable".to_string(),
            LinkType::Wireless => "2.4 GHz".to_string(),
            LinkType::Other(b) => format!("unknown ({b:#04x})"),
        };
        f.add(&mark("Connection"), link);
        if let Some(rate) = &view.poll_rate {
            f.add(
                "Polling rate",
                rate.hz.map_or_else(
                    || format!("code {:#04x}", rate.code),
                    |hz| format!("{hz} Hz"),
                ),
            );
        }
        if let Some(g) = &view.trigger_global {
            let unit = g.unit_micrometres(s.flags_b.high_precision);
            f.add(
                "Travel range",
                format!(
                    "{} - {} (1 unit = {})",
                    micrometres(u32::from(g.min_travel.units()) * unit),
                    micrometres(u32::from(g.max_travel.units()) * unit),
                    micrometres(unit)
                ),
            );
        }
        f.add(
            &mark("Key rollover"),
            if s.six_key_rollover { "6-key" } else { "N-key" },
        );
        let w = &s.win_lock;
        let blocked: Vec<&str> = [
            (w.block_win, "Windows key"),
            (w.block_shift_tab, "Shift+Tab"),
            (w.block_alt_f4, "Alt+F4"),
            (w.block_alt_tab, "Alt+Tab"),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, name)| *name)
        .collect();
        f.add(
            &mark("Win-Lock blocks"),
            if blocked.is_empty() {
                "nothing".to_string()
            } else {
                blocked.join(", ")
            },
        );
        let a = &s.flags_a;
        let b = &s.flags_b;
        // (flag, name, meaning CODE-ONLY per spec section 4.2)
        let features: Vec<String> = [
            (a.custom_light, "per-key lighting", false),
            (s.socd_supported, "SOCD", false),
            (a.more_switch, "switch types", false),
            (a.poll_rate, "polling rate", false),
            (a.dead_band, "dead band", false),
            (a.any_key_calibration, "any-key calibration", true),
            (a.rollover_switch, "N-key/6-key switch", true),
            (a.music_rhythm, "music rhythm", true),
            (b.gamepad, "gamepad", true),
            (b.rs, "RS keys", true),
            (b.rkrt, "RKRT keys", true),
            (b.mds, "MDS keys", true),
            (b.high_precision, "high-precision travel", false),
        ]
        .iter()
        .filter(|(on, _, _)| *on)
        .map(|(_, name, code_only)| {
            if *code_only {
                mark(name)
            } else {
                (*name).to_string()
            }
        })
        .collect();
        f.add("Features", features.join(", "));
        f.add(
            "Capability bytes",
            format!("A = {:#04x}, B = {:#04x}", a.raw, b.raw),
        );
    }
    let source = match &view.created_at {
        Some(created) => format!("backup {} (created {created})", view.source),
        None => format!("{} (read-only)", view.source),
    };
    f.add("Source", source);
    format!("{title}\n{}", f.render("  "))
}

fn mode_cell(mode: TriggerMode) -> String {
    match mode {
        TriggerMode::Off => "off".into(),
        TriggerMode::RapidTrigger => "on".into(),
        TriggerMode::RapidTriggerSeparate => "on (separate)".into(),
        TriggerMode::Unknown(b) => format!("? {b:#04x}"),
    }
}

/// `openwraith show triggers`.
pub fn triggers(view: &TriggersView, unit_um: Option<u32>) -> String {
    with_legend(triggers_body(view, unit_um))
}

fn triggers_body(view: &TriggersView, unit_um: Option<u32>) -> String {
    let mut out = format!("Triggers{}\n", profile_suffix(view.active_profile));
    if let Some(g) = view.trigger_global {
        out.push_str(&format!(
            "  Actuation range {} - {}\n",
            g.min_travel, g.max_travel
        ));
    }
    if let Some(unit) = unit_um.filter(|u| *u != 20) {
        out.push_str(&format!(
            "  Warning: the keyboard reports a trigger unit of {}; the values below assume 0.02 mm.\n",
            micrometres(unit)
        ));
    }
    if view.keys.is_empty() {
        out.push_str("  No trigger settings were read.\n");
        return out;
    }
    // The top/bottom order of the dead band is CODE-ONLY (spec 4.12).
    let dead_band_header = mark("Dead band top/bottom");
    let mut table = Table::new(&[
        "Key",
        "Idx",
        "Pos",
        "Actuation",
        "RT",
        "Press",
        "Release",
        &dead_band_header,
    ])
    .right_align(&[1]);
    for k in &view.keys {
        let band = k
            .dead_band
            .map(|b| format!("{} / {}", b.top, b.bottom))
            .unwrap_or_else(|| "-".into());
        let mut cells = vec![
            k.name.clone(),
            k.index.to_string(),
            format!("r{}c{}", k.row, k.col),
        ];
        match &k.trigger {
            Some(t) => cells.extend([
                t.actuation.to_string(),
                mode_cell(t.mode),
                t.rt_press.to_string(),
                t.rt_release.to_string(),
            ]),
            // The reply could not be decoded (see the warning at the end).
            None => cells.extend(["?", "?", "?", "?"].map(String::from)),
        }
        cells.push(band);
        table.row(cells);
    }
    out.push('\n');
    out.push_str(&table.render("  "));
    out.push('\n');

    let decoded: Vec<(&str, &openwraith_protocol::KeyTrigger)> = view
        .keys
        .iter()
        .filter_map(|k| k.trigger.as_ref().map(|t| (k.name.as_str(), t)))
        .collect();
    let rt: Vec<&str> = decoded
        .iter()
        .filter(|(_, t)| t.rapid_trigger())
        .map(|(name, _)| *name)
        .collect();
    let min = decoded.iter().map(|(_, t)| t.actuation).min();
    let max = decoded.iter().map(|(_, t)| t.actuation).max();
    out.push_str(&format!("  {} keys", view.keys.len()));
    let missing = view.keys.len() - decoded.len();
    if missing > 0 {
        out.push_str(&format!(" ({missing} not decoded, shown as ?)"));
    }
    if let (Some(min), Some(max)) = (min, max) {
        if min == max {
            out.push_str(&format!(", all actuate at {min}"));
        } else {
            out.push_str(&format!(", actuation {min} - {max}"));
        }
    }
    if rt.is_empty() {
        out.push_str("; Rapid Trigger is off on every key.\n");
    } else {
        out.push_str(&format!(
            "; Rapid Trigger on {} key(s): {}.\n",
            rt.len(),
            rt.join(", ")
        ));
    }
    out.push_str(
        "  Press/Release are the Rapid Trigger sensitivities; the keyboard keeps them while RT is off.\n",
    );
    let odd: Vec<String> = decoded
        .iter()
        .filter(|(_, t)| t.second_travel != t.actuation)
        .map(|(name, t)| format!("{name} ({})", t.second_travel))
        .collect();
    if !odd.is_empty() {
        out.push_str(&format!(
            "  Second travel value differs from the actuation point on: {}\n",
            odd.join(", ")
        ));
    }
    out
}

fn direction_name(direction: LightDirection) -> String {
    match direction {
        LightDirection::Right => "right".into(),
        LightDirection::Left => "left".into(),
        LightDirection::Up => "up".into(),
        LightDirection::Down => "down".into(),
        LightDirection::Spread => "spread".into(),
        LightDirection::Gather => "gather".into(),
        LightDirection::Unknown(b) => format!("unknown ({b:#04x})"),
    }
}

fn effect_block(effect: &LightEffect, modes: Option<&LightModeList>) -> String {
    let of_max = |value: u8, max: Option<u8>| match max {
        Some(max) => format!("{value} of {max}"),
        None => value.to_string(),
    };
    let mut f = Fields::new();
    f.add(
        "Mode",
        format!("{} ({:#04x})", effect.mode, effect.mode.id()),
    );
    f.add(
        "Brightness",
        of_max(effect.brightness, modes.map(|m| m.max_brightness)),
    );
    f.add("Speed", of_max(effect.speed, modes.map(|m| m.max_speed)));
    f.add("Foreground", effect.foreground.to_string());
    // Spec 4.7.2: background, direction and power are CODE-ONLY (only one
    // value ever observed); the meaning of "full colour" is a GUESS.
    f.add(&mark("Background"), effect.background.to_string());
    f.add(&mark("Direction"), direction_name(effect.direction));
    f.add(&mark("Full colour"), on_off(effect.full_color));
    f.add(&mark("Power"), on_off(effect.power_on));
    f.add("Raw record", effect.raw.to_spaced_hex());
    let mut out = f.render("  ");
    if let Some(modes) = modes
        && !modes.modes.is_empty()
    {
        let names: Vec<String> = modes.modes.iter().map(|m| m.name()).collect();
        out.push_str("  Modes listed by the keyboard:\n");
        out.push_str(&wrap_list(&names, "    ", WIDTH));
    }
    out
}

/// Lit slots of a colour table grouped by colour, in slot order.
fn colour_groups(colors: &[Rgb]) -> Vec<(Rgb, Vec<u8>)> {
    let mut groups: Vec<(Rgb, Vec<u8>)> = Vec::new();
    for (index, colour) in (0u8..).zip(colors) {
        if colour.is_black() {
            continue;
        }
        match groups.iter_mut().find(|(c, _)| c == colour) {
            Some((_, slots)) => slots.push(index),
            None => groups.push((*colour, vec![index])),
        }
    }
    groups
}

/// `openwraith show light`.
pub fn light(view: &LightView, model: Option<&Model>) -> String {
    with_legend(light_body(view, model))
}

fn light_body(view: &LightView, model: Option<&Model>) -> String {
    let mut out = String::from("Main light\n");
    match view.main_light {
        Some(effect) => out.push_str(&effect_block(effect, view.main_light_modes)),
        None => out.push_str("  Not read.\n"),
    }
    out.push_str("\nSide light\n");
    match (view.side_light, view.side_light_modes) {
        (Some(effect), Some(modes)) if modes.modes.is_empty() => {
            out.push_str(
                "  The keyboard lists no side-light modes (no side light to configure).\n",
            );
            out.push_str(&format!(
                "  It still answers the side-light read: {}\n",
                effect.raw.to_spaced_hex()
            ));
        }
        (Some(effect), modes) => out.push_str(&effect_block(effect, modes)),
        (None, _) => out.push_str("  Not read.\n"),
    }

    out.push_str("\nCustom colour tables (per-key lighting)\n");
    if let Some(active) = view.active_custom_table {
        let name = match active.table {
            Some(table) => format!("Custom {}", table.number() + 1),
            None => format!("unknown ({:#04x})", active.slot),
        };
        // Spec 4.7.6: meaning CODE-ONLY, and the reply is byte-identical
        // to a colour-write ack.
        out.push_str(&format!("  {}: {name}\n", mark("Active table")));
    }
    if view.custom_colors.is_empty() {
        out.push_str("  Not read (the keyboard does not report per-key lighting).\n");
    }
    let mut outside_layout = false;
    for table in &view.custom_colors {
        let lit_slots = table.lit_indices();
        outside_layout |= lit_slots
            .iter()
            .any(|i| model.is_none_or(|m| m.key(*i).is_none()));
        let lit = lit_slots.len();
        out.push_str(&format!(
            "  Custom {}: {lit} of {} slots lit\n",
            table.table.number() + 1,
            table.colors.len()
        ));
        for (colour, slots) in colour_groups(&table.colors) {
            out.push_str(&format!("    {colour} ({} slots):\n", slots.len()));
            let names: Vec<String> = slots.iter().map(|i| key_name(model, *i)).collect();
            out.push_str(&wrap_list(&names, "      ", WIDTH));
        }
    }
    if outside_layout {
        out.push_str("  Slots named #N are not keys of the layout but still hold a colour.\n");
    }
    out
}

fn layer_name(layer: Layer) -> &'static str {
    match layer {
        Layer::Base => "Base layer",
        Layer::Fn => "Fn layer",
    }
}

/// `openwraith show keymap`.
pub fn keymap(view: &KeymapView, model: Option<&Model>) -> String {
    with_legend(keymap_body(view, model))
}

fn keymap_body(view: &KeymapView, model: Option<&Model>) -> String {
    let mut out = format!("Keymap{}\n", profile_suffix(view.active_profile));
    if let Some(locked) = view.fn_locked_keys {
        let names: Vec<String> = locked
            .indices()
            .iter()
            .map(|i| format!("{} ({i})", key_name(model, *i)))
            .collect();
        // Spec 4.3: the meaning of the 0x0E mask is CODE-ONLY.
        out.push_str(&format!(
            "  {}: {}\n",
            mark("Keys that cannot be remapped on the Fn layer"),
            if names.is_empty() {
                "none".to_string()
            } else {
                names.join(", ")
            }
        ));
    }
    if view.layers.is_empty() {
        out.push_str("  Not read.\n");
    }
    for layer in &view.layers {
        out.push_str(&layer_block(layer, model));
    }
    if let Some(model) = model.filter(|_| !view.layers.is_empty()) {
        if let Some(knob) = model.knob_index {
            out.push_str(&format!(
                "  The knob's entries (#{knob}-#{}) are managed by the keyboard: OpenWraith shows \
                 them but never changes them.\n",
                knob + 6
            ));
        }
        out.push_str(&edit_text::fn_builtins_text(model).replacen(
            "Built-in Fn functions",
            &mark("Built-in Fn functions"),
            1,
        ));
    }
    out
}

/// What an override row means for an editor: the knob and the Fn key are
/// kept as read, an entry OpenWraith does not write is kept as read, an
/// Fn-layer entry replaces a built-in function.
fn entry_note(layer: Layer, entry: &KeymapEntry, model: Option<&Model>) -> String {
    let Some(model) = model else {
        return String::new();
    };
    let index = entry.index;
    if model
        .knob_index
        .is_some_and(|knob| (knob..=knob + 6).contains(&index))
    {
        return "knob, kept as read".to_string();
    }
    if model.fn_index == Some(index) {
        return "the Fn key, kept as read".to_string();
    }
    let (Some(pos), Ok(bytes)) = (
        KeyPos::from_index(index),
        <[u8; 4]>::try_from(entry.raw.0.as_slice()),
    ) else {
        return String::new();
    };
    match KeyAssignment::classify(model, pos, bytes) {
        EntryKind::Preserved(_) => "kept as read (a kind OpenWraith does not write)".to_string(),
        EntryKind::Assignable(_) if layer == Layer::Fn => model
            .fn_builtin(index)
            .map(|b| format!("replaces the built-in function \"{}\"", b.function))
            .unwrap_or_default(),
        EntryKind::Assignable(_) => String::new(),
    }
}

fn layer_block(layer: &KeymapTable, model: Option<&Model>) -> String {
    let overrides: Vec<_> = layer.overrides().collect();
    if overrides.is_empty() {
        return format!(
            "  {}: no remapped keys (all {} entries are 00 00 00 00, the keyboard's defaults apply)\n",
            layer_name(layer.layer),
            layer.entries.len()
        );
    }
    let mut out = format!(
        "  {}: {} remapped position(s)\n",
        layer_name(layer.layer),
        overrides.len()
    );
    let mut table = Table::new(&["Key", "Idx", "Action", "Raw", "Note"]).right_align(&[1]);
    for entry in overrides {
        table.row(vec![
            key_name(model, entry.index),
            entry.index.to_string(),
            entry.action.to_string(),
            entry.raw.to_spaced_hex(),
            entry_note(layer.layer, entry, model),
        ]);
    }
    out.push_str(&table.render("    "));
    out
}

fn play_mode(content: &MacroContent) -> String {
    match content.play_mode {
        MacroPlayMode::Once => "play once".into(),
        MacroPlayMode::Times => format!("play {} times", content.repeat_count),
        MacroPlayMode::Toggle => "press to start, press again to stop".into(),
        MacroPlayMode::WhileHeld => "repeat while held".into(),
        MacroPlayMode::Unknown(b) => format!("unknown play mode {b:#04x}"),
    }
}

fn step_text(step: &MacroStep) -> String {
    let action = if step.is_press() { "press" } else { "release" };
    let target = if step.kind() == 0 {
        usage_label(step.code)
    } else {
        format!("code {:#04x} (non-keyboard)", step.code)
    };
    if step.delay_ms > 0 {
        format!("{action} {target}, then wait {} ms", step.delay_ms)
    } else {
        format!("{action} {target}")
    }
}

/// The steps of a macro of keyboard steps in the notation of `set macro
/// --steps` (`A+ 30, A-`); `None` when a step is not a keyboard step.
fn steps_notation(content: &MacroContent) -> Option<String> {
    let steps: Option<Vec<String>> = content
        .steps
        .iter()
        .map(|step| {
            if step.kind() != 0 {
                return None;
            }
            let name = openwraith_protocol::usage::hid_usage_name(step.code)?;
            let motion = if step.is_press() { '+' } else { '-' };
            Some(if step.delay_ms > 0 {
                format!("{name}{motion} {}", step.delay_ms)
            } else {
                format!("{name}{motion}")
            })
        })
        .collect();
    steps.filter(|s| !s.is_empty()).map(|s| s.join(", "))
}

/// `bound to Caps Lock (base layer)` for the bindings of `slot`.
fn bound_text(bindings: &[Binding], slot: u8) -> String {
    let keys: Vec<String> = bindings
        .iter()
        .filter(|b| b.slot == slot)
        .map(|b| format!("{} ({})", b.key, layer_name(b.layer)))
        .collect();
    if keys.is_empty() {
        "bound to no key (the keyboard keeps it)".to_string()
    } else {
        format!("bound to {}", keys.join(", "))
    }
}

/// `openwraith show macros`.
pub fn macros(view: &MacrosView) -> String {
    let used = view.slots.iter().filter(|s| !s.is_empty()).count();
    let mut out = format!(
        "Macros{}: {used} of {} slots used\n",
        profile_suffix(view.active_profile),
        view.slots.len()
    );
    for slot in &view.slots {
        match &slot.content {
            None => out.push_str(&format!("  Slot {}: empty\n", slot.slot)),
            Some(content) => {
                out.push_str(&format!(
                    "  Slot {}: macro #{}{}, {}, {} step(s); {}\n",
                    slot.slot,
                    content.macro_id,
                    if content.copy { " (copy)" } else { "" },
                    play_mode(content),
                    content.steps.len(),
                    bound_text(&view.bindings, slot.slot)
                ));
                for (i, step) in content.steps.iter().enumerate() {
                    out.push_str(&format!("      {}. {}\n", i + 1, step_text(step)));
                }
                if let Some(steps) = steps_notation(content) {
                    out.push_str(&format!("      As steps: \"{steps}\"\n"));
                }
            }
        }
    }
    out.push_str("  Macro names are not stored on the keyboard.\n");
    out
}

pub(crate) fn advanced_text(key: &AdvancedKey) -> String {
    match key {
        AdvancedKey::Empty => "empty".into(),
        AdvancedKey::Dks {
            press_points,
            actions,
            ..
        } => {
            let points: Vec<String> = press_points
                .iter()
                .map(|p| format!("{}.{} mm", p / 10, p % 10))
                .collect();
            let actions: Vec<String> = actions
                .iter()
                .filter(|a| !a.is_default())
                .map(KeyAction::to_string)
                .collect();
            format!(
                "DKS, press points {}, actions: {}",
                points.join(" / "),
                if actions.is_empty() {
                    "none".to_string()
                } else {
                    actions.join(", ")
                }
            )
        }
        AdvancedKey::ModTap {
            hold,
            tap,
            hold_threshold_10ms,
        } => format!(
            "Mod-tap: tap {tap}, hold {hold} (hold after {} ms)",
            u32::from(*hold_threshold_10ms) * 10
        ),
        AdvancedKey::Toggle { action } => format!("Toggle {action}"),
        AdvancedKey::Unknown { kind } => format!("unknown type {kind}"),
        other => format!("{other:?}"),
    }
}

/// `openwraith show advanced`.
pub fn advanced(view: &AdvancedView) -> String {
    let mut out = format!("Advanced keys{}", profile_suffix(view.active_profile));
    let Some(keys) = view.advanced_keys else {
        out.push_str("\n  Not read.\n");
        return out;
    };
    out.push_str(&format!(
        ": {} of {} slots used\n",
        view.slots.len(),
        keys.slots.len()
    ));
    for row in &view.slots {
        let what = match &row.definition {
            Some(definition) => definition.clone(),
            None => keys
                .slots
                .iter()
                .find(|s| s.slot == row.slot)
                .map(|s| format!("{} (kept as read)", advanced_text(&s.key)))
                .unwrap_or_default(),
        };
        out.push_str(&format!(
            "  Slot {}: {what}; {}\n",
            row.slot,
            bound_text(&row.bound_to, row.slot)
        ));
    }
    out.push_str(
        "  Change one with `openwraith set advanced KEY dks|mod-tap|toggle ...`; clear an unbound \
         slot with `openwraith set advanced --slot N --clear`.\n",
    );
    out
}

/// `openwraith show socd`.
pub fn socd(view: &SocdView) -> String {
    let mut out = format!("SOCD{}\n", profile_suffix(view.active_profile));
    let Some(settings) = view.socd_settings else {
        out.push_str("  Not read (the keyboard does not report SOCD support).\n");
        return out;
    };
    out.push_str(&format!(
        "  Power (all profiles): {} (Fn + Right Shift also switches it on the keyboard)\n",
        if settings.enabled { "on" } else { "off" }
    ));
    match &view.state {
        Some(state) => match &state.pairs {
            Some(pairs) if pairs.is_empty() => out.push_str("  Pairs: none\n"),
            Some(pairs) => {
                out.push_str("  Pairs (key 1 / key 2, what happens while both are held):\n");
                for (i, pair) in pairs.iter().enumerate() {
                    out.push_str(&format!(
                        "    {}. {}\n",
                        i + 1,
                        edit_text::socd_pair_text(pair)
                    ));
                }
            }
            None => out.push_str(&format!(
                "  Pairs: not in the form OpenWraith writes ({}); raw {}\n",
                state.unwritable.unwrap_or("unreadable"),
                state.raw_pairs
            )),
        },
        None => out.push_str("  Pairs: not read.\n"),
    }
    let [b1, b2] = settings.hotkey;
    out.push_str(&format!(
        "  {}: bytes {b1:02x} {b2:02x} (never written)\n",
        mark("Hotkey")
    ));
    out.push_str(
        "  The power is one setting of the whole keyboard; the pairs belong to each profile. \
         OpenWraith writes the power and the pairs together, as the vendor site does: \
         `openwraith set socd --on --pair A/D`.\n",
    );
    out
}

/// Advanced keys, SOCD and switch types.
pub fn extras(view: &ExtrasView, keymaps: &[&KeymapTable], model: Option<&Model>) -> String {
    let mut out = String::from("Advanced keys\n");
    match view.advanced_keys {
        None => out.push_str("  Not read.\n"),
        Some(adv) => {
            let used: Vec<_> = adv.used().collect();
            if used.is_empty() {
                out.push_str(&format!(
                    "  None of the {} slots is used.\n",
                    adv.slots.len()
                ));
            }
            for slot in used {
                let bound: Vec<String> = keymaps
                    .iter()
                    .flat_map(|layer| {
                        layer
                            .entries
                            .iter()
                            .filter(|e| e.action == KeyAction::Advanced { slot: slot.slot })
                            .map(|e| {
                                format!(
                                    "{} ({})",
                                    key_name(model, e.index),
                                    layer_name(layer.layer)
                                )
                            })
                    })
                    .collect();
                out.push_str(&format!(
                    "  Slot {}: {}; bound to {}\n",
                    slot.slot,
                    advanced_text(&slot.key),
                    if bound.is_empty() {
                        "no key".to_string()
                    } else {
                        bound.join(", ")
                    }
                ));
            }
        }
    }

    out.push_str("\nSOCD\n");
    match view.socd_settings {
        Some(socd) => {
            // Spec 4.15: the purpose of these bytes is a GUESS; the vendor
            // site hard-codes 0xFA as "Fn".
            let hotkey: Vec<String> = socd
                .hotkey
                .iter()
                .map(|b| {
                    if *b == 0xFA {
                        "Fn".to_string()
                    } else {
                        usage_label(*b)
                    }
                })
                .collect();
            let [b1, b2] = socd.hotkey;
            out.push_str(&format!(
                "  Power (all profiles): {}; hotkey bytes {b1:02x} {b2:02x}, shown by the vendor \
                 as {}\n",
                if socd.enabled { "On" } else { "Off" },
                mark(&hotkey.join(" + "))
            ));
        }
        None => out.push_str("  Not read (the keyboard does not report SOCD support).\n"),
    }
    if let Some(pairs) = view.socd_pairs {
        let used: Vec<_> = pairs.used().collect();
        if used.is_empty() {
            out.push_str("  No key pairs defined.\n");
        }
        for pair in used {
            out.push_str(&format!(
                "  Pair {}: {} / {}, model {}\n",
                pair.index,
                usage_label(pair.key1),
                usage_label(pair.key2),
                pair.model
            ));
        }
    }

    out.push_str("\nSwitch types\n");
    match (view.switch_type_list, view.switch_types) {
        (None, None) => out.push_str("  Not read.\n"),
        (list, table) => {
            if let Some(list) = list {
                let names: Vec<String> = list
                    .ids
                    .iter()
                    .map(|id| {
                        switch_type_name(*id).map_or_else(|| format!("type {id}"), str::to_string)
                    })
                    .collect();
                out.push_str(&format!("  Supported: {}\n", names.join(", ")));
            }
            if let (Some(table), Some(model)) = (table, model) {
                let mut counts: Vec<(u8, usize)> = Vec::new();
                for key in model.keys {
                    let id = table
                        .per_key
                        .get(usize::from(key.index))
                        .copied()
                        .unwrap_or(0);
                    match counts.iter_mut().find(|(i, _)| *i == id) {
                        Some((_, n)) => *n += 1,
                        None => counts.push((id, 1)),
                    }
                }
                for (id, n) in counts {
                    let name =
                        switch_type_name(id).map_or_else(|| format!("type {id}"), str::to_string);
                    out.push_str(&format!("  {n} keys: {name}\n"));
                }
            }
        }
    }
    out
}

/// `openwraith show all`.
pub fn all(view: &AllView, model: Option<&Model>, unit_um: Option<u32>) -> String {
    with_legend(
        [
            info_body(&view.info),
            light_body(&view.light, model),
            triggers_body(&view.triggers, unit_um),
            keymap_body(&view.keymap, model),
            macros(&view.macros),
            extras(&view.extras, &view.keymap.layers, model),
        ]
        .join("\n"),
    )
}

/// A warning listing the replies that were kept raw because they could not
/// be decoded; empty when there are none.
pub fn undecoded_footer(notes: &[UndecodedNote]) -> String {
    if notes.is_empty() {
        return String::new();
    }
    let count = match notes.len() {
        1 => "1 reply".to_string(),
        n => format!("{n} replies"),
    };
    let mut out = format!("\nWarning: {count} could not be decoded (the raw bytes are kept):\n");
    for note in notes {
        out.push_str(&format!("  {}: {}\n", note.description, note.error));
    }
    out
}

/// Short summary printed after `openwraith backup`.
pub fn backup_summary(backup: &Backup, path: &Path) -> String {
    let mut f = Fields::new();
    let product = backup.device.product.clone().unwrap_or_else(|| "?".into());
    f.add(
        "Keyboard",
        format!(
            "{} ({product}), firmware {}",
            backup
                .device
                .model
                .clone()
                .unwrap_or_else(|| "unknown model".into()),
            backup.device.firmware.clone().unwrap_or_else(|| "?".into())
        ),
    );
    let active = backup
        .active_profile
        .map_or_else(|| "unknown".to_string(), |p| p.to_string());
    if backup.is_all_profiles() {
        let held: Vec<String> = backup
            .profile_numbers()
            .iter()
            .map(ToString::to_string)
            .collect();
        f.add(
            "Profiles",
            format!(
                "{} (profile {active} was active and is summarised below; the keyboard is back \
                 on it)",
                held.join(", ")
            ),
        );
    } else {
        f.add(
            "Profile",
            match backup.active_profile {
                Some(p) => format!("{p} (only the active onboard profile is included)"),
                None => "unknown".into(),
            },
        );
    }
    let snapshots = backup.snapshots();
    let replies: usize = snapshots.iter().map(|s| s.entries().len()).sum();
    let reports: usize = snapshots
        .iter()
        .flat_map(|s| s.entries())
        .map(|e| e.raw.reports().len())
        .sum();
    f.add(
        "Contents",
        format!("{replies} replies, {reports} raw reports"),
    );
    let triggers = backup.key_triggers();
    let rt = triggers.iter().filter(|t| t.rapid_trigger()).count();
    f.add(
        "Triggers",
        format!("{} keys, Rapid Trigger on {rt}", triggers.len()),
    );
    if let Some(light) = backup.main_light() {
        f.add(
            "Lighting",
            format!(
                "{}, {}, brightness {}",
                light.mode, light.foreground, light.brightness
            ),
        );
    }
    let remaps: Vec<String> = backup
        .keymaps()
        .iter()
        .map(|l| format!("{} {}", l.overrides().count(), layer_name(l.layer)))
        .collect();
    f.add("Remapped keys", remaps.join(", "));
    let macros = backup.macros();
    f.add(
        "Macros",
        format!(
            "{} of {} slots used",
            macros.iter().filter(|m| !m.is_empty()).count(),
            macros.len()
        ),
    );
    if let Some(adv) = backup.advanced_keys() {
        f.add(
            "Advanced keys",
            format!("{} of {} slots used", adv.used().count(), adv.slots.len()),
        );
    }
    format!("Backup written to {}\n{}", path.display(), f.render("  "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{backup_from, c06_backup, replace_reply, replace_table};
    use crate::view::{KeymapView, undecoded_notes};
    use openwraith_protocol::{KeyPos, MacroSlotId, REPORT_LEN, ReadRequest, Report};

    #[test]
    fn info_from_capture() {
        let backup = c06_backup();
        let text = info(&InfoView::from_backup(&backup, None));
        assert!(text.starts_with("Wraith W75 (W75HEARGB)\n"), "{text}");
        assert!(
            text.contains("3.22.07 (built Aug  6 2026,13:46:54)"),
            "{text}"
        );
        assert!(text.contains("2e3c:c368 \"Wraith W75\""), "{text}");
        assert!(text.contains("1 (onboard profiles 1-4)"), "{text}");
        assert!(text.contains("8000 Hz"), "{text}");
        assert!(
            text.contains("0.10 mm - 3.40 mm (1 unit = 0.02 mm)"),
            "{text}"
        );
        assert!(text.contains("Windows key"), "{text}");
        assert!(text.contains("keyboard (read-only)"), "{text}");
        // CODE-ONLY fields are marked and explained.
        assert!(text.contains("Connection*"), "{text}");
        assert!(text.contains("Win-Lock blocks*"), "{text}");
        assert!(text.contains("Key rollover*"), "{text}");
        assert!(text.contains("gamepad*"), "{text}");
        assert!(text.contains("per-key lighting, "), "{text}");
        assert!(text.ends_with(LEGEND), "{text}");
    }

    #[test]
    fn triggers_from_capture() {
        let backup = c06_backup();
        let text = triggers(&TriggersView::new(&backup), Some(20));
        let w_line = text
            .lines()
            .find(|l| l.trim_start().starts_with("W "))
            .expect("W row");
        assert!(w_line.contains("r2c2"), "{w_line}");
        assert!(w_line.contains("1.00 mm"), "{w_line}");
        assert!(w_line.contains(" on "), "{w_line}");
        assert_eq!(w_line.matches("0.30 mm").count(), 2, "{w_line}");
        assert!(
            text.contains("82 keys, actuation 1.00 mm - 1.70 mm"),
            "{text}"
        );
        assert!(text.contains("Rapid Trigger on 1 key(s): W."), "{text}");
        assert!(!text.contains("Warning"), "{text}");
        assert!(text.contains("Dead band top/bottom*"), "{text}");
        assert!(text.ends_with(LEGEND), "{text}");
        // 82 key rows plus the header row start with two spaces and a name.
        assert_eq!(text.lines().filter(|l| l.contains(" r")).count(), 83);
    }

    #[test]
    fn undecodable_trigger_replies_keep_their_row_and_are_reported() {
        let mut backup = c06_backup();
        // Esc: header and row/col echo intact, but only 3 bytes declared.
        let esc = ReadRequest::KeyTrigger(KeyPos::new(0, 0).unwrap());
        let mut bytes = [0u8; REPORT_LEN];
        bytes[..9].copy_from_slice(&[0x21, 0, 0, 0, 0x03, 0x05, 0x00, 0x55, 0x55]);
        replace_reply(&mut backup, esc, vec![Report::new(bytes)]);
        let notes = undecoded_notes(&backup);
        assert_eq!(notes.len(), 1);

        let view = TriggersView::new(&backup);
        assert_eq!(view.keys.len(), 82);
        let esc_row = view.keys.iter().find(|k| k.index == 0).expect("Esc row");
        assert!(esc_row.trigger.is_none());
        assert_eq!(
            esc_row.raw.as_ref().map(|r| r.0.clone()),
            Some(vec![0x05, 0x00, 0x55])
        );

        let text = triggers(&view, Some(20)) + &undecoded_footer(&notes);
        let esc_line = text
            .lines()
            .find(|l| l.trim_start().starts_with("Esc "))
            .expect("Esc line");
        assert!(esc_line.contains(" ? "), "{esc_line}");
        assert!(
            text.contains("82 keys (1 not decoded, shown as ?)"),
            "{text}"
        );
        assert!(
            text.contains("Warning: 1 reply could not be decoded"),
            "{text}"
        );
        assert!(
            text.contains("key trigger r0c0 (21/05): key trigger record: need 12 byte(s), got 3"),
            "{text}"
        );
        assert_eq!(undecoded_footer(&[]), "");

        // Without any trigger reply there are no placeholder rows.
        backup
            .entries
            .retain(|e| !matches!(e.request(), ReadRequest::KeyTrigger(_)));
        let text = triggers(&TriggersView::new(&backup), Some(20));
        assert!(text.contains("No trigger settings were read."), "{text}");
    }

    #[test]
    fn light_from_capture() {
        let backup = c06_backup();
        let text = light(&LightView::new(&backup), backup.model());
        assert!(text.contains("Custom (per-key) (0x0a)"), "{text}");
        assert!(text.contains("Foreground    #ff0000"), "{text}");
        assert!(text.contains("Direction*"), "{text}");
        assert!(text.contains("Full colour*"), "{text}");
        assert!(text.contains("4 of 4"), "{text}");
        assert!(text.contains("no side-light modes"), "{text}");
        assert!(text.contains("Active table*: Custom 1"), "{text}");
        assert!(text.ends_with(LEGEND), "{text}");
        assert!(text.contains("Custom 1: 81 of 132 slots lit"), "{text}");
        assert!(text.contains("#ff0000 (81 slots):"), "{text}");
        assert!(text.contains("Custom 2: 34 of 132 slots lit"), "{text}");
        assert!(!text.contains("Slots named #N"), "{text}");
    }

    #[test]
    fn factory_colour_slots_outside_the_layout_are_shown() {
        // c01: factory Custom1 = W, A, S, D, #103, Down, Right, #126 (spec 4.7.6).
        let backup = backup_from("capture_01_connect");
        let text = light(&LightView::new(&backup), backup.model());
        assert!(text.contains("Custom 1: 8 of 132 slots lit"), "{text}");
        assert!(
            text.contains("W, A, S, D, #103, Down, Right, #126"),
            "{text}"
        );
        assert!(text.contains("Slots named #N"), "{text}");
        assert!(text.contains("Mode          Wave (0x02)"), "{text}");
    }

    #[test]
    fn default_keymap_and_macros() {
        let backup = c06_backup();
        let text = keymap(&KeymapView::new(&backup), backup.model());
        assert!(text.contains("Fn layer*: Fn (121)"), "{text}");
        assert!(text.contains("Base layer: no remapped keys"), "{text}");
        assert!(text.ends_with(LEGEND), "{text}");
        let text = macros(&MacrosView::new(&backup));
        assert!(text.contains("0 of 10 slots used"), "{text}");
        assert!(text.contains("Slot 9: empty"), "{text}");
        // Nothing in the macro view is marked.
        assert!(!text.contains(LEGEND), "{text}");
    }

    #[test]
    fn remapped_key_and_macro_are_shown() {
        let mut backup = c06_backup();
        // Base layer: W (index 46, offset 184) -> A; Caps (66) -> macro slot 0.
        let mut keymap_table = vec![0u8; 528];
        keymap_table[184..188].copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);
        keymap_table[264..268].copy_from_slice(&[0x10, 0x39, 0x00, 0x00]);
        replace_table(
            &mut backup,
            ReadRequest::Keymap(Layer::Base),
            0x18,
            0x80,
            &keymap_table,
        );
        // Spec 4.9.3 example: play once, "A down, 30 ms, A up".
        let mut slot = vec![0u8; 256];
        slot[..16].copy_from_slice(&[
            0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00, 0x00, 0x04, 0x10, 0x00, 0x1e, 0x04, 0x00,
            0x00, 0x00,
        ]);
        let slot0 = ReadRequest::Macro(MacroSlotId::new(0).unwrap());
        replace_table(&mut backup, slot0, 0x19, 0x80, &slot);

        let text = keymap(&KeymapView::new(&backup), backup.model());
        assert!(
            text.contains("Base layer: 2 remapped position(s)"),
            "{text}"
        );
        let w_line = text.lines().find(|l| l.contains("W ")).expect("W row");
        assert!(
            w_line.contains("A") && w_line.contains("00 04 00 00"),
            "{w_line}"
        );
        assert!(text.contains("Macro slot 0"), "{text}");

        let text = macros(&MacrosView::new(&backup));
        assert!(text.contains("1 of 10 slots used"), "{text}");
        assert!(
            text.contains("Slot 0: macro #0, play once, 2 step(s)"),
            "{text}"
        );
        assert!(text.contains("1. press A, then wait 30 ms"), "{text}");
        assert!(text.contains("2. release A"), "{text}");
    }

    #[test]
    fn all_sections_and_summary() {
        let backup = c06_backup();
        let view = AllView::new(&backup, Some(Path::new("b.owbackup.json")));
        let text = all(&view, backup.model(), Some(20));
        for heading in [
            "Wraith W75 (W75HEARGB)",
            "Main light",
            "Side light",
            "Custom colour tables",
            "Triggers (active profile 1)",
            "Keymap (active profile 1)",
            "Macros (active profile 1)",
            "Advanced keys",
            "SOCD",
            "Switch types",
        ] {
            assert!(text.contains(heading), "missing {heading}");
        }
        assert!(text.contains("backup b.owbackup.json (created 2026-10-24T07:30:00Z)"));
        assert!(text.contains("None of the 40 slots is used."), "{text}");
        assert!(
            text.contains(
                "Power (all profiles): Off; hotkey bytes fa e5, shown by the vendor as Fn + \
                 R-Shift*"
            ),
            "{text}"
        );
        assert!(text.contains("82 keys: HM1 (magnetic, 3.4 mm)"), "{text}");
        // One legend for the whole command, at the end.
        assert_eq!(text.matches(LEGEND).count(), 1, "{text}");
        assert!(text.ends_with(LEGEND), "{text}");

        let summary = backup_summary(&backup, Path::new("b.owbackup.json"));
        assert!(summary.starts_with("Backup written to b.owbackup.json\n"));
        assert!(summary.contains("82 keys, Rapid Trigger on 1"), "{summary}");
        assert!(summary.contains("Custom (per-key), #ff0000"), "{summary}");
        assert!(summary.contains("0 of 10 slots used"), "{summary}");
    }
}
