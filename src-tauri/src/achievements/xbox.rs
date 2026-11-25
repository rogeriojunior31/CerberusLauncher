//! Xbox achievements, from the account signed in under Configurações › Xbox.
//!
//! Gentle with Xbox Live: the progress of every game comes in one question
//! (title history with its achievement summary), the latest unlocks in another,
//! and a game's full list only when it's opened, kept on disk until the game is
//! played again. The Xbox Live session is reused for an hour instead of signing
//! in for each question.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cerberus_shared::{tr, Achievement, AchievementGame, AchievementSource, RecentUnlock};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::xbox_account::{self, Session};

/// The session, kept between questions (a sign-in is three requests).
static SESSION: Mutex<Option<(Instant, String, Session)>> = Mutex::new(None);
const SESSION_FOR: Duration = Duration::from_secs(3600);

/// Signs in with the kept refresh token, or reuses the last hour's session.
/// The new refresh token, when there is one, is handed to `save`.
pub async fn session(http: &reqwest::Client, refresh: &str, save: impl FnOnce(&str)) -> Result<Session, String> {
    if let Ok(kept) = SESSION.lock() {
        if let Some((at, from, s)) = kept.as_ref() {
            if from == refresh && at.elapsed() < SESSION_FOR {
                return Ok(Session { refresh: s.refresh.clone(), auth: s.auth.clone(), xuid: s.xuid.clone(), gamertag: s.gamertag.clone() });
            }
        }
    }
    let fresh = xbox_account::sign_in(http, xbox_account::Grant::Refresh(refresh)).await?;
    save(&fresh.refresh);
    if let Ok(mut kept) = SESSION.lock() {
        let copy = Session { refresh: fresh.refresh.clone(), auth: fresh.auth.clone(), xuid: fresh.xuid.clone(), gamertag: fresh.gamertag.clone() };
        *kept = Some((Instant::now(), fresh.refresh.clone(), copy));
    }
    Ok(fresh)
}

fn cache_dir(data: &Path) -> PathBuf {
    data.join("cache").join("achievements")
}

async fn get(http: &reqwest::Client, s: &Session, url: &str, contract: &str) -> Result<Value, String> {
    let (_, language) = crate::gamepass::market();
    let response = crate::net::send(
        http.get(url).header("Authorization", &s.auth).header("x-xbl-contract-version", contract).header("Accept-Language", language),
    )
    .await
    .map_err(|e| e.to_string())?;
    match response.status().as_u16() {
        200..=299 => response.json().await.map_err(|e| e.to_string()),
        429 => Err(tr!("O Xbox Live pediu uma pausa nas consultas. O Cerberus mostra o que já tinha e tenta de novo mais tarde.").into()),
        status => {
            // Which address and why, for the log (the answer never has the token).
            let body = response.text().await.unwrap_or_default();
            eprintln!("xbox live {status} {}: {}", url.split('?').next().unwrap_or_default(), body.chars().take(200).collect::<String>());
            Err(tr!("O Xbox Live não entregou as conquistas.").into())
        }
    }
}

/// Unix seconds from Xbox Live's dates ("2024-05-01T20:11:45.1234567Z").
fn seconds(text: &str) -> Option<u64> {
    let date = text.get(..19)?;
    let (y, mo, d) = (date.get(0..4)?.parse::<i64>().ok()?, date.get(5..7)?.parse::<i64>().ok()?, date.get(8..10)?.parse::<i64>().ok()?);
    let (h, mi, s) = (date.get(11..13)?.parse::<i64>().ok()?, date.get(14..16)?.parse::<i64>().ok()?, date.get(17..19)?.parse::<i64>().ok()?);
    // Days since 1970 (Howard Hinnant's civil-from-days, reversed).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (mo + if mo > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let total = days * 86_400 + h * 3600 + mi * 60 + s;
    (total > 0 && y > 1971).then_some(total as u64)
}

