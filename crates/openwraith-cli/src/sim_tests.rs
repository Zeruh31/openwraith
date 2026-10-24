//! The commands that may write, run exactly as the binary runs them (parsed
//! from command-line arguments) but against the device simulator: the
//! connector opens sessions over `DeviceSim`, the console is scripted. No
//! HID device is enumerated or opened.

use std::path::{Path, PathBuf};

use clap::Parser;
use openwraith_hid::{Backup, DeviceSim, SimTable, WriteError};
use openwraith_protocol::{KeyPos, ProfileId, ReadRequest, Report};

use crate::commands::Exit;
use crate::test_support::{SimConnector, TestUi, c06_backup, sim_with_profiles};
use crate::{Cli, globals, is_usage_error, run_command};

/// W in c06: 1.00 mm, Rapid Trigger 0.30 mm.
const W_C06: [u8; 9] = [0x0c, 0x32, 0x32, 0x0f, 0x0f, 0, 0, 0, 0];
/// Factory trigger record: 1.70 mm, Rapid Trigger off.
const FACTORY: [u8; 9] = [0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0];

fn pos(row: u8, col: u8) -> KeyPos {
    KeyPos::new(row, col).expect("matrix position")
}

fn profile(n: u8) -> ProfileId {
    ProfileId::new(n).expect("profile 1-4")
}

fn w() -> KeyPos {
    pos(2, 2)
}

fn q() -> KeyPos {
    pos(2, 1)
}

/// A temporary directory with the backup files and safety backups of a test.
struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Fixture {
            dir: tempfile::tempdir().expect("temporary directory"),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn safety_dir(&self) -> PathBuf {
        self.path("safety")
    }

    fn save(&self, backup: &Backup, name: &str) -> PathBuf {
        let path = self.path(&format!("{name}.owbackup.json"));
        backup.save(&path, false).expect("saved");
        path
    }

    /// The safety backups saved so far.
    fn safety_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(self.safety_dir())
            .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default();
        files.sort();
        files
    }
}

fn text(path: &Path) -> &str {
    path.to_str().expect("utf-8 temporary path")
}

/// Parses `args` like the binary and runs the command against the simulator.
fn run(args: &[&str], connector: &mut SimConnector, ui: &mut TestUi) -> anyhow::Result<Exit> {
    let cli = Cli::try_parse_from(std::iter::once("openwraith").chain(args.iter().copied()))
        .unwrap_or_else(|e| panic!("{args:?} does not parse: {e}"));
    run_command(&cli.command, &globals(&cli), connector, ui)
}

/// The c06 main-light record with the mode set to Static and the colour to
/// blue.
fn static_blue_light() -> Vec<u8> {
    let mut light = c06_backup()
        .response(ReadRequest::MainLight)
        .expect("c06 light")
        .payload();
    light[0] = 0x00;
    light[3..6].copy_from_slice(&[0, 0, 0xff]);
    light
}

/// The c06 keyboard after two changes: W back to factory values and the
/// main light set to static blue.
fn changed_sim() -> DeviceSim {
    let mut sim = DeviceSim::from_backup(&c06_backup());
    sim.set_trigger(profile(1), w(), FACTORY);
    sim.set_table(profile(1), SimTable::MainLight, static_blue_light());
    sim
}

/// Every packet the simulator received is a documented read.
fn assert_reads_only(sim: &DeviceSim) {
    assert_eq!(sim.write_packets().count(), 0, "a write was sent");
    for packet in sim.written() {
        assert!(
            ReadRequest::from_packet(packet.as_bytes()).is_some(),
            "not a read: {:?}",
            packet.as_bytes()
        );
    }
    sim.assert_clean();
}

/// The write packets the simulator received, as spaced hex.
fn writes(sim: &DeviceSim) -> Vec<String> {
    sim.write_packets().map(|p| p.summary()).collect()
}

#[test]
fn dry_run_against_the_keyboard_reads_only_and_shows_the_plan() {
    let fx = Fixture::new();
    let file = fx.save(&c06_backup(), "c06");
    let mut connector = SimConnector::new(changed_sim());
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &["restore", text(&file), "--dry-run"],
        &mut connector,
        &mut ui,
    )
    .expect("dry run");
    // Shown with `cargo test -- --nocapture`: the dry-run output against
    // the simulator.
    println!(
        "--- openwraith restore <c06 backup> --dry-run ---\n{}",
        ui.out
    );
    println!("--- stderr ---\n{}", ui.err);
    assert_eq!(exit, Exit::Success);
    let out = &ui.out;
    assert!(
        out.starts_with("Restore plan (dry run: nothing is written)\n"),
        "{out}"
    );
    assert!(out.contains("trigger of W (r2c2)"), "{out}");
    assert!(
        out.contains("1.70 mm, RT off -> 1.00 mm, RT 0.30 mm"),
        "{out}"
    );
    assert!(
        out.contains("main light with mode change (17 00, 07 00)"),
        "{out}"
    );
    assert!(
        out.contains(
            "Static, colour #0000ff, brightness 4, speed 3 -> Custom (per-key), colour #ff0000"
        ),
        "{out}"
    );
    assert!(
        out.contains("    dead band, base keymap, Fn keymap,"),
        "{out}"
    );
    assert!(
        out.contains("Total: 2 writes (3 packets) in profile 1."),
        "{out}"
    );
    // A dry run opens a read-only session and asks nothing.
    assert_eq!((connector.read_sessions, connector.write_sessions), (1, 0));
    assert!(ui.questions.is_empty());
    assert!(!ui.cancel_enabled);
    assert_reads_only(&connector.sim.borrow());
    assert!(fx.safety_files().is_empty());
}

