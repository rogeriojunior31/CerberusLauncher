//! Catalog sources the player adds by URL: projects in the Cerberus or
//! RecompLauncher format, and Hydra lists with direct HTTP(S) downloads.
//! All of them show up in Discover; Hydra's magnet and torrent links remain
//! available as metadata in Settings, but are not installed.

use cerberus_shared::tr;
use std::fs;
use std::path::{Path, PathBuf};
use std::io::Write;
use std::time::{Duration, UNIX_EPOCH};

use cerberus_shared::{SourceListing, SourcePage};
use futures_util::StreamExt;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::steam_store;
use super::catalog::{self, CatalogEntry, DirectFile};

/// One game in our source format. Only `id`, `name` and `repo` are required.
#[derive(Deserialize, Default)]
#[serde(default)]
struct SourceGame {
    id: String,
    name: String,
    repo: String,
    game: String,
    console: String,
    origin: String,
    about: String,
    icon: Option<String>,
    cover: Option<String>,
    hero: Option<String>,
    start: Option<String>,
    tags: Vec<String>,
    /// Release file name pattern per system, e.g. `{"windows": "Game-*-Windows.zip"}`.
    assets: std::collections::HashMap<String, String>,
    systems: Vec<String>,
    // Anything that names a download outside GitHub disqualifies the entry.
    uris: Value,
    magnet: Value,
    url: Value,
    download: Value,
}

#[derive(Deserialize)]
struct SourceFile {
    #[serde(default)]
    name: String,
    games: Vec<SourceGame>,
}

/// `owner/name`, both plain: a GitHub project, nothing else.
fn is_github_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let ok = |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) && p != "." && p != "..");
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

fn plain_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 80 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub enum Entries {
    Projects(Vec<CatalogEntry>),
    Hydra { listings: Vec<SourceListing>, games: Vec<CatalogEntry> },
}

pub struct CatalogSource {
    pub name: String,
    pub entries: Entries,
    pub synced_at: Option<u64>,
}

impl CatalogSource {
    pub fn listings(&self, query: &str, offset: usize) -> Result<SourcePage, String> {
        let Entries::Hydra { listings: entries, .. } = &self.entries else { return Err(tr!("Escolha uma fonte Hydra.").into()) };
        let query = query.trim().to_lowercase();
        let mut items = Vec::new();
        let mut total = 0;
        for item in entries {
            if query.is_empty() || item.title.to_lowercase().contains(&query) {
                if total >= offset && items.len() < 25 { items.push(item.clone()); }
                total += 1;
            }
        }
        Ok(SourcePage { items, total })
    }
}

/// Which hoster a link points at, when it is one we can resolve. GoFile is left
/// out on purpose: its API needs a token the Hydra gets by running JS.
fn hoster_of(url: &reqwest::Url) -> Option<&'static str> {
    let host = url.host_str().map(|h| h.trim_end_matches('.').to_ascii_lowercase())?;
    if host == "pixeldrain.com" || host.ends_with(".pixeldrain.com") {
        return Some("pixeldrain");
    }
    if host == "mediafire.com" || host.ends_with(".mediafire.com") {
        return Some("mediafire");
    }
    if host == "mega.nz" || host == "mega.co.nz" {
        return Some("mega");
    }
    None
}

fn hydra_game(row: &Value, source: &str) -> Option<CatalogEntry> {
    let field = |key: &str| row.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let title = field("title")?;
    for url in row.get("uris")?.as_array()?.iter().filter_map(Value::as_str).map(str::trim) {
        let Ok(parsed) = reqwest::Url::parse(url) else { continue };
        if !matches!(parsed.scheme(), "http" | "https") {
            continue;
        }
        // A hoster link carries no usable name in its path; the resolver fills it.
        let (name, hoster) = if let Some(hoster) = hoster_of(&parsed) {
            (String::new(), hoster.to_owned())
        } else {
            let last = parsed.path_segments().and_then(|mut parts| parts.next_back()).unwrap_or_default();
            if last.is_empty() || last == "." || last == ".." || (super::assets::kind(last).is_none() && !super::assets::is_program(last)) {
                continue;
            }
            (last.to_owned(), String::new())
        };
        // A MEGA link without the key in its fragment cannot be decrypted.
        if hoster == "mega" && parsed.fragment().unwrap_or_default().is_empty() {
            continue;
        }
        let id = format!("hydra-{}", &format!("{:x}", Sha256::digest(format!("{title}\0{url}")))[..24]);
        let date = field("uploadDate").unwrap_or_default().to_owned();
        let files = std::collections::HashMap::from([("windows".into(), DirectFile { name: name.clone(), sha256: String::new(), url: url.to_owned(), hoster: hoster.clone() })]);
        let kind = if hoster.is_empty() { Some(if super::assets::looks_like_installer(&name) { "installer" } else if super::assets::is_program(&name) { "single-exe" } else { "portable" }) } else { None };
        return Some(CatalogEntry {
            title: id.clone(),
            id,
            name: title.to_owned(),
            game: title.to_owned(),
            console: "PC".into(),
            origin: "pc".into(),
            author: if source.is_empty() { "Hydra".into() } else { source.to_owned() },
            from_source: true,
            size_text: field("fileSize").unwrap_or_default().to_owned(),
            released: date.clone(),
            added: date.clone(),
            tag: Some(if date.is_empty() { "1".into() } else { date }),
            files,
            systems: vec!["windows".into()],
            kind: kind.unwrap_or("portable").into(),
            ..CatalogEntry::default()
        });
    }
    None
}