/// The game list with progress, every title with achievements, in one question.
pub async fn games(http: &reqwest::Client, s: &Session) -> Result<Vec<AchievementGame>, String> {
    let url = format!("https://titlehub.xboxlive.com/users/xuid({})/titles/titlehistory/decoration/achievement,image", s.xuid);
    let v = get(http, s, &url, "2").await?;
    Ok(read_games(&v))
}

fn read_games(v: &Value) -> Vec<AchievementGame> {
    let mut games = Vec::new();
    for t in v.get("titles").and_then(Value::as_array).into_iter().flatten() {
        let total = t.pointer("/achievement/totalAchievements").and_then(Value::as_u64).unwrap_or(0) as u32;
        let id = t.get("titleId").and_then(Value::as_str).unwrap_or_default();
        if total == 0 || id.is_empty() {
            continue;
        }
        let unlocked = t.pointer("/achievement/currentAchievements").and_then(Value::as_u64).unwrap_or(0) as u32;
        let pc = t.get("devices").and_then(Value::as_array).is_some_and(|d| d.iter().any(|x| x.as_str() == Some("PC")));
        games.push(AchievementGame {
            source: AchievementSource::Xbox,
            key: id.to_string(),
            name: t.get("name").and_then(Value::as_str).unwrap_or_default().trim().to_string(),
            art: t.get("displayImage").and_then(Value::as_str).map(|u| vec![u.to_string()]).unwrap_or_default(),
            platform: if pc { "Xbox · PC".into() } else { "Xbox".into() },
            total,
            unlocked: unlocked.min(total),
            hardcore: 0,
            points: t.pointer("/achievement/currentGamerscore").and_then(Value::as_u64).unwrap_or(0) as u32,
            points_total: t.pointer("/achievement/totalGamerscore").and_then(Value::as_u64).unwrap_or(0) as u32,
            last_unlock: None,
            playtime_secs: None,
            last_played: t.pointer("/titleHistory/lastTimePlayed").and_then(Value::as_str).and_then(seconds),
            award: None,
            award_at: None,
        });
    }
    games
}

/// One achievement as Xbox Live gives it.
fn read_achievement(a: &Value) -> Option<Achievement> {
    let unlocked = a.get("progressState").and_then(Value::as_str) == Some("Achieved");
    let icon = a.get("mediaAssets").and_then(Value::as_array).and_then(|m| m.iter().find_map(|x| x.get("url").and_then(Value::as_str))).map(str::to_string);
    let secret = a.get("isSecret").and_then(Value::as_bool).unwrap_or(false);
    let description = if unlocked || !secret {
        a.get(if unlocked { "description" } else { "lockedDescription" }).and_then(Value::as_str).filter(|d| !d.is_empty()).or_else(|| a.get("description").and_then(Value::as_str)).unwrap_or_default()
    } else {
        ""
    };
    Some(Achievement {
        id: a.get("id").and_then(Value::as_str)?.to_string(),
        name: a.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
        description: description.to_string(),
        icon: icon.clone(),
        icon_locked: icon,
        hidden: secret,
        unlocked_at: if unlocked { a.pointer("/progression/timeUnlocked").and_then(Value::as_str).and_then(seconds).or(Some(1)) } else { None },
        hardcore_at: None,
        points: a
            .get("rewards")
            .and_then(Value::as_array)
            .and_then(|r| r.iter().find(|x| x.get("type").and_then(Value::as_str) == Some("Gamerscore")))
            .and_then(|r| r.get("value").and_then(Value::as_str))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        rarity: a.pointer("/rarity/currentPercentage").and_then(Value::as_f64).map(|p| p as f32),
    })
}

