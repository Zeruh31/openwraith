//! What the commands show, as serialisable views over a [`Backup`] (or over
//! values read directly from the keyboard). Text output (`render`) and JSON
//! output (`--json`) are both built from these views.

use std::path::Path;

use openwraith_hid::{Backup, DeviceSummary, SocdState, UsbInfo};
use openwraith_protocol::{
    ActiveCustomTable, AdvancedKeys, AdvancedSlotData, ColorTable, DeadBand, FnLockedKeys,
    HexBytes, KeyAction, KeyTrigger, KeymapTable, Layer, LightEffect, LightModeList, MacroSlot,
    Model, PollRate, ReadRequest, SocdPairs, SocdSettings, Status, SwitchTypeList, SwitchTypeTable,
    TriggerGlobal, TriggerMode,
};
use serde::Serialize;

/// The `note` of every `--json` output.
pub const JSON_NOTE: &str = "Decoded values follow the confidence labels of docs/PROTOCOL.md: \
some meanings (link type, Win-Lock and rollover bits, light direction / full colour / power / \
background, dead-band order, SOCD hotkey, active custom table, Fn-locked keys, some capability \
flags) come from the vendor configurator's code and are not yet confirmed on the wire. The raw \
bytes are authoritative.";

/// Display name of key `index` (`"#103"` outside the layout or without a model).
pub fn key_name(model: Option<&Model>, index: u8) -> String {
    match model {
        Some(model) => model.key_name(index),
        None => format!("#{index}"),
    }
}

/// A reply that a backup keeps raw because it could not be decoded.
#[derive(Debug, Clone, Serialize)]
pub struct UndecodedNote {
    /// The request.
    pub request: ReadRequest,
    /// The request in words, e.g. `key trigger r0c0 (21/05)`.
    pub description: String,
    /// Why it could not be decoded.
    pub error: String,
}

/// Every reply of `backup` that could not be decoded.
pub fn undecoded_notes(backup: &Backup) -> Vec<UndecodedNote> {
    backup
        .undecoded()
        .map(|entry| UndecodedNote {
            request: entry.request(),
            description: entry.request().to_string(),
            error: entry
                .decode_error
                .clone()
                .unwrap_or_else(|| "unknown error".into()),
        })
        .collect()
}

/// The top level of every `--json` output: the view's fields, a [`JSON_NOTE`]
/// and the replies that could not be decoded.
#[derive(Debug, Serialize)]
pub struct JsonOutput<'a, V: Serialize> {
    /// The command's view.
    #[serde(flatten)]
    pub view: &'a V,
    /// [`JSON_NOTE`].
    pub note: &'static str,
    /// Replies kept raw because they could not be decoded.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub undecoded: Vec<UndecodedNote>,
}

/// Device summary shown by `openwraith info`.
#[derive(Debug, Clone, Serialize)]
pub struct InfoView {
    /// Where the data came from: `"keyboard"` or the backup file path.
    pub source: String,
    /// Backup creation time, when shown from a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Model name.
    pub model: Option<String>,
    /// Product code (`W75HEARGB`).
    pub product: Option<String>,
    /// Firmware version.
    pub firmware: Option<String>,
    /// Firmware build date.
    pub build_date: Option<String>,
    /// The whole device-info string.
    pub info: String,
    /// USB identity.
    pub usb: Option<UsbInfo>,
    /// Active onboard profile.
    pub active_profile: Option<u8>,
    /// Status and capability flags.
    pub status: Option<Status>,
    /// Polling rate.
    pub poll_rate: Option<PollRate>,
    /// Travel range and unit.
    pub trigger_global: Option<TriggerGlobal>,
}

impl InfoView {
    /// Builds the view from a summary read from the keyboard.
    pub fn from_summary(summary: &DeviceSummary) -> Self {
        let device = &summary.device;
        InfoView {
            source: "keyboard".to_string(),
            created_at: None,
            model: device.model.clone(),
            product: device.product.clone(),
            firmware: device.firmware.clone(),
            build_date: device.build_date.clone(),
            info: device.info.clone(),
            usb: device.usb.clone(),
            active_profile: Some(summary.active_profile),
            status: Some(summary.status.clone()),
            poll_rate: summary.poll_rate.clone(),
            trigger_global: Some(summary.trigger_global.clone()),
        }
    }

