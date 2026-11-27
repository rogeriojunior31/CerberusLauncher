//! The player's own pictures for a game: cover, background and logo, picked
//! from SteamGridDB (with the player's own free key) or from a file, framed in
//! the launcher and kept here. They win over the store's or the catalog's art,
//! for any game: added by hand, a recomp, or one from Steam or Epic.
//!
//! The choices live in `art.json` (game id -> files), the pictures in the
//! covers folder. Only pictures made here are ever deleted.

use cerberus_shared::tr;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use cerberus_shared::{ArtGame, ArtOptions, ArtPic, Game};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const KINDS: [&str; 3] = ["cover", "hero", "logo"];
/// A picture bigger than this isn't art, or isn't worth keeping.
const MOST: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Custom {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cover: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hero: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logo: Option<String>,
}

impl Custom {
    fn slot(&mut self, kind: &str) -> Option<&mut Option<String>> {
        match kind {
            "cover" => Some(&mut self.cover),
            "hero" => Some(&mut self.hero),
            "logo" => Some(&mut self.logo),
            _ => None,
        }
    }
}

pub struct CustomArt {
    file: PathBuf,
    folder: PathBuf,
    map: Mutex<BTreeMap<String, Custom>>,
}

/// What kind of picture the bytes are, by their first bytes.
fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpeg")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

