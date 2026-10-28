//! The command names are the same everywhere: `command_names.rs` (the single
//! source), the frontend's `commands.ts`, and the capabilities (which allow
//! exactly these commands and no core or plugin permission).

use super::generated::repo_root;

/// The string literals between `start` and the next `end` in `text`.
fn names(text: &str, start: &str, end: &str) -> Vec<String> {
    let at = text
        .find(start)
        .unwrap_or_else(|| panic!("{start} not found"));
    let rest = &text[at + start.len()..];
    let body = &rest[..rest.find(end).unwrap_or_else(|| panic!("{end} not found"))];
    body.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repo_root().join(path)).unwrap()
}

#[test]
fn commands_ts_declares_exactly_the_rust_commands() {
    let rust = read("app/src-tauri/src/command_names.rs");
    let commands = names(&rust, "pub const COMMANDS: &[&str] = &[", "];");
    let dev = names(&rust, "pub const DEV_COMMANDS: &[&str] = &[", "];");
    assert_eq!(commands.len(), 79);
    let ts = read("app/src/ipc/commands.ts");
    assert_eq!(
        names(&ts, "export const COMMAND_NAMES = [", "] as const"),
        commands
    );
    assert_eq!(
        names(&ts, "export const DEV_COMMAND_NAMES = [", "] as const"),
        dev
    );
    // Every command has its entry in `Args` and `Results`.
    for name in commands.iter().chain(&dev) {
        assert!(ts.contains(&format!("  {name}: ")), "{name} is not typed");
    }
}

