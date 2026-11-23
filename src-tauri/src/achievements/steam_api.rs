//! Steam's official Web API, used only when the player turns it on with their
//! own free key (Settings › Steam). It fills in what the Steam client on this
//! PC never cached: every owned game, with play time, and the achievements of
//! games played elsewhere. The player's profile must show game details publicly.
//!
//! Everything read is kept in `cache/achievements/steam-api/`, and a game's
//! achievements are asked again only after it was played since the last time.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cerberus_shared::i18n::{Lang, lang};
use cerberus_shared::{SteamAccount, tr};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::steam::Kv;

const API: &str = "https://api.steampowered.com";
/// SteamID64 of account number 0; a SteamID64 is this plus the account number.
const ID_BASE: u64 = 76_561_197_960_265_728;

fn steam_lang() -> &'static str {
    if lang() == Lang::Pt { "brazilian" } else { "english" }
}

async fn get(http: &reqwest::Client, path: &str, query: &str) -> Result<Value, String> {
    let url = format!("{API}/{path}?{query}");
    let reply = http
        .get(&url)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|_| tr!("Não foi possível falar com a Steam. Confira a internet.").to_string())?;
    match reply.status().as_u16() {
        401 | 403 => return Err(tr!("A Steam não aceitou essa chave da Web API.").into()),
        429 => return Err(tr!("A Steam pediu para esperar um pouco. Tente de novo em instantes.").into()),
        // Asking for a game without stats answers 400 with a JSON body; read it.
        400 => {}
        s if !(200..300).contains(&s) => return Err(tr!("A Steam respondeu {}.", s)),
        _ => {}
    }
    reply.json().await.map_err(|_| tr!("A Steam respondeu algo inesperado.").to_string())
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The account that last signed in to Steam on this PC: its SteamID64 and name.
pub fn local_account(steam: &Path) -> Option<(String, String)> {
    let text = fs::read_to_string(steam.join("config").join("loginusers.vdf")).ok()?;
    let root = super::steam::text_kv(&text);
    let users = root.get("users")?;
    let mut best: Option<(bool, i64, String, String)> = None;
    for (id, user) in users.children() {
        if id.len() != 17 || !id.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let recent = user.get("MostRecent").and_then(Kv::int) == Some(1);
        let stamp = user.get("Timestamp").and_then(Kv::int).unwrap_or(0);
        let name = user.get("PersonaName").and_then(Kv::text).unwrap_or_default().to_string();
        if best.as_ref().is_none_or(|(r, t, _, _)| (recent, stamp) > (*r, *t)) {
            best = Some((recent, stamp, id.clone(), name));
        }
    }
    best.map(|(_, _, id, name)| (id, name))
}

/// A SteamID64 from what the player typed: the number, a `/profiles/<id>` link,
/// or a `/id/<name>` link (asked of Steam).
pub async fn resolve(http: &reqwest::Client, key: &str, typed: &str) -> Result<String, String> {
    let typed = typed.trim().trim_end_matches('/');
    let last = typed.rsplit('/').next().unwrap_or(typed);
    if last.len() == 17 && last.chars().all(|c| c.is_ascii_digit()) {
        return Ok(last.to_string());
    }
    if last.is_empty() {
        return Err(tr!("Informe o seu perfil da Steam.").into());
    }
    let v = get(http, "ISteamUser/ResolveVanityURL/v1/", &format!("key={}&vanityurl={}", encode(key), encode(last))).await?;
    match v.pointer("/response/steamid").and_then(Value::as_str) {
        Some(id) => Ok(id.to_string()),
        None => Err(tr!("A Steam não encontrou esse perfil.").into()),
    }
}

/// The account behind a key and SteamID, and whether its game details are public.
pub async fn account(http: &reqwest::Client, key: &str, steamid: &str) -> Result<SteamAccount, String> {
    let v = get(http, "ISteamUser/GetPlayerSummaries/v2/", &format!("key={}&steamids={}", encode(key), encode(steamid))).await?;
    let p = v.pointer("/response/players/0").ok_or(tr!("A Steam não encontrou esse perfil."))?;
    let owned = owned_raw(http, key, steamid).await?;
    Ok(SteamAccount {
        steamid: steamid.to_string(),
        name: p.get("personaname").and_then(Value::as_str).unwrap_or_default().to_string(),
        avatar: p.get("avatarfull").and_then(Value::as_str).map(str::to_string),
        url: p.get("profileurl").and_then(Value::as_str).unwrap_or_default().to_string(),
        profile_public: p.get("communityvisibilitystate").and_then(Value::as_i64) == Some(3),
        games_public: owned.is_some(),
        games: owned.map(|o| o.len() as u32).unwrap_or(0),
    })
}

