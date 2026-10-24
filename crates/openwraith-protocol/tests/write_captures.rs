//! Byte-identical checks of the write builders against the writes the vendor
//! site sent in the USB captures (c02, c03, c08-c13), and of the ack and
//! read-back logic against the keyboard's real replies.
//!
//! Where the site's write is lossy (it forces knob defaults into the keymap
//! and zeroes colour slots outside its layout), the tests document the
//! difference instead of reproducing it.

mod common;

use common::{exchanges_in, first, load};
use openwraith_protocol::capture::{Capture, CaptureEvent};
use openwraith_protocol::write::{
    AckOffer, AckSpec, AdvancedSlotId, ColorTableData, DeadBandData, DeadBandWrite, KeymapData,
    LightLimits, LightRecord, MacroData, MainLightWrite, ProfileId, TriggerLimits, TriggerSetting,
    TriggerWrite, TriggerWriteMode, WriteRequest, WriteStep, is_write_ack,
};
use openwraith_protocol::{
    CustomTable, Distance, KeyMask, KeyPos, Layer, LightMode, LightModeList, MacroSlotId,
    RawResponse, ReadRequest, Report, Rgb, SubMatch, TriggerGlobal, W75,
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

/// Milliseconds from the first entry of the capture to entry `index`.
fn offset_ms(c: &Capture, index: usize) -> u64 {
    c.entries[index].t - c.start_time()
}

/// The first `count` `OUT` packets at or after entry `start` that satisfy `want`.
fn outs_from(
    c: &Capture,
    start: usize,
    count: usize,
    want: impl Fn(&[u8; 63]) -> bool,
) -> Vec<(usize, [u8; 63])> {
    let found: Vec<(usize, [u8; 63])> = c
        .out_packets()
        .filter(|(i, bytes)| *i >= start && want(bytes))
        .take(count)
        .map(|(i, bytes)| (i, *bytes))
        .collect();
    assert_eq!(found.len(), count, "OUT packets from entry {start}");
    found
}

/// The last `count` `OUT` packets up to and including entry `last` that
/// satisfy `want` (a table write whose final chunk is entry `last`).
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

/// The packets of a request.
fn packets(request: &WriteRequest) -> Vec<[u8; 63]> {
    request
        .steps()
        .iter()
        .map(|step| *step.packet.as_bytes())
        .collect()
}

/// Asserts that `request` sends exactly the captured packets.
fn assert_same_packets(request: &WriteRequest, captured: &[(usize, [u8; 63])]) {
    let ours = packets(request);
    assert_eq!(ours.len(), captured.len(), "{request}: packet count");
    for (k, (ours, (index, theirs))) in ours.iter().zip(captured).enumerate() {
        assert_eq!(
            ours[..],
            theirs[..],
            "{request}: packet {k} differs from capture entry #{index}"
        );
    }
}

/// The reply the keyboard gives when `table` is read with `request`,
/// chunked as the spec describes (section 2.5).
fn reply(request: ReadRequest, table: &[u8]) -> RawResponse {
    let spec = request.response_spec();
    let sub = match spec.sub {
        SubMatch::Exact(sub) => sub,
        SubMatch::Any | SubMatch::Below(_) => 0,
    };
    let layout = request.table_layout().expect("a table read");
    assert_eq!(table.len(), layout.total_len);
    let reports = (0..layout.chunks).map(|index| {
        let start = usize::from(index) * layout.chunk_len;
        let len = layout.expected_len(index);
        let mut bytes = [0u8; 63];
        let [hi, lo] = index.to_be_bytes();
        bytes[..5].copy_from_slice(&[spec.cmd, sub, hi, lo, u8::try_from(len).unwrap()]);
        bytes[5..5 + len].copy_from_slice(&table[start..start + len]);
        Report::new(bytes)
    });
    RawResponse::from_reports(request, reports).unwrap()
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
/// packet) to the step's ack tracker and asserts it completes.
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
        tracker.is_complete(),
        "no ack for capture entry #{index} ({:?})",
        step.ack
    );
}

/// Number of `IN` reports in the capture that `spec` accepts as its ack.
fn count_acks(c: &Capture, spec: AckSpec) -> usize {
    c.in_reports()
        .filter(|(_, report)| spec.tracker().offer(report) == AckOffer::Ack)
        .count()
}

