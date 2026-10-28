//! The texts the Rust glue shows outside the window: the notification-area
//! menu and its tooltip, and the Windows notifications (design B5, B10).
//!
//! The table below is the source. Its mirror for translators and for the
//! frontend is generated into `app/src/i18n/en/native.ts` and
//! `app/src/i18n/tr/native.ts` by this module's test
//! (`UPDATE_NATIVE_TEXTS=1 cargo test -p openwraith-app texts`); the test fails
//! when the mirror is out of date. `{name}` marks a value filled in here.
//!
//! The language follows the app's language setting (`system` = the
//! operating system's locale: Turkish locales give Turkish, every other
//! locale English, as in the frontend's `resolveLang`).

use std::collections::BTreeMap;

use openwraith_app_core::dto::LanguageSetting;
use tauri::{AppHandle, Manager, Runtime};

use crate::commands::SettingsState;

/// A language of the texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// English.
    En,
    /// Turkish.
    Tr,
}

/// The language for a setting and the operating system's locale.
pub fn resolve(setting: LanguageSetting, os_locale: Option<&str>) -> Lang {
    match setting {
        LanguageSetting::En => Lang::En,
        LanguageSetting::Tr => Lang::Tr,
        LanguageSetting::System => {
            if os_locale.is_some_and(|l| l.to_ascii_lowercase().starts_with("tr")) {
                Lang::Tr
            } else {
                Lang::En
            }
        }
    }
}

/// The language the app uses now (English before the settings are loaded).
pub fn current<R: Runtime>(app: &AppHandle<R>) -> Lang {
    let setting = app
        .try_state::<SettingsState>()
        .and_then(|state| state.value.lock().ok().map(|s| s.language))
        .unwrap_or_default();
    resolve(setting, sys_locale::get_locale().as_deref())
}

