//! Random interleavings of commands and keyboard events (seeded, no new
//! dependency) against the invariants of the write path:
//!
//! * a write packet is sent only while an apply step consumes its token;
//! * nothing is sent after a reset notification until a reconnect;
//! * nothing is sent while observing or while another program is heard;
//! * no profile select within 2 s of a setting write (same power cycle);
//! * the simulator refuses no packet.
//!
//! Besides trigger, light, profile-switch and backup operations, the
//! sequences contain keymap (key assignment), key-colour, dead-band and SOCD
//! edits, SOCD pushes from the keyboard (Fn + Right Shift), stray travel
//! streams (a test another run left open), live key-depth views, and the
//! known-good store: resets with the factory forms (detection holds the
//! profile), "It's fine, keep them", recovery comparisons and their apply.
//! Detection and promotion never send a packet: only apply steps write, and
//! a recovery only through the restore's apply.
//!
//! Half of the seeds switch the per-game automatic profiles on: the
//! sequences then contain foreground changes, games that exit, drafts, and
//! the CLI taking and releasing the device lock. Outside an apply step the
//! only write is an automatic profile select; nothing at all is sent while
//! the app is paused for the CLI.

use std::sync::Arc;
use std::time::Duration;

use openwraith_hid::DeviceLock;
use openwraith_hid::coordination::TOOL_CLI;

use super::harness::{Harness, pos};
use crate::dto::{
    ActionValue, AutoProfileRuleIntent, AutoProfileRulesIntent, KeyColorIntent, KeymapChangeIntent,
    LayerView, PlanToken, RecoveryIntent, RtIntent, RuleProgramIntent, SimAction, UiStateReport,
};
use crate::handle::WorkerHandle;
use crate::shell::ForegroundProgram;
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

/// Whether `bytes` opens or closes the travel test (they store nothing).
fn is_travel(bytes: &[u8]) -> bool {
    bytes[..5] == [0x21, 0, 0, 0, 0x18] && matches!(bytes[5], 0x02 | 0x03)
}

/// A live view's frames: it stops at the first one.
struct StopAtFirst(Arc<crate::shared::Shared>);

impl crate::sink::DepthSink for StopAtFirst {
    fn send(&self, _frame: crate::dto::DepthFrame) -> bool {
        self.0.stop_depth(crate::dto::DepthStopReason::Requested);
        true
    }
}

/// The games of the automatic profiles' seeds: (exe, profile).
const GAMES: [(&str, u8); 2] = [("game.exe", 2), ("other.exe", 3)];

