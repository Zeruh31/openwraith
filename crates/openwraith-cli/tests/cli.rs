//! Runs the `openwraith` binary on backup files replayed from the USB
//! captures (and on all-profile backups made with the device simulator).
//! Only `--from` paths, offline `restore --dry-run --against` plans, help and
//! argument errors are exercised: these tests never enumerate or open a HID
//! device. The commands that write are tested against the simulator by the
//! CLI's unit tests.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, UNIX_EPOCH};

use openwraith_hid::{
    Backup, BackupEntry, BackupOptions, MockTransport, Session, ToolInfo, read_all_with,
};
use openwraith_protocol::{KeyPos, REPORT_LEN, RawResponse, ReadRequest, Report};

fn openwraith(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_openwraith"))
        .args(args)
        .output()
        .expect("runs the binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A backup replayed from capture c06.
fn c06_backup() -> Backup {
    let capture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../captures/capture_06_powercycle_W_persisted.json");
    let mock = MockTransport::from_capture_file(capture).expect("capture loads");
    let options = BackupOptions {
        tool: ToolInfo {
            name: "openwraith-cli-tests".into(),
            version: "0".into(),
        },
        created_at: Some(UNIX_EPOCH + Duration::from_secs(1_792_827_000)),
    };
    read_all_with(&mut Session::new(mock), &options, |_| {}).expect("backup")
}

/// A fresh temporary backup file path.
fn temp_backup_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "openwraith-cli-test-{}-{name}.owbackup.json",
        std::process::id()
    ))
}

/// Writes a backup replayed from capture c06 to a fresh temporary file.
fn c06_backup_file(name: &str) -> PathBuf {
    let path = temp_backup_path(name);
    c06_backup().save(&path, true).expect("saved");
    path
}

/// Whether `text` contains a control character other than a newline.
fn has_control_characters(text: &str) -> bool {
    text.chars().any(|c| c.is_control() && c != '\n')
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("utf-8 temp path")
}

#[test]
fn show_all_from_backup() {
    let file = c06_backup_file("show-all");
    let out = openwraith(&["show", "all", "--from", path_str(&file)]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("Wraith W75 (W75HEARGB)"), "{text}");
    assert!(text.contains("Custom (per-key) (0x0a)"), "{text}");
    // Unverified meanings are marked, with one legend at the end.
    assert!(text.contains("Active table*: Custom 1"), "{text}");
    assert!(
        text.trim_end()
            .ends_with("not yet confirmed on the wire; see docs/PROTOCOL.md"),
        "{text}"
    );
    assert!(text.contains("Rapid Trigger on 1 key(s): W."), "{text}");
    assert!(text.contains("Base layer: no remapped keys"), "{text}");
    assert!(text.contains("0 of 10 slots used"), "{text}");
    // Nothing about reading a keyboard: the file was the only source.
    assert!(!stderr(&out).contains("Reading the keyboard"));
    std::fs::remove_file(file).ok();
}

#[test]
fn show_triggers_json_from_backup() {
    let file = c06_backup_file("triggers-json");
    let out = openwraith(&["show", "triggers", "--json", "--from", path_str(&file)]);
    assert!(out.status.success(), "{}", stderr(&out));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let keys = json["keys"].as_array().expect("keys");
    assert_eq!(keys.len(), 82);
    let w = keys.iter().find(|k| k["name"] == "W").expect("W");
    assert_eq!(w["index"], 46);
    assert_eq!(w["mode"], "rapid_trigger");
    assert_eq!(w["actuation_mm"], 1.0);
    assert_eq!(w["rt_press_mm"], 0.3);
    assert_eq!(w["rt_release_mm"], 0.3);
    assert_eq!(json["active_profile"], 1);
    let esc = keys.iter().find(|k| k["name"] == "Esc").expect("Esc");
    assert_eq!(esc["actuation_mm"], 1.7);
    assert_eq!(esc["rapid_trigger"], false);
    // Every JSON output explains the confidence of decoded values.
    assert!(
        json["note"]
            .as_str()
            .is_some_and(|n| n.contains("raw bytes are authoritative")),
        "{json}"
    );
    assert!(json.get("undecoded").is_none(), "{json}");
    std::fs::remove_file(file).ok();
}