#[test]
fn the_capabilities_allow_exactly_the_commands() {
    let rust = read("app/src-tauri/src/command_names.rs");
    let allow = |list: Vec<String>| -> Vec<String> {
        list.iter()
            .map(|c| format!("allow-{}", c.replace('_', "-")))
            .collect()
    };
    let commands = allow(names(&rust, "pub const COMMANDS: &[&str] = &[", "];"));
    let dev = allow(names(&rust, "pub const DEV_COMMANDS: &[&str] = &[", "];"));
    for (file, expected, windows) in [
        ("app/src-tauri/capabilities/main.json", commands, "main"),
        ("app/src-tauri/capabilities/dev-sim.json", dev, "main"),
    ] {
        let json: serde_json::Value = serde_json::from_str(&read(file)).unwrap();
        let permissions: Vec<String> = json["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_string())
            .collect();
        assert_eq!(permissions, expected, "{file}");
        assert_eq!(json["windows"], serde_json::json!([windows]), "{file}");
        assert!(
            !permissions.iter().any(|p| p.contains(':')),
            "{file}: no core or plugin permission"
        );
    }
    // Only `main` is enabled in a normal build; the simulator config adds
    // `dev-sim`.
    let conf: serde_json::Value =
        serde_json::from_str(&read("app/src-tauri/tauri.conf.json")).unwrap();
    assert_eq!(
        conf["app"]["security"]["capabilities"],
        serde_json::json!(["main"])
    );
    assert_eq!(conf["app"]["withGlobalTauri"], serde_json::json!(false));
    assert_eq!(
        conf["app"]["security"]["freezePrototype"],
        serde_json::json!(true)
    );
    let csp = conf["app"]["security"]["csp"].as_str().unwrap();
    for part in [
        "default-src 'self'",
        "script-src 'self'",
        "object-src 'none'",
        "connect-src ipc: http://ipc.localhost",
        "frame-ancestors 'none'",
    ] {
        assert!(csp.contains(part), "the CSP lacks {part}");
    }
    assert!(!csp.contains("unsafe"), "no unsafe-* in the release CSP");
    let sim: serde_json::Value =
        serde_json::from_str(&read("app/src-tauri/tauri.sim.conf.json")).unwrap();
    assert_eq!(
        sim["app"]["security"]["capabilities"],
        serde_json::json!(["main", "dev-sim"])
    );
    // The simulator configuration never produces an installer, and only
    // `tauri dev` scripts build with the simulator (a release build with it
    // does not compile).
    assert_eq!(sim["bundle"]["active"], serde_json::json!(false));
    let package: serde_json::Value = serde_json::from_str(&read("app/package.json")).unwrap();
    for (name, script) in package["scripts"].as_object().unwrap() {
        let script = script.as_str().unwrap();
        if script.contains("sim") {
            assert!(
                script.starts_with("tauri dev "),
                "script {name} uses the simulator outside `tauri dev`: {script}"
            );
        }
    }
}

/// Splits `text` at commas outside `<>`, `()` and `[]`.
fn split_top_level(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in text.chars() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    parts.push(current);
    parts
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// The index of the bracket that closes the one `text` starts with.
fn closing(text: &str, open: char, close: char) -> usize {
    let mut depth = 0;
    for (i, c) in text.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
    }
    panic!("unbalanced {open}{close} in {text}");
}

/// A Rust command's parameters: `(name, type)`.
type Params = Vec<(String, String)>;

/// Every `#[tauri::command]` function of `commands.rs` with its parameters.
fn rust_commands(source: &str) -> Vec<(String, Params)> {
    let mut commands = Vec::new();
    for chunk in source.split("#[tauri::command]").skip(1) {
        let at = chunk
            .find("fn ")
            .expect("a function after #[tauri::command]");
        let rest = &chunk[at + 3..];
        let name_end = rest.find(['<', '(']).expect("a parameter list");
        let name = rest[..name_end].trim().to_string();
        let mut rest = &rest[name_end..];
        if rest.starts_with('<') {
            rest = &rest[closing(rest, '<', '>') + 1..];
        }
        assert!(rest.starts_with('('), "{name}: parameters");
        let close = closing(rest, '(', ')');
        let params = split_top_level(&rest[1..close])
            .into_iter()
            .map(|p| {
                let (name, ty) = p.split_once(':').expect("name: Type");
                (name.trim().to_string(), ty.trim().to_string())
            })
            .collect();
        commands.push((name, params));
    }
    commands
}

/// Parameters Tauri fills in itself (not part of the IPC arguments).
fn injected(ty: &str) -> bool {
    [
        "State<",
        "tauri::State<",
        "AppHandle",
        "WebviewWindow",
        "Window",
        "Webview",
        "Worker<",
    ]
    .iter()
    .any(|prefix| ty.starts_with(prefix))
}

/// The argument keys of each command in `commands.ts`' `Args` type.
fn ts_args(ts: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let start = "export type Args = {";
    let at = ts.find(start).expect("Args");
    let rest = &ts[at + start.len()..];
    let body = &rest[..rest.find("\n};").expect("end of Args")];
    let mut args = std::collections::BTreeMap::new();
    for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (name, value) = line.split_once(':').expect("name: value");
        let value = value.trim().trim_end_matches(';').trim();
        let keys = if value == "Record<string, never>" {
            Vec::new()
        } else {
            let inner = value
                .strip_prefix('{')
                .and_then(|v| v.strip_suffix('}'))
                .unwrap_or_else(|| panic!("{name}: an object type"));
            inner
                .split(';')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| p.split_once(':').expect("key: Type").0.trim().to_string())
                .collect()
        };
        args.insert(name.trim().to_string(), keys);
    }
    args
}

/// Every command name of `command_names.rs` (both lists).
fn names_list(source: &str) -> Vec<String> {
    let mut all = names(source, "pub const COMMANDS: &[&str] = &[", "];");
    all.extend(names(source, "pub const DEV_COMMANDS: &[&str] = &[", "];"));
    all
}

#[test]
fn argument_names_match_the_rust_commands() {
    let rust = rust_commands(&read("app/src-tauri/src/commands.rs"));
    let mut all = names_list(&read("app/src-tauri/src/command_names.rs"));
    all.sort();
    let mut declared: Vec<String> = rust.iter().map(|(n, _)| n.clone()).collect();
    declared.sort();
    assert_eq!(declared, all, "every command is a #[tauri::command]");

    let ts = read("app/src/ipc/commands.ts");
    let ts_args = ts_args(&ts);
    let progress_commands: std::collections::BTreeSet<String> = {
        let start = "export type ProgressCommand =";
        let at = ts.find(start).expect("ProgressCommand");
        let rest = &ts[at + start.len()..];
        rest[..rest.find(';').expect("end of ProgressCommand")]
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect()
    };
    let mut with_progress = std::collections::BTreeSet::new();
    for (command, params) in &rust {
        let mut keys = Vec::new();
        for (name, ty) in params {
            if injected(ty) {
                continue;
            }
            assert!(
                !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase()),
                "{command}: argument `{name}` must be one lowercase word (Tauri converts the case)"
            );
            if ty.starts_with("Channel<") {
                if ty.contains("ProgressEvent") {
                    assert_eq!(name, "progress", "{command}: the progress channel");
                    with_progress.insert(command.clone());
                } else {
                    assert!(
                        matches!(
                            (command.as_str(), name.as_str()),
                            ("subscribe", "channel") | ("start_depth_view", "channel")
                        ),
                        "{command}: a channel other than progress"
                    );
                }
                continue;
            }
            keys.push(name.clone());
        }
        let mut expected = ts_args
            .get(command)
            .unwrap_or_else(|| panic!("{command} has no Args entry"))
            .clone();
        expected.sort();
        keys.sort();
        assert_eq!(keys, expected, "{command}: argument names in commands.ts");
    }
    assert_eq!(
        with_progress, progress_commands,
        "ProgressCommand lists exactly the commands with a progress channel"
    );
}

