//! Test support: loading the USB captures in `captures/` and pulling decoded
//! replies out of them.

#![allow(dead_code)]

use std::path::PathBuf;

use openwraith_protocol::capture::{Capture, Exchange};
use openwraith_protocol::{FromResponse, KeyTrigger, ReadRequest};

/// Every capture file, without the `.json` extension.
pub const CAPTURES: [&str; 7] = [
    "capture_01_connect",
    "capture_02_light",
    "capture_03_actuation_W",
    "capture_04_session_after_W",
    "capture_05_reconnect_W_persisted",
    "capture_06_powercycle_W_persisted",
    "capture_07_fn_space_reset",
];

/// Path of `captures/<name>.json` at the repository root.
pub fn capture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../captures")
        .join(format!("{name}.json"))
}

/// Loads and parses a capture (panics with a clear message in tests).
pub fn load(name: &str) -> Capture {
    let path = capture_path(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    Capture::from_json(&text).unwrap_or_else(|e| panic!("cannot parse {name}: {e}"))
}

/// Complete exchanges of `request` whose `OUT` entry lies in `range`.
pub fn exchanges_in(
    capture: &Capture,
    range: std::ops::Range<usize>,
    want: impl Fn(&ReadRequest) -> bool,
) -> Vec<Exchange> {
    capture
        .pair_reads()
        .exchanges
        .into_iter()
        .filter(|ex| ex.complete && range.contains(&ex.out_index) && want(&ex.request))
        .collect()
}

/// Decodes the first complete reply to `request` in the capture.
pub fn first<T: FromResponse>(capture: &Capture, request: ReadRequest) -> T {
    let ex = exchanges_in(capture, 0..usize::MAX, |r| *r == request)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no complete reply to {request}"));
    let response = ex.response().expect("complete reply");
    T::from_response(&response).unwrap_or_else(|e| panic!("decode {request}: {e}"))
}

/// The last trigger reading of every key whose request lies in `range`,
/// indexed by key index.
pub fn triggers_in(
    capture: &Capture,
    range: std::ops::Range<usize>,
) -> std::collections::BTreeMap<u8, KeyTrigger> {
    exchanges_in(capture, range, |r| matches!(r, ReadRequest::KeyTrigger(_)))
        .into_iter()
        .map(|ex| {
            let t = KeyTrigger::from_response(&ex.response().expect("complete")).expect("decodes");
            (t.index(), t)
        })
        .collect()
}