/// Number of `OUT` packets that satisfy `want`.
fn count_outs(c: &Capture, want: impl Fn(&[u8; 63]) -> bool) -> usize {
    c.out_packets().filter(|(_, bytes)| want(bytes)).count()
}

fn pos(row: u8, col: u8) -> KeyPos {
    KeyPos::new(row, col).unwrap()
}

fn trigger_write(
    c: &Capture,
    key: KeyPos,
    mode: TriggerWriteMode,
    values: [u16; 3],
) -> WriteRequest {
    let limits = TriggerLimits::new(&W75, &first::<TriggerGlobal>(c, ReadRequest::TriggerGlobal));
    let setting = TriggerSetting {
        mode,
        actuation: Distance(values[0]),
        rt_press: Distance(values[1]),
        rt_release: Distance(values[2]),
    };
    let keys = KeyMask::from_positions([key]);
    WriteRequest::Trigger(TriggerWrite::new(&W75, keys, setting, &limits).unwrap())
}

const W: KeyPos = match KeyPos::new(2, 2) {
    Some(pos) => pos,
    None => panic!("W is inside the matrix"),
};

// ---------------------------------------------------------------------------
// W8: trigger writes.
// ---------------------------------------------------------------------------

#[test]
fn c03_w_trigger_writes() {
    let c = load("capture_03_actuation_W");
    // Mode 00, actuation stepped 1.06 -> 1.00 mm, sensitivity kept at 0.02.
    for (index, travel) in [(880, 0x35), (882, 0x34), (884, 0x33), (888, 0x32)] {
        let request = trigger_write(&c, W, TriggerWriteMode::Off, [travel, 1, 1]);
        assert_same_packets(&request, &[(index, out(&c, index))]);
        assert_acked(&c, index, &request.steps()[0]);
    }
    // Final RT write: 1.00 mm, RT 0.30 mm (spec 4.11.3 byte by byte).
    assert_eq!(offset_ms(&c, 934), 48277);
    let request = trigger_write(&c, W, TriggerWriteMode::RapidTrigger, [0x32, 0x0f, 0x0f]);
    assert_same_packets(&request, &[(934, out(&c, 934))]);
    assert_acked(&c, 934, &request.steps()[0]);

    // The persisted value (c05) is exactly what this request verifies.
    let c05 = load("capture_05_reconnect_W_persisted");
    let read = reply_in(&c05, 0..usize::MAX, ReadRequest::KeyTrigger(W));
    assert_eq!(request.check(std::slice::from_ref(&read)), Ok(()));
    let (key, setting) = TriggerSetting::from_response(&read).unwrap();
    assert_eq!(key, W);
    let WriteRequest::Trigger(write) = &request else {
        unreachable!()
    };
    assert_eq!(&setting, write.setting());
}

#[test]
fn c10_q_and_w_trigger_writes_and_read_back() {
    let c = load("capture_10_triggers_Q_W_profile_reset");
    let q = pos(2, 1);
    // Q (row 2, col 1): mask byte 7 = 0x04, 1.50 mm.
    let q_write = trigger_write(&c, q, TriggerWriteMode::Off, [0x4b, 1, 1]);
    assert_same_packets(&q_write, &[(222, out(&c, 222))]);
    assert_eq!(out(&c, 222)[7], 0x04);
    assert_acked(&c, 222, &q_write.steps()[0]);
    // W: separate press 0.30 / release 0.50 mm.
    let w_write = trigger_write(
        &c,
        W,
        TriggerWriteMode::RapidTriggerSeparate,
        [0x55, 0x0f, 0x19],
    );
    assert_same_packets(&w_write, &[(282, out(&c, 282))]);
    assert_acked(&c, 282, &w_write.steps()[0]);

    // The site's re-read after the profile round trip confirms both writes.
    let q_read = reply_in(&c, 800..usize::MAX, ReadRequest::KeyTrigger(q));
    let w_read = reply_in(&c, 800..usize::MAX, ReadRequest::KeyTrigger(W));
    assert_eq!(q_write.check(&[q_read.clone(), w_read.clone()]), Ok(()));
    assert_eq!(w_write.check(&[q_read, w_read.clone()]), Ok(()));
    let (_, w_setting) = TriggerSetting::from_response(&w_read).unwrap();
    assert_eq!(
        w_setting.read_bytes(),
        [0x0d, 0x55, 0x55, 0x0f, 0x19, 0, 0, 0, 0]
    );
    // Profile 2 kept the factory values: there the check fails.
    let before = reply_in(&c, 300..799, ReadRequest::KeyTrigger(W));
    assert!(w_write.check(&[before]).is_err());
}

