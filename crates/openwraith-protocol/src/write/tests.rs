//! Unit tests of the write module: every constructible request (a bounded
//! sample) is checked against an independent description of the nine write
//! forms, plus step, ack and read-back details.

use super::*;
use crate::decode::test_util::{report, single, table};
use crate::decode::{FromResponse, LightMode, LightModeList, Rgb, TriggerGlobal};
use crate::layout::{KeyMask, KeyPos};
use crate::model::W75;
use crate::units::Distance;

// ---------------------------------------------------------------------------
// Independent description of the allowed write forms (design section 1).
// Written from the table, not from the builders.
// ---------------------------------------------------------------------------

/// W75 Select-All mask (spec section 2.9), spelled out.
const SELECTABLE: [u8; 22] = [
    0x3f, 0x26, 0x3f, 0x1f, 0x1f, 0x1f, 0x3f, 0x1f, 0x1f, 0x1f, 0x3f, 0x3f, 0x2f, 0x31, 0x3f, 0x2e,
    0, 0, 0, 0, 0, 0,
];

/// The W75 `0x0A` mode list (c01).
const W75_MODES: [u8; 14] = [
    0x02, 0x01, 0x03, 0x09, 0x04, 0x00, 0x0e, 0x0f, 0x06, 0x07, 0x0b, 0x0c, 0x10, 0x08,
];

const LEAVE: [u8; 7] = [0x17, 0, 0, 0, 0x02, 0, 0];

/// Whether `p` is one of the allowed write forms. `previous` is the packet
/// sent just before it in the same request (`None` for the first one).
fn is_allowed_write(p: &[u8; 63], previous: Option<&[u8; 63]>) -> bool {
    let zero_from = |i: usize| p.get(i..).is_some_and(|rest| rest.iter().all(|b| *b == 0));
    // Chunk `p[3]` of `count`, every chunk `full` bytes long except the last.
    let chunk = |count: u8, full: u8, last: u8| {
        let expected = if p[3].checked_add(1) == Some(count) {
            last
        } else {
            full
        };
        p[2] == 0 && p[3] < count && p[4] == expected && zero_from(5 + usize::from(p[4]))
    };
    let same_table = || previous.is_none_or(|q| q[..2] == p[..2]);
    match p[0] {
        // W1: select profile.
        0x21 if p[4] == 0x00 => {
            p[..6] == [0x21, 0, 0, 0, 0x00, 0x12]
                && (1..=4).contains(&p[6])
                && zero_from(7)
                && previous.is_none()
        }
        // W8: trigger.
        0x21 => {
            let mask = &p[6..28];
            let bits: u32 = mask.iter().map(|b| b.count_ones()).sum();
            let subset = mask.iter().zip(SELECTABLE).all(|(m, s)| m & !s == 0);
            let value = |lo: usize, hi: usize| u16::from(p[lo]) | (u16::from(p[hi]) << 8);
            let (a, press, release) = (value(28, 32), value(30, 34), value(31, 35));
            let rt_ok = match p[5] {
                0x00 => press <= 85 && release <= 85,
                0x0C => press == release && (1..=85).contains(&press),
                0x0D => (1..=85).contains(&press) && (1..=85).contains(&release),
                _ => false,
            };
            p[..5] == [0x21, 0, 0, 0, 0x18]
                && bits == 1
                && subset
                && p[29] == p[28]
                && p[33] == p[32]
                && (5..=170).contains(&a)
                && rt_ok
                && zero_from(36)
                && previous.is_none_or(|q| q[..5] == p[..5])
        }
        // W2: music-rhythm leave, only as the first packet.
        0x17 => p[..7] == LEAVE && zero_from(7) && previous.is_none(),
        // W3: main light, alone or right after W2.
        0x07 => {
            p[..5] == [0x07, 0, 0, 0, 0x0E]
                && p[5] != 0x64
                && p[14] < 6
                && p[15] <= 1
                && p[16] <= 1
                && zero_from(17)
                && previous.is_none_or(|q| q[..7] == LEAVE)
        }
        // W4: custom colours 1.
        0x09 => p[1] == 0x00 && chunk(8, 0x36, 0x12) && same_table(),
        // W5: keymap base / Fn.
        0x18 => matches!(p[1], 0x00 | 0x02) && chunk(10, 0x38, 0x18) && same_table(),
        // W6: macro slot.
        0x19 => p[1] <= 9 && chunk(5, 0x3A, 0x18) && same_table(),
        // W7: advanced-key clear.
        0x22 => p[1] <= 0x27 && p[2..5] == [0, 0, 0x32] && zero_from(5) && previous.is_none(),
        // W9: dead band, values within 0..=50.
        0x26 => {
            p[1] == 0x01
                && chunk(5, 0x3A, 0x20)
                && p.get(5..5 + usize::from(p[4]))
                    .is_some_and(|values| values.iter().all(|v| *v <= 50))
                && same_table()
        }
        _ => false,
    }
}

