//! The recomp catalog.
//!
//! The list comes from recomplauncher.com (`pc-catalog.json`, the same one
//! RecompLauncher uses: about 1900 recomps across 25+ consoles), read in its own
//! format and turned into ours. A copy is kept on disk and asked for again only
//! when it is six hours old, like RecompLauncher itself does, so the site is not
//! asked more often than by its own launcher. Without that copy (first start
//! offline, the site down) the list bundled with Cerberus is used.
//!
//! Our bundled entries also fill in what the site's list doesn't have, such as
//! where each recomp keeps its saves (checked against each project's source).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use cerberus_shared::CopyHandoff;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

pub const SOURCE_URL: &str = "https://recomplauncher.com/pc-catalog.json";
pub const SOURCE_NAME: &str = "recomplauncher.com";
const FRESH_FOR: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CatalogEntry {
    pub id: String,
    /// Which game this is; entries sharing it are versions of one game.
    pub title: String,
    pub name: String,
    pub game: String,
    pub console: String,
    /// The console's short id (`n64`, `x360`...).
    pub origin: String,
    pub repo: String,
    pub icon: Option<String>,
    pub cover: Option<String>,
    pub hero: Option<String>,
    /// The game's logo, a see-through picture shown in place of its name.
    pub logo: Option<String>,
    pub genre: String,
    #[serde(skip)]
    pub original: Option<cerberus_shared::OriginalGame>,
    pub first_release: String,
    pub releases: u64,
    pub players: u64,
    pub about: String,
    pub adds: String,
    pub needs: String,
    /// Where the recomp keeps its saves, as `{local}/...`, `{appdata}/...` or
    /// `{install}/...` templates. Empty when unknown.
    pub saves: Vec<String>,
    pub author: String,
    /// The release the catalog knows about, e.g. `v1.2.2`.
    pub tag: Option<String>,
    /// "28 MB", as the catalog says it.
    pub size_text: String,
    pub released: String,
    pub added: String,
    pub downloads: u64,
    pub stars: u64,
    /// windows, linux, macos.
    pub systems: Vec<String>,
    /// The release file's name per system, as a pattern: `Zelda64Recompiled-*-Windows.zip`.
    pub assets: HashMap<String, String>,
    /// The exact release file per system, with GitHub's checksum: downloaded
    /// straight from the release, with no question to GitHub's API first.
    pub files: HashMap<String, DirectFile>,
    pub tags: Vec<String>,
    pub features: Vec<String>,
    /// Steps the player does by hand before playing, when the recomp needs them.
    pub setup_steps: Vec<String>,
    pub setup_required: bool,
    /// Announced but not out yet.
    pub soon: bool,
    /// The version the catalog recommends among the game's versions, and why.
    pub recommended: bool,
    pub why: String,
    /// The program to start, when the catalog names it.
    pub start: Option<String>,
    /// How the release ships: `portable` (an archive), `installer`, `single-exe`.
    pub kind: String,
    /// Other ids this recomp was known by (our own list's, from before the
    /// site's list), so games installed under them still find their entry.
    pub aliases: Vec<String>,
    /// How the recomp is given the player's copy of the game.
    pub handoff: Option<CopyHandoff>,
    /// For N64 recomps, the cartridge version it was built against.
    pub n64: Option<crate::roms::N64Wanted>,
    /// Whether the copy can be a folder of extracted files (`"folder or iso"`).
    pub accepts_folder: bool,
    /// The game's own settings the launcher can change.
    #[serde(skip)]
    pub settings: crate::game_settings::Offer,
    /// What the player should know before the first start ("pick your ROM in its menu").
    pub player_note: String,
    /// "local", "online" or "both"; empty for single player.
    pub multiplayer: String,
}

impl CatalogEntry {
    pub fn is(&self, id: &str) -> bool {
        self.id == id || self.aliases.iter().any(|alias| alias == id)
    }
}

