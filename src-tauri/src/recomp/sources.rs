//! Catalog sources the player adds by URL, like Hydra's: a JSON list whose games
//! show up in Discover beside the recomps.
//!
//! Unlike Hydra's download sources, an entry here is a project on GitHub, and is
//! installed from its developer's own releases, checked against GitHub's
//! checksum, the same way recomps are. Entries pointing anywhere else (magnet
//! links, torrents, file hosts) are left out: Cerberus installs what developers
//! publish themselves, never copies of games from third parties.
//!
//! Two shapes are read: ours, `{"name": ..., "games": [...]}`, and
//! RecompLauncher's catalog (`{"apps": [...]}`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

/// A source's name and games, or why it can't be used. Entries that aren't a
/// GitHub project, or that carry their own download links, are skipped.
pub fn parse(text: &str) -> Result<(String, Vec<CatalogEntry>), String> {
    let value: Value = serde_json::from_str(text).map_err(|_| "Isso não é um JSON válido.".to_string())?;
    if value.get("apps").is_some() {
        let list = catalog::from_recomplauncher(text)?;
        return Ok(("Catálogo no formato do RecompLauncher".into(), list));
    }
    let source: SourceFile = serde_json::from_value(value).map_err(|_| "Formato desconhecido: esperava {\"name\", \"games\": [...]}.".to_string())?;
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
        return Err("Nenhum jogo utilizável: cada um precisa de \"id\", \"name\" e \"repo\" (dono/projeto no GitHub).".into());
    }
    Ok((source.name, list))
}

fn cache_file(cache_dir: &Path, url: &str) -> PathBuf {
    let name = format!("{:x}", Sha256::digest(url.as_bytes()));
    cache_dir.join("sources").join(format!("{}.json", &name[..24]))
}

/// Fetches a source and keeps its copy; errors leave the old copy in use.
pub async fn fetch(client: &Client, cache_dir: &Path, url: &str) -> Result<(String, Vec<CatalogEntry>), String> {
    if !url.starts_with("https://") {
        return Err("Use um endereço https://.".into());
    }
    let reply = client.get(url).timeout(Duration::from_secs(30)).send().await.map_err(|_| "Não foi possível baixar a fonte.".to_string())?;
    if !reply.status().is_success() {
        return Err(format!("A fonte respondeu {}.", reply.status().as_u16()));
    }
    let text = reply.text().await.map_err(|e| e.to_string())?;
    let parsed = parse(&text)?;
    let file = cache_file(cache_dir, url);
    let _ = fs::create_dir_all(file.parent().unwrap_or(cache_dir));
    let _ = fs::write(&file, &text);
    Ok(parsed)
}

/// The copy on disk of a source, for start and offline.
pub fn load_copy(cache_dir: &Path, url: &str) -> Option<(String, Vec<CatalogEntry>)> {
    let text = fs::read_to_string(cache_file(cache_dir, url)).ok()?;
    parse(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_our_format() {
        let text = r#"{"name": "Jogos livres", "games": [
            {"id": "srb2", "name": "Sonic Robo Blast 2", "repo": "STJr/SRB2", "assets": {"windows": "SRB2-*-Full.zip"}},
            {"id": "tux", "name": "SuperTuxKart", "repo": "supertuxkart/stk-code", "console": "PC"}
        ]}"#;
        let (name, list) = parse(text).unwrap();
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
        let (_, list) = parse(text).unwrap();
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
    fn hydra_style_repack_lists_are_refused() {
        let hydra = r#"{"name": "Some Repacks", "downloads": [{"title": "Game", "uris": ["magnet:?xt=..."], "fileSize": "10 GB"}]}"#;
        assert!(parse(hydra).is_err());
    }

    #[test]
    fn reads_recomplaunchers_format_too() {
        let text = r#"{"apps": [{"id": "x", "name": "X", "repo": "dev/x", "systems": ["windows"]}]}"#;
        assert_eq!(parse(text).unwrap().1.len(), 1);
    }
}