/// The latest unlocks across games, in one question.
pub async fn recent(http: &reqwest::Client, s: &Session, games: &[AchievementGame]) -> Result<Vec<RecentUnlock>, String> {
    let url = format!("https://achievements.xboxlive.com/users/xuid({})/achievements?unlockedOnly=true&orderBy=UnlockTime&maxItems=40", s.xuid);
    let v = get(http, s, &url, "2").await?;
    let by_id: HashMap<&str, &AchievementGame> = games.iter().map(|g| (g.key.as_str(), g)).collect();
    let mut list = Vec::new();
    for a in v.get("achievements").and_then(Value::as_array).into_iter().flatten() {
        let Some(achievement) = read_achievement(a) else { continue };
        let title = a.pointer("/titleAssociations/0/id").and_then(Value::as_u64).map(|n| n.to_string()).unwrap_or_default();
        let Some(game) = by_id.get(title.as_str()) else { continue };
        list.push(RecentUnlock { source: AchievementSource::Xbox, game_key: game.key.clone(), game_name: game.name.clone(), achievement });
    }
    Ok(list)
}

#[derive(Serialize, Deserialize)]
struct Kept {
    played: Option<u64>,
    list: Vec<Achievement>,
}

/// One game's full list: from disk unless the game was played since it was read.
pub async fn game_list(http: &reqwest::Client, data: &Path, s: &Session, title: &str, played: Option<u64>) -> Result<Vec<Achievement>, String> {
    let file = cache_dir(data).join(format!("xbox-{}-{}.json", s.xuid, title.chars().filter(char::is_ascii_digit).collect::<String>()));
    if let Some(kept) = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Kept>(&t).ok()) {
        if kept.played >= played {
            return Ok(kept.list);
        }
    }
    let url = format!("https://achievements.xboxlive.com/users/xuid({})/achievements?titleId={title}&maxItems=1000", s.xuid);
    let v = get(http, s, &url, "2").await?;
    let list: Vec<Achievement> = v.get("achievements").and_then(Value::as_array).into_iter().flatten().filter_map(read_achievement).collect();
    let _ = fs::create_dir_all(cache_dir(data));
    let _ = fs::write(&file, serde_json::to_string(&Kept { played, list: list.clone() }).unwrap_or_default());
    Ok(list)
}

/// What the hub showed last time, to open at once and between syncs.
#[derive(Default, Serialize, Deserialize)]
pub struct HubKept {
    pub games: Vec<AchievementGame>,
    pub recent: Vec<RecentUnlock>,
}

pub fn hub_file(data: &Path) -> PathBuf {
    cache_dir(data).join("xbox-hub.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_xbox_dates() {
        assert_eq!(seconds("1970-01-02T00:00:00Z"), None, "too early to be real");
        assert_eq!(seconds("2024-05-01T20:11:45.1234567Z"), Some(1_714_594_305));
    }

    #[test]
    fn reads_titles_and_achievements() {
        let v = json!({ "titles": [
            { "titleId": "1144039928", "name": "Halo Infinite", "displayImage": "http://img", "devices": ["PC", "XboxSeries"],
              "achievement": { "currentAchievements": 12, "totalAchievements": 119, "currentGamerscore": 120, "totalGamerscore": 1000 },
              "titleHistory": { "lastTimePlayed": "2024-05-01T20:11:45.123Z" } },
            { "titleId": "1", "name": "Netflix", "achievement": { "totalAchievements": 0 } }
        ]});
        let games = read_games(&v);
        assert_eq!(games.len(), 1);
        assert_eq!((games[0].unlocked, games[0].total, games[0].points), (12, 119, 120));
        assert_eq!(games[0].platform, "Xbox · PC");
        let a = read_achievement(&json!({ "id": "7", "name": "Firefight", "description": "Done", "lockedDescription": "Do it", "progressState": "Achieved",
            "progression": { "timeUnlocked": "2024-05-01T20:11:45Z" }, "rewards": [{ "type": "Gamerscore", "value": "20" }],
            "rarity": { "currentPercentage": 12.5 }, "mediaAssets": [{ "url": "http://icon" }] })).unwrap();
        assert_eq!((a.points, a.unlocked_at, a.rarity), (20, Some(1_714_594_305), Some(12.5)));
        let secret = read_achievement(&json!({ "id": "8", "name": "???", "description": "Spoiler", "isSecret": true, "progressState": "NotStarted" })).unwrap();
        assert_eq!(secret.description, "", "a secret stays secret until unlocked");
    }
}