pub fn find(id: &str) -> Option<CatalogEntry> {
    all().iter().find(|entry| entry.is(id)).cloned()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DirectFile {
    pub name: String,
    pub sha256: String,
}

/// The catalog in use: the main list plus the player's sources, put together once per change.
static STORE: RwLock<Option<Arc<Vec<CatalogEntry>>>> = RwLock::new(None);
/// The site's list (with ours merged in), once read.
static BASE: RwLock<Option<Vec<CatalogEntry>>> = RwLock::new(None);
/// The games of each source the player added.
static EXTRAS: RwLock<Vec<Vec<CatalogEntry>>> = RwLock::new(Vec::new());

/// The catalog in use: the site's list once read, else the bundled one, plus the player's sources.
pub fn all() -> Arc<Vec<CatalogEntry>> {
    let store = STORE.read().unwrap_or_else(|p| p.into_inner());
    match store.as_ref() {
        Some(list) => Arc::clone(list),
        None => Arc::new(bundled()),
    }
}

fn set(list: Vec<CatalogEntry>) {
    *BASE.write().unwrap_or_else(|p| p.into_inner()) = Some(list);
    rebuild();
}

/// The games of the player's sources, replacing the ones before.
pub fn set_extras(sources: Vec<Vec<CatalogEntry>>) {
    *EXTRAS.write().unwrap_or_else(|p| p.into_inner()) = sources;
    rebuild();
}

/// The main list first; a source's game that's already listed (same id or repo) is left out.
fn rebuild() {
    let base = BASE.read().unwrap_or_else(|p| p.into_inner()).clone();
    let mut list = base.unwrap_or_else(bundled);
    for source in EXTRAS.read().unwrap_or_else(|p| p.into_inner()).iter() {
        for game in source {
            let known = list.iter().any(|e| e.id == game.id || e.repo.eq_ignore_ascii_case(&game.repo));
            if !known {
                list.push(game.clone());
            }
        }
    }
    *STORE.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(list));
}

/// Cerberus's own list, as published in its repository: updated there, it
/// reaches every launcher without a new release.
pub const OWN_URL: &str = "https://raw.githubusercontent.com/rogeriojunior31/CerberusLauncher/main/src-tauri/catalog/recomps.json";

/// The newest copy of our own list fetched from the repository, once read.
static OWN: RwLock<Option<Vec<CatalogEntry>>> = RwLock::new(None);

/// Our own list: the repository's newest copy when there is one, else the one built in.
pub fn bundled() -> Vec<CatalogEntry> {
    if let Some(list) = OWN.read().unwrap_or_else(|p| p.into_inner()).clone() {
        return list;
    }
    built_in()
}

fn built_in() -> Vec<CatalogEntry> {
    let text = include_str!("../../catalog/recomps.json");
    serde_json::from_str(text).expect("the bundled catalog is valid JSON (checked by a test)")
}

fn own_file(cache_dir: &Path) -> PathBuf {
    cache_dir.join("own-catalog.json")
}

/// Our list in its JSON form; only a list that reads and isn't empty is taken.
fn parse_own(text: &str) -> Option<Vec<CatalogEntry>> {
    let list: Vec<CatalogEntry> = serde_json::from_str(text).ok()?;
    (!list.is_empty()).then_some(list)
}

/// Asks the repository for our own list when the copy is old; errors keep the one in use.
async fn refresh_own(client: &Client, cache_dir: &Path, force: bool) {
    let file = own_file(cache_dir);
    let age = fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
    if !force && age.is_some_and(|a| a < FRESH_FOR) {
        return;
    }
    let Ok(reply) = client.get(OWN_URL).timeout(Duration::from_secs(20)).send().await else {
        return;
    };
    let Ok(text) = reply.error_for_status().map(|r| r.text()) else {
        return;
    };
    let Ok(text) = text.await else {
        return;
    };
    let Some(list) = parse_own(&text) else {
        return;
    };
    let _ = fs::create_dir_all(cache_dir);
    let _ = fs::write(&file, &text);
    *OWN.write().unwrap_or_else(|p| p.into_inner()) = Some(list);
}

fn copy_file(cache_dir: &Path) -> PathBuf {
    cache_dir.join("pc-catalog.json")
}