/// Explicit checks for the forms that must never appear.
fn assert_not_forbidden(p: &[u8; 63], what: &str) {
    assert!(
        !matches!(p[0], 0x14 | 0x08 | 0x24 | 0x25),
        "{what}: forbidden command {:#04x}",
        p[0]
    );
    if p[0] == 0x17 {
        assert_eq!(p[5], 0, "{what}: music-rhythm enter");
    }
    assert!(p[..2] != [0x09, 0x01], "{what}: custom table 2");
    if p[0] == 0x21 {
        assert!(
            !matches!(
                p[5],
                0x02 | 0x03 | 0x04 | 0x06 | 0x07 | 0x08 | 0x09 | 0x0F | 0x10 | 0x11
            ),
            "{what}: forbidden 0x21 selector {:#04x}",
            p[5]
        );
        if p[4] == 0x18 {
            assert!(p[6..28].iter().any(|b| *b != 0), "{what}: empty mask");
        }
    }
    if p[0] == 0x22 {
        assert!(p[5..].iter().all(|b| *b == 0), "{what}: non-empty slot");
    }
    if matches!(p[0], 0x09 | 0x18 | 0x19 | 0x22 | 0x26) {
        assert!(p[1] < 0x80, "{what}: read sub-command in a write");
    }
    assert_eq!(
        ReadRequest::from_packet(p),
        None,
        "{what}: a write packet is also a read"
    );
}

/// Checks every packet of `request`, the order rules and that W2 appears
/// exactly when the mode changes (`current_mode`: the mode before a
/// main-light write).
fn check_request(request: &WriteRequest, current_mode: Option<u8>) -> usize {
    let steps = request.steps();
    assert!(!steps.is_empty(), "{request}");
    let mut previous: Option<&[u8; 63]> = None;
    for (i, step) in steps.iter().enumerate() {
        let p = step.packet.as_bytes();
        let what = format!("{request} step {i}: {}", step.packet);
        assert!(
            is_allowed_write(p, previous),
            "{what} is not an allowed write"
        );
        assert_not_forbidden(p, &what);
        previous = Some(p);
    }
    let leaves: Vec<usize> = steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.packet.cmd() == 0x17)
        .map(|(i, _)| i)
        .collect();
    if let WriteRequest::MainLight(write) = request {
        let target = write.record().as_bytes()[0];
        let current = current_mode.expect("main-light samples know the current mode");
        assert_eq!(write.current_mode(), current);
        if target == current {
            assert_eq!(steps.len(), 1, "{request}");
            assert!(leaves.is_empty(), "{request}: leave without a mode change");
        } else {
            assert_eq!(leaves, vec![0], "{request}");
            assert_eq!(steps.len(), 2);
            assert_eq!(steps[1].packet.cmd(), 0x07);
            assert_ne!(steps[1].packet.as_bytes()[5], current);
        }
    } else {
        assert!(leaves.is_empty(), "{request}: leave outside a light write");
    }
    steps.len()
}

// ---------------------------------------------------------------------------
// Sample construction.
// ---------------------------------------------------------------------------

/// xorshift64*: small, deterministic test data.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn byte(&mut self) -> u8 {
        self.next().to_le_bytes()[0]
    }

    fn below(&mut self, n: u8) -> u8 {
        u8::try_from(self.next() % u64::from(n)).unwrap()
    }
}

fn w75_light_limits() -> LightLimits {
    let mut payload = W75_MODES.to_vec();
    payload.extend_from_slice(&[0xa5, 0x5a, 0x04, 0x04]);
    let list =
        LightModeList::from_response(&single(ReadRequest::MainLightModes, 0x0A, 0, &payload))
            .unwrap();
    LightLimits::new(&W75, &list)
}

fn w75_trigger_limits() -> TriggerLimits {
    let global = TriggerGlobal::from_response(&single(
        ReadRequest::TriggerGlobal,
        0x21,
        0,
        &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
    ))
    .unwrap();
    TriggerLimits::new(&W75, &global)
}

fn light_record(bytes: [u8; 12]) -> LightRecord {
    LightRecord::from_response(&single(ReadRequest::MainLight, 0x07, 0x00, &bytes)).unwrap()
}

fn keymap(layer: Layer, bytes: &[u8]) -> KeymapData {
    KeymapData::from_response(&table(ReadRequest::Keymap(layer), bytes))
        .unwrap()
        .1
}

fn macro_data(slot: MacroSlotId, bytes: &[u8]) -> MacroData {
    MacroData::from_response(&table(ReadRequest::Macro(slot), bytes))
        .unwrap()
        .1
}

fn colors(bytes: &[u8]) -> ColorTableData {
    ColorTableData::from_response(&table(
        ReadRequest::CustomColors(CustomTable::Custom1),
        bytes,
    ))
    .unwrap()
}

fn dead_band(bytes: &[u8]) -> DeadBandWrite {
    let data = DeadBandData::from_response(&table(ReadRequest::DeadBand, bytes)).unwrap();
    DeadBandWrite::new(&W75, data, DeadBandData::max_units(20)).unwrap()
}

/// c13 macro slot 0.
fn c13_macro() -> Vec<u8> {
    let mut bytes = vec![0u8; 256];
    bytes[..24].copy_from_slice(&[
        0x00, 0x00, 0x00, 0x10, 0x00, 0x01, 0x00, 0x00, 0x04, 0x10, 0x00, 0x64, 0x04, 0x00, 0x00,
        0x64, 0x05, 0x10, 0x00, 0x64, 0x05, 0x00, 0x00, 0x00,
    ]);
    bytes
}

/// A random macro slot in the canonical flat layout.
fn random_canonical_macro(rng: &mut Rng) -> Vec<u8> {
    let mut bytes = vec![0u8; 256];
    let steps = 1 + usize::from(rng.below(62));
    bytes[0] = rng.byte();
    bytes[1] = rng.below(4);
    bytes[3] = u8::try_from(steps * 4).unwrap();
    bytes[4] = rng.byte();
    bytes[5] = rng.byte();
    for k in 0..steps {
        let at = 8 + 4 * k;
        bytes[at] = rng.byte();
        bytes[at + 1] = [0x00, 0x01, 0x10, 0x11][usize::from(rng.below(4))];
        bytes[at + 2] = rng.byte();
        bytes[at + 3] = rng.byte();
    }
    bytes
}

