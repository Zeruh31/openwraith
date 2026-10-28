//! Unit tests of the write module: every constructible request (a bounded
//! sample) is checked against an independent description of the write
//! forms (W1-W11, T1, T2), plus step, ack and read-back details.

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

/// W75 mask of all 82 positions, knob included: the only travel-test open
/// mask (spec section 2.9, c14 #981), spelled out.
const LAYOUT: [u8; 22] = [
    0x3f, 0x26, 0x3f, 0x1f, 0x1f, 0x1f, 0x3f, 0x1f, 0x1f, 0x1f, 0x3f, 0x3f, 0x2f, 0x31, 0x3f, 0x2f,
    0, 0, 0, 0, 0, 0,
];

/// The HID usages the vendor configurator offers (spec section 4.8.3),
/// spelled out group by group.
fn offered(u: u8) -> bool {
    matches!(
        u,
        0x04..=0x1D // A-Z
            | 0x1E..=0x27 // 1-0
            | 0x28..=0x2C // Enter Esc Backspace Tab Space
            | 0x2D..=0x31 // - = [ ] \
            | 0x33..=0x38 // ; ' ` , . /
            | 0x39 // Caps Lock
            | 0x3A..=0x45 // F1-F12
            | 0x46..=0x4E // PrtSc .. PgDn
            | 0x4F..=0x52 // arrows
            | 0x53..=0x63 // Num Lock, keypad
            | 0x64 // Intl backslash
            | 0x65 // Menu
            | 0x68..=0x73 // F13-F24
            | 0x87..=0x8B // JIS keys
            | 0xE0..=0xE7 // modifiers
    )
}

/// Whether `s` (write bytes 5-44) is a non-empty advanced-key slot in the
/// DKS, MT or TGL layout of spec section 4.14.
fn is_allowed_slot(s: &[u8]) -> bool {
    let zero = |from: usize, to: usize| s.get(from..to).is_some_and(|r| r.iter().all(|b| *b == 0));
    let key = |at: usize| {
        s.get(at..at + 4)
            .is_some_and(|a| a[0] == 0 && offered(a[1]) && a[2] == 0 && a[3] == 0)
    };
    if s.len() != 40 {
        return false;
    }
    match s[0] {
        // DKS: routes r1 r2 r3 r4, four rows of action + steps.
        1 => {
            let (r1, r2, r3, r4) = (s[1], s[2], s[3], s[4]);
            if !(1 <= r1 && r1 <= r2 && r2 == r3 && r2 <= 30 && r4 == r1) {
                return false;
            }
            let mut used = 0;
            for k in 0..4 {
                let at = 5 + 8 * k;
                if zero(at, at + 8) {
                    continue;
                }
                used += 1;
                let st = &s[at + 4..at + 8];
                let grammar = st[0] <= 2
                    && st[1] <= 3
                    && (st[1] != 3 || st[0] == 2)
                    && st[2] <= 3
                    && (st[2] != 3 || st[1] >= 2)
                    && st[3] <= 1
                    && st.iter().any(|v| *v != 0);
                if !(key(at) && grammar) {
                    return false;
                }
            }
            used > 0 && zero(37, 40)
        }
        // MT: hold, tap, threshold 15..=100.
        2 => key(1) && key(5) && (15..=100).contains(&s[9]) && zero(10, 40),
        // TGL: one key.
        3 => key(1) && zero(5, 40),
        _ => false,
    }
}

/// Whether the 10 SOCD entries of a chunk are in the site's form: used
/// entries `01 mode k1 k2` (mode 0-3, offered, k1 != k2) packed first, then
/// zeros. Returns the usages of the used entries.
fn socd_chunk(entries: &[u8]) -> Option<Vec<u8>> {
    let mut usages = Vec::new();
    let mut ended = false;
    for e in entries.as_chunks::<4>().0 {
        if e.iter().all(|b| *b == 0) {
            ended = true;
            continue;
        }
        if ended || e[0] != 1 || e[1] > 3 || !offered(e[2]) || !offered(e[3]) || e[2] == e[3] {
            return None;
        }
        usages.extend_from_slice(&e[2..4]);
    }
    Some(usages)
}