    /// Builds the view from a backup that was loaded from `file`, or that
    /// was just read from the keyboard (`None`).
    pub fn from_backup(backup: &Backup, file: Option<&Path>) -> Self {
        InfoView {
            source: file.map_or_else(|| "keyboard".to_string(), |f| f.display().to_string()),
            created_at: file.map(|_| backup.created_at.clone()),
            model: backup.device.model.clone(),
            product: backup.device.product.clone(),
            firmware: backup.device.firmware.clone(),
            build_date: backup.device.build_date.clone(),
            info: backup.device.info.clone(),
            usb: backup.device.usb.clone(),
            active_profile: backup.active_profile,
            status: backup.status().cloned(),
            poll_rate: backup.poll_rate().cloned(),
            trigger_global: backup.trigger_global().cloned(),
        }
    }
}

/// One key's trigger settings with distances in millimetres.
///
/// The decoded fields are `None` when the key's reply was kept raw because
/// it could not be decoded (see `raw` and the output's `undecoded` list).
#[derive(Debug, Clone, Serialize)]
pub struct TriggerRow {
    /// Key index (row x 22 + col).
    pub index: u8,
    /// Key name.
    pub name: String,
    /// Matrix row.
    pub row: u8,
    /// Matrix column.
    pub col: u8,
    /// Trigger mode.
    pub mode: Option<TriggerMode>,
    /// Whether Rapid Trigger is on.
    pub rapid_trigger: Option<bool>,
    /// Actuation point.
    pub actuation_mm: Option<f64>,
    /// Second travel value (equal to the actuation point in every capture).
    pub second_travel_mm: Option<f64>,
    /// Rapid Trigger press sensitivity.
    pub rt_press_mm: Option<f64>,
    /// Rapid Trigger release sensitivity.
    pub rt_release_mm: Option<f64>,
    /// Dead band at the top of the travel (order CODE-ONLY).
    pub dead_band_top_mm: Option<f64>,
    /// Dead band at the bottom of the travel (order CODE-ONLY).
    pub dead_band_bottom_mm: Option<f64>,
    /// The `21/05` reply payload (12 bytes when decoded), if the key was read.
    pub raw: Option<HexBytes>,
    /// Source record (for text output).
    #[serde(skip)]
    pub trigger: Option<KeyTrigger>,
    /// Dead band record (for text output).
    #[serde(skip)]
    pub dead_band: Option<DeadBand>,
}

/// `show triggers`.
#[derive(Debug, Clone, Serialize)]
pub struct TriggersView<'a> {
    /// Active onboard profile.
    pub active_profile: Option<u8>,
    /// Travel range and unit.
    pub trigger_global: Option<&'a TriggerGlobal>,
    /// Every key, in key-index order.
    pub keys: Vec<TriggerRow>,
}

impl<'a> TriggersView<'a> {
    /// Builds the view.
    ///
    /// With a known model there is one row per key of the layout, also for
    /// keys whose reply could not be decoded (placeholder rows), so no key
    /// silently disappears. Without a model, the decoded keys are listed.
    pub fn new(backup: &'a Backup) -> Self {
        let model = backup.model();
        let dead_band = backup.dead_band();
        let row = |index: u8, row: u8, col: u8, trigger: Option<&KeyTrigger>| {
            let band = dead_band.and_then(|d| d.get(index));
            let raw = match trigger {
                Some(t) => Some(t.raw.clone()),
                None => openwraith_protocol::KeyPos::new(row, col)
                    .and_then(|pos| backup.response(ReadRequest::KeyTrigger(pos)))
                    .map(|r| HexBytes(r.payload())),
            };
            TriggerRow {
                index,
                name: key_name(model, index),
                row,
                col,
                mode: trigger.map(|t| t.mode),
                rapid_trigger: trigger.map(KeyTrigger::rapid_trigger),
                actuation_mm: trigger.map(|t| t.actuation.to_mm()),
                second_travel_mm: trigger.map(|t| t.second_travel.to_mm()),
                rt_press_mm: trigger.map(|t| t.rt_press.to_mm()),
                rt_release_mm: trigger.map(|t| t.rt_release.to_mm()),
                dead_band_top_mm: band.map(|b| b.top.to_mm()),
                dead_band_bottom_mm: band.map(|b| b.bottom.to_mm()),
                raw,
                trigger: trigger.cloned(),
                dead_band: band,
            }
        };
        let any_read = backup
            .entries
            .iter()
            .any(|e| matches!(e.request(), ReadRequest::KeyTrigger(_)));
        let mut keys: Vec<TriggerRow> = match model {
            // No trigger was read at all: nothing to show, not 82 placeholders.
            _ if !any_read => Vec::new(),
            Some(model) => model
                .keys
                .iter()
                .map(|key| row(key.index, key.row, key.col, backup.key_trigger(key.index)))
                .collect(),
            None => backup
                .key_triggers()
                .into_iter()
                .map(|t| row(t.index(), t.pos.row(), t.pos.col(), Some(t)))
                .collect(),
        };
        keys.sort_by_key(|k| k.index);
        TriggersView {
            active_profile: backup.active_profile,
            trigger_global: backup.trigger_global(),
            keys,
        }
    }
}