fn pos(row: u8, col: u8) -> KeyPos {
    KeyPos::new(row, col).unwrap()
}

fn trigger(keys: KeyMask, mode: TriggerWriteMode, a: u16, p: u16, r: u16) -> WriteRequest {
    let setting = TriggerSetting {
        mode,
        actuation: Distance(a),
        rt_press: Distance(p),
        rt_release: Distance(r),
    };
    WriteRequest::Trigger(TriggerWrite::new(&W75, keys, setting, &w75_trigger_limits()).unwrap())
}

/// A bounded sample of every kind of constructible request, with the mode
/// before each main-light write.
fn sample() -> Vec<(WriteRequest, Option<u8>)> {
    let mut rng = Rng(0x0057_5241_4954_4832);
    let mut out: Vec<(WriteRequest, Option<u8>)> = Vec::new();
    let mut push = |request: WriteRequest| out.push((request, None));

    // W1: all four profiles.
    for profile in ProfileId::ALL {
        push(WriteRequest::SelectProfile(profile));
    }

    // W4: custom colours 1.
    push(WriteRequest::CustomColors1(colors(&[0u8; 396])));
    push(WriteRequest::CustomColors1(colors(&[0xFF; 396])));
    let random: Vec<u8> = (0..396).map(|_| rng.byte()).collect();
    push(WriteRequest::CustomColors1(colors(&random)));

    // W5: both layers; zeros, the site's knob defaults + W -> A, random.
    let mut knob = vec![0u8; 528];
    knob[64..88].copy_from_slice(&[
        0x07, 0x12, 0, 0, 0x07, 0x11, 0, 0, 0x07, 0x10, 0, 0, 0x02, 0x02, 0, 0, 0x03, 0x01, 0, 0,
        0x03, 0x00, 0, 0,
    ]);
    knob[184..188].copy_from_slice(&[0x00, 0x04, 0, 0]);
    knob[484] = 0x01;
    let random: Vec<u8> = (0..528).map(|_| rng.byte()).collect();
    for layer in Layer::ALL {
        for bytes in [&[0u8; 528][..], &knob, &random] {
            push(WriteRequest::Keymap(layer, keymap(layer, bytes)));
        }
    }

    // W6: every slot; empty (from 00 and FF), the c13 slot, random canonical.
    for slot in MacroSlotId::all() {
        push(WriteRequest::Macro(slot, macro_data(slot, &[0u8; 256])));
        push(WriteRequest::Macro(slot, macro_data(slot, &[0xFF; 256])));
        push(WriteRequest::Macro(slot, macro_data(slot, &c13_macro())));
        for _ in 0..5 {
            let bytes = random_canonical_macro(&mut rng);
            push(WriteRequest::Macro(slot, macro_data(slot, &bytes)));
        }
    }

    // W7: all 40 advanced-key slots.
    for slot in AdvancedSlotId::all() {
        push(WriteRequest::AdvancedKeyClear(slot));
    }

    // W8: every selectable key x 3 modes x boundary values, plus Select All.
    use TriggerWriteMode::{Off, RapidTrigger, RapidTriggerSeparate};
    for key in W75.selectable_mask().positions() {
        let mask = KeyMask::from_positions([key]);
        for (mode, a, p, r) in [
            (Off, 5, 0, 0),
            (Off, 170, 85, 85),
            (Off, 0x55, 1, 1),
            (RapidTrigger, 5, 1, 1),
            (RapidTrigger, 170, 85, 85),
            (RapidTriggerSeparate, 5, 1, 85),
            (RapidTriggerSeparate, 170, 85, 1),
        ] {
            push(trigger(mask, mode, a, p, r));
        }
    }
    push(trigger(
        W75.selectable_mask(),
        RapidTrigger,
        0x32,
        0x0f,
        0x0f,
    ));

    // W9: zeros, W top/bottom, random within 0..=50.
    push(WriteRequest::DeadBand(dead_band(&[0u8; 264])));
    let mut w = vec![0u8; 264];
    w[92] = 5;
    w[93] = 10;
    push(WriteRequest::DeadBand(dead_band(&w)));
    let random: Vec<u8> = (0..264).map(|_| rng.below(51)).collect();
    push(WriteRequest::DeadBand(dead_band(&random)));

    // W2 + W3: every list mode plus Custom x brightness x speed x change.
    let limits = w75_light_limits();
    for target_mode in W75_MODES.into_iter().chain([0x0A]) {
        for brightness in 0..=4 {
            for speed in 0..=4 {
                for change in [false, true] {
                    let current_mode = match (change, target_mode) {
                        (false, m) => m,
                        (true, 0x02) => 0x0A,
                        (true, _) => 0x02,
                    };
                    let current =
                        light_record([current_mode, 4, 3, 0xff, 0xff, 0xff, 0, 0, 0, 0, 1, 0]);
                    let target = current
                        .with_mode(LightMode::from_id(target_mode))
                        .with_brightness(brightness)
                        .with_speed(speed)
                        .with_foreground(Rgb::new(rng.byte(), rng.byte(), rng.byte()))
                        .with_direction(LightRecord::default_direction(LightMode::from_id(
                            target_mode,
                        )));
                    let write = MainLightWrite::new(&current, target, &limits).unwrap();
                    out.push((WriteRequest::MainLight(write), Some(current_mode)));
                }
            }
        }
    }
    out
}