#[test]
fn trigger_acks_in_every_capture() {
    let spec = trigger_write(
        &load("capture_03_actuation_W"),
        W,
        TriggerWriteMode::Off,
        [0x55, 1, 1],
    )
    .steps()[0]
        .ack;
    for name in [
        "capture_03_actuation_W",
        "capture_10_triggers_Q_W_profile_reset",
    ] {
        let c = load(name);
        // Every 0x21 write with byte 4 = 0x18 that is not a read is acked
        // with `21 00 00 00 01 00`.
        let writes = count_outs(&c, |b| {
            b[0] == 0x21 && b[4] == 0x18 && !matches!(b[5], 0x04 | 0x05)
        });
        assert_eq!(count_acks(&c, spec), writes, "{name}");
    }
}

// ---------------------------------------------------------------------------
// W1: profile select.
// ---------------------------------------------------------------------------

#[test]
fn c08_and_c10_profile_selects() {
    let cases = [
        ("capture_08_profile_switch", 830, 2),
        ("capture_08_profile_switch", 1317, 1),
        ("capture_10_triggers_Q_W_profile_reset", 300, 2),
        ("capture_10_triggers_Q_W_profile_reset", 799, 1),
    ];
    for (name, index, profile) in cases {
        let c = load(name);
        let request = WriteRequest::SelectProfile(ProfileId::new(profile).unwrap());
        assert_same_packets(&request, &[(index, out(&c, index))]);
        let step = &request.steps()[0];
        assert_eq!(
            step.ack,
            AckSpec::ProfileSelected(ProfileId::new(profile).unwrap())
        );
        assert_acked(&c, index, step);
        // The push names the new profile; a tracker for another profile
        // takes the ack but never completes.
        let other = ProfileId::new(if profile == 1 { 2 } else { 1 }).unwrap();
        let mut tracker = AckSpec::ProfileSelected(other).tracker();
        for (_, report) in c.in_reports().filter(|(i, _)| *i > index).take(2) {
            tracker.offer(report);
        }
        assert!(tracker.ack_received(), "{name}#{index}");
        assert!(!tracker.is_complete(), "{name}#{index}");
    }
    assert_eq!(offset_ms(&load("capture_08_profile_switch"), 830), 10913);
}

// ---------------------------------------------------------------------------
// W2 + W3: main light.
// ---------------------------------------------------------------------------

fn w75_light_limits(c: &Capture) -> LightLimits {
    LightLimits::new(
        &W75,
        &first::<LightModeList>(c, ReadRequest::MainLightModes),
    )
}

#[test]
fn c02_main_light_writes() {
    let c01 = load("capture_01_connect");
    let c = load("capture_02_light");
    let limits = w75_light_limits(&c01);
    let current =
        LightRecord::from_response(&reply_in(&c01, 0..usize::MAX, ReadRequest::MainLight)).unwrap();
    assert_eq!(current.mode(), LightMode::Wave);

    // Wave -> Custom red: the leave packet, then the record (#1087, #1088).
    let custom = current
        .with_mode(LightMode::Custom)
        .with_foreground(Rgb::new(0xff, 0, 0));
    let request = WriteRequest::MainLight(MainLightWrite::new(&current, custom, &limits).unwrap());
    assert_same_packets(&request, &[(1087, out(&c, 1087)), (1088, out(&c, 1088))]);
    assert_eq!(offset_ms(&c, 1087), 137930);
    // The leave is acked 13 ms later (after the light write, which has no reply).
    assert_acked(&c, 1088, &request.steps()[0]);

    // Brightness only: no leave packet (#1119), then back to 4 (#1122).
    let dark = custom.with_brightness(0);
    let request = WriteRequest::MainLight(MainLightWrite::new(&custom, dark, &limits).unwrap());
    assert_same_packets(&request, &[(1119, out(&c, 1119))]);
    let request = WriteRequest::MainLight(MainLightWrite::new(&dark, custom, &limits).unwrap());
    assert_same_packets(&request, &[(1122, out(&c, 1122))]);

    // The next page session (c03) reads back what c02 wrote.
    let c03 = load("capture_03_actuation_W");
    assert_eq!(
        request.check(&[reply_in(&c03, 0..usize::MAX, ReadRequest::MainLight)]),
        Ok(())
    );
    assert_eq!(
        LightRecord::from_response(&reply_in(&c03, 0..usize::MAX, ReadRequest::MainLight)).unwrap(),
        custom
    );
}

