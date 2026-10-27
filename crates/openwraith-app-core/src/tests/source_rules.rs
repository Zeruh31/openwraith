//! Fitness tests on the source: the app writes only through the library's
//! entry points, with its consent built in one place and used only by the
//! apply steps.

use std::path::{Path, PathBuf};

use super::generated::repo_root;

/// Every `.rs` file of the core (tests excluded) and of the app crate, with
/// its path relative to the repository and its code without comments.
fn sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in ["crates/openwraith-app-core/src", "app/src-tauri/src"] {
        collect(&root.join(dir), &mut files);
    }
    files
        .into_iter()
        .filter(|p| {
            !p.to_string_lossy()
                .replace('\\', "/")
                .contains("/src/tests/")
        })
        .map(|p| {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&p).unwrap();
            let code: String = text
                .lines()
                .map(|line| match line.find("//") {
                    Some(at) => &line[..at],
                    None => line,
                })
                .collect::<Vec<_>>()
                .join("\n");
            (rel, code)
        })
        .collect()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Whether `token` occurs in `code` as a whole word.
fn has_word(code: &str, token: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    code.match_indices(token).any(|(at, _)| {
        let before = code[..at].chars().next_back().is_none_or(|c| !word(c));
        let after = code[at + token.len()..]
            .chars()
            .next()
            .is_none_or(|c| !word(c));
        before && after
    })
}

#[test]
fn the_sources_are_scanned() {
    let files = sources();
    assert!(files.iter().any(|(p, _)| p.ends_with("ops/trigger.rs")));
    assert!(
        files
            .iter()
            .any(|(p, _)| p.ends_with("app/src-tauri/src/commands.rs"))
    );
    assert!(!files.iter().any(|(p, _)| p.contains("/src/tests/")));
}

#[test]
fn no_raw_or_unguarded_write_api_is_used() {
    // The simulator backend may forward write packets to the simulator.
    let forbidden = [
        ("write_guarded", false),
        ("write_guarded_observed", false),
        ("write_commit_trigger", false),
        ("write_commit_trigger_observed", false),
        ("write_command", true),
        ("WritePacket", true),
        ("WriteRequest", false),
        ("TriggerGrouping", false),
        ("SameSetting", false),
        ("with_grouping", false),
        ("CommitSettle", false),
        ("disabled_for_tests", false),
        ("SessionOptions", false),
        ("with_options", false),
        ("writer_with", false),
        ("HidTransport", false),
        ("unsafe", false),
    ];
    for (path, code) in sources() {
        for (token, sim_may) in forbidden {
            if sim_may && path.ends_with("backend/sim.rs") {
                continue;
            }
            assert!(!has_word(&code, token), "{path} uses {token}");
        }
        assert!(
            !code.contains("WriteOptions {"),
            "{path} builds WriteOptions by hand (always use default())"
        );
        assert!(!code.contains("#[ts(export"), "{path} exports bindings");
    }
}

#[test]
fn only_the_profile_switch_creates_a_writer() {
    for (path, code) in sources() {
        if code.contains(".writer(") {
            assert!(path.ends_with("ops/profile.rs"), "{path} creates a writer");
        }
    }
}

#[test]
fn consent_is_built_in_one_place_and_used_only_by_apply_steps() {
    let allowed_files = [
        "ops/profile.rs",
        "ops/trigger.rs",
        "ops/light.rs",
        "ops/backup.rs",
        "ops/restore.rs",
    ];
    let mut uses = 0;
    for (path, code) in sources() {
        if code.contains("ConsentOrigin::") || code.contains("WriteConsent::new") {
            assert!(path.ends_with("ops/consent.rs"), "{path} builds a consent");
            assert_eq!(
                code.matches("ConsentOrigin::").count(),
                1,
                "one consent origin"
            );
            assert!(code.contains("ConsentOrigin::Application"));
        }
        for (at, _) in code.match_indices("consent::application()") {
            uses += 1;
            assert!(
                allowed_files.iter().any(|f| path.ends_with(f)),
                "{path} uses the consent"
            );
            // The enclosing function: the last `fn` before the use.
            let head = &code[..at];
            let name = head
                .rfind("fn ")
                .map(|i| {
                    head[i + 3..]
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect::<String>()
                })
                .unwrap_or_default();
            assert!(
                name.starts_with("apply") || name == "prepare_restore",
                "{path}: consent used in fn {name}"
            );
        }
    }
    assert_eq!(
        uses, 6,
        "profile, trigger, light, all-profile backup, restore prepare and apply"
    );
}

#[test]
fn hidapi_is_opened_only_by_the_hid_backend() {
    for (path, code) in sources() {
        if code.contains("HidBackend::new") {
            assert!(path.ends_with("backend/hid.rs"), "{path} opens hidapi");
        }
    }
}

#[test]
fn the_simulator_is_never_part_of_a_release_build() {
    let guard = "compile_error!(\"the simulator must never be part of a release build\")";
    for file in [
        "crates/openwraith-app-core/src/lib.rs",
        "app/src-tauri/src/lib.rs",
    ] {
        let text = std::fs::read_to_string(repo_root().join(file)).unwrap();
        assert!(
            text.contains("#[cfg(all(feature = \"sim\", not(debug_assertions)))]")
                && text.contains(guard),
            "{file} lacks the release guard"
        );
    }
}
