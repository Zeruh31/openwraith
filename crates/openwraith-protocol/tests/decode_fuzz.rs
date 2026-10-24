//! Decoders must never panic on device input (hard rule of the project).
//!
//! For every constructible read request this builds a few hundred replies
//! whose headers match the request (so they pass [`RawResponse::from_reports`])
//! but whose declared lengths and contents are arbitrary, and decodes them.
//! A panic fails the test; decode errors are expected and fine. The random
//! generator has a fixed seed, so the test is deterministic.

use openwraith_protocol::{
    HEADER_LEN, REPORT_LEN, RawResponse, ReadRequest, Report, ResponseSpec, SubMatch,
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