#[test]
fn every_sampled_request_produces_only_allowed_writes() {
    let sample = sample();
    let mut packets = 0;
    let mut kinds = std::collections::BTreeMap::new();
    for (request, current_mode) in &sample {
        packets += check_request(request, *current_mode);
        *kinds.entry(request.kind()).or_insert(0) += 1;
        // Compile-time reminder: a new variant must be sampled here.
        match request {
            WriteRequest::SelectProfile(_)
            | WriteRequest::MainLight(_)
            | WriteRequest::CustomColors1(_)
            | WriteRequest::Keymap(..)
            | WriteRequest::Macro(..)
            | WriteRequest::AdvancedKeyClear(_)
            | WriteRequest::Trigger(_)
            | WriteRequest::DeadBand(_) => {}
        }
    }
    assert_eq!(kinds.len(), 8, "every variant is sampled: {kinds:?}");
    assert_eq!(kinds["select_profile"], 4);
    assert_eq!(kinds["advanced_key_clear"], 40);
    assert_eq!(kinds["trigger"], 81 * 7 + 1);
    assert_eq!(kinds["main_light"], 15 * 5 * 5 * 2);
    // 4 + 3 x 8 + 6 x 10 + 80 x 5 + 40 + (567 + 81) + 3 x 5 + 375 x 2 + 375.
    assert_eq!(packets, 2316);
}

#[test]
fn write_packets_are_never_reads() {
    for (request, _) in sample() {
        for step in request.steps() {
            assert_eq!(
                ReadRequest::from_packet(step.packet.as_bytes()),
                None,
                "{request}: {}",
                step.packet
            );
        }
    }
    // And no read request is an allowed write.
    for read in ReadRequest::all() {
        assert!(
            !is_allowed_write(read.packet().as_bytes(), None),
            "{read} looks like a write"
        );
    }
}

#[test]
fn the_predicate_rejects_the_forbidden_forms() {
    let packet = |prefix: &[u8]| {
        let mut bytes = [0u8; 63];
        bytes[..prefix.len()].copy_from_slice(prefix);
        bytes
    };
    let mut w_mask = [0x21, 0, 0, 0, 0x18, 0x0C, 0, 0, 0x04].to_vec();
    w_mask.resize(28, 0);
    w_mask.extend_from_slice(&[0x32, 0x32, 0x0f, 0x0f]);
    assert!(is_allowed_write(&packet(&w_mask), None), "c03 W write");
    let forbidden: [&[u8]; 16] = [
        &[0x14, 0, 0, 0, 0x01, 0x01],          // reset
        &[0x14, 0x03, 0, 0, 0x01, 0x01],       // gamepad
        &[0x17, 0, 0, 0, 0x02, 0x01, 0x01],    // music-rhythm enter
        &[0x08, 0, 0, 0, 0x0E, 0x02],          // side light
        &[0x09, 0x01, 0, 0, 0x36],             // custom table 2
        &[0x24, 0x03, 0, 0, 0x01, 0x01],       // SOCD power
        &[0x24, 0x00, 0, 0, 0x28],             // SOCD pairs
        &[0x25, 0x01, 0, 0, 0x3A],             // switch types
        &[0x21, 0, 0, 0, 0x18, 0x06],          // reset triggers
        &[0x21, 0, 0, 0, 0x18, 0x0F, 0x00],    // calibration start
        &[0x21, 0, 0, 0, 0x18, 0x10, 0x00],    // calibration stop
        &[0x21, 0, 0, 0, 0x18, 0x03],          // travel test close
        &[0x21, 0, 0, 0, 0x00, 0x09, 0x01, 8], // set poll rate
        &[0x21, 0, 0, 0, 0x18, 0x00],          // empty-mask trigger write
        &[0x22, 0x00, 0, 0, 0x32, 0x01],       // non-empty advanced key
        &[0x07, 0, 0, 0, 0x0E, 0x64],          // music-rhythm mode
    ];
    for prefix in forbidden {
        assert!(!is_allowed_write(&packet(prefix), None), "{prefix:02x?}");
    }
    // A leave is only allowed first; a light write only after a leave.
    let leave = packet(&LEAVE);
    let light = packet(&[0x07, 0, 0, 0, 0x0E, 0x02, 4, 3]);
    assert!(is_allowed_write(&light, Some(&leave)));
    assert!(!is_allowed_write(&leave, Some(&light)));
    // A two-key mask is not allowed by default (never captured).
    let mut two = packet(&w_mask);
    two[7] = 0x04;
    assert!(!is_allowed_write(&two, None));
}

// ---------------------------------------------------------------------------
// Steps, acks and gaps.
// ---------------------------------------------------------------------------

