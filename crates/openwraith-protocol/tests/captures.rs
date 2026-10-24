//! Checks the protocol crate against the real USB traffic in `captures/`
//! (Wraith W75, PID 0xC368, firmware V3.22.07).

mod common;

use std::collections::BTreeSet;

use common::{CAPTURES, exchanges_in, first, load, triggers_in};
use openwraith_protocol::capture::CaptureEvent;
use openwraith_protocol::{
    ActiveCustomTable, ActiveProfile, AdvancedKeys, ColorTable, CustomTable, DeadBandTable,
    DeviceEvent, DeviceInfo, Distance, FnLockedKeys, FromResponse, KeyAction, KeyPos, KeymapTable,
    Layer, LightEffect, LightMode, LightModeList, MacroSlot, MacroSlotId, PollRate, RawResponse,
    ReadRequest, Rgb, SocdPairs, SocdSettings, Status, SwitchTypeList, SwitchTypeTable,
    TriggerGlobal, TriggerMode, W75, decode, is_keyboard_reset,
};

const W: u8 = 46;

fn w_pos() -> KeyPos {
    KeyPos::new(2, 2).unwrap()
}

#[test]
fn every_capture_parses_and_opens_the_w75() {
    for name in CAPTURES {
        let capture = load(name);
        assert_eq!(capture.open_pid(), Some(0xC368), "{name}");
        assert!(capture.entries.len() > 800, "{name}");
    }
}

#[test]
fn every_captured_read_is_recognised_and_every_other_packet_is_a_write() {
    for name in CAPTURES {
        let capture = load(name);
        for (index, bytes) in capture.out_packets() {
            if ReadRequest::from_packet(bytes).is_some() {
                continue;
            }
            // Everything the site sent that is not one of our reads must be a
            // documented write (spec section 3), never an unknown read form.
            let write = match (bytes[0], bytes[1]) {
                (0x07, 0x00) | (0x09, 0x00 | 0x01) | (0x14, _) | (0x17, _) | (0x24, 0x03) => true,
                (0x21, 0x00) => matches!(bytes[5], 0x00 | 0x0C | 0x0D | 0x12),
                _ => false,
            };
            assert!(
                write,
                "{name}#{index}: unrecognised packet {:02x?}",
                &bytes[..8]
            );
        }
    }
}

#[test]
fn every_complete_reply_decodes() {
    for name in CAPTURES {
        let capture = load(name);
        let pairing = capture.pair_reads();
        let mut complete = 0;
        for ex in &pairing.exchanges {
            if !ex.complete {
                // Only heartbeats were ever left unanswered (c02 +165764), or
                // requests at the very end of a capture.
                assert_eq!(ex.request, ReadRequest::Status, "{name}#{}", ex.out_index);
                continue;
            }
            complete += 1;
            let response = ex.response().expect("complete exchange");
            decode(&response).unwrap_or_else(|e| panic!("{name}#{}: {e}", ex.out_index));
        }
        assert!(complete > 150, "{name}: only {complete} complete replies");
    }
}

#[test]
fn device_info_in_every_capture() {
    for name in CAPTURES {
        let info: DeviceInfo = first(&load(name), ReadRequest::DeviceInfo);
        assert_eq!(info.product.as_deref(), Some("W75HEARGB"), "{name}");
        assert_eq!(info.firmware.as_deref(), Some("3.22.07"), "{name}");
        assert_eq!(info.firmware_raw.as_deref(), Some("V3.22.07"), "{name}");
        assert_eq!(
            info.build_date.as_deref(),
            Some("Aug  6 2026,13:46:54"),
            "{name}"
        );
        assert_eq!(
            info.fields,
            ["W669", "34", "KB", "ML", "W75HEARGB", "V3.22.07"]
        );
        assert_eq!(
            openwraith_protocol::model_for_product("W75HEARGB").map(|m| m.name),
            Some("Wraith W75")
        );
    }
}

