//! The v0.2 commands (`set key` / `macro` / `advanced` / `colors` /
//! `deadband` / `socd`, trigger presets, `travel`, `monitor`, `profile
//! export` / `import`, `show advanced` / `socd`) run exactly as the binary
//! runs them, against the device simulator. Where the vendor site sent the
//! same write in capture session 3, the packets are compared byte for byte
//! with the capture (cited by capture and entry index).

use std::path::PathBuf;
use std::time::Duration;

use openwraith_hid::protocol::capture::Capture;
use openwraith_hid::{Backup, DeviceSim, KeymapCommit, SimTable, WriteError, read_all};
use openwraith_protocol::{Layer, MacroSlotId, ReadRequest, Report};

use crate::commands::Exit;
use crate::sim_tests::{Fixture, assert_reads_only, pos, profile, run, text, writes};
use crate::test_support::{SimConnector, TestUi, c06_backup, sim_with_profiles};
use crate::view::{AdvancedView, MacrosView, SocdView};
use crate::{is_usage_error, render};

fn c06_sim() -> DeviceSim {
    DeviceSim::from_backup(&c06_backup())
}

/// A capture of `captures/`.
fn capture(name: &str) -> Capture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../captures")
        .join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).expect("capture readable");
    Capture::from_json(&text).expect("capture parses")
}

