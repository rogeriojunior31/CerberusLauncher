//! Epic Games achievements, from the account signed in under Configurações › Epic.
//!
//! Gentle with Epic: the play time of every game comes in one question, and a
//! game's achievements are asked only when it's new to the store or was played
//! since (its play time grew). A game without achievements is remembered as such.

use std::collections::HashMap;
use std::time::Duration;

use cerberus_shared::{tr, Achievement, AchievementGame, AchievementSource};
use serde_json::{json, Value};

const GRAPHQL: &str = "https://launcher.store.epicgames.com/graphql";
const PLAYTIME: &str = "https://library-service.live.use1a.on.epicgames.com/library/api/public/playtime/account";
const PAUSE: Duration = Duration::from_millis(250);

const DEFINITIONS: &str = "query Achievement($sandboxId: String!, $locale: String!) { Achievement { productAchievementsRecordBySandbox(sandboxId: $sandboxId, locale: $locale) { totalAchievements achievements { achievement { name hidden unlockedDisplayName lockedDisplayName unlockedDescription lockedDescription unlockedIconLink lockedIconLink XP rarity { percent } } } } } }";
const PLAYER: &str = "query PlayerAchievement($epicAccountId: String!, $sandboxId: String!) { PlayerAchievement { playerAchievementGameRecordsBySandbox(epicAccountId: $epicAccountId, sandboxId: $sandboxId) { records { totalUnlocked playerAchievements { playerAchievement { achievementName unlocked unlockDate } } } } } }";

async fn ask(http: &reqwest::Client, access: &str, query: &str, variables: Value) -> Result<Value, String> {
    let response = crate::net::send(
        http.post(GRAPHQL).bearer_auth(access).header("User-Agent", "EpicGamesLauncher/15.0.0").json(&json!({ "query": query, "variables": variables })),
    )
    .await
    .map_err(|e| e.to_string())?;
    tokio::time::sleep(PAUSE).await;
    match response.status().as_u16() {
        200..=299 => {}
        429 => return Err(tr!("A Epic pediu uma pausa nas consultas. O Cerberus mostra o que já tinha e tenta de novo mais tarde.").into()),
        _ => return Err(tr!("A Epic não entregou as conquistas.").into()),
    }
    let v: Value = response.json().await.map_err(|e| e.to_string())?;
    if let Some(errors) = v.get("errors").filter(|e| e.as_array().is_some_and(|a| !a.is_empty())) {
        let text = errors.to_string().to_lowercase();
        return Err(if text.contains("rate") || text.contains("too many") {
            tr!("A Epic pediu uma pausa nas consultas. O Cerberus mostra o que já tinha e tenta de novo mais tarde.").into()
        } else {
            tr!("A Epic não entregou as conquistas.").into()
        });
    }
    Ok(v)
}

/// Seconds played per game (by its app name) and when, in one question.
pub async fn playtimes(http: &reqwest::Client, access: &str, account: &str) -> Result<HashMap<String, (u64, u64)>, String> {
    let response = crate::net::send(http.get(format!("{PLAYTIME}/{account}/all")).bearer_auth(access)).await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(tr!("A Epic não entregou o tempo de jogo.").into());
    }
    let v: Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let last = r.get("lastModified").and_then(Value::as_str).and_then(seconds).unwrap_or(0);
            Some((r.get("artifactId")?.as_str()?.to_string(), (r.get("totalTime")?.as_u64()?, last)))
        })
        .collect())
}

/// Unix seconds from Epic's dates ("2024-05-01T20:11:45.123Z").
fn seconds(text: &str) -> Option<u64> {
    super::xbox::seconds(text)
}

/// A game's whole list with the player's unlocks (empty: the game has none).
pub async fn game_list(http: &reqwest::Client, access: &str, account: &str, namespace: &str) -> Result<Vec<Achievement>, String> {
    let locale = if cerberus_shared::i18n::lang() == cerberus_shared::i18n::Lang::Pt { "pt-BR" } else { "en-US" };
    let defs = ask(http, access, DEFINITIONS, json!({ "sandboxId": namespace, "locale": locale })).await?;
    let list = defs.pointer("/data/Achievement/productAchievementsRecordBySandbox/achievements").and_then(Value::as_array).cloned().unwrap_or_default();
    if list.is_empty() {
        return Ok(Vec::new());
    }
    let mine = ask(http, access, PLAYER, json!({ "sandboxId": namespace, "epicAccountId": account })).await?;
    Ok(read(&list, &mine))
}