#[test]
fn c01_factory_state() {
    let c = load("capture_01_connect");

    let status: Status = first(&c, ReadRequest::Status);
    assert_eq!(status.flags_a.raw, 0xDF);
    assert_eq!(status.flags_b.raw, 0x04);
    assert!(status.socd_supported);
    assert!(!status.flags_b.high_precision);

    let locked: FnLockedKeys = first(&c, ReadRequest::FnLockedKeys);
    assert_eq!(locked.indices(), vec![121]);
    assert_eq!(W75.key(121).map(|k| k.name), Some("Fn"));

    let modes: LightModeList = first(&c, ReadRequest::MainLightModes);
    assert_eq!(modes.modes.len(), 14);
    assert_eq!((modes.max_speed, modes.max_brightness), (4, 4));
    let side_modes: LightModeList = first(&c, ReadRequest::SideLightModes);
    assert!(side_modes.modes.is_empty());

    let main: LightEffect = first(&c, ReadRequest::MainLight);
    assert_eq!(main.mode, LightMode::Wave);
    assert_eq!(main.foreground, Rgb::new(0xff, 0xff, 0xff));
    assert_eq!((main.brightness, main.speed), (4, 3));
    assert!(main.full_color && main.power_on);
    let side: LightEffect = first(&c, ReadRequest::SideLight);
    assert_eq!(side.foreground, Rgb::new(0xff, 0, 0));

    let slot: ActiveCustomTable = first(&c, ReadRequest::ActiveCustomTable);
    assert_eq!(slot.table, Some(CustomTable::Custom1));

    // Factory colour tables (spec 4.7.6).
    let c1: ColorTable = first(&c, ReadRequest::CustomColors(CustomTable::Custom1));
    assert_eq!(c1.lit_indices(), vec![46, 68, 69, 70, 103, 124, 125, 126]);
    assert!(
        c1.lit_indices()
            .iter()
            .all(|i| c1.color(*i) == Some(Rgb::new(0xff, 0, 0)))
    );
    let c2: ColorTable = first(&c, ReadRequest::CustomColors(CustomTable::Custom2));
    let expected: Vec<u8> = (45..=56)
        .chain([58])
        .chain(68..=78)
        .chain(90..=99)
        .collect();
    assert_eq!(c2.lit_indices(), expected);
    assert_eq!(c2.color(45), Some(Rgb::new(0, 0, 0xff)));

    for layer in Layer::ALL {
        let keymap: KeymapTable = first(&c, ReadRequest::Keymap(layer));
        assert_eq!(keymap.entries.len(), 132);
        assert_eq!(keymap.overrides().count(), 0, "{layer:?}");
        assert!(
            keymap
                .entries
                .iter()
                .all(|e| e.action == KeyAction::Default)
        );
    }
    for slot in MacroSlotId::all() {
        let m: MacroSlot = first(&c, ReadRequest::Macro(slot));
        assert!(m.is_empty(), "macro slot {}", slot.get());
        assert!(m.raw.iter().all(|b| *b == 0xFF));
    }
    let adv: AdvancedKeys = first(&c, ReadRequest::AdvancedKeys);
    assert_eq!(adv.slots.len(), 40);
    assert_eq!(adv.used().count(), 0);

    let socd: SocdSettings = first(&c, ReadRequest::SocdSettings);
    assert!(!socd.enabled);
    assert_eq!(socd.hotkey, [0xfa, 0xe5]);
    let pairs: SocdPairs = first(&c, ReadRequest::SocdPairs);
    assert_eq!(pairs.used().count(), 0);

    let global: TriggerGlobal = first(&c, ReadRequest::TriggerGlobal);
    assert_eq!(global.max_travel.to_string(), "3.40 mm");
    assert_eq!(global.min_travel.to_string(), "0.10 mm");
    assert_eq!(
        global.unit_micrometres(status.flags_b.high_precision),
        Distance::MICROMETRES_PER_UNIT
    );

    let rate: PollRate = first(&c, ReadRequest::PollRate);
    assert_eq!(rate.hz, Some(8000));
    let list: SwitchTypeList = first(&c, ReadRequest::SwitchTypeList);
    assert_eq!(list.ids, vec![1]);
    let switches: SwitchTypeTable = first(&c, ReadRequest::SwitchTypes);
    assert!(switches.per_key.iter().all(|id| *id == 1));
    let dead: DeadBandTable = first(&c, ReadRequest::DeadBand);
    assert!(
        dead.entries
            .iter()
            .all(|d| d.top.units() == 0 && d.bottom.units() == 0)
    );

    let profile: ActiveProfile = first(&c, ReadRequest::ActiveProfile);
    assert_eq!((profile.profile, profile.extra), (1, 1));
}