#[test]
fn chunking_matches_the_wire_format() {
    let lens = |request: &WriteRequest| -> Vec<u8> {
        request
            .steps()
            .iter()
            .map(|s| s.packet.as_bytes()[4])
            .collect()
    };
    let indices = |request: &WriteRequest| -> Vec<u8> {
        request
            .steps()
            .iter()
            .map(|s| s.packet.as_bytes()[3])
            .collect()
    };
    let colors = WriteRequest::CustomColors1(colors(&[0u8; 396]));
    assert_eq!(
        lens(&colors),
        [0x36, 0x36, 0x36, 0x36, 0x36, 0x36, 0x36, 0x12]
    );
    assert_eq!(indices(&colors), (0..8).collect::<Vec<u8>>());
    let fn_layer = WriteRequest::Keymap(Layer::Fn, keymap(Layer::Fn, &[0u8; 528]));
    assert_eq!(
        lens(&fn_layer),
        [0x38; 9].into_iter().chain([0x18]).collect::<Vec<u8>>()
    );
    assert!(fn_layer.steps().iter().all(|s| s.packet.sub() == 0x02));
    let base = WriteRequest::Keymap(Layer::Base, keymap(Layer::Base, &[0u8; 528]));
    assert!(base.steps().iter().all(|s| s.packet.sub() == 0x00));
    let slot = MacroSlotId::new(7).unwrap();
    let macro_write = WriteRequest::Macro(slot, macro_data(slot, &[0xFF; 256]));
    assert_eq!(lens(&macro_write), [0x3A, 0x3A, 0x3A, 0x3A, 0x18]);
    assert!(macro_write.steps().iter().all(|s| s.packet.sub() == 0x07));
    assert!(
        macro_write
            .steps()
            .iter()
            .all(|s| s.packet.as_bytes()[5..].iter().all(|b| *b == 0)),
        "an empty slot is written as zeros, never FF"
    );
    let band = WriteRequest::DeadBand(dead_band(&[0u8; 264]));
    assert_eq!(lens(&band), [0x3A, 0x3A, 0x3A, 0x3A, 0x20]);
}

#[test]
fn acks_and_gaps() {
    let header = |request: &WriteRequest| request.steps()[0].ack;
    let report_ack = |h: [u8; 5]| AckSpec::Report {
        header: h,
        selector: None,
        required: true,
    };
    let profile = ProfileId::new(3).unwrap();
    assert_eq!(
        header(&WriteRequest::SelectProfile(profile)),
        AckSpec::ProfileSelected(profile)
    );
    let colors = WriteRequest::CustomColors1(colors(&[0u8; 396]));
    assert!(
        colors
            .steps()
            .iter()
            .all(|s| s.gap_after == COLOR_CHUNK_GAP && s.ack == report_ack([0x09, 0, 0, 0, 0]))
    );
    for layer in Layer::ALL {
        let request = WriteRequest::Keymap(layer, keymap(layer, &[0u8; 528]));
        // Byte 1 is not echoed: both layers are acked with `18 00`.
        assert!(
            request
                .steps()
                .iter()
                .all(|s| s.ack == report_ack([0x18, 0, 0, 0, 0]) && s.gap_after.is_zero())
        );
    }
    let slot = MacroSlotId::new(9).unwrap();
    assert_eq!(
        header(&WriteRequest::Macro(slot, MacroData::Empty)),
        report_ack([0x19, 0, 0, 0, 0])
    );
    assert_eq!(
        header(&WriteRequest::AdvancedKeyClear(
            AdvancedSlotId::new(39).unwrap()
        )),
        report_ack([0x22, 0, 0, 0, 0])
    );
    assert_eq!(
        header(&WriteRequest::DeadBand(dead_band(&[0u8; 264]))),
        report_ack([0x26, 0x01, 0, 0, 0])
    );
    let w = trigger(
        KeyMask::from_positions([pos(2, 2)]),
        TriggerWriteMode::RapidTrigger,
        0x32,
        0x0f,
        0x0f,
    );
    assert_eq!(
        header(&w),
        AckSpec::Report {
            header: [0x21, 0, 0, 0, 0x01],
            selector: Some(0x00),
            required: true
        }
    );
    // Main light: optional leave ack, no ack for 07, then the settle time.
    let limits = w75_light_limits();
    let current = light_record([0x02, 4, 3, 0xff, 0xff, 0xff, 0, 0, 0, 0, 1, 0]);
    let write =
        MainLightWrite::new(&current, current.with_mode(LightMode::Custom), &limits).unwrap();
    let steps = WriteRequest::MainLight(write).steps();
    assert!(!steps[0].ack.is_required());
    assert!(steps[0].ack.expects_reply());
    assert_eq!(steps[1].ack, AckSpec::None);
    assert!(!steps[1].ack.expects_reply());
    assert_eq!(steps[1].gap_after, LIGHT_SETTLE);
}

#[test]
fn trigger_grouping() {
    let keys = KeyMask::from_positions([pos(2, 1), pos(2, 2)]);
    let single_key = trigger(keys, TriggerWriteMode::Off, 0x4b, 1, 1);
    let steps = single_key.steps();
    assert_eq!(steps.len(), 2, "one packet per key by default");
    assert_eq!(steps[0].packet.as_bytes()[6..10], [0, 0x04, 0, 0]);
    assert_eq!(steps[1].packet.as_bytes()[6..10], [0, 0, 0x04, 0]);
    let WriteRequest::Trigger(write) = single_key else {
        unreachable!()
    };
    let grouped = WriteRequest::Trigger(write.with_grouping(TriggerGrouping::SameSetting));
    let steps = grouped.steps();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].packet.as_bytes()[6..10], [0, 0x04, 0x04, 0]);
    assert_eq!(
        grouped.verify_reads(),
        vec![
            ReadRequest::KeyTrigger(pos(2, 1)),
            ReadRequest::KeyTrigger(pos(2, 2))
        ]
    );
}

