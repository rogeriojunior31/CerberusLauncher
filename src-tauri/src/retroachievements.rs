//! RetroAchievements: the player links their account with their username and
//! their own Web API key (from retroachievements.org, Settings › Keys), and
//! Cerberus reads their profile and the games they played lately, with how
//! many achievements each has. Nothing is written to their account.
//!
//! Only retroachievements.org is called, with Cerberus's own User-Agent, as the
//! site asks of clients.

use cerberus_shared::tr;
use std::time::Duration;

use cerberus_shared::{RaGame, RaProfile};
use serde_json::Value;

const API: &str = "https://retroachievements.org/API";
const SITE: &str = "https://retroachievements.org";

fn user_agent() -> String {
    format!("CerberusLauncher/{}", env!("CARGO_PKG_VERSION"))
}

pub(crate) async fn get(http: &reqwest::Client, endpoint: &str, user: &str, key: &str, extra: &str) -> Result<Value, String> {
    let url = format!("{API}/{endpoint}?u={}&y={}{extra}", encode(user), encode(key));
    let reply = crate::net::send(http.get(&url).header("User-Agent", user_agent()).timeout(Duration::from_secs(15)))
        .await
        .map_err(|_| tr!("Não foi possível falar com o RetroAchievements. Confira a internet.").to_string())?;
    match reply.status().as_u16() {
        401 | 403 => return Err(tr!("O RetroAchievements não aceitou esse usuário e chave.").into()),
        404 => return Err(tr!("Esse usuário não existe no RetroAchievements.").into()),
        429 => return Err(tr!("O RetroAchievements pediu para esperar um pouco. Tente de novo em instantes.").into()),
        s if !(200..300).contains(&s) => return Err(tr!("O RetroAchievements respondeu {}.", s)),
        _ => {}
    }
    reply.json().await.map_err(|_| tr!("O RetroAchievements respondeu algo inesperado.").to_string())
}

/// Percent-encodes a query value.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

pub(crate) fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

pub(crate) fn number(v: &Value, key: &str) -> u64 {
    match v.get(key) {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

pub(crate) fn picture(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Some(if path.starts_with("http") { path.to_string() } else { format!("{SITE}{path}") })
}

/// The profile, which also proves the username and key go together.
pub async fn profile(http: &reqwest::Client, user: &str, key: &str) -> Result<RaProfile, String> {
    let v = get(http, "API_GetUserProfile.php", user, key, "").await?;
    let name = text(&v, "User");
    if name.is_empty() {
        return Err(tr!("O RetroAchievements não aceitou esse usuário e chave.").into());
    }
    let recent = get(http, "API_GetUserRecentlyPlayedGames.php", user, key, "&c=8").await.unwrap_or(Value::Null);
    let games = recent
        .as_array()
        .map(|list| {
            list.iter()
                .map(|g| RaGame {
                    id: number(g, "GameID"),
                    title: text(g, "Title"),
                    console: text(g, "ConsoleName"),
                    icon: picture(&text(g, "ImageIcon")),
                    achieved: number(g, "NumAchieved"),
                    total: number(g, "NumPossibleAchievements"),
                    last_played: text(g, "LastPlayed"),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(RaProfile {
        user: name.clone(),
        picture: picture(&text(&v, "UserPic")),
        points: number(&v, "TotalPoints"),
        softcore_points: number(&v, "TotalSoftcorePoints"),
        true_points: number(&v, "TotalTruePoints"),
        member_since: text(&v, "MemberSince"),
        playing_now: text(&v, "RichPresenceMsg"),
        url: format!("{SITE}/user/{}", encode(&name)),
        games,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_encoded_and_pictures_made_whole() {
        assert_eq!(encode("a b&c"), "a%20b%26c");
        assert_eq!(picture("/UserPic/Me.png").as_deref(), Some("https://retroachievements.org/UserPic/Me.png"));
        assert_eq!(picture(""), None);
        let v: Value = serde_json::json!({"TotalPoints": "120", "NumAchieved": 3});
        assert_eq!((number(&v, "TotalPoints"), number(&v, "NumAchieved")), (120, 3));
    }
}