#[test]
fn c01_factory_triggers_on_all_82_keys() {
    let c = load("capture_01_connect");
    let triggers = triggers_in(&c, 0..usize::MAX);
    let read: BTreeSet<u8> = triggers.keys().copied().collect();
    let layout: BTreeSet<u8> = W75.keys.iter().map(|k| k.index).collect();
    assert_eq!(
        read, layout,
        "the site reads exactly the 82 layout positions"
    );
    for t in triggers.values() {
        assert_eq!(
            t.raw.0[1..6],
            [0x00, 0x55, 0x55, 0x01, 0x01],
            "key {}",
            t.index()
        );
        assert_eq!(t.mode, TriggerMode::Off);
        assert_eq!(t.actuation.to_string(), "1.70 mm");
        assert_eq!(t.rt_press.to_string(), "0.02 mm");
    }
}

#[test]
fn every_full_connect_reads_all_82_keys() {
    for name in [
        "capture_01_connect",
        "capture_03_actuation_W",
        "capture_05_reconnect_W_persisted",
        "capture_06_powercycle_W_persisted",
    ] {
        let triggers = triggers_in(&load(name), 0..usize::MAX);
        assert_eq!(triggers.len(), 82, "{name}");
        assert!(triggers.keys().all(|i| W75.key(*i).is_some()), "{name}");
    }
}

#[test]
fn c05_and_c06_w_key_persisted() {
    for name in [
        "capture_05_reconnect_W_persisted",
        "capture_06_powercycle_W_persisted",
    ] {
        let c = load(name);
        let triggers = triggers_in(&c, 0..usize::MAX);
        let w = &triggers[&W];
        assert_eq!(w.pos, w_pos());
        assert_eq!(w.mode, TriggerMode::RapidTrigger, "{name}");
        assert_eq!(w.mode.byte(), 0x0c);
        assert_eq!(w.actuation.to_string(), "1.00 mm", "{name}");
        assert_eq!(w.second_travel.to_string(), "1.00 mm", "{name}");
        assert_eq!(w.rt_press.to_string(), "0.30 mm", "{name}");
        assert_eq!(w.rt_release.to_string(), "0.30 mm", "{name}");
        assert_eq!(
            w.raw.0,
            [0x05, 0x0c, 0x32, 0x32, 0x0f, 0x0f, 0, 0, 0, 0, 0x02, 0x02]
        );
        // Every other key is untouched.
        assert!(
            triggers
                .values()
                .filter(|t| t.index() != W)
                .all(|t| t.actuation.units() == 0x55 && t.mode == TriggerMode::Off)
        );

        let main: LightEffect = first(&c, ReadRequest::MainLight);
        assert_eq!(main.mode, LightMode::Custom, "{name}");
        assert_eq!(main.mode.id(), 0x0a);
        assert_eq!(main.foreground, Rgb::new(0xff, 0, 0), "{name}");

        let profile: ActiveProfile = first(&c, ReadRequest::ActiveProfile);
        assert_eq!(profile.profile, 1, "{name}");

        // Custom 1 = the 81 selectable keys painted red in c02 (knob excluded).
        let c1: ColorTable = first(&c, ReadRequest::CustomColors(CustomTable::Custom1));
        let selectable: Vec<u8> = W75
            .selectable_mask()
            .positions()
            .iter()
            .map(|p| p.index())
            .collect();
        assert_eq!(c1.lit_indices(), selectable, "{name}");
    }
}

#[test]
fn c03_light_change_persisted_from_c02() {
    let c = load("capture_03_actuation_W");
    let main: LightEffect = first(&c, ReadRequest::MainLight);
    assert_eq!(main.mode, LightMode::Custom);
    assert_eq!(
        main.raw.0,
        [0x0a, 0x04, 0x03, 0xff, 0x00, 0x00, 0, 0, 0, 0, 0x01, 0x00]
    );
}