#[test]
fn undecodable_replies_are_reported_not_dropped() {
    let mut backup = c06_backup();
    // Esc: header and row/col echo intact, but only 3 bytes declared.
    let esc = ReadRequest::KeyTrigger(KeyPos::new(0, 0).expect("r0c0"));
    let mut bytes = [0u8; REPORT_LEN];
    bytes[..9].copy_from_slice(&[0x21, 0, 0, 0, 0x03, 0x05, 0x00, 0x55, 0x55]);
    let raw = RawResponse::from_reports(esc, [Report::new(bytes)]).expect("valid reply");
    let entry = backup
        .entries
        .iter_mut()
        .find(|e| e.request() == esc)
        .expect("Esc entry");
    *entry = BackupEntry::new(raw);
    let file = temp_backup_path("undecoded");
    backup.save(&file, true).expect("saved");

    let out = openwraith(&["show", "triggers", "--from", path_str(&file)]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("82 keys (1 not decoded, shown as ?)"),
        "{text}"
    );
    assert!(
        text.contains("Warning: 1 reply could not be decoded"),
        "{text}"
    );
    assert!(text.contains("key trigger r0c0 (21/05)"), "{text}");

    let out = openwraith(&["show", "triggers", "--json", "--from", path_str(&file)]);
    assert!(out.status.success(), "{}", stderr(&out));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(json["keys"].as_array().map(Vec::len), Some(82));
    let esc_row = &json["keys"][0];
    assert_eq!(esc_row["name"], "Esc");
    assert!(esc_row["actuation_mm"].is_null(), "{esc_row}");
    assert_eq!(esc_row["raw"], "050055");
    let undecoded = json["undecoded"].as_array().expect("undecoded list");
    assert_eq!(undecoded.len(), 1);
    assert_eq!(undecoded[0]["description"], "key trigger r0c0 (21/05)");

    for args in [vec!["show", "all"], vec!["info"], vec!["show", "light"]] {
        let mut args = args.clone();
        args.extend(["--from", path_str(&file)]);
        let out = openwraith(&args);
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        assert!(
            stdout(&out).contains("could not be decoded"),
            "{args:?}: {}",
            stdout(&out)
        );
    }
    std::fs::remove_file(file).ok();
}

#[test]
fn edited_headers_and_escape_sequences_do_not_reach_the_output() {
    let backup = c06_backup();
    let mut value = serde_json::to_value(&backup).expect("serialises");
    // A hand-edited header that contradicts the raw replies.
    value["device"]["firmware"] = "9.99.99\u{1b}[5m".into();
    value["device"]["info"] = "\u{1b}]0;pwned title\u{7}\u{1b}[2J\u{1b}[31mFAKE".into();
    value["active_profile"] = 3.into();
    // Kept from the file: the USB identity.
    value["device"]["usb"]["product"] = "W75\u{1b}[2J".into();
    // A raw device-info reply whose build-date chunk holds escape sequences.
    let mut chunk1 = [0u8; REPORT_LEN];
    chunk1[..5].copy_from_slice(&[0x0D, 0x00, 0x00, 0x01, 0x32]);
    let payload = b"\x1b]0;pwned\x07\x1b[2J\x9b31m";
    chunk1[5..5 + payload.len()].copy_from_slice(payload);
    let info_entry = value["entries"]
        .as_array_mut()
        .expect("entries")
        .iter_mut()
        .find(|e| e["raw"]["request"] == "device_info")
        .expect("device info entry");
    info_entry["raw"]["reports"][1] = Report::new(chunk1).to_hex().into();
    let file = temp_backup_path("crafted");
    std::fs::write(&file, serde_json::to_string_pretty(&value).expect("json")).expect("write");

    let out = openwraith(&["info", "--from", path_str(&file)]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(!has_control_characters(&text), "{text:?}");
    // The raw replies win over the edited header.
    assert!(text.contains("3.22.07"), "{text}");
    assert!(!text.contains("9.99.99"), "{text}");
    assert!(!text.contains("FAKE"), "{text}");
    assert!(text.contains("1 (onboard profiles 1-4)"), "{text}");

    let out = openwraith(&["show", "triggers", "--from", path_str(&file)]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).starts_with("Triggers (active profile 1)"));

    for args in [vec!["info", "--json"], vec!["show", "all"]] {
        let mut args = args.clone();
        args.extend(["--from", path_str(&file)]);
        let out = openwraith(&args);
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        assert!(!has_control_characters(&stdout(&out)), "{args:?}");
    }
    std::fs::remove_file(file).ok();
}

#[test]
fn info_json_from_backup() {
    let file = c06_backup_file("info-json");
    let out = openwraith(&["info", "--json", "--from", path_str(&file)]);
    assert!(out.status.success(), "{}", stderr(&out));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(json["product"], "W75HEARGB");
    assert_eq!(json["firmware"], "3.22.07");
    assert_eq!(json["active_profile"], 1);
    assert_eq!(json["poll_rate"]["hz"], 8000);
    assert_eq!(json["created_at"], "2026-10-24T07:30:00Z");
    assert!(json["note"].is_string(), "{json}");
    std::fs::remove_file(file).ok();
}