#[test]
fn ack_tracker() {
    let ack = |prefix: &[u8]| report(prefix);
    // Profile select: ack and push in either order (bytes 4 and 7 of the push ignored).
    let two = ProfileId::new(2).unwrap();
    for order in [[0usize, 1], [1, 0]] {
        let reports = [
            ack(&[0x21, 0, 0, 0, 0x02, 0x12, 0x01]),
            ack(&[0x21, 0, 0, 0, 0x03, 0x11, 0x02, 0x00]),
        ];
        let mut tracker = AckSpec::ProfileSelected(two).tracker();
        assert!(!tracker.is_complete());
        for i in order {
            assert_ne!(tracker.offer(&reports[i]), AckOffer::Ignored);
        }
        assert!(tracker.is_complete());
        assert!(tracker.ack_received() && tracker.push_received());
        // Second copies are not taken again.
        assert_eq!(tracker.offer(&reports[0]), AckOffer::Ignored);
    }
    let mut tracker = AckSpec::ProfileSelected(two).tracker();
    // A push for another profile, a status report, a trigger ack.
    for other in [
        ack(&[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x00]),
        ack(&[0x01, 0, 0, 0, 0x12]),
        ack(&[0x21, 0, 0, 0, 0x01, 0x00]),
    ] {
        assert_eq!(tracker.offer(&other), AckOffer::Ignored);
    }
    assert!(!tracker.is_complete());

    // Trigger ack: byte 5 must be 00; stale bytes after the length are ignored.
    let spec = trigger(
        KeyMask::from_positions([pos(2, 2)]),
        TriggerWriteMode::Off,
        0x55,
        1,
        1,
    )
    .steps()[0]
        .ack;
    let mut tracker = spec.tracker();
    assert_eq!(
        tracker.offer(&ack(&[0x21, 0, 0, 0, 0x02, 0x12, 0x01])),
        AckOffer::Ignored
    );
    assert_eq!(
        tracker.offer(&ack(&[0x21, 0, 0, 0, 0x01, 0x05])),
        AckOffer::Ignored
    );
    assert_eq!(
        tracker.offer(&ack(&[0x21, 0, 0, 0, 0x01, 0x00, 0xEE, 0xEE])),
        AckOffer::Ack
    );
    assert!(tracker.is_complete());
    assert_eq!(tracker.spec(), spec);

    // Table acks never echo a chunk index.
    let mut tracker = AckSpec::Report {
        header: [0x18, 0, 0, 0, 0],
        selector: None,
        required: true,
    }
    .tracker();
    assert_eq!(
        tracker.offer(&ack(&[0x18, 0x02, 0, 0, 0])),
        AckOffer::Ignored
    );
    assert_eq!(
        tracker.offer(&ack(&[0x18, 0x80, 0, 3, 0x38])),
        AckOffer::Ignored
    );
    assert_eq!(
        tracker.offer(&ack(&[0x18, 0, 0, 0, 0, 0x55])),
        AckOffer::Ack
    );
    // No-ack writes are complete at once.
    assert!(AckSpec::None.tracker().is_complete());
    assert_eq!(
        AckSpec::None.tracker().offer(&ack(&[0x07, 0, 0, 0, 0x0c])),
        AckOffer::Ignored
    );
}

#[test]
fn write_ack_shapes() {
    for prefix in [
        &[0x21, 0, 0, 0, 0x01, 0x00][..],
        &[0x21, 0, 0, 0, 0x02, 0x12, 0x01],
        &[0x09, 0, 0, 0, 0],
        &[0x17, 0, 0, 0, 0],
        &[0x18, 0, 0, 0, 0],
        &[0x19, 0, 0, 0, 0],
        &[0x22, 0, 0, 0, 0],
        &[0x26, 0x01, 0, 0, 0],
    ] {
        assert!(is_write_ack(&report(prefix)), "{prefix:02x?}");
    }
    for prefix in [
        &[0x21, 0, 0, 0, 0x03, 0x11, 0x01, 0x00][..], // profile report
        &[0x21, 0, 0, 0, 0x0c, 0x05],                 // trigger read reply
        &[0x01, 0, 0, 0, 0x12],                       // status
        &[0x09, 0x01, 0, 0, 0],                       // Custom 2 active
        &[0x18, 0x80, 0, 0, 0x38],                    // keymap chunk
        &[0x26, 0x00, 0, 0, 0x3A],                    // dead-band chunk
        &[0x14, 0, 0, 0, 0x01, 0x01],                 // reset notification
    ] {
        assert!(!is_write_ack(&report(prefix)), "{prefix:02x?}");
    }
}

// ---------------------------------------------------------------------------
// Read-back checks.
// ---------------------------------------------------------------------------

fn key_reply(key: KeyPos, record: [u8; 9]) -> RawResponse {
    let mut prefix = vec![0x21, 0, 0, 0, 0x0c, 0x05];
    prefix.extend_from_slice(&record);
    prefix.extend_from_slice(&[key.row(), key.col()]);
    RawResponse::from_reports(ReadRequest::KeyTrigger(key), [report(&prefix)]).unwrap()
}

#[test]
fn check_profile_and_light() {
    let two = WriteRequest::SelectProfile(ProfileId::new(2).unwrap());
    assert_eq!(two.verify_reads(), vec![ReadRequest::ActiveProfile]);
    let reply = |p| single(ReadRequest::ActiveProfile, 0x21, 0, &[0x11, p, 0x01]);
    assert_eq!(two.check(&[reply(2)]), Ok(()));
    let err = two.check(&[reply(1)]).unwrap_err();
    assert_eq!(err.kind, MismatchKind::Differs);
    assert_eq!(err.table, TableId::Profile);
    assert_eq!(
        (err.expected.0.clone(), err.got.0.clone()),
        (vec![2], vec![1])
    );
    assert_eq!(two.check(&[]).unwrap_err().kind, MismatchKind::MissingReply);
    let short = single(ReadRequest::ActiveProfile, 0x21, 0, &[0x11]);
    assert_eq!(
        two.check(&[short]).unwrap_err().kind,
        MismatchKind::InvalidReply
    );

    let limits = w75_light_limits();
    let record = [0x0a, 4, 3, 0xff, 0, 0, 0, 0, 0, 0, 1, 0];
    let current = light_record([0x02, 4, 3, 0xff, 0xff, 0xff, 0, 0, 0, 0, 1, 0]);
    let light = WriteRequest::MainLight(
        MainLightWrite::new(&current, light_record(record), &limits).unwrap(),
    );
    let reply = |bytes: &[u8]| single(ReadRequest::MainLight, 0x07, 0x00, bytes);
    assert_eq!(light.check(&[reply(&record)]), Ok(()));
    let mut dimmer = record;
    dimmer[1] = 3;
    let err = light.check(&[reply(&dimmer)]).unwrap_err();
    assert_eq!(err.differing_offsets(), vec![1]);
    // A longer declared payload is not the record.
    let mut longer = record.to_vec();
    longer.extend_from_slice(&[0, 0]);
    assert!(light.check(&[reply(&longer)]).is_err());
}

