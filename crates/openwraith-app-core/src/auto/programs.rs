//! Programs for the per-game rules: the tokens of programs the app offered
//! (paths stay in Rust), the warning for generic program names, the
//! running-programs list, and the conversion of the Games table's rules into
//! the stored rules and the engine's.

use std::collections::{BTreeMap, VecDeque};

use super::engine::{EngineConfig, Rule, normalize_folder};
use crate::dto::{
    AppError, AutoProfileRule, AutoProfileRuleView, AutoProfileRulesIntent, AutoProfileSettings,
    EditProblemView, ProgramToken, ProgramView, RuleProgramIntent,
};
use crate::map::clean;
use crate::shell::RunningProgram;

/// At most this many program tokens are remembered (oldest first out).
const MAX_PROGRAMS: usize = 512;

/// At most this many running programs are listed.
const MAX_RUNNING: usize = 300;

/// Program names many unrelated programs share: a rule for one of them
/// switches for all of them (the Games table warns).
pub(crate) const GENERIC_NAMES: &[&str] = &[
    "javaw.exe",
    "java.exe",
    "python.exe",
    "pythonw.exe",
    "py.exe",
    "node.exe",
    "electron.exe",
    "dotnet.exe",
    "steam.exe",
    "steamwebhelper.exe",
    "epicgameslauncher.exe",
    "battle.net.exe",
    "eadesktop.exe",
    "galaxyclient.exe",
    "launcher.exe",
    "game.exe",
    "start.exe",
    "explorer.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "conhost.exe",
    "windowsterminal.exe",
    "rundll32.exe",
    "dllhost.exe",
    "svchost.exe",
    "applicationframehost.exe",
    "wscript.exe",
    "mshta.exe",
    "msedge.exe",
    "chrome.exe",
    "firefox.exe",
];

/// Whether `exe` is a generic program name.
pub(crate) fn is_generic(exe: &str) -> bool {
    let exe = exe.trim().to_lowercase();
    GENERIC_NAMES.contains(&exe.as_str())
}

/// A program the app knows by token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnownProgram {
    /// The file name.
    pub(crate) exe: String,
    /// The folder, if known.
    pub(crate) folder: Option<String>,
}

/// Token -> program.
#[derive(Debug, Default)]
pub(crate) struct ProgramRegistry {
    entries: VecDeque<(ProgramToken, KnownProgram)>,
}

/// The file name and folder of a path (`\` or `/`).
pub(crate) fn split_path(path: &str) -> (Option<String>, String) {
    let at = path.rfind(['\\', '/']);
    match at {
        Some(at) => (
            Some(path[..at].to_string()).filter(|f| !f.is_empty()),
            path[at + 1..].to_string(),
        ),
        None => (None, path.to_string()),
    }
}

impl ProgramRegistry {
    /// The token of a program (the same program keeps its token).
    pub(crate) fn register(
        &mut self,
        mint: &mut crate::tokens::TokenMint,
        program: KnownProgram,
    ) -> ProgramToken {
        if let Some((token, _)) = self.entries.iter().find(|(_, p)| *p == program) {
            return token.clone();
        }
        let token = mint.program();
        self.entries.push_back((token.clone(), program));
        while self.entries.len() > MAX_PROGRAMS {
            self.entries.pop_front();
        }
        token
    }

    /// The program of `token`.
    pub(crate) fn get(&self, token: &ProgramToken) -> Option<KnownProgram> {
        self.entries
            .iter()
            .find(|(t, _)| t == token)
            .map(|(_, p)| p.clone())
    }

    /// The view of a program, registering it.
    pub(crate) fn view(
        &mut self,
        mint: &mut crate::tokens::TokenMint,
        program: KnownProgram,
    ) -> ProgramView {
        let generic_name = is_generic(&program.exe);
        let exe = clean(&program.exe);
        let folder = program.folder.as_deref().map(clean);
        let token = self.register(mint, program);
        ProgramView {
            token,
            exe,
            folder,
            generic_name,
        }
    }
}

/// A program from a path (the file dialog, a running process).
pub(crate) fn program_of(exe: Option<&str>, path: Option<&str>) -> Option<KnownProgram> {
    let (folder, name) = match path {
        Some(path) => {
            let (folder, name) = split_path(path.trim());
            (folder, Some(name))
        }
        None => (None, None),
    };
    let exe = exe
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .or(name)
        .filter(|e| !e.trim().is_empty())?;
    if exe.contains(['/', '\\', ':']) || exe.chars().any(char::is_control) {
        return None;
    }
    Some(KnownProgram { exe, folder })
}

