//! The device lock shared with the desktop app, with the commands run as
//! the binary runs them against the device simulator (no HID device is
//! opened; the lock lives in a temporary directory).

use clap::Parser;
use openwraith_hid::coordination::{TOOL_APP, TOOL_CLI};
use openwraith_hid::{DeviceLock, DeviceSim, LockMode};

use crate::commands::Exit;
use crate::lock::LockNeed;
use crate::sim_tests::{Fixture, text};
use crate::test_support::{SimConnector, TestUi, c06_backup};
use crate::{Cli, hint, lock_need};

/// Parses `args` like the binary and runs the command against a fresh
/// simulator whose connector takes the lock in `fx`'s `lock` directory.
fn run_locked(fx: &Fixture, args: &[&str]) -> (anyhow::Result<Exit>, SimConnector) {
    let mut connector =
        SimConnector::new(DeviceSim::from_backup(&c06_backup())).with_lock_dir(fx.path("lock"));
    let mut ui = TestUi::answering(&[]);
    let result = crate::sim_tests::run(args, &mut connector, &mut ui);
    (result, connector)
}

/// `set trigger W --actuation 1.20` with the safety backups in `safety`.
fn set_trigger<'a>(safety: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "--safety-dir",
        safety,
        "set",
        "trigger",
        "W",
        "--actuation",
        "1.20",
    ];
    args.extend_from_slice(extra);
    args
}

#[test]
fn a_command_stops_at_once_while_the_app_writes() {
    let fx = Fixture::new();
    let safety = fx.safety_dir();
    let lock_dir = fx.path("lock");
    let app = DeviceLock::try_exclusive(&lock_dir, TOOL_APP, "automatic profile switch")
        .expect("the app's lock");
    for args in [
        set_trigger(text(&safety), &["--yes"]),
        set_trigger(text(&safety), &["--dry-run"]),
        vec!["profile"],
        vec!["profile", "select", "2"],
        vec!["monitor", "--duration", "1s"],
    ] {
        let (result, connector) = run_locked(&fx, &args);
        let err = result.expect_err("refused");
        assert_eq!(
            err.to_string(),
            "the OpenWraith app is writing; try again",
            "{args:?}"
        );
        assert_eq!(
            hint(&err),
            Some(
                "nothing was sent to the keyboard; run the command again when the other \
                 OpenWraith program has finished"
            )
        );
        // Refused before the keyboard was opened: nothing was sent, no
        // safety backup was made.
        assert_eq!(
            (connector.read_sessions, connector.write_sessions),
            (0, 0),
            "{args:?}"
        );
        assert!(connector.sim.borrow().written().is_empty());
        assert!(fx.safety_files().is_empty());
    }
    // The app's description is untouched.
    assert_eq!(
        DeviceLock::holder(&lock_dir).map(|h| h.tool),
        Some(TOOL_APP.to_string())
    );

    // Once the app has let go, the write goes through, holding the lock
    // exclusively from before the keyboard is opened until it ends.
    drop(app);
    let (result, connector) = run_locked(&fx, &set_trigger(text(&safety), &["--yes"]));
    assert_eq!(result.expect("written"), Exit::Success);
    assert!(connector.write_sessions > 0);
    assert!(!connector.lock_seen.is_empty());
    for seen in &connector.lock_seen {
        assert!(!seen.exclusive_free && !seen.shared_free, "{seen:?}");
        let holder = seen.holder.as_ref().expect("described");
        assert_eq!(holder.tool, TOOL_CLI);
        assert_eq!(holder.purpose, "set trigger");
        assert_eq!(holder.mode, LockMode::Exclusive);
        assert_eq!(holder.pid, std::process::id());
    }
    assert_eq!(DeviceLock::holder(&lock_dir), None);
    assert!(DeviceLock::try_exclusive(&lock_dir, TOOL_APP, "write").is_ok());
}