/// `show light`.
#[derive(Debug, Clone, Serialize)]
pub struct LightView<'a> {
    /// Main light effect (`07`).
    pub main_light: Option<&'a LightEffect>,
    /// Main-light modes the keyboard lists (`0A`).
    pub main_light_modes: Option<&'a LightModeList>,
    /// Side light effect (`08`).
    pub side_light: Option<&'a LightEffect>,
    /// Side-light modes the keyboard lists (`02`).
    pub side_light_modes: Option<&'a LightModeList>,
    /// Active custom colour table (`09 20`; meaning CODE-ONLY).
    pub active_custom_table: Option<&'a ActiveCustomTable>,
    /// Custom colour tables (`09 80` / `09 81`).
    pub custom_colors: Vec<&'a ColorTable>,
}

impl<'a> LightView<'a> {
    /// Builds the view.
    pub fn new(backup: &'a Backup) -> Self {
        LightView {
            main_light: backup.main_light(),
            main_light_modes: backup.main_light_modes(),
            side_light: backup.side_light(),
            side_light_modes: backup.side_light_modes(),
            active_custom_table: backup.active_custom_table(),
            custom_colors: backup.custom_colors(),
        }
    }
}

/// `show keymap`.
#[derive(Debug, Clone, Serialize)]
pub struct KeymapView<'a> {
    /// Active onboard profile.
    pub active_profile: Option<u8>,
    /// Keys that cannot be remapped on the Fn layer (`0E`; meaning CODE-ONLY).
    pub fn_locked_keys: Option<&'a FnLockedKeys>,
    /// Base and Fn layers (`18 80` / `18 82`), all 132 entries each.
    pub layers: Vec<&'a KeymapTable>,
}

impl<'a> KeymapView<'a> {
    /// Builds the view.
    pub fn new(backup: &'a Backup) -> Self {
        KeymapView {
            active_profile: backup.active_profile,
            fn_locked_keys: backup.fn_locked_keys(),
            layers: backup.keymaps(),
        }
    }
}

/// A key whose keymap entry refers to a macro or an advanced-key slot.
#[derive(Debug, Clone, Serialize)]
pub struct Binding {
    /// The slot.
    pub slot: u8,
    /// The layer of the entry.
    pub layer: Layer,
    /// The key index.
    pub index: u8,
    /// The key's name.
    pub key: String,
}

/// Every entry of both layers whose action `slot_of` maps to a slot.
fn bindings(backup: &Backup, slot_of: impl Fn(&KeyAction) -> Option<u8>) -> Vec<Binding> {
    let model = backup.model();
    backup
        .keymaps()
        .iter()
        .flat_map(|table| {
            table.entries.iter().filter_map(|entry| {
                slot_of(&entry.action).map(|slot| Binding {
                    slot,
                    layer: table.layer,
                    index: entry.index,
                    key: key_name(model, entry.index),
                })
            })
        })
        .collect()
}

/// `show macros`.
#[derive(Debug, Clone, Serialize)]
pub struct MacrosView<'a> {
    /// Active onboard profile.
    pub active_profile: Option<u8>,
    /// The ten macro slots (`19 80`..`19 89`).
    pub slots: Vec<&'a MacroSlot>,
    /// The keys bound to a macro slot (both layers).
    pub bindings: Vec<Binding>,
}

impl<'a> MacrosView<'a> {
    /// Builds the view.
    pub fn new(backup: &'a Backup) -> Self {
        MacrosView {
            active_profile: backup.active_profile,
            slots: backup.macros(),
            bindings: bindings(backup, |action| match action {
                KeyAction::Macro { slot, .. } => Some(*slot),
                _ => None,
            }),
        }
    }
}

/// One used advanced-key slot of `show advanced`.
#[derive(Debug, Clone, Serialize)]
pub struct AdvancedRow {
    /// The slot (0-39).
    pub slot: u8,
    /// What OpenWraith reads in it, when it is a DKS, Mod-Tap or Toggle in
    /// the form OpenWraith writes; `None` otherwise (kept as read; see
    /// `advanced_keys`).
    pub definition: Option<String>,
    /// The keys bound to it.
    pub bound_to: Vec<Binding>,
}

