//! Byte-identical checks of the v0.2 write builders and decoders against the
//! capture session 3 traffic (c13-c17): macro programs, advanced-key slots,
//! keymap assignments on both layers, custom colours, SOCD, the travel test
//! and the keyboard's own pushes. Every expected packet is the one the
//! vendor site sent and the keyboard accepted (cited by capture and entry
//! index).
//!
//! c15-c17 have no OPEN entry (one page session continued from c14); the
//! capture parser does not need one.

mod common;

use common::{exchanges_in, first, load};
use openwraith_protocol::capture::{Capture, CaptureEvent};
use openwraith_protocol::write::{
    AckOffer, AckSpec, AdvancedKeyDef, AdvancedLimits, AdvancedSlotData, AdvancedSlotId,
    BuiltinAck, ColorTableData, Combo, Dks, DksRow, DksStep, EntryKind, HidUsage, KeyAssignment,
    KeymapData, MacroData, MacroPlay, MacroProgram, MacroStep, MediaKey, ModTap, MouseAction,
    MouseButton, SocdMode, SocdPair, SocdPairsData, SocdWrite, Toggle, TravelTestMask,
    TravelTestRequest, WriteRequest, WriteStep, check_placement, is_permitted_packet, is_write_ack,
};
use openwraith_protocol::{
    CustomTable, DeviceEvent, KeyMask, KeyPos, Layer, MacroSlotId, RawResponse, ReadRequest,
    Report, Rgb, Status, TravelReport, TravelVariant, TriggerGlobal, W75,
};

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

/// The `OUT` packet at entry `index`.
fn out(c: &Capture, index: usize) -> [u8; 63] {
    match &c.entries[index].event {
        CaptureEvent::Out(bytes) => *bytes,
        other => panic!("entry {index} is not an OUT packet: {other:?}"),
    }
}

/// The `IN` report at entry `index`.
fn input(c: &Capture, index: usize) -> &Report {
    match &c.entries[index].event {
        CaptureEvent::In(report) => report,
        other => panic!("entry {index} is not an IN report: {other:?}"),
    }
}

/// The last `count` `OUT` packets up to and including entry `last` that
/// satisfy `want`.
fn outs_ending_at(
    c: &Capture,
    last: usize,
    count: usize,
    want: impl Fn(&[u8; 63]) -> bool,
) -> Vec<(usize, [u8; 63])> {
    let mut found: Vec<(usize, [u8; 63])> = c
        .out_packets()
        .filter(|(i, bytes)| *i <= last && want(bytes))
        .map(|(i, bytes)| (i, *bytes))
        .collect();
    let found = found.split_off(found.len().saturating_sub(count));
    assert_eq!(found.len(), count, "OUT packets up to entry {last}");
    assert_eq!(found.last().map(|(i, _)| *i), Some(last));
    found
}

/// Asserts that `steps` send exactly the captured packets.
fn assert_same_steps(what: &str, steps: &[WriteStep], captured: &[(usize, [u8; 63])]) {
    assert_eq!(steps.len(), captured.len(), "{what}: packet count");
    for (k, (step, (index, theirs))) in steps.iter().zip(captured).enumerate() {
        assert_eq!(
            step.packet.as_bytes()[..],
            theirs[..],
            "{what}: packet {k} differs from capture entry #{index}"
        );
        assert!(is_permitted_packet(&step.packet), "{what}: packet {k}");
    }
}

/// Asserts that `request` sends exactly the captured packets.
fn assert_same_packets(request: &WriteRequest, captured: &[(usize, [u8; 63])]) {
    assert_same_steps(&request.to_string(), &request.steps(), captured);
}

/// The complete reply to the read sent at `OUT` entry `out_index`.
fn reply_to(c: &Capture, out_index: usize, request: ReadRequest) -> RawResponse {
    exchanges_in(c, out_index..out_index + 1, |r| *r == request)
        .first()
        .unwrap_or_else(|| panic!("no complete reply to {request} sent at #{out_index}"))
        .response()
        .unwrap()
}