/// The running programs worth offering: Windows programs (`.exe`), not
/// OpenWraith itself, each program once (by folder and name), sorted by
/// name; at most [`MAX_RUNNING`].
pub(crate) fn running(programs: Vec<RunningProgram>, own_pid: u32) -> Vec<KnownProgram> {
    let mut seen = BTreeMap::new();
    for p in programs {
        if p.pid == own_pid || !p.exe.to_lowercase().ends_with(".exe") {
            continue;
        }
        let Some(program) = program_of(Some(&p.exe), p.path.as_deref()) else {
            continue;
        };
        let key = (
            program.exe.to_lowercase(),
            program.folder.as_deref().map(normalize_folder),
        );
        seen.entry(key).or_insert(program);
    }
    seen.into_values().take(MAX_RUNNING).collect()
}

/// The `invalid_request` of a rule problem.
fn problem(message: String) -> AppError {
    crate::map::editors::refused(
        EditProblemView::Other { message },
        "the game rules were not saved",
    )
}

/// The next free rule id (`r-<n>`).
fn next_id(used: &[String]) -> String {
    let mut n = 1u32;
    loop {
        let id = format!("r-{n}");
        if !used.contains(&id) {
            return id;
        }
        n += 1;
    }
}

/// A program name the user typed: trimmed, and `<name>.exe` when it has no
/// extension (rules compare the whole file name, so "VALORANT" would never
/// match `VALORANT.exe`).
pub(crate) fn typed_name(exe: &str) -> String {
    let exe = exe.trim();
    let has_extension = exe
        .rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && !ext.is_empty());
    if exe.is_empty() || has_extension {
        exe.to_string()
    } else {
        format!("{exe}.exe")
    }
}

/// The rules the Games table sent, as stored: programs from their tokens
/// (or the existing rule, or a typed file name), new ids for new rules.
pub(crate) fn rules_from_intent(
    intent: &AutoProfileRulesIntent,
    stored: &AutoProfileSettings,
    registry: &ProgramRegistry,
) -> Result<AutoProfileSettings, AppError> {
    let mut used: Vec<String> = intent.rules.iter().filter_map(|r| r.id.clone()).collect();
    let mut rules = Vec::new();
    for rule in &intent.rules {
        let existing = rule
            .id
            .as_ref()
            .and_then(|id| stored.rules.iter().find(|r| r.id == *id));
        let (exe, path_hint) = match &rule.program {
            RuleProgramIntent::Keep => {
                let existing = existing.ok_or_else(|| {
                    problem(format!(
                        "rule {:?} keeps a program but no such rule exists",
                        rule.id
                    ))
                })?;
                (existing.exe.clone(), existing.path_hint.clone())
            }
            RuleProgramIntent::Picked { token } => {
                let program = registry
                    .get(token)
                    .ok_or_else(|| problem("the program is not known any more".to_string()))?;
                (program.exe, program.folder)
            }
            RuleProgramIntent::Name { exe } => (typed_name(exe), None),
        };
        let id = match &rule.id {
            Some(id) => id.clone(),
            None => {
                let id = next_id(&used);
                used.push(id.clone());
                id
            }
        };
        rules.push(AutoProfileRule {
            id,
            label: rule.label.clone(),
            exe,
            path_hint,
            profile: rule.profile,
            enabled: rule.enabled,
        });
    }
    crate::settings::validate_auto_profile(&AutoProfileSettings {
        enabled: intent.enabled,
        default_profile: intent.default_profile,
        rules,
    })
}

/// The engine's configuration of the stored rules (only rules that are on).
pub(crate) fn engine_config(settings: &AutoProfileSettings) -> EngineConfig {
    EngineConfig {
        enabled: settings.enabled,
        default_profile: settings.default_profile,
        rules: settings
            .rules
            .iter()
            .filter(|r| r.enabled)
            .map(|r| Rule {
                id: r.id.clone(),
                label: r.label.clone(),
                exe: r.exe.trim().to_lowercase(),
                folder: r
                    .path_hint
                    .as_deref()
                    .map(normalize_folder)
                    .filter(|f| !f.is_empty()),
                profile: r.profile,
            })
            .collect(),
    }
}

