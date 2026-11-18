//! Finds games installed through the Epic Games Launcher.
//!
//! Epic keeps install records in three places depending on launcher version, so
//! all three are read (the same approach Hydra takes): the classic `.item`
//! manifests, the newer launcher's `installations.jsonl`, and the older
//! `LauncherInstalled.dat`. Names and covers come from the launcher's own
//! catalog cache, so no network request is needed.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use cerberus_shared::{Game, Source};
use serde::Deserialize;

/// One installed app, whichever file it came from.
#[derive(Debug, Default, PartialEq)]
struct Install {
    app_name: String,
    namespace: String,
    item_id: String,
    dir: String,
    display_name: String,
    categories: Vec<String>,
    is_addon: bool,
}

pub fn installed_games() -> Vec<Game> {
    let Some(program_data) = program_data() else {
        return Vec::new();
    };
    let epic = program_data.join("Epic");
    let catalog_file = epic.join(r"EpicGamesLauncher\Data\Catalog\catcache.bin");
    let catalog = fs::read_to_string(catalog_file).map(|t| parse_catalog(&t)).unwrap_or_default();

    let mut installs = read_item_manifests(&epic.join(r"EpicGamesLauncher\Data\Manifests"));
    let mut removed = HashSet::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let file = PathBuf::from(local).join(r"com.epicgames.eglv2\installations.jsonl");
        if let Ok(text) = fs::read_to_string(file) {
            let (active, gone) = parse_launcher_v2(&text);
            installs.extend(active);
            removed = gone;
        }
    }
    // The old list can still name games the newer launcher already uninstalled.
    let dat = epic.join(r"UnrealEngineLauncher\LauncherInstalled.dat");
    if let Ok(text) = fs::read_to_string(dat) {
        let listed = parse_launcher_installed(&text);
        installs.extend(listed.into_iter().filter(|i| !removed.contains(&i.app_name)));
    }

    let mut games: Vec<Game> = Vec::new();
    for install in installs {
        let id = format!("epic:{}", install.app_name);
        if games.iter().any(|known| known.id == id) {
            continue;
        }
        let entry = catalog.get(&install.item_id);
        if !is_game(&install, entry) || !Path::new(&install.dir).is_dir() {
            continue;
        }
        games.push(to_game(install, entry, id));
    }
    games
}

pub fn launch_url(namespace: &str, item_id: &str, app_name: &str) -> String {
    // The full form picks the right game when one app name exists in several
    // namespaces; the short one is all we have when the ids are unknown.
    if namespace.is_empty() || item_id.is_empty() {
        return format!("com.epicgames.launcher://apps/{app_name}?action=launch&silent=true");
    }
    format!("com.epicgames.launcher://apps/{namespace}%3A{item_id}%3A{app_name}?action=launch&silent=true")
}

fn to_game(install: Install, entry: Option<&CatalogItem>, id: String) -> Game {
    let catalog_title = entry.map(|e| e.title.clone()).filter(|t| !t.is_empty());
    let name = Some(install.display_name.clone())
        .filter(|n| !n.is_empty())
        .or(catalog_title)
        .unwrap_or_else(|| install.app_name.clone());
    Game {
        id,
        name,
        source: Source::Epic {
            app_name: install.app_name,
            namespace: install.namespace,
            item_id: install.item_id,
        },
        install_dir: install.dir,
        cover_path: None,
        cover_url: entry.and_then(CatalogItem::tall_cover),
        hero_path: None,
        hero_url: entry.and_then(CatalogItem::wide_art),
        playtime_secs: 0,
        last_played: None,
        last_session_secs: None,
        launch_args: String::new(),
        tracking_exe: None,
        game_copy: None,
        setup_pending: false,
        next_step: Default::default(),
        setting_picks: Default::default(),
        external: false,
    }
}

/// Skips DLC, the Unreal Engine and tools such as Twinmotion, which Epic
/// installs the same way it installs games.
fn is_game(install: &Install, entry: Option<&CatalogItem>) -> bool {
    if install.is_addon || install.app_name.starts_with("UE_") {
        return false;
    }
    if !install.categories.is_empty() {
        return install.categories.iter().any(|c| c == "games");
    }
    let Some(entry) = entry else {
        // Nothing says otherwise; showing a stray tool beats hiding a game.
        return true;
    };
    // Base games carry a `mainGameItem` too, with an empty id; only DLC points at another game.
    let is_dlc = entry.main_game_item.as_ref().is_some_and(|main| !main.id.is_empty());
    let paths: Vec<&str> = entry.categories.iter().map(|c| c.path.as_str()).collect();
    !is_dlc && paths.contains(&"games") && !paths.contains(&"addons")
}

fn program_data() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let dir = std::env::var_os("PROGRAMDATA").map(PathBuf::from);
    Some(dir.unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")))
}

/// Epic writes some of these files with a byte-order mark, which JSON parsers reject.
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct ItemManifest {
    display_name: String,
    install_location: String,
    app_name: String,
    catalog_namespace: String,
    catalog_item_id: String,
    main_game_app_name: String,
    app_categories: Vec<String>,
    #[serde(rename = "bIsIncompleteInstall")]
    incomplete: bool,
}