/// The last complete reply to `request` whose `OUT` lies in `range`.
fn reply_in(c: &Capture, range: std::ops::Range<usize>, request: ReadRequest) -> RawResponse {
    exchanges_in(c, range, |r| *r == request)
        .last()
        .unwrap_or_else(|| panic!("no complete reply to {request}"))
        .response()
        .unwrap()
}

/// Feeds the `IN` reports that follow entry `index` (up to the next `OUT`
/// packet) to the step's ack tracker and asserts it completes with an ack.
fn assert_acked(c: &Capture, index: usize, step: &WriteStep) {
    let mut tracker = step.ack.tracker();
    for entry in &c.entries[index + 1..] {
        match &entry.event {
            CaptureEvent::Out(_) => break,
            CaptureEvent::In(report) => {
                tracker.offer(report);
            }
            _ => {}
        }
    }
    assert!(
        tracker.ack_received(),
        "no ack for capture entry #{index} ({:?})",
        step.ack
    );
}

fn usage(u: u8) -> HidUsage {
    HidUsage::new(u).unwrap()
}

fn key(name: &str) -> KeyPos {
    W75.key_by_name(name)
        .and_then(|k| k.pos())
        .unwrap_or_else(|| panic!("no key {name}"))
}

/// The advanced-key limits from the session-3 connect (c14: `21/04` and
/// the status report).
fn c14_limits() -> AdvancedLimits {
    let c14 = load("capture_14_travel_test");
    let global = first::<TriggerGlobal>(&c14, ReadRequest::TriggerGlobal);
    let status = first::<Status>(&c14, ReadRequest::Status);
    let unit = global.unit_micrometres(status.flags_b.high_precision);
    AdvancedLimits::new(&W75, &global, unit).unwrap()
}

// ---------------------------------------------------------------------------
// W6: macro programs (c13).
// ---------------------------------------------------------------------------

/// c13 slot 0: A down 100 ms, A up 100 ms, B down 100 ms, B up, play once.
fn c13_program() -> MacroProgram {
    let (a, b) = (usage(0x04), usage(0x05));
    MacroProgram::new(
        MacroPlay::Once,
        vec![
            MacroStep::press(a, 100),
            MacroStep::release(a, 100),
            MacroStep::press(b, 100),
            MacroStep::release(b, 0),
        ],
    )
    .unwrap()
}

#[test]
fn c13_macro_program_matches_the_site() {
    let c = load("capture_13_macro_ab_on_caps");
    let slot = MacroSlotId::new(0).unwrap();
    let request = WriteRequest::Macro(slot, c13_program().encode(slot));
    // #316 `19 00 00 00 3a 00 00 00 10 00 01 00 00 04 10 00 64 04 00 00 64
    // 05 10 00 64 05`, then the zero chunks #320, #322, #324, #326.
    let captured = outs_ending_at(&c, 326, 5, |b| b[..2] == [0x19, 0x00]);
    assert_eq!(
        captured.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        [316, 320, 322, 324, 326]
    );
    assert_same_packets(&request, &captured);
    // The first chunk's ack is #318 (a heartbeat went out at #317).
    let steps = request.steps();
    assert_eq!(steps[0].ack.tracker().offer(input(&c, 318)), AckOffer::Ack);
    for (k, (index, _)) in captured.iter().enumerate().skip(1) {
        assert_acked(&c, *index, &steps[k]);
    }
    // The site's slot decodes back to the same program.
    let bytes = request.steps()[0].packet.as_bytes()[5..].to_vec();
    let MacroData::Content(content) = c13_program().encode(slot) else {
        panic!("an encoded macro is never empty");
    };
    assert_eq!(content.as_bytes()[..58], bytes[..58]);
    assert_eq!(MacroProgram::decode(&content), Some(c13_program()));

    // The binding: Caps -> macro slot 0 = `10 39 00 00` at entry 66, which
    // is chunk 4, payload 40 of the base keymap write #425.
    let caps = key("Caps Lock");
    let entry = KeyAssignment::Macro(slot).entry(&W75, caps).unwrap();
    assert_eq!(entry, [0x10, 0x39, 0x00, 0x00]);
    assert_eq!(out(&c, 425)[3], 4);
    assert_eq!(out(&c, 425)[5 + 40..5 + 44], entry);
    assert_eq!(
        KeyAssignment::classify(&W75, caps, entry),
        EntryKind::Assignable(KeyAssignment::Macro(slot))
    );
}