fn read(list: &[Value], mine: &Value) -> Vec<Achievement> {
    let unlocked: HashMap<&str, u64> = mine
        .pointer("/data/PlayerAchievement/playerAchievementGameRecordsBySandbox/records/0/playerAchievements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| p.get("playerAchievement"))
        .filter(|p| p.get("unlocked").and_then(Value::as_bool) == Some(true))
        .filter_map(|p| Some((p.get("achievementName")?.as_str()?, p.get("unlockDate").and_then(Value::as_str).and_then(seconds).unwrap_or(1))))
        .collect();
    list.iter()
        .filter_map(|x| x.get("achievement"))
        .filter_map(|a| {
            let id = a.get("name")?.as_str()?.to_string();
            let at = unlocked.get(id.as_str()).copied();
            let text = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            let hidden = a.get("hidden").and_then(Value::as_bool).unwrap_or(false);
            let (name, description) = if at.is_some() || !hidden {
                (text("unlockedDisplayName"), if at.is_some() { text("unlockedDescription") } else { text("lockedDescription") })
            } else {
                (text("lockedDisplayName"), String::new())
            };
            Some(Achievement {
                id,
                name,
                description,
                icon: a.get("unlockedIconLink").and_then(Value::as_str).map(str::to_string),
                icon_locked: a.get("lockedIconLink").and_then(Value::as_str).map(str::to_string),
                hidden,
                unlocked_at: at,
                hardcore_at: None,
                points: a.get("XP").and_then(Value::as_u64).unwrap_or(0) as u32,
                rarity: a.pointer("/rarity/percent").and_then(Value::as_f64).map(|p| p as f32),
            })
        })
        .collect()
}

/// The hub's line for a game, from its list.
pub fn entry(namespace: &str, title: &str, cover: Option<&str>, played_secs: u64, list: &[Achievement]) -> AchievementGame {
    let unlocked = list.iter().filter(|a| a.unlocked_at.is_some()).count() as u32;
    AchievementGame {
        source: AchievementSource::Epic,
        key: namespace.to_string(),
        name: title.to_string(),
        art: cover.map(|c| vec![c.to_string()]).unwrap_or_default(),
        platform: "Epic Games".into(),
        total: list.len() as u32,
        unlocked,
        hardcore: 0,
        points: list.iter().filter(|a| a.unlocked_at.is_some()).map(|a| a.points).sum(),
        points_total: list.iter().map(|a| a.points).sum(),
        last_unlock: list.iter().filter_map(|a| a.unlocked_at).filter(|t| *t > 1).max(),
        playtime_secs: (played_secs > 0).then_some(played_secs),
        last_played: None,
        award: None,
        award_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_epic_lists_with_unlocks_and_secrets() {
        let list = vec![
            json!({ "achievement": { "name": "a", "unlockedDisplayName": "First", "unlockedDescription": "Did it", "lockedDescription": "Do it", "XP": 10, "rarity": { "percent": 40.5 } } }),
            json!({ "achievement": { "name": "b", "hidden": true, "lockedDisplayName": "Secret", "unlockedDisplayName": "Spoiler", "unlockedDescription": "Spoiler", "XP": 20 } }),
        ];
        let mine = json!({ "data": { "PlayerAchievement": { "playerAchievementGameRecordsBySandbox": { "records": [
            { "playerAchievements": [ { "playerAchievement": { "achievementName": "a", "unlocked": true, "unlockDate": "2024-05-01T20:11:45.000Z" } } ] }
        ] } } } });
        let got = read(&list, &mine);
        assert_eq!(got[0].unlocked_at, Some(1_714_594_305));
        assert_eq!((got[0].description.as_str(), got[0].points), ("Did it", 10));
        assert_eq!((got[1].name.as_str(), got[1].description.as_str()), ("Secret", ""), "a secret stays secret until unlocked");
        let g = entry("ns", "Game", None, 3600, &got);
        assert_eq!((g.unlocked, g.total, g.points, g.points_total), (1, 2, 10, 30));
    }
}