/// The Games table's view of a stored rule.
pub(crate) fn rule_view(rule: &AutoProfileRule) -> AutoProfileRuleView {
    AutoProfileRuleView {
        id: rule.id.clone(),
        label: clean(&rule.label),
        exe: clean(&rule.exe),
        folder: rule.path_hint.as_deref().map(clean),
        profile: rule.profile,
        enabled: rule.enabled,
        generic_name: is_generic(&rule.exe),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::AutoProfileRuleIntent;

    #[test]
    fn generic_names_are_recognised_in_any_case() {
        assert!(is_generic("JavaW.exe"));
        assert!(is_generic(" steam.exe "));
        assert!(!is_generic("cs2.exe"));
    }

    #[test]
    fn running_programs_are_filtered_and_unique() {
        let p = |pid, exe: &str, path: Option<&str>| RunningProgram {
            pid,
            exe: exe.to_string(),
            path: path.map(str::to_string),
        };
        let list = running(
            vec![
                p(1, "openwraith-app.exe", Some(r"C:\OW\openwraith-app.exe")),
                p(2, "b.exe", Some(r"C:\B\b.exe")),
                p(3, "a.exe", Some(r"C:\A\a.exe")),
                p(4, "a.exe", Some(r"c:\a\A.exe")),
                p(5, "a.exe", None),
                p(6, "kworker", None),
            ],
            1,
        );
        let names: Vec<_> = list
            .iter()
            .map(|k| (k.exe.clone(), k.folder.clone()))
            .collect();
        assert_eq!(
            names,
            [
                ("a.exe".to_string(), None),
                ("a.exe".to_string(), Some(r"C:\A".to_string())),
                ("b.exe".to_string(), Some(r"C:\B".to_string())),
            ]
        );
    }

    #[test]
    fn rules_come_from_tokens_names_and_existing_rules() {
        let mut mint = crate::tokens::TokenMint::new(Some(1));
        let mut registry = ProgramRegistry::default();
        let token = registry.register(
            &mut mint,
            KnownProgram {
                exe: "cs2.exe".to_string(),
                folder: Some(r"C:\Steam\cs2".to_string()),
            },
        );
        assert!(token.0.starts_with("g-"));
        let stored = AutoProfileSettings {
            enabled: true,
            default_profile: Some(1),
            rules: vec![AutoProfileRule {
                id: "r-1".to_string(),
                label: "Old".to_string(),
                exe: "old.exe".to_string(),
                path_hint: Some(r"D:\Old".to_string()),
                profile: 3,
                enabled: true,
            }],
        };
        let intent = AutoProfileRulesIntent {
            enabled: true,
            default_profile: Some(1),
            rules: vec![
                AutoProfileRuleIntent {
                    id: Some("r-1".to_string()),
                    label: "Old game".to_string(),
                    program: RuleProgramIntent::Keep,
                    profile: 4,
                    enabled: false,
                },
                AutoProfileRuleIntent {
                    id: None,
                    label: "CS2".to_string(),
                    program: RuleProgramIntent::Picked { token },
                    profile: 2,
                    enabled: true,
                },
                AutoProfileRuleIntent {
                    id: None,
                    label: "Typed".to_string(),
                    program: RuleProgramIntent::Name {
                        exe: "typed.exe".to_string(),
                    },
                    profile: 3,
                    enabled: true,
                },
            ],
        };
        let saved = rules_from_intent(&intent, &stored, &registry).unwrap();
        let ids: Vec<_> = saved.rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["r-1", "r-2", "r-3"]);
        assert_eq!(saved.rules[0].path_hint.as_deref(), Some(r"D:\Old"));
        assert_eq!(saved.rules[1].path_hint.as_deref(), Some(r"C:\Steam\cs2"));
        assert_eq!(saved.rules[2].path_hint, None);
        // A typed name without an extension is the `.exe` of that name.
        assert_eq!(typed_name(" VALORANT "), "VALORANT.exe");
        assert_eq!(typed_name("game.EXE"), "game.EXE");
        assert_eq!(typed_name("javaw.exe"), "javaw.exe");
        assert_eq!(typed_name("tool.bin"), "tool.bin");
        let config = engine_config(&saved);
        assert_eq!(
            config.rules.len(),
            2,
            "rules that are off are not the engine's"
        );
        assert_eq!(config.rules[0].folder.as_deref(), Some(r"c:\steam\cs2"));

        let unknown = AutoProfileRulesIntent {
            rules: vec![AutoProfileRuleIntent {
                id: None,
                label: "X".to_string(),
                program: RuleProgramIntent::Picked {
                    token: ProgramToken("g-0000000000000000".to_string()),
                },
                profile: 2,
                enabled: true,
            }],
            ..intent.clone()
        };
        assert!(rules_from_intent(&unknown, &stored, &registry).is_err());
        let bad_name = AutoProfileRulesIntent {
            rules: vec![AutoProfileRuleIntent {
                id: None,
                label: "X".to_string(),
                program: RuleProgramIntent::Name {
                    exe: r"C:\x.exe".to_string(),
                },
                profile: 2,
                enabled: true,
            }],
            ..intent
        };
        assert!(rules_from_intent(&bad_name, &stored, &registry).is_err());
    }
}