fn read_item_manifests(dir: &Path) -> Vec<Install> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let files = entries.flatten().map(|e| e.path());
    let items = files.filter(|p| p.extension().is_some_and(|ext| ext == "item"));
    let texts = items.filter_map(|p| fs::read_to_string(p).ok());
    texts.filter_map(|text| parse_item_manifest(&text)).collect()
}

fn parse_item_manifest(text: &str) -> Option<Install> {
    let item: ItemManifest = serde_json::from_str(strip_bom(text)).ok()?;
    if item.incomplete || item.app_name.is_empty() || item.install_location.is_empty() {
        return None;
    }
    let is_addon = !item.main_game_app_name.is_empty() && item.main_game_app_name != item.app_name;
    Some(Install {
        is_addon,
        app_name: item.app_name,
        namespace: item.catalog_namespace,
        item_id: item.catalog_item_id,
        dir: item.install_location,
        display_name: item.display_name,
        categories: item.app_categories,
    })
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
struct LauncherRecord {
    installation_id: String,
    artifact_id: String,
    dir: String,
    complete: bool,
    deleted: bool,
    platform: String,
    namespace: String,
    item_id: String,
}

/// The newer launcher appends a line per change, so only each installation's
/// last line counts. Returns the active installs and the app names removed.
fn parse_launcher_v2(text: &str) -> (Vec<Install>, HashSet<String>) {
    let mut latest: HashMap<String, LauncherRecord> = HashMap::new();
    // Later lines may leave the artifact id out, so remember it from any line.
    let mut app_names: HashMap<String, String> = HashMap::new();
    for line in strip_bom(text).lines() {
        let Ok(record) = serde_json::from_str::<LauncherRecord>(line) else {
            continue;
        };
        if record.installation_id.is_empty() {
            continue;
        }
        if !record.artifact_id.is_empty() {
            app_names.insert(record.installation_id.clone(), record.artifact_id.clone());
        }
        latest.insert(record.installation_id.clone(), record);
    }

    let mut active = Vec::new();
    let mut removed = HashSet::new();
    for (installation_id, record) in latest {
        let Some(app_name) = app_names.get(&installation_id).cloned() else {
            continue;
        };
        let windows = record.platform.is_empty() || record.platform == "Windows";
        let installed = !record.deleted && record.complete && !record.dir.is_empty() && windows;
        if !installed {
            removed.insert(app_name);
            continue;
        }
        active.push(Install {
            app_name,
            namespace: record.namespace,
            item_id: record.item_id,
            dir: record.dir,
            ..Install::default()
        });
    }
    (active, removed)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct InstalledList {
    installation_list: Vec<InstalledEntry>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct InstalledEntry {
    app_name: String,
    install_location: String,
    namespace_id: String,
    item_id: String,
}

fn parse_launcher_installed(text: &str) -> Vec<Install> {
    let list: InstalledList = serde_json::from_str(strip_bom(text)).unwrap_or_default();
    let entries = list.installation_list.into_iter();
    let usable = entries.filter(|e| !e.app_name.is_empty() && !e.install_location.is_empty());
    usable
        .map(|entry| Install {
            app_name: entry.app_name,
            namespace: entry.namespace_id,
            item_id: entry.item_id,
            dir: entry.install_location,
            ..Install::default()
        })
        .collect()
}

#[derive(Deserialize)]
struct CatalogItem {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "keyImages")]
    key_images: Vec<KeyImage>,
    #[serde(default)]
    categories: Vec<Category>,
    #[serde(default, rename = "mainGameItem")]
    main_game_item: Option<MainGame>,
}

#[derive(Deserialize)]
struct MainGame {
    #[serde(default)]
    id: String,
}

#[derive(Deserialize)]
struct KeyImage {
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

#[derive(Deserialize)]
struct Category {
    path: String,
}

impl CatalogItem {
    fn tall_cover(&self) -> Option<String> {
        self.first_image(&["DieselGameBoxTall", "OfferImageTall", "Thumbnail"])
    }

    fn wide_art(&self) -> Option<String> {
        self.first_image(&["DieselGameBox", "OfferImageWide", "DieselStoreFrontWide"])
    }

    fn first_image(&self, kinds: &[&str]) -> Option<String> {
        let pick = |kind: &&str| self.key_images.iter().find(|image| image.kind == *kind);
        let image = kinds.iter().find_map(pick)?;
        Some(image.url.clone())
    }
}

/// `catcache.bin` is the launcher's copy of the store catalog: base64 over a JSON array.
fn parse_catalog(text: &str) -> HashMap<String, CatalogItem> {
    let engine = base64::engine::general_purpose::STANDARD;
    let Ok(bytes) = engine.decode(text.trim()) else {
        return HashMap::new();
    };
    let items: Vec<CatalogItem> = serde_json::from_slice(&bytes).unwrap_or_default();
    items.into_iter().map(|item| (item.id.clone(), item)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORTNITE_ITEM: &str = r#"{
        "DisplayName": "Fortnite", "InstallLocation": "D:\\Epic\\Fortnite",
        "AppName": "Fortnite", "CatalogNamespace": "fn", "CatalogItemId": "4fe75bbc",
        "MainGameAppName": "Fortnite", "AppCategories": ["public", "games", "applications"],
        "bIsIncompleteInstall": false
    }"#;

    #[test]
    fn reads_a_classic_manifest() {
        let install = parse_item_manifest(FORTNITE_ITEM).unwrap();
        assert_eq!(install.app_name, "Fortnite");
        assert_eq!(install.dir, r"D:\Epic\Fortnite");
        assert!(is_game(&install, None));
    }

    #[test]
    fn accepts_a_byte_order_mark() {
        let with_bom = format!("\u{feff}{FORTNITE_ITEM}");
        assert!(parse_item_manifest(&with_bom).is_some());
    }

    #[test]
    fn skips_unfinished_installs() {
        let partial = FORTNITE_ITEM.replace(r#""bIsIncompleteInstall": false"#, r#""bIsIncompleteInstall": true"#);
        assert!(parse_item_manifest(&partial).is_none());
    }

    #[test]
    fn treats_other_main_game_as_dlc() {
        let dlc = FORTNITE_ITEM.replace(r#""AppName": "Fortnite""#, r#""AppName": "FortniteDLC""#);
        let install = parse_item_manifest(&dlc).unwrap();
        assert!(!is_game(&install, None));
    }

    #[test]
    fn skips_apps_that_are_not_games() {
        let tool = FORTNITE_ITEM.replace(r#"["public", "games", "applications"]"#, r#"["applications"]"#);
        let install = parse_item_manifest(&tool).unwrap();
        assert!(!is_game(&install, None));
    }

    #[test]
    fn newer_launcher_keeps_the_last_line_per_install() {
        let log = concat!(
            r#"{"installationId":"a","artifactId":"Hades","dir":"C:\\Hades","complete":false}"#, "\n",
            r#"{"installationId":"a","dir":"C:\\Hades","complete":true,"namespace":"ns","itemId":"it"}"#, "\n",
        );
        let (active, removed) = parse_launcher_v2(log);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].app_name, "Hades");
        assert_eq!(active[0].item_id, "it");
        assert!(removed.is_empty());
    }

    #[test]
    fn newer_launcher_reports_uninstalled_games() {
        let log = concat!(
            r#"{"installationId":"a","artifactId":"Hades","dir":"C:\\Hades","complete":true}"#, "\n",
            r#"{"installationId":"a","deleted":true}"#, "\n",
        );
        let (active, removed) = parse_launcher_v2(log);
        assert!(active.is_empty());
        assert!(removed.contains("Hades"));
    }

    #[test]
    fn reads_the_old_installed_list() {
        let dat = r#"{"InstallationList":[{"AppName":"Hades","InstallLocation":"C:\\Hades","NamespaceId":"ns","ItemId":"it"}]}"#;
        let installs = parse_launcher_installed(dat);
        assert_eq!(installs[0].app_name, "Hades");
        assert_eq!(installs[0].namespace, "ns");
    }

    #[test]
    fn takes_name_and_tall_cover_from_the_catalog() {
        let json = r#"[{"id":"it","title":"Hades","categories":[{"path":"games"}],
            "keyImages":[{"type":"DieselGameBox","url":"wide"},{"type":"DieselGameBoxTall","url":"tall"}]}]"#;
        let encoded = base64::engine::general_purpose::STANDARD.encode(json);
        let catalog = parse_catalog(&encoded);
        let install = Install { app_name: "Hades".into(), item_id: "it".into(), ..Install::default() };
        let entry = catalog.get("it");
        assert!(is_game(&install, entry));
        let game = to_game(install, entry, "epic:Hades".into());
        assert_eq!(game.name, "Hades");
        assert_eq!(game.cover_url.as_deref(), Some("tall"));
    }

    #[test]
    fn catalog_marks_dlc_as_not_a_game() {
        let json = r#"[{"id":"it","title":"Hades OST","categories":[{"path":"addons"}],"mainGameItem":{"id":"x"}}]"#;
        let encoded = base64::engine::general_purpose::STANDARD.encode(json);
        let catalog = parse_catalog(&encoded);
        let install = Install { app_name: "HadesOST".into(), item_id: "it".into(), ..Install::default() };
        assert!(!is_game(&install, catalog.get("it")));
    }

    #[test]
    fn base_games_with_an_empty_main_game_are_games() {
        let json = r#"[{"id":"it","title":"Hogwarts Legacy","categories":[{"path":"games"}],"mainGameItem":{"namespace":"","id":""}}]"#;
        let encoded = base64::engine::general_purpose::STANDARD.encode(json);
        let catalog = parse_catalog(&encoded);
        let install = Install { app_name: "Fig".into(), item_id: "it".into(), ..Install::default() };
        assert!(is_game(&install, catalog.get("it")));
    }

    #[test]
    fn builds_the_full_launch_url() {
        let url = launch_url("ns", "it", "Hades");
        assert_eq!(url, "com.epicgames.launcher://apps/ns%3Ait%3AHades?action=launch&silent=true");
    }
}