// ---------------------------------------------------------------------------
// W4: custom colours 1.
// ---------------------------------------------------------------------------

#[test]
fn c02_colour_burst() {
    let c = load("capture_02_light");
    let burst = outs_from(&c, 1137, 8, |b| b[..2] == [0x09, 0x00]);
    assert_eq!(offset_ms(&c, 1137), 160405);
    assert_eq!(burst[7].0, 1151);

    // The site painted every layout key f2f2f2 (the knob black) into a
    // zero-filled table.
    let paint = |mut data: ColorTableData| {
        for key in W75.positions() {
            let color = if Some(key.index()) == W75.knob_index {
                Rgb::new(0, 0, 0)
            } else {
                Rgb::new(0xf2, 0xf2, 0xf2)
            };
            data.set_color(key, color);
        }
        data
    };
    let zeros = reply(ReadRequest::CustomColors(CustomTable::Custom1), &[0u8; 396]);
    let request =
        WriteRequest::CustomColors1(paint(ColorTableData::from_response(&zeros).unwrap()));
    assert_same_packets(&request, &burst);
    for ((index, _), step) in burst.iter().zip(request.steps()) {
        assert_acked(&c, *index, &step);
    }

    // Read-modify-write from the real table keeps the two lit slots outside
    // the layout (103, 126) that the site zeroed.
    let factory = reply_in(
        &load("capture_01_connect"),
        0..usize::MAX,
        ReadRequest::CustomColors(CustomTable::Custom1),
    );
    let ours = paint(ColorTableData::from_response(&factory).unwrap());
    let site: Vec<u8> = burst
        .iter()
        .flat_map(|(_, b)| b[5..5 + usize::from(b[4])].to_vec())
        .collect();
    let differing: std::collections::BTreeSet<usize> = (0..396)
        .filter(|i| ours.as_bytes()[*i] != site[*i])
        .map(|i| i / 3)
        .collect();
    assert_eq!(differing.into_iter().collect::<Vec<_>>(), vec![103, 126]);
}

// ---------------------------------------------------------------------------
// W5, W6, W7: keymap, macros, advanced keys.
// ---------------------------------------------------------------------------

/// The site's forced knob defaults for entries 16-21 (spec section 4.10).
const KNOB_DEFAULTS: [u8; 24] = [
    0x07, 0x12, 0, 0, 0x07, 0x11, 0, 0, 0x07, 0x10, 0, 0, 0x02, 0x02, 0, 0, 0x03, 0x01, 0, 0, 0x03,
    0x00, 0, 0,
];

#[test]
fn c09_keymap_remap_without_the_sites_defaults() {
    let c08 = load("capture_08_profile_switch");
    let c = load("capture_09_keymap_W_to_A");
    // Read-modify-write from the device's all-zero table (c08).
    let (layer, mut base) = KeymapData::from_response(&reply_in(
        &c08,
        0..usize::MAX,
        ReadRequest::Keymap(Layer::Base),
    ))
    .unwrap();
    assert_eq!(layer, Layer::Base);
    assert!(base.as_bytes().iter().all(|b| *b == 0));
    base.set_entry(W, [0x00, 0x04, 0x00, 0x00]);
    let request = WriteRequest::Keymap(Layer::Base, base);
    let ours = packets(&request);
    let site = outs_from(&c, 202, 10, |b| b[..2] == [0x18, 0x00]);
    for chunk in [0, 2, 3, 4, 5, 6, 7, 9] {
        assert_eq!(ours[chunk][..], site[chunk].1[..], "chunk {chunk}");
    }
    // Chunk 1 differs exactly by the knob defaults (entries 16-21 = payload
    // 8..32), chunk 8 by `01 00 00 00` at Fn (entry 121 = payload 36).
    let mut forced = ours[1];
    forced[5 + 8..5 + 32].copy_from_slice(&KNOB_DEFAULTS);
    assert_eq!(forced[..], site[1].1[..]);
    let mut forced = ours[8];
    forced[5 + 36] = 0x01;
    assert_eq!(forced[..], site[8].1[..]);
    assert_ne!(
        ours[1][..],
        site[1].1[..],
        "we never write the knob defaults"
    );

    // The Fn layer is written back unchanged (all zero).
    let (layer, fn_layer) = KeymapData::from_response(&reply_in(
        &c08,
        0..usize::MAX,
        ReadRequest::Keymap(Layer::Fn),
    ))
    .unwrap();
    assert_eq!(layer, Layer::Fn);
    let request = WriteRequest::Keymap(Layer::Fn, fn_layer);
    assert_same_packets(
        &request,
        &outs_from(&c, 217, 10, |b| b[..2] == [0x18, 0x02]),
    );
}

