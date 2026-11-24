//! RetroAchievements through its official Web API, with the player's own
//! username and key (Settings › RetroAchievements):
//!
//! - `API_GetUserCompletionProgress`: every game they played, with how many
//!   achievements they have (normal and hardcore).
//! - `API_GetUserRecentAchievements`: their unlocks of the last 30 days.
//! - `API_GetGameInfoAndUserProgress`: one game's achievements with points,
//!   badges, unlock dates and how many players have each.

use cerberus_shared::{Achievement, AchievementGame, AchievementSource, GameAward, RecentUnlock};
use serde_json::Value;

use crate::retroachievements::{get, number, picture, text};

const MEDIA: &str = "https://media.retroachievements.org";

/// "2024-01-31 18:05:00" (UTC) or "2024-01-31T18:05:00+00:00" in Unix seconds.
pub fn unix(date: &str) -> Option<u64> {
    let date = date.trim();
    if date.len() < 19 {
        return None;
    }
    let num = |range: std::ops::Range<usize>| date.get(range)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // An offset like "+02:00" moves it back to UTC.
    let offset = match date.get(19..) {
        Some(rest) if rest.len() >= 6 && (rest.starts_with('+') || rest.starts_with('-')) => {
            let sign = if rest.starts_with('-') { -1 } else { 1 };
            let oh = rest.get(1..3)?.parse::<i64>().ok()?;
            let om = rest.get(4..6)?.parse::<i64>().ok()?;
            sign * (oh * 3600 + om * 60)
        }
        _ => 0,
    };
    let days = days_from_civil(y, mo, d);
    let secs = days * 86_400 + h * 3600 + mi * 60 + s - offset;
    (secs > 0).then_some(secs as u64)
}

/// Days since 1970-01-01 for a calendar date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn badge(name: &str, locked: bool) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some(if locked { format!("{MEDIA}/Badge/{name}_lock.png") } else { format!("{MEDIA}/Badge/{name}.png") })
}

fn media(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        None
    } else if path.starts_with("http") {
        Some(path.to_string())
    } else {
        Some(format!("{MEDIA}{path}"))
    }
}

/// The player's points, hardcore and softcore, for the hub's summary.
pub async fn points(http: &reqwest::Client, user: &str, key: &str) -> Result<(u64, u64), String> {
    let v = get(http, "API_GetUserProfile.php", user, key, "").await?;
    Ok((number(&v, "TotalPoints"), number(&v, "TotalSoftcorePoints")))
}

/// Every game the player has played on RetroAchievements.
pub async fn games(http: &reqwest::Client, user: &str, key: &str) -> Result<Vec<AchievementGame>, String> {
    let mut out = Vec::new();
    let mut offset = 0;
    // 500 per page; a cap keeps a huge account from looping for long.
    for _ in 0..10 {
        let page = get(http, "API_GetUserCompletionProgress.php", user, key, &format!("&c=500&o={offset}")).await?;
        let results = page.get("Results").and_then(Value::as_array).cloned().unwrap_or_default();
        out.extend(results.iter().map(game_from_progress));
        let total = number(&page, "Total");
        offset += results.len() as u64;
        if results.is_empty() || offset >= total {
            break;
        }
    }
    Ok(out)
}

fn game_from_progress(g: &Value) -> AchievementGame {
    AchievementGame {
        source: AchievementSource::Retro,
        key: number(g, "GameID").to_string(),
        name: text(g, "Title"),
        art: media(&text(g, "ImageIcon")).into_iter().collect(),
        platform: text(g, "ConsoleName"),
        total: number(g, "MaxPossible") as u32,
        unlocked: number(g, "NumAwarded") as u32,
        hardcore: number(g, "NumAwardedHardcore") as u32,
        points: 0,
        points_total: 0,
        last_unlock: unix(&text(g, "MostRecentAwardedDate")),
        playtime_secs: None,
        last_played: None,
        award: GameAward::from_code(&text(g, "HighestAwardKind")),
        award_at: unix(&text(g, "HighestAwardDate")),
    }
}