/// Whether `usages` holds no value twice.
fn distinct(usages: &[u8]) -> bool {
    usages
        .iter()
        .enumerate()
        .all(|(i, u)| !usages[..i].contains(u))
}

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
        // T1: travel test open, only with the full layout mask.
        0x21 if p[4] == 0x18 && p[5] == 0x02 => {
            p[..6] == [0x21, 0, 0, 0, 0x18, 0x02]
                && p[6..28] == LAYOUT
                && zero_from(28)
                && previous.is_none()
        }
        // T2: travel test close.
        0x21 if p[4] == 0x18 && p[5] == 0x03 => {
            p[..6] == [0x21, 0, 0, 0, 0x18, 0x03] && zero_from(6) && previous.is_none()
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
        // W7: advanced-key clear; W10: a DKS, MT or TGL slot.
        0x22 => {
            p[1] <= 0x27
                && p[2..5] == [0, 0, 0x32]
                && (zero_from(5) || (is_allowed_slot(&p[5..45]) && zero_from(45)))
                && previous.is_none()
        }
        // W11: SOCD power first, then pair chunk 0, then chunk 1.
        0x24 => match p[1] {
            0x03 => p[2..5] == [0, 0, 0x01] && p[5] <= 1 && zero_from(6) && previous.is_none(),
            0x00 => {
                let Some(mine) = socd_chunk(&p[5..45]) else {
                    return false;
                };
                let order = match (p[3], previous) {
                    (0, Some(q)) => q[..5] == [0x24, 0x03, 0, 0, 0x01],
                    (1, Some(q)) => match socd_chunk(&q[5..45]) {
                        // Chunk 1 holds pairs only when chunk 0 is full.
                        Some(first) if q[..5] == [0x24, 0, 0, 0, 0x28] => {
                            (mine.is_empty() || first.len() == 20)
                                && distinct(&[first, mine.clone()].concat())
                        }
                        _ => false,
                    },
                    _ => false,
                };
                p[2] == 0 && p[4] == 0x28 && zero_from(45) && distinct(&mine) && order
            }
            _ => false,
        },
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
    let zero_from = |i: usize| p[i..].iter().all(|b| *b == 0);
    assert!(
        !matches!(p[0], 0x14 | 0x08 | 0x25),
        "{what}: forbidden command {:#04x}",
        p[0]
    );
    if p[0] == 0x17 {
        assert_eq!(p[5], 0, "{what}: music-rhythm enter");
    }
    assert!(p[..2] != [0x09, 0x01], "{what}: custom table 2");
    if p[0] == 0x24 {
        assert!(
            matches!(p[1], 0x00 | 0x03),
            "{what}: SOCD sub-command {:#04x} written",
            p[1]
        );
        if p[1] == 0x03 {
            assert!(
                p[4] == 0x01 && p[5] <= 1 && zero_from(6),
                "{what}: SOCD power"
            );
        } else {
            assert!(
                p[2] == 0 && p[3] <= 1 && p[4] == 0x28 && zero_from(45),
                "{what}: SOCD pair chunk"
            );
        }
    }
    if p[0] == 0x21 {
        let travel = p[4] == 0x18 && matches!(p[5], 0x02 | 0x03);
        if travel && p[5] == 0x02 {
            assert_eq!(p[6..28], LAYOUT, "{what}: travel open with another mask");
            assert!(zero_from(28), "{what}: travel open");
        } else if travel {
            assert!(zero_from(6), "{what}: travel close");
        } else {
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
    }
    if p[0] == 0x22 {
        assert!(p[5] <= 3, "{what}: advanced-key type {} written", p[5]);
        if p[5] == 1 {
            assert!(p[6] <= p[7], "{what}: DKS first point below the deep point");
        }
        assert!(zero_from(45), "{what}: bytes after the 40-byte slot");
    }
    if matches!(p[0], 0x09 | 0x18 | 0x19 | 0x22 | 0x24 | 0x26) {
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
        assert!(is_permitted_packet(&step.packet), "{what}: not permitted");
        assert!(
            !(p[0] == 0x21 && p[4] == 0x18 && matches!(p[5], 0x02 | 0x03)),
            "{what}: a table write produced a travel-test packet"
        );
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
            | WriteRequest::DeadBand(_)
            | WriteRequest::AdvancedKey(..)
            | WriteRequest::Socd(_) => {}
        }
    }
    // The v0.2 variants are sampled by `sample_v02`.
    assert_eq!(
        kinds.len(),
        8,
        "every Phase 2 variant is sampled: {kinds:?}"
    );
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
    let mut select_all_open = vec![0x21, 0, 0, 0, 0x18, 0x02];
    select_all_open.extend_from_slice(&SELECTABLE);
    let mut layout_open = vec![0x21, 0, 0, 0, 0x18, 0x02];
    layout_open.extend_from_slice(&LAYOUT);
    let forbidden: [&[u8]; 25] = [
        &[0x14, 0, 0, 0, 0x01, 0x01],                         // reset
        &[0x14, 0x03, 0, 0, 0x01, 0x01],                      // gamepad
        &[0x17, 0, 0, 0, 0x02, 0x01, 0x01],                   // music-rhythm enter
        &[0x08, 0, 0, 0, 0x0E, 0x02],                         // side light
        &[0x09, 0x01, 0, 0, 0x36],                            // custom table 2
        &[0x24, 0x03, 0, 0, 0x01, 0x02],                      // SOCD power value other than 0/1
        &[0x24, 0x03, 0, 0, 0x01, 0x01, 0x01],                // SOCD power with extra bytes
        &[0x24, 0x00, 0, 0, 0x28],                            // SOCD pairs without the power write
        &[0x24, 0x01, 0, 0, 0x28],                            // SOCD pairs read as a write
        &[0x24, 0x02, 0, 0, 0x06, 0x01],                      // SOCD settings read as a write
        &[0x25, 0x01, 0, 0, 0x3A],                            // switch types
        &[0x21, 0, 0, 0, 0x18, 0x06],                         // reset triggers
        &[0x21, 0, 0, 0, 0x18, 0x0F, 0x00],                   // calibration start
        &[0x21, 0, 0, 0, 0x18, 0x10, 0x00],                   // calibration stop
        &[0x21, 0, 0, 0, 0x18, 0x07, 2, 2],                   // single-key calibration
        &[0x21, 0, 0, 0, 0x18, 0x08, 0x00],                   // calibration (other devices)
        &[0x21, 0, 0, 0, 0x18, 0x03, 0x01],                   // travel test close with data
        &select_all_open,                                     // travel test open, knob left out
        &[0x21, 0, 0, 0, 0x00, 0x09, 0x01, 8],                // set poll rate
        &[0x21, 0, 0, 0, 0x18, 0x00],                         // empty-mask trigger write
        &[0x22, 0x00, 0, 0, 0x32, 0x01],                      // DKS without points or rows
        &[0x22, 0x00, 0, 0, 0x32, 0x04, 0, 0x04, 0, 0],       // RS
        &[0x22, 0x00, 0, 0, 0x32, 0x06, 0, 0x64, 0, 1, 0x04], // MDS
        &[0x22, 0x28, 0, 0, 0x32],                            // slot 40
        &[0x07, 0, 0, 0, 0x0E, 0x64],                         // music-rhythm mode
    ];
    for prefix in forbidden {
        assert!(!is_allowed_write(&packet(prefix), None), "{prefix:02x?}");
    }
    // The travel test: only the 82-position layout mask, only alone.
    assert!(is_allowed_write(&packet(&layout_open), None));
    let close = packet(&[0x21, 0, 0, 0, 0x18, 0x03]);
    assert!(is_allowed_write(&close, None));
    assert!(!is_allowed_write(&close, Some(&close)));
    // SOCD: power, chunk 0, chunk 1 in this order.
    let power = packet(&[0x24, 0x03, 0, 0, 0x01, 0x01]);
    let chunk0 = packet(&[0x24, 0x00, 0, 0, 0x28, 0x01, 0x00, 0x04, 0x07]);
    let chunk1 = packet(&[0x24, 0x00, 0, 1, 0x28]);
    assert!(is_allowed_write(&power, None));
    assert!(is_allowed_write(&chunk0, Some(&power)));
    assert!(is_allowed_write(&chunk1, Some(&chunk0)));
    assert!(!is_allowed_write(&chunk1, Some(&power)));
    assert!(!is_allowed_write(&chunk0, Some(&chunk0)));
    assert!(!is_allowed_write(&power, Some(&chunk1)));
    // Chunk 1 with pairs while chunk 0 is not full: a gap.
    let pair1 = packet(&[0x24, 0x00, 0, 1, 0x28, 0x01, 0x00, 0x1A, 0x16]);
    assert!(!is_allowed_write(&pair1, Some(&chunk0)));
    // A pair with the same key twice, a duplicate key, a mode above 3, a gap.
    for entries in [
        &[0x01, 0x00, 0x04, 0x04][..],
        &[0x01, 0x00, 0x04, 0x07, 0x01, 0x00, 0x07, 0x1A],
        &[0x01, 0x04, 0x04, 0x07],
        &[0, 0, 0, 0, 0x01, 0x00, 0x04, 0x07],
        &[0x07, 0xFF, 0xFF, 0xFF],
    ] {
        let mut bytes = vec![0x24, 0x00, 0, 0, 0x28];
        bytes.extend_from_slice(entries);
        assert!(
            !is_allowed_write(&packet(&bytes), Some(&power)),
            "{entries:02x?}"
        );
    }
    // The c16 DKS, MT and TGL slots; a DKS with r1 above r2, a stray byte
    // after the slot.
    let dks = [
        0x22, 0x00, 0, 0, 0x32, 0x01, 0x0f, 0x1e, 0x1e, 0x0f, 0x00, 0x08, 0x00, 0x00, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x15, 0x00, 0x00, 0x00, 0x01,
    ];
    assert!(is_allowed_write(&packet(&dks), None));
    assert!(is_allowed_write(
        &packet(&[
            0x22, 1, 0, 0, 0x32, 0x02, 0, 0xe0, 0, 0, 0, 0x0a, 0, 0, 0x1e
        ]),
        None
    ));
    assert!(is_allowed_write(
        &packet(&[0x22, 2, 0, 0, 0x32, 0x03, 0, 0x0b]),
        None
    ));
    let mut deep = dks;
    deep[6] = 0x1f;
    deep[9] = 0x1f;
    assert!(!is_allowed_write(&packet(&deep), None));
    let mut tail = packet(&dks);
    tail[45] = 0x01;
    assert!(!is_allowed_write(&tail, None));
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
        &[0x24, 0, 0, 0, 0],
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
        &[0x24, 0x02, 0, 0, 0x06, 0, 0, 0, 0, 0xfa],  // SOCD settings / push
        &[0x24, 0x01, 0, 0, 0x28],                    // SOCD pairs chunk
        &[0x21, 0, 0, 0, 0x05, 0x01, 0, 0, 0x10],     // travel stream
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

// ---------------------------------------------------------------------------
// v0.2 writes: advanced-key slots (W10), SOCD (W11), keymap assignments,
// macro programs, key colours and the travel test (T1, T2).
// ---------------------------------------------------------------------------

fn w75_advanced_limits() -> AdvancedLimits {
    let global = TriggerGlobal::from_response(&single(
        ReadRequest::TriggerGlobal,
        0x21,
        0,
        &[0x04, 0xaa, 0x02, 0x02, 0x00, 0x05],
    ))
    .unwrap();
    AdvancedLimits::new(&W75, &global, 20).unwrap()
}

fn hid(u: u8) -> HidUsage {
    HidUsage::new(u).unwrap()
}

/// c16 slot 0: DKS on E, E tap at stage 1, R tap at stage 2, 1.5 mm.
fn c16_dks() -> AdvancedKeyDef {
    let row = |key, steps: [DksStep; 4]| {
        Some(DksRow {
            key: hid(key),
            steps,
        })
    };
    let (n, t) = (DksStep::None, DksStep::Tap);
    AdvancedKeyDef::Dks(Dks {
        first_point: 15,
        rows: [row(0x08, [t, n, n, n]), row(0x15, [n, t, n, n]), None, None],
    })
}

/// c16 slot 1: Mod-Tap, hold L-Ctrl, tap G, 300 ms.
fn c16_mt() -> AdvancedKeyDef {
    AdvancedKeyDef::ModTap(ModTap {
        hold: hid(0xE0),
        tap: hid(0x0A),
        threshold_10ms: 30,
    })
}

/// c16 slot 2: Toggle H.
fn c16_tgl() -> AdvancedKeyDef {
    AdvancedKeyDef::Toggle(Toggle { key: hid(0x0B) })
}

fn pick<T: Copy>(rng: &mut Rng, items: &[T]) -> T {
    items[usize::try_from(rng.next() % u64::try_from(items.len()).unwrap()).unwrap()]
}

fn random_dks_steps(rng: &mut Rng) -> [DksStep; 4] {
    let s0 = rng.below(3);
    let s1 = if s0 == 2 { rng.below(4) } else { rng.below(3) };
    let s2 = if s1 >= 2 { rng.below(4) } else { rng.below(3) };
    let s3 = rng.below(2);
    let mut steps = [s0, s1, s2, s3];
    if steps == [0; 4] {
        steps[0] = 1;
    }
    steps.map(|s| DksStep::from_byte(s).unwrap())
}

fn random_def(rng: &mut Rng, usages: &[HidUsage]) -> AdvancedKeyDef {
    match rng.below(3) {
        0 => {
            let mut rows = [None; 4];
            for row in rows.iter_mut() {
                if rng.below(2) == 0 {
                    *row = Some(DksRow {
                        key: pick(rng, usages),
                        steps: random_dks_steps(rng),
                    });
                }
            }
            if rows.iter().all(Option::is_none) {
                rows[usize::from(rng.below(4))] = Some(DksRow {
                    key: pick(rng, usages),
                    steps: random_dks_steps(rng),
                });
            }
            AdvancedKeyDef::Dks(Dks {
                first_point: 1 + rng.below(30),
                rows,
            })
        }
        1 => AdvancedKeyDef::ModTap(ModTap {
            hold: pick(rng, usages),
            tap: pick(rng, usages),
            threshold_10ms: 15 + rng.below(86),
        }),
        _ => AdvancedKeyDef::Toggle(Toggle {
            key: pick(rng, usages),
        }),
    }
}

fn random_assignment(rng: &mut Rng, usages: &[HidUsage]) -> KeyAssignment {
    let modifiers: Vec<HidUsage> = usages.iter().copied().filter(|u| u.is_modifier()).collect();
    let keys: Vec<HidUsage> = usages
        .iter()
        .copied()
        .filter(|u| !u.is_modifier())
        .collect();
    let mouse = [
        MouseAction::Button(MouseButton::Left),
        MouseAction::Button(MouseButton::Right),
        MouseAction::Button(MouseButton::Middle),
        MouseAction::Button(MouseButton::Back),
        MouseAction::Button(MouseButton::Forward),
        MouseAction::WheelUp,
        MouseAction::WheelDown,
    ];
    match rng.below(7) {
        0 => KeyAssignment::Default,
        1 => KeyAssignment::Key(pick(rng, usages)),
        2 => KeyAssignment::Media(MediaKey::new(pick(rng, &MediaKey::ALL)).unwrap()),
        3 => KeyAssignment::Mouse(pick(rng, &mouse)),
        4 => {
            let first = pick(rng, &modifiers);
            let key = pick(rng, &keys);
            let combo = if rng.below(2) == 0 {
                Combo::new(&[first], key)
            } else {
                let others: Vec<HidUsage> =
                    modifiers.iter().copied().filter(|m| *m != first).collect();
                Combo::new(&[first, pick(rng, &others)], key)
            };
            KeyAssignment::Combo(combo.unwrap())
        }
        5 => KeyAssignment::Macro(MacroSlotId::new(rng.below(10)).unwrap()),
        _ => KeyAssignment::Advanced(AdvancedSlotId::new(rng.below(40)).unwrap()),
    }
}

fn random_program(rng: &mut Rng, usages: &[HidUsage]) -> MacroProgram {
    let pairs = 1 + usize::from(rng.below(20));
    let mut steps = Vec::new();
    for k in 0..pairs {
        let usage = pick(rng, usages);
        let delay = u16::from_le_bytes([rng.byte(), rng.byte()]);
        steps.push(MacroStep::press(usage, delay));
        let last = if k + 1 == pairs { 0 } else { delay };
        steps.push(MacroStep::release(usage, last));
    }
    let play = match rng.below(4) {
        0 => MacroPlay::Once,
        1 => MacroPlay::Times(1 + rng.below(255)),
        2 => MacroPlay::Toggle,
        _ => MacroPlay::WhileHeld,
    };
    MacroProgram::new(play, steps).unwrap()
}

/// `n` SOCD pairs of distinct W75 keys with random modes.
fn random_socd(rng: &mut Rng, n: usize) -> SocdPairsData {
    let mut keys: Vec<u8> = W75.keys.iter().filter_map(|k| k.hid_usage).collect();
    // Fisher-Yates with the test generator.
    for i in (1..keys.len()).rev() {
        let j = usize::try_from(rng.next() % u64::try_from(i + 1).unwrap()).unwrap();
        keys.swap(i, j);
    }
    let pairs: Vec<SocdPair> = (0..n)
        .map(|i| SocdPair {
            mode: pick(rng, &SocdMode::ALL),
            key1: hid(keys[2 * i]),
            key2: hid(keys[2 * i + 1]),
        })
        .collect();
    SocdPairsData::from_pairs(&W75, &pairs).unwrap()
}

/// Positions of the W75 keys that have a HID usage (every key but the knob
/// and Fn).
fn usage_keys() -> Vec<KeyPos> {
    W75.keys
        .iter()
        .filter(|k| k.hid_usage.is_some())
        .filter_map(|k| k.pos())
        .collect()
}

/// A bounded sample of every kind of v0.2 request.
fn sample_v02() -> Vec<WriteRequest> {
    let mut rng = Rng(0x5632_5352_4F54_4F31);
    let usages: Vec<HidUsage> = HidUsage::all().collect();
    let limits = w75_advanced_limits();
    let mut out = Vec::new();

    // W10: the captured DKS, MT and TGL in every slot, plus 3 random ones.
    for slot in AdvancedSlotId::all() {
        let mut defs = vec![c16_dks(), c16_mt(), c16_tgl()];
        defs.extend((0..3).map(|_| random_def(&mut rng, &usages)));
        for def in defs {
            let data = AdvancedSlotData::from_def(&def, &limits).unwrap();
            out.push(WriteRequest::AdvancedKey(slot, data));
        }
    }

    // W11: both powers x 0-20 pairs (chunk boundaries), plus random tables.
    for power in [false, true] {
        for n in [0, 1, 2, 9, 10, 11, 19, 20] {
            let pairs = random_socd(&mut rng, n);
            out.push(WriteRequest::Socd(SocdWrite::new(power, pairs)));
        }
    }
    for _ in 0..10 {
        let n = usize::from(rng.below(21));
        let power = rng.below(2) == 1;
        let pairs = random_socd(&mut rng, n);
        out.push(WriteRequest::Socd(SocdWrite::new(power, pairs)));
    }

    // W5: assignments of every kind on random keys of both layers.
    let positions = usage_keys();
    for layer in Layer::ALL {
        for _ in 0..20 {
            let mut data = keymap(layer, &[0u8; 528]);
            for _ in 0..8 {
                let pos = pick(&mut rng, &positions);
                let assignment = random_assignment(&mut rng, &usages);
                data.set_entry(pos, assignment.entry(&W75, pos).unwrap());
            }
            out.push(WriteRequest::Keymap(layer, data));
        }
    }

    // W6: random programs in every slot.
    for slot in MacroSlotId::all() {
        for _ in 0..5 {
            let program = random_program(&mut rng, &usages);
            out.push(WriteRequest::Macro(slot, program.encode(slot)));
        }
    }

    // W4: random key colours on a table with a lit non-layout slot.
    for _ in 0..3 {
        let mut table: Vec<u8> = (0..396).map(|_| rng.byte()).collect();
        table[103 * 3] = 0xff;
        let mut data = colors(&table);
        let mut paint: Vec<(KeyPos, Rgb)> = Vec::new();
        for pos in &positions {
            if rng.below(2) == 0 {
                paint.push((*pos, Rgb::new(rng.byte(), rng.byte(), rng.byte())));
            }
        }
        data.set_key_colors(&W75, &paint).unwrap();
        assert_eq!(data.as_bytes()[103 * 3], 0xff);
        out.push(WriteRequest::CustomColors1(data));
    }
    out
}

#[test]
fn every_sampled_v02_request_produces_only_allowed_writes() {
    let mut packets = 0;
    let mut kinds = std::collections::BTreeMap::new();
    for request in sample_v02() {
        packets += check_request(&request, None);
        for step in request.steps() {
            assert_eq!(
                ReadRequest::from_packet(step.packet.as_bytes()),
                None,
                "{request}"
            );
        }
        *kinds.entry(request.kind()).or_insert(0) += 1;
    }
    assert_eq!(kinds["advanced_key"], 240);
    assert_eq!(kinds["socd"], 26);
    assert_eq!(kinds["keymap"], 40);
    assert_eq!(kinds["macro"], 50);
    assert_eq!(kinds["custom_colors1"], 3);
    // 240 + 26 x 3 + 40 x 10 + 50 x 5 + 3 x 8.
    assert_eq!(packets, 992);
    // Together with the Phase 2 sample, every variant is sampled.
    let mut all: std::collections::BTreeSet<&str> = kinds.keys().copied().collect();
    all.extend(sample().iter().map(|(r, _)| r.kind()));
    assert_eq!(all.len(), 10, "{all:?}");
}

#[test]
fn travel_test_requests() {
    let open = TravelTestRequest::Open(TravelTestMask::full_layout(&W75).unwrap());
    for request in [open, TravelTestRequest::Close] {
        let step = request.step();
        let p = step.packet.as_bytes();
        let what = request.to_string();
        assert!(is_allowed_write(p, None), "{what}");
        assert_not_forbidden(p, &what);
        assert!(is_permitted_packet(&step.packet), "{what}");
        assert_eq!(ReadRequest::from_packet(p), None, "{what}");
        assert_eq!(
            step.ack,
            AckSpec::Report {
                header: [0x21, 0, 0, 0, 0x01],
                selector: Some(0x00),
                required: true
            }
        );
        assert!(step.gap_after.is_zero());
    }
    let mut expected = [0u8; 63];
    expected[..6].copy_from_slice(&[0x21, 0, 0, 0, 0x18, 0x02]);
    expected[6..28].copy_from_slice(&LAYOUT);
    assert_eq!(*open.step().packet.as_bytes(), expected);
}

#[test]
fn assignments_only_produce_confirmed_categories() {
    let mut rng = Rng(0x4B45_594D_4150_3032);
    let usages: Vec<HidUsage> = HidUsage::all().collect();
    let positions = usage_keys();
    for _ in 0..5000 {
        let pos = pick(&mut rng, &positions);
        let assignment = random_assignment(&mut rng, &usages);
        let entry = assignment.entry(&W75, pos).unwrap();
        assert!(
            matches!(entry[0], 0x00 | 0x07 | 0x10 | 0x12 | 0x13 | 0x17),
            "{assignment:?}: {entry:02x?}"
        );
        assert_eq!(
            KeyAssignment::classify(&W75, pos, entry),
            EntryKind::Assignable(assignment)
        );
        // Arbitrary bytes are classified without panicking, and an
        // assignable entry is exactly what its assignment writes.
        let bytes = [rng.byte(), rng.byte(), rng.byte(), rng.byte()];
        if let EntryKind::Assignable(a) = KeyAssignment::classify(&W75, pos, bytes) {
            assert_eq!(a.entry(&W75, pos), Ok(bytes));
        }
    }
}

#[test]
fn the_egress_allowlist_refuses_other_headers() {
    let packet = |prefix: &[u8]| {
        let header: [u8; 5] = prefix[..5].try_into().unwrap();
        WritePacket::new(header, &prefix[5..])
    };
    for prefix in [
        &[0x14, 0, 0, 0, 0x01, 0x01][..],   // reset
        &[0x14, 0x03, 0, 0, 0x01, 0x01],    // gamepad
        &[0x08, 0, 0, 0, 0x0E, 0x02],       // side light
        &[0x09, 0x01, 0, 0, 0x36],          // custom table 2
        &[0x24, 0x01, 0, 0, 0x28],          // SOCD pairs read
        &[0x24, 0x02, 0, 0, 0x06],          // SOCD settings read
        &[0x24, 0x00, 0, 0x02, 0x28],       // SOCD chunk 2
        &[0x24, 0x00, 0, 0, 0x27],          // SOCD chunk of another length
        &[0x24, 0x03, 0, 0, 0x02],          // SOCD power of another length
        &[0x24, 0x03, 0, 0, 0x01, 0x02],    // SOCD power value 2
        &[0x25, 0x01, 0, 0, 0x3A],          // switch types
        &[0x21, 0, 0, 0, 0x18, 0x06],       // reset triggers
        &[0x21, 0, 0, 0, 0x18, 0x07, 2, 2], // single-key calibration
        &[0x21, 0, 0, 0, 0x18, 0x08],       // calibration
        &[0x21, 0, 0, 0, 0x18, 0x0F],       // calibration start
        &[0x21, 0, 0, 0, 0x18, 0x10],       // calibration stop
        &[0x21, 0, 0, 0, 0x00, 0x09, 1, 8], // set poll rate
        &[0x21, 0, 0, 1, 0x18, 0x00],       // byte 3 set
        &[0x17, 0, 0, 0, 0x02, 0x01, 0x01], // music-rhythm enter
        &[0x07, 0, 0, 0, 0x0E, 0x64],       // music-rhythm mode
        &[0x18, 0x01, 0, 0, 0x38],          // keymap sub 01
        &[0x19, 0x0A, 0, 0, 0x3A],          // macro slot 10
        &[0x22, 0x28, 0, 0, 0x32],          // advanced slot 40
        &[0x22, 0x00, 0, 0, 0x28],          // advanced write of another length
        &[0x22, 0x00, 0, 0, 0x32, 0x04],    // advanced key type 4
        &[0x26, 0x02, 0, 0, 0x3A],          // dead band sub 02
        &[0x0D, 0, 0, 0, 0],                // anything else
    ] {
        assert!(!is_permitted_packet(&packet(prefix)), "{prefix:02x?}");
    }
    // No read request passes the allowlist either.
    for read in ReadRequest::all() {
        assert!(
            !is_permitted_packet(&packet(read.packet().as_bytes())),
            "{read}"
        );
    }
}

#[test]
fn v02_steps_acks_and_gaps() {
    let report_ack = |h: [u8; 5]| AckSpec::Report {
        header: h,
        selector: None,
        required: true,
    };
    let limits = w75_advanced_limits();
    let slot = AdvancedSlotId::new(39).unwrap();
    let w10 = WriteRequest::AdvancedKey(
        slot,
        AdvancedSlotData::from_def(&c16_tgl(), &limits).unwrap(),
    );
    let steps = w10.steps();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].packet.summary(), "22 27 00 00 32 03 00 0b");
    assert_eq!(steps[0].ack, report_ack([0x22, 0, 0, 0, 0]));
    assert!(steps[0].gap_after.is_zero());

    let ad = SocdPair {
        mode: SocdMode::M1,
        key1: hid(0x04),
        key2: hid(0x07),
    };
    let pairs = SocdPairsData::from_pairs(&W75, &[ad]).unwrap();
    let steps = WriteRequest::Socd(SocdWrite::new(true, pairs)).steps();
    let summaries: Vec<String> = steps.iter().map(|s| s.packet.summary()).collect();
    assert_eq!(
        summaries,
        [
            "24 03 00 00 01 01",
            "24 00 00 00 28 01 00 04 07",
            "24 00 00 01 28"
        ]
    );
    let possible = AckSpec::Possible {
        header: [0x24, 0, 0, 0, 0],
    };
    assert_eq!(steps[0].ack, possible);
    assert!(!possible.is_required());
    assert!(!possible.expects_reply());
    assert_eq!(steps[0].gap_after, SOCD_POWER_SETTLE);
    for step in &steps[1..] {
        assert_eq!(step.ack, report_ack([0x24, 0, 0, 0, 0]));
        assert!(step.gap_after.is_zero());
    }
    // A possible ack is complete without a reply and takes one matching
    // report if it comes.
    let mut tracker = possible.tracker();
    assert!(tracker.is_complete());
    assert_eq!(
        tracker.offer(&report(&[0x24, 0x01, 0, 0, 0x28])),
        AckOffer::Ignored
    );
    assert!(!tracker.ack_received());
    assert_eq!(tracker.offer(&report(&[0x24, 0, 0, 0, 0])), AckOffer::Ack);
    assert!(tracker.ack_received());
    assert_eq!(
        tracker.offer(&report(&[0x24, 0, 0, 0, 0])),
        AckOffer::Ignored
    );
}

