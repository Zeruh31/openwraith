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
    // The colour, dead-band, SOCD, preset, export and import intents are
    // typed: keys by index, colours as "#rrggbb", values in trigger units.
    for file in ["dto/editors.rs", "dto/share.rs", "dto/travel.rs"] {
        let text = std::fs::read_to_string(
            repo_root()
                .join("crates/openwraith-app-core/src")
                .join(file),
        )
        .unwrap();
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
        assert!(deserialized >= 1, "{file}: {deserialized} intent types");
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