/// A title without the markers repack lists decorate it with, for a Steam search:
/// bracketed groups drop out and version tokens are removed, so "SpaghettiKart
/// [MULTi] (USA) v1.2.3" searches as "SpaghettiKart".
pub fn clean_title(title: &str) -> String {
    let mut bare = String::new();
    let mut depth = 0u32;
    for c in title.chars() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => bare.push(c),
            _ => {}
        }
    }
    bare.split_whitespace().filter(|word| !is_version(word)).collect::<Vec<_>>().join(" ")
}

/// "v1.2.3", "1.2.3", "r2", "b4" — a version, not a word of the title. A bare
/// number stays: "64" in "Celeste 64" is the title, not a version.
fn is_version(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    if !lower.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let letters: String = lower.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    matches!(letters.as_str(), "v" | "r" | "b" | "rev" | "build" | "beta" | "alpha")
        || (letters.is_empty() && lower.contains('.'))
}

/// The Steam store's cover and hero, looked up once per game and kept on disk.
#[derive(Default, Serialize, Deserialize)]
struct ArtCache {
    entries: std::collections::HashMap<String, ArtEntry>,
}

#[derive(Serialize, Deserialize)]
struct ArtEntry {
    cover: Option<String>,
    hero: Option<String>,
    at: u64,
}

fn art_file(cache_dir: &Path) -> PathBuf {
    cache_dir.join("hydra-art.json")
}