#[test]
fn check_advanced_slot_and_socd() {
    let limits = w75_advanced_limits();
    let slot = AdvancedSlotId::new(5).unwrap();
    let data = AdvancedSlotData::from_def(&c16_mt(), &limits).unwrap();
    let request = WriteRequest::AdvancedKey(slot, data.clone());
    assert_eq!(request.verify_reads(), vec![ReadRequest::AdvancedKeys]);
    let mut bytes = vec![0u8; 1600];
    bytes[200..240].copy_from_slice(data.as_bytes());
    // Another slot may hold anything.
    bytes[6 * 40] = 0x03;
    assert_eq!(
        request.check(&[table(ReadRequest::AdvancedKeys, &bytes)]),
        Ok(())
    );
    bytes[200 + 9] = 31;
    let err = request
        .check(&[table(ReadRequest::AdvancedKeys, &bytes)])
        .unwrap_err();
    assert_eq!(err.table, TableId::AdvancedKey { slot });
    assert_eq!(err.differing_offsets(), vec![9]);
    // A clear of the same slot does not match the content.
    bytes[200 + 9] = 30;
    assert!(
        WriteRequest::AdvancedKeyClear(slot)
            .check(&[table(ReadRequest::AdvancedKeys, &bytes)])
            .is_err()
    );

    let ad = SocdPair {
        mode: SocdMode::M2,
        key1: hid(0x04),
        key2: hid(0x07),
    };
    let pairs = SocdPairsData::from_pairs(&W75, &[ad]).unwrap();
    let request = WriteRequest::Socd(SocdWrite::new(true, pairs.clone()));
    assert_eq!(
        request.verify_reads(),
        vec![ReadRequest::SocdSettings, ReadRequest::SocdPairs]
    );
    let settings = |payload: &[u8]| single(ReadRequest::SocdSettings, 0x24, 0x02, payload);
    let on = settings(&[1, 0, 0, 0, 0xfa, 0xe5]);
    let pair_reply = table(ReadRequest::SocdPairs, pairs.as_bytes());
    assert_eq!(request.check(&[on.clone(), pair_reply.clone()]), Ok(()));
    // Bytes 1-5 (unknown, hotkey) are not compared.
    assert_eq!(
        request.check(&[settings(&[1, 9, 9, 9, 0, 0]), pair_reply.clone()]),
        Ok(())
    );
    let err = request
        .check(&[settings(&[0, 0, 0, 0, 0xfa, 0xe5]), pair_reply.clone()])
        .unwrap_err();
    assert_eq!(err.read, ReadRequest::SocdSettings);
    assert_eq!(err.table, TableId::Socd);
    assert_eq!(
        (err.expected.0.clone(), err.got.0.clone()),
        (vec![1], vec![0])
    );
    let err = request
        .check(&[on.clone(), table(ReadRequest::SocdPairs, &[0u8; 80])])
        .unwrap_err();
    assert_eq!(err.read, ReadRequest::SocdPairs);
    assert_eq!(err.differing_offsets(), vec![0, 1, 2, 3]);
    assert_eq!(
        request.check(std::slice::from_ref(&on)).unwrap_err().kind,
        MismatchKind::MissingReply
    );
    assert_eq!(
        request
            .check(&[settings(&[]), pair_reply])
            .unwrap_err()
            .kind,
        MismatchKind::InvalidReply
    );
    // The compared bytes are available to callers.
    assert_eq!(request.expected_bytes(ReadRequest::SocdSettings), vec![1]);
    assert_eq!(request.observed_bytes(&on), Some(vec![1]));
    assert_eq!(
        request.expected_bytes(ReadRequest::DeadBand),
        Vec::<u8>::new()
    );
}