#[test]
fn the_nsis_uninstaller_removes_the_start_entry() {
    let conf: serde_json::Value =
        serde_json::from_str(&read("app/src-tauri/tauri.conf.json")).unwrap();
    let nsis = &conf["bundle"]["windows"]["nsis"];
    // Per user: the uninstaller runs as the user, so HKCU is that user's.
    assert_eq!(nsis["installMode"], serde_json::json!("currentUser"));
    assert_eq!(
        nsis["installerHooks"],
        serde_json::json!("./windows/hooks.nsh")
    );
    let hook = read("app/src-tauri/windows/hooks.nsh");
    let lines: Vec<&str> = hook
        .lines()
        .map(|line| line.split(';').next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();
    let code = lines.join("\n");
    // The same key paths and value name as the app's `autostart.rs`.
    let autostart = read("app/src-tauri/src/autostart.rs");
    let run = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let approved = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    assert!(autostart.contains(&format!("pub const RUN_KEY: &str = r\"{run}\";")));
    assert!(autostart.contains(&format!("r\"{approved}\";")));
    assert!(autostart.contains("pub const VALUE_NAME: &str = \"OpenWraith\";"));
    let start = lines
        .iter()
        .position(|line| *line == "!macro NSIS_HOOK_POSTUNINSTALL")
        .expect("hooks.nsh defines NSIS_HOOK_POSTUNINSTALL");
    assert_eq!(
        lines[start + 1],
        "!if \"${INSTALLMODE}\" != \"currentUser\""
    );
    // Each deletion with the conditions it runs under (`${If}` blocks).
    let mut open: Vec<Vec<String>> = Vec::new();
    let mut deletes = Vec::new();
    let mut ended = false;
    for line in &lines[start + 1..] {
        if *line == "!macroend" {
            ended = true;
            break;
        }
        if let Some(condition) = line.strip_prefix("${If} ") {
            open.push(vec![condition.to_string()]);
        } else if let Some(condition) = line.strip_prefix("${AndIf} ") {
            open.last_mut()
                .expect("${AndIf} in an ${If}")
                .push(condition.to_string());
        } else if *line == "${EndIf}" {
            open.pop().expect("${EndIf} of an ${If}");
        } else if line.starts_with("${") {
            panic!("hooks.nsh: {line} (only ${{If}}, ${{AndIf}} and ${{EndIf}} are checked)");
        } else if line.starts_with("DeleteReg") {
            deletes.push((line.to_string(), open.concat()));
        }
    }
    assert!(
        ended && open.is_empty(),
        "hooks.nsh: the macro's blocks end"
    );
    // The Run value on every uninstall but an update's. Task Manager's
    // choice only with the settings ("Delete the application data"): a
    // newer setup program uninstalls the old version first without
    // `/UPDATE` and keeps the settings, and the new version writes the Run
    // value again, which must not turn a start the user disabled back on.
    let update = "$UpdateMode <> 1".to_string();
    let app_data = "$DeleteAppDataCheckboxState = 1".to_string();
    assert_eq!(
        deletes,
        [
            (
                format!("DeleteRegValue HKCU \"{run}\" \"OpenWraith\""),
                vec![update.clone()]
            ),
            (
                format!("DeleteRegValue HKCU \"{approved}\" \"OpenWraith\""),
                vec![update, app_data]
            ),
        ]
    );
    // Nothing else is deleted or written, and no other hive is touched.
    assert_eq!(code.matches("DeleteReg").count(), 2);
    for token in ["WriteReg", "HKLM", "SHCTX", "HKEY_", "DeleteRegKey", "Exec"] {
        assert!(!code.contains(token), "hooks.nsh uses {token}");
    }
}

#[test]
fn the_nsis_hook_reads_the_variables_of_tauris_template() {
    // The installer template is inside the Tauri CLI that npm installed
    // (`@tauri-apps/cli-<platform>`); a Rust-only checkout has none.
    let cli = repo_root().join("app/node_modules/@tauri-apps");
    let binaries: Vec<_> = std::fs::read_dir(&cli)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|dir| dir.file_name().to_string_lossy().starts_with("cli-"))
        .filter_map(|dir| std::fs::read_dir(dir.path()).ok())
        .flatten()
        .flatten()
        .map(|file| file.path())
        .filter(|path| path.extension().is_some_and(|e| e == "node"))
        .collect();
    if cli.join("cli").exists() {
        assert!(!binaries.is_empty(), "the Tauri CLI's platform package");
    }
    for binary in binaries {
        let bytes = std::fs::read(&binary).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let at = text
            .find("Section Uninstall")
            .unwrap_or_else(|| panic!("{}: no NSIS template", binary.display()));
        let from = text[..at].rfind('\0').map_or(0, |i| i + 1);
        let to = text[at..].find('\0').map_or(text.len(), |i| at + i);
        let template = &text[from..to];
        let uninit = &template[template.find("Function un.onInit").unwrap()..];
        let uninit = &uninit[..uninit.find("FunctionEnd").unwrap()];
        assert!(uninit.contains("${GetOptions} $CMDLINE \"/UPDATE\" $UpdateMode"));
        assert!(template.contains("Var DeleteAppDataCheckboxState"));
        assert!(template.contains("BM_GETCHECK} 0 0 $DeleteAppDataCheckboxState"));
        // Tauri deletes the app data (the settings) under the condition the
        // hook adds to the StartupApproved deletion, and runs the hook after.
        let section = &template[template.find("Section Uninstall").unwrap()..];
        let section = &section[..section.find("SectionEnd").unwrap()];
        let compact: String = section.split_whitespace().collect::<Vec<_>>().join(" ");
        let app_data = compact
            .find("${If} $DeleteAppDataCheckboxState = 1 ${AndIf} $UpdateMode <> 1")
            .expect("Tauri's deletion of the app data");
        assert!(compact[app_data..].contains("RmDir /r \"$APPDATA\\${BUNDLEID}\""));
        let hook = compact
            .find("!insertmacro NSIS_HOOK_POSTUNINSTALL")
            .expect("the hook");
        assert!(app_data < hook);
    }
}