#[test]
fn reading_commands_share_the_lock() {
    let fx = Fixture::new();
    let safety = fx.safety_dir();
    let lock_dir = fx.path("lock");

    // A read holds the lock shared: another reader could join, a writer
    // could not.
    let (result, connector) = run_locked(&fx, &["profile"]);
    assert_eq!(result.expect("profile"), Exit::Success);
    let seen = connector.lock_seen.first().expect("a session");
    assert!(!seen.write_session);
    assert!(!seen.exclusive_free && seen.shared_free, "{seen:?}");
    let holder = seen.holder.as_ref().expect("described");
    assert_eq!(
        (holder.purpose.as_str(), holder.mode),
        ("profile", LockMode::Shared)
    );
    let (result, connector) = run_locked(&fx, &set_trigger(text(&safety), &["--dry-run"]));
    assert_eq!(result.expect("dry run"), Exit::Success);
    assert!(
        connector
            .lock_seen
            .iter()
            .all(|s| !s.exclusive_free && s.shared_free && !s.write_session)
    );
    assert!(fx.safety_files().is_empty());

    // While another command reads: reads go on, writes stop at once.
    let other = DeviceLock::try_shared(&lock_dir, TOOL_CLI, "show").expect("reader");
    let (result, _) = run_locked(&fx, &["profile"]);
    assert_eq!(result.expect("shared"), Exit::Success);
    let (result, connector) = run_locked(&fx, &["profile", "select", "2"]);
    let err = result.expect_err("busy");
    assert!(
        err.to_string()
            .starts_with("another openwraith command is using the keyboard (process"),
        "{err}"
    );
    assert!(connector.sim.borrow().written().is_empty());
    drop(other);
}

#[test]
fn commands_on_files_take_no_lock() {
    let fx = Fixture::new();
    let file = fx.save(&c06_backup(), "c06");
    let against = fx.save(&c06_backup(), "again");
    let exported = fx.path("p1.owprofile.json");
    let _app = DeviceLock::try_exclusive(&fx.path("lock"), TOOL_APP, "write").expect("app");
    let (result, connector) = run_locked(
        &fx,
        &[
            "restore",
            text(&file),
            "--dry-run",
            "--against",
            text(&against),
        ],
    );
    assert_eq!(result.expect("offline plan"), Exit::Success);
    let (result, _) = run_locked(
        &fx,
        &[
            "profile",
            "export",
            "1",
            "-o",
            text(&exported),
            "--from",
            text(&file),
        ],
    );
    assert_eq!(result.expect("export of a file"), Exit::Success);
    assert!(connector.lock_seen.is_empty());
}

#[test]
fn every_command_says_how_it_holds_the_lock() {
    let need = |args: &[&str]| {
        let cli = Cli::try_parse_from(std::iter::once("openwraith").chain(args.iter().copied()))
            .unwrap_or_else(|e| panic!("{args:?} does not parse: {e}"));
        lock_need(&cli.command)
    };
    let read = |p: &str| Some(LockNeed::read(p));
    let write = |p: &str| Some(LockNeed::write(p));
    for (args, expected) in [
        (&["list"][..], None),
        (&["info"], read("info")),
        (&["info", "--from", "b.json"], None),
        (&["show", "triggers"], read("show")),
        (&["show", "all", "--from", "b.json"], None),
        (&["backup"], read("backup")),
        (
            &["backup", "--all-profiles"],
            write("backup --all-profiles"),
        ),
        (&["profile"], read("profile")),
        (&["profile", "select", "2"], write("profile select")),
        (
            &["profile", "export", "1", "-o", "p.json"],
            write("profile export"),
        ),
        (
            &["profile", "export", "1", "-o", "p.json", "--from", "b.json"],
            None,
        ),
        (
            &["profile", "import", "p.json", "--into", "2"],
            write("profile import"),
        ),
        (
            &["profile", "import", "p.json", "--into", "2", "--dry-run"],
            read("profile import --dry-run"),
        ),
        (
            &["set", "trigger", "W", "--actuation", "1.20"],
            write("set trigger"),
        ),
        (
            &["set", "deadband", "W", "--top", "0.10", "--dry-run"],
            read("set deadband --dry-run"),
        ),
        (&["restore", "b.json"], write("restore")),
        (
            &["restore", "b.json", "--dry-run"],
            read("restore --dry-run"),
        ),
        (
            &["restore", "b.json", "--dry-run", "--against", "c.json"],
            None,
        ),
        (&["travel"], write("travel")),
        (&["travel", "--close"], write("travel --close")),
        (&["monitor"], read("monitor")),
    ] {
        assert_eq!(need(args), expected, "{args:?}");
    }
}