/// At start: the copy on disk, if there is one, so the window has the full list
/// straight away while a newer one is asked for.
pub fn load_copy(cache_dir: &Path) {
    if let Some(list) = fs::read_to_string(own_file(cache_dir)).ok().and_then(|t| parse_own(&t)) {
        *OWN.write().unwrap_or_else(|p| p.into_inner()) = Some(list);
        rebuild();
    }
    let Ok(text) = fs::read_to_string(copy_file(cache_dir)) else {
        return;
    };
    if let Ok(list) = from_recomplauncher(&text) {
        set(merge(list, bundled()));
    }
}

/// Asks the site for the list when the copy on disk is old (or `force`), and
/// switches to it. Errors leave the current list in place.
pub async fn refresh(client: &Client, cache_dir: &Path, force: bool) -> Result<(), String> {
    refresh_own(client, cache_dir, force).await;
    let file = copy_file(cache_dir);
    let age = fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
    if !force && age.is_some_and(|a| a < FRESH_FOR) {
        return Ok(());
    }
    let reply = client
        .get(SOURCE_URL)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| format!("Sem conexão com {SOURCE_NAME}."))?;
    if !reply.status().is_success() {
        return Err(format!("{SOURCE_NAME} respondeu {}.", reply.status().as_u16()));
    }
    let text = reply.text().await.map_err(|e| e.to_string())?;
    // Only a list that reads properly replaces the one in use.
    let list = from_recomplauncher(&text)?;
    fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
    let temp = file.with_extension("json.part");
    fs::write(&temp, &text).map_err(|e| e.to_string())?;
    fs::rename(&temp, &file).map_err(|e| e.to_string())?;
    set(merge(list, bundled()));
    Ok(())
}

/// The site's list first; our entries add what it lacks (save places) and the
/// recomps it doesn't list.
fn merge(mut list: Vec<CatalogEntry>, ours: Vec<CatalogEntry>) -> Vec<CatalogEntry> {
    for mine in ours {
        let Some(theirs) = list.iter_mut().find(|e| e.id == mine.id || e.repo.eq_ignore_ascii_case(&mine.repo)) else {
            list.push(mine);
            continue;
        };
        if theirs.saves.is_empty() {
            theirs.saves = mine.saves;
        }
        if theirs.id != mine.id {
            theirs.aliases.push(mine.id);
        }
    }
    list
}

// ---------- RecompLauncher's format ----------

#[derive(Deserialize)]
struct RlCatalog {
    apps: Vec<RlApp>,
    #[serde(default)]
    origins: Vec<RlOrigin>,
    #[serde(default)]
    titles: Vec<RlTitle>,
}

