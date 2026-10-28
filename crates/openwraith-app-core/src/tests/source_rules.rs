//! Fitness tests on the source: the app writes only through the library's
//! entry points, with its consent built in one place and used only by the
//! apply steps; `unsafe` lives in one file, which opens processes with the
//! limited query right only.

use std::path::{Path, PathBuf};

use super::generated::repo_root;

/// Where the scanned `.rs` files are: the core's code and the whole Tauri
/// crate (its `src`, its build script and any other Rust file in it).
const SCANNED_DIRS: [&str; 2] = ["crates/openwraith-app-core/src", "app/src-tauri"];

/// Whether the rules scan the file `rel` (relative to the repository, with
/// `/`) found under [`SCANNED_DIRS`]: all but the core's own tests. The
/// core forbids `unsafe_code`, which no module can allow again; the Tauri
/// crate only denies it (so that `winproc.rs` can allow it), which any of
/// its modules could allow, so its tests are scanned like the rest.
fn scanned(rel: &str) -> bool {
    !rel.starts_with("crates/openwraith-app-core/src/tests/")
}

/// Every scanned `.rs` file ([`scanned`]), with its path relative to the
/// repository and its code without comments.
fn sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in SCANNED_DIRS {
        collect(&root.join(dir), &mut files);
    }
    files
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, p)
        })
        .filter(|(rel, _)| scanned(rel))
        .map(|(rel, p)| {
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

/// The `.rs` files under `dir` (not in a build output folder `target`).
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != "target" {
                collect(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Whether `token` occurs in `code` as a whole word.
fn has_word(code: &str, token: &str) -> bool {
    !word_positions(code, token).is_empty()
}

/// Where `token` occurs in `code` as a whole word.
fn word_positions(code: &str, token: &str) -> Vec<usize> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    code.match_indices(token)
        .filter(|(at, _)| {
            let before = code[..*at].chars().next_back().is_none_or(|c| !word(c));
            let after = code[at + token.len()..]
                .chars()
                .next()
                .is_none_or(|c| !word(c));
            before && after
        })
        .map(|(at, _)| at)
        .collect()
}

/// The one file of the app that may use `unsafe`: the limited process
/// access of the per-game profiles' feed.
const UNSAFE_FILE: &str = "app/src-tauri/src/winproc.rs";

#[test]
fn the_sources_are_scanned() {
    let files = sources();
    assert!(files.iter().any(|(p, _)| p.ends_with("ops/trigger.rs")));
    assert!(
        files
            .iter()
            .any(|(p, _)| p.ends_with("app/src-tauri/src/commands.rs"))
    );
    assert!(files.iter().any(|(p, _)| p == "app/src-tauri/build.rs"));
    // Only the core's own tests are left out: the core forbids
    // `unsafe_code`; the Tauri crate only denies it, so a test module of it
    // could allow it and is scanned.
    assert!(
        !files
            .iter()
            .any(|(p, _)| p.starts_with("crates/openwraith-app-core/src/tests/"))
    );
    assert!(!scanned(
        "crates/openwraith-app-core/src/tests/source_rules.rs"
    ));
    for file in [
        "app/src-tauri/src/tests/mod.rs",
        "app/src-tauri/src/tray/tests/menu.rs",
        "app/src-tauri/tests/winproc.rs",
        "app/src-tauri/build.rs",
        "crates/openwraith-app-core/src/ops/tests.rs",
    ] {
        assert!(scanned(file), "{file} is scanned");
    }
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
fn unsafe_code_lives_only_in_the_limited_process_access() {
    let mut seen = false;
    for (path, code) in sources() {
        if path == UNSAFE_FILE {
            seen = true;
            continue;
        }
        assert!(!has_word(&code, "unsafe"), "{path} uses unsafe");
        assert!(
            !code.contains("allow(unsafe_code)"),
            "{path} allows unsafe code"
        );
    }
    assert!(seen, "{UNSAFE_FILE} is scanned");
    // The core forbids it; the Tauri crate denies it, so that only that
    // file can allow it, with its own lints for the unsafe blocks.
    assert!(code_of("crates/openwraith-app-core/src/lib.rs").contains("#![forbid(unsafe_code)]"));
    assert!(code_of("app/src-tauri/src/lib.rs").contains("#![deny(unsafe_code)]"));
    let code = code_of(UNSAFE_FILE);
    for lint in [
        "#![allow(unsafe_code)]",
        "#![deny(unsafe_op_in_unsafe_fn)]",
        "#![deny(clippy::undocumented_unsafe_blocks)]",
    ] {
        assert!(code.contains(lint), "{UNSAFE_FILE} lacks {lint}");
    }
    // Every unsafe block has its `// SAFETY:` comment (clippy checks where).
    let text = std::fs::read_to_string(repo_root().join(UNSAFE_FILE)).unwrap();
    let blocks = word_positions(&code, "unsafe").len();
    assert!(blocks > 0);
    assert_eq!(
        text.matches("// SAFETY: ").count(),
        blocks,
        "{UNSAFE_FILE}: a SAFETY comment per unsafe block"
    );
}

#[test]
fn processes_are_opened_with_the_limited_query_right_only() {
    // Design B7: no memory read and no full query right on any process (a
    // game's anti-cheat driver sees the handles), so `PROCESS_VM_READ`,
    // `PROCESS_QUERY_INFORMATION` (as a word: not inside
    // `PROCESS_QUERY_LIMITED_INFORMATION`) and the like never appear.
    let mut calls = Vec::new();
    for (path, code) in sources() {
        for token in [
            "PROCESS_VM_READ",
            "PROCESS_QUERY_INFORMATION",
            "PROCESS_ALL_ACCESS",
            "NtOpenProcess",
        ] {
            assert!(!has_word(&code, token), "{path} uses {token}");
        }
        assert!(!code.contains("PROCESS_VM_"), "{path} uses a memory right");
        for at in word_positions(&code, "OpenProcess") {
            assert_eq!(path, UNSAFE_FILE, "{path} opens a process");
            // An import (in a `use` list) or a call; never renamed.
            let rest: String = code[at + "OpenProcess".len()..]
                .chars()
                .filter(|c| !c.is_whitespace())
                .take(40)
                .collect();
            if rest.starts_with('(') {
                assert!(
                    rest.starts_with("(PROCESS_QUERY_LIMITED_INFORMATION,"),
                    "{path}: OpenProcess{rest}"
                );
                calls.push(path.clone());
            } else {
                assert!(
                    rest.starts_with(',') || rest.starts_with('}'),
                    "{path}: OpenProcess{rest}"
                );
            }
        }
    }
    assert_eq!(calls, [UNSAFE_FILE], "the only OpenProcess call");
    // No process library of its own (sysinfo 0.39 opened every listed
    // process with `PROCESS_QUERY_INFORMATION | PROCESS_VM_READ`).
    let manifest = std::fs::read_to_string(repo_root().join("app/src-tauri/Cargo.toml")).unwrap();
    for line in manifest
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
    {
        assert!(
            !line.contains("sysinfo"),
            "app/src-tauri/Cargo.toml: {line}"
        );
    }
}

/// The one file of the app that uses the registry: start with Windows.
const REGISTRY_FILE: &str = "app/src-tauri/src/autostart.rs";

#[test]
fn the_registry_is_written_only_at_the_per_user_run_value() {
    // Start with Windows (`autostart.rs`): the per-user Run value only,
    // never `HKLM` (also not as administrator), and Task Manager's
    // `StartupApproved` choice read, never written.
    for (path, code) in sources() {
        for token in ["windows_registry", "RegSetValueExW", "RegDeleteValueW"] {
            assert!(
                path == REGISTRY_FILE || !has_word(&code, token),
                "{path} uses {token}"
            );
        }
        for token in [
            "LOCAL_MACHINE",
            "HKEY_LOCAL_MACHINE",
            "CLASSES_ROOT",
            "CURRENT_CONFIG",
        ] {
            assert!(!has_word(&code, token), "{path} uses {token}");
        }
    }
    // Its code without its tests (which write only under their own key).
    let code = code_of(REGISTRY_FILE);
    let code = &code[..code.find("\nmod tests {").expect("autostart.rs has tests")];
    assert!(!has_word(code, "USERS"), "{REGISTRY_FILE} uses HKEY_USERS");
    // One write and one delete, both of OpenWraith's Run value.
    for (call, count) in [
        ("set_string(VALUE_NAME,", 1),
        ("remove_value(VALUE_NAME)", 1),
    ] {
        assert_eq!(code.matches(call).count(), count, "{REGISTRY_FILE}: {call}");
    }
    for token in [
        "set_string",
        "remove_value",
        "set_bytes",
        "set_value",
        "set_u32",
        "set_u64",
        "set_hstring",
        "set_expand_string",
        "set_expand_hstring",
        "set_multi_string",
        "remove_tree",
        "rename",
        "transaction",
        "raw_set_bytes",
    ] {
        let allowed = usize::from(matches!(token, "set_string" | "remove_value"));
        assert_eq!(
            word_positions(code, token).len(),
            allowed,
            "{REGISTRY_FILE}: {token}"
        );
    }
    // Every key opened for writing is the Run key; the StartupApproved key
    // is only ever opened for reading.
    let writes = code.match_indices(".write()").count();
    assert_eq!(writes, 2, "{REGISTRY_FILE}: the write and the delete");
    for (at, _) in code.match_indices(".write()") {
        let rest: String = code[at + ".write()".len()..]
            .chars()
            .filter(|c| !c.is_whitespace())
            .take(40)
            .collect();
        assert!(
            rest.starts_with(".open(&self.run_key)")
                || rest.starts_with(".create().open(&self.run_key)"),
            "{REGISTRY_FILE}: .write(){rest}"
        );
    }
    assert!(!code.contains(".create(&self.approved_key)"));
    let opened: Vec<_> = code.match_indices("&self.approved_key").collect();
    assert_eq!(
        opened.len(),
        1,
        "{REGISTRY_FILE}: the approved key is read once"
    );
    assert!(code.contains("CURRENT_USER.open(&self.approved_key)"));
    // The real keys: in the one constructor, which test builds do not have
    // (no test can write the real Run or StartupApproved key).
    for key in ["RUN_KEY", "APPROVED_KEY"] {
        assert_eq!(
            word_positions(code, key).len(),
            2,
            "{REGISTRY_FILE}: {key} is declared and used by the real registry only"
        );
    }
    let real = code.find("fn real()").expect("the real registry");
    assert!(
        code[..real].trim_end().ends_with("#[cfg(not(test))]"),
        "{REGISTRY_FILE}: the real keys are not in test builds"
    );
    // No start-entry library of its own (`tauri-plugin-autostart` 2.7 wrote
    // the path unquoted, `HKLM` as administrator, and forced Task Manager's
    // choice to enabled).
    let manifest = std::fs::read_to_string(repo_root().join("app/src-tauri/Cargo.toml")).unwrap();
    for line in manifest
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
    {
        for name in ["tauri-plugin-autostart", "auto-launch"] {
            assert!(!line.contains(name), "app/src-tauri/Cargo.toml: {line}");
        }
    }
}

#[test]
fn what_only_the_windows_registry_uses_may_be_unused_off_windows() {
    // The README builds the workspace on Linux too (`cargo clippy
    // --workspace --all-targets -- -D warnings`). The key paths, the value
    // name and the values read are used only by the real registry
    // (`cfg(windows)`), so each may be unused off Windows (as all of
    // `winproc.rs` may).
    let code = code_of(REGISTRY_FILE);
    for item in [
        "pub const RUN_KEY:",
        "pub const APPROVED_KEY:",
        "pub const VALUE_NAME:",
        "pub enum RunValue",
    ] {
        let at = code
            .find(item)
            .unwrap_or_else(|| panic!("{REGISTRY_FILE}: {item}"));
        // Its attributes (the doc comments are empty lines here).
        let allowed = code[..at]
            .lines()
            .rev()
            .map(str::trim)
            .take_while(|line| line.is_empty() || line.starts_with("#["))
            .any(|line| {
                line.starts_with("#[cfg_attr(")
                    && line.contains("not(windows)")
                    && line.ends_with("allow(dead_code))]")
            });
        assert!(
            allowed,
            "{REGISTRY_FILE}: {item} lacks #[cfg_attr(any(not(windows), ...), allow(dead_code))]"
        );
    }
    assert!(code_of(UNSAFE_FILE).contains("#![cfg_attr(not(windows), allow(dead_code))]"));
}

#[test]
fn only_the_profile_switch_and_the_stray_close_create_a_writer() {
    for (path, code) in sources() {
        if !code.contains(".writer(") {
            continue;
        }
        if path.ends_with("ops/travel.rs") {
            // One writer, for the close of a test left open and nothing
            // else (the library has no session-level form of it).
            let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
            assert_eq!(compact.matches(".writer(").count(), 1, "{path}");
            assert_eq!(
                compact.matches("writer.").count(),
                1,
                "{path}: the writer does nothing else"
            );
            assert!(
                compact.contains("writer.close_stray_travel_test()"),
                "{path}: the writer is used only for the stray close"
            );
            continue;
        }
        assert!(path.ends_with("ops/profile.rs"), "{path} creates a writer");
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
        "ops/keymap.rs",
        "ops/colors.rs",
        "ops/dead_band.rs",
        "ops/socd.rs",
        "ops/travel.rs",
        "ops/share.rs",
    ];
    // Steps the user started explicitly: the apply steps, the comparisons
    // that switch profiles after the user agreed (restore, import), an
    // export of another profile (switch agreed), the live key-depth view
    // and the close of a test left open (user clicks).
    let allowed_fns = [
        "prepare_restore",
        "prepare_import",
        "export_profile",
        "start_depth_view",
        "close_stray_travel_test",
    ];
    let mut uses = 0;
    for (path, code) in sources() {
        if code.contains("ConsentOrigin::") || code.contains("WriteConsent::new") {
            assert!(path.ends_with("ops/consent.rs"), "{path} builds a consent");
            assert_eq!(
                code.matches("ConsentOrigin::").count(),
                2,
                "two consent origins: the application's and the standing rule's"
            );
            assert!(code.contains("ConsentOrigin::Application"));
            assert!(code.contains("ConsentOrigin::StandingRule"));
        }
        // The standing rule's consent only in the automatic switch, where
        // it checks that the library's switcher uses it.
        if code.contains("consent::standing_rule()") {
            assert!(
                path.ends_with("ops/auto_profile.rs"),
                "{path} uses the standing rule's consent"
            );
            assert_eq!(
                enclosing_fns(&code, "consent::standing_rule()"),
                ["switch_for_rule"]
            );
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
                name.starts_with("apply") || allowed_fns.contains(&name.as_str()),
                "{path}: consent used in fn {name}"
            );
        }
    }
    assert_eq!(
        uses, 14,
        "profile, trigger, light, all-profile backup, restore prepare and apply, keymap edit, \
         colours, dead band, SOCD, depth view, stray close, export, import prepare \
         (restore and import apply share one call)"
    );
}

#[test]
fn table_contents_are_built_only_by_the_library() {
    // The editors hand the library typed intents (`KeyAssignment`,
    // `MacroProgram`, `AdvancedKeyDef`, ...); the app never builds or
    // patches a table, a slot or a raw entry, never decodes raw entries
    // past the library's views, and never leaves out the Fn keymap that
    // makes the keyboard keep a group (a hardware-test mode).
    let forbidden = [
        "set_entry",
        "KeymapData",
        "MacroData",
        "CanonicalMacro",
        "AdvancedSlotData",
        "SocdWrite",
        "SocdPairsData",
        "TravelTestRequest",
        "ColorTableData",
        "DeadBandData",
        "DeadBandWrite",
        "from_read_slot",
        "KeyAction::decode",
        "KeyAction::decode_unswapped",
        "MacroProgram::decode",
        "OmitForSlotsOnly",
    ];
    for (path, code) in sources() {
        for token in forbidden {
            assert!(!has_word(&code, token), "{path} uses {token}");
        }
    }
}

#[test]
fn keymap_intents_carry_no_raw_bytes() {
    // Every type the frontend sends for the Keys page is typed: no entry,
    // slot or packet bytes (the views may show bytes as hex).
    let text =
        std::fs::read_to_string(repo_root().join("crates/openwraith-app-core/src/dto/keys.rs"))
            .unwrap();
    let mut deserialized = 0;
    for item in text.split("#[derive(").skip(1) {
        let derive = &item[..item.find(')').unwrap()];
        if !derive.contains("Deserialize") {
            continue;
        }
        deserialized += 1;
        let body = &item[..item.find("\n}\n").unwrap_or(item.len())];
        // (Numbers are HID usages, indices and slots, never entry bytes.)
        for raw in ["hex:", "bytes:", "entry:", "data:", "[u8;"] {
            assert!(!body.contains(raw), "an intent type carries {raw}: {body}");
        }
    }
    assert!(deserialized >= 10, "{deserialized} intent types");
}

#[test]
fn travel_and_standing_rule_calls_stay_in_their_operations() {
    for (path, code) in sources() {
        if code.contains(".profile_switcher(") {
            assert!(
                path.ends_with("ops/auto_profile.rs"),
                "{path} uses the profile switcher"
            );
        }
        // The library's calls; the app's own operation, command and handle
        // method of the same name (`fn ...`, `self.`, `worker.`,
        // `commands::`) only forward to ops/travel.rs.
        let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        for call in ["live_travel(", "close_stray_travel_test("] {
            for (at, _) in compact.match_indices(call) {
                let before = &compact[..at];
                let forwarding = ["fn", "self.", "worker.", "commands::"]
                    .iter()
                    .any(|p| before.ends_with(p));
                if !forwarding {
                    assert!(path.ends_with("ops/travel.rs"), "{path} calls {call}");
                }
            }
        }
    }
}

#[test]
fn editor_intents_carry_no_raw_bytes() {
    // Every intent the frontend sends (every `Deserialize` type of `dto/`)
    // is typed: keys by index, colours as "#rrggbb", values in trigger
    // units, files as tokens: no bytes, no hex, no paths.
    let dto = repo_root().join("crates/openwraith-app-core/src/dto");
    let mut files: Vec<String> = std::fs::read_dir(&dto)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".rs"))
        .collect();
    files.sort();
    // The files known to hold intents must keep holding some (a renamed
    // derive would otherwise hide them from the scan).
    for known in [
        "editors.rs",
        "share.rs",
        "travel.rs",
        "auto.rs",
        "recovery.rs",
        "background.rs",
        "intent.rs",
    ] {
        assert!(files.iter().any(|f| f == known), "dto/{known}");
    }
    for name in &files {
        let file = format!("dto/{name}");
        let text = std::fs::read_to_string(dto.join(name)).unwrap();
        let mut deserialized = 0;
        for item in text.split("#[derive(").skip(1) {
            let derive = &item[..item.find(')').unwrap()];
            if !derive.contains("Deserialize") {
                continue;
            }
            deserialized += 1;
            let body = &item[..item.find("\n}\n").unwrap_or(item.len())];
            for raw in [
                "hex:", "bytes:", "entry:", "data:", "[u8;", "path:", "PathBuf",
            ] {
                assert!(
                    !body.contains(raw),
                    "{file}: an intent type carries {raw}: {body}"
                );
            }
        }
        if [
            "editors.rs",
            "share.rs",
            "travel.rs",
            "auto.rs",
            "recovery.rs",
            "background.rs",
            "intent.rs",
        ]
        .contains(&name.as_str())
        {
            assert!(deserialized >= 1, "{file}: {deserialized} intent types");
        }
    }
}

#[test]
fn the_new_editors_use_only_the_library_entry_points() {
    // Each editor's apply step calls exactly its library entry point; the
    // preset path ends in the trigger edit's plan (no write of its own).
    for (file, call) in [
        ("ops/colors.rs", "apply_color_edit("),
        ("ops/dead_band.rs", "apply_dead_band_edit("),
        ("ops/socd.rs", "apply_socd_edit("),
        ("ops/share.rs", "prepare_restore("),
    ] {
        let code = sources()
            .into_iter()
            .find(|(p, _)| p.ends_with(file))
            .map(|(_, c)| c)
            .unwrap_or_else(|| panic!("{file} not found"));
        assert!(code.contains(call), "{file} calls {call}");
    }
    let presets = sources()
        .into_iter()
        .find(|(p, _)| p.ends_with("ops/presets.rs"))
        .map(|(_, c)| c)
        .unwrap();
    assert!(!presets.contains("consent::application()"));
    assert!(presets.contains("prepare_trigger_plan("));
    assert!(!presets.contains("apply_trigger_edit("));
}

/// The code of `file` (comments removed).
fn code_of(file: &str) -> String {
    sources()
        .into_iter()
        .find(|(p, _)| p.ends_with(file))
        .map(|(_, c)| c)
        .unwrap_or_else(|| panic!("{file} not found"))
}

/// The names of the functions in `code` that contain `needle` (an empty
/// name where `needle` is the name of a function being defined).
fn enclosing_fns(code: &str, needle: &str) -> Vec<String> {
    code.match_indices(needle)
        .map(|(at, _)| {
            let head = &code[..at];
            head.rfind("fn ")
                .map(|i| {
                    head[i + 3..]
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect::<String>()
                })
                .unwrap_or_default()
        })
        .collect()
}

#[test]
fn recovery_never_writes_and_restores_only_through_a_user_apply() {
    // Detection, promotion, "keep them" and the Recovery card only read
    // and write the store's files: no consent, no writer, no profile
    // switcher, no journal, no restore execution, no session call.
    for file in ["ops/recovery.rs", "autobackup.rs"] {
        let code = code_of(file);
        for token in [
            "consent::",
            ".writer(",
            ".profile_switcher(",
            "execute_restore",
            "write_journal",
            "apply_restore",
            "apply_prepared_restore",
            "select_profile",
            "read_profiles",
            ".request(",
            "session",
        ] {
            assert!(!code.contains(token), "{file} uses {token}");
        }
    }
    // A recovery reaches the keyboard only through the restore's
    // comparison (`prepare_restore`, whose plan waits for a token).
    let recovery = code_of("ops/recovery.rs");
    assert_eq!(recovery.matches("self.prepare_restore(").count(), 1);
    assert_eq!(
        enclosing_fns(&recovery, "self.prepare_restore("),
        ["prepare_recovery"]
    );
    // A restore is executed in one place, which only the apply steps reach
    // (each consumes the token of a plan the user reviewed).
    let mut executes = Vec::new();
    for (path, code) in sources() {
        for name in enclosing_fns(&code, "execute_restore(") {
            executes.push((path.clone(), name));
        }
        for name in enclosing_fns(&code, "apply_prepared_restore(") {
            // (An empty name: the definition itself.)
            if !name.is_empty() {
                assert!(
                    name.starts_with("apply_") && code.contains("take_plan("),
                    "{path}: apply_prepared_restore reached from fn {name}"
                );
            }
        }
    }
    assert_eq!(
        executes,
        [(
            "crates/openwraith-app-core/src/ops/restore.rs".to_string(),
            "apply_prepared_restore".to_string()
        )]
    );
    // Automatic backups are saved only by the known-good comparison.
    for (path, code) in sources() {
        if code.contains("autobackup::save(") || code.contains("autobackup::save_recovery(") {
            assert!(
                path.ends_with("ops/recovery.rs"),
                "{path} saves automatic backups"
            );
        }
    }
}

#[test]
fn the_automatic_path_reuses_reads_only() {
    // The comparison runs on reads the worker made anyway (`full_read`);
    // it is never a request of its own.
    let snapshot = code_of("ops/snapshot.rs");
    assert_eq!(
        enclosing_fns(&snapshot, "self.observe_read("),
        ["full_read"]
    );
    for (path, code) in sources() {
        if code.contains("observe_read(") && !path.ends_with("ops/recovery.rs") {
            assert!(path.ends_with("ops/snapshot.rs"), "{path} compares reads");
        }
    }
    // Session hygiene replaces an idle session without sending anything.
    let hygiene = code_of("worker/hygiene.rs");
    for token in [".request(", ".read::<", ".writer(", "consent::", "listen("] {
        assert!(!hygiene.contains(token), "hygiene uses {token}");
    }
}

#[test]
fn the_automatic_switch_sends_only_the_profile_select_and_reads() {
    // `.profile_switcher(` once, in the switch of a game rule; its only use
    // is `switch_to`. The automatic path builds no writer, uses no
    // application consent, writes no journal or safety backup and never
    // reaches a table write or a restore.
    let auto = code_of("ops/auto_profile.rs");
    assert_eq!(auto.matches(".profile_switcher(").count(), 1);
    assert_eq!(
        enclosing_fns(&auto, ".profile_switcher("),
        ["switch_for_rule"]
    );
    let compact: String = auto.chars().filter(|c| !c.is_whitespace()).collect();
    assert_eq!(
        compact.matches("switcher.").count(),
        3,
        "consent, switch_to, session"
    );
    assert_eq!(compact.matches("switcher.switch_to(").count(), 1);
    for token in [
        ".writer(",
        "consent::application()",
        "write_journal",
        "save_safety",
        "execute_restore",
        "prepare_restore",
        "apply_",
        "select_profile",
        "read_profiles",
        "live_travel",
        "close_stray_travel_test",
        ".request(",
        "plan_",
    ] {
        assert!(!auto.contains(token), "ops/auto_profile.rs uses {token}");
    }
    // The engine and the shell's model are pure: no session, no file.
    for file in [
        "auto/engine.rs",
        "auto/tray.rs",
        "auto/notify.rs",
        "auto/programs.rs",
    ] {
        let code = code_of(file);
        for token in ["session", "Session", "std::fs", "consent::", "Worker"] {
            assert!(!code.contains(token), "{file} uses {token}");
        }
    }
    // The device lock is taken only by the worker's coexistence code.
    for (path, code) in sources() {
        if code.contains("DeviceLock::try_exclusive(") || code.contains("DeviceLock::try_shared(") {
            assert!(path.ends_with("worker/coexist.rs"), "{path} takes the lock");
        }
        assert!(
            !code.contains("DeviceLock::try_shared("),
            "{path}: the app never holds the lock shared"
        );
    }
    // Every write passes `ensure_quiet`, which takes the lock first.
    let ops = code_of("ops/mod.rs");
    let quiet = &ops[ops.find("fn ensure_quiet(").unwrap()..];
    let first = quiet.find("self.take_device_lock()?").unwrap();
    assert!(
        first < quiet.find("listen(").unwrap(),
        "the lock before anything"
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