/// A game the account owns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Owned {
    pub app: u32,
    pub name: String,
    pub minutes: u64,
    pub last_played: u64,
    /// Steam says the game has stats (and so maybe achievements).
    pub stats: bool,
}

/// `None` when the account's game details aren't public (Steam answers with nothing).
async fn owned_raw(http: &reqwest::Client, key: &str, steamid: &str) -> Result<Option<Vec<Owned>>, String> {
    let v = get(
        http,
        "IPlayerService/GetOwnedGames/v1/",
        &format!("key={}&steamid={}&include_appinfo=1&include_played_free_games=1", encode(key), encode(steamid)),
    )
    .await?;
    let Some(games) = v.pointer("/response/games").and_then(Value::as_array) else { return Ok(None) };
    Ok(Some(
        games
            .iter()
            .filter_map(|g| {
                Some(Owned {
                    app: g.get("appid")?.as_u64()? as u32,
                    name: g.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                    minutes: g.get("playtime_forever").and_then(Value::as_u64).unwrap_or(0),
                    last_played: g.get("rtime_last_played").and_then(Value::as_u64).unwrap_or(0),
                    stats: g.get("has_community_visible_stats").and_then(Value::as_bool).unwrap_or(false),
                })
            })
            .collect(),
    ))
}

fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("cache").join("achievements").join("steam-api")
}

fn age(file: &Path) -> Option<Duration> {
    fs::metadata(file).and_then(|m| m.modified()).ok()?.elapsed().ok()
}

/// Every owned game, kept for an hour.
pub async fn owned(http: &reqwest::Client, data_dir: &Path, key: &str, steamid: &str) -> Result<Vec<Owned>, String> {
    let file = cache_dir(data_dir).join(format!("owned-{steamid}.json"));
    if age(&file).is_some_and(|a| a < Duration::from_secs(3600)) {
        if let Some(list) = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()) {
            return Ok(list);
        }
    }
    let list = owned_raw(http, key, steamid)
        .await?
        .ok_or(tr!("Os detalhes de jogo do seu perfil da Steam estão privados. Deixe-os públicos para o Cerberus ler todos os seus jogos."))?;
    let _ = fs::create_dir_all(cache_dir(data_dir));
    let _ = fs::write(&file, serde_json::to_string(&list).unwrap_or_default());
    Ok(list)
}

/// One of the player's achievements, as the API gives it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mine {
    pub api: String,
    pub achieved: bool,
    pub unlocked_at: u64,
    pub name: String,
    pub description: String,
}

#[derive(Serialize, Deserialize)]
struct MineFile {
    last_played: u64,
    lang: String,
    list: Vec<Mine>,
}