/// The player's unlocks of the last 30 days, newest first.
pub async fn recent(http: &reqwest::Client, user: &str, key: &str) -> Result<Vec<RecentUnlock>, String> {
    let list = get(http, "API_GetUserRecentAchievements.php", user, key, "&m=43200").await?;
    let mut out: Vec<RecentUnlock> = list.as_array().map(|l| l.iter().map(recent_from).collect()).unwrap_or_default();
    out.sort_by(|a, b| b.achievement.unlocked_at.cmp(&a.achievement.unlocked_at));
    Ok(out)
}

fn recent_from(a: &Value) -> RecentUnlock {
    let when = unix(&text(a, "Date"));
    let hardcore = number(a, "HardcoreMode") == 1;
    RecentUnlock {
        source: AchievementSource::Retro,
        game_key: number(a, "GameID").to_string(),
        game_name: text(a, "GameTitle"),
        achievement: Achievement {
            id: number(a, "AchievementID").to_string(),
            name: text(a, "Title"),
            description: text(a, "Description"),
            icon: badge(&text(a, "BadgeName"), false).or_else(|| media(&text(a, "BadgeURL"))),
            icon_locked: badge(&text(a, "BadgeName"), true),
            hidden: false,
            unlocked_at: when,
            hardcore_at: if hardcore { when } else { None },
            points: number(a, "Points") as u32,
            rarity: None,
        },
    }
}

/// One game's achievements with the player's progress.
pub async fn game(http: &reqwest::Client, user: &str, key: &str, id: &str) -> Result<(AchievementGame, Vec<Achievement>), String> {
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return Err(cerberus_shared::tr!("Jogo inválido.").into());
    }
    // a=1 adds the game's highest award (beaten, completed, mastered).
    let v = get(http, "API_GetGameInfoAndUserProgress.php", user, key, &format!("&g={id}&a=1")).await?;
    Ok(game_from_info(id, &v))
}