#[test]
fn c07_keyboard_reset_is_detected_and_resets_triggers() {
    let c = load("capture_07_fn_space_reset");

    // Exactly one unsolicited reset notification from the keyboard.
    let resets: Vec<usize> = c
        .in_reports()
        .filter(|(_, r)| DeviceEvent::classify(r) == Some(DeviceEvent::KeyboardReset))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(resets.len(), 1);
    let reset_at = resets[0];
    let CaptureEvent::In(report) = &c.entries[reset_at].event else {
        panic!("not an input report");
    };
    assert!(is_keyboard_reset(report));
    assert_eq!(
        &report.as_bytes()[..6],
        &[0x14, 0x00, 0x00, 0x00, 0x01, 0x01]
    );
    // It is not an answer to anything we could have asked.
    assert!(c.pair_reads().unmatched.contains(&reset_at));
    // The site's answering reset is a write, never one of our requests.
    let site_reset = c
        .out_packets()
        .find(|(i, b)| *i > reset_at && b[0] == 0x14)
        .expect("the site answered with a reset");
    assert_eq!(ReadRequest::from_packet(site_reset.1), None);

    // Before the reset W still had Rapid Trigger (this capture starts with c06).
    let before = triggers_in(&c, 0..reset_at);
    assert_eq!(before.len(), 82);
    assert_eq!(before[&W].mode, TriggerMode::RapidTrigger);

    // After the reset every key reads `00 55 55 01 01` = 1.70 mm, RT off.
    let after = triggers_in(&c, reset_at..usize::MAX);
    assert_eq!(after.len(), 82);
    for t in after.values() {
        assert_eq!(
            t.raw.0[1..6],
            [0x00, 0x55, 0x55, 0x01, 0x01],
            "key {}",
            t.index()
        );
        assert_eq!(t.actuation, Distance::from(0x55));
        assert!(!t.rapid_trigger());
    }

    // Main light after the reset: Wave, colour kept.
    let lights = exchanges_in(&c, reset_at..usize::MAX, |r| *r == ReadRequest::MainLight);
    let light = LightEffect::from_response(&lights[0].response().unwrap()).unwrap();
    assert_eq!(light.mode, LightMode::Wave);
    assert_eq!(light.foreground, Rgb::new(0xff, 0, 0));
}

#[test]
fn c04_profile_reports_are_classified() {
    let c = load("capture_04_session_after_W");
    let profiles: BTreeSet<u8> = c
        .in_reports()
        .filter_map(|(_, r)| match DeviceEvent::classify(r) {
            Some(DeviceEvent::ProfileReport { profile, .. }) => Some(profile),
            _ => None,
        })
        .collect();
    assert_eq!(profiles, BTreeSet::from([1, 2, 3, 4]));
    assert!(
        c.in_reports()
            .all(|(_, r)| DeviceEvent::classify(r) != Some(DeviceEvent::KeyboardReset))
    );
}

#[test]
fn full_read_plan_is_served_by_c06() {
    // Every request of a backup appears, with a complete reply, in c06. This
    // is what lets the mock transport replay a full read.
    let c = load("capture_06_powercycle_W_persisted");
    let status: Status = first(&c, ReadRequest::Status);
    let plan = ReadRequest::full_read_plan(&W75, Some(&status));
    assert_eq!(plan.len(), ReadRequest::full_read_plan(&W75, None).len());
    let pairing = c.pair_reads();
    for request in plan {
        let ex = pairing
            .exchanges
            .iter()
            .find(|ex| ex.request == request && ex.complete)
            .unwrap_or_else(|| panic!("{request} has no complete reply in c06"));
        // The captured packet is byte-identical to ours.
        let CaptureEvent::Out(bytes) = &c.entries[ex.out_index].event else {
            panic!("not an OUT entry");
        };
        assert_eq!(bytes, request.packet().as_bytes());
    }
}

#[test]
fn raw_responses_survive_a_json_round_trip() {
    let c = load("capture_06_powercycle_W_persisted");
    let responses: Vec<RawResponse> = c
        .pair_reads()
        .exchanges
        .iter()
        .filter(|ex| ex.complete)
        .take(200)
        .map(|ex| ex.response().unwrap())
        .collect();
    let json = serde_json::to_string(&responses).unwrap();
    let back: Vec<RawResponse> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, responses);
    for response in &back {
        let decoded = decode(response).unwrap();
        let json = serde_json::to_string(&decoded).unwrap();
        let again: openwraith_protocol::Decoded = serde_json::from_str(&json).unwrap();
        assert_eq!(again, decoded);
    }
}