/// Key, English, Turkish.
const TEXTS: &[(&str, &str, &str)] = &[
    // ----- the notification-area icon and its menu -----
    ("tray.appName", "OpenWraith", "OpenWraith"),
    ("tray.connected", "Keyboard connected", "Klavye bağlı"),
    (
        "tray.notConnected",
        "Keyboard not connected",
        "Klavye bağlı değil",
    ),
    ("tray.profile", "Profile {profile}", "Profil {profile}"),
    (
        "tray.profileNamed",
        "Profile {profile} ({name})",
        "Profil {profile} ({name})",
    ),
    (
        "tray.pauseGames",
        "Pause game profiles",
        "Oyun profillerini duraklat",
    ),
    ("tray.backUpNow", "Back up now", "Şimdi yedekle"),
    (
        "tray.attention.reset",
        "Keyboard reset — review…",
        "Klavye sıfırlandı — gözden geçirin…",
    ),
    (
        "tray.attention.otherProgram",
        "Another program was using the keyboard — check again",
        "Klavyeyi başka bir program kullanıyordu — yeniden denetleyin",
    ),
    (
        "tray.attention.travel",
        "A key-depth test may still be running — stop it",
        "Bir tuş derinliği testi hâlâ çalışıyor olabilir — durdurun",
    ),
    (
        "tray.attention.autoPaused",
        "Game profiles paused — review…",
        "Oyun profilleri duraklatıldı — gözden geçirin…",
    ),
    ("tray.open", "Open OpenWraith", "OpenWraith'i aç"),
    ("tray.quit", "Quit", "Çık"),
    (
        "tray.needsAttention",
        "{status} — needs your attention",
        "{status} — dikkatinizi gerektiriyor",
    ),
    (
        "tray.closeExplained.title",
        "OpenWraith is still running",
        "OpenWraith çalışmaya devam ediyor",
    ),
    (
        "tray.closeExplained.body",
        "Closing the window keeps it in the notification area. To quit, choose Quit in the tray icon's menu.",
        "Pencereyi kapatınca bildirim alanında çalışmaya devam eder. Çıkmak için tepsi simgesinin menüsünden Çık'ı seçin.",
    ),
    // ----- Windows notifications -----
    (
        "notifications.keyboardReset.title",
        "The keyboard was reset",
        "Klavye sıfırlandı",
    ),
    (
        "notifications.keyboardReset.body",
        "It reported a reset (Fn + Space held). OpenWraith never restores anything without asking.",
        "Klavye bir sıfırlama bildirdi (Fn + Boşluk basılı tutuldu). OpenWraith sormadan hiçbir şeyi geri yüklemez.",
    ),
    (
        "notifications.resetSuspected.title",
        "Profile {profile} looks reset",
        "Profil {profile} sıfırlanmış görünüyor",
    ),
    (
        "notifications.resetSuspected.body",
        "Its settings now look like the factory settings. Open OpenWraith to compare them with the automatic backup.",
        "Ayarları artık fabrika ayarlarına benziyor. Otomatik yedekle karşılaştırmak için OpenWraith'i açın.",
    ),
    (
        "notifications.firmwareChanged.title",
        "The keyboard's firmware changed",
        "Klavyenin firmware sürümü değişti",
    ),
    (
        "notifications.firmwareChanged.body",
        "It now runs firmware {current} (before: {known}). An update can reset the settings: open OpenWraith to check them.",
        "Klavyede artık firmware {current} var (önceden: {known}). Bir güncelleme ayarları sıfırlayabilir: denetlemek için OpenWraith'i açın.",
    ),
    (
        "notifications.autoBackupFailed.title",
        "Automatic backup failed",
        "Otomatik yedekleme yapılamadı",
    ),
    (
        "notifications.autoBackupFailed.body",
        "OpenWraith couldn't save the automatic backup of profile {profile}.",
        "OpenWraith, Profil {profile} için otomatik yedeği kaydedemedi.",
    ),
    (
        "notifications.autoSwitched.title",
        "Profile {profile} is active",
        "Profil {profile} etkin",
    ),
    (
        "notifications.autoSwitched.body",
        "{label} is in front, so OpenWraith switched to its profile.",
        "{label} öne geldiği için OpenWraith onun profiline geçti.",
    ),
    (
        "notifications.autoSwitchedBack.title",
        "Profile {profile} is active",
        "Profil {profile} etkin",
    ),
    (
        "notifications.autoSwitchedBack.body",
        "The game closed, so OpenWraith changed the profile.",
        "Oyun kapandığı için OpenWraith profili değiştirdi.",
    ),
    (
        "notifications.autoSwitchWaiting.title",
        "A profile switch is waiting",
        "Bir profil geçişi bekliyor",
    ),
    (
        "notifications.autoSwitchWaiting.body",
        "{label} uses profile {profile}, but OpenWraith has unwritten changes. Write or discard them to switch.",
        "{label} Profil {profile} kullanıyor, ancak OpenWraith'te yazılmamış değişiklikler var. Geçiş için onları yazın ya da onlardan vazgeçin.",
    ),
    (
        "notifications.autoSwitchSkipped.title",
        "A profile switch was skipped",
        "Bir profil geçişi atlandı",
    ),
    (
        "notifications.autoSwitchSkipped.body",
        "{label} uses profile {profile}, but OpenWraith couldn't switch within 30 seconds: {reason}",
        "{label} Profil {profile} kullanıyor, ancak OpenWraith 30 saniye içinde geçiş yapamadı: {reason}",
    ),
    (
        "notifications.autoPausedRateLimit.title",
        "Game profiles paused",
        "Oyun profilleri duraklatıldı",
    ),
    (
        "notifications.autoPausedRateLimit.body",
        "More than 30 automatic switches in an hour. Resume them in Settings > Games or untick “Pause game profiles” in the notification-area menu.",
        "Bir saat içinde 30'dan fazla otomatik geçiş oldu. Ayarlar > Oyunlar bölümünden ya da bildirim alanı menüsünde “Oyun profillerini duraklat” işaretini kaldırarak sürdürün.",
    ),
    (
        "notifications.autoPausedFailed.title",
        "Game profiles paused",
        "Oyun profilleri duraklatıldı",
    ),
    (
        "notifications.autoPausedFailed.body",
        "The keyboard didn't take an automatic profile switch. Check the keyboard, then resume them in Settings > Games or from the notification-area menu.",
        "Klavye otomatik bir profil geçişini almadı. Klavyeyi denetleyin, sonra Ayarlar > Oyunlar bölümünden ya da bildirim alanı menüsünden sürdürün.",
    ),
    ("notifications.aGame", "A game", "Bir oyun"),
    ("notifications.versionUnknown", "unknown", "bilinmiyor"),
    // Why a game's switch was skipped (`AutoWaitReason`, snake_case).
    (
        "notifications.reasons.not_connected",
        "the keyboard is not connected.",
        "klavye bağlı değil.",
    ),
    (
        "notifications.reasons.connecting",
        "the keyboard was still connecting.",
        "klavye hâlâ bağlanıyordu.",
    ),
    (
        "notifications.reasons.busy",
        "another task was running.",
        "başka bir işlem sürüyordu.",
    ),
    (
        "notifications.reasons.plan_pending",
        "a change was waiting for your review.",
        "bir değişiklik onayınızı bekliyordu.",
    ),
    (
        "notifications.reasons.drafts",
        "a page has unwritten changes.",
        "bir sayfada yazılmamış değişiklikler var.",
    ),
    (
        "notifications.reasons.depth_view",
        "the key-depth view was running.",
        "tuş derinliği görünümü çalışıyordu.",
    ),
    (
        "notifications.reasons.travel_test",
        "a key-depth test may still be running.",
        "bir tuş derinliği testi hâlâ çalışıyor olabilir.",
    ),
    (
        "notifications.reasons.other_program",
        "another program was using the keyboard.",
        "klavyeyi başka bir program kullanıyordu.",
    ),
    (
        "notifications.reasons.paused_for_cli",
        "an OpenWraith command was using the keyboard.",
        "klavyeyi bir OpenWraith komutu kullanıyordu.",
    ),
    (
        "notifications.reasons.reset_seen",
        "the keyboard reported a reset.",
        "klavye bir sıfırlama bildirdi.",
    ),
    (
        "notifications.reasons.released",
        "OpenWraith had let go of the keyboard.",
        "OpenWraith klavyeyi bırakmıştı.",
    ),
    (
        "notifications.reasons.fault",
        "the keyboard stopped answering.",
        "klavye yanıt vermeyi bıraktı.",
    ),
    (
        "notifications.reasons.recent_foreign_activity",
        "another program used the keyboard in the last two minutes.",
        "son iki dakika içinde klavyeyi başka bir program kullandı.",
    ),
    (
        "notifications.reasons.keymap_unsaved",
        "the keyboard was still saving key assignments.",
        "klavye tuş atamalarını hâlâ kaydediyordu.",
    ),
    (
        "notifications.reasons.profile_changing",
        "the profile was changing at that moment.",
        "profil o sırada değişiyordu.",
    ),
    (
        "notifications.reasons.other",
        "the keyboard wasn't ready.",
        "klavye hazır değildi.",
    ),
];