/// The player's achievements in a game (empty when it has none), kept until
/// the game is played again.
pub async fn mine(http: &reqwest::Client, data_dir: &Path, key: &str, steamid: &str, app: u32, last_played: u64) -> Result<Vec<Mine>, String> {
    let file = cache_dir(data_dir).join(format!("mine-{steamid}-{app}.json"));
    let lang_now = steam_lang();
    if let Some(kept) = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<MineFile>(&t).ok()) {
        if kept.last_played >= last_played && kept.lang == lang_now {
            return Ok(kept.list);
        }
    }
    let v = get(
        http,
        "ISteamUserStats/GetPlayerAchievements/v1/",
        &format!("key={}&steamid={}&appid={app}&l={lang_now}", encode(key), encode(steamid)),
    )
    .await?;
    let stats = v.get("playerstats").ok_or(tr!("A Steam respondeu algo inesperado."))?;
    if stats.get("success").and_then(Value::as_bool) == Some(false) {
        let error = stats.get("error").and_then(Value::as_str).unwrap_or_default();
        if error.to_lowercase().contains("not public") {
            return Err(tr!("Os detalhes de jogo do seu perfil da Steam estão privados. Deixe-os públicos para o Cerberus ler todos os seus jogos.").into());
        }
        // "Requested app has no stats": a game without achievements.
    }
    let list: Vec<Mine> = stats
        .get("achievements")
        .and_then(Value::as_array)
        .map(|all| {
            all.iter()
                .map(|a| Mine {
                    api: a.get("apiname").and_then(Value::as_str).unwrap_or_default().to_string(),
                    achieved: a.get("achieved").and_then(Value::as_i64) == Some(1),
                    unlocked_at: a.get("unlocktime").and_then(Value::as_u64).unwrap_or(0),
                    name: a.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                    description: a.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    let _ = fs::create_dir_all(cache_dir(data_dir));
    let kept = MineFile { last_played, lang: lang_now.into(), list: list.clone() };
    let _ = fs::write(&file, serde_json::to_string(&kept).unwrap_or_default());
    Ok(list)
}

/// A game's achievement list with icons and secret flags, kept for a month.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaEntry {
    pub api: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub icon_locked: Option<String>,
    pub hidden: bool,
}

pub async fn schema(http: &reqwest::Client, data_dir: &Path, key: &str, app: u32) -> Result<Vec<SchemaEntry>, String> {
    let lang_now = steam_lang();
    let file = cache_dir(data_dir).join(format!("schema-{app}-{lang_now}.json"));
    if age(&file).is_some_and(|a| a < Duration::from_secs(30 * 24 * 3600)) {
        if let Some(list) = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()) {
            return Ok(list);
        }
    }
    let v = get(http, "ISteamUserStats/GetSchemaForGame/v2/", &format!("key={}&appid={app}&l={lang_now}", encode(key))).await?;
    let list: Vec<SchemaEntry> = v
        .pointer("/game/availableGameStats/achievements")
        .and_then(Value::as_array)
        .map(|all| {
            all.iter()
                .map(|a| SchemaEntry {
                    api: a.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                    name: a.get("displayName").and_then(Value::as_str).unwrap_or_default().to_string(),
                    description: a.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
                    icon: a.get("icon").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string),
                    icon_locked: a.get("icongray").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string),
                    hidden: a.get("hidden").and_then(Value::as_i64).unwrap_or(0) != 0,
                })
                .collect()
        })
        .unwrap_or_default();
    let _ = fs::create_dir_all(cache_dir(data_dir));
    let _ = fs::write(&file, serde_json::to_string(&list).unwrap_or_default());
    Ok(list)
}

/// The account number in Steam's files (`userdata/<n>`) for a SteamID64.
pub fn account_number(steamid: &str) -> Option<String> {
    steamid.parse::<u64>().ok().and_then(|id| id.checked_sub(ID_BASE)).map(|n| n.to_string())
}

/// What the API says about achievements, by API name.
pub fn by_api(list: &[Mine]) -> HashMap<&str, &Mine> {
    list.iter().map(|m| (m.api.as_str(), m)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_last_signed_in_is_found() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("config")).unwrap();
        fs::write(
            dir.path().join("config").join("loginusers.vdf"),
            "\"users\"\n{\n\t\"76561198000000001\"\n\t{\n\t\t\"PersonaName\"\t\t\"Old\"\n\t\t\"MostRecent\"\t\t\"0\"\n\t\t\"Timestamp\"\t\t\"200\"\n\t}\n\t\"76561198000000002\"\n\t{\n\t\t\"PersonaName\"\t\t\"Player\"\n\t\t\"MostRecent\"\t\t\"1\"\n\t\t\"Timestamp\"\t\t\"100\"\n\t}\n}\n",
        )
        .unwrap();
        assert_eq!(local_account(dir.path()), Some(("76561198000000002".into(), "Player".into())));
        assert_eq!(account_number("76561198000000002").as_deref(), Some("39734274"));
    }

    #[test]
    fn typed_profiles_with_the_number_need_no_question() {
        let http = reqwest::Client::new();
        let id = tauri::async_runtime::block_on(resolve(&http, "k", "https://steamcommunity.com/profiles/76561198000000002/")).unwrap();
        assert_eq!(id, "76561198000000002");
    }
}
