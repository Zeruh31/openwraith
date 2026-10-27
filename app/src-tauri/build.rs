//! Generates the permissions of the app's commands (`allow-<command>`), so
//! that the capabilities can allow exactly these and nothing else.

#[allow(dead_code)]
mod command_names {
    include!("src/command_names.rs");
}

use std::path::Path;

/// The first line of the placeholder notices of a debug build.
const NOTICES_PLACEHOLDER: &str = "OPENWRAITH-NOTICES-PLACEHOLDER";

fn main() {
    // `generate_context!` embeds the frontend; a checkout without a built
    // frontend (Rust-only CI, `cargo test --workspace`) gets a placeholder.
    // `app/dist/` is not committed.
    let dist = Path::new("../dist");
    let index = dist.join("index.html");
    if !index.exists() {
        let placeholder = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
            <title>OpenWraith</title></head><body><p>The frontend was not built \
            (npm --prefix app run build).</p></body></html>\n";
        if let Err(e) =
            std::fs::create_dir_all(dist).and_then(|()| std::fs::write(&index, placeholder))
        {
            println!("cargo:warning=cannot write the frontend placeholder: {e}");
        }
    }
    // The installers ship THIRD-PARTY-NOTICES.txt (`bundle.resources`),
    // written by `npm run build:notices` (part of `npm run build:web`, which
    // `tauri build` runs first). A debug build without it gets a
    // placeholder; a release build fails, so no installer goes out without
    // the notices.
    let release = std::env::var("PROFILE").is_ok_and(|p| p == "release");
    let notices = Path::new("legal").join("THIRD-PARTY-NOTICES.txt");
    let text = std::fs::read_to_string(&notices).unwrap_or_default();
    if text.trim().is_empty() || text.starts_with(NOTICES_PLACEHOLDER) {
        if release {
            eprintln!(
                "{} is missing: run `npm --prefix app run build:web` (or `npm --prefix app run                  tauri build`), which writes it",
                notices.display()
            );
            std::process::exit(1);
        }
        let placeholder = format!(
            "{NOTICES_PLACEHOLDER}
The third-party notices were not generated (debug build).              Run `npm --prefix app run build:notices`.
"
        );
        if let Err(e) =
            std::fs::create_dir_all("legal").and_then(|()| std::fs::write(&notices, placeholder))
        {
            println!("cargo:warning=cannot write the notices placeholder: {e}");
        }
    }
    println!("cargo:rerun-if-changed=legal/THIRD-PARTY-NOTICES.txt");
    println!("cargo:rerun-if-changed=src/command_names.rs");
    println!("cargo:rerun-if-changed=capabilities");
    println!("cargo:rerun-if-changed=../dist/index.html");

    let all: Vec<&'static str> = command_names::COMMANDS
        .iter()
        .chain(command_names::DEV_COMMANDS)
        .copied()
        .collect();
    let all: &'static [&'static str] = Box::leak(all.into_boxed_slice());
    // The VC runtime is linked statically into release builds (the
    // installer needs no VC++ redistributable). Debug builds link it
    // dynamically: the static variant puts an empty `msvcrt.lib` on the link
    // path of the whole workspace build, which breaks the doctests of the
    // other crates in `cargo test --workspace`.
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new().static_vc_runtime(release))
        .app_manifest(tauri_build::AppManifest::new().commands(all));
    if let Err(error) = tauri_build::try_build(attributes) {
        eprintln!("tauri-build failed: {error:#}");
        std::process::exit(1);
    }
}