/// The text of `key`, if the table has it.
pub fn get(lang: Lang, key: &str) -> Option<&'static str> {
    TEXTS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, en, tr)| match lang {
            Lang::En => *en,
            Lang::Tr => *tr,
        })
}

/// The text of `key`, or `key` itself when the table lacks it (a test
/// makes sure every key the glue asks for exists).
pub fn text(lang: Lang, key: &str) -> String {
    get(lang, key).unwrap_or(key).to_string()
}

/// `template` with every `{name}` replaced by `params[name]` (a name
/// without a value stays as it is).
pub fn fill(template: &str, params: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if is_name(&after[..end]) => {
                let name = &after[..end];
                match params.get(name) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text of `key` with its values filled in.
pub fn render(lang: Lang, key: &str, params: &BTreeMap<String, String>) -> String {
    fill(&text(lang, key), params)
}

/// A placeholder name: ASCII letters only.
fn is_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphabetic())
}

/// The placeholder names of a template, sorted.
#[cfg(test)]
fn placeholders(template: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if is_name(&after[..end]) => {
                names.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            _ => rest = after,
        }
    }
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use openwraith_app_core::dto::{AutoWaitReason, LanguageSetting};

    use super::*;

    #[test]
    fn the_language_follows_the_setting_and_the_locale() {
        assert_eq!(resolve(LanguageSetting::En, Some("tr-TR")), Lang::En);
        assert_eq!(resolve(LanguageSetting::Tr, Some("en-US")), Lang::Tr);
        assert_eq!(resolve(LanguageSetting::System, Some("tr-TR")), Lang::Tr);
        assert_eq!(resolve(LanguageSetting::System, Some("TR")), Lang::Tr);
        assert_eq!(resolve(LanguageSetting::System, Some("de-DE")), Lang::En);
        assert_eq!(resolve(LanguageSetting::System, None), Lang::En);
    }

    #[test]
    fn values_are_filled_in_and_unknown_names_stay() {
        let params: BTreeMap<String, String> = [("profile", "2"), ("label", "CS2")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(fill("Profile {profile}", &params), "Profile 2");
        assert_eq!(
            fill("{label} uses {profile}, {missing}", &params),
            "CS2 uses 2, {missing}"
        );
        // Braces that are no placeholder stay as they are.
        assert_eq!(fill("a { b } {} {x1}", &params), "a { b } {} {x1}");
        assert_eq!(fill("{", &params), "{");
        assert_eq!(fill("Türkçe {profile}…", &params), "Türkçe 2…");
        assert_eq!(placeholders("{b} {a} {b} {x1}"), vec!["a", "b"]);
    }

    #[test]
    fn the_table_is_complete_and_both_languages_match() {
        let mut keys: Vec<&str> = TEXTS.iter().map(|(k, _, _)| *k).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before, "duplicate keys");
        for (key, en, tr) in TEXTS {
            assert!(
                !en.trim().is_empty() && !tr.trim().is_empty(),
                "{key} empty"
            );
            assert_eq!(
                placeholders(en),
                placeholders(tr),
                "{key}: placeholders differ"
            );
            // The frontend's parity test allows identical texts only for
            // names and codes without lowercase letters.
            if en == tr {
                assert!(
                    *en == "OpenWraith" || !en.chars().any(char::is_lowercase),
                    "{key} is not translated"
                );
            }
            // A key is a text or a group, never both.
            let prefix = format!("{key}.");
            assert!(
                !TEXTS.iter().any(|(k, _, _)| k.starts_with(&prefix)),
                "{key} is a text and a group"
            );
        }
    }

    /// Every variant of the wait reasons (a new variant fails to compile
    /// here until it is listed).
    fn wait_reasons() -> Vec<AutoWaitReason> {
        use AutoWaitReason as W;
        let all = vec![
            W::NotConnected,
            W::Connecting,
            W::Busy,
            W::PlanPending,
            W::Drafts,
            W::DepthView,
            W::TravelTest,
            W::OtherProgram,
            W::PausedForCli,
            W::ResetSeen,
            W::Released,
            W::Fault,
            W::RecentForeignActivity,
            W::KeymapUnsaved,
            W::ProfileChanging,
        ];
        for reason in &all {
            match reason {
                W::NotConnected
                | W::Connecting
                | W::Busy
                | W::PlanPending
                | W::Drafts
                | W::DepthView
                | W::TravelTest
                | W::OtherProgram
                | W::PausedForCli
                | W::ResetSeen
                | W::Released
                | W::Fault
                | W::RecentForeignActivity
                | W::KeymapUnsaved
                | W::ProfileChanging => {}
            }
        }
        all
    }

    #[test]
    fn every_key_the_core_sends_has_a_title_and_a_body() {
        // `openwraith_app_core::shell::OsNotification::key` values (contract
        // section 4).
        for key in openwraith_app_core::shell::NOTIFICATION_KEYS
            .iter()
            .copied()
            .chain(["tray.closeExplained"])
        {
            for lang in [Lang::En, Lang::Tr] {
                assert!(get(lang, &format!("{key}.title")).is_some(), "{key}.title");
                assert!(get(lang, &format!("{key}.body")).is_some(), "{key}.body");
            }
        }
        for reason in wait_reasons() {
            let name = serde_json::to_value(reason)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            assert!(
                get(Lang::En, &format!("notifications.reasons.{name}")).is_some(),
                "no text for the wait reason {name}"
            );
        }
    }

    // ----- the generated mirror in the frontend's dictionaries -----

    /// A tree of the dotted keys.
    #[derive(Default)]
    struct Node {
        text: Option<&'static str>,
        children: BTreeMap<&'static str, Node>,
    }

    fn tree(lang: Lang) -> Node {
        let mut root = Node::default();
        for (key, en, tr) in TEXTS {
            let mut node = &mut root;
            for part in key.split('.') {
                node = node.children.entry(part).or_default();
            }
            node.text = Some(match lang {
                Lang::En => en,
                Lang::Tr => tr,
            });
        }
        root
    }

    fn write_node(node: &Node, indent: usize, out: &mut String) {
        for (name, child) in &node.children {
            let pad = "  ".repeat(indent);
            match child.text {
                Some(text) => {
                    let quoted = serde_json::to_string(text).unwrap_or_default();
                    out.push_str(&format!("{pad}{name}: {quoted},\n"));
                }
                None => {
                    out.push_str(&format!("{pad}{name}: {{\n"));
                    write_node(child, indent + 1, out);
                    out.push_str(&format!("{pad}}},\n"));
                }
            }
        }
    }

    const HEADER: &str = "// Generated by the test of app/src-tauri/src/texts.rs: the texts the desktop app\n\
// shows outside its window (the notification-area menu, Windows notifications).\n\
// `{name}` marks a value the app fills in. Edit the Rust table, not this file, then run\n\
// `UPDATE_NATIVE_TEXTS=1 cargo test -p openwraith-app texts`.\n";

    fn english_file() -> String {
        let mut out = format!("{HEADER}\nexport const native = {{\n");
        write_node(&tree(Lang::En), 1, &mut out);
        out.push_str("} as const;\n");
        out
    }

    fn turkish_file() -> String {
        let mut out = format!(
            "{HEADER}\nimport type {{ native as en }} from \"../en/native\";\nimport type {{ Messages }} from \"../types\";\n\nexport const native: Messages<typeof en> = {{\n"
        );
        write_node(&tree(Lang::Tr), 1, &mut out);
        out.push_str("};\n");
        out
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    #[test]
    fn the_dictionary_mirror_is_up_to_date() {
        let update = std::env::var_os("UPDATE_NATIVE_TEXTS").is_some_and(|v| v == "1");
        let dir = repo_root().join("app/src/i18n");
        let mut stale = Vec::new();
        for (lang, content) in [("en", english_file()), ("tr", turkish_file())] {
            let path = dir.join(lang).join("native.ts");
            let current = std::fs::read_to_string(&path).unwrap_or_default();
            // Git may check the file out with CRLF line ends.
            if current.replace("\r\n", "\n") == content {
                continue;
            }
            if update {
                std::fs::write(&path, &content)
                    .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
            } else {
                stale.push(path.display().to_string());
            }
        }
        assert!(
            stale.is_empty(),
            "out of date: {stale:?}; run `UPDATE_NATIVE_TEXTS=1 cargo test -p openwraith-app texts`"
        );
    }
}