#[test]
fn c13_keymap_read_modify_write_matches_the_site() {
    // Profile 1 after c09 holds the site's knob defaults; binding Caps to
    // macro slot 0 changes only entry 66 (c13).
    let c10 = load("capture_10_triggers_Q_W_profile_reset");
    let c = load("capture_13_macro_ab_on_caps");
    let read = reply_in(&c10, 799..usize::MAX, ReadRequest::Keymap(Layer::Base));
    let (_, mut base) = KeymapData::from_response(&read).unwrap();
    assert_eq!(&base.as_bytes()[64..88], &KNOB_DEFAULTS);
    let caps = W75.key_by_name("Caps Lock").and_then(|k| k.pos()).unwrap();
    assert_eq!(caps.index(), 66);
    base.set_entry(caps, [0x10, 0x39, 0x00, 0x00]);
    let refs = base.references();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].index(), 66);
    let request = WriteRequest::Keymap(Layer::Base, base);
    assert_same_packets(
        &request,
        &outs_ending_at(&c, 435, 10, |b| b[..2] == [0x18, 0x00]),
    );
}

#[test]
fn c13_macro_slot_0() {
    let c = load("capture_13_macro_ab_on_caps");
    // Spec 4.9.3 layout: ID 0, play once, 16 step bytes, count 1; A down
    // 100 ms, A up 100 ms, B down 100 ms, B up.
    let mut slot = vec![0u8; 256];
    slot[..24].copy_from_slice(&[
        0x00, 0x00, 0x00, 0x10, 0x00, 0x01, 0x00, 0x00, 0x04, 0x10, 0x00, 0x64, 0x04, 0x00, 0x00,
        0x64, 0x05, 0x10, 0x00, 0x64, 0x05, 0x00, 0x00, 0x00,
    ]);
    let zero = MacroSlotId::new(0).unwrap();
    let (id, data) = MacroData::from_response(&reply(ReadRequest::Macro(zero), &slot)).unwrap();
    assert_eq!(id, zero);
    assert!(!data.is_empty());
    let request = WriteRequest::Macro(zero, data);
    assert_same_packets(
        &request,
        &outs_ending_at(&c, 326, 5, |b| b[..2] == [0x19, 0x00]),
    );
    // The second Apply clears it again: zeros.
    let request = WriteRequest::Macro(zero, MacroData::Empty);
    assert_same_packets(
        &request,
        &outs_ending_at(&c, 564, 5, |b| b[..2] == [0x19, 0x00]),
    );
}

#[test]
fn c09_empty_macros_and_advanced_key_clears() {
    let c08 = load("capture_08_profile_switch");
    let c = load("capture_09_keymap_W_to_A");
    // The factory macro slots read as all FF and are written as zeros.
    let mut index = 102;
    for slot in MacroSlotId::all() {
        let read = reply_in(&c08, 0..usize::MAX, ReadRequest::Macro(slot));
        let (_, data) = MacroData::from_response(&read).unwrap();
        assert_eq!(data, MacroData::Empty);
        let request = WriteRequest::Macro(slot, data);
        let captured = outs_from(&c, index, 5, |b| b[..2] == [0x19, slot.get()]);
        assert_same_packets(&request, &captured);
        index = captured[4].0 + 1;
    }
    assert_eq!(index, 201);
    let captured = outs_from(&c, 229, 40, |b| b[0] == 0x22);
    for (slot, packet) in AdvancedSlotId::all().zip(&captured) {
        assert_same_packets(&WriteRequest::AdvancedKeyClear(slot), &[*packet]);
    }
    // The c10 read after the Apply shows every advanced slot clear.
    let c10 = load("capture_10_triggers_Q_W_profile_reset");
    let read = reply_in(&c10, 799..usize::MAX, ReadRequest::AdvancedKeys);
    for slot in AdvancedSlotId::all() {
        assert_eq!(
            WriteRequest::AdvancedKeyClear(slot).check(std::slice::from_ref(&read)),
            Ok(())
        );
    }
}