fn load_art(cache_dir: &Path) -> ArtCache {
    std::fs::read_to_string(art_file(cache_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_art(cache_dir: &Path, cache: &ArtCache) {
    let file = art_file(cache_dir);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = crate::state_file::write_bytes(&file, serde_json::to_string(cache).unwrap_or_default().as_bytes());
}

/// Puts the art already found into the games, so a restart never asks Steam again.
fn apply_cached_art(cache_dir: &Path, source: &mut CatalogSource) {
    let cache = load_art(cache_dir);
    let Entries::Hydra { games, .. } = &mut source.entries else { return };
    for game in games {
        if game.cover.is_some() {
            continue;
        }
        if let Some(entry) = cache.entries.get(&game.id) {
            game.cover = entry.cover.clone();
            game.hero = entry.hero.clone();
        }
    }
}

/// Fills Hydra games' covers from the Steam store, a few per sync so a big list
/// (shortcut: the rest fill in on later refreshes) never stalls the app.
pub async fn enrich(client: &Client, cache_dir: &Path, source: &mut CatalogSource) {
    const CAP: usize = 40;
    let Entries::Hydra { games, .. } = &mut source.entries else { return };
    let now = std::time::SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut cache = load_art(cache_dir);
    let mut done = 0;
    let mut changed = false;
    for game in games {
        if game.cover.is_some() {
            continue;
        }
        let fresh = cache.entries.get(&game.id).is_some_and(|e| now.saturating_sub(e.at) < 7 * 86_400);
        if !fresh {
            if done >= CAP {
                break;
            }
            // Exact match only: a wrong cover for a different game is worse than none.
            let art = steam_store::identify(client, &[clean_title(&game.name)]).await.map(|found| found.art);
            cache.entries.insert(game.id.clone(), ArtEntry {
                cover: art.as_ref().and_then(|a| a.cover.clone()),
                hero: art.as_ref().and_then(|a| a.hero.clone()),
                at: now,
            });
            changed = true;
            done += 1;
        }
        if let Some(entry) = cache.entries.get(&game.id) {
            game.cover = entry.cover.clone();
            game.hero = entry.hero.clone();
        }
    }
    if changed {
        save_art(cache_dir, &cache);
    }
}

fn parse(text: &str) -> Result<CatalogSource, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| tr!("Isso não é um JSON válido.").to_string())?;
    let (name, entries) = if value.get("games").is_none() && value.get("apps").is_none()
        && let Some(downloads) = value.get("downloads").and_then(Value::as_array) {
        let name = value.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
        let mut items = Vec::new();
        let mut games = Vec::new();
        for row in downloads {
            let field = |key: &str| row.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
            if let Some(title) = field("title") {
                items.push(SourceListing {
                    title: title.to_owned(),
                    file_size: field("fileSize").map(str::to_owned),
                    upload_date: field("uploadDate").map(str::to_owned),
                });
                games.extend(hydra_game(row, &name));
            }
        }
        if items.is_empty() && !downloads.is_empty() { return Err(tr!("A fonte Hydra não contém títulos legíveis.").into()); }
        (name, Entries::Hydra { listings: items, games })
    } else {
        let (name, projects) = parse_projects(text, value)?;
        (name, Entries::Projects(projects))
    };
    Ok(CatalogSource { name, entries, synced_at: None })
}

fn parse_projects(text: &str, value: Value) -> Result<(String, Vec<CatalogEntry>), String> {
    if value.get("apps").is_some() {
        let mut list = catalog::from_recomplauncher(text)?;
        for entry in &mut list {
            entry.from_source = true;
        }
        return Ok((tr!("Catálogo no formato do RecompLauncher").into(), list));
    }
    let source: SourceFile = serde_json::from_value(value).map_err(|_| tr!("Formato desconhecido: esperava {\"name\", \"games\": [...]}.").to_string())?;
    let mut list = Vec::new();
    for game in source.games {
        let own_links = [&game.uris, &game.magnet, &game.url, &game.download].iter().any(|v| !v.is_null());
        if own_links || !is_github_repo(&game.repo) || !plain_id(&game.id) || game.name.is_empty() {
            continue;
        }
        let origin = catalog::origin_id(&game.origin);
        list.push(CatalogEntry {
            title: game.id.clone(),
            console: catalog::origin_name(&origin, Some(&game.console)),
            game: if game.game.is_empty() { game.name.clone() } else { game.game },
            id: game.id,
            name: game.name,
            origin,
            author: game.repo.split('/').next().unwrap_or_default().to_string(),
            repo: game.repo,
            from_source: true,
            about: game.about,
            icon: game.icon,
            cover: game.cover,
            hero: game.hero,
            start: game.start,
            tags: game.tags,
            assets: game.assets,
            systems: game.systems,
            kind: "portable".into(),
            ..CatalogEntry::default()
        });
    }
    if list.is_empty() {
        return Err(tr!("Nenhum jogo utilizável: cada um precisa de \"id\", \"name\" e \"repo\" (dono/projeto no GitHub).").into());
    }
    Ok((source.name, list))
}

fn cache_file(cache_dir: &Path, url: &str) -> PathBuf {
    let name = format!("{:x}", Sha256::digest(url.as_bytes()));
    cache_dir.join("sources").join(format!("{}.json", &name[..24]))
}

/// Fetches a source and keeps its copy; errors leave the old copy in use.
pub async fn fetch(client: &Client, cache_dir: &Path, url: &str) -> Result<CatalogSource, String> {
    if !url.starts_with("https://") {
        return Err(tr!("Use um endereço https://.").into());
    }
    let reply = crate::net::send(client.get(url).timeout(Duration::from_secs(30))).await.map_err(|_| tr!("Não foi possível baixar a fonte.").to_string())?;
    if reply.status().as_u16() == 451 {
        return Err(tr!("HTTP 451 · Acesso indisponível por restrição legal informada pelo servidor.").into());
    }
    if !reply.status().is_success() {
        return Err(tr!("A fonte respondeu {}.", reply.status().as_u16()));
    }
    let mut bytes = Vec::new();
    let mut stream = reply.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        if bytes.len() + chunk.len() > 64 * 1024 * 1024 { return Err(tr!("A fonte ultrapassou o limite de 64 MiB.").into()); }
        bytes.extend_from_slice(&chunk);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| tr!("A fonte precisa usar UTF-8."))?;
    let mut source = store(cache_dir, url, text.trim_start_matches('\u{feff}'))?;
    enrich(client, cache_dir, &mut source).await;
    Ok(source)
}