/// A picture as a `data:` address the window can draw on a canvas.
pub fn data_url(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MOST {
        return Err(tr!("Essa imagem é grande demais (25 MB no máximo).").into());
    }
    let kind = image_type(bytes).ok_or(tr!("Isso não é uma imagem PNG, JPEG ou WebP."))?;
    Ok(format!("data:image/{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// The bytes of a `data:` address the window made, checked to be a picture.
fn from_data_url(data: &str) -> Result<(Vec<u8>, &'static str), String> {
    let (_, body) = data.split_once(";base64,").ok_or(tr!("A imagem não chegou inteira."))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(body.trim()).map_err(|_| tr!("A imagem não chegou inteira."))?;
    if bytes.len() > MOST {
        return Err(tr!("Essa imagem é grande demais (25 MB no máximo).").into());
    }
    let kind = image_type(&bytes).ok_or(tr!("A imagem não chegou inteira."))?;
    Ok((bytes, kind))
}

/// A picture kept in the covers folder: by its name (how new ones are kept),
/// or by a full path written before (on another PC or user, found by its name
/// in this covers folder). `None` when it isn't there.
pub fn in_folder(folder: &Path, kept: &str) -> Option<String> {
    let path = Path::new(kept);
    if path.is_absolute() && path.is_file() {
        return Some(kept.to_string());
    }
    // A path from another PC: Windows separators, whatever this system calls them.
    let name = kept.rsplit(['/', '\\']).next().filter(|n| !n.is_empty())?;
    let here = folder.join(name);
    here.is_file().then(|| here.to_string_lossy().into_owned())
}

impl CustomArt {
    pub fn new(data_dir: &Path, covers: &Path) -> Self {
        let file = data_dir.join("art.json");
        let map = crate::state_file::load_or_default(&file);
        Self { file, folder: covers.to_path_buf(), map: Mutex::new(map) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Custom>> {
        self.map.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Puts the player's pictures on a game, over whatever art it had.
    pub fn apply(&self, game: &mut Game) {
        let map = self.lock();
        let Some(custom) = map.get(&game.id) else { return };
        let exists = |p: &Option<String>| p.as_deref().and_then(|p| in_folder(&self.folder, p));
        if let Some(cover) = exists(&custom.cover) {
            game.cover_path = Some(cover);
        }
        if let Some(hero) = exists(&custom.hero) {
            game.hero_path = Some(hero);
        }
        game.logo_path = exists(&custom.logo);
    }

    fn save(&self, map: &BTreeMap<String, Custom>) -> Result<(), String> {
        crate::state_file::save(&self.file, map)
    }

    /// A picture made here (in the covers folder), deleted once nothing uses it.
    fn forget(&self, old: Option<String>) {
        if let Some(old) = old.and_then(|p| in_folder(&self.folder, &p)).map(PathBuf::from).filter(|p| p.starts_with(&self.folder)) {
            let _ = fs::remove_file(old);
        }
    }

    /// Keeps the framed picture the window made for one kind of a game's art;
    /// `None` puts the game's own art back.
    pub fn set(&self, id: &str, kind: &str, data: Option<&str>) -> Result<(), String> {
        if !KINDS.contains(&kind) {
            return Err(tr!("Esse tipo de imagem não existe.").into());
        }
        if id.trim().is_empty() {
            return Err(tr!("Jogo inválido.").into());
        }
        let new = match data {
            Some(data) => {
                let (bytes, format) = from_data_url(data)?;
                fs::create_dir_all(&self.folder).map_err(|e| e.to_string())?;
                let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or_default();
                let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
                let ext = if format == "jpeg" { "jpg" } else { format };
                let path = self.folder.join(format!("{safe}-{kind}-{stamp}.{ext}"));
                fs::write(&path, bytes).map_err(|e| tr!("Não foi possível gravar a imagem: {}", e))?;
                // Only the name: the data folder can move (another PC, a backup restored).
                path.file_name().map(|n| n.to_string_lossy().into_owned())
            }
            None => None,
        };
        let mut map = self.lock();
        let mut next = map.clone();
        let custom = next.entry(id.to_string()).or_default();
        let slot = custom.slot(kind).ok_or(tr!("Esse tipo de imagem não existe."))?;
        let old = std::mem::replace(slot, new.clone());
        if custom.cover.is_none() && custom.hero.is_none() && custom.logo.is_none() {
            next.remove(id);
        }
        if let Err(error) = self.save(&next) {
            self.forget(new);
            return Err(error);
        }
        *map = next;
        drop(map);
        self.forget(old);
        Ok(())
    }
}

// ---------- SteamGridDB ----------

const API: &str = "https://www.steamgriddb.com/api/v2";

async fn sgdb(http: &reqwest::Client, key: &str, path: &str) -> Result<Value, String> {
    let reply = http
        .get(format!("{API}/{path}"))
        .bearer_auth(key)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| tr!("Não foi possível falar com o SteamGridDB: {}", e))?;
    match reply.status().as_u16() {
        401 | 403 => return Err(tr!("O SteamGridDB não aceitou essa chave. Confira em Configurações.").into()),
        404 => return Ok(Value::Array(Vec::new())),
        s if !(200..300).contains(&s) => return Err(tr!("O SteamGridDB respondeu {}.", s)),
        _ => {}
    }
    let value: Value = reply.json().await.map_err(|e| e.to_string())?;
    Ok(value.get("data").cloned().unwrap_or(Value::Array(Vec::new())))
}

fn search_term(name: &str) -> String {
    let term: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '\'' { c } else { ' ' }).collect();
    term.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// SteamGridDB's games for a name, best first.
pub async fn search(http: &reqwest::Client, key: &str, name: &str) -> Result<Vec<ArtGame>, String> {
    let term = search_term(name);
    if term.is_empty() {
        return Ok(Vec::new());
    }
    let encoded: String = term.chars().map(|c| match c { ' ' => "%20".to_string(), '\'' => "%27".to_string(), c => c.to_string() }).collect();
    let data = sgdb(http, key, &format!("search/autocomplete/{encoded}")).await?;
    Ok(data
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|g| {
                    let year = g.get("release_date").and_then(Value::as_i64).map(|t| 1970 + t / 31_556_952);
                    Some(ArtGame { id: g.get("id")?.as_u64()?, name: g.get("name")?.as_str()?.to_string(), year: year.map(|y| y as u32) })
                })
                .take(10)
                .collect()
        })
        .unwrap_or_default())
}

fn pics(data: &Value, most: usize) -> Vec<ArtPic> {
    data.as_array()
        .map(|list| {
            list.iter()
                .filter_map(|p| {
                    let url = p.get("url")?.as_str()?.to_string();
                    let thumb = p.get("thumb").and_then(Value::as_str).unwrap_or(&url).to_string();
                    Some(ArtPic { url, thumb })
                })
                .take(most)
                .collect()
        })
        .unwrap_or_default()
}

/// Every still cover, background and logo SteamGridDB has for one of its games.
pub async fn options(http: &reqwest::Client, key: &str, game: u64) -> Result<ArtOptions, String> {
    let (covers, heroes, logos) = futures_util::future::join3(
        sgdb(http, key, &format!("grids/game/{game}?dimensions=600x900,342x482,660x930&types=static")),
        sgdb(http, key, &format!("heroes/game/{game}?types=static")),
        sgdb(http, key, &format!("logos/game/{game}?types=static")),
    )
    .await;
    Ok(ArtOptions { covers: pics(&covers?, 48), heroes: pics(&heroes?, 36), logos: pics(&logos?, 36) })
}

/// Only SteamGridDB's picture servers are fetched from.
fn from_steamgriddb(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split('/').next().unwrap_or_default();
    host == "steamgriddb.com" || host.ends_with(".steamgriddb.com")
}

/// A SteamGridDB picture, fetched here so the window can frame it.
pub async fn fetch(http: &reqwest::Client, url: &str) -> Result<String, String> {
    if !from_steamgriddb(url) {
        return Err(tr!("Só imagens do SteamGridDB podem ser baixadas aqui.").into());
    }
    let reply = http.get(url).timeout(Duration::from_secs(60)).send().await.map_err(|e| tr!("Não foi possível baixar a imagem: {}", e))?;
    if !reply.status().is_success() {
        return Err(tr!("O SteamGridDB respondeu {} para essa imagem.", reply.status()));
    }
    let bytes = reply.bytes().await.map_err(|e| e.to_string())?;
    data_url(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_art_survives_the_data_folder_moving() {
        let old = tempfile::tempdir().unwrap();
        let covers = old.path().join("covers");
        let art = CustomArt::new(old.path(), &covers);
        let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        art.set("steam:1", "cover", Some(png)).unwrap();
        // The whole data folder restored somewhere else (another PC, another user).
        let new = tempfile::tempdir().unwrap();
        crate::recomp::copy_tree(old.path(), new.path()).unwrap();
        drop(art);
        drop(old);
        let moved = CustomArt::new(new.path(), &new.path().join("covers"));
        let mut game: Game = serde_json::from_value(serde_json::json!({"id": "steam:1", "name": "x", "install_dir": "", "source": {"kind": "steam", "app_id": 1}})).unwrap();
        moved.apply(&mut game);
        assert!(game.cover_path.as_deref().is_some_and(|p| p.starts_with(&*new.path().to_string_lossy())), "{:?}", game.cover_path);
        // A full path from before, gone: found by its name here.
        let name = fs::read_dir(new.path().join("covers")).unwrap().next().unwrap().unwrap().file_name();
        let elsewhere = format!(r"C:\Users\someone\AppData\Roaming\x\covers\{}", name.to_string_lossy());
        assert!(in_folder(&new.path().join("covers"), &elsewhere).is_some());
    }

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 1, 2, 3];

    fn game(id: &str) -> Game {
        let mut g = crate::library::game_from_exe(Path::new("C:/x/Game.exe"));
        g.id = id.into();
        g
    }

    #[test]
    fn pictures_win_over_the_games_art_and_come_back_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let covers = dir.path().join("covers");
        let art = CustomArt::new(dir.path(), &covers);
        art.set("steam:123", "cover", Some(&data_url(PNG).unwrap())).unwrap();
        art.set("steam:123", "logo", Some(&data_url(PNG).unwrap())).unwrap();

        let reopened = CustomArt::new(dir.path(), &covers);
        let mut g = game("steam:123");
        g.cover_path = Some("C:/steam/library.jpg".into());
        reopened.apply(&mut g);
        assert!(g.cover_path.as_deref().unwrap().contains("steam_123-cover-"));
        assert!(g.logo_path.as_deref().unwrap().ends_with(".png"));
        assert!(g.hero_path.is_none());
    }

    #[test]
    fn replacing_or_resetting_deletes_only_pictures_made_here() {
        let dir = tempfile::tempdir().unwrap();
        let covers = dir.path().join("covers");
        let art = CustomArt::new(dir.path(), &covers);
        art.set("g", "hero", Some(&data_url(PNG).unwrap())).unwrap();
        let first = fs::read_dir(&covers).unwrap().count();
        std::thread::sleep(std::time::Duration::from_millis(5));
        art.set("g", "hero", Some(&data_url(PNG).unwrap())).unwrap();
        assert_eq!(fs::read_dir(&covers).unwrap().count(), first, "the old picture goes");
        art.set("g", "hero", None).unwrap();
        assert_eq!(fs::read_dir(&covers).unwrap().count(), 0);
        let mut g = game("g");
        art.apply(&mut g);
        assert!(g.hero_path.is_none());
    }

    #[test]
    fn only_real_pictures_and_steamgriddb_addresses_are_taken() {
        assert!(data_url(b"<html>not a picture").is_err());
        assert!(from_data_url("data:image/png;base64,PGh0bWw+").is_err());
        assert!(from_steamgriddb("https://cdn2.steamgriddb.com/grid/abc.png"));
        assert!(!from_steamgriddb("https://evil.example/steamgriddb.com/a.png"));
        assert!(!from_steamgriddb("http://cdn2.steamgriddb.com/a.png"));
        assert!(!from_steamgriddb("https://steamgriddb.com.evil.example/a.png"));
    }
}
