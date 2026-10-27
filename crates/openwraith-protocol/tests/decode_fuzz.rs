//! Decoders must never panic on device input (hard rule of the project).
//!
//! For every constructible read request this builds a few hundred replies
//! whose headers match the request (so they pass [`RawResponse::from_reports`])
//! but whose declared lengths and contents are arbitrary, and decodes them.
//! A panic fails the test; decode errors are expected and fine. The random
//! generator has a fixed seed, so the test is deterministic.

use openwraith_protocol::write::{AdvancedSlotData, KeyAssignment, MacroData, MacroProgram};
use openwraith_protocol::{
    DeviceEvent, EntryKind, HEADER_LEN, KeyPos, MacroSlotId, REPORT_LEN, RawResponse, ReadRequest,
    Report, ResponseSpec, SocdPairsData, SubMatch, TravelReport, W75,
};

/// Replies generated per request.
const REPLIES_PER_REQUEST: usize = 200;

/// xorshift64*: small, deterministic, good enough for test data.
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

    /// A value in `0..n` (`n` > 0).
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// How the payload bytes of a generated reply are filled.
#[derive(Clone, Copy)]
enum Fill {
    Zero,
    Ones,
    Random,
}

/// One reply chunk with a matching header and arbitrary contents.
fn chunk(
    rng: &mut Rng,
    spec: &ResponseSpec,
    request: ReadRequest,
    index: u16,
    fill: Fill,
) -> Report {
    let mut bytes = [0u8; REPORT_LEN];
    for byte in bytes.iter_mut().skip(HEADER_LEN) {
        *byte = match fill {
            Fill::Zero => 0x00,
            Fill::Ones => 0xFF,
            Fill::Random => rng.byte(),
        };
    }
    bytes[0] = spec.cmd;
    bytes[1] = match spec.sub {
        SubMatch::Exact(sub) => sub,
        SubMatch::Below(limit) => rng.byte().checked_rem(limit).unwrap_or(0),
        SubMatch::Any => rng.byte(),
    };
    let [hi, lo] = index.to_be_bytes();
    bytes[2] = hi;
    bytes[3] = lo;
    let expected = request
        .table_layout()
        .map_or(58, |layout| layout.expected_len(index));
    bytes[4] = match rng.below(4) {
        0 => u8::try_from(expected).unwrap_or(u8::MAX),
        1 => rng.byte(),
        2 => 0,
        _ => rng.byte() % 16,
    };
    if let Some(selector) = spec.selector {
        bytes[5] = selector;
    }
    if let Some(pos) = spec.echo {
        bytes[15] = pos.row();
        bytes[16] = pos.col();
    }
    Report::new(bytes)
}

#[test]
fn decoders_never_panic_on_arbitrary_replies() {
    let mut rng = Rng(0x0BAD_5EED_0123_4567);
    let mut decoded = 0usize;
    let mut errors = 0usize;
    for request in ReadRequest::all() {
        let spec = request.response_spec();
        for _ in 0..REPLIES_PER_REQUEST {
            let fill = match rng.below(3) {
                0 => Fill::Zero,
                1 => Fill::Ones,
                _ => Fill::Random,
            };
            let reports: Vec<Report> = (0..spec.chunks)
                .map(|i| chunk(&mut rng, &spec, request, i, fill))
                .collect();
            let raw = RawResponse::from_reports(request, reports)
                .unwrap_or_else(|e| panic!("generated reply for {request} is invalid: {e}"));
            match raw.decode() {
                Ok(_) => decoded += 1,
                Err(_) => errors += 1,
            }
        }
    }
    let total = ReadRequest::all().len() * REPLIES_PER_REQUEST;
    assert_eq!(decoded + errors, total);
    // Both paths were exercised.
    assert!(decoded > total / 10, "{decoded} of {total} decoded");
    assert!(errors > total / 10, "{errors} of {total} failed to decode");
}

/// A random byte biased towards small values and zero, so that the v0.2
/// parsers get past their first checks.
fn biased(rng: &mut Rng) -> u8 {
    match rng.below(4) {
        0 => 0,
        1 => u8::try_from(rng.below(5)).unwrap_or(0),
        2 => 0x04 + u8::try_from(rng.below(0x30)).unwrap_or(0),
        _ => rng.byte(),
    }
}

/// A valid advanced-key slot: TGL, MT or a one-row DKS with random keys.
fn valid_slot(rng: &mut Rng) -> Vec<u8> {
    let mut slot = vec![0u8; 40];
    let key = |rng: &mut Rng| 0x04 + u8::try_from(rng.below(0x20)).unwrap_or(0);
    match rng.below(3) {
        0 => slot[..3].copy_from_slice(&[0x03, 0x00, key(rng)]),
        1 => {
            let threshold = 15 + u8::try_from(rng.below(86)).unwrap_or(0);
            let (hold, tap) = (key(rng), key(rng));
            slot[..10].copy_from_slice(&[0x02, 0, hold, 0, 0, 0, tap, 0, 0, threshold]);
        }
        _ => {
            let r1 = 1 + u8::try_from(rng.below(30)).unwrap_or(0);
            let row = 5 + 8 * usize::try_from(rng.below(4)).unwrap_or(0);
            slot[..5].copy_from_slice(&[0x01, r1, 30, 30, r1]);
            slot[row + 1] = key(rng);
            slot[row + 4] = 1;
        }
    }
    slot
}

