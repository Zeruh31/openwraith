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
    assert_eq!(commands.len(), 32);
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
                    assert_eq!((command.as_str(), name.as_str()), ("subscribe", "channel"));
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