#[test]
fn check_tables() {
    let mut bytes = vec![0u8; 528];
    bytes[264..268].copy_from_slice(&[0x10, 0x39, 0, 0]);
    let request = WriteRequest::Keymap(Layer::Base, keymap(Layer::Base, &bytes));
    let same = table(ReadRequest::Keymap(Layer::Base), &bytes);
    let zeros = table(ReadRequest::Keymap(Layer::Base), &[0u8; 528]);
    let fn_layer = table(ReadRequest::Keymap(Layer::Fn), &bytes);
    // Other replies are ignored; of two replies to the same read the last counts.
    assert_eq!(
        request.check(&[fn_layer.clone(), zeros.clone(), same.clone()]),
        Ok(())
    );
    assert!(request.check(&[same, zeros.clone()]).is_err());
    let err = request.check(&[zeros]).unwrap_err();
    assert_eq!(err.differing_offsets(), vec![264, 265]);
    assert_eq!(
        err.to_string(),
        format!(
            "base keymap: {} (base keymap (18 80))",
            MismatchKind::Differs
        )
    );
    // The Fn layer's reply does not verify the base layer.
    assert_eq!(
        request.check(&[fn_layer]).unwrap_err().kind,
        MismatchKind::MissingReply
    );
    // Malformed chunks.
    let reports = (0..10u8).map(|i| report(&[0x18, 0x80, 0, i, 0x38]));
    let bad = RawResponse::from_reports(ReadRequest::Keymap(Layer::Base), reports).unwrap();
    assert_eq!(
        request.check(&[bad]).unwrap_err().kind,
        MismatchKind::InvalidReply
    );

    let mut band = vec![0u8; 264];
    band[92] = 5;
    band[93] = 10;
    let request = WriteRequest::DeadBand(dead_band(&band));
    assert_eq!(
        request.check(&[table(ReadRequest::DeadBand, &band)]),
        Ok(())
    );
    assert!(
        request
            .check(&[table(ReadRequest::DeadBand, &[0u8; 264])])
            .is_err()
    );

    let request = WriteRequest::CustomColors1(colors(&[0x11; 396]));
    let custom2 = table(
        ReadRequest::CustomColors(CustomTable::Custom2),
        &[0x11; 396],
    );
    assert_eq!(
        request.check(&[custom2]).unwrap_err().kind,
        MismatchKind::MissingReply
    );
    let custom1 = table(
        ReadRequest::CustomColors(CustomTable::Custom1),
        &[0x11; 396],
    );
    assert_eq!(request.check(&[custom1]), Ok(()));
}

#[test]
fn check_macros_with_empty_equivalence() {
    let slot = MacroSlotId::new(4).unwrap();
    let read = |bytes: &[u8]| table(ReadRequest::Macro(slot), bytes);
    let empty = WriteRequest::Macro(slot, MacroData::Empty);
    assert_eq!(empty.check(&[read(&[0u8; 256])]), Ok(()));
    assert_eq!(empty.check(&[read(&[0xFF; 256])]), Ok(()));
    assert!(empty.check(&[read(&c13_macro())]).is_err());
    let content = WriteRequest::Macro(slot, macro_data(slot, &c13_macro()));
    assert_eq!(content.check(&[read(&c13_macro())]), Ok(()));
    assert!(content.check(&[read(&[0xFF; 256])]).is_err());
}

#[test]
fn check_advanced_key_clear() {
    let slot = AdvancedSlotId::new(5).unwrap();
    let request = WriteRequest::AdvancedKeyClear(slot);
    assert_eq!(request.verify_reads(), vec![ReadRequest::AdvancedKeys]);
    let mut bytes = vec![0u8; 1600];
    // Another slot may hold anything.
    bytes[6 * 40] = 0x02;
    assert_eq!(
        request.check(&[table(ReadRequest::AdvancedKeys, &bytes)]),
        Ok(())
    );
    bytes[5 * 40 + 39] = 0x01;
    let err = request
        .check(&[table(ReadRequest::AdvancedKeys, &bytes)])
        .unwrap_err();
    assert_eq!(err.table, TableId::AdvancedKey { slot });
    assert_eq!(err.differing_offsets(), vec![39]);
}