fn game_from_info(id: &str, v: &Value) -> (AchievementGame, Vec<Achievement>) {
    let players = number(v, "NumDistinctPlayers").max(number(v, "NumDistinctPlayersCasual")) as f32;
    let mut list: Vec<(u64, Achievement)> = v
        .get("Achievements")
        .and_then(Value::as_object)
        .map(|all| {
            all.values()
                .map(|a| {
                    let earned = unix(&text(a, "DateEarned"));
                    let hardcore = unix(&text(a, "DateEarnedHardcore"));
                    let awarded = number(a, "NumAwarded") as f32;
                    (
                        number(a, "DisplayOrder"),
                        Achievement {
                            id: number(a, "ID").to_string(),
                            name: text(a, "Title"),
                            description: text(a, "Description"),
                            icon: badge(&text(a, "BadgeName"), false),
                            icon_locked: badge(&text(a, "BadgeName"), true),
                            hidden: false,
                            // A hardcore unlock counts as an unlock too.
                            unlocked_at: earned.or(hardcore),
                            hardcore_at: hardcore,
                            points: number(a, "Points") as u32,
                            rarity: (players > 0.0).then(|| (awarded / players * 100.0).min(100.0)),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    list.sort_by_key(|(order, a)| (*order, a.id.parse::<u64>().unwrap_or(0)));
    let achievements: Vec<Achievement> = list.into_iter().map(|(_, a)| a).collect();
    let game = AchievementGame {
        source: AchievementSource::Retro,
        key: id.to_string(),
        name: text(v, "Title"),
        art: media(&text(v, "ImageIcon")).or_else(|| picture(&text(v, "ImageIcon"))).into_iter().collect(),
        platform: text(v, "ConsoleName"),
        total: achievements.len() as u32,
        unlocked: achievements.iter().filter(|a| a.unlocked_at.is_some()).count() as u32,
        hardcore: achievements.iter().filter(|a| a.hardcore_at.is_some()).count() as u32,
        points: achievements.iter().filter(|a| a.unlocked_at.is_some()).map(|a| a.points).sum(),
        points_total: achievements.iter().map(|a| a.points).sum(),
        last_unlock: achievements.iter().filter_map(|a| a.unlocked_at).max(),
        playtime_secs: None,
        last_played: None,
        award: GameAward::from_code(&text(v, "HighestAwardKind")),
        award_at: unix(&text(v, "HighestAwardDate")),
    };
    (game, achievements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_in_both_formats() {
        assert_eq!(unix("1970-01-02 00:00:00"), Some(86_400));
        assert_eq!(unix("2024-01-31 18:05:00"), Some(1_706_724_300));
        assert_eq!(unix("2024-01-31T20:05:00+02:00"), Some(1_706_724_300));
        assert_eq!(unix(""), None);
        assert_eq!(unix("ontem"), None);
    }

    #[test]
    fn a_game_page_with_progress() {
        let v = serde_json::json!({
            "Title": "Super Mario 64", "ConsoleName": "Nintendo 64", "ImageIcon": "/Images/1.png",
            "NumDistinctPlayers": 200,
            "Achievements": {
                "2": {"ID": 2, "Title": "Second", "Description": "b", "Points": 10, "BadgeName": "222", "DisplayOrder": 2, "NumAwarded": 20},
                "1": {"ID": 1, "Title": "First", "Description": "a", "Points": 5, "BadgeName": "111", "DisplayOrder": 1, "NumAwarded": 150,
                      "DateEarned": "2024-01-31 18:05:00", "DateEarnedHardcore": "2024-01-31 18:05:00"}
            }
        });
        let (game, list) = game_from_info("10", &v);
        assert_eq!(list[0].name, "First");
        assert_eq!(list[0].unlocked_at, Some(1_706_724_300));
        assert!(list[0].hardcore_at.is_some());
        assert_eq!(list[1].unlocked_at, None);
        assert_eq!(list[1].rarity, Some(10.0));
        assert_eq!(list[0].icon.as_deref(), Some("https://media.retroachievements.org/Badge/111.png"));
        assert_eq!(list[1].icon_locked.as_deref(), Some("https://media.retroachievements.org/Badge/222_lock.png"));
        assert_eq!((game.total, game.unlocked, game.hardcore, game.points, game.points_total), (2, 1, 1, 5, 15));
        assert_eq!(game.art, vec!["https://media.retroachievements.org/Images/1.png".to_string()]);
    }

    #[test]
    fn completion_and_recent_entries() {
        let g = game_from_progress(&serde_json::json!({"GameID": 7, "Title": "Sonic", "ConsoleName": "Mega Drive", "MaxPossible": 20, "NumAwarded": 5, "NumAwardedHardcore": 3, "MostRecentAwardedDate": "2024-01-31T18:05:00+00:00", "HighestAwardKind": "beaten-hardcore", "HighestAwardDate": "2024-01-31T18:05:00+00:00"}));
        assert_eq!(g.award, Some(GameAward::BeatenHardcore));
        assert!(g.award.unwrap().hardcore() && !g.award.unwrap().full());
        assert_eq!(g.award_at, Some(1_706_724_300));
        assert_eq!(game_from_progress(&serde_json::json!({"HighestAwardKind": null})).award, None);
        assert_eq!((g.key.as_str(), g.total, g.unlocked, g.hardcore, g.percent()), ("7", 20, 5, 3, 25));
        assert_eq!(g.last_unlock, Some(1_706_724_300));
        let r = recent_from(&serde_json::json!({"Date": "2024-01-31 18:05:00", "HardcoreMode": 1, "AchievementID": 9, "Title": "Ring", "Description": "x", "BadgeName": "55", "Points": 3, "GameTitle": "Sonic", "GameID": 7}));
        assert_eq!((r.game_key.as_str(), r.achievement.points), ("7", 3));
        assert!(r.achievement.hardcore_at.is_some());
    }
}