#[test]
fn every_section_works_from_backup() {
    let file = c06_backup_file("sections");
    for section in ["triggers", "light", "keymap", "macros", "all"] {
        for json in [false, true] {
            let mut args = vec!["show", section, "--from", path_str(&file)];
            if json {
                args.push("--json");
            }
            let out = openwraith(&args);
            assert!(out.status.success(), "{section}: {}", stderr(&out));
            if json {
                serde_json::from_str::<serde_json::Value>(&stdout(&out))
                    .unwrap_or_else(|e| panic!("{section} --json: {e}"));
            }
        }
    }
    std::fs::remove_file(file).ok();
}

#[test]
fn bad_files_give_friendly_errors() {
    let missing = std::env::temp_dir().join("openwraith-cli-test-does-not-exist.json");
    let out = openwraith(&["show", "light", "--from", path_str(&missing)]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).starts_with("error: cannot read"),
        "{}",
        stderr(&out)
    );

    let foreign = std::env::temp_dir().join(format!(
        "openwraith-cli-test-{}-foreign.json",
        std::process::id()
    ));
    std::fs::write(&foreign, "{\"hello\": 1}").expect("write");
    let out = openwraith(&["show", "all", "--from", path_str(&foreign)]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("not an OpenWraith backup"), "{err}");
    assert!(
        err.contains("hint: expected a file written by `openwraith backup`"),
        "{err}"
    );
    std::fs::remove_file(foreign).ok();
}