fn run(seed: u64) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let lock_folder = tempfile::tempdir().unwrap();
    let lock_dir = lock_folder.path().to_path_buf();
    let automation = seed.is_multiple_of(2);
    let mut h = if automation {
        let dir = lock_dir.clone();
        let mut h = Harness::with_config(1, move |c| c.lock_dir = Some(dir));
        h.until_ready();
        h
    } else {
        Harness::ready()
    };
    let handle = WorkerHandle::from_parts(
        h.tx.clone(),
        Arc::clone(&h.shared),
        h.worker.config.clone(),
        None,
        None,
    );
    if automation {
        let rules = GAMES
            .iter()
            .map(|(exe, profile)| AutoProfileRuleIntent {
                id: None,
                label: (*exe).to_string(),
                program: RuleProgramIntent::Name {
                    exe: (*exe).to_string(),
                },
                profile: *profile,
                enabled: true,
            })
            .collect();
        handle
            .set_auto_profile_rules(&AutoProfileRulesIntent {
                enabled: true,
                default_profile: Some(1),
                rules,
            })
            .unwrap();
    }
    let mut cli: Option<DeviceLock> = None;
    let mut next_pid = 100u32;
    let mut tokens: Vec<(PlanToken, u8)> = Vec::new();
    let mut unplugs: Vec<Duration> = Vec::new();
    for step in 0..30 {
        let writes = h.writes();
        let packets = h.packets();
        let before = h.worker.conn.clone();
        let paused_before = h.worker.paused_for_cli();
        let mut apply = false;
        let choice = rng.below(if automation { 29 } else { 25 });
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
                    if let Ok(p) = h.prepare_trigger(intent)
                        && let Some(t) = p.token
                    {
                        tokens.push((t, 0));
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
                    if let Ok(p) = h.prepare_light(intent)
                        && let Some(t) = p.token
                    {
                        tokens.push((t, 1));
                    }
                }
            }
            6 => {
                let profile = u8::try_from(rng.below(4)).unwrap() + 1;
                if let Ok(p) = h.prepare_switch(profile)
                    && let Some(t) = p.token
                {
                    tokens.push((t, 2));
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
                        4 => h.apply_socd(&token, &watch).map(|_| ()),
                        5 => h.apply_keymap(&token, &watch).map(|_| ()),
                        6 => h.apply_colors(&token, &watch).map(|_| ()),
                        7 => h.apply_dead_band(&token, &watch).map(|_| ()),
                        8 => h.apply_restore(&token, &watch).map(|_| ()),
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
            16 => {
                // A SOCD edit (the power toggled, the pairs kept).
                if let Some(cache) = h.shared.editors()
                    && let crate::ops::editors::TableRead::Ok(state) = &cache.socd.data
                {
                    let intent = crate::dto::SocdEditIntent {
                        power: Some(state.power == 0),
                        pairs: None,
                        allow_not_undoable: false,
                        check_persistence: false,
                        based_on: cache.socd.revision,
                    };
                    if let Ok(p) = h.prepare_socd(intent)
                        && let Some(t) = p.token
                    {
                        tokens.push((t, 4));
                    }
                }
            }
            17 => match rng.below(3) {
                0 => {
                    let after = Duration::from_millis(rng.below(2000));
                    h.with_sim(|s| s.press_socd_hotkey_at(after));
                }
                1 => {
                    let at = Duration::from_millis(rng.below(2000) + 1);
                    h.with_sim(|s| {
                        s.leave_travel_open();
                        s.travel_script(pos(46), &[(at, 30)]);
                    });
                }
                _ => {
                    // A live view (T1 and T2 are user-started packets).
                    apply = true;
                    h.with_sim(|s| s.travel_script(pos(68), &[(Duration::from_millis(80), 40)]));
                    let sink = Arc::new(StopAtFirst(Arc::clone(&h.shared)));
                    let _ = h.depth_view(sink);
                }
            },
            19 => {
                // A key assignment (the keymap group: base and Fn keymaps).
                if h.shared.keys().is_some() {
                    let key = [45u8, 47, 66][usize::try_from(rng.below(3)).unwrap()];
                    let usage = u8::try_from(rng.below(20)).unwrap() + 4;
                    let intent = h.keymap_intent(vec![KeymapChangeIntent::Assign {
                        layer: LayerView::Base,
                        key,
                        action: ActionValue::Key { usage },
                    }]);
                    if let Ok(p) = h.prepare_keymap(intent)
                        && let Some(t) = p.token
                    {
                        tokens.push((t, 5));
                    }
                }
            }
            20 => {
                // Key colours (custom table 1).
                if h.shared.editors().is_some() {
                    let key = KEYS[usize::try_from(rng.below(6)).unwrap()];
                    let intent = crate::dto::ColorEditIntent {
                        colors: vec![KeyColorIntent {
                            key,
                            color: format!("#{:06x}", rng.below(0x0100_0000)),
                        }],
                        switch_to_custom: rng.below(2) == 0,
                        check_persistence: false,
                        based_on: h.colors().revision,
                    };
                    if let Ok(p) = h.prepare_colors(intent)
                        && let Some(t) = p.token
                    {
                        tokens.push((t, 6));
                    }
                }
            }
            21 => {
                // The dead band.
                if h.shared.editors().is_some() {
                    let key = KEYS[usize::try_from(rng.below(6)).unwrap()];
                    let intent = crate::dto::DeadBandEditIntent {
                        keys: vec![key],
                        top: Some(u16::try_from(rng.below(40)).unwrap()),
                        bottom: None,
                        check_persistence: false,
                        based_on: h.dead_band().revision,
                    };
                    if let Ok(p) = h.prepare_dead_band(intent)
                        && let Some(t) = p.token
                    {
                        tokens.push((t, 7));
                    }
                }
            }
            22 => {
                // A reset with the factory forms (Fn + Space held): the next
                // read after the reconnect looks reset; nothing is sent.
                h.with_sim(|s| s.factory_reset_all());
                h.sim.apply(SimAction::KeyboardReset).unwrap();
            }
            23 => {
                // "It's fine, keep them", or a look at the Recovery card.
                if rng.below(2) == 0 {
                    let _ = h.keep_current(&[]);
                } else {
                    let _ = h.recovery();
                    let _ = h.auto_backups();
                }
            }
            24 => {
                // A recovery comparison of the newest automatic backups (the
                // plan is a restore plan; applying it is an apply step).
                let recovery = h.recovery();
                let files: Vec<_> = recovery
                    .profiles
                    .iter()
                    .filter_map(|p| p.backups.first())
                    .take(usize::try_from(rng.below(4)).unwrap() + 1)
                    .map(|b| b.file.token.clone())
                    .collect();
                let active = h.worker.active_profile;
                let switches: Vec<u8> = recovery
                    .profiles
                    .iter()
                    .filter(|p| !p.backups.is_empty())
                    .take(files.len())
                    .map(|p| p.profile)
                    .filter(|p| Some(*p) != active)
                    .collect();
                if !files.is_empty() {
                    // The comparison switches profiles the user agreed to.
                    apply = !switches.is_empty();
                    let intent = RecoveryIntent {
                        files,
                        allow_firmware_mismatch: false,
                        confirmed_switches: switches,
                    };
                    if let Ok(p) = h.prepare_recovery(intent, &h.watch())
                        && let Some(t) = p.token
                    {
                        tokens.push((t, 8));
                    }
                }
            }
            25 => {
                // Another program in front: a game, another program, or
                // nothing known (the ticks of later steps act on it).
                let program = match rng.below(4) {
                    0 | 1 => {
                        next_pid += 1;
                        let (exe, _) = GAMES[usize::try_from(rng.below(2)).unwrap()];
                        Some(ForegroundProgram {
                            pid: next_pid,
                            exe: Some(exe.to_string()),
                            path: None,
                            elevated: true,
                        })
                    }
                    2 => Some(ForegroundProgram {
                        pid: 7,
                        exe: Some("explorer.exe".to_string()),
                        path: Some(r"C:\Windows\explorer.exe".to_string()),
                        elevated: false,
                    }),
                    _ => None,
                };
                handle.foreground_changed(program);
                if rng.below(2) == 0 {
                    // Long enough in front for its switch.
                    h.ticks(25);
                }
            }
            26 => {
                // A game exits.
                let watched = handle.auto_watched_pids();
                if !watched.is_empty() {
                    let i = usize::try_from(rng.below(watched.len() as u64)).unwrap();
                    handle.process_exited(watched[i]);
                }
            }
            27 => {
                // The CLI takes the device lock, or releases it.
                cli = match cli.take() {
                    Some(_) => None,
                    None if rng.below(2) == 0 => {
                        DeviceLock::try_exclusive(&lock_dir, TOOL_CLI, "set trigger").ok()
                    }
                    None => DeviceLock::try_shared(&lock_dir, TOOL_CLI, "info").ok(),
                };
            }
            28 => {
                handle.report_ui_state(UiStateReport {
                    drafts: rng.below(2) == 0,
                    vendor_acknowledged: rng.below(2) == 0,
                });
            }
            _ => h.advance(Duration::from_millis(rng.below(4000))),
        }
        // Writes only inside an apply step, but for the automatic profile
        // selects.
        if !apply {
            let written: Vec<_> = h
                .packets_since(packets)
                .into_iter()
                .filter(|p| p.is_write())
                .collect();
            for packet in &written {
                assert!(
                    automation && is_select(packet.as_bytes()),
                    "seed {seed} step {step}: a write outside an apply"
                );
            }
            if !automation {
                assert_eq!(h.writes(), writes, "seed {seed} step {step}");
            }
        }
        // Nothing at all while the CLI holds the lock.
        if paused_before && h.worker.paused_for_cli() && !apply {
            assert_eq!(
                h.packets(),
                packets,
                "seed {seed} step {step}: traffic while paused for the CLI"
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
        let bytes = packet.as_bytes();
        if !packet.is_write() || is_travel(bytes) {
            continue;
        }
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
    drop(cli);
    // A test another run "left open" is the simulator's own setup.
    h.with_sim(|s| s.allow_travel_left_open());
    h.assert_clean();
    // The store holds only complete single-profile reads of this keyboard.
    for p in 1..=4 {
        for name in h.auto_files(p) {
            let dir = crate::autobackup::profile_dir(
                &h.worker.config.known_good_dir(),
                &crate::autobackup::device_key(Some("W75HEARGB"), Some("SIM-1")),
                p,
            );
            let backup = openwraith_hid::Backup::load(&dir.join(&name)).unwrap();
            assert!(!backup.is_all_profiles(), "seed {seed}: {name}");
            assert_eq!(backup.active_profile, Some(p), "seed {seed}: {name}");
        }
    }
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