/// Spaced hex up to the last non-zero byte (at least the 5-byte header),
/// like `WritePacket::summary`.
fn summary(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .rposition(|b| *b != 0)
        .map_or(5, |i| (i + 1).max(5));
    bytes[..end]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The OUT packets of a capture at `indices`, as summaries.
fn captured(name: &str, indices: &[usize]) -> Vec<String> {
    let capture = capture(name);
    indices
        .iter()
        .map(|index| {
            capture
                .out_packets()
                .find(|(i, _)| i == index)
                .map(|(_, bytes)| summary(bytes))
                .unwrap_or_else(|| panic!("no OUT packet #{index} in {name}"))
        })
        .collect()
}

/// The last `count` OUT packets of a capture up to entry `last` that start
/// with `head`.
fn captured_until(name: &str, last: usize, count: usize, head: &[u8]) -> Vec<String> {
    let all: Vec<String> = capture(name)
        .out_packets()
        .filter(|(i, bytes)| *i <= last && bytes.starts_with(head))
        .map(|(_, bytes)| summary(bytes))
        .collect();
    all[all.len().saturating_sub(count)..].to_vec()
}

/// The write packets after write `from` whose summary starts with `head`.
fn sent(sim: &DeviceSim, from: usize, head: &str) -> Vec<String> {
    writes(sim)[from..]
        .iter()
        .filter(|w| w.starts_with(head))
        .cloned()
        .collect()
}

/// `text` with every run of spaces collapsed to one (table columns).
fn squeezed(text: &str) -> String {
    text.lines()
        .map(|l| {
            l.split(' ')
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}

/// Asserts that `out` contains each of `expected`, spaces collapsed.
fn assert_has(out: &str, expected: &[&str]) {
    let have = squeezed(out);
    for e in expected {
        assert!(
            have.contains(&squeezed(e)),
            "{e}
---
{out}"
        );
    }
}

/// The profile selects the simulator received.
fn selects(sim: &DeviceSim) -> Vec<String> {
    sent(sim, 0, "21 00 00 00 00 12")
}

/// The 4 bytes of the entry of `pos` on `layer` of profile 1.
fn entry(sim: &DeviceSim, layer: Layer, pos: openwraith_protocol::KeyPos) -> Vec<u8> {
    let start = usize::from(pos.index()) * 4;
    sim.table(profile(1), SimTable::Keymap(layer))
        .expect("keymap")[start..start + 4]
        .to_vec()
}

fn key(name: &str) -> openwraith_protocol::KeyPos {
    openwraith_protocol::W75
        .key_by_name(name)
        .and_then(|k| k.pos())
        .unwrap_or_else(|| panic!("no key {name}"))
}

/// Runs `args` with `--yes --safety-dir` and expects `exit`.
fn run_yes(args: &[&str], connector: &mut SimConnector, fx: &Fixture, exit: Exit) -> TestUi {
    let dir = fx.safety_dir();
    let mut all: Vec<&str> = args.to_vec();
    all.extend(["--yes", "--safety-dir", text(&dir)]);
    let mut ui = TestUi::answering(&[]);
    let result = run(&all, connector, &mut ui).unwrap_or_else(|e| {
        panic!(
            "{args:?}: {e:#}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            ui.out, ui.err
        )
    });
    assert_eq!(result, exit, "{args:?}\n{}\n{}", ui.out, ui.err);
    ui
}

/// A backup of the simulator's active profile (read-only).
fn backup_of(connector: &SimConnector) -> Backup {
    read_all(&mut openwraith_hid::Session::new(
        &mut *connector.sim.borrow_mut(),
    ))
    .expect("backup")
}

#[test]
fn set_key_swaps_two_keys_and_re_sends_the_fn_keymap() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    let ui = run_yes(
        &["set", "key", "Caps=L-Ctrl", "L-Ctrl=Caps"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    println!("--- set key Caps=L-Ctrl L-Ctrl=Caps ---\n{}", ui.out);
    let out = &ui.out;
    for expected in [
        "Keymap group of profile 1:",
        "base   Caps Lock  default  L-Ctrl",
        "base   L-Ctrl     default  Caps Lock",
        "Writes, in this order (the vendor site's order):",
        "unchanged, re-sent: the keyboard keeps the base keymap only when the Fn keymap is \
         written after it",
        "Done: 1 table of 1 written and verified by reading them back; the Fn keymap (18 02) was \
         also re-sent unchanged to make the keyboard save the base keymap.",
        "Not checked by switching profiles: the keyboard is known from hardware tests to keep \
         what was written.",
        // The plan does not announce a switch that will not happen.
        "and does not switch profiles: hardware tests showed that the keyboard keeps these \
         tables",
        "To undo:       openwraith restore ",
    ] {
        assert!(out.contains(expected), "{expected}\n---\n{out}");
    }
    assert!(!out.contains("switches to profile 2 and back"), "{out}");
    let sim = connector.sim.borrow();
    // The base keymap, then the unchanged Fn keymap; no profile switch.
    let sent = writes(&sim);
    assert_eq!(sent.len(), 20, "{sent:?}");
    assert!(
        sent[..10].iter().all(|p| p.starts_with("18 00 ")),
        "{sent:?}"
    );
    assert!(
        sent[10..].iter().all(|p| p.starts_with("18 02 ")),
        "{sent:?}"
    );
    assert_eq!(entry(&sim, Layer::Base, key("Caps Lock")), [0, 0xE0, 0, 0]);
    assert_eq!(entry(&sim, Layer::Base, key("L-Ctrl")), [0, 0x39, 0, 0]);
    assert!(selects(&sim).is_empty());
    assert_eq!(sim.discarded_edits(), 0);
    assert!(!sim.has_edits_waiting_for_fn_write());
    sim.assert_clean();
    drop(sim);

    // The undo command restores the keymap.
    let saved = fx.safety_files();
    assert_eq!(saved.len(), 1);
    assert!(text(&saved[0]).contains("-set-key"), "{saved:?}");
    run_yes(
        &["restore", text(&saved[0])],
        &mut connector,
        &fx,
        Exit::Success,
    );
    let sim = connector.sim.borrow();
    assert_eq!(entry(&sim, Layer::Base, key("Caps Lock")), [0; 4]);
    assert_eq!(entry(&sim, Layer::Base, key("L-Ctrl")), [0; 4]);
    sim.assert_clean();
}

#[test]
fn fn_layer_codes_are_the_bytes_of_capture_17() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    let args = [
        "set",
        "key",
        "--layer",
        "fn",
        "F1=media:mute",
        "F2=mouse:wheel-down",
        "F3=mouse:0x10",
        "F4=media:0x25",
        "W=media:calculator",
        "Caps=combo:L-Ctrl+L-Shift+A",
        "--allow-builtin",
        "F1",
        "--allow-builtin",
        "F2",
        "--allow-builtin",
        "F3",
        "--allow-builtin",
        "F4",
    ];
    let ui = run_yes(&args, &mut connector, &fx, Exit::Success);
    println!("--- set key --layer fn ... ---\n{}", ui.out);
    for expected in [
        "Fn     F1         default (built-in: media player)  media Mute",
        "Fn     F3         default (built-in: volume +)      mouse button 0x10",
        "Fn     Caps Lock  default                           combination L-Ctrl+L-Shift+A",
        "Fn + F1 (#2) will no longer do its built-in function \"media player\"",
        "not yet verified on a W75: direction of browser forward/back (07 24 / 07 25) (Q14)",
    ] {
        assert!(ui.out.contains(expected), "{expected}\n---\n{}", ui.out);
    }
    // Only the Fn keymap: c17 #1890-#1900, byte for byte.
    let sim = connector.sim.borrow();
    assert_eq!(
        writes(&sim),
        captured_until("capture_17_keymap_codes_led_map", 1900, 10, &[0x18, 0x02])
    );
    assert_eq!(writes(&sim).len(), 10);
    sim.assert_clean();
}

#[test]
fn built_in_fn_functions_the_knob_and_fn_are_refused() {
    let fx = Fixture::new();
    let safety = fx.safety_dir();
    let mut connector = SimConnector::new(c06_sim());
    // A built-in function without --allow-builtin.
    let mut ui = TestUi::answering(&[]);
    let err = run(
        &[
            "set",
            "key",
            "--layer",
            "fn",
            "F1=A",
            "--yes",
            "--safety-dir",
            text(&safety),
        ],
        &mut connector,
        &mut ui,
    )
    .expect_err("refused");
    assert!(
        err.to_string()
            .contains("built-in function \"media player\""),
        "{err}"
    );
    assert!(
        crate::hint(&err).is_some_and(|h| h.contains("--allow-builtin")),
        "{err}"
    );
    // Fn + 2 (a profile switch) needs the critical acknowledgement.
    for allow in ["--allow-builtin", "--allow-critical-builtin"] {
        let mut ui = TestUi::answering(&[]);
        let result = run(
            &[
                "set",
                "key",
                "--layer",
                "fn",
                "2=A",
                allow,
                "2",
                "--dry-run",
            ],
            &mut connector,
            &mut ui,
        );
        if allow == "--allow-builtin" {
            let err = result.expect_err("critical");
            assert!(
                crate::hint(&err).is_some_and(|h| h.contains("--allow-critical-builtin")),
                "{err}"
            );
        } else {
            assert_eq!(result.expect("planned"), Exit::Success);
            assert!(ui.out.contains("CRITICAL"), "{}", ui.out);
            assert!(
                ui.out.contains("Dry run: nothing was written."),
                "{}",
                ui.out
            );
        }
    }
    assert_reads_only(&connector.sim.borrow());
    assert!(fx.safety_files().is_empty());
    // The knob and Fn are refused while parsing (exit code 2).
    for args in [
        &["set", "key", "Knob=A"][..],
        &["set", "key", "Fn=A"],
        &["set", "key", "A=Fn"],
        &["set", "macro", "Fn", "--steps", "A+, A-"],
        &["set", "advanced", "Knob", "toggle"],
    ] {
        let cli = <crate::Cli as clap::Parser>::try_parse_from(
            std::iter::once("openwraith").chain(args.iter().copied()),
        );
        let refused = match cli {
            Err(_) => true,
            Ok(cli) => {
                let mut ui = TestUi::answering(&[]);
                run_command_err(&cli, &mut connector, &mut ui)
            }
        };
        assert!(refused, "{args:?}");
    }
    assert_eq!(connector.sim.borrow().writes_sent(), 0);
}

/// Runs a parsed command and says whether it was refused as a usage error.
fn run_command_err(cli: &crate::Cli, connector: &mut SimConnector, ui: &mut TestUi) -> bool {
    crate::run_command(&cli.command, &crate::globals(cli), connector, ui)
        .is_err_and(|e| is_usage_error(&e))
}

#[test]
fn set_macro_writes_the_c13_slot_binds_it_and_checks_persistence() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    let ui = run_yes(
        &[
            "set",
            "macro",
            "Caps",
            "--steps",
            "A+ 100, A- 100, B+ 100, B-",
        ],
        &mut connector,
        &fx,
        Exit::Success,
    );
    println!("--- set macro Caps --steps ... ---\n{}", ui.out);
    let out = &ui.out;
    for expected in [
        "base   Caps Lock  default  macro slot 0",
        "macro slot 0: empty -> A+ 100, A- 100, B+ 100, B- (Once)",
        "switches to profile 2 and back",
        "Persistence check: profile 1 selected again after switching away; 2 written tables \
         still hold what was written (the keyboard saved them).",
        "whether the keyboard keeps macro slot 0 across a power loss is not tested yet",
    ] {
        assert!(out.contains(expected), "{expected}\n---\n{out}");
    }
    let sim = connector.sim.borrow();
    // The site's order: the macro slot (c13 #316-#326), the base keymap with
    // Caps -> `10 39 00 00` (c13 #425), the Fn keymap as commit trigger,
    // then the persistence check's switches.
    let c13 = "capture_13_macro_ab_on_caps";
    assert_eq!(
        sent(&sim, 0, "19 00"),
        captured(c13, &[316, 320, 322, 324, 326])
    );
    assert_eq!(
        entry(&sim, Layer::Base, key("Caps Lock")),
        [0x10, 0x39, 0, 0]
    );
    let heads: Vec<String> = writes(&sim).iter().map(|w| w[..5].to_string()).collect();
    assert_eq!(heads[..5], ["19 00"; 5]);
    assert_eq!(heads[5..15], ["18 00"; 10]);
    assert_eq!(heads[15..25], ["18 02"; 10]);
    assert_eq!(
        selects(&sim),
        ["21 00 00 00 00 12 02", "21 00 00 00 00 12 01"]
    );
    assert_eq!(sim.active_profile(), 1);
    assert_eq!(sim.discarded_edits(), 0);
    sim.assert_clean();
    drop(sim);

    // `show macros` gives the steps back in the same notation.
    let backup = backup_of(&connector);
    let text = render::macros(&MacrosView::new(&backup));
    assert!(text.contains("bound to Caps Lock (Base layer)"), "{text}");
    assert!(
        text.contains("As steps: \"A+ 100, A- 100, B+ 100, B-\""),
        "{text}"
    );

    // Unbinding clears the slot in the same group.
    let ui = run_yes(
        &["set", "macro", "Caps", "--unbind"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    assert!(
        ui.out
            .contains("macro slot 0: A+ 100, A- 100, B+ 100, B- (Once) -> empty"),
        "{}",
        ui.out
    );
    let sim = connector.sim.borrow();
    assert_eq!(entry(&sim, Layer::Base, key("Caps Lock")), [0; 4]);
    let slot0 = sim
        .table(
            profile(1),
            SimTable::Macro(MacroSlotId::new(0).expect("slot")),
        )
        .expect("slot");
    assert!(slot0.iter().all(|b| *b == 0), "{slot0:?}");
    sim.assert_clean();
}

#[test]
fn a_macro_slot_without_the_fn_keymap_is_checked_and_can_be_lost() {
    // Hardware test H19: does the keyboard keep a macro slot written without
    // a following Fn keymap? The simulator plays both answers.
    for macros_need_fn_write in [false, true] {
        let fx = Fixture::new();
        let mut sim = c06_sim();
        sim.set_keymap_commit(KeymapCommit {
            macros_need_fn_write,
            ..KeymapCommit::default()
        });
        let mut connector = SimConnector::new(sim);
        let exit = if macros_need_fn_write {
            Exit::Failure
        } else {
            Exit::Success
        };
        let ui = run_yes(
            &[
                "set",
                "macro",
                "--slot",
                "3",
                "--steps",
                "A+ 30, A-",
                "--commit-trigger",
                "skip",
            ],
            &mut connector,
            &fx,
            exit,
        );
        println!(
            "--- set macro --slot 3 ... --commit-trigger skip (macros need 18 02: \
             {macros_need_fn_write}) ---\n{}",
            ui.out
        );
        assert!(
            ui.out.contains("--commit-trigger skip (hardware test)"),
            "{}",
            ui.out
        );
        let sim = connector.sim.borrow();
        // Only the slot: no keymap at all, then the switches of the check.
        let heads: Vec<String> = writes(&sim).iter().map(|w| w[..5].to_string()).collect();
        assert_eq!(heads[..5], ["19 03"; 5]);
        assert!(sent(&sim, 0, "18 ").is_empty());
        assert_eq!(selects(&sim).len(), 2);
        if macros_need_fn_write {
            assert!(ui.out.contains("then LOST"), "{}", ui.out);
            assert!(ui.out.contains("1 written table LOST"), "{}", ui.out);
            // The way to repair it, not "the same command again".
            assert!(
                ui.out
                    .contains("Run the same command without --commit-trigger skip"),
                "{}",
                ui.out
            );
            assert!(!ui.out.contains("write them again"), "{}", ui.out);
        } else {
            assert!(ui.out.contains("persisted"), "{}", ui.out);
        }
        sim.assert_clean();
    }

    // Skipping the trigger is refused when a layer changes, and it never
    // goes without the check.
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    let mut ui = TestUi::answering(&[]);
    let err = run(
        &[
            "set",
            "macro",
            "Caps",
            "--steps",
            "A+, A-",
            "--commit-trigger",
            "skip",
            "--yes",
            "--safety-dir",
            text(&fx.safety_dir()),
        ],
        &mut connector,
        &mut ui,
    )
    .expect_err("a layer changes");
    assert!(err.to_string().contains("only be left out"), "{err}");
    let mut ui = TestUi::answering(&[]);
    let err = run(
        &[
            "set",
            "macro",
            "--slot",
            "3",
            "--steps",
            "A+, A-",
            "--commit-trigger",
            "skip",
            "--check-persistence",
            "never",
        ],
        &mut connector,
        &mut ui,
    )
    .expect_err("never with skip");
    assert!(is_usage_error(&err), "{err}");
    assert_eq!(connector.sim.borrow().writes_sent(), 0);
}

#[test]
fn set_advanced_writes_the_c16_slots_after_the_keymaps() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    let c16 = "capture_16_advanced_keys";
    let cases: [(&[&str], usize); 3] = [
        // The c16 DKS: 1.5 mm, E tap at stage 1, R tap at stage 2 (#2302).
        (
            &[
                "set", "advanced", "E", "dks", "--point", "1.5", "--row", "E:t...", "--row",
                "R:.t..",
            ],
            2302,
        ),
        // Mod-Tap: tap G, hold L-Ctrl after 300 ms, slot 1 (#3072).
        (
            &[
                "set", "advanced", "G", "mod-tap", "--hold", "L-Ctrl", "--time", "300ms",
            ],
            3072,
        ),
        // Toggle H in slot 2 (#3563).
        (&["set", "advanced", "H", "toggle"], 3563),
    ];
    for (args, index) in cases {
        let before = connector.sim.borrow().writes_sent();
        let ui = run_yes(args, &mut connector, &fx, Exit::Success);
        println!("--- {} ---\n{}", args.join(" "), ui.out);
        let sim = connector.sim.borrow();
        // Base keymap, the Fn keymap as commit trigger, then the slot: the
        // site's order, no profile switch (advanced keys after a complete
        // Fn keymap are known to be kept).
        let heads: Vec<String> = writes(&sim)[before..]
            .iter()
            .map(|w| w[..5].to_string())
            .collect();
        assert_eq!(heads.len(), 21, "{heads:?}");
        assert!(heads[20].starts_with("22 0"), "{heads:?}");
        assert_eq!(sent(&sim, before, "22 "), captured(c16, &[index]));
        assert!(selects(&sim).is_empty());
        assert!(
            ui.out
                .contains("completes a binding: written even after Ctrl+C"),
            "{}",
            ui.out
        );
        sim.assert_clean();
    }
    let sim = connector.sim.borrow();
    assert_eq!(entry(&sim, Layer::Base, key("E")), [0x17, 0, 0, 0]);
    assert_eq!(entry(&sim, Layer::Base, key("G")), [0x17, 1, 1, 0]);
    assert_eq!(entry(&sim, Layer::Base, key("H")), [0x17, 2, 2, 0]);
    drop(sim);

    // `show advanced` reads them back as definitions with their keys.
    let backup = backup_of(&connector);
    let text = render::advanced(&AdvancedView::new(&backup));
    println!("--- show advanced ---\n{text}");
    for expected in [
        "3 of 40 slots used",
        "Slot 0: DKS at 1.5 mm: E tap/-/-/-, R -/tap/-/-; bound to E (Base layer)",
        "Slot 1: Mod-Tap G / hold L-Ctrl after 300 ms; bound to G (Base layer)",
        "Slot 2: Toggle H; bound to H (Base layer)",
    ] {
        assert!(text.contains(expected), "{expected}\n---\n{text}");
    }

    // A lone slot change without the Fn keymap (hardware test H20): only
    // `22 00`, then the forced persistence check.
    let before = connector.sim.borrow().writes_sent();
    let ui = run_yes(
        &[
            "set",
            "advanced",
            "E",
            "dks",
            "--row",
            "E:..t.",
            "--commit-trigger",
            "skip",
        ],
        &mut connector,
        &fx,
        Exit::Success,
    );
    println!(
        "--- set advanced E dks ... --commit-trigger skip ---\n{}",
        ui.out
    );
    {
        let sim = connector.sim.borrow();
        let tables: Vec<String> = writes(&sim)[before..]
            .iter()
            .filter(|w| !w.starts_with("21 00 00 00 00 12"))
            .map(|w| w[..5].to_string())
            .collect();
        assert_eq!(tables, ["22 00"]);
        assert_eq!(selects(&sim).len(), 2);
        assert!(ui.out.contains("persisted"), "{}", ui.out);
        sim.assert_clean();
    }

    // Unbinding H clears its slot (2) in the same group.
    let ui = run_yes(
        &["set", "advanced", "H", "--unbind"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    assert!(
        ui.out.contains("advanced-key slot 2: Toggle H -> empty"),
        "{}",
        ui.out
    );
    let sim = connector.sim.borrow();
    assert_eq!(entry(&sim, Layer::Base, key("H")), [0; 4]);
    let slots = sim
        .table(profile(1), SimTable::AdvancedKeys)
        .expect("slots");
    assert!(slots[80..120].iter().all(|b| *b == 0));
    sim.assert_clean();
}

#[test]
fn clearing_a_bound_slot_is_refused() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    run_yes(
        &["set", "advanced", "H", "toggle"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    let before = connector.sim.borrow().writes_sent();
    let mut ui = TestUi::answering(&[]);
    let err = run(
        &[
            "set",
            "advanced",
            "--slot",
            "0",
            "--clear",
            "--yes",
            "--safety-dir",
            text(&fx.safety_dir()),
        ],
        &mut connector,
        &mut ui,
    )
    .expect_err("still referenced");
    assert!(err.to_string().contains("still refer to it"), "{err}");
    assert!(crate::hint(&err).is_some(), "{err}");
    assert_eq!(connector.sim.borrow().writes_sent(), before);
}

#[test]
fn set_colors_writes_custom_table_1_like_capture_17() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    let table_before = connector
        .sim
        .borrow()
        .table(profile(1), SimTable::CustomColors1)
        .expect("colours")
        .to_vec();
    let ui = run_yes(
        &["set", "colors", "Up=0000ff"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    println!("--- set colors Up=0000ff ---\n{}", ui.out);
    for expected in [
        "Custom colour table 1 of profile 1:",
        "Up   #ff0000  #0000ff",
        "keeps its bytes",
        "The main light is in the Custom mode",
    ] {
        assert!(ui.out.contains(expected), "{expected}\n---\n{}", ui.out);
    }
    let sim = connector.sim.borrow();
    // c06 holds the same table as c14: the packets are c17 #2223-#2237.
    assert_eq!(
        writes(&sim),
        captured_until("capture_17_keymap_codes_led_map", 2237, 8, &[0x09, 0x00])
    );
    let after = sim
        .table(profile(1), SimTable::CustomColors1)
        .expect("colours");
    let changed: Vec<usize> = (0..396)
        .filter(|i| after[*i] != table_before[*i])
        .map(|i| i / 3)
        .collect();
    assert_eq!(changed, [102, 102]);
    assert!(selects(&sim).is_empty());
    sim.assert_clean();
    drop(sim);

    // In another light mode the colours are not shown: a note, or the
    // switch to Custom with --custom-mode (17 00 + 07 00 after 09 00).
    let mut light = connector
        .sim
        .borrow()
        .table(profile(1), SimTable::MainLight)
        .expect("light")
        .to_vec();
    light[0] = 0x00;
    connector
        .sim
        .borrow_mut()
        .set_table(profile(1), SimTable::MainLight, light);
    let mut ui = TestUi::answering(&[]);
    run(
        &["set", "colors", "Right=0000ff", "--dry-run"],
        &mut connector,
        &mut ui,
    )
    .expect("dry run");
    assert!(ui.out.contains("add --custom-mode"), "{}", ui.out);
    let before = connector.sim.borrow().writes_sent();
    run_yes(
        &["set", "colors", "Right=0000ff", "--custom-mode"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    let sim = connector.sim.borrow();
    let heads: Vec<String> = writes(&sim)[before..]
        .iter()
        .map(|w| w[..5].to_string())
        .collect();
    assert_eq!(heads[..8], ["09 00"; 8]);
    assert_eq!(heads[8..], ["17 00", "07 00"]);
    assert_eq!(
        sim.table(profile(1), SimTable::MainLight).expect("light")[0],
        0x0A
    );
    sim.assert_clean();
}

#[test]
fn set_deadband_writes_one_table_and_says_its_scope_is_unknown() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    let ui = run_yes(
        &[
            "set", "deadband", "W", "--top", "0.10", "--bottom", "0.20", "--json",
        ],
        &mut connector,
        &fx,
        Exit::Success,
    );
    let json: serde_json::Value = serde_json::from_str(&ui.out).expect("JSON");
    assert_eq!(json["command"], "set deadband");
    assert_eq!(json["dry_run"], false);
    assert_eq!(json["confirmed"], true);
    assert_eq!(json["report"]["outcome"], "completed");
    assert!(json["safety_backup"]["path"].is_string(), "{json}");
    assert!(
        json["plan"]["warnings"]
            .as_array()
            .is_some_and(|w| w.iter().any(|w| w["warning"] == "scope_unknown")),
        "{json}"
    );
    let sim = connector.sim.borrow();
    let band = sim
        .table(profile(1), SimTable::DeadBand)
        .expect("dead band");
    // W (index 46) at offset 92: 0.10 / 0.20 mm (c12).
    assert_eq!(band[92..94], [0x05, 0x0a]);
    let heads: Vec<String> = writes(&sim).iter().map(|w| w[..5].to_string()).collect();
    assert_eq!(heads, ["26 01"; 5]);
    sim.assert_clean();
}

#[test]
fn set_socd_sends_the_c15_bursts_and_checks_persistence() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    let c15 = "capture_15_socd";
    // (arguments, the c15 burst it must equal)
    let cases: [(&[&str], usize); 4] = [
        (&["set", "socd", "--on", "--pair", "A/D"], 145), // B07: on, A/D mode 1
        (&["set", "socd", "--pair", "A/D:2"], 246),       // B10: mode 2 (wire 1)
        (&["set", "socd", "--remove-pair", "D/A"], 265),  // B11: pair deleted
        (&["set", "socd", "--off"], 272),                 // B12: off
    ];
    for (args, start) in cases {
        let before = connector.sim.borrow().writes_sent();
        let ui = run_yes(args, &mut connector, &fx, Exit::Success);
        println!("--- {} ---\n{}", args.join(" "), ui.out);
        assert!(
            ui.out
                .contains("may apply to every profile: its profile scope is not known yet"),
            "{}",
            ui.out
        );
        // SOCD's keeping is not proven: the plan announces the check.
        assert!(
            ui.out
                .contains("switches to profile 2 and back and reads SOCD again"),
            "{}",
            ui.out
        );
        let sim = connector.sim.borrow();
        let tables = sent(&sim, before, "24 ");
        assert_eq!(
            tables,
            captured(c15, &[start, start + 1, start + 2]),
            "{args:?}"
        );
        // SOCD has no persistence evidence yet: switched away and back.
        assert!(ui.out.contains("Persistence check:"), "{}", ui.out);
        assert!(ui.out.contains("persisted"), "{}", ui.out);
        sim.assert_clean();
    }
    let sim = connector.sim.borrow();
    assert_eq!(
        sim.table(profile(1), SimTable::SocdSettings)
            .expect("settings")[0],
        0
    );
    drop(sim);

    // `show socd` and the planning of a dry run.
    let mut ui = TestUi::answering(&[]);
    run(
        &[
            "set",
            "socd",
            "--on",
            "--pair",
            "Left/Right:4",
            "--dry-run",
            "--packets",
        ],
        &mut connector,
        &mut ui,
    )
    .expect("dry run");
    for expected in [
        "Power  off -> on",
        "1. Left / Right, mode 4: the later key only interrupts the first",
        "24 03 00 00 01 01",
        "not yet verified on a W75: SOCD modes 3 and 4 (Q35)",
        "Dry run: nothing was written.",
    ] {
        assert!(ui.out.contains(expected), "{expected}\n---\n{}", ui.out);
    }
    let backup = backup_of(&connector);
    let text = render::socd(&SocdView::new(&backup));
    assert!(text.contains("Power: off"), "{text}");
    assert!(text.contains("Pairs: none"), "{text}");
}

#[test]
fn a_socd_switch_on_the_keyboard_is_not_another_program() {
    let fx = Fixture::new();
    let mut sim = c06_sim();
    // Fn + Right Shift while the gate observes (before anything is sent):
    // it switches SOCD on, which is what the command asks for.
    sim.press_socd_hotkey_at(Duration::from_millis(500));
    let mut connector = SimConnector::new(sim);
    let ui = run_yes(
        &["--verbose", "set", "socd", "--on"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    println!(
        "--- set socd --on, Fn + Right Shift meanwhile ---\n{}",
        ui.out
    );
    // Counted as the keyboard's own push, not as another program.
    assert!(ui.err.contains("\"socd_pushes\":1"), "{}", ui.err);
    assert!(!ui.err.contains("another program"), "{}", ui.err);
    // The plan was power off -> on; the keyboard already held it when the
    // write came, so nothing was sent.
    assert!(ui.out.contains("0 tables of 1 written"), "{}", ui.out);
    assert!(ui.out.contains("(not written)"), "{}", ui.out);
    let sim = connector.sim.borrow();
    assert!(sent(&sim, 0, "24 ").is_empty());
    assert_eq!(
        sim.table(profile(1), SimTable::SocdSettings)
            .expect("settings")[0],
        1
    );
    sim.assert_clean();
}

#[test]
fn trigger_presets_and_all_keys() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    let ui = run_yes(
        &["set", "trigger", "W", "A", "--preset", "competitive"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    assert!(ui.out.starts_with("Preset \"competitive\"."), "{}", ui.out);
    {
        let sim = connector.sim.borrow();
        // 0.40 mm, Rapid Trigger 0.10 mm.
        assert_eq!(
            sim.trigger(profile(1), key("A")),
            Some([0x0c, 20, 20, 5, 5, 0, 0, 0, 0])
        );
        sim.assert_clean();
    }
    // The factory preset on every key: only W and A differ from it.
    let before = connector.sim.borrow().writes_sent();
    let ui = run_yes(
        &["set", "trigger", "--all", "--preset", "factory"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    assert!(ui.out.contains("(already set)"), "{}", ui.out);
    let sim = connector.sim.borrow();
    assert_eq!(sim.writes_sent() - before, 2);
    assert_eq!(
        sim.trigger(profile(1), pos(2, 2)),
        Some([0x00, 0x55, 0x55, 1, 1, 0, 0, 0, 0])
    );
    sim.assert_clean();
}

#[test]
fn dry_runs_read_only_and_ask_nothing() {
    let mut connector = SimConnector::new(c06_sim());
    for args in [
        &["set", "key", "Caps=L-Ctrl", "--dry-run", "--packets"][..],
        &["set", "macro", "Caps", "--steps", "A+, A-", "--dry-run"],
        &["set", "advanced", "H", "toggle", "--dry-run"],
        &["set", "colors", "--all", "00ff00", "--dry-run"],
        &["set", "deadband", "--all", "--top", "0.10", "--dry-run"],
        &["set", "socd", "--on", "--dry-run", "--json"],
        &["set", "trigger", "W", "--actuation", "1.20", "--dry-run"],
        &["set", "light", "--brightness", "2", "--dry-run"],
    ] {
        let mut ui = TestUi::non_interactive();
        let exit = run(args, &mut connector, &mut ui).expect("dry run");
        assert_eq!(exit, Exit::Success, "{args:?}");
        assert!(ui.questions.is_empty());
        if args.contains(&"--json") {
            let json: serde_json::Value = serde_json::from_str(&ui.out).expect("JSON");
            assert_eq!(json["dry_run"], true);
        } else {
            assert!(
                ui.out.contains("Dry run: nothing was written."),
                "{args:?}\n{}",
                ui.out
            );
        }
    }
    assert_eq!(connector.write_sessions, 0);
    assert_reads_only(&connector.sim.borrow());
}

#[test]
fn writing_v02_commands_refuse_without_a_terminal_or_yes() {
    for args in [
        &["set", "key", "Caps=L-Ctrl"][..],
        &["set", "macro", "Caps", "--steps", "A+, A-"],
        &["set", "advanced", "H", "toggle"],
        &["set", "colors", "Up=0000ff"],
        &["set", "deadband", "W", "--top", "0.10"],
        &["set", "socd", "--on"],
    ] {
        let mut connector = SimConnector::new(c06_sim());
        let mut ui = TestUi::non_interactive();
        let err = run(args, &mut connector, &mut ui).expect_err("refused");
        assert!(is_usage_error(&err), "{args:?}: {err}");
        assert_eq!((connector.read_sessions, connector.write_sessions), (0, 0));
    }
}

/// A slow press of W and a fast press of Esc, `start` from now.
fn script(sim: &mut DeviceSim, start: Duration) {
    let ms = Duration::from_millis;
    let slow: Vec<(Duration, u16)> = (0..5u16)
        .map(|k| (start + ms(72 * u64::from(k)), 10 * (k + 1)))
        .collect();
    sim.travel_script(pos(2, 2), &slow);
    let at = start + ms(500);
    sim.travel_script(
        pos(0, 0),
        &[
            (at, 40),
            (at + ms(1), 90),
            (at + ms(2), 169),
            (at + ms(80), 0),
        ],
    );
}

#[test]
fn travel_shows_depths_and_closes_with_the_c14_packets() {
    let mut sim = c06_sim();
    script(&mut sim, Duration::from_secs(4));
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    let exit = run(&["travel", "--duration", "5s"], &mut connector, &mut ui).expect("travel");
    println!(
        "--- travel --duration 5s ---\n{}\n--- stderr ---\n{}",
        ui.out, ui.err
    );
    assert_eq!(exit, Exit::Success, "{}\n{}", ui.out, ui.err);
    assert!(ui.close_first, "Ctrl+C closes the test first");
    assert_has(
        &ui.out,
        &[
            "Key-depth test: the time limit (5 s) was reached.",
            "The keyboard acknowledged the close: the test is off.",
            "Esc 4 3.38 mm 0.00 mm",
            "W 6 1.00 mm 0.00 mm",
            "10 depth report(s)",
        ],
    );
    assert!(
        ui.progress.iter().any(|p| p.starts_with("W [")),
        "{:?}",
        ui.progress
    );
    let sim = connector.sim.borrow();
    let c14 = "capture_14_travel_test";
    assert_eq!(writes(&sim), captured(c14, &[981, 1312]));
    assert!(!sim.travel_open());
    sim.assert_clean();
}

#[test]
fn ctrl_c_and_a_profile_switch_close_the_travel_test() {
    // Ctrl+C as soon as W moves.
    let mut sim = c06_sim();
    script(&mut sim, Duration::from_secs(4));
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    ui.cancel_at = Some(("W [".to_string(), 1));
    let exit = run(&["travel", "--duration", "60s"], &mut connector, &mut ui).expect("travel");
    assert_eq!(exit, Exit::Success);
    assert!(
        ui.out
            .contains("Key-depth test: stopped on request (Ctrl+C)."),
        "{}",
        ui.out
    );
    {
        let sim = connector.sim.borrow();
        assert_eq!((sim.travel_opens(), sim.travel_closes()), (1, 1));
        assert!(!sim.travel_open());
        sim.assert_clean();
    }

    // Fn + 2 on the keyboard while the test is open: closed, reported.
    let mut sim = c06_sim();
    script(&mut sim, Duration::from_secs(4));
    sim.switch_profile_at(Duration::from_secs(5), profile(2), true);
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    let exit = run(&["travel", "--duration", "60s"], &mut connector, &mut ui).expect("travel");
    assert_eq!(exit, Exit::Success);
    assert!(
        ui.out
            .contains("the keyboard switched to profile 2 (Fn + 2); the test was closed"),
        "{}",
        ui.out
    );
    let sim = connector.sim.borrow();
    assert!(!sim.travel_open());
    sim.assert_clean();
}

#[test]
fn travel_json_prints_one_object_per_report() {
    let mut sim = c06_sim();
    script(&mut sim, Duration::from_secs(4));
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    run(
        &["travel", "--duration", "5s", "--json", "--keys", "Esc"],
        &mut connector,
        &mut ui,
    )
    .expect("travel");
    let lines: Vec<serde_json::Value> = ui
        .out
        .lines()
        .map(|l| serde_json::from_str(l).expect("JSON line"))
        .collect();
    let depths: Vec<u64> = lines
        .iter()
        .filter(|l| l["event"] == "depth")
        .map(|l| l["depth_units"].as_u64().unwrap_or(0))
        .collect();
    // Only Esc (--keys): 40, 90, 169, 0.
    assert_eq!(depths, [40, 90, 169, 0]);
    let last = lines.last().expect("final line");
    assert_eq!(last["event"], "stopped");
    assert_eq!(last["outcome"]["close"]["close"], "acked");
    connector.sim.borrow().assert_clean();
}

#[test]
fn a_test_left_open_refuses_writes_until_travel_close() {
    let fx = Fixture::new();
    let mut sim = c06_sim();
    // A test a crashed run left open: the keyboard streams W.
    sim.leave_travel_open();
    script(&mut sim, Duration::ZERO);
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    let err = run(
        &[
            "set",
            "trigger",
            "W",
            "--actuation",
            "1.20",
            "--yes",
            "--safety-dir",
            text(&fx.safety_dir()),
        ],
        &mut connector,
        &mut ui,
    )
    .expect_err("refused");
    assert!(
        matches!(
            err.downcast_ref::<WriteError>(),
            Some(WriteError::TravelTestOpen { .. })
        ),
        "{err:#}"
    );
    assert!(
        crate::hint(&err).is_some_and(|h| h.contains("openwraith travel --close")),
        "{err}"
    );
    // The stream is never taken for another program.
    assert!(!ui.err.contains("another program"), "{}", ui.err);
    assert_eq!(connector.sim.borrow().writes_sent(), 0);

    let mut ui = TestUi::answering(&[]);
    let exit = run(&["travel", "--close"], &mut connector, &mut ui).expect("close");
    println!("--- travel --close ---\n{}", ui.out);
    assert_eq!(exit, Exit::Success);
    assert!(
        ui.out.contains("The keyboard acknowledged the close"),
        "{}",
        ui.out
    );
    assert!(ui.out.contains("key-depth report(s) arrived"), "{}", ui.out);
    assert!(!connector.sim.borrow().travel_open());
    assert_eq!(
        writes(&connector.sim.borrow()),
        captured("capture_14_travel_test", &[1312])
    );

    // Now the write goes through.
    run_yes(
        &["set", "trigger", "W", "--actuation", "1.20"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    connector.sim.borrow().assert_clean();
}

#[test]
fn monitor_sends_nothing_and_names_what_it_hears() {
    let mut sim = c06_sim();
    sim.press_socd_hotkey_at(Duration::from_millis(500));
    sim.switch_profile_at(Duration::from_millis(1200), profile(2), true);
    // The vendor site's heartbeat reply.
    let status: Report = c06_backup()
        .response(ReadRequest::Status)
        .expect("status")
        .reports()[0]
        .clone();
    sim.push_input_at(Duration::from_millis(2000), status);
    // A push in another form than the site-made one (byte 4 = 02, a byte
    // after the declared payload): shown exactly as it arrived.
    let mut odd = [0u8; openwraith_protocol::REPORT_LEN];
    odd[..9].copy_from_slice(&[0x21, 0, 0, 0, 0x02, 0x11, 0x03, 0x00, 0x5a]);
    sim.push_input_at(Duration::from_millis(2500), Report::new(odd));
    let base = sim.clone();
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    let exit = run(&["monitor", "--duration", "3s"], &mut connector, &mut ui).expect("monitor");
    println!("--- monitor --duration 3s ---\n{}", ui.out);
    assert_eq!(exit, Exit::Success);
    for expected in [
        "SOCD push: SOCD is now on (raw: 24 02 00 00 06 01 00 00 00 fa e5;",
        "profile report: profile 2 is active (raw: 21 00 00 00 03 11 02 00;",
        "profile report: profile 3 is active (raw: 21 00 00 00 02 11 03 00 5a;",
        "1 status report(s) nobody here asked for",
        "Listened for 3 s: 2 profile report(s), 1 SOCD push(es)",
    ] {
        assert!(ui.out.contains(expected), "{expected}\n---\n{}", ui.out);
    }
    // The JSON lines carry the raw bytes too.
    let mut json = SimConnector::new(base);
    let mut json_ui = TestUi::answering(&[]);
    run(
        &["monitor", "--duration", "3s", "--json"],
        &mut json,
        &mut json_ui,
    )
    .expect("monitor");
    let raws: Vec<String> = json_ui
        .out
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v["raw"].as_str().map(str::to_string))
        .collect();
    assert!(
        raws.contains(&"21 00 00 00 02 11 03 00 5a".to_string()),
        "{raws:?}"
    );
    assert_eq!((connector.read_sessions, connector.write_sessions), (1, 0));
    // Not even a read was sent.
    let sim = connector.sim.borrow();
    assert!(sim.written().is_empty(), "{:?}", sim.written().len());
    sim.assert_clean();
}

#[test]
fn a_profile_is_exported_without_the_usb_serial_and_imported_into_another() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(sim_with_profiles());
    let exported = fx.path("p1.owprofile.json");
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &[
            "profile",
            "export",
            "1",
            "-o",
            text(&exported),
            "--label",
            "FPS\u{1b}[5m",
        ],
        &mut connector,
        &mut ui,
    )
    .expect("export");
    assert_eq!(exit, Exit::Success);
    assert!(ui.out.contains("Profile 1 exported to"), "{}", ui.out);
    // The active profile: a pure read.
    assert_reads_only(&connector.sim.borrow());
    let file = Backup::load(&exported).expect("loads");
    assert!(file.device.usb.is_none());
    assert_eq!(file.scope, openwraith_hid::BACKUP_SCOPE_EXPORT);
    assert!(file.note.contains("Label: FPS[5m"), "{}", file.note);

    // Into profile 3 (factory triggers and light): triggers and lighting.
    let ui = run_yes(
        &["profile", "import", text(&exported), "--into", "3"],
        &mut connector,
        &fx,
        Exit::Success,
    );
    println!("--- profile import p1 --into 3 ---\n{}", ui.out);
    for expected in [
        "Import plan",
        "Profile 3",
        "trigger of W (r2c2)",
        "profile 1 of the file is imported into profile 3 of the keyboard",
        "persisted",
        "Active profile: 1 (as before).",
    ] {
        assert!(ui.out.contains(expected), "{expected}\n---\n{}", ui.out);
    }
    assert!(ui.err.contains("Importing profile 1 of"), "{}", ui.err);
    let saved = fx.safety_files();
    assert!(text(&saved[0]).contains("-import"), "{saved:?}");
    {
        let sim = connector.sim.borrow();
        assert_eq!(
            sim.trigger(profile(3), pos(2, 2)),
            Some([0x0c, 0x32, 0x32, 0x0f, 0x0f, 0, 0, 0, 0])
        );
        assert_eq!(sim.active_profile(), 1);
        sim.assert_clean();
    }

    // A profile that is not active is read after a switch (asked first).
    let other = fx.path("p2.owprofile.json");
    let mut ui = TestUi::answering(&[false]);
    let exit = run(
        &["profile", "export", "2", "-o", text(&other)],
        &mut connector,
        &mut ui,
    )
    .expect("export");
    assert_eq!(exit, Exit::Failure);
    assert!(
        ui.questions[0].contains("Profile 2 is not active"),
        "{:?}",
        ui.questions
    );
    assert!(!other.exists());
    let before = connector.sim.borrow().writes_sent();
    let mut ui = TestUi::answering(&[]);
    run(
        &["profile", "export", "2", "-o", text(&other), "--yes"],
        &mut connector,
        &mut ui,
    )
    .expect("export");
    assert_eq!(
        sent(&connector.sim.borrow(), before, ""),
        ["21 00 00 00 00 12 02", "21 00 00 00 00 12 01"]
    );
    assert_eq!(Backup::load(&other).expect("loads").active_profile, Some(2));

    // An existing file is refused before anything is read.
    let mut ui = TestUi::answering(&[]);
    let err = run(
        &["profile", "export", "1", "-o", text(&other)],
        &mut connector,
        &mut ui,
    )
    .expect_err("exists");
    assert!(err.to_string().contains("already exists"), "{err}");
}

// ----- review fixes: failure paths, imports, Ctrl+C, argument conflicts -----

#[test]
fn ctrl_c_during_the_fn_keymap_still_writes_the_slot_a_written_key_needs() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(c06_sim());
    let mut ui = TestUi::answering(&[]);
    ui.cancel_at = Some(("writing Fn keymap, packet 1/".to_string(), 1));
    let exit = run(
        &[
            "set",
            "advanced",
            "E",
            "toggle",
            "--yes",
            "--safety-dir",
            text(&fx.safety_dir()),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("runs");
    println!(
        "--- set advanced E toggle, Ctrl+C in the Fn keymap ---\n{}",
        ui.out
    );
    // Everything planned was written: exit 0, and the report says why.
    assert_eq!(exit, Exit::Success, "{}\n{}", ui.out, ui.err);
    assert!(ui.out.contains("Ctrl+C received"), "{}", ui.out);
    let sim = connector.sim.borrow();
    assert_eq!(sent(&sim, 0, "22 00").len(), 1);
    assert_eq!(sent(&sim, 0, "18 02").len(), 10);
    sim.assert_clean();
}

/// Exports profile 1 of `connector`'s keyboard (the active one) to a file
/// of `fx`.
fn export_profile_1(connector: &mut SimConnector, fx: &Fixture) -> PathBuf {
    let path = fx.path("p1.owprofile.json");
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &["profile", "export", "1", "-o", text(&path)],
        connector,
        &mut ui,
    )
    .expect("export");
    assert_eq!(exit, Exit::Success);
    path
}

#[test]
fn exporting_the_active_profile_reads_only_and_another_one_asks_first() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(sim_with_profiles());
    export_profile_1(&mut connector, &fx);
    // Only a read-only session for the active profile.
    assert_eq!((connector.read_sessions, connector.write_sessions), (1, 0));
    assert_reads_only(&connector.sim.borrow());
    // Another profile without a terminal and --yes: refused after the
    // read-only check, before a writable session is opened.
    let mut connector = SimConnector::new(sim_with_profiles());
    let mut ui = TestUi::non_interactive();
    let other = fx.path("p2.owprofile.json");
    let err = run(
        &["profile", "export", "2", "-o", text(&other)],
        &mut connector,
        &mut ui,
    )
    .expect_err("refused");
    assert!(is_usage_error(&err), "{err}");
    assert_eq!((connector.read_sessions, connector.write_sessions), (1, 0));
    assert!(!other.exists());
    assert_reads_only(&connector.sim.borrow());
}

#[test]
fn an_import_dry_run_into_a_profile_that_is_not_active_says_it_compared_nothing() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(sim_with_profiles());
    let file = export_profile_1(&mut connector, &fx);
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &[
            "profile",
            "import",
            text(&file),
            "--into",
            "3",
            "--include",
            "keys",
            "--dry-run",
            "--packets",
        ],
        &mut connector,
        &mut ui,
    )
    .expect("dry run");
    println!("--- profile import --into 3 --dry-run ---\n{}", ui.out);
    assert_eq!(exit, Exit::Success);
    assert!(
        ui.out.contains("Total: nothing compared (dry run)."),
        "{}",
        ui.out
    );
    assert!(!ui.out.contains("Total: nothing to write"), "{}", ui.out);
    assert!(
        ui.out
            .contains("Profile 3 is not active: a dry run cannot read it"),
        "{}",
        ui.out
    );
    assert!(ui.out.contains("openwraith profile select 3"), "{}", ui.out);
    assert_reads_only(&connector.sim.borrow());

    // Without a terminal and --yes, the real import refuses before the
    // keyboard is opened.
    let mut connector = SimConnector::new(sim_with_profiles());
    let mut ui = TestUi::non_interactive();
    let err = run(
        &["profile", "import", text(&file), "--into", "3"],
        &mut connector,
        &mut ui,
    )
    .expect_err("refused");
    assert!(is_usage_error(&err), "{err}");
    assert_eq!((connector.read_sessions, connector.write_sessions), (0, 0));
}

#[test]
fn an_import_blocked_by_a_critical_fn_override_fails_and_names_the_option() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(sim_with_profiles());
    // Profile 1: Fn + 2 plays Mute instead of switching to profile 2.
    run_yes(
        &[
            "set",
            "key",
            "--layer",
            "fn",
            "2=media:mute",
            "--allow-critical-builtin",
            "2",
        ],
        &mut connector,
        &fx,
        Exit::Success,
    );
    let file = export_profile_1(&mut connector, &fx);
    let before = connector.sim.borrow().writes_sent();
    let ui = run_yes(
        &[
            "profile",
            "import",
            text(&file),
            "--into",
            "3",
            "--include",
            "keys",
        ],
        &mut connector,
        &fx,
        Exit::Failure,
    );
    println!(
        "--- profile import --include keys (blocked) ---\n{}",
        ui.out
    );
    assert!(
        ui.out.contains("add --allow-critical-fn-overrides"),
        "{}",
        ui.out
    );
    assert!(
        ui.out
            .contains("A category chosen with --include was not imported"),
        "{}",
        ui.out
    );
    assert!(sent(&connector.sim.borrow(), before, "18 02").is_empty());
    // Allowed: the Fn keymap is written into profile 3.
    let before = connector.sim.borrow().writes_sent();
    run_yes(
        &[
            "profile",
            "import",
            text(&file),
            "--into",
            "3",
            "--include",
            "keys",
            "--allow-critical-fn-overrides",
        ],
        &mut connector,
        &fx,
        Exit::Success,
    );
    let sim = connector.sim.borrow();
    assert_eq!(sent(&sim, before, "18 02").len(), 10);
    let fn_layer = sim
        .persisted_table(profile(3), SimTable::Keymap(Layer::Fn))
        .expect("Fn keymap");
    assert_eq!(fn_layer[24 * 4..25 * 4], [0x07, 0x12, 0x00, 0x00]);
    sim.assert_clean();
}

#[test]
fn an_import_takes_the_profile_of_an_all_profile_backup() {
    let fx = Fixture::new();
    let mut connector = SimConnector::new(sim_with_profiles());
    let all = fx.path("all.owbackup.json");
    run(
        &["backup", "--all-profiles", "-o", text(&all)],
        &mut connector,
        &mut TestUi::answering(&[]),
    )
    .expect("backup");
    let ui = run_yes(
        &[
            "profile",
            "import",
            text(&all),
            "--profile",
            "2",
            "--into",
            "3",
        ],
        &mut connector,
        &fx,
        Exit::Success,
    );
    assert!(
        ui.out
            .contains("profile 2 of the file is imported into profile 3 of the keyboard"),
        "{}",
        ui.out
    );
}

/// The packet count up to and including the travel-test open of a run of
/// `travel` on a copy of `sim`.
fn travel_open_packet(sim: &DeviceSim) -> usize {
    let mut copy = sim.clone();
    let mut connector = SimConnector::new(copy.clone());
    let _ = run(
        &["travel", "--duration", "5s"],
        &mut connector,
        &mut TestUi::answering(&[]),
    );
    copy = connector.sim.borrow().clone();
    copy.written()
        .iter()
        .position(|p| p.is_write() && p.as_bytes()[..6] == [0x21, 0, 0, 0, 0x18, 0x02])
        .expect("the open")
        + 1
}

#[test]
fn travel_failures_say_how_to_close_the_test() {
    // Another program's heartbeat while the test is open: closed, exit 1.
    let mut sim = c06_sim();
    script(&mut sim, Duration::from_secs(4));
    let status: Report = c06_backup()
        .response(ReadRequest::Status)
        .expect("status")
        .reports()[0]
        .clone();
    sim.push_input_at(Duration::from_secs(5), status);
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    let exit = run(&["travel", "--duration", "20s"], &mut connector, &mut ui).expect("travel");
    assert_eq!(exit, Exit::Failure, "{}", ui.out);
    assert!(
        ui.out.contains("another program talked to the keyboard"),
        "{}",
        ui.out
    );
    assert!(!connector.sim.borrow().travel_open());

    // The close is never acknowledged: exit 1, and the output says how to
    // send it again.
    let mut sim = c06_sim();
    sim.drop_ack(2);
    sim.drop_ack(3);
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    let exit = run(&["travel", "--duration", "3s"], &mut connector, &mut ui).expect("travel");
    assert_eq!(exit, Exit::Failure, "{}", ui.out);
    assert!(ui.out.contains("NOT acknowledged"), "{}", ui.out);
    assert!(ui.out.contains("openwraith travel --close"), "{}", ui.out);

    // A reset notification right after the open: nothing more is sent, not
    // even the close; the output names `travel --close`.
    let mut sim = c06_sim();
    let open = travel_open_packet(&sim);
    sim.reset_after_packets(open);
    sim.allow_travel_left_open();
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    let said = match run(&["travel", "--duration", "5s"], &mut connector, &mut ui) {
        Ok(exit) => {
            assert_eq!(exit, Exit::Failure, "{}", ui.out);
            format!("{}{}", ui.out, ui.err)
        }
        Err(e) => format!("{e:#}\n{}{}", ui.out, ui.err),
    };
    assert!(said.contains("reset"), "{said}");
    assert!(said.contains("openwraith travel --close"), "{said}");
    let sim = connector.sim.borrow();
    assert_eq!(sim.travel_closes(), 0);
    sim.assert_clean();
}

#[test]
fn set_socd_names_an_unexpected_ack_and_a_switch_on_the_keyboard() {
    // The keyboard acknowledges the power packet (never seen on hardware).
    let fx = Fixture::new();
    let mut sim = c06_sim();
    sim.set_socd_power_acked(true);
    let mut connector = SimConnector::new(sim);
    let ui = run_yes(&["set", "socd", "--on"], &mut connector, &fx, Exit::Success);
    assert!(
        ui.out
            .contains("the keyboard acknowledged packet 1, which it is not seen to acknowledge"),
        "{}",
        ui.out
    );
    assert!(!ui.err.contains("another program"), "{}", ui.err);

    // Fn + Right Shift right after the power packet: the write is not
    // repeated over the user's switch.
    let mut copy = c06_sim();
    let mut probe = SimConnector::new(copy.clone());
    run_yes(&["set", "socd", "--on"], &mut probe, &fx, Exit::Success);
    copy = probe.sim.borrow().clone();
    let power_at = copy
        .written()
        .iter()
        .zip(copy.sent_times())
        .find(|(p, _)| p.is_write() && p.as_bytes()[..2] == [0x24, 0x03])
        .map(|(_, t)| *t)
        .expect("the power packet");
    let mut sim = c06_sim();
    sim.press_socd_hotkey_at(power_at + Duration::from_millis(10));
    let mut connector = SimConnector::new(sim);
    let ui = run_yes(&["set", "socd", "--on"], &mut connector, &fx, Exit::Failure);
    assert!(ui.out.contains("was changed on the keyboard"), "{}", ui.out);
    let bursts = sent(&connector.sim.borrow(), 0, "24 03").len();
    assert_eq!(bursts, 1);
}

#[test]
fn a_slot_without_a_key_needs_its_toggled_or_tap_key_before_anything_is_opened() {
    for (args, expected) in [
        (
            &["set", "advanced", "--slot", "3", "toggle", "--yes"][..],
            "give the toggled key with --key",
        ),
        (
            &[
                "set", "advanced", "--slot", "3", "mod-tap", "--hold", "L-Ctrl", "--yes",
            ],
            "give the tap key with --tap",
        ),
    ] {
        let mut connector = SimConnector::new(c06_sim());
        let mut ui = TestUi::answering(&[]);
        let err = run(args, &mut connector, &mut ui).expect_err("usage error");
        assert!(is_usage_error(&err), "{args:?}: {err}");
        assert!(err.to_string().contains(expected), "{err}");
        assert_eq!((connector.read_sessions, connector.write_sessions), (0, 0));
    }
}

#[test]
fn conflicting_v02_flags_are_usage_errors() {
    use clap::Parser;
    use clap::error::ErrorKind;
    let parse = |args: &[&str]| {
        crate::Cli::try_parse_from(std::iter::once("openwraith").chain(args.iter().copied()))
    };
    for (args, kind) in [
        (
            &["set", "macro", "Caps", "--unbind", "--steps", "A+, A-"][..],
            ErrorKind::ArgumentConflict,
        ),
        (
            &["set", "socd", "--on", "--off"],
            ErrorKind::ArgumentConflict,
        ),
        (
            &["travel", "--close", "--duration", "5s"],
            ErrorKind::ArgumentConflict,
        ),
        (
            &["set", "advanced", "H", "toggle", "--keep-slot"],
            ErrorKind::MissingRequiredArgument,
        ),
    ] {
        let err = parse(args).expect_err("refused");
        assert_eq!(err.kind(), kind, "{args:?}: {err}");
    }
    // --keep-slot without --unbind: a usage error before the keyboard is
    // opened (it would otherwise silently keep orphaned slots).
    let args = [
        "set",
        "macro",
        "Caps",
        "--steps",
        "A+, A-",
        "--keep-slot",
        "--yes",
    ];
    let mut connector = SimConnector::new(c06_sim());
    let mut ui = TestUi::answering(&[]);
    let err = run(&args, &mut connector, &mut ui).expect_err("usage error");
    assert!(is_usage_error(&err), "{args:?}: {err}");
    assert!(err.to_string().contains("--keep-slot"), "{err}");
    assert_eq!((connector.read_sessions, connector.write_sessions), (0, 0));
}