fn store(cache_dir: &Path, url: &str, text: &str) -> Result<CatalogSource, String> {
    let mut parsed = parse(text)?;
    let file = cache_file(cache_dir, url);
    let parent = file.parent().unwrap_or(cache_dir);
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    temp.persist(&file).map_err(|e| tr!("Falha ao guardar a fonte: {}", e))?;
    parsed.synced_at = modified(&file);
    Ok(parsed)
}

fn modified(path: &Path) -> Option<u64> {
    Some(fs::metadata(path).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs())
}

/// The copy on disk of a source, for start and offline.
pub fn load_copy(cache_dir: &Path, url: &str) -> Option<CatalogSource> {
    let file = cache_file(cache_dir, url);
    let text = fs::read_to_string(&file).ok()?;
    // Caches from before Hydra direct links dropped their `uris`: read them as
    // missing so the next check fetches the full list again and the links install.
    if text.contains("\"downloads\"") && !text.contains("\"uris\"") {
        return None;
    }
    let mut source = parse(&text).ok()?;
    apply_cached_art(cache_dir, &mut source);
    source.synced_at = modified(&file);
    Some(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projects(text: &str) -> (String, Vec<CatalogEntry>) {
        let source = parse(text).unwrap();
        let Entries::Projects(games) = source.entries else { panic!("expected projects") };
        (source.name, games)
    }

    #[test]
    fn reads_our_format() {
        let text = r#"{"name": "Jogos livres", "games": [
            {"id": "srb2", "name": "Sonic Robo Blast 2", "repo": "STJr/SRB2", "assets": {"windows": "SRB2-*-Full.zip"}},
            {"id": "tux", "name": "SuperTuxKart", "repo": "supertuxkart/stk-code", "console": "PC"}
        ]}"#;
        let (name, list) = projects(text);
        assert_eq!(name, "Jogos livres");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].assets["windows"], "SRB2-*-Full.zip");
        assert_eq!(list[1].author, "supertuxkart");
    }

    #[test]
    fn leaves_out_entries_with_their_own_download_links() {
        let text = r#"{"name": "x", "games": [
            {"id": "a", "name": "A", "repo": "dev/a"},
            {"id": "b", "name": "B", "repo": "dev/b", "uris": ["magnet:?xt=urn:btih:abc"]},
            {"id": "c", "name": "C", "repo": "dev/c", "download": "https://filehost.example/c.rar"}
        ]}"#;
        let (_, list) = projects(text);
        assert_eq!(list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["a"]);
    }

    #[test]
    fn needs_a_github_project() {
        let text = r#"{"games": [
            {"id": "a", "name": "A", "repo": "https://example.com/a"},
            {"id": "b", "name": "B", "repo": "../../etc"},
            {"id": "../c", "name": "C", "repo": "dev/c"}
        ]}"#;
        assert!(parse(text).is_err());
    }

    #[test]
    fn hydra_metadata_survives_an_invalid_refresh_and_reopens_offline() {
        let cache = tempfile::tempdir().unwrap();
        let url = "https://example.org/list.json";
        let text = r#"{"name":"Homebrew","downloads":[{"title":" Sky Runner ","uris":["magnet:?xt=urn:btih:abc","https://cdn.example/Sky.zip"],"uploadDate":"2026-11-18","fileSize":"10 MB"},{"title":"Sky II"},{"fileSize":"12 GB"}]}"#;
        store(cache.path(), url, text).unwrap();
        assert!(fs::read_to_string(cache_file(cache.path(), url)).unwrap().contains("https://cdn.example/Sky.zip"));
        assert!(store(cache.path(), url, r#"{"downloads":[{}]}"#).is_err());
        let source = load_copy(cache.path(), url).unwrap();
        assert!(source.synced_at.is_some());
        assert_eq!(source.name, "Homebrew");
        let Entries::Hydra { games, .. } = &source.entries else { panic!("expected Hydra") };
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Sky Runner");
        assert_eq!(games[0].files["windows"].url, "https://cdn.example/Sky.zip");
        assert_eq!(games[0].files["windows"].hoster, "");
        assert_eq!(games[0].kind, "portable");
        let page = source.listings("  SKY ", 0).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items[0].title, "Sky Runner");
        assert_eq!(page.items[0].file_size.as_deref(), Some("10 MB"));
        assert_eq!(page.items[0].upload_date.as_deref(), Some("2026-11-18"));
        assert!(page.items[1].upload_date.is_none());
        assert!(source.listings("unlisted", 0).unwrap().items.is_empty());
    }

    #[test]
    fn only_supported_direct_hydra_downloads_become_catalog_games() {
        let source = parse(r#"{"name":"List","downloads":[
            {"title":"ZIP","uris":["https://example.org/a.zip"]},
            {"title":"Installer","uris":["http://example.org/b-setup.exe"]},
            {"title":"Portable exe","uris":["https://example.org/Game.exe"]},
            {"title":"Torrent","uris":["magnet:?xt=urn:btih:abc"]},
            {"title":"Unsupported","uris":["https://example.org/c.rar"]}
        ]}"#).unwrap();
        let Entries::Hydra { listings, games } = source.entries else { panic!("expected Hydra") };
        assert_eq!(listings.len(), 5, "all entries remain searchable");
        assert_eq!(games.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), vec!["ZIP", "Installer", "Portable exe"]);
        assert_ne!(games[0].id, games[1].id);
        assert!(games.iter().all(|g| g.repo.is_empty() && g.from_source && g.tag.is_some()));
        assert_eq!(games[1].kind, "installer");
        assert_eq!(games[2].kind, "single-exe");
    }

    #[test]
    fn old_caches_without_uris_read_as_missing_so_they_refetch() {
        let cache = tempfile::tempdir().unwrap();
        let url = "https://example.org/list.json";
        // The pre-hoster cache shape: listings, but the download links were dropped.
        let old = r#"{"name":"Old","downloads":[{"title":"Game","fileSize":"10 MB","uploadDate":"2026-11-01"}]}"#;
        let file = cache_file(cache.path(), url);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, old).unwrap();
        assert!(load_copy(cache.path(), url).is_none(), "an old cache must be re-fetched");
    }

    #[test]
    fn title_cleaning_drops_repack_markers_for_search() {
        assert_eq!(clean_title("SpaghettiKart [MULTi] (USA) v1.2.3"), "SpaghettiKart");
        assert_eq!(clean_title("Hollow Knight [FitGirl Repack]"), "Hollow Knight");
        assert_eq!(clean_title("A {DL} Game [Update + DLCs]"), "A Game");
        assert_eq!(clean_title("Plain Title"), "Plain Title");
        assert_eq!(clean_title("Celeste 64 v1.1.1"), "Celeste 64");
        assert_eq!(clean_title("  [only markers]  "), "");
    }

    #[test]
    fn hoster_links_join_the_catalog_with_their_hoster_noted() {
        let source = parse(r#"{"name":"H","downloads":[
            {"title":"Pixel","uris":["https://pixeldrain.com/u/abc123"]},
            {"title":"Fire","uris":["https://www.mediafire.com/file/xyz/game.zip/file"]},
            {"title":"Mega","uris":["https://mega.nz/file/abcDEF#keykeykey"]},
            {"title":"Mega no key","uris":["https://mega.nz/file/nokey"]}
        ]}"#).unwrap();
        let Entries::Hydra { listings, games } = source.entries else { panic!("expected Hydra") };
        assert_eq!(listings.len(), 4);
        assert_eq!(games.len(), 3, "the MEGA link without a key stays reference-only");
        assert_eq!(games[0].files["windows"].hoster, "pixeldrain");
        assert_eq!(games[1].files["windows"].hoster, "mediafire");
        assert_eq!(games[2].files["windows"].hoster, "mega");
    }

    #[test]
    fn pages_hydra_results_and_keeps_empty_sources_valid() {
        let rows: Vec<_> = (0..31).map(|i| serde_json::json!({"title": format!("Game {i}")})).collect();
        let source = parse(&serde_json::json!({"downloads":rows}).to_string()).unwrap();
        assert_eq!(source.listings("", 0).unwrap().items.len(), 25);
        let last = source.listings("game", 25).unwrap();
        assert_eq!(last.items.len(), 6);
        assert_eq!(last.items[0].title, "Game 25");
        assert_eq!(last.total, 31);
        assert_eq!(parse(r#"{"downloads":[]}"#).unwrap().listings("", 0).unwrap().total, 0);
    }

    #[test]
    fn reads_recomplaunchers_format_too() {
        let text = r#"{"apps": [{"id": "x", "name": "X", "repo": "dev/x", "systems": ["windows"]}]}"#;
        assert_eq!(projects(text).1.len(), 1);
    }
}
