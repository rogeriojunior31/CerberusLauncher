//! Catalog sources the player adds by URL, like Hydra's: a JSON list whose games
//! show up in Discover beside the recomps.
//!
//! Unlike Hydra's download sources, an entry here is a project on GitHub, and is
//! installed from its developer's own releases, checked against GitHub's
//! checksum, the same way recomps are. Entries pointing anywhere else (magnet
//! links, torrents, file hosts) are left out: Cerberus installs what developers
//! publish themselves, never copies of games from third parties.
//!
//! Project sources use the Cerberus or RecompLauncher formats. Hydra lists contribute
//! searchable publication metadata in Settings. Only project sources feed the installer.

use cerberus_shared::tr;
use std::fs;
use std::path::{Path, PathBuf};
use std::io::Write;
use std::time::{Duration, UNIX_EPOCH};

use cerberus_shared::{SourceListing, SourcePage};
use futures_util::StreamExt;

use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::catalog::{self, CatalogEntry};

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
    Hydra(Vec<SourceListing>),
}

pub struct CatalogSource {
    pub name: String,
    pub entries: Entries,
    pub synced_at: Option<u64>,
}

impl CatalogSource {
    pub fn listings(&self, query: &str, offset: usize) -> Result<SourcePage, String> {
        let Entries::Hydra(entries) = &self.entries else { return Err(tr!("Escolha uma fonte Hydra.").into()) };
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

fn parse(text: &str) -> Result<CatalogSource, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| tr!("Isso não é um JSON válido.").to_string())?;
    let (name, entries) = if value.get("games").is_none() && value.get("apps").is_none()
        && let Some(downloads) = value.get("downloads").and_then(Value::as_array) {
        let mut items = Vec::new();
        for row in downloads {
            let field = |key: &str| row.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
            if let Some(title) = field("title") {
                items.push(SourceListing {
                    title: title.to_owned(),
                    file_size: field("fileSize").map(str::to_owned),
                    upload_date: field("uploadDate").map(str::to_owned),
                });
            }
        }
        if items.is_empty() && !downloads.is_empty() { return Err(tr!("A fonte Hydra não contém títulos legíveis.").into()); }
        let name = value.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
        (name, Entries::Hydra(items))
    } else {
        let (name, projects) = parse_projects(text, value)?;
        (name, Entries::Projects(projects))
    };
    Ok(CatalogSource { name, entries, synced_at: None })
}

fn parse_projects(text: &str, value: Value) -> Result<(String, Vec<CatalogEntry>), String> {
    if value.get("apps").is_some() {
        let list = catalog::from_recomplauncher(text)?;
        return Ok((tr!("Catálogo no formato do RecompLauncher").into(), list));
    }
    let source: SourceFile = serde_json::from_value(value).map_err(|_| tr!("Formato desconhecido: esperava {\"name\", \"games\": [...]}.").to_string())?;
    let mut list = Vec::new();
    for game in source.games {
        let own_links = [&game.uris, &game.magnet, &game.url, &game.download].iter().any(|v| !v.is_null());
        if own_links || !is_github_repo(&game.repo) || !plain_id(&game.id) || game.name.is_empty() {
            continue;
        }
        let origin = if game.origin.is_empty() { "pc".to_string() } else { game.origin };
        list.push(CatalogEntry {
            title: game.id.clone(),
            console: if game.console.is_empty() { "PC".to_string() } else { game.console },
            game: if game.game.is_empty() { game.name.clone() } else { game.game },
            id: game.id,
            name: game.name,
            origin,
            author: game.repo.split('/').next().unwrap_or_default().to_string(),
            repo: game.repo,
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
    let reply = client.get(url).timeout(Duration::from_secs(30)).send().await.map_err(|_| tr!("Não foi possível baixar a fonte.").to_string())?;
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
    store(cache_dir, url, text.trim_start_matches('\u{feff}'))
}

fn store(cache_dir: &Path, url: &str, text: &str) -> Result<CatalogSource, String> {
    let mut parsed = parse(text)?;
    let file = cache_file(cache_dir, url);
    let parent = file.parent().unwrap_or(cache_dir);
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    match &parsed.entries {
        Entries::Projects(_) => temp.write_all(text.as_bytes()).map_err(|e| e.to_string())?,
        Entries::Hydra(items) => {
            #[derive(serde::Serialize)]
            struct Metadata<'a> { name: &'a str, downloads: &'a [SourceListing] }
            serde_json::to_writer(&mut temp, &Metadata { name: &parsed.name, downloads: items }).map_err(|e| e.to_string())?;
        }
    }
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
    let mut source = parse(&fs::read_to_string(&file).ok()?).ok()?;
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
        let text = r#"{"name":"Homebrew","downloads":[{"title":" Sky Runner ","uris":["ignored"],"uploadDate":"2026-11-18","fileSize":"10 MB"},{"title":"Sky II"},{"fileSize":"12 GB"}]}"#;
        store(cache.path(), url, text).unwrap();
        assert!(!fs::read_to_string(cache_file(cache.path(), url)).unwrap().contains("uris"));
        assert!(store(cache.path(), url, r#"{"downloads":[{}]}"#).is_err());
        let source = load_copy(cache.path(), url).unwrap();
        assert!(source.synced_at.is_some());
        assert_eq!(source.name, "Homebrew");
        let page = source.listings("  SKY ", 0).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items[0].title, "Sky Runner");
        assert_eq!(page.items[0].file_size.as_deref(), Some("10 MB"));
        assert_eq!(page.items[0].upload_date.as_deref(), Some("2026-11-18"));
        assert!(page.items[1].upload_date.is_none());
        assert!(source.listings("unlisted", 0).unwrap().items.is_empty());
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