#[test]
fn check_triggers_per_key() {
    let (q, w) = (pos(2, 1), pos(2, 2));
    let request = trigger(
        KeyMask::from_positions([q, w]),
        TriggerWriteMode::RapidTriggerSeparate,
        0x55,
        0x0f,
        0x19,
    );
    assert_eq!(
        request.verify_reads(),
        vec![ReadRequest::KeyTrigger(q), ReadRequest::KeyTrigger(w)]
    );
    let good = [0x0d, 0x55, 0x55, 0x0f, 0x19, 0, 0, 0, 0];
    assert_eq!(
        request.check(&[key_reply(w, good), key_reply(q, good)]),
        Ok(())
    );
    let factory = [0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0];
    let err = request
        .check(&[key_reply(q, good), key_reply(w, factory)])
        .unwrap_err();
    assert_eq!(err.read, ReadRequest::KeyTrigger(w));
    assert_eq!(err.differing_offsets(), vec![0, 3, 4]);
    assert_eq!(
        request.check(&[key_reply(q, good)]).unwrap_err().kind,
        MismatchKind::MissingReply
    );
}

// ---------------------------------------------------------------------------
// Names, serialization, packet type.
// ---------------------------------------------------------------------------

#[test]
fn tables_and_names() {
    let slot = MacroSlotId::new(3).unwrap();
    let cases = [
        (
            WriteRequest::SelectProfile(ProfileId::new(1).unwrap()),
            TableId::Profile,
            "select_profile",
            "select profile 1 (21/12)",
        ),
        (
            WriteRequest::Macro(slot, MacroData::Empty),
            TableId::Macro { slot },
            "macro",
            "macro slot 3 (19 03)",
        ),
        (
            WriteRequest::AdvancedKeyClear(AdvancedSlotId::new(39).unwrap()),
            TableId::AdvancedKey {
                slot: AdvancedSlotId::new(39).unwrap(),
            },
            "advanced_key_clear",
            "clear advanced-key slot 39 (22 27)",
        ),
        (
            WriteRequest::CustomColors1(colors(&[0u8; 396])),
            TableId::CustomColors {
                table: CustomTable::Custom1,
            },
            "custom_colors1",
            "custom colours 1 (09 00)",
        ),
        (
            WriteRequest::Keymap(Layer::Fn, keymap(Layer::Fn, &[0u8; 528])),
            TableId::Keymap { layer: Layer::Fn },
            "keymap",
            "Fn keymap (18 02)",
        ),
        (
            WriteRequest::DeadBand(dead_band(&[0u8; 264])),
            TableId::DeadBand,
            "dead_band",
            "dead band (26 01)",
        ),
        (
            trigger(
                KeyMask::from_positions([pos(2, 1), pos(2, 2)]),
                TriggerWriteMode::Off,
                0x55,
                1,
                1,
            ),
            TableId::Triggers,
            "trigger",
            "triggers of r2c1, r2c2 (21)",
        ),
    ];
    for (request, table_id, kind, display) in cases {
        assert_eq!(request.table(), table_id);
        assert_eq!(request.kind(), kind);
        assert_eq!(request.to_string(), display);
    }
    assert_eq!(TableId::Macro { slot }.to_string(), "macro slot 3");
    assert_eq!(
        TableId::CustomColors {
            table: CustomTable::Custom2
        }
        .to_string(),
        "custom colours 2"
    );
}

#[test]
fn packet_type() {
    let request = WriteRequest::SelectProfile(ProfileId::new(2).unwrap());
    let packet = &request.steps()[0].packet;
    assert_eq!(packet.cmd(), 0x21);
    assert_eq!(packet.sub(), 0x00);
    assert_eq!(packet.summary(), "21 00 00 00 00 12 02");
    assert_eq!(format!("{packet:?}"), "WritePacket(21 00 00 00 00 12 02)");
    assert_eq!(packet.to_string(), "21 00 00 00 00 12 02");
    let hid = packet.to_hid_report();
    assert_eq!(hid[0], crate::REPORT_ID);
    assert_eq!(&hid[1..], packet.as_bytes());
}

#[cfg(feature = "serde")]
#[test]
fn serialization() {
    let request = WriteRequest::AdvancedKeyClear(AdvancedSlotId::new(5).unwrap());
    let step = &request.steps()[0];
    let json = serde_json::to_value(step).unwrap();
    assert_eq!(json["gap_after_ms"], 0);
    assert_eq!(json["packet"].as_str().map(str::len), Some(126));
    assert!(json["packet"].as_str().unwrap().starts_with("2205000032"));
    assert_eq!(json["ack"]["report"]["header"][0], 0x22);
    assert_eq!(
        serde_json::to_string(&request).unwrap(),
        r#"{"advanced_key_clear":5}"#
    );
    let colors_step = &WriteRequest::CustomColors1(colors(&[0u8; 396])).steps()[0];
    assert_eq!(
        serde_json::to_value(colors_step).unwrap()["gap_after_ms"],
        5
    );
    // TableId round-trips (it names tables in plans and reports).
    for table_id in [
        TableId::Profile,
        TableId::Macro {
            slot: MacroSlotId::new(9).unwrap(),
        },
        TableId::Keymap { layer: Layer::Fn },
        TableId::WinLockAndRollover,
    ] {
        let json = serde_json::to_string(&table_id).unwrap();
        assert_eq!(serde_json::from_str::<TableId>(&json).unwrap(), table_id);
    }
    assert_eq!(
        serde_json::to_string(&TableId::AdvancedKey {
            slot: AdvancedSlotId::new(2).unwrap()
        })
        .unwrap(),
        r#"{"advanced_key":{"slot":2}}"#
    );
    assert!(serde_json::from_str::<TableId>(r#"{"advanced_key":{"slot":40}}"#).is_err());
    let mismatch = WriteRequest::SelectProfile(ProfileId::new(2).unwrap())
        .check(&[])
        .unwrap_err();
    let json = serde_json::to_value(&mismatch).unwrap();
    assert_eq!(json["kind"], "missing_reply");
    assert_eq!(json["table"], "profile");
    assert_eq!(json["expected"], "02");
}