#[derive(Deserialize)]
struct RlOrigin {
    id: String,
    name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RlTitle {
    id: String,
    pick: Option<String>,
    why: Option<String>,
    pc: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RlApp {
    id: String,
    name: String,
    game: Option<String>,
    origin: Option<String>,
    repo: String,
    tag: Option<String>,
    version: Option<String>,
    size: Option<String>,
    released: Option<String>,
    added: Option<String>,
    downloads: u64,
    stars: u64,
    blurb: Option<String>,
    adds: Option<String>,
    needs: Option<String>,
    author: Option<String>,
    icon: Option<String>,
    cover: Option<String>,
    hero: Option<String>,
    #[serde(default)]
    logo: Option<String>,
    #[serde(default)]
    genre: Option<String>,
    /// The original game's facts (IGDB), read loosely.
    #[serde(default)]
    about: Value,
    #[serde(default)]
    first: Option<String>,
    #[serde(default)]
    releases: Value,
    #[serde(default)]
    players: Value,
    title: Option<String>,
    // The site's list isn't strict about shapes: an empty map can come as `[]`,
    // and missing parts as `null`. These are read loosely, so one odd entry
    // never makes the whole list unreadable.
    systems: Value,
    assets: Value,
    files: Value,
    tags: Value,
    features: Value,
    setup_required: Option<bool>,
    setup_steps: Value,
    soon: Option<bool>,
    /// An object, or the same object written as a JSON string.
    boot: Option<Value>,
}

/// The original game's facts, when the catalog has an object with some.
fn original_of(value: &Value) -> Option<cerberus_shared::OriginalGame> {
    let text = |k: &str| value.get(k).and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let list = |k: &str| value.get(k).and_then(Value::as_array).map(|l| l.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let facts = cerberus_shared::OriginalGame {
        released: text("released"),
        developer: text("developer"),
        publisher: text("publisher"),
        series: text("series"),
        genres: list("genres"),
        modes: list("modes"),
        platforms: list("platforms"),
        rating: value.get("rating").and_then(Value::as_f64),
        summary: text("summary"),
    };
    (!facts.summary.is_empty() || !facts.developer.is_empty() || !facts.genres.is_empty()).then_some(facts)
}

/// A map when it is one; anything else (`[]`, `null`) is an empty map.
fn map_of<T: serde::de::DeserializeOwned>(value: Value) -> HashMap<String, T> {
    let Value::Object(map) = value else {
        return HashMap::new();
    };
    map.into_iter().filter_map(|(key, item)| Some((key, serde_json::from_value(item).ok()?))).collect()
}

/// The strings of a list; anything else is an empty list.
fn strings_of(value: Value) -> Vec<String> {
    let Value::Array(items) = value else {
        return Vec::new();
    };
    items.into_iter().filter_map(|item| item.as_str().map(str::to_string)).collect()
}

/// What the site's `boot` says about a recomp.
struct Boot {
    kind: String,
    start: Option<String>,
    handoff: Option<CopyHandoff>,
    n64: Option<crate::roms::N64Wanted>,
    accepts_folder: bool,
    settings: crate::game_settings::Offer,
    player_note: String,
    multiplayer: String,
}

/// How the release ships, which program starts the game, and how that program
/// is given the player's copy of the game (and which version of it).
fn boot_of(boot: Option<Value>) -> Boot {
    let value = match boot {
        Some(Value::String(text)) => serde_json::from_str(&text).unwrap_or(Value::Null),
        Some(other) => other,
        None => Value::Null,
    };
    let files = value.get("files");
    Boot {
        kind: value.get("kind").and_then(Value::as_str).unwrap_or("portable").to_string(),
        start: value.get("start").and_then(Value::as_str).map(str::to_string),
        handoff: files.and_then(handoff_of),
        n64: files.and_then(n64_wanted),
        accepts_folder: files.and_then(|f| f.get("accepts")).and_then(Value::as_str).is_some_and(|a| a.contains("folder")),
        settings: crate::game_settings::offer_of(value.get("settings")),
        player_note: value.get("player_note").and_then(Value::as_str).unwrap_or_default().trim().to_string(),
        multiplayer: value.get("multiplayer").and_then(Value::as_str).unwrap_or_default().to_string(),
    }
}

/// `"n64_id": "KT", "region": "E", "n64_rev": 1` (or a list of revisions).
fn n64_wanted(files: &Value) -> Option<crate::roms::N64Wanted> {
    let id = files.get("n64_id").and_then(Value::as_str).unwrap_or_default().to_string();
    let regions = files.get("region").and_then(Value::as_str).unwrap_or_default().to_string();
    let revisions: Vec<u8> = match files.get("n64_rev") {
        Some(Value::Number(n)) => n.as_u64().map(|r| vec![r as u8]).unwrap_or_default(),
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_u64).map(|r| r as u8).collect(),
        _ => Vec::new(),
    };
    if id.is_empty() && regions.is_empty() && revisions.is_empty() {
        return None;
    }
    Some(crate::roms::N64Wanted { id, regions, revisions })
}

fn handoff_of(files: &Value) -> Option<CopyHandoff> {
    let text = |key: &str| files.get(key).and_then(Value::as_str).map(str::to_string);
    let handoff = match files.get("how")?.as_str()? {
        "self" => CopyHandoff::AsksItself,
        "flag" => CopyHandoff::Flag { flag: text("flag").unwrap_or_else(|| "--rom".into()) },
        "env" => CopyHandoff::Env { name: text("env")? },
        // Xbox disc images are unpacked by Cerberus; other kinds of unpacking are left to the player.
        "folder" if matches!(text("extract").as_deref(), None | Some("xdvdfs") | Some("iso9660")) => {
            let expected = files.get("contains").and_then(Value::as_array).and_then(|names| names.first()).and_then(Value::as_str);
            let single_file = text("accepts").as_deref() == Some("file");
            CopyHandoff::Folder {
                folder: text("folder").unwrap_or_else(|| ".".into()),
                file_name: expected.filter(|_| single_file).map(str::to_string),
                extract: text("extract").is_some(),
            }
        }
        _ => return None,
    };
    Some(handoff)
}

/// Their art lives on their own media server; an empty string means none.
fn some(text: Option<String>) -> Option<String> {
    text.filter(|t| !t.trim().is_empty())
}

pub fn from_recomplauncher(text: &str) -> Result<Vec<CatalogEntry>, String> {
    let catalog: RlCatalog = serde_json::from_str(text).map_err(|e| format!("Catálogo ilegível: {e}"))?;
    let consoles: HashMap<String, String> = catalog.origins.into_iter().map(|o| (o.id, o.name)).collect();
    // Versions of one game: the site groups them under a title, with the one it recommends.
    let mut title_of: HashMap<String, (String, bool, String)> = HashMap::new();
    for title in catalog.titles {
        let pick = title.pick.as_deref().and_then(|p| p.strip_prefix("pc:")).map(str::to_string);
        for id in title.pc {
            let recommended = pick.as_deref() == Some(id.as_str());
            title_of.insert(id, (title.id.clone(), recommended, title.why.clone().unwrap_or_default()));
        }
    }

    let mut list = Vec::new();
    for app in catalog.apps {
        let Boot { kind, start, handoff, n64, accepts_folder, settings, player_note, multiplayer } = boot_of(app.boot);
        if app.repo.is_empty() || kind == "not-a-game" {
            continue;
        }
        // The id names the install folder, so only plain names are taken.
        let plain_id = !app.id.is_empty() && app.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !plain_id {
            continue;
        }
        let origin = app.origin.unwrap_or_default();
        let (title, recommended, why) = match title_of.remove(&app.id) {
            Some(found) => found,
            None => (some(app.title).unwrap_or_else(|| app.id.clone()), false, String::new()),
        };
        list.push(CatalogEntry {
            title,
            recommended,
            why: if recommended { why } else { String::new() },
            console: consoles.get(&origin).cloned().unwrap_or_else(|| origin.to_uppercase()),
            origin,
            name: app.name,
            game: app.game.unwrap_or_default(),
            repo: app.repo,
            icon: some(app.icon),
            cover: some(app.cover),
            hero: some(app.hero),
            logo: some(app.logo),
            genre: app.genre.unwrap_or_default(),
            original: original_of(&app.about),
            first_release: app.first.unwrap_or_default(),
            releases: app.releases.as_u64().unwrap_or(0),
            players: app.players.as_u64().unwrap_or(0),
            about: app.blurb.unwrap_or_default(),
            adds: app.adds.unwrap_or_default(),
            needs: super::translate::needs(&app.needs.unwrap_or_default()),
            author: app.author.unwrap_or_default(),
            tag: some(app.tag).or(some(app.version)),
            size_text: app.size.unwrap_or_default(),
            released: app.released.unwrap_or_default(),
            added: app.added.unwrap_or_default(),
            downloads: app.downloads,
            stars: app.stars,
            systems: strings_of(app.systems),
            assets: map_of(app.assets),
            files: map_of(app.files),
            tags: strings_of(app.tags).iter().map(|t| super::translate::tag(t)).collect(),
            features: strings_of(app.features),
            setup_steps: strings_of(app.setup_steps),
            setup_required: app.setup_required.unwrap_or(false),
            soon: app.soon.unwrap_or(false),
            start,
            kind,
            handoff,
            n64,
            accepts_folder,
            settings,
            player_note,
            multiplayer,
            saves: Vec::new(),
            aliases: Vec::new(),
            id: app.id,
        });
    }
    if list.is_empty() {
        return Err("O catálogo veio vazio.".into());
    }
    Ok(list)
}

/// "28 MB" -> bytes, roughly; only used to show a size before the download says the real one.
pub fn bytes_of(size_text: &str) -> u64 {
    let mut parts = size_text.split_whitespace();
    let Some(number) = parts.next().and_then(|n| n.replace(',', ".").parse::<f64>().ok()) else {
        return 0;
    };
    let unit = parts.next().unwrap_or("MB").to_uppercase();
    let scale = match unit.as_str() {
        "KB" => 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        _ => 1024.0 * 1024.0,
    };
    (number * scale) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "apps": [
            {"id": "zelda64recomp", "name": "Zelda 64: Recompiled", "game": "The Legend of Zelda: Majora's Mask",
             "origin": "n64", "repo": "Zelda64Recomp/Zelda64Recomp", "tag": "v1.2.2", "size": "28 MB",
             "downloads": 401060, "stars": 7730, "blurb": "Majora's Mask as a native PC game.",
             "icon": "https://x/icon.png", "cover": "https://x/cover.jpg", "hero": "",
             "systems": ["windows", "linux"], "assets": {"windows": "Zelda64Recompiled-*-Windows.zip"},
             "files": {"windows": {"name": "Zelda64Recompiled-v1.2.2-Windows.zip", "sha256": "abc", "scan": "green"}},
             "tags": ["Mods"], "features": ["Gyro aiming"], "title": null,
             "boot": "{\"kind\": \"portable\", \"start\": \"Zelda64Recompiled.exe\"}"},
            {"id": "2ship", "name": "2 Ship 2 Harkinian", "game": "The Legend of Zelda: Majora's Mask",
             "origin": "n64", "repo": "2ship2harkinian/2ship2harkinian", "systems": ["windows"]},
            {"id": "tool", "name": "Some Tool", "origin": "pc", "repo": "a/b", "boot": {"kind": "not-a-game"}}
        ],
        "origins": [{"id": "n64", "name": "Nintendo 64"}],
        "titles": [{"id": "majoras-mask", "pick": "pc:zelda64recomp", "why": "Most downloaded", "pc": ["zelda64recomp", "2ship"]}]
    }"#;

    #[test]
    fn reads_the_sites_format() {
        let list = from_recomplauncher(SAMPLE).unwrap();
        let zelda = &list[0];
        assert_eq!(zelda.console, "Nintendo 64");
        assert_eq!(zelda.tag.as_deref(), Some("v1.2.2"));
        assert_eq!(zelda.start.as_deref(), Some("Zelda64Recompiled.exe"));
        assert_eq!(zelda.files["windows"].name, "Zelda64Recompiled-v1.2.2-Windows.zip");
        assert_eq!(zelda.hero, None, "an empty picture is no picture");
    }

    #[test]
    fn groups_versions_and_keeps_the_recommendation() {
        let list = from_recomplauncher(SAMPLE).unwrap();
        assert_eq!(list[0].title, "majoras-mask");
        assert_eq!(list[1].title, "majoras-mask");
        assert!(list[0].recommended);
        assert_eq!(list[0].why, "Most downloaded");
        assert!(!list[1].recommended);
    }

    #[test]
    fn reads_how_each_recomp_takes_the_game() {
        let parse = |files: &str| handoff_of(&serde_json::from_str(files).unwrap());
        assert_eq!(parse(r#"{"how": "self"}"#), Some(CopyHandoff::AsksItself));
        assert_eq!(parse(r#"{"how": "flag", "flag": "--rom"}"#), Some(CopyHandoff::Flag { flag: "--rom".into() }));
        assert_eq!(parse(r#"{"how": "env", "env": "ICO_ISO"}"#), Some(CopyHandoff::Env { name: "ICO_ISO".into() }));
        assert_eq!(
            parse(r#"{"how": "folder", "folder": ".", "accepts": "file", "contains": ["mk64.us.z64"]}"#),
            Some(CopyHandoff::Folder { folder: ".".into(), file_name: Some("mk64.us.z64".into()), extract: false })
        );
        assert_eq!(
            parse(r#"{"how": "folder", "folder": "WWE 13", "accepts": "folder or iso", "extract": "xdvdfs"}"#),
            Some(CopyHandoff::Folder { folder: "WWE 13".into(), file_name: None, extract: true })
        );
        assert_eq!(parse(r#"{"how": "folder", "extract": "wad"}"#), None, "other unpacking is left to the player");
        assert_eq!(parse(r#"{"how": "setup"}"#), None);
    }

    #[test]
    fn reads_loose_shapes() {
        let text = SAMPLE.replace(
            r#""systems": ["windows"]}"#,
            r#""systems": null, "assets": [], "files": null, "tags": null, "setup_required": null}"#,
        );
        let list = from_recomplauncher(&text).unwrap();
        let odd = list.iter().find(|e| e.id == "2ship").unwrap();
        assert!(odd.assets.is_empty());
        assert!(odd.files.is_empty());
        assert!(!odd.setup_required);
    }

    #[test]
    fn leaves_out_ids_that_are_not_plain_names() {
        let text = SAMPLE.replace(r#""id": "2ship""#, r#""id": "../../evil""#);
        let list = from_recomplauncher(&text).unwrap();
        assert!(list.iter().all(|e| !e.id.contains('.')));
    }

    #[test]
    fn leaves_out_what_is_not_a_game() {
        let list = from_recomplauncher(SAMPLE).unwrap();
        assert!(list.iter().all(|e| e.id != "tool"));
    }

    #[test]
    fn refuses_a_list_that_does_not_read() {
        assert!(from_recomplauncher("<html>busy</html>").is_err());
        assert!(from_recomplauncher(r#"{"apps": []}"#).is_err());
    }

    #[test]
    fn our_save_places_fill_in_the_sites_entries() {
        let theirs = from_recomplauncher(SAMPLE).unwrap();
        let merged = merge(theirs, bundled());
        let zelda = merged.iter().find(|e| e.id == "zelda64recomp").unwrap();
        assert_eq!(zelda.saves, vec!["{local}/Zelda64Recompiled/saves"]);
        assert!(merged.iter().any(|e| e.id == "unleashedrecomp"), "recomps only we list stay");
    }

    #[test]
    fn our_old_ids_still_find_the_sites_entry() {
        let theirs = from_recomplauncher(SAMPLE).unwrap();
        let mut ours = bundled();
        ours.retain(|e| e.id == "2ship2harkinian");
        let merged = merge(theirs, ours);
        let entry = merged.iter().find(|e| e.is("2ship2harkinian")).unwrap();
        assert_eq!(entry.id, "2ship", "the site's entry, found by our id");
    }

    #[test]
    fn reads_sizes() {
        assert_eq!(bytes_of("28 MB"), 28 * 1024 * 1024);
        assert_eq!(bytes_of("1.5 GB"), (1.5 * 1024.0 * 1024.0 * 1024.0) as u64);
        assert_eq!(bytes_of(""), 0);
    }
}

#[cfg(test)]
#[test]
fn the_own_catalog_reads_whole_and_every_entry_downloads_directly() {
    let list = built_in();
    assert!(list.len() >= 72, "only {} entries", list.len());
    for entry in &list {
        assert!(!entry.id.is_empty() && !entry.repo.is_empty(), "{entry:?}");
        assert!(entry.tag.is_some(), "{} has no release", entry.id);
        let windows = entry.files.get("windows");
        assert!(windows.is_some_and(|f| f.sha256.len() == 64) || entry.id == "starship" || entry.id == "unleashedrecomp", "{} has no checked Windows file", entry.id);
    }
    let mut ids: Vec<&str> = list.iter().map(|e| e.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), list.len(), "two entries share an id");
}