// ---------------------------------------------------------------------------
// W10: advanced-key slots (c16, c17).
// ---------------------------------------------------------------------------

fn dks_row(key: u8, steps: [DksStep; 4]) -> Option<DksRow> {
    Some(DksRow {
        key: usage(key),
        steps,
    })
}

/// c16 DKS: E tap at stage 1, R tap at stage 2, first point 1.5 mm.
fn c16_dks() -> AdvancedKeyDef {
    let (n, t) = (DksStep::None, DksStep::Tap);
    AdvancedKeyDef::Dks(Dks {
        first_point: 15,
        rows: [
            dks_row(0x08, [t, n, n, n]),
            dks_row(0x15, [n, t, n, n]),
            None,
            None,
        ],
    })
}

fn advanced(slot: u8, def: &AdvancedKeyDef) -> WriteRequest {
    let data = AdvancedSlotData::from_def(def, &c14_limits()).unwrap();
    WriteRequest::AdvancedKey(AdvancedSlotId::new(slot).unwrap(), data)
}

#[test]
fn c16_advanced_slots_match_the_site() {
    let c = load("capture_16_advanced_keys");
    assert_eq!(c14_limits().dks_bottom(), 30);

    // DKS in slot 0: #2302, byte-identical #3070 (bursts B4-B11).
    let dks = advanced(0, &c16_dks());
    for index in [2302, 3070] {
        assert_same_packets(&dks, &[(index, out(&c, index))]);
    }
    assert_eq!(
        out(&c, 2302)[..24],
        [
            0x22, 0x00, 0x00, 0x00, 0x32, 0x01, 0x0f, 0x1e, 0x1e, 0x0f, 0x00, 0x08, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00, 0x00, 0x15, 0x00, 0x00, 0x00, 0x01
        ]
    );
    // ... and in every c17 burst, e.g. #1901.
    let c17 = load("capture_17_keymap_codes_led_map");
    assert_same_packets(&dks, &[(1901, out(&c17, 1901))]);

    // MT in slot 1: hold L-Ctrl, tap G, 300 ms (#3072, acked #3073).
    let mt = advanced(
        1,
        &AdvancedKeyDef::ModTap(ModTap {
            hold: usage(0xE0),
            tap: usage(0x0A),
            threshold_10ms: 30,
        }),
    );
    assert_same_packets(&mt, &[(3072, out(&c, 3072))]);
    assert_acked(&c, 3072, &mt.steps()[0]);
    assert!(is_write_ack(input(&c, 3073)));

    // TGL H in slot 2 (#3563) and, in the first attempt, slot 0 (#489).
    let tgl = AdvancedKeyDef::Toggle(Toggle { key: usage(0x0B) });
    assert_same_packets(&advanced(2, &tgl), &[(3563, out(&c, 3563))]);
    let tgl0 = advanced(0, &tgl);
    assert_same_packets(&tgl0, &[(489, out(&c, 489))]);

    // The TGL slot read back after the profile round trip (#1320, reply to
    // #1319) re-encodes to the site's write #489 and verifies it.
    let read = reply_to(&c, 1319, ReadRequest::AdvancedKeys);
    let payload = read.reports()[0].payload().to_vec();
    assert_eq!(payload[..3], [0x03, 0x00, 0x0b]);
    let data = AdvancedSlotData::from_read_slot(&payload)
        .unwrap()
        .expect("slot 0 is not empty");
    assert_eq!(data.def(), Some(tgl));
    let restored = WriteRequest::AdvancedKey(AdvancedSlotId::new(0).unwrap(), data);
    assert_same_packets(&restored, &[(489, out(&c, 489))]);
    assert_eq!(tgl0.check(std::slice::from_ref(&read)), Ok(()));
    // The other 39 slots read empty.
    let table = read.table_bytes("advanced keys").unwrap();
    for slot in 1..40 {
        assert_eq!(
            AdvancedSlotData::from_read_slot(&table[slot * 40..slot * 40 + 40]),
            Ok(None)
        );
    }
}