#[test]
fn v02_tables_and_names() {
    let limits = w75_advanced_limits();
    let slot = AdvancedSlotId::new(3).unwrap();
    let w10 = WriteRequest::AdvancedKey(
        slot,
        AdvancedSlotData::from_def(&c16_dks(), &limits).unwrap(),
    );
    assert_eq!(w10.table(), TableId::AdvancedKey { slot });
    assert_eq!(w10.kind(), "advanced_key");
    assert_eq!(w10.to_string(), "advanced-key slot 3 (22 03)");
    for (power, text) in [(true, "on"), (false, "off")] {
        let w11 = WriteRequest::Socd(SocdWrite::new(
            power,
            SocdPairsData::from_pairs(&W75, &[]).unwrap(),
        ));
        assert_eq!(w11.table(), TableId::Socd);
        assert_eq!(w11.kind(), "socd");
        assert_eq!(
            w11.to_string(),
            format!("SOCD power {text} and pairs (24 03, 24 00)")
        );
    }
    assert_eq!(TableId::Socd.to_string(), "SOCD");
}

#[cfg(feature = "serde")]
#[test]
fn v02_serialization() {
    let pairs = SocdPairsData::from_pairs(&W75, &[]).unwrap();
    let request = WriteRequest::Socd(SocdWrite::new(true, pairs));
    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["socd"]["power"], true);
    assert_eq!(json["socd"]["pairs"].as_str().map(str::len), Some(160));
    let steps = serde_json::to_value(request.steps()).unwrap();
    assert_eq!(
        steps[0]["ack"],
        serde_json::json!({"possible": {"header": [36, 0, 0, 0, 0]}})
    );
    assert_eq!(steps[0]["gap_after_ms"], 50);
    let json = serde_json::to_string(&TableId::Socd).unwrap();
    assert_eq!(json, r#""socd""#);
    assert_eq!(
        serde_json::from_str::<TableId>(&json).unwrap(),
        TableId::Socd
    );
    let open = TravelTestRequest::Open(TravelTestMask::full_layout(&W75).unwrap());
    assert!(serde_json::to_value(open).unwrap()["open"].is_array());
    assert_eq!(
        serde_json::to_string(&TravelTestRequest::Close).unwrap(),
        r#""close""#
    );
}