#[test]
fn help_mentions_read_only_and_has_no_write_commands() {
    let out = openwraith(&["--help"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("READ-ONLY"), "{text}");
    assert!(
        text.contains("not affiliated with or endorsed by Wraith"),
        "{text}"
    );
    for command in ["list", "info", "backup", "show"] {
        assert!(text.contains(command), "{command}");
    }
    let out = openwraith(&["restore"]);
    assert_eq!(out.status.code(), Some(2));
}

/// An all-profile backup (schema 2) of the device simulator, in which
/// profile 2 has factory triggers and the factory light while profile 1
/// keeps c06 (W 1.00 mm RT 0.30, Custom red). No HID device is involved.
fn all_profiles_backup() -> Backup {
    use openwraith_hid::protocol::{ProfileId, W75};
    use openwraith_hid::{ConsentOrigin, DeviceSim, ProfilesOptions, SimTable, WriteConsent};
    let c06 = c06_backup();
    let mut sim = DeviceSim::from_backup(&c06);
    let mut factory = DeviceSim::from_backup(&c06);
    factory.factory_reset_all();
    for profile in ProfileId::ALL.into_iter().skip(1) {
        for key in W75.positions() {
            if let Some(record) = factory.trigger(profile, key) {
                sim.set_trigger(profile, key, record);
            }
        }
        sim.set_table(
            profile,
            SimTable::MainLight,
            openwraith_hid::sim::FACTORY_LIGHT.to_vec(),
        );
    }
    let options = ProfilesOptions {
        backup: BackupOptions {
            tool: ToolInfo {
                name: "openwraith-cli-tests".into(),
                version: "0".into(),
            },
            created_at: Some(UNIX_EPOCH + Duration::from_secs(1_792_827_000)),
        },
        ..ProfilesOptions::default()
    };
    let mut session = Session::new(sim);
    openwraith_hid::read_all_profiles(
        &mut session,
        WriteConsent::new(ConsentOrigin::CommandLineFlag),
        &options,
        |_| std::ops::ControlFlow::Continue(()),
    )
    .expect("all-profile backup")
}

#[test]
fn help_lists_the_commands_that_write() {
    let out = openwraith(&["--help"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(
        text.contains("list, info, show and backup are READ-ONLY"),
        "{text}"
    );
    assert!(
        text.contains("These commands write to the keyboard: profile select, set, restore, and"),
        "{text}"
    );
    assert!(text.contains("a tab in the"), "{text}");
    for command in ["profile", "set", "restore", "--safety-dir"] {
        assert!(text.contains(command), "{command}");
    }
    let out = openwraith(&["restore", "--help"]);
    assert!(out.status.success());
    let text = stdout(&out);
    for option in [
        "--dry-run",
        "--against",
        "--yes",
        "--allow-firmware-mismatch",
        "--packets",
    ] {
        assert!(text.contains(option), "{option}");
    }
}

#[test]
fn usage_errors_exit_with_code_2_before_anything_is_opened() {
    for args in [
        vec!["set"],
        vec!["restore"],
        vec!["set", "trigger", "W"],
        vec!["set", "trigger", "Knob", "--actuation", "1.20"],
        vec!["set", "trigger", "W", "--actuation", "1.21"],
        vec!["set", "socd"],
        vec!["reset-triggers"],
        vec!["profile", "select", "9"],
        vec!["show", "triggers", "--profile", "2"],
        vec!["restore", "x.json", "--dry-run", "--yes"],
    ] {
        let out = openwraith(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", stderr(&out));
        assert!(!stderr(&out).contains("Reading the keyboard"), "{args:?}");
    }
}

#[test]
fn offline_restore_plan_against_another_backup() {
    let target = c06_backup_file("restore-target");
    // The "keyboard": c06 with W back at factory values.
    let mut current = c06_backup();
    let w = ReadRequest::KeyTrigger(KeyPos::new(2, 2).expect("W"));
    let mut bytes = [0u8; REPORT_LEN];
    bytes[..17].copy_from_slice(&[
        0x21, 0, 0, 0, 0x0c, 0x05, 0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0, 0x02, 0x02,
    ]);
    let raw = RawResponse::from_reports(w, [Report::new(bytes)]).expect("valid reply");
    let entry = current
        .entries
        .iter_mut()
        .find(|e| e.request() == w)
        .expect("W entry");
    *entry = BackupEntry::new(raw);
    let against = temp_backup_path("restore-against");
    current.save(&against, true).expect("saved");

    let out = openwraith(&[
        "restore",
        path_str(&target),
        "--dry-run",
        "--against",
        path_str(&against),
        "--packets",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.starts_with("Restore plan (dry run: nothing is written)"),
        "{text}"
    );
    assert!(
        text.contains("1.70 mm, RT off -> 1.00 mm, RT 0.30 mm"),
        "{text}"
    );
    assert!(text.contains("21 00 00 00 18 0c 00 00 04"), "{text}");
    assert!(
        text.contains("Total: 1 write (1 packet) in profile 1."),
        "{text}"
    );

    let out = openwraith(&[
        "restore",
        path_str(&target),
        "--dry-run",
        "--against",
        path_str(&against),
        "--json",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(json["dry_run"], true);
    let items = json["plan"]["profiles"][0]["items"]
        .as_array()
        .expect("items");
    let write = items
        .iter()
        .find(|i| i["action"]["action"] == "write")
        .expect("a write");
    assert_eq!(write["table"], "triggers");
    assert_eq!(write["action"]["packets"], 1);
    assert!(json.get("report").is_none(), "{json}");

    // A missing backup file is an error, not a usage error.
    let missing = std::env::temp_dir().join("openwraith-cli-test-no-such-backup.json");
    let out = openwraith(&[
        "restore",
        path_str(&missing),
        "--dry-run",
        "--against",
        path_str(&against),
    ]);
    assert_eq!(out.status.code(), Some(1));
    std::fs::remove_file(target).ok();
    std::fs::remove_file(against).ok();
}

#[test]
fn profiles_of_an_all_profile_backup() {
    let file = temp_backup_path("all-profiles");
    let backup = all_profiles_backup();
    assert!(backup.is_all_profiles());
    backup.save(&file, true).expect("saved");

    // Without --profile: the profile that was active, and a note.
    let out = openwraith(&["show", "triggers", "--from", path_str(&file)]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("Rapid Trigger on 1 key(s): W."),
        "{}",
        stdout(&out)
    );
    assert!(
        stderr(&out).contains("all-profile backup (profiles 1, 2, 3, 4)"),
        "{}",
        stderr(&out)
    );

    // Profile 2: factory triggers.
    let out = openwraith(&[
        "show",
        "triggers",
        "--json",
        "--from",
        path_str(&file),
        "--profile",
        "2",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(json["active_profile"], 2);
    let keys = json["keys"].as_array().expect("keys");
    let w = keys.iter().find(|k| k["name"] == "W").expect("W");
    assert_eq!(w["actuation_mm"], 1.7);
    assert_eq!(w["rapid_trigger"], false);

    let out = openwraith(&["info", "--from", path_str(&file), "--profile", "3"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("showing profile 3"),
        "{}",
        stderr(&out)
    );

    // A single-profile backup has no profile 2.
    let single = c06_backup_file("single-profile");
    let out = openwraith(&[
        "show",
        "light",
        "--from",
        path_str(&single),
        "--profile",
        "2",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("holds no profile 2"),
        "{}",
        stderr(&out)
    );

    // Restoring profile 2 of the file, planned offline against the file
    // itself: nothing to write.
    let out = openwraith(&[
        "restore",
        path_str(&file),
        "--profile",
        "2",
        "--dry-run",
        "--against",
        path_str(&file),
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("Total: nothing to write."),
        "{}",
        stdout(&out)
    );
    std::fs::remove_file(file).ok();
    std::fs::remove_file(single).ok();
}