#[test]
fn c16_bindings_match_burst_b10() {
    let c = load("capture_16_advanced_keys");
    // Profile 1's base table read at #1371 (knob defaults at 16-21 and
    // `01 00 00 00` at Fn from the site's earlier Apply).
    let (layer, mut base) =
        KeymapData::from_response(&reply_to(&c, 1371, ReadRequest::Keymap(Layer::Base))).unwrap();
    assert_eq!(layer, Layer::Base);
    let none = KeyMask::default();
    for (name, slot) in [("E", 0), ("G", 1), ("H", 2)] {
        let pos = key(name);
        let target = KeyAssignment::Advanced(AdvancedSlotId::new(slot).unwrap());
        assert_eq!(
            check_placement(&W75, Layer::Base, pos, &target, &none, &[]),
            Ok(None)
        );
        base.set_entry(pos, target.entry(&W75, pos).unwrap());
    }
    // B10: #3993-#4010 (E at chunk 3 payload 20, G and H at chunk 5
    // payload 8 and 12).
    let request = WriteRequest::Keymap(Layer::Base, base);
    assert_same_packets(
        &request,
        &outs_ending_at(&c, 4010, 10, |b| b[..2] == [0x18, 0x00]),
    );
    assert_eq!(out(&c, 3998)[5 + 20..5 + 24], [0x17, 0x00, 0x00, 0x00]);
    assert_eq!(out(&c, 4002)[5 + 8..5 + 16], [0x17, 1, 1, 0, 0x17, 2, 2, 0]);
}

// ---------------------------------------------------------------------------
// W5: Fn-layer codes (c17).
// ---------------------------------------------------------------------------