#[test]
fn the_installers_name_the_project_and_ship_the_licenses() {
    let conf: serde_json::Value =
        serde_json::from_str(&read("app/src-tauri/tauri.conf.json")).unwrap();
    let bundle = &conf["bundle"];
    // Without a publisher, Tauri takes the identifier's second part
    // ("github") as the publisher of the installers and the executable.
    let publisher = bundle["publisher"].as_str().unwrap_or_default();
    assert!(!publisher.trim().is_empty(), "bundle.publisher is set");
    assert_ne!(publisher, "github");
    let cargo = read("Cargo.toml");
    let homepage = bundle["homepage"].as_str().unwrap_or_default();
    assert!(
        cargo.contains(&format!("repository = \"{homepage}\"")),
        "bundle.homepage is the repository"
    );
    assert!(
        bundle["copyright"]
            .as_str()
            .is_some_and(|c| c.starts_with("Copyright")),
        "bundle.copyright is a copyright line"
    );
    // The MSI upgrade code must never change (it was derived from the
    // product name before it was pinned).
    assert_eq!(
        bundle["windows"]["wix"]["upgradeCode"],
        serde_json::json!("faa01cb5-c09e-5745-8cee-10ba5a17f5a6")
    );
    assert!(
        bundle["windows"]["nsis"]["installerIcon"]
            .as_str()
            .is_some_and(|i| !i.is_empty())
    );
    let resources = bundle["resources"].as_object().expect("resources map");
    let targets: Vec<&str> = resources.values().filter_map(|v| v.as_str()).collect();
    assert!(
        targets.contains(&"LICENSE.txt"),
        "the GPL ships with the app"
    );
    assert!(
        targets.contains(&"THIRD-PARTY-NOTICES.txt"),
        "the third-party notices ship with the app"
    );
}
