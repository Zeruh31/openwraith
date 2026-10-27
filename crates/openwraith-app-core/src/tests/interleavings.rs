//! Random interleavings of commands and keyboard events (seeded, no new
//! dependency) against the invariants of the write path:
//!
//! * a write packet is sent only while an apply step consumes its token;
//! * nothing is sent after a reset notification until a reconnect;
//! * nothing is sent while observing or while another program is heard;
//! * no profile select within 2 s of a setting write (same power cycle);
//! * the simulator refuses no packet.

use std::time::Duration;

use super::harness::Harness;
use crate::dto::{PlanToken, RtIntent, SimAction};
use crate::worker::{Conn, Request};

/// xorshift64*.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const KEYS: [u8; 6] = [46, 68, 69, 70, 45, 47];

/// Whether `bytes` is a profile select.
fn is_select(bytes: &[u8]) -> bool {
    bytes[..6] == [0x21, 0, 0, 0, 0, 0x12]
}

fn run(seed: u64) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut h = Harness::ready();
    let mut tokens: Vec<(PlanToken, u8)> = Vec::new();
    let mut unplugs: Vec<Duration> = Vec::new();
    for step in 0..30 {
        let writes = h.writes();
        let packets = h.packets();
        let before = h.worker.conn.clone();
        let mut apply = false;
        let choice = rng.below(17);
        if std::env::var_os("TRACE_SEED").is_some_and(|v| v == seed.to_string().as_str()) {
            eprintln!("step {step}: choice {choice}, conn {:?}", h.worker.conn);
        }
        match choice {
            0..=3 => {
                let n = usize::try_from(rng.below(15)).unwrap() + 1;
                h.ticks(n);
            }
            4 => {
                if let Some(s) = h.shared.snapshot() {
                    let key = KEYS[usize::try_from(rng.below(6)).unwrap()];
                    let rt = match rng.below(3) {
                        0 => None,
                        1 => Some(RtIntent::Off),
                        _ => Some(RtIntent::Same { sensitivity: 15 }),
                    };
                    let intent = crate::dto::TriggerEditIntent {
                        keys: vec![key],
                        actuation: Some(u16::try_from(rng.below(160)).unwrap() + 5),
                        rapid_trigger: rt,
                        based_on: s.revision,
                    };
                    if let Ok(p) = h.prepare_trigger(intent) {
                        if let Some(t) = p.token {
                            tokens.push((t, 0));
                        }
                    }
                }
            }
            5 => {
                if let Some(s) = h.shared.snapshot() {
                    let intent = crate::dto::LightEditIntent {
                        mode: None,
                        color: None,
                        brightness: Some(u8::try_from(rng.below(5)).unwrap()),
                        speed: None,
                        based_on: s.revision,
                    };
                    if let Ok(p) = h.prepare_light(intent) {
                        if let Some(t) = p.token {
                            tokens.push((t, 1));
                        }
                    }
                }
            }
            6 => {
                let profile = u8::try_from(rng.below(4)).unwrap() + 1;
                if let Ok(p) = h.prepare_switch(profile) {
                    if let Some(t) = p.token {
                        tokens.push((t, 2));
                    }
                }
            }
            7 | 8 => {
                // Apply a recent token (or an older, stale one).
                if !tokens.is_empty() {
                    let i = usize::try_from(rng.below(tokens.len() as u64)).unwrap();
                    let (token, kind) = tokens.remove(i);
                    apply = true;
                    let watch = h.watch();
                    let _ = match kind {
                        0 => h.apply_trigger(&token, &watch).map(|_| ()),
                        1 => h.apply_light(&token, &watch).map(|_| ()),
                        2 => h.apply_switch(&token, &watch).map(|_| ()),
                        _ => h.apply_all(&token, &watch).map(|_| ()),
                    };
                }
            }
            9 => {
                let _ = h.refresh();
            }
            10 => {
                let action = match rng.below(7) {
                    0 => SimAction::VendorHeartbeat {
                        count: 1,
                        interval_ms: u32::try_from(rng.below(3000)).unwrap(),
                    },
                    1 => SimAction::KeyboardSwitch {
                        profile: u8::try_from(rng.below(4)).unwrap() + 1,
                        after_ms: u32::try_from(rng.below(3000)).unwrap(),
                        report: true,
                    },
                    2 => SimAction::KeyboardSwitch {
                        profile: u8::try_from(rng.below(4)).unwrap() + 1,
                        after_ms: u32::try_from(rng.below(3000)).unwrap(),
                        report: false,
                    },
                    3 => SimAction::KeyboardReset,
                    4 => {
                        unplugs.push(h.with_sim(|s| s.elapsed()));
                        SimAction::Unplug
                    }
                    5 => SimAction::Replug,
                    _ => SimAction::DropNextAck,
                };
                h.sim.apply(action).unwrap();
            }
            11 => {
                if matches!(
                    h.worker.conn,
                    Conn::ResetSeen | Conn::Lost { .. } | Conn::ReadFailed { .. }
                ) {
                    let _ = h.op(|reply| Request::Reconnect { reply });
                }
            }
            12 => {
                if matches!(h.worker.conn, Conn::OtherProgram { .. }) {
                    let _ = h.op(|reply| Request::RecheckOtherProgram { reply });
                }
            }
            13 => {
                h.post(Request::WindowFocused);
            }
            14 => {
                if let Some((token, _)) = tokens.last() {
                    let token = token.clone();
                    h.post(Request::DiscardPlan { token });
                }
            }
            15 => {
                let destination = h.destination(&format!("all-{step}.owbackup.json"));
                if let Ok(p) = h.prepare_all(&destination) {
                    tokens.push((p.token, 3));
                }
            }
            _ => h.advance(Duration::from_millis(rng.below(4000))),
        }
        // Writes only inside an apply step.
        if !apply {
            assert_eq!(
                h.writes(),
                writes,
                "seed {seed} step {step}: a write outside an apply"
            );
        }
        // Nothing sent while observing or while another program is heard.
        let quiet = |c: &Conn| matches!(c, Conn::Observing | Conn::OtherProgram { .. });
        if quiet(&before) && quiet(&h.worker.conn) {
            assert_eq!(
                h.packets(),
                packets,
                "seed {seed} step {step}: traffic while listening only"
            );
        }
        assert_eq!(
            h.sim.post_reset_packets(),
            0,
            "seed {seed} step {step}: traffic after a reset"
        );
    }
    // No select within 2 s of an earlier setting write (same power cycle).
    let (packets, times) = h.with_sim(|s| (s.written().to_vec(), s.sent_times().to_vec()));
    let mut last_write: Option<Duration> = None;
    for (packet, at) in packets.iter().zip(&times) {
        if !packet.is_write() {
            continue;
        }
        let bytes = packet.as_bytes();
        if is_select(bytes) {
            if let Some(w) = last_write {
                let unplugged_between = unplugs.iter().any(|u| *u >= w && *u <= *at);
                assert!(
                    unplugged_between || at.saturating_sub(w) >= Duration::from_secs(2),
                    "seed {seed}: a select {:?} after a setting write",
                    at.saturating_sub(w)
                );
            }
        } else {
            last_write = Some(*at);
        }
    }
    h.assert_clean();
}

#[test]
fn random_interleavings_keep_the_invariants() {
    // `INTERLEAVING_SEEDS=5000` runs more seeds locally.
    let seeds = std::env::var("INTERLEAVING_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(150);
    for seed in 1..=seeds {
        run(seed);
    }
}