#[test]
fn c17_fn_layer_codes_match_the_site() {
    let c16 = load("capture_16_advanced_keys");
    let c = load("capture_17_keymap_codes_led_map");
    // Profile 1's Fn table read at c16 #1382 (all zero).
    let (layer, mut fn_layer) =
        KeymapData::from_response(&reply_to(&c16, 1382, ReadRequest::Keymap(Layer::Fn))).unwrap();
    assert_eq!(layer, Layer::Fn);
    let ctrl_shift_a = Combo::new(&[usage(0xE0), usage(0xE1)], usage(0x04)).unwrap();
    let assignments = [
        ("F1", KeyAssignment::Media(MediaKey::new(0x12).unwrap())),
        ("F2", KeyAssignment::Mouse(MouseAction::WheelDown)),
        (
            "F3",
            KeyAssignment::Mouse(MouseAction::Button(MouseButton::Forward)),
        ),
        ("F4", KeyAssignment::Media(MediaKey::new(0x25).unwrap())),
        ("W", KeyAssignment::Media(MediaKey::new(0x32).unwrap())),
        ("Caps Lock", KeyAssignment::Combo(ctrl_shift_a)),
    ];
    // The W75's `0E` reply locks only Fn (121).
    let fn_locked = KeyMask::from_positions([KeyPos::from_index(121).unwrap()]);
    let acks: Vec<BuiltinAck> = ["F1", "F2", "F3", "F4"]
        .map(|name| BuiltinAck {
            pos: key(name),
            critical: false,
        })
        .to_vec();
    for (name, target) in assignments {
        let pos = key(name);
        // F1-F4 hide a built-in function and need the acknowledgement.
        let hidden = check_placement(&W75, Layer::Fn, pos, &target, &fn_locked, &acks).unwrap();
        assert_eq!(hidden.is_some(), name.starts_with('F'), "{name}");
        if name.starts_with('F') {
            assert!(check_placement(&W75, Layer::Fn, pos, &target, &fn_locked, &[]).is_err());
        }
        let entry = target.entry(&W75, pos).unwrap();
        assert_eq!(
            KeyAssignment::classify(&W75, pos, entry),
            EntryKind::Assignable(target),
            "{name}"
        );
        fn_layer.set_entry(pos, entry);
    }
    // c17 #1890-#1900: F1 `07 12`, F2 `12 01 ff 00`, F3 `12 00 10 00`,
    // F4 `07 25`, W `07 32`, Caps `13 e0 e1 04`.
    let request = WriteRequest::Keymap(Layer::Fn, fn_layer);
    assert_same_packets(
        &request,
        &outs_ending_at(&c, 1900, 10, |b| b[..2] == [0x18, 0x02]),
    );
    assert_eq!(
        out(&c, 1890)[5 + 8..5 + 24],
        [
            0x07, 0x12, 0, 0, 0x12, 0x01, 0xff, 0, 0x12, 0x00, 0x10, 0, 0x07, 0x25, 0, 0
        ]
    );
    // The knob and Fn are refused on both layers.
    for layer in Layer::ALL {
        for index in [15u8, 121] {
            let pos = KeyPos::from_index(index).unwrap();
            assert!(
                check_placement(&W75, layer, pos, &KeyAssignment::Default, &fn_locked, &[])
                    .is_err()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// W4: custom colours 1 (c14 read, c17 writes).
// ---------------------------------------------------------------------------

#[test]
fn c17_custom_colours_match_the_site() {
    let c14 = load("capture_14_travel_test");
    let c = load("capture_17_keymap_codes_led_map");
    // Custom 1 read at c14 #145 (#147-#154): 81 x ff0000, 51 x 000000.
    let read = reply_to(&c14, 145, ReadRequest::CustomColors(CustomTable::Custom1));
    let mut data = ColorTableData::from_response(&read).unwrap();
    let before = data.clone();
    let blue = Rgb::new(0x00, 0x00, 0xff);
    let up = key("Up");
    let right = key("Right");
    assert_eq!((up.index(), right.index()), (102, 125));

    // Up blue: #2223-#2237.
    data.set_key_colors(&W75, &[(up, blue)]).unwrap();
    let request = WriteRequest::CustomColors1(data.clone());
    let burst = outs_ending_at(&c, 2237, 8, |b| b[..2] == [0x09, 0x00]);
    assert_eq!(burst[0].0, 2223);
    assert_same_packets(&request, &burst);
    for ((index, _), step) in burst.iter().zip(request.steps()) {
        assert_acked(&c, *index, &step);
    }
    // No `07 00` is part of it (the site sent one at #2239).
    assert!(request.steps().iter().all(|s| s.packet.cmd() == 0x09));

    // Then Right blue as well: #2264-#2278.
    data.set_key_colors(&W75, &[(right, blue)]).unwrap();
    let request = WriteRequest::CustomColors1(data.clone());
    let burst = outs_ending_at(&c, 2278, 8, |b| b[..2] == [0x09, 0x00]);
    assert_eq!(burst[0].0, 2264);
    assert_same_packets(&request, &burst);

    // Only the two painted slots changed; the knob (15) and the non-layout
    // slots 103 and 126 keep what the keyboard holds.
    let changed: Vec<usize> = (0..396)
        .filter(|i| data.as_bytes()[*i] != before.as_bytes()[*i])
        .map(|i| i / 3)
        .collect();
    assert_eq!(changed, vec![102, 102, 125, 125]);
    for slot in [15, 103, 126] {
        assert_eq!(
            data.as_bytes()[slot * 3..slot * 3 + 3],
            before.as_bytes()[slot * 3..slot * 3 + 3]
        );
    }
}

// ---------------------------------------------------------------------------
// W11: SOCD (c15) and the keyboard's push.
// ---------------------------------------------------------------------------

fn socd(power: bool, pairs: &[SocdPair]) -> WriteRequest {
    WriteRequest::Socd(SocdWrite::new(
        power,
        SocdPairsData::from_pairs(&W75, pairs).unwrap(),
    ))
}

#[test]
fn c15_socd_bursts_match_the_site() {
    let c = load("capture_15_socd");
    let ad = |mode| SocdPair {
        mode,
        key1: usage(0x04),
        key2: usage(0x07),
    };
    let cases = [
        (103, socd(false, &[ad(SocdMode::M1)])), // B01: A/D Model1, off
        (145, socd(true, &[ad(SocdMode::M1)])),  // B07: switch on
        (246, socd(true, &[ad(SocdMode::M2)])),  // B10: Model2 (wire 1)
        (265, socd(true, &[])),                  // B11: pair deleted
        (272, socd(false, &[])),                 // B12: switch off
    ];
    for (start, request) in &cases {
        let captured: Vec<(usize, [u8; 63])> =
            (*start..*start + 3).map(|i| (i, out(&c, i))).collect();
        assert_same_packets(request, &captured);
        let steps = request.steps();
        assert_eq!(
            steps[0].ack,
            AckSpec::Possible {
                header: [0x24, 0, 0, 0, 0]
            }
        );
        assert!(!steps[0].ack.expects_reply());
        // Both chunk acks follow the burst (#106, #107 for B01).
        for (k, step) in steps[1..].iter().enumerate() {
            let ack = input(&c, start + 3 + k);
            assert_eq!(step.ack.tracker().offer(ack), AckOffer::Ack, "#{start}");
            assert!(is_write_ack(ack));
        }
    }
    // 12 bursts: 24 chunks, 24 acks, nothing for the 12 power writes.
    let chunk_ack = cases[0].1.steps()[1].ack;
    let acks = c
        .in_reports()
        .filter(|(_, r)| chunk_ack.tracker().offer(r) == AckOffer::Ack)
        .count();
    let chunks = c
        .out_packets()
        .filter(|(_, b)| b[..2] == [0x24, 0x00])
        .count();
    let powers = c
        .out_packets()
        .filter(|(_, b)| b[..2] == [0x24, 0x03])
        .count();
    assert_eq!((chunks, acks, powers), (24, 24, 12));

    // Profile 1 read back power off and no pairs at c16 #1360-#1364: the
    // last burst (B12) verifies against it, and the pair table is in the
    // site's form.
    let c16 = load("capture_16_advanced_keys");
    let settings = reply_in(&c16, 1300..1400, ReadRequest::SocdSettings);
    let pairs = reply_in(&c16, 1300..1400, ReadRequest::SocdPairs);
    assert_eq!(cases[4].1.check(&[settings.clone(), pairs.clone()]), Ok(()));
    assert!(cases[3].1.check(&[settings, pairs.clone()]).is_err());
    assert_eq!(
        SocdPairsData::from_response(&pairs).map(|p| p.pairs()),
        Ok(vec![])
    );
}

#[test]
fn c15_socd_pushes_are_keyboard_events() {
    let c = load("capture_15_socd");
    // Fn + R-Shift twice: #218 turned SOCD off, #231 on again.
    for (index, power) in [(218, 0), (231, 1)] {
        let report = input(&c, index);
        let event = DeviceEvent::classify(report);
        assert_eq!(
            event,
            Some(DeviceEvent::SocdPower {
                power,
                rest: [0x00, 0x00, 0x00, 0xfa, 0xe5]
            }),
            "#{index}"
        );
        assert!(event.is_some_and(|e| e.is_keyboard_initiated()));
        assert!(!is_write_ack(report));
    }
    // They are the only `24 02` reports of c15 (no `24 02` was read).
    let pushes: Vec<usize> = c
        .in_reports()
        .filter(|(_, r)| {
            matches!(
                DeviceEvent::classify(r),
                Some(DeviceEvent::SocdPower { .. })
            )
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(pushes, vec![218, 231]);
}

// ---------------------------------------------------------------------------
// T1, T2 and the travel stream (c14).
// ---------------------------------------------------------------------------

#[test]
fn c14_travel_test_open_and_close() {
    let c = load("capture_14_travel_test");
    let open = TravelTestRequest::Open(TravelTestMask::full_layout(&W75).unwrap()).step();
    let close = TravelTestRequest::Close.step();
    assert_same_steps(
        "travel open",
        std::slice::from_ref(&open),
        &[(981, out(&c, 981))],
    );
    assert_same_steps(
        "travel close",
        std::slice::from_ref(&close),
        &[(1312, out(&c, 1312))],
    );
    // Acked by #982 and #1313 (`21 00 00 00 01 00`).
    assert_acked(&c, 981, &open);
    assert_acked(&c, 1312, &close);
    assert_eq!(input(&c, 982).summary(), "21 00 00 00 01 00");
}

#[test]
fn c14_travel_stream_decodes() {
    let c = load("capture_14_travel_test");
    // #999: W, long, depth 17, motion 01.
    let w = TravelReport::parse(input(&c, 999)).unwrap();
    assert_eq!(w.pos, key("W"));
    assert_eq!(w.depth, 17);
    let TravelVariant::Long(fields) = w.variant else {
        panic!("#999 is a long report");
    };
    assert_eq!(fields.motion, 0x01);
    assert_eq!(fields.sensors, [2434, 2434]);
    assert_eq!(fields.constants, [2459, 2477, 1671]);
    assert_eq!(fields.tail, 0);
    // #1135: W back at 0, long, the two readings differ, tail 0x0a97.
    let rest = TravelReport::parse(input(&c, 1135)).unwrap();
    assert_eq!((rest.pos, rest.depth), (key("W"), 0));
    let TravelVariant::Long(fields) = rest.variant else {
        panic!("#1135 is a long report");
    };
    assert_eq!(fields.motion, 0x00);
    assert_eq!(fields.sensors, [2456, 2454]);
    assert_eq!(fields.tail, 0x0a97);
    // #1157: Esc, short, 169.
    let esc = TravelReport::parse(input(&c, 1157)).unwrap();
    assert_eq!((esc.pos, esc.depth), (key("Esc"), 169));
    assert_eq!(esc.variant, TravelVariant::Short);

    // Every stream report of c14: 253, all between the open (#981) and the
    // close ack (#1313), keys W, Esc, Space, Right, A, depth at most 170.
    let allowed = ["W", "Esc", "Space", "Right", "A"].map(key);
    let mut count = 0;
    let (mut short, mut long) = (0, 0);
    for (index, report) in c.in_reports() {
        let shape = report.cmd() == 0x21 && report.byte(5) == 0x01 && report.byte(4) != 0x01;
        let parsed = TravelReport::parse(report);
        assert_eq!(parsed.is_some(), shape, "#{index}");
        let Some(r) = parsed else {
            continue;
        };
        count += 1;
        assert!((982..1313).contains(&index), "#{index} outside the test");
        assert!(allowed.contains(&r.pos), "#{index}: {}", r.pos);
        assert!(r.depth <= 170, "#{index}");
        match r.variant {
            TravelVariant::Short => short += 1,
            TravelVariant::Long(_) => long += 1,
        }
        // A stream report is a keyboard event, never a write ack or a reply.
        assert_eq!(
            DeviceEvent::classify(report),
            Some(DeviceEvent::TravelStream(r))
        );
        assert!(!is_write_ack(report));
        assert!(
            !ReadRequest::all().iter().any(|q| q.matches(report)),
            "#{index}"
        );
    }
    assert_eq!((count, short, long), (253, 147, 106));
}