#[test]
fn dry_run_lists_packets_and_profiles_it_did_not_compare() {
    let fx = Fixture::new();
    // An all-profile backup of a keyboard whose profiles differ.
    let mut connector = SimConnector::new(sim_with_profiles());
    let all = fx.path("all.owbackup.json");
    let mut ui = TestUi::answering(&[]);
    run(
        &["backup", "--all-profiles", "-o", text(&all)],
        &mut connector,
        &mut ui,
    )
    .expect("all-profile backup");
    connector
        .sim
        .borrow_mut()
        .set_trigger(profile(1), w(), FACTORY);

    let mut ui = TestUi::answering(&[]);
    run(
        &["restore", text(&all), "--dry-run", "--packets"],
        &mut connector,
        &mut ui,
    )
    .expect("dry run");
    let out = &ui.out;
    assert!(
        out.contains("21 00 00 00 18 0c 00 00 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 32 32 0f 0f"),
        "{out}"
    );
    assert!(
        out.contains("Profiles 2, 3 and 4: not compared. A dry run reads only the active profile"),
        "{out}"
    );
}

#[test]
fn restore_writes_verifies_and_can_be_undone() {
    let fx = Fixture::new();
    let file = fx.save(&c06_backup(), "c06");
    let safety = fx.safety_dir();
    let mut connector = SimConnector::new(changed_sim());
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &[
            "restore",
            text(&file),
            "--yes",
            "--safety-dir",
            text(&safety),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("restore");
    let out = &ui.out;
    assert_eq!(exit, Exit::Success, "{out}\n{}", ui.err);
    assert!(ui.cancel_enabled, "Ctrl+C is handled while writing");
    assert!(
        out.contains("Done: 2 tables of 2 written and verified by reading them back."),
        "{out}"
    );
    // Only the active profile was written: the persistence check switched
    // away (after the keyboard had time to save) and back, and read both
    // tables again.
    assert!(
        out.contains("written and verified (1 packet), persisted"),
        "{out}"
    );
    assert!(
        out.contains(
            "Persistence check: profile 1 selected again after switching away; 2 written tables \
             still hold what was written (the keyboard saved them)."
        ),
        "{out}"
    );
    assert!(
        ui.err
            .contains("Waiting for the keyboard to save profile 1 (2 s)..."),
        "{}",
        ui.err
    );
    assert!(out.contains("Active profile: 1 (as before)."), "{out}");
    assert!(out.contains("To undo:       openwraith restore "), "{out}");
    assert!(ui.err.contains("Safety backup saved: "), "{}", ui.err);
    // The status push after the music-rhythm leave is ours, not another
    // program's.
    assert!(!ui.err.contains("another program"), "{}", ui.err);
    assert!(
        ui.err.contains("Close every wraith.software tab"),
        "{}",
        ui.err
    );
    {
        let sim = connector.sim.borrow();
        assert_eq!(sim.trigger(profile(1), w()), Some(W_C06));
        let c06_light = c06_backup()
            .response(ReadRequest::MainLight)
            .expect("light")
            .payload();
        assert_eq!(
            sim.table(profile(1), SimTable::MainLight),
            Some(&c06_light[..])
        );
        assert_eq!(sim.persisted_trigger(profile(1), w()), Some(W_C06));
        assert_eq!(sim.discarded_edits(), 0);
        // Trigger first, then the music-rhythm leave right before the
        // mode-changing light write; then the persistence check's switches.
        let sent = writes(&sim);
        assert_eq!(sent.len(), 5, "{sent:?}");
        assert!(sent[0].starts_with("21 00 00 00 18 0c"), "{sent:?}");
        assert_eq!(sent[1], "17 00 00 00 02");
        assert!(sent[2].starts_with("07 00 00 00 0e 0a"), "{sent:?}");
        assert_eq!(sent[3..], ["21 00 00 00 00 12 02", "21 00 00 00 00 12 01"]);
        sim.assert_clean();
    }

    // The undo command restores the safety backup: back to the changed state.
    let saved = fx.safety_files();
    assert_eq!(saved.len(), 1, "{saved:?}");
    let name = saved[0].file_name().and_then(|n| n.to_str()).unwrap_or("");
    assert!(name.contains("-restore"), "{name}");
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &[
            "restore",
            text(&saved[0]),
            "--yes",
            "--safety-dir",
            text(&safety),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("undo");
    assert_eq!(exit, Exit::Success, "{}", ui.out);
    let sim = connector.sim.borrow();
    assert_eq!(sim.trigger(profile(1), w()), Some(FACTORY));
    assert_eq!(
        sim.table(profile(1), SimTable::MainLight),
        Some(&static_blue_light()[..])
    );
    sim.assert_clean();
}

#[test]
fn restore_asks_and_a_no_writes_nothing() {
    let fx = Fixture::new();
    let file = fx.save(&c06_backup(), "c06");
    let safety = fx.safety_dir();
    let mut connector = SimConnector::new(changed_sim());
    let mut ui = TestUi::answering(&[false]);
    let exit = run(
        &["restore", text(&file), "--safety-dir", text(&safety)],
        &mut connector,
        &mut ui,
    )
    .expect("runs");
    assert_eq!(exit, Exit::Failure);
    assert_eq!(
        ui.questions,
        ["Write 2 table(s) to the keyboard (profile 1: 3 packets)?"]
    );
    assert!(
        ui.out.contains("Not confirmed: nothing was written."),
        "{}",
        ui.out
    );
    assert_reads_only(&connector.sim.borrow());
    // The safety backup was made before asking, and is kept.
    assert_eq!(fx.safety_files().len(), 1);

    // A "yes" answer writes.
    let mut ui = TestUi::answering(&[true]);
    let exit = run(
        &[
            "restore",
            text(&file),
            "--safety-dir",
            text(&safety),
            "--json",
        ],
        &mut connector,
        &mut ui,
    )
    .expect("runs");
    assert_eq!(exit, Exit::Success);
    // The plan was shown (on stderr) before the question, although the JSON
    // document comes only after the answer.
    assert_eq!(ui.questions.len(), 1);
    assert!(
        ui.notes_at_question[0].contains("trigger of W (r2c2)")
            && ui.notes_at_question[0].contains("main light"),
        "{}",
        ui.notes_at_question[0]
    );
    let json: serde_json::Value = serde_json::from_str(&ui.out).expect("JSON output");
    assert_eq!(json["confirmed"], true);
    assert_eq!(json["dry_run"], false);
    assert_eq!(json["report"]["outcome"], "completed");
    assert_eq!(json["report"]["consent"]["origin"], "interactive_prompt");
    assert_eq!(json["plan"]["profiles"][0]["profile"], 1);
    assert!(json["safety_backup"]["path"].is_string(), "{json}");
    assert!(json["note"].is_string());
    assert_eq!(connector.sim.borrow().trigger(profile(1), w()), Some(W_C06));
}

#[test]
fn nothing_to_restore_asks_nothing() {
    let fx = Fixture::new();
    let file = fx.save(&c06_backup(), "c06");
    let mut connector = SimConnector::new(DeviceSim::from_backup(&c06_backup()));
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &[
            "restore",
            text(&file),
            "--safety-dir",
            text(&fx.safety_dir()),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("runs");
    assert_eq!(exit, Exit::Success);
    assert!(ui.questions.is_empty());
    assert!(
        ui.out
            .contains("Nothing to write: the keyboard already holds the backup's values."),
        "{}",
        ui.out
    );
    assert_reads_only(&connector.sim.borrow());
}

#[test]
fn writing_commands_refuse_without_a_terminal_or_yes() {
    let fx = Fixture::new();
    let file = fx.save(&c06_backup(), "c06");
    let safety = fx.safety_dir();
    for args in [
        vec!["restore", text(&file), "--safety-dir", text(&safety)],
        vec!["set", "trigger", "W", "--actuation", "1.20"],
        vec!["set", "light", "--brightness", "2"],
    ] {
        let mut connector = SimConnector::new(changed_sim());
        let mut ui = TestUi::non_interactive();
        let err = run(&args, &mut connector, &mut ui).expect_err("refused");
        assert!(is_usage_error(&err), "{args:?}: {err}");
        assert!(err.to_string().contains("add --yes"), "{err}");
        // Refused before the keyboard was even opened.
        assert_eq!(
            (connector.read_sessions, connector.write_sessions),
            (0, 0),
            "{args:?}"
        );
        assert!(connector.sim.borrow().written().is_empty());
    }

    // A profile the backup does not hold: refused before opening, too.
    let mut connector = SimConnector::new(changed_sim());
    let mut ui = TestUi::answering(&[]);
    let err = run(
        &["restore", text(&file), "--profile", "3", "--yes"],
        &mut connector,
        &mut ui,
    )
    .expect_err("refused");
    assert!(
        err.to_string().contains("profile 3 is not in the backup"),
        "{err}"
    );
    assert_eq!((connector.read_sessions, connector.write_sessions), (0, 0));
}

#[test]
fn all_profile_backup_then_restore_of_one_profile() {
    let fx = Fixture::new();
    let safety = fx.safety_dir();
    let all = fx.path("all.owbackup.json");
    let mut connector = SimConnector::new(sim_with_profiles());
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &["backup", "--all-profiles", "-o", text(&all)],
        &mut connector,
        &mut ui,
    )
    .expect("all-profile backup");
    assert_eq!(exit, Exit::Success);
    assert!(
        ui.err
            .contains("switches profiles (the lighting changes briefly) and returns to profile 1"),
        "{}",
        ui.err
    );
    // Our own profile pushes are not another program's.
    assert!(!ui.err.contains("another program"), "{}", ui.err);
    assert!(
        ui.out
            .contains("1, 2, 3, 4 (profile 1 was active and is summarised below"),
        "{}",
        ui.out
    );
    let backup = Backup::load(&all).expect("loads");
    assert!(backup.is_all_profiles());
    assert_eq!(backup.profile_numbers(), [1, 2, 3, 4]);
    {
        let sim = connector.sim.borrow();
        // Only profile selects were written, ending on the original profile.
        let selects: Vec<String> = writes(&sim);
        assert_eq!(
            selects,
            [
                "21 00 00 00 00 12 02",
                "21 00 00 00 00 12 03",
                "21 00 00 00 00 12 04",
                "21 00 00 00 00 12 01"
            ]
        );
        assert_eq!(sim.active_profile(), 1);
        sim.assert_clean();
    }

    // Hardware test H7 (docs/HARDWARE_TESTS.md): change Q in profile 2, go back to profile 1, restore
    // profile 2 only.
    for args in [
        vec!["profile", "select", "2"],
        vec![
            "set",
            "trigger",
            "Q",
            "--actuation",
            "2.00",
            "--yes",
            "--safety-dir",
            text(&safety),
        ],
        vec!["profile", "select", "1"],
    ] {
        let mut ui = TestUi::answering(&[]);
        let exit = run(&args, &mut connector, &mut ui).expect("runs");
        assert_eq!(exit, Exit::Success, "{args:?}: {}", ui.out);
    }
    assert_eq!(
        connector.sim.borrow().trigger(profile(2), q()),
        Some([0x00, 100, 100, 0x01, 0x01, 0, 0, 0, 0])
    );
    assert_eq!(connector.sim.borrow().active_profile(), 1);

    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &[
            "restore",
            text(&all),
            "--profile",
            "2",
            "--yes",
            "--safety-dir",
            text(&safety),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("restore profile 2");
    // Shown with `cargo test -- --nocapture`: the restore that lost its
    // write on the real keyboard (PROTOCOL.md 4.5.3), now with the commit
    // settle and the persistence check.
    println!(
        "--- openwraith restore all.owbackup.json --profile 2 --yes ---\n{}",
        ui.out
    );
    println!("--- stderr ---\n{}", ui.err);
    assert_eq!(exit, Exit::Success, "{}\n{}", ui.out, ui.err);
    assert!(ui.out.contains("Profile 2\n"), "{}", ui.out);
    assert!(ui.out.contains("trigger of Q (r2c1)"), "{}", ui.out);
    // The keyboard got 2 s to save the write before the switch away, and the
    // write was read again after switching back.
    assert!(
        ui.err
            .contains("Waiting for the keyboard to save profile 2 (2 s)..."),
        "{}",
        ui.err
    );
    assert!(
        ui.out
            .contains("written and verified (1 packet), persisted"),
        "{}",
        ui.out
    );
    assert!(
        ui.out.contains(
            "Persistence check: profile 2 selected again after switching away; 1 written table \
             still holds what was written (the keyboard saved it)."
        ),
        "{}",
        ui.out
    );
    assert!(
        ui.out.contains("Active profile: 1 (as before)."),
        "{}",
        ui.out
    );
    let sim = connector.sim.borrow();
    assert_eq!(sim.trigger(profile(2), q()), Some(FACTORY));
    assert_eq!(sim.persisted_trigger(profile(2), q()), Some(FACTORY));
    assert_eq!(sim.trigger(profile(1), w()), Some(W_C06));
    assert_eq!(sim.active_profile(), 1);
    // No unsaved edit was ever dropped, by any of the commands above.
    assert_eq!(sim.discarded_edits(), 0);
    sim.assert_clean();
}

#[test]
fn set_trigger_writes_one_key_per_packet() {
    let fx = Fixture::new();
    let safety = fx.safety_dir();
    let mut connector = SimConnector::new(DeviceSim::from_backup(&c06_backup()));
    let mut ui = TestUi::answering(&[true]);
    let exit = run(
        &[
            "set",
            "trigger",
            "W",
            "q",
            "--actuation",
            "1.20",
            "--safety-dir",
            text(&safety),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("set trigger");
    assert_eq!(exit, Exit::Success, "{}\n{}", ui.out, ui.err);
    let out = &ui.out;
    assert!(
        out.contains("Trigger changes in profile 1 (one packet per key):"),
        "{out}"
    );
    assert!(
        out.contains("1.00 mm, RT 0.30 mm  1.20 mm, RT 0.30 mm"),
        "{out}"
    );
    assert!(
        out.contains("1.70 mm, RT off      1.20 mm, RT off"),
        "{out}"
    );
    assert!(
        out.contains("Done: 2 tables of 2 written and verified"),
        "{out}"
    );
    // The command ends only after the keyboard had time to save.
    assert!(
        ui.err
            .contains("Waiting for the keyboard to save profile 1 (2 s)..."),
        "{}",
        ui.err
    );
    assert!(
        out.contains("Waited 2 s after the last write so that the keyboard could save it"),
        "{out}"
    );
    assert!(!connector.sim.borrow().has_unsaved_edits());
    assert_eq!(
        ui.questions,
        ["Write the new trigger settings of 2 key(s) to profile 1?"]
    );
    {
        let sim = connector.sim.borrow();
        assert_eq!(
            sim.trigger(profile(1), w()),
            Some([0x0c, 60, 60, 0x0f, 0x0f, 0, 0, 0, 0])
        );
        assert_eq!(
            sim.trigger(profile(1), q()),
            Some([0x00, 60, 60, 0x01, 0x01, 0, 0, 0, 0])
        );
        for packet in sim.write_packets() {
            let bits: u32 = packet.as_bytes()[6..28]
                .iter()
                .map(|b| b.count_ones())
                .sum();
            assert_eq!(bits, 1, "{packet:?}");
        }
        sim.assert_clean();
    }
    let saved = fx.safety_files();
    assert_eq!(saved.len(), 1);
    assert!(text(&saved[0]).contains("-set-trigger"), "{saved:?}");

    // Separate press / release, then Rapid Trigger off.
    for (args, expected) in [
        (
            vec!["set", "trigger", "r2c2", "--rapid-trigger", "0.30/0.50"],
            [0x0d, 60, 60, 0x0f, 0x19, 0, 0, 0, 0],
        ),
        (
            vec!["set", "trigger", "#46", "--no-rapid-trigger"],
            [0x00, 60, 60, 0x0f, 0x19, 0, 0, 0, 0],
        ),
    ] {
        let mut args = args.clone();
        args.extend(["--yes", "--safety-dir", text(&safety)]);
        let mut ui = TestUi::answering(&[]);
        let exit = run(&args, &mut connector, &mut ui).expect("set trigger");
        assert_eq!(exit, Exit::Success, "{args:?}: {}", ui.out);
        assert_eq!(
            connector.sim.borrow().trigger(profile(1), w()),
            Some(expected)
        );
    }

    // Already set: nothing is written and no safety backup is made.
    let before = connector.sim.borrow().writes_sent();
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &[
            "set",
            "trigger",
            "W",
            "--no-rapid-trigger",
            "--yes",
            "--safety-dir",
            text(&safety),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("runs");
    assert_eq!(exit, Exit::Success);
    assert!(ui.out.contains("(already set)"), "{}", ui.out);
    assert!(ui.out.contains("Nothing to change"), "{}", ui.out);
    assert_eq!(connector.sim.borrow().writes_sent(), before);
    assert_eq!(fx.safety_files().len(), 3);

    // Out of the keyboard's range (3.40 mm maximum): refused, nothing written.
    let mut ui = TestUi::answering(&[]);
    let err = run(
        &[
            "set",
            "trigger",
            "W",
            "--actuation",
            "3.60",
            "--yes",
            "--safety-dir",
            text(&safety),
        ],
        &mut connector,
        &mut ui,
    )
    .expect_err("out of range");
    assert!(err.to_string().contains("actuation"), "{err}");
    assert_eq!(connector.sim.borrow().writes_sent(), before);
}

#[test]
fn set_light_sends_the_leave_only_on_a_mode_change() {
    let fx = Fixture::new();
    let safety = fx.safety_dir();
    let mut connector = SimConnector::new(DeviceSim::from_backup(&c06_backup()));
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &[
            "set",
            "light",
            "--mode",
            "static",
            "--color",
            "0000ff",
            "--yes",
            "--safety-dir",
            text(&safety),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("set light");
    assert_eq!(exit, Exit::Success, "{}\n{}", ui.out, ui.err);
    assert!(
        ui.out
            .contains("Now  Custom (per-key), colour #ff0000, brightness 4, speed 3"),
        "{}",
        ui.out
    );
    assert!(
        ui.out
            .contains("New  Static, colour #0000ff, brightness 4, speed 3"),
        "{}",
        ui.out
    );
    assert!(ui.out.contains("music-rhythm leave (17 00)"), "{}", ui.out);
    assert!(!ui.err.contains("another program"), "{}", ui.err);
    {
        let sim = connector.sim.borrow();
        let sent = writes(&sim);
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert_eq!(sent[0], "17 00 00 00 02");
        assert!(
            sent[1].starts_with("07 00 00 00 0e 00 04 03 00 00 ff"),
            "{sent:?}"
        );
        let light = sim.table(profile(1), SimTable::MainLight).expect("light");
        assert_eq!(light[..6], [0x00, 0x04, 0x03, 0x00, 0x00, 0xff]);
        sim.assert_clean();
    }

    // Brightness only: no leave.
    let mut ui = TestUi::answering(&[]);
    run(
        &[
            "set",
            "light",
            "--brightness",
            "2",
            "--yes",
            "--safety-dir",
            text(&safety),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("set light");
    let sim = connector.sim.borrow();
    let sent = writes(&sim);
    assert_eq!(sent.len(), 3, "{sent:?}");
    assert!(sent[2].starts_with("07 00 00 00 0e 00 02 03"), "{sent:?}");
    sim.assert_clean();
    drop(sim);

    // Out of range: refused before anything is written.
    let mut ui = TestUi::answering(&[]);
    let before = connector.sim.borrow().writes_sent();
    assert!(
        run(
            &[
                "set",
                "light",
                "--brightness",
                "9",
                "--yes",
                "--safety-dir",
                text(&safety)
            ],
            &mut connector,
            &mut ui,
        )
        .is_err()
    );
    assert_eq!(connector.sim.borrow().writes_sent(), before);
}

#[test]
fn profile_shows_and_selects() {
    let mut connector = SimConnector::new(sim_with_profiles());
    let mut ui = TestUi::non_interactive();
    let exit = run(&["profile"], &mut connector, &mut ui).expect("profile");
    assert_eq!(exit, Exit::Success);
    assert!(
        ui.out.contains("Wraith W75 (W75HEARGB), firmware 3.22.07"),
        "{}",
        ui.out
    );
    assert!(
        ui.out.contains("Active profile: 1 (onboard profiles 1-4)"),
        "{}",
        ui.out
    );
    assert_eq!((connector.read_sessions, connector.write_sessions), (1, 0));

    // No confirmation needed: switching is the command's only effect.
    let mut ui = TestUi::non_interactive();
    let exit = run(&["profile", "select", "3"], &mut connector, &mut ui).expect("select");
    assert_eq!(exit, Exit::Success);
    assert!(
        ui.out.contains("Switched from profile 1 to profile 3"),
        "{}",
        ui.out
    );
    assert_eq!(connector.sim.borrow().active_profile(), 3);
    assert_eq!(writes(&connector.sim.borrow()), ["21 00 00 00 00 12 03"]);

    let mut ui = TestUi::non_interactive();
    run(&["profile", "select", "3"], &mut connector, &mut ui).expect("select");
    assert!(
        ui.out
            .contains("Profile 3 is already active; nothing was sent."),
        "{}",
        ui.out
    );
    assert_eq!(connector.sim.borrow().writes_sent(), 1);
    connector.sim.borrow().assert_clean();
}

#[test]
fn ctrl_c_stops_at_the_next_table_boundary() {
    let fx = Fixture::new();
    let file = fx.save(&c06_backup(), "c06");
    let mut connector = SimConnector::new(changed_sim());
    let mut ui = TestUi::answering(&[]);
    // Ctrl+C while the first table (W's trigger) is being written.
    ui.cancel_at = Some((", packet 1/".to_string(), 1));
    let exit = run(
        &[
            "restore",
            text(&file),
            "--yes",
            "--safety-dir",
            text(&fx.safety_dir()),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("runs");
    assert_eq!(exit, Exit::Failure);
    assert!(
        ui.out
            .contains("Cancelled: 1 table written and verified before stopping"),
        "{}",
        ui.out
    );
    assert!(ui.out.contains("not attempted"), "{}", ui.out);
    let sim = connector.sim.borrow();
    // The first table is complete, the second was never started.
    assert_eq!(sim.trigger(profile(1), w()), Some(W_C06));
    assert_eq!(
        sim.table(profile(1), SimTable::MainLight),
        Some(&static_blue_light()[..])
    );
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

/// Q at 2.00 mm, Rapid Trigger off.
const Q_2_00: [u8; 9] = [0x00, 100, 100, 0x01, 0x01, 0, 0, 0, 0];

/// The virtual time of the last trigger write the simulator received.
fn last_trigger_write_at(sim: &DeviceSim) -> std::time::Duration {
    sim.written()
        .iter()
        .zip(sim.sent_times())
        .filter(|(p, _)| p.is_write() && p.as_bytes()[..5] == [0x21, 0, 0, 0, 0x18])
        .map(|(_, t)| *t)
        .next_back()
        .expect("a trigger write")
}

#[test]
fn ctrl_c_during_the_wait_between_profiles_stops_before_the_next_profile() {
    let fx = Fixture::new();
    let safety = fx.safety_dir();
    let all = fx.path("all.owbackup.json");
    let mut connector = SimConnector::new(sim_with_profiles());
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &["backup", "--all-profiles", "-o", text(&all)],
        &mut connector,
        &mut ui,
    )
    .expect("all-profile backup");
    assert_eq!(exit, Exit::Success);
    // Later Q was changed in profiles 2 and 3 (and saved long ago).
    {
        let mut sim = connector.sim.borrow_mut();
        sim.set_trigger(profile(2), q(), Q_2_00);
        sim.set_trigger(profile(3), q(), Q_2_00);
    }
    let writes_before = connector.sim.borrow().writes_sent();
    let mut ui = TestUi::answering(&[]);
    // Ctrl+C while the command stands still waiting for the keyboard to save
    // profile 2, before it goes on to profile 3.
    ui.cancel_after_note = Some("Waiting for the keyboard to save profile 2".to_string());
    let exit = run(
        &[
            "restore",
            text(&all),
            "--yes",
            "--safety-dir",
            text(&safety),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("runs");
    assert_eq!(exit, Exit::Failure, "{}\n{}", ui.out, ui.err);
    assert!(
        ui.out
            .contains("Cancelled: 1 table written and verified before stopping"),
        "{}",
        ui.out
    );
    assert!(ui.out.contains("not attempted"), "{}", ui.out);
    assert!(
        ui.err
            .contains("Waiting for the keyboard to save profile 2 (2 s)..."),
        "{}",
        ui.err
    );
    let sim = connector.sim.borrow();
    let sent = writes(&sim);
    let sent = &sent[writes_before..];
    let trigger = sent
        .iter()
        .position(|w| w.starts_with("21 00 00 00 18"))
        .expect("the write in profile 2");
    // Profile 2 was selected for the write; after the wait the restore went
    // straight back to profile 1: profile 3 was never selected or written.
    assert_eq!(sent[trigger - 1], "21 00 00 00 00 12 02");
    assert_eq!(sent[trigger + 1..], ["21 00 00 00 00 12 01"]);
    assert_eq!(sim.trigger(profile(3), q()), Some(Q_2_00));
    assert_eq!(sim.persisted_trigger(profile(2), q()), Some(FACTORY));
    assert_eq!(sim.discarded_edits(), 0);
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn a_switch_on_the_keyboard_during_an_edit_is_not_reported_as_done() {
    let args = [
        "set",
        "trigger",
        "q",
        "--actuation",
        "1.20",
        "--yes",
        "--safety-dir",
    ];
    // When the write goes out, from a run on a copy (deterministic).
    let write_at = {
        let fx = Fixture::new();
        let mut connector = SimConnector::new(DeviceSim::from_backup(&c06_backup()));
        let mut ui = TestUi::answering(&[]);
        let mut all: Vec<&str> = args.to_vec();
        let dir = fx.safety_dir();
        all.push(text(&dir));
        assert_eq!(
            run(&all, &mut connector, &mut ui).expect("runs"),
            Exit::Success
        );
        last_trigger_write_at(&connector.sim.borrow())
    };
    let fx = Fixture::new();
    let mut sim = DeviceSim::from_backup(&c06_backup());
    let before = sim.persisted_trigger(profile(1), q());
    // The user presses Fn + 2 300 ms after the write; the keyboard does not
    // report it.
    sim.switch_profile_at(
        write_at + std::time::Duration::from_millis(300),
        profile(2),
        false,
    );
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    let mut all: Vec<&str> = args.to_vec();
    let dir = fx.safety_dir();
    all.push(text(&dir));
    let exit = run(&all, &mut connector, &mut ui).expect("runs");
    println!(
        "--- set trigger q --actuation 1.20 --yes (Fn + 2 during the wait) ---\n{}",
        ui.out
    );
    println!("--- stderr ---\n{}", ui.err);
    assert_eq!(exit, Exit::Failure);
    for expected in [
        "FAILED: the active onboard profile changed to 2 before the keyboard had saved the last \
         write to profile 1",
        "1 table written and verified, but the keyboard may not have saved it",
        "written and verified (1 packet), but MAY BE LOST",
        "Active profile: 2. It was 1 before; switch back with `openwraith profile select 1`.",
        "To undo:",
    ] {
        assert!(ui.out.contains(expected), "{expected}\n---\n{}", ui.out);
    }
    assert!(!ui.out.contains("Something WAS written"), "{}", ui.out);
    assert!(!ui.out.contains("Waited 2 s"), "{}", ui.out);
    // The wait was for the profile that was written.
    assert!(
        ui.err
            .contains("Waiting for the keyboard to save profile 1 (2 s)..."),
        "{}",
        ui.err
    );
    let sim = connector.sim.borrow();
    assert_eq!(sim.persisted_trigger(profile(1), q()), before);
    assert_eq!(sim.discarded_edits(), 1);
    assert_eq!(sim.active_profile(), 2);
    sim.assert_clean();
}

#[test]
fn another_program_is_refused_before_asking() {
    let fx = Fixture::new();
    let mut sim = DeviceSim::from_backup(&c06_backup());
    // A heartbeat reply the vendor site asked for.
    let status: Report = c06_backup()
        .response(ReadRequest::Status)
        .expect("status")
        .reports()[0]
        .clone();
    sim.push_input(status);
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[true]);
    let err = run(
        &[
            "set",
            "trigger",
            "W",
            "--actuation",
            "1.20",
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
            Some(WriteError::OtherProgramActive { .. })
        ),
        "{err}"
    );
    assert!(ui.questions.is_empty());
    assert!(ui.err.contains("warning: another program"), "{}", ui.err);
    assert!(fx.safety_files().is_empty());
    assert_reads_only(&connector.sim.borrow());
}

#[test]
fn a_failed_verification_is_reported_with_what_was_written() {
    let fx = Fixture::new();
    let file = fx.save(&c06_backup(), "c06");
    let mut sim = changed_sim();
    // The light record never takes a write.
    sim.freeze(SimTable::MainLight);
    let mut connector = SimConnector::new(sim);
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &[
            "restore",
            text(&file),
            "--yes",
            "--safety-dir",
            text(&fx.safety_dir()),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("runs");
    assert_eq!(exit, Exit::Failure);
    let out = &ui.out;
    assert!(out.contains("FAILED: "), "{out}");
    assert!(
        out.contains("Something WAS written: 1 table written and verified before the failure."),
        "{out}"
    );
    assert!(
        out.contains("read-back differs at byte offset(s) 0"),
        "{out}"
    );
    assert!(out.contains("To undo:"), "{out}");
    let sim = connector.sim.borrow();
    assert_eq!(sim.trigger(profile(1), w()), Some(W_C06));
    sim.assert_clean();
}

#[test]
fn a_final_check_mismatch_is_reported_with_exit_code_1() {
    let fx = Fixture::new();
    // The backup: dead bands per profile (profile 1 `05 0a`, profile 2
    // `07 0b` on W).
    let mut source = sim_with_profiles();
    for (n, entry) in [(1, [0x05, 0x0a]), (2, [0x07, 0x0b])] {
        let mut band = vec![0u8; 264];
        band[92..94].copy_from_slice(&entry);
        source.set_table(profile(n), SimTable::DeadBand, band);
    }
    let target = {
        let mut session = openwraith_hid::Session::new(&mut source);
        openwraith_hid::read_all_profiles(
            &mut session,
            openwraith_hid::WriteConsent::new(openwraith_hid::ConsentOrigin::CommandLineFlag),
            &openwraith_hid::ProfilesOptions::default(),
            |_| std::ops::ControlFlow::Continue(()),
        )
        .expect("all-profile backup")
    };
    let file = fx.save(&target, "all");
    // The keyboard holds the same settings, but shares one dead band
    // between all profiles (holding profile 1's).
    let keyboard = DeviceSim::from_backup_with(
        &target,
        openwraith_hid::ScopeConfig {
            dead_band: openwraith_hid::Scope::Global,
            ..openwraith_hid::ScopeConfig::default()
        },
    );
    let mut connector = SimConnector::new(keyboard);
    let mut ui = TestUi::answering(&[]);
    let exit = run(
        &[
            "restore",
            text(&file),
            "--yes",
            "--safety-dir",
            text(&fx.safety_dir()),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("runs");
    assert_eq!(exit, Exit::Failure);
    let out = &ui.out;
    assert!(out.contains("FAILED: "), "{out}");
    assert!(out.contains("Final check: profiles 1, 2, 3"), "{out}");
    assert!(out.contains("no longer hold the backup's values"), "{out}");
    assert!(
        out.contains("profile 1: dead band differs from the backup"),
        "{out}"
    );
    assert!(out.contains("To undo:"), "{out}");
    let sim = connector.sim.borrow();
    assert_eq!(sim.active_profile(), 1);
    sim.assert_clean();
}

#[test]
fn offline_plan_against_another_backup_needs_no_keyboard() {
    let fx = Fixture::new();
    let target = fx.save(&c06_backup(), "c06");
    let current = {
        let mut session = openwraith_hid::Session::new(changed_sim());
        openwraith_hid::read_all(&mut session).expect("backup")
    };
    let against = fx.save(&current, "changed");
    let mut connector = SimConnector::new(DeviceSim::from_backup(&c06_backup()));
    let mut ui = TestUi::non_interactive();
    let exit = run(
        &[
            "restore",
            text(&target),
            "--dry-run",
            "--against",
            text(&against),
        ],
        &mut connector,
        &mut ui,
    )
    .expect("offline plan");
    assert_eq!(exit, Exit::Success);
    assert!(ui.out.contains("trigger of W (r2c2)"), "{}", ui.out);
    assert_eq!((connector.read_sessions, connector.write_sessions), (0, 0));
}