#[test]
fn every_table_packet_of_the_keymap_applies_is_acked() {
    let ack = |request: WriteRequest| request.steps()[0].ack;
    let zero = MacroSlotId::new(0).unwrap();
    let keymap_ack = ack(WriteRequest::Keymap(
        Layer::Fn,
        KeymapData::from_response(&reply(ReadRequest::Keymap(Layer::Fn), &[0u8; 528]))
            .unwrap()
            .1,
    ));
    let macro_ack = ack(WriteRequest::Macro(zero, MacroData::Empty));
    let advanced_ack = ack(WriteRequest::AdvancedKeyClear(
        AdvancedSlotId::new(0).unwrap(),
    ));
    for name in ["capture_09_keymap_W_to_A", "capture_13_macro_ab_on_caps"] {
        let c = load(name);
        let keymap = count_outs(&c, |b| b[0] == 0x18 && matches!(b[1], 0x00 | 0x02));
        let macros = count_outs(&c, |b| b[0] == 0x19 && b[1] < 0x80);
        let advanced = count_outs(&c, |b| b[0] == 0x22 && b[1] < 0x80);
        assert!(keymap > 0 && macros > 0 && advanced > 0, "{name}");
        assert_eq!(count_acks(&c, keymap_ack), keymap, "{name}: 18");
        assert_eq!(count_acks(&c, macro_ack), macros, "{name}: 19");
        assert_eq!(count_acks(&c, advanced_ack), advanced, "{name}: 22");
        // Every ack has the shape `is_write_ack` recognises.
        let acks = c
            .in_reports()
            .filter(|(_, r)| matches!(r.cmd(), 0x18 | 0x19 | 0x22) && is_write_ack(r))
            .count();
        assert_eq!(acks, keymap + macros + advanced, "{name}");
    }
}

// ---------------------------------------------------------------------------
// W9: dead band.
// ---------------------------------------------------------------------------

#[test]
fn c12_dead_band_writes() {
    let c08 = load("capture_08_profile_switch");
    let c = load("capture_12_deadband_W");
    let read = reply_in(&c08, 0..usize::MAX, ReadRequest::DeadBand);
    let device = DeadBandData::from_response(&read).unwrap();
    assert!(device.as_bytes().iter().all(|b| *b == 0));

    // Table 17 (last chunk #257): W top 0.10 mm (5 units), bottom 0.20 mm
    // (10 units).
    let mut w = device.clone();
    w.set_entry(W, [5, 10]);
    assert_eq!(w.validate(DeadBandData::max_units(20)), Ok(()));
    let request = WriteRequest::DeadBand(DeadBandWrite::new(&W75, w, 50).unwrap());
    assert_same_packets(
        &request,
        &outs_ending_at(&c, 257, 5, |b| b[..2] == [0x26, 0x01]),
    );
    assert_eq!(packets(&request)[1][5 + 34..5 + 36], [0x05, 0x0a]);

    // The final write is the all-zero table, the device's own bytes.
    let all: Vec<(usize, [u8; 63])> = c
        .out_packets()
        .filter(|(_, b)| b[..2] == [0x26, 0x01])
        .map(|(i, b)| (i, *b))
        .collect();
    assert_eq!(all.len(), 185);
    assert_same_packets(
        &WriteRequest::DeadBand(DeadBandWrite::new(&W75, device, 50).unwrap()),
        &all[180..],
    );

    // Every chunk is acked with `26 01 00 00 00` (byte 1 echoed).
    let spec = request.steps()[0].ack;
    assert_eq!(count_acks(&c, spec), 185);
}