/// `show advanced`.
#[derive(Debug, Clone, Serialize)]
pub struct AdvancedView<'a> {
    /// Active onboard profile.
    pub active_profile: Option<u8>,
    /// The forty slots as decoded (`22 80`).
    pub advanced_keys: Option<&'a AdvancedKeys>,
    /// The used slots.
    pub slots: Vec<AdvancedRow>,
}

impl<'a> AdvancedView<'a> {
    /// Builds the view.
    pub fn new(backup: &'a Backup) -> Self {
        let bound = bindings(backup, |action| match action {
            KeyAction::Advanced { slot } => Some(*slot),
            _ => None,
        });
        let slots = backup
            .advanced_keys()
            .map(|keys| {
                keys.used()
                    .map(|slot| AdvancedRow {
                        slot: slot.slot,
                        definition: AdvancedSlotData::from_read_slot(&slot.raw.0)
                            .ok()
                            .flatten()
                            .and_then(|data| data.def())
                            .map(|def| def.to_string()),
                        bound_to: bound
                            .iter()
                            .filter(|b| b.slot == slot.slot)
                            .cloned()
                            .collect(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        AdvancedView {
            active_profile: backup.active_profile,
            advanced_keys: backup.advanced_keys(),
            slots,
        }
    }
}

/// `show socd`.
#[derive(Debug, Clone, Serialize)]
pub struct SocdView<'a> {
    /// Active onboard profile.
    pub active_profile: Option<u8>,
    /// SOCD power and hotkey (`24 02`; hotkey meaning GUESS).
    pub socd_settings: Option<&'a SocdSettings>,
    /// SOCD pairs (`24 01`).
    pub socd_pairs: Option<&'a SocdPairs>,
    /// The state as OpenWraith edits it (power byte, pairs in the vendor
    /// site's form, or why they are not).
    pub state: Option<SocdState>,
}

impl<'a> SocdView<'a> {
    /// Builds the view.
    pub fn new(backup: &'a Backup) -> Self {
        let state = match (
            backup.response(ReadRequest::SocdSettings),
            backup.response(ReadRequest::SocdPairs),
        ) {
            (Some(settings), Some(pairs)) => SocdState::from_replies(settings, pairs).ok(),
            _ => None,
        };
        SocdView {
            active_profile: backup.active_profile,
            socd_settings: backup.socd_settings(),
            socd_pairs: backup.socd_pairs(),
            state,
        }
    }
}

/// Advanced keys, SOCD and switch types (part of `show all`).
#[derive(Debug, Clone, Serialize)]
pub struct ExtrasView<'a> {
    /// Advanced keys (`22 80`).
    pub advanced_keys: Option<&'a AdvancedKeys>,
    /// SOCD power and hotkey (`24 02`; hotkey meaning GUESS).
    pub socd_settings: Option<&'a SocdSettings>,
    /// SOCD pairs (`24 01`).
    pub socd_pairs: Option<&'a SocdPairs>,
    /// Supported switch types (`25 00`).
    pub switch_type_list: Option<&'a SwitchTypeList>,
    /// Per-key switch types (`25 02`).
    pub switch_types: Option<&'a SwitchTypeTable>,
}

impl<'a> ExtrasView<'a> {
    /// Builds the view.
    pub fn new(backup: &'a Backup) -> Self {
        ExtrasView {
            advanced_keys: backup.advanced_keys(),
            socd_settings: backup.socd_settings(),
            socd_pairs: backup.socd_pairs(),
            switch_type_list: backup.switch_type_list(),
            switch_types: backup.switch_types(),
        }
    }
}

/// `show all`: every section.
#[derive(Debug, Clone, Serialize)]
pub struct AllView<'a> {
    /// Device summary.
    pub info: InfoView,
    /// Lighting.
    pub light: LightView<'a>,
    /// Triggers.
    pub triggers: TriggersView<'a>,
    /// Keymap.
    pub keymap: KeymapView<'a>,
    /// Macros.
    pub macros: MacrosView<'a>,
    /// Advanced keys, SOCD and switch types.
    pub extras: ExtrasView<'a>,
}

impl<'a> AllView<'a> {
    /// Builds the view (`file`: see [`InfoView::from_backup`]).
    pub fn new(backup: &'a Backup, file: Option<&Path>) -> Self {
        AllView {
            info: InfoView::from_backup(backup, file),
            light: LightView::new(backup),
            triggers: TriggersView::new(backup),
            keymap: KeymapView::new(backup),
            macros: MacrosView::new(backup),
            extras: ExtrasView::new(backup),
        }
    }
}