#[test]
fn v02_parsers_never_panic_on_arbitrary_input() {
    let mut rng = Rng(0x7632_F022_0000_0001);
    let mut parsed = [0usize; 4];
    for _ in 0..20_000 {
        // Travel-stream reports and SOCD pushes: random bytes, usually with
        // a matching header.
        let mut bytes = [0u8; REPORT_LEN];
        for byte in bytes.iter_mut() {
            *byte = biased(&mut rng);
        }
        let variant = 0x03 + 2 * (bytes[4] & 1);
        match rng.below(3) {
            0 => bytes[..6].copy_from_slice(&[0x21, 0, 0, 0, variant, 0x01]),
            1 => bytes[..5].copy_from_slice(&[0x24, 0x02, 0, 0, 0x06]),
            _ => {}
        }
        let report = Report::new(bytes);
        if TravelReport::parse(&report).is_some() {
            parsed[0] += 1;
        }
        let _ = DeviceEvent::classify(&report);

        // Advanced-key slots: a valid DKS, MT or TGL slot, perturbed half of
        // the time; now and then a random length.
        let mut slot = valid_slot(&mut rng);
        if rng.below(2) == 0 {
            let at = usize::try_from(rng.below(40)).unwrap_or(0);
            slot[at] = biased(&mut rng);
        }
        if rng.below(10) == 0 {
            slot.truncate(usize::try_from(rng.below(64)).unwrap_or(0));
        }
        if let Ok(Some(data)) = AdvancedSlotData::from_read_slot(&slot) {
            parsed[1] += 1;
            assert!(data.def().is_some());
            assert_eq!(data.as_bytes()[..], slot[..]);
        }

        // Keymap entries on every matrix position.
        let entry = [
            biased(&mut rng),
            biased(&mut rng),
            biased(&mut rng),
            biased(&mut rng),
        ];
        let index = u8::try_from(rng.below(132)).unwrap_or(0);
        if let Some(pos) = KeyPos::from_index(index) {
            if let EntryKind::Assignable(a) = KeyAssignment::classify(&W75, pos, entry) {
                parsed[2] += 1;
                assert_eq!(a.entry(&W75, pos), Ok(entry));
            }
        }
    }

    // SOCD pair tables and macro slots, as complete replies.
    let slot = MacroSlotId::new(3).unwrap_or_else(|| unreachable!());
    for request in [ReadRequest::SocdPairs, ReadRequest::Macro(slot)] {
        let layout = request.table_layout().unwrap_or_else(|| unreachable!());
        let spec = request.response_spec();
        for _ in 0..2_000 {
            let mut table: Vec<u8> = (0..layout.total_len).map(|_| biased(&mut rng)).collect();
            if request == ReadRequest::SocdPairs {
                // The site's form (distinct letters and digits, modes 0-3),
                // perturbed half of the time.
                let used = usize::try_from(rng.below(21)).unwrap_or(0);
                table.fill(0);
                for (i, entry) in table.chunks_exact_mut(4).take(used).enumerate() {
                    let key = u8::try_from(0x04 + 2 * i).unwrap_or(0);
                    let mode = u8::try_from(rng.below(4)).unwrap_or(0);
                    entry.copy_from_slice(&[0x01, mode, key, key + 1]);
                }
                if rng.below(2) == 0 {
                    let at = usize::try_from(rng.below(80)).unwrap_or(0);
                    table[at] = biased(&mut rng);
                }
            } else {
                // A plausible macro header and step flags.
                let steps = usize::try_from(rng.below(64)).unwrap_or(0);
                table[1] = u8::try_from(rng.below(5)).unwrap_or(0);
                table[2] = 0;
                table[3] = u8::try_from(steps * 4).unwrap_or(0);
                table[6] = 0;
                table[7] = 0;
                for k in 0..steps.min(62) {
                    table[9 + 4 * k] = [0x00, 0x10, 0x01, 0x11][k % 4];
                }
                for byte in table.iter_mut().skip(8 + 4 * steps.min(62)) {
                    *byte = 0;
                }
            }
            let reports = (0..layout.chunks).map(|i| {
                let start = usize::from(i) * layout.chunk_len;
                let len = layout.expected_len(i);
                let mut bytes = [0u8; REPORT_LEN];
                let [hi, lo] = i.to_be_bytes();
                let sub = match spec.sub {
                    SubMatch::Exact(sub) => sub,
                    SubMatch::Any | SubMatch::Below(_) => 0,
                };
                bytes[..5].copy_from_slice(&[
                    spec.cmd,
                    sub,
                    hi,
                    lo,
                    u8::try_from(len).unwrap_or(0),
                ]);
                bytes[5..5 + len].copy_from_slice(&table[start..start + len]);
                Report::new(bytes)
            });
            let raw = RawResponse::from_reports(request, reports)
                .unwrap_or_else(|e| panic!("generated reply for {request} is invalid: {e}"));
            if let Ok(pairs) = SocdPairsData::from_response(&raw) {
                parsed[3] += 1;
                assert_eq!(pairs.as_bytes()[..], table[..]);
                let _ = pairs.pairs();
            }
            if let Ok((_, MacroData::Content(content))) = MacroData::from_response(&raw) {
                let _ = MacroProgram::decode(&content);
            }
        }
    }
    // Every parser accepted something, so its full path ran.
    assert!(parsed.iter().all(|n| *n > 0), "{parsed:?}");
}
